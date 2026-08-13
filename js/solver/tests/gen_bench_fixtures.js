// Benchmark-only score fixtures for scenario shapes the exported corpus does
// not contain: a declared buff-state slider (the dynamic-row path, matrix
// A6/B9) and custom blends with negative weights (the two-sided ceiling, B2).
//
// The performance work on both of those was measured against fixtures that
// existed only as scratch files, which made the headline numbers
// unreproducible from a clean checkout. These are the same fixtures, derived
// here instead so the edit is thirty lines of visible code rather than a
// megabyte of near-duplicate JSON.
//
// Each is `score_spell2.json` — a real export, so the item pools, atree,
// combo and tables are all genuine — plus one small edit. They pair with the
// checked-in `enum_spell2.txt`.
//
// THESE ARE NOT ORACLES. `cases[]` is emptied deliberately. The array carries
// per-case `expected_damage` and `combo_base` captured from a JS run of the
// *unedited* scenario, so against an edited one those numbers are simply
// wrong — the earlier scratch fixtures kept them and `score_kernel` duly
// reported "0 exact / 96 diff", which looks like a failing engine and is
// really a stale expectation. Correctness for these shapes is established
// elsewhere and against the JS: `mana_sim_check` for the simulation and the
// composed per-leaf chain, `SCORE_DENSE_CHECK=1` for dense against Obj on
// every trial, and `SCORE_DENSE=0` for the whole Obj path. What these
// fixtures are for is timing.
//
// Run: node js/solver/tests/gen_bench_fixtures.js
// Out: rust/sp_kernel/fixtures/score_{slider,hpcast,hpcast_2m,blend_pos,blend_neg,blend_mixed}.json

'use strict';

const fs = require('fs');
const path = require('path');
const { REPO_ROOT } = require('./harness');

const FIXTURES = path.join(REPO_ROOT, 'rust', 'sp_kernel', 'fixtures');
const BASE = path.join(FIXTURES, 'score_spell2.json');
// The ceiling-gate benchmark needs a search big enough for a gate to prune;
// spell2's 961 combinations finish before it matters, spell_wide is ~39.4bn.
const WIDE_PATH = path.join(FIXTURES, 'score_spell_wide.json');
// And one that runs to completion, so gated and ungated top-15s are
// comparable — 3.1M combinations, combo_damage.
const GAIA2M_PATH = path.join(FIXTURES, 'score_gaia2m.json');

// `fixtures/` is gitignored — everything in it is a derived artifact — so on a
// clean checkout the base has to be exported first. Say which command rather
// than throwing ENOENT at someone who has no reason to know.
if (!fs.existsSync(BASE)) {
    console.error(
        `missing ${path.relative(REPO_ROOT, BASE)}\n\n`
        + 'These fixtures are derived from an exported one. Export it first:\n\n'
        + '  SOLVER_EXPORT_RUST=rust/sp_kernel/fixtures/enum_spell2.txt \\\n'
        + '  SOLVER_EXPORT_SCORE=rust/sp_kernel/fixtures/score_spell2.json \\\n'
        + '    node js/solver/tests/test_solver_search.js <scenario>\n\n'
        + 'then re-run this script. See rust/sp_kernel/README.md.');
    process.exit(1);
}

const base = JSON.parse(fs.readFileSync(BASE, 'utf8'));
const load = (p) => (fs.existsSync(p) ? JSON.parse(fs.readFileSync(p, 'utf8')) : null);
const WIDE = load(WIDE_PATH);
const GAIA2M = load(GAIA2M_PATH);

/** A copy of a base fixture with no per-case expectations. See the header. */
function blank(from = base, name = 'score_spell2.json') {
    const f = JSON.parse(JSON.stringify(from));
    f.cases = [];
    f.meta = { ...f.meta, cases: 0, attempts: 0, derived_from: name };
    return f;
}

const out = [];

// ── Dynamic rows: a buff state that declares a slider (A6/B9) ───────────────
//
// A declared `slider_name` is the JS `has_dyn` condition: the simulation's
// per-leaf state value is injected into the damage rows as a boost token, so
// the rows stop being parse-time constants. The registry entry is what those
// injected tokens resolve against, and it deliberately spans all three bonus
// shapes the dense lowering has to handle — a plain stat, a second plain
// stat, and a `damMult` sub-key.
{
    const f = blank();
    f.meta.scoring_target = 'combo_damage';
    f.meta.note = 'benchmark only — dynamic rows via a declared buff-state slider';
    f.boost_registry = [...f.boost_registry, {
        name: 'Surge',
        stat_bonuses: [
            { key: 'damRaw', value: 2, round: true },
            { key: 'sdPct', value: 1, round: true },
            { key: 'damMult.Surge', value: 0.5, round: false },
        ],
        prop_bonuses: [],
    }];
    f.layer2.health_config = {
        hp_casting: false,
        health_cost: 0,
        damage_boost: null,
        buff_states: [{
            state_name: 'Surged',
            activate_on: { spell: 1 },
            slider_name: 'Surge',
            duration: 120,
            value_cap: 50,
            drain_pct_per_second: { mana: 1 },
            tracking: 'mana_loss',
        }],
        exit_triggers: [],
    };
    out.push(['score_slider.json', f]);
}

// ── HP-cost casting with no declared slider (B4) ────────────────────────────
//
// Blood Pact pays a spell's unaffordable mana from health. `damage_boost`
// with `slider_name: null` is the shape that matters for the ceiling gate:
// the simulation still computes a blood-pact bonus, but with no slider to
// inject it under, `inject_blood_pact_boosts` adds nothing and the damage
// rows are unchanged. So damage is NOT sim-coupled here, even though the
// build is HP-casting — which is what makes the all-150-SP ceiling still an
// upper bound. Declaring a slider would make it coupled, and that case is
// caught separately by `dynamic.is_some()`.
//
// Two of them, because the gate needs two different things proved. Whether
// it is ADMISSIBLE has to be shown on a search that runs to completion, so
// gated and ungated top-15s are comparable — `gaia2m`, 3.1M combinations,
// combo_damage. Whether it is WORTH anything needs a search big enough for
// pruning to matter — `spell_wide`, ~39.4 billion. spell2's 961 finish
// before either question arises.
for (const [name, src, base_name] of [
    ['score_hpcast_2m.json', GAIA2M, 'score_gaia2m.json'],
    ['score_hpcast.json', WIDE, 'score_spell_wide.json'],
]) {
    if (!src) {
        console.warn(`skipping ${name} — fixtures/${base_name} not exported`);
        continue;
    }
    const f = blank(src, base_name);
    f.meta.scoring_target = 'combo_damage';
    f.meta.note = 'benchmark only — HP-cost casting, no declared slider';
    f.layer2.hp_casting = true;
    f.layer2.health_config = {
        hp_casting: true,
        health_cost: 30,
        damage_boost: { min: 5, max: 20, slider_name: null },
        buff_states: [],
        exit_triggers: [],
    };
    out.push([name, f]);
}

// ── Two-sided ceiling: blends whose weights differ in sign (B2) ─────────────
//
// The score-ceiling gate assembles at all-150 SP, which maximizes a
// positively-weighted term but *minimizes* a negatively-weighted one — so an
// all-150 assemble understates a negative term and the ceiling stops being an
// upper bound. The two-sided ceiling reads each term at its own extreme.
// Three weightings so the on/off equivalence is checked where the fix does
// something (neg, mixed) and where it must change nothing (pos).
const BLENDS = [
    ['score_blend_pos.json', 'all weights non-negative — the two-sided ceiling must be a no-op',
        [{ target: 'combo_damage', weight: 1.0 }, { target: 'total_hp', weight: 50.0 }]],
    ['score_blend_neg.json', 'a single negative weight',
        [{ target: 'total_hp', weight: -1.0 }]],
    ['score_blend_mixed.json', 'both signs — the shape the gate was unsound on',
        [{ target: 'combo_damage', weight: 1.0 }, { target: 'total_hp', weight: -50.0 }]],
];
for (const [name, note, weights] of BLENDS) {
    const f = blank();
    f.meta.scoring_target = 'custom';
    f.meta.note = `benchmark only — ${note}`;
    f.layer2.custom_weights = weights;
    out.push([name, f]);
}

// ── Write ───────────────────────────────────────────────────────────────────
for (const [name, f] of out) {
    const dest = path.join(FIXTURES, name);
    fs.writeFileSync(dest, JSON.stringify(f));
    const kb = Math.round(fs.statSync(dest).size / 1024);
    console.log(`wrote ${path.relative(REPO_ROOT, dest)} (${kb} KB)`);
}
console.log(`\n${out.length} benchmark fixtures, all pairing with fixtures/enum_spell2.txt`);
