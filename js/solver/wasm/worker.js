import init, { solve_json } from './pkg/sp_kernel.js';

let moduleReady;

async function loadModule() {
    if (!moduleReady) moduleReady = init();
    return moduleReady;
}

self.onmessage = async (event) => {
    const message = event.data;
    if (message.type !== 'solve') return;

    try {
        await loadModule();
        const result = JSON.parse(solve_json(JSON.stringify(message.job)));
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
