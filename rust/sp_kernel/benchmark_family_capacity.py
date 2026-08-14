#!/usr/bin/env python3
"""Turn a bench.py family result into exact-space capacity estimates.

The benchmark fixture manifest is the source of truth for the Cartesian work
size. Completed runs report measured elapsed time; incomplete runs report
`space / credited_rate` and are explicitly labeled projected.

Example:
  python3 bench.py --scenarios families --threads 2 --time 30 --repeat 5 \
      --json /tmp/families.json
  python3 benchmark_family_capacity.py /tmp/families.json \
      --config defaults --threads 2

The report never selects the maximum-throughput row. It selects one declared
configuration and thread count, computes per-run capacity, and reports the
median across repeats. JSON output retains the selected source rows.
"""

from __future__ import annotations

import argparse
import json
import math
import statistics
from pathlib import Path

HERE = Path(__file__).resolve().parent
MANIFEST = HERE.parent.parent / "js" / "solver" / "benchmarks" / "family_suite.json"
SIZES = ("small", "medium", "large")
DEADLINES = (
    ("30 seconds", 30),
    ("1 minute", 60),
    ("5 minutes", 5 * 60),
    ("15 minutes", 15 * 60),
    ("1 hour", 60 * 60),
    ("1 day", 24 * 60 * 60),
    ("3 days", 3 * 24 * 60 * 60),
)


def load_spaces(path: Path) -> dict[str, dict]:
    doc = json.loads(path.read_text())
    spaces = {}
    for family in doc["families"]:
        for variant in family["variants"]:
            scenario = f"fam_{family['family']}_{variant['size']}"
            spaces[scenario] = {
                "family": family["family"],
                "weapon": family["core_weapon"],
                "size": variant["size"],
                "space": int(variant["calibrated_input_combinations"]),
            }
    return spaces


def _config_label(rows: list[dict], env: dict, threads: int) -> str:
    """Return the stable label for one scenario/thread/environment group."""
    if not env:
        return "defaults"
    labels = {
        str(row["config"]).removesuffix(f" t={threads}")
        for row in rows if row.get("config")
    }
    if len(labels) > 1:
        raise ValueError(
            "one environment group has multiple config labels: "
            + ", ".join(sorted(labels))
        )
    if labels:
        return next(iter(labels))
    return "env:" + json.dumps(env, sort_keys=True, separators=(",", ":"))


def choose_rows(
    doc: dict, config: str = "defaults", threads: int | None = None,
) -> dict[str, dict]:
    """Select one explicit config/thread group and median its repeats.

    `bench.py` stores every repeat in `results`. It may also store a thread
    sweep or ablation rows. Selecting the maximum `checked`/`rate` across that
    matrix silently combines different experiments, so this function groups
    first and refuses an ambiguous thread selection.
    """
    grouped: dict[tuple[str, int, str], list[dict]] = {}
    envs: dict[tuple[str, int, str], dict] = {}
    for row in doc.get("results", []):
        scenario = str(row.get("scenario", ""))
        if not scenario.startswith("fam_"):
            continue
        try:
            row_threads = int(row["threads"])
        except (KeyError, TypeError, ValueError) as exc:
            raise ValueError(f"{scenario}: missing/invalid threads metadata") from exc
        env = row.get("env", {})
        if not isinstance(env, dict):
            raise ValueError(f"{scenario}: env metadata must be an object")
        env_key = json.dumps(env, sort_keys=True, separators=(",", ":"))
        key = (scenario, row_threads, env_key)
        grouped.setdefault(key, []).append(row)
        envs[key] = env

    candidates = []
    available = set()
    for key, samples in grouped.items():
        scenario, row_threads, _ = key
        env = envs[key]
        label = _config_label(samples, env, row_threads)
        available.add((label, row_threads))
        if label == config and (threads is None or row_threads == threads):
            candidates.append((scenario, row_threads, env, samples))

    if not candidates:
        choices = ", ".join(
            f"{label!r} @ {count} thread(s)" for label, count in sorted(available)
        ) or "none"
        raise ValueError(
            f"no family rows for config {config!r}"
            + (f" and {threads} thread(s)" if threads is not None else "")
            + f"; available: {choices}"
        )

    selected_threads = {candidate[1] for candidate in candidates}
    if threads is None and len(selected_threads) != 1:
        choices = ", ".join(str(value) for value in sorted(selected_threads))
        raise ValueError(
            f"config {config!r} has multiple thread counts ({choices}); "
            "pass --threads explicitly"
        )
    selected_thread = threads if threads is not None else next(iter(selected_threads))

    selected: dict[str, dict] = {}
    for scenario, row_threads, env, samples in candidates:
        if row_threads != selected_thread:
            continue
        if scenario in selected:
            raise ValueError(
                f"{scenario}: config {config!r} maps to more than one environment; "
                "select an unambiguous config label"
            )
        selected[scenario] = {
            "scenario": scenario,
            "threads": row_threads,
            "config": config,
            "env": env,
            "sample_count": len(samples),
            "samples": samples,
        }
    return selected


def _capacity_sample(row: dict, scenario: str, space: int) -> dict:
    """Validate one raw run and attach its measured/projected capacity."""
    if row.get("exit") != 0:
        raise ValueError(f"{scenario}: benchmark process exited {row.get('exit')!r}")
    try:
        checked = int(row["checked"])
        rate = float(row["rate"])
        elapsed = float(row["elapsed"])
    except (KeyError, TypeError, ValueError) as exc:
        raise ValueError(
            f"{scenario}: run is missing checked/rate/elapsed parser output"
        ) from exc
    if checked < 0 or not math.isfinite(rate) or rate <= 0:
        raise ValueError(f"{scenario}: invalid checked/rate ({checked}, {rate})")
    if not math.isfinite(elapsed) or elapsed <= 0:
        raise ValueError(f"{scenario}: invalid elapsed time {elapsed}")
    complete = checked >= space
    seconds = elapsed if complete else space / rate
    return {
        **row,
        "capacity_complete": complete,
        "capacity_seconds": seconds,
    }


def duration(seconds: float) -> str:
    if not math.isfinite(seconds):
        return "-"
    if seconds < 60:
        return f"{seconds:.1f} s"
    if seconds < 3600:
        return f"{seconds / 60:.1f} min"
    if seconds < 86400:
        return f"{seconds / 3600:.2f} h"
    return f"{seconds / 86400:.2f} d"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("results", type=Path, help="bench.py --json output")
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument(
        "--config", required=True,
        help="exact benchmark config label to select (for example: defaults)",
    )
    parser.add_argument(
        "--threads", type=int,
        help="thread count to select; required when the input contains a sweep",
    )
    parser.add_argument("--json", action="store_true", help="emit JSON")
    args = parser.parse_args()

    spaces = load_spaces(args.manifest)
    bench_doc = json.loads(args.results.read_text())
    try:
        rows = choose_rows(bench_doc, config=args.config, threads=args.threads)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc
    missing = sorted(set(spaces) - set(rows))
    if missing:
        raise SystemExit("missing family scenarios: " + ", ".join(missing))

    report = []
    for scenario, meta in spaces.items():
        row = rows[scenario]
        try:
            samples = [
                _capacity_sample(sample, scenario, meta["space"])
                for sample in row["samples"]
            ]
        except ValueError as exc:
            raise SystemExit(str(exc)) from exc
        complete_samples = sum(sample["capacity_complete"] for sample in samples)
        seconds = statistics.median(
            sample["capacity_seconds"] for sample in samples
        )
        report.append({
            **meta,
            "scenario": scenario,
            "threads": row["threads"],
            "config": row["config"],
            "env": row["env"],
            "sample_count": len(samples),
            "complete_samples": complete_samples,
            "checked_median": statistics.median(
                int(sample["checked"]) for sample in samples
            ),
            "rate_median": statistics.median(
                float(sample["rate"]) for sample in samples
            ),
            "elapsed_median": statistics.median(
                float(sample["elapsed"]) for sample in samples
            ),
            "seconds": seconds,
            "complete": complete_samples == len(samples),
            "samples": samples,
        })

    thresholds = {
        label: sum(r["seconds"] <= cutoff for r in report)
        for label, cutoff in DEADLINES
    }
    if args.json:
        source_metadata = {
            key: value for key, value in bench_doc.items() if key != "results"
        }
        print(json.dumps({
            "schema_version": 2,
            "source_metadata": source_metadata,
            "selection": {
                "config": args.config,
                "threads": report[0]["threads"] if report else args.threads,
                "aggregation": "median of per-run capacity; no cross-config/thread maxima",
                "sample_counts": {
                    row["scenario"]: row["sample_count"] for row in report
                },
            },
            "rows": report,
            "thresholds": thresholds,
        }, indent=2))
        return

    sample_counts = sorted({row["sample_count"] for row in report})
    repeats = str(sample_counts[0]) if len(sample_counts) == 1 else "/".join(
        str(value) for value in sample_counts
    )
    selected_threads = report[0]["threads"] if report else args.threads
    print(
        f"Selection: config `{args.config}`, threads `{selected_threads}`, "
        f"repeats `{repeats}`; cells are per-run medians."
    )
    print()
    print("| family | small | medium | large |")
    print("|---|---:|---:|---:|")
    families = []
    for row in report:
        if row["family"] not in families:
            families.append(row["family"])
    by_key = {(r["family"], r["size"]): r for r in report}
    for family in families:
        cells = []
        for size in SIZES:
            row = by_key[(family, size)]
            if row["complete"]:
                suffix = ""
            elif row["complete_samples"] > 0:
                suffix = " mixed/projected"
            else:
                suffix = " projected"
            cells.append(duration(row["seconds"]) + suffix)
        print(f"| {family} | {' | '.join(cells)} |")
    print()
    print("| deadline | rows |")
    print("|---|---:|")
    for label, _ in DEADLINES:
        print(f"| {label} | {thresholds[label]} / {len(report)} |")


if __name__ == "__main__":
    main()
