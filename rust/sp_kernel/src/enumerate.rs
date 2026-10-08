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
use crate::bound_memo::{BoundMemo, CeilingKind};
use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use crate::clock::Instant;

pub mod anytime;

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

/// Anytime trace (roadmap R8): with ANYTIME_TRACE=1 the native CLI prints a
/// timestamped line whenever the best score found so far, or the shared
/// pruning cutoff, improves. `anytime.py` turns these into the primal
/// integral and time-to-target. Off unless asked for, and inert on wasm32,
/// whose clock does not run.
pub mod anytime_trace {
    use std::sync::{Mutex, OnceLock};
    use crate::clock::Instant;

    static T0: OnceLock<Instant> = OnceLock::new();
    static BEST: Mutex<f64> = Mutex::new(f64::NEG_INFINITY);

    #[inline]
    pub fn on() -> bool {
        !cfg!(target_arch = "wasm32") && crate::scoring::env_once!("ANYTIME_TRACE" == "1")
    }
    /// Pin t = 0 (the CLI calls this first thing; otherwise the first event).
    pub fn start() { let _ = T0.get_or_init(Instant::now); }
    fn t() -> f64 { T0.get_or_init(Instant::now).elapsed().as_secs_f64() }

    /// A real scored build reached `score`. Prints only global improvements;
    /// the lock is taken only when a thread's own best improves, which is rare.
    pub fn best(score: f64) {
        let mut b = BEST.lock().unwrap_or_else(|e| e.into_inner());
        if score > *b {
            *b = score;
            eprintln!("anytime: t={:.6} best={:.17e}", t(), score);
        }
    }
    /// The shared (floored) cutoff rose from `prev` to `now`.
    pub fn cutoff(prev: u64, now: u64) {
        if now > prev { eprintln!("anytime: t={:.6} cutoff={}", t(), now); }
    }
}

/// BOUND_OBSERVE=1: measure how loose the tail ceiling is. For every
/// subtree under the last two slots, compute the ceiling without pruning on
/// it and record it beside the best score any leaf in that subtree really
/// reached. Run with the other prunes that hide scored leaves off
/// (BOUND_CLUSTER=0 SCORE_CEILING_GATE=0), or the subtree's best is
/// understated. The CLI prints quantiles of ceiling / true best and how many
/// subtrees a perfect bound would prune at the final cutoff that this one
/// does not. Diagnostic only: slow, and never on by default.
///
/// BOUND_OBSERVE_SLOTS=k (default 1) moves the observation up so that k
/// slots are relaxed under the ceiling. The single-item and at-SP
/// diagnostics are one-slot measures and are recorded only for k = 1.
pub mod bound_observe {
    use std::sync::{Mutex, OnceLock};
    /// Number of relaxed slots under an observed ceiling (>= 1).
    pub fn slots() -> usize {
        static V: OnceLock<usize> = OnceLock::new();
        *V.get_or_init(|| std::env::var("BOUND_OBSERVE_SLOTS").ok()
            .and_then(|v| v.parse::<usize>().ok()).filter(|&k| k >= 1).unwrap_or(1))
    }
    static PAIRS: Mutex<Vec<(f64, f64)>> = Mutex::new(Vec::new());
    static AT_SP: Mutex<Vec<f64>> = Mutex::new(Vec::new());
    static FULL: Mutex<Vec<f64>> = Mutex::new(Vec::new());
    static SINGLE: Mutex<Vec<f64>> = Mutex::new(Vec::new());
    /// R2 tangent diagnostics: (tangent, min(tangent, ceiling)) / true best.
    static TAN: Mutex<Vec<(f64, f64)>> = Mutex::new(Vec::new());
    /// U(x) / f(x) at the best leaf's own items (must be >= 1).
    static ENV: Mutex<Vec<f64>> = Mutex::new(Vec::new());
    /// [refused: objective, use_max, negative, forbidden, no dense; tangent
    /// below the true best; envelope below the exact value]
    static TAN_COUNTS: Mutex<[u64; 7]> = Mutex::new([0; 7]);
    pub fn tan_count(k: usize) { TAN_COUNTS.lock().unwrap_or_else(|e| e.into_inner())[k] += 1; }
    static REASONS: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());
    /// Refusal detail (which factor, which forbidden key).
    pub fn tan_reason(r: String) {
        let mut v = REASONS.lock().unwrap_or_else(|e| e.into_inner());
        match v.iter_mut().find(|(k, _)| *k == r) { Some((_, c)) => *c += 1, None => v.push((r, 1)) }
    }
    pub fn record_tangent(tangent: f64, ceiling: f64, best: f64) {
        if !(best.is_finite() && best > 0.0 && tangent.is_finite()) { return; }
        if tangent < best * (1.0 - 1e-9) { tan_count(5); }
        TAN.lock().unwrap_or_else(|e| e.into_inner()).push((tangent / best, tangent.min(ceiling) / best));
    }
    pub fn record_envelope(u: f64, f: f64) {
        if !(f.is_finite() && f > 0.0 && u.is_finite()) { return; }
        if u < f * (1.0 - 1e-9) { tan_count(6); }
        ENV.lock().unwrap_or_else(|e| e.into_inner()).push(u / f);
    }
    pub fn record_single(single: Option<f64>, best: f64) {
        if let (Some(v), true) = (single, best.is_finite() && best > 0.0) {
            SINGLE.lock().unwrap_or_else(|e| e.into_inner()).push(v / best);
        }
    }
    #[inline]
    pub fn on() -> bool {
        static V: OnceLock<bool> = OnceLock::new();
        *V.get_or_init(|| std::env::var("BOUND_OBSERVE").as_deref() == Ok("1"))
    }
    pub fn record(ceiling: f64, best: f64, at_best_sp: Option<f64>, full_build: Option<f64>) {
        if best.is_finite() && best > 0.0 && ceiling.is_finite() {
            if let Some(f) = full_build.filter(|f| f.is_finite()) {
                FULL.lock().unwrap_or_else(|e| e.into_inner()).push(f / best);
            }
            PAIRS.lock().unwrap_or_else(|e| e.into_inner()).push((ceiling, best));
            if let Some(a) = at_best_sp.filter(|a| a.is_finite()) {
                AT_SP.lock().unwrap_or_else(|e| e.into_inner()).push(a / best);
            }
        }
    }
    /// Summary line for the CLI (None when nothing was recorded).
    pub fn report(final_cutoff: Option<f64>) -> Option<String> {
        let pairs = PAIRS.lock().unwrap_or_else(|e| e.into_inner());
        if pairs.is_empty() { return None; }
        let mut r: Vec<f64> = pairs.iter().map(|(c, b)| c / b).collect();
        r.sort_by(|a, b| a.total_cmp(b));
        let q = |p: f64| r[((r.len() - 1) as f64 * p) as usize];
        let frac = |t: f64| r.iter().filter(|&&x| x >= t).count() as f64 / r.len() as f64;
        let mut line = format!(
            "bound_observe: {} subtrees | ceiling/true best p10 {:.3} p50 {:.3} p90 {:.3} max {:.3} | >=1.05x {:.1}% >=1.2x {:.1}% >=1.5x {:.1}%",
            r.len(), q(0.1), q(0.5), q(0.9), r[r.len() - 1],
            100.0 * frac(1.05), 100.0 * frac(1.2), 100.0 * frac(1.5));
        if let Some(cut) = final_cutoff {
            let missed = pairs.iter().filter(|(c, b)| *b < cut && *c >= cut).count();
            let pruned = pairs.iter().filter(|(c, _)| *c < cut).count();
            line += &format!(" | at the final cutoff: current prunes {} ({:.1}%), a perfect bound would also prune {} more ({:.1}%)",
                pruned, 100.0 * pruned as f64 / pairs.len() as f64,
                missed, 100.0 * missed as f64 / pairs.len() as f64);
        }
        let mut a = AT_SP.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !a.is_empty() {
            a.sort_by(|x, y| x.total_cmp(y));
            let qa = |p: f64| a[((a.len() - 1) as f64 * p) as usize];
            line += &format!(" | item relaxation alone (ceiling at the best leaf's SP)/true best p10 {:.3} p50 {:.3} p90 {:.3}",
                qa(0.1), qa(0.5), qa(0.9));
        }
        let mut g = SINGLE.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !g.is_empty() {
            g.sort_by(|x, y| x.total_cmp(y));
            let qg = |p: f64| g[((g.len() - 1) as f64 * p) as usize];
            line += &format!(" | best single item, feasibility ignored, at the best leaf's SP/true best p10 {:.3} p50 {:.3} p90 {:.3}",
                qg(0.1), qg(0.5), qg(0.9));
        }
        let tan = TAN.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let counts = *TAN_COUNTS.lock().unwrap_or_else(|e| e.into_inner());
        if !tan.is_empty() || counts.iter().any(|&c| c > 0) {
            let q = |mut v: Vec<f64>, p: f64| -> f64 {
                if v.is_empty() { return f64::NAN; }
                v.sort_by(|a, b| a.total_cmp(b));
                v[((v.len() - 1) as f64 * p) as usize]
            };
            let t: Vec<f64> = tan.iter().map(|x| x.0).collect();
            let m: Vec<f64> = tan.iter().map(|x| x.1).collect();
            let env = ENV.lock().unwrap_or_else(|e| e.into_inner()).clone();
            line += &format!(
                " | R2 tangent/true best p10 {:.3} p50 {:.3} p90 {:.3} on {} subtrees, min(tangent, ceiling) p50 {:.3}; \
                 refused objective {} use_max {} negative {} forbidden {} no_dense {}; \
                 tangent below true best {}; envelope U/f p50 {:.4} max {:.4}, below exact {}",
                q(t.clone(), 0.1), q(t.clone(), 0.5), q(t.clone(), 0.9), t.len(), q(m, 0.5),
                counts[0], counts[1], counts[2], counts[3], counts[4], counts[5],
                q(env.clone(), 0.5), q(env, 1.0), counts[6]);
            let mut rs = REASONS.lock().unwrap_or_else(|e| e.into_inner()).clone();
            rs.sort_by(|a, b| b.1.cmp(&a.1));
            let top: Vec<String> = rs.iter().take(8).map(|(k, c)| format!("{k}={c}")).collect();
            if !top.is_empty() { line += &format!(" | refusals: {}", top.join(" ")); }
        }
        let mut f = FULL.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !f.is_empty() {
            f.sort_by(|x, y| x.total_cmp(y));
            let qf = |p: f64| f[((f.len() - 1) as f64 * p) as usize];
            line += &format!(" | no relaxation (ceiling of the best build itself)/true best p10 {:.3} p50 {:.3} p90 {:.3}",
                qf(0.1), qf(0.5), qf(0.9));
        }
        Some(line)
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
    /// The weapon's set id, or -1. A non-crafted weapon is a set piece and
    /// counts toward its set like any equipment (older fixtures omit the
    /// field and read as -1).
    weapon_set: i32,
    guild: Option<(Unit, i32)>,     // unit, set_id
    fixed: Vec<(usize, Unit, i32, i32)>, // pos, unit, set_id, illegal_id
    slots: Vec<Slot>,
    set_table: Vec<Vec<[i32; 5]>>,  // set_id -> bonuses per count (count-1 indexed)
    fixed_names: Vec<(usize, String)>,   // pos, display name (NAMES section)
    none_names: Vec<String>,             // 8 none-item names by slot position
    /// R21 search tolerance from an optional `EPS <value>` line (0 = exact).
    /// A search option rather than scenario data, carried here so the browser
    /// can set it through the fixture it already sends.
    pub eps: f64,
    /// R20 windowed archive: keep every build within `window` (a fraction)
    /// of the best, up to `archive_cap` entries, from optional `WINDOW <x>`
    /// and `ARCHIVE <n>` lines. 0 = the usual top-N.
    pub window: f64,
    pub archive_cap: usize,
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
    let weapon_set: i32 = wt.get(11).map(|v| v.parse().unwrap()).unwrap_or(-1);

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
    let mut eps = 0.0f64;
    let mut window = 0.0f64;
    let mut archive_cap = DEFAULT_ARCHIVE_CAP;
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
            "WINDOW" => {
                window = t.get(1).and_then(|v| v.parse::<f64>().ok())
                    .filter(|w| w.is_finite() && *w > 0.0 && *w < 1.0).unwrap_or(0.0);
            }
            "ARCHIVE" => {
                archive_cap = t.get(1).and_then(|v| v.parse::<usize>().ok())
                    .filter(|&n| n > 0).unwrap_or(DEFAULT_ARCHIVE_CAP);
            }
            "EPS" => {
                eps = t.get(1).and_then(|v| v.parse::<f64>().ok())
                    .filter(|e| e.is_finite() && *e > 0.0).unwrap_or(0.0);
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
              weapon, weapon_set, guild, fixed, slots, set_table, fixed_names, none_names, eps,
              window, archive_cap }
}

/// Default R20 archive capacity. Beyond it the window's completeness claim is
/// withdrawn rather than the archive growing without bound.
pub const DEFAULT_ARCHIVE_CAP: usize = 2000;

/// The result capacity a search over `fx` uses: the archive cap when an R20
/// window is set, the configured top-N otherwise.
pub fn effective_result_count(fx: &Fixture, options: &SearchOptions) -> usize {
    if fx.window > 0.0 { fx.archive_cap } else { options.result_count }
}

/// R20: whether an archive holds every build within the window. `complete`
/// is whether the search exhausted its space; `top` is the final archive,
/// best first. Sound when nothing in the final window was evicted or
/// pruned: an evicted entry scored at most the archive's last score at the
/// time, which only rises, and a pruned subtree's ceiling was below
/// max(last score, window line) at the time, both at most their final
/// values. So it holds when the archive is not full, or its last entry is
/// below the final window line.
pub fn window_complete(complete: bool, top: &[TopEntry], cap: usize, window: f64) -> bool {
    if !complete || window <= 0.0 { return false; }
    let Some(best) = top.first().map(|e| e.score) else { return true };
    top.len() < cap || top[cap - 1].score < best * (1.0 - window)
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
    /// Real evaluated-leaf budget, unlike credited subtree space.
    pub actual_leaf_budget: Option<u64>,
    /// Optional deadline epoch shared by preparation, warm search and repairs.
    global_started: Option<Instant>,
    /// SP_BOUND_OFF=1: read once at construction, never in the hot path.
    sp_bound_off: bool,
    /// R9 node bound; SP_NODE_BOUND=0 turns it off.
    sp_node_on: bool,
    /// Per slot depth: the largest positive provision per lane among the
    /// pool's non-crafted items (R1 at the last-slot cluster bounds).
    last_pool_max_skp: Vec<[i32; 5]>,
    /// R1 at the cluster bounds: off under SCORE_REACH_SP=0 or when a crafted
    /// item is anywhere in the search. Decided once at construction.
    reach_cap_on: bool,
    /// Last-slot ranges R9 rejected with one solve.
    pub sp_node_reject: u64,
    /// Per slot, per set id: the sorted pool offsets of that slot's items in
    /// the set (any item with that set id, as R9 always counted). For R9.
    slot_set_offsets: Vec<Vec<Vec<u32>>>,
    /// Per slot: the set ids its pool stocks.
    slot_set_ids: Vec<Vec<u32>>,
    /// BOUND_OBSERVE=1 diagnostic: best real score seen inside the subtree
    /// currently being observed (see `bound_observe`).
    observe_max: f64,
    /// total_sp of the leaf that set `observe_max`.
    observe_sp: [i32; 5],
    observe_names: [&'a str; 8],
    /// R9 ran early for the current last-slot node (from its parent, before
    /// the tail ceiling; see `r9_early`) and passed, so the node skips it.
    r9_done: bool,
    /// Ceiling evaluations the early R9 check avoided (diagnostics).
    pub r9_early_skips: u64,
    dense_work: crate::scoring::DenseWork,
    /// Separate buffers for bound evals so the leaf pipeline can't clobber
    /// the cached last-slot prefix state.
    bound_work: crate::scoring::DenseWork,
    checked_flushed: f64,

    /// Optional live-progress sink (browser UI). Called every
    /// `progress_every` credited leaves with a funnel snapshot plus the
    /// current top-N, so a long solve shows movement instead of looking
    /// hung. Exact mode retains deterministic credited-leaf emission points.
    progress: Option<&'a mut dyn FnMut(ProgressSnapshot) -> Option<f64>>,
    progress_every: f64,
    next_progress: f64,
    /// Anytime UI needs its first retained witness before a long repair ends.
    /// Kept off for exact search so its callbacks and work remain unchanged.
    progress_on_first_result: bool,

    // Scoring integration (P2.4 layer 3): current equip names by position,
    // the scenario scoring context, per-thread top-N, and the shared cutoff
    // (floor(score) as u64; 0 = unset — floor is admissible for the gate).
    equip_names: [&'a str; 8],
    scoring: Option<&'a crate::scoring::ScoringCtx>,
    /// Root-domain ring ordering for reduced neighbourhoods. Their two local
    /// pools can have different indices, so local ring flags must be off.
    /// Only attach when both rings were free in the original search.
    original_ring_order: Option<&'a std::collections::HashMap<String, usize>>,
    top_n: Vec<TopEntry>,
    result_count: usize,
    quality_trace: Option<Arc<QualityTrace>>,
    trace_phase: &'static str,
    shared_cutoff: Option<&'a AtomicU64>,
    /// R21: the best score any thread has found, as f64 bits (fetch_max on
    /// the bits orders non-negative floats correctly; only positive scores
    /// are published). Read only when `eps > 0`.
    shared_best: Option<&'a AtomicU64>,
    /// R21 tolerance. With eps > 0 a subtree is pruned once its ceiling is
    /// below (1 + eps) * best, so the reported top-1 is within eps of the
    /// optimum and ranks 2 to 15 are unverified. 0 (the default) is the exact
    /// search, unchanged. SEARCH_EPS sets it on the CLI.
    eps: f64,
    /// R20 window (0 = off). When set, `eps` is ignored: the exact window's
    /// prune line is (1 - window) * best, and a tolerance on top of it would
    /// drop builds inside the window.
    window: f64,
    scored: u64,
    gated: u64,
    mana_reject: u64,
    thresh_reject: u64,
    cluster_evals: u64,
    cluster_memo_hits: u64,
    adapt_super: AdaptiveBound,
    adapt_tail: AdaptiveBound,
    adapt_node: AdaptiveBound,

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
    /// Ceiling memo keyed by the complete prefix and (for subtrees) band
    /// budget. Small pools use packed keys; wider pools use structured keys.
    bound_memo: BoundMemo,
    /// Current prefix offsets (by depth); never truncate wide-pool offsets.
    prefix_offsets: [usize; 8],
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
        // The weapon is a worn set piece (calculate_skillpoints counts it).
        if fx.weapon_set >= 0 { set_counts[fx.weapon_set as usize] += 1; }

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
            sp_node_on: std::env::var("SP_NODE_BOUND").as_deref() != Ok("0"),
            reach_cap_on: std::env::var("SCORE_REACH_SP").as_deref() != Ok("0")
                && !fx.fixed.iter().any(|(_, u, _, _)| u.crafted)
                && !fx.slots.iter().any(|sl| sl.pool.iter().any(|it| it.crafted))
                && !fx.guild.as_ref().is_some_and(|(g, _)| g.crafted),
            last_pool_max_skp: fx.slots.iter().map(|sl| {
                let mut m = [0i32; 5];
                for it in &sl.pool {
                    if it.crafted { continue; }
                    for j in 0..5 { if it.skp[j] > m[j] { m[j] = it.skp[j]; } }
                }
                m
            }).collect(),
            sp_node_reject: 0,
            r9_done: false,
            slot_set_offsets: fx.slots.iter().map(|sl| {
                let mut v = vec![Vec::new(); fx.set_table.len()];
                for (o, it) in sl.pool.iter().enumerate() {
                    if it.set_id >= 0 && (it.set_id as usize) < v.len() {
                        v[it.set_id as usize].push(o as u32);
                    }
                }
                v
            }).collect(),
            slot_set_ids: fx.slots.iter().map(|sl| {
                let mut ids: Vec<u32> = sl.pool.iter()
                    .filter(|it| it.set_id >= 0 && (it.set_id as usize) < fx.set_table.len())
                    .map(|it| it.set_id as u32).collect();
                ids.sort_unstable();
                ids.dedup();
                ids
            }).collect(),
            observe_max: f64::NEG_INFINITY,
            observe_sp: [0; 5],
            observe_names: Default::default(),
            r9_early_skips: 0,
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
            actual_leaf_budget: None,
            global_started: None,
            time_cap: std::env::var("ENUM_TIME_CAP_SECS").ok().and_then(|v| v.parse().ok()),
            dense_work: Default::default(),
            thresh_reject: 0,
            cluster_evals: 0,
            cluster_memo_hits: 0,
            adapt_super: AdaptiveBound::new(),
            adapt_node: AdaptiveBound::new(),
            adapt_tail: AdaptiveBound::new(),
            bound_work: Default::default(),
            checked_flushed: 0.0,
            progress: None,
            progress_every: (1u64 << 21) as f64,
            next_progress: f64::INFINITY,
            progress_on_first_result: false,
            equip_names: Default::default(),
            scoring: None,
            original_ring_order: None,
            top_n: Vec::new(),
            result_count: SearchOptions::from_env().result_count,
            quality_trace: None,
            trace_phase: "enumerate",
            shared_cutoff: None,
            shared_best: None,
            // SEARCH_EPS (CLI) overrides the fixture's EPS line; "0" forces exact.
            eps: match std::env::var("SEARCH_EPS").ok().and_then(|v| v.parse::<f64>().ok()) {
                Some(e) if e.is_finite() && e > 0.0 => e,
                Some(_) => 0.0,
                None => fx.eps,
            },
            window: fx.window,
            scored: 0,
            gated: 0,
            mana_reject: 0,
            bound_tables: None,
            bound_max_depth: 2,
            bound_tail: 0,
            dense_bound: None,
            bound_pruned: 0.0,
            bound_memo: BoundMemo::new(fx.slots.iter().any(|s| s.pool.len() >= 128)),
            prefix_offsets: [0; 8],
        }
    }

    /// Current gate/bound cutoff: local 15th-best exact score or the shared
    /// floored cutoff, whichever is higher. None until either exists.
    fn cutoff(&self) -> Option<f64> {
        let mut cutoff: Option<f64> = None;
        if self.top_n.len() >= self.result_count {
            cutoff = Some(self.top_n[self.result_count - 1].score);
        }
        if let Some(shared) = self.shared_cutoff {
            let s = shared.load(Ordering::Relaxed);
            if s > 0 && (s as f64) > cutoff.unwrap_or(f64::NEG_INFINITY) {
                cutoff = Some(s as f64);
            }
        }
        if self.window > 0.0 || self.eps > 0.0 {
            // R21: (1 + eps) * best; R20: (1 - window) * best, which takes
            // precedence. Defined only for a positive best (a real build's
            // score), so the line is admissible like the 15th-best cutoff.
            let mut best = self.top_n.first().map_or(f64::NEG_INFINITY, |e| e.score);
            if let Some(sb) = self.shared_best {
                let b = f64::from_bits(sb.load(Ordering::Relaxed));
                if b > best { best = b; }
            }
            if best > 0.0 {
                let line = if self.window > 0.0 { best * (1.0 - self.window) }
                           else { best * (1.0 + self.eps) };
                cutoff = Some(cutoff.map_or(line, |c| c.max(line)));
            }
        }
        cutoff
    }

    /// Insert one distinct equipment/tome result before publishing any cutoff.
    /// Warm starts, repairs and enumeration may rediscover the same build.
    fn insert_top(&mut self, entry: TopEntry) -> bool {
        let first_result = self.top_n.is_empty();
        let score = entry.score;
        if !insert_top_n(&mut self.top_n, entry, self.result_count) { return false; }
        if let Some(trace) = &self.quality_trace {
            trace.record(self.trace_phase, &self.top_n, self.result_count);
        }
        // A new local best: report it (R8) and publish it for the R21 line.
        if self.top_n[0].score == score {
            if anytime_trace::on() { anytime_trace::best(score); }
            if let (true, Some(sb)) = ((self.eps > 0.0 || self.window > 0.0) && score > 0.0, self.shared_best) {
                sb.fetch_max(score.to_bits(), Ordering::Relaxed);
            }
        }
        if self.top_n.len() >= self.result_count {
            if let Some(shared) = self.shared_cutoff {
                let floor = self.top_n[self.result_count - 1].score.floor();
                if floor > 0.0 {
                    let prev = shared.fetch_max(floor as u64, Ordering::Relaxed);
                    if anytime_trace::on() { anytime_trace::cutoff(prev, floor as u64); }
                }
            }
        }
        if first_result && self.progress_on_first_result { self.emit_progress(); }
        true
    }

    /// Subtree ceiling for placing pool item `offset` at `depth`, memoized by
    /// the complete prefix and band budget. Returns true if it cannot beat `cutoff`.
    fn bound_prunes(&mut self, depth: usize, offset: usize, cutoff: f64, hi_rem: i64) -> bool {
        let Some(ceiling) = self.subtree_ceiling_value(depth, offset, hi_rem) else { return false };
        if crate::scoring::env_once!("BOUND_DEBUG" == "1") {
            use std::sync::atomic::AtomicU64 as A;
            static N: A = A::new(0);
            if N.fetch_add(1, Ordering::Relaxed) < 30 {
                eprintln!("bound_debug: depth {} ceiling {:.4e} cutoff {:.4e}", depth, ceiling, cutoff);
            }
        }
        ceiling < cutoff - cutoff.abs() * 1e-9
    }

    /// BOUND_OBSERVE only: the subtree ceiling with the SP lanes set to `sp`
    /// instead of the reachable caps. Not memoized, not a bound.
    fn ceiling_at_sp(&mut self, depth: usize, offset: usize, hi_rem: i64, sp: &[f64; 5]) -> Option<f64> {
        let (Some(sc), Some(db)) = (self.scoring, self.dense_bound) else { return None };
        let d = sc.dense.as_ref()?;
        let h_child = hi_rem - offset as i64;
        let slot = &self.fx.slots[depth];
        let mut names = self.equip_names;
        names[slot.pos] = &slot.item_names[offset];
        crate::scoring::dense_subtree_ceiling(
            d, db, depth + 1, h_child, &names, &mut self.bound_work,
            &sc.rows, &sc.compiled_rows, &sc.tables, sp)
    }

    /// BOUND_OBSERVE only: max over the last slot's items (offsets the
    /// ceiling covers) of the complete build's ceiling at `sp`, with the
    /// prefix placed: no super-item, and no SP or mana feasibility.
    fn best_single_item(&mut self, depth: usize, offset: usize, hi_rem: i64, sp: &[f64; 5]) -> Option<f64> {
        let sc = self.scoring?;
        let d = sc.dense.as_ref()?;
        let child = depth + 1;
        let h_child = (hi_rem - offset as i64).max(0) as usize;
        let fx: &'a Fixture = self.fx;
        let mut names = self.equip_names;
        names[fx.slots[depth].pos] = &fx.slots[depth].item_names[offset];
        let cslot = &fx.slots[child];
        let mut best = f64::NEG_INFINITY;
        for i in 0..=h_child.min(cslot.pool.len().saturating_sub(1)) {
            names[cslot.pos] = &cslot.item_names[i];
            if let Some(v) = crate::scoring::dense_ceiling_with(
                d, &[], &[], &names, &mut self.bound_work,
                &sc.rows, &sc.compiled_rows, &sc.tables, sp) {
                if v > best { best = v; }
            }
        }
        best.is_finite().then_some(best)
    }

    /// BOUND_OBSERVE only: the dense ceiling of the observed best leaf itself
    /// (all items fixed) at its own skill points; ideally its score.
    fn ceiling_of_build(&mut self, sp: &[f64; 5]) -> Option<f64> {
        let sc = self.scoring?;
        let d = sc.dense.as_ref()?;
        let names = self.observe_names;
        crate::scoring::dense_ceiling_with(
            d, &[], &[], &names, &mut self.bound_work,
            &sc.rows, &sc.compiled_rows, &sc.tables, sp)
    }

    /// BOUND_OBSERVE only: the R2 tangent bound (see `tangent`) for the
    /// subtree under (depth, offset), and the envelope check at the observed
    /// best leaf's own items. Records into `bound_observe`; never prunes.
    fn observe_tangent(&mut self, depth: usize, offset: usize, hi_rem: i64, ceiling: f64) {
        use crate::tangent::Refuse;
        let (Some(sc), Some(db)) = (self.scoring, self.dense_bound) else { bound_observe::tan_count(4); return };
        let Some(d) = sc.dense.as_ref() else { bound_observe::tan_count(4); return };
        let Some(dd) = d.direct.as_ref() else { bound_observe::tan_count(4); return };
        let fx: &'a Fixture = self.fx;
        let mut names = self.equip_names;
        names[fx.slots[depth].pos] = &fx.slots[depth].item_names[offset];
        let sp_cap = self.subtree_sp_cap(depth, offset);
        if !self.bound_work.leaf.fill_direct(d, dd, &names) { bound_observe::tan_count(4); return; }
        {
            let crate::scoring::DenseWork { leaf, scratch, .. } = &mut self.bound_work;
            scratch.reset(leaf, d);
            crate::scoring::dense_assemble(d, leaf, scratch, &sp_cap);
        }
        let atk = self.bound_work.leaf.atk_spd_idx;
        let env = match crate::tangent::build_envelope(
            d, &mut self.bound_work.scratch, &sc.rows, &sc.compiled_rows, &sc.tables, atk) {
            Ok(e) => e,
            Err(r) => {
                bound_observe::tan_count(match r {
                    Refuse::Objective => 0, Refuse::UseMax => 1, Refuse::Negative(_) => 2, Refuse::Forbidden => 3,
                });
                if let Refuse::Negative(why) = r { bound_observe::tan_reason(format!("negative:{why}")); }
                return;
            }
        };
        let forbidden = crate::tangent::forbidden_indices(d);
        let h_child = (hi_rem - offset as i64).max(0) as usize;
        let mut slots: Vec<&[Vec<(u32, f64)>]> = Vec::new();
        for j in depth + 1..self.n_free {
            let v = &db.item_vecs[j];
            if v.is_empty() { continue; }
            slots.push(&v[..=h_child.min(v.len() - 1)]);
        }
        let mut dense = vec![0.0f64; d.n];
        let (tan, _u_at_i) = match env.tangent(&slots, &forbidden, &mut dense) {
            Ok(v) => v,
            Err(i) => {
                bound_observe::tan_count(3);
                let key = d.idx.iter().find(|(_, &v)| v == i).map(|(k, _)| k.as_str()).unwrap_or("?");
                bound_observe::tan_reason(format!("forbidden:{key}"));
                return;
            }
        };
        bound_observe::record_tangent(tan, ceiling, self.observe_max);
        // Envelope check at the best leaf's own relaxed items.
        if self.observe_max.is_finite() {
            let mut x: Vec<(u32, f64)> = Vec::new();
            for j in depth + 1..self.n_free {
                let slot = &fx.slots[j];
                let name = self.observe_names[slot.pos];
                let Some(o) = slot.item_names.iter().position(|n| n.as_str() == name) else { return };
                x.extend(db.item_vecs[j][o].iter().cloned());
            }
            x.sort_by_key(|(i, _)| *i);
            let mut merged: Vec<(u32, f64)> = Vec::new();
            for (i, v) in x {
                match merged.last_mut() { Some((j, w)) if *j == i => *w += v, _ => merged.push((i, v)) }
            }
            let u = env.value(&merged, &mut dense);
            // fill_direct above left the prefix in bound_work.leaf.
            let f = crate::scoring::dense_ceiling_cached(
                d, &mut self.bound_work, &merged, &[], &sc.rows, &sc.compiled_rows, &sc.tables, &sp_cap);
            bound_observe::record_envelope(u, f);
        }
    }

    /// The memoized subtree ceiling behind `bound_prunes` (None without
    /// bound tables).
    fn subtree_ceiling_value(&mut self, depth: usize, offset: usize, hi_rem: i64) -> Option<f64> {
        let (Some(sc), Some(bt)) = (self.scoring, self.bound_tables) else { return None };
        let h_child = hi_rem - offset as i64;
        let key = self.bound_memo.key(
            CeilingKind::Subtree, depth, &self.prefix_offsets, offset, h_child);
        let ceiling = match self.bound_memo.get(&key) {
            Some(&c) => { if crate::scoring::trace::fine() { crate::scoring::trace::add(crate::scoring::trace::BM_HIT, 1); } c }
            None => {
                if crate::scoring::trace::fine() { crate::scoring::trace::add(crate::scoring::trace::BM_MISS, 1); }
                let slot = &self.fx.slots[depth];
                let mut names = self.equip_names;
                names[slot.pos] = &slot.item_names[offset];
                let sp_cap = self.subtree_sp_cap(depth, offset);
                let dense_c = match (sc.dense.as_ref(), self.dense_bound) {
                    (Some(d), Some(db)) => crate::scoring::dense_subtree_ceiling(
                        d, db, depth + 1, h_child, &names, &mut self.bound_work,
                        &sc.rows, &sc.compiled_rows, &sc.tables, &sp_cap),
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
        Some(ceiling)
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
    /// over 256 credit/leaf events so Instant::now() stays off the hot path).
    fn maybe_report(&mut self) {
        // Checked first and unmasked: a browser chunk of a few thousand
        // leaves must actually stop there, and the masked path below only
        // fires every 256 events.
        if let Some(budget) = self.leaf_budget {
            if self.checked >= budget { self.stop = true; return; }
        }
        if self.checked >= self.next_progress {
            self.next_progress = self.checked + self.progress_every;
            self.emit_progress();
        }
        if self.actual_leaf_budget.is_some_and(|budget| self.leaf_calls >= budget) {
            self.stop = true;
        }
        self.report_calls += 1;
        // Repairs need responsive cancellation; one clock read per 256 events.
        if self.report_calls & 0xFF != 0 { return; }
        if let Some(f) = self.stop_flag {
            if f.load(Ordering::Relaxed) != 0 { self.stop = true; }
        } else if let Some(cap) = self.time_cap {
            // Single-thread mode has no monitor thread; honor the cap here.
            if self.global_started.unwrap_or(self.started).elapsed().as_secs_f64() >= cap { self.stop = true; }
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
            // Interim frames show at most the top 15: an R20 archive can hold
            // thousands, and the sink serializes every frame.
            top_n: self.top_n.iter().take(15).cloned().collect(),
        };
        // The sink may hand back the best score any OTHER partition has
        // reached (browser workers share it through a SharedArrayBuffer).
        // Folding it into this partition's cutoff is exactly what the native
        // threaded path does with `shared_cutoff`, and it is admissible for
        // the same reason: a score another partition has already achieved is
        // a valid lower bound on the global top-N threshold, so a leaf whose
        // ceiling cannot reach it cannot enter the merged top-N either.
        let feedback = match self.progress.as_mut() { Some(f) => f(snap), None => None };
        // The host computes that floor from the 15th-best score. Under an R20
        // window the archive needs every build above (1 - x) * best, which
        // can sit far below the 15th best, so the floor would prune builds
        // inside the window: ignore it there.
        let feedback = if self.window > 0.0 { None } else { feedback };
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
        let l_max = self.l_max;
        // The prefix sums of the next row are `subtree_prefix[d + 1]`: built
        // the same way in `new`, and only this ring-2 row is ever rebuilt. Used
        // in place of a fresh copy and sum (two allocations per ring-1
        // placement); the values are bit-identical.
        let (head, rest) = self.subtree_prefix.split_at_mut(d + 1);
        let prefix = &rest[0];
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
        let own = &mut head[d];
        for t in 0..=l_max {
            own[t + 1] = own[t] + self.subtree[d][t];
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

    /// R9: exact SP feasibility of the placed prefix plus a RELAXED last item,
    /// for the last slot's offsets [from, to].
    ///
    /// The relaxed item has no requirements and, per lane, the largest
    /// provision any non-crafted candidate in the range carries (crafted SP is
    /// added after feasibility, so it never helps). Every real candidate has
    /// at least its requirements and at most those provisions, and SP
    /// feasibility only gets easier with more provision and fewer
    /// requirements, so an infeasible relaxation proves every candidate
    /// infeasible. Set-granted points are bounded per set by the best row any
    /// reachable piece count gives (worn now, plus one if the range stocks the
    /// set), positive parts only, SUMMED over sets: the PR #19 review's
    /// counterexample is a suffix piece with no own provision that completes a
    /// +30 Dex set, which a provision-only relaxation would wrongly reject.
    /// Skipped (returns true) when the scored path chooses among guild tome
    /// candidates, or when SP_NODE_BOUND=0.
    fn sp_node_feasible(&mut self, depth: usize, from: usize, to: usize) -> bool {
        if self.sp_bound_off || !self.sp_node_on { return true; }
        let guild: Option<crate::Unit> = match self.scoring {
            Some(sc) => {
                if !sc.layer2.guild_tome_cands.is_empty() { return true; }
                sc.guild_unit
            }
            None => self.fx.guild.as_ref().map(|(g, _)| *g),
        };
        let slot = &self.fx.slots[depth];
        let mut skp = [0i32; 5];
        let mut first = true;
        for it in &slot.pool[from..=to] {
            if it.crafted { continue; }
            for j in 0..5 {
                if first || it.skp[j] > skp[j] { skp[j] = it.skp[j]; }
            }
            first = false;
        }
        if first { skp = [0; 5]; }   // only crafted candidates: none adds SP
        // Set term. Only sets that are worn, or stocked by this slot within
        // [from, to], contribute; `slot_set_offsets` answers "stocked in the
        // range" by binary search, where this used to scan the whole range
        // once per set in the game (O(sets x range) per call, about a
        // quarter of all instructions early in a search). Integer sums, so
        // the visiting order does not change the result.
        let offs = &self.slot_set_offsets[depth];
        let in_range = |sid: usize| -> usize {
            let v = &offs[sid];
            let i = v.partition_point(|&x| (x as usize) < from);
            usize::from(i < v.len() && (v[i] as usize) <= to)
        };
        let mut set_free = [0i32; 5];
        let mut add = |rows: &Vec<[i32; 5]>, worn: usize, reach: usize| {
            let lo = worn.max(1).min(rows.len());
            let hi = rows.len().min(worn + reach).max(lo);
            for j in 0..5 {
                let mut best = 0;
                for t in lo..=hi { best = best.max(rows[t - 1][j]); }
                set_free[j] += best;
            }
        };
        for (sid, &cnt) in self.set_counts.iter().enumerate() {
            if cnt <= 0 { continue; }
            let rows = &self.fx.set_table[sid];
            if rows.is_empty() { continue; }
            add(rows, cnt as usize, in_range(sid));
        }
        for &sid in &self.slot_set_ids[depth] {
            let sid = sid as usize;
            if self.set_counts[sid] > 0 { continue; }   // counted above
            let rows = &self.fx.set_table[sid];
            if rows.is_empty() || in_range(sid) == 0 { continue; }
            add(rows, 0, 1);
        }
        let mut equipment = self.equips;
        equipment[slot.pos] = Unit { crafted: false, reqs: [0; 5], skp };
        let case = Case { budget: self.fx.budget, equipment, weapon: self.fx.weapon, set_free, expected: None };
        self.kernel.calculate_with_extra(&case, guild.as_ref()).is_some()
    }

    /// R1 at the last-slot cluster bounds: per lane, the highest total a
    /// completion of this prefix can reach. Totals are assigned points plus
    /// provisions, assigned points are at most 100 per lane (and at most the
    /// budget), and provisions are at most the positive parts of the fixed,
    /// guild, placed and weapon provisions, the best reachable set rows
    /// (sp_bound_base already sums the first four bar the weapon, and the set
    /// term) and the largest the last pool offers. Capped at 150, the input
    /// cap of the skill-point curve. All-150 when the scored path chooses
    /// among guild tome candidates (a candidate can add provisions the fixture
    /// guild does not), when any crafted item is fixed or pooled (crafted
    /// points reach the final totals but none of the provision sums used
    /// here), or when SCORE_REACH_SP=0.
    fn last_slot_sp_cap(&self, depth: usize) -> [f64; 5] {
        let off = !self.reach_cap_on || match self.scoring {
            Some(sc) => !sc.layer2.guild_tome_cands.is_empty(),
            None => true,
        };
        if off { return [150.0; 5]; }
        let pool_max = &self.last_pool_max_skp[depth];
        let assign = 100.min(self.fx.budget.max(0));
        std::array::from_fn(|j| {
            let prov = self.sp_bound_base[j] + pool_max[j] + self.fx.weapon.skp[j].max(0);
            (prov + assign).min(150) as f64
        })
    }

    /// R1 at the tail bound: the SP cap for the subtree below pool item
    /// `offset` placed at `depth`. sp_bound_base (refreshed for `depth`)
    /// already covers every later pool's largest provision and the reachable
    /// set rows; the candidate's own and the weapon's are added. Same
    /// switches as last_slot_sp_cap.
    fn subtree_sp_cap(&self, depth: usize, offset: usize) -> [f64; 5] {
        let off = !self.reach_cap_on || match self.scoring {
            Some(sc) => !sc.layer2.guild_tome_cands.is_empty(),
            None => true,
        };
        if off { return [150.0; 5]; }
        let it = &self.fx.slots[depth].pool[offset];
        let assign = 100.min(self.fx.budget.max(0));
        std::array::from_fn(|j| {
            let prov = self.sp_bound_base[j] + it.skp[j].max(0) + self.fx.weapon.skp[j].max(0);
            (prov + assign).min(150) as f64
        })
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
        // Preserve the original ordered tuple domain before the SP solver.
        // Merely deduplicating swapped rings after scoring is insufficient:
        // tied minimum-SP allocations can depend on equipment input order.
        if let Some(order) = self.original_ring_order {
            let canonical = matches!(
                (order.get(self.equip_names[4]), order.get(self.equip_names[5])),
                (Some(left), Some(right)) if left <= right
            );
            if !canonical {
                self.precheck_reject += 1.0;
                return;
            }
        }
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
            // floored cutoff, whichever is higher (and the R21 line when
            // eps > 0). Same rule as every bound, from one place.
            let cutoff = self.cutoff();
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
                    if r.score > self.observe_max {
                        self.observe_max = r.score; self.observe_sp = r.total_sp; self.observe_names = names;
                    }
                    // Allocate result strings only when the score can enter the archive.
                    if self.top_n.len() < self.result_count
                        || r.score >= self.top_n[self.result_count - 1].score {
                        self.insert_top(TopEntry {
                            score: r.score, items: names.iter().map(|s| s.to_string()).collect(),
                            base_sp: r.base_sp, total_sp: r.total_sp,
                            assigned_sp: r.assigned_sp, tome: tome_choice,
                        });
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
        // Borrow through the fixture reference itself (lifetime 'a, not tied
        // to `self`), so the item is read in place instead of cloned: the
        // clone copied its restriction Vec on every placement.
        let fx: &'a Fixture = self.fx;
        let slot = &fx.slots[depth];
        let it = &slot.pool[item_idx];
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
        let fx: &'a Fixture = self.fx;
        let slot = &fx.slots[depth];
        let it = &slot.pool[item_idx];
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
            // R9: one exact SP solve for the whole in-band range (see
            // sp_node_feasible). If even the relaxation is infeasible, no
            // candidate is, so the range is credited and skipped.
            // Self-tuning like the cluster layers: where it rarely rejects
            // (tierstack measured 10% slower with it always on), it switches
            // itself off and re-samples later. Speed only; never a result.
            let r9_done = std::mem::replace(&mut self.r9_done, false);
            if from <= to && !r9_done && self.sp_node_on && self.adapt_node.armed(self.checked) {
                let ok = self.sp_node_feasible(depth, from as usize, to as usize);
                let skipped = if ok { 0.0 } else { (to - from + 1) as f64 };
                self.adapt_node.record(skipped, self.checked);
                if !ok {
                    self.checked += skipped;
                    self.sp_leaf_reject += skipped as u64;
                    self.sp_node_reject += 1;
                    self.maybe_report();
                    return;
                }
            }
            let mut offset = from;
            // R1 at the last-slot clusters: the highest total each lane can
            // reach from this prefix (see last_slot_sp_cap).
            let node_sp_cap = self.last_slot_sp_cap(depth);
            // Cached prefix state for the cluster bound: filled at the first
            // cluster miss, reused for every cluster in this node.
            let mut prefix_state: i8 = 0; // 0 unfilled, 1 ok, -1 unavailable
            while offset <= to && !self.stop {
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
                                let skey = self.bound_memo.key(
                                    CeilingKind::SuperCluster, depth, &self.prefix_offsets, sci, 0);
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
                                                &sc.rows, &sc.compiled_rows, &sc.tables, &node_sp_cap)
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
                            let key = self.bound_memo.key(
                                CeilingKind::Cluster, depth, &self.prefix_offsets, c, 0);
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
                                            &sc.rows, &sc.compiled_rows, &sc.tables, &node_sp_cap)
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
        while offset <= max_offset && !self.stop {
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
            let bound_here = (depth < self.bound_max_depth || tail_here) && !bound_observe::on();
            let mut r9_pre_ok = false;
            // R9 before the tail ceiling. When the child is the last slot, the
            // child's first act is R9's one SP solve over its in-band range,
            // and an infeasible verdict skips the whole range. A ceiling
            // evaluation (several times the cost of the solve) made just
            // before it is then wasted, so run the solve first. Both are
            // admissible rejections of the same leaves, so results are
            // unchanged; a passing verdict is handed to the child so the solve
            // is not repeated. R9_EARLY=0 disables.
            if bound_here && depth + 2 == self.n_free && self.bound_tables.is_some()
                && self.sp_node_on && crate::scoring::env_once!("R9_EARLY" != "0")
                && self.cutoff().is_some() && self.adapt_node.armed(self.checked)
                && !self.fx.slots[depth + 1].pool.is_empty() {
                let child = depth + 1;
                let child_ring2 = child as isize == self.ring2_depth && self.ring1_depth >= 0;
                let child_min: i64 = if child_ring2 {
                    if slot_is_ring1 { offset } else { self.ring1_placed_offset as i64 }
                } else { 0 };
                let from = child_min.max(lo_rem - offset).max(0);
                let to = (self.fx.slots[child].pool.len() as i64 - 1).min(hi_rem - offset);
                if from <= to {
                    self.place(depth, o);
                    let ok = self.sp_node_feasible(child, from as usize, to as usize);
                    self.unplace(depth, o);
                    let skipped = if ok { 0.0 } else { (to - from + 1) as f64 };
                    self.adapt_node.record(skipped, self.checked);
                    if !ok {
                        if slot_is_ring1 && self.rings_contiguous { self.rebuild_ring2_subtree(o); }
                        self.checked += skipped;
                        self.sp_leaf_reject += skipped as u64;
                        self.sp_node_reject += 1;
                        self.r9_early_skips += 1;
                        self.maybe_report();
                        offset += 1;
                        continue;
                    }
                    r9_pre_ok = true;
                }
            }
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
            self.prefix_offsets[depth] = o;
            if slot_is_ring1 {
                self.ring1_placed_offset = o;
                if self.rings_contiguous { self.rebuild_ring2_subtree(o); }
            }
            // The child refreshes the hoist for its own depth; restore ours
            // before the next offset is tested against it.
            let saved_base = self.sp_bound_base;
            // Set only here, so an offset the ceiling prunes cannot leak it.
            self.r9_done = r9_pre_ok;
            // Record only visits whose band covers every child offset the
            // ceiling covers (0..=h_child): under the level-band sweep the
            // lower offsets of a later band were visited in an earlier pass,
            // so their leaves are missing from this subtree's best. Ring-2
            // children start at ring 1's offset, which the ceiling ignores.
            let child_ring2 = (depth + 1) as isize == self.ring2_depth;
            let observe = bound_observe::on() && depth + 1 + bound_observe::slots() == self.n_free
                && lo_rem - offset <= 0 && !child_ring2;
            let observed_ceiling = if observe { self.subtree_ceiling_value(depth, o, hi_rem) } else { None };
            let outer_max = std::mem::replace(&mut self.observe_max, f64::NEG_INFINITY);
            let outer_sp = self.observe_sp;
            let outer_names = self.observe_names;
            self.enumerate(depth + 1, lo_rem - offset, hi_rem - offset);
            // A subtree the time cap cut short understates its best.
            if let Some(c) = observed_ceiling.filter(|_| !self.stop) {
                // The same ceiling at the best leaf's own skill points: not a
                // bound (diagnostic only), it isolates the item relaxation.
                let one_slot = bound_observe::slots() == 1;
                let (at_sp, full) = if one_slot && self.observe_max.is_finite() {
                    let sp = self.observe_sp.map(|v| v as f64);
                    (self.ceiling_at_sp(depth, o, hi_rem, &sp), self.ceiling_of_build(&sp))
                } else { (None, None) };
                let single = if one_slot && self.observe_max.is_finite() {
                    let sp = self.observe_sp.map(|v| v as f64);
                    self.best_single_item(depth, o, hi_rem, &sp)
                } else { None };
                bound_observe::record(c, self.observe_max, at_sp, full);
                bound_observe::record_single(single, self.observe_max);
                self.observe_tangent(depth, o, hi_rem, c);
            }
            if outer_max > self.observe_max {
                self.observe_max = outer_max; self.observe_sp = outer_sp; self.observe_names = outer_names;
            }
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
        if self.actual_leaf_budget == Some(0)
            || self.leaf_budget.is_some_and(|n| n <= 0.0)
            || self.time_cap.is_some_and(|cap|
                self.global_started.unwrap_or(self.started).elapsed().as_secs_f64() >= cap) {
            self.stop = true;
            return;
        }

        if self.n_free == 0 {
            self.evaluate_leaf();
            return;
        }
        // Geometric level bands (see the JS worker): fine-grained ordering
        // early, O(log L_max) prefix re-walks overall.
        let l_max = self.l_max as i64;
        let mut band_lo: i64 = 0;
        let mut band_width: i64 = 1;
        while band_lo <= l_max && !self.stop {
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

/// True when a result (score `a`, items `a_items`) ranks strictly ahead of
/// `(b, b_items)`: higher score first, then item names in ascending order,
/// slot by slot. The same order as the JS `compareTopResult`, so tied
/// builds rank identically in both engines and do not depend on which
/// thread or partition found them first. Names are compared bytewise; JS
/// compares UTF-16 code units, which agree on every shipped item name.
fn ranks_before<A: AsRef<str>, B: AsRef<str>>(a: f64, a_items: &[A], b: f64, b_items: &[B]) -> bool {
    if a != b { return a > b; }
    let n = a_items.len().max(b_items.len());
    for i in 0..n {
        let x = a_items.get(i).map(|s| s.as_ref()).unwrap_or("");
        let y = b_items.get(i).map(|s| s.as_ref()).unwrap_or("");
        if x != y { return x < y; }
    }
    false
}

/// Gear identity ignores SP allocation (keep the best allocation for that gear),
/// treats the two ring slots as interchangeable, and includes the chosen tome
/// multiset. Delimiters are JSON escaped, so names cannot collide.
pub fn build_identity(entry: &TopEntry) -> String {
    let mut items = entry.items.clone();
    if items.len() >= 6 && items[4] > items[5] { items.swap(4, 5); }
    let tome = entry.tome.as_ref().map(|t| {
        let mut weapon = t.weapon_names.clone();
        let mut armor = t.armor_names.clone();
        weapon.sort();
        armor.sort();
        (t.guild_idx, weapon, armor)
    });
    serde_json::to_string(&(items, tome)).expect("build identity")
}

fn same_build(a: &TopEntry, b: &TopEntry) -> bool {
    if a.items.len() != b.items.len() { return false; }
    for i in 0..a.items.len() {
        if a.items.len() >= 6 && (i == 4 || i == 5) { continue; }
        if a.items[i] != b.items[i] { return false; }
    }
    if a.items.len() >= 6 {
        let ring_a = (&a.items[4], &a.items[5]);
        let ring_b = (&b.items[4], &b.items[5]);
        if ring_a != ring_b && ring_a != (ring_b.1, ring_b.0) { return false; }
    }
    match (&a.tome, &b.tome) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let multiset_equal = |a: &[String], b: &[String]| {
                a.len() == b.len() && a.iter().all(|name|
                    a.iter().filter(|x| *x == name).count()
                        == b.iter().filter(|x| *x == name).count())
            };
            a.guild_idx == b.guild_idx
                && multiset_equal(&a.weapon_names, &b.weapon_names)
                && multiset_equal(&a.armor_names, &b.armor_names)
        }
        _ => false,
    }
}

fn insert_top_n(into: &mut Vec<TopEntry>, entry: TopEntry, count: usize) -> bool {
    if count == 0 || !entry.score.is_finite() { return false; }
    if into.len() >= count && entry.score < into[count - 1].score { return false; }
    if let Some(old) = into.iter().position(|e| same_build(e, &entry)) {
        if into[old].score >= entry.score { return false; }
        into.remove(old);
    }
    // Ties rank by item names (ranks_before), the order the JS engine and
    // the page use, so equal-score builds come out the same way in both
    // engines regardless of which thread, partition or repair found them.
    let pos = into.iter().position(|e| ranks_before(entry.score, &entry.items, e.score, &e.items))
        .unwrap_or(into.len());
    if pos >= count { return false; }
    into.insert(pos, entry);
    into.truncate(count);
    true
}

pub fn merge_top_n(into: &mut Vec<TopEntry>, from: Vec<TopEntry>, count: usize) {
    // Normalize the destination as well: callers may merge legacy results.
    let previous = std::mem::take(into);
    for e in previous.into_iter().chain(from) { insert_top_n(into, e, count); }
}

pub fn merge_top(into: &mut Vec<TopEntry>, from: Vec<TopEntry>) {
    merge_top_n(into, from, 15);
}

#[derive(Clone, Copy, Debug)]
pub struct SearchOptions {
    pub result_count: usize,
    pub retain_warm: bool,
}

impl Default for SearchOptions {
    fn default() -> Self { Self { result_count: 15, retain_warm: true } }
}

impl SearchOptions {
    pub fn from_env() -> Self {
        Self {
            result_count: env::var("RESULT_COUNT").ok().and_then(|v| v.parse().ok())
                .filter(|&n| n > 0).unwrap_or(15),
            retain_warm: env::var("RETAIN_WARM").as_deref() != Ok("0"),
        }
    }
}

/// Opt-in native quality log. No file, clock reads or locks on the default
/// leaf path. Timestamps include fixture/scoring preparation and warm search.
/// The lock also gives parallel discoveries a single monotonic archive.
pub struct QualityTrace {
    started: Instant,
    state: Mutex<(BufWriter<fs::File>, Vec<TopEntry>)>,
}

impl QualityTrace {
    pub fn new(path: &str, started: Instant) -> std::io::Result<Self> {
        let mut writer = BufWriter::new(fs::File::create(path)?);
        writeln!(writer, "{}", serde_json::json!({"event":"start", "wall_seconds":0.0}))?;
        writer.flush()?;
        Ok(Self { started, state: Mutex::new((writer, Vec::new())) })
    }

    pub fn record(&self, phase: &str, entries: &[TopEntry], count: usize) {
        let mut state = self.state.lock().expect("quality trace lock");
        let previous_best = state.1.first().map(|e| e.score);
        let previous_kth = state.1.get(count.saturating_sub(1)).map(|e| e.score);
        merge_top_n(&mut state.1, entries.to_vec(), count);
        let best = state.1.first();
        let kth = state.1.get(count.saturating_sub(1)).map(|e| e.score);
        if best.map(|e| e.score) == previous_best && kth == previous_kth { return; }
        let Some(best) = best else { return; };
        let event = serde_json::json!({
            "event":"incumbent", "phase":phase,
            "wall_seconds":self.started.elapsed().as_secs_f64(),
            "score":best.score, "kth_score":kth, "result_count":count,
            "archive_size":state.1.len(), "items":best.items,
            "base_sp":best.base_sp, "total_sp":best.total_sp,
            "assigned_sp":best.assigned_sp, "identity":build_identity(best),
            "tome":best.tome.as_ref().map(|t| serde_json::json!({
                "guild_idx":t.guild_idx, "weaponTome":t.weapon_names, "armorTome":t.armor_names})),
        });
        writeln!(state.0, "{}", event).expect("write quality trace");
        state.0.flush().expect("flush quality trace");
    }

    pub fn finish(&self, complete: bool, warm_seconds: f64) {
        let mut state = self.state.lock().expect("quality trace lock");
        writeln!(state.0, "{}", serde_json::json!({"event":"finish", "complete":complete,
            "wall_seconds":self.started.elapsed().as_secs_f64(), "warm_seconds":warm_seconds}))
            .expect("write quality trace finish");
        state.0.flush().expect("flush quality trace");
    }

    /// Native benchmark telemetry, separate from incumbent events. Counters
    /// describe this phase only: warm work is not original-domain completion.
    fn work(&self, phase: &str, p: &ProgressSnapshot) {
        let mut state = self.state.lock().expect("quality trace lock");
        writeln!(state.0, "{}", serde_json::json!({
            "event":"work", "phase":phase,
            "wall_seconds":self.started.elapsed().as_secs_f64(),
            "checked":p.checked, "total":p.total, "leaf_calls":p.leaf_calls,
            "scored":p.scored, "feasible":p.feasible,
            "precheck_reject":p.precheck_reject, "sp_kernel_reject":p.sp_kernel_reject,
            "gated":p.gated, "bound_pruned":p.bound_pruned,
        })).expect("write quality work trace");
        state.0.flush().expect("flush quality work trace");
    }
}

/// Wide-key bounds are opt-in: the expanded benchmark found their setup and
/// lookup cost outweighed pruning. The switch does not change the bound
/// mathematics. Read during setup only (including the WASM path).
fn wide_pool_bounds_allowed(fx: &Fixture) -> bool {
    std::env::var("WIDE_BOUND_KEYS").as_deref() == Ok("1")
        || fx.slots.iter().all(|s| s.pool.len() < 128)
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
    run_single_with_options(fx, scoring, leaf_budget, progress, part, SearchOptions::from_env())
}

pub fn run_single_with_options(
    fx: &Fixture,
    scoring: Option<&crate::scoring::ScoringCtx>,
    leaf_budget: Option<f64>,
    progress: Option<&mut dyn FnMut(ProgressSnapshot) -> Option<f64>>,
    part: Option<(i64, i64)>,
    options: SearchOptions,
) -> Totals {
    assert!(options.result_count > 0, "result_count must be positive");
    let options = SearchOptions { result_count: effective_result_count(fx, &options), ..options };
    let overall_started = Instant::now();
    let shared_cutoff = AtomicU64::new(0);
    let shared_best = AtomicU64::new(0);
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
        if !wide_pool_bounds_allowed(fx) { return None; }
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
    let warm = seed_warm_cutoff(fx, scoring, &shared_cutoff, &shared_best, warm_k, bound_cluster,
        false, options.result_count, overall_started, None);


    let mut search = Search::new(fx);
    search.scoring = scoring;
    search.result_count = options.result_count;
    search.global_started = Some(overall_started);
    search.shared_cutoff = Some(&shared_cutoff);
    search.shared_best = Some(&shared_best);
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
    // R21 needs the warm builds: with eps > 0 the main search prunes below
    // (1 + eps) * best, which removes the build that set `best` when the warm
    // start found it, so retention is forced on there.
    if options.retain_warm || search.eps > 0.0 {
        for entry in warm {
            // Each browser partition publishes only its own witnesses. This
            // keeps the existing disjoint-partition merge contract intact.
            let owned = part.is_none_or(|(lo, hi)| fx.slots.first().is_none_or(|slot| {
                slot.item_names.iter().position(|n| entry.items.get(slot.pos) == Some(n))
                    .is_some_and(|offset| offset as i64 >= lo && offset as i64 <= hi)
            }));
            if owned { search.insert_top(entry); }
        }
        search.total_space = search.total_space_of();
        search.emit_progress();
    }
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
        top.push_str(&format!("],\"base_sp\":{},\"total_sp\":{},\"assigned_sp\":{}{}{}}}",
                              sp(&e.base_sp), sp(&e.total_sp), e.assigned_sp,
                              tome_json(&e.tome), stats_json(&fx, ctx.as_ref(), e)));
    }
    top.push(']');
    format!(
        "{{\"checked\":{},\"feasible\":{},\"scored\":{},\"gated\":{},\
         \"mana_reject\":{},\"thresh_reject\":{},\"bound_pruned\":{},\
         \"complete\":{},{}\"top\":{}}}",
        totals.checked, totals.feasible, totals.scored, totals.gated,
        totals.mana_reject, totals.thresh_reject, totals.bound_pruned,
        complete, window_json(&fx, complete, &totals.top_n), top,
    )
}

/// R20: `,"stats":{...}` for an archived build in a windowed run (empty
/// otherwise, or when the build cannot be re-assembled).
fn stats_json(fx: &Fixture, ctx: Option<&crate::scoring::ScoringCtx>, e: &TopEntry) -> String {
    let (true, Some(sc)) = (fx.window > 0.0, ctx) else { return String::new() };
    let names: Vec<&str> = e.items.iter().map(String::as_str).collect();
    let Some(stats) = crate::scoring::explain_build(sc, &names, &e.total_sp, e.tome.as_ref()) else { return String::new() };
    let body: Vec<String> = stats.iter().filter(|(_, v)| v.is_finite())
        .map(|(k, v)| format!("\"{k}\":{v}")).collect();
    format!(",\"stats\":{{{}}}", body.join(","))
}

/// R20 fields for the solve JSON (empty without a window). `archive_full`
/// and `archive_last` let a host that merges several partitions apply the
/// completeness rule itself: every partition complete, and every full
/// partition's last score below the merged window line.
fn window_json(fx: &Fixture, complete: bool, top: &[TopEntry]) -> String {
    if fx.window <= 0.0 { return String::new(); }
    let full = top.len() >= fx.archive_cap;
    let last = if full { top[fx.archive_cap - 1].score } else { f64::NEG_INFINITY };
    format!("\"window\":{},\"archive_cap\":{},\"archive_full\":{},\"archive_last\":{},\"window_complete\":{},",
            fx.window, fx.archive_cap, full,
            if last.is_finite() { format!("{last:.17e}") } else { "null".into() },
            window_complete(complete, top, fx.archive_cap, fx.window))
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
    shared_cutoff: &AtomicU64, shared_best: &AtomicU64, warm_k: usize, bound_cluster: usize,
    verbose: bool,
    result_count: usize, overall_started: Instant, quality_trace: Option<&Arc<QualityTrace>>,
) -> Vec<TopEntry> {
if !(scoring.is_some() && warm_k > 0 && fx.slots.iter().any(|s| s.pool.len() > warm_k)) {
    return Vec::new();
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
                            &sc.rows, &sc.compiled_rows, &sc.tables, &[150.0; 5])
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
        weapon_set: fx.weapon_set,
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
        eps: fx.eps,
        window: fx.window,
        archive_cap: fx.archive_cap,
    };
    let warm_started = Instant::now();
    let wpools: Vec<Vec<String>> = wfx.slots.iter().map(|s| s.item_names.clone()).collect();
    let wdb = sc.dense.as_ref().and_then(|d| {
        crate::scoring::DenseBound::build(&sc.layer2, d, &wpools, bound_cluster)
    });
    let mut ws = Search::new(&wfx);
    ws.scoring = scoring;
    ws.result_count = result_count;
    ws.global_started = Some(overall_started);
    ws.quality_trace = quality_trace.cloned();
    ws.trace_phase = "warm";
    ws.shared_cutoff = Some(&shared_cutoff);
    ws.shared_best = Some(shared_best);
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
    ws.top_n
}
}

/// CLI entry point (thin wrapper lives in src/bin/enum_kernel.rs).
pub fn cli_main() {
    anytime_trace::start();
    let overall_started = Instant::now();
    let options = SearchOptions::from_env();
    let quality_trace = env::var("QUALITY_TRACE_PATH").ok().map(|path|
        Arc::new(QualityTrace::new(&path, overall_started).expect("create quality trace")));
    let args: Vec<String> = env::args().collect();
    let fixture_path = args.get(1).map(String::as_str)
        .expect("usage: enum_kernel <fixture> [threads] [score_fixture.json]");
    let text = fs::read_to_string(fixture_path).expect("cannot read fixture");
    let mut fx = parse_fixture(&text);
    // SEARCH_WINDOW (CLI) overrides the fixture's WINDOW line; "0" turns it off.
    if let Some(w) = env::var("SEARCH_WINDOW").ok().and_then(|v| v.parse::<f64>().ok()) {
        fx.window = if w.is_finite() && w > 0.0 && w < 1.0 { w } else { 0.0 };
    }
    let options = SearchOptions { result_count: effective_result_count(&fx, &options), ..options };

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
    let shared_best = AtomicU64::new(0);

    // Mid-tree damage ceiling bound tables (objective B&B). Structured keys
    // allow wide pools; WIDE_BOUND_KEYS=0 restores the old wide-pool bypass.
    let bound_max_depth: usize = env::var("BOUND_DEPTH").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(0);
    let bound_tail: usize = env::var("BOUND_TAIL").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(1);
    let bound_tables: Option<crate::scoring::BoundTables> = scoring.and_then(|sc| {
        let bound_cluster_on: bool = env::var("BOUND_CLUSTER").ok()
            .and_then(|s| s.parse::<usize>().ok()).unwrap_or(4) > 0;
        if bound_max_depth == 0 && bound_tail == 0 && !bound_cluster_on { return None; }
        if !wide_pool_bounds_allowed(&fx) {
            eprintln!("bound: WIDE_BOUND_KEYS=0 and pool >=128 items — skipping bound");
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
    let warm_started = Instant::now();
    let warm = seed_warm_cutoff(&fx, scoring, &shared_cutoff, &shared_best, warm_k, bound_cluster,
        true, options.result_count, overall_started, quality_trace.as_ref());
    let warm_seconds = warm_started.elapsed().as_secs_f64();

    let start = Instant::now();

    let (totals, elapsed) = if n_threads <= 1 || fx.slots.is_empty() {
        let counters = env::var("QUALITY_TRACE_COUNTERS").as_deref() == Ok("1");
        let mut last_counter = f64::NEG_INFINITY;
        let mut on_work = |p: ProgressSnapshot| {
            let elapsed = overall_started.elapsed().as_secs_f64();
            if elapsed - last_counter >= 0.1 {
                if let Some(trace) = &quality_trace { trace.work("search", &p); }
                last_counter = elapsed;
            }
            None
        };
        let mut search = Search::new(&fx);
        search.scoring = scoring;
        search.result_count = options.result_count;
        search.global_started = Some(overall_started);
        search.quality_trace = quality_trace.clone();
        search.shared_cutoff = Some(&shared_cutoff);
        search.shared_best = Some(&shared_best);
        if options.retain_warm || search.eps > 0.0 {
            for entry in warm.iter().cloned() { search.insert_top(entry); }
        }
        search.bound_tables = bounds;
        search.bound_max_depth = bound_max_depth;
        search.bound_tail = bound_tail;
        search.dense_bound = dense_bound;
        search.leaf_budget = cli_leaf_budget;
        if counters && quality_trace.is_some() {
            search.progress = Some(&mut on_work);
            search.progress_every = 8192.0;
            search.next_progress = 1.0;
        }
        search.init_equip_names();
        search.run();
        search.emit_progress();
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
                    search.result_count = options.result_count;
                    search.global_started = Some(overall_started);
                    search.quality_trace = quality_trace.clone();
                    search.shared_cutoff = Some(&shared_cutoff);
                    search.shared_best = Some(&shared_best);
                    if options.retain_warm || search.eps > 0.0 {
                        for entry in warm.iter().cloned() { search.insert_top(entry); }
                    }
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
                        if time_cap.is_some_and(|cap| overall_started.elapsed().as_secs_f64() >= cap) {
                            search.stop = true;
                        }
                        if search.stop { break; }
                        let o = next_offset.fetch_add(1, Ordering::Relaxed);
                        if o >= first_pool_len { break; }
                        search.part_lo = o as i64;
                        search.part_hi = o as i64;
                        let mut band_lo: i64 = 0;
                        let mut band_width: i64 = 1;
                        while band_lo <= l_max && !search.stop {
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
                        if overall_started.elapsed().as_secs_f64() >= cap { stop_flag.store(1, Ordering::Relaxed); }
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
                totals.leaf_calls += t.leaf_calls;
                totals.stopped_early |= t.stopped_early;
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
                totals.stopped_early |= t.stopped_early;
                merge_top_n(&mut totals.top_n, t.top_n, options.result_count);
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
    // Whether the space was exhausted (a proof) or a budget or time cap
    // stopped it first. anytime.py keys proven optima on this.
    println!("search: complete {}", if totals.stopped_early { "no" } else { "yes" });
    let final_cut = totals.top_n.get(14).map(|e| e.score);
    if let Some(line) = bound_observe::report(final_cut) { println!("{line}"); }
    if fx.window > 0.0 {
        let best = totals.top_n.first().map_or(f64::NAN, |e| e.score);
        let inside = totals.top_n.iter().filter(|e| e.score >= best * (1.0 - fx.window)).count();
        println!("search: window {} | {} builds within {:.3}% of the best (archive {} of cap {}) | complete within window: {}",
                 fx.window, inside, fx.window * 100.0, totals.top_n.len(), fx.archive_cap,
                 if window_complete(!totals.stopped_early, &totals.top_n, fx.archive_cap, fx.window) { "yes" } else { "no" });
    }
    let eps = Search::new(&fx).eps;
    if eps > 0.0 {
        // R21's claim, stated where the result is: the top-1 is within eps of
        // the optimum (if complete); ranks 2 to 15 are not certified.
        println!("search: eps {} | top-1 within {:.3}% of optimal{} | ranks 2-15 unverified",
                 eps, eps * 100.0, if totals.stopped_early { " (NOT proved: stopped early)" } else { " (proved)" });
    }
    println!("timing: wall_total {:.6}s | warm {:.6}s | main {:.6}s | complete {} | result_count {} | retain_warm {}",
        overall_started.elapsed().as_secs_f64(), warm_seconds, elapsed.as_secs_f64(),
        !totals.stopped_early, options.result_count, options.retain_warm);
    if let Some(trace) = &quality_trace { trace.finish(!totals.stopped_early, warm_seconds); }
    crate::scoring::trace::report();
    if let Some(r) = crate::scoring::greedy_audit_report() { println!("{r}"); }
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
        // R20: the explain pass's stats per archived build, one line each,
        // after the top15 lines so their parsers are unaffected.
        if fx.window > 0.0 {
            for (rank, e) in totals.top_n.iter().enumerate() {
                let names: Vec<&str> = e.items.iter().map(String::as_str).collect();
                if let Some(st) = crate::scoring::explain_build(scoring.unwrap(), &names, &e.total_sp, e.tome.as_ref()) {
                    let tome = e.tome.as_ref().map(|t| format!(" guild_idx={} tomes={}", t.guild_idx,
                        t.weapon_names.len() + t.armor_names.len())).unwrap_or_default();
                    println!("stats: {} {}{} total_sp={:?}", rank + 1, st.iter().map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>().join(" "), tome, e.total_sp);
                }
            }
        }
    }
}

#[cfg(test)]
mod top_order_tests {
    use super::*;

    fn entry(score: f64, names: &[&str]) -> TopEntry {
        TopEntry { score, items: names.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn ties_rank_by_item_names() {
        assert!(ranks_before(2.0, &["b"], 1.0, &["a"]));
        assert!(ranks_before(1.0, &["Clandestine"], 1.0, &["Mechanical Augmentation"]));
        assert!(!ranks_before(1.0, &["Mechanical Augmentation"], 1.0, &["Clandestine"]));
        assert!(!ranks_before(1.0, &["a"], 1.0, &["a"]));
    }

    #[test]
    fn merge_is_independent_of_thread_order() {
        // Two threads each found one of a tied pair. Merging in either order
        // must give the same list, and at capacity the same survivor.
        let mut base: Vec<TopEntry> = (0..14).map(|i| entry(100.0 - i as f64, &["x"])).collect();
        base.iter_mut().enumerate().for_each(|(i, e)| e.items = vec![format!("x{i:02}")]);
        let a = vec![entry(50.0, &["Mechanical Augmentation"])];
        let b = vec![entry(50.0, &["Clandestine"])];
        let mut ab = base.clone();
        merge_top(&mut ab, a.clone());
        merge_top(&mut ab, b.clone());
        let mut ba = base.clone();
        merge_top(&mut ba, b);
        merge_top(&mut ba, a);
        let names = |v: &Vec<TopEntry>| v.iter().map(|e| e.items[0].clone()).collect::<Vec<_>>();
        assert_eq!(ab.len(), 15);
        assert_eq!(names(&ab), names(&ba));
        assert_eq!(ab[14].items[0], "Clandestine");
    }
}

#[cfg(test)]
mod eps_tests {
    //! R21 regression: with eps > 0 the search prunes below (1 + eps) * best.
    //! When the warm start found the best build, that prune removed the build
    //! itself and the run returned no results at all (tierstack_small at 5%,
    //! heavy_melee_small at 2%). The claim under test: results are non-empty
    //! and the top-1 is within eps of the proved optimum.
    use super::*;

    fn fixture(name: &str) -> String {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/");
        std::fs::read_to_string(format!("{dir}{name}")).expect("fixture")
    }

    fn top1(json: &str) -> Option<f64> {
        let v: serde_json::Value = serde_json::from_str(json).expect("solve json");
        v["top"].as_array().and_then(|a| a.first()).and_then(|e| e["score"].as_f64())
    }

    fn check(fam: &str, eps: f64, proved_optimum: f64) {
        let enum_fx = fixture(&format!("enum_{fam}.txt")) + &format!("\nEPS {eps}\n");
        let score_fx = fixture(&format!("score_{fam}.json"));
        let out = solve_json_full(&enum_fx, &score_fx, 0.0, None, 0, 1);
        let best = top1(&out).unwrap_or_else(|| panic!("{fam} eps {eps}: no results: {}", &out[..out.len().min(300)]));
        assert!(best <= proved_optimum * (1.0 + 1e-12), "{fam}: {best} above the proved optimum");
        assert!(best >= proved_optimum / (1.0 + eps) * (1.0 - 1e-12),
                "{fam} eps {eps}: {best} not within eps of {proved_optimum}");
    }

    #[test]
    fn eps_keeps_the_incumbent_tierstack() {
        // Proved optimum: full exact run (anytime_ref.json).
        check("fam_tierstack_small", 0.05, 2.20863848359218158e5);
    }

    #[test]
    fn eps_keeps_the_incumbent_heavy_melee() {
        check("fam_heavy_melee_small", 0.02, 3.84581312053956135e4);
    }

    #[test]
    fn eps_line_parses_and_zero_is_exact() {
        let base = fixture("enum_fam_tierstack_small.txt");
        assert_eq!(parse_fixture(&base).eps, 0.0);
        assert_eq!(parse_fixture(&(base.clone() + "\nEPS 0.01\n")).eps, 0.01);
        assert_eq!(parse_fixture(&(base.clone() + "\nEPS -1\n")).eps, 0.0);
        assert_eq!(parse_fixture(&(base + "\nEPS nan\n")).eps, 0.0);
    }
}

#[cfg(test)]
mod anytime_core_tests {
    use super::*;

    fn entry(name: &str, score: f64) -> TopEntry {
        let mut items = vec![String::new(); 8];
        items[0] = name.into();
        TopEntry { items, score, ..Default::default() }
    }

    #[test]
    fn archive_deduplicates_ring_exchange_before_cutoff() {
        let mut a = entry("a", 100.0);
        a.items[4] = "ring A".into();
        a.items[5] = "ring B".into();
        let mut swapped = a.clone();
        swapped.items.swap(4, 5);
        let mut archive = Vec::new();
        merge_top_n(&mut archive, vec![a.clone(), swapped, a.clone()], 15);
        assert_eq!(archive.len(), 1);
        assert_eq!(build_identity(&archive[0]), build_identity(&a));
        let mut stronger_allocation = a.clone();
        stronger_allocation.base_sp[0] = 10;
        stronger_allocation.score = 120.0;
        merge_top_n(&mut archive, vec![stronger_allocation], 15);
        assert_eq!(archive.len(), 1);
        assert_eq!(archive[0].score, 120.0);
        assert_eq!(archive[0].base_sp[0], 10);
    }

    #[test]
    fn archive_identity_includes_tome_multiset_and_guild() {
        let mut a = entry("a", 10.0);
        a.tome = Some(crate::scoring::TomeChoice {
            guild_idx: 2, weapon_names: vec!["A".into(), "B".into()],
            armor_names: vec!["C".into(), "D".into()],
        });
        let mut permutation = a.clone();
        permutation.tome.as_mut().unwrap().weapon_names.reverse();
        permutation.tome.as_mut().unwrap().armor_names.reverse();
        let mut other = a.clone();
        other.tome.as_mut().unwrap().guild_idx = 3;
        let mut multiplicity = a.clone();
        multiplicity.tome.as_mut().unwrap().weapon_names = vec!["A".into(), "A".into()];
        let mut archive = Vec::new();
        merge_top_n(&mut archive, vec![a, permutation, other, multiplicity], 15);
        assert_eq!(archive.len(), 3);
    }

    fn empty_fixture() -> Fixture {
        parse_fixture("BUDGET 200\nPRECHECKS 0\nEHP 0 0 0 0\nEHPNA 0 0 0 0\nTHP 0 0 0\nHPSTART 0\nWEAPON 0 0 0 0 0 0 0 0 0 0\nGUILD 0\nNFIXED 0\nNSLOTS 0\nNSETS 0\n")
    }

    #[test]
    fn duplicate_witnesses_cannot_raise_top15_cutoff() {
        let fx = empty_fixture();
        let mut search = Search::new(&fx);
        search.result_count = 15;
        let shared = AtomicU64::new(0);
        search.shared_cutoff = Some(&shared);
        for _ in 0..30 { search.insert_top(entry("same", 100.0)); }
        assert_eq!(search.cutoff(), None);
        assert_eq!(shared.load(Ordering::Relaxed), 0);
        for i in 0..14 { search.insert_top(entry(&format!("distinct {i}"), 90.0)); }
        assert_eq!(search.cutoff(), Some(90.0));
        assert_eq!(shared.load(Ordering::Relaxed), 90);
    }

    #[test]
    fn top1_cutoff_has_a_retained_witness_after_zero_budget() {
        let fx = empty_fixture();
        let mut search = Search::new(&fx);
        search.result_count = 1;
        search.actual_leaf_budget = Some(0);
        search.insert_top(entry("warm", 123.0));
        search.run();
        assert!(search.stop);
        assert_eq!(search.leaf_calls, 0);
        assert_eq!(search.cutoff(), Some(123.0));
        assert_eq!(search.top_n[0].items[0], "warm");
    }

    #[test]
    fn shared_deadline_can_expire_before_main_search() {
        let fx = empty_fixture();
        let mut search = Search::new(&fx);
        search.global_started = Some(Instant::now());
        search.time_cap = Some(0.0);
        search.run();
        assert!(search.stop);
        assert_eq!(search.leaf_calls, 0);
    }

    #[test]
    fn actual_leaf_budget_counts_work_instead_of_credited_space() {
        let fx = empty_fixture();
        let mut search = Search::new(&fx);
        search.actual_leaf_budget = Some(1);
        search.run();
        assert_eq!(search.leaf_calls, 1);
        assert!(search.stop);
    }
    fn wide_fixture(nested: bool) -> Fixture {
        let mut fx = empty_fixture();
        let make_slot = |name: &str, pos: usize, count: usize| Slot {
            name: name.into(), pos, is_ring1: false, is_ring2: false,
            pool: vec![PoolItem { crafted: false, reqs: [0; 5], skp: [0; 5],
                set_id: -1, illegal_id: -1, hp: 3.0, pc: Vec::new() }; count],
            item_names: Vec::new(),
        };
        if nested { fx.slots.push(make_slot("helmet", 0, 3)); }
        fx.slots.push(make_slot("boots", 3, 257));
        fx
    }

    #[test]
    fn wide_last_slot_honors_exact_actual_leaf_budgets() {
        for budget in [10, 37] {
            let fx = wide_fixture(false);
            let mut search = Search::new(&fx);
            search.actual_leaf_budget = Some(budget);
            search.run();
            assert!(search.stop);
            assert_eq!(search.leaf_calls, budget);
            assert_eq!(search.checked, budget as f64);
            assert_eq!(search.feasible, budget);
            assert_eq!(search.hp_running, fx.hp_start);
        }
    }

    #[test]
    fn nested_budget_stop_restores_parent_state_and_stops_siblings() {
        let fx = wide_fixture(true);
        let mut search = Search::new(&fx);
        search.actual_leaf_budget = Some(37);
        search.run();
        assert!(search.stop);
        assert_eq!(search.leaf_calls, 37);
        assert_eq!(search.checked, 37.0);
        assert_eq!(search.hp_running, fx.hp_start);
        assert_eq!(search.sp_free_prov, [0; 5]);
        assert!(search.equip_set.iter().all(|&set| set == -1));
    }

    #[test]
    fn wide_last_slot_honors_credited_budget_without_band_overrun() {
        let fx = wide_fixture(false);
        let mut search = Search::new(&fx);
        search.leaf_budget = Some(37.0);
        search.run();
        assert!(search.stop);
        assert_eq!(search.leaf_calls, 37);
        assert_eq!(search.checked, 37.0);
    }

    #[test]
    fn wide_last_slot_observes_shared_cancellation_within_one_poll_window() {
        let fx = wide_fixture(false);
        let flag = AtomicU64::new(1);
        let mut search = Search::new(&fx);
        search.stop_flag = Some(&flag);
        search.run();
        assert!(search.stop);
        assert_eq!(search.leaf_calls, 256);
        assert_eq!(search.hp_running, fx.hp_start);
    }

    fn tied_ring_fixture(canonical_flags: bool, freeze_first: bool) -> Fixture {
        let mut fx = empty_fixture();
        fx.budget = 10;
        let a = PoolItem { crafted: false, reqs: [10, 0, 0, 0, 0],
            skp: [0, 10, 0, 0, 0], set_id: -1, illegal_id: -1,
            hp: 0.0, pc: Vec::new() };
        let b = PoolItem { reqs: [0, 10, 0, 0, 0], skp: [10, 0, 0, 0, 0], ..a.clone() };
        fx.slots.push(Slot { name: "ring1".into(), pos: 4,
            is_ring1: canonical_flags, is_ring2: false,
            pool: if freeze_first { vec![b.clone()] } else { vec![a.clone(), b.clone()] },
            item_names: if freeze_first { vec!["B".into()] } else { vec!["A".into(), "B".into()] },
        });
        fx.slots.push(Slot { name: "ring2".into(), pos: 5,
            is_ring1: false, is_ring2: canonical_flags,
            pool: vec![a, b], item_names: vec!["A".into(), "B".into()],
        });
        fx
    }

    #[test]
    fn original_ring_guard_preserves_domain_before_order_sensitive_sp_ties() {
        let a = Unit { crafted: false, reqs: [10, 0, 0, 0, 0], skp: [0, 10, 0, 0, 0] };
        let b = Unit { crafted: false, reqs: [0, 10, 0, 0, 0], skp: [10, 0, 0, 0, 0] };
        let mut case = Case { budget: 10, equipment: [Unit::default(); 8],
            weapon: Unit::default(), set_free: [0; 5], expected: None };
        case.equipment[4] = a;
        case.equipment[5] = b;
        let forward = Kernel::new().calculate(&case).unwrap();
        case.equipment.swap(4, 5);
        let reverse = Kernel::new().calculate(&case).unwrap();
        assert_eq!(forward.2, reverse.2);
        assert_eq!(forward.1, [20, 10, 0, 0, 0]);
        assert_eq!(reverse.1, [10, 20, 0, 0, 0]);

        let order = std::collections::HashMap::from([("A".into(), 0), ("B".into(), 1)]);
        let original = tied_ring_fixture(true, false);
        let mut root = Search::new(&original);
        root.run();
        assert_eq!(root.feasible, 3);
        let reduced = tied_ring_fixture(false, false);
        let mut repair = Search::new(&reduced);
        repair.original_ring_order = Some(&order);
        repair.run();
        assert_eq!(repair.checked, 4.0);
        assert_eq!(repair.precheck_reject, 1.0);
        assert_eq!(repair.precheck_pass, 3);
        assert_eq!(repair.feasible, root.feasible);
    }

    #[test]
    fn original_ring_guard_uses_root_ranks_when_one_repair_ring_is_frozen() {
        let order = std::collections::HashMap::from([("A".into(), 0), ("B".into(), 1)]);
        let reduced = tied_ring_fixture(false, true);
        let mut guarded = Search::new(&reduced);
        guarded.original_ring_order = Some(&order);
        guarded.run();
        assert_eq!(guarded.checked, 2.0);
        assert_eq!(guarded.precheck_reject, 1.0);
        assert_eq!(guarded.feasible, 1);
        // No root rule is imposed by default. This is necessary when only
        // one ring was free in the user's original search configuration.
        let mut unguarded = Search::new(&reduced);
        unguarded.run();
        assert_eq!(unguarded.feasible, 2);
    }

}

#[cfg(test)]
mod window_tests {
    //! R20 windowed archive. The completeness rule, the fixture lines, and an
    //! end-to-end check against a brute-force reference: on tierstack_small
    //! a run with no cutoff at all (RESULT_COUNT 1e6, every feasible build
    //! scored and kept; 221,194 builds) has exactly 3 builds within 2% of the
    //! best, and the windowed run must return those 3.
    use super::*;

    fn entry(score: f64) -> TopEntry { TopEntry { score, ..Default::default() } }

    #[test]
    fn completeness_rule() {
        let top: Vec<TopEntry> = [100.0, 99.0, 97.0].iter().map(|&s| entry(s)).collect();
        // Not full: complete if the search finished.
        assert!(window_complete(true, &top, 10, 0.02));
        assert!(!window_complete(false, &top, 10, 0.02));
        // Full, last entry (97) below the line (98): nothing in the window lost.
        assert!(window_complete(true, &top, 3, 0.02));
        // Full, last entry (97) at or above the line (96.5): an evicted build
        // may have been inside the window, so the claim is withdrawn.
        assert!(!window_complete(true, &top, 3, 0.035));
        // No window: never claims.
        assert!(!window_complete(true, &top, 10, 0.0));
    }

    #[test]
    fn window_lines_parse() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/");
        let base = std::fs::read_to_string(format!("{dir}enum_fam_tierstack_small.txt")).unwrap();
        let fx = parse_fixture(&base);
        assert_eq!((fx.window, fx.archive_cap), (0.0, DEFAULT_ARCHIVE_CAP));
        let fx = parse_fixture(&(base.clone() + "\nWINDOW 0.05\nARCHIVE 300\n"));
        assert_eq!((fx.window, fx.archive_cap), (0.05, 300));
        let fx = parse_fixture(&(base + "\nWINDOW 1.5\nARCHIVE 0\n"));
        assert_eq!((fx.window, fx.archive_cap), (0.0, DEFAULT_ARCHIVE_CAP));
    }

    #[test]
    fn window_holds_every_build_within_it() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/");
        let enum_fx = std::fs::read_to_string(format!("{dir}enum_fam_tierstack_small.txt")).unwrap()
            + "\nWINDOW 0.02\n";
        let score_fx = std::fs::read_to_string(format!("{dir}score_fam_tierstack_small.json")).unwrap();
        let out = solve_json_full(&enum_fx, &score_fx, 0.0, None, 0, 1);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["window_complete"], serde_json::json!(true), "{}", &out[..out.len().min(400)]);
        let scores: Vec<f64> = v["top"].as_array().unwrap().iter()
            .map(|e| e["score"].as_f64().unwrap()).collect();
        let best = scores[0];
        assert!((best - 2.20863848359218158e5).abs() < 1e-6, "top-1 {best}");
        let inside = scores.iter().filter(|&&s| s >= best * 0.98).count();
        assert_eq!(inside, 3, "builds within 2%: {scores:?}");
    }
}
