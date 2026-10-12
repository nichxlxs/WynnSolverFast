// Radiance on item-granted skill points: the solver's stat assembly must
// apply it the way the builder's compute_radiance does, and both must read
// the same boost from the same toggles.
// Run: node js/solver/tests/test_radiance_item_sp.js

'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');
const { createSandbox, TestRunner } = require('./harness');

const t = new TestRunner('Radiance item SP');
const ctx = createSandbox();
const run = (src) => vm.runInContext(src, ctx);

// 1. One floor over the sum, like the builder. At boost 1 + 0.15, item 20
// gives 2.9999999999999982 and 14 + that is exactly 17.0 in a double, so the
// builder shows 17 where sp + floor(item * 0.15) would give 16.
const apply = (sp, item, boost) => JSON.stringify(run(`(() => {
    const m = new Map(skp_order.map((k, i) => [k, ${JSON.stringify(sp)}[i]]));
    _apply_radiance_item_sp(m, ${JSON.stringify(item)}, ${boost});
    return skp_order.map(k => m.get(k));
})()`));
t.assert(apply([14, 30, 50, 7, 0], [20, -10, 0, 7, 6], '1 + 0.15') === '[17,30,50,8,0]',
    'positive lanes gain floor(sp + item * (boost - 1)); negative and zero lanes are untouched');
t.assert(apply([14, 0, 0, 0, 0], [20, 0, 0, 0, 0], 1) === '[14,0,0,0,0]', 'no boost, no change');
t.assert(apply([14, 0, 0, 0, 0], null, 1.4) === '[14,0,0,0,0]', 'no item SP, no change');

// 2. The same as compute_radiance on an identical map (the builder path).
const builder = (sp, item, boost) => {
    const src = fs.readFileSync(path.join(__dirname, '../../game/shared_graph_nodes.js'), 'utf8');
    const body = src.slice(src.indexOf('    if (total_item_skillpoints) {'), src.indexOf('    return ret;\n}', src.indexOf('function compute_radiance')));
    return JSON.stringify(run(`(() => {
        const ret = new Map(skp_order.map((k, i) => [k, ${JSON.stringify(sp)}[i]]));
        const total_item_skillpoints = ${JSON.stringify(item)}; const boost = ${boost};
        ${body}
        return skp_order.map(k => ret.get(k));
    })()`));
};
for (const [sp, item, boost] of [
    [[14, 30, 50, 7, 0], [20, -10, 0, 7, 6], '1 + 0.15'],
    [[150, 98, 61, 0, 3], [44, 53, 12, 9, 1], '1 + 0.15 + 0.05'],
    [[-3, 135, 40, 22, 0], [8, 38, 17, 5, 2], '1.4'],
]) {
    t.assert(apply(sp, item, boost) === builder(sp, item, boost),
        `solver term equals the builder's at boost ${boost}`);
}

// 3. One boost: search.js must add what compute_radiance adds for each toggle.
const incs = (file, re) => {
    const src = fs.readFileSync(path.join(__dirname, file), 'utf8');
    return [...src.matchAll(re)].map(m => `${m[1]}${m[2]}`).join(',');
};
const solverIncs = incs('../engine/search.js', /getElementById\('([a-z]+)-boost'\)\?\.classList\.contains\('toggleOn'\)\) radiance_boost (\+= [0-9.]+|= [0-9.]+)/g);
const builderIncs = incs('../../game/shared_graph_nodes.js', /getElementById\('([a-z]+)-boost'\)\?\.classList\.contains\('toggleOn'\)\) \{ boost (\+= [0-9.]+|= [0-9.]+)/g);
t.assert(solverIncs.length > 0 && solverIncs === builderIncs,
    `search.js and compute_radiance read the same boosts (${solverIncs} vs ${builderIncs})`);
t.assert(solverIncs.includes('divinehonor+= 0.05'), 'Divine Honor adds 5%, as its toggle and the game data say');

const summary = t.summary();
if (summary.fail > 0) process.exit(1);
