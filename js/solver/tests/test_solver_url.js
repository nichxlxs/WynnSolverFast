// ══════════════════════════════════════════════════════════════════════════════
// SOLVER URL PARAM TESTS
//
// Round-trips encodeSolverParams/decodeSolverParams for the v11 fields and
// pins the backward-compatibility rules for older links.
//
// Run: node js/solver/tests/test_solver_url.js
// ══════════════════════════════════════════════════════════════════════════════

'use strict';

const vm = require('vm');
const { createSandbox, loadGameData, TestRunner } = require('./harness');

const ctx = createSandbox();
loadGameData(ctx);
const t = new TestRunner('Solver URL Params');

const BASE = {
    roll_groups: { damage: 85, mana: 100, healing: 85, misc: 85 },
    sfree: 0, dir_enabled: 31, lvl_min: 1, lvl_max: 121,
    lvl_overrides: {}, nomaj: false, gtome: 0, dtime: false, mana_disabled: false,
    restrictions: [], combo_rows: [], blacklist_ids: [], custom_weights: [],
};

function roundtrip(overrides) {
    ctx.__p = JSON.stringify({ ...BASE, ...overrides });
    const out = vm.runInContext(`(function(){
        const p = JSON.parse(__p);
        const s = encodeSolverParams(p);
        const d = decodeSolverParams(s);
        return JSON.stringify({ chars: s.length, d });
    })()`, ctx);
    return JSON.parse(out);
}

function decodeOnly(hash) {
    ctx.__h = hash;
    return JSON.parse(vm.runInContext(
        '(function(){ return JSON.stringify(decodeSolverParams(__h)); })()', ctx));
}

// ── Guild tome: all seven values survive, legacy values remap ──────────────

for (let g = 0; g < 7; g++) {
    const r = roundtrip({ gtome: g });
    t.assert(r.d && r.d.gtome === g, `guild tome ${g} round-trips (got ${r.d && r.d.gtome})`);
}

// A real pre-v11 link that carried gtome = 1 ("Standard (+4 SP)"). That mode
// has no real tome, so it must fall back to Off rather than silently becoming
// Strength — falling back can only understate a build, never overstate it.
const LEGACY_V10 = 'GV6Z6DQa-PiQ088Ef4438E-dKEJWGS2204Wz0000';
const legacy = decodeOnly(LEGACY_V10);
t.assert(legacy !== null, 'a pre-v11 solver link still decodes');
t.assert(legacy.gtome === 0,
    `legacy "Standard (+4 SP)" falls back to Off, not Strength (got ${legacy.gtome})`);
t.assert(legacy.restrictions.length === 6,
    `legacy link keeps its other fields (6 restrictions, got ${legacy.restrictions.length})`);
t.assert(legacy.lvl_min === 50 && legacy.lvl_max === 121,
    'legacy link keeps its global level range');
t.assert(JSON.stringify(legacy.lvl_overrides) === '{}',
    'legacy link decodes with no per-slot overrides');

// ── Per-slot level overrides ───────────────────────────────────────────────

const none = roundtrip({});
t.assert(JSON.stringify(none.d.lvl_overrides) === '{}', 'no overrides round-trips to {}');

const baseline_chars = none.chars;
t.assert(roundtrip({ lvl_overrides: {} }).chars === baseline_chars,
    'an empty override map costs no extra characters');

const both = roundtrip({
    lvl_overrides: {
        helmet: { min: 90, max: 121 }, chestplate: { min: 90, max: 121 },
        leggings: { min: 90, max: 121 }, boots: { min: 90, max: 121 },
        ring: { min: 100, max: 121 }, bracelet: { min: 100, max: 121 },
        necklace: { min: 100, max: 121 },
    },
});
t.assert(both.d.lvl_overrides.helmet?.min === 90 && both.d.lvl_overrides.ring?.min === 100,
    'armour 90+ / accessories 100+ round-trips');
t.assert(Object.keys(both.d.lvl_overrides).length === 7, 'all seven slots round-trip');

// The blank-side rule: a side left blank must come back blank (null), so it
// keeps inheriting the global bound instead of being frozen at whatever the
// global happened to be when the link was made.
const blanks = roundtrip({
    lvl_min: 50, lvl_max: 121,
    lvl_overrides: {
        helmet: { min: 90, max: null },     // min only
        boots: { min: null, max: 110 },     // max only
        necklace: { min: 100, max: 120 },   // both
    },
});
const ov = blanks.d.lvl_overrides;
t.assert(ov.helmet?.min === 90 && ov.helmet?.max === null,
    `a blank max stays blank (got ${JSON.stringify(ov.helmet)})`);
t.assert(ov.boots?.min === null && ov.boots?.max === 110,
    `a blank min stays blank (got ${JSON.stringify(ov.boots)})`);
t.assert(ov.necklace?.min === 100 && ov.necklace?.max === 120,
    'a fully specified slot round-trips both sides');
t.assert(!Object.values(ov).some(o => o.max === 121 && o.min === 90 && o !== ov.helmet),
    'a blank side is never backfilled with the global bound');

// A slot whose sides are both blank carries no information and is dropped.
const empty_slot = roundtrip({ lvl_overrides: { helmet: { min: null, max: null } } });
t.assert(JSON.stringify(empty_slot.d.lvl_overrides) === '{}',
    'a slot with both sides blank is not encoded');

// ── Tome roll percentage (v11 bit 11) ──────────────────────────────────────

const roll_default = vm.runInContext('TOME_ROLL_DEFAULT', ctx);
t.assert(none.d.tome_roll === roll_default,
    `an absent tome roll decodes to the default (${roll_default}, got ${none.d.tome_roll})`);
t.assert(roundtrip({ tome_roll: roll_default }).chars === baseline_chars,
    'a default tome roll costs no extra characters');
for (const pct of [0, 55, 100]) {
    const r = roundtrip({ tome_roll: pct });
    t.assert(r.d.tome_roll === pct, `tome roll ${pct}% round-trips (got ${r.d.tome_roll})`);
}

// ── Owned-tome inventory (v11 bit 12) ──────────────────────────────────────

const universe = vm.runInContext(
    '(function(){ return JSON.stringify(_solver_tome_universe()); })()', ctx);
const ALL_TOMES = JSON.parse(universe);
// tomeMap is keyed by displayName, so tomes sharing a name collapse to one
// entry — the universe is the SELECTABLE tomes, not every row in tomes.json.
t.assert(ALL_TOMES.length > 40,
    `the tome universe is the solver-relevant tomes (got ${ALL_TOMES.length})`);
t.assert(new Set(ALL_TOMES).size === ALL_TOMES.length && ALL_TOMES.every(Number.isInteger),
    'the universe is distinct integer ids');

t.assert(none.d.tome_inventory === null,
    'an absent inventory decodes to null (owns everything)');
t.assert(roundtrip({ tome_inventory: ALL_TOMES }).chars === baseline_chars,
    'owning every tome is encoded as absent — no wasted characters');

// Owning a handful stores the owned ids; owning all-but-a-handful stores the
// missing ids. Both must come back as the same owned set.
const few = ALL_TOMES.slice(0, 3);
const few_rt = roundtrip({ tome_inventory: few });
t.assert(JSON.stringify(few_rt.d.tome_inventory) === JSON.stringify(few),
    `a 3-tome inventory round-trips (got ${JSON.stringify(few_rt.d.tome_inventory)})`);

const most = ALL_TOMES.filter(id => id !== ALL_TOMES[0] && id !== ALL_TOMES[1]);
const most_rt = roundtrip({ tome_inventory: most });
t.assert(JSON.stringify(most_rt.d.tome_inventory) === JSON.stringify(most),
    'an all-but-two inventory round-trips');
t.assert(most_rt.chars < few_rt.chars + 10,
    `all-but-two is stored as the short missing list, not ${most.length} ids `
    + `(${most_rt.chars} chars vs ${few_rt.chars} for 3 owned)`);

// Owning nothing is a real state, distinct from owning everything: it must not
// collapse to null, or unticking every box would silently re-enable every tome.
const nil_rt = roundtrip({ tome_inventory: [] });
t.assert(Array.isArray(nil_rt.d.tome_inventory) && nil_rt.d.tome_inventory.length === 0,
    `an empty inventory stays empty (got ${JSON.stringify(nil_rt.d.tome_inventory)})`);

// The tome fields must not disturb the fields encoded around them.
const combined = roundtrip({
    tome_opt: 2, tome_roll: 65, tome_inventory: few,
    lvl_overrides: { helmet: { min: 90, max: null } },
    gtome: 6, lvl_min: 50,
});
t.assert(combined.d.tome_opt === 2 && combined.d.tome_roll === 65
    && combined.d.gtome === 6 && combined.d.lvl_min === 50
    && combined.d.lvl_overrides.helmet?.min === 90
    && JSON.stringify(combined.d.tome_inventory) === JSON.stringify(few),
    'tome_opt + tome_roll + inventory + overrides all coexist in one link');

// Pre-v11 links predate all three fields and must decode to the defaults.
t.assert(legacy.tome_roll === roll_default && legacy.tome_inventory === null
    && legacy.tome_opt === 0,
    'a pre-v11 link decodes with tome optimisation off, default roll, no inventory');

// ── Guild tome helpers used by the UI consumers ────────────────────────────

const helpers = JSON.parse(vm.runInContext(`(function(){
    return JSON.stringify({
        totals: GUILD_TOMES.map((_, i) => guild_tome_sp_total(i)),
        off: guild_tome_statmap(0),
        str: [...(guild_tome_statmap(1) || new Map())].map(([k, v]) => [k, v]),
    });
})()`, ctx));
t.assert(JSON.stringify(helpers.totals) === JSON.stringify([0, 4, 4, 4, 4, 4, 5]),
    `guild_tome_sp_total returns [0,4,4,4,4,4,5] (got ${JSON.stringify(helpers.totals)})`);
t.assert(helpers.off === null, 'guild_tome_statmap(0) is null for Off');
const strSkp = helpers.str.find(e => e[0] === 'skillpoints');
t.assert(strSkp && JSON.stringify(strSkp[1]) === JSON.stringify([4, 0, 0, 0, 0]),
    'guild_tome_statmap(1) grants exactly +4 Strength');

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
