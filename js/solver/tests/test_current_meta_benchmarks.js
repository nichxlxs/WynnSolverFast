#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const {
    EXPECTED_ARCHETYPES,
    REMOVAL_VARIANTS,
    deterministicRemoval,
    validateProfiles,
} = require('../benchmarks/current_meta_lib');
const {
    PRUNING_STRATEGIES,
    expandPruningSelection,
    getPruningStrategy,
} = require('../benchmarks/pruning_strategies');
const {
    annotatePruningResults,
    summarizePruningResults,
} = require('../benchmarks/pruning_metrics');
const { analyzeAnytimeCampaign } = require('../benchmarks/campaign_metrics');
const { assessCampaignTiming } = require('../benchmarks/campaign_timing');
const catalog = require('../benchmarks/current_meta_profiles.json');
const targets = require('../benchmarks/current_meta_targets.json');
const manifest = require('../benchmarks/current_meta_suite.json');
const { TestRunner, REPO_ROOT } = require('./harness');

const t = new TestRunner('Current Meta Benchmarks');

t.assert(validateProfiles(catalog), 'family/class catalog retains complete source archetype coverage');
t.assert(catalog.profiles.length === 15, 'catalog has 15 family/class profiles');
t.assert(Object.keys(EXPECTED_ARCHETYPES).length === 5, 'family/class catalog spans five classes');
t.assert(manifest.profiles.length === 15, 'manifest has 15 profiles');
t.assert(manifest.primary_grouping === 'build_family',
    'family is the primary benchmark grouping; archetype remains metadata');
t.assert(manifest.scenarios.length === 15 * REMOVAL_VARIANTS.length,
    'manifest has every deterministic recovery variant per profile');
t.assert(Object.keys(targets.profiles).length === 15, 'all profile targets are calibrated');
t.assert(Object.keys(PRUNING_STRATEGIES).length === 6, 'six pruning strategies are defined');
t.assert(JSON.stringify(expandPruningSelection('matrix'))
    === JSON.stringify(['off', 'certified', 'balanced', 'conservative', 'current', 'aggressive']),
    'matrix selection expands to all pruning strategies');
t.assert(JSON.stringify(expandPruningSelection('paired'))
    === JSON.stringify(['off', 'balanced']),
    'paired selection compares the full pool with the production balanced reducer');
t.assert(getPruningStrategy('certified').exact === true,
    'certified reduction is explicitly exact');
t.assert(getPruningStrategy('balanced').guard === 'structural',
    'balanced reduction retains uncertain active dimensions');
t.assert(getPruningStrategy('current').sensitivity_ratio === 0.005,
    'current strategy remains available as the legacy 0.5% sensitivity baseline');

const metricFixture = annotatePruningResults([
    {
        snapshot: 'metric_fixture', profile_id: 'fixture', variant: 'remove_3',
        pruning_mode: 'off', timed_out: false, best_score: 10,
        input_combinations: 100, search_combinations: 100, checked: 100, elapsed_ms: 1000,
    },
    {
        snapshot: 'metric_fixture', profile_id: 'fixture', variant: 'remove_3',
        pruning_mode: 'current', timed_out: false, best_score: 9,
        input_combinations: 100, search_combinations: 40, checked: 40, elapsed_ms: 500,
    },
    {
        snapshot: 'metric_fixture', profile_id: 'fixture', variant: 'remove_3',
        pruning_mode: 'aggressive', timed_out: true, best_score: 8,
        input_combinations: 100, search_combinations: 30, checked: 15, elapsed_ms: 500,
    },
]);
const metricSummary = summarizePruningResults(metricFixture);
t.assert(metricFixture[1].combinations_removed === 60,
    'pruning metrics report the absolute number of combinations removed');
t.assert(metricFixture[1].checked_per_second === 80,
    'pruning metrics report checked builds per second');
t.assert(metricFixture[1].search_space_max_score === 9,
    'completed pruned search reports its exact pruned-space maximum');
t.assert(metricFixture[1].global_optimum_preserved === false,
    'completed pruned search detects loss against the exhaustive control');
t.assert(metricFixture[2].search_space_max_score === null,
    'timed-out scores are censored rather than labelled maxima');
t.assert(metricFixture[2].global_optimum_preserved === null,
    'timed-out pruned search cannot claim optimum preservation or loss');
t.assert(metricSummary.total_combinations_removed === 130,
    'summary reports total combinations removed across runs');

const anytimeFixture = analyzeAnytimeCampaign(metricFixture.map((result, index) => ({
    ...result,
    class_name: 'mage',
    build_family: 'fixture',
    checkpoints: [{
        checkpoint_ms: 1000,
        checked: result.checked,
        checked_per_second: result.checked,
        best_score: 10 - index,
    }],
})), 1);
t.assert(anytimeFixture.rows.find(row => row.pruning_mode === 'off').score_rank === 1,
    'anytime campaign ranks checkpoint score within the same family scenario');
t.assert(anytimeFixture.rows.find(row => row.pruning_mode === 'aggressive').reduction_rank === 1,
    'anytime campaign ranks combination reduction independently');
t.assert(anytimeFixture.summaries.length === 3,
    'anytime campaign exposes a separate summary for each pruning strategy');
t.assert(anytimeFixture.summaries.every(summary => Number.isFinite(summary.total_combinations_removed)),
    'anytime campaign reports absolute combinations removed');
t.assert(assessCampaignTiming({
    campaign_seconds: 180,
    elapsed_ms: 180100,
    checkpoints: [{ checkpoint_ms: 60000, observed_at_ms: 60100 }],
}).valid, 'campaign timing accepts small scheduler drift');
t.assert(!assessCampaignTiming({
    campaign_seconds: 180,
    elapsed_ms: 240000,
    checkpoints: [{ checkpoint_ms: 60000, observed_at_ms: 60000 }],
}).valid, 'campaign timing rejects a starved timeout');

const expectedFamilies = new Set(catalog.profiles.map(profile => profile.build_family));
t.assert(manifest.family_groups.length === expectedFamilies.size,
    'manifest groups profiles by build family');
t.assert(new Set(manifest.family_groups.flatMap(group => group.classes)).size === 5,
    'family groups collectively span all five classes');

for (const profile of catalog.profiles) {
    const manifestProfile = manifest.profiles.find(entry => entry.id === profile.id);
    t.assert(!!manifestProfile, `${profile.id}: present in generated manifest`);
    t.assert(typeof targets.profiles[profile.id]?.score === 'number',
        `${profile.id}: has numeric calibration score`);
    t.assert(Array.isArray(targets.profiles[profile.id]?.resolved_combo_rows),
        `${profile.id}: stores resolved ability-tree rows`);
    t.assert(Object.hasOwn(profile, 'source_message_url'),
        `${profile.id}: records Discord provenance or an explicit null fallback`);
    t.assert(profile.source_message_url === null || profile.source_message_url.startsWith('https://discord.com/channels/'),
        `${profile.id}: Discord provenance has a canonical message URL`);
    for (const variant of REMOVAL_VARIANTS) {
        const scenario = manifestProfile.scenarios.find(entry => entry.variant === variant.name);
        const expected = deterministicRemoval(profile.id, variant.remove_count);
        t.assert(!!scenario, `${profile.id}/${variant.name}: scenario generated`);
        t.assert(scenario.profile_id === profile.id,
            `${profile.id}/${variant.name}: profile identity retained`);
        t.assert(scenario.build_family === profile.build_family,
            `${profile.id}/${variant.name}: build family retained as grouping key`);
        t.assert(scenario.class_name === profile.class_name,
            `${profile.id}/${variant.name}: class retained for family coverage analysis`);
        t.assert(scenario.free_mask === expected.mask,
            `${profile.id}/${variant.name}: deterministic free mask`);
        t.assert(JSON.stringify(scenario.free_equipment_slots) === JSON.stringify(expected.removed_slots),
            `${profile.id}/${variant.name}: deterministic removed slots`);
        t.assert(scenario.recovery_contract.required === variant.required,
            `${profile.id}/${variant.name}: correctness versus sensitivity role retained`);
        t.assert(scenario.recovery_contract.enforcement === (variant.enforce ? 'fail' : 'report'),
            `${profile.id}/${variant.name}: enforcement mode retained`);
        t.assert(scenario.time_limit_seconds === variant.time_limit_seconds,
            `${profile.id}/${variant.name}: default time cap retained`);
        const snapshotPath = path.join(REPO_ROOT, 'js', 'solver', 'tests', 'snapshots',
            `${scenario.snapshot}.snap.json`);
        t.assert(fs.existsSync(snapshotPath), `${profile.id}/${variant.name}: JS snapshot exists`);
    }
}

t.summary();
