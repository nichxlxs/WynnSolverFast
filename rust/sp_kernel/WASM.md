# Running the Rust engine in the browser

Phase 2 status: the engine is a library, the browser entry point exists,
and a loadable `.wasm` module builds. What remains for a full in-app
rollout is listed at the bottom.

## Build

```bash
./rust/sp_kernel/build-wasm.sh        # cargo -> wasm-bindgen -> wasm-opt -Oz
```
Output lands in `js/solver/wasm/`. The `-Oz` pass takes the module from
587 KB to **517 KB** (-12%) with no behavior change — verified in-browser
afterwards (armor4 still 344 ms, scores still bit-identical).

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

## Measured against the JS engine

Same machine, same scenario, identical work — `readme_armor4`
(3,223,584 canonical combinations):

| | JS engine (workers) | Rust/WASM (1 thread) |
|---|---:|---:|
| combinations checked | 150,000 (**4.7%**) | 3,223,584 (**100%**) |
| wall time | 10,501 ms — **timed out** | **348 ms** — completed |
| throughput | 14,285 /s | **9,263,172 /s** |
| best score found | 24,848 | 24,848 — **identical** |

**~648x throughput.** In user terms: a search the JS engine gives up on
after 10 seconds having seen 5% of the space becomes an exhaustive answer
in a third of a second — and finds the same best build.

### Verified in a real browser (Chromium via Playwright)

Same `readme_armor4` scenario, running the actual web build:

| run | checked | wall | rate | top1 |
|---|---:|---:|---:|---:|
| 1 (cold, includes V8 tier-up) | 3,223,584 (100%) | 444 ms | 7.27 M/s | 24,848 |
| 2 (warm) | 3,223,584 (100%) | 357 ms | 9.04 M/s | 24,848 |
| 3 (warm) | 3,223,584 (100%) | 346 ms | 9.31 M/s | 24,848 |

**~651x the JS engine's rate in-browser**, with the same best build — and
even the cold run completes the whole space 23x faster in wall time than
the JS engine's timed-out 4.7% pass. `readme_armor2` is 38 ms warm vs
422 ms for JS (11x).

Reproduce with `wasm_browser_test.html` (serve the repo root, map
`/fx/` to `rust/sp_kernel/fixtures/`, then drive it with Playwright
pointing at the pre-installed Chromium).

### In the dedicated worker

The app runs the engine in `js/solver/wasm/worker.js` — a module worker —
rather than chunking on the main thread. Same scenario, measured through
the worker:

| run | wall | rate | top1 |
|---|---:|---:|---:|
| 1 (cold: worker spawn + wasm init + compile) | 487 ms | 6.62 M/s | 24,848 |
| 2 (warm) | 358 ms | 9.00 M/s | 24,848 |
| 3 (warm) | 350 ms | 9.21 M/s | 24,848 |

Steady state matches the main-thread figures, so the worker costs nothing
once warm — and the page is free for the whole solve instead of being
interrupted every chunk. `solve_with_progress` streams funnel counters and
the interim top-N back (~every 2M leaves, plus a final snapshot that equals
the returned totals), so long searches visibly move.

Progress is keyed on **leaves, not wall time**, for the same reason the
budget is: wasm32 has no usable clock, and leaf-keyed emission points are
reproducible.

Cancellation is `worker.terminate()`. A scenario the engine cannot
reproduce bit-exactly comes back as `worker_error` with the specific
mechanic named, and `search.js` re-runs it on the JS workers rather than
leaving the user with no results.

Note: the first `solve` call in a page is several times slower than
steady state — V8 runs wasm through its baseline compiler (Liftoff)
before tiering up to TurboFan. Chunked solving warms it up naturally.

Smaller scenarios are dominated by fixed startup cost, so they show a
smaller ratio (1,872-leaf `readme_armor2`: 422 ms JS vs 75 ms WASM ≈
5.6x). Sustained throughput on a large spell search is ~328K leaves/s
single-threaded in WASM, versus ~2.4K/s for the JS engine on the same
scenario.

## Multi-worker

Correctness first: `partition_check` runs every partition count from 2 to 8
on `spell2`, `armor4` (3,223,584 leaves) and `hp2`, and requires that
`checked` sums to the whole-space total and that the merged top-N is
**bit-identical** to a single-partition run. All exact.

Counters *downstream of the score gate* (`feasible`, `scored`, `gated`,
`bound_pruned`) do differ, and that is expected rather than a defect: each
partition discovers its own score cutoff, so the gate prunes less. It is the
same reason those counters vary between 1 and 4 native threads.

That lost sharing is also the speed ceiling. Measured natively on `armor4`:

| | wall | leaves scored |
|---|---:|---:|
| whole space | 382 ms | 78 |
| 4 partitions, slowest | 208 ms | 384 across all four |

So **~1.8x at 4-way, not 4x** — the partitions do 1.8x the total CPU because
they each re-derive a cutoff. Sharing it would need `SharedArrayBuffer` (the
worker's `solve` is synchronous, so the host cannot inject one mid-run).

### When partitioning is a loss

Each extra worker costs spawn, module instantiation and structured-cloning
the score fixture (~880 KB on `armor4`), and the main thread serializes that.
Measured in Chromium on a 4-core machine with empty partitions — pure
overhead, no search:

| workers | 1 | 2 | 4 |
|---|---:|---:|---:|
| overhead | 80 ms | 140 ms | 261 ms |

Against ~300 ms of actual search on `armor4`, 4-way spends 261 ms of
overhead to save ~167 ms. It is a **net loss**, and the measurement says so:
384 ms at 1 worker versus 666 ms at 4.

Two fixes followed from that:

- **The module is compiled once** on the main thread and structured-cloned
  to every worker (`WebAssembly.Module` is cloneable), instead of each
  worker compiling ~650 KB itself. Single-worker `armor4` went 484 ms ->
  381 ms.
- **Partitioning is gated on search size.** The host estimates the space
  from the fixture's `SLOT` pool counts (exact on `armor4`: 3,223,584) and
  uses one worker below 8M — roughly 2x the ~4M break-even implied by the
  numbers above. `armor4` and `spell2` get one worker; `spell_wide` (3.9e10)
  and `gaia_colossal` (6.4e11) get the full count.

The honest summary: partitioning is exact and helps on the long searches it
is gated to, but it is not a substitute for shared-cutoff threading, and on
a short search it would have made things worse.

## Remaining for a full rollout

1. **App → fixture path**: today's payloads come from the test harness.
   The app needs to serialize its own scenario into the same format
   (or the loader needs to accept the app's native structures).
2. ~~**Threads**~~ — **done, by partitioning instead.** wasm threads need
   `SharedArrayBuffer` plus COOP/COEP cross-origin isolation, which the app
   cannot assume. Partitioning needs neither: the host spawns one ordinary
   worker per core, each calling `solve_partition` with its own
   `part_index`, and merges the results. The split is by first-slot offset —
   the same one the native threaded path work-steals over. See
   "Multi-worker" below for the measurements, including where it is *not*
   worth doing.
3. **Feature coverage**: scenarios using until-OOM loop brackets, buff
   states, Blood Pact, dynamic sliders, or non-lowered ability trees
   hard-fail and fall back to the JS engine. `solve` returns
   `{"error": "..."}` naming the unsupported mechanic, and `search.js`
   drops back to the JS workers on it. (Count loops, Radiance and
   `total_healing` are supported — see `SUPPORT_MATRIX.md` section A.)
4. **Size**: `wasm-opt -Oz` runs as part of `build-wasm.sh` (587 KB -> 517 KB).
