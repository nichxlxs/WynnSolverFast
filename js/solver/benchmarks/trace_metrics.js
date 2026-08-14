'use strict';

const PHASES = ['precheck', 'sp', 'finalize', 'ceiling', 'greedy', 'assemble', 'threshold', 'mana', 'score', 'topn'];
const TRACE_SCHEMA_VERSION = 3;
const TRACE_V3_COUNTERS = [
    'recursion_calls', 'leaf_evaluator_calls', 'credited_leaves',
    'illegal_rejects', 'sp_bound_rejects', 'restriction_bound_rejects',
    'precheck_rejects', 'initial_sp_rejects',
    'sp_exact_calls', 'sp_exact_rejects',
    'threshold_rejects', 'mana_rejects', 'ceiling_rejects',
    'scored_leaves',
];

function mergeTraceMetrics(...metrics) {
    const result = {};
    for (const metric of metrics) {
        if (!metric) continue;
        for (const [key, value] of Object.entries(metric)) {
            if (typeof value !== 'number') continue;
            // A schema identifier describes the merged object; summing it
            // across workers would turn two v3 traces into the fictional v6.
            if (key === 'schema_version') result[key] = Math.max(result[key] ?? 0, value);
            else result[key] = (result[key] ?? 0) + value;
        }
    }
    return result;
}

/**
 * Reconcile v3's exhaustive-search funnel.  These identities are deliberately
 * independent of phase timings, which can overlap wall time across workers.
 */
function reconcileTraceMetrics(metric) {
    const schemaVersion = metric?.schema_version ?? 0;
    if (schemaVersion < TRACE_SCHEMA_VERSION) {
        return {
            schemaVersion,
            applicable: false,
            credited: { ok: true, expected: null, observed: null, delta: null },
            evaluator: { ok: true, terminal: null, observed: null, remainder: null },
            spExact: { ok: true, calls: null, rejects: null },
        };
    }

    const observedCredited = metric.credited_leaves ?? 0;
    const expectedCredited = (metric.illegal_rejects ?? 0)
        + (metric.sp_bound_rejects ?? 0)
        + (metric.restriction_bound_rejects ?? 0)
        + (metric.leaf_evaluator_calls ?? 0);
    const creditedDelta = observedCredited - expectedCredited;

    // These are mutually exclusive leaf-level exits. Threshold/mana and
    // candidate SP rejects intentionally do not appear here: tome optimisation
    // can try several candidates inside one evaluator call. A non-negative
    // remainder is the number of evaluator calls whose candidate loop found no
    // winner after those candidate-level outcomes.
    const terminalLeaves = (metric.precheck_rejects ?? 0)
        + (metric.initial_sp_rejects ?? 0)
        + (metric.ceiling_rejects ?? 0)
        + (metric.scored_leaves ?? 0);
    const evaluatorCalls = metric.leaf_evaluator_calls ?? 0;
    const evaluatorRemainder = evaluatorCalls - terminalLeaves;

    const spCalls = metric.sp_exact_calls ?? 0;
    const spRejects = metric.sp_exact_rejects ?? 0;
    return {
        schemaVersion,
        applicable: true,
        credited: {
            ok: creditedDelta === 0,
            expected: expectedCredited,
            observed: observedCredited,
            delta: creditedDelta,
        },
        evaluator: {
            ok: evaluatorRemainder >= 0,
            terminal: terminalLeaves,
            observed: evaluatorCalls,
            remainder: evaluatorRemainder,
        },
        spExact: { ok: spRejects <= spCalls, calls: spCalls, rejects: spRejects },
    };
}

function summarizeTraceMetrics(metric) {
    metric = metric ?? {};
    const wallMs = metric.wall_ms ?? 0;
    const phases = PHASES.map(name => {
        const totalMs = metric[`${name}_ms`] ?? 0;
        const calls = metric[`${name}_calls`] ?? 0;
        return {
            name,
            calls,
            totalMs,
            wallPct: wallMs ? totalMs / wallMs * 100 : 0,
            avgUs: calls ? totalMs / calls * 1000 : 0,
        };
    }).sort((a, b) => b.totalMs - a.totalMs);
    const accountedMs = phases.reduce((sum, phase) => sum + phase.totalMs, 0);
    const unaccountedMs = Math.max(0, wallMs - accountedMs);
    return {
        schemaVersion: metric.schema_version ?? 0,
        wallMs,
        leafCount: metric.leaf_count ?? 0,
        counters: Object.fromEntries(TRACE_V3_COUNTERS.map(key => [key, metric[key] ?? 0])),
        reconciliation: reconcileTraceMetrics(metric),
        accountedMs,
        unaccountedMs,
        unaccountedPct: wallMs ? unaccountedMs / wallMs * 100 : 0,
        phases,
    };
}

module.exports = {
    PHASES, TRACE_SCHEMA_VERSION, TRACE_V3_COUNTERS,
    mergeTraceMetrics, reconcileTraceMetrics, summarizeTraceMetrics,
};
