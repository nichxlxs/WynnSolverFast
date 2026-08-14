//! Browser entry point (cargo feature `wasm`).
//!
//! Exposes the same engine the native CLI runs — identical enumeration,
//! bounds and scoring — to JavaScript. Inputs are the two fixture payloads
//! as strings (the browser fetches or generates them; there is no
//! filesystem in wasm), and the result is JSON.
//!
//! Work is bounded by a deterministic LEAF BUDGET rather than wall time:
//! wasm32 has no usable monotonic clock, and a leaf budget also gives the
//! UI reproducible chunks it can loop over while keeping the page
//! responsive. Threads are out of scope here (they need SharedArrayBuffer
//! plus cross-origin isolation) — this is the single-threaded path, which
//! is already ~1000x the current JS engine per core.

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
/// Emission is keyed on leaves rather than wall time because wasm32 has no
/// usable clock; that also makes the emission points reproducible.
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
    let this = JsValue::NULL;
    let mut sink = |p: crate::enumerate::ProgressSnapshot| -> Option<f64> {
        let payload = crate::enumerate::progress_json(&p);
        // A throwing callback must not abort the solve.
        //
        // Its RETURN value, when it is a finite positive number, is the best
        // score any sibling partition has reached — the host keeps that in a
        // SharedArrayBuffer when the page is cross-origin isolated. Feeding it
        // back gives browser partitions the shared cutoff that native threads
        // get, which is the one thing `solve_partition` loses to them. It is a
        // pruning floor only: a score already achieved elsewhere bounds the
        // global top-N threshold from below, so nothing reachable is skipped.
        match on_progress.call1(&this, &JsValue::from_str(&payload)) {
            Ok(v) => v.as_f64().filter(|x| x.is_finite() && *x > 0.0),
            Err(_) => None,
        }
    };
    crate::enumerate::solve_json_full(
        enum_fixture, score_fixture, max_leaves, Some(&mut sink), part_index, part_count,
    )
}
