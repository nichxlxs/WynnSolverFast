# Solver benchmarks

Benchmarks are separate from correctness tests: timings vary by machine, while
correctness must be deterministic. Run the initial hot-path benchmark with:

```bash
node js/solver/benchmarks/skillpoints.js > benchmark.json
node js/solver/benchmarks/top_results.js > top-results.json
node js/solver/benchmarks/real_world.js > real-world.json

# Supplied Gaia tier-stack build: ~0.8M, ~3.2M, and ~8.1M combinations.
BENCH_SUITE=large BENCH_SAMPLES=1 BENCH_SECONDS=30 BENCH_WORKERS=2 \
  BENCH_INCLUDE_ISOLATION=0 node js/solver/benchmarks/real_world.js > gaia-large.json

# Long case: 95.2M input combinations, two-minute timeout, two workers.
BENCH_SCENARIOS=gaia_wide_95m_input BENCH_SAMPLES=1 BENCH_SECONDS=120 \
  BENCH_WORKERS=2 BENCH_INCLUDE_ISOLATION=0 \
  node js/solver/benchmarks/real_world.js > gaia-95m.json
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
