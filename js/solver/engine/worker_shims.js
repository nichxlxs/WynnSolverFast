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

// ── Numeric-index running stat vector (P1.3) ────────────────────────────────
// Replaces string-keyed running-stat storage with a Float64Array addressed by
// a fixed stat_id → index table built once per worker from the full item
// universe (pools, ring pool, locked, tomes, weapon, NONE items).
//
// The first _VEC_BASE_LEN indices are the base statmap keys (STATMAP_STATIC_IDS
// + STATMAP_MUST_IDS + agiDef) and are always materialized at leaves; the
// remaining indices are item-provided stats and are materialized only when
// nonzero (zero and absent are semantically identical to every consumer:
// all downstream reads are `get(id) ?? 0` / `get(id) || 0`).

let _VEC_INDEX = null;    // Map<stat_id, index>
let _VEC_NAMES = null;    // string[] index → stat_id
let _VEC_BASE_LEN = 0;    // count of always-materialized base keys
const _VEC_ENTRY_CACHE = new WeakMap();  // item_sm → {idxs: Int32Array, vals: Float64Array}

/**
 * Build the stat index from every item statMap the search can touch.
 * Must be called before _vec_init_running/_vec_add_item.
 */
function _vec_build_index(item_sm_lists) {
    _VEC_INDEX = new Map();
    _VEC_NAMES = [];
    const add = (id) => {
        if (!_VEC_INDEX.has(id)) {
            _VEC_INDEX.set(id, _VEC_NAMES.length);
            _VEC_NAMES.push(id);
        }
    };
    for (const id of STATMAP_STATIC_IDS) add(id);
    for (const id of STATMAP_MUST_IDS) add(id);
    add('agiDef');
    _VEC_BASE_LEN = _VEC_NAMES.length;
    for (const list of item_sm_lists) {
        if (!list) continue;
        for (const item_sm of list) {
            if (!item_sm) continue;
            const entries = _get_incr_entries(item_sm);
            for (let i = 0; i < entries.keys.length; i++) add(entries.keys[i]);
        }
    }
}

/** Resolve a stat name to its vector index, or -1 when no item can provide it. */
function _vec_stat_index(id) {
    return _VEC_INDEX ? (_VEC_INDEX.get(id) ?? -1) : -1;
}

/** Initialize the running vector from a fully-populated running statMap. */
function _vec_init_running(statmap) {
    const vec = new Float64Array(_VEC_NAMES.length);
    for (const [id, value] of statmap) {
        const idx = _VEC_INDEX.get(id);
        if (idx === undefined) {
            throw new Error(`_vec_init_running: unindexed stat "${id}"`);
        }
        vec[idx] = value;
    }
    return vec;
}

/** Compile an item's additive stats to index/value arrays (cached per item). */
function _vec_entries(item_sm) {
    let entries = _VEC_ENTRY_CACHE.get(item_sm);
    if (entries) return entries;
    const base = _get_incr_entries(item_sm);
    const n = base.keys.length;
    const idxs = new Int32Array(n);
    const vals = new Float64Array(n);
    for (let i = 0; i < n; i++) {
        const idx = _VEC_INDEX.get(base.keys[i]);
        if (idx === undefined) {
            throw new Error(`_vec_entries: unindexed stat "${base.keys[i]}"`);
        }
        idxs[i] = idx;
        vals[i] = base.values[i];
    }
    entries = { idxs, vals };
    _VEC_ENTRY_CACHE.set(item_sm, entries);
    return entries;
}

function _vec_add_item(vec, item_sm) {
    const { idxs, vals } = _vec_entries(item_sm);
    for (let i = 0; i < idxs.length; i++) vec[idxs[i]] += vals[i];
}

function _vec_remove_item(vec, item_sm) {
    const { idxs, vals } = _vec_entries(item_sm);
    for (let i = 0; i < idxs.length; i++) vec[idxs[i]] -= vals[i];
}

// Entry-based variants: the worker precompiles each pool item's entries onto
// the pool wrapper at enumeration setup, skipping the per-call WeakMap lookup
// on the hot place/unplace path. (Shape adopted from PR #3.)
function _vec_add_entries(vec, entries) {
    const idxs = entries.idxs, vals = entries.vals;
    for (let i = 0; i < idxs.length; i++) vec[idxs[i]] += vals[i];
}

function _vec_remove_entries(vec, entries) {
    const idxs = entries.idxs, vals = entries.vals;
    for (let i = 0; i < idxs.length; i++) vec[idxs[i]] -= vals[i];
}

/** Materialize the running vector into a statMap (base keys + nonzero rest). */
function _vec_materialize(vec, target) {
    target.clear();
    for (let i = 0; i < _VEC_BASE_LEN; i++) target.set(_VEC_NAMES[i], vec[i]);
    for (let i = _VEC_BASE_LEN; i < vec.length; i++) {
        const v = vec[i];
        if (v !== 0) target.set(_VEC_NAMES[i], v);
    }
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
    if (running_sm instanceof Float64Array) {
        sm = _vec_materialize(running_sm, target ?? new Map());
    } else if (target) {
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
