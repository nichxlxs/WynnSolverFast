//! Verifies that N-way partitioning covers the search space exactly.
//!
//! Each partition is what one browser worker runs. The integral counters
//! must sum to the whole-space run's, and the merged top-N must be
//! identical — otherwise multi-worker solving would silently lose builds.
//!
//! It also checks the R24 work queue: `EngineSession`s (one per simulated
//! worker) solving first-slot units in order, each unit seeded with the
//! merged cutoff of every unit before it. Same invariants: `checked` sums
//! to the whole run's and the merged top-N is identical, bit for bit.
//!
//! Usage: partition_check <enum fixture> <score fixture> [max_parts]
use serde_json::Value;

fn run(enum_f: &str, score_f: &str, idx: usize, n: usize) -> Value {
    serde_json::from_str(&sp_kernel::enumerate::solve_json_full(
        enum_f, score_f, 0.0, None, idx, n)).expect("json")
}

fn scores(v: &Value) -> Vec<f64> {
    v["top"].as_array().map_or(Vec::new(), |a| a.iter().filter_map(|t| t["score"].as_f64()).collect())
}

/// Whether the merged parts reproduce the whole run's results. Without a
/// window: the top-N, bit for bit. Under an R20 window the contract is the
/// host's (`mergeShortlistArchives`): the deduplicated builds at or above the
/// merged window line, plus a completeness flag; when both sides claim
/// completeness the in-window scores must match bit for bit, and the merged
/// side must claim it whenever the whole run does.
fn same_results(parts: &[Value], whole: &Value) -> bool {
    let mut merged: Vec<f64> = parts.iter().flat_map(scores).collect();
    merged.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let mut whole_top = scores(whole);
    whole_top.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let Some(w) = whole["window"].as_f64() else {
        merged.truncate(whole_top.len());
        return merged.len() == whole_top.len()
            && merged.iter().zip(&whole_top).all(|(a, b)| a.to_bits() == b.to_bits());
    };
    let line = merged.first().copied().unwrap_or(f64::NAN) * (1.0 - w);
    let merged_complete = parts.iter().all(|p| p["complete"].as_bool() == Some(true)
        && (p["archive_full"].as_bool() != Some(true)
            || p["archive_last"].as_f64().is_some_and(|l| l < line)));
    let whole_complete = whole["window_complete"].as_bool() == Some(true);
    if whole_complete && !merged_complete { return false; }
    if !(whole_complete && merged_complete) { return true; }
    let inside = |v: &[f64]| v.iter().copied().filter(|s| *s >= line).collect::<Vec<_>>();
    let (a, b) = (inside(&merged), inside(&whole_top));
    a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits())
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
        let top_ok = same_results(&parts, &whole);
        if !top_ok {
            ok = false;
            line.push_str(" TOP-N DIFFERS");
        }
        let checked: f64 = parts.iter().map(|p| get(p, "checked")).sum();
        println!("{line} checked={checked} top-N={}",
                 if top_ok { "identical" } else { "DIFFERS" });
    }
    let rc = sp_kernel::enumerate::EngineSession::new(&enum_f, &score_f).expect("session").result_count();
    let pool = sp_kernel::enumerate::EngineSession::new(&enum_f, &score_f).expect("session").first_pool_len();
    for (workers, units) in [(1, 3), (2, 7), (3, 16), (4, pool + 2)] {
        let sessions: Vec<_> = (0..workers)
            .map(|_| sp_kernel::enumerate::EngineSession::new(&enum_f, &score_f).expect("session"))
            .collect();
        let mut last: Vec<Value> = vec![Value::Null; workers];
        for u in 0..units {
            let mut seen: Vec<f64> = last.iter().flat_map(scores).collect();
            seen.sort_by(|a, b| b.partial_cmp(a).unwrap());
            let cut = if seen.len() >= rc { seen[rc - 1].floor() } else { 0.0 };
            let best = seen.first().copied().unwrap_or(0.0);
            let w = u % workers;
            last[w] = serde_json::from_str(&sessions[w].solve_unit_json(u, units, cut, best, 0.0, None))
                .expect("json");
        }
        let checked: f64 = last.iter().map(|p| get(p, "checked")).sum();
        let top_ok = same_results(&last, &whole);
        let checked_ok = checked == get(&whole, "checked");
        let complete_ok = last.iter().all(|p| p["complete"].as_bool() == Some(true));
        ok &= top_ok && checked_ok && complete_ok;
        println!("  queue {workers}w x {units} units: checked={checked}{} top-N={}{}",
                 if checked_ok { "" } else { " MISMATCH" },
                 if top_ok { "identical" } else { "DIFFERS" },
                 if complete_ok { "" } else { " INCOMPLETE" });
    }
    println!("partition_check: {}", if ok { "EXACT" } else { "MISMATCH" });
    if !ok { std::process::exit(1); }
}
