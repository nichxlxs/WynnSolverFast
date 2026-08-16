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
const _set_sensitivity_stat = ctx._set_sensitivity_stat;
const reduce_candidate_pools = ctx.reduce_candidate_pools;
const get_candidate_search_stages = ctx.get_candidate_search_stages;

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
    _prune_dominated_items(pools, ds);
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
    _prune_dominated_items(pools, ds);
    t.assert(pools.helmet.length === 1, 'Test 2: pool should have 1 item');
    t.assert(pools.helmet.includes(A), 'Test 2: A should remain');
}

// Test 3: Bidirectional — A dominates B on both directions
{
    const A = makeItem({ damPct: 20, spRaw1: -15 });
    const B = makeItem({ damPct: 15, spRaw1: -10 });
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct']), lower: new Set(['spRaw1']) };
    _prune_dominated_items(pools, ds);
    t.assert(pools.helmet.length === 1, 'Test 3: B should be pruned');
    t.assert(pools.helmet.includes(A), 'Test 3: A should remain');
}

// Test 4: No dominance when one direction fails
{
    const A = makeItem({ damPct: 20, spRaw1: -5 });   // better damage, worse cost
    const B = makeItem({ damPct: 15, spRaw1: -10 });  // worse damage, better cost
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct']), lower: new Set(['spRaw1']) };
    _prune_dominated_items(pools, ds);
    t.assert(pools.helmet.length === 2, 'Test 4: neither should be pruned');
}

// Test 5: SP requirements break dominance
{
    const A = makeItem({ damPct: 20 }, [50,0,0,0,0]);  // higher reqs
    const B = makeItem({ damPct: 15 }, [10,0,0,0,0]);
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct']), lower: new Set() };
    _prune_dominated_items(pools, ds);
    t.assert(pools.helmet.length === 2, 'Test 5: B not pruned due to lower reqs');
}

// Test 6: SP provisions break dominance
{
    const A = makeItem({ damPct: 20 }, [0,0,0,0,0], [0,0,0,0,0]);  // no SP
    const B = makeItem({ damPct: 15 }, [0,0,0,0,0], [5,0,0,0,0]);  // gives str
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct']), lower: new Set() };
    _prune_dominated_items(pools, ds);
    t.assert(pools.helmet.length === 2, 'Test 6: B not pruned due to SP provisions');
}

// Test 7: NONE items never pruned
{
    const A = makeItem({ damPct: 20 });
    const N = makeNoneItem();
    const pools = { helmet: [A, N] };
    const ds = { higher: new Set(['damPct']), lower: new Set() };
    _prune_dominated_items(pools, ds);
    t.assert(pools.helmet.length === 2, 'Test 7: NONE item not pruned');
    t.assert(pools.helmet.includes(N), 'Test 7: NONE item still in pool');
}

// Test 8: Empty lower set — same behavior as original
{
    const A = makeItem({ damPct: 20, sdPct: 10 });
    const B = makeItem({ damPct: 15, sdPct: 5 });
    const pools = { helmet: [A, B] };
    const ds = { higher: new Set(['damPct', 'sdPct']), lower: new Set() };
    _prune_dominated_items(pools, ds);
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

// Test 17b: benchmark strategy ratios expose conservative/current/aggressive fronts
{
    const snap = { combo_time: 0, parsed_combo: [] };
    const dmg_weights = new Map([['damPct', 1], ['weakStat', 0.003]]);
    const restrictions = { stat_thresholds: [] };
    const conservative = _build_dominance_stats(
        snap, dmg_weights, restrictions, { sensitivity_ratio: 0 });
    const current = _build_dominance_stats(snap, dmg_weights, restrictions);
    const aggressive = _build_dominance_stats(
        snap, dmg_weights, restrictions, { sensitivity_ratio: 0.02 });
    t.assert(conservative.higher.has('weakStat'),
        'Test 17b: conservative strategy retains weak nonzero sensitivity');
    t.assert(!current.higher.has('weakStat'),
        'Test 17b: current strategy excludes sensitivity below 0.5%');
    t.assert(!aggressive.higher.has('weakStat'),
        'Test 17b: aggressive strategy excludes sensitivity below 2%');
}

// Test 18: ordinary set pieces are not safe standalone dominance candidates.
// A weaker item can enable a globally superior set bonus with another slot.
{
    const standalone = makeItem({ damPct: 20 });
    const setPiece = makeItem({ damPct: 10 });
    setPiece.statMap.set('set', 'Test Set');
    const pools = { helmet: [standalone, setPiece] };
    const ds = { higher: new Set(['damPct']), lower: new Set() };
    _prune_dominated_items(pools, ds);
    t.assert(pools.helmet.includes(setPiece),
        'Test 18: set piece survives standalone-stat dominance pruning');

    const legacyPools = { helmet: [standalone, setPiece] };
    _prune_dominated_items(legacyPools, ds, { preserve_set_items: false });
    t.assert(!legacyPools.helmet.includes(setPiece),
        'Test 18: benchmark-only legacy mode reproduces original set pruning');
}

// Test 18b: healPct is finalized into healMult.item. Sensitivity perturbation
// must update both representations or healing objectives see a false zero
// sensitivity and can prune the exact optimum.
{
    const combo = new Map([
        ['healPct', 2],
        ['healMult', new Map([['item', 2]])],
    ]);
    _set_sensitivity_stat(combo, 'healPct', 10);
    t.assert(combo.get('healPct') === 10,
        'Test 18b: healing sensitivity updates the source item stat');
    t.assert(combo.get('healMult').get('item') === 10,
        'Test 18b: healing sensitivity updates the finalized multiplier');
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
    _prune_dominated_items(pools, ds);
    t.assert(pools.necklace.includes(tierItem),
        'Test 19: unique atkTier item survives dominance pruning');

    // Equal atkTier values still allow normal dominance.
    const weakSameTier = makeItem({ damPct: 1, atkTier: 1 });
    const pools2 = { necklace: [tierItem, weakSameTier] };
    _prune_dominated_items(pools2, { higher: new Set(['damPct']), lower: new Set(), equal: new Set(['atkTier']) });
    t.assert(!pools2.necklace.includes(weakSameTier),
        'Test 19: equal-stat items still prune on monotonic stats');
}

// ── Dominance modes: off / safe / legacy ─────────────────────────────────────

const _normalize_dominance_mode = ctx._normalize_dominance_mode;
const _dominance_signature = ctx._dominance_signature;

t.assert(_normalize_dominance_mode('OFF') === 'off'
    && _normalize_dominance_mode('exact') === 'off'
    && _normalize_dominance_mode('none') === 'off',
    'Modes: off has exact/none aliases and is case-insensitive');
t.assert(_normalize_dominance_mode('nonsense') === 'legacy'
    && _normalize_dominance_mode(undefined) === 'legacy',
    'Modes: an unrecognised mode falls back to the shipped default');

// The oracle: `off` must leave every pool exactly as it found it. Without this
// there is no baseline to measure the heuristic against.
{
    const strong = makeItem({ damPct: 30 });
    const weak = makeItem({ damPct: 10 });
    const pools = { necklace: [strong, weak] };
    const pruned = _prune_dominated_items(
        pools, { higher: new Set(['damPct']), lower: new Set(), equal: new Set() },
        { mode: 'off' });
    t.assert(pruned === 0 && pools.necklace.length === 2
        && pools.necklace.includes(weak),
        'Modes: off prunes nothing, including a strictly dominated item');
}

// `safe` removes only indistinguishable items.
{
    const a = makeItem({ damPct: 20, sdPct: 5 }, [1, 0, 0, 0, 0], [0, 2, 0, 0, 0]);
    const twin = makeItem({ damPct: 20, sdPct: 5 }, [1, 0, 0, 0, 0], [0, 2, 0, 0, 0]);
    const dominated = makeItem({ damPct: 10, sdPct: 5 });
    const pools = { necklace: [a, twin, dominated] };
    const pruned = _prune_dominated_items(
        pools, { higher: new Set(['damPct']), lower: new Set(), equal: new Set() },
        { mode: 'safe' });
    t.assert(pruned === 1 && pools.necklace.length === 2,
        'Modes: safe removes an exact duplicate');
    t.assert(pools.necklace.includes(dominated),
        'Modes: safe keeps a merely dominated item -- only identity is proof');
}

// A difference in any consumed value must defeat the safe signature.
{
    const base = makeItem({ damPct: 20 });
    const differs = makeItem({ damPct: 20 });
    differs.statMap.set('set', 'Morph');
    t.assert(_dominance_signature(base) !== _dominance_signature(differs),
        'Modes: safe signature separates items differing only by set membership');

    const reqDiffers = makeItem({ damPct: 20 }, [0, 0, 1, 0, 0]);
    t.assert(_dominance_signature(base) !== _dominance_signature(reqDiffers),
        'Modes: safe signature separates items differing only by SP requirement');

    // Presentation-only fields must NOT split otherwise identical items,
    // or `safe` would never dedupe anything real.
    const renamed = makeItem({ damPct: 20 });
    renamed.statMap.set('displayName', 'Something Else');
    base.statMap.set('displayName', 'Original');
    t.assert(_dominance_signature(base) === _dominance_signature(renamed),
        'Modes: safe signature ignores display name');

    // Fail closed: an unknown key the solver might consume must be compared.
    const unknownKey = makeItem({ damPct: 20 });
    unknownKey.statMap.set('displayName', 'Original');
    unknownKey.statMap.set('someFutureMechanic', 7);
    t.assert(_dominance_signature(base) !== _dominance_signature(unknownKey),
        'Modes: safe signature includes unknown keys rather than ignoring them');
}

// The exclusive-set tag lives on the wrapper, not the statMap.
{
    const a = makeItem({ damPct: 20 });
    const b = makeItem({ damPct: 20 });
    b._illegalSet = 'Slayer';
    t.assert(_dominance_signature(a) !== _dominance_signature(b),
        'Modes: safe signature includes the exclusive-set tag');
}

// The pool-reduction report is what an A/B against `off` reads.
{
    const pools = { necklace: [makeItem({ damPct: 30 }), makeItem({ damPct: 10 })] };
    _prune_dominated_items(
        pools, { higher: new Set(['damPct']), lower: new Set(), equal: new Set() },
        { mode: 'legacy' });
    const report = ctx._dominance_report();
    t.assert(report && report.mode === 'legacy' && report.pruned === 1,
        'Modes: the run reports its mode and prune count');
    t.assert(report.per_slot.necklace.before === 2 && report.per_slot.necklace.after === 1,
        'Modes: per-slot pool sizes are recorded for measurement');
}

// ── Summary ──────────────────────────────────────────────────────────────────

// Test 20: the reducer preserves a discrete melee enabler even when the local
// sensitivity probe is zero. This is the reduced form of the exact
// mage_riftwalker_cancelstack counterexample where Knucklebones (+3 atkTier)
// was incorrectly classified as dominated by an empty bracelet.
{
    t.assert(typeof reduce_candidate_pools === 'function',
        'Test 20: candidate reducer interface is available');
    if (typeof reduce_candidate_pools === 'function') {
        const empty = makeItem({});
        const knucklebones = makeItem({ atkTier: 3, ls: -2100, ms: -160 });
        const pools = { bracelet: [empty, knucklebones] };
        const reduction = reduce_candidate_pools(pools, {
            snap: {
                combo_time: 0,
                parsed_combo: [{ spell: { base_spell: 0, scaling: 'melee' }, mana_excl: true }],
            },
            dmg_weights: new Map(),
            restrictions: { stat_thresholds: [] },
            mode: 'certified',
        });
        t.assert(reduction.active_pools.bracelet.includes(knucklebones),
            'Test 20: certified reduction preserves zero-gradient atkTier items');
    }
}

// Test 21: below-threshold but nonzero objective dimensions are uncertain, not
// irrelevant. This models the aggressive slow-heavy-melee counterexample where
// Diamond Fiber Bracelet was removed after its damage dimensions were omitted.
{
    if (typeof reduce_candidate_pools === 'function') {
        const buzzsaw = makeItem({}, [50,0,0,0,0], [6,0,0,0,0]);
        const diamondFiber = makeItem(
            { mdPct: 16, eDamPct: 16 },
            [100,0,0,0,0],
            [6,0,0,0,0],
        );
        const reduction = reduce_candidate_pools({ bracelet: [buzzsaw, diamondFiber] }, {
            snap: {
                combo_time: 0,
                parsed_combo: [{ spell: { base_spell: 0, scaling: 'melee' }, mana_excl: true }],
            },
            dmg_weights: new Map([
                ['dominantScale', 1],
                ['mdPct', 0.003],
                ['eDamPct', 0.003],
            ]),
            restrictions: { stat_thresholds: [] },
            mode: 'balanced',
        });
        t.assert(reduction.active_pools.bracelet.includes(diamondFiber),
            'Test 21: balanced reduction defers rather than deletes weak active dimensions');
    }
}

// Test 22: every removed item remains recoverable through the deferred pool and
// carries a machine-readable dominance certificate for diagnostics and staged
// widening.
{
    if (typeof reduce_candidate_pools === 'function') {
        const strong = makeItem({ damPct: 20 });
        const weak = makeItem({ damPct: 10 });
        const reduction = reduce_candidate_pools({ helmet: [strong, weak] }, {
            snap: { combo_time: 0, parsed_combo: [] },
            dmg_weights: new Map([['damPct', 1]]),
            restrictions: { stat_thresholds: [] },
            mode: 'certified',
        });
        const certificate = reduction.certificates.find(entry => entry.item === weak);
        t.assert(reduction.deferred_pools.helmet.includes(weak)
            && certificate?.dominator === strong
            && certificate?.proof_kind === 'contract_dominance',
        'Test 22: reducer returns deferred items with dominance certificates');
    }
}

// Test 23: a standalone stat comparison cannot prove that an item with a
// unique Major ID is replaceable. Major IDs can change combat behaviour in
// ways that do not appear in rolled identification stats.
{
    const plain = makeItem({ damPct: 20 });
    const major = makeItem({ damPct: 10 });
    major.statMap.set('majorIds', ['FREERUNNER']);
    const pools = { boots: [plain, major] };
    _prune_dominated_items(pools, {
        higher: new Set(['damPct']), lower: new Set(), equal: new Set(),
    });
    t.assert(pools.boots.includes(major),
        'Test 23: items with Major IDs are not standalone-dominance pruned');
}

// Test 24: static armour HP participates in an EHP contract. It lives on the
// item statMap rather than maxRolls, so omitting it could erase the only item
// that makes an EHP threshold feasible.
{
    if (typeof reduce_candidate_pools === 'function') {
        const damage = makeItem({ damPct: 20 });
        const health = makeItem({ damPct: 10 });
        health.statMap.set('hp', 4000);
        const reduction = reduce_candidate_pools({ chestplate: [damage, health] }, {
            snap: { combo_time: 0, parsed_combo: [] },
            dmg_weights: new Map([['damPct', 1]]),
            restrictions: { stat_thresholds: [{ stat: 'ehp', op: 'ge', value: 20000 }] },
            mode: 'certified',
        });
        t.assert(reduction.active_pools.chestplate.includes(health),
            'Test 24: certified EHP reduction preserves static armour HP');
    }
}

// Test 25: disabling equipment pruning does not disable the dominance contract
// metadata consumed by tome optimisation and diagnostics.
{
    if (typeof reduce_candidate_pools === 'function') {
        const reduction = reduce_candidate_pools({ ring: [makeItem({ mr: 5 })] }, {
            snap: { combo_time: 0, parsed_combo: [] },
            dmg_weights: new Map([['mr', 1]]),
            restrictions: { stat_thresholds: [] },
            mode: 'off',
        });
        t.assert(reduction.dominance_stats?.higher?.has('mr')
            && reduction.removed_count === 0,
        'Test 25: unpruned mode retains contract metadata without removing items');
    }
}

// Test 26: fast verification first searches the guarded active pool, then the
// full pool. The second stage provides eventual exhaustive coverage while the
// first supplies an incumbent for score bounds and early UI results.
{
    t.assert(typeof get_candidate_search_stages === 'function'
        && JSON.stringify(get_candidate_search_stages('fast_verify'))
            === JSON.stringify(['balanced', 'off']),
    'Test 26: fast verification expands from balanced to the full pool');
}

// Test 27: Balanced is the product default when no explicit pruning mode is
// supplied. Certified and unpruned modes remain available for exact controls.
{
    t.assert(typeof get_candidate_search_stages === 'function'
        && JSON.stringify(get_candidate_search_stages())
            === JSON.stringify(['balanced']),
    'Test 27: unspecified pruning mode defaults to balanced');
}

const summary = t.summary();
if (require.main === module) {
    if (summary.fail > 0) process.exit(1);
}
module.exports = summary;
