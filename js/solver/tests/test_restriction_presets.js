// Playstyle presets (roadmap R18): the table matches the validated family
// suite, every stat resolves in the restriction catalogue, and applying a
// preset replaces only the stat rows with exactly its floors.
// Run: node js/solver/tests/test_restriction_presets.js

'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');
const { createSandbox, TestRunner } = require('./harness');

const t = new TestRunner('Restriction Presets');
const ctx = createSandbox();

// A minimal DOM: the restriction container, rows built by restriction_add_row
// (its innerHTML is parsed only for the three controls the code reads), and
// the blacklist container that must be left alone.
function makeEl(tag) {
    const el = {
        tagName: tag, children: [], dataset: {}, value: '', id: '', title: '', style: {}, className: '',
        classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
        addEventListener() {}, setAttribute() {}, getAttribute() { return null; },
        appendChild(c) { c.parentNode = el; el.children.push(c); return c; },
        remove() { const p = el.parentNode; if (p) p.children.splice(p.children.indexOf(el), 1); },
        querySelector(sel) { return el.querySelectorAll(sel)[0] ?? null; },
        querySelectorAll(sel) {
            const out = [];
            const walk = (n) => { for (const c of n.children) { if (matches(c, sel)) out.push(c); walk(c); } };
            walk(el);
            return out;
        },
        set innerHTML(html) {
            el.children = [];
            if (html.includes('restr-stat-input')) {
                const stat = makeEl('input'); stat.className = 'restr-stat-input';
                const op = makeEl('select'); op.value = 'ge';
                const val = makeEl('input'); val.className = 'restr-value-input';
                for (const c of [stat, op, val]) el.appendChild(c);
            }
        },
    };
    return el;
}
function matches(n, sel) {
    if (sel === 'select') return n.tagName === 'select';
    if (sel.startsWith('.')) return n.className.split(' ').includes(sel.slice(1));
    const m = sel.match(/^\[id\^="(.+)"\]$/);
    return m ? (n.id ?? '').startsWith(m[1]) : false;
}
const rows = makeEl('div');
const blacklist = makeEl('div');
blacklist.appendChild(makeEl('div')).id = 'bl-row-1';
ctx.document = {
    getElementById: (id) => (id === 'restriction-rows' ? rows : id === 'blacklist-rows' ? blacklist : null),
    createElement: makeEl,
};
ctx._schedule_solver_hash_update = () => {};
ctx.autoComplete = function () {};   // the stat-name dropdown widget
const src = fs.readFileSync(path.join(__dirname, '..', 'restrictions.js'), 'utf8');
vm.runInContext(src, ctx, { filename: 'restrictions.js' });
const presets = vm.runInContext('SOLVER_RESTRICTION_PRESETS', ctx);
const stats = vm.runInContext('RESTRICTION_STATS', ctx);

// 1. Same families and floors as the suite the seeds were validated against.
const suite = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'benchmarks', 'family_suite.json'), 'utf8'));
t.assert(presets.length === suite.families.length, `one preset per suite family (${presets.length})`);
for (const fam of suite.families) {
    const p = presets.find(x => x.id === fam.family);
    t.assert(!!p && JSON.stringify(p.restrictions) === JSON.stringify(fam.success_restrictions)
        && p.seed_weapon === fam.core_weapon,
        `${fam.family}: preset floors equal the suite's success_restrictions`);
}

// 2. Every stat resolves, so no floor is silently dropped on apply.
for (const p of presets) {
    for (const r of p.restrictions) {
        t.assert(stats.some(s => s.key === r.stat) && (r.op === 'ge' || r.op === 'le'),
            `${p.id}: ${r.stat} ${r.op} is a known restriction`);
    }
}

// 3. Applying replaces the stat rows, keeps the blacklist, and is idempotent.
vm.runInContext('restriction_add_row()', ctx);       // a user row to be replaced
for (const p of presets) {
    const added = vm.runInContext(`solver_apply_restriction_preset(${JSON.stringify(p.id)})`, ctx);
    const got = rows.children.map(r => ({
        stat: r.querySelector('.restr-stat-input').dataset.statKey,
        op: r.querySelector('select').value,
        value: Number(r.querySelector('.restr-value-input').value),
    }));
    t.assert(added === p.restrictions.length && JSON.stringify(got) === JSON.stringify(p.restrictions),
        `${p.id}: apply leaves exactly the preset's ${p.restrictions.length} rows`);
}
t.assert(blacklist.children.length === 1, 'applying presets leaves blacklist rows alone');
t.assert(vm.runInContext("solver_apply_restriction_preset('no_such_family')", ctx) === 0
    && rows.children.length === presets.at(-1).restrictions.length,
    'an unknown preset id changes nothing');

const summary = t.summary();
if (require.main === module) {
    if (summary.fail > 0) process.exit(1);
}
module.exports = summary;
