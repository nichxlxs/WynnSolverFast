//! Enumeration-kernel replay (P2.3 prototype).
//!
//! Replays a solver scenario exported by test_solver_search.js
//! (SOLVER_EXPORT_RUST=<path>): level-based enumeration over free slots with
//! ring canonicalization, illegal-set blocking, mid-tree SP feasibility
//! pruning, restriction/EHP suffix-bound pruning, leaf prechecks, and the
//! exact SP kernel at surviving leaves. Reports the same funnel counters as
//! the JS worker (checked / precheck_reject / feasible) and wall time.
//!
//! Scoring (greedy SP, mana sim, damage) is intentionally absent: feasible
//! leaves are counted, not scored, so compare against the JS run's funnel
//! and treat the time as the enumeration+SP engine cost.
//!
//! Usage: enum_kernel <fixture.txt> [threads]
//!
//! Threading: worker threads claim first-slot offsets from an atomic queue
//! and run the full band sweep restricted to that offset (the same 'slot'
//! partition shape the JS engine uses). Every counter is integral, so the
//! per-thread sums combine exactly regardless of scheduling order.

use sp_kernel::{Case, Kernel, Unit, SP_PER_ATTR_CAP};
use std::env;
use std::fs;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use web_time::Instant;

#[derive(Clone)]
struct PoolItem {
    crafted: bool,
    reqs: [i32; 5],
    skp: [i32; 5],
    set_id: i32,
    illegal_id: i32,
    hp: f64,
    pc: Vec<f64>,
}

struct Slot {
    #[allow(dead_code)]
    name: String,
    pos: usize,
    is_ring1: bool,
    is_ring2: bool,
    pool: Vec<PoolItem>,
    /// Item display names (from the optional NAMES section), for joining to
    /// a score fixture's item registry. Empty when the section is absent.
    item_names: Vec<String>,
}

struct Fixture {
    budget: i32,
    pc_thresholds: Vec<f64>,
    pc_start: Vec<f64>,
    ehp: Option<(f64, f64, f64)>, // threshold, fixed_hp, divisor
    ehpna: Option<(f64, f64, f64)>,
    thp: Option<(f64, f64)>, // threshold, fixed_hp
    hp_start: f64,
    weapon: Unit,
    guild: Option<(Unit, i32)>,          // unit, set_id
    fixed: Vec<(usize, Unit, i32, i32)>, // pos, unit, set_id, illegal_id
    slots: Vec<Slot>,
    set_table: Vec<Vec<[i32; 5]>>, // set_id -> bonuses per count (count-1 indexed)
    fixed_names: Vec<(usize, String)>, // pos, display name (NAMES section)
    none_names: Vec<String>,       // 8 none-item names by slot position
}

fn parse_fixture(text: &str) -> Fixture {
    let mut lines = text.lines();
    let mut next = || lines.next().expect("truncated fixture");
    let toks = |l: &str| {
        l.split_ascii_whitespace()
            .map(String::from)
            .collect::<Vec<_>>()
    };

    let budget: i32 = toks(next())[1].parse().unwrap();
    let n_pc: usize = toks(next())[1].parse().unwrap();
    let mut pc_thresholds = Vec::new();
    let mut pc_start = Vec::new();
    for _ in 0..n_pc {
        let t = toks(next());
        pc_thresholds.push(t[2].parse().unwrap());
        pc_start.push(t[3].parse().unwrap());
    }
    let parse_gate3 = |t: &[String]| -> Option<(f64, f64, f64)> {
        if t[1] == "1" {
            Some((
                t[2].parse().unwrap(),
                t[3].parse().unwrap(),
                t[4].parse().unwrap(),
            ))
        } else {
            None
        }
    };
    let ehp = parse_gate3(&toks(next()));
    let ehpna = parse_gate3(&toks(next()));
    let thp_t = toks(next());
    let thp = if thp_t[1] == "1" {
        Some((thp_t[2].parse().unwrap(), thp_t[3].parse().unwrap()))
    } else {
        None
    };
    let hp_start: f64 = toks(next())[1].parse().unwrap();

    let unit_from = |t: &[String], off: usize| -> Unit {
        let mut reqs = [0i32; 5];
        let mut skp = [0i32; 5];
        for j in 0..5 {
            reqs[j] = t[off + j].parse().unwrap();
        }
        for j in 0..5 {
            skp[j] = t[off + 5 + j].parse().unwrap();
        }
        Unit {
            crafted: false,
            reqs,
            skp,
        }
    };

    let wt = toks(next());
    let weapon = unit_from(&wt, 1);

    let gt = toks(next());
    let guild = if gt[1] == "1" {
        let mut u = unit_from(&gt, 2);
        u.crafted = gt[2] == "1";
        // fields: GUILD present crafted reqs5 skp5 set_id → set at index 13
        let set_id: i32 = gt[13].parse().unwrap();
        Some((u, set_id))
    } else {
        None
    };

    let n_fixed: usize = toks(next())[1].parse().unwrap();
    let mut fixed = Vec::new();
    for _ in 0..n_fixed {
        let t = toks(next());
        // FIXED pos crafted reqs5 skp5 set_id illegal_id
        let pos: usize = t[1].parse().unwrap();
        let mut u = unit_from(&t, 3);
        u.crafted = t[2] == "1";
        let set_id: i32 = t[13].parse().unwrap();
        let illegal_id: i32 = t[14].parse().unwrap();
        fixed.push((pos, u, set_id, illegal_id));
    }

    let n_slots: usize = toks(next())[1].parse().unwrap();
    let mut slots = Vec::new();
    for _ in 0..n_slots {
        let t = toks(next());
        // SLOT name pos is_ring1 is_ring2 npool
        let name = t[1].clone();
        let pos: usize = t[2].parse().unwrap();
        let is_ring1 = t[3] == "1";
        let is_ring2 = t[4] == "1";
        let npool: usize = t[5].parse().unwrap();
        let mut pool = Vec::with_capacity(npool);
        for _ in 0..npool {
            let it = toks(next());
            // ITEM crafted reqs5 skp5 set_id illegal_id hp pc...
            let mut u = unit_from(&it, 2);
            u.crafted = it[1] == "1";
            let set_id: i32 = it[12].parse().unwrap();
            let illegal_id: i32 = it[13].parse().unwrap();
            let hp: f64 = it[14].parse().unwrap();
            let mut pc = Vec::with_capacity(n_pc);
            for k in 0..n_pc {
                pc.push(it[15 + k].parse().unwrap());
            }
            pool.push(PoolItem {
                crafted: u.crafted,
                reqs: u.reqs,
                skp: u.skp,
                set_id,
                illegal_id,
                hp,
                pc,
            });
        }
        slots.push(Slot {
            name,
            pos,
            is_ring1,
            is_ring2,
            pool,
            item_names: Vec::new(),
        });
    }

    let n_sets: usize = toks(next())[1].parse().unwrap();
    let mut set_table: Vec<Vec<[i32; 5]>> = vec![Vec::new(); n_sets];
    for _ in 0..n_sets {
        let t = toks(next());
        // SET id ncounts (skp5)*ncounts
        let id: usize = t[1].parse().unwrap();
        let ncounts: usize = t[2].parse().unwrap();
        let mut rows = Vec::with_capacity(ncounts);
        for c in 0..ncounts {
            let mut row = [0i32; 5];
            for j in 0..5 {
                row[j] = t[3 + c * 5 + j].parse().unwrap();
            }
            rows.push(row);
        }
        set_table[id] = rows;
    }

    // Optional NAMES section (item display names for score-fixture joining).
    let mut fixed_names = Vec::new();
    let mut none_names = Vec::new();
    loop {
        let Some(line) = lines.next() else { break };
        let t = toks(line);
        if t.is_empty() {
            continue;
        }
        match t[0].as_str() {
            "NAMES" => {}
            "INAMES" => {
                let si: usize = t[1].parse().unwrap();
                let n: usize = t[2].parse().unwrap();
                let mut names = Vec::with_capacity(n);
                for _ in 0..n {
                    names.push(lines.next().expect("truncated INAMES").trim().to_string());
                }
                slots[si].item_names = names;
            }
            "FNAMES" => {
                let n: usize = t[1].parse().unwrap();
                for _ in 0..n {
                    let line = lines.next().expect("truncated FNAMES");
                    let (pos_s, name) = line.split_once(' ').expect("FNAMES line");
                    fixed_names.push((pos_s.parse().unwrap(), name.trim().to_string()));
                }
            }
            "NONENAMES" => {
                let n: usize = t[1].parse().unwrap();
                for _ in 0..n {
                    none_names.push(
                        lines
                            .next()
                            .expect("truncated NONENAMES")
                            .trim()
                            .to_string(),
                    );
                }
            }
            _ => {}
        }
    }

    Fixture {
        budget,
        pc_thresholds,
        pc_start,
        ehp,
        ehpna,
        thp,
        hp_start,
        weapon,
        guild,
        fixed,
        slots,
        set_table,
        fixed_names,
        none_names,
    }
}

struct Search<'a> {
    fx: &'a Fixture,
    n_free: usize,
    l_max: usize,
    ring1_depth: isize,
    ring2_depth: isize,
    rings_contiguous: bool,

    // Suffix bounds
    sp_suffix_max_prov: Vec<[i32; 5]>, // [n_free+1][5]
    pc_suffix: Vec<f64>,               // [(n_free+1) * n_pc]
    hp_suffix: Vec<f64>,               // [n_free+1]

    // Subtree leaf counts
    subtree: Vec<Vec<f64>>,        // [n_free+1][l_max+1]
    subtree_prefix: Vec<Vec<f64>>, // [n_free+1][l_max+2]
    suffix_max_rank: Vec<i64>,     // [n_free+1]
    ring_pair_count: Vec<f64>,

    // Running state
    pc_running: Vec<f64>,
    hp_running: f64,
    sp_fixed_max_req: [i32; 5],
    sp_fixed_prov: [i32; 5],
    sp_free_prov: [i32; 5],
    sp_max_req: [i32; 5],
    sp_max_save: Vec<[i32; 5]>,
    set_counts: Vec<i32>,
    illegal_counts: Vec<i32>,
    equips: [Unit; 8],
    equip_set: [i32; 8],
    ring1_placed_offset: usize,

    // Funnel
    checked: f64,
    precheck_reject: f64,
    precheck_pass: u64,
    feasible: u64,
    sp_leaf_reject: u64,
    kernel: Kernel,

    // Progress reporting
    total_space: f64,
    started: Instant,
    next_report: f64,
    report_every: f64,
    report_calls: u64,

    // Multithreading: first-slot offset bounds (inclusive) and the shared
    // progress counter this thread flushes its local `checked` delta into.
    part_lo: i64,
    part_hi: i64,
    shared_checked: Option<&'a AtomicU64>,
    stop_flag: Option<&'a AtomicU64>,
    stop: bool,
    dense_work: sp_kernel::scoring::DenseWork,
    /// Separate buffers for bound evals so the leaf pipeline can't clobber
    /// the cached last-slot prefix state.
    bound_work: sp_kernel::scoring::DenseWork,
    checked_flushed: f64,

    // Scoring integration (P2.4 layer 3): current equip names by position,
    // the scenario scoring context, per-thread top-N, and the shared cutoff
    // (floor(score) as u64; 0 = unset — floor is admissible for the gate).
    equip_names: [&'a str; 8],
    scoring: Option<&'a sp_kernel::scoring::ScoringCtx>,
    top_n: Vec<(f64, Vec<String>)>,
    shared_cutoff: Option<&'a AtomicU64>,
    scored: u64,
    gated: u64,
    mana_reject: u64,
    thresh_reject: u64,
    cluster_evals: u64,
    cluster_memo_hits: u64,

    // Mid-tree damage ceiling bound (objective branch-and-bound).
    bound_tables: Option<&'a sp_kernel::scoring::BoundTables>,
    bound_max_depth: usize,
    /// Apply the per-offset bound when the REMAINING depth (slots after the
    /// candidate) is <= bound_tail — near the leaves the suffix maxima cover
    /// one or two pools and the ceiling is nearly as tight as the leaf gate,
    /// so one eval prunes a whole last-pool subtree.
    bound_tail: usize,
    dense_bound: Option<&'a sp_kernel::scoring::DenseBound>,
    bound_pruned: f64,
    /// Ceiling memo keyed by packed prefix offsets (the ceiling depends on
    /// the prefix, not the band, and prefixes recur across band sweeps).
    bound_memo: std::collections::HashMap<u64, f64>,
    /// Current prefix offsets (by depth) for memo keys.
    prefix_offsets: [u8; 8],
}

impl<'a> Search<'a> {
    fn new(fx: &'a Fixture) -> Self {
        let n_free = fx.slots.len();
        let n_pc = fx.pc_thresholds.len();
        let mut ring1_depth = -1isize;
        let mut ring2_depth = -1isize;
        for (d, s) in fx.slots.iter().enumerate() {
            if s.is_ring1 {
                ring1_depth = d as isize;
            }
            if s.is_ring2 {
                ring2_depth = d as isize;
            }
        }
        let both = ring1_depth >= 0 && ring2_depth >= 0;
        let rings_contiguous = both && ring2_depth == ring1_depth + 1;

        let l_max: usize = fx
            .slots
            .iter()
            .map(|s| s.pool.len().saturating_sub(1))
            .filter(|&ub| ub > 0)
            .sum();

        // SP max provisions per slot + suffix
        let mut sp_suffix_max_prov = vec![[0i32; 5]; n_free + 1];
        for d in (0..n_free).rev() {
            let mut maxp = [0i32; 5];
            for it in &fx.slots[d].pool {
                for j in 0..5 {
                    if it.skp[j] > maxp[j] {
                        maxp[j] = it.skp[j];
                    }
                }
            }
            for j in 0..5 {
                sp_suffix_max_prov[d][j] = sp_suffix_max_prov[d + 1][j] + maxp[j];
            }
        }

        // Restriction suffix bounds
        let mut pc_suffix = vec![0f64; (n_free + 1) * n_pc];
        for d in (0..n_free).rev() {
            for i in 0..n_pc {
                let mut slot_max = f64::NEG_INFINITY;
                if fx.slots[d].pool.is_empty() {
                    slot_max = 0.0;
                }
                for it in &fx.slots[d].pool {
                    if it.pc[i] > slot_max {
                        slot_max = it.pc[i];
                    }
                }
                pc_suffix[d * n_pc + i] = pc_suffix[(d + 1) * n_pc + i] + slot_max;
            }
        }
        let mut hp_suffix = vec![0f64; n_free + 1];
        for d in (0..n_free).rev() {
            let mut slot_max = f64::NEG_INFINITY;
            if fx.slots[d].pool.is_empty() {
                slot_max = 0.0;
            }
            for it in &fx.slots[d].pool {
                if it.hp > slot_max {
                    slot_max = it.hp;
                }
            }
            hp_suffix[d] = hp_suffix[d + 1] + slot_max;
        }

        // Ring pair count (contiguous case)
        let mut ring_pair_count = vec![0f64; l_max + 1];
        if rings_contiguous {
            let n = fx.slots[ring1_depth as usize].pool.len();
            for a in 0..n {
                for b in a..n {
                    if a + b <= l_max {
                        ring_pair_count[a + b] += 1.0;
                    }
                }
            }
        }

        // Subtree leaf counts
        let mut subtree = vec![vec![0f64; l_max + 1]; n_free + 1];
        subtree[n_free][0] = 1.0;
        for d in (0..n_free).rev() {
            if rings_contiguous && d as isize == ring2_depth {
                continue; // rebuilt per ring1 placement
            }
            if rings_contiguous && d as isize == ring1_depth {
                let tail = subtree[d + 2].clone();
                for lp in 0..=l_max {
                    let c = ring_pair_count[lp];
                    if c == 0.0 {
                        continue;
                    }
                    for lt in 0..=(l_max - lp) {
                        let t = tail[lt];
                        if t != 0.0 {
                            subtree[d][lp + lt] += c * t;
                        }
                    }
                }
                continue;
            }
            let ub = fx.slots[d].pool.len().saturating_sub(1);
            if fx.slots[d].pool.is_empty() {
                continue;
            }
            let tail = subtree[d + 1].clone();
            let mut prefix = vec![0f64; l_max + 2];
            for l in 0..=l_max {
                prefix[l + 1] = prefix[l] + tail[l];
            }
            for l in 0..=l_max {
                let lo = l.saturating_sub(ub);
                let hi = l.min(l_max);
                if hi + 1 > lo {
                    subtree[d][l] = prefix[hi + 1] - prefix[lo];
                }
            }
        }

        // Prefix sums of subtree rows (band credits) + max rank suffixes.
        let mut subtree_prefix = vec![vec![0f64; l_max + 2]; n_free + 1];
        for d in 0..=n_free {
            for t in 0..=l_max {
                subtree_prefix[d][t + 1] = subtree_prefix[d][t] + subtree[d][t];
            }
        }
        let mut suffix_max_rank = vec![0i64; n_free + 1];
        for d in (0..n_free).rev() {
            let ub = fx.slots[d].pool.len().saturating_sub(1) as i64;
            suffix_max_rank[d] = suffix_max_rank[d + 1] + ub.max(0);
        }

        // Fixed SP baseline
        let mut sp_fixed_max_req = [0i32; 5];
        let mut sp_fixed_prov = [0i32; 5];
        let mut set_counts = vec![0i32; fx.set_table.len()];
        let mut illegal_counts = vec![0i32; 64];
        let mut equips: [Unit; 8] = Default::default();
        let mut equip_set = [-1i32; 8];
        for (pos, u, set_id, illegal_id) in &fx.fixed {
            equips[*pos] = *u;
            equip_set[*pos] = *set_id;
            if !u.crafted {
                for j in 0..5 {
                    if u.skp[j] > 0 {
                        sp_fixed_prov[j] += u.skp[j];
                    }
                }
            }
            for j in 0..5 {
                if u.reqs[j] > sp_fixed_max_req[j] {
                    sp_fixed_max_req[j] = u.reqs[j];
                }
            }
            if *set_id >= 0 && !u.crafted {
                set_counts[*set_id as usize] += 1;
            }
            if *illegal_id >= 0 {
                illegal_counts[*illegal_id as usize] += 1;
            }
        }
        if let Some((g, gset)) = &fx.guild {
            for j in 0..5 {
                if g.skp[j] > 0 {
                    sp_fixed_prov[j] += g.skp[j];
                }
            }
            for j in 0..5 {
                if g.reqs[j] > sp_fixed_max_req[j] {
                    sp_fixed_max_req[j] = g.reqs[j];
                }
            }
            if *gset >= 0 && !g.crafted {
                set_counts[*gset as usize] += 1;
            }
        }
        for j in 0..5 {
            if fx.weapon.reqs[j] > sp_fixed_max_req[j] {
                sp_fixed_max_req[j] = fx.weapon.reqs[j];
            }
        }

        Search {
            fx,
            n_free,
            l_max,
            ring1_depth,
            ring2_depth,
            rings_contiguous,
            sp_suffix_max_prov,
            pc_suffix,
            hp_suffix,
            subtree,
            subtree_prefix,
            suffix_max_rank,
            ring_pair_count,
            pc_running: fx.pc_start.clone(),
            hp_running: fx.hp_start,
            sp_fixed_max_req,
            sp_fixed_prov,
            sp_free_prov: [0; 5],
            sp_max_req: sp_fixed_max_req,
            sp_max_save: vec![[0i32; 5]; n_free],
            set_counts,
            illegal_counts,
            equips,
            equip_set,
            ring1_placed_offset: 0,
            checked: 0.0,
            precheck_reject: 0.0,
            precheck_pass: 0,
            feasible: 0,
            sp_leaf_reject: 0,
            kernel: Kernel::new(),
            total_space: 0.0,
            started: Instant::now(),
            next_report: 0.0,
            report_every: 0.0,
            report_calls: 0,
            part_lo: 0,
            part_hi: i64::MAX,
            shared_checked: None,
            stop_flag: None,
            stop: false,
            dense_work: Default::default(),
            thresh_reject: 0,
            cluster_evals: 0,
            cluster_memo_hits: 0,
            bound_work: Default::default(),
            checked_flushed: 0.0,
            equip_names: Default::default(),
            scoring: None,
            top_n: Vec::new(),
            shared_cutoff: None,
            scored: 0,
            gated: 0,
            mana_reject: 0,
            bound_tables: None,
            bound_max_depth: 2,
            bound_tail: 0,
            dense_bound: None,
            bound_pruned: 0.0,
            bound_memo: std::collections::HashMap::new(),
            prefix_offsets: [0; 8],
        }
    }

    /// Current gate/bound cutoff: local 15th-best exact score or the shared
    /// floored cutoff, whichever is higher. None until either exists.
    fn cutoff(&self) -> Option<f64> {
        let mut cutoff: Option<f64> = None;
        if self.top_n.len() >= 15 {
            cutoff = Some(self.top_n[14].0);
        }
        if let Some(shared) = self.shared_cutoff {
            let s = shared.load(Ordering::Relaxed);
            if s > 0 && (s as f64) > cutoff.unwrap_or(f64::NEG_INFINITY) {
                cutoff = Some(s as f64);
            }
        }
        cutoff
    }

    /// Subtree ceiling for placing pool item `offset` at `depth`, memoized by
    /// the packed prefix. Returns true when the subtree CANNOT beat `cutoff`.
    fn bound_prunes(&mut self, depth: usize, offset: usize, cutoff: f64, hi_rem: i64) -> bool {
        let (Some(sc), Some(bt)) = (self.scoring, self.bound_tables) else {
            return false;
        };
        let h_child = hi_rem - offset as i64;
        let mut key = (depth as u64) << 60;
        key |= (h_child.clamp(0, 2047) as u64) & 0x7FF;
        for d in 0..depth {
            key |= (self.prefix_offsets[d] as u64) << (11 + d * 7);
        }
        key |= (offset as u64) << (11 + depth * 7);
        let ceiling = match self.bound_memo.get(&key) {
            Some(&c) => c,
            None => {
                let slot = &self.fx.slots[depth];
                let mut names = self.equip_names;
                names[slot.pos] = &slot.item_names[offset];
                let dense_c = match (sc.dense.as_ref(), self.dense_bound) {
                    (Some(d), Some(db)) => sp_kernel::scoring::dense_subtree_ceiling(
                        d,
                        db,
                        depth + 1,
                        h_child,
                        &names,
                        &mut self.bound_work,
                        &sc.rows,
                        &sc.compiled_rows,
                        &sc.tables,
                    ),
                    _ => None,
                };
                let c = match dense_c {
                    Some(c) => c,
                    None => sc
                        .layer2
                        .subtree_ceiling(
                            &names,
                            bt,
                            depth + 1,
                            &sc.weapon,
                            &sc.rows,
                            &sc.registry,
                            &sc.hit_refs,
                            &sc.tables,
                            &sc.objective,
                            Some(&sc.compiled_rows),
                        )
                        .expect("bound eval error"),
                };
                self.bound_memo.insert(key, c);
                c
            }
        };
        if std::env::var("BOUND_DEBUG").as_deref() == Ok("1") {
            use std::sync::atomic::AtomicU64 as A;
            static N: A = A::new(0);
            if N.fetch_add(1, Ordering::Relaxed) < 30 {
                eprintln!(
                    "bound_debug: depth {} ceiling {:.4e} cutoff {:.4e}",
                    depth, ceiling, cutoff
                );
            }
        }
        ceiling < cutoff - cutoff.abs() * 1e-9
    }

    /// Initialize equip names: none-item names per position, overridden by
    /// fixed items. Free-slot names are set/cleared by place()/unplace().
    fn init_equip_names(&mut self) {
        if self.fx.none_names.len() == 8 {
            for p in 0..8 {
                self.equip_names[p] = &self.fx.none_names[p];
            }
        }
        for (pos, name) in &self.fx.fixed_names {
            self.equip_names[*pos] = name;
        }
    }

    /// Progress line with rate + ETA, every ~5 seconds (time check amortized
    /// over 65k credit/leaf events so Instant::now() stays off the hot path).
    fn maybe_report(&mut self) {
        self.report_calls += 1;
        if self.report_calls & 0xFFFF != 0 {
            return;
        }
        if let Some(f) = self.stop_flag {
            if f.load(Ordering::Relaxed) != 0 {
                self.stop = true;
            }
        }
        if let Some(shared) = self.shared_checked {
            // Threaded mode: flush the local delta; the monitor thread prints.
            let delta = self.checked - self.checked_flushed;
            if delta > 0.0 {
                shared.fetch_add(delta as u64, Ordering::Relaxed);
                self.checked_flushed = self.checked;
            }
            return;
        }
        let elapsed = self.started.elapsed().as_secs_f64();
        if elapsed < self.next_report {
            return;
        }
        self.next_report = elapsed + 5.0;
        let rate = self.checked / elapsed;
        let remaining = (self.total_space - self.checked).max(0.0);
        eprintln!(
            "progress: {:.2}% | checked {:.3e}/{:.3e} | {:.2e} checked/s | elapsed {:.0}s | eta {:.0}s",
            self.checked / self.total_space * 100.0,
            self.checked, self.total_space, rate, elapsed, remaining / rate,
        );
    }

    /// Final flush of the local `checked` delta into the shared counter.
    fn flush_checked(&mut self) {
        if let Some(shared) = self.shared_checked {
            let delta = self.checked - self.checked_flushed;
            if delta > 0.0 {
                shared.fetch_add(delta as u64, Ordering::Relaxed);
                self.checked_flushed = self.checked;
            }
        }
    }

    fn rebuild_ring2_subtree(&mut self, ring1_offset: usize) {
        if !self.rings_contiguous {
            return;
        }
        let d = self.ring2_depth as usize;
        let ub = self.fx.slots[d].pool.len() - 1;
        let lb = ring1_offset;
        let tail = self.subtree[d + 1].clone();
        let l_max = self.l_max;
        let mut prefix = vec![0f64; l_max + 2];
        for l in 0..=l_max {
            prefix[l + 1] = prefix[l] + tail[l];
        }
        {
            let row = &mut self.subtree[d];
            for l in 0..=l_max {
                row[l] = 0.0;
            }
            if lb <= ub {
                for l in 0..=l_max {
                    let lo = l.saturating_sub(ub);
                    if l < lb {
                        continue;
                    }
                    let hi_incl = l - lb;
                    if hi_incl < lo {
                        continue;
                    }
                    let hi = hi_incl.min(l_max);
                    row[l] = prefix[hi + 1] - prefix[lo];
                }
            }
        }
        for t in 0..=l_max {
            self.subtree_prefix[d][t + 1] = self.subtree_prefix[d][t] + self.subtree[d][t];
        }
    }

    /// Leaves below depth d with remaining rank sum in [lo, hi].
    fn band_credit(&self, d: usize, lo: i64, hi: i64) -> f64 {
        if hi < 0 {
            return 0.0;
        }
        let lo_c = lo.max(0) as usize;
        let hi_c = (hi as usize).min(self.l_max);
        if lo_c > hi_c {
            return 0.0;
        }
        let p = &self.subtree_prefix[d];
        p[hi_c + 1] - p[lo_c]
    }

    /// SP bound for placing pool[offset] at `depth` — identical outcome to
    /// placing and running sp_mid_tree_feasible / sp_leaf_feasible.
    fn sp_bound_ok(&self, depth: usize, offset: usize, is_leaf: bool) -> bool {
        let it = &self.fx.slots[depth].pool[offset];
        let mut total_deficit = 0i32;
        for j in 0..5 {
            let m = it.reqs[j].max(self.sp_max_req[j]);
            if m == 0 {
                continue;
            }
            let item_prov = if !it.crafted && it.skp[j] > 0 {
                it.skp[j]
            } else {
                0
            };
            let sfx = if is_leaf {
                0
            } else {
                self.sp_suffix_max_prov[depth + 1][j]
            };
            let prov = self.sp_fixed_prov[j] + self.sp_free_prov[j] + item_prov + sfx;
            if m <= prov {
                continue;
            }
            let deficit = m - prov;
            if deficit > SP_PER_ATTR_CAP {
                return false;
            }
            total_deficit += deficit;
            if total_deficit > self.fx.budget {
                return false;
            }
        }
        true
    }

    /// Restriction/EHP bound for placing pool[offset] at `depth`.
    fn restr_bound_ok(&self, depth: usize, offset: usize, is_leaf: bool) -> bool {
        let it = &self.fx.slots[depth].pool[offset];
        let n_pc = self.fx.pc_thresholds.len();
        for i in 0..n_pc {
            let sfx = if is_leaf {
                0.0
            } else {
                self.pc_suffix[(depth + 1) * n_pc + i]
            };
            if self.pc_running[i] + it.pc[i] + sfx < self.fx.pc_thresholds[i] {
                return false;
            }
        }
        if self.fx.ehp.is_some() || self.fx.ehpna.is_some() || self.fx.thp.is_some() {
            let sfx = if is_leaf {
                0.0
            } else {
                self.hp_suffix[depth + 1]
            };
            if !self.hp_gates_ok(self.hp_running + it.hp + sfx) {
                return false;
            }
        }
        true
    }

    fn sp_mid_tree_feasible(&self, next_depth: usize) -> bool {
        if next_depth >= self.n_free {
            return true;
        }
        let mut total_deficit = 0i32;
        for j in 0..5 {
            if self.sp_max_req[j] == 0 {
                continue;
            }
            let prov = self.sp_fixed_prov[j]
                + self.sp_free_prov[j]
                + self.sp_suffix_max_prov[next_depth][j];
            if self.sp_max_req[j] <= prov {
                continue;
            }
            let deficit = self.sp_max_req[j] - prov;
            if deficit > SP_PER_ATTR_CAP {
                return false;
            }
            total_deficit += deficit;
            if total_deficit > self.fx.budget {
                return false;
            }
        }
        true
    }

    fn sp_leaf_feasible(&self) -> bool {
        let mut total_deficit = 0i32;
        for j in 0..5 {
            if self.sp_max_req[j] == 0 {
                continue;
            }
            let prov = self.sp_fixed_prov[j] + self.sp_free_prov[j];
            if self.sp_max_req[j] <= prov {
                continue;
            }
            let deficit = self.sp_max_req[j] - prov;
            if deficit > SP_PER_ATTR_CAP {
                return false;
            }
            total_deficit += deficit;
            if total_deficit > self.fx.budget {
                return false;
            }
        }
        true
    }

    fn hp_gates_ok(&self, raw_hp: f64) -> bool {
        if let Some((thr, fixed_hp, div)) = self.fx.ehp {
            let mut total = raw_hp + fixed_hp;
            if total < 5.0 {
                total = 5.0;
            }
            if total / div < thr {
                return false;
            }
        }
        if let Some((thr, fixed_hp, div)) = self.fx.ehpna {
            let mut total = raw_hp + fixed_hp;
            if total < 5.0 {
                total = 5.0;
            }
            if total / div < thr {
                return false;
            }
        }
        if let Some((thr, fixed_hp)) = self.fx.thp {
            let mut total = raw_hp + fixed_hp;
            if total < 5.0 {
                total = 5.0;
            }
            if total < thr {
                return false;
            }
        }
        true
    }

    fn restr_mid_tree_feasible(&self, next_depth: usize) -> bool {
        let n_pc = self.fx.pc_thresholds.len();
        for i in 0..n_pc {
            if self.pc_running[i] + self.pc_suffix[next_depth * n_pc + i] < self.fx.pc_thresholds[i]
            {
                return false;
            }
        }
        self.hp_gates_ok(self.hp_running + self.hp_suffix[next_depth])
    }

    fn evaluate_leaf(&mut self) {
        self.checked += 1.0;
        self.maybe_report();
        // Leaf prechecks (constraint + EHP family)
        let n_pc = self.fx.pc_thresholds.len();
        for i in 0..n_pc {
            if self.pc_running[i] < self.fx.pc_thresholds[i] {
                self.precheck_reject += 1.0;
                return;
            }
        }
        if !self.hp_gates_ok(self.hp_running) {
            self.precheck_reject += 1.0;
            return;
        }
        self.precheck_pass += 1;

        // Exact SP with set bonuses folded into the free pool.
        let mut set_free = [0i32; 5];
        for (sid, &cnt) in self.set_counts.iter().enumerate() {
            if cnt <= 0 {
                continue;
            }
            let rows = &self.fx.set_table[sid];
            let idx = (cnt as usize).min(rows.len());
            if idx == 0 {
                continue;
            }
            let row = rows[idx - 1];
            for j in 0..5 {
                set_free[j] += row[j];
            }
        }
        // Scored path (P2.4 layer 3): full leaf pipeline with ceiling gate.
        if let Some(sc) = self.scoring {
            let names: [&str; 8] = self.equip_names;
            // Gate cutoff: local 15th-best exact score, or the shared
            // floored cutoff — whichever is higher.
            let mut cutoff: Option<f64> = None;
            if self.top_n.len() >= 15 {
                cutoff = Some(self.top_n[14].0);
            }
            if let Some(shared) = self.shared_cutoff {
                let s = shared.load(Ordering::Relaxed);
                if s > 0 && (s as f64) > cutoff.unwrap_or(f64::NEG_INFINITY) {
                    cutoff = Some(s as f64);
                }
            }
            use sp_kernel::scoring::LeafOutcome;
            match sp_kernel::scoring::leaf_pipeline_gated(
                &names,
                &sc.layer2,
                &sc.weapon,
                sc.guild_unit.as_ref(),
                &mut self.kernel,
                &sc.rows,
                &sc.registry,
                &sc.hit_refs,
                &sc.tables,
                &sc.consts,
                &sc.objective,
                Some(&sc.compiled_rows),
                cutoff,
                sc.dense.as_ref().map(|d| (d, &mut self.dense_work)),
                &sc.thresholds,
                &sc.spell_base_costs,
            )
            .expect("scoring pipeline error")
            {
                LeafOutcome::SpInfeasible => {}
                LeafOutcome::Gated => {
                    self.feasible += 1;
                    self.gated += 1;
                }
                LeafOutcome::ManaReject => {
                    self.feasible += 1;
                    self.mana_reject += 1;
                }
                LeafOutcome::ThresholdReject => {
                    self.feasible += 1;
                    self.thresh_reject += 1;
                }
                LeafOutcome::Scored(r) => {
                    self.feasible += 1;
                    self.scored += 1;
                    let pos = self
                        .top_n
                        .iter()
                        .position(|(s, _)| r.score > *s)
                        .unwrap_or(self.top_n.len());
                    if pos < 15 {
                        let names_owned = names.iter().map(|s| s.to_string()).collect();
                        self.top_n.insert(pos, (r.score, names_owned));
                        self.top_n.truncate(15);
                        if self.top_n.len() == 15 {
                            if let Some(shared) = self.shared_cutoff {
                                let floored = self.top_n[14].0.floor();
                                if floored > 0.0 {
                                    shared.fetch_max(floored as u64, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                }
            }
            return;
        }

        let case = Case {
            budget: self.fx.budget,
            equipment: self.equips,
            weapon: self.fx.weapon,
            set_free,
            expected: None,
        };
        let guild_unit = self.fx.guild.as_ref().map(|(g, _)| *g);
        if self
            .kernel
            .calculate_with_extra(&case, guild_unit.as_ref())
            .is_some()
        {
            self.feasible += 1;
        }
    }

    fn place(&mut self, depth: usize, item_idx: usize) {
        let slot = &self.fx.slots[depth];
        let it = slot.pool[item_idx].clone();
        let n_pc = it.pc.len();
        for i in 0..n_pc {
            self.pc_running[i] += it.pc[i];
        }
        self.hp_running += it.hp;
        if !it.crafted {
            for j in 0..5 {
                if it.skp[j] > 0 {
                    self.sp_free_prov[j] += it.skp[j];
                }
            }
            if it.set_id >= 0 {
                self.set_counts[it.set_id as usize] += 1;
            }
        }
        self.sp_max_save[depth] = self.sp_max_req;
        for j in 0..5 {
            if it.reqs[j] > self.sp_max_req[j] {
                self.sp_max_req[j] = it.reqs[j];
            }
        }
        self.equips[slot.pos] = Unit {
            crafted: it.crafted,
            reqs: it.reqs,
            skp: it.skp,
        };
        self.equip_set[slot.pos] = it.set_id;
        if !slot.item_names.is_empty() {
            self.equip_names[slot.pos] = &slot.item_names[item_idx];
        }
        if it.illegal_id >= 0 {
            self.illegal_counts[it.illegal_id as usize] += 1;
        }
    }

    fn unplace(&mut self, depth: usize, item_idx: usize) {
        let slot = &self.fx.slots[depth];
        let it = slot.pool[item_idx].clone();
        let n_pc = it.pc.len();
        for i in 0..n_pc {
            self.pc_running[i] -= it.pc[i];
        }
        self.hp_running -= it.hp;
        if !it.crafted {
            for j in 0..5 {
                if it.skp[j] > 0 {
                    self.sp_free_prov[j] -= it.skp[j];
                }
            }
            if it.set_id >= 0 {
                self.set_counts[it.set_id as usize] -= 1;
            }
        }
        self.sp_max_req = self.sp_max_save[depth];
        self.equips[slot.pos] = Unit::default();
        self.equip_set[slot.pos] = -1;
        if !slot.item_names.is_empty() && self.fx.none_names.len() == 8 {
            self.equip_names[slot.pos] = &self.fx.none_names[slot.pos];
        }
        if it.illegal_id >= 0 {
            self.illegal_counts[it.illegal_id as usize] -= 1;
        }
    }

    fn blocks(&self, illegal_id: i32) -> bool {
        illegal_id >= 0 && self.illegal_counts[illegal_id as usize] > 0
    }

    // Visit every completion whose remaining rank sum lies in [lo_rem, hi_rem].
    fn enumerate(&mut self, depth: usize, lo_rem: i64, hi_rem: i64) {
        if self.stop {
            return;
        }
        if depth == self.n_free {
            self.evaluate_leaf();
            return;
        }
        let slot_is_ring1 = depth as isize == self.ring1_depth;
        let slot_is_ring2 = depth as isize == self.ring2_depth;
        let pool_len = self.fx.slots[depth].pool.len();
        if pool_len == 0 {
            self.enumerate(depth + 1, lo_rem, hi_rem);
            return;
        }
        let pool_max = (pool_len - 1) as i64;
        let min_offset: i64 = if slot_is_ring2 && self.ring1_depth >= 0 {
            self.ring1_placed_offset as i64
        } else {
            0
        };

        if depth == self.n_free - 1 {
            // Last slot: the leaf's remaining rank equals its offset, so the
            // in-band offsets are exactly [lo_rem, hi_rem].
            let mut from = min_offset.max(lo_rem).max(0);
            let mut to = pool_max.min(hi_rem);
            if depth == 0 {
                from = from.max(self.part_lo);
                to = to.min(self.part_hi);
            }
            let mut offset = from;
            // Cached prefix state for the cluster bound: filled at the first
            // cluster miss, reused for every cluster in this node.
            let mut prefix_state: i8 = 0; // 0 unfilled, 1 ok, -1 unavailable
            while offset <= to {
                let o = offset as usize;
                // Last-slot cluster bound: one ceiling eval covers a cluster
                // of level-adjacent items; below-cutoff clusters are skipped
                // whole (each in-band offset here is exactly one leaf).
                if let (Some(sc), Some(db)) = (self.scoring, self.dense_bound) {
                    if db.cluster_size > 0 {
                        if let Some(cutoff) = self.cutoff() {
                            // Coarse level first: one eval covers 4 fine
                            // clusters; only surviving regions descend.
                            if db.super_size > 0 && env::var("SUPER_CLUSTER").as_deref() != Ok("0")
                            {
                                let sci = o / db.super_size;
                                let mut skey = 0xEu64 << 60;
                                skey |= (sci as u64) << 49;
                                for dd in 0..depth {
                                    skey |= (self.prefix_offsets[dd] as u64) << (dd * 7);
                                }
                                let sceiling = match self.bound_memo.get(&skey) {
                                    Some(&v) => {
                                        self.cluster_memo_hits += 1;
                                        v
                                    }
                                    None => {
                                        self.cluster_evals += 1;
                                        if prefix_state == 0 {
                                            prefix_state =
                                                match sc.dense.as_ref().and_then(|d| {
                                                    d.direct.as_ref().map(|dd| (d, dd))
                                                }) {
                                                    Some((d, dd)) => {
                                                        if self.bound_work.leaf.fill_direct(
                                                            d,
                                                            dd,
                                                            &self.equip_names,
                                                        ) {
                                                            1
                                                        } else {
                                                            -1
                                                        }
                                                    }
                                                    None => -1,
                                                };
                                        }
                                        let v = if prefix_state == 1 {
                                            let d = sc.dense.as_ref().unwrap();
                                            sp_kernel::scoring::dense_ceiling_cached(
                                                d,
                                                &mut self.bound_work,
                                                &db.super_clusters[sci],
                                                &db.super_cluster_terms[sci],
                                                &sc.rows,
                                                &sc.compiled_rows,
                                                &sc.tables,
                                            )
                                        } else {
                                            f64::INFINITY
                                        };
                                        if self.bound_memo.len() >= 4_000_000 {
                                            self.bound_memo.clear();
                                        }
                                        self.bound_memo.insert(skey, v);
                                        v
                                    }
                                };
                                if sceiling < cutoff - cutoff.abs() * 1e-9 {
                                    let end = to.min(((sci + 1) * db.super_size) as i64 - 1);
                                    let skipped = (end - offset + 1) as f64;
                                    self.checked += skipped;
                                    self.bound_pruned += skipped;
                                    self.maybe_report();
                                    offset = end + 1;
                                    continue;
                                }
                            }
                            let c = o / db.cluster_size;
                            let mut key = 0xFu64 << 60;
                            key |= (c as u64) << 49;
                            for dd in 0..depth {
                                key |= (self.prefix_offsets[dd] as u64) << (dd * 7);
                            }
                            let ceiling = match self.bound_memo.get(&key) {
                                Some(&v) => {
                                    self.cluster_memo_hits += 1;
                                    v
                                }
                                None => {
                                    self.cluster_evals += 1;
                                    if prefix_state == 0 {
                                        prefix_state = match sc
                                            .dense
                                            .as_ref()
                                            .and_then(|d| d.direct.as_ref().map(|dd| (d, dd)))
                                        {
                                            Some((d, dd)) => {
                                                if self.bound_work.leaf.fill_direct(
                                                    d,
                                                    dd,
                                                    &self.equip_names,
                                                ) {
                                                    1
                                                } else {
                                                    -1
                                                }
                                            }
                                            None => -1,
                                        };
                                    }
                                    let v = if prefix_state == 1 {
                                        let d = sc.dense.as_ref().unwrap();
                                        sp_kernel::scoring::dense_ceiling_cached(
                                            d,
                                            &mut self.bound_work,
                                            &db.last_clusters[c],
                                            &db.last_cluster_terms[c],
                                            &sc.rows,
                                            &sc.compiled_rows,
                                            &sc.tables,
                                        )
                                    } else {
                                        f64::INFINITY
                                    };
                                    // Bound the memo's memory: recent
                                    // prefixes dominate hits, so a periodic
                                    // clear costs little and caps growth.
                                    if self.bound_memo.len() >= 4_000_000 {
                                        self.bound_memo.clear();
                                    }
                                    self.bound_memo.insert(key, v);
                                    v
                                }
                            };
                            if ceiling < cutoff - cutoff.abs() * 1e-9 {
                                let end = to.min(((c + 1) * db.cluster_size) as i64 - 1);
                                let skipped = (end - offset + 1) as f64;
                                self.checked += skipped;
                                self.bound_pruned += skipped;
                                self.maybe_report();
                                offset = end + 1;
                                continue;
                            }
                        }
                    }
                }
                let illegal = self.fx.slots[depth].pool[o].illegal_id;
                if self.blocks(illegal) {
                    self.checked += 1.0;
                    self.maybe_report();
                } else if !self.sp_bound_ok(depth, o, true) {
                    self.checked += 1.0;
                    self.sp_leaf_reject += 1;
                    self.maybe_report();
                } else if !self.restr_bound_ok(depth, o, true) {
                    self.checked += 1.0;
                    self.precheck_reject += 1.0;
                    self.maybe_report();
                } else {
                    self.place(depth, o);
                    self.evaluate_leaf();
                    self.unplace(depth, o);
                }
                offset += 1;
            }
            return;
        }

        // Band reachability: offsets too small to reach lo_rem have no
        // in-band leaves (all were visited in earlier bands).
        let reach_min = lo_rem - self.suffix_max_rank[depth + 1];
        let mut offset = min_offset.max(reach_min).max(0);
        let mut max_offset = hi_rem.min(pool_max);
        if depth == 0 {
            offset = offset.max(self.part_lo);
            max_offset = max_offset.min(self.part_hi);
        }
        while offset <= max_offset {
            let o = offset as usize;
            let illegal = self.fx.slots[depth].pool[o].illegal_id;
            if self.blocks(illegal) {
                if slot_is_ring1 && self.rings_contiguous {
                    self.rebuild_ring2_subtree(o);
                }
                self.checked += self.band_credit(depth + 1, lo_rem - offset, hi_rem - offset);
                self.maybe_report();
                offset += 1;
                continue;
            }
            if !self.sp_bound_ok(depth, o, false) {
                if slot_is_ring1 && self.rings_contiguous {
                    self.rebuild_ring2_subtree(o);
                }
                self.checked += self.band_credit(depth + 1, lo_rem - offset, hi_rem - offset);
                self.maybe_report();
                offset += 1;
                continue;
            }
            if !self.restr_bound_ok(depth, o, false) {
                if slot_is_ring1 && self.rings_contiguous {
                    self.rebuild_ring2_subtree(o);
                }
                let pruned = self.band_credit(depth + 1, lo_rem - offset, hi_rem - offset);
                self.checked += pruned;
                self.precheck_reject += pruned;
                self.maybe_report();
                offset += 1;
                continue;
            }
            // Mid-tree damage ceiling bound (shallow depths only; the eval is
            // a full damage computation, memoized per prefix).
            let bound_here = depth < self.bound_max_depth
                || (self.bound_tail > 0
                    && depth + 1 < self.n_free
                    && self.n_free - (depth + 1) <= self.bound_tail);
            if bound_here && self.bound_tables.is_some() {
                if let Some(cutoff) = self.cutoff() {
                    if self.bound_prunes(depth, o, cutoff, hi_rem) {
                        if slot_is_ring1 && self.rings_contiguous {
                            self.rebuild_ring2_subtree(o);
                        }
                        let pruned = self.band_credit(depth + 1, lo_rem - offset, hi_rem - offset);
                        self.checked += pruned;
                        self.bound_pruned += pruned;
                        self.maybe_report();
                        offset += 1;
                        continue;
                    }
                }
            }
            self.place(depth, o);
            self.prefix_offsets[depth] = o as u8;
            if slot_is_ring1 {
                self.ring1_placed_offset = o;
                if self.rings_contiguous {
                    self.rebuild_ring2_subtree(o);
                }
            }
            self.enumerate(depth + 1, lo_rem - offset, hi_rem - offset);
            self.unplace(depth, o);
            offset += 1;
        }
    }

    fn run(&mut self) {
        // Total canonical space = sum over L of the root subtree counts.
        let mut total = 0.0;
        for l in 0..=self.l_max {
            total += self.subtree[0][l];
        }
        self.total_space = total.max(1.0);
        self.started = Instant::now();
        self.report_every = 1.0;
        self.next_report = 5.0;

        if self.n_free == 0 {
            self.evaluate_leaf();
            return;
        }
        // Geometric level bands (see the JS worker): fine-grained ordering
        // early, O(log L_max) prefix re-walks overall.
        let l_max = self.l_max as i64;
        let mut band_lo: i64 = 0;
        let mut band_width: i64 = 1;
        while band_lo <= l_max {
            let band_hi = l_max.min(band_lo + band_width - 1);
            self.enumerate(0, band_lo, band_hi);
            band_lo = band_hi + 1;
            band_width *= 2;
        }
    }
}

#[derive(Default)]
struct Totals {
    checked: f64,
    precheck_reject: f64,
    precheck_pass: u64,
    sp_leaf_reject: u64,
    feasible: u64,
    scored: u64,
    gated: u64,
    mana_reject: u64,
    thresh_reject: u64,
    bound_pruned: f64,
    top_n: Vec<(f64, Vec<String>)>,
}

fn merge_top(into: &mut Vec<(f64, Vec<String>)>, from: Vec<(f64, Vec<String>)>) {
    for (score, names) in from {
        let pos = into
            .iter()
            .position(|(s, _)| score > *s)
            .unwrap_or(into.len());
        if pos < 15 {
            into.insert(pos, (score, names));
            into.truncate(15);
        }
    }
}

pub struct CoreResult {
    pub exhaustive: bool,
    pub checked: f64,
    pub precheck_reject: f64,
    pub precheck_pass: u64,
    pub sp_leaf_reject: u64,
    pub feasible: u64,
    pub scored: u64,
    pub gated: u64,
    pub mana_reject: u64,
    pub threshold_reject: u64,
    pub bound_pruned: f64,
    pub top_n: Vec<(f64, Vec<String>)>,
}

pub fn solve_single(
    enumeration_fixture: &str,
    scoring_fixture: Option<&serde_json::Value>,
) -> Result<CoreResult, String> {
    let fx = parse_fixture(enumeration_fixture);
    let scoring_ctx = scoring_fixture
        .map(sp_kernel::scoring::ScoringCtx::load)
        .transpose()?;

    if scoring_ctx.is_some() {
        if !fx
            .slots
            .iter()
            .all(|slot| slot.item_names.len() == slot.pool.len())
        {
            return Err("enumeration fixture lacks a complete NAMES section".to_owned());
        }
        if fx.none_names.len() != 8 {
            return Err("enumeration fixture lacks NONENAMES".to_owned());
        }
    }

    let scoring = scoring_ctx.as_ref();
    let shared_cutoff = AtomicU64::new(0);
    let bound_tail = 1;
    let bound_cluster = 4;
    let bound_tables = scoring.and_then(|context| {
        if !fx.slots.iter().all(|slot| slot.pool.len() < 128)
            || !context.objective.supports_ceiling()
            || !context.layer2.ceiling_vars_ok
            || context.consts.hp_casting
        {
            return None;
        }
        let slot_pools: Vec<Vec<String>> = fx
            .slots
            .iter()
            .map(|slot| slot.item_names.clone())
            .collect();
        context.layer2.build_bound_tables(&slot_pools).ok()
    });
    let bounds = bound_tables.as_ref();
    let dense_bound = match (scoring, bounds) {
        (Some(context), Some(_)) => context.dense.as_ref().and_then(|dense| {
            let slot_pools: Vec<Vec<String>> = fx
                .slots
                .iter()
                .map(|slot| slot.item_names.clone())
                .collect();
            sp_kernel::scoring::DenseBound::build(
                &context.layer2,
                dense,
                &slot_pools,
                bound_cluster,
            )
        }),
        _ => None,
    };

    let mut search = Search::new(&fx);
    search.scoring = scoring;
    search.shared_cutoff = Some(&shared_cutoff);
    search.bound_tables = bounds;
    search.bound_max_depth = 0;
    search.bound_tail = bound_tail;
    search.dense_bound = dense_bound.as_ref();
    search.init_equip_names();
    search.next_report = f64::INFINITY;
    search.run();

    Ok(CoreResult {
        exhaustive: !search.stop,
        checked: search.checked,
        precheck_reject: search.precheck_reject,
        precheck_pass: search.precheck_pass,
        sp_leaf_reject: search.sp_leaf_reject,
        feasible: search.feasible,
        scored: search.scored,
        gated: search.gated,
        mana_reject: search.mana_reject,
        threshold_reject: search.thresh_reject,
        bound_pruned: search.bound_pruned,
        top_n: search.top_n,
    })
}

pub fn run_cli() {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args
        .get(1)
        .map(String::as_str)
        .expect("usage: enum_kernel <fixture> [threads] [score_fixture.json]");
    let text = fs::read_to_string(fixture_path).expect("cannot read fixture");
    let fx = parse_fixture(&text);

    let n_threads: usize = args
        .get(2)
        .map(|s| s.parse().expect("threads must be a number"))
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        });

    // Optional scoring context (P2.4 layer 3): full leaf pipeline + top-N.
    let scoring_ctx: Option<sp_kernel::scoring::ScoringCtx> = args.get(3).map(|p| {
        let text = fs::read_to_string(p).expect("cannot read score fixture");
        let json: serde_json::Value = serde_json::from_str(&text).expect("invalid score fixture");
        let ctx = sp_kernel::scoring::ScoringCtx::load(&json).expect("scoring context");
        assert!(
            fx.slots.iter().all(|s| s.item_names.len() == s.pool.len()),
            "enum fixture lacks the NAMES section — re-export with the current exporter"
        );
        assert_eq!(fx.none_names.len(), 8, "enum fixture lacks NONENAMES");
        ctx
    });
    let scoring = scoring_ctx.as_ref();
    sp_kernel::scoring::trace::init_from_env();
    let shared_cutoff = AtomicU64::new(0);

    // Mid-tree damage ceiling bound tables (objective B&B). Memo keys pack
    // offsets into 7 bits, so guard on pool sizes.
    let bound_max_depth: usize = env::var("BOUND_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let bound_tail: usize = env::var("BOUND_TAIL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let bound_tables: Option<sp_kernel::scoring::BoundTables> = scoring.and_then(|sc| {
        let bound_cluster_on: bool = env::var("BOUND_CLUSTER")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(4)
            > 0;
        if bound_max_depth == 0 && bound_tail == 0 && !bound_cluster_on {
            return None;
        }
        if !fx.slots.iter().all(|s| s.pool.len() < 128) {
            eprintln!("bound: pool >= 128 items, memo packing disabled — skipping bound");
            return None;
        }
        if !sc.objective.supports_ceiling() || !sc.layer2.ceiling_vars_ok || sc.consts.hp_casting {
            return None;
        }
        let slot_pools: Vec<Vec<String>> = fx.slots.iter().map(|s| s.item_names.clone()).collect();
        Some(
            sc.layer2
                .build_bound_tables(&slot_pools)
                .expect("bound tables"),
        )
    });
    let bounds = bound_tables.as_ref();
    let bound_cluster: usize = env::var("BOUND_CLUSTER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let dense_bound: Option<sp_kernel::scoring::DenseBound> = match (scoring, bounds) {
        (Some(sc), Some(_)) => sc.dense.as_ref().and_then(|d| {
            let slot_pools: Vec<Vec<String>> =
                fx.slots.iter().map(|s| s.item_names.clone()).collect();
            sp_kernel::scoring::DenseBound::build(&sc.layer2, d, &slot_pools, bound_cluster)
        }),
        _ => None,
    };
    let dense_bound = dense_bound.as_ref();

    // Warm start: solve the elite subspace (top-WARM_K tail of each
    // level-ordered pool) first, sharing the cutoff atomic. Its top-15 are
    // real builds, so the seeded cutoff is admissible for the main run —
    // the gate and cluster bound prune hard from the first node instead of
    // waiting for the cutoff to warm up. WARM_K=0 disables.
    let warm_k: usize = env::var("WARM_K")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    if scoring.is_some() && warm_k > 0 && fx.slots.iter().any(|s| s.pool.len() > warm_k) {
        // Rank each pool's items by their solo objective ceiling (item alone
        // on a none-item build at all-150 SP) and keep the top WARM_K per
        // slot, preserving level order so the band machinery stays valid.
        // Ranking is a heuristic — cutoff admissibility comes from the warm
        // builds being real scored builds, not from the selection.
        let sc = scoring.unwrap();
        let mut base_names: [&str; 8] = Default::default();
        if fx.none_names.len() == 8 {
            for p in 0..8 {
                base_names[p] = &fx.none_names[p];
            }
        }
        for (pos, name) in &fx.fixed_names {
            base_names[*pos] = name;
        }
        let mut warm_sel: Vec<Vec<usize>> = Vec::with_capacity(fx.slots.len());
        {
            let mut work = sp_kernel::scoring::DenseWork::default();
            for sl in &fx.slots {
                let mut ranked: Vec<(usize, f64)> = sl
                    .item_names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        let mut names = base_names;
                        names[sl.pos] = name.as_str();
                        let c = sc
                            .dense
                            .as_ref()
                            .and_then(|d| {
                                sp_kernel::scoring::dense_ceiling_with(
                                    d,
                                    &[],
                                    &[],
                                    &names,
                                    &mut work,
                                    &sc.rows,
                                    &sc.compiled_rows,
                                    &sc.tables,
                                )
                            })
                            .unwrap_or(f64::NEG_INFINITY);
                        (i, c)
                    })
                    .collect();
                ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let mut sel: Vec<usize> = ranked.iter().take(warm_k).map(|(i, _)| *i).collect();
                sel.sort_unstable();
                warm_sel.push(sel);
            }
        }
        let wfx = Fixture {
            budget: fx.budget,
            pc_thresholds: fx.pc_thresholds.clone(),
            pc_start: fx.pc_start.clone(),
            ehp: fx.ehp,
            ehpna: fx.ehpna,
            thp: fx.thp,
            hp_start: fx.hp_start,
            weapon: fx.weapon,
            guild: fx.guild,
            fixed: fx.fixed.clone(),
            slots: fx
                .slots
                .iter()
                .zip(&warm_sel)
                .map(|(sl, sel)| Slot {
                    name: sl.name.clone(),
                    pos: sl.pos,
                    is_ring1: sl.is_ring1,
                    is_ring2: sl.is_ring2,
                    pool: sel.iter().map(|&i| sl.pool[i].clone()).collect(),
                    item_names: sel.iter().map(|&i| sl.item_names[i].clone()).collect(),
                })
                .collect(),
            set_table: fx.set_table.clone(),
            fixed_names: fx.fixed_names.clone(),
            none_names: fx.none_names.clone(),
        };
        let warm_started = Instant::now();
        let wpools: Vec<Vec<String>> = wfx.slots.iter().map(|s| s.item_names.clone()).collect();
        let wdb = sc.dense.as_ref().and_then(|d| {
            sp_kernel::scoring::DenseBound::build(&sc.layer2, d, &wpools, bound_cluster)
        });
        let mut ws = Search::new(&wfx);
        ws.scoring = scoring;
        ws.shared_cutoff = Some(&shared_cutoff);
        ws.dense_bound = wdb.as_ref();
        ws.init_equip_names();
        ws.next_report = f64::INFINITY;
        ws.run();
        eprintln!(
            "warm: {} leaves ({} scored) in {:.2}s | cutoff seeded {:.6e}",
            ws.checked,
            ws.scored,
            warm_started.elapsed().as_secs_f64(),
            shared_cutoff.load(Ordering::Relaxed) as f64,
        );
    }

    let start = Instant::now();

    let (totals, elapsed) = if n_threads <= 1 || fx.slots.is_empty() {
        let mut search = Search::new(&fx);
        search.scoring = scoring;
        search.shared_cutoff = Some(&shared_cutoff);
        search.bound_tables = bounds;
        search.bound_max_depth = bound_max_depth;
        search.bound_tail = bound_tail;
        search.dense_bound = dense_bound;
        search.init_equip_names();
        search.run();
        let elapsed = start.elapsed();
        (
            Totals {
                checked: search.checked,
                precheck_reject: search.precheck_reject,
                precheck_pass: search.precheck_pass,
                sp_leaf_reject: search.sp_leaf_reject,
                feasible: search.feasible,
                scored: search.scored,
                gated: search.gated,
                mana_reject: search.mana_reject,
                thresh_reject: search.thresh_reject,
                bound_pruned: search.bound_pruned,
                top_n: search.top_n,
            },
            elapsed,
        )
    } else {
        // Work-stealing over first-slot offsets: each claim runs the full
        // band sweep restricted to one offset — the same 'slot' partition
        // shape the JS engine uses, so per-offset subspaces are disjoint and
        // the integral counters sum exactly.
        let first_pool_len = fx.slots[0].pool.len();
        let next_offset = AtomicUsize::new(0);
        let shared_checked = AtomicU64::new(0);
        let stop_flag = AtomicU64::new(0);
        let time_cap: Option<f64> = std::env::var("ENUM_TIME_CAP_SECS")
            .ok()
            .and_then(|v| v.parse().ok());
        let done = AtomicU64::new(0);

        // Full-space total for the monitor line.
        let total_space = {
            let s = Search::new(&fx);
            let mut total = 0.0;
            for l in 0..=s.l_max {
                total += s.subtree[0][l];
            }
            total.max(1.0)
        };

        let totals = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..n_threads.min(first_pool_len) {
                handles.push(scope.spawn(|| {
                    let mut search = Search::new(&fx);
                    search.shared_checked = Some(&shared_checked);
                    search.stop_flag = Some(&stop_flag);
                    search.scoring = scoring;
                    search.shared_cutoff = Some(&shared_cutoff);
                    search.bound_tables = bounds;
                    search.bound_max_depth = bound_max_depth;
                    search.bound_tail = bound_tail;
                    search.dense_bound = dense_bound;
                    search.init_equip_names();
                    search.started = Instant::now();
                    // Suppress the single-thread report path entirely.
                    search.next_report = f64::INFINITY;
                    let l_max = search.l_max as i64;
                    loop {
                        if search.stop {
                            break;
                        }
                        let o = next_offset.fetch_add(1, Ordering::Relaxed);
                        if o >= first_pool_len {
                            break;
                        }
                        search.part_lo = o as i64;
                        search.part_hi = o as i64;
                        let mut band_lo: i64 = 0;
                        let mut band_width: i64 = 1;
                        while band_lo <= l_max {
                            let band_hi = l_max.min(band_lo + band_width - 1);
                            search.enumerate(0, band_lo, band_hi);
                            band_lo = band_hi + 1;
                            band_width *= 2;
                        }
                    }
                    search.flush_checked();
                    if std::env::var("CLUSTER_STATS").as_deref() == Ok("1") {
                        eprintln!(
                            "cluster_stats: evals {} | memo_hits {} | memo_len {}",
                            search.cluster_evals,
                            search.cluster_memo_hits,
                            search.bound_memo.len()
                        );
                    }
                    Totals {
                        checked: search.checked,
                        precheck_reject: search.precheck_reject,
                        precheck_pass: search.precheck_pass,
                        sp_leaf_reject: search.sp_leaf_reject,
                        feasible: search.feasible,
                        scored: search.scored,
                        gated: search.gated,
                        mana_reject: search.mana_reject,
                        thresh_reject: search.thresh_reject,
                        bound_pruned: search.bound_pruned,
                        top_n: search.top_n,
                    }
                }));
            }

            // Monitor thread: progress/rate/ETA line every ~5s. Polls the
            // done flag at 20Hz so small runs aren't floored by its sleep.
            let monitor = scope.spawn(|| {
                let started = Instant::now();
                let mut next_report = 5.0f64;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    if done.load(Ordering::Relaxed) != 0 { break; }
                    let elapsed = started.elapsed().as_secs_f64();
                    if let Some(cap) = time_cap {
                        if elapsed >= cap { stop_flag.store(1, Ordering::Relaxed); }
                    }
                    if elapsed < next_report { continue; }
                    next_report = elapsed + 5.0;
                    let checked = shared_checked.load(Ordering::Relaxed) as f64;
                    if checked == 0.0 { continue; }
                    let rate = checked / elapsed;
                    let remaining = (total_space - checked).max(0.0);
                    eprintln!(
                        "progress: {:.2}% | checked {:.3e}/{:.3e} | {:.2e} checked/s | elapsed {:.0}s | eta {:.0}s",
                        checked / total_space * 100.0, checked, total_space,
                        rate, elapsed, remaining / rate,
                    );
                }
            });

            let mut totals = Totals::default();
            for h in handles {
                let t = h.join().expect("worker thread panicked");
                totals.checked += t.checked;
                totals.precheck_reject += t.precheck_reject;
                totals.precheck_pass += t.precheck_pass;
                totals.sp_leaf_reject += t.sp_leaf_reject;
                totals.feasible += t.feasible;
                totals.scored += t.scored;
                totals.gated += t.gated;
                totals.mana_reject += t.mana_reject;
                totals.thresh_reject += t.thresh_reject;
                totals.bound_pruned += t.bound_pruned;
                merge_top(&mut totals.top_n, t.top_n);
            }
            done.store(1, Ordering::Relaxed);
            monitor.join().expect("monitor thread panicked");
            totals
        });
        (totals, start.elapsed())
    };

    println!(
        "enum_kernel: checked {} | precheck_reject {} | precheck_pass {} | sp_leaf_reject {} | feasible {} | threads {} | elapsed {:.3}s | {:.0} checked/s",
        totals.checked, totals.precheck_reject, totals.precheck_pass,
        totals.sp_leaf_reject, totals.feasible,
        if fx.slots.is_empty() { 1 } else { n_threads.min(fx.slots[0].pool.len()).max(1) },
        elapsed.as_secs_f64(),
        totals.checked / elapsed.as_secs_f64(),
    );
    sp_kernel::scoring::trace::report();
    if scoring.is_some() {
        println!(
            "scoring: scored {} | gated {} | mana_reject {} | thresh_reject {} | bound_pruned {}",
            totals.scored,
            totals.gated,
            totals.mana_reject,
            totals.thresh_reject,
            totals.bound_pruned,
        );
        for (score, names) in &totals.top_n {
            println!(
                "top15: {:.17e} | {}",
                score,
                names
                    .iter()
                    .filter(|n| !n.starts_with("No "))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}
