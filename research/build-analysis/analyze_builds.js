'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');

const {
    createSandbox,
    loadGameData,
    decodeSolverUrl,
    decodeActiveNodes,
    buildAtreeMerged,
    collectSpells,
    collectRawStats,
    extractAtreeInteractiveDefaults,
    REPO_ROOT,
    LATEST_VERSION,
} = require('../../js/solver/tests/harness');

const INPUT_PATH = path.join(REPO_ROOT, 'research', 'build-database', 'functional-builds.json');
const OUTPUT_DIR = path.join(REPO_ROOT, 'research', 'build-analysis');

const ROLL_PROFILES = {
    median: { damage: 50, mana: 50, healing: 50, misc: 50 },
    upper_quartile_proxy: { damage: 75, mana: 75, healing: 75, misc: 75 },
    solver_default: { damage: 85, mana: 100, healing: 85, misc: 85 },
    maximum: { damage: 100, mana: 100, healing: 100, misc: 100 },
};

const FAMILY_PRIORITY = [
    'family_heavy_melee',
    'family_tierstack',
    'family_cancelstack',
    'family_spellsteal',
    'family_spell_sustained',
    'family_hybrid',
    'family_poison',
    'family_support',
    'family_lootrun',
];

const SUMMARY_METRICS = [
    'total_hp', 'ehp', 'ehp_no_agi', 'hpr', 'ehpr', 'ls', 'effective_ls',
    'mr', 'total_mr', 'ms', 'mana_per_hit', 'total_mana', 'walk_speed',
    'attack_tier', 'attacks_per_second', 'poison', 'main_attack_range_pct',
    'main_attack_range_blocks', 'main_attack_dps', 'spell_display_avg_max',
    'spell_display_avg_sum', 'heal_pct', 'skill_points_assigned',
];

const THRESHOLD_METRICS = [
    'ehp', 'ehp_no_agi', 'total_hp', 'hpr', 'ls', 'effective_ls', 'total_mr',
    'ms', 'total_mana', 'main_attack_range_pct', 'main_attack_dps',
    'spell_display_avg_max', 'poison',
];

function extractHash(url) {
    const index = url.indexOf('#');
    return index >= 0 ? url.slice(index + 1) : url;
}

function parsePowders(ctx, raw) {
    if (Array.isArray(raw)) return raw;
    if (typeof raw !== 'string' || raw.length === 0) return [];
    const ids = [];
    for (let i = 0; i + 1 < raw.length; i += 2) {
        const id = ctx.powderIDs.get(raw.slice(i, i + 2));
        if (id !== undefined) ids.push(id);
    }
    return ids;
}

function finite(value) {
    return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function round(value, digits = 4) {
    if (!Number.isFinite(value)) return null;
    const scale = 10 ** digits;
    return Math.round(value * scale) / scale;
}

function quantile(sorted, q) {
    if (sorted.length === 0) return null;
    if (sorted.length === 1) return sorted[0];
    const index = (sorted.length - 1) * q;
    const lower = Math.floor(index);
    const upper = Math.ceil(index);
    if (lower === upper) return sorted[lower];
    return sorted[lower] + (sorted[upper] - sorted[lower]) * (index - lower);
}

function summarize(values) {
    const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
    if (sorted.length === 0) return null;
    const mean = sorted.reduce((sum, value) => sum + value, 0) / sorted.length;
    const variance = sorted.reduce((sum, value) => sum + (value - mean) ** 2, 0) / sorted.length;
    const median = quantile(sorted, 0.5);
    const deviations = sorted.map(value => Math.abs(value - median)).sort((a, b) => a - b);
    return {
        n: sorted.length,
        min: round(sorted[0]),
        p10: round(quantile(sorted, 0.10)),
        p25: round(quantile(sorted, 0.25)),
        median: round(median),
        p75: round(quantile(sorted, 0.75)),
        p90: round(quantile(sorted, 0.90)),
        max: round(sorted[sorted.length - 1]),
        mean: round(mean),
        sample_sd: sorted.length > 1
            ? round(Math.sqrt(variance * sorted.length / (sorted.length - 1)))
            : 0,
        mad: round(quantile(deviations, 0.5)),
    };
}

function familyIds(build) {
    return (build.threshold_profile_ids || []).filter(id => id.startsWith('family_'));
}

function primaryFamily(build) {
    const ids = new Set(familyIds(build));
    return FAMILY_PRIORITY.find(id => ids.has(id)) || 'family_unclassified';
}

function setupContext() {
    const ctx = createSandbox();
    loadGameData(ctx);
    const searchPath = path.join(REPO_ROOT, 'js', 'solver', 'engine', 'search.js');
    vm.runInContext(fs.readFileSync(searchPath, 'utf8'), ctx, { filename: searchPath });
    vm.runInContext(`
        globalThis.tomeRedirectMap = new Map();
        globalThis._apply_roll_mode_to_item = _apply_roll_mode_to_item;
        globalThis.tomeFieldsExport = [...tome_fields];
        globalThis.attackSpeedsExport = [...attackSpeeds];
        globalThis.baseDamageMultiplierExport = [...baseDamageMultiplier];
        globalThis.classDefenseMultipliersExport = new Map(classDefenseMultipliers);
    `, ctx);
    ctx.tome_fields = ctx.tomeFieldsExport;
    return ctx;
}

function resolveTomes(ctx, decoded) {
    const names = decoded.tomes || [];
    const noneByType = {};
    for (const tome of ctx.none_tomes) noneByType[tome.type] = tome;
    return ctx.tome_fields.map((field, index) => {
        const name = names[index];
        const type = field.replace(/[0-9]/g, '');
        return name && ctx.tomeMap.has(name)
            ? ctx.tomeMap.get(name)
            : (noneByType[type] || ctx.none_tomes[0]);
    });
}

function applyProfile(ctx, profile) {
    vm.runInContext(`current_roll_mode = ${JSON.stringify(profile)}`, ctx);
}

function itemWithRolls(ctx, rawItem) {
    return ctx._apply_roll_mode_to_item(new ctx.Item(rawItem));
}

function extractOne(ctx, build, profileName, profile) {
    applyProfile(ctx, profile);
    const decoded = decodeSolverUrl(ctx, extractHash(build.builder_url));
    if (!decoded.playerClass) throw new Error('could not infer class from decoded weapon');

    const rawEquipment = decoded.equipment.slice(0, 8).map((name, index) => {
        if (!name) return ctx.none_items[index];
        const item = ctx.itemMap.get(name);
        if (!item) throw new Error(`item not present in local data: ${name}`);
        return item;
    });
    const weaponName = decoded.equipment[8];
    const rawWeapon = weaponName ? ctx.itemMap.get(weaponName) : null;
    if (!rawWeapon) throw new Error(`weapon not present in local data: ${weaponName}`);

    const equipItems = rawEquipment.map(item => itemWithRolls(ctx, item));
    for (let index = 0; index < 4; index++) {
        const powders = parsePowders(ctx, decoded.powders?.[index]);
        if (powders.length > 0) equipItems[index].statMap.set('powders', powders);
    }
    const weaponItem = itemWithRolls(ctx, rawWeapon);
    const weaponPowders = parsePowders(ctx, decoded.powders?.[4]);
    weaponItem.statMap.set('powders', weaponPowders);
    ctx.apply_weapon_powders(weaponItem.statMap);

    const rawTomes = resolveTomes(ctx, decoded);
    const tomeItems = rawTomes.map(item => itemWithRolls(ctx, item));
    const guildTomeIndex = ctx.tome_fields.indexOf('guildTome1');
    const wynnOrder = [
        equipItems[3], equipItems[2], equipItems[1], equipItems[0],
        equipItems[4], equipItems[5], equipItems[6], equipItems[7],
        tomeItems[guildTomeIndex],
    ].map(item => item.statMap);

    const spResult = ctx.calculate_skillpoints(
        wynnOrder,
        weaponItem.statMap,
        ctx.levelToSkillPoints(decoded.level),
    );
    if (!spResult) throw new Error('skill-point requirements are infeasible under this roll profile');
    const [assignedSp, totalSp, , activeSetCounts] = spResult;

    const equipMaps = equipItems.map(item => item.statMap);
    const tomeMaps = tomeItems.map(item => item.statMap);
    const lockedMaps = [weaponItem.statMap, ...tomeMaps];
    const running = ctx._init_running_statmap(decoded.level, lockedMaps);
    for (const statMap of equipMaps) ctx._incr_add_item(running, statMap);
    const stats = ctx._finalize_leaf_statmap(
        running,
        weaponItem.statMap,
        activeSetCounts || new Map(),
        ctx.sets,
        [...equipMaps, ...tomeMaps, weaponItem.statMap],
        null,
        null,
    );
    for (let index = 0; index < 5; index++) stats.set(ctx.skp_order[index], totalSp[index]);
    const weaponType = weaponItem.statMap.get('type');
    stats.set('classDef', ctx.classDefenseMultipliersExport.get(weaponType) || 1);

    const activeNodes = decodeActiveNodes(ctx, decoded.playerClass, decoded.atree_data);
    const atree = buildAtreeMerged(ctx, decoded.playerClass, activeNodes, stats, decoded.aspects);
    for (const [key, value] of collectRawStats(ctx, atree)) ctx.merge_stat(stats, key, value);

    let scalingStatus = 'applied_defaults';
    try {
        const defaults = extractAtreeInteractiveDefaults(atree);
        const [, scaled] = ctx.atree_compute_scaling(
            atree,
            stats,
            new Map(defaults.button_states),
            new Map(defaults.slider_states),
        );
        for (const [key, value] of scaled) ctx.merge_stat(stats, key, value);
    } catch (error) {
        scalingStatus = `raw_only: ${error.message}`;
    }

    const defense = ctx.getDefenseStats(stats);
    const attackIndex = Math.max(0, Math.min(
        6,
        ctx.attackSpeedsExport.indexOf(stats.get('atkSpd')) + (stats.get('atkTier') || 0),
    ));
    const attacksPerSecond = ctx.baseDamageMultiplierExport[attackIndex];
    const totalHp = defense[0];
    const lifeSteal = stats.get('ls') || 0;
    const manaSteal = stats.get('ms') || 0;
    const intMana = Math.floor(ctx.skillPointsToPercentage(stats.get('int') || 0) * 100);

    const spellMap = collectSpells(ctx, atree);
    const critChance = ctx.skillPointsToPercentage(stats.get('dex') || 0);
    const spellDamage = {};
    const spellCosts = {};
    for (let baseSpell = 0; baseSpell <= 4; baseSpell++) {
        const spell = spellMap.get(baseSpell);
        if (!spell) continue;
        spellDamage[baseSpell] = round(ctx.computeSpellDisplayAvg(
            stats,
            weaponItem.statMap,
            spell,
            critChance,
        ));
        if (baseSpell > 0 && spell.cost != null) {
            spellCosts[baseSpell] = round(ctx.getSpellCost(stats, spell));
        }
    }
    const nonMeleeSpellValues = Object.entries(spellDamage)
        .filter(([key, value]) => key !== '0' && Number.isFinite(value))
        .map(([, value]) => value);

    const baseMeleeRange = atree.get(999)?.properties?.range ?? null;
    const rangePct = stats.get('mainAttackRange') || 0;
    const elementalDefense = defense[5] || [];

    return {
        build_id: build.id,
        class: build.class,
        weapon: build.weapon,
        weapon_type: weaponType,
        archetype: build.archetype || null,
        variant: build.variant,
        crafted: build.crafted,
        primary_family: primaryFamily(build),
        family_ids: familyIds(build),
        engine_tags: build.engine_tags || [],
        roll_profile: profileName,
        roll_groups: profile,
        local_data_version: LATEST_VERSION,
        source_url: build.builder_url,
        decoded_weapon: weaponName,
        active_node_count: activeNodes.length,
        aspect_count: (decoded.aspects || []).filter(Boolean).length,
        atree_scaling_status: scalingStatus,
        equipment: decoded.equipment,
        metrics: {
            total_hp: round(totalHp),
            ehp: round(defense[1][0]),
            ehp_no_agi: round(defense[1][1]),
            hpr: round(defense[2]),
            ehpr: round(defense[3][0]),
            defense_pct: round(defense[4][0]),
            agility_pct: round(defense[4][1]),
            earth_defense: round(elementalDefense[0] || 0),
            thunder_defense: round(elementalDefense[1] || 0),
            water_defense: round(elementalDefense[2] || 0),
            fire_defense: round(elementalDefense[3] || 0),
            air_defense: round(elementalDefense[4] || 0),
            hpr_raw: finite(stats.get('hprRaw') || 0),
            hpr_pct: finite(stats.get('hprPct') || 0),
            ls: finite(lifeSteal),
            effective_ls: round(totalHp > 0 ? defense[1][0] * lifeSteal / totalHp : 0),
            life_per_hit: round(lifeSteal / 3 / attacksPerSecond),
            mr: finite(stats.get('mr') || 0),
            total_mr: finite((stats.get('mr') || 0) + 25),
            ms: finite(manaSteal),
            mana_per_hit: round(manaSteal / 3 / attacksPerSecond),
            max_mana_bonus: finite(stats.get('maxMana') || 0),
            total_mana: finite(100 + (stats.get('maxMana') || 0) + intMana),
            walk_speed: finite(stats.get('spd') || 0),
            attack_tier: finite(stats.get('atkTier') || 0),
            attack_speed_base: stats.get('atkSpd'),
            attack_speed_final: ctx.attackSpeedsExport[attackIndex],
            attacks_per_second: attacksPerSecond,
            poison: finite(stats.get('poison') || 0),
            heal_pct: finite(stats.get('healPct') || 0),
            main_attack_range_pct: finite(rangePct),
            main_attack_range_blocks: baseMeleeRange == null
                ? null
                : round(baseMeleeRange * (1 + rangePct / 100)),
            main_attack_dps: spellDamage[0] ?? null,
            spell_1_display_avg: spellDamage[1] ?? null,
            spell_2_display_avg: spellDamage[2] ?? null,
            spell_3_display_avg: spellDamage[3] ?? null,
            spell_4_display_avg: spellDamage[4] ?? null,
            spell_display_avg_max: nonMeleeSpellValues.length
                ? Math.max(...nonMeleeSpellValues)
                : null,
            spell_display_avg_sum: nonMeleeSpellValues.length
                ? round(nonMeleeSpellValues.reduce((sum, value) => sum + value, 0))
                : null,
            spell_1_cost: spellCosts[1] ?? null,
            spell_2_cost: spellCosts[2] ?? null,
            spell_3_cost: spellCosts[3] ?? null,
            spell_4_cost: spellCosts[4] ?? null,
            skill_points_assigned: assignedSp.reduce((sum, value) => sum + value, 0),
            strength: totalSp[0],
            dexterity: totalSp[1],
            intelligence: totalSp[2],
            defense: totalSp[3],
            agility: totalSp[4],
            md_pct: finite(stats.get('mdPct') || 0),
            md_raw: finite(stats.get('mdRaw') || 0),
            sd_pct: finite(stats.get('sdPct') || 0),
            sd_raw: finite(stats.get('sdRaw') || 0),
            damage_pct: finite(stats.get('damPct') || 0),
            damage_raw: finite(stats.get('damRaw') || 0),
        },
    };
}

function groupSummaries(rows) {
    const groups = new Map();
    for (const row of rows) {
        for (const scope of [
            ['class', row.class],
            ['family', row.primary_family],
            ['family_class', `${row.primary_family}:${row.class}`],
            ['archetype', row.archetype || 'none'],
            ['archetype_class', `${row.archetype || 'none'}:${row.class}`],
        ]) {
            const key = `${scope[0]}|${scope[1]}|${row.roll_profile}`;
            if (!groups.has(key)) groups.set(key, { scope: scope[0], group: scope[1], roll_profile: row.roll_profile, rows: [] });
            groups.get(key).rows.push(row);
        }
    }

    return [...groups.values()].map(group => {
        const metrics = {};
        for (const metric of SUMMARY_METRICS) {
            metrics[metric] = summarize(group.rows.map(row => row.metrics[metric]));
        }
        return {
            scope: group.scope,
            group: group.group,
            roll_profile: group.roll_profile,
            build_count: group.rows.length,
            build_ids: group.rows.map(row => row.build_id),
            metrics,
        };
    });
}

function thresholdCandidates(summaries) {
    return summaries
        .filter(summary => summary.scope === 'family' && summary.roll_profile === 'median')
        .map(summary => {
            const thresholds = {};
            for (const metric of THRESHOLD_METRICS) {
                const stats = summary.metrics[metric];
                if (!stats) continue;
                thresholds[metric] = {
                    observed_floor: stats.min,
                    robust_floor_p10: stats.p10,
                    typical_p50: stats.median,
                    strong_p75: stats.p75,
                };
            }
            return {
                family: summary.group,
                build_count: summary.build_count,
                status: summary.build_count >= 8
                    ? 'provisional'
                    : summary.build_count >= 4
                        ? 'exploratory'
                        : 'insufficient_sample',
                roll_profile: 'median',
                use: 'Use p10 as a permissive regression guardrail, p50 as a typical target, and p75 as a strong target. The median profile evaluates every rolled ID at the midpoint of its displayed range. Do not treat these curated-sample quantiles as population percentiles.',
                thresholds,
            };
        });
}

function rollSensitivity(rows) {
    const byBuild = new Map();
    for (const row of rows) {
        if (!byBuild.has(row.build_id)) byBuild.set(row.build_id, new Map());
        byBuild.get(row.build_id).set(row.roll_profile, row);
    }
    const metrics = ['ehp', 'ehp_no_agi', 'hpr', 'ls', 'total_mr', 'ms', 'main_attack_dps', 'spell_display_avg_max'];
    const buildRows = [];
    for (const [buildId, profiles] of byBuild) {
        const median = profiles.get('median');
        const upper = profiles.get('upper_quartile_proxy');
        const maximum = profiles.get('maximum');
        if (!median || !upper || !maximum) continue;
        const metricRows = {};
        for (const metric of metrics) {
            const medianValue = median.metrics[metric];
            const upperValue = upper.metrics[metric];
            const maximumValue = maximum.metrics[metric];
            metricRows[metric] = {
                median: medianValue,
                upper_quartile_proxy: upperValue,
                maximum: maximumValue,
                upper_quartile_minus_median: Number.isFinite(upperValue) && Number.isFinite(medianValue)
                    ? round(upperValue - medianValue)
                    : null,
                maximum_minus_upper_quartile: Number.isFinite(maximumValue) && Number.isFinite(upperValue)
                    ? round(maximumValue - upperValue)
                    : null,
                upper_quartile_vs_median_pct: Number.isFinite(upperValue) && Number.isFinite(medianValue) && medianValue !== 0
                    ? round((upperValue / medianValue - 1) * 100)
                    : null,
                maximum_vs_upper_quartile_pct: Number.isFinite(maximumValue) && Number.isFinite(upperValue) && upperValue !== 0
                    ? round((maximumValue / upperValue - 1) * 100)
                    : null,
            };
        }
        buildRows.push({
            build_id: buildId,
            class: upper.class,
            weapon: upper.weapon,
            archetype: upper.archetype,
            primary_family: upper.primary_family,
            metrics: metricRows,
        });
    }

    const familyGroups = new Map();
    for (const row of buildRows) {
        if (!familyGroups.has(row.primary_family)) familyGroups.set(row.primary_family, []);
        familyGroups.get(row.primary_family).push(row);
    }
    const familySummaries = [...familyGroups].map(([family, familyRows]) => {
        const metricSummaries = {};
        for (const metric of metrics) {
            metricSummaries[metric] = {
                upper_quartile_vs_median_pct: summarize(familyRows.map(row => row.metrics[metric].upper_quartile_vs_median_pct)),
                maximum_vs_upper_quartile_pct: summarize(familyRows.map(row => row.metrics[metric].maximum_vs_upper_quartile_pct)),
                maximum_minus_upper_quartile: summarize(familyRows.map(row => row.metrics[metric].maximum_minus_upper_quartile)),
            };
        }
        return { family, build_count: familyRows.length, metrics: metricSummaries };
    });
    return { build_rows: buildRows, family_summaries: familySummaries };
}

function csvEscape(value) {
    if (value == null) return '';
    const text = typeof value === 'object' ? JSON.stringify(value) : String(value);
    return /[",\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

function writeCsv(filePath, rows) {
    if (rows.length === 0) return fs.writeFileSync(filePath, '', 'utf8');
    const headers = [...new Set(rows.flatMap(row => Object.keys(row)))];
    const lines = [headers.map(csvEscape).join(',')];
    for (const row of rows) lines.push(headers.map(header => csvEscape(row[header])).join(','));
    fs.writeFileSync(filePath, `${lines.join('\n')}\n`, 'utf8');
}

function main() {
    const ctx = setupContext();
    const database = JSON.parse(fs.readFileSync(INPUT_PATH, 'utf8'));
    const rows = [];
    const failures = [];

    for (const build of database.builds) {
        for (const [profileName, profile] of Object.entries(ROLL_PROFILES)) {
            try {
                rows.push(extractOne(ctx, build, profileName, profile));
            } catch (error) {
                failures.push({
                    build_id: build.id,
                    weapon: build.weapon,
                    class: build.class,
                    crafted: build.crafted,
                    roll_profile: profileName,
                    source_url: build.builder_url,
                    error: error.message,
                });
            }
        }
    }

    const summaries = groupSummaries(rows);
    const thresholds = thresholdCandidates(summaries);
    const sensitivity = rollSensitivity(rows);
    const generatedAt = new Date().toISOString();
    const metadata = {
        schema_version: 1,
        generated_at: generatedAt,
        source_build_database: path.relative(REPO_ROOT, INPUT_PATH).replace(/\\/g, '/'),
        local_game_data_version: LATEST_VERSION,
        source_build_count: database.builds.length,
        evaluated_build_profile_rows: rows.length,
        unique_builds_evaluated: new Set(rows.map(row => row.build_id)).size,
        failure_count: failures.length,
        roll_profiles: ROLL_PROFILES,
        warning: 'The source is a curated forum catalogue, not a random population sample. Quantiles are empirical descriptions and provisional optimizer guardrails, not estimates of a normally distributed player-build population.',
    };

    fs.mkdirSync(OUTPUT_DIR, { recursive: true });
    fs.writeFileSync(path.join(OUTPUT_DIR, 'build-stats.json'), JSON.stringify({ metadata, rows, failures }, null, 2) + '\n');
    fs.writeFileSync(path.join(OUTPUT_DIR, 'family-summary.json'), JSON.stringify({ metadata, summaries }, null, 2) + '\n');
    fs.writeFileSync(path.join(OUTPUT_DIR, 'threshold-candidates.json'), JSON.stringify({ metadata, candidates: thresholds }, null, 2) + '\n');
    fs.writeFileSync(path.join(OUTPUT_DIR, 'roll-sensitivity.json'), JSON.stringify({ metadata, ...sensitivity }, null, 2) + '\n');

    writeCsv(path.join(OUTPUT_DIR, 'build-stats.csv'), rows.map(row => ({
        build_id: row.build_id,
        class: row.class,
        weapon: row.weapon,
        archetype: row.archetype,
        variant: row.variant,
        primary_family: row.primary_family,
        family_ids: row.family_ids,
        roll_profile: row.roll_profile,
        ...row.metrics,
        source_url: row.source_url,
    })));
    writeCsv(path.join(OUTPUT_DIR, 'family-summary.csv'), summaries.flatMap(summary =>
        SUMMARY_METRICS.map(metric => ({
            scope: summary.scope,
            group: summary.group,
            roll_profile: summary.roll_profile,
            build_count: summary.build_count,
            metric,
            ...(summary.metrics[metric] || {}),
        })),
    ));
    writeCsv(path.join(OUTPUT_DIR, 'roll-sensitivity.csv'), sensitivity.build_rows.flatMap(row =>
        Object.entries(row.metrics).map(([metric, values]) => ({
            build_id: row.build_id,
            class: row.class,
            weapon: row.weapon,
            archetype: row.archetype,
            primary_family: row.primary_family,
            metric,
            ...values,
        })),
    ));

    console.log(JSON.stringify(metadata, null, 2));
}

main();
