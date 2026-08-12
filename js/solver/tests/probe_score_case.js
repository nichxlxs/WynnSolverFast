#!/usr/bin/env node
// Debug probe for the Rust score-kernel differential (P2.4 layer 1).
//
// Usage: node js/solver/tests/probe_score_case.js <fixture.json> <case_idx>
//
// Recomputes one exported score case's per-part damage values through the
// same JS pure functions the worker used, printing full-precision numbers to
// diff against score_kernel's SCORE_KERNEL_DEBUG_CASE output.

'use strict';

const fs = require('fs');
const { createSandbox, loadGameData } = require('./harness');

const fixturePath = process.argv[2];
const caseIdx = parseInt(process.argv[3] ?? '0', 10);

const ctx = createSandbox();
loadGameData(ctx);

const fixture = JSON.parse(fs.readFileSync(fixturePath, 'utf8'));

// Inverse of the exporter's _jser encoding.
function unser(v) {
    if (v && typeof v === 'object') {
        if (Array.isArray(v)) return v.map(unser);
        if ('__m' in v) {
            const m = new Map();
            for (const [k, x] of Object.entries(v.__m)) m.set(k, unser(x));
            return m;
        }
        if ('__s' in v) return new Set(v.__s.map(unser));
        const o = {};
        for (const [k, x] of Object.entries(v)) o[k] = unser(x);
        return o;
    }
    return v;
}

const weapon_sm = unser(fixture.weapon_sm);
const parsed_combo = unser(fixture.parsed_combo);
const boost_registry = unser(fixture.boost_registry);
const c = fixture.cases[caseIdx];
if (!c) { console.error('no such case'); process.exit(1); }
const combo_base = unser(c.combo_base);

const crit = ctx.skillPointsToPercentage(combo_base.get('dex') || 0);
console.log(`case ${caseIdx} crit=${crit.toExponential(17)}`);

for (let ri = 0; ri < parsed_combo.length; ri++) {
    const row = parsed_combo[ri];
    if (!row.spell || row.qty <= 0 || row.pseudo) continue;
    const { stats } = ctx.apply_combo_row_boosts(combo_base, row.boost_tokens, boost_registry, null);
    // prop overrides skipped in the probe (no tokens with prop bonuses in
    // these fixtures — score_kernel handles them; diffs would show anyway).
    const full = ctx.computeSpellDisplayFull(stats, weapon_sm, row.spell, crit);
    if (full) {
        for (const p of full.parts) {
            console.log(`  row ${ri} part "${p.name}" norm=[${p.normal_total[0].toExponential(17)},${p.normal_total[1].toExponential(17)}] crit=[${p.crit_total[0].toExponential(17)},${p.crit_total[1].toExponential(17)}]`);
        }
    }
    const avg = ctx.computeSpellDisplayAvg(stats, weapon_sm, row.spell, crit);
    console.log(`  row ${ri} per_cast=${avg.toExponential(17)}`);
}
