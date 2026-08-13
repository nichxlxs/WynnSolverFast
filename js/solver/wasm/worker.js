import init, { solve_json_with_progress } from './pkg/sp_kernel.js';

let moduleReady;

async function loadModule() {
    if (!moduleReady) moduleReady = init();
    return moduleReady;
}

self.onmessage = async (event) => {
    const message = event.data;
    if (message.type !== 'solve') return;

    try {
        self.postMessage({
            type: 'progress',
            worker_id: message.worker_id ?? 0,
            phase: 'loading engine',
            checked: 0,
            total: 0,
        });
        await loadModule();
        self.postMessage({
            type: 'progress',
            worker_id: message.worker_id ?? 0,
            phase: 'preparing search',
            checked: 0,
            total: 0,
        });
        const result = JSON.parse(solve_json_with_progress(
            JSON.stringify(message.job),
            (payload) => {
                const progress = JSON.parse(payload);
                self.postMessage({
                    type: 'progress',
                    worker_id: message.worker_id ?? 0,
                    phase: 'searching',
                    checked: progress.checked,
                    total: progress.total,
                    precheck_pass: progress.precheck_pass,
                    precheck_reject: progress.precheck_reject,
                    feasible: progress.feasible,
                    met_req: progress.scored,
                    top5_names: progress.top_n,
                });
            },
        ));
        if (result.status !== 'completed') {
            self.postMessage({
                type: 'worker_error',
                worker_id: message.worker_id ?? 0,
                code: result.error?.code ?? 'rust_engine_error',
                message: result.error?.message ?? 'Rust solver rejected the search job',
            });
            return;
        }
        self.postMessage({
            type: 'done',
            worker_id: message.worker_id ?? 0,
            checked: result.counters.checked,
            precheck_pass: result.counters.precheck_pass,
            precheck_reject: result.counters.precheck_reject,
            feasible: result.counters.feasible,
            met_req: result.counters.scored,
            top5: result.top_n,
            exhaustive: result.exhaustive,
            engine: result.engine,
        });
    } catch (error) {
        self.postMessage({
            type: 'worker_error',
            worker_id: message.worker_id ?? 0,
            code: 'rust_worker_crash',
            message: error?.message ?? String(error),
            stack: error?.stack,
        });
    }
};
