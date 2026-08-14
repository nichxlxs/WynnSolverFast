'use strict';

const { TestRunner } = require('./harness');
const {
    mergeTraceMetrics, reconcileTraceMetrics, summarizeTraceMetrics,
} = require('../benchmarks/trace_metrics');

const t = new TestRunner('Trace metrics');
const merged = mergeTraceMetrics(
    { leaf_count: 10, sp_calls: 4, sp_ms: 8, score_calls: 2, score_ms: 3 },
    { leaf_count: 5, sp_calls: 3, sp_ms: 6, score_calls: 1, score_ms: 2 },
);
t.assert(merged.leaf_count === 15 && merged.sp_calls === 7, 'counters merge');
t.assert(merged.sp_ms === 14 && merged.score_ms === 5, 'timings merge');

const mergedV3 = mergeTraceMetrics(
    { schema_version: 3, credited_leaves: 40 },
    { schema_version: 3, credited_leaves: 60 },
);
t.assert(mergedV3.schema_version === 3, 'schema versions merge by max, not addition');
t.assert(mergedV3.credited_leaves === 100, 'v3 counters add across workers');

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

const traceV3 = {
    schema_version: 3,
    credited_leaves: 100,
    illegal_rejects: 10,
    sp_bound_rejects: 20,
    restriction_bound_rejects: 30,
    leaf_evaluator_calls: 40,
    precheck_rejects: 5,
    initial_sp_rejects: 6,
    ceiling_rejects: 7,
    scored_leaves: 8,
    sp_exact_calls: 25,
    sp_exact_rejects: 9,
};
const reconciled = reconcileTraceMetrics(traceV3);
t.assert(reconciled.applicable && reconciled.credited.ok && reconciled.credited.delta === 0,
    'v3 credited leaves reconcile to bounds plus evaluator calls');
t.assert(reconciled.evaluator.ok && reconciled.evaluator.remainder === 14,
    'v3 leaf exits leave a non-negative candidate-level remainder');
t.assert(reconciled.spExact.ok, 'v3 exact-SP rejects cannot exceed calls');
t.assert(summarizeTraceMetrics(traceV3).reconciliation.credited.ok,
    'trace summary carries reconciliation evidence');

const brokenV3 = reconcileTraceMetrics({ ...traceV3, credited_leaves: 99 });
t.assert(!brokenV3.credited.ok && brokenV3.credited.delta === -1,
    'v3 reconciliation detects a missing credited leaf');

const legacy = reconcileTraceMetrics({ leaf_count: 1 });
t.assert(!legacy.applicable && legacy.credited.ok,
    'legacy traces remain readable without inventing v3 failures');

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
