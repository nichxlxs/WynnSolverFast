// Does the COI service worker actually cross-origin isolate the page?
//
// `SharedArrayBuffer` needs COOP/COEP, and GitHub Pages cannot set headers.
// The service worker supplies them. This test serves the site with NO
// COOP/COEP at all — exactly the Pages situation — so if the page ends up
// isolated, it is the worker that did it.
//
// It also guards the two ways this can go wrong on a live site:
//   * isolation must not break the page (the fonts are cross-origin, which
//     is why the worker uses `credentialless` rather than `require-corp`),
//   * `?nocoi=1` must unregister it, so a bad worker is recoverable.
//
// Run: node js/solver/tests/test_browser_coi.js

'use strict';

const http = require('http');
const fs = require('fs');
const path = require('path');
const { TestRunner, REPO_ROOT } = require('./harness');

const t = new TestRunner('Browser: cross-origin isolation');

const MIME = {
    '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
    '.json': 'application/json', '.wasm': 'application/wasm',
    '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png',
    '.ico': 'image/x-icon', '.woff2': 'font/woff2',
};

/** Deliberately sets no COOP/COEP — the point is that the worker adds them. */
function serve(root) {
    const server = http.createServer((req, res) => {
        const rel = decodeURIComponent(req.url.split('?')[0]).replace(/^\/+/, '');
        const file = path.join(root, rel);
        if (!file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
            res.writeHead(404); res.end('not found'); return;
        }
        res.writeHead(200, {
            'Content-Type': MIME[path.extname(file)] || 'application/octet-stream',
            // Service workers must not be served from cache during the test.
            'Cache-Control': 'no-store',
        });
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

    const server = await serve(REPO_ROOT);
    const port = server.address().port;
    const base = `http://127.0.0.1:${port}/solver/index.html`;
    // 127.0.0.1 counts as a secure context, so service workers are allowed.
    const browser = await chromium.launch({ executablePath: exe, args: ['--no-sandbox'] });
    try {
        const page = await browser.newPage();
        const errors = [];
        page.on('pageerror', (e) => errors.push(String(e.message || e)));

        // First load registers the worker; it takes control and reloads.
        await page.goto(base, { waitUntil: 'load', timeout: 60000 });
        await page.waitForFunction(() => window.crossOriginIsolated === true,
                                   null, { timeout: 30000 }).catch(() => {});

        const state = await page.evaluate(() => ({
            isolated: window.crossOriginIsolated === true,
            hasSAB: typeof SharedArrayBuffer !== 'undefined',
            controlled: !!navigator.serviceWorker.controller,
        }));

        t.assert(state.controlled,
            'the COI service worker takes control of the page — without it the '
            + 'headers are never added and isolation cannot happen');
        t.assert(state.isolated,
            'the page is cross-origin isolated even though the server sends no '
            + 'COOP/COEP, which is the GitHub Pages situation');
        t.assert(state.hasSAB,
            'SharedArrayBuffer is available once isolated — this is the whole '
            + 'point of the worker');

        // Allocating one is the real check; the constructor can exist while
        // construction throws in a non-isolated context.
        const sabWorks = await page.evaluate(() => {
            try { return new SharedArrayBuffer(8).byteLength === 8; }
            catch (e) { return false; }
        });
        t.assert(sabWorks, 'a SharedArrayBuffer can actually be allocated');

        // Isolation must not cost the page its cross-origin resources. This is
        // why the worker uses `credentialless`: under `require-corp` the Google
        // Fonts stylesheet carries no CORP header and would be blocked.
        //
        // Measured COMPARATIVELY, against the same fetch on a non-isolated
        // page, because a sandboxed CI box may have no route to the font CDN
        // at all — and then an absolute "it loads" assertion fails for a
        // reason that has nothing to do with COEP. What matters is that
        // isolation does not make it worse.
        const probeFont = async (p) => p.evaluate(async () => {
            const links = [...document.querySelectorAll('link[rel="stylesheet"]')]
                .map((l) => l.href).filter((h) => h.startsWith('http'));
            if (!links.length) return 'none';
            try {
                const r = await fetch(links[0], { mode: 'no-cors' });
                return (r.type === 'opaque' || r.ok) ? 'ok' : `status ${r.status}`;
            } catch (e) { return `blocked: ${e.message}`; }
        });
        const isolatedFont = await probeFont(page);

        const plainCtx = await browser.newContext();
        const plain = await plainCtx.newPage();
        await plain.goto(`${base}?nocoi=1`, { waitUntil: 'load', timeout: 60000 });
        const plainFont = await probeFont(plain);
        const plainIsolated = await plain.evaluate(() => window.crossOriginIsolated === true);
        await plainCtx.close();

        t.assert(!plainIsolated,
            'the ?nocoi=1 baseline page is genuinely NOT isolated, so the '
            + 'comparison below is between two different states');
        t.assert(isolatedFont === plainFont,
            `isolation does not change how cross-origin stylesheets load `
            + `(isolated: ${isolatedFont}, not isolated: ${plainFont}) — `
            + `require-corp would block these, credentialless does not`);
        if (isolatedFont !== 'ok') {
            console.log(`  note: this environment cannot reach the font CDN `
                + `(${plainFont}); the check above still holds because it is relative`);
        }

        // The page must still come up. If isolation broke the app, the run
        // button would never enable.
        const ready = await page.waitForFunction(
            () => { const b = document.getElementById('solver-run-btn');
                    return b && !b.disabled; }, null, { timeout: 120000 })
            .then(() => true).catch(() => false);
        t.assert(ready, 'the solver page still finishes loading under isolation');

        // A bad worker on a live site must be recoverable.
        const p2 = await browser.newPage();
        await p2.goto(`${base}?nocoi=1`, { waitUntil: 'load', timeout: 60000 });
        await p2.waitForTimeout(2000);
        const regs = await p2.evaluate(
            () => navigator.serviceWorker.getRegistrations().then((r) => r.length));
        t.assert(regs === 0,
            `?nocoi=1 unregisters the worker (${regs} left) — without an escape `
            + `hatch a misbehaving service worker is unrecoverable for users`);

        const fatal = errors.filter((e) => !/fonts|net::ERR|favicon|Google/i.test(e));
        t.assert(fatal.length === 0, `no page errors: ${fatal.slice(0, 2).join(' | ') || 'none'}`);
    } finally {
        await browser.close();
        server.close();
    }
    t.summary();
})().catch((e) => { console.error(e); process.exit(1); });
