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

// ── Fast path: the grouped tangent at the ceiling's own point ───────────────
//
// The live prune runs where the tail ceiling has just been evaluated, so
// its assembled scratch (prefix + the table deltas P, crit floor applied)
// is reused instead of filling and assembling the prefix again. The
// tangent point is P itself: every completion's deltas x satisfy
// 0 <= x <= P componentwise (P sums each slot's per-stat maxima and the set
// transitions), which is all the grouping needs (x - P <= 0, gradients
// >= 0). Each factor is still anchored at x = 0 for its clamps and gates:
// F(0) = F(P) - c . P. Terms are grouped by tag with the componentwise
// minimum gradient; nothing is allocated per node.

/// Per-run tables for `fast_bound`.
pub struct TanPlan {
    /// Dense stat index -> coordinate (or -1): every index a factor reads,
    /// plus the inputs of var effects writing such an index.
    u_of: Vec<i32>,
    u_idx: Vec<u32>,
    /// Per depth, per offset: the item's clamped deltas on the coordinates.
    slot_rows: Vec<Vec<Vec<(u16, f64)>>>,
    /// Per depth: first offset whose item has a positive delta on a
    /// forbidden index (usize::MAX when none).
    slot_forbid_from: Vec<usize>,
    chains: Vec<ChainPlan>,
    /// Coordinate of critDamPct when it is a var output (refused then).
    crit_is_var_output: bool,
}

struct ChainPlan {
    eff: usize,
    /// Output coordinates the envelope reads.
    outs: Vec<u16>,
    /// Captured inputs: (const-term slot, coordinate, factor), every sign.
    inputs: Vec<(usize, u16, f64)>,
}

impl TanPlan {
    pub fn new(d: &DenseCtx, item_vecs: &[Vec<Vec<(u32, f64)>>], rows: &[Row]) -> Option<TanPlan> {
        if !matches!(d.obj, DObjective::Damage) { return None; }
        let dd = d.direct.as_ref()?;
        let reads = read_indices(d);
        let forbidden = forbidden_indices(d, rows);
        let mut u_of = vec![-1i32; d.n];
        let mut u_idx: Vec<u32> = Vec::new();
        let mut coord = |i: u32, u_of: &mut Vec<i32>, u_idx: &mut Vec<u32>| -> u16 {
            if u_of[i as usize] < 0 { u_of[i as usize] = u_idx.len() as i32; u_idx.push(i); }
            u_of[i as usize] as u16
        };
        for (i, &r) in reads.iter().enumerate() { if r { coord(i as u32, &mut u_of, &mut u_idx); } }
        let mut chains = Vec::new();
        for (e, eff) in d.var_effects.iter().enumerate() {
            let outs: Vec<u16> = eff.out_slots.iter()
                .map(|(slot, _)| d.var_slots[*slot])
                .filter(|&o| reads.get(o as usize).copied().unwrap_or(false))
                .map(|o| u_of[o as usize] as u16).collect();
            if outs.is_empty() { continue; }
            let mut inputs = Vec::new();
            for t in &eff.terms {
                if let DTerm::Const(slot, f) = t {
                    let Some(&i) = dd.term_capture.get(*slot) else { continue };
                    inputs.push((*slot, coord(i, &mut u_of, &mut u_idx), *f));
                }
            }
            if inputs.is_empty() { continue; }
            chains.push(ChainPlan { eff: e, outs, inputs });
        }
        let crit_u = u_of[d.s_crit_dam_pct as usize];
        let crit_is_var_output = crit_u >= 0 && chains.iter().any(|c| c.outs.contains(&(crit_u as u16)));
        let mut slot_rows = Vec::with_capacity(item_vecs.len());
        let mut slot_forbid_from = Vec::with_capacity(item_vecs.len());
        for items in item_vecs {
            let mut rows_out = Vec::with_capacity(items.len());
            let mut forbid_from = usize::MAX;
            for (o, it) in items.iter().enumerate() {
                let mut row = Vec::new();
                for &(i, v) in it {
                    if forbidden.get(i as usize).copied().unwrap_or(false) && v > 0.0 && forbid_from == usize::MAX {
                        forbid_from = o;
                    }
                    let u = u_of.get(i as usize).copied().unwrap_or(-1);
                    if u >= 0 { row.push((u as u16, v)); }
                }
                rows_out.push(row);
            }
            slot_rows.push(rows_out);
            slot_forbid_from.push(forbid_from);
        }
        Some(TanPlan { u_of, u_idx, slot_rows, slot_forbid_from, chains, crit_is_var_output })
    }

    pub fn n_coords(&self) -> usize { self.u_idx.len() }
}

/// Reused buffers for `fast_bound`.
#[derive(Default)]
pub struct TanWork {
    p: Vec<f64>,
    lift: Vec<f64>,
    chain_c: Vec<Vec<(u16, f64)>>,
    /// Per tag: summed value at P and the sparse componentwise-min gradient
    /// (sorted by coordinate; a coordinate missing from a term is 0 there).
    gsum: Vec<f64>,
    gsp: Vec<Vec<(u16, f64)>>,
    gtag: Vec<u16>,
    term_g: Vec<(u16, f64)>,
    merge: Vec<(u16, f64)>,
    dense_g: Vec<f64>,
    mult: Vec<f64>,
    journal: Vec<DenseUndo>,
    kbuf: Vec<(u16, f64)>,
    bbuf: Vec<(u16, f64)>,
    dbuf: Vec<(u16, f64)>,
    rbuf: Vec<(u16, f64)>,
    sbuf: Vec<(u16, f64)>,
}

/// A factor at P: value, coefficients, and its value at x = 0.
struct Fac<'a> { at_p: f64, c: &'a [(u16, f64)] }

/// The grouped tangent bound for completions within `slots` (per relaxed
/// depth, the highest offset h), given the scratch assembled at P = prefix
/// + `delta` (crit floor applied) and the leaf at P. None when a gate
/// refuses.
#[allow(clippy::too_many_arguments)]
pub fn fast_bound(
    plan: &TanPlan, w: &mut TanWork, d: &DenseCtx, s: &mut DScratch, leaf: &DenseLeaf,
    rows: &[Row], compiled: &[CompiledRow], tables: &Tables, sp: &[f64; 5],
    delta: &[(u32, f64)], slots: &[(usize, usize)],
) -> Option<f64> {
    if plan.crit_is_var_output { return None; }
    for &(j, h) in slots {
        if h >= plan.slot_forbid_from[j] { return None; }
    }
    let n_u = plan.u_idx.len();
    // P on the coordinates.
    w.p.clear(); w.p.resize(n_u, 0.0);
    for &(i, v) in delta {
        let u = plan.u_of.get(i as usize).copied().unwrap_or(-1);
        if u >= 0 { w.p[u as usize] += v; }
    }
    // Var chains at P: lift each read output from out(P) to the affine
    // bound u(P), and collect the input coefficients per output coordinate.
    w.lift.clear(); w.lift.resize(n_u, 0.0);
    if w.chain_c.len() < n_u { w.chain_c.resize_with(n_u, Vec::new); }
    for c in w.chain_c.iter_mut() { c.clear(); }
    if !plan.chains.is_empty() {
        let mut skp_pre = [0.0f64; 5];
        for i in 0..5 {
            let mut v = sp[i];
            for a in &d.skp_atree_adds[i] { v += a; }
            skp_pre[i] = v;
        }
        for ch in &plan.chains {
            let eff = &d.var_effects[ch.eff];
            let mut t = eff.const_add;
            for term in &eff.terms {
                t += match term {
                    DTerm::Skp(i, f) => skp_pre[*i] * f,
                    DTerm::Const(slot, f) => leaf.const_term_vals[*slot] * f,
                };
            }
            let mut o = t;
            if eff.round { o = crate::scoring::round_near(o).floor(); }
            if eff.positive && o < 0.0 { o = 0.0; }
            if let Some(mx) = eff.max {
                if mx > 0.0 && o > mx { o = mx; }
                if mx < 0.0 && o < mx { o = mx; }
            }
            let margin = if eff.round { 1e-6 } else { 0.0 };
            // t at x = 0, and the affine bound u(x) = t0 + margin + sum_{f>0} f x.
            let mut t0 = t;
            let mut pos_at_p = 0.0;
            for &(_, u, f) in &ch.inputs {
                t0 -= f * w.p[u as usize];
                if f > 0.0 { pos_at_p += f * w.p[u as usize]; }
            }
            let u0 = t0 + margin;
            if !u0.is_finite() || !o.is_finite() { return None; }
            if eff.positive && u0 < 0.0 { return None; }
            if let Some(mx) = eff.max { if mx < 0.0 && u0 < mx { return None; } }
            let u_p = u0 + pos_at_p;
            for &ou in &ch.outs {
                w.lift[ou as usize] += u_p - o;
                for &(_, iu, f) in &ch.inputs {
                    if f > 0.0 { w.chain_c[ou as usize].push((iu, f)); }
                }
            }
        }
    }

    if !matches!(d.obj, DObjective::Damage) { return None; }
    let dex = s.num(d.dex_idx);
    let crit = tables.sp_to_pct(if dex.is_nan() || dex == 0.0 { 0.0 } else { dex });
    if !ok(crit) { return None; }
    // Groups: 2 specs x (6 elements x 2 + raw) tags.
    const N_TAGS: usize = 32;
    w.gsum.clear(); w.gsum.resize(N_TAGS, 0.0);
    if w.gsp.len() < N_TAGS { w.gsp.resize_with(N_TAGS, Vec::new); }
    w.gtag.clear();
    w.dense_g.clear(); w.dense_g.resize(n_u, 0.0);
    let no_extra = DenseRowExtra::default();

    for ((row, comp), drow) in rows.iter().zip(compiled).zip(&d.rows) {
        if comp.mod_spell.is_none() { continue; }
        if row.qty <= 0.0 || row.pseudo || row.dmg_excl { continue; }
        let Some(plan_s) = comp.plan.as_ref() else { continue };
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
        // The melee rate at P's tier: no completion's tier is higher.
        let eff_qty = if row.is_melee_time {
            let period = match row.melee_cd_override {
                Some(p) => p,
                None => {
                    let tier = s.num_or0(d.atk_tier_idx);
                    let adj = (leaf.atk_spd_idx as f64 + tier).clamp(0.0, 6.0);
                    1.0 / tables.base_damage_multiplier[adj as usize]
                }
            };
            row.qty / period.max(SPELL_CAST_DELAY)
        } else { row.qty };
        let n = plan_s.parts.len();
        w.mult.clear(); w.mult.resize(n, 0.0);
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
        let display = if eff_dps { plan_s.dps_display_idx } else { plan_s.display_idx };
        if let Some(i) = display {
            if plan_s.parts[i].static_kind == Some("damage") {
                let m = eff_qty * if eff_dps { eff_dps_hits } else { 1.0 };
                if !ok(m) || !add(i, m, plan_s, &mut w.mult) { return None; }
            }
        }
        if has_final_root {
            for &i in &plan_s.flat_idxs {
                if !add(i, 1.0, plan_s, &mut w.mult) { return None; }
            }
        }
        if w.mult.iter().all(|&m| m == 0.0) { continue; }
        for &(i, _, use_max) in &drow.stat_ops {
            if use_max && plan.u_of.get(i as usize).copied().unwrap_or(-1) >= 0 { return None; }
        }
        dense_apply_row(s, drow, &no_extra, &mut w.journal);
        let mut res = Some(());
        for j in 0..n {
            let m = w.mult[j];
            if m == 0.0 { continue; }
            let PartKindPlan::Damage(dp) = &plan_s.parts[j].kind else { continue };
            let Some(conv_idx) = drow.parts_conv[j].as_ref() else { res = None; break };
            if fast_part(plan, w, d, s, &dp.multipliers, plan_s.use_spell, !plan_s.use_speed,
                         Some(&dp.part_id), !dp.use_str, &dp.ignored_mults, conv_idx, tables,
                         crit, m).is_none() {
                res = None;
                break;
            }
        }
        dense_undo_row(s, &mut w.journal);
        res?;
    }

    // Per group: sum_s max_i g . x_i - g . P.
    let mut total = 0.0;
    for &tag in &w.gtag {
        for &(u, v) in &w.gsp[tag as usize] { w.dense_g[u as usize] = v; }
        let g = &w.dense_g;
        let mut expo = -w.gsp[tag as usize].iter().map(|&(u, v)| v * w.p[u as usize]).sum::<f64>();
        for &(j, h) in slots {
            let rows_j = &plan.slot_rows[j];
            if rows_j.is_empty() { continue; }
            let mut best = f64::NEG_INFINITY;
            for r in &rows_j[..=h.min(rows_j.len() - 1)] {
                let v: f64 = r.iter().map(|&(u, x)| g[u as usize] * x).sum();
                if v > best { best = v; }
            }
            expo += best;
        }
        for &(u, _) in &w.gsp[tag as usize] { w.dense_g[u as usize] = 0.0; }
        // expo <= 0 by construction (x <= P, g >= 0); no clamp, so an
        // unexpected positive value loosens the bound instead of breaking it.
        total += w.gsum[tag as usize] * expo.exp();
    }
    Some(total)
}

/// Folds one term (weight, factors at P) into its tag group: the value
/// adds, the gradient takes the componentwise minimum (sparse).
fn fold_term(
    gsum: &mut [f64], gsp: &mut [Vec<(u16, f64)>], gtag: &mut Vec<u16>,
    term_g: &mut Vec<(u16, f64)>, merge: &mut Vec<(u16, f64)>,
    tag: u16, weight: f64, facs: &[Fac],
) {
    let mut tp = weight;
    for f in facs { tp *= f.at_p; }
    if !(tp > 0.0) { return; }
    term_g.clear();
    for f in facs {
        if f.at_p <= 0.0 { return; }
        for &(u, c) in f.c { term_g.push((u, c / f.at_p)); }
    }
    term_g.sort_unstable_by_key(|x| x.0);
    // Merge duplicate coordinates (sum).
    let mut k = 0;
    for i in 0..term_g.len() {
        if k > 0 && term_g[k - 1].0 == term_g[i].0 { term_g[k - 1].1 += term_g[i].1; }
        else { term_g[k] = term_g[i]; k += 1; }
    }
    term_g.truncate(k);
    let t = tag as usize;
    if gsum[t] == 0.0 {
        gtag.push(tag);
        gsp[t].clear();
        gsp[t].extend_from_slice(term_g);
    } else {
        // Intersection with min: a coordinate absent on either side is 0.
        merge.clear();
        let (g, h) = (&gsp[t], &*term_g);
        let (mut a, mut b) = (0, 0);
        while a < g.len() && b < h.len() {
            match g[a].0.cmp(&h[b].0) {
                std::cmp::Ordering::Less => a += 1,
                std::cmp::Ordering::Greater => b += 1,
                std::cmp::Ordering::Equal => { merge.push((g[a].0, g[a].1.min(h[b].1))); a += 1; b += 1; }
            }
        }
        std::mem::swap(&mut gsp[t], merge);
    }
    gsum[t] += tp;
}

#[allow(clippy::too_many_arguments)]
fn fast_part(
    plan: &TanPlan, w: &mut TanWork, d: &DenseCtx, s: &DScratch, mults: &[f64], use_spell: bool,
    ignore_speed: bool, part_filter: Option<&str>, ignore_str: bool, ignored: &[String],
    conv_idx: &[u32; 6], tables: &Tables, crit: f64, weight: f64,
) -> Option<()> {
    let uo = |i: u32| -> Option<u16> {
        let u = plan.u_of.get(i as usize).copied().unwrap_or(-1);
        (u >= 0).then_some(u as u16)
    };
    // Builds a factor's coefficient list (with chain expansion) into `buf`
    // and returns (value at P lifted by the chains, value at x = 0).
    let factor = |coefs: &[(u32, f64)], at_p_raw: f64, lift: &[f64], chain_c: &[Vec<(u16, f64)>],
                  p: &[f64], buf: &mut Vec<(u16, f64)>| -> (f64, f64) {
        buf.clear();
        let mut at_p = at_p_raw;
        for &(i, c) in coefs {
            let Some(u) = uo(i) else { continue };
            buf.push((u, c));
            at_p += c * lift[u as usize];
            for &(iu, f) in &chain_c[u as usize] { buf.push((iu, c * f)); }
        }
        let cp: f64 = buf.iter().map(|&(u, c)| c * p[u as usize]).sum();
        (at_p, at_p - cp)
    };
    let TanWork { p, lift, chain_c, gsum, gsp, gtag, term_g, merge, kbuf, bbuf, dbuf, rbuf, sbuf, .. } = w;
    let (p, lift, chain_c) = (&p[..], &lift[..], &chain_c[..]);

    // Conversions and base damages (constant on the box; same arithmetic as
    // dense_spell_damage).
    let mut present = d.w_present;
    let mut conversions = [0.0f64; 6];
    for i in 0..mults.len().min(6) { conversions[i] = mults[i]; }
    for i in 0..6 { let ci = conv_idx[i]; if s.has(ci) { conversions[i] += s.num(ci); } }
    for i in 0..6 { let ci = d.conv_base_idx[i]; if s.has(ci) { conversions[i] += s.num(ci); } }
    let neutral_convert = conversions[0] / 100.0;
    if neutral_convert == 0.0 { present = [false; 6]; }
    let mut base = [[0.0f64; 2]; 6];
    let (mut weapon_min, mut weapon_max) = (0.0, 0.0);
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
        if let Some(sm) = &e.spell_match { if Some(&**sm) != part_filter { continue; } }
        if ignored.iter().any(|m| m.as_str() == &*e.key) { continue; }
        match e.target {
            MultTarget::All => damage_mult *= 1.0 + v / 100.0,
            MultTarget::MeleeOnly => { if !use_spell { damage_mult *= 1.0 + v / 100.0; } }
            MultTarget::Ele(i) => em[i] *= 1.0 + v / 100.0,
            MultTarget::Inert => {}
        }
    }
    if !ok(damage_mult) || !ok(str_boost) || !ok(total_convert) || !em.iter().all(|&m| ok(m)) {
        return None;
    }
    let spec_tag: u16 = if use_spell { 16 } else { 0 };

    // K = str + crit * (1 + critDamPct/100), anchored at x = 0 with the
    // -100 floor (see CRIT_CEILING_FLOOR).
    kbuf.clear();
    let k_at_p = if ignore_str { 1.0 } else {
        let cu = uo(d.s_crit_dam_pct);
        let pc = cu.map(|u| p[u as usize]).unwrap_or(0.0);
        let v = s.num(d.s_crit_dam_pct);
        if !v.is_finite() { return None; }
        if let Some(u) = cu { kbuf.push((u, crit / 100.0)); }
        str_boost + crit * (1.0 + ((v - pc).max(crate::scoring::CRIT_CEILING_FLOOR) + pc) / 100.0)
    };
    if !ok(k_at_p) { return None; }

    let static_boost = (s.num(spec_pct_s) + s.num(d.s_dam_pct)) / 100.0;
    let r_pct = (s.num(r_pct_s) + s.num(d.s_r_dam_pct)) / 100.0;
    for i in 0..6 {
        // An element with no damage and no add an item can raise is zero.
        let can_add = present[i];
        if !can_add && base[i][0] == 0.0 && base[i][1] == 0.0 { continue; }
        let mut bc: [(u32, f64); 6] = [(spec_pct_s, 0.01), (d.s_dam_pct, 0.01),
                                       (spec_pct[i], 0.01), (d.dam_pct_idx[i], 0.01),
                                       (r_pct_s, 0.0), (d.s_r_dam_pct, 0.0)];
        let mut b_raw = 1.0 + skill_boost[i] + static_boost
            + (s.num(spec_pct[i]) + s.num(d.dam_pct_idx[i])) / 100.0;
        if i > 0 { b_raw += r_pct; bc[4].1 = 0.01; bc[5].1 = 0.01; }
        if !b_raw.is_finite() { return None; }
        let (b_p, b_0) = factor(&bc, b_raw, lift, chain_c, p, bbuf);
        // max(0, a + c.x) <= max(0, a) + c.x, anchored at x = 0.
        let b_at_p = b_p - b_0 + b_0.max(0.0);
        for mm in 0..2 {
            let add_idx = if mm == 0 { d.dam_add_min_idx[i] } else { d.dam_add_max_idx[i] };
            let (d_p, d_0) = if can_add {
                factor(&[(add_idx, 1.0)], base[i][mm] + s.num(add_idx), lift, chain_c, p, dbuf)
            } else {
                dbuf.clear();
                (base[i][mm], base[i][mm])
            };
            if !d_p.is_finite() || !(d_0 >= 0.0) { return None; }
            let wt = weight * 0.5 * damage_mult * em[i];
            if wt > 0.0 {
                let facs = [Fac { at_p: k_at_p, c: kbuf }, Fac { at_p: d_p, c: dbuf },
                            Fac { at_p: b_at_p, c: bbuf }];
                fold_term(gsum, gsp, gtag, term_g, merge, spec_tag + (i * 2 + mm) as u16, wt, &facs);
            }
        }
    }

    // Raw: each summand at its positive part at x = 0.
    let e0 = em.iter().cloned().fold(0.0f64, f64::max);
    let e1 = em[1..].iter().cloned().fold(0.0f64, f64::max);
    let tc = total_convert;
    rbuf.clear();
    let mut r_at_p = 0.0;
    let mut summand = |coefs: &[(u32, f64)], at_p_raw: f64, scale: f64,
                       rbuf: &mut Vec<(u16, f64)>, sbuf: &mut Vec<(u16, f64)>| -> Option<f64> {
        if scale == 0.0 { return Some(0.0); }
        if !at_p_raw.is_finite() { return None; }
        let (vp, z) = factor(coefs, at_p_raw, lift, chain_c, p, sbuf);
        for &(u, c) in sbuf.iter() { rbuf.push((u, c * scale)); }
        Some(scale * (vp - z + z.max(0.0)))
    };
    r_at_p += summand(&[(spec_raw_s, 1.0), (d.s_dam_raw, 1.0)],
                      s.num(spec_raw_s) + s.num(d.s_dam_raw), tc * e0, rbuf, sbuf)?;
    r_at_p += summand(&[(r_raw_s, 1.0), (d.s_r_dam_raw, 1.0)],
                      s.num(r_raw_s) + s.num(d.s_r_dam_raw), tc * e1, rbuf, sbuf)?;
    for i in 0..6 {
        if !present[i] { continue; }
        r_at_p += summand(&[(spec_raw[i], 1.0), (d.dam_raw_idx[i], 1.0)],
                          s.num(spec_raw[i]) + s.num(d.dam_raw_idx[i]), tc * em[i], rbuf, sbuf)?;
    }
    let wt = weight * damage_mult;
    if wt > 0.0 && r_at_p > 0.0 {
        let facs = [Fac { at_p: k_at_p, c: kbuf }, Fac { at_p: r_at_p, c: rbuf }];
        fold_term(gsum, gsp, gtag, term_g, merge, spec_tag + 12, wt, &facs);
    }
    Some(())
}
