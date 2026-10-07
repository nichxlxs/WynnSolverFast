# Research roadmap: faster proofs and better anytime results

Written 2026-10-07 against `027b0e8`. This is a plan, not a ledger: nothing
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

**The bound.** Each damage term is a product of positive affine factors of the
stat vector `x`: `T(x) = prod_k (a_k + c_k . x)`. Where every factor is
positive on the box, `log T` is concave, so for any linearization point `p`:

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
- SP enters linearly too, so the joint 200-point budget from R1 becomes a
  fractional knapsack over five lanes inside the same bound.

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
`UB(i) <= cutoff`, delete `i` from the pool for the whole run. Iterate:
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
`UB <= cutoff`. This turns the ring pair into one merged group, the classic
MCKP group-merging move.

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

**What.** A side worker that never prunes, only raises `shared_cutoff`:

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

### R11. Mid-tree mana bound (exact, medium risk)

**What.** The doom precheck (a mana sim at the highest reachable Int) runs
only at the leaf (`scoring.rs`); `enumerate.rs` has no mana bound at all.
Mana feasibility is monotone in every stat the fast sim reads (`mr`, `ms`,
`maxMana`, `int`, `hp`, cost reductions), so a doom sim at the suffix maxima
of those stats is a valid subtree bound, exactly as the restriction suffix
bounds extend the leaf precheck. The bounded-coupling argument from task #28
covers atree var effects. On defensive objectives (B1) the mana sim is the
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

**Step 2, structure.** For `combo_damage` without atree effects that read SP,
only Str (a multiplicative factor through `skillPointsToPercentage`) and Dex
(crit chance, a convex combination of normal and crit damage) touch damage,
and Int touches only mana. The allocation is then a two-lane problem with a
known shape, solvable exactly by a one-dimensional scan over the Str/Dex
split with the mana rescue as a side condition. When atree var effects read
other lanes, fall back to the trials. `ceiling_vars_ok` already classifies
exactly this. This both removes 5 trials per step and makes the leaf score
exact on the common case.

### R13. Result archive across runs (anytime, low risk)

**Why.** The intended workflow is iterative: run, look, tighten a condition,
run again. Today every run starts cold except for the warm subspace.

**What.** Keep an archive of scored builds from previous runs (the top few
thousand with their assembled stat vectors, not just the top 15). When a new
run starts with the same weapon, combo and objective and only the
restrictions changed, filter the archive by the new restrictions and seed
`shared_cutoff` from the best survivor. A score is unchanged by a restriction
change, so the seed is admissible. Also seed the Rust path from the user's
current UI build, which the JS engine does (`_eval_current_build`) and the
Rust bridge does not.

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
the cutoff when the objective is comparable (same combo rows and target):
a build found under weapon A is a real build, so its score is an admissible
cutoff for weapon B. Order weapons by their solo ceiling (R3's `UB(i)` with
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

**Measure.** Pruning cost of the window cutoff versus the top-15 cutoff at
`x` = 2, 5, 10% on the family suite (fixed-work A/B, expect divergence in
`gated`, none in the top-1); archive size at the end of each completing
scenario; and a user check that the default weights reproduce the guide's
ranking order on the six validated family seeds.

### Smaller notes

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

## 5. Suggested order

1. **R1** reachable-SP ceilings: smallest change, provable, likely the largest
   single bound tightening.
2. **R9** depth n-1 exact SP bound, then **R10** conflict pairs: cheap,
   exact, and they attack the 51.7% SP cost on restricted workloads.
3. **R12 step 1** greedy audit: decide whether the leaf score itself is
   losing the optimum before investing further in bounds.
4. **R5** gap reporting and **R8** anytime metrics: make every later change
   visible and judgeable.
5. **R13** result archive, **R20** windowed archive with QoL ranking, and
   **R14** soft/lexicographic objectives: the iterative "describe the
   playstyle" loop. R20 is the user-facing payoff and needs only the
   archive and a cutoff rule change, so it can land early.
6. **R2** tangent bound, then **R3** fixing on top of it.
7. **R11** mid-tree mana bound (for defensive and sustain objectives).
8. **R4** set-aware dominance, **R15** roll-robust evaluation.
9. **R16** weapon as outer group, **R17** diverse top-N, **R18** presets.
10. **R6** LNS incumbent thread.
11. **R7** learned ordering, once there are enough completed runs to train on.
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
