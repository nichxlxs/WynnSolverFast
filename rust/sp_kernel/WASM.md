# Running the Rust engine in the browser

Phase 2 status: the engine is a library, the browser entry point exists,
and a loadable `.wasm` module builds. What remains for a full in-app
rollout is listed at the bottom.

## Build

```bash
cd rust/sp_kernel
cargo build --release --target wasm32-unknown-unknown --features wasm --lib
wasm-bindgen target/wasm32-unknown-unknown/release/sp_kernel.wasm \
    --out-dir ../../js/solver/wasm --target web
```

`-C target-cpu=native` is scoped to non-wasm targets in `.cargo/config.toml`,
so cross-compilation is clean. The module is ~630 KB before `wasm-opt`.

## API

```js
import init, { solve, search_space } from './wasm/sp_kernel.js';
await init();

const total = search_space(enumFixtureText);          // canonical space size
const json  = solve(enumFixtureText, scoreFixtureJson, 5_000_000);
const r = JSON.parse(json);
// { checked, feasible, scored, gated, mana_reject, thresh_reject,
//   bound_pruned, complete, top: [{score, items:[...]}, ...] }
```

Both fixture payloads are passed **as strings** — wasm32 has no
filesystem. They are produced today by the export harness
(`SOLVER_EXPORT_RUST` / `SOLVER_EXPORT_SCORE`); wiring the app to generate
them directly is the remaining integration step (matrix item C2).

### Why a leaf budget instead of a time limit

`std::time::Instant` panics on wasm32-unknown-unknown, so all timing goes
through `src/clock.rs`, which is inert there. Instead of wall time, `solve`
takes **`max_leaves`**: it stops once that many leaves are credited and
reports `complete: false`. That is deterministic (same input → same
result, unlike a time slice) and is exactly the shape an incremental UI
wants — call it in a loop from a Web Worker and repaint between chunks.
Pass `0` to run to completion.

## Validation

**Verified running in a JS runtime**: built with the matching
`wasm-bindgen` CLI (0.2.127) and loaded in Node —

```
search_space = 3712
checked = 3712 | scored = 348 | complete = true
top1 = 8.11046498646568507e+6 | Brainwash, Tesla, The Crossing, ...
chunked(1000): checked = 1008 | complete = false
```

That top score is byte-identical to the native CLI's, and chunking stops
promptly at the requested budget (the leaf-budget check is unmasked, so a
few-thousand-leaf chunk really stops there).

The path is also exercised natively so the normal test loop covers it:

```bash
cargo build --release
./target/release/wasm_selftest fixtures/enum_spell2.txt fixtures/score_spell2.json
```

`wasm_selftest` calls `enumerate::solve_json`, the exact function the
bindgen shim wraps. Its top-15 is byte-identical to the CLI's on the dense
spell scenario, so the browser entry point inherits the same bit-exactness
guarantee as everything else.

## Remaining for a full rollout

1. **App → fixture path**: today's payloads come from the test harness.
   The app needs to serialize its own scenario into the same format
   (or the loader needs to accept the app's native structures).
2. **Threads**: this is the single-threaded path. WASM threads need
   `SharedArrayBuffer` plus COOP/COEP cross-origin isolation — the same
   setup the existing JS shared-cutoff already prefers — and a
   `wasm-bindgen-rayon`-style pool. Single-threaded is already ~1000x the
   current JS engine per core.
3. **Feature coverage**: scenarios using loop brackets, buff states, Blood
   Pact, Radiance, dynamic sliders, `total_healing`, or non-lowered
   ability trees still hard-fail and must fall back to the JS engine
   (see `SUPPORT_MATRIX.md` section A).
4. **Size**: run `wasm-opt -Oz` for a meaningful shrink before shipping.
