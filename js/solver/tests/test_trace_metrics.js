'use strict';

const { TestRunner } = require('./harness');
const { mergeTraceMetrics, summarizeTraceMetrics } = require('../benchmarks/trace_metrics');

const t = new TestRunner('Trace metrics');
const merged = mergeTraceMetrics(
    { leaf_count: 10, sp_calls: 4, sp_ms: 8, score_calls: 2, score_ms: 3 },
    { leaf_count: 5, sp_calls: 3, sp_ms: 6, score_calls: 1, score_ms: 2 },
);
t.assert(merged.leaf_count === 15 && merged.sp_calls === 7, 'counters merge');
t.assert(merged.sp_ms === 14 && merged.score_ms === 5, 'timings merge');

const summary = summarizeTraceMetrics({
    wall_ms: 100,
    leaf_count: 20,
    precheck_calls: 20, precheck_ms: 10,
    sp_calls: 10, sp_ms: 40,
    finalize_calls: 8, finalize_ms: 16,
    greedy_calls: 8, greedy_ms: 20,
    assemble_calls: 8, assemble_ms: 4,
    threshold_calls: 8, threshold_ms: 2,
    mana_calls: 6, mana_ms: 3,
    score_calls: 5, score_ms: 5,
    topn_calls: 5, topn_ms: 0.5,
});
t.assert(summary.phases[0].name === 'sp', 'phases sort by descending total time');
t.assert(summary.phases.find(p => p.name === 'sp').wallPct === 40, 'wall percentage is calculated');
t.assert(summary.phases.find(p => p.name === 'greedy').avgUs === 2500, 'average microseconds are calculated');
t.assert(summary.accountedMs === 100.5, 'accounted phase time is totaled');
t.assert(summary.unaccountedMs === 0, 'negative instrumentation remainder clamps to zero');

const partial = summarizeTraceMetrics({ wall_ms: 100, sp_calls: 1, sp_ms: 40 });
t.assert(partial.unaccountedMs === 60 && partial.unaccountedPct === 60,
    'enumeration and other uninstrumented time is exposed');

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
