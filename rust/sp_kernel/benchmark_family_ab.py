#!/usr/bin/env python3
"""Run a validated, counterbalanced fixed-work A/B over family fixtures.

The comparison used by OPTIMIZATION_VALIDATION.md is:

  current:    reachable set-SP bound + adaptive memo (defaults)
  prior-safe: global positive-set-SP bypass + memo forced on

Both modes are correctness-safe. This harness refuses to print evidence when
the kernel output is incomplete, malformed, incomparable, or outside the
declared credited-work tolerance. Medium/large rows remain fixed-work samples,
not exhaustive results.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import os
import platform
import random
import statistics
import subprocess
import sys
from datetime import datetime, timezone
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any

import bench


HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
DEFAULT_MANIFEST = REPO / "js" / "solver" / "benchmarks" / "family_suite.json"
HARNESS_SCHEMA_VERSION = 2
DEFAULT_ORDER_SEED = 20_260_815
CONFIG_NAMES = ("current", "prior_safe")


class EvidenceError(RuntimeError):
    """The measurement is not safe to report as comparable evidence."""


def positive_int(raw: str) -> int:
    value = int(raw)
    if value <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return value


def nonnegative_float(raw: str) -> float:
    value = float(raw)
    if not math.isfinite(value) or value < 0:
        raise argparse.ArgumentTypeError("must be a finite non-negative number")
    return value


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_scores(scores: list[str]) -> tuple[str, ...]:
    parsed: list[tuple[Decimal, str]] = []
    for raw in scores:
        if not isinstance(raw, str) or not raw:
            raise EvidenceError(f"malformed top-15 score {raw!r}")
        try:
            value = Decimal(raw)
        except InvalidOperation as exc:
            raise EvidenceError(f"malformed top-15 score {raw!r}") from exc
        if not value.is_finite():
            raise EvidenceError(f"non-finite top-15 score {raw!r}")
        parsed.append((value, raw))
    parsed.sort(key=lambda pair: (pair[0], pair[1]), reverse=True)
    return tuple(raw for _, raw in parsed)


def score_hash(scores: tuple[str, ...] | list[str]) -> str:
    canonical = canonical_scores(list(scores))
    payload = "\n".join(canonical).encode()
    return hashlib.sha256(payload).hexdigest()[:12]


def load_search_spaces(path: Path) -> tuple[dict[str, int], dict[str, Any]]:
    try:
        document = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise EvidenceError(f"cannot load family manifest {path}: {exc}") from exc
    families = document.get("families")
    if not isinstance(families, list) or not families:
        raise EvidenceError("family manifest has no non-empty 'families' list")

    spaces: dict[str, int] = {}
    for family in families:
        if not isinstance(family, dict) or not isinstance(family.get("family"), str):
            raise EvidenceError("malformed family manifest entry")
        variants = family.get("variants")
        if not isinstance(variants, list) or not variants:
            raise EvidenceError(f"family {family['family']!r} has no variants")
        for variant in variants:
            if not isinstance(variant, dict):
                raise EvidenceError(f"malformed variant for {family['family']!r}")
            scenario = f"fam_{family['family']}_{variant.get('size', '')}"
            raw_space = variant.get("calibrated_search_combinations")
            if isinstance(raw_space, bool) or not isinstance(raw_space, int) or raw_space <= 0:
                raise EvidenceError(f"{scenario}: invalid calibrated search space {raw_space!r}")
            if scenario in spaces:
                raise EvidenceError(f"duplicate manifest scenario {scenario}")
            spaces[scenario] = raw_space
    return spaces, document


def require_int(run: dict[str, Any], field: str, scenario: str, config: str) -> int:
    value = run.get(field)
    if isinstance(value, bool) or not isinstance(value, int):
        raise EvidenceError(f"{scenario}/{config}: missing or malformed integer field {field!r}")
    if value < 0:
        raise EvidenceError(f"{scenario}/{config}: negative field {field!r}: {value}")
    return value


def require_float(run: dict[str, Any], field: str, scenario: str, config: str) -> float:
    value = run.get(field)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise EvidenceError(f"{scenario}/{config}: missing or malformed numeric field {field!r}")
    value = float(value)
    if not math.isfinite(value) or value <= 0:
        raise EvidenceError(f"{scenario}/{config}: field {field!r} must be finite and positive")
    return value


def validate_cli_run(
    run: dict[str, Any], scenario: str, config: str,
) -> dict[str, Any]:
    """Validate every CLI field used by the report and its funnel invariants."""
    exit_code = run.get("exit")
    if isinstance(exit_code, bool) or not isinstance(exit_code, int):
        raise EvidenceError(f"{scenario}/{config}: missing or malformed process exit code")
    if exit_code != 0:
        detail = "; ".join(str(x) for x in run.get("error", []))
        raise EvidenceError(f"{scenario}/{config}: kernel exited {exit_code}: {detail}")

    checked = require_int(run, "checked", scenario, config)
    feasible = require_int(run, "feasible", scenario, config)
    scored = require_int(run, "scored", scenario, config)
    gated = require_int(run, "gated", scenario, config)
    mana_reject = require_int(run, "mana_reject", scenario, config)
    thresh_reject = require_int(run, "thresh_reject", scenario, config)
    require_int(run, "bound_pruned", scenario, config)
    explicit_top_count = require_int(run, "top15_count", scenario, config)
    require_int(run, "rate", scenario, config)
    elapsed = require_float(run, "elapsed", scenario, config)
    wall = require_float(run, "wall", scenario, config)

    if feasible > checked:
        raise EvidenceError(f"{scenario}/{config}: feasible {feasible} exceeds checked {checked}")
    terminal = scored + gated + mana_reject + thresh_reject
    if terminal != feasible:
        raise EvidenceError(
            f"{scenario}/{config}: scored+gated+mana+threshold={terminal} "
            f"does not reconcile to feasible={feasible}"
        )
    if wall + 0.05 < elapsed:
        raise EvidenceError(
            f"{scenario}/{config}: subprocess wall {wall:.3f}s is below CLI elapsed {elapsed:.3f}s"
        )

    reported_rate = run["rate"]
    computed_rate = checked / elapsed
    # CLI elapsed is rounded to milliseconds, so the tolerance includes its
    # worst-case quantisation error plus 2% for the integer rate rendering.
    rate_tolerance = 0.02 + 0.0005 / elapsed
    if computed_rate > 0 and abs(reported_rate - computed_rate) / computed_rate > rate_tolerance:
        raise EvidenceError(
            f"{scenario}/{config}: reported rate {reported_rate} does not reconcile "
            f"with checked/elapsed={computed_rate:.0f}"
        )

    raw_scores = run.get("top_scores")
    raw_items = run.get("top_items")
    if not isinstance(raw_scores, list) or not isinstance(raw_items, list):
        raise EvidenceError(f"{scenario}/{config}: missing parsed top-15 fields")
    if len(raw_scores) != len(raw_items):
        raise EvidenceError(
            f"{scenario}/{config}: {len(raw_scores)} top scores but {len(raw_items)} item rows"
        )
    expected_top_count = min(15, scored)
    if explicit_top_count != len(raw_scores) or explicit_top_count != expected_top_count:
        raise EvidenceError(
            f"{scenario}/{config}: CLI declared {explicit_top_count} top rows, parsed "
            f"{len(raw_scores)}, expected {expected_top_count} from scored={scored}; "
            "output may be missing/malformed"
        )
    scores = canonical_scores(raw_scores)
    return {
        "checked": checked,
        "elapsed": elapsed,
        "credited_rate": checked / elapsed,
        "scores": scores,
        # Empty is accepted only because the authoritative `scoring:` line
        # explicitly reported scored=0 and reconciled above.
        "valid_empty_top": scored == 0 and not scores,
    }


def validate_work(
    validated: dict[str, Any], scenario: str, config: str,
    target: int, expected_space: int,
    max_overshoot_leaves: int, max_overshoot_pct: float,
) -> dict[str, float | int]:
    checked = int(validated["checked"])
    if checked < target:
        raise EvidenceError(
            f"{scenario}/{config}: checked {checked:,} below required target {target:,}; "
            "the wall cap, fixture, or leaf-budget plumbing is invalid"
        )
    if checked > expected_space:
        raise EvidenceError(
            f"{scenario}/{config}: checked {checked:,} exceeds manifest space {expected_space:,}"
        )
    overshoot = checked - target
    overshoot_pct = overshoot * 100.0 / target
    if overshoot > max_overshoot_leaves or overshoot_pct > max_overshoot_pct:
        raise EvidenceError(
            f"{scenario}/{config}: leaf-budget overshoot {overshoot:,} "
            f"({overshoot_pct:.6f}%) exceeds limits {max_overshoot_leaves:,} and "
            f"{max_overshoot_pct:.6f}%"
        )
    return {"target": target, "overshoot": overshoot, "overshoot_pct": overshoot_pct}


def validate_pair(
    scenario: str,
    current: dict[str, Any], prior: dict[str, Any],
    target: int, max_delta_leaves: int, max_delta_pct: float,
) -> dict[str, float | int]:
    if current["scores"] != prior["scores"]:
        raise EvidenceError(f"{scenario}: current/prior-safe top-score sets differ")
    current_checked = int(current["checked"])
    prior_checked = int(prior["checked"])
    delta = abs(current_checked - prior_checked)
    delta_pct = delta * 100.0 / target
    if delta > max_delta_leaves or delta_pct > max_delta_pct:
        raise EvidenceError(
            f"{scenario}: current/prior credited-work delta {delta:,} ({delta_pct:.6f}%) "
            f"exceeds limits {max_delta_leaves:,} and {max_delta_pct:.6f}%"
        )
    speedup = float(current["credited_rate"]) / float(prior["credited_rate"])
    if not math.isfinite(speedup) or speedup <= 0:
        raise EvidenceError(f"{scenario}: invalid normalized speedup {speedup!r}")
    return {"pair_work_delta": delta, "pair_work_delta_pct": delta_pct, "speedup": speedup}


def median(values: list[float]) -> float:
    if not values:
        raise EvidenceError("cannot summarize an empty sample list")
    return float(statistics.median(values))


def paired_config_order(initial: str, repeat_index: int) -> tuple[str, str]:
    if initial not in CONFIG_NAMES or repeat_index < 0:
        raise EvidenceError("invalid counterbalanced order request")
    first = initial
    if repeat_index % 2:
        first = "prior_safe" if first == "current" else "current"
    second = "prior_safe" if first == "current" else "current"
    return first, second


def git_metadata() -> dict[str, Any]:
    def run_git(*args: str) -> str:
        result = subprocess.run(
            ["git", *args], cwd=REPO, capture_output=True, text=True, check=False,
        )
        return result.stdout.strip() if result.returncode == 0 else "unknown"

    status = run_git("status", "--porcelain")
    return {
        "commit": run_git("rev-parse", "HEAD"),
        "dirty": status not in ("", "unknown"),
    }


def cpu_model() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "unknown"


def build_configs(leaf_budget: int) -> dict[str, dict[str, str]]:
    common = {
        "ENUM_LEAF_BUDGET": str(leaf_budget),
        "ALLOW_RELAXED_CONSTRAINTS": "0",
        "UNSAFE_PRECHECKS": "0",
        "SCORE_TRACE": "0",
        "CLUSTER_STATS": "0",
        "BOUND_DEBUG": "0",
        "BOUND_DEPTH": "0",
        "BOUND_TAIL": "1",
        "BOUND_CLUSTER": "4",
        "SUPER_CLUSTER": "1",
        "WARM_K": "6",
        "SCORE_DENSE": "1",
        "SCORE_BOUNDED_DOOM": "1",
    }
    return {
        "current": {
            **common,
            "SP_SET_BOUND_MODE": "reachable",
            "BOUND_MEMO_MODE": "auto",
        },
        "prior_safe": {
            **common,
            "SP_SET_BOUND_MODE": "bypass",
            "BOUND_MEMO_MODE": "on",
        },
    }


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--threads", type=positive_int, default=1)
    parser.add_argument("--leaf-budget", type=positive_int, default=2_000_000)
    parser.add_argument("--time", type=positive_int, default=120,
                        help="safety wall cap for each run")
    parser.add_argument("--repeat", type=positive_int, default=3,
                        help="paired samples per scenario (default: 3)")
    parser.add_argument("--order-seed", type=int, default=DEFAULT_ORDER_SEED,
                        help="deterministic scenario/config counterbalancing seed")
    parser.add_argument("--max-work-overshoot-leaves", type=positive_int, default=10_000)
    parser.add_argument("--max-work-overshoot-pct", type=nonnegative_float, default=0.1)
    parser.add_argument("--max-pair-work-delta-leaves", type=positive_int, default=10_000)
    parser.add_argument("--max-pair-work-delta-pct", type=nonnegative_float, default=0.1)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--scenarios", nargs="*", default=None,
                        help="family scenarios to run (default: all 18)")
    parser.add_argument("--json", type=Path,
                        help="also write metadata and complete parsed run records")
    return parser.parse_args(argv)


def run_benchmark(args: argparse.Namespace) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    if args.threads != 1:
        raise EvidenceError(
            "fixed credited-work A/B currently requires --threads 1; the native CLI "
            "does not yet provide an exact aggregate budget across worker threads"
        )
    if not bench.BIN.is_file():
        raise EvidenceError(f"missing release kernel {bench.BIN}; run cargo build --release")

    spaces, manifest = load_search_spaces(args.manifest)
    scenarios = list(args.scenarios) if args.scenarios is not None else list(bench.FAMILY_SCENARIOS)
    if not scenarios:
        raise EvidenceError("no scenarios selected")
    if len(set(scenarios)) != len(scenarios):
        raise EvidenceError("duplicate scenarios selected")
    unknown = sorted(set(scenarios) - set(bench.FAMILY_SCENARIOS))
    if unknown:
        raise EvidenceError("unknown/non-family scenarios: " + ", ".join(unknown))
    missing_manifest = sorted(set(scenarios) - set(spaces))
    if missing_manifest:
        raise EvidenceError("scenarios missing from manifest: " + ", ".join(missing_manifest))

    fixture_metadata: dict[str, dict[str, Any]] = {}
    for scenario in scenarios:
        enum_name, score_name = bench.SCENARIOS[scenario]
        if not score_name:
            raise EvidenceError(f"{scenario}: fixed-work exact A/B requires a score fixture")
        for name in (enum_name, score_name):
            path = bench.FIX / name
            if not path.is_file() or path.stat().st_size == 0:
                raise EvidenceError(f"{scenario}: missing/empty fixture {path}")
        enum_path = bench.FIX / enum_name
        score_path = bench.FIX / score_name
        fixture_metadata[scenario] = {
            "enum_path": str(enum_path),
            "enum_sha256": file_sha256(enum_path),
            "enum_size_bytes": enum_path.stat().st_size,
            "score_path": str(score_path),
            "score_sha256": file_sha256(score_path),
            "score_size_bytes": score_path.stat().st_size,
        }

    configs = build_configs(args.leaf_budget)
    rng = random.Random(args.order_seed)
    execution_order = list(scenarios)
    rng.shuffle(execution_order)
    initial_first = {scenario: rng.choice(("current", "prior_safe")) for scenario in execution_order}
    sequence = 0
    records: list[dict[str, Any]] = []
    summaries: dict[str, dict[str, Any]] = {}

    for scenario in execution_order:
        expected_space = spaces[scenario]
        target = min(args.leaf_budget, expected_space)
        samples: dict[str, list[dict[str, Any]]] = {"current": [], "prior_safe": []}
        pair_metrics: list[dict[str, Any]] = []
        reference_scores: tuple[str, ...] | None = None

        for repeat_index in range(args.repeat):
            first, second = paired_config_order(initial_first[scenario], repeat_index)
            pair: dict[str, dict[str, Any]] = {}

            for order_index, config in enumerate((first, second)):
                raw = bench.run_one(
                    scenario, args.threads, configs[config], args.time, trace=False,
                )
                validated = validate_cli_run(raw, scenario, config)
                work = validate_work(
                    validated, scenario, config, target, expected_space,
                    args.max_work_overshoot_leaves, args.max_work_overshoot_pct,
                )
                if reference_scores is None:
                    reference_scores = validated["scores"]
                elif validated["scores"] != reference_scores:
                    raise EvidenceError(
                        f"{scenario}/{config}: top-score set differs across repeats/configs"
                    )
                sequence += 1
                sample = {
                    "scenario": scenario,
                    "config": config,
                    "repeat": repeat_index + 1,
                    "order_in_pair": order_index + 1,
                    "execution_sequence": sequence,
                    "target_checked": target,
                    "expected_search_space": expected_space,
                    "work": work,
                    "validated": {
                        **validated,
                        "scores": list(validated["scores"]),
                    },
                    "raw": raw,
                }
                samples[config].append(sample)
                records.append(sample)
                pair[config] = validated

            pair_metrics.append(validate_pair(
                scenario, pair["current"], pair["prior_safe"], target,
                args.max_pair_work_delta_leaves, args.max_pair_work_delta_pct,
            ))

        assert reference_scores is not None
        current_checked = [int(s["validated"]["checked"]) for s in samples["current"]]
        prior_checked = [int(s["validated"]["checked"]) for s in samples["prior_safe"]]
        current_elapsed = [float(s["validated"]["elapsed"]) for s in samples["current"]]
        prior_elapsed = [float(s["validated"]["elapsed"]) for s in samples["prior_safe"]]
        speedups = [float(pair["speedup"]) for pair in pair_metrics]
        summaries[scenario] = {
            "scenario": scenario,
            "repeats": args.repeat,
            "threads": args.threads,
            "leaf_budget": args.leaf_budget,
            "expected_search_space": expected_space,
            "target_checked": target,
            "current_checked": round(statistics.median(current_checked)),
            "prior_safe_checked": round(statistics.median(prior_checked)),
            "current_checked_min": min(current_checked),
            "current_checked_max": max(current_checked),
            "prior_safe_checked_min": min(prior_checked),
            "prior_safe_checked_max": max(prior_checked),
            "current_elapsed_s": median(current_elapsed),
            "prior_safe_elapsed_s": median(prior_elapsed),
            "current_elapsed_min_s": min(current_elapsed),
            "current_elapsed_max_s": max(current_elapsed),
            "prior_safe_elapsed_min_s": min(prior_elapsed),
            "prior_safe_elapsed_max_s": max(prior_elapsed),
            "speedup_current_over_prior_safe": median(speedups),
            "speedup_min": min(speedups),
            "speedup_max": max(speedups),
            "max_pair_work_delta": max(int(pair["pair_work_delta"]) for pair in pair_metrics),
            "max_pair_work_delta_pct": max(float(pair["pair_work_delta_pct"]) for pair in pair_metrics),
            "current_max_overshoot": max(int(s["work"]["overshoot"]) for s in samples["current"]),
            "prior_safe_max_overshoot": max(int(s["work"]["overshoot"]) for s in samples["prior_safe"]),
            "top_scores_equal": True,
            "top_score_count": len(reference_scores),
            "valid_empty_top": len(reference_scores) == 0,
            "current_top_score_sha256_12": score_hash(reference_scores),
            "first_config_pattern": ",".join(
                paired_config_order(initial_first[scenario], i)[0]
                for i in range(args.repeat)
            ),
        }

    generated_at = datetime.now(timezone.utc).isoformat()
    git = git_metadata()
    metadata = {
        "schema_version": HARNESS_SCHEMA_VERSION,
        "valid": True,
        "generated_at_utc": generated_at,
        "command": [sys.executable, *sys.argv],
        "host": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "cpu_count": os.cpu_count(),
            "cpu_model": cpu_model(),
            "python": platform.python_version(),
        },
        "git": git,
        "binary": {
            "path": str(bench.BIN),
            "sha256": file_sha256(bench.BIN),
            "size_bytes": bench.BIN.stat().st_size,
        },
        "manifest": {
            "path": str(args.manifest.resolve()),
            "sha256": file_sha256(args.manifest),
            "schema_version": manifest.get("schema_version"),
            "generated_at": manifest.get("generated_at"),
        },
        "fixtures": fixture_metadata,
        "threads": args.threads,
        "leaf_budget": args.leaf_budget,
        "time_cap_seconds": args.time,
        "repeat": args.repeat,
        "order_seed": args.order_seed,
        "order_strategy": "seeded scenario shuffle; paired config order alternates each repeat",
        "scenario_execution_order": execution_order,
        "work_limits": {
            "max_overshoot_leaves": args.max_work_overshoot_leaves,
            "max_overshoot_pct": args.max_work_overshoot_pct,
            "max_pair_delta_leaves": args.max_pair_work_delta_leaves,
            "max_pair_delta_pct": args.max_pair_work_delta_pct,
            "speedup_basis": "median paired credited-rate ratio",
        },
        "configs": configs,
        "records": records,
    }
    ordered_summaries = [summaries[scenario] for scenario in scenarios]
    return metadata, ordered_summaries


TSV_FIELDS = [
    "scenario", "repeats", "threads", "leaf_budget", "expected_search_space",
    "target_checked", "current_checked", "prior_safe_checked",
    "current_checked_min", "current_checked_max",
    "prior_safe_checked_min", "prior_safe_checked_max",
    "current_elapsed_s", "prior_safe_elapsed_s",
    "current_elapsed_min_s", "current_elapsed_max_s",
    "prior_safe_elapsed_min_s", "prior_safe_elapsed_max_s",
    "speedup_current_over_prior_safe", "speedup_min", "speedup_max",
    "max_pair_work_delta", "max_pair_work_delta_pct",
    "current_max_overshoot", "prior_safe_max_overshoot",
    "top_scores_equal", "top_score_count", "valid_empty_top",
    "current_top_score_sha256_12", "first_config_pattern",
    "generated_at_utc", "git_commit", "git_dirty", "binary_sha256",
    "manifest_sha256", "enum_fixture_sha256", "score_fixture_sha256",
    "time_cap_seconds", "order_seed", "host_cpu_model", "host_platform",
    "current_sp_set_bound_mode", "current_bound_memo_mode",
    "prior_safe_sp_set_bound_mode", "prior_safe_bound_memo_mode",
]


def emit_tsv(metadata: dict[str, Any], summaries: list[dict[str, Any]]) -> None:
    writer = csv.DictWriter(
        sys.stdout, fieldnames=TSV_FIELDS, delimiter="\t", lineterminator="\n",
        extrasaction="ignore",
    )
    writer.writeheader()
    shared = {
        "generated_at_utc": metadata["generated_at_utc"],
        "git_commit": metadata["git"]["commit"],
        "git_dirty": str(metadata["git"]["dirty"]).lower(),
        "binary_sha256": metadata["binary"]["sha256"],
        "manifest_sha256": metadata["manifest"]["sha256"],
        "time_cap_seconds": metadata["time_cap_seconds"],
        "order_seed": metadata["order_seed"],
        "host_cpu_model": metadata["host"]["cpu_model"],
        "host_platform": metadata["host"]["platform"],
        "current_sp_set_bound_mode": metadata["configs"]["current"]["SP_SET_BOUND_MODE"],
        "current_bound_memo_mode": metadata["configs"]["current"]["BOUND_MEMO_MODE"],
        "prior_safe_sp_set_bound_mode": metadata["configs"]["prior_safe"]["SP_SET_BOUND_MODE"],
        "prior_safe_bound_memo_mode": metadata["configs"]["prior_safe"]["BOUND_MEMO_MODE"],
    }
    float_fields = {
        "current_elapsed_s", "prior_safe_elapsed_s", "current_elapsed_min_s",
        "current_elapsed_max_s", "prior_safe_elapsed_min_s", "prior_safe_elapsed_max_s",
        "speedup_current_over_prior_safe", "speedup_min", "speedup_max",
        "max_pair_work_delta_pct",
    }
    for summary in summaries:
        fixture = metadata["fixtures"][summary["scenario"]]
        row = {
            **summary,
            **shared,
            "enum_fixture_sha256": fixture["enum_sha256"],
            "score_fixture_sha256": fixture["score_sha256"],
        }
        for field in float_fields:
            row[field] = f"{float(row[field]):.6f}"
        row["top_scores_equal"] = str(bool(row["top_scores_equal"])).lower()
        row["valid_empty_top"] = str(bool(row["valid_empty_top"])).lower()
        writer.writerow(row)


def main(argv: list[str] | None = None) -> None:
    args = parse_args(argv)
    try:
        metadata, summaries = run_benchmark(args)
    except (EvidenceError, OSError, subprocess.SubprocessError) as exc:
        print(f"benchmark_family_ab: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc

    # Emit only after every scenario/repeat passes. A partial TSV must never
    # be mistaken for a successful all-family evidence artifact.
    if args.json:
        try:
            args.json.write_text(json.dumps({
                **metadata,
                "summaries": summaries,
            }, indent=2))
        except OSError as exc:
            print(f"benchmark_family_ab: FAIL: cannot write {args.json}: {exc}", file=sys.stderr)
            raise SystemExit(1) from exc
    emit_tsv(metadata, summaries)


if __name__ == "__main__":
    main()
