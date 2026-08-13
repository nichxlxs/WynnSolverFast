// ══════════════════════════════════════════════════════════════════════════════
// TOME BUNDLE TESTS
//
// Covers the pieces the tome optimiser is built on:
//   - tome_stat()             reads rolled IDs out of maxRolls, not top level
//   - tome_prune_dominated()  collapses tiered tome lines
//   - tome_bundles()          multiset combinations, Pareto-pruned
//   - tome_optimistic_bound() per-key maxima used as an admissible bound
//
// Run: node js/solver/tests/test_tome_bundles.js
// ══════════════════════════════════════════════════════════════════════════════

'use strict';

const vm = require('vm');
const fs = require('fs');
const path = require('path');
const { createSandbox, loadGameData, TestRunner, REPO_ROOT } = require('./harness');

const ctx = createSandbox();
loadGameData(ctx);
// search.js supplies _apply_roll_mode_to_item, which is what turns a tome's
// rolled ID range into concrete values.
vm.runInContext(fs.readFileSync(path.join(REPO_ROOT, 'js', 'solver', 'engine', 'search.js'), 'utf8'),
    ctx, { filename: 'search.js' });

const t = new TestRunner('Tome Bundles');

function run(src) {
    return JSON.parse(vm.runInContext(`(function(){ ${src} })()`, ctx));
}

// ── tome_stat reads rolled IDs ─────────────────────────────────────────────
//
// This is the trap: _apply_roll_mode_to_item rewrites the maxRolls map in
// place and never sets top-level keys, so reading sm.get('hpBonus') on a tome
// returns undefined and any dominance/bound computation silently sees zero.

vm.runInContext('current_roll_mode = { damage: 80, mana: 80, healing: 80, misc: 80 };', ctx);

const rolled = run(`
    const raw = [...tomeMap.values()].find(x => x.type === 'armorTome' && x.hpBonus);
    // Capture the roll range BEFORE applying roll mode: _apply_roll_mode_to_item
    // overwrites maxRolls in place with the rolled result.
    const before = new Item(raw).statMap;
    const lo = before.get('minRolls')?.get('hpBonus') ?? null;
    const hi = before.get('maxRolls')?.get('hpBonus') ?? null;
    const sm = _apply_roll_mode_to_item(new Item(raw)).statMap;
    return JSON.stringify({
        top_level: sm.get('hpBonus') ?? null,
        via_helper: tome_stat(sm, 'hpBonus'),
        lo, hi, base: raw.hpBonus,
    });
`);
t.assert(rolled.top_level === null,
    'a rolled tome stat is NOT a top-level statMap key (guards the trap)');
t.assert(rolled.via_helper > 0,
    `tome_stat reads the rolled value (got ${rolled.via_helper})`);
// The raw field is the BASE value; the rolled range sits around it, so compare
// against the range rather than the base.
t.assert(rolled.lo !== null && rolled.hi !== null,
    `the tome carries a rolled range for hpBonus (${rolled.lo}..${rolled.hi})`);
t.assert(rolled.via_helper >= rolled.lo && rolled.via_helper <= rolled.hi,
    `an 80% roll lands inside the range (${rolled.lo} <= ${rolled.via_helper} <= ${rolled.hi})`);
t.assert(tome_stat_missing() === 0, 'tome_stat returns 0 for an absent key');
function tome_stat_missing() {
    return run(`return JSON.stringify(tome_stat(new Map(), 'hpBonus'));`);
}

// ── Dominance and bundles on synthetic pools ───────────────────────────────

const synth = run(`
    const mk = obj => { const m = new Map(); const r = new Map();
        for (const [k, v] of Object.entries(obj)) r.set(k, v);
        m.set('maxRolls', r); return m; };
    const KEYS = ['a', 'b'];
    // t3 dominates t1 and t2; t4 is a distinct trade-off.
    const pool = [mk({a: 1, b: 1}), mk({a: 2, b: 1}), mk({a: 3, b: 2}), mk({a: 0, b: 9})];
    const pruned = tome_prune_dominated(pool, KEYS);
    const dup = tome_prune_dominated([mk({a: 5, b: 5}), mk({a: 5, b: 5})], KEYS);
    const pairs = tome_bundles(pruned, 2, KEYS);
    const single = tome_bundles(pruned, 1, KEYS);
    return JSON.stringify({
        pruned: pruned.map(m => KEYS.map(k => tome_stat(m, k))),
        dup_len: dup.length,
        pair_vecs: pairs.map(p => p.vec).sort(),
        pair_picks: pairs.map(p => p.picks.length),
        single_len: single.length,
    });
`);
t.assert(synth.pruned.length === 2,
    `dominated tomes are dropped (kept ${synth.pruned.length}, expected 2)`);
t.assert(JSON.stringify(synth.pruned.sort()) === JSON.stringify([[0, 9], [3, 2]].sort()),
    `the survivors are the non-dominated pair (got ${JSON.stringify(synth.pruned)})`);
t.assert(synth.dup_len === 1,
    'two identical tomes collapse to one rather than deleting each other');
t.assert(synth.pair_picks.every(n => n === 2),
    'each 2-slot bundle picks exactly 2 tomes');
// Duplicates are allowed, so [3,2]+[3,2] = [6,4] must be present.
t.assert(JSON.stringify(synth.pair_vecs).includes('[6,4]'),
    `duplicates are legal, so a doubled tome bundle exists (got ${JSON.stringify(synth.pair_vecs)})`);
t.assert(synth.single_len === 2, 'a 1-slot bundle set is just the pruned pool');

// ── Equality-scoped keys (score-positive but le-capped stats) ──────────────
//
// signs[i] === 0 marks a stat where neither direction is safely better; a
// dominator must match it exactly, or the only cap-compliant tome could be
// pruned in favour of one that violates the cap.

const eq = run(`
    const mk = obj => { const m = new Map(); const r = new Map();
        for (const [k, v] of Object.entries(obj)) r.set(k, v);
        m.set('maxRolls', r); return m; };
    const KEYS = ['a', 'b'];
    // t1 beats t2 on 'a' but differs on 'b'.
    const pool = [mk({a: 3, b: 5}), mk({a: 1, b: 2})];
    return JSON.stringify({
        with_eq:    tome_prune_dominated(pool, KEYS, [1, 0]).length,
        without_eq: tome_prune_dominated(pool, KEYS, [1, 1]).length,
        bundles_eq: tome_bundles(pool, 1, KEYS, [1, 0]).length,
    });
`);
t.assert(eq.with_eq === 2,
    `tomes differing on an equality key are both kept (got ${eq.with_eq})`);
t.assert(eq.without_eq === 1,
    `sanity: without the equality mark the dominated tome is pruned (got ${eq.without_eq})`);
t.assert(eq.bundles_eq === 2,
    `bundle Pareto also respects equality keys (got ${eq.bundles_eq})`);

// ── The optimistic bound is an upper bound on every real bundle ────────────

const bound = run(`
    const mk = obj => { const m = new Map(); const r = new Map();
        for (const [k, v] of Object.entries(obj)) r.set(k, v);
        m.set('maxRolls', r); return m; };
    const KEYS = ['a', 'b'];
    const pool = [mk({a: 3, b: 2}), mk({a: 0, b: 9})];
    const b1 = tome_optimistic_bound(pool, 1);
    const b2 = tome_optimistic_bound(pool, 2);
    const bundles = tome_bundles(pool, 2, KEYS);
    const worst = KEYS.map((k, i) => Math.max(...bundles.map(x => x.vec[i])));
    return JSON.stringify({
        b1: KEYS.map(k => b1.get(k) ?? 0),
        b2: KEYS.map(k => b2.get(k) ?? 0),
        max_real: worst,
    });
`);
t.assert(JSON.stringify(bound.b1) === JSON.stringify([3, 9]),
    `the bound is the per-key maximum (got ${JSON.stringify(bound.b1)})`);
t.assert(JSON.stringify(bound.b2) === JSON.stringify([6, 18]),
    `the bound scales with slot count (got ${JSON.stringify(bound.b2)})`);
for (let i = 0; i < bound.max_real.length; i++) {
    t.assert(bound.b2[i] >= bound.max_real[i],
        `bound[${i}] (${bound.b2[i]}) is >= the best real bundle (${bound.max_real[i]}) — admissible`);
}

// ── Real tome pools: pruning has to actually bite ──────────────────────────

const real = run(`
    const KEYS = ['hpBonus','hprRaw','ls','damPct','mdPct','sdPct','mdRaw','sdRaw',
        'eDamPct','tDamPct','wDamPct','fDamPct','aDamPct',
        'eDefPct','tDefPct','wDefPct','fDefPct','aDefPct','str','dex','int','def','agi'];
    const byType = {};
    for (const [, raw] of tomeMap) {
        if (!['weaponTome','armorTome','guildTome'].includes(raw.type)) continue;
        if ((raw.name || '').startsWith('No ')) continue;
        (byType[raw.type] = byType[raw.type] || []).push(_apply_roll_mode_to_item(new Item(raw)).statMap);
    }
    const out = {};
    for (const [type, sms] of Object.entries(byType)) {
        const pruned = tome_prune_dominated(sms, KEYS);
        const slots = type === 'weaponTome' ? 2 : (type === 'armorTome' ? 4 : 1);
        out[type] = { pool: sms.length, pruned: pruned.length, slots,
                      bundles: tome_bundles(pruned, slots, KEYS).length };
    }
    return JSON.stringify(out);
`);
for (const [type, v] of Object.entries(real)) {
    t.assert(v.pruned >= 1 && v.pruned <= v.pool,
        `${type}: pruned pool is a non-empty subset (${v.pruned}/${v.pool})`);
    t.assert(v.bundles >= 1, `${type}: at least one bundle`);
}
t.assert(real.armorTome.pruned < real.armorTome.pool,
    `armour tome dominance actually prunes (${real.armorTome.pool} -> ${real.armorTome.pruned})`);
t.assert(real.weaponTome.pruned < real.weaponTome.pool,
    `weapon tome dominance actually prunes (${real.weaponTome.pool} -> ${real.weaponTome.pruned})`);
// Every guild tome grants a different attribute, so none dominates another.
t.assert(real.guildTome.pruned === real.guildTome.pool,
    `no guild tome dominates another (${real.guildTome.pruned}/${real.guildTome.pool})`);

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
