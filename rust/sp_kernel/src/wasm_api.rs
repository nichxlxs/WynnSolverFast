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
