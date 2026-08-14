// Dedicated module worker for the Rust/WASM solver.
//
// Runs the whole search off the main thread and streams live progress back,
// so the page stays responsive and a long solve visibly moves instead of
// looking hung. Replaces the earlier main-thread `setTimeout` chunk loop:
// the engine now runs to completion in one call and reports through a
// callback, and cancellation is `worker.terminate()` rather than a flag the
// loop has to notice.
//
// The message protocol matches the JS solver workers (`progress` / `done` /
// `worker_error` carrying `worker_id`), so `_on_all_workers_done` and the
// progress UI consume both engines without branching.

import init, { initSync, solve_partition, search_space } from './sp_kernel.js';

let moduleReady;

/**
 * @param {WebAssembly.Module} [compiled] - a module the host compiled once
 *   and structured-cloned to every worker. Without it each worker compiles
 *   the ~650 KB module itself, and N workers compiling at once on N cores
 *   serialize badly enough to make partitioning a net loss on short
 *   searches. With it, instantiation is nearly free.
 */
function loadModule(compiled) {
    if (!moduleReady) {
        moduleReady = compiled
            ? Promise.resolve(initSync({ module: compiled }))
            : init();
    }
    return moduleReady;
}

self.onmessage = async (event) => {
    const msg = event.data;
    if (msg?.type !== 'solve') return;
    const worker_id = msg.worker_id ?? 0;
    const post = (body) => self.postMessage({ worker_id, ...body });
    // Int32Array over the host's SharedArrayBuffer, or null when the page is
    // not cross-origin isolated. Atomics need a shared buffer; a plain one
    // would silently give each worker a private copy.
    let cutoff = msg.cutoff_sab ? new Int32Array(msg.cutoff_sab) : null;

    try {
        post({ type: 'progress', phase: 'loading engine', checked: 0, total: 0 });
        await loadModule(msg.compiled_module);

        post({ type: 'progress', phase: 'preparing search', checked: 0, total: 0 });
        let total = 0;
        try { total = search_space(msg.enum_fixture); } catch (e) { /* progress-only */ }
        post({ type: 'progress', phase: 'searching', checked: 0, total });

        const raw = solve_partition(
            msg.enum_fixture, msg.score_fixture, msg.max_leaves ?? 0,
            msg.part_index ?? 0, msg.part_count ?? 1,
            (payload) => {
                const p = JSON.parse(payload);
                // Progress FIRST, unconditionally. The shared-cutoff work below
                // is an optimisation; if it throws, the host must still get its
                // funnel counters and interim results. Doing it the other way
                // round cost the UI every progress message the moment Atomics
                // misbehaved, and the engine swallows a throwing callback, so
                // the search looked frozen while running perfectly.
                post({
                    type: 'progress',
                    phase: 'searching',
                    checked: p.checked,
                    total: p.total,
                    precheck_pass: p.precheck_pass,
                    precheck_reject: p.precheck_reject,
                    feasible: p.feasible,
                    met_req: p.scored,
                    gated: p.gated,
                    mana_reject: p.mana_reject,
                    bound_pruned: p.bound_pruned,
                    top_n: p.top_n,
                });

                // Shared cutoff. When the page is cross-origin isolated the
                // host passes a SharedArrayBuffer; every partition publishes
                // its 15th-best score into it and reads back the maximum, so
                // each prunes against what the others have already found
                // rather than rediscovering a cutoff from scratch. A score one
                // partition has already reached is a valid lower bound on the
                // global top-N threshold, so this only ever skips leaves that
                // could not have placed anyway.
                //
                // Returning undefined means "no floor", which is exactly the
                // behaviour before any of this existed.
                if (!cutoff) return undefined;
                try {
                    if (p.top_n && p.top_n.length >= 15) {
                        const mine = Math.floor(p.top_n[14].score);
                        if (Number.isFinite(mine) && mine > 0) {
                            // i32 saturation: a score above 2^31-1 would wrap,
                            // and a wrapped-negative floor is nonsense.
                            Atomics.max(cutoff, 0, Math.min(mine, 0x7fffffff));
                        }
                    }
                    const g = Atomics.load(cutoff, 0);
                    return g > 0 ? g : undefined;
                } catch (e) {
                    // One failure disables sharing for this worker, nothing more.
                    cutoff = null;
                    return undefined;
                }
            },
        );
        const result = JSON.parse(raw);

        // The engine reports scenarios it cannot reproduce bit-exactly
        // rather than solving them approximately; the host falls back to the
        // JS workers on this.
        if (result.error) {
            post({ type: 'worker_error', code: 'unsupported_scenario', message: result.error });
            return;
        }

        post({
            type: 'done',
            checked: result.checked,
            precheck_pass: result.feasible,
            precheck_reject: 0,
            feasible: result.feasible,
            met_req: result.scored,
            gated: result.gated,
            mana_reject: result.mana_reject,
            bound_pruned: result.bound_pruned,
            // `solve` reports `items`, the progress sink reports
            // `item_names`; normalize so the host sees one shape.
            top_n: (result.top ?? []).map((t) => ({
                score: t.score, item_names: t.item_names ?? t.items,
                // The greedy's SP assignment. The host installs this as the
                // build's skill points, so dropping it here (as this
                // normalizer used to) puts a zeroed allocation in the UI
                // whose recomputed stats contradict the score.
                base_sp: t.base_sp, total_sp: t.total_sp,
                assigned_sp: t.assigned_sp,
                // Which tome the engine chose, when tome optimisation is on.
                // Same reasoning as the SP fields above: this normalizer is
                // the last link before the host, and it is where the SP
                // assignment was silently dropped before.
                tome: t.tome ?? null,
            })),
            complete: result.complete,
        });
    } catch (error) {
        post({
            type: 'worker_error',
            code: 'rust_worker_crash',
            message: error?.message ?? String(error),
            stack: error?.stack,
        });
    }
};
