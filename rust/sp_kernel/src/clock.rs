//! Monotonic clock that also compiles for wasm32.
//!
//! `std::time::Instant::now()` panics on wasm32-unknown-unknown, so the
//! enumeration engine routes every timing call through here. On wasm the
//! clock is inert (always 0 elapsed): browser runs are bounded by a
//! deterministic leaf budget instead of wall time, which is both
//! reproducible and what an incremental UI wants anyway.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug)]
pub struct Instant;

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub fn now() -> Instant { Instant }
    pub fn elapsed(&self) -> Duration { Duration }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug)]
pub struct Duration;

#[cfg(target_arch = "wasm32")]
impl Duration {
    pub fn as_secs_f64(&self) -> f64 { 0.0 }
    pub fn as_nanos(&self) -> u128 { 0 }
}

/// Marker trait alias kept so callers can name the type uniformly.
pub type Clock = Instant;
