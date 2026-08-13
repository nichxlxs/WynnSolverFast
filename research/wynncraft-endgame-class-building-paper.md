# End-Game Class Building in Wynncraft

## A mechanics-first framework for builds, archetypes, Aspects, and optimization

**Research baseline:** Wynncraft 2.2.2, 13 August 2026
**Primary authority:** live Wynncraft API v3.7.2 and official API documentation
**Implementation reference:** the current WynnSolverFast/WynnBuilder codebase

## Abstract

End-game class building in Wynncraft is a constrained, stateful, multi-objective design problem. A complete build is not merely a weapon and eight wearable items. It combines equipment and item rolls, skill-point allocation, powders, Mastery Tomes, a connected ability-tree path, five Aspects, Major Identifications, class mechanics, a combat rotation, and assumptions about the target encounter. The best configuration for a raid support role is therefore not the best configuration for a solo boss, lootrun, guild war, or short burst test.

This paper develops a current model of class building for Wynncraft 2.2.2. It distinguishes official mechanics from community vocabulary, describes the major build families, explains all five classes and their archetypes, and treats Aspects as tree-dependent ability modifiers rather than generic item stats. It audits the maintained Ultimate Build Guide, captures all 129 unique direct WynnBuilder records, and covers all 55 unique Mythic weapons plus the 11 Mythic armour pieces. It then formulates build selection as exact legal enumeration followed by threshold-aware scoring, rotation simulation, and Pareto ranking. Particular attention is given to heavy melee and cancelstack. These families benefit disproportionately from exact optimization because attack-speed tiers, skill-point feasibility, item rolls, raw damage, and powder-special interactions create large discontinuities that cannot be handled reliably by ranking items one at a time.

The main conclusion is that optimization is valuable in proportion to a build's thresholds and modelability. Cancelstack, tierstack, heavy melee, rawstack, and mana-constrained spell cycles often reward exact search. Support, control, minion, aim-heavy, and randomized lootrun builds still benefit from search, but their final ranking requires encounter and execution evidence rather than one static score.

## 1. Scope, version boundary, and method

### 1.1 Research questions

This study addresses eight questions:

1. What constitutes a complete end-game build in current Wynncraft?
2. What build families are used by the class-building community, and what mechanics define them?
3. How do classes, archetypes, ability trees, ultimates, and Aspects change the optimization problem?
4. Which metrics are meaningful for different end-game activities?
5. Which build families benefit most from exact optimization?
6. How should a build solver represent legality, thresholds, state, uncertainty, and practical validation?
7. What current community build families are being used with every Mythic weapon?
8. Which elemental, identification, Major ID, and equipment-requirement patterns recur across functional builds?

### 1.2 Source hierarchy

Sources were ranked in this order:

1. Live official Wynncraft API responses for current ability trees and Aspects.
2. Official Wynncraft API documentation and changelogs.
3. Current pages on the officially hosted Wynncraft Wiki.
4. Current implementation source in this repository for formulas and solver semantics.
5. The current Ultimate Build Guide first post for community build examples and content tags.
6. Other Wynncraft forum guides for terminology, critique, and falsification cases.

This distinction matters. The official 2.2.2 Wiki page states that ability changes are still incomplete, while the live API exposes the current trees and Aspect data. The repository's checked-in `2.2.2.0` Aspect data matches live names, rarity, counts, and thresholds, but at least one effect has drifted: the live maximum effect for Aspect of a Thousand Hours is `+3 Area of Effect`, while the local snapshot contains `+1`. Live API data is therefore authoritative for current user-facing ability and Aspect facts.

### 1.3 Current baseline

Version 2.2.0, the Fruma Expansion, raised end-game content to level 120, added ultimate abilities, increased the ability-point budget to 50, introduced more than one thousand items, added new Major IDs and Mythics, extended powders to Tier VII, introduced item ascension, and removed manual armour equip-order handling. Version 2.2.2 released on 10 July 2026 and is the current baseline used here.

Mechanics and item balance are patch-sensitive. This paper defines a system and method, not a permanent tier list.

The current machine-readable research snapshot contains 6,701 official item records, 124 Mythic records, 71 Mythic weapon records including Ascensions, 55 unique Mythic weapon names, complete live trees and Aspects for all five classes, and all 129 unique direct forum WynnBuilder builds. The forum first post was last edited 11 August 2026. Its separate meta tracker was not yet populated, so this study calls the links current community examples rather than a complete tier ranking.

## 2. The build as a complete combat configuration

A build should be represented as the tuple:

```text
B = (class, equipment, rolls, skill points, powders, tomes,
     ability path, aspects, major IDs, rotation, encounter assumptions)
```

### 2.1 Equipment and progression layers

| Layer | Decisions | Principal effects |
|---|---|---|
| Weapon | Bow, spear, wand, dagger, or relik; base damage; base speed; requirements; rolls; powder order; Major ID; ascension | Establishes main-attack packet, spell base, attack-speed starting point, conversion, and often the build family |
| Armour | Helmet, chestplate, leggings, boots | Health, elemental defence, requirements, damage, mana, sustain, attack-speed tiers, set bonuses, armour powder specials |
| Accessories | Two rings, one bracelet, one necklace | Efficient requirements, skill points, damage, mana, attack speed, utility, and set completion |
| Skill points | Strength, Dexterity, Intelligence, Defence, Agility | Legality, damage, crit chance, maximum mana, costs, damage reduction, dodge, and elemental scaling |
| Powders | Ordered weapon powders and armour powders | Base damage, neutral conversion, elemental defence, and one special per eligible item |
| Mastery Tomes | Four Armour, two Weapon, two Marathon, two Expertise, two Mysticism, one Lootrunning, one Guild | Health, damage, movement, mana, costs, loot, and skill-point support |
| Ability tree | Connected nodes, prerequisites, locks, archetype requirements, up to 50 AP | Defines the actual class kit, resource loops, multipliers, movement, support, and ultimate |
| Aspects | Five slots, no more than one Mythic | Modifies abilities, resource caps, cooldowns, hit counts, range, duration, and ultimate performance |
| Item rolls | Actual or assumed identification values | Can change legality, speed breakpoints, mana feasibility, survivability, and damage |
| Rotation and state | Attack and spell order, cooldowns, repeat costs, stacks, buffs, debuffs, target count | Converts sheet statistics into encounter output |
| Encounter | Content, party, duration, geometry, range, movement, incoming damage, allowed consumables | Selects the correct objectives and constraints |

### 2.2 Legal build constraints

A candidate is not a build until it passes all relevant constraints:

1. Every item occupies the correct slot and the weapon matches the class.
2. Combat-level, class, quest, set, and other restrictions are met.
3. Skill-point requirements are satisfied with the legal manual budget and per-attribute cap.
4. Equipment-granted skill points obey current requirement rules. Weapon and crafted-item skill points do not satisfy other equipment requirements.
5. The ability path is connected and obeys AP, prerequisites, archetype minimums, and mutual locks.
6. No more than five Aspects are selected and no more than one is Mythic.
7. Each Aspect modifies an ability present in the selected tree, unless its generic effect is intentionally selected.
8. Powder count, tier, order, and item capacity are legal. An item can have only one powder special.
9. Crafted equipment assumptions include durability and repairs.
10. The intended rotation is feasible under real spell costs, repeat-cast escalation, maximum mana, regeneration, steal, and downtime.

The 2.2 removal of manual armour equip order reduces player friction, but it does not eliminate the solver's responsibility to prove requirement feasibility.

## 3. Mechanics that dominate end-game building

### 3.1 Skill points

End-game characters normally have 200 manually assignable points. At most 100 can be assigned manually to one attribute, while item effects can raise a final attribute to 150.

- **Strength** increases all damage and Earth damage.
- **Dexterity** increases critical-hit chance and Thunder damage.
- **Intelligence** increases Water damage and maximum mana, and reduces spell costs.
- **Defence** increases Fire damage and reduces incoming damage.
- **Agility** increases Air damage and gives a chance to reduce a hit by 90 percent.

Strength, Dexterity, and the maximum-mana part of Intelligence use a diminishing curve that reaches about 80.8 percent at 150. Intelligence cost reduction reaches 50 percent at 150. Defence reaches 70.0 percent reduction and Agility reaches a 76.8 percent dodge chance.

The local calculator implements the skill-point curve as:

```text
p(s) = [r / (1-r)] * [1 - r^s] / 100
r = 0.9908, 0 <= s <= 150
```

Skill points are both requirements and performance variables. A solver must not spend all points on damage before proving that the complete equipment set is legal.

### 3.2 Attack-speed tiers

Attack speed is a seven-step ladder. Net attack-speed IDs move the weapon up or down this ladder and clamp at the endpoints.

| Final speed | Hits per second |
|---|---:|
| Super Slow | 0.51 |
| Very Slow | 0.83 |
| Slow | 1.50 |
| Normal | 2.05 |
| Fast | 2.50 |
| Very Fast | 3.10 |
| Super Fast | 4.30 |

This is a discontinuous variable, not a smooth percentage. Moving from Very Fast to Super Fast increases theoretical attack frequency by about 38.7 percent. That single breakpoint can dominate the difference between two cancelstack or tierstack candidates.

### 3.3 Damage pipeline

The current calculator performs a high-level sequence of:

1. Read neutral and elemental weapon damage, including powders.
2. Apply ability conversions and damage parts.
3. Apply the attack-speed multiplier when the calculation uses weapon DPS rather than a single hit.
4. Add eligible elemental weapon damage.
5. Apply generic, attack-type, elemental, and skill-point percentage bonuses.
6. Allocate raw damage to compatible neutral or converted elements.
7. Apply Strength and other multiplicative effects.
8. Produce normal and critical ranges, then weight them by Dexterity for an expectation.

Important consequences follow:

- Percent damage scales weapon base damage, not raw damage IDs.
- Raw damage contributes only to compatible damage parts and is multiplied by the relevant attack or spell coefficient.
- Elemental presence and conversion affect the value of raw elemental damage.
- Strength and Dexterity do not simply multiply one another on a critical hit.
- Major IDs and ability modifiers can create separate multiplicative layers.
- A displayed sum of all spell parts may be impossible to realize if parts are mutually exclusive or cannot all hit the same target.

### 3.4 Mana and spell cost

The current local formula is represented as:

```text
baseAfterInt = baseCost * (1 - intelligenceReduction)
afterRaw     = baseAfterInt + rawSpellCost
afterPercent = afterRaw * (1 + percentSpellCost / 100)
finalCost    = max(1, afterPercent * (1 + finalPercentModifier / 100))
```

Repeatedly casting the same spell quickly increases its cost. The ordinary minimum is 1 mana unless a specific Major ID changes the rule. Mana Regen is passive, while Mana Steal is realized through main attacks and certain powder-special hits. A spell build must therefore be scored against an explicit rotation, not a first-cast tooltip.

### 3.5 Steal mechanics

Life Steal and Mana Steal are stated per three seconds of continuous attacking. Slower attacks receive a larger fraction per hit.

| Speed | Single-hit fraction per hit |
|---|---:|
| Super Slow | 65.4% |
| Very Slow | 40.2% |
| Slow | 22.2% |
| Normal | 16.3% |
| Fast | 13.3% |
| Very Fast | 10.7% |
| Super Fast | 7.8% |

Multi-hit main attacks divide the fraction among their hits. Powder specials can trigger steal, including each valid Quake target and each Chain Lightning chain. Spellsteal sustain is consequently sensitive to hit timing, target count, attack form, and powder-special use.

### 3.6 Effective health and recovery

The local implementation uses the conceptual form:

```text
EHP_no_agi = totalHP /
             [(1 - defenceReduction) * classDamageFactor * otherMultipliers]

classDamageFactor = 2 - classResistance
```

| Class | Base resistance | Relative incoming damage |
|---|---:|---:|
| Warrior | 100% | 1.00x |
| Assassin | 100% | 1.00x |
| Mage | 80% | 1.20x |
| Archer | 70% | 1.30x |
| Shaman | 60% | 1.40x |

Agility-aware EHP is an expectation over dodged and non-dodged hits. It is not a guarantee against a large non-dodged hit. Serious build comparison should report total HP, EHP without Agility, expected EHP with Agility, Health Regen, Life Steal, and any class-specific healing or overhealth separately.

### 3.7 Powders and specials

Two same-element powders of Tier IV or higher unlock one special on an eligible item. Attack-speed-specific charge counts are scaled to approximately five seconds of continuous main attacking: 3 hits at Super Slow and 22 at Super Fast.

| Element | Weapon special | Optimization role |
|---|---|---|
| Earth | Quake | Large Main Attack-scaled AoE, excellent for heavy melee, target-count-sensitive steal |
| Thunder | Chain Lightning | Multi-target Main Attack scaling, chains, wave clear, and steal |
| Water | Curse | Short damage-taken debuff for self and party burst |
| Fire | Courage | Damage flare plus short self and party damage buff |
| Air | Wind Prison | Control plus a large multiplier on a planned next hit |

Armour specials are conditional. Rage depends on missing health, Kill Streak on kills, Concentration on mana spending, Endurance on taking hits, and Dodge on staying near enemies without being hit. Their maximum values must not be assumed without an encounter-state model.

## 4. Build taxonomy

The following terms are community vocabulary. They describe the main damage engine or resource loop, not an official class selection menu.

### 4.1 Main-attack builds

**Heavy melee.** Concentrates damage into slow individual main attacks, usually with high raw and percentage Main Attack damage and negative attack-speed tiers. Utility spells, Vanish, and weapon powder specials can be central even though the main hit is the primary packet.

**Tierstack.** Starts with a slow, high-base-damage weapon and adds positive attack-speed tiers, commonly targeting Super Fast. The main trade is slot efficiency: every tier item competes with raw damage, percentage damage, requirements, and survival.

**Cancelstack.** Uses high raw Main Attack armour with negative tiers, then cancels those penalties using positive-tier equipment and an already-fast weapon. Its key question is whether the complete set reaches the chosen final tier.

**Rawstack.** Uses a fast weapon, commonly ending at Super Fast, and repeats large raw Main Attack additions at 4.30 hits per second.

### 4.2 Spell builds

**Spellspam.** Uses Mana Regen, Intelligence, and cost reductions to repeat a spell cycle.

**Spellsteal.** Weaves main attacks into the spell cycle to recover mana through Mana Steal. Slower attack speeds often improve mana per successful hit.

**Mixed mana.** Uses both regeneration and steal. It may be more robust across phases with different target access, although excess sustain can displace damage or survival.

**Intless or zero-Int spell.** Uses little or no Intelligence and relies on gear-based spell-cost reductions. This describes stat allocation, not a complete rotation.

**Heavy spell.** Sacrifices sustain and cost efficiency for a large spell packet. It is useful only when the burst window, recovery, and encounter justify the trade.

### 4.3 Hybrids and special engines

**Fast hybrid.** Combines fast main attacks and spells. A valid score must schedule both, since adding separate sheet DPS values double-counts time.

**Heavy hybrid.** Places spells between Super Slow main attacks and may use both heavy raw damage and spell damage.

**Poison.** Optimizes application and three-second uptime. Poison cannot crit and does not receive Strength's generic damage multiplier, so ordinary damage rankings are misleading.

**Support or tank.** Optimizes healing, buffs, resistance, damage prevented, control, revives, and party output. Personal DPS is secondary.

**Content builds.** Lootrun, guild-war, raid, XP, loot, and movement builds are defined by their activity. Their correct objective often includes clear speed, range, AoE, robustness, sustain, or rewards per time rather than dummy DPS.

## 5. Ability trees, archetypes, and ultimates

Every class has three archetypes, but a legal build may mix branches. Tree selection is a graph problem with connected paths, prerequisites, AP costs, archetype-count requirements, and mutually locked choices. Current live API responses expose three ultimate abilities per class. Each ultimate is a final archetype commitment, not merely an extra spell multiplier.

### 5.1 Archer

Archer uses bows, has 70 percent base resistance, and trades defence for range and damage.

- **Boltslinger:** close-range hit volume and burst. Its ultimate, **Angelic Ascension**, empowers Main Attack, Arrow Storm, Arrow Bomb, and Guardian Angels. Builds should model close-range uptime, projectile spread, hit count, Frenzy, and Guardian Angel cadence.
- **Sharpshooter:** aimed power shots and range. Its ultimate, **The Twilight Zone**, marks visible enemies and converts marks into a large spell and main-attack release. Builds should treat accuracy and Focus retention as first-class assumptions.
- **Trapper:** delayed traps, beasts, crowd control, and enemy pathing. Its ultimate, **Extinction Event**, carpet-bombs an area and leaves traps. Builds should score trap cap, placement, delayed damage, AoE, and beast uptime.

### 5.2 Warrior

Warrior uses spears and has 100 percent base resistance.

- **Fallen:** high-risk damage through Corrupted and low-health states. **Beyond Salvation** creates spikes that can be detonated for damage and overhealth. Builds must model corruption uptime, healing lockout, exit recovery, and realistic low-health safety.
- **Battle Monk:** close-range spell combos, mobility, knockback, and stack-based combat. **Zenith Stance** adds commanded fists that respond to spells or main attacks. Rotation order and uptime matter more than one spell tooltip.
- **Paladin:** resistance, ally support, Mantle, Holy Power, and self-preservation. **Judgement** deals a large hit and grants party buffs. Party contribution and non-dodge EHP are primary objectives.

### 5.3 Mage

Mage uses wands and has 80 percent base resistance.

- **Riftwalker:** mobility and long-fight ramp through Distortion and dimensional effects. **Gravitational Collapse** overloads the Dimensional Tear and ends in a large collapse. Builds should price setup time, stack decay, and phase resets.
- **Light Bender:** healing-driven support and offensive light constructs. **Dawn** summons a sun that damages enemies and heals allies. Healing thresholds, orb count, range, and party coverage dominate.
- **Arcanist:** destructive mana banking that trades away normal healing. **Tangled Origin** summons coordinated Thunder and Fire serpents. Mana Bank generation, Arcane Transfer timing, Chaos Explosion thresholds, and recovery risk define the build.

### 5.4 Assassin

Assassin uses daggers and has 100 percent base resistance.

- **Shadestepper:** stealth, marks, back-angle execution, and timed burst. **Pierce the Veil** grants an unbreakable Vanish state and applies Marks nearby. Burst setup, angle access, cooldowns, and exit safety are essential.
- **Trickster:** clones, Tricks, control, and debuffs. **Another Self** creates a clone that copies spells and can swap positions. Clone survival, stored Tricks, debuff count, target density, and swap timing are state variables.
- **Acrobat:** aerial movement and combo-heavy Momentum. **Serpent's Garden** creates a damaging garden that accelerates Momentum. The build must be evaluated with aerial uptime, control difficulty, and practical hit coverage.

### 5.5 Shaman

Shaman uses reliks and has 60 percent base resistance, the lowest of the five classes.

- **Summoner:** totems, puppets, effigies, and minion command. **Patchwork Abomination** creates a switchable ranged or close-range summon. AI, target access, summon uptime, and multi-totem penalties must be represented.
- **Ritualist:** masks, chants, state switching, and flexible party effects. **Sundered Skies** enlarges the Totem and adds repeated stunning lightning. Mask sequence, Awakened threshold, and buff uptime determine performance.
- **Acolyte:** health sacrifice, Blood Pool, healing, and support damage. **Monument to Gloom** empowers a Totem to restore Blood Pool or party health. Self-damage, overhealth, healing recovery, and tether uptime are hard constraints.

## 6. Aspect trees and selection

### 6.1 System rules

Aspects are account-wide Raid rewards stored through the ability-tree interface. A character has five Aspect slots. The fifth progression-gated slot requires combat level 80 and Sentinel III Raid Division rank. Only one Mythic Aspect may be equipped.

Live cumulative copy thresholds are:

| Rarity | Tier thresholds | Maximum tier |
|---|---|---|
| Legendary | 1, 5, 30, 150 | IV |
| Fabled | 1, 15, 75 | III |
| Mythic | 1, 5, 15 | III |

Legendary Aspects generally improve generic abilities. Fabled Aspects tend to target archetype mechanics. Mythic Aspects make major archetype or ultimate changes. Rarity is not a universal priority order. An active Legendary that fixes range or cadence can be more valuable than a Fabled effect attached to an unselected node.

### 6.2 Selection model

Aspect selection should proceed in five passes:

1. Remove every Aspect whose referenced ability is absent from the chosen tree.
2. Identify the build's active engine: main attack, a spell cycle, stacks, summons, traps, healing, support, or ultimate cycling.
3. Quantify the actual gain to that engine: damage, hits, cap, cooldown, duration, range, AoE, activation threshold, or resource throughput.
4. Reserve the Mythic slot only if its gain exceeds the best five-slot combination without it.
5. Re-score using the Aspect tiers actually owned, not maximum-tier values.

### 6.3 Current class portfolios

The live API contained 128 Aspects on the research date: 25 Archer, 27 Warrior, 26 Assassin, 25 Mage, and 25 Shaman. The companion evidence catalogue records every name and maximum-tier effect. The portfolios can be understood by what they modify:

| Class | Generic and Fabled themes | Mythic decisions |
|---|---|---|
| Archer | Projectile spread, pierce, Arrow Bomb, Arrow Storm, Guardian Angels, Focus, traps, beasts, Frenzy | Generic ultimate cycling versus dedicated Boltslinger, Sharpshooter, or Trapper mechanics |
| Warrior | Bash, Uppercut, War Scream, Bloodied Armory, Holy Power, Corruption recovery, Discombobulated, party resistance | Generic ultimate cycling versus Fallen risk, Paladin Mantle/Holy Power, or Battle Monk combo power |
| Mage | Range, AoE, Time Dilation, Distortion, Lightweaver thresholds, Mana Bank, Judrajim, sigils, summons | Generic ultimate cycling versus Light Bender orbs, Riftwalker persistence, or Arcanist mana thresholds |
| Assassin | Dash, Main Attack range, Smoke Bomb, Multihit, Backstab, Vanish, Tricks, Knives, Momentum | Generic ultimate cycling versus Shadestepper cooldowns, Trickster clones/debuffs, or Acrobat combo cadence |
| Shaman | Totem duration/AoE, Aura, puppets, Effigies, masks, Blood Pool, summons, chants, party resistance | Generic ultimate cycling versus Acolyte blood mechanics, Summoner multi-totem/minions, or Ritualist Awakened |

### 6.4 Mythic Aspect tradeoffs

Each class currently has one general class Mythic that improves ultimate charging and all three ultimate branches, plus one Mythic for each archetype. This creates a clear decision:

- Select the general Mythic when frequent ultimate access and flexible tree use produce more value.
- Select the archetype Mythic when it changes the primary engine enough to outperform faster ultimate cycling.
- Select no Mythic if five lower-rarity effects provide better value at the tiers actually owned.

This is a constrained five-slot set-selection problem. It should not be solved by rarity or maximum-tier text alone.

## 7. Content-specific objectives

| Target content | Primary objective | Hard constraints | Secondary objectives |
|---|---|---|---|
| Solo boss | Realistic sustained time-to-kill | Survive a non-dodged hit, stable resource loop, target access | Mobility, healing, burst phases, consistency |
| Short burst | Damage in a fixed window or packet | Reachable setup state, survival until delivery | Cooldown, repeatability, aim tolerance |
| Raid DPS | Party-adjusted sustained damage | Challenge survival, movement, role coverage | Buff synergy, AoE, revive/support capability |
| Raid support | Marginal party damage, healing, and damage prevented | Buff/heal uptime, range, personal survival | Personal DPS, control, ease of execution |
| Lootrun | Long-run expected progress and rewards | Robust EHP, sustain, movement, AoE, curse tolerance | Peak DPS, boon scaling, chest utility |
| Guild war | Tower time-to-kill | Non-dodge EHP, unavoidable-hit sustain, stable boss damage | Aggro role, party buffs, setup movement |
| General use | Pareto balance | Legal, affordable, playable, acceptable damage and EHP | Walk speed, range, forgiving rotation |

Every reported candidate should include:

1. Average main-attack damage, final speed, and main-attack DPS.
2. Relevant spell-part values with mutual exclusions identified.
3. Burst and sustained rotation DPS over stated horizons.
4. Mana consumption, regeneration, steal assumptions, and worst-cycle floor.
5. HP, EHP without Agility, expected EHP with Agility, and recovery.
6. Range, AoE, walk speed, and movement assumptions.
7. Setup time and uptime for stacks, buffs, debuffs, powders, and armour specials.
8. Cost, availability, ascension, crafted durability, Tome, Aspect, and roll requirements.

## 8. What current "meta" evidence establishes

The maintained [Ultimate Build Guide](https://forums.wynncraft.com/threads/the-ultimate-build-guide.320092/) is the strongest single current community corpus found. Its author describes the listed builds as public optimal examples and uses content tags such as TNA, TCC, NOL, NOTG, WTP, Annihilation, and lootrun to indicate suitability. The post also says users should modify examples for more damage, EHP, or mana sustain and that not every playstyle is represented.

The audit recovered all 129 unique direct WynnBuilder records from the first post: 26 Archer, 26 Assassin, 36 Mage, 21 Shaman, and 20 Warrior. The extra Mage record is the auxiliary Timerift build in the Warp notes. Every one of the 55 unique current Mythic weapons has at least one direct example. There are crafted and non-crafted variants, alternate archetypes, lootrun variants, and content-tagged builds.

This is evidence of a current functional community recommendation, not proof of a global optimum. The guide's dedicated meta-tracker sections currently say that they will be updated later. Its builds also use different crafted policies, Aspect tiers, Tomes, rolls, party assumptions, and intended content. A ranking without those controls would be spurious.

The research database therefore keeps four labels separate:

| Label | Meaning |
|---|---|
| Official item or tree fact | Current live API or official documentation data |
| Observed current family | A family or archetype named in the dated forum corpus |
| Content-tagged recommendation | The guide associates the build with named content |
| Inferred candidate family | Item properties suggest the family, but it remains unbenchmarked |

All 129 source links and their provenance are stored in `research/build-database/functional-builds.json`. The full per-Mythic catalogue is in `research/current-meta-mythic-build-evidence.md` and `research/build-database/weapon-build-families.json`.

## 9. Elemental item language and recurring item patterns

Community build labels mix several concepts, so the notation must be explicit:

- A weapon's elements describe its base damage package.
- An item's elemental requirements describe which skill points are needed to equip it.
- A build requirement code records which elemental requirements appear anywhere in the equipment.
- A combat engine describes how the build produces value: heavy melee, tierstack, spell, poison, support, or another loop.

The requirement-code order is `ETWFA`: Earth, Thunder, Water, Fire, Air. `ETA Gaia melee` therefore means a Gaia melee build whose equipment uses Earth, Thunder, and Air requirements. It does not mean that Gaia deals three damage elements. A decoded current non-crafted Gaia example uses Dune Storm, Twilight-Gilded Cloak, Bull Charge, Warchief, Breezehands, Diamond Fiber Ring, Span of the Starfield, Recalcitrance, and Gaia. The package is ETA even though Gaia is an Earth wand. The same corpus contains ETA Epoch tierstack, ETWFA Singularity Arcanist, and EFA Guardian Paladin examples.

The common elemental stereotypes are statistically visible, but they are tendencies rather than rules. The reproducible sample is level 90+ Rare-or-higher equipment in the checked-in 2.2.2.0 WynnBuilder data. Multi-element items contribute to every applicable group.

| Element | End-game tendency | Measured evidence in the sample | Common build pressure |
|---|---|---|---|
| Earth | Slower, larger packets, Strength, raw main, poison | Median weapon speed rank 2, or Slow; single-element median DPS 622; 10.7% positive poison, the highest group | Heavy melee, tierstack, poison, requirement-heavy ETA packages |
| Thunder | Crit-oriented burst, steal, speed support | Single-element median DPS 604; 33.9% positive Mana Steal and 27.2% positive Life Steal | Spellsteal, rawstack, cancelstack, glass burst |
| Water | Mana, spell cost, healing and spell cycles | 39.3% positive Mana Regen, the strongest group; single-element median DPS 576.5 | Spellspam, healer, Intelligence and mixed-mana packages |
| Fire | Health, regeneration and Defence | 27.5% positive raw HPR and 15.8% positive health bonus, the strongest group | Tank, support, HPR cancellation, safe heavy melee |
| Air | Speed, mobility, Agility, range or positioning | 47.1% positive Walk Speed, the strongest group; median weapon speed rank 4, or Fast | Mobility, lootrun, fast hybrid, positional archetypes |

These patterns explain why the same supporting items recur. A solver should represent item roles directly: skill-point enabler, positive attack tier, negative-tier raw-main payload, mana sustain, HPR cancellation, spell-cost reducer, range or movement support, Major ID carrier, or defensive floor. Item rarity alone is a poor feature.

## 10. Mythic-centered building

A Mythic weapon is usually a constraint package, not an automatic upgrade. Start from its base speed, requirements, elements, rolled IDs, fixed Major ID, powder slots, penalties, and Ascension. Then select the ability path and support items that make its special condition realizable. Major IDs are fixed mechanics that can change ability topology, targets, state, or resource conversion and do not stack with themselves. What players sometimes call minor IDs are the ordinary rolled or fixed identifications such as Mana Regen, raw Main Attack, Walk Speed, Life Steal, and spell costs. Those must be evaluated using actual rolls whenever a build sits near a speed, requirement, mana, HPR, or damage threshold.

The complete current weapon roster is 11 per class:

| Class | Current Mythic weapons | Dominant observed and inferred centers |
|---|---|---|
| Archer | Az, Divzer, Epoch, Eschaton, Freedom, Grandmother, Ignis, Labyrinth, Revolution, Spring, Stratiformis | Sharpshooter range and Focus, Boltslinger hit cadence, Trapper geometry, tierstack, HPR cancellation, mobility |
| Assassin | Archangel, Architect, Cataclysm, Grimtrap, Hanafubuki, Inferno, Nirvana, Nullification, Oblivion, Vengeance, Weathered | Shadestepper Vanish and Marked state, Trickster clones, Acrobat aerial cycles, heavy melee, spellsteal, HPR cancellation |
| Mage | Fatal, Gaia, Halcyon, Lament, Monster, Pure, Quetzalcoatl, Riptide, Singularity, Trance, Warp | Arcanist Mana Bank, Riftwalker Distortion and melee, Light Bender Ophanim and healing, rainbow powder chains, teleport mobility |
| Shaman | Absolution, Aftershock, Fantasia, Fate, Hadal, Immolation, Olympic, Resonance, Sunstar, Toxoplasmosis, Transfiguration | Acolyte health economy, Ritualist masks and links, Summoner Totem and puppets, tierstack, poison, healing |
| Warrior | Alkatraz, Apocalypse, Ascendancy, Bloodbath, Collapse, Convergence, Guardian, Hero, Idol, Restitution, Thrundacrack | Fallen low-health and Blood Pact loops, Paladin support, Battle Monk movement, heavy melee, upperbash or upperscream cycles |

Several Mythics demand dedicated evaluator logic:

- Singularity is the Mage rainbow Mythic. Its 42-point five-stat requirements, 15 powder slots, Super Slow base, and Orbital Chain make powder order, requirement feasibility, charge time, and target grouping first-class variables.
- Warp is not merely an Air DPS wand. Its 125 Agility requirement, extreme Walk Speed and Teleport reduction, negative Mana Regen, negative HPR, and negative healing create a mobility engine with an HPR and mana cancellation problem.
- Gaia is a Super Slow Earth wand with Earthen Splinter, poison, Main Attack percentage, and raw Main Attack damage. It can support Riftwalker melee or spell variants, but the melee search is highly threshold-sensitive.
- Epoch, Aftershock, Sunstar, Fate, Trance, and Transfiguration have direct or natural attack-tier interactions. Exact final-speed calculation is mandatory.
- Guardian, Lament, Halcyon, Absolution, Ignis, and Architect can create more party value than personal DPS. Their objective must include healing, redirection, buffs, control, and uptime.
- Toxoplasmosis has negligible ordinary weapon damage and extreme Poison. Its objective is Totem priming, Outbreak storage, Uproot release, grouping, AoE, and survivability.
- Revolution, Labyrinth, Riptide, Fate, Hanafubuki, and Architect depend strongly on distance, overlap, bounce geometry, linked targets, aerial success, or ally position.

The Mythic armour boundary is equally important. Current data contains Discoverer plus ten pairs of Mythic boots, one for each two-element pairing: Boreal, Crusade Sabatons, Dawnbreak, Galleon, Moontower, Resurgence, Revenant, Slayer, Stardew, and Warchief. There are no Mythic accessories. Dawnbreak's large negative attack tier and raw Main Attack, Slayer's positive tier, Warchief's raw and percentage Main Attack, and the HPR or mana identities of the other boots create specific engines rather than generic rarity upgrades.

Morph is a broad rainbow set and a useful generalist or starter structure. It is not synonymous with an optimized Singularity build. A rainbow Mythic still needs a content-specific engine, tree, powder order, mana plan, EHP floor, and support-item opportunity-cost analysis. `Morph Singularity` is a legal archetypal idea, not a meta conclusion.

## 11. Ability coupling and configurable acceptance profiles

The full live ability trees and Aspects are stored in `research/build-database/ability-trees.json` and `research/build-database/aspects.json`. Encoded WynnBuilder links are decoded against the 2.2.2 data where compatible, but the live API remains the authority when an effect has drifted.

| Class | Current coupling pattern | Required model state |
|---|---|---|
| Warrior | Fallen dominates damage loops; Paladin owns tank/support; Battle Monk supplies movement and hybrid paths | Low-health floor, Corrupted or Blood Pact uptime, Mantle and party state, spell sequence |
| Archer | Sharpshooter is range and precision; Boltslinger is hit cadence; Trapper is placement and overlap | Hit rate, distance, Focus, trap pathing, channel time |
| Mage | Arcanist uses Mana Bank; Riftwalker uses Distortion; Light Bender uses Ophanim and healing | Mana Bank, Distortion buildup/decay, Ophanim contact, healing output |
| Assassin | Shadestepper uses Vanish and Marked; Trickster uses clones; Acrobat uses aerial loops | Position success, Vanish dwell, clone survival, aerial uptime and landing |
| Shaman | Acolyte spends health; Summoner uses Totem and puppets; Ritualist uses masks and links | Health sacrifice, overhealth, Totem/minion uptime, mask state, linked target count |

Build acceptance should merge configurable layers: baseline, family, archetype, weapon, then activity. Thresholds are gates before optimization, not weighted penalties. Example research defaults are:

| Profile | Starting gates | Optimize after passing |
|---|---|---|
| General combat | Legal set, EHP without Agility at least 18,000, survives one configured non-dodge hit | Sustained damage, range, mobility, cost |
| Sustained spell | Non-negative mana after 30 seconds, mana trough at least 5, executable rotation | Rotation damage, mana margin, EHP |
| Heavy melee | EHP without Agility at least 20,000, Life Steal at least 0, raw HPR at least -100, Main Attack Range at least -20%, modeled hit rate at least 75% | Delivered damage, per-hit packet, Quake value, roll robustness |
| Tierstack | Exact configured final speed, normally Super Fast, plus melee survival and sustain gates | Main Attack DPS, tier robustness, raw main, sustain |
| Cancelstack | Exact speed certificate and result one tier lower, plus melee survival and sustain gates | Lexicographic speed then damage, owned-roll robustness |
| Support | EHP without Agility at least 28,000, survives configured non-dodge hit, required state uptime at least 80% | Party damage added, healing, prevented damage, uptime |
| Gaia melee | EHP without Agility at least 20,000; Life Steal at least 0; raw HPR at least -100; Main Attack Range at least -20% | Delivered melee damage, Earthen Splinter or Quake cadence, poison realization, Riftwalker uptime |

The Gaia values implement the requested starting contract and are explicitly configurable. Other content may need a higher EHP floor, a positive HPR margin, stronger range, or a lower tolerance for missed attacks. `research/build-database/threshold-profiles.json` provides 33 composable profiles rather than pretending one universal threshold set exists.

## 12. Optimization sensitivity

### 12.1 High-value exact optimization

| Family | Expected benefit | Why |
|---|---|---|
| Cancelstack | Very high | Net attack-speed threshold, raw-damage rolls, skill-point legality, and one-tier discontinuities |
| Tierstack | Very high | Same tier threshold plus slow-weapon base selection and positive-tier slot competition |
| Heavy melee | High | Raw and percentage damage, Strength, crits, powders, Quake, and survival compound around each hit |
| Rawstack | High | Raw damage is repeated at the maximum speed and slot efficiency is enumerable |
| Spellspam | Medium to high | Integer costs, repeat-cost escalation, mana margins, and rotation breakpoints |
| Spellsteal | Medium to high | Enumeration helps, but sustain also depends on hit timing, targets, and powder procs |
| Hybrids | High, simulation-dependent | Attacks and spells share time, and objectives form a broad frontier |

### 12.2 Lower-confidence static optimization

| Family | Static benefit | Missing real-world variables |
|---|---|---|
| Poison | Medium | Application, spread, uptime, target transitions, and excluded multipliers |
| Support/tank | Medium | Party composition, prevented damage, positioning, and encounter mechanics |
| Lootrun | Medium | Randomized boons, curses, rooms, movement, and long-run risk |
| Aim-heavy archetypes | Medium | Miss rate, target geometry, latency, and stack loss |
| Summons and traps | Medium | AI, pathing, placement, target access, and phase changes |

The correct use of a solver in the second group is to generate a small Pareto set for practical testing, not to declare a universal winner.

## 13. Heavy melee case study

### 13.1 Objective

Heavy melee should maximize expected delivered damage over a stated horizon, subject to requirement legality, minimum non-dodge EHP, health sustain, practical range or movement, final speed, hit probability, and powder-special assumptions.

```text
maximize E[damage delivered over H]
subject to legal equipment and skill points
           EHP_no_agi >= threshold
           recovery >= threshold
           practical target access
           explicit powder-special state
```

### 13.2 Score components

A useful heavy-melee score includes:

- Weapon base and powder-adjusted damage by element.
- Raw Main Attack damage only on compatible present or converted elements.
- Main Attack and elemental percentages.
- Strength's generic and Earth scaling.
- Dexterity expectation and critical bonus.
- Ability modifiers such as Vanish where applicable.
- Real hit frequency and expected miss rate.
- Quake, Chain Lightning, Courage, Curse, or Wind Prison with reachable uptime.
- Life Steal and powder-special steal triggers.
- Time lost to utility spells, movement, and setup.
- Non-dodge one-shot safety.

### 13.3 Why exact search succeeds

Heavy-melee pieces can provide very large raw damage while imposing severe negative tiers or requirements. Their value depends on the rest of the set, elemental compatibility, the weapon, powders, and the class state. Ranking each item independently loses these interactions. Exact enumeration with early legality bounds is therefore well suited to this family.

## 14. Cancelstack case study

Cancelstack combines raw-damage pieces with negative attack-speed tiers and positive-tier gear on a fast weapon. Its objective is naturally lexicographic:

1. Reach the required final attack-speed tier, commonly Super Fast.
2. Satisfy skill-point, EHP, sustain, and content constraints.
3. Maximize expected main-attack DPS and then secondary utility.

```text
netTier = clamp(baseWeaponTier + sum(attackSpeedTierIDs))
mainDPS = expectedAveragePerHit * hitsPerSecond(netTier)
```

A candidate one tier short is not a slightly weaker version of the same build. It can lose 38.7 percent attack frequency at the Very Fast to Super Fast boundary.

### 14.1 Roll sensitivity

The optimizer must use actual inventory rolls or report minimum required rolls. A theoretical maximum-roll set can be unusable when one attack-speed or skill-point ID rolls lower.

Recommended certificate:

- Final base and net attack-speed tier.
- Minimum tier roll on every contributing item.
- Minimum skill-point rolls by attribute.
- Minimum damage rolls assumed by the score.
- Result if any tier or skill-point roll decreases by one integer.
- Robust score at owned or conservative rolls.

### 14.2 Search implications

Cancelstack is an ideal branch-and-bound target because the attack-speed threshold creates a strong admissible bound. A partial candidate that cannot reach the required tier with all remaining positive tiers can be pruned immediately. Requirement and EHP suffix bounds can further reduce the search space before expensive damage scoring.

## 15. Solver formulation

### 15.1 Stage 1: exact legal enumeration

Enumerate equipment and relevant configuration choices while applying early filters for type, class, level, requirements, sets, ownership, Mythic and crafted policies, ability legality, Aspect slots, and powder capacity.

### 15.2 Stage 2: threshold-aware scoring

Resolve exact discrete states before applying any weighted score:

- Final attack-speed tier.
- Spell-cost floor and repeat-cast state.
- Mana feasibility.
- Archetype thresholds and caps.
- Minimum EHP, recovery, range, and walk speed.
- Required item and Aspect rolls.

Do not scalarize a failed threshold into a near-success.

### 15.3 Stage 3: rotation simulation

Simulate a user-selected horizon with cast times, delays, attacks, steal, powder charging, cooldowns, stack generation and decay, buff and debuff uptime, target count, range, AoE overlap, and hit probability.

Important state includes Focus, Corrupted, Holy Power, Discombobulated, Distortion, Mana Bank, Marks, Tricks, Momentum, masks, Blood Pool, puppets, traps, and ultimate charge.

### 15.4 Stage 4: Pareto ranking

Return non-dominated candidates across sustained damage, burst, non-dodge EHP, health and mana sustain, mobility, range, party contribution, cost, and roll robustness. Allow the user to select priorities after seeing the frontier.

### 15.5 Stage 5: in-game acceptance

Static calculation is not encounter acceptance. Finalists should be tested on a target dummy and in the intended activity. Record time-to-kill, mana floor, forced potion use, deaths, buff and archetype-state uptime, miss rate, target access, and party outcomes.

## 16. Limitations

1. The public API exposes current descriptions and requirements, but not every server-side implementation detail or bug.
2. The official 2.2.2 Wiki changelog is incomplete for ability changes.
3. Community build vocabulary is useful but not formally defined by the game and can drift between patches.
4. Static damage cannot fully model latency, aim, hitboxes, invulnerability, pathing, or minion AI.
5. Armour powder-special uptime requires encounter-state inputs.
6. Universal EHP or DPS thresholds do not exist. They depend on content, party role, and player execution.
7. Maximum-tier Aspect effects do not describe a player's owned tiers.
8. Item costs and availability change. A theoretical optimum may be economically irrelevant.
9. The current forum corpus is curated rather than controlled. Content tags are community judgments, not normalized benchmarks.
10. Local decoding succeeds for 75 of 129 links. Crafted and incompatible encodings are preserved as source URLs but excluded from decoded item-frequency statistics.

## 17. Conclusion

End-game class building is best treated as a legal configuration problem followed by content-specific optimization. The equipment set supplies a statistical foundation, but the ability path, Aspects, resource loop, rotation, target geometry, and encounter determine whether those statistics become useful output.

Exact optimization has its highest value where the system has hard breakpoints. Cancelstack and tierstack have discrete attack-speed thresholds. Heavy melee compounds raw damage, percentages, skill points, powders, and Quake around expensive individual hits. Spell cycles have integer cost and mana-feasibility boundaries. These families are strong candidates for solver-driven search.

Other families require a different standard of success. A support build should be judged by party value, a lootrun build by robust long-run performance, and an aim- or AI-dependent archetype by measured uptime. For them, the solver should expose a frontier of plausible candidates and the player should complete the decision with in-game evidence.

The durable principle is simple: define the content, model the full combat configuration, enforce thresholds before scoring, report uncertainty, and validate the finalists in the activity they are meant to solve.

## References

1. Wynncraft API documentation, Introduction: https://docs.wynncraft.com/welcome
2. Wynncraft API, Fruma Expansion v3.6 changelog: https://docs.wynncraft.com/2026-04-04-v3-6
3. Wynncraft API, ability tree endpoint: https://docs.wynncraft.com/modules/ability-aspect/get-ability-tree
4. Wynncraft API, Aspect endpoint: https://docs.wynncraft.com/modules/ability-aspect/list-aspects
5. Live ability data: `https://api.wynncraft.com/v3/ability/tree/{class}`
6. Live Aspect data: `https://api.wynncraft.com/v3/aspects/{class}`
7. Wynncraft Wiki, Version 2.2.2: https://wynncraft.wiki.gg/wiki/Version_2.2.2
8. Wynncraft Wiki, Version 2.2: https://wynncraft.wiki.gg/wiki/Version_2.2
9. Wynncraft Wiki, Ability Tree: https://wynncraft.wiki.gg/wiki/Ability_Tree
10. Wynncraft Wiki, Aspects: https://wynncraft.wiki.gg/wiki/Aspects
11. Wynncraft Wiki, Skill Points: https://wynncraft.wiki.gg/wiki/Strength
12. Wynncraft Wiki, Identifications: https://wynncraft.wiki.gg/wiki/Identifying
13. Wynncraft Wiki, Powders: https://wynncraft.wiki.gg/wiki/Powders
14. Wynncraft Wiki, Mastery Tomes: https://wynncraft.wiki.gg/wiki/Tomes
15. Wynncraft Wiki, Builds: https://wynncraft.wiki.gg/wiki/Builds
16. Wynncraft Wiki, Classes: https://wynncraft.wiki.gg/wiki/Classes
17. Wynncraft Wiki, Lootrunning: https://wynncraft.wiki.gg/wiki/Lootrunning
18. Wynncraft Wiki, Raids: https://wynncraft.wiki.gg/wiki/Raid
19. Wynncraft Wiki, Guild War: https://wynncraft.wiki.gg/wiki/Guild_War
20. Jello, *Class Building Terminology, 2.0*: https://forums.wynncraft.com/threads/2-0-class-building-terminology.305994/
21. Druser, *Class Building 101*: https://forums.wynncraft.com/threads/class-building-101.266243/
22. WynnBuilder project: https://github.com/wynnbuilder/wynnbuilder.github.io
23. Companion evidence catalogue: `research/endgame-class-building-evidence.md`
24. Current Mythic and forum-build evidence: `research/current-meta-mythic-build-evidence.md`
25. The Ultimate Build Guide: https://forums.wynncraft.com/threads/the-ultimate-build-guide.320092/
26. Official item database endpoint: https://docs.wynncraft.com/modules/item-recipe/list-items
27. Functional build research database: `research/build-database/README.md`

### Local implementation evidence

- `js/game/game_rules.js`: skill-point, level, mana, and timing constants.
- `js/game/build_utils.js`: skill-point conversion and attack-speed multipliers.
- `js/game/damage_calc.js`: damage pipeline.
- `js/game/shared_game_stats.js`: spell costs and EHP.
- `js/builder/mana_calc.js`: cycle mana model.
- `js/game/skillpoints.js`: requirement-feasibility search.
