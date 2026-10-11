#!/usr/bin/env python3
"""Execute the frozen long-query campaign sequentially using benchmark_quality.

Usage: python3 benchmark_long_campaign.py --fixtures DIR --out DIR
       [--stage baseline|wide|heuristic|all] [--resume]

The 180/60-second baseline runs also supply their 30-second common-budget
endpoints. The single baseline repetition is descriptive, not a statistical
estimate of runtime variability. Work counters describe exact main traversal
and exclude the warm search; they never imply heuristic coverage/completeness.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys

from benchmark_quality import sha256, summarize, write_report
from quality_suite import HERE


def combine(destination, plan_dir, fixtures):
    plan = json.loads((plan_dir / "plan.json").read_text())
    references = json.loads((plan_dir / "references.json").read_text())
    campaigns = [p for p in destination.glob("*/campaign.json") if p.parent.name != "combined"]
    variants = {}
    rows = []
    sources = []
    for file in sorted(campaigns):
        campaign = json.loads(file.read_text())
        sources.append({"path": str(file.relative_to(destination)), "sha256": sha256(file)})
        for v in campaign["variants"]:
            if v["name"] in variants and variants[v["name"]] != v:
                raise ValueError(f"Variant changed between stages: {v['name']}")
            variants[v["name"]] = v
        for raw in campaign["runs"]:
            row = dict(raw)
            row["source_campaign"] = str(file.relative_to(destination))
            observation_file = file.parent / raw.get("observation_file", "missing")
            observations = [json.loads(line) for line in observation_file.read_text().splitlines() if line.strip()] if observation_file.exists() else []
            work = [r for r in observations if r.get("event") == "work"]
            row["work_checkpoints"] = {}
            for checkpoint in (5, 15, 30, 60, 180):
                if checkpoint > row["budget_seconds"]:
                    continue
                eligible = [w for w in work if w["observed_seconds"] <= checkpoint]
                last = eligible[-1] if eligible else None
                row["work_checkpoints"][str(checkpoint)] = last
            row["work_counter_scope"] = "Exact main DFS only; excludes warm search. Null means no work snapshot was observed by this checkpoint, not zero work."
            rows.append(row)
    names = ("current_exact15", "wide_exact15", "current_alns15", "elite_alns15")
    combined = {"schema_version": 1, "plan_sha256": sha256(plan_dir / "plan.json"),
        "references_sha256": sha256(plan_dir / "references.json"),
        "fixture_index_sha256": sha256(fixtures / "index.json"), "sources": sources,
        "variants": [variants[n] for n in names if n in variants], "runs": rows,
        "scope": plan["comparison_policy"], "planned_runs": plan["planned_runs"],
        "observed_runs": len(rows), "campaign_complete": len(rows) == plan["planned_runs"]}
    out = destination / "combined"
    out.mkdir(parents=True, exist_ok=True)
    (out / "campaign.json").write_text(json.dumps(combined, indent=2, allow_nan=False) + "\n")
    if rows:
        summary = summarize(combined, references)
        (out / "summary.json").write_text(json.dumps(summary, indent=2, allow_nan=False) + "\n")
        write_report(summary, out / "report.md")
    print(f"Combined {len(rows)}/{plan['planned_runs']} planned runs: {out}", flush=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--fixtures", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--plan-dir", type=Path, default=HERE / "evidence/quick_2026_09_08/long_setup")
    ap.add_argument("--stage", choices=("baseline", "wide", "heuristic", "all", "combine"), default="all")
    ap.add_argument("--resume", action="store_true")
    args = ap.parse_args()
    args.fixtures = args.fixtures.resolve()
    args.out = args.out.resolve()
    args.plan_dir = args.plan_dir.resolve()
    plan = json.loads((args.plan_dir / "plan.json").read_text())
    if sha256(args.fixtures / "index.json") != plan["fixture_index_sha256"]:
        raise ValueError("Fixture index changed after plan was frozen")
    args.out.mkdir(parents=True, exist_ok=True)
    jobs = []
    names = [q["name"] for q in plan["queries"]]
    if args.stage in ("baseline", "all"):
        for budget in sorted({q["baseline_cap_seconds"] for q in plan["queries"]}, reverse=True):
            selected = [q["name"] for q in plan["queries"] if q["baseline_cap_seconds"] == budget]
            jobs.append((f"baseline_{budget}", "baseline_config.json", selected, budget, 1, [plan["baseline_seed"]]))
    if args.stage in ("wide", "all"):
        jobs.append(("wide", "wide_config.json", names, plan["wide_exact_seconds"], 1, [plan["baseline_seed"]]))
    if args.stage in ("heuristic", "all"):
        jobs.append(("heuristic", "heuristic_config.json", names, plan["heuristic_seconds"],
                     len(plan["heuristic_seeds"]), plan["heuristic_seeds"]))
    for name, config, selected, seconds, repeats, seeds in jobs:
        command = [sys.executable, str(HERE / "benchmark_quality.py"),
            "--config", str(args.plan_dir / config), "--fixtures", str(args.fixtures),
            "--out", str(args.out / name), "--references", str(args.plan_dir / "references.json"),
            "--seconds", str(seconds), "--repeat", str(repeats), "--seeds", ",".join(map(str, seeds)),
            "--top-k", str(plan["top_k"]), "--scenarios", *selected]
        if args.resume:
            command.append("--resume")
        print(f"Starting {name}: {len(selected)} queries, {seconds}s cap, {repeats} repeat(s)", flush=True)
        subprocess.run(command, check=True)
        combine(args.out, args.plan_dir, args.fixtures)
    if not jobs:
        combine(args.out, args.plan_dir, args.fixtures)


if __name__ == "__main__":
    main()
