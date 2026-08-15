'use strict';

const PHASES = ['precheck', 'sp', 'finalize', 'ceiling', 'greedy', 'assemble', 'threshold', 'mana', 'score', 'topn'];

// Schema 3 (see worker.js) separates recursion nodes, real evaluator calls and
// credited tuple-space totals, which schema 2 reported as one `checked` number.
const TRACE_SCHEMA_VERSION = 3;
const TRACE_V3_COUNTERS = [
    'recursion_calls', 'leaf_evaluator_calls', 'credited_leaves',
    'illegal_rejects', 'sp_bound_rejects', 'restriction_bound_rejects',
    'precheck_rejects', 'initial_sp_rejects',
    'ceiling_rejects', 'threshold_rejects', 'mana_rejects',
    'scored_leaves',
];

function mergeTraceMetrics(...metrics) {
    const result = {};
    for (const metric of metrics) {
        if (!metric) continue;
        for (const [key, value] of Object.entries(metric)) {
            if (typeof value !== 'number') continue;
            // The schema identifier describes the merged object. Summing it
            // across eight workers would turn a v3 trace into a v24.
            if (key === 'schema_version') result[key] = Math.max(result[key] ?? 0, value);
            else result[key] = (result[key] ?? 0) + value;
        }
    }
    return result;
}

/**
 * Check schema-3's funnel identities.
 *
 * Two independent identities, neither derived from phase timings (those can
 * overlap in wall time across workers and so cannot be reconciled):
 *
 *   credited  every credited leaf left the search at exactly one place — an
 *             illegal-set block, an SP bound prune, a restriction bound prune,
 *             or a real evaluator call. A non-zero delta means a counter is
 *             double-counting or a code path credits leaves without attribution.
 *
 *   evaluator every evaluator call ends at one of four leaf-level terminals.
 *             Threshold and mana rejects are deliberately excluded: the tome
 *             loop can try several candidates inside one evaluator call, so
 *             those are candidate-level, not leaf-level. The remainder is
 *             therefore evaluator calls whose candidate loop produced no
 *             winner, and only a NEGATIVE remainder indicates a bug.
 */
function reconcileTraceMetrics(metric) {
    const schemaVersion = metric?.schema_version ?? 0;
    if (schemaVersion < TRACE_SCHEMA_VERSION) {
        return {
            schemaVersion,
            applicable: false,
            ok: true,
            credited: { ok: true, expected: null, observed: null, delta: null },
            evaluator: { ok: true, terminal: null, observed: null, remainder: null },
        };
    }

    const observedCredited = metric.credited_leaves ?? 0;
    const expectedCredited = (metric.illegal_rejects ?? 0)
        + (metric.sp_bound_rejects ?? 0)
        + (metric.restriction_bound_rejects ?? 0)
        + (metric.leaf_evaluator_calls ?? 0);
    const creditedDelta = observedCredited - expectedCredited;

    const terminalLeaves = (metric.precheck_rejects ?? 0)
        + (metric.initial_sp_rejects ?? 0)
        + (metric.ceiling_rejects ?? 0)
        + (metric.scored_leaves ?? 0);
    const evaluatorCalls = metric.leaf_evaluator_calls ?? 0;
    const evaluatorRemainder = evaluatorCalls - terminalLeaves;

    const credited = {
        ok: creditedDelta === 0,
        expected: expectedCredited,
        observed: observedCredited,
        delta: creditedDelta,
    };
    const evaluator = {
        ok: evaluatorRemainder >= 0,
        terminal: terminalLeaves,
        observed: evaluatorCalls,
        remainder: evaluatorRemainder,
    };
    return {
        schemaVersion,
        applicable: true,
        ok: credited.ok && evaluator.ok,
        credited,
        evaluator,
    };
}

function summarizeTraceMetrics(metric) {
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
    const evaluatorCalls = metric.leaf_evaluator_calls ?? metric.leaf_count ?? 0;
    const creditedLeaves = metric.credited_leaves ?? 0;
    return {
        wallMs,
        leafCount: metric.leaf_count ?? 0,
        // Per-evaluation cost must divide by evaluations, not by credited
        // leaves: whole pruned subtrees are credited without being visited, so
        // dividing by the credited total understates real per-leaf cost by
        // whatever factor the bounds are currently achieving.
        evaluatorCalls,
        creditedLeaves,
        creditedPerEvaluation: evaluatorCalls ? creditedLeaves / evaluatorCalls : 0,
        usPerEvaluation: evaluatorCalls ? wallMs / evaluatorCalls * 1000 : 0,
        accountedMs,
        unaccountedMs,
        unaccountedPct: wallMs ? unaccountedMs / wallMs * 100 : 0,
        phases,
        reconciliation: reconcileTraceMetrics(metric),
    };
}

module.exports = {
    PHASES,
    TRACE_SCHEMA_VERSION,
    TRACE_V3_COUNTERS,
    mergeTraceMetrics,
    reconcileTraceMetrics,
    summarizeTraceMetrics,
};
