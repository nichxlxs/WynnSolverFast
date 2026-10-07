# Research roadmap: faster proofs and better anytime results

Written 2026-10-07 against `027b0e8`, revised the same day after the Codex
review on PR #19 (corrections in R2, R3, R6, R11, R12, R13, R16, R21 and
R22, each marked "Correction"). This is a plan, not a ledger: nothing
here is measured yet. Every "expected" effect below is a hypothesis to be
tested with the existing tools (`benchmark_ab.py`, the oracles,
`SCORE_DENSE_CHECK`, the family suite) before it is believed.

## 1. Where the solver stands

The engine is an exact, best-first branch-and-bound over an MMKP-shaped space:

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

**Where it breaks.** Factors that can go non-positive on the box (negative %
items driving `1 + sum` toward zero), `max(0, .)` clamps, atree var-scaling
terms, min/max damage ranges and crit mixing. Apply the tangent bound only to
terms whose factor lower bound is provably positive on the box, and keep the
current super-item path for the rest. `ceiling_vars_ok` already encodes a
similar applicability test.

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
Re-run whenever the shared cutoff rises meaningfully.

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
same reachable set-SP term `sp_set_bound.js` now tests). Negative
transitions must be treated as zero in `B`'s favour.

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
max provision`. Provisions can only help (the tome plan already relies on
"a requirement-free bonus can never make a build infeasible"), so if that
solve fails, every leaf under the node fails. One kernel call per node
amortizes over the ~70-250 leaves beneath it.

**Why it matters.** On `solver_mage_gaia_6free_lvl50` the exact SP solve was
51.7% of wall and every leaf paid it. This moves the first solve up one
level where its verdict covers a whole pool.

**Measure.** `sp_kernel_reject` before and after; `benchmark_ab.py` on the
family suite (counters must stay comparable: this only changes where a leaf
is rejected, never whether).

### R10. SP conflict pairs (exact, low risk)

**What.** For every pair of items in different pools, solve the exact SP
kernel on `{weapon, locked, A, B}` plus the per-lane max provisions of all
other pools as a free item. If that fails, `A` and `B` can never coexist.
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
how much the greedy falls short, per family. If it never does, document it
and stop. If it does, the leaf scorer is where the optimum is being lost,
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
   evicting the lowest score); if the cap binds, report that the window
   was truncated to the top 2,000 and tell the user to narrow `x`.
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
asked). Prune a subtree when `ceiling <= (1 + eps) * best` instead of
`ceiling <= best`, and stop the run when the global bound (R5) satisfies the
same test. The result is guaranteed within `eps` of the optimum; that is
the `MIPGap` rule every MILP solver ships with, and the guarantee is as
exact as the ceiling is admissible. It composes with every bound in R1-R3,
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
`eps`; *approximate window* prunes at `(1 + eps)(1 - x) * best` and the UI
states that the archive is complete only above that line, with the bottom
`eps` sliver of the window possibly missing. `eps` on its own (no window)
keeps the top-1 guarantee stated above.

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

- the global bound (R5) is the queue head, so the gap is exact and cheap;
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

### `codex/current-family-benchmarks`: a correctness finding on master's default

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

### `agent/anytime-neighborhood-benchmarks`: R6 is already built

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

1. **Set weapons.** `calculate_skillpoints` iterates `equipment` for set
   counts and the weapon is passed separately, so a non-crafted set weapon
   (Bony Bow in the Bony set) never activates its set in either engine. The
   Rust loader counts the weapon's requirements but not its set id. The
   mechanism is confirmed on master, but **no weapon in the 2.2.3.0 data
   carries a `set` field** (Bony Bow included), so today it is latent rather
   than live; take the fix as hygiene, with a test on a synthetic set
   weapon. The anytime branch refuses set weapons for this reason.
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
of tracker item 2) is also disabled on that branch; that one is a known
open decision on master and belongs with R1's bound work rather than a
cherry-pick.

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
restoring journaled old values rather than subtracting (bit-exact, the same
trick `dense_ceiling_cached` uses; a subtract-based running vector is not
ulp-exact). Resolve item names to integer ids at load so the hot path does
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
so they are built once per run (or cached by R27's hash). They give R3 its
pair fixing for free: a pair not on the front, or on it with `UB` strictly
below the cutoff, is gone. Set bonuses enter only through the same
transition deltas the current bound uses.

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
nested cores: `k = 3`, then buckets of the next three per slot, each solved
exactly on kernel plus bucket, skipped when its bound cannot beat the
incumbent, with the kernel grown by whatever enters the top-N. Every
intermediate answer is exact for its core, the incumbent and cutoff only
rise, and the final sweep of the remaining buckets, under the bounds, is
the proof. R22's prefix queue is the natural executor (a bucket is a set of
prefixes). Exhaustiveness is claimed only when every bucket has been solved
or bound-skipped, never from the kernel alone.

**Measure.** Primal integral and proof time on the family suite against
warm-start-then-full-run; the fraction of buckets skipped by bound, which
is the direct measure of how much R1, R2 and R28 bought.

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
- **No set weapons in current data.** Counted in 2.2.3.0: zero weapons with
  a `set` field, so weapon-set handling is hygiene, not a live bug.

## 7. Suggested order

1. **R1** reachable-SP ceilings: smallest change, provable, likely the largest
   single bound tightening. Alongside it, **R23** incremental leaf fill and
   **R25** build flags: exact, cheap, with measured headroom.
2. **R9** depth n-1 exact SP bound, then **R10** conflict pairs: cheap,
   exact, and they attack the 51.7% SP cost on restricted workloads.
3. **R12 step 1** greedy audit: decide whether the leaf score itself is
   losing the optimum before investing further in bounds.
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
10. **R6** LNS incumbent thread.
11. **R29** priority-gap levels and ordering portfolios, then **R7** learned
    ordering once there are enough completed runs to train on.
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
