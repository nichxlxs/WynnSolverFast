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

const indexed = ctx._init_running_stats_indexed(
    new Map([['hp', 1000], ['damPct', 5]]), [item],
);
ctx._incr_add_item(indexed, item);
t.assert(indexed.values[indexed.index.get('hp')] === 1250, 'indexed stats add static values');
t.assert(indexed.values[indexed.index.get('damPct')] === 17, 'indexed stats add rolled values');
ctx._incr_remove_item(indexed, item);
const indexedMap = ctx._materialize_running_statmap(indexed, new Map());
t.assert(indexedMap.get('hp') === 1000 && indexedMap.get('damPct') === 5,
    'indexed stats reverse and materialize exactly');
t.assert(ctx._get_indexed_incr_entries(indexed, item) === ctx._get_indexed_incr_entries(indexed, item),
    'numeric item indices are cached for the search registry');

const compiledIndexed = ctx._compile_indexed_item(indexed, item);
ctx._incr_add_indexed(indexed, compiledIndexed);
t.assert(indexed.values[indexed.index.get('hp')] === 1250,
    'precompiled numeric entries add without a statMap lookup');
ctx._incr_remove_indexed(indexed, compiledIndexed);
t.assert(indexed.values[indexed.index.get('hp')] === 1000,
    'precompiled numeric entries reverse exactly');

const progressState = { next: 5000 };
t.assert(!ctx._take_progress_checkpoint(4999, progressState),
    'progress checkpoint stays idle below its threshold');
t.assert(ctx._take_progress_checkpoint(5500, progressState),
    'progress checkpoint fires when subtree pruning jumps over its threshold');
t.assert(progressState.next === 10000,
    'progress checkpoint advances without per-leaf modulo');

const runningMax = new Int32Array([10, 20, 30, 40, 50]);
const maxStack = new Int32Array(10);
ctx._push_running_max(runningMax, new Int32Array([5, 25, 35, 1, 60]), maxStack, 1);
t.assert([...runningMax].join(',') === '10,25,35,40,60',
    'running maxima update at placement');
ctx._pop_running_max(runningMax, maxStack, 1);
t.assert([...runningMax].join(',') === '10,20,30,40,50',
    'running maxima restore in constant time at backtracking');

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
module.exports = summary;
