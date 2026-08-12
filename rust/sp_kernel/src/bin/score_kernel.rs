//! Combo-damage score kernel (P2.4 layer 1).
//!
//! Faithful Rust port of the JS combo-damage evaluation pipeline:
//!   compute_combo_damage_totals → computeSpellDisplayAvg → _eval_spell_parts
//!   → calculateSpellDamage, plus apply_combo_row_boosts and
//!   apply_spell_prop_overrides.
//!
//! Validated against fixtures exported by test_solver_search.js with
//! SOLVER_EXPORT_SCORE=<path>: each case carries the exact combo_base stat
//! map the production worker assembled and the expected total combo damage;
//! this binary must reproduce every expected value bit-for-bit (f64).
//!
//! JS semantics notes (deliberately mirrored):
//!  - `stats.get(missing)` is undefined; arithmetic with it yields NaN. A
//!    missing numeric key therefore reads as NaN, and `?? 0` / `|| 0` sites
//!    use explicit helpers.
//!  - Map iteration order is insertion order (serde_json preserve_order).
//!  - Math.max/min propagate NaN.
//!
//! Healing is NOT ported: heal parts evaluate to heal_amount 0. Healing
//! never contributes to total_damage, which is the only value compared.
//!
//! Usage: score_kernel <fixture.json>

use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::time::Instant;

type Obj = serde_json::Map<String, Value>;

const DAMAGE_ELEMENTS: [&str; 6] = ["n", "e", "t", "w", "f", "a"];
const SKP_ORDER: [&str; 5] = ["str", "dex", "int", "def", "agi"];
const SPELL_CAST_DELAY: f64 = 0.1;

// ── JS numeric semantics ─────────────────────────────────────────────────────

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else if a > b { a } else { b }
}

fn js_round(v: f64) -> f64 {
    // JS Math.round: half-up toward +Infinity (Rust f64::round is half-away).
    if v.is_nan() { return f64::NAN; }
    (v + 0.5).floor()
}

fn round_near(v: f64) -> f64 {
    let eps = 0.00000001;
    if (v - js_round(v)).abs() < eps { js_round(v) } else { v }
}

// ── Stat map access (serialized-Map JSON: {"__m": {...}}) ───────────────────

fn as_map(v: &Value) -> Option<&Obj> {
    v.get("__m").and_then(|m| m.as_object())
}

/// Read-only view over a stat map; `Owned` carries boosted row stats.
enum StatsView<'a> {
    Borrowed(&'a Obj),
    Owned(Obj),
}

impl<'a> StatsView<'a> {
    fn obj(&self) -> &Obj {
        match self {
            StatsView::Borrowed(o) => o,
            StatsView::Owned(o) => o,
        }
    }
    fn has(&self, k: &str) -> bool { self.obj().contains_key(k) }
    /// stats.get(k) in a numeric context: missing → NaN (undefined arithmetic).
    fn num(&self, k: &str) -> f64 {
        match self.obj().get(k) {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
            Some(Value::Bool(b)) => if *b { 1.0 } else { 0.0 },
            _ => f64::NAN,
        }
    }
    /// stats.get(k) ?? 0 / || 0 semantics.
    fn num_or0(&self, k: &str) -> f64 {
        let v = self.num(k);
        if v.is_nan() { 0.0 } else { v }
    }
    fn str_of(&self, k: &str) -> Option<&str> {
        self.obj().get(k).and_then(|v| v.as_str())
    }
    fn nested(&self, k: &str) -> Option<&Obj> {
        self.obj().get(k).and_then(as_map)
    }
}

fn arr_f64(v: &Value) -> Vec<f64> {
    v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default()
}

fn arr_bool(v: &Value) -> Vec<bool> {
    v.as_array().map(|a| a.iter().map(|x| x.as_bool().unwrap_or(false)).collect()).unwrap_or_default()
}

// ── Game tables ──────────────────────────────────────────────────────────────

struct Tables {
    skillpoint_damage_mult: Vec<f64>,
    base_damage_multiplier: Vec<f64>,
    attack_speeds: Vec<String>,
    damage_keys: Vec<String>,
    sp_rate: f64,
    sp_cap: f64,
    sp_pct_table: Vec<f64>,
}

impl Tables {
    fn parse(v: &Value) -> Tables {
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
    fn sp_to_pct(&self, skp: f64) -> f64 {
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
    fn atk_spd_index(&self, spd: Option<&str>) -> i64 {
        match spd {
            Some(s) => self.attack_speeds.iter().position(|x| x == s).map(|i| i as i64).unwrap_or(-1),
            None => -1,
        }
    }
}

// ── calculateSpellDamage (damage_calc.js) ────────────────────────────────────

fn calculate_spell_damage(
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
struct PartResult {
    kind: Option<&'static str>, // "damage" | "heal" | None
    normal_total: [f64; 2],
    crit_total: [f64; 2],
    display: bool,
    name: String,
}

fn spell_parts(spell: &Value) -> &[Value] {
    spell.get("parts").and_then(|p| p.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
}

fn eval_spell_parts(
    stats: &StatsView, weapon: &Obj, spell: &Value, tables: &Tables,
) -> Vec<PartResult> {
    let use_speed = spell.get("use_atkspd").and_then(|v| v.as_bool()).unwrap_or(true);
    let use_spell = spell.get("scaling").and_then(|v| v.as_str()).unwrap_or("spell") == "spell";
    let base_spell = spell.get("base_spell").and_then(|v| v.as_i64()).unwrap_or(0);
    let parts = spell_parts(spell);
    let by_name: HashMap<&str, &Value> = parts.iter()
        .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(|n| (n, p)))
        .collect();

    fn eval_part<'p>(
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

fn collect_referenced_part_names(spell: &Value) -> Vec<String> {
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

fn part_produces_damage(part: &Value, by_name: &HashMap<&str, &Value>, seen: &mut Vec<String>) -> bool {
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

fn find_dps_root_name(spell: &Value) -> Option<String> {
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

fn find_dps_display_root(spell: &Value) -> Option<String> {
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

fn spell_is_dps(spell: &Value) -> bool {
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

struct DpsHitsInfo {
    per_hit_name: String,
    max_hits: f64,
    dps_chain_root: String,
}

fn compute_dps_spell_hits_info(spell: &Value) -> Option<DpsHitsInfo> {
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

    fn count_hits(name: &str, leaf: &str, by_name: &HashMap<&str, &Value>) -> f64 {
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

fn find_display_result<'r>(spell: &Value, all: &'r [PartResult]) -> Option<&'r PartResult> {
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

fn compute_spell_display_avg(
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

fn compute_spell_flat_damage(
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

struct Token {
    name: String,
    value: f64,
    is_pct: bool,
}

struct PropOverride {
    replace: Option<f64>,
    add: f64,
    base: f64,
}

fn find_all_matching_boosts<'r>(
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

fn apply_combo_row_boosts<'a>(
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
fn apply_spell_prop_overrides(
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

fn compute_melee_time_hits(
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

struct Row {
    qty: f64,
    dmg_excl: bool,
    pseudo: bool,
    is_melee_time: bool,
    melee_cd_override: Option<f64>,
    dps_per_hit_name: Option<String>,
    dps_hits: f64,
    dps_hits_override: Option<f64>,
    tokens: Vec<Token>,
    spell: Option<Value>,
}

fn parse_rows(v: &Value) -> Vec<Row> {
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
            is_melee_time: r.get("is_melee_time").and_then(|x| x.as_bool()).unwrap_or(false),
            melee_cd_override: r.get("melee_cd_override").and_then(|x| x.as_f64()),
            dps_per_hit_name: r.get("dps_per_hit_name").and_then(|x| x.as_str()).map(String::from),
            dps_hits: r.get("dps_hits").and_then(|x| x.as_f64()).unwrap_or(0.0),
            dps_hits_override: r.get("dps_hits_override").and_then(|x| x.as_f64()),
            tokens,
            spell: r.get("spell").filter(|s| !s.is_null()).cloned(),
        });
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn eval_combo_damage(
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

// ── Main: differential validation ────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args.get(1).map(String::as_str).expect("usage: score_kernel <fixture.json>");
    let text = fs::read_to_string(fixture_path).expect("cannot read fixture");
    let fixture: Value = serde_json::from_str(&text).expect("invalid fixture JSON");

    let tables = Tables::parse(&fixture["tables"]);
    let weapon = as_map(&fixture["weapon_sm"]).expect("weapon_sm must be a map").clone();
    let rows = parse_rows(&fixture["parsed_combo"]);
    let registry: Vec<Value> = fixture["boost_registry"].as_array().cloned().unwrap_or_default();

    let mut hit_refs: HashMap<i64, HashMap<String, Obj>> = HashMap::new();
    if let Some(hr) = fixture["atree_hit_refs"].as_object() {
        for (bs, parts) in hr {
            let bs_num: i64 = bs.parse().unwrap_or(i64::MIN);
            let mut m = HashMap::new();
            if let Some(po) = parts.as_object() {
                for (part_name, hits) in po {
                    if let Some(h) = hits.as_object() {
                        m.insert(part_name.clone(), h.clone());
                    }
                }
            }
            hit_refs.insert(bs_num, m);
        }
    }

    let cases = fixture["cases"].as_array().expect("cases array");
    let mut pass = 0u64;
    let mut fail = 0u64;
    let started = Instant::now();
    for (i, case) in cases.iter().enumerate() {
        let combo_base = as_map(&case["combo_base"]).expect("combo_base must be a map");
        let expected = case["expected_damage"].as_f64().expect("expected_damage");
        if let Ok(dbg_case) = env::var("SCORE_KERNEL_DEBUG_CASE") {
            if dbg_case.parse::<usize>() == Ok(i) {
                let base_view = StatsView::Borrowed(combo_base);
                let dex = base_view.num("dex");
                let crit = tables.sp_to_pct(if dex.is_nan() || dex == 0.0 { 0.0 } else { dex });
                eprintln!("case {} crit={:.17e}", i, crit);
                for (ri, row) in rows.iter().enumerate() {
                    let Some(spell) = &row.spell else { continue };
                    if row.qty <= 0.0 || row.pseudo { continue; }
                    let (stats, po) = apply_combo_row_boosts(combo_base, &row.tokens, &registry);
                    let mod_spell = apply_spell_prop_overrides(spell, &po, &hit_refs);
                    let all = eval_spell_parts(&stats, &weapon, &mod_spell, &tables);
                    for r in &all {
                        eprintln!("  row {} part {:?} norm=[{:.17e},{:.17e}] crit=[{:.17e},{:.17e}]",
                            ri, r.name, r.normal_total[0], r.normal_total[1],
                            r.crit_total[0], r.crit_total[1]);
                    }
                    let mut eff_dps_name = row.dps_per_hit_name.clone();
                    let mut eff_dps_hits = row.dps_hits;
                    if eff_dps_name.is_none() {
                        if let Some(info) = compute_dps_spell_hits_info(&mod_spell) {
                            eff_dps_name = Some(info.per_hit_name);
                            eff_dps_hits = row.dps_hits_override.unwrap_or(info.max_hits);
                        }
                    }
                    let per_cast = match &eff_dps_name {
                        Some(name) => compute_spell_display_avg(&stats, &weapon, &mod_spell, crit, &tables, Some(name)) * eff_dps_hits,
                        None => compute_spell_display_avg(&stats, &weapon, &mod_spell, crit, &tables, None),
                    };
                    let eff_qty = if row.is_melee_time {
                        compute_melee_time_hits(row.qty, &base_view, row.melee_cd_override, &tables)
                    } else { row.qty };
                    eprintln!("  row {} per_cast={:.17e} eff_qty={:.17e} dps={:?}",
                        ri, per_cast, eff_qty, eff_dps_name);
                }
            }
        }
        let got = eval_combo_damage(combo_base, &weapon, &rows, &registry, &hit_refs, &tables);
        if got.to_bits() == expected.to_bits() {
            pass += 1;
        } else {
            fail += 1;
            if fail <= 10 {
                eprintln!(
                    "case {}: MISMATCH expected {:.17e} ({:016x}) got {:.17e} ({:016x}) (diff {:.3e}) items={}",
                    i, expected, expected.to_bits(), got, got.to_bits(), (got - expected).abs(),
                    case["item_names"].as_array().map(|a| a.iter()
                        .filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
                        .unwrap_or_default(),
                );
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "score_kernel: {} cases | {} exact | {} mismatched | {:.1} evals/s",
        cases.len(), pass, fail,
        cases.len() as f64 / elapsed,
    );
    if fail > 0 { std::process::exit(1); }
}
