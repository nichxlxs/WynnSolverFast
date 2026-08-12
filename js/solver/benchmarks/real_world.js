#!/usr/bin/env node
'use strict';

const path = require('path');
const os = require('os');
const { execFileSync } = require('child_process');
const { summarize } = require('./stats');
const { parseSolverOutput, compareVariants } = require('./real_world_lib');

const repoRoot = path.resolve(__dirname, '..', '..', '..');
const testScript = path.join(repoRoot, 'js', 'solver', 'tests', 'test_solver_search.js');
const samples = Math.max(1, Number(process.env.BENCH_SAMPLES) || 3);
const seconds = Math.max(1, Number(process.env.BENCH_SECONDS) || 10);
const workers = Math.max(1, Number(process.env.BENCH_WORKERS) || 1);
const includeIsolation = process.env.BENCH_INCLUDE_ISOLATION !== '0';
const defaultScenarios = process.env.BENCH_SUITE === 'large'
    ? ['gaia_armor_bracelet_1m', 'gaia_armor_ring_2m', 'gaia_armor_ring_5m',
        'gaia_armor_bracelet_10m', 'gaia_wide_95m_input']
    : ['readme_rings2', 'readme_armor2', 'readme_armor4'];
const scenarios = (process.env.BENCH_SCENARIOS
    ? process.env.BENCH_SCENARIOS.split(',')
    : defaultScenarios)
    .map(value => value.trim()).filter(Boolean);

function run(scenario, variant) {
    const output = execFileSync(process.execPath, [testScript, scenario], {
        cwd: repoRoot,
        encoding: 'utf8',
        timeout: (seconds + 30) * 1000,
        env: {
            ...process.env,
            SOLVER_BENCH_VARIANT: variant,
            SOLVER_BENCH_SECONDS: String(seconds),
            SOLVER_BENCH_WORKERS: String(workers),
        },
    });
    return parseSolverOutput(output);
}

function aggregate(runs) {
    const timingMs = summarize(runs.map(run => run.elapsedMs));
    const checkedPerSecond = summarize(runs.map(run => run.checkedPerSecond));
    const bestScore = summarize(runs.map(run => run.bestScore));
    const checked = summarize(runs.map(run => run.checked));
    const feasible = summarize(runs.map(run => run.feasible));
    return {
        timingMs, checkedPerSecond, bestScore, checked, feasible,
        timedOutRuns: runs.filter(run => run.timedOut).length,
        poolSizes: runs[0].poolSizes,
        combinations: runs[0].combinations,
        inputCombinations: runs[0].inputCombinations,
        runs,
    };
}

const results = [];
for (const scenario of scenarios) {
    const variants = includeIsolation
        ? ['original', 'current_without_top_cutoff', 'current']
        : ['original', 'current'];
    const runs = Object.fromEntries(variants.map(variant => [variant, []]));
    for (let sample = 0; sample < samples; sample++) {
        // Alternate order to reduce systematic warm-machine and thermal bias.
        const order = sample % 2 ? [...variants].reverse() : variants;
        for (const variant of order) runs[variant].push(run(scenario, variant));
    }
    const original = aggregate(runs.original);
    const currentWithoutTopCutoff = includeIsolation
        ? aggregate(runs.current_without_top_cutoff) : null;
    const current = aggregate(runs.current);
    results.push({
        scenario,
        original,
        currentWithoutTopCutoff,
        current,
        comparison: compareVariants(
            { checked: original.checked.median, elapsedMs: original.timingMs.median, bestScore: original.bestScore.median, combinations: original.combinations },
            { checked: current.checked.median, elapsedMs: current.timingMs.median, bestScore: current.bestScore.median, combinations: current.combinations },
        ),
        topCutoffComparison: includeIsolation ? compareVariants(
            { checked: currentWithoutTopCutoff.checked.median, elapsedMs: currentWithoutTopCutoff.timingMs.median, bestScore: currentWithoutTopCutoff.bestScore.median, combinations: currentWithoutTopCutoff.combinations },
            { checked: current.checked.median, elapsedMs: current.timingMs.median, bestScore: current.bestScore.median, combinations: current.combinations },
        ) : null,
    });
}

let commit = null;
try { commit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repoRoot, encoding: 'utf8' }).trim(); }
catch (_) { /* optional metadata */ }

process.stdout.write(JSON.stringify({
    benchmark: 'real_world_solver',
    generatedAt: new Date().toISOString(),
    commit,
    definition: {
        original: 'pre-project top-N allocation/sort and original set dominance behavior',
        currentWithoutTopCutoff: 'set-safe dominance with pre-project top-N allocation/sort, isolating the top-N optimization',
        current: 'cutoff-aware top-N and set-safe dominance behavior',
    },
    runtime: { node: process.version, v8: process.versions.v8, platform: process.platform, arch: process.arch },
    machine: { cpu: os.cpus()[0]?.model ?? 'unknown', logicalCpus: os.cpus().length, memoryBytes: os.totalmem() },
    config: { samples, seconds, workers, includeIsolation, scenarios },
    results,
}, null, 2) + '\n');
