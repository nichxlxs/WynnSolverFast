#!/usr/bin/env python3
"""Counterbalanced fixed-work A/B between two enumerator configurations.

Why fixed work rather than fixed time
-------------------------------------
A time-capped run answers "how much work fits in N seconds", so the answer
carries every scheduling hiccup that happened during those N seconds. Repeated
often enough the noise averages out, but on this machine a single 6s sample
has a ~15% spread, which is wider than most of the wins worth shipping.

`ENUM_LEAF_BUDGET` pins the work instead: both sides stop after the same number
of credited leaves, and the measurement is the time each took to get there. The
enumerator is deterministic, so at a fixed budget every funnel counter is
reproducible to the leaf -- which turns the comparison into a much stronger
check than timing alone (see `compare_work`).

What this refuses to report
---------------------------
Two configurations are only comparable if they searched the same space. If the
funnel counters or the top-15 scores differ, the timing ratio is meaningless
(one side simply did different work), so the row is marked NOT COMPARABLE and
excluded from the aggregate. That is the failure this harness exists to catch:
a "speedup" that is really a silently narrowed search.

Ordering
--------
Repeats alternate A,B,B,A,... so a machine that drifts warmer or busier over
the run cannot systematically favour whichever side ran first.

Examples
--------
  # Compare a rebuilt binary against a saved reference binary.
  python3 benchmark_ab.py --a-bin /tmp/ref/enum_kernel \\
      --scenarios ehp spell_wide --calibrate-seconds 4 --repeat 5

  # Compare two env configurations of the same binary.
  python3 benchmark_ab.py --b-env WARM_K=0 --scenarios families --repeat 3
"""

from __future__ import annotations

import argparse
import hashlib
import math
import json
import os
import platform
import statistics
import subprocess
import sys
import time
from pathlib import Path

import bench

HERE = Path(__file__).resolve().parent
HARNESS_SCHEMA_VERSION = 1

# Counters that must agree between A and B for the timing to mean anything.
# These describe the shape of the search, not its speed.
WORK_KEYS = ("checked", "leaf_calls", "feasible", "scored", "gated", "bound_pruned")


class EvidenceError(RuntimeError):
    """The measurement is not safe to report as a comparison."""


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_env_assignment(raw: str) -> tuple[str, str]:
    if "=" not in raw:
        raise argparse.ArgumentTypeError(f"expected KEY=VALUE, got {raw!r}")
    key, _, value = raw.partition("=")
    if not key:
        raise argparse.ArgumentTypeError(f"empty variable name in {raw!r}")
    return key, value


def positive_int(raw: str) -> int:
    value = int(raw)
    if value <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return value


def positive_float(raw: str) -> float:
    value = float(raw)
    if value <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return value


def run_once(binary: Path, scenario: str, env_extra: dict, budget: float,
             timeout_s: float) -> dict:
    """One fixed-work run. Threads is pinned to 1: `ENUM_LEAF_BUDGET` is a
    per-worker counter, so a threaded run would cap n*budget aggregate work
    and the kernel refuses it outright."""
    enum_f, score_f = bench.SCENARIOS[scenario]
    cmd = [str(binary), str(bench.FIX / enum_f), "1"]
    if score_f:
        cmd.append(str(bench.FIX / score_f))
    env = dict(os.environ)
    env["ENUM_LEAF_BUDGET"] = repr(float(budget))
    # Backstop only. A run that hits this did not complete its budget and is
    # rejected below rather than reported as a fast result.
    env["ENUM_TIME_CAP_SECS"] = str(timeout_s)
    env.pop("SCORE_TRACE", None)  # tracing perturbs the thing being timed
    env.update(env_extra)

    started = time.perf_counter()
    proc = subprocess.run(cmd, capture_output=True, text=True, env=env,
                          timeout=timeout_s * 4 + 120)
    wall = time.perf_counter() - started
    out = proc.stdout + proc.stderr
    if proc.returncode != 0:
        raise EvidenceError(
            f"{scenario}: kernel exited {proc.returncode}: "
            + " / ".join(out.strip().splitlines()[-3:]))

    match = bench.RE_ENUM.search(out)
    if not match:
        raise EvidenceError(f"{scenario}: no enum_kernel summary line in output")
    row = {
        "checked": int(match.group(1)),
        "precheck_reject": int(match.group(2)),
        "precheck_pass": int(match.group(3)),
        "sp_leaf_reject": int(match.group(4)),
        "sp_kernel_reject": int(match.group(5)),
        "feasible": int(match.group(6)),
        "elapsed": float(match.group(8)),
        "wall": wall,
    }
    if match.group(10) is not None:
        row["leaf_calls"] = int(match.group(10))
    match = bench.RE_SCORING.search(out)
    if match:
        row.update(scored=int(match.group(1)), gated=int(match.group(2)),
                   mana_reject=int(match.group(3)),
                   thresh_reject=int(match.group(4)),
                   bound_pruned=int(match.group(5)))
    row["top_scores"] = [t[0] for t in bench.RE_TOP15.findall(out)]

    if row["checked"] < budget:
        raise EvidenceError(
            f"{scenario}: run stopped at {row['checked']} credited leaves, short of "
            f"the {budget:.0f} budget -- the search space is smaller than the budget, "
            "or the time backstop fired. Lower --budget or raise --timeout.")
    return row


def work_signature(row: dict) -> dict:
    return {k: row[k] for k in WORK_KEYS if k in row}


# `checked` counts the tuple space covered, which a pruning change does not
# alter -- pruned subtrees are credited exactly as if they had been walked. So
# it stays the anchor even when the two sides genuinely disagree about results.
#
# It is not exactly equal, though. The budget is a stopping threshold tested
# after each credit, and a credit can be a whole pruned subtree, so each side
# overshoots by however much its last chunk was worth. Measured overshoot
# between two pruning configurations is ~0.001% -- three orders of magnitude
# below the harness noise floor -- so the anchor holds to a tolerance rather
# than to the leaf.
WORK_ANCHOR_KEYS = ("checked",)
ANCHOR_TOLERANCE = 0.005


def compare_work(a_rows: list[dict], b_rows: list[dict],
                 expect_divergence: bool = False) -> tuple[list[str], list[str]]:
    """Return (problems, divergences).

    `problems` are reasons the timing is not a comparison at all. `divergences`
    are result differences between the two sides.

    By default any divergence is a problem: for an optimization that must
    preserve results, differing counts mean one side simply searched less, and
    the ratio would be measuring that rather than speed.

    `expect_divergence` is for the other case -- a deliberate correctness change,
    where the extra work IS the thing being measured. The result differences are
    then reported rather than fatal, but the anchor still has to hold: both sides
    must cover the same tuple space, or they are not doing comparable work.
    """
    problems, divergences = [], []
    for name, rows in (("A", a_rows), ("B", b_rows)):
        signatures = {json.dumps(work_signature(r), sort_keys=True) for r in rows}
        if len(signatures) > 1:
            problems.append(f"side {name} is non-deterministic across repeats")
        tops = {tuple(r["top_scores"]) for r in rows}
        if len(tops) > 1:
            problems.append(f"side {name} returned different top-15 across repeats")

    a_sig, b_sig = work_signature(a_rows[0]), work_signature(b_rows[0])
    for key in sorted(set(a_sig) | set(b_sig)):
        if a_sig.get(key) == b_sig.get(key):
            continue
        a_val, b_val = a_sig.get(key), b_sig.get(key)
        note = f"{key}: A={a_val} B={b_val}"
        if key in WORK_ANCHOR_KEYS:
            # The anchor may only drift by the last credited chunk.
            scale = max(abs(a_val or 0), abs(b_val or 0), 1)
            drift = abs((a_val or 0) - (b_val or 0)) / scale
            if drift > ANCHOR_TOLERANCE:
                problems.append(
                    f"{note} -- tuple space covered differs by {drift:.3%}, "
                    f"beyond the {ANCHOR_TOLERANCE:.1%} budget-overshoot tolerance")
            continue
        if expect_divergence:
            divergences.append(note)
        else:
            problems.append(note)
    if tuple(a_rows[0]["top_scores"]) != tuple(b_rows[0]["top_scores"]):
        (divergences if expect_divergence else problems).append("top-15 scores differ")
    return problems, divergences


def calibrate_budget(binary: Path, scenario: str, env_extra: dict,
                     seconds: float) -> float:
    """Pick a leaf budget that takes roughly `seconds` on the A side.

    Uses a plain time-capped run and takes the work it completed. The budget
    only has to be identical for both sides, not exact, so one probe is enough.
    A scenario that exhausts its space before the cap just yields the whole
    space as the budget, which makes both sides run the complete search.
    """
    enum_f, score_f = bench.SCENARIOS[scenario]
    cmd = [str(binary), str(bench.FIX / enum_f), "1"]
    if score_f:
        cmd.append(str(bench.FIX / score_f))
    env = dict(os.environ)
    env["ENUM_TIME_CAP_SECS"] = str(seconds)
    env.pop("ENUM_LEAF_BUDGET", None)
    env.pop("SCORE_TRACE", None)
    env.update(env_extra)
    proc = subprocess.run(cmd, capture_output=True, text=True, env=env,
                          timeout=seconds * 4 + 120)
    match = bench.RE_ENUM.search(proc.stdout + proc.stderr)
    if not match or not int(match.group(1)):
        raise EvidenceError(f"{scenario}: calibration run produced no work count")
    return float(match.group(1))


def summarize(a_rows: list[dict], b_rows: list[dict]) -> dict:
    a_med = statistics.median(r["elapsed"] for r in a_rows)
    b_med = statistics.median(r["elapsed"] for r in b_rows)
    # Fixed work: less time is better. Ratio > 1 means B is faster than A.
    return {
        "a_median_s": a_med,
        "b_median_s": b_med,
        "a_times": [r["elapsed"] for r in a_rows],
        "b_times": [r["elapsed"] for r in b_rows],
        "speedup": (a_med / b_med) if b_med > 0 else float("inf"),
        "delta_pct": ((a_med - b_med) / a_med * 100.0) if a_med > 0 else 0.0,
    }


def resolve_scenarios(names: list[str]) -> list[str]:
    out = []
    for name in names:
        if name == "families":
            out.extend(bench.FAMILY_SCENARIOS)
        elif name == "defaults":
            out.extend(bench.DEFAULT_SCENARIOS)
        elif name == "all":
            out.extend(bench.SCENARIOS)
        elif name in bench.SCENARIOS:
            out.append(name)
        else:
            raise SystemExit(f"unknown scenario {name!r}")
    seen, unique = set(), []
    for name in out:
        if name not in seen:
            seen.add(name)
            unique.append(name)
    return unique


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Counterbalanced fixed-work A/B between two kernel configurations")
    parser.add_argument("--a-bin", type=Path, default=bench.BIN,
                        help="baseline binary (default: the built enum_kernel)")
    parser.add_argument("--b-bin", type=Path, default=bench.BIN,
                        help="candidate binary (default: the built enum_kernel)")
    parser.add_argument("--a-env", type=parse_env_assignment, action="append",
                        default=[], metavar="KEY=VALUE")
    parser.add_argument("--b-env", type=parse_env_assignment, action="append",
                        default=[], metavar="KEY=VALUE")
    parser.add_argument("--scenarios", nargs="+", default=["defaults"],
                        help="scenario names, or the groups defaults/families/all")
    parser.add_argument("--budget", type=positive_float,
                        help="credited leaves per run; omit to calibrate")
    parser.add_argument("--calibrate-seconds", type=positive_float, default=5.0,
                        help="target run length when calibrating a budget")
    parser.add_argument("--repeat", type=positive_int, default=5,
                        help="paired repeats per scenario (counterbalanced)")
    parser.add_argument("--timeout", type=positive_float, default=300.0,
                        help="per-run backstop in seconds")
    parser.add_argument("--expect-divergence", action="store_true",
                        help="the change deliberately alters results (a correctness "
                             "fix); report result differences instead of refusing. "
                             "Both sides must still cover the same tuple space.")
    parser.add_argument("--json", type=Path, help="write the full record here")
    parser.add_argument("--tsv", type=Path, help="write a summary table here")
    args = parser.parse_args()

    a_env, b_env = dict(args.a_env), dict(args.b_env)
    if args.a_bin == args.b_bin and a_env == b_env:
        raise SystemExit(
            "A and B are the same binary with the same environment; "
            "nothing to compare. Pass --b-bin or --b-env.")
    for binary in (args.a_bin, args.b_bin):
        if not binary.exists():
            raise SystemExit(f"binary not found: {binary}")

    scenarios = resolve_scenarios(args.scenarios)
    record = {
        "schema_version": HARNESS_SCHEMA_VERSION,
        "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "host": {
            "platform": platform.platform(),
            "python": platform.python_version(),
            **bench.detect_hardware(),
        },
        "a": {"binary": str(args.a_bin), "sha256": file_sha256(args.a_bin),
              "env": a_env},
        "b": {"binary": str(args.b_bin), "sha256": file_sha256(args.b_bin),
              "env": b_env},
        "repeat": args.repeat,
        "threads": 1,
        "rows": [],
    }
    if record["a"]["sha256"] == record["b"]["sha256"] and a_env == b_env:
        raise SystemExit("A and B binaries are byte-identical with identical env")

    rows = []
    for scenario in scenarios:
        enum_fixture = bench.FIX / bench.SCENARIOS[scenario][0]
        row = {"scenario": scenario,
               "fixture_sha256": file_sha256(enum_fixture)}
        try:
            budget = args.budget or calibrate_budget(
                args.a_bin, scenario, a_env, args.calibrate_seconds)
            row["budget"] = budget

            a_runs, b_runs = [], []
            for i in range(args.repeat):
                # ABBA: pair i runs A-first, pair i+1 runs B-first.
                order = ("a", "b") if i % 2 == 0 else ("b", "a")
                for side in order:
                    if side == "a":
                        a_runs.append(run_once(args.a_bin, scenario, a_env,
                                               budget, args.timeout))
                    else:
                        b_runs.append(run_once(args.b_bin, scenario, b_env,
                                               budget, args.timeout))

            problems, divergences = compare_work(
                a_runs, b_runs, expect_divergence=args.expect_divergence)
            row["comparable"] = not problems
            row["problems"] = problems
            row["divergences"] = divergences
            row["a_runs"], row["b_runs"] = a_runs, b_runs
            row["work"] = work_signature(a_runs[0])
            row.update(summarize(a_runs, b_runs))
        except (EvidenceError, subprocess.TimeoutExpired) as exc:
            row["comparable"] = False
            row["problems"] = [str(exc)]
            row["divergences"] = []
        rows.append(row)
        record["rows"].append(row)
        _print_row(row)

    _print_aggregate(rows)

    if args.json:
        args.json.write_text(json.dumps(record, indent=2))
        print(f"\nrecord: {args.json}")
    if args.tsv:
        lines = ["scenario\tbudget\ta_median_s\tb_median_s\tspeedup\tdelta_pct\tcomparable"]
        for row in rows:
            if row.get("comparable"):
                lines.append("\t".join([
                    row["scenario"], f"{row['budget']:.0f}",
                    f"{row['a_median_s']:.3f}", f"{row['b_median_s']:.3f}",
                    f"{row['speedup']:.4f}", f"{row['delta_pct']:+.2f}", "yes"]))
            else:
                lines.append(f"{row['scenario']}\t-\t-\t-\t-\t-\tno")
        args.tsv.write_text("\n".join(lines) + "\n")
        print(f"table:  {args.tsv}")

    if any(not row.get("comparable") for row in rows):
        sys.exit(1)


def _print_row(row: dict) -> None:
    if not row.get("comparable"):
        print(f"{row['scenario']:<28} NOT COMPARABLE")
        for problem in row.get("problems", []):
            print(f"{'':<28}   {problem}")
        return
    print(f"{row['scenario']:<28} A {row['a_median_s']:7.3f}s   "
          f"B {row['b_median_s']:7.3f}s   "
          f"{row['speedup']:6.3f}x  ({row['delta_pct']:+6.2f}%)  "
          f"@ {row['budget']:.3g} leaves")
    for note in row.get("divergences", []):
        print(f"{'':<28}   results differ -- {note}")


def _print_aggregate(rows: list[dict]) -> None:
    good = [r for r in rows if r.get("comparable")]
    skipped = len(rows) - len(good)
    print()
    if not good:
        print("no comparable rows -- nothing to aggregate")
        return
    speedups = [r["speedup"] for r in good]
    geomean = math_geomean(speedups)
    wins = sum(1 for s in speedups if s > 1.0)
    print(f"comparable rows : {len(good)}"
          + (f"   (skipped {skipped})" if skipped else ""))
    print(f"B faster on     : {wins}/{len(good)}")
    print(f"geometric mean  : {geomean:.4f}x")
    print(f"median          : {statistics.median(speedups):.4f}x")
    print(f"range           : {min(speedups):.4f}x .. {max(speedups):.4f}x")


def math_geomean(values: list[float]) -> float:
    if not values:
        return float("nan")
    total = 0.0
    for value in values:
        if value <= 0:
            return float("nan")
        total += math.log(value)
    return math.exp(total / len(values))


if __name__ == "__main__":
    main()
