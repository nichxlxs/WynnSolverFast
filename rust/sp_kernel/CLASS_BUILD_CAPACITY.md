# Class-build search capacity

This is the user-facing interpretation of the exact-family benchmark suite. It
answers a practical question: how many of the Trance, Vengeance, Fate,
Oblivion, and Divzer search tiers can be exhausted in a useful amount of time?

## Benchmark definition

The suite has six build families and three widths per family:

- **small:** four free equipment slots;
- **medium:** five free equipment slots;
- **large:** six free equipment slots.

Every row optimizes `combo_damage` and retains the family-specific survivability
and sustain restrictions. The weapon, tree, combo, rolls, and supplied ideal
pieces are fixed. "Exhaustive" means the engine covered the complete calibrated
Cartesian space under the declared evaluator, not merely that it found a good
incumbent.

The exact-mode recalibration intentionally uses the raw input product. The
former `calibrated_search_combinations` values were measured after legacy
dominance and understated current exact work by 1.58x to 27.61x.

| Family | Small | Medium | Large |
|---|---:|---:|---:|
| Trance cancelstack | 198,563,904 | 15,289,420,608 | 1,024,391,180,736 |
| Vengeance heavy melee | 57,758,022 | 5,371,496,046 | 290,060,786,484 |
| Fate tierstack | 54,521,040 | 2,562,488,880 | 61,499,733,120 |
| Oblivion spellsteal | 54,521,040 | 2,562,488,880 | 61,499,733,120 |
| Divzer sustained spell | 198,563,904 | 13,303,781,568 | 452,328,573,312 |
| Divzer hybrid | 71,280,000 | 3,706,560,000 | 98,223,840,000 |

Moving small to medium multiplies the space by 47x to 93x. Moving medium to
large multiplies it by another 24x to 67x. This is why a 10% micro-optimization
can move a borderline deadline but cannot normally buy one whole free slot.

## Historical pre-rebase two-thread measurement

The numbers below were recorded on the integrated `f84850a8` patch immediately
before it was rebased onto `4e6d257`. They are useful as an early sizing result,
but they are **not final-PR timings**. All six small rows were run to exhaustion
on that tree. The medium and large values were transcribed from short capped
throughput runs; their raw `bench.py --json` source was not retained in this
repository, so they are illustrative projections rather than independently
reproducible artifacts. Incumbent tightening, partition imbalance, the rebase's
later constant-factor changes, and host noise can all change the final rate.

| Family | Small (historical exhaustive) | Medium (historical projection) | Large (historical projection) |
|---|---:|---:|---:|
| Trance cancelstack | 12.3 s | 8.7 min | 1.43 d |
| Vengeance heavy melee | 4.2 s | 6.5 min | 2.66 d |
| Fate tierstack | 2.3 s | 1.8 min | 54.5 min |
| Oblivion spellsteal | 18.0 s | 15.4 min | 6.85 h |
| Divzer sustained spell | 13.8 s | 9.9 min | 8.64 h |
| Divzer hybrid | 7.3 s | 7.4 min | 5.71 h |

Historical projected/exhaustive threshold coverage is:

| Deadline | Rows at or below deadline |
|---|---:|
| 30 seconds | 6 / 18 |
| 1 minute | 6 / 18 |
| 5 minutes | 7 / 18 |
| 15 minutes | 11 / 18 |
| 1 hour | 13 / 18 |
| 1 day | 16 / 18 |
| 3 days | 18 / 18 |

Only the first number—six exhaustive small solves—is a completed all-row door
count, and even that count belongs to the named pre-rebase tree. The others are
legacy capacity projections. Rerun the final commit with retained source JSON
before treating any threshold in this table as current or making product
promises from it.

## What the new pruning opens

On the `f84850a8` measurement tree, against the conservative correctness
baseline, reachable set-SP pruning and the adaptive memo moved the interactive
small tier from **three of six to six of six**. The newly opened
under-30-second rows in that experiment were:

- Trance cancelstack: over 130 seconds to 12.3 seconds;
- Oblivion spellsteal: 40.5 seconds to 18.0 seconds;
- Divzer sustained spell: over 130 seconds to 13.8 seconds.

That is a measured capacity change on the named tree, not just a
leaves/second headline. The direct A/B also preserved the top-score set. It
must be rerun on the final PR commit before being described as the current
capacity change.

If the historical projections hold, the medium tier is the next realistic
product target: five of six projected below 15 minutes and Oblivion projected
at 15.4 minutes. The large-tier hour/day statements are hypotheses until
longer retained runs replace those projections.

## How much more speed buys another door

Using the historical projections only, the indicative speed targets are:

- about **1.1x** to move the projected Oblivion medium row below 15 minutes;
- about **2.7x** to move every projected large row below one day;
- about **5.3x** to move every projected large row below twelve hours;
- about **64x** to move every projected large row below one hour.

These are ratios derived from unverified projections, not current benchmark
results or promises that a uniform optimization exists.
Across eight free pools a speed factor `F` only buys `F^(1/8)` equivalent width
per pool: 1.2x throughput is roughly 2.3% more candidates per pool, while 10x is
about 33%. Large new doors require structural pruning, target-specific bounds,
or decomposition—not only cheaper leaves.

## Next benchmark gates

1. Run all 18 rows for at least five repeats with fixed thread affinity and
   publish medians/spread.
2. Exhaust every medium row; projections near 15 minutes are cheap enough to
   replace with direct measurements.
3. Build finalized-stat admissible bounds for EHP and restrictions. This is the
   best chance to recover the correctness tax without restoring false pruning.
4. Measure safe dominance on each raw family pool before enabling it. Catalog
   reduction does not imply the same reduction in a query pool.
5. Treat large-family completion as a scheduled/offline mode until the one-day
   gate is met with measured, not extrapolated, runs.

Use [`benchmark_family_capacity.py`](benchmark_family_capacity.py) to turn a
retained `bench.py --json` result into a fresh exact-space ETA/threshold table.
Select one configuration and thread count explicitly; the helper reports
medians across repeats and refuses to blend a thread sweep:

```bash
python3 benchmark_family_capacity.py /path/to/families.json \
  --config defaults --threads 2 --json
```
