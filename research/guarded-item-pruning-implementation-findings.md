# Guarded item-pruning implementation and findings

Date: 2026-08-16  
Game data: Wynncraft 2.2.3.0  
Benchmark cohort: 15 current build-family profiles across all five classes

## Conclusion

Item pruning is worth keeping, but the previous sensitivity-only rule is not a
safe default. Certified contract dominance removes fewer combinations, 8.50%
in aggregate across the 1/2/3 missing-slot matrix, but matched the unpruned
result in all 45 equal-cap comparisons and all 34 exhaustive unpruned controls.

Balanced guarded pruning is the useful speed mode. It removed 56.02% of the
aggregate Cartesian space, lowered median time to the calibrated score from
245 ms to 189 ms, and completed two more remove-3 searches inside five seconds.
It preserved every optimum for which an exhaustive unpruned control was
available. It is still a heuristic, so the UI also provides a fast-then-verify
mode that runs the full pool after the balanced stage. Based on these measured
tradeoffs, Balanced is the product default while Certified and No item pruning
remain available for exact controls.

The legacy and aggressive policies remain useful only as benchmark controls.
They have confirmed optimum losses:

- Mage cancelstack: current/aggressive exhausted at 314,987 versus the full
  maximum of 350,291, a 10.08% loss.
- Archer slow heavy melee: aggressive exhausted at 36,513 versus the guarded
  maximum of 37,546, a 2.75% loss.

## What changed

The new `reduce_candidate_pools(pools, context)` module owns policy selection,
contract guards, active/deferred pools, and proof diagnostics.

Certified pruning now requires a candidate replacement to be no worse on every
active monotone dimension, equal on every other modelled direct dimension, no
harder to equip, and no lower in any skill-point provision. Additional guards:

- attack tier is equal for melee contracts because tier changes cadence,
  per-hit damage, mana steal, and life steal nonlinearly;
- static armour HP and powder-slot capacity are included;
- set items are preserved because a standalone item cannot prove set-bonus
  replacement;
- Major-ID items are preserved because their behaviour is not represented by
  ordinary identification stats;
- every removed item remains in a deferred pool;
- every removal has a certificate naming its surviving dominator and the
  higher/lower/equality dimensions used.

Balanced pruning uses the existing 0.5% sensitivity scale but treats weak
nonzero responses as uncertain equality dimensions rather than irrelevant. It
also applies the discrete attack-tier guard.

The solver UI now exposes:

1. Balanced guarded pruning, default.
2. Fast result, then verify full pool.
3. Certified pruning.
4. No item pruning.
5. Legacy and aggressive modes, labelled known unsafe.

Fast verification searches the balanced pool first, seeds that result into the
second stage's score bounds, and then searches the complete eligible pool. The
full stage repeats the active portion, so it is intended to improve time to a
good incumbent and restore eventual coverage, not to guarantee less total
enumeration than starting unpruned.

## Final 1/2/3 missing-slot matrix

The final campaign used 270 searches: 45 family/removal scenarios by six
strategies, five-second caps, four workers per search, and two concurrent jobs.

| Mode | Aggregate combinations removed | Median reduction | Exhaustive runs | Median elapsed | Median time to calibrated score | Equal-cap score vs unpruned |
|---|---:|---:|---:|---:|---:|---|
| Off | 0.00% | 0.00% | 34/45 | 787 ms | 245 ms | 45 equal |
| Certified | 8.50% | 4.61% | 34/45 | 712 ms | 215 ms | 45 equal |
| Balanced | 56.02% | 36.81% | 36/45 | 618 ms | 189 ms | 1 better, 43 equal, 1 lower |
| Conservative legacy | 59.51% | 45.60% | 36/45 | 533 ms | 216 ms | 44 equal, 1 lower |
| Current legacy | 62.41% | 45.60% | 36/45 | 474 ms | 222 ms | 44 equal, 1 lower |
| Aggressive | 64.32% | 45.60% | 36/45 | 562 ms | 165 ms | 43 equal, 2 lower |

There were 34 exhaustive unpruned controls. All strategies matched the exact
maximum in those controls. The remaining comparisons are censored and are not
global-optimum claims.

The single balanced equal-cap deficit was Mage rawstack meteor remove-3 at
98.44% of the five-second unpruned observation. A separate exhaustive balanced
run found the same 35,641 score after 15.2 seconds, so the five-second deficit
was search order, not elimination of that observed build. Assassin spellsteal
had the same pattern in an earlier cohort and recovered the unpruned score after
5.2 seconds.

Checked builds per second did not improve under balanced pruning. Median rate
was about 25.7k/s versus 29.7k/s unpruned. The benefit is fewer leaves and a
better early incumbent, not cheaper evaluation of each leaf. Search-space
reduction, exhaustive completion, and score preservation are therefore the
primary metrics.

## Resolved counterexamples

### Mage cancelstack, three missing items

All modes were allowed 30 seconds and completed their search spaces.

| Mode | Combinations | Maximum score | Best found at | Exhaustive time | Full optimum retained |
|---|---:|---:|---:|---:|---|
| Off | 5,383,532 | 350,290.56 | 6.25 s | 19.12 s | yes |
| Certified | 5,103,162 | 350,290.56 | 7.28 s | 18.93 s | yes |
| Balanced | 2,027,400 | 350,290.56 | 3.31 s | 9.49 s | yes |
| Current legacy | 1,925,376 | 314,987.09 | 6.06 s | 9.08 s | no |
| Aggressive | 1,925,376 | 314,987.09 | 4.40 s | 6.60 s | no |

The original false proof removed Knucklebones as dominated by an empty
bracelet because attack tier was absent from the dominance dimensions. The
optimal build requires Knucklebones' +3 attack tier. The melee equality guard
prevents that comparison in certified and balanced modes.

### Archer slow heavy melee, three missing items

Balanced removed 54.92% of combinations, exhausted 1,237,720 combinations in
3.82 seconds, and scored 37,546.24. Aggressive removed 78.13%, exhausted
600,576 combinations in 2.55 seconds, and could only score 36,512.76.

The false proof removed Diamond Fiber Bracelet after its weak melee and earth
damage responses fell below the 2% threshold. Balanced keeps those weak active
dimensions as equality guards.

## Verification

- Full JavaScript suite: 1,672 passed, 0 failed, 2 existing combo warnings.
- Dominance/reducer regressions: 55 passed, including attack tier, weak active
  damage, deferred certificates, Major IDs, static HP, unpruned metadata, and
  fast-verification stage ordering.
- Current-meta benchmark contracts: 1,153 passed.
- Real browser, JavaScript: balanced stage 48 combinations, full stage 106,
  final score 17,564.
- Real browser, Rust/WASM: balanced stage 56 combinations, full stage 106,
  final score 17,564, engine reported `rust`, no fallback.
- Native `cargo test` and WASM `cargo check` could not start because this Windows
  machine lacks the MSVC `link.exe` needed for host build scripts. This is an
  environment limitation; the existing WASM bundle executed the staged path in
  the real browser.

## Safety boundary and recommendation

No benchmark suite can prove safety for every future item, Major ID, set bonus,
ability mechanic, or objective that the model does not represent. Certified
mode is a proof only under the solver's current modelled direct dimensions and
explicit guards. For research that claims a theoretical global optimum, use
No item pruning or allow Fast result, then verify full pool to finish.

Recommended product behaviour:

- Keep Balanced as the default toggle state for materially smaller searches
  and faster calibrated-score recovery.
- Use Fast result, then verify full pool when early interactivity and eventual
  full coverage both matter.
- Keep Certified available as the exact guarded control.
- Keep No item pruning available for scientific controls and global-optimum
  claims.
- Do not select Legacy or Aggressive automatically for any build family.

## Reproducible artifacts

- `js/solver/benchmarks/current_meta_guarded_exact123_final_results.json`
- `js/solver/benchmarks/current_meta_guarded_exact123_final_report.md`
- `js/solver/benchmarks/current_meta_guarded_exact123_final_paired_analysis.json`
- `js/solver/benchmarks/current_meta_guarded_cancelstack_final_exact_results.json`
- `js/solver/benchmarks/current_meta_guarded_counterexamples_final_results.json`
- `js/solver/benchmarks/current_meta_guarded_balanced_resolution_results.json`
