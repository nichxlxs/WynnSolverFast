'use strict';

const { TestRunner } = require('./harness');
const { exhaustiveSearchOracle } = require('./search_oracle');
const t = new TestRunner('Search oracle');

const pools = {
    helmet: [{ score: 1, hp: 4 }, { score: 5, hp: -2 }],
    ring1: [{ score: 2, hp: 1 }, { score: 4, hp: 3 }],
    ring2: [{ score: 2, hp: 1 }, { score: 4, hp: 3 }],
};
const out = exhaustiveSearchOracle({
    slots: ['helmet', 'ring1', 'ring2'], pools,
    evaluate(selected) {
        const items = Object.values(selected).map(entry => entry.value);
        const hp = items.reduce((sum, item) => sum + item.hp, 0);
        if (hp < 4) return null;
        return {
            score: items.reduce((sum, item) => sum + item.score, 0),
            key: Object.values(selected).map(entry => entry.poolIndex).join(','),
        };
    },
});

t.assert(out.counts.tuples === 6, 'canonical rings produce n*(n+1)/2 tuples per armor');
t.assert(out.counts.feasible === 4 && out.counts.scored === 4,
    'oracle reports an independent feasibility funnel');
t.assert(out.results[0].score === 13 && out.results[0].key === '1,1,1',
    'oracle returns the exact deterministic optimum');
t.assert(new Set(out.results.map(result => result.key)).size === out.results.length,
    'oracle never emits symmetric duplicate ring tuples');

const summary = t.summary();
if (require.main === module && summary.fail > 0) process.exit(1);
module.exports = summary;
