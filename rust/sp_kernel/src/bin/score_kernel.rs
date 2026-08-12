//! Differential validator for the combo-damage scoring pipeline (P2.4).
//!
//! Validates sp_kernel::scoring against fixtures exported by
//! test_solver_search.js with SOLVER_EXPORT_SCORE=<path>: layer 1
//! (damage on the worker-assembled combo_base), layer 2 assembly
//! (rebuild combo_base from raw items), and the full leaf pipeline
//! (SP solve -> greedy -> mana -> score). Bit-exact or exit 1.
//!
//! Usage: score_kernel <fixture.json>

use serde_json::Value;
use sp_kernel::scoring::*;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::time::Instant;

/// Compare an assembled combo_base against the worker-exported one.
/// Missing numeric keys are 0 (the worker's vector materialization drops
/// non-base zeros); nested maps, the majorID set, and strings compare
/// structurally.
fn diff_stat_maps(mine: &Obj, expected: &Obj) -> Vec<String> {
    let mut diffs = Vec::new();
    let mut keys: Vec<&String> = mine.keys().chain(expected.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let a = mine.get(k.as_str());
        let b = expected.get(k.as_str());
        match (a, b) {
            (Some(Value::Number(x)), Some(Value::Number(y))) => {
                let (x, y) = (x.as_f64().unwrap_or(f64::NAN), y.as_f64().unwrap_or(f64::NAN));
                if x.to_bits() != y.to_bits() && !(x == 0.0 && y == 0.0) {
                    diffs.push(format!("{}: mine {:?} vs expected {:?}", k, x, y));
                }
            }
            (Some(Value::Number(x)), None) => {
                if x.as_f64() != Some(0.0) { diffs.push(format!("{}: mine {:?} vs missing", k, x)); }
            }
            (None, Some(Value::Number(y))) => {
                if y.as_f64() != Some(0.0) { diffs.push(format!("{}: missing vs expected {:?}", k, y)); }
            }
            (Some(Value::String(x)), Some(Value::String(y))) => {
                if x != y { diffs.push(format!("{}: mine {:?} vs expected {:?}", k, x, y)); }
            }
            (Some(a), Some(b)) => {
                if let (Some(ma), Some(mb)) = (a.get("__m"), b.get("__m")) {
                    if let (Some(ma), Some(mb)) = (ma.as_object(), mb.as_object()) {
                        for (mk, mv) in ma.iter() {
                            let ev = mb.get(mk).and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let sv = mv.as_f64().unwrap_or(f64::NAN);
                            if sv.to_bits() != ev.to_bits() && !(sv == 0.0 && ev == 0.0) {
                                diffs.push(format!("{}.{}: mine {:?} vs expected {:?}", k, mk, sv, ev));
                            }
                        }
                        for (mk, mv) in mb.iter() {
                            if !ma.contains_key(mk) && mv.as_f64() != Some(0.0) {
                                diffs.push(format!("{}.{}: missing vs expected {:?}", k, mk, mv));
                            }
                        }
                        continue;
                    }
                }
                if let (Some(sa), Some(sb)) = (a.get("__s"), b.get("__s")) {
                    let (sa, sb) = (sa.as_array().cloned().unwrap_or_default(), sb.as_array().cloned().unwrap_or_default());
                    let mut xa: Vec<String> = sa.iter().filter_map(|v| v.as_str().map(String::from)).collect();
                    let mut xb: Vec<String> = sb.iter().filter_map(|v| v.as_str().map(String::from)).collect();
                    xa.sort(); xb.sort();
                    if xa != xb { diffs.push(format!("{}: set mine {:?} vs expected {:?}", k, xa, xb)); }
                    continue;
                }
                if a != b { diffs.push(format!("{}: structural mismatch", k)); }
            }
            (Some(a), None) => diffs.push(format!("{}: mine {:?} vs missing", k, a)),
            (None, Some(b)) => diffs.push(format!("{}: missing vs expected {:?}", k, b)),
            (None, None) => {}
        }
    }
    diffs
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

    let layer2 = Layer2::parse(&fixture);
    let layer2_supported = layer2.as_ref()
        .map(|l| l.scaling_kind == "cached" || l.scaling_kind == "split")
        .unwrap_or(false);
    let mut l2_pass = 0u64;
    let mut l2_fail = 0u64;
    let mut l2_score_pass = 0u64;
    let mut l2_score_fail = 0u64;

    // Layer 2 full-pipeline state (greedy + mana): SP kernel + guild unit.
    let l2consts = L2Consts::parse(&fixture);
    let mut kernel = sp_kernel::Kernel::new();
    let guild_unit: Option<sp_kernel::Unit> = fixture["layer2"].get("guild_tome_sm")
        .and_then(as_map)
        .map(|sm| {
            let arr5 = |k: &str| -> [i32; 5] {
                let mut out = [0i32; 5];
                if let Some(a) = sm.get(k).and_then(|v| v.as_array()) {
                    for (i, x) in a.iter().take(5).enumerate() {
                        out[i] = x.as_f64().unwrap_or(0.0) as i32;
                    }
                }
                out
            };
            sp_kernel::Unit {
                crafted: sm.get("crafted").and_then(|v| v.as_bool()).unwrap_or(false),
                reqs: arr5("reqs"),
                skp: arr5("skillpoints"),
            }
        });
    let mut l3_pass = 0u64;
    let mut l3_fail = 0u64;

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
        // Layer 2: rebuild combo_base from raw items and compare.
        if layer2_supported {
            let l2 = layer2.as_ref().unwrap();
            let names: Vec<&str> = case["item_names"].as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let sp: Vec<f64> = arr_f64(&case["total_sp"]);
            match l2.assemble(&names, &sp, &weapon) {
                Ok(assembled) => {
                    let diffs = diff_stat_maps(&assembled, combo_base);
                    if diffs.is_empty() { l2_pass += 1; } else {
                        l2_fail += 1;
                        if l2_fail <= 3 {
                            eprintln!("case {}: ASSEMBLY DIFFS ({}):", i, diffs.len());
                            for d in diffs.iter().take(8) { eprintln!("    {}", d); }
                        }
                    }
                    // End-to-end: score the rebuilt map.
                    let got2 = eval_combo_damage(&assembled, &weapon, &rows, &registry, &hit_refs, &tables);
                    if got2.to_bits() == expected.to_bits() { l2_score_pass += 1; } else {
                        l2_score_fail += 1;
                        if l2_score_fail <= 3 {
                            eprintln!("case {}: L2 SCORE MISMATCH expected {:.17e} got {:.17e}", i, expected, got2);
                        }
                    }
                }
                Err(e) => {
                    l2_fail += 1;
                    if l2_fail <= 3 { eprintln!("case {}: assemble failed: {}", i, e); }
                }
            }

            // Layer 2 full pipeline: SP solve → greedy → mana → score,
            // validated against the worker's base_sp/total_sp/assigned/score.
            if let (Some(consts), Some(exp_base), Some(exp_assigned)) = (
                l2consts.as_ref(),
                case.get("base_sp").map(arr_f64),
                case.get("assigned_sp").and_then(|v| v.as_i64()),
            ) {
                let exp_total: Vec<f64> = arr_f64(&case["total_sp"]);
                match leaf_pipeline(&names, l2, &weapon, guild_unit.as_ref(),
                                    &mut kernel, &rows, &registry, &hit_refs, &tables, consts) {
                    Ok(Some(r)) => {
                        let base_ok = (0..5).all(|j| r.base_sp[j] as f64 == exp_base[j]);
                        let total_ok = (0..5).all(|j| r.total_sp[j] as f64 == exp_total[j]);
                        let ok = base_ok && total_ok
                            && r.assigned_sp as i64 == exp_assigned
                            && r.score.to_bits() == expected.to_bits();
                        if ok { l3_pass += 1; } else {
                            l3_fail += 1;
                            if l3_fail <= 3 {
                                eprintln!("case {}: PIPELINE MISMATCH", i);
                                eprintln!("    base_sp  got {:?} want {:?}", r.base_sp, exp_base);
                                eprintln!("    total_sp got {:?} want {:?}", r.total_sp, exp_total);
                                eprintln!("    assigned got {} want {}", r.assigned_sp, exp_assigned);
                                eprintln!("    score    got {:.17e} want {:.17e}", r.score, expected);
                            }
                        }
                    }
                    Ok(None) => {
                        l3_fail += 1;
                        if l3_fail <= 3 { eprintln!("case {}: pipeline says infeasible, worker scored it", i); }
                    }
                    Err(e) => {
                        l3_fail += 1;
                        if l3_fail <= 3 { eprintln!("case {}: pipeline error: {}", i, e); }
                    }
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
    if layer2_supported {
        println!(
            "layer2: assembly {} exact / {} diff | end-to-end score {} exact / {} diff",
            l2_pass, l2_fail, l2_score_pass, l2_score_fail,
        );
        if l3_pass + l3_fail > 0 {
            println!(
                "pipeline (SP+greedy+mana+score): {} exact / {} diff",
                l3_pass, l3_fail,
            );
        }
    } else if layer2.is_some() {
        println!("layer2: scaling plan unsupported (kind=full), skipped");
    }
    if fail > 0 || l2_fail > 0 || l2_score_fail > 0 || l3_fail > 0 { std::process::exit(1); }
}
