// Guards the condition the "split" scaling plan's collision rule rests on
// (rust_bridge.js, support matrix A8).
//
// When an atree variable effect writes a key the CONSTANT partition also
// writes, the two evaluation orders differ:
//
//   full pass:  base + (c1 + v1 + c2 + v2)     — interleaved, effect order
//   split:     (base + (c1 + c2)) + (v1 + v2)  — each partition summed apart
//
// Float addition is not associative, so those can disagree in the last bits.
// The exporter therefore refuses to lower such a tree — EXCEPT when every
// contribution is integral, where the sum is exact in any order (integers
// below 2^53 are exactly representable, and so are their partial sums).
//
// This test pins both halves of that claim. If it ever fails, the exporter's
// relaxation is no longer safe and the collision must go back to `full`.
//
// Run: node js/solver/tests/test_scaling_association.js

'use strict';

const { TestRunner } = require('./harness');
const t = new TestRunner('Scaling Partition Association');

const full = (base, c, v) => base + (c.concat(v).reduce((a, b) => a + b, 0));
const split = (base, c, v) =>
    (base + c.reduce((a, b) => a + b, 0)) + v.reduce((a, b) => a + b, 0);

// ── Integral contributions: exact in any association ─────────────────────────
{
    const TRIALS = 200000;
    let diverged = 0;
    // Deterministic LCG so a failure is reproducible.
    let seed = 0x2545F491;
    const rnd = () => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
    const ri = (n) => Math.floor(rnd() * n);
    for (let i = 0; i < TRIALS; i++) {
        const base = ri(1e6);
        const c = [ri(1e6), ri(1e6)];
        const v = [ri(1e6), ri(1e6)];
        if (full(base, c, v) !== split(base, c, v)) diverged++;
    }
    t.assert(diverged === 0,
        `integral contributions must be association-independent, ${diverged}/${TRIALS} diverged`);
}

// ── Fractional contributions: association is observable ─────────────────────
{
    // Not a probabilistic claim about any particular input — just that the
    // hazard is real, so the exporter is right to refuse the fractional case.
    let seed = 0x9E3779B9;
    const rnd = () => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
    let diverged = 0;
    for (let i = 0; i < 200000 && diverged === 0; i++) {
        const base = rnd() * 1000;
        const c = [rnd() * 1000, rnd() * 1000];
        const v = [rnd() * 1000, rnd() * 1000];
        if (full(base, c, v) !== split(base, c, v)) diverged++;
    }
    t.assert(diverged > 0,
        'fractional contributions must be able to diverge — otherwise the '
        + 'collision rule is guarding nothing and should be re-examined');
}

// ── The boundary the integral case relies on ────────────────────────────────
{
    // Exactness holds while partial sums stay below 2^53. Past it, even
    // integers stop being association-independent — which is why the rule is
    // stated with that bound rather than "integers are always fine".
    const big = Math.pow(2, 53);
    t.assert((big + (1 + 1)) !== ((big + 1) + 1),
        '2^53 must be the point where integral association breaks, so the '
        + 'bound in the collision rule is the right one');
    const safe = Math.pow(2, 40);
    t.assert((safe + (1 + 1)) === ((safe + 1) + 1),
        'well below 2^53 integral association must hold');
}

t.summary();
