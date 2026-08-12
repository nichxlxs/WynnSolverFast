// ══════════════════════════════════════════════════════════════════════════════
// SOLVER PURE — Combo boost application & relevance filtering
// Shared between the solver main thread (<script> tag) and the Web Worker
// (importScripts). NO DOM access, NO window/document/navigator references.
//
// pure/ dependencies: pure/spell.js (spell_has_damage, spell_has_heal)
// External dependencies (must be loaded before this file):
//   - utils.js:       round_near
//   - build_utils.js: merge_stat
// ══════════════════════════════════════════════════════════════════════════════

/**
 * Find all registry entries that apply for a given boost token name.
 * Rules:
 *  - Exact name match (case-insensitive, stripping leading "Activate ").
 *  - Alias match.
 *  - If is_pct=true, ALSO find sliders that CONTAIN the name (for "Enkindled 100%" → "Enkindled Percent").
 *
 * Returns [{entry, effective_value}] where effective_value is 1 for toggles
 * and token.value for sliders.
 */
function find_all_matching_boosts(token_name, token_value, is_pct, registry) {
    const name_lower = token_name.toLowerCase().trim();
    const results = [];

    for (const entry of registry) {
        const ename     = entry.name.toLowerCase();
        const aliases_l = (entry.aliases ?? []).map(a => a.toLowerCase());

        const exact_match = ename === name_lower
                         || ename === 'activate ' + name_lower
                         || aliases_l.includes(name_lower);

        if (entry.type === 'toggle') {
            if (exact_match) results.push({ entry, effective_value: 1 });
        } else {
            // slider
            if (exact_match) {
                results.push({ entry, effective_value: token_value });
            } else if (is_pct && (ename.includes(name_lower) || ename.startsWith(name_lower))) {
                // "Enkindled 100%" also activates "Enkindled Percent" slider.
                results.push({ entry, effective_value: token_value });
            }
        }
    }
    return results;
}

/**
 * Apply per-row boost tokens to a clone of base_stats.
 * Returns { stats: modified_stats, prop_overrides: Map<'abilId.prop', value> }.
 */
function apply_combo_row_boosts(base_stats, boost_tokens, registry, scratch) {
    // Fast path: a row with no boost tokens cannot modify any stat, so hand
    // back base_stats itself instead of copying ~170 entries plus the nested
    // mult maps. Callers treat the returned stats as read-only. Gated on the
    // nested maps already existing because downstream readers iterate
    // damMult/defMult/healMult unguarded (every finalized statMap has them).
    if ((!boost_tokens || boost_tokens.length === 0)
        && base_stats.has('damMult') && base_stats.has('defMult')) {
        const prop_overrides = scratch?.prop_overrides ?? new Map();
        if (scratch?.prop_overrides) prop_overrides.clear();
        return { stats: base_stats, prop_overrides };
    }

    let stats, damMult, defMult;
    if (scratch) {
        // Reuse pre-allocated Maps (zero allocation for the outer + nested Maps)
        stats = scratch.stats;   stats.clear();
        for (const [k, v] of base_stats) stats.set(k, v);
        damMult = scratch.damMult; damMult.clear();
        const dm = base_stats.get('damMult');
        if (dm) for (const [k, v] of dm) damMult.set(k, v);
        defMult = scratch.defMult; defMult.clear();
        const dfm = base_stats.get('defMult');
        if (dfm) for (const [k, v] of dfm) defMult.set(k, v);
    } else {
        // Allocating path (main thread / non-worker callers)
        stats   = new Map(base_stats);
        damMult = new Map(base_stats.get('damMult') ?? []);
        defMult = new Map(base_stats.get('defMult') ?? []);
    }
    stats.set('damMult', damMult);
    stats.set('defMult', defMult);

    const prop_overrides = scratch?.prop_overrides ?? new Map();
    if (scratch?.prop_overrides) prop_overrides.clear();

    for (const { name, value, is_pct } of boost_tokens) {
        const matches = find_all_matching_boosts(name, value, !!is_pct, registry);
        for (const { entry, effective_value } of matches) {
            for (const b of entry.stat_bonuses) {
                let contrib = b.value * effective_value;
                if (b.round !== false) contrib = Math.floor(round_near(contrib));
                if (b.max != null) {
                    if (b.max > 0 && contrib > b.max) contrib = b.max;
                    else if (b.max < 0 && contrib < b.max) contrib = b.max;
                }
                if (b.key.startsWith('damMult.')) {
                    const key = b.key.substring(8);
                    // Potion and Vulnerability use max semantics (matching merge_stat).
                    // These represent non-stacking party buffs where only the highest applies.
                    if (b.mode === 'max' || key === 'Potion' || key === 'Vulnerability') {
                        damMult.set(key, Math.max(damMult.get(key) ?? 0, contrib));
                    } else {
                        damMult.set(key, (damMult.get(key) ?? 0) + contrib);
                    }
                } else if (b.key.startsWith('defMult.')) {
                    const key = b.key.substring(8);
                    if (b.mode === 'max' || key === 'Potion' || key === 'Vulnerability') {
                        defMult.set(key, Math.max(defMult.get(key) ?? 0, contrib));
                    } else {
                        defMult.set(key, (defMult.get(key) ?? 0) + contrib);
                    }
                } else {
                    // Direct stats Map entry (e.g. "nConvBase:4.Winded Damage", "sdPct", …)
                    if (b.mode === 'max') {
                        stats.set(b.key, Math.max(stats.get(b.key) ?? 0, contrib));
                    } else {
                        stats.set(b.key, (stats.get(b.key) ?? 0) + contrib);
                    }
                }
            }
            for (const p of entry.prop_bonuses) {
                let contrib = (p.value_per_unit ?? 1) * effective_value;
                // Apply rounding (mirrors atree_compute_scaling's round logic).
                if (p.round) contrib = Math.floor(round_near(contrib));
                // Apply output cap (mirrors atree_compute_scaling's max logic).
                if (p.max != null) {
                    if (p.max > 0 && contrib > p.max) contrib = p.max;
                    else if (p.max < 0 && contrib < p.max) contrib = p.max;
                }
                const existing = prop_overrides.get(p.ref) ?? { replace: null, add: 0, base: p.base ?? 0 };
                if (p.mode === 'add') {
                    existing.add += contrib;
                } else {
                    existing.replace = (existing.replace ?? 0) + contrib;
                }
                prop_overrides.set(p.ref, existing);
            }
        }
    }
    return { stats, prop_overrides };
}

/**
 * Compute a row's unclamped spell cost without materializing boosted row
 * stats. The cost depends on exactly four stats — int, spRaw{bs}, spPct{bs},
 * spPct{bs}Final — so this applies boost-token bonuses to those four values
 * with merge semantics identical to apply_combo_row_boosts (same token /
 * match / bonus order, same round + max + mode handling) and then evaluates
 * getBaseSpellCost/getUnclampedSpellCost's formula directly. Used by the
 * mana simulations, whose row stats were only ever read for the cost.
 */
function row_unclamped_spell_cost(base_stats, spell, boost_tokens, registry) {
    const bs = spell.mana_derived_from ?? spell.base_spell;
    const kRaw = 'spRaw' + bs, kPct = 'spPct' + bs, kFinal = 'spPct' + bs + 'Final';
    let vInt = base_stats.get('int') ?? 0;
    let vRaw = base_stats.get(kRaw) ?? 0;
    let vPct = base_stats.get(kPct) ?? 0;
    let vFinal = base_stats.get(kFinal) ?? 0;

    if (boost_tokens && boost_tokens.length) {
        for (const { name, value, is_pct } of boost_tokens) {
            const matches = find_all_matching_boosts(name, value, !!is_pct, registry);
            for (const { entry, effective_value } of matches) {
                for (const b of entry.stat_bonuses) {
                    const key = b.key;
                    if (key !== 'int' && key !== kRaw && key !== kPct && key !== kFinal) continue;
                    let contrib = b.value * effective_value;
                    if (b.round !== false) contrib = Math.floor(round_near(contrib));
                    if (b.max != null) {
                        if (b.max > 0 && contrib > b.max) contrib = b.max;
                        else if (b.max < 0 && contrib < b.max) contrib = b.max;
                    }
                    if (key === 'int') {
                        vInt = b.mode === 'max' ? Math.max(vInt, contrib) : vInt + contrib;
                    } else if (key === kRaw) {
                        vRaw = b.mode === 'max' ? Math.max(vRaw, contrib) : vRaw + contrib;
                    } else if (key === kPct) {
                        vPct = b.mode === 'max' ? Math.max(vPct, contrib) : vPct + contrib;
                    } else {
                        vFinal = b.mode === 'max' ? Math.max(vFinal, contrib) : vFinal + contrib;
                    }
                }
            }
        }
    }

    // Mirror getBaseSpellCost + getUnclampedSpellCost exactly.
    const int_reduction = skillPointsToPercentage(vInt) * skillpoint_final_mult[2];
    let cost = spell.cost * (1 - int_reduction);
    cost += vRaw;
    cost *= (1 + vPct / 100);
    return cost * (1 + vFinal / 100);
}

/**
 * Clone a spell and override already-resolved hit-count values using prop_overrides.
 * Looks up the original (unresolved) string references inside atree_merged's
 * replace_spell effects to know WHICH hits to patch.
 */
function apply_spell_prop_overrides(spell, prop_overrides, atree_merged) {
    if (!prop_overrides || prop_overrides.size === 0) return spell;
    if (!atree_merged) return spell;

    // Build map of original (unresolved) hit string refs from the atree.
    const orig_part_hits = new Map();  // partName → {subName → original_string_or_num}
    for (const [, abil] of atree_merged) {
        for (const effect of abil.effects) {
            if (effect.type !== 'replace_spell' || effect.base_spell !== spell.base_spell) continue;
            for (const part of (effect.parts ?? [])) {
                if ('hits' in part) orig_part_hits.set(part.name, part.hits);
            }
        }
    }
    // Also collect hit refs from add_spell_prop effects (e.g. Meteor Shower, Shrapnel Bomb).
    for (const [, abil] of atree_merged) {
        for (const effect of abil.effects) {
            if (effect.type !== 'add_spell_prop' || effect.base_spell !== spell.base_spell) continue;
            if (effect.target_part && 'hits' in effect) {
                const existing = orig_part_hits.get(effect.target_part);
                if (existing) {
                    orig_part_hits.set(effect.target_part, { ...existing, ...effect.hits });
                } else {
                    orig_part_hits.set(effect.target_part, effect.hits);
                }
            }
        }
    }
    if (orig_part_hits.size === 0) return spell;

    // Check if any string reference is in our overrides.
    let needs_clone = false;
    outer: for (const [, orig_hits] of orig_part_hits) {
        for (const orig_val of Object.values(orig_hits)) {
            if (typeof orig_val === 'string' && prop_overrides.has(orig_val)) {
                needs_clone = true;
                break outer;
            }
        }
    }
    if (!needs_clone) return spell;

    const clone = structuredClone(spell);
    for (const part of clone.parts) {
        if (!('hits' in part)) continue;
        const orig_hits = orig_part_hits.get(part.name);
        if (!orig_hits) continue;
        for (const sub_name of Object.keys(part.hits)) {
            const orig_val = orig_hits[sub_name];
            if (typeof orig_val === 'string' && prop_overrides.has(orig_val)) {
                const ov = prop_overrides.get(orig_val);
                if (ov.replace != null) {
                    part.hits[sub_name] = (ov.base ?? 0) + ov.replace + ov.add;
                } else {
                    part.hits[sub_name] += ov.add;
                }
            }
        }
    }
    return clone;
}

// ── Boost relevance filtering ────────────────────────────────────────────────

const _DAMAGE_STATS = new Set();
const _SPELL_STATS = new Set();
const _MELEE_STATS = new Set();
for (const e of ['n', 'e', 't', 'w', 'f', 'a']) {
    _DAMAGE_STATS.add(e + 'DamPct').add(e + 'DamRaw').add(e + 'DamAddMin').add(e + 'DamAddMax');
    _SPELL_STATS.add(e + 'SdPct').add(e + 'SdRaw');
    _MELEE_STATS.add(e + 'MdPct').add(e + 'MdRaw');
}
_DAMAGE_STATS.add('damPct').add('damRaw').add('rDamPct').add('rDamRaw');
_SPELL_STATS.add('sdPct').add('sdRaw').add('rSdPct').add('rSdRaw');
_MELEE_STATS.add('mdPct').add('mdRaw').add('rMdPct').add('rMdRaw');
const _HEAL_STATS = new Set(['hp', 'hpBonus']);
const _IRRELEVANT_STATS = new Set(['spd', 'ls', 'mr', 'ms', 'lb', 'lq', 'xpb', 'gSpd', 'gXp',
    'hprRaw', 'hprPct', 'ref', 'thorns', 'expd', 'spRegen', 'eSteal', 'sprint', 'sprintReg']);

function is_boost_relevant(entry, spell) {
    if (!spell) return true;  // no spell selected = show all

    const has_damage = spell.parts.some(p => 'multipliers' in p || 'hits' in p);
    const has_heal = spell.parts.some(p => 'max_hp_heal_pct' in p || 'hits' in p);

    const use_spell = (spell.scaling ?? 'spell') === 'spell';

    const part_ids = new Set();
    for (const part of spell.parts) {
        part_ids.add(spell.base_spell + '.' + part.name);
    }

    for (const b of entry.stat_bonuses) {
        if (_is_stat_relevant(b.key, has_damage, has_heal, use_spell, part_ids)) return true;
    }

    if (entry.prop_target_spells && entry.prop_target_spells.has(spell.base_spell)) return true;

    return false;
}

function _is_stat_relevant(key, has_damage, has_heal, use_spell, part_ids) {
    if (key.startsWith('damMult.')) {
        if (!has_damage) return false;
        const sub = key.substring(8);
        if (sub.includes(':')) {
            const part_id = sub.split(':')[1];
            return part_ids.has(part_id);
        }
        if (sub.includes(';')) {
            const qualifier = sub.split(';')[1];
            if (qualifier === 'm') return !use_spell;
        }
        return true;
    }

    if (key.startsWith('healMult.')) {
        if (!has_heal) return false;
        const sub = key.substring(9);
        if (sub.includes(':')) {
            const part_id = sub.split(':')[1];
            return part_ids.has(part_id);
        }
        return true;
    }

    if (key.startsWith('defMult.')) return false;

    if (key.includes('ConvBase')) {
        if (!has_damage) return false;
        if (key.includes(':')) {
            const part_id = key.split(':').slice(1).join(':');
            return part_ids.has(part_id);
        }
        return true;
    }

    if (_DAMAGE_STATS.has(key) || key === 'critDamPct') return has_damage;
    if (_SPELL_STATS.has(key)) return has_damage && use_spell;
    if (_MELEE_STATS.has(key)) return has_damage && !use_spell;
    if (_HEAL_STATS.has(key)) return has_heal;
    if (_IRRELEVANT_STATS.has(key)) return false;

    // Unknown key — conservatively show
    return true;
}
