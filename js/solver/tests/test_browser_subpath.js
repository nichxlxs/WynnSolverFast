// The site must work when served from a SUBDIRECTORY, not just a domain root.
//
// GitHub Pages project sites live at `username.github.io/<repo>/`, so every
// absolute URL the app builds needs that `/<repo>` prefix. `SITE_BASE` supplies
// it. It used to be matched against a hardcoded
// /^\/(wynnbuilder(-beta)?\.github\.io)\// and fall back to '' — so under any
// other repo name the prefix was silently dropped, every fetch of the item
// database 404'd, and the page came up with no items at all: nothing to search,
// nothing to place in a slot. That is exactly what happened on the first real
// deploy.
//
// Every other browser test here serves the repo at `/`, where SITE_BASE=''
// happens to be correct, so none of them could see it. This one serves the repo
// under a prefix and checks the app still finds its data.
//
// Run: node js/solver/tests/test_browser_subpath.js

'use strict';

const http = require('http');
const fs = require('fs');
const path = require('path');
const { TestRunner, REPO_ROOT } = require('./harness');

const t = new TestRunner('Browser: served from a subdirectory');

// Deliberately not the repo's real name — the point is that NOTHING may be
// hardcoded to a particular repo.
const PREFIX = '/some-project-name';

const MIME = {
    '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
    '.json': 'application/json', '.wasm': 'application/wasm',
    '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png',
    '.ico': 'image/x-icon', '.woff2': 'font/woff2',
};

/** Serves the repo under PREFIX, exactly as a Pages project site does. */
function serve(root, misses) {
    const server = http.createServer((req, res) => {
        const url = decodeURIComponent(req.url.split('?')[0]);
        if (!url.startsWith(PREFIX + '/')) {
            misses.push(url);                    // would be a 404 on Pages
            res.writeHead(404); res.end('not found'); return;
        }
        const rel = url.slice(PREFIX.length + 1);
        const file = path.join(root, rel);
        if (!file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
            misses.push(url);
            res.writeHead(404); res.end('not found'); return;
        }
        res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream' });
        fs.createReadStream(file).pipe(res);
    });
    return new Promise((r) => server.listen(0, '127.0.0.1', () => r(server)));
}

function chromiumPath() {
    for (const p of ['/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
                     '/opt/pw-browsers/chromium/chrome-linux/chrome',
                     process.env.CHROMIUM_PATH]) {
        if (p && fs.existsSync(p)) return p;
    }
    return null;
}

(async () => {
    let chromium;
    const pkgDir = path.join(REPO_ROOT, 'node_modules', 'playwright-core');
    try {
        ({ chromium } = require('playwright-core'));
    } catch (e) {
        if (!fs.existsSync(pkgDir)) {
            console.log('  SKIP: playwright-core not installed (npm i)');
            t.summary(); return;
        }
        t.assert(false, `playwright-core present but will not load: ${e && e.message}`);
        t.summary(); return;
    }
    const exe = chromiumPath();
    if (!exe) { console.log('  SKIP: no Chromium binary found'); t.summary(); return; }

    const misses = [];
    const server = await serve(REPO_ROOT, misses);
    const port = server.address().port;
    const browser = await chromium.launch({ executablePath: exe, args: ['--no-sandbox'] });
    try {
        const page = await browser.newPage();
        const failed = [];
        page.on('requestfailed', (r) => failed.push(r.url()));
        page.on('response', (r) => { if (r.status() === 404) failed.push(r.url()); });

        await page.goto(`http://127.0.0.1:${port}${PREFIX}/solver/index.html`,
                        { waitUntil: 'load', timeout: 90000 });

        const base = await page.evaluate(() => (typeof SITE_BASE === 'string' ? SITE_BASE : null));
        t.assert(base === PREFIX,
            `SITE_BASE picks up the subdirectory (got ${JSON.stringify(base)}, `
            + `expected ${JSON.stringify(PREFIX)}) — if this is '' every absolute `
            + `URL loses the prefix and 404s`);

        // The real symptom: the item database never loads, so the page has
        // nothing to search and nothing to place in a slot.
        const ready = await page.waitForFunction(
            () => { const b = document.getElementById('solver-run-btn');
                    return b && !b.disabled; }, null, { timeout: 120000 })
            .then(() => true).catch(() => false);
        t.assert(ready,
            'the page finishes loading its item database under a subdirectory — '
            + 'the run button only enables once items are populated');

        // `itemMap` is a script-scope binding, not a window property, so it is
        // read through eval. Waited for rather than sampled: the run button
        // enables a moment before the map finishes filling, and sampling it
        // immediately reports 0 on a perfectly healthy page.
        const itemCount = await page.waitForFunction(() => {
            try { const m = eval('itemMap'); return (m && m.size > 100) ? m.size : false; }
            catch (e) { return false; }
        }, null, { timeout: 60000 }).then((h) => h.jsonValue()).catch(() => 0);
        t.assert(itemCount > 100,
            `items actually loaded (${itemCount} in itemMap) — this is what "cannot `
            + `search items or put them in slots" looks like from the inside`);

        // Nothing may be requested from outside the prefix.
        const off_prefix = misses.filter((u) => !u.startsWith(PREFIX + '/'));
        t.assert(off_prefix.length === 0,
            `no request escapes the subdirectory (${off_prefix.length} did`
            + `${off_prefix.length ? ': ' + off_prefix.slice(0, 3).join(', ') : ''})`);

        // The default page must register NO service worker. Isolation buys
        // almost nothing (partitioning is flat on completable workloads, see
        // WASM.md) and a misbehaving worker persists across visits, so it is
        // opt-in via ?coi=1. Two deploys were broken by having it on by
        // default; this keeps it off.
        const swCount = await page.evaluate(
            () => navigator.serviceWorker
                ? navigator.serviceWorker.getRegistrations().then((r) => r.length) : 0);
        t.assert(swCount === 0,
            `the default page registers no service worker (${swCount} found) — `
            + `isolation is opt-in via ?coi=1`);

        const real404 = failed.filter((u) => !/fonts|gstatic|favicon/i.test(u));
        t.assert(real404.length === 0,
            `no 404s under the subdirectory (${real404.slice(0, 3).join(', ') || 'none'})`);
    } finally {
        await browser.close();
        server.close();
    }
    t.summary();
})().catch((e) => { console.error(e); process.exit(1); });
