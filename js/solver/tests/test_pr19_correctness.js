// Regression tests for PR #19 correctness fixes.
// Run: node js/solver/tests/test_pr19_correctness.js

'use strict';

const { createSandbox, loadGameData, TestRunner } = require('./harness');
const { accumulate_reachable_set_bonus } = require('../engine/sp_set_bound');

const ctx = createSandbox();
const { itemMap } = loadGameData(ctx);
const t = new TestRunner('PR19 correctness regressions');

const zeros = () => [0, 0, 0, 0, 0];

function sameArray(actual, expected, label) {
    t.assert(JSON.stringify(Array.from(actual)) === JSON.stringify(expected),
        `${label}: got ${JSON.stringify(Array.from(actual))}, expected ${JSON.stringify(expected)}`);
}

// 1. Reachable set-SP helper must sum independent set maxima, not take a global
// per-attribute max across all sets. Different disjoint sets can be worn at the
// same time, so their reachable bonuses are additive.
{
    const out = zeros();
    accumulate_reachable_set_bonus([[0, 40, 0, 0, 0]], 0, 1, out);
    accumulate_reachable_set_bonus([[0, 35, 0, 0, 0]], 0, 1, out);
    sameArray(out, [0, 75, 0, 0, 0],
        'reachable SP set bound sums disjoint set bonuses');
}

// 2. Set weapons count toward set bonuses. Bony Circlet + Bony Bow should grant
// the two-piece +8 agi set bonus during skill-point feasibility, and the
// non-SP set stats during leaf stat finalization.
{
    const circlet = ctx.expandItem(itemMap.get('Bony Circlet'));
    const bow = ctx.expandItem(itemMap.get('Bony Bow'));
    const none = ctx.none_items.slice(1, 8).map(it => ctx.expandItem(it));
    const result = ctx.calculate_skillpoints([circlet, ...none], bow, 200);

    t.assert(result !== null, 'Bony Circlet + Bony Bow is skillpoint-feasible');
    if (result) {
        const [assign, total, total_assigned, setCounts, totalItemSkillpoints] = result;
        t.assert(setCounts.get('Bony') === 2,
            `Bony weapon contributes to set count: got ${setCounts.get('Bony')}`);
        t.assert(total[4] === 14,
            `Bony two-piece agi total includes bow + set bonus: got ${total[4]}`);
        t.assert(totalItemSkillpoints[4] === 14,
            `Bony total item agi includes bow + set bonus: got ${totalItemSkillpoints[4]}`);
        t.assert(assign[4] === 0 && total_assigned === 0,
            `Bony set bonus satisfies weapon req without extra assigned SP: assign=${assign[4]} total_assigned=${total_assigned}`);
    }

    const running = ctx._init_running_statmap(106, [circlet, bow]);
    const finalized = ctx._finalize_leaf_statmap(
        running, bow, new Map([['Bony', 2]]), ctx.sets, [circlet, bow], null, null);
    t.assert(finalized.get('mdRaw') === 84,
        `Bony two-piece mdRaw applied during finalization: got ${finalized.get('mdRaw')}`);
    t.assert(finalized.get('aDamPct') === 15,
        `Bony two-piece aDamPct applied during finalization: got ${finalized.get('aDamPct')}`);
}

// 3. Greedy SP allocation should not get stuck at the first 20-point step when
// a split allocation scores higher. This synthetic surface has 10/10 as the
// best accepted state and 20/0 worse than the starting score.
{
    const base = [0, 0, 0, 0, 0];
    const total = [0, 0, 0, 0, 0];
    const score = () => {
        const a = total[0], b = total[1];
        return 100 - Math.abs(a - 10) - Math.abs(b - 10);
    };
    const allocated = ctx.greedy_sp_allocate(base, total, 20, [100, 100, 100, 100, 100], null, score, null);
    t.assert(allocated === 20,
        `greedy SP allocated the available split budget: got ${allocated}`);
    t.assert(base[0] === 10 && base[1] === 10,
        `greedy SP chose 10/10 synthetic optimum: got ${JSON.stringify(base)}`);
    t.assert(score() === 100,
        `greedy SP reached synthetic optimum score: got ${score()}`);
}

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
