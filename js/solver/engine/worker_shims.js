// ══════════════════════════════════════════════════════════════════════════════
// SOLVER WORKER HELPERS
// Worker-only code that cannot be shared with the main thread.
//
// Dependencies (loaded via importScripts before this file):
//   - utils.js:       zip2, round_near, clamp, rawToPct, rawToPctUncapped, etc.
//   - build_utils.js: merge_stat, skp_order, skp_elements, skillPointsToPercentage,
//                     skillpoint_final_mult, reversedIDs, levelToHPBase,
//                     STATMAP_STATIC_IDS, STATMAP_STATIC_ID_SET,
//                     createBaseStatmap, applySetBonuses, finalizeStatmap
//   - damage_calc.js: calculateSpellDamage
//   - shared_game_stats.js: classDefenseMultipliers, damageMultipliers,
//                           specialNames, radiance_affected, getDefenseStats,
//                           getBaseSpellCost, getSpellCost
//   - pure/spell.js:  computeSpellDisplayAvg, spell_has_heal,
//                     computeSpellHealingTotal
//   - pure/boost.js:  find_all_matching_boosts, apply_combo_row_boosts,
//                     apply_spell_prop_overrides
//   - pure/utils.js:  atree_compute_scaling, atree_translate,
//                     _deep_clone_statmap, _merge_into,
//                     _apply_radiance_scale
// ══════════════════════════════════════════════════════════════════════════════

// worker_atree_scaling and atree_translate moved to pure/utils.js (atree_compute_scaling)

// ── Build stat assembly (replaces Build.initBuildStats without DOM) ─────────

// ── Incremental stat accumulation helpers ────────────────────────────────────
// Uses STATMAP_STATIC_IDS / STATMAP_STATIC_ID_SET from build_utils.js

const _INCR_ENTRY_CACHE = new WeakMap();
let _INCR_CACHE_ENABLED = true;
let _INCR_PARALLEL_LAYOUT = true;
function _set_incr_cache_enabled(enabled) { _INCR_CACHE_ENABLED = enabled; }
function _set_incr_parallel_layout(enabled) { _INCR_PARALLEL_LAYOUT = enabled; }

function _init_running_stats_compact(statmap) {
    const compact = Object.create(null);
    for (const [id, value] of statmap) compact[id] = value;
    return compact;
}

function _materialize_running_statmap(compact, target) {
    target.clear();
    for (const id of Object.keys(compact)) target.set(id, compact[id]);
    return target;
}

/** Compile the additive portion of an immutable item statMap once. */
function _get_incr_entries(item_sm) {
    let entries = _INCR_ENTRY_CACHE.get(item_sm);
    if (entries) return entries;
    const keys = [];
    const values = [];
    const maxRolls = item_sm.get('maxRolls');
    if (maxRolls) {
        for (const [id, value] of maxRolls) {
            if (!STATMAP_STATIC_ID_SET.has(id) && value) {
                keys.push(id);
                values.push(value);
            }
        }
    }
    for (let i = 0; i < STATMAP_STATIC_IDS.length; i++) {
        const id = STATMAP_STATIC_IDS[i];
        const value = item_sm.get(id);
        if (value) {
            keys.push(id);
            values.push(value);
        }
    }
    const pairs = keys.map((key, i) => Object.freeze([key, values[i]]));
    entries = Object.freeze({
        keys: Object.freeze(keys), values: Object.freeze(values), pairs: Object.freeze(pairs),
    });
    _INCR_ENTRY_CACHE.set(item_sm, entries);
    return entries;
}

/**
 * Add an item's stats to a running statMap (incremental accumulation).
 * Only handles additive stats (staticIDs + maxRolls). damMult/defMult/healMult
 * are set up at the leaf, not during incremental search.
 */
function _incr_add_item(running_sm, item_sm) {
    if (!_INCR_CACHE_ENABLED) {
        const maxRolls = item_sm.get('maxRolls');
        if (maxRolls) for (const [id, value] of maxRolls) {
            if (!STATMAP_STATIC_ID_SET.has(id)) running_sm.set(id, (running_sm.get(id) || 0) + value);
        }
        for (let i = 0; i < STATMAP_STATIC_IDS.length; i++) {
            const id = STATMAP_STATIC_IDS[i], value = item_sm.get(id);
            if (value) running_sm.set(id, (running_sm.get(id) || 0) + value);
        }
        return;
    }
    const entries = _get_incr_entries(item_sm);
    const keys = entries.keys, values = entries.values;
    if (!_INCR_PARALLEL_LAYOUT) {
        const pairs = entries.pairs;
        for (let i = 0; i < pairs.length; i++) {
            const pair = pairs[i];
            running_sm.set(pair[0], (running_sm.get(pair[0]) || 0) + pair[1]);
        }
        return;
    }
    if (!(running_sm instanceof Map)) {
        for (let i = 0; i < keys.length; i++) {
            const id = keys[i];
            running_sm[id] = (running_sm[id] || 0) + values[i];
        }
        return;
    }
    for (let i = 0; i < keys.length; i++) {
        const id = keys[i];
        running_sm.set(id, (running_sm.get(id) || 0) + values[i]);
    }
}

/**
 * Remove an item's stats from a running statMap (backtrack).
 * Exact inverse of _incr_add_item.
 */
function _incr_remove_item(running_sm, item_sm) {
    if (!_INCR_CACHE_ENABLED) {
        const maxRolls = item_sm.get('maxRolls');
        if (maxRolls) for (const [id, value] of maxRolls) {
            if (!STATMAP_STATIC_ID_SET.has(id)) running_sm.set(id, (running_sm.get(id) || 0) - value);
        }
        for (let i = 0; i < STATMAP_STATIC_IDS.length; i++) {
            const id = STATMAP_STATIC_IDS[i], value = item_sm.get(id);
            if (value) running_sm.set(id, (running_sm.get(id) || 0) - value);
        }
        return;
    }
    const entries = _get_incr_entries(item_sm);
    const keys = entries.keys, values = entries.values;
    if (!_INCR_PARALLEL_LAYOUT) {
        const pairs = entries.pairs;
        for (let i = 0; i < pairs.length; i++) {
            const pair = pairs[i];
            running_sm.set(pair[0], (running_sm.get(pair[0]) || 0) - pair[1]);
        }
        return;
    }
    if (!(running_sm instanceof Map)) {
        for (let i = 0; i < keys.length; i++) {
            const id = keys[i];
            running_sm[id] = (running_sm[id] || 0) - values[i];
        }
        return;
    }
    for (let i = 0; i < keys.length; i++) {
        const id = keys[i];
        running_sm.set(id, (running_sm.get(id) || 0) - values[i]);
    }
}

/**
 * Initialize a running statMap from level + fixed items (locked equips, tomes, weapon).
 * This is the base that free items are incrementally added to/removed from during search.
 */
function _init_running_statmap(level, fixed_item_sms) {
    const sm = createBaseStatmap(level);
    for (const item_sm of fixed_item_sms) {
        _incr_add_item(sm, item_sm);
    }
    return sm;
}

/**
 * Finalize a leaf statMap from the running accumulated stats.
 * Applies set bonuses, sets up damMult/defMult/healMult/majorIDs.
 * Finalizes all stats at the leaf level of the search tree.
 */
function _finalize_leaf_statmap(running_sm, weapon_sm, activeSetCounts, sets_map, all_equip_sms, target, inner_scratch) {
    let sm;
    if (target) {
        sm = target;
        if (running_sm instanceof Map) {
            sm.clear();
            for (const [k, v] of running_sm) sm.set(k, v);
        } else {
            _materialize_running_statmap(running_sm, sm);
        }
    } else {
        sm = running_sm instanceof Map
            ? new Map(running_sm)
            : _materialize_running_statmap(running_sm, new Map());
    }

    applySetBonuses(sm, activeSetCounts, sets_map);
    finalizeStatmap(sm, weapon_sm, all_equip_sms, inner_scratch);

    return sm;
}

// getBaseSpellCost and getSpellCost moved to shared_game_stats.js
