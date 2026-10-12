# Browser anytime API

The solver page offers **Quick search** with 5, 15 and 30-second budgets.
Exhaustive search remains the default. Quick uses one dedicated worker and
retains only builds returned by the WASM evaluator, including their skill
points and tome choices. Stop, deadline and engine errors preserve these
witnesses; no unvalidated prior UI build is inserted into the result archive.
Unsupported scenarios produce a visible error without silently launching an
unbounded JavaScript search.

The host deadline begins before fixture preparation. Preparation checkpoints
and budget checks inside tome enumeration limit expensive work, although a
synchronous preparation phase can block interaction until its deadline.
Pending fixture/module promises and worker messages are guarded by a run
generation so canceled work cannot overwrite a later run.

Quick keeps raw gear pools and disables the known-unproved raw-stat and
HP/EHP prechecks; finalized restrictions remain in the scorer. Existing tome
candidate preparation and evaluator limitations remain. Set weapons are
explicitly unsupported because the current scoring engine omits their set
membership. Already-invalid locked exclusive-set combinations are rejected.
Changing query controls during an active run can still affect subsequent
result application; the broader existing result-loading behavior is unchanged.

The `wasm` feature exports:

```js
const final = JSON.parse(solve_anytime_with_progress(
  enumFixtureText,
  scoreFixtureJson,
  JSON.stringify({ seconds: 5, seed: 1, top_k: 15 }),
  json => publish(JSON.parse(json)),
));
```

Call this synchronously inside a dedicated worker. The worker's monotonic
`performance.now()` clock enforces the search budget; the host can terminate
the worker to cancel it. Input parsing and scoring-plan construction count
towards elapsed time. These preparation operations and one individual
evaluator call are not preemptible inside WASM, so the host should also own
the external deadline and retain the last streamed builds.

This is a heuristic over overlapping repair domains. Every payload has
`algorithm: "alns"` and `complete: false`, even if a repair exhausts its own
domain. No percentage of global completion or certified optimality gap is
reported. Published builds use the existing production evaluator and inherit
its skill-point allocation and game-model limitations.

The first feasible retained witness is published before its repair finishes.
Later progress is published approximately every 50 ms or on a newly observed
best score. Internal sampling occurs every 256 credited search events, so a
long evaluator call can delay a callback. Callbacks receive the full retained
top-N across all repairs, preserving previous discoveries when a subsequent
repair is worse. Throwing JavaScript callbacks are ignored by the WASM shim.

Progress has these fields:

| Field | Meaning |
|---|---|
| `elapsed_secs`, `seconds_budget` | Monotonic elapsed time and requested budget |
| `leaf_calls` | Actual evaluator leaf calls across repairs |
| `credited_tuples` | Visited or pruned tuples, with overlapping repairs counted again |
| `scored` | Completed valid scores |
| `repairs`, `completed_repairs` | Finished repairs and those that exhausted their own domains |
| `phase`, `stop_reason` | Current operator and final stopping condition |
| `top_n` | Retained entries with `score`, `item_names`, `base_sp`, `total_sp`, `assigned_sp`, optional `tome` |

Tome payloads preserve `guild_idx`, `weaponTome`, and `armorTome`. The final
JSON additionally includes the native CLI's `top` array (using `items` in
place of `item_names`) and operator counters. `leaf_calls` and
`credited_tuples` are distinct measures; neither is a progress percentage.

Options use snake_case; unknown keys and invalid values return an `error`
JSON object. Inputs are the internally generated enum and scoring fixtures,
the same format consumed by the exact WASM API. Scoring contexts rejected by
the exact loader are also rejected here.

| Option | Browser default | Accepted values |
|---|---:|---|
| `seconds` | 5 | finite number, 0 through 300 |
| `seed` | 1 | integer, 0 through 2^53−1 |
| `top_k` | 15 | integer, 1 through 15 |
| `warm_k` | 6 | integer, 1 through 64 |
| `warm_budget` | 2,000,000 | integer, 1 through 10,000,000 credited tuples |
| `repair_budget` | 100,000 | integer, 1 through 10,000,000 credited tuples |
| `max_repairs` | 10,000 | integer, 1 through 100,000 |
| `cycle_stagnation` | true | boolean |
| `elite_pool` | false | boolean; optional six-slot candidate recombination experiment |
| `archive_size` | 24 | integer, 1 through 64; raised to at least `top_k` |
| `work_budget` | unlimited | optional integer, 0 through 2^53−1 actual leaves |

The platform-neutral Rust entry point is
`enumerate::anytime::solve_json_with_progress`. A generous time limit with a
fixed seed/work/repair budget permits native/WASM deterministic witness
comparisons. Timing-dependent runs should be compared using observed quality
within the same external deadline instead.

Native CLI defaults and exact WASM exports remain unchanged. Featureless
`wasm32` compile checks do not import a clock or export the browser solver;
the shipped browser build must use `--features wasm` as `build-wasm.sh` does.
