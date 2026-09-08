#!/usr/bin/env node
/**
 * Node-hosted WASM quick-search validation, not a browser UI timing benchmark.
 * Uses the shipped wasm-bindgen glue/binary in a fresh Worker for every run.
 * The parent enforces the total budget, including fixture reads and cold load,
 * and retains only witnesses received before that deadline. Fixture generation
 * is reported separately. All runs are sequential; do not run competing builds.
 *
 * node benchmark_quick_wasm.mjs --fixtures DIR --out OUTPUT.json
 *     [--mode parity|timed|clock] [--arms alns|exact,alns]
 *     [--scenarios all|confirmation|NAME,...]
 *     [--seeds 707,808,909] [--seconds 5] [--work-budget 100000]
 *     [--elite-pool 0|1] [--warm-budget 2000000] [--repair-budget 100000]
 *     [--native target/release/quick_kernel_json] [--self-test]
 *
 * Parity uses a fixed actual-leaf budget and compares every returned item,
 * assigned/total skill-point and tome witness against the native API. It does
 * not call equal-budget timed endpoints "parity". Timed milestones use the
 * previously frozen, hash-matched screening targets, never a new run maximum.
 * Clock mode gives the kernel --seconds without a matching parent timeout;
 * --throw-callback additionally checks the WASM callback exception boundary.
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {performance} from 'node:perf_hooks';
import {execFileSync} from 'node:child_process';
import {Worker, isMainThread, parentPort, workerData} from 'node:worker_threads';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '../..');
const PROFILE = Object.freeze({seed: 707, top_k: 15, warm_k: 6,
    warm_budget: 2_000_000, repair_budget: 100_000,
    max_repairs: 10_000, cycle_stagnation: true});
const sha256 = data => crypto.createHash('sha256').update(data).digest('hex');
const readJSON = p => JSON.parse(fs.readFileSync(p, 'utf8'));

function selection(rows, names) {
    const picked = new Map();
    for (const name of names) {
        const matches = rows.filter(q => name === 'all' ||
            (name === 'confirmation' && ((q.group === 'meta' && q.variant === 'remove_6') ||
                (q.group === 'families' && q.variant === 'large') || q.name === 'gaia_all_free')) ||
            (name === 'known_controls' && q.variant === 'known_good') ||
            (name === 'parity_small' && q.group === 'meta' &&
                ['known_good', 'remove_1', 'remove_2'].includes(q.variant)) ||
            name === q.name || name === q.snapshot);
        if (!matches.length) throw Error(`Unknown or empty scenario selector: ${name}`);
        for (const q of matches) picked.set(q.name, q);
    }
    return [...picked.values()];
}

function normalizedTop(payload) {
    return (payload?.top_n ?? payload?.top ?? []).map(e => ({score: e.score,
        items: e.item_names ?? e.items, base_sp: e.base_sp, total_sp: e.total_sp,
        assigned_sp: e.assigned_sp, tome: e.tome ?? null}));
}

function identity(entry) {
    const items = [...entry.items];
    if (items.length === 8 && items[4] > items[5]) [items[4], items[5]] = [items[5], items[4]];
    const tome = entry.tome && {guild_idx: entry.tome.guild_idx,
        weaponTome: [...entry.tome.weaponTome].sort(), armorTome: [...entry.tome.armorTome].sort()};
    return JSON.stringify([items, tome]);
}

function validateArchive(payload, score, topK = 15, algorithm = 'alns') {
    const errors = [];
    if (algorithm === 'alns' && payload?.complete !== false) errors.push('Heuristic payload must have complete=false');
    if (payload?.algorithm !== algorithm) errors.push('Unexpected algorithm');
    const entries = normalizedTop(payload), seen = new Set();
    if (entries.length > topK) errors.push('Archive exceeds requested top_k');
    const registry = score?.layer2?.item_registry?.__m ?? score?.layer2?.item_registry;
    let previous = Infinity;
    for (const [i, e] of entries.entries()) {
        if (!Number.isFinite(e.score) || e.score > previous) errors.push(`entry ${i}: invalid score order`);
        previous = e.score;
        if (!Array.isArray(e.items) || e.items.length !== 8 || e.items.some(n => typeof n !== 'string')) {
            errors.push(`entry ${i}: missing eight-item witness`); continue;
        }
        if (registry && e.items.some(n => !(n in registry))) errors.push(`entry ${i}: item absent from fixture registry`);
        if (![e.base_sp, e.total_sp].every(a => Array.isArray(a) && a.length === 5 && a.every(Number.isFinite))) {
            errors.push(`entry ${i}: invalid skill-point witness`);
        } else {
            if (e.base_sp.some(x => !Number.isInteger(x) || x < 0 || x > 100)) errors.push(`entry ${i}: invalid manual skill points`);
            if (Math.abs(e.base_sp.reduce((a, b) => a + b, 0) - e.assigned_sp) > 1e-9) errors.push(`entry ${i}: assigned SP total mismatch`);
        }
        if (!Number.isFinite(e.assigned_sp) || e.assigned_sp > score.layer2.sp_budget) errors.push(`entry ${i}: exceeds SP budget`);
        if (e.tome && (!Number.isInteger(e.tome.guild_idx) ||
            !Array.isArray(e.tome.weaponTome) || !Array.isArray(e.tome.armorTome))) {
            errors.push(`entry ${i}: invalid tome witness`); continue;
        }
        const key = identity(e);
        if (seen.has(key)) errors.push(`entry ${i}: duplicate gear/tome identity`);
        seen.add(key);
    }
    return errors;
}

function compareResults(native, wasm) {
    const errors = [];
    const a = normalizedTop(native), b = normalizedTop(wasm);
    if (a.length !== b.length) errors.push(`archive length ${a.length} != ${b.length}`);
    for (const key of ['leaf_calls', 'scored', 'repairs', 'completed_repairs', 'stop_reason']) {
        if (native[key] !== wasm[key]) errors.push(`${key}: ${native[key]} != ${wasm[key]}`);
    }
    for (let i = 0; i < Math.min(a.length, b.length); i++) {
        if (Math.abs(a[i].score - b[i].score) > Math.max(1, Math.abs(a[i].score)) * 1e-10) errors.push(`entry ${i}: score mismatch`);
        for (const key of ['items', 'base_sp', 'total_sp', 'assigned_sp', 'tome']) {
            if (JSON.stringify(a[i][key]) !== JSON.stringify(b[i][key])) errors.push(`entry ${i}: ${key} mismatch`);
        }
    }
    return errors;
}

async function workerMain() {
    const begun = performance.now();
    try {
        const glue = fs.readFileSync(workerData.glue, 'utf8');
        const wasm = await import('data:text/javascript;base64,' + Buffer.from(glue).toString('base64'));
        wasm.initSync({module: fs.readFileSync(workerData.wasm)});
        const load = (performance.now() - begun) / 1000;
        const opts = {...workerData.options};
        if (workerData.chargeLoad) opts.seconds = Math.max(0, opts.seconds - load);
        parentPort.postMessage({type: 'loaded', load_seconds: load, kernel_budget_seconds: opts.seconds});
        const kernelStart = performance.now();
        const callback = text => {
            const payload = JSON.parse(text);
            if (workerData.arm === 'exact') payload.algorithm = 'exact';
            parentPort.postMessage({type: 'progress', payload});
            if (workerData.throwCallback) throw Error('Intentional callback failure for WASM boundary regression');
        };
        let raw;
        if (workerData.arm === 'exact') {
            raw = wasm.solve_with_progress(workerData.enumText, workerData.scoreText, 0, callback);
        } else {
            if (typeof wasm.solve_anytime_with_progress !== 'function') throw Error('Shipped WASM glue lacks solve_anytime_with_progress; rebuild WASM');
            raw = wasm.solve_anytime_with_progress(workerData.enumText,
                workerData.scoreText, JSON.stringify(opts), callback);
        }
        const result = JSON.parse(raw);
        if (workerData.arm === 'exact') result.algorithm = 'exact';
        parentPort.postMessage({type: 'done', payload: result,
            kernel_call_seconds: (performance.now() - kernelStart) / 1000});
    } catch (e) {
        parentPort.postMessage({type: 'error', error: String(e.stack ?? e)});
    }
}

async function wasmRun(args, query, options, arm = 'alns') {
    const hostStarted = performance.now();
    const enumPath = path.join(args.fixtures, query.enum_file), scorePath = path.join(args.fixtures, query.score_file);
    const enumText = fs.readFileSync(enumPath, 'utf8'), scoreText = fs.readFileSync(scorePath, 'utf8');
    const fixtureHashes = {enum_sha256: sha256(enumText), score_sha256: sha256(scoreText)};
    if (fixtureHashes.enum_sha256 !== query.enum_sha256 || fixtureHashes.score_sha256 !== query.score_sha256) {
        throw Error(`${query.name}: fixture hashes do not match index`);
    }
    const score = JSON.parse(scoreText);
    const preparation = (performance.now() - hostStarted) / 1000;
    const timed = args.mode === 'timed';
    const hardBudget = timed ? args.seconds : args.parityTimeout;
    const remaining = hardBudget - preparation;
    const details = {scenario: query.name, group: query.group, variant: query.variant, arm,
        seed: options.seed, options, ...fixtureHashes, complete: false,
        host_budget_seconds: hardBudget, preparation_seconds: preparation,
        fixture_generation_seconds: query.generation_wall_seconds,
        worker_load_seconds: null, kernel_call_seconds: null, kernel_budget_seconds: null,
        first_result_host_seconds: null, progress_messages: 0, trajectory: [],
        status: 'pending', error: null, result: null};
    if (remaining <= 0) return {...details, status: 'host_deadline_before_worker',
        host_wall_seconds: preparation, cleanup_seconds: 0, archive: [], validation_errors: []};
    const worker = new Worker(new URL(import.meta.url), {workerData: {
        glue: path.join(args.repo, 'js/solver/wasm/sp_kernel.js'),
        wasm: path.join(args.repo, 'js/solver/wasm/sp_kernel_bg.wasm'),
        arm, enumText, scoreText, options: {...options, seconds: timed ? remaining : options.seconds},
        chargeLoad: timed, throwCallback: args.throwCallback}});
    let retained = null, previousBest = -Infinity;
    await new Promise(resolve => {
        let done = false;
        const elapsed = () => (performance.now() - hostStarted) / 1000;
        const settle = (status, error = null) => {
            if (done) return;
            done = true; clearTimeout(timer);
            details.status = status; details.error = error; details.host_wall_seconds = elapsed();
            resolve();
        };
        const timer = setTimeout(() => settle('host_deadline'), remaining * 1000);
        worker.on('message', message => {
            if (done) return;
            const observed = elapsed();
            if (observed > hardBudget) { settle('host_deadline'); return; }
            if (message.type === 'loaded') {
                details.worker_load_seconds = message.load_seconds;
                details.kernel_budget_seconds = message.kernel_budget_seconds;
            } else if (message.type === 'progress' || message.type === 'done') {
                const payload = message.payload;
                if (payload.error) { settle('kernel_error', payload.error); return; }
                const entries = normalizedTop(payload);
                if (entries.length) {
                    retained = payload;
                    if (details.first_result_host_seconds === null) details.first_result_host_seconds = observed;
                    if (entries[0].score > previousBest) {
                        previousBest = entries[0].score;
                        details.trajectory.push({observed_seconds: observed, kernel_seconds: payload.elapsed_secs,
                            score: entries[0].score, leaf_calls: payload.leaf_calls, phase: payload.phase ?? 'finished'});
                    }
                }
                if (message.type === 'progress') details.progress_messages++;
                else {
                    details.result = payload;
                    details.kernel_call_seconds = message.kernel_call_seconds;
                    retained = payload;
                    settle('finished');
                }
            } else if (message.type === 'error') settle('worker_error', message.error);
        });
        worker.on('error', e => settle('worker_error', String(e.stack ?? e)));
        worker.on('exit', code => { if (!done) settle('worker_exit', `Worker exited ${code} without final result`); });
    });
    const cleanupStart = performance.now();
    await worker.terminate();
    details.cleanup_seconds = (performance.now() - cleanupStart) / 1000;
    details.archive = normalizedTop(retained);
    details.retained_payload = retained;
    details.complete = details.result?.complete === true;
    details.best_score = details.archive[0]?.score ?? null;
    details.validation_errors = retained ? validateArchive(retained, score, options.top_k, arm) : [];
    return details;
}

function parseArgs(argv) {
    const opts = {repo: REPO, fixtures: null, out: null, mode: 'parity',
        scenarios: ['all'], seeds: [707], arms: ['alns'], seconds: 5, workBudget: 100000, parityTimeout: 60,
        elitePool: null, warmBudget: null, repairBudget: null,
        native: path.join(HERE, 'target/release/quick_kernel_json'),
        references: path.join(HERE, 'evidence/anytime_2026_09_07/confirmation_setup/screening_references.json')};
    const names = {'--repo': 'repo', '--fixtures': 'fixtures', '--out': 'out', '--mode': 'mode',
        '--scenarios': 'scenarios', '--seeds': 'seeds', '--arms': 'arms', '--seconds': 'seconds', '--work-budget': 'workBudget',
        '--native': 'native', '--references': 'references', '--parity-timeout': 'parityTimeout',
        '--elite-pool': 'elitePool', '--warm-budget': 'warmBudget', '--repair-budget': 'repairBudget'};
    for (let i = 0; i < argv.length; i++) {
        if (argv[i] === '--self-test') { opts.selfTest = true; continue; }
        if (argv[i] === '--throw-callback') { opts.throwCallback = true; continue; }
        const key = names[argv[i]];
        if (!key || i + 1 >= argv.length) throw Error(`Unknown/incomplete option ${argv[i]}`);
        let value = argv[++i];
        if (key === 'scenarios' || key === 'arms') value = value.split(',');
        else if (key === 'seeds') value = value.split(',').map(Number);
        else if (['seconds', 'workBudget', 'parityTimeout', 'elitePool', 'warmBudget', 'repairBudget'].includes(key)) value = Number(value);
        opts[key] = value;
    }
    if (opts.selfTest) return opts;
    if (!opts.fixtures || !opts.out) throw Error('--fixtures DIR and --out OUTPUT.json are required');
    if (!['parity', 'timed', 'clock'].includes(opts.mode)) throw Error('--mode must be parity, timed or clock');
    if (opts.arms.some(a => !['exact', 'alns'].includes(a)) ||
        (opts.mode === 'parity' && opts.arms.some(a => a !== 'alns'))) throw Error('Parity supports ALNS only; timed arms are exact and alns');
    if (!(opts.seconds > 0 && opts.seconds <= 300) || !(opts.parityTimeout > 0 && Number.isFinite(opts.parityTimeout)) ||
        !Number.isSafeInteger(opts.workBudget) || opts.workBudget < 1 ||
        opts.seeds.some(s => !Number.isSafeInteger(s) || s < 0)) throw Error('Invalid finite budget or seed');
    if (opts.elitePool !== null && ![0, 1].includes(opts.elitePool)) throw Error('--elite-pool must be 0 or 1');
    for (const key of ['warmBudget', 'repairBudget']) {
        if (opts[key] !== null && (!Number.isInteger(opts[key]) || opts[key] < 1 || opts[key] > 10_000_000)) throw Error(`Invalid ${key}`);
    }
    for (const key of ['repo', 'fixtures', 'out', 'native', 'references']) opts[key] = path.resolve(opts[key]);
    return opts;
}

function selfTest() {
    const manifest = readJSON(path.join(HERE, 'quality_suite.json'));
    assert.equal(selection(manifest.scenarios, ['all']).length, 132);
    assert.equal(selection(manifest.scenarios, ['confirmation']).length, 22);
    assert.equal(selection(manifest.scenarios, ['parity_small']).length, 45);
    assert.throws(() => selection(manifest.scenarios, ['typo']));
    const e = {score: 10, items: ['a', 'b', 'c', 'd', 'r2', 'r1', 'f', 'g'],
        base_sp: [1, 2, 3, 4, 5], total_sp: [1, 2, 3, 4, 5], assigned_sp: 15};
    const result = {algorithm: 'alns', complete: false, top: [e]};
    const score = {layer2: {sp_budget: 200}};
    assert.deepEqual(validateArchive(result, score), []);
    assert(validateArchive({...result, complete: true}, score).length);
    assert(validateArchive({...result, top: [e, {...e, items: ['a', 'b', 'c', 'd', 'r1', 'r2', 'f', 'g']}]}, score).length);
    assert.deepEqual(compareResults(result, result), []);
    assert(compareResults(result, {...result, top: [{...e, assigned_sp: 14}]}).length);
    assert(compareResults(result, {...result, top: [{...e, score: 11}]}).length);
    assert.throws(() => parseArgs(['--fixtures', '.', '--out', 'x', '--work-budget', 'NaN']));
    console.log('quick WASM benchmark integrity: 11 assertions passed');
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    if (args.selfTest) { selfTest(); return; }
    const index = readJSON(path.join(args.fixtures, 'index.json'));
    const queries = selection(index.scenarios, args.scenarios);
    if (queries.some(q => q.status !== 'ok' || q.dominance_mode !== 'off' || q.precheck_mode !== 'disabled')) {
        throw Error('Benchmark requires successful raw-pool, disabled-unproved-precheck fixtures');
    }
    const refs = fs.existsSync(args.references) ? readJSON(args.references) : {};
    const wasmPath = path.join(args.repo, 'js/solver/wasm/sp_kernel_bg.wasm');
    const output = {schema_version: 1, mode: args.mode, created_at: new Date().toISOString(),
        scope: 'Node-hosted WASM, fresh worker and cold module initialization each run. Not browser UI timings. Fixture reads and cold loading count toward host deadline; prior fixture generation does not.',
        quality_scope: 'T99 is 99% of a prior frozen, same-fixture screening best-known score, not a global optimality guarantee. Archive validation checks witness structure and native parity, not independent game accuracy.',
        runtime: {node: process.version, platform: process.platform, arch: process.arch,
            cpus: os.cpus()[0]?.model, cpu_count: os.cpus().length},
        source_commit: execFileSync('git', ['rev-parse', 'HEAD'], {cwd: args.repo, encoding: 'utf8'}).trim(),
        source_dirty: Boolean(execFileSync('git', ['status', '--porcelain'], {cwd: args.repo, encoding: 'utf8'}).trim()),
        fixture_index_sha256: sha256(fs.readFileSync(path.join(args.fixtures, 'index.json'))),
        wasm_sha256: sha256(fs.readFileSync(wasmPath)),
        glue_sha256: sha256(fs.readFileSync(path.join(args.repo, 'js/solver/wasm/sp_kernel.js'))),
        harness_sha256: sha256(fs.readFileSync(fileURLToPath(import.meta.url))),
        native_sha256: args.mode === 'parity' ? sha256(fs.readFileSync(args.native)) : null,
        references_sha256: fs.existsSync(args.references) ? sha256(fs.readFileSync(args.references)) : null,
        config: args, profile: PROFILE, queries: queries.map(q => q.name), runs: []};
    fs.mkdirSync(path.dirname(args.out), {recursive: true});
    const save = () => fs.writeFileSync(args.out, JSON.stringify(output, null, 2) + '\n');
    save();
    let failures = 0;
    for (const [queryIndex, query] of queries.entries()) for (const [seedIndex, seed] of args.seeds.entries())
      for (const arm of (queryIndex + seedIndex) % 2 ? [...args.arms].reverse() : args.arms) {
        const options = {...PROFILE, seed, seconds: args.mode === 'parity' ? 300 : args.seconds};
        if (args.mode === 'parity') options.work_budget = args.workBudget;
        if (args.elitePool !== null) options.elite_pool = Boolean(args.elitePool);
        if (args.warmBudget !== null) options.warm_budget = args.warmBudget;
        if (args.repairBudget !== null) options.repair_budget = args.repairBudget;
        let row;
        try {
            row = await wasmRun(args, query, options, arm);
            if (args.mode === 'parity') {
                const optionPath = path.join(path.dirname(args.out), '.quick-options.json');
                fs.writeFileSync(optionPath, JSON.stringify(options));
                try {
                    const stdout = execFileSync(args.native,
                        [path.join(args.fixtures, query.enum_file), path.join(args.fixtures, query.score_file), optionPath],
                        {encoding: 'utf8', timeout: args.parityTimeout * 1000, maxBuffer: 16 * 1024 * 1024});
                    row.native_result = JSON.parse(stdout);
                    row.parity_errors = row.result ? compareResults(row.native_result, row.result) : ['No complete API return available for deterministic parity'];
                    if (row.native_result.stop_reason === 'time_budget' || row.result?.stop_reason === 'time_budget') row.parity_errors.push('Wall cap reached: this is not a fixed-work comparison');
                } finally { fs.rmSync(optionPath, {force: true}); }
            }
            if (args.mode === 'clock') {
                row.clock_errors = [];
                if (row.status !== 'finished' || row.result?.stop_reason !== 'time_budget') row.clock_errors.push('Kernel did not return on its own time budget');
                if (!(row.result?.elapsed_secs > 0 && row.result.elapsed_secs < args.seconds + 1)) row.clock_errors.push('WASM monotonic elapsed time invalid or excessive');
                if (args.throwCallback && !row.progress_messages) row.clock_errors.push('Throwing callback was never exercised');
            }
            const ref = refs[query.name];
            row.reference = ref && ref.enum_sha256 === row.enum_sha256 && ref.score_sha256 === row.score_sha256 ? ref : null;
            row.reference_status = !ref ? 'no_frozen_reference' : row.reference ? 'matching_frozen_reference' : 'fixture_hash_mismatch';
            row.t99_host_seconds = row.reference ? row.trajectory.find(t => t.score >= row.reference.score * 0.99)?.observed_seconds ?? null : null;
            row.t999_host_seconds = row.reference ? row.trajectory.find(t => t.score >= row.reference.score * 0.999)?.observed_seconds ?? null : null;
            if (row.validation_errors.length || row.parity_errors?.length || row.clock_errors?.length ||
                !['finished', 'host_deadline', 'host_deadline_before_worker'].includes(row.status)) failures++;
        } catch (e) {
            row = {scenario: query.name, seed, arm, status: 'harness_error', error: String(e.stack ?? e)};
            failures++;
        }
        output.runs.push(row); save();
        console.log(`${query.name} ${arm} seed=${seed}: ${row.status}, n=${row.archive?.length ?? 0}, best=${row.best_score ?? 'none'}, T99=${row.t99_host_seconds ?? 'n/a'}, errors=${(row.validation_errors?.length ?? 0) + (row.parity_errors?.length ?? 0) + (row.clock_errors?.length ?? 0)}`);
    }
    output.summary = {runs: output.runs.length, failures,
        returned_api: output.runs.filter(r => r.status === 'finished').length,
        host_deadlines: output.runs.filter(r => r.status?.startsWith('host_deadline')).length,
        with_result: output.runs.filter(r => r.archive?.length).length,
        with_frozen_reference: output.runs.filter(r => r.reference).length,
        attained_t99: output.runs.filter(r => r.t99_host_seconds != null).length,
        attained_t999: output.runs.filter(r => r.t999_host_seconds != null).length,
        parity_pass: output.runs.filter(r => r.parity_errors?.length === 0).length};
    save();
    console.log(JSON.stringify(output.summary));
    if (failures) process.exitCode = 1;
}

if (isMainThread) await main(); else await workerMain();
