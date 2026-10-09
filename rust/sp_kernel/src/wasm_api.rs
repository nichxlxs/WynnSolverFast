//! Browser entry point (cargo feature `wasm`).
//!
//! Exposes the same engine the native CLI runs — identical enumeration,
//! bounds and scoring — to JavaScript. Inputs are the two fixture payloads
//! as strings (the browser fetches or generates them; there is no
//! filesystem in wasm), and the result is JSON.
//!
//! Exact search retains its deterministic leaf-budget API. Anytime search
//! additionally accepts a wall-time budget using the worker's monotonic
//! `performance.now()` clock. Both run inside a dedicated worker so the
//! page remains responsive and the host can cancel by terminating it.

use wasm_bindgen::prelude::*;

/// Solve a scenario and return results as a JSON string. Thin bindgen shim
/// over `enumerate::solve_json`, which is also callable natively so the
/// browser path can be tested without a browser.
///
/// `max_leaves <= 0` means "run to completion".
#[wasm_bindgen]
pub fn solve(enum_fixture: &str, score_fixture: &str, max_leaves: f64) -> String {
    crate::enumerate::solve_json(enum_fixture, score_fixture, max_leaves)
}

/// Total canonical search size for a fixture, so the UI can show progress
/// without starting a solve.
#[wasm_bindgen]
pub fn search_space(enum_fixture: &str) -> f64 {
    let fx = crate::enumerate::parse_fixture(enum_fixture);
    crate::enumerate::Search::new(&fx).total_space_of()
}

/// `solve` with a live-progress callback.
///
/// `on_progress` is invoked with a JSON string (checked/total, the funnel
/// counters, and the interim top-N) roughly every 2M credited leaves and
/// once more when the search ends. This is what lets a long solve show
/// movement in the UI instead of appearing hung — the reason to run this in
/// a dedicated worker rather than chunking on the main thread.
///
/// Exact-mode emission is keyed on credited leaves, preserving its existing
/// progress behavior and deterministic emission points.
#[wasm_bindgen]
pub fn solve_with_progress(
    enum_fixture: &str, score_fixture: &str, max_leaves: f64, on_progress: &js_sys::Function,
) -> String {
    solve_partition(enum_fixture, score_fixture, max_leaves, 0, 1, on_progress)
}

/// `solve_with_progress` restricted to one partition of the search space.
///
/// wasm threads need `SharedArrayBuffer` and COOP/COEP cross-origin
/// isolation, which the app cannot assume. Partitioning needs neither: the
/// host spawns one ordinary worker per core, each running this with its own
/// `part_index`, and merges the results. The split is by first-slot offset —
/// the same one the native threaded path work-steals over — so the
/// partitions are disjoint, `checked` sums to the whole space, and the
/// merged top-N is identical to a single-partition run (`partition_check`).
///
/// Each partition still reports the FULL space as `total`, so a host summing
/// `checked` across workers gets a coherent percentage.
///
/// The one thing lost versus native threads is the shared score cutoff: each
/// partition discovers its own, so the gate prunes a little less. That costs
/// work, never results.
#[wasm_bindgen]
pub fn solve_partition(
    enum_fixture: &str, score_fixture: &str, max_leaves: f64,
    part_index: usize, part_count: usize, on_progress: &js_sys::Function,
) -> String {
    let mut sink = progress_sink(on_progress);
    crate::enumerate::solve_json_full(
        enum_fixture, score_fixture, max_leaves, Some(&mut sink), part_index, part_count,
    )
}

/// R24: one worker's engine for the work-queue solve.
///
/// Browser workers cannot share a cutoff without `SharedArrayBuffer`, and a
/// running solve cannot receive messages, so `solve_partition` workers each
/// rediscover their own cutoff. An `Engine` instead parses the fixtures,
/// builds the bound tables and runs the warm start once, then solves units
/// (contiguous first-slot offset ranges) one call at a time. Between calls
/// the host hands the worker its next unit together with the merged cutoff
/// of everything every worker has finished, so later units prune against
/// the best builds found anywhere. Results accumulate across a worker's
/// units: each call returns the solve JSON of all of them so far, in the
/// same shape `solve_partition` returns, and progress is cumulative too.
#[wasm_bindgen]
pub struct Engine {
    inner: crate::enumerate::EngineSession,
}

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new(enum_fixture: &str, score_fixture: &str) -> Result<Engine, JsValue> {
        crate::enumerate::EngineSession::new(enum_fixture, score_fixture)
            .map(|inner| Engine { inner })
            .map_err(|e| JsValue::from_str(&e))
    }

    /// How many entries the merged cutoff counts down to: the result count,
    /// or the archive cap under an R20 window.
    pub fn result_count(&self) -> usize { self.inner.result_count() }

    /// Solves unit `index` of `count`. `seed_cutoff` is the floor of the
    /// `result_count()`-th best distinct score across every worker's latest
    /// report (0 for none) and `seed_best` the best; both are admissible
    /// because they are scores of real builds.
    pub fn solve_unit(
        &self, index: usize, count: usize, seed_cutoff: f64, seed_best: f64,
        on_progress: &js_sys::Function,
    ) -> String {
        let mut sink = progress_sink(on_progress);
        self.inner.solve_unit_json(index, count, seed_cutoff, seed_best, 0.0, Some(&mut sink))
    }
}

/// The progress callback shared by the exact entry points: posts the JSON
/// snapshot and returns the host's cutoff floor, when it gives one.
///
/// A throwing callback must not abort the solve.
///
/// Its RETURN value, when it is a finite positive number, is the best
/// score any sibling partition has reached — the host keeps that in a
/// SharedArrayBuffer when the page is cross-origin isolated. Feeding it
/// back gives browser partitions the shared cutoff that native threads
/// get, which is the one thing `solve_partition` loses to them. It is a
/// pruning floor only: a score already achieved elsewhere bounds the
/// global top-N threshold from below, so nothing reachable is skipped.
fn progress_sink(on_progress: &js_sys::Function)
    -> impl FnMut(crate::enumerate::ProgressSnapshot) -> Option<f64> + '_ {
    move |p| {
        let payload = crate::enumerate::progress_json(&p);
        match on_progress.call1(&JsValue::NULL, &JsValue::from_str(&payload)) {
            Ok(v) => v.as_f64().filter(|x| x.is_finite() && *x > 0.0),
            Err(_) => None,
        }
    }
}

/// Search overlapping neighborhoods for strong builds within a time budget.
/// This is a heuristic: every progress/final payload sets `complete:false`.
/// Options and witness shapes are shared with the native testable wrapper.
#[wasm_bindgen]
pub fn solve_anytime_with_progress(
    enum_fixture: &str, score_fixture: &str, options_json: &str,
    on_progress: &js_sys::Function,
) -> String {
    let mut sink = |payload: &str| {
        // UI callback failures must not turn a valid search into a WASM trap.
        let _ = on_progress.call1(&JsValue::NULL, &JsValue::from_str(payload));
    };
    crate::enumerate::anytime::solve_json_with_progress(
        enum_fixture, score_fixture, options_json, Some(&mut sink),
    )
}
