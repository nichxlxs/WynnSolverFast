#!/usr/bin/env node
'use strict';

const {
    CANDIDATE_REDUCTION_POLICIES: PRUNING_STRATEGIES,
    get_candidate_reduction_policy,
} = require('../engine/candidate_reducer');

function getPruningStrategy(name) {
    return get_candidate_reduction_policy(name);
}

function expandPruningSelection(selection) {
    if (selection === 'paired') return ['off', 'balanced'];
    if (selection === 'legacy-paired') return ['off', 'current'];
    if (selection === 'legacy-matrix') return ['off', 'conservative', 'current', 'aggressive'];
    if (selection === 'matrix' || selection === 'all') return Object.keys(PRUNING_STRATEGIES);
    const names = selection.split(',').map(value => value.trim()).filter(Boolean);
    for (const name of names) getPruningStrategy(name);
    return names;
}

module.exports = { PRUNING_STRATEGIES, expandPruningSelection, getPruningStrategy };
