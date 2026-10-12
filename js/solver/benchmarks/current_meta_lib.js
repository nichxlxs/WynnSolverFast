#!/usr/bin/env node
'use strict';

const crypto = require('crypto');

const EQUIPMENT_SLOTS = [
    'helmet', 'chestplate', 'leggings', 'boots',
    'ring1', 'ring2', 'bracelet', 'necklace',
];

const EXPECTED_ARCHETYPES = {
    archer: ['boltslinger', 'sharpshooter', 'trapper'],
    assassin: ['acrobat', 'shadestepper', 'trickster'],
    mage: ['arcanist', 'light_bender', 'riftwalker'],
    shaman: ['acolyte', 'ritualist', 'summoner'],
    warrior: ['battle_monk', 'fallen', 'paladin'],
};

const REMOVAL_VARIANTS = [
    {
        name: 'known_good', remove_count: 0, required: true, enforce: true,
        time_limit_seconds: 10, benchmark_tier: 'anchor', checkpoints_seconds: [],
    },
    {
        name: 'remove_1', remove_count: 1, required: true, enforce: true,
        time_limit_seconds: 60, benchmark_tier: 'exact', checkpoints_seconds: [],
    },
    {
        name: 'remove_2', remove_count: 2, required: true, enforce: true,
        time_limit_seconds: 180, benchmark_tier: 'exact', checkpoints_seconds: [],
    },
    {
        name: 'remove_3', remove_count: 3, required: false, enforce: false,
        time_limit_seconds: 600, benchmark_tier: 'exact', checkpoints_seconds: [],
    },
    {
        name: 'remove_4', remove_count: 4, required: false, enforce: false,
        time_limit_seconds: 300, benchmark_tier: 'anytime', checkpoints_seconds: [60, 180, 300],
    },
    {
        name: 'remove_5', remove_count: 5, required: false, enforce: false,
        time_limit_seconds: 300, benchmark_tier: 'anytime', checkpoints_seconds: [60, 180, 300],
    },
    {
        name: 'remove_6', remove_count: 6, required: false, enforce: false,
        time_limit_seconds: 300, benchmark_tier: 'anytime', checkpoints_seconds: [60, 180, 300],
    },
];

function stableSeed(text) {
    return crypto.createHash('sha256').update(text).digest().readUInt32LE(0) || 1;
}

function mulberry32(seed) {
    let value = seed >>> 0;
    return () => {
        value = (value + 0x6D2B79F5) >>> 0;
        let mixed = value;
        mixed = Math.imul(mixed ^ (mixed >>> 15), mixed | 1);
        mixed ^= mixed + Math.imul(mixed ^ (mixed >>> 7), mixed | 61);
        return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
    };
}

function deterministicRemoval(profileId, removeCount, replicate = 0) {
    if (!Number.isInteger(removeCount) || removeCount < 0 || removeCount > EQUIPMENT_SLOTS.length) {
        throw new Error(`invalid removal count ${removeCount}`);
    }
    const slots = [...EQUIPMENT_SLOTS];
    const random = mulberry32(stableSeed(`${profileId}:${removeCount}:${replicate}`));
    for (let index = slots.length - 1; index > 0; index -= 1) {
        const swapIndex = Math.floor(random() * (index + 1));
        [slots[index], slots[swapIndex]] = [slots[swapIndex], slots[index]];
    }
    const removed = slots.slice(0, removeCount).sort(
        (left, right) => EQUIPMENT_SLOTS.indexOf(left) - EQUIPMENT_SLOTS.indexOf(right));
    const mask = removed.reduce(
        (value, slot) => value | (1 << EQUIPMENT_SLOTS.indexOf(slot)), 0);
    return { mask, removed_slots: removed, replicate };
}

function currentizeBuilderUrl(url) {
    const hashIndex = url.indexOf('#');
    if (hashIndex < 0) throw new Error(`builder URL has no hash: ${url}`);
    const fragment = url.slice(hashIndex + 1);
    if (!/^C[A-Z]/.test(fragment)) throw new Error(`unsupported builder hash: ${fragment.slice(0, 8)}`);
    return `${url.slice(0, hashIndex + 1)}CX${fragment.slice(2)}`
        .replace('wynnbuilder-beta.github.io', 'wynnbuilder.github.io');
}

function builderHash(url) {
    const hashIndex = url.indexOf('#');
    if (hashIndex < 0) throw new Error(`builder URL has no hash: ${url}`);
    return url.slice(hashIndex + 1).split('_')[0];
}

function scenarioName(profileId, variantName) {
    return `solver_meta_${profileId}_${variantName}`;
}

function validateProfiles(catalog) {
    if (catalog.schema_version !== 1) throw new Error('current-meta profile schema must be version 1');
    if (!Array.isArray(catalog.profiles)) throw new Error('profiles must be an array');

    const expected = new Set(Object.entries(EXPECTED_ARCHETYPES)
        .flatMap(([className, archetypes]) => archetypes.map(archetype => `${className}/${archetype}`)));
    const seenCoverage = new Set();
    const seenIds = new Set();
    const validTargets = new Set(['combo_damage', 'total_healing', 'ehp', 'ehp_no_agi']);

    for (const profile of catalog.profiles) {
        if (seenIds.has(profile.id)) throw new Error(`duplicate profile id ${profile.id}`);
        seenIds.add(profile.id);
        const coverageKey = `${profile.class_name}/${profile.archetype}`;
        if (!expected.has(coverageKey)) throw new Error(`${profile.id}: unexpected coverage ${coverageKey}`);
        if (seenCoverage.has(coverageKey)) throw new Error(`${profile.id}: duplicate coverage ${coverageKey}`);
        seenCoverage.add(coverageKey);
        if (!validTargets.has(profile.objective.scoring_target)) {
            throw new Error(`${profile.id}: unsupported scoring target ${profile.objective.scoring_target}`);
        }
        if (!Array.isArray(profile.seed_item_names) || profile.seed_item_names.length !== 8) {
            throw new Error(`${profile.id}: seed_item_names must contain exactly eight equipment pieces`);
        }
        if (!profile.core_weapon || !profile.authored_url || !profile.designed_version) {
            throw new Error(`${profile.id}: missing seed provenance`);
        }
        if (profile.source_message_url != null
            && !/^https:\/\/discord\.com\/channels\/\d+\/\d+\/\d+$/.test(profile.source_message_url)) {
            throw new Error(`${profile.id}: invalid Discord source message URL`);
        }
        if (!profile.build_id || !profile.source_evidence || !profile.seed_quality
            || !profile.build_family || !Array.isArray(profile.elements) || !profile.elements.length) {
            throw new Error(`${profile.id}: incomplete analysis metadata`);
        }
        if (!Array.isArray(profile.conditions.restrictions) || !Array.isArray(profile.objective.combo_rows)) {
            throw new Error(`${profile.id}: invalid objective or condition contract`);
        }
        if (!['authored_current', 'standardized_current_gap_fill'].includes(profile.version_role)) {
            throw new Error(`${profile.id}: invalid version role ${profile.version_role}`);
        }
    }

    const missing = [...expected].filter(key => !seenCoverage.has(key));
    if (missing.length || seenCoverage.size !== expected.size) {
        throw new Error(`archetype coverage mismatch; missing: ${missing.join(', ') || 'none'}`);
    }
    return true;
}

module.exports = {
    EQUIPMENT_SLOTS,
    EXPECTED_ARCHETYPES,
    REMOVAL_VARIANTS,
    builderHash,
    currentizeBuilderUrl,
    deterministicRemoval,
    scenarioName,
    validateProfiles,
};
