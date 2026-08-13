//! Port of `simulate_combo_mana_hp` (js/solver/pure/simulate.js:353-824) —
//! the stateful mana + HP simulation.
//!
//! This is the path the loop-free fast sim (`scoring::simulate_mana_fast_ff`)
//! deliberately does not cover: generic buff states with durations, mana-regen
//! suppression and continuous/one-shot drain, exit triggers, Blood Pact
//! health-cost casting, and loop brackets (both `count` and `until-OOM`).
//!
//! Every arithmetic step mirrors the JS line-for-line, including the places
//! where JS produces `NaN` from a missing field — see `js_min` uses below.
//! Bit-exactness is checked by `bin/mana_sim_check.rs` against results dumped
//! from the JS implementation (`fixtures/mana_sim_cases.json`).

use serde_json::Value;

use crate::scoring::{
    compute_melee_time_hits, js_max, js_min, js_round, raw_to_pct, row_unclamped_spell_cost,
    Obj, Row, StatsView, Tables, Token,
};

/// `LOOP_COND_COUNT` (js/solver/constants.js).
pub const LOOP_COND_COUNT: i64 = 0;
/// `LOOP_COND_UNTIL_OOM` (js/solver/constants.js).
pub const LOOP_COND_UNTIL_OOM: i64 = 1;
/// `LOOP_SAFETY_CAP` (js/solver/constants.js).
pub const LOOP_SAFETY_CAP: i64 = 255;

/// `HIDDEN_BASE_HPR` (js/game/game_rules.js).
pub const HIDDEN_BASE_HPR: f64 = 3.0;
/// `HPR_TICK_SECONDS` (js/game/game_rules.js).
pub const HPR_TICK_SECONDS: f64 = 4.0;

// ── health_config ───────────────────────────────────────────────────────────

/// `health_config.damage_boost` — the Blood Pact bonus range.
#[derive(Clone, Debug)]
pub struct DamageBoost {
    pub min: f64,
    pub max: f64,
    /// Non-null makes this a *dynamic* slider: the sim's `blood_pact_bonus`
    /// is injected into the damage rows under this name.
    pub slider_name: Option<String>,
}

/// `buff_state.drain_pct_per_second`. Missing keys read as 0, matching the
/// JS `drain.mana > 0` / `drain.hp > 0` probes.
#[derive(Clone, Debug, Default)]
pub struct DrainRates {
    pub mana: f64,
    pub hp: f64,
}

/// One entry of `health_config.buff_states`.
#[derive(Clone, Debug)]
pub struct BuffState {
    pub state_name: String,
    /// `activate_on.spell` — the `base_spell` id that turns the state on.
    pub activate_on_spell: Option<i64>,
    /// `deactivate === 'next_action'`.
    pub deactivate_next_action: bool,
    pub suppress_healing: bool,
    pub suppress_mana_regen: bool,
    pub drain_pct_per_second: Option<DrainRates>,
    pub compute_delay: bool,
    pub apply_to_next: bool,
    pub duration: Option<f64>,
    pub value_cap: Option<f64>,
    pub spell_rate_field: Option<String>,
    pub spell_flat_field: Option<String>,
    /// `'mana_loss'` | `'hp_loss_pct'` | absent.
    pub tracking: Option<String>,
    /// Non-null makes this state a *dynamic* slider (see `DamageBoost`).
    pub slider_name: Option<String>,
}

/// One entry of `health_config.exit_triggers`.
#[derive(Clone, Debug)]
pub struct ExitTrigger {
    pub state: String,
    /// Always `'exit'` today; kept so the guard mirrors the JS.
    pub on: String,
    /// `'heal_pct_of_state'` | `'heal_per_hit'`.
    pub effect: String,
    pub value: f64,
    pub slider_name: Option<String>,
    pub cooldown: f64,
}

/// `extract_health_config()`'s output, in the shape the sim consumes.
#[derive(Clone, Debug, Default)]
pub struct HealthConfig {
    pub hp_casting: bool,
    pub health_cost: f64,
    pub damage_boost: Option<DamageBoost>,
    pub buff_states: Vec<BuffState>,
    pub exit_triggers: Vec<ExitTrigger>,
}

impl HealthConfig {
    /// Parses the JSON emitted by `extract_health_config()`.
    pub fn parse(v: &Value) -> HealthConfig {
        let buff_states = v
            .get("buff_states")
            .and_then(|b| b.as_array())
            .map(|a| a.iter().map(BuffState::parse).collect())
            .unwrap_or_default();
        let exit_triggers = v
            .get("exit_triggers")
            .and_then(|b| b.as_array())
            .map(|a| a.iter().map(ExitTrigger::parse).collect())
            .unwrap_or_default();
        HealthConfig {
            hp_casting: v.get("hp_casting").and_then(|x| x.as_bool()).unwrap_or(false),
            health_cost: v.get("health_cost").and_then(|x| x.as_f64()).unwrap_or(0.0),
            damage_boost: v.get("damage_boost").filter(|d| !d.is_null()).map(|d| DamageBoost {
                min: d.get("min").and_then(|x| x.as_f64()).unwrap_or(0.0),
                max: d.get("max").and_then(|x| x.as_f64()).unwrap_or(0.0),
                slider_name: d.get("slider_name").and_then(|x| x.as_str()).map(String::from),
            }),
            buff_states,
            exit_triggers,
        }
    }
}

impl BuffState {
    fn parse(v: &Value) -> BuffState {
        BuffState {
            state_name: v.get("state_name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            activate_on_spell: v
                .get("activate_on")
                .filter(|a| !a.is_null())
                .and_then(|a| a.get("spell"))
                .and_then(|s| s.as_i64()),
            deactivate_next_action: v.get("deactivate").and_then(|x| x.as_str())
                == Some("next_action"),
            suppress_healing: v.get("suppress_healing").and_then(|x| x.as_bool()).unwrap_or(false),
            suppress_mana_regen: v
                .get("suppress_mana_regen")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            drain_pct_per_second: v
                .get("drain_pct_per_second")
                .filter(|d| !d.is_null())
                .map(|d| DrainRates {
                    mana: d.get("mana").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    hp: d.get("hp").and_then(|x| x.as_f64()).unwrap_or(0.0),
                }),
            compute_delay: v.get("compute_delay").and_then(|x| x.as_bool()).unwrap_or(false),
            apply_to_next: v.get("apply_to_next").and_then(|x| x.as_bool()).unwrap_or(false),
            duration: v.get("duration").and_then(|x| x.as_f64()),
            value_cap: v.get("value_cap").and_then(|x| x.as_f64()),
            spell_rate_field: v
                .get("spell_rate_field")
                .and_then(|x| x.as_str())
                .map(String::from),
            spell_flat_field: v
                .get("spell_flat_field")
                .and_then(|x| x.as_str())
                .map(String::from),
            tracking: v.get("tracking").and_then(|x| x.as_str()).map(String::from),
            slider_name: v.get("slider_name").and_then(|x| x.as_str()).map(String::from),
        }
    }
}

impl ExitTrigger {
    fn parse(v: &Value) -> ExitTrigger {
        ExitTrigger {
            state: v.get("state").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            on: v.get("on").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            effect: v.get("effect").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            value: v.get("value").and_then(|x| x.as_f64()).unwrap_or(f64::NAN),
            slider_name: v.get("slider_name").and_then(|x| x.as_str()).map(String::from),
            cooldown: v.get("cooldown").and_then(|x| x.as_f64()).unwrap_or(0.0),
        }
    }
}

// ── results ─────────────────────────────────────────────────────────────────

/// One entry of `row_results`. `None` in the JS array is represented by the
/// `filled` flag staying false — rows never reached keep JS's `null`.
#[derive(Clone, Debug, Default)]
pub struct RowResult {
    pub filled: bool,
    pub blood_pact_bonus: f64,
    /// `state_values`, positionally in `buff_states` declaration order.
    ///
    /// Indexed rather than keyed by name: this is rebuilt for every row of
    /// every greedy SP trial, and cloning a state name per row per trial
    /// dominated the dynamic-row path. Callers that need names zip against
    /// `HealthConfig::buff_states`.
    pub state_values: Vec<f64>,
    pub hp_warning: bool,
    pub mana_warning: bool,
    pub computed_delay: Option<f64>,
    pub mana_lost: f64,
    pub mana_gained: f64,
    pub elapsed_time: f64,
    pub row_dt: f64,
    /// Only spell/melee rows carry these (JS omits them on the early-exit
    /// branches); `None` mirrors `undefined`.
    pub cast_time: Option<f64>,
    pub delay: Option<f64>,
}

/// One entry of `spell_costs`.
#[derive(Clone, Debug)]
pub struct SpellCost {
    pub name: String,
    pub qty: i64,
    pub cost: f64,
    pub recast_penalty: f64,
}

/// `simulate_combo_mana_hp`'s return value.
#[derive(Clone, Debug, Default)]
pub struct SimResult {
    pub end_mana: f64,
    pub start_mana: f64,
    pub max_mana: f64,
    pub end_hp: f64,
    pub max_hp: f64,
    pub row_results: Vec<RowResult>,
    pub spell_costs: Vec<SpellCost>,
    pub total_mana_cost: f64,
    pub total_mana_drain: f64,
    pub melee_hits: f64,
    pub recast_penalty_total: f64,
    pub mana_wasted: f64,
    /// `loop_iteration_counts`: LOOP_START row index → iteration count.
    pub loop_iteration_counts: Vec<(usize, i64)>,
}

/// Constants the sim needs, mirroring `js/game/game_rules.js`.
pub struct SimConsts {
    pub base_mana_regen: f64,
    pub mana_tick_seconds: f64,
    pub spell_cast_time: f64,
    pub spell_cast_delay: f64,
    pub hpr_tick_seconds: f64,
    pub hidden_base_hpr: f64,
    /// `skillpoint_final_mult[2]` — the INT cost-reduction multiplier used by
    /// `row_unclamped_spell_cost`.
    pub skillpoint_final_mult_2: f64,
}

impl Default for SimConsts {
    fn default() -> Self {
        SimConsts {
            base_mana_regen: 25.0,
            mana_tick_seconds: 5.0,
            spell_cast_time: 0.3,
            spell_cast_delay: 0.1,
            hpr_tick_seconds: HPR_TICK_SECONDS,
            hidden_base_hpr: HIDDEN_BASE_HPR,
            skillpoint_final_mult_2: f64::NAN,
        }
    }
}

/// Live per-state tracking (`active_states[name]`).
#[derive(Clone, Debug, Default)]
struct StateSlot {
    active: bool,
    value: f64,
    activated_at: f64,
}

/// `compute_drain_override` (simulate.js:280).
pub struct DrainOverride {
    pub computed_delay: f64,
    pub actual_drain: f64,
    pub state_value: f64,
    /// true = mana, false = hp.
    pub is_mana: bool,
}

pub fn compute_drain_override(
    bs: &BuffState, current_mana: f64, max_mana: f64, current_hp: f64, max_hp: f64,
    override_time: Option<f64>,
) -> Option<DrainOverride> {
    if !bs.compute_delay {
        return None;
    }
    let drain = bs.drain_pct_per_second.as_ref()?;

    let (drain_pct, pool_current, pool_max) = if drain.mana > 0.0 {
        (drain.mana, current_mana, max_mana)
    } else if drain.hp > 0.0 {
        (drain.hp, current_hp, max_hp)
    } else {
        (0.0, 0.0, 0.0)
    };
    if drain_pct <= 0.0 || pool_max <= 0.0 {
        return None;
    }

    let drain_rate = drain_pct / 100.0 * pool_max;
    // `Math.min(bs.value_cap ?? Infinity, Math.max(0, pool_current))`
    let target = js_min(bs.value_cap.unwrap_or(f64::INFINITY), js_max(0.0, pool_current));
    let drain_time = match override_time {
        // `Math.min(override_time, bs.duration ?? Infinity)`
        Some(t) => js_min(t, bs.duration.unwrap_or(f64::INFINITY)),
        None => {
            let t = target / drain_rate;
            match bs.duration {
                Some(d) => js_min(t, d),
                None => t,
            }
        }
    };
    let actual_drain = js_min(target, drain_rate * drain_time);
    let state_value = js_min(bs.value_cap.unwrap_or(f64::INFINITY), actual_drain);
    Some(DrainOverride {
        computed_delay: drain_time,
        actual_drain,
        state_value,
        is_mana: drain.mana > 0.0,
    })
}

/// `loop_condition_met` (simulate.js:318) — returns true when the loop stops.
pub fn loop_condition_met(
    cond_type: i64, cond_value: f64, iteration: i64, mana_warning: bool, hp_warning: bool,
) -> bool {
    if iteration >= LOOP_SAFETY_CAP {
        return true;
    }
    match cond_type {
        LOOP_COND_COUNT => iteration as f64 >= cond_value,
        LOOP_COND_UNTIL_OOM => mana_warning || hp_warning,
        // Unknown condition type → don't loop.
        _ => true,
    }
}

fn get_boost_token_value(tokens: &[Token], name: Option<&str>) -> f64 {
    let Some(name) = name else { return 0.0 };
    if name.is_empty() {
        return 0.0;
    }
    for t in tokens {
        if t.name == name {
            return t.value;
        }
    }
    0.0
}

/// `_apply_exit_trigger` (simulate.js:1113) — returns the new HP.
#[allow(clippy::too_many_arguments)]
fn apply_exit_trigger(
    trigger: &ExitTrigger, state_value: f64, max_hp: f64, hp: f64, tokens: &[Token],
    melee_hits: f64, elapsed_time: f64,
) -> f64 {
    match trigger.effect.as_str() {
        "heal_pct_of_state" => js_min(max_hp, hp + state_value * trigger.value / 100.0 * max_hp),
        "heal_per_hit" => {
            let enemies = get_boost_token_value(tokens, trigger.slider_name.as_deref());
            let max_procs = if trigger.cooldown > 0.0 {
                (elapsed_time / trigger.cooldown).floor()
            } else {
                melee_hits
            };
            let procs = js_min(melee_hits, max_procs);
            let missing_ratio = js_max(0.0, (max_hp - hp) / max_hp);
            js_min(max_hp, hp + procs * enemies * trigger.value * missing_ratio * max_hp)
        }
        _ => hp,
    }
}

/// `_snapshot_states`. `row_deact` holds values captured when a state was
/// deactivated during *this* row, positionally aligned with `slots`.
fn snapshot_states(slots: &[StateSlot], row_deact: Option<&Vec<Option<f64>>>) -> Vec<f64> {
    slots
        .iter()
        .enumerate()
        .map(|(i, st)| {
            if st.active {
                st.value
            } else {
                row_deact.and_then(|d| d.get(i).copied().flatten()).unwrap_or(0.0)
            }
        })
        .collect()
}

// ── the simulation ──────────────────────────────────────────────────────────

/// `simulate_combo_mana_hp` (simulate.js:353).
///
/// `rows` must retain their loop markers — unlike the fast path, this sim
/// executes them. `registry` is the boost registry used for per-row spell
/// cost adjustment.
#[allow(clippy::too_many_arguments)]
pub fn simulate_combo_mana_hp(
    rows: &[Row], base_stats: &Obj, hc: &HealthConfig, has_transcendence: bool,
    registry: &[Value], tables: &Tables, consts: &SimConsts,
) -> SimResult {
    let stats = StatsView::Borrowed(base_stats);
    let mr = stats.num_or0("mr");
    let ms = stats.num_or0("ms");
    let item_mana = stats.num_or0("maxMana");
    let int_mana = (tables.sp_to_pct(stats.num_or0("int")) * 100.0).floor();
    let start_mana = 100.0 + item_mana + int_mana;
    let max_mana = start_mana;
    let mut mana_wasted = 0.0f64;

    let base_hp = stats.num_or0("hp");
    let hp_bonus = stats.num_or0("hpBonus");
    let max_hp = js_max(5.0, base_hp + hp_bonus);
    let total_hpr = raw_to_pct(stats.num_or0("hprRaw"), stats.num_or0("hprPct") / 100.0);
    let hpr_tick = total_hpr + consts.hidden_base_hpr;
    let mr_per_sec = (mr + consts.base_mana_regen) / consts.mana_tick_seconds;

    let health_cost_pct = hc.health_cost;

    let mut adj = tables.atk_spd_index(stats.str_of("atkSpd")) as f64 + stats.num_or0("atkTier");
    if adj < 0.0 {
        adj = 0.0;
    }
    if adj > 6.0 {
        adj = 6.0;
    }
    let base_dmg_mult = tables.base_damage_multiplier[adj as usize];
    let ms_per_hit = if ms != 0.0 { ms / 3.0 / base_dmg_mult } else { 0.0 };

    let n_states = hc.buff_states.len();
    let mut slots: Vec<StateSlot> = vec![StateSlot::default(); n_states];
    let mut state_melee_hits: Vec<f64> = vec![0.0; n_states];

    let melee_period = 1.0 / base_dmg_mult;
    let mut melee_cd_remaining = 0.0f64;

    let mut mana = start_mana;
    let mut hp = max_hp;
    let mut elapsed_time = 0.0f64;

    let mut row_results: Vec<RowResult> = vec![RowResult::default(); rows.len()];
    let mut spell_costs: Vec<SpellCost> = Vec::new();
    let mut total_mana_cost = 0.0f64;
    let mut total_mana_drain = 0.0f64;
    let mut melee_hits = 0.0f64;
    let mut recast_penalty_total = 0.0f64;

    let mut loop_body_start: i64 = -1;
    let mut loop_condition: Option<(i64, f64)> = None;
    let mut loop_iteration: i64 = 0;
    let mut loop_had_mana_warn = false;
    let mut loop_had_hp_warn = false;
    let mut loop_iteration_counts: Vec<(usize, i64)> = Vec::new();

    // `_advance_time(advance_dt)` — mutates elapsed_time / mana / hp /
    // mana_wasted / total_mana_drain / row_mana_gained / slots.
    macro_rules! advance_time {
        ($advance_dt:expr, $row_mana_gained:expr) => {{
            let advance_dt: f64 = $advance_dt;
            if advance_dt > 0.0 {
                let prev_time = elapsed_time;
                elapsed_time += advance_dt;

                let mut mana_regen_dt = advance_dt;

                for (bi, bs) in hc.buff_states.iter().enumerate() {
                    if !slots[bi].active {
                        continue;
                    }

                    let mut active_dt = advance_dt;
                    if let Some(duration) = bs.duration {
                        let elapsed_in_state = prev_time - slots[bi].activated_at;
                        let remaining = duration - elapsed_in_state;
                        if remaining <= 0.0 {
                            slots[bi].active = false;
                            continue;
                        }
                        active_dt = js_min(advance_dt, remaining);
                        if active_dt < advance_dt {
                            slots[bi].active = false;
                        }
                    }

                    if bs.suppress_mana_regen {
                        mana_regen_dt = js_min(mana_regen_dt, advance_dt - active_dt);
                    }

                    if !bs.compute_delay {
                        if let Some(drain) = &bs.drain_pct_per_second {
                            let drain_pct = drain.mana;
                            if drain_pct > 0.0 {
                                let d = drain_pct / 100.0 * max_mana * active_dt;
                                let actual = js_min(mana, d);
                                mana -= actual;
                                total_mana_drain += actual;
                                if bs.tracking.as_deref() == Some("mana_loss") {
                                    // JS: `Math.min(st.value + actual, bs.value_cap)` —
                                    // no `??`, so a missing cap yields NaN.
                                    slots[bi].value = js_min(
                                        slots[bi].value + actual,
                                        bs.value_cap.unwrap_or(f64::NAN),
                                    );
                                }
                            }
                        }
                    }
                }

                if mana_regen_dt > 0.0 {
                    let mana_before_regen = mana;
                    let uncapped_mr = mana + mr_per_sec * mana_regen_dt;
                    if uncapped_mr > max_mana {
                        mana_wasted += uncapped_mr - max_mana;
                    }
                    mana = js_min(max_mana, uncapped_mr);
                    $row_mana_gained += mana - mana_before_regen;
                }

                let any_suppress = hc
                    .buff_states
                    .iter()
                    .enumerate()
                    .any(|(bi, bs)| bs.suppress_healing && slots[bi].active);
                if !any_suppress {
                    let prev_ticks = (prev_time / consts.hpr_tick_seconds).floor();
                    let cur_ticks = (elapsed_time / consts.hpr_tick_seconds).floor();
                    if cur_ticks > prev_ticks {
                        hp = js_min(max_hp, hp + hpr_tick * (cur_ticks - prev_ticks));
                    }
                }
            }
        }};
    }

    let mut ri: i64 = 0;
    while (ri as usize) < rows.len() {
        let idx = ri as usize;
        let row = &rows[idx];

        // ── Loop bracket handling ──
        if let Some(cond) = row.loop_start {
            if loop_condition.is_none() {
                loop_body_start = ri + 1;
                loop_condition = Some(cond);
                loop_iteration = 0;
                loop_had_mana_warn = false;
                loop_had_hp_warn = false;
            }
            row_results[idx] = RowResult {
                filled: true,
                state_values: snapshot_states(&slots, None),
                elapsed_time,
                ..Default::default()
            };
            ri += 1;
            continue;
        }

        if row.loop_end {
            if let Some((ct, cv)) = loop_condition {
                loop_iteration += 1;
                let should_stop = loop_condition_met(
                    ct, cv, loop_iteration, loop_had_mana_warn, loop_had_hp_warn,
                );
                if !should_stop {
                    ri = loop_body_start;
                    loop_had_mana_warn = false;
                    loop_had_hp_warn = false;
                    continue;
                }
                loop_iteration_counts.push(((loop_body_start - 1) as usize, loop_iteration));
                loop_condition = None;
            }
            row_results[idx] = RowResult {
                filled: true,
                state_values: snapshot_states(&slots, None),
                elapsed_time,
                ..Default::default()
            };
            ri += 1;
            continue;
        }

        let mana_before = mana;
        let time_before = elapsed_time;
        let tokens = &row.tokens;

        // ── Cancel state pseudo-spell ──
        if let Some(kind) = row.pseudo_kind.as_deref() {
            if let Some(state_name) = kind.strip_prefix("cancel_state:") {
                let bi = hc.buff_states.iter().position(|b| b.state_name == state_name);
                if let Some(bi) = bi {
                    if slots[bi].active && !row.mana_excl {
                        for trigger in &hc.exit_triggers {
                            if trigger.state == state_name && trigger.on == "exit" {
                                hp = apply_exit_trigger(
                                    trigger, slots[bi].value, max_hp, hp, tokens,
                                    state_melee_hits[bi], elapsed_time,
                                );
                            }
                        }
                        slots[bi].active = false;
                        slots[bi].value = 0.0;
                        state_melee_hits[bi] = 0.0;
                    }
                }
                row_results[idx] = RowResult {
                    filled: true,
                    state_values: snapshot_states(&slots, None),
                    mana_lost: js_max(0.0, mana_before - mana),
                    elapsed_time,
                    row_dt: elapsed_time - time_before,
                    ..Default::default()
                };
                ri += 1;
                continue;
            }

            // ── Mana Reset ──
            if kind == "mana_reset" {
                let mana_gained_amt = if row.mana_excl { 0.0 } else { max_mana - mana };
                if !row.mana_excl {
                    mana = max_mana;
                }
                row_results[idx] = RowResult {
                    filled: true,
                    state_values: snapshot_states(&slots, None),
                    mana_lost: js_max(0.0, mana_before - mana),
                    mana_gained: mana_gained_amt,
                    elapsed_time,
                    row_dt: elapsed_time - time_before,
                    ..Default::default()
                };
                ri += 1;
                continue;
            }

            // ── Add Flat Mana ──
            if kind == "add_flat_mana" {
                if !row.mana_excl && row.qty != 0.0 {
                    let uncapped = mana + row.qty;
                    if uncapped > max_mana {
                        mana_wasted += uncapped - max_mana;
                    }
                    mana = js_max(0.0, js_min(max_mana, uncapped));
                }
                row_results[idx] = RowResult {
                    filled: true,
                    state_values: snapshot_states(&slots, None),
                    mana_lost: js_max(0.0, mana_before - mana),
                    elapsed_time,
                    row_dt: elapsed_time - time_before,
                    ..Default::default()
                };
                ri += 1;
                continue;
            }
        }

        // JS: `if (qty <= 0 || !spell)` — a null spell falls here too.
        if row.qty <= 0.0 || row.spell.is_none() {
            row_results[idx] = RowResult {
                filled: true,
                state_values: snapshot_states(&slots, None),
                elapsed_time,
                ..Default::default()
            };
            ri += 1;
            continue;
        }
        let spell = row.spell.as_ref().unwrap();

        // Mana-excluded rows: skip cost/regen tracking entirely.
        if row.mana_excl {
            row_results[idx] = RowResult {
                filled: true,
                state_values: snapshot_states(&slots, None),
                elapsed_time,
                ..Default::default()
            };
            ri += 1;
            continue;
        }

        let is_spell = row.sim_cost_present;
        let unclamped_cost = if is_spell {
            row_unclamped_spell_cost(
                base_stats, spell, tokens, registry, tables, consts.skillpoint_final_mult_2,
            )
        } else {
            0.0
        };
        let cost_per = js_max(1.0, unclamped_cost);
        let is_melee_scaling = row.sim_melee_scaling;
        let is_melee = row.sim_recast_base == 0;

        let eff_cast_time = if is_melee {
            0.0
        } else {
            row.cast_time.unwrap_or(consts.spell_cast_time)
        };
        let eff_delay = row.delay.unwrap_or(consts.spell_cast_delay);

        let sim_qty = if row.is_melee_time {
            // JS passes eff_delay here, not SPELL_CAST_DELAY (the damage
            // path is the one that passes the constant).
            js_round(compute_melee_time_hits(
                row.qty, &stats, row.melee_cd_override, tables, Some(eff_delay),
            ))
        } else {
            js_round(row.qty)
        } as i64;
        if is_melee_scaling {
            melee_hits += sim_qty as f64;
        }

        let mut row_blood_total = 0.0f64;
        let mut row_blood_casts = 0i64;
        let mut hp_warning = false;
        let mut mana_warning = false;
        let mut row_mana_cost = 0.0f64;
        let mut row_mana_gained = 0.0f64;
        let mut row_deact: Option<Vec<Option<f64>>> = None;
        let mut row_computed_delay: Option<f64> = None;

        let eff_melee_period = row.melee_cd_override.unwrap_or(melee_period);
        let spell_hp_cost = row.sim_hp_cost;

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

            advance_time!(pre_dt, row_mana_gained);

            if is_melee_scaling {
                for bi in 0..n_states {
                    if slots[bi].active {
                        state_melee_hits[bi] += 1.0;
                    }
                }
            }

            if is_melee_scaling && ms_per_hit != 0.0 {
                let mana_before_ms = mana;
                let uncapped = mana + ms_per_hit;
                if uncapped > max_mana {
                    mana_wasted += uncapped - max_mana;
                }
                mana = js_max(0.0, js_min(max_mana, uncapped));
                row_mana_gained += mana - mana_before_ms;
            }

            // "next_action" deactivation (Vanish).
            if c == 0 && (is_spell || is_melee_scaling) {
                for (bi, bs) in hc.buff_states.iter().enumerate() {
                    if !bs.deactivate_next_action || !slots[bi].active {
                        continue;
                    }
                    for trigger in &hc.exit_triggers {
                        if trigger.state == bs.state_name && trigger.on == "exit" {
                            hp = apply_exit_trigger(
                                trigger, slots[bi].value, max_hp, hp, tokens,
                                state_melee_hits[bi], elapsed_time,
                            );
                        }
                    }
                    row_deact
                        .get_or_insert_with(|| vec![None; n_states])[bi] = Some(slots[bi].value);
                    slots[bi].active = false;
                    slots[bi].value = 0.0;
                    state_melee_hits[bi] = 0.0;
                }
            }

            // Spell-level HP cost & state tracking.
            for (bi, bs) in hc.buff_states.iter().enumerate() {
                if !slots[bi].active {
                    continue;
                }

                if let Some(field) = &bs.spell_rate_field {
                    let rate = spell.get(field.as_str()).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    if rate > 0.0 {
                        slots[bi].value = js_min(
                            bs.value_cap.unwrap_or(100.0),
                            slots[bi].value + rate / base_dmg_mult,
                        );
                    }
                }
                if let Some(field) = &bs.spell_flat_field {
                    let flat = spell.get(field.as_str()).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    if flat > 0.0 {
                        slots[bi].value =
                            js_min(bs.value_cap.unwrap_or(100.0), slots[bi].value + flat);
                    }
                }

                if spell_hp_cost > 0.0 {
                    let hp_deduction = spell_hp_cost / 100.0 * max_hp;
                    if hp < hp_deduction {
                        hp_warning = true;
                    }
                    hp -= hp_deduction;
                    if bs.tracking.as_deref() == Some("hp_loss_pct") {
                        slots[bi].value =
                            js_min(bs.value_cap.unwrap_or(100.0), slots[bi].value + spell_hp_cost);
                    }
                    if let Some(db) = &hc.damage_boost {
                        row_blood_total += db.max;
                        row_blood_casts += 1;
                    }
                }
            }

            // Spell cost payment.
            if is_spell {
                let penalty = row.recast_penalties.get(c as usize).copied().unwrap_or(0.0);
                let effective_cost = js_max(1.0, unclamped_cost + penalty);
                recast_penalty_total += effective_cost - cost_per;
                let adj_cost = if has_transcendence { effective_cost * 0.75 } else { effective_cost };

                if mana >= effective_cost {
                    mana -= adj_cost;
                } else if health_cost_pct > 0.0 {
                    // Blood Pact: pay the remainder from health.
                    let remaining_mana = js_max(0.0, mana);
                    let health_mana = adj_cost - remaining_mana;
                    mana = 0.0;
                    let blood_ratio = health_mana / adj_cost;

                    let hp_cost = health_mana * health_cost_pct * max_hp / 100.0;
                    if hp < hp_cost {
                        hp_warning = true;
                    }
                    hp -= hp_cost;

                    for (bi, bs) in hc.buff_states.iter().enumerate() {
                        if slots[bi].active && bs.tracking.as_deref() == Some("hp_loss_pct") {
                            slots[bi].value =
                                js_min(100.0, slots[bi].value + hp_cost / max_hp * 100.0);
                        }
                    }

                    if let Some(db) = &hc.damage_boost {
                        row_blood_total += db.min + (db.max - db.min) * blood_ratio;
                        row_blood_casts += 1;
                    }
                } else {
                    mana -= effective_cost;
                    mana_warning = true;
                }

                row_mana_cost += adj_cost;

                // State activation.
                for (bi, bs) in hc.buff_states.iter().enumerate() {
                    let Some(activate_spell) = bs.activate_on_spell else { continue };
                    let base_spell =
                        spell.get("base_spell").and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
                    if base_spell != activate_spell || slots[bi].active {
                        continue;
                    }
                    slots[bi].active = true;
                    slots[bi].value = 0.0;
                    slots[bi].activated_at = elapsed_time;

                    let dro = compute_drain_override(
                        bs, mana, max_mana, hp, max_hp,
                        if row.auto_delay { None } else { Some(post_dt) },
                    );
                    if let Some(dro) = &dro {
                        if row.auto_delay && row_computed_delay.is_none() {
                            row_computed_delay = Some(dro.computed_delay);
                        }
                        if dro.is_mana {
                            mana -= dro.actual_drain;
                            total_mana_drain += dro.actual_drain;
                        } else {
                            hp -= dro.actual_drain;
                        }
                        slots[bi].activated_at += dro.computed_delay;
                    }

                    if bs.apply_to_next {
                        slots[bi].value = match &dro {
                            Some(d) => d.state_value,
                            None => bs.value_cap.unwrap_or(0.0),
                        };
                    }
                }
            }

            // Post-action time (overridden delay on the activating cast).
            let mut effective_post_dt = post_dt;
            if let Some(rcd) = row_computed_delay {
                if c == 0 {
                    effective_post_dt = rcd;
                    melee_cd_remaining =
                        js_max(0.0, melee_cd_remaining - (effective_post_dt - post_dt));
                }
            }
            advance_time!(effective_post_dt, row_mana_gained);
        }

        let avg_blood_bonus = if row_blood_casts > 0 {
            row_blood_total / row_blood_casts as f64
        } else {
            0.0
        };
        total_mana_cost += row_mana_cost;
        if is_spell {
            // JS: `(recast_penalties ?? []).reduce(..., 0)` — the seed is
            // +0. Rust's `Sum for f64` folds from -0.0, which would make an
            // empty row report -0.0 instead.
            let row_recast: f64 = row
                .recast_penalties
                .iter()
                .fold(0.0f64, |sum, p| sum + (js_max(1.0, unclamped_cost + p) - cost_per));
            spell_costs.push(SpellCost {
                name: spell.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                qty: sim_qty,
                cost: cost_per,
                recast_penalty: row_recast,
            });
        }

        if hp_warning {
            loop_had_hp_warn = true;
        }
        if mana_warning {
            loop_had_mana_warn = true;
        }

        row_results[idx] = RowResult {
            filled: true,
            blood_pact_bonus: avg_blood_bonus,
            state_values: snapshot_states(&slots, row_deact.as_ref()),
            hp_warning,
            mana_warning,
            computed_delay: row_computed_delay,
            mana_lost: js_max(0.0, mana_before - mana),
            mana_gained: row_mana_gained,
            elapsed_time,
            row_dt: elapsed_time - time_before,
            cast_time: Some(eff_cast_time),
            delay: Some(eff_delay),
        };
        ri += 1;
    }

    SimResult {
        end_mana: mana,
        start_mana,
        max_mana,
        end_hp: hp,
        max_hp,
        row_results,
        spell_costs,
        total_mana_cost,
        total_mana_drain,
        melee_hits,
        recast_penalty_total,
        mana_wasted,
        loop_iteration_counts,
    }
}

// ── connective functions between the sim and scoring ────────────────────────

/// `RECAST_MANA_PENALTY` (js/solver/constants.js).
pub const RECAST_MANA_PENALTY: f64 = 5.0;

/// `compute_recast_penalties` (simulate.js:193) — fills each row's
/// `recast_penalties` / `recast_penalty_per_cast` in place.
///
/// Reads `sim_qty` (not `qty`): casts are discrete, and the caller has
/// already resolved melee-time rows into a hit count. Must be re-run after
/// a dynamic loop unroll, because the flat sequence changes which casts are
/// consecutive.
pub fn compute_recast_penalties(rows: &mut [Row]) {
    let penalty_per = RECAST_MANA_PENALTY;
    let mut last_base: Option<i64> = None;
    let mut consec: f64 = 0.0;
    let mut penalty: f64 = 0.0;

    for row in rows.iter_mut() {
        if row.loop_start.is_some() || row.loop_end {
            continue;
        }
        if row.pseudo {
            if row.pseudo_kind.as_deref() == Some("mana_reset") && !row.mana_excl {
                last_base = None;
                consec = 0.0;
                penalty = 0.0;
            }
            continue;
        }

        let sim_qty = row.sim_qty;
        row.recast_penalty_per_cast = 0.0;
        row.recast_penalties.clear();

        // JS: `!spell || !(sim_qty > 0) || !Number.isFinite(sim_qty) ||
        //      mana_excl || spell.cost == null`. The NaN guard matters — a
        // half-edited field parses to NaN and `!(NaN > 0)` is true.
        if row.spell.is_none() || !(sim_qty > 0.0) || !sim_qty.is_finite()
            || row.mana_excl || !row.sim_cost_present
        {
            continue;
        }
        let rc_base = row.sim_recast_base;
        if rc_base == 0 {
            continue;
        }

        let n = sim_qty as usize;
        let mut penalties = vec![f64::NAN; n];
        let mut row_penalty = 0.0f64;
        let mut is_switch = false;
        if Some(rc_base) != last_base {
            is_switch = true;
            if consec <= 1.0 { penalty = 0.0; } else { penalty += 1.0; }
            consec = 0.0;
            last_base = Some(rc_base);
        }

        if is_switch && penalty > 0.0 {
            penalties[0] = penalty * penalty_per;
            row_penalty = penalties[0];
            penalty = 0.0;
            consec = 1.0;
            let remaining = sim_qty - 1.0;
            if remaining > 0.0 {
                let free_remaining = js_min(remaining, 1.0);
                let mut i = 1usize;
                while (i as f64) <= free_remaining {
                    penalties[i] = 0.0;
                    i += 1;
                }
                let penalty_start = 1.0 + free_remaining;
                let mut i = penalty_start as usize;
                while i < n {
                    let k = i as f64 - penalty_start + 1.0;
                    penalties[i] = k * penalty_per;
                    row_penalty += penalties[i];
                    i += 1;
                }
                let penalty_remaining = remaining - free_remaining;
                if penalty_remaining > 0.0 {
                    penalty = penalty_remaining;
                }
                consec += remaining;
            }
        } else if penalty > 0.0 {
            for (i, slot) in penalties.iter_mut().enumerate() {
                *slot = (penalty + 1.0 + i as f64) * penalty_per;
                row_penalty += *slot;
            }
            penalty += sim_qty;
            consec += sim_qty;
        } else {
            let free_casts = js_max(0.0, js_min(sim_qty, 2.0 - consec));
            let mut i = 0usize;
            while (i as f64) < free_casts {
                penalties[i] = 0.0;
                i += 1;
            }
            let mut i = free_casts as usize;
            while i < n {
                let k = i as f64 - free_casts + 1.0;
                penalties[i] = k * penalty_per;
                row_penalty += penalties[i];
                i += 1;
            }
            let penalty_casts = sim_qty - free_casts;
            if penalty_casts > 0.0 {
                penalty = penalty_casts;
            }
            consec += sim_qty;
        }

        row.recast_penalties = penalties;
        row.recast_penalty_per_cast = if sim_qty > 0.0 { row_penalty / sim_qty } else { 0.0 };
    }
}

/// `_unroll_loops_pure` (engine.js:299) — flattens loop brackets using the
/// simulation's observed iteration counts.
///
/// Unlike `scoring::unroll_count_loops`, this handles until-OOM loops: their
/// iteration count is whatever the sim actually ran, looked up by the
/// LOOP_START row index. Count loops still use their static value, so this
/// agrees with the static unroll on loop-free-after-unroll input.
pub fn unroll_loops_dynamic(rows: &[Row], iteration_counts: &[(usize, i64)]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::with_capacity(rows.len());
    let mut i = 0usize;
    while i < rows.len() {
        if let Some((cond_type, cond_value)) = rows[i].loop_start {
            let Some(end_idx) = (i + 1..rows.len()).find(|&j| rows[j].loop_end) else {
                // Unterminated marker: JS skips it and keeps scanning.
                i += 1;
                continue;
            };
            // JS: `cond.value || 1` for count, `counts[i] || 1` otherwise —
            // a missing or zero count degenerates to a single pass.
            let iters = if cond_type == LOOP_COND_COUNT {
                cond_value
            } else {
                iteration_counts
                    .iter()
                    .find(|(k, _)| *k == i)
                    .map(|(_, v)| *v as f64)
                    .filter(|v| *v != 0.0)
                    .unwrap_or(1.0)
            };
            let body: Vec<&Row> = (i + 1..end_idx)
                .filter(|&j| rows[j].loop_start.is_none() && !rows[j].loop_end)
                .map(|j| &rows[j])
                .collect();
            let mut iter = 0.0f64;
            while iter < iters {
                for br in &body {
                    out.push((*br).clone());
                }
                iter += 1.0;
            }
            i = end_idx + 1;
        } else if rows[i].loop_end {
            i += 1;
        } else {
            out.push(rows[i].clone());
            i += 1;
        }
    }
    out
}

/// `extract_slider_names` (simulate.js:173) — returns
/// `(bp_slider_name, [(buff_state index, slider_name)])`.
///
/// The state is identified by index rather than name so injection can read
/// `RowResult::state_values` positionally, with no name comparison in the
/// per-trial path.
pub fn extract_slider_names(hc: &HealthConfig) -> (Option<String>, Vec<(usize, String)>) {
    let bp = hc.damage_boost.as_ref().and_then(|d| d.slider_name.clone());
    let states = hc
        .buff_states
        .iter()
        .enumerate()
        .filter_map(|(i, bs)| bs.slider_name.clone().map(|sn| (i, sn)))
        .collect();
    (bp, states)
}

/// `inject_blood_pact_boosts` (engine.js:237) — appends simulation-derived
/// boost tokens to each row, so the damage pass sees the Blood Pact bonus and
/// the buff-state slider values the sim actually produced.
///
/// A manually-set token of the same name wins (the user's slider is not
/// overridden), matching the JS `_has_manual` check.
pub fn inject_blood_pact_boosts(
    rows: &[Row], sim: &SimResult, bp_slider_name: Option<&str>,
    state_slider_names: &[(usize, String)],
) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let Some(res) = sim.row_results.get(i).filter(|r| r.filled) else {
            out.push(row.clone());
            continue;
        };
        let has_manual =
            |n: &str| row.tokens.iter().any(|t| t.manual && t.name == n);

        let mut extra: Vec<Token> = Vec::new();
        if res.blood_pact_bonus > 0.0 {
            if let Some(name) = bp_slider_name {
                if !has_manual(name) {
                    extra.push(Token {
                        name: name.to_string(),
                        // JS `Math.round(x * 10) / 10`.
                        value: js_round(res.blood_pact_bonus * 10.0) / 10.0,
                        is_pct: true,
                        manual: false,
                    });
                }
            }
        }
        for (state_idx, slider_name) in state_slider_names {
            let val = res.state_values.get(*state_idx).copied().unwrap_or(0.0);
            if val > 0.0 && !has_manual(slider_name) {
                extra.push(Token {
                    name: slider_name.clone(),
                    value: js_round(val),
                    is_pct: false,
                    manual: false,
                });
            }
        }

        if extra.is_empty() {
            out.push(row.clone());
            continue;
        }
        let mut r = row.clone();
        r.tokens.extend(extra);
        out.push(r);
    }
    out
}
