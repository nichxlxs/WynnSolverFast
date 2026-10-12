#!/usr/bin/env python3
"""Report long-query quality at a common budget without inventing solve times."""
import argparse
import json
import math
from pathlib import Path
import statistics

from benchmark_quality import finite, quality_time, score_at, sha256
from quality_suite import HERE


def format_seconds(value):
    return "not reached" if value is None else f"{value:.3f} s"


def expected_counts(plan):
    heuristic_repeats = plan.get("heuristic_repeats", len(plan["heuristic_seeds"]))
    return {"current_exact15": plan["baseline_repeats"],
            "wide_exact15": plan["wide_exact_repeats"],
            "current_alns15": heuristic_repeats, "elite_alns15": heuristic_repeats}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--campaign", type=Path, required=True)
    ap.add_argument("--plan-dir", type=Path, default=HERE / "evidence/quick_2026_09_08/long_setup")
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    campaign = json.loads(args.campaign.read_text())
    plan = json.loads((args.plan_dir / "plan.json").read_text())
    refs = json.loads((args.plan_dir / "references.json").read_text())
    names = [q["name"] for q in plan["queries"]]
    expected = expected_counts(plan)
    common_budget = plan["heuristic_seconds"]
    heuristic_config = json.loads((args.plan_dir / "heuristic_config.json").read_text())
    repair_limits = sorted({int(v["args"][v["args"].index("--max-repairs") + 1])
                            for v in heuristic_config["variants"]})
    report = {"schema_version": 1, "campaign_sha256": sha256(args.campaign),
        "plan_sha256": sha256(args.plan_dir / "plan.json"),
        "reference_sha256": sha256(args.plan_dir / "references.json"), "queries": [],
        "scope": f"Native shared evaluator and hash-identical fixtures. T99 refers to a previously frozen best-known milestone, not 99% of a certified global optimum. Baseline has {plan['baseline_repeats']} repetition(s); latency ratios are descriptive.",
        "common_budget_seconds": common_budget, "observed_runs": len(campaign["runs"]),
        "planned_runs": plan["planned_runs"], "missing_runs": [], "failures": []}
    caps = ", ".join(f"{x:g}" for x in sorted({q['baseline_cap_seconds'] for q in plan['queries']}))
    lines = ["# Long-query optimization benchmark", "", report["scope"], "",
        f"This {len(names)}-query cohort uses baseline caps of {caps} seconds. {plan['selection_policy']} Medium and broad labels were capacity hypotheses; the table reports actual completion or a measured lower bound. No short-window rate projection is presented as an observed minute or hour runtime.", "",
        f"When an exact baseline is included, its longer trace also supplies its {common_budget}-second common-budget endpoint. Each ALNS arm has {expected['current_alns15']} repeats at {plan['heuristic_seconds']} seconds, top {plan['top_k']}, and identical budgets; the new arm changes only the elite-pool operator. The exact wide-key arm has {expected['wide_exact15']} repeat(s) at {plan['wide_exact_seconds']} seconds.", "",
        f"These native ALNS runs use max_repairs={','.join(map(str, repair_limits))}; the browser Quick search profile uses max_repairs=10000 and is validated separately. {plan['heuristic_profile_adjustment']} Native results therefore must not be presented as measured browser timings.", "",
        "| Query | Exact completion | Exact T99 | Current ALNS T99 (all seeds) | Elite ALNS T99 (all seeds) |",
        "|---|---:|---:|---|---|"]
    for name in names:
        ref = refs[name]
        query = {"name": name, "reference": ref, "arms": {}, "comparisons": {}}
        rows = [r for r in campaign["runs"] if r["scenario"] == name]
        completed_exact = [r for r in rows if r["variant"] in ("current_exact15", "wide_exact15")
                           and r["complete"] and r["wall_seconds"] <= r["budget_seconds"]
                           and finite(r.get("best_score"))]
        if completed_exact:
            evaluator_max = max(r["best_score"] for r in completed_exact)
            query["completed_exact_evaluator_max"] = evaluator_max
            for run in rows:
                if finite(run.get("best_score")) and run["best_score"] > evaluator_max + max(1, abs(evaluator_max)) * 1e-8:
                    report["failures"].append({"scenario": name, "arm": run["variant"], "seed": run["seed"],
                        "error": "Observed score exceeds completed exact result on the same fixture; investigate evaluator/pruning parity before claiming a quality extension."})
        for arm, count in expected.items():
            runs = sorted([r for r in rows if r["variant"] == arm], key=lambda r: r["seed"])
            if len(runs) != count:
                report["missing_runs"].append({"scenario": name, "arm": arm, "expected": count, "observed": len(runs)})
            for run in runs:
                if any(run[k] != ref[k] for k in ("enum_sha256", "score_sha256")):
                    raise ValueError(f"{name}: fixture differs from frozen reference")
                if run["status"] in ("failed", "unavailable_fixture") or run.get("trace_errors"):
                    report["failures"].append({"scenario": name, "arm": arm, "seed": run["seed"], "status": run["status"], "trace_errors": run.get("trace_errors")})
            times = [quality_time(r["trajectory"], ref["score"], .99, r["budget_seconds"])
                     if r["status"] not in ("failed", "unavailable_fixture") else None for r in runs]
            times95 = [quality_time(r["trajectory"], ref["score"], .95, r["budget_seconds"])
                       if r["status"] not in ("failed", "unavailable_fixture") else None for r in runs]
            times999 = [quality_time(r["trajectory"], ref["score"], .999, r["budget_seconds"])
                        if r["status"] not in ("failed", "unavailable_fixture") else None for r in runs]
            checkpoints = {str(t): [score_at(r["trajectory"], t) if t <= r["budget_seconds"] else None for r in runs]
                           for t in sorted({5, 15, common_budget})}
            query["arms"][arm] = {"runs": len(runs), "seeds": [r["seed"] for r in runs],
                "status": [r["status"] for r in runs], "budgets_seconds": [r["budget_seconds"] for r in runs],
                "t95_seconds": times95, "t99_seconds": times, "t999_seconds": times999,
                "all_t99_attained": bool(runs) and len(runs) == count and all(t is not None for t in times),
                "median_t99_seconds": statistics.median(times) if times and all(t is not None for t in times) else None,
                "common_budget_scores": checkpoints,
                "completed_wall_seconds": [r["wall_seconds"] if r["complete"] else None for r in runs],
                "work_at_30_seconds": [r.get("work_checkpoints", {}).get("30") for r in runs],
                "work_at_common_budget": [r.get("work_checkpoints", {}).get(str(common_budget)) for r in runs],
                "no_result": sum(score_at(r["trajectory"], min(common_budget, r["budget_seconds"])) is None for r in runs)}
        baseline = query["arms"]["current_exact15"]
        for arm in ("wide_exact15", "current_alns15", "elite_alns15"):
            candidate = query["arms"][arm]
            comparison = {"t99_ratio": None, "t99_ratio_lower_bound": None,
                "all_candidate_seeds_at_least_tenfold": False}
            if candidate["all_t99_attained"] and baseline["runs"] == 1:
                worst_candidate = max(candidate["t99_seconds"])
                baseline_time = baseline["t99_seconds"][0]
                if baseline_time is not None and candidate["median_t99_seconds"] > 0:
                    comparison["t99_ratio"] = baseline_time / candidate["median_t99_seconds"]
                    comparison["all_candidate_seeds_at_least_tenfold"] = worst_candidate > 0 and baseline_time / worst_candidate >= 10
                elif baseline["status"][0] in ("timeout", "capped") and worst_candidate > 0:
                    comparison["t99_ratio_lower_bound"] = baseline["budgets_seconds"][0] / worst_candidate
                    comparison["all_candidate_seeds_at_least_tenfold"] = comparison["t99_ratio_lower_bound"] >= 10
            query["comparisons"][arm] = comparison
        endpoints = [s for a in query["arms"].values() for s in a["common_budget_scores"][str(common_budget)] if finite(s)]
        best_new = max(endpoints, default=None)
        query["new_best_known_at_common_budget"] = best_new
        query["new_best_known_extension_pct"] = max(0, (best_new / ref["score"] - 1) * 100) if finite(best_new) and ref["score"] > 0 else None
        exact_completion = baseline["completed_wall_seconds"][0] if baseline["completed_wall_seconds"] else None
        completion_text = format_seconds(exact_completion) if exact_completion is not None else (
            f">{baseline['budgets_seconds'][0]:g} s" if baseline["runs"] and baseline["status"][0] in ("timeout", "capped") else "unavailable")
        base_time = format_seconds(baseline["t99_seconds"][0]) if baseline["t99_seconds"] else "unavailable"
        times_text = lambda arm: ", ".join(format_seconds(t) for t in query["arms"][arm]["t99_seconds"]) or "unavailable"
        lines.append(f"| {name} | {completion_text} | {base_time} | {times_text('current_alns15')} | {times_text('elite_alns15')} |")
        report["queries"].append(query)
    report["complete_matrix"] = not report["missing_runs"] and not report["failures"]
    report["tenfold_all_seed_queries"] = {arm: [q["name"] for q in report["queries"] if q["comparisons"][arm]["all_candidate_seeds_at_least_tenfold"]]
        for arm in ("wide_exact15", "current_alns15", "elite_alns15")}
    lines.extend(["", "A not-reached T99 is a censored observation, not zero seconds. A numeric speedup requires reaching the identical frozen target; a lower bound uses the baseline's measured no-hit time, never its projected exhaustive completion. Missing/failed runs remain visible.", "",
        "## Secondary 95% milestone", "",
        "The primary T99 target remains unchanged. This secondary threshold distinguishes recovering a substantial quality gap from improving the last percentage point near the reference.", "",
        "| Query | Exact T95 | Current ALNS T95 | Elite ALNS T95 |",
        "|---|---:|---|---|"])
    for q in report["queries"]:
        def t95_cell(arm):
            values = q["arms"][arm]["t95_seconds"]
            return ", ".join(format_seconds(t) for t in values) if values else "unavailable"
        lines.append(f"| {q['name']} | {t95_cell('current_exact15')} | {t95_cell('current_alns15')} | {t95_cell('elite_alns15')} |")
    lines.extend(["",
        f"## Common {common_budget}-second quality", "",
        "| Query | Prior reference | Exact | Wide exact | Current ALNS (range) | Elite ALNS (range) |",
        "|---|---:|---:|---:|---:|---:|"])
    for q in report["queries"]:
        def cell(arm):
            scores = q["arms"][arm]["common_budget_scores"][str(common_budget)]
            if not scores or any(s is None for s in scores): return "no result / unavailable"
            ratios = [100 * s / q["reference"]["score"] for s in scores]
            return f"{min(ratios):.2f}%" if max(ratios) - min(ratios) < .005 else f"{min(ratios):.2f}–{max(ratios):.2f}%"
        lines.append(f"| {q['name']} | {q['reference']['score']:.6g} | {cell('current_exact15')} | {cell('wide_exact15')} | {cell('current_alns15')} | {cell('elite_alns15')} |")
    lines.extend(["", "Percentages above 100% are new best-known extensions beyond the frozen reference. They are reported separately and do not change the primary T99 milestone after seeing the results.", "",
        f"## Observed exact-search work by {common_budget} seconds", "",
        "| Query | Exact credited tuples | Exact leaf calls | Wide credited tuples | Wide leaf calls |",
        "|---|---:|---:|---:|---:|"])
    for q in report["queries"]:
        def count_cell(arm, key):
            snapshots = q["arms"][arm]["work_at_common_budget"]
            values = [s.get(key) for s in snapshots if s is not None and finite(s.get(key))]
            if not values: return "unavailable"
            return ", ".join(f"{v:.6g}" for v in values)
        lines.append(f"| {q['name']} | {count_cell('current_exact15', 'checked')} | {count_cell('current_exact15', 'leaf_calls')} | {count_cell('wide_exact15', 'checked')} | {count_cell('wide_exact15', 'leaf_calls')} |")
    lines.extend(["", f"These are the last parent-observed main-search snapshots at or before {common_budget} seconds, not inferred completion rates. Enabling the wide-key flag does not establish that an objective supports bounds or that its pools needed wider keys; only the observed work/result differences are evidence.", "",
        f"Observed {report['observed_runs']}/{report['planned_runs']} planned runs; missing blocks: {len(report['missing_runs'])}; failed/error runs: {len(report['failures'])}.", "",
        "Exact work snapshots count main DFS only and exclude warm search; heuristic neighborhoods overlap. Credited tuples are not concrete evaluator calls and cannot certify heuristic search coverage. Native and WASM parity, game-model validity, and global skill-point allocation are separate claims.", ""])
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "long_analysis.json").write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    (args.out / "long_analysis.md").write_text("\n".join(lines))
    print(f"Long report: {args.out / 'long_analysis.md'} ({report['observed_runs']} runs)")


if __name__ == "__main__":
    main()
