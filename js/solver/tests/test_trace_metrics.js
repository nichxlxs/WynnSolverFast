'use strict';

const { TestRunner } = require('./harness');
const {
    TRACE_SCHEMA_VERSION, mergeTraceMetrics, reconcileTraceMetrics,
    summarizeTraceMetrics,
} = require('../benchmarks/trace_metrics');

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

// ── Schema 3: funnel reconciliation ──────────────────────────────────────────

t.assert(mergeTraceMetrics(
    { schema_version: 3, credited_leaves: 10 },
    { schema_version: 3, credited_leaves: 5 },
).schema_version === 3, 'schema version is not summed when merging workers');

// A balanced funnel: 1000 credited leaves leave the search at exactly one
// place, and each of the 100 real evaluations ends at one leaf-level terminal.
const balanced = {
    schema_version: 3,
    credited_leaves: 1000,
    illegal_rejects: 300, sp_bound_rejects: 500, restriction_bound_rejects: 100,
    leaf_evaluator_calls: 100,
    precheck_rejects: 60, initial_sp_rejects: 20, ceiling_rejects: 15,
    scored_leaves: 5,
};
const good = reconcileTraceMetrics(balanced);
t.assert(good.applicable && good.ok, 'a balanced v3 funnel reconciles');
t.assert(good.credited.delta === 0, 'credited leaves match their attributions');
t.assert(good.evaluator.remainder === 0, 'evaluator calls match their terminals');

// The failure this exists to catch: leaves credited without attribution.
const unattributed = reconcileTraceMetrics({ ...balanced, credited_leaves: 1200 });
t.assert(!unattributed.ok && unattributed.credited.delta === 200,
    'unattributed credited leaves are reported, not silently absorbed');

// More terminals than evaluator calls means a leaf was counted twice.
const overcounted = reconcileTraceMetrics({ ...balanced, scored_leaves: 25 });
t.assert(!overcounted.ok && overcounted.evaluator.remainder < 0,
    'double-counted leaf terminals produce a negative remainder');

// The tome loop legitimately leaves evaluator calls with no winning candidate.
const noWinner = reconcileTraceMetrics({ ...balanced, scored_leaves: 0 });
t.assert(noWinner.ok && noWinner.evaluator.remainder === 5,
    'evaluator calls with no winning candidate are a positive remainder, not an error');

const legacy = reconcileTraceMetrics({ leaf_count: 10, sp_calls: 1 });
t.assert(!legacy.applicable && legacy.ok,
    'a pre-v3 trace is reported as not applicable rather than failing');
t.assert(TRACE_SCHEMA_VERSION === 3, 'schema version is exported for producers');

// Per-evaluation cost must not be computed from the credited total.
const perLeaf = summarizeTraceMetrics({ ...balanced, wall_ms: 200 });
t.assert(perLeaf.evaluatorCalls === 100 && perLeaf.creditedPerEvaluation === 10,
    'credited leaves per real evaluation is exposed');
t.assert(perLeaf.usPerEvaluation === 2000,
    'per-evaluation time divides by evaluations, not by credited leaves');

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
