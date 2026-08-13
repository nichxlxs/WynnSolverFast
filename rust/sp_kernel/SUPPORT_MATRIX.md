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
| A1 | Loop brackets — **count loops now SUPPORTED**; until-OOM still not | Combo editor `[loop]` sections | A count loop's iteration count is a static constant, so `unroll_count_loops` expands the body at load (mirroring `_unroll_loops_pure`) and everything downstream stays on the validated loop-free path. Verified: a count-2 loop produces byte-identical results (funnel counters + full top-15) to the manually duplicated combo. **until-OOM loops** still reject with a clear error. Both simulations handle them (termination on either warning, plus the 255-iteration safety cap, all validated), and `mana_check_passes` implements the JS rule that a mana warning terminates such a loop rather than failing the build. What is still missing is the *damage* side: the iteration count is leaf-dependent, so the rows must be unrolled per leaf via `unroll_loops_dynamic` with a `compute_recast_penalties` re-run. Both are ported and validated; the routing is not built, so `unroll_count_loops` still refuses them at load. |
| ~~A2~~ | Buff states — **SUPPORTED** (unless a slider is declared) | Combos toggling buff uptime windows | The gap was never the *full* sim: the JS **worker** runs `simulate_combo_mana_fast`, which already models buff states (duration expiry, mana-regen suppression, continuous drain, `compute_delay` one-shot drain at activation, `next_action` deactivation). The Rust fast sim did not, so it mana-rejected builds the JS accepts. It now mirrors it and is validated bit-exact against the JS fast sim on all 53 `mana_sim_check` scenarios. A buff state that declares a `slider_name` still hard-fails — see A6. |
| ~~A3~~ | Blood Pact — **SUPPORTED** (unless a slider is declared) | Shaman HP-cost casting + BP boost injection | The health-cost payment branch is in the fast sim now, so the mana verdict is exact. Damage is only affected when `damage_boost.slider_name` is set (that is what `inject_blood_pact_boosts` injects); with no slider declared, injection adds nothing and the damage rows are unchanged, so those builds solve. **This previously produced silently wrong answers** — the exporter shipped `health_config` and the engine never read it, so BP builds were mana-rejected and dropped from results rather than refused. |
| A6 | Dynamic sliders (`damage_boost.slider_name` / `buff_state.slider_name`) | Atree sliders fed from the simulation | The one genuinely remaining piece of A2/A3. `eval_combo_damage_with_bp` injects simulation-derived values into the damage rows **per leaf**, so boost tokens stop being parse-time constants — which both `compile_rows` and the dense lowering assume they are. `inject_blood_pact_boosts` and `extract_slider_names` are ported and validated; what is missing is routing these scenarios onto the Obj path with a per-leaf sim. Refused by name today. |
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
| B1 | Defensive objectives (ehp/ehpr/hpr/total_hp) | Ceiling gate discriminates poorly (defensively-flat pools), so most leaves run greedy+mana; mana sim dominates | ~9x slower than damage goals (406K vs ~3.7M leaves/s) | Cluster-level mana prefilter; cheaper sustain model |
| B2 | Custom blends with any negative weight | Monotonicity proof fails → gate + all bounds off | Unpruned (~50-100x slower on wide pools) | Interval (min/max) ceiling bounds |
| B3 | Atrees failing `ceiling_vars_ok` (negative stat-input factors, or `atkTier`/`*ConvBase` var outputs) | All-150 SP is not an upper bound → gate + bounds off (matches JS) | Unpruned | Extremal-per-output ceiling assemble (like the bounded doom) could re-arm the gate soundly |
| B4 | `hp_casting` builds | Gate off (JS parity); HP-sim path heavier | Unpruned | Prove/derive an hp-casting-safe ceiling |
| B5 | Scenarios with maxMana/int var couplings in ≤5-sustain mode | Bounded doom disabled (start-mana monotonicity hole) | Full greedy+mana on mana-dead leaves | Two-sided doom bound on start-minus-end |
| B6 | ehp-family **thresholds** on huge pools | Additive prechecks are weaker than the exact leaf check; ehp thresholds only reject at the leaf | Scenario-dependent | Fold atree scaling into the precheck (JS TODO notes this too) |
| B7 | Warm start on SP-antagonistic objectives (e.g. xpb) | The elite subspace has NO jointly SP-feasible build, so the warm pass seeds nothing | ~0.2s wasted, then organic cutoff | **Tried and rejected**: falling back to a level-ordered top-K also scored 0 (that subspace is infeasible too) and merely doubled the wasted time, so it was not shipped. A real fix needs SP-feasibility folded into the *selection* (pick a jointly-feasible elite set), not a second guess. Cost is ~0.1% of a 180s run, so this is low priority. |
| B8 | The JS engine as a whole | No dense vectors / cluster bounds / warm start | ~430K leaves per 180s vs Rust ~670M | Port improvements back, or ship Rust via WASM (C1) |

## C. Integration gaps

| # | Gap | Notes |
|---|-----|-------|
| C1 | Rust engine is not shipped to users | **Phase 2 DONE** (see WASM.md): enumeration extracted into the library, `wasm_api::solve`/`search_space` bindgen entry points, wasm-safe clock, deterministic leaf-budget chunking, loadable ~630KB `.wasm`, and `wasm_selftest` proving the browser code path reproduces CLI results byte-for-byte. Remaining: app→fixture serialization (C2), wasm threads via SharedArrayBuffer/COOP+COEP, `wasm-opt` size pass. |
| C4 | Optional GPU runner | Detection DONE (`--features gpu`, `gpu_probe` bin): ranks real adapters, reports exact-f64 (discrete + SHADER_F64, e.g. 3060 Ti) / prescreen-f32 (typical integrated) / cpu-only tiers with graceful fallback. Batched ceiling evaluation per GPU_PLAN.md is the follow-up. |
| C2 | Fixture export is test-harness-driven | **DONE**: `js/solver/engine/rust_bridge.js` builds both fixtures from the same `initMsgBase` the solver already assembles, browser-safe (sandbox access injected as `env`; worker-based case sampling gated behind `env.sampling`). Re-export through it is byte-identical. `search.js` calls it behind `_try_run_solver_search_rust`, falling back to JS workers on any problem. **Verified in real Chromium**: full armor4 space in 346ms (9.3M leaves/s, ~651x the JS engine) with an identical best build. |
| C3 | Rust top-15 tie membership at the boundary | Score sets match JS exactly; which of several EQUAL-scoring builds occupies the last slot can differ (insertion order). Documented, not a correctness issue. |

## Remaining work, in the order I'd tackle it

1. **Per-leaf dynamic rows** — the remainder of A6 and the until-OOM half of
   A1. Both need the same thing: rows that differ per leaf, either because
   boost tokens are injected from that leaf's simulation or because the loop
   ran a different number of iterations. Every piece is ported and validated
   (`inject_blood_pact_boosts`, `extract_slider_names`,
   `unroll_loops_dynamic`, `compute_recast_penalties`); what is missing is
   routing those scenarios onto the Obj path, since `compile_rows` and the
   dense lowering both assume parse-time-constant tokens. Expect them
   *correct but slow* rather than fast.
2. **wasm threads** (SharedArrayBuffer + COOP/COEP): full core scaling in
   the browser on top of the single-threaded engine already shipping.
3. **A6 dynamic sliders / A8 non-lowered ability trees**: both are
   exporter-capability questions rather than engine ports; worth scoping
   before committing.
4. **B-section speedups**: B1 (defensive objectives, mana-sim bound) has
   the most headroom; B2/B3 need interval-style bounds to prune at all.

## Verification status

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
