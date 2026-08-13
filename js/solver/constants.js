/**
 * Solver-specific constants.
 * Shared slot/field definitions are in ../shared_constants.js (loaded first).
 */

// ── Lock toggle SVG icons ───────────────────────────────────────────────────

const LOCK_SVG = '<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M11 6V4a3 3 0 0 0-6 0v2H4a1 1 0 0 0-1 1v7a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V7a1 1 0 0 0-1-1h-1zM7 4a1 1 0 0 1 2 0v2H7V4z"/></svg>';
const UNLOCK_SVG = '<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M11 4a3 3 0 0 0-6 0H7a1 1 0 0 1 2 0v2H4a1 1 0 0 0-1 1v7a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V7a1 1 0 0 0-1-1h-1V4z"/></svg>';

// ── Timing button SVG icon ─────────────────────────────────────────────────
const CLOCK_SVG = '<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1zm0 12.5A5.5 5.5 0 1 1 8 2.5a5.5 5.5 0 0 1 0 11zM8.5 4H7v5l3.5 2.1.75-1.23L8.5 8.25V4z"/></svg>';

// ── Encoding limits (derived from binary URL format bit widths) ──────────────
// These caps prevent silent truncation when values are encoded into the URL hash.
// Mirror of the bit widths in encodeSolverParams() / decodeSolverParams()
// in build_encode.js / build_decode.js — keep them in sync.

/** 26-bit signed 2's complement: ±33,554,431 (size class 3, rarely used) */
const RESTR_VALUE_MAX = 33554431;
const RESTR_VALUE_MIN = -33554431;

/** 7-bit unsigned: combo row quantity */
const COMBO_QTY_MAX = 127;

/** 10-bit unsigned: boost slider value (v4 encoding) */
const BOOST_SLIDER_MAX = 1023;

/** Max rows for restrictions / blacklist (4-bit count = 0-15) */
const MAX_RESTRICTION_ROWS = 15;
/** Max combo rows (8-bit count = 0-255) */
const MAX_COMBO_ROWS = 255;
const MAX_BLACKLIST_ROWS = 15;

// ── Spell recast penalty ──────────────────────────────────────────────────────

/** Mana penalty per consecutive recast of the same spell (+5 per recast). */
const RECAST_MANA_PENALTY = 5;

/**
 * Internal spell ID for the "Mana Reset" pseudo-spell.
 * Represents a timeout / pause that resets the recast counter.
 */
const MANA_RESET_SPELL_ID = -2000;

/** Reserved 7-bit node ID for Mana Reset in binary URL encoding. */
const MANA_RESET_NODE_ID = 126;

/**
 * Internal spell ID for the "Cancel Bak'al's Grasp" pseudo-spell.
 */
const CANCEL_BAKALS_SPELL_ID = -2001;

/** Reserved 7-bit node ID for Cancel Bak'al's Grasp in binary URL encoding. */
const CANCEL_BAKALS_NODE_ID = 119;

/**
 * Internal spell ID for the "Melee Time" attack option.
 * Like regular Melee (base_spell 0) but qty represents seconds instead of hits.
 * Effective hit count is computed from attack speed.
 */
const MELEE_TIME_SPELL_ID = -2002;

/** Reserved 7-bit node ID for Melee Time in binary URL encoding. */
const MELEE_TIME_NODE_ID = 118;

/**
 * Internal spell ID for the "Add Flat Mana" pseudo-spell.
 * Injects qty mana into the simulation at that point in the combo.
 */
const ADD_FLAT_MANA_SPELL_ID = -2003;

/** Reserved 7-bit node ID for Add Flat Mana in binary URL encoding. */
const ADD_FLAT_MANA_NODE_ID = 117;

// ── Loop pseudo-spells ──────────────────────────────────────────────────────

/** Internal spell ID for the Loop Start pseudo-row. */
const LOOP_START_SPELL_ID = -2004;
/** Reserved 7-bit node ID for Loop Start in binary URL encoding. */
const LOOP_START_NODE_ID = 116;

/** Internal spell ID for the Loop End pseudo-row. */
const LOOP_END_SPELL_ID = -2005;
/** Reserved 7-bit node ID for Loop End in binary URL encoding. */
const LOOP_END_NODE_ID = 115;

/** Loop condition type constants. */
const LOOP_COND_COUNT = 0;
const LOOP_COND_UNTIL_OOM = 1;

/** Safety cap for condition-driven loops. */
const LOOP_SAFETY_CAP = 255;

/**
 * Normalize a loop condition.
 * @param {number|string} type_or_count - Integer for fixed count, or condition type constant
 * @param {Object} [params] - Additional params for non-count conditions
 * @returns {{ type: number, value?: number }}
 */
function make_loop_condition(type_or_count, params) {
    if (typeof type_or_count === 'number' && (type_or_count === Math.floor(type_or_count)) && type_or_count > 0) {
        return { type: LOOP_COND_COUNT, value: type_or_count };
    }
    return { type: type_or_count, ...(params || {}) };
}

// ── Generic buff state cancel mappings ───────────────────────────────────────
// Static spell ID → state name mapping for cancel pseudo-spells.
// Do NOT auto-generate IDs — they appear in URL-encoded combos.
const STATE_CANCEL_IDS = new Map([['Corrupted', CANCEL_BAKALS_SPELL_ID]]);
const STATE_CANCEL_NODE_IDS = new Map([['Corrupted', CANCEL_BAKALS_NODE_ID]]);

/** Look up the cancel pseudo-spell ID for a buff state name. */
function get_cancel_spell_id(state_name) {
    return STATE_CANCEL_IDS.get(state_name) ?? null;
}

// ── Solver-specific constants ────────────────────────────────────────────────

/**
 * Roll percentage (0-100) controlling which value in the rolled ID range is
 * used when evaluating items during the solve.
 *   100 → use maxRolls  (matches WynnBuilder default)
 *    85 → minRolls + 0.85 * (maxRolls - minRolls) (solver default)
 *    50 → (minRolls + maxRolls) / 2
 *     0 → use minRolls
 */
const ROLL_DEFAULT = 85;

/** Per-group default roll percentages. */
const ROLL_GROUP_DEFAULTS = { damage: 85, mana: 100, healing: 85, misc: 85 };

/** Ordered list of roll group keys (used for encoding/iteration). */
const ROLL_GROUP_ORDER = ['damage', 'mana', 'healing', 'misc'];

/** Display labels for each roll group. */
const ROLL_GROUP_LABELS = { damage: 'Damage', mana: 'Mana', healing: 'Healing', misc: 'Misc' };

/**
 * Mapping from stat key → roll group name.
 * Stats not listed here fall into 'misc'.
 */
const ROLL_STAT_GROUP = (() => {
    const m = {};
    // Damage stats
    for (const k of [
        'sdPct', 'mdPct', 'sdRaw', 'mdRaw', 'damPct', 'damRaw', 'critDamPct', 'poison',
        'nDamPct', 'nDamRaw', 'rDamPct', 'rDamRaw',
    ]) m[k] = 'damage';
    for (const e of ['e', 't', 'w', 'f', 'a']) {
        m[e + 'DamPct'] = 'damage'; m[e + 'DamRaw'] = 'damage';
        m[e + 'SdPct'] = 'damage'; m[e + 'SdRaw'] = 'damage';
        m[e + 'MdPct'] = 'damage'; m[e + 'MdRaw'] = 'damage';
        m[e + 'DamAddMin'] = 'damage'; m[e + 'DamAddMax'] = 'damage';
    }
    for (const e of ['n', 'r', '']) {
        m[e + 'DamAddMin'] = 'damage'; m[e + 'DamAddMax'] = 'damage';
    }
    for (const e of ['n', 'r']) {
        m[e + 'SdPct'] = 'damage'; m[e + 'SdRaw'] = 'damage';
        m[e + 'MdPct'] = 'damage'; m[e + 'MdRaw'] = 'damage';
    }
    // Mana stats
    for (const k of [
        'mr', 'ms', 'maxMana',
        'spPct1', 'spPct2', 'spPct3', 'spPct4',
        'spRaw1', 'spRaw2', 'spRaw3', 'spRaw4',
        'spPct1Final', 'spPct2Final', 'spPct3Final', 'spPct4Final',
    ]) m[k] = 'mana';
    // Healing stats
    for (const k of ['hprPct', 'hprRaw', 'healPct', 'ls', 'hpBonus']) m[k] = 'healing';
    return m;
})();

/** Look up the roll group for a given stat key. */
function _get_roll_group(statKey) {
    return ROLL_STAT_GROUP[statKey] || 'misc';
}

/**
 * Per-group roll mode. Each key is a group name, value is 0-100.
 * Replaces the old scalar `current_roll_mode`.
 */
let current_roll_mode = { ...ROLL_GROUP_DEFAULTS };

/** Returns true if all roll groups match their defaults. */
function isRollDefault() {
    for (const g of ROLL_GROUP_ORDER) {
        if (current_roll_mode[g] !== ROLL_GROUP_DEFAULTS[g]) return false;
    }
    return true;
}

/** Returns true if all groups share the same value. */
function _isRollUniform() {
    const v = current_roll_mode[ROLL_GROUP_ORDER[0]];
    return ROLL_GROUP_ORDER.every(g => current_roll_mode[g] === v);
}

/**
 * Returns display text for the roll mode input.
 * "Default" when at defaults, "N%" when uniform non-default, "Custom" otherwise.
 */
function rollDisplayText() {
    if (isRollDefault()) return 'Default';
    if (_isRollUniform()) return current_roll_mode.damage + '%';
    return 'Custom';
}

/** Returns true if ALL groups are >= 100 (i.e. no rolling needed). */
function _allRollsMax() {
    return ROLL_GROUP_ORDER.every(g => current_roll_mode[g] >= 100);
}

/**
 * Guild tome choices, indexed by the value encoded in solver URLs (v11+).
 *
 * Every guild tome in the game is a skill-point tome: five grant +4 to one
 * attribute, and Assimilator's grants +1 to all five. There is no guild tome
 * that does anything else, so the only decision is which one — hence an
 * explicit list rather than an abstract "+4 SP" mode.
 *
 * This replaces the pre-v11 `gtome` encoding, where value 1 meant "Standard
 * (+4 SP)" and was implemented by inflating the assignable SP budget by 4. That
 * let the solver split the bonus across attributes (e.g. [102,102,0,0,0]),
 * producing builds no real tome can support. `sp` here is the exact
 * per-attribute contribution, applied as a real item statMap instead.
 *
 * WARNING: order is LOAD-BEARING for URL encoding — append only.
 */
const GUILD_TOMES = [
    { key: 'none',  label: 'Off (200 SP)',         item: null,                            sp: [0, 0, 0, 0, 0] },
    { key: 'str',   label: 'Strength (+4 Str)',    item: "Psychopath's Tome of Allegiance",   sp: [4, 0, 0, 0, 0] },
    { key: 'dex',   label: 'Dexterity (+4 Dex)',   item: "Sadist's Tome of Allegiance",       sp: [0, 4, 0, 0, 0] },
    { key: 'int',   label: 'Intelligence (+4 Int)', item: "Warlock's Tome of Allegiance",     sp: [0, 0, 4, 0, 0] },
    { key: 'def',   label: 'Defense (+4 Def)',     item: "Destroyer's Tome of Allegiance",    sp: [0, 0, 0, 4, 0] },
    { key: 'agi',   label: 'Agility (+4 Agi)',     item: "Sycophant's Tome of Allegiance",    sp: [0, 0, 0, 0, 4] },
    { key: 'rainbow', label: 'Rainbow (+1 each)',  item: "Assimilator's Tome of Allegiance",  sp: [1, 1, 1, 1, 1] },
];

/** Encoded value for the Rainbow tome — the one pre-v11 value that survives. */
const GUILD_TOME_RAINBOW = 6;

/**
 * Total skill points a guild tome choice grants. Use this instead of testing
 * the encoded value against a literal: the values changed meaning in v11 (1 was
 * "Standard", now Strength; 2 was "Rainbow", now Dexterity), so any surviving
 * `=== 1` / `=== 2` comparison is a latent bug.
 */
function guild_tome_sp_total(idx) {
    const g = GUILD_TOMES[idx] ?? GUILD_TOMES[0];
    return g.sp.reduce((a, b) => a + b, 0);
}

/**
 * A synthetic statMap for a guild tome choice, shaped like an equipped tome so
 * calculate_skillpoints counts it as bonus skillpoints. Returns null for "Off".
 */
function guild_tome_statmap(idx) {
    const g = GUILD_TOMES[idx] ?? GUILD_TOMES[0];
    if (!g.sp.some(v => v !== 0)) return null;
    const sm = new Map();
    sm.set('skillpoints', g.sp.slice());
    sm.set('reqs', [0, 0, 0, 0, 0]);
    return sm;
}

/**
 * Tome optimisation modes (encoded value in solver URLs, v11+).
 *
 * OFF   — tomes are whatever the user selected; the solver never varies them.
 * GUILD — the solver picks the best guild tome from the enabled set.
 * ALL   — the solver also picks weapon and armour tomes.
 */
const TOME_OPT_OFF = 0;
const TOME_OPT_GUILD = 1;
const TOME_OPT_ALL = 2;

/**
 * Build the admissible optimistic stat bound over a set of tome statMaps: the
 * per-key maximum, which is an upper bound on any single legal choice.
 *
 * This is the "tighten the bound per tome type" rule from
 * solver/TOME_AND_LEVEL_PLAN.md — call it once per tome type rather than once
 * across all types, because a global maximum is reachable by no real
 * combination and would prune far less.
 *
 * Soundness: no combat tome carries a skill point requirement (verified across
 * all 101), so a tome can only ever make a build more feasible. A gearset that
 * fails a threshold against these per-key maxima therefore fails against every
 * real tome choice, and pruning it discards nothing reachable.
 *
 * @param {Map[]} statmaps  - candidate tome statMaps
 * @param {number} count    - how many of this tome type are equipped at once
 * @returns {Map} per-key maxima, scaled by `count`
 */
function tome_optimistic_bound(statmaps, count = 1) {
    const out = new Map();
    const bump = (k, v) => {
        const cur = out.get(k);
        if (cur === undefined || v > cur) out.set(k, v);
    };
    for (const sm of statmaps) {
        if (!sm) continue;
        for (const [k, v] of sm) {
            if (typeof v === 'number') bump(k, v);
            else if (k === 'skillpoints' && Array.isArray(v)) {
                const cur = out.get(k) ?? [0, 0, 0, 0, 0];
                out.set(k, cur.map((c, i) => Math.max(c, v[i] ?? 0)));
            }
        }
        // Rolled IDs live in maxRolls after _apply_roll_mode_to_item; missing
        // them here would make the bound too tight and prune real builds.
        const rolled = sm.get('maxRolls');
        if (rolled) for (const [k, v] of rolled) if (typeof v === 'number') bump(k, v);
    }
    if (count !== 1) {
        for (const [k, v] of out) {
            out.set(k, Array.isArray(v) ? v.map(x => x * count) : v * count);
        }
    }
    return out;
}

/**
 * Read a tome's effective value for `key`.
 *
 * Rolled IDs do NOT live as top-level statMap keys. `_apply_roll_mode_to_item`
 * rewrites the `maxRolls` map in place to the rolled value, and leaf assembly
 * reads from there. Anything inspecting tome stats must do the same or it
 * silently sees zero for every rolled stat — which is every stat that matters
 * on weapon and armour tomes.
 */
function tome_stat(sm, key) {
    if (!sm) return 0;
    const direct = sm.get(key);
    if (typeof direct === 'number') return direct;
    const rolled = sm.get('maxRolls');
    if (rolled) {
        const v = rolled.get(key);
        if (typeof v === 'number') return v;
    }
    return 0;
}

/**
 * Drop tomes that another tome in the same pool matches or beats on every
 * solver-relevant stat. Tome lines are heavily tiered (Combat Mastery I/II/III),
 * so this collapses the pools hard before any combination is enumerated.
 *
 * Only `keys` are compared. As with item dominance, a stat outside that set is
 * invisible here — so `keys` must cover everything the current search can score
 * or threshold on, or a genuinely better tome can be discarded.
 */
function tome_prune_dominated(statmaps, keys, signs = null) {
    // signs[i] = -1 flips key i so "more is better" holds universally — used
    // for stats under an 'le' restriction, where less is better.
    const vecs = statmaps.map(sm => keys.map((k, i) => tome_stat(sm, k) * (signs ? signs[i] : 1)));
    const keep = [];
    for (let i = 0; i < statmaps.length; i++) {
        let dominated = false;
        for (let j = 0; j < statmaps.length && !dominated; j++) {
            if (i === j) continue;
            let ge_all = true, gt_any = false;
            for (let k = 0; k < keys.length; k++) {
                if (vecs[j][k] < vecs[i][k]) { ge_all = false; break; }
                if (vecs[j][k] > vecs[i][k]) gt_any = true;
            }
            // Ties are broken by index so two identical tomes don't delete each
            // other, which would leave the pool empty.
            if (ge_all && (gt_any || j < i)) dominated = true;
        }
        if (!dominated) keep.push(statmaps[i]);
    }
    return keep;
}

/**
 * Enumerate the distinct stat bundles obtainable by filling `count` slots from
 * `statmaps`, then Pareto-prune them.
 *
 * Duplicates are allowed (the same tome may occupy more than one slot), so
 * these are multiset combinations: C(n+count-1, count). Prune the pool with
 * tome_prune_dominated first — this is exponential in `count` and enumerating
 * raw pools (35 armour tomes over 4 slots is ~74k quads) is not viable.
 *
 * The result does not depend on the gear, so compute it ONCE per search and
 * reuse it at every leaf. That is the difference between a short front to
 * iterate and re-enumerating the tome space inside the hot loop.
 *
 * @returns {{vec: number[], picks: Map[]}[]} bundles, each a summed stat vector
 *          over `keys` plus the tomes that produced it
 */
function tome_bundles(statmaps, count, keys, signs = null) {
    if (count <= 0 || statmaps.length === 0) return [{ vec: keys.map(() => 0), picks: [] }];
    const out = [];
    const cur = [];
    const vecs = statmaps.map(sm => keys.map((k, i) => tome_stat(sm, k) * (signs ? signs[i] : 1)));
    (function pick(start, depth, acc) {
        if (depth === count) {
            out.push({ vec: acc.slice(), picks: cur.slice() });
            return;
        }
        // start, not start+1: duplicates are legal.
        for (let i = start; i < statmaps.length; i++) {
            cur.push(statmaps[i]);
            pick(i, depth + 1, acc.map((v, k) => v + vecs[i][k]));
            cur.pop();
        }
    })(0, 0, keys.map(() => 0));
    return pareto_prune_bundles(out);
}

/** Keep only bundles no other bundle matches or beats on every stat. */
function pareto_prune_bundles(bundles) {
    const keep = [];
    for (let i = 0; i < bundles.length; i++) {
        let dominated = false;
        for (let j = 0; j < bundles.length && !dominated; j++) {
            if (i === j) continue;
            const a = bundles[i].vec, b = bundles[j].vec;
            let ge_all = true, gt_any = false;
            for (let k = 0; k < a.length; k++) {
                if (b[k] < a[k]) { ge_all = false; break; }
                if (b[k] > a[k]) gt_any = true;
            }
            if (ge_all && (gt_any || j < i)) dominated = true;
        }
        if (!dominated) keep.push(bundles[i]);
    }
    return keep;
}

/**
 * Slots that can carry a per-slot item-level override, in encoding order.
 *
 * WARNING: This order is LOAD-BEARING for URL encoding — solver URLs encode the
 * override set as a bitmask over these indices. Never reorder or remove
 * entries; only append, and only behind a solver URL version bump.
 *
 * `ring` is deliberately one entry covering BOTH ring slots. The two rings draw
 * from a single shared, priority-ordered pool and the enumerator canonicalises
 * ring pairs so (A,B) and (B,A) are not both visited; per-ring ranges would
 * break that symmetry. See solver/TOME_AND_LEVEL_PLAN.md, "Ring caveat".
 */
const LVL_OVERRIDE_SLOTS = [
    'helmet', 'chestplate', 'leggings', 'boots', 'ring', 'bracelet', 'necklace',
];

/**
 * Stats available for use in restriction threshold rows.
 * Each entry: { key: <statMap key>, label: <display name> }
 * Ordered by category for readability in the autocomplete list.
 *
 * WARNING: The order of entries in this array is LOAD-BEARING for URL encoding.
 * Solver URLs encode restriction stats by their index in this array.
 * NEVER reorder or remove existing entries — only append new ones at the end.
 * Reordering requires a solver URL version bump.
 */
const RESTRICTION_STATS = [
    // ── Health / Sustain ────────────────────────────────────────────────
    { key: 'ehp', label: 'Effective HP' },          // derived — computed during solver eval
    { key: 'ehp_no_agi', label: 'EHP (No Agi)' }, // derived — EHP without agility dodge
    { key: 'ehpr', label: 'Effective HPR' },         // derived — computed during solver eval
    { key: 'hpr', label: 'HP Regen' },              // derived — hprRaw + hprPct combined
    { key: 'total_hp', label: 'Total HP' },              // derived — hp + hpBonus
    { key: 'hprRaw', label: 'Health Regen Raw' },
    { key: 'hprPct', label: 'Health Regen %' },
    { key: 'healPct', label: 'Heal Effectiveness %' },
    { key: 'ls', label: 'Life Steal' },
    // ── Mana ────────────────────────────────────────────────────────────
    { key: 'mr', label: 'Mana Regen' },
    { key: 'ms', label: 'Mana Steal' },
    // ── Skill Points ────────────────────────────────────────────────────
    { key: 'str', label: 'Strength' },
    { key: 'dex', label: 'Dexterity' },
    { key: 'int', label: 'Intelligence' },
    { key: 'def', label: 'Defense' },
    { key: 'agi', label: 'Agility' },
    // ── Damage (generic) ────────────────────────────────────────────────
    { key: 'sdRaw', label: 'Spell Damage Raw' },
    { key: 'sdPct', label: 'Spell Damage %' },
    { key: 'mdRaw', label: 'Melee Damage Raw' },
    { key: 'mdPct', label: 'Melee Damage %' },
    { key: 'damRaw', label: 'Damage Raw' },
    { key: 'damPct', label: 'Damage %' },
    { key: 'critDamPct', label: 'Crit Damage %' },
    // ── Neutral Damage ─────────────────────────────────────────────────
    { key: 'nDamPct', label: 'Neutral Damage %' },
    { key: 'nDamRaw', label: 'Neutral Damage Raw' },
    { key: 'nSdPct', label: 'Neutral Spell Damage %' },
    { key: 'nSdRaw', label: 'Neutral Spell Damage Raw' },
    { key: 'nMdPct', label: 'Neutral Melee Damage %' },
    { key: 'nMdRaw', label: 'Neutral Melee Damage Raw' },
    // ── Elemental Damage % ──────────────────────────────────────────────
    { key: 'eDamPct', label: 'Earth Damage %' },
    { key: 'tDamPct', label: 'Thunder Damage %' },
    { key: 'wDamPct', label: 'Water Damage %' },
    { key: 'fDamPct', label: 'Fire Damage %' },
    { key: 'aDamPct', label: 'Air Damage %' },
    // ── Elemental Damage Raw ────────────────────────────────────────────
    { key: 'eDamRaw', label: 'Earth Damage Raw' },
    { key: 'tDamRaw', label: 'Thunder Damage Raw' },
    { key: 'wDamRaw', label: 'Water Damage Raw' },
    { key: 'fDamRaw', label: 'Fire Damage Raw' },
    { key: 'aDamRaw', label: 'Air Damage Raw' },
    // ── Elemental Spell Damage ──────────────────────────────────────────
    { key: 'eSdPct', label: 'Earth Spell Damage %' },
    { key: 'tSdPct', label: 'Thunder Spell Damage %' },
    { key: 'wSdPct', label: 'Water Spell Damage %' },
    { key: 'fSdPct', label: 'Fire Spell Damage %' },
    { key: 'aSdPct', label: 'Air Spell Damage %' },
    { key: 'eSdRaw', label: 'Earth Spell Damage Raw' },
    { key: 'tSdRaw', label: 'Thunder Spell Damage Raw' },
    { key: 'wSdRaw', label: 'Water Spell Damage Raw' },
    { key: 'fSdRaw', label: 'Fire Spell Damage Raw' },
    { key: 'aSdRaw', label: 'Air Spell Damage Raw' },
    // ── Elemental Melee Damage ──────────────────────────────────────────
    { key: 'eMdPct', label: 'Earth Melee Damage %' },
    { key: 'tMdPct', label: 'Thunder Melee Damage %' },
    { key: 'wMdPct', label: 'Water Melee Damage %' },
    { key: 'fMdPct', label: 'Fire Melee Damage %' },
    { key: 'aMdPct', label: 'Air Melee Damage %' },
    { key: 'eMdRaw', label: 'Earth Melee Damage Raw' },
    { key: 'tMdRaw', label: 'Thunder Melee Damage Raw' },
    { key: 'wMdRaw', label: 'Water Melee Damage Raw' },
    { key: 'fMdRaw', label: 'Fire Melee Damage Raw' },
    { key: 'aMdRaw', label: 'Air Melee Damage Raw' },
    // ── Rainbow Damage ──────────────────────────────────────────────────
    { key: 'rDamPct', label: 'Elemental Damage %' },
    { key: 'rDamRaw', label: 'Elemental Damage Raw' },
    { key: 'rSdRaw', label: 'Elemental Spell Damage Raw' },
    { key: 'rSdPct', label: 'Elemental Spell Damage %' },
    { key: 'rMdPct', label: 'Elemental Melee Damage %' },
    { key: 'rMdRaw', label: 'Elemental Melee Damage Raw' },
    // ── Spell Costs ─────────────────────────────────────────────────────
    { key: 'spRaw1', label: '1st Spell Cost Raw' },
    { key: 'spRaw2', label: '2nd Spell Cost Raw' },
    { key: 'spRaw3', label: '3rd Spell Cost Raw' },
    { key: 'spRaw4', label: '4th Spell Cost Raw' },
    { key: 'spPct1', label: '1st Spell Cost %' },
    { key: 'spPct2', label: '2nd Spell Cost %' },
    { key: 'spPct3', label: '3rd Spell Cost %' },
    { key: 'spPct4', label: '4th Spell Cost %' },
    // ── Movement ────────────────────────────────────────────────────────
    { key: 'spd', label: 'Walk Speed Bonus' },
    { key: 'atkTier', label: 'Attack Speed Bonus' },
    { key: 'mainAttackRange', label: 'Melee Range %' },
    // ── Other Combat ────────────────────────────────────────────────────
    { key: 'poison', label: 'Poison' },
    { key: 'thorns', label: 'Thorns' },
    { key: 'expd', label: 'Exploding' },
    { key: 'ref', label: 'Reflection' },
    { key: 'spRegen', label: 'Soul Point Regen' },
    { key: 'eSteal', label: 'Stealing' },
    { key: 'sprint', label: 'Sprint Bonus' },
    { key: 'sprintReg', label: 'Sprint Regen Bonus' },
    { key: 'jh', label: 'Jump Height' },
    { key: 'kb', label: 'Knockback' },
    { key: 'weakenEnemy', label: 'Weaken Enemy' },
    { key: 'slowEnemy', label: 'Slow Enemy' },
    // ── Loot / XP ───────────────────────────────────────────────────────
    { key: 'lb', label: 'Loot Bonus' },
    { key: 'lq', label: 'Loot Quality' },
    { key: 'xpb', label: 'XP Bonus' },
    { key: 'gXp', label: 'Gathering XP Bonus' },
    { key: 'gSpd', label: 'Gathering Speed Bonus' },
    // ── Final Spell Costs (computed — depends on int, spRaw, spPct, atree) ──
    { key: 'finalSpellCost1', label: '1st Spell Cost (Final)' },
    { key: 'finalSpellCost2', label: '2nd Spell Cost (Final)' },
    { key: 'finalSpellCost3', label: '3rd Spell Cost (Final)' },
    { key: 'finalSpellCost4', label: '4th Spell Cost (Final)' },
    // ── Max Mana ───────────────────────────────────────────────────────
    { key: 'maxMana', label: 'Max Mana' },
    // total_mana = 100 (base) + maxMana + intelligence-derived mana
    { key: 'total_mana', label: 'Total Mana' },
];

/**
 * Returns the effective rolled value for a stat given the current roll percentage.
 * @param {number} minVal
 * @param {number} maxVal
 * @param {string} [statKey] - stat identifier to look up the roll group (optional; defaults to 'misc')
 * @returns {number}
 */
function getRolledValue(minVal, maxVal, statKey) {
    const pct = current_roll_mode[_get_roll_group(statKey)] ?? current_roll_mode.misc ?? 100;
    if (pct >= 100) return maxVal;
    if (pct <= 0) return minVal;
    return Math.round(minVal + (pct / 100) * (maxVal - minVal));
}
