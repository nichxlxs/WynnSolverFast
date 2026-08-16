#!/usr/bin/env node
'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile } = require('child_process');
const { REPO_ROOT } = require('../tests/harness');
const { annotatePruningResults, summarizePruningResults } = require('./pruning_metrics');
const { expandPruningSelection, getPruningStrategy } = require('./pruning_strategies');
const { DEFAULT_TIMING_GRACE_MS, assessCampaignTiming } = require('./campaign_timing');

const BENCHMARK_DIR = __dirname;
const MANIFEST_PATH = path.join(BENCHMARK_DIR, 'current_meta_suite.json');
const TEST_PATH = path.join(REPO_ROOT, 'js', 'solver', 'tests', 'test_solver_search.js');

function readArg(name, fallback = '') {
    const prefix = `--${name}=`;
    const argument = process.argv.find(value => value.startsWith(prefix));
    return argument ? argument.slice(prefix.length) : fallback;
}

function parseList(value) {
    return value.split(',').map(entry => entry.trim()).filter(Boolean);
}

function resultKey(result) {
    return [
        result.snapshot,
        result.pruning_mode,
        result.campaign_seconds,
        result.campaign_workers,
        (result.campaign_checkpoints_seconds || []).join(','),
    ].join('\0');
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

function batchTasks(tasks, batchSize) {
    const batches = [];
    const candidates = new Map();
    for (const task of tasks) {
        const key = [task.profile_id, task.pruning_mode, task.seconds, task.workers,
            task.checkpoints_seconds.join(',')].join('\0');
        if (!candidates.has(key)) candidates.set(key, []);
        candidates.get(key).push(task);
    }
    for (const group of candidates.values()) {
        for (let index = 0; index < group.length; index += batchSize) {
            batches.push(group.slice(index, index + batchSize));
        }
    }
    return batches;
}

function parseRecoveries(output, tasks) {
    const matches = [...output.matchAll(/^\s*\[recovery\]\s+(\{.*\})$/gm)];
    if (matches.length !== tasks.length) {
        throw new Error(`${tasks[0].profile_id}/${tasks[0].pruning_mode}: expected ${tasks.length} recovery results, found ${matches.length}`);
    }
    const bySnapshot = new Map(matches.map(match => {
        const result = JSON.parse(match[1]);
        return [result.snapshot, result];
    }));
    return tasks.map(task => {
        const result = bySnapshot.get(task.snapshot);
        if (!result) throw new Error(`${task.snapshot}/${task.pruning_mode}: recovery result missing from batch`);
        return result;
    });
}

function runTaskBatch(tasks) {
    return new Promise((resolve, reject) => {
        const first = tasks[0];
        const args = [TEST_PATH, ...tasks.map(task => task.snapshot)];
        execFile(process.execPath, args, {
            cwd: REPO_ROOT,
            encoding: 'utf8',
            maxBuffer: 64 * 1024 * 1024,
            env: {
                ...process.env,
                SOLVER_BENCH_SECONDS: String(first.seconds),
                SOLVER_BENCH_WORKERS: String(first.workers),
                SOLVER_BENCH_CHECKPOINTS: first.checkpoints_seconds.join(','),
                SOLVER_ENFORCE_RECOVERY: '0',
                SOLVER_ITEM_PRUNING: first.pruning_mode,
            },
        }, (error, stdout, stderr) => {
            if (error) {
                reject(new Error(`${first.profile_id}/${first.pruning_mode}: ${error.message}\n${stdout}\n${stderr}`));
                return;
            }
            try {
                resolve(parseRecoveries(stdout, tasks));
            } catch (parseError) {
                reject(parseError);
            }
        });
    });
}

function writeArtifact(outputPath, manifest, campaign, results) {
    const annotated = annotatePruningResults(results)
        .sort((left, right) => left.campaign_order - right.campaign_order);
    const artifact = {
        schema_version: 1,
        generated_at: new Date().toISOString(),
        manifest_generated_at: manifest.generated_at,
        campaign,
        summary: summarizePruningResults(annotated),
        by_variant: groupedSummaries(annotated, ['variant']),
        by_pruning_mode: groupedSummaries(annotated, ['pruning_mode']),
        variant_strategy_matrix: groupedSummaries(annotated, ['variant', 'pruning_mode']),
        family_strategy_matrix: groupedSummaries(
            annotated, ['build_family', 'class_name', 'profile_id', 'variant', 'pruning_mode']),
        results: annotated,
    };
    const temporaryPath = `${outputPath}.${process.pid}.tmp`;
    fs.writeFileSync(temporaryPath, `${JSON.stringify(artifact, null, 2)}\n`, 'utf8');
    try {
        fs.renameSync(temporaryPath, outputPath);
    } catch (error) {
        // Windows does not reliably replace an existing or briefly scanned
        // destination with renameSync. copyFileSync replaces it while retaining
        // the complete temporary artifact as the source of truth.
        if (!['EEXIST', 'EPERM', 'EACCES'].includes(error.code)) throw error;
        fs.copyFileSync(temporaryPath, outputPath);
        fs.unlinkSync(temporaryPath);
    }
    return artifact;
}

async function main() {
    const manifest = JSON.parse(fs.readFileSync(MANIFEST_PATH, 'utf8'));
    const tier = readArg('tier', 'exact');
    if (!['exact', 'anytime', 'all'].includes(tier)) throw new Error(`invalid tier ${tier}`);
    const requestedVariants = new Set(parseList(readArg('variants')));
    const requestedProfiles = new Set(parseList(readArg('profiles')));
    const requestedFamilies = new Set(parseList(readArg('families')));
    const pruningModes = expandPruningSelection(readArg('pruning', 'matrix'));
    const secondsOverrideRaw = readArg('seconds');
    const secondsOverride = secondsOverrideRaw ? Number(secondsOverrideRaw) : null;
    if (secondsOverride != null
        && (!Number.isFinite(secondsOverride) || secondsOverride <= 0 || secondsOverride > 3600)) {
        throw new Error('seconds must be greater than zero and no more than 3600');
    }
    const checkpointOverrideRaw = readArg('checkpoints');
    const checkpointOverride = checkpointOverrideRaw
        ? parseList(checkpointOverrideRaw).map(Number)
        : null;
    if (checkpointOverride?.some(value => !Number.isFinite(value) || value <= 0)) {
        throw new Error('checkpoints must contain positive seconds');
    }
    const jobs = Number(readArg('jobs', '1'));
    const workers = Number(readArg('workers', '2'));
    const batchSize = Number(readArg('batch-size', '1'));
    if (!Number.isInteger(jobs) || jobs < 1 || jobs > 16) throw new Error('jobs must be 1-16');
    if (!Number.isInteger(workers) || workers < 1 || workers > 16) throw new Error('workers must be 1-16');
    if (!Number.isInteger(batchSize) || batchSize < 1 || batchSize > 16) throw new Error('batch-size must be 1-16');
    const outputName = path.basename(readArg('output', `current_meta_${tier}_campaign_results.json`));
    if (!outputName.endsWith('_results.json')) throw new Error('output must end with _results.json');
    const outputPath = path.join(BENCHMARK_DIR, outputName);
    const resume = readArg('resume', '0') === '1';

    const scenarios = manifest.scenarios.filter(scenario => {
        if (requestedVariants.size && !requestedVariants.has(scenario.variant)) return false;
        if (!requestedVariants.size && tier !== 'all' && scenario.benchmark_tier !== tier) return false;
        if (!requestedVariants.size && tier === 'all' && scenario.benchmark_tier === 'anchor') return false;
        if (requestedProfiles.size && !requestedProfiles.has(scenario.profile_id)) return false;
        if (requestedFamilies.size && !requestedFamilies.has(scenario.build_family)) return false;
        return true;
    });
    if (!scenarios.length) throw new Error('no scenarios matched the campaign selection');

    const tasks = [];
    let campaignOrder = 0;
    for (const scenario of scenarios) {
        for (const pruningMode of pruningModes) {
            const seconds = secondsOverride ?? scenario.time_limit_seconds;
            const checkpoints = (checkpointOverride ?? scenario.checkpoints_seconds ?? [])
                .filter(checkpoint => checkpoint <= seconds);
            tasks.push({
                ...scenario,
                pruning_mode: pruningMode,
                seconds,
                workers,
                checkpoints_seconds: checkpoints,
                campaign_order: campaignOrder++,
            });
        }
    }

    let results = [];
    if (resume && fs.existsSync(outputPath)) {
        results = JSON.parse(fs.readFileSync(outputPath, 'utf8')).results || [];
    }
    const expectedKeys = new Set(tasks.map(task => resultKey({
        snapshot: task.snapshot,
        pruning_mode: task.pruning_mode,
        campaign_seconds: task.seconds,
        campaign_workers: task.workers,
        campaign_checkpoints_seconds: task.checkpoints_seconds,
    })));
    const incompatibleResults = results.filter(result => !expectedKeys.has(resultKey(result)));
    if (incompatibleResults.length) {
        throw new Error(`cannot resume ${outputName}: ${incompatibleResults.length} result(s) use a different selection, cap, worker count, or checkpoint configuration`);
    }
    const completedKeys = new Set(results.map(resultKey));
    const pending = tasks.filter(task => !completedKeys.has(resultKey({
        snapshot: task.snapshot,
        pruning_mode: task.pruning_mode,
        campaign_seconds: task.seconds,
        campaign_workers: task.workers,
        campaign_checkpoints_seconds: task.checkpoints_seconds,
    })));
    const pendingGroups = batchTasks(pending, batchSize);
    const allGroups = batchTasks(tasks, batchSize);
    const campaign = {
        tier,
        variants: [...new Set(scenarios.map(scenario => scenario.variant))],
        profiles: [...new Set(scenarios.map(scenario => scenario.profile_id))],
        families: [...new Set(scenarios.map(scenario => scenario.build_family))],
        pruning_modes: pruningModes,
        seconds_override: secondsOverride,
        checkpoints_override: checkpointOverride,
        jobs,
        workers_per_search: workers,
        batch_size: batchSize,
        batch_count: allGroups.length,
        pending_batch_count: pendingGroups.length,
        logical_cpus: os.cpus().length,
        task_count: tasks.length,
        execution_note: 'Concurrent jobs share the host. Compare throughput only within runs using the same jobs and workers settings.',
    };

    if (!pending.length) {
        const artifact = writeArtifact(outputPath, manifest, campaign, results);
        console.log(JSON.stringify(artifact.summary));
        console.log(`No pending tasks; ${outputName} is complete for this configuration.`);
        return;
    }

    console.log(`Campaign: ${pending.length}/${tasks.length} pending in ${pendingGroups.length} batch(es), ${jobs} jobs, ${workers} worker(s) per search.`);
    let nextIndex = 0;
    let finished = 0;
    async function campaignWorker() {
        while (true) {
            const index = nextIndex++;
            if (index >= pendingGroups.length) return;
            const taskBatch = pendingGroups[index];
            const recoveries = await runTaskBatch(taskBatch);
            for (let taskIndex = 0; taskIndex < taskBatch.length; taskIndex++) {
                const task = taskBatch[taskIndex];
                const strategy = getPruningStrategy(task.pruning_mode);
                const timing = assessCampaignTiming({
                    ...recoveries[taskIndex],
                    campaign_seconds: task.seconds,
                }, DEFAULT_TIMING_GRACE_MS);
                results.push({
                    ...recoveries[taskIndex],
                    profile_id: task.profile_id,
                    class_name: task.class_name,
                    archetype: task.archetype,
                    build_family: task.build_family,
                    variant: task.variant,
                    benchmark_tier: task.benchmark_tier,
                    score_semantics: task.score_semantics,
                    pruning_mode: task.pruning_mode,
                    pruning_enabled: strategy.enabled,
                    configured_sensitivity_ratio: strategy.sensitivity_ratio,
                    campaign_seconds: task.seconds,
                    campaign_workers: task.workers,
                    campaign_jobs: jobs,
                    campaign_checkpoints_seconds: task.checkpoints_seconds,
                    campaign_order: task.campaign_order,
                    timing_valid: timing.valid,
                    timing_grace_ms: timing.grace_ms,
                    timing_violations: timing.violations,
                });
            }
            const artifact = writeArtifact(outputPath, manifest, campaign, results);
            for (const task of taskBatch) {
                finished++;
                const measured = artifact.results.find(result =>
                    result.snapshot === task.snapshot && result.pruning_mode === task.pruning_mode
                    && result.campaign_order === task.campaign_order);
                console.log(`[${finished}/${pending.length}] ${task.profile_id}/${task.variant}/${task.pruning_mode}: `
                    + `${measured.combination_reduction.toFixed(3)} reduced, `
                    + `${Math.round(measured.checked_per_second).toLocaleString('en-US')} checked/s, `
                    + `${measured.completed ? 'exhaustive' : 'censored'}`);
            }
        }
    }

    await Promise.all(Array.from({ length: Math.min(jobs, pendingGroups.length) }, campaignWorker));
    const artifact = writeArtifact(outputPath, manifest, campaign, results);
    console.log(JSON.stringify(artifact.summary));
    console.log(`Wrote ${path.relative(REPO_ROOT, outputPath)}`);
}

main().catch(error => {
    console.error(error.stack || error.message);
    process.exit(1);
});
