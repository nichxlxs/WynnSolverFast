// End-to-end browser check: run real searches on BOTH engines through the
// actual solver page and compare what the UI ends up holding.
//
// This exists because the Node suite structurally cannot cover this path.
// Node tests `require()` the bridge directly, which is exactly the route that
// worked while the page's own route was dead for the whole life of the
// branch. Only a real browser loading the real page exercises what a user
// gets.
//
// Phases:
//   1. total_hp     — both engines, full completion, scores + SP compared
//   2. combo_damage — the damage pipeline (mana sim, atree, spell rows)
//   3. partitioning — Rust at 1 worker vs 4, same space, same results
//   4. cancellation — stop mid-search on both engines, then solve again
//
// Not part of run_all.js: it needs a built wasm bundle and takes ~3 min.
// Run: node js/solver/tests/browser_e2e.js [--headed]

'use strict';

const http = require('http');
const fs = require('fs');
const path = require('path');
const { REPO_ROOT } = require('./harness');

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
        // The worker is a module worker; wasm needs its real type.
        res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream' });
        fs.createReadStream(file).pipe(res);
    });
    return new Promise((r) => server.listen(0, '127.0.0.1', () => r(server)));
}

const CHROMIUM = ['/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
                  '/opt/pw-browsers/chromium/chrome-linux/chrome']
    .find((p) => fs.existsSync(p));

// A small, fast, fully-enumerable scenario. Same build the README uses.
const BUILD_HASH = process.env.E2E_HASH
    || fs.readFileSync(path.join(__dirname, 'e2e_hash.txt'), 'utf8').trim();

const ARMOUR = ['helmet', 'chestplate', 'leggings', 'boots'];
const ACCESSORIES = ['ring1', 'ring2', 'bracelet', 'necklace'];

let pass = 0, fail = 0;
const ok = (cond, msg) => { if (cond) { pass++; console.log(`  PASS ${msg}`); }
                            else { fail++; console.log(`  FAIL ${msg}`); } };

// ── Page helpers ─────────────────────────────────────────────────────────────

// Every load must be a REAL navigation. `page.goto()` to a URL that differs
// only in its hash is a same-document navigation: the document is not
// reloaded, so init scripts do not re-run and — worse — UI state carries over.
// That silently made two phases meaningless: the partition-threshold override
// never took effect, and a phase that freed the accessory slots left them free
// for every later phase, so a "fresh" search inherited a space too large to
// finish. A unique query string forces a genuine load each time.
let _loadSeq = 0;
async function loadPage(page, url) {
    const [base, hash] = url.split('#');
    const fresh = `${base}${base.includes('?') ? '&' : '?'}e2e=${++_loadSeq}`
        + (hash ? `#${hash}` : '');
    await page.goto(fresh, { waitUntil: 'load', timeout: 90000 });
    // The COI service worker reloads the page once, when it takes control, to
    // get cross-origin isolation. Let that settle before touching anything:
    // interacting first means the reload wipes the state mid-test.
    await page.waitForFunction(
        () => window.crossOriginIsolated === true
            || sessionStorage.getItem('coi-reloaded') === '1'
            || !navigator.serviceWorker,
        null, { timeout: 30000 }).catch(() => {});
    // The page enables the run button once the item DB is populated; that is
    // the readiness signal the UI itself uses.
    await page.waitForFunction(
        () => { const b = document.getElementById('solver-run-btn');
                return b && !b.disabled; }, null, { timeout: 120000 });
    await page.waitForTimeout(3000);   // let the atree/UI settle
}

/**
 * Configure the scenario. Returns the target the select actually holds, so the
 * caller can refuse to measure a run whose target silently failed to apply.
 */
async function setup(page, { target, freeSlots, manaOff, engine, threads }) {
    await page.evaluate((cfg) => {
        const mb = document.getElementById('combo-mana-btn');
        if (mb && cfg.manaOff && mb.classList.contains('toggleOn')) mb.click();
        if (mb && !cfg.manaOff && !mb.classList.contains('toggleOn')) mb.click();

        const tgt = document.getElementById('solver-target');
        if (tgt) { tgt.value = cfg.target; tgt.dispatchEvent(new Event('change')); }

        for (const slot of cfg.freeSlots) {
            const i = document.getElementById(slot + '-choice');
            if (i) { i.value = ''; i.dispatchEvent(new Event('change')); }
        }

        const th = document.getElementById('solver-thread-count');
        if (th && cfg.threads) { th.value = String(cfg.threads); th.dispatchEvent(new Event('change')); }
    }, { target, freeSlots, manaOff, threads });
    await page.waitForTimeout(2500);

    await page.evaluate((eng) => {
        const sel = document.getElementById('solver-engine');
        if (sel) { sel.value = eng; sel.dispatchEvent(new Event('change')); }
    }, engine);

    // A select silently drops an unknown value, leaving ''. Every build then
    // scores 0 and both engines "agree" on a list of zeroes. Fail loudly.
    const held = await page.evaluate(
        () => document.getElementById('solver-target')?.value ?? null);
    if (held !== target) {
        throw new Error(`target did not take: wanted '${target}', select holds '${held}'. `
            + `The option is missing from solver/index.html, so this run would have `
            + `compared two all-zero result lists.`);
    }
    return held;
}

const readState = (page) => page.evaluate(() => ({
    checked: _solver_state.checked,
    running: _solver_state.running,
    engine_used: _solver_state.engine_used,
    engine_fallback_reason: _solver_state.engine_fallback_reason,
    partitions: _solver_state.partitions,
    worker_count: _solver_state.workers.length,
    top: _solver_state.top5.slice(0, 15).map((r) => ({
        score: r.score,
        items: r.items.map((i) => i.statMap.get('displayName') ?? i.statMap.get('name')),
        base_sp: r.base_sp, total_sp: r.total_sp, assigned_sp: r.assigned_sp,
    })),
}));

/** Wait for the browser's live-worker list to empty; returns the final count. */
async function drainWorkers(page, timeout_ms) {
    const deadline = Date.now() + timeout_ms;
    while (Date.now() < deadline) {
        if (page.workers().length === 0) return 0;
        await page.waitForTimeout(250);
    }
    return page.workers().length;
}

async function clickRun(page) {
    await page.evaluate(() => {
        const btn = document.getElementById('solver-run-btn');
        if (btn) btn.click();
    });
    // Confirm the search actually STARTED before waiting for it to finish,
    // otherwise a run that never begins looks the same as one that ended.
    await page.waitForFunction(() => _solver_state.running, null, { timeout: 60000 });
}

async function runToCompletion(page) {
    await clickRun(page);
    const t0 = Date.now();
    await page.waitForFunction(
        () => !_solver_state.running && _solver_state.top5.length > 0,
        null, { timeout: 300000 });
    const elapsed_ms = Date.now() - t0;
    return { ...(await readState(page)), elapsed_ms };
}

const scores = (r) => r.top.map((x) => x.score.toFixed(6)).join(',');

/** Compare two runs by build identity, not position — equal scores may sit in
 *  a different order between engines (documented tie behaviour). */
function spMatches(a, b) {
    const byBuild = new Map(b.top.map((r) => [r.items.join('|'), r]));
    const common = a.top.filter((r) => byBuild.has(r.items.join('|')));
    const same = common.every((r) => {
        const j = byBuild.get(r.items.join('|'));
        return JSON.stringify(r.total_sp) === JSON.stringify(j.total_sp)
            && JSON.stringify(r.base_sp) === JSON.stringify(j.base_sp);
    });
    return { common: common.length, same };
}

// ── Phases ───────────────────────────────────────────────────────────────────

async function comparedRun(page, url, label, cfg) {
    console.log(`— ${label} —`);
    await loadPage(page, url);
    await setup(page, { ...cfg, engine: 'rust' });
    const rust = await runToCompletion(page);
    console.log(`  rust: engine_used=${rust.engine_used} checked=${rust.checked} `
        + `results=${rust.top.length} wall=${rust.elapsed_ms}ms`);

    await loadPage(page, url);
    await setup(page, { ...cfg, engine: 'javascript' });
    const js = await runToCompletion(page);
    console.log(`  js:   engine_used=${js.engine_used} checked=${js.checked} `
        + `results=${js.top.length} wall=${js.elapsed_ms}ms`);

    ok(rust.top.length > 0, `${label}: Rust returned results through the page`);
    ok(js.top.length > 0, `${label}: JS returned results through the page`);

    // Every Rust-path failure falls back to the JS workers, which return
    // correct results — so every comparison below would still pass if the Rust
    // engine never ran. That is not hypothetical: it is what happened for the
    // entire life of the branch. Assert the engine that actually ran.
    ok(rust.engine_used === 'rust',
       `${label}: the Rust run really used the Rust engine (engine_used=${rust.engine_used})`);
    ok(js.engine_used === 'javascript',
       `${label}: the JS run used the JS engine (engine_used=${js.engine_used})`);

    ok(scores(rust) === scores(js),
       `${label}: both engines agree on the top-N scores\n     rust: ${scores(rust).slice(0, 80)}\n     js:   ${scores(js).slice(0, 80)}`);

    const zeroSp = rust.top.filter((r) => !r.total_sp || r.total_sp.every((v) => v === 0));
    ok(zeroSp.length === 0,
       `${label}: Rust results carry a real SP assignment (${zeroSp.length}/${rust.top.length} zeroed)`);

    const sp = spMatches(rust, js);
    ok(sp.common > 0 && sp.same,
       `${label}: SP matches the JS engine build-for-build (${sp.common}/${rust.top.length} common)`);

    const nonZero = rust.top.filter((r) => r.score !== 0).length;
    ok(nonZero > 0, `${label}: scores are real, not a degenerate all-zero tie (${nonZero}/${rust.top.length})`);

    if (rust.checked === js.checked && rust.elapsed_ms > 0) {
        console.log(`  speedup: ${(js.elapsed_ms / rust.elapsed_ms).toFixed(1)}x on `
            + `${rust.checked.toLocaleString()} leaves (same space, same results)`);
    }
    return { rust, js };
}

/**
 * Rust at 1 worker vs 4: partitioning must not change the answer.
 *
 * Every scenario in this build that clears the 8M partition threshold is
 * astronomically large (freeing a fifth slot jumps the space from ~10^5 to
 * 10^11-10^15 and never completes), so the threshold is lowered for this phase
 * to let a completable scenario partition. The partition arithmetic is proven
 * exactly elsewhere (`partition_check`, 2..8 partitions on three fixtures);
 * what is browser-specific, and only testable here, is the orchestration —
 * disjoint ranges per worker, every worker reporting, and the merge.
 */
async function partitionPhase(page, url, cfg) {
    console.log('— partitioning (Rust, 1 worker vs 4) —');
    await page.addInitScript(() => { window.__SOLVER_TEST_PARTITION_MIN_SPACE = 1; });

    await loadPage(page, url);
    await setup(page, { ...cfg, engine: 'rust', threads: 1 });
    const one = await runToCompletion(page);
    console.log(`  threads=1: checked=${one.checked} partitions=${one.partitions} `
        + `wall=${one.elapsed_ms}ms`);

    await loadPage(page, url);
    await setup(page, { ...cfg, engine: 'rust', threads: 4 });
    const four = await runToCompletion(page);
    console.log(`  threads=4: checked=${four.checked} partitions=${four.partitions} `
        + `wall=${four.elapsed_ms}ms`);

    ok(one.engine_used === 'rust' && four.engine_used === 'rust',
       `partitioning: both runs used the Rust engine `
       + `(${one.engine_used}, ${four.engine_used})`);

    // If the 4-thread run did not actually split, the comparison is vacuous —
    // it would be comparing a 1-worker run against itself.
    ok(four.partitions > 1,
       `partitioning: the 4-thread run really split into multiple partitions `
       + `(partitions=${four.partitions}) — at 1 this phase proves nothing`);
    ok(one.partitions === 1,
       `partitioning: the 1-thread run really used a single partition `
       + `(partitions=${one.partitions})`);

    ok(one.checked === four.checked,
       `partitioning: both cover the same space (${one.checked} vs ${four.checked} leaves)`);
    ok(scores(one) === scores(four),
       `partitioning: identical top-N scores at 1 and 4 workers\n     1: ${scores(one).slice(0, 80)}\n     4: ${scores(four).slice(0, 80)}`);
    const sp = spMatches(one, four);
    ok(sp.common > 0 && sp.same,
       `partitioning: identical SP assignment (${sp.common}/${one.top.length} common)`);
}

/** Stop mid-search, then confirm the page is still usable. */
async function cancelPhase(page, url, engine, cfg) {
    console.log(`— cancellation (${engine}) —`);
    await loadPage(page, url);
    await setup(page, { ...cfg, engine });
    await clickRun(page);

    // Let it get properly under way, so we are cancelling real work rather
    // than racing the startup path.
    await page.waitForFunction(() => _solver_state.checked > 0 || !_solver_state.running,
                               null, { timeout: 60000 });
    await page.waitForTimeout(1200);
    const mid = await readState(page);
    // Live workers as the BROWSER sees them, not as the app reports them.
    const live_before = page.workers().length;

    await page.evaluate(() => {
        const btn = document.getElementById('solver-run-btn');
        if (btn) btn.click();          // same button toggles stop
    });
    await page.waitForTimeout(1500);
    const after = await readState(page);
    // Chromium tears workers down asynchronously: `page.workers()` keeps
    // listing a terminated worker for a second or two, so a fixed wait here
    // fails on correct code. Poll until the list drains. A worker that was
    // never terminated keeps running the search and never leaves the list, so
    // this still fails on the regression it is here to catch.
    const live_after = await drainWorkers(page, 20000);

    ok(mid.running,
       `cancel/${engine}: the search was still running when cancelled `
       + `(checked=${mid.checked})`);

    // Without this the phase is vacuous: if the Rust engine declines the wide
    // scenario it falls back to the JS workers, and a cancel of the JS engine
    // satisfies every other assertion here while proving nothing about Rust.
    ok(mid.engine_used === engine,
       `cancel/${engine}: the cancelled search really used the ${engine} engine `
       + `(engine_used=${mid.engine_used}${mid.engine_fallback_reason
            ? `, reason: ${mid.engine_fallback_reason}` : ''})`);

    ok(!after.running, `cancel/${engine}: the search stopped (running=${after.running})`);

    // `_solver_state.workers` is NOT evidence of termination: `_stop_solver()`
    // assigns `workers = []` unconditionally after its terminate loop, so an
    // assertion against it reads zero even if the loop were deleted or threw —
    // it would pass under the exact leaked-core regression it claims to catch.
    // `page.workers()` is the browser's own list of live dedicated workers, so
    // a worker that was never terminated still appears in it.
    ok(live_before > 0,
       `cancel/${engine}: the browser really had workers running before the stop `
       + `(${live_before})`);
    ok(live_after === 0,
       `cancel/${engine}: every worker was terminated (${live_after} still live in the `
       + `browser after 20s) — a leaked worker keeps burning a core after the user `
       + `hits stop`);

    // The real risk of a cancel bug is not the stop itself but the state it
    // leaves behind: the next search must still work. Reload first — freeing a
    // slot is not undone by re-clearing another, so without this the follow-up
    // inherits the wide space and never finishes.
    await loadPage(page, url);
    await setup(page, { ...cfg, engine, freeSlots: ARMOUR });
    const again = await runToCompletion(page);
    ok(again.top.length > 0 && !again.running,
       `cancel/${engine}: a fresh search runs correctly after a cancel `
       + `(${again.top.length} results, checked=${again.checked})`);
    ok(again.engine_used === engine,
       `cancel/${engine}: the post-cancel search also used the ${engine} engine `
       + `(engine_used=${again.engine_used})`);
}

// ── Main ─────────────────────────────────────────────────────────────────────

(async () => {
    if (!CHROMIUM) { console.log('SKIP: no chromium'); process.exit(0); }
    const { chromium } = require('playwright-core');
    const server = await serve(REPO_ROOT);
    const port = server.address().port;
    const url = `http://127.0.0.1:${port}/solver/index.html${BUILD_HASH}`;
    console.log(`url: ${url}\n`);

    const browser = await chromium.launch({ executablePath: CHROMIUM, args: ['--no-sandbox'] });
    try {
        const page = await browser.newPage();
        const errs = [];
        page.on('pageerror', (e) => errs.push(String(e.message || e)));
        page.on('console', (m) => { if (m.type() === 'error') errs.push(m.text()); });

        // 1. total_hp — this build's own benchmark target (solver_readme_armor4
        //    is the same build with the same four free armour slots).
        await comparedRun(page, url, 'total_hp', {
            target: 'total_hp', freeSlots: ARMOUR, manaOff: true,
        });

        // 2. combo_damage — the damage pipeline end to end: mana simulation,
        //    ability-tree scaling, spell rows, greedy SP damage trials. None of
        //    that is exercised by a defensive target.
        //
        //    Mana stays OFF: this build cannot sustain its own combo (the page's
        //    own editor says "Mana: 0/100 not sustainable (-100)"), so with the
        //    check on, every candidate is correctly rejected by both engines and
        //    the comparison degenerates to two empty lists.
        await comparedRun(page, url, 'combo_damage', {
            target: 'combo_damage', freeSlots: ARMOUR, manaOff: true,
        });

        // 3. Partitioning, on the four-armour space so the run completes; the
        //    threshold is lowered inside the phase (see its comment).
        await partitionPhase(page, url, {
            target: 'total_hp', freeSlots: ARMOUR, manaOff: true,
        });

        // 4. Cancellation, on the wide space: freeing the accessories makes it
        //    far too large to finish, which is exactly what a cancel test wants
        //    — the search is guaranteed to still be running when we stop it.
        //    (The lowered threshold from phase 3 persists, so the Rust run here
        //    is also partitioned — cancelling several workers, not just one.)
        for (const engine of ['rust', 'javascript']) {
            await cancelPhase(page, url, engine, {
                target: 'total_hp', freeSlots: [...ARMOUR, ...ACCESSORIES], manaOff: true,
            });
        }

        const fatal = errs.filter((e) => !/fonts|net::ERR|favicon/i.test(e));
        ok(fatal.length === 0, `no page errors (${fatal.slice(0, 2).join(' | ') || 'none'})`);

        console.log(`\n${pass} passed, ${fail} failed`);
        process.exit(fail ? 1 : 0);
    } finally {
        await browser.close();
        server.close();
    }
})().catch((e) => { console.error('E2E ERROR:', e.message); process.exit(1); });
