# Current Wynncraft Mythic and Meta Build Evidence

**Research date:** 2026-08-13 (Australia/Sydney)
**Game snapshot:** Wynncraft 2.2.2, released 2026-07-10
**API documentation snapshot:** v3.7.2, dated 2026-05-14
**Community guide snapshot:** first post last edited 2026-08-11

## Purpose and evidence model

This note is a patch-sensitive catalogue for an end-game build solver. It records every current Mythic weapon, all Mythic armour, current community build examples, archetype and ability-tree coupling, and optimization-sensitive build families. It is not a tier list.

Evidence codes used in the tables:

| Code | Authority | Meaning |
|---|---|---|
| O | Official | Wynncraft API documentation or Official Wynncraft Wiki item data. Item and mechanics facts. |
| T | Tool source | WynnBuilder 2.2.2 data and calculator source. Useful for machine-readable items, ability trees, Aspects, calculations, and encoded builds. Community-maintained, not server authority. |
| C | Current community | The Ultimate Build Guide first post, observed 2026-08-13 and last edited 2026-08-11. A maintained recommendation, not a controlled benchmark. |
| I | Inference | A plausible family inferred from the weapon's speed, IDs, Major ID, or archetype. It must be tested before being called meta. |

The [official item database endpoint](https://docs.wynncraft.com/modules/item-recipe/list-items) exposes tier, attack speed, elements, requirements, Major IDs, IDs, and base damage through `GET /v3/item/database`; `?fullResult` bypasses pagination. The official API also exposes the [ability tree and Aspect modules](https://docs.wynncraft.com/modules/ability-aspect/list-aspects), including `GET /v3/aspects/:tree`. The documented API cache is one hour. A direct live download succeeded during integration and returned API v3.7.2 with 6,701 item records and 124 Mythic records. The machine-readable snapshot is `research/build-database/mythics.json`. WynnBuilder's checked-in [2.2.2.0 data](https://github.com/wynnbuilder/wynnbuilder.github.io/tree/master/data/2.2.2.0) remains the compatible source for local build decoding and calculation behavior.

The [WynnBuilder source](https://github.com/wynnbuilder/wynnbuilder.github.io) says it computes damage, spell costs, equip order, skill points, EHP, elemental defenses, ability-tree effects, boosts, and powder specials. WynnAtlas supports item filtering by name, rarity, and IDs. This is strong implementation evidence, but WynnBuilder remains community software.

## Current Mythic inventory facts

The current [official Mythic Items page](https://wynncraft.wiki.gg/wiki/Mythic_Items) reports:

- 11 Mythic weapons for each class, 55 base weapons total.
- 10 pairs of Mythic Boots, one for each two-element pairing.
- One Mythic chestplate, Discoverer.
- No Mythic accessories. Any ring, bracelet, or necklace in a Mythic build is therefore non-Mythic or crafted.
- 15 weapons have Ascensions: Az, Stratiformis, Epoch; Apocalypse, Guardian, Idol; Fatal, Pure, Lament; Archangel, Nirvana, Cataclysm; Aftershock, Olympic, Immolation.
- Ascension changes level, base damage and positive ID maxima, and can add or replace a Major ID. Base and Ascended versions should be modeled as variants of one drop item, not as independent drop items.
- Bloodbath, Labyrinth, Hanafubuki, Trance, and Resonance are Corrupted Cache items and cannot be Shiny.

## How to read the weapon tables

`Elements` lists non-zero base-damage elements. `N` means Neutral. Attack speed is the intrinsic tier: SS = Super Slow, VS = Very Slow, S = Slow, Nrm = Normal, F = Fast, VF = Very Fast, SF = Super Fast. `Current family` is what the maintained forum guide currently links. `Other viable/theoretical` is an inference or a second linked guide family. `Sensitivity` means how strongly exact optimization affects whether the build works, not only whether its displayed DPS is maximal.

### Warrior Mythics

| Item | Lvl | Elements | Speed | Major ID / special constraint | Signature ID pattern | Current family and tree | Other viable/theoretical | Sensitivity | Example | Evidence |
|---|---:|---|---|---|---|---|---|---|---|---|
| [Apocalypse](https://wynncraft.wiki.gg/wiki/Apocalypse) | 81 | N, F | SS | Hellfire; Ascension | Life steal, exploding, negative HPR%, fire focus | Fallen Heavy Melee | Cancelstack variants around SS breakpoint | Very high: tier breakpoint, raw main, life-steal cadence, HPR | [WB](https://wynnbuilder-beta.github.io/builder/#CV0u2HtJyw0SQ05Ge0Yy4qmmjpRgBzRpF+xyf1) | O,C,I |
| [Hero](https://wynncraft.wiki.gg/wiki/Hero) | 91 | N, A | VF | Saviour's Sacrifice | STR/DEX/AGI, HPR%, main%, walk speed | Fallen Upperscream | Low-health party buff or mobility variants | Medium: low-health state and EHP matter more than one damage maximum | [WB](https://wynnbuilder-beta.github.io/builder/#CV0W9GuJ8t2uQG862nmoHEGIhcYu8G0eVhx-ER2ZE) | O,C |
| [Guardian](https://wynncraft.wiki.gg/wiki/Guardian_%28Spear%29) | 93 | N, F | Nrm | Guardian; Ascended Heroine's Blessing | Very high health, HPR, mana regen, defenses | Paladin | Raid support, healer-redirection tank | High for support threshold and EHP, low for personal DPS ranking | [WB](https://wynnbuilder-beta.github.io/builder/#CV0O0HsEyH4OQG05eCYy4AZHtqRgBzR-1-UI7q0) | O,C |
| [Alkatraz](https://wynncraft.wiki.gg/wiki/Alkatraz) | 94 | E | SS | No Major ID | STR, extreme off-stat penalties, exploding, earth and main damage | Fallen Melee | Heavy melee, tierstack or cancelstack | Very high: SS base, equip feasibility, raw main and tier cancellation | [WB](https://wynnbuilder-beta.github.io/builder/#CV0B+045e42+QWw4CDI36am1b68M0eVRizxvNP) | O,C,I |
| [Idol](https://wynncraft.wiki.gg/wiki/Idol) | 95 | W | Nrm | Tidal Drift; Ascension | INT, mana regen, raw spell, cheaper Charge | Battle Monk Surf | Lootrun movement, spell cycle variants | Medium-high: mana cycle and movement uptime | [WB combat](https://wynnbuilder.github.io/builder/#CW0Q0JvqECb+iKCbapBqKgJoDIkH15ammwqKSfzuBQZnc9Mm6), [WB lootrun](https://wynnbuilder-beta.github.io/builder/#CV0Q0nd6GI4sQmu3iD2hJkmmwqKSQ10qUyvPts8XU) | O,C |
| [Thrundacrack](https://wynncraft.wiki.gg/wiki/Thrundacrack) | 96 | N, T | VF | No Major ID | DEX, thunder/water damage, walk speed, cheaper Uppercut | Fallen Bash Cut | Upperscream | High: spell cycle, mana and crit scaling | [WB](https://wynnbuilder-beta.github.io/builder/#CV0G7WM3aE1hLWW64q066mlnkcDuiCuZ0uVRwZ-Tsn0) | O,C |
| [Collapse](https://wynncraft.wiki.gg/wiki/Collapse) | 97 | N, all elements | SS | Fission | Mana steal, extreme exploding, main%, negative elemental defenses | Fallen Upperscream | Explosive melee or hybrid | High: rainbow feasibility, sustain, negative defenses | [WB](https://wynnbuilder-beta.github.io/builder/#CV013HxGiK4lGHgJITI15YFYdc6SS98H0qlrzVdDH2B) | O,C,I |
| [Bloodbath](https://wynncraft.wiki.gg/wiki/Bloodbath) | 103 | N, E, T | S | No Major ID; Blood Pact tree is central | STR/DEX, life steal, health penalty, extreme negative mana regen, cheaper Bash | Fallen Upperbash | Health-casting spellsteal | Very high: health economy, life-steal cadence and Blood Pact safety | [WB](https://wynnbuilder-beta.github.io/builder/#CV0G7Qu8GiXJwa6EhLQu8e11DuRa3aOoh1dy10-RJVylpE60) | O,C |
| [Convergence](https://wynncraft.wiki.gg/wiki/Convergence) | 104 | N, W, F | Nrm | No Major ID | HPR%, off-element damage bonuses, sprint regen, cheaper Uppercut | Fallen Upperbash | Paladin or Battle Monk hybrid | High: cycle cost, sustain and hybrid tree opportunity cost | [WB](https://wynnbuilder-beta.github.io/builder/#CV0G7WM3me0bEHbHgCI15WX2Fk6Sq70ylDz+xiZX1) | O,C,I |
| [Ascendancy](https://wynncraft.wiki.gg/wiki/Ascendancy) | 112 | N, W, F, A | F | Divine Right | -100 STR and DEX, health, reflection, raw spell; max-health sacrifice mechanic | Paladin Big Mac / Sacred Surge | Support-damage Paladin | Very high: exact lost-health and maximum-health interactions | [WB](https://wynnbuilder.github.io/builder/#CW0XJnDJGI4OQG05gsHhJM1mTpRSmr0qlTVl9oTK3) | O,C |
| [Restitution](https://wynncraft.wiki.gg/wiki/Restitution) | 120 | T, W, F | Nrm | Old Spark | DEX, health, mana regen, thunder spell, cheaper Uppercut, negative walk speed | Fallen Upperscream | Controlled Whirlwind spell variants | High: end-game SP, cost cycle, mobility floor | [WB](https://wynnbuilder-beta.github.io/builder/#CV0G7WM3Wi3d-Wl4ybG15EGospDSA506gNDQ-VwVdDYv20) | O,C |

### Archer Mythics

| Item | Lvl | Elements | Speed | Major ID / special constraint | Signature ID pattern | Current family and tree | Other viable/theoretical | Sensitivity | Example | Evidence |
|---|---:|---|---|---|---|---|---|---|---|---|
| [Az](https://wynncraft.wiki.gg/wiki/Az) | 74 | N, T | F | Ascended Fallout | INT/DEF, XP, water/fire bonuses, Arrow Storm cost | Sharpshooter | Ascended spell or leveling variants | Medium-high: low base level and Ascension change feasible gear pool | [WB](https://wynnbuilder.github.io/builder/#CW0bu0OJeJ4yQWjHiDI15EGYipKmbtFZFFD6-aD0) | O,C |
| [Freedom](https://wynncraft.wiki.gg/wiki/Freedom) | 93 | E,T,W,F,A | Nrm | No Major ID | AGI, health, mana regen, raw spell/main, walk speed | Sharpshooter | Rainbow Boltslinger or hybrid | Medium: broad stat profile creates a wide plateau | [WB](https://wynnbuilder.github.io/builder/#CW013HyGyb0lGHgJITI15YF2ecKu488G0uzpupJZnFP3) | O,C,I |
| [Grandmother](https://wynncraft.wiki.gg/wiki/Grandmother) | 95 | N, E | S | No Major ID | STR/AGI, spell/main damage, negative raw and percent HPR, negative walk speed | Sharpshooter | Boltslinger | Very high: HPR cancellation, EHP and damage range | [WB](https://wynnbuilder.github.io/builder/#CW0s2nwG8Y3dQ06EM0H15UmGgc5peSS10y+Pyvfnudi1) | O,C,I |
| [Ignis](https://wynncraft.wiki.gg/wiki/Ignis) | 95 | N, F | Nrm | Altruism | DEF, health/HPR, fire/air defense, Arrow Shield cost | Sharpshooter | Trapper or party-support sustain | High for support/HPR, medium for DPS | [WB](https://wynnbuilder.github.io/builder/#CW0s2HyG0I4XQmgH2eWy4kongcRSa70y+Pyvfnudi1) | O,C,I |
| [Divzer](https://wynncraft.wiki.gg/wiki/Divzer) | 97 | N, T | SF | No Major ID | DEX, very negative DEF/AGI, life/mana steal, +tier, raw damage | Boltslinger Hybrid or Spell Boltslinger | Glass Sharpshooter | Very high: glass EHP floor, sustain and hit-rate assumptions | [WB hybrid](https://wynnbuilder.github.io/builder/#CW0cVAwCufe3dWYwZlcYwZfHCD266M1GdcDSM7qI830qFwzFuXx9S8), [WB spell](https://wynnbuilder.github.io/builder/#CW0o2tm9im6EfJsm1jv6E2QGG3OOG09TQsmfRGB1m-exVq7SEa30) | O,C |
| [Spring](https://wynncraft.wiki.gg/wiki/Spring) | 98 | N, W | Nrm | No Major ID | STR/INT, negative DEX, mana regen, slow/weaken, water damage | Boltslinger | Sharpshooter | High: mana and Arrow Storm cycle | [WB](https://wynnbuilder.github.io/builder/#CW062smXVs6ET4tmvju6Es6ns8jEv23wQInb-Ht-eFuS870) | O,C |
| [Stratiformis](https://wynncraft.wiki.gg/wiki/Stratiformis) | 99 | N, A | F | Ascended Elven Current | AGI, negative health, spell/air-main, extreme walk speed | Sharpshooter | Bolt Hybrid, lootrun mobility | High: negative health and distance uptime | [WB sharp](https://wynnbuilder.github.io/builder/#CW0F3HyGmX3c515G2emoHUFYtqYSvzpupJZnFP3), [WB bolt](https://wynnbuilder.github.io/builder/#CW0l2Bw40eep6mYw3kcYwJiHKd0s34+WtqYSY4490qFwzFuXx9S8) | O,C |
| [Epoch](https://wynncraft.wiki.gg/wiki/Epoch) | 102 | N, T, A | SS | Ascension | Life steal, negative mana steal and walk speed, raw main, spell%, spell-cost reductions | Tier Stack | Cancelstack interpretation | Very high: discrete speed tiers and raw-main scaling | [WB](https://wynnbuilder.github.io/builder/#CW0iuAw0HfeJOcYwZlcYwZfHKdmW4om0wqDSfVqxVm3tJuG) | O,C,I |
| [Labyrinth](https://wynncraft.wiki.gg/wiki/Labyrinth) | 104 | E, F | VS | Twisting Threads; cache-only | DEF, mana steal, slow, earth/fire damage, Bomb cost | Trapper | No strong cross-archetype case because MID is Trapper-specific | High: trap overlap, duration, target geometry and cost cycle | [WB](https://wynnbuilder.github.io/builder/#CW0O0lvWEycphZREXQkvyuW77WrXKS4oxcJA1WsNpjRe3VU001) | O,C |
| [Revolution](https://wynncraft.wiki.gg/wiki/Revolution) | 111 | N, E, W, A | VS | Regicide | INT, mana steal, negative main range and health, cheaper Escape | Sharpshooter | Distance-specialized spell build | Very high: Regicide distance and range behavior | [WB](https://wynnbuilder.github.io/builder/#CW0ZJ1OJWi3dQG05ETIhJ4+0TpKSq40y+Pyvfnudi1) | O,C |
| [Eschaton](https://wynncraft.wiki.gg/wiki/Eschaton) | 119 | T, F, A | SF | Death Sentence | +tier, exploding, negative health, raw elemental damage, Shield cost | Boltslinger | Tierstack hybrid | Very high: MID/archetype lock, health floor and hit cadence | [WB](https://wynnbuilder.github.io/builder/#CW0rkAwCufeZ3eYwJSnYwJ056sX57UFYtpRkqFwzFuXx9S8) | O,C,I |

### Mage Mythics

| Item | Lvl | Elements | Speed | Major ID / special constraint | Signature ID pattern | Current family and tree | Other viable/theoretical | Sensitivity | Example | Evidence |
|---|---:|---|---|---|---|---|---|---|---|---|
| [Pure](https://wynncraft.wiki.gg/wiki/Pure) | 65 | N, W, A | F | Gravity Well; Ascended Cosmic Capture | Mana steal, reflection, very high spell%, negative main%; Ascended Meteor cost | Arcanist | Ascended utility spell | High: Ascension, Meteor timing, mana bank | [WB](https://wynnbuilder-beta.github.io/builder/#CT0vAIv0sCbp4lKE1DJvi1XD8KG19Cyxay+elEyw-C) | O,C |
| [Lament](https://wynncraft.wiki.gg/wiki/Lament) | 96 | N, W | S | Ascended Requiem | Negative life steal, high mana steal and healing efficiency, water%, cheaper Heal | Lightbender | Dedicated raid healer | Very high for healing/sustain thresholds, medium for damage | [WB](https://wynnbuilder-beta.github.io/builder/#CV00+00nKS69Uxmh9bJoDIkHhJEG2xqKSo10qFl5tQgh7D) | O,C |
| [Gaia](https://wynncraft.wiki.gg/wiki/Gaia) | 97 | N, E | SS | Earthen Splinter | STR, poison, main% and raw main, cheaper Ice Snake | Riftwalker Melee | Riftwalker spell | Very high for melee tier/raw-main; high for poison and Quake cadence | [WB melee](https://wynnbuilder.github.io/builder/#CW0W9Qu0HfXZBn67VDDSKdG46A1Y4EArqGSf-TFTlPo8M0), [WB spell](https://wynnbuilder.github.io/builder/#CX05uQeSmPUJVo9Up9KJ3rP14BW0w8k7fKJ3vHQ04lspho4jHhJT) | O,C |
| [Monster](https://wynncraft.wiki.gg/wiki/Monster) | 98 | N, F | S | No Major ID | DEF, life/mana steal, health, fire/main%, more expensive Heal | Lightbender, Arcanist, Riftwalker | Tanky spell or melee hybrid | High: healing-cost penalty and sustain | [WB Lightbender](https://wynnbuilder-beta.github.io/builder/#CV0h2lvK1ycpIbRET5l9S4DI05mUmLEarSJUM40qFluMpTTCQ0), [WB Arcanist](https://wynnbuilder-beta.github.io/builder/#CT02E105uH42D1iHe0Yy4koXicRSo3a70yUvlcM-Z3), [WB Riftwalker](https://wynnbuilder.github.io/builder/#CX0h2lvK1yc3xaREO5l9Eh6Pr85KG33oQknPGGW0G-lNCCHBjEC) | O,C |
| [Fatal](https://wynncraft.wiki.gg/wiki/Fatal) | 99 | T | VF | Unstable Reaction; Ascension | DEX, mana steal, spell%, walk speed, expensive Heal, cheap Teleport | Arcanist or Riftwalker | Lightbender damage variant | High: random damage range, Judrajim timing and mana | [WB Arcanist](https://wynnbuilder-beta.github.io/builder/#CT02EnZJCn3eDH76+QG15uM0tqDyikoMso2GUF+hfr-v), [WB Riftwalker](https://wynnbuilder.github.io/builder/#CX0X6GyGm9OZyWsSZy4G1V9WX1yRmDT3dn1j4420jdNb9QZPEr1) | O,C |
| [Singularity](https://wynncraft.wiki.gg/wiki/Singularity) | 99 | E,T,W,F,A | SS | Orbital Chain; 15 slots | DEX, raw HPR, spell/raw spell, main/raw main | Arcanist or Riftwalker | Powder-special hybrid | Very high: 15 powder slots, rainbow SP, Chain Lightning setup | [WB Arcanist](https://wynnbuilder-beta.github.io/builder/#CT013ntFWi3i5146KyG15ko1jcD00SvzulcM-d3), [WB Riftwalker](https://wynnbuilder.github.io/builder/#CX013J9Fn3nInKAElGt8FfE9r95K8+8qQE10hn9L0GxvLPYserfE) | O,C |
| [Warp](https://wynncraft.wiki.gg/wiki/Warp) | 99 | N, A | VF | No Major ID | AGI, negative mana/HPR/healing, extreme walk speed and Teleport cost reduction | Arcanist or Riftwalker | Lootrun mobility | Very high: HPR cancellation, 125 AGI feasibility, survivability | [WB Arcanist](https://wynnbuilder-beta.github.io/builder/#CT0oqmFJiK4-v0cHITI15amWmcYSA20nxzaVDRzFE), [WB lootrun](https://wynnbuilder-beta.github.io/builder/#CT0820OJWi3kQGCKG0H15EGWmcYoRy3DrM7jvtJ+rir-u), [WB Riftwalker](https://wynnbuilder.github.io/builder/#CX0cFBAE9uAwiKieWZPnYEfE9r9B7XR12RAof80GxvLPYserfE) | O,C |
| [Quetzalcoatl](https://wynncraft.wiki.gg/wiki/Quetzalcoatl) | 103 | N, E, A | VF | No Major ID | Very high life steal, negative healing, raw spell/main, movement | Arcanist or Riftwalker | Hybrid melee-spell | High: life-steal timing and negative healing | [WB Arcanist](https://wynnbuilder-beta.github.io/builder/#CT0X6W5GCn3rQmKEcoH15aGIFk6uoxnVDRzFE), [WB Riftwalker](https://wynnbuilder.github.io/builder/#CX0mCBw4+iecp9nYwptqYEJvW2827vz8zuQGSo6SA0yUUHceDRUr1) | O,C |
| [Trance](https://wynncraft.wiki.gg/wiki/Trance) | 104 | N, T, F | SF | Fixate; cache-only; Riftwalker-specific | Life/mana steal, +10 tiers, negative main% and walk speed | Riftwalker Melee | Spellsteal Riftwalker | Very high: +tier interaction, steal cadence, Distortion timing | [WB](https://wynnbuilder.github.io/builder/#CW0rkkvWHycp3ZRgpUnRgZfHCDIvHamn8lRgpmWW1W+tjqzDJMm2) | O,C,I |
| [Riptide](https://wynncraft.wiki.gg/wiki/Riptide) | 110 | N, E, T, W | SS | Wavebreak; Arcanist interaction | INT, negative DEF, max mana, mana steal, water spell, cheaper Meteor | Arcanist | Mana-bank Meteor specialist | Very high: Psychokinesis bounces, mana-bank loss, target geometry | [WB](https://wynnbuilder-beta.github.io/builder/#CV0G7IvmFBbJ7kKEyQIvOR4RZqwaBC8rC5d71720l7zrir-v) | O,C |
| [Halcyon](https://wynncraft.wiki.gg/wiki/Halcyon) | 118 | T,W,F,A | F | Blinding Lights; Lightbender-specific | -50 STR, negative health, healing, elemental raw spell/defense | Lightbender | Close-orbit Ophanim specialist | Very high: orbit uptime, Crystallize, health floor | [WB](https://wynnbuilder.github.io/builder/#CX0g2nDJSI4jWWfJCTIhJMbnupKSS90qFluMpT-G3) | O,C |

### Assassin Mythics

| Item | Lvl | Elements | Speed | Major ID / special constraint | Signature ID pattern | Current family and tree | Other viable/theoretical | Sensitivity | Example | Evidence |
|---|---:|---|---|---|---|---|---|---|---|---|
| [Archangel](https://wynncraft.wiki.gg/wiki/Archangel) | 69 | N, A | F | Ascended Grand Influence | DEF/AGI, main range, health/HPR, walk speed; Ascended weaken | Acrobat | Fast-melee Trickster support | High after Ascension because 80% health support threshold | [WB](https://wynnbuilder.github.io/builder/#CX0oq0xGWi3X7mbHIk1LEEGmmpYSg2C50yUoVnMtuN1) | O,C,I |
| [Nullification](https://wynncraft.wiki.gg/wiki/Nullification) | 95 | E,T,W,F,A | F | Punishment | DEF, life/mana steal, reflection, negative poison, elemental defense | Trickster | Shadestepper or Acrobat | High: archetype choice changes objective completely | [WB Trickster](https://wynnbuilder-beta.github.io/builder/#CT013nwGyb0d5XfJWmG15ammicRohtczXl+u-SdKcF+QJK8), [WB Shade](https://wynnbuilder.github.io/builder/#CW0++GxGyb0lGXT8GtH15YToicDSk20ce3vVyVOztN72), [WB Acro](https://wynnbuilder.github.io/builder/#CX013HzGyb0lGHgJC0G15YFoicDSE30yUoVnMtuN1) | O,C |
| [Cataclysm](https://wynncraft.wiki.gg/wiki/Cataclysm) | 96 | N, T | SF | Ascension | DEX, severe negative health, thunder%, cheaper Spin and Ascended Multihit | Acrobat | Premade-party Trickster | Very high: glass EHP, hit execution, Ascension | [WB](https://wynnbuilder.github.io/builder/#CX03EXu4qX3S5HoDIk166MbHwqDSvza-Yjknl2) | O,C |
| [Grimtrap](https://wynncraft.wiki.gg/wiki/Grimtrap) | 96 | N, E | S | Torment Liturgy; Shadestepper-specific | STR, life steal, poison, negative HPR%, cheaper Multihit/Smoke | Shadestepper | Melee or poison hybrid | Very high: Vanish refresh, Surprise Strike multiplier and HPR | [WB](https://wynnbuilder.github.io/builder/#CW0-2nO68Y3dQ0wE2eGs3gL0fc6Sg70yFwFi+xtX0) | O,C,I |
| [Weathered](https://wynncraft.wiki.gg/wiki/Weathered) | 96 | N, A | VF | Roving Assassin | AGI, mana steal, +1 tier, air%, walk speed, negative exploding | Shadestepper | Sustain-focused spellsteal | High: Vanish regeneration and steal cadence | [WB](https://wynnbuilder.github.io/builder/#CX0oqG0Ei15EvG05e0orHiFolcYSvVf-mwRftG) | O,C |
| [Inferno](https://wynncraft.wiki.gg/wiki/Inferno) | 97 | F | VS | No Major ID | DEF, health, negative HPR%, fire%, large raw main, cheaper Spin | Trickster | Heavy melee/cancelstack | Very high for melee tier cancellation; high for Trickster sustain | [WB](https://wynnbuilder-beta.github.io/builder/#CT0vvmwmREIWkv4gvcpM72eWhJko1hcRSvVkJAp7Vj9A4) | O,C,I |
| [Nirvana](https://wynncraft.wiki.gg/wiki/Nirvana) | 97 | W | Nrm | Transcendence; Ascended Enlightenment | INT, negative mana steal/health/main%, mana regen, spell% | Acrobat | Trickster spell | Very high: probabilistic free/refunded casts and mana cycle | [WB](https://wynnbuilder.github.io/builder/#CX0e1GyGqH4yQGoDIkHhJOFosqKSk30yUoVnMtuN1) | O,C,I |
| [Oblivion](https://wynncraft.wiki.gg/wiki/Oblivion) | 101 | N, T, W | SS | No Major ID | DEX, mana steal, negative mana regen, raw spell, cheaper Dash | Spell Shadestepper | Glass Shadestepper or Trickster | Very high: mana-steal cycle and wide random base damage | [WB](https://wynnbuilder.github.io/builder/#CW0buEvWWxaJ7Emh9Zp28qm1M7Qd0GkC2turVqVOztl31), [WB glass](https://wynnbuilder.github.io/builder/#CW0bum9DaE1yQGoDqm1KEml1GkCuK98O0YN-H-XrV-E4) | O,C |
| [Hanafubuki](https://wynncraft.wiki.gg/wiki/Hanafubuki) | 102 | N, A | SF | Efflorescence; 0 slots; cache-only; Acrobat-specific | Ability redistribution and Acrobat landing/garden coupling | Acrobat | No credible cross-archetype case without discarding MID value | Very high: Acrobat loop execution and ability-tree lock | [WB](https://wynnbuilder.github.io/builder/#CX0k2HyGuY3VJHgJITI15am0AF8F0uza-Yjknl2) | O,C |
| [Architect](https://wynncraft.wiki.gg/wiki/Architect) | 109 | N, E, W, F | S | Living Museum; Trickster-specific | +15 all stats, negative life steal, health, mana regen, healing | Trickster support | Clone-healing premade support | Very high: clone destruction timing and party proximity | [WB](https://wynnbuilder-beta.github.io/builder/#CU0TlIviECbpglRElGlvawaKdKG19CW1z6Sv-tJANwuDv00) | O,C |
| [Vengeance](https://wynncraft.wiki.gg/wiki/Vengeance) | 116 | N, E, T, F | SS | Manic Edge; Shadestepper-specific | Negative mana steal, positive HPR% with extremely negative raw HPR, damage% | Heavy Melee Shadestepper | Spell Shadestepper | Extreme: HPR cancellation, mana drain, Vanish dwell time, SS raw-main | [WB heavy](https://wynnbuilder.github.io/builder/#CW04KQuCA3c1HXX6EE0n1827XD8xDRmna0m-ybq-j82BB), [WB spell](https://wynnbuilder.github.io/builder/#CW0m2HyGG00dQ0wEGt1s3OGoUp6SvVw-mwlTD2) | O,C |

### Shaman Mythics

| Item | Lvl | Elements | Speed | Major ID / special constraint | Signature ID pattern | Current family and tree | Other viable/theoretical | Sensitivity | Example | Evidence |
|---|---:|---|---|---|---|---|---|---|---|---|
| [Aftershock](https://wynncraft.wiki.gg/wiki/Aftershock) | 77 | N, E | SS | Ascension | STR/DEF, health, earth%, negative spell%, cheaper Uproot | Ritualist Tierstack | Heavy melee or cancelstack | Very high: SS tiers, triple-beam hit model, Ascension | [WB](https://wynnbuilder-beta.github.io/builder/#CT0W9045uI4+QWw4CDI9Gam1pp2p8TvzUNjMkk4a2) | O,C,I |
| [Olympic](https://wynncraft.wiki.gg/wiki/Olympic) | 93 | A | F | Lightweight; Ascension | Movement/jump identity and air damage | Summoner | Acolyte mobility | High: movement execution and Ascension, medium stat sensitivity | [WB](https://wynnbuilder-beta.github.io/builder/#CV0ZJX5GaJ4c5H86mCooHmlXsqKoB-3NNyTUfe2) | O,C,I |
| [Hadal](https://wynncraft.wiki.gg/wiki/Hadal) | 94 | W | SS | No Major ID; 130 INT requirement | Max mana, mana regen, high spell%, positive Aura/Uproot cost% | Acolyte | Summoner spell | Very high: cost penalties despite 130 INT, mana-cycle feasibility | [WB](https://wynnbuilder-beta.github.io//builder/#CV0e1IviECb39nKEXQIvi0Y5G0S4-SWg95NQljFwzmS) | O,C,I |
| [Sunstar](https://wynncraft.wiki.gg/wiki/Sunstar) | 95 | N, T | VS | No Major ID | Life steal, +1 tier, reflection/thorns, negative healing, raw main | Ritualist Tierstack | Cancelstack melee | Very high: speed tier, triple beams, raw main and negative healing | [WB](https://wynnbuilder-beta.github.io/builder/#CT0o2145C62+QWw4CDYOGamHkcDSvzUNjMkk4a2) | O,C,I |
| [Fantasia](https://wynncraft.wiki.gg/wiki/Fantasia) | 96 | E,T,W,F,A | VS | No Major ID | +50 INT, negative mana steal/regen, spell%, cheaper all spells | Acolyte | Ritualist or Summoner rainbow spell | Very high: offsetting negative mana IDs and multi-spell cycle | [WB](https://wynnbuilder-beta.github.io/builder/#CV013HxGiK4lGHgJITI15YFYecKSU4490qVw+UyzqW0) | O,C,I |
| [Toxoplasmosis](https://wynncraft.wiki.gg/wiki/Toxoplasmosis) | 96 | E | VF | Outbreak; 3-3 base damage | Loot bonus, life/mana steal, up to 65,000 poison, walk speed | Mob Grinding | Poison cloud/grouping specialist | Extreme: normal weapon DPS is irrelevant; poison, priming, Uproot and grouping dominate | [WB](https://wynnbuilder-beta.github.io/builder/#CV06End6HWHoJ26ZVXVXVnXY482CIUImOyByByBQKa0HmaZPoUoapaZPoaI241JEc9x9JEJEc9JA90yUe+8zQ8-pkzv5OOYi0) | O,C |
| [Absolution](https://wynncraft.wiki.gg/wiki/Absolution) | 97 | F | SF | No Major ID; 0 slots | Health, mana regen, up to 75% healing efficiency, defenses, cheap Totem | Acolyte | Dedicated raid healer/support | Very high for healing breakpoint and team survival, lower for DPS | [WB](https://wynnbuilder-beta.github.io/builder/#CV05uk9ZJ5mREiJkvy2zcuiQaLZuw4DCCfXqlxHl7UVA4) | O,C |
| [Immolation](https://wynncraft.wiki.gg/wiki/Immolation) | 101 | F, A | S | Ascended Blazing Effigy | Large negative HPR% and healing, fire/air%, cheaper Aura | Summoner | Damage-conversion Totem after Ascension | Very high: regeneration-to-damage conversion and no-heal trade | [WB](https://wynnbuilder-beta.github.io/builder/#CV013HuJiK4lGHgJITI15YFYwqRYB-3M7yTUhe2) | O,C |
| [Resonance](https://wynncraft.wiki.gg/wiki/Resonance) | 104 | N, E, W | Nrm | Lifestream; cache-only; Acolyte-specific | DEF, mana/HPR, elemental spell%, cheaper Uproot, movement penalties | Acolyte | Blood Sorrow burst/overhealth specialist | Very high: 4x Blood Sorrow with 75% shorter duration | [WB](https://wynnbuilder.github.io/builder/#CW05uIv0sCb3okKdJDfS2eWaGQToLE4vbYZs0W+JttZld64) | O,C |
| [Fate](https://wynncraft.wiki.gg/wiki/Fate) | 108 | N, E, F, A | VS | Starcrossed; Ritualist-specific | STR/DEF/AGI, weaken, elemental raw main, walk speed, expensive Aura, cheap Uproot | Ritualist Tierstack | Linked-target cleave support | Extreme: tierstack plus Linked-target encounter geometry | [WB](https://wynnbuilder-beta.github.io/builder/#CT0i21s1Cn3OVWw4CDI15amn8q6fpb0WsxDL3NNv4a2) | O,C |
| [Transfiguration](https://wynncraft.wiki.gg/wiki/Transfiguration) | 117 | E, T, A | SF | Flain Remnants; Summoner-specific | AGI, negative max mana, mana steal, +5 tiers, elemental raw damage | Summoner | Puppet-remnant main-attack hybrid | Extreme: puppet destruction rate, five-remnant cap and item-roll feasibility | [WB](https://wynnbuilder-beta.github.io/builder/#CU0t1HO10ufe+WhPZuG1C2k8CJk8k8k8k8WJa0Ae0wHgQUJTu10yUJTmtzj2A) | O,C |

## Mythic armour and accessory boundary

Mythic armour is build-enabling, not universally best-in-slot. Each piece has a sharp objective. The live v3.7.2 snapshot resolves current wiki-page discrepancies for Galleon, Resurgence, and Revenant, so the live API values below are the exact solver inputs for this research date.

| Item | Slot | Lvl | Pair / requirement theme | Signature IDs and role | Most relevant families | Exact-optimization concern | Evidence |
|---|---|---:|---|---|---|---|---|
| [Discoverer](https://wynncraft.wiki.gg/wiki/Discoverer) | Chest | 89 | None | 0 health, 0 slots, +46 to +200 loot bonus, +5 to +20 XP | Lootrun and chest-running utility only | Survival and route speed must be constrained; combat DPS is the wrong objective | O |
| [Crusade Sabatons](https://wynncraft.wiki.gg/wiki/Crusade_Sabatons) | Boots | 90 | Earth/Fire | STR/DEF, health, HPR%, thorns, earth/fire defenses, negative walk speed | Tank, HPR, Guardian/Paladin, heavy melee | Walk-speed floor and HPR/EHP | O |
| [Resurgence](https://wynncraft.wiki.gg/wiki/Resurgence) | Boots | 91 | Water/Fire | INT, mana regen, HPR, negative spell/main%, negative walk speed | Mana sustain, healer, tank spell | Cost-cycle threshold versus damage penalties | O |
| [Galleon](https://wynncraft.wiki.gg/wiki/Galleon) | Boots | 92 | Earth/Water | Loot bonus, max mana, mana steal, stealing, -1 tier, earth/water and main% | Spellsteal, loot utility, slow melee | -1 tier can be beneficial, neutral, or fatal depending on speed target | O |
| [Boreal](https://wynncraft.wiki.gg/wiki/Boreal) | Boots | 93 | Fire/Air | Reflection, mana regen, HPR%, raw HPR, walk speed, defensive focus | HPR tank, support, Warp cancellation candidate | HPR cancellation and survivability | O,I |
| [Slayer](https://wynncraft.wiki.gg/wiki/Slayer) | Boots | 94 | Thunder/Air | DEX, +1 tier, negative HPR, raw main, walk speed, third-spell cost | Tierstack, cancelstack, melee | Tier breakpoint and negative HPR | O |
| [Moontower](https://wynncraft.wiki.gg/wiki/Moontower) | Boots | 95 | Water/Air | Strong INT/AGI, negative other stats, water/air defense, walk speed | Warp lootrun, mobility spell | SP feasibility and survivability | O |
| [Dawnbreak](https://wynncraft.wiki.gg/wiki/Dawnbreak) | Boots | 96 | Thunder/Fire | Life/mana steal, -14 tiers, exploding, thunder/fire%, exceptionally high raw main | Heavy melee and cancelstack | One of the strongest exact-cancellation cases: -14 tiers must be deliberately exploited or neutralized | O |
| [Stardew](https://wynncraft.wiki.gg/wiki/Stardew) | Boots | 97 | Thunder/Water | Mana steal, reflection, negative mana regen, thunder/water%, raw spell | Spellsteal and burst spell | Mana-steal cadence and cycle feasibility | O |
| [Warchief](https://wynncraft.wiki.gg/wiki/Warchief) | Boots | 98 | Earth/Thunder | STR/DEX, exploding, earth/thunder%, main% and raw main, negative walk speed | Heavy melee, tierstack, cancelstack | Raw-main multiplication, speed and mobility floor | O |
| [Revenant](https://wynncraft.wiki.gg/wiki/Revenant) | Boots | 99 | Earth/Air | Mana steal, reflection, negative health, earth/air%, negative main% plus raw main, walk speed, fourth-spell cost | Fast hybrid, Acrobat, mobility melee | Negative health and mixed raw/percent main interaction | O |

There are **no Mythic accessories**. Relevant accessories in a build should be catalogued by function instead: skill-point enabler, raw-main stack, attack-tier source, mana sustain, HPR cancellation, spell-cost reduction, Major ID, walk speed, healing, or elemental defense. The current forum builds regularly use Legendary, Fabled, raid, crafted, and guild-tome-dependent accessories. A solver must not assume a Mythic rarity constraint for these slots.

## Ability-tree and archetype coupling patterns

The encoded current WynnBuilder links are the best reproducible record of a whole build because they include an ability tree as well as equipment. However, ability trees and Aspects are patch-sensitive. Decode against the 2.2.2 `atree.json` and `aspects.json`, or query the official endpoints, rather than reading a hash with a different data version.

| Class | Archetype pattern in current Mythic builds | Mythics with strong or hard coupling | Solver implication |
|---|---|---|---|
| Warrior | Fallen dominates damage builds through low-health, Corrupted, Blood Pact and Upperbash/Upperscream loops. Paladin is the tank/support and Sacred Surge path. Battle Monk supplies Surf/mobility and hybrid play. | Apocalypse and Bloodbath to Fallen; Guardian and Ascendancy to Paladin; Idol to Battle Monk/Surf. | Ability state, health state and spell sequence are part of the genome. Gear-only DPS is invalid. |
| Archer | Sharpshooter is range/precision and distance-sensitive. Boltslinger is high hit-rate spell/melee hybrid. Trapper uses trap overlap and encounter geometry. | Labyrinth to Trapper; Eschaton to Boltslinger; Revolution strongly rewards distance; Stratiformis supports Sharpshooter and Bolt Hybrid. | Model target distance, number of targets, trap overlap, hit rate and Arrow Storm channel time. |
| Mage | Arcanist revolves around Mana Bank and spell sequencing. Riftwalker revolves around Winded/Distortion and can support melee/spellsteal. Lightbender revolves around Ophanim and healing. | Trance to Riftwalker; Riptide to Arcanist; Halcyon to Lightbender; Lament strongly supports Lightbender/healing. | Model Mana Bank state, Distortion, Ophanim proximity, healing output and actual rotation. |
| Assassin | Shadestepper depends on Vanish, backstab/Surprise Strike and Marked state. Trickster depends on clones and party context. Acrobat depends on aerial loops, Lacerate/Swan Dive and landing. | Grimtrap and Vengeance to Shadestepper; Architect to Trickster; Hanafubuki to Acrobat. | Model positional success, Vanish dwell time, clone deaths, aerial uptime and execution failure. |
| Shaman | Acolyte trades health and uses Blood Sorrow/overhealth. Summoner uses puppets/Totem. Ritualist uses masks, memories and target links. | Resonance to Acolyte; Transfiguration and Immolation to Summoner; Fate to Ritualist; Toxoplasmosis to Totem/Uproot poison loop. | Model health sacrifice, puppet generation/destruction, Totem uptime, mask state, linked targets and grouping. |

### Major IDs that change the optimization problem

Major IDs are not ordinary additive IDs. The official [Identifications page](https://wynncraft.wiki.gg/wiki/Identifications) says they are fixed, do not stack with themselves, and can provide class/archetype-specific behavior unavailable elsewhere. Examples that require dedicated evaluator logic:

- `Divine Right`: changes maximum-health assumptions and Sacred Surge scaling.
- `Blood Pact` is an ability rather than a weapon MID, but Bloodbath's negative mana profile makes the health-casting path central.
- `Twisting Threads`, `Regicide`, `Death Sentence`, `Blinding Lights`, `Wavebreak`, `Fixate`, `Efflorescence`, `Living Museum`, `Manic Edge`, `Lifestream`, `Starcrossed`, `Flain Remnants`, and `Outbreak` all change topology, state, range, target count, duration, or resource conversion. A static tooltip-DPS score cannot compare them fairly.
- Ascended MIDs can create a different archetype: Pure's Cosmic Capture, Lament's Requiem, Archangel's Grand Influence, Nirvana's Enlightenment, and Immolation's Blazing Effigy are material changes.

## Build families and optimization sensitivity

### Heavy melee, tierstack, and cancelstack

These families gain the most from exact combinatorial optimization.

**Heavy melee** usually starts with a slow weapon whose large per-hit base damage and raw main-attack bonuses are preserved. Apocalypse and Vengeance have explicit current Heavy Melee forum builds. Alkatraz and Gaia have current melee builds and are natural exact-optimization candidates. Warchief and Dawnbreak can supply exceptional raw main damage.

**Tierstack** pushes attack speed through discrete tiers. Current explicit examples are Epoch Tier Stack, Sunstar Ritualist Tierstack, Aftershock Ritualist Tierstack, and Fate Tierstack Ritualist. Trance and Transfiguration carry large positive tier IDs and need the actual multi-hit/class main-attack model.

**Cancelstack** deliberately combines large negative attack-tier IDs and compensating positive tiers while stacking raw main damage. Dawnbreak's -14 tiers is the clearest armour-side cancellation term. Super Slow or Very Slow weapons, Slayer's +1 tier, Warchief raw main, powders, accessories, and crafted items create a discontinuous search surface. The optimum can move abruptly when one item crosses a speed breakpoint. This is why greedy per-slot scoring performs poorly.

For these families, validate at least:

1. final attack-speed tier and class-specific hits per attack;
2. raw and percent main damage in the actual damage formula order;
3. powder conversion and specials;
4. skill-point equip order and weapon-held skill points;
5. life/mana-steal activation rate at that attack speed;
6. negative HPR cancellation and time-to-death while idle;
7. realistic target contact and multi-beam/multi-hit rate;
8. tomes and crafted-roll assumptions.

A current forum exchange is a useful falsification example. In the [Axiom build thread](https://forums.wynncraft.com/threads/need-help-on-axiom-builds.323788/#post-3707933), a 2026-03-09 reply proposed this [Axiom heavy-melee build](https://wynnbuilder.github.io/builder/#CN09DmeCym056GxFQ-X57Y-1Vm5Svz5tSErw7). A [2026-03-15 correction](https://forums.wynncraft.com/threads/need-help-on-axiom-builds.323788/#post-3708550) rejected it because of earth powders, negative life steal, and loss of Double Vision, then proposed a different eight-item package in prose. This is community evidence, not a proof that the correction is globally optimal, but it demonstrates the relevant failure mode: a plausible displayed heavy-melee number can still be unusable because powder choice, sustain, and a key interaction were optimized incorrectly.

### Spell, spellsteal, and mana-bank builds

Spell builds often have broad plateaus once an executable mana cycle and EHP floor are met, but exact work still matters at integer spell-cost and cycle boundaries. The objective should be damage over a named cycle, not one spell. Spellsteal builds require a main-attack cadence that actually triggers mana steal between spells. Hadal, Fantasia, Oblivion, Lament, Fatal, Stardew-equipped builds, and several Riftwalker builds are highly cycle-sensitive.

Arcanist requires Mana Bank state. Riptide even changes Mana Bank gain from Meteor. Nirvana introduces probabilistic free or refunded casts. Bloodbath can invert the resource problem by spending health. These cannot share one generic `mana_sustain >= 0` constraint.

### HPR cancellation

Warp, Grandmother, Grimtrap, Vengeance, Slayer, and Inferno can require an exact positive-HPR offset. Vengeance is especially nonlinear because it combines strongly positive HPR% with extremely negative raw HPR. The correct objective is not `maximize HPR`; it is usually `meet a safe net-HPR threshold with the least lost damage/EHP`, then optimize the main objective.

### Support, raid, and healing

Guardian Paladin, Ignis Altruism, Lament/Requiem Lightbender, Halcyon Lightbender, Absolution Acolyte, and Architect Trickster are not fairly ranked by personal DPS. Objectives may include redirected damage, ally healing per second, overhealth uptime, damage/defense buff uptime, clone proximity, and survival under raid mechanics. The forum itself contains criticism that a universal DPS objective can mis-rank healing/support builds. Treat party composition and content as inputs.

### Lootrun and mobility

Warp and Moontower, Idol's linked lootrun variant, Stratiformis, and Discoverer target route completion, chest reach, or loot bonus. The correct objectives are time per route, death probability, walk/teleport throughput, loot bonus, and enough damage for route blockers. Stationary boss DPS is secondary.

### Poison and mob grinding

Toxoplasmosis has 3-3 weapon damage and up to 65,000 Poison. Outbreak stores a portion of Poison while the Totem is primed, then Uproot releases it as a cloud. Optimize poison, Totem/Uproot timing, grouping, area coverage, mobility, and survivability. Normal main or spell DPS is almost meaningless here.

### Geometry and execution

Labyrinth needs trap overlap; Revolution needs distance; Riptide needs bounce geometry; Fate needs two linked targets; Architect needs allies near clones; Hanafubuki and Acrobat need aerial/landing uptime; Shadestepper needs positioning; Halcyon needs close Ophanim contact. A paper maximum that assumes 100% positional success is a theoretical upper bound, not an expected result.

## What the forum guide does and does not establish

The maintained [Ultimate Build Guide, direct first post](https://forums.wynncraft.com/threads/the-ultimate-build-guide.320092/#post-3683330) began 2024-12-11. Its first post was observed last edited 2026-08-11 at 15:41 in the forum's displayed time. The audited post contained 129 unique WynnBuilder URLs: 118 Mythic variants and 11 non-Mythic builds. It states that builds are intended as optimal public examples and supplies content tags such as TNA, TCC, NOL, NOTG, WTP, Annihilation, and lootrun use. It also says:

- spell keybinds are needed to reach the highest damage potential;
- the Fruma update removed pre-Fruma builds from the live list;
- users should modify examples for more damage, EHP, or mana sustain;
- not every playstyle is represented;
- guide maintainers accept better builds and update the post.

Important limit: its current `META TRACKER` sections all say `Will be updated later`. Therefore the thread supports `this is a maintained recommended build for this weapon and stated content`, but does not support a complete current ranking of all Mythics. Comments and changelogs also show that trees, sustain, and recommendations change frequently. Treat every builder link as a dated test case, not an eternal optimum.

## Data-quality discrepancies and unresolved gaps

1. **Official wiki mismatches resolved against live v3.7.2.** The live API gives Galleon 60 Strength and 60 Intelligence requirements, Resurgence 80 Defense and 65 Intelligence requirements, and Revenant 6,500 base health. Those values are authoritative for this snapshot even where wiki tables disagree.
2. **Older category pages can be stale.** Some weapon-list pages display damage values that differ from the recently updated Mythic summary and individual item pages. Prefer live API, then current item page and 2.2.2 builder data.
3. **Forum hashes are mutable in interpretation.** A hash encodes selections, but a hosted builder can load them against newer data later. Persist decoded items, powders, skill points, tree nodes, tomes, Aspects, site, and retrieval date.
4. **Forum-rendered links can be abbreviated.** The visible forum text shortens long crafted URLs with `...`. Full `href` targets were recovered from the HTML. Preserve URL fragments exactly because the build state is encoded after `#`.
5. **Crafted builds and guild tomes.** The guide often offers stronger crafted variants and warns that negative displayed skill points may require a guild tome. Crafted recipes and roll ranges require a separate optimization layer. The current local decoder records crafted links but cannot yet expand their six ingredients and recipes.
6. **No controlled performance dataset.** Content tags and `meta` language are maintainer judgments. There is no common boss, Aspect tier, tome set, consumable set, latency, player skill, or sample size across the linked builds.
7. **Aspect and tree decoding is version-sensitive.** Current builder hashes can encode Aspects and ability trees, but old or beta encodings do not always decode against the repository's 2.2.2 constants. Failed decoding is retained as explicit database provenance instead of silently inventing a tree.

## Source ledger

| Source | Authority | Date / state | Exact support |
|---|---|---|---|
| [Official Wynncraft API v3.7.2 changelog](https://docs.wynncraft.com/2026-05-14-v3-7-2) | Official primary | 2026-05-14 | Current documented API surface includes item, ability-tree, Aspect, class and other modules. |
| [List item database entries](https://docs.wynncraft.com/modules/item-recipe/list-items) | Official primary | Docs observed 2026-08-13; enum metadata dated 2026-04-23 | `GET /v3/item/database`, fields, cache, `fullResult`. |
| [List Aspects for a class](https://docs.wynncraft.com/modules/ability-aspect/list-aspects) | Official primary | Observed 2026-08-13 | `GET /v3/aspects/:tree`, tier thresholds/descriptions and class enum. |
| [Version 2.2.2](https://wynncraft.wiki.gg/wiki/Version_2.2.2) | Official wiki | Released 2026-07-10 | Patch baseline. |
| [Mythic Items](https://wynncraft.wiki.gg/wiki/Mythic_Items) | Official wiki | Updated shortly before research; observed 2026-08-13 | Complete current Mythic roster, levels, elements, speed, requirements, Ascensions, cache-only and no-accessory facts. |
| [Identifications](https://wynncraft.wiki.gg/wiki/Identifications) | Official wiki | Updated July 2026; observed 2026-08-13 | ID semantics, steal cadence by speed, spell costs, Major IDs and non-stacking behavior. |
| Individual official item pages linked in every table row | Official wiki | Most last edited May to July 2026 | Current IDs, Major ID descriptions, item-specific role and acquisition. |
| [WynnBuilder source](https://github.com/wynnbuilder/wynnbuilder.github.io) | Tool source | Observed 2026-08-13 | Calculation scope, ability tree, WynnAtlas filters, client-side model, maintained by class builders. |
| [WynnBuilder 2.2.2.0 data](https://github.com/wynnbuilder/wynnbuilder.github.io/tree/master/data/2.2.2.0) | Tool source | Observed 2026-08-13 | `items.json`, `atree.json`, `aspects.json`, `majid.json`, DPS constants and encoding data. |
| [The Ultimate Build Guide, first post](https://forums.wynncraft.com/threads/the-ultimate-build-guide.320092/#post-3683330) | Current community | Started 2024-12-11; first post last edited 2026-08-11 15:41 displayed forum time; observed 2026-08-13 | 129 unique builder links, including 118 Mythic variants and 11 non-Mythic builds; authors, stated families, content tags, tutorials, caveats and placeholder meta trackers. |
| [Axiom heavy-melee proposal](https://forums.wynncraft.com/threads/need-help-on-axiom-builds.323788/#post-3707933) and [correction](https://forums.wynncraft.com/threads/need-help-on-axiom-builds.323788/#post-3708550) | Current community | 2026-03-09 and 2026-03-15 | Concrete example of a heavy-melee recommendation being rejected over powders, negative life steal, and missing Double Vision. |

## Recommended ingestion contract

For each build example, persist a record with these fields:

```text
game_version
retrieved_at
source_forum_url
source_builder_url
source_author
class
weapon
weapon_variant_base_or_ascended
content_tags
declared_build_family
declared_archetype
items[8]
powders
crafted_recipes_and_rolls
assigned_skill_points
equip_order
ability_tree_nodes
aspects_and_tiers
tomes
raid_buffs
consumables
spell_cycle
main_attack_hit_model
optimization_objectives
hard_constraints
expected_metrics
upper_bound_metrics
evidence_code
```

This prevents a visually valid WynnBuilder number from being mistaken for an in-game expected value, and it gives a solver enough context to reproduce heavy melee, cancelstack, tierstack, spellsteal, support, poison, and geometry-dependent results.
