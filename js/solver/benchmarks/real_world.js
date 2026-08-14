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
const suite = process.env.BENCH_SUITE || 'default';
const isFamilySuite = suite === 'family' || suite.startsWith('family-');
const seconds = isFamilySuite
    ? Math.min(30, Math.max(1, Number(process.env.BENCH_SECONDS) || 30))
    : Math.max(1, Number(process.env.BENCH_SECONDS) || 10);
const workers = Math.max(1, Number(process.env.BENCH_WORKERS) || 1);
const includeIsolation = process.env.BENCH_INCLUDE_ISOLATION !== '0';
const familyNames = ['cancelstack', 'heavy_melee', 'tierstack', 'spellsteal', 'spell_sustained', 'hybrid'];
const familySize = suite === 'family' ? 'medium' : suite.slice('family-'.length);
const allowedFamilySizes = new Set(['small', 'medium', 'large', 'all']);
if (isFamilySuite && !allowedFamilySizes.has(familySize)) {
    throw new Error(`Unknown family benchmark suite: ${suite}`);
}
const familyScenarios = familySize === 'all'
    ? ['small', 'medium', 'large'].flatMap(size => familyNames.map(family => `family_${family}_${size}`))
    : familyNames.map(family => `family_${family}_${familySize}`);
const defaultScenarios = isFamilySuite
    ? familyScenarios
    : suite === 'large'
        ? ['gaia_armor_bracelet_1m', 'gaia_armor_ring_2m', 'gaia_armor_ring_5m',
        'gaia_armor_bracelet_10m', 'gaia_wide_95m_input', 'gaia_armor4_ring_135m_input']
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
    const scoreSamples = runs.map(run => run.bestScore).filter(Number.isFinite);
    const bestScore = scoreSamples.length ? summarize(scoreSamples) : null;
    const checked = summarize(runs.map(run => run.checked));
    const feasible = summarize(runs.map(run => run.feasible));
    return {
        timingMs, checkedPerSecond, bestScore, checked, feasible,
        timedOutRuns: runs.filter(run => run.timedOut).length,
        poolSizes: runs[0].poolSizes,
        combinations: runs[0].combinations,
        inputCombinations: runs[0].inputCombinations,
        dominanceMode: runs[0].dominanceMode,
        dominanceInputItems: runs[0].dominanceInputItems,
        dominanceOutputItems: runs[0].dominanceOutputItems,
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
            { checked: original.checked.median, elapsedMs: original.timingMs.median, bestScore: original.bestScore?.median ?? null, combinations: original.combinations },
            { checked: current.checked.median, elapsedMs: current.timingMs.median, bestScore: current.bestScore?.median ?? null, combinations: current.combinations },
        ),
        topCutoffComparison: includeIsolation ? compareVariants(
            { checked: currentWithoutTopCutoff.checked.median, elapsedMs: currentWithoutTopCutoff.timingMs.median, bestScore: currentWithoutTopCutoff.bestScore?.median ?? null, combinations: currentWithoutTopCutoff.combinations },
            { checked: current.checked.median, elapsedMs: current.timingMs.median, bestScore: current.bestScore?.median ?? null, combinations: current.combinations },
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
        original: 'historical legacy gear-dominance heuristic and pre-project top-N behavior',
        currentWithoutTopCutoff: 'exact raw gear pools with pre-project top-N behavior, isolating the top-N optimization',
        current: 'exact raw gear pools and cutoff-aware top-N behavior',
    },
    runtime: { node: process.version, v8: process.versions.v8, platform: process.platform, arch: process.arch },
    machine: { cpu: os.cpus()[0]?.model ?? 'unknown', logicalCpus: os.cpus().length, memoryBytes: os.totalmem() },
    config: { samples, seconds, workers, includeIsolation, scenarios },
    results,
}, null, 2) + '\n');
