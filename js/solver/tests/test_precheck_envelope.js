// The raw `>=` prechecks must never reject a build whose final stat meets
// the restriction (C3 and C4 of the PR #19 review).
// Run: node js/solver/tests/test_precheck_envelope.js
//
// The prechecks compare a running sum of item stats against a threshold
// before the leaf adds set bonuses, the Radiance scale and ability-tree
// scaling. precheck_required_running (pure/utils.js) turns the threshold into
// the running sum a build must reach once those are counted; anything below
// it provably fails. These tests hold it to that, on the shipped case that
// exposed the bug and over random tables.

'use strict';

const { createSandbox, loadGameData, TestRunner } = require('./harness');

const t = new TestRunner('Precheck envelope');
const ctx = createSandbox();
const { itemMap, sets } = loadGameData(ctx);
const { precheck_envelope_context, precheck_required_running, collect_set_names,
        expandItem, skillPointsToPercentage } = ctx;
const skillpoint_final_mult = require('vm').runInContext('skillpoint_final_mult', ctx);

const noTree = (extra = {}) => precheck_envelope_context({
    atree_merged: new Map(), button_states: new Map(), slider_states: new Map(),
    radiance_boost: 1, sets_map: sets, ...extra,
});

// ── C3: Jester Bracelet + Jester Ring against xpb >= 5 ───────────────────────
{
    const bracelet = expandItem(itemMap.get('Jester Bracelet'));
    const ring = expandItem(itemMap.get('Jester Ring'));
    const running = bracelet.get('maxRolls').get('xpb') + ring.get('maxRolls').get('xpb');
    const twoPiece = sets.get('Jester').bonuses[1].xpb;
    t.assert(running < 5 && running + twoPiece >= 5,
        `the shipped case: raw xpb ${running}, +${twoPiece} set bonus, final ${running + twoPiece}`);

    const env = noTree({ set_names: collect_set_names([bracelet, ring]) });
    const need = precheck_required_running(env, ['xpb'], 5, 0, 0);
    t.assert(need !== null && running >= need,
        `the precheck admits the pair (running ${running} >= need ${need})`);

    const withoutSets = noTree({ set_names: [] });
    const old = precheck_required_running(withoutSets, ['xpb'], 5, 0, 0);
    t.assert(running < old,
        'without the set allowance the same check rejects it (the original bug)');
}

// ── Radiance: positive values scale up, so less is needed ────────────────────
{
    const env = noTree({ radiance_boost: 1.4, set_names: [] });
    const need = precheck_required_running(env, ['mr'], 14, 0, 0);
    t.assert(Math.abs(need - 10) < 1e-12, `Radiance 1.4: mr 10 can reach 14 (need ${need})`);
    const neg = precheck_required_running(env, ['mr'], -5, 0, 0);
    t.assert(neg === -5, 'a non-positive target is not divided (Radiance never raises negatives)');
    const notAffected = precheck_required_running(env, ['xpb'], 14, 0, 0);
    t.assert(notAffected === 14, 'a stat Radiance does not touch is unchanged');
}

// ── Randomized admissibility against the assembly formula ────────────────────
//
// final = floor-free rad(items + sets + pre) + post, rad(v) = v * r for v > 0.
// For every random case where the final value meets the threshold, the
// running item sum must be at least `need`.
{
    let seed = 7;
    const rand = (n) => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return Math.floor(seed / 65536) % n; };
    const fakeSets = new Map();
    let ok = true, fail = null;
    for (let trial = 0; trial < 5000 && ok; trial++) {
        fakeSets.clear();
        const names = [];
        for (let s = 0, n = rand(4); s < n; s++) {
            const rows = [];
            for (let r = 0, rc = 1 + rand(4); r < rc; r++) rows.push({ mr: rand(80) - 20 });
            fakeSets.set('S' + s, { bonuses: rows });
            names.push('S' + s);
        }
        const r = rand(2) ? 1 : 1 + rand(5) / 10;
        const env = precheck_envelope_context({
            atree_merged: new Map(), button_states: new Map(), slider_states: new Map(),
            radiance_boost: r, set_names: names, sets_map: fakeSets,
        });
        // Use a radiance-affected stat name so the scale applies when r > 1.
        const stat = 'mr';
        const pre = rand(40) - 20, post = rand(40) - 20, value = rand(200) - 60;
        const need = precheck_required_running(env, [stat], value, pre, post);
        const running = rand(240) - 120;
        // Actual: any reachable choice of one row per set (or none).
        let best = -Infinity;
        const walk = (i, acc) => {
            if (i === names.length) {
                const x = running + acc + pre;
                const fin = (x > 0 ? x * r : x) + post;
                if (fin > best) best = fin;
                return;
            }
            walk(i + 1, acc);
            for (const row of fakeSets.get(names[i]).bonuses) walk(i + 1, acc + row.mr);
        };
        walk(0, 0);
        if (best >= value && running < need) { ok = false; fail = { trial, running, need, best, value, r }; }
    }
    t.assert(ok, 'admissible over 5000 random set/Radiance cases'
        + (fail ? ` — ${JSON.stringify(fail)}` : ''));
}

// ── Ability-tree outputs ─────────────────────────────────────────────────────
{
    const env = noTree({ set_names: [] });
    env.var_keys = new Set(['sdPct']);
    t.assert(precheck_required_running(env, ['sdPct'], 10, 0, 0) === null,
        'a stat written by stat-dependent tree scaling has no envelope (precheck skipped)');
    env.const_scaled = new Map([['mdPct', 15]]);
    t.assert(precheck_required_running(env, ['mdPct'], 20, 0, 0) === 5,
        'a constant tree scaling output is credited');
    env.unknown_atree = true;
    t.assert(precheck_required_running(env, ['mdPct'], 20, 0, 0) === null,
        'an unanalysable tree disables every raw precheck');
}

// ── C4: the EHP divisor is taken at the 150 cap ──────────────────────────────
{
    const at = (sp) => skillPointsToPercentage(sp);
    const div = (sp) => {
        const d = at(sp) * skillpoint_final_mult[3], a = at(sp) * skillpoint_final_mult[4];
        return 0.1 * a + (1 - a) * (1 - d);
    };
    t.assert(div(150) < div(100),
        'the divisor at 150 is smaller, so EHP at 150 exceeds EHP at 100 (100 was not an upper bound)');
}

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
