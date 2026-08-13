# WynnSolver performance project tracker

Updated: 2026-08-13

Status legend: **DONE**, **IN PROGRESS**, **NEXT**, **BLOCKED**, **BACKLOG**.

## Objective and invariant

Deliver substantially larger, unattended searches on one machine, with a path
to remote workers later. Every optimization must preserve exact results against
an independent oracle. Heuristics may change result order, but only admissible
bounds may remove search space from a run advertised as exhaustive.

## Current baseline

- **DONE** Clean-clone test runner no longer crashes when optional snapshot
  directories are absent (`562e425`). It reports missing coverage as warnings.
- **DONE** Benchmark statistics and the first SP hot-path benchmark emit JSON
  with repeated samples and machine/runtime/commit metadata (`a5dffd4`).
- **DONE** Architecture decision and local-to-cloud boundaries are documented in
  `ARCHITECTURE_PLAN.md` (`a5dffd4`).
- **DONE** Top-N result insertion rejects strictly noncompetitive leaves before
  allocating item names and SP snapshots, with deterministic ties (`549b29b`).
- **DONE** Dominance pruning now conservatively preserves ordinary set items;
  standalone stats cannot prove their cross-slot set contribution (`d3b7c99`).
- **DONE** Real-world original/current comparison now covers free rings, two
  armor slots, and four armor slots using a checked-in WynnSolver README build.
- **DONE** The supplied tier-stack Gaia build now has 0.84M, 3.19M, and 8.14M
  combination variants with 30-second original/current anytime baselines.
- **IN PROGRESS** The fixture corpus now covers 258 assertions across solver,
  combo-damage, mana, dominance, display, enumeration, and benchmark paths.
  Nine warnings still identify missing combo/class/target coverage, so P0.6 is
  not complete.
- **DONE** The SP-Algorithm-Bounty archive was supplied as a ZIP in the repo
  root and fully reviewed on 2026-08-12; see the Lodestone section below. Its
  worst-case-greedy insight is now adopted as an exact closure fast path in
  `calculate_skillpoints`, guarded by a 300-build differential test.
- **DONE** Two latent correctness bugs found and fixed on 2026-08-12:
  `calculate_skillpoints` dropped `total_item_skillpoints` from its k>0 return
  (silently disabling Radiance item-SP scaling), and `getDefenseStats` read
  hp/hpBonus/hprRaw/hprPct without null guards, turning EHP-family scores into
  NaN for any build without an hpBonus item ("best score: NaN"). With the fix,
  every running-stat variant agrees at the historical 15,095 two-armor score.
- **DONE** A 90-second phase trace and V8 CPU profile identified incremental
  stat add/remove as 87.75% of pre-change worker samples.
- **DONE** Per-item compiled additive-stat caching raises the 2M long benchmark
  from 27K to 215K checked/s; original completes in 57s, current in 9.4s.
- **DONE** Parallel cached stat arrays and compact running stats add another
  +29.3% and +19.1% respectively on the 2M completion benchmark.
- **DONE** 2026-08-12 (second wave, fresh container baseline 290,713 checked/s
  on the 2M scenario): numeric-index running vector (P1.3), in-place greedy
  trial evaluation, O(5) SP unplace restore, and mid-tree restriction suffix
  bounds (P1.5) raise the 2M scenario to 4,857,074 checked/s (417ms
  completion) and complete the 95.17M-input case in 17.7s (was 80.4s; original
  now completes in 81.0s on this machine). Best scores unchanged everywhere.
- **DONE** P0.4 exhaustive Cartesian oracle: three fixtures re-enumerate the
  canonical tuple space independently and require exact checked/feasible/top-N
  equality plus partition-count invariance.
- **DONE** P2.2 Rust SP kernel prototype (`rust/sp_kernel`): exact parity on
  500 seeded fixtures, 8.5x kernel-level speedup over warmed JS (0.61 vs 5.20
  µs/call).
- **DONE** P2.3/P2.4 Rust scored enumerator: `enum_kernel` now includes the
  complete SP -> dense stat assembly -> greedy allocation -> restriction ->
  mana -> objective pipeline, exact fixture/top-15 checks, admissible subtree
  bounds, and native multi-thread work stealing. The recorded end-to-end gate
  is decisively above 1.5x on supported workloads.
- **DONE** First application integration slice (2026-08-13): the scored core is
  a reusable library behind a versioned `SearchJob`/`SearchResult` JSON
  contract, with thin native and `wasm-bindgen` adapters. A production browser
  serializer and dedicated module worker call the same engine. Rust/WASM is the
  default UI selector; JavaScript remains explicit and selectable.
- **DONE** Contract evidence: the browser-produced two-armor oracle job passed
  through the native adapter with exact 36 checked / 31 feasible / 11 scored,
  score 15,095, and identical winning items. The generated Wasm module passed
  its direct smoke test, and the local UI completed a real 754/754 exhaustive
  EHP search with rendered top builds.
- **IN PROGRESS** Browser Rust remains one worker, v1 jobs embed the canonical
  text/JSON payloads, and unsupported combo/scaling inputs return typed errors.
  Download/startup/memory benchmarks, capability coverage, and compact typed
  payloads remain before calling the browser target fully productized.

## Optimization ledger

This is the sole durable performance record. Raw benchmark JSON, profiler files,
and narrative reports are temporary and must not be treated as portable results.
Rates are machine-specific; compare rows only when the scenario/hardware match.

| Date | Change | Scenario | Original space | Current space | Original rate | Current rate | Improvement | Result check |
|---|---|---|---:|---:|---:|---:|---:|---|
| 2026-08-12 | Cutoff-aware top-N (`549b29b`) | Two armor, same set-safe pools | 1,872 | 1,872 | 469/s | 473/s | +0.86% | Same 15,095 score |
| 2026-08-12 | Preserve set items (`d3b7c99`) | Gaia armor + ring, pre-cache | 2,354,670 | 3,188,160 | 45,492/s | 49,671/s | +9.19% | Seed incumbent now fixes both at 470,163 |
| 2026-08-12 | Compiled incremental item stats (`57184ed`) | Gaia 2M long completion | 1,543,104 | 2,025,400 | 27,048/s | 215,033/s | **+695.01% (7.95x)** | Seed incumbent 470,163 in both modes |
| 2026-08-12 | Parallel cached stat arrays (`8f294c8`) | Gaia 2M, same current space | 2,025,400 | 2,025,400 | 152,137/s | 196,679/s | **+29.28%** | Same 470,163 incumbent |
| 2026-08-12 | Compact running stat object (`95231ae`) | Gaia 2M, same current space | 2,025,400 | 2,025,400 | 190,018/s | 226,226/s | **+19.06%** | Same 470,163 incumbent |
| 2026-08-12 | Current cumulative | Gaia 95M input / two-minute cap | 9,938,880 | 16,007,310 | 40,021/s | 199,113/s | **+397.52%** | Original timed out; current completed in 80.39s; both retain 470,163 |
| 2026-08-12 | New container baseline (all rows below share it) | Gaia 2M, 1 worker | 1,543,104 | 2,025,400 | 38,897/s | 290,713/s | — | 470,163 both |
| 2026-08-12 | Numeric-index stat vector (P1.3) | Gaia 2M, 1 worker | — | 2,025,400 | — | 818,674/s | **+181.6%** | 470,163; 15,095 on two-armor across variants |
| 2026-08-12 | In-place greedy trials + O(5) SP restore | Gaia 2M, 1 worker | — | 2,025,400 | — | 1,025,519/s | **+25.3%** | 470,163; greedy phase 678→231µs/leaf on two-armor |
| 2026-08-12 | Restriction suffix bounds (P1.5) | Gaia 2M, 1 worker | 1,543,104 | 2,025,400 | 569,621/s | 4,857,074/s | **+373.6%** | 470,163; oracle-exact funnel counts |
| 2026-08-12 | Cumulative second wave | Gaia 95M input, 2 workers | 9,938,880 | 16,007,310 | 122,733/s | 904,163/s | completes in 17.7s (was 80.4s) | 470,163 both; original also completes now (81.0s) |
| 2026-08-12 | Rust SP kernel prototype (P2.2) | 500 seeded SP fixtures | — | — | 192,466 calls/s (JS) | 1,638,372 calls/s | **8.5x kernel-level** | Exact parity, 0 mismatches |
| 2026-08-12 | Cost-only mana-sim boosts + atree scaling reuse | Two-armor dense trace | — | 1,872 | — | — | mana 260→23µs/leaf; wall 1957→640ms | 15,095 unchanged; mana divergence suite green |
| 2026-08-12 | Same, Gaia 2M | Gaia 2M, 1 worker | 1,543,104 | 2,025,400 | 600,897/s | 5,904,956/s | 343ms completion | 470,163 both |
| 2026-08-12 | New 135M benchmark (`gaia_armor4_ring_135m_input`) | 135.5M input / 22.97M search, 2 workers | 18,123,840 | 22,967,010 | 944,196/s (19.2s) | 12,210,000/s (**1.88s**) | — | 470,163 both |
| 2026-08-12 | New 1.9B benchmark (`gaia_ultra_1900m_input`) | 1.898B input / 229.7M search, 2 workers | 172,176,480 | 229,670,100 | 10.1M/s (17.0s) | 166.9M/s (**1.38s**) | — | 470,163 both; pruning proves most subtrees |
| 2026-08-12 | Rust enum_kernel (P2.3 prototype, 1 thread, no scoring) | 135M scenario | — | 22,967,010 | JS 1.881s | Rust **0.342s** | 5.5x vs 2-worker JS | Exact funnel parity: feasible 10,313 both |
| 2026-08-12 | Rust enum_kernel, 1.9B scenario | 1.9B scenario | — | 229,670,100 | JS 1.376s | Rust **0.332s** | 4.1x vs 2-worker JS | Exact funnel parity: feasible 4,017 both |
| 2026-08-12 | PR #3 absorption: progress checkpoint + precompiled entries | 135M scenario | — | 22,967,010 | 221 progress msgs (modulo) | 1,545 progress msgs (checkpoint) | anytime top-5 no longer starved | Gaia 2M median 366ms over 3 samples, 470,163 |
| 2026-08-12 | Dominance equality-set fix | All-free Gaia, 8 slots, lvl 100+ | — | 100.4B → 144.7B | feasible 0 (atkTier items wrongly pruned) | feasible 81,616 | correctness fix | Recalcitrance/Buzzsaw Bracer restored; Test 19 guards |
| 2026-08-12 | Split atree scaling (const cache + per-trial stat effects) | Two-armor dense trace | — | 1,872 | wall 640ms | wall **330ms** | greedy 254→97µs/leaf | 15,095 unchanged; oracle-exact on all fixtures |
| 2026-08-12 | New colossal benchmark (`gaia_colossal_4_5t_input`) | 4.52T input / 334.8B search, lvl 98-121, all slots free, 180s cap | 8,206,486,035 @ timeout (2.5%) | 301,683,889,228 @ timeout (90.1%) | 45.5M/s | 1.67B/s | Rust 1 thread completes in **77.1s** (feasible 329,883) | Both JS variants time out at 180s holding 470,163 |
| 2026-08-12 | Adaptive progress throttle + pre-placement column bounds | Colossal, 1 worker, 40s window | — | 41.1B → 62.5B checked | — | +52% | postMessage flood fixed; SP place/unplace 20% → 5% of samples | 470,163 |
| 2026-08-12 | Geometric level-band enumeration | Colossal, 1 worker, 40s window | — | 62.5B → 167.9B checked | — | +169% (4.1x round total) | prefix re-walks O(L_max) → O(log L_max); within-band order coarser, visited set identical | Oracle-exact; suite 240/240 |
| 2026-08-12 | Bands cumulative, full run | Colossal 4.52T input, 2 workers, 180s cap | completes 31.2s (147.8B legacy space) | **completes 32.9s** (334,843,891,200 = full space) | — | 10.17B/s | was timeout @ 90.1% | feasible 329,883 identical to Rust; 470,163 |
| 2026-08-12 | Rust enum_kernel with bands | all fixtures, 1 thread | — | — | 77.1s (colossal) | **4.33s**; all-free 8.4→0.85s; 1.9B 0.33→0.06s | 17.8x | Funnel counters bit-identical pre/post bands |
| 2026-08-12 | New spell-spam benchmark (`readme_spell_wide`) | 350.6B input / 20.07B search, combo_damage, no restrictions | — | 35,000 checked @ 180s timeout | — | ~194 leaves/s | feasibility-dense: 17,433/35,000 feasible — leaf pipeline dominates | best 7,832,627; the anti-Gaia workload |
| 2026-08-12 | Score-ceiling gate (one damage eval at all-150 SP bounds greedy) | Dense combo_damage (readme armor2 pools, 3,712 search) | — | 3,712 | wall 62.5s | wall **18.0s** (3.5x) | 1,798/3,542 feasible leaves gated at 1.0ms vs greedy 17.4ms | Oracle fixture `solver_oracle_spell_cd`: gated top-N exactly equals ungated Cartesian ground truth; suite 253/253 |
| 2026-08-12 | Ceiling gate, spell-spam workload | `readme_spell_wide`, 2 workers, 180s cap | — | 35,000 → 380,000 checked @ timeout | ~194 leaves/s | **~2,111 leaves/s (10.9x)** | leaf pipeline was 95% greedy; gate skips most of it | best improved 7,832,627 → 8,119,340 (more space covered) |
| 2026-08-12 | Ceiling gate, colossal | 4.52T input / 334.8B search, 2 workers | — | 334,843,891,200 (full space) | completes 32.9s | **completes 19.97s (1.65x)**, 16.76B/s | 329,883 feasible leaves now mostly gate before greedy | feasible 329,883 and best 470,163 unchanged |
| 2026-08-12 | Re-baseline post-dominance-fix + gate (queue item 7) | 2M / 95M / 135M / 1.9B, 2 workers | 2,486,862 / 13.4M / 25.2M / 239.0M (original variant) | 3,100,680 / 21.3M / 31.5M / 314.7M | 515ms / 8.88s / 1.82s / 1.08s (original) | **437ms / 8.76s / 1.39s / 0.93s** | current beats original despite 25-59% larger post-dominance space | 470,163 on all eight runs |
| 2026-08-12 | Cross-worker shared cutoff (SAB) for the ceiling gate | Dense combo_damage (readme armor2 pools, 3,712 search) | — | 3,712 | wall 18.0s (local-only gate) | wall **7.54s** (2.4x; 8.3x vs ungated) | greedy calls 1,744 → 521; gate armed from partition start via global 15th-best distinct score | best 8,110,465 unchanged; suite 253/253 |
| 2026-08-12 | Shared cutoff, spell-spam workload | `readme_spell_wide`, 2 workers, 180s cap | — | 380,000 → 430,000 checked @ timeout | ~2,111 leaves/s | **~2,389 leaves/s** (12.3x vs ungated) | — | best 8,119,340 and items unchanged |
| 2026-08-12 | Shared cutoff, colossal | 4.52T input / 334.8B search, 2 workers | — | 334,843,891,200 (full space) | completes 19.97s | **completes 17.89s** | enumeration-bound, so leaf gating gains are marginal here | feasible 329,883 and best 470,163 unchanged |
| 2026-08-12 | Multithreaded Rust enum_kernel (first-slot work stealing) | Colossal 4.52T, Rust | — | 334,843,891,200 | 4.35s (1 thread) | **1.16s (4 threads, 3.8x)**; all-free 1.816T 0.86→0.26s; 135M 0.10→0.05s | per-thread counter sums bit-identical to single-thread on every fixture | feasible 329,883/81,616/12,905/4,862 all match JS current |
| 2026-08-12 | Rust combo-damage core (P2.4 layer 1, `score_kernel`) | 96 sampled melee + 96 sampled spell builds | — | — | — | ~119K evals/s (1-row melee), ~1.6K evals/s (33-row spell), parity-first impl | first Rust scoring layer; JS `Math.pow` table + serde_json `float_roundtrip` needed for parity | **192/192 bit-exact** vs production worker damage |
| 2026-08-12 | Rust layer-2 leaf pipeline (greedy+mana+rescue) | 96 melee + 96 spell sampled builds | — | — | — | — | full (base_sp, total_sp, assigned, score) reproduction | **192/192 bit-exact** vs worker |
| 2026-08-12 | Rust scored enumeration (layer 3: gate + shared cutoff + top-15) | Dense combo_damage 3,712 search, 4 threads | — | 3,712 | JS 10.2s | **5.1s → 2.9s** (with layer-4 first pass) | top-15 scores + items bit-identical to JS | restriction scenarios need check_thresholds port |
| 2026-08-12 | Rust spell-spam coverage (first end-to-end Rust spell benchmark) | `readme_spell_wide` 20.07B search, 180s cap | — | 430K (JS) → **1.38M checked (3.2x)** | ~2.4K leaves/s (JS avg) | **~7.6K leaves/s and climbing at cap** | gate warms as shared cutoff rises | same scenario, zero JS at runtime |

The larger current spaces are themselves correctness improvements: original set
dominance removed candidates without proving that their cross-slot bonuses were
irrelevant. Completion time must therefore be read alongside space and rate.
The long-case input space before dominance is 95,169,492 in both modes; the
ledger's original/current spaces are the actual post-optimization enumeration
spaces.

## Current profiling conclusion (dated, provisional)

On 2026-08-12, before compiled-item caching, V8 self samples attributed 44.75%
to incremental add and 43.00% to remove. After caching, add/remove remained about
59%, recursion 13%, SP mid-tree place/remove 8%, greedy leaf allocation about
10% of wall time, and exact SP calculation below 1% on Gaia. This is provisional
and must be replaced after the next hot-path change rather than accumulated as a
permanent report.

## SP algorithm / Lodestone status

The SP-Algorithm-Bounty archive was reviewed in full on 2026-08-12. Findings:

- The bounty solves the **dual** problem — given *fixed* allocated SP, find the
  maximum-cardinality valid item subset — not our minimize-assigned-SP problem.
  Its differential fuzz test proves set-feasibility is order-independent under
  the cascade rule, which licenses subset-mask search instead of permutation
  search.
- **LodestoneAlgorithm / LodestoneSwiftAlgorithm** share one engine: a
  worst-case greedy fixpoint (items whose requirements hold even with every
  negative bonus applied are unconditionally safe) settles nearly everything on
  real builds; only the undecided + negative-bonus remainder (usually 0–3
  items) needs an exact bounded subset search. Swift only moves per-item
  normalization to equip time. LodestoneFallback contributes the cleanest
  cascade invariant (`need[s] = max(req[s] + bonus[s])`) and an admissible
  count/weight bound.
- **Adopted**: the greedy-fixpoint insight, transposed to minimization, is now
  an exact closure fast path in `calculate_skillpoints` — when no ordering item
  has a negative SP bonus lane and the closure at `assign = post_floor`
  activates everything, `post_floor` is optimal and the activation-order
  backtracking is skipped (O(k²) vs pruned k!). A differential test runs 300
  seeded real-item builds against a backtracking-only copy and requires
  identical outputs. The k==0/tiny-k structure, caps, budget, crafted/weapon/
  set handling are unchanged.
- **Not adopted**: the max-valid-subset objective, node caps, and the fallback
  repair heuristic — they answer a different question than build legality.

Exact SP remains <1% of Gaia wall time; the fast path mainly hardens SP-heavy
workloads (many ordering items) and simplified the Rust kernel port.

## Gaia incumbent clarification

The supplied build—Sureshot, Twilight-Gilded Cloak, Bull Charge, Warchief,
Breezehands, Bygg, Buzzsaw Bracer, Recalcitrance, and Gaia with the +4 guild
tome—scores 470,163 in the headless evaluator. Earlier temporary benchmark
output showed lower equipment because the headless benchmark omitted the
production solver's incumbent seed and several runs timed out. The search space
was not responsible. Gaia fixtures now seed 470,163 and those eight equipment
names, while Gaia remains the fixed weapon; original and current modes can never
report a worse best-so-far result.

## Rust status and decision gate

The Rust engine has a complete scored enumeration path for the supported
subset, including dense stat vectors, exact restrictions, mana, generic
objectives, admissible branch-and-bound, warm starts, and native multi-thread
scheduling. Differential fixtures and top-15 comparisons establish exact
parity for that subset; project benchmarks exceed the original 1.5x end-to-end
go/no-go gate.

The first productization boundary is now implemented. `search_core` owns
parsing and single-job execution, `engine` owns the versioned JSON envelope,
and native/Wasm binaries are adapters. The browser directly exports its active
configuration and renders Rust results through the existing UI. Unsupported
Rust inputs return a typed error and never silently switch algorithms. The next
browser work is payload compaction, capability expansion, multi-worker
scheduling, and measured startup/memory/throughput evidence.

## Phase 0 — measurement and correctness foundation

| ID | Status | Deliverable | Exit criterion |
|---|---|---|---|
| P0.1 | DONE | Statistical benchmark utilities | Unit-tested median, p90, mean, deviation, comparison |
| P0.2 | DONE | SP microbenchmark | 0/4/8 cascade cases, allocating/scratch, JSON output |
| P0.3 | DONE | Top-N old/new benchmark | Exact-output check plus runtime/allocation comparison |
| P0.4 | DONE | Small exhaustive search oracle | Three fixtures assert exact top-N/funnel equality and partition invariance |
| P0.5 | DONE | Real-world end-to-end benchmark | Original/current comparison over complete and time-bounded searches |
| P0.6 | IN PROGRESS | Real snapshot corpus | README and Gaia builds cover 0.8M-8.1M spaces; every class, mana/HP, crafted, and other targets remain |
| P0.7 | BACKLOG | Controlled-machine baseline | Store artifact outside Git; document hardware and runtime settings |
| P0.8 | BACKLOG | Long soak/resume benchmark | Cancellation, bounded memory, restart and exact resumed result |

## Phase 1 — JavaScript reference optimization

| ID | Status | Deliverable | Safety gate |
|---|---|---|---|
| P1.1 | DONE | Allocation-free top-N cutoff | Deterministic unit test and equality with legacy algorithm |
| P1.2 | DONE | Set-safe dominance pruning | Set-piece survival regression; all set items conservatively excluded |
| P1.3 | DONE | Indexed item/stat vectors | Float64Array running vector + compiled index/value entries; leaf materialization boundary |
| P1.4 | BACKLOG | Compiled combo plan | General/compiled differential tests for every combo fixture |
| P1.5 | DONE | Direct restriction suffix bounds | Oracle-exact equality; prunes credited to checked/precheck_reject |
| P1.6 | IN PROGRESS | Objective branch-and-bound | JS has a proven leaf ceiling/shared cutoff; Rust has subtree bounds for supported monotone objectives; general JS subtree bounds remain |
| P1.7 | DONE | SP/greedy allocation reduction | In-place trial eval (bit-identical scores) + O(5) SP restore + closure fast path |
| P1.8 | BACKLOG | Adaptive prefix task scheduler | Complete partition coverage and deterministic single-thread mode |
| P1.9 | BACKLOG | Shared immutable worker data | Clone/shared paths produce identical results |

## Phase 2 — native Rust proof of concept

| ID | Status | Deliverable | Go/no-go criterion |
|---|---|---|---|
| P2.1 | IN PROGRESS | Versioned `SearchJob`/`SearchResult` schema | V1 round-trip, schema rejection, provenance echo, native adapter, and Wasm smoke are green; compact typed payload and explicit data-compatibility policy remain |
| P2.2 | DONE | Compact Rust item and SP kernel | SP kernel exact parity on 500 fixtures, 8.5x kernel-level |
| P2.3 | DONE | Canonical scored enumerator | Exact funnel/top-N parity on supported melee, spell, restriction, and generic-objective fixtures; single and multi-thread paths |
| P2.4 | DONE | Stateless objective prototype | Dense scored Rust path passes the 1.5x end-to-end gate on supported workloads |

## Phase 3 — unattended local engine

| ID | Status | Deliverable | Exit criterion |
|---|---|---|---|
| P3.1 | BACKLOG | Native prefix-task coordinator | Bounded queue and deterministic reduction |
| P3.2 | BACKLOG | Checkpoint journal and resume cursor | Kill/restart test matches uninterrupted result exactly |
| P3.3 | IN PROGRESS | CLI budgets and progress | Thread count, time cap, and progress exist; structured job output, memory budget, and cooperative external cancellation remain |
| P3.4 | IN PROGRESS | Browser export/result import | Direct app export, Wasm result rendering, and typed errors exist; broaden unsupported-capability tests and add saved job/result files |
| P3.5 | IN PROGRESS | Multi-core scheduler | Atomic first-slot work stealing and 1/4-thread evidence exist; general prefix tasks and full available-core/tail benchmarks remain |

## Phase 4 — optional execution targets

| ID | Status | Deliverable | Trigger |
|---|---|---|---|
| P4.1 | IN PROGRESS | Rust/Wasm worker build | Build, module worker, direct Wasm smoke, native oracle parity, and local UI completion are green; startup/download/memory/throughput and multi-worker benchmarks remain |
| P4.2 | BACKLOG | Remote task leasing | One-machine compute, not algorithmic waste, is limiting |
| P4.3 | BACKLOG | GPU batch-scoring experiment | Profiling exposes a large branch-free scoring batch |

## Immediate work queue

1. P2.1: replace embedded text/JSON job payloads with a compact, explicitly
   versioned typed schema and define the data-compatibility policy. Retain v1
   rejection and provenance tests during migration.
2. P4.1: measure Wasm download, instantiation, memory, checked/s, and final
   top-N against native and JavaScript on melee, spell, restriction, and generic
   objective fixtures.
3. Add capability-contract tests for loop brackets, buff states, Blood Pact,
   Radiance, dynamic sliders, full atree scaling, and total healing. Rust
   selection must keep returning a typed error until each feature has oracle
   parity.
4. Add multi-worker Wasm partitioning and deterministic result merging only
   after the single-worker startup/memory baseline establishes a useful worker
   count.
5. Add durable SearchJob/SearchResult file import/export for the native
   unattended path, then continue P3.2 checkpoint/resume and P3.1 general
   prefix-task coordination.
6. Continue P0.6 fixture recovery, especially loop brackets, buff states, Blood
   Pact, Radiance, dynamic sliders, total healing, and multi-partition cases.

## Update protocol

For every tracker item:

1. Write a failing correctness or benchmark test first.
2. Record the red command in the commit/PR notes.
3. Implement the smallest change that passes it.
4. Run the focused test, full solver suite, relevant benchmark, and
   `git diff --check`.
5. Commit atomically and update the status/evidence in this file in that commit
   or the immediately following documentation-only commit.
