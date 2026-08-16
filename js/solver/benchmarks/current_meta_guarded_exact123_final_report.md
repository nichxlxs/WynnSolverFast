# Current-meta pruning campaign

Campaign tier: exact. Searches: 270/270.
Execution: 2 concurrent job(s), 4 worker(s) per search.
Batch size: 1 search(es) per loaded harness.
Throughput comparisons are valid only against runs with the same concurrency and worker configuration.

## Exact-tier completion

| Missing | Strategy | Runs | Exhaustive | Removed | Reduction | Checked/s | Exact optima preserved | Exact optima pruned |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | off | 15 | 15 | 0 | 0.0% | 964 | 15 | 0 |
| 1 | certified | 15 | 15 | 67 | 2.5% | 1,043 | 15 | 0 |
| 1 | balanced | 15 | 15 | 675 | 25.5% | 816 | 15 | 0 |
| 1 | conservative | 15 | 15 | 715 | 27.0% | 753 | 15 | 0 |
| 1 | current | 15 | 15 | 749 | 28.3% | 723 | 15 | 0 |
| 1 | aggressive | 15 | 15 | 753 | 28.4% | 775 | 15 | 0 |
| 2 | off | 15 | 15 | 0 | 0.0% | 33,864 | 15 | 0 |
| 2 | certified | 15 | 15 | 28,251 | 5.8% | 33,350 | 15 | 0 |
| 2 | balanced | 15 | 15 | 204,427 | 42.2% | 28,068 | 15 | 0 |
| 2 | conservative | 15 | 15 | 213,886 | 44.2% | 26,982 | 15 | 0 |
| 2 | current | 15 | 15 | 219,613 | 45.3% | 26,258 | 15 | 0 |
| 2 | aggressive | 15 | 15 | 227,453 | 47.0% | 26,002 | 15 | 0 |
| 3 | off | 15 | 4 | 0 | 0.0% | 507,252 | 4 | 0 |
| 3 | certified | 15 | 4 | 8,129,804 | 8.5% | 477,205 | 4 | 0 |
| 3 | balanced | 15 | 6 | 53,564,546 | 56.1% | 342,356 | 4 | 0 |
| 3 | conservative | 15 | 6 | 56,906,126 | 59.6% | 312,375 | 4 | 0 |
| 3 | current | 15 | 6 | 59,680,254 | 62.5% | 276,858 | 4 | 0 |
| 3 | aggressive | 15 | 6 | 61,504,969 | 64.4% | 271,749 | 4 | 0 |

Exhaustive unpruned controls available: 34. Only these scenarios support global-maximum comparisons.

### Censored unpruned controls

These families did not exhaust within the campaign cap, so their pruning safety remains unknown.

| Class | Family | Missing | Checked | Checked/s | Observed score |
|---|---|---:|---:|---:|---:|
| archer | fast_hybrid | 3 | 695,000 | 138,226 | 153,405.21 |
| archer | heavy_melee | 3 | 2,860,000 | 571,771 | 35,390.29 |
| archer | slow_heavy_melee | 3 | 2,130,920 | 423,474 | 37,546.24 |
| assassin | spellsteal | 3 | 1,140,000 | 226,595 | 45,500.95 |
| assassin | spell | 3 | 315,000 | 62,837 | 78,069.44 |
| mage | rawstack_spell | 3 | 120,000 | 23,914 | 35,641.42 |
| mage | support_healing | 3 | 1,149,360 | 229,139 | 9,165.15 |
| mage | cancelstack | 3 | 1,387,231 | 276,451 | 308,554.54 |
| shaman | spell_summoner | 3 | 220,000 | 43,886 | 7,663.17 |
| warrior | blood_pact_spell | 3 | 230,000 | 45,963 | 32,785.11 |
| warrior | tank | 3 | 1,237,470 | 246,705 | 10,069.21 |

