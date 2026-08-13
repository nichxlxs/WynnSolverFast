// End-to-end browser check: run a real search on BOTH engines through the
// actual solver page and compare what the UI ends up holding.
//
// This exists because the Node suite structurally cannot cover this path.
// Node tests `require()` the bridge directly, which is exactly the route that
// worked while the page's own route was dead for the whole life of the
// branch. Only a real browser loading the real page exercises what a user
// gets.
//
// Not part of run_all.js: it needs a built wasm bundle and takes ~1 min.
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

let pass = 0, fail = 0;
const ok = (cond, msg) => { if (cond) { pass++; console.log(`  PASS ${msg}`); }
                            else { fail++; console.log(`  FAIL ${msg}`); } };

async function runEngine(page, engine, url) {
    await page.goto(url, { waitUntil: 'load', timeout: 90000 });
    // The page enables the run button once the item DB is populated; that is
    // the readiness signal the UI itself uses.
    await page.waitForFunction(
        () => { const b = document.getElementById('solver-run-btn');
                return b && !b.disabled; }, null, { timeout: 120000 });
    await page.waitForTimeout(3000);   // let the atree/UI settle
    // Scenario setup. The README build cannot sustain its own combo, so with
    // the mana constraint on, EVERY candidate is rejected — by both engines
    // identically, which is correct but tests nothing about results. Turning
    // the mana toggle off makes combo_time 0 so the check passes trivially
    // and real builds come back.
    await page.evaluate(() => {
        const mb = document.getElementById('combo-mana-btn');
        if (mb && mb.classList.contains('toggleOn')) mb.click();
        const tgt = document.getElementById('solver-target');
        // Must be a value the select actually offers: assigning an unknown
        // one leaves it empty, and the scorer then returns 0 for every build,
        // which looks like agreement while testing nothing.
        if (tgt) { tgt.value = 'ehp'; tgt.dispatchEvent(new Event('change')); }
        for (const slot of ['helmet', 'chestplate', 'leggings', 'boots']) {
            const i = document.getElementById(slot + '-choice');
            if (i) { i.value = ''; i.dispatchEvent(new Event('change')); }
        }
    });
    await page.waitForTimeout(2500);

    await page.evaluate((eng) => {
        const sel = document.getElementById('solver-engine');
        if (sel) { sel.value = eng; sel.dispatchEvent(new Event('change')); }
    }, engine);

    const chosen = await page.evaluate(() => {
        const sel = document.getElementById('solver-engine');
        return sel ? sel.value : null;
    });

    await page.evaluate(() => {
        const btn = document.getElementById('solver-run-btn');
        if (btn) btn.click();
    });

    // `_solver_state` is a script-scope const, reachable as a bare identifier.
    // Confirm the search actually STARTED before waiting for it to finish,
    // otherwise a run that never begins looks the same as one that ended.
    await page.waitForFunction(() => _solver_state.running, null, { timeout: 60000 });
    const t0 = Date.now();
    await page.waitForFunction(
        () => !_solver_state.running && _solver_state.top5.length > 0,
        null, { timeout: 300000 });

    const elapsed_ms = Date.now() - t0;
    return page.evaluate(() => ({
        checked: _solver_state.checked,
        top: _solver_state.top5.slice(0, 15).map((r) => ({
            score: r.score,
            items: r.items.map((i) => i.statMap.get('displayName') ?? i.statMap.get('name')),
            base_sp: r.base_sp, total_sp: r.total_sp, assigned_sp: r.assigned_sp,
        })),
    })).then((r) => ({ ...r, chosen, elapsed_ms }));
}

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

        console.log('— Rust engine —');
        const rust = await runEngine(page, 'rust', url);
        console.log(`  engine=${rust.chosen} checked=${rust.checked} results=${rust.top.length} `
            + `wall=${rust.elapsed_ms}ms`);

        console.log('— JS engine —');
        const js = await runEngine(page, 'javascript', url);
        console.log(`  engine=${js.chosen} checked=${js.checked} results=${js.top.length} `
            + `wall=${js.elapsed_ms}ms`);

        ok(rust.top.length > 0, 'Rust engine returned results through the page');
        ok(js.top.length > 0, 'JS engine returned results through the page');

        const scores = (r) => r.top.map((x) => x.score.toFixed(6)).join(',');
        ok(scores(rust) === scores(js),
           `both engines agree on the top-N scores\n     rust: ${scores(rust).slice(0, 90)}\n     js:   ${scores(js).slice(0, 90)}`);

        const zeroSp = rust.top.filter((r) =>
            !r.total_sp || r.total_sp.every((v) => v === 0));
        ok(zeroSp.length === 0,
           `Rust results carry a real SP assignment (${zeroSp.length}/${rust.top.length} were zeroed)`);

        // Match by build identity, not position: equal-scoring builds may sit
        // in a different order between engines (documented tie behaviour), so
        // a positional compare would fail on a difference that is not one.
        const jsByBuild = new Map(js.top.map((r) => [r.items.join('|'), r]));
        const compared = rust.top.filter((r) => jsByBuild.has(r.items.join('|')));
        const spMatch = compared.every((r) => {
            const j = jsByBuild.get(r.items.join('|'));
            return JSON.stringify(r.total_sp) === JSON.stringify(j.total_sp)
                && JSON.stringify(r.base_sp) === JSON.stringify(j.base_sp);
        });
        ok(compared.length > 0 && spMatch,
           `the SP assignment matches the JS engine build-for-build `
           + `(${compared.length}/${rust.top.length} builds common to both lists)`);

        const nonZero = rust.top.filter((r) => r.score !== 0).length;
        ok(nonZero > 0, `scores are real, not a degenerate all-zero tie (${nonZero}/${rust.top.length} non-zero)`);

        if (rust.checked === js.checked && rust.elapsed_ms > 0) {
            console.log(`  speedup: ${(js.elapsed_ms / rust.elapsed_ms).toFixed(1)}x `
                + `on ${rust.checked.toLocaleString()} leaves (same space, same results)`);
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
