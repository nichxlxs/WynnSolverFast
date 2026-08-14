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
    (() => {
        const params = new URLSearchParams(window.location.search);

        if (params.get('nocoi') === '1') {
            navigator.serviceWorker?.controller?.postMessage({ type: 'deregister' });
            return;
        }
        if (window.crossOriginIsolated) return;          // already isolated
        if (!window.isSecureContext) return;             // SW needs https/localhost
        if (!navigator.serviceWorker) return;

        navigator.serviceWorker.register(window.document.currentScript.src)
            .then((registration) => {
                // Reload exactly once, and only once the worker is in control —
                // reloading before that just produces a non-isolated page again.
                // sessionStorage keeps a failure from becoming a reload loop.
                registration.addEventListener('updatefound', () => {});
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
