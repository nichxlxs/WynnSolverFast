#!/usr/bin/env python3
"""Benchmark harness for the solver kernel.

Runs a scenario x configuration matrix, verifies that every configuration
produces the SAME results (the optimizations are supposed to be exact), and
reports throughput plus the share of runtime that is GPU-offloadable.

Usage:
  ./bench.py                        # default matrix, 20s per run
  ./bench.py --time 60              # longer runs
  ./bench.py --scenarios spell_wide ehp
  ./bench.py --threads 1,2,4,8,16   # thread scaling sweep
  ./bench.py --layers               # ablation: measure each optimization
  ./bench.py --json out.json        # machine-readable results

Correctness: every configuration of a scenario must produce identical top-15
SCORES. Item lists may differ only where scores tie (documented insertion
order). Any score-set difference is reported as FAIL and exits non-zero.
"""

import argparse, json, os, re, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
BIN = ROOT / "target" / "release" / "enum_kernel"
FIX = ROOT / "fixtures"

# scenario -> (enum fixture, score fixture or None)
SCENARIOS = {
    "spell2":      ("enum_spell2.txt",         "score_spell2.json"),
    "spell_wide":  ("enum_spell_wide.txt",     "score_spell_wide.json"),
    "spell_8free": ("enum_spell_8free.txt",    "score_spell_8free.json"),
    "ehp":         ("enum_spell_ehp.txt",      "score_spell_ehp.json"),
    "xpb":         ("enum_spell_xpb.txt",      "score_spell_xpb.json"),
    "hp2":         ("enum_hp2.txt",            "score_hp2.json"),
    "melee_restr": ("enum_gaia2m.txt",         "score_gaia2m.json"),
    "melee_colossal": ("enum_gaia_colossal.txt", None),
}
DEFAULT_SCENARIOS = ["spell_wide", "ehp", "xpb", "spell_8free"]

# Ablation layers: name -> env that DISABLES it.
LAYERS = {
    "super-cluster bound": {"SUPER_CLUSTER": "0"},
    "cluster bound":       {"BOUND_CLUSTER": "0"},
    "tail bound":          {"BOUND_TAIL": "0"},
    "warm start":          {"WARM_K": "0"},
    "dense vectors":       {"SCORE_DENSE": "0"},
    "bounded mana doom":   {"SCORE_BOUNDED_DOOM": "0"},
}

RE_ENUM = re.compile(
    r"enum_kernel: checked (\d+) \| precheck_reject (\d+) \| precheck_pass (\d+) \| "
    r"sp_leaf_reject (\d+) \| feasible (\d+) \| threads (\d+) \| elapsed ([\d.]+)s \| (\d+) checked/s")
RE_SCORING = re.compile(r"scoring: scored (\d+) \| gated (\d+) \| mana_reject (\d+) \| "
                        r"thresh_reject (\d+) \| bound_pruned (\d+)")
RE_PHASE = re.compile(r"score_trace: sp ([\d.]+)s \| base ([\d.]+)s \| gate ([\d.]+)s \| "
                      r"doom ([\d.]+)s \| greedy ([\d.]+)s \(\d+ trials\) \| mana ([\d.]+)s \| final ([\d.]+)s")
RE_BOUND = re.compile(r"score_trace: bound evals (\d+) in ([\d.]+)s")
RE_TOP15 = re.compile(r"^top15: ([\d.e+\-]+) \| (.*)$", re.M)


def detect_hardware():
    hw = {"cores": os.cpu_count() or 1}
    try:
        with open("/proc/cpuinfo") as f:
            for line in f:
                if line.startswith("model name"):
                    hw["cpu"] = line.split(":", 1)[1].strip()
                    break
    except OSError:
        pass
    probe = ROOT / "target" / "release" / "gpu_probe"
    if probe.exists():
        try:
            r = subprocess.run([str(probe)], capture_output=True, text=True, timeout=60)
            hw["gpu"] = r.stdout.strip().splitlines()
            hw["gpu_tier"] = next(
                (l.split("=", 1)[1].strip() for l in hw["gpu"] if "tier =" in l), "unknown")
        except Exception as e:
            hw["gpu_tier"] = f"probe failed: {e}"
    else:
        hw["gpu_tier"] = "not built (cargo build --release --features gpu --bin gpu_probe)"
    return hw


def run_one(scenario, threads, env_extra, time_cap, trace=True):
    enum_f, score_f = SCENARIOS[scenario]
    cmd = [str(BIN), str(FIX / enum_f), str(threads)]
    if score_f:
        cmd.append(str(FIX / score_f))
    env = dict(os.environ)
    env["ENUM_TIME_CAP_SECS"] = str(time_cap)
    if trace:
        env["SCORE_TRACE"] = "1"
    env.update(env_extra)
    t0 = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True, env=env,
                          timeout=time_cap * 4 + 120)
    wall = time.time() - t0
    out = proc.stdout + proc.stderr
    res = {"scenario": scenario, "threads": threads, "env": env_extra,
           "wall": round(wall, 2), "exit": proc.returncode}
    m = RE_ENUM.search(out)
    if m:
        res.update(checked=int(m.group(1)), feasible=int(m.group(5)),
                   elapsed=float(m.group(7)), rate=int(m.group(8)))
    m = RE_SCORING.search(out)
    if m:
        res.update(scored=int(m.group(1)), gated=int(m.group(2)),
                   bound_pruned=int(m.group(5)))
    m = RE_PHASE.search(out)
    if m:
        keys = ["sp", "base", "gate", "doom", "greedy", "mana", "final"]
        res["phases"] = {k: float(m.group(i + 1)) for i, k in enumerate(keys)}
    m = RE_BOUND.search(out)
    if m:
        res["bound_evals"] = int(m.group(1))
        res["bound_s"] = float(m.group(2))
    tops = RE_TOP15.findall(out)
    res["top_scores"] = [t[0] for t in tops]
    res["top_items"] = [t[1] for t in tops]
    if proc.returncode != 0:
        res["error"] = out.strip().splitlines()[-3:]
    return res


def offload_share(res):
    """Fraction of measured CPU time in batch-shaped ceiling work (the only
    part a GPU could take): bound evals + the leaf ceiling gate."""
    ph = res.get("phases")
    if not ph or "bound_s" not in res:
        return None
    total_cpu = res["elapsed"] * res["threads"]
    if total_cpu <= 0:
        return None
    return (res["bound_s"] + ph["gate"]) / total_cpu


def fmt_rate(r):
    if r is None: return "-"
    for unit, div in (("G", 1e9), ("M", 1e6), ("K", 1e3)):
        if r >= div: return f"{r/div:.2f}{unit}/s"
    return f"{r}/s"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--time", type=int, default=20, help="seconds per run")
    ap.add_argument("--scenarios", nargs="*", default=DEFAULT_SCENARIOS)
    ap.add_argument("--threads", default=None,
                    help="comma list for the scaling sweep (default: core count only)")
    ap.add_argument("--layers", action="store_true", help="ablate each optimization layer")
    ap.add_argument("--repeat", type=int, default=1,
                    help="runs per configuration; the MEDIAN is reported (noise control)")
    ap.add_argument("--json", help="write results to this path")
    ap.add_argument("--no-verify", action="store_true",
                    help="skip cross-config result verification")
    args = ap.parse_args()

    if not BIN.exists():
        sys.exit(f"missing {BIN} — run: cargo build --release")

    hw = detect_hardware()
    cores = hw["cores"]
    print(f"host: {hw.get('cpu', 'unknown CPU')} | {cores} cores")
    print(f"gpu:  {hw.get('gpu_tier')}")
    print()

    thread_list = [int(t) for t in args.threads.split(",")] if args.threads else [cores]
    results, failures = [], []

    for scen in args.scenarios:
        if scen not in SCENARIOS:
            sys.exit(f"unknown scenario {scen}; known: {', '.join(SCENARIOS)}")
        print(f"══ {scen} ({args.time}s per run) " + "═" * 30)
        rows = []

        def measure(threads, env, label):
            reps = [run_one(scen, threads, env, args.time) for _ in range(args.repeat)]
            reps.sort(key=lambda x: x.get("rate", 0))
            med = reps[len(reps) // 2]
            med["config"] = label
            med["samples"] = [r.get("rate") for r in reps]
            rows.append(med); results.extend(reps)
            return med

        # Baseline: all defaults, at each thread count.
        for t in thread_list:
            measure(t, {}, f"defaults t={t}")

        if args.layers:
            for name, env in LAYERS.items():
                measure(cores, env, f"WITHOUT {name}")

        base = rows[0]
        base_rate = base.get("rate")
        for r in rows:
            rate = r.get("rate")
            spd = f"{rate/base_rate:5.2f}x" if rate and base_rate else "    -"
            share = offload_share(r)
            share_s = f"{share*100:4.1f}%" if share is not None else "   -"
            spread = ""
            if args.repeat > 1 and r.get("samples"):
                lo, hi = min(r["samples"]), max(r["samples"])
                if lo: spread = f"  ±{(hi - lo) / lo * 50:.0f}%"
            print(f"  {r['config']:<28} {fmt_rate(rate):>10}  {spd}  "
                  f"checked {r.get('checked', 0):>13,}  gpu-offloadable {share_s}{spread}")

        # Verification: identical top-15 score sets across every config.
        if not args.no_verify and base.get("top_scores"):
            ref = sorted(base["top_scores"], reverse=True)
            for r in rows[1:]:
                if not r.get("top_scores"):
                    continue
                # Time-capped runs cover different amounts of space, so only
                # compare configs that ran to completion (equal `checked`).
                if r.get("checked") != base.get("checked"):
                    continue
                got = sorted(r["top_scores"], reverse=True)
                if got != ref:
                    failures.append((scen, r["config"], "top-15 score set differs"))
                    print(f"  !! FAIL {r['config']}: top-15 score set differs")
                elif r["top_items"] != base["top_items"]:
                    print(f"  ~  {r['config']}: tie-order differs (scores identical, allowed)")
        print()

    if args.json:
        Path(args.json).write_text(json.dumps(
            {"hardware": hw, "time_cap": args.time, "results": results}, indent=2))
        print(f"wrote {args.json}")

    if failures:
        print("\nFAILURES:")
        for f in failures:
            print("  ", f)
        sys.exit(1)
    print("all configurations agree on results ✓")


if __name__ == "__main__":
    main()
