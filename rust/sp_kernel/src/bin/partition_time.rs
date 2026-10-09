//! Times N-way partitioning the way the browser runs it (R24 measurement).
//!
//! Default mode: each partition is one browser worker with its own
//! process-local cutoff, no sharing. Prints the whole run's time, each
//! partition's, the sum (work inflation from the unshared cutoff) and the
//! slowest (the wall time N workers would take), single-threaded throughout.
//!
//! `queue` mode simulates the R24 work queue: the first slot's pool is cut
//! into `units` contiguous offset ranges, handed out in order to `workers`
//! virtual workers on one session (parse, bounds and warm start once). Each
//! unit starts seeded with the merged cutoff of every unit that finished
//! before it started in simulated time; units run one after another for
//! real, so the simulated wall time is the event-driven makespan.
//!
//! Usage: partition_time <enum fixture> <score fixture> [parts]
//!        partition_time <enum fixture> <score fixture> queue <workers> <units>
use serde_json::Value;
use sp_kernel::enumerate::EngineSession;
use std::time::Instant;

fn run(enum_f: &str, score_f: &str, idx: usize, n: usize) -> (f64, Value) {
    let t = Instant::now();
    let v: Value = serde_json::from_str(&sp_kernel::enumerate::solve_json_full(
        enum_f, score_f, 0.0, None, idx, n)).expect("json");
    (t.elapsed().as_secs_f64(), v)
}

fn scored(v: &Value) -> f64 { v["scored"].as_f64().unwrap_or(f64::NAN) }

fn top_scores(v: &Value) -> Vec<f64> {
    v["top"].as_array().map_or(Vec::new(), |a| a.iter().filter_map(|e| e["score"].as_f64()).collect())
}

/// (cutoff, best) of the merged top lists, as insert_top publishes them.
fn merged(scores: &[f64], rc: usize) -> (f64, f64) {
    let mut s = scores.to_vec();
    s.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let best = s.first().copied().unwrap_or(0.0);
    let cut = if s.len() >= rc { s[rc - 1].floor().max(0.0) } else { 0.0 };
    (cut, best)
}

fn queue(enum_f: &str, score_f: &str, workers: usize, units: usize) {
    let (whole_t, whole) = run(enum_f, score_f, 0, 1);
    // One session per virtual worker, as in the browser; each accumulates
    // its own units, so a worker's latest JSON covers all its finished units.
    let mut sessions = Vec::new();
    let mut free = Vec::new();
    for _ in 0..workers {
        let t = Instant::now();
        sessions.push(EngineSession::new(enum_f, score_f).expect("session"));
        free.push(t.elapsed().as_secs_f64());
    }
    let rc = sessions[0].result_count();
    let pool = sessions[0].first_pool_len();
    let bounds: Vec<(i64, i64)> = (0..units)
        .map(|i| sp_kernel::enumerate::partition_bounds(pool, i, units))
        .filter(|(lo, hi)| lo <= hi)
        .collect();
    // (finish time, worker, that worker's accumulated top scores).
    let mut done: Vec<(f64, usize, Vec<f64>)> = Vec::new();
    let mut last: Vec<Value> = vec![Value::Null; workers];
    let mut work = 0.0;
    for &(lo, hi) in &bounds {
        let w = (0..workers).min_by(|&a, &b| free[a].partial_cmp(&free[b]).unwrap()).unwrap();
        let start = free[w];
        // What the host knows at `start`: each worker's latest finished report.
        let mut seen = Vec::new();
        for o in 0..workers {
            if let Some((_, _, s)) = done.iter().rev().find(|(f, ow, _)| *ow == o && *f <= start) {
                seen.extend_from_slice(s);
            }
        }
        let (cut, best) = merged(&seen, rc);
        let t = Instant::now();
        let v: Value = serde_json::from_str(&sessions[w].solve_range_json(lo, hi, cut, best, 0.0, None)).expect("json");
        let d = t.elapsed().as_secs_f64();
        work += d;
        free[w] = start + d;
        done.push((start + d, w, top_scores(&v)));
        last[w] = v;
    }
    let makespan = free.iter().cloned().fold(0.0f64, f64::max);
    let all: Vec<f64> = last.iter().flat_map(top_scores).collect();
    let scored_sum: f64 = last.iter().filter(|v| !v.is_null()).map(scored).sum();
    let (cut, best) = merged(&all, rc);
    let (wcut, wbest) = merged(&top_scores(&whole), rc);
    let mut mine = all.clone();
    mine.sort_by(|a, b| b.partial_cmp(a).unwrap());
    mine.truncate(rc);
    println!(
        "whole {whole_t:.2}s scored {} | queue {workers}w x {} units: work {work:.2}s \
         ({:.2}x the whole) makespan {makespan:.2}s => speedup {:.2}x | scored sum {scored_sum} | \
         cutoff {cut} vs whole {wcut} best match {} top-{rc} scores match {}",
        scored(&whole), bounds.len(), work / whole_t, whole_t / makespan,
        cut == wcut && best == wbest, mine == top_scores(&whole));
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let enum_f = std::fs::read_to_string(&a[1]).expect("enum fixture");
    let score_f = std::fs::read_to_string(&a[2]).expect("score fixture");
    if a.get(3).map(String::as_str) == Some("queue") {
        let workers = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(4);
        let units = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(16);
        return queue(&enum_f, &score_f, workers, units);
    }
    let n: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(4);

    let (whole_t, whole) = run(&enum_f, &score_f, 0, 1);
    let mut times = Vec::new();
    let mut scored_sum = 0.0;
    for i in 0..n {
        let (t, v) = run(&enum_f, &score_f, i, n);
        times.push(t);
        scored_sum += scored(&v);
    }
    let sum: f64 = times.iter().sum();
    let max = times.iter().cloned().fold(0.0f64, f64::max);
    let parts: Vec<String> = times.iter().map(|t| format!("{t:.2}")).collect();
    println!(
        "whole {whole_t:.2}s scored {} | {n}-way parts [{}]s sum {sum:.2}s ({:.2}x the whole) slowest {max:.2}s \
         => speedup with {n} workers {:.2}x | scored sum {scored_sum}",
        scored(&whole), parts.join(", "), sum / whole_t, whole_t / max);
}
