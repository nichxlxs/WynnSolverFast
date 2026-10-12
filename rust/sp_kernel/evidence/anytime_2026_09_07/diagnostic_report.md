# Time-to-quality benchmark

Process wall time includes launch, parsing, compilation, and warm search. Incumbent traces are observed by the parent process every 5 ms; delivery and trace overhead are included. Fixture generation is recorded separately.

References are labelled best-known unless independently established for the identical query. Default references are the maximum found by this campaign, so percentages do not certify proximity to the global optimum. Timeouts and no-result runs remain in all denominators.

| Scenario | Variant | T99 successes | T99 median (s) | No result | Completed | T99 speedup vs first arm |
|---|---|---:|---:|---:|---:|---:|
| meta_warrior_battle_monk_uppercut_remove_4 | safe_baseline15 | 3/3 | 8.2441 | 0 | 0 | — |
| meta_warrior_battle_monk_uppercut_remove_4 | alns1 | 3/3 | 0.5344 | 0 | 0 | 15.43× |
| meta_warrior_battle_monk_uppercut_remove_4 | alns_warm6 | 3/3 | 0.5678 | 0 | 0 | 14.52× |
| fam_spellsteal_small | safe_baseline15 | 3/3 | 3.1868 | 0 | 0 | — |
| fam_spellsteal_small | alns1 | 1/3 | censored | 0 | 0 | — |
| fam_spellsteal_small | alns_warm6 | 3/3 | 0.4641 | 0 | 0 | 6.87× |
| ehp | safe_baseline15 | 3/3 | 7.2632 | 0 | 0 | — |
| ehp | alns1 | 3/3 | 1.0934 | 0 | 0 | 6.64× |
| ehp | alns_warm6 | 3/3 | 0.3651 | 0 | 0 | 19.89× |

A censored T99 median means the target was not reached in at least half the runs. Speedup is omitted unless every repeat on both arms reached the same target. Completion time is never substituted for baseline T99.
