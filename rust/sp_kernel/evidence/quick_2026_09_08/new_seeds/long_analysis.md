# Long-query optimization benchmark

Native shared evaluator and hash-identical fixtures. T99 refers to a previously frozen best-known milestone, not 99% of a certified global optimum. Baseline has 0 repetition(s); latency ratios are descriptive.

This 3-query cohort uses baseline caps of 60, 180 seconds. Supplemental repeatability check selected from the three observed exact-baseline quality gaps before any new heuristic campaign results. Uses previously unused seeds1217 and2423. Separate from the primary eight-query denominator; not winner-selected from heuristic outcomes. Medium and broad labels were capacity hypotheses; the table reports actual completion or a measured lower bound. No short-window rate projection is presented as an observed minute or hour runtime.

When an exact baseline is included, its longer trace also supplies its 15-second common-budget endpoint. Each ALNS arm has 2 repeats at 15 seconds, top 15, and identical budgets; the new arm changes only the elite-pool operator. The exact wide-key arm has 0 repeat(s) at 30 seconds.

These native ALNS runs use max_repairs=100000; the browser Quick search profile uses max_repairs=10000 and is validated separately. Same max_repairs100000 top15 profile as the primary campaign; shorter15-second cap is identical for both arms. Native results therefore must not be presented as measured browser timings.

| Query | Exact completion | Exact T99 | Current ALNS T99 (all seeds) | Elite ALNS T99 (all seeds) |
|---|---:|---:|---|---|
| fam_spellsteal_medium | unavailable | unavailable | 0.612 s, 0.468 s | 0.702 s, 0.907 s |
| meta_mage_arcanist_meteor_remove_6 | unavailable | unavailable | 1.236 s, 1.578 s | 1.403 s, 4.055 s |
| meta_mage_light_bender_healing_remove_6 | unavailable | unavailable | 5.289 s, 2.998 s | 1.738 s, 8.542 s |

A not-reached T99 is a censored observation, not zero seconds. A numeric speedup requires reaching the identical frozen target; a lower bound uses the baseline's measured no-hit time, never its projected exhaustive completion. Missing/failed runs remain visible.

## Secondary 95% milestone

The primary T99 target remains unchanged. This secondary threshold distinguishes recovering a substantial quality gap from improving the last percentage point near the reference.

| Query | Exact T95 | Current ALNS T95 | Elite ALNS T95 |
|---|---:|---|---|
| fam_spellsteal_medium | unavailable | 0.612 s, 0.344 s | 0.644 s, 0.907 s |
| meta_mage_arcanist_meteor_remove_6 | unavailable | 0.867 s, 0.500 s | 1.206 s, 1.470 s |
| meta_mage_light_bender_healing_remove_6 | unavailable | 3.038 s, 2.998 s | 1.717 s, 7.530 s |

## Common 15-second quality

| Query | Prior reference | Exact | Wide exact | Current ALNS (range) | Elite ALNS (range) |
|---|---:|---:|---:|---:|---:|
| fam_spellsteal_medium | 92806.8 | no result / unavailable | no result / unavailable | 100.00% | 100.00% |
| meta_mage_arcanist_meteor_remove_6 | 44734.3 | no result / unavailable | no result / unavailable | 100.62% | 99.78–103.22% |
| meta_mage_light_bender_healing_remove_6 | 22459.5 | no result / unavailable | no result / unavailable | 100.00–103.14% | 99.48–100.00% |

Percentages above 100% are new best-known extensions beyond the frozen reference. They are reported separately and do not change the primary T99 milestone after seeing the results.

## Observed exact-search work by 15 seconds

| Query | Exact credited tuples | Exact leaf calls | Wide credited tuples | Wide leaf calls |
|---|---:|---:|---:|---:|
| fam_spellsteal_medium | unavailable | unavailable | unavailable | unavailable |
| meta_mage_arcanist_meteor_remove_6 | unavailable | unavailable | unavailable | unavailable |
| meta_mage_light_bender_healing_remove_6 | unavailable | unavailable | unavailable | unavailable |

These are the last parent-observed main-search snapshots at or before 15 seconds, not inferred completion rates. Enabling the wide-key flag does not establish that an objective supports bounds or that its pools needed wider keys; only the observed work/result differences are evidence.

Observed 12/12 planned runs; missing blocks: 0; failed/error runs: 0.

Exact work snapshots count main DFS only and exclude warm search; heuristic neighborhoods overlap. Credited tuples are not concrete evaluator calls and cannot certify heuristic search coverage. Native and WASM parity, game-model validity, and global skill-point allocation are separate claims.
