'use strict';

function parseSolverOutput(output) {
    const progress = output.match(/checked:\s*(\d+),\s*feasible:\s*(\d+),\s*top5:\s*(\d+),\s*time:\s*(\d+)ms(\s*\(timed out\))?/);
    const score = output.match(/best score:\s*([\d.eE+-]+)/);
    if (!progress || !score) throw new Error('solver output did not contain a completed result');
    const poolSizes = {};
    const pool = output.match(/pool sizes:\s*\{([^}]*)\}/);
    if (pool) {
        for (const entry of pool[1].split(',')) {
            const match = entry.match(/\s*([\w]+):\s*(\d+)/);
            if (match) poolSizes[match[1]] = Number(match[2]);
        }
    }
    const checked = Number(progress[1]);
    const elapsedMs = Number(progress[4]);
    const combinations = output.match(/search combinations:\s*(\d+)/);
    const inputCombinations = output.match(/input combinations:\s*(\d+)/);
    return {
        checked,
        feasible: Number(progress[2]),
        topCount: Number(progress[3]),
        elapsedMs,
        timedOut: !!progress[5],
        bestScore: Number(score[1]),
        checkedPerSecond: checked / elapsedMs * 1000,
        poolSizes,
        combinations: combinations ? Number(combinations[1]) : null,
        inputCombinations: inputCombinations ? Number(inputCombinations[1]) : null,
    };
}

function percentChange(before, after) {
    if (before === 0) return null;
    return (after - before) / before * 100;
}

function compareVariants(original, current) {
    return {
        originalSpace: original.combinations ?? null,
        currentSpace: current.combinations ?? null,
        spaceChangePct: original.combinations == null || current.combinations == null
            ? null : percentChange(original.combinations, current.combinations),
        throughputChangePct: percentChange(
            original.checked / original.elapsedMs,
            current.checked / current.elapsedMs,
        ),
        scoreChangePct: percentChange(original.bestScore, current.bestScore),
    };
}

module.exports = { parseSolverOutput, compareVariants };
