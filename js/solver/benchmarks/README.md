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

## Current-meta recovery suite

`current_meta_profiles.json` defines 15 executable family/class profiles across
all five classes. Benchmarks group them by `build_family`; archetype is retained
only as provenance. The broad `spell` family intentionally has both Assassin
and Warrior profiles so class-specific behaviour remains visible. Each profile
preserves the authored WynnBuilder URL and game version, labels whether it is
authored-current or a standardized current-data gap fill, and records its
family, element mix, objective, combo rows, mana policy, restrictions, seed
items, and fixed weapon. The modelling decisions and known proxy boundaries are
documented in
`research/current-build-family-benchmark-design.md`.

The generator creates seven deterministic scenarios per profile. The shallow
tier is intended to produce exact maxima. The wide tier is an anytime campaign
with fixed checkpoints, so it measures search quality and throughput without
pretending a timed-out score is a global optimum.

| Variant | Freed equipment slots | Default cap | Tier | Checkpoints | Recovery enforcement |
|---|---:|---:|---|---|---|
| `known_good` | 0 | 10s | exact anchor | completion | required |
| `remove_1` | 1 | 60s | exact | completion | required |
| `remove_2` | 2 | 180s | exact | completion | required |
| `remove_3` | 3 | 600s | exact candidate | completion | report only |
| `remove_4` | 4 | 300s | anytime | 60s, 180s, 300s | report only |
| `remove_5` | 5 | 300s | anytime | 60s, 180s, 300s | report only |
| `remove_6` | 6 | 300s | anytime | 60s, 180s, 300s | report only |

The random removal mask is stable for a profile, count, and replicate. Recovery
means finding the calibrated seed score or a better score without injecting the
seed as an incumbent. It does not require returning the same item tuple. The
generator also lowers each profile's item-level floor enough to keep every freed
seed item eligible, and rejects missing or mismatched source items before it
writes a snapshot.

Regenerate and validate the shared catalog and all 105 JS snapshots with:

```bash
node js/solver/benchmarks/generate_current_meta_snapshots.js
node js/solver/tests/test_current_meta_benchmarks.js
```

Recalibration is a deliberate operation after changing scoring semantics,
profile objectives, or item data. It runs all 15 exact anchors through the
production JS worker, records their exact scores and resolved combo rows, then
regenerates the suite:

```bash
node js/solver/benchmarks/calibrate_current_meta_targets.js
```

Run the required one-item or two-item gates and a paired pruning comparison with:

```bash
node js/solver/tests/test_solver_search.js remove_1
node js/solver/tests/test_solver_search.js remove_2
node js/solver/benchmarks/run_current_meta_recovery.js \
  --variants=remove_2 --pruning=paired \
  --output=current_meta_recovery_paired_results.json
```

Run the wider report-only sensitivity cases with:

```bash
node js/solver/benchmarks/run_current_meta_recovery.js \
  '--variants=remove_3,remove_4,remove_5,remove_6' --seconds=60 --pruning=current \
  --output=current_meta_recovery_results.json
```

Family experiments treat `build_family` as the primary grouping. Class and
archetype remain provenance metadata. Run all four equipment-pruning strategies
over the 3, 4, 5, and 6-missing-item quick matrix with:

```bash
node js/solver/benchmarks/run_current_meta_recovery.js \
  '--variants=remove_3,remove_4,remove_5,remove_6' --seconds=5 --pruning=matrix \
  --output=current_meta_family_pruning_results.json
node js/solver/benchmarks/report_current_meta_pruning.js
```

The strategy matrix includes the new contract guards and the three legacy
sensitivity-only controls. Set-item preservation and all downstream solver
pruning remain identical:

| Strategy | Sensitivity | Guard | Purpose |
|---|---:|---|---|
| `off` | n/a | none | unpruned equipment-pool control |
| `certified` | 0% | all modelled direct dimensions | exact guarded control |
| `balanced` | 0.5% | discrete and weak-active dimensions | production default |
| `conservative` | 0% | legacy | legacy sensitivity control |
| `current` | 0.5% | legacy | previous production behaviour, known unsafe |
| `aggressive` | 2% | legacy | known-unsafe experimental maximum reduction |

The ratio is not the fraction of items removed. The solver perturbs each
objective-relevant stat around a representative build, measures the score
response, and uses the largest absolute response as a scale. `conservative`
keeps every dimension with any measured response when deciding whether one item
dominates another. `current` ignores dimensions below 0.5% of that scale, and
`aggressive` ignores dimensions below 2%. A larger ratio therefore makes more
item pairs comparable and can remove more items, but increases the chance that
a weak-looking stat interaction matters to the true optimum. `certified`
requires equality on every omitted modelled dimension, attack tier, static HP,
and powder-slot capacity; it also preserves every set item and Major-ID item.
`balanced` treats below-threshold nonzero responses and attack tier as equality
guards while retaining removed candidates in a deferred pool. `off` retains
the full eligible equipment pool and is the only empirical control that can
establish the unpruned maximum when it completes. Set preservation, eligibility
rules, priority ordering, and worker-side bounds are unchanged across modes.

The browser also exposes `fast_verify`. It runs `balanced` first, seeds its best
build into the score bounds, then runs `off` across the full pool. This provides
an early guarded result and eventual full-pool coverage if allowed to finish.

Use `--families=<comma-separated-build-families>` or
`--profiles=<comma-separated-profile-ids>` to select a cohort. Results include
`by_family`, `by_variant`, `variant_strategy_matrix`, and
`family_strategy_matrix` summaries. Quote comma-separated values in PowerShell
so the list is passed to Node unchanged. The report generator writes
`current_meta_family_pruning_metrics.json` and the human-readable
`current_meta_family_pruning_report.md`.

The primary pruning metrics are absolute and relative combinations removed,
checked builds per second, and an exhaustive maximum score. A score is a
maximum only when its search space completed. A timed-out `best_score` is kept
as a censored diagnostic observation and must not be ranked as an optimum.
Comparing maximum scores for pruning safety requires an exhaustive unpruned
control for the same profile and removal mask. A completed pruned-space maximum
below that control proves that the global optimum was removed; a timed-out
pruned result below it remains inconclusive.

### Exact and anytime campaigns

The resumable campaign runner writes after every search and supports bounded
process-level parallelism. Use the shallow suite to establish exact maxima:

```bash
node js/solver/benchmarks/run_current_meta_campaign.js \
  --tier=exact '--variants=remove_1,remove_2' --pruning=matrix \
  --jobs=4 --workers=1 \
  --output=current_meta_exact_campaign_results.json
```

Only a completed `off` search is an exact full-space maximum. A completed
pruned search can then be compared with it to prove optimum preservation or
loss. Incomplete three-item cases remain censored and should move to the
anytime analysis rather than receiving an exact rank.

For four, five, and six missing items, run each search once for five minutes
and capture comparable 60, 180, and 300 second observations:

```bash
node js/solver/benchmarks/run_current_meta_campaign.js \
  --tier=anytime --pruning=matrix --jobs=8 --workers=1 --batch-size=3 \
  --output=current_meta_anytime_5m_campaign_results.json
node js/solver/benchmarks/report_current_meta_campaign.js \
  --input=current_meta_anytime_5m_campaign_results.json --checkpoint=300
```

Quote comma-separated options in PowerShell. Resume an interrupted campaign by
repeating the same command with `--resume=1`. The report ranks each strategy
within the same family and removal mask on three separate axes: combinations
removed, checked builds per second, and score ratio against the best score
observed by any strategy at that checkpoint. It also marks Pareto-efficient
results. There is intentionally no arbitrary weighted overall rank. Raw scores
are never compared between different families, and checkpoint score ratio is
an anytime observation, not a global-optimality claim. Throughput comparisons
require identical `--jobs` and `--workers` settings because concurrent searches
share the host. `--batch-size=3` reuses one loaded harness for a family's 4, 5,
and 6-missing searches under the same strategy, reducing startup cost without
changing any search-level timing or checkpoint.

### Measured campaign baseline

The checked-in baseline used eight concurrent jobs, one worker per search, and
three searches per loaded harness. One and two missing items produced 120/120
exhaustive results. All four strategies preserved every one of the 30 exact
unpruned family maxima. Aggregate equipment-space reduction was 36.8%, 38.1%,
and 39.0% for conservative, current, and aggressive at one missing item, then
56.9%, 57.6%, and 58.7% at two missing items.

The three-item campaign used a 180-second cap. It exhausted 52/60 searches,
including 10/15 unpruned controls. It found four confirmed optimum losses:
all three dominance modes lost the cancelstack optimum, and aggressive lost
the slow-heavy-melee optimum. The other five unpruned controls remain censored,
so they provide no safety claim. See `current_meta_exact3_campaign_report.md`.

The wide campaign captured both 60-second and 180-second checkpoints for all
180 searches. At 180 seconds it recovered the calibrated seed in 169/180 runs;
only 10/180 spaces exhausted. Current pruning removed 80.3%, 89.0%, and 91.4% of
aggregate combinations at four, five, and six missing items. Aggressive raised
those figures only to 80.7%, 89.3%, and 91.7%, while current and aggressive had
lower median checked-per-second rates than the unpruned control. The benefit is
therefore fewer combinations, not faster processing of each checked build.
Family details, absolute removals, observed scores, time-to-best, three separate
ranks, and Pareto flags are in `current_meta_anytime_campaign_report.md`.
The canonical timing-valid source is
`current_meta_anytime_repaired_campaign_results.json`. It replaces six complete
family/removal cohorts whose original eight-job child timers were starved. The
raw saturated artifact and both four-job repair artifacts are retained as
lineage, and the report refuses any result observed more than 10 seconds past
its checkpoint or search cap.

These results do not support one globally safe hard-pruning mode. In
particular, cancelstack needs either an interaction-aware rule or an unpruned
verification pass. Tierstack is a high-value family-specific experiment because
current removed 99.4% at six missing items versus 89.4% for conservative and
preserved its completed three-item optimum. Slow heavy melee is the opposite:
aggressive removed much more at five missing items but already has an exact
three-item counterexample.

`current_meta_optimality_regression_results.json` records the first confirmed
counterexample and its fix. For the 3-missing Shaman support-spell profile, the
exhaustive unpruned maximum uses `Vetiver` and scores 6632.94375. Before the
fix, all three pruning modes removed that optimum and exhausted at 6431.90625.
The corrected healing sensitivity retains `Vetiver`; all pruning modes now
exhaust at the same 6632.94375 maximum while removing about 90% of combinations.

Current-meta runs accept an explicit cap from 1 to 3,600 seconds. Unlike the
legacy build-family suite, they are not clamped to 30 seconds. Limit a longer
paired experiment to selected profiles with `--profiles`:

```bash
node js/solver/benchmarks/run_current_meta_recovery.js \
  --variants=remove_6 --seconds=60 --pruning=paired \
  '--profiles=assassin_trickster_spell,mage_arcanist_meteor,mage_riftwalker_cancelstack,shaman_summoner_aura' \
  --output=current_meta_recovery_long_results.json
```

Recovery results retain elapsed time and approximate time to first observe the
seed score for diagnostics. The pruning report derives combinations removed,
reduction ratio, checked builds per second, completion status, exact
search-space maximum, exhaustive full-space maximum when available, and an
explicit optimum-retention status.

For timing comparisons, `real_world.js` discovers scenarios from the same
manifest. `BENCH_SUITE=current-meta` aliases the required `remove_2` workload.
A variant suffix selects another removal level, and `current-meta-all` selects
all 105 scenarios:

```bash
BENCH_SUITE=current-meta-remove_2 BENCH_SAMPLES=1 BENCH_SECONDS=30 \
  BENCH_WORKERS=2 BENCH_INCLUDE_ISOLATION=0 \
  node js/solver/benchmarks/real_world.js > current-meta-remove-2.json
```

Time-capped recovery is evidence about search ordering only. It is not a
pruning-safety or global-optimality metric. Use it for diagnostics, and use only
completed full-space controls for maximum-score claims.
