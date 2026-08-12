# Solver support & performance matrix

Tracking list of (A) inputs the engines cannot solve today, (B) inputs that
solve correctly but noticeably slower because the fast paths switch off, and
(C) integration gaps. "Rust engine" = the enum/score kernel; "JS engine" =
the in-browser worker solver (the reference implementation). Items are
ordered roughly by expected user impact within each section.

## A. Not supported

### Rust engine only (falls back to the JS engine — correct, ~100-1500x slower)

| # | Feature | Where it appears | Notes |
|---|---------|------------------|-------|
| A1 | Loop brackets in combos | Combo editor `[loop]` sections | Fast mana sim + scoring hard-fail; the most commonly used unsupported feature. Port = loop-aware mana/dps simulation. |
| A2 | Buff states | Combos toggling buff uptime windows | Hard-fail at export; needs the stateful simulate path. |
| A3 | Blood Pact | Shaman HP-cost casting + BP boost injection | Requires simulation-derived boost tokens (`inject_blood_pact_boosts`) per leaf. |
| A4 | `total_healing` objective | Scoring target / custom-blend sub-target | Heal totals need the heal part evaluation path; `Objective::parse` rejects it. |
| A5 | Radiance boost | Radiance major ID scaling | Exporter asserts null; scaling multiplies stat maps non-additively. |
| A6 | Dynamic sliders | Atree sliders marked stat-dependent/dynamic | JS gates features on `has_dynamic_sliders`; fixtures never carry them. |
| A7 | Multi-partition scenarios | Crafted variations / partitioned searches | Fixture format captures ONE partition; JS runs several and merges. Restricted-melee comparison shows the gap (one JS-only top entry). |
| A8 | Non-lowered atree plans (`scaling_kind == "full"`) | Trees the exporter cannot lower to cached/split | Layer2 rejects; scenario falls back to JS. |

### Both engines (nobody solves these today)

| # | Feature | Notes |
|---|---------|-------|
| A9 | Tome slot search | Tomes are fixed inputs; the solver never searches tome combinations. |
| A10 | Weapon slot search | The weapon is fixed; solving across weapons means separate runs per weapon. |
| A11 | Powder optimization | Powders are taken as-is on the fixed weapon/armor inputs; no search over powder sets. |
| A12 | Crafted-item ingredient search | Crafted items participate as fixed statmaps; the solver does not search ingredient/recipe space. |

## B. Supported but slow (correct results, fast paths off or weak)

| # | Case | Why slow | Speed today | Possible fix |
|---|------|----------|-------------|--------------|
| B1 | Defensive objectives (ehp/ehpr/hpr/total_hp) | Ceiling gate discriminates poorly (defensively-flat pools), so most leaves run greedy+mana; mana sim dominates | ~9x slower than damage goals (406K vs ~3.7M leaves/s) | Cluster-level mana prefilter; cheaper sustain model |
| B2 | Custom blends with any negative weight | Monotonicity proof fails → gate + all bounds off | Unpruned (~50-100x slower on wide pools) | Interval (min/max) ceiling bounds |
| B3 | Atrees failing `ceiling_vars_ok` (negative stat-input factors, or `atkTier`/`*ConvBase` var outputs) | All-150 SP is not an upper bound → gate + bounds off (matches JS) | Unpruned | Extremal-per-output ceiling assemble (like the bounded doom) could re-arm the gate soundly |
| B4 | `hp_casting` builds | Gate off (JS parity); HP-sim path heavier | Unpruned | Prove/derive an hp-casting-safe ceiling |
| B5 | Scenarios with maxMana/int var couplings in ≤5-sustain mode | Bounded doom disabled (start-mana monotonicity hole) | Full greedy+mana on mana-dead leaves | Two-sided doom bound on start-minus-end |
| B6 | ehp-family **thresholds** on huge pools | Additive prechecks are weaker than the exact leaf check; ehp thresholds only reject at the leaf | Scenario-dependent | Fold atree scaling into the precheck (JS TODO notes this too) |
| B7 | Warm start on SP-antagonistic objectives (e.g. xpb) | Solo-ceiling ranking picks SP-infeasible elites → warm pass seeds nothing | ~1.5s wasted, then organic cutoff | Rank by solo ceiling × SP-feasibility screen |
| B8 | The JS engine as a whole | No dense vectors / cluster bounds / warm start | ~430K leaves per 180s vs Rust ~670M | Port improvements back, or ship Rust via WASM (C1) |

## C. Integration gaps

| # | Gap | Notes |
|---|-----|-------|
| C1 | Rust engine is not shipped to users | It runs as a native benchmark/validation kernel; the browser uses the JS engine. A WASM build + worker integration would give end users the ~1000x. |
| C2 | Fixture export is test-harness-driven | Scenarios come from snapshots via `test_solver_search.js`; no direct app → fixture path. |
| C3 | Rust top-15 tie membership at the boundary | Score sets match JS exactly; which of several EQUAL-scoring builds occupies the last slot can differ (insertion order). Documented, not a correctness issue. |

## Verification status

Everything in section B is covered by the bit-exact validators (9 fixtures ×
5 levels + per-trial dense assertions + on/off top-15 equivalence). Section A
items hard-fail loudly at export or load — nothing silently degrades.
