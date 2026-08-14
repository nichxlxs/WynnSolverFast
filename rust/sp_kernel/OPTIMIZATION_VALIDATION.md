# Exact-search optimization validation

This note records the correctness and performance validation behind the exact
solver changes in this branch. It deliberately separates game-model fixes from
speed work: a faster search is not useful if a precheck can discard the optimum.

## What changed

1. Count a fixed, non-crafted weapon toward set membership everywhere the leaf
   evaluator, skill-point solver, seed scorer, or Rust kernel computes set state.
2. Clamp maximum mana to the live-game cap of 400 in the JS and Rust simulators.
3. Disable two prechecks that were not upper bounds:
   - raw `>=` checks that omitted positive set/tree contributions;
   - EHP checks that fixed Defence and Agility at 100 even though effective
     skill points may reach 150.
4. Replace the global "positive set SP exists" bypass with a reachable-set SP
   bound. The bound includes only bonuses whose thresholds can still be reached
   from the current depth, and takes optimistic component-wise maxima.
5. Make the objective-ceiling memo self-disabling when its measured hit rate is
   too low to pay for hash-map traffic.
6. Make gear-dominance policy explicit. Exact mode does not apply unproved
   dominance; safe mode compares only complete, identical interaction
   signatures; legacy mode remains available for measurement.
7. Add a raw, pre-dominance Cartesian oracle and balanced trace counters so
   preprocessing and pruning can be checked independently.

The already-landed priority-sort cache on `master` is not duplicated here.

## Correctness evidence

The focused regressions cover:

- Bony Bow + Bony Circlet activating the two-piece Bony bonus;
- crafted weapons not contributing ordinary set membership;
- item/set SP rescuing a tuple which the former cheap bound rejected;
- an EHP candidate above the old 100/100 relaxation;
- the 400 maximum-mana cap in both simulator implementations;
- a real HP dominance counterexample (Water Mask versus Last Stand);
- set, Major-ID, powder, crafted/custom, restriction, exclusive-set, and
  unknown-effect signature separation;
- empty production/oracle result equality without weakening expected-score
  assertions.

Recorded validation from the integrated development run:

| Gate | Result |
|---|---:|
| JS suite | 637 passed, 0 failed, 2 pre-existing warnings |
| solver-search suite (including raw oracles) | 232 passed, 0 failed |
| Rust `cargo test --release` | unit and CLI integration gates passed |
| Family score-fixture differential | exporter requests up to 96 sampled feasible cases per fixture |
| Differential layers when supported | SP/greedy/mana/score, compiled rows, dense pipeline |

The numerical JS/Rust gate rows above are run notes, not committed test logs.
Family fixtures are generated into the ignored `fixtures/` directory, and the
requested sample count is not evidence that every exporter actually produced
96 feasible cases. Consequently this document does not claim `18 * 96 = 1,728`
validated cases from configuration alone. The strict reproduction gate below
checks the actual fixture count and each supported differential layer. Observed
top-15 score sets matched in the dated A/B captures below.

## Direct small-family benchmark

Contemporaneous run notes identify these as exhaustive two-thread runs on an
AMD EPYC 9V74 host, recorded on the integrated `f84850a8` patch immediately
before rebasing this PR onto `4e6d257`. The dated TSV does not embed the host,
thread count, commit, mode environment, or repeat count, so those provenance
fields cannot be recovered from the artifact alone. The intervening commits
cache setup priority scores and pack leaf stat additions; reviewers should
rerun the harness and retain its JSON output for final release numbers. "Prior-safe"
means the conservative correctness baseline: positive set SP globally bypasses
the cheap bound and the ceiling memo is forced on. "Current" uses reachable set
SP and the adaptive memo.

| Family | Current | Prior-safe | Speedup | Under 30 s before/after |
|---|---:|---:|---:|---:|
| Trance cancelstack | 12.306 s | >130.082 s (82.2% covered) | >10.57x | no / yes |
| Vengeance heavy melee | 4.215 s | 8.071 s | 1.915x | yes / yes |
| Fate tierstack | 2.259 s | 4.009 s | 1.775x | yes / yes |
| Oblivion spellsteal | 17.965 s | 40.527 s | 2.256x | no / yes |
| Divzer sustained spell | 13.804 s | >130.083 s (88.7% covered) | >9.42x | no / yes |
| Divzer hybrid | 7.303 s | 16.600 s | 2.273x | yes / yes |

The practical result is **six of six small class-family searches below 30
seconds, up from three of six** under the prior-safe implementation. The two
capped prior-safe runs had already found the same best-score hash, but had not
proved exhaustion.

The unusually large sustained-spell result is explainable rather than a memo
interaction. In a fixed two-million-credit trace, the reachable SP bound
rejected 1,433,803 credits before the exact path and made 58,167 exact-path
calls. The bypass made roughly two million exact-path calls. Both found 44,513
feasible candidates and the same top 15; the memo was disabled for that fixture.

Raw rows are included in
[`FAMILY_SMALL_EXHAUSTIVE_AB_2026-08-15.tsv`](FAMILY_SMALL_EXHAUSTIVE_AB_2026-08-15.tsv).

## Fixed-work all-family A/B

For a less expensive whole-suite comparison, the final-tree harness ran three
counterbalanced pairs for each of the 18 class-family scenarios, stopping each
invocation after approximately two million credited leaves on one thread. The
TSV embeds the leaf cap, modes, repeats/order, commit state, host, binary,
manifest, and fixture hashes; the companion JSON retains every parsed run.

- Current was faster in 18/18 scenarios.
- Median speedup was 1.537x; range was 1.108x to 4.978x.
- The observed top-score sets at the cap were identical in 18/18 scenarios.
- Credited-work overshoot differed by at most 1,230 leaves (less than 0.062%).

This is an early fixed-work measurement, not a substitute for an exhaustive
medium/large solve. Raw rows are in
[`FAMILY_CAPACITY_AB_2026-08-15.tsv`](FAMILY_CAPACITY_AB_2026-08-15.tsv).

## Component isolation

The component timings below are development run notes. No corresponding raw
rows, repeat distribution, or host metadata are included in the dated TSVs, so
treat them as directional isolation probes rather than release benchmarks.

| Change | Scenario | Before | After | Result |
|---|---|---:|---:|---:|
| reachable set-SP bound | EHP | 11.204 s | 9.997 s | 1.121x |
| reachable set-SP bound | Gaia | 0.849 s | 0.787 s | 1.079x |
| reachable set-SP bound (current rebase) | tierstack-small | 3.588 s | 3.166 s | 1.133x |
| adaptive memo vs forced-on (current rebase) | tierstack-small | 4.682 s | 3.567 s | 1.313x |
| adaptive memo vs forced-on | EHP | 11.149 s | 9.557 s | 1.167x |

Hashes and relevant funnel counters matched mode-to-mode. The memo result is a
comparison with a forced-on memo, not with an imaginary zero-cost cache.
Forced-off tierstack-small took 3.298 seconds; auto's sampling/controller cost
was about 8% on that low-hit run while avoiding most of the forced-on loss.

## Ideas tested but not enabled

Two research ideas were implemented as isolated prototypes and rejected as
defaults:

- **Per-depth direct-state snapshots:** -2.5% on EHP to +0.5% on tierstack;
  the copy/journal cost erased the avoided rebuild work.
- **Subset/Pareto SP activation DP:** 2.07x slower on generated real-item cases
  and 9.4x slower on an eight-item cascade. It was 23.6x faster on a deliberately
  permutation-hard outlier, so an experimental tail-latency crossover remains
  plausible, but it is not a throughput default.

These negative results are part of the evidence; their prototype code is not
shipped by this PR.

## Reproduction

```bash
# JavaScript correctness and raw-oracle gates
node js/solver/tests/test_solver_search.js oracle_
node js/solver/tests/test_dominance.js
node js/solver/tests/test_trace_metrics.js
node js/solver/tests/test_real_world_benchmark.js

# Generate the ignored Rust fixtures, then build the two required binaries
bash rust/sp_kernel/gen_family_fixtures.sh
cd rust/sp_kernel
cargo build --release --bin enum_kernel --bin score_kernel
cargo test --release

# Fail if the generator produced a short/empty family sample, then validate
# every generated fixture. score_kernel exits nonzero on a layer mismatch.
python3 - <<'PY'
import json
import subprocess
from pathlib import Path

fixtures = sorted(Path("fixtures").glob("score_fam_*.json"))
if len(fixtures) != 18:
    raise SystemExit(f"expected 18 family score fixtures, found {len(fixtures)}")
total_cases = 0
for fixture in fixtures:
    cases = len(json.loads(fixture.read_text())["cases"])
    if cases == 0:
        raise SystemExit(f"{fixture}: no sampled cases; differential would be vacuous")
    total_cases += cases
    subprocess.run(["./target/release/score_kernel", str(fixture)], check=True)
print(f"validated {total_cases} sampled cases across {len(fixtures)} fixtures")
PY

# Time-capped family matrix and exact-space capacity projection
python3 bench.py --scenarios families --threads 2 --time 30 --repeat 1 \
  --json /tmp/families.json
python3 benchmark_family_capacity.py /tmp/families.json \
  --config defaults --threads 2

# Fixed-work A/B. One thread is required because the native kernel does not yet
# expose an exact aggregate credited-work budget across multiple workers.
python3 benchmark_family_ab.py --threads 1 --leaf-budget 2000000 --time 120 \
  --repeat 3 --order-seed 20260815 --json /tmp/family-ab.json \
  > /tmp/family-ab.tsv
```

Generated fixtures are intentionally ignored. `bench.py --json` records host
information, the time cap, and per-run thread/work/result fields. The hardened
A/B helper records the invocation, host, Git state, binary and manifest hashes,
mode flags, caps, repeats/order, and parsed runs in JSON; the fixed-work TSV also
carries summary provenance. The small exhaustive TSV retains its earlier compact
schema and is a historical capture, not an output mode of the fixed-work helper.
Preserve the JSON companion for future release claims. Sub-microsecond phases should be measured in
batches; do not derive end-to-end speedups from rejection counts alone.

## Remaining exactness limits

This branch makes the search safer, but it does **not** yet certify live-game
global optimality for every query. Remaining contracts/blockers include:

- candidate Major IDs must rebuild/partition the spell evaluator;
- extra skill-point allocation is still greedy for arbitrary objectives;
- negative crafted requirements are not fully modeled;
- powder choices, roll semantics, MR tick phase, and some abilities remain
  fixed or approximate mechanics contracts.

Accordingly, "exact" here means exhaustive under the declared evaluator and
fixed mechanics envelope—not a claim that every live Wynncraft mechanic has
already been modeled.
