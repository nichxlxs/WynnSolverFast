# Solver support & performance matrix

Tracking list of (A) inputs the engines cannot solve today, (B) inputs that
solve correctly but noticeably slower because the fast paths switch off, and
(C) integration gaps. "Rust engine" = the enum/score kernel; "JS engine" =
the in-browser worker solver (the reference implementation). Items are
ordered roughly by expected user impact within each section.

## A. Not supported

### Rust engine only (falls back to the JS engine — correct, ~100-1500x slower)

| # | Feature | Where it appears | Notes |
|---|---------|------------------|-------|
| ~~A1~~ | Loop brackets — **SUPPORTED** (count and until-OOM) | Combo editor `[loop]` sections | A count loop's iteration count is a static constant, so `unroll_count_loops` expands the body at load and everything downstream stays on the fast loop-free path. An **until-OOM** loop's length depends on the leaf's own mana trajectory, so its markers are kept and the rows are unrolled *per leaf* (`unroll_loops_dynamic` + a `compute_recast_penalties` re-run), mirroring `eval_combo_damage_with_bp`. `mana_check_passes` implements the JS rule that a mana warning terminates such a loop rather than failing the build. Cross-checked end to end: an until-OOM loop that sustains to the 255-iteration safety cap scores **bit-identically** to a count-255 loop (`3.48257942933359921e+08`), and those are two independent code paths — static unroll at load versus dynamic unroll per leaf. |
| ~~A2~~ | Buff states — **SUPPORTED** (unless a slider is declared) | Combos toggling buff uptime windows | The gap was never the *full* sim: the JS **worker** runs `simulate_combo_mana_fast`, which already models buff states (duration expiry, mana-regen suppression, continuous drain, `compute_delay` one-shot drain at activation, `next_action` deactivation). The Rust fast sim did not, so it mana-rejected builds the JS accepts. It now mirrors it and is validated bit-exact against the JS fast sim on all 53 `mana_sim_check` scenarios. A buff state that declares a `slider_name` still hard-fails — see A6. |
| ~~A3~~ | Blood Pact — **SUPPORTED** (unless a slider is declared) | Shaman HP-cost casting + BP boost injection | The health-cost payment branch is in the fast sim now, so the mana verdict is exact. Damage is only affected when `damage_boost.slider_name` is set (that is what `inject_blood_pact_boosts` injects); with no slider declared, injection adds nothing and the damage rows are unchanged, so those builds solve. **This previously produced silently wrong answers** — the exporter shipped `health_config` and the engine never read it, so BP builds were mana-rejected and dropped from results rather than refused. |
| ~~A6~~ | Dynamic sliders — **SUPPORTED** | `damage_boost.slider_name` / `buff_state.slider_name` | These are the JS `has_dyn` condition: `eval_combo_damage_with_bp` injects simulation-derived values into the damage rows **per leaf** via `inject_blood_pact_boosts`. Such scenarios now run the whole chain per leaf — simulate, unroll if needed, recompute recast penalties, inject, score — on the Obj path with the compiled rows, dense lowering, ceiling gate and every bound switched off (all of them assume parse-time-constant rows). Verified live: the same buff state with no slider declared scores identically to no buff state at all (`8.192296800e+06`), and declaring the slider moves it to `8.222605726e+06`. |
| ~~A7~~ | ~~Multi-partition scenarios~~ | — | **NOT A GAP (corrected).** The Rust engine enumerates the whole space in one process (work-stealing over first-slot offsets), and checks the identical leaf count as the JS run (3,100,680 on the restricted-melee scenario). The one JS-only top entry is the **seeded current UI build** (`_eval_current_build` → `_insert_top5`), which the solver inserts as a baseline and which no enumeration would produce. |
| ~~A8~~ | Non-lowered atree plans (`scaling_kind == "full"`) — **no shipped tree hits one** | Trees the exporter cannot lower to cached/split | Was four causes; three are now lowered. **Dotted / multiplier-map var outputs** (`damMult.Surge`) were refused out of caution, but Rust already handled them: `eval_var_effects` routes outputs through `merge_stat` exactly as the JS does, `merge_into` recombines the partitions the same way, and the dense lowering declines them (`nested_prefix`) so they take the validated Obj path — proved by `var_effect_check`, 17 synthetic effect lists bit-exact against the JS. **Const/var key collisions** are lowered when every contribution is integral: the full pass interleaves const and var terms in effect order while the split sums each partition apart, and float addition is not associative — but integers below 2^53 sum exactly in *any* order, so the two agree bit-for-bit. `test_scaling_association.js` pins both halves (fractional terms diverge within a few trials; 200k integral trials never do) and the 2^53 boundary the rule is stated against. **Three causes are still `full`** — `var_has_prop_io`, a bare multiplier-root var output, and a non-integral const/var collision — **but none of them occurs in any ability tree Wynncraft has shipped**: `test_atree_lowering_coverage.js` classifies the all-nodes tree of every class across all 35 data versions in the repo (2.0.1.1 → 2.2.2.0, 175 class trees, 337 variable effects) through the real `atree_collect_stat_effects`, and finds zero. All three conditions are monotone in the node set, so a clean superset means every real build is clean too. Doing the port would be writing code for a shape the game has never produced; the test is the standing watch instead — if a data update introduces one, it fails and names the class and ability. |

### Both engines (nobody solves these today)

| # | Feature | Notes |
|---|---------|-------|
| A9 | Tome slot search | Tomes are fixed inputs; the solver never searches tome combinations. |
| A10 | Weapon slot search | The weapon is fixed; solving across weapons means separate runs per weapon. |
| A11 | Powder optimization | Powders are taken as-is on the fixed weapon/armor inputs; no search over powder sets. |
| A12 | Crafted-item ingredient search | Crafted items participate as fixed statmaps; the solver does not search ingredient/recipe space. |

## B. Supported but slow (correct results, fast paths off or weak)

| # | Case | Why slow | Speed today | Possible fix |
|---|------|----------|-------------|--------------|
| B1 | Defensive objectives (ehp/ehpr/hpr/total_hp) | The ceiling gate discriminates poorly on defensively-flat pools, so most leaves reach the mana simulation | Was ~9x slower than damage goals, with mana + doom **78% of the run**. The doom precheck now tests the highest Int the leaf can actually *reach* instead of a flat 150: **1.46x** on `spell_ehp` (126,035 against 86,154 leaves/s), mana 22.25 s → 9.13 s, greedy trials 31.1M → 21.9M | Reducing the *number* of leaves reaching the simulation is what works — the per-leaf cost is not the problem (see the rejected memo below) |

**The doom precheck was asking the wrong question.** It simulates one leaf at
maximum Int to prove no SP allocation can make it mana-feasible — mana
feasibility is monotone in Int, so if the best case is dead, every case is.
It used a flat `Int = 150`. But **every point that ends up in Int is either
added by the greedy or moved there by the mana rescue**, and both raise
`base_sp[2]` and `total_sp[2]` together under `base_sp[2] < 100` and
`total_sp[2] < 150`, while the rescue can only relocate points the greedy
placed. So between them they add at most `remaining`, and the reachable
ceiling is

    total_sp[2] + min(remaining, 100 - base_sp[2], 150 - total_sp[2])

which is a valid upper bound and a much tighter one. On `spell_ehp` it
averages **59**, and it is below 150 on **100% of doom checks** — on every
benchmark, not just that one. The old check was asking whether builds could
sustain with skill points they cannot have, so it passed leaves the mana
check would later reject, after paying for the greedy.

**1.46x** on `spell_ehp` (126,035 against 86,154 leaves/s over a 60 s window).
The work moves out of the expensive stages: mana 22.25 s → 9.13 s, greedy
trials 31.1M → 21.9M, doom 9.68 s → 14.41 s (more leaves reach it, same cost
each). Damage scenarios are unaffected — byte-identical counters, and
`armor4`'s full space is within noise.

Validated by making the tripwire actually cover it. `SCORE_DENSE_CHECK=1` now
falls a doom rejection through to the full pipeline and asserts the pipeline
rejects it too; it previously did that only for the *bounded* doom, leaving
the Int ceiling — the part that decides how many leaves get cut — unchecked on
the scenarios where it fires most. The Obj-path scoring exit had no tripwire at
all. With both closed, 436,712 rejections on `spell_ehp` pass. Single-threaded
`scored` counts and top-15 are identical to `SCORE_DOOM_INT=150` on every
fixture, which is the sharp test: a wrongly doomed leaf is one that would
otherwise have been scored.

**What is left in B1 is irreducible per-leaf work.** After the change the
60 s `spell_ehp` window is doom 14.41 s, mana 9.13 s, dense leaf fill 6.36 s,
greedy 5.71 s. The doom sim is ~5.6 µs per leaf and there is no repetition to
exploit: keying it the way `SimMemo` keys the B9 simulation gives **0.0%** —
1,958,602 distinct keys in 1,959,464 checks. That is the difference between
the two memos. B9's repeats *within* a leaf across greedy trials, which move
one skill point at a time; this one would have to repeat *across* leaves,
where every item swap changes `mr`/`ms`/`maxMana`/`hp`. It also confirms why
the earlier attempt below only reached 66% by dropping stats from the key.

Going further needs the ceiling gate to discriminate on defensively-flat
pools — it prunes 20% here against far more on damage goals — which is the
same missing capability as B3, not more tuning of this path.

**Tried and rejected: memoizing the mana verdict.** The simulation reads only `DenseCtx::mana_keys`, so its verdict can be cached by exact stat values. First attempt hit 0.7% — `hp`/`hpBonus` are in the key and vary on nearly every defensive item. Narrowing the key was sound and worked: those two reach the verdict only through `has_hp_warning`, which needs an HP cost to exist (a row `hp_cost`, Blood Pact, or an HP-draining buff state), so with none of those they can be dropped. Hit rate went to **66%** and the doom phase fell from 4.6 s to 2.5 s.

It still made the run **2% slower** — 12.05 s versus 11.79 s over repeated 1M-leaf runs. Building and hashing the key costs about what the simulation it replaces costs; the fast mana sim is simply cheap. Two things worth keeping from it: the per-leaf cost is not the problem (the leaf *count* is), and the final mana check is a worse memo target than the doom check, because it runs at the greedy-chosen Int rather than a fixed Int=150, so its key almost never repeats.

| ~~B2~~ | Custom blends with any negative weight — **now gated** | The all-150-SP assemble maximizes every target, which *under*-states a negative term, so it was not an upper bound and the gate and all bounds were switched off | Was unpruned. Now on `armor4` (3,223,584 leaves): **2.2x** — 10.1 s against 22.4 s, scoring **78 leaves instead of 202,536**. Smaller spaces: `hp2` 1.31x (1,533 of 1,872 gated), a `total_hp`-negative blend on `spell2` 1.16x | Two-sided ceiling — see below |

**Two-sided ceiling.** Every sub-target is non-decreasing in SP (the
assumption `supports_ceiling` already rested on, backed by
`ceiling_vars_ok`), so a term with a negative weight is maximized where its
target is *smallest* — the leaf's pre-greedy SP — not largest. Evaluating
each term at its own extreme still bounds the blend from above:

    for w >= 0:  w * target(sp)  <=  w * target(150)
    for w <  0:  w * target(sp)  <=  w * target(sp_min)

since `sp_min <= sp <= 150` componentwise for every state the greedy and the
mana rescue can reach. Summing preserves the inequality.

Validated the only way a bound can be: `SCORE_TWO_SIDED=0` restores the
unpruned behaviour, and the full-precision top-15 is **identical** with the
gate on and off across four scenarios (a damage/HP blend, a pure negative
`total_hp` blend, a `total_hp`/`ehp` blend on `hp2`, and the same blend on
`armor4`'s full 3,223,584-leaf space) — while the gate prunes up to 68% of
leaves outright and cuts leaves *scored* by 2,600x.

The mid-tree bound stays off for these: its machinery evaluates one
assembled state and cannot express a two-sided bound. Only the leaf gate is
two-sided.

First attempt was *slower* despite gating 82% of leaves, because it built
the materialized Obj base on every leaf for a value only the Obj fallback
needs — `base` went from 0.00 s to 0.13 s on `hp2`. The dense path assembles
both SP states from the lowered leaf and needs no Obj base at all.

| B3 | Atrees failing `ceiling_vars_ok` (negative stat-input factors, or `atkTier`/`*ConvBase` var outputs) | All-150 SP is not an upper bound → gate + bounds off (matches JS) | Unpruned | Extremal-per-output ceiling assemble (like the bounded doom) could re-arm the gate soundly |
| B4 | `hp_casting` builds | Gate off (JS parity); HP-sim path heavier | Unpruned | Prove/derive an hp-casting-safe ceiling |
| B5 | Scenarios with maxMana/int var couplings in ≤5-sustain mode | Bounded doom disabled (start-mana monotonicity hole) | Full greedy+mana on mana-dead leaves | Two-sided doom bound on start-minus-end |
| B6 | ehp-family **thresholds** on huge pools | Additive prechecks are weaker than the exact leaf check; ehp thresholds only reject at the leaf | Scenario-dependent | Fold atree scaling into the precheck (JS TODO notes this too) |
| B7 | Warm start on SP-antagonistic objectives (e.g. xpb) | The elite subspace has NO jointly SP-feasible build, so the warm pass seeds nothing | ~0.2s wasted, then organic cutoff | **Tried and rejected**: falling back to a level-ordered top-K also scored 0 (that subspace is infeasible too) and merely doubled the wasted time, so it was not shipped. A real fix needs SP-feasibility folded into the *selection* (pick a jointly-feasible elite set), not a second guess. Cost is ~0.1% of a 180s run, so this is low priority. |
| B9 | Dynamic-row scenarios (declared sliders, until-OOM loops) | Rows are leaf-dependent, so the per-leaf simulation runs on every greedy SP trial | **Closed: 16.7x** (0.24 s against 4.00 s), top-15 identical at every step. Dense lowering for injected tokens 4.00→2.55 s; not cloning the row set per trial 2.55→0.87 s; memoizing the simulation 0.87→0.24 s. Reproduce with `fixtures/score_slider.json` (see `gen_bench_fixtures.js`) against `enum_spell2` | Nothing measurable left here. The scenario now scores in the same ballpark as a static one |

**Getting the dense path to serve injected tokens.** The lowering bakes row
damage in at build time, which is exactly what a per-leaf token changes.
Four things had to hold:

- **Slots reserved up front.** `DScratch` is sized from the key universe, so
  a stat key only an injected token produces has nowhere to live.
  `DenseCtx::build` takes the injectable keys and interns them.
- **`row_canon` bypassed.** It shares one per-cast result between rows the
  lowering proved identical — but injected values are per-leaf and can differ
  between exactly those rows, so a cache hit could return another row's
  damage.
- **All three bonus shapes.** Injected `damMult`/`defMult` bonuses go through
  the same journaled `dam_ops`/`def_ops` application as static ones; plain
  stats become slot writes applied right after the row's own, matching the
  Obj path's `comp.bonuses.chain(extra)` order.
- **A load-time gate.** A slider whose registry entry carries a *prop* bonus
  rewrites the row's spell, which the lowering baked in — no slot can
  represent that, so those scenarios keep the Obj path. Same for `atkSpd` and
  dotted stat keys, which the lowering refuses for static rows too.

Validated by the existing net rather than a new one: `SCORE_DENSE_CHECK=1`
asserts dense against Obj on **every trial of every leaf**, and it earned its
place immediately — it caught that only the trial site had been wired and the
final-score site still used the static path.

**Then the rows stopped being cloned.** Dense left the per-leaf simulation at
1.57 s of 2.16 s, so the next measurement was of the simulation — and it said
the simulation was not the problem. Splitting that 1.57 s three ways:

    sim 0.32 s | clone rows 0.56 s | inject (clone again) 0.46 s

Two thirds of it was `Vec<Row>` copying, twice per trial, for rows that never
change: `dynamic_damage_rows` cloned the parse-time rows and then
`inject_blood_pact_boosts` cloned them again to append the boost tokens. Every
row carries its spell as a `serde_json::Value`, so each clone is a deep JSON
copy.

`dynamic_damage_rows_split` hands the rows back as a `Cow` and puts the
injected tokens in a side vector. Without a per-leaf unroll the rows *are* the
parse-time ones, and neither fast scorer reads `row.tokens` — the compiled and
dense paths both take injected bonuses as a separate argument — so nothing has
to be copied at all. **2.55 s → 0.87 s.** The uncompiled fallback still needs
the tokens in the rows and materializes them.

`mana_sim_check` pins the split form to the merged one: merging its output
must reproduce `dynamic_damage_rows` row-for-row and token-for-token (10,419
comparisons, up from 9,902). That matters because only the merged form is
checked against the JS.

**A gate that one caller skipped.** `score_kernel` built its `DenseCtx`
directly instead of through the load-time injectability gate, so it could
build one for a scenario the solver refuses — and since `dense_dynamic_score`
treats an unreservable key as unreachable, it panicked on the first leaf
instead of reporting a diff. The gate is now `dense_injectable_keys`, and both
builders go through it.

**Then the simulation stopped running most of the time.** With the copying
gone, the 0.87 s was the simulation proper. Its only per-trial input is
`dense_sim_obj`'s stat map — a dozen or so values — so two trials agreeing on
those produce identical injected tokens. Greedy moves one skill point at a
time and most of those moves do not touch a stat the simulation reads: **89%
of trials repeat an earlier input** (10,678 trials, 1,169 distinct).

`SimMemo` keys on exactly that map plus `has_arcanes`, and caches the compiled
`DenseRowExtra`, so a hit skips the stat map, the simulation, and the bonus
compile. It is direct-mapped over a fixed 512 slots, so memory is bounded on
any run length, and it stores the full key and compares it — a hash collision
costs a recompute, it can never return another trial's answer. **0.87 s →
0.24 s.**

Checked the way an admissible-looking cache has to be: `SCORE_DENSE_CHECK=1`
asserts dense against Obj on every trial of every leaf and is clean; results
are identical across 1/2/4/8 threads (the memo is per-worker, so a stale entry
would show up as a thread-count-dependent answer); and rebuilding with the
table shrunk to **one slot**, so every single trial collides, still produces
identical output with the per-trial check clean.

| B8 | The JS engine as a whole | No dense vectors / cluster bounds / warm start | ~430K leaves per 180s vs Rust ~670M | Port improvements back, or ship Rust via WASM (C1) |

## C. Integration gaps

| # | Gap | Notes |
|---|-----|-------|
| C1 | Rust engine is not shipped to users | **Phase 2 DONE** (see WASM.md): enumeration extracted into the library, `wasm_api::solve`/`search_space` bindgen entry points, wasm-safe clock, deterministic leaf-budget chunking, loadable ~630KB `.wasm`, and `wasm_selftest` proving the browser code path reproduces CLI results byte-for-byte. Remaining: app→fixture serialization (C2), wasm threads via SharedArrayBuffer/COOP+COEP, `wasm-opt` size pass. |
| C4 | Optional GPU runner | Detection DONE (`--features gpu`, `gpu_probe` bin): ranks real adapters, reports exact-f64 (discrete + SHADER_F64, e.g. 3060 Ti) / prescreen-f32 (typical integrated) / cpu-only tiers with graceful fallback. Batched ceiling evaluation per GPU_PLAN.md is the follow-up. |
| C2 | Fixture export is test-harness-driven | **DONE**: `js/solver/engine/rust_bridge.js` builds both fixtures from the same `initMsgBase` the solver already assembles, browser-safe (sandbox access injected as `env`; worker-based case sampling gated behind `env.sampling`). Re-export through it is byte-identical. `search.js` calls it behind `_try_run_solver_search_rust`, falling back to JS workers on any problem. **Verified in real Chromium**: full armor4 space in 346ms (9.3M leaves/s, ~651x the JS engine) with an identical best build. |
| C3 | Rust top-15 tie membership at the boundary | Score sets match JS exactly; which of several EQUAL-scoring builds occupies the last slot can differ (insertion order). Documented, not a correctness issue. |

## Remaining work, in the order I'd tackle it

1. ~~**Speed, not coverage**~~ — B9 is closed (16.7x) and B1's doom precheck is
   1.46x. What is left in section B needs new bound *shapes* rather than
   tuning: B3 and B4 have no admissible ceiling at all today, and B5 needs a
   two-sided doom bound. Six tried-and-rejected attempts are recorded above;
   each was plausible and each was wrong until measured.
2. ~~**wasm threads**~~ — **addressed by partitioning** (see WASM.md): one
   ordinary worker per core, split by first-slot offset, no
   `SharedArrayBuffer` or cross-origin isolation needed. Exact (verified at
   2..8 partitions on three fixtures), ~1.8x at 4-way rather than 4x because
   each partition re-derives its own score cutoff, and gated on search size
   because worker startup would otherwise dominate a short solve. True
   shared-cutoff threading still wants SharedArrayBuffer.
3. ~~**A6 dynamic sliders / A8 non-lowered ability trees**~~: A6 is supported.
   A8's three remaining causes are unreachable in every ability tree the game
   has shipped, and `test_atree_lowering_coverage.js` now watches for that
   changing — port them if it ever fails, not before.
4. **A9-A12** (tome / weapon / powder / ingredient search) are the only
   remaining *capability* gaps, and neither engine does them. They change the
   enumeration space and the UI in both, so they are a product call rather
   than a port.

## Verification status

`mana_sim_check` also validates the **composed** function the engine calls
per leaf (`dynamic_damage_rows`), not just its parts — checking the pieces
separately would not catch them being wired in the wrong order. Doing that
turned up a real difference: `eval_combo_damage_with_bp` only re-runs
`compute_recast_penalties` when the rows actually carry loop markers, so a
loop-free combo keeps the penalties computed at snapshot time.

Everything in section B is covered by the bit-exact validators (11 fixtures ×
5 levels + per-trial dense assertions + on/off top-15 equivalence), plus
`mana_sim_check` (48 scenarios × every returned field, compared on raw f64
bit patterns). Section A items hard-fail loudly at load — `solve_json`
returns `{"error": ...}` and the web solver falls back to the JS workers.

The one case where that was *not* true — `health_config` being exported but
never read, so buff-state and Blood Pact scenarios silently ran the
state-free sim — was found and fixed while porting the sim. Absence of a
reader is now checked by construction: `ScoringCtx::load` inspects
`health_config` and refuses anything the loop-free path cannot reproduce.

**Var-effect outputs**: `var_effect_check` validates the split plan's
variable half (`eval_var_effects`) against the JS on 17 synthetic effect
lists. It exists because relaxing the exporter's `full` triggers needed
evidence, not an argument — and it turned up that the collision check for
dotted keys had to look *inside* the constant partition's multiplier map,
since `const_scaled` stores those under the root and a plain `has(key)`
would have missed the collision entirely.

**Bound admissibility under drain**: a mana-draining buff state makes the
Int=150 doom precheck unsound — drain is a percentage of `max_mana`, and
`max_mana` rises with Int, so more Int means more absolute drain and the
Int=150 sim no longer upper-bounds feasibility. The precheck is switched off
whenever any buff state drains mana, independently of `mana_doom_ok` (which
only covers atree var-effect coupling).

**Threading determinism** (re-verified): `enum_kernel` at 1 / 2 / 4 / 8
threads produces identical `checked` counts and identical top-15 *score
sets* on `spell2`, `armor4` (3,223,584 leaves) and `hp2`. Only tie
membership at the cutoff boundary varies (C3), as does `bound_pruned` —
both are consequences of cutoff-discovery order, not of the search space.
