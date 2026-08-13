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
