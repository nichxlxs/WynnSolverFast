#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');
const {
    createSandbox,
    loadGameData,
    decodeSolverUrl,
    decodeActiveNodes,
    REPO_ROOT,
    LATEST_VERSION,
} = require('../../js/solver/tests/harness');

const sourcePath = path.join(__dirname, 'nori-builds.json');
const browserPath = path.join(__dirname, 'nori-browser-validation.json');
const outputPath = path.join(__dirname, 'nori-validation.json');
const csvPath = path.join(__dirname, 'nori-build-status.csv');
const atreeCompletenessThreshold = Number(process.env.NORI_ATREE_COMPLETENESS || 0.8);

function extractHash(url) {
    const index = url.indexOf('#');
    return index >= 0 ? url.slice(index + 1) : url;
}

function normalize(value) {
    return String(value || '').trim().toLowerCase();
}

function csvEscape(value) {
    const text = Array.isArray(value) ? value.join('|') : String(value ?? '');
    return /[",\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

function weaponMetadataMatches(expected, actual) {
    const left = normalize(expected);
    const right = normalize(actual);
    return left === right || right === `masterwork ${left}`;
}

function classifyFamilies(build) {
    const text = `${build.name || ''},${build.tags_raw || ''}`.toLowerCase();
    const families = [];
    const add = family => { if (!families.includes(family)) families.push(family); };
    if (/cancel\s*stack/.test(text)) add('cancelstack');
    if (/\b(?:tstack|tier\s*stack)\b/.test(text)) add('tierstack');
    if (/\bheavy\s+melee\b/.test(text)) add('heavy_melee');
    if (/\bhybrid\b/.test(text)) add('hybrid');
    if (/\bspell\s*steal\b/.test(text)) add('spellsteal');
    if (/\b(?:spellspam|spell|arcanist|auraspam)\b/.test(text)) add('spell_sustained');
    if (/\bmelee\b/.test(text)) add('melee');
    if (/\bpoison\b/.test(text)) add('poison');
    if (/\b(?:support|healing|healer)\b/.test(text)) add('support');
    if (/\blootrun\b/.test(text)) add('lootrun');
    return families.length ? families : ['unclassified'];
}

function setupContext() {
    const ctx = createSandbox();
    loadGameData(ctx);
    const searchPath = path.join(REPO_ROOT, 'js', 'solver', 'engine', 'search.js');
    vm.runInContext(fs.readFileSync(searchPath, 'utf8'), ctx, { filename: searchPath });
    vm.runInContext(`
        globalThis._apply_roll_mode_to_item = _apply_roll_mode_to_item;
        globalThis.tomeFieldsExport = [...tome_fields];
        globalThis.tomeRedirectMap = new Map();
        current_roll_mode = { damage: 50, mana: 50, healing: 50, misc: 50 };
    `, ctx);
    ctx.tome_fields = ctx.tomeFieldsExport;
    return ctx;
}

function resolveTomes(ctx, decoded) {
    const noneByType = Object.fromEntries(ctx.none_tomes.map(tome => [tome.type, tome]));
    return ctx.tome_fields.map((field, index) => {
        const name = decoded.tomes?.[index];
        const type = field.replace(/[0-9]/g, '');
        return name && ctx.tomeMap.has(name) ? ctx.tomeMap.get(name) : noneByType[type];
    });
}

function localValidation(ctx, currentUrl, expectedClass, expectedWeapon) {
    const decoded = decodeSolverUrl(ctx, extractHash(currentUrl));
    const equipmentNames = decoded.equipment || [];
    const missingItems = equipmentNames.filter(name => name && !ctx.itemMap.has(name));
    const craftedItems = equipmentNames.filter(name => /^CR-/.test(name || ''));
    const actualWeapon = equipmentNames[8] || null;
    const activeNodes = decoded.playerClass
        ? decodeActiveNodes(ctx, decoded.playerClass, decoded.atree_data)
        : [];
    const apCost = activeNodes.reduce((sum, node) => sum + Number(node.cost || 0), 0);

    let skillpointsFeasible = null;
    let skillpointError = null;
    if (missingItems.length === 0 && equipmentNames.length >= 9) {
        try {
            const equipment = equipmentNames.slice(0, 8).map((name, index) => {
                const raw = name ? ctx.itemMap.get(name) : ctx.none_items[index];
                return ctx._apply_roll_mode_to_item(new ctx.Item(raw)).statMap;
            });
            const weapon = ctx._apply_roll_mode_to_item(new ctx.Item(ctx.itemMap.get(actualWeapon))).statMap;
            const tomeMaps = resolveTomes(ctx, decoded)
                .map(raw => ctx._apply_roll_mode_to_item(new ctx.Item(raw)).statMap);
            const guildIndex = ctx.tome_fields.indexOf('guildTome1');
            const wynnOrder = [
                equipment[3], equipment[2], equipment[1], equipment[0],
                equipment[4], equipment[5], equipment[6], equipment[7],
                tomeMaps[guildIndex],
            ];
            skillpointsFeasible = Boolean(ctx.calculate_skillpoints(
                wynnOrder,
                weapon,
                ctx.levelToSkillPoints(decoded.level),
            ));
        } catch (error) {
            skillpointError = error.message;
            skillpointsFeasible = false;
        }
    }

    return {
        decoded: true,
        encoded_version_id: decoded.versionId,
        level: decoded.level,
        player_class: decoded.playerClass,
        class_matches: normalize(decoded.playerClass) === normalize(expectedClass),
        equipment: equipmentNames,
        equipment_complete: equipmentNames.length >= 9 && equipmentNames.every(Boolean),
        missing_items: missingItems,
        crafted_items: craftedItems,
        weapon: actualWeapon,
        weapon_metadata_matches: weaponMetadataMatches(expectedWeapon, actualWeapon),
        active_node_count: activeNodes.length,
        active_node_ap_cost: apCost,
        skillpoints_feasible_at_median_ids: skillpointsFeasible,
        skillpoint_error: skillpointError,
    };
}

function main() {
    if (!(atreeCompletenessThreshold > 0 && atreeCompletenessThreshold <= 1)) {
        throw new Error('NORI_ATREE_COMPLETENESS must be greater than 0 and at most 1');
    }
    const source = JSON.parse(fs.readFileSync(sourcePath, 'utf8'));
    const browser = JSON.parse(fs.readFileSync(browserPath, 'utf8'));
    const browserByIndex = new Map(browser.results.map(result => [result.source_index, result]));
    const ctx = setupContext();
    const builds = [];

    for (const build of source.builds) {
        const live = browserByIndex.get(build.source_index);
        const apUtilization = live?.ap_cap > 0 ? live.ap_cost / live.ap_cap : 0;
        const structurallyValid = Boolean(live?.valid_current_builder && live?.ability_tree_valid);
        const treeComplete = structurallyValid && apUtilization >= atreeCompletenessThreshold;
        let local = null;
        let localError = null;
        if (live?.final_url && live.status !== 'not_wynnbuilder_link') {
            try {
                local = localValidation(ctx, live.final_url, build.class, build.weapon);
            } catch (error) {
                localError = error.message;
            }
        }

        const endgame = Number(local?.level) >= 100;
        const solverCompatible = Boolean(
            treeComplete
            && endgame
            && local
            && local.class_matches
            && local.equipment_complete
            && local.missing_items.length === 0
            && local.crafted_items.length === 0
            && local.skillpoints_feasible_at_median_ids
        );
        let compatibilityStatus;
        if (live?.status === 'not_wynnbuilder_link') compatibilityStatus = 'not_wynnbuilder_link';
        else if (live?.atree_warning) compatibilityStatus = 'broken_current_atree';
        else if (!structurallyValid) compatibilityStatus = 'current_load_failure';
        else if (!treeComplete) compatibilityStatus = 'migrated_incomplete_atree';
        else if (!local) compatibilityStatus = 'current_functional_builder_only';
        else if (!endgame) compatibilityStatus = 'current_functional_non_endgame';
        else if (!solverCompatible) compatibilityStatus = 'current_functional_builder_only';
        else compatibilityStatus = 'current_functional_solver_seed';

        builds.push({
            ...build,
            family_tags: classifyFamilies(build),
            current_validation: {
                compatibility_status: compatibilityStatus,
                current_game_version: live?.current_game_version || null,
                current_builder_url: live?.final_url || null,
                page_load_valid: Boolean(live?.valid_current_builder),
                ability_tree_structurally_valid: Boolean(live?.ability_tree_valid),
                ability_tree_completeness_proxy: apUtilization,
                ability_tree_completeness_threshold: atreeCompletenessThreshold,
                ability_tree_complete: treeComplete,
                ap_cost: live?.ap_cost ?? null,
                ap_cap: live?.ap_cap ?? null,
                atree_warning: live?.atree_warning || '',
                error_text: live?.error_text || '',
                navigation_error: live?.navigation_error || null,
                page_errors: live?.page_errors || [],
                version_update_prompted: Boolean(live?.version_update_prompted),
                beta_migration_prompted: Boolean(live?.beta_migration_prompted),
                endgame_level: endgame,
                solver_compatible: solverCompatible,
                local_data_version: LATEST_VERSION,
                local,
                local_error: localError,
            },
        });
    }

    const statuses = [...new Set(builds.map(build => build.current_validation.compatibility_status))].sort();
    const summary = {
        source_build_count: builds.length,
        current_game_version: browser.results.find(result => result.current_game_version)?.current_game_version || null,
        local_game_data_version: LATEST_VERSION,
        ability_tree_completeness_threshold: atreeCompletenessThreshold,
        counts_by_compatibility_status: Object.fromEntries(statuses.map(status => [
            status,
            builds.filter(build => build.current_validation.compatibility_status === status).length,
        ])),
        current_page_load_valid: builds.filter(build => build.current_validation.page_load_valid).length,
        current_structural_atree_valid: builds.filter(build => build.current_validation.ability_tree_structurally_valid).length,
        current_complete_atree: builds.filter(build => build.current_validation.ability_tree_complete).length,
        current_endgame_solver_compatible: builds.filter(build => build.current_validation.solver_compatible).length,
        local_decode_success: builds.filter(build => build.current_validation.local?.decoded).length,
        local_decode_failure: builds.filter(build => build.current_validation.local_error).length,
    };
    const output = {
        schema_version: 1,
        generated_at: new Date().toISOString(),
        source_snapshot: 'research/build-database/nori-builds.json',
        browser_snapshot: 'research/build-database/nori-browser-validation.json',
        methodology: {
            current_builder: 'Every WynnBuilder link was loaded in a fresh Playwright page. Beta links were redirected to the official site and every old-version migration prompt was accepted.',
            tree_acceptance: `No rendered ATree error, AP cost at or below cap, and at least ${atreeCompletenessThreshold * 100}% of AP retained. The AP threshold is a configurable completeness proxy, not a legality rule.`,
            solver_acceptance: 'Current tree acceptance plus level 100 or higher, current local decode, complete noncrafted equipment, matching class, and feasible skill points at median displayed ID rolls.',
        },
        summary,
        builds,
    };
    fs.writeFileSync(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
    const csvRows = builds.map(build => ({
        id: build.id,
        source_index: build.source_index,
        name: build.name,
        class: build.class,
        weapon: build.weapon,
        families: build.family_tags,
        compatibility_status: build.current_validation.compatibility_status,
        level: build.current_validation.local?.level,
        ap_cost: build.current_validation.ap_cost,
        ap_cap: build.current_validation.ap_cap,
        ap_completeness: build.current_validation.ability_tree_completeness_proxy,
        solver_compatible: build.current_validation.solver_compatible,
        median_id_skillpoints_feasible: build.current_validation.local?.skillpoints_feasible_at_median_ids,
        source_url: build.source_builder_url,
        current_builder_url: build.current_validation.current_builder_url,
        atree_warning: build.current_validation.atree_warning,
        local_error: build.current_validation.local_error,
    }));
    const headers = Object.keys(csvRows[0]);
    const lines = [headers.join(',')];
    for (const row of csvRows) lines.push(headers.map(header => csvEscape(row[header])).join(','));
    fs.writeFileSync(csvPath, `${lines.join('\n')}\n`, 'utf8');
    console.log(JSON.stringify(summary, null, 2));
}

main();
