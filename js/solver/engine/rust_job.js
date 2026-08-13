'use strict';

function _rust_jser(value) {
    if (value instanceof Map) {
        const object = {};
        for (const [key, entry] of value) object[String(key)] = _rust_jser(entry);
        return { __m: object };
    }
    if (value instanceof Set) return { __s: [...value].map(_rust_jser) };
    if (Array.isArray(value)) return value.map(_rust_jser);
    if (value && typeof value === 'object') {
        const object = {};
        for (const [key, entry] of Object.entries(value)) {
            if (entry !== undefined) object[key] = _rust_jser(entry);
        }
        return object;
    }
    return value === undefined ? null : value;
}

function _rust_number(value) {
    return typeof value === 'number' && Number.isFinite(value) ? String(value) : '0';
}

function _rust_item_name(item) {
    const statMap = item?.statMap ?? item;
    return statMap?.get?.('displayName') ?? statMap?.get?.('name') ?? '';
}

function _build_rust_enumeration_fixture(init, ring_pool, snap) {
    const lines = [];
    const staticIds = new Set(STATMAP_STATIC_IDS);
    const excluded = new Set([
        ...INDIRECT_CONSTRAINT_STATS,
        'str', 'dex', 'int', 'def', 'agi',
    ]);
    const none = (statMap) => !statMap || statMap.has('NONE');
    const locked = init.locked ?? {};
    const partial = {
        helmet: locked.helmet?.statMap,
        chestplate: locked.chestplate?.statMap,
        leggings: locked.leggings?.statMap,
        boots: locked.boots?.statMap,
        ring1: init.ring1_locked?.statMap,
        ring2: init.ring2_locked?.statMap,
        bracelet: locked.bracelet?.statMap,
        necklace: locked.necklace?.statMap,
    };
    const fixedStatMaps = Object.values(partial).filter((statMap) => !none(statMap));
    fixedStatMaps.push(...init.tome_sms, init.weapon_sm);
    const running = _init_running_statmap(init.level, fixedStatMaps);
    const fixedContribution = (stat) =>
        (snap.atree_raw?.get(stat) ?? 0) + (snap.static_boosts?.get(stat) ?? 0);

    const prechecks = [];
    let ehp = null;
    let ehpNoAgi = null;
    let totalHp = null;
    for (const { stat, op, value } of (snap.restrictions?.stat_thresholds ?? [])) {
        if (op !== 'ge') continue;
        if (stat === 'ehp' || stat === 'ehp_no_agi') {
            const fixedHp = fixedContribution('hpBonus');
            const defPct = skillPointsToPercentage(100) * skillpoint_final_mult[3];
            const defMult = 2 - (classDefenseMultipliers.get(init.weapon_sm.get('type')) || 1);
            if (stat === 'ehp') {
                const agiPct = skillPointsToPercentage(100) * skillpoint_final_mult[4];
                ehp = { threshold: value, fixedHp, divisor: (0.1 * agiPct + (1 - agiPct) * (1 - defPct)) * defMult };
            } else {
                ehpNoAgi = { threshold: value, fixedHp, divisor: (1 - defPct) * defMult };
            }
            continue;
        }
        if (stat === 'total_hp') {
            totalHp = { threshold: value, fixedHp: fixedContribution('hpBonus') };
            continue;
        }
        if (!excluded.has(stat)) {
            prechecks.push({
                stat,
                threshold: value - fixedContribution(stat),
                start: running.get(stat) ?? 0,
            });
        }
    }

    const setIds = new Map();
    const illegalIds = new Map();
    const idFor = (table, name) => {
        if (!name) return -1;
        if (!table.has(name)) table.set(name, table.size);
        return table.get(name);
    };
    const itemLine = (item) => {
        const statMap = item.statMap;
        const rolls = statMap.get('maxRolls');
        const valueFor = (stat) => staticIds.has(stat)
            ? (statMap.get(stat) || 0)
            : (rolls?.get(stat) || 0);
        const hp = (statMap.get('hp') || 0) + (rolls?.get('hpBonus') || 0);
        return [
            'ITEM', statMap.get('crafted') ? 1 : 0,
            ...statMap.get('reqs'), ...statMap.get('skillpoints'),
            idFor(setIds, statMap.get('crafted') ? null : statMap.get('set')),
            idFor(illegalIds, item._illegalSet), _rust_number(hp),
            ...prechecks.map((precheck) => _rust_number(valueFor(precheck.stat))),
        ].join(' ');
    };

    const pools = init.pools ?? {};
    const poolFor = (slot) => slot.startsWith('ring') ? ring_pool : pools[slot];
    const freeSlots = [];
    for (const slot of ['helmet', 'chestplate', 'leggings', 'boots', 'bracelet', 'necklace']) {
        if (!locked[slot]) freeSlots.push(slot);
    }
    if (!init.ring1_locked) freeSlots.push('ring1');
    if (!init.ring2_locked) freeSlots.push('ring2');
    freeSlots.sort((left, right) => {
        const sizeDifference = (poolFor(left)?.length ?? 0) - (poolFor(right)?.length ?? 0);
        if (sizeDifference !== 0) return sizeDifference;
        if (left === 'ring1' && right === 'ring2') return -1;
        if (left === 'ring2' && right === 'ring1') return 1;
        return 0;
    });
    const slotPositions = {
        helmet: 0, chestplate: 1, leggings: 2, boots: 3,
        ring1: 4, ring2: 5, bracelet: 6, necklace: 7,
    };

    lines.push(`BUDGET ${init.sp_budget}`);
    lines.push(`PRECHECKS ${prechecks.length}`);
    for (const precheck of prechecks) {
        lines.push(`PC ${precheck.stat} ${_rust_number(precheck.threshold)} ${_rust_number(precheck.start)}`);
    }
    lines.push(`EHP ${ehp ? 1 : 0} ${_rust_number(ehp?.threshold)} ${_rust_number(ehp?.fixedHp)} ${_rust_number(ehp?.divisor)}`);
    lines.push(`EHPNA ${ehpNoAgi ? 1 : 0} ${_rust_number(ehpNoAgi?.threshold)} ${_rust_number(ehpNoAgi?.fixedHp)} ${_rust_number(ehpNoAgi?.divisor)}`);
    lines.push(`THP ${totalHp ? 1 : 0} ${_rust_number(totalHp?.threshold)} ${_rust_number(totalHp?.fixedHp)}`);
    lines.push(`HPSTART ${_rust_number((running.get('hp') ?? 0) + (running.get('hpBonus') ?? 0))}`);
    lines.push(`WEAPON ${init.weapon_sm.get('reqs').join(' ')} ${init.weapon_sm.get('skillpoints').join(' ')}`);
    const guild = init.guild_tome_sm;
    const guildPresent = guild && !guild.has('NONE');
    lines.push(`GUILD ${guildPresent ? 1 : 0} ${guildPresent && guild.get('crafted') ? 1 : 0} `
        + `${(guildPresent ? guild.get('reqs') : [0, 0, 0, 0, 0]).join(' ')} `
        + `${(guildPresent ? guild.get('skillpoints') : [0, 0, 0, 0, 0]).join(' ')} `
        + `${guildPresent ? idFor(setIds, guild.get('set')) : -1}`);

    const fixedLines = [];
    for (const [slot, statMap] of Object.entries(partial)) {
        if (none(statMap)) continue;
        const wrapper = slot === 'ring1' ? init.ring1_locked
            : slot === 'ring2' ? init.ring2_locked : locked[slot];
        fixedLines.push(`FIXED ${slotPositions[slot]} ${statMap.get('crafted') ? 1 : 0} `
            + `${statMap.get('reqs').join(' ')} ${statMap.get('skillpoints').join(' ')} `
            + `${idFor(setIds, statMap.get('crafted') ? null : statMap.get('set'))} `
            + `${idFor(illegalIds, wrapper?._illegalSet)}`);
    }
    lines.push(`NFIXED ${fixedLines.length}`, ...fixedLines);
    lines.push(`NSLOTS ${freeSlots.length}`);
    for (const slot of freeSlots) {
        const pool = poolFor(slot) ?? [];
        lines.push(`SLOT ${slot} ${slotPositions[slot]} ${slot === 'ring1' ? 1 : 0} ${slot === 'ring2' ? 1 : 0} ${pool.length}`);
        for (const item of pool) lines.push(itemLine(item));
    }
    const setLines = [];
    for (const [name, id] of setIds) {
        const bonuses = sets.get(name)?.bonuses ?? [];
        const rows = bonuses.map((bonus) => skp_order.map((key) => bonus?.[key] || 0).join(' '));
        setLines.push(`SET ${id} ${rows.length} ${rows.join('  ')}`);
    }
    lines.push(`NSETS ${setIds.size}`, ...setLines, 'NAMES 1');
    for (let index = 0; index < freeSlots.length; index++) {
        const pool = poolFor(freeSlots[index]) ?? [];
        lines.push(`INAMES ${index} ${pool.length}`);
        for (const item of pool) lines.push(_rust_item_name(item));
    }
    const fixedNames = [];
    for (const [slot, statMap] of Object.entries(partial)) {
        if (!none(statMap)) fixedNames.push(`${slotPositions[slot]} ${_rust_item_name(statMap)}`);
    }
    lines.push(`FNAMES ${fixedNames.length}`, ...fixedNames, 'NONENAMES 8');
    for (let index = 0; index < 8; index++) lines.push(_rust_item_name(init.none_item_sms[index]));
    return lines.join('\n') + '\n';
}

function _build_rust_scaling_plan(init) {
    const atree = init.atree_merged;
    const analysis = atree_scaling_analysis(atree);
    const skipEdit = !analysis.has_prop_outputs;
    if (!analysis.stat_dependent) {
        const [, scaled] = atree_compute_scaling(
            atree, new Map(), init.button_states, init.slider_states, null, skipEdit,
        );
        return { kind: 'cached', scaled: _rust_jser(scaled) };
    }
    const plan = atree_collect_stat_effects(atree);
    if (!plan || plan.var_has_prop_io) return { kind: 'full' };
    const [, constantScaled] = atree_compute_scaling(
        atree, new Map(), init.button_states, init.slider_states, null, skipEdit, true,
    );
    for (const key of plan.var_keys) {
        if (key.includes('.') || ['damMult', 'defMult', 'healMult', 'manaMult'].includes(key)
            || constantScaled.has(key)) return { kind: 'full' };
    }
    const effects = plan.var_effects.map((effect) => {
        const terms = [];
        let constantAdd = 0;
        const scaling = effect.scaling ?? [0];
        const inputs = effect.inputs ?? [];
        for (let index = 0; index < Math.min(scaling.length, inputs.length); index++) {
            const factor = atree_translate(atree, scaling[index]);
            if (inputs[index].type === 'stat') terms.push({ stat: inputs[index].name, factor });
            else if (inputs[index].type === 'prop') {
                const ability = atree.get(inputs[index].abil);
                if (ability) constantAdd += ability.properties[inputs[index].name] * factor;
            }
        }
        const lowered = {
            round: effect.round ?? true,
            positive: effect.positive ?? true,
            terms,
            const_add: constantAdd,
            outputs: (Array.isArray(effect.output) ? effect.output : [effect.output])
                .filter((output) => output && output.type === 'stat')
                .map((output) => output.name),
        };
        if ('max' in effect) lowered.max = atree_translate(atree, effect.max);
        return lowered;
    });
    return { kind: 'split', const_scaled: _rust_jser(constantScaled), var_effects: effects };
}

function _build_rust_scoring_fixture(init, ring_pool) {
    const hitRefs = {};
    for (const [, ability] of (init.atree_merged ?? new Map())) {
        for (const effect of (ability.effects ?? [])) {
            if (effect.type === 'replace_spell') {
                for (const part of (effect.parts ?? [])) {
                    if (part && typeof part === 'object' && 'hits' in part) {
                        ((hitRefs[effect.base_spell] ??= {}))[part.name] = {
                            ...((hitRefs[effect.base_spell] ?? {})[part.name] ?? {}), ...part.hits,
                        };
                    }
                }
            } else if (effect.type === 'add_spell_prop' && effect.target_part && 'hits' in effect) {
                ((hitRefs[effect.base_spell] ??= {}))[effect.target_part] = {
                    ...((hitRefs[effect.base_spell] ?? {})[effect.target_part] ?? {}), ...effect.hits,
                };
            }
        }
    }
    const itemRegistry = {};
    const register = (item) => {
        const statMap = item?.statMap ?? item;
        if (!statMap?.get) return;
        const name = _rust_item_name(statMap);
        if (name && !(name in itemRegistry)) itemRegistry[name] = _rust_jser(statMap);
    };
    for (const pool of Object.values(init.pools)) for (const item of pool) register(item);
    for (const item of ring_pool) register(item);
    for (const item of Object.values(init.locked ?? {})) register(item);
    register(init.ring1_locked);
    register(init.ring2_locked);

    return {
        meta: { generated: 'solver_build_rust_search_job', cases: 0, scoring_target: init.scoring_target },
        tables: {
            skillpoint_damage_mult: [...skillpoint_damage_mult],
            skillpoint_final_mult: [...skillpoint_final_mult],
            baseDamageMultiplier: [...baseDamageMultiplier],
            attackSpeeds: [...attackSpeeds],
            damage_keys: [...damage_keys],
            sp_percentage_rate: SP_PERCENTAGE_RATE,
            sp_percentage_input_cap: SP_PERCENTAGE_INPUT_CAP,
            sp_pct_table: Array.from(
                { length: SP_PERCENTAGE_INPUT_CAP + 1 }, (_, index) => skillPointsToPercentage(index),
            ),
        },
        weapon_sm: _rust_jser(init.weapon_sm),
        parsed_combo: _rust_jser(init.parsed_combo),
        boost_registry: _rust_jser(init.boost_registry),
        atree_hit_refs: _rust_jser(hitRefs),
        layer2: {
            level: init.level,
            sp_budget: init.sp_budget,
            combo_time: init.combo_time,
            allow_downtime: init.allow_downtime,
            hp_casting: init.hp_casting,
            health_config: _rust_jser(init.health_config),
            tome_sms: _rust_jser(init.tome_sms),
            guild_tome_sm: _rust_jser(init.guild_tome_sm),
            none_item_sms: _rust_jser(init.none_item_sms),
            none_idx_map: init.none_idx_map,
            sets_data: _rust_jser(new Map(init.sets_data)),
            atree_raw: _rust_jser(init.atree_raw),
            atree_merged: _rust_jser(init.atree_merged),
            button_states: _rust_jser(init.button_states),
            slider_states: _rust_jser(init.slider_states),
            static_boosts: _rust_jser(init.static_boosts),
            radiance_boost: _rust_jser(init.radiance_boost ?? null),
            spell_base_costs: _rust_jser(init.spell_base_costs ?? null),
            restrictions: init.restrictions ?? { stat_thresholds: [] },
            custom_weights: _rust_jser(init.custom_weights ?? null),
            scaling_plan: _build_rust_scaling_plan(init),
            constants: {
                statmap_static_ids: [...STATMAP_STATIC_IDS],
                statmap_must_ids: [...STATMAP_MUST_IDS],
                hp_base_for_level: levelToHPBase(init.level),
                class_def: Object.fromEntries(classDefenseMultipliers),
                base_mana_regen: BASE_MANA_REGEN,
                mana_tick_seconds: MANA_TICK_SECONDS,
                spell_cast_time: SPELL_CAST_TIME,
                spell_cast_delay: SPELL_CAST_DELAY,
                skp_order: [...skp_order],
            },
            item_registry: itemRegistry,
        },
        cases: [],
    };
}

function solver_build_rust_search_job(init, ring_pool, snap) {
    if (typeof wynn_version_id === 'undefined' || !Number.isInteger(wynn_version_id)) {
        throw new Error('The active Wynn data version is unavailable');
    }
    const versionName = typeof wynn_version_names !== 'undefined'
        ? wynn_version_names[wynn_version_id]
        : null;
    const dataVersion = versionName
        ? `wynn-${wynn_version_id}:${versionName}`
        : `wynn-${wynn_version_id}`;
    return {
        schema_version: 1,
        data_version: dataVersion,
        enumeration_fixture: _build_rust_enumeration_fixture(init, ring_pool, snap),
        scoring_fixture: _build_rust_scoring_fixture(init, ring_pool),
    };
}
