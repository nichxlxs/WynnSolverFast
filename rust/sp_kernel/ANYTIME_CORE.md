# Incumbent retention and result-count experiments

The native enumeration CLI now supports these independent experiment controls:

| Variable | Default | Meaning |
|---|---:|---|
| `RETAIN_WARM` | `1` | Keep actual warm-start builds and their skill-point assignments in the result archive. `0` reproduces the former cutoff-only handoff for A/B comparisons. |
| `RESULT_COUNT` | `15` | Number of distinct equipment/tome results to maintain. Set `1` to optimize only the best build. |
| `QUALITY_TRACE_PATH` | unset | Flush a JSONL record whenever the globally best score or kth score improves. |
| `ENUM_TIME_CAP_SECS` | unset | Native wall budget, including input/scoring preparation and warm search. Preparation is not interruptible; enumeration checks cancellation at most every 256 progress events. |

`RESULT_COUNT` is a native experiment. Existing WASM/browser entrypoints still request the default 15 results; no browser top-1 control is added here. Warm retention applies to both native and WASM execution.

## Result identity and safe cutoffs

Warm search, enumeration and neighbourhood repair can rediscover identical gear. Archive insertion deduplicates **before** calculating any shared cutoff. Identity includes all eight gear names and the selected guild/weapon/armor tome multiset. The two ring slots and permutations within a tome category are interchangeable; repeated tomes remain significant. Alternative SP assignments for the same equipment/tome identity replace the old result only when they score better. Tied scores are equivalent for optimization; the selected identities among ties can depend on traversal order, because the hot path avoids materializing candidates that merely equal a full archive's cutoff.

Each browser partition retains only warm builds whose first-slot choice belongs to that partition. Therefore the existing disjoint-partition merge contract is preserved. Native workers may retain the same warm builds, but their final union is deduplicated. Reaching a wall/leaf budget in the retained mode never erases the warm witnesses that established a cutoff.

`RETAIN_WARM=0` is explicitly a legacy ablation: its original cutoff-only handoff can return no witnesses under an immediate budget. It should not be used as the user-facing default.

The reference evaluator and configured candidate pools are unchanged. These features do not establish raw-catalog game optimality or eliminate the evaluator's heuristic SP-allocation limitations.

## Timing and traces

Existing `elapsed` and throughput metrics remain main-enumeration metrics for compatibility. A separate `timing:` line prints `wall_total`, `warm`, `main`, `complete`, `result_count`, and `retain_warm`.

The wall timer starts before input/scoring preparation. JSONL has:

- `start`: wall time zero.
- `incumbent`: wall time, phase (`warm` or `enumerate`), best score, kth score once available, distinct archive size, equipment, SP allocation and tome choice.
- `finish`: wall time, warm time, and whether enumeration exhausted the configured space.

Warm discoveries are traced even in the cutoff-only ablation. This measures when a valid build was evaluated; it should not be confused with when the old UI published that build. To measure presentation latency, use progress callbacks separately. Native timestamps exclude process launch; external benchmark harnesses should record process wall time too.

With tracing disabled, the hot leaf path does no trace file I/O, locking, serialization, or trace clock reads. Trace-enabled comparisons should enable the same tracing on both variants.

## Focused verification

```sh
cargo test --release --lib
cargo run --release --bin anytime_core_check -- enum_fixture.txt score_fixture.json
cargo run --release --bin partition_check -- enum_fixture.txt score_fixture.json 8
```

Use a tractable fixture with more than three candidates and feasible warm builds. `anytime_core_check` checks exhaustive top-15 parity with/without retention, top-1 score parity, canonical uniqueness, warm publication at zero main-search budget, and three-way disjoint partition parity. Unit tests cover ring/tome identity, repeated witnesses and cutoffs, zero budgets, deadlines that expire before enumeration begins, and 257-candidate scans capped at exactly 10 or 37 evaluated leaves. Nested stops also verify restoration of the parent state. A credited-space budget can exceed its numeric threshold by the size of an already-pruned subtree; an actual-leaf budget is exact.
