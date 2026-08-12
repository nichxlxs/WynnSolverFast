'use strict';

const { createSandbox, TestRunner } = require('./harness');
const ctx = createSandbox();
const t = new TestRunner('Incremental stats');

const item = new Map([
    ['maxRolls', new Map([['damPct', 12], ['hp', 999], ['mr', -2]])],
    ['hp', 250], ['eDef', -15], ['str', 4],
]);
const running = new Map([['damPct', 5], ['hp', 1000], ['eDef', 2], ['str', 0], ['mr', 3]]);

ctx._incr_add_item(running, item);
t.assert(running.get('damPct') === 17, 'rolled stat is added');
t.assert(running.get('hp') === 1250, 'static stat uses direct value exactly once');
t.assert(running.get('eDef') === -13 && running.get('str') === 4, 'static values are added');
t.assert(running.get('mr') === 1, 'negative roll is added');

ctx._incr_remove_item(running, item);
t.assert(running.get('damPct') === 5 && running.get('hp') === 1000, 'remove reverses add');
t.assert(running.get('eDef') === 2 && running.get('str') === 0 && running.get('mr') === 3,
    'all values return to their original state');

const entries1 = ctx._get_incr_entries(item);
const entries2 = ctx._get_incr_entries(item);
t.assert(entries1 === entries2, 'compiled additive entries are cached per statMap');
t.assert(entries1.keys.length === entries1.values.length, 'compiled keys and values stay aligned');
t.assert(entries1.values[entries1.keys.indexOf('hp')] === 250, 'compiled entries retain static stats');
t.assert(Object.isFrozen(entries1) && Object.isFrozen(entries1.keys) && Object.isFrozen(entries1.values),
    'compiled entries are immutable');

const compact = ctx._init_running_stats_compact(new Map([['hp', 1000], ['damPct', 5]]));
ctx._incr_add_item(compact, item);
t.assert(compact.hp === 1250 && compact.damPct === 17 && compact.mr === -2,
    'compact running stats receive the same additions');
ctx._incr_remove_item(compact, item);
t.assert(compact.hp === 1000 && compact.damPct === 5 && compact.mr === 0,
    'compact running stats exactly reverse additions');

const materialized = ctx._materialize_running_statmap(compact, new Map());
t.assert(materialized.get('hp') === 1000 && materialized.get('damPct') === 5,
    'compact running stats materialize to a statMap');

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
module.exports = summary;
