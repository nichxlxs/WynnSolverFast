// ══════════════════════════════════════════════════════════════════════════════
// GUILD TOME TESTS
//
// Every guild tome in the game grants a FIXED per-attribute skill point bonus:
// five give +4 to one attribute, Assimilator's gives +1 to all five. Before
// v11 the solver modelled the "Standard (+4 SP)" option by inflating the
// assignable SP budget by 4, which let it spread the bonus across attributes
// and accept builds no real tome can support.
//
// These tests pin the corrected behaviour and would fail against that hack.
//
// Run: node js/solver/tests/test_guild_tome.js
// ══════════════════════════════════════════════════════════════════════════════

'use strict';

const vm = require('vm');
const { createSandbox, loadGameData, TestRunner } = require('./harness');

const ctx = createSandbox();
loadGameData(ctx);
const t = new TestRunner('Guild Tome');

/**
 * Solve skillpoints for one passive item with the given requirements, plus a
 * synthetic guild tome contributing `sp_vec`.
 */
function solve(sp_vec, reqs, budget) {
    ctx.__sp = JSON.stringify(sp_vec);
    ctx.__rq = JSON.stringify(reqs);
    ctx.__bg = budget;
    const out = vm.runInContext(`(function(){
        const mk = (skp, rq) => new Map([
            ['skillpoints', skp], ['reqs', rq], ['crafted', false], ['set', null],
        ]);
        const tome = mk(JSON.parse(__sp), [0, 0, 0, 0, 0]);
        const need = mk([0, 0, 0, 0, 0], JSON.parse(__rq));
        const wep  = mk([0, 0, 0, 0, 0], [0, 0, 0, 0, 0]);
        const r = calculate_skillpoints([need, tome], wep, __bg);
        return JSON.stringify(r ? { assign: [...r[0]], total: r[2] } : null);
    })()`, ctx);
    return JSON.parse(out);
}

const GUILD_TOMES = vm.runInContext('JSON.stringify(GUILD_TOMES)', ctx)
    ? JSON.parse(vm.runInContext('JSON.stringify(GUILD_TOMES)', ctx))
    : [];

// ── The table matches the real tomes ────────────────────────────────────────

t.assert(GUILD_TOMES.length === 7, `GUILD_TOMES has 7 entries (Off + 6 tomes), got ${GUILD_TOMES.length}`);
for (const g of GUILD_TOMES) {
    const sum = g.sp.reduce((a, b) => a + b, 0);
    const want = g.key === 'none' ? 0 : (g.key === 'rainbow' ? 5 : 4);
    t.assert(sum === want, `guild tome "${g.key}" grants ${sum} SP total (expected ${want})`);
    const lanes = g.sp.filter(v => v !== 0).length;
    const want_lanes = g.key === 'none' ? 0 : (g.key === 'rainbow' ? 5 : 1);
    t.assert(lanes === want_lanes,
        `guild tome "${g.key}" spreads over ${lanes} attribute(s) (expected ${want_lanes})`);
}

// ── Every guild tome resolves to a REAL tome ───────────────────────────────
//
// Regression: GUILD_TOMES records each tome's INTERNAL name, but tomeMap is
// keyed by displayName, and four of the six ship under a different display
// name (Psychopath's -> Brute's, Warlock's -> Mastermind's, Destroyer's ->
// Arsonist's, Sycophant's -> Ghost's). Looking them up by name returned
// undefined for those four, which silently defeated every check written
// against the lookup — the level gate fell back to its default and the
// owned-tome filter waved the tome through. guild_tome_real matches on the
// skill point vector instead, which no rename can break.

const resolved = JSON.parse(vm.runInContext(`(function(){
    return JSON.stringify(GUILD_TOMES.map((g, i) => {
        const r = guild_tome_real(i);
        return { key: g.key, name: g.item, found: r ? r.displayName : null,
                 id: r ? r.id : null, lvl: r ? r.lvl : null,
                 sp: r ? r.skillpoints : null };
    }));
})()`, ctx));

t.assert(resolved[0].found === null, 'the "Off" entry resolves to no tome');
for (let i = 1; i < resolved.length; i++) {
    const r = resolved[i];
    t.assert(r.found !== null,
        `guild tome "${r.key}" resolves to a real tome (got ${r.found})`);
    t.assert(JSON.stringify(r.sp) === JSON.stringify(GUILD_TOMES[i].sp),
        `"${r.key}" resolves to the tome granting exactly ${JSON.stringify(GUILD_TOMES[i].sp)}`);
    t.assert(Number.isInteger(r.id) && Number.isInteger(r.lvl),
        `"${r.key}" carries the id and level the gates and inventory need`);
}
const found_ids = resolved.slice(1).map(r => r.id);
t.assert(new Set(found_ids).size === found_ids.length,
    `each guild tome resolves to a DISTINCT tome (got ids ${JSON.stringify(found_ids)})`);

// Pin the mismatch itself, so a future data update that re-aligns the names
// (or breaks a different one) shows up here rather than as a silent no-op.
const renamed = resolved.slice(1).filter(r => r.found !== r.name);
t.assert(renamed.length === 4,
    `4 of 6 guild tomes display under a different name than the internal one `
    + `(got ${renamed.length}: ${renamed.map(r => r.name + ' -> ' + r.found).join('; ')})`);

// ── Per-attribute cap: the tome must supply the right attribute ─────────────
//
// An item needing 104 Strength cannot be worn on assigned points alone (the cap
// is 100 per attribute). Only a Strength tome closes the gap.

const CAP_REQ = [104, 0, 0, 0, 0];
t.assert(solve([0, 0, 0, 0, 0], CAP_REQ, 200) === null,
    '104 Str is infeasible with no guild tome (per-attribute cap is 100)');

const cap_str = solve([4, 0, 0, 0, 0], CAP_REQ, 200);
t.assert(cap_str !== null && cap_str.assign[0] === 100,
    '104 Str becomes feasible with a +4 Str tome, assigning exactly 100');

t.assert(solve([0, 4, 0, 0, 0], CAP_REQ, 200) === null,
    'a +4 Dex tome does not satisfy a Strength requirement');

// ── Budget bound: this is the case the old hack got wrong ──────────────────
//
// Needing dex 100 + int 100 + def 4 is 204 effective points against a 200
// budget. The pre-v11 implementation raised the budget to 204 and accepted
// assign [0,100,100,4,0] — 4 points in a lane no tome was contributing to.
// Only a tome in the Defense lane should make this feasible.

const BUD_REQ = [0, 100, 100, 4, 0];

t.assert(solve([0, 0, 0, 0, 0], BUD_REQ, 200) === null,
    '204 required against a 200 budget is infeasible with no guild tome');

t.assert(solve([4, 0, 0, 0, 0], BUD_REQ, 200) === null,
    'REGRESSION: a +4 Str tome must not satisfy a Defense requirement '
    + '(the pre-v11 budget hack accepted this)');

const bud_def = solve([0, 0, 0, 4, 0], BUD_REQ, 200);
t.assert(bud_def !== null && bud_def.total === 200,
    'a +4 Def tome makes it feasible using only the 200-point budget');

// Rainbow supplies 1 per lane, which is not enough for a 4-point Defense gap.
t.assert(solve([1, 1, 1, 1, 1], BUD_REQ, 200) === null,
    'Rainbow (+1 each) does not cover a 4-point Defense gap');

// Reproduce the old behaviour to show the tests above genuinely discriminate.
t.assert(solve([0, 0, 0, 0, 0], BUD_REQ, 204) !== null,
    'sanity: the old "+4 budget" model did accept this — that was the bug');

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
