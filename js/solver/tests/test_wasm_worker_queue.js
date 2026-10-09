'use strict';

// R24 work-queue protocol of the Rust/WASM worker (wasm/worker.js), run
// against a mock kernel: `session` builds one Engine, `unit` messages are
// handled in order (a unit posted right behind its session waits for the
// engine), each answers `unit_done` in the `done` shape, and failures map
// to the codes the host falls back on. The real engine's exactness is
// checked natively (partition_check queue rows, session_tests) and in the
// page (browser_e2e.js partition phase).

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../wasm/worker.js'), 'utf8')
    .replace(/^import init, \* as kernel from '\.\/sp_kernel\.js';$/m, '');

function worker(kernelOverrides = {}) {
    const messages = [];
    const kernel = { initSync() {}, search_space() { return 1000; }, ...kernelOverrides };
    const context = {
        kernel, init: async () => {},
        performance: { now: () => 0 },
        self: { postMessage: value => messages.push(structuredClone(value)) },
        Int32Array, Atomics, SharedArrayBuffer,
    };
    vm.runInNewContext(source, context, { filename: 'worker.js' });
    return { messages, send: data => context.self.onmessage({ data }) };
}

const unitResult = (n) => JSON.stringify({
    checked: 10 * n, feasible: n, scored: n, gated: 0, mana_reject: 0, bound_pruned: 0,
    complete: true,
    top: [{ score: 100 + n, items: ['a', 'b'], base_sp: [0, 0, 0, 0, 0], total_sp: [0, 0, 0, 0, 0], assigned_sp: 0 }],
});

(async () => {
    let count = 0;
    const calls = [];
    let built = 0;
    class Engine {
        constructor(e, s) { built++; this.e = e; this.s = s; this.n = 0; }
        result_count() { return 15; }
        solve_unit(index, unitCount, cut, best, progress) {
            calls.push({ index, unitCount, cut, best });
            this.n++;
            const top_n = [{ score: 1, item_names: ['a'] }];
            assert.equal(progress(JSON.stringify({ checked: 5, total: 1000, scored: 1, top_n })), undefined);
            return unitResult(this.n);
        }
    }
    const w = worker({ Engine });
    // Posted back to back, as the host does: the unit must wait for the engine.
    const a = w.send({ type: 'session', worker_id: 3, enum_fixture: 'E', score_fixture: 'S' });
    const b = w.send({ type: 'unit', worker_id: 3, index: 0, count: 8, seed_cutoff: 0, seed_best: 0 });
    await Promise.all([a, b]);
    await w.send({ type: 'unit', worker_id: 3, index: 5, count: 8, seed_cutoff: 77, seed_best: 99 });
    assert.equal(built, 1, 'one engine per worker'); count++;
    const ready = w.messages.find(m => m.type === 'ready');
    assert.deepEqual([ready.result_count, ready.total, ready.worker_id], [15, 1000, 3]); count++;
    assert.ok(w.messages.indexOf(ready) < w.messages.findIndex(m => m.type === 'unit_done'),
        'ready precedes the first unit_done'); count++;
    assert.deepEqual(calls, [{ index: 0, unitCount: 8, cut: 0, best: 0 }, { index: 5, unitCount: 8, cut: 77, best: 99 }]); count++;
    const dones = w.messages.filter(m => m.type === 'unit_done');
    assert.equal(dones.length, 2); count++;
    // The `done` shape the host already merges: items -> item_names, counters.
    assert.deepEqual([dones[1].checked, dones[1].met_req, dones[1].complete, dones[1].top_n[0].item_names, dones[1].index],
        [20, 2, true, ['a', 'b'], 5]); count++;
    assert.ok(w.messages.some(m => m.type === 'progress' && m.phase === 'searching' && m.checked === 5)); count++;

    // A unit without a session (or after a failed one) does nothing.
    const orphan = worker({ Engine });
    await orphan.send({ type: 'unit', index: 0, count: 8 });
    assert.equal(orphan.messages.length, 0); count++;

    // The engine rejecting the scenario sends the host to the JS workers.
    const bad = worker({ Engine: class { constructor() { throw new Error('unsupported: hp casting'); } } });
    await bad.send({ type: 'session', enum_fixture: 'E', score_fixture: 'S' });
    await bad.send({ type: 'unit', index: 0, count: 8 });
    assert.equal(bad.messages.at(-1).code, 'unsupported_scenario'); count++;
    assert.equal(bad.messages.filter(m => m.type === 'unit_done').length, 0); count++;

    // A stale bundle without the Engine export reports itself.
    const stale = worker();
    await stale.send({ type: 'session', enum_fixture: 'E', score_fixture: 'S' });
    assert.equal(stale.messages.at(-1).code, 'session_engine_unavailable'); count++;

    // A crash inside a unit is reported, not swallowed.
    const crash = worker({ Engine: class extends Engine { solve_unit() { throw new Error('boom'); } } });
    await crash.send({ type: 'session', enum_fixture: 'E', score_fixture: 'S' });
    await crash.send({ type: 'unit', index: 0, count: 8 });
    assert.equal(crash.messages.at(-1).code, 'rust_worker_crash'); count++;

    console.log(`${count} passed, 0 failed, 0 warnings`);
})().catch(error => { console.error(error); process.exitCode = 1; });
