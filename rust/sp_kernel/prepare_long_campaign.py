#!/usr/bin/env python3
"""Freeze the long-query cohort and historical quality targets before new runs.

The three medium and five broad queries are capacity candidates. Prior short
timed windows did not prove their exhaustive completion times. Longer baseline
runs will report observed completion or a lower bound, never rate projections
as measured minutes or hours.
"""
import argparse
import hashlib
import json
from pathlib import Path
import tarfile

from benchmark_quality import finite, sha256
from quality_suite import HERE

MINUTE = ["fam_tierstack_medium", "fam_spellsteal_medium", "fam_hybrid_medium"]
BROAD = ["fam_cancelstack_large", "fam_heavy_melee_large",
         "meta_mage_arcanist_meteor_remove_6", "meta_mage_light_bender_healing_remove_6",
         "spell_8free"]
SEEDS = [707, 808]


def load_prior_campaign(historical, source):
    """Read committed archives without requiring untracked unpacked byproducts.

    Prefer the tracked archive even when a local directory exists: an old or
    edited unpacked campaign must not silently become new reference evidence.
    Verify the archive against its committed manifest and record the selected
    member's digest independently of compression/container metadata.
    """
    archive = historical / f"{source}_raw.tar.gz"
    member = f"{source}/campaign.json"
    file = historical / member
    provenance = {"file": str(file.relative_to(HERE)) if file.is_relative_to(HERE) else str(file)}
    if archive.exists():
        digest = sha256(archive)
        manifest_path = historical / "manifest.json"
        if manifest_path.exists():
            manifest = json.loads(manifest_path.read_text())
            expected = next((r for r in manifest["files"] if r["path"] == archive.name), None)
            if expected is None or expected["sha256"] != digest:
                raise ValueError(f"Historical archive hash mismatch: {archive}")
        with tarfile.open(archive, "r:gz") as packed:
            stream = packed.extractfile(member)
            if stream is None:
                raise ValueError(f"Missing historical campaign member: {archive}:{member}")
            payload = stream.read()
        provenance.update(archive=str(archive.relative_to(HERE)) if archive.is_relative_to(HERE) else str(archive),
                          archive_sha256=digest, archive_member=member)
    else:
        payload = file.read_bytes()
        provenance["archive_unavailable"] = True
    provenance["sha256"] = hashlib.sha256(payload).hexdigest()
    return {**provenance, "runs": json.loads(payload)["runs"]}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--fixtures", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    fixtures = {q["name"]: q for q in json.loads((args.fixtures / "index.json").read_text())["scenarios"]}
    historical = HERE / "evidence/anytime_2026_09_07"
    prior = {}
    for source in ("screen", "confirmation", "diagnostic", "followup"):
        prior[source] = load_prior_campaign(historical, source)
    references = {}
    query_rows = []
    for name in MINUTE + BROAD:
        fixture = fixtures[name]
        if fixture["status"] != "ok" or fixture["dominance_mode"] != "off" or fixture["precheck_mode"] != "disabled":
            raise ValueError(f"{name}: expected valid raw-pool, conservative-precheck fixture")
        for file_key, hash_key in (("enum_file", "enum_sha256"), ("score_file", "score_sha256")):
            if sha256(args.fixtures / fixture[file_key]) != fixture[hash_key]:
                raise ValueError(f"{name}: changed fixture {file_key}")
        eligible = [(source, r) for source, p in prior.items() for r in p["runs"]
                    if r["scenario"] == name and finite(r.get("best_score"))
                    and r["status"] not in ("failed", "unavailable_fixture")
                    and all(r[k] == fixture[k] for k in ("enum_sha256", "score_sha256"))]
        if not eligible:
            raise ValueError(f"{name}: no prior hash-matched endpoint; do not substitute a new-arm maximum")
        best = max(r["best_score"] for _, r in eligible)
        winners = [{"campaign": source, "campaign_sha256": prior[source]["sha256"],
                    "run_id": r["run_id"], "budget_seconds": r["budget_seconds"],
                    "variant": r["variant"], "seed": r["seed"]}
                   for source, r in eligible if r["best_score"] == best]
        references[name] = {"kind": "best_known", "score": best,
            "enum_sha256": fixture["enum_sha256"], "score_sha256": fixture["score_sha256"],
            "source": "Frozen before the long campaign: maximum eligible, same-fixture endpoint across the previously committed screen, confirmation, diagnostic and tuned follow-up campaigns. Not a global optimum certificate.",
            "evidence_kind": "prior_campaign_best_known", "supporting_runs": winners,
            "eligible_prior_runs": len(eligible)}
        query_rows.append({"name": name, "candidate_band": "minutes" if name in MINUTE else "hour_or_larger",
            "completion_time_established": False,
            "classification_note": "Search-size capacity hypothesis only; prior eligible timed runs did not establish exhaustive completion duration.",
            "prior_completed_runs": sum(bool(r["complete"]) for _, r in eligible),
            "search_combinations": fixture.get("search_combinations"),
            "baseline_cap_seconds": 180 if name in MINUTE else 60,
            "enum_sha256": fixture["enum_sha256"], "score_sha256": fixture["score_sha256"]})
    baseline = {"name": "current_exact15", "kind": "enum",
        "binary": str(HERE / "target/release/enum_kernel"),
        "env": {"RESULT_COUNT": "15", "RETAIN_WARM": "1", "WIDE_BOUND_KEYS": "0", "WARM_K": "6", "QUALITY_TRACE_COUNTERS": "1"}}
    common = ["--warm-k", "6", "--warm-budget", "2000000", "--repair-budget", "100000",
              "--max-repairs", "100000", "--cycle-stagnation", "1"]
    current = {"name": "current_alns15", "kind": "lns",
        "binary": str(HERE / "target/release/anytime_kernel"), "env": {"WIDE_BOUND_KEYS": "0"}, "args": common}
    elite = {**current, "name": "elite_alns15", "args": common + ["--elite-pool", "1"]}
    wide = {**baseline, "name": "wide_exact15", "env": {**baseline["env"], "WIDE_BOUND_KEYS": "1"}}
    plan = {"schema_version": 1, "selection_policy": "Fixed before the new long campaign: three original medium families and five larger spell, melee, sustain and healing cases; all failures and no-result runs retained.",
        "queries": query_rows, "top_k": 15, "heuristic_seconds": 30, "heuristic_seeds": SEEDS,
        "baseline_repeats": 1, "baseline_seed": 707,
        "wide_exact_seconds": 30, "wide_exact_repeats": 1,
        "heuristic_profile_adjustment": "Both ALNS arms use max_repairs=100000 for the longer 30-second budget. The earlier five-second profile used 10000; the elite A/B differs only by --elite-pool 1.",
        "comparison_policy": "Use each longer baseline trace's 30-second checkpoint for equal-budget endpoints. Compare time to identical frozen positive targets, not heuristic time versus exhaustive completion. One baseline repeat cannot establish runtime variance or statistical significance.",
        "reference_policy": "Targets are frozen from earlier committed evidence. New maxima are separate best-known extensions and never retroactively replace primary milestones.",
        "runtime_policy": "Native one-thread runs sequentially on the same machine with no competing builds or benchmarks. Generation excluded and recorded separately. Timed tests include launch, parse and warm start.",
        "prior_sources": {s: {k: v for k, v in p.items() if k != "runs"} for s, p in prior.items()},
        "fixture_index_sha256": sha256(args.fixtures / "index.json"),
        "planned_runs": 2 * len(MINUTE + BROAD) + len(MINUTE + BROAD) * len(SEEDS) * 2,
        "all_controls_are_correctness_only": True}
    args.out.mkdir(parents=True, exist_ok=True)
    for name, payload in (("plan.json", plan), ("references.json", references),
                          ("baseline_config.json", {"variants": [baseline]}),
                          ("wide_config.json", {"variants": [wide]}),
                          ("heuristic_config.json", {"variants": [current, elite]})):
        target = args.out / name
        if target.exists():
            raise ValueError(f"Refusing to overwrite a frozen campaign file: {target}")
        target.write_text(json.dumps(payload, indent=2, allow_nan=False) + "\n")
    print(f"Frozen {len(query_rows)} queries, {plan['planned_runs']} planned runs: {args.out}")


if __name__ == "__main__":
    main()
