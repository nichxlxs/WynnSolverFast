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

// ── Tome roll percentage is scoped, and actually moves the numbers ─────────
//
// with_tome_roll swaps the module-global roll mode. The restore matters more
// than the swap: leaking a tome percentage into item expansion would silently
// reroll every item in the search.

const roll = run(`
    current_roll_mode = { damage: 30, mana: 30, healing: 30, misc: 30 };
    const raw = [...tomeMap.values()].find(x => x.type === 'armorTome' && x.hpBonus);
    const at = pct => with_tome_roll(pct, () =>
        tome_stat(_apply_roll_mode_to_item(new Item(raw)).statMap, 'hpBonus'));
    const lo = at(0), mid = at(50), hi = at(100);
    const outside = current_roll_mode.damage;
    // Throwing inside must still restore — otherwise one bad tome poisons the run.
    let threw = false;
    try { with_tome_roll(90, () => { throw new Error('boom'); }); }
    catch (e) { threw = true; }
    return JSON.stringify({ lo, mid, hi, outside, after_throw: current_roll_mode.damage, threw });
`);
t.assert(roll.lo < roll.mid && roll.mid < roll.hi,
    `a higher tome roll gives a higher stat (${roll.lo} < ${roll.mid} < ${roll.hi})`);
t.assert(roll.outside === 30,
    `the item roll mode is restored after with_tome_roll (got ${roll.outside})`);
t.assert(roll.threw && roll.after_throw === 30,
    `the roll mode is restored even when the body throws (got ${roll.after_throw})`);
t.assert(run(`return JSON.stringify(with_tome_roll(undefined, () => current_roll_mode.damage));`)
    === TOME_ROLL_DEFAULT_VAL(),
    'an absent percentage falls back to TOME_ROLL_DEFAULT');
function TOME_ROLL_DEFAULT_VAL() {
    return run(`return JSON.stringify(TOME_ROLL_DEFAULT);`);
}

// ── Inventory filtering ────────────────────────────────────────────────────

const inv = run(`
    const some = [...tomeMap.values()].filter(x => x.type === 'weaponTome' && x.id !== undefined);
    const owned = new Set([some[0].id]);
    return JSON.stringify({
        null_allows_all: some.every(x => tome_inventory_allows(null, x)),
        owned_allowed:   tome_inventory_allows(owned, some[0]),
        unowned_blocked: some.slice(1).every(x => !tome_inventory_allows(owned, x)),
        // A synthetic tome with no id (the "none" placeholders) is never filtered:
        // an empty slot must stay reachable whatever the user owns.
        idless_allowed:  tome_inventory_allows(owned, { name: 'No Weapon Tome' }),
        empty_blocks_all: some.every(x => !tome_inventory_allows(new Set(), x)),
    });
`);
t.assert(inv.null_allows_all, 'a null inventory owns everything');
t.assert(inv.owned_allowed, 'a ticked tome is allowed');
t.assert(inv.unowned_blocked, 'an unticked tome is blocked');
t.assert(inv.idless_allowed, 'an id-less (placeholder) tome is never filtered out');
t.assert(inv.empty_blocks_all,
    'an EMPTY inventory blocks every tome — it is not treated as "owns everything"');

// The filter has to shrink the real pool the optimiser searches, not just pass
// its own unit test: a pool filtered to two tomes must produce fewer bundles.
const inv_pool = run(`
    const KEYS = ['hpBonus','hprRaw','mdPct','sdPct'];
    const all = [], two = [];
    const owned = new Set();
    let n = 0;
    for (const [, raw] of tomeMap) {
        if (raw.type !== 'armorTome') continue;
        if ((raw.name || '').startsWith('No ')) continue;
        if (n++ < 2) owned.add(raw.id);
    }
    with_tome_roll(80, () => {
        for (const [, raw] of tomeMap) {
            if (raw.type !== 'armorTome') continue;
            if ((raw.name || '').startsWith('No ')) continue;
            const sm = _apply_roll_mode_to_item(new Item(raw)).statMap;
            all.push(sm);
            if (tome_inventory_allows(owned, raw)) two.push(sm);
        }
    });
    return JSON.stringify({
        all_pool: all.length, filtered_pool: two.length,
        all_bundles: tome_bundles(tome_prune_dominated(all, KEYS), 4, KEYS).length,
        filtered_bundles: tome_bundles(tome_prune_dominated(two, KEYS), 4, KEYS).length,
    });
`);
t.assert(inv_pool.filtered_pool === 2,
    `the inventory filters the real pool (${inv_pool.all_pool} -> ${inv_pool.filtered_pool})`);
t.assert(inv_pool.filtered_bundles <= inv_pool.all_bundles && inv_pool.filtered_bundles >= 1,
    `a filtered pool yields no more bundles and never zero `
    + `(${inv_pool.all_bundles} -> ${inv_pool.filtered_bundles})`);

// ── End to end through _prepare_tome_optimisation ──────────────────────────
//
// The unit checks above prove the pieces; this proves the production entry
// point actually consults them. An empty solver_item_final_nodes means no tome
// is equipped anywhere, so every slot is optimisable.

vm.runInContext('globalThis.solver_item_final_nodes = [];', ctx);

const DOM_STATS = `{ higher: new Set(['sdPct','mdPct','hpBonus']), lower: new Set(), equal: new Set() }`;

const prep = run(`
    const call = (inv, roll) => {
        const s = { tome_opt: TOME_OPT_ALL, level: 106, tome_roll: roll,
                    tome_inventory: inv ? new Set(inv) : null };
        _prepare_tome_optimisation(s, {}, ${DOM_STATS});
        return s;
    };
    // Own two armour tomes, two weapon tomes, and exactly one guild tome.
    // Level-eligible ones, or the level gate would empty the pool and the
    // bundle comparison below would prove nothing.
    const owned = [];
    let w = 0, a = 0;
    for (const [, raw] of tomeMap) {
        if ((raw.name || '').startsWith('No ')) continue;
        if ((raw.lvl ?? 0) > 106) continue;
        if (raw.type === 'weaponTome' && w < 2) { owned.push(raw.id); w++; }
        if (raw.type === 'armorTome' && a < 2) { owned.push(raw.id); a++; }
    }
    const one_guild = guild_tome_real(3);   // Intelligence
    owned.push(one_guild.id);

    const all = call(null, 80);
    const some = call(owned, 80);
    const none_owned = call([], 80);
    // Level 50 is below every guild tome, so the level gate excludes them all —
    // the other way to reach the "only none survives" case.
    const low = (() => {
        const s = { tome_opt: TOME_OPT_ALL, level: 50, tome_roll: 80, tome_inventory: null };
        _prepare_tome_optimisation(s, {}, ${DOM_STATS});
        return s;
    })();
    return JSON.stringify({
        all_guild: all.guild_tome_candidates?.length ?? 0,
        some_guild: some.guild_tome_candidates?.map(c => c.idx) ?? null,
        none_guild: none_owned.guild_tome_candidates?.map(c => c.idx) ?? null,
        low_guild: low.guild_tome_candidates?.map(c => c.idx) ?? null,
        all_bundles: all.tome_wa_bundles?.length ?? 0,
        some_bundles: some.tome_wa_bundles?.length ?? 0,
        none_bundles: none_owned.tome_wa_bundles?.length ?? 0,
    });
`);
t.assert(prep.all_guild === GUILD_TOMES_LEN(),
    `with no inventory every guild tome is a candidate (got ${prep.all_guild})`);
t.assert(JSON.stringify(prep.some_guild) === JSON.stringify([0, 3]),
    `owning only the Intelligence tome leaves candidates [none, int] (got ${JSON.stringify(prep.some_guild)})`);
// Regression (Codex P1 on PR #12): when NO guild tome survives filtering, the
// candidate list must still contain the "none" candidate rather than collapsing
// to null. Null means "the guild tome is fixed" to the worker, which then falls
// back to guild_tome_sm — synthesised from the manual dropdown that the UI
// greys out in optimisation mode. A user who picked Strength before enabling
// optimisation would get builds using a tome they just said they do not own.
t.assert(JSON.stringify(prep.none_guild) === JSON.stringify([0]),
    `owning no guild tome leaves the "none" candidate active, not null `
    + `(got ${JSON.stringify(prep.none_guild)})`);
t.assert(JSON.stringify(prep.low_guild) === JSON.stringify([0]),
    `a build below every guild tome's level keeps the "none" candidate too `
    + `(got ${JSON.stringify(prep.low_guild)})`);
t.assert(prep.some_bundles > 0 && prep.some_bundles <= prep.all_bundles,
    `the inventory shrinks the bundle front (${prep.all_bundles} -> ${prep.some_bundles})`);
// No owned weapon/armour tome means no bundle DIMENSION at all, not a bundle
// front containing one empty option: a null front is the signal that makes the
// worker run its plain no-bundle path. Guild optimisation is unaffected — the
// candidates are computed before this point.
t.assert(prep.none_bundles === 0,
    `owning no weapon/armour tome removes the bundle dimension (got ${prep.none_bundles})`);
function GUILD_TOMES_LEN() {
    return run(`return JSON.stringify(GUILD_TOMES.length);`);
}

// The roll percentage has to reach the bundles the workers actually receive,
// not just the pool: the bound is what gates enumeration.
const prep_roll = run(`
    const before = current_roll_mode.damage;
    const call = roll => {
        const s = { tome_opt: TOME_OPT_ALL, level: 106, tome_roll: roll, tome_inventory: null };
        _prepare_tome_optimisation(s, {}, ${DOM_STATS});
        let best = 0;
        for (const [, v] of (s.tome_bound ?? new Map())) best = Math.max(best, v);
        return best;
    };
    const lo = call(10), hi = call(100);
    return JSON.stringify({ lo, hi, before, after: current_roll_mode.damage });
`);
t.assert(prep_roll.lo < prep_roll.hi,
    `a higher tome roll raises the precheck bound (${prep_roll.lo} < ${prep_roll.hi})`);
t.assert(prep_roll.after === prep_roll.before,
    `_prepare_tome_optimisation leaves the item roll mode untouched `
    + `(${prep_roll.before} -> ${prep_roll.after})`);

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
