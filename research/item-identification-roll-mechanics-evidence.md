# Wynncraft item identification roll mechanics: evidence note

Research snapshot: 2026-08-13, Australia/Sydney
Game context: Wynncraft 2.2.2, released 2026-07-10
API context: Wynncraft API v3.7.2 documentation, with item enums based on 2026-04-23 metadata
Code snapshots: WynnBuilder commit `245bf0489f2e3166b37d4039e856ae425bb9645f` from 2026-08-10; Wynntils commit `c09575cda32580db986825c396c4a84c0a654107` from 2026-08-09

## Executive verdict

The claim `Max = base ID * 1.3` and `Min = base ID * 0.3` is **false as a universal rule**.

It is a useful shorthand only for the unrounded endpoints of an ordinary positive, rollable identification. The complete current model has at least five qualifications:

1. Ordinary positive variable IDs use internal rolls from 30 through 130 percent. Ordinary negative variable IDs use 70 through 130 percent.
2. The internal roll is an integer percentage, not an arbitrary real multiplier. Current Wynntils code represents it as one byte and calculates probabilities over 101 or 61 equally weighted integer cases.
3. The displayed value is the rounded product of the base value and internal percentage. Nonzero base IDs are clamped away from zero to `+1` or `-1`.
4. Spell-cost IDs are displayed with inverted signs and use reversed rounding and quality semantics.
5. Fixed, pre-identified, static, skill-point, Major ID, crafted, and Ascended cases do not all follow the ordinary variable-ID rule.

The best compact model for an ordinary variable ID is:

```text
internal roll r:
  positive base: r in {30, 31, ..., 130}
  negative base: r in {70, 71, ..., 130}

displayed value V = clamp_away_from_zero(round_for_stat(base * r / 100))
```

The historical staff explanation gives the same 0.3 to 1.3 positive and 0.7 to 1.3 negative ranges and the nonzero clamp. Current Wynntils code supplies the exact integer grid, tie rounding, inversion logic, and probability enumeration. See [Selvut's rounding-change post](https://forums.wynncraft.com/threads/item-id-rounding-changes-could-break-builds-please-read.209530/), [Wynntils `StatType`](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/type/StatType.java#L14-L20), and [Wynntils `StatCalculator`](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/StatCalculator.java#L22-L57).

## Evidence hierarchy and confidence

| Claim | Best evidence | Confidence | Limitation |
|---|---|---:|---|
| Positive range is 30 to 130 percent; negative range is 70 to 130 percent | Wynncraft staff post, current Wynntils, current WynnBuilder | High | Staff post is old, but two current independent clients still implement it. |
| Internal roll is an integer percentage | Wynntils item-share byte encoding, current star bands, staff star explanation | High | The Wynncraft server RNG source is not public. |
| Each internal integer is equally likely | Wynntils probability code and WynnBuilder's uniformity statement | High, but inferred | Client code calculates probability; it does not expose the server's RNG call. |
| Each variable ID is rolled independently | Current official wiki's Corkian Augment description | High | The wiki is official-hosted and community-maintained, not server source. |
| Rounding and zero clamp | Staff post, current Wynntils, current WynnBuilder | High | Spell-cost sign inversion requires special handling. |
| Amplifier exact transformation and rounding | Official wiki endpoints only | Medium to low | No public server implementation was found. Do not use WynnBuilder's shifted-floor approximation as proof. |

No public Wynncraft server code was located. Therefore this note distinguishes direct game/API statements from reverse-engineered client behavior.

## Formal roll model

### Positive ordinary IDs

For a positive base value `b > 0`, current Wynntils defines the internal range as `30..130`, uses `HALF_UP` rounding, and applies a minimum displayed value of `1`. The current WynnBuilder independently expands the template to `idRound(0.3b)` through `idRound(1.3b)`. Sources: [Wynntils `StatType`, lines 70-84](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/type/StatType.java#L70-L84), [Wynntils calculation, lines 127-142](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/StatCalculator.java#L127-L142), and [WynnBuilder `expandItem`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/build_utils.js#L187-L225).

```text
R+ = {30, 31, ..., 130}, |R+| = 101
V+(b, r) = max(1, round_HALF_UP(b * r / 100))
```

Consequences:

- The unrounded endpoints are `0.30b` and `1.30b`.
- The displayed endpoints are integers after rounding and zero clamping.
- A positive base of `10` has displayed range `3..13`, but `13` is produced by internal rolls 125 through 130. Under the integer-uniform model, displayed `13` has probability `6/101 = 5.9406%`, not 5%.
- A positive base of `5` has displayed maximum `7`, produced only by internal roll 130 because `5 * 1.30 = 6.5` rounds to 7. Its probability is `1/101 = 0.9901%`, not 0%.
- If `|b| > 100`, adjacent internal rolls can skip displayed integers. For example, the API's Idol raw spell-damage base is 264: 30 percent displays 79 and 31 percent displays 82. Values 80 and 81 are not reachable from those two rolls. A min/max interval must not be mistaken for a uniform set of every integer in between.

The official API's Idol example directly shows `manaRegen` as `min=3, raw=10, max=13` and `rawSpellDamage` as `min=79, raw=264, max=343`. It also shows fixed Intelligence as the plain scalar `rawIntelligence=26`. See the [official list-items response example](https://docs.wynncraft.com/modules/item-recipe/list-items#response).

### Negative ordinary IDs

For an ordinary negative base `b < 0`, current Wynntils uses internal rolls `70..130`, `HALF_DOWN` rounding, and a maximum value of `-1` so the ID cannot disappear at zero. This matches the staff description that negative IDs use 0.7 to 1.3 and are clamped to at least magnitude one. Sources: [Wynntils `StatType`, lines 79-84](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/type/StatType.java#L79-L84) and [staff rounding explanation](https://forums.wynncraft.com/threads/item-id-rounding-changes-could-break-builds-please-read.209530/).

```text
R- = {70, 71, ..., 130}, |R-| = 61
V-(b, r) = min(-1, round_HALF_DOWN(b * r / 100))
```

Because `b` is negative, larger internal rolls are more negative and usually worse. Thus:

- Best, closest-to-zero endpoint: approximately `0.70b`.
- Worst, most-negative endpoint: approximately `1.30b`.
- The claim that a negative ID has a `0.30b` minimum is wrong.
- A perfect favorable negative roll is normally internal roll 70, with unamplified probability `1/61 = 1.6393%`, before displayed-value collisions.

A current item example is Arcane Parasite's negative Health Regen, shown from `-455` to `-245`; this is the 1.3 and 0.7 treatment of a negative base, after rounding. See the [official wiki item page](https://wynncraft.wiki.gg/wiki/Arcane_Parasite).

### Spell-cost inversion

Spell-cost reductions are beneficial when displayed as negative. WynnBuilder explicitly marks all eight raw and percent spell-cost fields as reversed IDs. Wynntils stores spell-cost stats with display and calculation inversion, and reverses the half-rounding mode so the signed display agrees with the game. Sources: [WynnBuilder reversed ID list and expansion](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/build_utils.js#L184-L217) and [Wynntils `SpellStatType`](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/type/SpellStatType.java#L24-L33).

The official Idol API example is a useful warning about labels. It reports `raw2ndSpellCost` as `min=-15, raw=-50, max=-65`. Here `max` means the best-quality endpoint, although `-65` is numerically less than `-15`. See [official quick-search documentation](https://docs.wynncraft.com/modules/item-recipe/quick-search-items#response).

Do not infer roll quality from numeric sign or from the words `min` and `max` alone. Use the stat's inversion semantics.

### Exact rounding

Current Wynntils uses decimal arithmetic rather than binary floating point:

- Ordinary positive base: nearest integer, half up.
- Ordinary negative base: nearest integer, half down.
- Calculation-inverted stat such as spell cost: swap those half modes before display inversion.
- If the result is zero, replace it with the sign of the base value.

This signed combination agrees with WynnBuilder's `Math.round` plus nonzero clamp for ordinary values. WynnBuilder's implementation is at [`idRound`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/build_utils.js#L290-L300). Wynntils' exact implementation is at [`calculateStatValue`](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/StatCalculator.java#L127-L142).

The API changelog is also important: v3.3.2 fixed ID rounding, fixed spell-cost sign treatment, and changed identification values with base `1` to raw values rather than rollable objects. See [Wynncraft API v3.3.2](https://docs.wynncraft.com/2024-08-29-v3-3-2#item-database-and-search).

## Integer distribution and probability

### What is directly represented

Wynntils' current share encoding writes each non-pre-identified stat as a stat key plus a one-byte internal roll. On decode, it reads that byte and recomputes the displayed value from the item base. Pre-identified stats do not carry an internal roll. See [the encoding comments and implementation](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/items/encoding/impl/block/IdentificationDataTransformer.java#L84-L97) and [the internal-roll byte write](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/items/encoding/impl/block/IdentificationDataTransformer.java#L132-L176).

This proves that item roll state is represented on an integer percentage grid. It does not, by itself, prove how the server selects the byte.

### Uniformity evidence

Wynntils calculates reroll probabilities with inclusive integer counts:

```text
allCases = internalHigh - internalLow + 1
probability = acceptedInternalRollCases / allCases
```

See [`getPerfectChance`, `getIncreaseChance`, and `getDecreaseChance`](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/StatCalculator.java#L180-L226). This treats each integer internal roll as equally likely. WynnBuilder separately states that IDs are uniformly distributed, although its probability calculator models a continuous interval. A recent community explanation also describes 0.01 increments and a 1-in-101 perfect internal roll. These independent sources make integer-uniform rolls high confidence, but it remains reverse-engineered because the server RNG call is private.

For an unamplified stat with internal set `R` and displayed target `v`:

```text
P(displayed value = v)
  = count({r in R : display(base, r) = v}) / |R|
```

For multiple independently rolled IDs with acceptable internal-roll sets `A_i`:

```text
P(all constraints pass) = product_i (|A_i| / |R_i|)
```

This is the correct basis for a solver. Displayed integers are generally not uniformly distributed.

### Stars, near-perfect, and all-good rolls

The game founder's published positive-ID star bands are 101 to 124 percent for one star, 125 to 129 for two stars, and exactly 130 for three stars. Current Wynntils encodes the same bands as `30..100`, `101..124`, `125..129`, and `130`. Sources: [Salted's forum answer](https://forums.wynncraft.com/threads/about-the-little-asterisks.147931/#post-1654183) and [current Wynntils bands](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/type/StatType.java#L14-L20).

Unamplified positive-ID probabilities are therefore:

| Positive internal quality | Accepted rolls | Single-ID probability |
|---|---:|---:|
| No star, 30 to 100 | 71 | `71/101 = 70.2970%` |
| One star only, 101 to 124 | 24 | `24/101 = 23.7624%` |
| Two stars only, 125 to 129 | 5 | `5/101 = 4.9505%` |
| Three stars, exactly 130 | 1 | `1/101 = 0.9901%` |
| At least one star, staff-described "good" or better | 30 | `30/101 = 29.7030%` |
| At least two stars, "near-perfect" or perfect | 6 | `6/101 = 5.9406%` |

Assuming `n` independent positive variable IDs and no Amplifier:

| Event | General probability | Five-ID example |
|---|---:|---:|
| Every ID at least one star | `(30/101)^n` | `0.2312%`, about 1 in 433 |
| Every ID at least two stars | `(6/101)^n` | `0.00007399%`, about 1 in 1,351,608 |
| Every ID exactly three stars | `(1/101)^n` | `9.5147e-9%`, about 1 in 10,510,100,501 |

These examples define "all-good" and "near-perfect" using the official star vocabulary. Other market or build communities may use different thresholds.

Do not equate displayed maximum with a three-star internal roll. Rounding can make several internal rolls produce the maximum displayed integer. Salted explicitly noted that a low-number ID can display its maximum with only two stars. Thus all-displayed-max probability is item-specific:

```text
P(all displayed max) = product_i (
  count(internal rolls that display the favorable maximum for ID i) / |R_i|
)
```

For ordinary negative IDs, favorable quality runs from internal roll 70 toward 130 as worse, and Wynntils does not assign the positive star bands. A midpoint-or-better negative roll is `70..100`, or `31/61 = 50.8197%`; a best internal roll is `1/61 = 1.6393%`. Spell-cost inversion must be handled by the stat type, not by this sign shortcut.

## Rerolls and Corkian Augments

### Ordinary rerolls

Re-identifying samples new roll state. The current official wiki says each ID is rerolled independently and an Amplifier does not guarantee improvement over the old value. The same independence assumption is used by the client probability tools. See [Corkian Augments](https://wynncraft.wiki.gg/wiki/Corkian_Augments).

If a complete desired event has probability `p` on one identification attempt, the chance of seeing it at least once in `k` independent attempts is:

```text
1 - (1 - p)^k
```

Keeping the best result across attempts is a player strategy. The reroll itself has no memory unless an Insulator is used.

The first reroll costs five times the initial identification cost. Subsequent rerolls multiply the original cost by successive powers of five according to current WynnBuilder: `round(initialCost * 5^rerolls)`. The official wiki documents the first 5x step; an older forum guide and WynnBuilder document the geometric continuation. Sources: [Identifier](https://wynncraft.wiki.gg/wiki/Identifier), [WynnBuilder cost implementation](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/item_display.js#L59-L92), and [forum ID-cost guide](https://forums.wynncraft.com/threads/id-cost.194522/).

### Augments

Current augment behavior:

- Corkian Amplifier I through IV boost positive new rolls by up to 5, 10, 15, or 20 percentage points.
- The effect decreases as the unamplified roll rises. The official example says Amplifier III turns a 30 percent roll into 45 percent, but adds nothing to a 130 percent roll.
- Corkian Simulator prevents the reroll count from increasing, preserving the next reroll's price tier.
- Corkian Insulator locks one selected ID while the other IDs reroll.
- At most one Amplifier and one of Simulator or Insulator can be used together.

Sources: [Corkian Amplifier](https://wynncraft.wiki.gg/wiki/Corkian_Amplifier), [Corkian Simulator](https://wynncraft.wiki.gg/wiki/Corkian_Simulator), and [Corkian Augments](https://wynncraft.wiki.gg/wiki/Corkian_Augments).

**Uncertainty:** no public source found gives the Amplifier's exact per-internal-roll transformation, tie rounding, or post-transform probability mass. WynnBuilder currently approximates the Amplifier by shifting the positive lower multiplier from `0.30` to `0.30 + 0.05*tier` while keeping a continuous uniform interval. That does not match the official wording that the boost decreases with roll quality, so it should not be used as authoritative probability code. See [WynnBuilder Amplifier and PDF code](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/item_display.js#L187-L232) and [its continuous interval calculation](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/item_display.js#L343-L380).

## Edge cases

| Case | Current treatment | Evidence and caveat |
|---|---|---|
| Fixed or pre-identified item | Identification values are fixed and carry no internal roll. Such items are not ordinary reroll targets. | Wynntils omits pre-identified stats from normal roll encoding. The live API returns `identified:true` and scalar IDs for [Ornate Shadow Cowl](https://api.wynncraft.com/v3/item/search/Ornate%20Shadow%20Cowl). WynnBuilder maps API `identified` to `fixID` and sets min=max. |
| Fixed stat on an otherwise variable item | Plain scalar API value, not `{min, raw, max}`. | Official Idol example has scalar `rawIntelligence=26`. API v3.3.2 specifically changed base-1 IDs to raw. Current WynnBuilder also supports per-stat `static` values. |
| Skill points | Current standard item skill-point IDs are nonrolled scalars. | Official API example and WynnBuilder's `nonRolledIDs` list. This differs from early Gavel-era behavior discussed in old posts. |
| Major ID | Fixed effect, not randomly rolled; duplicate instances do not stack with themselves. | [Official Identifications wiki, Major IDs section](https://wynncraft.wiki.gg/wiki/Identifications#Major_Identifications). |
| Set item | Set membership does not make all IDs fixed. A Set item can have fixed skill points plus normal variable IDs and can be identified. | The Identifier explicitly accepts Set items. [Infinitesimal](https://wynncraft.wiki.gg/wiki/Infinitesimal) is a current Set dagger with fixed Defense and ranged Mana Steal, damage, and defence IDs. |
| Mythic item | Mythic rarity uses the same ordinary variable-ID mechanics unless a particular stat or item is fixed. Rarity affects identification cost, not the 30/130 or 70/130 range. | Official Idol Mythic API example; WynnBuilder applies one generic `expandItem` path across tiers. |
| Ascended Mythic | Ascension raises weapon damage and positive-ID limits while preserving roll quality. A perfect pre-Ascension roll remains perfect after Ascension. | [Official Mythic Items wiki, Mythic Ascension](https://wynncraft.wiki.gg/wiki/Mythic_Items#Mythic_Ascension). **Uncertainty:** public sources do not specify the exact integer remapping or its rounding for every changed ID. |
| Crafted item | The craft result is randomized during crafting, including identifications, but it is not an ordinary unidentified template rerolled at an Identifier. Recrafting is separate and is not currently an in-game ordinary reroll system. | [Official Crafting wiki](https://wynncraft.wiki.gg/wiki/Crafting) says the craft result varies. The Identifier's accepted-item list omits Crafted, WynnBuilder omits Crafted from ID costs, and its crafted representation uses ingredient/recipe ranges. Community discussion also states crafted items cannot be rerolled. |
| Custom WynnBuilder item | Can be fixed or ranged by author choice. It is a calculator artifact, not evidence of in-game rollability. | [WynnBuilder custom encoding](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/ENCODING.md#section-c---custom-item-encoding). |
| Charms and other special stat classes | Wynntils comments that the standard star ranges apply everywhere except charms. | Not investigated deeply in this task. Do not automatically extend ordinary gear probability tables to charms. |

Mythic rarity itself does not make rolls worse. The perception that Mythics are harder to perfect comes mainly from their price, scarcity, many relevant IDs, and geometric reroll cost, not a different roll interval.

## WynnBuilder URL encoding and exact rolls

Current WynnBuilder build hash V12 stores build version, equipment and powders, assigned skill points, level, tomes, Aspects, and ability tree. For a normal database item it stores only the stable equipment ID plus powder data. It does **not** store that physical item's internal rolls or displayed ID values. When the builder evaluates that normal item, `Build.initBuildStats()` explicitly sums the item's `maxRolls` map. A normal WynnBuilder link is therefore calculated as a favorable-endpoint, maximum-roll template even though no physical rolls are encoded. See [WynnBuilder encoding overview](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/ENCODING.md#overview), [normal equipment encoding](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/ENCODING.md#2---equipment), and [`Build.initBuildStats()`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/build.js#L89-L96).

Implications:

- Two copies of the same normal item with different rolls produce the same normal equipment portion of a WynnBuilder build URL.
- A normal build link is a maximum-roll template, not proof that the linked author owns the exact required rolls. Its displayed totals can materially overstate a realistically acquired copy when several IDs or breakpoints matter.
- Exact-roll sensitivity must be communicated separately or encoded as a fixed custom item.
- WynnBuilder custom-item hashes can store rolled-ID minimum and maximum values. In fixed mode they store one exact value; in ranged mode they store both endpoints.
- A crafted hash stores ingredients, recipe, level, material tiers, and attack speed. It does not store the realized random output of a specific physical craft. See [crafted encoding](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/ENCODING.md#section-b---crafted-encoding).

This differs from Wynntils' own item-share encoding, which can carry the exact internal roll byte for each non-pre-identified stat and optionally the base values. Do not confuse a Wynntils item share with a normal WynnBuilder build hash.

## WynnBuilder probability discrepancy

Current WynnBuilder states that IDs use a uniform distribution but integrates a continuous multiplier interval. Its endpoint disclaimer says some maximum values can have 0 percent probability because only an exact endpoint multiplier produces them. Current Wynntils instead counts integer internal rolls inclusively. These models disagree at boundaries:

| Example | Continuous WynnBuilder model | Integer internal-roll model |
|---|---:|---:|
| Positive base 5, display maximum 7 | 0% | `1/101 = 0.9901%`, internal 130 |
| Positive base 10, display maximum 13 | 5% | `6/101 = 5.9406%`, internal 125 through 130 |

The integer model is better supported by current item encoding, current probability code, and the staff star definition. Therefore a solver should enumerate integer internal rolls, not integrate a continuous range. WynnBuilder remains reliable evidence for endpoint expansion, fixed/static handling, reversed IDs, reroll-cost display, and URL structure, but its `stringPDF` and `stringCDF` functions should not be treated as exact current game probabilities.

## Solver implementation guidance

Use a two-layer representation:

```text
Item template:
  base value
  stat type and inversion flags
  internal-roll domain
  fixed/pre-identified flag

Physical item copy:
  exact internal roll per variable stat, if known
  displayed value derived from base and internal roll
  reroll count
  Ascended state
```

For exact probability and optimization:

1. Enumerate `30..130` for positive variable bases or `70..130` for negative variable bases.
2. Apply stat-specific inversion and exact decimal rounding.
3. Clamp a nonzero base away from displayed zero.
4. Aggregate probability mass by displayed integer. Never assume displayed integers are uniform or even contiguous.
5. Multiply per-ID accepted-case probabilities only when independence applies and no Insulator introduces a locked state.
6. Treat Amplifier probabilities as unresolved until the exact transformation is verified from current game behavior or authoritative code.
7. Treat normal WynnBuilder URLs as template links. Require custom fixed items or an external roll manifest for roll-sensitive optimization.

## Explicit uncertainties and research gaps

1. **Server RNG source unavailable:** no public Wynncraft server code proves the exact random-number generator or independence implementation. Integer uniformity is high-confidence reverse engineering, not first-party source disclosure.
2. **Amplifier transform:** official sources provide endpoints and qualitative behavior but not the full mapping or rounding. Exact Amplifier probability tables remain unresolved.
3. **Ascension remap:** official sources say roll quality is preserved, but do not publish the exact conversion for every positive ID or the handling of displayed-value collisions.
4. **Crafted output distribution:** the official wiki confirms randomized craft output, but the exact joint distribution across level, base stats, durability, duration, attack speed, and IDs was outside this note.
5. **Special item families:** charms, tomes, consumables, and legacy/broken items may use special stat calculation information. This note is about current ordinary equipment unless explicitly stated.
6. **Future drift:** item API metadata and builder code can change within patch 2.2.2. Re-check current commits and API output before baking constants into a long-lived solver.

## Source ledger

| Source | Authority | Date or snapshot | Support used |
|---|---|---|---|
| [Wynncraft 2.2.2 wiki](https://wynncraft.wiki.gg/wiki/Version_2.2.2) | Official-hosted community wiki | Released 2026-07-10 | Current game version context. |
| [Wynncraft API item database docs](https://docs.wynncraft.com/modules/item-recipe/list-items) | First-party API documentation | Item metadata noted as 2026-04-23 | Raw/min/max and scalar fixed-ID shape. |
| [Wynncraft API v3.3.2 changelog](https://docs.wynncraft.com/2024-08-29-v3-3-2) | First-party API documentation | 2024-08-29 | Rounding, base-1 raw IDs, spell-cost treatment. |
| [Selvut rounding post](https://forums.wynncraft.com/threads/item-id-rounding-changes-could-break-builds-please-read.209530/) | Wynncraft staff forum post | 2017-11-27 | Positive/negative multiplier ranges, nearest rounding, nonzero clamp. |
| [Salted star post](https://forums.wynncraft.com/threads/about-the-little-asterisks.147931/#post-1654183) | Wynncraft founder/staff forum post | 2016-08-29 | 101-124, 125-129, and 130 star bands; displayed-max collision warning. |
| [Wynntils `StatType`](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/type/StatType.java) | Current open-source client reverse engineering | Commit 2026-08-09 | Integer domains, rounding modes, star ranges. |
| [Wynntils `StatCalculator`](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/stats/StatCalculator.java) | Current open-source client reverse engineering | Commit 2026-08-09 | Display calculation, inverse roll ranges, integer probability counts. |
| [Wynntils identification transformer](https://github.com/Wynntils/Wynntils/blob/c09575cda32580db986825c396c4a84c0a654107/common/src/main/java/com/wynntils/models/items/encoding/impl/block/IdentificationDataTransformer.java) | Current open-source client reverse engineering | Commit 2026-08-09 | One-byte internal roll; pre-identified exclusion. |
| [WynnBuilder `build_utils.js`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/build_utils.js) | Current community calculator source | Commit 2026-08-10 | Endpoint expansion, reversed IDs, fixed/static handling, zero clamp. |
| [WynnBuilder `item_display.js`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/item_display.js) | Current community calculator source | Commit 2026-08-10 | Cost model, uniformity claim, continuous PDF discrepancy. |
| [WynnBuilder `ENCODING.md`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/ENCODING.md) | Current community calculator specification | Commit 2026-08-10 | Normal, crafted, and custom URL fields. |
| [Corkian Amplifier](https://wynncraft.wiki.gg/wiki/Corkian_Amplifier) | Official-hosted community wiki | Page snapshot oldid 160940, crawled 2026-08 | Four tiers and quality-dependent endpoint behavior. |
| [Mythic Items](https://wynncraft.wiki.gg/wiki/Mythic_Items#Mythic_Ascension) | Official-hosted community wiki | Updated 2026-07 | Ascension preserves roll quality and raises positive-ID limits. |
