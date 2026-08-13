//! Runs a single partition, for timing one browser worker's share.
//! Usage: partition_one <enum fixture> <score fixture> <index> <count>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let e = std::fs::read_to_string(&a[1]).expect("enum fixture");
    let sf = std::fs::read_to_string(&a[2]).expect("score fixture");
    let idx: usize = a[3].parse().unwrap();
    let n: usize = a[4].parse().unwrap();
    println!("{}", sp_kernel::enumerate::solve_json_full(&e, &sf, 0.0, None, idx, n));
}
