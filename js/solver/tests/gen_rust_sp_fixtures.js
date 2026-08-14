#!/usr/bin/env node
// Export seeded skillpoint fixtures for the Rust SP kernel prototype.
//
// Usage: node js/solver/tests/gen_rust_sp_fixtures.js [count] [outfile]
//
// Line format (all integers, space separated):
//   budget
//   then 9 units (8 equipment in wynn order, then weapon):
//     crafted r0 r1 r2 r3 r4 s0 s1 s2 s3 s4
//   then set_free_bonus: b0..b4   (set-bonus SP folded into the free pool)
//   then expected: feas assign0..4 final0..4 total_assigned
//     (feas=0 => infeasible; the remaining 11 fields are zeros)
//
// The expected values come from the JS calculate_skillpoints on the same
// inputs, so the Rust kernel must reproduce them bit-for-bit.

'use strict';

const fs = require('fs');
const path = require('path');
const { createSandbox, loadGameData } = require('./harness');

const count = parseInt(process.argv[2] ?? '500', 10);
const outPath = process.argv[3]
    ?? path.join(__dirname, '..', '..', '..', 'rust', 'sp_kernel', 'fixtures', 'sp_cases.txt');

const ctx = createSandbox();
const { itemMap, sets, none_items } = loadGameData(ctx);
const expandItem = ctx.expandItem;
const skp_order = ctx.skp_order;

const SLOT_TYPES = ['helmet', 'chestplate', 'leggings', 'boots', 'ring', 'ring', 'bracelet', 'necklace'];
const NONE_NAMES = [
    'No Helmet', 'No Chestplate', 'No Leggings', 'No Boots',
    'No Ring 1', 'No Ring 2', 'No Bracelet', 'No Necklace',
];
const WYNN_ORDER = [3, 2, 1, 0, 4, 5, 6, 7];

let seed = 0xBADC0DE;
const rand = () => (seed = (seed * 1664525 + 1013904223) >>> 0) / 0x100000000;
const pickR = (arr) => arr[Math.floor(rand() * arr.length)];

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

const lines = [];
for (let c = 0; c < count; c++) {
    const equipSMs = [];
    for (let s = 0; s < 8; s++) {
        const pool = poolByType[SLOT_TYPES[s]] ?? [];
        const name = (rand() < 0.2 || pool.length === 0)
            ? NONE_NAMES[s] : pickR(pool);
        const raw = (name === NONE_NAMES[s]) ? none_items[s] : itemMap.get(name);
        equipSMs.push(expandItem(raw));
    }
    const weaponSM = expandItem(itemMap.get(pickR(weaponPool)));
    const wynnSMs = WYNN_ORDER.map(i => equipSMs[i]);
    const budget = 200;

    const result = ctx.calculate_skillpoints(wynnSMs, weaponSM, budget);

    // Recompute the set-bonus SP contribution exactly as the JS does, so the
    // Rust kernel can fold it into free_bonus without needing the sets table.
    const set_counts = new Map();
    for (const sm of wynnSMs) {
        if (sm.get('crafted')) continue;
        const set_name = sm.get('set');
        if (set_name) set_counts.set(set_name, (set_counts.get(set_name) ?? 0) + 1);
    }
    if (!weaponSM.get('crafted')) {
        const set_name = weaponSM.get('set');
        if (set_name) set_counts.set(set_name, (set_counts.get(set_name) ?? 0) + 1);
    }
    const set_free = [0, 0, 0, 0, 0];
    for (const [set_name, cnt] of set_counts) {
        const bonus = sets.get(set_name).bonuses[cnt - 1];
        for (const i in skp_order) {
            set_free[i] += (bonus[skp_order[i]] || 0);
        }
    }

    const parts = [budget];
    for (const sm of [...wynnSMs, weaponSM]) {
        parts.push(sm.get('crafted') ? 1 : 0);
        parts.push(...sm.get('reqs'));
        parts.push(...sm.get('skillpoints'));
    }
    parts.push(...set_free);
    if (result === null) {
        parts.push(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    } else {
        parts.push(1, ...Array.from(result[0]), ...Array.from(result[1]), result[2]);
    }
    lines.push(parts.join(' '));
}

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, lines.join('\n') + '\n');
console.log(`Wrote ${lines.length} fixtures to ${outPath}`);
