import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { initSync, solve_json } from '../wasm/pkg/sp_kernel.js';

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

const result = JSON.parse(solve_json(JSON.stringify({
    schema_version: 1,
    data_version: 'wasm-smoke-test',
    enumeration_fixture: readFileSync(fixturePath, 'utf8'),
})));

assert.equal(result.status, 'completed');
assert.equal(result.engine.target, 'wasm32');
assert.equal(result.exhaustive, true);
assert.equal(result.counters.checked, 36);
assert.equal(result.counters.feasible, 31);
assert.deepEqual(result.top_n, []);

console.log('Rust/WASM SearchJob smoke test passed');
