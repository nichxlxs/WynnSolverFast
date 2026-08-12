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
./target/release/enum_kernel fixtures/enum_gaia_135m.txt
```

Measured (2026-08-12, this container) — funnel parity is exact in both
scenarios (identical checked and feasible):

| Scenario | JS (2 workers, full leaf pipeline) | Rust (1 thread, no scoring) |
|---|---:|---:|
| Gaia 135.5M input / 22.97M search, 10,313 feasible | 1.881s | 0.342s |
| Gaia 1.898B input / 229.7M search, 4,017 feasible | 1.376s | 0.332s |
