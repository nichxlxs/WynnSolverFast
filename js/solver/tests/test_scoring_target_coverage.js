// Guards that every scoring target the ENGINE supports is one a user can
// actually SELECT, and that the two target lists in the UI stay consistent.
//
// This exists because `total_hp` was a first-class scoring target for the whole
// life of the project — six committed snapshots optimize it, including all
// three README benchmarks (`readme_armor2`, `readme_armor4`, `readme_rings2`),
// and both the JS and Rust engines score it correctly via `eval_indirect_stat`
// — while the solver page's Target dropdown never offered it. The engine and
// the regression corpus agreed on a target the UI could not produce, so no
// user could run the very searches the project benchmarks itself on.
//
// Nothing caught it because the snapshot tests drive the solver headlessly:
// they set `scoring_target` directly from the snapshot JSON, which is exactly
// the path that worked. Only comparing the two lists finds the gap.
//
// Run: node js/solver/tests/test_scoring_target_coverage.js

'use strict';

const fs = require('fs');
const path = require('path');
const { TestRunner, REPO_ROOT } = require('./harness');

const t = new TestRunner('Scoring target coverage');

// ── The three lists ──────────────────────────────────────────────────────────

/** `<option value=...>` of the Target select on the solver page. */
function dropdownTargets() {
    const html = fs.readFileSync(path.join(REPO_ROOT, 'solver', 'index.html'), 'utf8');
    const sel = html.match(/<select[^>]*id="solver-target"[\s\S]*?<\/select>/);
    if (!sel) return null;
    return [...sel[0].matchAll(/<option\s+value="([^"]+)"/g)].map((m) => m[1]);
}

/** CUSTOM_WEIGHT_TARGETS keys, read from the source rather than executed. */
function customWeightTargets() {
    const src = fs.readFileSync(path.join(REPO_ROOT, 'js', 'solver', 'restrictions.js'), 'utf8');
    const arr = src.match(/const CUSTOM_WEIGHT_TARGETS = \[([\s\S]*?)\];/);
    if (!arr) return null;
    return [...arr[1].matchAll(/key:\s*'([^']+)'/g)].map((m) => m[1]);
}

/** Every scoring_target appearing in the committed snapshot corpus. */
function snapshotTargets() {
    const dir = path.join(__dirname, 'snapshots');
    const out = new Map();   // target -> [snapshot names]
    for (const f of fs.readdirSync(dir).filter((x) => x.endsWith('.snap.json'))) {
        let d;
        try { d = JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8')); } catch { continue; }
        const tgt = d.scoring_target;
        if (!tgt) continue;
        if (!out.has(tgt)) out.set(tgt, []);
        out.get(tgt).push(f.replace('.snap.json', ''));
    }
    return out;
}

const dropdown = dropdownTargets();
const custom = customWeightTargets();
const snaps = snapshotTargets();

t.assert(Array.isArray(dropdown) && dropdown.length > 0,
    'the solver page must expose a #solver-target select with options');
t.assert(Array.isArray(custom) && custom.length > 0,
    'CUSTOM_WEIGHT_TARGETS must be parseable from restrictions.js');
t.assert(snaps.size > 0, 'the snapshot corpus must declare scoring targets');

// ── 1. Every benchmarked target must be selectable ───────────────────────────
// This is the assertion that fails on the original bug.
const dd = new Set(dropdown || []);
for (const [target, users] of [...snaps].sort()) {
    t.assert(dd.has(target),
        `scoring target '${target}' is used by ${users.length} snapshot(s) `
        + `(${users.slice(0, 3).join(', ')}${users.length > 3 ? ', …' : ''}) but is not `
        + `offered by the Target dropdown, so no user can run that search`);
}

// ── 2. Every selectable target must be blendable ─────────────────────────────
// A target good enough to optimize alone is good enough to weight in a blend;
// letting the two lists drift is how one of them ends up missing an entry.
const cw = new Set(custom || []);
for (const target of (dropdown || []).filter((x) => x !== 'custom')) {
    t.assert(cw.has(target),
        `Target dropdown offers '${target}' but CUSTOM_WEIGHT_TARGETS does not, `
        + `so it cannot be used in a custom blend`);
}

// ── 3. The custom-weight list is append-only ─────────────────────────────────
// Solver URLs encode each weight's target as a 4-bit index into this array
// (build_encode.js), so reordering silently rewrites existing shared links and
// more than 16 entries cannot be encoded at all.
const CUSTOM_WEIGHT_PREFIX = [
    'combo_damage', 'ehp', 'ehpr', 'total_healing',
    'spd', 'poison', 'lb', 'xpb',
];
t.assert(
    JSON.stringify((custom || []).slice(0, CUSTOM_WEIGHT_PREFIX.length))
        === JSON.stringify(CUSTOM_WEIGHT_PREFIX),
    'CUSTOM_WEIGHT_TARGETS must keep its existing entries in order — solver URLs '
    + 'encode the target as an index into it, so reordering rewrites shared links');
t.assert((custom || []).length <= 16,
    `CUSTOM_WEIGHT_TARGETS must fit the 4-bit URL field (<=16 entries, has ${(custom || []).length})`);

// ── 4. Selectable targets must be understood by BOTH engines ─────────────────
// The Rust engine routes anything that is not combo_damage/total_healing/custom
// through eval_indirect_stat; a derived target missing from its match arm would
// silently fall through to `stats.num_or0(name)` and score 0 for every build —
// indistinguishable from a legitimately-zero stat.
const rustSrc = fs.readFileSync(
    path.join(REPO_ROOT, 'rust', 'sp_kernel', 'src', 'scoring.rs'), 'utf8');
const rustIndirect = rustSrc.match(
    /pub fn eval_indirect_stat[\s\S]*?\n\}/);
const jsSrc = fs.readFileSync(
    path.join(REPO_ROOT, 'js', 'solver', 'pure', 'engine.js'), 'utf8');
const jsIndirect = jsSrc.match(/function eval_indirect_stat[\s\S]*?\n\}/);

// Derived targets are the ones that MUST be dispatched explicitly; plain
// statMap keys (spd, poison, lb, xpb) legitimately use the fallback.
const DERIVED = ['ehp', 'ehp_no_agi', 'total_hp', 'hpr', 'ehpr', 'total_mana'];
for (const target of (dropdown || [])) {
    if (!DERIVED.includes(target)) continue;
    t.assert(!!rustIndirect && rustIndirect[0].includes(`"${target}"`),
        `Rust eval_indirect_stat must handle derived target '${target}' — without an `
        + `explicit arm it falls through to a raw stat lookup and scores 0`);
    t.assert(!!jsIndirect && jsIndirect[0].includes(`'${target}'`),
        `JS eval_indirect_stat must handle derived target '${target}'`);
}

t.summary();
