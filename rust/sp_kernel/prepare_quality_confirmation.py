#!/usr/bin/env python3
"""Prepare fixed confirmation arms and freeze independent screening targets.

This changes only the prespecified ALNS warm-depth ablation, never the solver
binary or candidate universe. Freeze must run after the complete screening
matrix and before any confirmation run.
"""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
from pathlib import Path

from benchmark_quality import finite, reference_for, sha256
from quality_suite import DEFAULT_MANIFEST, load_manifest, select_scenarios


def confirmation_config(screen):
    by_name = {variant["name"]: variant for variant in screen["variants"]}
    names = ("safe_baseline15", "best1_warm3", "alns1")
    if any(name not in by_name for name in names):
        raise ValueError("screen is missing a required baseline/small-warm/ALNS arm")
    variants = [copy.deepcopy(by_name[name]) for name in names]
    if variants[2].get("args", []).count("--warm-k") != 1:
        raise ValueError("expected exactly one explicit ALNS --warm-k option")
    index = variants[2]["args"].index("--warm-k")
    if index + 1 >= len(variants[2]["args"]) or str(variants[2]["args"][index + 1]) != "3":
        raise ValueError("screen ALNS must use the prespecified warm depth 3")
    warm6 = copy.deepcopy(variants[2])
    warm6["name"] = "alns_warm6"
    warm6["args"][index + 1] = "6"
    variants.append(warm6)
    return {"variants": variants}


def freeze_screen_references(screen, expected_screen_names, confirmation_names, source):
    arm_names = [variant["name"] for variant in screen["variants"]]
    repeat = screen["repeat"]
    by_query = {}
    for run in screen["runs"]:
        by_query.setdefault(run["scenario"], {}).setdefault(run["variant"], []).append(run)
    missing = [name for name in expected_screen_names if not all(
        len(by_query.get(name, {}).get(arm, [])) == repeat for arm in arm_names)]
    if missing:
        raise ValueError(f"screen incomplete: {len(missing)} query blocks pending/partial; cannot freeze confirmation targets")
    references = {}
    for name in confirmation_names:
        arms = by_query[name]
        rows = [run for runs in arms.values() for run in runs]
        identities = {(run["enum_sha256"], run["score_sha256"]) for run in rows}
        if len(identities) != 1:
            raise ValueError(f"screen contains different fixture pairs for {name}")
        reference = reference_for(name, rows, {})
        if not finite(reference["score"]):
            raise ValueError(f"{name}: screen found no finite reference; do not silently replace a frozen target with a confirmation maximum")
        reference["source"] = f"Frozen maximum observed within screening budgets across all screening arms and repeats; {source}. Written before confirmation. Best-known screening target, not an optimum certificate."
        reference["evidence_kind"] = "frozen_screening_best_known"
        reference["screening_run_ids"] = [run.get("run_id") for run in rows]
        reference["screening_budget_seconds"] = sorted({run["budget_seconds"] for run in rows})
        references[name] = reference
    return references


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--screen", type=Path, required=True)
    ap.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--config-only", action="store_true",
                    help="prepare arms before screening finishes; do not freeze references")
    ap.add_argument("--diagnostic-scenarios", nargs="+",
                    help="separate, explicitly screen-selected diagnostics; does not alter the fixed primary cohort")
    args = ap.parse_args()
    payload = args.screen.read_bytes()
    screen = json.loads(payload)
    config = confirmation_config(screen)
    if args.diagnostic_scenarios:
        config["variants"] = [v for v in config["variants"] if v["name"] != "best1_warm3"]
    for variant in config["variants"]:
        if sha256(variant["binary"]) != variant.get("binary_sha256"):
            raise ValueError(f"{variant['name']}: binary changed since screening; confirmation must use the same executable")
    manifest = load_manifest(args.manifest)
    selected = select_scenarios(manifest, args.diagnostic_scenarios or ["confirmation"])
    screen_names = [query["name"] for query in select_scenarios(manifest, ["all"])]
    diagnostic = bool(args.diagnostic_scenarios)
    prefix = "diagnostic" if diagnostic else "confirmation"
    plan = {"schema_version": 1, "cohort": "screen_selected_diagnostic" if diagnostic else "confirmation", "query_count": len(selected),
            "queries": [query["name"] for query in selected], "seconds": 15 if diagnostic else 5,
            "repeat": 3, "seeds": [1001, 2002, 3003] if diagnostic else [101, 202, 303],
            "arms": [variant["name"] for variant in config["variants"]],
            "selection_policy": "All fifteen recovered archetypes at remove-six, all six original large families, Gaia all-free. No winner-only selection.",
            "scope": "New random seeds on prespecified workload definitions; not unseen query definitions.",
            "screen_path": str(args.screen.resolve()),
            "screen_sha256": hashlib.sha256(payload).hexdigest(),
            "references_frozen": False}
    if diagnostic:
        plan["selection_policy"] = "Selected after screening to investigate a specific observed effect. Separate from the prespecified primary confirmation cohort; not unbiased population evidence."
    references = None
    if not args.config_only:
        source = f"{args.screen.resolve()} (SHA-256 {plan['screen_sha256']})"
        references = freeze_screen_references(screen, screen_names, plan["queries"], source)
        plan["references_frozen"] = True
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / (prefix + "_config.json")).write_text(json.dumps(config, indent=2) + "\n")
    (args.out / (prefix + "_plan.json")).write_text(json.dumps(plan, indent=2) + "\n")
    if references is not None:
        target = args.out / "screening_references.json"
        target.write_text(json.dumps(references, indent=2, allow_nan=False) + "\n")
        print(f"Froze {len(references)} screening targets: {target}")
    print(f"Prepared {len(selected)}-query, {len(config['variants'])}-arm, three-seed {prefix} config: {args.out / (prefix + '_config.json')}")


if __name__ == "__main__":
    main()
