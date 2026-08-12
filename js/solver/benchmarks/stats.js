'use strict';

function summarize(samples) {
    if (!Array.isArray(samples) || samples.length === 0 || samples.some(v => !Number.isFinite(v))) {
        throw new TypeError('samples must be a non-empty array of finite numbers');
    }
    const sorted = [...samples].sort((a, b) => a - b);
    const count = sorted.length;
    const middle = Math.floor(count / 2);
    const median = count % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
    const mean = sorted.reduce((sum, value) => sum + value, 0) / count;
    const variance = sorted.reduce((sum, value) => sum + (value - mean) ** 2, 0) / count;
    return {
        count,
        min: sorted[0],
        max: sorted[count - 1],
        median,
        p90: sorted[Math.ceil(count * 0.9) - 1],
        mean,
        stddev: Math.sqrt(variance),
    };
}

function relativeChange(baseline, current) {
    if (!Number.isFinite(baseline) || !Number.isFinite(current) || baseline === 0) return null;
    return (current - baseline) / baseline * 100;
}

module.exports = { summarize, relativeChange };
