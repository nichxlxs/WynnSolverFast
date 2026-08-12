# sp_kernel — Rust SP kernel prototype (P2.2)

A Rust port of `js/game/skillpoints.js::calculate_skillpoints`
(classification, post_floor bounds, the Lodestone-style closure fast path, and
the pruned activation-order backtracking), validated for exact parity against
fixtures exported from the JS implementation.

## Workflow

```bash
# 1. Export seeded fixtures (500 builds from real item data, with the JS
#    results baked in as expected values):
node js/solver/tests/gen_rust_sp_fixtures.js 500

# 2. Verify parity and benchmark the kernel:
cd rust/sp_kernel
cargo build --release
./target/release/sp_kernel fixtures/sp_cases.txt 2000
```

The binary exits nonzero on any parity mismatch. Fixture files are generated
artifacts — regenerate them whenever `skillpoints.js` or the item data
changes; do not commit them.

## Measured (2026-08-12, this container)

| Implementation | µs/call | calls/s |
|---|---:|---:|
| JS `calculate_skillpoints` (scratch buffers, warmed V8) | 5.196 | 192,466 |
| Rust kernel (same 500 cases, exact same outputs) | 0.610 | 1,638,372 |

Kernel-level speedup: **8.5x**, with matching per-iteration checksums.

Caveats: this covers only the SP kernel, with reqs/skillpoints pre-parsed
into flat structs — the same representational advantage the JS numeric-index
vector work (P1.3) captured for running stats. The Phase-2 go/no-go gate
(≥1.5x end-to-end or a decisive memory/checkpoint win) still requires porting
the enumeration loop and leaf pipeline; this prototype establishes exact
parity methodology and the expected order of magnitude.

## enum_kernel — enumeration replay (P2.3 prototype)

`src/bin/enum_kernel.rs` replays a full solver scenario: level-based
enumeration with ring canonicalization, illegal-set blocking, mid-tree SP
pruning, restriction/EHP suffix bounds, leaf prechecks, and the exact SP
kernel at surviving leaves. Scoring (greedy SP / mana sim / damage) is
intentionally absent — feasible leaves are counted, not scored.

```bash
# Export a scenario fixture from the JS pipeline:
SOLVER_EXPORT_RUST=rust/sp_kernel/fixtures/enum_gaia_135m.txt \
  node js/solver/tests/test_solver_search.js gaia_armor4_ring_135m_input

cargo build --release
./target/release/enum_kernel fixtures/enum_gaia_135m.txt            # all cores
./target/release/enum_kernel fixtures/enum_gaia_135m.txt 1          # single thread
```

The binary prints a progress/rate/ETA line to stderr every ~5 seconds on
long runs. Threading (2026-08-12): worker threads claim first-slot offsets
from an atomic queue and run the full band sweep restricted to each offset
(the JS engine's 'slot' partition shape). Per-offset subspaces are disjoint
and all funnel counters are integral, so the per-thread sums are exactly
the single-thread counters, verified bit-identical on every fixture.

Measured (2026-08-12, this container, fixtures regenerated after the
dominance equality-set fix) — funnel parity is exact wherever both engines
completed (identical checked and feasible):

| Scenario | JS (2 workers, full leaf pipeline) | Rust 1 thread | Rust 4 threads |
|---|---:|---:|---:|
| Gaia 135.5M input / 31.5M search, 12,905 feasible | 1.39s | 0.097s | 0.054s |
| Gaia 1.898B input / 314.7M search, 4,862 feasible | 0.93s | 0.071s | 0.055s |
| Gaia all-free 1.816T input / 144.7B search, 81,616 feasible | — | 0.859s | 0.256s |
| Gaia colossal 4.52T input / 334.8B search (lvl 98-121, all slots free), 329,883 feasible | **completes 17.9s** | **4.35s** | **1.16s** |

Both engines use geometric level-band enumeration with pre-placement column
bound checks (2026-08-12); funnel counters are bit-identical to the
per-level enumeration they replaced.

Spell-workload caveat: a fixture exported from `readme_spell_wide` (20.07B
search, combo_damage, no stat restrictions) enumerates with almost no
pruning — nearly every leaf reaches the exact SP kernel, so the replay runs
at ~13.3M checked/s (4 threads, ETA ~25 min) instead of billions/s. That
workload's real cost lives in the scoring layer (greedy damage trials),
which this kernel intentionally lacks; porting the combo-damage scoring
pipeline (with the score-ceiling gate) is the prerequisite for meaningful
Rust spell benchmarks.

Sizing note: enumeration cost is governed by pruning effectiveness, not
space size. Lowering the level filter floods the pools with tier-stack
items, weakening the atkTier suffix bound: lvl_min 100 → 8.4s, 99 → 22.4s,
98 → 77.1s, 97 → ~3.6h (ETA) despite only ~3x space growth per step.

## score_kernel — combo-damage core (P2.4 layer 1)

`src/bin/score_kernel.rs` ports the combo-damage evaluation pipeline
(compute_combo_damage_totals → computeSpellDisplayAvg → _eval_spell_parts →
calculateSpellDamage, plus per-row boost tokens and atree prop overrides)
and validates it against differential fixtures: sampled builds evaluated by
the production JS worker, exporting the exact assembled combo_base stat map
and the expected damage per case.

```bash
# Export differential fixtures (96 sampled builds each):
SOLVER_EXPORT_SCORE=rust/sp_kernel/fixtures/score_gaia.json \
  node js/solver/tests/test_solver_search.js gaia_armor_ring_2m
SOLVER_EXPORT_SCORE=rust/sp_kernel/fixtures/score_spell.json \
  node js/solver/tests/test_solver_search.js readme_spell_wide

cargo build --release
./target/release/score_kernel fixtures/score_gaia.json
./target/release/score_kernel fixtures/score_spell.json
```

Measured (2026-08-12, this container): **96/96 bit-exact** on the melee
(Gaia, one melee-time row) fixture and **96/96 bit-exact** on the spell
(readme spell combo, 33 rows) fixture — every expected f64 reproduced
bit-for-bit.

Two parity traps worth remembering:
- V8's `Math.pow` and Rust's `powf` differ by 1 ULP on some inputs; skill
  points are integers, so the fixture ships the exact JS
  `skillPointsToPercentage` table (0–150) instead.
- serde_json's default float parsing is not correctly rounded — the
  `float_roundtrip` feature is required for bit-exact fixture comparison.

This implementation is parity-first (dynamic JSON stat maps, per-part
allocations): ~119K evals/s on the 1-row melee combo, ~1.6K evals/s on the
33-row spell combo, single-threaded. The compiled-stat-index optimization
pass comes after greedy SP + mana sim land, turning this into a full leaf
scorer inside enum_kernel.

## Scored enumeration (P2.4 layer 3+4)

`enum_kernel <enum_fixture> [threads] [score_fixture.json]` runs the full
leaf pipeline at every feasible leaf: SP solve → score-ceiling gate (vs a
cross-thread shared cutoff) → greedy damage trials → mana check + rescue →
merged top-15. Both fixtures must be co-exported from one scenario run:

```bash
SOLVER_EXPORT_RUST=rust/sp_kernel/fixtures/enum_spell2.txt \
SOLVER_EXPORT_SCORE=rust/sp_kernel/fixtures/score_spell2.json \
  node js/solver/tests/test_solver_search.js <scenario>
./target/release/enum_kernel fixtures/enum_spell2.txt 4 fixtures/score_spell2.json
```

Measured (2026-08-12, this container, 4 threads vs JS 2 workers):

| Scenario | JS | Rust |
|---|---:|---:|
| Dense combo_damage (readme armor2 pools, 3,712 search, no restrictions) | 10.2s | **2.9s**, top-15 bit-identical |
| `readme_spell_wide` coverage @ 180s (20.07B search) | 430K checked | **1.38M checked (3.2x)**, rate still climbing at cap |

Caveat: scenarios with stat restrictions need a `check_thresholds` port
(the exact assembled-stat check that runs after the additive prechecks)
before their scored top-N matches JS.
