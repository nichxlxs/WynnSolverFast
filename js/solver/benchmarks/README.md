# Solver benchmarks

Benchmarks are separate from correctness tests: timings vary by machine, while
correctness must be deterministic. Run the initial hot-path benchmark with:

```bash
node js/solver/benchmarks/skillpoints.js > benchmark.json
node js/solver/benchmarks/top_results.js > top-results.json
node js/solver/benchmarks/real_world.js > real-world.json

# Family suite: choose small, medium, large, or all. BENCH_SUITE=family is
# an alias for family-medium. Every family run is capped at 30 seconds.
BENCH_SUITE=family-small BENCH_SAMPLES=1 BENCH_SECONDS=30 BENCH_WORKERS=2 \
  BENCH_INCLUDE_ISOLATION=0 node js/solver/benchmarks/real_world.js \
  > family-small.json

BENCH_SUITE=family-medium BENCH_SAMPLES=1 BENCH_SECONDS=30 BENCH_WORKERS=2 \
  BENCH_INCLUDE_ISOLATION=0 node js/solver/benchmarks/real_world.js \
  > family-medium.json

BENCH_SUITE=family-large BENCH_SAMPLES=1 BENCH_SECONDS=30 BENCH_WORKERS=2 \
  BENCH_INCLUDE_ISOLATION=0 node js/solver/benchmarks/real_world.js \
  > family-large.json

# Supplied Gaia tier-stack build: ~0.8M, ~3.2M, and ~8.1M combinations.
BENCH_SUITE=large BENCH_SAMPLES=1 BENCH_SECONDS=30 BENCH_WORKERS=2 \
  BENCH_INCLUDE_ISOLATION=0 node js/solver/benchmarks/real_world.js > gaia-large.json

# Long case: 95.2M input combinations, two-minute timeout, two workers.
BENCH_SCENARIOS=gaia_wide_95m_input BENCH_SAMPLES=1 BENCH_SECONDS=120 \
  BENCH_WORKERS=2 BENCH_INCLUDE_ISOLATION=0 \
  node js/solver/benchmarks/real_world.js > gaia-95m.json
```

## Build-family suite

`family_suite.json` is the machine-readable manifest. Its snapshots are seeded
from forum builds migrated and browser-validated on current WynnBuilder 2.2.3.0,
then solver-validated against the matching local data. They use uniform midpoint rolls for every ID.
That roll mode is an acquisition-oriented deterministic proxy, not a joint
50th-percentile statement about an entire physical build.

The calibrated post-dominance search spaces are:

| Family | Ideal items provided, S/M/L | Small | Medium | Large |
|---|---:|---:|---:|---:|
| Trance cancelstack | 5 / 4 / 3 | 76,976,100 | 4,079,733,300 | 195,827,198,400 |
| Vengeance heavy melee | 5 / 4 / 3 | 18,216,000 | 1,256,904,000 | 50,276,160,000 |
| Fate tierstack | 5 / 4 / 3 | 6,854,400 | 171,360,000 | 2,227,680,000 |
| Oblivion spellsteal | 5 / 4 / 3 | 34,424,208 | 1,411,392,528 | 29,639,243,088 |
| Divzer sustained spell | 5 / 4 / 3 | 99,810,090 | 5,788,985,220 | 170,775,063,990 |
| Divzer hybrid | 5 / 4 / 3 | 34,765,200 | 1,599,199,200 | 37,581,181,200 |

The item count includes the locked weapon. Small cases are all greater than one
million combinations. Large cases are all less than 1.88 trillion combinations.
The same 1.88 trillion cap also applies to the unpruned input space. The largest
input case is Trance cancelstack at 1,024,391,180,736 combinations, retaining a
roughly one-trillion starting-space benchmark. The generator rejects definitions
that violate those limits or do not increase strictly from small to medium to large.

The snapshots contain broad calibrated bands for input and search combinations.
An explicit run fails if item-data or pruning drift moves a case outside its
band. Each snapshot has `time_limit_seconds: 30`; `BENCH_SUITE=family` also
aliases the medium suite. All family suite entry points and direct snapshot runs
clamp an environment override to the snapshot's 30-second cap.

Each of the 18 broad snapshots embeds the score and item tuple of its validated seed as a
fallback result. A time-capped search therefore cannot regress below the known
good build merely because it did not reach that region of a large search space.
Each family also has one `_known_good` companion snapshot with all eight equipment
slots locked. Those six one-combination tests independently prove that the seed
meets the current family restrictions. Live browser evidence is stored in
`research/build-database/family-seed-browser-validation.json`.

Regenerate the URL-encoded snapshots after deliberately changing their cores,
combos, roll profile, or restrictions:

```bash
node js/solver/benchmarks/generate_family_snapshots.js
```

Run a three-second structural smoke check without collecting a timing baseline:

```bash
SOLVER_BENCH_SECONDS=3 SOLVER_BENCH_WORKERS=2 \
  node js/solver/tests/test_solver_search.js solver_family_
```

For the approximately one-minute original-solver completion case:

```bash
BENCH_SCENARIOS=gaia_armor_ring_2m BENCH_SAMPLES=1 BENCH_SECONDS=90 \
  BENCH_WORKERS=1 BENCH_INCLUDE_ISOLATION=0 \
  node js/solver/benchmarks/real_world.js > gaia-2m.json
```

Set `SOLVER_BENCH_TRACE=1` when invoking `test_solver_search.js` directly to
include per-phase worker timing. Generated JSON and CPU profiles are temporary
machine artifacts; do not commit them as durable results. Record only dated
optimization deltas in `../PROJECT_TRACKER.md`.

`skillpoints.js` measures the cascade DP with zero, four, and eight ordering
items, both with normal allocation and with the worker's reusable scratch
storage. It warms up V8, takes repeated samples, and emits machine-readable JSON
with median, p90, dispersion, throughput, runtime, hardware, and commit metadata.

`top_results.js` compares the worker's previous allocate-sort-truncate result
path with the cutoff-aware implementation. It first asserts identical top-N
output, then reports runtime and candidate allocation counts for both versions.

`real_world.js` runs actual item-data build optimizations from the checked-in
README-derived snapshots. It compares a faithful benchmark-only recreation of
the pre-project result tracking and dominance behavior with the current engine.
The suite contains two-ring, two-armor, and four-armor searches; reports raw
runs plus medians and p90s for throughput, elapsed time, feasibility, and best
score; and alternates variant order to reduce systematic thermal bias.

```bash
# Quick comparison; the four-armor case is intentionally time-bounded.
BENCH_SAMPLES=1 BENCH_SECONDS=6 node js/solver/benchmarks/real_world.js

# Controlled baseline (defaults: 3 samples, 10 seconds, 1 worker).
node js/solver/benchmarks/real_world.js > real-world.json
```

For a quick smoke run:

```bash
BENCH_SAMPLES=2 BENCH_ITERATIONS=100 node js/solver/benchmarks/skillpoints.js
```

Do not commit a timing baseline produced by an arbitrary laptop or shared CI
runner. Keep JSON artifacts per machine and compare medians on the same machine.
The next benchmark layer should replay checked-in solver snapshots and report
phase timings, search funnel counts, time-to-score thresholds, and fixed-work
throughput.
