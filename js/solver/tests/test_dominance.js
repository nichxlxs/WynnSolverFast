// Tests for bidirectional dominance pruning.
// Run: node js/solver/tests/test_dominance.js

'use strict';

const { createSandbox, TestRunner } = require('./harness');

const ctx = createSandbox();
const t = new TestRunner('Dominance Pruning');

// Access functions from the sandbox (function declarations are global).
const _prune_dominated_items = ctx._prune_dominated_items;
const _build_dominance_stats = ctx._build_dominance_stats;
const _item_stat_val = ctx._item_stat_val;

// ── Helpers ──────────────────────────────────────────────────────────────────

function makeItem(stats, reqs = [0,0,0,0,0], skillpoints = [0,0,0,0,0]) {
    const maxRolls = new Map(Object.entries(stats));
    const sm = new Map();
    sm.set('maxRolls', maxRolls);
    sm.set('reqs', reqs);
    sm.set('skillpoints', skillpoints);
    return { statMap: sm };
}

function makeNoneItem() {
    const sm = new Map();
    sm.set('NONE', true);
    sm.set('reqs', [0,0,0,0,0]);
    sm.set('skillpoints', [0,0,0,0,0]);
    sm.set('maxRolls', new Map());
    return { statMap: sm };
}

// ── _prune_dominated_items tests ─────────────────────────────────────────────

// Test 1: Higher-only (regression test for current behavior)
{
    const A = makeItem({ damPct: 20, sdPct: 10 });
    const B = makeItem({ damPct: 15, sdPct: 5 });   // dominated by A
    const C = makeItem({ damPct: 10, sdPct: 15 });   // not dominated (sdPct > A)
    const pools = { helmet: [A, B, C] };
    const ds = { higher: new Set(['damPct', 'sdPct']), lower: new Set() };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.helmet.length === 2, 'Test 1: pool should have 2 items');
    t.assert(pools.helmet.includes(A), 'Test 1: A should remain');
    t.assert(!pools.helmet.includes(B), 'Test 1: B should be pruned');
    t.assert(pools.helmet.includes(C), 'Test 1: C should remain');
}

// Test 2: Lower-only
{
    const A = makeItem({ spRaw1: -15 });  // lower = better, A dominates
    const B = makeItem({ spRaw1: -10 });
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(), lower: new Set(['spRaw1']) };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.helmet.length === 1, 'Test 2: pool should have 1 item');
    t.assert(pools.helmet.includes(A), 'Test 2: A should remain');
}

// Test 3: Bidirectional — A dominates B on both directions
{
    const A = makeItem({ damPct: 20, spRaw1: -15 });
    const B = makeItem({ damPct: 15, spRaw1: -10 });
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct']), lower: new Set(['spRaw1']) };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.helmet.length === 1, 'Test 3: B should be pruned');
    t.assert(pools.helmet.includes(A), 'Test 3: A should remain');
}

// Test 4: No dominance when one direction fails
{
    const A = makeItem({ damPct: 20, spRaw1: -5 });   // better damage, worse cost
    const B = makeItem({ damPct: 15, spRaw1: -10 });  // worse damage, better cost
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct']), lower: new Set(['spRaw1']) };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.helmet.length === 2, 'Test 4: neither should be pruned');
}

// Test 5: SP requirements break dominance
{
    const A = makeItem({ damPct: 20 }, [50,0,0,0,0]);  // higher reqs
    const B = makeItem({ damPct: 15 }, [10,0,0,0,0]);
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct']), lower: new Set() };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.helmet.length === 2, 'Test 5: B not pruned due to lower reqs');
}

// Test 6: SP provisions break dominance
{
    const A = makeItem({ damPct: 20 }, [0,0,0,0,0], [0,0,0,0,0]);  // no SP
    const B = makeItem({ damPct: 15 }, [0,0,0,0,0], [5,0,0,0,0]);  // gives str
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct']), lower: new Set() };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.helmet.length === 2, 'Test 6: B not pruned due to SP provisions');
}

// Test 7: NONE items never pruned
{
    const A = makeItem({ damPct: 20 });
    const N = makeNoneItem();
    const pools = { helmet: [A, N] };
    const ds = { higher: new Set(['damPct']), lower: new Set() };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.helmet.length === 2, 'Test 7: NONE item not pruned');
    t.assert(pools.helmet.includes(N), 'Test 7: NONE item still in pool');
}

// Test 8: Empty lower set — same behavior as original
{
    const A = makeItem({ damPct: 20, sdPct: 10 });
    const B = makeItem({ damPct: 15, sdPct: 5 });
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct', 'sdPct']), lower: new Set() };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.helmet.length === 1, 'Test 8: B pruned with empty lower set');
}

// ── _build_dominance_stats tests ─────────────────────────────────────────────

// Test 9: Pure damage combo (no mana) — lower is empty
{
    const snap = { combo_time: 0, parsed_combo: [], scoring_target: 'combo_damage' };
    const dmg_weights = new Map([['damPct', 1], ['sdPct', 1]]);
    const restrictions = { stat_thresholds: [] };
    const ds = _build_dominance_stats(snap, dmg_weights, restrictions);
    t.assert(ds.higher.has('damPct'), 'Test 9: damPct in higher');
    t.assert(ds.higher.has('sdPct'), 'Test 9: sdPct in higher');
    t.assert(ds.lower.size === 0, 'Test 9: lower is empty');
}

// Test 10: Mana-constrained combo — spell cost stats in lower
{
    const snap = {
        combo_time: 10, hp_casting: false,
        parsed_combo: [
            { spell: { base_spell: 1, scaling: 'spell' }, mana_excl: false },
            { spell: { base_spell: 3, scaling: 'spell' }, mana_excl: false },
        ],
    };
    const dmg_weights = new Map([['damPct', 1]]);
    const restrictions = { stat_thresholds: [] };
    const ds = _build_dominance_stats(snap, dmg_weights, restrictions);
    t.assert(ds.lower.has('spRaw1'), 'Test 10: spRaw1 in lower');
    t.assert(ds.lower.has('spPct1'), 'Test 10: spPct1 in lower');
    t.assert(ds.lower.has('spRaw3'), 'Test 10: spRaw3 in lower');
    t.assert(ds.lower.has('spPct3'), 'Test 10: spPct3 in lower');
    t.assert(!ds.lower.has('spRaw2'), 'Test 10: spRaw2 NOT in lower');
}

// Test 11: hp_casting excludes spell costs
{
    const snap = {
        combo_time: 10, hp_casting: true,
        parsed_combo: [
            { spell: { base_spell: 1, scaling: 'spell' }, mana_excl: false },
        ],
    };
    const dmg_weights = new Map([['damPct', 1]]);
    const restrictions = { stat_thresholds: [] };
    const ds = _build_dominance_stats(snap, dmg_weights, restrictions);
    t.assert(!ds.lower.has('spRaw1'), 'Test 11: no spell cost stats when hp_casting');
    t.assert(ds.lower.size === 0, 'Test 11: lower is empty');
}

// Test 12: le restriction adds to lower
{
    const snap = { combo_time: 0, parsed_combo: [] };
    const dmg_weights = new Map([['damPct', 1]]);
    const restrictions = { stat_thresholds: [{ stat: 'atkTier', op: 'le', value: 3 }] };
    const ds = _build_dominance_stats(snap, dmg_weights, restrictions);
    t.assert(ds.lower.has('atkTier'), 'Test 12: atkTier in lower from le restriction');
}

// Test 13: Conflict resolution — stat in both sets removed from both
{
    const snap = { combo_time: 0, parsed_combo: [] };
    const dmg_weights = new Map([['atkTier', 1]]);  // higher
    const restrictions = { stat_thresholds: [{ stat: 'atkTier', op: 'le', value: 3 }] };  // lower
    const ds = _build_dominance_stats(snap, dmg_weights, restrictions);
    t.assert(!ds.higher.has('atkTier'), 'Test 13: atkTier removed from higher');
    t.assert(!ds.lower.has('atkTier'), 'Test 13: atkTier removed from lower');
}

// Test 14: atkTier special case — melee + mana sustain
{
    const snap = {
        combo_time: 10, allow_downtime: false,
        parsed_combo: [
            { spell: { base_spell: 1, scaling: 'melee', cost: 60 }, qty: 1, sim_qty: 1, mana_excl: false },
        ],
        hp_casting: false,
    };
    const dmg_weights = new Map([['atkTier', 1], ['damPct', 1]]);
    const restrictions = { stat_thresholds: [] };
    const ds = _build_dominance_stats(snap, dmg_weights, restrictions);
    t.assert(!ds.higher.has('atkTier'), 'Test 14: atkTier removed from higher (melee+sustain)');
    t.assert(ds.higher.has('damPct'), 'Test 14: damPct still in higher');
}

// Test 15: Indirect stats filtered — ge ehp not added to higher
{
    const snap = { combo_time: 0, parsed_combo: [] };
    const dmg_weights = new Map();
    const restrictions = { stat_thresholds: [
        { stat: 'ehp', op: 'ge', value: 1000 },
        { stat: 'finalSpellCost1', op: 'le', value: 5 },
    ]};
    const ds = _build_dominance_stats(snap, dmg_weights, restrictions);
    t.assert(!ds.higher.has('ehp'), 'Test 15: ehp not in higher (indirect)');
    t.assert(!ds.lower.has('finalSpellCost1'), 'Test 15: finalSpellCost1 not in lower (indirect)');
}

// Test 16: mana_excl rows skipped
{
    const snap = {
        combo_time: 10, hp_casting: false,
        parsed_combo: [
            { spell: { base_spell: 1, scaling: 'spell' }, mana_excl: false },
            { spell: { base_spell: 2, scaling: 'spell' }, mana_excl: true },
        ],
    };
    const dmg_weights = new Map();
    const restrictions = { stat_thresholds: [] };
    const ds = _build_dominance_stats(snap, dmg_weights, restrictions);
    t.assert(ds.lower.has('spRaw1'), 'Test 16: spRaw1 in lower');
    t.assert(!ds.lower.has('spRaw2'), 'Test 16: spRaw2 NOT in lower (mana_excl)');
    t.assert(!ds.lower.has('spPct2'), 'Test 16: spPct2 NOT in lower (mana_excl)');
}

// Test 17: Melee rows (bs=0) skipped
{
    const snap = {
        combo_time: 10, hp_casting: false,
        parsed_combo: [
            { spell: { base_spell: 0, scaling: 'melee' }, mana_excl: false },
            { spell: { base_spell: 1, scaling: 'spell' }, mana_excl: false },
        ],
    };
    const dmg_weights = new Map();
    const restrictions = { stat_thresholds: [] };
    const ds = _build_dominance_stats(snap, dmg_weights, restrictions);
    t.assert(!ds.lower.has('spRaw0'), 'Test 17: spRaw0 NOT in lower (melee bs=0)');
    t.assert(ds.lower.has('spRaw1'), 'Test 17: spRaw1 in lower');
}

// Test 18: ordinary set pieces are not safe standalone dominance candidates.
// A weaker item can enable a globally superior set bonus with another slot.
{
    const standalone = makeItem({ damPct: 20 });
    const setPiece = makeItem({ damPct: 10 });
    setPiece.statMap.set('set', 'Test Set');
    const pools = { helmet: [standalone, setPiece] };
    const ds = { higher: new Set(['damPct']), lower: new Set() };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.helmet.includes(setPiece),
        'Test 18: set piece survives standalone-stat dominance pruning');

    const legacyPools = { helmet: [standalone, setPiece] };
    _prune_dominated_items(legacyPools, ds, { mode: 'legacy', preserve_set_items: false });
    t.assert(!legacyPools.helmet.includes(setPiece),
        'Test 18: benchmark-only legacy mode reproduces original set pruning');
}

// Test 19: relevant-but-non-monotonic stats require equality for dominance.
// An atkTier >= N restriction combined with melee + ls-constraint used to
// DELETE atkTier from the comparison, letting an atkTier-0 item dominate the
// only atkTier item in the pool and making the restriction unsatisfiable
// (observed: Recalcitrance pruned from the all-slots-free Gaia scenario).
{
    const snap = {
        combo_time: 0, hp_casting: false,
        parsed_combo: [{ spell: { base_spell: 0, scaling: 'melee' }, mana_excl: false }],
    };
    const restrictions = { stat_thresholds: [
        { stat: 'atkTier', op: 'ge', value: 6 },
        { stat: 'ls', op: 'ge', value: 0 },
    ] };
    const ds = _build_dominance_stats(snap, new Map(), restrictions);
    t.assert(!ds.higher.has('atkTier') && !ds.lower.has('atkTier'),
        'Test 19: atkTier leaves higher/lower under melee + ls constraint');
    t.assert(ds.equal.has('atkTier'),
        'Test 19: atkTier moves to the must-be-equal set instead of vanishing');

    const strong = makeItem({ damPct: 20 });
    const tierItem = makeItem({ damPct: 5, atkTier: 1 });
    const pools = { necklace: [strong, tierItem] };
    _prune_dominated_items(pools, ds, { mode: 'legacy' });
    t.assert(pools.necklace.includes(tierItem),
        'Test 19: unique atkTier item survives dominance pruning');

    // Equal atkTier values still allow normal dominance.
    const weakSameTier = makeItem({ damPct: 1, atkTier: 1 });
    const pools2 = { necklace: [tierItem, weakSameTier] };
    _prune_dominated_items(pools2,
        { higher: new Set(['damPct']), lower: new Set(), equal: new Set(['atkTier']) },
        { mode: 'legacy' });
    t.assert(!pools2.necklace.includes(weakSameTier),
        'Test 19: equal-stat items still prune on monotonic stats');
}

// Test 20: exact/off is the default and never changes a gear pool.
{
    const A = makeItem({ damPct: 20 });
    const B = makeItem({ damPct: 10 });
    const pools = { helmet: [A, B] };
    const metrics = {};
    const pruned = _prune_dominated_items(
        pools, { higher: new Set(['damPct']), lower: new Set() }, { metrics });
    t.assert(pruned === 0 && pools.helmet.length === 2,
        'Test 20: default exact mode preserves the raw pool');
    t.assert(metrics.mode === 'off' && metrics.input_items === 2 && metrics.pruned_items === 0,
        'Test 20: off-mode measurements are explicit');
}

// Test 21: safe mode only deduplicates a complete gameplay signature; display
// identity is deliberately irrelevant to evaluation.
{
    const A = makeItem({ damPct: 20, mr: 5 });
    const B = makeItem({ damPct: 20, mr: 5 });
    A.statMap.set('name', 'Cosmetic Variant A');
    B.statMap.set('name', 'Cosmetic Variant B');
    const pools = { helmet: [A, B] };
    const metrics = {};
    _prune_dominated_items(pools,
        { higher: new Set(['damPct']), lower: new Set() },
        { mode: 'safe', metrics });
    t.assert(pools.helmet.length === 1 && pools.helmet[0] === A,
        'Test 21: safe mode removes a complete-signature duplicate');
    t.assert(metrics.mode === 'safe' && metrics.pruned_items === 1
        && metrics.by_slot.helmet.output_items === 1,
    'Test 21: safe-mode reduction counters reconcile');
}

// Test 22: static armour HP is part of both sensitivity and the complete
// residual signature. This is the Water Mask / Last Stand regression shape.
{
    const damageItem = makeItem({ damPct: 20 });
    damageItem.statMap.set('hp', -40);
    const healthItem = makeItem({ damPct: 10 });
    healthItem.statMap.set('hp', 6550);
    const pools = { helmet: [damageItem, healthItem] };
    _prune_dominated_items(pools,
        { higher: new Set(['damPct']), lower: new Set() }, { mode: 'safe' });
    t.assert(pools.helmet.length === 2,
        'Test 22: safe dominance preserves a different static-HP signature');

    const snap = { combo_time: 0, parsed_combo: [] };
    const ds = _build_dominance_stats(snap, new Map(), {
        stat_thresholds: [{ stat: 'ehp', op: 'ge', value: 10000 }],
    });
    t.assert(ds.higher.has('hp') && ds.higher.has('hpBonus'),
        'Test 22: EHP constraints classify static hp and hpBonus');
}

// Test 23: every known nonlocal effect, plus unknown future keys, participates
// in safe signatures. Each weaker item must survive because its effect differs.
{
    const effectCases = [
        ['set', 'Test Set'],
        ['majorIds', ['TEST_MAJOR_ID']],
        ['powders', [1, 2]],
        ['crafted', true],
        ['custom', true],
        ['slots', 3],
        ['classReq', 'Mage'],
        ['restrict', 'untradable'],
        ['futureUnknownEffect', { amount: 7 }],
    ];
    let allPreserved = true;
    for (const [key, value] of effectCases) {
        const A = makeItem({ damPct: 20 });
        const B = makeItem({ damPct: 10 });
        B.statMap.set(key, value);
        const pools = { helmet: [A, B] };
        _prune_dominated_items(pools,
            { higher: new Set(['damPct']), lower: new Set() }, { mode: 'safe' });
        allPreserved = allPreserved && pools.helmet.length === 2;
    }
    const exclusiveA = makeItem({ damPct: 20 });
    const exclusiveB = makeItem({ damPct: 10 });
    exclusiveB._illegalSet = 'Master Hive';
    const exclusivePools = { helmet: [exclusiveA, exclusiveB] };
    _prune_dominated_items(exclusivePools,
        { higher: new Set(['damPct']), lower: new Set() }, { mode: 'safe' });
    allPreserved = allPreserved && exclusivePools.helmet.length === 2;
    t.assert(allPreserved,
        'Test 23: set/MajorID/powder/crafted/custom/slot/class/exclusive/unknown effects survive');
}

// Test 24: `exact` is a documented alias for `off`; legacy stays available
// only when a benchmark explicitly asks for the historical heuristic.
{
    const ds = { higher: new Set(['damPct']), lower: new Set() };
    const exact = { helmet: [makeItem({ damPct: 20 }), makeItem({ damPct: 10 })] };
    const legacy = { helmet: [makeItem({ damPct: 20 }), makeItem({ damPct: 10 })] };
    _prune_dominated_items(exact, ds, { mode: 'exact' });
    _prune_dominated_items(legacy, ds, { mode: 'legacy' });
    t.assert(exact.helmet.length === 2 && legacy.helmet.length === 1,
        'Test 24: exact/off and legacy policies are behaviorally distinct');
}

// Test 25: safe is signature deduplication, not sensitivity-based dominance.
{
    const A = makeItem({ damPct: 20 });
    const B = makeItem({ damPct: 10 });
    const pools = { helmet: [A, B] };
    _prune_dominated_items(pools,
        { higher: new Set(['damPct']), lower: new Set() }, { mode: 'safe' });
    t.assert(pools.helmet.length === 2,
        'Test 25: safe mode preserves any gameplay-stat difference');
}

// Test 26: objects with semantics outside simple enumerable data properties
// fail closed. Internal slots, typed storage, non-enumerable fields, accessors,
// symbols, and prototype methods cannot be inferred from Object.keys safely.
{
    class HiddenEffect {
        constructor(amount) {
            Object.defineProperty(this, 'amount', { value: amount, enumerable: false });
        }
        apply() { return this.amount; }
    }
    const hiddenPlain = () => {
        const value = {};
        Object.defineProperty(value, 'amount', { value: 7, enumerable: false });
        return value;
    };
    const accessorPlain = () => Object.defineProperty({}, 'amount', {
        enumerable: true,
        get() { return 7; },
    });
    const symbolPlain = () => ({ [Symbol('amount')]: 7 });
    const opaquePairs = [
        [new Date(0), new Date(0)],
        [/effect/gi, /effect/gi],
        [new Uint8Array([1, 2]), new Uint8Array([1, 2])],
        [new HiddenEffect(7), new HiddenEffect(7)],
        [hiddenPlain(), hiddenPlain()],
        [accessorPlain(), accessorPlain()],
        [symbolPlain(), symbolPlain()],
    ];
    let allPreserved = true;
    for (const [leftEffect, rightEffect] of opaquePairs) {
        const A = makeItem({ damPct: 20 });
        const B = makeItem({ damPct: 20 });
        A.statMap.set('futureOpaqueEffect', leftEffect);
        B.statMap.set('futureOpaqueEffect', rightEffect);
        const pools = { helmet: [A, B] };
        _prune_dominated_items(pools,
            { higher: new Set(['damPct']), lower: new Set() }, { mode: 'safe' });
        allPreserved = allPreserved && pools.helmet.length === 2;
    }
    t.assert(allPreserved,
        'Test 26: internal-slot/prototype/hidden/accessor/symbol effects cannot deduplicate');
}

// ── Summary ──────────────────────────────────────────────────────────────────

const summary = t.summary();
if (require.main === module) {
    if (summary.fail > 0) process.exit(1);
}
module.exports = summary;
