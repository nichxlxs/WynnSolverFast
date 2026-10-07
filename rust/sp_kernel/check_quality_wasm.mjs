// Exhaustive top-15 score parity on the 45 recovered control/remove-one/two
// fixtures. Run after build-wasm.sh and a native release build.
// Usage: node check_quality_wasm.mjs REPO FIXTURES [OUTPUT.json]
import fs from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';

const [repoArg, fixtureArg, output] = process.argv.slice(2);
if (!repoArg || !fixtureArg) throw Error('usage: check_quality_wasm.mjs REPO FIXTURES [OUTPUT.json]');
const repo = path.resolve(repoArg), fixtures = path.resolve(fixtureArg);
const glue = fs.readFileSync(path.join(repo, 'js/solver/wasm/sp_kernel.js'), 'utf8');
const wasm = await import('data:text/javascript;base64,' + Buffer.from(glue).toString('base64'));
wasm.initSync({module: fs.readFileSync(path.join(repo, 'js/solver/wasm/sp_kernel_bg.wasm'))});
const index = JSON.parse(fs.readFileSync(path.join(fixtures, 'index.json')));
const queries = index.scenarios.filter(q => q.group === 'meta' &&
  ['known_good', 'remove_1', 'remove_2'].includes(q.variant));
if (queries.length !== 45) throw Error(`expected 45 fixtures, got ${queries.length}`);
const results = [];
for (const query of queries) {
  const enumPath = path.join(fixtures, query.enum_file);
  const scorePath = path.join(fixtures, query.score_file);
  const result = JSON.parse(wasm.solve(fs.readFileSync(enumPath, 'utf8'), fs.readFileSync(scorePath, 'utf8'), 0));
  const env = {...process.env};
  for (const key of ['ENUM_TIME_CAP_SECS', 'ENUM_LEAF_BUDGET', 'QUALITY_TRACE_PATH',
    'BOUND_DEPTH', 'BOUND_TAIL', 'BOUND_CLUSTER', 'SP_BOUND_OFF', 'SCORE_CEILING_GATE']) delete env[key];
  Object.assign(env, {WARM_K: '3', WIDE_BOUND_KEYS: '1', RESULT_COUNT: '15', RETAIN_WARM: '1'});
  const native = execFileSync(path.join(repo, 'rust/sp_kernel/target/release/enum_kernel'),
    [enumPath, '1', scorePath], {encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], env});
  const nativeScores = Array.from(native.matchAll(/^top15: ([^ ]+) \|/gm), x => Number(x[1]));
  const wasmScores = result.top.map(entry => entry.score);
  if (!result.complete || !native.includes('| complete true |') ||
      nativeScores.length !== wasmScores.length || nativeScores.some((value, i) =>
        !Number.isFinite(value) || !Number.isFinite(wasmScores[i]) ||
        Math.abs(value - wasmScores[i]) > Math.max(1, Math.abs(value)) * 1e-10)) {
    throw Error(`${query.name}: parity failed ${JSON.stringify({nativeScores, wasmScores, complete: result.complete})}`);
  }
  results.push({scenario: query.name, complete: result.complete, checked: result.checked, scores: wasmScores});
}
if (output) fs.writeFileSync(output, JSON.stringify(results, null, 2) + '\n');
console.log(`WASM/native exhaustive top-15 score parity: ${results.length} cases PASS`);
