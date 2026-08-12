'use strict';

/**
 * Deliberately simple Cartesian oracle. It shares no traversal code with the
 * production level enumerator and is intended only for tiny differential cases.
 */
function exhaustiveSearchOracle({ slots, pools, evaluate, limit = 15 }) {
    const selected = Object.create(null);
    const results = [];
    const counts = { tuples: 0, feasible: 0, scored: 0 };

    function visit(depth) {
        if (depth === slots.length) {
            counts.tuples++;
            const result = evaluate(selected);
            if (!result) return;
            counts.feasible++;
            if (!Number.isFinite(result.score)) return;
            counts.scored++;
            results.push(result);
            results.sort((a, b) => b.score - a.score || String(a.key).localeCompare(String(b.key)));
            if (results.length > limit) results.length = limit;
            return;
        }
        const slot = slots[depth];
        const pool = pools[slot];
        for (let i = 0; i < pool.length; i++) {
            if (slot === 'ring2' && selected.ring1?.poolIndex > i) continue;
            selected[slot] = { value: pool[i], poolIndex: i };
            visit(depth + 1);
        }
        delete selected[slot];
    }

    visit(0);
    return { counts, results };
}

module.exports = { exhaustiveSearchOracle };
