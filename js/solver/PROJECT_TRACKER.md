# WynnSolver performance project tracker

Updated: 2026-08-12

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
- **IN PROGRESS** The first search fixtures exist, but combo snapshots and wider
  class/target coverage are still absent. The suite passes 196 assertions with
  two missing-combo-fixture warnings; regression coverage is not complete.
- **BLOCKED** Direct review of SP Algorithm Bounty PR 1: GitHub returned HTTP 403
  through both HTTPS and Git transports on 2026-08-12. Retry from an environment
  with GitHub access; do not infer its behavior from its title.
- **DONE** The local side of the SP review confirms exact calculation is required
  at qualifying leaves and identifies the current algorithm as factorial
  activation-order search, with an explicit external adoption gate.
- **DONE** A 90-second phase trace and V8 CPU profile identified incremental
  stat add/remove as 87.75% of pre-change worker samples.
- **DONE** Per-item compiled additive-stat caching raises the 2M long benchmark
  from 27K to 215K checked/s; original completes in 57s, current in 9.4s.
- **DONE** Parallel cached stat arrays and compact running stats add another
  +29.3% and +19.1% respectively on the 2M completion benchmark.
- **DONE** A registry-built numeric stat vector replaces string-keyed updates
  in current mode, adding +90.04% throughput on the same 2M benchmark.
- **DONE** Per-item numeric index lists are now compiled before enumeration,
  avoiding a WeakMap lookup on every placement and removal.
- **DONE** Progress reporting uses monotonic checkpoints instead of a modulo at
  every leaf and now still fires when subtree pruning jumps past a checkpoint.
- **DONE** SP requirement maxima restore from a depth stack rather than being
  recomputed from every preceding slot during backtracking.
- **DONE** A 95.17M-input benchmark now completes current mode in 59.70s while
  original mode reaches 4.82M checked tuples and times out at 120s.
- **BLOCKED** The user-mentioned ZIP containing lodestone/lodestone swift is not
  present under the repo, workspace, or `/tmp`; source-level review awaits it.

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
| 2026-08-12 | Indexed numeric running stats | Gaia 2M, same current space | 2,025,400 | 2,025,400 | 203,660/s | 387,044/s | **+90.04%** | Complete runs; same 470,163 incumbent |
| 2026-08-12 | Precompiled numeric item indices | Gaia 2M, three-run median | 2,025,400 | 2,025,400 | 393,282/s | 434,542/s | **+10.49%** | Same 470,163 incumbent in every run |
| 2026-08-12 | Monotonic progress checkpoints | Gaia 2M, three-run median | 2,025,400 | 2,025,400 | 420,906/s | 441,264/s | **+4.84%** | Same 470,163 incumbent in every run |
| 2026-08-12 | Constant-time SP max restoration | Gaia 2M, three interleaved runs | 2,025,400 | 2,025,400 | 408,100/s | 480,066/s | **+17.63%** | Same 470,163 incumbent in every run |
| 2026-08-12 | Indexed numeric running stats | Gaia 95M input, same current space | 16,007,310 | 16,007,310 | 199,113/s | 240,466/s | **+20.77%** | Complete in 80.39s vs 66.57s; same 470,163 incumbent |
| 2026-08-12 | Precompiled numeric item indices | Gaia 95M input, same current space | 16,007,310 | 16,007,310 | 240,466/s | 265,567/s | **+10.44%** | Complete in 66.57s vs 60.28s; same 470,163 incumbent |
| 2026-08-12 | Constant-time SP max restoration | Gaia 95M input, same current space | 16,007,310 | 16,007,310 | 265,567/s | 268,120/s | **+0.96%** | Complete in 60.28s vs 59.70s; same 470,163 incumbent |
| 2026-08-12 | Current cumulative | Gaia 95M input / two-minute cap | 9,938,880 | 16,007,310 | 40,021/s | 268,120/s | **+569.95%** | Original timed out; current completed in 59.70s; both retain 470,163 |

The larger current spaces are themselves correctness improvements: original set
dominance removed candidates without proving that their cross-slot bonuses were
irrelevant. Completion time must therefore be read alongside space and rate.
The long-case input space before dominance is 95,169,492 in both modes; the
ledger's original/current spaces are the actual post-optimization enumeration
spaces.

## Current profiling conclusion (dated, provisional)

On 2026-08-12, after numeric indexing and precompiled item indices, the Gaia 2M
phase trace attributed 8.58% of wall time to greedy SP allocation and 4.52% to
leaf prechecks. Exact SP, assembly, finalization, scoring, mana, thresholds, and
top-N together accounted for under 2.6%. The remaining 84.37% is outside the
leaf phase timers: recursive enumeration, item/SP place and remove, subtree
accounting, and worker/runtime overhead. This makes enumeration the next
profiling target, not exact SP. These figures are temporary and machine-specific;
replace them after the next traversal change rather than treating them as a
portable baseline.

## Constraint-driven search reduction plan

User restrictions can safely reduce the search, but only through admissible
bounds over every remaining slot. Never discard an item merely because it
individually violates a condition: later items, set bonuses, SP assignment, and
negative/positive modifiers can compensate.

Implement in this order after the exhaustive oracle:

1. **Direct additive suffix bounds (P1.5).** For each depth and direct `ge`/`le`
   restriction, precompute the maximum/minimum contribution achievable from all
   remaining slots. Prune only when `current + remainingBest` cannot satisfy the
   restriction. Include positive set-bonus envelopes or exclude affected stats
   from this optimization. This is interval propagation and is exact.
2. **Multi-condition Pareto suffix frontiers.** For correlated restrictions,
   retain only nondominated suffix vectors. A suffix state dominates another
   only when it is no worse for every active condition and no more expensive in
   SP. Cap frontier construction by depth and fall back to interval bounds if it
   grows too large.
3. **Requirement feasibility bounds.** Extend the existing SP suffix provision
   check with per-attribute requirement envelopes and set-aware provisions. The
   current SP bound is already the model for safe mid-tree pruning.
4. **Objective upper bounds (P1.6).** Once a top-N cutoff exists, compute an
   optimistic score for a partial build plus all remaining slots. Prune only if
   that upper bound cannot beat the cutoff. Damage needs bounds over attack
   speed, additive damage, multipliers, spell properties, and SP; unsupported
   interactions must make the bound looser, never heuristic.
5. **Condition-aware item filtering.** Apply dominance within a slot using the
   active restriction/objective dimensions plus requirements and set identity.
   This can remove items before Cartesian enumeration, but proof must compare
   every relevant dimension and preserve equal/tie-distinct results.

The likely high-value sequence is P0.4 oracle, worker-level traversal profiling,
P1.5 additive suffix bounds, P1.4 compiled combo plan, then P1.6 objective bounds.
P1.7 SP/greedy work should use a deliberately SP-heavy benchmark; Gaia does not
currently justify prioritizing a new exact SP algorithm.

### Multi-size traversal trace (2026-08-12, temporary)

Tracing itself adds substantial `performance.now()` overhead, so compare phase
shares rather than traced throughput. The 2.03M case completed; wider cases use
the same 30-second, one-worker observation window.

| Scenario | Checked | Greedy | Stat add/remove | SP place/remove | SP bounds | Unaccounted |
|---|---:|---:|---:|---:|---:|---:|
| Gaia ring 2M | 2,025,400 | 3.13% | 22.64% | 23.07% | 7.34% | 41.80% |
| Gaia bracelet 10M | 1,550,000 | 56.98% | 7.98% | 8.23% | 2.06% | 13.83% |
| Gaia wide 95M input | 3,250,000 | 45.98% | 10.98% | 11.03% | 2.72% | 18.96% |

The workload changes with pool shape and feasibility: enumeration dominates the
ring case, while greedy SP scoring dominates the wider bracelet cases because
many more tuples pass exact SP. Next work must therefore benchmark both a
traversal-heavy and greedy-heavy case; optimizing only the 2M case is misleading.

## SP algorithm / Lodestone status

Exact SP calculation is required for candidate legality, but it is not currently
the Gaia bottleneck. Full-build result caching is unlikely to hit because each
canonical tuple is visited once; bounded subset-state or compiled-vector caches
may help SP-heavy cases. Lodestone and Lodestone Swift must be differential-tested
for negative SP, self-exclusion, intermediate sustainability, crafted/weapon/set
rules, caps, budget, and returned metadata before adoption. As of 2026-08-12 the
referenced ZIP is not present in this checkout and no Git remote is configured,
so source-level review remains blocked until the archive is available locally.

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

We have **not moved to Rust**. The current engine remains JavaScript/Web Workers.
That is intentional: profiling exposed representation and traversal costs that
were fixed much faster in the reference engine, producing larger gains than a
language-only port would guarantee. Rust should still improve memory density,
allocation control, native threading, and unattended execution, but its speedup
cannot be responsibly promised before the exact oracle and a compact-kernel
prototype exist. The existing go/no-go gate remains at least 1.5x end-to-end or
a decisive memory/checkpoint advantage. Algorithmic branch-and-bound and compact
data layouts are expected to matter more than language alone; the same design
will be used by both JS reference and future Rust core.

## Phase 0 — measurement and correctness foundation

| ID | Status | Deliverable | Exit criterion |
|---|---|---|---|
| P0.1 | DONE | Statistical benchmark utilities | Unit-tested median, p90, mean, deviation, comparison |
| P0.2 | DONE | SP microbenchmark | 0/4/8 cascade cases, allocating/scratch, JSON output |
| P0.3 | DONE | Top-N old/new benchmark | Exact-output check plus runtime/allocation comparison |
| P0.4 | IN PROGRESS | Small exhaustive search oracle | Exact top-N and funnel counts independent of production enumeration |
| P0.5 | DONE | Real-world end-to-end benchmark | Original/current comparison over complete and time-bounded searches |
| P0.6 | IN PROGRESS | Real snapshot corpus | README and Gaia builds cover 0.8M-8.1M spaces; every class, mana/HP, crafted, and other targets remain |
| P0.7 | BACKLOG | Controlled-machine baseline | Store artifact outside Git; document hardware and runtime settings |
| P0.8 | BACKLOG | Long soak/resume benchmark | Cancellation, bounded memory, restart and exact resumed result |

## Phase 1 — JavaScript reference optimization

| ID | Status | Deliverable | Safety gate |
|---|---|---|---|
| P1.1 | DONE | Allocation-free top-N cutoff | Deterministic unit test and equality with legacy algorithm |
| P1.2 | DONE | Set-safe dominance pruning | Set-piece survival regression; all set items conservatively excluded |
| P1.3 | DONE | Indexed item/stat vectors | Parallel item arrays, numeric registry, typed running vector, and exact materialization tests |
| P1.4 | BACKLOG | Compiled combo plan | General/compiled differential tests for every combo fixture |
| P1.5 | BACKLOG | Direct restriction suffix bounds | Exact oracle equality and prune-reason counters |
| P1.6 | BACKLOG | Objective branch-and-bound | Formal admissible bound per supported target |
| P1.7 | BACKLOG | SP/greedy allocation reduction | Identical allocation, score and mana behavior |
| P1.8 | BACKLOG | Adaptive prefix task scheduler | Complete partition coverage and deterministic single-thread mode |
| P1.9 | BACKLOG | Shared immutable worker data | Clone/shared paths produce identical results |

## Phase 2 — native Rust proof of concept

| ID | Status | Deliverable | Go/no-go criterion |
|---|---|---|---|
| P2.1 | BACKLOG | Versioned `SearchJob`/`SearchResult` schema | Round trip plus data/engine version rejection tests |
| P2.2 | BACKLOG | Compact Rust item and SP kernel | Exact JS/oracle parity on generated cases |
| P2.3 | BACKLOG | Canonical single-thread enumerator | Exact counts and top-N across small exhaustive corpus |
| P2.4 | BACKLOG | Stateless objective prototype | At least 1.5x end-to-end speedup, or decisive memory benefit |

## Phase 3 — unattended local engine

| ID | Status | Deliverable | Exit criterion |
|---|---|---|---|
| P3.1 | BACKLOG | Native prefix-task coordinator | Bounded queue and deterministic reduction |
| P3.2 | BACKLOG | Checkpoint journal and resume cursor | Kill/restart test matches uninterrupted result exactly |
| P3.3 | BACKLOG | CLI budgets and progress | Time, CPU, memory, cancellation and structured output |
| P3.4 | BACKLOG | Browser export/result import | Versioned compatibility and validation errors |
| P3.5 | BACKLOG | Multi-core scheduler | Scaling and tail-utilization benchmark at available core counts |

## Phase 4 — optional execution targets

| ID | Status | Deliverable | Trigger |
|---|---|---|---|
| P4.1 | BACKLOG | Rust/Wasm worker build | Native core is proven and browser speed/startup benchmark wins |
| P4.2 | BACKLOG | Remote task leasing | One-machine compute, not algorithmic waste, is limiting |
| P4.3 | BACKLOG | GPU batch-scoring experiment | Profiling exposes a large branch-free scoring batch |

## Immediate work queue

1. Build P0.4 using tiny numeric pools and a deliberately simple Cartesian
   oracle; cover ring canonicalization and partition completeness.
2. Recover or recreate more real snapshots for P0.6. Check upstream WynnBuilder test
   history and port only fixtures whose inputs and expected semantics are clear.
3. Re-profile the indexed numeric vector before choosing between P1.4 and P1.5;
   the previous string-key hot-path profile is now obsolete.
4. Add an SP-heavy benchmark before evaluating lodestone swift; exact SP is
   under 1% on the traced Gaia workload and is not currently the main target.

## Update protocol

For every tracker item:

1. Write a failing correctness or benchmark test first.
2. Record the red command in the commit/PR notes.
3. Implement the smallest change that passes it.
4. Run the focused test, full solver suite, relevant benchmark, and
   `git diff --check`.
5. Commit atomically and update the status/evidence in this file in that commit
   or the immediately following documentation-only commit.
