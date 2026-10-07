#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const { annotatePruningResults, summarizePruningResults } = require('./pruning_metrics');
const { assessCampaignTiming } = require('./campaign_timing');

const BENCHMARK_DIR = __dirname;

function readArg(name, fallback = '') {
    const prefix = `--${name}=`;
    const argument = process.argv.find(value => value.startsWith(prefix));
    return argument ? argument.slice(prefix.length) : fallback;
}

function parseList(value) {
    return value.split(',').map(entry => entry.trim()).filter(Boolean);
}

function key(result) {
    return `${result.snapshot}\0${result.pruning_mode}`;
}

function groupedSummaries(results, keys) {
    const groups = new Map();
    for (const result of results) {
        const values = keys.map(groupKey => result[groupKey]);
        const groupKey = JSON.stringify(values);
        if (!groups.has(groupKey)) groups.set(groupKey, { values, results: [] });
        groups.get(groupKey).results.push(result);
    }
    return [...groups.values()].map(group => ({
        ...Object.fromEntries(keys.map((groupKey, index) => [groupKey, group.values[index]])),
        ...summarizePruningResults(group.results),
    }));
}

function load(name) {
    const fileName = path.basename(name);
    const artifact = JSON.parse(fs.readFileSync(path.join(BENCHMARK_DIR, fileName), 'utf8'));
    return {
        fileName,
        artifact,
        results: artifact.results.map(result => ({
            ...result,
            campaign_jobs: result.campaign_jobs ?? artifact.campaign.jobs,
        })),
    };
}

function sameConfiguration(base, replacement) {
    return base.campaign_seconds === replacement.campaign_seconds
        && base.campaign_workers === replacement.campaign_workers
        && JSON.stringify(base.campaign_checkpoints_seconds) === JSON.stringify(replacement.campaign_checkpoints_seconds);
}

function main() {
    const baseName = readArg('base');
    const supplementNames = parseList(readArg('supplements'));
    const outputName = path.basename(readArg('output', 'current_meta_merged_campaign_results.json'));
    if (!baseName || !supplementNames.length) {
        throw new Error('--base=<results.json> and --supplements=<one.json,two.json> are required');
    }
    if (!outputName.endsWith('_results.json')) throw new Error('output must end with _results.json');

    const base = load(baseName);
    const supplements = supplementNames.map(load);
    const merged = new Map(base.results.map(result => [key(result), result]));
    for (const supplement of supplements) {
        for (const replacement of supplement.results) {
            const existing = merged.get(key(replacement));
            if (!existing) throw new Error(`${supplement.fileName}: ${key(replacement)} is absent from base`);
            if (!sameConfiguration(existing, replacement)) {
                throw new Error(`${supplement.fileName}: ${key(replacement)} has incompatible cap, worker, or checkpoints`);
            }
            const timing = assessCampaignTiming(replacement, replacement.timing_grace_ms);
            if (!timing.valid) {
                throw new Error(`${supplement.fileName}: ${key(replacement)} timing invalid: ${timing.violations.join('; ')}`);
            }
            merged.set(key(replacement), {
                ...existing,
                ...replacement,
                campaign_order: existing.campaign_order,
                campaign_jobs: replacement.campaign_jobs ?? supplement.artifact.campaign.jobs,
                timing_valid: true,
                timing_grace_ms: timing.grace_ms,
                timing_violations: [],
            });
        }
    }

    const results = annotatePruningResults([...merged.values()])
        .map(result => {
            const timing = assessCampaignTiming(result, result.timing_grace_ms);
            return {
                ...result,
                timing_valid: timing.valid,
                timing_grace_ms: timing.grace_ms,
                timing_violations: timing.violations,
            };
        })
        .sort((left, right) => left.campaign_order - right.campaign_order);
    const timingFailures = results.filter(result => !assessCampaignTiming(result, result.timing_grace_ms).valid);
    if (timingFailures.length) {
        throw new Error(`${timingFailures.length} timing-invalid base result(s) remain after supplements`);
    }
    const scenarioCohorts = new Map();
    for (const result of results) {
        const scenarioKey = `${result.snapshot}`;
        if (!scenarioCohorts.has(scenarioKey)) scenarioCohorts.set(scenarioKey, new Set());
        scenarioCohorts.get(scenarioKey).add(`${result.campaign_jobs}/${result.campaign_workers}`);
    }
    const mixedScenarios = [...scenarioCohorts].filter(([, cohorts]) => cohorts.size !== 1);
    if (mixedScenarios.length) {
        throw new Error(`${mixedScenarios.length} scenario(s) mix execution cohorts across pruning modes`);
    }

    const executionCohorts = new Map();
    for (const result of results) {
        const cohortKey = `${result.campaign_jobs}/${result.campaign_workers}`;
        executionCohorts.set(cohortKey, (executionCohorts.get(cohortKey) || 0) + 1);
    }
    const artifact = {
        schema_version: 1,
        generated_at: new Date().toISOString(),
        manifest_generated_at: base.artifact.manifest_generated_at,
        campaign: {
            ...base.artifact.campaign,
            jobs: null,
            base_source: base.fileName,
            supplements: supplements.map(supplement => supplement.fileName),
            execution_cohorts: [...executionCohorts].map(([cohort, count]) => {
                const [jobs, workers] = cohort.split('/').map(Number);
                return { jobs, workers, results: count };
            }),
            execution_note: 'Each family/removal scenario uses one execution cohort across all pruning modes. Aggregate throughput mixes cohorts but preserves a balanced strategy comparison.',
        },
        summary: summarizePruningResults(results),
        by_variant: groupedSummaries(results, ['variant']),
        by_pruning_mode: groupedSummaries(results, ['pruning_mode']),
        variant_strategy_matrix: groupedSummaries(results, ['variant', 'pruning_mode']),
        family_strategy_matrix: groupedSummaries(
            results, ['build_family', 'class_name', 'profile_id', 'variant', 'pruning_mode']),
        results,
    };
    const outputPath = path.join(BENCHMARK_DIR, outputName);
    fs.writeFileSync(outputPath, `${JSON.stringify(artifact, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${outputPath}`);
}

main();
