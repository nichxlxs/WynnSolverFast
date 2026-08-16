// Tests for the reachable set-granted skill-point term of the SP bound.
// Run: node js/solver/tests/test_sp_set_bound.js
//
// The bound this feeds is the one that decides whether a branch can be thrown
// away. Claim less than a completion can actually reach and the solver silently
// discards real builds; claim far more and it stops pruning. Both failures are
// invisible to a timing benchmark, so they are asserted directly here.

'use strict';

const { TestRunner } = require('./harness');
const {
    accumulate_reachable_set_bonus, reachable_set_counts,
} = require('../engine/sp_set_bound');

const t = new TestRunner('SP set bound');

const zeros = () => [0, 0, 0, 0, 0];
const cap = (rows, worn, reach) =>
    accumulate_reachable_set_bonus(rows, worn, reach, zeros());

/** What a build actually receives at a given piece count. Mirrors
 *  calculate_skillpoints: counts past the end of the table keep the top row. */
function trueBonus(rows, count) {
    if (count < 1 || rows.length === 0) return zeros();
    return rows[Math.min(count, rows.length) - 1];
}

// ── Basic shape ──────────────────────────────────────────────────────────────

const morph = [
    [0, 0, 0, 0, 0],    // 1 piece
    [0, 20, 0, 0, 0],   // 2 pieces
    [0, 85, 0, 0, 0],   // 3 pieces — the shape that motivated the fix
];

t.assert(JSON.stringify(cap(morph, 0, 3)) === JSON.stringify([0, 85, 0, 0, 0]),
    'three pieces reachable from nothing: the full +85 counts');
t.assert(JSON.stringify(cap(morph, 0, 2)) === JSON.stringify([0, 20, 0, 0, 0]),
    'only two pieces reachable: the three-piece bonus does not count');
t.assert(JSON.stringify(cap(morph, 0, 0)) === JSON.stringify([0, 0, 0, 0, 0]),
    'no pieces reachable: the set contributes nothing');
t.assert(JSON.stringify(cap(morph, 2, 1)) === JSON.stringify([0, 85, 0, 0, 0]),
    'two already worn plus one reachable reaches the top row');
t.assert(JSON.stringify(cap([], 3, 3)) === JSON.stringify(zeros())
    && JSON.stringify(cap(null, 3, 3)) === JSON.stringify(zeros()),
    'an empty or absent bonus table contributes nothing');

// The regression that motivated extracting this function: a set worn more times
// than its table has rows must keep the top row, not fall through the inverted
// range and contribute zero. That bug shipped briefly in the Rust port and cost
// two feasible builds on fam_hybrid_small.
t.assert(JSON.stringify(cap(morph, 5, 0)) === JSON.stringify([0, 85, 0, 0, 0]),
    'worn past the end of the table keeps the top row, not zero');
t.assert(JSON.stringify(cap(morph, 5, 3)) === JSON.stringify([0, 85, 0, 0, 0]),
    'worn past the end with reach to spare still keeps the top row');

// Negative bonuses must never be credited as provision.
const mixed = [[0, 0, 0, 0, 0], [-25, 0, 0, 15, 15]];
const mixedCap = cap(mixed, 0, 2);
t.assert(mixedCap[0] === 0, 'a negative set bonus is not credited as provision');
t.assert(mixedCap[3] === 15 && mixedCap[4] === 15,
    'positive attributes of the same row are still credited');

// Accumulation across sets takes the per-attribute maximum into one buffer.
{
    const out = zeros();
    accumulate_reachable_set_bonus(morph, 0, 3, out);
    accumulate_reachable_set_bonus([[10, 100, 0, 0, 0]], 0, 1, out);
    t.assert(JSON.stringify(out) === JSON.stringify([10, 100, 0, 0, 0]),
        'accumulating a second set maxes per attribute in place');
}

// ── Admissibility, over random tables ────────────────────────────────────────
//
// The property that matters: for EVERY piece count a completion could reach,
// the bound must be at least what that count actually grants. A bound that
// dips below even once prunes a build the solver was supposed to find.
//
// Deterministic PRNG so a failure is reproducible from the seed alone.
// Take the high bits: this LCG's low bits barely vary, so `_seed % 8` returned
// nearly a constant and the clamp case below went uncovered while the coverage
// assertion still reported the trials as run.
let _seed = 20260815;
function rand(n) {
    _seed = (_seed * 1103515245 + 12345) & 0x7fffffff;
    return Math.floor(_seed / 65536) % n;
}

let admissible = true;
let tight = true;
let exercisedClamp = false;
let firstFailure = null;
for (let trial = 0; trial < 4000; trial++) {
    const rowCount = rand(6);                       // 0..5 tiers, including empty
    const rows = [];
    for (let r = 0; r < rowCount; r++) {
        const row = zeros();
        for (let j = 0; j < 5; j++) row[j] = rand(120) - 30;   // some negative
        rows.push(row);
    }
    const worn = rand(8);                           // can exceed rowCount
    const reach = rand(6);
    if (rowCount > 0 && worn > rowCount) exercisedClamp = true;

    const bound = cap(rows, worn, reach);
    const counts = reachable_set_counts(rows.length, worn, reach);

    for (const count of counts) {
        const actual = trueBonus(rows, count);
        for (let j = 0; j < 5; j++) {
            if (bound[j] < actual[j]) {
                admissible = false;
                firstFailure ??= { trial, rows, worn, reach, count, j, bound, actual };
            }
        }
    }
    // Not merely admissible: nothing above the reachable maximum, or the bound
    // stops pruning. Zero is always allowed (a set may simply not be worn).
    for (let j = 0; j < 5; j++) {
        const best = counts.reduce((m, c) => Math.max(m, trueBonus(rows, c)[j]), 0);
        if (bound[j] > Math.max(best, 0)) {
            tight = false;
            firstFailure ??= { trial, rows, worn, reach, j, bound, best };
        }
    }
}

t.assert(admissible,
    'admissible: never below what a reachable piece count actually grants'
    + (firstFailure ? ` — ${JSON.stringify(firstFailure)}` : ''));
t.assert(tight,
    'tight: never above the best reachable piece count'
    + (firstFailure ? ` — ${JSON.stringify(firstFailure)}` : ''));
t.assert(exercisedClamp,
    'the random trials actually covered worn-past-end-of-table');

// Reachability itself: counts below what is worn are unreachable, because
// pieces cannot be taken off partway through a search.
{
    const counts = reachable_set_counts(4, 2, 1);
    t.assert(JSON.stringify(counts) === JSON.stringify([2, 3]),
        'reachable counts start at what is worn and stop at worn + reach');
    t.assert(JSON.stringify(reachable_set_counts(0, 2, 2)) === '[]',
        'an empty table has no reachable counts');
    t.assert(JSON.stringify(reachable_set_counts(3, 9, 2)) === JSON.stringify([3]),
        'worn past the end collapses to the top row alone');
}

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
