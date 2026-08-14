// ══════════════════════════════════════════════════════════════════════════════
// SOLVER SEARCH TESTS
// Runs the full solver pipeline headlessly: pool building, sensitivity weights,
// dominance pruning, priority sorting, partitioning, and parallel worker
// enumeration via worker_threads.  Asserts the solver finds builds scoring
// >= a known threshold within a time limit.
//
// Run: node js/solver/tests/test_solver_search.js [filter1] [filter2] ...
// With no args, runs all solver_*.snap.json snapshots.
// With args, runs only snapshots whose names contain any of the given strings.
// Requires Node.js >= 18.
// ══════════════════════════════════════════════════════════════════════════════

'use strict';

const { Worker } = require('worker_threads');
const {
    createSandbox, loadGameData, decodeSolverUrl, decodeActiveNodes,
    buildAtreeMerged, collectSpells, collectRawStats, extractAtreeInteractiveDefaults,
    TestRunner, loadSnapshot, saveSnapshot, snapshotNeedsGeneration,
    checkSnapshotFreshness, extractLockedItemStats,
    REPO_ROOT,
} = require('./harness');
const fs = require('fs');
const path = require('path');
const vm = require('vm');
const os = require('os');
const _rust_bridge = require('../engine/rust_bridge.js');
const {
    mergeTraceMetrics, summarizeTraceMetrics, reconcileTraceMetrics,
} = require('../benchmarks/trace_metrics');

// ── Setup ────────────────────────────────────────────────────────────────────

const ctx = createSandbox();
loadGameData(ctx);
const t = new TestRunner('Solver Search');

// Inject constants that search.js needs from other files (const/let scoped).
vm.runInContext(`
    // _NONE_ITEM_IDX is defined in solver/graph/build.js — inject it here
    // since we don't load that file (it has graph node dependencies).
    var _NONE_ITEM_IDX = {
        helmet: 0, chestplate: 1, leggings: 2, boots: 3,
        ring1: 4, ring2: 5, bracelet: 6, necklace: 7, weapon: 8,
    };
`, ctx);

// Load combo/boost.js, combo/codec.js, and combo/simulate.js for boost registry,
// name resolution, and health config extraction.
for (const relPath of ['js/solver/combo/boost.js', 'js/solver/combo/codec.js', 'js/solver/combo/simulate.js']) {
    const absPath = path.join(REPO_ROOT, relPath);
    vm.runInContext(fs.readFileSync(absPath, 'utf8'), ctx, { filename: absPath });
}

// Load search.js for the pool-building / weight / prune / priority pipeline.
const searchPath = path.join(REPO_ROOT, 'js', 'solver', 'engine', 'search.js');
vm.runInContext(fs.readFileSync(searchPath, 'utf8'), ctx, { filename: searchPath });

// Export let/const vars from search.js we need.
vm.runInContext(`
    globalThis._build_item_pools = _build_item_pools;
    globalThis._prepare_tome_optimisation = typeof _prepare_tome_optimisation !== 'undefined' ? _prepare_tome_optimisation : null;
    // Page global read by _prepare_tome_optimisation; empty array = no tome
    // equipped in any slot, so every tome slot is optimisable.
    globalThis.solver_item_final_nodes = [];
    globalThis._serialize_pools = _serialize_pools;
    globalThis._serialize_locked = _serialize_locked;
    globalThis._build_worker_init_msg = _build_worker_init_msg;
    globalThis._apply_roll_mode_to_item = _apply_roll_mode_to_item;
    globalThis._partition_work = _partition_work;
    globalThis._normalize_dominance_mode = _normalize_dominance_mode;
    globalThis._tag_illegal_set_item = _tag_illegal_set_item;
    globalThis._has_illegal_set_conflict = _has_illegal_set_conflict;
    globalThis._eval_current_build = _eval_current_build;
    globalThis._TOP_N = typeof _TOP_N !== 'undefined' ? _TOP_N : 15;
    globalThis.build_combo_boost_registry = typeof build_combo_boost_registry !== 'undefined' ? build_combo_boost_registry : null;
    globalThis.node_ref_to_boost_name = typeof node_ref_to_boost_name !== 'undefined' ? node_ref_to_boost_name : null;
    globalThis.node_id_to_spell_value = typeof node_id_to_spell_value !== 'undefined' ? node_id_to_spell_value : null;
    globalThis.MELEE_TIME_NODE_ID = MELEE_TIME_NODE_ID;
    globalThis.extract_health_config = typeof extract_health_config !== 'undefined' ? extract_health_config : null;
    globalThis.compute_recast_penalties = typeof compute_recast_penalties !== 'undefined' ? compute_recast_penalties : null;
    globalThis.compute_dps_spell_hits_info = typeof compute_dps_spell_hits_info !== 'undefined' ? compute_dps_spell_hits_info : null;
`, ctx);

// ── Slot constants ───────────────────────────────────────────────────────────

const SLOT_NAMES = ['helmet', 'chestplate', 'leggings', 'boots', 'ring1', 'ring2', 'bracelet', 'necklace'];
const NONE_IDX = { helmet: 0, chestplate: 1, leggings: 2, boots: 3, ring1: 4, ring2: 5, bracelet: 6, necklace: 7 };
const WORKER_THREAD_PATH = path.join(__dirname, 'worker_thread.js');

// ── Powder parsing helper ────────────────────────────────────────────────────

/**
 * Convert decoded powder data to an array of integer powder IDs.
 * Handles both formats:
 *   - Modern (array of ints): already correct, return as-is
 *   - Legacy (string like "f7f7f7"): parse 2-char codes via powderIDs map
 */
function parsePowderData(raw) {
    if (Array.isArray(raw)) return raw;
    if (typeof raw !== 'string' || raw === '') return [];
    const ids = [];
    for (let i = 0; i + 1 < raw.length; i += 2) {
        const code = raw.substring(i, i + 2);
        const pid = ctx.powderIDs.get(code);
        if (pid !== undefined) ids.push(pid);
    }
    return ids;
}

// ── Build solver snapshot from decoded URL ───────────────────────────────────

function buildTestSnapshot(decoded, snap, spellMap, atreeMerged, rawStats) {
    const sp = decoded.solverParams || {};

    // ── 1. Weapon with powders ──────────────────────────────────────────────
    // Apply roll mode to weapon (expandItem only creates minRolls/maxRolls;
    // _apply_roll_mode_to_item selects actual values for the top-level statMap).
    const weaponName = snap.weapon_override ?? decoded.equipment[8];
    const weaponItem = ctx.itemMap.get(weaponName);
    const weaponIt = ctx._apply_roll_mode_to_item(new ctx.Item(weaponItem));
    const weaponSM = weaponIt.statMap;
    // Apply weapon powders from decoded URL (powderables map: slot 8 → index 4)
    const weaponPowders = parsePowderData(decoded.powders && decoded.powders[4]);
    weaponSM.set('powders', weaponPowders);
    ctx.apply_weapon_powders(weaponSM);

    // ── 2. Tomes ────────────────────────────────────────────────────────────
    // tome_fields has 14 entries (some types have 2+ slots). Map each slot
    // to its corresponding none_tome by type.
    const tomeNames = decoded.tomes || [];
    const _tome_fields = ctx.tome_fields ?? vm.runInContext('tome_fields', ctx);
    const _none_tome_by_type = {};
    for (let i = 0; i < ctx.none_tomes.length; i++)
        _none_tome_by_type[ctx.none_tomes[i].type] = ctx.none_tomes[i];

    const tomes = [];
    for (let i = 0; i < _tome_fields.length; i++) {
        const name = tomeNames[i];
        const type = _tome_fields[i].replace(/[0-9]/g, '');
        const none = _none_tome_by_type[type] ?? ctx.none_tomes[0];
        const tome = (name && ctx.tomeMap.has(name)) ? ctx.tomeMap.get(name) : none;
        const tomeIt = ctx._apply_roll_mode_to_item(new ctx.Item(tome));
        tomes.push({ statMap: tomeIt.statMap });
    }

    // ── 3. SP budget with guild tome handling (mirrors search.js:210-224) ───
    // Note: _apply_roll_mode_to_item does not set the 'NONE' flag — that flag
    // is set by
    // the graph node system at runtime.  Detect NONE guild tomes by checking
    // whether a real tome name was decoded from the URL.
    // Guild tome is at tome_fields index 6 ('guildTome1').
    const GUILD_TOME_IDX = _tome_fields.indexOf('guildTome1');
    const guildTomeName = tomeNames[GUILD_TOME_IDX];
    const has_real_guild_tome = !!(guildTomeName && ctx.tomeMap.has(guildTomeName));
    let sp_budget = ctx.levelToSkillPoints(decoded.level);
    if (!has_real_guild_tome) {
        const gtome_mode = sp.gtome ?? 0;
        if (gtome_mode === 1) {
            // Standard: +4 freely assignable SP
            sp_budget = ctx.levelToSkillPoints(decoded.level) + 4;
        } else if (gtome_mode === 2) {
            // Rainbow: fixed [1,1,1,1,1] synthetic tome
            const synth = new Map();
            synth.set('skillpoints', [1, 1, 1, 1, 1]);
            synth.set('reqs', [0, 0, 0, 0, 0]);
            tomes[GUILD_TOME_IDX] = { statMap: synth };
        }
    }

    // ── 4. Atree raw stats ──────────────────────────────────────────────────
    const atreeRaw = new Map();
    for (const [stat, value] of rawStats) ctx.merge_stat(atreeRaw, stat, value);

    // Button/slider states — use snapshot overrides if present, else atree defaults.
    // After combo parsing, we infer slider/button states from combo boost tokens
    // (the URL doesn't encode atree interactive state, but the combo rows reflect it).
    const atreeDefaults = extractAtreeInteractiveDefaults(atreeMerged);
    const buttonStates = snap.button_states
        ? new Map(Object.entries(snap.button_states))
        : new Map(atreeDefaults.button_states);
    const sliderStates = snap.slider_states
        ? new Map(Object.entries(snap.slider_states))
        : new Map(atreeDefaults.slider_states);

    // Static boosts — in the real app these come from compute_boosts() (potion
    // toggle buttons on the page).  No potions are active for headless tests,
    // so static_boosts is empty.
    const staticBoosts = new Map();

    // ── 5. Augmented spell map with powder specials (mirrors search.js)
    const augSpellMap = new Map(spellMap);
    const weaponSpecial = ctx.get_powder_special(weaponPowders);
    if (weaponSpecial && [0, 1, 3].includes(weaponSpecial.ps_idx)) {
        augSpellMap.set(-1000 - weaponSpecial.ps_idx,
            ctx.make_powder_special_spell(weaponSpecial.ps_idx, weaponSpecial.tier));
    }
    ctx.apply_deferred_powder_special_effects(augSpellMap, spellMap);

    // ── 6. Boost registry with weapon + armor powders ───────────────────────
    // Create minimal mock build so build_combo_boost_registry includes powder
    // buff entries (weapon buffs + armor sliders).
    const mockEquip = [];
    for (let i = 0; i < 4; i++) {
        const sm = new Map();
        sm.set('powders', parsePowderData(decoded.powders && decoded.powders[i]));
        mockEquip.push({ statMap: sm });
    }
    const mockBuild = { weapon: { statMap: weaponSM }, equipment: mockEquip };
    const boostRegistry = ctx.build_combo_boost_registry
        ? ctx.build_combo_boost_registry(atreeMerged, mockBuild)
        : [];

    // ── 7. Health config / Blood Pact ───────────────────────────────────────
    const health_config = ctx.extract_health_config
        ? ctx.extract_health_config(atreeMerged)
        : null;
    const hp_casting = health_config?.hp_casting ?? false;

    // ── 8. Parse combo rows (powder specials + pseudo-spells) ───────────────
    const parsedCombo = [];
    for (const row of (sp.combo_rows || [])) {
        const node_id = row.spell_node_id;

        // Pseudo-spell: Mana Reset
        if (node_id === ctx.MANA_RESET_NODE_ID) {
            parsedCombo.push({ pseudo: 'mana_reset', mana_excl: row.mana_excl });
            continue;
        }

        // Pseudo-spell: Cancel state (e.g. Cancel Corrupted)
        let cancel_pseudo = null;
        if (ctx.STATE_CANCEL_NODE_IDS) {
            for (const [state_name, cancel_node_id] of ctx.STATE_CANCEL_NODE_IDS) {
                if (node_id === cancel_node_id) {
                    cancel_pseudo = 'cancel_state:' + state_name;
                    break;
                }
            }
        }
        if (cancel_pseudo) {
            parsedCombo.push({ pseudo: cancel_pseudo, mana_excl: row.mana_excl });
            continue;
        }

        // Pseudo-spell: Add Flat Mana (adjusts mana budget directly)
        const ADD_FLAT_MANA_NID = vm.runInContext('ADD_FLAT_MANA_NODE_ID', ctx);
        if (node_id === ADD_FLAT_MANA_NID) {
            parsedCombo.push({ pseudo: 'add_flat_mana', qty: row.qty, mana_excl: row.mana_excl });
            continue;
        }

        // Melee Time pseudo-spell: resolve to the melee spell (key 0) and flag
        // is_melee_time so downstream qty computation uses time-based hits.
        const is_melee_time = (node_id === ctx.MELEE_TIME_NODE_ID);

        // Regular spell or powder special: resolve to spell map key
        let spell;
        if (is_melee_time) {
            spell = augSpellMap.get(0) ?? null;
        } else {
            const spell_value_str = ctx.node_id_to_spell_value
                ? ctx.node_id_to_spell_value(node_id)
                : String(node_id);
            const spell_key = parseInt(spell_value_str);
            spell = augSpellMap.get(spell_key);
        }
        if (!spell) continue;

        const entry = {
            qty: row.qty,
            sim_qty: Math.round(row.qty),
            spell,
            is_melee_time,
            boost_tokens: (row.boosts || []).map(b => ({
                name: ctx.node_ref_to_boost_name
                    ? ctx.node_ref_to_boost_name(b.node_id, b.effect_pos, atreeMerged)
                    : `node_${b.node_id}_${b.effect_pos}`,
                value: b.has_value ? b.value : 1,
                is_pct: false,
            })),
            mana_excl: row.mana_excl,
            dmg_excl: row.dmg_excl,
            cast_time: row.cast_time,
            delay: row.delay,
        };

        // DPS spell info
        if (ctx.compute_dps_spell_hits_info) {
            const dps_info = ctx.compute_dps_spell_hits_info(spell);
            if (dps_info) {
                entry.dps_per_hit_name = dps_info.per_hit_name;
                entry.dps_hits = row.has_hits ? row.hits : dps_info.max_hits;
            }
        }

        parsedCombo.push(entry);
    }

    // ── 8b. Atree interactive state ─────────────────────────────────────────
    // The URL hash doesn't encode atree interactive state (button/slider).
    // We leave button_states and slider_states at defaults.  Toggle stat
    // bonuses (e.g. spPctXFinal from "Activate Dimensional Tear") are
    // applied per-row by combo boost tokens — NOT globally — to avoid the
    // double-counting bug documented in TOGGLE_DOUBLE_COUNT_BUG.md.
    // Snapshots can override via explicit button_states / slider_states fields.

    // ── 9. Recast penalties (shared pure function) ──────────────────────────
    if (ctx.compute_recast_penalties) {
        ctx.compute_recast_penalties(parsedCombo);
    }

    // ── 10. Auto-slider stripping (mirrors search.js:300-315) ───────────────
    const auto_slider_names_set = new Set();
    if (hp_casting && health_config) {
        if (health_config.damage_boost?.slider_name)
            auto_slider_names_set.add(health_config.damage_boost.slider_name);
        for (const bs of (health_config.buff_states || []))
            if (bs.slider_name) auto_slider_names_set.add(bs.slider_name);
    }
    if (auto_slider_names_set.size > 0) {
        for (const row of parsedCombo) {
            if (row.boost_tokens) {
                row.boost_tokens = row.boost_tokens.filter(t => t.manual || !auto_slider_names_set.has(t.name));
            }
        }
    }

    // ── 11. Restrictions ────────────────────────────────────────────────────
    const rStats = vm.runInContext('RESTRICTION_STATS', ctx);
    const restrictions = {
        stat_thresholds: (sp.restrictions || []).map(r => ({
            stat: rStats?.[r.stat_index]?.key ?? 'unknown',
            op: r.op === 0 ? 'ge' : 'le',
            value: r.value,
        })),
    };
    // Snapshot-level extra thresholds (oracle fixtures exercise restriction
    // pruning without needing a new URL hash).
    if (snap.extra_restrictions) {
        restrictions.stat_thresholds.push(...snap.extra_restrictions);
    }

    // ── 12. Spell base costs ────────────────────────────────────────────────
    const spellBaseCosts = {};
    for (const [id, spell] of augSpellMap) {
        if (typeof id === 'number' && id >= 1 && id <= 4 && spell.cost != null) {
            spellBaseCosts[id] = spell.cost;
        }
    }
    // Prefer costs from parsed combo rows (user's active spells)
    for (const row of parsedCombo) {
        if (row.spell?.base_spell >= 1 && row.spell?.base_spell <= 4 && row.spell.cost != null) {
            spellBaseCosts[row.spell.base_spell] = row.spell.cost;
        }
    }

    // ── 13. Combo cycle time ─────────────────────────────────────────────────
    // atkTier is a rolled ID — read from maxRolls rather than the item statMap directly.
    const weapon_cycle_sm = new Map([
        ['atkSpd', weaponSM.get('atkSpd')],
        ['atkTier', weaponSM.get('maxRolls')?.get('atkTier') ?? 0]
    ]);
    const combo_time = sp.mana_disabled ? 0 : ctx.compute_combo_cycle_time(
        ctx._unroll_loops_pure ? ctx._unroll_loops_pure(parsedCombo, {}) : parsedCombo, weapon_cycle_sm);

    return {
        weapon: { statMap: weaponSM },
        weapon_sm: weaponSM,
        level: decoded.level,
        tomes,
        guild_tome_item: { statMap: tomes[GUILD_TOME_IDX].statMap },
        sp_budget,
        atree_mgd: atreeMerged,
        atree_raw: atreeRaw,
        button_states: buttonStates,
        slider_states: sliderStates,
        radiance_boost: 1.0,
        static_boosts: staticBoosts,
        parsed_combo: parsedCombo,
        boost_registry: boostRegistry,
        scoring_target: snap.scoring_target || 'combo_damage',
        combo_time,
        allow_downtime: sp.dtime || false,
        hp_casting,
        health_config,
        auto_slider_names: [...auto_slider_names_set],
        spell_base_costs: spellBaseCosts,
        restrictions,
        // Pool building restrictions
        lvl_min: snap.lvl_min ?? sp.lvl_min ?? 1,
        lvl_overrides: snap.lvl_overrides ?? sp.lvl_overrides ?? {},
        lvl_max: snap.lvl_max ?? sp.lvl_max ?? decoded.level,
        no_major_id: sp.nomaj || false,
        dir_enabled: sp.dir_enabled ?? 0x1F,
    };
}

// Partition work uses the real _partition_work() from search.js.

// ── Run solver with worker_threads ───────────────────────────────────────────

function runSolverWorkers(initMsgBase, ringPoolSer, partitions, numWorkers, timeLimitMs, targetScore, seedResult = null) {
    return new Promise((resolve) => {
        const workers = [];
        const allTop = seedResult ? [seedResult] : [];
        const progressTop = [];
        const progressCounts = {};
        let totalChecked = 0, totalFeasible = 0;
        let totalTrace = {};
        let partitionIdx = 0;
        let doneCount = 0;
        let timedOut = false;
        let workerError = null;

        // ── Shared global top-N cutoff ─────────────────────────────────────
        // The 15th-best distinct score reported by any worker (or the seed) is
        // a lower bound on the final merged top-N cutoff: every reported build
        // either survives to the final merge or was displaced by 15 higher-
        // scoring builds from its own partition. Workers read it mid-partition
        // through a SharedArrayBuffer (Int32, floored — flooring only lowers
        // the cutoff, which keeps it admissible for the ceiling gate).
        const CUTOFF_SENTINEL = -0x80000000;
        const cutoffSab = typeof SharedArrayBuffer !== 'undefined' ? new SharedArrayBuffer(4) : null;
        const cutoffView = cutoffSab ? new Int32Array(cutoffSab) : null;
        if (cutoffView) Atomics.store(cutoffView, 0, CUTOFF_SENTINEL);
        const cutoffScores = new Map();  // item_names key -> best score seen
        let lastCutoff = -Infinity;
        if (seedResult?.item_names) {
            cutoffScores.set(seedResult.item_names.join('\0'), seedResult.score);
        }
        function updateCutoff(entries) {
            if (!entries) return;
            for (const r of entries) {
                if (!r || typeof r.score !== 'number' || !r.item_names) continue;
                const key = r.item_names.join('\0');
                const prev = cutoffScores.get(key);
                if (prev === undefined || r.score > prev) cutoffScores.set(key, r.score);
            }
            if (cutoffScores.size < 15) return;
            const scores = [...cutoffScores.values()].sort((a, b) => b - a);
            const cutoff = scores[14];
            if (cutoff <= lastCutoff) return;
            lastCutoff = cutoff;
            if (cutoffView) {
                const floored = Math.max(CUTOFF_SENTINEL + 1, Math.min(0x7FFFFFFF, Math.floor(cutoff)));
                Atomics.store(cutoffView, 0, floored);
            }
            for (const w of workers) {
                try { w.postMessage({ type: 'cutoff', value: cutoff }); } catch (e) {}
            }
        }

        function dispatchNext(worker, workerId) {
            if (timedOut || partitionIdx >= partitions.length) return false;
            const partition = partitions[partitionIdx++];
            try {
                if (workerId === -1) {
                    // First init message
                    const msg = { ...initMsgBase, partition, worker_id: worker._workerId, cutoff_sab: cutoffSab };
                    worker.postMessage(msg);
                } else {
                    worker.postMessage({ type: 'run', partition, worker_id: workerId });
                }
            } catch (err) {
                console.error(`  [dispatch] postMessage failed for worker ${worker._workerId}:`, err.message);
                return false;
            }
            return true;
        }

        function onProgress(msg) {
            // Accumulate running checked/feasible counts from progress updates.
            // Track per-worker latest counts to avoid double-counting.
            if (!progressCounts[msg.worker_id]) progressCounts[msg.worker_id] = { checked: 0, feasible: 0 };
            progressCounts[msg.worker_id].checked = msg.checked || 0;
            progressCounts[msg.worker_id].feasible = msg.feasible || 0;
            progressCounts[msg.worker_id].trace = msg.trace || null;
            // Collect top5 from progress (when top5 changes, worker sends top5_names).
            if (msg.top5_names) {
                for (const entry of msg.top5_names) {
                    progressTop.push(entry);
                }
                updateCutoff(msg.top5_names);
                // Early termination: if any worker found a build meeting the target,
                // stop all workers immediately instead of waiting for the full timeout.
                if (targetScore != null && !timedOut) {
                    const best = msg.top5_names[0];
                    if (best && best.score >= targetScore) {
                        timedOut = true;
                        cleanup();
                    }
                }
            }
        }

        function onDone(msg) {
            totalChecked += msg.checked || 0;
            totalFeasible += msg.feasible || 0;
            if (msg.top5) allTop.push(...msg.top5);
            updateCutoff(msg.top5);
            totalTrace = mergeTraceMetrics(totalTrace, msg.trace);
            // Clear progress counts for this worker (done supersedes progress).
            delete progressCounts[msg.worker_id];

            doneCount++;
            // Dispatch next partition to this worker
            const w = workers.find(w => w._workerId === msg.worker_id);
            if (w && !timedOut) {
                if (!dispatchNext(w, msg.worker_id)) {
                    // No more work for this worker
                }
            }

            // Check if all workers are idle
            if (doneCount >= partitions.length || timedOut) {
                cleanup();
            }
        }

        function cleanup() {
            clearTimeout(timer);
            for (const w of workers) {
                try { w.terminate(); } catch (e) {}
            }
            // Add progress counts from workers that didn't finish (timed out).
            for (const wid in progressCounts) {
                totalChecked += progressCounts[wid].checked;
                totalFeasible += progressCounts[wid].feasible;
                totalTrace = mergeTraceMetrics(totalTrace, progressCounts[wid].trace);
            }
            // Merge top results from done messages + progress messages.
            const merged = [...allTop, ...progressTop];
            merged.sort((a, b) => (b.score || 0) - (a.score || 0));
            resolve({
                top5: merged.slice(0, 15),
                checked: totalChecked,
                feasible: totalFeasible,
                timedOut,
                trace: totalTrace,
                workerError,
            });
        }

        // Time limit
        const timer = setTimeout(() => {
            timedOut = true;
            for (const w of workers) {
                try { w.postMessage({ type: 'cancel' }); } catch (e) {}
            }
            // Give workers a moment to finish current work
            setTimeout(cleanup, 500);
        }, timeLimitMs);

        // Spawn workers
        const actualWorkers = Math.min(numWorkers, partitions.length);
        for (let i = 0; i < actualWorkers; i++) {
            const w = new Worker(WORKER_THREAD_PATH, {
                workerData: { repoRoot: REPO_ROOT },
            });
            w._workerId = i;
            w.on('message', (msg) => {
                if (msg.type === 'done') onDone(msg);
                else if (msg.type === 'progress') onProgress(msg);
                else if (msg.type === 'worker_error') {
                    workerError = msg.message;
                    console.error(`  Worker ${i} error:`, msg.message);
                }
            });
            w.on('error', (err) => {
                console.error(`  Worker ${i} error:`, err.message);
                console.error(err.stack);
            });
            workers.push(w);

            // Send init message with first partition
            dispatchNext(w, -1);
        }
    });
}

// ── Exhaustive Cartesian oracle (P0.4) ───────────────────────────────────────
//
// Re-enumerates the entire canonical tuple space with plain nested loops
// (rings as unordered pairs, ring2 index >= ring1 index) and evaluates each
// tuple through a worker whose slots are ALL locked (N_free = 0), completely
// bypassing the production level enumeration, mid-tree pruning, partitioning,
// and top-N cutoff. The production search must reproduce the oracle's
// checked count, feasible count, and top-N scores exactly.

const ORACLE_ARMOR_SLOTS = ['helmet', 'chestplate', 'leggings', 'boots', 'bracelet', 'necklace'];

/** Replicates _make_illegal_tracker semantics over a full equipment set:
 *  at most one item per illegal set across all eight slots. */
function _oracleTupleBlocked(items) {
    const seen = new Set();
    for (const it of items) {
        const is = it?._illegalSet;
        if (!is) continue;
        if (seen.has(is)) return true;
        seen.add(is);
    }
    return false;
}

function runOracleEnumeration(initMsgBase, ringPoolSer) {
    return new Promise((resolve, reject) => {
        // Enumerate canonical tuples: Cartesian product of free armor slots,
        // times canonical ring pairs (i <= j) when both rings are free.
        const freeSlots = ORACLE_ARMOR_SLOTS.filter(s => initMsgBase.pools[s]?.length);
        const ring1Free = !initMsgBase.ring1_locked && ringPoolSer.length > 0;
        const ring2Free = !initMsgBase.ring2_locked && ringPoolSer.length > 0;

        // ring1_locked/ring2_locked alias entries already present in `locked`
        // for this harness. Deduplicate by wrapper identity so an exclusive
        // locked ring is not mistaken for two copies by the oracle itself.
        const lockedItems = [...new Set([
            ...Object.values(initMsgBase.locked ?? {}),
            initMsgBase.ring1_locked, initMsgBase.ring2_locked,
        ].filter(Boolean)),
            {
                statMap: initMsgBase.weapon_sm,
                _illegalSet: initMsgBase.weapon_illegal_set ?? null,
                _illegalSetName: initMsgBase.weapon_illegal_set_name ?? null,
            },
        ];

        const tuples = [];
        const build = (slotIdx, chosen) => {
            if (slotIdx === freeSlots.length) {
                if (ring1Free && ring2Free) {
                    for (let i = 0; i < ringPoolSer.length; i++) {
                        for (let j = i; j < ringPoolSer.length; j++) {
                            tuples.push({ armor: chosen.slice(), ring1: i, ring2: j });
                        }
                    }
                } else if (ring1Free) {
                    for (let i = 0; i < ringPoolSer.length; i++) {
                        tuples.push({ armor: chosen.slice(), ring1: i, ring2: -1 });
                    }
                } else if (ring2Free) {
                    for (let j = 0; j < ringPoolSer.length; j++) {
                        tuples.push({ armor: chosen.slice(), ring1: -1, ring2: j });
                    }
                } else {
                    tuples.push({ armor: chosen.slice(), ring1: -1, ring2: -1 });
                }
                return;
            }
            const pool = initMsgBase.pools[freeSlots[slotIdx]];
            for (let k = 0; k < pool.length; k++) {
                chosen.push(k);
                build(slotIdx + 1, chosen);
                chosen.pop();
            }
        };
        build(0, []);

        const results = { tupleCount: tuples.length, blocked: 0, feasible: 0, top: [] };
        let idx = 0;

        const worker = new Worker(WORKER_THREAD_PATH, { workerData: { repoRoot: REPO_ROOT } });

        const sendNext = () => {
            while (idx < tuples.length) {
                const tup = tuples[idx];
                const items = [];
                const lockedOverride = { ...initMsgBase.locked };
                for (let s = 0; s < freeSlots.length; s++) {
                    const item = initMsgBase.pools[freeSlots[s]][tup.armor[s]];
                    lockedOverride[freeSlots[s]] = item;
                    items.push(item);
                }
                let ring1_locked = initMsgBase.ring1_locked;
                let ring2_locked = initMsgBase.ring2_locked;
                if (tup.ring1 >= 0) {
                    ring1_locked = ringPoolSer[tup.ring1];
                    items.push(ring1_locked);
                }
                if (tup.ring2 >= 0) {
                    ring2_locked = ringPoolSer[tup.ring2];
                    items.push(ring2_locked);
                }
                items.push(...lockedItems);

                if (_oracleTupleBlocked(items)) {
                    // Production counts illegal-set tuples as checked without
                    // evaluating them.
                    results.blocked++;
                    idx++;
                    continue;
                }

                worker.postMessage({
                    ...initMsgBase,
                    pools: {},
                    ring_pool: [],
                    locked: lockedOverride,
                    ring1_locked,
                    ring2_locked,
                    partition: { type: 'full' },
                    worker_id: 0,
                });
                idx++;
                return;
            }
            worker.terminate();
            results.top.sort((a, b) => (b.score || 0) - (a.score || 0));
            results.top = results.top.slice(0, 15);
            resolve(results);
        };

        worker.on('message', (msg) => {
            if (msg.type === 'done') {
                results.feasible += msg.feasible || 0;
                if (msg.top5) results.top.push(...msg.top5);
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


/**
 * Ground truth for tome optimisation: run the plain oracle once per
 * (guild candidate × bundle) with the candidate as the FIXED guild tome and
 * the bundle's stats folded into static_boosts (merged additively — the same
 * arithmetic position assemble_combo_stats gives the bundle), then take the
 * per-gearset maximum. Per-variant top-15 truncation is safe: any gearset
 * outside a variant's top-15 is beaten by 15 distinct gearsets whose merged
 * scores are at least as high, so it cannot enter the merged top-15.
 */
async function runTomeOracle(initMsgBase, ringPoolSer) {
    const cands = initMsgBase.guild_tome_candidates
        ?? [{ idx: -1, sm: initMsgBase.guild_tome_sm }];
    const bundles = initMsgBase.tome_wa_bundles ?? [{ stats: null, names: null }];
    const byGear = new Map();
    let tupleCount = 0;
    for (const cand of cands) {
        for (const b of bundles) {
            const variant = { ...initMsgBase };
            variant.tome_opt = 0;
            variant.guild_tome_candidates = null;
            variant.tome_wa_bundles = null;
            variant.tome_bound = null;
            variant.guild_tome_sm = cand.sm;
            if (b.stats) {
                const sb = new Map(initMsgBase.static_boosts ?? []);
                for (const [k, v] of b.stats) sb.set(k, (sb.get(k) ?? 0) + v);
                variant.static_boosts = sb;
            }
            const res = await runOracleEnumeration(variant, ringPoolSer);
            tupleCount = res.tupleCount;
            for (const r of res.top) {
                const key = r.item_names.join('|');
                const cur = byGear.get(key);
                if (!cur || r.score > cur.score) {
                    byGear.set(key, { score: r.score, item_names: r.item_names, guild_idx: cand.idx });
                }
            }
        }
    }
    const top = [...byGear.values()].sort((a, b) => b.score - a.score).slice(0, 15);
    return { top, tupleCount, variants: cands.length * bundles.length };
}

// ── Rust score-kernel differential fixture export (P2.4 layer 1) ────────────
//
// SOLVER_EXPORT_SCORE=<path> samples K random feasible builds (seeded LCG),
// evaluates each through the production worker with all slots locked and
// SOLVER_DEBUG_COMBO capture on, and dumps a JSON fixture containing the
// exact assembled combo_base stat map + expected combo damage per case,
// plus everything the Rust damage core needs to recompute it: weapon stats,
// parsed combo rows (with embedded spell definitions and boost tokens), the
// boost registry, atree hit-string refs for prop overrides, and the game
// constant tables. The Rust score kernel must reproduce every expected
// value bit-for-bit (f64).

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

function exportScoreFixture(outPath, initMsgBase, ringPoolSer, numCases) {
    // Delegates to the shared builder (js/solver/engine/rust_bridge.js) so the
    // browser and the test harness emit byte-identical fixtures.
    const env = {
        ctx, evalInCtx: (src) => vm.runInContext(src, ctx),
        sampling: { ORACLE_ARMOR_SLOTS, WORKER_THREAD_PATH, Worker, REPO_ROOT, _oracleTupleBlocked },
    };
    return _rust_bridge.buildScoreFixture(initMsgBase, ringPoolSer, numCases, (text) => {
        fs.mkdirSync(path.dirname(outPath), { recursive: true });
        fs.writeFileSync(outPath, text);
    }, env);
}

// ── Rust enumeration-kernel fixture export (P2.3) ────────────────────────────
//
// SOLVER_EXPORT_RUST=<path> dumps everything the Rust enumeration kernel
// needs to replay this scenario's search space: free slots in enumeration
// order with priority-ordered pools (reqs/skp/crafted/set/illegal-set +
// precheck stat values), fixed equipment, weapon/guild tome, the set-bonus
// SP table, precheck thresholds with root running values, and EHP precheck
// constants. All floats are printed with full precision.

function exportRustFixture(outPath, { initMsgBase, ringPoolSer, solverSnap }) {
    const env = { ctx, evalInCtx: (src) => vm.runInContext(src, ctx) };
    const text = _rust_bridge.buildEnumFixture({ initMsgBase, ringPoolSer, solverSnap, env });
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, text);
    console.log(`  [export] Rust fixture written to ${outPath}`);
}

// ── Seed build helper ────────────────────────────────────────────────────────

/**
 * Build a statMap from the URL-hash items to extract activeMajorIDs.
 * This mirrors the real solver which uses the pre-search UI build state
 * for the atree merge (major ID abilities depend on equipped items).
 */
function _buildSeedStatMap(decoded) {
    const sp = decoded.solverParams || {};
    if (sp.roll_groups) {
        vm.runInContext(`current_roll_mode = ${JSON.stringify(sp.roll_groups)}`, ctx);
    }

    // Build equipment statMaps
    const equipSMs = [];
    for (let i = 0; i < 8; i++) {
        const name = decoded.equipment[i];
        const item_obj = (name && ctx.itemMap.has(name)) ? ctx.itemMap.get(name) : ctx.none_items[NONE_IDX[SLOT_NAMES[i]]];
        const it = ctx._apply_roll_mode_to_item(new ctx.Item(item_obj));
        equipSMs.push(it.statMap);
    }

    // Build weapon statMap with powders (roll mode + powders)
    const weaponIt = ctx._apply_roll_mode_to_item(new ctx.Item(ctx.itemMap.get(decoded.equipment[8])));
    const weaponSM = weaponIt.statMap;
    const weaponPowders = parsePowderData(decoded.powders && decoded.powders[4]);
    weaponSM.set('powders', weaponPowders);
    ctx.apply_weapon_powders(weaponSM);

    // Build tome statMaps (all 14 slots)
    const _tf = ctx.tome_fields ?? vm.runInContext('tome_fields', ctx);
    const _ntbt = {};
    for (let i = 0; i < ctx.none_tomes.length; i++) _ntbt[ctx.none_tomes[i].type] = ctx.none_tomes[i];
    const tomeSMs = [];
    for (let i = 0; i < _tf.length; i++) {
        const name = (decoded.tomes || [])[i];
        const type = _tf[i].replace(/[0-9]/g, '');
        const none = _ntbt[type] ?? ctx.none_tomes[0];
        const tome = (name && ctx.tomeMap.has(name)) ? ctx.tomeMap.get(name) : none;
        tomeSMs.push(ctx._apply_roll_mode_to_item(new ctx.Item(tome)).statMap);
    }

    // Assemble via worker-shim path to get activeMajorIDs from finalizeStatmap
    const GUILD_TOME_SEED_IDX = _tf.indexOf('guildTome1');
    const locked_sms = [weaponSM, ...tomeSMs];
    const running = ctx._init_running_statmap(decoded.level, locked_sms);
    for (const sm of equipSMs) ctx._incr_add_item(running, sm);
    const activeSetCounts = ctx.calculate_skillpoints(
        [...equipSMs, tomeSMs[GUILD_TOME_SEED_IDX]], weaponSM, ctx.levelToSkillPoints(decoded.level))?.[3];
    const build_sm = ctx._finalize_leaf_statmap(
        running, weaponSM, activeSetCounts || new Map(), ctx.sets,
        [...equipSMs, ...tomeSMs, weaponSM], null, null);
    return build_sm;
}

// ── Test runner ──────────────────────────────────────────────────────────────

async function runSolverTest(snapName) {
    const snap = loadSnapshot(snapName);

    // 1. Decode URL
    const decoded = decodeSolverUrl(ctx, snap.url_hash);
    t.assert(decoded.playerClass !== null, `${snapName}: decoded class = ${decoded.playerClass}`);

    // 2. Build atree + spells
    // Build a seed statMap from the URL-hash items so activeMajorIDs are populated
    // (mirrors the real solver which uses atree_merge.value from the pre-search UI state).
    const activeNodes = decodeActiveNodes(ctx, decoded.playerClass, decoded.atree_data);
    t.assert(activeNodes.length > 0, `${snapName}: ${activeNodes.length} atree nodes`);

    const seedSM = _buildSeedStatMap(decoded);
    const atreeMerged = buildAtreeMerged(ctx, decoded.playerClass, activeNodes, seedSM, decoded.aspects);
    const rawStats = collectRawStats(ctx, atreeMerged);
    const spellMap = collectSpells(ctx, atreeMerged);

    // 3. Build solver snapshot
    const solverSnap = buildTestSnapshot(decoded, snap, spellMap, atreeMerged, rawStats);

    // 4. Set roll mode in sandbox for pool building
    const sp = decoded.solverParams || {};
    if (sp.roll_groups) {
        vm.runInContext(`current_roll_mode = ${JSON.stringify(sp.roll_groups)}`, ctx);
    }

    // 5. Build pools using the real _build_item_pools
    const buildDir = {};
    const dirOrder = ['str', 'dex', 'int', 'def', 'agi'];
    for (let i = 0; i < 5; i++) {
        buildDir[dirOrder[i]] = !!((solverSnap.dir_enabled) & (1 << i));
    }
    const poolRestrictions = {
        lvl_overrides: solverSnap.lvl_overrides,
        lvl_min: solverSnap.lvl_min,
        lvl_max: solverSnap.lvl_max,
        no_major_id: solverSnap.no_major_id,
        build_dir: buildDir,
    };

    const illegalAt2 = new Set();
    for (const [setName, setData] of ctx.sets) {
        if (setData.bonuses?.length >= 2 && setData.bonuses[1]?.illegal) {
            illegalAt2.add(setName);
        }
    }
    ctx._tag_illegal_set_item(solverSnap.weapon, illegalAt2);
    const allPools = ctx._build_item_pools(poolRestrictions, illegalAt2);
    t.assert(Object.values(allPools).every(pool => pool.some(item => item.statMap.has('NONE'))),
        `${snapName}: every gear pool carries a protected NONE candidate`);

    // 6. Determine locked vs free items from sfree mask (from URL).
    // Benchmarks may deliberately widen a real saved build without rewriting
    // its encoded solver section. Production URL behavior remains the default.
    const sfree = snap.free_mask ?? sp.sfree ?? 0;
    const locked = {};
    let freePools = {};

    for (let i = 0; i < 8; i++) {
        const slot = SLOT_NAMES[i];
        const isFree = !!(sfree & (1 << i));

        if (!isFree) {
            // Lock this slot
            const name = snap.locked_item_overrides?.[slot] ?? decoded.equipment[i];
            const item = (name && ctx.itemMap.has(name)) ? ctx.itemMap.get(name) : ctx.none_items[NONE_IDX[slot]];
            const it = ctx._apply_roll_mode_to_item(new ctx.Item(item));
            // Apply armor powders from decoded URL (powderables indices 0-3 = armor slots 0-3)
            if (i < 4) {
                const armorPowders = parsePowderData(decoded.powders && decoded.powders[i]);
                if (armorPowders.length > 0) {
                    it.statMap.set('powders', armorPowders);
                }
            }
            ctx._tag_illegal_set_item(it, illegalAt2);
            locked[slot] = {
                statMap: it.statMap,
                _illegalSet: it._illegalSet,
                _illegalSetName: it._illegalSetName,
            };
        }
    }

    function countCombinations(poolMap) {
        let total = 1;
        const bothRingsFree = !!(sfree & (1 << 4)) && !!(sfree & (1 << 5));
        for (const [slot, pool] of Object.entries(poolMap)) {
            const size = pool.length;
            total *= slot === 'ring' && bothRingsFree ? size * (size + 1) / 2 : size;
        }
        return total;
    }
    // Build free pools: map slot names to their item type pools
    const slotToType = { helmet: 'helmet', chestplate: 'chestplate', leggings: 'leggings',
                         boots: 'boots', ring1: 'ring', ring2: 'ring', bracelet: 'bracelet', necklace: 'necklace' };
    for (let i = 0; i < 8; i++) {
        const slot = SLOT_NAMES[i];
        if (sfree & (1 << i)) {
            const type = slotToType[slot];
            if (type === 'ring') {
                if (!freePools.ring) freePools.ring = allPools.ring.slice();
            } else {
                freePools[slot] = allPools[type].slice();
            }
        }
    }
    // Tiny oracle regressions may name an exact candidate universe. Apply it
    // before the raw-pool copy and dominance so both enumerators see the same
    // candidates. `__NONE__` selects the protected empty-slot item.
    if (snap.oracle_pool_items) {
        for (const [slot, names] of Object.entries(snap.oracle_pool_items)) {
            const source = freePools[slot];
            if (!source) throw new Error(`${snapName}: no free pool named ${slot}`);
            freePools[slot] = names.map(name => {
                const item = name === '__NONE__'
                    ? source.find(it => it.statMap.has('NONE'))
                    : source.find(it => (it.statMap.get('displayName') ?? it.statMap.get('name')) === name);
                if (!item) throw new Error(`${snapName}: ${name} not found in ${slot} pool`);
                return item;
            });
        }
    }
    const inputCombinations = countCombinations(freePools);
    const preDominancePools = Object.fromEntries(
        Object.entries(freePools).map(([slot, pool]) => [slot, pool.slice()]));

    // 7. Sensitivity weights, dominance pruning, priority sorting
    const dmgWeights = ctx._build_dmg_weights(solverSnap, locked, freePools);
    let domStats = null;
    const dominanceMode = ctx._normalize_dominance_mode(
        process.env.SOLVER_DOMINANCE_MODE
        ?? (process.env.SOLVER_BENCH_VARIANT === 'original' ? 'legacy' : 'off'));
    const dominanceMetrics = {};
    if (dmgWeights) {
        domStats = ctx._build_dominance_stats(solverSnap, dmgWeights, solverSnap.restrictions);

        // Oracle fixtures retain a separately sorted copy of the raw,
        // pre-dominance pools. When they truncate pools, truncate that raw
        // candidate set first and derive the production pools from it; this
        // prevents a pruned full pool from pulling a different item across the
        // truncation boundary and invalidating the Cartesian ground truth.
        if (snap.oracle) ctx._prioritize_pools(preDominancePools, dmgWeights);
    }

    if (snap.oracle) {
        if (snap.max_pool_size) {
            for (const key of Object.keys(preDominancePools)) {
                preDominancePools[key] = preDominancePools[key].slice(0, snap.max_pool_size);
            }
        }
        freePools = Object.fromEntries(
            Object.entries(preDominancePools).map(([slot, pool]) => [slot, pool.slice()]));
    }

    if (dmgWeights) {
        ctx._prune_dominated_items(freePools, domStats, {
            mode: dominanceMode,
            preserve_set_items: process.env.SOLVER_BENCH_VARIANT !== 'original',
            metrics: dominanceMetrics,
        });
        ctx._prioritize_pools(freePools, dmgWeights);
    }

    // Non-oracle snapshots keep the historical order: reduce and prioritize
    // before any optional benchmark-only truncation.
    if (!snap.oracle && snap.max_pool_size) {
        for (const key of Object.keys(freePools)) {
            freePools[key] = freePools[key].slice(0, snap.max_pool_size);
        }
    }

    // Freshness check: locked item stats + compress hash (has free slots).
    const currentLockedStats = extractLockedItemStats(locked);
    const hasFreeSlots = Object.keys(freePools).length > 0;

    // Auto-generate snapshot freshness data on first run.
    if (snapshotNeedsGeneration(snap)) {
        console.log(`  [${snapName}] First run — generating snapshot data...`);
        snap.locked_items = currentLockedStats;
        if (!snap.scoring_target) snap.scoring_target = 'combo_damage';
        saveSnapshot(snapName, snap);
    }
    checkSnapshotFreshness(snap, t, currentLockedStats, hasFreeSlots);

    // Log pool sizes
    const poolSizes = {};
    for (const [slot, pool] of Object.entries(freePools)) {
        poolSizes[slot] = pool.length;
    }
    console.log(`  [${snapName}] pool sizes:`, poolSizes);
    console.log(`  [${snapName}] input combinations: ${inputCombinations}`);
    const combinations = countCombinations(freePools);
    console.log(`  [${snapName}] search combinations: ${combinations}`);
    console.log(`  [${snapName}] dominance: ${dominanceMode}, `
        + `${dominanceMetrics.input_items ?? 0} -> ${dominanceMetrics.output_items ?? 0} items`);
    if (snap.combination_budget) {
        const budget = snap.combination_budget;
        t.assert(inputCombinations >= budget.input_min && inputCombinations <= budget.input_max,
            `${snapName}: input combinations ${inputCombinations} within calibrated band ${budget.input_min}-${budget.input_max}`);
        t.assert(combinations >= budget.search_min && combinations <= budget.search_max,
            `${snapName}: search combinations ${combinations} within calibrated band ${budget.search_min}-${budget.search_max}`);
    }

    // 8. Serialize for worker transfer
    const poolsSer = ctx._serialize_pools(freePools);
    const oraclePools = snap.oracle ? preDominancePools : freePools;
    const oracleCombinations = countCombinations(oraclePools);
    const oraclePoolsSer = ctx._serialize_pools(oraclePools);
    const lockedSer = ctx._serialize_locked(locked);
    const ringPoolSer = poolsSer.ring || [];
    const oracleRingPoolSer = oraclePoolsSer.ring || [];
    const noneItemSMs = ctx.none_items.slice(0, 8).map(ni => ctx.expandItem(ni));

    // 9. Build base init message (without partition)
    const initMsgBase = {
        type: 'init',
        pools: poolsSer,
        locked: lockedSer,
        weapon_sm: solverSnap.weapon_sm,
        weapon_illegal_set: solverSnap.weapon._illegalSet ?? null,
        weapon_illegal_set_name: solverSnap.weapon._illegalSetName ?? null,
        level: solverSnap.level,
        tome_sms: solverSnap.tomes.map(t => t.statMap),
        guild_tome_sm: solverSnap.guild_tome_item.statMap,
        sp_budget: solverSnap.sp_budget,
        atree_merged: solverSnap.atree_mgd,
        atree_raw: solverSnap.atree_raw,
        button_states: solverSnap.button_states,
        slider_states: solverSnap.slider_states,
        radiance_boost: solverSnap.radiance_boost,
        static_boosts: solverSnap.static_boosts,
        parsed_combo: solverSnap.parsed_combo,
        boost_registry: solverSnap.boost_registry,
        scoring_target: solverSnap.scoring_target,
        combo_time: solverSnap.combo_time,
        allow_downtime: solverSnap.allow_downtime,
        hp_casting: solverSnap.hp_casting,
        health_config: solverSnap.health_config,
        auto_slider_names: solverSnap.auto_slider_names,
        spell_base_costs: solverSnap.spell_base_costs,
        restrictions: solverSnap.restrictions,
        benchmark_legacy_top_results: ['original', 'current_without_top_cutoff']
            .includes(process.env.SOLVER_BENCH_VARIANT),
        benchmark_trace: process.env.SOLVER_BENCH_TRACE === '1',
        benchmark_legacy_incremental: process.env.SOLVER_BENCH_VARIANT === 'original',
        benchmark_nested_incremental: process.env.SOLVER_BENCH_VARIANT === 'current_nested_incremental',
        benchmark_legacy_running_map: ['original', 'current_running_map']
            .includes(process.env.SOLVER_BENCH_VARIANT),
        benchmark_compact_running: process.env.SOLVER_BENCH_VARIANT === 'current_compact_object',
        sets_data: [...ctx.sets],
        ring_pool: ringPoolSer,
        ring1_locked: lockedSer.ring1 ?? null,
        ring2_locked: lockedSer.ring2 ?? null,
        none_item_sms: noneItemSMs,
        none_idx_map: NONE_IDX,
    };
    // Tome optimisation must be prepared BEFORE the fixture exports below.
    // It used to sit after them, so an exported tome fixture carried
    // tome_opt=0 and no candidates or bundles — the Rust engine could never
    // have been validated against a tome scenario, because the fixture did
    // not describe one.
    if (snap.tome_opt) {
        ctx.__tome_mode = snap.tome_opt;
        ctx.__tome_level = solverSnap.level;
        ctx.__dom_stats_tome = domStats;
        // tome_roll / tome_inventory come from the snapshot so a fixture can
        // pin the assumed roll quality and restrict the pool to owned tomes.
        // Absent means the production defaults (80%, owns everything).
        ctx.__tome_roll = snap.tome_roll ?? null;
        ctx.__tome_inv = snap.tome_inventory ?? null;
        const prep = vm.runInContext(`(function(){
            const s = {
                tome_opt: __tome_mode, level: __tome_level,
                tome_roll: __tome_roll ?? TOME_ROLL_DEFAULT,
                tome_inventory: __tome_inv ? new Set(__tome_inv) : null,
            };
            _prepare_tome_optimisation(s, {}, __dom_stats_tome ?? { higher: new Set(), lower: new Set() });
            return s;
        })()`, ctx);
        initMsgBase.tome_opt = snap.tome_opt;
        // tome_guild_locked simulates a real tome equipped in the guildTome1
        // slot: the guild choice stays fixed and only bundles are optimised.
        initMsgBase.guild_tome_candidates = snap.tome_guild_locked
            ? null : (prep.guild_tome_candidates ?? null);
        initMsgBase.tome_wa_bundles = prep.tome_wa_bundles ?? null;
        initMsgBase.tome_bound = prep.tome_bound ?? null;
        console.log(`  [${snapName}] tome opt mode ${snap.tome_opt}: `
            + `${prep.guild_tome_candidates?.length ?? 0} guild candidates, `
            + `${prep.tome_wa_bundles?.length ?? 0} bundles`);
    }

    // Clone only after tome preparation so the raw Cartesian oracle receives
    // exactly the same evaluator configuration as production; only its gear
    // pools differ.
    const oracleInitMsgBase = snap.oracle ? {
        ...initMsgBase,
        pools: oraclePoolsSer,
        ring_pool: oracleRingPoolSer,
    } : initMsgBase;

    // Optional: dump this scenario as a Rust enumeration-kernel fixture.
    // Both exports can run in one invocation (the Rust scoring integration
    // joins them by item name, so they must come from the same scenario run).
    if (process.env.SOLVER_EXPORT_RUST) {
        exportRustFixture(process.env.SOLVER_EXPORT_RUST, { initMsgBase, ringPoolSer, solverSnap });
        t.assert(true, `${snapName}: exported Rust fixture`);
        if (!process.env.SOLVER_EXPORT_SCORE) return;
    }

    // Optional: dump sampled score cases for the Rust damage-core port and stop.
    if (process.env.SOLVER_EXPORT_SCORE) {
        const k = parseInt(process.env.SOLVER_EXPORT_SCORE_CASES ?? '96', 10);
        const fixture = await exportScoreFixture(
            process.env.SOLVER_EXPORT_SCORE, initMsgBase, ringPoolSer, k);
        // The builder resolves with the fixture. It used to `return` it from
        // an inner function instead, leaving this promise pending forever -
        // the file was still written, so the only visible symptom was that
        // this assertion never ran.
        const n = fixture?.cases?.length ?? 0;
        t.assert(n > 0, `${snapName}: exported ${n} score cases`);
        return;
    }

    // 10. Build partitions and run workers
    // Match the real solver's worker count: min(hardwareConcurrency - 2, 16), at least 1.
    const numWorkers = Number(process.env.SOLVER_BENCH_WORKERS)
        || snap.num_workers || Math.max(1, Math.min((os.cpus().length || 4) - 2, 16));
    const requestedTimeLimitSeconds = Number(process.env.SOLVER_BENCH_SECONDS)
        || snap.time_limit_seconds || 30;
    // Tome optimisation: run the PRODUCTION preparation (guild candidates,
    // scoped bundle front, precheck bound). solver_item_final_nodes is an empty
    // stub in the sandbox, so every tome slot counts as empty/optimisable —
    // both the production run and the tome oracle below see the same inputs,
    // so the comparison stays internally consistent even if the URL carried
    // equipped tomes.

    const timeLimitSeconds = snap.benchmark_family
        ? Math.min(requestedTimeLimitSeconds, snap.time_limit_seconds || 30)
        : requestedTimeLimitSeconds;
    const timeLimitMs = timeLimitSeconds * 1000;
    // Real solver creates 4× worker count partitions for work-stealing.
    const numPartitions = Math.max(numWorkers * 4, numWorkers);
    const partitions = ctx._partition_work(freePools, locked, numPartitions);
    console.log(`  [${snapName}] ${partitions.length} partitions, ${numWorkers} workers, ${timeLimitMs / 1000}s limit`);

    const t0 = Date.now();
    const seedResult = snap.seed_score == null ? null : {
        score: snap.seed_score,
        item_names: snap.seed_item_names,
        seed: true,
    };
    const result = await runSolverWorkers(
        initMsgBase, ringPoolSer, partitions, numWorkers, timeLimitMs,
        snap.expected_min_score, seedResult,
    );
    const elapsed = Date.now() - t0;
    if (result.workerError) throw new Error(`solver worker failed: ${result.workerError}`);

    console.log(`  [${snapName}] checked: ${result.checked}, feasible: ${result.feasible}, top5: ${result.top5?.length}, time: ${elapsed}ms${result.timedOut ? ' (timed out)' : ''}`);
    if (process.env.SOLVER_BENCH_TRACE === '1') {
        const traceSummary = summarizeTraceMetrics(result.trace);
        console.log(`  [${snapName}] trace: ${JSON.stringify(traceSummary)}`);
        if ((result.trace?.schema_version ?? 0) >= 3) {
            const reconciliation = reconcileTraceMetrics(result.trace);
            t.assert(reconciliation.credited.ok,
                `${snapName}: credited-leaf trace counters reconcile`);
            t.assert(reconciliation.evaluator.ok,
                `${snapName}: evaluator trace counters do not over-account calls`);
            t.assert(reconciliation.spExact.ok,
                `${snapName}: exact-SP trace rejects do not exceed calls`);
        }
    }

    // 11. Assert results
    if (result.top5 && result.top5.length > 0) {
        const bestScore = result.top5[0].score;
        console.log(`  [${snapName}] best score: ${Math.round(bestScore)}`);
        if (process.env.SOLVER_PRINT_TOP15 === '1') {
            for (const r of result.top5) {
                console.log(`  [top15] ${r.score.toExponential(17)} | ${r.item_names?.filter(n => n).join(', ')}`);
            }
        }

        if (snap.expected_min_score != null) {
            // Score must reach or surpass the target.
            t.assertGe(bestScore, snap.expected_min_score,
                `${snapName}: best score ${Math.round(bestScore)} >= target ${snap.expected_min_score}`);
        } else {
            // No target given — just verify the solver found a functional build.
            t.assert(true, `${snapName}: found a functional build (score=${Math.round(bestScore)})`);
        }

        const best = result.top5[0];
        if (best.item_names) {
            const items = best.item_names.map((n, i) => n || `(none@${SLOT_NAMES[i]})`);
            console.log(`  [${snapName}] best items: ${items.join(', ')}`);
        }
    } else {
        // An empty result is a valid exact answer when every tuple is
        // infeasible. Oracle fixtures verify that independently below instead
        // of treating an empty top-N as a harness failure.
        if (snap.oracle || snap.expect_empty_results) {
            t.assert(true, `${snapName}: empty result retained for exact oracle verification`);
        } else {
            t.assert(false, `${snapName}: solver found no results`);
        }
    }

    // 12. Oracle verification (P0.4): exact equality against an independent
    // Cartesian enumeration, plus partition-count invariance.
    if (snap.oracle) {
        t.assert(!result.timedOut, `${snapName}: oracle run completed within time limit`);

        let oracle;
        if (initMsgBase.tome_opt) {
            oracle = await runTomeOracle(oracleInitMsgBase, oracleRingPoolSer);
            console.log(`  [${snapName}] tome oracle: ${oracle.tupleCount} tuples x ${oracle.variants} tome variants`);
            t.assert(oracle.tupleCount === oracleCombinations,
                `${snapName}: raw Cartesian count ${oracleCombinations} == oracle tuple count ${oracle.tupleCount}`);
            t.assert(result.checked <= oracle.tupleCount,
                `${snapName}: production checked ${result.checked} <= raw oracle ${oracle.tupleCount}`);
            // No feasible-count comparison in tome mode: production counts a
            // leaf feasible when the OPTIMISTIC gate passes, which is a
            // superset of any single candidate's feasibility.
            for (const r of result.top5) {
                t.assert(typeof r.guild_tome_idx === 'number',
                    `${snapName}: tome-mode result entries carry guild_tome_idx`);
            }
        } else {
            oracle = await runOracleEnumeration(oracleInitMsgBase, oracleRingPoolSer);
            console.log(`  [${snapName}] oracle: ${oracle.tupleCount} tuples, ${oracle.blocked} illegal-set blocked, ${oracle.feasible} feasible`);

            if (snap.expected_oracle_blocked != null) {
                t.assert(oracle.blocked === snap.expected_oracle_blocked,
                    `${snapName}: oracle blocked ${oracle.blocked} exclusive-set tuples`);
            }

            t.assert(oracle.tupleCount === oracleCombinations,
                `${snapName}: raw Cartesian count ${oracleCombinations} == oracle tuple count ${oracle.tupleCount}`);
            t.assert(result.checked <= oracle.tupleCount,
                `${snapName}: production checked ${result.checked} <= raw oracle ${oracle.tupleCount}`);
            if (dominanceMode === 'off' || dominanceMode === 'exact') {
                t.assert(result.checked === oracle.tupleCount,
                    `${snapName}: exact-mode checked ${result.checked} == raw oracle ${oracle.tupleCount}`);
                t.assert(result.feasible === oracle.feasible,
                    `${snapName}: exact-mode feasible ${result.feasible} == oracle ${oracle.feasible}`);
            }
        }

        const prodScores = result.top5.map(r => r.score);
        const oracleScores = oracle.top.map(r => r.score);
        const exactDominance = dominanceMode === 'off' || dominanceMode === 'exact';
        if (exactDominance) {
            t.assert(prodScores.length === oracleScores.length,
                `${snapName}: top-N length ${prodScores.length} == oracle ${oracleScores.length}`);
            let scoresEqual = prodScores.length === oracleScores.length;
            for (let i = 0; i < Math.min(prodScores.length, oracleScores.length); i++) {
                if (prodScores[i] !== oracleScores[i]) { scoresEqual = false; break; }
            }
            t.assert(scoresEqual,
                `${snapName}: top-N scores match raw oracle exactly`
                + (scoresEqual ? '' : ` (prod=${JSON.stringify(prodScores)} oracle=${JSON.stringify(oracleScores)})`));
        } else {
            t.assert(prodScores[0] === oracleScores[0],
                `${snapName}: dominance mode ${dominanceMode} preserves the raw-oracle optimum`);
        }
        if ((dominanceMode === 'off' || dominanceMode === 'exact')
            && result.top5.length && oracle.top.length) {
            t.assert(JSON.stringify(result.top5[0].item_names) === JSON.stringify(oracle.top[0].item_names),
                `${snapName}: best build items match oracle`);
        }

        // Partition completeness: a different partition count must not change
        // checked, feasible, or top-N scores.
        const altPartitions = ctx._partition_work(freePools, locked, 3);
        const altResult = await runSolverWorkers(
            initMsgBase, ringPoolSer, altPartitions, 1, timeLimitMs, null, null);
        t.assert(altResult.checked === result.checked,
            `${snapName}: 3-partition checked ${altResult.checked} == ${result.checked}`);
        t.assert(altResult.feasible === result.feasible,
            `${snapName}: 3-partition feasible ${altResult.feasible} == ${result.feasible}`);
        const altScores = altResult.top5.map(r => r.score);
        t.assert(JSON.stringify(altScores) === JSON.stringify(prodScores),
            `${snapName}: 3-partition top-N scores identical`);
    }
}

// ── Discover and run test cases ──────────────────────────────────────────────

function testInvalidExclusiveSeedRejected() {
    const nodes = ctx.none_items.slice(0, 8).map((none, i) => {
        const item = new ctx.Item(none);
        item.statMap.set('NONE', true);
        return { value: item, slot: i };
    });
    nodes[4] = { value: new ctx.Item(ctx.itemMap.get('Intensity')), slot: 4 };
    ctx.solver_item_final_nodes = nodes;

    const weapon = new ctx.Item(ctx.itemMap.get('Infused Hive Wand'));
    const craftedWeapon = new Map(weapon.statMap);
    craftedWeapon.set('crafted', true);
    t.assert(!ctx._has_illegal_set_conflict(
        [nodes[4].value.statMap], craftedWeapon),
        'crafted items do not occupy game set exclusivity slots');
    const originalWarn = ctx.console.warn;
    ctx.console.warn = () => {};
    let seed;
    try {
        seed = ctx._eval_current_build(
            { weapon_sm: weapon.statMap }, { stat_thresholds: [] }, new Set());
    } finally {
        ctx.console.warn = originalWarn;
    }
    t.assert(seed === null,
        'invalid weapon + gear exclusive-set build is rejected as an incumbent');

    // These are the two guarded consumers in start_solver_search and the
    // shared-cutoff initialiser: a null evaluator result reaches neither.
    const seededTop = seed ? [seed] : [];
    const sharedCutoff = typeof seed?.score === 'number' ? seed.score : null;
    t.assert(seededTop.length === 0 && sharedCutoff === null,
        'invalid exclusive-set seed cannot enter top-N or the shared cutoff');
    ctx.solver_item_final_nodes = [];
}

async function main() {
    testInvalidExclusiveSeedRejected();
    const snapDir = path.join(__dirname, 'snapshots');
    const allSnaps = (fs.existsSync(snapDir) ? fs.readdirSync(snapDir) : [])
        .filter(f => f.startsWith('solver_') && f.endsWith('.snap.json'))
        .map(f => f.replace('.snap.json', ''));

    // Allow filtering by CLI args: node test_solver_search.js <name1> <name2> ...
    // Names can be partial matches (e.g. "archer" matches "solver_archer_dps").
    const args = process.argv.slice(2);
    let solverSnaps;
    if (args.length > 0) {
        solverSnaps = allSnaps.filter(s => args.some(a => s.includes(a)));
        if (solverSnaps.length === 0) {
            console.error(`No snapshots matched: ${args.join(', ')}`);
            console.error(`Available: ${allSnaps.join(', ')}`);
            process.exit(1);
        }
        console.log(`Running ${solverSnaps.length}/${allSnaps.length} snapshot(s): ${solverSnaps.join(', ')}`);
    } else {
        solverSnaps = allSnaps.filter(name => {
            const snap = loadSnapshot(name);
            return !snap.benchmark_only;
        });
    }

    if (solverSnaps.length === 0) {
        t.warn('No solver snapshots found. Create snapshots/solver_*.snap.json to add test cases.');
        t.warn('See README.md for snapshot format.');
    }

    for (const snapName of solverSnaps) {
        try {
            await runSolverTest(snapName);
        } catch (err) {
            t.assert(false, `${snapName}: threw error — ${err.message}`);
            console.error(err.stack);
        }
    }

    const summary = t.summary();
    if (require.main === module) {
        if (summary.fail > 0) process.exit(1);
    }
}

main();
