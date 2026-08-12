'use strict';

const { summarize, relativeChange } = require('../benchmarks/stats');
const { TestRunner } = require('./harness');

const t = new TestRunner('Benchmark statistics');

const summary = summarize([5, 1, 3, 2, 4]);
t.assert(summary.count === 5, 'count is reported');
t.assert(summary.min === 1 && summary.max === 5, 'range is reported');
t.assert(summary.median === 3, 'odd median is exact');
t.assert(summary.p90 === 5, 'p90 uses nearest-rank sampling');
t.assert(Math.abs(summary.mean - 3) < 1e-12, 'mean is exact');
t.assert(Math.abs(summary.stddev - Math.sqrt(2)) < 1e-12, 'population deviation is exact');

const even = summarize([4, 2]);
t.assert(even.median === 3, 'even median averages central values');
t.assert(relativeChange(100, 125) === 25, 'relative change reports percentage');
t.assert(relativeChange(0, 1) === null, 'zero baseline has no relative change');

let emptyRejected = false;
try { summarize([]); } catch (err) { emptyRejected = err instanceof TypeError; }
t.assert(emptyRejected, 'empty samples are rejected');

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
