// Guards that the Rust engine is actually REACHABLE from the solver page.
//
// `search.js` reaches the Rust engine through `window.__solver_rust_bridge`.
// That lookup is the only route to it, and for the whole life of this branch
// it returned undefined: `solver/index.html` never loaded `rust_bridge.js`,
// and the file only assigned `module.exports`. So the browser silently ran
// the JS engine every time, and every "the Rust engine is 651x faster in the
// browser" measurement had gone through a test harness that loaded the bridge
// itself rather than through the page a user opens.
//
// Nothing in the Node test suite could catch that, because Node tests require
// the module directly — exactly the path that worked. It needs a real browser
// loading the real page.
//
// Run: node js/solver/tests/test_browser_rust_bridge.js
// Skips (rather than fails) when no Chromium is available, so the suite still
// runs in environments without one.

'use strict';

const http = require('http');
const fs = require('fs');
const path = require('path');
const { TestRunner, REPO_ROOT } = require('./harness');

const t = new TestRunner('Browser: Rust bridge reachable');

const MIME = {
    '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
    '.json': 'application/json', '.wasm': 'application/wasm',
    '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png',
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
    return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
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
    try { ({ chromium } = require('playwright-core')); }
    catch { console.log('  SKIP: playwright-core not installed'); t.summary(); return; }

    const exe = chromiumPath();
    if (!exe) { console.log('  SKIP: no Chromium binary found'); t.summary(); return; }

    const server = await serve(REPO_ROOT);
    const port = server.address().port;
    const browser = await chromium.launch({ executablePath: exe, args: ['--no-sandbox'] });
    try {
        const page = await browser.newPage();
        const errors = [];
        page.on('pageerror', (e) => errors.push(String(e.message || e)));

        await page.goto(`http://127.0.0.1:${port}/solver/index.html`,
                        { waitUntil: 'load', timeout: 60000 });

        // The bridge is a plain script, so it is present the moment the page
        // has loaded — no need to wait for game data.
        const bridge = await page.evaluate(() => {
            const b = window.__solver_rust_bridge;
            if (!b) return null;
            return Object.keys(b).filter((k) => typeof b[k] === 'function').sort();
        });

        t.assert(bridge !== null,
            'window.__solver_rust_bridge must exist on the solver page — it is the '
            + 'only route search.js has to the Rust engine, and without it the page '
            + 'silently falls back to the JS engine');

        for (const fn of ['buildEnumFixture', 'buildScoreFixture', 'browserEnv']) {
            t.assert(!!bridge && bridge.includes(fn),
                `the bridge must expose ${fn}() — search.js calls it directly`);
        }

        // The script must load before search.js, or the lookup runs too early.
        const order = await page.evaluate(() => {
            const srcs = [...document.querySelectorAll('script[src]')].map((s) => s.getAttribute('src'));
            return { bridge: srcs.findIndex((s) => s.includes('rust_bridge.js')),
                     search: srcs.findIndex((s) => s.includes('search.js')) };
        });
        t.assert(order.bridge >= 0 && order.bridge < order.search,
            `rust_bridge.js must be loaded before search.js (bridge at ${order.bridge}, `
            + `search at ${order.search})`);

        const fatal = errors.filter((e) => !/Google|fonts|net::ERR/i.test(e));
        t.assert(fatal.length === 0, `no page errors on load — got: ${fatal.join(' | ')}`);
    } finally {
        await browser.close();
        server.close();
    }
    t.summary();
})().catch((e) => { console.error(e); process.exit(1); });
