#!/usr/bin/env python3
"""Read-only class/removal cohort analysis of a recorded quality campaign.

Reports paired endpoint quality, target attainment, and censored denominators.
Only query blocks with every configured arm/repeat are included. A partial
campaign is explicitly provisional. Numeric speedups are comparisons to the
same reference score, not comparisons to exhaustive completion time.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
from pathlib import Path
import statistics
import time

from benchmark_quality import finite, quality_time, reference_for, sha256
from quality_suite import DEFAULT_MANIFEST, load_manifest, select_scenarios


def median(values):
    return statistics.median(values) if values else None


def classify(query):
    if query["group"] == "meta":
        variant = query["variant"]
        return {"removal_cohort": "meta_seed_control" if variant == "known_good" else "meta_" + variant,
                "class_cohort": "meta_class_" + query["class"],
                "family_cohort": "meta_family_" + query["family"]}
    if query["group"] == "families":
        return {"removal_cohort": "family_" + query["variant"], "class_cohort": None,
                "family_cohort": "original_family_" + query["family"]}
    return {"removal_cohort": "legacy", "class_cohort": None, "family_cohort": "legacy"}


def valid_score(run):
    if run.get("status") in ("failed", "unavailable_fixture"):
        return None
    return run["best_score"] if finite(run.get("best_score")) else None


def compare_query(query, baseline_name, candidate_name, arms, reference, variant_metadata=None):
    baseline, candidate = arms[baseline_name], arms[candidate_name]
    a_scores = [valid_score(r) for r in baseline]
    b_scores = [valid_score(r) for r in candidate]
    a_times = [quality_time(r["trajectory"], reference["score"], .99, r["budget_seconds"])
               if valid_score(r) is not None else None for r in baseline]
    b_times = [quality_time(r["trajectory"], reference["score"], .99, r["budget_seconds"])
               if valid_score(r) is not None else None for r in candidate]
    all_results = all(s is not None for s in a_scores + b_scores)
    a_median = median(a_scores) if all(s is not None for s in a_scores) else None
    b_median = median(b_scores) if all(s is not None for s in b_scores) else None
    endpoint_ratio = b_median / a_median if all_results and a_median > 0 else None
    all_attained = all(t is not None for t in a_times + b_times)
    speedup = median(a_times) / median(b_times) if all_attained and median(b_times) > 0 else None
    # This separate lower bound is never confused with an observed ratio. It
    # holds only if EVERY baseline repeat misses and EVERY candidate repeat
    # reaches; using fastest baseline deadline / slowest candidate is conservative.
    lower_bound = None
    if all(t is None for t in a_times) and all(t is not None and t > 0 for t in b_times):
        if all(r.get("status") in ("capped", "timeout") for r in baseline):
            lower_bound = min(r["budget_seconds"] for r in baseline) / max(b_times)
    metadata = variant_metadata or {}
    base_variant, candidate_variant = metadata.get(baseline_name, {}), metadata.get(candidate_name, {})
    base_count = str(base_variant.get("env", {}).get("RESULT_COUNT", "15"))
    candidate_count = str(candidate_variant.get("env", {}).get("RESULT_COUNT", "15"))
    same_completion_goal = base_variant.get("kind") == candidate_variant.get("kind") == "enum" and base_count == candidate_count
    a_complete = [r.get("wall_seconds") if r.get("complete") and r.get("status") == "completed" else None for r in baseline]
    b_complete = [r.get("wall_seconds") if r.get("complete") and r.get("status") == "completed" else None for r in candidate]
    completion_speedup = None
    if same_completion_goal and all(finite(t) and t > 0 for t in a_complete + b_complete):
        completion_speedup = median(a_complete) / median(b_complete)
    warm_gains = []
    for run in candidate:
        initial = [e["score"] for e in run["trajectory"] if e.get("phase") in ("ordered_seed", "warm")
                   and e["observed_seconds"] <= run["budget_seconds"]]
        first_best = max(initial) if initial else None
        last_best = valid_score(run)
        warm_gains.append(last_best / first_best if last_best is not None and first_best and first_best > 0 else None)
    return {"scenario": query["name"], "group": query["group"], **classify(query),
            "baseline": baseline_name, "candidate": candidate_name,
            "reference_score": reference["score"], "reference_kind": reference["kind"],
            "reference_evidence": reference.get("evidence_kind", reference.get("source")),
            "baseline_runs": len(baseline), "candidate_runs": len(candidate),
            "baseline_scores": a_scores, "candidate_scores": b_scores,
            "baseline_result_count": sum(s is not None for s in a_scores),
            "candidate_result_count": sum(s is not None for s in b_scores),
            "baseline_complete_count": sum(r.get("complete", False) for r in baseline),
            "candidate_complete_count": sum(r.get("complete", False) for r in candidate),
            "same_completion_result_count_goal": same_completion_goal,
            "baseline_completion_wall_seconds": a_complete,
            "candidate_completion_wall_seconds": b_complete,
            "completion_speedup_all_completed_same_goal": completion_speedup,
            "baseline_endpoint_median": a_median, "candidate_endpoint_median": b_median,
            "endpoint_ratio": endpoint_ratio,
            "baseline_t99_times": a_times, "candidate_t99_times": b_times,
            "baseline_t99_attained": sum(t is not None for t in a_times),
            "candidate_t99_attained": sum(t is not None for t in b_times),
            "t99_speedup_all_attained": speedup,
            "t99_censored_speedup_lower_bound": lower_bound,
            "candidate_gain_over_own_warm": warm_gains,
            "timing_near_observer_resolution": all_attained and min(a_times + b_times) < .05}


def aggregate(rows, baseline, candidate, cohort):
    ratios = [r["endpoint_ratio"] for r in rows if r["endpoint_ratio"] is not None]
    speeds = [r["t99_speedup_all_attained"] for r in rows if r["t99_speedup_all_attained"] is not None]
    completion_speeds = [r["completion_speedup_all_completed_same_goal"] for r in rows
                         if r["completion_speedup_all_completed_same_goal"] is not None]
    total_a_runs = sum(r["baseline_runs"] for r in rows)
    total_b_runs = sum(r["candidate_runs"] for r in rows)
    return {"baseline": baseline, "candidate": candidate, "cohort": cohort, "queries": len(rows),
            "baseline_runs": total_a_runs, "candidate_runs": total_b_runs,
            "baseline_no_result_runs": total_a_runs - sum(r["baseline_result_count"] for r in rows),
            "candidate_no_result_runs": total_b_runs - sum(r["candidate_result_count"] for r in rows),
            "baseline_completed_runs": sum(r["baseline_complete_count"] for r in rows),
            "candidate_completed_runs": sum(r["candidate_complete_count"] for r in rows),
            "completion_matched_same_goal_queries": len(completion_speeds),
            "completion_median_speedup_matched_same_goal": median(completion_speeds),
            "baseline_t99_attained_runs": sum(r["baseline_t99_attained"] for r in rows),
            "candidate_t99_attained_runs": sum(r["candidate_t99_attained"] for r in rows),
            "baseline_t99_censored_runs": total_a_runs - sum(r["baseline_t99_attained"] for r in rows),
            "candidate_t99_censored_runs": total_b_runs - sum(r["candidate_t99_attained"] for r in rows),
            "baseline_t99_all_repeats_queries": sum(r["baseline_t99_attained"] == r["baseline_runs"] for r in rows),
            "candidate_t99_all_repeats_queries": sum(r["candidate_t99_attained"] == r["candidate_runs"] for r in rows),
            "paired_endpoint_queries": len(ratios),
            "endpoint_wins": sum(x > 1 + 1e-9 for x in ratios),
            "endpoint_ties": sum(abs(x - 1) <= 1e-9 for x in ratios),
            "endpoint_losses": sum(x < 1 - 1e-9 for x in ratios),
            "endpoint_geomean_ratio_paired_results": math.exp(sum(math.log(x) for x in ratios) / len(ratios))
                if ratios and min(ratios) > 0 else None,
            "t99_common_attainment_queries": len(speeds),
            "t99_median_speedup_common_attainment": median(speeds),
            "t99_observed_at_least_10x_queries": sum(x >= 10 for x in speeds),
            "t99_censored_lower_bound_at_least_10x_queries": sum(
                r["t99_censored_speedup_lower_bound"] is not None and r["t99_censored_speedup_lower_bound"] >= 10 for r in rows),
            "timing_near_observer_resolution_queries": sum(r["timing_near_observer_resolution"] for r in rows)}


def analyze(campaign, manifest, references=None, candidates=None, baselines=None, expected=None):
    variants = [v["name"] for v in campaign["variants"]]
    variant_metadata = {v["name"]: v for v in campaign["variants"]}
    candidates = candidates or [v for v in variants if v.startswith("alns")]
    baselines = baselines or [v for v in ("safe_baseline15", "best1_warm3") if v in variants]
    if any(v not in variants for v in candidates + baselines):
        raise ValueError("requested comparison arm is absent from the campaign")
    query_map = {q["name"]: q for q in manifest["scenarios"]}
    by_query = {}
    for run in campaign["runs"]:
        by_query.setdefault(run["scenario"], {}).setdefault(run["variant"], []).append(run)
    repeated = campaign["repeat"]
    comparisons, partial = [], []
    names = expected or list(by_query)
    complete_queries = []
    for name in names:
        if name not in query_map:
            raise ValueError(f"query absent from manifest: {name}")
        arms = by_query.get(name, {})
        if not all(len(arms.get(v, [])) == repeated for v in variants):
            partial.append(name)
            continue
        complete_queries.append(name)
        all_runs = [r for runs in arms.values() for r in runs]
        hashes = {(r["enum_sha256"], r["score_sha256"]) for r in all_runs}
        if len(hashes) != 1:
            raise ValueError(f"mixed fixture identities: {name}")
        ref = reference_for(name, all_runs, references or {})
        for baseline in baselines:
            for candidate in candidates:
                if candidate != baseline:
                    comparisons.append(compare_query(query_map[name], baseline, candidate, arms, ref, variant_metadata))
    aggregates = []
    for baseline in baselines:
        for candidate in candidates:
            pair = [r for r in comparisons if r["baseline"] == baseline and r["candidate"] == candidate]
            if not pair:
                continue
            groups = {"all_including_seed_controls": pair,
                      "all_searches_excluding_seed_controls": [r for r in pair if r["removal_cohort"] != "meta_seed_control"]}
            for field in ("removal_cohort", "class_cohort", "family_cohort"):
                for label in dict.fromkeys(r[field] for r in pair if r[field]):
                    groups[label] = [r for r in pair if r[field] == label]
            for label in dict.fromkeys(r["class_cohort"] for r in pair if r["class_cohort"]):
                groups[label + "_searches"] = [r for r in pair if r["class_cohort"] == label
                                               and r["removal_cohort"] != "meta_seed_control"]
            aggregates.extend(aggregate(rows, baseline, candidate, label) for label, rows in groups.items() if rows)
    return {"schema_version": 1, "run_count": len(campaign["runs"]), "configured_repeats": repeated,
            "expected_query_count": len(expected) if expected else None,
            "complete_query_count": len(complete_queries), "complete_queries": complete_queries,
            "pending_or_partial_queries": partial,
            "complete_for_expected_query_set": not partial if expected else None,
            "interpretation": "Provisional diagnostics; do not headline a partial campaign. Endpoint gains are score quality, not runtime speedups. Censored lower bounds describe target attainment within observed deadlines, not optimality. Meta-class groups intentionally exclude original families without class metadata.",
            "aggregates": aggregates, "comparisons": comparisons}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--campaign", type=Path, required=True)
    ap.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    ap.add_argument("--references", type=Path)
    ap.add_argument("--candidates", nargs="+")
    ap.add_argument("--baselines", nargs="+")
    ap.add_argument("--expected-scenarios", nargs="+")
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    manifest = load_manifest(args.manifest)
    expected = [q["name"] for q in select_scenarios(manifest, args.expected_scenarios)] if args.expected_scenarios else None
    # The running harness rewrites a checkpoint JSON. A brief read retry avoids
    # treating an in-progress write as corrupt evidence; it never alters it.
    for attempt in range(3):
        try:
            payload = args.campaign.read_bytes()
            campaign = json.loads(payload)
            break
        except json.JSONDecodeError:
            if attempt == 2:
                raise
            time.sleep(.01)
    references = json.loads(args.references.read_text()) if args.references else {}
    result = analyze(campaign, manifest, references, args.candidates, args.baselines, expected)
    result["campaign_path"] = str(args.campaign.resolve())
    result["campaign_sha256"] = hashlib.sha256(payload).hexdigest()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    print(f"{result['complete_query_count']} complete query blocks; {len(result['pending_or_partial_queries'])} pending/partial; {len(result['aggregates'])} cohort aggregates")


if __name__ == "__main__":
    main()
