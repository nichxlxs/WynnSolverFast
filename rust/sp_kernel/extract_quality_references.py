#!/usr/bin/env python3
"""Write an independent enumeration config, or extract completed references.

These are best_known references with an explicit
exhaustive_under_current_evaluator evidence label. They are not upgraded to a
game-global optimum: SP allocation, fixed trees, and game-model assumptions are
part of the evaluator being tested.
"""
from __future__ import annotations
import argparse
import json
import math
from pathlib import Path

from benchmark_quality import finite, sha256

ORACLE_ENV = {
    "RESULT_COUNT": "1", "RETAIN_WARM": "0", "WARM_K": "0",
    "SP_BOUND_OFF": "1", "BOUND_DEPTH": "0", "BOUND_TAIL": "0",
    "BOUND_CLUSTER": "0", "SCORE_CEILING_GATE": "0",
}


def oracle_config(binary):
    return {"variants": [{"name": "independent_enumeration", "kind": "enum",
                          "binary": str(Path(binary).resolve()), "env": dict(ORACLE_ENV)}]}


def extract_references(campaign, campaign_source):
    variants = {v["name"]: v for v in campaign["variants"]}
    references = {}
    audit = []
    scenario_names = list(dict.fromkeys(r["scenario"] for r in campaign["runs"]))
    for name in scenario_names:
        runs = [r for r in campaign["runs"] if r["scenario"] == name]
        reasons = []
        valid = []
        for run in runs:
            variant = variants.get(run["variant"], {})
            environment = variant.get("env", {})
            bad_flags = [flag for flag, expected in ORACLE_ENV.items()
                         if str(environment.get(flag)) != expected]
            reason = None
            if variant.get("kind") != "enum":
                reason = "not an enumeration variant"
            elif bad_flags:
                reason = "missing required oracle settings: " + ", ".join(bad_flags)
            elif not run.get("complete") or run.get("status") != "completed":
                reason = "censored or unsuccessful enumeration; completion not established"
            elif run.get("trace_errors"):
                reason = "trace parsing errors"
            elif not finite(run.get("best_score")):
                reason = "completed without a finite feasible score observed within budget"
            elif not run.get("enum_sha256") or not run.get("score_sha256"):
                reason = "missing fixture hashes"
            if reason:
                reasons.append({"run_id": run.get("run_id"), "reason": reason})
            else:
                valid.append(run)
        entry = {"scenario": name, "considered_runs": len(runs),
                 "completed_eligible_runs": len(valid), "rejected_runs": reasons}
        if not valid:
            entry.update(status="not_extracted", reason="no eligible completed oracle run")
            audit.append(entry)
            continue
        hashes = {(r["enum_sha256"], r["score_sha256"]) for r in valid}
        if len(hashes) != 1:
            entry.update(status="not_extracted", reason="completed runs used different fixture hashes")
            audit.append(entry)
            continue
        values = [float(r["best_score"]) for r in valid]
        minimum, maximum = min(values), max(values)
        tolerance = max(abs(maximum), abs(minimum), 1.) * 1e-9
        if maximum - minimum > tolerance:
            entry.update(status="not_extracted", reason="completed oracle repeats disagree", scores=values)
            audit.append(entry)
            continue
        enum_hash, score_hash = next(iter(hashes))
        references[name] = {
            "kind": "best_known", "evidence_kind": "exhaustive_under_current_evaluator",
            "score": maximum, "enum_sha256": enum_hash, "score_sha256": score_hash,
            "source": f"Independent completed enumeration from {campaign_source}; configured early/subtree/leaf score pruning disabled. Exhaustive under the current evaluator, fixed query and greedy SP semantics; not a game-global optimum.",
            "oracle_run_ids": [r.get("run_id") for r in valid],
            "oracle_binary_sha256": sorted({variants[r["variant"]].get("binary_sha256", "unrecorded") for r in valid}),
            "oracle_environment": dict(ORACLE_ENV),
            "observed_complete_scores": values,
        }
        entry.update(status="extracted", score=maximum)
        audit.append(entry)
    return references, {"schema_version": 1, "source": campaign_source,
                        "references_extracted": len(references), "scenarios": audit}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="command", required=True)
    config = sub.add_parser("config", help="write reproducible independent-enumeration settings")
    config.add_argument("--enum-bin", type=Path, required=True)
    config.add_argument("--out", type=Path, required=True)
    extract = sub.add_parser("extract", help="extract only completed, finite, same-fixture references")
    extract.add_argument("--campaign", type=Path, required=True)
    extract.add_argument("--out", type=Path, required=True)
    extract.add_argument("--audit", type=Path)
    args = ap.parse_args()
    if args.command == "config":
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(oracle_config(args.enum_bin), indent=2) + "\n")
        print(args.out)
        return
    campaign = json.loads(args.campaign.read_text())
    source = f"{args.campaign.resolve()} (SHA-256 {sha256(args.campaign)})"
    references, audit = extract_references(campaign, source)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(references, indent=2, allow_nan=False) + "\n")
    audit_path = args.audit or args.out.with_name(args.out.stem + ".audit.json")
    audit_path.parent.mkdir(parents=True, exist_ok=True)
    audit_path.write_text(json.dumps(audit, indent=2, allow_nan=False) + "\n")
    print(f"{len(references)} independent references extracted; {len(audit['scenarios']) - len(references)} scenarios not extracted")
    print(f"references: {args.out}; audit: {audit_path}")


if __name__ == "__main__":
    main()
