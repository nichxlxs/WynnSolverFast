//! Shared fixture builders for the Rust/WASM engine.
//!
//! These produce the two payloads the Rust engine consumes — the enumeration
//! fixture (text) and the scoring fixture (JSON) — from the SAME
//! `initMsgBase` the solver already builds when it spawns its workers. Both
//! the Node test harness and the browser use this one implementation, so the
//! format cannot drift between what is validated and what ships.
//!
//! Sandbox access is injected as `env = { ctx, evalInCtx }`: `ctx` exposes
//! the game functions (calculate_skillpoints, atree_*, _init_running_statmap,
//! sets, ...) and `evalInCtx(src)` evaluates an expression where the game
//! globals are in scope. Node passes a `vm` context; the browser passes its
//! own globals — see `browserEnv()` below.
//!
//! Browser-safe: no fs/path/vm imports. Callers write the strings wherever
//! they need them (file, Blob, or straight into WASM).

function _jser(v) {
    if (v instanceof Map) {
        const o = {};
        for (const [k, x] of v) o[String(k)] = _jser(x);
        return { __m: o };
    }
    if (v instanceof Set) return { __s: [...v].map(_jser) };
    if (Array.isArray(v)) return v.map(_jser);
    if (v && typeof v === 'object') {
        const o = {};
        for (const [k, x] of Object.entries(v)) if (x !== undefined) o[k] = _jser(x);
        return o;
    }
    return v === undefined ? null : v;
}


/// Build the scoring fixture.
///
/// `env.sampling` is TEST-ONLY machinery (it spawns solver workers to
/// generate validation cases with expected values). The browser omits it
/// and receives the same fixture with an empty `cases` array — the engine
/// only needs `cases` for differential validation, never to solve.
function buildScoreFixture(initMsgBase, ringPoolSer, numCases, writeOut, env) {
    return new Promise((resolve, reject) => {
        const sampling = env && env.sampling;
        const ORACLE_ARMOR_SLOTS = (sampling && sampling.ORACLE_ARMOR_SLOTS)
            || ['helmet', 'chestplate', 'leggings', 'boots', 'bracelet', 'necklace'];
        const WORKER_THREAD_PATH = sampling && sampling.WORKER_THREAD_PATH;
        const WorkerCtor = sampling && sampling.Worker;
        const REPO_ROOT = sampling && sampling.REPO_ROOT;
        const _oracleTupleBlocked = (sampling && sampling._oracleTupleBlocked)
            || (() => false);
        if (typeof process !== 'undefined' && process.env) process.env.SOLVER_DEBUG_COMBO = '1';

        const freeSlots = ORACLE_ARMOR_SLOTS.filter(s => initMsgBase.pools[s]?.length);
        const ring1Free = !initMsgBase.ring1_locked && ringPoolSer.length > 0;
        const ring2Free = !initMsgBase.ring2_locked && ringPoolSer.length > 0;
        const lockedItems = [
            ...Object.values(initMsgBase.locked ?? {}),
            initMsgBase.ring1_locked, initMsgBase.ring2_locked,
        ].filter(Boolean);

        let seed = 0x5EEDCAFE;
        const rand = () => (seed = (seed * 1664525 + 1013904223) >>> 0) / 0x100000000;
        // Pools are priority-ordered, so jointly-feasible builds concentrate
        // at low offsets; a geometric bias samples realistic builds while
        // still reaching deep into the pool occasionally.
        const geoPick = (n) => Math.min(n - 1, Math.floor(-Math.log(1 - rand()) * (n / 6)));

        const cases = [];
        let attempts = 0;
        let posted = 0, doneNoTop = 0, doneNoDebug = 0, spFiltered = 0, blocked = 0;
        const MAX_ATTEMPTS = numCases * 2000;

        const worker = new WorkerCtor(WORKER_THREAD_PATH, { workerData: { repoRoot: REPO_ROOT } });

        const sendNext = () => {
            while (cases.length < numCases && attempts < MAX_ATTEMPTS) {
                attempts++;
                const lockedOverride = { ...initMsgBase.locked };
                const items = [];
                for (const s of freeSlots) {
                    const pool = initMsgBase.pools[s];
                    const item = pool[geoPick(pool.length)];
                    lockedOverride[s] = item;
                    items.push(item);
                }
                let ring1_locked = initMsgBase.ring1_locked;
                let ring2_locked = initMsgBase.ring2_locked;
                if (ring1Free && ring2Free) {
                    const i = geoPick(ringPoolSer.length);
                    const j = Math.min(ringPoolSer.length - 1, i + geoPick(ringPoolSer.length - i));
                    ring1_locked = ringPoolSer[i];
                    ring2_locked = ringPoolSer[j];
                    items.push(ring1_locked, ring2_locked);
                } else if (ring1Free) {
                    ring1_locked = ringPoolSer[geoPick(ringPoolSer.length)];
                    items.push(ring1_locked);
                } else if (ring2Free) {
                    ring2_locked = ringPoolSer[geoPick(ringPoolSer.length)];
                    items.push(ring2_locked);
                }
                items.push(...lockedItems);
                if (_oracleTupleBlocked(items)) { blocked++; continue; }

                // In-process SP prefilter: skip obviously SP-infeasible builds
                // without a worker round-trip (the worker remains the truth).
                const bySlot = {
                    helmet: lockedOverride.helmet, chestplate: lockedOverride.chestplate,
                    leggings: lockedOverride.leggings, boots: lockedOverride.boots,
                    ring1: ring1_locked, ring2: ring2_locked,
                    bracelet: lockedOverride.bracelet, necklace: lockedOverride.necklace,
                };
                const wynnSMs = ['boots', 'leggings', 'chestplate', 'helmet',
                                 'ring1', 'ring2', 'bracelet', 'necklace']
                    .map(s => bySlot[s]?.statMap ?? initMsgBase.none_item_sms[NONE_IDX[s]]);
                let spOk = null;
                try {
                    spOk = env.ctx.calculate_skillpoints(
                        wynnSMs, initMsgBase.weapon_sm, initMsgBase.sp_budget);
                } catch (e) {
                    if (spFiltered === 0) console.log('  [score-export] prefilter error:', e.message);
                }
                if (spOk === null) { spFiltered++; continue; }
                posted++;

                worker.postMessage({
                    ...initMsgBase,
                    // Restrictions gate which leaves reach scoring but do not
                    // affect the damage computation these cases validate —
                    // strip them so random tuples survive to scoring.
                    restrictions: { stat_thresholds: [] },
                    pools: {},
                    ring_pool: [],
                    locked: lockedOverride,
                    ring1_locked,
                    ring2_locked,
                    partition: { type: 'full' },
                    worker_id: 0,
                });
                return;
            }
            worker.terminate();
            finish();
        };

        const finish = () => {
            const ctxTables = env.evalInCtx(`({
                skillpoint_damage_mult: [...skillpoint_damage_mult],
                skillpoint_final_mult: [...skillpoint_final_mult],
                baseDamageMultiplier: [...baseDamageMultiplier],
                attackSpeeds: [...attackSpeeds],
                damage_keys: [...damage_keys],
                sp_percentage_rate: SP_PERCENTAGE_RATE,
                sp_percentage_input_cap: SP_PERCENTAGE_INPUT_CAP,
                // V8's Math.pow and Rust's powf can differ by 1 ULP; skill
                // points are integers, so ship the exact JS values instead.
                sp_pct_table: Array.from({length: SP_PERCENTAGE_INPUT_CAP + 1},
                    (_, i) => skillPointsToPercentage(i)),
            })`);

            // Pre-resolve atree hit-string refs per base_spell (mirrors
            // apply_spell_prop_overrides's scan of atree_merged).
            const hit_refs = {};
            for (const [, abil] of (initMsgBase.atree_merged ?? new Map())) {
                for (const effect of (abil.effects ?? [])) {
                    if (effect.type === 'replace_spell') {
                        const bs = effect.base_spell;
                        for (const part of (effect.parts ?? [])) {
                            if (part && typeof part === 'object' && 'hits' in part) {
                                ((hit_refs[bs] ??= {}))[part.name] = { ...((hit_refs[bs] ?? {})[part.name] ?? {}), ...part.hits };
                            }
                        }
                    } else if (effect.type === 'add_spell_prop'
                               && effect.target_part && 'hits' in effect) {
                        const bs = effect.base_spell;
                        ((hit_refs[bs] ??= {}))[effect.target_part] = { ...((hit_refs[bs] ?? {})[effect.target_part] ?? {}), ...effect.hits };
                    }
                }
            }

            // Layer-2 scenario data: everything the Rust leaf pipeline needs
            // to reproduce build stats → assemble → greedy → mana → score
            // from raw items (PORT_PLAN.md). Item registry covers pools,
            // locked, rings, and none items, keyed by displayName.
            const item_registry = {};
            const regAdd = (it) => {
                const sm = it?.statMap ?? it;
                if (!sm?.get) return;
                const name = sm.get('displayName') ?? sm.get('name');
                if (name && !(name in item_registry)) item_registry[name] = _jser(sm);
            };
            for (const pool of Object.values(initMsgBase.pools)) for (const it of pool) regAdd(it);
            for (const it of ringPoolSer) regAdd(it);
            for (const it of Object.values(initMsgBase.locked ?? {})) regAdd(it);
            regAdd(initMsgBase.ring1_locked);
            regAdd(initMsgBase.ring2_locked);

            // Lower the atree scaling plan exactly the way the worker's
            // _atree_scaling_setup does (const partition + numeric var
            // effects), so the Rust side ports only atree_eval_stat_effects.
            // kind 'full' marks scenarios outside the supported subset —
            // the Rust layer-2 pipeline must refuse those fixtures.
            const scaling_plan = (() => {
                const am = initMsgBase.atree_merged;
                const A = env.ctx.atree_scaling_analysis(am);
                const skip_edit = !A.has_prop_outputs;
                if (!A.stat_dependent) {
                    const [, scaled] = env.ctx.atree_compute_scaling(am, new Map(),
                        initMsgBase.button_states, initMsgBase.slider_states, null, skip_edit);
                    return { kind: 'cached', scaled: _jser(scaled) };
                }
                const plan = env.ctx.atree_collect_stat_effects(am);
                if (!plan || plan.var_has_prop_io) return { kind: 'full' };
                const [, const_scaled] = env.ctx.atree_compute_scaling(am, new Map(),
                    initMsgBase.button_states, initMsgBase.slider_states, null, skip_edit, true);
                for (const key of plan.var_keys) {
                    if (key.includes('.') || ['damMult', 'defMult', 'healMult', 'manaMult'].includes(key)
                        || const_scaled.has(key)) {
                        return { kind: 'full' };
                    }
                }
                const lowered = plan.var_effects.map(effect => {
                    const scaling = effect.scaling ?? [0];
                    const inputs = effect.inputs ?? [];
                    const terms = [];
                    let const_add = 0;
                    for (let i = 0; i < Math.min(scaling.length, inputs.length); i++) {
                        const s = env.ctx.atree_translate(am, scaling[i]);
                        if (inputs[i].type === 'stat') {
                            terms.push({ stat: inputs[i].name, factor: s });
                        } else if (inputs[i].type === 'prop') {
                            const abil = am.get(inputs[i].abil);
                            if (abil) const_add += abil.properties[inputs[i].name] * s;
                        }
                    }
                    const low = {
                        round: effect.round ?? true,
                        positive: effect.positive ?? true,
                        terms,
                        const_add,
                        outputs: (Array.isArray(effect.output) ? effect.output : [effect.output])
                            .filter(o => o && o.type === 'stat').map(o => o.name),
                    };
                    if ('max' in effect) low.max = env.ctx.atree_translate(am, effect.max);
                    return low;
                });
                return { kind: 'split', const_scaled: _jser(const_scaled), var_effects: lowered };
            })();

            const ctxLayer2 = env.evalInCtx(`({
                statmap_static_ids: [...STATMAP_STATIC_IDS],
                statmap_must_ids: [...STATMAP_MUST_IDS],
                hp_base_for_level: levelToHPBase(${JSON.stringify(initMsgBase.level)}),
                class_def: Object.fromEntries(classDefenseMultipliers),
                base_mana_regen: BASE_MANA_REGEN,
                mana_tick_seconds: MANA_TICK_SECONDS,
                spell_cast_time: SPELL_CAST_TIME,
                spell_cast_delay: SPELL_CAST_DELAY,
                skp_order: [...skp_order],
            })`);

            const fixture = {
                meta: {
                    generated: 'exportScoreFixture',
                    cases: cases.length,
                    attempts,
                    scoring_target: initMsgBase.scoring_target,
                },
                tables: ctxTables,
                weapon_sm: _jser(initMsgBase.weapon_sm),
                parsed_combo: _jser(initMsgBase.parsed_combo),
                boost_registry: _jser(initMsgBase.boost_registry),
                atree_hit_refs: _jser(hit_refs),
                // Layer-2 scenario data (see rust/sp_kernel/PORT_PLAN.md)
                layer2: {
                    level: initMsgBase.level,
                    sp_budget: initMsgBase.sp_budget,
                    combo_time: initMsgBase.combo_time,
                    allow_downtime: initMsgBase.allow_downtime,
                    hp_casting: initMsgBase.hp_casting,
                    health_config: _jser(initMsgBase.health_config),
                    tome_sms: _jser(initMsgBase.tome_sms),
                    guild_tome_sm: _jser(initMsgBase.guild_tome_sm),
                    none_item_sms: _jser(initMsgBase.none_item_sms),
                    none_idx_map: initMsgBase.none_idx_map,
                    sets_data: _jser(new Map(initMsgBase.sets_data)),
                    atree_raw: _jser(initMsgBase.atree_raw),
                    atree_merged: _jser(initMsgBase.atree_merged),
                    button_states: _jser(initMsgBase.button_states),
                    slider_states: _jser(initMsgBase.slider_states),
                    static_boosts: _jser(initMsgBase.static_boosts),
                    radiance_boost: _jser(initMsgBase.radiance_boost ?? null),
                    spell_base_costs: _jser(initMsgBase.spell_base_costs ?? null),
                    restrictions: initMsgBase.restrictions ?? { stat_thresholds: [] },
                    custom_weights: _jser(initMsgBase.custom_weights ?? null),
                    scaling_plan,
                    constants: ctxLayer2,
                    item_registry,
                },
                cases,
            };
            if (writeOut) writeOut(JSON.stringify(fixture));
            return fixture;
            console.log(`  [score-export] wrote ${cases.length} cases (${attempts} attempts, `
                + `${blocked} blocked, ${spFiltered} sp-filtered, ${posted} posted, `
                + `${doneNoTop} no-top, ${doneNoDebug} no-debug) to ${outPath}`);
            resolve(cases.length);
        };

        worker.on('message', (msg) => {
            if (msg.type === 'done') {
                const top = msg.top5?.[0];
                if (top && top._debug_combo_base) {
                    cases.push({
                        item_names: top.item_names,
                        base_sp: [...top.base_sp],
                        total_sp: [...top.total_sp],
                        assigned_sp: top.assigned_sp,
                        expected_damage: top.score,
                        combo_base: _jser(top._debug_combo_base),
                    });
                } else if (!top) {
                    doneNoTop++;
                    if (doneNoTop === 1 && process.env.SOLVER_EXPORT_SCORE_DEBUG) {
                        console.log('  [score-export] first no-top done msg:',
                            JSON.stringify({ ...msg, top5: msg.top5?.length, trace: undefined }));
                    }
                } else {
                    doneNoDebug++;
                }
                sendNext();
            } else if (msg.type === 'worker_error') {
                worker.terminate();
                reject(new Error(msg.message));
            }
        });
        worker.on('error', (err) => { worker.terminate(); reject(err); });

        sendNext();
    });
}

function buildEnumFixture({ initMsgBase, ringPoolSer, solverSnap, env }) {
    const L = [];
    const f = (x) => (typeof x === 'number' && Number.isFinite(x)) ? String(x) : '0';

    const H = env.evalInCtx(`({
        sp100: skillPointsToPercentage(100),
        fm3: skillpoint_final_mult[3], fm4: skillpoint_final_mult[4],
        static_ids: [...STATMAP_STATIC_IDS],
        indirect: [...INDIRECT_CONSTRAINT_STATS],
        skp_order: [...skp_order],
        classDefFor: (t) => classDefenseMultipliers.get(t) || 1.0,
    })`);
    const STATIC_SET = new Set(H.static_ids);
    const EXCLUDED = new Set([...H.indirect, 'str', 'dex', 'int', 'def', 'agi']);

    // ── Mirror the worker's partial[] and fixed running statmap ──
    const NONE = (sm) => !sm || sm.has('NONE');
    const lk = initMsgBase.locked ?? {};
    const partial = {
        helmet: lk.helmet?.statMap, chestplate: lk.chestplate?.statMap,
        leggings: lk.leggings?.statMap, boots: lk.boots?.statMap,
        ring1: initMsgBase.ring1_locked?.statMap, ring2: initMsgBase.ring2_locked?.statMap,
        bracelet: lk.bracelet?.statMap, necklace: lk.necklace?.statMap,
    };
    const fixed_sms = [];
    for (const sm of Object.values(partial)) if (!NONE(sm)) fixed_sms.push(sm);
    for (const t of initMsgBase.tome_sms) fixed_sms.push(t);
    fixed_sms.push(initMsgBase.weapon_sm);
    const running0 = env.ctx._init_running_statmap(initMsgBase.level, fixed_sms);

    // ── Prechecks (mirror _build_constraint_prechecks) ──
    const fixedContrib = (stat) =>
        (solverSnap.atree_raw?.get(stat) ?? 0) + (solverSnap.static_boosts?.get(stat) ?? 0);
    const thresholds = solverSnap.restrictions?.stat_thresholds ?? [];
    const pcs = [];
    let ehp = null, ehpna = null, thp = null;
    for (const { stat, op, value } of thresholds) {
        if (op !== 'ge') continue;
        if (stat === 'ehp' || stat === 'ehp_no_agi') {
            const fixed_hp = fixedContrib('hpBonus');
            const def_pct = H.sp100 * H.fm3;
            const defMult = 2 - H.classDefFor(initMsgBase.weapon_sm.get('type'));
            if (stat === 'ehp') {
                const agi_pct = H.sp100 * H.fm4;
                const agi_reduction = (100 - 90) / 100;
                ehp = { threshold: value, fixed_hp, divisor: (agi_reduction * agi_pct + (1 - agi_pct) * (1 - def_pct)) * defMult };
            } else {
                ehpna = { threshold: value, fixed_hp, divisor: (1 - def_pct) * defMult };
            }
            continue;
        }
        if (stat === 'total_hp') { thp = { threshold: value, fixed_hp: fixedContrib('hpBonus') }; continue; }
        if (EXCLUDED.has(stat)) continue;
        pcs.push({ stat, adjusted_threshold: value - fixedContrib(stat), start: running0.get(stat) ?? 0 });
    }

    // ── Set / illegal-set id tables ──
    const setIds = new Map();
    const illegalIds = new Map();
    const setId = (name) => {
        if (!name) return -1;
        if (!setIds.has(name)) setIds.set(name, setIds.size);
        return setIds.get(name);
    };
    const illegalId = (name) => {
        if (!name) return -1;
        if (!illegalIds.has(name)) illegalIds.set(name, illegalIds.size);
        return illegalIds.get(name);
    };

    const itemLine = (it) => {
        const sm = it.statMap;
        const reqs = sm.get('reqs'), skp = sm.get('skillpoints');
        const maxRolls = sm.get('maxRolls');
        const sVal = (stat) => STATIC_SET.has(stat) ? (sm.get(stat) || 0) : (maxRolls?.get(stat) || 0);
        const hp = (sm.get('hp') || 0) + (maxRolls?.get('hpBonus') || 0);
        return ['ITEM', sm.get('crafted') ? 1 : 0, ...reqs, ...skp,
            setId(sm.get('crafted') ? null : sm.get('set')), illegalId(it._illegalSet),
            f(hp), ...pcs.map(pc => f(sVal(pc.stat)))].join(' ');
    };

    // ── Free slots in the worker's enumeration order ──
    const pools = initMsgBase.pools ?? {};
    const getPool = (slot) => (slot === 'ring1' || slot === 'ring2') ? ringPoolSer : pools[slot];
    const free_slots = [];
    for (const slot of ['helmet', 'chestplate', 'leggings', 'boots', 'bracelet', 'necklace']) {
        if (!lk[slot]) free_slots.push(slot);
    }
    if (!initMsgBase.ring1_locked) free_slots.push('ring1');
    if (!initMsgBase.ring2_locked) free_slots.push('ring2');
    free_slots.sort((a, b) => {
        const diff = (getPool(a)?.length ?? 0) - (getPool(b)?.length ?? 0);
        if (diff !== 0) return diff;
        if (a === 'ring1' && b === 'ring2') return -1;
        if (a === 'ring2' && b === 'ring1') return 1;
        return 0;
    });
    const SLOT_POS = { helmet: 0, chestplate: 1, leggings: 2, boots: 3, ring1: 4, ring2: 5, bracelet: 6, necklace: 7 };

    // ── Emit ──
    L.push(`BUDGET ${initMsgBase.sp_budget}`);
    L.push(`PRECHECKS ${pcs.length}`);
    for (const pc of pcs) L.push(`PC ${pc.stat} ${f(pc.adjusted_threshold)} ${f(pc.start)}`);
    L.push(`EHP ${ehp ? 1 : 0} ${f(ehp?.threshold)} ${f(ehp?.fixed_hp)} ${f(ehp?.divisor)}`);
    L.push(`EHPNA ${ehpna ? 1 : 0} ${f(ehpna?.threshold)} ${f(ehpna?.fixed_hp)} ${f(ehpna?.divisor)}`);
    L.push(`THP ${thp ? 1 : 0} ${f(thp?.threshold)} ${f(thp?.fixed_hp)}`);
    L.push(`HPSTART ${f((running0.get('hp') ?? 0) + (running0.get('hpBonus') ?? 0))}`);
    const wep = initMsgBase.weapon_sm;
    L.push(`WEAPON ${wep.get('reqs').join(' ')} ${wep.get('skillpoints').join(' ')}`);
    const gt = initMsgBase.guild_tome_sm;
    const gtPresent = gt && !gt.has('NONE');
    L.push(`GUILD ${gtPresent ? 1 : 0} ${gtPresent && gt.get('crafted') ? 1 : 0} `
        + `${(gtPresent ? gt.get('reqs') : [0,0,0,0,0]).join(' ')} `
        + `${(gtPresent ? gt.get('skillpoints') : [0,0,0,0,0]).join(' ')} `
        + `${gtPresent ? setId(gt.get('set')) : -1}`);

    // Fixed equipment (non-NONE locked slots).
    const fixedLines = [];
    for (const [slot, sm] of Object.entries(partial)) {
        if (NONE(sm)) continue;
        const wrapper = (slot === 'ring1') ? initMsgBase.ring1_locked
            : (slot === 'ring2') ? initMsgBase.ring2_locked : lk[slot];
        fixedLines.push(`FIXED ${SLOT_POS[slot]} ${sm.get('crafted') ? 1 : 0} `
            + `${sm.get('reqs').join(' ')} ${sm.get('skillpoints').join(' ')} `
            + `${setId(sm.get('crafted') ? null : sm.get('set'))} ${illegalId(wrapper?._illegalSet)}`);
    }
    L.push(`NFIXED ${fixedLines.length}`);
    L.push(...fixedLines);

    // Free slots + pools (items reference set/illegal ids, so emit before SETS).
    L.push(`NSLOTS ${free_slots.length}`);
    const slotLines = [];
    for (const slot of free_slots) {
        const pool = getPool(slot) ?? [];
        slotLines.push(`SLOT ${slot} ${SLOT_POS[slot]} ${slot === 'ring1' ? 1 : 0} ${slot === 'ring2' ? 1 : 0} ${pool.length}`);
        for (const it of pool) slotLines.push(itemLine(it));
    }
    L.push(...slotLines);

    // Set-bonus SP table (ids assigned while emitting items above).
    const setLines = [];
    for (const [name, id] of setIds) {
        const bonuses = env.ctx.sets.get(name)?.bonuses ?? [];
        const rows = bonuses.map(b => H.skp_order.map(k => (b?.[k] || 0)).join(' '));
        setLines.push(`SET ${id} ${rows.length} ${rows.join('  ')}`);
    }
    L.push(`NSETS ${setIds.size}`);
    L.push(...setLines);

    // Item display names (optional trailing section) — lets the Rust scoring
    // integration join pool/fixed items to the score fixture's item registry.
    // Names are raw line remainders (they contain spaces).
    const nameOf = (it) => {
        const sm = it?.statMap ?? it;
        return sm?.get?.('displayName') ?? sm?.get?.('name') ?? '';
    };
    L.push('NAMES 1');
    for (let si = 0; si < free_slots.length; si++) {
        const pool = getPool(free_slots[si]) ?? [];
        L.push(`INAMES ${si} ${pool.length}`);
        for (const it of pool) L.push(nameOf(it));
    }
    const fixedNameLines = [];
    for (const [slot, sm] of Object.entries(partial)) {
        if (NONE(sm)) continue;
        fixedNameLines.push(`${SLOT_POS[slot]} ${sm.get('displayName') ?? sm.get('name') ?? ''}`);
    }
    L.push(`FNAMES ${fixedNameLines.length}`);
    L.push(...fixedNameLines);
    // All-8 none-item names by slot position (for slots neither free nor fixed).
    L.push('NONENAMES 8');
    for (let p = 0; p < 8; p++) {
        const noneSm = initMsgBase.none_item_sms[p];
        L.push(noneSm?.get?.('displayName') ?? noneSm?.get?.('name') ?? '');
    }

    return L.join('\n') + '\n';
    console.log(`  [export] Rust fixture written to ${outPath}`);
}

/// Default env for the browser, where the game functions are globals.
function browserEnv(scope) {
    const g = scope || (typeof globalThis !== 'undefined' ? globalThis : {});
    return { ctx: g, evalInCtx: (src) => (0, eval)(src) };
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = { buildScoreFixture, buildEnumFixture, browserEnv, _jser };
}
