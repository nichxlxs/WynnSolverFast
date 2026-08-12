'use strict';

const { TestRunner } = require('./harness');
const { compareTopResult, tryInsertTopResult } = require('../engine/top_results');

const t = new TestRunner('Top results');
const result = (score, names) => ({ score, item_names: names });

t.assert(compareTopResult(result(11, ['b']), result(10, ['a'])) < 0,
    'higher scores sort first');
t.assert(compareTopResult(result(10, ['a']), result(10, ['b'])) < 0,
    'equal scores use canonical item-name order');

const top = [];
let allocations = 0;
for (const candidate of [
    result(10, ['z']), result(30, ['b']), result(20, ['c']), result(5, ['x']),
    result(30, ['a']), result(25, ['d']),
]) {
    tryInsertTopResult(top, candidate.score, () => {
        allocations++;
        return candidate;
    }, 3);
}

t.assert(top.length === 3, 'buffer respects its capacity');
t.assert(top.map(r => r.score).join(',') === '30,30,25', 'best scores remain');
t.assert(top[0].item_names[0] === 'a' && top[1].item_names[0] === 'b',
    'ties have deterministic order');
t.assert(allocations === 5, 'strictly noncompetitive score avoids allocation');

let rejectedFactoryCalled = false;
const inserted = tryInsertTopResult(top, 1, () => {
    rejectedFactoryCalled = true;
    return result(1, ['never']);
}, 3);
t.assert(!inserted && !rejectedFactoryCalled, 'rejected candidates stay allocation-free');

let invalidRejected = false;
try { tryInsertTopResult([], 1, () => result(1, []), 0); }
catch (err) { invalidRejected = err instanceof RangeError; }
t.assert(invalidRejected, 'invalid capacity is rejected');

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
module.exports = summary;
