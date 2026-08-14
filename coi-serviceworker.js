// Cross-origin isolation for a statically-hosted site.
//
// `SharedArrayBuffer` — and therefore any shared state between the solver's
// Rust/WASM workers — requires the document to be cross-origin isolated,
// which requires two response headers:
//
//     Cross-Origin-Opener-Policy: same-origin
//     Cross-Origin-Embedder-Policy: credentialless
//
// GitHub Pages serves static files and cannot set either. A service worker
// can: it re-fetches each response and hands the page a copy with the headers
// attached. That is all this file does — it caches nothing.
//
// Two deliberate choices:
//
//   * `credentialless`, not `require-corp`. Under `require-corp` every
//     cross-origin subresource must carry `Cross-Origin-Resource-Policy`, and
//     the fonts this page loads (fonts.googleapis.com / fonts.gstatic.com) do
//     not. `credentialless` sends those no-cors requests without credentials
//     instead, which needs no cooperation from the other origin, so the page
//     keeps its fonts.
//   * Isolation is a bonus, never a requirement. If registration fails, the
//     browser lacks support, or the reload is suppressed, `crossOriginIsolated`
//     stays false and every caller falls back to the non-shared path.
//
// Escape hatch: load any page with `?nocoi=1` to unregister the worker and
// stop reloading. Nothing here persists beyond that.

'use strict';

// Captured at parse time: document.currentScript is only non-null while the
// script is executing synchronously, and the page-side code below reads it
// after an await.
const _COI_SRC = (typeof document !== 'undefined' && document.currentScript)
    ? document.currentScript.src : '';

if (typeof window === 'undefined') {
    // ── Service worker context ──────────────────────────────────────────────
    self.addEventListener('install', () => self.skipWaiting());
    self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));

    self.addEventListener('message', (ev) => {
        if (ev.data && ev.data.type === 'deregister') {
            self.registration.unregister()
                .then(() => self.clients.matchAll())
                .then((clients) => clients.forEach((c) => c.navigate(c.url)));
        }
    });

    self.addEventListener('fetch', (event) => {
        const r = event.request;
        // A range request re-fetched here would lose its 206 semantics, and
        // no-store responses must not be re-issued.
        if (r.cache === 'only-if-cached' && r.mode !== 'same-origin') return;

        event.respondWith(
            fetch(r)
                .then((response) => {
                    // Opaque responses have no readable headers and their body
                    // cannot be re-wrapped; pass them straight through.
                    if (response.status === 0) return response;
                    const headers = new Headers(response.headers);
                    headers.set('Cross-Origin-Embedder-Policy', 'credentialless');
                    headers.set('Cross-Origin-Opener-Policy', 'same-origin');
                    return new Response(response.body, {
                        status: response.status,
                        statusText: response.statusText,
                        headers,
                    });
                })
                .catch((e) => {
                    // Never turn a network hiccup into a broken page.
                    console.error('[coi] passthrough failed:', e && e.message);
                    return fetch(r);
                }),
        );
    });
} else {
    // ── Page context ────────────────────────────────────────────────────────
    //
    // OFF BY DEFAULT. Opt in with ?coi=1.
    //
    // Isolation only buys a score cutoff shared between the engine's workers,
    // and the measurements in WASM.md say partitioning is flat on every
    // workload that actually completes — so the benefit is close to zero,
    // while a service worker that misbehaves takes the whole site down and
    // keeps doing it on every later visit. That trade is not worth making by
    // default. It cost two outages to learn: once when the worker's scope did
    // not cover the engine's worker script, and once when a stale registration
    // from an earlier deploy kept controlling the page.
    //
    // The default path therefore does the opposite of registering: it removes
    // any worker a previous version of this file left behind. That cleanup is
    // the important part — simply deleting the <script> tag would strand every
    // visitor who already had one registered, with no way to recover short of
    // clearing site data by hand.
    (() => {
        if (!navigator.serviceWorker) return;
        const params = new URLSearchParams(window.location.search);
        const wanted = params.get('coi') === '1' && params.get('nocoi') !== '1';

        if (!wanted) {
            // Unregister EVERY registration in scope, not just this script's.
            // An earlier build served this file from /solver/, which registers
            // separately and — being the more specific scope — wins for solver
            // pages. Both have to go.
            navigator.serviceWorker.getRegistrations().then((regs) => {
                if (!regs.length) return;
                let removed = 0;
                Promise.all(regs.map((r) => r.unregister().then((ok) => { if (ok) removed++; })))
                    .then(() => {
                        if (!removed) return;
                        console.info(`[coi] removed ${removed} stale service worker(s)`);
                        // A page loaded UNDER the old worker is still controlled
                        // by it until navigation; reload once so this visit is
                        // clean rather than half-controlled.
                        if (navigator.serviceWorker.controller
                            && sessionStorage.getItem('coi-cleaned') !== '1') {
                            sessionStorage.setItem('coi-cleaned', '1');
                            window.location.reload();
                        }
                    });
            }).catch(() => {});
            return;
        }

        if (window.crossOriginIsolated) return;          // already isolated
        if (!window.isSecureContext) return;             // SW needs https/localhost

        navigator.serviceWorker.register(_COI_SRC || 'coi-serviceworker.js')
            .then((registration) => {
                if (registration.active && !navigator.serviceWorker.controller) {
                    if (sessionStorage.getItem('coi-reloaded') === '1') return;
                    sessionStorage.setItem('coi-reloaded', '1');
                    window.location.reload();
                }
            })
            .catch((e) => console.warn('[coi] registration failed:', e && e.message));

        navigator.serviceWorker.addEventListener('controllerchange', () => {
            if (sessionStorage.getItem('coi-reloaded') === '1') return;
            sessionStorage.setItem('coi-reloaded', '1');
            window.location.reload();
        });
    })();
}
