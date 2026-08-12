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

- **Layer 2 — leaf pipeline: DONE (2026-08-12).** Full pipeline
  (SP solve → greedy damage trials → mana check + Int-shift rescue →
  score) reproduces the worker's (base_sp, total_sp, assigned_sp, score)
  192/192 bit-exact on both fixtures. Unsupported subsets (loops, buff
  states, Blood Pact) hard-fail. Parity-first throughput: 1,276 leaves/s
  melee, 60 leaves/s spell (greedy = ~50-90 full 33-row damage evals per
  leaf), single-threaded.
  Original scope: given a leaf's items, reproduce the
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

- **Layer 3 — enum_kernel integration: DONE (2026-08-12).** Scored leaves
  via the layer-2 pipeline with the ceiling gate + shared AtomicU64
  cutoff and per-thread top-15 merge. Validated on the dense combo_damage
  scenario (readme armor2 pools, 3,712 search, no restrictions): top-15
  scores and item lists bit-identical to the JS 2-worker run; wall 5.1s
  (4 threads) vs 10.2s (JS). Caveats:
  - Scenarios WITH stat restrictions need a `check_thresholds` port (the
    exact assembled-stat threshold check that runs after the additive
    prechecks) before their scored top-N matches JS — the melee 2M probe
    over-included builds for exactly this reason.
  - Per-leaf scoring is parity-first slow (~1.4ms avg incl. gate): Rust
    covers fewer leaves/s than the compiled-vector JS engine (~0.7K vs
    ~2.4K), so the spell_wide coverage benchmark waits for layer 4.
  Design decisions (2026-08-12):
  - Move the scoring code from `bin/score_kernel.rs` into the lib
    (`src/scoring.rs`) so enum_kernel can use it; score_kernel stays as
    the differential-validation bin.
  - enum_kernel takes the enum text fixture AND the score JSON fixture
    for the same scenario; pool items join to the item registry via name
    lists exported in the score fixture (`pool_names` per slot in
    enumeration order, `ring_pool_names`, `locked_names`) — both exports
    must come from the same scenario run (SOLVER_EXPORT_RUST and
    SOLVER_EXPORT_SCORE now co-exist in one invocation).
  - Read-only scoring state (Layer2/Tables/rows/registry) shared by
    reference across threads; sp Kernel + scratch per thread.

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

## Condition-based early termination (2026-08-12)

- **Leaf mana-doom precheck: DONE.** Mana feasibility depends on greedy SP
  only through Int (monotone: start mana up, costs down), so one fast sim
  at Int=150 before greedy proves whether ANY allocation can sustain the
  combo — doomed leaves skip the ~17ms greedy entirely. Guarded by
  `Layer2::mana_doom_ok` (no atree var effect may output a mana-relevant
  stat: mr/ms/maxMana/hp/hpBonus/atkTier/int/spRaw*/spPct*).
- **Subtree mana bound (designed, next):** extend BoundTables with suffix
  MINIMA for the cost stats (spRaw*/spPct*/spPct*Final; suffix_max already
  covers mr/ms/maxMana) and the achievable integer atkTier range; a prefix
  is mana-dead when the fast sim fails for EVERY atkTier in range with
  best-case stats (sim is monotone in the others once atkTier is fixed).
  Evaluate alongside the objective ceiling in bound_prunes (same memo).

## Generic objectives (2026-08-12)

DONE — see `Objective` in scoring.rs. Gate/bound/greedy dispatch through
it; ceiling-based pruning arms only when `supports_ceiling()` proves
monotonicity (combo_damage, indirect stats, custom blends with all
weights >= 0). Validated bit-exact on total_hp (readme_armor2) at all
levels; tie membership at the top-15 boundary may differ from JS
(insertion-order semantics), score sets are identical.

## Layer 4 progress (2026-08-12, second pass)

- DONE: compiled row boosts + pre-patched mod_spell + compiled mana-cost
  deltas (all per-row constants hoisted to load; dual-path validator keeps
  both implementations 200/200 bit-exact).
- Measured: spell_wide 180s coverage unchanged (~1.38M) — this scenario's
  rows were already on the no-token borrow fast path, and the per-trial
  base clone is only ~3% of an eval. The eval cost lives in the damage
  core's per-part JSON traversal.
- DONE (2026-08-12, third pass): **compiled spell plans** (structural
  lowering of mod_spell; single shared parts pass), **in-place row
  overlays** (undo journals instead of clone-and-extend), **dense stat
  vectors** (`DenseCtx`/`DenseLeaf`/`DScratch` in scoring.rs — the whole
  gate+greedy trial pipeline on flat f64 vectors with a presence bitset;
  per-trial assemble is a memcpy + skp add chains + var-effect programs),
  **direct dense leaf build** (items/tomes/weapon/sets as indexed add
  lists; gated leaves never materialize a JSON stat map; Obj base built
  lazily for survivors), **row-group memoization** (identical rows compute
  once per eval), **per-thread buffer reuse** and **read-universe write
  filtering**. Every step is guarded: unsupported shapes fall back to the
  Obj path per scenario or per leaf; SCORE_DENSE_CHECK=1 asserts every
  dense score bit-equal to the Obj path; SCORE_DENSE=0 kills the path.
  Measured (4 threads): dense spell scenario 62.5s JS → 10.2s JS-optimized
  → 5.1s L3 Rust → 0.10s now; readme_spell_wide 180s coverage 430K JS →
  1.38M L3 → ~100M now. ENUM_TIME_CAP_SECS caps benchmark runs gracefully.
- Historical design note (superseded by the above): **compiled spell plans.** Per row (on the constant
  mod_spell): parts lowered to flat structs — Damage{multipliers[6],
  use_str, ignored_mults, part_id, precomputed part-scoped ConvBase
  names[6]} / Heal / Total{edges: (part_idx, hits, tick_rounding)} — with
  part KIND resolved statically (multipliers→damage, heal→heal,
  total→first sub's kind), so the display-result index and referenced-set
  are compile-time constants. eval uses an indexed memo (Vec, no HashMap)
  and ONE parts pass shared by display-avg and flat-damage (JS evaluates
  parts twice on DPS rows; same inputs → same results, so single-pass is
  bit-exact). Kills the remaining format!/JSON-get/alloc traffic in the
  innermost loop. Then: dense stat vectors for combo_base (memcpy clones,
  index-resolved reads) as the final representation jump.

## Search-space round (2026-08-12, fourth pass)

- **Last-slot cluster bound (DEFAULT ON, BOUND_CLUSTER=4)**: the innermost
  loop tests one dense ceiling per cluster of 4 level-adjacent items and
  skips whole clusters below the shared cutoff; prefix state cached per
  node in a dedicated buffer, verdicts memoized per (prefix, cluster)
  across level bands. Whole-pool and banded suffix bounds (BOUND_DEPTH /
  BOUND_TAIL, default off) exist but rarely fire — per-stat pool maxima
  form a "super-item" ~2x above real ceilings. Two fixes made bounds bite:
  level-banded suffix maxima (pool[0..=h] per band budget h) and
  set-transition deltas replacing the global set_upper (which alone
  inflated every ceiling ~2x and had silently neutered the bound).
- **Warm start (WARM_K=6)**: pools ranked by solo objective ceiling, the
  top-6-per-slot subspace solved first with the same Search sharing the
  cutoff atomic. Real builds → admissible seed; ranking only affects speed.
- readme_spell_wide 180s: 95.2M → 335.4M leaves (organic best now 8.192M,
  above the JS engine's all-time 8.119M). xpb objective: 6.3M leaves/s
  (99.9% cluster-pruned). ehp objective: gate/bound discriminate poorly
  (many near-cutoff leaves run the full pipeline; mana sim dominates) —
  future: indirect-objective mana/pipeline specialization.
- New benchmark fixtures (validated 96/96 bit-exact incl. dense per-trial):
  spell_8free (all 8 slots free), spell_ehp, spell_xpb.
