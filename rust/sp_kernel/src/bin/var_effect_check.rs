//! Differential validator for `scoring::eval_var_effects` — the "split"
//! scaling plan's variable half.
//!
//! Replays `fixtures/var_effect_cases.json` (from
//! `js/solver/tests/gen_var_effect_fixtures.js`) and requires every produced
//! stat to match the JS `atree_eval_stat_effects` **bit-for-bit**.
//!
//! The cases that matter are the ones the exporter currently refuses to
//! lower (support matrix A8): variable effects writing a dotted key such as
//! `damMult.Surge`, including the non-stacking sub-keys that take the max
//! instead of accumulating.
//!
//! Usage: var_effect_check [fixtures/var_effect_cases.json]

use serde_json::Value;
use sp_kernel::scoring::{eval_var_effects, Obj};

/// Flattens Rust's `{ __m: { ... } }` nesting to dotted paths, matching how
/// the exporter flattens the JS Map, so neither encoding leaks into the
/// comparison.
fn flatten(map: &Obj, prefix: &str, out: &mut Vec<(String, f64)>) {
    for (k, v) in map {
        if let Some(inner) = v.get("__m").and_then(|m| m.as_object()) {
            let p = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
            flatten(inner, &p, out);
            continue;
        }
        let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
        out.push((key, v.as_f64().unwrap_or(f64::NAN)));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned()
        .unwrap_or_else(|| "fixtures/var_effect_cases.json".to_string());
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let fixture: Value = serde_json::from_str(&text).expect("fixture is not valid JSON");

    let mut failures: Vec<String> = Vec::new();
    let mut checks = 0usize;
    let cases = fixture["cases"].as_array().expect("cases[] missing");

    for case in cases {
        let name = case["name"].as_str().unwrap_or("<unnamed>");
        let pre: Obj = case["stats"].as_object().cloned().unwrap_or_default();
        let effects: Vec<Value> =
            case["var_effects"].as_array().cloned().unwrap_or_default();

        let got_map = eval_var_effects(&effects, &pre);
        let mut got: Vec<(String, f64)> = Vec::new();
        flatten(&got_map, "", &mut got);
        got.sort_by(|a, b| a.0.cmp(&b.0));

        let want_obj = case["expected"].as_object().cloned().unwrap_or_default();
        let mut want: Vec<(String, f64)> = want_obj
            .iter()
            .map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(f64::NAN)))
            .collect();
        want.sort_by(|a, b| a.0.cmp(&b.0));

        checks += 1;
        if got.len() != want.len() {
            failures.push(format!(
                "{name}: key count: rust={:?} js={:?}",
                got.iter().map(|(k, _)| k).collect::<Vec<_>>(),
                want.iter().map(|(k, _)| k).collect::<Vec<_>>()));
            continue;
        }
        for ((gk, gv), (wk, wv)) in got.iter().zip(&want) {
            checks += 1;
            if gk != wk {
                failures.push(format!("{name}: key: rust={gk:?} js={wk:?}"));
            } else if gv.to_bits() != wv.to_bits() {
                failures.push(format!(
                    "{name}: {gk}: rust={gv:.17e} js={wv:.17e}"));
            }
        }
    }

    println!("var_effect_check: {} cases | {checks} value comparisons | {} mismatches",
             cases.len(), failures.len());
    for f in failures.iter().take(30) {
        println!("  MISMATCH {f}");
    }
    if !failures.is_empty() {
        std::process::exit(1);
    }
    println!("var_effect_check: ALL EXACT");
}
