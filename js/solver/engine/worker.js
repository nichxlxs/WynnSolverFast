// ══════════════════════════════════════════════════════════════════════════════
// SOLVER WEB WORKER
// Runs a synchronous level-based enumeration over item combinations.
// No DOM access — all state is received via postMessage.
// ══════════════════════════════════════════════════════════════════════════════

'use strict';

importScripts(
    '../../core/utils.js',
    '../../game/game_rules.js',
    '../../game/build_utils.js',
    '../../game/skillpoints.js',
    '../../game/powders.js',
    '../../game/damage_calc.js',
    '../../game/shared_game_stats.js',
    '../constants.js',
    '../debug_toggles.js',
    '../pure/spell.js',
    '../pure/boost.js',
    '../pure/utils.js',
    '../pure/simulate.js',
    '../pure/engine.js',
    './top_results.js',
    './worker_shims.js'
);

// ── Globals set during init ─────────────────────────────────────────────────

let sets = new Map();   // needed by calculate_skillpoints (set bonus tracking)
let _cfg = null;        // full config from init message
let _cancelled = false;

// ── Constraint prechecks (computed once at init) ────────────────────────────
// For each eligible ge-threshold, we precompute:
//   adjusted_threshold = threshold - fixed_contributions
// where fixed_contributions = atree_raw[stat] + static_boosts[stat].
// At the leaf, we check running_sm.get(stat) >= adjusted_threshold.
// This is a conservative lower bound (ignores radiance boost, atree scaling,
// set bonuses — all of which can only increase the stat for ge constraints).
//
// Stats excluded from simple precheck:
//   - INDIRECT_CONSTRAINT_STATS (from pure/engine.js): derived stats needing full build context
//   - 'str','dex','int','def','agi': overwritten by total_sp from SP assignment
const _PRECHECK_EXCLUDED = new Set(INDIRECT_CONSTRAINT_STATS);
for (const sp of ['str', 'dex', 'int', 'def', 'agi']) _PRECHECK_EXCLUDED.add(sp);
let _constraint_prechecks = [];  // [{stat, stat_idx, adjusted_threshold}]
let _ehp_precheck = null;        // {threshold, fixed_hp, ehp_divisor} or null
let _ehp_no_agi_precheck = null; // {threshold, fixed_hp, ehp_divisor} or null
let _total_hp_precheck = null;   // {threshold, fixed_hp} or null

// Vector indices for the EHP precheck reads (resolved after _vec_build_index).
let _vec_hp_idx = -1;
let _vec_hpBonus_idx = -1;

/**
 * Build the numeric stat index from every item statMap this worker can touch.
 * Called once at init; 'run' messages reuse the same _cfg pools so the index
 * stays valid across work-stealing partitions.
 */
function _vec_setup() {
    const lists = [];
    for (const slot of Object.keys(_cfg.pools ?? {})) {
        const pool = _cfg.pools[slot];
        if (pool) lists.push(pool.map(it => it.statMap));
    }
    if (_cfg.ring_pool) lists.push(_cfg.ring_pool.map(it => it.statMap));
    const singles = [];
    for (const item of Object.values(_cfg.locked ?? {})) {
        if (item && item.statMap) singles.push(item.statMap);
    }
    if (_cfg.ring1_locked?.statMap) singles.push(_cfg.ring1_locked.statMap);
    if (_cfg.ring2_locked?.statMap) singles.push(_cfg.ring2_locked.statMap);
    for (const t of _cfg.tome_sms ?? []) singles.push(t);
    if (_cfg.guild_tome_sm) singles.push(_cfg.guild_tome_sm);
    if (_cfg.weapon_sm) singles.push(_cfg.weapon_sm);
    for (const sm of _cfg.none_item_sms ?? []) singles.push(sm);
    lists.push(singles);
    _vec_build_index(lists);
    _vec_hp_idx = _vec_stat_index('hp');
    _vec_hpBonus_idx = _vec_stat_index('hpBonus');
    for (const pc of _constraint_prechecks) {
        pc.stat_idx = _vec_stat_index(pc.stat);
    }
}

// ── SP floor/cap constraints (computed once at init) ─────────────────────────
// Floors: minimum total_sp per attribute (from ge thresholds on SP stats).
// Caps:   maximum total_sp per attribute (from le thresholds on SP stats).
const _SKP_STAT_TO_IDX = { str: 0, dex: 1, int: 2, def: 3, agi: 4 };
let _sp_floors = null;  // Int32Array(5) or null
let _sp_caps = null;    // Int32Array(5) or null
const _default_sp_caps = new Int32Array([150, 150, 150, 150, 150]);

// Debug toggles: SOLVER_DEBUG_WORKER, SOLVER_DEBUG_COMBO
// (defined in js/solver/debug_toggles.js, loaded via importScripts)

// ── Search state ────────────────────────────────────────────────────────────

const PROGRESS_INTERVAL = 5000;
const PROGRESS_INTERVAL_LONG = 50000;

// Progress reporting is checkpoint-based rather than modulo-based: subtree
// crediting (illegal-set skips, SP/restriction pruning) advances _checked in
// large jumps that step over interval multiples, which silently starved the
// modulo check of progress (and therefore anytime top-5) messages.
// (Adopted from PR #3's _take_progress_checkpoint.)
const _progress_checkpoint = { next: PROGRESS_INTERVAL };
function _take_progress_checkpoint(checked) {
    if (checked < _progress_checkpoint.next) return false;
    // Adaptive step: fixed intervals early, then ~0.4% of the current count.
    // Subtree crediting can advance checked by billions — a fixed 50K step
    // would flood postMessage/structuredClone (measured ~15% of worker CPU
    // on the 4.5T scenario).
    const step = checked < 1_000_000
        ? PROGRESS_INTERVAL
        : Math.max(PROGRESS_INTERVAL_LONG, Math.floor(checked / 256));
    do {
        _progress_checkpoint.next += step;
    } while (_progress_checkpoint.next <= checked);
    return true;
}
let _checked = 0;
let _precheck_pass = 0;
let _precheck_reject = 0;
let _feasible = 0;
let _met_req = 0;
let _top5 = [];
let _top5_version = 0;
let _last_sent_top5_version = 0;
let _checked_at_last_top5_change = 0;
let _current_L = 0;
let _trace = null;
let _trace_started = 0;

const _TRACE_PHASES = ['precheck', 'sp', 'finalize', 'ceiling', 'greedy', 'assemble', 'threshold', 'mana', 'score', 'topn'];
function _reset_trace() {
    if (!_cfg?.benchmark_trace) { _trace = null; return; }
    _trace = { leaf_count: 0 };
    _trace_started = performance.now();
    for (const phase of _TRACE_PHASES) {
        _trace[phase + '_calls'] = 0;
        _trace[phase + '_ms'] = 0;
    }
}
function _trace_snapshot() {
    if (!_trace) return null;
    return { ..._trace, wall_ms: performance.now() - _trace_started };
}
function _trace_start(phase) {
    if (!_trace) return 0;
    _trace[phase + '_calls']++;
    return performance.now();
}
function _trace_end(phase, start) {
    if (_trace) _trace[phase + '_ms'] += performance.now() - start;
}

function _insert_top5(score, candidateFactory) {
    if (_cfg?.benchmark_legacy_top_results) {
        _top5.push(candidateFactory());
        _top5.sort((a, b) => b.score - a.score);
        if (_top5.length > 15) _top5.length = 15;
        _top5_version++;
        _checked_at_last_top5_change = _checked;
        return;
    }
    if (tryInsertTopResult(_top5, score, candidateFactory, 15)) {
        _top5_version++;
        _checked_at_last_top5_change = _checked;
    }
}

// ── Pre-allocated scratch Maps (reused across leaves to eliminate GC churn) ──

let _cached_hp_sim = null;  // cached simulate_combo_mana_hp result (avoids double-sim)

const _scratch_finalize = new Map();
const _scratch_pre_scale = new Map();
const _scratch_pre_scale_nested = { damMult: new Map(), defMult: new Map(), healMult: new Map() };
const _scratch_combo_base = new Map();
const _scratch_combo_base_nested = { damMult: new Map(), defMult: new Map(), healMult: new Map() };
const _scratch_thresh = new Map();
const _scratch_thresh_nested = { damMult: new Map(), defMult: new Map(), healMult: new Map() };
const _scratch_row = { stats: new Map(), damMult: new Map(), defMult: new Map(), prop_overrides: new Map() };
const _scratch_atree = { atree_edit: new Map(), ret_effects: new Map() };
const _scratch_finalize_inner = { damMult: new Map(), defMult: new Map(), healMult: new Map(), majorIds: new Set() };
const _scratch_sp_set_counts = new Map();
const _scratch_orig_base_sp = new Int32Array(5);  // pre-greedy base_sp snapshot for mana rescue

// ── Pre-allocated scratch arrays for leaf evaluation (eliminate per-leaf allocs) ──
const _scratch_equip_8 = new Array(8);
const _scratch_sp_input = new Array(9);    // 8 equips + guild_tome
let _scratch_all_equip = null;             // sized at init: 8 + tome_sms.length + 1 (weapon)
const _scratch_sp = {
    assign:          [0, 0, 0, 0, 0],
    final:           [0, 0, 0, 0, 0],
    free_bonus:      [0, 0, 0, 0, 0],
    max_passive_req: [0, 0, 0, 0, 0],
    post_floor:      [0, 0, 0, 0, 0],
    running_bonus:   [0, 0, 0, 0, 0],
    best_assign:     [0, 0, 0, 0, 0],
    save_stack:      new Array(45),      // 9 depths * 5 attrs
    ord_items:       new Array(9),
    ord_reqs:        new Array(9),
    ord_skp:         new Array(9),
    no_bonus:        [],  // sized at init (max 9: weapon + up to 8 crafted items)
    _no_bonus_len:   0,
    total_item_skp:  [0, 0, 0, 0, 0],
};

/**
 * Build constraint prechecks from the restriction thresholds.
 * Called once during worker init.
 */
function _build_constraint_prechecks() {
    _constraint_prechecks = [];
    _ehp_precheck = null;
    _ehp_no_agi_precheck = null;
    _total_hp_precheck = null;

    const thresholds = _cfg.restrictions?.stat_thresholds ?? [];
    if (thresholds.length === 0) return;

    // Compute fixed stat contributions (constant across all candidates).
    // atree_raw and static_boosts are both Maps.
    const fixed = (stat) => {
        return (_cfg.atree_raw?.get(stat) ?? 0) + (_cfg.static_boosts?.get(stat) ?? 0);
    };

    for (const { stat, op, value } of thresholds) {
        if (op !== 'ge') continue;  // only ge constraints benefit from early rejection

        if (stat === 'ehp' || stat === 'ehp_no_agi') {
            // Precompute fixed EHP constants
            const fixed_hp = fixed('hpBonus');

            const def_pct = skillPointsToPercentage(100) * skillpoint_final_mult[3];
            const weaponType = _cfg.weapon_sm?.get('type');
            const classDef = classDefenseMultipliers.get(weaponType) || 1.0;
            const defMult = (2 - classDef);

            if (stat === 'ehp') {
                const agi_pct = skillPointsToPercentage(100) * skillpoint_final_mult[4];
                const agi_reduction = (100 - 90) / 100;
                const ehp_divisor = (agi_reduction * agi_pct + (1 - agi_pct) * (1 - def_pct)) * defMult;
                _ehp_precheck = { threshold: value, fixed_hp, ehp_divisor };
            } else {
                // ehp_no_agi: no agility dodge factor, just def_pct
                const ehp_divisor = (1 - def_pct) * defMult;
                _ehp_no_agi_precheck = { threshold: value, fixed_hp, ehp_divisor };
            }
            continue;
        }

        if (stat === 'total_hp') {
            _total_hp_precheck = { threshold: value, fixed_hp: fixed('hpBonus') };
            continue;
        }

        if (_PRECHECK_EXCLUDED.has(stat)) continue;

        const fixed_contrib = fixed(stat);
        _constraint_prechecks.push({
            stat,
            stat_idx: -1,  // resolved by _vec_setup after the stat index exists
            adjusted_threshold: value - fixed_contrib,
        });
    }
}

/**
 * Build SP floor/cap constraints from restriction thresholds.
 * Floors come from ge thresholds on SP stats, caps from le thresholds.
 * Called once during worker init.
 */
function _build_sp_constraints() {
    _sp_floors = null;
    _sp_caps = null;

    const thresholds = _cfg.restrictions?.stat_thresholds ?? [];
    if (thresholds.length === 0) return;

    for (const { stat, op, value } of thresholds) {
        const idx = _SKP_STAT_TO_IDX[stat];
        if (idx === undefined) continue;

        if (op === 'ge') {
            if (!_sp_floors) { _sp_floors = new Int32Array(5); }
            _sp_floors[idx] = Math.max(_sp_floors[idx], value);
        } else if (op === 'le') {
            if (!_sp_caps) { _sp_caps = new Int32Array([150, 150, 150, 150, 150]); }
            _sp_caps[idx] = Math.min(_sp_caps[idx], value);
        }
    }
}

/**
 * Fast constraint precheck against the running statMap.
 * Returns false if any ge-threshold cannot be met (conservative lower bound).
 *
 * TODO: Unify user-requirement checking into the precheck.
 * This precheck operates on raw additive stat sums from running_sm plus
 * fixed atree/static contributions. For the vast majority of user-requestable
 * stats this is actually the authoritative value — they are NOT affected by
 * multipliers, SP bonuses (except EHP/mana-adjacent), powders, or crit
 * weighting. The main exception is atree scaling (conditional / slider-driven
 * bonuses), which we could fold into the precheck by precomputing the
 * post-scaling additive contribution per active atree branch.
 * If that's done, _check_thresholds below can be reduced to handling only
 * mana/HP-sim outputs (and scoring targets that genuinely need combo_base).
 * Deferred — current split is correct but redundant.
 */
function _fast_constraint_precheck(running_sm) {
    if (running_sm instanceof Float64Array) {
        for (let i = 0; i < _constraint_prechecks.length; i++) {
            const pc = _constraint_prechecks[i];
            const value = pc.stat_idx >= 0 ? running_sm[pc.stat_idx] : 0;
            if (value < pc.adjusted_threshold) return false;
        }
        return true;
    }
    for (let i = 0; i < _constraint_prechecks.length; i++) {
        const pc = _constraint_prechecks[i];
        const value = running_sm instanceof Map ? running_sm.get(pc.stat) : running_sm[pc.stat];
        if ((value ?? 0) < pc.adjusted_threshold) return false;
    }
    return true;
}

/**
 * Optimistic EHP precheck using precomputed constants.
 * Computes an upper bound on EHP assuming max def/agi skill points (100 each)
 * and no extra defMult penalties. If even this can't meet the threshold, reject.
 * Also checks ehp_no_agi and total_hp prechecks.
 */
function _fast_ehp_precheck(running_sm) {
    if (!_ehp_precheck && !_ehp_no_agi_precheck && !_total_hp_precheck) return true;

    // running_sm.get('hp') = levelToHPBase + sum of item 'hp' (static ID)
    // running_sm.get('hpBonus') = sum of item hpBonus (from maxRolls)
    let raw_hp;
    if (running_sm instanceof Float64Array) {
        raw_hp = (_vec_hp_idx >= 0 ? running_sm[_vec_hp_idx] : 0)
            + (_vec_hpBonus_idx >= 0 ? running_sm[_vec_hpBonus_idx] : 0);
    } else {
        const get = running_sm instanceof Map
            ? stat => running_sm.get(stat)
            : stat => running_sm[stat];
        raw_hp = (get('hp') ?? 0) + (get('hpBonus') ?? 0);
    }

    if (_ehp_precheck) {
        let totalHp = raw_hp + _ehp_precheck.fixed_hp;
        if (totalHp < 5) totalHp = 5;
        if ((totalHp / _ehp_precheck.ehp_divisor) < _ehp_precheck.threshold) return false;
    }

    if (_ehp_no_agi_precheck) {
        let totalHp = raw_hp + _ehp_no_agi_precheck.fixed_hp;
        if (totalHp < 5) totalHp = 5;
        if ((totalHp / _ehp_no_agi_precheck.ehp_divisor) < _ehp_no_agi_precheck.threshold) return false;
    }

    if (_total_hp_precheck) {
        let totalHp = raw_hp + _total_hp_precheck.fixed_hp;
        if (totalHp < 5) totalHp = 5;
        if (totalHp < _total_hp_precheck.threshold) return false;
    }

    return true;
}

// ── Per-candidate stat assembly ─────────────────────────────────────────────

// ── Atree scaling optimization state (computed once per worker) ─────────────
// When no non-slider stat_scaling reads a build stat, the scaling output is
// constant for this search's fixed button/slider states and is computed once.
// When no effect produces a 'prop'-type bonus, the per-call ability clone in
// atree_compute_scaling is skipped (skip_edit).
let _atree_scaling_ready = false;
let _atree_scaled_cache = null;   // Map | null — whole output constant
let _atree_split = null;          // { const_scaled: Map, var_effects: [] } | null
let _atree_skip_edit = false;
const _scratch_var_scaled = new Map();
const _ATREE_SCALING_OPTS = { cached: null, split: null, scratch_var: null, skip_edit: false };
const _MULT_PREFIXES = new Set(['damMult', 'defMult', 'healMult', 'manaMult']);

function _atree_scaling_setup() {
    if (_atree_scaling_ready) return;
    _atree_scaling_ready = true;
    const a = atree_scaling_analysis(_cfg.atree_merged);
    _atree_skip_edit = !a.has_prop_outputs;
    if (!a.stat_dependent) {
        const [, scaled] = atree_compute_scaling(
            _cfg.atree_merged, new Map(), _cfg.button_states, _cfg.slider_states,
            null, _atree_skip_edit);
        _atree_scaled_cache = scaled;
    } else {
        // Stat-dependent: split the scaling into a constant partition
        // (computed once, including any prop mutations — legal because no
        // VARIABLE effect reads or writes props) and the stat-input effects
        // (re-evaluated per candidate). Exact only when the two partitions
        // write disjoint plain (non-Mult, non-nested) keys — per-key merge
        // order is then preserved within each partition.
        const plan = atree_collect_stat_effects(_cfg.atree_merged);
        if (plan && !plan.var_has_prop_io) {
            const [, const_scaled] = atree_compute_scaling(
                _cfg.atree_merged, new Map(), _cfg.button_states, _cfg.slider_states,
                null, _atree_skip_edit, /* skip_stat_inputs */ true);
            let ok = true;
            for (const key of plan.var_keys) {
                if (key.includes('.') || _MULT_PREFIXES.has(key) || const_scaled.has(key)) {
                    ok = false;
                    break;
                }
            }
            if (ok) {
                _atree_split = { const_scaled, var_effects: plan.var_effects };
            }
        }
    }
    _ATREE_SCALING_OPTS.cached = _atree_scaled_cache;
    _ATREE_SCALING_OPTS.split = _atree_split;
    _ATREE_SCALING_OPTS.scratch_var = _scratch_var_scaled;
    _ATREE_SCALING_OPTS.skip_edit = _atree_skip_edit;
    _ceiling_gate_setup(a);
}

// ── Score-ceiling gate (combo_damage only) ───────────────────────────────────
//
// Combo damage is monotone non-decreasing in every total_sp dimension when:
//   - skill boosts: skillPointsToPercentage is nondecreasing, damage mults >= 0;
//   - crit: avg = normal + crit_chance * (crit - normal), crit >= normal, and
//     crit_chance = skillPointsToPercentage(dex);
//   - atree stat-input effects: their outputs are monotone in the inputs when
//     every resolved stat scaling factor is >= 0 (floor/clamp/cap steps are
//     monotone), and the output keys feed damage non-negatively (plain
//     additive boost/raw stats; atkTier and ConvBase keys are excluded
//     because attack-speed and conversion effects are not monotone).
// Under those conditions the damage evaluated at total_sp = [150]*5 is an
// upper bound on any allocation the greedy could reach, so a leaf whose
// ceiling cannot beat the current top-N cutoff can skip greedy + mana +
// score entirely with an identical top-N. Sim-coupled damage (Blood Pact,
// dynamic sliders) is excluded. met_req no longer counts gated leaves
// (diagnostic-only change).
let _ceiling_gate_ok = false;
const _SP_CEILING = new Int32Array([150, 150, 150, 150, 150]);

function _ceiling_gate_setup(analysis) {
    _ceiling_gate_ok = false;
    if ((_cfg.scoring_target ?? 'combo_damage') !== 'combo_damage') return;
    if (_cfg.hp_casting || _cfg.has_dynamic_sliders) return;
    if (_cfg.custom_weights?.length) return;
    if (analysis.stat_dependent) {
        // Only reason about stat-input effects through the split plan.
        if (!_atree_split) return;
        for (const effect of _atree_split.var_effects) {
            const scaling = effect.scaling ?? [0];
            const inputs = effect.inputs ?? [];
            const n = Math.min(scaling.length, inputs.length);
            for (let i = 0; i < n; i++) {
                if (inputs[i].type !== 'stat') continue;
                const factor = atree_translate(_cfg.atree_merged, scaling[i]);
                if (!(typeof factor === 'number' && factor >= 0)) return;
            }
            if ('output' in effect) {
                const outputs = Array.isArray(effect.output) ? effect.output : [effect.output];
                for (const o of outputs) {
                    if (o.type !== 'stat') return;
                    if (o.name === 'atkTier' || o.name.includes('ConvBase')) return;
                }
            }
        }
    }
    _ceiling_gate_ok = true;
}

function _assemble_combo_stats(build_sm, total_sp, weapon_sm) {
    return assemble_combo_stats(build_sm, total_sp, weapon_sm,
        _cfg.atree_raw, _cfg.radiance_boost, _cfg.atree_merged,
        _cfg.button_states, _cfg.slider_states, _cfg.static_boosts,
        { pre_scale: _scratch_pre_scale, pre_scale_nested: _scratch_pre_scale_nested,
          combo_base: _scratch_combo_base, combo_base_nested: _scratch_combo_base_nested,
          atree: _scratch_atree },
        _ATREE_SCALING_OPTS);
}

function _assemble_threshold_stats(combo_base) {
    // static_boosts are already merged into combo_base by _assemble_combo_stats.
    return _deep_clone_statmap_into(_scratch_thresh, combo_base, _scratch_thresh_nested);
}

// TODO: Narrow _check_thresholds to mana/HP and scoring-target-only stats.
// Currently this re-checks every user-configured threshold against the fully
// assembled combo_base (post SP solve + atree scaling + multipliers). For most
// stats this is redundant with _fast_constraint_precheck, since user-requestable
// stats aren't affected by multipliers, SP bonuses (except EHP/mana-adjacent),
// powders, or crit weighting — only atree scaling differs, which could be
// folded into the precheck (see the TODO at _fast_constraint_precheck).
// Once that migration happens, this call should only validate stats that
// genuinely require the full combo evaluation (mana costs, combo_dps, healing
// totals, hp_sim outputs), and everything else can be rejected earlier.
function _check_thresholds(stats, thresholds) {
    return check_thresholds(stats, thresholds, _cfg.spell_base_costs);
}

function _eval_combo_damage(combo_base, debug) {
    const result = eval_combo_damage_with_bp(combo_base, _cfg.weapon_sm, _cfg.parsed_combo, {
        hp_casting: _cfg.hp_casting,
        has_dynamic_sliders: _cfg.has_dynamic_sliders,
        health_config: _cfg.health_config,
        boost_registry: _cfg.boost_registry,
        atree_merged: _cfg.atree_merged,
        bp_slider_name: _cfg.bp_slider_name,
        state_slider_names: _cfg.state_slider_names,
    }, {
        scratch_row: _scratch_row,
        debug,
        debug_label: '[WORKER]',
        cached_hp_sim: _cached_hp_sim,
    });
    _cached_hp_sim = null;  // consume cache
    return result.total_damage;
}

/** Mana/HP feasibility gate — delegates to shared eval_combo_mana_check(). */
function _eval_combo_mana_check(combo_base) {
    _cached_hp_sim = null;
    const result = eval_combo_mana_check({
        parsed_combo: _cfg.parsed_combo,
        combo_base,
        hp_casting: _cfg.hp_casting,
        combo_time: _cfg.combo_time ?? 0,
        allow_downtime: _cfg.allow_downtime,
        health_config: _cfg.health_config ?? DEFAULT_HEALTH_CONFIG,
        boost_registry: _cfg.boost_registry,
        scratch_row: _scratch_row,
        use_fast_sim: true,
    });
    _cached_hp_sim = result.sim;
    return result.passed;
}

function _eval_combo_healing(combo_base) {
    return eval_combo_healing(_cfg.parsed_combo, combo_base, _cfg.boost_registry, _scratch_row);
}

/**
 * Dispatch to the correct scoring function based on _cfg.scoring_target.
 * @param {Map} combo_base  - stats after radiance+atree scaling+static_boosts
 * @param {Map} thresh_stats - deep clone of combo_base (may be null; computed lazily)
 */
function _eval_score(combo_base, thresh_stats) {
    return eval_score_dispatch(_cfg.scoring_target, combo_base,
        () => _eval_combo_damage(combo_base),
        () => _eval_combo_healing(combo_base),
        thresh_stats ?? _assemble_threshold_stats(combo_base),
        _cfg.custom_weights);
}

// get_item_display_name() — shared from pure/engine.js
const _get_item_name = get_item_display_name;

// ── Illegal-set tracking ────────────────────────────────────────────────────

function _make_illegal_tracker() {
    const occupants = new Map();
    return {
        add(setName, itemName) {
            if (!occupants.has(setName)) occupants.set(setName, new Map());
            const m = occupants.get(setName);
            m.set(itemName, (m.get(itemName) ?? 0) + 1);
        },
        remove(setName, itemName) {
            const m = occupants.get(setName);
            if (!m) return;
            const c = m.get(itemName) ?? 1;
            if (c <= 1) m.delete(itemName); else m.set(itemName, c - 1);
        },
        blocks(is, iname) {
            if (!is) return false;
            const m = occupants.get(is);
            // Block any item from an illegal-at-2 set once one is already placed,
            // including a duplicate of the same item (e.g. two Hive rings).
            return !!(m && m.size > 0);
        }
    };
}

// ── Level-based enumeration ──────────────────────────────────────────────────
//
// Enumerates all item combinations ordered by sum-of-rank-offsets (level L).
// Level L=0 visits (rank0, rank0, ..., rank0) — the globally best build first.
// Level L=1 visits all builds with exactly one slot at rank 1 (others rank 0).
// Memory is O(k). No heap or visited set needed.
//
// Items in pools/locked are wrapper objects: { statMap: Map, _illegalSet, _illegalSetName }
// none_item_sms are raw statMaps (no illegal set info needed for NONE items).
// We wrap NONE items too for uniform handling in partial[].

function _run_level_enum() {
    const { locked, weapon_sm, level, tome_sms, guild_tome_sm,
        sp_budget, restrictions, partition, none_item_sms,
        ring_pool, ring1_locked, ring2_locked } = _cfg;

    // Shallow-copy pools so partition slicing doesn't mutate _cfg.pools.
    // Without this, subsequent work-stealing partitions on the same worker
    // would see already-sliced (effectively empty) pools.
    const pools = { ..._cfg.pools };
    const _dbg = SOLVER_DEBUG_WORKER && _cfg.worker_id === 0;
    let _dbg_sp_prune_count = 0;
    let _dbg_restr_prune_count = 0;
    let _dbg_sp_leaf_reject = 0;
    let _dbg_precheck_reject = 0;
    let _dbg_ehp_reject = 0;
    let _dbg_sp_reject = 0;
    let _dbg_threshold_reject = 0;
    let _dbg_mana_reject = 0;
    let _dbg_mana_rescued = 0;
    let _dbg_hp_reject = 0;
    let _dbg_scored = 0;
    let _dbg_ceiling_skip = 0;
    let _dbg_leaf_time = 0;  // cumulative ms for feasible leaf processing

    const tracker = _make_illegal_tracker();

    // Wrap NONE statMaps into the same {statMap, _illegalSet, _illegalSetName} format
    const none_items_wrapped = none_item_sms.map(sm => ({ statMap: sm, _illegalSet: null, _illegalSetName: null }));

    // Determine all free slots (armor/accessory + rings), sorted by pool size ascending.
    // Rings are included in the unified level enumeration for combined-priority ordering.
    const free_slots = [];
    for (const slot of ['helmet', 'chestplate', 'leggings', 'boots', 'bracelet', 'necklace']) {
        if (!locked[slot]) free_slots.push(slot);
    }
    if (!ring1_locked) free_slots.push('ring1');
    if (!ring2_locked) free_slots.push('ring2');

    // Pool lookup: ring1/ring2 share ring_pool; armor slots use pools[slot].
    const _get_pool = (slot) =>
        (slot === 'ring1' || slot === 'ring2') ? ring_pool : pools[slot];

    free_slots.sort((a, b) => {
        const diff = (_get_pool(a)?.length ?? 0) - (_get_pool(b)?.length ?? 0);
        if (diff !== 0) return diff;
        // Ensure ring1 before ring2 (same pool size) for symmetry constraint.
        if (a === 'ring1' && b === 'ring2') return -1;
        if (a === 'ring2' && b === 'ring1') return 1;
        return 0;
    });

    // Depth indices for ring slots (-1 if locked). Used for symmetry & partition logic.
    const ring1_depth = free_slots.indexOf('ring1');
    const ring2_depth = free_slots.indexOf('ring2');

    // partial: holds item wrapper objects for each of the 8 equipment positions
    const partial = {
        helmet: locked.helmet ?? none_items_wrapped[0],
        chestplate: locked.chestplate ?? none_items_wrapped[1],
        leggings: locked.leggings ?? none_items_wrapped[2],
        boots: locked.boots ?? none_items_wrapped[3],
        ring1: ring1_locked ?? none_items_wrapped[4],
        ring2: ring2_locked ?? none_items_wrapped[5],
        bracelet: locked.bracelet ?? none_items_wrapped[6],
        necklace: locked.necklace ?? none_items_wrapped[7],
    };

    // Track illegal sets for locked items
    for (const item of Object.values(partial)) {
        if (!item || !item.statMap || item.statMap.has('NONE')) continue;
        const name = _get_item_name(item.statMap);
        const is = item._illegalSet;
        if (is && name) tracker.add(is, name);
    }

    // If this worker has a partition, apply it: restrict one slot's pool to [start, end)
    if (partition && partition.type === 'slot' && pools[partition.slot]) {
        pools[partition.slot] = pools[partition.slot].slice(partition.start, partition.end);
    }

    const N_free = free_slots.length;

    // ── Mid-tree SP pruning precomputation ─────────────────────────────────
    //
    // For each free slot's pool, the element-wise max provision across
    // all items.  Used to compute an optimistic upper bound on bonus SP from
    // remaining (unplaced) slots.

    const _sp_max_pool_prov = [];  // _sp_max_pool_prov[depth_idx] = [5]
    for (let d = 0; d < N_free; d++) {
        const pool = _get_pool(free_slots[d]);
        const maxp = [0, 0, 0, 0, 0];
        if (pool) {
            for (const item of pool) {
                const skp = item.statMap.get('skillpoints');
                for (let i = 0; i < 5; i++) {
                    if (skp[i] > maxp[i]) maxp[i] = skp[i];
                }
            }
        }
        _sp_max_pool_prov.push(maxp);
    }

    // Suffix sums: _sp_suffix_max_prov[d][i] = sum of _sp_max_pool_prov[k][i]
    // for k = d, d+1, ..., N_free-1.
    // Index N_free = [0,0,0,0,0] (no remaining slots).
    const _sp_suffix_max_prov = new Array(N_free + 1);
    _sp_suffix_max_prov[N_free] = [0, 0, 0, 0, 0];
    for (let d = N_free - 1; d >= 0; d--) {
        _sp_suffix_max_prov[d] = [0, 0, 0, 0, 0];
        for (let i = 0; i < 5; i++) {
            _sp_suffix_max_prov[d][i] = _sp_suffix_max_prov[d + 1][i]
                + _sp_max_pool_prov[d][i];
        }
    }

    // ── Mid-tree restriction suffix bounds (P1.5) ───────────────────────────
    //
    // For each direct ge-precheck stat (and the hp+hpBonus base of the
    // EHP-family prechecks), precompute per-depth suffix sums of the maximum
    // contribution any item in each remaining slot's pool can add. A subtree
    // is pruned only when even that optimistic completion fails the exact
    // check the leaf precheck would apply, so pruning removes only leaves the
    // precheck would reject: checked/feasible/top-N are unchanged.

    const _item_stat_value = (sm, stat) => {
        if (STATMAP_STATIC_ID_SET.has(stat)) return sm.get(stat) || 0;
        return sm.get('maxRolls')?.get(stat) || 0;
    };

    const _n_pc = _constraint_prechecks.length;
    const _hp_prechecks_active = !!(_ehp_precheck || _ehp_no_agi_precheck || _total_hp_precheck);
    let _pc_suffix = null;   // Float64Array[(N_free+1) * _n_pc], row-major by depth
    let _hp_suffix = null;   // Float64Array[N_free+1] for hp + hpBonus

    if (_n_pc > 0 && N_free > 0) {
        _pc_suffix = new Float64Array((N_free + 1) * _n_pc);
        for (let d = N_free - 1; d >= 0; d--) {
            const pool = _get_pool(free_slots[d]);
            for (let i = 0; i < _n_pc; i++) {
                let slot_max = -Infinity;
                if (pool && pool.length) {
                    for (const item of pool) {
                        const v = _item_stat_value(item.statMap, _constraint_prechecks[i].stat);
                        if (v > slot_max) slot_max = v;
                    }
                } else {
                    slot_max = 0;
                }
                _pc_suffix[d * _n_pc + i] = _pc_suffix[(d + 1) * _n_pc + i] + slot_max;
            }
        }
    }
    if (_hp_prechecks_active && N_free > 0) {
        _hp_suffix = new Float64Array(N_free + 1);
        for (let d = N_free - 1; d >= 0; d--) {
            const pool = _get_pool(free_slots[d]);
            let slot_max = -Infinity;
            if (pool && pool.length) {
                for (const item of pool) {
                    const sm = item.statMap;
                    const v = (sm.get('hp') || 0) + (sm.get('maxRolls')?.get('hpBonus') || 0);
                    if (v > slot_max) slot_max = v;
                }
            } else {
                slot_max = 0;
            }
            _hp_suffix[d] = _hp_suffix[d + 1] + slot_max;
        }
    }

    // Current-value getters that work for every running-store variant.
    const _running_get = (stat, idx) => {
        if (_running_is_vec) return idx >= 0 ? running_sm[idx] : 0;
        if (running_sm instanceof Map) return running_sm.get(stat) ?? 0;
        return running_sm[stat] ?? 0;
    };

    /**
     * Returns false when no completion of the current prefix can pass the
     * leaf constraint/EHP prechecks (same math, optimistic suffix added).
     */
    function _restr_mid_tree_feasible(next_depth) {
        if (_pc_suffix) {
            const row = next_depth * _n_pc;
            for (let i = 0; i < _n_pc; i++) {
                const pc = _constraint_prechecks[i];
                const cur = _running_get(pc.stat, pc.stat_idx);
                if (cur + _pc_suffix[row + i] < pc.adjusted_threshold) return false;
            }
        }
        if (_hp_suffix) {
            const raw_hp = _running_get('hp', _vec_hp_idx)
                + _running_get('hpBonus', _vec_hpBonus_idx)
                + _hp_suffix[next_depth];
            if (_ehp_precheck) {
                let totalHp = raw_hp + _ehp_precheck.fixed_hp;
                if (totalHp < 5) totalHp = 5;
                if ((totalHp / _ehp_precheck.ehp_divisor) < _ehp_precheck.threshold) return false;
            }
            if (_ehp_no_agi_precheck) {
                let totalHp = raw_hp + _ehp_no_agi_precheck.fixed_hp;
                if (totalHp < 5) totalHp = 5;
                if ((totalHp / _ehp_no_agi_precheck.ehp_divisor) < _ehp_no_agi_precheck.threshold) return false;
            }
            if (_total_hp_precheck) {
                let totalHp = raw_hp + _total_hp_precheck.fixed_hp;
                if (totalHp < 5) totalHp = 5;
                if (totalHp < _total_hp_precheck.threshold) return false;
            }
        }
        return true;
    }
    const _restr_pruning_active = !!(_pc_suffix || _hp_suffix);

    // ── Pre-placement bound columns ─────────────────────────────────────────
    //
    // Flat per-slot columns of every pool item's bound-relevant values, so
    // enumerate() can test the SP and restriction suffix bounds for a child
    // BEFORE paying placement (identical math to the post-placement checks:
    // running-after-place equals running-before + the column value). At
    // trillion scale most children are pruned, so skipping place/unplace for
    // them dominates the enumeration cost.

    const _col_pc = new Array(N_free);    // Float64Array [offset*_n_pc + i] | null
    const _col_hp = new Array(N_free);    // Float64Array [offset] | null
    const _col_req = new Array(N_free);   // Int32Array [offset*5 + j]
    const _col_prov = new Array(N_free);  // Int32Array [offset*5 + j], non-crafted positive skp
    for (let d = 0; d < N_free; d++) {
        const pool = _get_pool(free_slots[d]) ?? [];
        const n = pool.length;
        const pcs = _n_pc > 0 ? new Float64Array(n * _n_pc) : null;
        const hps = _hp_prechecks_active ? new Float64Array(n) : null;
        const reqs = new Int32Array(n * 5);
        const provs = new Int32Array(n * 5);
        for (let o = 0; o < n; o++) {
            const sm = pool[o].statMap;
            if (pcs) {
                for (let i = 0; i < _n_pc; i++) {
                    pcs[o * _n_pc + i] = _item_stat_value(sm, _constraint_prechecks[i].stat);
                }
            }
            if (hps) {
                hps[o] = (sm.get('hp') || 0) + (sm.get('maxRolls')?.get('hpBonus') || 0);
            }
            const req = sm.get('reqs');
            const skp = sm.get('skillpoints');
            const crafted = sm.get('crafted');
            for (let j = 0; j < 5; j++) {
                reqs[o * 5 + j] = req[j];
                provs[o * 5 + j] = (!crafted && skp[j] > 0) ? skp[j] : 0;
            }
        }
        _col_pc[d] = pcs;
        _col_hp[d] = hps;
        _col_req[d] = reqs;
        _col_prov[d] = provs;
    }

    // Per-depth hoist buffers (recursion-safe: one set per depth).
    const _hoist_prov_sfx = [];   // Int32Array(5): fixed+free prov + suffix at child depth
    const _hoist_pc_base = [];    // Float64Array(_n_pc): running pc + suffix at child depth
    const _hoist_hp_base = new Float64Array(N_free || 1);
    for (let d = 0; d < N_free; d++) {
        _hoist_prov_sfx.push(new Int32Array(5));
        _hoist_pc_base.push(_n_pc > 0 ? new Float64Array(_n_pc) : null);
    }
    const _pc_thresholds_flat = new Float64Array(_n_pc);
    for (let i = 0; i < _n_pc; i++) _pc_thresholds_flat[i] = _constraint_prechecks[i].adjusted_threshold;

    /** SP bound for placing pool[offset] at slot_idx (bases pre-hoisted). */
    function _sp_bound_ok(slot_idx, offset) {
        const req_col = _col_req[slot_idx];
        const prov_col = _col_prov[slot_idx];
        const prov_sfx = _hoist_prov_sfx[slot_idx];
        const ro = offset * 5;
        let total_deficit = 0;
        for (let j = 0; j < 5; j++) {
            const r = req_col[ro + j];
            const m = r > _sp_running_max_eff_req[j] ? r : _sp_running_max_eff_req[j];
            if (m === 0) continue;
            const prov = prov_sfx[j] + prov_col[ro + j];
            if (m <= prov) continue;
            const deficit = m - prov;
            if (deficit > SP_PER_ATTR_CAP) return false;
            total_deficit += deficit;
            if (total_deficit > sp_budget) return false;
        }
        return true;
    }

    /** Restriction/EHP bound for placing pool[offset] at slot_idx. */
    function _restr_bound_ok(slot_idx, offset) {
        const pc_col = _col_pc[slot_idx];
        if (pc_col) {
            const pc_base = _hoist_pc_base[slot_idx];
            const po = offset * _n_pc;
            for (let i = 0; i < _n_pc; i++) {
                if (pc_base[i] + pc_col[po + i] < _pc_thresholds_flat[i]) return false;
            }
        }
        const hp_col = _col_hp[slot_idx];
        if (hp_col) {
            if (!_hp_gates_ok(_hoist_hp_base[slot_idx] + hp_col[offset])) return false;
        }
        return true;
    }

    /** EHP-family gate on an optimistic raw hp value (mirrors _fast_ehp_precheck). */
    function _hp_gates_ok(raw_hp) {
        if (_ehp_precheck) {
            let totalHp = raw_hp + _ehp_precheck.fixed_hp;
            if (totalHp < 5) totalHp = 5;
            if ((totalHp / _ehp_precheck.ehp_divisor) < _ehp_precheck.threshold) return false;
        }
        if (_ehp_no_agi_precheck) {
            let totalHp = raw_hp + _ehp_no_agi_precheck.fixed_hp;
            if (totalHp < 5) totalHp = 5;
            if ((totalHp / _ehp_no_agi_precheck.ehp_divisor) < _ehp_no_agi_precheck.threshold) return false;
        }
        if (_total_hp_precheck) {
            let totalHp = raw_hp + _total_hp_precheck.fixed_hp;
            if (totalHp < 5) totalHp = 5;
            if (totalHp < _total_hp_precheck.threshold) return false;
        }
        return true;
    }

    // ── Incremental stat accumulation ───────────────────────────────────────
    // Build base statMap from locked items + tomes + weapon. Free items are added/removed during search.

    const fixed_item_sms = [];
    // Locked equipment
    for (const item of Object.values(partial)) {
        if (item && item.statMap && !item.statMap.has('NONE')) fixed_item_sms.push(item.statMap);
    }
    // Tomes and weapon
    for (const t of tome_sms) fixed_item_sms.push(t);
    fixed_item_sms.push(weapon_sm);

    const running_sm_map = _init_running_statmap(level, fixed_item_sms);
    let running_sm;
    if (_cfg.benchmark_legacy_running_map) {
        running_sm = running_sm_map;
    } else if (_cfg.benchmark_compact_running) {
        running_sm = _init_running_stats_compact(running_sm_map);
    } else {
        running_sm = _vec_init_running(running_sm_map);
    }
    const _running_is_vec = running_sm instanceof Float64Array;

    // Size scratch arrays now that tome_sms is known
    _scratch_all_equip = new Array(8 + tome_sms.length + 1);  // 8 equips + tomes + weapon
    _scratch_sp.no_bonus = new Array(9);  // weapon + up to 8 crafted items (max possible)

    // ── Progress reporting ──────────────────────────────────────────────────

    function _maybe_progress() {
        if (_take_progress_checkpoint(_checked)) {
            const msg = {
                type: 'progress',
                worker_id: _cfg.worker_id,
                checked: _checked,
                precheck_pass: _precheck_pass,
                precheck_reject: _precheck_reject,
                feasible: _feasible,
                met_req: _met_req,
                checked_since_top5: _checked - _checked_at_last_top5_change,
                L_progress: [_current_L, L_max],
                trace: _trace_snapshot() ?? undefined,
            };
            // Only include top5 data when it has actually changed
            if (_top5_version !== _last_sent_top5_version) {
                msg.top5_names = _top5.map(r => ({
                    score: r.score, item_names: r.item_names,
                    base_sp: r.base_sp, total_sp: r.total_sp, assigned_sp: r.assigned_sp,
                }));
                _last_sent_top5_version = _top5_version;
            }
            postMessage(msg);
        }
    }

    // ── Greedy extra-SP allocator ───────────────────────────────────────────
    //
    // After the minimum SP is assigned to meet item requirements, any
    // remaining budget is greedily distributed to maximise the scoring target.
    // Uses geometric step-down (20 → 4 → 1) for O(50-95) trials worst case.

    // ── In-place greedy trial evaluation ───────────────────────────────────
    //
    // assemble_combo_stats deep-clones two ~170-entry Maps per call, and the
    // greedy loop calls it up to ~95 times per feasible leaf. Between trials
    // only the five skill-point entries change, so instead we build the
    // trial base once per leaf (clone + classDef + atree_raw + radiance —
    // radiance_affected excludes the SP stats, so its floors are independent
    // of the per-trial SP values) and per trial: overwrite the five SP keys,
    // run atree scaling, merge the scaled + static stats in place with an
    // undo journal, score, and roll the journal back. Addition order matches
    // assemble_combo_stats exactly, so trial scores are bit-identical.

    const _scratch_trial_pre = new Map();
    const _scratch_trial_pre_nested = { damMult: new Map(), defMult: new Map(), healMult: new Map() };
    const _trial_raw_skp = [0, 0, 0, 0, 0];
    const _UNDO_ABSENT = Symbol('absent');
    const _undo_maps = [];
    const _undo_keys = [];
    const _undo_vals = [];
    let _undo_len = 0;

    function _undo_record(map, key) {
        _undo_maps[_undo_len] = map;
        _undo_keys[_undo_len] = key;
        _undo_vals[_undo_len] = map.has(key) ? map.get(key) : _UNDO_ABSENT;
        _undo_len++;
    }

    function _undo_rollback() {
        for (let j = _undo_len - 1; j >= 0; j--) {
            const m = _undo_maps[j], k = _undo_keys[j], v = _undo_vals[j];
            if (v === _UNDO_ABSENT) m.delete(k); else m.set(k, v);
            _undo_maps[j] = null;  // release references
        }
        _undo_len = 0;
    }

    /** merge_stat (build_utils.js) with every mutation journaled for undo. */
    function _merge_stat_undo(stats, name, value) {
        const [start, end] = name.split('.', 2);
        if (start === 'damMult' || start === 'defMult' || start === 'healMult' || start === 'manaMult') {
            if (!stats.has(start)) {
                _undo_record(stats, start);
                stats.set(start, new Map());
            }
            const map = stats.get(start);
            if (value instanceof Map) {
                for (const [k, v] of value.entries()) {
                    _merge_stat_undo(map, k, v);
                }
                return;
            }
            if (nonstacking_stats.includes(end)) {
                let highest = stats.get(start).get(end);
                if (highest !== undefined) {
                    if (value > highest) {
                        _undo_record(map, end);
                        map.set(end, value);
                        stats.set(start, map);
                    }
                    return;
                }
            }
            _merge_stat_undo(map, name.slice(name.indexOf('.') + 1), value);
            return;
        }
        _undo_record(stats, name);
        if (stats.has(name)) {
            stats.set(name, stats.get(name) + value);
        }
        else { stats.set(name, value); }
    }

    /** _merge_into (pure/utils.js) with journaling. */
    function _merge_into_undo(target, source) {
        if (!source) return;
        for (const [k, v] of source) {
            if (v instanceof Map) {
                for (const [mk, mv] of v) _merge_stat_undo(target, k + '.' + mk, mv);
            } else {
                _merge_stat_undo(target, k, v);
            }
        }
    }

    function _greedy_allocate_sp(build_sm, base_sp, total_sp, assigned_sp, weapon_sm) {
        const remaining = sp_budget - assigned_sp;

        const target = _cfg.scoring_target ?? 'combo_damage';
        const need_thresh = (target !== 'combo_damage' && target !== 'total_healing');

        // Trial base is built lazily: greedy_sp_allocate can return without
        // ever scoring (no remaining budget / no room).
        let trial_ready = false;
        let P = null;

        function _trial_score() {
            if (!trial_ready) {
                P = _deep_clone_statmap_into(_scratch_trial_pre, build_sm, _scratch_trial_pre_nested);
                const weaponType = weapon_sm.get('type');
                if (weaponType) P.set('classDef', classDefenseMultipliers.get(weaponType) || 1.0);
                _merge_into(P, _cfg.atree_raw);
                _apply_radiance_scale_inplace(P, _cfg.radiance_boost);
                for (let i = 0; i < 5; i++) {
                    _trial_raw_skp[i] = _cfg.atree_raw?.get(skp_order[i]) ?? 0;
                }
                trial_ready = true;
            }
            // assemble_combo_stats sets skp to total_sp then adds atree_raw;
            // writing the sum directly is the same two-operand addition.
            for (let i = 0; i < 5; i++) {
                P.set(skp_order[i], total_sp[i] + _trial_raw_skp[i]);
            }
            let atree_scaled_stats;
            let atree_var_stats = null;
            if (_atree_scaled_cache) {
                atree_scaled_stats = _atree_scaled_cache;
            } else if (_atree_split) {
                atree_scaled_stats = _atree_split.const_scaled;
                atree_var_stats = atree_eval_stat_effects(
                    _cfg.atree_merged, _atree_split.var_effects, P, _scratch_var_scaled);
            } else {
                [, atree_scaled_stats] = atree_compute_scaling(
                    _cfg.atree_merged, P, _cfg.button_states, _cfg.slider_states,
                    _scratch_atree, _atree_skip_edit);
            }
            _merge_into_undo(P, atree_scaled_stats);
            if (atree_var_stats) _merge_into_undo(P, atree_var_stats);
            _merge_into_undo(P, _cfg.static_boosts);
            const s = _eval_score(P, P);
            _undo_rollback();
            return s;
        }

        const cap_total = _sp_caps ?? _default_sp_caps;
        // Snapshot base_sp for mana rescue. on_post_floor re-snapshots after
        // floor enforcement so rescue sees post-floor (not post-greedy) state.
        _scratch_orig_base_sp.set(base_sp);
        assigned_sp += greedy_sp_allocate(base_sp, total_sp, remaining, cap_total,
            _sp_floors, _trial_score, () => _scratch_orig_base_sp.set(base_sp));

        return assigned_sp;
    }

    // ── Mana rescue: shift SP into Int when mana check fails ───────────────
    //
    // After greedy allocation optimises for score, the mana check may fail.
    // This function attempts to steal freely-assigned SP from other attributes
    // and shift it into Int (which reduces spell costs and increases mana pool).
    // Only applies to non-Blood-Pact builds with a combo_time constraint.
    //
    // Returns true if rescue succeeded (base_sp/total_sp mutated, combo_base
    // reassembled). Returns false if rescue impossible or insufficient.

    const _scratch_rescue_base = new Int32Array(5);
    const _scratch_rescue_total = new Int32Array(5);

    function _mana_rescue(build_sm, base_sp, total_sp, orig_base_sp, weapon_sm) {
        if (_cfg.hp_casting && _cfg.health_config?.health_cost > 0) return false;
        if (!(_cfg.combo_time ?? 0)) return false;

        const INT_IDX = 2;
        const int_base_room = 100 - base_sp[INT_IDX];
        const int_total_room = ((_sp_caps ? _sp_caps[INT_IDX] : 150) - total_sp[INT_IDX]);
        const int_room = Math.min(int_base_room, int_total_room);
        if (int_room <= 0) return false;

        // Compute how much SP the greedy allocator freely assigned per attribute
        // (i.e. beyond the SP solver minimum). These are stealable.
        let total_stealable = 0;
        const stealable = [0, 0, 0, 0, 0];
        for (let i = 0; i < 5; i++) {
            if (i === INT_IDX) continue;
            stealable[i] = base_sp[i] - orig_base_sp[i];
            total_stealable += stealable[i];
        }
        if (total_stealable <= 0) return false;

        const max_shift = Math.min(total_stealable, int_room);
        if (max_shift <= 0) return false;

        // Save current allocation in case rescue fails
        _scratch_rescue_base.set(base_sp);
        _scratch_rescue_total.set(total_sp);

        // Try shifting SP into Int in increasing amounts: 25%, 50%, 75%, 100% of max
        for (const frac of [0.25, 0.5, 0.75, 1.0]) {
            let shift_target = Math.ceil(max_shift * frac);
            if (shift_target <= 0) continue;

            // Restore to pre-rescue state
            for (let i = 0; i < 5; i++) {
                base_sp[i] = _scratch_rescue_base[i];
                total_sp[i] = _scratch_rescue_total[i];
            }

            // Steal from attributes with most free SP first
            let shifted = 0;
            // Build sorted steal order (descending by stealable)
            const order = [0, 1, 3, 4]; // exclude INT_IDX=2
            order.sort((a, b) => stealable[b] - stealable[a]);

            for (const i of order) {
                if (shifted >= shift_target) break;
                const take = Math.min(stealable[i], shift_target - shifted);
                if (take <= 0) continue;
                base_sp[i] -= take;
                total_sp[i] -= take;
                shifted += take;
            }

            // Add stolen SP to Int
            base_sp[INT_IDX] += shifted;
            total_sp[INT_IDX] += shifted;

            // Reassemble combo stats and check mana
            _assemble_combo_stats(build_sm, total_sp, weapon_sm);
            if (_eval_combo_mana_check(_scratch_combo_base)) {
                return true;  // Rescue succeeded; combo_base (scratch) is valid
            }
        }

        // All attempts failed — restore original allocation
        for (let i = 0; i < 5; i++) {
            base_sp[i] = _scratch_rescue_base[i];
            total_sp[i] = _scratch_rescue_total[i];
        }
        return false;
    }

    // ── Leaf evaluation ─────────────────────────────────────────────────────

    function _evaluate_leaf() {
        _checked++;
        if (_trace) _trace.leaf_count++;
        const precheck_t0 = _trace_start('precheck');

        // Fast constraint precheck: reject builds that can't meet simple
        // additive stat thresholds, before expensive SP solver + stat assembly.
        // running_sm has all item stats accumulated; prechecks account for
        // fixed contributions (atree_raw + static_boosts).
        if (_constraint_prechecks.length > 0 && !_fast_constraint_precheck(running_sm)) {
            _trace_end('precheck', precheck_t0);
            _dbg_precheck_reject++;
            _precheck_reject++;
            _maybe_progress();
            return;
        }
        if (!_fast_ehp_precheck(running_sm)) {
            _trace_end('precheck', precheck_t0);
            _dbg_ehp_reject++;
            _precheck_reject++;
            _maybe_progress();
            return;
        }
        _trace_end('precheck', precheck_t0);
        _precheck_pass++;

        // Fill scratch arrays (pointer writes only, no allocation)
        _scratch_equip_8[0] = partial.helmet.statMap;
        _scratch_equip_8[1] = partial.chestplate.statMap;
        _scratch_equip_8[2] = partial.leggings.statMap;
        _scratch_equip_8[3] = partial.boots.statMap;
        _scratch_equip_8[4] = partial.ring1.statMap;
        _scratch_equip_8[5] = partial.ring2.statMap;
        _scratch_equip_8[6] = partial.bracelet.statMap;
        _scratch_equip_8[7] = partial.necklace.statMap;

        // SP input: 8 equips + guild_tome (reuse scratch)
        for (let i = 0; i < 8; i++) _scratch_sp_input[i] = _scratch_equip_8[i];
        _scratch_sp_input[8] = guild_tome_sm;

        // Combined SP feasibility check + full calculation (single pass).
        const sp_t0 = _trace_start('sp');
        const sp_result = calculate_skillpoints(_scratch_sp_input, weapon_sm, sp_budget, _scratch_sp_set_counts, _scratch_sp);
        _trace_end('sp', sp_t0);
        if (!sp_result) {
            _dbg_sp_reject++;
            _maybe_progress();
            return;
        }
        const base_sp = sp_result[0];
        const total_sp = sp_result[1];
        const assigned_sp = sp_result[2];
        const activeSetCounts = sp_result[3];
        _feasible++;

        // Build stat assembly from running statMap (incremental accumulation)
        const t0 = _dbg ? performance.now() : 0;
        // Fill _scratch_all_equip: 8 equips + tomes + weapon (no allocation)
        for (let i = 0; i < 8; i++) _scratch_all_equip[i] = _scratch_equip_8[i];
        for (let i = 0; i < tome_sms.length; i++) _scratch_all_equip[8 + i] = tome_sms[i];
        _scratch_all_equip[8 + tome_sms.length] = weapon_sm;
        const finalize_t0 = _trace_start('finalize');
        const build_sm = _finalize_leaf_statmap(running_sm, weapon_sm, activeSetCounts, sets, _scratch_all_equip, _scratch_finalize, _scratch_finalize_inner);
        _trace_end('finalize', finalize_t0);

        // Score-ceiling gate: when combo damage is provably monotone in SP,
        // one damage evaluation at all-150 SP upper-bounds anything the
        // greedy allocator can reach. If even that ceiling cannot beat the
        // current top-N cutoff, the leaf's final score would be rejected at
        // insertion — skip greedy + mana + score with an identical top-N.
        if (_ceiling_gate_ok && _top5.length >= 15) {
            const ceiling_t0 = _trace_start('ceiling');
            const cb150 = _assemble_combo_stats(build_sm, _SP_CEILING, weapon_sm);
            _cached_hp_sim = null;
            const ceiling = _eval_combo_damage(cb150);
            _trace_end('ceiling', ceiling_t0);
            const cutoff = _top5[14].score;
            // Strict margin: a float-ulp monotonicity wobble must never gate
            // a genuine top-N candidate.
            if (ceiling < cutoff - Math.abs(cutoff) * 1e-9) {
                _dbg_ceiling_skip++;
                if (_dbg) _dbg_leaf_time += performance.now() - t0;
                _maybe_progress();
                return;
            }
        }

        // Greedily assign any remaining SP budget to maximise the scoring target
        const greedy_t0 = _trace_start('greedy');
        let final_assigned = _greedy_allocate_sp(build_sm, base_sp, total_sp, assigned_sp, weapon_sm);
        _trace_end('greedy', greedy_t0);

        // Stat assembly + atree scaling
        const assemble_t0 = _trace_start('assemble');
        const combo_base = _assemble_combo_stats(build_sm, total_sp, weapon_sm);
        _trace_end('assemble', assemble_t0);

        // Compute thresh_stats once: used for threshold gate and non-damage scoring
        const need_thresh = restrictions.stat_thresholds.length > 0
            || (_cfg.scoring_target ?? 'combo_damage') !== 'combo_damage';
        const threshold_t0 = _trace_start('threshold');
        let thresh_stats = need_thresh ? _assemble_threshold_stats(combo_base) : null;

        // Threshold check
        if (restrictions.stat_thresholds.length > 0) {
            if (!_check_thresholds(thresh_stats, restrictions.stat_thresholds)) {
                _trace_end('threshold', threshold_t0);
                _dbg_threshold_reject++;
                if (_dbg) _dbg_leaf_time += performance.now() - t0;
                _maybe_progress();
                return;
            }
        }
        _trace_end('threshold', threshold_t0);

        // Mana / HP constraint check (with rescue attempt on failure)
        const mana_t0 = _trace_start('mana');
        let mana_hp_result = _eval_combo_mana_check(combo_base);
        if (!mana_hp_result) {
            if (!_cfg.hp_casting && _mana_rescue(build_sm, base_sp, total_sp, _scratch_orig_base_sp, weapon_sm)) {
                // Rescue succeeded — combo_base was reassembled by _mana_rescue.
                // Re-check thresholds since SP distribution changed.
                if (restrictions.stat_thresholds.length > 0) {
                    const ts2 = _assemble_threshold_stats(combo_base);
                    if (!_check_thresholds(ts2, restrictions.stat_thresholds)) {
                        _trace_end('mana', mana_t0);
                        _dbg_threshold_reject++;
                        if (_dbg) _dbg_leaf_time += performance.now() - t0;
                        _maybe_progress();
                        return;
                    }
                    thresh_stats = ts2;
                } else if (need_thresh) {
                    thresh_stats = _assemble_threshold_stats(combo_base);
                }
                // final_assigned unchanged: rescue is a zero-sum redistribution
                _dbg_mana_rescued++;
                mana_hp_result = true;
            }
            if (!mana_hp_result) {
                _trace_end('mana', mana_t0);
                if (_cfg.hp_casting) _dbg_hp_reject++; else _dbg_mana_reject++;
                if (_dbg) _dbg_leaf_time += performance.now() - t0;
                _maybe_progress();
                return;
            }
        }
        _trace_end('mana', mana_t0);

        // Score
        const score_t0 = _trace_start('score');
        const score = _eval_score(combo_base, thresh_stats);
        _trace_end('score', score_t0);
        _dbg_scored++;
        _met_req++;
        if (_dbg) _dbg_leaf_time += performance.now() - t0;
        const topn_t0 = _trace_start('topn');
        _insert_top5(score, () => {
            const item_names = _scratch_equip_8.map(sm => _get_item_name(sm));
            // Clone SP arrays only for competitive results; they alias scratch buffers.
            const entry = { score, item_names, base_sp: base_sp.slice(), total_sp: total_sp.slice(), assigned_sp: final_assigned };
            if (SOLVER_DEBUG_COMBO) entry._debug_combo_base = _deep_clone_statmap(combo_base);
            return entry;
        });
        _trace_end('topn', topn_t0);
        _maybe_progress();
    }

    // ── Stat tracking helpers ────────────────────────────────────────────────

    // Precompile each pool item's vector entries onto its wrapper so the hot
    // place/unplace path reads a plain property instead of a WeakMap.
    if (_running_is_vec) {
        for (const pool of [...Object.values(pools), ring_pool ?? []]) {
            if (!pool) continue;
            for (const item of pool) {
                if (!item._vec_entries) item._vec_entries = _vec_entries(item.statMap);
            }
        }
    }
    const _place_item = _running_is_vec
        ? (item) => _vec_add_entries(running_sm, item._vec_entries)
        : (item) => _incr_add_item(running_sm, item.statMap);
    const _unplace_item = _running_is_vec
        ? (item) => _vec_remove_entries(running_sm, item._vec_entries)
        : (item) => _incr_remove_item(running_sm, item.statMap);

    // ── Level-based enumeration over free armor/accessory slots ─────────────
    //
    // enumerate(slot_idx, remaining_L) tries all offsets 0..min(remaining_L, pool.size-1)
    // for the current slot, recurses with (remaining_L - offset) for the next slot.
    // The outer loop iterates L = 0, 1, ..., L_max so combinations are visited in
    // increasing order of sum-of-rank-offsets: best build first, then one step away, etc.

    // Effective offset bounds per free slot (respects partition restrictions).
    // Armor 'slot' partition is already baked into pools[slot] via slicing above;
    // ring partitions restrict offsets without modifying the shared ring_pool.
    const _slot_lb = new Array(N_free);
    const _slot_ub = new Array(N_free);
    for (let d = 0; d < N_free; d++) {
        const pool = _get_pool(free_slots[d]);
        let lb = 0;
        let ub = pool ? pool.length - 1 : -1;
        if (d === ring1_depth && partition?.type === 'ring') {
            lb = Math.max(lb, partition.start);
            ub = Math.min(ub, partition.end - 1);
        }
        if ((d === ring1_depth || d === ring2_depth) && partition?.type === 'ring_single') {
            lb = Math.max(lb, partition.start);
            ub = Math.min(ub, partition.end - 1);
        }
        _slot_lb[d] = lb;
        _slot_ub[d] = ub;
    }

    // Max achievable level, respecting partition bounds.
    let L_max = 0;
    for (let d = 0; d < N_free; d++) {
        if (_slot_ub[d] > 0) L_max += _slot_ub[d];
    }

    // ── Subtree leaf count table (for SP-prune + illegal-set-prune tallying) ─
    //
    // _subtree_leaf_count[d][L] = number of leaf builds in the subtree from
    // depth d..N_free-1 with remaining level budget L.  Respects partition
    // bounds and ring-pair symmetry (ring2_offset >= ring1_offset).  Used to
    // credit pruned subtrees to _checked so checked converges to total.
    //
    // When both rings are free, the entry at ring2_depth depends on the
    // offset ring1 was placed at; we rebuild it each time ring1 is placed.

    const both_rings_free = ring1_depth >= 0 && ring2_depth >= 0;
    const rings_contiguous = both_rings_free && ring2_depth === ring1_depth + 1;

    const _subtree_leaf_count = new Array(N_free + 1);
    _subtree_leaf_count[N_free] = new Float64Array(L_max + 1);
    _subtree_leaf_count[N_free][0] = 1;

    // Ring-pair count: number of (a, b) pairs with a+b=L, a in ring1 bounds,
    // b in ring2 bounds, b >= a (symmetry).  Used when both rings still to place.
    let _ring_pair_count = null;
    if (both_rings_free && rings_contiguous) {
        _ring_pair_count = new Float64Array(L_max + 1);
        const lb1 = _slot_lb[ring1_depth], ub1 = _slot_ub[ring1_depth];
        const lb2 = _slot_lb[ring2_depth], ub2 = _slot_ub[ring2_depth];
        for (let a = lb1; a <= ub1; a++) {
            const b_lo = Math.max(a, lb2);
            for (let b = b_lo; b <= ub2; b++) {
                if (a + b <= L_max) _ring_pair_count[a + b]++;
            }
        }
    }

    for (let d = N_free - 1; d >= 0; d--) {
        const arr = new Float64Array(L_max + 1);
        _subtree_leaf_count[d] = arr;

        if (both_rings_free && rings_contiguous && d === ring2_depth) {
            // Placeholder — filled by _rebuild_ring2_subtree_leaf_count on ring1 placement.
            continue;
        }
        if (both_rings_free && rings_contiguous && d === ring1_depth) {
            // Fold the ring-pair count with the tail from d+2 (post-ring2 slots).
            const tail = _subtree_leaf_count[d + 2];
            for (let Lp = 0; Lp <= L_max; Lp++) {
                const c = _ring_pair_count[Lp];
                if (c === 0) continue;
                for (let Lt = 0; Lt + Lp <= L_max; Lt++) {
                    const t = tail[Lt];
                    if (t !== 0) arr[Lp + Lt] += c * t;
                }
            }
            continue;
        }
        // Normal slot: offset in [lb, ub] contributes to sum; tail from d+1.
        const tail = _subtree_leaf_count[d + 1];
        const lb = _slot_lb[d], ub = _slot_ub[d];
        if (lb > ub) continue;  // empty slot — no valid placements
        const prefix = new Float64Array(L_max + 2);
        for (let L = 0; L <= L_max; L++) prefix[L + 1] = prefix[L] + tail[L];
        for (let L = 0; L <= L_max; L++) {
            const lo = Math.max(0, L - ub);
            const hi_incl = L - lb;
            if (hi_incl < lo) continue;
            const hi = Math.min(L_max, hi_incl);
            arr[L] = prefix[hi + 1] - prefix[lo];
        }
    }

    // ── Band accounting ──────────────────────────────────────────────────────
    //
    // The enumeration visits leaves in geometric LEVEL BANDS [lo, hi] instead
    // of one level at a time: band widths double (1, 2, 4, ...) so the early,
    // quality-critical ordering stays fine-grained while deep-tail prefixes
    // are re-walked O(log L_max) times instead of O(L_max). Each tuple's
    // level falls in exactly one band, so the visited set and all funnel
    // totals are unchanged; only the within-band visit order differs from the
    // strict per-level ordering.
    //
    // _subtree_prefix[d][t+1] = Σ_{u<=t} _subtree_leaf_count[d][u], so a
    // pruned child subtree's in-band leaves are credited with one prefix
    // difference.

    const _subtree_prefix = new Array(N_free + 1);
    function _rebuild_subtree_prefix(d) {
        let p = _subtree_prefix[d];
        if (!p) p = _subtree_prefix[d] = new Float64Array(L_max + 2);
        const row = _subtree_leaf_count[d];
        for (let t = 0; t <= L_max; t++) p[t + 1] = p[t] + row[t];
    }
    for (let d = 0; d <= N_free; d++) _rebuild_subtree_prefix(d);

    /** Leaves below depth d with remaining rank sum in [lo, hi]. */
    function _band_credit(d, lo, hi) {
        if (hi < 0) return 0;
        const p = _subtree_prefix[d];
        const lo_c = lo > 0 ? lo : 0;
        const hi_c = hi < L_max ? hi : L_max;
        if (lo_c > hi_c) return 0;
        return p[hi_c + 1] - p[lo_c];
    }

    // Max achievable rank sum from depth d onward (band reachability).
    const _suffix_max_rank = new Int32Array(N_free + 1);
    for (let d = N_free - 1; d >= 0; d--) {
        _suffix_max_rank[d] = _suffix_max_rank[d + 1]
            + (_slot_ub[d] > 0 ? _slot_ub[d] : 0);
    }

    // Rebuild _subtree_leaf_count[ring2_depth] for the given ring1 placement.
    // Only meaningful when both rings free and contiguous.
    function _rebuild_ring2_subtree_leaf_count(ring1_offset) {
        if (!(both_rings_free && rings_contiguous)) return;
        const arr = _subtree_leaf_count[ring2_depth];
        arr.fill(0);
        const lb2 = Math.max(_slot_lb[ring2_depth], ring1_offset);
        const ub2 = _slot_ub[ring2_depth];
        if (lb2 <= ub2) {
            const tail = _subtree_leaf_count[ring2_depth + 1];
            const prefix = new Float64Array(L_max + 2);
            for (let L = 0; L <= L_max; L++) prefix[L + 1] = prefix[L] + tail[L];
            for (let L = 0; L <= L_max; L++) {
                const lo = Math.max(0, L - ub2);
                const hi_incl = L - lb2;
                if (hi_incl < lo) continue;
                const hi = Math.min(L_max, hi_incl);
                arr[L] = prefix[hi + 1] - prefix[lo];
            }
        }
        _rebuild_subtree_prefix(ring2_depth);
    }

    // ── Mid-tree SP pruning state & helpers ──────────────────────────────────
    //
    // Track running SP requirements and provisions as free armor items are
    // placed/unplaced during enumerate().  An optimistic feasibility check
    // prunes subtrees where SP assignment provably exceeds the budget.

    // Fixed-item SP baseline (recomputed once per ring combination).
    const _sp_fixed_max_eff_req = [0, 0, 0, 0, 0];
    const _sp_fixed_sum_prov = [0, 0, 0, 0, 0];

    // Per-depth snapshot of the running max taken just before placing at
    // that depth, so unplace is an O(5) restore instead of an O(depth*5)
    // recompute from the fixed baseline.
    const _sp_max_save = [];
    for (let d = 0; d < N_free; d++) _sp_max_save.push([0, 0, 0, 0, 0]);

    // Running max eff req (fixed + placed free) and running free provisions.
    const _sp_running_max_eff_req = [0, 0, 0, 0, 0];
    const _sp_running_free_prov = [0, 0, 0, 0, 0];

    /**
     * Compute SP baseline from all fixed items (locked equips, guild tome,
     * weapon).  Called once before the unified level enumeration.
     */
    function _sp_compute_fixed_baseline() {
        _sp_fixed_max_eff_req.fill(0);
        _sp_fixed_sum_prov.fill(0);

        const free_set = new Set(free_slots);
        for (const [slot, item] of Object.entries(partial)) {
            if (free_set.has(slot)) continue;
            if (!item || !item.statMap || item.statMap.has('NONE')) continue;
            const sm = item.statMap;
            const skp = sm.get('skillpoints');
            const req = sm.get('reqs');
            const is_crafted = sm.get('crafted');

            if (!is_crafted) {
                for (let i = 0; i < 5; i++) {
                    if (skp[i] > 0) _sp_fixed_sum_prov[i] += skp[i];
                }
            }

            // Raw requirements (cascade: no self-contribution undoing)
            for (let i = 0; i < 5; i++) {
                const eff = req[i];
                if (eff > _sp_fixed_max_eff_req[i])
                    _sp_fixed_max_eff_req[i] = eff;
            }
        }

        // Guild tome: adds provisions + effective reqs
        if (guild_tome_sm && !guild_tome_sm.has('NONE')) {
            const skp = guild_tome_sm.get('skillpoints');
            const req = guild_tome_sm.get('reqs');
            for (let i = 0; i < 5; i++) {
                if (skp[i] > 0) _sp_fixed_sum_prov[i] += skp[i];
            }
            for (let i = 0; i < 5; i++) {
                const eff = req[i];
                if (eff > _sp_fixed_max_eff_req[i])
                    _sp_fixed_max_eff_req[i] = eff;
            }
        }

        // Weapon: raw requirements only, excluded from prov
        const wep_req = weapon_sm.get('reqs');
        for (let i = 0; i < 5; i++) {
            if (wep_req[i] > _sp_fixed_max_eff_req[i])
                _sp_fixed_max_eff_req[i] = wep_req[i];
        }
    }

    /**
     * Reset running SP state and compute fixed baseline.
     * Called once before the unified level enumeration.
     */
    function _sp_reset() {
        _sp_compute_fixed_baseline();
        _sp_running_free_prov.fill(0);
        for (let i = 0; i < 5; i++)
            _sp_running_max_eff_req[i] = _sp_fixed_max_eff_req[i];
    }

    /**
     * Update running SP state when placing a free item at a given depth.
     */
    function _sp_place_free_item(sm, depth) {
        const skp = sm.get('skillpoints');
        const req = sm.get('reqs');
        const is_crafted = sm.get('crafted');

        if (!is_crafted) {
            for (let i = 0; i < 5; i++) {
                if (skp[i] > 0) _sp_running_free_prov[i] += skp[i];
            }
        }

        // Snapshot the running max, then fold in this item's raw requirements
        // (cascade: no self-contribution undoing).
        const save = _sp_max_save[depth];
        for (let i = 0; i < 5; i++) {
            save[i] = _sp_running_max_eff_req[i];
            if (req[i] > _sp_running_max_eff_req[i])
                _sp_running_max_eff_req[i] = req[i];
        }
    }

    /**
     * Restore running SP state when unplacing a free item at a given depth.
     * Placement is strictly LIFO, so restoring the pre-place snapshot is exact.
     */
    function _sp_unplace_free_item(sm, depth) {
        if (!sm.get('crafted')) {
            const skp = sm.get('skillpoints');
            for (let i = 0; i < 5; i++) {
                if (skp[i] > 0) _sp_running_free_prov[i] -= skp[i];
            }
        }

        const save = _sp_max_save[depth];
        for (let i = 0; i < 5; i++) _sp_running_max_eff_req[i] = save[i];
    }

    /**
     * Returns true if the subtree rooted at next_depth might contain a
     * feasible build (SP-wise).  Returns false to prune.
     */
    function _sp_mid_tree_feasible(next_depth) {
        if (next_depth >= N_free) return true;

        let total_deficit = 0;
        for (let i = 0; i < 5; i++) {
            if (_sp_running_max_eff_req[i] === 0) continue;
            const optimistic_prov = _sp_fixed_sum_prov[i]
                + _sp_running_free_prov[i]
                + _sp_suffix_max_prov[next_depth][i];
            if (_sp_running_max_eff_req[i] <= optimistic_prov) continue;
            const deficit = _sp_running_max_eff_req[i] - optimistic_prov;
            if (deficit > SP_PER_ATTR_CAP) return false;
            total_deficit += deficit;
            if (total_deficit > sp_budget) return false;
        }
        return true;
    }

    /**
     * Leaf-level SP feasibility check — all items are placed, no suffix term.
     * Identical logic to _sp_mid_tree_feasible but without the suffix
     * optimistic provision (no remaining slots) and no early return.
     * ~20 int ops, zero allocation — cheapest possible rejection at the leaf.
     */
    function _sp_leaf_feasible() {
        let total_deficit = 0;
        for (let i = 0; i < 5; i++) {
            if (_sp_running_max_eff_req[i] === 0) continue;
            const prov = _sp_fixed_sum_prov[i] + _sp_running_free_prov[i];
            if (_sp_running_max_eff_req[i] <= prov) continue;
            const deficit = _sp_running_max_eff_req[i] - prov;
            if (deficit > SP_PER_ATTR_CAP) return false;
            total_deficit += deficit;
            if (total_deficit > sp_budget) return false;
        }
        return true;
    }

    // Track ring1's placed offset for ring2 symmetry constraint (ring2 offset >= ring1 offset).
    let _ring1_placed_offset = 0;

    // enumerate(slot_idx, lo_rem, hi_rem): visit every completion of the
    // current prefix whose remaining rank sum lies in [lo_rem, hi_rem].
    // lo_rem may go <= 0 (no lower bound left); hi_rem is the budget.
    function enumerate(slot_idx, lo_rem, hi_rem) {
        if (_cancelled) return;

        if (slot_idx === N_free) {
            _evaluate_leaf();
            return;
        }

        const slot = free_slots[slot_idx];
        const pool = _get_pool(slot);
        if (!pool) { enumerate(slot_idx + 1, lo_rem, hi_rem); return; }

        const is_ring1 = (slot_idx === ring1_depth);
        const is_ring2 = (slot_idx === ring2_depth);

        // Compute offset bounds for this slot.
        let min_offset = 0;
        let pool_max = pool.length - 1;

        // Ring2 symmetry: offset >= ring1's offset (deduplicates symmetric pairs).
        if (is_ring2 && ring1_depth >= 0) {
            min_offset = _ring1_placed_offset;
        }
        // Ring partition: restrict ring1 (or single free ring) offset range.
        if (is_ring1 && partition?.type === 'ring') {
            min_offset = Math.max(min_offset, partition.start);
            pool_max = Math.min(pool_max, partition.end - 1);
        }
        if ((is_ring1 || is_ring2) && partition?.type === 'ring_single') {
            min_offset = Math.max(min_offset, partition.start);
            pool_max = Math.min(pool_max, partition.end - 1);
        }

        // ── Hoisted bound bases (constant across this slot's offsets) ──────
        // running-after-place[x] == running-before[x] + column[x] for every
        // bound-relevant value, so testing base + column reproduces the
        // post-placement checks exactly, without placing.
        const next_depth = slot_idx + 1;
        const req_col = _col_req[slot_idx];
        const prov_col = _col_prov[slot_idx];
        const pc_col = _col_pc[slot_idx];
        const hp_col = _col_hp[slot_idx];
        const is_leaf_slot = (slot_idx === N_free - 1);

        {
            // Leaf slot uses no suffix (mirrors _sp_leaf_feasible / the leaf
            // prechecks); interior slots add the child-depth suffix.
            const prov_sfx = _hoist_prov_sfx[slot_idx];
            const sfx = is_leaf_slot ? null : _sp_suffix_max_prov[next_depth];
            for (let j = 0; j < 5; j++) {
                prov_sfx[j] = _sp_fixed_sum_prov[j] + _sp_running_free_prov[j] + (sfx ? sfx[j] : 0);
            }
            if (pc_col) {
                const pc_base = _hoist_pc_base[slot_idx];
                const row = next_depth * _n_pc;
                for (let i = 0; i < _n_pc; i++) {
                    const pc = _constraint_prechecks[i];
                    pc_base[i] = _running_get(pc.stat, pc.stat_idx)
                        + (is_leaf_slot ? 0 : _pc_suffix[row + i]);
                }
            }
            if (hp_col) {
                _hoist_hp_base[slot_idx] = _running_get('hp', _vec_hp_idx)
                    + _running_get('hpBonus', _vec_hpBonus_idx)
                    + (is_leaf_slot ? 0 : _hp_suffix[next_depth]);
            }
        }

        // For the last free slot, the leaf's total rank equals its offset, so
        // the in-band offsets form the exact range [lo_rem, hi_rem]. Each
        // tuple's level lies in exactly one band, so no duplicates.
        if (is_leaf_slot) {
            const from = Math.max(min_offset, lo_rem);
            const to = Math.min(pool_max, hi_rem);
            for (let offset = from; offset <= to; offset++) {
                if (_cancelled) return;
                const item = pool[offset];
                const is = item._illegalSet;
                const iname = item._illegalSetName;
                if (tracker.blocks(is, iname)) {
                    // Illegal-set blocked — the single leaf for this tuple is still
                    // counted toward total, so credit it to _checked.
                    _checked++;
                    _maybe_progress();
                } else if (!_sp_bound_ok(slot_idx, offset)) {
                    // Same outcome _sp_leaf_feasible would produce after placing.
                    _checked++;
                    _dbg_sp_leaf_reject++;
                    _maybe_progress();
                } else if (_restr_pruning_active && !_restr_bound_ok(slot_idx, offset)) {
                    // Same outcome the leaf prechecks would produce after placing.
                    _checked++;
                    _precheck_reject++;
                    _dbg_precheck_reject++;
                    _maybe_progress();
                } else {
                    if (is) tracker.add(is, iname);
                    partial[slot] = item;
                    _place_item(item);
                    _sp_place_free_item(item.statMap, slot_idx);
                    _evaluate_leaf();
                    _sp_unplace_free_item(item.statMap, slot_idx);
                    _unplace_item(item);
                    if (is) tracker.remove(is, iname);
                }
            }
            partial[slot] = locked[slot] ?? none_items_wrapped[_cfg.none_idx_map[slot]];
            return;
        }

        // Band reachability: offsets too small to reach lo_rem with the
        // remaining slots' maximum ranks have no in-band leaves (they were
        // all visited in earlier bands).
        const reach_min = lo_rem - _suffix_max_rank[next_depth];
        if (reach_min > min_offset) min_offset = reach_min;
        const max_offset = Math.min(hi_rem, pool_max);

        for (let offset = min_offset; offset <= max_offset; offset++) {
            if (_cancelled) return;
            const item = pool[offset];
            const is = item._illegalSet;
            const iname = item._illegalSetName;
            if (tracker.blocks(is, iname)) {
                // Illegal-set blocked — credit the subtree leaves that would
                // have been enumerated below this placement so _checked tracks
                // the same tuple space as total.
                // When the blocked slot is ring1, the tail count depends on
                // ring1's offset; use this offset for the symmetry bound.
                if (is_ring1 && both_rings_free && rings_contiguous) {
                    _rebuild_ring2_subtree_leaf_count(offset);
                }
                const skipped = _band_credit(next_depth, lo_rem - offset, hi_rem - offset);
                _checked += skipped;
                _maybe_progress();
                continue;
            }

            if (!_sp_bound_ok(slot_idx, offset)) {
                if (is_ring1 && both_rings_free && rings_contiguous) {
                    _rebuild_ring2_subtree_leaf_count(offset);
                }
                const pruned = _band_credit(next_depth, lo_rem - offset, hi_rem - offset);
                _checked += pruned;
                _dbg_sp_prune_count += pruned;
                _maybe_progress();
                continue;
            }
            if (_restr_pruning_active && !_restr_bound_ok(slot_idx, offset)) {
                // Every completion would fail the leaf precheck — credit the
                // subtree to checked and precheck_reject so funnel totals
                // match the unpruned enumeration exactly.
                if (is_ring1 && both_rings_free && rings_contiguous) {
                    _rebuild_ring2_subtree_leaf_count(offset);
                }
                const pruned = _band_credit(next_depth, lo_rem - offset, hi_rem - offset);
                _checked += pruned;
                _precheck_reject += pruned;
                _dbg_restr_prune_count += pruned;
                _maybe_progress();
                continue;
            }

            if (is) tracker.add(is, iname);
            partial[slot] = item;
            _place_item(item);
            _sp_place_free_item(item.statMap, slot_idx);

            if (is_ring1) {
                _ring1_placed_offset = offset;
                _rebuild_ring2_subtree_leaf_count(offset);
            }

            enumerate(next_depth, lo_rem - offset, hi_rem - offset);

            _sp_unplace_free_item(item.statMap, slot_idx);
            _unplace_item(item);
            if (is) tracker.remove(is, iname);
        }
        partial[slot] = locked[slot] ?? none_items_wrapped[_cfg.none_idx_map[slot]];
    }

    // ── Unified level enumeration over all free slots (armor + rings) ────────

    _sp_reset();
    if (N_free === 0) {
        _evaluate_leaf();
    } else {
        // Geometric level bands: strict per-level ordering for the first,
        // narrow bands (the quality-critical head of the search) and
        // doubling widths for the tail, cutting prefix re-walks from
        // O(L_max) to O(log L_max) per prefix.
        let band_lo = 0;
        let band_width = 1;
        while (band_lo <= L_max && !_cancelled) {
            const band_hi = Math.min(L_max, band_lo + band_width - 1);
            _current_L = band_hi;
            enumerate(0, band_lo, band_hi);
            band_lo = band_hi + 1;
            band_width *= 2;
        }
    }

    if (_dbg) {
        const pool_sizes = Object.fromEntries(
            Object.entries(pools).map(([k, v]) => [k, v.length]));
        console.log('[w0] enum setup | free:', free_slots,
            '| pools:', pool_sizes,
            '| L_max:', L_max,
            '| ring_pool:', ring_pool?.length,
            '| partition:', JSON.stringify(partition));
        console.log('[w0] leaf breakdown | checked:', _checked,
            '| precheck_reject:', _dbg_precheck_reject,
            '| ehp_reject:', _dbg_ehp_reject,
            '| sp_leaf_reject:', _dbg_sp_leaf_reject,
            '| sp_reject:', _dbg_sp_reject,
            '| sp_pruned:', _dbg_sp_prune_count,
            '| restr_pruned:', _dbg_restr_prune_count,
            '| feasible:', _feasible,
            '| threshold_reject:', _dbg_threshold_reject,
            '| mana_reject:', _dbg_mana_reject,
            '| mana_rescued:', _dbg_mana_rescued,
            '| ceiling_skip:', _dbg_ceiling_skip,
            '| hp_reject:', _dbg_hp_reject,
            '| scored:', _dbg_scored);
        if (_feasible > 0) {
            console.log('[w0] perf | avg feasible leaf:',
                (_dbg_leaf_time / _feasible).toFixed(2), 'ms',
                '| total feasible time:', _dbg_leaf_time.toFixed(0), 'ms');
        }
        if (_top5.length > 0) {
            console.log('[w0] best score:', _top5[0].score.toFixed(1),
                '| items:', _top5[0].item_names.filter(n => n).join(', '));
        }
    }

}

// ── Message handler ─────────────────────────────────────────────────────────

self.onmessage = function (e) {
    const msg = e.data;
    if (msg.type === 'init') {
        // Heavy one-time initialization: store all shared data
        sets = new Map(msg.sets_data);
        _cfg = msg;
        _set_incr_cache_enabled(!msg.benchmark_legacy_incremental);
        _set_incr_parallel_layout(!msg.benchmark_nested_incremental);
        _cancelled = false;
        // Precompute generic slider names for boost token injection
        const _sn = extract_slider_names(_cfg.health_config);
        _cfg.bp_slider_name = _sn.bp_slider_name;
        _cfg.state_slider_names = _sn.state_slider_names;
        try {
            _build_constraint_prechecks();
            _build_sp_constraints();
            _vec_setup();
            _atree_scaling_setup();
        } catch (err) {
            console.error('[w] prechecks crashed:', err.message, err.stack);
            postMessage({ type: 'done', worker_id: msg.worker_id, checked: 0, feasible: 0, met_req: 0, top5: [] });
            return;
        }
        if (SOLVER_DEBUG_WORKER && msg.worker_id === 0) {
            console.log('[w0] init | scoring:', msg.scoring_target,
                '| combo_rows:', msg.parsed_combo?.length,
                '| combo_time:', msg.combo_time,
                '| allow_downtime:', msg.allow_downtime,
                '| sp_budget:', msg.sp_budget,
                '| prechecks:', _constraint_prechecks.length,
                '| ehp_precheck:', !!_ehp_precheck, '| ehp_no_agi_precheck:', !!_ehp_no_agi_precheck, '| total_hp_precheck:', !!_total_hp_precheck,
                '| thresholds:', msg.restrictions?.stat_thresholds?.length ?? 0,
                '| sp_floors:', _sp_floors ? Array.from(_sp_floors) : null,
                '| sp_caps:', _sp_caps ? Array.from(_sp_caps) : null);
        }

        // Run immediately if a partition is requested
        if (msg.partition) {
            _checked = 0;
            _precheck_pass = 0;
            _precheck_reject = 0;
            _feasible = 0;
            _met_req = 0;
            _top5 = [];
            _top5_version = 0;
            _last_sent_top5_version = 0;
            _checked_at_last_top5_change = 0;
            _current_L = 0;
            _progress_checkpoint.next = PROGRESS_INTERVAL;
            _reset_trace();
            try {
                _run_level_enum();
            } catch (err) {
                console.error('[w] enum crashed:', err.message, err.stack);
                postMessage({ type: 'worker_error', worker_id: msg.worker_id, message: err.message, stack: err.stack });
            }
            postMessage({
                type: 'done',
                worker_id: msg.worker_id,
                checked: _checked,
                precheck_pass: _precheck_pass,
                precheck_reject: _precheck_reject,
                feasible: _feasible,
                met_req: _met_req,
                top5: _top5,
                trace: _trace_snapshot(),
            });
        }
    } else if (msg.type === 'run') {
        // Lightweight partition assignment — reuse stored _cfg data
        _cfg.partition = msg.partition;
        _cfg.worker_id = msg.worker_id;
        _checked = 0;
        _precheck_pass = 0;
        _precheck_reject = 0;
        _feasible = 0;
        _met_req = 0;
        _top5 = [];
        _top5_version = 0;
        _last_sent_top5_version = 0;
        _checked_at_last_top5_change = 0;
        _current_L = 0;
        _cancelled = false;
        _progress_checkpoint.next = PROGRESS_INTERVAL;
        _reset_trace();

        try {
            _run_level_enum();
        } catch (err) {
            console.error('[w] enum crashed:', err.message, err.stack);
            postMessage({ type: 'worker_error', worker_id: msg.worker_id, message: err.message, stack: err.stack });
        }
        postMessage({
            type: 'done',
            worker_id: msg.worker_id,
            checked: _checked,
            precheck_pass: _precheck_pass,
            precheck_reject: _precheck_reject,
            feasible: _feasible,
            met_req: _met_req,
            top5: _top5,
            trace: _trace_snapshot(),
        });
    } else if (msg.type === 'cancel') {
        _cancelled = true;
    }
};
