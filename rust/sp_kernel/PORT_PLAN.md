# P2.4 — Rust leaf-scorer port plan

Goal: enum_kernel scores feasible leaves end-to-end (combo_damage target),
reproducing the JS worker's top-N bit-for-bit, then benchmarks spell-spam
workloads that the JS engine cannot finish (readme_spell_wide: 20.07B
search, JS covers ~430K in 180s).

## Layer status

- **Layer 1 — damage core: DONE (2026-08-12).** `score_kernel` reproduces
  `compute_combo_damage_totals` 192/192 bit-exact on melee + spell
  differential fixtures (`SOLVER_EXPORT_SCORE`). Parity traps solved:
  integer-SP `skillPointsToPercentage` table (V8 pow vs Rust powf 1 ULP),
  serde_json `float_roundtrip`.

- **Layer 2 — leaf pipeline (NEXT):** given a leaf's items, reproduce the
  worker's (base_sp, total_sp, final score). The existing score fixtures
  already carry expected total_sp + score per case, so validation is free.
  Sub-pieces, in dependency order:
  1. **Running/build stats**: `_init_running_statmap(level, fixed_sms)` +
     per-item merge (numeric add on the compact key union) +
     `_finalize_leaf_statmap` (set bonuses via activeSetCounts, weapon,
     tomes). Mirror worker_shims.js. Fixture needs: item registry
     (name → full statMap) for pools+locked+weapon+tomes+guild, sets table
     with FULL stat bonuses (not just SP), level.
  2. **assemble_combo_stats**: clone build_sm, set skp_order = total_sp,
     set classDef (weapon type multiplier), merge atree_raw, radiance
     scale (null in benchmark scenarios — hard-fail if set), atree scaling
     via the SPLIT plan: fixture ships `const_scaled` (statmap) +
     `var_effects` lowered to JSON (input stat key, slider factor, output
     key, max, round) — recompute per SP trial with
     `atree_eval_stat_effects` semantics; merge static_boosts.
     The exporter can lower these with the sandbox's pure functions
     (`atree_scaling_analysis`, `atree_collect_stat_effects`,
     `atree_compute_scaling(..., skip_stat_inputs)`).
  3. **Greedy SP** (`_greedy_allocate_sp` + `greedy_sp_loop`): step-down
     [20,4,1], try-revert-keep, trial score = assemble (SP-dependent parts
     only) + damage eval. Mirror the worker's in-place trial semantics
     (results must be identical; journaling is an optimization detail).
  4. **Mana check** (`eval_combo_mana_check` → `simulate_combo_mana_fast`)
     + the worker's mana rescue path (re-allocate greedy SP after failure,
     `_scratch_orig_base_sp` snapshot semantics). Constants: BASE_MANA_REGEN,
     MANA_TICK_SECONDS, SPELL_CAST_TIME/DELAY; `row_unclamped_spell_cost`
     for costs; melee cooldown model (`compute_wall_dt`); loop brackets and
     buff states can hard-fail initially (benchmark scenarios use neither —
     assert on export).
  5. Scoring dispatch for combo_damage = total_damage (done in layer 1).

- **Layer 3 — enum_kernel integration:** replace `feasible += 1` with the
  layer-2 leaf pipeline; add per-thread top-N (15), the score-ceiling gate
  (one damage eval at all-150 SP vs cutoff), and a shared AtomicU64 cutoff
  across threads (floor(score) is admissible). Validate: top-N score-set
  equality vs JS on completing scenarios (oracle-style), then benchmark
  spell_wide end-to-end.

- **Layer 4 — optimization pass:** replace dynamic JSON stat maps with
  compiled dense stat vectors (the JS P1.3 trick): fixture already ships
  string-keyed maps; build the key-index union at load, compile items and
  combo-row boost deltas to index/value arrays, precompute per-part
  ConvBase/`damMult` filters. Only after layers 2–3 are bit-exact.

## Validation protocol

Every layer lands with a differential fixture regenerated from the JS
worker (`SOLVER_EXPORT_SCORE`, extended per layer) and bit-exact expected
values; mismatches are hunted with `probe_score_case.js` (JS side) and
`SCORE_KERNEL_DEBUG_CASE` (Rust side). No layer merges with tolerance-based
comparison.

## Benchmark protocol (goal loop)

After each layer with a runtime effect, report:
- melee: colossal 4.52T wall time (Rust threads vs JS 2-worker),
- spell: readme_spell_wide coverage (checked @ time cap or completion),
- plain-terms framing: "N empty slots solved exhaustively in X seconds".
