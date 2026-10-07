// Roadmap R20: shortlist merge, completeness across partitions, and QoL
// ranking (js/solver/engine/shortlist.js).
// Run: node js/solver/tests/test_shortlist.js
'use strict';

const { TestRunner } = require('./harness');
const {
    SHORTLIST_DEFAULTS, shortlistUtilities, rankShortlist, mergeShortlistArchives,
} = require('../engine/shortlist.js');

const t = new TestRunner('Shortlist (R20)');
const e = (score, name, stats = {}) => ({ score, item_names: [name], stats });

// Merge: dedupe across partitions, keep the window, apply the rule.
{
    const a = { complete: true, archive_full: false, archive_last: null,
        top_n: [e(100, 'A'), e(97, 'B'), e(80, 'Z')] };
    const b = { complete: true, archive_full: false, archive_last: null,
        top_n: [e(99, 'C'), e(97, 'B')] };
    const m = mergeShortlistArchives([a, b], 0.05);
    t.assert(m.entries.map(x => x.item_names[0]).join() === 'A,C,B', 'merged window is A, C, B (Z is outside, B deduped)');
    t.assert(m.complete, 'complete when every partition finished and none filled');

    t.assert(!mergeShortlistArchives([a, { ...b, complete: false }], 0.05).complete,
        'an unfinished partition withdraws the claim');
    // A full partition whose last kept score is inside the merged window
    // (line 95) may have evicted an in-window build.
    t.assert(!mergeShortlistArchives([a, { ...b, archive_full: true, archive_last: 96 }], 0.05).complete,
        'a full partition with its last score inside the window withdraws the claim');
    t.assert(mergeShortlistArchives([a, { ...b, archive_full: true, archive_last: 94 }], 0.05).complete,
        'a full partition with its last score below the window keeps the claim');
    // The rule uses the GLOBAL best (100, line 95), not partition b's own
    // (99, line 94.05): b's last score 94.8 is inside b's own window but
    // below the merged line, so nothing the merged window needs was lost.
    t.assert(mergeShortlistArchives([a, { ...b, archive_full: true, archive_last: 94.8 }], 0.05).complete,
        'the rule is applied with the merged best');
}

// Utilities saturate and clamp.
{
    const s = SHORTLIST_DEFAULTS;
    const u = shortlistUtilities({ ehp_no_agi: s.ehp_sat * 2, mana_delta: 0, spd: 45, hpr: 300, ls: 0 });
    t.assert(u.ehp === 1 && u.mana === 1, 'EHP and mana saturate at 1');
    t.assert(Math.abs(u.speed - 2 / 5) < 1e-12, 'walk speed is a staircase: +45% is 2 of 5 tiers');
    t.assert(Math.abs(u.sustain - 0.5) < 1e-12, 'sustain is linear to saturation');
    const low = shortlistUtilities({ ehp_no_agi: 0, mana_delta: -50, spd: -20 });
    t.assert(low.ehp === 0 && low.mana === 0 && low.speed === 0, 'below the floors clamps to 0');
    t.assert(shortlistUtilities({ mr: 6 }).mana === 0.5, 'without a timed combo, mana regen stands in');
}

// Ranking: weights 0 is score order; a weight can promote a lower score.
{
    const tanky = e(97, 'Tank', { ehp_no_agi: 30000 });
    const glass = e(100, 'Glass', { ehp_no_agi: 12000 });
    const plain = rankShortlist([tanky, glass]);
    t.assert(plain[0].item_names[0] === 'Glass', 'all weights 0: score order');
    const weighted = rankShortlist([tanky, glass], { ...SHORTLIST_DEFAULTS, w_ehp: 0.05 });
    t.assert(weighted[0].item_names[0] === 'Tank',
        'EHP weight 0.05 promotes a build 3% behind with saturated EHP');
    t.assert(Math.abs(weighted[0].utility - (0.97 + 0.05)) < 1e-12, 'utility is score ratio plus weighted utilities');
    t.assert(rankShortlist([]).length === 0, 'empty archive ranks to nothing');
}

// Variant grouping (R20 step 3).
{
    const { shortlistSlotDistance, collapseShortlistVariants } = require('../engine/shortlist.js');
    const b = (...n) => n;
    const base = b('H', 'C', 'L', 'B', 'R1', 'R2', 'Br', 'N');
    t.assert(shortlistSlotDistance(base, b('H', 'C', 'L', 'B', 'R2', 'R1', 'Br', 'N')) === 0,
        'swapped rings are the same build');
    t.assert(shortlistSlotDistance(base, b('H', 'C', 'L', 'B', 'R1', 'R3', 'Br', 'N')) === 1, 'one ring changed is one slot');
    t.assert(shortlistSlotDistance(base, b('X', 'C', 'L', 'B', 'R1', 'R2', 'Br', 'Y')) === 2, 'two slots');
    const ranked = [
        { score: 100, item_names: base },
        { score: 99, item_names: b('H', 'C', 'L', 'B', 'R1', 'R2', 'Br', 'N2') },   // 1 from #1
        { score: 98, item_names: b('X', 'Y', 'L', 'B', 'R1', 'R2', 'Br', 'N') },    // 2 from #1
        { score: 97, item_names: b('X', 'Y', 'L', 'B', 'R1', 'R2', 'Br', 'N3') },   // 1 from #3
    ];
    const reps = collapseShortlistVariants(ranked, 2);
    t.assert(reps.length === 2 && reps[0].variants.length === 1 && reps[1].variants.length === 1,
        'one-slot variants fold under their best representative');
    t.assert(collapseShortlistVariants(ranked, 1).length === 4, 'k = 1 groups nothing');
}

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
