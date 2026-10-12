//! End-to-end archive/warm/partition check against a tractable fixture.
//! Usage: anytime_core_check <enum fixture> <score fixture>
use sp_kernel::enumerate::{self as en, SearchOptions, TopEntry};
use std::collections::HashSet;

fn scores(entries: &[TopEntry]) -> Vec<u64> {
    entries.iter().map(|e| e.score.to_bits()).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let fixture = std::fs::read_to_string(&args[1]).unwrap();
    let score = std::fs::read_to_string(&args[2]).unwrap();
    let fx = en::parse_fixture(&fixture);
    let ctx = sp_kernel::scoring::ScoringCtx::load(&serde_json::from_str(&score).unwrap()).unwrap();
    let run = |n, retain, budget, part| en::run_single_with_options(
        &fx, Some(&ctx), budget, None, part,
        SearchOptions { result_count: n, retain_warm: retain });
    let legacy = run(15, false, None, None);
    let retained = run(15, true, None, None);
    let top1 = run(1, true, None, None);
    assert!(!retained.stopped_early && !top1.stopped_early);
    assert!(!retained.top_n.is_empty(), "fixture needs feasible scored builds");
    assert_eq!(scores(&legacy.top_n), scores(&retained.top_n), "warm retention top15 parity");
    assert_eq!(top1.top_n.len(), 1);
    assert_eq!(top1.top_n[0].score.to_bits(), retained.top_n[0].score.to_bits());
    let distinct: HashSet<_> = retained.top_n.iter().map(en::build_identity).collect();
    assert_eq!(distinct.len(), retained.top_n.len());

    // Stop before DFS: a valid warm witness must already be visible in the
    // first callback and in the returned archive, including its SP assignment.
    let mut first = None;
    let mut sink = |p: en::ProgressSnapshot| { if first.is_none() { first = Some(p); } None };
    let early = en::run_single_with_options(&fx, Some(&ctx), Some(0.0), Some(&mut sink), None,
        SearchOptions { result_count: 15, retain_warm: true });
    assert!(early.stopped_early);
    assert_eq!(early.leaf_calls, 0);
    assert!(!early.top_n.is_empty(), "fixture needs a feasible warm seed");
    assert_eq!(scores(&first.unwrap().top_n), scores(&early.top_n));

    let pool_len = fixture.lines().find(|line| line.starts_with("SLOT "))
        .and_then(|line| line.split_whitespace().last()).and_then(|n| n.parse().ok()).unwrap_or(0);
    if pool_len > 0 {
        let mut merged = Vec::new();
        let mut partition_keys = HashSet::new();
        let mut checked = 0.0;
        for i in 0..3 {
            let t = run(15, true, None, Some(en::partition_bounds(pool_len, i, 3)));
            checked += t.checked;
            for e in &t.top_n {
                assert!(partition_keys.insert(en::build_identity(e)), "partition warm witnesses overlap");
            }
            en::merge_top_n(&mut merged, t.top_n, 15);
        }
        assert_eq!(checked, retained.checked);
        assert_eq!(scores(&merged), scores(&retained.top_n), "partition top15 parity");
    }
    println!("anytime_core_check: PASS (top15 retention, top1 optimum, dedup, zero-budget warm publication, three-way partition parity)");
}
