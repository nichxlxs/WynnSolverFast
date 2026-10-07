// Bounded browser integration gate for the real solver page and shipped WASM.
// Serves the repository itself on a free local port, like the other browser
// tests, so it runs under run_all.js. To test another server instead:
//   QUICK_E2E_BASE_URL=http://127.0.0.1:8765 node js/solver/tests/test_browser_quick_search.js
// Unlike the optional browser guards, missing dependencies/browser are failures.
// No exhaustive four-slot reference search is launched by this test.
'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const { REPO_ROOT } = require('./harness');

const fixture = JSON.parse(fs.readFileSync(path.join(__dirname,
    'snapshots/solver_meta_warrior_paladin_tank_known_good.snap.json'), 'utf8'));
// Use the real known equipment and ability tree, without benchmark-specific
// restrictions. This suite checks browser lifecycle, not historical scores.
const BUILD_HASH = new URL(fixture.seed_build_url).hash;
let BASE_URL = process.env.QUICK_E2E_BASE_URL || null;
// Traces and screenshots go to the temp directory unless asked for: they
// used to land in an untracked folder at the repository root.
const ARTIFACTS = process.env.QUICK_E2E_ARTIFACTS || path.join(os.tmpdir(), 'quick-browser-artifacts');
const MIME = {
    '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
    '.json': 'application/json', '.wasm': 'application/wasm',
    '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png',
    '.ico': 'image/x-icon', '.woff2': 'font/woff2',
};
function serve(root) {
    const server = http.createServer((req, res) => {
        const rel = decodeURIComponent(req.url.split('?')[0]).replace(/^\/+/, '');
        const file = path.join(root, rel);
        if (!file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
            res.writeHead(404); res.end('not found'); return;
        }
        res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream' });
        fs.createReadStream(file).pipe(res);
    });
    return new Promise((r) => server.listen(0, '127.0.0.1', () => r(server)));
}
const GEAR = ['helmet', 'chestplate', 'leggings', 'boots', 'ring1', 'ring2', 'bracelet', 'necklace'];
let assertions = 0;
let loadSequence = 0;
const measurements = [];
const diagnostics = { browser_errors: [], worker_urls: [], measurements };

function check(condition, message) {
    assert.ok(condition, message);
    assertions++;
    console.log(`PASS ${message}`);
}

function requireBrowser() {
    // Explicit dependency path permits isolated CI provisioning and use of an
    // existing runtime. Never silently substitute a broken checked-in package.
    const modulePath = process.env.PLAYWRIGHT_CORE_PATH || 'playwright-core';
    const { chromium } = require(modulePath);
    const executablePath = process.env.CHROMIUM_PATH || chromium.executablePath();
    assert.ok(fs.existsSync(executablePath), `Chromium is required: ${executablePath}`);
    return { chromium, executablePath };
}

async function state(page) {
    return page.evaluate(() => ({
        running: _solver_state.running,
        run_id: _solver_state.run_id,
        search_mode: _solver_state.search_mode,
        engine_used: _solver_state.engine_used,
        fallback: _solver_state.engine_fallback_reason,
        elapsed_ms: Date.now() - _solver_state.start,
        budget_secs: _solver_state.quick_budget_secs,
        leaf_calls: _solver_state.leaf_calls,
        worker_count: _solver_state.workers.length,
        stop_reason: _solver_state.stop_reason,
        deadline_timer: _solver_state.deadline_timer,
        error: document.getElementById('solver-error-text').textContent,
        status: document.getElementById('solver-status-msg').textContent,
        top: _solver_state.top5.map(r => ({
            score: r.score,
            items: r.items.map(i => i.statMap.get('displayName') ?? i.statMap.get('name')),
            base_sp: r.base_sp, total_sp: r.total_sp, assigned_sp: r.assigned_sp,
            guild_tome_idx: r.guild_tome_idx, tome_names: r.tome_names,
        })),
    }));
}

function checkWitness(result, label, expectTome) {
    check(!!result && Number.isFinite(result.score) && result.score > 0,
        `${label}: finite positive score`);
    check(result.items.length === 8 && result.items.every(n => typeof n === 'string'),
        `${label}: eight named equipment slots`);
    for (const field of ['base_sp', 'total_sp']) {
        check(Array.isArray(result[field]) && result[field].length === 5
            && result[field].every(Number.isFinite), `${label}: ${field} witness`);
    }
    check(Number.isFinite(result.assigned_sp), `${label}: assigned SP witness`);
    if (expectTome) {
        check(Number.isInteger(result.guild_tome_idx), `${label}: guild tome witness`);
        check(Array.isArray(result.tome_names?.weaponTome)
            && Array.isArray(result.tome_names?.armorTome), `${label}: tome bundle witness`);
    }
}

async function load(page) {
    await page.goto(`${BASE_URL}/solver/index.html?quick-e2e=${++loadSequence}${BUILD_HASH}`,
        { waitUntil: 'load', timeout: 60000 });
    // Service-worker adoption may reload once. Do not configure the UI before
    // that settles; Quick also works without cross-origin isolation.
    await page.waitForFunction(() => window.crossOriginIsolated
        || sessionStorage.getItem('coi-reloaded') === '1' || !navigator.serviceWorker,
    null, { timeout: 15000 }).catch(() => {});
    await page.waitForFunction(expectedWeapon => {
        const button = document.getElementById('solver-run-btn');
        return button && !button.disabled
            && document.getElementById('weapon-choice')?.value === expectedWeapon
            && typeof solver_build_node !== 'undefined' && solver_build_node.value
            && typeof window.__solver_rust_bridge?.sanitizeEnumFixtureForAnytime === 'function';
    }, fixture.core_weapon, { timeout: 60000 });
    check(await page.locator('#solver-search-mode').inputValue() === 'exhaustive',
        'fresh page defaults to Exhaustive search');
    const scriptOrder = await page.evaluate(() => {
        const scripts = [...document.scripts].map(s => s.src);
        return { bridge: scripts.findIndex(s => s.endsWith('/rust_bridge.js')),
            search: scripts.findIndex(s => s.endsWith('/engine/search.js')) };
    });
    check(scriptOrder.bridge >= 0 && scriptOrder.search > scriptOrder.bridge,
        'actual page loads the bridge before search orchestration');
}

async function configure(page, { quick = true, seconds = 5, wide = false, tomes = true } = {}) {
    await page.locator('#solver-target').selectOption('total_hp');
    const mana = page.locator('#combo-mana-btn');
    if ((await mana.getAttribute('class')).includes('toggleOn')) await mana.click();
    await page.locator('#restr-tome-opt').selectOption(tomes ? '1' : '0');
    await page.locator('#solver-search-mode').selectOption(quick ? 'quick' : 'exhaustive');
    if (quick) await page.locator('#solver-quick-budget').selectOption(String(seconds));
    else await page.locator('#solver-engine').selectOption('rust');
    for (const slot of GEAR) {
        const shouldFree = wide || slot === 'helmet';
        const isFree = await page.locator(`#${slot}-choice`).getAttribute('data-solver-filled');
        if ((isFree === 'true') !== shouldFree) await page.locator(`#${slot}-lock`).click({ force: true });
    }
    // The completion regression opens only a single real-item level cohort,
    // keeping its reference search small even when the catalogue grows.
    const level = await page.evaluate(() => solver_item_final_nodes[0].value.statMap.get('lvl'));
    await page.locator('#restr-lvl-min').fill(wide ? '1' : String(level));
    await page.locator('#restr-lvl-max').fill(wide ? '121' : String(level));
    await page.locator('#restr-lvl-max').press('Tab');
    check(await page.locator('#solver-target').inputValue() === 'total_hp', 'target selection applies');
    check((await page.locator('#solver-engine').isDisabled()) === quick,
        'Quick owns the engine selector; Exhaustive retains engine choice');
}

async function start(page) {
    const old = await state(page);
    await page.locator('#solver-run-btn').click();
    // A tiny search can finish between polls. A changed generation proves the
    // button started a new run even if `running` is already false again.
    await page.waitForFunction(previous => _solver_state.run_id > previous,
        old.run_id, { timeout: 5000 });
}

async function waitFinished(page, timeout = 9000) {
    await page.waitForFunction(() => !_solver_state.running, null, { timeout });
    const result = await state(page);
    await page.waitForFunction(() => _solver_state.workers.length === 0,
        null, { timeout: 3000 });
    return result;
}

async function deadlineCase(page) {
    await load(page);
    await configure(page);
    await start(page);
    await page.waitForFunction(() => _solver_state.leaf_calls > 0 && _solver_state.top5.length > 0,
        null, { timeout: 7500 });
    const first = await state(page);
    const final = await waitFinished(page);
    check(first.elapsed_ms <= 7000, 'five-second search publishes a real worker result within its budget tolerance');
    check(final.elapsed_ms <= 7000, 'five-second budget includes preparation and finishes within two-second scheduling tolerance');
    check(final.engine_used === 'rust' && final.search_mode === 'quick', 'Quick executes Rust/WASM');
    check(!final.error && !final.fallback && final.stop_reason !== 'error', 'Quick completes without fallback or an engine error');
    check(final.worker_count === 0 && final.deadline_timer === 0, 'finished Quick run releases its worker and timer');
    check(/longer search may find better/i.test(final.status), 'Quick result remains explicitly uncertified');
    checkWitness(final.top[0], 'finished Quick result', true);
    measurements.push({ scenario: 'quick_5s_small', first_result_ms: first.elapsed_ms,
        final_ms: final.elapsed_ms, leaf_calls: final.leaf_calls, stop_reason: final.stop_reason });
}

async function cancelRestartCase(page) {
    await load(page);
    await configure(page, { seconds: 15, wide: true });
    await start(page);
    await page.waitForFunction(() => _solver_state.running && _solver_state.leaf_calls > 0
        && _solver_state.workers[0]?._cur_top5?.length > 0 && _solver_state.top5.length > 0,
    null, { timeout: 13000 });
    // Invoke the real button synchronously to freeze the witnessed result, then
    // restart immediately. Only the late-message probe is synthetic: the old
    // callback is the one installed on the real worker, not a mock orchestrator.
    const transition = await page.evaluate(() => {
        const pack = r => ({ score: r.score,
            items: r.items.map(i => i.statMap.get('displayName') ?? i.statMap.get('name')),
            base_sp: r.base_sp, total_sp: r.total_sp, assigned_sp: r.assigned_sp,
            guild_tome_idx: r.guild_tome_idx, tome_names: r.tome_names });
        const before = pack(_solver_state.top5[0]);
        const oldId = _solver_state.run_id;
        const oldMessage = _solver_state.workers[0].worker.onmessage;
        const button = document.getElementById('solver-run-btn');
        button.click();
        const stopped = pack(_solver_state.top5[0]);
        const stoppedReason = _solver_state.stop_reason;
        const override = JSON.parse(JSON.stringify(_solver_sp_override));
        document.getElementById('solver-quick-budget').value = '5';
        button.click();
        const newId = _solver_state.run_id;
        oldMessage({ data: { type: 'done', stop_reason: 'stale-probe', leaf_calls: 999999999,
            top_n: [{ ...before, item_names: before.items, score: 1e99 }] } });
        return { before, stopped, stoppedReason, override, oldId, newId,
            running: _solver_state.running, stop_reason: _solver_state.stop_reason,
            staleAccepted: _solver_state.top5.some(r => r.score === 1e99)
                || _solver_state.leaf_calls === 999999999 };
    });
    checkWitness(transition.stopped, 'cancelled Quick result', true);
    check(JSON.stringify(transition.before) === JSON.stringify(transition.stopped),
        'Stop preserves the exact score, equipment, SP and tome witness');
    check(transition.stoppedReason === 'stopped', 'Stop records cancellation');
    check(JSON.stringify(transition.override.base_sp) === JSON.stringify(transition.stopped.base_sp)
        && JSON.stringify(transition.override.total_sp) === JSON.stringify(transition.stopped.total_sp),
    'Stop applies the witnessed SP allocation to the displayed build');
    check(transition.newId > transition.oldId && transition.running && !transition.staleAccepted
        && transition.stop_reason !== 'stale-probe', 'immediate restart ignores the previous worker callback');
    await page.waitForFunction(() => !_solver_state.running
        || (_solver_state.leaf_calls > 0 && _solver_state.workers.length === 1),
    null, { timeout: 7500 });
    const restarted = await waitFinished(page);
    check(!restarted.error && restarted.top.length > 0, 'immediate restart produces a valid result');
    checkWitness(restarted.top[0], 'restarted Quick result', true);
    measurements.push({ scenario: 'cancel_restart', final_ms: restarted.elapsed_ms,
        leaf_calls: restarted.leaf_calls, stop_reason: restarted.stop_reason });
}

async function unavailableCase(page) {
    await load(page);
    await configure(page);
    const workersBefore = diagnostics.worker_urls.length;
    await page.evaluate(() => { window.SOLVER_RUST_ENGINE = false; });
    await start(page);
    const result = await waitFinished(page, 3000);
    check(result.stop_reason === 'error' && /requires Rust\/WASM/i.test(result.error),
        'unavailable engine produces a visible actionable error');
    check(diagnostics.worker_urls.length === workersBefore && result.leaf_calls === 0,
        'unavailable Quick engine never starts a JavaScript fallback worker');

    // A set weapon used to be refused here as unmodelled. Both engines now
    // count the weapon toward its set (roadmap C1), so Quick runs it on the
    // Rust engine like any other build and still never falls back to JS.
    await page.evaluate(() => { window.SOLVER_RUST_ENGINE = true; });
    await page.locator('#weapon-choice').fill('Infused Hive Spear');
    await page.locator('#weapon-choice').press('Tab');
    await page.waitForFunction(() => solver_item_final_nodes[8]?.value?.statMap.get('set') === 'Master Hive',
        null, { timeout: 10000 });
    await start(page);
    const setWeapon = await waitFinished(page, 60000);
    check(setWeapon.stop_reason !== 'error' && setWeapon.leaf_calls > 0,
        `a set weapon runs on Quick search (stop_reason=${setWeapon.stop_reason}, error=${setWeapon.error ?? ''})`);
    const newWorkers = diagnostics.worker_urls.slice(workersBefore);
    check(!newWorkers.some(u => /engine\/worker\.js/.test(u)),
        `a set-weapon Quick search never starts the JavaScript engine worker (${newWorkers.join(', ')})`);
}

async function exhaustiveCase(page) {
    await load(page);
    await configure(page, { quick: false, tomes: false });
    await start(page);
    const result = await waitFinished(page, 20000);
    check(result.search_mode === 'exhaustive' && result.engine_used === 'rust',
        'default Exhaustive search still executes the existing Rust path');
    check(result.deadline_timer === 0 && !result.error && result.top.length > 0,
        'small Exhaustive search completes without inheriting a Quick deadline');
    checkWitness(result.top[0], 'Exhaustive result', false);
    measurements.push({ scenario: 'exhaustive_one_slot', final_ms: result.elapsed_ms });
}

(async () => {
    fs.mkdirSync(ARTIFACTS, { recursive: true });
    let server = null;
    if (!BASE_URL) {
        server = await serve(REPO_ROOT);
        BASE_URL = `http://127.0.0.1:${server.address().port}`;
    }
    const { chromium, executablePath } = requireBrowser();
    const browser = await chromium.launch({ executablePath, args: ['--no-sandbox'] });
    const context = await browser.newContext();
    await context.tracing.start({ screenshots: true, snapshots: true, sources: true });
    const page = await context.newPage();
    page.on('pageerror', error => diagnostics.browser_errors.push(error.message));
    page.on('worker', worker => diagnostics.worker_urls.push(worker.url()));
    let failure;
    try {
        await deadlineCase(page);
        await cancelRestartCase(page);
        await unavailableCase(page);
        await exhaustiveCase(page);
        const fatal = diagnostics.browser_errors.filter(message => !/fonts|favicon|Google/i.test(message));
        check(fatal.length === 0, `no browser script errors (${fatal.join('; ') || 'none'})`);
        console.log(`\n${assertions} browser assertions passed`);
    } catch (error) {
        failure = error;
        diagnostics.failure = error.stack || String(error);
        diagnostics.state = await state(page).catch(() => null);
        await page.screenshot({ path: path.join(ARTIFACTS, 'failure.png'), fullPage: true }).catch(() => {});
    } finally {
        fs.writeFileSync(path.join(ARTIFACTS, 'results.json'), JSON.stringify({ assertions, ...diagnostics }, null, 2));
        await context.tracing.stop({ path: path.join(ARTIFACTS, 'trace.zip') });
        await browser.close();
        if (server) server.close();
    }
    if (failure) throw failure;
})().catch(error => { console.error(error.stack || error); process.exitCode = 1; });
