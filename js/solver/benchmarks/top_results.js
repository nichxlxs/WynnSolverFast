#!/usr/bin/env node
'use strict';

const os = require('os');
const { performance } = require('perf_hooks');
const { execFileSync } = require('child_process');
const { tryInsertTopResult } = require('../engine/top_results');
const { summarize } = require('./stats');

const capacity = 15;
const samples = Math.max(1, Number(process.env.BENCH_SAMPLES) || 9);
const iterations = Math.max(100, Number(process.env.BENCH_ITERATIONS) || 500000);
const scores = new Float64Array(iterations);
for (let i = 0; i < iterations; i++) {
    // The first results fill the buffer; most later leaves are noncompetitive,
    // matching a search after it has found a strong frontier.
    scores[i] = i < capacity ? 1_000_000 + i : 100_000 - (i % 10_000);
}

function candidate(score, index) {
    return {
        score,
        item_names: [`helmet-${index}`, 'chest', 'legs', 'boots', 'ring', 'ring', 'bracelet', 'necklace'],
        base_sp: [1, 2, 3, 4, 5],
        total_sp: [6, 7, 8, 9, 10],
        assigned_sp: 15,
    };
}

function legacy() {
    const top = [];
    let allocations = 0;
    for (let i = 0; i < scores.length; i++) {
        top.push(candidate(scores[i], i));
        allocations++;
        top.sort((a, b) => b.score - a.score);
        if (top.length > capacity) top.length = capacity;
    }
    return { top, allocations };
}

function cutoff() {
    const top = [];
    let allocations = 0;
    for (let i = 0; i < scores.length; i++) {
        tryInsertTopResult(top, scores[i], () => {
            allocations++;
            return candidate(scores[i], i);
        }, capacity);
    }
    return { top, allocations };
}

const reference = legacy();
const optimized = cutoff();
const signature = value => value.top.map(entry => `${entry.score}:${entry.item_names.join('|')}`).join('\n');
if (signature(reference) !== signature(optimized)) {
    throw new Error('optimized top-results buffer differs from legacy output');
}

function measure(run) {
    run();
    const values = [];
    let allocations = 0;
    for (let sample = 0; sample < samples; sample++) {
        const start = performance.now();
        const result = run();
        values.push(performance.now() - start);
        allocations = result.allocations;
    }
    return {
        timingMs: summarize(values),
        medianCandidatesPerSecond: iterations / summarize(values).median * 1000,
        allocations,
    };
}

let commit = null;
try { commit = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(); } catch (_) { /* optional metadata */ }

process.stdout.write(JSON.stringify({
    benchmark: 'top_results',
    generatedAt: new Date().toISOString(), commit,
    runtime: { node: process.version, v8: process.versions.v8, platform: process.platform, arch: process.arch },
    machine: { cpu: os.cpus()[0]?.model ?? 'unknown', logicalCpus: os.cpus().length, memoryBytes: os.totalmem() },
    config: { samples, iterations, capacity },
    cases: { legacy: measure(legacy), cutoff: measure(cutoff) },
}, null, 2) + '\n');
