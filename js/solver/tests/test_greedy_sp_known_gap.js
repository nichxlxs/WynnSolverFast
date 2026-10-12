// Known failure C6: the greedy skill-point allocator is not globally optimal.
// Run: node js/solver/tests/test_greedy_sp_known_gap.js
//
// The leaf scorer spends the unassigned skill points with greedy_sp_allocate:
// coordinate ascent in steps of 20, then 4, then 1. A step of 20 can spend the
// whole budget on one attribute before the finer steps get a chance, so on a
// concave objective an even split can beat it. That means an exhaustive search
// over equipment proves the best equipment UNDER GREEDY ALLOCATION, not the
// best build. Reported on PR #19 (reproduced: greedy 20/0 scores
// 1.18176053024, the feasible 10/10 split 1.19015423825, 0.71% better).
//
// This file records the gap rather than hiding it. While the gap exists it
// emits a WARN, not a FAIL, so the suite stays usable; when a certified
// allocator lands (roadmap R12) the gap closes and the file says so, at which
// point the WARN branch should become a hard assertion.

'use strict';

const { createSandbox, TestRunner } = require('./harness');

const t = new TestRunner('Greedy SP known gap (C6)');
const ctx = createSandbox();
const { greedy_sp_allocate, skillPointsToPercentage } = ctx;

t.assert(typeof greedy_sp_allocate === 'function', 'greedy_sp_allocate is loaded');

// The review's normalized objective: neutral-only damage with no crit damage
// bonus and no mana or tree coupling reduces to 1 + f(Str) + f(Dex).
const f = (sp) => skillPointsToPercentage(sp);
const objective = (total) => 1 + f(total[0]) + f(total[1]);

const base = [0, 0, 60, 60, 60];
const remaining = 20;

// Greedy, through the production allocator.
const gBase = base.slice();
const gTotal = base.slice();
const cap = [150, 150, 150, 150, 150];
greedy_sp_allocate(gBase, gTotal, remaining, cap, null, () => objective(gTotal), null);
const greedyScore = objective(gTotal);

// Exhaustive over the two lanes that matter (every integer split of 20).
let bestScore = -Infinity, bestSplit = null;
for (let s = 0; s <= remaining; s++) {
    const total = base.slice();
    total[0] += s;
    total[1] += remaining - s;
    const v = objective(total);
    if (v > bestScore) { bestScore = v; bestSplit = [s, remaining - s]; }
}

t.assert(bestScore >= greedyScore,
    'the exhaustive split is never worse than greedy (oracle sanity)');
t.assertClose(greedyScore, 1.1817605302407017, 1e-12,
    'greedy result reproduces the reported 20/0 score');

const gap = (bestScore - greedyScore) / greedyScore;
if (gap > 1e-12) {
    t.warn(`KNOWN FAILURE C6: greedy allocates [${gTotal[0]}, ${gTotal[1]}] `
        + `scoring ${greedyScore}; best split ${JSON.stringify(bestSplit)} scores `
        + `${bestScore} (${(gap * 100).toFixed(4)}% better). See roadmap R12.`);
} else {
    console.log('  C6 appears fixed: greedy now matches the exhaustive split. '
        + 'Turn the WARN branch of this test into a hard assertion.');
}

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
