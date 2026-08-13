# Architecture plan for large, unattended searches

## Decision

Build a **native, headless Rust search engine and CLI (Option C)** while keeping
the existing JavaScript engine (Option A) as the correctness oracle and an
explicit browser fallback. Compile the same Rust engine to WebAssembly (Option
B) for interactive browser searches. Do not put the general search on a GPU
(Option D); reconsider GPU scoring only after profiling a compact, branch-free
candidate batch.

This is an additive migration. It does not require removing or replacing the
web solver. The first Rust milestone must beat indexed JavaScript on an agreed
benchmark and match the exhaustive oracle exactly before it becomes a supported
execution path.

Implementation status, 2026-08-13: the proven scored enumerator is now a
reusable Rust library behind a versioned JSON `SearchJob`/`SearchResult`
boundary. Thin native CLI and `wasm-bindgen` adapters call that same entrypoint.
The solver page defaults to a dedicated Rust/WASM module worker and keeps the
JavaScript engine user-selectable. Unsupported Rust inputs return typed errors;
the page does not silently change engines. Browser workers are single-threaded
in this first delivery, while the native CLI retains its multi-thread path.

## Why this fits the goal

Unattended searches need capabilities that are awkward in a browser: durable
jobs, checkpoint/resume, stable memory limits, process supervision, hours-long
runs, profiling, and eventually remote workers. A native process provides these
without requiring a desktop UI. The browser remains useful for composing and
viewing builds; it can export a versioned search request and import results.

Rust is recommended for the search kernel because its compact data layouts,
explicit allocation, native threads, and portable CLI deployment match a
branch-heavy combinatorial workload. WebAssembly remains an output target, not
the primary architecture. A GPU does not naturally fit recursive enumeration,
skill-point feasibility, set rules, dynamic combo state, or uneven pruning.

## Target architecture

```text
Browser UI / CLI request generator
              |
       versioned SearchJob
              |
   local coordinator + durable journal
              |
      bounded prefix-task queue
       /        |         \
 worker 0    worker 1   worker N       (native threads initially)
       \        |         /
       deterministic top-N reducer
              |
 checkpoint + progress + SearchResult
```

Use these boundaries from the beginning:

- **`SearchJob`**: immutable, versioned input with a game-data hash, item-pool
  IDs, locked items, combo plan, restrictions, target, ordering configuration,
  and deterministic tie-breaking rules.
- **Prefix task**: a canonical partial equipment selection plus remaining
  bounds. Tasks are independent, idempotent, and identified by a content hash.
- **Coordinator**: owns the queue, global cutoff, cancellation, checkpoint
  journal, progress counters, and final deterministic reduction.
- **Worker**: owns mutable scratch memory and evaluates batches of prefix tasks.
  It periodically reports counters, candidates, and a resume cursor.
- **Result**: includes engine/data versions, complete configuration, counters,
  elapsed CPU and wall time, top-N, and whether the search was exhaustive.

Start with one process and native threads. Do not start with local microservices,
containers, Redis, or a network database. Persist the job manifest and an
append-only checkpoint journal in a local job directory; use atomic rename for
snapshots. SQLite is reasonable when queue recovery or querying becomes more
complex, but is not required for the first prototype.

## Future cloud compatibility

Cloud distribution should reuse prefix tasks rather than introduce a second
search algorithm. Add it only after local scheduling and recovery are proven.
The coordinator may later lease batches through a durable queue. Workers must be
stateless apart from a lease, tolerate duplicate execution, and return results
idempotently. Store immutable game data and job manifests by content hash.

The global top-N cutoff is an optimization, not correctness state: delayed
cutoff propagation may waste work but must never omit a result. A task is
complete only when its result or resume cursor is durably acknowledged. This
model supports a single workstation, multiple local processes, spot instances,
and heterogeneous workers without changing result semantics.

## Language and platform sequence

1. **JavaScript reference**: repair fixtures, add exhaustive differential tests,
   and measure the current implementation. Make low-risk improvements here when
   they also clarify the future kernel.
2. **Rust native prototype**: port compact item representation, SP feasibility,
   canonical enumeration, and a simple stateless target. Keep calls coarse and
   compare complete results with JavaScript.
3. **Rust native product**: add the compiled combo plan, threads, prefix tasks,
   checkpoint/resume, resource budgets, CLI progress, and import/export.
4. **Wasm build**: compile the proven core for a dedicated browser worker. This
   is now the default interactive path, but remains distinct from the
   unattended native job host. Measure download, startup, and throughput before
   adding multiple Wasm workers or shared-memory Wasm.
5. **GPU experiment**: only if profiles show a large regular scoring batch. Keep
   enumeration and verification on CPU; include transfer costs in benchmarks.

## Benchmark and regression program

Correctness gates precede optimization:

1. Check in real combo and solver snapshots for every class, scoring target,
   ring configuration, SP pressure, mana/HP mode, sets, crafted items, and
   restrictions.
2. Add a deliberately simple exhaustive enumerator for small pools. Compare
   exact feasible counts, canonical top-N scores/items, and rejection counts.
3. Add fixed-seed generated cases and retain every discovered mismatch.
4. Cross-check builder and solver calculations, then cross-check JS and Rust.
5. Make partition coverage tests prove no gaps and no noncanonical duplicates.

Performance has two distinct dimensions:

- **Fixed work** measures engine efficiency: initialization time, bytes per
  item, checked and scored leaves/second, allocation/GC, SP calls, combo calls,
  pruning counts, CPU time, wall time, and parallel efficiency.
- **Anytime quality** measures ordering: time to first feasible build, time to
  validated score thresholds, and best score after fixed time budgets.

Use warmups, repeated samples, medians, p90, dispersion, runtime/hardware/commit
metadata, and machine-readable artifacts. Compare timings on the same controlled
machine. In ordinary CI, gate deterministic outputs and operation counts; use
generous time limits only to catch catastrophic regressions.

Initial scenario tiers:

- **Micro**: SP cascade DP, stat add/remove, threshold checks, combo plan, top-N.
- **Small exhaustive**: 2-4 candidates in every free slot; exact oracle result.
- **Medium complete**: finishes in seconds and measures end-to-end throughput.
- **Wide anytime**: all slots and broad levels, evaluated at fixed time marks.
- **Long soak**: hours-long checkpoint, cancellation, resume, and memory test.
- **Scaling**: 1, 2, 4, 8, and available-core counts with task-tail metrics.

## Optimization order

Apply optimizations in this order, with an oracle test and benchmark for each:

1. Fix unsafe pruning around set bonuses and establish deterministic ties.
2. Reject noncompetitive scores before allocating result objects.
3. Convert string-keyed `Map` data in the hot path to indexed numeric arrays.
4. Compile the selected combo and restrictions once per job.
5. Add suffix bounds for direct restrictions and simple monotonic objectives.
6. Add admissible branch-and-bound score limits; never use heuristic limits to
   claim an exhaustive optimum.
7. Reduce repeated greedy SP scoring and add a bounded monotonic mana rescue.
8. Generate dynamically scheduled prefix tasks and measure worker utilization.
9. Share immutable data between workers and keep scratch state worker-local.
10. Profile again before choosing additional algorithms or hardware.

Heuristic ordering and admissible pruning must remain separate. Ordering can
change which good answer appears first; pruning may only remove a subtree when a
proof shows it cannot satisfy constraints or beat the accepted cutoff.

## SP Algorithm Bounty integration

The linked pull request must be evaluated as an isolated replacement for
`calculate_skillpoints`, not copied directly into the solver. Capture its source
and claimed complexity, then run it through:

1. Hand-authored cascade, negative-SP, crafted, set-bonus, weapon, cap, and
   budget cases.
2. Fixed-seed randomized differential tests against the current implementation
   and, for small item counts, a permutation/exhaustive oracle.
3. The SP microbenchmark at every count of ordering items from zero through
   eight, with and without reusable scratch storage.
4. End-to-end searches measuring both SP calls/second and total solver speed.
5. Allocation and peak-memory profiles.

Only adopt it if semantics match and end-to-end performance improves. A faster
standalone algorithm may provide little benefit if greedy score evaluation or
combo simulation dominates. Conversely, a mathematically clearer algorithm is
valuable as an independent test oracle even if it does not win the hot-path
benchmark.

Network access to the pull request was unavailable while preparing this plan,
so no claim about its implementation or measured speed is made here.

## Milestones and decision gates

### M0: measurement foundation

- Full suite runs from a clean clone.
- Real fixtures and exhaustive oracle are checked in.
- Micro, medium, wide, and soak benchmark commands emit JSON.
- Record a controlled-machine baseline.

### M1: optimized JavaScript baseline

- Safe dominance, indexed hot data, compiled common combos, and first admissible
  bounds land behind flags.
- Exact regression parity is maintained.
- Profile identifies remaining dominant costs.

### M2: Rust proof of concept

- One stateless scenario and SP solver match exact JS results.
- Go forward only if Rust is at least 1.5x faster single-thread end to end, or
  offers a compelling memory/checkpoint benefit that raw throughput misses.

### M3: unattended local engine

- CLI supports resource/time budgets, cancellation, deterministic output,
  checkpoint/resume after process termination, and long soak tests.
- Browser export/import works; no browser engine is removed.

### M4: scale out

- Prefix tasks demonstrate useful multi-core scaling locally.
- Add leases and remote workers only when a representative job has enough
  coarse tasks and compute-to-transfer ratio to justify cloud cost.

## Switch points

- Stay JavaScript-only if indexed JS plus branch-and-bound meets search targets.
- Proceed with Rust native if unattended durability or measured throughput still
  falls short.
- Ship Rust/Wasm if the native core also improves interactive browser searches
  without unacceptable download/startup overhead.
- Use cloud workers when one-machine wall time, not algorithmic waste, is the
  proven constraint.
- Use GPU scoring only when an integrated prototype improves total job time on
  representative hardware.
