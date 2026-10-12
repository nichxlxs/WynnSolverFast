'use strict';

// Browser orchestration without native timing: fake workers, clock and DOM
// exercise cancellation races and preserve actual SP/tome witnesses.
const assert = require('assert');
const fs = require('fs');
const path = require('path');
const vm = require('vm');
// The page loads top_results.js (the shared result order) before search.js.
const source = fs.readFileSync(path.join(__dirname, '../engine/top_results.js'), 'utf8')
    + '\n' + fs.readFileSync(path.join(__dirname, '../engine/search.js'), 'utf8');
let passed = 0;

function setup() {
    let now = 1000, monotonic = 1000, timerId = 0;
    const timers = new Map(), elements = new Map(), workers = [];
    const el = id => {
        if (!elements.has(id)) elements.set(id, {
            value: '', textContent: '', innerHTML: '', className: '', disabled: false,
            style: {}, dataset: {}, classList: { contains: () => false },
            querySelector: () => null,
        });
        return elements.get(id);
    };
    el('solver-engine').value = 'rust';
    el('solver-search-mode').value = 'quick';
    el('solver-quick-budget').value = '5';
    class MockWorker {
        constructor() { this.messages = []; this.terminated = false; workers.push(this); }
        postMessage(m) { this.messages.push(m); }
        terminate() { this.terminated = true; }
        emit(data) { this.onmessage?.({ data }); }
    }
    const ctx = vm.createContext({
        console: { log() {}, warn() {}, error() {} }, Date: class extends Date { static now() { return now; } },
        performance: { now: () => monotonic },
        document: { getElementById: el }, window: {}, navigator: { hardwareConcurrency: 8 },
        Worker: MockWorker, Map, Set, Array, Number, Uint32Array,
        RESTRICTION_STATS: [], TOME_OPT_ALL: 2, tome_fields: [], solver_item_final_nodes: [],
        setTimeout(fn, ms) { timers.set(++timerId, { fn, ms, interval: false }); return timerId; },
        setInterval(fn, ms) { timers.set(++timerId, { fn, ms, interval: true }); return timerId; },
        clearTimeout(id) { timers.delete(id); }, clearInterval(id) { timers.delete(id); },
        crypto: { getRandomValues: a => { a[0] = 123; return a; } },
    });
    vm.runInContext(source, ctx);
    vm.runInContext('globalThis.state = _solver_state', ctx);
    ctx.schedule_search_space_update = () => {};
    ctx._display_solver_results = results => { ctx.rendered = results.slice(); };
    ctx._fill_build_into_ui = result => { ctx.applied = result; };
    ctx._reconstruct_result_items = names => names.map(name => ({ statMap: new Map([['name', name]]) }));
    ctx._rust_compiled_module = () => Promise.resolve({ compiled: true });
    Object.assign(ctx.state, { running: true, search_mode: 'quick', run_id: 1,
        start: now, quick_started: monotonic, quick_deadline: monotonic + 5000,
        quick_budget_secs: 5, engine_used: 'rust' });
    return { ctx, el, workers, timers, setNow: n => { now = n; monotonic = n; }, shiftWall: n => { now = n; } };
}

function witness(score = 100) {
    return { score, item_names: ['H', 'C', 'L', 'B', 'R1', 'R2', 'Br', 'N'],
        base_sp: [1, 2, 3, 4, 5], total_sp: [11, 12, 13, 14, 15], assigned_sp: 15,
        tome: { guild_idx: 3, weaponTome: ['Power'], armorTome: ['Guard'] } };
}
async function tick() { await Promise.resolve(); await Promise.resolve(); }
async function prepareToScorePromise(s) {
    const { ctx, timers } = s;
    ctx.state.running = false;
    ctx.sets = new Map(); ctx.atree_validate = { value: [false] };
    ctx.get_restrictions = () => ({ stat_thresholds: [{ stat: 'total_hp', op: '>=', val: 1000 }] });
    ctx._build_solver_snapshot = restrictions => ({
        weapon: { statMap: new Map() }, scoring_target: 'total_hp', restrictions,
    });
    ctx.get_blacklist = () => new Set();
    ctx._collect_locked_items = () => ({});
    ctx._eval_current_build = () => { throw new Error('Quick must not rank unvalidated UI seeds'); };
    ctx._build_item_pools = () => ({ helmet: [1, 2] });
    ctx._build_dmg_weights = () => ({}); ctx._display_priority_weights = () => {};
    ctx._prioritize_pools = () => {}; ctx._prepare_tome_optimisation = () => {};
    ctx._build_dominance_stats = () => ({});
    ctx._prune_dominated_items = () => { throw new Error('Quick must retain raw gear pools'); };
    ctx._serialize_pools = p => p; ctx._serialize_locked = p => p;
    ctx._build_worker_init_msg = snap => ({ restrictions: snap.restrictions });
    let resolve, captured;
    ctx.window.__solver_rust_bridge = {
        browserEnv: () => ({}), buildEnumFixture: () => 'raw enum',
        sanitizeEnumFixtureForAnytime: text => { assert.equal(text, 'raw enum'); return 'safe enum'; },
        buildScoreFixture: init => { captured = init; return new Promise(r => { resolve = r; }); },
    };
    const pending = ctx._start_quick_solver_search();
    for (let i = 0; i < 20 && !resolve; i++) {
        for (const [id, t] of [...timers]) if (t.ms === 0) { timers.delete(id); t.fn(); }
        await tick();
    }
    assert.ok(resolve, 'preparation reached score fixture promise');
    assert.equal(captured.restrictions.stat_thresholds[0].val, 1000);
    return { resolve, pending };
}
async function check(name, test) {
    await test(); passed++; console.log(`PASS: ${name}`);
}

(async () => {
    await check('quick controls disable engine/threads and preserve exhaustive option', () => {
        const { ctx, el } = setup();
        ctx.state.running = false; ctx.solver_engine_changed();
        assert.equal(el('solver-engine').disabled, true);
        assert.equal(el('solver-thread-count').disabled, true);
        assert.equal(el('solver-quick-budget-row').style.display, '');
        el('solver-search-mode').value = 'exhaustive'; ctx.solver_engine_changed();
        assert.equal(el('solver-thread-count').disabled, false);
        assert.equal(el('solver-quick-budget-row').style.display, 'none');
    });
    await check('worker receives remaining wall budget and only one partition', async () => {
        const { ctx, workers, setNow } = setup();
        setNow(2500); ctx._start_quick_search_worker(1, 'enum', 'score'); await tick();
        assert.equal(workers.length, 1);
        assert.equal(workers[0].messages[0].type, 'solve_anytime');
        assert.equal(workers[0].messages[0].options.seconds, 3.5);
        assert.equal(workers[0].messages[0].options.warm_budget, 2000000);
        assert.equal(workers[0].messages[0].options.cycle_stagnation, true);
    });
    await check('Stop retains validated interim SP/tomes and excludes unvalidated UI seed', async () => {
        const { ctx, workers, el } = setup();
        const seed = witness(50); seed.item_names[0] = 'Seed';
        ctx.state.seed_build = { ...seed, items: ctx._reconstruct_result_items(seed.item_names) };
        ctx._start_quick_search_worker(1, 'enum', 'score'); await tick();
        workers[0].emit({ type: 'progress', leaf_calls: 1234, top_n: [witness()] });
        ctx.toggle_solver();
        assert.equal(ctx.state.running, false); assert.equal(workers[0].terminated, true);
        assert.equal(ctx.state.top5.length, 1);
        assert.equal(ctx.applied.assigned_sp, 15);
        assert.deepEqual(Array.from(ctx.applied.total_sp), [11, 12, 13, 14, 15]);
        assert.equal(ctx.applied.guild_tome_idx, 3);
        assert.equal(ctx.applied.tome_names.weaponTome[0], 'Power');
        assert.match(el('solver-progress-left').textContent, /Builds evaluated/);
        assert.doesNotMatch(el('solver-progress-left').textContent, /Checked|Solved|%/);
        assert.equal(el('solver-remaining-text').textContent, '');
    });
    await check('stale worker result cannot overwrite immediately restarted run', async () => {
        const { ctx, workers } = setup();
        ctx._start_quick_search_worker(1, 'enum', 'score'); await tick();
        const old = workers[0]; ctx._finish_quick_search(1, 'stopped');
        Object.assign(ctx.state, { running: true, run_id: 3, top5: [], seed_build: null, workers: [] });
        old.emit({ type: 'progress', top_n: [witness(1000)] });
        old.emit({ type: 'done', top_n: [witness(2000)] });
        assert.equal(ctx.state.top5.length, 0); assert.equal(ctx.state.running, true);
    });
    await check('pending module promise cannot post after Stop or restart', async () => {
        const { ctx, workers } = setup(); let resolve;
        ctx._rust_compiled_module = () => new Promise(r => { resolve = r; });
        ctx._start_quick_search_worker(1, 'enum', 'score');
        ctx._finish_quick_search(1, 'stopped');
        Object.assign(ctx.state, { running: true, run_id: 3 });
        resolve({}); await tick();
        assert.equal(workers[0].messages.length, 0);
    });
    await check('worker error keeps discovered builds and never launches JS fallback', async () => {
        const { ctx, workers, el } = setup(); let fallback = false;
        ctx._run_solver_search_workers = () => { fallback = true; };
        ctx._start_quick_search_worker(1, 'enum', 'score'); await tick();
        workers[0].emit({ type: 'progress', top_n: [witness()] });
        workers[0].emit({ type: 'worker_error', code: 'unsupported', message: 'unsupported objective' });
        assert.equal(fallback, false); assert.equal(ctx.state.engine_used, 'rust');
        assert.equal(ctx.applied.score, 100);
        assert.match(el('solver-error-text').textContent, /Exhaustive search/);
    });
    await check('natural completion is never presented as exhaustive or infeasible', async () => {
        const { ctx, workers, el } = setup();
        ctx._start_quick_search_worker(1, 'enum', 'score'); await tick();
        workers[0].emit({ type: 'done', complete: false, top_n: [], stop_reason: 'time_budget' });
        assert.equal(ctx.state.stop_reason, 'time_budget');
        assert.match(el('solver-results-panel').textContent, /No matching build found in this run/);
        assert.doesNotMatch(el('solver-progress-left').textContent, /Solved|%/);
    });
    await check('missing SP does not fabricate a result and empty final preserves interim', async () => {
        const { ctx, workers } = setup();
        ctx._start_quick_search_worker(1, 'enum', 'score'); await tick();
        workers[0].emit({ type: 'progress', top_n: [{ score: 500, item_names: witness().item_names }] });
        assert.equal(ctx.state.top5.length, 0);
        workers[0].emit({ type: 'progress', top_n: [witness()] });
        workers[0].emit({ type: 'done', top_n: [], stop_reason: 'time_budget' });
        assert.equal(ctx.applied.score, 100);
    });
    await check('host deadline wins even while compiled module remains pending', async () => {
        const { ctx, workers, setNow } = setup(); let resolve;
        ctx._rust_compiled_module = () => new Promise(r => { resolve = r; });
        ctx._start_quick_search_worker(1, 'enum', 'score');
        setNow(7000); resolve({}); await tick();
        assert.equal(workers[0].messages.length, 0);
        assert.equal(ctx.state.running, false); assert.equal(ctx.state.stop_reason, 'time_budget');
    });
    await check('system clock changes cannot extend a quick search budget', async () => {
        const { ctx, workers, setNow, shiftWall } = setup();
        setNow(2500); shiftWall(-1000000);
        ctx._start_quick_search_worker(1, 'enum', 'score'); await tick();
        assert.equal(workers[0].messages[0].options.seconds, 3.5);
    });
    await check('pending exact fixture cannot resurrect a canceled search either', async () => {
        const { ctx, workers } = setup(); let resolve;
        ctx.state.search_mode = 'exhaustive';
        ctx.window.__solver_rust_bridge = {
            browserEnv: () => ({}), buildEnumFixture: () => '',
            buildScoreFixture: () => new Promise(r => { resolve = r; }),
        };
        assert.equal(ctx._try_run_solver_search_rust({}, {}, {}, [], {}, () => {}), true);
        ctx._stop_solver(); Object.assign(ctx.state, { running: true, run_id: 3 });
        resolve({}); await tick(); assert.equal(workers.length, 0);
    });
    await check('stale JavaScript exact callbacks cannot stop or modify a new Quick run', () => {
        const { ctx, workers, timers } = setup();
        ctx.state.search_mode = 'exhaustive';
        ctx.solver_selected_worker_count = () => 1;
        ctx._serialize_pools = p => p; ctx._serialize_locked = p => p;
        ctx._partition_work = () => [{}]; ctx._build_worker_init_msg = () => ({});
        ctx._run_solver_search_workers({}, {}, {}, true);
        const old = workers[0]; const queuedTimer = [...timers.values()].find(t => t.interval);
        ctx._stop_solver();
        Object.assign(ctx.state, { running: true, run_id: 3, search_mode: 'quick', checked: 999, workers: [] });
        old.emit({ type: 'progress', checked: 5, feasible: 2, top5_names: [] });
        old.emit({ type: 'done', checked: 5, feasible: 2, met_req: 2, top5: [] });
        old.onerror({ message: 'late crash' }); queuedTimer.fn();
        assert.equal(ctx.state.running, true); assert.equal(ctx.state.checked, 999);
    });
    await check('pending quick fixture keeps raw pools/final constraints and cannot resurrect a run', async () => {
        const s = setup();
        const { resolve, pending } = await prepareToScorePromise(s);
        s.ctx.toggle_solver();
        Object.assign(s.ctx.state, { running: true, run_id: s.ctx.state.run_id + 1 });
        resolve({}); await tick();
        for (const [id, t] of [...s.timers]) if (t.ms === 0) { s.timers.delete(id); t.fn(); }
        await pending;
        assert.equal(s.workers.length, 0); assert.equal(s.ctx.state.running, true);
    });
    await check('host deadline finishes pending preparation and discards late fixture resolution', async () => {
        const s = setup();
        const { resolve, pending } = await prepareToScorePromise(s);
        s.setNow(6500);
        const deadline = [...s.timers.values()].find(t => t.ms === 5000 && !t.interval);
        assert.ok(deadline); deadline.fn();
        assert.equal(s.ctx.state.stop_reason, 'time_budget');
        assert.equal(s.ctx.state.running, false);
        resolve({}); await tick();
        for (const [id, t] of [...s.timers]) if (t.ms === 0) { s.timers.delete(id); t.fn(); }
        await pending; assert.equal(s.workers.length, 0);
    });
    await check('set weapons are accepted (C1); already-invalid locked exclusive sets are rejected', () => {
        const { ctx } = setup();
        const item = name => ({ statMap: new Map([['set', name]]) });
        assert.doesNotThrow(() => ctx._quick_assert_supported_base({ weapon: item('Petal') }, {}, new Set()));
        const snap = { weapon: { statMap: new Map() } };
        assert.throws(() => ctx._quick_assert_supported_base(snap,
            { helmet: item('Hive'), boots: item('Hive') }, new Set(['Hive'])), /multiple exclusive/);
        assert.doesNotThrow(() => ctx._quick_assert_supported_base(snap,
            { helmet: item('Hive') }, new Set(['Hive'])));
    });
    await check('consecutive auto-tome runs replace prior results while preserving fixed user tomes', () => {
        const { ctx, el } = setup();
        ctx.tome_fields = ['weaponTome1', 'armorTome1', 'guildTome1', 'weaponTome2'];
        ctx.solver_item_final_nodes = Array(9).fill(null).concat([
            { value: { statMap: new Map([['name', 'Power']]) } },
            { value: { statMap: new Map([['name', 'Guard']]) } },
            { value: { statMap: new Map([['name', 'Guild']]) } },
            { value: { statMap: new Map([['name', 'User lock']]) } },
        ]);
        el('weaponTome1-choice').dataset.solverFilled = 'true';
        el('armorTome1-choice').dataset.solverFilled = 'true';
        const snap = { tome_opt: 2, tomes: [] };
        ctx._quick_fixed_tomes(snap);
        assert.deepEqual(Array.from(snap.tomes, item => item.statMap.get('name')), ['Guild', 'User lock']);
        const fixed = { tome_opt: 0, tomes: ['unchanged'] };
        ctx._quick_fixed_tomes(fixed); assert.deepEqual(fixed.tomes, ['unchanged']);
    });
    await check('tome preparation uses captured slots after live inputs change', () => {
        const { ctx, el } = setup();
        ctx.tome_fields = ['weaponTome1', 'armorTome1', 'guildTome1'];
        ctx.solver_item_final_nodes = Array(9).fill(null).concat(['Power', 'Guard', 'Guild'].map(name => ({
            value: { statMap: new Map([['name', name]]) },
        })));
        const snap = { tome_opt: 2, tomes: [] }; ctx._quick_fixed_tomes(snap);
        ctx.solver_item_final_nodes = Array(12).fill(null);
        el('weaponTome1-choice').dataset.solverFilled = 'true';
        el('armorTome1-choice').dataset.solverFilled = 'true';
        // All captured slots were locks: no replacement bundle or guild
        // candidate may appear, even though the live inputs are now empty.
        ctx._prepare_tome_optimisation(snap, {}, {});
        assert.equal(snap.tome_wa_bundles, null);
        assert.equal(snap.guild_tome_candidates, null);
        assert.equal(snap.tomes.length, 3);
    });
    await check('tome recursion and quadratic Pareto loops honor preparation budget callbacks', () => {
        const { ctx } = setup();
        const constants = fs.readFileSync(path.join(__dirname, '../constants.js'), 'utf8');
        for (const name of ['tome_prune_dominated', 'tome_bundles', 'pareto_prune_bundles']) {
            const match = constants.match(new RegExp(`function ${name}\\([\\s\\S]*?^}`, 'm'));
            assert.ok(match); vm.runInContext(match[0], ctx);
        }
        ctx.tome_stat = (sm, key) => sm.get(key) ?? 0;
        const maps = Array.from({ length: 40 }, (_, i) => new Map([['x', i]]));
        const deadline = () => { let checks = 0; return () => { if (++checks === 3) throw new Error('budget exhausted'); }; };
        assert.throws(() => ctx.tome_prune_dominated(maps, ['x'], [0], deadline()), /budget exhausted/);
        assert.throws(() => ctx.tome_bundles(maps, 4, ['x'], [0], deadline()), /budget exhausted/);
        const bundles = Array.from({ length: 100 }, (_, i) => ({ vec: [i], picks: [] }));
        assert.throws(() => ctx.pareto_prune_bundles(bundles, [0], deadline()), /budget exhausted/);
        assert.equal(ctx.tome_bundles(maps.slice(0, 2), 2, ['x'], [0]).length, 3);
    });
    console.log(`${passed} passed, 0 failed, 0 warnings`);
})().catch(error => { console.error(error); process.exitCode = 1; });
