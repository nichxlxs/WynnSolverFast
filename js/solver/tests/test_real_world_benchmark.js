'use strict';

const fs = require('fs');
const path = require('path');
const { TestRunner } = require('./harness');
const { parseSolverOutput, compareVariants } = require('../benchmarks/real_world_lib');

const t = new TestRunner('Real-world benchmark parsing');
const parsed = parseSolverOutput(`
  [solver_case] pool sizes: { helmet: 39, chestplate: 48 }
  [solver_case] input combinations: 4096
  [solver_case] search combinations: 1872
  [solver_case] dominance: safe, 87 -> 84 items
  [solver_case] checked: 1872, feasible: 1752, top5: 15, time: 4280ms
  [solver_case] best score: 15095
`);
t.assert(parsed.checked === 1872 && parsed.feasible === 1752, 'search funnel is parsed');
t.assert(parsed.elapsedMs === 4280 && !parsed.timedOut, 'completed duration is parsed');
t.assert(parsed.bestScore === 15095, 'best score is parsed');
t.assert(parsed.poolSizes.helmet === 39 && parsed.poolSizes.chestplate === 48, 'pool sizes are parsed');
t.assert(parsed.combinations === 1872, 'search-space combinations are parsed');
t.assert(parsed.inputCombinations === 4096, 'unoptimized input combinations are parsed');
t.assert(parsed.dominanceMode === 'safe'
    && parsed.dominanceInputItems === 87 && parsed.dominanceOutputItems === 84,
'dominance policy and item reduction are parsed');

const timed = parseSolverOutput('[x] checked: 5000, feasible: 9, top5: 15, time: 5500ms (timed out)\n[x] best score: 99');
t.assert(timed.timedOut, 'timeout is parsed');

const empty = parseSolverOutput('[x] checked: 36, feasible: 30, top5: 0, time: 80ms');
t.assert(empty.topCount === 0 && empty.bestScore === null,
    'completed empty search is parsed without inventing a score');

const comparison = compareVariants(
    { checked: 100, elapsedMs: 1000, bestScore: 10, combinations: 1000 },
    { checked: 150, elapsedMs: 1000, bestScore: 12, combinations: 1250 },
);
t.assert(Math.abs(comparison.throughputChangePct - 50) < 1e-9, 'throughput change is calculated');
t.assert(Math.abs(comparison.scoreChangePct - 20) < 1e-9, 'score change is calculated');
t.assert(comparison.originalSpace === 1000 && comparison.currentSpace === 1250,
    'both search spaces are retained in comparisons');
t.assert(comparison.spaceChangePct === 25, 'search-space change is calculated');
t.assert(compareVariants(
    { checked: 1, elapsedMs: 1, bestScore: null },
    { checked: 1, elapsedMs: 1, bestScore: null },
).scoreChangePct === null, 'empty-result score comparison stays null');

const seeded = parseSolverOutput(`
  [x] search combinations: 100
  [x] checked: 50, feasible: 1, top5: 1, time: 1000ms (timed out)
  [x] best score: 470163
`);
t.assert(seeded.bestScore === 470163, 'seeded incumbent score is preserved in output');

let invalidRejected = false;
try { parseSolverOutput('not solver output'); } catch (err) { invalidRejected = true; }
t.assert(invalidRejected, 'invalid solver output is rejected');

const familyManifest = JSON.parse(fs.readFileSync(
    path.join(__dirname, '..', 'benchmarks', 'family_suite.json'),
    'utf8',
));
const expectedSizes = ['small', 'medium', 'large'];
t.assert(familyManifest.families.length === 6, 'family manifest covers six build families');
t.assert(familyManifest.families.every((family) =>
    family.variants.map((variant) => variant.size).join(',') === expectedSizes.join(',')),
'each family has ordered small, medium, and large variants');
t.assert(familyManifest.families.every((family) =>
    family.variants[0].calibrated_search_combinations
        >= familyManifest.minimum_small_search_combinations),
'every small family variant exceeds one million search combinations');
t.assert(familyManifest.families.every((family) =>
    family.variants[2].calibrated_search_combinations
        <= familyManifest.maximum_large_search_combinations),
'every large family variant stays at or below 1.88 trillion search combinations');
t.assert(familyManifest.families.every((family) =>
    family.variants[2].calibrated_input_combinations
        <= familyManifest.maximum_large_input_combinations),
'every large family variant stays at or below 1.88 trillion input combinations');
t.assert(familyManifest.families.every((family) =>
    family.variants.every((variant, index, variants) => index === 0
        || variant.calibrated_search_combinations > variants[index - 1].calibrated_search_combinations)),
'search spaces increase strictly within every family');
t.assert(familyManifest.families.every((family) =>
    family.variants.every((variant, index, variants) => index === 0
        || variant.calibrated_input_combinations > variants[index - 1].calibrated_input_combinations)),
'input spaces increase strictly within every family');
t.assert(familyManifest.families.every((family) => family.variants.every((variant) =>
    variant.calibrated_search_combinations === variant.calibrated_input_combinations)),
'exact-mode family calibration uses the raw post-filter Cartesian space');
t.assert(familyManifest.families.every((family) => family.variants.every((variant) =>
    variant.combination_budget.search_min === variant.combination_budget.input_min
        && variant.combination_budget.search_max === variant.combination_budget.input_max)),
'exact-mode search and input calibration bands are identical');
t.assert(familyManifest.families.every((family) =>
    family.variants.every((variant, index, variants) => index === 0
        || variant.provided_ideal_item_count < variants[index - 1].provided_ideal_item_count)),
'larger variants are created by providing fewer ideal-build items');

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
module.exports = summary;
