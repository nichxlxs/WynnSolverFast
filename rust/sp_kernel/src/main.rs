use sp_kernel::{Case, Kernel, Unit};
use std::env;
use std::fs;
use std::time::Instant;

fn parse_cases(text: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    for line in text.lines() {
        let nums: Vec<i32> = line
            .split_ascii_whitespace()
            .map(|t| t.parse().expect("bad int"))
            .collect();
        if nums.is_empty() {
            continue;
        }
        let mut i = 0usize;
        let mut next = || {
            let v = nums[i];
            i += 1;
            v
        };
        let budget = next();
        let mut read_unit = || {
            let crafted = next() != 0;
            let mut reqs = [0i32; 5];
            let mut skp = [0i32; 5];
            for r in reqs.iter_mut() {
                *r = next();
            }
            for s in skp.iter_mut() {
                *s = next();
            }
            Unit { crafted, reqs, skp }
        };
        let mut equipment = [Unit::default(); 8];
        for e in equipment.iter_mut() {
            *e = read_unit();
        }
        let weapon = read_unit();
        let mut set_free = [0i32; 5];
        for b in set_free.iter_mut() {
            *b = next();
        }
        let feas = next() != 0;
        let mut assign = [0i32; 5];
        let mut final_sp = [0i32; 5];
        for a in assign.iter_mut() {
            *a = next();
        }
        for f in final_sp.iter_mut() {
            *f = next();
        }
        let total = next();
        let expected = if feas {
            Some((assign, final_sp, total))
        } else {
            None
        };
        cases.push(Case {
            budget,
            equipment,
            weapon,
            set_free,
            expected,
        });
    }
    cases
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("fixtures/sp_cases.txt");
    let bench_iters: usize = args
        .get(2)
        .map(|s| s.parse().expect("bad iters"))
        .unwrap_or(2000);

    let text = fs::read_to_string(fixture_path).expect("cannot read fixtures");
    let cases = parse_cases(&text);
    let mut kernel = Kernel::new();

    // Parity check.
    let mut mismatches = 0usize;
    let mut feasible = 0usize;
    for (idx, case) in cases.iter().enumerate() {
        let got = kernel.calculate(case);
        if got != case.expected {
            mismatches += 1;
            if mismatches <= 5 {
                eprintln!(
                    "MISMATCH case {}: got {:?}, expected {:?}",
                    idx, got, case.expected
                );
            }
        }
        if got.is_some() {
            feasible += 1;
        }
    }
    println!(
        "parity: {} cases, {} feasible, {} mismatches",
        cases.len(),
        feasible,
        mismatches
    );
    if mismatches > 0 {
        std::process::exit(1);
    }

    // Bench: kernel-only throughput.
    let start = Instant::now();
    let mut sink = 0i64;
    for _ in 0..bench_iters {
        for case in &cases {
            if let Some((_, _, total)) = kernel.calculate(case) {
                sink += total as i64;
            }
        }
    }
    let elapsed = start.elapsed();
    let calls = bench_iters * cases.len();
    println!(
        "bench: {} calls in {:.1} ms => {:.0} calls/s ({:.3} us/call, checksum {})",
        calls,
        elapsed.as_secs_f64() * 1e3,
        calls as f64 / elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1e6 / calls as f64,
        sink
    );
}
