import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { initSync, solve_json, solve_json_with_progress } from '../wasm/pkg/sp_kernel.js';

const wasmPath = fileURLToPath(new URL('../wasm/pkg/sp_kernel_bg.wasm', import.meta.url));
const fixturePath = fileURLToPath(
    new URL('../../../rust/sp_kernel/tests/fixtures/oracle_armor2.enum.txt', import.meta.url),
);

initSync({ module: readFileSync(wasmPath) });
const rejected = JSON.parse(solve_json(JSON.stringify({
    schema_version: 99,
    data_version: 'wasm-smoke-test',
    enumeration_fixture: '',
})));
assert.equal(rejected.status, 'error');
assert.equal(rejected.error.code, 'unsupported_schema_version');

const snapshots = [];
const result = JSON.parse(solve_json_with_progress(JSON.stringify({
    schema_version: 1,
    data_version: 'wasm-smoke-test',
    enumeration_fixture: readFileSync(fixturePath, 'utf8'),
}), (payload) => snapshots.push(JSON.parse(payload))));

assert.equal(result.status, 'completed');
assert.equal(result.engine.target, 'wasm32');
assert.equal(result.exhaustive, true);
assert.equal(result.counters.checked, 36);
assert.equal(result.counters.feasible, 31);
assert.deepEqual(result.top_n, []);
assert.ok(snapshots.length > 0);
assert.equal(snapshots.at(-1).checked, 36);
assert.equal(snapshots.at(-1).total, 36);

console.log('Rust/WASM SearchJob smoke test passed');
