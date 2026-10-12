'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { sanitizeEnumFixtureForAnytime } = require('../engine/rust_bridge.js');

// Execute the production worker's message handler with a controlled clock and
// kernel boundary. Real WASM and browser execution have separate parity gates.
const source = fs.readFileSync(path.join(__dirname, '../wasm/worker.js'), 'utf8')
    .replace(/^import init, \* as kernel from '\.\/sp_kernel\.js';$/m, '');
function worker(overrides = {}) {
    const messages = [];
    let now = 0;
    const kernel = {
        initSync() {}, search_space() { return 10; },
        solve_partition() { throw new Error('Quick search entered exhaustive solver'); },
        ...overrides,
    };
    const context = {
        kernel, init: async () => { now = 250; },
        performance: { now: () => now },
        self: { postMessage: value => messages.push(structuredClone(value)) },
    };
    vm.runInNewContext(source, context, { filename: 'worker.js' });
    return { messages, send: data => context.self.onmessage({ data }) };
}

(async () => {
    let count = 0;
    const fixture = 'PRECHECKS 1\nPC sdPct 20 2\nEHP 1 500 0 0.2\nEHPNA 1 500 0 0.4\nTHP 1 10000 2\nITEM 0 4 9\nNAMES 1\nPC fictional name\nTHP\n\n';
    const sanitized = sanitizeEnumFixtureForAnytime(fixture);
    assert.equal(sanitized, 'PRECHECKS 1\nPC sdPct -1e300 2\nEHP 0 0 0 0\nEHPNA 0 0 0 0\nTHP 0 0 0\nITEM 0 4 9\nNAMES 1\nPC fictional name\nTHP\n\n'); count++;
    assert.equal(sanitizeEnumFixtureForAnytime(sanitized), sanitized); count++;
    assert.throws(() => sanitizeEnumFixtureForAnytime('PC broken')); count++;
    assert.throws(() => sanitizeEnumFixtureForAnytime('')); count++;

    const witness = { score: 321, item_names: ['helmet'], base_sp: [1,2,3,4,5],
        total_sp: [6,7,8,9,10], assigned_sp: 15, tome: { names: ['Tome'] } };
    let calls = 0;
    const active = worker({ solve_anytime_with_progress(_e, _s, json, progress) {
        calls++;
        assert.equal(JSON.parse(json).seconds, 4.75);
        progress(JSON.stringify({ leaf_calls: 1, top_n: [witness], complete: true }));
        return JSON.stringify({ top_n: [witness], stop_reason: 'deadline', complete: true });
    } });
    await active.send({ type: 'solve_anytime', worker_id: 7, options: { seconds: 5 }, enum_fixture: 'enum', score_fixture: 'score' });
    assert.equal(calls, 1); count++;
    const progress = active.messages.find(m => m.top_n?.length);
    assert.deepEqual(progress.top_n[0], witness); count++;
    assert.equal(progress.complete, false); count++;
    const done = active.messages.at(-1);
    assert.equal(done.type, 'done'); assert.equal(done.worker_id, 7);
    assert.equal(done.complete, false); assert.deepEqual(done.top_n[0], witness); count++;

    const expired = worker({ solve_anytime_with_progress() { throw new Error('Expired budget ran'); } });
    await expired.send({ type: 'solve_anytime', options: { seconds: 0.1 } });
    assert.equal(expired.messages.at(-1).stop_reason, 'deadline'); count++;
    const stale = worker();
    await stale.send({ type: 'solve_anytime', options: { seconds: 5 } });
    assert.equal(stale.messages.at(-1).code, 'quick_engine_unavailable'); count++;
    const unsupported = worker({ solve_anytime_with_progress() { return JSON.stringify({ error: 'unsupported fixture' }); } });
    await unsupported.send({ type: 'solve_anytime', options: { seconds: 5 } });
    assert.equal(unsupported.messages.at(-1).code, 'quick_unsupported_scenario'); count++;
    const invalid = worker({ solve_anytime_with_progress() { throw new Error('Invalid budget ran'); } });
    await invalid.send({ type: 'solve_anytime', options: { seconds: Infinity } });
    assert.equal(invalid.messages.at(-1).code, 'quick_invalid_budget'); count++;
    console.log(`${count} passed, 0 failed, 0 warnings`);
})().catch(error => { console.error(error); process.exitCode = 1; });
