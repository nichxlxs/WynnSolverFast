# Faster access to good builds: implementation and expanded benchmarks

For the newer **minute-scale and much larger workloads**, start with [LONG_QUERY_OPTIMIZATION_RESULTS.md](LONG_QUERY_OPTIMIZATION_RESULTS.md): 66 additional native runs, fresh-seed checks, browser/WASM validation, and negative results for wider bounds and elite recombination. This document preserves the earlier 132-query research and its original measurements.

The largest demonstrated improvement is in **time to a good scored build**, using the existing Rust evaluator inside a large-neighborhood search. There are repeatable gains above 10× on selected workloads. There is **no evidence of a universal 10× reduction in exhaustive solve time**.

The practical recommendation is to preserve the existing ranked warm search, retain and publish its actual builds, and offer a bounded neighborhood-search mode for improving them. Keep exhaustive enumeration available for users who want to finish the configured search. The measurements below concern the original native CLI research. The September 8 continuation adds a browser **Quick search** mode; its API, deadline behavior and scope are documented in [BROWSER_ANYTIME.md](rust/sp_kernel/BROWSER_ANYTIME.md). Native timing results below are not browser timing claims.

## What was implemented

* Retained warm-start witnesses, including equipment, skill-point allocation and tome choices; deduplicated archives before publishing cutoffs. Browser partitions retain only witnesses belonging to their partition.
* Native `RESULT_COUNT=1` experiments and immediately flushed incumbent traces. Wall deadlines include input/scoring preparation and warm search. Actual-leaf budgets stop at their requested count.
* A native large-neighborhood search with ranked and ordered seeds, two/three-slot repairs, a diverse archive, set-aware coordinated perturbations, crossover and restarts. Every candidate uses the existing evaluator and final restrictions.
* Collision-safe wide-pool bound keys, with separate subtree/cluster key namespaces. This experiment remains opt-in because it was slower.
* Optional cyclic recovery from stagnation and a separate warm-search tuple budget, motivated by observed unused time and Gaia seed failures.
* Recovery of 105 class/archetype snapshots from another branch and a reproducible 132-query quality suite, with shared input hashes, frozen reference targets, fresh-seed confirmation and independent enumeration checks.
* A healing-schema correctness repair discovered by the wider suite: Rust now recognizes current `power` fields as well as the legacy healing alias, including zero-value precedence and dynamic/compiled spell classification.

## Same-target timing: where a 10× gain survived repetition

These are three **screen-selected diagnostic cases**, run for up to 15 seconds with seeds 1001, 2002 and 3003. Targets were frozen from the earlier screen before these repeats. The table uses one consistent heuristic configuration: warm-six, 1,000 repairs, the original scheduler.

| Query | Fixed score target | Baseline median time | Neighborhood median time | Ratio | Target hits |
|---|---:|---:|---:|---:|---:|
| Battle Monk, four slots removed | 24,970.464 | 8.244 s | 0.568 s | **14.52×** | 3/3 for both |
| EHP | 55,645.653 | 7.263 s | 0.365 s | **19.89×** | 3/3 for both |
| Original small spellsteal family | 90,786.831 | 3.187 s | 0.464 s | **6.87×** | 3/3 for both |

Battle Monk's paired ratios were 14.71×, 17.60× and 10.88×. EHP's were 23.22×, 19.70× and 6.94×: its median exceeds 10×, but every seed does not. Warm-three also worked well on Battle Monk (15.43× median), but missed the spellsteal target in two of three repeats. All misses remain in the evidence.

These targets are 99% of a **fixed screening best-known score**. They are not certified within 1% of the true optimum, and subsequent runs sometimes improved the screening reference. For example, the later EHP heuristic found scores up to 57,646.937. The table answers how quickly each method reached the same useful milestone; it does not certify proximity to that later score or to game-global optimality.

## Wider coverage and confirmation

The suite contains 132 queries: 105 recovered snapshots (15 archetypes across all five classes, each fully supplied and with one through six slots removed), 18 original class-family cases, and nine legacy Gaia/spell/EHP/healing/XP cases. This is **117 searches plus 15 fully supplied controls**. Controls are excluded from search-capacity counts.

The recovered snapshots came from `codex/current-family-benchmarks@58c0e3dc5f5ebc2419a636b54601e0247d4e26c6`. Two other inspected branches contributed no additional missing query definitions. See [the benchmark guide](rust/sp_kernel/QUALITY_BENCHMARKING.md) for the branch inventory, query contracts and exact selectors.

The initial screen ran all 132 queries, six arms and one seed with a two-second cap: **792 runs**. Against that screen's best-known targets, baseline enumeration attained T99 on 88/117 searches and neighborhood search on 106/117. This is an in-sample exploratory result, not an independent quality guarantee.

The primary confirmation fixed **22 broad queries before running new seeds**: all 15 remove-six archetypes, all six original large families, and Gaia all-free. Four arms, seeds 101/202/303 and five seconds per run produced **264 runs**. Frozen screening targets were not updated using the confirmation results.

| Configuration | Runs with a valid build | Runs reaching T99 | Queries reaching T99 on every seed |
|---|---:|---:|---:|
| Baseline enumeration, top 15, warm-six | 66/66 | 36/66 | 12/22 |
| Enumeration, top one, warm-three | 66/66 | 18/66 | 6/22 |
| Neighborhood search, warm-three | 61/66 | 53/66 | 14/22 |
| Neighborhood search, warm-six | 63/66 | **55/66** | **16/22** |

Warm-six endpoint score medians improved on 12 queries, tied on eight and lost on one among the 21 queries with results on every repeat. Gaia had no heuristic result and is explicitly retained as a failure outside that paired-score denominator. The large hybrid family lost 2.09% at the endpoint median. No primary-cohort query demonstrated a 10× baseline T99 improvement; the larger timing gains are specific to the separately selected diagnostics.

Examples of better five-second endpoint scores were **Arcanist +37.21%, Light Bender +51.98%, Riftwalker +8.61%, and large spellsteal +17.17%**. These percentages are score improvements, not speed multipliers. Their importance is that local repair can improve builds well before broad enumeration finishes.

## Budget and stagnation follow-up

The initial heuristic was not spending the full requested time: 113/132 confirmation runs hit the 1,000-repair limit, typically around 1.68 seconds. After eight unsuccessful repairs its scheduler restricted itself to perturbations/restarts until a global improvement, excluding the local operators that had produced most improvements. Gaia's warm-six seed also inherited a 100,000-tuple repair cap, below its possible `6^8 = 1,679,616` seed combinations. The existing enumerator's warm search has different bounds and no such credited-tuple cap.

The opt-in follow-up profile reopens local repairs after diversification, permits 10,000 repairs, and gives ranked seeding a separate two-million-tuple cap. It preserves the same final scorer, candidate universe and five-second wall budget. The fixed 22-query cohort and new seeds 707/808/909 are used again. This is a post-hoc combined configuration experiment; it does not isolate the contribution of each parameter or provide paired timing ratios against the earlier seeds.

The follow-up produced **66/66 valid builds and 66/66 T99 hits: all 22 queries reached the frozen target on all three fresh seeds**. T99.9 attainment was 65/66. Every run used the full five-second deadline; there were no process failures. The earlier primary baseline reached T99 in 36/66 runs across 12/22 queries on every seed. This is a useful quality comparison across recorded cohorts, not a paired timing estimate or a guarantee for unseen builds.

Gaia now returned a feasible build in 0.118–0.124 seconds and reached its fixed target in 0.154–0.159 seconds on every seed. Its best score remained 324,111.661, matching the baseline; the baseline reached that milestone sooner. This fixes the prototype's missing-result problem, not Gaia's exhaustive completion time.

Endpoint medians beat the earlier baseline on 15 queries and tied on seven, with no losses; no individual follow-up endpoint fell below the earlier baseline's maximum for that query. Against the earlier warm-six heuristic, there were ten median wins, nine ties and two losses, with Gaia previously unmatched. Light Bender lost 0.52% and Acolyte 5.47% against that earlier heuristic, while both still beat the baseline and attained the fixed T99 target. This profile is the **preferred tested experimental configuration**, not a claim that every score improves.

```sh
rust/sp_kernel/target/release/anytime_kernel ENUM.txt SCORE.json \
  --seconds 5 --seed 707 --top-k 1 --warm-k 6 \
  --warm-budget 2000000 --repair-budget 100000 \
  --max-repairs 10000 --cycle-stagnation 1 --trace /tmp/anytime.jsonl
```

The selected-case 14.52×/19.89× diagnostic measurements above belong to the original warm-six configuration. They are not reassigned to this changed configuration. CLI defaults preserve the earlier ablation for reproducibility; use all explicit flags above for the preferred profile.

## Exact-search experiments and default decisions

| Change | Evidence | Decision |
|---|---|---|
| Retain warm witnesses | Approximately neutral median timing; fixes cutoff-only handoff losing available builds under a budget | Enabled |
| Maintain only the best result | Approximately neutral T99 with the same warm depth | Native option; top 15 remains default |
| Reduce warm depth from six to three | Weaker target attainment on broad queries | Do not make this the general default |
| Wide-pool objective bounds | 0.773× median completion speed on 33 completed same-top-15 comparisons; 0.688× T99 on 88 common-attainment searches | Default off; `WIDE_BOUND_KEYS=1` for experiments |
| Neighborhood search | Better broad-query quality and selected 10×+ same-target timings, with misses and regressions | Experimental native mode; never reports global completion |

The wide-key result means about 29% longer median completion time, not a 23% time reduction. A per-case win would still be possible; none justifies enabling it globally here. Comparing top-one completion against top-15 completion would change the requested proof task, so the reporting harness forbids that completion-speed comparison.

## Independent checks and remaining scope

Independent enumeration disabled warm search, SP feasibility bounds, objective subtree/cluster/tail bounds and the score-ceiling gate on 60 small/control queries. **58 completed; two healing queries timed out after 30 seconds.** No screened candidate exceeded those 58 independent maxima, and every completed enumeration comparison matched. Among the 43 completed non-control references, baseline T99 attainment was 43/43 and heuristic attainment 42/43. Thus the heuristic's main benefit is on broad searches, not a replacement for already-cheap small searches.

Validation records include:

* 132/132 sampled JS/Rust scoring cases after the healing repair, compared with 117 passed / 15 failed before it. One sampled witness per query is a smoke check, not exhaustive evaluator validation.
* 45 exhaustive native/WASM top-15 comparisons: 15 controls plus all remove-one and remove-two profiles.
* 21 analytic scored wide-key checks, including pool sizes 127, 128, 129, 257 and 2050.
* 29 Rust unit tests and 14 benchmark-integrity tests passed on the final native source.
* JavaScript: 545 passed, three browser guard failures and two warnings on both the baseline and modified checkout. Playwright/Chromium were unavailable here; browser end-to-end validation is not claimed.

The baseline is pinned to `master@027b0e8490432e6c87fe6c3024aed57fafcfb724`, but the timed `safe_baseline15` arm is an **instrumented baseline behavior on a shared repaired evaluator**. It includes the healing correction and uses the same raw pools with known-unproved early HP/EHP/raw-stat prechecks disabled. Final scored restrictions remain active. This is not an untouched-master speed claim or a production browser benchmark.

Warm discoveries are traced in both retained and legacy handoff arms. Times measure when a valid scored build was found, not when the old UI happened to publish it. Native process wall time includes process launch, parsing, scoring-plan preparation and warm search; fixture generation is measured separately (132 exports completed in about 110 seconds total). Runs are sequential on one native thread; the observer polls every five milliseconds. Very short timings are near that resolution.

All optimality statements are limited to the configured items, fixed weapon/tree and current evaluator. Extra-SP allocation remains heuristic, and these experiments do not establish game accuracy or full optimization over every skill-point allocation. The benchmark exporter relaxes known unsafe prechecks; this change does not repair all production admissibility issues.

## Where AMPL/CPLEX fits

AMPL could express one binary choice per item/slot, set-count activations, additive requirements and a surrogate objective. CPLEX could then solve that mathematical model or a restricted neighborhood. Its documented domain includes linear and quadratic objectives with integer variables and supported quadratic constraints; it does not automatically optimize this Rust combat simulator as an opaque objective. [AMPL's CPLEX capabilities](https://ampl.com/products/solvers/linear-solvers/cplex/), [IBM CPLEX overview](https://www.ibm.com/products/ilog-cplex-optimization-studio/cplex-optimizer).

For this codebase, my assessment is that a full reformulation would require substantial work on nonlinear skill scaling, activation order, set/ability effects, mana/cast breakpoints and simulation semantics. A CPLEX gap on an approximate surrogate would certify that surrogate, not the real simulator. The more direct first experiment is the implemented neighborhood approach: restrict a few slots and reuse the fast scorer already available. An MILP candidate generator or repair subproblem remains a future option if these small repair searches become the measured bottleneck. No CPLEX speed claim is made and no CPLEX implementation is included.

For exact completion, stronger admissible bounds on finalized constrained stats remain a plausible larger structural improvement. For good results under a short deadline, a portfolio combining the existing warm search and bounded neighborhood repair is the strongest direction supported by this evidence. Genetic algorithms or simulated annealing are alternatives, but a generic one-item mutation operator would need additional handling for coupled requirements and set activations; neither was benchmarked here.

## Reproduce and inspect

* [Quality benchmark guide and commands](rust/sp_kernel/QUALITY_BENCHMARKING.md)
* [Native neighborhood CLI and flags](rust/sp_kernel/ANYTIME_SEARCH.md)
* [Warm-result and result-count semantics](rust/sp_kernel/ANYTIME_CORE.md)
* [Evidence directory](rust/sp_kernel/evidence/anytime_2026_09_07/)

There are **1,209 recorded campaign runs** across the screen, primary confirmation, selected diagnostics, independent enumeration and configuration follow-up. The evidence package preserves positive and negative results, initial failures, fixture hashes, source/binary identities, raw traces, process outcomes and frozen targets. Compressed raw campaigns are provided alongside readable reports; their manifest records integrity hashes.
