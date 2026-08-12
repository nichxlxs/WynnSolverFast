'use strict';

const PHASES = [
    'enum_stat', 'enum_sp', 'enum_bound',
    'precheck', 'sp', 'finalize', 'greedy', 'assemble', 'threshold', 'mana', 'score', 'topn',
];

function mergeTraceMetrics(...metrics) {
    const result = {};
    for (const metric of metrics) {
        if (!metric) continue;
        for (const [key, value] of Object.entries(metric)) {
            if (typeof value === 'number') result[key] = (result[key] ?? 0) + value;
        }
    }
    return result;
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
    return {
        wallMs,
        leafCount: metric.leaf_count ?? 0,
        accountedMs,
        unaccountedMs,
        unaccountedPct: wallMs ? unaccountedMs / wallMs * 100 : 0,
        phases,
    };
}

module.exports = { PHASES, mergeTraceMetrics, summarizeTraceMetrics };
