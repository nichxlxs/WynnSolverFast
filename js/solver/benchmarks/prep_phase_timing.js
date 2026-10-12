// Prep-phase timing (roadmap R27): where the time between clicking Solve
// and the engine starting goes, stage by stage, in a real page.
//
// Wraps the page's own stage functions (pools, damage weights, candidate
// reduction, fixture builds, worker posts) with performance.now() marks and
// prints them relative to the click, two repeats per scenario. PROFILE=1
// also records a CDP CPU profile and prints self and inclusive time.
//
// Usage: node js/solver/benchmarks/prep_phase_timing.js '[{"name":"wide",
//   "target":"combo_damage","free":["helmet",...],"threads":1,"stop":true}]'
// `stop` cancels once the search reports progress (for spaces too large to
// finish). Needs Chromium (as browser_e2e.js) and playwright-core.
'use strict';
const http = require('http'), fs = require('fs'), path = require('path');
const REPO = path.resolve(__dirname, '../../..');
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.json': 'application/json', '.wasm': 'application/wasm', '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png' };
function serve(root) {
    const server = http.createServer((req, res) => {
        const rel = decodeURIComponent(req.url.split('?')[0]).replace(/^\/+/, '');
        const file = path.join(root, rel);
        if (!file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); res.end(); return; }
        res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream' });
        fs.createReadStream(file).pipe(res);
    });
    return new Promise((r) => server.listen(0, '127.0.0.1', () => r(server)));
}
const HASH = fs.readFileSync(path.join(REPO, 'js/solver/tests/e2e_hash.txt'), 'utf8').trim();
const ARMOUR = ['helmet', 'chestplate', 'leggings', 'boots'];
const ACC = ['ring1', 'ring2', 'bracelet', 'necklace'];
let seq = 0;
async function load(page, url) {
    const [base, hash] = url.split('#');
    await page.goto(`${base}?p=${++seq}#${hash}`, { waitUntil: 'load', timeout: 90000 });
    await page.waitForFunction(() => { const b = document.getElementById('solver-run-btn'); return b && !b.disabled; }, null, { timeout: 120000 });
    await page.waitForTimeout(3000);
}
async function setup(page, cfg) {
    await page.evaluate((cfg) => {
        const mb = document.getElementById('combo-mana-btn');
        if (mb && mb.classList.contains('toggleOn')) mb.click();
        const tgt = document.getElementById('solver-target');
        tgt.value = cfg.target; tgt.dispatchEvent(new Event('change'));
        for (const slot of cfg.free) { const i = document.getElementById(slot + '-choice'); i.value = ''; i.dispatchEvent(new Event('change')); }
        const th = document.getElementById('solver-thread-count');
        th.value = String(cfg.threads); th.dispatchEvent(new Event('change'));
        const sel = document.getElementById('solver-engine'); sel.value = 'rust'; sel.dispatchEvent(new Event('change'));
    }, cfg);
    await page.waitForTimeout(2500);
}
async function measure(page, stop) {
    await page.evaluate(() => {
        const T = window.__T = { marks: [], sizes: {} };
        const mark = (n) => T.marks.push([n, performance.now()]);
        const wrap = (obj, name, label) => {
            const f = obj[name]; if (typeof f !== 'function') return;
            obj[name] = function (...a) { mark(label + '>'); const r = f.apply(this, a); mark(label + '<');
                if (label === 'buildEnumFixture') T.sizes.enum = r.length;
                if (label === 'buildScoreFixture' && r && typeof r === 'object' && !r.then) T.sizes.score = JSON.stringify(r).length;
                return r; };
        };
        for (const n of ['get_restrictions', '_build_solver_snapshot', '_collect_locked_items', '_build_item_pools', '_build_dmg_weights', '_display_priority_weights', '_prepare_candidate_search_stage', '_eval_current_build', 'solver_engine_changed', '_run_solver_search_workers', '_build_worker_init_msg', '_try_run_solver_search_rust', '_rust_solve_in_worker'])
            wrap(window, n, n);
        wrap(window.__solver_rust_bridge, 'buildEnumFixture', 'buildEnumFixture');
        wrap(window.__solver_rust_bridge, 'buildScoreFixture', 'buildScoreFixture');
        const pm = Worker.prototype.postMessage;
        Worker.prototype.postMessage = function (m, ...r) { if (m && (m.type === 'session' || m.type === 'solve')) mark('post:' + m.type); return pm.call(this, m, ...r); };
        let phase = '';
        T.poll = setInterval(() => {
            const p = _solver_state.engine_phase || '';
            if (p !== phase) { mark('phase:' + p); phase = p; }
            if (_solver_state.checked > 0 && !T.firstChecked) { T.firstChecked = true; mark('checked>0'); }
        }, 2);
        mark('click');
        document.getElementById('solver-run-btn').click();
        mark('click-returned');
    });
    if (stop) {
        await page.waitForFunction(() => _solver_state.checked > 0, null, { timeout: 600000 });
        await page.evaluate(() => document.getElementById('solver-run-btn').click());
        await page.waitForTimeout(500);
    } else {
        await page.waitForFunction(() => !_solver_state.running && _solver_state.top5.length > 0, null, { timeout: 600000 });
    }
    return page.evaluate(() => { clearInterval(__T.poll); return { marks: __T.marks, sizes: __T.sizes, checked: _solver_state.checked }; });
}
(async () => {
    const { chromium } = require('playwright-core');
    const server = await serve(REPO);
    const url = `http://127.0.0.1:${server.address().port}/solver/index.html${HASH}`;
    const browser = await chromium.launch({ executablePath: ['/opt/pw-browsers/chromium-1194/chrome-linux/chrome', '/opt/pw-browsers/chromium/chrome-linux/chrome'].find(p => fs.existsSync(p)), headless: true });
    const page = await browser.newPage();
    const scen = JSON.parse(process.argv[2] || '[]');
    for (const sc of scen) {
        for (let rep = 0; rep < 2; rep++) {
            await load(page, url);
            await setup(page, sc);
            let cdp = null;
            if (process.env.PROFILE) { cdp = await page.context().newCDPSession(page); await cdp.send('Profiler.enable'); await cdp.send('Profiler.setSamplingInterval', { interval: 100 }); await cdp.send('Profiler.start'); }
            const r = await measure(page, sc.stop);
            if (cdp) {
                const { profile } = await cdp.send('Profiler.stop');
                // self time by function, and inclusive time under the two big prep stages
                const byId = new Map(profile.nodes.map(n => [n.id, n]));
                const parent = new Map(); for (const n of profile.nodes) for (const c of (n.children || [])) parent.set(c, n.id);
                const dt = profile.timeDeltas; const self = new Map(); const incl = new Map();
                profile.samples.forEach((id, i) => {
                    const d = (dt[i + 1] ?? 0) / 1000;
                    const n = byId.get(id); const k = n.callFrame.functionName + ' ' + n.callFrame.url.split('/').pop() + ':' + n.callFrame.lineNumber;
                    self.set(k, (self.get(k) || 0) + d);
                    const seen = new Set(); let cur = id;
                    while (cur !== undefined) { const m = byId.get(cur); const kk = m.callFrame.functionName + ' ' + m.callFrame.url.split('/').pop() + ':' + m.callFrame.lineNumber; if (!seen.has(kk)) { incl.set(kk, (incl.get(kk) || 0) + d); seen.add(kk); } cur = parent.get(cur); }
                });
                const top = (m) => [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, 40).map(([k, v]) => '     ' + v.toFixed(1) + 'ms ' + k).join('\n');
                console.log('  SELF\n' + top(self)); console.log('  INCLUSIVE\n' + top(incl));
            }
            const t0 = r.marks.find(m => m[0] === 'click')[1];
            console.log(`${sc.name} rep${rep} enum=${r.sizes.enum} score=${r.sizes.score} checked=${r.checked}`);
            console.log('   ' + r.marks.map(([n, t]) => `${n}@${(t - t0).toFixed(0)}`).join(' '));
        }
    }
    await browser.close(); server.close();
})().catch(e => { console.error(e); process.exit(1); });
