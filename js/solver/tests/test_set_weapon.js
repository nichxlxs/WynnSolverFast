// A non-crafted weapon is a set piece: it counts toward its set's bonus.
// Run: node js/solver/tests/test_set_weapon.js
//
// calculate_skillpoints receives the weapon separately from the eight
// equipment slots, and for a long time it only counted set membership over
// those eight. A weapon-inclusive set (44 weapons in 2.2.3.0 belong to one,
// e.g. Bony Bow with Bony Circlet) therefore never activated, in the builder
// or the solver. Reported on PR #19, reproduced with exactly this pair.

'use strict';

const { createSandbox, loadGameData, TestRunner } = require('./harness');

const t = new TestRunner('Set weapon');
const ctx = createSandbox();
const { itemMap, sets, none_items } = loadGameData(ctx);
const { calculate_skillpoints, expandItem } = ctx;

const bow = itemMap.get('Bony Bow');
const circlet = itemMap.get('Bony Circlet');
t.assert(bow && circlet, 'Bony Bow and Bony Circlet exist in the item data');
t.assert(bow?.set === 'Bony' && circlet?.set === 'Bony',
    'the loader assigns both to the Bony set');

const bony = sets.get('Bony');
const two = bony?.bonuses?.[1] ?? {};
t.assert((two.agi ?? 0) > 0, 'the Bony two-piece bonus grants Agility');

// Wynn order: boots, legs, chest, helmet, ring1, ring2, bracelet, necklace.
function equipWith(helmetItem) {
    const order = [3, 2, 1, 0, 4, 5, 6, 7];
    return order.map((slot) => expandItem(slot === 0 && helmetItem ? helmetItem : none_items[slot]));
}

const weaponSm = expandItem(bow);
const withCirclet = calculate_skillpoints(equipWith(circlet), weaponSm);
t.assert(withCirclet !== null, 'Bony Bow + Bony Circlet is skill-point feasible');
const setCounts = withCirclet?.[3];
t.assert(setCounts?.get('Bony') === 2,
    `the weapon counts toward its set: Bony pieces = 2 (got ${setCounts?.get('Bony')})`);

// total_item_skillpoints (index 4) carries set-granted skill points.
const agiIdx = 4;
const withoutCirclet = calculate_skillpoints(equipWith(null), weaponSm);
const gained = withCirclet[4][agiIdx] - withoutCirclet[4][agiIdx]
    - (circlet.agi ?? 0);
t.assert(gained === (two.agi ?? 0) - (bony.bonuses[0]?.agi ?? 0),
    `wearing the circlet adds the two-piece Agility bonus (got +${gained}, `
    + `expected +${(two.agi ?? 0) - (bony.bonuses[0]?.agi ?? 0)})`);

// A crafted weapon never counts toward a set, matching equipment.
{
    const crafted = expandItem(bow);
    crafted.set('crafted', true);
    const r = calculate_skillpoints(equipWith(circlet), crafted);
    t.assert(r?.[3]?.get('Bony') === 1,
        `a crafted weapon does not count toward the set (got ${r?.[3]?.get('Bony')})`);
}

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
