//! Monotonic clock that also compiles for wasm32.
//!
//! `std::time::Instant::now()` panics on wasm32-unknown-unknown, so the
//! enumeration engine routes every timing call through here. On wasm the
//! browser build uses `performance.now()`, which is monotonic in both
//! Window and Worker globals. Deterministic leaf budgets remain available;
//! anytime searches additionally need real elapsed time for their deadline.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug)]
pub struct Instant(f64);

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

#[cfg(target_arch = "wasm32")]
fn monotonic_millis() -> f64 {
    #[cfg(feature = "wasm")]
    { performance_now() }
    // A featureless wasm target check has no JS imports or exported solver.
    // Actual browser builds must enable `wasm` (as build-wasm.sh does).
    #[cfg(not(feature = "wasm"))]
    { 0.0 }
}

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub fn now() -> Instant { Instant(monotonic_millis()) }
    pub fn elapsed(&self) -> Duration {
        Duration((monotonic_millis() - self.0).max(0.0) / 1_000.0)
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug)]
pub struct Duration(f64);

#[cfg(target_arch = "wasm32")]
impl Duration {
    pub fn as_secs_f64(&self) -> f64 { self.0 }
    pub fn as_nanos(&self) -> u128 { (self.0 * 1_000_000_000.0) as u128 }
}

/// Marker trait alias kept so callers can name the type uniformly.
pub type Clock = Instant;
