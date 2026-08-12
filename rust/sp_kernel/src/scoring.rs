//! Combo-damage scoring pipeline (P2.4) — see PORT_PLAN.md.
//!
//! Faithful port of the JS worker's leaf pipeline: damage core (layer 1)
//! plus stat assembly, greedy SP, fast mana sim, and mana rescue (layer 2),
//! validated bit-exact by `bin/score_kernel.rs` against fixtures exported
//! with SOLVER_EXPORT_SCORE. JS numeric semantics are mirrored throughout
//! (missing-key NaN flow, insertion-order maps, NaN-propagating max,
//! half-up rounding). Healing is not ported (never affects total_damage).

use serde_json::Value;
use std::collections::HashMap;
use crate::{Case, Kernel, Unit};

pub type Obj = serde_json::Map<String, Value>;


pub const DAMAGE_ELEMENTS: [&str; 6] = ["n", "e", "t", "w", "f", "a"];
pub const SKP_ORDER: [&str; 5] = ["str", "dex", "int", "def", "agi"];
pub const SPELL_CAST_DELAY: f64 = 0.1;

// ── JS numeric semantics ─────────────────────────────────────────────────────

pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else if a > b { a } else { b }
}

pub fn js_round(v: f64) -> f64 {
    // JS Math.round: half-up toward +Infinity (Rust f64::round is half-away).
    if v.is_nan() { return f64::NAN; }
    (v + 0.5).floor()
}

pub fn round_near(v: f64) -> f64 {
    let eps = 0.00000001;
    if (v - js_round(v)).abs() < eps { js_round(v) } else { v }
}

// ── Stat map access (serialized-Map JSON: {"__m": {...}}) ───────────────────

pub fn as_map(v: &Value) -> Option<&Obj> {
    v.get("__m").and_then(|m| m.as_object())
}

/// Read-only view over a stat map; `Owned` carries boosted row stats.
pub enum StatsView<'a> {
    Borrowed(&'a Obj),
    Owned(Obj),
}

impl<'a> StatsView<'a> {
    pub fn obj(&self) -> &Obj {
        match self {
            StatsView::Borrowed(o) => o,
            StatsView::Owned(o) => o,
        }
    }
    pub fn has(&self, k: &str) -> bool { self.obj().contains_key(k) }
    /// stats.get(k) in a numeric context: missing → NaN (undefined arithmetic).
    pub fn num(&self, k: &str) -> f64 {
        match self.obj().get(k) {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
            Some(Value::Bool(b)) => if *b { 1.0 } else { 0.0 },
            _ => f64::NAN,
        }
    }
    /// stats.get(k) ?? 0 / || 0 semantics.
    pub fn num_or0(&self, k: &str) -> f64 {
        let v = self.num(k);
        if v.is_nan() { 0.0 } else { v }
    }
    pub fn str_of(&self, k: &str) -> Option<&str> {
        self.obj().get(k).and_then(|v| v.as_str())
    }
    pub fn nested(&self, k: &str) -> Option<&Obj> {
        self.obj().get(k).and_then(as_map)
    }
}

pub fn arr_f64(v: &Value) -> Vec<f64> {
    v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default()
}

pub fn arr_bool(v: &Value) -> Vec<bool> {
    v.as_array().map(|a| a.iter().map(|x| x.as_bool().unwrap_or(false)).collect()).unwrap_or_default()
}

// ── Game tables ──────────────────────────────────────────────────────────────

pub struct Tables {
    pub skillpoint_damage_mult: Vec<f64>,
    pub base_damage_multiplier: Vec<f64>,
    pub attack_speeds: Vec<String>,
    pub damage_keys: Vec<String>,
    pub sp_rate: f64,
    pub sp_cap: f64,
    pub sp_pct_table: Vec<f64>,
}

impl Tables {
    pub fn parse(v: &Value) -> Tables {
        Tables {
            skillpoint_damage_mult: arr_f64(&v["skillpoint_damage_mult"]),
            base_damage_multiplier: arr_f64(&v["baseDamageMultiplier"]),
            attack_speeds: v["attackSpeeds"].as_array().unwrap()
                .iter().map(|x| x.as_str().unwrap().to_string()).collect(),
            damage_keys: v["damage_keys"].as_array().unwrap()
                .iter().map(|x| x.as_str().unwrap().to_string()).collect(),
            sp_rate: v["sp_percentage_rate"].as_f64().unwrap(),
            sp_cap: v["sp_percentage_input_cap"].as_f64().unwrap(),
            sp_pct_table: v.get("sp_pct_table").map(arr_f64).unwrap_or_default(),
        }
    }
    pub fn sp_to_pct(&self, skp: f64) -> f64 {
        // Mirrors skillPointsToPercentage, including NaN flow-through
        // (NaN <= 0 and NaN >= cap are both false → pow(r, NaN) = NaN).
        if skp <= 0.0 { return 0.0; }
        let s = if skp >= self.sp_cap { self.sp_cap } else { skp };
        // Integer skill points hit the JS-exported table (V8's Math.pow and
        // Rust's powf can disagree by 1 ULP); the formula is the fallback.
        if s.fract() == 0.0 && s >= 0.0 && (s as usize) < self.sp_pct_table.len() {
            return self.sp_pct_table[s as usize];
        }
        let r = self.sp_rate;
        (r / (1.0 - r) * (1.0 - r.powf(s))) / 100.0
    }
    pub fn atk_spd_index(&self, spd: Option<&str>) -> i64 {
        match spd {
            Some(s) => self.attack_speeds.iter().position(|x| x == s).map(|i| i as i64).unwrap_or(-1),
            None => -1,
        }
    }
}

// ── calculateSpellDamage (damage_calc.js) ────────────────────────────────────

pub fn calculate_spell_damage(
    stats: &StatsView, weapon: &Obj, conversions_in: &[f64], use_spell_damage: bool,
    ignore_speed: bool, part_filter: Option<&str>, ignore_str: bool,
    ignored_mults: &[String], tables: &Tables,
) -> ([f64; 2], [f64; 2]) {
    let wview = StatsView::Borrowed(weapon);
    let crafted = wview.str_of("tier") == Some("Crafted");

    // 1. Weapon damages (crafted uses the max-roll array).
    let weapon_damages: Vec<[f64; 2]> = tables.damage_keys.iter().map(|k| {
        let v = weapon.get(k).cloned().unwrap_or(Value::Null);
        let a = if crafted {
            v.as_array().and_then(|a| a.get(1)).cloned().unwrap_or(Value::Null)
        } else { v };
        let d = arr_f64(&a);
        [d.first().copied().unwrap_or(f64::NAN), d.get(1).copied().unwrap_or(f64::NAN)]
    }).collect();
    let mut present: Vec<bool> = weapon.get("damagePresent").map(arr_bool)
        .unwrap_or_else(|| vec![false; 6]);

    // 2. Conversions (part-scoped ConvBase first, then plain ConvBase).
    let mut conversions: Vec<f64> = conversions_in.to_vec();
    if let Some(pf) = part_filter {
        for (i, el) in DAMAGE_ELEMENTS.iter().enumerate() {
            let name = format!("{}ConvBase:{}", el, pf);
            if stats.has(&name) { conversions[i] += stats.num(&name); }
        }
    }
    for (i, el) in DAMAGE_ELEMENTS.iter().enumerate() {
        let name = format!("{}ConvBase", el);
        if stats.has(&name) { conversions[i] += stats.num(&name); }
    }

    // 2.1 Neutral conversion.
    let neutral_convert = conversions[0] / 100.0;
    if neutral_convert == 0.0 { present = vec![false; 6]; }
    let mut damages: Vec<[f64; 2]> = Vec::with_capacity(6);
    let mut weapon_min = 0.0;
    let mut weapon_max = 0.0;
    for d in &weapon_damages {
        damages.push([d[0] * neutral_convert, d[1] * neutral_convert]);
        weapon_min += d[0];
        weapon_max += d[1];
    }

    // 2.2 Elemental conversions.
    let mut total_convert = 0.0;
    for i in 1..=5 {
        if conversions[i] > 0.0 {
            let f = conversions[i] / 100.0;
            damages[i][0] += f * weapon_min;
            damages[i][1] += f * weapon_max;
            present[i] = true;
            total_convert += f;
        }
    }
    total_convert += conversions[0] / 100.0;

    // 3. Attack speed multiplier.
    if !ignore_speed {
        let idx = tables.atk_spd_index(wview.str_of("atkSpd"));
        let m = if idx >= 0 { tables.base_damage_multiplier[idx as usize] } else { f64::NAN };
        for d in damages.iter_mut() { d[0] *= m; d[1] *= m; }
    }

    // 4. Additive damage.
    for (i, el) in DAMAGE_ELEMENTS.iter().enumerate() {
        if present[i] {
            damages[i][0] += stats.num(&format!("{}DamAddMin", el));
            damages[i][1] += stats.num(&format!("{}DamAddMax", el));
        }
    }

    // 5. ID bonuses.
    let specific = if use_spell_damage { "Sd" } else { "Md" };
    let specific_lower = if use_spell_damage { "sd" } else { "md" };
    let mut skill_boost = vec![0.0f64];
    for (i, skp) in SKP_ORDER.iter().enumerate() {
        skill_boost.push(tables.sp_to_pct(stats.num(skp)) * tables.skillpoint_damage_mult[i]);
    }
    let static_boost = (stats.num(&format!("{}Pct", specific_lower)) + stats.num("damPct")) / 100.0;

    let mut total_min = 0.0;
    let mut total_max = 0.0;
    let mut save_prop: Vec<[f64; 2]> = Vec::with_capacity(6);
    for (i, el) in DAMAGE_ELEMENTS.iter().enumerate() {
        save_prop.push(damages[i]);
        total_min += damages[i][0];
        total_max += damages[i][1];

        let mut boost = 1.0 + skill_boost[i] + static_boost
            + (stats.num(&format!("{}{}Pct", el, specific)) + stats.num(&format!("{}DamPct", el))) / 100.0;
        if i > 0 {
            boost += (stats.num(&format!("r{}Pct", specific)) + stats.num("rDamPct")) / 100.0;
        }
        damages[i][0] *= boost;
        damages[i][1] *= boost;
    }

    let total_elem_min = total_min - save_prop[0][0];
    let total_elem_max = total_max - save_prop[0][1];

    // 5.2 Raw application.
    let prop_raw = stats.num(&format!("{}Raw", specific_lower)) + stats.num("damRaw");
    let rainbow_raw = stats.num(&format!("r{}Raw", specific)) + stats.num("rDamRaw");
    for i in 0..6 {
        let save_obj = save_prop[i];
        let el = DAMAGE_ELEMENTS[i];
        let mut raw_boost = 0.0;
        if present[i] {
            raw_boost += stats.num(&format!("{}{}Raw", el, specific)) + stats.num(&format!("{}DamRaw", el));
        }
        let mut min_boost = raw_boost;
        let mut max_boost = raw_boost;
        if total_max > 0.0 {
            if total_min == 0.0 {
                min_boost += (save_obj[1] / total_max) * prop_raw;
            } else {
                min_boost += (save_obj[0] / total_min) * prop_raw;
            }
            max_boost += (save_obj[1] / total_max) * prop_raw;
        }
        if i != 0 && total_elem_max > 0.0 {
            if total_elem_min == 0.0 {
                min_boost += (save_obj[1] / total_elem_max) * rainbow_raw;
            } else {
                min_boost += (save_obj[0] / total_elem_min) * rainbow_raw;
            }
            max_boost += (save_obj[1] / total_elem_max) * rainbow_raw;
        }
        damages[i][0] += min_boost * total_convert;
        damages[i][1] += max_boost * total_convert;
    }

    // 6. Strength/crit multipliers + damMult map.
    let str_boost = if ignore_str { 1.0 } else { 1.0 + skill_boost[1] };
    let mut damage_mult = 1.0f64;
    let mut ele_damage_mult = [1.0f64; 6];
    if let Some(mult_map) = stats.nested("damMult") {
        for (k, v) in mult_map {
            let v = v.as_f64().unwrap_or(f64::NAN);
            if let Some(colon) = k.find(':') {
                let spell_match = &k[colon + 1..];
                if Some(spell_match) != part_filter { continue; }
            }
            if ignored_mults.iter().any(|m| m == k) { continue; }
            if let Some(semi) = k.find(';') {
                let ele_bonus = &k[semi + 1..];
                let ele_match = DAMAGE_ELEMENTS.iter().position(|e| *e == ele_bonus);
                if ele_bonus == "m" && !use_spell_damage {
                    damage_mult *= 1.0 + v / 100.0;
                } else if let Some(idx) = ele_match {
                    ele_damage_mult[idx] *= 1.0 + v / 100.0;
                }
            } else {
                damage_mult *= 1.0 + v / 100.0;
            }
        }
    }
    let crit_mult = if ignore_str { 0.0 } else { 1.0 + stats.num("critDamPct") / 100.0 };

    for i in 0..6 {
        damages[i][0] *= ele_damage_mult[i];
        damages[i][1] *= ele_damage_mult[i];
    }

    let mut total_dam_norm = [0.0f64; 2];
    let mut total_dam_crit = [0.0f64; 2];
    for d in damages.iter_mut() {
        if d[0] < 0.0 { d[0] = 0.0; }
        if d[1] < 0.0 { d[1] = 0.0; }
        let res = [
            d[0] * str_boost * damage_mult,
            d[1] * str_boost * damage_mult,
            d[0] * (str_boost + crit_mult) * damage_mult,
            d[1] * (str_boost + crit_mult) * damage_mult,
        ];
        total_dam_norm[0] += res[0];
        total_dam_norm[1] += res[1];
        total_dam_crit[0] += res[2];
        total_dam_crit[1] += res[3];
    }

    (total_dam_norm, total_dam_crit)
}

// ── Spell part evaluation (pure/spell.js) ────────────────────────────────────

#[derive(Clone, Default)]
pub struct PartResult {
    pub kind: Option<&'static str>, // "damage" | "heal" | None
    pub normal_total: [f64; 2],
    pub crit_total: [f64; 2],
    pub display: bool,
    pub name: String,
}

pub fn spell_parts(spell: &Value) -> &[Value] {
    spell.get("parts").and_then(|p| p.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
}

pub fn eval_spell_parts(
    stats: &StatsView, weapon: &Obj, spell: &Value, tables: &Tables,
) -> Vec<PartResult> {
    let use_speed = spell.get("use_atkspd").and_then(|v| v.as_bool()).unwrap_or(true);
    let use_spell = spell.get("scaling").and_then(|v| v.as_str()).unwrap_or("spell") == "spell";
    let base_spell = spell.get("base_spell").and_then(|v| v.as_i64()).unwrap_or(0);
    let parts = spell_parts(spell);
    let by_name: HashMap<&str, &Value> = parts.iter()
        .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(|n| (n, p)))
        .collect();

    pub fn eval_part<'p>(
        name: &str, memo: &mut HashMap<String, PartResult>, by_name: &HashMap<&str, &'p Value>,
        stats: &StatsView, weapon: &Obj, tables: &Tables,
        use_speed: bool, use_spell: bool, base_spell: i64,
    ) -> Option<PartResult> {
        if let Some(r) = memo.get(name) { return Some(r.clone()); }
        let part = *by_name.get(name)?;
        let part_id = format!("{}.{}", base_spell, name);
        let mut result = PartResult::default();

        if let Some(mults) = part.get("multipliers") {
            let use_str = part.get("use_str").and_then(|v| v.as_bool()).unwrap_or(true);
            let ignored: Vec<String> = part.get("ignored_mults").and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let (norm, crit) = calculate_spell_damage(
                stats, weapon, &arr_f64(mults), use_spell, !use_speed,
                Some(&part_id), !use_str, &ignored, tables);
            result.kind = Some("damage");
            result.normal_total = norm;
            result.crit_total = crit;
        } else if part.get("max_hp_heal_pct").is_some() {
            // Healing not ported: heal parts never contribute to total_damage.
            result.kind = Some("heal");
        } else if let Some(hits) = part.get("hits").and_then(|h| h.as_object()) {
            let tick_rounding = part.get("tick_rounding").and_then(|v| v.as_bool()).unwrap_or(false);
            for (sub_name, hits_v) in hits {
                let sub = eval_part(sub_name, memo, by_name, stats, weapon, tables,
                                    use_speed, use_spell, base_spell);
                let Some(sub) = sub else { continue };
                if result.kind.is_none() { result.kind = sub.kind; }
                let hits_n = hits_v.as_f64().unwrap_or(f64::NAN);
                let effective_hits = if tick_rounding {
                    1.0 / ((1.0 / hits_n * 20.0).floor() * 0.05)
                } else { hits_n };
                if sub.kind == Some("damage") {
                    result.normal_total[0] += sub.normal_total[0] * effective_hits;
                    result.normal_total[1] += sub.normal_total[1] * effective_hits;
                    result.crit_total[0] += sub.crit_total[0] * effective_hits;
                    result.crit_total[1] += sub.crit_total[1] * effective_hits;
                }
                // heal accumulation intentionally skipped (see module docs)
            }
        }
        result.name = name.to_string();
        result.display = part.get("display").and_then(|v| v.as_bool()).unwrap_or(true);
        memo.insert(name.to_string(), result.clone());
        Some(result)
    }

    let mut memo: HashMap<String, PartResult> = HashMap::new();
    parts.iter()
        .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
        .filter_map(|n| eval_part(n, &mut memo, &by_name, stats, weapon, tables,
                                  use_speed, use_spell, base_spell))
        .collect()
}

pub fn collect_referenced_part_names(spell: &Value) -> Vec<String> {
    let mut refs = Vec::new();
    for p in spell_parts(spell) {
        if let Some(hits) = p.get("hits").and_then(|h| h.as_object()) {
            for k in hits.keys() {
                if !refs.contains(k) { refs.push(k.clone()); }
            }
        }
    }
    refs
}

pub fn part_produces_damage(part: &Value, by_name: &HashMap<&str, &Value>, seen: &mut Vec<String>) -> bool {
    let Some(name) = part.get("name").and_then(|n| n.as_str()) else { return false };
    if seen.iter().any(|s| s == name) { return false; }
    seen.push(name.to_string());
    if part.get("multipliers").is_some() { return true; }
    if let Some(hits) = part.get("hits").and_then(|h| h.as_object()) {
        return hits.keys().any(|n| {
            by_name.get(n.as_str()).map(|p| part_produces_damage(p, by_name, seen)).unwrap_or(false)
        });
    }
    false
}

pub fn find_dps_root_name(spell: &Value) -> Option<String> {
    let parts = spell_parts(spell);
    if parts.is_empty() { return None; }
    let referenced = collect_referenced_part_names(spell);
    let by_name: HashMap<&str, &Value> = parts.iter()
        .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(|n| (n, p)))
        .collect();
    for p in parts {
        let Some(name) = p.get("name").and_then(|n| n.as_str()) else { continue };
        if p.get("display").and_then(|v| v.as_bool()) == Some(false) { continue; }
        if referenced.iter().any(|r| r == name) { continue; }
        if name != "DPS" && !name.ends_with(" DPS") { continue; }
        if !part_produces_damage(p, &by_name, &mut Vec::new()) { continue; }
        return Some(name.to_string());
    }
    None
}

pub fn find_dps_display_root(spell: &Value) -> Option<String> {
    let parts = spell_parts(spell);
    if parts.is_empty() { return None; }
    let by_name: HashMap<&str, &Value> = parts.iter()
        .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(|n| (n, p)))
        .collect();
    let referenced = collect_referenced_part_names(spell);
    if let Some(display) = spell.get("display").and_then(|d| d.as_str()) {
        if let Some(p) = by_name.get(display) {
            if !referenced.iter().any(|r| r == display)
                && part_produces_damage(p, &by_name, &mut Vec::new()) {
                return Some(display.to_string());
            }
        }
    }
    find_dps_root_name(spell)
}

pub fn spell_is_dps(spell: &Value) -> bool {
    let display = spell.get("display").and_then(|d| d.as_str());
    if let Some(d) = display {
        if d == "DPS" || d.ends_with(" DPS") { return true; }
    }
    let parts = spell_parts(spell);
    if display.is_some() && !parts.is_empty() {
        let by_name: HashMap<&str, &Value> = parts.iter()
            .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(|n| (n, p)))
            .collect();
        if let Some(dp) = by_name.get(display.unwrap()) {
            if dp.get("display").and_then(|v| v.as_bool()) != Some(false)
                && part_produces_damage(dp, &by_name, &mut Vec::new()) {
                return false;
            }
        }
    }
    find_dps_root_name(spell).is_some()
}

pub struct DpsHitsInfo {
    pub per_hit_name: String,
    pub max_hits: f64,
    pub dps_chain_root: String,
}

pub fn compute_dps_spell_hits_info(spell: &Value) -> Option<DpsHitsInfo> {
    if !spell_is_dps(spell) { return None; }
    let parts = spell_parts(spell);
    let by_name: HashMap<&str, &Value> = parts.iter()
        .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(|n| (n, p)))
        .collect();

    let leaf = parts.iter().find(|p| p.get("multipliers").is_some())?;
    let leaf_name = leaf.get("name")?.as_str()?;

    let display = spell.get("display").and_then(|d| d.as_str());
    let dps_name: Option<String> = match display {
        Some(d) if by_name.contains_key(d) => Some(d.to_string()),
        _ => find_dps_root_name(spell),
    };
    let dps_part = dps_name.as_deref().and_then(|n| by_name.get(n).copied())?;
    dps_part.get("hits").and_then(|h| h.as_object())?;

    let is_word_total_max = |name: &str| -> bool {
        // Mirrors /\b(Total|Max)\b/i on part names.
        name.split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|w| w.eq_ignore_ascii_case("Total") || w.eq_ignore_ascii_case("Max"))
    };
    let mut total_part = parts.iter().find(|p| {
        !std::ptr::eq(*p, dps_part)
            && p.get("hits").and_then(|h| h.as_object()).is_some()
            && p.get("name").and_then(|n| n.as_str()).map(is_word_total_max).unwrap_or(false)
    });
    if total_part.is_none() {
        let dps_part_name = dps_part.get("name").and_then(|n| n.as_str()).unwrap_or("");
        total_part = parts.iter().find(|p| {
            !std::ptr::eq(*p, dps_part)
                && p.get("hits").and_then(|h| h.as_object())
                    .map(|h| h.contains_key(dps_part_name)).unwrap_or(false)
        });
    }
    let total_part = total_part?;

    pub fn count_hits(name: &str, leaf: &str, by_name: &HashMap<&str, &Value>) -> f64 {
        if name == leaf { return 1.0; }
        let Some(p) = by_name.get(name) else { return 0.0 };
        let Some(hits) = p.get("hits").and_then(|h| h.as_object()) else { return 0.0 };
        let mut total = 0.0;
        for (sub, count) in hits {
            total += count.as_f64().unwrap_or(f64::NAN) * count_hits(sub, leaf, by_name);
        }
        total
    }
    let total_name = total_part.get("name")?.as_str()?;
    let max_hits = count_hits(total_name, leaf_name, &by_name);
    if !(max_hits > 0.0) { return None; }

    Some(DpsHitsInfo {
        per_hit_name: leaf_name.to_string(),
        max_hits,
        dps_chain_root: total_name.to_string(),
    })
}

pub fn find_display_result<'r>(spell: &Value, all: &'r [PartResult]) -> Option<&'r PartResult> {
    let display = spell.get("display").and_then(|d| d.as_str());
    let mut display_result = display.and_then(|d| all.iter().find(|r| r.name == d));
    if display_result.map(|r| r.kind) != Some(Some("damage")) {
        let dps_name = find_dps_root_name(spell);
        display_result = dps_name.as_deref()
            .and_then(|n| all.iter().find(|r| r.name == n))
            .or_else(|| all.iter().rev().find(|r| r.display && r.kind == Some("damage")));
    }
    display_result
}

pub fn compute_spell_display_avg(
    stats: &StatsView, weapon: &Obj, spell: &Value, crit_chance: f64, tables: &Tables,
    display_override: Option<&str>,
) -> f64 {
    let all = eval_spell_parts(stats, weapon, spell, tables);
    let display_result = match display_override {
        // { ...mod_spell, display: name } — explicit per-hit display always
        // resolves through find_display_result with the overridden name.
        Some(name) => {
            let r = all.iter().find(|r| r.name == name);
            if r.map(|r| r.kind) == Some(Some("damage")) { r } else {
                let dps_name = find_dps_root_name(spell);
                dps_name.as_deref().and_then(|n| all.iter().find(|r| r.name == n))
                    .or_else(|| all.iter().rev().find(|r| r.display && r.kind == Some("damage")))
            }
        }
        None => find_display_result(spell, &all),
    };
    let Some(r) = display_result else { return 0.0 };
    if r.kind != Some("damage") { return 0.0; }
    let non_crit_avg = (r.normal_total[0] + r.normal_total[1]) / 2.0;
    let crit_avg = (r.crit_total[0] + r.crit_total[1]) / 2.0;
    (1.0 - crit_chance) * non_crit_avg + crit_chance * crit_avg
}

pub fn compute_spell_flat_damage(
    stats: &StatsView, weapon: &Obj, spell: &Value, crit_chance: f64,
    exclude_root: &str, tables: &Tables,
) -> f64 {
    let all = eval_spell_parts(stats, weapon, spell, tables);
    let referenced = collect_referenced_part_names(spell);
    let mut total = 0.0;
    for r in &all {
        if r.kind != Some("damage") || !r.display { continue; }
        if r.name == exclude_root { continue; }
        if referenced.iter().any(|n| *n == r.name) { continue; }
        let non_crit_avg = (r.normal_total[0] + r.normal_total[1]) / 2.0;
        let crit_avg = (r.crit_total[0] + r.crit_total[1]) / 2.0;
        total += (1.0 - crit_chance) * non_crit_avg + crit_chance * crit_avg;
    }
    total
}

// ── Boost application (pure/boost.js) ────────────────────────────────────────

pub struct Token {
    pub name: String,
    pub value: f64,
    pub is_pct: bool,
}

pub struct PropOverride {
    pub replace: Option<f64>,
    pub add: f64,
    pub base: f64,
}

pub fn find_all_matching_boosts<'r>(
    token: &Token, registry: &'r [Value],
) -> Vec<(&'r Value, f64)> {
    let name_lower = token.name.to_lowercase().trim().to_string();
    let mut results = Vec::new();
    for entry in registry {
        let ename = entry.get("name").and_then(|n| n.as_str()).unwrap_or("").to_lowercase();
        let aliases: Vec<String> = entry.get("aliases").and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_lowercase())).collect())
            .unwrap_or_default();
        let exact = ename == name_lower
            || ename == format!("activate {}", name_lower)
            || aliases.iter().any(|a| *a == name_lower);
        let is_toggle = entry.get("type").and_then(|t| t.as_str()) == Some("toggle");
        if is_toggle {
            if exact { results.push((entry, 1.0)); }
        } else if exact {
            results.push((entry, token.value));
        } else if token.is_pct && (ename.contains(&name_lower) || ename.starts_with(&name_lower)) {
            results.push((entry, token.value));
        }
    }
    results
}

pub fn apply_combo_row_boosts<'a>(
    base: &'a Obj, tokens: &[Token], registry: &[Value],
) -> (StatsView<'a>, HashMap<String, PropOverride>) {
    if tokens.is_empty() && base.contains_key("damMult") && base.contains_key("defMult") {
        return (StatsView::Borrowed(base), HashMap::new());
    }

    let mut stats = base.clone();
    let mut dam_mult: Obj = base.get("damMult").and_then(as_map).cloned().unwrap_or_default();
    let mut def_mult: Obj = base.get("defMult").and_then(as_map).cloned().unwrap_or_default();
    let mut prop_overrides: HashMap<String, PropOverride> = HashMap::new();

    for token in tokens {
        for (entry, effective_value) in find_all_matching_boosts(token, registry) {
            for b in entry.get("stat_bonuses").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
                let mut contrib = b.get("value").and_then(|v| v.as_f64()).unwrap_or(f64::NAN) * effective_value;
                if b.get("round").and_then(|v| v.as_bool()) != Some(false) {
                    contrib = round_near(contrib).floor();
                }
                if let Some(mx) = b.get("max").and_then(|v| v.as_f64()) {
                    if mx > 0.0 && contrib > mx { contrib = mx; }
                    else if mx < 0.0 && contrib < mx { contrib = mx; }
                }
                let key = b.get("key").and_then(|k| k.as_str()).unwrap_or("");
                let mode_max = b.get("mode").and_then(|m| m.as_str()) == Some("max");
                if let Some(sub) = key.strip_prefix("damMult.") {
                    let cur = dam_mult.get(sub).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let nv = if mode_max || sub == "Potion" || sub == "Vulnerability" {
                        js_max(cur, contrib)
                    } else { cur + contrib };
                    dam_mult.insert(sub.to_string(), Value::from(nv));
                } else if let Some(sub) = key.strip_prefix("defMult.") {
                    let cur = def_mult.get(sub).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let nv = if mode_max || sub == "Potion" || sub == "Vulnerability" {
                        js_max(cur, contrib)
                    } else { cur + contrib };
                    def_mult.insert(sub.to_string(), Value::from(nv));
                } else {
                    let cur = stats.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let nv = if mode_max { js_max(cur, contrib) } else { cur + contrib };
                    stats.insert(key.to_string(), Value::from(nv));
                }
            }
            for p in entry.get("prop_bonuses").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
                let vpu = p.get("value_per_unit").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let mut contrib = vpu * effective_value;
                if p.get("round").and_then(|v| v.as_bool()) == Some(true) {
                    contrib = round_near(contrib).floor();
                }
                if let Some(mx) = p.get("max").and_then(|v| v.as_f64()) {
                    if mx > 0.0 && contrib > mx { contrib = mx; }
                    else if mx < 0.0 && contrib < mx { contrib = mx; }
                }
                let rf = p.get("ref").and_then(|r| r.as_str()).unwrap_or("").to_string();
                let base_v = p.get("base").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let existing = prop_overrides.entry(rf).or_insert(PropOverride {
                    replace: None, add: 0.0, base: base_v,
                });
                if p.get("mode").and_then(|m| m.as_str()) == Some("add") {
                    existing.add += contrib;
                } else {
                    existing.replace = Some(existing.replace.unwrap_or(0.0) + contrib);
                }
            }
        }
    }

    let mut wrap_dam = Obj::new(); wrap_dam.insert("__m".into(), Value::Object(dam_mult));
    let mut wrap_def = Obj::new(); wrap_def.insert("__m".into(), Value::Object(def_mult));
    stats.insert("damMult".into(), Value::Object(wrap_dam));
    stats.insert("defMult".into(), Value::Object(wrap_def));
    (StatsView::Owned(stats), prop_overrides)
}

/// apply_spell_prop_overrides — patch hit counts through atree hit-string refs.
pub fn apply_spell_prop_overrides(
    spell: &Value, prop_overrides: &HashMap<String, PropOverride>,
    hit_refs: &HashMap<i64, HashMap<String, Obj>>,
) -> Value {
    if prop_overrides.is_empty() { return spell.clone(); }
    let base_spell = spell.get("base_spell").and_then(|v| v.as_i64()).unwrap_or(0);
    let Some(orig_part_hits) = hit_refs.get(&base_spell) else { return spell.clone() };
    if orig_part_hits.is_empty() { return spell.clone(); }

    let mut needs_clone = false;
    'outer: for (_, orig_hits) in orig_part_hits {
        for orig_val in orig_hits.values() {
            if let Some(s) = orig_val.as_str() {
                if prop_overrides.contains_key(s) { needs_clone = true; break 'outer; }
            }
        }
    }
    if !needs_clone { return spell.clone(); }

    let mut clone = spell.clone();
    if let Some(parts) = clone.get_mut("parts").and_then(|p| p.as_array_mut()) {
        for part in parts {
            let Some(name) = part.get("name").and_then(|n| n.as_str()).map(String::from) else { continue };
            let Some(orig_hits) = orig_part_hits.get(&name) else { continue };
            let Some(hits) = part.get_mut("hits").and_then(|h| h.as_object_mut()) else { continue };
            let keys: Vec<String> = hits.keys().cloned().collect();
            for sub_name in keys {
                let Some(orig_val) = orig_hits.get(&sub_name).and_then(|v| v.as_str()) else { continue };
                let Some(ov) = prop_overrides.get(orig_val) else { continue };
                let cur = hits.get(&sub_name).and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
                let nv = match ov.replace {
                    Some(r) => ov.base + r + ov.add,
                    None => cur + ov.add,
                };
                hits.insert(sub_name, Value::from(nv));
            }
        }
    }
    clone
}

// ── compute_melee_time_hits (pure/utils.js) ──────────────────────────────────

pub fn compute_melee_time_hits(
    qty_seconds: f64, base_stats: &StatsView, melee_cd_override: Option<f64>, tables: &Tables,
) -> f64 {
    let melee_period = match melee_cd_override {
        Some(p) => p,
        None => {
            let idx = tables.atk_spd_index(base_stats.str_of("atkSpd"));
            let tier = base_stats.num_or0("atkTier");
            let mut adj = idx as f64 + tier;
            if adj < 0.0 { adj = 0.0; }
            if adj > 6.0 { adj = 6.0; }
            1.0 / tables.base_damage_multiplier[adj as usize]
        }
    };
    qty_seconds / melee_period.max(SPELL_CAST_DELAY)
}

// ── compute_combo_damage_totals (pure/engine.js) ─────────────────────────────

pub struct Row {
    pub qty: f64,
    pub dmg_excl: bool,
    pub pseudo: bool,
    pub pseudo_kind: Option<String>,
    pub is_melee_time: bool,
    pub melee_cd_override: Option<f64>,
    pub dps_per_hit_name: Option<String>,
    pub dps_hits: f64,
    pub dps_hits_override: Option<f64>,
    pub tokens: Vec<Token>,
    pub spell: Option<Value>,
    // mana-sim fields
    pub mana_excl: bool,
    pub cast_time: Option<f64>,
    pub delay: Option<f64>,
    pub recast_penalties: Vec<f64>,
    pub has_loop_marker: bool,
}

pub fn parse_rows(v: &Value) -> Vec<Row> {
    let mut rows = Vec::new();
    for r in v.as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let tokens = r.get("boost_tokens").and_then(|t| t.as_array())
            .map(|a| a.iter().map(|t| Token {
                name: t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                value: t.get("value").and_then(|x| x.as_f64()).unwrap_or(f64::NAN),
                is_pct: t.get("is_pct").and_then(|x| x.as_bool()).unwrap_or(false),
            }).collect())
            .unwrap_or_default();
        rows.push(Row {
            qty: r.get("qty").and_then(|q| q.as_f64()).unwrap_or(0.0),
            dmg_excl: r.get("dmg_excl").and_then(|x| x.as_bool()).unwrap_or(false),
            pseudo: r.get("pseudo").map(|p| !p.is_null()).unwrap_or(false),
            pseudo_kind: r.get("pseudo").and_then(|p| p.as_str()).map(String::from),
            is_melee_time: r.get("is_melee_time").and_then(|x| x.as_bool()).unwrap_or(false),
            melee_cd_override: r.get("melee_cd_override").and_then(|x| x.as_f64()),
            dps_per_hit_name: r.get("dps_per_hit_name").and_then(|x| x.as_str()).map(String::from),
            dps_hits: r.get("dps_hits").and_then(|x| x.as_f64()).unwrap_or(0.0),
            dps_hits_override: r.get("dps_hits_override").and_then(|x| x.as_f64()),
            tokens,
            spell: r.get("spell").filter(|s| !s.is_null()).cloned(),
            mana_excl: r.get("mana_excl").and_then(|x| x.as_bool()).unwrap_or(false),
            cast_time: r.get("cast_time").and_then(|x| x.as_f64()),
            delay: r.get("delay").and_then(|x| x.as_f64()),
            recast_penalties: r.get("recast_penalties").map(arr_f64).unwrap_or_default(),
            has_loop_marker: r.get("loop_start").map(|v| !v.is_null()).unwrap_or(false)
                || r.get("loop_end").map(|v| !v.is_null()).unwrap_or(false),
        });
    }
    rows
}

#[allow(clippy::too_many_arguments)]
pub fn eval_combo_damage(
    combo_base: &Obj, weapon: &Obj, rows: &[Row], registry: &[Value],
    hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables,
) -> f64 {
    let base_view = StatsView::Borrowed(combo_base);
    // crit = skillPointsToPercentage(combo_base.get('dex') || 0)
    let dex = base_view.num("dex");
    let crit = tables.sp_to_pct(if dex.is_nan() || dex == 0.0 { 0.0 } else { dex });

    let mut total_damage = 0.0;
    for row in rows {
        let Some(spell) = &row.spell else { continue };
        if row.qty <= 0.0 || row.pseudo { continue; }

        let (stats, prop_overrides) = apply_combo_row_boosts(combo_base, &row.tokens, registry);
        let mod_spell = apply_spell_prop_overrides(spell, &prop_overrides, hit_refs);

        let mut eff_dps_name = row.dps_per_hit_name.clone();
        let mut eff_dps_hits = row.dps_hits;
        let mut dps_chain_root: Option<String> = None;
        if eff_dps_name.is_none() {
            if let Some(info) = compute_dps_spell_hits_info(&mod_spell) {
                eff_dps_name = Some(info.per_hit_name);
                eff_dps_hits = row.dps_hits_override.unwrap_or(info.max_hits);
                dps_chain_root = Some(info.dps_chain_root);
            }
        }

        let per_cast = match &eff_dps_name {
            Some(name) => compute_spell_display_avg(&stats, weapon, &mod_spell, crit, tables, Some(name)) * eff_dps_hits,
            None => compute_spell_display_avg(&stats, weapon, &mod_spell, crit, tables, None),
        };

        let mut flat_per_cast = 0.0;
        let chain_root = dps_chain_root.or_else(|| {
            if spell_is_dps(&mod_spell) { find_dps_display_root(&mod_spell) } else { None }
        });
        if let Some(root) = &chain_root {
            flat_per_cast = compute_spell_flat_damage(&stats, weapon, &mod_spell, crit, root, tables);
        }

        let eff_qty = if row.is_melee_time {
            compute_melee_time_hits(row.qty, &base_view, row.melee_cd_override, tables)
        } else { row.qty };
        let row_damage = if row.dmg_excl { 0.0 } else { per_cast * eff_qty + flat_per_cast };
        total_damage += row_damage;
    }
    total_damage
}

// ── Layer 2: build-stat assembly (PORT_PLAN.md) ─────────────────────────────
//
// From a leaf's item names, reproduce the worker's assembled combo_base at
// the case's total_sp: createBaseStatmap → per-item additive accumulation →
// set bonuses → finalizeStatmap → assemble_combo_stats (skp, classDef,
// atree_raw merge, scaling plan, static_boosts). Validated key-by-key
// against the exported combo_base, then end-to-end by scoring it.

/// merge_stat (build_utils.js): dotted mult-map keys, non-stacking maxima,
/// plain additive otherwise.
pub fn merge_stat(stats: &mut Obj, name: &str, value: &Value) {
    const NONSTACKING: [&str; 3] = ["Potion", "Vulnerability", "Mask"];
    let start = name.split('.').next().unwrap_or(name);
    if start == "damMult" || start == "defMult" || start == "healMult" || start == "manaMult" {
        // Ensure nested map exists.
        if !stats.contains_key(start) {
            let mut wrap = Obj::new();
            wrap.insert("__m".into(), Value::Object(Obj::new()));
            stats.insert(start.to_string(), Value::Object(wrap));
        }
        if let Some(m) = value.get("__m").and_then(|v| v.as_object()).cloned() {
            // Merging a whole map: recurse per entry.
            for (k, v) in &m {
                let nested = stats.get_mut(start).and_then(|w| w.get_mut("__m")).and_then(|v| v.as_object_mut()).unwrap();
                merge_plain(nested, k, v, &NONSTACKING);
            }
            return;
        }
        let rest = &name[name.find('.').map(|i| i + 1).unwrap_or(name.len())..];
        let nested = stats.get_mut(start).and_then(|w| w.get_mut("__m")).and_then(|v| v.as_object_mut()).unwrap();
        merge_plain(nested, rest, value, &NONSTACKING);
        return;
    }
    merge_plain(stats, name, value, &NONSTACKING);
}

/// The scalar tail of merge_stat: additive when present, insert otherwise.
/// Mirrors the JS non-stacking check only at the mult-map level (the JS
/// nonstacking branch runs before recursing into the map).
pub fn merge_plain(map: &mut Obj, name: &str, value: &Value, nonstacking: &[&str]) {
    // Non-stacking keys keep the max (JS checks this on mult maps; on plain
    // maps these names never occur, so the branch is inert there).
    if nonstacking.contains(&name) {
        if let Some(prev) = map.get(name).and_then(|v| v.as_f64()) {
            let nv = value.as_f64().unwrap_or(f64::NAN);
            if nv > prev { map.insert(name.to_string(), value.clone()); }
            return;
        }
    }
    match map.get(name) {
        Some(prev) => {
            let sum = prev.as_f64().unwrap_or(f64::NAN) + value.as_f64().unwrap_or(f64::NAN);
            map.insert(name.to_string(), Value::from(sum));
        }
        None => { map.insert(name.to_string(), value.clone()); }
    }
}

/// _merge_into (pure/utils.js): merge a source stat map into target.
pub fn merge_into(target: &mut Obj, source: Option<&Obj>) {
    let Some(source) = source else { return };
    for (k, v) in source {
        if let Some(m) = v.get("__m").and_then(|x| x.as_object()) {
            for (mk, mv) in m {
                merge_stat(target, &format!("{}.{}", k, mk), mv);
            }
        } else {
            merge_stat(target, k, v);
        }
    }
}

pub struct Layer2 {
    pub item_registry: HashMap<String, Obj>,
    pub sets_data: Obj,
    pub tome_sms: Vec<Obj>,
    pub atree_raw: Option<Obj>,
    pub static_boosts: Option<Obj>,
    pub scaling_kind: String,
    pub scaled_cached: Option<Obj>,
    pub const_scaled: Option<Obj>,
    pub var_effects: Vec<Value>,
    pub static_ids: Vec<String>,
    pub must_ids: Vec<String>,
    pub hp_base: f64,
    pub class_def: HashMap<String, f64>,
    pub skp_order: Vec<String>,
}

impl Layer2 {
    pub fn parse(v: &Value) -> Option<Layer2> {
        let l2 = v.get("layer2")?;
        let consts = l2.get("constants")?;
        let mut item_registry = HashMap::new();
        for (name, sm) in l2.get("item_registry")?.as_object()? {
            if let Some(m) = as_map(sm) { item_registry.insert(name.clone(), m.clone()); }
        }
        // none items keyed by displayName too
        if let Some(nones) = l2.get("none_item_sms").and_then(|x| x.as_array()) {
            for sm in nones {
                if let Some(m) = as_map(sm) {
                    if let Some(n) = m.get("displayName").and_then(|x| x.as_str()) {
                        item_registry.entry(n.to_string()).or_insert_with(|| m.clone());
                    }
                }
            }
        }
        let plan = l2.get("scaling_plan")?;
        let strvec = |v: &Value| -> Vec<String> {
            v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
        };
        Some(Layer2 {
            item_registry,
            sets_data: l2.get("sets_data").and_then(as_map).cloned().unwrap_or_default(),
            tome_sms: l2.get("tome_sms").and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|t| as_map(t).cloned()).collect()).unwrap_or_default(),
            atree_raw: l2.get("atree_raw").and_then(as_map).cloned(),
            static_boosts: l2.get("static_boosts").and_then(as_map).cloned(),
            scaling_kind: plan.get("kind").and_then(|k| k.as_str()).unwrap_or("full").to_string(),
            scaled_cached: plan.get("scaled").and_then(as_map).cloned(),
            const_scaled: plan.get("const_scaled").and_then(as_map).cloned(),
            var_effects: plan.get("var_effects").and_then(|x| x.as_array()).cloned().unwrap_or_default(),
            static_ids: strvec(consts.get("statmap_static_ids")?),
            must_ids: strvec(consts.get("statmap_must_ids")?),
            hp_base: consts.get("hp_base_for_level")?.as_f64()?,
            class_def: consts.get("class_def")?.as_object()?
                .iter().filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f))).collect(),
            skp_order: strvec(consts.get("skp_order")?),
        })
    }

    /// _incr_add_item: maxRolls (minus static ids) + static ids, additive.
    pub fn add_item(&self, sm: &mut Obj, item: &Obj) {
        if let Some(mr) = item.get("maxRolls").and_then(as_map) {
            for (id, value) in mr {
                if self.static_ids.iter().any(|s| s == id) { continue; }
                let v = value.as_f64().unwrap_or(f64::NAN);
                if v == 0.0 { continue; } // JS: `if (!value) skip` in compiled entries
                let cur = sm.get(id).and_then(|x| x.as_f64()).unwrap_or(0.0);
                sm.insert(id.clone(), Value::from(cur + v));
            }
        }
        for id in &self.static_ids {
            let v = item.get(id).and_then(|x| x.as_f64()).unwrap_or(0.0);
            if v == 0.0 { continue; }
            let cur = sm.get(id).and_then(|x| x.as_f64()).unwrap_or(0.0);
            sm.insert(id.clone(), Value::from(cur + v));
        }
    }

    /// Build combo_base for a case's items at the given total_sp.
    pub fn assemble(&self, item_names: &[&str], total_sp: &[f64], weapon: &Obj) -> Result<Obj, String> {
        // createBaseStatmap
        let mut sm = Obj::new();
        for id in self.static_ids.iter().chain(self.must_ids.iter()) {
            sm.insert(id.clone(), Value::from(0.0));
        }
        sm.insert("hp".into(), Value::from(self.hp_base));
        sm.insert("agiDef".into(), Value::from(90.0));

        // Items: 8 equips + tomes + weapon (additive, order-independent sums)
        let mut equips: Vec<&Obj> = Vec::new();
        for name in item_names {
            let item = self.item_registry.get(*name)
                .ok_or_else(|| format!("item not in registry: {}", name))?;
            equips.push(item);
        }
        for item in &equips { self.add_item(&mut sm, item); }
        for tome in &self.tome_sms { self.add_item(&mut sm, tome); }
        self.add_item(&mut sm, weapon);

        // Set bonuses (skip SP keys) from non-crafted equips' 'set' names.
        let mut set_counts: Vec<(String, i64)> = Vec::new();
        for item in &equips {
            if item.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false) { continue; }
            let Some(set_name) = item.get("set").and_then(|v| v.as_str()) else { continue };
            match set_counts.iter_mut().find(|(n, _)| n == set_name) {
                Some((_, c)) => *c += 1,
                None => set_counts.push((set_name.to_string(), 1)),
            }
        }
        // JS coerces in `(sm.get(id)||0) + bonus[id]`: booleans become 0/1
        // (e.g. the illegal-at-2 double-ring marker `illegal: true`).
        let js_num = |v: &Value| -> f64 {
            match v {
                Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
                Value::Bool(b) => if *b { 1.0 } else { 0.0 },
                Value::Null => 0.0,
                _ => f64::NAN,
            }
        };
        for (set_name, count) in &set_counts {
            let Some(set_data) = self.sets_data.get(set_name) else { continue };
            let bonuses = set_data.get("bonuses").and_then(|b| b.as_array());
            let Some(bonus) = bonuses.and_then(|b| b.get((*count - 1) as usize)) else { continue };
            let Some(bonus) = bonus.as_object() else { continue };
            for (id, v) in bonus {
                if self.skp_order.iter().any(|s| s == id) { continue; }
                let cur = sm.get(id).and_then(|x| x.as_f64()).unwrap_or(0.0);
                sm.insert(id.clone(), Value::from(cur + js_num(v)));
            }
        }

        // finalizeStatmap
        let mut dam_mult = Obj::new();
        dam_mult.insert("tome".into(), Value::from(sm.get("damMobs").and_then(|v| v.as_f64()).unwrap_or(0.0)));
        let mut def_mult = Obj::new();
        def_mult.insert("tome".into(), Value::from(sm.get("defMobs").and_then(|v| v.as_f64()).unwrap_or(0.0)));
        let wrap = |m: Obj| { let mut w = Obj::new(); w.insert("__m".into(), Value::Object(m)); Value::Object(w) };
        sm.insert("damMult".into(), wrap(dam_mult));
        sm.insert("defMult".into(), wrap(def_mult));
        let mut major_ids: Vec<Value> = Vec::new();
        for item in equips.iter().copied().chain(self.tome_sms.iter()).chain(std::iter::once(weapon)) {
            // Item maps store majorIds as a plain array; sets ({__s}) also
            // accepted for robustness.
            let mids = item.get("majorIds").and_then(|v| {
                v.as_array().or_else(|| v.get("__s").and_then(|s| s.as_array()))
            });
            if let Some(mids) = mids {
                for mid in mids {
                    if !major_ids.contains(mid) { major_ids.push(mid.clone()); }
                }
            }
        }
        let mut mid_wrap = Obj::new();
        mid_wrap.insert("__s".into(), Value::Array(major_ids));
        sm.insert("activeMajorIDs".into(), Value::Object(mid_wrap));
        sm.insert("poisonPct".into(), Value::from(0.0));
        let mut heal_mult = Obj::new();
        heal_mult.insert("item".into(), Value::from(sm.get("healPct").and_then(|v| v.as_f64()).unwrap_or(0.0)));
        sm.insert("healMult".into(), wrap(heal_mult));
        if let Some(spd) = weapon.get("atkSpd") { sm.insert("atkSpd".into(), spd.clone()); }

        // assemble_combo_stats: pre_scale = clone + skp + classDef + atree_raw
        let mut pre_scale = sm;
        for (i, skp) in self.skp_order.iter().enumerate() {
            pre_scale.insert(skp.clone(), Value::from(total_sp[i]));
        }
        if let Some(wt) = weapon.get("type").and_then(|v| v.as_str()) {
            let cd = self.class_def.get(wt).copied().unwrap_or(1.0);
            pre_scale.insert("classDef".into(), Value::from(cd));
        }
        merge_into(&mut pre_scale, self.atree_raw.as_ref());
        // radiance_boost asserted null at export for supported scenarios.

        let mut var_out: Option<Obj> = None;
        let scaled: Option<&Obj> = match self.scaling_kind.as_str() {
            "cached" => self.scaled_cached.as_ref(),
            "split" => {
                // atree_eval_stat_effects on lowered var effects.
                let mut out = Obj::new();
                for eff in &self.var_effects {
                    let mut total = 0.0;
                    total += eff.get("const_add").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    for term in eff.get("terms").and_then(|t| t.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
                        let stat = term.get("stat").and_then(|s| s.as_str()).unwrap_or("");
                        let factor = term.get("factor").and_then(|f| f.as_f64()).unwrap_or(f64::NAN);
                        let v = pre_scale.get(stat).and_then(|x| x.as_f64()).unwrap_or(0.0);
                        total += v * factor;
                    }
                    let round = eff.get("round").and_then(|v| v.as_bool()).unwrap_or(true);
                    let positive = eff.get("positive").and_then(|v| v.as_bool()).unwrap_or(true);
                    let mut t = total;
                    if round { t = round_near(t).floor(); }
                    if positive && t < 0.0 { t = 0.0; }
                    if let Some(mx) = eff.get("max").and_then(|v| v.as_f64()) {
                        if mx > 0.0 && t > mx { t = mx; }
                        if mx < 0.0 && t < mx { t = mx; }
                    }
                    for output in eff.get("outputs").and_then(|o| o.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
                        if let Some(name) = output.as_str() {
                            merge_stat(&mut out, name, &Value::from(t));
                        }
                    }
                }
                var_out = Some(out);
                self.const_scaled.as_ref()
            }
            _ => return Err(format!("unsupported scaling plan: {}", self.scaling_kind)),
        };

        let mut combo_base = pre_scale;
        merge_into(&mut combo_base, scaled);
        merge_into(&mut combo_base, var_out.as_ref());
        merge_into(&mut combo_base, self.static_boosts.as_ref());
        Ok(combo_base)
    }
}

// ── Layer 2 sub-pieces 3–4: greedy SP + fast mana check ─────────────────────

/// simulate_combo_mana_fast, ported for the supported subset: no buff
/// states, no loop brackets, no Blood Pact. Returns
/// (start_mana, end_mana, has_hp_warning, has_mana_warning).
#[allow(clippy::too_many_arguments)]
pub fn simulate_mana_fast(
    rows: &[Row], combo_base: &Obj, has_transcendence: bool,
    registry: &[Value], tables: &Tables, consts: &L2Consts,
) -> (f64, f64, bool, bool) {
    let stats = StatsView::Borrowed(combo_base);
    let mr = stats.num_or0("mr");
    let ms = stats.num_or0("ms");
    let item_mana = stats.num_or0("maxMana");
    let int_mana = (tables.sp_to_pct(stats.num_or0("int")) * 100.0).floor();
    let start_mana = 100.0 + item_mana + int_mana;
    let max_mana = start_mana;
    let mut mana_wasted = 0.0;

    let base_hp = stats.num_or0("hp");
    let hp_bonus = stats.num_or0("hpBonus");
    let max_hp = js_max(5.0, base_hp + hp_bonus);

    let mr_per_sec = (mr + consts.base_mana_regen) / consts.mana_tick_seconds;

    let mut adj = tables.atk_spd_index(stats.str_of("atkSpd")) as f64 + stats.num_or0("atkTier");
    if adj < 0.0 { adj = 0.0; }
    if adj > 6.0 { adj = 6.0; }
    let ms_per_hit = if ms != 0.0 { ms / 3.0 / tables.base_damage_multiplier[adj as usize] } else { 0.0 };
    let melee_period = 1.0 / tables.base_damage_multiplier[adj as usize];
    let mut melee_cd_remaining = 0.0f64;

    let mut mana = start_mana;
    let mut hp = max_hp;
    let mut has_hp_warning = false;
    let mut has_mana_warning = false;

    // _advance_time_fast (no buff states): regen with cap + wasted tracking.
    macro_rules! advance {
        ($dt:expr) => {{
            let dt = $dt;
            if dt > 0.0 {
                let uncapped = mana + mr_per_sec * dt;
                if uncapped > max_mana { mana_wasted += uncapped - max_mana; }
                mana = if uncapped < max_mana { uncapped } else { max_mana };
            }
        }};
    }
    let _ = mana_wasted;

    for row in rows {
        // Add Flat Mana: inject (or drain) qty mana at this point.
        if row.pseudo_kind.as_deref() == Some("add_flat_mana") {
            if !row.mana_excl && row.qty != 0.0 {
                let uncapped = mana + row.qty;
                if uncapped > max_mana { mana_wasted += uncapped - max_mana; }
                mana = js_max(0.0, if uncapped < max_mana { uncapped } else { max_mana });
            }
            continue;
        }
        if row.pseudo || row.qty <= 0.0 { continue; }
        let Some(spell) = &row.spell else { continue };
        if row.mana_excl { continue; }

        let unclamped_cost = if spell.get("cost").map(|c| !c.is_null()).unwrap_or(false) {
            row_unclamped_spell_cost(combo_base, spell, &row.tokens, registry, tables, consts)
        } else { 0.0 };
        let is_spell = spell.get("cost").map(|c| !c.is_null()).unwrap_or(false);
        let is_melee_scaling = spell.get("scaling").and_then(|s| s.as_str()) == Some("melee");
        let recast_base = spell.get("mana_derived_from").and_then(|v| v.as_i64())
            .or_else(|| spell.get("base_spell").and_then(|v| v.as_i64())).unwrap_or(0);
        let is_melee = recast_base == 0;

        let eff_cast_time = if is_melee { 0.0 } else { row.cast_time.unwrap_or(consts.spell_cast_time) };
        let eff_delay = row.delay.unwrap_or(consts.spell_cast_delay);
        let base_view = StatsView::Borrowed(combo_base);
        let sim_qty = if row.is_melee_time {
            // JS passes eff_delay as the floor for the melee period here.
            let period = match row.melee_cd_override {
                Some(p) => p,
                None => melee_period,
            };
            js_round(row.qty / period.max(eff_delay))
        } else {
            js_round(row.qty)
        } as i64;
        let _ = base_view;
        let eff_melee_period = row.melee_cd_override.unwrap_or(melee_period);

        for c in 0..sim_qty {
            // compute_wall_dt
            let (pre_dt, post_dt, new_cd) = if is_melee {
                (melee_cd_remaining, eff_delay, js_max(0.0, eff_melee_period - eff_delay))
            } else if is_spell {
                let spell_dt = eff_cast_time + eff_delay;
                (eff_cast_time, eff_delay, js_max(0.0, melee_cd_remaining - spell_dt))
            } else {
                (0.0, 0.0, melee_cd_remaining)
            };
            melee_cd_remaining = new_cd;

            advance!(pre_dt);

            if is_melee_scaling && ms_per_hit != 0.0 {
                let uncapped = mana + ms_per_hit;
                if uncapped > max_mana { mana_wasted += uncapped - max_mana; }
                mana = uncapped.min(max_mana).max(0.0);
            }

            // Spell-level HP cost (e.g. Mindless Slaughter)
            let spell_hp_cost = spell.get("hp_cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if spell_hp_cost > 0.0 {
                let hp_deduction = spell_hp_cost / 100.0 * max_hp;
                if hp < hp_deduction { has_hp_warning = true; }
                hp -= hp_deduction;
            }

            if is_spell {
                let penalty = row.recast_penalties.get(c as usize).copied().unwrap_or(0.0);
                let effective_cost = js_max(1.0, unclamped_cost + penalty);
                let adj_cost = if has_transcendence { effective_cost * 0.75 } else { effective_cost };
                if mana >= effective_cost {
                    mana -= adj_cost;
                } else {
                    mana -= effective_cost;
                    has_mana_warning = true;
                }
            }

            advance!(post_dt);
        }
    }
    (start_mana, mana, has_hp_warning, has_mana_warning)
}

/// row_unclamped_spell_cost (pure/boost.js).
pub fn row_unclamped_spell_cost(
    base_stats: &Obj, spell: &Value, tokens: &[Token], registry: &[Value],
    tables: &Tables, consts: &L2Consts,
) -> f64 {
    let bs = spell.get("mana_derived_from").and_then(|v| v.as_i64())
        .or_else(|| spell.get("base_spell").and_then(|v| v.as_i64())).unwrap_or(0);
    let k_raw = format!("spRaw{}", bs);
    let k_pct = format!("spPct{}", bs);
    let k_final = format!("spPct{}Final", bs);
    let sv = StatsView::Borrowed(base_stats);
    let mut v_int = sv.num_or0("int");
    let mut v_raw = sv.num_or0(&k_raw);
    let mut v_pct = sv.num_or0(&k_pct);
    let mut v_final = sv.num_or0(&k_final);

    for token in tokens {
        for (entry, effective_value) in find_all_matching_boosts(token, registry) {
            for b in entry.get("stat_bonuses").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
                let key = b.get("key").and_then(|k| k.as_str()).unwrap_or("");
                if key != "int" && key != k_raw && key != k_pct && key != k_final { continue; }
                let mut contrib = b.get("value").and_then(|v| v.as_f64()).unwrap_or(f64::NAN) * effective_value;
                if b.get("round").and_then(|v| v.as_bool()) != Some(false) {
                    contrib = round_near(contrib).floor();
                }
                if let Some(mx) = b.get("max").and_then(|v| v.as_f64()) {
                    if mx > 0.0 && contrib > mx { contrib = mx; }
                    else if mx < 0.0 && contrib < mx { contrib = mx; }
                }
                let mode_max = b.get("mode").and_then(|m| m.as_str()) == Some("max");
                if key == "int" { v_int = if mode_max { js_max(v_int, contrib) } else { v_int + contrib }; }
                else if key == k_raw { v_raw = if mode_max { js_max(v_raw, contrib) } else { v_raw + contrib }; }
                else if key == k_pct { v_pct = if mode_max { js_max(v_pct, contrib) } else { v_pct + contrib }; }
                else { v_final = if mode_max { js_max(v_final, contrib) } else { v_final + contrib }; }
            }
        }
    }

    let int_reduction = tables.sp_to_pct(v_int) * consts.skillpoint_final_mult_2;
    let mut cost = spell.get("cost").and_then(|c| c.as_f64()).unwrap_or(f64::NAN) * (1.0 - int_reduction);
    cost += v_raw;
    cost *= 1.0 + v_pct / 100.0;
    cost * (1.0 + v_final / 100.0)
}

pub struct L2Consts {
    pub base_mana_regen: f64,
    pub mana_tick_seconds: f64,
    pub spell_cast_time: f64,
    pub spell_cast_delay: f64,
    skillpoint_final_mult_2: f64,
    pub combo_time: f64,
    pub allow_downtime: bool,
    pub hp_casting: bool,
    pub sp_budget: i32,
}

impl L2Consts {
    pub fn parse(fixture: &Value) -> Option<L2Consts> {
        let l2 = fixture.get("layer2")?;
        let c = l2.get("constants")?;
        Some(L2Consts {
            base_mana_regen: c.get("base_mana_regen")?.as_f64()?,
            mana_tick_seconds: c.get("mana_tick_seconds")?.as_f64()?,
            spell_cast_time: c.get("spell_cast_time")?.as_f64()?,
            spell_cast_delay: c.get("spell_cast_delay")?.as_f64()?,
            skillpoint_final_mult_2: fixture.get("tables")?
                .get("skillpoint_final_mult")?.as_array()?.get(2)?.as_f64()?,
            combo_time: l2.get("combo_time").and_then(|v| v.as_f64()).unwrap_or(0.0),
            allow_downtime: l2.get("allow_downtime").and_then(|v| v.as_bool()).unwrap_or(false),
            hp_casting: l2.get("hp_casting").and_then(|v| v.as_bool()).unwrap_or(false),
            sp_budget: l2.get("sp_budget").and_then(|v| v.as_i64()).unwrap_or(200) as i32,
        })
    }
}

/// eval_combo_mana_check (fast-sim path) for the supported subset:
/// no loops, no Blood Pact, no buff states.
pub fn mana_check_passes(
    rows: &[Row], combo_base: &Obj, registry: &[Value], tables: &Tables, consts: &L2Consts,
) -> bool {
    if consts.combo_time == 0.0 && !consts.hp_casting {
        return true;
    }
    assert!(!rows.iter().any(|r| r.has_loop_marker),
        "loop brackets not supported in the ported fast mana sim");
    let has_transcendence = combo_base.get("activeMajorIDs")
        .and_then(|v| v.get("__s")).and_then(|s| s.as_array())
        .map(|a| a.iter().any(|m| m.as_str() == Some("ARCANES")))
        .unwrap_or(false);
    let (start_mana, end_mana, hp_warn, mana_warn) =
        simulate_mana_fast(rows, combo_base, has_transcendence, registry, tables, consts);
    if hp_warn { return false; }
    if mana_warn { return false; }
    if consts.allow_downtime { return end_mana > 0.0; }
    (start_mana - end_mana) <= 5.0
}

/// greedy_sp_loop (pure/engine.js): step-down [20,4,1], try-revert-keep.
pub fn greedy_sp_loop<F: FnMut(&[i32; 5]) -> f64>(
    base_sp: &mut [i32; 5], total_sp: &mut [i32; 5], mut remaining: i32,
    cap_total: &[i32; 5], mut trial_score: F,
) -> i32 {
    let mut allocated = 0;
    let mut cur = trial_score(total_sp);

    for step in [20, 4, 1] {
        let mut progress = true;
        while progress && remaining > 0 {
            progress = false;
            let mut best_i: i32 = -1;
            let mut best_s = cur;

            for i in 0..5 {
                let a = step.min(remaining).min(100 - base_sp[i]).min(cap_total[i] - total_sp[i]);
                if a <= 0 { continue; }
                total_sp[i] += a;
                let s = trial_score(total_sp);
                total_sp[i] -= a;
                if s > best_s { best_s = s; best_i = i as i32; }
            }

            if best_i >= 0 {
                let i = best_i as usize;
                let a = step.min(remaining).min(100 - base_sp[i]).min(cap_total[i] - total_sp[i]);
                base_sp[i] += a;
                total_sp[i] += a;
                remaining -= a;
                allocated += a;
                cur = best_s;
                progress = true;
            }
        }
    }
    allocated
}

/// greedy_sp_allocate without sp_floors (score fixtures are exported with
/// restrictions stripped, so floors are always null there).
pub fn greedy_sp_allocate<F: FnMut(&[i32; 5]) -> f64>(
    base_sp: &mut [i32; 5], total_sp: &mut [i32; 5], remaining: i32,
    cap_total: &[i32; 5], trial_score: F,
) -> i32 {
    if remaining <= 0 { return 0; }
    let mut any_room = false;
    for i in 0..5 {
        if base_sp[i] < 100 && total_sp[i] < 150 { any_room = true; break; }
    }
    if !any_room { return 0; }
    greedy_sp_loop(base_sp, total_sp, remaining, cap_total, trial_score)
}

/// The worker's full leaf pipeline for one locked build:
/// SP solve → greedy (damage-trial) → assemble → mana check (+rescue) → score.
/// Returns None when the leaf is infeasible (SP or mana).
pub struct LeafResult {
    pub base_sp: [i32; 5],
    pub total_sp: [i32; 5],
    pub assigned_sp: i32,
    pub score: f64,
}

pub enum LeafOutcome {
    SpInfeasible,
    /// SP-feasible but the all-150-SP damage ceiling cannot beat the cutoff
    /// (counts as feasible; greedy/mana/score skipped — same as the JS gate).
    Gated,
    ManaReject,
    Scored(LeafResult),
}

#[allow(clippy::too_many_arguments)]
pub fn leaf_pipeline(
    item_names: &[&str], l2: &Layer2, weapon: &Obj, guild: Option<&crate::Unit>,
    kernel: &mut crate::Kernel, rows: &[Row], registry: &[Value],
    hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables, consts: &L2Consts,
) -> Result<Option<LeafResult>, String> {
    match leaf_pipeline_gated(item_names, l2, weapon, guild, kernel, rows,
                              registry, hit_refs, tables, consts, None)? {
        LeafOutcome::Scored(r) => Ok(Some(r)),
        LeafOutcome::Gated => unreachable!("no cutoff passed"),
        _ => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn leaf_pipeline_gated(
    item_names: &[&str], l2: &Layer2, weapon: &Obj, guild: Option<&crate::Unit>,
    kernel: &mut crate::Kernel, rows: &[Row], registry: &[Value],
    hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables, consts: &L2Consts,
    gate_cutoff: Option<f64>,
) -> Result<LeafOutcome, String> {
    // Units in worker order (helmet-first _scratch_sp_input order).
    let unit_of = |sm: &Obj| -> crate::Unit {
        let arr5 = |k: &str| -> [i32; 5] {
            let mut out = [0i32; 5];
            if let Some(a) = sm.get(k).and_then(|v| v.as_array()) {
                for (i, x) in a.iter().take(5).enumerate() {
                    out[i] = x.as_f64().unwrap_or(0.0) as i32;
                }
            }
            out
        };
        crate::Unit {
            crafted: sm.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false),
            reqs: arr5("reqs"),
            skp: arr5("skillpoints"),
        }
    };
    let mut equipment: [crate::Unit; 8] = Default::default();
    let mut equips: Vec<&Obj> = Vec::new();
    for (i, name) in item_names.iter().enumerate().take(8) {
        let item = l2.item_registry.get(*name)
            .ok_or_else(|| format!("item not in registry: {}", name))?;
        equipment[i] = unit_of(item);
        equips.push(item);
    }

    // Set-bonus SP folded into the free pool (worker leaf behavior).
    let mut set_free = [0i32; 5];
    {
        let mut set_counts: Vec<(String, i64)> = Vec::new();
        for item in &equips {
            if item.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false) { continue; }
            let Some(set_name) = item.get("set").and_then(|v| v.as_str()) else { continue };
            match set_counts.iter_mut().find(|(n, _)| n == set_name) {
                Some((_, c)) => *c += 1,
                None => set_counts.push((set_name.to_string(), 1)),
            }
        }
        for (set_name, count) in &set_counts {
            let Some(set_data) = l2.sets_data.get(set_name) else { continue };
            let Some(bonus) = set_data.get("bonuses").and_then(|b| b.as_array())
                .and_then(|b| b.get((*count - 1) as usize)).and_then(|b| b.as_object()) else { continue };
            for (i, skp) in l2.skp_order.iter().enumerate() {
                if let Some(v) = bonus.get(skp).and_then(|x| x.as_f64()) {
                    set_free[i] += v as i32;
                }
            }
        }
    }

    let case = crate::Case {
        budget: consts.sp_budget,
        equipment,
        weapon: unit_of(weapon),
        set_free,
        expected: None,
    };
    let Some((assign, total, assigned)) = kernel.calculate_with_extra(&case, guild) else {
        return Ok(LeafOutcome::SpInfeasible);
    };
    let mut base_sp = assign;
    let mut total_sp = total;
    let mut assigned_sp = assigned;

    // Score-ceiling gate (mirrors the JS worker): one damage eval at
    // all-150 SP upper-bounds anything greedy can reach. Strict margin so
    // a float-ulp monotonicity wobble never gates a genuine candidate.
    if let Some(cutoff) = gate_cutoff {
        let ceiling_sp = [150f64; 5];
        let cb150 = l2.assemble(item_names, &ceiling_sp, weapon)?;
        let ceiling = eval_combo_damage(&cb150, weapon, rows, registry, hit_refs, tables);
        if ceiling < cutoff - cutoff.abs() * 1e-9 {
            return Ok(LeafOutcome::Gated);
        }
    }

    // Greedy allocation with damage trials (combo_damage target).
    let remaining = consts.sp_budget - assigned_sp;
    let cap_total = [150i32; 5];
    let orig_base_sp = base_sp;
    let mut trial_sp_f = [0f64; 5];
    let mut trial = |sp: &[i32; 5]| -> f64 {
        for i in 0..5 { trial_sp_f[i] = sp[i] as f64; }
        match l2.assemble(item_names, &trial_sp_f, weapon) {
            Ok(cb) => eval_combo_damage(&cb, weapon, rows, registry, hit_refs, tables),
            Err(_) => f64::NEG_INFINITY,
        }
    };
    assigned_sp += greedy_sp_allocate(&mut base_sp, &mut total_sp, remaining, &cap_total, &mut trial);

    // Final assemble + mana check (+ rescue).
    let sp_f: Vec<f64> = total_sp.iter().map(|&x| x as f64).collect();
    let mut combo_base = l2.assemble(item_names, &sp_f, weapon)?;
    if !mana_check_passes(rows, &combo_base, registry, tables, consts) {
        match mana_rescue(item_names, l2, weapon, &mut base_sp, &mut total_sp,
                          &orig_base_sp, rows, registry, hit_refs, tables, consts)? {
            Some(rescued) => combo_base = rescued,
            None => return Ok(LeafOutcome::ManaReject),
        }
    }

    let score = eval_combo_damage(&combo_base, weapon, rows, registry, hit_refs, tables);
    Ok(LeafOutcome::Scored(LeafResult { base_sp, total_sp, assigned_sp, score }))
}

/// _mana_rescue: shift freely-assigned SP into Int in increasing fractions.
#[allow(clippy::too_many_arguments)]
pub fn mana_rescue(
    item_names: &[&str], l2: &Layer2, weapon: &Obj,
    base_sp: &mut [i32; 5], total_sp: &mut [i32; 5], orig_base_sp: &[i32; 5],
    rows: &[Row], registry: &[Value],
    hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables, consts: &L2Consts,
) -> Result<Option<Obj>, String> {
    let _ = hit_refs;
    if consts.hp_casting { return Ok(None); }
    if consts.combo_time == 0.0 { return Ok(None); }

    const INT_IDX: usize = 2;
    let int_room = (100 - base_sp[INT_IDX]).min(150 - total_sp[INT_IDX]);
    if int_room <= 0 { return Ok(None); }

    let mut total_stealable = 0;
    let mut stealable = [0i32; 5];
    for i in 0..5 {
        if i == INT_IDX { continue; }
        stealable[i] = base_sp[i] - orig_base_sp[i];
        total_stealable += stealable[i];
    }
    if total_stealable <= 0 { return Ok(None); }
    let max_shift = total_stealable.min(int_room);
    if max_shift <= 0 { return Ok(None); }

    let saved_base = *base_sp;
    let saved_total = *total_sp;

    for frac in [0.25f64, 0.5, 0.75, 1.0] {
        let shift_target = (max_shift as f64 * frac).ceil() as i32;
        if shift_target <= 0 { continue; }

        *base_sp = saved_base;
        *total_sp = saved_total;

        // Steal from attributes with most free SP first (stable sort mirrors
        // JS Array.sort on 4 elements — insertion-order ties preserved).
        let mut order = [0usize, 1, 3, 4];
        order.sort_by(|&a, &b| stealable[b].cmp(&stealable[a]));

        let mut shifted = 0;
        for &i in &order {
            if shifted >= shift_target { break; }
            let take = stealable[i].min(shift_target - shifted);
            if take <= 0 { continue; }
            base_sp[i] -= take;
            total_sp[i] -= take;
            shifted += take;
        }
        base_sp[INT_IDX] += shifted;
        total_sp[INT_IDX] += shifted;

        let sp_f: Vec<f64> = total_sp.iter().map(|&x| x as f64).collect();
        let combo_base = l2.assemble(item_names, &sp_f, weapon)?;
        if mana_check_passes(rows, &combo_base, registry, tables, consts) {
            return Ok(Some(combo_base));
        }
    }

    *base_sp = saved_base;
    *total_sp = saved_total;
    Ok(None)
}


// ── Scenario scoring context (for the enum_kernel integration) ──────────────

/// Everything the leaf pipeline needs, loaded from a score-fixture JSON.
pub struct ScoringCtx {
    pub tables: Tables,
    pub weapon: Obj,
    pub rows: Vec<Row>,
    pub registry: Vec<Value>,
    pub hit_refs: HashMap<i64, HashMap<String, Obj>>,
    pub layer2: Layer2,
    pub consts: L2Consts,
    pub guild_unit: Option<crate::Unit>,
}

impl ScoringCtx {
    pub fn load(fixture: &Value) -> Result<ScoringCtx, String> {
        let layer2 = Layer2::parse(fixture).ok_or("missing/invalid layer2 data")?;
        if layer2.scaling_kind != "cached" && layer2.scaling_kind != "split" {
            return Err(format!("unsupported scaling plan: {}", layer2.scaling_kind));
        }
        let consts = L2Consts::parse(fixture).ok_or("missing layer2 constants")?;
        let mut hit_refs: HashMap<i64, HashMap<String, Obj>> = HashMap::new();
        if let Some(hr) = fixture["atree_hit_refs"].as_object() {
            for (bs, parts) in hr {
                let bs_num: i64 = bs.parse().unwrap_or(i64::MIN);
                let mut m = HashMap::new();
                if let Some(po) = parts.as_object() {
                    for (part_name, hits) in po {
                        if let Some(h) = hits.as_object() { m.insert(part_name.clone(), h.clone()); }
                    }
                }
                hit_refs.insert(bs_num, m);
            }
        }
        let guild_unit = fixture["layer2"].get("guild_tome_sm").and_then(as_map).map(|sm| {
            let arr5 = |k: &str| -> [i32; 5] {
                let mut out = [0i32; 5];
                if let Some(a) = sm.get(k).and_then(|v| v.as_array()) {
                    for (i, x) in a.iter().take(5).enumerate() {
                        out[i] = x.as_f64().unwrap_or(0.0) as i32;
                    }
                }
                out
            };
            crate::Unit {
                crafted: sm.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false),
                reqs: arr5("reqs"),
                skp: arr5("skillpoints"),
            }
        });
        Ok(ScoringCtx {
            tables: Tables::parse(&fixture["tables"]),
            weapon: as_map(&fixture["weapon_sm"]).ok_or("weapon_sm must be a map")?.clone(),
            rows: parse_rows(&fixture["parsed_combo"]),
            registry: fixture["boost_registry"].as_array().cloned().unwrap_or_default(),
            hit_refs,
            layer2,
            consts,
            guild_unit,
        })
    }
}
