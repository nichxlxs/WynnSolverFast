#!/usr/bin/env python3
"""Sequential, counterbalanced time-to-quality campaigns.

Unlike a throughput benchmark, different search orders are allowed. They must
consume exactly the same enum/score fixtures. A speedup compares time to the
SAME target, never heuristic time against exhaustive completion time. Missing
targets and process timeouts remain censored observations, not discarded rows.

Config JSON: {"variants": [{"name": "baseline", "kind": "enum", "binary":
"/absolute/enum_kernel", "env": {"RETAIN_WARM": "0"}}, {"name": "lns",
"kind": "lns", "binary": "/absolute/anytime_kernel", "args": []}]}.
Optional reference JSON maps query names to {"kind":"known_optimum" or
"best_known", "score":123, "enum_sha256":..., "score_sha256":..., "source":...}.
Only explicit, fixture-matched references can be labelled known_optimum.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import time

from quality_suite import DEFAULT_MANIFEST, HERE, load_manifest, select_scenarios

SCHEMA = 1
POLL_SECONDS = 0.005
RE_TOP = re.compile(r"^top15: ([\d.e+\-]+) \| (.*)$", re.M)
RE_FINISH = re.compile(r"complete (true|false)")
CONTROL_ENV = ("QUALITY_TRACE_PATH", "ENUM_TIME_CAP_SECS", "ENUM_LEAF_BUDGET", "SCORE_TRACE")


def finite(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def parse_json_stdout(text):
    """Kernels may print diagnostic lines before their final single-line JSON."""
    for line in reversed(text.splitlines()):
        try:
            parsed = json.loads(line)
            if isinstance(parsed, dict):
                return parsed
        except json.JSONDecodeError:
            continue
    try:
        parsed = json.loads(text)
        return parsed if isinstance(parsed, dict) else {}
    except json.JSONDecodeError:
        return {}


def event_score(event):
    for key in ("score", "best_score", "best"):
        if finite(event.get(key)):
            return float(event[key])
    for key in ("top_n", "top", "top15", "results", "best"):
        value = event.get(key)
        if isinstance(value, dict) and finite(value.get("score")):
            return float(value["score"])
        if isinstance(value, list):
            scores = [x.get("score") for x in value if isinstance(x, dict) and finite(x.get("score"))]
            if scores:
                return float(max(scores))
    return None


def canonical_trace(events):
    """Keep validated monotonic incumbent improvements using observer wall time."""
    result = []
    best = -math.inf
    for event in sorted(events, key=lambda e: e["observed_seconds"]):
        score = event_score(event)
        if score is None or score <= best:
            continue
        best = score
        result.append({**event, "score": score})
    return result


def quality_time(events, reference_score, fraction, limit=math.inf):
    if not finite(reference_score) or reference_score <= 0:
        return None  # Multiplicative quality is undefined for these objectives.
    threshold = reference_score * fraction
    for event in events:
        if event["observed_seconds"] <= limit and event["score"] >= threshold:
            return event["observed_seconds"]
    return None


def score_at(events, seconds):
    reached = [e["score"] for e in events if e["observed_seconds"] <= seconds]
    return max(reached) if reached else None


def normalize_variants(config):
    variants = config["variants"]
    names = set()
    for variant in variants:
        name = variant["name"]
        if not re.fullmatch(r"[A-Za-z0-9_-]+", name) or name in names:
            raise ValueError(f"invalid or duplicate variant name: {name}")
        names.add(name)
        if variant["kind"] not in ("enum", "lns"):
            raise ValueError(f"unknown variant kind: {variant['kind']}")
        variant["binary"] = str(Path(variant["binary"]).resolve())
        variant["binary_sha256"] = sha256(variant["binary"])
        variant["env"] = {k: str(v) for k, v in variant.get("env", {}).items()}
    if not variants:
        raise ValueError("at least one variant is required")
    return variants


def command_for(variant, fixture, fixture_dir, trace_path, seconds, seed, top_k):
    enum = fixture_dir / fixture["enum_file"]
    score = fixture_dir / fixture["score_file"]
    env = dict(os.environ)
    for key in CONTROL_ENV:
        env.pop(key, None)
    env.update(variant.get("env", {}))
    if variant["kind"] == "enum":
        env.update(QUALITY_TRACE_PATH=str(trace_path), ENUM_TIME_CAP_SECS=str(seconds))
        command = [variant["binary"], str(enum), "1", str(score)]
    else:
        command = [variant["binary"], str(enum), str(score), "--seconds", str(seconds),
                   "--seed", str(seed), "--top-k", str(top_k), "--trace", str(trace_path)]
    command.extend(map(str, variant.get("args", [])))
    return command, env


def run_once(query, fixture, variant, fixture_dir, destination, seconds, seed, repeat, top_k):
    run_id = f"{query['name']}__{variant['name']}__r{repeat:02d}__s{seed}"
    trace_path = destination / "traces" / (run_id + ".jsonl")
    observation_path = destination / "observations" / (run_id + ".jsonl")
    stdout_path = destination / "logs" / (run_id + ".stdout.log")
    stderr_path = destination / "logs" / (run_id + ".stderr.log")
    for path in (trace_path, observation_path, stdout_path, stderr_path):
        path.parent.mkdir(parents=True, exist_ok=True)
    trace_path.unlink(missing_ok=True)
    command, env = command_for(variant, fixture, fixture_dir, trace_path, seconds, seed, top_k)
    events, errors = [], []
    position = 0
    partial = b""

    def collect(started, observer):
        nonlocal position, partial
        if not trace_path.exists():
            return
        with trace_path.open("rb") as fh:
            fh.seek(position)
            new = fh.read()
            position = fh.tell()
        chunks = (partial + new).split(b"\n")
        partial = chunks.pop()
        elapsed = time.perf_counter() - started
        for line in chunks:
            if not line.strip():
                continue
            try:
                event = json.loads(line)
                if not isinstance(event, dict):
                    raise ValueError("event must be an object")
                event["observed_seconds"] = elapsed
                events.append(event)
                observer.write(json.dumps(event, allow_nan=False) + "\n")
                observer.flush()
            except (ValueError, json.JSONDecodeError) as exc:
                errors.append(f"invalid trace event: {exc}")

    process_timeout = False
    # Independent hard cap includes process spawn, parse, compile and warm start.
    # Internal clocks cannot silently extend the equal user-visible budgets.
    started = time.perf_counter()
    with stdout_path.open("w") as stdout, stderr_path.open("w") as stderr, observation_path.open("w") as observer:
        proc = subprocess.Popen(command, stdout=stdout, stderr=stderr, env=env)
        while proc.poll() is None:
            collect(started, observer)
            elapsed = time.perf_counter() - started
            if elapsed >= seconds:
                # Give a cooperative solver at most one polling interval to
                # flush final output. Results after the budget remain excluded.
                try:
                    proc.wait(timeout=POLL_SECONDS)
                except subprocess.TimeoutExpired:
                    process_timeout = True
                    proc.kill()
                break
            time.sleep(min(POLL_SECONDS, seconds - elapsed))
        proc.wait()
        wall = time.perf_counter() - started
        collect(started, observer)
        text = stdout_path.read_text()
        parsed = parse_json_stdout(text)
        scores = [float(match[0]) for match in RE_TOP.findall(text)]
        final_score = event_score(parsed)
        if final_score is None and scores:
            final_score = max(scores)
        if final_score is not None:
            event = {"event": "final_stdout", "score": final_score, "observed_seconds": wall}
            events.append(event)
            observer.write(json.dumps(event) + "\n")
    finish_events = [e for e in events if e.get("event") == "finish"]
    complete = parsed.get("complete")
    if finish_events:
        complete = finish_events[-1].get("complete", complete)
    if complete is None:
        match = RE_FINISH.search(text)
        complete = match[1] == "true" if match else False
    trajectory = canonical_trace(events)
    eligible = [e for e in trajectory if e["observed_seconds"] <= seconds]
    best = max((e["score"] for e in eligible), default=None)
    if proc.returncode and not process_timeout:
        status = "failed"
    elif process_timeout:
        status = "timeout"
    elif complete:
        status = "completed"
    else:
        status = "capped"
    return {"run_id": run_id, "scenario": query["name"], "group": query["group"],
            "variant": variant["name"], "kind": variant["kind"], "seed": seed, "repeat": repeat,
            "budget_seconds": seconds, "wall_seconds": wall, "status": status,
            "returncode": proc.returncode, "complete": bool(complete) and not process_timeout,
            "best_score": best, "has_result_within_budget": best is not None,
            "enum_sha256": fixture["enum_sha256"], "score_sha256": fixture["score_sha256"],
            "dominance_mode": fixture.get("dominance_mode"), "trajectory": trajectory,
            "trace_errors": errors, "final_summary": parsed,
            "command": command, "env_overrides": variant.get("env", {}),
            "stdout_file": str(stdout_path.relative_to(destination)),
            "stderr_file": str(stderr_path.relative_to(destination)),
            "trace_file": str(trace_path.relative_to(destination)),
            "observation_file": str(observation_path.relative_to(destination))}


def reference_for(name, rows, supplied):
    reference = supplied.get(name)
    if reference:
        if reference.get("kind") not in ("known_optimum", "best_known"):
            raise ValueError(f"{name}: invalid reference kind")
        if not finite(reference.get("score")):
            raise ValueError(f"{name}: non-finite reference")
        for field in ("enum_sha256", "score_sha256"):
            if any(reference.get(field) != row[field] for row in rows):
                raise ValueError(f"{name}: reference does not match fixture {field}")
        if not reference.get("source"):
            raise ValueError(f"{name}: reference requires provenance")
        # Exceeding a purported optimum is a correctness/reference failure.
        if reference["kind"] == "known_optimum" and any(
                finite(row["best_score"]) and row["best_score"] > reference["score"] + max(abs(reference["score"]), 1) * 1e-8
                for row in rows):
            raise ValueError(f"{name}: observed score exceeds claimed optimum")
        return reference
    values = [row["best_score"] for row in rows if finite(row["best_score"]) and row["status"] != "failed"]
    return {"kind": "best_known", "score": max(values) if values else None,
            "source": "in_sample maximum across all campaign arms/repeats; not a certificate",
            "enum_sha256": rows[0]["enum_sha256"], "score_sha256": rows[0]["score_sha256"]}


def summarize(record, references=None):
    rows = record["runs"]
    summaries = []
    for name in dict.fromkeys(row["scenario"] for row in rows):
        scenario_rows = [row for row in rows if row["scenario"] == name]
        fixture_ids = {(row["enum_sha256"], row["score_sha256"]) for row in scenario_rows}
        if len(fixture_ids) != 1:
            raise ValueError(f"{name}: mixed fixtures across arms")
        reference = reference_for(name, scenario_rows, references or {})
        summary = {"scenario": name, "reference": reference, "variants": []}
        for variant in record["variants"]:
            runs = [row for row in scenario_rows if row["variant"] == variant["name"]]
            if not runs:
                continue
            entry = {"variant": variant["name"], "runs": len(runs),
                     "failures": sum(r["status"] in ("failed", "unavailable_fixture") for r in runs),
                     "timeouts": sum(r["status"] == "timeout" for r in runs),
                     "no_result": sum(r["best_score"] is None for r in runs),
                     "completed": sum(r["complete"] for r in runs),
                     "final_scores": [r["best_score"] for r in runs], "targets": {}}
            for fraction in (0.95, 0.99, 0.999):
                times = [quality_time(r["trajectory"], reference["score"], fraction, r["budget_seconds"])
                         if r["status"] not in ("failed", "unavailable_fixture") else None for r in runs]
                attained = [t for t in times if t is not None]
                # A successes-only median would bias random search comparisons.
                # Censored observations are +inf for the unconditional median;
                # null reports that at least half did not attain the target.
                uncensored_median = statistics.median([t if t is not None else math.inf for t in times])
                entry["targets"][str(fraction)] = {"times_seconds": times, "attained": len(attained),
                    "total": len(times), "success_rate": len(attained) / len(times),
                    "median_seconds": uncensored_median if math.isfinite(uncensored_median) else None,
                    "median_is_censored": not math.isfinite(uncensored_median),
                    "successful_only_median_seconds": statistics.median(attained) if attained else None}
            entry["checkpoints"] = {}
            checkpoints = sorted(set([1., 5., 15., 30., 60.] + [r["budget_seconds"] for r in runs]))
            for checkpoint in checkpoints:
                if checkpoint > max(r["budget_seconds"] for r in runs):
                    continue
                scores = [score_at(r["trajectory"], checkpoint) if checkpoint <= r["budget_seconds"] else None for r in runs]
                entry["checkpoints"][str(checkpoint)] = {"scores": scores,
                    "reference_ratios": [s / reference["score"] if finite(s) and finite(reference["score"]) and reference["score"] > 0 else None for s in scores]}
            summary["variants"].append(entry)
        baseline = summary["variants"][0] if summary["variants"] else None
        summary["comparisons"] = []
        for candidate in summary["variants"][1:]:
            a, b = baseline["targets"]["0.99"], candidate["targets"]["0.99"]
            # Conservative reporting: require every repeat on BOTH arms to
            # attain. Otherwise report success/censoring, never a numeric ratio.
            ratio = None
            if (a["attained"] == a["total"] and b["attained"] == b["total"]
                    and b["median_seconds"] and a["median_seconds"]):
                ratio = a["median_seconds"] / b["median_seconds"]
            summary["comparisons"].append({"baseline": baseline["variant"], "candidate": candidate["variant"],
                "t99_speedup": ratio, "baseline_attained": a["attained"], "candidate_attained": b["attained"],
                "note": "Ratio only when all repeats reach the same positive reference target."})
        summaries.append(summary)
    return {"schema_version": SCHEMA, "scope": "Time to same target on identical configured fixtures; no global/game-accuracy certificate inferred.",
            "reference_semantics": "best_known defaults to in-sample maximum; independently supplied known_optimum must match both fixture hashes",
            "rows": summaries, "run_count": len(rows)}


def write_report(summary, path):
    lines = ["# Time-to-quality benchmark", "",
             "Process wall time includes launch, parsing, compilation, and warm search. Incumbent traces are observed by the parent process every 5 ms; delivery and trace overhead are included. Fixture generation is recorded separately.", "",
             "References are labelled best-known unless independently established for the identical query. Default references are the maximum found by this campaign, so percentages do not certify proximity to the global optimum. Timeouts and no-result runs remain in all denominators.", "",
             "| Scenario | Variant | T99 successes | T99 median (s) | No result | Completed | T99 speedup vs first arm |",
             "|---|---|---:|---:|---:|---:|---:|"]
    for row in summary["rows"]:
        comparisons = {c["candidate"]: c for c in row["comparisons"]}
        for variant in row["variants"]:
            target = variant["targets"]["0.99"]
            median = f"{target['median_seconds']:.4f}" if target["median_seconds"] is not None else "censored"
            ratio = comparisons.get(variant["variant"], {}).get("t99_speedup")
            speed = f"{ratio:.2f}×" if ratio is not None else "—"
            lines.append(f"| {row['scenario']} | {variant['variant']} | {target['attained']}/{target['total']} | {median} | {variant['no_result']} | {variant['completed']} | {speed} |")
    lines.extend(["", "A censored T99 median means the target was not reached in at least half the runs. Speedup is omitted unless every repeat on both arms reached the same target. Completion time is never substituted for baseline T99.", ""])
    path.write_text("\n".join(lines))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--config", type=Path)
    ap.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    ap.add_argument("--fixtures", type=Path, default=HERE / "fixtures" / "quality")
    ap.add_argument("--scenarios", nargs="+", default=["wide_screen"])
    ap.add_argument("--seconds", type=float, default=5)
    ap.add_argument("--repeat", type=int, default=3)
    ap.add_argument("--seeds", default="1,2,3")
    ap.add_argument("--top-k", type=int, default=1)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--references", type=Path)
    ap.add_argument("--summarize-only", action="store_true")
    ap.add_argument("--resume", action="store_true")
    args = ap.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    record_path = args.out / "campaign.json"
    reference = json.loads(args.references.read_text()) if args.references else {}
    if args.summarize_only:
        record = json.loads(record_path.read_text())
    else:
        if not args.config or args.seconds <= 0 or args.repeat < 1 or not 1 <= args.top_k <= 15:
            ap.error("config, positive seconds/repeat and top-k 1..15 are required")
        variants = normalize_variants(json.loads(args.config.read_text()))
        queries = select_scenarios(load_manifest(args.manifest), args.scenarios)
        seeds = [int(s) for s in args.seeds.split(",")]
        args.fixtures = args.fixtures.resolve()
        index = json.loads((args.fixtures / "index.json").read_text())
        fixture_map = {q["name"]: q for q in index["scenarios"]}
        for query in queries:
            fixture = fixture_map.get(query["name"])
            if not fixture or fixture.get("status") != "ok":
                # Keep failed exports visible in the requested matrix. They
                # are unavailable queries, never zero-second solve successes.
                continue
            for field, hash_field in (("enum_file", "enum_sha256"), ("score_file", "score_sha256")):
                if sha256(args.fixtures / fixture[field]) != fixture[hash_field]:
                    ap.error(f"fixture hash changed: {query['name']} {field}")
        record = {"schema_version": SCHEMA, "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                  "platform": platform.platform(), "python": platform.python_version(), "cpu_count": os.cpu_count(),
                  "variants": variants, "fixtures_index_sha256": sha256(args.fixtures / "index.json"),
                  "poll_seconds": POLL_SECONDS, "seconds": args.seconds, "repeat": args.repeat,
                  "seeds": seeds, "runs": [], "generation_timing_excluded": True}
        if args.resume and record_path.exists():
            old = json.loads(record_path.read_text())
            for field in ("variants", "fixtures_index_sha256", "seconds", "repeat", "seeds"):
                if old[field] != record[field]:
                    ap.error(f"cannot resume changed campaign field: {field}")
            record = old
        already = {r["run_id"] for r in record["runs"]}
        for query_index, query in enumerate(queries):
            for repeat in range(args.repeat):
                seed = seeds[repeat % len(seeds)]
                # Rotate first arm by query and repeat, reverse alternate rows.
                offset = query_index % len(variants)
                order = variants[offset:] + variants[:offset]
                if repeat % 2:
                    order = order[::-1]
                for variant in order:
                    run_id = f"{query['name']}__{variant['name']}__r{repeat:02d}__s{seed}"
                    if run_id in already:
                        continue
                    fixture = fixture_map.get(query["name"], {})
                    if fixture.get("status") == "ok":
                        row = run_once(query, fixture, variant, args.fixtures, args.out,
                                       args.seconds, seed, repeat, args.top_k)
                    else:
                        row = {"run_id": run_id, "scenario": query["name"], "group": query["group"],
                               "variant": variant["name"], "seed": seed, "repeat": repeat,
                               "budget_seconds": args.seconds, "wall_seconds": 0,
                               "status": "unavailable_fixture", "complete": False,
                               "best_score": None, "trajectory": [], "enum_sha256": None,
                               "score_sha256": None, "error": fixture.get("error", "fixture not exported")}
                    record["runs"].append(row)
                    record_path.write_text(json.dumps(record, indent=2, allow_nan=False) + "\n")
                    with (args.out / "runs.jsonl").open("a") as fh:
                        fh.write(json.dumps(row, allow_nan=False) + "\n")
                    print(f"{run_id}: {row['status']} best={row['best_score']} wall={row['wall_seconds']:.3f}s", flush=True)
    summary = summarize(record, reference)
    (args.out / "summary.json").write_text(json.dumps(summary, indent=2, allow_nan=False) + "\n")
    write_report(summary, args.out / "report.md")
    print(f"{summary['run_count']} runs; report {args.out / 'report.md'}")


if __name__ == "__main__":
    main()
