#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');
const {
    createSandbox,
    loadGameData,
    REPO_ROOT,
} = require('../tests/harness');

const snapshotDir = path.join(REPO_ROOT, 'js', 'solver', 'tests', 'snapshots');
const manifestPath = path.join(REPO_ROOT, 'js', 'solver', 'benchmarks', 'family_suite.json');
const uniformMedianRolls = { damage: 50, mana: 50, healing: 50, misc: 50 };
const equipmentSlots = ['helmet', 'chestplate', 'leggings', 'boots', 'ring1', 'ring2', 'bracelet', 'necklace'];
const minimumSmallSearch = 1000001;
const maximumLargeInput = 1880000000000;
const maximumLargeSearch = 1880000000000;

const families = [
    {
        family: 'cancelstack',
        seedBuildId: 'ubg-trance-trance-riftwalker-melee-non-crafted-1',
        seedBuildUrl: 'https://wynnbuilder.github.io/builder/#CX0rkkvWHycp3ZRgpUnRgZfHCDIvHamn8lRgpmWW1W+tjqzDJMm2',
        coreWeapon: 'Trance',
        seedScore: 476516.7353716639,
        seedItemNames: ['Shawl of Gaea', 'Lithosphere', 'Writhing Growth', 'Boulderstorm', 'Vivisected', 'Vivisected', 'Ironleaf Bangle', 'Recalcitrance'],
        levelMin: 100,
        variations: [
            { size: 'small', freeMask: 15, calibratedInput: 198563904, calibratedSearch: 76976100 },
            { size: 'medium', freeMask: 79, calibratedInput: 15289420608, calibratedSearch: 4079733300 },
            { size: 'large', freeMask: 95, calibratedInput: 1024391180736, calibratedSearch: 195827198400 },
        ],
        comboRows: [{ spell_node_id: 118, qty: 3, mana_excl: true, dmg_excl: false }],
        manaDisabled: true,
        restrictions: [
            { stat: 'ehp', op: 'ge', value: 18000 },
            { stat: 'ls', op: 'ge', value: 0 },
            { stat: 'hpr', op: 'ge', value: -200 },
            { stat: 'mainAttackRange', op: 'ge', value: -20 },
        ],
    },
    {
        family: 'heavy_melee',
        seedBuildId: 'ubg-vengeance-vengeance-heavy-melee-shadestepper-non-crafted-2',
        seedBuildUrl: 'https://wynnbuilder.github.io/builder/#CX04KQuCA3c1HXX6EE0n1827XD8xDRmna0m-ybq-j82BB',
        coreWeapon: 'Vengeance',
        seedScore: 38613.608150629116,
        seedItemNames: ['Obsidian-Framed Helmet', 'Taurus', 'Babel', 'Blind Thrust', 'Ad Terram', 'Ad Terram', 'Silent Grove', 'Auxetic Capacitor'],
        levelMin: 104,
        variations: [
            { size: 'small', freeMask: 77, calibratedInput: 57758022, calibratedSearch: 18216000 },
            { size: 'medium', freeMask: 79, calibratedInput: 5371496046, calibratedSearch: 1256904000 },
            { size: 'large', freeMask: 95, calibratedInput: 290060786484, calibratedSearch: 50276160000 },
        ],
        comboRows: [{ spell_node_id: 118, qty: 3, mana_excl: true, dmg_excl: false }],
        manaDisabled: true,
        restrictions: [
            { stat: 'ehp', op: 'ge', value: 18000 },
            { stat: 'ls', op: 'ge', value: 0 },
            { stat: 'hpr', op: 'ge', value: -100 },
        ],
    },
    {
        family: 'tierstack',
        seedBuildId: 'ubg-fate-fate-tierstack-ritualist-standard-1',
        seedBuildUrl: 'https://wynnbuilder.github.io/builder/#CX0i21s1Cn3OVWw4CDI15amn8q6fpb0WsxDL3NNv4a2',
        coreWeapon: 'Fate',
        seedScore: 166361.89991284287,
        seedItemNames: ['Timeskip', 'Calidade Mail', 'Runebound Chains', "Nether's Scar", 'Breezehands', 'Vivisected', 'Prowess', 'Recalcitrance'],
        levelMin: 106,
        variations: [
            { size: 'small', freeMask: 15, calibratedInput: 54521040, calibratedSearch: 6854400 },
            { size: 'medium', freeMask: 47, calibratedInput: 2562488880, calibratedSearch: 171360000 },
            { size: 'large', freeMask: 63, calibratedInput: 61499733120, calibratedSearch: 2227680000 },
        ],
        comboRows: [{ spell_node_id: 118, qty: 3, mana_excl: true, dmg_excl: false }],
        manaDisabled: true,
        restrictions: [
            { stat: 'ehp_no_agi', op: 'ge', value: 12000 },
            { stat: 'atkTier', op: 'ge', value: 3 },
            { stat: 'hpr', op: 'ge', value: -100 },
        ],
    },
    {
        family: 'spellsteal',
        seedBuildId: 'ubg-oblivion-oblivion-spell-shadestepper-non-crafted-1',
        seedBuildUrl: 'https://wynnbuilder.github.io/builder/#CX0bum9DaE1yQGoDqm1KEml1GkCuK98O0YN-H-XrV-E4',
        coreWeapon: 'Oblivion',
        seedScore: 96191.52388364627,
        seedItemNames: ['Prosencephalon', 'Umbral Mail', 'Chaos-Woven Greaves', 'Stardew', 'Yang', 'Photon', 'Misalignment', 'Metamorphosis'],
        levelMin: 106,
        variations: [
            { size: 'small', freeMask: 15, calibratedInput: 54521040, calibratedSearch: 34424208 },
            { size: 'medium', freeMask: 47, calibratedInput: 2562488880, calibratedSearch: 1411392528 },
            { size: 'large', freeMask: 63, calibratedInput: 61499733120, calibratedSearch: 29639243088 },
        ],
        comboRows: [
            { spell_node_id: 118, qty: 2, mana_excl: true, dmg_excl: false },
            { spell_node_id: 1, qty: 2, mana_excl: false, dmg_excl: false },
            { spell_node_id: 3, qty: 2, mana_excl: false, dmg_excl: false },
            { spell_node_id: 2, qty: 1, mana_excl: false, dmg_excl: false },
        ],
        allowDowntime: true,
        restrictions: [
            { stat: 'ehp', op: 'ge', value: 10000 },
            { stat: 'ms', op: 'ge', value: 20 },
            { stat: 'hpr', op: 'ge', value: -250 },
        ],
    },
    {
        family: 'spell_sustained',
        seedBuildId: 'ubg-divzer-divzer-spell-boltslinger-non-crafted-1',
        seedBuildUrl: 'https://wynnbuilder.github.io/builder/#CX0o2tm9im6EfJsm1jv6E2QGG3OOG09TQsmfRGB1m-exVq7SEa30',
        coreWeapon: 'Divzer',
        seedScore: 1476409.9923970918,
        seedItemNames: ['Jailbroken', 'Bete Noire', 'Chaos-Woven Greaves', 'Static Cling', 'Lodestone', 'Lodestone', 'Diamond Static Bracelet', 'Stormleader'],
        levelMin: 100,
        variations: [
            { size: 'small', freeMask: 15, calibratedInput: 198563904, calibratedSearch: 99810090 },
            { size: 'medium', freeMask: 47, calibratedInput: 13303781568, calibratedSearch: 5788985220 },
            { size: 'large', freeMask: 63, calibratedInput: 452328573312, calibratedSearch: 170775063990 },
        ],
        comboRows: [
            { spell_node_id: 1, qty: 2, mana_excl: false, dmg_excl: false },
            { spell_node_id: 3, qty: 2, mana_excl: false, dmg_excl: false },
            { spell_node_id: 2, qty: 2, mana_excl: false, dmg_excl: false },
            { spell_node_id: 4, qty: 1, mana_excl: false, dmg_excl: false },
        ],
        allowDowntime: true,
        restrictions: [
            { stat: 'ehp', op: 'ge', value: 8000 },
            { stat: 'ms', op: 'ge', value: 20 },
            { stat: 'hpr', op: 'ge', value: -300 },
        ],
    },
    {
        family: 'hybrid',
        seedBuildId: 'ubg-divzer-divzer-boltslinger-hybrid-non-crafted-1',
        seedBuildUrl: 'https://wynnbuilder.github.io/builder/#CX0cVAwCufe3dWYwZlcYwZfHCD266M1GdcDSM7qI830qFwzFuXx9S8',
        coreWeapon: 'Divzer',
        seedScore: 282400.7169377645,
        seedItemNames: ['Nighthawk', 'Ornate Shadow Cloak', 'Asphyxia', 'Warchief', 'Vivisected', 'Vivisected', 'Diamond Static Bracelet', 'Achromatic Gloom'],
        levelMin: 105,
        variations: [
            { size: 'small', freeMask: 15, calibratedInput: 71280000, calibratedSearch: 34765200 },
            { size: 'medium', freeMask: 31, calibratedInput: 3706560000, calibratedSearch: 1599199200 },
            { size: 'large', freeMask: 63, calibratedInput: 98223840000, calibratedSearch: 37581181200 },
        ],
        comboRows: [
            { spell_node_id: 118, qty: 2, mana_excl: true, dmg_excl: false },
            { spell_node_id: 1, qty: 1, mana_excl: false, dmg_excl: false },
            { spell_node_id: 3, qty: 1, mana_excl: false, dmg_excl: false },
        ],
        allowDowntime: true,
        restrictions: [
            { stat: 'ehp', op: 'ge', value: 10000 },
            { stat: 'ms', op: 'ge', value: 10 },
            { stat: 'hpr', op: 'ge', value: -250 },
        ],
    },
];

function hashFromUrl(url) {
    return url.slice(url.indexOf('#') + 1).split('_')[0];
}

function maskSlots(mask, free) {
    return equipmentSlots.filter((_, index) => Boolean(mask & (1 << index)) === free);
}

function combinationBudget(variation) {
    const tolerance = 0.25;
    const inputMax = Math.ceil(variation.calibratedInput * (1 + tolerance));
    const searchMax = Math.ceil(variation.calibratedSearch * (1 + tolerance));
    return {
        input_min: Math.floor(variation.calibratedInput * (1 - tolerance)),
        input_max: variation.size === 'large'
            ? Math.min(maximumLargeInput, inputMax)
            : inputMax,
        search_min: Math.max(minimumSmallSearch, Math.floor(variation.calibratedSearch * (1 - tolerance))),
        search_max: variation.size === 'large'
            ? Math.min(maximumLargeSearch, searchMax)
            : searchMax,
    };
}

function validateFamilyDefinitions() {
    const expectedSizes = ['small', 'medium', 'large'];
    for (const family of families) {
        const sizes = family.variations.map(variation => variation.size);
        if (JSON.stringify(sizes) !== JSON.stringify(expectedSizes)) {
            throw new Error(`${family.family}: expected small, medium, and large variants`);
        }
        const freeCounts = family.variations.map(variation => maskSlots(variation.freeMask, true).length);
        if (!(freeCounts[0] < freeCounts[1] && freeCounts[1] < freeCounts[2])) {
            throw new Error(`${family.family}: each larger variant must receive fewer ideal-build items`);
        }
        const spaces = family.variations.map(variation => variation.calibratedSearch);
        if (!(spaces[0] < spaces[1] && spaces[1] < spaces[2])) {
            throw new Error(`${family.family}: calibrated search spaces must increase small to large`);
        }
        const inputSpaces = family.variations.map(variation => variation.calibratedInput);
        if (!(inputSpaces[0] < inputSpaces[1] && inputSpaces[1] < inputSpaces[2])) {
            throw new Error(`${family.family}: calibrated input spaces must increase small to large`);
        }
        if (spaces[0] < minimumSmallSearch) {
            throw new Error(`${family.family}: small search space ${spaces[0]} is below ${minimumSmallSearch}`);
        }
        if (spaces[2] > maximumLargeSearch) {
            throw new Error(`${family.family}: large search space ${spaces[2]} exceeds ${maximumLargeSearch}`);
        }
        if (inputSpaces[2] > maximumLargeInput) {
            throw new Error(`${family.family}: large input space ${inputSpaces[2]} exceeds ${maximumLargeInput}`);
        }
    }
}

function createSolverHash(ctx, family, freeMask) {
    ctx.__familyParams = {
        roll_groups: uniformMedianRolls,
        sfree: freeMask,
        dir_enabled: 31,
        lvl_min: family.levelMin,
        lvl_max: 121,
        nomaj: false,
        gtome: 0,
        dtime: !!family.allowDowntime,
        mana_disabled: !!family.manaDisabled,
        restrictions: [],
        combo_rows: family.comboRows,
        blacklist_ids: [],
        custom_weights: [],
    };
    const solverHash = vm.runInContext('encodeSolverParams(__familyParams)', ctx);
    delete ctx.__familyParams;
    return solverHash;
}

function buildSnapshot(ctx, family, variation) {
    const name = `solver_family_${family.family}_${variation.size}`;
    const freeSlots = maskSlots(variation.freeMask, true);
    const providedSlots = maskSlots(variation.freeMask, false);
    const providedItems = providedSlots.map(slot => family.seedItemNames[equipmentSlots.indexOf(slot)]);
    providedItems.push(family.coreWeapon);
    return {
        name,
        description: `${family.coreWeapon} ${family.family} ${variation.size} search. ${providedItems.length} ideal-build items provided; ${freeSlots.join(', ')} free.`,
        benchmark_family: family.family,
        benchmark_size: variation.size,
        seed_build_id: family.seedBuildId,
        seed_build_url: family.seedBuildUrl,
        core_weapon: family.coreWeapon,
        seed_score: family.seedScore,
        seed_item_names: family.seedItemNames,
        provided_ideal_item_count: providedItems.length,
        provided_ideal_items: providedItems,
        free_equipment_slots: freeSlots,
        url_hash: `${hashFromUrl(family.seedBuildUrl)}_${createSolverHash(ctx, family, variation.freeMask)}`,
        scoring_target: 'combo_damage',
        free_mask: variation.freeMask,
        lvl_min: family.levelMin,
        lvl_max: 121,
        time_limit_seconds: 30,
        num_workers: 2,
        extra_restrictions: family.restrictions,
        roll_profile_note: 'All roll groups use the midpoint of the displayed min-to-max interval. This is a deterministic median-ID proxy, not a claim that the resulting whole item or build is a joint 50th-percentile roll.',
        benchmark_only: true,
        calibrated_input_combinations: variation.calibratedInput,
        calibrated_search_combinations: variation.calibratedSearch,
        combination_budget: combinationBudget(variation),
    };
}

function writeSnapshot(snapshot) {
    fs.writeFileSync(
        path.join(snapshotDir, `${snapshot.name}.snap.json`),
        `${JSON.stringify(snapshot, null, 2)}\n`,
        'utf8',
    );
}

function main() {
    validateFamilyDefinitions();
    const ctx = createSandbox();
    loadGameData(ctx);
    const manifest = {
        schema_version: 2,
        generated_at: new Date().toISOString(),
        time_cap_seconds: 30,
        minimum_small_search_combinations: minimumSmallSearch,
        maximum_large_input_combinations: maximumLargeInput,
        maximum_large_search_combinations: maximumLargeSearch,
        roll_profile: uniformMedianRolls,
        scale_semantics: 'Each family has small, medium, and large variants created only by changing how many equipment pieces from the ideal build remain locked. Search space means the post-dominance Cartesian space.',
        families: [],
        scenarios: [],
    };

    for (const family of families) {
        const familyManifest = {
            family: family.family,
            core_weapon: family.coreWeapon,
            seed_build_id: family.seedBuildId,
            seed_build_url: family.seedBuildUrl,
            seed_score: family.seedScore,
            seed_item_names: family.seedItemNames,
            seed_browser_validation: 'research/build-database/family-seed-browser-validation.json',
            success_restrictions: family.restrictions,
            variants: [],
        };
        for (const variation of family.variations) {
            const snapshot = buildSnapshot(ctx, family, variation);
            writeSnapshot(snapshot);
            const manifestScenario = {
                snapshot: snapshot.name,
                family: family.family,
                size: variation.size,
                provided_ideal_item_count: snapshot.provided_ideal_item_count,
                provided_ideal_items: snapshot.provided_ideal_items,
                free_equipment_slots: snapshot.free_equipment_slots,
                time_cap_seconds: 30,
                calibrated_input_combinations: snapshot.calibrated_input_combinations,
                calibrated_search_combinations: snapshot.calibrated_search_combinations,
                combination_budget: snapshot.combination_budget,
            };
            manifest.scenarios.push(manifestScenario);
            familyManifest.variants.push(manifestScenario);
        }

        const knownGood = buildSnapshot(ctx, family, {
            size: 'known_good',
            freeMask: 0,
            calibratedInput: 1,
            calibratedSearch: 1,
        });
        knownGood.name = `solver_family_${family.family}_known_good`;
        knownGood.description = `${family.coreWeapon} ${family.family} exact known-good seed with all equipment slots locked.`;
        knownGood.time_limit_seconds = 10;
        knownGood.num_workers = 1;
        knownGood.calibrated_input_combinations = 1;
        knownGood.calibrated_search_combinations = 1;
        knownGood.combination_budget = { input_min: 1, input_max: 1, search_min: 1, search_max: 1 };
        knownGood.known_good_seed_verification = true;
        delete knownGood.seed_score;
        delete knownGood.seed_item_names;
        writeSnapshot(knownGood);
        familyManifest.known_good_snapshot = knownGood.name;
        manifest.families.push(familyManifest);
    }

    fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${manifest.scenarios.length} family variants, ${manifest.families.length} known-good snapshots, and ${path.relative(REPO_ROOT, manifestPath)}`);
}

main();
