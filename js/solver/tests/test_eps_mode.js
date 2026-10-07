// Roadmap R21/R32: the Near-optimal search mode reaches the Rust engine as an
// `EPS <v>` line in the enumeration fixture (parsed by enumerate.rs, whose
// own tests cover the parse and the within-eps guarantee).
// Run: node js/solver/tests/test_eps_mode.js
'use strict';

const fs = require('fs');
const path = require('path');
const { TestRunner } = require('./harness');
const bridge = require('../engine/rust_bridge.js');

const t = new TestRunner('Near-optimal mode (EPS line)');

t.assert(bridge.epsFixtureLine({ search_eps: 0.01 }) === 'EPS 0.01', 'a positive tolerance emits EPS');
t.assert(bridge.epsFixtureLine({ search_eps: 0 }) === null, 'zero is the exact search: no line');
t.assert(bridge.epsFixtureLine({}) === null, 'absent is the exact search: no line');
t.assert(bridge.epsFixtureLine({ search_eps: -1 }) === null, 'negative is refused');
t.assert(bridge.epsFixtureLine({ search_eps: NaN }) === null, 'NaN is refused');
t.assert(bridge.epsFixtureLine(null) === null, 'no init message: no line');

// buildEnumFixture must append it; search.js must set it only in 'within'.
const bridgeSrc = fs.readFileSync(path.join(__dirname, '../engine/rust_bridge.js'), 'utf8');
t.assert(/const eps = epsFixtureLine\(initMsgBase\);\s*if \(eps\) L\.push\(eps\);/.test(bridgeSrc),
    'buildEnumFixture appends the EPS line');
const searchSrc = fs.readFileSync(path.join(__dirname, '../engine/search.js'), 'utf8');
t.assert(/init_base\.search_eps = _solver_state\.search_eps \|\| 0;/.test(searchSrc),
    'the Rust init message carries the tolerance');
t.assert(/_solver_state\.search_eps = within\b/.test(searchSrc),
    'the tolerance is set only for the within mode');
const html = fs.readFileSync(path.join(__dirname, '../../../solver/index.html'), 'utf8');
t.assert(/<option value="within">/.test(html) && /id="solver-eps"/.test(html),
    'the page offers the mode and its tolerance');

const result = t.summary();
if (require.main === module && result.fail > 0) process.exit(1);
module.exports = result;
