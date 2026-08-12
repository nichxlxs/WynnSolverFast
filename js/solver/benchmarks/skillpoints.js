#!/usr/bin/env node
'use strict';

const os = require('os');
const { performance } = require('perf_hooks');
const { execFileSync } = require('child_process');
const { createSandbox } = require('../tests/harness');
const { summarize } = require('./stats');

const ctx = createSandbox();
const calculate = ctx.calculate_skillpoints;
ctx.sets = new Map();

const samples = Math.max(1, Number(process.env.BENCH_SAMPLES) || 7);
const iterations = Math.max(1, Number(process.env.BENCH_ITERATIONS) || 20000);
const warmupIterations = Math.max(1, Math.floor(iterations / 5));

function item(reqs, skillpoints) {
    return new Map([
        ['reqs', reqs], ['skillpoints', skillpoints],
        ['crafted', false], ['set', null],
    ]);
}

const weapon = item([0, 0, 0, 0, 0], [0, 0, 0, 0, 0]);
const passive = item([20, 0, 0, 0, 0], [0, 0, 0, 0, 0]);

function cascade(count) {
    const result = [];
    for (let i = 0; i < count; i++) {
        const attr = i % 5;
        const reqs = [0, 0, 0, 0, 0];
        const skillpoints = [0, 0, 0, 0, 0];
        reqs[attr] = 25 + i * 3;
        skillpoints[(attr + 1) % 5] = 5 + (i % 3);
        result.push(item(reqs, skillpoints));
    }
    while (result.length < 8) result.push(passive);
    return result;
}

function scratch() {
    return {
        no_bonus: new Array(9), assign: [0, 0, 0, 0, 0], final: [0, 0, 0, 0, 0],
        free_bonus: [0, 0, 0, 0, 0], max_passive_req: [0, 0, 0, 0, 0],
        ord_items: new Array(9), ord_reqs: new Array(9), ord_skp: new Array(9),
        post_floor: [0, 0, 0, 0, 0], running_bonus: [0, 0, 0, 0, 0],
        best_assign: [0, 0, 0, 0, 0], save_stack: new Array(45),
        total_item_skp: [0, 0, 0, 0, 0],
    };
}

function timeCase(equipment, useScratch, count) {
    const setCounts = useScratch ? new Map() : null;
    const workspace = useScratch ? scratch() : null;
    const start = performance.now();
    let feasible = 0;
    for (let i = 0; i < count; i++) {
        if (calculate(equipment, weapon, 200, setCounts, workspace)) feasible++;
    }
    return { elapsedMs: performance.now() - start, feasible };
}

const cases = [];
for (const cascadeItems of [0, 4, 8]) {
    const equipment = cascade(cascadeItems);
    for (const useScratch of [false, true]) {
        timeCase(equipment, useScratch, warmupIterations);
        const timings = [];
        let feasible = 0;
        for (let sample = 0; sample < samples; sample++) {
            const result = timeCase(equipment, useScratch, iterations);
            timings.push(result.elapsedMs);
            feasible = result.feasible;
        }
        const timing = summarize(timings);
        cases.push({
            name: `cascade_${cascadeItems}_${useScratch ? 'scratch' : 'allocating'}`,
            cascadeItems, useScratch, iterations, feasible,
            timingMs: timing,
            medianCallsPerSecond: iterations / timing.median * 1000,
        });
    }
}

let commit = null;
try { commit = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(); } catch (_) { /* optional metadata */ }

process.stdout.write(JSON.stringify({
    benchmark: 'calculate_skillpoints',
    generatedAt: new Date().toISOString(),
    commit,
    runtime: { node: process.version, v8: process.versions.v8, platform: process.platform, arch: process.arch },
    machine: { cpu: os.cpus()[0]?.model ?? 'unknown', logicalCpus: os.cpus().length, memoryBytes: os.totalmem() },
    config: { samples, iterations, warmupIterations },
    cases,
}, null, 2) + '\n');
