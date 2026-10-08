#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const { REPO_ROOT } = require('../tests/harness');
const { expandPruningSelection, getPruningStrategy } = require('./pruning_strategies');
const { annotatePruningResults, summarizePruningResults } = require('./pruning_metrics');

const manifestPath = path.join(__dirname, 'current_meta_suite.json');
const testPath = path.join(REPO_ROOT, 'js', 'solver', 'tests', 'test_solver_search.js');

function readArg(name, fallback) {
    const prefix = `--${name}=`;
    const value = process.argv.find(argument => argument.startsWith(prefix));
    return value ? value.slice(prefix.length) : fallback;
}

function groupedSummaries(results, keys) {
    const groups = new Map();
    for (const result of results) {
        const values = keys.map(key => result[key]);
        const groupKey = JSON.stringify(values);
        if (!groups.has(groupKey)) groups.set(groupKey, { values, results: [] });
        groups.get(groupKey).results.push(result);
    }
    return [...groups.values()].map(group => ({
        ...Object.fromEntries(keys.map((key, index) => [key, group.values[index]])),
        ...summarizePruningResults(group.results),
    }));
}

function resultKey(result) {
    return `${result.snapshot}\0${result.pruning_mode}`;
}

function main() {
    const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
    const requested = new Set(readArg('variants', 'remove_2')
        .split(',').map(value => value.trim()).filter(Boolean));
    const requestedProfiles = new Set(readArg('profiles', '')
        .split(',').map(value => value.trim()).filter(Boolean));
    const requestedFamilies = new Set(readArg('families', '')
        .split(',').map(value => value.trim()).filter(Boolean));
    const seconds = Number(readArg('seconds', '30'));
    if (!Number.isFinite(seconds) || seconds <= 0 || seconds > 3600) {
        throw new Error('seconds must be greater than zero and no more than 3600');
    }
    const pruningArg = readArg('pruning', 'on');
    const pruningModes = expandPruningSelection(pruningArg === 'on' ? 'current' : pruningArg);
    const outputName = path.basename(readArg('output', 'current_meta_recovery_results.json'));
    if (!outputName.endsWith('_results.json')) throw new Error('output must end with _results.json');
    const outputPath = path.join(__dirname, outputName);
    const mergeExistingName = readArg('merge-existing', '');
    const mergeExistingPath = mergeExistingName
        ? path.join(__dirname, path.basename(mergeExistingName))
        : null;
    if (mergeExistingPath && !fs.existsSync(mergeExistingPath)) {
        throw new Error(`merge input not found: ${mergeExistingPath}`);
    }
    const profileBySnapshot = new Map(manifest.profiles.flatMap(profile =>
        profile.scenarios.map(scenario => [scenario.snapshot, profile.id])));
    const scenarioEntries = manifest.scenarios
        .filter(scenario => requested.has(scenario.variant)
            && (!requestedProfiles.size || requestedProfiles.has(profileBySnapshot.get(scenario.snapshot)))
            && (!requestedFamilies.size || requestedFamilies.has(scenario.build_family)));
    const scenarios = scenarioEntries.map(scenario => scenario.snapshot);
    if (!scenarios.length) throw new Error(`no scenarios matched ${[...requested].join(', ')}`);
    const scenarioBySnapshot = new Map(scenarioEntries.map(scenario => [scenario.snapshot, scenario]));

    const results = [];
    for (const pruningMode of pruningModes) {
        const pruningStrategy = getPruningStrategy(pruningMode);
        const output = execFileSync(process.execPath, [testPath, ...scenarios], {
            cwd: REPO_ROOT,
            encoding: 'utf8',
            maxBuffer: 32 * 1024 * 1024,
            env: {
                ...process.env,
                SOLVER_BENCH_SECONDS: String(seconds),
                SOLVER_ENFORCE_RECOVERY: '0',
                SOLVER_ITEM_PRUNING: pruningMode,
            },
        });
        const parsed = [...output.matchAll(/^\s*\[recovery\]\s+(\{.*\})$/gm)]
            .map(match => {
                const recovery = JSON.parse(match[1]);
                const scenario = scenarioBySnapshot.get(recovery.snapshot);
                return {
                    ...recovery,
                    profile_id: scenario.profile_id,
                    class_name: scenario.class_name,
                    archetype: scenario.archetype,
                    build_family: scenario.build_family,
                    variant: scenario.variant,
                    benchmark_tier: scenario.benchmark_tier,
                    score_semantics: scenario.score_semantics,
                    checkpoints_seconds: scenario.checkpoints_seconds,
                    pruning_mode: pruningMode,
                    pruning_enabled: pruningStrategy.enabled,
                    configured_sensitivity_ratio: pruningStrategy.sensitivity_ratio,
                };
            });
        if (parsed.length !== scenarios.length) {
            throw new Error(`expected ${scenarios.length} ${pruningMode} results, found ${parsed.length}`);
        }
        results.push(...parsed);
    }
    let annotatedResults = annotatePruningResults(results);
    if (mergeExistingPath) {
        const existing = JSON.parse(fs.readFileSync(mergeExistingPath, 'utf8'));
        const replacements = new Map(annotatedResults.map(result => [resultKey(result), result]));
        const merged = existing.results.map(result => {
            const key = resultKey(result);
            const replacement = replacements.get(key);
            if (replacement) replacements.delete(key);
            return replacement ?? result;
        });
        merged.push(...replacements.values());
        annotatedResults = annotatePruningResults(merged);
    }
    const resultModes = [...new Set(annotatedResults.map(entry => entry.pruning_mode))];
    const result = {
        schema_version: 2,
        generated_at: new Date().toISOString(),
        manifest_generated_at: manifest.generated_at,
        time_cap_seconds: seconds,
        variants: [...new Set(annotatedResults.map(entry => entry.variant))],
        profiles: [...new Set(annotatedResults.map(entry => entry.profile_id))],
        families: [...new Set(annotatedResults.map(entry => entry.build_family))],
        pruning_modes: resultModes,
        merged_from: mergeExistingPath ? path.basename(mergeExistingPath) : null,
        summary: summarizePruningResults(annotatedResults),
        by_pruning_mode: Object.fromEntries(resultModes.map(mode => {
            const modeResults = annotatedResults.filter(entry => entry.pruning_mode === mode);
            return [mode, summarizePruningResults(modeResults)];
        })),
        by_family: groupedSummaries(annotatedResults, ['build_family']),
        by_variant: groupedSummaries(annotatedResults, ['variant']),
        variant_strategy_matrix: groupedSummaries(annotatedResults, ['variant', 'pruning_mode']),
        family_strategy_matrix: groupedSummaries(
            annotatedResults, ['build_family', 'class_name', 'profile_id', 'variant', 'pruning_mode']),
        results: annotatedResults,
    };
    fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    process.stdout.write(`${JSON.stringify(result.summary)}\n`);
    console.log(`Wrote ${path.relative(REPO_ROOT, outputPath)}`);
}

main();
