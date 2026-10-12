#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const { REPO_ROOT } = require('../tests/harness');
const { scenarioName, validateProfiles } = require('./current_meta_lib');

const catalogPath = path.join(__dirname, 'current_meta_profiles.json');
const generatorPath = path.join(__dirname, 'generate_current_meta_snapshots.js');
const testPath = path.join(REPO_ROOT, 'js', 'solver', 'tests', 'test_solver_search.js');
const targetsPath = path.join(__dirname, 'current_meta_targets.json');
const catalog = JSON.parse(fs.readFileSync(catalogPath, 'utf8'));

function parseBest(output, scenario) {
    const scoreLine = output.match(/\[top15\]\s+([\d.e+\-]+)/i);
    if (!scoreLine) throw new Error(`${scenario}: no exact top score found in solver output\n${output}`);
    const itemLine = output.match(/best items:\s*(.+)$/m);
    const resolvedLine = output.match(/resolved combo rows:\s*(\[[^\r\n]*\])/m);
    return {
        score: Number(scoreLine[1]),
        item_names: itemLine ? itemLine[1].split(',').map(value => value.trim()) : [],
        resolved_combo_rows: resolvedLine ? JSON.parse(resolvedLine[1]) : [],
    };
}

function main() {
    validateProfiles(catalog);
    execFileSync(process.execPath, [generatorPath], { cwd: REPO_ROOT, stdio: 'inherit' });
    const result = {
        schema_version: 1,
        generated_at: new Date().toISOString(),
        benchmark_game_version: catalog.benchmark_game_version,
        calibration_contract: 'Exact known-good score produced by the JS production search path with every equipment slot and weapon locked, median roll groups, and the profile objective and playability constraints.',
        profiles: {},
    };
    const failures = [];
    for (const profile of catalog.profiles) {
        const scenario = scenarioName(profile.id, 'known_good');
        process.stdout.write(`Calibrating ${scenario} ... `);
        try {
            const output = execFileSync(process.execPath, [testPath, scenario], {
                cwd: REPO_ROOT,
                encoding: 'utf8',
                stdio: ['ignore', 'pipe', 'pipe'],
                env: {
                    ...process.env,
                    SOLVER_BENCH_SECONDS: '10',
                    SOLVER_BENCH_WORKERS: '1',
                    SOLVER_PRINT_TOP15: '1',
                },
            });
            result.profiles[profile.id] = {
                snapshot: scenario,
                ...parseBest(output, scenario),
                scoring_target: profile.objective.scoring_target,
            };
            console.log(result.profiles[profile.id].score);
        } catch (error) {
            failures.push(profile.id);
            console.log('FAILED');
            process.stderr.write(error.stdout || error.stderr || String(error));
        }
    }
    if (failures.length) {
        throw new Error(`calibration failed for: ${failures.join(', ')}`);
    }
    fs.writeFileSync(targetsPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    execFileSync(process.execPath, [generatorPath], { cwd: REPO_ROOT, stdio: 'inherit' });
    console.log(`Wrote ${path.relative(REPO_ROOT, targetsPath)}`);
}

main();
