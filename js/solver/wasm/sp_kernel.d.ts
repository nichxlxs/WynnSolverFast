/* tslint:disable */
/* eslint-disable */

/**
 * R24: one worker's engine for the work-queue solve.
 *
 * Browser workers cannot share a cutoff without `SharedArrayBuffer`, and a
 * running solve cannot receive messages, so `solve_partition` workers each
 * rediscover their own cutoff. An `Engine` instead parses the fixtures,
 * builds the bound tables and runs the warm start once, then solves units
 * (contiguous first-slot offset ranges) one call at a time. Between calls
 * the host hands the worker its next unit together with the merged cutoff
 * of everything every worker has finished, so later units prune against
 * the best builds found anywhere. Results accumulate across a worker's
 * units: each call returns the solve JSON of all of them so far, in the
 * same shape `solve_partition` returns, and progress is cumulative too.
 */
export class Engine {
    free(): void;
    [Symbol.dispose](): void;
    constructor(enum_fixture: string, score_fixture: string);
    /**
     * How many entries the merged cutoff counts down to: the result count,
     * or the archive cap under an R20 window.
     */
    result_count(): number;
    /**
     * Solves unit `index` of `count`. `seed_cutoff` is the floor of the
     * `result_count()`-th best distinct score across every worker's latest
     * report (0 for none) and `seed_best` the best; both are admissible
     * because they are scores of real builds.
     */
    solve_unit(index: number, count: number, seed_cutoff: number, seed_best: number, on_progress: Function): string;
}

/**
 * Total canonical search size for a fixture, so the UI can show progress
 * without starting a solve.
 */
export function search_space(enum_fixture: string): number;

/**
 * Solve a scenario and return results as a JSON string. Thin bindgen shim
 * over `enumerate::solve_json`, which is also callable natively so the
 * browser path can be tested without a browser.
 *
 * `max_leaves <= 0` means "run to completion".
 */
export function solve(enum_fixture: string, score_fixture: string, max_leaves: number): string;

/**
 * Search overlapping neighborhoods for strong builds within a time budget.
 * This is a heuristic: every progress/final payload sets `complete:false`.
 * Options and witness shapes are shared with the native testable wrapper.
 */
export function solve_anytime_with_progress(enum_fixture: string, score_fixture: string, options_json: string, on_progress: Function): string;

/**
 * `solve_with_progress` restricted to one partition of the search space.
 *
 * wasm threads need `SharedArrayBuffer` and COOP/COEP cross-origin
 * isolation, which the app cannot assume. Partitioning needs neither: the
 * host spawns one ordinary worker per core, each running this with its own
 * `part_index`, and merges the results. The split is by first-slot offset —
 * the same one the native threaded path work-steals over — so the
 * partitions are disjoint, `checked` sums to the whole space, and the
 * merged top-N is identical to a single-partition run (`partition_check`).
 *
 * Each partition still reports the FULL space as `total`, so a host summing
 * `checked` across workers gets a coherent percentage.
 *
 * The one thing lost versus native threads is the shared score cutoff: each
 * partition discovers its own, so the gate prunes a little less. That costs
 * work, never results.
 */
export function solve_partition(enum_fixture: string, score_fixture: string, max_leaves: number, part_index: number, part_count: number, on_progress: Function): string;

/**
 * `solve` with a live-progress callback.
 *
 * `on_progress` is invoked with a JSON string (checked/total, the funnel
 * counters, and the interim top-N) roughly every 2M credited leaves and
 * once more when the search ends. This is what lets a long solve show
 * movement in the UI instead of appearing hung — the reason to run this in
 * a dedicated worker rather than chunking on the main thread.
 *
 * Exact-mode emission is keyed on credited leaves, preserving its existing
 * progress behavior and deterministic emission points.
 */
export function solve_with_progress(enum_fixture: string, score_fixture: string, max_leaves: number, on_progress: Function): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_engine_free: (a: number, b: number) => void;
    readonly engine_new: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly engine_result_count: (a: number) => number;
    readonly engine_solve_unit: (a: number, b: number, c: number, d: number, e: number, f: any) => [number, number];
    readonly search_space: (a: number, b: number) => number;
    readonly solve: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly solve_anytime_with_progress: (a: number, b: number, c: number, d: number, e: number, f: number, g: any) => [number, number];
    readonly solve_partition: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: any) => [number, number];
    readonly solve_with_progress: (a: number, b: number, c: number, d: number, e: number, f: any) => [number, number];
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
