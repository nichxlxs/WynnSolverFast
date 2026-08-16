#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const { REPO_ROOT } = require('../tests/harness');
const { analyzeAnytimeCampaign } = require('./campaign_metrics');
const { annotatePruningResults, summarizePruningResults } = require('./pruning_metrics');
const { assessCampaignTiming } = require('./campaign_timing');

const BENCHMARK_DIR = __dirname;
const MODE_ORDER = ['off', 'certified', 'balanced', 'conservative', 'current', 'aggressive'];

function readArg(name, fallback = '') {
    const prefix = `--${name}=`;
    const argument = process.argv.find(value => value.startsWith(prefix));
    return argument ? argument.slice(prefix.length) : fallback;
}

function number(value, digits = 0) {
    return Number.isFinite(value)
        ? value.toLocaleString('en-US', { maximumFractionDigits: digits })
        : 'n/a';
}

function percent(value, digits = 1) {
    return Number.isFinite(value) ? `${(value * 100).toFixed(digits)}%` : 'n/a';
}

function score(value) {
    return number(value, 2);
}

function markdownReport(artifact, analysis) {
    const results = annotatePruningResults(artifact.results);
    const lines = [];
    lines.push('# Current-meta pruning campaign');
    lines.push('');
    lines.push(`Campaign tier: ${artifact.campaign.tier}. Searches: ${results.length}/${artifact.campaign.task_count}.`);
    if (artifact.campaign.execution_cohorts?.length) {
        lines.push(`Execution cohorts: ${artifact.campaign.execution_cohorts.map(cohort => `${cohort.results} searches at ${cohort.jobs} job(s) and ${cohort.workers} worker(s)`).join('; ')}.`);
    } else {
        lines.push(`Execution: ${artifact.campaign.jobs} concurrent job(s), ${artifact.campaign.workers_per_search} worker(s) per search.`);
    }
    lines.push(`Batch size: ${artifact.campaign.batch_size || 1} search(es) per loaded harness.`);
    if (artifact.campaign.execution_cohorts?.length) {
        lines.push('Every family/removal scenario keeps all pruning modes in one execution cohort, so its throughput ranks are valid. Aggregate throughput mixes balanced cohorts and is comparative, not a single-load absolute baseline.');
    } else {
        lines.push('Throughput comparisons are valid only against runs with the same concurrency and worker configuration.');
    }
    lines.push('');

    const exactResults = results.filter(result => result.benchmark_tier === 'exact');
    if (exactResults.length) {
        lines.push('## Exact-tier completion');
        lines.push('');
        lines.push('| Missing | Strategy | Runs | Exhaustive | Removed | Reduction | Checked/s | Exact optima preserved | Exact optima pruned |');
        lines.push('|---:|---|---:|---:|---:|---:|---:|---:|---:|');
        const variants = [...new Set(exactResults.map(result => result.variant))];
        for (const variant of variants) {
            for (const mode of MODE_ORDER) {
                const group = exactResults.filter(result => result.variant === variant && result.pruning_mode === mode);
                if (!group.length) continue;
                const summary = summarizePruningResults(group);
                lines.push(`| ${variant.slice('remove_'.length)} | ${mode} | ${group.length} | ${summary.completed_count} | ${number(summary.total_combinations_removed)} | ${percent(summary.aggregate_combination_reduction)} | ${number(summary.aggregate_checked_per_second)} | ${summary.optimum_preserved_count} | ${summary.optimum_pruned_count} |`);
            }
        }
        lines.push('');
        const controls = exactResults.filter(result =>
            result.pruning_mode === 'off' && result.maximum_score_scope === 'full_input_space');
        lines.push(`Exhaustive unpruned controls available: ${controls.length}. Only these scenarios support global-maximum comparisons.`);
        lines.push('');

        const losses = exactResults.filter(result => result.global_optimum_preserved === false);
        if (losses.length) {
            lines.push('### Confirmed optimum losses');
            lines.push('');
            lines.push('| Class | Family | Missing | Strategy | Reduction | Full-space maximum | Pruned maximum | Score loss |');
            lines.push('|---|---|---:|---|---:|---:|---:|---:|');
            for (const result of losses) {
                const scoreLoss = result.full_space_max_score - result.search_space_max_score;
                lines.push(`| ${result.class_name} | ${result.build_family} | ${result.variant.slice('remove_'.length)} | ${result.pruning_mode} | ${percent(result.combination_reduction)} | ${score(result.full_space_max_score)} | ${score(result.search_space_max_score)} | ${score(scoreLoss)} |`);
            }
            lines.push('');
        }

        const incompleteControls = exactResults.filter(result =>
            result.pruning_mode === 'off' && result.maximum_score_scope !== 'full_input_space');
        if (incompleteControls.length) {
            lines.push('### Censored unpruned controls');
            lines.push('');
            lines.push('These families did not exhaust within the campaign cap, so their pruning safety remains unknown.');
            lines.push('');
            lines.push('| Class | Family | Missing | Checked | Checked/s | Observed score |');
            lines.push('|---|---|---:|---:|---:|---:|');
            for (const result of incompleteControls) {
                lines.push(`| ${result.class_name} | ${result.build_family} | ${result.variant.slice('remove_'.length)} | ${number(result.checked)} | ${number(result.checked_per_second)} | ${score(result.best_score)} |`);
            }
            lines.push('');
        }
    }

    if (analysis) {
        lines.push(`## Anytime ranking at ${analysis.checkpoint_seconds} seconds`);
        lines.push('');
        lines.push('Score ratio is normalized within each family/removal scenario against the best score observed across strategies at the same checkpoint. It is not a global-optimality claim.');
        lines.push('');
        lines.push('| Missing | Strategy | Families | Total combinations removed | Aggregate reduction | Median checked/s | Median score ratio | Worst score ratio | Seeds recovered | Reduction wins | Throughput wins | Score wins | Pareto appearances |');
        lines.push('|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
        const variants = [...new Set(analysis.summaries.map(summary => summary.variant))];
        for (const variant of variants) {
            for (const mode of MODE_ORDER) {
                const summary = analysis.summaries.find(entry =>
                    entry.variant === variant && entry.pruning_mode === mode);
                if (!summary) continue;
                lines.push(`| ${variant.slice('remove_'.length)} | ${mode} | ${summary.scenario_count} | ${number(summary.total_combinations_removed)} | ${percent(summary.aggregate_combination_reduction)} | ${number(summary.median_checked_per_second)} | ${percent(summary.median_score_ratio, 2)} | ${percent(summary.worst_score_ratio, 2)} | ${summary.target_recovered_count}/${summary.scenario_count} | ${summary.reduction_wins} | ${summary.throughput_wins} | ${summary.score_wins} | ${summary.pareto_count} |`);
            }
        }
        lines.push('');
        lines.push('## Per-family anytime ranks');
        lines.push('');
        lines.push('| Class | Family | Missing | Strategy | Combinations removed | Reduction | Checked/s | Observed score | Best at | Reduction rank | Throughput rank | Score rank | Pareto |');
        lines.push('|---|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---|');
        for (const row of analysis.rows) {
            lines.push(`| ${row.class_name} | ${row.build_family} | ${row.variant.slice('remove_'.length)} | ${row.pruning_mode} | ${number(row.combinations_removed)} | ${percent(row.combination_reduction)} | ${number(row.checkpoint_checked_per_second)} | ${score(row.checkpoint_observed_score)} | ${number(row.checkpoint_time_to_best_seconds, 1)}s | ${row.reduction_rank} | ${row.throughput_rank} | ${row.score_rank} | ${row.pareto ? 'yes' : 'no'} |`);
        }
        lines.push('');
    }
    return `${lines.join('\n')}\n`;
}

function main() {
    const inputName = path.basename(readArg('input'));
    if (!inputName) throw new Error('--input=<campaign-results.json> is required');
    const inputPath = path.join(BENCHMARK_DIR, inputName);
    const artifact = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
    const timingFailures = artifact.results.map(result => ({
        result,
        timing: assessCampaignTiming(result, result.timing_grace_ms),
    })).filter(entry => !entry.timing.valid);
    if (timingFailures.length) {
        const examples = timingFailures.slice(0, 4)
            .map(entry => `${entry.result.snapshot}/${entry.result.pruning_mode}: ${entry.timing.violations.join('; ')}`)
            .join('\n');
        throw new Error(`${timingFailures.length} result(s) violate checkpoint timing integrity:\n${examples}`);
    }
    const checkpoints = artifact.results.flatMap(result =>
        (result.checkpoints || []).map(checkpoint => checkpoint.checkpoint_ms / 1000));
    const checkpoint = Number(readArg('checkpoint', checkpoints.length ? String(Math.max(...checkpoints)) : '0'));
    const anytimeResults = artifact.results.filter(result => result.benchmark_tier === 'anytime');
    const analysis = anytimeResults.length && checkpoint > 0
        ? analyzeAnytimeCampaign(anytimeResults, checkpoint)
        : null;
    const analysisPath = path.join(BENCHMARK_DIR,
        path.basename(readArg('analysis-output', inputName.replace('_results.json', '_analysis.json'))));
    const reportPath = path.join(BENCHMARK_DIR,
        path.basename(readArg('output', inputName.replace('_results.json', '_report.md'))));
    const analysisArtifact = {
        schema_version: 1,
        generated_at: new Date().toISOString(),
        source: inputName,
        campaign: artifact.campaign,
        exact_summary: summarizePruningResults(
            annotatePruningResults(artifact.results.filter(result => result.benchmark_tier === 'exact'))),
        anytime: analysis,
    };
    fs.writeFileSync(analysisPath, `${JSON.stringify(analysisArtifact, null, 2)}\n`, 'utf8');
    fs.writeFileSync(reportPath, markdownReport(artifact, analysis), 'utf8');
    console.log(`Wrote ${path.relative(REPO_ROOT, analysisPath)}`);
    console.log(`Wrote ${path.relative(REPO_ROOT, reportPath)}`);
}

if (require.main === module) main();

module.exports = { markdownReport };
