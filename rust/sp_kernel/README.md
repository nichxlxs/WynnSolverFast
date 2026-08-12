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
