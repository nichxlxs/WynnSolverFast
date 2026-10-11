//! Anytime large-neighbourhood search using the production evaluator.
//!
//! This searches overlapping subspaces and DOES NOT prove global optimality.
//! `complete` is consequently always false, even if a particular repair
//! exhausted its domain. No surrogate score is ever published as a result.
//! The immutable ScoringCtx is loaded once; every repair uses Search and the
//! existing exact SP/legality/scoring pipeline. SP allocation and other game
//! modelling limitations of that evaluator are inherited unchanged.
//!
//! All original slots remain present, with unselected slots reduced to a
//! singleton. This preserves set, fixed-stat, requirement and illegal-item
//! handling. Index-based ring symmetry is deliberately disabled on reduced
//! pools: different shortlists do not share comparable offsets. The archive
//! deduplicates actual gear/tome identity, including swapped rings.

use super::{Fixture, Search, Slot, TopEntry};
use crate::clock::Instant;
use crate::scoring::ScoringCtx;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone)]
pub struct Options {
    pub seconds: f64,
    pub seed: u64,
    /// Actual calls to evaluate_leaf across all repairs, not credited space.
    pub work_budget: u64,
    /// Credited tuples per repair. Bounds may credit whole skipped subtrees.
    pub repair_budget: u64,
    /// Independent ranked-seed budget; None preserves the old repair budget.
    pub warm_budget: Option<u64>,
    /// Resume local operators after one forced stagnation diversification.
    /// Kept off by default so the existing scheduler remains available for A/B.
    pub cycle_stagnation: bool,
    /// Periodically recombine small domains learned from the diverse archive.
    /// This restricts a heuristic repair only, never the original search space.
    pub elite_pool: bool,
    pub max_repairs: usize,
    pub top_k: usize,
    pub archive_size: usize,
    pub warm_k: usize,
    pub quality_trace: Option<Arc<super::QualityTrace>>,
}

impl Default for Options {
    fn default() -> Self {
        Self { seconds: 10.0, seed: 1, work_budget: u64::MAX,
            repair_budget: 100_000, warm_budget: None, cycle_stagnation: false, elite_pool: false,
            max_repairs: 1_000,
            top_k: 1, archive_size: 24, warm_k: 3, quality_trace: None }
    }
}

#[derive(Clone, Debug)]
pub struct QualityPoint {
    pub elapsed_secs: f64,
    pub best_score: f64,
    pub leaf_calls: u64,
    pub repair: usize,
    pub phase: String,
    pub entry: TopEntry,
}

#[derive(Default)]
pub struct Result {
    pub top: Vec<TopEntry>,
    pub elapsed_secs: f64,
    pub setup_secs: f64,
    pub leaf_calls: u64,
    pub credited_tuples: f64,
    pub scored: u64,
    pub repairs: usize,
    pub completed_repairs: usize,
    pub operator_calls: [u64; 5],
    pub operator_improvements: [u64; 5],
    pub elite_calls: u64,
    pub elite_improvements: u64,
    pub elite_leaf_calls: u64,
    /// Sums of overlapping local domain sizes, NOT unique work avoided.
    pub elite_full_domain_tuples: f64,
    pub elite_restricted_domain_tuples: f64,
    pub stop_reason: String,
    pub trace: Vec<QualityPoint>,
}

/// A snapshot of the retained best builds across every repair so far.
/// Credited tuples include repeated and pruned domains and are deliberately
/// never divided by the original search space to imply completeness.
#[derive(Clone, Debug)]
pub struct Progress {
    pub elapsed_secs: f64,
    pub leaf_calls: u64,
    pub credited_tuples: f64,
    pub scored: u64,
    pub repairs: usize,
    pub completed_repairs: usize,
    pub phase: String,
    pub stop_reason: String,
    pub top: Vec<TopEntry>,
}

impl Progress {
    pub fn json(&self, opts: &Options) -> Value {
        let mut top = Vec::new();
        for e in &self.top {
            let mut row = json!({ "score": e.score, "item_names": e.items,
                "base_sp": e.base_sp, "total_sp": e.total_sp,
                "assigned_sp": e.assigned_sp });
            if let Some(t) = &e.tome {
                row["tome"] = json!({ "guild_idx": t.guild_idx,
                    "weaponTome": t.weapon_names, "armorTome": t.armor_names });
            }
            top.push(row);
        }
        json!({ "algorithm": "alns", "complete": false,
            "quality_reference": "production_evaluator", "seed": opts.seed,
            "seconds_budget": opts.seconds, "elapsed_secs": self.elapsed_secs,
            "leaf_calls": self.leaf_calls, "credited_tuples": self.credited_tuples,
            "scored": self.scored, "repairs": self.repairs,
            "completed_repairs": self.completed_repairs,
            "phase": self.phase, "stop_reason": self.stop_reason,
            "best_score": self.top.first().map(|e| e.score), "top_n": top })
    }
}

fn progress_from_result(result: &Result, phase: &str) -> Progress {
    Progress { elapsed_secs: result.elapsed_secs, leaf_calls: result.leaf_calls,
        credited_tuples: result.credited_tuples, scored: result.scored,
        repairs: result.repairs, completed_repairs: result.completed_repairs,
        phase: phase.into(), stop_reason: result.stop_reason.clone(), top: result.top.clone() }
}

impl Result {
    pub fn json(&self, opts: &Options) -> Value {
        let top: Vec<Value> = self.top.iter().map(|e| {
            let mut v = json!({ "score": e.score, "items": e.items,
                "base_sp": e.base_sp, "total_sp": e.total_sp,
                "assigned_sp": e.assigned_sp });
            if let Some(t) = &e.tome {
                v["tome"] = json!({ "guild_idx": t.guild_idx,
                    "weaponTome": t.weapon_names, "armorTome": t.armor_names });
            }
            v
        }).collect();
        json!({ "algorithm": "alns", "complete": false,
            "quality_reference": "production_evaluator", "seed": opts.seed,
            "seconds_budget": opts.seconds, "work_budget": opts.work_budget,
            "repair_budget": opts.repair_budget,
            "warm_budget": opts.warm_budget.unwrap_or(opts.repair_budget),
            "cycle_stagnation": opts.cycle_stagnation,
            "elite_pool": opts.elite_pool,
            "elapsed_secs": self.elapsed_secs,
            "setup_secs": self.setup_secs, "leaf_calls": self.leaf_calls,
            "credited_tuples": self.credited_tuples, "scored": self.scored,
            "repairs": self.repairs, "completed_repairs": self.completed_repairs,
            "operator_names": ["pair", "triple", "perturb", "crossover", "restart"],
            "operator_calls": self.operator_calls,
            "operator_improvements": self.operator_improvements,
            "elite_calls": self.elite_calls, "elite_improvements": self.elite_improvements,
            "elite_leaf_calls": self.elite_leaf_calls,
            "elite_full_domain_tuples": self.elite_full_domain_tuples,
            "elite_restricted_domain_tuples": self.elite_restricted_domain_tuples,
            "elite_domain_semantics": "sum_of_overlapping_local_domains_not_unique_work_avoided",
            "stop_reason": self.stop_reason,
            "best_score": self.top.first().map(|e| e.score), "top": top })
    }
}

/// Small specified PRNG; work-limited runs reproduce independently of the
/// platform's standard-library RNG implementation and HashMap iteration.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    fn index(&mut self, n: usize) -> usize { (self.next() % n.max(1) as u64) as usize }
    fn shuffle<T>(&mut self, a: &mut [T]) {
        for i in (1..a.len()).rev() { a.swap(i, self.index(i + 1)); }
    }
}

/// Returns the selected operator and whether it is a forced stagnation
/// diversification. Keeping selection separate makes the legacy and cyclic
/// schedulers use exactly the same random draws before their first reset.
fn select_operator(
    rng: &mut Rng, has_base: bool, stalled: usize, result: &Result,
) -> (usize, bool) {
    let forced = has_base && stalled >= 8;
    let op = if !has_base || result.repairs % 11 == 0 { 4 }
    else if forced { if rng.index(2) == 0 { 2 } else { 4 } }
    else {
        let weights: Vec<u64> = (0..4).map(|i| 2 +
            8 * result.operator_improvements[i] / (result.operator_calls[i] + 1)).collect();
        let mut draw = rng.next() % weights.iter().sum::<u64>();
        let mut pick = 0;
        for (i, w) in weights.iter().enumerate() {
            if draw < *w { pick = i; break; } draw -= *w;
        }
        pick
    };
    (op, forced)
}

fn stall_after_repair(stalled: usize, improved: bool, forced: bool, cyclic: bool) -> usize {
    if improved || (forced && cyclic) { 0 } else { stalled.saturating_add(1) }
}

fn warm_options(opts: &Options) -> Options {
    let mut warm = opts.clone();
    warm.repair_budget = opts.warm_budget.unwrap_or(opts.repair_budget);
    warm
}

fn identity(e: &TopEntry) -> String {
    let mut names = e.items.clone();
    if names.len() > 5 && names[4] > names[5] { names.swap(4, 5); }
    let mut v = json!({ "items": names });
    if let Some(t) = &e.tome {
        let mut w = t.weapon_names.clone(); w.sort();
        let mut a = t.armor_names.clone(); a.sort();
        v["tome"] = json!([t.guild_idx, w, a]);
    }
    v.to_string()
}

fn distance(a: &TopEntry, b: &TopEntry) -> usize {
    let mut aa = a.items.clone();
    let mut bb = b.items.clone();
    if aa.len() > 5 && aa[4] > aa[5] { aa.swap(4, 5); }
    if bb.len() > 5 && bb[4] > bb[5] { bb.swap(4, 5); }
    aa.iter().zip(&bb).filter(|(x, y)| x != y).count()
}

/// Keep the best quarter by score, then greedily preserve distant builds.
/// Diversity affects which valid candidates we explore, never their score.
fn update_archive(archive: &mut Vec<TopEntry>, entries: Vec<TopEntry>, limit: usize) {
    for e in entries {
        if !e.score.is_finite() { continue; }
        let key = identity(&e);
        if let Some(i) = archive.iter().position(|old| identity(old) == key) {
            if e.score > archive[i].score { archive[i] = e; }
        } else { archive.push(e); }
    }
    archive.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| identity(a).cmp(&identity(b))));
    if archive.len() <= limit { return; }
    let mut remainder = archive.split_off((limit / 4).max(1));
    while archive.len() < limit && !remainder.is_empty() {
        let i = remainder.iter().enumerate().max_by(|(ia, a), (ib, b)| {
            let da = archive.iter().map(|x| distance(x, a)).min().unwrap_or(0);
            let db = archive.iter().map(|x| distance(x, b)).min().unwrap_or(0);
            da.cmp(&db).then_with(|| a.score.total_cmp(&b.score)).then_with(|| ib.cmp(ia))
        }).map(|(i, _)| i).unwrap();
        archive.push(remainder.remove(i));
    }
    archive.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| identity(a).cmp(&identity(b))));
}

fn reduced_fixture(fx: &Fixture, choices: &[Vec<usize>]) -> Fixture {
    Fixture {
        budget: fx.budget, pc_thresholds: fx.pc_thresholds.clone(),
        pc_start: fx.pc_start.clone(), ehp: fx.ehp, ehpna: fx.ehpna,
        thp: fx.thp, hp_start: fx.hp_start, weapon: fx.weapon,
        weapon_set: fx.weapon_set,
        guild: fx.guild, fixed: fx.fixed.clone(),
        slots: fx.slots.iter().zip(choices).map(|(s, ix)| Slot {
            name: s.name.clone(), pos: s.pos, is_ring1: false, is_ring2: false,
            pool: ix.iter().map(|&i| s.pool[i].clone()).collect(),
            item_names: ix.iter().map(|&i| s.item_names[i].clone()).collect(),
        }).collect(),
        set_table: fx.set_table.clone(), fixed_names: fx.fixed_names.clone(),
        none_names: fx.none_names.clone(),
        eps: fx.eps,
        // Repairs are heuristic sub-searches: no R20 archive inside them.
        window: 0.0, archive_cap: crate::enumerate::DEFAULT_ARCHIVE_CAP,
    }
}

fn full_choices(fx: &Fixture) -> Vec<Vec<usize>> {
    fx.slots.iter().map(|s| (0..s.pool.len()).collect()).collect()
}

fn indices(fx: &Fixture, e: &TopEntry) -> Option<Vec<usize>> {
    fx.slots.iter().map(|s| s.item_names.iter().position(|n| Some(n) == e.items.get(s.pos))).collect()
}

/// Optimistic ranking is only used to construct seed domains. Actual scores
/// and feasibility come from Search. Non-finite rankings fall back to the
/// existing ordered pool, rather than treating an unsupported ceiling as 0.
fn warm_choices(fx: &Fixture, sc: &ScoringCtx, k: usize) -> Vec<Vec<usize>> {
    let mut base: [&str; 8] = Default::default();
    for (p, n) in fx.none_names.iter().enumerate().take(8) { base[p] = n; }
    for (p, n) in &fx.fixed_names { base[*p] = n; }
    let mut work = crate::scoring::DenseWork::default();
    fx.slots.iter().map(|s| {
        let mut ranked: Vec<_> = s.item_names.iter().enumerate().map(|(i, n)| {
            let mut names = base; names[s.pos] = n;
            let score = sc.dense.as_ref().and_then(|d| {
                crate::scoring::dense_ceiling_with(d, &[], &[], &names, &mut work,
                    &sc.rows, &sc.compiled_rows, &sc.tables, &[150.0; 5])
            }).filter(|v| v.is_finite()).unwrap_or(f64::NEG_INFINITY);
            (i, score)
        }).collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        ranked.into_iter().take(k.max(1)).map(|(i, _)| i).collect()
    }).collect()
}

/// At most six choices in each of six reopened slots: at most 46,656
/// combinations before the existing feasibility checks. Incumbent items
/// are first, two frequent archive items follow, and warm/rand proposals
/// maintain routes outside the learned domain. Remaining positions use
/// archive/warm candidates, then a cyclic scan from a random offset.
///
/// Pool membership and original ring offsets are preserved. Domains are
/// rebuilt from the current archive on every call and are not permanent
/// eliminations. Other operators continue to visit the full original pools.
fn elite_choices(
    fx: &Fixture, base: &[usize], archive: &[TopEntry], warm: &[Vec<usize>],
    order: &[usize], rng: &mut Rng,
) -> (Vec<Vec<usize>>, f64) {
    let parents: Vec<Vec<usize>> = archive.iter().filter_map(|e| indices(fx, e)).collect();
    let mut choices: Vec<Vec<usize>> = base.iter().map(|&i| vec![i]).collect();
    let mut full_tuples = 1.0;
    for &d in order.iter().take(6) {
        let size = fx.slots[d].pool.len();
        full_tuples *= size as f64;
        let cap = size.min(6);
        let mut counts = vec![0u64; size];
        for (rank, parent) in parents.iter().enumerate() {
            // Keep the best parents influential without discarding the
            // diverse part of the archive. Integer weights are portable.
            counts[parent[d]] += if rank < 6 { 3 } else { 1 };
        }
        let mut ranked: Vec<usize> = (0..size).filter(|&i| counts[i] > 0 && i != base[d]).collect();
        ranked.sort_by(|&a, &b| counts[b].cmp(&counts[a]).then_with(|| a.cmp(&b)));
        for &i in ranked.iter().take(2) {
            if choices[d].len() < cap { choices[d].push(i); }
        }
        if let Some(prior) = warm.get(d).filter(|p| !p.is_empty()) {
            let start = rng.index(prior.len());
            for j in 0..prior.len() {
                let i = prior[(start + j) % prior.len()];
                if choices[d].len() < cap && !choices[d].contains(&i) {
                    choices[d].push(i); break;
                }
            }
        }
        // Reserve exploration before filling further archive choices.
        let start = rng.index(size);
        for j in 0..size {
            let i = (start + j) % size;
            if choices[d].len() < cap && !choices[d].contains(&i) {
                choices[d].push(i); break;
            }
        }
        for &i in &ranked {
            if choices[d].len() < cap && !choices[d].contains(&i) { choices[d].push(i); }
        }
        if let Some(prior) = warm.get(d) {
            for &i in prior {
                if choices[d].len() < cap && !choices[d].contains(&i) { choices[d].push(i); }
            }
        }
        for j in 0..size {
            if choices[d].len() == cap { break; }
            let i = (start + j) % size;
            if !choices[d].contains(&i) { choices[d].push(i); }
        }
    }
    (choices, full_tuples)
}

fn run_repair(
    fx: &Fixture, sc: &ScoringCtx, opts: &Options, started: Instant,
    choices: &[Vec<usize>], phase: &str, result: &mut Result,
    sink: &mut Option<&mut dyn FnMut(&QualityPoint)>,
    progress: &mut Option<&mut dyn FnMut(&Progress)>,
) -> Vec<TopEntry> {
    let reduced = reduced_fixture(fx, choices);
    // Canonicalize against the ORIGINAL domain, never the reduced offsets.
    // SP-solver ties can choose different assignments for swapped rings, so
    // allowing extra permutations would compare a different evaluator domain.
    let original_ring_order: Option<HashMap<String, usize>> = match (
        fx.slots.iter().find(|s| s.is_ring1), fx.slots.iter().find(|s| s.is_ring2),
    ) {
        (Some(a), Some(b)) if a.item_names == b.item_names =>
            Some(a.item_names.iter().enumerate().map(|(i, n)| (n.clone(), i)).collect()),
        _ => None,
    };
    let used = result.leaf_calls;
    let repair = result.repairs;
    let credited_before = result.credited_tuples;
    let scored_before = result.scored;
    let completed_before = result.completed_repairs;
    let has_progress = progress.is_some();
    let mut retained = if has_progress { result.top.clone() } else { Vec::new() };
    let mut best = result.trace.last().map(|p| p.best_score).unwrap_or(f64::NEG_INFINITY);
    let mut on_progress = |p: super::ProgressSnapshot| {
        if let Some(e) = p.top_n.first() {
            if e.score.is_finite() && e.score > best {
                best = e.score;
                result.trace.push(QualityPoint { elapsed_secs: started.elapsed().as_secs_f64(),
                    best_score: best, leaf_calls: used + p.leaf_calls, repair,
                    phase: phase.to_string(), entry: e.clone() });
                if let Some(callback) = sink.as_mut() {
                    callback(result.trace.last().unwrap());
                }
            }
        }
        if let Some(callback) = progress.as_mut() {
            super::merge_top_n(&mut retained, p.top_n, opts.top_k);
            callback(&Progress {
                elapsed_secs: started.elapsed().as_secs_f64(),
                leaf_calls: used + p.leaf_calls,
                credited_tuples: credited_before + p.checked,
                scored: scored_before + p.scored, repairs: repair,
                completed_repairs: completed_before,
                phase: phase.into(), stop_reason: String::new(), top: retained.clone(),
            });
        }
        None
    };
    let mut search = Search::new(&reduced);
    search.original_ring_order = original_ring_order.as_ref();
    search.scoring = Some(sc);
    search.quality_trace = opts.quality_trace.clone();
    search.trace_phase = match phase {
        "warm" => "warm", "ordered_seed" => "ordered_seed", "pair" => "pair",
        "triple" => "triple", "perturb" => "perturb", "crossover" => "crossover",
        "restart" => "restart", "elite" => "elite", _ => "repair",
    };
    // Retain alternatives for the diversity archive, not only a local winner.
    search.result_count = opts.archive_size.max(opts.top_k).min(64);
    search.actual_leaf_budget = Some(opts.work_budget.saturating_sub(used));
    search.leaf_budget = Some(opts.repair_budget as f64);
    search.time_cap = Some(opts.seconds);
    search.global_started = Some(started);
    search.progress_every = 256.0;
    search.next_progress = if has_progress { 1.0 } else { 256.0 };
    search.progress_on_first_result = has_progress;
    search.progress = Some(&mut on_progress);
    search.init_equip_names();
    search.run();
    search.emit_progress();
    result.leaf_calls += search.leaf_calls;
    result.credited_tuples += search.checked;
    result.scored += search.scored;
    result.repairs += 1;
    if !search.stop { result.completed_repairs += 1; }
    search.top_n
}

fn expired(opts: &Options, started: Instant, result: &Result) -> Option<&'static str> {
    if started.elapsed().as_secs_f64() >= opts.seconds { Some("time_budget") }
    else if result.leaf_calls >= opts.work_budget { Some("work_budget") }
    else if result.repairs >= opts.max_repairs { Some("repair_limit") }
    else { None }
}

/// Public parsed-fixture entry point. `started` must precede input parsing
/// when measuring end-to-end latency. A wall-limited run is intentionally
/// timing-dependent; use a fixed work/repair budget for seed reproducibility.
pub fn run(fx: &Fixture, sc: &ScoringCtx, opts: &Options, started: Instant) -> Result {
    run_with_trace(fx, sc, opts, started, None)
}

/// As `run`, with an immediately called sink for each observed incumbent.
/// The CLI flushes these points so a host can display partial results or
/// terminate the process without losing already found witnesses.
pub fn run_with_trace(
    fx: &Fixture, sc: &ScoringCtx, opts: &Options, started: Instant,
    sink: Option<&mut dyn FnMut(&QualityPoint)>,
) -> Result {
    run_with_sinks(fx, sc, opts, started, sink, None)
}

/// Streaming progress contains actual retained top-N witnesses, including
/// skill-point and tome choices, even during a long first repair.
pub fn run_with_progress(
    fx: &Fixture, sc: &ScoringCtx, opts: &Options, started: Instant,
    progress: Option<&mut dyn FnMut(&Progress)>,
) -> Result {
    run_with_sinks(fx, sc, opts, started, None, progress)
}

fn run_with_sinks(
    fx: &Fixture, sc: &ScoringCtx, opts: &Options, started: Instant,
    mut sink: Option<&mut dyn FnMut(&QualityPoint)>,
    mut progress: Option<&mut dyn FnMut(&Progress)>,
) -> Result {
    let mut result = Result { setup_secs: started.elapsed().as_secs_f64(), ..Result::default() };
    if fx.slots.iter().any(|s| s.pool.is_empty() || s.pool.len() != s.item_names.len()) {
        result.stop_reason = "empty_or_unnamed_pool".into();
        result.elapsed_secs = started.elapsed().as_secs_f64();
        return result;
    }
    if let (Some(a), Some(b)) = (
        fx.slots.iter().find(|s| s.is_ring1), fx.slots.iter().find(|s| s.is_ring2),
    ) {
        if a.item_names != b.item_names {
            result.stop_reason = "incompatible_original_ring_pools".into();
            result.elapsed_secs = started.elapsed().as_secs_f64();
            return result;
        }
    }
    let mut rng = Rng(opts.seed);
    let mut archive = Vec::new();
    let mut elite_prior = Vec::new();
    let mut seen_domains = HashSet::<Vec<Vec<usize>>>::new();
    // Match the production warm-start ranking first, but retain its witnesses.
    if expired(opts, started, &result).is_none() {
        let warm = warm_choices(fx, sc, opts.warm_k);
        if opts.elite_pool { elite_prior = warm.clone(); }
        if expired(opts, started, &result).is_none() {
            let seed_opts = warm_options(opts);
            let entries = run_repair(fx, sc, &seed_opts, started, &warm, "warm", &mut result, &mut sink, &mut progress);
            super::merge_top_n(&mut result.top, entries.clone(), opts.top_k);
            update_archive(&mut archive, entries, opts.archive_size.max(1));
        }
        seen_domains.insert(warm);
    }
    // A separate ordered-domain probe avoids relying entirely on individual
    // ceilings, which can miss supporting items and mutually useful pairs.
    if expired(opts, started, &result).is_none() {
        let full = full_choices(fx);
        let mut probe = opts.clone();
        probe.repair_budget = opts.repair_budget.min(10_000);
        let entries = run_repair(fx, sc, &probe, started, &full, "ordered_seed", &mut result, &mut sink, &mut progress);
        super::merge_top_n(&mut result.top, entries.clone(), opts.top_k);
        update_archive(&mut archive, entries, opts.archive_size.max(1));
    }
    let n = fx.slots.len();
    let mut stalled = 0usize;
    let mut attempts = 0usize;
    while expired(opts, started, &result).is_none() && n > 0 {
        attempts += 1;
        if attempts > opts.max_repairs.saturating_mul(50).max(100) {
            result.stop_reason = "neighbourhoods_repeated".into(); break;
        }
        let base_ix = if !archive.is_empty() {
            // Exploit the best half the time; otherwise visit diverse regions.
            let a = if rng.index(2) == 0 { 0 } else { rng.index(archive.len()) };
            indices(fx, &archive[a])
        } else { None };
        // Adapt success weights, but force regular restart/exploration trials.
        let (op, forced_diversification) = if opts.elite_pool && n >= 4
            && archive.len() >= 2 && attempts % 5 == 1 {
            (5, false)
        } else { select_operator(&mut rng, base_ix.is_some(), stalled, &result) };
        let base = base_ix.unwrap_or_else(|| fx.slots.iter().map(|s| rng.index(s.pool.len())).collect());
        let mut choices: Vec<Vec<usize>> = base.iter().map(|&i| vec![i]).collect();
        let mut order: Vec<usize> = (0..n).collect(); rng.shuffle(&mut order);
        let mut elite_full_tuples = 0.0;
        match op {
            5 => {
                (choices, elite_full_tuples) = elite_choices(
                    fx, &base, &archive, &elite_prior, &order, &mut rng);
            }
            0 | 1 => {
                let count = if op == 0 { 2 } else { 3 };
                for &d in order.iter().take(count) {
                    choices[d] = (0..fx.slots[d].pool.len()).collect();
                    // Visit the incumbent before widening, then rotate a new
                    // tail on every repair so capped triples explore fairly.
                    choices[d].swap(0, base[d]);
                    rng.shuffle(&mut choices[d][1..]);
                }
            }
            2 => {
                // Coordinated 4+-slot perturbation, finite repair of two
                // candidates per slot; whole-set transitions are possible.
                for &d in order.iter().take(4.max(n / 2)) {
                    let alt = rng.index(fx.slots[d].pool.len());
                    if alt != base[d] { choices[d].push(alt); }
                }
                // Half these moves explicitly propose pieces of one set.
                // This crosses activation barriers without hoping unrelated
                // random mutations happen to select the same set together.
                if rng.index(2) == 0 {
                    let mut sets: Vec<i32> = fx.slots.iter().flat_map(|s| s.pool.iter())
                        .map(|p| p.set_id).filter(|&sid| sid >= 0).collect();
                    sets.sort_unstable(); sets.dedup();
                    if !sets.is_empty() {
                        let sid = sets[rng.index(sets.len())];
                        for d in 0..n {
                            let members: Vec<usize> = fx.slots[d].pool.iter().enumerate()
                                .filter_map(|(i, p)| if p.set_id == sid { Some(i) } else { None }).collect();
                            if !members.is_empty() {
                                let i = members[rng.index(members.len())];
                                choices[d] = vec![base[d]];
                                if i != base[d] { choices[d].push(i); }
                            }
                        }
                    }
                }
            }
            3 => {
                if let Some(other) = archive.get(rng.index(archive.len())) {
                    if let Some(other_ix) = indices(fx, other) {
                        for d in 0..n {
                            if other_ix[d] != base[d] { choices[d].push(other_ix[d]); }
                        }
                    }
                }
                // An additional reopened slot permits repair even when the
                // two parent builds happen to differ in only one item.
                choices[order[0]] = (0..fx.slots[order[0]].pool.len()).collect();
            }
            _ => {
                for d in 0..n {
                    choices[d] = vec![rng.index(fx.slots[d].pool.len())];
                    let second = rng.index(fx.slots[d].pool.len());
                    if second != choices[d][0] { choices[d].push(second); }
                    // Include incumbent support pieces in half of restarts.
                    if rng.index(2) == 0 && !choices[d].contains(&base[d]) { choices[d].push(base[d]); }
                }
            }
        }
        // The key preserves order: two capped searches over different orders
        // are different experiments, whereas an identical domain is wasted.
        if !seen_domains.insert(choices.clone()) { continue; }
        if seen_domains.len() > 4096 { seen_domains.clear(); }
        if op == 5 {
            result.elite_calls += 1;
            result.elite_full_domain_tuples += elite_full_tuples;
            result.elite_restricted_domain_tuples += choices.iter().map(|d| d.len() as f64).product::<f64>();
        } else { result.operator_calls[op] += 1; }
        let old = result.top.first().map(|e| e.score).unwrap_or(f64::NEG_INFINITY);
        let phase = ["pair", "triple", "perturb", "crossover", "restart", "elite"][op];
        let leaves_before = result.leaf_calls;
        let entries = run_repair(fx, sc, opts, started, &choices, phase, &mut result, &mut sink, &mut progress);
        super::merge_top_n(&mut result.top, entries.clone(), opts.top_k);
        update_archive(&mut archive, entries, opts.archive_size.max(1));
        let new = result.top.first().map(|e| e.score).unwrap_or(f64::NEG_INFINITY);
        if op == 5 {
            result.elite_leaf_calls += result.leaf_calls - leaves_before;
            if new > old { result.elite_improvements += 1; }
        } else if new > old { result.operator_improvements[op] += 1; }
        // Reset only after an executed diversification. A repeated domain
        // skipped above must not consume this recovery step.
        stalled = stall_after_repair(stalled, new > old, forced_diversification, opts.cycle_stagnation);
    }
    if result.stop_reason.is_empty() {
        result.stop_reason = expired(opts, started, &result).unwrap_or("no_free_slots").into();
    }
    result.elapsed_secs = started.elapsed().as_secs_f64();
    if let Some(callback) = progress.as_mut() { callback(&progress_from_result(&result, "finished")); }
    result
}

/// Bounded browser defaults are explicit and do not change native CLI
/// defaults used by the archived benchmark comparisons.
pub fn browser_options(options_json: &str) -> std::result::Result<Options, String> {
    let value: Value = serde_json::from_str(options_json).map_err(|e| format!("invalid anytime options: {e}"))?;
    let object = value.as_object().ok_or("anytime options must be a JSON object")?;
    let allowed = ["seconds", "seed", "work_budget", "repair_budget", "warm_budget",
        "cycle_stagnation", "elite_pool", "max_repairs", "top_k", "archive_size", "warm_k"];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) { return Err(format!("unknown anytime option: {key}")); }
    }
    let mut opts = Options { seconds: 5.0, top_k: 15, warm_k: 6,
        warm_budget: Some(2_000_000), repair_budget: 100_000,
        max_repairs: 10_000, cycle_stagnation: true, ..Options::default() };
    if let Some(v) = object.get("seconds") {
        opts.seconds = v.as_f64().filter(|n| n.is_finite() && (0.0..=300.0).contains(n))
            .ok_or("seconds must be a finite number in 0..300")?;
    }
    let integer = |key: &str, min: u64, max: u64| -> std::result::Result<Option<u64>, String> {
        object.get(key).map(|value| {
            // JSON emitted by JavaScript may write integral values as 1.0.
            value.as_f64().filter(|n| n.is_finite() && n.fract() == 0.0
                && *n >= min as f64 && *n <= max as f64).map(|n| n as u64)
                .ok_or_else(|| format!("{key} must be an integer in {min}..{max}"))
        }).transpose()
    };
    if let Some(v) = integer("seed", 0, 9_007_199_254_740_991)? { opts.seed = v; }
    if let Some(v) = integer("work_budget", 0, 9_007_199_254_740_991)? { opts.work_budget = v; }
    if let Some(v) = integer("repair_budget", 1, 10_000_000)? { opts.repair_budget = v; }
    if let Some(v) = integer("warm_budget", 1, 10_000_000)? { opts.warm_budget = Some(v); }
    if let Some(v) = integer("max_repairs", 1, 100_000)? { opts.max_repairs = v as usize; }
    if let Some(v) = integer("top_k", 1, 15)? { opts.top_k = v as usize; }
    if let Some(v) = integer("archive_size", 1, 64)? { opts.archive_size = v as usize; }
    if let Some(v) = integer("warm_k", 1, 64)? { opts.warm_k = v as usize; }
    if let Some(v) = object.get("cycle_stagnation") {
        opts.cycle_stagnation = v.as_bool().ok_or("cycle_stagnation must be a boolean")?;
    }
    if let Some(v) = object.get("elite_pool") {
        opts.elite_pool = v.as_bool().ok_or("elite_pool must be a boolean")?;
    }
    opts.archive_size = opts.archive_size.max(opts.top_k);
    Ok(opts)
}

/// Platform-neutral browser entry point, callable natively for differential
/// tests. The deadline starts before parsing and scoring-plan preparation.
/// Callbacks receive JSON with a full retained `top_n`, never a lone delta.
pub fn solve_json_with_progress(
    enum_fixture: &str, score_fixture: &str, options_json: &str,
    mut callback: Option<&mut dyn FnMut(&str)>,
) -> String {
    let started = Instant::now();
    let error = |message: String| json!({ "algorithm": "alns", "complete": false,
        "error": message, "top": [], "top_n": [] }).to_string();
    let opts = match browser_options(options_json) { Ok(o) => o, Err(e) => return error(e) };
    if enum_fixture.trim().is_empty() { return error("missing enum fixture".into()); }
    let sc = match serde_json::from_str::<Value>(score_fixture).map_err(|e| e.to_string())
        .and_then(|v| ScoringCtx::load(&v)) {
        Ok(sc) => sc, Err(e) => return error(e),
    };
    let fx = super::parse_fixture(enum_fixture);
    // Publish the first feasible result promptly. Thereafter permit up to
    // 20 updates/second, or an immediate update for a new best score. The
    // internal search still checks its own work and elapsed-time budgets.
    let mut last_elapsed = f64::NEG_INFINITY;
    let mut last_best = f64::NEG_INFINITY;
    let mut sink = |p: &Progress| {
        let best = p.top.first().map(|e| e.score).unwrap_or(f64::NEG_INFINITY);
        if p.phase == "finished" || best > last_best || p.elapsed_secs - last_elapsed >= 0.05 {
            last_elapsed = p.elapsed_secs;
            last_best = best;
            if let Some(cb) = callback.as_mut() { cb(&p.json(&opts).to_string()); }
        }
    };
    let result = run_with_progress(&fx, &sc, &opts, started, Some(&mut sink));
    let mut output = result.json(&opts);
    output["top_n"] = progress_from_result(&result, "finished").json(&opts)["top_n"].clone();
    output.to_string()
}

pub fn cli_main() -> std::result::Result<(), String> {
    let started = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 { return Err("usage: anytime_kernel ENUM.txt SCORE.json [--seconds N] [--seed N] [--work-budget N] [--repair-budget N] [--warm-budget N] [--cycle-stagnation 0|1] [--elite-pool 0|1] [--max-repairs N] [--top-k N] [--warm-k N] [--trace PATH]".into()); }
    let mut opts = Options::default();
    let mut trace_path = None;
    let mut i = 2;
    while i < args.len() {
        let val = args.get(i + 1).ok_or_else(|| format!("missing value for {}", args[i]))?;
        match args[i].as_str() {
            "--seconds" => opts.seconds = val.parse().map_err(|_| "invalid seconds")?,
            "--seed" => opts.seed = val.parse().map_err(|_| "invalid seed")?,
            "--work-budget" => opts.work_budget = val.parse().map_err(|_| "invalid work budget")?,
            "--repair-budget" => opts.repair_budget = val.parse().map_err(|_| "invalid repair budget")?,
            "--warm-budget" => opts.warm_budget = Some(val.parse().map_err(|_| "invalid warm budget")?),
            "--cycle-stagnation" => opts.cycle_stagnation = match val.as_str() {
                "0" => false, "1" => true, _ => return Err("cycle-stagnation must be 0 or 1".into()),
            },
            "--elite-pool" => opts.elite_pool = match val.as_str() {
                "0" => false, "1" => true, _ => return Err("elite-pool must be 0 or 1".into()),
            },
            "--max-repairs" => opts.max_repairs = val.parse().map_err(|_| "invalid repair count")?,
            "--top-k" => opts.top_k = val.parse().map_err(|_| "invalid top-k")?,
            "--warm-k" => opts.warm_k = val.parse().map_err(|_| "invalid warm-k")?,
            "--trace" => trace_path = Some(val.clone()),
            k => return Err(format!("unknown option {k}")),
        }
        i += 2;
    }
    if !opts.seconds.is_finite() || opts.seconds < 0.0 || opts.top_k == 0 || opts.top_k > 64
        || opts.repair_budget == 0 || opts.warm_budget == Some(0) || opts.max_repairs == 0 || opts.warm_k == 0 {
        return Err("seconds must be finite and >=0, top-k 1..64, and repair limits/warm-k/warm-budget positive".into());
    }
    opts.quality_trace = trace_path.map(|path| super::QualityTrace::new(&path, started)
        .map(Arc::new)).transpose().map_err(|e| e.to_string())?;
    let enum_text = std::fs::read_to_string(&args[0]).map_err(|e| e.to_string())?;
    let score_text = std::fs::read_to_string(&args[1]).map_err(|e| e.to_string())?;
    let fx = super::parse_fixture(&enum_text);
    let score_json: Value = serde_json::from_str(&score_text).map_err(|e| e.to_string())?;
    let sc = ScoringCtx::load(&score_json)?;
    let result = run(&fx, &sc, &opts, started);
    if let Some(trace) = &opts.quality_trace { trace.finish(false, 0.0); }
    println!("{}", result.json(&opts));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Unit, enumerate::PoolItem};

    fn synthetic(rings: bool) -> (Fixture, ScoringCtx) {
        synthetic_shape(rings, false)
    }

    fn synthetic_shape(rings: bool, four_slot_barrier: bool) -> (Fixture, ScoringCtx) {
        let none_names: Vec<String> = (0..8).map(|i| format!("none{i}")).collect();
        let mut registry = serde_json::Map::new();
        for n in &none_names {
            registry.insert(n.clone(), json!({ "__m": { "displayName": n, "reqs": vec![0;5],
                "skillpoints": vec![0;5], "hp": 0, "maxRolls": { "__m": {} } } }));
        }
        let make = |hp: f64, req: i32, skp: i32| PoolItem {
            crafted: false, reqs: [req, 0, 0, 0, 0], skp: [skp, 0, 0, 0, 0],
            set_id: -1, illegal_id: -1, hp, pc: Vec::new(),
        };
        let mut defs = if rings {
            vec![("low", make(10.0, 0, 0)), ("high", make(100.0, 0, 0))]
        } else {
            vec![("helmet0", make(100.0, 0, 0)), ("helmet1", make(1000.0, if four_slot_barrier { 30 } else { 10 }, 0)),
                 ("support0", make(100.0, 0, 0)), ("support1", make(-10.0, 0, 10))]
        };
        if four_slot_barrier {
            defs.extend([("support20", make(100.0, 0, 0)), ("support21", make(-10.0, 0, 10)),
                         ("support30", make(100.0, 0, 0)), ("support31", make(-10.0, 0, 10))]);
        }
        for (n, p) in &defs {
            registry.insert((*n).into(), json!({ "__m": { "displayName": n, "reqs": p.reqs,
                "skillpoints": p.skp, "hp": p.hp, "maxRolls": { "__m": {} } } }));
        }
        let slots = (0..if four_slot_barrier { 4 } else { 2 }).map(|d| {
            let start = if rings { 0 } else { d * 2 };
            Slot { name: format!("slot{d}"), pos: if rings { d + 4 } else { d },
                is_ring1: rings && d == 0, is_ring2: rings && d == 1,
                pool: defs[start..start+2].iter().map(|(_, p)| p.clone()).collect(),
                item_names: defs[start..start+2].iter().map(|(n, _)| (*n).into()).collect() }
        }).collect();
        let fx = Fixture { budget: 0, pc_thresholds: vec![], pc_start: vec![],
            ehp: None, ehpna: None, thp: None, hp_start: 0.0, weapon: Unit::default(),
            guild: None, fixed: vec![], slots, set_table: vec![], fixed_names: vec![],
            none_names: none_names.clone(), weapon_set: -1, eps: 0.0,
            window: 0.0, archive_cap: crate::enumerate::DEFAULT_ARCHIVE_CAP };
        let score = json!({
            "meta": { "scoring_target": "total_hp" },
            "weapon_sm": { "__m": { "reqs": vec![0;5], "skillpoints": vec![0;5], "type": "wand",
                "atkSpd": "NORMAL", "maxRolls": { "__m": {} } } },
            "parsed_combo": [], "boost_registry": [],
            "tables": { "skillpoint_damage_mult": vec![1.0;5],
                "baseDamageMultiplier": vec![1.0;7],
                "attackSpeeds": ["SUPER_SLOW", "VERY_SLOW", "SLOW", "NORMAL", "FAST", "VERY_FAST", "SUPER_FAST"],
                "damage_keys": ["nDam", "eDam", "tDam", "wDam", "fDam", "aDam"],
                "sp_percentage_rate": 0.9908, "sp_percentage_input_cap": 150,
                "skillpoint_final_mult": vec![1.0;5] },
            "layer2": { "item_registry": registry,
                "scaling_plan": { "kind": "cached", "scaled": { "__m": {} } },
                "sp_budget": 0, "combo_time": 0,
                "constants": { "statmap_static_ids": ["hp"],
                    "statmap_must_ids": ["hpBonus"], "hp_base_for_level": 100,
                    "class_def": { "wand": 1.0 },
                    "skp_order": ["str", "dex", "int", "def", "agi"],
                    "base_mana_regen": 5.0, "mana_tick_seconds": 5.0,
                    "spell_cast_time": 0.5, "spell_cast_delay": 0.0 } }
        });
        (fx, ScoringCtx::load(&score).unwrap())
    }

    fn complete_repair(fx: &Fixture, sc: &ScoringCtx, choices: &[Vec<usize>]) -> Vec<TopEntry> {
        let opts = Options { seconds: 60.0, repair_budget: 10_000, ..Options::default() };
        let mut out = Result::default();
        let entries = run_repair(fx, sc, &opts, Instant::now(), choices, "test", &mut out, &mut None, &mut None);
        assert_eq!(out.completed_repairs, 1);
        entries
    }

    #[test]
    fn joint_pair_crosses_sp_support_barrier_and_matches_full_search() {
        let (fx, sc) = synthetic(false);
        let base = complete_repair(&fx, &sc, &[vec![0], vec![0]]);
        let left = complete_repair(&fx, &sc, &[vec![0, 1], vec![0]]);
        let right = complete_repair(&fx, &sc, &[vec![0], vec![0, 1]]);
        let joint = complete_repair(&fx, &sc, &[vec![0, 1], vec![0, 1]]);
        assert_eq!(base[0].score, 300.0);
        assert_eq!(left[0].score, base[0].score);
        assert_eq!(right[0].score, base[0].score);
        assert_eq!(joint[0].score, 1090.0);
        let mut exhaustive = Search::new(&fx);
        exhaustive.scoring = Some(&sc); exhaustive.init_equip_names(); exhaustive.run();
        assert_eq!(joint[0].score, exhaustive.top_n[0].score);
        assert_eq!(joint[0].items, exhaustive.top_n[0].items);
    }

    #[test]
    fn unequal_ring_pools_preserve_crossed_choices_and_legal_duplicates() {
        let (fx, sc) = synthetic(true);
        let both = complete_repair(&fx, &sc, &[vec![0, 1], vec![0, 1]]);
        assert_eq!(both[0].score, 300.0);
        assert_eq!(both[0].items[4], "high");
        assert_eq!(both[0].items[5], "high");
        let frozen_high = complete_repair(&fx, &sc, &[vec![1], vec![0]]);
        assert!(frozen_high.is_empty(), "swapped original ring order must not add an SP tie variant");
        let frozen_low = complete_repair(&fx, &sc, &[vec![0], vec![1, 0]]);
        assert_eq!(frozen_low[0].score, 210.0);
        let mut only_one_original_free_ring = fx;
        only_one_original_free_ring.slots[0].is_ring1 = false;
        let allowed = complete_repair(&only_one_original_free_ring, &sc, &[vec![1], vec![0]]);
        assert_eq!(allowed[0].score, 210.0, "an originally locked ring must not impose symmetry");
        let mut merged = Vec::new();
        update_archive(&mut merged, both, 24);
        assert_eq!(merged.len(), 3, "ordered ring permutations must deduplicate");
        update_archive(&mut merged, frozen_high, 24);
        update_archive(&mut merged, frozen_low, 24);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn work_limited_seed_reproduces_and_budget_is_actual_leaf_calls() {
        let (fx, sc) = synthetic(false);
        let opts = Options { seconds: 60.0, seed: 19, work_budget: 12,
            repair_budget: 4, max_repairs: 12, warm_k: 1, ..Options::default() };
        let a = run(&fx, &sc, &opts, Instant::now());
        let b = run(&fx, &sc, &opts, Instant::now());
        assert_eq!(a.top[0].score, b.top[0].score);
        assert_eq!(a.top[0].items, b.top[0].items);
        assert_eq!(a.operator_calls, b.operator_calls);
        assert_eq!(a.leaf_calls, b.leaf_calls);
        assert!(a.leaf_calls <= opts.work_budget);
        assert_eq!(a.json(&opts)["complete"], false);
    }

    #[test]
    fn elite_recombination_crosses_four_slot_barrier_with_full_scorer() {
        let (fx, sc) = synthetic_shape(false, true);
        let base = vec![0usize; 4];
        let initial = complete_repair(&fx, &sc, &base.iter().map(|&i| vec![i]).collect::<Vec<_>>());
        assert_eq!(initial[0].score, 500.0);
        let mut archive = initial.clone();
        // Every subspace changing at most three slots is no better. The
        // expensive helmet needs ALL three individually harmful supports.
        for mask in 0u32..15 {
            let choices: Vec<_> = (0..4).map(|d| if mask & (1 << d) != 0 { vec![0, 1] } else { vec![0] }).collect();
            let entries = complete_repair(&fx, &sc, &choices);
            assert!(entries[0].score <= initial[0].score);
            update_archive(&mut archive, entries, 24);
        }
        let (choices, full) = elite_choices(&fx, &base, &archive, &vec![vec![0, 1]; 4], &[0, 1, 2, 3], &mut Rng(707));
        assert_eq!(full, 16.0);
        assert!(choices.iter().all(|domain| domain == &vec![0, 1]));
        let repaired = complete_repair(&fx, &sc, &choices);
        let exhaustive = complete_repair(&fx, &sc, &full_choices(&fx));
        assert_eq!(repaired[0].score, 1070.0);
        assert_eq!(repaired[0].score, exhaustive[0].score);
        assert_eq!(repaired[0].items, exhaustive[0].items);
        assert_eq!(repaired[0].assigned_sp, 0);
    }

    #[test]
    fn elite_domains_are_bounded_deduplicated_and_preserve_incumbent() {
        let (mut fx, _) = synthetic_shape(false, true);
        while fx.slots.len() < 8 {
            fx.slots.push(Slot { name: format!("slot{}", fx.slots.len()), pos: fx.slots.len(),
                is_ring1: false, is_ring2: false, pool: fx.slots[0].pool.clone(),
                item_names: fx.slots[0].item_names.clone() });
        }
        for (d, slot) in fx.slots.iter_mut().enumerate() {
            slot.pool = vec![slot.pool[0].clone(); 200];
            slot.item_names = (0..200).map(|i| format!("item{d}_{i}")).collect();
        }
        let base = vec![19; 8];
        let incumbent = TopEntry { score: 100.0, items: (0..8).map(|d| format!("item{d}_19")).collect(), ..TopEntry::default() };
        let mut archive = vec![incumbent];
        for i in 0..12 { archive.push(TopEntry { score: 99.0 - i as f64,
            items: (0..8).map(|d| format!("item{d}_{i}")).collect(), ..TopEntry::default() }); }
        let warm = vec![vec![40, 41, 42, 43, 44, 45]; 8];
        let order: Vec<_> = (0..8).collect();
        let (a, full) = elite_choices(&fx, &base, &archive, &warm, &order, &mut Rng(909));
        let (b, _) = elite_choices(&fx, &base, &archive, &warm, &order, &mut Rng(909));
        assert_eq!(a, b);
        assert_eq!(full, 200f64.powi(6));
        assert_eq!(a.iter().map(|d| d.len()).product::<usize>(), 46_656);
        for (d, domain) in a.iter().enumerate() {
            assert_eq!(domain[0], base[d]);
            assert!(domain.iter().all(|&i| i < 200));
            assert_eq!(domain.iter().copied().collect::<HashSet<_>>().len(), domain.len());
            if d >= 6 { assert_eq!(domain.len(), 1); }
        }
    }

    #[test]
    fn elite_fixed_work_reproduces_and_off_keeps_legacy_path() {
        let (fx, sc) = synthetic_shape(false, true);
        let opts = Options { seconds: 60.0, seed: 19, work_budget: 64,
            repair_budget: 4, max_repairs: 50, warm_k: 1, elite_pool: true, ..Options::default() };
        let a = run(&fx, &sc, &opts, Instant::now());
        let b = run(&fx, &sc, &opts, Instant::now());
        assert_eq!(a.top[0].score, b.top[0].score);
        assert_eq!(a.top[0].items, b.top[0].items);
        assert_eq!(a.leaf_calls, b.leaf_calls);
        assert_eq!(a.operator_calls, b.operator_calls);
        assert_eq!(a.elite_calls, b.elite_calls);
        assert!(a.elite_calls > 0);
        assert!(a.elite_leaf_calls <= a.leaf_calls && a.leaf_calls <= opts.work_budget);
        assert_eq!(a.json(&opts)["complete"], false);
        assert!(!Options::default().elite_pool);
        let off = run(&fx, &sc, &Options { elite_pool: false, ..opts.clone() }, Instant::now());
        assert_eq!(off.elite_calls, 0);
        assert_eq!(off.elite_leaf_calls, 0);
        assert!(!browser_options("{}").unwrap().elite_pool);
        assert!(browser_options("{\"elite_pool\":true}").unwrap().elite_pool);
        assert!(browser_options("{\"elite_pool\":1}").is_err());
    }

    #[test]
    fn zero_wall_and_work_budgets_do_not_start_repair() {
        let (fx, sc) = synthetic(false);
        let wall = run(&fx, &sc, &Options { seconds: 0.0, ..Options::default() }, Instant::now());
        assert_eq!(wall.repairs, 0); assert_eq!(wall.leaf_calls, 0);
        assert_eq!(wall.stop_reason, "time_budget");
        let work = run(&fx, &sc, &Options { seconds: 60.0, work_budget: 0,
            ..Options::default() }, Instant::now());
        assert_eq!(work.repairs, 0); assert_eq!(work.leaf_calls, 0);
        assert_eq!(work.stop_reason, "work_budget");
    }

    #[test]
    fn archive_preserves_diversity_and_best_score() {
        let mut base = TopEntry { score: 100.0, items: (0..8).map(|i| format!("item{i}")).collect(),
            ..TopEntry::default() };
        let mut candidates = vec![base.clone()];
        for i in 0..12 {
            let mut e = base.clone(); e.score -= 1.0 + i as f64;
            e.items[0] = format!("helmet{i}"); candidates.push(e);
        }
        base.score = 50.0; base.items = (0..8).map(|i| format!("other{i}")).collect();
        candidates.push(base.clone());
        let mut archive = Vec::new(); update_archive(&mut archive, candidates, 4);
        assert_eq!(archive.len(), 4); assert_eq!(archive[0].score, 100.0);
        assert!(archive.iter().any(|e| e.items == base.items));
    }

    #[test]
    fn reduced_domains_preserve_fixed_sets_and_illegal_collisions() {
        let (mut fx, _) = synthetic(false);
        // One locked set piece plus one singleton repair piece supplies the
        // 10 STR needed by the other selected item, with no manual budget.
        fx.fixed.push((2, Unit::default(), 0, -1));
        fx.fixed_names.push((2, "locked_set_piece".into()));
        fx.slots[1].pool[1].skp = [0; 5];
        fx.slots[1].pool[1].set_id = 0;
        fx.set_table = vec![vec![[0; 5], [10, 0, 0, 0, 0]]];
        let choices = [vec![1], vec![1]];
        let count = |f: &Fixture| {
            let r = reduced_fixture(f, &choices);
            let mut search = Search::new(&r); search.run(); search.feasible
        };
        assert_eq!(count(&fx), 1, "frozen pieces must retain reachable set SP");
        fx.set_table[0][1] = [-10, 0, 0, 0, 0];
        assert_eq!(count(&fx), 0, "negative cumulative set tiers must also be retained");
        fx.set_table[0][1] = [10, 0, 0, 0, 0];
        fx.fixed[0].3 = 0;
        fx.slots[1].pool[1].illegal_id = 0;
        assert_eq!(count(&fx), 0, "a fixed exclusive piece must block a conflicting free piece");
    }

    #[test]
    fn cyclic_stagnation_returns_to_local_operators_without_global_improvement() {
        let simulate = |cyclic: bool| {
            let mut rng = Rng(19);
            let mut state = Result { repairs: 2, ..Result::default() };
            let mut stalled = 8;
            let mut ops = Vec::new();
            for _ in 0..64 {
                let (op, forced) = select_operator(&mut rng, true, stalled, &state);
                ops.push((op, forced));
                state.operator_calls[op] += 1;
                state.repairs += 1;
                // Deliberately no new global best in any of these repairs.
                stalled = stall_after_repair(stalled, false, forced, cyclic);
            }
            ops
        };
        let legacy = simulate(false);
        assert!(legacy.iter().all(|(op, forced)| matches!(op, 2 | 4) && *forced));
        let cyclic = simulate(true);
        assert!(matches!(cyclic[0].0, 2 | 4) && cyclic[0].1);
        assert!(!cyclic[1].1, "the next completed repair must again allow local selection");
        assert!(cyclic.iter().any(|(op, _)| matches!(op, 0 | 1 | 3)));
        assert!(cyclic.iter().skip(1).any(|(_, forced)| *forced), "diversification must recur");
        assert_eq!(cyclic, simulate(true), "scheduler must be reproducible at a fixed seed");
        assert!(!Options::default().cycle_stagnation, "preserve the A/B baseline default");
    }

    #[test]
    fn cyclic_reset_requires_forced_diversification_or_an_improvement() {
        assert_eq!(stall_after_repair(8, false, true, true), 0);
        assert_eq!(stall_after_repair(8, false, true, false), 9);
        assert_eq!(stall_after_repair(7, false, false, true), 8);
        assert_eq!(stall_after_repair(12, true, false, false), 0);
        let mut rng = Rng(1);
        let state = Result { repairs: 3, ..Result::default() };
        let (op, forced) = select_operator(&mut rng, false, 12, &state);
        assert_eq!(op, 4);
        assert!(!forced, "with no feasible base the search must keep trying to establish one");
    }

    #[test]
    fn warm_budget_is_independent_and_still_obeys_global_work_limit() {
        let (fx, sc) = synthetic(false);
        let opts = Options { seconds: 60.0, warm_k: 2, repair_budget: 1,
            max_repairs: 1, ..Options::default() };
        let old = run(&fx, &sc, &opts, Instant::now());
        assert_eq!(old.repairs, 1);
        assert_eq!(old.credited_tuples, 1.0);
        assert_eq!(warm_options(&opts).repair_budget, opts.repair_budget);
        let expanded = Options { warm_budget: Some(4), ..opts.clone() };
        let larger = run(&fx, &sc, &expanded, Instant::now());
        assert_eq!(larger.repairs, 1);
        assert_eq!(larger.credited_tuples, 4.0);
        assert_eq!(larger.top[0].score, 1090.0);
        assert_eq!(expanded.repair_budget, 1, "normal repair budget must remain unchanged");
        let capped = Options { work_budget: 1, ..expanded };
        let cap_result = run(&fx, &sc, &capped, Instant::now());
        assert!(cap_result.leaf_calls <= 1, "an expanded seed cannot bypass the global actual-work cap");
    }

    #[test]
    fn browser_options_reject_unbounded_and_ambiguous_inputs() {
        for invalid in ["null", "[]", "{\"seconds\":-1}", "{\"seconds\":301}",
            "{\"seconds\":1e999}", "{\"top_k\":16}", "{\"top_k\":0}",
            "{\"work_budget\":-1}", "{\"work_budget\":1.5}",
            "{\"seed\":9007199254740992}", "{\"max_repairs\":0}",
            "{\"cycle_stagnation\":1}", "{\"warm_budget\":0}", "{\"typo\":1}"] {
            assert!(browser_options(invalid).is_err(), "accepted {invalid}");
        }
        let opts = browser_options("{\"seconds\":0,\"work_budget\":0,\"seed\":1.0}").unwrap();
        assert_eq!(opts.seconds, 0.0); assert_eq!(opts.work_budget, 0); assert_eq!(opts.seed, 1);
        assert_eq!(opts.top_k, 15); assert_eq!(opts.warm_k, 6);
        assert!(opts.cycle_stagnation);
        assert_eq!(Options::default().top_k, 1, "native CLI defaults must stay unchanged");
    }

    #[test]
    fn streaming_retains_full_witnesses_and_does_not_change_fixed_work_search() {
        let (fx, sc) = synthetic(false);
        let opts = Options { seconds: 60.0, seed: 19, work_budget: 12,
            repair_budget: 4, max_repairs: 12, top_k: 3, warm_k: 1, ..Options::default() };
        let expected = run(&fx, &sc, &opts, Instant::now());
        let mut updates = Vec::new();
        let mut sink = |p: &Progress| updates.push(p.clone());
        let actual = run_with_progress(&fx, &sc, &opts, Instant::now(), Some(&mut sink));
        assert_eq!(actual.json(&opts)["top"], expected.json(&opts)["top"],
            "callbacks must not change retained witnesses");
        assert_eq!(actual.leaf_calls, expected.leaf_calls);
        assert_eq!(actual.operator_calls, expected.operator_calls);
        assert!(actual.leaf_calls <= opts.work_budget);
        let first = updates.iter().find(|p| !p.top.is_empty()).unwrap();
        assert_eq!(first.leaf_calls, 1, "publish the first scored witness before a repair ends");
        assert!(first.repairs < actual.repairs);
        assert_ne!(first.phase, "finished");
        for pair in updates.windows(2) {
            assert!(pair[1].leaf_calls >= pair[0].leaf_calls);
            assert!(pair[1].credited_tuples >= pair[0].credited_tuples);
            if let Some(previous) = pair[0].top.first() {
                assert!(pair[1].top.first().unwrap().score >= previous.score);
            }
        }
        let final_update = updates.last().unwrap();
        assert_eq!(final_update.json(&opts)["top_n"],
            progress_from_result(&actual, "finished").json(&opts)["top_n"]);
        assert_eq!(final_update.phase, "finished");
        assert!(final_update.top.len() > 1, "stream the full retained top-N, not only one winner");
        let best = final_update.top.first().unwrap();
        assert_eq!(best.score, 1090.0);
        assert_eq!(best.base_sp, [0; 5]);
        assert_eq!(best.total_sp, [10, 0, 0, 0, 0]);
        let payload = final_update.json(&opts);
        assert_eq!(payload["complete"], false);
        assert_eq!(payload["top_n"][0]["total_sp"], json!(best.total_sp));
        assert!(payload.get("total").is_none(), "overlapping repairs have no completion percentage");
    }

    #[test]
    fn streamed_tome_and_manual_assignment_are_the_retained_witness() {
        let entry = TopEntry { score: 123.0, items: (0..8).map(|i| format!("item{i}")).collect(),
            base_sp: [1, 2, 3, 4, 5], total_sp: [11, 12, 13, 14, 15], assigned_sp: 15,
            tome: Some(crate::scoring::TomeChoice { guild_idx: 3,
                weapon_names: vec!["weapon tome".into()], armor_names: vec!["armor tome".into()] }) };
        let result = Result { top: vec![entry], ..Result::default() };
        let payload = progress_from_result(&result, "test").json(&Options::default());
        assert_eq!(payload["top_n"][0]["base_sp"], json!([1, 2, 3, 4, 5]));
        assert_eq!(payload["top_n"][0]["assigned_sp"], 15);
        assert_eq!(payload["top_n"][0]["tome"]["guild_idx"], 3);
        assert_eq!(payload["top_n"][0]["tome"]["weaponTome"], json!(["weapon tome"]));
        assert_eq!(payload["top_n"][0]["tome"]["armorTome"], json!(["armor tome"]));
    }
}
