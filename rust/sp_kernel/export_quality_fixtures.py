#!/usr/bin/env python3
"""Export recovered queries through THIS checkout's production fixture builder.

Historical reference scores/timings are deliberately not imported. Generation
time is recorded separately from native solve time. Every solver arm consumes
the byte-identical generated enum/score pair, recorded by SHA-256.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

from quality_suite import HERE, DEFAULT_MANIFEST, load_manifest, select_scenarios

REPO = HERE.parent.parent


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def resolved_rows(score):
    rows = []
    for row in score.get("parsed_combo", []):
        spell = row.get("spell") or {}
        parts = spell.get("parts")
        if parts is None:
            parts = spell.get("display")
        rows.append({"base_spell": spell.get("base_spell"),
                     "name": spell.get("display_name", spell.get("name")),
                     "damage_parts": len(parts) if parts is not None else None,
                     "mana_excl": row.get("mana_excl"),
                     "dmg_excl": row.get("dmg_excl")})
    return rows


def disable_unproved_prechecks(text):
    """Disable only raw-total and 100-SP HP/EHP rejection predicates.

    Preserve PC count, names, starting totals, and per-item columns so the
    native input schema stays intact. Final score.layer2.restrictions are
    unaffected. This is a conservative relaxation, not an alternate objective.
    """
    lines = []
    for line in text.splitlines():
        fields = line.split()
        if fields and fields[0] == "PC":
            if len(fields) != 4:
                raise ValueError(f"unexpected PC fixture schema: {line}")
            fields[2] = "-1e300"
            line = " ".join(fields)
        elif fields and fields[0] in ("EHP", "EHPNA", "THP"):
            line = " ".join([fields[0]] + ["0"] * (len(fields) - 1))
        lines.append(line)
    return "\n".join(lines) + "\n"


def export_one(query, destination, dominance, samples, timeout, prechecks="disabled"):
    name = query["name"]
    snapshot = REPO / "js/solver/tests/snapshots" / (query["snapshot"] + ".snap.json")
    original = snapshot.read_bytes()
    snap = json.loads(original)
    enum = destination / ("enum_" + name + ".txt")
    score = destination / ("score_" + name + ".json")
    log = destination / (name + ".export.log")
    # Never accept files left over from a previous failed generation attempt.
    enum.unlink(missing_ok=True)
    score.unlink(missing_ok=True)
    env = dict(os.environ, SOLVER_EXPORT_RUST=str(enum), SOLVER_EXPORT_SCORE=str(score),
               SOLVER_EXPORT_SCORE_CASES=str(samples), SOLVER_DOMINANCE_MODE=dominance,
               SOLVER_BENCH_SECONDS="0.01", SOLVER_BENCH_WORKERS="1",
               SOLVER_EXPORT_ALLOW_UNCALIBRATED="1")
    started = time.perf_counter()
    record = {**query, "dominance_mode": dominance, "precheck_mode": prechecks,
              "snapshot_sha256": hashlib.sha256(original).hexdigest(),
              "enum_file": enum.name, "score_file": score.name, "log_file": log.name,
              "requested_score_samples": samples, "status": "failed"}
    try:
        with log.open("w") as fh:
            proc = subprocess.run(["node", "js/solver/tests/test_solver_search.js", query["snapshot"]],
                                  cwd=REPO, env=env, stdout=fh, stderr=subprocess.STDOUT, timeout=timeout)
        record["returncode"] = proc.returncode
        if proc.returncode:
            raise ValueError(f"fixture builder exited {proc.returncode}")
        fixture = json.loads(score.read_text())
        if not enum.is_file() or not fixture.get("layer2"):
            raise ValueError("incomplete enum/score fixture pair")
        record["production_enum_sha256"] = sha256(enum)
        if prechecks == "disabled":
            enum.write_text(disable_unproved_prechecks(enum.read_text()))
        actual = resolved_rows(fixture)
        expected = snap.get("expected_resolved_combo_rows")
        if expected is not None and actual != expected:
            raise ValueError(f"ability contract mismatch: expected {expected!r}, got {actual!r}")
        text = log.read_text()
        input_count = re.search(r"input combinations: ([0-9]+)", text)
        search_count = re.search(r"search combinations: ([0-9]+)", text)
        record.update(status="ok", enum_sha256=sha256(enum), score_sha256=sha256(score),
                      resolved_combo_rows=actual, score_samples=len(fixture.get("cases", [])),
                      input_combinations=int(input_count[1]) if input_count else None,
                      search_combinations=int(search_count[1]) if search_count else None)
        if not record["score_samples"]:
            record["warning"] = "No feasible sampled parity witness; context export alone is not a correctness check."
    except (ValueError, OSError, subprocess.TimeoutExpired) as exc:
        record["error"] = str(exc)
    finally:
        # The legacy test runner fills snapshot freshness fields. They are
        # local generation byproducts, not a modification to the query catalog.
        snapshot.write_bytes(original)
        record["generation_wall_seconds"] = time.perf_counter() - started
    return record


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    ap.add_argument("--scenarios", nargs="+", default=["all"])
    ap.add_argument("--out", type=Path, default=HERE / "fixtures" / "quality")
    ap.add_argument("--dominance", choices=("off", "safe", "legacy"), default="off")
    ap.add_argument("--prechecks", choices=("disabled", "production"), default="disabled",
                    help="disable unproved raw-stat/100-SP EHP gates; preserve final scoring restrictions")
    ap.add_argument("--samples", type=int, default=1)
    ap.add_argument("--timeout", type=float, default=120)
    ap.add_argument("--resume", action="store_true")
    args = ap.parse_args()
    if args.samples < 1 or args.timeout <= 0:
        ap.error("samples and timeout must be positive")
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    queries = select_scenarios(load_manifest(args.manifest), args.scenarios)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip()
    index_path = args.out / "index.json"
    previous = json.loads(index_path.read_text()) if args.resume and index_path.exists() else {}
    rows = {q["name"]: q for q in previous.get("scenarios", [])}
    index = {"schema_version": 1, "generated_by_commit": revision,
             "manifest_sha256": sha256(args.manifest), "dominance_mode": args.dominance,
             "precheck_mode": args.prechecks,
             "scope": "Same configured evaluator and pools; raw dominance=off alone does not prove pruning or game correctness.",
             "scenarios": []}
    failures = 0
    for query in queries:
        old = rows.get(query["name"])
        snapshot_path = REPO / "js/solver/tests/snapshots" / (query["snapshot"] + ".snap.json")
        if (old and old.get("status") == "ok" and old.get("dominance_mode") == args.dominance
                and old.get("precheck_mode") == args.prechecks
                and old.get("requested_score_samples") == args.samples
                and old.get("snapshot_sha256") == sha256(snapshot_path)):
            paths = [(args.out / old["enum_file"], old["enum_sha256"]),
                     (args.out / old["score_file"], old["score_sha256"])]
            if all(p.exists() and sha256(p) == h for p, h in paths):
                print(f"{query['name']}: existing validated pair", flush=True)
                continue
        row = export_one(query, args.out, args.dominance, args.samples, args.timeout, args.prechecks)
        row["generated_by_commit"] = revision
        rows[query["name"]] = row
        failures += row["status"] != "ok"
        index["scenarios"] = list(rows.values())
        index_path.write_text(json.dumps(index, indent=2) + "\n")
        print(f"{query['name']}: {row['status']} {row['generation_wall_seconds']:.2f}s "
              f"{row.get('search_combinations', '')} {row.get('error', '')}", flush=True)
    index["scenarios"] = list(rows.values())
    index_path.write_text(json.dumps(index, indent=2) + "\n")
    raise SystemExit(1 if failures else 0)


if __name__ == "__main__":
    main()
