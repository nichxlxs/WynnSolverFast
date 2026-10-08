//! R2: first-order (log-tangent) ceilings. Diagnostic stage: computed under
//! BOUND_OBSERVE only, never used to prune.
//!
//! ## The envelope
//!
//! At a fixed skill-point vector (the ceiling's own), and with the gates below
//! holding, the damage objective of `prefix + x` (x = the relaxed slots'
//! clamped item deltas, x >= 0) is bounded above by
//!
//! ```text
//! U(x) = sum_t w_t * prod_{f in t} (a_f + c_f . x),   w_t >= 0, a_f >= 0, c_f >= 0
//! ```
//!
//! Per damage part and element `i` (mirroring `dense_spell_damage`):
//!
//! - `K = str + crit * (1 + critDamPct/100)` (1 when the part ignores Str):
//!   the crit mix `(1 - crit) * norm + crit * crit_hit` folded into one factor.
//! - `D_i = base_i + add_i`, for min and max separately: the converted,
//!   speed-scaled weapon damage plus the flat element adds.
//! - `boost_i = 1 + skill + (sd/md% + dam%)/100 + elem%/100 (+ rainbow%)`.
//! - one raw term per part, `K * R` with
//!   `R = tc * (sum_i em_i raw_i + max_i em_i * prop_raw + max_{i>0} em_i * rainbow_raw)`.
//!   The engine splits `prop_raw` and `rainbow_raw` across elements by their
//!   share of pre-boost damage; the shares lie in [0, 1] and sum to at most 1,
//!   so the largest element multiplier bounds the split.
//!
//! Every factor is non-negative on the whole box (gated at the prefix point,
//! the box minimum because c_f >= 0 and x >= 0), so the `max(0, .)` clamp
//! is a no-op and the decomposition is an upper bound. Terms carry the part's
//! multiplicity through the spell plan (display chain hits, DPS hits, the
//! melee rate, flat roots) and the damage and element multipliers.
//!
//! ## The tangent
//!
//! A product of positive affine factors is log-concave, so for any point p
//! with every factor positive:
//!
//! ```text
//! prod_f (a_f + c_f . x) <= T(p) * exp(g . (x - p)),   g = sum_f c_f / (a_f + c_f . p)
//! ```
//!
//! The right side is separable over slots: the maximum over completions takes,
//! per relaxed slot, the best item for this term's gradient. With p = I (the
//! per-stat maxima, the point today's ceiling evaluates) and x <= I, every
//! exponent is <= 0. A term with T(I) = 0 has a zero factor at I, hence (non-
//! decreasing factors, x <= I) everywhere on the box, and contributes 0.
//!
//! ## Gates (any failing means no tangent bound; the caller keeps today's)
//!
//! - damage objective, no injected per-leaf rows;
//! - no relaxed item touches an index the envelope treats as constant:
//!   conversions, damMobs/defMobs, var-effect inputs feeding those (the
//!   attack-speed tier is bounded instead: melee rows take the rate at the
//!   prefix tier plus the relaxed slots' largest tier bonus);
//! - no row overlay takes a `max` on an index the envelope reads;
//! - every factor, multiplier and multiplicity is finite and >= 0 at the
//!   prefix point.
//!
//! ## Var effects (atree stat-scaling)
//!
//! An effect writes `out = clamp(t)` into its output stats, with
//! `t = const + sum f_k * input_k` (skill-point inputs are constant at the
//! ceiling SP). `round` then `floor` gives at most `t + 1e-8`, `positive` is
//! the identity once that is >= 0, and a positive `max` only lowers it, so
//! `out <= u_pre + sum_{f_k > 0} f_k * dx_k` on the box (inputs only grow;
//! negative-factor inputs only lower `out` and are dropped). A factor that
//! reads an output stat therefore gets `c_out * f_k` on each input and its
//! prefix value lifted by `c_out * (u_pre - out_pre)`. Refused when
//! `u_pre < 0` with a `positive` clamp, or below a negative `max` floor (both
//! convex there).
//!
//! `check` mode (the observe pass) verifies `U(x) >= f(x)` at real points.

use crate::scoring::{
    dense_apply_row, dense_undo_row, DObjective, DScratch, DTerm, DenseCtx, DenseLeaf,
    DenseRowExtra, DenseUndo, CompiledRow, MultTarget, PartKindPlan, Row, Tables,
    SPELL_CAST_DELAY,
};
use std::collections::HashMap;

/// `a + c . x` over dense stat indices.
#[derive(Clone, Debug)]
pub struct Factor {
    pub a: f64,
    pub c: Vec<(u32, f64)>,
}

#[derive(Clone, Debug)]
pub struct Term {
    pub w: f64,
    pub f: Vec<Factor>,
    /// Structure tag for grouping: spell parts +16; damage terms
    /// element * 2 + (0 min, 1 max); the raw term 12.
    pub tag: u16,
}

/// How `Envelope::tangent` groups terms. A group is bounded by its summed
/// value at I times exp(g_min . delta), g_min the componentwise minimum of
/// its terms' gradients: every delta = x - I is <= 0 and every gradient
/// >= 0, so g_t . delta <= g_min . delta for each term. Fewer groups is
/// cheaper (one max over items per group per slot) and never tighter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grouping { PerTerm, ByTag, One }

#[derive(Debug, Default)]
pub struct Envelope {
    pub terms: Vec<Term>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refuse {
    Objective,
    UseMax,
    /// A factor, multiplier or multiplicity is negative or not finite at the
    /// prefix point; the string names which.
    Negative(&'static str),
    Forbidden,
}

/// Indices a relaxed item must not touch for the envelope to hold.
///
/// The attack-speed tier is not among them: it only sets melee-rate rows'
/// hits per second, which is nondecreasing in it, and `build_envelope` takes
/// that rate at the prefix tier plus the relaxed slots' largest tier bonus.
pub fn forbidden_indices(d: &DenseCtx, _rows: &[Row]) -> Vec<bool> {
    let mut f = vec![false; d.n];
    let mark = |f: &mut Vec<bool>, i: u32| { if (i as usize) < f.len() { f[i as usize] = true; } };
    for &i in &d.conv_base_idx { mark(&mut f, i); }
    for r in &d.rows {
        for c in r.parts_conv.iter().flatten() { for &i in c { mark(&mut f, i); } }
    }
    if let Some(dd) = d.direct.as_ref() {
        mark(&mut f, dd.dam_mobs_idx);
        mark(&mut f, dd.def_mobs_idx);
        // A var effect whose output lands on an index the envelope treats as
        // constant (conversion, attack tier) cannot be chained, so its
        // captured inputs are forbidden. Outputs the envelope reads are
        // chained (see the module comment). Var outputs never feed var
        // inputs: inputs are captured at fill time.
        for eff in &d.var_effects {
            let feeds = eff.out_slots.iter().any(|(slot, _)| {
                let o = d.var_slots[*slot] as usize;
                f.get(o).copied().unwrap_or(false)
            });
            if !feeds { continue; }
            for t in &eff.terms {
                if let crate::scoring::DTerm::Const(slot, _) = t {
                    if let Some(&i) = dd.term_capture.get(*slot) {
                        if (i as usize) < f.len() { f[i as usize] = true; }
                    }
                }
            }
        }
    }
    f
}

fn merged(mut c: Vec<(u32, f64)>) -> Vec<(u32, f64)> {
    c.retain(|(_, v)| *v != 0.0);
    c.sort_by_key(|(i, _)| *i);
    let mut out: Vec<(u32, f64)> = Vec::with_capacity(c.len());
    for (i, v) in c {
        match out.last_mut() {
            Some((j, w)) if *j == i => *w += v,
            _ => out.push((i, v)),
        }
    }
    out
}

fn ok(v: f64) -> bool { v.is_finite() && v >= 0.0 }

/// Per output stat index: the lift of the prefix value and the input
/// coefficients (see "Var effects" in the module comment).
#[derive(Default)]
struct Chain { lift: f64, coeffs: Vec<(u32, f64)> }

fn var_chains(d: &DenseCtx, leaf: &DenseLeaf, sp: &[f64; 5]) -> Result<HashMap<u32, Chain>, Refuse> {
    let mut out: HashMap<u32, Chain> = HashMap::new();
    let Some(dd) = d.direct.as_ref() else { return Ok(out) };
    let reads = read_indices(d);
    let mut skp_pre = [0.0f64; 5];
    for i in 0..5 {
        let mut v = sp[i];
        for a in &d.skp_atree_adds[i] { v += a; }
        skp_pre[i] = v;
    }
    for eff in &d.var_effects {
        // Effects writing only stats the envelope does not read (poison, say)
        // cannot change it.
        if !eff.out_slots.iter().any(|(slot, _)| reads.get(d.var_slots[*slot] as usize).copied().unwrap_or(false)) {
            continue;
        }
        let mut t = eff.const_add;
        let mut coeffs: Vec<(u32, f64)> = Vec::new();
        for term in &eff.terms {
            match term {
                DTerm::Skp(i, f) => t += skp_pre[*i] * f,
                DTerm::Const(slot, f) => {
                    t += leaf.const_term_vals[*slot] * f;
                    if *f > 0.0 {
                        if let Some(&i) = dd.term_capture.get(*slot) { coeffs.push((i, *f)); }
                    }
                }
            }
        }
        if coeffs.is_empty() { continue; } // constant on the box: already in the prefix
        // The engine's own clamp at the prefix (dense_assemble).
        let mut o = t;
        if eff.round { o = crate::scoring::round_near(o).floor(); }
        if eff.positive && o < 0.0 { o = 0.0; }
        if let Some(mx) = eff.max {
            if mx > 0.0 && o > mx { o = mx; }
            if mx < 0.0 && o < mx { o = mx; }
        }
        let u = t + if eff.round { 1e-6 } else { 0.0 };
        if !u.is_finite() || !o.is_finite() { return Err(Refuse::Negative("var_effect")); }
        if eff.positive && u < 0.0 { return Err(Refuse::Negative("var_effect")); }
        if let Some(mx) = eff.max { if mx < 0.0 && u < mx { return Err(Refuse::Negative("var_effect")); } }
        for (slot, _) in &eff.out_slots {
            let e = out.entry(d.var_slots[*slot]).or_default();
            e.lift += u - o;
            e.coeffs.extend(coeffs.iter().cloned());
        }
    }
    Ok(out)
}

/// Rewrites a factor through the var chains: each coefficient on an output
/// stat also lands on that effect's inputs, and the prefix value is lifted.
fn chained(f: Factor, chains: &HashMap<u32, Chain>) -> Factor {
    if chains.is_empty() { return f; }
    let mut a = f.a;
    let mut c = f.c.clone();
    for &(k, ck) in &f.c {
        if let Some(ch) = chains.get(&k) {
            a += ck * ch.lift;
            for &(i, fi) in &ch.coeffs { c.push((i, ck * fi)); }
        }
    }
    Factor { a, c: merged(c) }
}

/// Builds the envelope at the assembled prefix point in `s`.
///
/// `s` must hold the prefix (relaxed slots at their none-items) assembled at
/// the ceiling's skill points, with `ceiling_crit_floor_dense` applied (the
/// crit factor K is only an upper bound with that floor; see
/// CRIT_CEILING_FLOOR). Row overlays are applied and undone here.
pub fn build_envelope(
    d: &DenseCtx, s: &mut DScratch, rows: &[Row], compiled: &[CompiledRow], tables: &Tables,
    leaf: &DenseLeaf, sp: &[f64; 5], tier_extra: f64,
) -> Result<Envelope, Refuse> {
    if !matches!(d.obj, DObjective::Damage) { return Err(Refuse::Objective); }
    let atk_spd_idx = leaf.atk_spd_idx;
    let chains = var_chains(d, leaf, sp)?;
    let reads = read_indices(d);
    let crit = {
        let dex = s.num(d.dex_idx);
        tables.sp_to_pct(if dex.is_nan() || dex == 0.0 { 0.0 } else { dex })
    };
    if !ok(crit) { return Err(Refuse::Negative("crit")); }
    let mut env = Envelope::default();
    let no_extra = DenseRowExtra::default();
    let mut journal: Vec<DenseUndo> = Vec::new();
    for ((row, comp), drow) in rows.iter().zip(compiled).zip(&d.rows) {
        if comp.mod_spell.is_none() { continue; }
        if row.qty <= 0.0 || row.pseudo || row.dmg_excl { continue; }
        let Some(plan) = comp.plan.as_ref() else { continue };

        let mut eff_dps = row.dps_per_hit_name.is_some();
        let mut eff_dps_hits = row.dps_hits;
        let mut chain_root = false;
        if !eff_dps {
            if let Some((_, hits, _)) = &comp.dps {
                eff_dps = true;
                eff_dps_hits = row.dps_hits_override.unwrap_or(*hits);
                chain_root = true;
            }
        }
        let has_final_root = chain_root || comp.fallback_root.is_some();

        let eff_qty = if row.is_melee_time {
            let period = match row.melee_cd_override {
                Some(p) => p,
                None => {
                    // Highest tier any completion reaches: the rate only
                    // rises with it (base_damage_multiplier is increasing).
                    let tier = s.num_or0(d.atk_tier_idx) + tier_extra.max(0.0);
                    let adj = (atk_spd_idx as f64 + tier).clamp(0.0, 6.0);
                    1.0 / tables.base_damage_multiplier[adj as usize]
                }
            };
            row.qty / period.max(SPELL_CAST_DELAY)
        } else { row.qty };

        // Part multiplicities through the plan (dense_spell_plan's eval).
        let n = plan.parts.len();
        let mut mult = vec![0.0f64; n];
        fn add(i: usize, m: f64, plan: &crate::scoring::SpellPlan, mult: &mut [f64]) -> bool {
            match &plan.parts[i].kind {
                PartKindPlan::Damage(_) => { mult[i] += m; true }
                PartKindPlan::Heal => true,
                PartKindPlan::Total(edges) => {
                    for (j, hits, tick) in edges {
                        if plan.parts[*j].static_kind != Some("damage") { continue; }
                        let eff = if *tick { 1.0 / ((1.0 / hits * 20.0).floor() * 0.05) } else { *hits };
                        if !ok(eff) { return false; }
                        if !add(*j, m * eff, plan, mult) { return false; }
                    }
                    true
                }
            }
        }
        let display = if eff_dps { plan.dps_display_idx } else { plan.display_idx };
        if let Some(i) = display {
            if plan.parts[i].static_kind == Some("damage") {
                let m = eff_qty * if eff_dps { eff_dps_hits } else { 1.0 };
                if !ok(m) || !add(i, m, plan, &mut mult) { return Err(Refuse::Negative("multiplicity")); }
            }
        }
        if has_final_root {
            for &i in &plan.flat_idxs {
                if !add(i, 1.0, plan, &mut mult) { return Err(Refuse::Negative("multiplicity")); }
            }
        }
        if mult.iter().all(|&m| m == 0.0) { continue; }

        for &(i, _, use_max) in &drow.stat_ops {
            if use_max && reads.get(i as usize).copied().unwrap_or(false) { return Err(Refuse::UseMax); }
        }
        dense_apply_row(s, drow, &no_extra, &mut journal);
        let mut res = Ok(());
        for (j, &m) in mult.iter().enumerate() {
            if m == 0.0 { continue; }
            let PartKindPlan::Damage(dp) = &plan.parts[j].kind else { continue };
            let Some(conv_idx) = drow.parts_conv[j].as_ref() else { res = Err(Refuse::Negative("conv")); break };
            if let Err(e) = part_terms(d, s, &dp.multipliers, plan.use_spell, !plan.use_speed,
                                       Some(&dp.part_id), !dp.use_str, &dp.ignored_mults, conv_idx,
                                       tables, crit, m, &chains, &mut env.terms) {
                res = Err(e);
                break;
            }
        }
        dense_undo_row(s, &mut journal);
        res?;
    }
    Ok(env)
}

/// Every index `part_terms` reads off the scratch as a decision variable.
fn read_indices(d: &DenseCtx) -> Vec<bool> {
    let mut r = vec![false; d.n];
    let mut mark = |i: u32| { if (i as usize) < r.len() { r[i as usize] = true; } };
    for a in [&d.dam_add_min_idx, &d.dam_add_max_idx, &d.sd_pct_idx, &d.md_pct_idx, &d.dam_pct_idx,
              &d.sd_raw_idx, &d.md_raw_idx, &d.dam_raw_idx] {
        for &i in a.iter() { mark(i); }
    }
    for i in [d.s_sd_pct, d.s_md_pct, d.s_dam_pct, d.s_r_sd_pct, d.s_r_md_pct, d.s_r_dam_pct,
              d.s_sd_raw, d.s_md_raw, d.s_dam_raw, d.s_r_sd_raw, d.s_r_md_raw, d.s_r_dam_raw,
              d.s_crit_dam_pct] {
        mark(i);
    }
    r
}

#[allow(clippy::too_many_arguments)]
fn part_terms(
    d: &DenseCtx, s: &DScratch, mults: &[f64], use_spell: bool, ignore_speed: bool,
    part_filter: Option<&str>, ignore_str: bool, ignored: &[String], conv_idx: &[u32; 6],
    tables: &Tables, crit: f64, weight: f64, chains: &HashMap<u32, Chain>, out: &mut Vec<Term>,
) -> Result<(), Refuse> {
    // Conversions, base damages, total conversion: constant under the gates
    // (no relaxed item touches a conversion index). Same arithmetic as
    // dense_spell_damage.
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
    let mut base = [[0.0f64; 2]; 6];
    let mut weapon_min = 0.0;
    let mut weapon_max = 0.0;
    for i in 0..6 {
        base[i] = [d.w_damages[i][0] * neutral_convert, d.w_damages[i][1] * neutral_convert];
        weapon_min += d.w_damages[i][0];
        weapon_max += d.w_damages[i][1];
    }
    let mut total_convert = 0.0;
    for i in 1..=5 {
        if conversions[i] > 0.0 {
            let f = conversions[i] / 100.0;
            base[i][0] += f * weapon_min;
            base[i][1] += f * weapon_max;
            present[i] = true;
            total_convert += f;
        }
    }
    total_convert += conversions[0] / 100.0;
    if !ignore_speed {
        let m = d.w_spd_mult;
        for b in base.iter_mut() { b[0] *= m; b[1] *= m; }
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
    let str_boost = if ignore_str { 1.0 } else { 1.0 + skill_boost[1] };
    let mut damage_mult = 1.0f64;
    let mut em = [1.0f64; 6];
    for (e, v) in s.dam_entries.iter().zip(&s.dam_vals) {
        if let Some(sm) = &e.spell_match {
            if Some(&**sm) != part_filter { continue; }
        }
        if ignored.iter().any(|m| m.as_str() == &*e.key) { continue; }
        match e.target {
            MultTarget::All => damage_mult *= 1.0 + v / 100.0,
            MultTarget::MeleeOnly => { if !use_spell { damage_mult *= 1.0 + v / 100.0; } }
            MultTarget::Ele(i) => em[i] *= 1.0 + v / 100.0,
            MultTarget::Inert => {}
        }
    }
    if !ok(damage_mult) { return Err(Refuse::Negative("damage_mult")); }
    if !ok(str_boost) { return Err(Refuse::Negative("str_boost")); }
    if !ok(total_convert) { return Err(Refuse::Negative("total_convert")); }
    if !em.iter().all(|&m| ok(m)) { return Err(Refuse::Negative("element_mult")); }

    // K = str + crit * crit_mult, crit_mult = 1 + critDamPct / 100.
    let k = if ignore_str {
        Factor { a: 1.0, c: Vec::new() }
    } else {
        Factor {
            a: str_boost + crit * (1.0 + s.num(d.s_crit_dam_pct) / 100.0),
            c: merged(vec![(d.s_crit_dam_pct, crit / 100.0)]),
        }
    };
    let k = chained(k, chains);
    if !ok(k.a) { return Err(Refuse::Negative("crit_factor")); }

    let static_boost = (s.num(spec_pct_s) + s.num(d.s_dam_pct)) / 100.0;
    let r_pct = (s.num(r_pct_s) + s.num(d.s_r_dam_pct)) / 100.0;
    for i in 0..6 {
        // D_i for min and max. An element with no damage and no flat add the
        // items can raise is identically zero (its share of the split raw is
        // zero too), whatever its boost.
        let ds: [(f64, Vec<(u32, f64)>); 2] = std::array::from_fn(|mm| {
            let add_idx = if mm == 0 { d.dam_add_min_idx[i] } else { d.dam_add_max_idx[i] };
            if present[i] {
                (base[i][mm] + s.num(add_idx), merged(vec![(add_idx, 1.0)]))
            } else {
                (base[i][mm], Vec::new())
            }
        });
        if ds.iter().all(|(a, c)| *a == 0.0 && c.is_empty()) { continue; }
        if !ds.iter().all(|(a, _)| ok(*a)) { return Err(Refuse::Negative("base_damage")); }

        let mut boost_c = vec![(spec_pct_s, 0.01), (d.s_dam_pct, 0.01),
                               (spec_pct[i], 0.01), (d.dam_pct_idx[i], 0.01)];
        let mut boost_a = 1.0 + skill_boost[i] + static_boost
            + (s.num(spec_pct[i]) + s.num(d.dam_pct_idx[i])) / 100.0;
        if i > 0 {
            boost_a += r_pct;
            boost_c.push((r_pct_s, 0.01));
            boost_c.push((d.s_r_dam_pct, 0.01));
        }
        if !boost_a.is_finite() { return Err(Refuse::Negative("boost")); }
        // D >= 0 and B >= 0 below, so max(0, D*boost + B) <= D*max(0, boost)
        // + B, and max(0, a + c.x) <= max(0, a) + c.x for c >= 0, x >= 0.
        // Chain first (the lift is part of the prefix value), then clamp.
        let boost = chained(Factor { a: boost_a, c: merged(boost_c) }, chains);
        let boost = Factor { a: boost.a.max(0.0), c: boost.c };
        for (mm, (a, c)) in ds.into_iter().enumerate() {
            let w = weight * 0.5 * damage_mult * em[i];
            if w > 0.0 {
                let dfac = chained(Factor { a, c }, chains);
                if !ok(dfac.a) { return Err(Refuse::Negative("base_damage")); }
                let tag = if use_spell { 16 } else { 0 } + (i * 2 + mm) as u16;
                out.push(Term { w, f: vec![k.clone(), dfac, boost.clone()], tag });
            }
        }
    }

    // Raw: R = tc * (sum_i em_i raw_i + E0 * prop_raw + E1 * rainbow_raw),
    // each summand at max(0, its prefix value): the engine adds them before
    // the element clamp with shares in [0, 1], so the positive part bounds
    // every contribution (see the module comment).
    let e0 = em.iter().cloned().fold(0.0f64, f64::max);
    let e1 = em[1..].iter().cloned().fold(0.0f64, f64::max);
    let prop = s.num(spec_raw_s) + s.num(d.s_dam_raw);
    let rainbow = s.num(r_raw_s) + s.num(d.s_r_dam_raw);
    if !prop.is_finite() { return Err(Refuse::Negative("prop_raw")); }
    if !rainbow.is_finite() { return Err(Refuse::Negative("rainbow_raw")); }
    let tc = total_convert;
    let mut ra = tc * (e0 * prop.max(0.0) + e1 * rainbow.max(0.0));
    let mut rc = vec![(spec_raw_s, tc * e0), (d.s_dam_raw, tc * e0),
                      (r_raw_s, tc * e1), (d.s_r_dam_raw, tc * e1)];
    for i in 0..6 {
        if !present[i] { continue; }
        let raw = s.num(spec_raw[i]) + s.num(d.dam_raw_idx[i]);
        if !raw.is_finite() { return Err(Refuse::Negative("element_raw")); }
        ra += tc * em[i] * raw.max(0.0);
        rc.push((spec_raw[i], tc * em[i]));
        rc.push((d.dam_raw_idx[i], tc * em[i]));
    }
    let w = weight * damage_mult;
    if w > 0.0 {
        let r = chained(Factor { a: ra, c: merged(rc) }, chains);
        let tag = if use_spell { 16 } else { 0 } + 12;
        out.push(Term { w, f: vec![k, Factor { a: r.a.max(0.0), c: r.c }], tag });
    }
    Ok(())
}

impl Envelope {
    /// U(x) for a sparse x (sorted or not).
    pub fn value(&self, x: &[(u32, f64)], dense: &mut Vec<f64>) -> f64 {
        scatter(x, dense);
        let mut total = 0.0;
        for t in &self.terms {
            let mut p = t.w;
            for f in &t.f {
                p *= f.a + f.c.iter().map(|&(i, c)| c * dense[i as usize]).sum::<f64>();
            }
            total += p;
        }
        gather_clear(x, dense);
        total
    }

    /// The tangent bound over completions that take one item per relaxed
    /// slot from `slots[s]`, linearized at the per-stat maxima I. Returns
    /// (tangent bound, U(I)); None when an item touches a forbidden index.
    pub fn tangent(
        &self, slots: &[&[Vec<(u32, f64)>]], forbidden: &[bool], dense: &mut Vec<f64>,
        grouping: Grouping,
    ) -> Result<(f64, f64), u32> {
        // Per-slot maxima, and their sum p = I.
        let mut supers: Vec<Vec<(u32, f64)>> = Vec::with_capacity(slots.len());
        for items in slots {
            let mut m: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();
            for it in items.iter() {
                for &(i, v) in it {
                    if forbidden.get(i as usize).copied().unwrap_or(false) { return Err(i); }
                    let e = m.entry(i).or_insert(0.0);
                    if v > *e { *e = v; }
                }
            }
            let mut v: Vec<(u32, f64)> = m.into_iter().collect();
            v.sort_by_key(|(i, _)| *i);
            supers.push(v);
        }
        let p: Vec<(u32, f64)> = merged(supers.iter().flatten().cloned().collect());
        scatter(&p, dense);
        let mut grads: Vec<(u16, f64, Vec<(u32, f64)>)> = Vec::with_capacity(self.terms.len());
        let mut u_at_i = 0.0;
        for t in &self.terms {
            let mut tp = t.w;
            let mut g: Vec<(u32, f64)> = Vec::new();
            let mut zero = false;
            for f in &t.f {
                let fp = f.a + f.c.iter().map(|&(i, c)| c * dense[i as usize]).sum::<f64>();
                tp *= fp;
                if fp <= 0.0 { zero = true; continue; }
                for &(i, c) in &f.c { g.push((i, c / fp)); }
            }
            u_at_i += tp;
            if zero || tp <= 0.0 { continue; }
            let key = match grouping { Grouping::PerTerm => 0, Grouping::ByTag => t.tag, Grouping::One => 0 };
            grads.push((key, tp, merged(g)));
        }
        gather_clear(&p, dense);
        let grads: Vec<(f64, Vec<(u32, f64)>)> = match grouping {
            Grouping::PerTerm => grads.into_iter().map(|(_, tp, g)| (tp, g)).collect(),
            _ => {
                // Sum values, componentwise-min gradients (absent = 0).
                let mut groups: Vec<(u16, f64, Vec<(u32, f64)>)> = Vec::new();
                for (key, tp, g) in grads {
                    match groups.iter_mut().find(|(k, _, _)| *k == key) {
                        None => groups.push((key, tp, g)),
                        Some((_, sum, gm)) => {
                            *sum += tp;
                            let mut out = Vec::with_capacity(gm.len());
                            let (mut a, mut b) = (0, 0);
                            while a < gm.len() && b < g.len() {
                                match gm[a].0.cmp(&g[b].0) {
                                    std::cmp::Ordering::Less => a += 1,
                                    std::cmp::Ordering::Greater => b += 1,
                                    std::cmp::Ordering::Equal => {
                                        out.push((gm[a].0, gm[a].1.min(g[b].1)));
                                        a += 1; b += 1;
                                    }
                                }
                            }
                            *gm = out;
                        }
                    }
                }
                groups.into_iter().map(|(_, tp, g)| (tp, g)).collect()
            }
        };
        let mut total = 0.0;
        for (tp, g) in &grads {
            scatter(g, dense);
            let mut expo = 0.0;
            for (s, items) in slots.iter().enumerate() {
                let at_super: f64 = supers[s].iter().map(|&(i, v)| v * dense[i as usize]).sum();
                let mut best = 0.0f64; // the none-item (x = 0) is never better than any item here
                let mut first = true;
                for it in items.iter() {
                    let v: f64 = it.iter().map(|&(i, x)| x * dense[i as usize]).sum();
                    if first || v > best { best = v; first = false; }
                }
                expo += best - at_super;
            }
            gather_clear(g, dense);
            total += tp * expo.exp();
        }
        Ok((total, u_at_i))
    }
}

fn scatter(x: &[(u32, f64)], dense: &mut Vec<f64>) {
    for &(i, v) in x {
        let iu = i as usize;
        if iu >= dense.len() { dense.resize(iu + 1, 0.0); }
        dense[iu] += v;
    }
}

fn gather_clear(x: &[(u32, f64)], dense: &mut [f64]) {
    for &(i, _) in x { dense[i as usize] = 0.0; }
}
