# Wynncraft end-game class building: research evidence

Research date: 13 August 2026, Australia/Sydney
Current game baseline: Wynncraft 2.2.2, released 10 July 2026
Official API observed: v3.7.2, response date `Wed, 12 Aug 2026 23:56:06 GMT`, one-hour cache

## Executive findings

1. A useful end-game build is not only eight equipment pieces and a weapon. It is a complete combat configuration: gear and rolls, skill-point assignment, weapon and armour powders, 14 Mastery Tome slots, ability-tree path, five Aspect slots, Major IDs, offhand utility choices, consumables where allowed, spell or attack rotation, and the target content.
2. There is no single meaningful global "best build" objective. Raid support, solo bosses, lootrun longevity, guild wars, and short damage tests place different values on burst, sustained damage, range, area coverage, mobility, EHP, healing, mana stability, and party buffs.
3. Exact combinatorial optimization matters most when the build has hard thresholds or discontinuities. Cancelstack and tierstack are the clearest cases because a single attack-speed tier changes the final hits-per-second tier. Heavy melee also benefits strongly because raw and percent main-attack damage, Strength, Dexterity, powder conversion, and Quake compound around a very slow high-damage hit.
4. Static optimization is less decisive for builds whose real output depends on aim, uptime, encounter geometry, party composition, randomized lootrun boons, or a stateful ability loop. For those builds the best static score should be treated as a candidate generator, then evaluated by rotation simulation and practical acceptance tests.
5. Ability trees are not five isolated presets. Each class has three archetypes, but a legal path may mix them. Nodes have connectivity, AP, prerequisite, minimum-archetype, and mutual-lock constraints. The current cap is 50 AP at combat level 120.
6. Aspects are ability modifiers, not generic gear stats. A slot used on an Aspect for an ability absent from the selected tree has no build value. Five slots exist, only one Mythic Aspect may be equipped, and current maximum copy thresholds are 150 for Legendary Tier IV, 75 for Fabled Tier III, and 15 for Mythic Tier III.
7. The checked-in `data/2.2.2.0/aspects.json` matches the official API in Aspect names, counts, rarity, and tier thresholds for all five classes, but at least one effect has drifted. `Aspect of a Thousand Hours` is +1 AoE locally and +3 AoE in the live API. Current public API data should therefore be authoritative for user-facing output.

## Authority and patch-sensitivity

Source precedence used here:

1. Live official Wynncraft API responses and official API documentation.
2. Current official Wynncraft Wiki pages, which are community-maintained but officially hosted and have recent 2026 revisions.
3. Current implementation source in this repository, which is a community calculator and solver. It is valuable for formulas and solver semantics, but must be checked against live data.
4. Wynncraft forum terminology posts and guides. These establish community vocabulary but are highly patch-sensitive.

Important current-version qualification:

- The official Wiki identifies 2.2.2 as the latest update and explicitly says that ability changes still need to be added to its 2.2.2 version page. Use the live ability and Aspect API for current tree facts.
- The Fruma Expansion, 2.2.0, raised end-game content to level 120, added ultimates, over one thousand items, 29 Major IDs, ten Mythic items, Tier VII powders, ascended items, and removed the need for players to manually equip armour in a particular order.
- The official API Fruma changelog states that ability endpoints were updated for ultimates.

Primary URLs:

- Current version: https://wynncraft.wiki.gg/wiki/Version_2.2.2
- Fruma item and end-game changes: https://wynncraft.wiki.gg/wiki/Version_2.2
- Official API introduction: https://docs.wynncraft.com/welcome
- Official API Fruma update: https://docs.wynncraft.com/2026-04-04-v3-6
- Official ability-tree endpoint: https://docs.wynncraft.com/modules/ability-aspect/get-ability-tree
- Official Aspect endpoint: https://docs.wynncraft.com/modules/ability-aspect/list-aspects

## What constitutes an end-game build

### Loadout and configuration

| Layer | Choices and constraints | Why it matters |
|---|---|---|
| Weapon | Class-specific bow, spear, wand, dagger, or relik; base damage by element; base attack speed; powder slots; requirements; IDs; Major ID; possible ascension | Establishes base DPS or per-hit damage, spell base, available powder special, and often the playstyle |
| Armour | Helmet, chestplate, leggings, boots | Provides health, elemental defence, requirements, damage and sustain IDs, attack-speed tiers, set bonuses, and armour powder specials |
| Accessories | Two rings, one bracelet, one necklace | Often close skill-point gaps or add efficient damage, mana, attack speed, and utility |
| Skill points | 200 assignable total; at most 100 manually in one skill; items can raise final values to 150 | Must satisfy gear requirements and determine damage, crit chance, mana, damage reduction, dodge, and elemental scaling |
| Powders | Ordered weapon powders; armour powders; Tier IV or higher pairs unlock one special per item | Convert neutral damage, add elemental damage or defence, and introduce powerful multiplicative or stateful specials |
| Mastery Tomes | 14 slots: four Armour, two Weapon, two Marathon, two Expertise, two Mysticism, one Lootrunning, one Guild | Add health, damage, movement, mana, spell-cost, loot, and up to +4 or +5 assignable SP through the Guild Tome |
| Ability tree | Up to 50 AP; connected path; prerequisites; archetype minimums; locks; three ultimates per class | Defines the actual spell kit, multipliers, resource loops, support, mobility, and damage state |
| Aspects | Five slots; level and Sentinel III unlocks; one Mythic maximum; ability-dependent effects | Modifies selected abilities, often changing cooldowns, caps, hit counts, range, or archetype resource throughput |
| Item rolls | Positive IDs normally roll over a 30 to 130 percent base range; negative IDs over 70 to 130 percent, with rounding; fixed IDs and Major IDs are exceptions | Can change legality, mana thresholds, attack-speed outcomes, and damage substantially |
| Rotation and state | Main attacks, spell order, repeat-cost escalation, buffs, debuffs, archetype resource stacks, powder-special uptime | Static sheet damage is not sustained encounter output |
| Content assumptions | Solo or party, boss or waves, time horizon, range, incoming damage, crowd control, consumables, offhands, raid buffs, lootrun boons | Determines the correct objective and constraints |

Official support:

- Armour has four pieces: https://wynncraft.wiki.gg/wiki/Armour
- Accessories have two ring slots, one bracelet, and one necklace: https://wynncraft.wiki.gg/wiki/Accessories
- There are 14 Tome slots and seven Tome types: https://wynncraft.wiki.gg/wiki/Tomes
- Ability-tree structure and AP: https://wynncraft.wiki.gg/wiki/Ability_Tree
- Aspect slots, rarity rules, and upgrading: https://wynncraft.wiki.gg/wiki/Aspects
- Identification mechanics: https://wynncraft.wiki.gg/wiki/Identifying

### Legality and feasibility checks

A solver or human builder should reject a candidate unless all of these pass:

1. Correct item type and class weapon.
2. Combat level, quest, class, and skill-point requirements.
3. Total manually assigned SP within the applicable budget, normally 200 before a Guild Tome increase, and no more than 100 manually in any attribute.
4. Equipment-granted SP handled according to current requirement rules. Weapon and crafted-item SP do not contribute to other gear requirements. Since 2.2.0 the player no longer needs to manually equip armour in a particular order, but a calculator still needs to prove the resulting set of requirement and bonus relationships is legal.
5. A connected ability path within AP, prerequisite, archetype-count, and lock constraints.
6. No more than five Aspects and no more than one Mythic Aspect. Every selected Aspect should modify a selected ability or an intentional universal function.
7. Powder slots and powder order are legal. Only one powder special can exist on an item.
8. If crafted equipment is allowed, durability and repair assumptions are explicit. Crafted equipment loses ID strength below 50 percent durability and falls to 10 percent at zero durability.
9. The intended rotation is mana-feasible after repeat-cast escalation and encounter downtime, not only at the displayed first-cast cost.

## Core formulas and discontinuities

### Skill-point scaling

The current official Skill Points page states:

- 200 points are earned by level 101; levels 102 through 121 grant none.
- Manual cap per attribute is 100; item bonuses can raise a final attribute to 150.
- Strength and Dexterity scale from 0 to 80.8 percent at 150.
- Intelligence grants Water damage and maximum mana on that same scale, but spell-cost reduction reaches 50 percent at 150.
- Defence reaches 70.0 percent reduction at 150.
- Agility reaches a 76.8 percent dodge chance at 150, and a dodge reduces a hit by 90 percent.
- Strength and Dexterity combine additively for critical-hit damage rather than multiplying each other.

Source: https://wynncraft.wiki.gg/wiki/Strength, last edited in 2026.

The local builder uses the exact geometric conversion:

```text
p(s) = [r / (1-r)] * [1 - r^s] / 100, r = 0.9908, 0 <= s <= 150
```

Then it applies final multipliers by attribute. See `js/game/build_utils.js:6`, `js/game/game_rules.js:7`, and `js/game/game_rules.js:11`.

### Attack speed

Attack speed is a seven-level ladder. Each net tier moves one step, clamped to the endpoints.

| Final tier | Hits per second |
|---|---:|
| Super Slow | 0.51 |
| Very Slow | 0.83 |
| Slow | 1.50 |
| Normal | 2.05 |
| Fast | 2.50 |
| Very Fast | 3.10 |
| Super Fast | 4.30 |

Source: https://wynncraft.wiki.gg/wiki/Identifying, revised July 2026. The same values are implemented as `baseDamageMultiplier` in `js/game/build_utils.js`.

This is the key cancelstack discontinuity. Moving from Very Fast to Super Fast is a 38.7 percent increase in attack frequency before any other change. A candidate one tier short can lose to an otherwise similar candidate by a large margin.

### Damage pipeline

The current WynnBuilder implementation in `js/game/damage_calc.js:31` performs this high-level sequence:

1. Read neutral and five elemental weapon damage ranges, including powders.
2. Apply ability neutral and elemental conversions.
3. Apply attack-speed multiplier for DPS-based spell calculations, but not for a single main-attack hit.
4. Add elemental additive weapon damage.
5. Apply generic, attack-type, elemental, and skill-point percent bonuses.
6. Distribute proportional and rainbow raw damage, and apply element-specific raw damage where the element is present.
7. Apply Strength and other multiplicative damage effects.
8. Produce normal and critical ranges. Average damage weights them using the Dexterity crit chance.

Important consequences:

- Percent damage scales weapon base damage and does not scale raw damage IDs.
- Raw damage is multiplied by the relevant attack or spell multiplier, but only contributes where the conversion type matches or neutral damage is present.
- Strength applies after other modifiers, and critical damage adds its critical increment to the Strength-boosted result rather than multiplying Strength again.
- Ability-specific and Major-ID multipliers can be mutually multiplicative, so a single "damage percent" sum is not sufficient.

Official identification descriptions: https://wynncraft.wiki.gg/wiki/Identifying
Local implementation: `js/game/damage_calc.js:31-246`

### Mana and spell cost

The local current formula in `js/game/shared_game_stats.js:58-75` is:

```text
baseAfterInt = baseCost * (1 - intelligenceReduction)
afterRaw     = baseAfterInt + rawSpellCost
afterPercent = afterRaw * (1 + percentSpellCost / 100)
finalCost    = max(1, afterPercent * (1 + finalPercentModifier / 100))
```

The official Identifications page additionally states that repeating the same spell quickly increases its cost and that the normal absolute minimum is 1 mana, except for specified Major-ID cases.

The builder mana panel models baseline gain as 5 mana per second plus `Mana Regen / 5` and, when enabled, `Mana Steal / 3`, then compares it with an explicit cycle. See `js/builder/mana_calc.js:43-123`.

### Mana and life steal

Mana Steal and Life Steal are normalized as per-three-second values under continuous attacks. Slower speeds receive a larger fraction per hit. For ordinary single-hit main attacks, the official current fractions are:

| Attack speed | Fraction per hit |
|---|---:|
| Super Slow | 65.4% |
| Very Slow | 40.2% |
| Slow | 22.2% |
| Normal | 16.3% |
| Fast | 13.3% |
| Very Fast | 10.7% |
| Super Fast | 7.8% |

Multi-projectile class attacks divide this fraction among their hits. Powder specials also trigger steal, including each Quake or Courage target and each Chain Lightning chain. Source: https://wynncraft.wiki.gg/wiki/Identifying.

### EHP and recovery

The current local implementation defines:

```text
totalHP = base and armour HP + raw Health
classDamageFactor = 2 - classResistance
EHP_no_agi = totalHP / [(1 - defenceReduction) * classDamageFactor * otherDefenceMultipliers]
```

The Agility-aware value treats a dodge as taking 10 percent of the hit and weights dodged and non-dodged cases. See `js/game/shared_game_stats.js:84-126`.

Base resistance and relative incoming damage:

| Class | Base resistance | Relative damage taken |
|---|---:|---:|
| Warrior | 100% | 1.00x |
| Assassin | 100% | 1.00x |
| Mage | 80% | 1.20x |
| Archer | 70% | 1.30x |
| Shaman | 60% | 1.40x |

Source: https://wynncraft.wiki.gg/wiki/Classes and current class pages.

Report both EHP with and without Agility. Community requests often mean no-Agility EHP because dodge is probabilistic and one-shot risk depends on non-dodged hits.

### Powders

Current 2.2 rules include seven tiers. Weapon powder order affects neutral conversion and which special is created. Two same-element Tier IV or higher powders unlock one special.

| Element | Weapon special | Build consequence |
|---|---|---|
| Earth | Quake | 240 to 480 percent Main Attack damage in an AoE; scales strongly with heavy melee and triggers steal per target |
| Thunder | Chain Lightning | 200 to 350 percent Main Attack damage and 5 to 11 chains; good wave clear and multi-target steal |
| Water | Curse | Target takes 10 to 25 percent more damage for 4 seconds; party and burst amplifier |
| Fire | Courage | 110 to 200 percent Main Attack flare plus 10 to 25 percent damage boost for 4 seconds; party and self amplifier |
| Air | Wind Prison | Holds nearby mobs and multiplies the next hit by 100 to 250 percent; suited to a planned single burst |

Attack-speed-specific charge counts are scaled so each special takes about five seconds of continuous main attacking. Super Slow requires 3 hits and Super Fast 22 hits. Source: https://wynncraft.wiki.gg/wiki/Powders, revised July 2026.

Armour specials are encounter-dependent: Rage rewards missing health, Kill Streak requires kills, Concentration rewards mana spending, Endurance requires taking hits, and Dodge requires nearby enemies without being hit. An optimizer must not grant their caps unconditionally.

## Build taxonomy

The official Wiki's Builds page now records community terminology, but it labels itself incomplete. The 2.0 forum terminology guide is useful corroboration and is patch-sensitive.

| Type | Core loop | Primary optimization | Critical constraints and failure modes |
|---|---|---|---|
| Spellspam | Cast a repeatable spell cycle using Mana Regen and low costs | Sustained cycle DPS, mana margin, relevant spell damage, EHP | Repeat-cost escalation, cast time, range, missed hits, and forced movement |
| Spellsteal | Weave main attacks into a spell cycle to proc Mana Steal | Spell-cycle DPS subject to attack windows and mana probability | Slower attack speed gives more steal per hit; target count and powder procs change sustain |
| Mixed mana | Combine Mana Regen and Mana Steal | Robust mana across downtime and contact phases | May pay opportunity cost for redundant sustain |
| Heavy melee | Very slow, high-damage main attacks; spells mainly for utility | Per-hit damage, real hit rate, Strength/Dexterity, Quake, life sustain | Missed hits are expensive; close range, negative attack speed, and one-shot safety matter |
| Tierstack | Start with a slow or Super Slow high-base weapon and add positive attack-speed tiers | Super Fast DPS while preserving raw/percent damage and survival | Final speed is a hard threshold; tier items consume slots and SP |
| Cancelstack | Combine high raw main-attack armour carrying negative tiers with positive-tier gear and a fast weapon | Net tier threshold plus maximum raw-stacked DPS | One tier short causes a discrete DPS loss; roll quality and requirements can change feasibility |
| Rawstack | Use an already-fast weapon and add raw main-attack damage without major tier manipulation | Raw damage times 4.3 hits per second, then EHP/sustain | Raw damage does not benefit from percent-damage IDs in the same way as weapon base |
| Fast hybrid | Both fast main attacks and spells contribute | Combined rotation DPS and mana stability | Static melee DPS plus spell DPS double-counts time unless a rotation model is used |
| Heavy hybrid | Super Slow main hit plus spell damage | Burst packet and spell cycle between hits | Competes for both main and spell IDs, often producing a wider Pareto frontier |
| Poison | Apply a three-second damage-over-time value, often with AoE or piercing delivery | Reliable application uptime, Poison, survival, and applicable debuffs | Poison cannot crit and is not affected by Strength; many general damage boosts exclude it |
| Support/tank | Buff, heal, control, or absorb damage for a group | Party effective DPS, healing throughput, uptime, EHP, resurrections, control | Personal sheet DPS is the wrong objective |
| Lootrun | Clear randomized, escalating rooms while moving quickly and surviving curses | Robust clear speed, AoE, mobility, self-sustain, long-run scaling | Boons and curses create state uncertainty; pure dummy DPS is brittle |
| Guild war | Kill a tower while absorbing unavoidable attacks | Sustained boss DPS, non-dodge EHP, healing and life sustain | Tower damage is constant and directed at the nearest player; burst-only builds can fail |

Sources:

- Current terminology and build types: https://wynncraft.wiki.gg/wiki/Builds
- 2.0 terminology guide, 8 December 2022: https://forums.wynncraft.com/threads/2-0-class-building-terminology.305994/
- Historical Class Building 101, 14 April 2020: https://forums.wynncraft.com/threads/class-building-101.266243/
- Current lootrun rules: https://wynncraft.wiki.gg/wiki/Lootrunning
- Current raid rules: https://wynncraft.wiki.gg/wiki/Raid
- Current guild-war rules: https://wynncraft.wiki.gg/wiki/Guild_War

## Ability-tree concepts and all class archetypes

All five classes have three archetypes, but players may select mixed legal paths. Official API responses on 13 August 2026 contained 97 Archer nodes, 96 Warrior nodes, 96 Assassin nodes, 88 Mage nodes, and 93 Shaman nodes, with three ultimate nodes per class.

Shared tree rules:

- A node can only be selected from a connected prior node, and traversal cannot go back upward to obtain disconnected branches.
- Requirements can include AP, a prerequisite ability, a minimum count in an archetype, or a combination.
- Locks make certain choices mutually exclusive.
- Node colours communicate function and AP cost. Current ultimate nodes cost 2 AP and are the final branch node for each archetype.
- Maximum AP is 50 at combat level 120.

Official sources: https://wynncraft.wiki.gg/wiki/Ability_Tree and https://docs.wynncraft.com/modules/ability-aspect/get-ability-tree.

| Class | Base kit and resistance | Archetype | Official identity | What a build should align |
|---|---|---|---|---|
| Archer | Bow; Arrow Storm, Escape, Arrow Bomb, Arrow Shield; 70% | Boltslinger | Close-range torrents of hits, speed, burst, Guardian Angels | Hit count, close-range uptime, projectile spread/range, Frenzy, Guardian Angel ammo and rate |
| Archer |  | Sharpshooter | Aimed power shots and range; Focus rewards not missing | Focus uptime, aim realism, single-target range, power-shot cadence, miss penalty |
| Archer |  | Trapper | Delayed traps, beasts, crowd control, Mana Trap sustain | Trap cap, enemy pathing, AoE, delayed damage, snare/control, beast uptime |
| Warrior | Spear; Bash, Charge, Uppercut, War Scream; 100% | Fallen | High-risk damage that grows under attack and low health; Corrupted state blocks healing | Corruption and low-health uptime, Blood Pact, recovery exit, Rage/Endurance assumptions |
| Warrior |  | Battle Monk | Swift close combat, spell combos, movement, knockback | Spell cadence, close-range uptime, movement safety, Discombobulated and Pressure stacks |
| Warrior |  | Paladin | Resistance, ally support, Mantle, provoke, self-revival | Non-dodge EHP, Mantle charges, Holy Power, party resistance and buff uptime |
| Assassin | Dagger; Spin/Lacerate, Dash, Multihit/Backstab, Smoke Bomb; 100% | Shadestepper | Stealth glass cannon and timed burst | Vanish/Satsujin/Marks timing, back angle, cooldowns, one-shot packet, exit safety |
| Assassin |  | Trickster | Clones, deception, crowd control and debuffs | Clone count and death triggers, Tricks/debuff cap, target density, Last Laugh copies |
| Assassin |  | Acrobat | Agile aerial combo fighter | Momentum, aerial uptime, Lacerate hits, movement control, close-range risk |
| Mage | Wand; Heal, Teleport, Meteor, Ice Snake; 80% | Riftwalker | Winded stacking, mobility, long-fight ramp and Timelock | Time-to-ramp, Winded uptime, boss duration, Distortion, reset risk |
| Mage |  | Light Bender | Healing support with offensive Ophanim/Lightweaver orbs | Healing-to-proc thresholds, orb count/health, ally buffs, healing efficiency, range |
| Mage |  | Arcanist | Trades Heal for Mana Bank and free destructive cycles | Mana Bank generation, Arcane Transfer timing, Chaos Explosion threshold, no-heal risk |
| Shaman | Relik; Totem, Haul, Aura, Uproot/Switch Masks; 60% | Summoner | Totems and minions overwhelm enemies | Puppet/effigy counts, summon rate, multi-totem penalties, AI/target uptime, Commander |
| Shaman |  | Ritualist | Masks, chants, state switching and versatility | Mask cycle, mana saved to Awakened, attached-mask downtime, party buff uptime |
| Shaman |  | Acolyte | Sacrifices health for damage, healing and support through Blood Pool | Blood Pool generation/spend, self-damage, healing recovery, overhealth, tether uptime |

Class-page sources, revised in 2026:

- Archer: https://wynncraft.wiki.gg/wiki/Archer
- Warrior: https://wynncraft.wiki.gg/wiki/Warrior
- Assassin: https://wynncraft.wiki.gg/wiki/Assassin
- Mage: https://wynncraft.wiki.gg/wiki/Mage
- Shaman: https://wynncraft.wiki.gg/wiki/Shaman

## Current Aspect system

Official mechanics:

- Aspects were added in 2.1 and are obtained from Raid reward pulls.
- They are account-wide and do not become unavailable when used by another character.
- Each character has five slots. Four unlock at combat levels 20, 40, 60, and at level 80 with Sentinel III Raid Syndicate progress; the remaining slot is available by default.
- Legendary Aspects have four tiers and tend to improve generic abilities.
- Fabled Aspects have three tiers and tend to improve archetype-focused abilities.
- Mythic Aspects have three tiers, make major archetype changes, and only one Mythic may be equipped on a character.
- Current cumulative thresholds from the live API are 1, 5, 30, 150 for Legendary; 1, 15, 75 for Fabled; and 1, 5, 15 for Mythic. This corresponds to additional duplicate counts of 4, 25, 120; 14, 60; and 4, 10.

Source: https://wynncraft.wiki.gg/wiki/Aspects and live official API URLs below.

The tables record the maximum-tier effect from the live API on 13 August 2026. They are an evidence catalogue, not a recommendation. Select only effects that are active in the chosen ability path and useful for the intended encounter.

### Archer Aspects, 25 total

Source: https://api.wynncraft.com/v3/aspects/archer

| Aspect | Rarity | Max effect |
|---|---|---|
| Aspect of the Iron String | Legendary | Phasing Beam pierces +4 enemies |
| Aspect of Bullet Hell | Legendary | Shrapnel Bomb gains +22 shrapnel |
| Aspect of Further Horizons | Legendary | Arrow Storm has 34% less spread; Phantom Ray gains +6 range |
| Aspect of Extreme Firepower | Legendary | Arrow Bomb gains +1.5 AoE |
| Aspect of Battlement Fortification | Legendary | Arrow Shield gains +10% resistance |
| Aspect of Dynamic Entry | Legendary | Fierce Stomp gains +4 AoE |
| Aspect of the Thunderbolt | Legendary | Arrow Bomb travels 35% faster |
| Aspect of the North Wind | Legendary | Snow Storm gains +3 range |
| Aspect of Olfactorial Enhancement | Legendary | Sniffer Dog cooldown is reduced by 0.4 seconds |
| Aspect of Clinging Lichen | Legendary | Bryophyte Roots gains +1.5 AoE |
| Aspect of the Barley-Woven | Legendary | Grappling Hook range increases by 30% |
| Aspect of the Battle-Fletcher | Fabled | Maximum Flint Arrows +3, with -3% Main Attack damage per arrow |
| Aspect of Extreme Current | Fabled | Coursing Restraints gains +2 seconds duration |
| Aspect of Undercrank | Fabled | Frenzy builds speed 5% faster and its cap increases by 20% |
| Aspect of the Poltergeist | Fabled | Phantom Ray gains +0.4 seconds duration |
| Aspect of the Beastmaster | Fabled | Hound attack speed +25% and movement +50%; Snake weakness +4%; Crow distraction +0.2 seconds |
| Aspect of Fragmentation Rounds | Fabled | Grape Bomb gains +1 grape and +45% DPS damage |
| Aspect of the Steadying Hand | Fabled | Ghostly Trigger cooldown reduced by 2.5 seconds |
| Aspect of the Heavenly Mandate | Fabled | Guardian Angels gain +9 ammunition |
| Aspect of Chaotic Demolition | Fabled | Maximum Traps +4 |
| Aspect of the Inexhaustible Quiver | Fabled | Arrow Storm gains +4 arrows per stream |
| Archer's Embodiment of Perceptive Finesse | Mythic | Ultimate charge +25%; Ascension +20% duration; Extinction Event +30% bombs; Twilight Zone mark cap +20% |
| Sharpshooter's Embodiment of Laser Precision | Mythic | Focus cooldown reduced by 0.3 seconds and maximum Focus +1 |
| Boltslinger's Embodiment of Rended Skies | Mythic | Guardian Angels fire 1.8x faster under Divine Intervention, explosive shots +10%, Triple Shots +2 arrows, Main Attack -15% |
| Trapper's Embodiment of Persistence Predation | Mythic | Patient Hunter +15% damage per second and Chilling Snare +3 seconds |

### Warrior Aspects, 27 total

Source: https://api.wynncraft.com/v3/aspects/warrior

| Aspect | Rarity | Max effect |
|---|---|---|
| Aspect of the Humming Choir | Legendary | Devout Union gains +4 AoE |
| Aspect of the Anvil Drop | Legendary | Flyby Slam landing, Flying Kick and Collide gain +2 AoE |
| Aspect of Maniacal Frisson | Legendary | Exhilarate restores +5% of Corrupted bar as max health |
| Aspect of the Crimson Scrivener | Legendary | Bloodied Armory angle +40 degrees |
| Aspect of the Returning Javelin | Legendary | Main Attack and Bloodied Armory gain +2 range |
| Aspect of Overflowing Hope | Legendary | Sparkling Hope needs 2.5% less healing to activate |
| Aspect of Steel Chords | Legendary | War Scream, Tempest and Bloodlust gain +4 AoE |
| Aspect of the Golden Dawn | Legendary | Luster Purge grants +10% Main Attack damage and +10% range |
| Aspect of Skyward Strikes | Legendary | Uppercut gains +2 range |
| Aspect of Deafening Echoes | Legendary | Tempest hit delay reduced by 0.3 seconds |
| Aspect of Turbulence | Legendary | Cyclone gains +3 AoE |
| Aspect of Earthshaking | Legendary | Bash gains +1.5 AoE |
| Aspect of Bovine Inspiration | Legendary | Emboldening Cry grants allies +5% resistance |
| Aspect of the Megaphone | Legendary | Air Shout gains +2 AoE |
| Aspect of the Berserker | Fabled | Armour Breaker grants self +6% damage bonus |
| Aspect of the Enforcer | Fabled | Damage per Harmony +1.2% |
| Aspect of Unquenching Flames | Fabled | Boiling Blood cooldown reduced by 1.5 seconds |
| Aspect of Empowering Fantasy | Fabled | Radiance gains +2 seconds duration |
| Aspect of the Tightrope Walk | Fabled | Brink of Madness activates at 8% higher health |
| Aspect of Hyper-Perception | Fabled | Counter cooldown reduced by 0.5 seconds |
| Aspect of Rallying Fervor | Fabled | Sacred Surge gains +0.2 Holy Power per triggered spell/ability; Luster Purge grants +2.5 Holy Power |
| Aspect of Rekindling | Fabled | Rejuvenating Skin restores health 5 seconds faster |
| Aspect of Seeing Stars | Fabled | Maximum Discombobulated +50 |
| Fallen's Embodiment of Blind Fury | Mythic | Blood Pact health cost -15%; health-cast damage increase +6% to +10% |
| Paladin's Embodiment of Undying Determination | Mythic | Mantle +3 charges; Sacred Surge costs 5 less Holy Power |
| Battle Monk's Embodiment of Complete Synchrony | Mythic | Stronger Pressure, Bash, Uppercut and War Scream, including +120% Uppercut damage and +4% player damage buff |
| Warrior's Embodiment of Everlasting Perseverance | Mythic | Ultimate charge +25%; Beyond Salvation +3 spikes; Zenith Stance +3 seconds; Judgement +20% duration |

### Assassin Aspects, 26 total

Source: https://api.wynncraft.com/v3/aspects/assassin

| Aspect | Rarity | Max effect |
|---|---|---|
| Aspect of Athleticism | Legendary | Dash 25% stronger; Shadow Projection launches 15% farther |
| Aspect of the Stellar Flurry | Legendary | Multihit gains +4 hits |
| Aspect of Shadow Armor | Legendary | Dissolution gains +0.5 seconds duration |
| Aspect of the Pinwheel | Legendary | Spin Attack and Lacerate gain +1.5 AoE |
| Aspect of Enduring Illusions | Legendary | Mirror Clones reduce damage by another 6% |
| Aspect of Redoublement | Legendary | Multihit +1.5 range; Backstab +3 range |
| Aspect of Flamboyance | Legendary | Bamboozle gains +3 AoE |
| Aspect of the Chain Knife | Legendary | Main Attack gains +2 range |
| Aspect of the Fog Machine | Legendary | Smoke Bomb gains +2 AoE |
| Aspect of the Airborne | Fabled | Jasmine Bloom gains +0.8 AoE per bloom |
| Aspect of Seeking Stars | Fabled | Ricochets gains +3 bounces |
| Aspect of Unyielding Fate | Fabled | Backstab double-damage angle +50 degrees |
| Aspect of the Agile Blade | Fabled | Aerial Ace gains +2.5 seconds duration |
| Aspect of Visual Distortion | Fabled | Mirage cooldown reduced by 0.5 seconds |
| Aspect of Sleight-Of-Hand | Fabled | Shuriken preparation +3 and capacity +9, with -36% damage per shuriken packet |
| Aspect of the Unstoppable Force | Fabled | Aerial Ace gains +10% damage bonus |
| Aspect of the Pernicious Prankster | Fabled | Store +25 more Tricks on enemies |
| Aspect of False Coercing | Fabled | Sandbagging threshold +10% max health |
| Aspect of the Calling Card | Fabled | Maximum Marks +3, damage per Mark -1.5% |
| Aspect of the Dagger's Silhouette | Fabled | Maximum Knives -2, each Knife +14% damage |
| Aspect of Clouded Vision | Fabled | Smoke Bomb duration +30% |
| Aspect of the Disappearing Act | Fabled | Shadow Projection cooldown reduced by 2 seconds |
| Assassin's Embodiment of Otherworldly Detachment | Mythic | Ultimate charge +25%; Pierce the Veil +4 seconds; Another Self +30% health; Serpent's Garden +4 seconds |
| Trickster's Embodiment of Malevolent Mischief | Mythic | Noxious Haze benefits from +1 debuff and +1% per debuff; Mirror Image gains +1 clone |
| Shadestepper's Embodiment of Unseen Execution | Mythic | Vanish cooldown -1.75 seconds and Satsujin cooldown -5 seconds |
| Acrobat's Embodiment of Gravity Defiance | Mythic | Lacerate +3 hits and 50% faster; Momentum takes 3 fewer hits per stack |

### Mage Aspects, 25 total

Source: https://api.wynncraft.com/v3/aspects/mage

| Aspect | Rarity | Max effect |
|---|---|---|
| Aspect of a Thousand Hours | Legendary | Time Dilation gains +3 AoE |
| Aspect of the Vast Emptiness | Legendary | Vacuokinesis gains +20% damage from Distortion |
| Aspect of the Savior | Legendary | Heal gains +3 range |
| Aspect of Indoctrination | Legendary | Gospel of Light gains +8 AoE |
| Aspect of the Magic Missile | Legendary | Psychokinesis gains +12 range |
| Aspect of the Comet | Legendary | Meteor and Astral Fragmentation +2 AoE; Ophanim +8 range and 37% faster orbs |
| Aspect of a Scorching Sun | Legendary | Pyrokinesis gains +2 AoE |
| Aspect of the Ray of Frost | Legendary | Ice Snake and Frozen Tornado gain +2 AoE |
| Aspect of Wind Walking | Legendary | Arcane Speed gains +20% speed |
| Aspect of the Apprentice's Bolt | Legendary | Main Attack gains +5 range |
| Aspect of the Dimension's Door | Legendary | Teleport +6 range; Displacement +15 range |
| Aspect of Runic Extravagance | Fabled | Burning and Freezing Sigils gain +3 AoE |
| Aspect of Limitless Knowledge | Fabled | Maximum Arcane Overflow Mana +33% |
| Aspect of Futures Rewritten | Fabled | Paradox delays 5% more damage and refunds 5% more mana |
| Aspect of the Inescapable Void | Fabled | Dimensional Tear and Rift Rupture +3 AoE; Dimensional Tear +5% resistance |
| Aspect of Shining Status | Fabled | Lightweaver needs 20% less healing to activate |
| Aspect of Manaflux | Fabled | Manastorm needs 20 less mana to activate |
| Aspect of the Forbidden Ritual | Fabled | Judrajim cooldown reduced by 1.5 seconds |
| Aspect of Burning Providence | Fabled | Sunflare needs 50% less healing to activate |
| Aspect of Mystic Transfer | Fabled | Fortitude gains +3 seconds duration |
| Aspect of the Indescribable | Fabled | Riftspawn attack speed +40% and vision +3 |
| Light Bender's Embodiment of Celestial Brilliance | Mythic | Lightweaver +3 orbs and Ophanim +1 orb |
| Mage's Embodiment of Morbid Curiosity | Mythic | Ultimate charge +25%; Dawn +30% wisps; Gravitational Collapse +2.5 seconds; Tangled Origin +15% attack speed |
| Riftwalker's Embodiment of Reality Alteration | Mythic | Distortion decays 15% slower; Riftbound +2 seconds |
| Arcanist's Embodiment of Total Obliteration | Mythic | Chaos Explosion threshold -20 mana, casts 14% faster, maximum Mana Bank +25 |

### Shaman Aspects, 25 total

Source: https://api.wynncraft.com/v3/aspects/shaman

| Aspect | Rarity | Max effect |
|---|---|---|
| Aspect of the Alraune's Roots | Legendary | Uproot and Haunting Memory gain +6 range |
| Aspect of Summer Storms | Legendary | Storm Dance pull strength +25% |
| Aspect of the Monolith | Legendary | Totem gains +30 seconds duration |
| Aspect of Acceleration | Legendary | Puppets and Effigies move 25% faster |
| Aspect of Reverberation | Legendary | Totemic Smash gains +4 AoE |
| Aspect of Incineration | Legendary | Friendly Fire reduces Puppet duration by another 1 second |
| Aspect of Surging Presence | Legendary | Aura travels 0.3 seconds faster |
| Aspect of Emanant Force | Legendary | Totem and Eldritch Call gain +3 AoE |
| Aspect of Gushing Blood | Legendary | Lashing Lance inflicts +0.3 seconds Bleeding per hit |
| Aspect of Empathy | Legendary | Twisted Tether needs 0.5% less health loss to activate |
| Aspect of the Blurred Line | Legendary | Corporeal Manifestation spins 65% faster |
| Aspect of Motivation | Legendary | Bullwhip grants summons +10% damage bonus |
| Aspect of Occupation | Legendary | Hummingbirds attack +10 seconds and gain +8 vision |
| Aspect of the Bodyguard | Legendary | Fortified Formation grants +20% resistance |
| Aspect of the Beckoned Legion | Fabled | Puppet Master +3 Puppets with -6% total damage |
| Aspect of Stances | Fabled | Mystic Masks bonus effect multiplied by 1.25 |
| Aspect of the Artisan | Fabled | Crystal Knives gains +4 pierce |
| Aspect of Seismic Sense | Fabled | Nature's Jolt gains +3 AoE |
| Aspect of Exsanguination | Fabled | Maximum Blood Pool and Bleeding duration +33% |
| Aspect of the Channeler | Fabled | Tribal Chants gain +4 AoE |
| Aspect of the Amphibian | Fabled | Frog Dance gains +4 bounces |
| Acolyte's Embodiment of Unwavering Adherence | Mythic | Blood Sorrow +2 seconds; Eldritch Call +1 tentacle and 25% faster |
| Shaman's Embodiment of Serene Harmony | Mythic | Ultimate charge +25%; Patchwork Abomination +6 seconds; Sundered Skies interval -0.3 seconds; Monument to Gloom +30% duration |
| Summoner's Embodiment of the Omnipotent Overseer | Mythic | Triple Totem +1 Totem; Totem/Aura and healing -55%; Hummingbird +1 and attacks 25% faster |
| Ritualist's Embodiment of the Ancestral Avatar | Mythic | Awakened threshold -50 mana and duration +15 seconds |

## What to optimize for

### Metrics should be content-specific

| Target | Primary objective | Required constraints | Secondary objectives |
|---|---|---|---|
| Solo boss | Sustained single-target time-to-kill over a realistic window | Survive worst non-dodged hit, stable mana/resource loop, practical range | Mobility, healing, burst phase, consistency |
| Short burst or one-shot | Damage in a fixed burst window or one packet | Setup time and state must be reachable; survival until delivery | Cooldown, repeatability, aiming tolerance |
| Raid DPS | Party-adjusted sustained DPS | Room survival, movement, target access, role coverage | Buff synergy, revive/support, AoE for challenges |
| Raid support | Marginal party damage plus healing and damage prevented | Buff/heal uptime, range, enough personal survival | Personal DPS, crowd control, ease of use |
| Lootrun | Expected progress or rewards over a long randomized run | Robust EHP, self-sustain, movement, AoE, scaling under curses | Peak DPS, flying-chest utility, offhand loot setup |
| Guild war | Tower time-to-kill | Non-dodge EHP, unavoidable-hit sustain, stable boss DPS | Party buffs, aggro role, mobility during setup |
| General use | Pareto balance rather than one scalar | Legal, affordable, playable, acceptable damage and EHP | Walk speed, quality of life, range, forgiving rotation |

Suggested reported output for every candidate:

1. Average main-attack damage, final speed, and average main-attack DPS.
2. Relevant spell-part averages, not a misleading sum of mutually exclusive parts.
3. Rotation DPS over at least one short and one sustained window.
4. Mana used per second, passive gain, steal gain assumptions, and worst-case margin.
5. Total HP, EHP without Agility, expected EHP with Agility, HPR/EHPR, Life Steal.
6. Range, AoE, walk speed, and movement-spell assumptions.
7. Setup time and expected uptime for archetype stacks, debuffs, powder specials, and armour specials.
8. Cost and availability: Mythics, ascensions, crafted durability, Aspect tier, Tome requirements, and roll thresholds.

### Optimization sensitivity

| Build family | Benefit from exact optimization | Reason |
|---|---|---|
| Cancelstack | Very high | Discrete net-tier threshold; raw-damage roll values; intertwined SP feasibility; one tier can change 3.1 to 4.3 hits/s |
| Tierstack | Very high | Same speed breakpoint, plus choice of high-base slow weapon and expensive positive-tier slots |
| Heavy melee | High | Per-hit raw and percent damage, Strength, crit, powder conversion and Quake compound; missed constraints are costly |
| Rawstack | High | Raw damage is repeated 4.3 times/s and slot efficiencies can be enumerated exactly |
| Spellspam | Medium to high | Spell-cost floor, repeat-cost and mana feasibility create breakpoints, but many candidates can be operationally equivalent |
| Spellsteal | Medium to high | Static enumeration helps, but actual sustain depends on hit timing, targets, projectiles, and powder procs |
| Fast/heavy hybrid | High but simulation-dependent | Must schedule attacks and spells without double-counting time; multi-objective frontier is broad |
| Poison | Medium | Application and uptime matter more than fine sheet-damage differences; many damage bonuses exclude poison |
| Support/tank | Medium | Party composition and encounter mechanics dominate; optimize a Pareto frontier, not personal DPS |
| Lootrun | Medium | Robustness to randomized boons, curses and rooms matters more than a narrow nominal optimum |
| Aim/state-heavy archetypes | Medium | Focus, Backstab, Winded, Marks, masks, minion AI and similar state depend on player execution and encounter geometry |

## Heavy melee and cancelstack deep dive

### Heavy melee

Community definition: a Main Attack build that stacks damage into slow individual hits, often using armour with negative attack-speed tiers and large raw or percent Main Attack damage. Spells are primarily utility, with Vanish as an important damage tool for Assassin. The current official Builds page also notes that Quake contributes substantially.

Optimization model:

```text
maximize expected damage delivered over horizon H
subject to:
  legal SP and item requirements
  minimum EHP_no_agi and health sustain
  minimum practical range or mobility
  final attack speed, hit probability, and animation availability
  chosen powder-special setup and uptime
```

Do not optimize only the displayed per-hit number. A proper score includes:

- Base and powder-adjusted damage by element.
- Raw Main Attack damage only on compatible present/conversion elements.
- Percent Main Attack and elemental bonuses.
- Strength's generic and Earth multipliers.
- Dexterity expected crit contribution and critical bonus.
- Ability multipliers, Vanish or other state uptime.
- Real hit frequency, not a theoretical click rate.
- Quake every approximately five seconds of active charging, target count, and its steal procs.
- Wind Prison, Courage or Curse setup if used, without assuming permanent uptime.
- Life Steal, recovery gaps and one-shot safety.

Why exact search helps: individual heavy-melee pieces can contribute thousands of raw Main Attack damage while imposing large negative tiers or requirements. The best set is a non-obvious interaction of legality, elemental presence, raw distribution, percent scaling, and multipliers. Greedy ranking of each item independently is unreliable.

### Cancelstack

Current community definition: combine armour with high raw Main Attack damage and negative attack-speed tiers with items that supply positive tiers, usually on an already-fast weapon, so that the tiers cancel and final attack speed reaches Very Fast or Super Fast.

The objective is lexicographic or constrained, not a smooth weighted sum:

1. Reach the required final attack-speed tier, usually Super Fast.
2. Satisfy SP, EHP, sustain, and content constraints.
3. Maximize expected main-attack DPS and secondary utility.

For each candidate:

```text
netTier = clamp(baseWeaponTier + sum(all attack-speed tier IDs), Super Slow, Super Fast)
mainDPS = expectedAveragePerHit(candidate, state) * hitsPerSecond(netTier)
```

The optimizer must use actual rolled tier values. A single low tier roll can make an inventory copy fail the target even when the theoretical max-roll build succeeds. Positive tiers normally roll over 30 to 130 percent of their base, with integer rounding, so purchase recommendations need explicit minimum rolls per item.

Cancelstack is especially sensitive to:

- Exact attack-tier rolls and whether an item is fixed.
- Raw Main Attack rolls on the negative-tier pieces.
- Skill-point bonus rolls that make the set legal or free points for damage.
- The weapon's base speed and Main Attack range.
- Class multi-hit main attacks and their steal fractions.
- Whether a Major ID or Aspect replaces or modifies the main attack.
- Whether the target content rewards pure single-target DPS, AoE, or survivability.

Recommended solver output should include a roll-feasibility certificate:

- Minimum required attack-speed roll on each contributing item.
- Minimum SP rolls by attribute.
- Minimum damage rolls assumed by the score.
- Sensitivity if any one roll drops by one integer.
- A robust score using owned or conservative rolls, not only theoretical max rolls.

## Proposed optimization formulation for this repository

Treat the builder as a constrained, state-aware multi-objective search.

### Stage 1: exact legal enumeration

Enumerate one item per slot, weapon, Tome configuration, skill points, powders, ability path, and Aspects. Apply hard filters early:

- Type, class and level.
- Requirement feasibility and SP budget.
- Ability connectivity/AP/locks.
- Aspect slot and one-Mythic rule.
- Powder capacity and order.
- User bans, ownership, crafted, Mythic, and ascension policies.

### Stage 2: threshold-aware scoring

Use exact discrete states for:

- Final attack-speed tier.
- Spell-cost floor and repeat-cost state.
- Mana feasibility.
- Archetype caps and activation thresholds.
- Minimum EHP, HPR, Life Steal, range, and walk speed.

Do not scalarize before thresholds pass. A cancelstack build one tier short is not "almost" the same build.

### Stage 3: rotation simulation

Simulate or analytically evaluate a user-selected horizon with:

- Cast time and delays.
- Spell repetition costs.
- Main-attack windows and steal probability.
- Powder charge and activation.
- Buff/debuff duration and cooldown.
- Archetype resources such as Focus, Winded, Corrupted, Marks, Momentum, Holy Power, masks, Blood Pool, Mana Bank, puppets and traps.
- Target count, range, AoE overlap, and expected hit rate.

### Stage 4: Pareto results

Return non-dominated candidates on axes such as:

- Sustained DPS.
- Burst DPS.
- EHP without Agility.
- Health and mana sustain.
- Mobility/range.
- Party contribution.
- Cost/availability.
- Robustness to roll variation and encounter-state assumptions.

### Stage 5: in-game acceptance

No calculator is a UI or encounter acceptance test. Validate finalists on a dummy and in target content. Record:

- Actual rotation and time-to-kill.
- Mana floor during a representative cycle.
- Deaths or forced potion use.
- Buff and archetype-state uptime.
- Miss rate, target access and movement comfort.
- Raid or lootrun party outcomes.

## Gaps and unresolved questions

1. The official 2.2.2 Wiki version page says ability changes are still missing. Live API data is current for names, descriptions and requirements, but the public API does not expose every server-side combat implementation detail or bug.
2. The repository's local 2.2.2.0 Aspect data has at least one stale effect: `Aspect of a Thousand Hours` max is +1 AoE locally but +3 AoE live. A full description-level diff should be part of data refresh validation.
3. The official Wiki's Builds article is marked incomplete and much elemental-tri advice was originally written for older patches. Terminology is useful; meta rankings and example builds should not be copied as current truth.
4. Identification roll rules are well established in official-forum material and implemented locally, but the current official Wiki does not expose a concise authoritative roll-distribution section. Roll probability claims beyond min/max multipliers should be qualified.
5. Static builder damage does not fully prove real encounter DPS. Aim, hitboxes, minion AI, latency, repeat-cost timing, boss invulnerability, and server bugs require in-game validation.
6. Armour powder-special uptime cannot be inferred from the build alone. Rage, Kill Streak, Concentration, Endurance and Dodge require encounter-state inputs.
7. Exact optimal EHP thresholds are content- and player-dependent. Publish assumptions rather than asserting universal minimums.
8. Current Wiki formula prose for Agility-aware EHP is less precise than the current calculator implementation. Report no-Agility EHP separately and use expected Agility EHP only with its probabilistic interpretation.

## Source ledger

| Source | Authority | Published or updated | Exact support used |
|---|---|---|---|
| https://api.wynncraft.com/v3/aspects/{class} | Official live API | Retrieved 13 Aug 2026; API v3.7.2; response dated 12 Aug UTC | Complete current Aspect names, rarity, tier thresholds and effects for all classes |
| https://api.wynncraft.com/v3/ability/tree/{class} | Official live API | Retrieved 13 Aug 2026 | Current archetype names/descriptions, node counts and ultimate presence |
| https://docs.wynncraft.com/modules/ability-aspect/list-aspects | Official API docs | Current crawl Aug 2026 | Endpoint schema and allowed five classes |
| https://docs.wynncraft.com/modules/ability-aspect/get-ability-tree | Official API docs | Current crawl Aug 2026 | Archetype/pages schema, requirements, links and locks |
| https://docs.wynncraft.com/2026-04-04-v3-6 | Official API changelog | 4 Apr 2026 | Fruma API update and ultimates added |
| https://wynncraft.wiki.gg/wiki/Version_2.2.2 | Official Wiki | Release 10 Jul 2026 | Current version and explicit missing ability-change documentation |
| https://wynncraft.wiki.gg/wiki/Version_2.2 | Official Wiki | Release 4 Apr 2026 | Fruma level range, items, ascension, powders and equip-order changes |
| https://wynncraft.wiki.gg/wiki/Ability_Tree | Official Wiki | Last edit 28 Apr 2026 | AP cap, connectivity, node types, prerequisites and reset rules |
| https://wynncraft.wiki.gg/wiki/Aspects | Official Wiki | Revised Jul/Aug 2026 | Slots, one-Mythic rule, account sharing, rarity pattern and duplicate upgrading |
| https://wynncraft.wiki.gg/wiki/Strength | Official Wiki | Revised May 2026 | SP budgets, caps and five attribute effects |
| https://wynncraft.wiki.gg/wiki/Identifying | Official Wiki | Revised Jul 2026 | IDs, attack speed, steal fractions, spell costs, mana cap and Major IDs |
| https://wynncraft.wiki.gg/wiki/Powders | Official Wiki | Revised Jul 2026 | Tier VII, conversion order, charge counts and all powder specials |
| https://wynncraft.wiki.gg/wiki/Tomes | Official Wiki | Revised May 2026 | Fourteen slots and Tome categories |
| https://wynncraft.wiki.gg/wiki/Builds | Official Wiki, community terminology | Revised Apr 2026; page marked incomplete | Build-type definitions and elemental vocabulary |
| https://wynncraft.wiki.gg/wiki/Archer | Official Wiki | Revised Apr 2026 | Archer base resistance and archetypes |
| https://wynncraft.wiki.gg/wiki/Warrior | Official Wiki | Revised Jun 2026 | Warrior base resistance and archetypes |
| https://wynncraft.wiki.gg/wiki/Assassin | Official Wiki | Revised Jul 2026 | Assassin base resistance and archetypes |
| https://wynncraft.wiki.gg/wiki/Mage | Official Wiki | Revised early 2026 | Mage base resistance and archetypes |
| https://wynncraft.wiki.gg/wiki/Shaman | Official Wiki | Revised Jun 2026 | Shaman base resistance and archetypes |
| https://wynncraft.wiki.gg/wiki/Lootrunning | Official Wiki | Revised Jul 2026 | End-game objective, fixed loadout, timer, scaling, challenges and rewards |
| https://wynncraft.wiki.gg/wiki/Raid | Official Wiki | Revised Jul 2026 | Team activity structure, scaling, Aspect and Tome rewards |
| https://wynncraft.wiki.gg/wiki/Guild_War | Official Wiki | Revised Mar 2026 | End-game classification and unavoidable tower attack model |
| https://forums.wynncraft.com/threads/2-0-class-building-terminology.305994/ | High-quality community guide | 8 Dec 2022 | Tierstack, cancelstack, rawstack and other terminology; patch-sensitive |
| https://forums.wynncraft.com/threads/class-building-101.266243/ | High-quality historical community guide | 14 Apr 2020 | Spellspam, spellsteal and heavy melee concepts; strongly patch-sensitive |
| https://forums.wynncraft.com/threads/how-identifications-are-calculated.128923/ | Official-forum mechanics guide | 30 May 2016 | Positive 30-130%, negative 70-130%, rounding and star bands; historical but matches local implementation |
| `js/game/game_rules.js` | Current local tool source | Working tree inspected 13 Aug 2026 | SP, level, mana and timing constants |
| `js/game/build_utils.js` | Current local tool source | Working tree inspected 13 Aug 2026 | SP conversion and attack-speed multipliers |
| `js/game/damage_calc.js` | Current local tool source | Working tree inspected 13 Aug 2026 | Damage pipeline and multiplicative effects |
| `js/game/shared_game_stats.js` | Current local tool source | Working tree inspected 13 Aug 2026 | Spell-cost and EHP formulas |
| `js/builder/mana_calc.js` | Current local tool source | Working tree inspected 13 Aug 2026 | Cycle mana model |
| `js/game/skillpoints.js` | Current local tool source | Working tree inspected 13 Aug 2026 | Requirement feasibility and activation-order search |
| `data/2.2.2.0/aspects.json` | Current local community data | Versioned 2.2.2.0 | Cross-check of names, counts and thresholds; one known live-description drift |
