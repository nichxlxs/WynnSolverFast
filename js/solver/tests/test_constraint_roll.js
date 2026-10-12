// R15 roll-robust requirements: which stats a requirement reads, which way
// is worse for it, and the values items are baked at.
// Run: node js/solver/tests/test_constraint_roll.js

'use strict';

const vm = require('vm');
const { createSandbox, TestRunner } = require('./harness');

const t = new TestRunner('Constraint Roll (R15)');
const ctx = createSandbox();
const run = (src) => vm.runInContext(src, ctx);

// 1. Directions.
const dirs = (r, mana) => JSON.stringify([...run(`constraint_roll_dirs(${JSON.stringify(r)}, ${mana})`)].sort());
t.assert(dirs({ stat_thresholds: [{ stat: 'ehp', op: 'ge', value: 1 }] }, false) === '[["hpBonus",1]]',
    'an EHP floor reads hpBonus, higher is better');
t.assert(dirs({ stat_thresholds: [{ stat: 'finalSpellCost2', op: 'le', value: 30 }] }, false)
    === '[["spPct2",-1],["spRaw2",-1]]', 'a spell-cost cap wants lower spRaw and spPct');
t.assert(dirs({ stat_thresholds: [{ stat: 'mr', op: 'ge', value: 1 }, { stat: 'mr', op: 'le', value: 9 }] }, false) === '[]',
    'a stat wanted both ways is left at its group roll');
t.assert(dirs({ soft_floors: [{ stat: 'total_mana', value: 200 }] }, false) === '[["maxMana",1]]',
    'soft floors count, total mana reads maxMana');
const m = JSON.parse(dirs({}, true));
t.assert(m.length === 11 && m.some(([k, d]) => k === 'mr' && d === 1) && m.some(([k, d]) => k === 'spRaw3' && d === -1),
    'the mana check reads mr, ms, maxMana (higher better) and spell costs (lower better)');
t.assert(dirs({ stat_thresholds: [{ stat: 'ms', op: 'ge', value: 20 }] }, true).includes('["ms",1]'),
    'a mana-steal floor and the mana check agree on ms');

// 2. Values: conservative toward the worse end, objective stats untouched.
run(`current_roll_mode = { damage: 85, mana: 100, healing: 85, misc: 85 };
     current_constraint_roll = { pct: 0, dirs: new Map([['hpBonus', 1], ['spRaw1', -1]]) };`);
const rv = (min, max, k) => run(`getRolledValue(${min}, ${max}, '${k}')`);
t.assert(rv(800, 1200, 'hpBonus') === 800, 'a floored stat at 0% takes its low extreme');
t.assert(rv(-5, -2, 'spRaw1') === -2, 'a capped stat at 0% takes its high extreme (the costlier spell)');
t.assert(rv(10, 20, 'sdPct') === Math.round(10 + 0.85 * 10), 'an unconstrained stat keeps its group roll');
run(`current_constraint_roll.pct = 50;`);
t.assert(rv(800, 1200, 'hpBonus') === 1000 && rv(-5, -2, 'spRaw1') === Math.round(-2 - 0.5 * 3),
    'at 50% both directions meet in the middle');
t.assert(run('_allRollsMax()') === false, 'items are rolled while a constraint roll is active');
run(`current_constraint_roll = null; current_roll_mode = { damage: 100, mana: 100, healing: 100, misc: 100 };`);
t.assert(run('_allRollsMax()') === true && rv(800, 1200, 'hpBonus') === 1200, 'off: max rolls, no change');

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
module.exports = summary;
