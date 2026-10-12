#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const { annotatePruningResults, summarizePruningResults } = require('./pruning_metrics');

const BENCHMARK_DIR = __dirname;
const DEFAULT_INPUT = path.join(BENCHMARK_DIR, 'current_meta_family_pruning_results.json');
const DEFAULT_OUTPUT = path.join(BENCHMARK_DIR, 'current_meta_family_pruning_report.md');
const DEFAULT_METRICS_OUTPUT = path.join(BENCHMARK_DIR, 'current_meta_family_pruning_metrics.json');
const PROFILE_PATH = path.join(BENCHMARK_DIR, 'current_meta_profiles.json');
const VARIANT_ORDER = ['remove_3', 'remove_4', 'remove_6'];
const MODE_ORDER = ['off', 'conservative', 'current', 'aggressive'];

function readArg(name, fallback) {
    const prefix = `--${name}=`;
    const argument = process.argv.find(value => value.startsWith(prefix));
    return argument ? argument.slice(prefix.length) : fallback;
}

function percent(value, digits = 1) {
    return value == null ? 'n/a' : `${(value * 100).toFixed(digits)}%`;
}

function milliseconds(value) {
    return Number.isFinite(value) ? `${value.toLocaleString('en-US')} ms` : 'not reached';
}

function seconds(value) {
    return Number.isFinite(value) ? `${(value / 1000).toFixed(3)} s` : 'n/a';
}

function number(value) {
    return Number.isFinite(value) ? value.toLocaleString('en-US', { maximumFractionDigits: 3 }) : 'n/a';
}

function score(value) {
    return Number.isFinite(value) ? value.toLocaleString('en-US', { maximumFractionDigits: 1 }) : 'n/a';
}

function restrictionLabel(restriction) {
    return `${restriction.stat} ${restriction.op === 'ge' ? '>=' : '<='} ${number(restriction.value)}`;
}

function contractLabel(profile) {
    const restrictions = (profile.conditions?.restrictions || []).map(restrictionLabel);
    if (profile.conditions?.mana_disabled) restrictions.push('mana disabled');
    else restrictions.push(profile.conditions?.allow_downtime ? 'mana with downtime allowed' : 'mana sustained');
    return restrictions.join('; ');
}

function summarize(results) {
    const base = summarizePruningResults(results);
    return {
        ...base,
        count: base.scenario_count,
        recovered: base.recovered_count,
        completed: base.completed_count,
        median_recovery_ms: base.median_time_to_recovery_ms,
        median_reduction: base.median_combination_reduction,
    };
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

function buildMetricsArtifact(run) {
    const results = annotatePruningResults(run.results);
    const modes = run.pruning_modes || [...new Set(results.map(result => result.pruning_mode))];
    return {
        ...run,
        schema_version: 2,
        metric_contract: {
            combinations_removed: 'input_combinations - search_combinations',
            checked_per_second: 'checked / elapsed_seconds',
            maximum_score: 'reported only when the relevant search space completed',
            timed_out_score: 'censored observation, not an optimum',
            global_optimum: 'requires an exhaustive unpruned control for the same profile and variant',
        },
        summary: summarizePruningResults(results),
        by_pruning_mode: Object.fromEntries(modes.map(mode => [
            mode,
            summarizePruningResults(results.filter(result => result.pruning_mode === mode)),
        ])),
        by_family: groupedSummaries(results, ['build_family']),
        by_variant: groupedSummaries(results, ['variant']),
        variant_strategy_matrix: groupedSummaries(results, ['variant', 'pruning_mode']),
        family_strategy_matrix: groupedSummaries(
            results, ['build_family', 'class_name', 'profile_id', 'variant', 'pruning_mode']),
        results,
    };
}

function buildReport(run, profiles) {
    run = { ...run, results: annotatePruningResults(run.results) };
    const lines = [];
    const profileById = new Map(profiles.map(profile => [profile.id, profile]));
    const includedProfiles = new Set(run.results.map(result => result.profile_id));
    const profileIds = profiles.map(profile => profile.id)
        .filter(profileId => includedProfiles.has(profileId));

    lines.push('# Current-meta family pruning benchmark');
    lines.push('');
    lines.push(`Generated from ${run.results.length} searches at a ${run.time_cap_seconds}-second cap per search.`);
    lines.push('A score is reported as a maximum only when that search space was exhausted.');
    lines.push('Scores from timed-out searches are censored observations and are not used as optimality evidence.');
    lines.push('');
    lines.push('## Pruning and throughput summary');
    lines.push('');
    lines.push('| Strategy | Runs | Exhaustive | Total input combinations | Total combinations removed | Aggregate reduction | Total checked | Aggregate checked/s | Median checked/s |');
    lines.push('|---|---:|---:|---:|---:|---:|---:|---:|---:|');
    for (const mode of MODE_ORDER) {
        const subset = run.results.filter(result => result.pruning_mode === mode);
        const summary = summarize(subset);
        lines.push(`| ${mode} | ${summary.count} | ${summary.completed}/${summary.count} | ${number(summary.total_input_combinations)} | ${number(summary.total_combinations_removed)} | ${percent(summary.aggregate_combination_reduction)} | ${number(summary.total_checked)} | ${number(Math.round(summary.aggregate_checked_per_second))} | ${number(Math.round(summary.median_checked_per_second))} |`);
    }
    lines.push('');
    lines.push('## Missing-slot depth by strategy');
    lines.push('');
    lines.push('| Missing | Strategy | Exhaustive | Total input | Total removed | Aggregate reduction | Median reduction | Total checked | Checked/s |');
    lines.push('|---:|---|---:|---:|---:|---:|---:|---:|---:|');
    for (const variant of VARIANT_ORDER) {
        for (const mode of MODE_ORDER) {
            const subset = run.results.filter(result => result.variant === variant && result.pruning_mode === mode);
            const summary = summarize(subset);
            lines.push(`| ${variant.slice('remove_'.length)} | ${mode} | ${summary.completed}/${summary.count} | ${number(summary.total_input_combinations)} | ${number(summary.total_combinations_removed)} | ${percent(summary.aggregate_combination_reduction)} | ${percent(summary.median_combination_reduction)} | ${number(summary.total_checked)} | ${number(Math.round(summary.aggregate_checked_per_second))} |`);
        }
    }
    lines.push('');
    lines.push('## Exhaustive maximum-score checks');
    lines.push('');
    const exhaustiveControls = run.results.filter(result =>
        result.pruning_mode === 'off' && result.maximum_score_scope === 'full_input_space');
    lines.push(`Only ${exhaustiveControls.length} of ${run.results.filter(result => result.pruning_mode === 'off').length} unpruned searches exhausted the full input space. Other score observations are not global maxima.`);
    lines.push('');
    lines.push('| Class | Build family | Missing | Full combinations | Full-space maximum | Strategy | Search combinations | Removed | Exhaustive | Search-space maximum | Global optimum result |');
    lines.push('|---|---|---:|---:|---:|---|---:|---:|---:|---:|---|');
    for (const control of exhaustiveControls) {
        const profile = profileById.get(control.profile_id);
        const comparisons = run.results.filter(result =>
            result.profile_id === control.profile_id && result.variant === control.variant);
        for (const result of comparisons) {
            lines.push(`| ${profile.class_name} | ${profile.build_family} | ${control.variant.slice('remove_'.length)} | ${number(control.input_combinations)} | ${score(control.full_space_max_score)} | ${result.pruning_mode} | ${number(result.search_combinations)} | ${number(result.combinations_removed)} | ${result.completed ? 'yes' : 'no'} | ${score(result.search_space_max_score)} | ${result.optimality_status} |`);
        }
    }
    lines.push('');
    const prunedOptima = run.results.filter(result => result.global_optimum_preserved === false);
    lines.push(`Confirmed global-optimum pruning events: ${prunedOptima.length}.`);
    for (const result of prunedOptima) {
        lines.push(`- ${result.profile_id}/${result.variant}/${result.pruning_mode}: full maximum ${score(result.full_space_max_score)}, pruned-space maximum ${score(result.search_space_max_score)}.`);
    }
    if (!prunedOptima.length) lines.push('- None in the currently exhaustive control set.');
    lines.push('');
    lines.push('## Build-family contracts');
    lines.push('');
    lines.push('| Class | Profile | Build family | Optimization objective | Requirements | Seed score |');
    lines.push('|---|---|---|---|---|---:|');
    for (const profileId of profileIds) {
        const profile = profileById.get(profileId);
        if (!profile) continue;
        const target = run.results.find(result => result.profile_id === profileId)?.target_score;
        lines.push(`| ${profile.class_name} | ${profile.id} | ${profile.build_family} | ${profile.objective.label} (${profile.objective.scoring_target}) | ${contractLabel(profile)} | ${score(target)} |`);
    }
    lines.push('');
    lines.push('## Current-pruning measurements by family');
    lines.push('');
    lines.push('| Class | Build family | Missing | Input combinations | Combinations removed | Reduction | Checked | Checked/s | Exhaustive | Maximum score | Maximum scope |');
    lines.push('|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|');
    for (const profileId of profileIds) {
        const profile = profileById.get(profileId);
        if (!profile) continue;
        for (const variant of VARIANT_ORDER) {
            const subset = run.results.filter(result => result.profile_id === profileId && result.variant === variant);
            if (!subset.length) continue;
            const current = subset.find(result => result.pruning_mode === 'current');
            lines.push(`| ${profile.class_name} | ${profile.build_family} | ${variant.slice('remove_'.length)} | ${number(current.input_combinations)} | ${number(current.combinations_removed)} | ${percent(current.combination_reduction)} | ${number(current.checked)} | ${number(Math.round(current.checked_per_second))} | ${current.completed ? 'yes' : 'no'} | ${score(current.search_space_max_score)} | ${current.maximum_score_scope} |`);
        }
    }
    lines.push('');
    lines.push('The complete per-run record remains in `current_meta_family_pruning_results.json`. Timed-out `best_score` values are retained as diagnostic observations only.');
    lines.push('');
    return `${lines.join('\n')}\n`;
}

function main() {
    const inputPath = path.resolve(BENCHMARK_DIR, readArg('input', path.basename(DEFAULT_INPUT)));
    const outputPath = path.resolve(BENCHMARK_DIR, readArg('output', path.basename(DEFAULT_OUTPUT)));
    const metricsOutputPath = path.resolve(
        BENCHMARK_DIR, readArg('metrics-output', path.basename(DEFAULT_METRICS_OUTPUT)));
    const run = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
    const profileDocument = JSON.parse(fs.readFileSync(PROFILE_PATH, 'utf8'));
    const metrics = buildMetricsArtifact(run);
    const markdown = buildReport(metrics, profileDocument.profiles);
    fs.writeFileSync(metricsOutputPath, `${JSON.stringify(metrics, null, 2)}\n`, 'utf8');
    fs.writeFileSync(outputPath, markdown, 'utf8');
    console.log(`Wrote ${path.relative(process.cwd(), metricsOutputPath)}`);
    console.log(`Wrote ${path.relative(process.cwd(), outputPath)}`);
}

if (require.main === module) main();

module.exports = { buildMetricsArtifact, buildReport, summarize };
