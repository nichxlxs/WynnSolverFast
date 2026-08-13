//! Native driver for the WASM entry point: runs `solve_json` — byte for
//! byte the code the browser executes — on a fixture pair and prints the
//! JSON, so the browser path is validated by the normal test loop.
//!
//! Usage: wasm_selftest <enum fixture> <score fixture> [max_leaves]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let enum_f = std::fs::read_to_string(&a[1]).expect("enum fixture");
    let score_f = a.get(2).map(|p| std::fs::read_to_string(p).expect("score fixture"))
        .unwrap_or_default();
    let budget: f64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    println!("{}", sp_kernel::enumerate::solve_json(&enum_f, &score_f, budget));
}
