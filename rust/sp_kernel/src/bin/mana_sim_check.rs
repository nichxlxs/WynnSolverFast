//! Differential validator for the stateful mana+HP simulation port.
//!
//! Replays `fixtures/mana_sim_cases.json` (produced by
//! `js/solver/tests/gen_mana_sim_fixtures.js`) through
//! `sp_kernel::mana_sim::simulate_combo_mana_hp` and requires every scalar,
//! every `row_results` entry and every `spell_costs` entry to match the JS
//! implementation's output **bit-for-bit** (raw f64 bit patterns, so NaN and
//! -0.0 are distinguished too).
//!
//! Usage: mana_sim_check [fixtures/mana_sim_cases.json] [--verbose]

use serde_json::Value;
use sp_kernel::mana_sim::{
    compute_recast_penalties, extract_slider_names, inject_blood_pact_boosts,
    simulate_combo_mana_hp, unroll_loops_dynamic, HealthConfig, SimConsts, SimResult,
};
use sp_kernel::scoring::{parse_rows, simulate_mana_fast_ff, L2Consts, Obj, Row, Tables};

/// The exporter encodes non-finite doubles as strings ("@NaN", "@Inf").
fn dec_num(v: &Value) -> f64 {
    match v {
        Value::String(s) => match s.as_str() {
            "@NaN" => f64::NAN,
            "@Inf" => f64::INFINITY,
            "@-Inf" => f64::NEG_INFINITY,
            _ => f64::NAN,
        },
        _ => v.as_f64().unwrap_or(f64::NAN),
    }
}

/// Exact identity, not tolerance: same bits or it is a mismatch.
fn same_bits(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits()
}

struct Report {
    failures: Vec<String>,
    checks: usize,
}

impl Report {
    fn eq(&mut self, case: &str, field: &str, got: f64, want: f64) {
        self.checks += 1;
        if !same_bits(got, want) {
            self.failures.push(format!(
                "{case}: {field}: rust={got:.17e} ({:#018x}) js={want:.17e} ({:#018x})",
                got.to_bits(),
                want.to_bits()
            ));
        }
    }
    fn eq_bool(&mut self, case: &str, field: &str, got: bool, want: bool) {
        self.checks += 1;
        if got != want {
            self.failures.push(format!("{case}: {field}: rust={got} js={want}"));
        }
    }
    fn eq_str(&mut self, case: &str, field: &str, got: &str, want: &str) {
        self.checks += 1;
        if got != want {
            self.failures.push(format!("{case}: {field}: rust={got:?} js={want:?}"));
        }
    }
    fn eq_i64(&mut self, case: &str, field: &str, got: i64, want: i64) {
        self.checks += 1;
        if got != want {
            self.failures.push(format!("{case}: {field}: rust={got} js={want}"));
        }
    }
    /// Compares an `Option<f64>` against JS `null | number`.
    fn eq_opt(&mut self, case: &str, field: &str, got: Option<f64>, want: &Value) {
        self.checks += 1;
        match (got, want.is_null()) {
            (None, true) => {}
            (Some(g), false) => {
                let w = dec_num(want);
                if !same_bits(g, w) {
                    self.failures
                        .push(format!("{case}: {field}: rust={g:.17e} js={w:.17e}"));
                }
            }
            (g, _) => self
                .failures
                .push(format!("{case}: {field}: rust={g:?} js={want}")),
        }
    }
}

fn check_case(name: &str, res: &SimResult, expected: &Value, rep: &mut Report) {
    let g = |k: &str| dec_num(&expected[k]);
    rep.eq(name, "end_mana", res.end_mana, g("end_mana"));
    rep.eq(name, "start_mana", res.start_mana, g("start_mana"));
    rep.eq(name, "max_mana", res.max_mana, g("max_mana"));
    rep.eq(name, "end_hp", res.end_hp, g("end_hp"));
    rep.eq(name, "max_hp", res.max_hp, g("max_hp"));
    rep.eq(name, "total_mana_cost", res.total_mana_cost, g("total_mana_cost"));
    rep.eq(name, "total_mana_drain", res.total_mana_drain, g("total_mana_drain"));
    rep.eq(name, "melee_hits", res.melee_hits, g("melee_hits"));
    rep.eq(name, "recast_penalty_total", res.recast_penalty_total, g("recast_penalty_total"));
    rep.eq(name, "mana_wasted", res.mana_wasted, g("mana_wasted"));

    // loop_iteration_counts
    let want_loops = expected["loop_iteration_counts"].as_array().cloned().unwrap_or_default();
    rep.eq_i64(name, "loop_iteration_counts.len", res.loop_iteration_counts.len() as i64,
        want_loops.len() as i64);
    for (i, w) in want_loops.iter().enumerate() {
        let Some((gi, gc)) = res.loop_iteration_counts.get(i) else { continue };
        rep.eq_i64(name, &format!("loop_iteration_counts[{i}].row"), *gi as i64,
            w[0].as_i64().unwrap_or(-1));
        rep.eq_i64(name, &format!("loop_iteration_counts[{i}].count"), *gc,
            w[1].as_i64().unwrap_or(-1));
    }

    // spell_costs
    let want_costs = expected["spell_costs"].as_array().cloned().unwrap_or_default();
    rep.eq_i64(name, "spell_costs.len", res.spell_costs.len() as i64, want_costs.len() as i64);
    for (i, w) in want_costs.iter().enumerate() {
        let Some(sc) = res.spell_costs.get(i) else { continue };
        let f = |k: &str| format!("spell_costs[{i}].{k}");
        rep.eq_str(name, &f("name"), &sc.name, w["name"].as_str().unwrap_or(""));
        rep.eq_i64(name, &f("qty"), sc.qty, w["qty"].as_i64().unwrap_or(-1));
        rep.eq(name, &f("cost"), sc.cost, dec_num(&w["cost"]));
        rep.eq(name, &f("recast_penalty"), sc.recast_penalty, dec_num(&w["recast_penalty"]));
    }

    // row_results
    let want_rows = expected["row_results"].as_array().cloned().unwrap_or_default();
    rep.eq_i64(name, "row_results.len", res.row_results.len() as i64, want_rows.len() as i64);
    for (i, w) in want_rows.iter().enumerate() {
        let Some(rr) = res.row_results.get(i) else { continue };
        let f = |k: &str| format!("row_results[{i}].{k}");
        if w.is_null() {
            // JS left this row null (never reached) — the port must agree.
            rep.eq_bool(name, &f("<null>"), rr.filled, false);
            continue;
        }
        rep.eq_bool(name, &f("<present>"), rr.filled, true);
        if !rr.filled {
            continue;
        }
        rep.eq(name, &f("blood_pact_bonus"), rr.blood_pact_bonus, dec_num(&w["blood_pact_bonus"]));
        rep.eq_bool(name, &f("hp_warning"), rr.hp_warning, w["hp_warning"].as_bool().unwrap_or(false));
        rep.eq_bool(name, &f("mana_warning"), rr.mana_warning,
            w["mana_warning"].as_bool().unwrap_or(false));
        rep.eq_opt(name, &f("computed_delay"), rr.computed_delay, &w["computed_delay"]);
        rep.eq(name, &f("mana_lost"), rr.mana_lost, dec_num(&w["mana_lost"]));
        rep.eq(name, &f("mana_gained"), rr.mana_gained, dec_num(&w["mana_gained"]));
        rep.eq(name, &f("elapsed_time"), rr.elapsed_time, dec_num(&w["elapsed_time"]));
        rep.eq(name, &f("row_dt"), rr.row_dt, dec_num(&w["row_dt"]));
        rep.eq_opt(name, &f("cast_time"), rr.cast_time, &w["cast_time"]);
        rep.eq_opt(name, &f("delay"), rr.delay, &w["delay"]);

        let want_sv = w["state_values"].as_array().cloned().unwrap_or_default();
        rep.eq_i64(name, &f("state_values.len"), rr.state_values.len() as i64,
            want_sv.len() as i64);
        for (j, ws) in want_sv.iter().enumerate() {
            let Some((k, v)) = rr.state_values.get(j) else { continue };
            rep.eq_str(name, &f(&format!("state_values[{j}].key")), k,
                ws[0].as_str().unwrap_or(""));
            rep.eq(name, &f(&format!("state_values[{j}].value")), *v, dec_num(&ws[1]));
        }
    }
}

/// Compares a row list against the exporter's `encRows` shape.
fn check_rows(case: &str, label: &str, rows: &[Row], want: &Value, rep: &mut Report) {
    let want = want.as_array().cloned().unwrap_or_default();
    rep.eq_i64(case, &format!("{label}.len"), rows.len() as i64, want.len() as i64);
    for (i, w) in want.iter().enumerate() {
        let Some(r) = rows.get(i) else { continue };
        let f = |k: &str| format!("{label}[{i}].{k}");
        rep.eq_str(case, &f("spell_name"),
            r.spell.as_ref().and_then(|s| s.get("name")).and_then(|n| n.as_str()).unwrap_or(""),
            w["spell_name"].as_str().unwrap_or(""));
        rep.eq(case, &f("recast_penalty_per_cast"), r.recast_penalty_per_cast,
            dec_num(&w["recast_penalty_per_cast"]));
        if w["recast_penalties"].is_null() {
            rep.eq_i64(case, &f("recast_penalties<null>"), r.recast_penalties.len() as i64, 0);
        } else {
            let wp = w["recast_penalties"].as_array().cloned().unwrap_or_default();
            rep.eq_i64(case, &f("recast_penalties.len"), r.recast_penalties.len() as i64,
                wp.len() as i64);
            for (j, wv) in wp.iter().enumerate() {
                let Some(gv) = r.recast_penalties.get(j) else { continue };
                rep.eq(case, &f(&format!("recast_penalties[{j}]")), *gv, dec_num(wv));
            }
        }
        let wt = w["boost_tokens"].as_array().cloned().unwrap_or_default();
        rep.eq_i64(case, &f("boost_tokens.len"), r.tokens.len() as i64, wt.len() as i64);
        for (j, wv) in wt.iter().enumerate() {
            let Some(t) = r.tokens.get(j) else { continue };
            rep.eq_str(case, &f(&format!("boost_tokens[{j}].name")), &t.name,
                wv["name"].as_str().unwrap_or(""));
            rep.eq(case, &f(&format!("boost_tokens[{j}].value")), t.value, dec_num(&wv["value"]));
            rep.eq_bool(case, &f(&format!("boost_tokens[{j}].is_pct")), t.is_pct,
                wv["is_pct"].as_bool().unwrap_or(false));
            rep.eq_bool(case, &f(&format!("boost_tokens[{j}].manual")), t.manual,
                wv["manual"].as_bool().unwrap_or(false));
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let verbose = args.iter().any(|a| a == "--verbose");
    let path = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "fixtures/mana_sim_cases.json".to_string());

    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let fixture: Value = serde_json::from_str(&text).expect("fixture is not valid JSON");

    let tables = Tables::parse(&fixture["tables"]);
    let jc = &fixture["constants"];
    let consts = SimConsts {
        base_mana_regen: jc["base_mana_regen"].as_f64().unwrap(),
        mana_tick_seconds: jc["mana_tick_seconds"].as_f64().unwrap(),
        spell_cast_time: jc["spell_cast_time"].as_f64().unwrap(),
        spell_cast_delay: jc["spell_cast_delay"].as_f64().unwrap(),
        hpr_tick_seconds: jc["hpr_tick_seconds"].as_f64().unwrap(),
        hidden_base_hpr: jc["hidden_base_hpr"].as_f64().unwrap(),
        skillpoint_final_mult_2: fixture["tables"]["skillpoint_final_mult"][2].as_f64().unwrap(),
    };

    let mut rep = Report { failures: Vec::new(), checks: 0 };
    let cases = fixture["cases"].as_array().expect("cases[] missing");

    for case in cases {
        let name = case["name"].as_str().unwrap_or("<unnamed>");
        let stats: Obj = case["stats"].as_object().cloned().unwrap_or_default();
        let rows = parse_rows(&case["rows"]);
        let hc = HealthConfig::parse(&case["health_config"]);
        let has_trans = case["has_transcendence"].as_bool().unwrap_or(false);
        let registry: Vec<Value> =
            case["registry"].as_array().cloned().unwrap_or_default();

        let res = simulate_combo_mana_hp(
            &rows, &stats, &hc, has_trans, &registry, &tables, &consts,
        );

        // The fast sim (what the solver's leaf pipeline actually runs) must
        // match the JS fast sim, which models buff states, Blood Pact and
        // loops but tracks no state values.
        let l2 = L2Consts {
            base_mana_regen: consts.base_mana_regen,
            mana_tick_seconds: consts.mana_tick_seconds,
            spell_cast_time: consts.spell_cast_time,
            spell_cast_delay: consts.spell_cast_delay,
            skillpoint_final_mult_2: consts.skillpoint_final_mult_2,
            combo_time: 1.0,
            allow_downtime: false,
            hp_casting: false,
            sp_budget: 200,
            health: hc.clone(),
            has_oom_loop: false,
        };
        let (f_start, f_end, f_hp, f_mana) = simulate_mana_fast_ff(
            &rows, &stats, has_trans, &registry, &tables, &l2, None, false,
        );
        let want_fast = &case["expected_fast"];
        rep.eq(name, "fast.start_mana", f_start, dec_num(&want_fast["start_mana"]));
        rep.eq(name, "fast.end_mana", f_end, dec_num(&want_fast["end_mana"]));
        rep.eq_bool(name, "fast.has_hp_warning", f_hp,
            want_fast["has_hp_warning"].as_bool().unwrap_or(false));
        rep.eq_bool(name, "fast.has_mana_warning", f_mana,
            want_fast["has_mana_warning"].as_bool().unwrap_or(false));
        let before = rep.failures.len();
        check_case(name, &res, &case["expected"], &mut rep);

        // extract_slider_names
        let (bp, states) = extract_slider_names(&hc);
        let want_sn = &case["expected_slider_names"];
        rep.eq_str(name, "slider_names.bp", bp.as_deref().unwrap_or(""),
            want_sn["bp"].as_str().unwrap_or(""));
        let want_states = want_sn["states"].as_array().cloned().unwrap_or_default();
        rep.eq_i64(name, "slider_names.states.len", states.len() as i64,
            want_states.len() as i64);
        for (j, ws) in want_states.iter().enumerate() {
            let Some((sn, sl)) = states.get(j) else { continue };
            rep.eq_str(name, &format!("slider_names.states[{j}].state"), sn,
                ws[0].as_str().unwrap_or(""));
            rep.eq_str(name, &format!("slider_names.states[{j}].slider"), sl,
                ws[1].as_str().unwrap_or(""));
        }

        // dynamic unroll + recast-penalty re-run, then boost injection
        let mut flat = unroll_loops_dynamic(&rows, &res.loop_iteration_counts);
        compute_recast_penalties(&mut flat);
        check_rows(name, "unrolled", &flat, &case["expected_unrolled"], &mut rep);

        let flat_sim = simulate_combo_mana_hp(
            &flat, &stats, &hc, has_trans, &registry, &tables, &consts,
        );
        let injected = inject_blood_pact_boosts(&flat, &flat_sim, bp.as_deref(), &states);
        check_rows(name, "injected", &injected, &case["expected_injected"], &mut rep);
        if verbose {
            let status = if rep.failures.len() == before { "ok" } else { "FAIL" };
            println!("  {status:4} {name}");
        }
    }

    println!(
        "mana_sim_check: {} cases | {} value comparisons | {} mismatches",
        cases.len(),
        rep.checks,
        rep.failures.len()
    );
    if !rep.failures.is_empty() {
        for f in rep.failures.iter().take(40) {
            println!("  MISMATCH {f}");
        }
        if rep.failures.len() > 40 {
            println!("  ... {} more", rep.failures.len() - 40);
        }
        std::process::exit(1);
    }
    println!("mana_sim_check: ALL EXACT");
}
