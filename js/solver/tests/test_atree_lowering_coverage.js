// Guards that every shipped ability tree can be LOWERED for the Rust engine
// (support matrix A8).
//
// rust_bridge.js classifies a build's atree scaling into 'cached' (no
// stat-dependent effects), 'split' (a constant partition plus a list of
// variable effects the Rust side re-evaluates per leaf), or 'full'. 'full'
// means "outside the supported subset" — the Rust engine refuses the fixture
// and the search silently falls back to the JS engine, i.e. ~650x slower with
// no visible reason.
//
// Three conditions still produce 'full':
//
//   1. var_has_prop_io — a variable effect reads or writes an ability
//      *property*. Property mutations feed back into other effects' translate
//      lookups, so there is no partition that can be summed separately.
//   2. a variable effect whose output is a bare multiplier root (damMult with
//      no sub-key), which has no sub-key to reason about.
//   3. a variable key the constant partition also writes, where the
//      contributions are not all integral. The full pass interleaves const and
//      var contributions in effect order while the split sums each partition
//      apart, and float addition is not associative (test_scaling_association).
//
// None of the three occurs in any ability tree Wynncraft has shipped. This
// test asserts that over EVERY data version in the repo, so the claim stays
// true rather than being a note about the day it was checked. If a future data
// update introduces one, this fails and names the class and ability — which is
// the signal to do the porting work, and the place to start looking.
//
// Method. For each class it builds the all-nodes ability tree — every node
// active at once, which no real build is — and classifies that. All three
// conditions are monotone in the node set: each needs a specific effect (or
// pair of effects) to be *present*, so if the superset is clean, every real
// subset of it is too.
//
// It calls the real `atree_collect_stat_effects` rather than reimplementing
// the classification, so the test cannot drift away from the code it guards.
//
// Run: node js/solver/tests/test_atree_lowering_coverage.js

'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');
const { createSandbox, TestRunner, REPO_ROOT } = require('./harness');

const t = new TestRunner('Atree Lowering Coverage (A8)');

const ctx = createSandbox();
const atree_collect_stat_effects = vm.runInContext('(atree_collect_stat_effects)', ctx);

const MULT_ROOTS = ['damMult', 'defMult', 'healMult', 'manaMult'];

/**
 * The keys the CONSTANT partition can write, for a whole class at once.
 *
 * Mirrors which branches of `atree_compute_scaling` reach `apply_bonus` with a
 * 'stat' bonus: a `raw_stat` effect's stat bonuses only when it is toggled
 * (the untoggled branch skips them), and any `stat_scaling` effect that is not
 * a variable one. Taking every toggle as on and every node as active makes
 * this a superset of what any real build's `const_scaled` can contain.
 */
function constWrittenKeys(abilities) {
    const keys = new Set();
    for (const abil of abilities) {
        for (const effect of abil.effects ?? []) {
            if (effect.type === 'raw_stat') {
                if (!effect.toggle) continue;
                for (const bonus of effect.bonuses ?? []) {
                    if (bonus.type === 'stat') keys.add(bonus.name);
                }
                continue;
            }
            if (effect.type !== 'stat_scaling') continue;
            const isVar = !effect.slider
                && (effect.inputs ?? []).some((i) => i.type === 'stat');
            if (isVar) continue;
            if (!('output' in effect)) continue;
            const outs = Array.isArray(effect.output) ? effect.output : [effect.output];
            for (const o of outs) if (o && o.type === 'stat') keys.add(o.name);
        }
    }
    return keys;
}

/** Every ability of a class, keyed by id, in the file's order. */
function allNodesMerged(atrees, cls) {
    const merged = new Map();
    for (const abil of atrees[cls]) {
        // `base_abil` nodes append onto their base rather than standing alone;
        // appending is what a real merge does too, and for this test only the
        // union of effects matters.
        if ('base_abil' in abil && merged.has(abil.base_abil)) {
            const base = merged.get(abil.base_abil);
            base.effects = base.effects.concat(abil.effects ?? []);
            continue;
        }
        merged.set(abil.id, {
            ...abil,
            effects: [...(abil.effects ?? [])],
            properties: { ...(abil.properties ?? {}) },
        });
    }
    return merged;
}

const versions = fs.readdirSync(path.join(REPO_ROOT, 'data'))
    .filter((v) => fs.existsSync(path.join(REPO_ROOT, 'data', v, 'atree.json')))
    .sort();

t.assert(versions.length > 0, 'found at least one data version to scan');

const propIo = [];
const multRoot = [];
const collisions = [];
let classesScanned = 0;
let varEffects = 0;

for (const version of versions) {
    const atrees = JSON.parse(
        fs.readFileSync(path.join(REPO_ROOT, 'data', version, 'atree.json'), 'utf8'));
    for (const cls of Object.keys(atrees)) {
        if (!Array.isArray(atrees[cls]) || atrees[cls].length === 0) continue;
        classesScanned++;
        const merged = allNodesMerged(atrees, cls);
        const plan = atree_collect_stat_effects(merged);
        if (!plan) continue;
        varEffects += plan.var_effects.length;

        if (plan.var_has_prop_io) propIo.push(`${version}/${cls}`);

        const constKeys = constWrittenKeys(merged.values());
        for (const key of plan.var_keys) {
            if (MULT_ROOTS.includes(key)) {
                multRoot.push(`${version}/${cls}: ${key}`);
                continue;
            }
            const root = key.split('.')[0];
            if (constKeys.has(key) || (key.includes('.') && constKeys.has(root))) {
                collisions.push(`${version}/${cls}: ${key}`);
            }
        }
    }
}

t.assert(varEffects > 0,
    `the scan must actually see variable effects, or it proves nothing — found ${varEffects}`);

t.assert(propIo.length === 0,
    'no shipped ability tree may have a variable effect with prop I/O — '
    + `otherwise the Rust engine falls back to JS on it. Found: ${propIo.join(', ')}`);

t.assert(multRoot.length === 0,
    'no shipped ability tree may have a variable effect writing a bare multiplier '
    + `root. Found: ${multRoot.join(', ')}`);

t.assert(collisions.length === 0,
    'no shipped ability tree may have a variable key its constant partition also '
    + 'writes — a fractional contribution there makes the split and the full pass '
    + `disagree in the last bits. Found: ${collisions.join(', ')}`);

console.log(`  scanned ${versions.length} data versions, ${classesScanned} class trees, `
    + `${varEffects} variable effects`);

t.summary();
