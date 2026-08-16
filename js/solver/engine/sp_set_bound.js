'use strict';

// ══════════════════════════════════════════════════════════════════════════════
// REACHABLE SET-GRANTED SKILL POINTS
//
// The enumerator's skill-point bound estimates how much provision a branch can
// still reach. Items contribute through `skillpoints`, but sets also grant
// skill points once enough pieces are worn — one set in the shipped corpus
// grants +85 to a single attribute at three pieces. Leaving that out of the
// estimate understates provision, which overstates the deficit, which prunes
// branches that are genuinely buildable.
//
// The bound must never claim LESS than a completion can actually reach, or it
// prunes real builds. It should also not claim wildly more than reachable, or
// it stops pruning anything: a blanket cap of every set's best row is
// admissible and measurably useless — in the Rust engine it prunes so little
// that it runs slower than having no bound at all.
//
// Both properties live in one small function, which is why it is here on its
// own rather than inline in the enumeration: it is worth testing directly.
// ══════════════════════════════════════════════════════════════════════════════

/**
 * Accumulate the best set bonus per attribute that is still reachable.
 *
 * Reachable piece counts run from what is already worn (pieces cannot be
 * removed) up to that plus the pieces the remaining slots could still supply.
 * Both ends are clamped into the table: a set worn more times than its table
 * has rows keeps the top row, matching how calculate_skillpoints reads it.
 * Letting the range invert instead would contribute nothing and make the bound
 * inadmissible — that is a real bug this guards against, caught once in the
 * Rust port at two builds short on fam_hybrid_small.
 *
 * @param {number[][]} rows  bonus per attribute, indexed by (count - 1)
 * @param {number} worn      pieces of this set already equipped
 * @param {number} reach     further pieces the remaining slots could supply
 * @param {Int32Array|number[]} out  accumulator, maxed in place (length 5)
 */
function accumulate_reachable_set_bonus(rows, worn, reach, out) {
    if (!rows || rows.length === 0) return out;
    const lo = Math.min(Math.max(worn, 1), rows.length);
    const hi = Math.max(lo, Math.min(rows.length, worn + reach));
    for (let t = lo; t <= hi; t++) {
        const row = rows[t - 1];
        for (let j = 0; j < 5; j++) if (row[j] > out[j]) out[j] = row[j];
    }
    return out;
}

/**
 * The piece counts `accumulate_reachable_set_bonus` considers reachable.
 * Exposed so tests can state the admissibility property against the same
 * notion of reachability the solver uses, rather than a restatement of it.
 */
function reachable_set_counts(rowCount, worn, reach) {
    if (rowCount === 0) return [];
    const lo = Math.min(Math.max(worn, 1), rowCount);
    const hi = Math.max(lo, Math.min(rowCount, worn + reach));
    const counts = [];
    for (let t = lo; t <= hi; t++) counts.push(t);
    return counts;
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = { accumulate_reachable_set_bonus, reachable_set_counts };
}
