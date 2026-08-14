# WynnSolver exact-search optimization plan

This is the implementation-facing update to the original audit of upstream
commit `6c009d3`. It incorporates profiling and class-family benchmarks from the
Rust/WASM tree through 15 August 2026.

The central rule is simple: **an exact mode may only prune with an admissible
proof**. A fast/anytime mode may use measured heuristics, but it must not present
100% progress as a global certificate when preprocessing or leaf allocation is
approximate.

## Current decision

The Rust/WASM kernel, compiled spell/row plans, packed numeric path, subtree and
cluster bounds, score ceiling, warm incumbents, worker partitioning, and guarded
top-15 maintenance are already substantial wins. The next useful work is not a
wholesale rewrite. It is:

1. repair game/evaluator parity and remove false-pruning paths;
2. recover the resulting exactness cost with provably optimistic bounds;
3. measure target-specific structural reduction on the 18-family corpus;
4. add exact subproblem solvers only where the profiler shows a bottleneck;
5. use AI to tune ordering and safe policy parameters, never to invent an
   unverified prune.

## Status

| Area | Status in this PR | Result |
|---|---|---|
| weapon-inclusive sets | implemented | fixed JS/Rust set count and regressions |
| max mana 400 | implemented | fixed full/fast JS and Rust simulation |
| unsafe raw/EHP prechecks | disabled | restores exhaustive semantics |
| reachable set-SP bound | implemented | 1.08x–1.12x focused; much larger on two constrained family traces |
| ceiling memo | adaptive | 1.17x–1.20x versus forced-on on measured cases |
| priority score cache | already on `master` | setup sort only; about 12.9x locally |
| exact dominance | explicit policy | exact default; safe signature mode optional |
| raw oracle/counters | implemented | preprocessing and pruning checked independently |
| subset/Pareto SP DP | prototype rejected as default | representative cases slower; rare hard tail faster |
| per-depth snapshots | prototype rejected | neutral/slower in measured scenarios |
| finalized-stat score bounds | next | highest-value route to recover exactness tax |
| CP-SAT/MITM | deferred | require evidence that summaries/linear master compress |

## Complexity and attainable limits

Let `K <= 8` be free gear slots, `n_i` each retained pool size, `N` their
ring-adjusted Cartesian product, `M = sum(n_i)`, `P = 5` skill axes, `J <= 9`
activation-order items, `H = 15` retained results, and `V` search nodes visited
after safe pruning.

| Component | Current/naive bound | Best realistic target | Limit |
|---|---|---|---|
| pool filter | `O(I)` | indexed `O(log I + M)` | `Omega(M)` to materialize output |
| priority sort | `O(M log M * score)` | cache score: `O(M*score + M log M)` | comparison-sort bound |
| pairwise dominance | `O(sum n_i^2 d)` | signature-partitioned skyline | high-dimensional front may retain all |
| traversal | `O(KN)` | `O(V * bound_cost + scored * leaf_cost)` | worst case still `N` |
| partial SP bound | `O(P)` per node | reachable set-aware `O(P + reachable_sets)` | five-axis read is already constant/minimal |
| activation ordering | `O(J! * J * P)` | sparse subset/Pareto `O(J 2^J F P)` | `F` frontier width; profiling says BT usually wins today |
| extra-SP allocation | greedy `O(P B E)` | exact target-specific B&B/DP | black-box `<=B` allocations are `Theta(B^5)` |
| combo simulation | `O(T)` transitions | compiled segments + exact stateful remainder | genuinely stateful sequence is `Omega(T)` |
| top 15 | sort/allocate per score | threshold + binary insert winners | `H=15` makes it constant but hot |

With eight fixed pools of equal width, enumeration is `Theta(n^8)`, not
exponential in `n`. The generalized variable-slot, multi-constraint problem is
MMKP-like and strongly NP-hard. A direct uncoupled additive objective can fall
to `Theta(M)`; timed spell/sustain objectives retain set, tier, mana, HP, and
equip-order coupling.

## Wynncraft mechanics that constrain safe algorithms

- Manual SP is capped at 100 per axis, effective SP can reach 150, and equip
  order/cascades matter.
- Set bonuses are count-thresholded, may be non-monotone, may grant SP, and may
  include the fixed weapon.
- Duplicate rings are legal and can be required for a set threshold.
- Attack speed has seven capped discrete tiers.
- Major IDs can change the evaluator program, not only additive stats.
- Powder ordering changes conversion/special state.
- Mana, HP casting, steal, recast escalation, and loop state make combo scoring
  a state machine.

Any dominance key, DP state, MITM summary, or CP model must retain the relevant
interaction signature or be used only as an optimistic relaxation.

## Technique decisions

### 1. Admissible score upper bounds — build next

Use suffix extrema over a target-specific sufficient feature vector, reachable
set-count states, and explicit attack-tier/Major-ID partitions. For nonlinear
scores, interval evaluation must be sign-aware: evaluating the scorer at a
coordinatewise maximum is not safe unless monotonicity has been proved.

Start with finalized-stat bounds for EHP and family restrictions. The former raw
prechecks were fast precisely because they rejected huge subtrees, but they
could underestimate the completed build. A safe replacement is the largest
remaining performance opportunity identified by the audit.

### 2. SP feasibility bounds — implemented, then tighten selectively

The reachable-set bound is the safe level-A filter. Possible level-B/C filters
are nonnegative support directions and small five-dimensional suffix Pareto
fronts. Gate both on depth, restriction pressure, and measured prune yield.

The leaf counters prove an SP gap exists, but counters alone do not prove a new
bound will be cheap or fire early. Report bound calls, cost, depth, descendant
credits avoided, and exact-solve calls.

### 3. Set-aware dominance — exact policy first

Exact mode currently retains every candidate that passed the ordinary pool
filters; it performs no gear-dominance pruning. Optional safe mode deduplicates
only candidates with identical complete interaction signatures. Before enabling
safe mode by default, measure `q` on every raw family pool and differential-test
against the pre-dominance Cartesian oracle. The algebraic tuple multiplier
`1/(1-q)^8` is only a sensitivity, not measured throughput.

### 4. Dynamic programming / Pareto fronts — use for focused states

Sparse DP is attractive for five-dimensional SP or a compact additive target,
not an 80-stat universal state. The activation DP prototype was normally slower
because the existing backtracker rejects early and `J <= 9`; retain the result
as a tail-latency experiment, not default code.

### 5. Meet in the middle — conditional

A 4+4 split reduces summary generation to roughly `O(n^4)` per side but an
arbitrary join remains `O(N)`. Proceed only if signature-partitioned Pareto
compression makes indexed complement queries much smaller than the Cartesian
join. Preserve SP transfer, set counts, exclusivity, and evaluator signatures.

### 6. Constraint programming — oracle/hybrid, not browser leaf scorer

CP-SAT is well suited to exactly-one-per-slot, ring symmetry, set-threshold
binaries, exclusions, additive restrictions, and linear objective relaxations.
Use it offline as an independent oracle, candidate generator, or safe bound.
Equip-order cascades need order variables or lazy validation/cuts; timed combo
remains an exact JS/Rust leaf evaluator.

### 7. AI optimization harness — tune policies under proof gates

An evaluator-driven loop may propose:

- item/depth ordering;
- when to evaluate a safe bound;
- memo/sample thresholds;
- which support vectors or cluster widths to use;
- target-family routing.

Every candidate must run the raw reduced oracle, deterministic corpus, full
family matrix, adversarial mechanics fixtures, and repeated wall-time gate.
Fitness should combine exactness, time-to-best, exhaustive throughput, tail
latency, and memory. A single mismatch is a hard rejection for exact mode.

## Roadmap

### P0 — mergeable correctness foundation

- land the fixes, reachable bound, adaptive memo, explicit dominance contract,
  raw oracle, counters, recalibrated family gates, and benchmark artifacts in
  this PR;
- rebuild the shipped WASM artifact in CI/release infrastructure;
- document remaining Major-ID, crafted-requirement, roll, powder, and timing
  contracts.

### P1 — recover safe throughput

- implement finalized-stat admissible restriction/EHP bounds;
- add per-bound attribution and a five-repeat all-family A/B;
- measure safe dominance reduction on raw query pools;
- exhaust all medium family rows.

### P2 — certified target portfolio

- exact per-build SP allocation for supported objectives;
- signature-partitioned damage/EHP bounds;
- best-first/DFS hybrid with incumbent, upper bound, and optimality gap;
- CP-SAT or MILP reduced-pool oracle for additive targets.

### P3 — conditional structural experiments

- support-vector/Pareto SP suffixes;
- compressed MITM summaries;
- decision-diagram/Lagrangian core bounds;
- AI policy tuning on the sealed corpus.

## Definition of done

- Exact mode declares its mechanics envelope and has zero raw-oracle mismatches.
- Every prune is attributable to a tested admissible rule.
- Progress exposes concrete/evaluated work as well as descendant credits.
- Supported targets report exhaustion or a valid optimality gap.
- Speed claims name commit, data, scenario, mode flags, hardware, threads,
  repeats, cap, and spread.

## Research and mechanics references

- [Wynncraft API: set parts and count-keyed bonuses](https://docs.wynncraft.com/2026-04-04-v3-6)
- [Wynncraft Wiki: skill points](https://wynncraft.wiki.gg/wiki/Strength)
- [Wynncraft Wiki: identifications, mana, attack tiers](https://wynncraft.wiki.gg/wiki/Identifying)
- [Mansini & Zanotti, core-based exact MMKP](https://doi.org/10.1287/ijoc.2019.0909)
- [Coppé, Gillard & Schaus, cached decision-diagram B&B](https://doi.org/10.1287/ijoc.2022.0340)
- [van Hoeve, decision diagrams for optimization (2024)](https://doi.org/10.1287/educ.2024.0276)
- [Rong & Figueira, exact Pareto DP for bi-objective knapsack](https://doi.org/10.1016/j.ejor.2013.10.010)
- [Perron, Didier & Gay, CP-SAT-LP](https://doi.org/10.4230/LIPIcs.CP.2023.3)
- [Romera-Paredes et al., FunSearch](https://doi.org/10.1038/s41586-023-06924-6)
- [Novikov et al., AlphaEvolve](https://arxiv.org/abs/2506.13131)
- [Liu et al., Evolution of Heuristics](https://arxiv.org/abs/2401.02051)

Detailed measured outcomes are in
[`rust/sp_kernel/OPTIMIZATION_VALIDATION.md`](rust/sp_kernel/OPTIMIZATION_VALIDATION.md)
and [`rust/sp_kernel/CLASS_BUILD_CAPACITY.md`](rust/sp_kernel/CLASS_BUILD_CAPACITY.md).
