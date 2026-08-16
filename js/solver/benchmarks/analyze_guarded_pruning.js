#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');

function median(values) {
    if (!values.length) return null;
    const sorted = [...values].sort((left, right) => left - right);
    const middle = Math.floor(sorted.length / 2);
    return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

function scenarioKey(result) {
    return `${result.profile_id}\0${result.variant}`;
}

function analyzeGuardedCampaign(artifact) {
    const results = artifact.results ?? [];
    const controls = new Map(results
        .filter(result => result.pruning_mode === 'off')
        .map(result => [scenarioKey(result), result]));
    const modes = [...new Set(results.map(result => result.pruning_mode))];

    const by_mode = modes.map(mode => {
        const group = results.filter(result => result.pruning_mode === mode);
        let observed_better = 0;
        let observed_equal = 0;
        let observed_worse = 0;
        let exact_preserved = 0;
        let exact_lost = 0;
        const score_ratios = [];
        const observed_worse_cases = [];

        for (const result of group) {
            const control = controls.get(scenarioKey(result));
            if (!control) continue;
            const tolerance = Math.max(1, Math.abs(control.best_score)) * 1e-12;
            const difference = result.best_score - control.best_score;
            if (difference > tolerance) observed_better++;
            else if (difference < -tolerance) {
                observed_worse++;
                observed_worse_cases.push({
                    profile_id: result.profile_id,
                    variant: result.variant,
                    score_ratio: result.best_score / control.best_score,
                    control_completed: control.completed,
                    candidate_completed: result.completed,
                });
            }
            else observed_equal++;
            if (control.best_score !== 0) score_ratios.push(result.best_score / control.best_score);
            if (control.completed) {
                if (Math.abs(difference) <= tolerance) exact_preserved++;
                else exact_lost++;
            }
        }

        const input = group.reduce((sum, result) => sum + result.input_combinations, 0);
        const search = group.reduce((sum, result) => sum + result.search_combinations, 0);
        return {
            mode,
            runs: group.length,
            completed: group.filter(result => result.completed).length,
            aggregate_combination_reduction: input ? 1 - search / input : 0,
            median_combination_reduction: median(group.map(result => result.combination_reduction)),
            median_elapsed_ms: median(group.map(result => result.elapsed_ms)),
            median_time_to_recovery_ms: median(group
                .map(result => result.time_to_recovery_ms)
                .filter(Number.isFinite)),
            median_checked_per_second: median(group.map(result => result.checked_per_second)),
            observed_vs_equal_cap_control: {
                better: observed_better,
                equal: observed_equal,
                worse: observed_worse,
                median_score_ratio: median(score_ratios),
                worse_cases: observed_worse_cases,
            },
            exhaustive_control_optimum: {
                preserved: exact_preserved,
                lost: exact_lost,
            },
        };
    });

    return {
        schema_version: 1,
        source_generated_at: artifact.generated_at,
        scenario_count: controls.size,
        exhaustive_unpruned_controls: [...controls.values()].filter(result => result.completed).length,
        by_mode,
    };
}

function main() {
    const inputArg = process.argv.find(argument => argument.startsWith('--input='));
    if (!inputArg) throw new Error('--input=<campaign-results.json> is required');
    const inputPath = path.resolve(process.cwd(), inputArg.slice('--input='.length));
    const artifact = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
    const analysis = analyzeGuardedCampaign(artifact);
    const outputArg = process.argv.find(argument => argument.startsWith('--output='));
    if (outputArg) {
        const outputPath = path.resolve(process.cwd(), outputArg.slice('--output='.length));
        fs.writeFileSync(outputPath, `${JSON.stringify(analysis, null, 2)}\n`, 'utf8');
    }
    console.log(JSON.stringify(analysis, null, 2));
}

if (require.main === module) main();

module.exports = { analyzeGuardedCampaign };
