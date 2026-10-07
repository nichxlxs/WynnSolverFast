'use strict';

// Roadmap R20: rank a windowed archive by tunable quality of life.
//
// The engine (WINDOW mode) returns every build within x of the best score,
// each with explain-pass stats. This file merges the browser partitions'
// archives, decides whether the merged list provably holds every build in
// the window, and ranks it by
//
//   U = score / best + sum_k w_k * u_k(stats)
//
// where each u_k is a saturating utility in [0, 1]. With every weight at 0
// the order is the score order. Nothing here re-solves: changing a weight
// only re-sorts. The defaults are starting points from the community
// guidance quoted in the roadmap, not game rules.

const SHORTLIST_DEFAULTS = Object.freeze({
    w_ehp: 0, w_mana: 0, w_speed: 0, w_sustain: 0,
    // Non-dodge EHP: 0 at the floor, 1 at saturation.
    ehp_floor: 12000, ehp_sat: 30000,
    // Mana over one combo: 0 when the combo drains `mana_drain` or more, 1 when
    // it is mana-neutral or better. Without a timed combo, mana regen stands in
    // (1 at `mr_sat`).
    mana_drain: 20, mr_sat: 12,
    // Walk speed in 20% tiers (each is one Speed level), 1 at +100%.
    speed_tier: 20, speed_tiers: 5,
    // HP regen plus life steal per second, 1 at saturation.
    sustain_sat: 600,
});

const clamp01 = v => (Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0);

/** Per-stat utilities in [0, 1] for one build's explain stats. */
function shortlistUtilities(stats, s = SHORTLIST_DEFAULTS) {
    const st = stats ?? {};
    const ehp = st.ehp_no_agi ?? st.ehp;
    const mana = st.mana_delta !== undefined
        ? clamp01((st.mana_delta + s.mana_drain) / s.mana_drain)
        : clamp01((st.mr ?? 0) / s.mr_sat);
    return {
        ehp: clamp01((ehp - s.ehp_floor) / (s.ehp_sat - s.ehp_floor)),
        mana,
        speed: clamp01(Math.floor(Math.max(0, st.spd ?? 0) / s.speed_tier) / s.speed_tiers),
        sustain: clamp01(((st.hpr ?? 0) + (st.ls ?? 0)) / s.sustain_sat),
    };
}

/** U for one entry; `best` is the archive's best score (positive). */
function shortlistUtility(entry, best, s = SHORTLIST_DEFAULTS) {
    const u = shortlistUtilities(entry.stats, s);
    return entry.score / best
        + s.w_ehp * u.ehp + s.w_mana * u.mana + s.w_speed * u.speed + s.w_sustain * u.sustain;
}

/** Entries sorted by U, best first; ties by score, then item names. */
function rankShortlist(entries, s = SHORTLIST_DEFAULTS) {
    if (!entries.length) return [];
    const best = Math.max(...entries.map(e => e.score));
    const scored = entries.map(e => ({ entry: e, u: shortlistUtility(e, best, s) }));
    scored.sort((a, b) => (b.u - a.u) || (b.entry.score - a.entry.score)
        || String(a.entry.item_names ?? '').localeCompare(String(b.entry.item_names ?? '')));
    return scored.map(x => ({ ...x.entry, utility: x.u, utilities: shortlistUtilities(x.entry.stats, s) }));
}

/**
 * Merge partition archives. `parts` are the engine's done messages, each with
 * `top_n` entries ({score, item_names, stats, ...}), `complete`, and the
 * window fields. Returns the deduplicated entries inside the window (best
 * first), the window line, and whether the merged list provably holds every
 * build in it: every partition finished, and every partition whose archive
 * filled had its last entry below the merged window line (the engine's rule
 * per partition, applied with the global best, which is at least each
 * partition's own).
 */
function mergeShortlistArchives(parts, window) {
    const byName = new Map();
    for (const p of parts) {
        for (const e of p.top_n ?? []) {
            const key = (e.item_names ?? []).join('\u0000');
            const old = byName.get(key);
            if (!old || e.score > old.score) byName.set(key, e);
        }
    }
    const all = [...byName.values()].sort((a, b) => b.score - a.score);
    const best = all.length ? all[0].score : NaN;
    const line = best * (1 - window);
    const entries = all.filter(e => e.score >= line);
    const complete = parts.length > 0 && parts.every(p => p.complete && (!p.archive_full
        || (Number.isFinite(p.archive_last) && p.archive_last < line)));
    return { entries, best, line, complete };
}

/**
 * Slots in which two builds differ. Positions follow the engine's item order
 * (helmet, chestplate, leggings, boots, ring, ring, bracelet, necklace); the
 * two rings compare as a pair, so swapped rings are not a difference.
 */
function shortlistSlotDistance(a, b) {
    const x = a ?? [], y = b ?? [];
    const n = Math.max(x.length, y.length);
    let d = 0;
    for (let i = 0; i < n; i++) {
        if (n >= 6 && (i === 4 || i === 5)) continue;
        if (x[i] !== y[i]) d++;
    }
    if (n >= 6) {
        const ra = [x[4], x[5]].sort(), rb = [y[4], y[5]].sort();
        d += (ra[0] !== rb[0]) + (ra[1] !== rb[1]);
    }
    return d;
}

/**
 * Roadmap R20 step 3 (R17): group near-duplicates. Walks `ranked` in order;
 * a build within `k - 1` slots of an earlier representative becomes one of
 * its variants, otherwise it is a new representative. Returns the
 * representatives, each with `variants` (in rank order). k = 2 groups
 * builds that differ in a single slot.
 */
function collapseShortlistVariants(ranked, k = 2) {
    const reps = [];
    for (const e of ranked) {
        const home = reps.find(r => shortlistSlotDistance(r.item_names, e.item_names) < k);
        if (home) home.variants.push(e);
        else reps.push({ ...e, variants: [] });
    }
    return reps;
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = {
        SHORTLIST_DEFAULTS, shortlistUtilities, shortlistUtility, rankShortlist,
        mergeShortlistArchives, shortlistSlotDistance, collapseShortlistVariants,
    };
}
