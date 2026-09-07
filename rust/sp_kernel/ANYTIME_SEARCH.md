# Native anytime search

`anytime_kernel` is an experimental, bounded large-neighbourhood search. It
loads one scoring context and repeatedly calls the existing Rust search and
scoring pipeline over reduced equipment domains. It publishes evaluated
builds, including their SP/tome allocation, and always reports `complete:false`.
It does not certify a global optimum, a percentage gap, or game accuracy beyond
the current production evaluator.

```sh
cargo build --release --bin anytime_kernel
target/release/anytime_kernel fixtures/enum_fam_tierstack_medium.txt \
  fixtures/score_fam_tierstack_medium.json \
  --seconds 10 --seed 1 --top-k 1 --trace /tmp/alns-trace.jsonl
```

The native elapsed-time origin precedes reading/parsing both fixtures and
loading the compiled scoring context. `setup_secs` includes that preparation;
`elapsed_secs` includes preparation, warm search and all repairs. The external
benchmark harness should additionally measure process wall time. A large
initialization or individual evaluator call cannot be interrupted midway;
search checks its deadline every 256 reporting events and before a repair.

## Search strategy

1. Evaluate a small Cartesian seed selected by the production solo-ceiling
   ranking, retaining actual results. Probe a separate ordered input domain.
2. Keep a score archive and a diverse exploration archive. Archive identity
   includes gear and tome choice, with ring exchanges normalized.
3. Search complete two-slot and selected three-slot replacement domains,
   subject to the per-repair and global budgets. Capped domains use seeded
   varied ordering, retaining the incumbent item as their first option.
4. Mix in coordinated perturbations, set-piece proposals, crossover domains
   from two evaluated builds and randomized restarts. Reward operators that
   improve the best evaluated score, while retaining forced exploration.

No selected candidate is permanently excluded from the source universe by
the heuristic. Individual repairs explore subsets. Existing upstream pool
preprocessing is inherited from the exported fixture.

Every original slot remains present in a repair, with frozen slots reduced
to singleton pools. This preserves the original set, fixed-stat and SP data.
Reduced pools disable index-based ring symmetry, since their offsets no
longer share a common meaning. When both rings were originally free, a separate
leaf guard enforces their original pool order before SP solving. This matters
because tied SP solutions can differ when ring order changes. Originally
locked rings do not receive this symmetry constraint. Exported fixtures with
two differently ordered original ring pools are rejected as unsupported.
Actual gear/tome identities are deduplicated before archive cutoffs are
computed; duplicate copies of a legal ring remain valid choices.

## Budgets and reproducibility

| Option | Default | Meaning |
|---|---:|---|
| `--seconds` | 10 | Overall wall deadline, including preparation |
| `--seed` | 1 | Specified SplitMix64 random sequence |
| `--work-budget` | Unlimited | Actual `evaluate_leaf` calls across repairs |
| `--repair-budget` | 100,000 | Credited combinations per individual repair |
| `--warm-budget` | Inherit repair budget | Credited combinations for the ranked seed only |
| `--cycle-stagnation` | 0 | Set to 1 to resume local moves after a forced diversification |
| `--max-repairs` | 1,000 | Upper bound on total seed/repair searches |
| `--top-k` | 1 | Number of distinct output gear/tome builds; range 1–64 |
| `--warm-k` | 3 | Candidate count per slot for initial ranked seed |
| `--trace` | None | JSONL of observed improvements |

`credited_tuples` includes skipped subtrees; `leaf_calls` counts actual leaf
pipeline calls. They are intentionally distinct. `completed_repairs` means a
particular local domain finished, not that the original universe was exhausted.

For reproducibility, use a generous wall limit plus fixed work and repair
limits. Wall-limited runs are timing-dependent even with a fixed seed.

```sh
target/release/anytime_kernel ENUM.txt SCORE.json \
  --seconds 3600 --seed 19 --work-budget 100000 --max-repairs 100
cargo test --release --lib enumerate::anytime
```

The expanded benchmark's preferred tested profile is explicit so the legacy
defaults remain available for reproducing earlier ablations:

```sh
target/release/anytime_kernel ENUM.txt SCORE.json \
  --seconds 5 --seed 707 --top-k 1 --warm-k 6 \
  --warm-budget 2000000 --repair-budget 100000 \
  --max-repairs 10000 --cycle-stagnation 1 --trace /tmp/anytime.jsonl
```

This profile reached every frozen benchmark target on all 66 fresh-seed runs
across 22 broad queries. Targets are screening best-known milestones, not
certified global optima. See [the results report](../../ANYTIME_OPTIMIZATION_RESULTS.md)
for misses and regressions in the original profiles and the tuning caveat.

The CLI uses the same immediately flushed `QualityTrace` as the exact
enumerator. Entries include `event`, `wall_seconds`, `score`, `kth_score`,
`phase`, and the equipment/SP/tome witness. Recording happens when evaluated
builds enter the search archive, including during seeding and before a repair
returns. An external process can therefore observe or retain candidates even
if it later terminates the search. `start` and `finish` entries bracket the
run; a terminated run may lack `finish`.

The optional in-process callback and `Result.trace` are separate lightweight
progress observations, taken every 256 credited tuples and at repair
completion. Use the CLI trace for benchmark discovery timing.

Compare time-to-quality with the current solver's time to the same score,
not only its completion time. For small cases use a controlled exhaustive
reference; for unsolved broad cases label the comparison best-known quality.
Compare several random seeds and publish score distributions as well as speed.

## Optional scheduler and seed-budget experiments

Both switches preserve the previous behavior when omitted, so they can be
compared against the initial ALNS measurements without changing the baseline.

`--cycle-stagnation 1` resets the stagnation counter after an executed forced
diversification, even if that move did not improve the global best. This lets
the following repair select pair, triple or crossover moves again. Without
this switch, eight non-improving repairs restrict selection to perturbations
and restarts until a new global best appears. With it, diversification and
local improvement recur in cycles. A repeated domain that is skipped does not
consume the reset, and a search with no feasible base continues constructing
seeds. The switch changes search allocation; it adds no quality guarantee.

`--warm-budget N` allows the ranked Cartesian seed to have a different credited
tuple cap from each later repair. For example, six candidates across eight
free slots represent up to 1,679,616 tuples; a 100,000-tuple repair budget would
otherwise stop that seed early. Overall wall and actual-leaf budgets still
apply, so a larger warm budget does not guarantee that the seed finishes.

```sh
target/release/anytime_kernel ENUM.txt SCORE.json --seconds 10 --seed 1 \
  --warm-k 6 --warm-budget 1700000 --repair-budget 100000 \
  --cycle-stagnation 1 --trace /tmp/alns-cyclic.jsonl
```

The final JSON records the effective `warm_budget` and `cycle_stagnation` so
benchmark results identify which configuration ran. The separate
`--max-repairs` guard remains unchanged; use an explicit larger value when
testing whether a run benefits from continuing until its wall/work deadline.
