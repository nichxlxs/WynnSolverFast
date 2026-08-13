#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');

const STAT_TYPES = {
    positive: { low: 30, high: 130, rounding: 'half_up', favorable: 'higher' },
    negative: { low: 70, high: 130, rounding: 'half_down', favorable: 'higher' },
};

function roundHalfUp(value) {
    return Math.sign(value) * Math.floor(Math.abs(value) + 0.5);
}

function roundHalfDown(value) {
    return Math.sign(value) * Math.ceil(Math.abs(value) - 0.5);
}

function displayValue(base, internalRoll, statType = 'positive') {
    const type = STAT_TYPES[statType];
    if (!type) throw new Error(`unknown stat type: ${statType}`);
    if (!Number.isInteger(internalRoll) || internalRoll < type.low || internalRoll > type.high) {
        throw new Error(`internal roll ${internalRoll} is outside ${type.low}..${type.high}`);
    }
    const raw = base * internalRoll / 100;
    let value;
    if (type.rounding === 'half_up') value = roundHalfUp(raw);
    else value = roundHalfDown(raw);
    if (value === 0 && base !== 0) return Math.sign(base);
    return value;
}

function distribution(base, statType = 'positive') {
    const type = STAT_TYPES[statType];
    if (!type) throw new Error(`unknown stat type: ${statType}`);
    const counts = new Map();
    const internalRolls = new Map();
    for (let roll = type.low; roll <= type.high; roll++) {
        const value = displayValue(base, roll, statType);
        counts.set(value, (counts.get(value) || 0) + 1);
        if (!internalRolls.has(value)) internalRolls.set(value, []);
        internalRolls.get(value).push(roll);
    }
    const total = type.high - type.low + 1;
    return [...counts.keys()].sort((a, b) => a - b).map(value => ({
        displayed_value: value,
        internal_rolls: internalRolls.get(value),
        cases: counts.get(value),
        probability: counts.get(value) / total,
    }));
}

function probability(base, statType, predicate) {
    return distribution(base, statType)
        .filter(row => predicate(row.displayed_value))
        .reduce((sum, row) => sum + row.probability, 0);
}

function independentJointProbability(events) {
    return events.reduce((product, event) => product * event, 1);
}

function starProbabilities() {
    return {
        no_star: 71 / 101,
        one_star_only: 24 / 101,
        two_star_only: 5 / 101,
        three_star: 1 / 101,
        at_least_one_star: 30 / 101,
        at_least_two_stars: 6 / 101,
    };
}

function exampleReport() {
    const base10 = distribution(10, 'positive');
    const base5 = distribution(5, 'positive');
    const negative10 = distribution(-10, 'negative');
    return {
        schema_version: 1,
        model: 'inclusive integer internal rolls with equal case weights',
        positive_domain: [30, 130],
        negative_domain: [70, 130],
        star_probabilities: starProbabilities(),
        examples: {
            positive_base_10: base10,
            positive_base_5: base5,
            negative_base_minus_10: negative10,
        },
        joint_examples: {
            five_positive_ids_at_least_one_star: independentJointProbability(Array(5).fill(30 / 101)),
            five_positive_ids_at_least_two_stars: independentJointProbability(Array(5).fill(6 / 101)),
            five_positive_ids_three_star: independentJointProbability(Array(5).fill(1 / 101)),
        },
    };
}

function selfTest() {
    const base10 = distribution(10, 'positive');
    const max10 = base10.find(row => row.displayed_value === 13);
    if (!max10 || max10.cases !== 6) throw new Error('positive base 10 maximum must have 6/101 probability');
    const base5 = distribution(5, 'positive');
    const max5 = base5.find(row => row.displayed_value === 7);
    if (!max5 || max5.cases !== 1) throw new Error('positive base 5 maximum must have 1/101 probability');
    if (displayValue(1, 30, 'positive') !== 1) throw new Error('nonzero positive values must clamp away from zero');
    if (displayValue(-1, 70, 'negative') !== -1) throw new Error('nonzero negative values must clamp away from zero');
}

function main(args) {
    selfTest();
    if (args.includes('--write-examples')) {
        const output = path.join(__dirname, 'id-roll-probability-examples.json');
        fs.writeFileSync(output, `${JSON.stringify(exampleReport(), null, 2)}\n`, 'utf8');
        console.log(output);
        return;
    }
    const base = Number(args[0] ?? 10);
    const statType = args[1] ?? (base < 0 ? 'negative' : 'positive');
    console.log(JSON.stringify({ base, stat_type: statType, distribution: distribution(base, statType) }, null, 2));
}

if (require.main === module) main(process.argv.slice(2));

module.exports = {
    STAT_TYPES,
    displayValue,
    distribution,
    probability,
    independentJointProbability,
    starProbabilities,
};
