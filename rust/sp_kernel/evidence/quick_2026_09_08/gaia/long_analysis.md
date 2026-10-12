# Long-query optimization benchmark

Native shared evaluator and hash-identical fixtures. T99 refers to a previously frozen best-known milestone, not 99% of a certified global optimum. Baseline has 1 repetition(s); latency ratios are descriptive.

This 1-query cohort uses baseline caps of 60 seconds. Separate supplemental Gaia all-free workload requested during primary campaign, before new Gaia results. Preserves the original eight-query denominator; no winner-based selection from the new campaign. Medium and broad labels were capacity hypotheses; the table reports actual completion or a measured lower bound. No short-window rate projection is presented as an observed minute or hour runtime.

When an exact baseline is included, its longer trace also supplies its 30-second common-budget endpoint. Each ALNS arm has 2 repeats at 30 seconds, top 15, and identical budgets; the new arm changes only the elite-pool operator. The exact wide-key arm has 1 repeat(s) at 30 seconds.

These native ALNS runs use max_repairs=100000; the browser Quick search profile uses max_repairs=10000 and is validated separately. Both ALNS arms use max_repairs=100000 for the longer 30-second budget. The earlier five-second profile used 10000; the elite A/B differs only by --elite-pool 1. Native results therefore must not be presented as measured browser timings.

| Query | Exact completion | Exact T99 | Current ALNS T99 (all seeds) | Elite ALNS T99 (all seeds) |
|---|---:|---:|---|---|
| gaia_all_free | >60 s | 0.057 s | 0.164 s, 0.154 s | 0.171 s, 0.175 s |

A not-reached T99 is a censored observation, not zero seconds. A numeric speedup requires reaching the identical frozen target; a lower bound uses the baseline's measured no-hit time, never its projected exhaustive completion. Missing/failed runs remain visible.

## Secondary 95% milestone

The primary T99 target remains unchanged. This secondary threshold distinguishes recovering a substantial quality gap from improving the last percentage point near the reference.

| Query | Exact T95 | Current ALNS T95 | Elite ALNS T95 |
|---|---:|---|---|
| gaia_all_free | 0.057 s | 0.164 s, 0.154 s | 0.171 s, 0.175 s |

## Common 30-second quality

| Query | Prior reference | Exact | Wide exact | Current ALNS (range) | Elite ALNS (range) |
|---|---:|---:|---:|---:|---:|
| gaia_all_free | 324112 | 100.00% | 100.00% | 100.00% | 100.00% |

Percentages above 100% are new best-known extensions beyond the frozen reference. They are reported separately and do not change the primary T99 milestone after seeing the results.

## Observed exact-search work by 30 seconds

| Query | Exact credited tuples | Exact leaf calls | Wide credited tuples | Wide leaf calls |
|---|---:|---:|---:|---:|
| gaia_all_free | 2.49026e+08 | 230462 | 2.37915e+08 | 205345 |

These are the last parent-observed main-search snapshots at or before 30 seconds, not inferred completion rates. Enabling the wide-key flag does not establish that an objective supports bounds or that its pools needed wider keys; only the observed work/result differences are evidence.

Observed 6/6 planned runs; missing blocks: 0; failed/error runs: 0.

Exact work snapshots count main DFS only and exclude warm search; heuristic neighborhoods overlap. Credited tuples are not concrete evaluator calls and cannot certify heuristic search coverage. Native and WASM parity, game-model validity, and global skill-point allocation are separate claims.
