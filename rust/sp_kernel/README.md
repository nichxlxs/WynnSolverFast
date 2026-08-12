# sp_kernel — Rust SP kernel prototype (P2.2)

A dependency-free Rust port of `js/game/skillpoints.js::calculate_skillpoints`
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
