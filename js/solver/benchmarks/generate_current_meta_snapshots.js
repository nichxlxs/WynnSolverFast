#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');
const {
    createSandbox,
    decodeSolverUrl,
    loadGameData,
    REPO_ROOT,
} = require('../tests/harness');
const {
    EQUIPMENT_SLOTS,
    REMOVAL_VARIANTS,
    builderHash,
    currentizeBuilderUrl,
    deterministicRemoval,
    scenarioName,
    validateProfiles,
} = require('./current_meta_lib');

const catalogPath = path.join(__dirname, 'current_meta_profiles.json');
const targetsPath = path.join(__dirname, 'current_meta_targets.json');
const manifestPath = path.join(__dirname, 'current_meta_suite.json');
const snapshotDir = path.join(REPO_ROOT, 'js', 'solver', 'tests', 'snapshots');
const catalog = JSON.parse(fs.readFileSync(catalogPath, 'utf8'));
const targets = fs.existsSync(targetsPath)
    ? JSON.parse(fs.readFileSync(targetsPath, 'utf8'))
    : { schema_version: 1, profiles: {} };

function profileLevelMin(ctx, profile) {
    return Math.min(100, ...profile.seed_item_names.map(itemName => {
        const item = ctx.itemMap.get(itemName);
        if (!item) throw new Error(`${profile.id}: missing seed item ${itemName}`);
        return item.lvl ?? 1;
    }));
}

function createSolverHash(ctx, profile, freeMask) {
    ctx.__currentMetaParams = {
        roll_groups: catalog.roll_profile,
        sfree: freeMask,
        dir_enabled: 31,
        lvl_min: profileLevelMin(ctx, profile),
        lvl_max: 121,
        nomaj: false,
        gtome: 0,
        dtime: profile.conditions.allow_downtime,
        mana_disabled: profile.conditions.mana_disabled,
        restrictions: [],
        combo_rows: profile.objective.combo_rows,
        blacklist_ids: [],
        custom_weights: [],
    };
    const solverHash = vm.runInContext('encodeSolverParams(__currentMetaParams)', ctx);
    delete ctx.__currentMetaParams;
    return solverHash;
}

function parsePowderIds(ctx, raw) {
    if (Array.isArray(raw)) return raw;
    if (typeof raw !== 'string') return [];
    const ids = [];
    for (let offset = 0; offset + 1 < raw.length; offset += 2) {
        const id = ctx.powderIDs.get(raw.slice(offset, offset + 2));
        if (id != null) ids.push(id);
    }
    return ids;
}

function powderMetadata(ctx, decoded) {
    const elementNames = ['earth', 'thunder', 'water', 'fire', 'air'];
    const bySlot = (decoded.powders || []).map((raw, index) => {
        const powderIds = parsePowderIds(ctx, raw);
        return {
            slot_index: index,
            powder_ids: powderIds,
            powders: powderIds.map(id => ({
                element: elementNames[Math.floor(id / 7)] || 'unknown',
                tier: (id % 7) + 1,
            })),
        };
    }).filter(entry => entry.powder_ids.length > 0);
    return {
        by_slot: bySlot,
        elements: [...new Set(bySlot.flatMap(entry => entry.powders.map(powder => powder.element)))],
    };
}

function validateDecodedProfile(ctx, profile, currentUrl) {
    const decoded = decodeSolverUrl(ctx, builderHash(currentUrl));
    const expectedClass = profile.class_name[0].toUpperCase() + profile.class_name.slice(1);
    if (decoded.playerClass !== expectedClass) {
        throw new Error(`${profile.id}: decoded class ${decoded.playerClass}, expected ${expectedClass}`);
    }
    const decodedItems = decoded.equipment.slice(0, 8);
    if (JSON.stringify(decodedItems) !== JSON.stringify(profile.seed_item_names)) {
        throw new Error(`${profile.id}: decoded equipment differs from catalog`);
    }
    if (decoded.equipment[8] !== profile.core_weapon) {
        throw new Error(`${profile.id}: decoded weapon ${decoded.equipment[8]}, expected ${profile.core_weapon}`);
    }
    for (const itemName of decoded.equipment) {
        if (!ctx.itemMap.has(itemName)) throw new Error(`${profile.id}: ${itemName} missing from 2.2.3.0 item data`);
    }
    return decoded;
}

function buildSnapshot(ctx, profile, variant, decoded, currentUrl) {
    const removal = deterministicRemoval(profile.id, variant.remove_count);
    const suppliedSlots = EQUIPMENT_SLOTS.filter(slot => !removal.removed_slots.includes(slot));
    const suppliedItems = suppliedSlots.map(slot => profile.seed_item_names[EQUIPMENT_SLOTS.indexOf(slot)]);
    suppliedItems.push(profile.core_weapon);
    const target = targets.profiles?.[profile.id]?.score ?? null;
    const name = scenarioName(profile.id, variant.name);
    const recovery = {
        target_score: target,
        required: variant.required,
        enforcement: variant.enforce ? 'fail' : 'report',
        relative_tolerance: 1e-9,
        removal_count: variant.remove_count,
        removal_seed: `${profile.id}:${variant.remove_count}:0`,
        removed_slots: removal.removed_slots,
        contract_kind: variant.name === 'known_good' ? 'seed_calibration' : 'rediscovery_same_or_better',
    };
    const snapshot = {
        name,
        description: `${profile.class_name}/${profile.archetype} ${profile.build_family}: ${variant.name}; ${removal.removed_slots.join(', ') || 'no'} equipment slots free.`,
        benchmark_only: true,
        benchmark_suite: 'current_meta',
        benchmark_family: profile.build_family,
        benchmark_class: profile.class_name,
        benchmark_archetype: profile.archetype,
        benchmark_variant: variant.name,
        benchmark_tier: variant.benchmark_tier,
        benchmark_checkpoints_seconds: variant.checkpoints_seconds,
        benchmark_version: catalog.benchmark_game_version,
        seed_build_id: profile.build_id,
        seed_build_url: currentUrl,
        authored_build_url: profile.authored_url,
        designed_version: profile.designed_version,
        version_role: profile.version_role,
        seed_quality: profile.seed_quality,
        source_evidence: profile.source_evidence,
        source_message_url: profile.source_message_url,
        core_weapon: profile.core_weapon,
        seed_item_names: profile.seed_item_names,
        elements: profile.elements,
        powder_profile: powderMetadata(ctx, decoded),
        objective_contract: profile.objective,
        playability_contract: profile.conditions,
        recovery_contract: recovery,
        disable_seed_incumbent: true,
        supplied_seed_items: suppliedItems,
        free_equipment_slots: removal.removed_slots,
        free_mask: removal.mask,
        url_hash: `${builderHash(currentUrl)}_${createSolverHash(ctx, profile, removal.mask)}`,
        scoring_target: profile.objective.scoring_target,
        lvl_min: profileLevelMin(ctx, profile),
        lvl_max: 121,
        level_floor_contract: 'The floor is the lower of level 100 and the minimum seed-equipment level, so every unlocked seed item remains eligible for recovery.',
        time_limit_seconds: variant.time_limit_seconds,
        num_workers: variant.name === 'known_good' ? 1 : 2,
        extra_restrictions: profile.conditions.restrictions,
        roll_profile_note: 'All roll groups use the midpoint of each displayed ID range. This is deterministic and is not a claim that a whole build is jointly 50th percentile.',
    };
    const resolvedRows = targets.profiles?.[profile.id]?.resolved_combo_rows;
    if (resolvedRows) {
        snapshot.expected_resolved_combo_rows = resolvedRows;
    }
    return snapshot;
}

function writeSnapshot(snapshot) {
    fs.writeFileSync(
        path.join(snapshotDir, `${snapshot.name}.snap.json`),
        `${JSON.stringify(snapshot, null, 2)}\n`,
        'utf8');
}

function main() {
    validateProfiles(catalog);
    const ctx = createSandbox();
    loadGameData(ctx);
    for (const fileName of fs.readdirSync(snapshotDir)) {
        if (fileName.startsWith('solver_meta_') && fileName.endsWith('.snap.json')) {
            fs.unlinkSync(path.join(snapshotDir, fileName));
        }
    }
    const manifest = {
        schema_version: 1,
        generated_at: new Date().toISOString(),
        benchmark_game_version: catalog.benchmark_game_version,
        primary_grouping: 'build_family',
        profile_catalog: path.relative(REPO_ROOT, catalogPath).replaceAll('\\', '/'),
        targets: path.relative(REPO_ROOT, targetsPath).replaceAll('\\', '/'),
        deterministic_removal_semantics: 'Slots are shuffled with a SHA-256-seeded Mulberry32 stream keyed by profile, removal count, and replicate. Standard scenarios use replicate zero.',
        recovery_semantics: 'remove_1 through remove_3 are exhaustive candidates whose maximum is valid only on completion. remove_4 through remove_6 are fixed-budget anytime workloads with censored score checkpoints.',
        profiles: [],
        family_groups: [],
        scenarios: [],
    };

    for (const profile of catalog.profiles) {
        const currentUrl = currentizeBuilderUrl(profile.authored_url);
        const decoded = validateDecodedProfile(ctx, profile, currentUrl);
        const profileManifest = {
            id: profile.id,
            class_name: profile.class_name,
            archetype: profile.archetype,
            build_family: profile.build_family,
            core_weapon: profile.core_weapon,
            authored_url: profile.authored_url,
            benchmark_url: currentUrl,
            designed_version: profile.designed_version,
            version_role: profile.version_role,
            seed_quality: profile.seed_quality,
            source_message_url: profile.source_message_url,
            objective: profile.objective,
            conditions: profile.conditions,
            elements: profile.elements,
            powder_profile: powderMetadata(ctx, decoded),
            target_score: targets.profiles?.[profile.id]?.score ?? null,
            scenarios: [],
        };
        for (const variant of REMOVAL_VARIANTS) {
            const snapshot = buildSnapshot(ctx, profile, variant, decoded, currentUrl);
            writeSnapshot(snapshot);
            const scenario = {
                snapshot: snapshot.name,
                profile_id: profile.id,
                class_name: profile.class_name,
                archetype: profile.archetype,
                build_family: profile.build_family,
                variant: variant.name,
                benchmark_tier: variant.benchmark_tier,
                checkpoints_seconds: variant.checkpoints_seconds,
                score_semantics: variant.benchmark_tier === 'anytime'
                    ? 'censored_anytime_observation'
                    : variant.benchmark_tier === 'exact'
                        ? 'maximum_only_if_completed'
                        : 'seed_anchor',
                free_mask: snapshot.free_mask,
                free_equipment_slots: snapshot.free_equipment_slots,
                supplied_seed_items: snapshot.supplied_seed_items,
                time_limit_seconds: snapshot.time_limit_seconds,
                recovery_contract: snapshot.recovery_contract,
            };
            manifest.scenarios.push(scenario);
            profileManifest.scenarios.push(scenario);
        }
        manifest.profiles.push(profileManifest);
    }

    const familyGroups = new Map();
    for (const profile of manifest.profiles) {
        if (!familyGroups.has(profile.build_family)) familyGroups.set(profile.build_family, []);
        familyGroups.get(profile.build_family).push({
            profile_id: profile.id,
            class_name: profile.class_name,
            archetype: profile.archetype,
            objective: profile.objective,
        });
    }
    manifest.family_groups = [...familyGroups.entries()]
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([buildFamily, profiles]) => ({
            build_family: buildFamily,
            classes: [...new Set(profiles.map(profile => profile.class_name))].sort(),
            profiles,
        }));

    fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
    const calibrated = manifest.profiles.filter(profile => profile.target_score != null).length;
    console.log(`Wrote ${manifest.scenarios.length} scenarios for ${manifest.profiles.length} profiles; ${calibrated} calibrated.`);
}

main();
