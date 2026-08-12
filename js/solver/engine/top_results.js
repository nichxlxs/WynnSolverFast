'use strict';

function compareTopResult(a, b) {
    const scoreOrder = b.score - a.score;
    if (scoreOrder !== 0) return scoreOrder;
    const aNames = a.item_names ?? [];
    const bNames = b.item_names ?? [];
    const length = Math.max(aNames.length, bNames.length);
    for (let i = 0; i < length; i++) {
        const aName = String(aNames[i] ?? '');
        const bName = String(bNames[i] ?? '');
        if (aName < bName) return -1;
        if (aName > bName) return 1;
    }
    return 0;
}

/**
 * Insert a candidate into a bounded, best-first result buffer. The factory is
 * deliberately deferred so leaves below a full buffer's score cutoff allocate
 * neither item names nor SP snapshots.
 */
function tryInsertTopResult(buffer, score, candidateFactory, capacity) {
    if (!Number.isInteger(capacity) || capacity < 1) {
        throw new RangeError('capacity must be a positive integer');
    }
    if (buffer.length === capacity && score < buffer[capacity - 1].score) return false;

    const candidate = candidateFactory();
    let lo = 0;
    let hi = buffer.length;
    while (lo < hi) {
        const mid = (lo + hi) >>> 1;
        if (compareTopResult(candidate, buffer[mid]) < 0) hi = mid;
        else lo = mid + 1;
    }
    if (lo >= capacity) return false;
    buffer.splice(lo, 0, candidate);
    if (buffer.length > capacity) buffer.pop();
    return true;
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = { compareTopResult, tryInsertTopResult };
}
