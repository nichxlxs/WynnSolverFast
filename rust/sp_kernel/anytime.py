#!/usr/bin/env python3
"""Anytime metrics for the enumerator (roadmap R8).

`benchmark_ab.py` answers "how fast is a full proof"; this answers "how good
is the answer if the user stops early". It runs each scenario under a wall
time cap with ANYTIME_TRACE=1, reads the kernel's timestamped incumbent
events, and scores the incumbent curve against a reference optimum:

  primal gap   p(x)  = |R - x| / max(|R|, |x|), 1 with no incumbent
                       (Berthold 2013, "Measuring the impact of primal
                       heuristics"); 0 once the optimum is found.
  primal integral PI = integral of p over [0, T], in seconds. Lower is
                       better; it rewards finding good builds early, not
                       just eventually.
  time to target     = first t with x >= 99% of R, and with x == R.
  final gap          = p at T.

The reference R comes from `--ref` (default anytime_ref.json next to this
file): the best score of a run that exhausted the space ("proven"), or, for
scenarios no run has finished, the best score any run has seen ("best
known", flagged as such; metrics against it are lower bounds on the gap).
`--make-ref` refreshes it by running each scenario to completion, or to
`--ref-timeout`, whichever comes first. Entries carry the fixtures' sha256
and are ignored when a fixture changes.

"Proven" means proven over the fixture's pools, which is what a benchmark
needs, not the scenario's optimum: the general suite exports pools after
the legacy `current` dominance policy and the snapshot's level band, and
family sizes are not nested (tierstack small locks Vivisected, level 105,
which medium's 106-121 band then excludes, so medium's optimum is lower).

Times include fixture load and warm start, measured from process start, so
they compare like with like between configurations of one machine, not
across machines.

Examples
--------
  python3 anytime.py --make-ref --scenarios families --ref-timeout 900
  python3 anytime.py --scenarios fam_hybrid_medium --time 30 \\
      --config base --config nowarm:WARM_K=0 --repeat 3
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

import bench

HERE = Path(__file__).resolve().parent
DEFAULT_REF = HERE / "anytime_ref.json"
SCHEMA = 1

RE_BEST = re.compile(r"^anytime: t=([0-9.]+) best=(\S+)$", re.M)
RE_CUTOFF = re.compile(r"^anytime: t=([0-9.]+) cutoff=(\d+)$", re.M)
RE_COMPLETE = re.compile(r"^search: complete (yes|no)$", re.M)
RE_TOP15 = re.compile(r"^top15: (\S+) \|", re.M)


def fixture_digest(scenario: str) -> str:
    enum_f, score_f = bench.SCENARIOS[scenario]
    h = hashlib.sha256()
    for name in (enum_f, score_f):
        if name:
            h.update((bench.FIX / name).read_bytes())
    return h.hexdigest()


def expand(names):
    out = []
    for n in names:
        if n == "families":
            out += bench.FAMILY_SCENARIOS
        elif n == "defaults":
            out += bench.DEFAULT_SCENARIOS
        elif n in bench.SCENARIOS:
            out.append(n)
        else:
            sys.exit(f"unknown scenario {n!r}")
    return out


def parse_config(text: str):
    """`name` or `name:K=V,K=V`."""
    name, _, rest = text.partition(":")
    env = {}
    for kv in filter(None, rest.split(",")):
        k, _, v = kv.partition("=")
        env[k] = v
    return name, env


def run(binary: Path, scenario: str, threads: int, env_extra: dict, cap: float):
    enum_f, score_f = bench.SCENARIOS[scenario]
    if not score_f:
        raise SystemExit(f"{scenario} has no score fixture; anytime metrics need scores")
    cmd = [str(binary), str(bench.FIX / enum_f), str(threads), str(bench.FIX / score_f)]
    env = dict(os.environ)
    env.update({"ANYTIME_TRACE": "1", "ENUM_TIME_CAP_SECS": str(cap)})
    env.update(env_extra)
    t0 = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True, env=env,
                          timeout=cap * 4 + 180)
    wall = time.time() - t0
    out = proc.stdout + proc.stderr
    if proc.returncode != 0:
        raise SystemExit(f"{scenario}: exit {proc.returncode}\n" + "\n".join(out.splitlines()[-5:]))
    best = [(float(t), float(s)) for t, s in RE_BEST.findall(out)]
    cut = [(float(t), int(c)) for t, c in RE_CUTOFF.findall(out)]
    m = RE_COMPLETE.search(out)
    tops = [float(s) for s in RE_TOP15.findall(out)]
    return {
        "best_events": best,
        "cutoff_events": cut,
        "complete": bool(m and m.group(1) == "yes"),
        "final_best": tops[0] if tops else (best[-1][1] if best else None),
        "wall": wall,
    }


def primal_gap(ref: float, x):
    if x is None:
        return 1.0
    den = max(abs(ref), abs(x))
    if den == 0.0:
        return 0.0
    return min(1.0, abs(ref - x) / den)


def metrics(events, ref: float, horizon: float):
    """Primal integral, times to 99% / 100%, final gap over [0, horizon]."""
    pi = 0.0
    t_prev, x = 0.0, None
    t99 = t100 = None
    for t, s in events:
        t = min(t, horizon)
        pi += primal_gap(ref, x) * (t - t_prev)
        t_prev, x = t, s
        if t99 is None and ref > 0 and s >= 0.99 * ref:
            t99 = t
        if t100 is None and s >= ref - abs(ref) * 1e-12:
            t100 = t
    pi += primal_gap(ref, x) * max(0.0, horizon - t_prev)
    return {"primal_integral": pi, "t99": t99, "t100": t100,
            "final_gap": primal_gap(ref, x), "above_ref": x is not None and x > ref + abs(ref) * 1e-12}


def load_ref(path: Path):
    if path.exists():
        data = json.loads(path.read_text())
        if data.get("schema") == SCHEMA:
            return data
    return {"schema": SCHEMA, "scenarios": {}}


def ref_for(refs, scenario):
    e = refs["scenarios"].get(scenario)
    if e and e.get("fixture_sha256") == fixture_digest(scenario):
        return e
    return None


def make_ref(args, scenarios, binary):
    refs = load_ref(args.ref)
    for sc in scenarios:
        old = ref_for(refs, sc)
        if old and old.get("proven") and not args.force:
            print(f"{sc:32s} proven {old['best']:.10e} (kept)")
            continue
        r = run(binary, sc, args.threads, {}, args.ref_timeout)
        best = r["final_best"]
        if old and not r["complete"] and old.get("best", -math.inf) >= (best or -math.inf):
            print(f"{sc:32s} best known {old['best']:.10e} (kept; this run reached {best})")
            continue
        refs["scenarios"][sc] = {
            "best": best, "proven": r["complete"],
            "fixture_sha256": fixture_digest(sc),
            "seconds": round(r["wall"], 1), "threads": args.threads,
        }
        args.ref.write_text(json.dumps(refs, indent=1, sort_keys=True) + "\n")
        print(f"{sc:32s} {'proven' if r['complete'] else 'best known'} {best:.10e} in {r['wall']:.1f}s")


def fmt_t(v):
    return "-" if v is None else f"{v:.2f}"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--scenarios", nargs="+", default=["families"])
    ap.add_argument("--time", type=float, default=30.0, help="wall cap per run (s)")
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--config", action="append", default=None,
                    help="NAME or NAME:K=V,K=V (repeatable); default: one 'base' config")
    ap.add_argument("--bin", type=Path, default=bench.BIN)
    ap.add_argument("--repeat", type=int, default=1)
    ap.add_argument("--ref", type=Path, default=DEFAULT_REF)
    ap.add_argument("--make-ref", action="store_true")
    ap.add_argument("--ref-timeout", type=float, default=900.0)
    ap.add_argument("--force", action="store_true", help="with --make-ref, rerun proven entries")
    ap.add_argument("--json", type=Path)
    args = ap.parse_args()

    scenarios = expand(args.scenarios)
    if not args.bin.exists():
        sys.exit(f"missing {args.bin}; cargo build --release first")
    if args.make_ref:
        make_ref(args, scenarios, args.bin)
        return

    refs = load_ref(args.ref)
    configs = [parse_config(c) for c in (args.config or ["base"])]
    record = {"schema": SCHEMA, "time": args.time, "threads": args.threads,
              "hardware": bench.detect_hardware(), "rows": []}
    print(f"{'scenario':30s} {'config':10s} {'ref':>10s} {'PI(s)':>8s} {'PI/T':>7s} "
          f"{'t99':>7s} {'t100':>7s} {'gap@T':>9s}")
    for sc in scenarios:
        ref_e = ref_for(refs, sc)
        for rep in range(args.repeat):
            # Alternate the config order per repeat so drift cannot favour one.
            order = configs if rep % 2 == 0 else list(reversed(configs))
            for name, env in order:
                r = run(args.bin, sc, args.threads, env, args.time)
                if ref_e:
                    ref, kind = ref_e["best"], "proven" if ref_e["proven"] else "known"
                else:
                    ref, kind = r["final_best"], "self"
                m = metrics(r["best_events"], ref, args.time) if ref is not None else {}
                row = {"scenario": sc, "config": name, "env": env, "repeat": rep,
                       "ref": ref, "ref_kind": kind, **m, **r}
                record["rows"].append(row)
                flag = " ABOVE REF" if m.get("above_ref") else ""
                print(f"{sc:30s} {name:10s} {kind:>10s} {m.get('primal_integral', math.nan):8.3f} "
                      f"{m.get('primal_integral', math.nan) / args.time:7.4f} "
                      f"{fmt_t(m.get('t99')):>7s} {fmt_t(m.get('t100')):>7s} "
                      f"{m.get('final_gap', math.nan):9.2e}{flag}")
    # Per-config summary: geometric mean of PI (floored at 1 ms so a config
    # that finds the optimum instantly does not zero the product).
    if len(configs) > 1 or args.repeat > 1:
        print("\nper config (geometric mean of primal integral, median t100):")
        for name, _ in configs:
            rows = [r for r in record["rows"] if r["config"] == name and "primal_integral" in r]
            if not rows:
                continue
            gm = math.exp(statistics.mean(math.log(max(r["primal_integral"], 1e-3)) for r in rows))
            t100s = [r["t100"] for r in rows if r["t100"] is not None]
            med = statistics.median(t100s) if t100s else None
            print(f"  {name:10s} PI {gm:8.4f}s | t100 {fmt_t(med)}s "
                  f"({len(t100s)}/{len(rows)} runs reached the reference)")
    if args.json:
        args.json.write_text(json.dumps(record, indent=1))


if __name__ == "__main__":
    main()
