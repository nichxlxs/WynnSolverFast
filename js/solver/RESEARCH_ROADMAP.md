# Research roadmap: faster proofs and better anytime results

Written 2026-10-07 against `027b0e8`, revised the same day after the Codex
review on PR #19 (corrections in R2, R3, R6, R11, R12, R13, R16, R21 and
R22, each marked "Correction"). This is a plan, not a ledger: nothing
here is measured yet. Every "expected" effect below is a hypothesis to be
tested with the existing tools (`benchmark_ab.py`, the oracles,
`SCORE_DENSE_CHECK`, the family suite) before it is believed.

## Progress (implementation started 2026-10-07)

Two branches, stacked: `claude/correctness-c1-c7` (section 0) and
`claude/r1-reachable-sp` on top of it (R1, R9, R12, tie order). Each item below was
measured, and every pruning change passed the oracles before landing.

| Item | Status | Where | Evidence |
|---|---|---|---|
| C1 set weapons | done, both engines | `f824bf1` (JS), `d5b82fd` (Rust) | Bony test; new `solver_set_weapon_empty` snapshot; Rust parity 32/96 to 96/96 |
| C2 cross-set SP bound | done | `f824bf1` | multi-set admissibility test, red on master |
| C3/C4 prechecks | done, shared envelope | `b0ab23f` | Jester case, 5,000 random cases, Rust bridge mirrors it |
| C5 Rust healing | done (from PR #18) | `282826a` | healing fixture 0/96 to 96/96 |
| C6 greedy gap | closed by the R12 polish | `0ecc907` | test is now a hard assertion |
| C7 default dominance | PR #17 merged, certified default | `c14319c`, `73101ff` | review counterexample test |
| Independent oracle | done | `223316d` | catches a deliberately over-strict HP precheck |
| R1 reachable-SP ceiling | done, both engines, on by default | `21b579b` | tripwire over ~3.5B leaves; fixed-work top-15 identical; Rust 1.02x, JS 4x leaves in 30 s |
| R12 audit + polish | done, on by default | `0ecc907` | greedy beaten on 32-58% of leaves in 4 families before; Rust 0.975x, JS 12-19% fewer leaves; scores never lower |
| R9 node SP feasibility | done, Rust, adaptive (`SP_NODE_BOUND=0` disables) | `765ce0f` | one exact SP solve per last-slot range with relaxed items and summed set rows; 1.50x geometric mean, top-15 identical; always-on cost tierstack 10%, so AdaptiveBound switches it off where it does not reject |
| R1 at cluster bounds | done, Rust | `075b053` | last-slot cluster ceilings at reachable SP; 1.059x, exact |
| R1 at the tail bound | done, Rust | `5a622d2` | per-subtree cap from `sp_bound_base`; 1.0505x geometric mean, 8/11 faster (0.951 to 1.116); top-15 scores identical on all fixtures; full-space `checked` identical |
| Deterministic ties | done, both engines | `93bd4e8` | Rust `merge_top` and the page's merges kept tied builds in arrival order, so thread or worker scheduling decided their rank and, at rank 15, membership (seen on `fam_heavy_melee_small`: two builds tie exactly at ranks 8/9). Both now use `compareTopResult`'s order (score, then item names) |

Found while doing this, not yet fixed:

- **Radiance item-SP scaling.** With Radiance, Divine Honor, Shine or
  Judgement on, the builder also scales the skill points granted by items
  and set bonuses (`compute_radiance` with `total_item_skillpoints`, after
  the requirement check); the solver scales the radiance-affected stats but
  not item SP (the function's own comment says so). So with a boost on, the
  two disagree on effective SP and on every SP-derived multiplier, and the
  solver can rank builds differently from what the builder then shows.
  Needs: the in-game rule confirmed, the leaf and every SP-dependent bound
  (R1 caps, the ceiling's SP) updated together, and an oracle fixture with
  the boost on.

**Profile-driven overhead removal** (`df952c5`; not a roadmap item, found by
profiling before R10). Measured with callgrind on `fam_hybrid_medium`, the
bound and leaf paths spent more on bookkeeping than expected: `getenv` on
every subtree-bound eval (`BOUND_DEBUG`, 5.6% of instructions), two
`Arc<str>` allocations per `fill_direct` call (`parse_mult_entry("tome")`),
SipHash on item names and memo keys, and a `PoolItem` clone per placement.
All removed, results bit-identical (full-space top-15 and `checked` equal on
the six small families), each measured on its own:

| Change | A/B (families, 3 repeats) |
|---|---|
| per-eval `getenv` and per-call `Arc` allocation removed | 1.062x geometric mean, 13/18 faster |
| multiply-shift hasher for the item-name map and bound memo (`FAST_HASH=0` disables) | 1.059x, 16/18 |
| no `PoolItem` clone in `place`/`unplace`, no allocation in the ring-2 rebuild | 1.0145x, 12/18 |

R10 was re-scoped by the same pass: exact-kernel rejects (`sp_kernel_reject`)
are 0.3M to 8.6M per medium family run, against 0.7B to 3.4B leaves the
deficit bound and R9 already reject, and the leaf pipeline is about 1% of
wall. Pairwise conflicts can only remove the kernel rejects, so R10 is
parked until a profile shows SP kernel time again.

Not done yet, in order: R10 (parked, see above), R5/R21/R8 (gap, epsilon,
anytime metrics), R23 (incremental leaf fill), the JS mirror of R9, then the
rest of section 7.

## 0. Correctness first (from the author's review of PR #19, 2026-10-07)

The review's main conclusion stands above everything below: fix
correctness and define what "exact" guarantees before investing in faster
proofs. These are pre-existing runtime issues on master, not regressions of
this documentation PR; three were reproduced with focused JS fixtures at
`ef846c5`. Each needs its own small PR with its own test, in this order:

| # | Finding | Where | Status |
|---|---|---|---|
| C1 | **Set weapons never count toward their set.** 44 weapons in 2.2.3.0 carry a `sets` entry (Boundless, Corrupted, Cindercurse, Empty, Bony, ...). The loader writes `item.set` from the set tables; `calculate_skillpoints` treats the weapon as passive and never adds it to `set_counts`, so the leaf sees neither the set's stats nor its skill points. The Rust loader counts the weapon's requirements but not its set id. `build.js` reads `activeSetCounts` from the same call, so the builder's own display likely shares the omission (verify as part of the fix). | `js/game/skillpoints.js` weapon block; `enumerate.rs` fixture load | reproduced: Bony Bow + Circlet counts one piece, loses +8 Agi, +45 mdRaw, +15 aDamPct |
| C2 | **The JS reachable set-SP bound takes the maximum across sets**; the Rust engine sums per set. Two disjoint sets each granting +10 Dex are credited +10, so a completion needing 95 assigned Dex is bounded at 105 and rejected against the 100-per-lane cap. `test_sp_set_bound.js` pins the wrong behaviour. | `sp_set_bound.js` `accumulate_reachable_set_bonus` | reproduced |
| C3 | **The raw-stat `>=` precheck runs before set bonuses.** Jester bracelet + ring: raw XP -34 at max roll, +45 set bonus, +11 net; a minimum of +5 rejects them. Tracker queue item 2 called this an open decision; it is a bug. The envelope must include reachable set, tree and SP contributions, or the gate is disabled for any stat that has them. | `_build_constraint_prechecks`, `_fast_constraint_precheck` | counterexample from shipped data |
| C4 | **The EHP precheck evaluates Def/Agi at 100**; totals reach 150 with provisions, so it is not an upper bound. The divisor is exported to the Rust fixture, so both engines share it. | `worker.js` `_build_constraint_prechecks` | confirmed in source |
| C5 | **Rust healing reads `max_hp_heal_pct` only**; current data uses `power`, so a 15%-HP heal scores zero in Rust. Fixed on the anytime branch (PR #18). | `scoring.rs` | reproduced on that branch's suite, 15 of 132 fixtures |
| C6 | **Greedy SP allocation is not optimal.** With 20 points left and `base_sp = [0, 0, 60, 60, 60]`, the real allocator and evaluator pick 20/0 Str/Dex and score 1.18176; the feasible 10/10 split scores 1.19015, 0.71% better. An exhaustive equipment search therefore proves the best equipment *under greedy allocation*, not the best build. R12's audit measures how often this bites on shipped data. | `pure/engine.js` greedy; R12 | reproduced |
| C7 | **Default dominance is heuristic.** Dimensions below 0.5% sensitivity are ignored, and a synthetic objective `100 xpb + 0.4 lb` lets A = (xpb 1, lb 0) delete B = (xpb 0, lb 1000) although B scores four times A; the benchmarks branch's Knucklebones case is the shipped-data version. Exact mode uses certified dominance or none; heuristic policies are labelled (R32). | `item_priority.js` `_build_dominance_stats`; R4, R32 | synthetic, plus the branch's measured case |

Validation that goes with them, before any speed work: a tiny
**independent oracle** over raw pools and integer SP allocations with every
precheck, bound and dominance policy disabled (the current Cartesian oracle
reuses production prechecks and scoring, so both can agree on the same
bug); every proposed prune tested against small exhaustive *subtrees*, not
against surviving leaves; and fixture coverage for multiple sets, set
weapons, healing `power`, raw-plus-percent damage, negative SP lanes, future
set completion, guild tomes, `<=` restrictions, ties, active-worker bounds,
zero or negative scores and cross-bucket combinations. Then re-profile the
current Rust/WASM engine before ranking speed work: the 51.7% SP figure and
the 24 h projection quoted below are JS-engine measurements.

**Implementation sequence (review follow-up, 2026-10-07).** Commit the
three reproduced failures (C1, C2, C6) as regression tests first; C6's
stays a recorded known failure, never a skipped test, until R12's certified
allocator lands. Then fix C1 and C2. Then C3, C4 and C5, and build the
independent small-instance oracle. Integrate the useful work from PR #17
(guarded pruning, archetype fixtures) and PR #18 (Quick search, healing
schema, quality harness) in small validated changes rather than rebuilding
it. Freeze a corrected Rust/WASM benchmark baseline. Then R1 first, and
every later optimisation behind a switch, benchmarked on its own.

**Gate for every new pruning rule:** a documented safe-use condition, an
adversarial test, and an unpruned comparison. No speedup is accepted that
silently changes the search universe or the output guarantee. Tangent
bounds (R2), kernel scheduling (R30) and every new proof label (R21, R32)
stay behind that gate until they pass it.

## 1. Where the solver stands

The engine is a branch-and-bound over an MMKP-shaped space. Three
qualifications, from the review: its traversal is rank-band order (limited
discrepancy search, R29), not bound-priority search; its default item
reduction is a sensitivity heuristic that can remove the optimum (C7); and
its leaf skill-point allocation is a greedy that is not globally optimal
(C6). "Exact" today therefore means "every surviving tuple is scored under
the greedy allocator, on the reduced pools"; section 0 is what has to
change before the word is earned. The space:

- 8 groups (helmet, chest, legs, boots, ring x2, bracelet, necklace), one
  item per group, pools of ~150-250 items at lvl 80+ (72-148 at lvl 98+).
- Side constraints: the skill-point cascade (order-dependent requirement
  satisfaction with a 200-point budget and a 100-per-lane assign cap), stat
  restrictions, mana/HP sustain simulation, illegal (exclusive) sets.
- A non-linear objective: per spell part and element, a product of sums of
  item stats (raw x (1 + %) x skill-point curves x crit), plus atree var
  scaling and set bonuses.

Already in place, mapped to literature names:

| Engine feature | Literature equivalent |
|---|---|
| Level-band enumeration over sensitivity-ranked pools | Utility-sorted B&B (Kress, Kiel, Metternich 2021) |
| Dominance pruning on sensitivity-classified stats | Pareto / item dominance reduction |
| Restriction suffix bounds, SP mid-tree bound | Constraint propagation / feasibility bounds |
| Score-ceiling gate, cluster and tail bounds | Upper bounds from an "ideal point" relaxation |
| `WARM_K` elite subspace seeding the cutoff | Core problem (Mansini & Zanotti 2020), without the fixing step |
| Shared cutoff across workers | Parallel B&B incumbent sharing |
| `AdaptiveBound` | Online bound-layer selection (a bandit, in effect) |

What the tracker says is binding now:

1. **Proof, not discovery, on large spaces.** `solver_mage_gaia_6free_lvl50`
   finds its best build in seconds and then spends ~24h proving it.
   `readme_spell_wide` keeps improving over minutes, so its cold incumbent is
   weak.
2. **Bounds are loose.** PORT_PLAN records that per-stat pool maxima form a
   "super-item ~2x above real ceilings". On top of that, every ceiling is
   assembled at **all-150 SP in all five lanes** (`ceiling_sp = [150f64; 5]`
   in `scoring.rs`), which no build can reach: the assignable budget is 200
   and the per-lane assign cap is 100. With `SP_PERCENTAGE_RATE = 0.9908`,
   Str 150 gives x1.808 against x1.458 at Str 60, and Dex 150 gives 80.8%
   crit chance against 45.8%. Those two lanes alone can inflate a damage
   ceiling by tens of percent before the super-item effect.
3. **Per-leaf cost** on defensive objectives (B1) and SP-heavy restricted
   scenarios (`_bt` was 51.7% of JS wall on the mage case).

Bound quality drives (1) directly: a tighter admissible ceiling prunes more
subtrees and gates more leaves, and both the proof and the anytime curve
improve. So the roadmap is ordered by "how much does this tighten the bound
or raise the cutoff, per unit of risk".

## 2. Ranked recommendations

### R1. Reachable-SP ceilings (exact, low risk, do first)

**What.** Replace all-150 in every ceiling with the highest SP each lane can
actually reach.

- **Leaf gate.** After the exact SP solve, the leaf knows `base_sp`,
  `total_sp` and `remaining`. The doom precheck already proves (B1 section of
  SUPPORT_MATRIX) that greedy plus rescue can raise lane `s` by at most
  `min(remaining, 100 - base_sp[s], 150 - total_sp[s])`. That argument is not
  Int-specific. Use the per-lane cap for all five lanes in the gate.
- **Joint budget.** The per-lane caps are individually reachable but not
  jointly: the five additions share one `remaining`. The cheap admissible
  version is the per-lane cap (a superset). The tight version needs either a
  small enumeration over which lanes receive the budget, or the linearized
  bound in R2, where the joint budget becomes a fractional knapsack over five
  lanes that greedy solves exactly.
- **Mid-tree.** Cap each lane at
  `min(150, fixed_prov + prefix_prov + suffix_max_prov + set_reach_prov + 100)`.
  Weaker than the leaf version but still below 150 whenever a lane has no
  provision headroom.

**Soundness caveats to check before shipping.** Tome guild candidates each get
their own SP solve (take the max over candidates, or bound per candidate);
Radiance scaling of item SP; negative-SP lanes; `two_sided` blends already
handle the low side separately.

**Measure.** `benchmark_ab.py --expect-divergence` is the wrong tool here:
this must not change any top-15. Run with `SCORE_DENSE_CHECK=1` plus a new
tripwire asserting `reachable_ceiling >= greedy_score` on every scored leaf,
the three oracle fixtures, then `bench.py --scenarios families spell_wide`.
Report `gated / scored` before and after.

### R2. First-order (log-tangent) ceilings: a Lagrangian-style relaxation (exact, medium risk)

**Why.** The super-item bound evaluates `f(I)` where `I[k] = sum over slots
of max over items of stat k`. It credits every slot with the best value of
every stat at once. The knapsack literature's standard cure is a relaxation
that couples stats through prices: Lagrangian or surrogate multipliers
(Dyer-Zemel style bounds for MCKP, surrogate bounds in RECORD 2026, the
group-envelope bounds of Shao 2026 for MCKP families).

**The bound.** Each per-element damage term is a product of positive factors
that are either affine in the stat vector `x` (`a_k + c_k . x`: raw, %,
multipliers) or a concave curve of one skill-point lane
(`1 + m . f(s)` with `f = skillPointsToPercentage`, a geometric curve that
is concave and increasing; the element's own lane, Str through the
strength multiplier, Dex through the crit mix, which is affine in a concave
`f(dex)`). An affine function is concave, a concave increasing function of
a concave function is concave, and `log` of a positive concave function is
concave, so `log T` is concave in `(x, s)` jointly. For any linearization
point `p`:

```
log T(x) <= log T(p) + g(p) . (x - p),   g(p) = sum_k c_k / (a_k + c_k . p)
```

The right side is linear in `x`, so its maximum over completions is
separable: for each remaining slot take `max over items of g . x_item`.
That is one dot product per item and a max per slot, cheaper than a full
assemble. Sum per term: `f(x) <= sum_t exp(B_t)` where `B_t` is the per-term
bound. Two properties make it attractive:

- With `p = I` (the current super-item point), every per-term bound is
  `<= T(I)`, so the result is **never looser than today's ceiling** and is
  strictly tighter whenever no single item in a slot holds all the maxima.
- **Correction (review).** SP does *not* enter linearly: the gradient in
  each lane is taken through the curve, `h_lane = m f'(s_p) / (1 + m f(s_p))`,
  summed over the terms that read that lane (every element present in the
  combo has one, so Int, Def and Agi carry damage gradients whenever water,
  fire or air damage is present, not just Str and Dex). The tangent is then
  linear in the *raw* lane values, and only then does the joint 200-point
  budget from R1 become a fractional knapsack over the five lanes. Write
  the SP envelope down and test it on its own (bound `>=` true factor on a
  grid of lane values) before it goes into the bound.
- The same tangent is a cheap *incremental* ceiling: moving from one
  last-slot cluster to the next changes the bound by a dot product of the
  cluster deltas with `g`, instead of a full assemble and score per cluster
  as `dense_ceiling_cached` does today.

Any `p` is valid, so `p` can be tuned for tightness (a few Frank-Wolfe style
iterations: take the separable argmax, move `p` toward it) without risking
admissibility.

**Where it breaks (correction, review).** Positivity of the factors is not
enough: the implemented formula contains *sums of products*, and the log of
a sum of products is not concave. The review's counterexample is
`T(D, b) = D b + 100`: the exponentiated log-tangent at (20, 2), evaluated at
(10, 1), gives about 105.2 against the true 110, so a bound built that way
prunes a real completion. The couplings with this shape here are the crit
mix (`1 + p(dex) * (1 + critDamPct / 100)`, a product of two
decision-dependent quantities inside a sum), raw boosts scaled by the total
conversion fraction, and any raw addition that follows a product of
variables. The sound construction is: decompose each spell part's damage
into positive summands; apply the tangent only to a summand that is a
product of factors each provably positive and concave on the box; bound a
bilinear coupling by its super-item value or a McCormick envelope; and keep
the current super-item ceiling as the fallback for every unsupported term,
including factors that can go non-positive (negative % items driving
`1 + sum` toward zero), `max(0, .)` clamps, atree var-scaling terms and
min/max damage ranges. The admissibility proof is written per term before
any pruning uses it; `ceiling_vars_ok` is the model for the applicability
test.

**Measure.** Same tripwire as R1 (`bound >= true score` asserted at every
evaluated leaf in check mode), oracle fixtures, and `bound_pruned` /
`cluster_evals` from the stats line. The hypothesis to falsify: per-eval
cost stays within ~2x of the dense ceiling while `bound_pruned` rises enough
to pay for it.

### R3. Bound-based item fixing: a core-problem loop (exact, medium risk)

**What.** For every slot `j` and item `i`, compute `UB(i)` = ceiling with
slot `j` fixed to `i` and every other slot at its relaxed best. If
`UB(i)` is *strictly* below the cutoff, by the same float margin the ceiling
gate uses, delete `i` from the pool for the whole run (**correction**: not
`<=`; a build scoring exactly the 15th-best can still enter the top-15
through the item-name tie-breaker, which is why every existing gate prunes
strictly below the cutoff). Iterate:
smaller pools give smaller ideal points, which tighten every other `UB`.
Re-run whenever the shared cutoff rises meaningfully. Two rules from the
review: the cutoff is always an actual one made of distinct real witnesses
(the 15th-best distinct score), never an incumbent top-1; and a pool is
never mutated under a running search. Fix between immutable search
snapshots, rebuilding the suffix tables, band credits and ring-index tables
for the reduced pools, so counters and canonical ordering stay consistent.

This is reduced-cost / variable fixing, the engine of the core-based exact
MMKP algorithm (Mansini & Zanotti 2020: solve a core of promising items,
then prove the rest cannot help) and of RECORD's fixing-by-dominance
(da Silva, de Queiroz, Schouery 2026). The engine already has the core half
(`WARM_K`). It does not have the fixing half: today a hopeless item is
re-rejected inside every subtree it appears in.

Cost is ~1,500 ceiling evals per pass, negligible. Pool order is preserved
(deletion only), so level bands and ring canonicalization stay valid. It
generalizes tracker item 9 ("restriction-based iterated pool filtering")
from restrictions to the objective. It is much stronger after R1 and R2,
since a 2x-loose ceiling fixes very little.

**Also worth trying.** Pair fixing for the two most expensive slot pairs
(e.g. ring1 x ring2, about 12K canonical pairs): delete pairs whose joint
`UB` is strictly below the cutoff. This turns the ring pair into one merged
group, the classic MCKP group-merging move.

**Measure.** Pool sizes before/after at the warm cutoff; the oracle fixtures
must stay bit-identical; then the completing scenarios and the mage case's
projected completion time.

### R4. Set-aware dominance: the forfeit-set view (exact, low-medium risk)

**Mapping.** The Knapsack Problem with Forfeit Sets (D'Ambrosio, Laureana,
Raiconi, Vitale 2023) attaches a penalty to a subset of items once more than
an allowance `h` of them is chosen. Wynncraft sets are the same structure
with the sign flipped and a piecewise profile: a reward that depends on how
many of the set are equipped. Illegal sets are forfeit sets with `h = 1`
and an infinite penalty, i.e. conflict constraints. The literature's lesson
is to reason about a set's contribution as a bounded quantity attached to
the set, not to the item.

**What.** Dominance today exempts every set item. `DenseBound::build` already
computes, per set, the maximum positive transition delta for adding one more
piece (`set_delta`). So a set item `B` can be dominated by a setless item
`A` when `A` beats `B + max_transition(B.set)` on every higher-is-better
stat (and `B`'s set skill points are counted in its provisions, using the
same reachable set-SP term `sp_set_bound.js` now tests, after C2 fixes its
cross-set maximum). Negative transitions must be treated as zero in `B`'s
favour.

**Promise (correction, review).** Any dominance deletion, this one
included, preserves the optimum *value* and a best representative; it does
not preserve the literal top-15, because a dominated item can be the #2 to
#15 build by name. State the two promises separately in the UI: "top-1
value exact, list is representative" under dominance, "top-15 exact" only
with dominance off or certified-equal-only. Even certified-equal deletion
changes the literal list and its tie order (**gate, review follow-up**), so
exact top-15 mode either deletes nothing, or records each deleted item's
equivalence class and expands it back into the named results at the end
in the tie-break order the undeleted search would have produced.

**Measure.** Pool reduction on the family suite (the Nori and family
fixtures are set-heavy); `test_dominance.js` plus a new oracle that
re-enumerates with and without set dominance on small pools.

### R5. Optimality-gap reporting (no search change, high user value)

When the proof will not finish (mage: ~24h), the user should still learn how
good the answer is. The engine can maintain a **global dual bound**: the max
ceiling over all unexplored subtrees. Since enumeration is band-ordered, the
bound over the remaining bands is available from `subtree` / banded suffix
tables without visiting them. Report `best`, `bound` and
`gap = (bound - best) / bound` in the progress line and in the UI.

This is the standard MILP solver display, and it turns R1-R3 into visible
user value: a tighter bound shrinks the reported gap even when the run is
stopped early. It also gives a stopping rule ("stop at 1% gap") that is
honest, unlike an ML "probably optimal" guess.

**Correction (implementation pass, 2026-10-07).** "The bound over the
remaining bands is available from the banded tables" does not hold as
stated. After band `[0, H]` the unexplored set is *every leaf whose rank
sum exceeds `H`*, and that set is not a union of subtrees: under any
prefix, one slot can sit at a high rank while the rest are at rank 0. The
subtree ceilings bound rank from above (`h_child`), never from below, so
the best they give for "rank sum > H" is the root ceiling. Two honest
options, cheapest first:

1. **Per-partition frontier.** Native threads and browser partitions both
   own whole first-slot offsets and finish them completely. The dual bound
   is `max(best, max over unfinished offsets of the depth-0 ceiling)`, which
   falls as offsets finish. Depth-0 ceilings are loose (that is why
   `bound_max_depth` is 0), so measure how informative it is before
   building UI on it: the reported gap at 25/50/75% of wall time on the
   family suite.
2. **Rank-sum DP over a separable bound.** With R2's per-item linear upper
   bound `c_j(r)`, `max sum_j c_j(r_j) subject to sum_j r_j > H` is a small
   DP over slots and rank budget, recomputed once per band. It tightens as
   `H` grows exactly when priority order tracks contribution, which is what
   the band order already assumes. Depends on R2.

### R6. LNS with exact repair: an incumbent thread (heuristic, cutoff only)

**Why.** A higher cutoff earlier makes every bound bite earlier.
`readme_spell_wide` improved from 7.83M to 8.19M over minutes, which is
exactly the case a primal heuristic fixes.

**What.** A side worker that never prunes. It feeds real scored builds into
the shared top-N merge, and the cutoff stays what it is today, the 15th-best
*distinct* score among real builds (**correction**: it must never publish
its own best score as the cutoff; that would arm the gate above ranks 2 to
15 and the exhaustive run would no longer reproduce the exact top-15):

1. Start from the warm incumbent.
2. **Destroy**: unlock `k` slots (2-3), chosen by rotation or by which slots
   the elite top-15 disagree on.
3. **Repair exactly**: run the existing `Search` with the other slots locked.
   That subproblem is ~`200^k` leaves under full pruning, which completes in
   milliseconds.
4. Accept improvements; restart from a different elite build on stagnation.

This is the fixed set search matheuristic (Jovanovic & Voss 2024, applied to
MKP and to forfeit sets) with the engine standing in for the ILP subproblem
solver: elite solutions vote on which items to fix, the rest is solved
exactly. It is also what the community workflow quoted in the solver README
does by hand (lock accessories, search armour, swap, repeat). An
accessories-only neighbourhood is the natural repair for SP-infeasible
armour, since accessories carry most SP provisions.

**On GA / SA.** A slot-indexed genome is the right representation, but the
MMKP literature is consistent that population methods only become
competitive once a neighbourhood search is added (the empirical study in
Inderscience found five population metaheuristics indistinguishable once
each had local search). Repair by exact enumeration of `k` slots is a
stronger local search than GA crossover and reuses code that is already
oracle-tested. Recommend LNS first; a GA is worth it only if LNS stagnates
on the family suite.

**Measure.** Anytime metrics from R8: time to the final top-1, and primal
integral at 10/60/180s, with and without the LNS thread, across the family
suite. Because it only seeds the cutoff with real scored builds, it cannot
change an exhaustive result; the oracles confirm that.

### R7. Supervised ML for ordering and warm selection (heuristic, ordering only)

**Literature.** Rezoug, Bader-El-Den, Boughaci (Neural Processing Letters
2022) train classifiers on optimal solutions of small MKP instances to
predict which items appear in large-instance optima, then use the
prediction for GA initialization and repair. Neural Diving (Nair et al.
2020) and Predict-and-Search (Han et al., ICLR 2023) do the same for MILP:
predict a partial solution, then search a neighbourhood around it. The
Q-learning repair operator work (Khelifa et al. 2025) is a thesis-level
result on MKP benchmarks; treat it as weak evidence.

**Where it fits here, safely.** Only in places where the engine already
accepts a heuristic:

- **`WARM_K` selection.** Today: the top-6 per slot by solo ceiling. Replace
  or blend with a learned score `P(item in final top-15)`.
- **Pool order.** Blend the learned score into the sensitivity priority.
  Level bands stay exact regardless of order.
- **LNS destroy choice** (R6).

None of these can remove a build, so admissibility is untouched.

**Data and features.** Labels come from completed runs: the family suite,
the Nori catalogue and the build database under `research/`. Features must
survive patches, so no item IDs: priority score, solo ceiling, tangent-bound
score from R2, SP requirement and provision vectors relative to the
scenario, set membership and set progress, dominance depth, level. Start
with logistic regression or gradient-boosted trees; a GNN is not justified
at this data size.

**Measure.** Time to the final incumbent versus the solo-ceiling baseline,
on held-out families (leave-one-family-out, so the model is not tested on
builds it was trained on).

**Not recommended.** Using ML to declare "probably optimal" and stop. If
early stopping is wanted, R5's gap is the honest version.

### R8. Benchmarking: add anytime metrics to the existing harness

The fixed-work A/B harness is already better than most of the literature's
methodology. What is missing is the anytime side the ARCHITECTURE_PLAN asked
for:

- **Primal integral** (Berthold 2013): integral over time of
  `(best_known - incumbent(t)) / best_known`. One number per run that
  rewards finding good builds early.
- **Time to target**: time until the incumbent reaches 99% / 100% of the best
  known score.
- **Gap curve** once R5 exists: dual bound over time, which measures R1-R3
  directly.

Emit them from `bench.py --json` per scenario. Keep the rule that
correctness changes go through `--expect-divergence` and speed changes must
reproduce top-15 exactly.

## 3. The three approaches in the request, assessed

### MILP / CP-SAT

Fits poorly as the main engine:

- The objective is a sum of products of stat sums with concave SP curves and
  atree var scaling. Exact linearization needs McCormick envelopes or
  piecewise approximations of bilinear and trilinear terms over integer-valued
  sums. The LP relaxations of those formulations are weak, which is what makes
  MILP fast when it is fast.
- The SP cascade can be modelled (the bounty review showed feasibility is
  order-independent, `need[s] = max(req[s] + bonus[s])`), but negative lanes
  and the greedy allocation rule make it heavy, and the mana simulation with
  buff states is not MILP-shaped at all.
- The "exact optimality in milliseconds" claim holds for linear stat-sum
  objectives. Here the Rust engine already completes 334.8B-leaf melee
  spaces in ~1.5s, so there is little left to win on the targets that
  linearize.

Where it does fit:

1. **Linear targets** (`spd`, `poison`, `lb`, `xpb`, possibly `total_hp`) with
   linear restrictions are a true MMKP plus SP side constraints. CP-SAT
   could be an independent **oracle** for those targets in the test suite,
   which is valuable even if it is never shipped.
2. **Bound oracle.** The R2 linearization plus SP feasibility constraints is
   an MCKP with side constraints. Solving its LP gives a dual bound that
   accounts for SP coupling, which the separable bound cannot. Worth an
   offline experiment to see how much tighter it is than R2. If it is much
   tighter, use it at the root or for R3 fixing, not per node.
3. **Forfeit-set formulations** (KPFS) are the clean way to write sets in a
   MILP if this is ever pursued.

In the browser, HiGHS has a wasm build; OR-Tools CP-SAT does not run there
practically. Native-only use is fine for oracles.

### GA / SA

Useful only as an incumbent generator feeding the shared cutoff, never as the
answer. See R6: LNS with exact `k`-slot repair is the stronger form of the
same idea, and it reuses the engine.

### Pareto pruning and branch-and-bound / DP

Already implemented: dominance pruning (20-40% pool reduction) and a
sophisticated B&B. The 80-90% reduction figure assumes few stats; here ~80
stats are classified, so Pareto fronts stay large. The gains left are R4
(set items) and R3 (fixing by objective bound, which is dominance in
objective space rather than stat space). Exact DP over slots does not apply,
because the objective is not separable; meet-in-the-middle over
armour/accessory halves is equivalent to what the bounded enumeration
already does.

## 4. Second pass: more levers, and the "describe a playstyle" goal

Added 2026-10-07 after a closer read of the leaf pipeline, the SP kernel, the
restriction model and the research corpus. The goal these serve: the user
encodes a set of conditions (melee, spell spam, sustain, survivability
floors, owned items) and the solver returns the best build that meets them.
That splits into (a) being able to say the conditions, (b) evaluating them
exactly, and (c) finishing the proof fast enough to iterate on them. R9-R13
are about (c), R14-R19 about (a) and (b).

### R9. Exact SP bound at depth n-1 (exact, low risk)

**What.** `sp_kernel_reject` counts leaves the cheap per-lane SP bound admits
and the exact kernel then rejects. That headroom is claimable without
touching the leaf: at the last node before the leaves, run the exact kernel
once on `prefix + a synthetic free item carrying the last pool's per-lane
max provision plus every set-granted bonus still reachable from this depth,
summed per set as the Rust refresh_sp_bound_base does`. Provisions can only
help (the tome plan already relies on "a requirement-free bonus can never
make a build infeasible"), so if that solve fails, every leaf under the
node fails. **Correction (review):** without the set term the bound is
inadmissible. A prefix set piece requiring Dex 120 whose matching suffix
piece carries no Dex of its own but completes a +30 Dex set is rejected by a
provision-only synthetic item, although the real completion is feasible
with 90 assigned Dex. One kernel call per node
amortizes over the ~70-250 leaves beneath it.

**Why it matters.** On `solver_mage_gaia_6free_lvl50` the exact SP solve was
51.7% of wall and every leaf paid it. This moves the first solve up one
level where its verdict covers a whole pool.

**Measure.** `sp_kernel_reject` before and after; `benchmark_ab.py` on the
family suite (counters must stay comparable: this only changes where a leaf
is rejected, never whether).

### R10. SP conflict pairs (exact, low risk)

**What.** For every pair of items in different pools, solve the exact SP
kernel on `{weapon, locked, A, B}` plus a free item carrying the per-lane max
provisions of all other pools *and* every set bonus any completion of the
other pools could still unlock (the same reachable set term as R9; the
review's future-set-completion case applies here too). If that fails, `A`
and `B` can never coexist.
Store it as a bitset (~1,500 items squared is ~280 KB) and consult it when
placing an item, the way the illegal-set tracker already works. This is the
conflict-graph knapsack (the special case of forfeit sets with allowance 1
and infinite penalty), and it is the pairwise version of R9. Cost is ~1.1M
kernel calls at ~0.6 µs, under a second at startup, and it can be made lazy
(only pairs whose prefix is actually visited).

**Measure.** Pairs found, and the same funnel comparison as R9. Expected to
matter on tier-stack and high-requirement families, and to find nothing on
unrestricted spell spam.

**Extension: forward checking.** Store each item's compatibility bitset
(the items in every other pool it can coexist with). At each node, the
candidate set of every remaining slot is the AND of the placed items'
bitsets: an empty set backtracks at once, and the last-slot loop iterates
only the surviving candidates. This is constraint-programming forward
checking on the SP constraint, and it is where R10's per-node cost goes to
one AND per remaining slot per placement.

### R11. Mid-tree mana bound (exact, medium risk)

**What.** The doom precheck (a mana sim at the highest reachable Int) runs
only at the leaf (`scoring.rs`); `enumerate.rs` has no mana bound at all.
Mana feasibility is monotone in most of what the fast sim reads (`mr`,
`ms`, `int`, `hp`, cost reductions), so a doom sim at the suffix maxima of
those stats is a valid subtree bound, exactly as the restriction suffix
bounds extend the leaf precheck. **Correction (review):** it is *not*
monotone in `maxMana` whenever the combo carries a buff state with
`drain_pct_per_second.mana`: `compute_drain_override` drains
`drain_pct / 100 * max_mana` per second, so under a duration-capped state a
larger pool drains more, and a doom sim at the `maxMana` maximum can fail
where a completion with less max mana passes. So the bound must classify
each input's direction per scenario, the way the bounded-coupling doom
(task #28) already classifies var-effect outputs: evaluate `maxMana` at its
suffix *minimum* when such a state is present, or switch the bound off for
that scenario. The same two-sided check applies to any other input a state
reads as a percentage of a pool. On defensive objectives (B1) the mana sim is the
dominant per-leaf cost, and "reducing the number of leaves reaching the
simulation is what works" is already the recorded conclusion.

**Measure.** `spell_ehp` leaves/s and the `mana_reject` counter; the
`SCORE_DENSE_CHECK` tripwire extended to assert that any subtree the bound
prunes contains no leaf the full pipeline accepts (sample and verify).

### R12. Greedy SP allocation: audit it, then replace the trials (exactness, medium risk)

**The gap.** A leaf's score is "the score at the SP allocation the greedy
found" (coordinate ascent with steps 20, 4, 1 over five lanes). That is a
heuristic, so the solver's optimum is the optimum *under greedy allocation*,
and a build whose best allocation the greedy misses is under-scored. Nothing
in the test suite measures how often that happens.

**Step 1, audit.** An offline oracle: for a sample of scored leaves, enumerate
every allocation of `remaining` at step 5 over the five lanes (a few hundred
thousand at most, fine offline), score each, and report how often and by
how much the greedy falls short, per family. If it never does on the sample, that is evidence,
not a proof (**gate, review follow-up**): sampling cannot certify
integer-SP optimality. Exact mode therefore needs a certified allocator for
every case it supports (the exact scan over the present lanes below, the
closed-form Def/Agi split for the EHP family, and a declared refusal for
any case neither covers), or its label must read "optimal under greedy SP
allocation". "Optimal (proved)" never implies more than the allocator
proves. If it does, the leaf scorer is where the optimum is being lost,
and no amount of bound tightening fixes that.

**Step 2, structure.** **Correction (review):** damage is not a two-lane
problem. `calculateSpellDamage` applies `skillPointsToPercentage` of the
matching lane to each elemental component (Str to earth, Dex to thunder,
Int to water, Def to fire, Agi to air, line 141 of `damage_calc.js`), on
top of the Str multiplier and the Dex crit mix. The lanes that matter are
therefore Str, Dex, and every lane whose element is present in the combo's
damage after conversions; a mono-element build has two or three, a rainbow
build has five, and Int always also carries the mana side condition. Within
that set the objective has a known shape (products of the concave lane
curves), so an exact scan over the present lanes at step 1 is small for two
or three lanes and still bounded for five (the trials are 5 evals per step
today; an exact 3-lane scan at step 5 over 200 points is ~1,500 evals, so
do it only where the audit shows the greedy misses). When atree var effects
read SP, fall back to the trials. This makes the leaf score exact on the
common case; whether it is also faster depends on the lane count.

**Objective-specific exact allocators.** Non-damage targets go through the
same trial loop (`Objective::Indirect` in Rust, `need_thresh` in the JS
greedy), but EHP, EHP-no-agi, total HP, HPR and EHPR are closed forms in
`hp`, `def`, `agi` and `defMult`: the optimal split of the remaining budget
between Def and Agi, the only lanes they read, each through the same
concave curve, is a one-dimensional scan, and the mana side condition binds
Int separately. Replacing the trials with that scan on those targets is
exact by construction and removes the greedy's share of the per-leaf cost
on the defensive objectives, where the leaf pipeline is the bottleneck
(B1).

### R13. Result archive across runs (anytime, low risk)

**Why.** The intended workflow is iterative: run, look, tighten a condition,
run again. Today every run starts cold except for the warm subspace.

**What.** Keep an archive of scored builds from previous runs (the top few
thousand with their assembled stat vectors, not just the top 15). When a new
run starts with the same weapon, combo and objective and only the
restrictions changed, filter the archive by the new restrictions and insert
the survivors into the run's top-N buffer as real scored builds. The cutoff
is then derived as it always is, the 15th-best distinct score, so it arms
only when at least 15 distinct survivors exist (**correction**: seeding the
cutoff from the single best survivor would prune ranks 2 to 15 before the
buffer ever sees them; the best survivor is a top-1 incumbent for display,
not a cutoff). A score is unchanged by a restriction change, so the
survivors are admissible entries. Also insert the user's current UI build
on the Rust path the same way, as the JS engine does
(`_eval_current_build` / `_insert_top5`) and the Rust bridge does not.

### R14. Soft constraints and lexicographic objectives (expressiveness, exact)

**Why.** "At least 18k EHP" is a floor the user would trade a little damage
for, not a cliff. The restriction model has only hard `ge`/`le` thresholds
and one weighted blend. Two additions make most playstyle descriptions
expressible:

- **Penalised shortfall**: `score - w * max(0, threshold - stat)`. This is
  the forfeit-set penalty with the sign flipped, and it keeps the ceiling
  admissible: the penalty term is monotone in the stat, so evaluate it at
  the stat's suffix maximum, which the restriction bound tables already
  hold. Covers "prefer sustain but allow a little downtime" too, with
  mana deficit as the stat.
- **Lexicographic tiers**: maximise damage; among builds within `x%` of the
  best damage, maximise EHP. Implement as two passes: pass one finds the best
  damage `D*` exactly; pass two adds the restriction `damage >= (1 - x) D*`
  and optimises EHP, seeded from pass one's archive (R13). Each pass is an
  ordinary exact run.

### R15. Roll-robust evaluation (expressiveness, low risk)

**Why.** Items roll. The research corpus measured that needing eight
independent IDs at the 75% point is a ~1 in 52,000 event, and the roll
sensitivity tables show which builds only meet their floors at generous
rolls. A build that is optimal at 85% rolls and infeasible at 50% is not the
build the user will own.

**What.** Two stat vectors per item: one at the objective roll profile, one
at a conservative profile for the constraints. The pools are pre-baked per
roll group already, so this is a second bake. Constraints, SP requirements
and the mana check use the conservative vector; the objective uses the
optimistic one. Both bound families stay admissible because each uses its
own vector's suffix maxima. A stricter variant (worst case over a roll
range for the objective too) is the robust-knapsack setting of Shao 2026,
and that paper's group-envelope bounds are the tool if it is ever wanted.

### R16. Weapon as an outer group (scope, exact)

**Why.** A10 in the support matrix: the weapon is fixed, so "best build for
this playstyle" today means "best build for this weapon". The user's goal
needs the weapon chosen too.

**What.** Treat the weapon list as an outer group. Each weapon is its own
run (the combo, atree and sensitivity weights depend on it), but they share
one top-N when the objective is comparable (same combo rows and target):
a build found under weapon A is a real build, so it enters the cross-weapon
top-N, and the cutoff is that merged list's 15th-best distinct score
(**correction**: not weapon A's best; that would prune weapon B's ranks 2
to 15). If the user wants a top-15 *per weapon*, each weapon keeps its own
cutoff and only the top-1 is shared as a display incumbent. Order weapons by their solo ceiling (R3's `UB(i)` with
the weapon as the fixed item) and run best-first; most weapons are then
fixed out before their run starts. The browser bridge already spawns one
single-threaded engine per worker, so weapons map onto workers directly.

### R17. Diverse top-N (presentation, no change to the optimum)

Fifteen results that differ by one ring are fifteen copies of one answer.
Keep the exact top-1, and fill the remaining slots with the best builds that
differ from every kept build in at least `k` slots (k = 2 or 3). Implement as
a second-pass filter over R13's archive so the exact top-15 is still
available on request. For the iterative workflow this shows the user the
alternatives a tighter condition would select.

### R18. Playstyle presets from the research corpus (product, no engine change)

`research/build-database/threshold-profiles.json` already defines families
(spell sustained, spellsteal, heavy melee, hybrid) as hard constraints plus
an optimise list, and the family suite encodes six validated seeds. Expose
those as restriction templates in the UI: pick "sustained spell", get the
mana horizon, trough and EHP floors pre-filled, then edit. Combined with R14
(soft floors) and R16 (weapon choice) this is the "describe the playstyle"
front end; the exact engine underneath is unchanged.

### R19. Checkpoint and resume (enabling, planned as P3.2)

Long proofs (the mage case at ~24h) are only usable if a run survives a
closed laptop. P3.2 is already on the tracker; it is listed here because
R5's gap and R13's archive both need a durable run record, so the three
share one job directory format.

### R20. QoL ranking over a windowed archive (presentation plus one exact search rule)

**The complaint.** The top-5 panel usually shows one build five times with
an accessory swapped, ranked by a single number. The user would often take
the build that is 3% behind on damage if it has a comfortable mana margin,
more EHP and better walk speed. Today nothing surfaces that trade.

**What the community actually ranks by.** The repo's own guide
(`research/wynncraft-endgame-class-building-guide.md`, "ranking order")
puts it as: requirements and legality, then EHP and sustain, then damage,
then range, movement, AoE and quality of life as tie-breakers, and notes
that "a 2 percent damage gain can be a downgrade if it loses the only
comfortable mana margin". Forum consensus numbers worth using as defaults:
8 to 12 mana regen for a sustained spell build, with under 8 only with
another mana source; walk speed in 20% tiers (each 20% is one Speed level,
so utility is a staircase, not a line); non-dodge EHP floors by content,
which `threshold-profiles.json` already tabulates (18k general combat,
higher for heavy melee and guild wars). None of these are official rules;
they are the starting defaults a user should be able to override.

**Design: separate what the search optimises from how results are ranked.**

1. **Windowed archive (exact).** Instead of keeping the top 15 by score,
   keep *every* feasible build whose score is within `x%` of the incumbent,
   and set the shared cutoff to `(1 - x) * best` rather than the 15th-best
   score. That cutoff is still a real scored build's score, so every bound
   and gate stays admissible; pruning is slightly weaker, by a measurable
   amount. The payoff: at the end of an exhaustive run the archive provably
   contains *all* builds within `x%` of the optimum, so any re-ranking of
   it is exact over that window. Cap the archive (say 2,000 entries,
   evicting the lowest score); if the cap binds, the completeness claim is
   withdrawn, the report says "top 2,000 of the window", and the user is
   told to narrow `x` (**gate, review follow-up**: a capped or truncated
   archive never claims to hold every build in the window). Equal-stat
   deletions (R4) apply here too: the window is complete over
   representatives unless dominance is off or expanded back.
   Store the assembled stat vector and the mana-sim summary per entry;
   both are already computed at scoring time.

2. **Tunable QoL utility (client-side, instant).** Rank the archive by
   `U = score_norm + sum_k w_k * u_k(stat_k)` where each `u_k` is a
   saturating utility, not a raw stat:
   - mana margin: `mana_end` and `mana_trough` from the sim, saturating at
     a comfortable margin (default: +1 mana regen tier above sustain);
   - non-dodge EHP: ramps from the content floor to a saturation point;
   - HPR and life steal: saturating on raw regen per second;
   - walk speed: a staircase on 20% tiers;
   - attack speed tier, range, spell cost margin where relevant.
   Each `w_k` and each saturation point is a slider with a preset per
   playstyle (R18). Changing a slider re-sorts the archive instantly; no
   re-solve. Show the components as columns so the user can see *why* the
   third build outranks the first.

3. **Diversity inside the ranking (R17).** Collapse builds that differ in
   fewer than `k` slots to their best representative before showing the
   list, with an expander to see the variants.

4. **Optional: utility as the search target.** If the user wants the search
   itself to maximise `U`, that is a `custom` blend whose terms are monotone
   non-decreasing transforms of monotone stats, so the existing ceiling
   argument (non-negative weights, each term at its suffix maximum) still
   holds and the gate stays exact. Saturating transforms keep it monotone.
   Do this only after the archive version proves the utility is right; a
   bad utility baked into the search is expensive to iterate on, the same
   utility applied to an archive is free.

**Why the window and not a bigger top-N.** A top-500 by damage is still
500 near-copies of the best build. The window is defined on the quantity
the user is willing to trade, and it is the window size `x` the user
chooses ("I'd give up 5% damage"), which is the question they are actually
asking.

**The exact multi-objective route, for later.** The window approximates the
Pareto set of (damage, EHP, ...) by a slab below the damage optimum. The
exact alternative is bi-objective branch-and-bound with *bound sets*: a
node is discarded when a set of points, not one ideal point, separates its
feasible completions from the current nondominated set (Ehrgott and
Gandibleux 2007; Cerqueus, Przybylski and Gandibleux 2015 for knapsack).
Two objectives is tractable with the engine's per-objective ceilings as the
bound set; three or more is not worth it here. Keep it as the follow-up if
users want the full damage/EHP front rather than a damage-first window.

**Measure.** Pruning cost of the window cutoff versus the top-15 cutoff at
`x` = 2, 5, 10% on the family suite (fixed-work A/B, expect divergence in
`gated`, none in the top-1); archive size at the end of each completing
scenario; and a user check that the default weights reproduce the guide's
ranking order on the six validated family seeds.

### R21. Epsilon-optimality tolerance (exact in the approximation sense, low risk)

**The observation.** Most of a long proof is spent on subtrees whose
ceiling sits just above the incumbent. The mage case finds its best build
in seconds; the remaining ~24h is proving that nothing beats it by even one
point. Nobody needs that: "no build beats this by more than 1%" is the
useful statement, and it is far cheaper to prove.

**What.** A user-set tolerance `eps` (default 0, so nothing changes until
asked). Prune a subtree when `ceiling` is strictly below `(1 + eps) * best`
(same margin rule as R3) instead of strictly below `best`, and stop the run
when the global bound (R5) satisfies the same test. The result is guaranteed
within `eps` of the optimum; that is the `MIPGap` rule every MILP solver
ships with, and the guarantee is as exact as the ceiling is admissible.
**Definitions (review):** the multiplicative form is defined only for
`best > 0`; objectives that can be zero or negative (custom blends with
negative weights) take an absolute tolerance `eps_abs` instead, or `eps` is
refused for them. The claim is about the **top-1 only**: a pruned subtree
may hold a build better than the current 15th-best, so under `eps > 0`
ranks 2 to 15 are reported as unverified. A strict comparison protects ties only at
`eps = 0` (**gate, review follow-up**): with `best = 100` and `eps = 1%`, a
subtree whose bound is exactly 100 is below the 101 line and is pruned, so
under `eps > 0` an alternative optimum tied with the incumbent can be lost.
The promise is "a build within `eps` of the optimum", and says nothing
about ties. For a genuine gap (R5), every `eps`-pruned region's bound is
kept in the ledger alongside the queued, active and unmaterialised work, so
the reported gap bounds all of it. It composes with every bound in R1-R3,
R9-R11: each gets `eps` more bite. The UI reports "optimal within 1%" with
the proof status, which is the honest replacement for a progress bar that
stalls at 97%.

**Interaction with R20 (correction, review).** They are *not* independent.
The window archive needs every build scoring at least `(1 - x) * best`,
and any subtree whose ceiling is above that line may hold one, so with a
complete window the admissible prune line is `(1 - x) * best` and `eps`
buys nothing on top of it (`(1 + eps)(1 - x) best` is strictly above the
window boundary and would drop builds inside the window). Offer the two
modes explicitly: *exact window* prunes at `(1 - x) * best` and ignores
`eps`; *approximate window* prunes at `(1 + eps)(1 - x) * best`, which is
meaningful only while `(1 + eps)(1 - x) <= 1` (**second correction,
review**: with `x = 1%` and `eps = 2%` the line is `1.0098 * best`, above
the incumbent, so it can prune a subtree holding an undiscovered better
build; that build is still within `eps` of the incumbent, so the top-1
claim survives, but no statement about the archive does). When the product
exceeds 1 the mode reduces to the exact window. The UI states that the
archive is complete only above the prune line. `eps` on its own (no
window) keeps the top-1 guarantee stated above.

**Measure.** Proof time at `eps` = 0, 0.5%, 1%, 2% on the mage scenario
and the family-large suite; confirm top-1 is unchanged at every `eps` on
the completing scenarios (it must be, unless the exact optimum is itself
within `eps` of the found build).

### R22. Best-bound prefix scheduling (exact, medium risk)

**Today.** Enumeration is level-band order: sum of pool ranks, a static
heuristic fixed before the search. Workers claim first-slot offsets, which
are coarse and leave a tail where one thread finishes a large subtree
alone. In the browser, `solve_json_full` hands each worker a static
partition, with no stealing.

**What.** Materialise prefix nodes at a fixed depth (2 or 3 slots, tens of
thousands of nodes) with their ceiling (R2/R3) and run them from a priority
queue ordered by bound, highest first. This is best-bound node selection,
the default in MILP solvers, and it does three things at once:

- the global bound (R5) is the maximum over the queue head *and every
  prefix a worker currently holds* (the bound of its unexplored remainder),
  with any region not yet materialised carrying its parent's bound and
  every `eps`-pruned region (R21) keeping its bound in the same ledger
  (**correction, review**: incumbent 100, queue head 99 and a claimed
  prefix at 150 is not a proof). Tracked that way the gap is exact and
  still cheap;
- the search spends its time where the optimum can still be, so the
  incumbent improves faster and `eps` (R21) is reached sooner;
- the queue is the work unit for everything else: dynamic claiming across
  native threads and browser workers alike (one atomic per claim, the
  cutoff SAB already exists), a fine-grained tail, and the checkpoint
  record for R19 (a prefix node is done or not done).

Within a prefix, keep the band sweep as today; only the order *between*
prefixes changes, so the covered space and the final top-15 are identical to
the band order's. **Correction (review):** the cutoff-dependent counters
(`bound_pruned`, `gated`, `scored`, `feasible`) will *not* match, because a
different order finds the incumbent and the 15th-best at different times,
exactly as they differ between 1 and 4 native threads today. So measure
this with `benchmark_ab.py --expect-divergence`, asserting total credited
leaves at completion and the top-15, not counter equality.

**Caveat.** Best-bound order is memory-hungry in MILP because the tree is
unbounded; here the depth is fixed, so the queue is bounded by the prefix
count. Diving (depth-first within a prefix) keeps the incumbent moving,
which the band sweep already does.

### Smaller notes

- **Lock hints from the archive.** When every build in the window archive
  (R20) shares the same item in a slot, say so and offer a one-click lock.
  Locking a slot removes a factor of ~100 from the space; this is the
  manual workflow from the README, made visible.
- **Native first for long proofs.** The browser engine has no clock,
  one thread per worker and no checkpoint. Anything that will not finish
  in minutes belongs on the native CLI with all cores and R19; make the
  UI export the job and say so when the estimate (the live search-space
  line) exceeds a threshold.
- **Pool order by solo ceiling.** Pools are ordered by the sensitivity
  priority score; `WARM_K` ranks by solo ceiling instead. Measure which
  order finds the final incumbent sooner across the family suite, since
  the band sweep's anytime quality is entirely that ordering.
- **Warm-start-informed reorder.** Before the main run, move items that
  appear in the warm subspace's top-15 to the front of their pools. Zero
  ML, zero risk, and it is the cheap baseline R7 has to beat.
- **Nested cores by level.** `WARM_K` nests by priority rank. A second core
  nested by level (`lvl_min` 100 first, then the full range) is the same
  trick with a different ordering; the tracker already observed the lvl-100
  space is 254x smaller and finds a near-optimal build.
- **Slot order by bound tightness.** Free slots are ordered by pool size.
  Ordering by how much the suffix maxima shrink when that slot is fixed
  (bound tightness) would make R2/R3 bite earlier. Measure per family.

## 5. Existing unmerged branches (reviewed 2026-10-07)

Five branches carry commits master does not have. Two are superseded, one
is a large finished piece of this roadmap, one is a correctness finding
with tests, and one is a conflicting bundle with three real fixes inside.

| Branch | State vs master | Verdict |
|---|---|---|
| `codex/review-and-optimize-wynnsolver-performance` | 1 ahead, 122 behind | **Delete.** Its one commit (SP maxima restore from a stack, trace phases, `top_results.js`) is on master in the same shape (`_sp_max_save`, `_TRACE_PHASES`). |
| `codex/rust-wasm-solver` | 5 ahead, 96 behind | **Delete.** Superseded by PR #5. Its two admissibility fixes (seed dedup by item names, `ceiling_vars_ok`) are both on master. The `search_core.rs` layout never landed and the current `enumerate.rs` replaced it. |
| `codex/current-family-benchmarks` | 1 ahead, 0 behind | **Merge.** See below. |
| `agent/anytime-neighborhood-benchmarks` | 4 ahead, 0 behind | **Merge, after trimming the evidence tarballs.** See below. |
| `agent/exact-solver-optimization-validation` | 1 ahead, 6 behind, 7 conflicts | **Do not merge; cherry-pick three fixes.** See below. |

Fully merged branches (`agent/add-family-benchmark-variations`,
`claude/build-algorithm-optimization-egcj4c`, `claude/rust-wasm-phase2`,
`claude/wynnsolver-performance-traces-0131sj`) can be deleted.

### `codex/current-family-benchmarks` (PR #17): a correctness finding on master's default

Adds `engine/candidate_reducer.js` with explicit pruning policies
(certified, balanced, legacy, aggressive, off), a "fast then verify" mode,
15 current-meta archetype snapshots at six removal depths (90 fixtures), and
`research/guarded-item-pruning-implementation-findings.md`. Its headline
result is against **current master's default dominance**: on Mage
Riftwalker cancelstack with three slots free, the default prunes
Knucklebones (+3 attack tier) as dominated by an empty bracelet, because
attack tier was not a dominance dimension in that scenario, and the
exhaustive optimum drops from 350,291 to 314,987 (a 10.08% loss). The
certified policy matched the unpruned optimum in all 34 exhaustive controls;
balanced matched every available control at 56% space reduction.

Checked here: the branch is on current master with no conflicts, and its
tests pass (`test_dominance.js` 68/68, `test_current_meta_benchmarks.js`
1,153/1,153). Its "Test 20" pins the Knucklebones case. Merge it; then this
roadmap's R4 (set-aware dominance) builds on the certified policy rather
than on the legacy one. One question for the merge: the branch makes
*balanced* the product default, which is still a heuristic. Given the
finding, certified-by-default with balanced as an opt-in speed mode is the
safer reading of its own numbers.

### `agent/anytime-neighborhood-benchmarks` (PR #18): R6 is already built

This is the LNS incumbent thread (R6), the warm-witness retention half of
R13, and the archive deduplication of R17, implemented in Rust
(`enumerate/anytime.rs`, 1,174 lines) with a browser "Quick search" mode
(5/15/30 s budgets), a 132-query quality suite, 1,209 recorded runs, and
frozen-target methodology that is better than most of the literature cited
above. Honest about its limits: it never reports completion, and the
reports separate score gains from timing gains. On the six-slot-free
archetype queries the follow-up profile reached the frozen 99% target on
66/66 runs where plain enumeration managed 36/66, with endpoint score gains
of +38% (Arcanist), +51% (Light Bender), +28% (Ritualist tierstack) at a
five-second budget.

It also carries a **Rust scoring bug fix**: healing parts use the current
`power` field, and the Rust evaluator only recognised the legacy
`max_hp_heal_pct` alias, so it returned zero healing on 15 of 132 exported
fixtures (every Light Bender and Acolyte snapshot) where the JS returned
positive values. That fix alone is worth merging.

Checked here: no conflicts with master, `test_quick_search.js` 18/18,
`cargo test --release --lib` 35/35. Two
things to trim before merging: `rust/sp_kernel/evidence/**` holds ~7.6 MB
of `.tar.gz` raw campaign archives, which belong in a release asset or a
separate evidence branch, not in the source tree; and the shipped wasm
grows from 632 KB to 893 KB, which the browser pays on every load, so the
anytime module should probably be a separate wasm build or feature-gated.
After merging, R6 becomes "integrate Quick search's archive with the
exhaustive run's cutoff" (the branch keeps them as separate modes) and R20's
windowed archive can reuse its diverse-archive code.

### `agent/exact-solver-optimization-validation`: three fixes worth taking

A single 4,298-line commit from 2026-08-15 bundling the reachable set-SP
bound (which landed separately as PRs #15 and #16), an adaptive ceiling
memo (master took a different fix), dominance policy modes (overlapping
with the branch above), two benchmark harnesses, and three game-correctness
fixes. It conflicts with master in seven files, and its own docs say its
medium/large timings are projections whose raw data was not retained. Do
not merge it. Cherry-pick these, each as its own small PR with its own test:

1. **Set weapons (C1, live).** `calculate_skillpoints` iterates `equipment`
   for set counts and the weapon is passed separately, so a non-crafted set
   weapon never activates its set in either engine; the Rust loader counts
   the weapon's requirements but not its set id. An earlier draft here
   called this latent after grepping a `set` key on the raw JSON; the raw
   field is `sets` (an array) and the loader writes `item.set` from the set
   tables, and 44 weapons in 2.2.3.0 carry one. The review reproduced it on
   Bony Bow + Bony Circlet: one piece counted, the two-piece row's +8 Agi,
   +45 mdRaw and +15 aDamPct lost. A correctness fix in both engines, with
   that pair as the test. The anytime branch refuses set weapons for this
   reason; after the fix it need not.
2. **EHP precheck at 100 Def/Agi.** `_build_constraint_prechecks` computes
   the optimistic EHP divisor with `skillPointsToPercentage(100)`, but
   total Def/Agi reach 150 with item provisions, so the precheck is not an
   upper bound and can reject a build whose real EHP meets the `>=`
   threshold. The divisor is exported to the Rust fixture, so both engines
   share the hole. Confirmed on master. Fix: evaluate at the reachable cap
   (150, or the R1 per-lane cap), which also closes tracker queue item 2's
   EHP half.
3. **Maximum mana cap.** The branch clamps start mana to 400 in both
   simulators. The wiki's Mana page states no cap, so verify this against
   the live game before adopting; if it is real, it is a one-line change in
   each simulator, and it is a feasibility fix (the sim currently
   overestimates mana on high-Int, high-maxMana builds).

The raw `>=` precheck ignoring set and tree contributions (the other half
of tracker item 2) is also disabled on that branch; it is C3 in section 0,
a bug rather than an open decision (the Jester bracelet-and-ring case), and
the branch's "disable the gate" is the safe interim fix until a proven
envelope exists.

## 6. Third pass: engine, browser and data levers (2026-10-07)

From reading the Rust leaf path, the browser bridge, the build settings and
the item data, plus the measurements the engine's own probes already
recorded but nothing acted on.

### R23. Incremental leaf fill in the Rust engine (exact, low risk, measured headroom)

**Fact.** `LeafState::fill_direct` rebuilds every leaf from scratch: copy
the template value and presence vectors, then re-apply all eight items'
stat deltas, each found by a string-keyed `dd.items.get(name)`. The
`SCORE_TRACE=2` probe added in `2050186` measured **0.82 to 1.16 of the 8
slots differing between consecutive leaves** across ehp, spell_wide,
spellsteal, tierstack, hybrid and melee_restr: the search walks one slot,
so roughly seven eighths of the per-item work recomputes what the previous
leaf already had. The cluster-bound path already caches a prefix state per
node (`enumerate.rs` around line 1192); the leaf path does not.

**What.** Keep a per-depth journal of the stat writes each placed item
made, and at the leaf apply only the changed slot(s), rolling back by
restoring journaled old values rather than subtracting (the same trick
`dense_ceiling_cached` uses; a subtract-based running vector is not
ulp-exact). Journaling makes only the *rollback* exact (**review**): the
forward result is bit-identical to a full rebuild only if the adds land in
the same arithmetic and stage order as `fill_direct` applies them, which is
item position order, not search depth order. So the incremental path must
replay that order, or the parity suite must accept and bound an ulp
difference. Resolve item names to integer ids at load so the hot path does
no string hashing. The BASE phase's share of leaf time, per family, bounds
the gain; the trace already splits it out.

### R24. Resumable chunked solve: cutoff sharing in the browser, checkpoints, tails

**Facts.** Cross-origin isolation is off by default since `fc3e451` (the
service worker broke the live site twice), so browser partitions have no
`SharedArrayBuffer` and `js/solver/wasm/worker.js` returns `undefined` for
the cutoff: **partitions never share a cutoff in the deployed default**.
Measured natively, four partitions scored 384 leaves where one scored 78,
which is why 4-way gives ~1.8x instead of 4x and `search.js` refuses to
partition below 8M leaves. The engine is one synchronous call per
partition, so a worker cannot receive a `postMessage` mid-run; WASM.md
names "making the engine resumable across chunks" as the way out and it
was never built.

**What.** Make `Search` resumable: run for a leaf budget (`max_leaves`
already exists), return a cursor (band position, running state, local
top-N), yield to the event loop, exchange the 15th-best cutoff by
`postMessage`, continue from the cursor. One mechanism then serves three
needs: cutoff sharing without isolation, R19's checkpoint record, and the
fine-grained tail units R22 wants. Also stop structured-cloning the
~880 KB score fixture per worker: post it once as a transferable buffer or
build it inside each worker from the shared game data (P1.9).

### R25. Build-flag experiments (cheap, exact)

- `build-wasm.sh` runs `wasm-opt -Oz`, the *size* preset. Try `-O3` and
  `-C target-feature=+simd128` and measure the in-browser rate on `armor4`
  and `spell_wide`; expect a few percent to low tens, paid in module size.
- Native already has `target-cpu=native`, LTO and `codegen-units = 1`.
  Profile-guided optimisation (`-Cprofile-generate` over the family suite,
  then `-Cprofile-use`) is the remaining compiler-side lever and is
  typically worth 5 to 15% on branchy code like the enumerator.
- Both are measured with `benchmark_ab.py`; neither can change a result.
- Follow-on, once R23 lands: evaluate the last slot's candidates in
  structure-of-arrays batches of 4 or 8 so the damage arithmetic vectorises
  (AVX2 natively, simd128 in wasm). `GPU_PLAN.md` measured that batch-shaped
  work at ~29% of a spell search, so the ceiling is about 1.4x on damage
  targets and nothing on the others: cheap to try, not a priority.

### R26. Never-used item detection (exact where it can be, labelled where it cannot)

**Not availability filters.** An earlier draft proposed tradable-only and
owned-only pools. That answers a different question: the optimal build
routinely needs the untradable quest reward or the mythic, and players go
and get it. What would shrink the space is removing items that *cannot*
appear in the answer, and there are three tiers of that, in decreasing
strength:

1. **Exact, per run: bound fixing (R3), applied slot-wise.** `UB(i)`
   strictly below the cutoff deletes `i`; once every item but one in a slot
   is fixed out, the slot is locked for the rest of the run. That is the
   group-fixing step of reduce-and-solve for MMKP (Chen and Hao 2014) and
   the exact form of the lock hint. Its strength is the bound's tightness,
   which is why R1, R2 and R28 come first.
2. **Exact, per data version: full-space dominance.** Dominance today is
   projected onto the stats the current objective reads. An item dominated
   on *every* stat the engine can read, with requirements no cheaper and
   provisions no higher, in the same slot and outside any set, cannot be in
   any optimum for any objective, so it can be dropped at data-build time.
   Expect little: Walker and Dyer (1998) show the undominated fraction
   grows quickly with the number of resource dimensions, and here there are
   ~80 live stats. Count it once on the 2.2.3.0 data before building
   anything.
3. **Heuristic, opt-in, labelled: persistence.** Across the archive of
   completed runs for a class and weapon family, items that never entered
   any window archive form an exclusion list the user can enable for
   discovery runs. This is the core idea applied across runs, and it is
   exactly the pruning the tracker says must never back an exhaustive
   claim, so the UI says "n items excluded by history" and proof mode
   ignores the list.

Availability data (522 `untradable`, 20 `quest_item`, 82 mythics in
2.2.3.0) still earns its place as *display* information on results ("needs
Divzer, mythic; needs an untradable quest reward") and as an explicit
opt-in filter for a user who wants a tradable-only answer, never as a
default.

### R27. Measure, then cache, the browser preparation phase

`buildEnumFixture` and `buildScoreFixture` run synchronously on the main
thread at every solve, serialising pools, the lowered atree and the scoring
plan (~1 MB). No measurement of that phase exists; the browser e2e logs
wall time only. Measure it per scenario size. If it is seconds, cache the
fixture by a hash of (weapon, atree, combo, pools, roll mode) across solves
(it pairs with R13: only restrictions change between iterations), or build
it in a worker.

### R28. Suffix Pareto fronts: meet-in-the-middle bounds (exact, medium risk)

**Why.** Every bound in the engine upper-bounds the unplaced slots by
per-stat maxima, a "super-item" no real item matches, which is where the
recorded ~2x looseness comes from. The tangent bound (R2) tightens the
objective side but still takes each stat's maximum independently across
slots.

**What.** For the last `k` slots (start with `k = 2`: the two rings, or
bracelet and necklace), precompute the real `k`-tuples and keep only their
Pareto front in the objective's monotone stat space (the same
higher-is-better classification dominance uses, plus requirements as
lower-is-better, provisions as higher-is-better, set transitions as in
`DenseBound`). At a node at depth `n - k`, the subtree ceiling is the
maximum of the objective over the front's points placed on the prefix,
not over a super-item. The front covers every real completion, so the
bound is admissible, and it is as tight as a bound over real completions
can be.

**Cost.** 150 x 150 ring pairs are ~11K canonical tuples; a front in five to
eight effective dimensions is typically hundreds of points, so a node pays
hundreds of cheap evaluations (dot products, in the tangent form) to prune a
subtree of 11K leaves, against the ~2,800 cluster evaluations the last-slot
cluster bound spends on the same subtree. This is the Horowitz-Sahni
meet-in-the-middle idea used as a bound rather than as a solver, and the
dominance between partial solutions that multi-objective knapsack dynamic
programming relies on to keep its state sets small (Bazgan, Hugot and
Vanderpooten 2009).

**Interaction.** Fronts are per objective direction and per data version,
so they are built once per run (or cached by R27's hash). A front used for
bounding does not license deleting the pairs it represents (**correction,
review**): a dominated pair can still be the literal #2 to #15 build. Pair
deletion comes only from R3's rule, `UB` strictly below the distinct-witness
cutoff, evaluated over the full domain; the front merely makes that `UB`
cheap. Set bonuses enter only through the same transition deltas the
current bound uses, and those deltas should themselves be tightened to the
best row among the piece counts actually reachable from the prefix, as the
SP term already does, instead of crediting the best single transition on
every added item (**review**).

**Measure.** `bound_pruned` and proof time on the family suite against the
cluster bound, with `--expect-divergence` (counters shift, the top-15 must
not); front sizes per family, which decide whether `k = 3` is affordable.

### R29. Priority-gap levels: weighted discrepancy bands (anytime, medium risk)

**Why.** Level-band enumeration is limited discrepancy search (Harvey and
Ginsberg 1995) with one discrepancy per rank step in any slot, so stepping
from the best helmet to the second-best costs the same as the same step in
a bracelet pool whose top items are near-identical. The tracker's own first
"high priority improvement" asks for exactly this: explore accessory
alternatives deeper per armour combination, because armour moves the score
far more. Weighted discrepancy search is the general form.

**What.** Give each item an integer *level* from its priority-score gap to
the pool's best item, quantised so that near-equal items share a level and
a large gap costs several; enumerate bands over the sum of levels instead
of the sum of ranks. The band machinery stays, since bands are still integer
sums, but an offset now maps to a group of items, so the band credits, the
ring canonicalisation and the first-slot partitioning need the group form.
Exactness is untouched: the same visited set, in a different order.

**Measure.** Time to final incumbent and primal integral (R8) on the family
suite against rank bands. Also try a *portfolio*: two workers running
different orderings (rank bands, gap bands, solo-ceiling order) sharing one
cutoff once R24 lands. Portfolios are the cheapest robust anytime gain in
the search literature, and they cost nothing in exactness.

### R30. Kernel search as the schedule for cores and fixing (exact once the buckets close)

**The recipe.** Kernel search (Angelelli, Mansini and Speranza 2010; the
two-phase MMKP variant of Lamanna, Mansini and Zanotti 2022) organises
"prune, then solve exactly on the rest" as a loop: rank items by a promise
score from the LP relaxation; take the top items as the *kernel*; solve the
restricted problem exactly; add the next *bucket* of items and solve on
kernel plus bucket with the constraint that at least one bucket item is
used (otherwise the solve repeats work); keep the bucket items that entered
the solution in the kernel; repeat. With a bound test per bucket (skip it
when the restricted problem's upper bound cannot beat the incumbent) and
the buckets run to exhaustion, the heuristic becomes the core-based exact
algorithm of Mansini and Zanotti 2020, from the same group.

**Mapping.** The kernel is `WARM_K` (top-k per slot). The promise score is
the solo ceiling today and `UB(i)` from R3 once it exists, which is the
reduced-cost analogue for a nonlinear objective through R2's tangent. The
restricted exact solve is this engine on reduced pools, which the warm
start already runs. "At least one bucket item" is one slot restricted to
the bucket, which the band enumerator expresses as a prefix set. The
bucket bound test is R3's fixing. In the quoted recipe, the ML classifier
is R7 and the iterated local search is R6; the MIP solver's role is played
by the exact engine, because the objective does not linearise.

**What it adds.** A schedule. Instead of warm start then full run, run
nested cores: `k = 3`, then buckets of the next three per slot, each solve
over the *cumulative* set (kernel plus every bucket admitted so far), never
kernel-plus-one-bucket (**correction, review**: for an objective like
`x * y`, two core items at (2, 2) score 16, outside items (5, 0) and (0, 5)
score 14 each with a core partner and 25 together, so neither enters on its
own and a one-bucket-at-a-time schedule never finds the pair). A bucket is
skipped only when `UB` over the *full* domain of every item in it is
strictly below the distinct-witness cutoff, and the final claim needs every
outside item fixed out that way or the remaining space enumerated: the
proof is a partition of the whole space, not a sequence of restricted
solves. The kernel grows by whatever enters the top-N. Every
intermediate answer is exact for its core, the incumbent and cutoff only
rise, and the final sweep of the remaining buckets, under the bounds, is
the proof. R22's prefix queue is the natural executor (a bucket is a set of
prefixes). Exhaustiveness is claimed only when every bucket has been solved
or bound-skipped, never from the kernel alone.

**Measure.** Primal integral and proof time on the family suite against
warm-start-then-full-run; the fraction of buckets skipped by bound, which
is the direct measure of how much R1, R2 and R28 bought.

### R31. Graph neural networks and deep reinforcement learning: assessed, mostly no

**What the results actually are.** The strong results in this area come
from distributions of generated instances with a solver in the loop. Gasse
et al. (NeurIPS 2019) learn MILP branching from strong-branching labels on
thousands of instances and gain a constant factor in node selection.
Neural Diving and Predict-and-Search (Nair et al. 2020; Han et al. 2023)
predict a partial assignment and hand the rest to an exact solver. For
knapsack specifically, deep RL constructive policies reach gaps of about
0.3% on standard MKP benchmarks at a fraction of CPLEX's time (Yilmaz and
Büyüktahtakın 2024): a fast heuristic with no proof, on instances drawn
from the training distribution.

**Why a GNN does not fit here.** A GNN earns its place when the instance
structure varies (arbitrary variable-constraint graphs). This problem's
structure is fixed: eight slots, pools, five SP lanes, set membership. The
per-item information that matters (solo ceiling, `UB(i)`, requirements and
provisions against the budget, set and set progress, dominance depth,
level) is a short feature vector, and a gradient-boosted or logistic model
on it (R7) is the right size for hundreds to thousands of labelled runs.
The data constraint is softer than it looks, since the exact engine can
label generated instances at will, but the *benefit* ceiling is the same
as R7's, ordering and warm selection, which R29 and the ordering portfolio
also address with no model at all. Revisit only if R7 plateaus and an
instance generator yields more than 10^4 labelled instances, and then as a
promise score for R30, never as a solver.

**Why deep RL as a solver does not fit.** The user wants the optimum with a
proof. A policy that constructs builds returns a heuristic answer, usually
within a percent, no faster than the warm start plus LNS this engine
already has (sub-second to seconds, with real builds), and it learns the
current data version's items, so every patch retrains it. R6 already
covers the "fast good answer" niche without training.

**Where RL fits, in its small form.** The engine already has one bandit:
`AdaptiveBound` measures pruned leaves per evaluation and switches bound
layers off. The anytime branch's operator scheduler is a measured problem
(113 of 132 runs stopped at the 1,000-repair cap with ~99% of calls spent
on perturbations), which is the textbook case for adaptive operator
selection in ALNS (Ropke and Pisinger 2006): weights or UCB over operators
by recent improvement. A bandit, not a deep network. The same applies to
choosing `WARM_K`, the cluster size and the portfolio shares at run time.

**The one place a large model earns its keep.** R18's front end. Turning
"sustained spell build for Nameless Anomaly, survive the big hit, keep my
walk speed" into restrictions, weights and a window is a language task, and
an LLM that drafts a preset the user then edits is the right tool for it.
The engine underneath stays exact.

### R32. Search modes: "definite optimal" versus "near optimal", one engine, one harness

**The toggle.** One `search_mode` setting, carried in the job and the URL,
shown as a selector with a one-line label saying what the result means.
Two halves of it already exist on unmerged branches: the anytime branch's
"Exhaustive / Quick search" selector (`solver-search-mode`, 5/15/30 s
budgets) and the benchmarks branch's pruning policies (certified, balanced,
fast-then-verify). Unify them into one enum rather than add a third switch:

| Mode | What runs | What the result means | Report line |
|---|---|---|---|
| `exact` | exhaustive engine, admissible bounds only, dominance off or equal-only with expansion (R4), a certified SP allocator (R12) | the optimum, proved; under the greedy allocator only "the best equipment under greedy SP allocation" | "optimal (proved)", or "optimal under greedy SP (proved)" until R12 lands |
| `exact_eps` (R21) | same, prune at `(1 + eps) * best` | within `eps` of the optimum, proved | "within 1% (proved)" |
| `window` (R20) | same, cutoff `(1 - x) * best`, archive, QoL re-rank | every build within `x%`, proved | "complete within 5% (proved)" |
| `quick` | LNS with exact k-slot repair (anytime branch), time budget | a real build, no optimality claim | "best found in 15 s (no proof)" |
| `fast_verify` | `quick` or balanced pruning, then `exact` seeded with the result (R13) | the optimum, proved, found sooner | "optimal (proved), found at 0.4 s" |
| `pseudo_gap` | exact engine on pools restricted to `UB(i) >= incumbent - g`, `g` grown iteratively | optimal within the restricted pools; proved only once `g` reaches the bound | "optimal within gap g (no proof)" |

Every mode uses the same leaf evaluator, so a score means the same thing
in all of them, and every near-optimal mode's output is a real build that
`exact` can take as a seed. One rule: `complete: true` and the word
"optimal" appear only when the exact engine finished or the bound closed;
everything else prints its budget and "no proof". The anytime branch
already enforces this (`complete` is always false for its ALNS).

**Near-optimal methods worth implementing, in order, with the engine piece
each reuses.**

1. **LNS with exact repair (R6; built, on the anytime branch).** First
   because it exists. Merge it, then add adaptive operator selection (the
   bandit from R31, for the measured 1,000-repair stall) and
   *bound-guided neighbourhoods*: fix the slots where the incumbent agrees
   with the tangent bound's argmax (R2) and re-solve the rest exactly. That
   is RINS (Danna, Rothberg and Le Pape 2005) with the bound standing in
   for the LP; local branching (Fischetti and Lodi 2003) is the k-slot
   neighbourhood R6 already has.
2. **Pseudo-gap enumeration (Gao, Lu, Yao and Li 2017).** Restrict every
   pool to the items whose `UB(i)` is within a gap `g` of the incumbent,
   solve exactly on the restricted pools, raise `g`, repeat. It is R3's
   fixing used as a dial instead of a proof, and R30's bucket order falls
   out of it (buckets are gap bands). The exact engine is the inner solver,
   so it needs no new search code, only pool restriction by a score the
   warm start already computes; when `g` reaches the cutoff it *is* the
   exact run, so the mode is anytime toward a proof.
3. **Beam search over prefixes by ceiling.** R22's prefix queue with a width
   cap `B` per depth: keep the `B` best prefixes by ceiling, expand, repeat
   to the leaves, polish with method 1. The textbook construction heuristic
   for knapsack-shaped problems; the only new code is the width cap on a
   queue R22 builds anyway. Deterministic and cheap to reason about, which
   makes it the right baseline arm.
4. **Path relinking between elite builds.** Walk from one archive build to
   another one slot at a time, evaluating each intermediate exactly. The
   fixed set search of Jovanovic and Voss 2024 and the anytime branch's
   crossover are relatives; nearly free once the archive (R13/R20) exists.
5. **GRASP restarts.** Greedy randomised construction (sample each slot
   from the top-k by priority, biased by score), then local search.
   Trivially parallel, a known-good diversification for method 1's
   restarts; a component, not a mode.
6. **NRPA (nested rollout policy adaptation; Rosin 2011, beam variant
   Cazenave and Teytaud 2012).** Learns a per-run policy over (slot, item)
   weights from the best rollouts at each nesting level. Fits an
   eight-decision sequence and needs only the leaf evaluator, but its
   rollouts are blind to bounds and SP feasibility, so expect many
   infeasible leaves on requirement-heavy families. One experiment arm;
   a product mode only if it beats methods 1 to 3 on the matrix.
7. **Not as modes:** GA and SA (R6's rationale); tabu or reactive local
   search over single-item moves (Hifi, Michrafy and Sbihi 2004 for MMKP;
   a one-item move cannot cross coupled requirement barriers, which the
   anytime branch documented as its reason for multi-slot repairs); ant
   colony.

**Implementation.** In the engine, a `SearchMode` on the job: `exact`,
`exact_eps` and `window` are cutoff rules in `Search` (one line each once
R20 and R21 exist); `quick` is `enumerate/anytime.rs`; `pseudo_gap` is a
driver loop around `solve_json_full` with restricted pools; beam is a width
cap on R22's queue. In the UI, one selector, one budget field where a mode
takes one, the report line above, and a **"Prove it"** button that runs
`exact` seeded from the current result (R13): that is how a user upgrades a
quick answer into a certified one without re-entering anything.

**Test plan.** The anytime branch's `QUALITY_BENCHMARKING.md` method is the
right one and becomes the shared harness for every mode: a cohort fixed
before the runs, frozen best-known targets per fixture, several seeds,
T95/T99/T99.9 attainment, endpoint score, primal integral (R8). Add two
things: (a) **regret against the proved optimum** on every fixture where
`exact` completes (family-small and the 34 exhaustive controls), so
heuristic modes are measured against the truth and not a frozen reference;
(b) a **mode matrix** in CI, each mode on family-small at a 5 s budget,
reporting attainment and regret, with `exact` held bit-identical to the
oracle top-15. A near-optimal mode is promoted to the UI only when it beats
`quick` on the matrix at the same budget on at least one family without
losing on the rest, and that is also the rule for retiring one.

### Smaller notes

- **The JS engine is the oracle now.** The Rust engine runs by default in
  the browser and the support matrix's "not supported" section is nearly
  empty. The tracker's JS hot-path items (the 18% Map round trip in
  `_finalize_leaf_statmap`) no longer pay; put that effort into Rust
  coverage of the last fallbacks and into the JS suite's role as the
  differential oracle.
- **Elemental EHP for named content.** `eDef`/`tDef`/… and `*DefPct` are in
  the data, and raid bosses and guild-war towers have element mixes; an EHP
  variant weighted by a content damage profile is a restriction or
  objective users ask for. It fits R14/R18 as a preset, not a new engine
  feature.
- **Set weapons are in current data.** 44 weapons carry a `sets` entry in
  2.2.3.0 (Boundless, Corrupted, Cindercurse, Empty, Bony, ...); the earlier
  "none" count here read the wrong key. See C1.
- **Cost-aware adaptive bounds (review).** `AdaptiveBound` measures pruned
  leaves per evaluation; the right criterion is wall time saved per
  evaluation (leaf time of what was pruned, minus the bound's cost). The
  anytime branch's wider-bound experiment pruned 82% more leaves and covered
  41% less space per second, which the leaf-count criterion would have
  called a win.

## 7. Suggested order

0. **Section 0, C1 to C7**, each as its own PR with its own test, plus the
   independent oracle; then re-profile the Rust/WASM engine. Everything
   below is ranked against measurements taken after these land, not against
   the historical JS numbers.
1. **R1** reachable-SP ceilings: smallest change, provable, likely the largest
   single bound tightening. Alongside it, **R23** incremental leaf fill
   (with its arithmetic-order caveat) and **R25** build flags: exact, cheap,
   with measured headroom.
2. **R9** depth n-1 exact SP bound, then **R10** conflict pairs: cheap,
   exact, and they attack the SP cost on restricted workloads (re-measure it
   on the Rust engine first; 51.7% was a JS number).
3. **R12** beyond the audit (C6 already shows the greedy loses on a
   synthetic case): the shipped-data loss rate decides how much of step 2
   and the exact allocators is needed.
4. **R5** gap reporting, **R21** epsilon tolerance and **R8** anytime
   metrics: make every later change visible and judgeable, and turn
   "cannot finish" into "optimal within 1%" immediately.
5. **R13** result archive, **R20** windowed archive with QoL ranking, and
   **R14** soft/lexicographic objectives: the iterative "describe the
   playstyle" loop. R20 is the user-facing payoff and needs only the
   archive and a cutoff rule change, so it can land early.
6. **R2** tangent bound, then **R3** fixing on top of it, then **R28**
   suffix Pareto fronts, which replace the super-item for the last slots;
   **R30** is the kernel-search schedule that strings them together once
   they exist.
7. **R11** mid-tree mana bound (for defensive and sustain objectives), and
   **R24** resumable chunked solve (browser cutoff sharing, checkpoints)
   followed by **R22** best-bound prefix scheduling once R2/R3 give
   prefixes a bound worth ordering by.
8. **R4** set-aware dominance, **R15** roll-robust evaluation.
9. **R16** weapon as outer group, **R17** diverse top-N, **R18** presets,
   **R26** never-used item detection (its exact tier is R3), **R27**
   preparation-phase caching.
10. **R32** the search-mode toggle, housing **R6** (merge the anytime branch
    first), then the pseudo-gap and beam arms, all measured on the mode
    matrix against the proved optimum.
11. **R29** priority-gap levels and ordering portfolios, then **R7** learned
    ordering once there are enough completed runs to train on; **R31**
    (GNN, deep RL) only under the conditions it states, with bandit-style
    operator selection for R6 the one RL item worth doing now.
12. MILP/CP-SAT as an oracle for linear targets, opportunistically.

## References

- D'Ambrosio, Laureana, Raiconi, Vitale. *The Knapsack Problem with forfeit
  sets.* Computers & Operations Research, 2023. doi:10.1016/j.cor.2022.106093
- Jovanovic, Voss. *Matheuristic fixed set search applied to the
  multidimensional knapsack problem and the knapsack problem with forfeit
  sets.* OR Spectrum 46(4):1329-1365, 2024. doi:10.1007/s00291-024-00746-2
- Mansini, Zanotti. *A Core-Based Exact Algorithm for the Multidimensional
  Multiple Choice Knapsack Problem.* INFORMS Journal on Computing
  32(4):1061-1079, 2020. doi:10.1287/ijoc.2019.0909
- Kress, Kiel, Metternich. *Utility Sorted Branch and Bound Algorithm for the
  Multiple-Choice Multidimensional Knapsack Problem.* TU Darmstadt, 2021.
- da Silva, de Queiroz, Schouery. *Solving Hard Instances from Knapsack and
  Bounded Knapsack Problems: A new state-of-the-art solver (RECORD).*
  arXiv:2604.05232, 2026.
- Shao. *Simultaneous Group-Envelope Bounds for Gamma-Robust Multiple-Choice
  Knapsack Problems.* arXiv:2608.08861, 2026.
- Rezoug, Bader-El-Den, Boughaci. *Application of Supervised Machine Learning
  Methods on the Multidimensional Knapsack Problem.* Neural Processing
  Letters 54:871-890, 2022.
- Khelifa, Idder, Mallem. *A Machine Learning-Guided Metaheuristic Framework
  for the Multidimensional Knapsack Problem.* Univ. Kasdi Merbah Ouargla,
  2025 (thesis).
- Nair et al. *Solving Mixed Integer Programs Using Neural Networks* (Neural
  Diving). arXiv:2012.13349, 2020.
- Han et al. *A GNN-Guided Predict-and-Search Framework for Mixed-Integer
  Linear Programming.* ICLR 2023.
- Berthold. *Measuring the impact of primal heuristics.* Operations Research
  Letters 41(6), 2013 (primal integral).
- *An empirical study of population-based metaheuristics for the
  multiple-choice multidimensional knapsack problem.* Inderscience.
- Chen, Hao. *A "reduce and solve" approach for the multiple-choice
  multidimensional knapsack problem.* European Journal of Operational
  Research 239(2):313-322, 2014.
- Walker, Dyer. *Dominance in multi-dimensional multiple-choice knapsack
  problems.* Asia-Pacific Journal of Operational Research, 1998.
- Harvey, Ginsberg. *Limited Discrepancy Search.* IJCAI 1995.
- Bazgan, Hugot, Vanderpooten. *Solving efficiently the 0-1 multi-objective
  knapsack problem.* Computers & Operations Research 36(1), 2009.
- Ehrgott, Gandibleux. *Bound sets for biobjective combinatorial
  optimization problems.* Computers & Operations Research 34(9), 2007.
- Cerqueus, Przybylski, Gandibleux. *Surrogate upper bound sets for
  bi-objective bi-dimensional binary knapsack problems.* European Journal
  of Operational Research 244(2), 2015.
- Horowitz, Sahni. *Computing partitions with applications to the knapsack
  problem.* Journal of the ACM 21(2), 1974 (meet in the middle).
- Angelelli, Mansini, Speranza. *Kernel Search: a general heuristic for
  the multi-dimensional knapsack problem.* Computers & Operations Research
  37(11), 2010.
- Lamanna, Mansini, Zanotti. *A two-phase kernel search variant for the
  multidimensional multiple-choice knapsack problem.* European Journal of
  Operational Research 297(1):53-65, 2022.
- Gasse, Chételat, Ferroni, Charlin, Lodi. *Exact Combinatorial
  Optimization with Graph Convolutional Neural Networks.* NeurIPS 2019.
- Yilmaz, Büyüktahtakın. *A k-means supported reinforcement learning
  framework to multi-dimensional knapsack.* Journal of Global Optimization
  89(3), 2024.
- Ropke, Pisinger. *An adaptive large neighborhood search heuristic for the
  pickup and delivery problem with time windows.* Transportation Science
  40(4), 2006 (adaptive operator selection).
- Gao, Lu, Yao, Li. *An iterative pseudo-gap enumeration approach for the
  Multidimensional Multiple-choice Knapsack Problem.* European Journal of
  Operational Research 260(1):1-11, 2017.
- Danna, Rothberg, Le Pape. *Exploring relaxation induced neighborhoods to
  improve MIP solutions.* Mathematical Programming 102, 2005 (RINS).
- Fischetti, Lodi. *Local branching.* Mathematical Programming 98, 2003.
- Hifi, Michrafy, Sbihi. *Heuristic algorithms for the multiple-choice
  multidimensional knapsack problem.* Journal of the Operational Research
  Society 55(12), 2004 (reactive local search).
- Mansi, Alves, Carvalho, Hanafi. *A hybrid heuristic for the multiple
  choice multidimensional knapsack problem.* Engineering Optimization,
  2013.
- Rosin. *Nested Rollout Policy Adaptation for Monte Carlo Tree Search.*
  IJCAI 2011. Cazenave, Teytaud. *Beam Nested Rollout Policy Adaptation.*
  2012.
