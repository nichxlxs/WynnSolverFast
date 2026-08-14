// Tests for calculate_skillpoints.
// Run: node js/solver/tests/test_skillpoints.js [--update]
//
// Test cases live in test_skillpoints.json.
// --update  : recompute all expected values from current code (mass update).
//
// Generate random cases:  node js/solver/tests/gen_sp_cases.js [count]

'use strict';

const fs = require('fs');
const path = require('path');
const { createSandbox, loadGameData, TestRunner } = require('./harness');

const CASES_PATH = path.join(__dirname, 'test_skillpoints.json');

// ── Setup ────────────────────────────────────────────────────────────────────

const ctx = createSandbox();
const { itemMap, sets, none_items } = loadGameData(ctx);

const calculate_skillpoints = ctx.calculate_skillpoints;
const expandItem = ctx.expandItem;

// Slot index → item type for lookup in itemMap.
const SLOT_TYPES = ['helmet', 'chestplate', 'leggings', 'boots', 'ring', 'ring', 'bracelet', 'necklace'];
const NONE_NAMES = [
    'No Helmet', 'No Chestplate', 'No Leggings', 'No Boots',
    'No Ring 1', 'No Ring 2', 'No Bracelet', 'No Necklace',
];

// Wynn order: boots, legs, chest, helmet, ring1, ring2, bracelet, necklace
const WYNN_ORDER = [3, 2, 1, 0, 4, 5, 6, 7];

// ── Helpers ──────────────────────────────────────────────────────────────────

function resolveItem(name, slotIdx) {
    if (!name || name === NONE_NAMES[slotIdx]) return none_items[slotIdx];
    return itemMap.get(name) || null;
}

function resolveWeapon(name) {
    if (!name || name === 'No Weapon') return none_items[8];
    return itemMap.get(name) || null;
}

/**
 * Build the equipment statMap array in wynn order from a test case.
 */
function buildStatMaps(tc) {
    const rawEquips = [];
    for (let i = 0; i < 8; i++) {
        const item = resolveItem(tc.items[i], i);
        if (!item) return { error: `Unknown item "${tc.items[i]}" at slot ${i}` };
        rawEquips.push(item);
    }
    const weaponRaw = resolveWeapon(tc.weapon);
    if (!weaponRaw) return { error: `Unknown weapon "${tc.weapon}"` };

    const equipSMs = rawEquips.map(it => expandItem(it));
    const weaponSM = expandItem(weaponRaw);

    // Reorder to wynn order (boots, legs, chest, helmet, ring1, ring2, bracelet, necklace).
    const wynnEquipSMs = WYNN_ORDER.map(i => equipSMs[i]);

    return { equipSMs: wynnEquipSMs, weaponSM };
}

/**
 * Run calculate_skillpoints on a test case.
 * Returns { assign, total, total_assigned } or null if infeasible.
 */
function runSP(tc) {
    const resolved = buildStatMaps(tc);
    if (resolved.error) return { error: resolved.error };

    const budget = tc.sp_budget !== undefined ? tc.sp_budget : Infinity;
    const result = calculate_skillpoints(resolved.equipSMs, resolved.weaponSM, budget);
    if (result === null) return null;

    const [assign, total, total_assigned] = result;
    return {
        assign: Array.from(assign),
        total: Array.from(total),
        total_assigned,
    };
}

// ── Load & Run ──────────────────────────────────────────────────────────────

const isUpdate = process.argv.includes('--update');

const data = JSON.parse(fs.readFileSync(CASES_PATH, 'utf8'));
const cases = data.cases;

if (cases.length === 0 && !isUpdate) {
    console.log('No test cases in test_skillpoints.json (closure differential still runs). Use gen_sp_cases.js to generate some.');
}

if (isUpdate) {
    // ── Mass Update Mode ────────────────────────────────────────────────────
    let updated = 0;
    let errors = 0;
    for (const tc of cases) {
        const result = runSP(tc);
        if (result && result.error) {
            console.error(`  ERROR [${tc.name}]: ${result.error}`);
            errors++;
            continue;
        }
        tc.expected = result;  // null if infeasible, object if feasible
        updated++;
    }
    fs.writeFileSync(CASES_PATH, JSON.stringify(data, null, '\t') + '\n');
    console.log(`Updated ${updated} test case(s), ${errors} error(s).`);
    process.exit(errors > 0 ? 1 : 0);
}

// ── Normal Test Mode ────────────────────────────────────────────────────────

const t = new TestRunner('Skillpoints');

for (const tc of cases) {
    const label = tc.name;

    // Skip cases with null expected (not yet computed).
    if (tc.expected === null || tc.expected === undefined) {
        t.warn(`${label}: no expected values (run with --update or fill in manually)`);
        continue;
    }

    const resolved = buildStatMaps(tc);
    if (resolved.error) {
        t.assert(false, `${label}: ${resolved.error}`);
        continue;
    }

    const { equipSMs, weaponSM } = resolved;
    const budget = tc.sp_budget !== undefined ? tc.sp_budget : Infinity;
    const result = calculate_skillpoints(equipSMs, weaponSM, budget);

    if (tc.expected === 'infeasible') {
        t.assert(result === null, `${label}: expected infeasible, got result`);
        continue;
    }

    // Expected is { assign, total, total_assigned }.
    if (result === null) {
        t.assert(false, `${label}: calculate_skillpoints returned null (infeasible), expected feasible`);
        continue;
    }

    const [assign, total, total_assigned] = result;

    // Compare assign.
    let assign_ok = true;
    for (let i = 0; i < 5; i++) {
        if (assign[i] !== tc.expected.assign[i]) {
            t.assert(false, `${label}: assign[${i}] = ${assign[i]}, expected ${tc.expected.assign[i]}`);
            assign_ok = false;
        }
    }
    if (assign_ok) t.assert(true, `${label}: assign matches`);

    // Compare total.
    let total_ok = true;
    for (let i = 0; i < 5; i++) {
        if (total[i] !== tc.expected.total[i]) {
            t.assert(false, `${label}: total[${i}] = ${total[i]}, expected ${tc.expected.total[i]}`);
            total_ok = false;
        }
    }
    if (total_ok) t.assert(true, `${label}: total matches`);

    // Compare total_assigned.
    t.assert(
        total_assigned === tc.expected.total_assigned,
        `${label}: total_assigned = ${total_assigned}, expected ${tc.expected.total_assigned}`
    );
}

// ── Weapon-inclusive set membership ────────────────────────────────────────
//
// The separately-passed weapon must contribute to both the active-set count
// and the SP bonus pool. Bony's two-piece bonus is
// +8 AGI; together with Bony Bow's +6 AGI it satisfies the weapon's 8 AGI
// requirement without assigned points.
(function weaponSetMembership() {
    const resolved = buildStatMaps({
        items: ['Bony Circlet', ...NONE_NAMES.slice(1)],
        weapon: 'Bony Bow',
    });
    if (resolved.error) {
        t.assert(false, `weapon set membership: ${resolved.error}`);
        return;
    }

    const result = calculate_skillpoints(resolved.equipSMs, resolved.weaponSM, 200);
    t.assert(result !== null, 'weapon set membership: Bony pair is feasible');
    if (result) {
        t.assert(result.length === 5,
            `weapon set membership: expected complete five-field result, got ${result.length}`);
        t.assert(result[3].get('Bony') === 2,
            `weapon set membership: expected Bony count 2, got ${result[3].get('Bony')}`);
        t.assert(result[0][4] === 0,
            `weapon set membership: expected 0 assigned AGI, got ${result[0][4]}`);
        t.assert(result[1][4] === 14,
            `weapon set membership: expected 14 final AGI, got ${result[1][4]}`);
        t.assert(result[4][4] === 14,
            `weapon set membership: expected 14 item/set AGI, got ${result[4][4]}`);
        const setStats = ctx.createBaseStatmap(22);
        ctx.applySetBonuses(setStats, result[3], sets);
        t.assert(setStats.get('mdRaw') === 45,
            `weapon set membership: expected +45 set mdRaw, got ${setStats.get('mdRaw')}`);
    }

    // Crafted/custom weapons may retain a set label in their map, but game
    // rules exclude crafted items from set membership.
    const craftedWeapon = new Map(resolved.weaponSM);
    craftedWeapon.set('crafted', true);
    const craftedResult = calculate_skillpoints(resolved.equipSMs, craftedWeapon, 200);
    t.assert(craftedResult !== null, 'crafted weapon set exclusion: build is feasible');
    if (craftedResult) {
        t.assert(craftedResult[3].get('Bony') === 1,
            `crafted weapon set exclusion: expected Bony count 1, got ${craftedResult[3].get('Bony')}`);
        t.assert(craftedResult[0][4] === 8,
            `crafted weapon set exclusion: expected 8 assigned AGI, got ${craftedResult[0][4]}`);
        t.assert(craftedResult[4][4] === 6,
            `crafted weapon set exclusion: expected no set AGI (6 weapon only), got ${craftedResult[4][4]}`);
    }
})();

// ── Closure fast-path differential ──────────────────────────────────────────
//
// calculate_skillpoints has a Lodestone-style closure fast path that skips
// the activation-order backtracking when every ordering item has nonnegative
// SP bonuses and the closure activates everything at assign = post_floor.
// Verify it against a second sandbox whose skillpoints.js has the fast path
// disabled (pure backtracking) over seeded random builds from real items.
(function closureDifferential() {
    const vm = require('vm');
    const REPO = path.join(__dirname, '..', '..', '..');
    const spPath = path.join(REPO, 'js', 'game', 'skillpoints.js');
    const src = fs.readFileSync(spPath, 'utf8');
    const GUARD = 'if (!closure_solved) _bt(0, 0, 0);';
    if (!src.includes(GUARD)) {
        t.assert(false, 'closure differential: guard line not found in skillpoints.js');
        return;
    }
    // Re-evaluate a backtracking-only copy inside the same sandbox under a
    // different name so it shares game data (sets, SP_PER_ATTR_CAP, ...).
    const patched = src
        .replace(GUARD, '_bt(0, 0, 0);')
        .replace('function calculate_skillpoints(', 'function calculate_skillpoints_bt_only(');
    vm.runInContext(patched, ctx, { filename: 'skillpoints_bt_only.js' });
    const calc_bt_only = ctx.calculate_skillpoints_bt_only;

    // Deterministic LCG so failures are reproducible.
    let seed = 0xC0FFEE;
    const rand = () => (seed = (seed * 1664525 + 1013904223) >>> 0) / 0x100000000;
    const pickR = (arr) => arr[Math.floor(rand() * arr.length)];

    // Per-slot pools of real items (SP-relevant items favored: keep items
    // with any req or skillpoint, plus the NONE item).
    const poolByType = {};
    for (const [name, item] of itemMap) {
        if (item.category === 'weapon') continue;
        const hasSP = (item.reqs ?? []).some(r => r > 0) || (item.skillpoints ?? []).some(s => s !== 0);
        if (!hasSP) continue;
        (poolByType[item.type] ??= []).push(name);
    }
    const weaponPool = [];
    for (const [name, item] of itemMap) {
        if (['bow', 'wand', 'dagger', 'spear', 'relik'].includes(item.type)) weaponPool.push(name);
    }

    const CASES = 300;
    let mismatches = 0;
    let closure_cases = 0;
    for (let c = 0; c < CASES; c++) {
        const names = [];
        for (let s = 0; s < 8; s++) {
            const pool = poolByType[SLOT_TYPES[s]] ?? [];
            // ~20% NONE to vary k.
            names.push(rand() < 0.2 || pool.length === 0 ? NONE_NAMES[s] : pickR(pool));
        }
        const weaponName = pickR(weaponPool);
        const tc = { items: names, weapon: weaponName };
        const resolved = buildStatMaps(tc);
        if (resolved.error) continue;

        const budget = 200;
        const a = calculate_skillpoints(resolved.equipSMs, resolved.weaponSM, budget);
        const b = calc_bt_only(resolved.equipSMs, resolved.weaponSM, budget);

        const aKey = a === null ? 'null' : JSON.stringify([Array.from(a[0]), Array.from(a[1]), a[2]]);
        const bKey = b === null ? 'null' : JSON.stringify([Array.from(b[0]), Array.from(b[1]), b[2]]);
        if (aKey !== bKey) {
            mismatches++;
            if (mismatches <= 3) {
                console.error(`  DIFF case ${c}: items=${JSON.stringify(names)} weapon=${weaponName}`);
                console.error(`    fast=${aKey}`);
                console.error(`    bt  =${bKey}`);
            }
        }
        if (a !== null) closure_cases++;
    }
    t.assert(mismatches === 0,
        `closure differential: ${CASES} seeded builds, ${mismatches} mismatches (${closure_cases} feasible)`);
})();

const { fail } = t.summary();
if (fail > 0) process.exit(1);
