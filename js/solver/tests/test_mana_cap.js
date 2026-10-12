// Max mana cap: the pool (base 100, Max Mana IDs and the Int bonus) caps at
// MAX_MANA_CAP = 400 (official wiki, Identifying: "Max Mana caps at 400,
// including the bonuses provided by Intelligence and Base Mana (100)"). Every
// solver site reads it through total_mana_pool, so the mana simulators, the
// total_mana requirement/objective and the displays agree.
// Run: node js/solver/tests/test_mana_cap.js

'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');
const { createSandbox, TestRunner } = require('./harness');

const t = new TestRunner('Max mana cap');
const ctx = createSandbox();
const run = (src) => vm.runInContext(src, ctx);

t.assert(run('MAX_MANA_CAP') === 400, 'the cap is 400');
t.assert(run('total_mana_pool(50, 40)') === 190, 'below the cap: 100 + Max Mana + Int bonus');
t.assert(run('total_mana_pool(284, 80)') === 400, 'Hydrotoxemia + Space Dust at 150 Int (464) caps at 400');

// Int 150 gives the largest Int bonus; with 284 Max Mana the pool is capped.
const stats = `new Map([['maxMana', 284], ['int', 150], ['mr', 0], ['ms', 0], ['hp', 10000]])`;
t.assert(run(`threshold_stat_value(${stats}, 'total_mana', {})`) === 400, 'total_mana as a requirement reads the capped pool');
t.assert(run(`eval_indirect_stat(${stats}, 'total_mana')`) === 400, 'total_mana as an objective reads the capped pool');
const hc = 'DEFAULT_HEALTH_CONFIG';
t.assert(run(`simulate_combo_mana_fast([], ${stats}, ${hc}, false, [], null).start_mana`) === 400,
    'the fast mana simulator starts (and tops out) at the cap');
t.assert(run(`simulate_combo_mana_hp([], ${stats}, ${hc}, false, [], null).start_mana`) === 400,
    'the full mana simulator starts at the cap');

// No solver site may compute the pool by hand.
const files = ['../pure/simulate.js', '../pure/engine.js', '../engine/item_priority.js',
               '../graph/stat.js', '../combo/node.js'];
for (const f of files) {
    const src = fs.readFileSync(path.join(__dirname, f), 'utf8');
    t.assert(!/100 \+ (item_mana|mm|maxMana) \+ int_mana|100 \+ int_mana \+ item_mana/.test(src),
        `${path.basename(f)} reads the pool through total_mana_pool`);
}
// The Rust engine receives the same value.
const bridge = fs.readFileSync(path.join(__dirname, '../engine/rust_bridge.js'), 'utf8');
t.assert(bridge.includes('max_mana_cap: MAX_MANA_CAP'), 'the Rust tables carry MAX_MANA_CAP');

const summary = t.summary();
if (summary.fail > 0) process.exit(1);
