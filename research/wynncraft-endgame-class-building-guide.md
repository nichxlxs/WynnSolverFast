# Wynncraft End-Game Class Building Guide

## A practical workflow for version 2.2.2

This guide turns current Wynncraft mechanics into a repeatable build process. It is written for level 120 end-game play and uses live API data as the authority for current items, ability trees, and Aspects. Its current community examples come from all 129 unique direct WynnBuilder links in the Ultimate Build Guide, with every one of the 55 unique Mythic weapons represented.

The core rule is:

> Build for an activity and a combat loop, not for the largest isolated number.

## 1. Start with a build contract

Write the contract before selecting items.

```text
Class:
Target content:
Solo or party:
Primary damage/support engine:
Expected fight length:
Required range or AoE:
Minimum HP and EHP without Agility:
Health sustain requirement:
Mana or class-resource requirement:
Movement requirement:
Budget and ownership limits:
Mythic, ascended, crafted, Tome, and Aspect policy:
Actual item rolls or conservative roll policy:
```

Good examples:

- "Solo Nameless Anomaly, sustained single-target spell cycle, no consumables, 45-second window, minimum non-dodge EHP, owned item rolls."
- "Four-player raid Paladin support, keep Judgement and party resistance active, survive gambits, personal DPS secondary."
- "Super Fast cancelstack Assassin, minimum 3.1 to 4.3 tier breakpoint certificate, no crafted items, owned attack-tier and skill-point rolls."
- "Long Fruma lootrun, reliable AoE and movement, self-sustain, conservative boon assumptions, peak dummy DPS secondary."

Bad contracts are vague: "best build", "maximum DPS", or "tankiest". They do not define what success means.

## 2. Understand the nine equipment slots

A standard combat loadout contains:

- One class weapon: bow, spear, wand, dagger, or relik.
- Four armour pieces: helmet, chestplate, leggings, boots.
- Four accessories: two rings, one bracelet, one necklace.

Then add:

- Skill points.
- Weapon and armour powders.
- Fourteen Mastery Tome slots where applicable.
- Ability-tree path.
- Five Aspects, with at most one Mythic.
- Major IDs, set bonuses, ascensions, and crafted durability.
- Rotation, state, and target assumptions.

An item is valuable only through its effect on the complete build. A high-damage chestplate that breaks the attack-speed target, skill-point requirements, or survival contract is not an upgrade.

### 2.1 Separate element, requirement, and engine labels

Read a build label from left to right:

```text
equipment requirement code + weapon + engine or archetype
ETA                        + Gaia   + melee
```

The requirement code uses `ETWFA` order: Earth, Thunder, Water, Fire, Air. It records which elemental skill requirements occur across the equipment. It does not describe the weapon's damage elements. Gaia is an Earth wand even when its supporting equipment produces an ETA requirement code.

Keep these fields separate in every build record:

- Weapon base-damage elements.
- Maximum requirement and item count for each skill.
- Requirement-presence code such as ETA or ETWFA.
- Combat engine such as heavy melee, tierstack, spellsteal, poison, or support.
- Archetype and exact ability path.

### 2.2 Use elemental tendencies as search priors

Current end-game items show useful correlations:

- Earth tends toward slower, higher packets, Strength, raw Main Attack, and poison.
- Thunder tends toward burst, attack speed, crit support, Mana Steal, and Life Steal.
- Water tends toward Mana Regen, spell costs, spell damage, and healing.
- Fire tends toward health, HPR, Defence, and stable survival.
- Air tends toward attack speed, Walk Speed, Agility, range, and mobility.

These are search priors, not rules. Multi-element items deliberately mix identities, and a requirement code is often driven by support gear rather than the weapon. Use `research/build-database/element-patterns.json` for the measured correlations.

## 3. Choose the engine before the items

### 3.1 Main-attack engines

| Engine | How it works | Optimize first | Main trap |
|---|---|---|---|
| Heavy melee | Slow, extremely large hits | Per-hit packet, realistic hit rate, Quake or another special, non-dodge safety | Maximizing the screenshot hit while ignoring misses, charge time, and survival |
| Tierstack | Slow high-base weapon accelerated with positive tiers | Required final speed, base damage, slot efficiency | Ending one speed tier short |
| Cancelstack | Negative-tier raw-damage armour offset by positive tiers and a fast weapon | Net speed threshold, raw damage, roll certificate | Assuming maximum attack-tier or skill-point rolls |
| Rawstack | Fast weapon plus raw Main Attack damage | Super Fast access, raw damage, survival | Overvaluing Main Attack percentage that does not scale raw damage |

### 3.2 Spell engines

| Engine | How it works | Optimize first | Main trap |
|---|---|---|---|
| Spellspam | Regeneration and low costs support a repeatable cycle | Rotation DPS, mana floor, cast timing | Using first-cast costs and ignoring repeat escalation |
| Spellsteal | Main attacks recover mana between spells | Attack windows, steal per hit, target access | Treating displayed Mana Steal as guaranteed passive income |
| Mixed mana | Regen and steal cover different phases | Worst-phase robustness | Paying too much for redundant mana |
| Intless spell | Gear-based cost reduction replaces Intelligence | Exact costs, item opportunity cost | Treating zero Intelligence as a complete playstyle |
| Heavy spell | One large spell packet with weak sustain | Burst window and reset | Comparing packet damage without setup or recovery time |

### 3.3 Hybrid and role engines

- **Fast hybrid:** schedule attacks and spells in one timeline.
- **Heavy hybrid:** place spells between slow main hits.
- **Poison:** optimize reliable application and uptime, not ordinary crit or Strength scaling.
- **Support:** optimize added party damage, healing, resistance, control, and uptime.
- **Tank:** optimize non-dodge survival, sustain, threat or positioning, and role mechanics.
- **Lootrun:** optimize robust long-run clear speed, AoE, movement, and self-sustain.
- **Guild war:** optimize stable tower damage, non-dodge EHP, and unavoidable-hit sustain.

## 4. Select the class and archetype

Ability trees can mix archetypes, but the main engine should align with the branch and ultimate you intend to use.

### 4.1 Archer

**Baseline:** bow, long range, 70 percent base resistance, 1.30x relative incoming damage before other defence.

#### Boltslinger

Use when you want close-range hit volume and burst.

Optimize:

- Close-range uptime and projectile coverage.
- Arrow Storm and Main Attack hit count.
- Frenzy and Guardian Angels.
- Effects that narrow spread or increase ammunition and cadence.
- Angelic Ascension uptime and what can actually hit one target.

Reject a paper-optimal build if the encounter prevents close-range uptime.

#### Sharpshooter

Use when you want long-range aimed power and a high single-target ceiling.

Optimize:

- Focus uptime under realistic accuracy.
- Range and line of sight.
- Power-shot cadence and miss penalty.
- The Twilight Zone setup and mark realization.

Always publish the assumed hit rate. A build scored at 100 percent accuracy is a benchmark, not an expectation.

#### Trapper

Use when delayed damage, control, traps, and beasts fit the room.

Optimize:

- Trap cap and placement time.
- Enemy pathing and time in the damage area.
- AoE and snare coverage.
- Beast target uptime.
- Extinction Event value in waves versus mobile bosses.

### 4.2 Warrior

**Baseline:** spear, close-range combat, 100 percent base resistance.

#### Fallen

Use when you can manage Corrupted and low-health risk for damage.

Optimize:

- Time safely spent Corrupted.
- Health-cast and Blood Pact economics.
- Damage during the low-health window.
- Exhilarate or other exit recovery.
- Beyond Salvation spikes, detonation, and overhealth.

Do not grant permanent Rage, Corrupted, or maximum low-health bonuses without proving the state is sustainable.

#### Battle Monk

Use for mobile close-range spell combos.

Optimize:

- One explicit spell sequence.
- Discombobulated and Pressure generation.
- Movement uptime and target access.
- Zenith Stance inputs and duration.
- Mana feasibility after movement and utility spells.

#### Paladin

Use for resistance, party buffs, Mantle, Holy Power, and survival.

Optimize:

- EHP without Agility.
- Mantle and Holy Power generation.
- Ally resistance and buff uptime.
- Judgement coverage and duration.
- Personal damage only after the support contract passes.

### 4.3 Mage

**Baseline:** wand, healing and mobility options, 80 percent base resistance, 1.20x relative incoming damage.

#### Riftwalker

Use for movement and ramping damage over longer fights.

Optimize:

- Time to build Distortion and related state.
- Stack decay during invulnerability or movement.
- Dimensional Tear uptime and target access.
- Gravitational Collapse setup and finish.

Score both a short window and a long window. Riftwalker can change rank as the horizon increases.

#### Light Bender

Use for healing-driven support and light constructs.

Optimize:

- Healing needed to activate effects.
- Orb count, range, survival, and attack uptime.
- Healing Efficiency and ally coverage.
- Dawn's damage and healing paths.
- Marginal party value rather than only personal DPS.

#### Arcanist

Use for destructive Mana Bank cycles and accept the healing trade.

Optimize:

- Mana Bank generation per rotation.
- Arcane Transfer timing.
- Chaos Explosion thresholds.
- Judrajim and other expensive stateful actions.
- Survival without the normal Heal safety net.
- Tangled Origin duration and target access.

### 4.4 Assassin

**Baseline:** dagger, close-range burst or sustained damage, 100 percent base resistance.

#### Shadestepper

Use for timed stealth, Marks, back-angle burst, and execution.

Optimize:

- Vanish and Satsujin cooldowns.
- Mark generation and detonation.
- Back-angle access.
- Burst packet over a stated setup horizon.
- Exit safety and downtime between packets.
- Pierce the Veil duration and mark coverage.

Heavy melee can be effective here because Vanish can amplify a planned hit, but the score must include the complete setup cadence.

#### Trickster

Use for clones, Tricks, debuffs, and control.

Optimize:

- Clone count, health, and copied spells.
- Debuff count and effect.
- Stored Tricks and detonation threshold.
- Target density and clone pathing.
- Another Self duration and swap timing.

#### Acrobat

Use for aerial combos and Momentum.

Optimize:

- Momentum generation.
- Aerial uptime under the actual encounter roof and geometry.
- Lacerate hit coverage.
- Mana and movement control.
- Serpent's Garden uptime and whether the target stays inside.

### 4.5 Shaman

**Baseline:** relik, Totem-centric AoE and support, 60 percent base resistance, 1.40x relative incoming damage.

#### Summoner

Use for totems, puppets, effigies, and commanded minions.

Optimize:

- Summon count and rate.
- Effective target uptime, not theoretical attack frequency.
- AI, vision, movement, and room geometry.
- Multi-totem penalties.
- Patchwork Abomination mode, duration, and explosion value.

#### Ritualist

Use for mask switching, chants, and flexible party states.

Optimize:

- Mask sequence and transition cost.
- Mana saved toward Awakened.
- Buff, resistance, and movement uptime.
- Attached-mask downtime.
- Sundered Skies control and repeated hits.

#### Acolyte

Use for health sacrifice, Blood Pool, healing, and support damage.

Optimize:

- Blood Pool generation and spending.
- Health lost per second and recovery.
- Overhealth, tether, and party healing.
- Non-dodge EHP because Shaman's base resistance is low.
- Monument to Gloom placement and duration.

## 5. Build the ability tree before final item selection

Use this order:

1. Select the mandatory base spells and movement tools.
2. Select the main engine and its key nodes.
3. Decide whether an ultimate is part of the contract.
4. Satisfy the archetype count for the desired capstone.
5. Add support, defence, or quality-of-life nodes.
6. Check mutual locks and path connectivity.
7. Count AP. The current end-game maximum is 50.
8. Recalculate spell parts, costs, cooldowns, and class resources.

Do not copy an ability tree because it has the same archetype label. Two trees can share an archetype but select different multipliers, conversions, costs, movement, support, or ultimate access.

## 6. Select Aspects as ability modifiers

### 6.1 Rules

- Five slots are available at full progression.
- Only one Mythic Aspect can be equipped.
- Legendary maximum tier is IV.
- Fabled and Mythic maximum tier is III.
- Live cumulative copy thresholds are 1/5/30/150, 1/15/75, and 1/5/15 respectively.
- Aspects are account-wide, but the selected effects are character-specific.

### 6.2 Five-pass method

1. **Filter by tree.** Remove every Aspect whose ability is absent.
2. **Tag the engine.** Mark damage, mana, healing, resource cap, duration, cooldown, range, AoE, minion, trap, or ultimate effects.
3. **Calculate uptime.** Convert a listed bonus into encounter value. A cooldown cut may be worth nothing if the ability is already gated by another resource.
4. **Compare Mythic opportunity cost.** The Mythic must beat the fifth-best lower-rarity option it displaces.
5. **Use owned tiers.** Maximum-tier catalogues are not valid loadouts unless you own those tiers.

### 6.3 Class-specific Aspect priorities

#### Archer

- Boltslinger: Arrow Storm streams, Guardian Angel ammunition/cadence, Frenzy, spread, hit count.
- Sharpshooter: Focus cap/cooldown, range, reliable aimed-hit effects, Twilight Zone realization.
- Trapper: trap cap, delayed-damage scaling, beast uptime, snare duration, AoE.

#### Warrior

- Fallen: Corrupted economics, Blood Pact, healing exit, low-health damage, Beyond Salvation.
- Battle Monk: combo stacks, Pressure, cooldowns, hit count, Zenith Stance duration.
- Paladin: Mantle, Holy Power, party resistance, healing thresholds, Judgement duration.

#### Mage

- Riftwalker: Distortion persistence, Tear AoE/resistance, long-fight summons, Collapse duration.
- Light Bender: healing activation thresholds, Ophanim and Lightweaver orbs, support range, Dawn throughput.
- Arcanist: Mana Bank cap, Chaos Explosion threshold, Judrajim cooldown, sigil coverage, Tangled Origin.

#### Assassin

- Shadestepper: Vanish/Satsujin cooldowns, Marks, Backstab angle and range, Pierce the Veil.
- Trickster: clone count/health, Tricks capacity, debuff value, Another Self.
- Acrobat: Momentum rate, Lacerate hits, aerial duration, Serpent's Garden uptime.

#### Shaman

- Summoner: puppet cap, movement, Hummingbirds, minion damage, Abomination duration.
- Ritualist: mask multipliers, chants, Awakened threshold/duration, Sundered Skies cadence.
- Acolyte: Blood Pool cap, Bleeding duration, healing triggers, tentacles, Monument duration.

The complete live effect catalogue is in `research/endgame-class-building-evidence.md`.

## 7. Choose equipment in the right order

### 7.1 Lock the weapon

The weapon establishes:

- Class and main engine.
- Base damage by element.
- Starting attack-speed tier.
- Spell base DPS or main-hit packet.
- Powder slots and special options.
- Requirements and Major ID.
- Ascension or roll assumptions.

Compare weapons using the engine's score, not average weapon DPS alone.

For a Mythic, write a weapon contract before searching armour:

```text
Base elements and neutral damage:
Base attack speed:
Skill requirements:
Positive engine IDs:
Penalties that must be cancelled:
Major ID and exact ability interaction:
Powder slots and intended powder special:
Base or Ascended version:
Compatible archetypes:
Candidate combat families:
Content and geometry assumptions:
```

Examples:

- Gaia: Super Slow Earth melee or spell package, Earthen Splinter, raw and percentage Main Attack, poison, Riftwalker state, HPR and range gates.
- Singularity: five-element requirements, 15 powder slots, Super Slow base, Orbital Chain, powder ordering, Arcanist or Riftwalker state.
- Warp: 125 Agility, extreme movement and Teleport efficiency, negative Mana Regen, HPR, and healing. Treat it as a movement engine with cancellation constraints.
- Guardian: health, regeneration, redirection, and Paladin support. Personal DPS is secondary.
- Toxoplasmosis: poison application, Totem priming, Outbreak storage, Uproot release, grouping, and AoE. Ordinary weapon DPS is irrelevant.

Morph is a broad rainbow set and a useful generalist starting point. It does not make `Morph Singularity` automatically optimized. Compare the set's convenience with specialized requirement, mana, damage, EHP, powder, and Major ID support.

### 7.2 Identify keystone items

Keystones are items whose effect cannot be replaced cheaply:

- Major ID or set effect.
- Large raw Main Attack or raw spell damage.
- Positive attack tiers.
- Mana or cost threshold.
- Critical skill-point bridge.
- High health or sustain.
- Archetype-specific interaction.

Lock as few keystones as necessary. Every lock removes search flexibility.

### 7.3 Fill requirements and constraints

Now satisfy:

- Skill-point legality.
- Required attack-speed tier.
- Minimum non-dodge EHP and health.
- Mana and class-resource loop.
- Range, AoE, and movement.
- Budget, availability, and ownership.

Only then maximize the primary score.

### 7.4 Add powders last enough to be informed, early enough to matter

Powder choice changes elemental distribution, conversions, and the active special. It cannot be treated as cosmetic.

- Use **Quake** for heavy Main Attack scaling and AoE when close-range charging is realistic.
- Use **Chain Lightning** for multi-target reach and chains.
- Use **Curse** for planned self or party burst on a target.
- Use **Courage** for short party and self damage windows.
- Use **Wind Prison** for a planned next-hit packet and control.

Model approximately five seconds of active main attacking per special charge, with speed-specific hit counts.

## 8. Skill-point allocation workflow

1. Calculate equipment requirements.
2. Apply only skill-point bonuses that legally contribute.
3. Satisfy the full set with no more than 100 manually assigned to one attribute.
4. Record the manual points consumed by legality.
5. Allocate remaining points according to the engine:
   - Strength for generic and Earth damage.
   - Dexterity for expected crits and Thunder damage.
   - Intelligence for Water damage, maximum mana, and costs.
   - Defence for Fire damage and deterministic reduction.
   - Agility for Air damage and probabilistic mitigation.
6. Re-run damage, costs, mana, and EHP after every allocation change.
7. Save the exact assignment and minimum required item rolls.

Do not compare skill-point allocations using only the in-game displayed percentage. Damage, cost, EHP, and legality use different consequences of the same point.

## 9. Mana and rotation validation

Write the actual repeating sequence:

```text
Example only:
utility spell -> damage spell -> main attack -> damage spell -> movement -> repeat
```

For each action record:

- Cost after Intelligence, raw cost, and percentage cost.
- Repeat-cast escalation.
- Cast or animation time.
- Expected damage parts that hit.
- Mana Regen received during the interval.
- Mana Steal probability and target count.
- Cooldown, buff, stack, or resource change.

Validate at least two windows:

- **Burst window:** enough time to reach and deliver the main packet.
- **Sustained window:** long enough for repeat costs, resource recovery, powder specials, and state decay to matter.

Required outputs:

- Starting and ending mana.
- Lowest mana during the cycle.
- Average mana used and gained per second.
- Result when one expected steal hit misses.
- Recovery after forced movement or an invulnerability phase.

## 10. Survivability validation

Report all of these:

- Total HP.
- EHP without Agility.
- Expected EHP with Agility.
- Elemental defences if the content makes them relevant.
- Health Regen and effective Health Regen.
- Life Steal and the hit assumptions behind it.
- Active resistance, healing, overhealth, Mantle, clones, or class-specific defences.
- Worst expected non-dodged hit.

Do not use Agility-aware EHP as a one-shot guarantee. Do not use maximum Life Steal or armour-special uptime if the encounter does not supply the required hits or state.

## 11. Heavy melee procedure

### 11.1 Contract

Specify:

- Class and weapon.
- Target content and range.
- Per-hit or horizon objective.
- Powder special.
- Minimum non-dodge EHP and sustain.
- Vanish, Corrupted, Wind Prison, Courage, Curse, or other setup state.
- Actual item rolls.

### 11.2 Search order

1. Lock the weapon and intended powder special.
2. Enumerate high raw and percentage Main Attack pieces.
3. Enforce requirements and safety before damage scoring.
4. Calculate compatible raw elemental damage.
5. Apply Strength, crit expectation, ability multipliers, and powders.
6. Add the powder special at reachable charge frequency.
7. Subtract setup, utility, movement, and miss time.
8. Return a Pareto set for damage, EHP, sustain, range, and cost.

### 11.3 Acceptance checks

- Can every main hit connect in the intended range?
- Does the build survive while charging Quake or another special?
- Is Life Steal frequent enough between hits?
- Does Vanish or another amplifier return before the next planned packet?
- Does a simpler lower-hit build kill faster because it misses less or sets up faster?

For a Gaia melee search, start with this configurable profile:

```text
EHP without Agility >= 20,000
Life Steal >= 0
raw Health Regen >= -100
Main Attack Range >= -20%
```

Then add the content-specific non-dodge hit, hit-rate, movement, mana, Earthen Splinter or Quake cadence, poison realization, and Riftwalker-uptime requirements. A candidate that misses one of the four gates is infeasible, not a near-winner that can be rescued by a weighted score.

## 12. Cancelstack procedure

### 12.1 Treat speed as a hard constraint

```text
netTier = base weapon tier + sum of all attack-speed tier IDs
finalTier = clamp(netTier, Super Slow, Super Fast)
```

Common objective order:

1. Final tier equals Super Fast.
2. Requirements, EHP, sustain, and budget pass.
3. Main-attack DPS is maximized.
4. Range, movement, AoE, and quality of life break ties.

### 12.2 Use actual rolls

For each result report:

- Base weapon speed.
- Every positive and negative tier contribution.
- Actual or minimum acceptable tier roll.
- Final net tier and hits per second.
- Raw Main Attack rolls on negative-tier pieces.
- Skill-point rolls required for legality.
- Damage if any one tier roll falls by one.
- Damage at the next lower final speed.

Moving from Super Fast to Very Fast reduces frequency from 4.30 to 3.10 hits per second. A maximum-roll link without a roll certificate is not a finished cancelstack build.

### 12.3 Solver pruning

Useful early bounds:

- If the partial build plus all remaining positive tiers cannot reach the target, prune.
- If remaining skill-point bonuses cannot satisfy a requirement, prune.
- If maximum remaining health and Defence cannot reach the EHP threshold, prune.
- If a partial set already violates a user ban, set rule, or slot limit, prune.

Score expensive damage and rotations only after these gates pass.

## 13. Optimization target templates

### 13.1 General-purpose combat

```text
maximize sustained rotation DPS
subject to EHP_no_agi >= chosen threshold
           mana floor >= 0 under one missed steal
           walk speed >= chosen threshold
           owned items and rolls only
then rank by burst, range, sustain, and cost
```

### 13.2 Heavy melee

```text
maximize expected delivered damage over H
subject to legal SP
           EHP_no_agi and recovery thresholds
           explicit hit probability
           explicit powder charge and setup state
```

### 13.3 Cancelstack or tierstack

```text
require final attack speed = Super Fast
require legal SP, EHP, sustain, and budget
maximize main-attack DPS
then maximize robustness to lower rolls
```

### 13.4 Raid support

```text
maximize marginal party damage + healing + prevented damage
subject to buff/heal range and uptime
           personal survival
           role-required abilities and Aspects
then maximize personal DPS
```

### 13.5 Lootrun

```text
maximize expected clear progress over a long run
subject to robust EHP, sustain, AoE, and movement
evaluate under conservative boon and curse scenarios
then compare peak damage
```

## 14. Pareto comparison

Do not force every build into one score too early. Keep candidates that are not dominated across:

- Sustained DPS.
- Burst damage.
- EHP without Agility.
- Health sustain.
- Mana sustain.
- Range and AoE.
- Walk speed and movement.
- Party contribution.
- Cost and availability.
- Roll robustness.
- Execution difficulty.

A 2 percent damage gain can be a downgrade if it loses the only comfortable mana margin, falls below a one-shot threshold, requires perfect aim, or depends on unaffordable rolls.

## 15. Common build failures

1. **Optimizing the wrong content.** A dummy score is not a lootrun or raid score.
2. **Ignoring legality.** The linked set may require impossible skill points or better rolls.
3. **Missing a speed tier.** Particularly severe for cancelstack and tierstack.
4. **Adding separate DPS numbers.** Attacks and spells share time.
5. **Assuming all spell parts hit.** Many are conditional or mutually exclusive.
6. **Using first-cast mana costs.** Repeat casting changes costs.
7. **Treating steal as passive.** It requires successful attacks and can depend on targets.
8. **Assuming permanent buffs.** Powder and armour specials have charge or state requirements.
9. **Selecting Aspects by rarity.** The effect may target an absent ability.
10. **Using maximum Aspect tiers.** Owned progression may be much lower.
11. **Using Agility EHP as a guarantee.** A non-dodged hit still matters.
12. **Ignoring class resistance.** The same HP and skill points do not give the same EHP on every class.
13. **Ignoring rolls.** Tier, skill-point, mana, and raw-damage rolls can cross thresholds.
14. **Ignoring crafted durability.** A degraded crafted item does not retain full IDs.
15. **Over-locking items.** Too many keystones prevent the search from finding better interactions.
16. **Publishing a single result.** A frontier makes tradeoffs visible.

## 16. Final in-game validation

### 16.1 Dummy test

Record:

- Exact tree, Aspects, powders, Tomes, and rolls.
- Rotation.
- Burst window damage.
- Sustained window damage.
- Mana floor.
- Main-attack miss rate.
- Buff, stack, and powder-special uptime.

### 16.2 Target-content test

Record:

- Completion time or time-to-kill.
- Deaths, second chances, or failed rooms.
- Forced potions or emergency recovery.
- Time unable to damage the target.
- Actual range and movement problems.
- Party buff, heal, control, or revive outcomes.
- Whether the build remains comfortable after several runs.

### 16.3 Decision rule

Keep the theoretical winner only if it also wins the contract. Otherwise select the build that meets the thresholds and performs more reliably in target content.

## 17. Build review checklist

### Contract

- [ ] Target content and party size are stated.
- [ ] Burst or sustained horizon is stated.
- [ ] Damage, defence, mana, movement, and budget requirements are stated.

### Legality

- [ ] Correct weapon and equipment slots.
- [ ] Level, quest, class, and set restrictions pass.
- [ ] Skill-point assignment is legal at owned rolls.
- [ ] Ability path is connected and within 50 AP.
- [ ] Locks and archetype minimums pass.
- [ ] Five or fewer Aspects and one or fewer Mythic.
- [ ] Powder capacity, tier, and order are legal.

### Engine

- [ ] Main damage or support engine is explicit.
- [ ] Final attack-speed tier is recorded.
- [ ] Spell cycle or attack cadence is recorded.
- [ ] Mana and class-resource loops pass.
- [ ] Buff, debuff, stack, summon, or trap uptime is stated.

### Survivability

- [ ] HP and non-dodge EHP are reported.
- [ ] Expected Agility EHP is labeled probabilistic.
- [ ] Health Regen, Life Steal, healing, and overhealth assumptions are explicit.
- [ ] Worst non-dodged hit is considered.

### Practicality

- [ ] Actual or minimum item rolls are recorded.
- [ ] Mythic, ascension, crafted, Tome, and Aspect requirements are disclosed.
- [ ] Cost and availability are acceptable.
- [ ] Dummy and target-content tests are complete.

## 18. Current Mythic and functional-build index

Use this as a routing index. The observed families are current forum examples, not an ordered tier list.

| Class | All current Mythic weapons | Start by testing |
|---|---|---|
| Archer | Az, Divzer, Epoch, Eschaton, Freedom, Grandmother, Ignis, Labyrinth, Revolution, Spring, Stratiformis | Sharpshooter range, Boltslinger cadence, Trapper overlap, tierstack, HPR cancellation, mobility |
| Assassin | Archangel, Architect, Cataclysm, Grimtrap, Hanafubuki, Inferno, Nirvana, Nullification, Oblivion, Vengeance, Weathered | Vanish and Marked state, clones, aerial uptime, heavy melee, spellsteal, HPR cancellation |
| Mage | Fatal, Gaia, Halcyon, Lament, Monster, Pure, Quetzalcoatl, Riptide, Singularity, Trance, Warp | Mana Bank, Distortion, Ophanim contact, melee, healing, rainbow powders, Teleport mobility |
| Shaman | Absolution, Aftershock, Fantasia, Fate, Hadal, Immolation, Olympic, Resonance, Sunstar, Toxoplasmosis, Transfiguration | Health sacrifice, masks, target links, Totem and puppets, tierstack, poison, healing |
| Warrior | Alkatraz, Apocalypse, Ascendancy, Bloodbath, Collapse, Convergence, Guardian, Hero, Idol, Restitution, Thrundacrack | Fallen health state, Blood Pact, Paladin support, Battle Monk movement, heavy melee, spell sequence |

Current representative examples:

- [Gaia Riftwalker Melee](https://wynnbuilder.github.io/builder/#CW0W9Qu0HfXZBn67VDDSKdG46A1Y4EArqGSf-TFTlPo8M0)
- [Singularity Arcanist](https://wynnbuilder-beta.github.io/builder/#CT013ntFWi3i5146KyG15ko1jcD00SvzulcM-d3)
- [Warp Riftwalker](https://wynnbuilder.github.io/builder/#CX0cFBAE9uAwiKieWZPnYEfE9r9B7XR12RAof80GxvLPYserfE)
- [Epoch Tier Stack](https://wynnbuilder.github.io/builder/#CW0iuAw0HfeJOcYwZlcYwZfHKdmW4om0wqDSfVqxVm3tJuG)
- [Guardian Paladin](https://wynnbuilder-beta.github.io/builder/#CV0O0HsEyH4OQG05eCYy4AZHtqRgBzR-1-UI7q0)
- [Toxoplasmosis Mob Grinding](https://wynnbuilder-beta.github.io/builder/#CV06End6HWHoJ26ZVXVXVnXY482CIUImOyByByBQKa0HmaZPoUoapaZPoaI241JEc9x9JEJEc9JA90yUe+8zQ8-pkzv5OOYi0)

The database at `research/build-database/` contains:

- `functional-builds.json`: all 129 unique direct links and provenance.
- `weapon-build-families.json`: all 55 Mythic weapons, observed examples, signature data, and inferred candidate families.
- `mythics.json`: the full official v3.7.2 Mythic snapshot, including Ascensions and Major IDs.
- `ability-trees.json` and `aspects.json`: complete live data for every class.
- `threshold-profiles.json`: configurable baseline, family, archetype, and signature-weapon gates.
- `build-patterns.json`: decoded item, requirement-code, engine, and archetype frequencies.

When a forum URL fails local decoding, retain the source link and inspect it in the current builder. Do not fabricate missing crafted items, tree nodes, Aspects, or Tomes.

## 19. Sources and companion material

- Research paper: `research/wynncraft-endgame-class-building-paper.md`
- Complete live Aspect catalogue and evidence ledger: `research/endgame-class-building-evidence.md`
- Current Mythic and forum-build evidence: `research/current-meta-mythic-build-evidence.md`
- Functional research database: `research/build-database/README.md`
- Ultimate Build Guide: https://forums.wynncraft.com/threads/the-ultimate-build-guide.320092/
- Official item database endpoint: https://docs.wynncraft.com/modules/item-recipe/list-items
- Official API introduction: https://docs.wynncraft.com/welcome
- Official ability-tree endpoint: https://docs.wynncraft.com/modules/ability-aspect/get-ability-tree
- Official Aspect endpoint: https://docs.wynncraft.com/modules/ability-aspect/list-aspects
- Official Wiki, Ability Tree: https://wynncraft.wiki.gg/wiki/Ability_Tree
- Official Wiki, Aspects: https://wynncraft.wiki.gg/wiki/Aspects
- Official Wiki, Skill Points: https://wynncraft.wiki.gg/wiki/Strength
- Official Wiki, Identifications: https://wynncraft.wiki.gg/wiki/Identifying
- Official Wiki, Powders: https://wynncraft.wiki.gg/wiki/Powders
- Official Wiki, Builds: https://wynncraft.wiki.gg/wiki/Builds
- Official Wiki, Raids: https://wynncraft.wiki.gg/wiki/Raid
- Official Wiki, Lootrunning: https://wynncraft.wiki.gg/wiki/Lootrunning
- Community terminology: https://forums.wynncraft.com/threads/2-0-class-building-terminology.305994/
