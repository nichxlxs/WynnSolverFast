//! Combo-damage scoring pipeline (P2.4) — see PORT_PLAN.md.
//!
//! Faithful port of the JS worker's leaf pipeline: damage core (layer 1)
//! plus stat assembly, greedy SP, fast mana sim, and mana rescue (layer 2),
//! validated bit-exact by `bin/score_kernel.rs` against fixtures exported
//! with SOLVER_EXPORT_SCORE. JS numeric semantics are mirrored throughout
//! (missing-key NaN flow, insertion-order maps, NaN-propagating max,
//! half-up rounding). Healing is not ported (never affects total_damage).

use serde_json::Value;
use std::borrow::Cow;
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
    pub skillpoint_final_mult_3: f64,
    pub skillpoint_final_mult_4: f64,
    /// Precomputed per-element stat key names (hot-path format! hoist).
    pub names: ElemNames,
}

pub struct ElemNames {
    pub conv_base: [String; 6],
    pub dam_add_min: [String; 6],
    pub dam_add_max: [String; 6],
    pub sd_pct: [String; 6],
    pub md_pct: [String; 6],
    pub dam_pct: [String; 6],
    pub sd_raw: [String; 6],
    pub md_raw: [String; 6],
    pub dam_raw: [String; 6],
}

impl ElemNames {
    fn build() -> ElemNames {
        let mk = |suffix: &str| -> [String; 6] {
            std::array::from_fn(|i| format!("{}{}", DAMAGE_ELEMENTS[i], suffix))
        };
        ElemNames {
            conv_base: mk("ConvBase"),
            dam_add_min: mk("DamAddMin"),
            dam_add_max: mk("DamAddMax"),
            sd_pct: mk("SdPct"),
            md_pct: mk("MdPct"),
            dam_pct: mk("DamPct"),
            sd_raw: mk("SdRaw"),
            md_raw: mk("MdRaw"),
            dam_raw: mk("DamRaw"),
        }
    }
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
            skillpoint_final_mult_3: v.get("skillpoint_final_mult")
                .and_then(|a| a.get(3)).and_then(|x| x.as_f64()).unwrap_or(f64::NAN),
            skillpoint_final_mult_4: v.get("skillpoint_final_mult")
                .and_then(|a| a.get(4)).and_then(|x| x.as_f64()).unwrap_or(f64::NAN),
            names: ElemNames::build(),
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
    calculate_spell_damage_pc(stats, weapon, conversions_in, use_spell_damage,
        ignore_speed, part_filter, ignore_str, ignored_mults, tables, None)
}

#[allow(clippy::too_many_arguments)]
pub fn calculate_spell_damage_pc(
    stats: &StatsView, weapon: &Obj, conversions_in: &[f64], use_spell_damage: bool,
    ignore_speed: bool, part_filter: Option<&str>, ignore_str: bool,
    ignored_mults: &[String], tables: &Tables, part_conv: Option<&[String; 6]>,
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
    if let Some(pc) = part_conv {
        for (i, name) in pc.iter().enumerate() {
            if stats.has(name) { conversions[i] += stats.num(name); }
        }
    } else if let Some(pf) = part_filter {
        for (i, el) in DAMAGE_ELEMENTS.iter().enumerate() {
            let name = format!("{}ConvBase:{}", el, pf);
            if stats.has(&name) { conversions[i] += stats.num(&name); }
        }
    }
    for i in 0..6 {
        let name = &tables.names.conv_base[i];
        if stats.has(name) { conversions[i] += stats.num(name); }
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
    let names = &tables.names;
    for i in 0..6 {
        if present[i] {
            damages[i][0] += stats.num(&names.dam_add_min[i]);
            damages[i][1] += stats.num(&names.dam_add_max[i]);
        }
    }

    // 5. ID bonuses.
    let (spec_pct, spec_raw, spec_pct_name, spec_raw_name, r_pct_name, r_raw_name) =
        if use_spell_damage {
            (&names.sd_pct, &names.sd_raw, "sdPct", "sdRaw", "rSdPct", "rSdRaw")
        } else {
            (&names.md_pct, &names.md_raw, "mdPct", "mdRaw", "rMdPct", "rMdRaw")
        };
    let mut skill_boost = [0.0f64; 6];
    for (i, skp) in SKP_ORDER.iter().enumerate() {
        skill_boost[i + 1] = tables.sp_to_pct(stats.num(skp)) * tables.skillpoint_damage_mult[i];
    }
    let static_boost = (stats.num(spec_pct_name) + stats.num("damPct")) / 100.0;

    let mut total_min = 0.0;
    let mut total_max = 0.0;
    let mut save_prop = [[0.0f64; 2]; 6];
    let r_pct = (stats.num(r_pct_name) + stats.num("rDamPct")) / 100.0;
    for i in 0..6 {
        save_prop[i] = damages[i];
        total_min += damages[i][0];
        total_max += damages[i][1];

        let mut boost = 1.0 + skill_boost[i] + static_boost
            + (stats.num(&spec_pct[i]) + stats.num(&names.dam_pct[i])) / 100.0;
        if i > 0 {
            boost += r_pct;
        }
        damages[i][0] *= boost;
        damages[i][1] *= boost;
    }

    let total_elem_min = total_min - save_prop[0][0];
    let total_elem_max = total_max - save_prop[0][1];

    // 5.2 Raw application.
    let prop_raw = stats.num(spec_raw_name) + stats.num("damRaw");
    let rainbow_raw = stats.num(r_raw_name) + stats.num("rDamRaw");
    for i in 0..6 {
        let save_obj = save_prop[i];
        let mut raw_boost = 0.0;
        if present[i] {
            raw_boost += stats.num(&spec_raw[i]) + stats.num(&names.dam_raw[i]);
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

#[derive(Clone)]
pub struct Token {
    pub name: String,
    pub value: f64,
    pub is_pct: bool,
    /// User-set slider. `inject_blood_pact_boosts` will not override a
    /// manual token of the same name.
    pub manual: bool,
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
/// Borrows the spell untouched in the (dominant) no-override case.
pub fn apply_spell_prop_overrides<'s>(
    spell: &'s Value, prop_overrides: &HashMap<String, PropOverride>,
    hit_refs: &HashMap<i64, HashMap<String, Obj>>,
) -> std::borrow::Cow<'s, Value> {
    use std::borrow::Cow;
    if prop_overrides.is_empty() { return Cow::Borrowed(spell); }
    let base_spell = spell.get("base_spell").and_then(|v| v.as_i64()).unwrap_or(0);
    let Some(orig_part_hits) = hit_refs.get(&base_spell) else { return Cow::Borrowed(spell) };
    if orig_part_hits.is_empty() { return Cow::Borrowed(spell); }

    let mut needs_clone = false;
    'outer: for (_, orig_hits) in orig_part_hits {
        for orig_val in orig_hits.values() {
            if let Some(s) = orig_val.as_str() {
                if prop_overrides.contains_key(s) { needs_clone = true; break 'outer; }
            }
        }
    }
    if !needs_clone { return Cow::Borrowed(spell); }

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
    std::borrow::Cow::Owned(clone)
}

// ── compute_melee_time_hits (pure/utils.js) ──────────────────────────────────

/// `delay` mirrors the JS third argument: the two simulations pass the row's
/// effective delay, the damage path passes `SPELL_CAST_DELAY`. `None` means
/// the latter.
pub fn compute_melee_time_hits(
    qty_seconds: f64, base_stats: &StatsView, melee_cd_override: Option<f64>, tables: &Tables,
    delay: Option<f64>,
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
    melee_time_hits_from_period(qty_seconds, melee_period, delay)
}

/// The tail of `compute_melee_time_hits`, once the period is known.
///
/// A caller that has already derived the unmodified melee period from the
/// same atkSpd/atkTier pair can pass it straight in — the `None` branch
/// above recomputes exactly that expression, so feeding it here is the same
/// arithmetic without re-reading the two stats.
#[inline]
pub fn melee_time_hits_from_period(
    qty_seconds: f64, melee_period: f64, delay: Option<f64>,
) -> f64 {
    qty_seconds / melee_period.max(delay.unwrap_or(SPELL_CAST_DELAY))
}

// ── compute_combo_damage_totals (pure/engine.js) ─────────────────────────────

#[derive(Clone)]
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
    /// `row.loop_start` as (condition type, condition value). Only the
    /// stateful sim consumes this; the loop-free paths reject or unroll
    /// before they see it.
    pub loop_start: Option<(i64, f64)>,
    pub loop_end: bool,
    /// JS destructures `auto_delay = true`, i.e. absent means true.
    pub auto_delay: bool,
    /// `row.sim_qty` as set by `_parse_combo_for_search`. Only
    /// `compute_recast_penalties` reads it (the sim recomputes its own).
    pub sim_qty: f64,
    /// `row.recast_penalty_per_cast` — the per-cast average. Carried for
    /// parity with the JS row shape; the sim reads `recast_penalties`.
    pub recast_penalty_per_cast: f64,
    // Static spell fields hoisted for the mana sim (reads the ORIGINAL
    // spell, which is parse-time constant).
    pub sim_cost_present: bool,
    pub sim_melee_scaling: bool,
    pub sim_recast_base: i64,
    pub sim_hp_cost: f64,
    // Parse-time DPS analysis of the ORIGINAL spell — valid whenever no
    // prop override modified the spell for a given evaluation.
    pub static_dps: Option<(String, f64, String)>, // per_hit_name, max_hits, chain_root
    pub static_fallback_root: Option<String>,
}

/// JS `LOOP_COND_COUNT` — the loop repeats a fixed number of times.
pub const LOOP_COND_COUNT: i64 = 0;
/// JS `LOOP_SAFETY_CAP`.
pub const LOOP_SAFETY_CAP: i64 = 255;

/// Unroll count-type loop brackets, mirroring `_unroll_loops_pure`.
///
/// A count loop's iteration count is a static constant, so unrolling is
/// exact and independent of the simulation: the body is emitted N times
/// and the markers dropped, leaving an ordinary combo that the validated
/// loop-free pipeline handles bit-for-bit. `until-OOM` loops need the sim's
/// feedback (iterations depend on when mana runs out) and are NOT unrolled
/// — callers still reject those.
pub fn unroll_count_loops(rows: &[Value]) -> Result<Vec<Value>, String> {
    let is_start = |r: &Value| r.get("loop_start").map(|v| !v.is_null()).unwrap_or(false);
    let is_end = |r: &Value| r.get("loop_end").map(|v| !v.is_null()).unwrap_or(false);
    let mut out: Vec<Value> = Vec::with_capacity(rows.len());
    let mut i = 0usize;
    while i < rows.len() {
        let r = &rows[i];
        if is_start(r) {
            let Some(end) = (i + 1..rows.len()).find(|&j| is_end(&rows[j])) else {
                // Unterminated marker: JS skips it.
                i += 1;
                continue;
            };
            let cond = r.get("loop_start").unwrap();
            let ty = cond.get("type").and_then(|v| v.as_i64()).unwrap_or(-1);
            if ty != LOOP_COND_COUNT {
                return Err("until-OOM loop brackets not supported (iteration count \
                            depends on the mana simulation)".into());
            }
            let iters = cond.get("value").and_then(|v| v.as_i64()).unwrap_or(1).max(1)
                .min(LOOP_SAFETY_CAP);
            let body: Vec<&Value> = (i + 1..end)
                .map(|j| &rows[j])
                .filter(|b| !is_start(b) && !is_end(b))
                .collect();
            for _ in 0..iters {
                for b in &body { out.push((*b).clone()); }
            }
            i = end + 1;
        } else if is_end(r) {
            i += 1;
        } else {
            out.push(r.clone());
            i += 1;
        }
    }
    Ok(out)
}

pub fn parse_rows(v: &Value) -> Vec<Row> {
    let mut rows = Vec::new();
    for r in v.as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let tokens = r.get("boost_tokens").and_then(|t| t.as_array())
            .map(|a| a.iter().map(|t| Token {
                name: t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                value: t.get("value").and_then(|x| x.as_f64()).unwrap_or(f64::NAN),
                is_pct: t.get("is_pct").and_then(|x| x.as_bool()).unwrap_or(false),
                manual: t.get("manual").and_then(|x| x.as_bool()).unwrap_or(false),
            }).collect())
            .unwrap_or_default();
        let spell_ref: Option<Value> = r.get("spell").filter(|s| !s.is_null()).cloned();
        let sim_cost_present = spell_ref.as_ref()
            .and_then(|sp| sp.get("cost")).map(|c| !c.is_null()).unwrap_or(false);
        let sim_melee_scaling = spell_ref.as_ref()
            .and_then(|sp| sp.get("scaling")).and_then(|s| s.as_str()) == Some("melee");
        let sim_recast_base = spell_ref.as_ref().and_then(|sp| {
            sp.get("mana_derived_from").and_then(|v| v.as_i64())
                .or_else(|| sp.get("base_spell").and_then(|v| v.as_i64()))
        }).unwrap_or(0);
        let sim_hp_cost = spell_ref.as_ref()
            .and_then(|sp| sp.get("hp_cost")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        rows.push(Row {
            sim_cost_present, sim_melee_scaling, sim_recast_base, sim_hp_cost,
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
            spell: spell_ref.clone(),
            mana_excl: r.get("mana_excl").and_then(|x| x.as_bool()).unwrap_or(false),
            cast_time: r.get("cast_time").and_then(|x| x.as_f64()),
            delay: r.get("delay").and_then(|x| x.as_f64()),
            recast_penalties: r.get("recast_penalties").map(arr_f64).unwrap_or_default(),
            has_loop_marker: r.get("loop_start").map(|v| !v.is_null()).unwrap_or(false)
                || r.get("loop_end").map(|v| !v.is_null()).unwrap_or(false),
            loop_start: r.get("loop_start").filter(|v| !v.is_null()).map(|c| (
                c.get("type").and_then(|v| v.as_i64()).unwrap_or(-1),
                // JS `condition.value || 1` — absent/0/NaN all fall back to 1.
                c.get("value").and_then(|v| v.as_f64()).filter(|v| *v != 0.0 && !v.is_nan())
                    .unwrap_or(1.0),
            )),
            loop_end: r.get("loop_end").map(|v| !v.is_null()).unwrap_or(false),
            auto_delay: r.get("auto_delay").and_then(|v| v.as_bool()).unwrap_or(true),
            sim_qty: r.get("sim_qty").and_then(|v| v.as_f64()).unwrap_or(f64::NAN),
            recast_penalty_per_cast: r.get("recast_penalty_per_cast")
                .and_then(|v| v.as_f64()).unwrap_or(0.0),
            static_dps: spell_ref.as_ref()
                .and_then(|s| compute_dps_spell_hits_info(s))
                .map(|i| (i.per_hit_name, i.max_hits, i.dps_chain_root)),
            static_fallback_root: spell_ref.as_ref().and_then(|s| {
                if spell_is_dps(s) { find_dps_display_root(s) } else { None }
            }),
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
        let mod_spell_cow = apply_spell_prop_overrides(spell, &prop_overrides, hit_refs);
        // Borrowed = no prop overrides fired, so the row's precomputed DPS
        // analysis (done on the original spell at parse time) applies.
        let is_unmodified = matches!(mod_spell_cow, std::borrow::Cow::Borrowed(_));
        let mod_spell: &Value = mod_spell_cow.as_ref();

        let mut eff_dps_name: Option<&str> = row.dps_per_hit_name.as_deref();
        let mut eff_dps_hits = row.dps_hits;
        let mut chain_root: Option<&str> = None;
        let dps_info_owned;
        if eff_dps_name.is_none() {
            if is_unmodified {
                if let Some((name, hits, root)) = &row.static_dps {
                    eff_dps_name = Some(name);
                    eff_dps_hits = row.dps_hits_override.unwrap_or(*hits);
                    chain_root = Some(root);
                }
            } else {
                dps_info_owned = compute_dps_spell_hits_info(mod_spell);
                if let Some(info) = &dps_info_owned {
                    eff_dps_name = Some(&info.per_hit_name);
                    eff_dps_hits = row.dps_hits_override.unwrap_or(info.max_hits);
                    chain_root = Some(&info.dps_chain_root);
                }
            }
        }

        let per_cast = match eff_dps_name {
            Some(name) => compute_spell_display_avg(&stats, weapon, mod_spell, crit, tables, Some(name)) * eff_dps_hits,
            None => compute_spell_display_avg(&stats, weapon, mod_spell, crit, tables, None),
        };

        let mut flat_per_cast = 0.0;
        let fallback_owned;
        let final_root: Option<&str> = if chain_root.is_some() {
            chain_root
        } else if is_unmodified {
            row.static_fallback_root.as_deref()
        } else if spell_is_dps(mod_spell) {
            fallback_owned = find_dps_display_root(mod_spell);
            fallback_owned.as_deref()
        } else {
            None
        };
        if let Some(root) = final_root {
            flat_per_cast = compute_spell_flat_damage(&stats, weapon, mod_spell, crit, root, tables);
        }

        let eff_qty = if row.is_melee_time {
            compute_melee_time_hits(row.qty, &base_view, row.melee_cd_override, tables, None)
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

/// `atree_eval_stat_effects` (js/solver/pure/utils.js:163) over the lowered
/// var-effect list.
///
/// Outputs go through `merge_stat`, so dotted keys land in the nested
/// `damMult`/`defMult`/`healMult`/`manaMult` maps with the non-stacking
/// sub-keys taking the max — exactly as the JS does. The dense lowering
/// declines any var effect writing such a key (`nested_prefix` in
/// `DenseCtx::build`), so those scenarios evaluate here.
pub fn eval_var_effects(var_effects: &[Value], pre_scale: &Obj) -> Obj {
    let mut out = Obj::new();
    for eff in var_effects {
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
    out
}

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

/// Load-time-resolved per-item inputs to the leaf prologue.
#[derive(Clone, Default)]
pub struct ItemPrelude {
    pub unit: crate::Unit,
    /// The set this item counts toward, or None when it is crafted or
    /// belongs to no set -- the two cases the prologue skips.
    pub set_name: Option<String>,
}

pub struct Layer2 {
    /// True when no atree var effect outputs a mana-relevant stat, i.e. the
    /// fast mana sim is monotone in Int and the Int=150 doom precheck is
    /// admissible (see leaf_pipeline_gated).
    pub mana_doom_ok: bool,
    /// Mirrors the worker's _ceiling_gate_setup var-effect conditions: every
    /// atree var effect must have non-negative stat-input factors and only
    /// plain stat outputs that are neither atkTier nor *ConvBase. Without
    /// this, evaluating at all-150 SP is not an upper bound and the ceiling
    /// gate / subtree bounds would prune inadmissibly.
    pub ceiling_vars_ok: bool,
    pub item_registry: HashMap<String, Obj>,
    /// Everything the per-leaf prologue used to dig back out of an item's
    /// JSON: its SP Unit, and its set name when it contributes one.
    ///
    /// The prologue ran this for all eight items on every single leaf --
    /// three map lookups and two five-element array walks for the Unit, two
    /// more lookups for the set pool, and a String allocation per distinct
    /// set. On the ehp benchmark that came to 29 of 81 CPU-seconds, more
    /// than any real phase, to recompute values fixed at load time. Filled
    /// by exactly the code the prologue used, so the answers are the same
    /// ones by construction.
    pub item_prelude: HashMap<String, ItemPrelude>,
    /// Set name -> per-(count-1) skill-point bonus, in `skp_order`. Same
    /// reason as `item_prelude`: the prologue walked the sets JSON on every
    /// leaf (a lookup for the set, one for its bonus list, one per index,
    /// then five for the attributes) to read numbers fixed at load time.
    pub set_bonus_sp: HashMap<String, Vec<[i32; 5]>>,
    pub sets_data: Obj,
    pub tome_sms: Vec<Obj>,
    /// Tome optimisation (0 = off, 1 = guild, 2 = guild + weapon/armour).
    /// When non-zero the tome is a SEARCHED dimension and `tome_sms` /
    /// `guild_tome_sm` describe only the currently fixed choice, which the
    /// leaf loop replaces with each candidate.
    pub tome_opt: i64,
    /// Guild tome choices: the "none" option plus each owned guild tome.
    /// Each carries requirements and skill points, so each needs its own SP
    /// solve rather than just a different stat sum.
    pub guild_tome_cands: Vec<TomeCand>,
    /// Pareto front of weapon+armour tome bundles. `stats` is a flat additive
    /// sum over the bundle's rolled IDs; it may include `damMobs`/`defMobs`,
    /// which reach the damMult/defMult "tome" entries through the ordinary
    /// dense-indexed slots (see `dense_apply`), so a bundle lowers exactly
    /// like any other item.
    pub tome_wa_bundles: Vec<TomeBundle>,
    /// Per-key maximum across every bundle, for the ge-prechecks.
    pub tome_bound: Obj,
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
    /// Radiance major-ID scaling: `boost` multiplies each affected stat and
    /// floors it (positive values, or negative ones for the reversed IDs).
    /// 1.0 = inactive.
    pub radiance_boost: f64,
    pub radiance_affected: Vec<String>,
    pub radiance_reversed: Vec<String>,
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
        let strvec_early = |v: &Value| -> Vec<String> {
            v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };
        // Same reads the prologue used to do per leaf, done once per item.
        let mut item_prelude = HashMap::new();
        for (name, sm) in &item_registry {
            let arr5 = |k: &str| -> [i32; 5] {
                let mut out = [0i32; 5];
                if let Some(a) = sm.get(k).and_then(|v| v.as_array()) {
                    for (i, x) in a.iter().take(5).enumerate() {
                        out[i] = x.as_f64().unwrap_or(0.0) as i32;
                    }
                }
                out
            };
            let crafted = sm.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false);
            item_prelude.insert(name.clone(), ItemPrelude {
                unit: crate::Unit { crafted, reqs: arr5("reqs"), skp: arr5("skillpoints") },
                set_name: if crafted { None } else {
                    sm.get("set").and_then(|v| v.as_str()).map(String::from)
                },
            });
        }
        // Set skill-point bonuses, resolved once. Mirrors the prologue's walk
        // exactly, including its skips: a bonus entry that is not an object,
        // or an attribute the entry omits, contributes nothing.
        let skp_order_v: Vec<String> = strvec_early(consts.get("skp_order")?);
        let sets_data_v: Obj = l2.get("sets_data").and_then(as_map).cloned().unwrap_or_default();
        let mut set_bonus_sp: HashMap<String, Vec<[i32; 5]>> = HashMap::new();
        for (set_name, set_data) in &sets_data_v {
            let Some(list) = set_data.get("bonuses").and_then(|b| b.as_array()) else { continue };
            let per_count = list.iter().map(|entry| {
                let mut out = [0i32; 5];
                if let Some(o) = entry.as_object() {
                    for (i, skp) in skp_order_v.iter().enumerate().take(5) {
                        if let Some(v) = o.get(skp).and_then(|x| x.as_f64()) { out[i] = v as i32; }
                    }
                }
                out
            }).collect();
            set_bonus_sp.insert(set_name.clone(), per_count);
        }
        let plan = l2.get("scaling_plan")?;
        let strvec = |v: &Value| -> Vec<String> {
            v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
        };
        let mana_relevant = |k: &str| -> bool {
            matches!(k, "mr" | "ms" | "maxMana" | "hp" | "hpBonus" | "atkTier" | "int")
                || k.starts_with("spRaw") || k.starts_with("spPct")
        };
        let var_effects_list = plan.get("var_effects").and_then(|x| x.as_array())
            .cloned().unwrap_or_default();
        let mana_doom_ok = var_effects_list.iter().all(|eff| {
            eff.get("outputs").and_then(|o| o.as_array()).map(|outs| {
                outs.iter().all(|o| o.as_str().map(|k| !mana_relevant(k)).unwrap_or(false))
            }).unwrap_or(false)
        });
        let ceiling_vars_ok = var_effects_list.iter().all(|eff| {
            let terms_ok = eff.get("terms").and_then(|t| t.as_array())
                .map(|ts| ts.iter().all(|t| {
                    t.get("factor").and_then(|f| f.as_f64()).map(|f| f >= 0.0).unwrap_or(false)
                }))
                .unwrap_or(true);
            let outs_ok = eff.get("outputs").and_then(|o| o.as_array())
                .map(|outs| outs.iter().all(|o| {
                    o.as_str().map(|n| n != "atkTier" && !n.contains("ConvBase")).unwrap_or(false)
                }))
                .unwrap_or(true);
            terms_ok && outs_ok
        });
        Some(Layer2 {
            mana_doom_ok,
            ceiling_vars_ok,
            item_registry, item_prelude, set_bonus_sp,
            sets_data: l2.get("sets_data").and_then(as_map).cloned().unwrap_or_default(),
            tome_sms: l2.get("tome_sms").and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|t| as_map(t).cloned()).collect()).unwrap_or_default(),
            tome_opt: l2.get("tome_opt").and_then(|v| v.as_i64()).unwrap_or(0),
            guild_tome_cands: parse_tome_cands(l2.get("guild_tome_candidates")),
            tome_wa_bundles: parse_tome_bundles(l2.get("tome_wa_bundles")),
            tome_bound: l2.get("tome_bound").and_then(as_map).cloned().unwrap_or_default(),
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
            radiance_boost: l2.get("radiance_boost").and_then(|v| v.as_f64()).unwrap_or(1.0),
            radiance_affected: consts.get("radiance_affected").map(strvec).unwrap_or_default(),
            radiance_reversed: consts.get("reversed_ids").map(strvec).unwrap_or_default(),
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
    /// Convenience: full leaf assembly (build stats + SP-dependent parts).
    pub fn assemble(&self, item_names: &[&str], total_sp: &[f64], weapon: &Obj) -> Result<Obj, String> {
        let base = self.build_base(item_names, weapon)?;
        Ok(self.assemble_from_base(&base, total_sp, weapon))
    }

    /// Leaf-invariant build stats: base statmap + item/tome/weapon sums +
    /// set bonuses + finalize. Compute ONCE per leaf; every greedy trial and
    /// the ceiling gate then reuse it through assemble_from_base — the same
    /// split the JS worker gets from its incremental running stats.
    pub fn build_base(&self, item_names: &[&str], weapon: &Obj) -> Result<Obj, String> {
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
        Ok(sm)
    }

    /// _apply_radiance_scale_inplace (pure/utils.js).
    pub fn apply_radiance(&self, sm: &mut Obj) {
        if self.radiance_boost == 1.0 { return; }
        for id in &self.radiance_affected {
            let Some(val) = sm.get(id).and_then(|v| v.as_f64()) else { continue };
            let reversed = self.radiance_reversed.iter().any(|r| r == id);
            let scale = if reversed { val < 0.0 } else { val > 0.0 };
            if scale {
                sm.insert(id.clone(), Value::from((val * self.radiance_boost).floor()));
            }
        }
    }

    /// SP-dependent assembly on a prebuilt base: clone + skp + classDef +
    /// atree_raw merge + atree scaling (cached/split) + static boosts.
    pub fn assemble_from_base(&self, base: &Obj, total_sp: &[f64], weapon: &Obj) -> Obj {
        self.assemble_from_base_extra(base, total_sp, weapon, None)
    }

    /// `assemble_from_base` with a tome bundle merged last.
    ///
    /// Mirrors the JS `assemble_combo_stats(..., extra_stats)`, which merges
    /// the bundle after `static_boosts` with `_merge_into` (plain-key merge is
    /// additive), because a tome bundle is a plain additive stat sum with no
    /// requirements of its own. It is NOT summed at the item stage: the JS
    /// applies it at the end of assembly, and `merge_into` and `add_item`
    /// differ on how a previous non-numeric value reads.
    ///
    /// The guild tome deliberately contributes nothing here — it reaches the
    /// build only through the SP solve (`_scratch_all_equip` in the JS worker
    /// is 8 equips + tome_sms + weapon, with `guild_tome_sm` excluded), so a
    /// guild candidate changes `base_sp`/`total_sp` and nothing else.
    pub fn assemble_from_base_extra(
        &self, base: &Obj, total_sp: &[f64], weapon: &Obj, extra: Option<&Obj>,
    ) -> Obj {
        // assemble_combo_stats: pre_scale = clone + skp + classDef + atree_raw
        let mut pre_scale = base.clone();
        for (i, skp) in self.skp_order.iter().enumerate() {
            pre_scale.insert(skp.clone(), Value::from(total_sp[i]));
        }
        if let Some(wt) = weapon.get("type").and_then(|v| v.as_str()) {
            let cd = self.class_def.get(wt).copied().unwrap_or(1.0);
            pre_scale.insert("classDef".into(), Value::from(cd));
        }
        merge_into(&mut pre_scale, self.atree_raw.as_ref());
        // _apply_radiance_scale_inplace: scale-and-floor the affected stats.
        self.apply_radiance(&mut pre_scale);

        let mut var_out: Option<Obj> = None;
        let scaled: Option<&Obj> = match self.scaling_kind.as_str() {
            "cached" => self.scaled_cached.as_ref(),
            "split" => {
                var_out = Some(eval_var_effects(&self.var_effects, &pre_scale));
                self.const_scaled.as_ref()
            }
            // Callers are gated on scaling_kind at load (ScoringCtx::load /
            // the validator's layer2_supported check).
            other => unreachable!("unsupported scaling plan: {}", other),
        };

        let mut combo_base = pre_scale;
        merge_into(&mut combo_base, scaled);
        merge_into(&mut combo_base, var_out.as_ref());
        merge_into(&mut combo_base, self.static_boosts.as_ref());
        merge_into(&mut combo_base, extra);
        combo_base
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
    compiled: Option<&[CompiledRow]>,
) -> (f64, f64, bool, bool) {
    simulate_mana_fast_ff(rows, combo_base, has_transcendence, registry, tables, consts, compiled, false)
}

/// simulate_mana_fast with an optional fail-fast: when `fail_fast` is set
/// and a warning fires, the sim returns immediately — mana_check_passes
/// fails on any warning regardless of the remaining trajectory, so the
/// verdict is identical and the rest of the combo is skipped.
///
/// Fail-fast on a *mana* warning is only valid when the combo has no
/// until-OOM loop: there, running out of mana is the loop terminator rather
/// than a failure, so the caller passes `fail_fast = false`.
#[allow(clippy::too_many_arguments)]
pub fn simulate_mana_fast_ff(
    rows: &[Row], combo_base: &Obj, has_transcendence: bool,
    registry: &[Value], tables: &Tables, consts: &L2Consts,
    compiled: Option<&[CompiledRow]>, fail_fast: bool,
) -> (f64, f64, bool, bool) {
    simulate_mana_fast_src(ManaSrc::Obj(combo_base), rows, has_transcendence,
                           registry, tables, consts, compiled, fail_fast)
}

/// Where the fast mana sim reads its stats from.
///
/// The simulation touches a small, fixed set of numbers: seven scalars up
/// front (mr, ms, maxMana, int, hp, hpBonus, atkTier) plus the weapon's
/// attack speed, and then three cost stats per spell row. `Obj` fetches
/// those by string key out of a map; `Dense` reads them straight out of the
/// assembled dense vector by precomputed slot.
///
/// This exists purely to delete work. Materializing the map cost a fresh
/// allocation and a String clone per key per leaf, and every read back out
/// of it hashed a string — on the ehp benchmark, whose combo is 33 rows,
/// that came to roughly 140 hashed lookups per simulation, several million
/// times over. The `Dense` arm does none of it. Both arms feed the *same*
/// simulation body below, so there is one implementation of the physics and
/// no way for the two to drift.
pub enum ManaSrc<'a> {
    Obj(&'a Obj),
    Dense(&'a ManaIdx, &'a DScratch),
}

/// The fast mana sim's stat reads, resolved to dense slots once per
/// scenario. `ok` is false when any read could not be lowered, in which
/// case callers must keep using the map.
#[derive(Default, Clone)]
pub struct ManaIdx {
    pub ok: bool,
    pub mr: u32, pub ms: u32, pub max_mana: u32, pub int: u32,
    pub hp: u32, pub hp_bonus: u32, pub atk_tier: u32,
    /// The weapon's attack-speed index — scenario-constant.
    pub atk_spd: i64,
    /// Per row: the (spRaw, spPct, spPctFinal) slots and the spell's base
    /// cost, or None for a row the sim never charges.
    pub row_cost: Vec<Option<([u32; 3], f64)>>,
}

impl<'a> ManaSrc<'a> {
    /// `StatsView::num_or0` semantics over a dense slot: a slot the leaf
    /// never wrote is absent (the map would not contain the key), and a NaN
    /// is what the map stores as Null. Both read as 0.
    #[inline]
    fn dense_num(s: &DScratch, i: u32) -> f64 {
        if !s.has(i) { return 0.0; }
        let v = s.vals[i as usize];
        if v.is_nan() { 0.0 } else { v }
    }
}

#[allow(clippy::too_many_arguments)]
fn simulate_mana_fast_src(
    src: ManaSrc, rows: &[Row], has_transcendence: bool,
    registry: &[Value], tables: &Tables, consts: &L2Consts,
    compiled: Option<&[CompiledRow]>, fail_fast: bool,
) -> (f64, f64, bool, bool) {
    use crate::mana_sim::{compute_drain_override, loop_condition_met};

    let hc = &consts.health;
    // Eight scalars, fetched once. The dense arm resolves them by slot; the
    // map arm by string key. Identical values, identical order.
    let (mr, ms, item_mana, int_v, base_hp, hp_bonus, spd_idx, atk_tier) = match &src {
        ManaSrc::Obj(o) => {
            let stats = StatsView::Borrowed(o);
            (stats.num_or0("mr"), stats.num_or0("ms"), stats.num_or0("maxMana"),
             stats.num_or0("int"), stats.num_or0("hp"), stats.num_or0("hpBonus"),
             tables.atk_spd_index(stats.str_of("atkSpd")), stats.num_or0("atkTier"))
        }
        ManaSrc::Dense(m, s) => {
            let n = |i: u32| ManaSrc::dense_num(s, i);
            (n(m.mr), n(m.ms), n(m.max_mana), n(m.int), n(m.hp), n(m.hp_bonus),
             m.atk_spd, n(m.atk_tier))
        }
    };
    let int_mana = (tables.sp_to_pct(int_v) * 100.0).floor();
    let start_mana = 100.0 + item_mana + int_mana;
    let max_mana = start_mana;
    let mut mana_wasted = 0.0;
    let mut total_mana_drain = 0.0;

    let health_cost_pct = hc.health_cost;
    let max_hp = js_max(5.0, base_hp + hp_bonus);

    let mr_per_sec = (mr + consts.base_mana_regen) / consts.mana_tick_seconds;

    let mut adj = spd_idx as f64 + atk_tier;
    if adj < 0.0 { adj = 0.0; }
    if adj > 6.0 { adj = 6.0; }
    let ms_per_hit = if ms != 0.0 { ms / 3.0 / tables.base_damage_multiplier[adj as usize] } else { 0.0 };
    let melee_period = 1.0 / tables.base_damage_multiplier[adj as usize];
    let mut melee_cd_remaining = 0.0f64;

    let mut mana = start_mana;
    let mut hp = max_hp;
    let mut elapsed_time = 0.0f64;
    let mut has_hp_warning = false;
    let mut has_mana_warning = false;

    // Minimal per-state tracking: unlike the full sim this keeps no state
    // *value* and fires no exit triggers — the fast sim only needs the
    // mana/HP trajectory, not the slider values the damage pass would want.
    let n_states = hc.buff_states.len();
    let mut st_active: Vec<bool> = vec![false; n_states];
    let mut st_at: Vec<f64> = vec![0.0; n_states];
    let no_buff_states = n_states == 0;

    let mut loop_body_start: i64 = -1;
    let mut loop_condition: Option<(i64, f64)> = None;
    let mut loop_iteration: i64 = 0;
    let mut loop_mana_warn = false;
    let mut loop_hp_warn = false;

    // _advance_time_fast. Note there is no HP-regen tick here: the JS fast
    // sim omits it too (the full sim has it), so mirroring means omitting it.
    macro_rules! advance {
        ($dt:expr) => {{
            let advance_dt: f64 = $dt;
            if advance_dt > 0.0 {
                let prev_time = elapsed_time;
                elapsed_time += advance_dt;
                let mut mana_regen_dt = advance_dt;

                if !no_buff_states {
                    for (bi, bs) in hc.buff_states.iter().enumerate() {
                        if !st_active[bi] { continue; }
                        let mut active_dt = advance_dt;
                        if let Some(duration) = bs.duration {
                            let remaining = duration - (prev_time - st_at[bi]);
                            if remaining <= 0.0 { st_active[bi] = false; continue; }
                            active_dt = js_min(advance_dt, remaining);
                            if active_dt < advance_dt { st_active[bi] = false; }
                        }
                        if bs.suppress_mana_regen {
                            mana_regen_dt = js_min(mana_regen_dt, advance_dt - active_dt);
                        }
                        if !bs.compute_delay {
                            if let Some(drain) = &bs.drain_pct_per_second {
                                if drain.mana > 0.0 {
                                    let d = drain.mana / 100.0 * max_mana * active_dt;
                                    let actual = js_min(mana, d);
                                    mana -= actual;
                                    total_mana_drain += actual;
                                }
                            }
                        }
                    }
                }

                if mana_regen_dt > 0.0 {
                    let uncapped = mana + mr_per_sec * mana_regen_dt;
                    if uncapped > max_mana { mana_wasted += uncapped - max_mana; }
                    mana = js_min(max_mana, uncapped);
                }
            }
        }};
    }
    let _ = mana_wasted;
    let _ = total_mana_drain;

    let mut ri: i64 = 0;
    while (ri as usize) < rows.len() {
        let row = &rows[ri as usize];

        if let Some(cond) = row.loop_start {
            if loop_condition.is_none() {
                loop_body_start = ri + 1;
                loop_condition = Some(cond);
                loop_iteration = 0;
                loop_mana_warn = false;
                loop_hp_warn = false;
            }
            ri += 1;
            continue;
        }
        if row.loop_end {
            if let Some((ct, cv)) = loop_condition {
                loop_iteration += 1;
                if !loop_condition_met(ct, cv, loop_iteration, loop_mana_warn, loop_hp_warn) {
                    ri = loop_body_start;
                    loop_mana_warn = false;
                    loop_hp_warn = false;
                    continue;
                }
                loop_condition = None;
            }
            ri += 1;
            continue;
        }

        // Add Flat Mana: inject (or drain) qty mana at this point.
        if row.pseudo_kind.as_deref() == Some("add_flat_mana") {
            if !row.mana_excl && row.qty != 0.0 {
                let uncapped = mana + row.qty;
                if uncapped > max_mana { mana_wasted += uncapped - max_mana; }
                mana = js_max(0.0, js_min(max_mana, uncapped));
            }
            ri += 1;
            continue;
        }
        if row.pseudo || row.qty <= 0.0 || row.spell.is_none() || row.mana_excl {
            ri += 1;
            continue;
        }
        let spell = row.spell.as_ref().unwrap();

        let is_spell = row.sim_cost_present;
        let unclamped_cost = if is_spell {
            match (&src, compiled) {
                // The dense arm is only ever built when every row's cost
                // lowered, so a missing entry here would be a construction
                // bug rather than a shape to tolerate.
                (ManaSrc::Dense(m, s), Some(c)) => {
                    let (slots, spell_cost) = m.row_cost[ri as usize]
                        .expect("dense mana row cost");
                    let v = [int_v,
                             ManaSrc::dense_num(s, slots[0]),
                             ManaSrc::dense_num(s, slots[1]),
                             ManaSrc::dense_num(s, slots[2])];
                    row_cost_from_inputs(v, &c[ri as usize], spell_cost, tables, consts)
                }
                (ManaSrc::Obj(o), Some(c)) =>
                    row_cost_compiled(o, spell, &c[ri as usize], tables, consts),
                (ManaSrc::Obj(o), None) =>
                    row_unclamped_spell_cost(o, spell, &row.tokens, registry, tables,
                                             consts.skillpoint_final_mult_2),
                (ManaSrc::Dense(..), None) => unreachable!("dense mana without compiled rows"),
            }
        } else { 0.0 };
        let is_melee_scaling = row.sim_melee_scaling;
        let is_melee = row.sim_recast_base == 0;

        let eff_cast_time = if is_melee { 0.0 } else { row.cast_time.unwrap_or(consts.spell_cast_time) };
        let eff_delay = row.delay.unwrap_or(consts.spell_cast_delay);
        let sim_qty = if row.is_melee_time {
            // `melee_period` above is exactly what compute_melee_time_hits
            // derives for a row with no override, off the same atkSpd/atkTier.
            js_round(melee_time_hits_from_period(
                row.qty, row.melee_cd_override.unwrap_or(melee_period), Some(eff_delay)))
        } else {
            js_round(row.qty)
        } as i64;
        let eff_melee_period = row.melee_cd_override.unwrap_or(melee_period);
        let spell_hp_cost = row.sim_hp_cost;
        let mut fast_post_override: Option<f64> = None;

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
                mana = js_max(0.0, js_min(max_mana, uncapped));
            }

            // "next_action" deactivation (Vanish). The fast sim only clears
            // the flag — no exit triggers, no state value.
            if c == 0 && (is_spell || is_melee_scaling) {
                for (bi, bs) in hc.buff_states.iter().enumerate() {
                    if bs.deactivate_next_action && st_active[bi] { st_active[bi] = false; }
                }
            }

            // Spell-level HP cost. Unconditional here; the full sim gates it
            // on a buff state being active. Mirroring means keeping both.
            if spell_hp_cost > 0.0 {
                let hp_deduction = spell_hp_cost / 100.0 * max_hp;
                if hp < hp_deduction {
                    has_hp_warning = true;
                    if fail_fast { return (start_mana, mana, true, has_mana_warning); }
                }
                hp -= hp_deduction;
            }

            if is_spell {
                let penalty = row.recast_penalties.get(c as usize).copied().unwrap_or(0.0);
                let effective_cost = js_max(1.0, unclamped_cost + penalty);
                let adj_cost = if has_transcendence { effective_cost * 0.75 } else { effective_cost };

                if mana >= effective_cost {
                    mana -= adj_cost;
                } else if health_cost_pct > 0.0 {
                    // Blood Pact: pay the remainder from health.
                    let remaining_mana = js_max(0.0, mana);
                    let health_mana = adj_cost - remaining_mana;
                    mana = 0.0;
                    let hp_cost = health_mana * health_cost_pct * max_hp / 100.0;
                    if hp < hp_cost {
                        has_hp_warning = true;
                        if fail_fast { return (start_mana, mana, true, has_mana_warning); }
                    }
                    hp -= hp_cost;
                } else {
                    mana -= effective_cost;
                    has_mana_warning = true;
                    if fail_fast { return (start_mana, mana, has_hp_warning, true); }
                }

                // State activation.
                for (bi, bs) in hc.buff_states.iter().enumerate() {
                    let Some(activate_spell) = bs.activate_on_spell else { continue };
                    let base_spell = spell.get("base_spell").and_then(|v| v.as_i64())
                        .unwrap_or(i64::MIN);
                    if base_spell != activate_spell || st_active[bi] { continue; }
                    st_active[bi] = true;
                    st_at[bi] = elapsed_time;
                    let dro = compute_drain_override(
                        bs, mana, max_mana, hp, max_hp,
                        if row.auto_delay { None } else { Some(post_dt) },
                    );
                    if let Some(dro) = dro {
                        if row.auto_delay && fast_post_override.is_none() {
                            fast_post_override = Some(dro.computed_delay);
                        }
                        if dro.is_mana {
                            mana -= dro.actual_drain;
                            total_mana_drain += dro.actual_drain;
                        } else {
                            hp -= dro.actual_drain;
                        }
                        st_at[bi] += dro.computed_delay;
                    }
                }
            }

            let mut effective_post = post_dt;
            if let Some(fpo) = fast_post_override {
                if c == 0 {
                    effective_post = fpo;
                    melee_cd_remaining = js_max(0.0, melee_cd_remaining - (effective_post - post_dt));
                }
            }
            advance!(effective_post);
        }

        if has_hp_warning { loop_hp_warn = true; }
        if has_mana_warning { loop_mana_warn = true; }
        ri += 1;
    }
    (start_mana, mana, has_hp_warning, has_mana_warning)
}

/// row_unclamped_spell_cost (pure/boost.js).
pub fn row_unclamped_spell_cost(
    base_stats: &Obj, spell: &Value, tokens: &[Token], registry: &[Value],
    tables: &Tables, skillpoint_final_mult_2: f64,
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

    let int_reduction = tables.sp_to_pct(v_int) * skillpoint_final_mult_2;
    let mut cost = spell.get("cost").and_then(|c| c.as_f64()).unwrap_or(f64::NAN) * (1.0 - int_reduction);
    cost += v_raw;
    cost *= 1.0 + v_pct / 100.0;
    cost * (1.0 + v_final / 100.0)
}

/// One guild-tome choice in a tome-optimisation search.
///
/// `unit` feeds the SP solver (slot 8, where the JS worker puts `cand.sm`);
/// `sm` is the statmap whose stats join the build sum. `idx` is the JS-side
/// identifier echoed back so the UI can show which tome was chosen.
#[derive(Clone, Default)]
pub struct TomeCand {
    pub idx: i64,
    pub sm: Obj,
    pub unit: crate::Unit,
}

/// One weapon+armour tome bundle.
#[derive(Clone, Debug, Default)]
pub struct TomeBundle {
    /// Flat additive stat sum over the bundle's rolled IDs (may be empty).
    pub stats: Obj,
    /// Display names, echoed back with the result. `weaponTome` then
    /// `armorTome`, matching the JS `names` object.
    pub weapon_names: Vec<String>,
    pub armor_names: Vec<String>,
}

fn parse_unit_from_sm(sm: &Obj) -> crate::Unit {
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
}

fn parse_tome_cands(v: Option<&Value>) -> Vec<TomeCand> {
    let Some(arr) = v.and_then(|x| x.as_array()) else { return Vec::new() };
    arr.iter().filter_map(|c| {
        let o = c.as_object()?;
        let sm = o.get("sm").and_then(as_map).cloned().unwrap_or_default();
        Some(TomeCand {
            idx: o.get("idx").and_then(|x| x.as_i64()).unwrap_or(-1),
            unit: parse_unit_from_sm(&sm),
            sm,
        })
    }).collect()
}

fn parse_tome_bundles(v: Option<&Value>) -> Vec<TomeBundle> {
    let Some(arr) = v.and_then(|x| x.as_array()) else { return Vec::new() };
    let names_of = |o: &serde_json::Map<String, Value>, key: &str| -> Vec<String> {
        o.get("names").and_then(|n| n.as_object())
            .and_then(|n| n.get(key)).and_then(|n| n.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    };
    arr.iter().filter_map(|b| {
        let o = b.as_object()?;
        Some(TomeBundle {
            stats: o.get("stats").and_then(as_map).cloned().unwrap_or_default(),
            weapon_names: names_of(o, "weaponTome"),
            armor_names: names_of(o, "armorTome"),
        })
    }).collect()
}

pub struct L2Consts {
    pub base_mana_regen: f64,
    pub mana_tick_seconds: f64,
    pub spell_cast_time: f64,
    pub spell_cast_delay: f64,
    pub skillpoint_final_mult_2: f64,
    pub combo_time: f64,
    pub allow_downtime: bool,
    pub hp_casting: bool,
    pub sp_budget: i32,
    /// `layer2.health_config`. The fast mana sim models buff states and the
    /// Blood Pact payment branch, exactly as `simulate_combo_mana_fast`
    /// does, so it has to be reachable from every `mana_check_passes` call
    /// site — which is why it lives here rather than on `ScoringCtx`.
    pub health: crate::mana_sim::HealthConfig,
    /// True when any loop bracket is an until-OOM loop. Those deplete mana
    /// on purpose, so a mana warning is the terminator, not a failure.
    pub has_oom_loop: bool,
    /// Set when the damage rows are leaf-dependent — see `DynamicRows`.
    pub dynamic: Option<DynamicRows>,
}

/// Scenarios whose damage rows cannot be fixed at load time.
///
/// Two causes, both handled the same way: a declared slider means
/// `inject_blood_pact_boosts` writes simulation-derived boost tokens into
/// the rows per leaf, and an until-OOM loop means the iteration count (and
/// so the row *sequence*) depends on the leaf's own mana trajectory.
///
/// Either way the rows stop being parse-time constants, which the compiled
/// and dense lowerings both rely on — so these scenarios run on the Obj
/// path with the ceiling gate and every bound switched off. Correct, and
/// slower than a static scenario.
#[derive(Clone, Default)]
pub struct DynamicRows {
    pub bp_slider: Option<String>,
    /// `(buff_state index, slider name)` — indexed so injection reads
    /// `RowResult::state_values` positionally in the per-trial path.
    pub state_sliders: Vec<(usize, String)>,
    /// Rows still carry loop markers and must be unrolled per leaf.
    pub needs_unroll: bool,
}

impl L2Consts {
    /// The subset the stateful simulation needs.
    pub fn sim_consts(&self) -> crate::mana_sim::SimConsts {
        crate::mana_sim::SimConsts {
            base_mana_regen: self.base_mana_regen,
            mana_tick_seconds: self.mana_tick_seconds,
            spell_cast_time: self.spell_cast_time,
            spell_cast_delay: self.spell_cast_delay,
            hpr_tick_seconds: crate::mana_sim::HPR_TICK_SECONDS,
            hidden_base_hpr: crate::mana_sim::HIDDEN_BASE_HPR,
            skillpoint_final_mult_2: self.skillpoint_final_mult_2,
        }
    }

    pub fn parse(fixture: &Value) -> Option<L2Consts> {
        let l2 = fixture.get("layer2")?;
        let health = l2.get("health_config").map(crate::mana_sim::HealthConfig::parse)
            .unwrap_or_default();
        // Count loops unroll statically at load, so an until-OOM loop is the
        // only kind that survives to need a per-leaf unroll.
        let has_oom_loop = fixture.get("parsed_combo").and_then(|c| c.as_array())
            .map(|rows| rows.iter().any(|r| r.get("loop_start")
                .and_then(|c| c.get("type")).and_then(|t| t.as_i64())
                == Some(crate::mana_sim::LOOP_COND_UNTIL_OOM)))
            .unwrap_or(false);
        // A declared slider or a per-leaf unroll makes the damage rows
        // leaf-dependent. Decided here rather than in `ScoringCtx::load` so
        // every consumer of `L2Consts` — including the `score_kernel`
        // validator — takes the same path the solver does.
        let (bp_slider, state_sliders) = crate::mana_sim::extract_slider_names(&health);
        let dynamic_rows = if bp_slider.is_some() || !state_sliders.is_empty() || has_oom_loop {
            Some(DynamicRows { bp_slider, state_sliders, needs_unroll: has_oom_loop })
        } else { None };
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
            health,
            dynamic: dynamic_rows,
            has_oom_loop,
        })
    }
}

/// eval_combo_mana_check (fast-sim path) for the supported subset:
/// no loops, no Blood Pact, no buff states.
pub fn mana_check_passes(
    rows: &[Row], combo_base: &Obj, registry: &[Value], tables: &Tables, consts: &L2Consts,
    compiled: Option<&[CompiledRow]>,
) -> bool {
    if consts.combo_time == 0.0 && !consts.hp_casting {
        return true;
    }
    let has_transcendence = combo_base.get("activeMajorIDs")
        .and_then(|v| v.get("__s")).and_then(|s| s.as_array())
        .map(|a| a.iter().any(|m| m.as_str() == Some("ARCANES")))
        .unwrap_or(false);
    // An until-OOM loop is *meant* to run mana out — the warning terminates
    // the loop rather than failing the build, so the sim must be allowed to
    // finish (no fail-fast on a mana warning) and the verdict ignores it.
    let oom = consts.has_oom_loop;
    let (start_mana, end_mana, hp_warn, mana_warn) = simulate_mana_fast_ff(
        rows, combo_base, has_transcendence, registry, tables, consts, compiled, !oom);
    if hp_warn { return false; }
    if !oom && mana_warn { return false; }
    if oom { return true; }
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
    ThresholdReject,
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
    objective: &Objective, compiled: Option<&[CompiledRow]>,
    dense: Option<(&DenseCtx, &mut DenseWork)>,
    thresholds: &[Threshold], base_costs: &HashMap<i64, f64>,
) -> Result<Option<LeafResult>, String> {
    match leaf_pipeline_gated(item_names, l2, weapon, guild, kernel, rows,
                              registry, hit_refs, tables, consts, objective, compiled, None,
                              dense, thresholds, base_costs, None, None)? {
        LeafOutcome::Scored(r) => Ok(Some(r)),
        LeafOutcome::Gated => unreachable!("no cutoff passed"),
        _ => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
// ── Phase trace (SCORE_TRACE=1): coarse per-phase nanos across all threads ──
//
// Counters accumulate in THREAD-LOCAL cells and are summed into the shared
// totals when each thread finishes. They used to be shared atomics bumped in
// place, which put four threads in a fetch_add fight over the same few cache
// lines several million times a second — the greedy trial counter alone was
// hit 38M times in a 20s ehp run. The result was a profiler that cost 22% of
// throughput and charged most of that to the hottest phase it was measuring,
// which is precisely the phase you would then go and "optimize".
//
// The per-trial timers (assemble/damage/dynamic-rows) are separate: two
// `Instant::now()` calls around work measured in tens of nanoseconds cannot
// be made cheap, so they only arm at SCORE_TRACE=2. SCORE_TRACE=1 gives the
// phase-level picture at a cost that does not move the numbers.
pub mod trace {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};

    pub const SP: usize = 0;
    pub const BASE: usize = 1;
    pub const GATE: usize = 2;
    pub const DOOM: usize = 3;
    pub const GREEDY: usize = 4;
    pub const MANA: usize = 5;
    pub const FINAL: usize = 6;
    pub const GREEDY_TRIALS: usize = 7;
    /// Per-leaf dynamic row construction (simulate + unroll + inject).
    pub const DYN: usize = 8;
    pub const ASM: usize = 9;
    pub const DMG: usize = 10;
    /// Split of the doom + final-mana phases into the three things they do:
    /// assemble the dense vector, materialize the mini stat map for the sim,
    /// and run the sim itself.
    pub const MASM: usize = 11;
    pub const MOBJ: usize = 12;
    pub const MSIM: usize = 13;
    /// Mid-tree/cluster bound ceiling evaluations — the batch-shaped work a
    /// GPU offload would target. Counted in enum_kernel.
    pub const BOUND: usize = 14;
    pub const BOUND_EVALS: usize = 15;
    /// fill_direct call count.
    pub const FD_CALLS: usize = 16;
    pub const FD_ADDS: usize = 17;
    /// Whole-leaf scoring call, so enumeration work can be told apart from
    /// scoring work: everything outside this is the tree walk itself.
    pub const PIPE: usize = 18;
    /// Per-leaf prologue: rebuilding the 8 equipment Units out of JSON and
    /// recomputing the set-bonus pool, before the SP solve starts.
    pub const PRE: usize = 19;
    /// Copying the lowered leaf into the per-trial scratch.
    pub const RESET: usize = 20;
    /// fill_direct internals (SCORE_TRACE=2 only).
    pub const FD_COPY: usize = 21;
    pub const FD_ITEMS: usize = 22;
    pub const FD_TAIL: usize = 23;
    /// Memoization feasibility probe: how many of the eight item slots
    /// actually change from one leaf to the next.
    pub const FD_DIFF_SLOTS: usize = 24;
    pub const FD_SAME_ALL: usize = 25;
    /// Feasibility probe for memoizing the SP solve, whose result depends
    /// only on the eight units, the weapon unit, the budget and the set pool.
    pub const SPK_HIT: usize = 26;
    pub const SPK_MISS: usize = 27;
    pub const SPK_SAME_PREV: usize = 28;
    /// The existing subtree/cluster ceiling memo in the enumerator.
    pub const BM_HIT: usize = 29;
    pub const BM_MISS: usize = 30;
    pub const N: usize = 31;

    pub static ENABLED: AtomicBool = AtomicBool::new(false);
    /// SCORE_TRACE=2: also time the per-trial sub-phases.
    pub static FINE: AtomicBool = AtomicBool::new(false);
    static TOTAL: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];

    struct Local {
        v: [Cell<u64>; N],
    }
    impl Drop for Local {
        /// Worker threads are joined before the report is printed, so their
        /// destructors have already folded their counts in by then.
        fn drop(&mut self) {
            for i in 0..N {
                let n = self.v[i].get();
                if n != 0 { TOTAL[i].fetch_add(n, Ordering::Relaxed); }
            }
        }
    }
    thread_local! {
        static LOCAL: Local = Local { v: [const { Cell::new(0) }; N] };
    }

    pub fn init_from_env() {
        match std::env::var("SCORE_TRACE").as_deref() {
            Ok("1") => ENABLED.store(true, Ordering::Relaxed),
            Ok("2") => {
                ENABLED.store(true, Ordering::Relaxed);
                FINE.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }
    #[inline]
    pub fn on() -> bool { ENABLED.load(Ordering::Relaxed) }
    /// Whether the per-trial sub-timers are armed (SCORE_TRACE=2).
    #[inline]
    pub fn fine() -> bool { FINE.load(Ordering::Relaxed) }

    #[inline]
    pub fn add(id: usize, n: u64) {
        // During TLS teardown the cell is already gone; dropping the sample
        // is better than panicking inside a destructor.
        let _ = LOCAL.try_with(|l| l.v[id].set(l.v[id].get() + n));
    }

    /// Fold this thread's counts into the totals now, rather than waiting for
    /// its destructor. The reporting thread needs this for its own counts.
    pub fn flush() {
        let _ = LOCAL.try_with(|l| {
            for i in 0..N {
                let n = l.v[i].replace(0);
                if n != 0 { TOTAL[i].fetch_add(n, Ordering::Relaxed); }
            }
        });
    }

    pub fn report() {
        if !on() { return; }
        flush();
        let f = |i: usize| TOTAL[i].load(Ordering::Relaxed) as f64 / 1e9;
        let c = |i: usize| TOTAL[i].load(Ordering::Relaxed);
        eprintln!(
            "score_trace: sp {:.2}s | base {:.2}s | gate {:.2}s | doom {:.2}s | greedy {:.2}s ({} trials) | mana {:.2}s | final {:.2}s",
            f(SP), f(BASE), f(GATE), f(DOOM), f(GREEDY), c(GREEDY_TRIALS), f(MANA), f(FINAL));
        if fine() {
            eprintln!("score_trace: trial split — assemble {:.2}s | damage {:.2}s | dynamic rows {:.2}s",
                      f(ASM), f(DMG), f(DYN));
        } else {
            eprintln!("score_trace: trial split — (SCORE_TRACE=2; timing every trial \
                       distorts the phase totals it sits inside)");
        }
        eprintln!("score_trace: mana split — assemble {:.2}s | mini-obj {:.2}s | sim {:.2}s",
                  f(MASM), f(MOBJ), f(MSIM));
        eprintln!("score_trace: bound evals {} in {:.2}s (offloadable batch work)",
                  c(BOUND_EVALS), f(BOUND));
        eprintln!("score_trace: prologue (equip units + set pool) {:.2}s | scratch reset {:.2}s",
                  f(PRE), f(RESET));
        eprintln!("score_trace: whole leaf pipeline {:.2}s (the rest of the wall \
                   time x threads is the enumeration walk)", f(PIPE));
        if c(BM_HIT) + c(BM_MISS) > 0 {
            let (h, m) = (c(BM_HIT), c(BM_MISS));
            eprintln!("score_trace: bound memo — {} hit / {} miss = {:.1}%",
                      h, m, h as f64 / (h + m) as f64 * 100.0);
        }
        if c(SPK_HIT) + c(SPK_MISS) > 0 {
            let (h, m, p) = (c(SPK_HIT), c(SPK_MISS), c(SPK_SAME_PREV));
            eprintln!("score_trace: SP-solve key repeats — {:.1}% would hit a 4096-slot memo, \
                       {:.1}% identical to the previous leaf",
                      h as f64 / (h + m) as f64 * 100.0, p as f64 / (h + m) as f64 * 100.0);
        }
        if fine() && c(FD_CALLS) > 0 {
            eprintln!("score_trace: fill_direct slot churn — {:.2} of 8 item slots change \
                       per leaf ({} calls, {} identical to previous)",
                      c(FD_DIFF_SLOTS) as f64 / c(FD_CALLS) as f64, c(FD_CALLS), c(FD_SAME_ALL));
        }
        if fine() && c(FD_CALLS) > 0 {
            eprintln!("score_trace: fill_direct {} calls — template copy {:.2}s | \
                       item loop {:.2}s | tail {:.2}s",
                      c(FD_CALLS), f(FD_COPY), f(FD_ITEMS), f(FD_TAIL));
        }
    }
}

/// Read an env kill switch once, not on every visit.
///
/// `std::env::var` is not a cheap read: it takes a lock on the process
/// environment, scans it, and allocates a String for the answer. Several of
/// these sat in the per-leaf pipeline and one in the enumerator's innermost
/// cluster loop, where they ran millions of times per second to re-answer a
/// question whose answer cannot change while the process is alive.
///
/// Each expansion gets its own static, so the value is resolved on first use
/// and read as a plain bool thereafter.
macro_rules! env_once {
    ($name:literal == $val:literal) => {{
        static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *V.get_or_init(|| std::env::var($name).as_deref() == Ok($val))
    }};
    ($name:literal != $val:literal) => {{
        static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *V.get_or_init(|| std::env::var($name).as_deref() != Ok($val))
    }};
}
pub(crate) use env_once;

/// Set when a caller wants the dense/Obj cross-check on regardless of the
/// environment. `score_kernel` used to ask for it by setting
/// SCORE_DENSE_CHECK part-way through its own run, which a once-only read of
/// the environment would never see — it would have latched "off" from the
/// earlier pipeline calls and silently disarmed the tripwire. Asking through
/// this flag says the same thing without depending on when it is read.
static DENSE_CHECK_FORCED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Turn the dense/Obj cross-check on for the rest of the process.
pub fn force_dense_check() {
    DENSE_CHECK_FORCED.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[inline]
pub fn dense_check_on() -> bool {
    DENSE_CHECK_FORCED.load(std::sync::atomic::Ordering::Relaxed)
        || env_once!("SCORE_DENSE_CHECK" == "1")
}

macro_rules! phase {
    ($counter:ident, $body:expr) => {{
        if trace::on() {
            let t0 = std::time::Instant::now();
            let r = $body;
            trace::add(trace::$counter, t0.elapsed().as_nanos() as u64);
            r
        } else { $body }
    }};
    // Per-trial sub-phase: only armed at SCORE_TRACE=2, because the two
    // clock reads cost more than the work they bracket.
    (fine $counter:ident, $body:expr) => {{
        if trace::fine() {
            let t0 = std::time::Instant::now();
            let r = $body;
            trace::add(trace::$counter, t0.elapsed().as_nanos() as u64);
            r
        } else { $body }
    }};
}

/// Which tome the leaf loop settled on, echoed back with the result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TomeChoice {
    pub guild_idx: i64,
    pub weapon_names: Vec<String>,
    pub armor_names: Vec<String>,
}

/// `leaf_pipeline_gated` with the tome as a searched dimension.
///
/// Mirrors the JS worker's leaf loop: for each guild candidate (each needs its
/// own SP solve, because a guild tome carries requirements and skill points)
/// crossed with each weapon/armour bundle (a plain additive stat sum merged
/// last), score the leaf and keep the best.
///
/// The JS runs its SP gate with the optimistic guild tome and its ceiling gate
/// with the optimistic bundle before the loop, then re-gates per bundle inside
/// it. Here every combination is gated against its OWN ceiling, which is at
/// least as tight and never prunes a combination that could beat the cutoff —
/// only the best combination is ever inserted, and the best must beat the
/// cutoff to be inserted at all. Same results, different amount of work.
///
/// The leaf base is built once here and shared: it depends on neither the
/// guild choice (which reaches the build only through the SP solve) nor the
/// bundle (which is merged after assembly).
pub fn leaf_pipeline_tome(
    item_names: &[&str], l2: &Layer2, weapon: &Obj, guild: Option<&crate::Unit>,
    kernel: &mut crate::Kernel, rows: &[Row], registry: &[Value],
    hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables, consts: &L2Consts,
    objective: &Objective, compiled: Option<&[CompiledRow]>, gate_cutoff: Option<f64>,
    dense: Option<(&DenseCtx, &mut DenseWork)>,
    thresholds: &[Threshold], base_costs: &HashMap<i64, f64>,
) -> Result<(LeafOutcome, Option<TomeChoice>), String> {
    // Off, or nothing to search: the exact pre-tome pipeline, untouched.
    if l2.tome_opt == 0
        || (l2.guild_tome_cands.is_empty() && l2.tome_wa_bundles.is_empty()) {
        let out = leaf_pipeline_gated(
            item_names, l2, weapon, guild, kernel, rows, registry, hit_refs, tables,
            consts, objective, compiled, gate_cutoff, dense, thresholds, base_costs,
            None, None)?;
        return Ok((out, None));
    }

    // A bundle disables the dense lowering (no per-bundle delta yet), and the
    // base is then read on every combination, so build it once up front.
    let has_bundles = l2.tome_wa_bundles.iter().any(|b| !b.stats.is_empty());
    let base_owned: Option<Obj> = if has_bundles {
        Some(l2.build_base(item_names, weapon)?)
    } else { None };

    // `None` guild candidates means the guild tome is FIXED (the UI greys the
    // dropdown out in this mode but the choice still stands), so fall back to
    // the caller's unit rather than dropping it.
    let single;
    let cands: &[TomeCand] = if l2.guild_tome_cands.is_empty() {
        single = [TomeCand { idx: -1, sm: Obj::new(), unit: guild.copied().unwrap_or_default() }];
        &single
    } else {
        &l2.guild_tome_cands
    };
    let no_bundle = [TomeBundle::default()];
    let bundles: &[TomeBundle] = if l2.tome_wa_bundles.is_empty() {
        &no_bundle
    } else {
        &l2.tome_wa_bundles
    };

    let mut best: Option<(LeafResult, TomeChoice)> = None;
    // Worst outcome seen, so a leaf that every combination rejects still
    // reports WHY rather than collapsing to "SP infeasible".
    let mut worst = LeafOutcome::SpInfeasible;
    let rank = |o: &LeafOutcome| match o {
        LeafOutcome::SpInfeasible => 0,
        LeafOutcome::Gated => 1,
        LeafOutcome::ManaReject => 2,
        LeafOutcome::ThresholdReject => 3,
        LeafOutcome::Scored(_) => 4,
    };

    // Dense is only reachable when no bundle carries stats; it is a &mut, so
    // it cannot be handed to more than one call. Guild-only mode still gets it
    // on the first (and only) bundle.
    let mut dense_slot = if has_bundles { None } else { dense };

    for cand in cands {
        for bundle in bundles {
            let extra = if bundle.stats.is_empty() { None } else { Some(&bundle.stats) };
            let out = leaf_pipeline_gated(
                item_names, l2, weapon, Some(&cand.unit), kernel, rows, registry,
                hit_refs, tables, consts, objective, compiled,
                // Once this leaf has a best, a later combination only matters
                // if it beats it — tighten the gate accordingly.
                match (&best, gate_cutoff) {
                    (Some((b, _)), Some(c)) => Some(b.score.max(c)),
                    (Some((b, _)), None) => Some(b.score),
                    (None, c) => c,
                },
                dense_slot.take(), thresholds, base_costs,
                extra, base_owned.as_ref())?;
            let r_out = rank(&out);
            if let LeafOutcome::Scored(r) = out {
                let better = best.as_ref().map(|(b, _)| r.score > b.score).unwrap_or(true);
                if better {
                    best = Some((r, TomeChoice {
                        guild_idx: cand.idx,
                        weapon_names: bundle.weapon_names.clone(),
                        armor_names: bundle.armor_names.clone(),
                    }));
                }
            } else if r_out > rank(&worst) {
                worst = out;
            }
        }
    }

    match best {
        Some((r, choice)) => Ok((LeafOutcome::Scored(r), Some(choice))),
        None => Ok((worst, None)),
    }
}

pub fn leaf_pipeline_gated(
    item_names: &[&str], l2: &Layer2, weapon: &Obj, guild: Option<&crate::Unit>,
    kernel: &mut crate::Kernel, rows: &[Row], registry: &[Value],
    hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables, consts: &L2Consts,
    objective: &Objective, compiled: Option<&[CompiledRow]>, gate_cutoff: Option<f64>,
    dense: Option<(&DenseCtx, &mut DenseWork)>,
    thresholds: &[Threshold], base_costs: &HashMap<i64, f64>,
    // Tome bundle for this trial, merged last into every assemble. None when
    // tome optimisation is off or the bundle is empty.
    tome_extra: Option<&Obj>,
    // Prebuilt leaf base. The tome loop runs this pipeline once per
    // (guild candidate x bundle), and the base depends on neither — rebuilding
    // it per combination would cost more than the whole rest of the trial.
    base_pre: Option<&Obj>,
) -> Result<LeafOutcome, String> {
    // Dynamic rows rule out every precomputed shortcut: the compiled rows and
    // the dense lowering both key off parse-time-constant boost tokens, and
    // the all-150-SP ceiling is not an upper bound once the rows themselves
    // change per leaf. Force the Obj path here so no caller can forget.
    let dynamic = consts.dynamic.as_ref();
    // Dynamic rows still rule out the dense lowering and the ceiling, but a
    // no-unroll scenario keeps its compiled rows — see the obj_score branch.
    let compiled = match dynamic {
        Some(dy) if dy.needs_unroll => None,
        _ => compiled,
    };
    // A per-leaf unroll changes the row count, which the lowering cannot
    // follow. Without one, dense can serve these too — the injected tokens
    // become extra slot writes (see `dense_dynamic_score`).
    let dense = match dynamic {
        Some(dy) if dy.needs_unroll => None,
        _ => dense,
    };
    // The dense lowering folds the FIXED tomes in at load time and has no
    // per-bundle delta, so a bundle would silently score without its stats.
    // Guild-only tome optimisation has no bundle and keeps the dense path:
    // a guild candidate changes only the SP solve.
    let dense = if tome_extra.is_some() { None } else { dense };
    let gate_cutoff = if dynamic.is_some() { None } else { gate_cutoff };

    let obj_score = |cb: &mut Obj| -> f64 {
        if let Some(dy) = dynamic {
            let mut injected: Vec<Vec<Token>> = Vec::new();
            let damage_rows = phase!(fine DYN,
                dynamic_damage_rows_split(cb, rows, registry, tables, consts, dy, &mut injected));
            // With no unroll the rows line up 1:1 with the compiled ones, so
            // only the *appended* tokens need bonuses — compiling those (one
            // or two tokens per row) is far cheaper than the uncompiled
            // scorer re-resolving every token for every row on every greedy
            // trial. A prop-bonus on an injected token would change the row's
            // mod_spell, which the compiled row baked in at load, so that
            // case falls back rather than silently using a stale spell.
            if !dy.needs_unroll && !compiled.unwrap_or(&[]).is_empty() {
                let mut extra: Vec<Vec<CompiledBonus>> = Vec::with_capacity(damage_rows.len());
                let mut props: HashMap<String, PropOverride> = HashMap::new();
                let mut ok = true;
                for i in 0..damage_rows.len() {
                    let row_tokens = injected.get(i).map(|t| t.as_slice()).unwrap_or(&[]);
                    let mut b = Vec::new();
                    props.clear();
                    compile_token_bonuses(row_tokens, registry, &mut b, &mut props);
                    if !props.is_empty() { ok = false; break; }
                    extra.push(b);
                }
                if ok {
                    return objective.score_compiled_extra(
                        cb, weapon, &damage_rows, compiled.unwrap(), tables, &extra);
                }
            }
            // The uncompiled scorer resolves every token itself, so it needs
            // the injected ones actually in the rows.
            let merged: Vec<Row> = damage_rows.iter().cloned().enumerate()
                .map(|(i, mut r)| {
                    if let Some(t) = injected.get(i) { r.tokens.extend(t.iter().cloned()); }
                    r
                })
                .collect();
            return objective.score(cb, weapon, &merged, registry, hit_refs, tables);
        }
        match compiled {
            Some(c) => objective.score_compiled(cb, weapon, rows, c, tables),
            None => objective.score(cb, weapon, rows, registry, hit_refs, tables),
        }
    };
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
    // Every assemble in this function goes through here so a tome bundle
    // cannot be applied to some stages and missed by others.
    let asm = |base: &Obj, sp: &[f64]| l2.assemble_from_base_extra(base, sp, weapon, tome_extra);

    let _pre_t0 = if trace::on() { Some(std::time::Instant::now()) } else { None };
    let mut equipment: [crate::Unit; 8] = Default::default();
    let mut preludes: [Option<&ItemPrelude>; 8] = [None; 8];
    for (i, name) in item_names.iter().enumerate().take(8) {
        let pre = l2.item_prelude.get(*name)
            .ok_or_else(|| format!("item not in registry: {}", name))?;
        equipment[i] = pre.unit;
        preludes[i] = Some(pre);
    }
    if dense_check_on() {
        // The cache must agree with reading the item JSON, which is what
        // this pipeline did before it existed.
        for (i, name) in item_names.iter().enumerate().take(8) {
            let item = l2.item_registry.get(*name).expect("registry");
            assert_eq!(equipment[i], unit_of(item), "item prelude unit drift: {name}");
            let set = if item.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false) { None }
                      else { item.get("set").and_then(|v| v.as_str()) };
            assert_eq!(preludes[i].unwrap().set_name.as_deref(), set,
                       "item prelude set drift: {name}");
        }
    }

    // Set-bonus SP folded into the free pool (worker leaf behavior).
    let mut set_free = [0i32; 5];
    {
        let mut set_counts: Vec<(&str, i64)> = Vec::new();
        for pre in preludes.iter().flatten() {
            let Some(set_name) = pre.set_name.as_deref() else { continue };
            match set_counts.iter_mut().find(|(n, _)| *n == set_name) {
                Some((_, c)) => *c += 1,
                None => set_counts.push((set_name, 1)),
            }
        }
        for (set_name, count) in &set_counts {
            if let Some(b) = l2.set_bonus_sp.get(*set_name)
                .and_then(|per| per.get((*count - 1) as usize)) {
                for i in 0..5 { set_free[i] += b[i]; }
            }
        }
        if dense_check_on() {
            let mut want = [0i32; 5];
            for (set_name, count) in &set_counts {
                let Some(set_data) = l2.sets_data.get(*set_name) else { continue };
                let Some(bonus) = set_data.get("bonuses").and_then(|b| b.as_array())
                    .and_then(|b| b.get((*count - 1) as usize)).and_then(|b| b.as_object())
                    else { continue };
                for (i, skp) in l2.skp_order.iter().enumerate() {
                    if let Some(v) = bonus.get(skp).and_then(|x| x.as_f64()) { want[i] += v as i32; }
                }
            }
            assert_eq!(set_free, want, "set bonus table drift");
        }
    }

    if let Some(t0) = _pre_t0 {
        trace::add(trace::PRE, t0.elapsed().as_nanos() as u64);
    }
    let case = crate::Case {
        budget: consts.sp_budget,
        equipment,
        weapon: unit_of(weapon),
        set_free,
        expected: None,
    };
    if trace::fine() {
        // Would a memo on the SP solve hit? Key = everything the solve reads.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |x: u64| { h ^= x; h = h.wrapping_mul(0x100_0000_01b3); };
        for u in &case.equipment {
            mix(u.crafted as u64);
            for j in 0..5 { mix(u.reqs[j] as i64 as u64); mix(u.skp[j] as i64 as u64); }
        }
        for j in 0..5 { mix(case.set_free[j] as i64 as u64); }
        mix(case.budget as i64 as u64);
        thread_local! {
            static SPK: std::cell::RefCell<(Vec<u64>, u64)> =
                std::cell::RefCell::new((vec![0u64; 4096], 0));
        }
        SPK.with(|c| {
            let mut c = c.borrow_mut();
            if c.1 == h { trace::add(trace::SPK_SAME_PREV, 1); }
            c.1 = h;
            let slot = (h >> 20) as usize % 4096;
            if c.0[slot] == h { trace::add(trace::SPK_HIT, 1); }
            else { c.0[slot] = h; trace::add(trace::SPK_MISS, 1); }
        });
    }
    let Some((assign, total, assigned)) = phase!(SP, kernel.calculate_with_extra(&case, guild)) else {
        return Ok(LeafOutcome::SpInfeasible);
    };
    let mut base_sp = assign;
    let mut total_sp = total;
    let mut assigned_sp = assigned;

    // Dense hot path: per-leaf lowered stats for the gate + greedy trials.
    // The direct build skips the Obj base entirely; any shape the lowering
    // can't hold bit-exactly falls back to lowering from the Obj base, or to
    // the Obj path outright. SCORE_DENSE_CHECK=1 runs both paths per
    // evaluation and panics on any bit difference.
    let dense_check = dense_check_on();
    let compiled_rows = compiled.unwrap_or(&[]);
    let mut base_opt: Option<Obj> = None;
    let mut dwork: Option<(&DenseCtx, &mut DenseWork)> = None;
    if let Some((d, w)) = dense {
        let mut ok = match &d.direct {
            Some(dd) => phase!(BASE, w.leaf.fill_direct(d, dd, item_names)),
            None => false,
        };
        if !ok {
            if base_opt.is_none() {
                base_opt = Some(phase!(BASE, l2.build_base(item_names, weapon))?);
            }
            if let Some(l) = DenseLeaf::build(d, l2, base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), tables) {
                w.leaf = l;
                ok = true;
            }
        }
        if ok {
            phase!(RESET, w.scratch.reset(&w.leaf, d));
            dwork = Some((d, w));
        }
    }

    // Score-ceiling gate (mirrors the JS worker): one damage eval at
    // all-150 SP upper-bounds anything greedy can reach. Strict margin so
    // a float-ulp monotonicity wobble never gates a genuine candidate.
    if let Some(cutoff) = gate_cutoff {
        let two_sided_off = objective.needs_two_sided_ceiling() && !Objective::two_sided_enabled();
        // The JS excludes `hp_casting` here, describing it as sim-coupled
        // damage alongside dynamic sliders. Only the slider half is: HP-cost
        // casting reaches damage exclusively through the Blood Pact bonus,
        // and that bonus reaches the rows exclusively through
        // `inject_blood_pact_boosts`, which does nothing without a declared
        // `damage_boost.slider_name`. A declared one makes the scenario
        // `dynamic`, and `gate_cutoff` is already None for those.
        //
        // Everything else `hp_casting` touches — the mana check, the mana
        // rescue, the doom bound — decides FEASIBILITY. Those only ever
        // remove candidates, so the all-150 assemble still bounds the score
        // of every allocation the greedy can reach, which is all the gate
        // needs. Disabling the rescue in particular shrinks the reachable
        // set, and a ceiling over a superset is still a ceiling.
        let gate_env_off = env_once!("SCORE_HPCAST_GATE" == "0")
            && consts.hp_casting;
        if objective.supports_ceiling() && l2.ceiling_vars_ok && !two_sided_off
            && !gate_env_off {
            if (dwork.is_none() || dense_check) && base_pre.is_none() && base_opt.is_none() {
                base_opt = Some(phase!(BASE, l2.build_base(item_names, weapon))?);
            }
            // Blends with a negative weight need each term at its own
            // extreme: the negative ones off the leaf's pre-greedy SP (their
            // target's minimum over everything the greedy and the rescue can
            // reach), the rest off all-150. See needs_two_sided_ceiling.
            let two_sided = objective.needs_two_sided_ceiling();
            // Only the Obj fallback needs the materialized base; the dense
            // path assembles both SP states from the lowered leaf. Building
            // it unconditionally cost more than the gate saved.
            if two_sided && dwork.is_none() && base_pre.is_none() && base_opt.is_none() {
                base_opt = Some(phase!(BASE, l2.build_base(item_names, weapon))?);
            }
            let gated = phase!(GATE, {
                let ceiling_sp = [150f64; 5];
                let low_sp: [f64; 5] = std::array::from_fn(|i| total_sp[i] as f64);
                let ceiling = if let Some((d, w)) = dwork.as_mut() {
                    let DenseWork { leaf, scratch, .. } = &mut **w;
                    let neg = if two_sided {
                        dense_assemble(d, leaf, scratch, &low_sp);
                        dense_score_signed(d, leaf, scratch, rows, compiled_rows, tables, true)
                    } else { 0.0 };
                    dense_assemble(d, leaf, scratch, &ceiling_sp);
                    let v = if two_sided {
                        dense_score_signed(d, leaf, scratch, rows, compiled_rows, tables, false) + neg
                    } else {
                        dense_score(d, leaf, scratch, rows, compiled_rows, tables)
                    };
                    if dense_check && !two_sided {
                        let mut cb150 = asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &ceiling_sp);
                        let o = obj_score(&mut cb150);
                        assert!(v == o || (v.is_nan() && o.is_nan()),
                                "dense/obj gate mismatch: {v:?} vs {o:?}");
                    }
                    v
                } else if two_sided {
                    let base = base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap());
                    let cb_lo = asm(base, &low_sp);
                    let neg = objective.score_signed(
                        &cb_lo, weapon, rows, registry, hit_refs, tables, true);
                    let cb150 = asm(base, &ceiling_sp);
                    objective.score_signed(
                        &cb150, weapon, rows, registry, hit_refs, tables, false) + neg
                } else {
                    let mut cb150 = asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &ceiling_sp);
                    obj_score(&mut cb150)
                };
                ceiling < cutoff - cutoff.abs() * 1e-9
            });
            if gated { return Ok(LeafOutcome::Gated); }
        }
    }

    // Leaf-invariant build stats for the Obj-path stages (fallbacks, check
    // mode, and the mana rescue). Dense leaves never build it unless the
    // rescue path fires — the mana/doom/final stages read a mini stat map
    // materialized from the dense assembled state instead.
    if (dwork.is_none() || dense_check) && base_pre.is_none() && base_opt.is_none() {
        base_opt = Some(phase!(BASE, l2.build_base(item_names, weapon))?);
    }

    // Mana-doom precheck: mana feasibility depends on the greedy SP only
    // through Int (monotone — more Int means more starting mana and cheaper
    // spells), so a single sim at the highest Int this leaf can reach
    // upper-bounds every greedy/rescue outcome. Admissible only when no atree
    // var effect couples stats into the mana-relevant set (l2.mana_doom_ok).
    //
    // That highest Int is rarely 150, and testing 150 asks whether a build
    // could sustain with skill points it cannot have. Every point that ends up
    // in Int is added by the greedy or moved there by the mana rescue, and
    // both raise `base_sp[2]` and `total_sp[2]` together while respecting
    // `base_sp[2] < 100` and `total_sp[2] < 150`; the rescue can only relocate
    // points the greedy placed, so between them they add at most `remaining`.
    // Hence the reachable ceiling below. On the ehp benchmark it averages 59
    // against the 150 that was being tested — every leaf, without exception.
    //
    // `SCORE_DOOM_INT=150` restores the old ceiling, which is what the
    // equivalence check compares against: tightening an admissible bound must
    // not change which leaves get *scored*, only how early the rest are cut.
    let doom_int = if env_once!("SCORE_DOOM_INT" == "150") { 150.0 } else {
        let rem = (consts.sp_budget - assigned_sp).max(0);
        let room = rem.min(100 - base_sp[2]).min(150 - total_sp[2]).max(0);
        (total_sp[2] + room) as f64
    };
    let mut doom_reject_expected = false;
    // Bounded doom availability: sound whenever var effects don't couple
    // into atkTier, and start-mana couplings (maxMana/int) only under
    // allow_downtime (raising start can raise start-minus-end in the <=5
    // sustain mode).
    let bounded_doom_ok = match dwork.as_ref() {
        Some((dctx, _)) if !l2.mana_doom_ok =>
            !dctx.doom_couples_tier && (consts.allow_downtime || !dctx.doom_couples_start)
            && env_once!("SCORE_BOUNDED_DOOM" != "0"),
        _ => false,
    };
    // Buff-state drain is a percentage of max_mana, and max_mana rises with
    // Int — so more Int means MORE absolute drain, and the Int=150 sim no
    // longer upper-bounds mana feasibility. The precheck is inadmissible
    // there regardless of `mana_doom_ok` (which only covers atree var-effect
    // coupling), so switch it off rather than prune a feasible leaf.
    let drain_breaks_doom = consts.health.buff_states.iter()
        .any(|bs| bs.drain_pct_per_second.as_ref().map(|d| d.mana > 0.0).unwrap_or(false))
        // An until-OOM loop's length depends on the leaf's own mana
        // trajectory, so an Int=150 sim does not bound the real one either.
        || dynamic.map(|d| d.needs_unroll).unwrap_or(false);
    if !drain_breaks_doom
        && (l2.mana_doom_ok || bounded_doom_ok) && (consts.combo_time != 0.0 || consts.hp_casting) {
        let doomed = phase!(DOOM, {
            let mut doom_sp = [0f64; 5];
            let mut sp_lo = [0f64; 5];
            for i in 0..5 { doom_sp[i] = total_sp[i] as f64; sp_lo[i] = total_sp[i] as f64; }
            doom_sp[2] = doom_int;
            if let Some((d, w)) = dwork.as_mut() {
                let DenseWork { leaf, scratch, .. } = &mut **w;
                phase!(MASM, if l2.mana_doom_ok {
                    dense_assemble(d, leaf, scratch, &doom_sp);
                } else {
                    dense_assemble_doom(d, leaf, scratch, &doom_sp, &sp_lo);
                });
                let doomed = phase!(MSIM, match mana_check_passes_dense(
                        rows, d, leaf, scratch, registry, tables, consts, compiled) {
                    Some(v) => !v,
                    None => {
                        let mini = phase!(MOBJ, dense_mana_obj(d, leaf, scratch));
                        !mana_check_passes(rows, &mini, registry, tables, consts, compiled)
                    }
                });
                if dense_check && l2.mana_doom_ok {
                    let cb_doom = asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &doom_sp);
                    let od = !mana_check_passes(rows, &cb_doom, registry, tables, consts, compiled);
                    assert_eq!(doomed, od, "dense/obj doom mismatch");
                }
                doomed
            } else {
                let cb_doom = asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &doom_sp);
                !mana_check_passes(rows, &cb_doom, registry, tables, consts, compiled)
            }
        });
        if doomed {
            // Tripwire: under check mode EVERY doom reject falls through to the
            // full pipeline, which must also reject (asserted below). This used
            // to cover only the bounded-doom case, which left the reachable-Int
            // ceiling — the part that decides how many leaves are cut — with no
            // check at all on the scenarios where it fires most.
            if !dense_check {
                return Ok(LeafOutcome::ManaReject);
            }
            doom_reject_expected = true;
        }
    }

    // Greedy allocation with objective-scored trials.
    let remaining = consts.sp_budget - assigned_sp;
    let cap_total = [150i32; 5];
    let orig_base_sp = base_sp;
    let mut trial_sp_f = [0f64; 5];
    let mut trial = |sp: &[i32; 5]| -> f64 {
        if trace::on() { trace::add(trace::GREEDY_TRIALS, 1); }
        for i in 0..5 { trial_sp_f[i] = sp[i] as f64; }
        if let Some((d, w)) = dwork.as_mut() {
            let DenseWork { leaf, scratch, sim_memo, .. } = &mut **w;
            phase!(fine ASM, dense_assemble(d, leaf, scratch, &trial_sp_f));
            let v = phase!(fine DMG, match dynamic {
                Some(dy) => dense_dynamic_score(
                    d, leaf, scratch, sim_memo, rows, compiled_rows, tables, registry, consts, dy)
                    .expect("dense dynamic score: injected key without a slot — \
                             DenseCtx::build should have reserved one"),
                None => dense_score(d, leaf, scratch, rows, compiled_rows, tables),
            });
            if dense_check {
                let mut cb = asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &trial_sp_f);
                let o = obj_score(&mut cb);
                assert!(v == o || (v.is_nan() && o.is_nan()),
                        "dense/obj trial mismatch at {sp:?}: {v:?} vs {o:?}");
            }
            return v;
        }
        let mut cb = phase!(fine ASM, asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &trial_sp_f));
        phase!(fine DMG, obj_score(&mut cb))
    };
    assigned_sp += phase!(GREEDY, greedy_sp_allocate(&mut base_sp, &mut total_sp, remaining, &cap_total, &mut trial));

    // Final assemble + mana check (+ rescue).
    let mut sp_f5 = [0f64; 5];
    for i in 0..5 { sp_f5[i] = total_sp[i] as f64; }
    if let Some((d, w)) = dwork.as_mut() {
        {
            let DenseWork { leaf, scratch, .. } = &mut **w;
            dense_assemble(d, leaf, scratch, &sp_f5);
            if !d.thresholds.is_empty() && !dense_check_thresholds(d, scratch, tables, consts) {
                if dense_check {
                    let cb = asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &sp_f5);
                    let ov = check_thresholds_obj(&StatsView::Borrowed(&cb), thresholds, base_costs, tables, consts);
                    assert!(!ov, "dense/obj threshold mismatch (dense rejects)");
                }
                return Ok(LeafOutcome::ThresholdReject);
            }
            if dense_check && !thresholds.is_empty() {
                let cb = asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &sp_f5);
                let ov = check_thresholds_obj(&StatsView::Borrowed(&cb), thresholds, base_costs, tables, consts);
                assert!(ov, "dense/obj threshold mismatch (dense passes)");
            }
        }
        let mana_ok = phase!(MANA, {
            let DenseWork { leaf, scratch, .. } = &mut **w;
            let ok = phase!(MSIM, match mana_check_passes_dense(
                    rows, d, leaf, scratch, registry, tables, consts, compiled) {
                Some(v) => v,
                None => {
                    let mini = phase!(MOBJ, dense_mana_obj(d, leaf, scratch));
                    mana_check_passes(rows, &mini, registry, tables, consts, compiled)
                }
            });
            if dense_check {
                let cb = asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &sp_f5);
                let oo = mana_check_passes(rows, &cb, registry, tables, consts, compiled);
                assert_eq!(ok, oo, "dense/obj final mana mismatch");
            }
            ok
        });
        let saved_rescue_base = base_sp;
        let saved_rescue_total = total_sp;
        let _ = (saved_rescue_base, saved_rescue_total);
        if mana_ok {
            assert!(!doom_reject_expected, "doom precheck would have rejected a scored leaf");
            let score = phase!(FINAL, {
                let DenseWork { leaf, scratch, sim_memo, .. } = &mut **w;
                let v = match dynamic {
                    Some(dy) => dense_dynamic_score(
                        d, leaf, scratch, sim_memo, rows, compiled_rows, tables, registry, consts,
                        dy)
                        .expect("dense dynamic score: injected key without a slot"),
                    None => dense_score(d, leaf, scratch, rows, compiled_rows, tables),
                };
                if dense_check {
                    let mut cb = asm(base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()), &sp_f5);
                    let o = obj_score(&mut cb);
                    assert!(v == o || (v.is_nan() && o.is_nan()),
                            "dense/obj final score mismatch: {v:?} vs {o:?}");
                }
                v
            });
            return Ok(LeafOutcome::Scored(LeafResult { base_sp, total_sp, assigned_sp, score }));
        }
        // Rescue on the dense path (identical shift logic and checks).
        let rescued = phase!(MANA, {
            let (d, w) = dwork.as_mut().unwrap();
            let ok = dense_mana_rescue(d, w, &mut base_sp, &mut total_sp, &orig_base_sp,
                                       rows, registry, tables, consts, compiled);
            if dense_check {
                let bb = match &base_opt {
                    Some(b) => b,
                    None => { base_opt = Some(l2.build_base(item_names, weapon)?); base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap()) }
                };
                let mut b2 = saved_rescue_base;
                let mut t2 = saved_rescue_total;
                let o = mana_rescue(bb, l2, weapon, &mut b2, &mut t2, &orig_base_sp,
                                    rows, registry, hit_refs, tables, consts, compiled)?.is_some();
                assert_eq!(ok, o, "dense/obj rescue mismatch");
                assert_eq!((b2, t2), (base_sp, total_sp), "dense/obj rescue SP mismatch");
            }
            ok
        });
        if rescued {
            assert!(!doom_reject_expected, "doom precheck would have rejected a rescued leaf");
            {
                let (d2, w2) = dwork.as_mut().unwrap();
                if !d2.thresholds.is_empty()
                    && !dense_check_thresholds(d2, &w2.scratch, tables, consts) {
                    return Ok(LeafOutcome::ThresholdReject);
                }
            }
            let score = phase!(FINAL, {
                let (d, w) = dwork.as_mut().unwrap();
                let DenseWork { leaf, scratch, sim_memo, .. } = &mut **w;
                match dynamic {
                    Some(dy) => dense_dynamic_score(
                        d, leaf, scratch, sim_memo, rows, compiled_rows, tables, registry, consts,
                        dy)
                        .expect("dense dynamic score: injected key without a slot"),
                    None => dense_score(d, leaf, scratch, rows, compiled_rows, tables),
                }
            });
            return Ok(LeafOutcome::Scored(LeafResult { base_sp, total_sp, assigned_sp, score }));
        }
        return Ok(LeafOutcome::ManaReject);
    }
    let build_base = base_pre.unwrap_or_else(|| base_opt.as_ref().unwrap());
    let sp_f: Vec<f64> = sp_f5.to_vec();
    let mut combo_base = asm(build_base, &sp_f);
    if !thresholds.is_empty()
        && !check_thresholds_obj(&StatsView::Borrowed(&combo_base), thresholds, base_costs, tables, consts) {
        return Ok(LeafOutcome::ThresholdReject);
    }
    if phase!(MANA, !mana_check_passes(rows, &combo_base, registry, tables, consts, compiled)) {
        match mana_rescue(build_base, l2, weapon, &mut base_sp, &mut total_sp,
                          &orig_base_sp, rows, registry, hit_refs, tables, consts, compiled)? {
            Some(rescued) => {
                combo_base = rescued;
                if !thresholds.is_empty()
                    && !check_thresholds_obj(&StatsView::Borrowed(&combo_base), thresholds, base_costs, tables, consts) {
                    return Ok(LeafOutcome::ThresholdReject);
                }
            }
            None => return Ok(LeafOutcome::ManaReject),
        }
    }

    assert!(!doom_reject_expected, "doom precheck would have rejected a scored leaf (Obj path)");
    let score = phase!(FINAL, obj_score(&mut combo_base));
    Ok(LeafOutcome::Scored(LeafResult { base_sp, total_sp, assigned_sp, score }))
}

/// `eval_combo_damage_with_bp` (engine.js:332) for the dynamic-rows path.
///
/// Rebuilds this leaf's damage rows from its own simulation before scoring:
///
///   1. if the rows still carry loop markers, simulate to learn the
///      iteration counts, unroll by them, and recompute recast penalties on
///      the flat sequence (which casts are consecutive has changed);
///   2. simulate the flat rows and inject the resulting Blood Pact bonus and
///      buff-state slider values as boost tokens;
///   3. score the injected rows.
///
/// Step 2's simulation is over the *flat* rows so `row_results` lines up 1:1
/// with them, exactly as the JS does.
pub fn dynamic_damage_rows(
    combo_base: &Obj, rows: &[Row], registry: &[Value], tables: &Tables, consts: &L2Consts,
    dy: &DynamicRows,
) -> Vec<Row> {
    let mut extra: Vec<Vec<Token>> = Vec::new();
    let flat = dynamic_damage_rows_split(
        combo_base, rows, registry, tables, consts, dy, &mut extra);
    let mut out: Vec<Row> = flat.into_owned();
    for (r, e) in out.iter_mut().zip(extra) {
        r.tokens.extend(e);
    }
    out
}

/// `dynamic_damage_rows` with the injected tokens kept out of the rows.
///
/// Without a per-leaf unroll the rows are the parse-time ones and the boost
/// tokens are the only per-leaf part, so this hands back a borrow and fills
/// `extra` (one entry per row). Cloning the row set on every greedy trial was
/// the largest single cost in the dynamic path, and the compiled and dense
/// scorers take the injected tokens separately anyway — neither reads
/// `row.tokens`. An unroll changes the row count, so that case still owns.
///
/// `extra` is cleared and reused; it is empty when there is nothing to inject.
#[allow(clippy::too_many_arguments)]
pub fn dynamic_damage_rows_split<'a>(
    combo_base: &Obj, rows: &'a [Row], registry: &[Value], tables: &Tables, consts: &L2Consts,
    dy: &DynamicRows, extra: &mut Vec<Vec<Token>>,
) -> Cow<'a, [Row]> {
    extra.clear();
    let has_transcendence = combo_base.get("activeMajorIDs")
        .and_then(|v| v.get("__s")).and_then(|s| s.as_array())
        .map(|a| a.iter().any(|m| m.as_str() == Some("ARCANES")))
        .unwrap_or(false);
    let sim_consts = consts.sim_consts();
    let hc = &consts.health;

    let flat: Cow<'a, [Row]> = if dy.needs_unroll {
        let loop_sim = crate::mana_sim::simulate_combo_mana_hp(
            rows, combo_base, hc, has_transcendence, registry, tables, &sim_consts);
        let mut f = crate::mana_sim::unroll_loops_dynamic(rows, &loop_sim.loop_iteration_counts);
        crate::mana_sim::compute_recast_penalties(&mut f);
        Cow::Owned(f)
    } else {
        Cow::Borrowed(rows)
    };

    if dy.bp_slider.is_none() && dy.state_sliders.is_empty() {
        // Nothing to inject (the loop unroll was the only dynamic part).
        return flat;
    }
    let hp_sim = crate::mana_sim::simulate_combo_mana_hp(
        &flat, combo_base, hc, has_transcendence, registry, tables, &sim_consts);
    crate::mana_sim::blood_pact_extra_tokens(
        &flat, &hp_sim, dy.bp_slider.as_deref(), &dy.state_sliders, extra);
    flat
}

/// _mana_rescue: shift freely-assigned SP into Int in increasing fractions.
#[allow(clippy::too_many_arguments)]
pub fn mana_rescue(
    build_base: &Obj, l2: &Layer2, weapon: &Obj,
    base_sp: &mut [i32; 5], total_sp: &mut [i32; 5], orig_base_sp: &[i32; 5],
    rows: &[Row], registry: &[Value],
    hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables, consts: &L2Consts,
    compiled: Option<&[CompiledRow]>,
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
        let combo_base = l2.assemble_from_base(build_base, &sp_f, weapon);
        if mana_check_passes(rows, &combo_base, registry, tables, consts, compiled) {
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
    pub objective: Objective,
    pub compiled_rows: Vec<CompiledRow>,
    pub tables: Tables,
    pub weapon: Obj,
    pub rows: Vec<Row>,
    pub registry: Vec<Value>,
    pub hit_refs: HashMap<i64, HashMap<String, Obj>>,
    pub layer2: Layer2,
    pub consts: L2Consts,
    pub guild_unit: Option<crate::Unit>,
    /// Dense hot-path lowering; None when the scenario needs the Obj path.
    pub dense: Option<DenseCtx>,
    pub thresholds: Vec<Threshold>,
    pub spell_base_costs: HashMap<i64, f64>,
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
        let scoring_target = fixture["meta"].get("scoring_target")
            .and_then(|t| t.as_str()).unwrap_or("combo_damage").to_string();
        let custom_weights = fixture["layer2"].get("custom_weights").cloned();
        let objective = Objective::parse(&scoring_target, custom_weights.as_ref())?;
        // Count loops have a static iteration count, so they unroll here and
        // every downstream stage stays loop-free and fast. An until-OOM loop
        // cannot: its length depends on the leaf's own mana trajectory, so
        // the markers are kept and the rows are unrolled per leaf instead.
        let raw_rows = fixture["parsed_combo"].as_array().cloned().unwrap_or_default();
        let rows_value = match unroll_count_loops(&raw_rows) {
            Ok(unrolled) => Value::Array(unrolled),
            // until-OOM: keep the markers, `DynamicRows::needs_unroll` is set.
            Err(_) => Value::Array(raw_rows.clone()),
        };
        let rows_parsed = parse_rows(&rows_value);
        let registry_parsed: Vec<Value> = fixture["boost_registry"].as_array().cloned().unwrap_or_default();
        // Rows carrying loop markers are not a shape the compiler expects, and
        // a per-leaf unroll changes the row count, so those stay uncompiled.
        // A declared-slider scenario keeps its row list — injection only
        // *appends* tokens — so the load-time compiled rows stay valid and
        // only the injected tokens need per-leaf bonuses (see
        // `score_compiled_extra`).
        let needs_unroll = consts.dynamic.as_ref().map(|d| d.needs_unroll).unwrap_or(false);
        let compiled_rows = if needs_unroll { Vec::new() } else {
            compile_rows(&rows_parsed, &registry_parsed, &hit_refs)
        };
        let tables = Tables::parse(&fixture["tables"]);
        let weapon = as_map(&fixture["weapon_sm"]).ok_or("weapon_sm must be a map")?.clone();
        let thresholds = parse_thresholds(fixture);
        let spell_base_costs = parse_spell_base_costs(fixture);
        // A per-leaf unroll changes the row count, which the lowering cannot
        // follow. A declared slider only adds slot writes, so dense serves
        // those too — see `dense_dynamic_score`.
        let dense_blocked = consts.dynamic.as_ref().map(|d| d.needs_unroll).unwrap_or(false);
        let dense = if dense_blocked
            || std::env::var("SCORE_DENSE").as_deref() == Ok("0") { None } else {
            match dense_injectable_keys(consts.dynamic.as_ref(), &registry_parsed) {
                None => None,
                Some(extra_keys) =>
                    DenseCtx::build(&layer2, &tables, &weapon, &rows_parsed, &compiled_rows,
                                    &objective, &thresholds, &spell_base_costs, &extra_keys),
            }
        };
        Ok(ScoringCtx {
            objective,
            compiled_rows,
            tables,
            weapon,
            rows: rows_parsed,
            registry: registry_parsed,
            hit_refs,
            layer2,
            consts,
            guild_unit,
            dense,
            thresholds,
            spell_base_costs,
        })
    }
}

// ── Mid-tree damage ceiling bound (objective branch-and-bound) ──────────────
//
// The subtree bound mirrors the leaf ceiling gate one level up: damage is
// evaluated at all-150 SP on `prefix stats + per-stat maxima over every
// remaining slot's pool (+ a conservative set-bonus upper bound)`. Combo
// damage under the gate's verified conditions is non-decreasing in every
// additive stat it reads (per-element negatives clamp at 0; damMult factors
// only grow with their entries; atree var-effect factors are >= 0), so this
// upper-bounds every leaf ceiling in the subtree, which upper-bounds every
// leaf score. Pruning strictly below the cutoff is therefore admissible.

/// Additive per-stat maxima and set upper bounds for the bound builder.
pub struct BoundTables {
    /// suffix_max[d] = per-stat sum over slots d.. of each slot's pool max
    /// (only stats that appear on some item; damage reads of absent keys are
    /// 0 in the real assembly and 0 here).
    pub suffix_max: Vec<Obj>,
    /// Conservative upper bound on ADDITIONAL set-bonus stats reachable from
    /// free slots (per stat: sum over sets of the max positive per-count
    /// bonus). Added at every depth; overestimates, never underestimates.
    pub set_upper: Obj,
}

impl Layer2 {
    fn additive_item_stats(&self, item: &Obj, out: &mut HashMap<String, f64>) {
        // Mirrors add_item's key set: maxRolls (minus static ids) + static ids.
        if let Some(mr) = item.get("maxRolls").and_then(as_map) {
            for (id, value) in mr {
                if self.static_ids.iter().any(|s| s == id) { continue; }
                let v = value.as_f64().unwrap_or(0.0);
                let e = out.entry(id.clone()).or_insert(f64::NEG_INFINITY);
                if v > *e { *e = v; }
            }
        }
        for id in &self.static_ids {
            let v = item.get(id).and_then(|x| x.as_f64()).unwrap_or(0.0);
            let e = out.entry(id.clone()).or_insert(f64::NEG_INFINITY);
            if v > *e { *e = v; }
        }
    }

    /// Build the bound tables for an enumeration's slot pools (item names in
    /// enumeration order, rings included with their shared pool repeated).
    pub fn build_bound_tables(&self, slot_pools: &[Vec<String>]) -> Result<BoundTables, String> {
        let n = slot_pools.len();
        // Per-slot per-stat maxima (missing key on an item counts as 0 —
        // items without the key contribute 0 in the real sum).
        let mut per_slot: Vec<HashMap<String, f64>> = Vec::with_capacity(n);
        for pool in slot_pools {
            let mut maxima: HashMap<String, f64> = HashMap::new();
            for name in pool {
                let item = self.item_registry.get(name)
                    .ok_or_else(|| format!("bound: item not in registry: {}", name))?;
                self.additive_item_stats(item, &mut maxima);
            }
            // A stat absent on SOME item floors at 0 (choosing that item
            // contributes 0), so clamp maxima at >= 0 only when any pool item
            // lacks the key. Conservative shortcut: clamp at 0 always — an
            // upper bound stays an upper bound.
            for v in maxima.values_mut() {
                if *v < 0.0 { *v = 0.0; }
            }
            per_slot.push(maxima);
        }
        // Suffix sums.
        let mut suffix_max: Vec<Obj> = vec![Obj::new(); n + 1];
        for d in (0..n).rev() {
            let mut acc: Obj = suffix_max[d + 1].clone();
            for (k, v) in &per_slot[d] {
                let cur = acc.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                acc.insert(k.clone(), Value::from(cur + v));
            }
            suffix_max[d] = acc;
        }

        // Set-bonus upper bound: for each set reachable from any pool item,
        // add per-stat the max positive bonus over all counts.
        let mut set_names: Vec<&str> = Vec::new();
        for pool in slot_pools {
            for name in pool {
                if let Some(item) = self.item_registry.get(name) {
                    if let Some(s) = item.get("set").and_then(|v| v.as_str()) {
                        if !set_names.contains(&s) { set_names.push(s); }
                    }
                }
            }
        }
        let mut set_upper = Obj::new();
        for set_name in set_names {
            let Some(set_data) = self.sets_data.get(set_name) else { continue };
            let Some(bonuses) = set_data.get("bonuses").and_then(|b| b.as_array()) else { continue };
            let mut per_stat: HashMap<String, f64> = HashMap::new();
            for bonus in bonuses {
                let Some(bonus) = bonus.as_object() else { continue };
                for (id, v) in bonus {
                    let v = match v {
                        Value::Number(n) => n.as_f64().unwrap_or(0.0),
                        Value::Bool(b) => if *b { 1.0 } else { 0.0 },
                        _ => 0.0,
                    };
                    let e = per_stat.entry(id.clone()).or_insert(0.0);
                    if v > *e { *e = v; }
                }
            }
            for (id, v) in per_stat {
                if v <= 0.0 { continue; }
                let cur = set_upper.get(&id).and_then(|x| x.as_f64()).unwrap_or(0.0);
                set_upper.insert(id, Value::from(cur + v));
            }
        }

        Ok(BoundTables { suffix_max, set_upper })
    }

    /// Subtree damage ceiling: prefix items (by name, 8 slots with none-names
    /// for unplaced) + suffix maxima for slots `next_depth..` + set upper
    /// bound, assembled at all-150 SP.
    #[allow(clippy::too_many_arguments)]
    pub fn subtree_ceiling(
        &self, prefix_names: &[&str; 8], bounds: &BoundTables, next_depth: usize,
        weapon: &Obj, rows: &[Row], registry: &[Value],
        hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables,
        objective: &Objective, compiled: Option<&[CompiledRow]>,
    ) -> Result<f64, String> {
        let mut base = self.build_base(prefix_names, weapon)?;
        let add = |base: &mut Obj, delta: &Obj| {
            for (k, v) in delta {
                let dv = v.as_f64().unwrap_or(0.0);
                if dv == 0.0 { continue; }
                let cur = base.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                base.insert(k.clone(), Value::from(cur + dv));
            }
        };
        add(&mut base, &bounds.suffix_max[next_depth]);
        add(&mut base, &bounds.set_upper);
        let ceiling_sp = [150f64; 5];
        let mut cb = self.assemble_from_base(&base, &ceiling_sp, weapon);
        Ok(match compiled {
            Some(c) => objective.score_compiled(&mut cb, weapon, rows, c, tables),
            None => objective.score(&cb, weapon, rows, registry, hit_refs, tables),
        })
    }
}

// ── Generic scoring objectives ───────────────────────────────────────────────
//
// The solver optimizes whatever the scenario asks for — combo damage, an
// EHP-family stat, total mana, a plain stat, or a weighted custom blend.
// Every optimization that removes work (the leaf ceiling gate, the mid-tree
// bound, greedy trials) dispatches through this enum so nothing is
// hardcoded to damage. `supports_ceiling` marks objectives whose value is
// non-decreasing in every stat it reads (with SP at the 150 cap), which is
// what makes "evaluate at per-stat maxima" an admissible upper bound; the
// gate and bound simply stay off for objectives without that proof
// (correct, just unpruned).

pub enum Objective {
    ComboDamage,
    /// Sum of healing over the combo. Evaluated on the Obj path (the dense
    /// lowering covers damage/indirect objectives only), so it is correct
    /// but not on the fastest path.
    TotalHealing,
    /// ehp / ehp_no_agi / total_hp / hpr / ehpr / total_mana / plain stat.
    Indirect(String),
    /// Weighted blend; ceiling only when every weight is >= 0 and every
    /// sub-target supports a ceiling.
    Custom(Vec<(String, f64)>),
}

pub fn raw_to_pct(raw: f64, pct: f64) -> f64 {
    if raw < 0.0 {
        js_min(0.0, raw - raw * pct)
    } else if raw > 0.0 {
        raw + raw * pct
    } else {
        0.0
    }
}

pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else if a < b { a } else { b }
}

/// getDefenseStats subset: (total_hp, ehp, ehp_no_agi, hpr, ehpr).
/// computeSpellHealingTotal (pure/spell.js): sum of every part's heal
/// amount. Heal parts scale with max HP and the healMult map (entries
/// scoped with ':' apply only to their part); damage parts heal 0; total
/// parts sum their subs' heals times the effective hit count.
pub fn compute_spell_healing_total(stats: &StatsView, spell: &Value, tables: &Tables) -> f64 {
    let parts = spell_parts(spell);
    let base_spell = spell.get("base_spell").and_then(|v| v.as_i64()).unwrap_or(0);
    let total_hp = defense_stats(stats, tables).0;
    let heal_mult_map = stats.nested("healMult");

    fn eval<'a>(
        name: &str, parts: &'a [Value], base_spell: i64, total_hp: f64,
        heal_mult_map: Option<&Obj>, memo: &mut HashMap<String, f64>, depth: usize,
    ) -> f64 {
        if depth > 64 { return 0.0; }             // cycle guard
        if let Some(&v) = memo.get(name) { return v; }
        let Some(part) = parts.iter().find(|p| {
            p.get("name").and_then(|n| n.as_str()) == Some(name)
        }) else { return 0.0 };
        let part_id = format!("{}.{}", base_spell, name);
        let amount = if let Some(pct) = part.get("max_hp_heal_pct").and_then(|v| v.as_f64()) {
            let mut heal_mult = 1.0;
            if let Some(m) = heal_mult_map {
                for (k, v) in m {
                    let scoped_ok = match k.find(':') {
                        Some(i) => &k[i + 1..] == part_id,
                        None => true,
                    };
                    if scoped_ok {
                        heal_mult *= 1.0 + v.as_f64().unwrap_or(f64::NAN) / 100.0;
                    }
                }
            }
            pct * total_hp * heal_mult
        } else if part.get("multipliers").is_some() {
            0.0
        } else {
            let mut acc = 0.0;
            let tick_rounding = part.get("tick_rounding").and_then(|v| v.as_bool()).unwrap_or(false);
            if let Some(hits) = part.get("hits").and_then(|h| h.as_object()) {
                for (sub, hv) in hits {
                    let h = hv.as_f64().unwrap_or(f64::NAN);
                    let eff = if tick_rounding { 1.0 / ((1.0 / h * 20.0).floor() * 0.05) } else { h };
                    acc += eval(sub, parts, base_spell, total_hp, heal_mult_map, memo, depth + 1) * eff;
                }
            }
            acc
        };
        memo.insert(name.to_string(), amount);
        amount
    }

    let mut memo: HashMap<String, f64> = HashMap::new();
    let mut total = 0.0;
    for p in parts {
        let Some(n) = p.get("name").and_then(|x| x.as_str()) else { continue };
        total += eval(n, parts, base_spell, total_hp, heal_mult_map, &mut memo, 0);
    }
    total
}

/// eval_combo_healing: total_healing over the combo (mirrors the JS
/// accumulation, including melee-time effective quantities).
pub fn eval_combo_healing(
    combo_base: &Obj, weapon: &Obj, rows: &[Row], registry: &[Value],
    hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables,
) -> f64 {
    let mut total = 0.0;
    for row in rows {
        let Some(spell) = &row.spell else { continue };
        if row.qty <= 0.0 || row.pseudo { continue; }
        let (stats, po) = apply_combo_row_boosts(combo_base, &row.tokens, registry);
        let mod_spell = apply_spell_prop_overrides(spell, &po, hit_refs);
        let heal_per_cast = compute_spell_healing_total(&stats, &mod_spell, tables);
        let eff_qty = if row.is_melee_time {
            compute_melee_time_hits(row.qty, &StatsView::Borrowed(combo_base),
                                    row.melee_cd_override, tables, None)
        } else { row.qty };
        total += heal_per_cast * eff_qty;
    }
    total
}

pub fn defense_stats(stats: &StatsView, tables: &Tables) -> (f64, f64, f64, f64, f64) {
    let fm3 = tables.skillpoint_final_mult_3;
    let fm4 = tables.skillpoint_final_mult_4;
    let def_pct = tables.sp_to_pct(stats.num_or0("def")) * fm3;
    let agi_pct = tables.sp_to_pct(stats.num_or0("agi")) * fm4;
    let mut total_hp = stats.num_or0("hp") + stats.num_or0("hpBonus");
    if total_hp < 5.0 { total_hp = 5.0; }
    let mut def_mult = 2.0 - if stats.num("classDef").is_nan() { 1.0 } else { stats.num("classDef") };
    if let Some(dm) = stats.nested("defMult") {
        for (_, v) in dm {
            def_mult *= 1.0 - v.as_f64().unwrap_or(f64::NAN) / 100.0;
        }
    }
    let agi_reduction = (100.0 - stats.num_or0("agiDef")) / 100.0;
    let denom_full = agi_reduction * agi_pct + (1.0 - agi_pct) * (1.0 - def_pct);
    let ehp = total_hp / denom_full / def_mult;
    let ehp_no_agi = total_hp / ((1.0 - def_pct) * def_mult);
    let hpr = raw_to_pct(stats.num_or0("hprRaw"), stats.num_or0("hprPct") / 100.0);
    let ehpr = hpr / denom_full / def_mult;
    (total_hp, ehp, ehp_no_agi, hpr, ehpr)
}

pub fn eval_indirect_stat(stats: &StatsView, stat: &str, tables: &Tables) -> f64 {
    match stat {
        "ehp" => defense_stats(stats, tables).1,
        "ehp_no_agi" => defense_stats(stats, tables).2,
        "total_hp" => defense_stats(stats, tables).0,
        "hpr" => defense_stats(stats, tables).3,
        "ehpr" => defense_stats(stats, tables).4,
        "total_mana" => {
            let mm = stats.num_or0("maxMana");
            let int_mana = (tables.sp_to_pct(stats.num_or0("int")) * 100.0).floor();
            100.0 + mm + int_mana
        }
        other => stats.num_or0(other),
    }
}

impl Objective {
    pub fn parse(scoring_target: &str, custom_weights: Option<&Value>) -> Result<Objective, String> {
        match scoring_target {
            "combo_damage" => Ok(Objective::ComboDamage),
            "total_healing" => Ok(Objective::TotalHealing),
            "custom" => {
                let weights = custom_weights.and_then(|v| v.as_array())
                    .ok_or("custom objective without custom_weights")?;
                let mut out = Vec::new();
                for w in weights {
                    let target = w.get("target").and_then(|t| t.as_str())
                        .ok_or("custom weight missing target")?;
                    let weight = w.get("weight").and_then(|x| x.as_f64())
                        .ok_or("custom weight missing weight")?;
                    out.push((target.to_string(), weight));
                }
                Ok(Objective::Custom(out))
            }
            other => Ok(Objective::Indirect(other.to_string())),
        }
    }

    /// Leaf score — mirrors eval_score_dispatch.
    #[allow(clippy::too_many_arguments)]
    pub fn score(
        &self, combo_base: &Obj, weapon: &Obj, rows: &[Row], registry: &[Value],
        hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables,
    ) -> f64 {
        match self {
            Objective::ComboDamage =>
                eval_combo_damage(combo_base, weapon, rows, registry, hit_refs, tables),
            Objective::TotalHealing =>
                eval_combo_healing(combo_base, weapon, rows, registry, hit_refs, tables),
            Objective::Indirect(stat) =>
                eval_indirect_stat(&StatsView::Borrowed(combo_base), stat, tables),
            Objective::Custom(weights) => {
                let mut damage: Option<f64> = None;
                let stats = StatsView::Borrowed(combo_base);
                let mut sum = 0.0;
                for (target, weight) in weights {
                    let sub = if target == "combo_damage" {
                        *damage.get_or_insert_with(|| eval_combo_damage(
                            combo_base, weapon, rows, registry, hit_refs, tables))
                    } else if target == "total_healing" {
                        eval_combo_healing(combo_base, weapon, rows, registry, hit_refs, tables)
                    } else {
                        eval_indirect_stat(&stats, target, tables)
                    };
                    sum += weight * sub;
                }
                sum
            }
        }
    }

    /// `score` restricted to blend terms whose weight sign matches
    /// `want_neg` (see `dense_score_signed`).
    #[allow(clippy::too_many_arguments)]
    pub fn score_signed(
        &self, combo_base: &Obj, weapon: &Obj, rows: &[Row], registry: &[Value],
        hit_refs: &HashMap<i64, HashMap<String, Obj>>, tables: &Tables, want_neg: bool,
    ) -> f64 {
        let Objective::Custom(weights) = self else {
            return if want_neg { 0.0 }
                   else { self.score(combo_base, weapon, rows, registry, hit_refs, tables) };
        };
        let mut damage: Option<f64> = None;
        let stats = StatsView::Borrowed(combo_base);
        let mut sum = 0.0;
        for (target, weight) in weights {
            if (*weight < 0.0) != want_neg { continue; }
            let sub = if target == "combo_damage" {
                *damage.get_or_insert_with(|| eval_combo_damage(
                    combo_base, weapon, rows, registry, hit_refs, tables))
            } else if target == "total_healing" {
                eval_combo_healing(combo_base, weapon, rows, registry, hit_refs, tables)
            } else {
                eval_indirect_stat(&stats, target, tables)
            };
            sum += weight * sub;
        }
        sum
    }

    /// Hot-path score over precompiled rows (bit-identical to score()).
    pub fn score_compiled(
        &self, combo_base: &mut Obj, weapon: &Obj, rows: &[Row], compiled: &[CompiledRow],
        tables: &Tables,
    ) -> f64 {
        self.score_compiled_extra(combo_base, weapon, rows, compiled, tables, &[])
    }

    /// `score_compiled` with per-row bonuses for tokens appended after
    /// compilation — the per-leaf injected sliders. This is what lets a
    /// dynamic-row scenario keep the load-time compiled rows instead of
    /// falling back to the uncompiled scorer.
    #[allow(clippy::too_many_arguments)]
    pub fn score_compiled_extra(
        &self, combo_base: &mut Obj, weapon: &Obj, rows: &[Row], compiled: &[CompiledRow],
        tables: &Tables, extra: &[Vec<CompiledBonus>],
    ) -> f64 {
        match self {
            Objective::ComboDamage =>
                eval_combo_damage_compiled(combo_base, weapon, rows, compiled, tables, extra),
            Objective::TotalHealing =>
                eval_combo_healing_compiled(combo_base, rows, compiled, tables, extra),
            Objective::Indirect(stat) =>
                eval_indirect_stat(&StatsView::Borrowed(combo_base), stat, tables),
            Objective::Custom(weights) => {
                let mut damage: Option<f64> = None;
                let mut sum = 0.0;
                for (target, weight) in weights {
                    let sub = if target == "combo_damage" {
                        match damage {
                            Some(d) => d,
                            None => {
                                let d = eval_combo_damage_compiled(
                                    combo_base, weapon, rows, compiled, tables, extra);
                                damage = Some(d);
                                d
                            }
                        }
                    } else if target == "total_healing" {
                        eval_combo_healing_compiled(combo_base, rows, compiled, tables, extra)
                    } else {
                        eval_indirect_stat(&StatsView::Borrowed(combo_base), target, tables)
                    };
                    sum += weight * sub;
                }
                sum
            }
        }
    }

    /// Whether "score at per-stat maxima with SP=150" is an admissible upper
    /// bound for this objective (see module comment).
    pub fn supports_ceiling(&self) -> bool {
        match self {
            Objective::ComboDamage => true,
            // Healing is non-decreasing in max HP and the healMult entries it
            // reads, same argument as damage.
            Objective::TotalHealing => true,
            // All indirect stats are non-decreasing in the stats they read
            // (negative raw values are still bounded above by per-stat maxima).
            Objective::Indirect(_) => true,
            // A negative weight is still bounded, just not by the all-150
            // assemble alone — see `needs_two_sided_ceiling`.
            Objective::Custom(_) => true,
        }
    }

    /// True when the all-150-SP assemble is NOT on its own an upper bound.
    ///
    /// Every sub-target is non-decreasing in SP (the assumption
    /// `supports_ceiling` already rests on, backed by `ceiling_vars_ok`), so
    /// a term with a **negative** weight is maximized where its target is
    /// *smallest* — the leaf's pre-greedy SP — not largest. Evaluating each
    /// term at its own extreme still bounds the blend from above:
    ///
    ///   for w >= 0:  w * target(sp)  <=  w * target(150)
    ///   for w <  0:  w * target(sp)  <=  w * target(sp_min)
    ///
    /// since sp_min <= sp <= 150 componentwise for every state the greedy
    /// and the mana rescue can reach. Summing preserves the inequality.
    ///
    /// Without this these scenarios ran with the gate and every bound off.
    pub fn needs_two_sided_ceiling(&self) -> bool {
        match self {
            Objective::Custom(weights) => weights.iter().any(|(_, w)| *w < 0.0),
            _ => false,
        }
    }

    /// `SCORE_TWO_SIDED=0` restores the previous behaviour — no gate at all
    /// on these objectives — so the pruned and unpruned runs can be compared
    /// directly. An admissible bound must leave the top-N unchanged.
    pub fn two_sided_enabled() -> bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| std::env::var("SCORE_TWO_SIDED").as_deref() != Ok("0"))
    }
}

// ── Compiled row boosts (P2.4 layer 4, step 1) ──────────────────────────────
//
// A combo row's boost tokens, the registry entries they match, every
// bonus's rounded/capped contribution, and the prop overrides they produce
// are all CONSTANT per row — only the base stats they apply to vary. This
// lowers apply_combo_row_boosts + apply_spell_prop_overrides to a
// precomputed delta list and a pre-patched spell, applied in exactly the
// original (token, match, bonus) order so the float sequence — and thus
// every result bit — is unchanged.

#[derive(PartialEq, Clone, Copy)]
pub enum CompiledBonusKind { Stat, DamMult, DefMult }

pub struct CompiledBonus {
    pub kind: CompiledBonusKind,
    /// Stat key, or the damMult./defMult. sub-key.
    pub key: String,
    /// Fully rounded/capped contribution (constant per row).
    pub contrib: f64,
    /// max-merge semantics (mode == "max", or Potion/Vulnerability sub-key).
    pub use_max: bool,
}

pub struct CompiledRow {
    pub bonuses: Vec<CompiledBonus>,
    /// Cost-stat deltas for row_unclamped_spell_cost (which: 0=int, 1=spRaw,
    /// 2=spPct, 3=spPctFinal; contrib; use_max) and the row's cost key names.
    pub cost_bonuses: Vec<(u8, f64, bool)>,
    pub cost_keys: Option<(String, String, String)>,
    /// The spell with prop overrides already applied (or the original).
    pub mod_spell: Option<Value>,
    /// DPS analysis of mod_spell (always valid — overrides are constant).
    pub dps: Option<(String, f64, String)>,
    pub fallback_root: Option<String>,
    /// Compiled structural plan of mod_spell (None → dynamic fallback).
    pub plan: Option<SpellPlan>,
}

/// Compiles one token list's registry matches into stat deltas and prop
/// overrides, in the exact (token, match, bonus) order the uncompiled path
/// applies them — so appending a later token's bonuses is equivalent to
/// having compiled the longer token list from the start.
///
/// Shared by `compile_rows` and the per-leaf injected-token path, so the two
/// cannot drift.
pub fn compile_token_bonuses(
    tokens: &[Token], registry: &[Value],
    bonuses: &mut Vec<CompiledBonus>, prop_overrides: &mut HashMap<String, PropOverride>,
) {
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
            let (kind, sub) = if let Some(sub) = key.strip_prefix("damMult.") {
                (CompiledBonusKind::DamMult, sub)
            } else if let Some(sub) = key.strip_prefix("defMult.") {
                (CompiledBonusKind::DefMult, sub)
            } else {
                (CompiledBonusKind::Stat, key)
            };
            let use_max = match kind {
                CompiledBonusKind::Stat => mode_max,
                _ => mode_max || sub == "Potion" || sub == "Vulnerability",
            };
            bonuses.push(CompiledBonus { kind, key: sub.to_string(), contrib, use_max });
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
}

pub fn compile_rows(
    rows: &[Row], registry: &[Value],
    hit_refs: &HashMap<i64, HashMap<String, Obj>>,
) -> Vec<CompiledRow> {
    rows.iter().map(|row| {
        let mut bonuses = Vec::new();
        let mut prop_overrides: HashMap<String, PropOverride> = HashMap::new();
        compile_token_bonuses(&row.tokens, registry, &mut bonuses, &mut prop_overrides);
        let mut cost_bonuses: Vec<(u8, f64, bool)> = Vec::new();
        let mut cost_keys: Option<(String, String, String)> = None;
        if let Some(spell) = &row.spell {
            if spell.get("cost").map(|c| !c.is_null()).unwrap_or(false) {
                let bs = spell.get("mana_derived_from").and_then(|v| v.as_i64())
                    .or_else(|| spell.get("base_spell").and_then(|v| v.as_i64())).unwrap_or(0);
                let k_raw = format!("spRaw{}", bs);
                let k_pct = format!("spPct{}", bs);
                let k_final = format!("spPct{}Final", bs);
                for token in &row.tokens {
                    for (entry, effective_value) in find_all_matching_boosts(token, registry) {
                        for b in entry.get("stat_bonuses").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
                            let key = b.get("key").and_then(|k| k.as_str()).unwrap_or("");
                            let which = if key == "int" { 0u8 }
                                else if key == k_raw { 1 }
                                else if key == k_pct { 2 }
                                else if key == k_final { 3 }
                                else { continue };
                            let mut contrib = b.get("value").and_then(|v| v.as_f64()).unwrap_or(f64::NAN) * effective_value;
                            if b.get("round").and_then(|v| v.as_bool()) != Some(false) {
                                contrib = round_near(contrib).floor();
                            }
                            if let Some(mx) = b.get("max").and_then(|v| v.as_f64()) {
                                if mx > 0.0 && contrib > mx { contrib = mx; }
                                else if mx < 0.0 && contrib < mx { contrib = mx; }
                            }
                            let use_max = b.get("mode").and_then(|m| m.as_str()) == Some("max");
                            cost_bonuses.push((which, contrib, use_max));
                        }
                    }
                }
                cost_keys = Some((k_raw, k_pct, k_final));
            }
        }
        let mod_spell: Option<Value> = row.spell.as_ref().map(|spell| {
            apply_spell_prop_overrides(spell, &prop_overrides, hit_refs).into_owned()
        });
        let dps = mod_spell.as_ref()
            .and_then(compute_dps_spell_hits_info)
            .map(|i| (i.per_hit_name, i.max_hits, i.dps_chain_root));
        let fallback_root = mod_spell.as_ref().and_then(|s| {
            if spell_is_dps(s) { find_dps_display_root(s) } else { None }
        });
        let plan = mod_spell.as_ref().and_then(|sp| {
            // DPS display precedence mirrors the eval: explicit per-row name
            // first, then the spell's own DPS analysis.
            let eff_dps = row.dps_per_hit_name.clone()
                .map(|n| (n, 0.0, String::new()))
                .or_else(|| dps.clone());
            compile_spell_plan(sp, &eff_dps, &fallback_root)
        });
        CompiledRow { bonuses, cost_bonuses, cost_keys, mod_spell, dps, fallback_root, plan }
    }).collect()
}

/// row_unclamped_spell_cost over compiled cost deltas — identical floats.
pub fn row_cost_compiled(
    base_stats: &Obj, spell: &Value, comp: &CompiledRow, tables: &Tables, consts: &L2Consts,
) -> f64 {
    let sv = StatsView::Borrowed(base_stats);
    let (k_raw, k_pct, k_final) = comp.cost_keys.as_ref().expect("cost keys");
    let v = [
        sv.num_or0("int"),
        sv.num_or0(k_raw),
        sv.num_or0(k_pct),
        sv.num_or0(k_final),
    ];
    let spell_cost = spell.get("cost").and_then(|c| c.as_f64()).unwrap_or(f64::NAN);
    row_cost_from_inputs(v, comp, spell_cost, tables, consts)
}

/// The cost arithmetic itself, over already-fetched inputs
/// `v = [int, spRaw, spPct, spPctFinal]` and the spell's base cost.
///
/// Split out so the map-reading and the dense-slot-reading callers run the
/// *same* float sequence — the two differ only in how the four numbers and
/// the base cost are obtained, never in what is done with them.
#[inline]
fn row_cost_from_inputs(
    mut v: [f64; 4], comp: &CompiledRow, spell_cost: f64, tables: &Tables, consts: &L2Consts,
) -> f64 {
    for (which, contrib, use_max) in &comp.cost_bonuses {
        let i = *which as usize;
        v[i] = if *use_max { js_max(v[i], *contrib) } else { v[i] + *contrib };
    }
    let int_reduction = tables.sp_to_pct(v[0]) * consts.skillpoint_final_mult_2;
    let mut cost = spell_cost * (1.0 - int_reduction);
    cost += v[1];
    cost *= 1.0 + v[2] / 100.0;
    cost * (1.0 + v[3] / 100.0)
}

/// apply_combo_row_boosts with a precompiled delta list — identical float
/// sequence, no matching or rounding at eval time.
pub fn apply_compiled_boosts<'a>(base: &'a Obj, compiled: &CompiledRow) -> StatsView<'a> {
    if compiled.bonuses.is_empty() && base.contains_key("damMult") && base.contains_key("defMult") {
        return StatsView::Borrowed(base);
    }
    let mut stats = base.clone();
    let mut dam_mult: Obj = base.get("damMult").and_then(as_map).cloned().unwrap_or_default();
    let mut def_mult: Obj = base.get("defMult").and_then(as_map).cloned().unwrap_or_default();
    for b in &compiled.bonuses {
        let target = match b.kind {
            CompiledBonusKind::Stat => &mut stats,
            CompiledBonusKind::DamMult => &mut dam_mult,
            CompiledBonusKind::DefMult => &mut def_mult,
        };
        let cur = target.get(&b.key).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let nv = if b.use_max { js_max(cur, b.contrib) } else { cur + b.contrib };
        target.insert(b.key.clone(), Value::from(nv));
    }
    let wrap = |m: Obj| { let mut w = Obj::new(); w.insert("__m".into(), Value::Object(m)); Value::Object(w) };
    stats.insert("damMult".into(), wrap(dam_mult));
    stats.insert("defMult".into(), wrap(def_mult));
    StatsView::Owned(stats)
}

/// Apply a row's compiled deltas in place with an undo journal. Existing
/// keys keep their positions and new keys append — the same layout the
/// clone-and-extend path produced — and rollback restores the exact
/// original bits, so results are bit-identical while touching only
/// len(bonuses) entries instead of cloning ~200.
pub fn with_row_overlay<R>(
    base: &mut Obj, comp: &CompiledRow, extra: &[CompiledBonus], f: impl FnOnce(&Obj) -> R,
) -> R {
    if comp.bonuses.is_empty() && extra.is_empty() {
        return f(base);
    }
    // (kind, key, previous value or None-if-absent)
    let mut journal: Vec<(u8, &str, Option<Value>)> =
        Vec::with_capacity(comp.bonuses.len() + extra.len());
    for b in comp.bonuses.iter().chain(extra) {
        let (kind_tag, target): (u8, &mut Obj) = match b.kind {
            CompiledBonusKind::Stat => (0, base),
            CompiledBonusKind::DamMult => (1, base.get_mut("damMult")
                .and_then(|v| v.get_mut("__m")).and_then(|m| m.as_object_mut())
                .expect("damMult wrap")),
            CompiledBonusKind::DefMult => (2, base.get_mut("defMult")
                .and_then(|v| v.get_mut("__m")).and_then(|m| m.as_object_mut())
                .expect("defMult wrap")),
        };
        let prev = target.get(&b.key).cloned();
        let cur = prev.as_ref().and_then(|v| v.as_f64()).unwrap_or(0.0);
        let nv = if b.use_max { js_max(cur, b.contrib) } else { cur + b.contrib };
        target.insert(b.key.clone(), Value::from(nv));
        journal.push((kind_tag, b.key.as_str(), prev));
    }
    let r = f(base);
    for (kind_tag, key, prev) in journal.into_iter().rev() {
        let target: &mut Obj = match kind_tag {
            0 => base,
            1 => base.get_mut("damMult").and_then(|v| v.get_mut("__m"))
                .and_then(|m| m.as_object_mut()).expect("damMult wrap"),
            _ => base.get_mut("defMult").and_then(|v| v.get_mut("__m"))
                .and_then(|m| m.as_object_mut()).expect("defMult wrap"),
        };
        match prev {
            Some(v) => { target.insert(key.to_string(), v); }
            None => { target.shift_remove(key); }
        }
    }
    r
}

/// eval_combo_healing over precompiled rows: uses each row's pre-patched
/// mod_spell and journaled boost overlay, exactly like the damage variant,
/// so results match the uncompiled path bit-for-bit.
/// `extra[i]` holds bonuses for tokens appended to row `i` after compilation
/// (the per-leaf injected sliders). Empty for every static scenario.
pub fn eval_combo_healing_compiled(
    combo_base: &mut Obj, rows: &[Row], compiled: &[CompiledRow], tables: &Tables,
    extra: &[Vec<CompiledBonus>],
) -> f64 {
    let mut total = 0.0;
    for (i, (row, comp)) in rows.iter().zip(compiled).enumerate() {
        let extra_i: &[CompiledBonus] = extra.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
        let Some(mod_spell) = &comp.mod_spell else { continue };
        if row.qty <= 0.0 || row.pseudo { continue; }
        let heal_per_cast = with_row_overlay(combo_base, comp, extra_i, |b| {
            compute_spell_healing_total(&StatsView::Borrowed(b), mod_spell, tables)
        });
        let eff_qty = if row.is_melee_time {
            let bv = StatsView::Borrowed(combo_base);
            compute_melee_time_hits(row.qty, &bv, row.melee_cd_override, tables, None)
        } else { row.qty };
        total += heal_per_cast * eff_qty;
    }
    total
}

/// eval_combo_damage over precompiled rows — the hot-path variant.
/// See `eval_combo_healing_compiled` for `extra`.
pub fn eval_combo_damage_compiled(
    combo_base: &mut Obj, weapon: &Obj, rows: &[Row], compiled: &[CompiledRow], tables: &Tables,
    extra: &[Vec<CompiledBonus>],
) -> f64 {
    let crit = {
        let bv = StatsView::Borrowed(combo_base);
        let dex = bv.num("dex");
        tables.sp_to_pct(if dex.is_nan() || dex == 0.0 { 0.0 } else { dex })
    };

    let mut total_damage = 0.0;
    for (i, (row, comp)) in rows.iter().zip(compiled).enumerate() {
        let extra_i: &[CompiledBonus] = extra.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
        let Some(mod_spell) = &comp.mod_spell else { continue };
        if row.qty <= 0.0 || row.pseudo { continue; }

        let mut eff_dps_name: Option<&str> = row.dps_per_hit_name.as_deref();
        let mut eff_dps_hits = row.dps_hits;
        let mut chain_root: Option<&str> = None;
        if eff_dps_name.is_none() {
            if let Some((name, hits, root)) = &comp.dps {
                eff_dps_name = Some(name);
                eff_dps_hits = row.dps_hits_override.unwrap_or(*hits);
                chain_root = Some(root);
            }
        }
        let final_root: Option<&str> = chain_root.or(comp.fallback_root.as_deref());

        let (per_cast, flat_per_cast) = with_row_overlay(combo_base, comp, extra_i, |b| {
            let stats = StatsView::Borrowed(b);
            if let Some(plan) = &comp.plan {
                let (mut per_cast, flat) = eval_spell_plan(
                    &stats, weapon, plan, crit, tables, eff_dps_name.is_some());
                if eff_dps_name.is_some() { per_cast *= eff_dps_hits; }
                // flat only applies when a chain root exists (same as dynamic)
                let flat = if final_root.is_some() { flat } else { 0.0 };
                (per_cast, flat)
            } else {
                let per_cast = match eff_dps_name {
                    Some(name) => compute_spell_display_avg(&stats, weapon, mod_spell, crit, tables, Some(name)) * eff_dps_hits,
                    None => compute_spell_display_avg(&stats, weapon, mod_spell, crit, tables, None),
                };
                let mut flat_per_cast = 0.0;
                if let Some(root) = final_root {
                    flat_per_cast = compute_spell_flat_damage(&stats, weapon, mod_spell, crit, root, tables);
                }
                (per_cast, flat_per_cast)
            }
        });

        let eff_qty = if row.is_melee_time {
            let bv = StatsView::Borrowed(combo_base);
            compute_melee_time_hits(row.qty, &bv, row.melee_cd_override, tables, None)
        } else { row.qty };
        let row_damage = if row.dmg_excl { 0.0 } else { per_cast * eff_qty + flat_per_cast };
        total_damage += row_damage;
    }
    total_damage
}

// ── Compiled spell plans (P2.4 layer 4, step 3) ─────────────────────────────
//
// Everything structural about a row's (constant) mod_spell is resolved at
// compile time: parts lowered to flat structs with precomputed multipliers,
// part ids, and part-scoped ConvBase key names; hit edges resolved to part
// indices; part KINDS resolved statically (multipliers → damage,
// max_hp_heal_pct → heal, total → first sub's kind — the same inference the
// dynamic path performs); the display part index (find_display_result) and
// the flat-damage contributor set (given the row's constant DPS chain root)
// precomputed. Evaluation walks arrays with an indexed memo and computes
// display-avg and flat damage from ONE parts pass (the dynamic path
// evaluates parts twice on DPS rows; identical inputs give identical
// results, so the shared pass is bit-exact).

pub struct PartDamagePlan {
    pub multipliers: Vec<f64>,
    pub use_str: bool,
    pub ignored_mults: Vec<String>,
    pub part_id: String,
    pub conv_names: [String; 6],
}

pub enum PartKindPlan {
    Damage(PartDamagePlan),
    Heal,
    /// (sub part index, hits, tick_rounding)
    Total(Vec<(usize, f64, bool)>),
}

pub struct PartPlan {
    pub kind: PartKindPlan,
    pub display: bool,
    /// Statically inferred result kind: "damage" | "heal" | none.
    pub static_kind: Option<&'static str>,
}

pub struct SpellPlan {
    pub parts: Vec<PartPlan>,
    pub use_speed: bool,
    pub use_spell: bool,
    /// find_display_result resolved statically (index into parts).
    pub display_idx: Option<usize>,
    /// Per-hit display override target for DPS rows (index into parts).
    pub dps_display_idx: Option<usize>,
    /// Flat-damage contributors (displayed damage roots, minus the DPS
    /// chain root), resolved statically.
    pub flat_idxs: Vec<usize>,
}

pub fn compile_spell_plan(spell: &Value, comp_dps: &Option<(String, f64, String)>,
                          fallback_root: &Option<String>) -> Option<SpellPlan> {
    let parts_json = spell_parts(spell);
    if parts_json.is_empty() { return None; }
    let use_speed = spell.get("use_atkspd").and_then(|v| v.as_bool()).unwrap_or(true);
    let use_spell = spell.get("scaling").and_then(|v| v.as_str()).unwrap_or("spell") == "spell";
    let base_spell = spell.get("base_spell").and_then(|v| v.as_i64()).unwrap_or(0);

    let names: Vec<&str> = parts_json.iter()
        .map(|p| p.get("name").and_then(|n| n.as_str()).unwrap_or(""))
        .collect();
    let idx_of = |n: &str| -> Option<usize> { names.iter().position(|x| *x == n) };

    // Static kind inference (mirrors eval_part's result.type assignment).
    fn static_kind_of(i: usize, parts: &[Value], names: &[&str], seen: &mut Vec<usize>) -> Option<&'static str> {
        if seen.contains(&i) { return None; }
        seen.push(i);
        let p = &parts[i];
        if p.get("multipliers").is_some() { return Some("damage"); }
        if p.get("max_hp_heal_pct").is_some() { return Some("heal"); }
        if let Some(hits) = p.get("hits").and_then(|h| h.as_object()) {
            for sub in hits.keys() {
                if let Some(j) = names.iter().position(|x| x == sub) {
                    if let Some(k) = static_kind_of(j, parts, names, seen) { return Some(k); }
                } else {
                    // Missing sub: dynamic eval_part returns None and the
                    // total's kind falls through to the next sub.
                    continue;
                }
            }
            return None;
        }
        None
    }

    let mut parts = Vec::with_capacity(parts_json.len());
    for (i, p) in parts_json.iter().enumerate() {
        let kind = if let Some(mults) = p.get("multipliers") {
            let part_id = format!("{}.{}", base_spell, names[i]);
            let conv_names = std::array::from_fn(|e| {
                format!("{}ConvBase:{}", DAMAGE_ELEMENTS[e], part_id)
            });
            PartKindPlan::Damage(PartDamagePlan {
                multipliers: arr_f64(mults),
                use_str: p.get("use_str").and_then(|v| v.as_bool()).unwrap_or(true),
                ignored_mults: p.get("ignored_mults").and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                part_id,
                conv_names,
            })
        } else if p.get("max_hp_heal_pct").is_some() {
            PartKindPlan::Heal
        } else if let Some(hits) = p.get("hits").and_then(|h| h.as_object()) {
            let tick_rounding = p.get("tick_rounding").and_then(|v| v.as_bool()).unwrap_or(false);
            let mut edges = Vec::with_capacity(hits.len());
            for (sub, v) in hits {
                if let Some(j) = idx_of(sub) {
                    edges.push((j, v.as_f64().unwrap_or(f64::NAN), tick_rounding));
                }
            }
            PartKindPlan::Total(edges)
        } else {
            PartKindPlan::Total(Vec::new())
        };
        parts.push(PartPlan {
            kind,
            display: p.get("display").and_then(|v| v.as_bool()).unwrap_or(true),
            static_kind: static_kind_of(i, parts_json, &names, &mut Vec::new()),
        });
    }

    // display_idx: find_display_result with static kinds.
    let display_name = spell.get("display").and_then(|d| d.as_str());
    let mut display_idx = display_name.and_then(idx_of)
        .filter(|&i| parts[i].static_kind == Some("damage"));
    if display_idx.is_none() {
        let dps_name = find_dps_root_name(spell);
        display_idx = dps_name.as_deref().and_then(idx_of)
            .or_else(|| (0..parts.len()).rev()
                .find(|&i| parts[i].display && parts[i].static_kind == Some("damage")));
    }

    let dps_display_idx = comp_dps.as_ref().and_then(|(name, _, _)| idx_of(name));

    // flat_idxs: displayed, unreferenced damage parts minus the chain root.
    // An explicit per-row DPS name arrives as (name, 0.0, "") — the empty
    // string is a SENTINEL for "no chain root", not a root. Taking it at face
    // value made this Some("") and short-circuited the .or(), so the flat sum
    // excluded a part named "" (nothing) and the real root's own damage was
    // counted as flat on top of the per-hit total. The eval path models the
    // same case as chain_root = None and falls through to the fallback root,
    // so any combo row with dps_per_hit_name set (e.g. Guardian Angels,
    // "Single Shot" x90) was silently over-scored by the root's contribution
    // on the compiled and dense paths — +3.85% on the spell_sustained family,
    // where the uncompiled path matches the JS engine exactly.
    let exclude_root: Option<&str> = comp_dps.as_ref().map(|(_, _, r)| r.as_str())
        .filter(|r| !r.is_empty())
        .or(fallback_root.as_deref());
    let referenced = collect_referenced_part_names(spell);
    let flat_idxs = if exclude_root.is_some() {
        (0..parts.len()).filter(|&i| {
            parts[i].static_kind == Some("damage") && parts[i].display
                && Some(names[i]) != exclude_root
                && !referenced.iter().any(|r| r == names[i])
        }).collect()
    } else { Vec::new() };

    Some(SpellPlan { parts, use_speed, use_spell, display_idx, dps_display_idx, flat_idxs })
}

/// One-pass evaluation of a compiled plan: (per_cast_avg, flat_damage).
pub fn eval_spell_plan(
    stats: &StatsView, weapon: &Obj, plan: &SpellPlan, crit: f64, tables: &Tables,
    use_dps_display: bool,
) -> (f64, f64) {
    let n = plan.parts.len();
    let mut memo: Vec<Option<[f64; 4]>> = vec![None; n]; // norm0 norm1 crit0 crit1

    fn eval(i: usize, plan: &SpellPlan, memo: &mut Vec<Option<[f64; 4]>>,
            stats: &StatsView, weapon: &Obj, crit_unused: f64, tables: &Tables) -> [f64; 4] {
        if let Some(r) = memo[i] { return r; }
        let r = match &plan.parts[i].kind {
            PartKindPlan::Damage(d) => {
                let (norm, crit_t) = calculate_spell_damage_pc(
                    stats, weapon, &d.multipliers, plan.use_spell, !plan.use_speed,
                    Some(&d.part_id), !d.use_str, &d.ignored_mults, tables,
                    Some(&d.conv_names));
                [norm[0], norm[1], crit_t[0], crit_t[1]]
            }
            PartKindPlan::Heal => [0.0; 4],
            PartKindPlan::Total(edges) => {
                let mut acc = [0.0f64; 4];
                for (j, hits, tick_rounding) in edges {
                    let sub = eval(*j, plan, memo, stats, weapon, crit_unused, tables);
                    if plan.parts[*j].static_kind == Some("damage") {
                        let eff = if *tick_rounding {
                            1.0 / ((1.0 / hits * 20.0).floor() * 0.05)
                        } else { *hits };
                        for k in 0..4 { acc[k] += sub[k] * eff; }
                    }
                }
                acc
            }
        };
        memo[i] = Some(r);
        r
    }

    let display = if use_dps_display { plan.dps_display_idx } else { plan.display_idx };
    let per_cast = match display {
        Some(i) if plan.parts[i].static_kind == Some("damage") => {
            let r = eval(i, plan, &mut memo, stats, weapon, crit, tables);
            let non_crit_avg = (r[0] + r[1]) / 2.0;
            let crit_avg = (r[2] + r[3]) / 2.0;
            (1.0 - crit) * non_crit_avg + crit * crit_avg
        }
        _ => 0.0,
    };
    let mut flat = 0.0;
    for &i in &plan.flat_idxs {
        let r = eval(i, plan, &mut memo, stats, weapon, crit, tables);
        let non_crit_avg = (r[0] + r[1]) / 2.0;
        let crit_avg = (r[2] + r[3]) / 2.0;
        flat += (1.0 - crit) * non_crit_avg + crit * crit_avg;
    }
    (per_cast, flat)
}

// ── Dense stat vectors (P2.4 layer 4, final step) ────────────────────────────
//
// The hot path (ceiling gate + greedy trials) spends its time in string-keyed
// IndexMap lookups and per-trial clones of the JSON stat map. This module
// lowers the whole trial pipeline onto a flat f64 vector:
//   - load time (DenseCtx): a key→index universe over every stat name the
//     scoring path can read or write, weapon constants hoisted, var effects
//     lowered to index programs, row bonus deltas and plan ConvBase names
//     index-resolved;
//   - per leaf (DenseLeaf): base + classDef + atree_raw + const_scaled +
//     static_boosts folded into one dense vector (constant merges keep their
//     per-key addition order, so folding is bit-exact; the one order-breaking
//     case — a key written by both a var effect and static_boosts — is
//     rejected at load and falls back to the Obj path);
//   - per trial: memcpy the leaf vector, write the five skillpoint values
//     through their ordered add chains, run var effects as pure arithmetic,
//     and score with an index-resolved mirror of the damage core.
// Every float operation replays the Obj path in the same order, so scores are
// bit-identical; SCORE_DENSE_CHECK=1 asserts that per trial at runtime, and
// score_kernel validates it per fixture case.

#[derive(Clone, Copy)]
pub enum MultTarget {
    All,
    MeleeOnly, // ";m" — damage_mult only when !use_spell_damage
    Ele(usize),
    Inert, // parse matched nothing → entry never applies
}

#[derive(Clone)]
pub struct DMultEntry {
    pub key: std::sync::Arc<str>,
    pub spell_match: Option<std::sync::Arc<str>>,
    pub target: MultTarget,
}

pub fn parse_mult_entry(key: &str) -> DMultEntry {
    let spell_match = key.find(':').map(|c| std::sync::Arc::<str>::from(&key[c + 1..]));
    let target = match key.find(';') {
        Some(semi) => {
            let tail = &key[semi + 1..];
            if tail == "m" { MultTarget::MeleeOnly }
            else if let Some(i) = DAMAGE_ELEMENTS.iter().position(|e| *e == tail) { MultTarget::Ele(i) }
            else { MultTarget::Inert }
        }
        None => MultTarget::All,
    };
    DMultEntry { key: std::sync::Arc::from(key), spell_match, target }
}

pub enum DTerm {
    Skp(usize, f64),       // (skp index, factor) — reads the pre-const value
    Const(usize, f64),     // (leaf const slot, factor)
}

pub struct DVarEffect {
    pub const_add: f64,
    pub terms: Vec<DTerm>,
    pub round: bool,
    pub positive: bool,
    pub max: Option<f64>,
    /// (var slot, first write for that slot this trial)
    pub out_slots: Vec<(usize, bool)>,
}

pub enum DInd {
    Ehp, EhpNoAgi, TotalHp, Hpr, Ehpr, TotalMana, Plain(u32),
}

pub enum DObjective {
    Damage,
    Indirect(DInd),
    Custom(Vec<(Option<DInd>, f64)>), // None → combo_damage
}

pub struct DRow {
    pub stat_ops: Vec<(u32, f64, bool)>,          // (idx, contrib, use_max)
    pub dam_ops: Vec<(DMultEntry, f64, bool)>,    // key resolved at apply time
    pub def_ops: Vec<(DMultEntry, f64, bool)>,
    /// Per plan part: resolved part-scoped ConvBase indices (Damage parts).
    pub parts_conv: Vec<Option<[u32; 6]>>,
}

pub struct DenseCtx {
    pub idx: HashMap<String, u32>,
    pub n: usize,
    pub skp_idx: [u32; 5],
    pub class_def_idx: u32,
    pub dex_idx: u32,
    pub atk_tier_idx: u32,
    pub conv_base_idx: [u32; 6],
    pub dam_add_min_idx: [u32; 6],
    pub dam_add_max_idx: [u32; 6],
    pub sd_pct_idx: [u32; 6],
    pub md_pct_idx: [u32; 6],
    pub dam_pct_idx: [u32; 6],
    pub sd_raw_idx: [u32; 6],
    pub md_raw_idx: [u32; 6],
    pub dam_raw_idx: [u32; 6],
    // scalar ids (sdPct/damPct/... variants used by the ID-bonus step)
    pub s_sd_pct: u32, pub s_md_pct: u32, pub s_dam_pct: u32,
    pub s_r_sd_pct: u32, pub s_r_md_pct: u32, pub s_r_dam_pct: u32,
    pub s_sd_raw: u32, pub s_md_raw: u32, pub s_dam_raw: u32,
    pub s_r_sd_raw: u32, pub s_r_md_raw: u32, pub s_r_dam_raw: u32,
    pub s_crit_dam_pct: u32,
    // defense / indirect ids
    pub s_def: u32, pub s_agi: u32, pub s_hp: u32, pub s_hp_bonus: u32,
    pub s_agi_def: u32, pub s_hpr_raw: u32, pub s_hpr_pct: u32,
    pub s_max_mana: u32, pub s_int: u32,
    /// Every stat key the fast mana sim / cost path reads, for the mini
    /// stat map materialized from the dense assembled state.
    pub mana_keys: Vec<(String, u32)>,
    /// The same reads resolved to dense slots, so the sim can take its
    /// inputs off the assembled vector instead of a String-keyed map.
    pub mana_idx: ManaIdx,
    pub atk_spd_str: Option<String>,
    /// Per var slot: mana direction for the bounded doom precheck.
    /// +1 = favorable-if-higher (credit t_max), -1 = favorable-if-lower
    /// (credit t_min), 0 = irrelevant to the mana sim.
    pub var_slot_mana_dir: Vec<i8>,
    /// Var outputs couple into start-mana stats (maxMana/int) — bounded
    /// doom is only sound then under allow_downtime.
    pub doom_couples_start: bool,
    /// Var outputs couple into atkTier — bounded doom unavailable.
    pub doom_couples_tier: bool,
    /// Restriction thresholds lowered onto the read universe: (kind, ge, value).
    pub thresholds: Vec<(DThresh, bool, f64)>,
    // weapon constants
    pub w_damages: [[f64; 2]; 6],
    pub w_present: [bool; 6],
    pub w_spd_mult: f64,
    // assemble program
    pub class_def_val: f64,
    pub skp_atree_adds: [Vec<f64>; 5],
    pub skp_const_adds: [Vec<f64>; 5],
    pub skp_static_adds: [Vec<f64>; 5],
    pub var_effects: Vec<DVarEffect>,
    pub var_slots: Vec<u32>,              // vals index per slot
    pub const_term_keys: Vec<String>,     // leaf-resolved var term inputs
    pub rows: Vec<DRow>,
    pub obj: DObjective,
    /// Direct leaf build (no Obj base for gated leaves); None → build the
    /// Obj base and lower it per leaf instead.
    pub direct: Option<DenseDirect>,
    /// row_canon[i] = smallest row index whose (per_cast, flat) computation
    /// is identical to row i's — same modified spell, boost deltas, and DPS
    /// parameters. Within one eval the canonical result is computed once and
    /// reused (pure memoization; the inputs are bit-identical).
    pub row_canon: Vec<usize>,
}

#[derive(Default)]
pub struct DenseLeaf {
    pub lc_vals: Vec<f64>,
    pub present: Vec<u64>,
    pub var_out_absent: Vec<bool>,        // per var slot: key absent in LC
    pub const_term_vals: Vec<f64>,
    pub dam_entries: Vec<DMultEntry>,
    pub dam_vals: Vec<f64>,
    pub def_entries: Vec<DMultEntry>,
    pub def_vals: Vec<f64>,
    pub atk_spd_idx: i64,
    pub has_arcanes: bool,
    /// Trace-only: previous call's item-name identities.
    pub probe_prev: [(usize, usize); 8],
}

/// Mutable trial state; dam/def lists are journaled per row and end each
/// row eval back in their leaf state.
#[derive(Default)]
pub struct DScratch {
    pub vals: Vec<f64>,
    pub present: Vec<u64>,
    pub dam_entries: Vec<DMultEntry>,
    pub dam_vals: Vec<f64>,
    pub def_entries: Vec<DMultEntry>,
    pub def_vals: Vec<f64>,
    pub var_acc: Vec<f64>,
}

#[inline]
fn bit_get(words: &[u64], i: u32) -> bool {
    words[(i / 64) as usize] & (1u64 << (i % 64)) != 0
}
#[inline]
fn bit_set(words: &mut [u64], i: u32) {
    words[(i / 64) as usize] |= 1u64 << (i % 64);
}

impl DenseCtx {
    /// Numeric read with StatsView::num semantics: present Number → f64,
    /// Bool → 0/1, anything else (or absent) → NaN.
    fn dense_num(v: Option<&Value>) -> f64 {
        match v {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
            Some(Value::Bool(b)) => if *b { 1.0 } else { 0.0 },
            _ => f64::NAN,
        }
    }

    /// `extra_keys` are stat names no static row mentions but a per-leaf
    /// injected token can produce. They need slots in the key universe up
    /// front — `DScratch` is sized from it, so a key discovered mid-search
    /// has nowhere to live.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        l2: &Layer2, tables: &Tables, weapon: &Obj, rows: &[Row],
        compiled: &[CompiledRow], objective: &Objective,
        raw_thresholds: &[Threshold], base_costs: &HashMap<i64, f64>,
        extra_keys: &[String],
    ) -> Option<DenseCtx> {
        // Under SCORE_TRACE=1, name WHY dense is declined. A declined dense
        // path silently multiplies per-leaf cost several-fold (base build +
        // Obj assembles), and until this existed the only symptom was a slow
        // benchmark with no visible cause.
        macro_rules! decline {
            ($reason:expr) => {{
                if std::env::var("SCORE_TRACE").as_deref() == Ok("1") {
                    eprintln!("dense declined: {}", $reason);
                }
                return None;
            }};
        }

        // Radiance applies a nonlinear scale-and-floor between the atree_raw
        // merge and the scaling stage; the dense fold has no equivalent, so
        // those scenarios use the validated Obj path.
        if l2.radiance_boost != 1.0 { decline!("radiance boost active"); }

        // Guard: every row the compiled eval would score must have a plan.
        //
        // Exception: a spell with NO parts (e.g. Escape — cast for movement or
        // mana cycling, `"parts": []`). `compile_spell_plan` cannot plan it,
        // but it cannot deal damage either: the Obj path's
        // `compute_spell_display_avg` finds no display part and returns 0.0,
        // and there is no flat-damage root. The dense scorer reproduces that
        // 0.0 explicitly (see the plan-None arm in the row loop), so the row
        // is harmless — declining for it used to switch the whole scenario to
        // the Obj path and roughly triple per-leaf cost. That was the entire
        // reason fam_spell_sustained ran ~3M/s while its siblings ran ~15M/s.
        for (row, comp) in rows.iter().zip(compiled) {
            if comp.mod_spell.is_none() || row.qty <= 0.0 || row.pseudo { continue; }
            if comp.plan.is_none() {
                let empty_parts = comp.mod_spell.as_ref()
                    .map(|sp| spell_parts(sp).is_empty())
                    .unwrap_or(false);
                if empty_parts { continue; }
                decline!("a row without a compiled plan");
            }
        }

        let mut idx: HashMap<String, u32> = HashMap::new();
        let mut keys: Vec<String> = Vec::new();
        let mut intern = |k: &str, idx: &mut HashMap<String, u32>, keys: &mut Vec<String>| -> u32 {
            if let Some(&i) = idx.get(k) { return i; }
            let i = keys.len() as u32;
            idx.insert(k.to_string(), i);
            keys.push(k.to_string());
            i
        };
        for k in extra_keys { intern(k, &mut idx, &mut keys); }
        macro_rules! it { ($k:expr) => { intern($k, &mut idx, &mut keys) } }

        let skp_idx = std::array::from_fn(|i| it!(SKP_ORDER[i]));
        let names = &tables.names;
        let arr6 = |a: &[String; 6], idx: &mut HashMap<String, u32>, keys: &mut Vec<String>| -> [u32; 6] {
            std::array::from_fn(|i| intern(&a[i], idx, keys))
        };
        let conv_base_idx = arr6(&names.conv_base, &mut idx, &mut keys);
        let dam_add_min_idx = arr6(&names.dam_add_min, &mut idx, &mut keys);
        let dam_add_max_idx = arr6(&names.dam_add_max, &mut idx, &mut keys);
        let sd_pct_idx = arr6(&names.sd_pct, &mut idx, &mut keys);
        let md_pct_idx = arr6(&names.md_pct, &mut idx, &mut keys);
        let dam_pct_idx = arr6(&names.dam_pct, &mut idx, &mut keys);
        let sd_raw_idx = arr6(&names.sd_raw, &mut idx, &mut keys);
        let md_raw_idx = arr6(&names.md_raw, &mut idx, &mut keys);
        let dam_raw_idx = arr6(&names.dam_raw, &mut idx, &mut keys);

        let s_sd_pct = it!("sdPct"); let s_md_pct = it!("mdPct"); let s_dam_pct = it!("damPct");
        let s_r_sd_pct = it!("rSdPct"); let s_r_md_pct = it!("rMdPct"); let s_r_dam_pct = it!("rDamPct");
        let s_sd_raw = it!("sdRaw"); let s_md_raw = it!("mdRaw"); let s_dam_raw = it!("damRaw");
        let s_r_sd_raw = it!("rSdRaw"); let s_r_md_raw = it!("rMdRaw"); let s_r_dam_raw = it!("rDamRaw");
        let s_crit_dam_pct = it!("critDamPct");
        let s_def = it!("def"); let s_agi = it!("agi"); let s_hp = it!("hp");
        let s_hp_bonus = it!("hpBonus"); let s_agi_def = it!("agiDef");
        let s_hpr_raw = it!("hprRaw"); let s_hpr_pct = it!("hprPct");
        let s_max_mana = it!("maxMana"); let s_int = it!("int");
        let s_mr = it!("mr"); let s_ms = it!("ms");
        let mut mana_keys: Vec<(String, u32)> = vec![
            ("mr".into(), s_mr), ("ms".into(), s_ms),
            ("maxMana".into(), s_max_mana), ("int".into(), s_int),
            ("hp".into(), s_hp), ("hpBonus".into(), s_hp_bonus),
        ];
        for comp in compiled {
            if let Some((kr, kp, kf)) = &comp.cost_keys {
                for k in [kr, kp, kf] {
                    if !mana_keys.iter().any(|(n, _)| n == k) {
                        let i = intern(k, &mut idx, &mut keys);
                        mana_keys.push((k.clone(), i));
                    }
                }
            }
        }
        let atk_spd_str = weapon.get("atkSpd").and_then(|v| v.as_str()).map(String::from);
        let class_def_idx = it!("classDef");
        let dex_idx = it!("dex");
        let atk_tier_idx = it!("atkTier");
        mana_keys.push(("atkTier".into(), atk_tier_idx));

        // The same mana reads, resolved to slots. Every key here was just
        // interned into the universe above, so the lookups cannot miss; the
        // one shape that can decline is a charged row whose cost keys were
        // never compiled, which leaves that row unable to price itself.
        let mana_idx = {
            let slot = |k: &str| idx.get(k).copied();
            let mut row_cost: Vec<Option<([u32; 3], f64)>> = Vec::with_capacity(rows.len());
            let mut ok = compiled.len() == rows.len()
                && std::env::var("SCORE_MANA_DENSE").as_deref() != Ok("0");
            for (r, comp) in rows.iter().zip(compiled.iter()) {
                let entry = match (&comp.cost_keys, r.spell.as_ref()) {
                    (Some((kr, kp, kf)), Some(sp)) => {
                        match (slot(kr), slot(kp), slot(kf)) {
                            (Some(a), Some(b), Some(c)) => Some((
                                [a, b, c],
                                sp.get("cost").and_then(|c| c.as_f64()).unwrap_or(f64::NAN),
                            )),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                // A row the sim actually charges must have lowered.
                if r.sim_cost_present && entry.is_none() { ok = false; }
                row_cost.push(entry);
            }
            ManaIdx {
                ok,
                mr: s_mr, ms: s_ms, max_mana: s_max_mana, int: s_int,
                hp: s_hp, hp_bonus: s_hp_bonus, atk_tier: atk_tier_idx,
                atk_spd: tables.atk_spd_index(atk_spd_str.as_deref()),
                row_cost,
            }
        };

        // Weapon constants (weapon is scenario-constant).
        let wview = StatsView::Borrowed(weapon);
        let crafted = wview.str_of("tier") == Some("Crafted");
        let mut w_damages = [[f64::NAN; 2]; 6];
        for (i, k) in tables.damage_keys.iter().enumerate().take(6) {
            let v = weapon.get(k).cloned().unwrap_or(Value::Null);
            let a = if crafted {
                v.as_array().and_then(|a| a.get(1)).cloned().unwrap_or(Value::Null)
            } else { v };
            let d = arr_f64(&a);
            w_damages[i] = [d.first().copied().unwrap_or(f64::NAN), d.get(1).copied().unwrap_or(f64::NAN)];
        }
        let mut w_present = [false; 6];
        if let Some(p) = weapon.get("damagePresent").map(arr_bool) {
            for i in 0..6.min(p.len()) { w_present[i] = p[i]; }
        }
        let w_spd_i = tables.atk_spd_index(wview.str_of("atkSpd"));
        let w_spd_mult = if w_spd_i >= 0 { tables.base_damage_multiplier[w_spd_i as usize] } else { f64::NAN };
        let class_def_val = weapon.get("type").and_then(|v| v.as_str())
            .map(|wt| l2.class_def.get(wt).copied().unwrap_or(1.0)).unwrap_or(1.0);
        // assemble_from_base only inserts classDef when weapon.type is a
        // string; scenarios always have it — otherwise fall back.
        if weapon.get("type").and_then(|v| v.as_str()).is_none() { decline!("weapon has no type"); }

        // skp add chains from the constant merge stages, in merge order.
        let nested_prefix = |k: &str| -> bool {
            let start = k.split('.').next().unwrap_or(k);
            matches!(start, "damMult" | "defMult" | "healMult" | "manaMult")
        };
        let stage_adds = |src: Option<&Obj>| -> [Vec<f64>; 5] {
            let mut out: [Vec<f64>; 5] = Default::default();
            if let Some(src) = src {
                for (k, v) in src {
                    if let Some(i) = SKP_ORDER.iter().position(|s| s == k) {
                        out[i].push(v.as_f64().unwrap_or(f64::NAN));
                    }
                }
            }
            out
        };
        let const_stage: Option<&Obj> = match l2.scaling_kind.as_str() {
            "cached" => l2.scaled_cached.as_ref(),
            _ => l2.const_scaled.as_ref(),
        };
        let skp_atree_adds = stage_adds(l2.atree_raw.as_ref());
        let skp_const_adds = stage_adds(const_stage);
        let skp_static_adds = stage_adds(l2.static_boosts.as_ref());

        // Var effects → index programs. Guards: outputs must be plain scalar
        // keys, and must not collide with static_boosts (fold-order).
        let mut var_slots: Vec<u32> = Vec::new();
        let mut const_term_keys: Vec<String> = Vec::new();
        let mut var_effects: Vec<DVarEffect> = Vec::new();
        let mut seen_slots: Vec<u32> = Vec::new();
        for eff in &l2.var_effects {
            let const_add = eff.get("const_add").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let mut terms = Vec::new();
            for term in eff.get("terms").and_then(|t| t.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
                let stat = term.get("stat").and_then(|s| s.as_str()).unwrap_or("");
                let factor = term.get("factor").and_then(|f| f.as_f64()).unwrap_or(f64::NAN);
                match SKP_ORDER.iter().position(|s| *s == stat) {
                    Some(i) => terms.push(DTerm::Skp(i, factor)),
                    None => {
                        let slot = match const_term_keys.iter().position(|k| k == stat) {
                            Some(p) => p,
                            None => { const_term_keys.push(stat.to_string()); const_term_keys.len() - 1 }
                        };
                        terms.push(DTerm::Const(slot, factor));
                    }
                }
            }
            let mut out_slots = Vec::new();
            for output in eff.get("outputs").and_then(|o| o.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
                let name = output.as_str()?;
                if nested_prefix(name) || name == "atkSpd" { decline!(format!("var output '{}' (multiplier map / atkSpd)", name)); }
                if l2.static_boosts.as_ref().map(|s| s.contains_key(name)).unwrap_or(false) {
                    decline!(format!("fold-order guard on '{}'", name)); // fold-order guard
                }
                let ki = intern(name, &mut idx, &mut keys);
                let slot = match var_slots.iter().position(|&s| s == ki) {
                    Some(p) => p,
                    None => { var_slots.push(ki); var_slots.len() - 1 }
                };
                let first = !seen_slots.contains(&ki);
                if first { seen_slots.push(ki); }
                out_slots.push((slot, first));
            }
            var_effects.push(DVarEffect {
                const_add, terms,
                round: eff.get("round").and_then(|v| v.as_bool()).unwrap_or(true),
                positive: eff.get("positive").and_then(|v| v.as_bool()).unwrap_or(true),
                max: eff.get("max").and_then(|v| v.as_f64()),
                out_slots,
            });
        }

        // Threshold lowering (read-universe interning happens here so item
        // writes to threshold stats survive the dead-write filter).
        let mut thresholds: Vec<(DThresh, bool, f64)> = Vec::new();
        for t in raw_thresholds {
            let kind = match t.stat.as_str() {
                "ehp" => DThresh::Ehp,
                "ehp_no_agi" => DThresh::EhpNoAgi,
                "total_hp" => DThresh::TotalHp,
                "ehpr" => DThresh::Ehpr,
                "hpr" => DThresh::Hpr,
                "total_mana" => DThresh::TotalMana,
                st if st.starts_with("finalSpellCost") => {
                    let n: i64 = st[st.len() - 1..].parse().unwrap_or(-1);
                    match base_costs.get(&n) {
                        Some(&base) => DThresh::SpellCost {
                            raw: intern(&format!("spRaw{}", n), &mut idx, &mut keys),
                            pct: intern(&format!("spPct{}", n), &mut idx, &mut keys),
                            fin: intern(&format!("spPct{}Final", n), &mut idx, &mut keys),
                            base,
                        },
                        None => DThresh::Skip,
                    }
                }
                st => DThresh::Plain(intern(st, &mut idx, &mut keys)),
            };
            thresholds.push((kind, t.ge, t.value));
        }

        // Bounded-doom classification of var output slots.
        let mut var_slot_mana_dir: Vec<i8> = Vec::with_capacity(var_slots.len());
        let mut doom_couples_start = false;
        let mut doom_couples_tier = false;
        for &ki in &var_slots {
            let name = keys[ki as usize].as_str();
            let dir: i8 = match name {
                "mr" | "ms" | "hp" | "hpBonus" => 1,
                "maxMana" | "int" => { doom_couples_start = true; 1 }
                "atkTier" => { doom_couples_tier = true; 0 }
                _ if name.starts_with("spRaw") || name.starts_with("spPct") => -1,
                _ => 0,
            };
            var_slot_mana_dir.push(dir);
        }

        // Rows: bonus deltas + plan ConvBase indices.
        let mut drows = Vec::with_capacity(compiled.len());
        for comp in compiled {
            let mut stat_ops = Vec::new();
            let mut dam_ops = Vec::new();
            let mut def_ops = Vec::new();
            for b in &comp.bonuses {
                match b.kind {
                    CompiledBonusKind::Stat => {
                        if b.key == "atkSpd" || nested_prefix(&b.key) { decline!(format!("boost key '{}'", b.key)); }
                        stat_ops.push((intern(&b.key, &mut idx, &mut keys), b.contrib, b.use_max));
                    }
                    CompiledBonusKind::DamMult =>
                        dam_ops.push((parse_mult_entry(&b.key), b.contrib, b.use_max)),
                    CompiledBonusKind::DefMult =>
                        def_ops.push((parse_mult_entry(&b.key), b.contrib, b.use_max)),
                }
            }
            let mut parts_conv = Vec::new();
            if let Some(plan) = &comp.plan {
                for part in &plan.parts {
                    parts_conv.push(match &part.kind {
                        PartKindPlan::Damage(d) => Some(std::array::from_fn(|i| {
                            intern(&d.conv_names[i], &mut idx, &mut keys)
                        })),
                        _ => None,
                    });
                }
            }
            drows.push(DRow { stat_ops, dam_ops, def_ops, parts_conv });
        }

        // Objective lowering.
        let lower_ind = |stat: &str, idx: &mut HashMap<String, u32>, keys: &mut Vec<String>| -> DInd {
            match stat {
                "ehp" => DInd::Ehp,
                "ehp_no_agi" => DInd::EhpNoAgi,
                "total_hp" => DInd::TotalHp,
                "hpr" => DInd::Hpr,
                "ehpr" => DInd::Ehpr,
                "total_mana" => DInd::TotalMana,
                other => DInd::Plain(intern(other, idx, keys)),
            }
        };
        let obj = match objective {
            Objective::ComboDamage => DObjective::Damage,
            // Healing has no dense lowering: fall back to the Obj path.
            Objective::TotalHealing => decline!("total_healing objective"),
            Objective::Indirect(stat) => {
                if nested_prefix(stat) { decline!(format!("indirect stat '{}' has a multiplier-map prefix", stat)); }
                DObjective::Indirect(lower_ind(stat, &mut idx, &mut keys))
            }
            Objective::Custom(weights) => {
                let mut out = Vec::new();
                for (target, w) in weights {
                    if target == "total_healing" { decline!("custom blend contains total_healing"); }
                    if target == "combo_damage" { out.push((None, *w)); }
                    else {
                        if nested_prefix(target) { decline!(format!("custom blend target '{}'", target)); }
                        out.push((Some(lower_ind(target, &mut idx, &mut keys)), *w));
                    }
                }
                DObjective::Custom(out)
            }
        };

        // ── Direct leaf-build lowering ──
        let direct = (|| -> Option<DenseDirect> {
            // Scalar merge value with merge_plain/num-compatible semantics;
            // Bool is rejected (num() would read 1, an overlay would read 0).
            let scalar_val = |v: &Value| -> Option<f64> {
                match v {
                    Value::Number(n) => Some(n.as_f64().unwrap_or(f64::NAN)),
                    Value::Bool(_) => None,
                    _ => Some(f64::NAN),
                }
            };
            const NONSTACKING: [&str; 3] = ["Potion", "Vulnerability", "Mask"];
            // damMult/defMult tail simulation across the three stages.
            struct MultSim { tome_adds: Vec<f64>, tail: Vec<(String, f64)> }
            impl MultSim {
                fn apply(&mut self, name: &str, v: f64) {
                    if name == "tome" { self.tome_adds.push(v); return; }
                    match self.tail.iter_mut().find(|(k, _)| k == name) {
                        Some((k, cur)) => {
                            if NONSTACKING.contains(&k.as_str()) && !cur.is_nan() {
                                if v > *cur { *cur = v; }
                                return;
                            }
                            *cur += v;
                        }
                        None => self.tail.push((name.to_string(), v)),
                    }
                }
            }
            let mut dam_sim = MultSim { tome_adds: Vec::new(), tail: Vec::new() };
            let mut def_sim = MultSim { tome_adds: Vec::new(), tail: Vec::new() };
            let mut progs: [Vec<(u32, f64)>; 3] = Default::default();
            let stages: [Option<&Obj>; 3] = [
                l2.atree_raw.as_ref(),
                match l2.scaling_kind.as_str() {
                    "cached" => l2.scaled_cached.as_ref(),
                    _ => l2.const_scaled.as_ref(),
                },
                l2.static_boosts.as_ref(),
            ];
            for (si, stage) in stages.iter().enumerate() {
                let Some(stage) = stage else { continue };
                // Expand __m wraps exactly like merge_into.
                let mut flat: Vec<(String, &Value)> = Vec::new();
                for (k, v) in *stage {
                    if let Some(m) = v.get("__m").and_then(|x| x.as_object()) {
                        for (mk, mv) in m { flat.push((format!("{}.{}", k, mk), mv)); }
                    } else {
                        flat.push((k.clone(), v));
                    }
                }
                for (name, v) in flat {
                    let start = name.split('.').next().unwrap_or(&name);
                    match start {
                        "atkSpd" => decline!("atkSpd in a lowered add chain"),
                        "damMult" | "defMult" => {
                            let rest = &name[name.find('.').map(|i| i + 1).unwrap_or(name.len())..];
                            let f = v.as_f64().unwrap_or(f64::NAN);
                            if start == "damMult" { dam_sim.apply(rest, f); }
                            else { def_sim.apply(rest, f); }
                        }
                        "healMult" | "manaMult" => {} // never read by the dense paths
                        _ => {
                            if SKP_ORDER.contains(&name.as_str()) { continue; } // skp chains
                            let f = scalar_val(v)?;
                            progs[si].push((intern(&name, &mut idx, &mut keys), f));
                        }
                    }
                }
            }
            let [atree_prog, const_prog, static_prog] = progs;

            // Complete the read universe before lowering writes.
            let hp_idx = intern("hp", &mut idx, &mut keys);
            let agi_def_idx = intern("agiDef", &mut idx, &mut keys);
            let dam_mobs_idx = intern("damMobs", &mut idx, &mut keys);
            let def_mobs_idx = intern("defMobs", &mut idx, &mut keys);
            let term_capture: Vec<u32> = const_term_keys.iter()
                .map(|k| intern(k, &mut idx, &mut keys)).collect();

            // Template: static/must id zeros + hp + agiDef (read keys only).
            let mut template_zero_idxs = Vec::new();
            for id in l2.static_ids.iter().chain(l2.must_ids.iter()) {
                if let Some(&i) = idx.get(id) { template_zero_idxs.push(i); }
            }

            // Items, tomes, weapon lowered to indexed adds (read keys only).
            let mut items = HashMap::new();
            for (name, item) in &l2.item_registry {
                let di = DenseDirect::lower_item(l2, item, |k| idx.get(k).copied())?;
                items.insert(name.clone(), di);
            }
            let mut post_item_adds = Vec::new();
            for tome in &l2.tome_sms {
                let di = DenseDirect::lower_item(l2, tome, |k| idx.get(k).copied())?;
                post_item_adds.extend(di.adds);
            }
            let wdi = DenseDirect::lower_item(l2, weapon, |k| idx.get(k).copied())?;
            let base_arcanes = l2.tome_sms.iter().any(item_has_arcanes) || item_has_arcanes(weapon);
            post_item_adds.extend(wdi.adds);

            // Set bonuses (skp keys excluded, js coercion prebaked).
            let js_num = |v: &Value| -> f64 {
                match v {
                    Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
                    Value::Bool(b) => if *b { 1.0 } else { 0.0 },
                    Value::Null => 0.0,
                    _ => f64::NAN,
                }
            };
            let mut sets = HashMap::new();
            for (set_name, set_data) in &l2.sets_data {
                let Some(bonuses) = set_data.get("bonuses").and_then(|b| b.as_array()) else { continue };
                let mut per_count = Vec::with_capacity(bonuses.len());
                for bonus in bonuses {
                    per_count.push(bonus.as_object().map(|bo| {
                        bo.iter()
                            .filter(|(id, _)| !l2.skp_order.iter().any(|s| s == *id))
                            .filter_map(|(id, v)| idx.get(id).map(|&i| (i, js_num(v))))
                            .collect::<Vec<_>>()
                    }));
                }
                sets.insert(set_name.clone(), per_count);
            }

            let parse_tail = |sim: &MultSim| -> Vec<(DMultEntry, f64)> {
                sim.tail.iter().map(|(k, v)| (parse_mult_entry(k), *v)).collect()
            };

            Some(DenseDirect {
                template_vals: Vec::new(),      // sized after interning settles
                template_present: Vec::new(),
                items, post_item_adds, sets,
                atree_prog, const_prog, static_prog,
                term_capture, dam_mobs_idx, def_mobs_idx,
                dam_tail: parse_tail(&dam_sim),
                def_tail: parse_tail(&def_sim),
                dam_tome_adds: dam_sim.tome_adds,
                def_tome_adds: def_sim.tome_adds,
                atk_spd_idx: tables.atk_spd_index(weapon.get("atkSpd").and_then(|v| v.as_str())),
                template_zero_idxs, base_arcanes, hp_idx, agi_def_idx,
                hp_base: l2.hp_base,
                class_def_idx: 0,               // filled below
                class_def_val,
            })
        })();
        // Size the template against the final key universe.
        let direct = direct.map(|mut dd| {
            dd.class_def_idx = class_def_idx;
            let n = keys.len();
            dd.template_vals = vec![f64::NAN; n];
            dd.template_present = vec![0u64; n.div_ceil(64)];
            for &i in &dd.template_zero_idxs {
                dd.template_vals[i as usize] = 0.0;
                bit_set(&mut dd.template_present, i);
            }
            dd.template_vals[dd.hp_idx as usize] = dd.hp_base;
            bit_set(&mut dd.template_present, dd.hp_idx);
            dd.template_vals[dd.agi_def_idx as usize] = 90.0;
            bit_set(&mut dd.template_present, dd.agi_def_idx);
            dd
        });

        // Row-group canonicalization for per-eval memoization.
        let bonuses_eq = |a: &[CompiledBonus], b: &[CompiledBonus]| -> bool {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| {
                x.kind == y.kind && x.key == y.key
                    && x.contrib.to_bits() == y.contrib.to_bits()
                    && x.use_max == y.use_max
            })
        };
        let dps_eq = |a: &Option<(String, f64, String)>, b: &Option<(String, f64, String)>| -> bool {
            match (a, b) {
                (None, None) => true,
                (Some((n1, h1, r1)), Some((n2, h2, r2))) =>
                    n1 == n2 && h1.to_bits() == h2.to_bits() && r1 == r2,
                _ => false,
            }
        };
        let mut row_canon: Vec<usize> = (0..compiled.len()).collect();
        for i in 0..compiled.len() {
            let (ri, ci) = (&rows[i], &compiled[i]);
            if ci.mod_spell.is_none() || ri.qty <= 0.0 || ri.pseudo { continue; }
            for j in 0..i {
                let (rj, cj) = (&rows[j], &compiled[j]);
                if row_canon[j] != j { continue; }
                if cj.mod_spell.is_none() || rj.qty <= 0.0 || rj.pseudo { continue; }
                let same = ci.mod_spell == cj.mod_spell
                    && bonuses_eq(&ci.bonuses, &cj.bonuses)
                    && dps_eq(&ci.dps, &cj.dps)
                    && ci.fallback_root == cj.fallback_root
                    && ri.dps_per_hit_name == rj.dps_per_hit_name
                    && ri.dps_hits.to_bits() == rj.dps_hits.to_bits()
                    && ri.dps_hits_override.map(f64::to_bits) == rj.dps_hits_override.map(f64::to_bits);
                if same { row_canon[i] = j; break; }
            }
        }

        let n = keys.len();
        Some(DenseCtx {
            direct, row_canon,
            idx, n, skp_idx, class_def_idx, dex_idx, atk_tier_idx,
            conv_base_idx, dam_add_min_idx, dam_add_max_idx,
            sd_pct_idx, md_pct_idx, dam_pct_idx, sd_raw_idx, md_raw_idx, dam_raw_idx,
            s_sd_pct, s_md_pct, s_dam_pct, s_r_sd_pct, s_r_md_pct, s_r_dam_pct,
            s_sd_raw, s_md_raw, s_dam_raw, s_r_sd_raw, s_r_md_raw, s_r_dam_raw,
            s_crit_dam_pct, s_def, s_agi, s_hp, s_hp_bonus, s_agi_def,
            s_hpr_raw, s_hpr_pct, s_max_mana, s_int, mana_keys, mana_idx, atk_spd_str,
            var_slot_mana_dir, doom_couples_start, doom_couples_tier, thresholds,
            w_damages, w_present, w_spd_mult, class_def_val,
            skp_atree_adds, skp_const_adds, skp_static_adds,
            var_effects, var_slots, const_term_keys, rows: drows, obj,
        })
    }
}

impl DenseLeaf {
    pub fn build(d: &DenseCtx, l2: &Layer2, base: &Obj, tables: &Tables) -> Option<DenseLeaf> {
        // lc_pre: base + classDef + atree_raw — var effect terms read this.
        let mut lc = base.clone();
        lc.insert("classDef".into(), Value::from(d.class_def_val));
        merge_into(&mut lc, l2.atree_raw.as_ref());
        let const_term_vals: Vec<f64> = d.const_term_keys.iter()
            .map(|k| lc.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0))
            .collect();
        // lc_full: + const_scaled/cached + static_boosts (constant folds).
        let const_stage: Option<&Obj> = match l2.scaling_kind.as_str() {
            "cached" => l2.scaled_cached.as_ref(),
            _ => l2.const_scaled.as_ref(),
        };
        merge_into(&mut lc, const_stage);
        merge_into(&mut lc, l2.static_boosts.as_ref());

        let mut lc_vals = vec![f64::NAN; d.n];
        let mut present = vec![0u64; d.n.div_ceil(64)];
        for (k, &i) in &d.idx {
            if let Some(v) = lc.get(k) {
                lc_vals[i as usize] = DenseCtx::dense_num(Some(v));
                bit_set(&mut present, i);
            }
        }
        // skp keys are overwritten every trial; mark present.
        for &i in &d.skp_idx { bit_set(&mut present, i); }
        // var-out keys become present on the first trial write.
        let mut var_out_absent = Vec::with_capacity(d.var_slots.len());
        for &ki in &d.var_slots {
            let absent = !bit_get(&present, ki);
            var_out_absent.push(absent);
            if absent { bit_set(&mut present, ki); }
        }
        // Row stat ops read `prev.as_f64() or 0` — a present non-Number LC
        // value would diverge from num() semantics; fall back on that leaf.
        for dr in &d.rows {
            for &(i, _, _) in &dr.stat_ops {
                if bit_get(&present, i) && lc_vals[i as usize].is_nan() {
                    if let Some(k) = d.idx.iter().find(|(_, &v)| v == i).map(|(k, _)| k) {
                        match lc.get(k) {
                            None | Some(Value::Number(_)) => {}
                            _ => return None,
                        }
                    }
                }
            }
        }

        let parse_mult_map = |key: &str| -> Option<(Vec<DMultEntry>, Vec<f64>)> {
            let mut entries = Vec::new();
            let mut vals = Vec::new();
            if let Some(m) = lc.get(key).and_then(as_map) {
                for (k, v) in m {
                    entries.push(parse_mult_entry(k));
                    vals.push(v.as_f64().unwrap_or(f64::NAN));
                }
            }
            Some((entries, vals))
        };
        let (dam_entries, dam_vals) = parse_mult_map("damMult")?;
        let (def_entries, def_vals) = parse_mult_map("defMult")?;
        let atk_spd_idx = tables.atk_spd_index(lc.get("atkSpd").and_then(|v| v.as_str()));
        let has_arcanes = lc.get("activeMajorIDs")
            .and_then(|v| v.get("__s")).and_then(|s| s.as_array())
            .map(|a| a.iter().any(|m| m.as_str() == Some("ARCANES")))
            .unwrap_or(false);

        Some(DenseLeaf {
            lc_vals, present, var_out_absent, const_term_vals,
            dam_entries, dam_vals, def_entries, def_vals, atk_spd_idx, has_arcanes,
            probe_prev: Default::default(),
        })
    }
}

impl DScratch {
    pub fn reset(&mut self, leaf: &DenseLeaf, d: &DenseCtx) {
        self.vals.resize(leaf.lc_vals.len(), f64::NAN);
        self.present.clear();
        self.present.extend_from_slice(&leaf.present);
        self.dam_entries.clear();
        self.dam_entries.extend(leaf.dam_entries.iter().cloned());
        self.dam_vals.clear();
        self.dam_vals.extend_from_slice(&leaf.dam_vals);
        self.def_entries.clear();
        self.def_entries.extend(leaf.def_entries.iter().cloned());
        self.def_vals.clear();
        self.def_vals.extend_from_slice(&leaf.def_vals);
        self.var_acc.clear();
        self.var_acc.resize(d.var_slots.len(), 0.0);
    }

    #[inline]
    pub fn num(&self, i: u32) -> f64 { self.vals[i as usize] }
    #[inline]
    pub fn num_or0(&self, i: u32) -> f64 {
        let v = self.vals[i as usize];
        if v.is_nan() { 0.0 } else { v }
    }
    #[inline]
    pub fn has(&self, i: u32) -> bool { bit_get(&self.present, i) }
}

/// Per-thread reusable dense buffers (leaf + trial scratch); one leaf's
/// state fully overwrites the previous leaf's.
#[derive(Default)]
pub struct DenseWork {
    pub leaf: DenseLeaf,
    pub scratch: DScratch,
    /// Reused across leaves: its key covers every simulation input, so an
    /// entry stays valid whichever leaf produced it.
    pub sim_memo: SimMemo,
}

/// Per-trial assemble: memcpy + skp chains + var effects + indexed writes.
pub fn dense_assemble(d: &DenseCtx, leaf: &DenseLeaf, s: &mut DScratch, total_sp: &[f64]) {
    s.vals.copy_from_slice(&leaf.lc_vals);
    let mut skp_pre = [0.0f64; 5]; // value var terms see (before const stage)
    for i in 0..5 {
        let mut v = total_sp[i];
        for a in &d.skp_atree_adds[i] { v += a; }
        skp_pre[i] = v;
        for a in &d.skp_const_adds[i] { v += a; }
        for a in &d.skp_static_adds[i] { v += a; }
        s.vals[d.skp_idx[i] as usize] = v;
    }
    for eff in &d.var_effects {
        let mut total = 0.0;
        total += eff.const_add;
        for term in &eff.terms {
            total += match term {
                DTerm::Skp(i, f) => skp_pre[*i] * f,
                DTerm::Const(slot, f) => leaf.const_term_vals[*slot] * f,
            };
        }
        let mut t = total;
        if eff.round { t = round_near(t).floor(); }
        if eff.positive && t < 0.0 { t = 0.0; }
        if let Some(mx) = eff.max {
            if mx > 0.0 && t > mx { t = mx; }
            if mx < 0.0 && t < mx { t = mx; }
        }
        for (slot, first) in &eff.out_slots {
            if *first { s.var_acc[*slot] = t; } else { s.var_acc[*slot] += t; }
        }
    }
    for (slot, &ki) in d.var_slots.iter().enumerate() {
        let acc = s.var_acc[slot];
        if leaf.var_out_absent[slot] { s.vals[ki as usize] = acc; }
        else { s.vals[ki as usize] += acc; }
    }
}

/// calculate_spell_damage_pc over the dense vector — same float sequence.
#[allow(clippy::too_many_arguments)]
fn dense_spell_damage(
    d: &DenseCtx, s: &DScratch, mults: &[f64], use_spell: bool, ignore_speed: bool,
    part_filter: Option<&str>, ignore_str: bool, ignored: &[String],
    conv_idx: &[u32; 6], tables: &Tables,
) -> ([f64; 2], [f64; 2]) {
    let mut present = d.w_present;
    let mut conversions = [0.0f64; 6];
    for i in 0..mults.len().min(6) { conversions[i] = mults[i]; }
    for i in 0..6 {
        let ci = conv_idx[i];
        if s.has(ci) { conversions[i] += s.num(ci); }
    }
    for i in 0..6 {
        let ci = d.conv_base_idx[i];
        if s.has(ci) { conversions[i] += s.num(ci); }
    }

    let neutral_convert = conversions[0] / 100.0;
    if neutral_convert == 0.0 { present = [false; 6]; }
    let mut damages = [[0.0f64; 2]; 6];
    let mut weapon_min = 0.0;
    let mut weapon_max = 0.0;
    for i in 0..6 {
        damages[i] = [d.w_damages[i][0] * neutral_convert, d.w_damages[i][1] * neutral_convert];
        weapon_min += d.w_damages[i][0];
        weapon_max += d.w_damages[i][1];
    }
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

    if !ignore_speed {
        let m = d.w_spd_mult;
        for dmg in damages.iter_mut() { dmg[0] *= m; dmg[1] *= m; }
    }

    for i in 0..6 {
        if present[i] {
            damages[i][0] += s.num(d.dam_add_min_idx[i]);
            damages[i][1] += s.num(d.dam_add_max_idx[i]);
        }
    }

    let (spec_pct, spec_raw, spec_pct_s, spec_raw_s, r_pct_s, r_raw_s) = if use_spell {
        (&d.sd_pct_idx, &d.sd_raw_idx, d.s_sd_pct, d.s_sd_raw, d.s_r_sd_pct, d.s_r_sd_raw)
    } else {
        (&d.md_pct_idx, &d.md_raw_idx, d.s_md_pct, d.s_md_raw, d.s_r_md_pct, d.s_r_md_raw)
    };
    let mut skill_boost = [0.0f64; 6];
    for i in 0..5 {
        skill_boost[i + 1] = tables.sp_to_pct(s.num(d.skp_idx[i])) * tables.skillpoint_damage_mult[i];
    }
    let static_boost = (s.num(spec_pct_s) + s.num(d.s_dam_pct)) / 100.0;

    let mut total_min = 0.0;
    let mut total_max = 0.0;
    let mut save_prop = [[0.0f64; 2]; 6];
    let r_pct = (s.num(r_pct_s) + s.num(d.s_r_dam_pct)) / 100.0;
    for i in 0..6 {
        save_prop[i] = damages[i];
        total_min += damages[i][0];
        total_max += damages[i][1];
        let mut boost = 1.0 + skill_boost[i] + static_boost
            + (s.num(spec_pct[i]) + s.num(d.dam_pct_idx[i])) / 100.0;
        if i > 0 { boost += r_pct; }
        damages[i][0] *= boost;
        damages[i][1] *= boost;
    }

    let total_elem_min = total_min - save_prop[0][0];
    let total_elem_max = total_max - save_prop[0][1];

    let prop_raw = s.num(spec_raw_s) + s.num(d.s_dam_raw);
    let rainbow_raw = s.num(r_raw_s) + s.num(d.s_r_dam_raw);
    for i in 0..6 {
        let save_obj = save_prop[i];
        let mut raw_boost = 0.0;
        if present[i] {
            raw_boost += s.num(spec_raw[i]) + s.num(d.dam_raw_idx[i]);
        }
        let mut min_boost = raw_boost;
        let mut max_boost = raw_boost;
        if total_max > 0.0 {
            if total_min == 0.0 { min_boost += (save_obj[1] / total_max) * prop_raw; }
            else { min_boost += (save_obj[0] / total_min) * prop_raw; }
            max_boost += (save_obj[1] / total_max) * prop_raw;
        }
        if i != 0 && total_elem_max > 0.0 {
            if total_elem_min == 0.0 { min_boost += (save_obj[1] / total_elem_max) * rainbow_raw; }
            else { min_boost += (save_obj[0] / total_elem_min) * rainbow_raw; }
            max_boost += (save_obj[1] / total_elem_max) * rainbow_raw;
        }
        damages[i][0] += min_boost * total_convert;
        damages[i][1] += max_boost * total_convert;
    }

    let str_boost = if ignore_str { 1.0 } else { 1.0 + skill_boost[1] };
    let mut damage_mult = 1.0f64;
    let mut ele_damage_mult = [1.0f64; 6];
    for (e, v) in s.dam_entries.iter().zip(&s.dam_vals) {
        if let Some(sm) = &e.spell_match {
            if Some(&**sm) != part_filter { continue; }
        }
        if ignored.iter().any(|m| m.as_str() == &*e.key) { continue; }
        match e.target {
            MultTarget::All => damage_mult *= 1.0 + v / 100.0,
            MultTarget::MeleeOnly => { if !use_spell { damage_mult *= 1.0 + v / 100.0; } }
            MultTarget::Ele(i) => ele_damage_mult[i] *= 1.0 + v / 100.0,
            MultTarget::Inert => {}
        }
    }
    let crit_mult = if ignore_str { 0.0 } else { 1.0 + s.num(d.s_crit_dam_pct) / 100.0 };

    for i in 0..6 {
        damages[i][0] *= ele_damage_mult[i];
        damages[i][1] *= ele_damage_mult[i];
    }

    let mut total_dam_norm = [0.0f64; 2];
    let mut total_dam_crit = [0.0f64; 2];
    for dmg in damages.iter_mut() {
        if dmg[0] < 0.0 { dmg[0] = 0.0; }
        if dmg[1] < 0.0 { dmg[1] = 0.0; }
        let res = [
            dmg[0] * str_boost * damage_mult,
            dmg[1] * str_boost * damage_mult,
            dmg[0] * (str_boost + crit_mult) * damage_mult,
            dmg[1] * (str_boost + crit_mult) * damage_mult,
        ];
        total_dam_norm[0] += res[0];
        total_dam_norm[1] += res[1];
        total_dam_crit[0] += res[2];
        total_dam_crit[1] += res[3];
    }
    (total_dam_norm, total_dam_crit)
}

/// eval_spell_plan over the dense vector.
fn dense_spell_plan(
    d: &DenseCtx, s: &DScratch, plan: &SpellPlan, drow: &DRow, crit: f64,
    tables: &Tables, use_dps_display: bool,
) -> (f64, f64) {
    let n = plan.parts.len();
    let mut memo: Vec<Option<[f64; 4]>> = vec![None; n];

    fn eval(i: usize, plan: &SpellPlan, drow: &DRow, memo: &mut Vec<Option<[f64; 4]>>,
            d: &DenseCtx, s: &DScratch, tables: &Tables) -> [f64; 4] {
        if let Some(r) = memo[i] { return r; }
        let r = match &plan.parts[i].kind {
            PartKindPlan::Damage(dp) => {
                let conv_idx = drow.parts_conv[i].as_ref().expect("damage part conv idx");
                let (norm, crit_t) = dense_spell_damage(
                    d, s, &dp.multipliers, plan.use_spell, !plan.use_speed,
                    Some(&dp.part_id), !dp.use_str, &dp.ignored_mults, conv_idx, tables);
                [norm[0], norm[1], crit_t[0], crit_t[1]]
            }
            PartKindPlan::Heal => [0.0; 4],
            PartKindPlan::Total(edges) => {
                let mut acc = [0.0f64; 4];
                for (j, hits, tick_rounding) in edges {
                    let sub = eval(*j, plan, drow, memo, d, s, tables);
                    if plan.parts[*j].static_kind == Some("damage") {
                        let eff = if *tick_rounding {
                            1.0 / ((1.0 / hits * 20.0).floor() * 0.05)
                        } else { *hits };
                        for k in 0..4 { acc[k] += sub[k] * eff; }
                    }
                }
                acc
            }
        };
        memo[i] = Some(r);
        r
    }

    let display = if use_dps_display { plan.dps_display_idx } else { plan.display_idx };
    let per_cast = match display {
        Some(i) if plan.parts[i].static_kind == Some("damage") => {
            let r = eval(i, plan, drow, &mut memo, d, s, tables);
            let non_crit_avg = (r[0] + r[1]) / 2.0;
            let crit_avg = (r[2] + r[3]) / 2.0;
            (1.0 - crit) * non_crit_avg + crit * crit_avg
        }
        _ => 0.0,
    };
    let mut flat = 0.0;
    for &i in &plan.flat_idxs {
        let r = eval(i, plan, drow, &mut memo, d, s, tables);
        let non_crit_avg = (r[0] + r[1]) / 2.0;
        let crit_avg = (r[2] + r[3]) / 2.0;
        flat += (1.0 - crit) * non_crit_avg + crit * crit_avg;
    }
    (per_cast, flat)
}

enum DenseUndo {
    Val(u32, f64, bool),           // idx, old val, old present bit
    DamVal(usize, f64),
    DamAppend,
    DefVal(usize, f64),
    DefAppend,
}

/// with_row_overlay over the dense scratch. `extra_row` carries bonuses for
/// tokens injected after the lowering was built; they apply after the row's
/// own ops, matching the Obj path's `comp.bonuses.chain(extra)` order.
fn dense_apply_row(
    s: &mut DScratch, drow: &DRow, extra_row: &DenseRowExtra,
    journal: &mut Vec<DenseUndo>,
) {
    for &(i, contrib, use_max) in drow.stat_ops.iter().chain(&extra_row.stat) {
        let old = s.vals[i as usize];
        let was = bit_get(&s.present, i);
        let cur = if was && !old.is_nan() { old } else { 0.0 };
        s.vals[i as usize] = if use_max { js_max(cur, contrib) } else { cur + contrib };
        bit_set(&mut s.present, i);
        journal.push(DenseUndo::Val(i, old, was));
    }
    for (tmpl, contrib, use_max) in drow.dam_ops.iter().chain(&extra_row.dam) {
        match s.dam_entries.iter().position(|e| e.key == tmpl.key) {
            Some(p) => {
                let old = s.dam_vals[p];
                let cur = if old.is_nan() { 0.0 } else { old };
                s.dam_vals[p] = if *use_max { js_max(cur, *contrib) } else { cur + *contrib };
                journal.push(DenseUndo::DamVal(p, old));
            }
            None => {
                s.dam_entries.push(tmpl.clone());
                s.dam_vals.push(if *use_max { js_max(0.0, *contrib) } else { *contrib });
                journal.push(DenseUndo::DamAppend);
            }
        }
    }
    for (tmpl, contrib, use_max) in drow.def_ops.iter().chain(&extra_row.def) {
        match s.def_entries.iter().position(|e| e.key == tmpl.key) {
            Some(p) => {
                let old = s.def_vals[p];
                let cur = if old.is_nan() { 0.0 } else { old };
                s.def_vals[p] = if *use_max { js_max(cur, *contrib) } else { cur + *contrib };
                journal.push(DenseUndo::DefVal(p, old));
            }
            None => {
                s.def_entries.push(tmpl.clone());
                s.def_vals.push(if *use_max { js_max(0.0, *contrib) } else { *contrib });
                journal.push(DenseUndo::DefAppend);
            }
        }
    }
}

fn dense_undo_row(s: &mut DScratch, journal: &mut Vec<DenseUndo>) {
    while let Some(u) = journal.pop() {
        match u {
            DenseUndo::Val(i, old, was) => {
                s.vals[i as usize] = old;
                if !was { s.present[(i / 64) as usize] &= !(1u64 << (i % 64)); }
            }
            DenseUndo::DamVal(p, old) => { s.dam_vals[p] = old; }
            DenseUndo::DamAppend => { s.dam_entries.pop(); s.dam_vals.pop(); }
            DenseUndo::DefVal(p, old) => { s.def_vals[p] = old; }
            DenseUndo::DefAppend => { s.def_entries.pop(); s.def_vals.pop(); }
        }
    }
}

/// eval_combo_damage_compiled over the dense scratch.
pub fn dense_combo_damage(
    d: &DenseCtx, leaf: &DenseLeaf, s: &mut DScratch, rows: &[Row],
    compiled: &[CompiledRow], tables: &Tables,
) -> f64 {
    dense_combo_damage_extra(d, leaf, s, rows, compiled, tables, &[])
}

/// `dense_combo_damage` with per-row bonuses for tokens injected after the
/// lowering was built — the dynamic-row path's sliders.
///
/// Two things change when `extra` is non-empty. The deltas are applied as
/// journaled index writes right after the row's own `stat_ops`, matching the
/// order `with_row_overlay` uses on the Obj path. And the `row_canon` cache
/// is bypassed: it shares one per-cast result between rows the lowering
/// proved identical, but injected values are per-leaf and can differ between
/// exactly those rows, so a cache hit could return another row's damage.
#[allow(clippy::too_many_arguments)]
pub fn dense_combo_damage_extra(
    d: &DenseCtx, leaf: &DenseLeaf, s: &mut DScratch, rows: &[Row],
    compiled: &[CompiledRow], tables: &Tables, extra: &[DenseRowExtra],
) -> f64 {
    let use_cache = extra.is_empty();
    let crit = {
        let dex = s.num(d.dex_idx);
        tables.sp_to_pct(if dex.is_nan() || dex == 0.0 { 0.0 } else { dex })
    };
    let mut journal: Vec<DenseUndo> = Vec::new();
    let mut row_cache: Vec<Option<(f64, f64)>> = vec![None; d.row_canon.len()];
    let mut total_damage = 0.0;
    for (ri, ((row, comp), drow)) in rows.iter().zip(compiled).zip(&d.rows).enumerate() {
        if comp.mod_spell.is_none() { continue; }
        if row.qty <= 0.0 || row.pseudo { continue; }

        let canon = d.row_canon[ri];
        let cached = if use_cache { row_cache[canon] } else { None };
        let (per_cast, flat_per_cast) = match cached {
            Some(pair) => pair,
            None => {
                let mut eff_dps_name: Option<&str> = row.dps_per_hit_name.as_deref();
                let mut eff_dps_hits = row.dps_hits;
                let mut chain_root: Option<&str> = None;
                if eff_dps_name.is_none() {
                    if let Some((name, hits, root)) = &comp.dps {
                        eff_dps_name = Some(name);
                        eff_dps_hits = row.dps_hits_override.unwrap_or(*hits);
                        chain_root = Some(root);
                    }
                }
                let final_root: Option<&str> = chain_root.or(comp.fallback_root.as_deref());
                // Plan-less row: only reachable for an empty-parts spell (the
                // build gate declines every other cause). The Obj path scores
                // it per_cast 0.0 with no flat root; yield the same pair and
                // fall through to the SHARED eff_qty/row_damage tail below, so
                // the result is bit-identical by construction, not merely
                // equal.
                let pair = match comp.plan.as_ref() {
                    None => (0.0, 0.0),
                    Some(plan) => {
                        static NO_EXTRA: std::sync::OnceLock<DenseRowExtra> = std::sync::OnceLock::new();
                        let extra_row = extra.get(ri)
                            .unwrap_or_else(|| NO_EXTRA.get_or_init(DenseRowExtra::default));
                        dense_apply_row(s, drow, extra_row, &mut journal);
                        let (mut per_cast, flat) = dense_spell_plan(
                            d, s, plan, drow, crit, tables, eff_dps_name.is_some());
                        dense_undo_row(s, &mut journal);
                        if eff_dps_name.is_some() { per_cast *= eff_dps_hits; }
                        let flat_per_cast = if final_root.is_some() { flat } else { 0.0 };
                        (per_cast, flat_per_cast)
                    }
                };
                row_cache[canon] = Some(pair);
                pair
            }
        };

        let eff_qty = if row.is_melee_time {
            let melee_period = match row.melee_cd_override {
                Some(p) => p,
                None => {
                    let tier = s.num_or0(d.atk_tier_idx);
                    let mut adj = leaf.atk_spd_idx as f64 + tier;
                    if adj < 0.0 { adj = 0.0; }
                    if adj > 6.0 { adj = 6.0; }
                    1.0 / tables.base_damage_multiplier[adj as usize]
                }
            };
            row.qty / melee_period.max(SPELL_CAST_DELAY)
        } else { row.qty };
        let row_damage = if row.dmg_excl { 0.0 } else { per_cast * eff_qty + flat_per_cast };
        total_damage += row_damage;
    }
    total_damage
}

fn dense_defense_stats(d: &DenseCtx, s: &DScratch, tables: &Tables) -> (f64, f64, f64, f64, f64) {
    let fm3 = tables.skillpoint_final_mult_3;
    let fm4 = tables.skillpoint_final_mult_4;
    let def_pct = tables.sp_to_pct(s.num_or0(d.s_def)) * fm3;
    let agi_pct = tables.sp_to_pct(s.num_or0(d.s_agi)) * fm4;
    let mut total_hp = s.num_or0(d.s_hp) + s.num_or0(d.s_hp_bonus);
    if total_hp < 5.0 { total_hp = 5.0; }
    let cd = s.num(d.class_def_idx);
    let mut def_mult = 2.0 - if cd.is_nan() { 1.0 } else { cd };
    for v in &s.def_vals {
        def_mult *= 1.0 - v / 100.0;
    }
    let agi_reduction = (100.0 - s.num_or0(d.s_agi_def)) / 100.0;
    let denom_full = agi_reduction * agi_pct + (1.0 - agi_pct) * (1.0 - def_pct);
    let ehp = total_hp / denom_full / def_mult;
    let ehp_no_agi = total_hp / ((1.0 - def_pct) * def_mult);
    let hpr = raw_to_pct(s.num_or0(d.s_hpr_raw), s.num_or0(d.s_hpr_pct) / 100.0);
    let ehpr = hpr / denom_full / def_mult;
    (total_hp, ehp, ehp_no_agi, hpr, ehpr)
}

fn dense_indirect(d: &DenseCtx, s: &DScratch, ind: &DInd, tables: &Tables) -> f64 {
    match ind {
        DInd::Ehp => dense_defense_stats(d, s, tables).1,
        DInd::EhpNoAgi => dense_defense_stats(d, s, tables).2,
        DInd::TotalHp => dense_defense_stats(d, s, tables).0,
        DInd::Hpr => dense_defense_stats(d, s, tables).3,
        DInd::Ehpr => dense_defense_stats(d, s, tables).4,
        DInd::TotalMana => {
            let mm = s.num_or0(d.s_max_mana);
            let int_mana = (tables.sp_to_pct(s.num_or0(d.s_int)) * 100.0).floor();
            100.0 + mm + int_mana
        }
        DInd::Plain(i) => s.num_or0(*i),
    }
}

/// Whether the dense lowering can serve this scenario's injected tokens, and
/// the stat keys it has to reserve slots for if so.
///
/// `Some(keys)` — pass them to `DenseCtx::build` as `extra_keys`; `None` — the
/// scenario keeps the Obj path. What a per-leaf injected token can produce has
/// to be expressible by the lowering, and its stat keys need slots up front
/// because `DScratch` is sized from the key universe. A *prop* bonus rewrites
/// the row's spell, which the lowering baked in — no slot can represent that.
///
/// Every builder of a `DenseCtx` must go through this. `dense_dynamic_score`
/// treats an unreservable key as unreachable, so a builder that skips the gate
/// does not fall back — it panics on the first leaf.
pub fn dense_injectable_keys(
    dynamic: Option<&DynamicRows>, registry: &[Value],
) -> Option<Vec<String>> {
    let mut extra_keys: Vec<String> = Vec::new();
    let Some(dy) = dynamic else { return Some(extra_keys) };

    let mut names: Vec<&str> = Vec::new();
    if let Some(n) = dy.bp_slider.as_deref() { names.push(n); }
    for (_, n) in &dy.state_sliders { names.push(n.as_str()); }

    for entry in registry {
        let Some(en) = entry.get("name").and_then(|v| v.as_str()) else { continue };
        if !names.contains(&en) { continue; }
        if entry.get("prop_bonuses").and_then(|v| v.as_array())
            .map(|a| !a.is_empty()).unwrap_or(false)
        {
            return None;
        }
        for b in entry.get("stat_bonuses").and_then(|v| v.as_array())
            .map(|a| a.as_slice()).unwrap_or(&[])
        {
            let Some(k) = b.get("key").and_then(|v| v.as_str()) else { continue };
            if k.starts_with("damMult.") || k.starts_with("defMult.") { continue; }
            // The lowering refuses these for static rows too.
            if k == "atkSpd" || k.contains('.') { return None; }
            if !extra_keys.iter().any(|e| e == k) { extra_keys.push(k.to_string()); }
        }
    }
    Some(extra_keys)
}

/// Per-row bonuses for tokens injected after the lowering was built,
/// in the three shapes `DRow` uses.
#[derive(Default, Clone)]
pub struct DenseRowExtra {
    pub stat: Vec<(u32, f64, bool)>,
    pub dam: Vec<(DMultEntry, f64, bool)>,
    pub def: Vec<(DMultEntry, f64, bool)>,
}

/// Slots in a `SimMemo`. Direct-mapped, so this is also the collision budget.
const SIM_MEMO_SLOTS: usize = 512;

/// Memo for the per-leaf simulation, keyed on everything it reads.
///
/// The simulation's only per-trial input is `dense_sim_obj`'s stat map, so two
/// trials agreeing on those values produce identical injected tokens and hence
/// identical bonuses. Greedy trials move one skill point at a time and most of
/// those moves do not touch a stat the simulation reads — 89% of trials on the
/// slider benchmark repeat an earlier input.
///
/// Direct-mapped and fixed-size, so memory is bounded however long the run is.
/// The full key is stored and compared on lookup: a hash collision costs a
/// recompute, it can never return another trial's answer.
pub struct SimMemo {
    key: Vec<u64>,
    slots: Vec<Option<(Vec<u64>, Vec<DenseRowExtra>)>>,
}

impl Default for SimMemo {
    fn default() -> Self {
        SimMemo { key: Vec::new(), slots: (0..SIM_MEMO_SLOTS).map(|_| None).collect() }
    }
}

impl SimMemo {
    /// Fills `self.key` from this trial's assembled state and returns its slot.
    ///
    /// Absent keys hash as a sentinel rather than 0.0 — the simulation reads a
    /// missing stat as 0, but a *present* NaN is a different input, and the
    /// bit pattern has to distinguish them.
    fn probe(&mut self, d: &DenseCtx, leaf: &DenseLeaf, s: &DScratch) -> usize {
        const ABSENT: u64 = 0xFFFF_FFFF_FFFF_FFFF;
        self.key.clear();
        let mut read = |i: u32| {
            if bit_get(&s.present, i) { s.vals[i as usize].to_bits() } else { ABSENT }
        };
        for (_, i) in &d.mana_keys { let v = read(*i); self.key.push(v); }
        // `dense_sim_obj` adds these two on top of `mana_keys`.
        let (hr, hp) = (read(d.s_hpr_raw), read(d.s_hpr_pct));
        self.key.push(hr);
        self.key.push(hp);
        self.key.push(leaf.has_arcanes as u64);

        // FxHash's multiply-xor round, enough for a direct-mapped table.
        let mut h: u64 = 0;
        for w in &self.key {
            h = (h.rotate_left(5) ^ w).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
        (h >> 32) as usize % SIM_MEMO_SLOTS
    }
}

/// One leaf-trial's dynamic score on the dense path.
///
/// Materializes just the stats the simulation reads (`dense_sim_obj`), runs
/// the per-leaf chain to get this trial's damage rows, resolves the injected
/// tokens' bonuses to dense slot writes, and scores. Returns `None` when an
/// injected bonus is not a plain stat or its key is absent from the dense
/// universe — the caller then has to use the Obj path.
#[allow(clippy::too_many_arguments)]
pub fn dense_dynamic_score(
    d: &DenseCtx, leaf: &DenseLeaf, s: &mut DScratch, memo: &mut SimMemo, rows: &[Row],
    compiled: &[CompiledRow], tables: &Tables, registry: &[Value], consts: &L2Consts,
    dy: &DynamicRows,
) -> Option<f64> {
    // A per-leaf unroll makes the rows themselves an output of the
    // simulation, which the memo does not carry — but dense is off for those
    // scenarios, so it never gets here.
    let memo_slot = if dy.needs_unroll { None } else {
        let slot = memo.probe(d, leaf, s);
        if memo.slots[slot].as_ref().is_some_and(|(k, _)| *k == memo.key) {
            let extra = &memo.slots[slot].as_ref().unwrap().1;
            return Some(dense_score_extra(d, leaf, s, rows, compiled, tables, extra));
        }
        Some(slot)
    };

    let mut injected: Vec<Vec<Token>> = Vec::new();
    let damage_rows = phase!(fine DYN, {
        let mini = dense_sim_obj(d, leaf, s);
        dynamic_damage_rows_split(&mini, rows, registry, tables, consts, dy, &mut injected)
    });

    let mut extra: Vec<DenseRowExtra> = Vec::with_capacity(damage_rows.len());
    let mut bonuses = Vec::new();
    let mut props: HashMap<String, PropOverride> = HashMap::new();
    for i in 0..damage_rows.len() {
        let row_tokens = injected.get(i).map(|t| t.as_slice()).unwrap_or(&[]);
        bonuses.clear();
        props.clear();
        compile_token_bonuses(row_tokens, registry, &mut bonuses, &mut props);
        // A prop bonus rewrites the row's spell, which the lowering baked in.
        if !props.is_empty() { return None; }
        let mut row_extra = DenseRowExtra::default();
        for b in &bonuses {
            match b.kind {
                // `atkSpd` and the nested-map roots are what the lowering
                // refuses at build time; refuse them here for the same reason.
                CompiledBonusKind::Stat => {
                    if b.key == "atkSpd" || b.key.contains('.') { return None; }
                    let Some(&slot) = d.idx.get(&b.key) else {
                        if std::env::var("DENSE_DYN_DEBUG").is_ok() {
                            eprintln!("dense dynamic: no slot for injected key {:?}", b.key);
                        }
                        return None;
                    };
                    row_extra.stat.push((slot, b.contrib, b.use_max));
                }
                CompiledBonusKind::DamMult =>
                    row_extra.dam.push((parse_mult_entry(&b.key), b.contrib, b.use_max)),
                CompiledBonusKind::DefMult =>
                    row_extra.def.push((parse_mult_entry(&b.key), b.contrib, b.use_max)),
            }
        }
        extra.push(row_extra);
    }
    if let Some(slot) = memo_slot {
        memo.slots[slot] = Some((memo.key.clone(), extra.clone()));
    }
    Some(dense_score_extra(d, leaf, s, &damage_rows, compiled, tables, &extra))
}

/// `dense_score` restricted to the terms whose weight sign matches `want_neg`.
///
/// Used to build the two-sided ceiling: the negative-weight terms are read
/// off the low-SP assemble and the non-negative ones off the all-150
/// assemble, so each term is taken at its own maximum.
pub fn dense_score_signed(
    d: &DenseCtx, leaf: &DenseLeaf, s: &mut DScratch, rows: &[Row],
    compiled: &[CompiledRow], tables: &Tables, want_neg: bool,
) -> f64 {
    let DObjective::Custom(weights) = &d.obj else {
        // Only blends have signed terms; anything else belongs entirely to
        // the non-negative side.
        return if want_neg { 0.0 } else { dense_score(d, leaf, s, rows, compiled, tables) };
    };
    let mut damage: Option<f64> = None;
    let mut sum = 0.0;
    for (ind, weight) in weights {
        if (*weight < 0.0) != want_neg { continue; }
        let sub = match ind {
            None => match damage {
                Some(dv) => dv,
                None => {
                    let dv = dense_combo_damage(d, leaf, s, rows, compiled, tables);
                    damage = Some(dv);
                    dv
                }
            },
            Some(ind) => dense_indirect(d, s, ind, tables),
        };
        sum += weight * sub;
    }
    sum
}

/// Objective::score over the dense trial state (bit-identical to the Obj path).
pub fn dense_score(
    d: &DenseCtx, leaf: &DenseLeaf, s: &mut DScratch, rows: &[Row],
    compiled: &[CompiledRow], tables: &Tables,
) -> f64 {
    dense_score_extra(d, leaf, s, rows, compiled, tables, &[])
}

/// `dense_score` with per-row injected bonuses (see
/// `dense_combo_damage_extra`). Only the damage term can see them; the
/// indirect stats read the assembled state, which already has them.
#[allow(clippy::too_many_arguments)]
pub fn dense_score_extra(
    d: &DenseCtx, leaf: &DenseLeaf, s: &mut DScratch, rows: &[Row],
    compiled: &[CompiledRow], tables: &Tables, extra: &[DenseRowExtra],
) -> f64 {
    match &d.obj {
        DObjective::Damage =>
            dense_combo_damage_extra(d, leaf, s, rows, compiled, tables, extra),
        DObjective::Indirect(ind) => dense_indirect(d, s, ind, tables),
        DObjective::Custom(weights) => {
            let mut damage: Option<f64> = None;
            let mut sum = 0.0;
            for (ind, weight) in weights {
                let sub = match ind {
                    None => match damage {
                        Some(dv) => dv,
                        None => {
                            let dv = dense_combo_damage_extra(
                                d, leaf, s, rows, compiled, tables, extra);
                            damage = Some(dv);
                            dv
                        }
                    },
                    Some(ind) => dense_indirect(d, s, ind, tables),
                };
                sum += weight * sub;
            }
            sum
        }
    }
}

// ── Dense direct leaf build (no Obj intermediate) ────────────────────────────
//
// build_base + the constant merge stages, lowered to indexed add programs so
// a gated leaf never materializes a JSON stat map at all. Two add semantics
// are mirrored exactly: item/tome/weapon/set sums read a Null/non-numeric
// previous value as 0 (add_item), the atree/const/static merge stages read
// it as NaN (merge_plain). The Obj base is built lazily, only for leaves
// that survive the ceiling gate.

pub struct DItem {
    pub adds: Vec<(u32, f64)>,
    pub set_name: Option<String>,
    pub crafted: bool,
    pub arcanes: bool,
}

fn item_has_arcanes(item: &Obj) -> bool {
    item.get("majorIds")
        .and_then(|v| v.as_array().or_else(|| v.get("__s").and_then(|s| s.as_array())))
        .map(|a| a.iter().any(|m| m.as_str() == Some("ARCANES")))
        .unwrap_or(false)
}

pub struct DenseDirect {
    pub template_vals: Vec<f64>,
    pub template_present: Vec<u64>,
    pub items: HashMap<String, DItem>,
    /// tome sums + weapon sums, applied after the per-leaf items (add_item order).
    pub post_item_adds: Vec<(u32, f64)>,
    /// set name → per-(count-1) bonus adds (skp keys excluded).
    pub sets: HashMap<String, Vec<Option<Vec<(u32, f64)>>>>,
    /// atree_raw / const_scaled / static_boosts scalar merge programs.
    pub atree_prog: Vec<(u32, f64)>,
    pub const_prog: Vec<(u32, f64)>,
    pub static_prog: Vec<(u32, f64)>,
    /// var-effect const term captures (read after the atree stage).
    pub term_capture: Vec<u32>,
    pub dam_mobs_idx: u32,
    pub def_mobs_idx: u32,
    /// constant adds applied to the damMult/defMult "tome" entry, then the
    /// constant tail entries, from the atree/const/static merges in order.
    pub dam_tome_adds: Vec<f64>,
    pub def_tome_adds: Vec<f64>,
    pub dam_tail: Vec<(DMultEntry, f64)>,
    pub def_tail: Vec<(DMultEntry, f64)>,
    pub atk_spd_idx: i64,
    pub template_zero_idxs: Vec<u32>,
    /// ARCANES present on tomes/weapon (leaf items OR onto this).
    pub base_arcanes: bool,
    pub hp_idx: u32,
    pub agi_def_idx: u32,
    pub hp_base: f64,
    pub class_def_idx: u32,
    pub class_def_val: f64,
}

impl DenseDirect {
    /// Lower a stat map to (idx, value) adds, mirroring add_item. Writes to
    /// keys outside the read universe are dropped — nothing dense reads them.
    fn lower_item(l2: &Layer2, item: &Obj, it: impl Fn(&str) -> Option<u32>) -> Option<DItem> {
        let mut adds = Vec::new();
        if let Some(mr) = item.get("maxRolls").and_then(as_map) {
            for (id, value) in mr {
                if l2.static_ids.iter().any(|s| s == id) { continue; }
                let v = value.as_f64().unwrap_or(f64::NAN);
                if v == 0.0 { continue; }
                if let Some(i) = it(id) { adds.push((i, v)); }
            }
        }
        for id in &l2.static_ids {
            let v = item.get(id).and_then(|x| x.as_f64()).unwrap_or(0.0);
            if v == 0.0 { continue; }
            if let Some(i) = it(id) { adds.push((i, v)); }
        }
        Some(DItem {
            adds,
            set_name: item.get("set").and_then(|v| v.as_str()).map(String::from),
            crafted: item.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false),
            arcanes: item_has_arcanes(item),
        })
    }
}

/// Mutable direct-build state; the working vectors live in DScratch, this
/// carries the leaf-constant results the trial pipeline reads.
impl DenseLeaf {
    pub fn build_direct(
        d: &DenseCtx, dd: &DenseDirect, item_names: &[&str],
    ) -> Option<DenseLeaf> {
        let mut leaf = DenseLeaf::default();
        if leaf.fill_direct(d, dd, item_names) { Some(leaf) } else { None }
    }

    /// build_direct into reused buffers; false → unknown item (caller falls
    /// back to the Obj path). Every field is fully overwritten.
    pub fn fill_direct(
        &mut self, d: &DenseCtx, dd: &DenseDirect, item_names: &[&str],
    ) -> bool {
        if trace::on() {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| eprintln!(
                "score_trace: dense universe {} keys ({} B/leaf copy), {} items; \
                 merge programs atree {} const {} static {}, tails dam {} def {}, \
                 zero-idxs {}, sets {}",
                dd.template_vals.len(), dd.template_vals.len() * 8 + dd.template_present.len() * 8,
                item_names.len(), dd.atree_prog.len(), dd.const_prog.len(),
                dd.static_prog.len(), dd.dam_tail.len(), dd.def_tail.len(),
                dd.template_zero_idxs.len(), dd.sets.len()));
            trace::add(trace::FD_CALLS, 1);
        }
        let _fd0 = if trace::fine() { Some(std::time::Instant::now()) } else { None };
        self.lc_vals.clear();
        self.lc_vals.extend_from_slice(&dd.template_vals);
        self.present.clear();
        self.present.extend_from_slice(&dd.template_present);
        if let Some(t) = _fd0 { trace::add(trace::FD_COPY, t.elapsed().as_nanos() as u64); }
        let _fd1 = if trace::fine() { Some(std::time::Instant::now()) } else { None };
        let vals = &mut self.lc_vals;
        let present = &mut self.present;

        // add_item semantics: previous Null/non-numeric reads as 0.
        macro_rules! add_item_ops {
            ($ops:expr) => {
                for &(i, v) in $ops {
                    let iu = i as usize;
                    if bit_get(present, i) {
                        let cur = vals[iu];
                        let cur = if cur.is_nan() { 0.0 } else { cur };
                        vals[iu] = cur + v;
                    } else {
                        vals[iu] = 0.0 + v;
                        bit_set(present, i);
                    }
                }
            };
        }
        if trace::fine() {
            // How much of this call is redundant with the previous one?
            let mut diff = 0u64;
            for i in 0..8 {
                let cur = item_names.get(i).map(|n| (n.as_ptr() as usize, n.len()))
                    .unwrap_or((0, 0));
                if self.probe_prev[i] != cur { diff += 1; self.probe_prev[i] = cur; }
            }
            trace::add(trace::FD_DIFF_SLOTS, diff);
            if diff == 0 { trace::add(trace::FD_SAME_ALL, 1); }
        }
        let mut set_counts: Vec<(&str, i64)> = Vec::new();
        self.has_arcanes = dd.base_arcanes;
        for name in item_names {
            let Some(item) = dd.items.get(*name) else { return false };
            add_item_ops!(&item.adds);
            if item.arcanes { self.has_arcanes = true; }
            if !item.crafted {
                if let Some(set_name) = &item.set_name {
                    match set_counts.iter_mut().find(|(n, _)| *n == set_name.as_str()) {
                        Some((_, c)) => *c += 1,
                        None => set_counts.push((set_name.as_str(), 1)),
                    }
                }
            }
        }
        add_item_ops!(&dd.post_item_adds);
        for (set_name, count) in &set_counts {
            let Some(per_count) = dd.sets.get(*set_name) else { continue };
            let Some(Some(adds)) = per_count.get((*count - 1) as usize) else { continue };
            add_item_ops!(adds);
        }

        if let Some(t) = _fd1 { trace::add(trace::FD_ITEMS, t.elapsed().as_nanos() as u64); }
        let _fd2 = if trace::fine() { Some(std::time::Instant::now()) } else { None };
        // damMult/defMult tome entries capture damMobs/defMobs here
        // (finalizeStatmap reads them before the assemble-stage merges).
        let read0 = |vals: &[f64], present: &[u64], i: u32| -> f64 {
            if bit_get(present, i) {
                let v = vals[i as usize];
                if v.is_nan() { 0.0 } else { v }
            } else { 0.0 }
        };
        let mut dam_tome = read0(vals, present, dd.dam_mobs_idx);
        let mut def_tome = read0(vals, present, dd.def_mobs_idx);
        for a in &dd.dam_tome_adds { dam_tome += a; }
        for a in &dd.def_tome_adds { def_tome += a; }

        // classDef insert, then the constant merge stages (merge_plain
        // semantics: previous non-numeric reads as NaN).
        vals[dd.class_def_idx as usize] = dd.class_def_val;
        bit_set(present, dd.class_def_idx);
        macro_rules! merge_ops {
            ($ops:expr) => {
                for &(i, v) in $ops {
                    let iu = i as usize;
                    if bit_get(present, i) {
                        vals[iu] += v;
                    } else {
                        vals[iu] = v;
                        bit_set(present, i);
                    }
                }
            };
        }
        merge_ops!(&dd.atree_prog);
        self.const_term_vals.clear();
        for &i in &dd.term_capture { self.const_term_vals.push(read0(vals, present, i)); }
        merge_ops!(&dd.const_prog);
        merge_ops!(&dd.static_prog);

        // Mult entry lists: tome first, then the constant tail.
        self.dam_entries.clear();
        self.dam_entries.push(parse_mult_entry("tome"));
        self.dam_vals.clear();
        self.dam_vals.push(dam_tome);
        for (e, v) in &dd.dam_tail { self.dam_entries.push(e.clone()); self.dam_vals.push(*v); }
        self.def_entries.clear();
        self.def_entries.push(parse_mult_entry("tome"));
        self.def_vals.clear();
        self.def_vals.push(def_tome);
        for (e, v) in &dd.def_tail { self.def_entries.push(e.clone()); self.def_vals.push(*v); }

        for &i in &d.skp_idx { bit_set(present, i); }
        self.var_out_absent.clear();
        for &ki in &d.var_slots {
            let absent = !bit_get(present, ki);
            self.var_out_absent.push(absent);
            if absent { bit_set(present, ki); }
        }
        self.atk_spd_idx = dd.atk_spd_idx;
        if let Some(t) = _fd2 { trace::add(trace::FD_TAIL, t.elapsed().as_nanos() as u64); }
        true
    }
}

// ── Dense subtree ceiling bound (mid-tree B&B on flat vectors) ───────────────
//
// BoundTables lowered onto the dense read universe: per depth, one combined
// add list (suffix_max[d] + set_upper, zero deltas dropped like the Obj
// path) plus the matching var-term-capture deltas — the Obj bound adds the
// maxima to the base BEFORE assembly, so atree var effects see the boosted
// inputs; the dense build captures term inputs mid-build and must add the
// deltas afterward or the ceiling would understate (inadmissible).
// Bit-parity with the Obj bound is not required — admissibility comes from
// monotonicity plus the relative cutoff margin — but the added quantities
// are identical, differing only in float association order.

pub struct DenseBound {
    /// table[d][h] = indexed stat deltas upper-bounding every completion of
    /// depths d.. when the remaining level-band budget is h: per slot j >= d,
    /// the per-stat running maxima over pool offsets 0..=min(h, len-1)
    /// (offsets are level-ranked, and a completion with budget h can only
    /// use offsets <= h in each slot), summed across slots, plus the global
    /// set-bonus upper bound. h is clamped to the largest pool.
    pub table: Vec<Vec<Vec<(u32, f64)>>>,
    /// Matching deltas for captured var-term inputs (see module comment).
    pub term_table: Vec<Vec<Vec<(usize, f64)>>>,
    pub h_max: i64,
    /// Last-slot cluster bounds: per cluster of `cluster_size` level-adjacent
    /// offsets, the per-stat maxima over just those items (+ set_upper) —
    /// far tighter than pool maxima, one eval covers cluster_size leaves.
    pub last_clusters: Vec<Vec<(u32, f64)>>,
    pub last_cluster_terms: Vec<Vec<(usize, f64)>>,
    pub cluster_size: usize,
    /// Coarse level: 4x-wider clusters tested first — one eval can skip
    /// four fine clusters (16 leaves) in dead regions.
    pub super_clusters: Vec<Vec<(u32, f64)>>,
    pub super_cluster_terms: Vec<Vec<(usize, f64)>>,
    pub super_size: usize,
}

impl DenseBound {
    /// Build from the slots' level-ordered item-name pools. Mirrors
    /// build_bound_tables' key set (maxRolls + static ids, clamped >= 0)
    /// but per offset prefix, lowered straight onto the read universe.
    pub fn build(
        l2: &Layer2, d: &DenseCtx, slot_pools: &[Vec<String>],
        cluster_size: usize,
    ) -> Option<DenseBound> {
        let n = slot_pools.len();
        let h_max = slot_pools.iter().map(|p| p.len() as i64 - 1).max().unwrap_or(0).max(0);
        // Set-transition deltas: adding one (non-crafted) item of set s moves
        // that set's count c -> c+1, changing the applied bonus from
        // bonuses[c-1] to bonuses[c]. Per set, the per-stat max POSITIVE
        // transition upper-bounds the delta for any prefix count — far
        // tighter than summing every set's best bonus globally, and 0 for
        // the setless majority. skp keys are excluded like build_base.
        let js_num = |v: &Value| -> f64 {
            match v {
                Value::Number(n) => n.as_f64().unwrap_or(0.0),
                Value::Bool(b) => if *b { 1.0 } else { 0.0 },
                _ => 0.0,
            }
        };
        let set_delta = |set_name: &str| -> HashMap<u32, f64> {
            let mut out = HashMap::new();
            let Some(set_data) = l2.sets_data.get(set_name) else { return out };
            let Some(bonuses) = set_data.get("bonuses").and_then(|b| b.as_array()) else { return out };
            let mut prev: HashMap<String, f64> = HashMap::new();
            for bonus in bonuses {
                let Some(bo) = bonus.as_object() else { continue };
                let mut cur: HashMap<String, f64> = HashMap::new();
                for (id, v) in bo {
                    if l2.skp_order.iter().any(|s| s == id) { continue; }
                    cur.insert(id.clone(), js_num(v));
                }
                for (id, v) in &cur {
                    let delta = v - prev.get(id).copied().unwrap_or(0.0);
                    if delta <= 0.0 { continue; }
                    let Some(&i) = d.idx.get(id.as_str()) else { continue };
                    let e = out.entry(i).or_insert(0.0);
                    if delta > *e { *e = delta; }
                }
                prev = cur;
            }
            out
        };
        // Per slot: running per-stat maxima and running max set delta by
        // offset (read-universe keys only).
        let mut per_slot: Vec<Vec<HashMap<u32, f64>>> = Vec::with_capacity(n);
        let mut per_slot_set: Vec<Vec<HashMap<u32, f64>>> = Vec::with_capacity(n);
        for pool in slot_pools {
            let mut running: HashMap<u32, f64> = HashMap::new();
            let mut running_set: HashMap<u32, f64> = HashMap::new();
            let mut by_offset = Vec::with_capacity(pool.len());
            let mut by_offset_set = Vec::with_capacity(pool.len());
            for name in pool {
                let item = l2.item_registry.get(name)?;
                let mut item_stats: HashMap<String, f64> = HashMap::new();
                l2.additive_item_stats(item, &mut item_stats);
                for (k, v) in item_stats {
                    let v = if v == f64::NEG_INFINITY { 0.0 } else { v };
                    let v = if v < 0.0 { 0.0 } else { v };
                    let Some(&i) = d.idx.get(&k) else { continue };
                    let e = running.entry(i).or_insert(0.0);
                    if v > *e { *e = v; }
                }
                let crafted = item.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false);
                if !crafted {
                    if let Some(sn) = item.get("set").and_then(|v| v.as_str()) {
                        for (i, v) in set_delta(sn) {
                            let e = running_set.entry(i).or_insert(0.0);
                            if v > *e { *e = v; }
                        }
                    }
                }
                by_offset.push(running.clone());
                by_offset_set.push(running_set.clone());
            }
            per_slot.push(by_offset);
            per_slot_set.push(by_offset_set);
        }

        let mut table = Vec::with_capacity(n + 1);
        let mut term_table = Vec::with_capacity(n + 1);
        for depth in 0..=n {
            let mut rows = Vec::with_capacity((h_max + 1) as usize);
            let mut term_rows = Vec::with_capacity((h_max + 1) as usize);
            for h in 0..=h_max {
                let mut acc: HashMap<u32, f64> = HashMap::new();
                for j in depth..n {
                    if per_slot[j].is_empty() { continue; }
                    let o = (h.min(per_slot[j].len() as i64 - 1)).max(0) as usize;
                    for (&i, &v) in &per_slot[j][o] {
                        *acc.entry(i).or_insert(0.0) += v;
                    }
                    for (&i, &v) in &per_slot_set[j][o] {
                        *acc.entry(i).or_insert(0.0) += v;
                    }
                }
                let list: Vec<(u32, f64)> = acc.into_iter().filter(|(_, v)| *v != 0.0).collect();
                let terms: Vec<(usize, f64)> = d.const_term_keys.iter().enumerate()
                    .filter_map(|(slot, k)| {
                        let &i = d.idx.get(k.as_str())?;
                        list.iter().find(|(j, _)| *j == i).map(|(_, dv)| (slot, *dv))
                    })
                    .collect();
                rows.push(list);
                term_rows.push(terms);
            }
            table.push(rows);
            term_table.push(term_rows);
        }

        // Last-slot clusters, fine and coarse.
        let build_clusters = |chunk: usize| -> Option<(Vec<Vec<(u32, f64)>>, Vec<Vec<(usize, f64)>>)> {
            let mut clusters = Vec::new();
            let mut terms_out = Vec::new();
            if chunk == 0 || n == 0 { return Some((clusters, terms_out)); }
            let pool = &slot_pools[n - 1];
            let mut c0 = 0usize;
            while c0 < pool.len() {
                let c1 = (c0 + chunk).min(pool.len());
                let mut acc: HashMap<u32, f64> = HashMap::new();
                let mut set_acc: HashMap<u32, f64> = HashMap::new();
                for name in &pool[c0..c1] {
                    let item = l2.item_registry.get(name)?;
                    let mut item_stats: HashMap<String, f64> = HashMap::new();
                    l2.additive_item_stats(item, &mut item_stats);
                    for (k, v) in item_stats {
                        let v = if !v.is_finite() || v < 0.0 { 0.0 } else { v };
                        let Some(&i) = d.idx.get(&k) else { continue };
                        let e = acc.entry(i).or_insert(0.0);
                        if v > *e { *e = v; }
                    }
                    let crafted = item.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false);
                    if !crafted {
                        if let Some(sn) = item.get("set").and_then(|v| v.as_str()) {
                            for (i, v) in set_delta(sn) {
                                let e = set_acc.entry(i).or_insert(0.0);
                                if v > *e { *e = v; }
                            }
                        }
                    }
                }
                for (i, v) in set_acc {
                    *acc.entry(i).or_insert(0.0) += v;
                }
                let list: Vec<(u32, f64)> = acc.into_iter().filter(|(_, v)| *v != 0.0).collect();
                let terms: Vec<(usize, f64)> = d.const_term_keys.iter().enumerate()
                    .filter_map(|(slot, k)| {
                        let &i = d.idx.get(k.as_str())?;
                        list.iter().find(|(j, _)| *j == i).map(|(_, dv)| (slot, *dv))
                    })
                    .collect();
                clusters.push(list);
                terms_out.push(terms);
                c0 = c1;
            }
            Some((clusters, terms_out))
        };
        let (last_clusters, last_cluster_terms) = build_clusters(cluster_size)?;
        let super_size = if cluster_size > 0 { cluster_size * 4 } else { 0 };
        let (super_clusters, super_cluster_terms) = build_clusters(super_size)?;
        Some(DenseBound { table, term_table, h_max, last_clusters, last_cluster_terms, cluster_size,
                          super_clusters, super_cluster_terms, super_size })
    }
}

/// mana_rescue on the dense path: identical shift logic, with the mana
/// check running on the mini stat map from the dense assembled state.
/// Returns true when rescued (leaving the rescued assemble in `work`).
#[allow(clippy::too_many_arguments)]
pub fn dense_mana_rescue(
    d: &DenseCtx, work: &mut DenseWork,
    base_sp: &mut [i32; 5], total_sp: &mut [i32; 5], orig_base_sp: &[i32; 5],
    rows: &[Row], registry: &[Value], tables: &Tables, consts: &L2Consts,
    compiled: Option<&[CompiledRow]>,
) -> bool {
    if consts.hp_casting { return false; }
    if consts.combo_time == 0.0 { return false; }

    const INT_IDX: usize = 2;
    let int_room = (100 - base_sp[INT_IDX]).min(150 - total_sp[INT_IDX]);
    if int_room <= 0 { return false; }

    let mut total_stealable = 0;
    let mut stealable = [0i32; 5];
    for i in 0..5 {
        if i == INT_IDX { continue; }
        stealable[i] = base_sp[i] - orig_base_sp[i];
        total_stealable += stealable[i];
    }
    if total_stealable <= 0 { return false; }
    let max_shift = total_stealable.min(int_room);
    if max_shift <= 0 { return false; }

    let saved_base = *base_sp;
    let saved_total = *total_sp;

    for frac in [0.25f64, 0.5, 0.75, 1.0] {
        let shift_target = (max_shift as f64 * frac).ceil() as i32;
        if shift_target <= 0 { continue; }

        *base_sp = saved_base;
        *total_sp = saved_total;

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

        let mut sp_f = [0f64; 5];
        for i in 0..5 { sp_f[i] = total_sp[i] as f64; }
        let DenseWork { leaf, scratch, .. } = work;
        dense_assemble(d, leaf, scratch, &sp_f);
        // Up to four fractions x five attributes of re-simulation per
        // rescued leaf, so this is the same map-building cost as the main
        // check paid many times over.
        let ok = match mana_check_passes_dense(
            rows, d, leaf, scratch, registry, tables, consts, compiled) {
            Some(v) => v,
            None => {
                let mini = dense_mana_obj(d, leaf, scratch);
                mana_check_passes(rows, &mini, registry, tables, consts, compiled)
            }
        };
        if ok { return true; }
    }

    *base_sp = saved_base;
    *total_sp = saved_total;
    false
}

/// Best-case doom assemble: like dense_assemble at `doom_sp`, but each var
/// effect is evaluated at its extremum over the reachable SP box
/// [sp_lo, 150] per attribute (term-sign-wise), and each output slot is
/// credited at t_max when higher is mana-favorable and t_min when lower is
/// (irrelevant slots take t_max — the mana mini never reads them). The
/// round/positive/max clamps are monotone, so extremal inputs stay
/// extremal. The resulting mini map upper-bounds mana feasibility for
/// every greedy/rescue outcome of this leaf.
pub fn dense_assemble_doom(
    d: &DenseCtx, leaf: &DenseLeaf, s: &mut DScratch, doom_sp: &[f64; 5], sp_lo: &[f64; 5],
) {
    s.vals.copy_from_slice(&leaf.lc_vals);
    let mut pre_lo = [0.0f64; 5];
    let mut pre_hi = [0.0f64; 5];
    for i in 0..5 {
        let mut v = doom_sp[i];
        let mut lo = sp_lo[i];
        let mut hi = 150.0f64;
        for a in &d.skp_atree_adds[i] { v += a; lo += a; hi += a; }
        pre_lo[i] = lo;
        pre_hi[i] = hi;
        for a in &d.skp_const_adds[i] { v += a; }
        for a in &d.skp_static_adds[i] { v += a; }
        s.vals[d.skp_idx[i] as usize] = v;
    }
    for eff in &d.var_effects {
        let mut t_lo = 0.0;
        t_lo += eff.const_add;
        let mut t_hi = 0.0;
        t_hi += eff.const_add;
        for term in &eff.terms {
            match term {
                DTerm::Skp(i, f) => {
                    if *f >= 0.0 { t_lo += pre_lo[*i] * f; t_hi += pre_hi[*i] * f; }
                    else { t_lo += pre_hi[*i] * f; t_hi += pre_lo[*i] * f; }
                }
                DTerm::Const(slot, f) => {
                    let c = leaf.const_term_vals[*slot] * f;
                    t_lo += c;
                    t_hi += c;
                }
            }
        }
        let clamp = |mut t: f64| -> f64 {
            if eff.round { t = round_near(t).floor(); }
            if eff.positive && t < 0.0 { t = 0.0; }
            if let Some(mx) = eff.max {
                if mx > 0.0 && t > mx { t = mx; }
                if mx < 0.0 && t < mx { t = mx; }
            }
            t
        };
        let t_lo = clamp(t_lo);
        let t_hi = clamp(t_hi);
        for (slot, first) in &eff.out_slots {
            let t = if d.var_slot_mana_dir[*slot] < 0 { t_lo } else { t_hi };
            if *first { s.var_acc[*slot] = t; } else { s.var_acc[*slot] += t; }
        }
    }
    for (slot, &ki) in d.var_slots.iter().enumerate() {
        let acc = s.var_acc[slot];
        if leaf.var_out_absent[slot] { s.vals[ki as usize] = acc; }
        else { s.vals[ki as usize] += acc; }
    }
}

/// `dense_mana_obj` plus the two stats only the *stateful* simulation reads.
///
/// The fast simulation has no HP-regen tick, so `hprRaw`/`hprPct` are absent
/// from `mana_keys`; `simulate_combo_mana_hp` does tick HP, so a dynamic-row
/// scenario materializing its stats for the per-leaf simulation needs them
/// or the HP trajectory silently differs from the Obj path's.
pub fn dense_sim_obj(d: &DenseCtx, leaf: &DenseLeaf, s: &DScratch) -> Obj {
    let mut o = dense_mana_obj(d, leaf, s);
    for (name, i) in [("hprRaw", d.s_hpr_raw), ("hprPct", d.s_hpr_pct)] {
        if o.contains_key(name) { continue; }
        if bit_get(&s.present, i) {
            let v = s.vals[i as usize];
            if v.is_nan() { o.insert(name.into(), Value::Null); }
            else { o.insert(name.into(), Value::from(v)); }
        }
    }
    o
}

/// `mana_check_passes` without materializing the mini stat map.
///
/// Same verdict logic, same simulation; the stats come off the assembled
/// dense vector by slot instead of a String-keyed map built per call.
/// Returns `None` when the scenario did not lower (no compiled rows, or a
/// charged row whose cost keys are missing from the universe), and the
/// caller falls back to the map. `SCORE_MANA_DENSE=0` forces that fallback.
pub fn mana_check_passes_dense(
    rows: &[Row], d: &DenseCtx, leaf: &DenseLeaf, s: &DScratch,
    registry: &[Value], tables: &Tables, consts: &L2Consts,
    compiled: Option<&[CompiledRow]>,
) -> Option<bool> {
    // `ok` already folds in the SCORE_MANA_DENSE kill switch, which is read
    // once when the context is built — this function runs several times per
    // leaf, and an env lookup here costs more than the sim it guards on
    // enumeration-bound scenarios.
    if !d.mana_idx.ok || compiled.is_none() { return None; }
    if d.mana_idx.row_cost.len() != rows.len() { return None; }
    if consts.combo_time == 0.0 && !consts.hp_casting {
        return Some(true);
    }
    // The map arm reads this back out of `activeMajorIDs`, which
    // `dense_mana_obj` writes from exactly this bit.
    let has_transcendence = leaf.has_arcanes;
    let oom = consts.has_oom_loop;
    let (start_mana, end_mana, hp_warn, mana_warn) = simulate_mana_fast_src(
        ManaSrc::Dense(&d.mana_idx, s), rows, has_transcendence,
        registry, tables, consts, compiled, !oom);
    if hp_warn { return Some(false); }
    if !oom && mana_warn { return Some(false); }
    if oom { return Some(true); }
    if consts.allow_downtime { return Some(end_mana > 0.0); }
    Some((start_mana - end_mana) <= 5.0)
}

/// Mini stat map for the fast mana sim, materialized from the dense
/// assembled state. Contains exactly the keys the sim and the compiled
/// cost path read, with bit-identical values (present non-numerics become
/// Null — both read as 0 through as_f64/num_or0).
pub fn dense_mana_obj(d: &DenseCtx, leaf: &DenseLeaf, s: &DScratch) -> Obj {
    let mut o = Obj::new();
    for (k, i) in &d.mana_keys {
        if bit_get(&s.present, *i) {
            let v = s.vals[*i as usize];
            if v.is_nan() { o.insert(k.clone(), Value::Null); }
            else { o.insert(k.clone(), Value::from(v)); }
        }
    }
    if let Some(spd) = &d.atk_spd_str {
        o.insert("atkSpd".into(), Value::from(spd.clone()));
    }
    if leaf.has_arcanes {
        let mut w = Obj::new();
        w.insert("__s".into(), Value::Array(vec![Value::from("ARCANES")]));
        o.insert("activeMajorIDs".into(), Value::Object(w));
    }
    o
}

/// Objective ceiling for bound deltas over an ALREADY-FILLED leaf in `work`
/// (the last-slot cluster loop fills the prefix once and probes many delta
/// sets against it). Deltas are journaled and rolled back.
pub fn dense_ceiling_cached(
    d: &DenseCtx, work: &mut DenseWork, adds: &[(u32, f64)], term_adds: &[(usize, f64)],
    rows: &[Row], compiled: &[CompiledRow], tables: &Tables,
) -> f64 {
    let leaf = &mut work.leaf;
    let mut journal: Vec<(u32, f64, bool)> = Vec::with_capacity(adds.len());
    for &(i, dv) in adds {
        let iu = i as usize;
        let was = bit_get(&leaf.present, i);
        let old = leaf.lc_vals[iu];
        let cur = if was && !old.is_nan() { old } else { 0.0 };
        leaf.lc_vals[iu] = cur + dv;
        bit_set(&mut leaf.present, i);
        journal.push((i, old, was));
    }
    let mut term_journal: Vec<(usize, f64)> = Vec::with_capacity(term_adds.len());
    for &(slot, dv) in term_adds {
        term_journal.push((slot, leaf.const_term_vals[slot]));
        leaf.const_term_vals[slot] += dv;
    }
    {
        let DenseWork { leaf, scratch, .. } = work;
        // Row-op journals rolled back after each eval, so the scratch still
        // mirrors the leaf except vals (fully overwritten by the assemble);
        // it only needs (re)sizing against this leaf.
        if scratch.vals.len() != leaf.lc_vals.len() || scratch.present.len() != leaf.present.len() {
            scratch.reset(leaf, d);
        } else {
            scratch.present.copy_from_slice(&leaf.present);
            scratch.dam_vals.clear();
            scratch.dam_vals.extend_from_slice(&leaf.dam_vals);
            scratch.def_vals.clear();
            scratch.def_vals.extend_from_slice(&leaf.def_vals);
        }
        let ceiling_sp = [150f64; 5];
        dense_assemble(d, leaf, scratch, &ceiling_sp);
    }
    let v = {
        let DenseWork { leaf, scratch, .. } = work;
        dense_score(d, leaf, scratch, rows, compiled, tables)
    };
    let leaf = &mut work.leaf;
    for (slot, old) in term_journal.into_iter().rev() {
        leaf.const_term_vals[slot] = old;
    }
    for (i, old, was) in journal.into_iter().rev() {
        leaf.lc_vals[i as usize] = old;
        if !was { leaf.present[(i / 64) as usize] &= !(1u64 << (i % 64)); }
    }
    v
}

/// Objective ceiling for `prefix + arbitrary bound deltas` at all-150 SP.
#[allow(clippy::too_many_arguments)]
pub fn dense_ceiling_with(
    d: &DenseCtx, adds: &[(u32, f64)], term_adds: &[(usize, f64)], prefix_names: &[&str],
    work: &mut DenseWork, rows: &[Row], compiled: &[CompiledRow], tables: &Tables,
) -> Option<f64> {
    let dd = d.direct.as_ref()?;
    if !work.leaf.fill_direct(d, dd, prefix_names) { return None; }
    let leaf = &mut work.leaf;
    for &(i, dv) in adds {
        let iu = i as usize;
        let cur = if bit_get(&leaf.present, i) {
            let v = leaf.lc_vals[iu];
            if v.is_nan() { 0.0 } else { v }
        } else { 0.0 };
        leaf.lc_vals[iu] = cur + dv;
        bit_set(&mut leaf.present, i);
    }
    for &(slot, dv) in term_adds {
        leaf.const_term_vals[slot] += dv;
    }
    work.scratch.reset(&work.leaf, d);
    let DenseWork { leaf, scratch, .. } = work;
    let ceiling_sp = [150f64; 5];
    dense_assemble(d, leaf, scratch, &ceiling_sp);
    Some(dense_score(d, leaf, scratch, rows, compiled, tables))
}

/// Subtree ceiling on the dense path with the level-banded suffix maxima:
/// direct leaf build from the prefix (unfilled slots hold their none-items)
/// + the (next_depth, remaining-budget) bound deltas + all-150 assemble +
/// objective score. None -> caller falls back to the Obj-path bound.
#[allow(clippy::too_many_arguments)]
pub fn dense_subtree_ceiling(
    d: &DenseCtx, db: &DenseBound, next_depth: usize, hi_rem: i64, prefix_names: &[&str],
    work: &mut DenseWork, rows: &[Row], compiled: &[CompiledRow], tables: &Tables,
) -> Option<f64> {
    let h = hi_rem.clamp(0, db.h_max) as usize;
    dense_ceiling_with(d, &db.table[next_depth][h], &db.term_table[next_depth][h],
                       prefix_names, work, rows, compiled, tables)
}

// ── check_thresholds (pure/engine.js) ────────────────────────────────────────
//
// The exact post-assemble restriction check the worker runs after greedy SP
// (and again after a successful mana rescue). Evaluated on the fully
// assembled stats; ge/le against the configured value.

pub struct Threshold {
    pub stat: String,
    pub ge: bool,
    pub value: f64,
}

pub fn parse_thresholds(fixture: &Value) -> Vec<Threshold> {
    fixture["layer2"]["restrictions"]["stat_thresholds"].as_array()
        .map(|a| a.iter().filter_map(|t| {
            Some(Threshold {
                stat: t.get("stat")?.as_str()?.to_string(),
                ge: t.get("op")?.as_str()? == "ge",
                value: t.get("value")?.as_f64()?,
            })
        }).collect())
        .unwrap_or_default()
}

pub fn parse_spell_base_costs(fixture: &Value) -> HashMap<i64, f64> {
    let mut out = HashMap::new();
    if let Some(m) = fixture["layer2"]["spell_base_costs"].as_object() {
        for (k, v) in m {
            if let (Ok(n), Some(c)) = (k.parse::<i64>(), v.as_f64()) {
                out.insert(n, c);
            }
        }
    }
    out
}

fn spell_cost_capped(int_v: f64, raw: f64, pct: f64, fin: f64, base_cost: f64,
                     tables: &Tables, consts: &L2Consts) -> f64 {
    let int_reduction = tables.sp_to_pct(int_v) * consts.skillpoint_final_mult_2;
    let mut cost = base_cost * (1.0 - int_reduction);
    cost += raw;
    cost *= 1.0 + pct / 100.0;
    cost *= 1.0 + fin / 100.0;
    js_max(1.0, cost)
}

pub fn check_thresholds_obj(
    stats: &StatsView, th: &[Threshold], base_costs: &HashMap<i64, f64>,
    tables: &Tables, consts: &L2Consts,
) -> bool {
    let mut def: Option<(f64, f64, f64, f64, f64)> = None;
    for t in th {
        let v = match t.stat.as_str() {
            "ehp" => def.get_or_insert_with(|| defense_stats(stats, tables)).1,
            "ehp_no_agi" => def.get_or_insert_with(|| defense_stats(stats, tables)).2,
            "total_hp" => def.get_or_insert_with(|| defense_stats(stats, tables)).0,
            "ehpr" => def.get_or_insert_with(|| defense_stats(stats, tables)).4,
            "hpr" => def.get_or_insert_with(|| defense_stats(stats, tables)).3,
            "total_mana" => {
                let mm = stats.num_or0("maxMana");
                let int_mana = (tables.sp_to_pct(stats.num_or0("int")) * 100.0).floor();
                100.0 + mm + int_mana
            }
            s if s.starts_with("finalSpellCost") => {
                let n: i64 = s[s.len() - 1..].parse().unwrap_or(-1);
                let Some(&base) = base_costs.get(&n) else { continue };
                spell_cost_capped(
                    stats.num_or0("int"),
                    stats.num_or0(&format!("spRaw{}", n)),
                    stats.num_or0(&format!("spPct{}", n)),
                    stats.num_or0(&format!("spPct{}Final", n)),
                    base, tables, consts)
            }
            s => stats.num_or0(s),
        };
        if t.ge && v < t.value { return false; }
        if !t.ge && v > t.value { return false; }
    }
    true
}

/// Threshold kinds lowered onto the dense read universe.
pub enum DThresh {
    Ehp, EhpNoAgi, TotalHp, Ehpr, Hpr, TotalMana,
    SpellCost { raw: u32, pct: u32, fin: u32, base: f64 },
    Plain(u32),
    /// finalSpellCost with no configured base cost — JS `continue`s.
    Skip,
}

pub fn dense_check_thresholds(
    d: &DenseCtx, s: &DScratch, tables: &Tables, consts: &L2Consts,
) -> bool {
    let mut def: Option<(f64, f64, f64, f64, f64)> = None;
    for (kind, ge, value) in &d.thresholds {
        let v = match kind {
            DThresh::Ehp => def.get_or_insert_with(|| dense_defense_stats(d, s, tables)).1,
            DThresh::EhpNoAgi => def.get_or_insert_with(|| dense_defense_stats(d, s, tables)).2,
            DThresh::TotalHp => def.get_or_insert_with(|| dense_defense_stats(d, s, tables)).0,
            DThresh::Ehpr => def.get_or_insert_with(|| dense_defense_stats(d, s, tables)).4,
            DThresh::Hpr => def.get_or_insert_with(|| dense_defense_stats(d, s, tables)).3,
            DThresh::TotalMana => {
                let mm = s.num_or0(d.s_max_mana);
                let int_mana = (tables.sp_to_pct(s.num_or0(d.s_int)) * 100.0).floor();
                100.0 + mm + int_mana
            }
            DThresh::SpellCost { raw, pct, fin, base } => spell_cost_capped(
                s.num_or0(d.s_int), s.num_or0(*raw), s.num_or0(*pct), s.num_or0(*fin),
                *base, tables, consts),
            DThresh::Plain(i) => s.num_or0(*i),
            DThresh::Skip => continue,
        };
        if *ge && v < *value { return false; }
        if !*ge && v > *value { return false; }
    }
    true
}
