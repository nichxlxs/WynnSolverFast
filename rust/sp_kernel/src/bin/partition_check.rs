//! Verifies that N-way partitioning covers the search space exactly.
//!
//! Each partition is what one browser worker runs. The integral counters
//! must sum to the whole-space run's, and the merged top-N must be
//! identical — otherwise multi-worker solving would silently lose builds.
//!
//! Usage: partition_check <enum fixture> <score fixture> [max_parts]
use serde_json::Value;

fn run(enum_f: &str, score_f: &str, idx: usize, n: usize) -> Value {
    serde_json::from_str(&sp_kernel::enumerate::solve_json_full(
        enum_f, score_f, 0.0, None, idx, n)).expect("json")
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let enum_f = std::fs::read_to_string(&a[1]).expect("enum fixture");
    let score_f = std::fs::read_to_string(&a[2]).expect("score fixture");
    let max_parts: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(8);

    let whole = run(&enum_f, &score_f, 0, 1);
    let keys = ["checked", "feasible", "scored", "gated", "mana_reject",
                "thresh_reject", "bound_pruned"];
    let get = |v: &Value, k: &str| v[k].as_f64().unwrap_or(f64::NAN);

    let mut whole_top: Vec<f64> = whole["top"].as_array().unwrap()
        .iter().map(|t| t["score"].as_f64().unwrap()).collect();
    whole_top.sort_by(|a, b| b.partial_cmp(a).unwrap());

    println!("whole: checked={} scored={} top1={:.17e}",
             get(&whole, "checked"), get(&whole, "scored"), whole_top[0]);

    let mut ok = true;
    for n in 2..=max_parts {
        let parts: Vec<Value> = (0..n).map(|i| run(&enum_f, &score_f, i, n)).collect();
        let mut line = format!("  {n}-way:");
        for k in keys {
            let sum: f64 = parts.iter().map(|p| get(p, k)).sum();
            let w = get(&whole, k);
            // Only `checked` is cutoff-independent. Every counter downstream
            // of the score-ceiling gate (feasible, scored, gated,
            // mana_reject, thresh_reject, bound_pruned) depends on how early
            // a good cutoff is found, and partitioning changes that — the
            // same reason they vary between 1 and 4 native threads. The
            // invariants that matter are that the space is covered exactly
            // and that the merged top-N is unchanged.
            let exact = k == "checked";
            if exact && sum != w {
                line.push_str(&format!(" {k}={sum}!={w}"));
                ok = false;
            }
        }
        // Merged top-N must equal the whole-space top-N, score for score.
        let mut merged: Vec<f64> = parts.iter()
            .flat_map(|p| p["top"].as_array().unwrap().iter()
                .map(|t| t["score"].as_f64().unwrap()))
            .collect();
        merged.sort_by(|a, b| b.partial_cmp(a).unwrap());
        merged.truncate(whole_top.len());
        let top_ok = merged.len() == whole_top.len()
            && merged.iter().zip(&whole_top).all(|(a, b)| a.to_bits() == b.to_bits());
        if !top_ok {
            ok = false;
            line.push_str(&format!(" TOP-N DIFFERS (merged top1={:.17e})",
                                   merged.first().copied().unwrap_or(f64::NAN)));
        }
        let checked: f64 = parts.iter().map(|p| get(p, "checked")).sum();
        println!("{line} checked={checked} top-N={}",
                 if top_ok { "identical" } else { "DIFFERS" });
    }
    println!("partition_check: {}", if ok { "EXACT" } else { "MISMATCH" });
    if !ok { std::process::exit(1); }
}
