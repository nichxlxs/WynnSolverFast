#!/usr/bin/env python3
"""Export current-meta JS snapshots into Rust enumeration and score fixtures."""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[1]
MANIFEST = REPO / "js" / "solver" / "benchmarks" / "current_meta_suite.json"
TEST = REPO / "js" / "solver" / "tests" / "test_solver_search.js"
FIXTURES = ROOT / "fixtures"


def fixture_stem(snapshot):
    return snapshot.removeprefix("solver_")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--variants", default="known_good,remove_2",
                        help="comma-separated variants or all")
    parser.add_argument("--profiles", default="",
                        help="comma-separated profile IDs; default is all")
    parser.add_argument("--seconds", type=float, default=0.2,
                        help="JS export time cap, used only if export reaches enumeration")
    args = parser.parse_args()

    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    variants = {"known_good", "remove_1", "remove_2", "remove_3", "remove_4", "remove_5", "remove_6"}
    if args.variants != "all":
        variants = {value.strip() for value in args.variants.split(",") if value.strip()}
    profiles = {value.strip() for value in args.profiles.split(",") if value.strip()}
    scenarios = []
    profile_by_snapshot = {
        scenario["snapshot"]: profile["id"]
        for profile in manifest["profiles"]
        for scenario in profile["scenarios"]
    }
    for scenario in manifest["scenarios"]:
        if scenario["variant"] not in variants:
            continue
        if profiles and profile_by_snapshot[scenario["snapshot"]] not in profiles:
            continue
        scenarios.append(scenario)

    FIXTURES.mkdir(parents=True, exist_ok=True)
    failures = []
    for index, scenario in enumerate(scenarios, 1):
        snapshot = scenario["snapshot"]
        stem = fixture_stem(snapshot)
        enum_path = FIXTURES / f"enum_{stem}.txt"
        score_path = FIXTURES / f"score_{stem}.json"
        print(f"[{index}/{len(scenarios)}] {snapshot}")
        env = dict(os.environ)
        env.update({
            "SOLVER_BENCH_SECONDS": str(args.seconds),
            "SOLVER_EXPORT_RUST": str(enum_path),
            "SOLVER_EXPORT_SCORE": str(score_path),
        })
        result = subprocess.run(
            [os.environ.get("NODE", "node"), str(TEST), snapshot],
            cwd=REPO, env=env, text=True, capture_output=True)
        if result.returncode:
            failures.append(snapshot)
            sys.stderr.write(result.stdout + result.stderr)

    if failures:
        raise SystemExit(f"fixture export failed: {', '.join(failures)}")
    print(f"exported {len(scenarios)} scenarios to {FIXTURES}")


if __name__ == "__main__":
    main()
