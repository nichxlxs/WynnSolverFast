# Tome optimisation & per-slot level ranges — design

Status: **Phases A, C and D shipped** (guild + all-tomes optimisation live). Phase B (tome roll % + inventory) remains.
Dated 2026-08-13. Supersedes nothing; complements `ARCHITECTURE_PLAN.md`.

---

## Findings that shape the design

Gathered from `data/2.2.3.0/tomes.json` and the current solver, before writing
any code.

**Slots.** `tome_fields` (js/game/shared_constants.js) has 14 entries:
`weaponTome1-2`, `armorTome1-4`, `guildTome1`, plus `lootrunTome1`,
`gatherXpTome1-2`, `dungeonXpTome1-2`, `mobXpTome1-2`. Only the first seven
affect combat; the XP/lootrun slots are out of scope for optimisation.

**Pools.** 156 tomes total: weaponTome 60, armorTome 35, guildTome 6,
plus 55 non-combat. Combat tome levels are 60/80/105 (weapon),
60/100/120 (armour), 100 (guild).

**Every combat tome rolls.** All 101 have `fixID: false`, so an average-roll
percentage is meaningful and should mirror the existing item roll mode.

**No combat tome has a skill-point requirement.** Verified across all 101:
zero `strReq`/`dexReq`/`intReq`/`defReq`/`agiReq`. This is the single most
important fact in this document — see "Why this is tractable" below.

**The combat tome stat surface is small.** Across all 101 tomes the only stats
present are: `hpBonus`, `hprRaw`, `ls`, `damPct`, `mdPct`, `sdPct`, `mdRaw`,
`sdRaw`, the five elemental `*DamPct`, the five elemental `*DefPct`, and the
five skill points. ~23 keys, heavily tiered — ideal for dominance pruning.

**All six guild tomes are skill-point tomes**: Strength/Dexterity/Intelligence/
Defense/Agility (+4 to one attribute) and Rainbow (+1 to all five). There is no
non-SP guild tome, so "use the guild slot for something else" is not a choice
the data supports. The real decision is *which* of the six (7-way incl. none).

**Existing bug to fix as part of Phase C.** `js/solver/engine/search.js:243`
carries a standing TODO: guild-tome "Standard" mode inflates `sp_budget` by +4
*freely distributable*, letting the solver spread the bonus across attributes
(e.g. `[102,102,0,0,0]`). A real Standard tome grants +4 to exactly one
attribute. Phase C replaces the budget hack with real tome selection and
removes the TODO.

**Duplicates are allowed** (confirmed with the repo owner): the same tome may
occupy `weaponTome1` and `weaponTome2`, and repeat across `armorTome1-4`. Tome
slot groups are therefore *multisets*, not sets.

---

## Why this is tractable

Multiplying the leaf space by tome combinations is hopeless: 60² × 35⁴ × 6 ≈
**3.2 × 10¹⁰**, on top of a gear space already in the tens of billions.

The way out is that **tomes have no requirements**, so adding a tome can never
make a build infeasible — only more feasible. That licenses an *admissible*
two-stage scheme:

1. **Enumeration stage.** Fold a single synthetic "best possible tome bundle"
   — the per-stat maximum over the enabled tome set — into the additive
   prechecks and the restriction/EHP suffix bounds. A gearset that fails even
   with the best conceivable tomes genuinely cannot be rescued by any tome
   choice, so pruning it is sound. Crucially this means gearsets that are only
   feasible *with* tomes now survive enumeration, which is the behaviour asked
   for.
2. **Leaf stage.** Solve tomes only at leaves that survive. On the standing
   restriction-heavy benchmark only ~0.3% of feasible leaves pass the ceiling
   gate, so the tome solve runs on a small fraction of the space.

Two further reductions:

- **Dominance pruning on tome pools**, reusing `_prune_dominated_items` with
  the same solver-relevant stat set. Given the tiered structure (Tome of
  Combat Mastery I/II/III …), the pools should collapse hard. Measure before
  assuming.
- **Skill-point tomes need no special handling in the SP kernel.** A tome with
  skill points and no requirements is already classified as a *free item* by
  `calculate_skillpoints` (`has_skp && !has_req` → `free_bonus`), so it never
  enters the activation-order backtracking. No changes to `_bt`.

- **Only guild tomes touch skill points at all.** Measured across the data:
  0 of 60 weapon tomes and 0 of 35 armour tomes grant any of str/dex/int/def/agi.
  So the SP solve — currently ~50% of wall on the restriction-heavy benchmark —
  is completely invariant to weapon and armour tome choice, and varies only over
  the seven guild tome options. This is what keeps Phase D off the SP kernel
  entirely; it lands only on the scoring stage.

### Where the cost actually lands

Being precise about this, because "tomes are cheap" is only true with the
design below and false without it:

| stage | today | with tomes |
|---|---|---|
| SP solve (~50% wall) | 1 per leaf | unchanged for weapon/armour; ×7 only if C2 lands |
| ceiling gate (~30% wall) | 1 per leaf | 1 per leaf, using the optimistic bundle |
| bundle evaluation | — | Pareto front size, at gate-passing leaves only (~0.3%) |
| leaves reaching the pipeline | baseline | **higher** — this is the real cost |

The honest risk is the last row: a looser precheck bound admits more leaves by
design (that is the feature — gearsets only viable *with* tomes must survive).
The multiplier is whatever the bound's looseness turns out to be, and it has to
be measured rather than assumed.

### Measured bundle sizes (2026-08-13, 80% roll)

With `tome_prune_dominated` over the full 23-key combat stat set, then multiset
enumeration and Pareto pruning:

| type | pool | pruned | slots | bundles |
|---|---:|---:|---:|---:|
| guildTome | 6 | 6 | 1 | 6 |
| weaponTome | 23 | 9 | 2 | 45 |
| armorTome | 20 | 7 | 4 | 210 |

Guild alone is trivially cheap. **Weapon + armour + guild together is
210 × 45 × 6 = 56,700 bundles**, which is far too many to evaluate per leaf even
at the ~0.3% of leaves that pass the ceiling gate — that would be hundreds of
millions of evaluations. Phase D therefore needs one more reduction before it is
viable:

**Prune the front against the stats this search actually scores on, not all 23.**
The numbers above dominate over every combat stat, so an armour tome that only
differs in `eDefPct` survives even in a pure spell-damage search that never
reads it.

**Measured 2026-08-13** — scoping the front to a realistic per-search stat set
collapses it:

| scope | guild | weapon | armour | combined |
|---|---:|---:|---:|---:|
| all 23 combat stats | 6 | 45 | 210 | **56,700** |
| spell damage + EHP restrictions | 6 | 6 | 1 | **36** |
| spell damage only | 6 | 6 | 1 | **36** |
| melee damage + EHP | 6 | 6 | 1 | **36** |

36 bundles at the ~0.3% of leaves that pass the ceiling gate is entirely
affordable, so Phase D is viable — provided the front is built from the
search's own stat set (what `_build_dominance_stats` already computes) and
never from the full combat stat list.

### A trap worth recording

Rolled tome IDs are **not** top-level statMap keys. `_apply_roll_mode_to_item`
rewrites the `maxRolls` map in place and leaf assembly reads from there, so
`sm.get('hpBonus')` on a tome returns `undefined`. Anything inspecting tome
stats must go through `tome_stat()`; reading top-level keys silently yields zero
for every rolled stat, which is every stat that matters on weapon and armour
tomes. This was caught by measurement (all pools "pruned" to 1 tome) rather than
by the code failing, so it would have shipped as a silent wrong answer.

Two mitigations to build in from the start:

1. **Precompute tome bundles once per search, not per leaf.** Weapon pairs and
   armour quads are multisets over the enabled pools and do not depend on the
   gear, so their summed stat vectors can be enumerated once, Pareto-pruned, and
   reused at every leaf. Per-leaf cost becomes "iterate a short front" rather
   than "enumerate 60² × 35⁴".
2. **Tighten the bound per tome type** (best weapon bundle + best armour bundle
   separately) rather than one global per-stat max, which would be reachable by
   no single legal combination.

---

## Phase A — per-slot level ranges — DONE 2026-08-13

Today `_build_item_pools` applies one `lvl_min`/`lvl_max` to every slot
(`js/solver/engine/search.js:89`). Phase A adds an optional per-slot override,
defaulting to the global range.

- **Model.** `restrictions.lvl_overrides` — a map of slot → `{min, max}` for
  slots that differ from the global. Absent slot = use global. Slots are the
  eight gear slots (`helmet`, `chestplate`, `leggings`, `boots`, `ring1`,
  `ring2`, `bracelet`, `necklace`).
- **Pool building.** `_build_item_pools` resolves the effective range per slot.
  Note the `ring` pool is shared by `ring1`/`ring2`; when the two rings carry
  different ranges the shared pool must widen to their union and the per-ring
  filter applies during enumeration, or the ring pool must be split. Splitting
  is the honest option — see "Ring caveat" below.
- **UI.** The global `restr-lvl-min` / `restr-lvl-max` inputs stay as the
  default. A collapsible per-slot panel exposes eight optional overrides,
  shown only when set.
- **Encoding.** Solver params are versioned (3-bit, `000` = extension signal +
  4-bit extended version, currently v10) with a 10-bit presence bitmask whose
  bit 9 is free. Phase A bumps to **v11** and uses bit 9 for
  `lvl_overrides`, payload `[8]` slot mask + per set slot `[7]` min + `[7]` max.
  Old links keep decoding at their own version.

### Ring caveat

`_build_item_pools` builds a single `ring` pool consumed by both ring slots,
and `_partition_work` / ring canonicalisation assume the two ring slots draw
from the same ordered pool (that canonicalisation is what avoids enumerating
both orderings of a ring pair). Giving `ring1` and `ring2` different level
ranges breaks that symmetry: the pair (A,B) is no longer equivalent to (B,A).

Options, in preference order:

1. **Restrict the UI to one shared "ring" override** covering both slots. Keeps
   canonicalisation intact, costs nothing, and matches the stated use case
   ("armour 90+, accessories 100+"). **Recommended.**
2. Allow divergent ring ranges and disable ring canonicalisation when they
   differ — correct, but silently doubles the ring subspace.
3. Allow divergence and split into two ordered pools — largest change.

Phase A shipped option 1 and records the constraint. Revisit only if someone
actually wants asymmetric rings.

**Measured** on `solver_mage_gaia_6free_perslot_lvl` (armour 90+, accessories
100+, otherwise the lvl-50 benchmark build): pools 88/83/54/48/45 →
53/53/18/23/21, input space 3.11T → 2.08B. The encoding costs 18 extra base64
characters with all seven slots set and nothing at all when unused; an existing
v10 link still decodes unchanged.

---

## Phase B — tome roll % and available-tome inventory

No search changes; data plumbing and UI only.

- **Average tome ID %.** One number, default **80%**, applied to every rolled
  tome stat via the existing `_apply_roll_mode_to_item` path. Kept separate
  from `current_roll_mode`'s per-group item percentages because tomes are a
  different acquisition model — you roll them far less.
- **Available tomes.** Per-type multi-select of what the user owns. Encoding
  should be "all enabled" as the default flag plus an exception list rather
  than a 101-bit mask, so the common case costs a couple of bits.
- Both feed the pools built in Phase D; until then they only affect the tomes
  already selected in the UI.

## Phase C — guild tome

### C1 — exact tome selection — DONE 2026-08-13

The `sp_budget + 4` hack is gone, along with the `search.js:243` TODO. The
dropdown now lists the seven real choices (Off + five single-attribute tomes +
Rainbow) and each is applied as a real item statMap with the exact
per-attribute skill points, counted as bonus skillpoints by
`calculate_skillpoints` exactly as an equipped tome would be.

`gtome` widened from 2 to 3 bits in the v11 solver encoding. Pre-v11 links
remap on decode: value 2 (Rainbow) maps to the real Rainbow tome; value 1
(the old "Standard (+4 SP)") has no real tome to map to, so it falls back to
Off with a console warning. That direction is deliberate — it can only ever
understate a build, never claim skill points it cannot have.

`test_guild_tome.js` pins the behaviour with 23 assertions, including the case
that discriminates the old model: an item needing dex 100 + int 100 + def 4 is
204 effective points against a 200 budget. The old hack raised the budget and
accepted `assign [0,100,100,4,0]`; now a +4 **Str** tome correctly leaves it
infeasible and only a +4 **Def** tome makes it work.

### C2 — automatic guild tome selection — DONE 2026-08-13

Scaffolding landed and inert (every hook is null unless optimisation is turned
on, so the default path is byte-for-byte the previous behaviour):

- `tome_opt` encoded in the URL (0 off / 1 guild / 2 all).
- `_cfg.tome_bound` folded into `_build_constraint_prechecks`' fixed
  contribution, so a tome-aware precheck stays admissible.
- `assemble_combo_stats` takes an `extra_stats` map, merged last like
  `static_boosts` — the per-candidate tome bundle goes here.
- `_tome_guild_optimistic` used for the leaf SP feasibility gate.

The candidate loop is live. `_evaluate_leaf`'s post-gate pipeline was
extracted verbatim into `_score_leaf_candidate()`; the default path calls it
once (A/B against master at a fixed 30s cap: sp 25.5 vs 25.5 µs/call,
identical leaf counts and best score — no regression), and tome mode loops it
over the enabled candidates, keeping the best feasible score and recording
`guild_tome_idx` in the result entry.

Loop shape at a gate-passing leaf: solve SP per candidate (optimistic-gate
leaves that fail every real candidate simply produce no result), snapshot the
solve, and restore it before each bundle run since greedy and rescue mutate
the arrays in place.

### Phase D — weapon/armour bundle loop — DONE 2026-08-13 (core)

Mode 2 ("All tomes") loops the scoped Pareto bundles as `extra_stats`:

- `_leaf_extra_stats` is read implicitly by `_assemble_combo_stats` and merged
  into the greedy trial via the undo journal, so greedy, rescue, thresholds,
  mana and scoring all see the current bundle without parameter threading.
- The main ceiling gate evaluates at the per-key optimistic bundle; a
  per-bundle ceiling gate (one damage eval each) then filters bundles before
  any greedy runs. Slots holding user-equipped tomes stay locked — bundles
  only fill empty slots, so no double counting.
- Result entries carry `tome_names`; the UI shows them and applies the chosen
  guild tome to the dropdown when a result is clicked. Worker progress
  messages forward the tome fields too (the interim `top5_names` mapping is a
  fixed field list and silently dropped them at first).

**Oracle coverage** (the load-bearing tests):
- `solver_oracle_tome_guild` — production candidate loop vs the plain oracle
  run once per fixed guild candidate, per-gearset max. Exact top-N match; the
  chosen tome raised the oracle build's best score 1,490,273 → 1,499,819.
- `solver_oracle_tome_all` — guild slot fixed, bundle loop vs per-bundle fixed
  ground truth (bundle stats folded into `static_boosts`, which occupies the
  same arithmetic position). Exact top-N match. The guild dimension is covered
  by the first oracle; running both dimensions in one oracle needs 252
  variants and times out, which is a property of the tiny oracle space (the
  cutoff never arms), not of real searches.
- Per-variant top-15 truncation in the merged ground truth is safe: a gearset
  outside a variant's top-15 is beaten by 15 distinct gearsets whose merged
  scores are at least as high.

**Browser-verified**: guild mode picks Strength on the standing mage build,
ALL mode picks 2× Abyssal Combat Mastery III + 4× Vampiric Defensive Mastery
III (best 73,480 → 77,875), throughput 1.1M checked/15s with optimisation on,
zero page errors, mode round-trips through the URL.

**Known limits, deliberate for now:**
- Tomes roll with the ITEM roll groups until Phase B lands the separate tome
  roll % (default 80) — rolled tome stats do not exist without a roll mode.
- No per-user tome inventory yet (Phase B): the enabled set is "everything at
  or below the build's level".
- The bundle front is scoped by `_build_dominance_stats`, the same stat basis
  item dominance trusts. A stat that basis misses is invisible to bundles too.
- Non-damage scoring targets get no ceiling gating (as before), so ALL mode
  on e.g. an `ehp` target runs the candidate loop on every feasible leaf.

### C2 — automatic guild tome selection — design notes

Let the solver choose among the seven rather than the user pre-committing.
Cheap: seven options, and the only tome type that touches the SP kernel (see
below), so it costs at most 7 SP solves per gearset. Should report the chosen
tome in the result so the build stays reproducible.

## Phase D — full weapon/armour tome search

The large one. Depends on B (pools) and C (leaf-stage tome solve shape).

- Build weapon/armour tome pools from the enabled set, dominance-prune them,
  and order them by the same priority machinery as items.
- Fold the optimistic tome bundle into `_fast_constraint_precheck`,
  `_fast_ehp_precheck` and the restriction suffix bounds.
- At surviving leaves, search tome multisets (duplicates allowed) with the
  score-ceiling gate applied per tome candidate.
- Oracle test: a small scenario where the exhaustive gear×tome Cartesian
  product is computable, asserting the gated top-N equals ground truth — the
  same pattern as `solver_oracle_spell_cd`.
- Report tome-space size alongside gear-space size in the funnel output, so
  the two are never conflated in benchmarks.

### Risks

- **The optimistic bundle may be too loose.** If the best-possible bundle is
  far above any real single combination, enumeration pruning weakens and the
  search slows. Mitigation: measure the bound's tightness before committing;
  if loose, tighten it per tome-type (best weapon bundle + best armour bundle
  computed separately rather than one global max).
- **Tome dominance is only valid against the solver-relevant stat set**, the
  same caveat that produced the earlier dominance equality-set bug (see the
  ledger row dated 2026-08-12). Preserve set-adjacent items the same way.
- Phase D changes the meaning of "feasible" in the funnel counters. Re-baseline
  the standing scenarios in the ledger rather than comparing across the change.
