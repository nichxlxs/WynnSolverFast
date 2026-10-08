# Current build-family benchmark design

## Purpose

This note defines a source-backed benchmark programme for testing item-pool
pruning across current Wynncraft build families. The benchmark target is not
"reproduce the most popular Discord build". It is:

1. preserve the best solver result for a named objective and playability
   contract;
2. cover every class and official ability archetype;
3. stress the mechanical build families that make item dominance contextual;
4. run from one fixture definition through both the JavaScript and Rust
   engines; and
5. make pruning failures reproducible as permanent regression cases.

The current benchmark suite has six family seeds and good large-space
calibration, but it is not class and archetype complete. The proposed first
release is a 15-scenario archetype floor plus family-specific stress scenarios.

## Source boundary

The evidence used here is limited to primary or direct sources:

- the local Blue's Builds Discord corpus and the exact Discord messages and
  WynnBuilder links it preserves;
- the official Wynncraft ability-tree API capture in
  [`build-database/ability-trees.json`](build-database/ability-trees.json);
- the versioned WynnBuilder and Wynncraft data under [`../data`](../data);
- the solver contracts and Rust bridge in this repository; and
- the community-authored build links themselves.

The official ability-tree capture defines three archetypes per class. Its
descriptions are useful for deciding what a benchmark must retain:

| Class | Archetype | Official mechanical emphasis |
|---|---|---|
| Archer | Boltslinger | Close-range speed and burst through many hits |
| Archer | Sharpshooter | Range and aimed power shots |
| Archer | Trapper | Delayed damage, traps, beasts, and crowd control |
| Warrior | Fallen | High-risk damage that becomes stronger under pressure |
| Warrior | Battle Monk | Close-combat spell combinations and mobility |
| Warrior | Paladin | Resistance, ally support, and surviving large hits |
| Mage | Arcanist | Burst damage and Mana Bank management |
| Mage | Riftwalker | Mobility and damage that ramps during a fight |
| Mage | Light Bender | Healing, ally support, and buffs |
| Assassin | Shadestepper | Timed positional burst from concealment |
| Assassin | Trickster | Clone-driven control and misdirection |
| Assassin | Acrobat | Fast aerial spell combinations |
| Shaman | Summoner | Totem and summon damage |
| Shaman | Ritualist | Mask states, buffs, and flexible transitions |
| Shaman | Acolyte | Health sacrifice for damage and healing |

The local threshold profiles already express most of these distinctions as
proposed research contracts in
[`build-database/threshold-profiles.json`](build-database/threshold-profiles.json).
They explicitly warn that their numbers are starting points, not official or
universal meta thresholds. That warning remains binding for this benchmark.

## Corpus readiness

Read-only queries against the local Discord database on 15 August 2026 found:

- 1,272 distinct builds in the five current class forums;
- 1,269 successful native decodes;
- 1,069 builds with at least one successful evaluation;
- 293 builds with browser evidence, of which 284 were observed nonbroken;
- 35 links whose native WynnBuilder header resolves to `2.2.3.0`;
- 33 builds with an exact authored design label in the `2.2.3*` family; and
- 786 current-forum builds without a final `family_style`, or 61.8%.

The complete corpus remains valuable for historical family coverage, but a
build being posted in a current class forum does not prove that it was designed
for the current patch. Version, browser health, family classification, and
build quality are independent evidence fields. The database design documents
that separation in [`build-database/README.md`](build-database/README.md).

Exact-current author evidence is also uneven. The current snapshot has useful
evidence for 11 of the 15 archetypes, but no clean exact-current authored seed
for Shadestepper, Light Bender, Battle Monk, or Paladin. It has current links
for Sharpshooter, Trapper, Trickster, and Acolyte, but some lack a complete
rotation or playability contract.

Therefore the initial suite should label every seed as one of:

- `current_exact_reviewed`: exact `2.2.3*`, source contract reviewed, native
  link preserved;
- `current_exact_discovery`: exact `2.2.3*`, useful for coverage but not yet a
  benchmark truth;
- `current_forum_standardized`: older authored version from a current forum,
  separately decoded and validated against `2.2.3.0`; or
- `historical_native`: intentionally retained at its authored version for a
  mechanical regression, never pooled with current results.

## Strong current seed evidence

These are the strongest exact-current starting points found in the corpus.
They are candidate anchors, not claims of global optimality.

| Coverage use | Direct evidence | What the authored evidence contributes |
|---|---|---|
| Archer Boltslinger fast hybrid | [Eschaton update](https://discord.com/channels/696616686928920598/1492948724245332090/1536832672993648700), [build](https://wynnbuilder.github.io/builder/#CX0cVAwCufeuCnRZm9kuDz9s88WeW218nxvD-m6MLBRPBhAcri+rlcjiPCmX0udpkuhHt-U+7Tk4Eq0) | WTP testing, an on-paper damage figure, and an explicit sustain tradeoff |
| Archer Sharpshooter | [Rage Grandmother update](https://discord.com/channels/696616686928920598/1508951549555114095/1535328383406186546), [build](https://wynnbuilder.github.io/builder/#CX0cV416976OCMCc6c61AIW8W9nIf6PuGFvEXDH2Wl6H0J2JoZnZn5n5naY441C9C9F6F6N4N4JAIG4OcSHSHSHSHSHaf818XZKDBcHweR0ypPNa+f+-1FSNuG) | Exact-current ranged damage seed; contract still needs manual review |
| Archer Trapper negative control | [Labyrinth update](https://discord.com/channels/696616686928920598/1492896149135163473/1535806495902404728), [build](https://wynnbuilder.github.io/builder/#CXG4OaaZWnObObObec81ty8m8971ZnAnAnAvCH2kvHWHIE26ZLYLYL2RY4Sp9KpDF2CJk8k8k8k8k8WJaW8mCvYuYuYuYuY0EH2Y0paBYBYBYBYB2p49WLP4oxcpiW51WsNpjRe3VU001) | Exact-current tree and equipment, plus an explicit mechanics warning. Keep as a broken-mechanics control until replaced |
| Assassin Acrobat | [Weathered starter](https://discord.com/channels/696616686928920598/1493348246322544861/1493348246322544861), [build](https://wynnbuilder.github.io/builder/#CX0vAAw4Whe37kYEc5BwCbZfSKGXKSyhf87t10ldyNirD+L0) | Current spell seed, damage range, difficulty rating, and EHP versus sustain alternative |
| Assassin Trickster | [Architect update](https://discord.com/channels/696616686928920598/1492965916818936028/1535398285907333221), [build](https://wynnbuilder.github.io/builder/#CX013ln9U8MzDpv5QAEfE9r9iEH23OGl1-m6MLhQLhgAkjiErfsic1L2uj7kQYJ-Fldqlp4LD0) | Exact-current user build for clone-oriented coverage; not yet evidence of meta status |
| Mage Riftwalker cancelstack | [Trance starter](https://discord.com/channels/696616686928920598/1492897519804616855/1492897519804616855), [build](https://wynnbuilder.github.io/builder/#CX0rkkvipycp3ZRgpUnRg71C9C9qDqDO7O7JAIG4mamaGtGtWTWTCf81KU49SCohHw4D8Q0e-TBTVpa5i0) | Explicit `2.2.3_2`, tank and damage variants, and a mana-sustain tradeoff |
| Mage Arcanist lootrun spell | [Brushfire starter](https://discord.com/channels/696616686928920598/1535072909360627862/1535072909360627862), [build](https://wynnbuilder.github.io/builder/#CX0X3HuJGL4D6146G0n45QdGeIGP0uzulcM-d3) | Meteor to Mana Bank to Ice Snake rotation and a healing dependency. This is a budget lootrun seed, not a maximum-damage seed |
| Shaman Acolyte discovery | [Olympic update](https://discord.com/channels/696616686928920598/1513322771785060514/1537436125550092348), [build](https://wynnbuilder.github.io/builder/#CX0F3HuJ8tge3kcYE60WW75KuS2QJBsEc30+tDl7VVKG0) | Exact-current Acolyte coverage, but no complete current contract yet |
| Shaman Summoner spell | [Olympic starter](https://discord.com/channels/696616686928920598/1536048071010549831/1536048071010549831), [build](https://wynnbuilder.github.io/builder/#CX0oqG2E8t2VJH052nmoHEdWsqYSk4S90yFOTntvbAg0) | Timed Aura, Uproot, Hummingbirds, and Commander cycle plus raid-use notes |
| Shaman Ritualist spell | [Shatterspam starter](https://discord.com/channels/696616686928920598/1536564453020270652/1536564453020270652), [build](https://wynnbuilder.github.io/builder/#CX0X6GyGCnheZtlDE1KmN2OO0-6jysmb-PdKkRAl0J0) | Totem to Aura cycle, Awakened charging, mana offhand, and raid-support notes |
| Shaman melee and summon interaction | [Transfiguration starter](https://discord.com/channels/696616686928920598/1523124999828017242/1523124999828017242), [build](https://wynnbuilder.github.io/builder/#CX0t1HO10u1+QG05mC266GFILpDSfzUt2bux99h0) | Constant melee, Aura to Haul cadence, and both mana-steal and life-steal priorities |
| Warrior Fallen spell | [Hero starter](https://discord.com/channels/696616686928920598/1505283155903647846/1505283155903647846), [build](https://wynnbuilder.github.io/builder/#CX0oq0xG8t2uQ05Ge0IlHmlHhcYuKAeK0uVUs-Ts4p0) | Blood Pact activation condition and two authored damage rotations |

The four exact-current gaps should be filled by a fresh Discord review. Until
then, explicitly versioned gap-fill fixtures can come from:

- Shadestepper heavy melee: [Inferno](https://discord.com/channels/696616686928920598/1492892524090753134/1492892524090753134), [build](https://wynnbuilder-beta.github.io/builder/#CT04KQuqziXphZ6ESQQuu047WaqaqZmgf1N+d-a+hr0AM);
- Light Bender lootrun: [Halcyon](https://discord.com/channels/696616686928920598/1502585245386412052/1502585245386412052), [build](https://wynnbuilder.github.io/builder/#CX0g2Jvyf1d4tuq2geZfJCTIhJEGoupKSfV+YRDtr3D);
- Battle Monk and Paladin lootrun: [Idol](https://discord.com/channels/696616686928920598/1492879315476414586/1492879315476414586), [build](https://wynnbuilder-beta.github.io/builder/#CT0Q0Jvyf1a4JvOh9b71O8O8O8O8O8O8JA9G4WXWXWXWXWXWXCfa0qw4sZeg957Z10j7VCBCPJp2z0); and
- Paladin heavy hybrid: [Apocalypse](https://discord.com/channels/696616686928920598/1496909233793274097/1496909233793274097), [build](https://wynnbuilder-beta.github.io/builder/#CT0O0lvW3ycphZREY5lvyuW778FXoOSxy6dz10l7ixtRwp2).

These gap-fill records must remain at their authored native version or be
stored as separate standardized-current derivatives. A benchmark generator
must never silently reinterpret the original URL as current.

## Benchmark matrix

### Spell-row identifier contract

The `spell_node_id` field is easy to misread. For a regular spell row it is
the spell's `base_spell` identifier, not the ability's node ID in the tree.
This behaviour is defined by
[`combo/codec.js`](../js/solver/combo/codec.js) and the reserved values are
defined by [`constants.js`](../js/solver/constants.js). The following base
rows were verified against the versioned
[`2.2.3.0` ability data](../data/2.2.3.0/atree.json):

| Class | `1` | `2` | `3` | `4` |
|---|---|---|---|---|
| Archer | Arrow Storm | Escape | Arrow Bomb | Arrow Shield |
| Warrior | Bash | Charge | Uppercut | War Scream |
| Mage | Heal, or Arcane Transfer when replaced | Teleport | Meteor, or Ophanim when replaced | Ice Snake, or Frozen Tornado when replaced |
| Assassin | Spin Attack, or Lacerate when replaced | Dash | Multihit, or Backstab when replaced | Smoke Bomb |
| Shaman | Totem | Haul | Aura | Uproot, or Switch Masks when replaced |

The important extended rows for the proposed profiles are:

| Class and archetype | Extended `spell_node_id` values selected by the tree |
|---|---|
| Archer Trapper | `7` Basaltic Trap, `8` Call of the Hound, `9` Ivyroot Mamba, `10` Murder Flock, `14` Chilling Snare, and `12` Extinction Event when selected |
| Archer Sharpshooter | `5` Twain's Arc and `6` Crepuscular Ray when selected |
| Warrior Battle Monk | `5` Counter, `14` Cyclone, and `12` Zenith Stance when selected |
| Warrior Paladin | `6` Shield Strike, `10` Buried Light, and `11` Judgement when selected |
| Warrior Fallen | `10` Bloodlust Damage and `13` Beyond Salvation when selected |
| Mage Light Bender | `5` Lightweaver, `7` Freezing Sigil, `9` Crystallize, and `11` Dawn when selected |
| Mage Riftwalker | `8` Dimensional Tear and `13` Gravitational Collapse when selected |
| Assassin Trickster | `8` Bamboozle, `9` Malicious Mockery, `13` Deflagrate, and `14` Shadow Projection when selected |
| Assassin Acrobat | `6` Shurikens, `7` Jasmine Bloom, `9` Ripple, `10` Swan Dive, and `16` Serpent's Garden when selected |
| Shaman Summoner | `6` Puppet Damage, `7` Crimson Effigy, `12` Hummingbirds' Song, and `13` Patchwork Abomination when selected |
| Shaman Ritualist | `5` Totemic Shatter and `14` Sundered Skies when selected |
| Shaman Acolyte | `8` Twisted Tether, `9` Bleeding, `10` Blood Sorrow, and `11` Eldritch Call when selected |

The same numeric value can resolve to a different replacement in a mutually
exclusive tree. A fixture must therefore store the tree and validate the
resolved row name after tree merge. It must not infer a row name from the
number alone.

Reserved rows used by the benchmark harness are:

| `spell_node_id` | Meaning |
|---:|---|
| `0` | One ordinary main attack. Quantity is a hit count |
| `118` | Melee Time. Quantity is seconds and the engine derives expected hits from final attack speed |
| `119` | Cancel Corrupted |
| `115`, `116` | Loop end and loop start |
| `117` | Add Flat Mana, a manual scenario injection rather than an automatically derived class mechanic |
| `121` to `125` | Quake, Chain Lightning, Curse, Courage, and Wind Prison |
| `126` | Mana Reset |

### Archetype floor

The first layer is one scenario per official archetype. The objective is a
solver-observable scalar. Playability belongs in hard constraints and the
authored rotation, not in an arbitrary weighted score.

`M` below means the JavaScript and Rust search engines model the condition.
`P` means the benchmark must store it as a proxy or validate it outside the
search objective. Numeric floors are populated from reviewed source evidence,
then the checked-in research profiles, then an explicitly labelled
seed-relative rule. They are not hard-coded in this design table.

| ID | Objective and exact row template | `M`: engine-modelled gates | `P`: proxy, pinned state, or post-search assertion |
|---|---|---|---|
| `archer_boltslinger_fast_hybrid` | `combo_damage`; authored counts containing `118` Melee Time and `1` Arrow Storm, with `3` Arrow Bomb only if the reviewed cycle uses it | Exact combo mana, `ehp_no_agi`, `mainAttackRange`, final attack speed through row `118`, equipment and tree legality | Close-range hit fraction, target count, Old Keeper's Ring trigger rate, and potion-free sustain |
| `archer_sharpshooter_power` | `combo_damage`; `3` Arrow Bomb fallback, or `5` Twain's Arc and `6` Crepuscular Ray only when the selected tree and reviewed rotation use them | Exact spell cost and mana timeline, EHP, range-related item stats, seed-relative sustain | Focus generation and decay, aim success, range-to-target, and Crepuscular aerial uptime |
| `archer_trapper_delayed` | `combo_damage`; a reviewed horizon containing `3` Arrow Bomb plus selected `7`, `8`, `9`, `10`, and `14` delayed or companion rows | Direct row damage, exact mana, EHP, item legality, and selected-tree properties | Trap placement, detonation, overlap, snare duration delivered, companion contact, and target geometry. The current Labyrinth seed remains a negative control |
| `warrior_fallen_blood_pact` | `combo_damage`; the Hero source supplies `1`, `4`, `3` cycles, with `119` only at an authored Corrupted cancel boundary | Mana and Blood Pact HP payment, rejection of lethal HP casts, combo order, EHP and total HP | Corrupted level and uptime unless dynamically derived by the selected data, safe remaining-HP margin above merely nonlethal, exit recovery, and the exact partial-payment damage bonus unless parity-tested for that tree |
| `warrior_battle_monk_combo` | `combo_damage`; reviewed `1` Bash, `2` Charge, `3` Uppercut, `4` War Scream, and `14` Cyclone when selected | Mana, EHP, `spd`, combo order, and damage exclusion of movement-only rows | Counter proc rate, close-range contact, movement execution, and Zenith uptime. Exact-current seed is still missing |
| `warrior_paladin_tank` | `combo_damage` under a defensive contract; base `1`, `3`, `4` plus selected `6` Shield Strike or `10` Buried Light | High `ehp_no_agi`, `total_hp`, seed-relative `hpr` or `ehpr`, mana, and direct row damage | Mantle charges, Holy Power generation, ally buff value, damage prevented, and Judgement charge. Exact-current seed is still missing |
| `mage_arcanist_bank` | `combo_damage`; authored `3` Meteor x5, `1` Arcane Transfer with damage excluded, then `4` Ice Snake. Use `117` only for an explicitly assumed bank transfer | Spell costs, recast penalties, row order, EHP, and the mana timeline after any explicit `117` injection | Mana Bank charge, hit count, cap, and transfer amount are not derived by the current simulator. `117` makes the assumed transfer reproducible but does not validate it |
| `mage_riftwalker_ramp` | Current Trance seed: `combo_damage` on `118` Melee Time. A spell variant must be a separate fixture using its own reviewed `3`, `4`, and optional `8` rows | Exact final attack speed through `118`, `atkTier` equality for cancelstack, EHP, HPR, range, and direct tree stat effects | Time Dilation stack acquisition and uptime, target contact, and encounter ramp length unless a dynamic state in the selected data explicitly models them |
| `mage_light_bender_heal` | `total_healing`; `1` Heal, with `3` Ophanim included for its mana and damage timeline. Report personal `combo_damage` separately | Healing calculation, spell costs, mana, EHP, and any explicitly pinned tree toggle | Ophanim survival and contact, heal target count, Lightweaver contact, party benefit, and Crystallize charge. Exact-current seed is still missing |
| `assassin_shadestepper_burst` | `combo_damage`; `3` Backstab plus `2` Dash or Vanish on the timeline. Use a separate `118` fixture for heavy melee | Direct Backstab damage, mana, EHP, HPR, and exact attack speed in the separate melee fixture | Behind-target success, Mark and Satsujin acquisition, Vanish uptime, enemy behaviour, and execution latency. Exact-current seed is still missing |
| `assassin_trickster_clones` | `combo_damage`; reviewed `3` Multihit and `4` Smoke Bomb plus selected `8` Bamboozle or `14` Shadow Projection | Direct row damage, exact mana, EHP, and pinned clone-related tree sliders or toggles | Clone survival, clone hit rate, target density, enemy targeting, and ultimate uptime |
| `assassin_acrobat_spellspam` | `combo_damage`; `1` Lacerate is the core damage row, with reviewed `2`, `3`, `4`, and selected Acrobat rows on the timeline | Exact mana, recast penalties, EHP, `spd`, direct row damage, and tree legality | Aerial uptime, contact per Lacerate strike, movement execution, and encounter geometry |
| `shaman_acolyte_blood_pool` | `combo_damage` or `total_healing` according to reviewed source; `3` Aura plus selected `8`, `10`, or `11` Blood Pool spenders | Direct row damage or healing, ordinary mana costs, EHP, total HP, and static tree modifiers | Blood Pool generation, spend, and uptime are not the generic HP-casting simulation; overhealth, tether or beam contact, and ally healing remain proxies |
| `shaman_summoner_rotation` | `combo_damage`; authored `3` Aura, `4` Uproot, and `12` Hummingbirds, with `1` Totem and selected `6` Puppet Damage on the horizon | Ordinary mana, EHP, direct summon-row damage, and a pinned Active Puppets slider | Puppet and Effigy spawn time, survival, target contact, Commander charge, and raid geometry |
| `shaman_ritualist_masks` | `combo_damage`; authored `1` Totem x2 then `3` Aura, with `4` mask switches and `5` Totemic Shatter when selected | Mana, EHP, `spd`, row order, and the chosen static mask or Awakened toggle | Awakened charging, three-mask transition timing, chant cooldown, and offhand switching. Lower and Silent Ballet phases require separate fixed-weapon fixtures because weapon search and mid-combo swaps are unsupported |

This matrix deliberately separates mixed mechanical variants. A Riftwalker
cancelstack fixture is not also the Riftwalker spell fixture, and a
Shadestepper Backstab fixture is not also its heavy-melee fixture. They share
an archetype label but require different objectives and dominance contexts.

### What both engines model, and what they do not

The shared capability boundary follows
[`SOLVER.md`](../js/solver/engine/SOLVER.md) and the current
[`Rust support matrix`](../rust/sp_kernel/SUPPORT_MATRIX.md).

Both engines model:

- fixed weapon, powders, tomes, aspects and ability tree, searchable equipment
  and tome slots, skill-point legality, level and item requirements;
- `combo_damage`, `total_healing`, `poison`, and the direct defensive or item
  scoring targets documented by the solver;
- combo order, repeated rows and loops, spell recast costs, mana regeneration,
  mana steal, attack-speed conversion for `118`, and the chosen
  `allow_downtime` rule;
- direct stat thresholds such as `ehp_no_agi`, `total_hp`, `hpr`, `ehpr`,
  `spd`, `atkTier`, `mainAttackRange`, and `finalSpellCost1` through
  `finalSpellCost4`; and
- HP-cost casting, lethal-cast rejection, tree modifiers, buff states and
  dynamic sliders when those mechanics are actually encoded in the selected
  ability data and exported fixture.

Some values are modelled only after the fixture pins an assumption. Examples
are Active Puppets, Marks, Focus, a mask toggle, a hit count, a slider value,
or a flat mana injection. The scorer can evaluate that state exactly, but it
does not thereby prove that a player can create or sustain the state in the
encounter.

The current engines do not derive target geometry, movement execution,
positional success, trap overlap, companion survival and contact, party
benefit, encounter clear time, offhand switching, Holy Power, Blood Pool, or
Mana Bank charge from gameplay. They also do not search weapons, powders, or
crafted ingredients. These remain fixture metadata, separate fixed-weapon
phases, or post-search validation. In particular, an arbitrary minimum
remaining HP above zero is not a search restriction today: HP casting rejects
a lethal sequence, while a safer health margin must be asserted from the
simulation result after evaluation or approximated with a stronger
`total_hp` contract.

### Mechanical family stress layer

One archetype scenario cannot expose all contextual item interactions. Add at
least the following family strata, even where they overlap an archetype seed:

| Family stratum | Minimum distinct contexts | Objective and critical contract |
|---|---:|---|
| Heavy melee | 3 classes | Main-attack packet damage or main-attack DPS. Preserve the seed's slow final speed, EHP, range, and HPR. Do not impose universal nonnegative life steal |
| Tierstack | 3 classes | Main-attack DPS with an exact final attack-tier target, EHP, HPR, and range |
| Rawstack | 2 classes | Main-attack DPS with fixed fast weapon, no negative-tier dependency, EHP, HPR, and range |
| Cancelstack | 2 classes | Main-attack DPS with exact tier equality and a tier certificate. Test one-lower-tier sensitivity |
| Fast hybrid | 2 classes | Damage from one combined melee and spell timeline with exact mana simulation |
| Heavy hybrid | 2 classes | Heavy main packet plus authored spell cycle, with slow-speed and mana contracts |
| Sustained spell | all 5 classes | Primary spell damage inside an executable repeated cycle, with no mana warning and an EHP floor |
| Spellsteal | 2 classes | Spell cycle with explicit melee windows, mana-steal cadence, EHP, and missed-steal recovery case |
| Healing and support | Mage, Shaman, Warrior | Healing where modeled, otherwise damage under high defensive floors. Report party-state assumptions separately |
| Poison | Shaman plus one historical control | `poison` objective, EHP, movement, and an explicit application and uptime assumption |
| Lootrun and mobility | at least 2 classes | Combat objective with walk-speed and loot floors. Do not optimize walk speed without a route-blocker damage contract |

Blue's own search guide distinguishes archetype, weapon, unique playstyle,
element code, Major ID, and raid purpose. It explicitly lists spell,
tierstack, fast hybrid, and heavy melee as playstyles. The wider server also
defines rawstack and cancelstack separately:

- [Blue's search guide](https://discord.com/channels/696616686928920598/962377198315130941/1006865719616602214)
- [heavy-melee definition](https://discord.com/channels/696616686928920598/747975427557031966/1504968668621176882)
- [rawstack definition](https://discord.com/channels/696616686928920598/747975427557031966/1504969005159682088)
- [cancelstack definition](https://discord.com/channels/696616686928920598/747975427557031966/1504969592693461003)
- [tierstack definition](https://discord.com/channels/696616686928920598/747975427557031966/1535093830960029737)

This is why family and archetype must remain separate fields in a fixture.

## Objective construction

### Prefer authored cycles when available

Both engines now support combo rows, count and until-out-of-mana loops, buff
states, Blood Pact, dynamic sliders, and total healing. The current capability
boundary is documented in
[`../rust/sp_kernel/SUPPORT_MATRIX.md`](../rust/sp_kernel/SUPPORT_MATRIX.md).
There is no need to reduce every spell benchmark to a context-free tooltip
number.

Build the combo as follows:

1. include the source-authored damage rotation in order;
2. mark movement, setup, resource-reset, and utility casts as damage-excluded
   when they belong on the timeline but are not the target damage packet;
3. leave them mana-included unless the game mechanic actually excludes them;
4. use the exact mana or HP simulation as a hard feasibility gate; and
5. record target count, expected hit fraction, geometry, state, and horizon as
   scenario metadata even when the current scorer cannot realize them.

### Single-spell fallback

Use a single-spell objective only when the source does not define a trustworthy
cycle. For example, a Mage spellspam fallback can maximize Meteor damage while
the combo repeats Meteor often enough for the exact mana check to enforce a
sustainable cycle. The same pattern applies to Arrow Bomb, Uppercut, Multihit,
or Aura.

The fallback must still include:

- the relevant movement or setup spell as a damage-excluded row when it is part
  of normal execution;
- `allow_downtime = false` for a sustained build unless the authored contract
  explicitly permits downtime;
- an EHP constraint; and
- any family-specific attack-tier, HPR, range, or resource constraint.

Single-spell score is a controlled gear-comparison surrogate. It is not a
claim about full encounter DPS.

### Avoid unstable weighted objectives

Do not start with a weighted blend of damage, EHP, mana, movement, and healing.
Their units differ by orders of magnitude, and a weight change can redefine
the apparent optimum. Use one primary scalar and express playability as hard
constraints. Custom blends remain useful as later sensitivity scenarios, not
as the primary correctness oracle.

## Playability constraints

The checked-in threshold profiles propose 18,000 non-Agility EHP for general
combat, 20,000 for heavy melee and tierstack, and higher values for support.
The corpus calibration has not established universal hard floors, so a
benchmark should store both an absolute contract and its source.

Use this precedence:

1. explicit author or reviewer requirement for the named build and activity;
2. accepted versus rejected human evidence in the same family and version;
3. a family-specific checked-in research profile; then
4. a clearly labelled seed-relative floor.

For an initial seed-relative floor, retain at least 80% of the seed's
nonobjective defensive or mobility value, but never below the named research
profile. This 80% value is a benchmark design parameter, not a discovered meta
law. Store it in the fixture so later calibration can replace it.

Important family rules:

- Heavy melee must not use `life_steal >= 0` as a universal rule. Slow attack
  cadence changes the realized cost of negative life steal. Preserve the
  seed's final speed and use a seed-relative life-steal floor until an
  encounter-aware effective-life-steal condition exists.
- Tierstack and cancelstack require exact speed-breakpoint tests. An `atkTier`
  floor alone can admit a different family.
- Spellsteal requires main-attack windows on the same timeline as spells.
- HP-casting builds require the HP simulator and a nonlethal sequence. Any
  safer minimum remaining-health margin is a post-search assertion, not a
  mana proxy or a currently searchable restriction.
- Lootrun builds need a combat floor as well as walk speed and loot bonus.
- Support builds need a defensive floor and a separate report of unmodeled
  party utility. Personal DPS must not be presented as total support value.
- Geometry and state assumptions are first-class metadata. A static build
  benchmark cannot prove trap overlap, positional success, clone survival, or
  target contact.

## One shared fixture contract

JavaScript and Rust fixtures should be generated from one versioned manifest,
not maintained independently. A scenario record should contain:

```json
{
  "schema_version": 1,
  "scenario_id": "mage_arcanist_bank_2_2_3",
  "game_version": "2.2.3.0",
  "source": {
    "discord_message_url": "...",
    "builder_url": "...",
    "evidence_status": "current_exact_reviewed"
  },
  "taxonomy": {
    "class": "mage",
    "archetypes": ["arcanist"],
    "primary_archetype": "arcanist",
    "family_primary": "spellspam",
    "family_tags": ["sustained_spell", "mana_bank"],
    "activity": "raid"
  },
  "seed": {
    "weapon": "...",
    "items": ["..."],
    "powders": ["..."],
    "tomes": ["..."],
    "ability_tree": ["..."],
    "roll_profile": "uniform_median"
  },
  "objective": {
    "target": "combo_damage",
    "combo_rows": ["..."],
    "allow_downtime": false
  },
  "constraints": [
    {"stat": "ehp_no_agi", "op": "ge", "value": 18000}
  ],
  "assumptions": {
    "horizon_seconds": 30,
    "target_count": 1,
    "hit_fraction": 1.0,
    "geometry": "single stationary target",
    "unmodeled": []
  },
  "recovery": {
    "unlock_policy": "stratified_masks_v1",
    "rng_seed": 1
  }
}
```

The generator should:

1. load the manifest's exact game-data version;
2. decode and validate the immutable source URL without accepting migration;
3. create a separate standardized-current derivative only when requested;
4. construct the same solver snapshot used by the browser;
5. export the Rust fixture through
   [`../js/solver/engine/rust_bridge.js`](../js/solver/engine/rust_bridge.js);
6. evaluate the locked seed in JavaScript;
7. assert Rust score-set parity for the same fixture; and
8. store source and contract hashes in the generated snapshot.

The weapon and powders remain fixed because neither engine searches weapons or
powder configurations. Crafted equipment can be fixed in a seed, but ingredient
search is outside the current solver. Those boundaries must appear in the
benchmark result.

## Reconstruction and ablation experiments

"Remove random items" can mean two different experiments. They must not be
mixed.

### Completion recovery

Unlock equipment slots from the known build while leaving every seed item in
the eligible pool. This asks whether the solver can recover the seed score or
find something better.

For each scenario:

- run all eight one-slot unlocks;
- run all 28 two-slot unlocks;
- for each unlock count from three through eight, run every mask when cheap,
  otherwise a deterministic stratified sample;
- ensure armour, accessories, duplicate rings, set pieces, Major IDs, crafted
  pieces, and negative-ID pieces each appear in held-out masks; and
- derive the pseudo-random stream from the scenario ID, manifest hash, and an
  explicit seed.

The success condition is score recovery under the same contract, not exact
item-tuple recovery. A different build with an equal or greater score is a
successful recovery.

### Knockout resilience

Blacklist one or more seed items from the candidate pool. This asks whether the
solver can find a viable alternative. It cannot be compared directly with the
original seed because that seed has been made infeasible.

For every knockout mask, run pruned and unpruned solvers with the identical
blacklist. Compare those paired optima. Existing human ablation and near-miss
records should be tested before purely random knockouts because they target
known family interactions.

### Paired pruning levels

Every recovery or knockout fixture should run at least:

- pruning off;
- certificate-only or conservative pruning;
- the current sensitivity threshold; and
- one deliberately more aggressive threshold for experimental analysis.

For a completed search, require exact best-score parity and best-score-set
parity between pruned and unpruned runs. Tied item membership may differ if the
score set is identical. Record:

- eligible item counts before and after pruning by slot;
- total Cartesian combinations;
- dominance certificates or pruning reasons;
- best score and score regret;
- top-15 score overlap;
- whether the known seed score was recovered;
- time and leaves to first reach the seed score;
- final result at a fixed time cap; and
- JS versus Rust parity.

A time-capped run is performance evidence only. It cannot prove that pruning
preserved the global optimum because the two searches may explore different
prefixes. Exact correctness claims require completed paired searches or an
independent exhaustive oracle.

## Statistical design

Do not tune pruning sensitivity on the same build variations used to report
success.

- Group every variation from the same Discord thread into one split to prevent
  source leakage.
- Stratify by class, primary archetype, family, version, crafted status, Major
  ID, set use, and negative-ID presence.
- Keep at least one whole class-family context as a final holdout when tuning a
  new pruning threshold.
- Report macro results by stratum as well as the pooled result, so a large
  spell stratum cannot hide failures in poison or cancelstack.
- Promote every counterexample to a permanent regression fixture before
  changing the rule.

Random trials estimate a failure rate but do not prove zero risk. If zero
failures occur in `n` independent completed trials, the usual one-sided 95%
upper bound is approximately `3 / n`. For example, zero failures in 1,000
trials supports an empirical upper bound near 0.3%, not a mathematical proof
that no theoretical optimum can be lost. Worst-case counterexamples matter
more than the mean pruning percentage.

## Acceptance gates

A scenario becomes a benchmark truth only when all of these pass:

1. exact source version and immutable URL are stored;
2. source status is not workshop-unreviewed, reported broken, or unexplained;
3. equipment, weapon, powders, tomes, tree, and roll profile decode;
4. the locked seed passes its own legality and playability contract;
5. the objective matches the authored build's damage or utility engine;
6. JS and Rust produce the same locked-seed score set;
7. at least the one-slot and two-slot completion matrix runs; and
8. any standardized-current derivative is stored separately from native
   historical evidence.

The Labyrinth Trapper mechanics warning is useful as an explicit negative
control, but it should not satisfy these positive-seed gates until the reported
mechanic is resolved or a replacement is reviewed.

## Recommended implementation order

1. Add the shared scenario-manifest schema and validator.
2. Import and review the strongest exact-current seeds above.
3. Refresh Discord evidence for the four exact-current archetype gaps.
4. Build the 15 archetype-floor fixtures and assert locked-seed JS/Rust parity.
5. Add deterministic one-slot and two-slot completion recovery.
6. Add family stress fixtures for heavy melee, tierstack, rawstack,
   cancelstack, fast hybrid, heavy hybrid, spellsteal, poison, support, and
   lootrun.
7. Add knockout tests from existing human ablations, then deterministic random
   knockouts.
8. Only after this baseline is frozen, compare conservative, current, and
   aggressive sensitivity levels on train and holdout strata.

This ordering gives pruning experiments a current, versioned, mechanically
diverse correctness harness without treating any Discord build as a proven
global optimum.
