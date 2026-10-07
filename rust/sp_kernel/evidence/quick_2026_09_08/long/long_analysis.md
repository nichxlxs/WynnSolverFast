# Long-query optimization benchmark

Native shared evaluator and hash-identical fixtures. T99 refers to a previously frozen best-known milestone, not 99% of a certified global optimum. Baseline has 1 repetition(s); latency ratios are descriptive.

This 8-query cohort uses baseline caps of 60, 180 seconds. Fixed before the new long campaign: three original medium families and five larger spell, melee, sustain and healing cases; all failures and no-result runs retained. Medium and broad labels were capacity hypotheses; the table reports actual completion or a measured lower bound. No short-window rate projection is presented as an observed minute or hour runtime.

When an exact baseline is included, its longer trace also supplies its 30-second common-budget endpoint. Each ALNS arm has 2 repeats at 30 seconds, top 15, and identical budgets; the new arm changes only the elite-pool operator. The exact wide-key arm has 1 repeat(s) at 30 seconds.

These native ALNS runs use max_repairs=100000; the browser Quick search profile uses max_repairs=10000 and is validated separately. Both ALNS arms use max_repairs=100000 for the longer 30-second budget. The earlier five-second profile used 10000; the elite A/B differs only by --elite-pool 1. Native results therefore must not be presented as measured browser timings.

| Query | Exact completion | Exact T99 | Current ALNS T99 (all seeds) | Elite ALNS T99 (all seeds) |
|---|---:|---:|---|---|
| fam_tierstack_medium | 140.350 s | 0.067 s | 0.037 s, 0.043 s | 0.037 s, 0.043 s |
| fam_spellsteal_medium | >180 s | not reached | 5.482 s, 0.916 s | 0.242 s, 0.306 s |
| fam_hybrid_medium | >180 s | 0.129 s | 0.135 s, 0.183 s | 0.047 s, 0.047 s |
| fam_cancelstack_large | >60 s | 0.068 s | 0.060 s, 0.064 s | 0.060 s, 0.064 s |
| fam_heavy_melee_large | >60 s | 0.083 s | 0.047 s, 0.052 s | 0.047 s, 0.047 s |
| meta_mage_arcanist_meteor_remove_6 | >60 s | not reached | 6.297 s, 1.349 s | 2.892 s, 2.369 s |
| meta_mage_light_bender_healing_remove_6 | >60 s | not reached | 8.517 s, 3.143 s | 6.951 s, 6.018 s |
| spell_8free | >60 s | 0.114 s | 0.057 s, 0.069 s | 0.078 s, 0.063 s |

A not-reached T99 is a censored observation, not zero seconds. A numeric speedup requires reaching the identical frozen target; a lower bound uses the baseline's measured no-hit time, never its projected exhaustive completion. Missing/failed runs remain visible.

## Secondary 95% milestone

The primary T99 target remains unchanged. This secondary threshold distinguishes recovering a substantial quality gap from improving the last percentage point near the reference.

| Query | Exact T95 | Current ALNS T95 | Elite ALNS T95 |
|---|---:|---|---|
| fam_tierstack_medium | 0.067 s | 0.037 s, 0.043 s | 0.037 s, 0.043 s |
| fam_spellsteal_medium | 14.893 s | 5.482 s, 0.916 s | 0.221 s, 0.300 s |
| fam_hybrid_medium | 0.067 s | 0.045 s, 0.042 s | 0.036 s, 0.036 s |
| fam_cancelstack_large | 0.068 s | 0.060 s, 0.058 s | 0.060 s, 0.064 s |
| fam_heavy_melee_large | 0.083 s | 0.047 s, 0.052 s | 0.047 s, 0.047 s |
| meta_mage_arcanist_meteor_remove_6 | not reached | 3.219 s, 1.349 s | 0.582 s, 1.324 s |
| meta_mage_light_bender_healing_remove_6 | not reached | 3.686 s, 1.886 s | 4.429 s, 0.976 s |
| spell_8free | 0.093 s | 0.057 s, 0.069 s | 0.078 s, 0.063 s |

## Common 30-second quality

| Query | Prior reference | Exact | Wide exact | Current ALNS (range) | Elite ALNS (range) |
|---|---:|---:|---:|---:|---:|
| fam_tierstack_medium | 215850 | 100.00% | 100.00% | 100.00% | 100.00% |
| fam_spellsteal_medium | 92806.8 | 98.87% | 98.87% | 100.00% | 100.00% |
| fam_hybrid_medium | 262856 | 100.00% | 100.00% | 100.00% | 100.00% |
| fam_cancelstack_large | 510715 | 100.00% | 100.00% | 100.00% | 100.00% |
| fam_heavy_melee_large | 37822.4 | 100.00% | 100.00% | 100.00% | 100.00% |
| meta_mage_arcanist_meteor_remove_6 | 44734.3 | 74.05% | 74.05% | 100.62% | 103.22% |
| meta_mage_light_bender_healing_remove_6 | 22459.5 | 72.66% | 72.66% | 100.00–100.06% | 100.00–103.14% |
| spell_8free | 1.53973e+06 | 100.00% | 100.00% | 100.00% | 100.00% |

Percentages above 100% are new best-known extensions beyond the frozen reference. They are reported separately and do not change the primary T99 milestone after seeing the results.

## Observed exact-search work by 30 seconds

| Query | Exact credited tuples | Exact leaf calls | Wide credited tuples | Wide leaf calls |
|---|---:|---:|---:|---:|
| fam_tierstack_medium | 3.39118e+08 | 1.29512e+06 | 3.31311e+08 | 1.29235e+06 |
| fam_spellsteal_medium | 6.82755e+07 | 1.99242e+06 | 6.84394e+07 | 2.00253e+06 |
| fam_hybrid_medium | 1.17407e+08 | 797641 | 1.11967e+08 | 758732 |
| fam_cancelstack_large | 1.48189e+08 | 1.0259e+07 | 8.76983e+07 | 1.82926e+06 |
| fam_heavy_melee_large | 3.27465e+07 | 3.29025e+06 | 3.21321e+07 | 3.24986e+06 |
| meta_mage_arcanist_meteor_remove_6 | 7.84797e+06 | 4.85125e+06 | 7.95458e+06 | 4.12648e+06 |
| meta_mage_light_bender_healing_remove_6 | 3.40625e+07 | 1.57683e+07 | 3.34645e+07 | 1.56877e+07 |
| spell_8free | 3.15616e+07 | 3.7981e+06 | 3.07669e+07 | 3.79546e+06 |

These are the last parent-observed main-search snapshots at or before 30 seconds, not inferred completion rates. Enabling the wide-key flag does not establish that an objective supports bounds or that its pools needed wider keys; only the observed work/result differences are evidence.

Observed 48/48 planned runs; missing blocks: 0; failed/error runs: 0.

Exact work snapshots count main DFS only and exclude warm search; heuristic neighborhoods overlap. Credited tuples are not concrete evaluator calls and cannot certify heuristic search coverage. Native and WASM parity, game-model validity, and global skill-point allocation are separate claims.
