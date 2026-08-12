'use strict';

const { TestRunner } = require('./harness');
const { parseSolverOutput, compareVariants } = require('../benchmarks/real_world_lib');

const t = new TestRunner('Real-world benchmark parsing');
const parsed = parseSolverOutput(`
  [solver_case] pool sizes: { helmet: 39, chestplate: 48 }
  [solver_case] input combinations: 4096
  [solver_case] search combinations: 1872
  [solver_case] checked: 1872, feasible: 1752, top5: 15, time: 4280ms
  [solver_case] best score: 15095
`);
t.assert(parsed.checked === 1872 && parsed.feasible === 1752, 'search funnel is parsed');
t.assert(parsed.elapsedMs === 4280 && !parsed.timedOut, 'completed duration is parsed');
t.assert(parsed.bestScore === 15095, 'best score is parsed');
t.assert(parsed.poolSizes.helmet === 39 && parsed.poolSizes.chestplate === 48, 'pool sizes are parsed');
t.assert(parsed.combinations === 1872, 'search-space combinations are parsed');
t.assert(parsed.inputCombinations === 4096, 'unoptimized input combinations are parsed');

const timed = parseSolverOutput('[x] checked: 5000, feasible: 9, top5: 15, time: 5500ms (timed out)\n[x] best score: 99');
t.assert(timed.timedOut, 'timeout is parsed');

const comparison = compareVariants(
    { checked: 100, elapsedMs: 1000, bestScore: 10, combinations: 1000 },
    { checked: 150, elapsedMs: 1000, bestScore: 12, combinations: 1250 },
);
t.assert(Math.abs(comparison.throughputChangePct - 50) < 1e-9, 'throughput change is calculated');
t.assert(Math.abs(comparison.scoreChangePct - 20) < 1e-9, 'score change is calculated');
t.assert(comparison.originalSpace === 1000 && comparison.currentSpace === 1250,
    'both search spaces are retained in comparisons');
t.assert(comparison.spaceChangePct === 25, 'search-space change is calculated');

const seeded = parseSolverOutput(`
  [x] search combinations: 100
  [x] checked: 50, feasible: 1, top5: 1, time: 1000ms (timed out)
  [x] best score: 470163
`);
t.assert(seeded.bestScore === 470163, 'seeded incumbent score is preserved in output');

let invalidRejected = false;
try { parseSolverOutput('not solver output'); } catch (err) { invalidRejected = true; }
t.assert(invalidRejected, 'invalid solver output is rejected');

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
module.exports = summary;
