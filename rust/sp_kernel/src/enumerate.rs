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

use crate::{Case, Kernel, Unit, SP_PER_ATTR_CAP};
use std::env;
use std::fs;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use crate::clock::{Clock, Instant};

/// Bound-eval timing helpers (SCORE_TRACE=1): measures the batch-shaped
/// ceiling work a GPU offload would target.
#[inline]
fn bound_timer() -> Option<Instant> {
    if crate::scoring::trace::on() { Some(Instant::now()) } else { None }
}
#[inline]
fn bound_timer_end(t0: Option<Instant>) {
    if let Some(t0) = t0 {
        crate::scoring::trace::add(
            crate::scoring::trace::BOUND, t0.elapsed().as_nanos() as u64);
        crate::scoring::trace::add(crate::scoring::trace::BOUND_EVALS, 1);
    }
}

/// Self-tuning switch for a bound layer.
///
/// Ablation shows the coarse bound layers are scenario-dependent: the tail
/// bound pays ~10% on tight-bound objectives (flat-stat targets whose
/// ceilings discriminate sharply) and costs ~6% on loose-bound ones
/// (combo damage), and no fixed default is right for both. So each layer
/// measures itself: a bound eval costs about what evaluating one leaf
/// costs, so a layer must average at least one pruned leaf per eval to pay
/// for itself. Layers that fall below that are switched off, and re-sampled
/// later since a tightening cutoff can make a layer profitable mid-run.
///
/// This only decides how much work is SKIPPED, never what a surviving leaf
/// scores, so results are unaffected — same top-N either way.
struct AdaptiveBound {
    enabled: bool,
    evals: u64,
    pruned: f64,
    window: u64,
    retry_at: f64,
}

/// Entry cap for the subtree/cluster ceiling memo.
///
/// Was four million, which let the map grow far past any cache: every probe
/// then paid a miss to the table itself, and on a low-hit scenario the memo
/// cost more than recomputing the ceiling it was caching. A ceiling eval is
/// microseconds against nanoseconds for a probe, so the memo is worth having
/// at almost any hit rate -- what it is not worth is being large. Capped
/// here at a size that stays cache-resident; on overflow the map is cleared
/// and refills against the current part of the search.
const BOUND_MEMO_CAP: usize = 1 << 18;

const ADAPT_WINDOW: u64 = 8192;
const ADAPT_RETRY_LEAVES: f64 = 20_000_000.0;

impl AdaptiveBound {
    fn new() -> Self {
        AdaptiveBound { enabled: true, evals: 0, pruned: 0.0, window: ADAPT_WINDOW, retry_at: 0.0 }
    }
    #[inline]
    fn armed(&mut self, checked: f64) -> bool {
        if !self.enabled && checked >= self.retry_at {
            self.enabled = true;
            self.evals = 0;
            self.pruned = 0.0;
            self.window = ADAPT_WINDOW;
        }
        self.enabled
    }
    #[inline]
    fn record(&mut self, pruned_leaves: f64, checked: f64) {
        self.evals += 1;
        self.pruned += pruned_leaves;
        if self.evals >= self.window {
            if self.pruned < self.evals as f64 {
                self.enabled = false;
                self.retry_at = checked + ADAPT_RETRY_LEAVES;
            }
            self.evals = 0;
            self.pruned = 0.0;
        }
    }
}

#[derive(Clone)]
pub struct PoolItem {
    crafted: bool,
    reqs: [i32; 5],
    skp: [i32; 5],
    set_id: i32,
    illegal_id: i32,
    hp: f64,
    pc: Vec<f64>,
}

pub struct Slot {
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

pub struct Fixture {
    budget: i32,
    pc_thresholds: Vec<f64>,
    pc_start: Vec<f64>,
    ehp: Option<(f64, f64, f64)>,   // threshold, fixed_hp, divisor
    ehpna: Option<(f64, f64, f64)>,
    thp: Option<(f64, f64)>,        // threshold, fixed_hp
    hp_start: f64,
    weapon: Unit,
    guild: Option<(Unit, i32)>,     // unit, set_id
    fixed: Vec<(usize, Unit, i32, i32)>, // pos, unit, set_id, illegal_id
    slots: Vec<Slot>,
    set_table: Vec<Vec<[i32; 5]>>,  // set_id -> bonuses per count (count-1 indexed)
    fixed_names: Vec<(usize, String)>,   // pos, display name (NAMES section)
    none_names: Vec<String>,             // 8 none-item names by slot position
}

pub fn parse_fixture(text: &str) -> Fixture {
    let mut lines = text.lines();
    let mut next = || lines.next().expect("truncated fixture");
    let toks = |l: &str| l.split_ascii_whitespace().map(String::from).collect::<Vec<_>>();

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
        if t[1] == "1" { Some((t[2].parse().unwrap(), t[3].parse().unwrap(), t[4].parse().unwrap())) } else { None }
    };
    let ehp = parse_gate3(&toks(next()));
    let ehpna = parse_gate3(&toks(next()));
    let thp_t = toks(next());
    let thp = if thp_t[1] == "1" { Some((thp_t[2].parse().unwrap(), thp_t[3].parse().unwrap())) } else { None };
    let hp_start: f64 = toks(next())[1].parse().unwrap();

    let unit_from = |t: &[String], off: usize| -> Unit {
        let mut reqs = [0i32; 5];
        let mut skp = [0i32; 5];
        for j in 0..5 { reqs[j] = t[off + j].parse().unwrap(); }
        for j in 0..5 { skp[j] = t[off + 5 + j].parse().unwrap(); }
        Unit { crafted: false, reqs, skp }
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
    } else { None };

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
            for k in 0..n_pc { pc.push(it[15 + k].parse().unwrap()); }
            pool.push(PoolItem { crafted: u.crafted, reqs: u.reqs, skp: u.skp, set_id, illegal_id, hp, pc });
        }
        slots.push(Slot { name, pos, is_ring1, is_ring2, pool, item_names: Vec::new() });
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
            for j in 0..5 { row[j] = t[3 + c * 5 + j].parse().unwrap(); }
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
        if t.is_empty() { continue; }
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
                    none_names.push(lines.next().expect("truncated NONENAMES").trim().to_string());
                }
            }
            _ => {}
        }
    }

    Fixture { budget, pc_thresholds, pc_start, ehp, ehpna, thp, hp_start,
              weapon, guild, fixed, slots, set_table, fixed_names, none_names }
}

pub struct Search<'a> {
    fx: &'a Fixture,
    n_free: usize,
    l_max: usize,
    ring1_depth: isize,
    ring2_depth: isize,
    rings_contiguous: bool,

    // Suffix bounds
    sp_suffix_max_prov: Vec<[i32; 5]>,   // [n_free+1][5]
    /// Set ids granting skill points at some piece count.
    sp_set_ids: Vec<u32>,
    /// [sp_set_ids index][depth] -> additional pieces the slots from
    /// `depth` onward could still supply. Flat, row length n_free + 1.
    sp_set_reach: Vec<u8>,
    /// Per-node hoist: every term of the `sp_bound_ok` provision estimate
    /// that is constant across one slot's offsets. Refreshed once per
    /// node, so the inner loop adds only the candidate item's own skp.
    sp_bound_base: [i32; 5],
    pc_suffix: Vec<f64>,                 // [(n_free+1) * n_pc]
    hp_suffix: Vec<f64>,                 // [n_free+1]

    // Subtree leaf counts
    subtree: Vec<Vec<f64>>,              // [n_free+1][l_max+1]
    subtree_prefix: Vec<Vec<f64>>,       // [n_free+1][l_max+2]
    suffix_max_rank: Vec<i64>,           // [n_free+1]
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
    /// Builds actually handed to `evaluate_leaf`. `checked` credits whole
    /// pruned subtrees it never visited, so the two diverge by orders of
    /// magnitude once the bounds engage -- and only this one counts work done.
    pub leaf_calls: u64,
    precheck_reject: f64,
    precheck_pass: u64,
    feasible: u64,
    sp_leaf_reject: u64,
    /// Leaves the cheap SP bound let through that the exact SP kernel then
    /// rejected — the headroom a stronger admissible SP bound could claim.
    sp_kernel_reject: u64,
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
    time_cap: Option<f64>,
    /// Deterministic work budget: stop once this many leaves are credited.
    /// Used where there is no usable wall clock (wasm) and wherever a
    /// reproducible chunk of work is wanted instead of a time slice.
    pub leaf_budget: Option<f64>,
    /// SP_BOUND_OFF=1: read once at construction, never in the hot path.
    sp_bound_off: bool,
    dense_work: crate::scoring::DenseWork,
    /// Separate buffers for bound evals so the leaf pipeline can't clobber
    /// the cached last-slot prefix state.
    bound_work: crate::scoring::DenseWork,
    checked_flushed: f64,

    /// Optional live-progress sink (browser UI). Called every
    /// `progress_every` credited leaves with a funnel snapshot plus the
    /// current top-N, so a long solve shows movement instead of looking
    /// hung. Keyed on leaves rather than wall time because wasm32 has no
    /// usable clock — which also makes emission points deterministic.
    progress: Option<&'a mut dyn FnMut(ProgressSnapshot) -> Option<f64>>,
    progress_every: f64,
    next_progress: f64,

    // Scoring integration (P2.4 layer 3): current equip names by position,
    // the scenario scoring context, per-thread top-N, and the shared cutoff
    // (floor(score) as u64; 0 = unset — floor is admissible for the gate).
    equip_names: [&'a str; 8],
    scoring: Option<&'a crate::scoring::ScoringCtx>,
    top_n: Vec<TopEntry>,
    shared_cutoff: Option<&'a AtomicU64>,
    scored: u64,
    gated: u64,
    mana_reject: u64,
    thresh_reject: u64,
    cluster_evals: u64,
    cluster_memo_hits: u64,
    adapt_super: AdaptiveBound,
    adapt_tail: AdaptiveBound,

    // Mid-tree damage ceiling bound (objective branch-and-bound).
    bound_tables: Option<&'a crate::scoring::BoundTables>,
    bound_max_depth: usize,
    /// Apply the per-offset bound when the REMAINING depth (slots after the
    /// candidate) is <= bound_tail — near the leaves the suffix maxima cover
    /// one or two pools and the ceiling is nearly as tight as the leaf gate,
    /// so one eval prunes a whole last-pool subtree.
    bound_tail: usize,
    dense_bound: Option<&'a crate::scoring::DenseBound>,
    bound_pruned: f64,
    /// Ceiling memo keyed by packed prefix offsets (the ceiling depends on
    /// the prefix, not the band, and prefixes recur across band sweeps).
    bound_memo: std::collections::HashMap<u64, f64>,
    /// Current prefix offsets (by depth) for memo keys.
    prefix_offsets: [u8; 8],
}

impl<'a> Search<'a> {
    pub fn new(fx: &'a Fixture) -> Self {
        let n_free = fx.slots.len();
        let n_pc = fx.pc_thresholds.len();
        let mut ring1_depth = -1isize;
        let mut ring2_depth = -1isize;
        for (d, s) in fx.slots.iter().enumerate() {
            if s.is_ring1 { ring1_depth = d as isize; }
            if s.is_ring2 { ring2_depth = d as isize; }
        }
        let both = ring1_depth >= 0 && ring2_depth >= 0;
        let rings_contiguous = both && ring2_depth == ring1_depth + 1;

        let l_max: usize = fx.slots.iter()
            .map(|s| s.pool.len().saturating_sub(1))
            .filter(|&ub| ub > 0)
            .sum();

        // Set-granted skill points, for the bound in `sp_bound_ok`.
        //
        // Items provide skill points through `skp`, but sets also grant them
        // once enough pieces are worn -- one set in the shipped corpus grants
        // +85 to a single attribute at three pieces. Omitting that understates
        // provision, which OVERSTATES the deficit and prunes branches that are
        // genuinely buildable: 818 lost builds across the six small family
        // fixtures.
        //
        // A static cap (each set's best row, summed) is admissible but useless:
        // measured, it prunes so little that it runs slower than deleting the
        // bound outright. The bound has to know what is still REACHABLE, which
        // depends on how many pieces of each set are already worn and how many
        // free slots remain -- see `refresh_sp_bound_base`.
        // Sets that grant skill points at some piece count. Sets with only
        // stat bonuses cannot affect the skill point bound, and skipping them
        // keeps the per-node walk proportional to the sets that matter.
        let sp_set_ids: Vec<u32> = fx.set_table.iter().enumerate()
            .filter(|(_, rows)| rows.iter().any(|r| r.iter().any(|&v| v > 0)))
            .map(|(sid, _)| sid as u32)
            .collect();

        // How many MORE pieces of each set the remaining slots could supply.
        //
        // Counting free slots is not the same question: a slot can only add a
        // piece if its pool actually holds one, and each slot adds at most one.
        // Where a set is simply not stocked by the slots that are still open,
        // this is zero and the set contributes no slack at all -- so scenarios
        // that never had a set-skill-point build to lose pay nothing for the
        // bound being correct.
        let mut sp_set_reach = vec![0u8; sp_set_ids.len() * (n_free + 1)];
        for (si, &sid) in sp_set_ids.iter().enumerate() {
            for d in (0..n_free).rev() {
                // `place` counts any item with a set id, crafted included, so
                // match it here. Over-counting only loosens the bound.
                let stocked = fx.slots[d].pool.iter()
                    .any(|it| it.set_id == sid as i32);
                sp_set_reach[si * (n_free + 1) + d] =
                    sp_set_reach[si * (n_free + 1) + d + 1] + u8::from(stocked);
            }
        }

        let mut sp_suffix_max_prov = vec![[0i32; 5]; n_free + 1];
        for d in (0..n_free).rev() {
            let mut maxp = [0i32; 5];
            for it in &fx.slots[d].pool {
                for j in 0..5 {
                    if it.skp[j] > maxp[j] { maxp[j] = it.skp[j]; }
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
                if fx.slots[d].pool.is_empty() { slot_max = 0.0; }
                for it in &fx.slots[d].pool {
                    if it.pc[i] > slot_max { slot_max = it.pc[i]; }
                }
                pc_suffix[d * n_pc + i] = pc_suffix[(d + 1) * n_pc + i] + slot_max;
            }
        }
        let mut hp_suffix = vec![0f64; n_free + 1];
        for d in (0..n_free).rev() {
            let mut slot_max = f64::NEG_INFINITY;
            if fx.slots[d].pool.is_empty() { slot_max = 0.0; }
            for it in &fx.slots[d].pool {
                if it.hp > slot_max { slot_max = it.hp; }
            }
            hp_suffix[d] = hp_suffix[d + 1] + slot_max;
        }

        // Ring pair count (contiguous case)
        let mut ring_pair_count = vec![0f64; l_max + 1];
        if rings_contiguous {
            let n = fx.slots[ring1_depth as usize].pool.len();
            for a in 0..n {
                for b in a..n {
                    if a + b <= l_max { ring_pair_count[a + b] += 1.0; }
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
                    if c == 0.0 { continue; }
                    for lt in 0..=(l_max - lp) {
                        let t = tail[lt];
                        if t != 0.0 { subtree[d][lp + lt] += c * t; }
                    }
                }
                continue;
            }
            let ub = fx.slots[d].pool.len().saturating_sub(1);
            if fx.slots[d].pool.is_empty() { continue; }
            let tail = subtree[d + 1].clone();
            let mut prefix = vec![0f64; l_max + 2];
            for l in 0..=l_max { prefix[l + 1] = prefix[l] + tail[l]; }
            for l in 0..=l_max {
                let lo = l.saturating_sub(ub);
                let hi = l.min(l_max);
                if hi + 1 > lo { subtree[d][l] = prefix[hi + 1] - prefix[lo]; }
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
                    if u.skp[j] > 0 { sp_fixed_prov[j] += u.skp[j]; }
                }
            }
            for j in 0..5 {
                if u.reqs[j] > sp_fixed_max_req[j] { sp_fixed_max_req[j] = u.reqs[j]; }
            }
            if *set_id >= 0 && !u.crafted { set_counts[*set_id as usize] += 1; }
            if *illegal_id >= 0 { illegal_counts[*illegal_id as usize] += 1; }
        }
        if let Some((g, gset)) = &fx.guild {
            for j in 0..5 {
                if g.skp[j] > 0 { sp_fixed_prov[j] += g.skp[j]; }
            }
            for j in 0..5 {
                if g.reqs[j] > sp_fixed_max_req[j] { sp_fixed_max_req[j] = g.reqs[j]; }
            }
            if *gset >= 0 && !g.crafted { set_counts[*gset as usize] += 1; }
        }
        for j in 0..5 {
            if fx.weapon.reqs[j] > sp_fixed_max_req[j] { sp_fixed_max_req[j] = fx.weapon.reqs[j]; }
        }

        Search {
            fx, n_free, l_max, ring1_depth, ring2_depth, rings_contiguous,
            sp_suffix_max_prov, sp_set_ids, sp_set_reach, sp_bound_base: [0; 5],
            pc_suffix, hp_suffix, subtree, subtree_prefix,
            suffix_max_rank, ring_pair_count,
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
            sp_bound_off: std::env::var("SP_BOUND_OFF").as_deref() == Ok("1"),
            checked: 0.0, leaf_calls: 0, precheck_reject: 0.0, precheck_pass: 0,
            feasible: 0, sp_leaf_reject: 0, sp_kernel_reject: 0,
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
            leaf_budget: None,
            time_cap: std::env::var("ENUM_TIME_CAP_SECS").ok().and_then(|v| v.parse().ok()),
            dense_work: Default::default(),
            thresh_reject: 0,
            cluster_evals: 0,
            cluster_memo_hits: 0,
            adapt_super: AdaptiveBound::new(),
            adapt_tail: AdaptiveBound::new(),
            bound_work: Default::default(),
            checked_flushed: 0.0,
            progress: None,
            progress_every: (1u64 << 21) as f64,
            next_progress: f64::INFINITY,
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
            cutoff = Some(self.top_n[14].score);
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
        let (Some(sc), Some(bt)) = (self.scoring, self.bound_tables) else { return false };
        let h_child = hi_rem - offset as i64;
        let mut key = (depth as u64) << 60;
        key |= (h_child.clamp(0, 2047) as u64) & 0x7FF;
        for d in 0..depth {
            key |= (self.prefix_offsets[d] as u64) << (11 + d * 7);
        }
        key |= (offset as u64) << (11 + depth * 7);
        let ceiling = match self.bound_memo.get(&key) {
            Some(&c) => { if crate::scoring::trace::fine() { crate::scoring::trace::add(crate::scoring::trace::BM_HIT, 1); } c }
            None => {
                if crate::scoring::trace::fine() { crate::scoring::trace::add(crate::scoring::trace::BM_MISS, 1); }
                let slot = &self.fx.slots[depth];
                let mut names = self.equip_names;
                names[slot.pos] = &slot.item_names[offset];
                let dense_c = match (sc.dense.as_ref(), self.dense_bound) {
                    (Some(d), Some(db)) => crate::scoring::dense_subtree_ceiling(
                        d, db, depth + 1, h_child, &names, &mut self.bound_work,
                        &sc.rows, &sc.compiled_rows, &sc.tables),
                    _ => None,
                };
                let c = match dense_c {
                    Some(c) => c,
                    None => sc.layer2.subtree_ceiling(
                        &names, bt, depth + 1, &sc.weapon, &sc.rows, &sc.registry,
                        &sc.hit_refs, &sc.tables, &sc.objective, Some(&sc.compiled_rows),
                    ).expect("bound eval error"),
                };
                self.bound_memo.insert(key, c);
                c
            }
        };
        if std::env::var("BOUND_DEBUG").as_deref() == Ok("1") {
            use std::sync::atomic::AtomicU64 as A;
            static N: A = A::new(0);
            if N.fetch_add(1, Ordering::Relaxed) < 30 {
                eprintln!("bound_debug: depth {} ceiling {:.4e} cutoff {:.4e}", depth, ceiling, cutoff);
            }
        }
        ceiling < cutoff - cutoff.abs() * 1e-9
    }

    /// Initialize equip names: none-item names per position, overridden by
    /// fixed items. Free-slot names are set/cleared by place()/unplace().
    pub fn init_equip_names(&mut self) {
        if self.fx.none_names.len() == 8 {
            for p in 0..8 { self.equip_names[p] = &self.fx.none_names[p]; }
        }
        for (pos, name) in &self.fx.fixed_names {
            self.equip_names[*pos] = name;
        }
    }

    /// Progress line with rate + ETA, every ~5 seconds (time check amortized
    /// over 65k credit/leaf events so Instant::now() stays off the hot path).
    fn maybe_report(&mut self) {
        // Checked first and unmasked: a browser chunk of a few thousand
        // leaves must actually stop there, and the masked path below only
        // fires every 65536 events.
        if let Some(budget) = self.leaf_budget {
            if self.checked >= budget { self.stop = true; return; }
        }
        if self.checked >= self.next_progress {
            self.next_progress = self.checked + self.progress_every;
            self.emit_progress();
        }
        self.report_calls += 1;
        if self.report_calls & 0xFFFF != 0 { return; }
        if let Some(f) = self.stop_flag {
            if f.load(Ordering::Relaxed) != 0 { self.stop = true; }
        } else if let Some(cap) = self.time_cap {
            // Single-thread mode has no monitor thread; honor the cap here.
            if self.started.elapsed().as_secs_f64() >= cap { self.stop = true; }
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
        if elapsed < self.next_report { return; }
        self.next_report = elapsed + 5.0;
        let rate = self.checked / elapsed;
        let remaining = (self.total_space - self.checked).max(0.0);
        eprintln!(
            "progress: {:.2}% | checked {:.3e}/{:.3e} | {:.2e} checked/s | elapsed {:.0}s | eta {:.0}s",
            self.checked / self.total_space * 100.0,
            self.checked, self.total_space, rate, elapsed, remaining / rate,
        );
    }

    /// Publishes a funnel snapshot to the progress sink, if one is attached.
    fn emit_progress(&mut self) {
        let snap = ProgressSnapshot {
            checked: self.checked,
            leaf_calls: self.leaf_calls,
            total: self.total_space,
            precheck_reject: self.precheck_reject,
            precheck_pass: self.precheck_pass,
            sp_leaf_reject: self.sp_leaf_reject,
            sp_kernel_reject: self.sp_kernel_reject,
            feasible: self.feasible,
            scored: self.scored,
            gated: self.gated,
            mana_reject: self.mana_reject,
            thresh_reject: self.thresh_reject,
            bound_pruned: self.bound_pruned,
            top_n: self.top_n.clone(),
        };
        // The sink may hand back the best score any OTHER partition has
        // reached (browser workers share it through a SharedArrayBuffer).
        // Folding it into this partition's cutoff is exactly what the native
        // threaded path does with `shared_cutoff`, and it is admissible for
        // the same reason: a score another partition has already achieved is
        // a valid lower bound on the global top-N threshold, so a leaf whose
        // ceiling cannot reach it cannot enter the merged top-N either.
        let feedback = match self.progress.as_mut() { Some(f) => f(snap), None => None };
        if let (Some(v), Some(shared)) = (feedback, self.shared_cutoff) {
            if v.is_finite() && v > 0.0 {
                shared.fetch_max(v.floor() as u64, Ordering::Relaxed);
            }
        }
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
        if !self.rings_contiguous { return; }
        let d = self.ring2_depth as usize;
        let ub = self.fx.slots[d].pool.len() - 1;
        let lb = ring1_offset;
        let tail = self.subtree[d + 1].clone();
        let l_max = self.l_max;
        let mut prefix = vec![0f64; l_max + 2];
        for l in 0..=l_max { prefix[l + 1] = prefix[l] + tail[l]; }
        {
            let row = &mut self.subtree[d];
            for l in 0..=l_max { row[l] = 0.0; }
            if lb <= ub {
                for l in 0..=l_max {
                    let lo = l.saturating_sub(ub);
                    if l < lb { continue; }
                    let hi_incl = l - lb;
                    if hi_incl < lo { continue; }
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
        if hi < 0 { return 0.0; }
        let lo_c = lo.max(0) as usize;
        let hi_c = (hi as usize).min(self.l_max);
        if lo_c > hi_c { return 0.0; }
        let p = &self.subtree_prefix[d];
        p[hi_c + 1] - p[lo_c]
    }

    /// SP bound for placing pool[offset] at `depth` — identical outcome to
    /// placing and running sp_mid_tree_feasible / sp_leaf_feasible.
    /// `is_leaf` is retained for call-site symmetry with `restr_bound_ok`;
    /// the suffix row for the last slot already encodes the empty suffix.
    /// Recompute the per-node constant part of the provision estimate.
    ///
    /// Called once on entering slot `depth`, before its offsets are scanned.
    /// Everything gathered here is fixed across those offsets: the fixed and
    /// already-placed provisions, the item suffix, and the set-granted skill
    /// points still reachable.
    ///
    /// Reachability is what makes the set term worth having. A set can only
    /// contribute at piece counts it can still attain: at least what is already
    /// worn, at most that plus the free slots left to fill. Deep in the tree,
    /// where most nodes are, a three-piece bonus the prefix never started is
    /// out of reach and contributes nothing -- which is the difference between
    /// a bound that pays for itself and one that does not.
    fn refresh_sp_bound_base(&mut self, depth: usize) {
        let sfx = self.sp_suffix_max_prov[depth + 1];
        let mut base = [0i32; 5];
        for j in 0..5 {
            base[j] = self.sp_fixed_prov[j] + self.sp_free_prov[j] + sfx[j];
        }
        let stride = self.n_free + 1;
        for (si, &sid) in self.sp_set_ids.iter().enumerate() {
            let rows = &self.fx.set_table[sid as usize];
            if rows.is_empty() { continue; }
            let worn = self.set_counts[sid as usize].max(0) as usize;
            let reach = self.sp_set_reach[si * stride + depth] as usize;
            // Wearing more pieces than the table has rows keeps the top row
            // (`evaluate_leaf` clamps the same way), so clamp both ends rather
            // than letting the range invert and contribute nothing -- that
            // silently made the bound inadmissible again, two builds short on
            // fam_hybrid_small.
            let lo = worn.max(1).min(rows.len());
            let hi = rows.len().min(worn + reach).max(lo);
            let mut best = [0i32; 5];
            for t in lo..=hi {
                let row = rows[t - 1];
                for j in 0..5 {
                    if row[j] > best[j] { best[j] = row[j]; }
                }
            }
            for j in 0..5 { base[j] += best[j]; }
        }
        self.sp_bound_base = base;
    }

    fn sp_bound_ok(&self, depth: usize, offset: usize, _is_leaf: bool) -> bool {
        // Measurement oracle. Skipping the bound entirely is trivially
        // admissible, so `feasible` under SP_BOUND_OFF=1 is the true count and
        // the gap against the default run is what the bound currently loses.
        if self.sp_bound_off { return true; }
        let it = &self.fx.slots[depth].pool[offset];
        let mut total_deficit = 0i32;
        for j in 0..5 {
            let own = if !it.crafted && it.skp[j] > 0 { it.skp[j] } else { 0 };
            let m = (it.reqs[j] + own).max(self.sp_max_req[j]);
            if m == 0 { continue; }
            let prov = self.sp_bound_base[j] + own;
            if m <= prov { continue; }
            let deficit = m - prov;
            if deficit > SP_PER_ATTR_CAP { return false; }
            total_deficit += deficit;
            if total_deficit > self.fx.budget { return false; }
        }
        true
    }

    /// Restriction/EHP bound for placing pool[offset] at `depth`.
    fn restr_bound_ok(&self, depth: usize, offset: usize, is_leaf: bool) -> bool {
        let it = &self.fx.slots[depth].pool[offset];
        let n_pc = self.fx.pc_thresholds.len();
        for i in 0..n_pc {
            let sfx = if is_leaf { 0.0 } else { self.pc_suffix[(depth + 1) * n_pc + i] };
            if self.pc_running[i] + it.pc[i] + sfx < self.fx.pc_thresholds[i] { return false; }
        }
        if self.fx.ehp.is_some() || self.fx.ehpna.is_some() || self.fx.thp.is_some() {
            let sfx = if is_leaf { 0.0 } else { self.hp_suffix[depth + 1] };
            if !self.hp_gates_ok(self.hp_running + it.hp + sfx) { return false; }
        }
        true
    }

    fn sp_mid_tree_feasible(&self, next_depth: usize) -> bool {
        if next_depth >= self.n_free { return true; }
        let mut total_deficit = 0i32;
        for j in 0..5 {
            if self.sp_max_req[j] == 0 { continue; }
            let prov = self.sp_fixed_prov[j] + self.sp_free_prov[j]
                + self.sp_suffix_max_prov[next_depth][j];
            if self.sp_max_req[j] <= prov { continue; }
            let deficit = self.sp_max_req[j] - prov;
            if deficit > SP_PER_ATTR_CAP { return false; }
            total_deficit += deficit;
            if total_deficit > self.fx.budget { return false; }
        }
        true
    }

    fn sp_leaf_feasible(&self) -> bool {
        let mut total_deficit = 0i32;
        for j in 0..5 {
            if self.sp_max_req[j] == 0 { continue; }
            let prov = self.sp_fixed_prov[j] + self.sp_free_prov[j];
            if self.sp_max_req[j] <= prov { continue; }
            let deficit = self.sp_max_req[j] - prov;
            if deficit > SP_PER_ATTR_CAP { return false; }
            total_deficit += deficit;
            if total_deficit > self.fx.budget { return false; }
        }
        true
    }

    fn hp_gates_ok(&self, raw_hp: f64) -> bool {
        if let Some((thr, fixed_hp, div)) = self.fx.ehp {
            let mut total = raw_hp + fixed_hp;
            if total < 5.0 { total = 5.0; }
            if total / div < thr { return false; }
        }
        if let Some((thr, fixed_hp, div)) = self.fx.ehpna {
            let mut total = raw_hp + fixed_hp;
            if total < 5.0 { total = 5.0; }
            if total / div < thr { return false; }
        }
        if let Some((thr, fixed_hp)) = self.fx.thp {
            let mut total = raw_hp + fixed_hp;
            if total < 5.0 { total = 5.0; }
            if total < thr { return false; }
        }
        true
    }

    fn restr_mid_tree_feasible(&self, next_depth: usize) -> bool {
        let n_pc = self.fx.pc_thresholds.len();
        for i in 0..n_pc {
            if self.pc_running[i] + self.pc_suffix[next_depth * n_pc + i]
                < self.fx.pc_thresholds[i] { return false; }
        }
        self.hp_gates_ok(self.hp_running + self.hp_suffix[next_depth])
    }

    fn evaluate_leaf(&mut self) {
        self.checked += 1.0;
        self.leaf_calls += 1;
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
            if cnt <= 0 { continue; }
            let rows = &self.fx.set_table[sid];
            let idx = (cnt as usize).min(rows.len());
            if idx == 0 { continue; }
            let row = rows[idx - 1];
            for j in 0..5 { set_free[j] += row[j]; }
        }
        // Scored path (P2.4 layer 3): full leaf pipeline with ceiling gate.
        if let Some(sc) = self.scoring {
            let names: [&str; 8] = self.equip_names;
            // Gate cutoff: local 15th-best exact score, or the shared
            // floored cutoff — whichever is higher.
            let mut cutoff: Option<f64> = None;
            if self.top_n.len() >= 15 {
                cutoff = Some(self.top_n[14].score);
            }
            if let Some(shared) = self.shared_cutoff {
                let s = shared.load(Ordering::Relaxed);
                if s > 0 && (s as f64) > cutoff.unwrap_or(f64::NEG_INFINITY) {
                    cutoff = Some(s as f64);
                }
            }
            use crate::scoring::LeafOutcome;
            let _pipe_t0 = if crate::scoring::trace::on() {
                Some(Instant::now()) } else { None };
            let (outcome, tome_choice) = crate::scoring::leaf_pipeline_tome(
                &names, &sc.layer2, &sc.weapon, sc.guild_unit.as_ref(),
                &mut self.kernel, &sc.rows, &sc.registry, &sc.hit_refs,
                &sc.tables, &sc.consts, &sc.objective, Some(&sc.compiled_rows), cutoff,
                sc.dense.as_ref().map(|d| (d, &mut self.dense_work)),
                &sc.thresholds, &sc.spell_base_costs,
            ).expect("scoring pipeline error");
            if let Some(t0) = _pipe_t0 {
                crate::scoring::trace::add(
                    crate::scoring::trace::PIPE, t0.elapsed().as_nanos() as u64);
            }
            match outcome {
                LeafOutcome::SpInfeasible => { self.sp_kernel_reject += 1; }
                LeafOutcome::Gated => { self.feasible += 1; self.gated += 1; }
                LeafOutcome::ManaReject => { self.feasible += 1; self.mana_reject += 1; }
                LeafOutcome::ThresholdReject => { self.feasible += 1; self.thresh_reject += 1; }
                LeafOutcome::Scored(r) => {
                    self.feasible += 1;
                    self.scored += 1;
                    let pos = self.top_n.iter().position(|x| r.score > x.score)
                        .unwrap_or(self.top_n.len());
                    if pos < 15 {
                        let names_owned = names.iter().map(|s| s.to_string()).collect();
                        self.top_n.insert(pos, TopEntry {
                            score: r.score, items: names_owned,
                            base_sp: r.base_sp, total_sp: r.total_sp,
                            assigned_sp: r.assigned_sp,
                            tome: tome_choice,
                        });
                        self.top_n.truncate(15);
                        if self.top_n.len() == 15 {
                            if let Some(shared) = self.shared_cutoff {
                                let floored = self.top_n[14].score.floor();
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
        if self.kernel.calculate_with_extra(&case, guild_unit.as_ref()).is_some() {
            self.feasible += 1;
        }
    }

    fn place(&mut self, depth: usize, item_idx: usize) {
        let slot = &self.fx.slots[depth];
        let it = slot.pool[item_idx].clone();
        let n_pc = it.pc.len();
        for i in 0..n_pc { self.pc_running[i] += it.pc[i]; }
        self.hp_running += it.hp;
        if !it.crafted {
            for j in 0..5 {
                if it.skp[j] > 0 { self.sp_free_prov[j] += it.skp[j]; }
            }
            if it.set_id >= 0 { self.set_counts[it.set_id as usize] += 1; }
        }
        self.sp_max_save[depth] = self.sp_max_req;
        for j in 0..5 {
            // Own-exclusion: an item can never use its own skill points to
            // meet its own requirements — in ANY equip order, the points it
            // grants arrive only after its requirement check. Tracking
            // req + own_skp⁺ (instead of raw req) keeps every deficit test
            // below exact for the best case (this item equipped last, using
            // everyone else's points but not its own) and therefore
            // admissible, while strictly tighter for skill-point sticks.
            // On spellsteal-family pools this is the difference between the
            // ~40ns bound and a ~3.5µs exact-kernel call for millions of
            // leaves whose requirements only look meetable because the item
            // was counting its own points.
            let own = if !it.crafted && it.skp[j] > 0 { it.skp[j] } else { 0 };
            let v = it.reqs[j] + own;
            if v > self.sp_max_req[j] { self.sp_max_req[j] = v; }
        }
        self.equips[slot.pos] = Unit { crafted: it.crafted, reqs: it.reqs, skp: it.skp };
        self.equip_set[slot.pos] = it.set_id;
        if !slot.item_names.is_empty() {
            self.equip_names[slot.pos] = &slot.item_names[item_idx];
        }
        if it.illegal_id >= 0 { self.illegal_counts[it.illegal_id as usize] += 1; }
    }

    fn unplace(&mut self, depth: usize, item_idx: usize) {
        let slot = &self.fx.slots[depth];
        let it = slot.pool[item_idx].clone();
        let n_pc = it.pc.len();
        for i in 0..n_pc { self.pc_running[i] -= it.pc[i]; }
        self.hp_running -= it.hp;
        if !it.crafted {
            for j in 0..5 {
                if it.skp[j] > 0 { self.sp_free_prov[j] -= it.skp[j]; }
            }
            if it.set_id >= 0 { self.set_counts[it.set_id as usize] -= 1; }
        }
        self.sp_max_req = self.sp_max_save[depth];
        self.equips[slot.pos] = Unit::default();
        self.equip_set[slot.pos] = -1;
        if !slot.item_names.is_empty() && self.fx.none_names.len() == 8 {
            self.equip_names[slot.pos] = &self.fx.none_names[slot.pos];
        }
        if it.illegal_id >= 0 { self.illegal_counts[it.illegal_id as usize] -= 1; }
    }

    fn blocks(&self, illegal_id: i32) -> bool {
        illegal_id >= 0 && self.illegal_counts[illegal_id as usize] > 0
    }

    // Visit every completion whose remaining rank sum lies in [lo_rem, hi_rem].
    fn enumerate(&mut self, depth: usize, lo_rem: i64, hi_rem: i64) {
        if self.stop { return; }
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
        } else { 0 };

        // Hoist everything `sp_bound_ok` needs that does not vary with the
        // offset. Recursion restores `sp_free_prov` and `set_counts` on the way
        // back up, so this stays valid for the whole offset scan below.
        self.refresh_sp_bound_base(depth);

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
                            if db.super_size > 0
                                && self.adapt_super.armed(self.checked)
                                && crate::scoring::env_once!("SUPER_CLUSTER" != "0") {
                                let sci = o / db.super_size;
                                let mut skey = 0xEu64 << 60;
                                skey |= (sci as u64) << 49;
                                for dd in 0..depth {
                                    skey |= (self.prefix_offsets[dd] as u64) << (dd * 7);
                                }
                                let sceiling = match self.bound_memo.get(&skey) {
                                    Some(&v) => { self.cluster_memo_hits += 1; v }
                                    None => {
                                        self.cluster_evals += 1;
                                        if prefix_state == 0 {
                                            prefix_state = match sc.dense.as_ref().and_then(|d| d.direct.as_ref().map(|dd| (d, dd))) {
                                                Some((d, dd)) => {
                                                    if self.bound_work.leaf.fill_direct(d, dd, &self.equip_names) { 1 } else { -1 }
                                                }
                                                None => -1,
                                            };
                                        }
                                        let bt0 = bound_timer();
                                        let v = if prefix_state == 1 {
                                            let d = sc.dense.as_ref().unwrap();
                                            crate::scoring::dense_ceiling_cached(
                                                d, &mut self.bound_work,
                                                &db.super_clusters[sci], &db.super_cluster_terms[sci],
                                                &sc.rows, &sc.compiled_rows, &sc.tables)
                                        } else { f64::INFINITY };
                                        bound_timer_end(bt0);
                                        if self.bound_memo.len() >= BOUND_MEMO_CAP {
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
                                    self.adapt_super.record(skipped, self.checked);
                                    self.maybe_report();
                                    offset = end + 1;
                                    continue;
                                }
                                self.adapt_super.record(0.0, self.checked);
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
                                    if crate::scoring::trace::fine() { crate::scoring::trace::add(crate::scoring::trace::BM_HIT, 1); }
                                    v
                                }
                                None => {
                                    self.cluster_evals += 1;
                                    if crate::scoring::trace::fine() { crate::scoring::trace::add(crate::scoring::trace::BM_MISS, 1); }
                                    if prefix_state == 0 {
                                        prefix_state = match sc.dense.as_ref().and_then(|d| d.direct.as_ref().map(|dd| (d, dd))) {
                                            Some((d, dd)) => {
                                                if self.bound_work.leaf.fill_direct(d, dd, &self.equip_names) { 1 } else { -1 }
                                            }
                                            None => -1,
                                        };
                                    }
                                    let bt0 = bound_timer();
                                    let v = if prefix_state == 1 {
                                        let d = sc.dense.as_ref().unwrap();
                                        crate::scoring::dense_ceiling_cached(
                                            d, &mut self.bound_work,
                                            &db.last_clusters[c], &db.last_cluster_terms[c],
                                            &sc.rows, &sc.compiled_rows, &sc.tables)
                                    } else { f64::INFINITY };
                                    bound_timer_end(bt0);
                                    // Bound the memo's memory: recent
                                    // prefixes dominate hits, so a periodic
                                    // clear costs little and caps growth.
                                    if self.bound_memo.len() >= BOUND_MEMO_CAP {
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
            let tail_here = self.bound_tail > 0
                && depth + 1 < self.n_free
                && self.n_free - (depth + 1) <= self.bound_tail
                && self.adapt_tail.armed(self.checked);
            let bound_here = depth < self.bound_max_depth || tail_here;
            if bound_here && self.bound_tables.is_some() {
                if let Some(cutoff) = self.cutoff() {
                    if self.bound_prunes(depth, o, cutoff, hi_rem) {
                        if slot_is_ring1 && self.rings_contiguous {
                            self.rebuild_ring2_subtree(o);
                        }
                        let pruned = self.band_credit(depth + 1, lo_rem - offset, hi_rem - offset);
                        self.checked += pruned;
                        self.bound_pruned += pruned;
                        if tail_here { self.adapt_tail.record(pruned, self.checked); }
                        self.maybe_report();
                        offset += 1;
                        continue;
                    }
                    if tail_here { self.adapt_tail.record(0.0, self.checked); }
                }
            }
            self.place(depth, o);
            self.prefix_offsets[depth] = o as u8;
            if slot_is_ring1 {
                self.ring1_placed_offset = o;
                if self.rings_contiguous { self.rebuild_ring2_subtree(o); }
            }
            // The child refreshes the hoist for its own depth; restore ours
            // before the next offset is tested against it.
            let saved_base = self.sp_bound_base;
            self.enumerate(depth + 1, lo_rem - offset, hi_rem - offset);
            self.sp_bound_base = saved_base;
            self.unplace(depth, o);
            offset += 1;
        }
    }

    /// Total canonical search size (sum of root subtree counts by level).
    pub fn total_space_of(&self) -> f64 {
        let mut total = 0.0;
        for l in 0..=self.l_max { total += self.subtree[0][l]; }
        total.max(1.0)
    }

    pub fn run(&mut self) {
        // Total canonical space = sum over L of the root subtree counts.
        let mut total = 0.0;
        for l in 0..=self.l_max { total += self.subtree[0][l]; }
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

/// A mid-search funnel snapshot handed to a progress sink.
#[derive(Clone)]
pub struct ProgressSnapshot {
    pub checked: f64,
    /// Builds actually handed to `evaluate_leaf`. `checked` credits whole
    /// pruned subtrees it never visited, so the two diverge by orders of
    /// magnitude once the bounds engage -- and only this one is a count of
    /// work done.
    pub leaf_calls: u64,
    pub total: f64,
    pub precheck_reject: f64,
    pub precheck_pass: u64,
    pub sp_leaf_reject: u64,
    pub sp_kernel_reject: u64,
    pub feasible: u64,
    pub scored: u64,
    pub gated: u64,
    pub mana_reject: u64,
    pub thresh_reject: u64,
    pub bound_pruned: f64,
    pub top_n: Vec<TopEntry>,
}

#[derive(Default)]
pub struct Totals {
    pub checked: f64,
    pub leaf_calls: u64,
    pub precheck_reject: f64,
    pub precheck_pass: u64,
    pub sp_leaf_reject: u64,
    pub sp_kernel_reject: u64,
    pub feasible: u64,
    pub scored: u64,
    pub gated: u64,
    pub mana_reject: u64,
    pub thresh_reject: u64,
    pub bound_pruned: f64,
    /// True when the search stopped on a budget/cap rather than exhausting
    /// the space.
    pub stopped_early: bool,
    pub top_n: Vec<TopEntry>,
}

/// One entry of the merged top-N.
///
/// Carries the SP assignment the greedy chose alongside the score. The
/// browser installs that as the build's skill points when a result is
/// loaded, so dropping it means the UI shows a zero allocation whose
/// recomputed stats do not match the score the build was ranked by.
#[derive(Clone, Debug, Default)]
pub struct TopEntry {
    pub score: f64,
    pub items: Vec<String>,
    pub base_sp: [i32; 5],
    pub total_sp: [i32; 5],
    pub assigned_sp: i32,
    /// Which tome the leaf loop chose, when tome optimisation is on. `None`
    /// leaves the UI's existing fixed-tome display untouched.
    pub tome: Option<crate::scoring::TomeChoice>,
}


/// `"tome":{...}` for a result, or empty when tome optimisation is off.
/// Emitted only when a choice exists so the UI's fixed-tome display is
/// untouched on every pre-tome scenario.
fn tome_json(t: &Option<crate::scoring::TomeChoice>) -> String {
    let Some(c) = t else { return String::new() };
    let names = |v: &Vec<String>| -> String {
        let mut out = String::from("[");
        for (i, n) in v.iter().enumerate() {
            if i > 0 { out.push(','); }
            out.push_str(&serde_json::to_string(n).unwrap_or_else(|_| "\"\"".into()));
        }
        out.push(']');
        out
    };
    format!(",\"tome\":{{\"guild_idx\":{},\"weaponTome\":{},\"armorTome\":{}}}",
            c.guild_idx, names(&c.weapon_names), names(&c.armor_names))
}

pub fn merge_top(into: &mut Vec<TopEntry>, from: Vec<TopEntry>) {
    for e in from {
        let pos = into.iter().position(|x| e.score > x.score).unwrap_or(into.len());
        if pos < 15 {
            into.insert(pos, e);
            into.truncate(15);
        }
    }
}

/// Run one single-threaded search over a parsed fixture. Shared by the CLI
/// (its 1-thread path) and the WASM entry point, so browser results come
/// from exactly the same engine as native ones.
pub fn run_single(
    fx: &Fixture,
    scoring: Option<&crate::scoring::ScoringCtx>,
    leaf_budget: Option<f64>,
) -> Totals {
    run_single_with_progress(fx, scoring, leaf_budget, None, None)
}

/// `run_single` with an optional live-progress sink. The sink fires every
/// ~2M credited leaves and at the end of the search, so a browser worker can
/// publish funnel counters and interim top-N while the solve runs.
pub fn run_single_with_progress(
    fx: &Fixture,
    scoring: Option<&crate::scoring::ScoringCtx>,
    leaf_budget: Option<f64>,
    progress: Option<&mut dyn FnMut(ProgressSnapshot) -> Option<f64>>,
    // `part`: inclusive first-slot offset range, or None for all of it.
    // Ranges partition the space exactly — the same split the native
    // threaded path work-steals over — so several single-threaded runs can
    // cover the search between them and their integral counters sum to the
    // whole-space totals.
    part: Option<(i64, i64)>,
) -> Totals {
    let shared_cutoff = AtomicU64::new(0);
    let bound_tables = scoring.and_then(|sc| {
        // Dynamic rows: the all-150-SP ceiling assumes the damage rows are
        // fixed, and they are not — so the mid-tree bound is inadmissible.
        // The mid-tree ceiling machinery evaluates one assembled state, so
        // it cannot express a two-sided bound; leave it off there. The leaf
        // gate handles those objectives on its own.
        if !sc.objective.supports_ceiling() || !sc.layer2.ceiling_vars_ok
            || sc.consts.hp_casting || sc.consts.dynamic.is_some()
            || sc.objective.needs_two_sided_ceiling() {
            return None;
        }
        if !fx.slots.iter().all(|s| s.pool.len() < 128) { return None; }
        let pools: Vec<Vec<String>> = fx.slots.iter().map(|s| s.item_names.clone()).collect();
        sc.layer2.build_bound_tables(&pools).ok()
    });
    let dense_bound = match (scoring, bound_tables.as_ref()) {
        (Some(sc), Some(_)) => sc.dense.as_ref().and_then(|d| {
            let pools: Vec<Vec<String>> = fx.slots.iter().map(|s| s.item_names.clone()).collect();
            crate::scoring::DenseBound::build(&sc.layer2, d, &pools, 4)
        }),
        _ => None,
    };
    // Seed the cutoff before enumerating. This used to run only in the CLI,
    // so the browser engine began every search with a cold cutoff and the
    // gate pruned nothing until one turned up organically. It matters more
    // still when partitioned: each partition would otherwise have to
    // rediscover a good cutoff over its own slice of the space.
    let warm_k: usize = std::env::var("WARM_K").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let bound_cluster: usize = std::env::var("BOUND_CLUSTER").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(4);
    seed_warm_cutoff(fx, scoring, &shared_cutoff, warm_k, bound_cluster, false);

    let mut search = Search::new(fx);
    search.scoring = scoring;
    search.shared_cutoff = Some(&shared_cutoff);
    search.bound_tables = bound_tables.as_ref();
    search.bound_max_depth = 0;
    search.bound_tail = 1;
    search.dense_bound = dense_bound.as_ref();
    search.leaf_budget = leaf_budget;
    search.next_report = f64::INFINITY;
    if let Some((lo, hi)) = part {
        search.part_lo = lo;
        search.part_hi = hi;
    }
    if let Some(p) = progress {
        // Reborrow so the sink's lifetime shrinks to the Search's rather
        // than forcing `'a` out to the caller's (which would outlive the
        // bound tables and cutoff declared above).
        search.progress = Some(&mut *p);
        search.next_progress = search.progress_every;
    }
    search.init_equip_names();
    search.run();
    // Final snapshot so the UI's last frame matches the returned totals.
    search.emit_progress();
    Totals {
        checked: search.checked,
        leaf_calls: search.leaf_calls,
        precheck_reject: search.precheck_reject,
        precheck_pass: search.precheck_pass,
        sp_leaf_reject: search.sp_leaf_reject, sp_kernel_reject: search.sp_kernel_reject,
        feasible: search.feasible,
        scored: search.scored,
        gated: search.gated,
        mana_reject: search.mana_reject,
        thresh_reject: search.thresh_reject,
        bound_pruned: search.bound_pruned,
        stopped_early: search.stop,
        top_n: search.top_n,
    }
}

/// Solve a scenario from in-memory fixture payloads and return JSON. This
/// is the browser entry point's engine (see `wasm_api`), exposed natively
/// too so the same code path is covered by native tests.
///
/// `max_leaves <= 0` runs to completion; otherwise the search stops once
/// that many leaves are credited (a deterministic alternative to a wall
/// clock, which wasm lacks).
pub fn solve_json(enum_fixture: &str, score_fixture: &str, max_leaves: f64) -> String {
    solve_json_full(enum_fixture, score_fixture, max_leaves, None, 0, 1)
}

/// Splits the first slot's pool into `part_count` contiguous offset ranges
/// and returns the inclusive bounds of `part_index`.
///
/// A worker whose range is empty (more workers than offsets) gets `lo > hi`
/// and enumerates nothing, which is correct rather than an error.
pub fn partition_bounds(pool_len: usize, part_index: usize, part_count: usize) -> (i64, i64) {
    if part_count <= 1 || pool_len == 0 {
        return (0, i64::MAX);
    }
    let n = pool_len as i64;
    let count = part_count as i64;
    let idx = part_index as i64;
    let base = n / count;
    let rem = n % count;
    // The first `rem` partitions take one extra offset.
    let lo = idx * base + idx.min(rem);
    let hi = lo + base + if idx < rem { 1 } else { 0 } - 1;
    (lo, hi)
}

/// Serializes a `ProgressSnapshot` for a JS sink.
pub fn progress_json(p: &ProgressSnapshot) -> String {
    let mut top = String::from("[");
    for (i, e) in p.top_n.iter().enumerate() {
        let (score, items) = (&e.score, &e.items);
        if i > 0 { top.push(','); }
        top.push_str(&format!("{{\"score\":{:.17e},\"item_names\":[", score));
        for (j, name) in items.iter().enumerate() {
            if j > 0 { top.push(','); }
            top.push_str(&json_str(name));
        }
        // Interim rows carry the SP assignment too, so a result shown mid-run
        // is not a zeroed placeholder that only becomes real when it finishes.
        let sp = |a: &[i32; 5]| format!("[{},{},{},{},{}]", a[0], a[1], a[2], a[3], a[4]);
        top.push_str(&format!("],\"base_sp\":{},\"total_sp\":{},\"assigned_sp\":{}{}}}",
                              sp(&e.base_sp), sp(&e.total_sp), e.assigned_sp,
                              tome_json(&e.tome)));
    }
    top.push(']');
    format!(
        "{{\"checked\":{:.0},\"total\":{:.0},\"precheck_reject\":{:.0},\"precheck_pass\":{},\
         \"sp_leaf_reject\":{},\"feasible\":{},\"scored\":{},\"gated\":{},\"mana_reject\":{},\
         \"thresh_reject\":{},\"bound_pruned\":{:.0},\"top_n\":{}}}",
        p.checked, p.total, p.precheck_reject, p.precheck_pass, p.sp_leaf_reject,
        p.feasible, p.scored, p.gated, p.mana_reject, p.thresh_reject, p.bound_pruned, top,
    )
}

/// `solve_json` with an optional live-progress sink (see
/// `run_single_with_progress`).
pub fn solve_json_with_progress(
    enum_fixture: &str, score_fixture: &str, max_leaves: f64,
    progress: Option<&mut dyn FnMut(ProgressSnapshot) -> Option<f64>>,
) -> String {
    solve_json_full(enum_fixture, score_fixture, max_leaves, progress, 0, 1)
}

/// `solve_json` with a progress sink and a partition assignment.
///
/// `part_count > 1` runs only this partition's share of the space, so a host
/// can spawn several single-threaded engines (one per browser worker) and
/// cover the search between them. Each partition reports the FULL space as
/// `total` so a host summing `checked` across workers gets a coherent
/// percentage.
pub fn solve_json_full(
    enum_fixture: &str, score_fixture: &str, max_leaves: f64,
    progress: Option<&mut dyn FnMut(ProgressSnapshot) -> Option<f64>>,
    part_index: usize, part_count: usize,
) -> String {
    let fx = parse_fixture(enum_fixture);
    let budget = if max_leaves > 0.0 { Some(max_leaves) } else { None };
    let ctx = if score_fixture.trim().is_empty() {
        None
    } else {
        match serde_json::from_str::<serde_json::Value>(score_fixture)
            .map_err(|e| e.to_string())
            .and_then(|v| crate::scoring::ScoringCtx::load(&v))
        {
            Ok(c) => Some(c),
            Err(e) => return format!("{{\"error\":{}}}", json_str(&e)),
        }
    };
    let part = if part_count > 1 {
        let pool_len = fx.slots.first().map(|s| s.pool.len()).unwrap_or(0);
        Some(partition_bounds(pool_len, part_index, part_count))
    } else { None };
    let totals = run_single_with_progress(&fx, ctx.as_ref(), budget, progress, part);
    let complete = !totals.stopped_early;
    let mut top = String::from("[");
    for (i, e) in totals.top_n.iter().enumerate() {
        let (score, items) = (&e.score, &e.items);
        if i > 0 { top.push(','); }
        top.push_str(&format!("{{\"score\":{:.17e},\"items\":[", score));
        for (j, name) in items.iter().enumerate() {
            if j > 0 { top.push(','); }
            top.push_str(&json_str(name));
        }
        // The SP assignment the greedy chose. The browser installs this as
        // the build's skill points; without it the UI would show a zeroed
        // allocation whose stats contradict the score.
        let sp = |a: &[i32; 5]| format!("[{},{},{},{},{}]", a[0], a[1], a[2], a[3], a[4]);
        top.push_str(&format!("],\"base_sp\":{},\"total_sp\":{},\"assigned_sp\":{}{}}}",
                              sp(&e.base_sp), sp(&e.total_sp), e.assigned_sp,
                              tome_json(&e.tome)));
    }
    top.push(']');
    format!(
        "{{\"checked\":{},\"feasible\":{},\"scored\":{},\"gated\":{},\
         \"mana_reject\":{},\"thresh_reject\":{},\"bound_pruned\":{},\
         \"complete\":{},\"top\":{}}}",
        totals.checked, totals.feasible, totals.scored, totals.gated,
        totals.mana_reject, totals.thresh_reject, totals.bound_pruned,
        complete, top,
    )
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Warm start: solve the elite subspace (top-WARM_K tail of each
/// level-ordered pool) first, seeding `shared_cutoff`.
///
/// Its top-15 are real scored builds, so the seeded cutoff is admissible for
/// the main run — the gate and cluster bound prune hard from the first node
/// instead of waiting for the cutoff to warm up. Ranking is a heuristic;
/// admissibility comes from the builds being real, not from the selection.
/// `WARM_K=0` disables.
///
/// Shared by the CLI and `run_single`, so the browser path gets it too — it
/// previously lived only in `cli_main`, which meant the WASM engine started
/// every search with a cold cutoff.
fn seed_warm_cutoff(
    fx: &Fixture, scoring: Option<&crate::scoring::ScoringCtx>,
    shared_cutoff: &AtomicU64, warm_k: usize, bound_cluster: usize, verbose: bool,
) {
if !(scoring.is_some() && warm_k > 0 && fx.slots.iter().any(|s| s.pool.len() > warm_k)) {
    return;
}
{
    // Rank each pool's items by their solo objective ceiling (item alone
    // on a none-item build at all-150 SP) and keep the top WARM_K per
    // slot, preserving level order so the band machinery stays valid.
    // Ranking is a heuristic — cutoff admissibility comes from the warm
    // builds being real scored builds, not from the selection.
    let sc = scoring.unwrap();
    let mut base_names: [&str; 8] = Default::default();
    if fx.none_names.len() == 8 {
        for p in 0..8 { base_names[p] = &fx.none_names[p]; }
    }
    for (pos, name) in &fx.fixed_names { base_names[*pos] = name; }
    let mut warm_sel: Vec<Vec<usize>> = Vec::with_capacity(fx.slots.len());
    {
        let mut work = crate::scoring::DenseWork::default();
        for sl in &fx.slots {
            let mut ranked: Vec<(usize, f64)> = sl.item_names.iter().enumerate()
                .map(|(i, name)| {
                    let mut names = base_names;
                    names[sl.pos] = name.as_str();
                    let c = sc.dense.as_ref().and_then(|d| {
                        crate::scoring::dense_ceiling_with(
                            d, &[], &[], &names, &mut work,
                            &sc.rows, &sc.compiled_rows, &sc.tables)
                    }).unwrap_or(f64::NEG_INFINITY);
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
        slots: fx.slots.iter().zip(&warm_sel).map(|(sl, sel)| Slot {
            name: sl.name.clone(),
            pos: sl.pos,
            is_ring1: sl.is_ring1,
            is_ring2: sl.is_ring2,
            pool: sel.iter().map(|&i| sl.pool[i].clone()).collect(),
            item_names: sel.iter().map(|&i| sl.item_names[i].clone()).collect(),
        }).collect(),
        set_table: fx.set_table.clone(),
        fixed_names: fx.fixed_names.clone(),
        none_names: fx.none_names.clone(),
    };
    let warm_started = Instant::now();
    let wpools: Vec<Vec<String>> = wfx.slots.iter().map(|s| s.item_names.clone()).collect();
    let wdb = sc.dense.as_ref().and_then(|d| {
        crate::scoring::DenseBound::build(&sc.layer2, d, &wpools, bound_cluster)
    });
    let mut ws = Search::new(&wfx);
    ws.scoring = scoring;
    ws.shared_cutoff = Some(&shared_cutoff);
    ws.dense_bound = wdb.as_ref();
    ws.init_equip_names();
    ws.next_report = f64::INFINITY;
    ws.run();
    if verbose {
        eprintln!(
            "warm: {} leaves ({} scored) in {:.2}s | cutoff seeded {:.6e}",
            ws.checked, ws.scored, warm_started.elapsed().as_secs_f64(),
            shared_cutoff.load(Ordering::Relaxed) as f64,
        );
    }
    let _ = warm_started;
}


}

/// CLI entry point (thin wrapper lives in src/bin/enum_kernel.rs).
pub fn cli_main() {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args.get(1).map(String::as_str)
        .expect("usage: enum_kernel <fixture> [threads] [score_fixture.json]");
    let text = fs::read_to_string(fixture_path).expect("cannot read fixture");
    let fx = parse_fixture(&text);

    let n_threads: usize = args.get(2)
        .map(|s| s.parse().expect("threads must be a number"))
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1));

    // Fixed-work mode: stop after a set number of credited leaves instead of
    // after a set wall time. A/B runs then compare time-to-same-work rather
    // than work-in-same-time, which removes the machine-noise term that a
    // time-capped comparison folds into the result.
    let cli_leaf_budget: Option<f64> = match env::var("ENUM_LEAF_BUDGET") {
        Ok(raw) => match raw.trim().parse::<f64>() {
            Ok(v) if v.is_finite() && v > 0.0 => Some(v),
            _ => {
                eprintln!("enum_kernel: error: ENUM_LEAF_BUDGET must be a finite number > 0");
                std::process::exit(2);
            }
        },
        Err(_) => None,
    };
    // Each worker holds its own `checked`, so a per-worker budget caps
    // n_threads * budget aggregate work — not the same experiment. Refuse
    // rather than silently measure something else.
    if cli_leaf_budget.is_some() && n_threads > 1 {
        eprintln!("enum_kernel: error: ENUM_LEAF_BUDGET requires one thread \
                   (pass 1 as the threads argument)");
        std::process::exit(2);
    }

    // Optional scoring context (P2.4 layer 3): full leaf pipeline + top-N.
    let scoring_ctx: Option<crate::scoring::ScoringCtx> = args.get(3).map(|p| {
        let text = fs::read_to_string(p).expect("cannot read score fixture");
        let json: serde_json::Value = serde_json::from_str(&text).expect("invalid score fixture");
        let ctx = crate::scoring::ScoringCtx::load(&json).expect("scoring context");
        assert!(fx.slots.iter().all(|s| s.item_names.len() == s.pool.len()),
            "enum fixture lacks the NAMES section — re-export with the current exporter");
        assert_eq!(fx.none_names.len(), 8, "enum fixture lacks NONENAMES");
        ctx
    });
    let scoring = scoring_ctx.as_ref();
    crate::scoring::trace::init_from_env();
    let shared_cutoff = AtomicU64::new(0);

    // Mid-tree damage ceiling bound tables (objective B&B). Memo keys pack
    // offsets into 7 bits, so guard on pool sizes.
    let bound_max_depth: usize = env::var("BOUND_DEPTH").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(0);
    let bound_tail: usize = env::var("BOUND_TAIL").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(1);
    let bound_tables: Option<crate::scoring::BoundTables> = scoring.and_then(|sc| {
        let bound_cluster_on: bool = env::var("BOUND_CLUSTER").ok()
            .and_then(|s| s.parse::<usize>().ok()).unwrap_or(4) > 0;
        if bound_max_depth == 0 && bound_tail == 0 && !bound_cluster_on { return None; }
        if !fx.slots.iter().all(|s| s.pool.len() < 128) {
            eprintln!("bound: pool >= 128 items, memo packing disabled — skipping bound");
            return None;
        }
        // Same gate as `run_single_with_progress`: dynamic rows make the
        // all-150-SP ceiling meaningless, because the rows themselves change
        // per leaf. Missing this here pruned every leaf of a slider scenario
        // and scored none of them.
        if !sc.objective.supports_ceiling()
            || !sc.layer2.ceiling_vars_ok
            || sc.consts.hp_casting
            || sc.consts.dynamic.is_some()
            // See run_single_with_progress: one assembled state cannot
            // express a two-sided bound.
            || sc.objective.needs_two_sided_ceiling() { return None; }
        let slot_pools: Vec<Vec<String>> = fx.slots.iter().map(|s| s.item_names.clone()).collect();
        Some(sc.layer2.build_bound_tables(&slot_pools).expect("bound tables"))
    });
    let bounds = bound_tables.as_ref();
    let bound_cluster: usize = env::var("BOUND_CLUSTER").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(4);
    let dense_bound: Option<crate::scoring::DenseBound> = match (scoring, bounds) {
        (Some(sc), Some(bt)) => sc.dense.as_ref().and_then(|d| {
            let slot_pools: Vec<Vec<String>> = fx.slots.iter().map(|s| s.item_names.clone()).collect();
            crate::scoring::DenseBound::build(&sc.layer2, d, &slot_pools, bound_cluster)
        }),
        _ => None,
    };
    let dense_bound = dense_bound.as_ref();

    // Warm start: solve the elite subspace (top-WARM_K tail of each
    // level-ordered pool) first, sharing the cutoff atomic. Its top-15 are
    // real builds, so the seeded cutoff is admissible for the main run —
    // the gate and cluster bound prune hard from the first node instead of
    // waiting for the cutoff to warm up. WARM_K=0 disables.
    let warm_k: usize = env::var("WARM_K").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(6);
    seed_warm_cutoff(&fx, scoring, &shared_cutoff, warm_k, bound_cluster, true);

    let start = Instant::now();

    let (totals, elapsed) = if n_threads <= 1 || fx.slots.is_empty() {
        let mut search = Search::new(&fx);
        search.scoring = scoring;
        search.shared_cutoff = Some(&shared_cutoff);
        search.bound_tables = bounds;
        search.bound_max_depth = bound_max_depth;
        search.bound_tail = bound_tail;
        search.dense_bound = dense_bound;
        search.leaf_budget = cli_leaf_budget;
        search.init_equip_names();
        search.run();
        let elapsed = start.elapsed();
        (Totals {
            checked: search.checked,
            leaf_calls: search.leaf_calls,
            precheck_reject: search.precheck_reject,
            precheck_pass: search.precheck_pass,
            sp_leaf_reject: search.sp_leaf_reject, sp_kernel_reject: search.sp_kernel_reject,
            feasible: search.feasible,
            scored: search.scored,
            gated: search.gated,
            mana_reject: search.mana_reject,
                thresh_reject: search.thresh_reject,
            bound_pruned: search.bound_pruned,
            stopped_early: search.stop,
            top_n: search.top_n,
        }, elapsed)
    } else {
        // Work-stealing over first-slot offsets: each claim runs the full
        // band sweep restricted to one offset — the same 'slot' partition
        // shape the JS engine uses, so per-offset subspaces are disjoint and
        // the integral counters sum exactly.
        let first_pool_len = fx.slots[0].pool.len();
        let next_offset = AtomicUsize::new(0);
        let shared_checked = AtomicU64::new(0);
        let stop_flag = AtomicU64::new(0);
        let time_cap: Option<f64> = std::env::var("ENUM_TIME_CAP_SECS").ok()
            .and_then(|v| v.parse().ok());
        let done = AtomicU64::new(0);

        // Full-space total for the monitor line.
        let total_space = {
            let s = Search::new(&fx);
            let mut total = 0.0;
            for l in 0..=s.l_max { total += s.subtree[0][l]; }
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
                        if search.stop { break; }
                        let o = next_offset.fetch_add(1, Ordering::Relaxed);
                        if o >= first_pool_len { break; }
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
                        eprintln!("cluster_stats: evals {} | memo_hits {} | memo_len {}",
                                  search.cluster_evals, search.cluster_memo_hits, search.bound_memo.len());
                    }
                    Totals {
                        checked: search.checked,
                        leaf_calls: search.leaf_calls,
                        precheck_reject: search.precheck_reject,
                        precheck_pass: search.precheck_pass,
                        sp_leaf_reject: search.sp_leaf_reject, sp_kernel_reject: search.sp_kernel_reject,
                        feasible: search.feasible,
                        scored: search.scored,
                        gated: search.gated,
                        mana_reject: search.mana_reject,
                thresh_reject: search.thresh_reject,
                        bound_pruned: search.bound_pruned,
                        stopped_early: search.stop,
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
                totals.sp_kernel_reject += t.sp_kernel_reject;
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
        "enum_kernel: checked {} | precheck_reject {} | precheck_pass {} | sp_leaf_reject {} | sp_kernel_reject {} | feasible {} | threads {} | elapsed {:.3}s | {:.0} checked/s | leaf_calls {} | {:.0} leaf_calls/s",
        totals.checked, totals.precheck_reject, totals.precheck_pass,
        totals.sp_leaf_reject, totals.sp_kernel_reject, totals.feasible,
        if fx.slots.is_empty() { 1 } else { n_threads.min(fx.slots[0].pool.len()).max(1) },
        elapsed.as_secs_f64(),
        totals.checked / elapsed.as_secs_f64(),
        totals.leaf_calls,
        totals.leaf_calls as f64 / elapsed.as_secs_f64(),
    );
    crate::scoring::trace::report();
    if scoring.is_some() {
        println!(
            "scoring: scored {} | gated {} | mana_reject {} | thresh_reject {} | bound_pruned {}",
            totals.scored, totals.gated, totals.mana_reject, totals.thresh_reject, totals.bound_pruned,
        );
        for e in &totals.top_n {
            let (score, names) = (&e.score, &e.items);
            println!("top15: {:.17e} | {}", score,
                names.iter().filter(|n| !n.starts_with("No ")).cloned()
                    .collect::<Vec<_>>().join(", "));
        }
    }
}
