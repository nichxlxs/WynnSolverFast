// Exports differential fixtures for the Rust port of simulate_combo_mana_hp
// (rust/sp_kernel/src/mana_sim.rs).
//
// Each case carries the full input (stats, rows, health_config, registry) and
// the JS simulation's output. `mana_sim_check` replays the inputs through the
// Rust port and requires every number to match bit-for-bit.
//
// Coverage is deliberately wider than test_mana_sim.js, which only checks
// full-vs-fast agreement on inputs the fast sim also handles. The cases below
// exercise the parts only the full sim reaches: buff-state durations,
// continuous and one-shot drain, mana-regen suppression, healing suppression
// and HPR ticks, exit triggers (both effects), Blood Pact health-cost casting,
// per-cast/per-second corruption tracking, cancel_state / mana_reset
// pseudo-spells, and loop brackets in both count and until-OOM form.
//
// Run: node js/solver/tests/gen_mana_sim_fixtures.js
// Out: rust/sp_kernel/fixtures/mana_sim_cases.json

'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');
const { createSandbox, REPO_ROOT } = require('./harness');

const ctx = createSandbox();
const simulate_combo_mana_hp = ctx.simulate_combo_mana_hp;
const simulate_combo_mana_fast = ctx.simulate_combo_mana_fast;

// Top-level `const`s in the sandbox live in its lexical scope, not on the
// context object, so they have to be read back through an expression.
const inCtx = (expr) => vm.runInContext(`(${expr})`, ctx);

// ── Builders ─────────────────────────────────────────────────────────────────

function makeStats(overrides = {}) {
    return {
        mr: 0, ms: 0, maxMana: 0, int: 0,
        hp: 1000, hpBonus: 0, hprRaw: 0, hprPct: 0,
        atkSpd: 'NORMAL', atkTier: 0,
        ...overrides,
    };
}

function makeSpell(name, cost, opts = {}) {
    return {
        name, cost,
        base_spell: opts.base_spell ?? 1,
        scaling: opts.scaling ?? 'spell',
        mana_derived_from: opts.mana_derived_from ?? null,
        hp_cost: opts.hp_cost ?? 0,
        ...(opts.extra ?? {}),
    };
}

function makeRow(qty, spell, opts = {}) {
    return {
        qty, spell,
        boost_tokens: opts.boost_tokens ?? [],
        mana_excl: opts.mana_excl ?? false,
        pseudo: opts.pseudo ?? null,
        recast_penalties: opts.recast_penalties ?? null,
        recast_penalty_per_cast: opts.recast_penalty_per_cast ?? 0,
        cast_time: opts.cast_time ?? null,
        delay: opts.delay ?? null,
        auto_delay: opts.auto_delay ?? true,
        is_melee_time: opts.is_melee_time ?? false,
        melee_cd_override: opts.melee_cd_override ?? null,
    };
}

const loopStart = (type, value) => ({ loop_start: { type, value }, qty: 0, spell: null });
const loopEnd = () => ({ loop_end: true, qty: 0, spell: null });

const DEFAULT_HC = {
    hp_casting: false, health_cost: 0, damage_boost: null,
    buff_states: [], exit_triggers: [],
};

// Vanish / Manic Edge, matching test_mana_sim.js::makeVanishConfig.
function vanishConfig(opts = {}) {
    return {
        hp_casting: false, health_cost: 0, damage_boost: null,
        buff_states: [{
            state_name: 'Vanished',
            activate_on: { spell: 2 },
            deactivate: 'next_action',
            slider_name: 'Mana Lost',
            suppress_healing: true,
            suppress_mana_regen: true,
            drain_pct_per_second: opts.mana_drain ? { mana: 5 } : null,
            compute_delay: opts.mana_drain ?? false,
            apply_to_next: opts.mana_drain ?? false,
            duration: opts.duration ?? 5,
            value_cap: opts.value_cap ?? 26,
            spell_rate_field: null,
            spell_flat_field: null,
            tracking: opts.tracking ?? null,
        }],
        exit_triggers: opts.exit_triggers ?? [],
    };
}

// Bak'al's Grasp style: continuous mana drain, mana_loss tracking, no
// compute_delay, activated by base_spell 3, no next_action deactivation.
function corruptionConfig(opts = {}) {
    return {
        hp_casting: false, health_cost: 0, damage_boost: null,
        buff_states: [{
            state_name: 'Corrupted',
            activate_on: { spell: 3 },
            deactivate: null,
            suppress_healing: opts.suppress_healing ?? true,
            suppress_mana_regen: opts.suppress_mana_regen ?? false,
            drain_pct_per_second: opts.drain ?? { mana: 4 },
            compute_delay: false,
            apply_to_next: false,
            duration: opts.duration ?? null,
            value_cap: opts.value_cap ?? 100,
            spell_rate_field: opts.spell_rate_field ?? null,
            spell_flat_field: opts.spell_flat_field ?? null,
            tracking: opts.tracking ?? 'mana_loss',
        }],
        exit_triggers: opts.exit_triggers ?? [],
    };
}

// Blood Pact: HP pays for mana shortfalls, damage_boost scales with the
// fraction of the cost paid in blood.
function bloodPactConfig(opts = {}) {
    return {
        hp_casting: true,
        health_cost: opts.health_cost ?? 0.5,
        damage_boost: opts.damage_boost ?? { min: 10, max: 40 },
        buff_states: opts.buff_states ?? [],
        exit_triggers: opts.exit_triggers ?? [],
    };
}

// ── Cases ────────────────────────────────────────────────────────────────────

const cases = [];
const add = (name, rows, stats, health_config, has_transcendence = false, registry = []) =>
    cases.push({ name, rows, stats, health_config, has_transcendence, registry });

// -- Baselines that also run on the fast path (regression floor) --------------
add('basic 2 rows',
    [makeRow(2, makeSpell('Spell 1', 30, { base_spell: 1 })),
     makeRow(1, makeSpell('Spell 2', 45, { base_spell: 2 }))],
    makeStats({ mr: 10, int: 50 }), DEFAULT_HC);

add('melee + mana steal',
    [makeRow(1, makeSpell('Spell 1', 25, { base_spell: 1 })),
     makeRow(3, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(1, makeSpell('Spell 2', 40, { base_spell: 2 }))],
    makeStats({ mr: 5, ms: 12, int: 30, atkSpd: 'FAST' }), DEFAULT_HC);

add('melee + negative mana steal',
    [makeRow(1, makeSpell('Spell 1', 25, { base_spell: 1 })),
     makeRow(3, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(1, makeSpell('Spell 2', 40, { base_spell: 2 }))],
    makeStats({ mr: 5, ms: -24, int: 30, atkSpd: 'FAST' }), DEFAULT_HC);

add('transcendence',
    [makeRow(3, makeSpell('Spell 1', 50, { base_spell: 1 })),
     makeRow(2, makeSpell('Spell 2', 35, { base_spell: 2 }))],
    makeStats({ mr: 8, int: 40 }), DEFAULT_HC, true);

add('add_flat_mana',
    [makeRow(2, makeSpell('Spell 1', 40, { base_spell: 1 })),
     makeRow(15, null, { pseudo: 'add_flat_mana' }),
     makeRow(1, makeSpell('Spell 2', 30, { base_spell: 2 }))],
    makeStats({ mr: 5, int: 20 }), DEFAULT_HC);

add('high regen (mana cap + waste)',
    [makeRow(1, makeSpell('Spell 1', 10, { base_spell: 1 })),
     makeRow(5, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(1, makeSpell('Spell 2', 10, { base_spell: 2 }))],
    makeStats({ mr: 200, ms: 50, int: 80, maxMana: 20, atkSpd: 'SUPER_FAST' }), DEFAULT_HC);

add('empty combo', [], makeStats({ mr: 10, int: 50 }), DEFAULT_HC);

add('mana warning (transcendence castability gate)',
    [makeRow(1, makeSpell('Expensive Spell', 120, { base_spell: 1 }))],
    makeStats(), DEFAULT_HC, true);

add('recast penalty absorbed by clamp',
    [makeRow(3, makeSpell('Cheap Spell', 30, { base_spell: 1 }),
        { recast_penalties: [0, 0, 5], recast_penalty_per_cast: 5 / 3 })],
    makeStats({ spRaw1: -200 }), DEFAULT_HC);

add('recast penalty partial absorption',
    [makeRow(3, makeSpell('Spell', 30, { base_spell: 1 }),
        { recast_penalties: [0, 0, 5], recast_penalty_per_cast: 5 / 3 })],
    makeStats({ spRaw1: -18 }), DEFAULT_HC);

add('mana_excl row skipped',
    [makeRow(2, makeSpell('Spell 1', 40, { base_spell: 1 }), { mana_excl: true }),
     makeRow(1, makeSpell('Spell 2', 30, { base_spell: 2 }))],
    makeStats({ mr: 5, int: 20 }), DEFAULT_HC);

add('melee time row',
    [makeRow(4, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 }),
        { is_melee_time: true })],
    makeStats({ mr: 5, ms: 10, atkSpd: 'FAST' }), DEFAULT_HC);

add('hpr ticks restore hp',
    [makeRow(20, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(2, makeSpell('Costly', 60, { base_spell: 1, hp_cost: 0 }))],
    makeStats({ mr: 5, hp: 2000, hprRaw: 120, hprPct: 30, atkSpd: 'SLOW' }), DEFAULT_HC);

// -- Buff states: suppression, duration, drain -------------------------------
add('vanish: mana regen suppression',
    [makeRow(1, makeSpell('Dash', 25, { base_spell: 2 })),
     makeRow(1, makeSpell('Attack', 30, { base_spell: 1 }))],
    makeStats({ mr: 10 }), vanishConfig({ mana_drain: false }));

add('manic edge: one-shot drain at activation',
    [makeRow(1, makeSpell('Dash', 10, { base_spell: 2 })),
     makeRow(1, makeSpell('Attack', 10, { base_spell: 1 }))],
    makeStats(), vanishConfig({ mana_drain: true }));

add('manic edge: duration cap on drain',
    [makeRow(1, makeSpell('Dash', 5, { base_spell: 2 })),
     makeRow(10, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 }))],
    makeStats({ atkSpd: 'SLOW' }), vanishConfig({ mana_drain: true }));

add('manic edge: value_cap with a large pool',
    [makeRow(1, makeSpell('Dash', 5, { base_spell: 2 })),
     makeRow(20, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 }))],
    makeStats({ maxMana: 900 }), vanishConfig({ mana_drain: true, value_cap: 26 }));

add('manic edge: low mana scales drain down',
    [makeRow(1, makeSpell('Dash', 90, { base_spell: 2 })),
     makeRow(1, makeSpell('Attack', 5, { base_spell: 1 }))],
    makeStats(), vanishConfig({ mana_drain: true }));

add('manic edge: manual delay (auto_delay false)',
    [makeRow(1, makeSpell('Dash', 10, { base_spell: 2 }), { auto_delay: false, delay: 1.5 }),
     makeRow(1, makeSpell('Attack', 10, { base_spell: 1 }))],
    makeStats(), vanishConfig({ mana_drain: true }));

add('buff state expires mid-tick (short duration)',
    [makeRow(1, makeSpell('Dash', 10, { base_spell: 2 })),
     makeRow(6, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(1, makeSpell('Attack', 10, { base_spell: 1 }))],
    makeStats({ mr: 20, atkSpd: 'SLOW' }),
    vanishConfig({ mana_drain: false, duration: 0.35 }));

// -- Continuous drain + tracking ---------------------------------------------
add('corruption: continuous mana drain + mana_loss tracking',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(8, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(2, makeSpell('Attack', 15, { base_spell: 1 }))],
    makeStats({ mr: 30, ms: 8, atkSpd: 'SLOW' }), corruptionConfig());

add('corruption: drain with duration + suppress_mana_regen',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(10, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 }))],
    makeStats({ mr: 40, atkSpd: 'SLOW' }),
    corruptionConfig({ duration: 3, suppress_mana_regen: true }));

add('corruption: per-second + per-cast spell fields',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(6, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0,
        extra: { corrupt_rate: 3, corrupt_flat: 1.5 } }))],
    makeStats({ mr: 25, atkSpd: 'NORMAL' }),
    corruptionConfig({ spell_rate_field: 'corrupt_rate', spell_flat_field: 'corrupt_flat',
        value_cap: 20, tracking: null }));

add('corruption: hp_loss_pct tracking from spell hp_cost',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(4, makeSpell('Slaughter', 30, { base_spell: 4, hp_cost: 6 }))],
    makeStats({ mr: 20, hp: 3000 }),
    corruptionConfig({ tracking: 'hp_loss_pct', value_cap: 100, drain: { mana: 0 } }));

// -- Exit triggers -----------------------------------------------------------
add('exit trigger: heal_pct_of_state on next_action',
    [makeRow(1, makeSpell('Dash', 10, { base_spell: 2 })),
     makeRow(1, makeSpell('Attack', 10, { base_spell: 1 }))],
    makeStats({ hp: 2000 }),
    vanishConfig({ mana_drain: true, exit_triggers: [
        { state: 'Vanished', on: 'exit', effect: 'heal_pct_of_state', value: 20,
          slider_name: null, cooldown: 0 }] }));

add('exit trigger: heal_per_hit with cooldown',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(12, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 }),
        { boost_tokens: [{ name: 'Enemies', value: 3, is_pct: false }] }),
     makeRow(1, null, { pseudo: 'cancel_state:Corrupted',
        boost_tokens: [{ name: 'Enemies', value: 3, is_pct: false }] })],
    makeStats({ mr: 20, hp: 4000, hprRaw: 50, atkSpd: 'SLOW' }),
    corruptionConfig({ drain: { mana: 2 }, exit_triggers: [
        { state: 'Corrupted', on: 'exit', effect: 'heal_per_hit', value: 0.02,
          slider_name: 'Enemies', cooldown: 1.5 }] }));

add('exit trigger: heal_per_hit without cooldown',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(6, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 }),
        { boost_tokens: [{ name: 'Enemies', value: 2, is_pct: false }] }),
     makeRow(1, null, { pseudo: 'cancel_state:Corrupted',
        boost_tokens: [{ name: 'Enemies', value: 2, is_pct: false }] })],
    makeStats({ mr: 20, hp: 4000, atkSpd: 'NORMAL' }),
    corruptionConfig({ drain: { mana: 2 }, exit_triggers: [
        { state: 'Corrupted', on: 'exit', effect: 'heal_per_hit', value: 0.01,
          slider_name: 'Enemies', cooldown: 0 }] }));

// -- Pseudo-spells -----------------------------------------------------------
add('mana_reset refills to max',
    [makeRow(2, makeSpell('Spell 1', 45, { base_spell: 1 })),
     makeRow(0, null, { pseudo: 'mana_reset' }),
     makeRow(1, makeSpell('Spell 2', 40, { base_spell: 2 }))],
    makeStats({ mr: 5, int: 20 }), DEFAULT_HC);

add('cancel_state on a mana_excl row is a no-op',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(4, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(1, null, { pseudo: 'cancel_state:Corrupted', mana_excl: true }),
     makeRow(4, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 }))],
    makeStats({ mr: 25, hp: 4000, atkSpd: 'NORMAL' }),
    corruptionConfig({ exit_triggers: [
        { state: 'Corrupted', on: 'exit', effect: 'heal_pct_of_state', value: 15,
          slider_name: null, cooldown: 0 }] }));

add('buff state duration expires fully between rows',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(30, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(2, makeSpell('Attack', 15, { base_spell: 1 }))],
    makeStats({ mr: 30, hp: 2000, hprRaw: 80, atkSpd: 'SLOW' }),
    corruptionConfig({ duration: 1.2, suppress_mana_regen: true, suppress_healing: true }));

add('cancel_state with no trigger',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(4, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(1, null, { pseudo: 'cancel_state:Corrupted' }),
     makeRow(4, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 }))],
    makeStats({ mr: 25, atkSpd: 'NORMAL' }), corruptionConfig());

// -- Blood Pact --------------------------------------------------------------
add('blood pact: partial blood payment',
    [makeRow(4, makeSpell('Totem', 40, { base_spell: 1 }))],
    makeStats({ mr: 5, hp: 3000 }), bloodPactConfig());

add('blood pact: full blood payment (mana exhausted)',
    [makeRow(8, makeSpell('Totem', 45, { base_spell: 1 }))],
    makeStats({ mr: 0, hp: 5000 }), bloodPactConfig());

add('blood pact + transcendence',
    [makeRow(6, makeSpell('Totem', 45, { base_spell: 1 }))],
    makeStats({ mr: 5, hp: 4000 }), bloodPactConfig(), true);

add('blood pact: hp warning when hp runs out',
    [makeRow(10, makeSpell('Totem', 60, { base_spell: 1 }))],
    makeStats({ mr: 0, hp: 300 }), bloodPactConfig({ health_cost: 2 }));

add('blood pact with corruption tracking',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(6, makeSpell('Totem', 50, { base_spell: 1 }))],
    makeStats({ mr: 5, hp: 3000 }),
    { ...bloodPactConfig(), buff_states: corruptionConfig({ tracking: 'hp_loss_pct',
        drain: { mana: 0 } }).buff_states });

add('spell hp_cost with blood pact damage_boost',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(3, makeSpell('Slaughter', 25, { base_spell: 4, hp_cost: 8 }))],
    makeStats({ mr: 20, hp: 4000 }),
    { ...bloodPactConfig({ health_cost: 0 }), hp_casting: false,
      buff_states: corruptionConfig({ tracking: 'hp_loss_pct',
        drain: { mana: 0 } }).buff_states });

// -- Boost-token injection (damage feedback from the sim) ---------------------
add('inject: blood pact slider',
    [makeRow(4, makeSpell('Totem', 40, { base_spell: 1 }))],
    makeStats({ mr: 5, hp: 3000 }),
    bloodPactConfig({ damage_boost: { min: 10, max: 40, slider_name: 'Blood Pact' } }));

add('inject: blood pact slider overridden by a manual token',
    [makeRow(4, makeSpell('Totem', 40, { base_spell: 1 }),
        { boost_tokens: [{ name: 'Blood Pact', value: 12, is_pct: true, manual: true }] })],
    makeStats({ mr: 5, hp: 3000 }),
    bloodPactConfig({ damage_boost: { min: 10, max: 40, slider_name: 'Blood Pact' } }));

add('inject: state slider overridden by a manual token',
    [makeRow(1, makeSpell('Dash', 10, { base_spell: 2 }),
        { boost_tokens: [{ name: 'Mana Lost', value: 7, is_pct: false, manual: true }] }),
     makeRow(1, makeSpell('Attack', 10, { base_spell: 1 }),
        { boost_tokens: [{ name: 'Mana Lost', value: 7, is_pct: false, manual: true }] })],
    makeStats(), vanishConfig({ mana_drain: true }));

add('inject: both sliders on the same row',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     makeRow(4, makeSpell('Totem', 55, { base_spell: 1 }))],
    makeStats({ mr: 5, hp: 3000 }),
    { ...bloodPactConfig({ damage_boost: { min: 10, max: 40, slider_name: 'Blood Pact' } }),
      buff_states: corruptionConfig({ tracking: 'hp_loss_pct', drain: { mana: 0 } })
        .buff_states.map(bs => ({ ...bs, slider_name: 'Corruption' })) });

add('inject: loop body rows get per-iteration values',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     loopStart(0, 3),
     makeRow(2, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     makeRow(1, makeSpell('Attack', 15, { base_spell: 1 })),
     loopEnd()],
    makeStats({ mr: 30, atkSpd: 'NORMAL' }),
    corruptionConfig({ tracking: 'mana_loss' }));

// -- Loop brackets -----------------------------------------------------------
add('loop: count 3',
    [loopStart(0, 3),
     makeRow(1, makeSpell('Spell 1', 20, { base_spell: 1 })),
     makeRow(2, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     loopEnd(),
     makeRow(1, makeSpell('Spell 2', 15, { base_spell: 2 }))],
    makeStats({ mr: 15, ms: 5, int: 20 }), DEFAULT_HC);

add('loop: count 1 (degenerate)',
    [loopStart(0, 1),
     makeRow(2, makeSpell('Spell 1', 20, { base_spell: 1 })),
     loopEnd()],
    makeStats({ mr: 10 }), DEFAULT_HC);

add('loop: until-OOM terminates on mana warning',
    [loopStart(1, 0),
     makeRow(1, makeSpell('Spell 1', 35, { base_spell: 1 })),
     loopEnd()],
    makeStats({ mr: 2, int: 0 }), DEFAULT_HC);

add('loop: until-OOM with regen (safety cap)',
    [loopStart(1, 0),
     makeRow(1, makeSpell('Cheap', 1, { base_spell: 1 })),
     loopEnd()],
    makeStats({ mr: 200, int: 50 }), DEFAULT_HC);

add('loop: until-OOM terminates on hp warning (blood pact)',
    [loopStart(1, 0),
     makeRow(2, makeSpell('Totem', 50, { base_spell: 1 })),
     loopEnd()],
    makeStats({ mr: 0, hp: 400 }), bloodPactConfig({ health_cost: 3 }));

add('loop: start marker with no matching end',
    [loopStart(0, 2),
     makeRow(1, makeSpell('Spell 1', 20, { base_spell: 1 })),
     makeRow(1, makeSpell('Spell 2', 20, { base_spell: 2 }))],
    makeStats({ mr: 10, hp: 400 }), bloodPactConfig({ health_cost: 3 }));

add('loop: until-OOM inside blood pact, terminated',
    [loopStart(1, 0),
     makeRow(2, makeSpell('Totem', 50, { base_spell: 1 })),
     loopEnd(),
     makeRow(1, makeSpell('Finisher', 10, { base_spell: 2 }))],
    makeStats({ mr: 0, hp: 600 }), bloodPactConfig({ health_cost: 3 }));

add('loop: buff state persists across iterations',
    [makeRow(1, makeSpell('Bakal', 20, { base_spell: 3 })),
     loopStart(0, 4),
     makeRow(2, makeSpell('Melee', null, { base_spell: 0, scaling: 'melee', mana_derived_from: 0 })),
     loopEnd()],
    makeStats({ mr: 30, atkSpd: 'NORMAL' }), corruptionConfig());

add('loop: unterminated start marker',
    [loopStart(0, 2),
     makeRow(1, makeSpell('Spell 1', 20, { base_spell: 1 }))],
    makeStats({ mr: 10 }), DEFAULT_HC);

add('loop: end marker with no start',
    [makeRow(1, makeSpell('Spell 1', 20, { base_spell: 1 })),
     loopEnd()],
    makeStats({ mr: 10 }), DEFAULT_HC);

// -- Boost registry ----------------------------------------------------------
{
    const registry = [{
        name: 'Focus',
        stat_bonuses: [
            { key: 'int', value: 5, round: true },
            { key: 'spRaw1', value: -3, round: true },
        ],
    }];
    add('boost registry adjusts spell cost',
        [makeRow(3, makeSpell('Spell 1', 40, { base_spell: 1 }),
            { boost_tokens: [{ name: 'Focus', value: 2, is_pct: false }] })],
        makeStats({ mr: 5, int: 40 }), DEFAULT_HC, false, registry);
}

// ── Run + serialize ──────────────────────────────────────────────────────────

// JSON has no NaN/Infinity; encode them so Rust can compare exactly.
function encNum(x) {
    if (typeof x !== 'number') return x;
    if (Number.isNaN(x)) return '@NaN';
    if (x === Infinity) return '@Inf';
    if (x === -Infinity) return '@-Inf';
    return x;
}

function encResult(r) {
    return {
        end_mana: encNum(r.end_mana),
        start_mana: encNum(r.start_mana),
        max_mana: encNum(r.max_mana),
        end_hp: encNum(r.end_hp),
        max_hp: encNum(r.max_hp),
        total_mana_cost: encNum(r.total_mana_cost),
        total_mana_drain: encNum(r.total_mana_drain),
        melee_hits: encNum(r.melee_hits),
        recast_penalty_total: encNum(r.recast_penalty_total),
        mana_wasted: encNum(r.mana_wasted),
        loop_iteration_counts: Object.entries(r.loop_iteration_counts)
            .map(([k, v]) => [Number(k), v]),
        spell_costs: r.spell_costs.map(sc => ({
            name: sc.name, qty: sc.qty,
            cost: encNum(sc.cost), recast_penalty: encNum(sc.recast_penalty),
        })),
        row_results: r.row_results.map(rr => rr == null ? null : ({
            blood_pact_bonus: encNum(rr.blood_pact_bonus),
            state_values: Object.entries(rr.state_values).map(([k, v]) => [k, encNum(v)]),
            hp_warning: rr.hp_warning,
            mana_warning: rr.mana_warning,
            computed_delay: rr.computed_delay == null ? null : encNum(rr.computed_delay),
            mana_lost: encNum(rr.mana_lost),
            mana_gained: encNum(rr.mana_gained),
            elapsed_time: encNum(rr.elapsed_time),
            row_dt: encNum(rr.row_dt),
            cast_time: rr.cast_time == null ? null : encNum(rr.cast_time),
            delay: rr.delay == null ? null : encNum(rr.delay),
        })),
    };
}

const out = {
    tables: inCtx(`{
        skillpoint_damage_mult: [...skillpoint_damage_mult],
        skillpoint_final_mult: [...skillpoint_final_mult],
        baseDamageMultiplier: [...baseDamageMultiplier],
        attackSpeeds: [...attackSpeeds],
        damage_keys: [...damage_keys],
        sp_percentage_rate: SP_PERCENTAGE_RATE,
        sp_percentage_input_cap: SP_PERCENTAGE_INPUT_CAP,
        // V8's Math.pow and Rust's powf can differ by 1 ULP; skill points are
        // integers, so ship the exact JS values instead.
        sp_pct_table: Array.from({length: SP_PERCENTAGE_INPUT_CAP + 1},
            (_, i) => skillPointsToPercentage(i)),
    }`),
    constants: inCtx(`{
        base_mana_regen: BASE_MANA_REGEN,
        mana_tick_seconds: MANA_TICK_SECONDS,
        spell_cast_time: SPELL_CAST_TIME,
        spell_cast_delay: SPELL_CAST_DELAY,
        hpr_tick_seconds: HPR_TICK_SECONDS,
        hidden_base_hpr: HIDDEN_BASE_HPR,
    }`),
    cases: [],
};

const compute_recast_penalties = ctx.compute_recast_penalties;
const _unroll_loops_pure = inCtx('_unroll_loops_pure');
const inject_blood_pact_boosts = inCtx('inject_blood_pact_boosts');
const extract_slider_names = inCtx('extract_slider_names');

// Rows carry `sim_qty` in the real pipeline (set by _parse_combo_for_search);
// the sim derives it the same way, so mirror that here.
function withSimQty(rows, stats) {
    const sm = new Map(Object.entries(stats));
    return rows.map(r => {
        if (r.loop_start || r.loop_end) return { ...r };
        const sim_qty = r.is_melee_time
            ? Math.round(inCtx('compute_melee_time_hits')(
                r.qty, sm, r.delay ?? undefined, r.melee_cd_override ?? undefined))
            : Math.round(r.qty ?? 0);
        return { ...r, sim_qty };
    });
}

function encRows(rows) {
    return rows.map(r => ({
        sim_qty: r.sim_qty == null ? null : encNum(r.sim_qty),
        recast_penalty_per_cast: encNum(r.recast_penalty_per_cast ?? 0),
        recast_penalties: r.recast_penalties == null ? null
            : r.recast_penalties.map(encNum),
        boost_tokens: (r.boost_tokens ?? []).map(t => ({
            name: t.name, value: encNum(t.value),
            is_pct: !!t.is_pct, manual: !!t.manual,
        })),
        spell_name: r.spell?.name ?? null,
    }));
}

for (const c of cases) {
    const stats = new Map(Object.entries(c.stats));
    const rows = withSimQty(c.rows, c.stats);
    const result = simulate_combo_mana_hp(
        rows, stats, c.health_config, c.has_transcendence, c.registry);

    // Dynamic unroll by the sim's observed iteration counts, then the
    // recast-penalty re-run the JS damage path performs on the flat combo.
    //
    // _unroll_loops_pure reuses (does not copy) rows outside a loop body, so
    // compute_recast_penalties would write through to `rows` and leave the
    // serialized input disagreeing with the `expected` captured above. The
    // engine is unaffected — recast penalties depend only on the row
    // sequence, so recomputing them is idempotent — but the fixture has to
    // record the inputs as they were. Unroll a deep copy instead.
    const flat = _unroll_loops_pure(
        JSON.parse(JSON.stringify(rows)), result.loop_iteration_counts ?? {});
    compute_recast_penalties(flat);

    // Boost-token injection needs a sim over the FLAT rows (row_results must
    // line up 1:1), exactly as eval_combo_damage_with_bp does.
    const flatSim = simulate_combo_mana_hp(
        flat, stats, c.health_config, c.has_transcendence, c.registry);
    const { bp_slider_name, state_slider_names } = extract_slider_names(c.health_config);
    const injected = inject_blood_pact_boosts(
        flat, flatSim, bp_slider_name, state_slider_names);

    // The worker path runs the FAST sim, which models buff states, the
    // Blood Pact payment branch and loop brackets too — it just tracks no
    // state values and fires no exit triggers. The Rust fast sim has to
    // match this, not the full one.
    const fast = simulate_combo_mana_fast(
        rows, stats, c.health_config, c.has_transcendence, c.registry);

    out.cases.push({
        name: c.name,
        stats: c.stats,
        rows,
        health_config: c.health_config,
        has_transcendence: c.has_transcendence,
        registry: c.registry,
        expected: encResult(result),
        expected_fast: {
            start_mana: encNum(fast.start_mana),
            end_mana: encNum(fast.end_mana),
            has_hp_warning: fast.has_hp_warning,
            has_mana_warning: fast.has_mana_warning,
        },
        expected_slider_names: {
            bp: bp_slider_name,
            states: Object.entries(state_slider_names),
        },
        expected_unrolled: encRows(flat),
        expected_injected: encRows(injected),
    });
}

const dest = path.join(REPO_ROOT, 'rust', 'sp_kernel', 'fixtures', 'mana_sim_cases.json');
fs.writeFileSync(dest, JSON.stringify(out, null, 1));
console.log(`wrote ${out.cases.length} cases -> ${path.relative(REPO_ROOT, dest)}`);
