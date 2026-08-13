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
| A8 | Non-lowered atree plans (`scaling_kind == "full"`) | Trees the exporter cannot lower to cached/split | Layer2 rejects; scenario falls back to JS. |

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
| B1 | Defensive objectives (ehp/ehpr/hpr/total_hp) | The ceiling gate discriminates poorly on defensively-flat pools, so most leaves reach the mana simulation | ~9x slower than damage goals. **Profiled** (`SCORE_TRACE=1`, `spell_ehp`, 1.9M leaves): mana 10.2 s + doom precheck 4.6 s = **78% of a 19 s run**, against 2.5 s greedy and 0.4 s SP | Reduce the *number* of leaves reaching the simulation, not the cost per simulation — see the tried-and-rejected note below |

**Tried and rejected: memoizing the mana verdict.** The simulation reads only `DenseCtx::mana_keys`, so its verdict can be cached by exact stat values. First attempt hit 0.7% — `hp`/`hpBonus` are in the key and vary on nearly every defensive item. Narrowing the key was sound and worked: those two reach the verdict only through `has_hp_warning`, which needs an HP cost to exist (a row `hp_cost`, Blood Pact, or an HP-draining buff state), so with none of those they can be dropped. Hit rate went to **66%** and the doom phase fell from 4.6 s to 2.5 s.

It still made the run **2% slower** — 12.05 s versus 11.79 s over repeated 1M-leaf runs. Building and hashing the key costs about what the simulation it replaces costs; the fast mana sim is simply cheap. Two things worth keeping from it: the per-leaf cost is not the problem (the leaf *count* is), and the final mana check is a worse memo target than the doom check, because it runs at the greedy-chosen Int rather than a fixed Int=150, so its key almost never repeats.

| B2 | Custom blends with any negative weight | Monotonicity proof fails → gate + all bounds off | Unpruned (~50-100x slower on wide pools) | Interval (min/max) ceiling bounds |
| B3 | Atrees failing `ceiling_vars_ok` (negative stat-input factors, or `atkTier`/`*ConvBase` var outputs) | All-150 SP is not an upper bound → gate + bounds off (matches JS) | Unpruned | Extremal-per-output ceiling assemble (like the bounded doom) could re-arm the gate soundly |
| B4 | `hp_casting` builds | Gate off (JS parity); HP-sim path heavier | Unpruned | Prove/derive an hp-casting-safe ceiling |
| B5 | Scenarios with maxMana/int var couplings in ≤5-sustain mode | Bounded doom disabled (start-mana monotonicity hole) | Full greedy+mana on mana-dead leaves | Two-sided doom bound on start-minus-end |
| B6 | ehp-family **thresholds** on huge pools | Additive prechecks are weaker than the exact leaf check; ehp thresholds only reject at the leaf | Scenario-dependent | Fold atree scaling into the precheck (JS TODO notes this too) |
| B7 | Warm start on SP-antagonistic objectives (e.g. xpb) | The elite subspace has NO jointly SP-feasible build, so the warm pass seeds nothing | ~0.2s wasted, then organic cutoff | **Tried and rejected**: falling back to a level-ordered top-K also scored 0 (that subspace is infeasible too) and merely doubled the wasted time, so it was not shipped. A real fix needs SP-feasibility folded into the *selection* (pick a jointly-feasible elite set), not a second guess. Cost is ~0.1% of a 180s run, so this is low priority. |
| B9 | Dynamic-row scenarios (declared sliders, until-OOM loops) | Rows are leaf-dependent, so the compiled and dense lowerings are both off and scoring falls back to the uncompiled Obj path | **~500x per greedy trial.** Same 496-leaf space, same ~10,130 trials: plain runs them in 0.02 s on the dense path, a slider scenario takes 10.40 s. Broken down on the same scenario: dense 0.06 s -> Obj-with-compiled-rows 1.75 s (**28x**) -> uncompiled 10.4 s (**a further 6x**) | Hoist the compile out of the trial — see below |

**Where B9's 500x actually is, and one more rejected fix.** The 6x from
compiled to uncompiled looked like the easy half, so I tried compiling each
leaf's injected rows and scoring through the compiled path. It is correct
(identical top-1) and **slower** — 11.0 s against 10.4 s — because a
per-trial `compile_rows` rebuilds every row's `mod_spell`, plan and DPS
analysis, not just its bonuses.

The win needs the compile hoisted out of the trial. In the slider case
`needs_unroll` is false, so the injected rows line up 1:1 with the originals
and the load-time compiled rows stay valid; only the injected tokens'
bonuses need appending per leaf. That means threading extra bonuses through
`score_compiled` rather than swapping one call for another. The remaining
28x (Obj versus dense) is harder still: the dense lowering bakes row damage
in at build time, which is exactly what a per-leaf token changes.

| B8 | The JS engine as a whole || B8 | The JS engine as a whole || B8 | The JS engine as a whole | No dense vectors / cluster bounds / warm start | ~430K leaves per 180s vs Rust ~670M | Port improvements back, or ship Rust via WASM (C1) |

## C. Integration gaps

| # | Gap | Notes |
|---|-----|-------|
| C1 | Rust engine is not shipped to users | **Phase 2 DONE** (see WASM.md): enumeration extracted into the library, `wasm_api::solve`/`search_space` bindgen entry points, wasm-safe clock, deterministic leaf-budget chunking, loadable ~630KB `.wasm`, and `wasm_selftest` proving the browser code path reproduces CLI results byte-for-byte. Remaining: app→fixture serialization (C2), wasm threads via SharedArrayBuffer/COOP+COEP, `wasm-opt` size pass. |
| C4 | Optional GPU runner | Detection DONE (`--features gpu`, `gpu_probe` bin): ranks real adapters, reports exact-f64 (discrete + SHADER_F64, e.g. 3060 Ti) / prescreen-f32 (typical integrated) / cpu-only tiers with graceful fallback. Batched ceiling evaluation per GPU_PLAN.md is the follow-up. |
| C2 | Fixture export is test-harness-driven | **DONE**: `js/solver/engine/rust_bridge.js` builds both fixtures from the same `initMsgBase` the solver already assembles, browser-safe (sandbox access injected as `env`; worker-based case sampling gated behind `env.sampling`). Re-export through it is byte-identical. `search.js` calls it behind `_try_run_solver_search_rust`, falling back to JS workers on any problem. **Verified in real Chromium**: full armor4 space in 346ms (9.3M leaves/s, ~651x the JS engine) with an identical best build. |
| C3 | Rust top-15 tie membership at the boundary | Score sets match JS exactly; which of several EQUAL-scoring builds occupies the last slot can differ (insertion order). Documented, not a correctness issue. |

## Remaining work, in the order I'd tackle it

1. **Speed, not coverage.** Every section-A gap the engine used to refuse is
   now supported; what remains is that dynamic-row scenarios (B9) score on
   the uncompiled Obj path and pay ~500x per greedy trial for it. Two
   tried-and-rejected attempts are recorded above (B1's mana memo, and the
   assumption that row *construction* was B9's cost) — both were measured
   rather than reasoned about, and both were wrong.
2. ~~**wasm threads**~~ — **addressed by partitioning** (see WASM.md): one
   ordinary worker per core, split by first-slot offset, no
   `SharedArrayBuffer` or cross-origin isolation needed. Exact (verified at
   2..8 partitions on three fixtures), ~1.8x at 4-way rather than 4x because
   each partition re-derives its own score cutoff, and gated on search size
   because worker startup would otherwise dominate a short solve. True
   shared-cutoff threading still wants SharedArrayBuffer.
3. ~~**wasm threads (old note)** (SharedArrayBuffer + COOP/COEP): full core scaling in
   the browser on top of the single-threaded engine already shipping.
3. **A6 dynamic sliders / A8 non-lowered ability trees**: both are
   exporter-capability questions rather than engine ports; worth scoping
   before committing.
4. **B-section speedups**: B1 (defensive objectives, mana-sim bound) has
   the most headroom; B2/B3 need interval-style bounds to prune at all.

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
