# Long-query and browser Quick evidence, 2026-09-08

Start with the [results report](../../../../LONG_QUERY_OPTIMIZATION_RESULTS.md).
This continuation records 66 native timing runs on nine distinct large queries,
six separate Node-hosted WASM timing runs, and browser/kernel validation.
It does not turn the 132-fixture parity suite into 132 long timed experiments.

| Native cohort | Queries | Arms and repetitions | Budgets | Runs | Readable evidence |
|---|---:|---|---|---:|---|
| Primary long | 8 | Exact once; wide exact once; core/elite twice each | Exact 60/180 s; others 30 s | 48 | [Analysis](long/long_analysis.md), [JSON](long/long_analysis.json) |
| Supplemental Gaia | 1 | Same six observations | Exact 60 s; others 30 s | 6 | [Analysis](gaia/long_analysis.md), [JSON](gaia/long_analysis.json) |
| Fresh-seed gap confirmation | 3 existing queries | Core/elite twice each | 15 s | 12 | [Analysis](new_seeds/long_analysis.md), [JSON](new_seeds/long_analysis.json) |

The three frozen setup directories preserve query selection, reference scores,
fixture hashes and exact variant arguments. Primary seeds 707/808 were used
previously; they are not new independent seeds. Supplemental 1217/2423 were
previously unused and selected before the new heuristic outcomes. Those three
queries were selected because of their measured exact-baseline quality gaps,
not because a new heuristic had won them. Gaia is a separate supplement;
neither supplement changes the primary eight-query denominator.

The local timed source was `1479d683e0de30d51f14b8ef0a8102bbcc12c34c`,
published with the identical tree as `d9b1d383c599cf02c7bff9994fa2c1b3eebfc1bb`.
The supplemental plans were published before their runs in
`84f5d57ef8ca8a66f77fc67ca75eecbee36a03c1`. Later analysis/documentation changes
did not change the timed native binaries. Each campaign records their hashes.

## What the measurements mean

T99 is time to 99% of a previously frozen, hash-matched best-known score;
T95 and T99.9 are secondary milestones on the same reference. None is a
certificate of distance from the true optimum. New scores above the frozen
reference remain separate quality extensions. Deadlines/no-hit observations
are censored, never zero-second successes or estimated completion times.

All native arms return the top 15. Both native heuristic arms use warm depth 6,
warm budget 2,000,000, repair budget 100,000, cyclic recovery and
`max_repairs=100000`; only the elite flag differs. Browser Quick uses
`max_repairs=10000`, so the native timings are not browser-default timings.
Processes ran sequentially with no competing builds. Launch, parsing and warm
search count toward the native budget; fixture generation is recorded separately.
Exact main-search counters exclude warm search. Credited tuples are distinct
from concrete leaf evaluations, and overlapping repairs do not measure unique
global coverage.

Only tierstack medium was exhausted (140.35 seconds). The other exact caps
establish lower bounds of 60/180 seconds, not measured hour-long runtimes.
Several huge queries already discover the reference early. Avoiding their
remaining proof wait is different from accelerating discovery of that build.

The fresh-seed check rejects a general elite-speed claim: elite is slower to
T99 in five of six pairs and produces two lower endpoints. Wider exact keys
also produced no better same-budget scores. Those negative results are retained
alongside the robust core-neighborhood improvements.

## Raw preservation and integrity

- [long_raw.tar.gz](long_raw.tar.gz): all 48 primary observations.
- [gaia_raw.tar.gz](gaia_raw.tar.gz): all six Gaia observations.
- [new_seeds_raw.tar.gz](new_seeds_raw.tar.gz): all twelve fresh-seed observations.
- [native_manifest.json](native_manifest.json): compressed and member-manifest
  hashes, binary/variant metadata and packaging checks.
- [historical_archive_verification.json](historical_archive_verification.json):
  proves that the frozen prior campaign member hashes match the tracked archives.
- [Validation index](validation/README.md): Rust/JavaScript checks, all 135 bounded
  native/WASM witness comparisons, independent clock/callback validation and the
  real Chromium job. It distinguishes local browser-environment failures from
  the later passing dedicated Chromium test.

Each native archive contains the complete campaign folders, original stdout and
stderr, native incumbent traces, parent-observed traces, normalized configurations,
full per-run trajectories, combined records and readable analyses. An internal
`archive_manifest.json` hashes every payload. No failed or unhelpful seed is removed.
Archive metadata is normalized for deterministic packaging. The small analyses
remain readable in git without extracting the raw records.

From this directory, inspect the original primary records with:

```sh
tar -xzf long_raw.tar.gz
python3 ../../analyze_long_campaign.py \
  --campaign long/combined/campaign.json --plan-dir long_setup --out long
```

Use the [benchmark guide](../../QUALITY_BENCHMARKING.md#long-query-continuation)
to regenerate fixtures, rebase binary paths, reproduce each frozen cohort and
run the Node-hosted WASM checks. The browser CI trace has its own preservation
and expiry metadata in the validation index; native raw evidence is fully
preserved here.
