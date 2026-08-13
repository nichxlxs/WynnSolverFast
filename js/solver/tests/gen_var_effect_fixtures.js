// Differential fixtures for the Rust port of atree_eval_stat_effects
// (js/solver/pure/utils.js:163) — the "split" scaling plan's variable half.
//
// The exporter refuses to lower an ability tree whose variable effects write
// a dotted key (damMult.X), a multiplier-map root, or a key the constant
// partition already wrote — it emits scaling_kind:'full', which the Rust
// engine rejects (support matrix A8). But the JS routes those outputs
// through merge_stat, Rust's eval_var_effects does the same, and the dense
// lowering already declines them (nested_prefix), so the Obj path handles
// them. These cases prove that rather than assuming it.
//
// Run: node js/solver/tests/gen_var_effect_fixtures.js
// Out: rust/sp_kernel/fixtures/var_effect_cases.json

'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');
const { createSandbox, REPO_ROOT } = require('./harness');

const ctx = createSandbox();
const inCtx = (expr) => vm.runInContext(`(${expr})`, ctx);

// atree_eval_stat_effects takes the *unlowered* effect shape and an
// atree_merged for translate lookups. The exporter lowers scaling factors
// and prop inputs ahead of time; with a numeric-only tree, translate is the
// identity, so a lowered effect maps 1:1 onto an unlowered one and the two
// sides compare directly.
const atree_eval_stat_effects = inCtx('atree_eval_stat_effects');

/**
 * @param {Array} outputs  stat names the effect writes
 * @param {Array} inputs   [{stat, factor}] read from pre-scale stats
 */
function effect(outputs, inputs, opts = {}) {
    return {
        js: {
            scaling: inputs.map((i) => i.factor),
            inputs: inputs.map((i) => ({ type: 'stat', name: i.stat })),
            output: outputs.map((name) => ({ type: 'stat', name })),
            ...(opts.round !== undefined ? { round: opts.round } : {}),
            ...(opts.positive !== undefined ? { positive: opts.positive } : {}),
            ...(opts.max !== undefined ? { max: opts.max } : {}),
        },
        rust: {
            terms: inputs.map((i) => ({ stat: i.stat, factor: i.factor })),
            const_add: 0,
            outputs,
            round: opts.round ?? true,
            positive: opts.positive ?? true,
            ...(opts.max !== undefined ? { max: opts.max } : {}),
        },
    };
}

const cases = [];
const add = (name, stats, effects) => cases.push({ name, stats, effects });

// ── Plain stat outputs (the shape already shipped) ──────────────────────────
add('plain stat output', { str: 40, dex: 25 },
    [effect(['sdPct'], [{ stat: 'str', factor: 0.5 }])]);

add('two inputs, rounding down', { str: 41, int: 17 },
    [effect(['mdPct'], [{ stat: 'str', factor: 0.5 }, { stat: 'int', factor: 0.25 }])]);

add('negative total clamped by positive', { str: -80 },
    [effect(['sdPct'], [{ stat: 'str', factor: 1 }])]);

add('negative total kept when positive=false', { str: -80 },
    [effect(['sdPct'], [{ stat: 'str', factor: 1 }], { positive: false })]);

add('round=false keeps the fraction', { str: 41 },
    [effect(['sdPct'], [{ stat: 'str', factor: 0.5 }], { round: false })]);

add('positive max cap', { str: 400 },
    [effect(['sdPct'], [{ stat: 'str', factor: 1 }], { max: 100 })]);

add('negative max cap', { str: -400 },
    [effect(['sdPct'], [{ stat: 'str', factor: 1 }], { max: -100, positive: false })]);

add('two effects accumulate on one key', { str: 40, dex: 60 },
    [effect(['sdPct'], [{ stat: 'str', factor: 0.5 }]),
     effect(['sdPct'], [{ stat: 'dex', factor: 0.25 }])]);

// ── The outputs the exporter currently refuses (A8 triggers 2 and 3) ────────
add('dotted damMult output', { str: 40 },
    [effect(['damMult.Surge'], [{ stat: 'str', factor: 0.5 }])]);

add('two dotted outputs on the same map', { str: 40, dex: 80 },
    [effect(['damMult.Surge'], [{ stat: 'str', factor: 0.5 }]),
     effect(['damMult.Focus'], [{ stat: 'dex', factor: 0.25 }])]);

add('dotted outputs accumulate on one sub-key', { str: 40, dex: 80 },
    [effect(['damMult.Surge'], [{ stat: 'str', factor: 0.5 }]),
     effect(['damMult.Surge'], [{ stat: 'dex', factor: 0.25 }])]);

add('non-stacking sub-key takes the max (higher second)', { str: 20, dex: 90 },
    [effect(['damMult.Potion'], [{ stat: 'str', factor: 1 }]),
     effect(['damMult.Potion'], [{ stat: 'dex', factor: 1 }])]);

add('non-stacking sub-key takes the max (lower second)', { str: 90, dex: 20 },
    [effect(['damMult.Potion'], [{ stat: 'str', factor: 1 }]),
     effect(['damMult.Potion'], [{ stat: 'dex', factor: 1 }])]);

add('Vulnerability is non-stacking too', { str: 30, dex: 70 },
    [effect(['defMult.Vulnerability'], [{ stat: 'str', factor: 1 }]),
     effect(['defMult.Vulnerability'], [{ stat: 'dex', factor: 1 }])]);

add('defMult / healMult / manaMult roots', { str: 40 },
    [effect(['defMult.Guard'], [{ stat: 'str', factor: 0.5 }]),
     effect(['healMult.Mend'], [{ stat: 'str', factor: 0.25 }]),
     effect(['manaMult.Flow'], [{ stat: 'str', factor: 0.125 }])]);

add('mixed plain and dotted outputs from one effect', { str: 40 },
    [effect(['sdPct', 'damMult.Surge'], [{ stat: 'str', factor: 0.5 }])]);

add('dotted output with max cap and clamp', { str: -200, dex: 900 },
    [effect(['damMult.Surge'], [{ stat: 'str', factor: 1 }]),
     effect(['damMult.Blaze'], [{ stat: 'dex', factor: 1 }], { max: 250 })]);

// ── Serialize ───────────────────────────────────────────────────────────────

// Rust represents a nested map as { __m: { ... } }; flatten both sides to
// dotted paths so the comparison does not depend on that encoding.
function flatten(map, prefix, out) {
    for (const [k, v] of map) {
        if (v instanceof Map) flatten(v, prefix ? `${prefix}.${k}` : k, out);
        else out[prefix ? `${prefix}.${k}` : k] = v;
    }
    return out;
}

const out = { cases: [] };
for (const c of cases) {
    const pre = new Map(Object.entries(c.stats));
    const res = atree_eval_stat_effects(
        new Map(), c.effects.map((e) => e.js), pre, new Map());
    out.cases.push({
        name: c.name,
        stats: c.stats,
        var_effects: c.effects.map((e) => e.rust),
        expected: flatten(res, '', {}),
    });
}

const dest = path.join(REPO_ROOT, 'rust', 'sp_kernel', 'fixtures', 'var_effect_cases.json');
fs.writeFileSync(dest, JSON.stringify(out, null, 1));
console.log(`wrote ${out.cases.length} cases -> ${path.relative(REPO_ROOT, dest)}`);
