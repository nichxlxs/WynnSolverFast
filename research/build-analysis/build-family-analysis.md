# End-game build-family statistics, identification rolls, and solver benchmarks

Research and analysis snapshot: 2026-08-13
Forum corpus: [The Ultimate Build Guide](https://forums.wynncraft.com/threads/the-ultimate-build-guide.320092/), first post last edited 2026-08-11
Game research context: Wynncraft 2.2.3
Local calculation authority: WynnSolverFast data `2.2.3.0`

## Executive findings

1. Normal WynnBuilder links are maximum-roll templates. They identify database
   items, powders, skill points, Aspects, and the ability tree, but not the
   physical internal rolls of those items. Current WynnBuilder sums `maxRolls`
   when it calculates a normal build.
2. The formula `base * 0.3` to `base * 1.3` is only a shorthand for ordinary
   positive rollable IDs before rounding. Ordinary negative IDs use internal
   rolls from 70 to 130, not 30 to 130. Spell costs, fixed IDs, skill points,
   crafted items, and Ascended items require separate handling.
3. Internal rolls are best modeled as discrete integer cases. Positive IDs use
   101 values from 30 through 130. Ordinary negative IDs use 61 values from 70
   through 130. Displayed integers are not uniformly distributed because
   multiple internal rolls can round to the same value.
4. The collected build corpus is not normally distributed. It is a selected,
   versioned, multimodal catalogue with strong class and family effects. Use
   empirical quantiles, medians, MAD, and stratification rather than z-scores.
5. Maximum-roll damage materially overstates accessible damage. Across the
   evaluated explicit families, median melee damage rose about 13% to 23% when
   every ID moved from the median profile to the upper-quartile proxy, then a
   further 10% to 18% from that proxy to maximum. This sensitivity is strongest
   in hybrid, heavy-melee, cancelstack, and tierstack examples.
6. A new 18-case benchmark suite covers 6.85M to 195.83B post-dominance
   combinations and reaches 1.024T unpruned input combinations. Every case uses
   median rolls, locks a real forum build core, enforces sustain or
   survivability, and caps runtime at 30 seconds.

## Corpus and calculation coverage

The source database contains 129 unique forum-linked builds across all five
classes. The local headless evaluator successfully calculated 55 distinct
builds under all four roll profiles, producing 220 whole-build records. The
remaining 74 builds were retained as failures:

| Exclusion | Distinct builds | Reason |
|---|---:|---|
| Crafted URL requires browser craft decoder | 48 | `decodeCraft` is outside the headless current-data harness; a crafted hash is also a recipe template, not one physical craft roll |
| Current local skill-point legality failed | 25 | The forum link and local item snapshot disagree or the saved build depends on state not present in the headless baseline |
| Truncated or incompatible current URL decode | 1 | The newer beta hash exhausted the local decoder bit vector |

This is an important boundary. The 55-build table is a reproducible
WynnSolverFast calibration set, not full current-game measurement. It should
grow after the headless harness gains crafted-item and remaining builder-codec
support.

For each successful record, the analysis stores:

- total HP, EHP with and without Agility, HPR, EHPR, class multiplier, and all
  elemental defenses;
- life steal, effective life steal, life per hit, mana regen including the base
  25, mana steal, mana per hit, and total mana;
- final attack-speed tier, attacks per second, poison, walk speed, and melee
  range;
- main-attack DPS proxy, four individual spell display damages and costs, and
  maximum and summed spell display damage;
- raw and percentage melee, spell, and generic damage IDs;
- assigned and total skill points, equipment, class, archetype, family, source
  URL, ability-node count, Aspect count, and roll profile.

## Why a normal distribution is the wrong starting point

The sample violates the conditions that would make a Gaussian model useful:

- forum authors selected builds they considered functional;
- classes have different defense multipliers and spell coefficients;
- families optimize different objectives;
- Major IDs and ability nodes create discrete modes;
- item and attack-speed breakpoints create discontinuities;
- EHP, mana feasibility, and damage are nonlinear;
- explicit family strata currently contain only one to eight evaluated builds.

The recommended analysis is an empirical distribution per
`family + class + roll profile`. Report p10, p25, median, p75, p90, and MAD.
Use the pooled family only when its class-specific strata are too small, and
always retain the class labels. A future sample with at least 30 comparable
builds per stratum can add bootstrap confidence intervals and distribution
diagnostics. It still should not assume normality by default.

The roll process itself is discrete uniform over internal integer percentages,
not normal. Aggregate build totals may become smoother as many IDs are added,
but skill-point legality and breakpoints prevent a simple central-limit
argument from being sufficient.

## Median-roll family observations

The table below reports empirical p10 guardrails and medians under the uniform
median profile. `N` is the number of evaluated builds, not the number of items
or player-owned copies.

| Family | N | EHP p10 | EHP median | EHP no-Agi p10 | HPR p10 | LS p10 | Total MR p10 | MS p10 | Family damage median |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Heavy melee | 2 | 25,841 | 40,188 | 25,841 | 0 | 402 | 11 | 10 | 23,658 melee |
| Cancelstack | 3 | 17,031 | 19,679 | 11,364 | -167 | 14 | 25 | -6 | 15,110 melee |
| Tierstack | 1 | 22,716 | 22,716 | 14,196 | 0 | -90 | 25 | 13 | 12,701 melee |
| Sustained spell | 8 | 8,629 | 20,734 | 7,978 | -88 | -324 | -48 | -12 | 35,084 max spell |
| Spellsteal label | 2 | 15,258 | 23,869 | 15,180 | 114 | -225 | -3 | 5 | 17,836 max spell |
| Hybrid | 2 | 14,114 | 22,003 | 10,065 | -188 | 348 | 17 | 16 | 122,162 max spell |

These are descriptions, not universal standards. In particular:

- Heavy melee is clearly sustain-sensitive in this tiny sample, but two builds
  cannot establish a population threshold.
- Cancelstack supports the existing Gaia-style floor near 20k EHP, nonnegative
  life steal, controlled negative HPR, and a melee-range floor. Its current p10
  values are close to 17k EHP, 14 LS, and -167 HPR.
- Fate tierstack is valid with negative life steal. A blanket `LS >= 0` would
  incorrectly reject that family example. Attack-speed completion and EHP
  without Agility matter more.
- Sustained spell builds can show negative raw MR or MS because ability and
  cycle mechanics supply sustain. The optimizer should validate the actual
  repeated combo, not require a universal raw mana-ID floor.
- The current `family_spellsteal` label contains Oblivion and Idol Surf, which
  use different sustain patterns. It must be split before its pooled quantiles
  become meaningful.
- Hybrid builds need two performance axes. A single maximum-spell score can
  select a nominal hybrid that contributes little melee damage.

### Pooled class comparison

The generated data also stratifies every metric by class and by
`family + class`. The pooled class medians below are useful diagnostics, but
they are confounded by the different family mix collected for each class.

| Class | N | EHP p10 | EHP median | EHP no-Agi median | HPR median | LS median | Total MR median | MS median |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Archer | 11 | 12,142 | 20,693 | 14,427 | 0 | 168 | 32 | 16 |
| Assassin | 11 | 13,105 | 23,061 | 17,135 | 0 | 0 | 35 | 12 |
| Mage | 16 | 16,527 | 26,792 | 17,317 | -271 | 217 | 27 | 13 |
| Shaman | 5 | 13,896 | 17,333 | 14,196 | 0 | 221 | 27 | 12 |
| Warrior | 12 | 11,303 | 26,067 | 14,048 | 0 | 0 | 16 | 1 |

For example, the pooled Mage HPR median is strongly negative because the
catalogue includes Warp and other self-penalty builds. It is not evidence that
negative HPR is a general Mage target. Thresholds should be estimated within a
family and class whenever enough records exist.

## Proposed optimizer conditions

The benchmark conditions are deliberately more permissive than the observed
medians. They stop obviously nonfunctional glass cannons while leaving enough
space for the optimizer to discover alternatives.

| Family | Default feasibility conditions | Objective and validation |
|---|---|---|
| Gaia cancelstack | `EHP >= 20k`, `LS >= 0`, `HPR >= -100`, `melee range >= -20%` | Maximize the exact melee cycle; expose each threshold as configuration |
| General cancelstack | `EHP >= 18k`, `LS >= 0`, `HPR >= -200`, `melee range >= -20%` | Confirm the intended attack-speed cancellation and any weapon-specific breakpoint |
| Heavy melee | `EHP >= 18k`, `LS >= 0`, `HPR >= -100` | Maximize time-normalized main attack; validate hit access and range separately |
| Tierstack | `EHP without Agility >= 12k`, `attack tier >= 3`, `HPR >= -100` | Require the intended final attack speed; do not impose universal nonnegative LS |
| Spellsteal | `EHP >= 10k`, `MS >= 20`, `HPR >= -250` | Repeated-cycle mana simulation with melee timing and downtime enabled |
| Sustained spell | `EHP >= 8k`, `MS >= 20`, `HPR >= -300` for the Divzer benchmark | Use combo feasibility as authority; raise EHP to 12k or more for general play |
| Hybrid | `EHP >= 10k`, `MS >= 10`, `HPR >= -250` | Score a declared mixed melee-spell cycle, not the larger component alone |

These are starting benchmark gates. The mini database marks them as
configurable and should eventually store content-specific profiles for raids,
lootruns, world events, and solo bossing.

## Exact identification-roll model

For an ordinary positive rollable base `b > 0`:

```text
r is an integer in {30, 31, ..., 130}
display = max(1, round_half_up(b * r / 100))
```

For an ordinary negative base `b < 0`:

```text
r is an integer in {70, 71, ..., 130}
display = min(-1, round_half_down(b * r / 100))
```

Spell-cost inversion, fixed IDs, skill points, and other special stat types
must carry explicit metadata. Do not infer quality from numeric sign alone.
Current Wynntils represents an exact internal roll in one byte and its
probability code counts the inclusive integer cases. See the current
[Wynntils StatType](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/type/StatType.java),
[StatCalculator](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/StatCalculator.java),
and [identification transformer](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/items/encoding/impl/block/IdentificationDataTransformer.java).

The displayed endpoints are rounded and clamped. For positive base 10, the
maximum displayed value 13 occurs for rolls 125 through 130, so it has
probability `6/101`, not `1/101`. For positive base 5, displayed 7 only occurs
at 130, so it has probability `1/101`. The exact calculator in
`id_roll_probability.js` aggregates this probability mass.

### What perfect-roll assumptions cost

For independent positive IDs without an Amplifier:

| Requirement per ID | Five relevant IDs |
|---|---:|
| At least one star, internal 101 to 130 | 0.2312%, about 1 in 433 |
| At least two stars, internal 125 to 130 | 0.00007399%, about 1 in 1,351,608 |
| Exactly three stars, internal 130 | about 1 in 10.51 billion |
| At least the solver's 75% interval point, internal 105 to 130 | about 1 in 885 |

At eight positive IDs, requiring at least the 75% point on every ID is already
about 1 in 51,854. This is why the new optimizer benchmarks use the median
profile. The upper-quartile and maximum profiles are sensitivity tests only.

Rerolling does not make the next ordinary result remember the previous one.
For per-attempt success probability `p`, the probability of at least one
success in `k` independent attempts is `1 - (1 - p)^k`. Insulators change the
joint state by locking one ID. The exact Corkian Amplifier transformation is
not publicly specified, so it should not be hard-coded from WynnBuilder's
continuous approximation.

## Roll sensitivity in the collected builds

Median family damage changes when every ID moves together:

| Family | Median to upper-quartile proxy | Upper-quartile proxy to maximum |
|---|---:|---:|
| Hybrid melee | +23.1% | +18.4% |
| Heavy-melee melee | +18.0% | +15.2% |
| Tierstack melee | +16.4% | +13.1% |
| Cancelstack melee | +14.9% | +12.4% |
| Sustained-spell max spell | +9.9% | +8.9% |
| Spellsteal max spell | +13.7% | +12.8% |

Because all IDs move together in this profile analysis, these are sensitivity
upper bounds rather than acquisition probabilities. The next data layer should
vary each physical ID independently and report which exact item IDs control
legality, sustain, attack-speed, and range breakpoints.

## Benchmark suite

The checked family suite is defined in
`js/solver/benchmarks/family_suite.json` and run by
`js/solver/benchmarks/real_world.js`.

| Family/core | Ideal items, S/M/L | Small | Medium | Large |
|---|---:|---:|---:|---:|
| Trance cancelstack | 5 / 4 / 3 | 76,976,100 | 4,079,733,300 | 195,827,198,400 |
| Vengeance heavy melee | 5 / 4 / 3 | 18,216,000 | 1,256,904,000 | 50,276,160,000 |
| Fate tierstack | 5 / 4 / 3 | 6,854,400 | 171,360,000 | 2,227,680,000 |
| Oblivion spellsteal | 5 / 4 / 3 | 34,424,208 | 1,411,392,528 | 29,639,243,088 |
| Divzer sustained spell | 5 / 4 / 3 | 99,810,090 | 5,788,985,220 | 170,775,063,990 |
| Divzer hybrid | 5 / 4 / 3 | 34,765,200 | 1,599,199,200 | 37,581,181,200 |

The benchmark labels describe post-dominance search space. Each snapshot also
stores its pre-pruning input space and an accepted calibration band. Explicit
runs fail if item-data or pruning drift moves either space outside its band.

Use same-machine medians and p90s for optimizer comparisons. Do not commit a
single laptop timing as a universal baseline. Track:

- combinations checked per second;
- feasible leaves per second;
- best score at 3s, 5s, 10s, and 30s;
- time to exceed the seed score;
- pruning funnel counts;
- exact top build and constraint satisfaction;
- peak memory and worker utilization.

Run the suite:

```bash
BENCH_SUITE=family-all BENCH_SAMPLES=1 BENCH_SECONDS=30 BENCH_WORKERS=2 \
  BENCH_INCLUDE_ISOLATION=0 node js/solver/benchmarks/real_world.js
```

## Next data work

1. Add crafted-item and remaining URL-codec support to the headless harness so
   the remaining source links can be evaluated.
2. Split coarse labels, especially spellsteal versus mana-regeneration spell
   and ability-sustained spell.
3. Collect at least 20 to 30 comparable builds per family and class, including
   rejected or merely average builds to reduce selection bias.
4. Add exact physical item-roll manifests from Wynntils shares. Run discrete
   Monte Carlo or exact convolution over independently rolled IDs.
5. Store content, party assumptions, buffs, tomes, Aspects, consumables,
   rotation, target defense, and movement requirements with each build.
6. Promote a threshold to a default only after out-of-sample validation against
   builds not used to derive it.

## Source notes

The exact source ledger and edge cases are in
`research/item-identification-roll-mechanics-evidence.md`. The strongest sources
are the [official API item example](https://docs.wynncraft.com/modules/item-recipe/list-items#response),
the [Wynncraft staff rounding explanation](https://forums.wynncraft.com/threads/item-id-rounding-changes-could-break-builds-please-read.209530/),
current [Wynntils source](https://github.com/Wynntils/Wynntils), and current
[WynnBuilder source and encoding specification](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/ENCODING.md).
