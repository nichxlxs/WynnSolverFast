# Independent review of the quality experiments

The three selected diagnostic queries establish case-specific gains. Battle Monk remove-four reached the same frozen 99% score target 15.43× faster with ALNS warm-three and 14.52× faster with warm-six by median time; all three paired seed comparisons exceeded 10×. EHP warm-six achieved a 19.89× median gain, with a slower individual seed at 6.94×. These results do not establish a general 10× improvement across class builds.

All comparisons use the same corrected scoring implementation and shared raw-pool fixtures with unproved early HP/EHP/stat prechecks disabled. The baseline is not untouched production master. Targets were frozen from the completed screening run before the new seeds ran. They are best-known targets, not certified optima.

## Fixed 22-query confirmation

Five seconds per run; seeds 101, 202, 303; 264 runs. The cohort was chosen by workload definitions: all fifteen archetypes at six removed slots, all six original large families, and Gaia all-free.

| Arm | Valid result | T95 | T99 | T99.9 | Process failures |
|---|---:|---:|---:|---:|---:|
| safe_baseline15 | 66/66 | 45/66 | 36/66 | 33/66 | 0 |
| best1_warm3 | 66/66 | 27/66 | 18/66 | 18/66 | 0 |
| alns1 | 61/66 | 57/66 | 53/66 | 50/66 | 0 |
| alns_warm6 | 63/66 | 61/66 | 55/66 | 52/66 | 0 |

Warm-six ALNS reached the frozen T99 target on all three seeds in 16/22 queries, compared with 12/22 for the baseline and 14/22 for warm-three ALNS. On paired queries with finite results in every repeat, warm-six endpoint medians improved on 12 queries, tied on eight, and lost on one; Gaia had no ALNS result and is kept outside that paired-score denominator. No query in the primary confirmation cohort showed an observed or fully censored lower-bound T99 speedup of at least 10× over the baseline.

Notable warm-six endpoint median gains over the five-second baseline were Mage Arcanist +37.21%, Light Bender +51.98%, Riftwalker +8.61%, and the original large spellsteal case +17.17%. These are score gains, not runtime gains. The original large hybrid case lost 2.09% by endpoint median and hit the frozen T99 target in only one of three runs.

Warm-six mitigated several weak initial seeds: Sharpshooter T99 median was 0.099 s versus 0.124 s for the baseline and 0.983 s for warm-three ALNS; Trickster warm-six hit all three in roughly 0.08 s, while warm-three missed one and took 2.5–4.2 s in the two successful runs. This improvement was not universal: Boltslinger warm-six hit only one of three frozen targets, versus two of three for warm-three.

### All 22 warm-six confirmation queries

Times and scores below retain seed order 101, 202, 303. "miss" means the fixed target was not reached or no result was returned, as appropriate. Full JSON preserves unrounded targets and both ALNS arms.

| Query | Frozen reference | 99% target | Baseline T99 seconds | Warm-six T99 seconds | Warm-six endpoint scores |
|---|---:|---:|---|---|---|
| meta_archer_boltslinger_fast_hybrid_remove_6 | 155663.780 | 154107.142 | miss, miss, miss | 0.743, miss, miss | 155663.780, 152074.587, 152074.587 |
| meta_archer_sharpshooter_heavy_melee_remove_6 | 38609.290 | 38223.197 | 0.119, 0.124, 0.129 | 0.099, 0.099, 0.110 | 38609.290, 38609.290, 38609.290 |
| meta_archer_trapper_slow_melee_remove_6 | 25894.478 | 25635.533 | 0.078, 0.072, 0.067 | 0.074, 0.073, 0.074 | 25894.478, 25894.478, 25894.478 |
| meta_assassin_acrobat_spellspam_remove_6 | 26124.680 | 25863.433 | miss, miss, miss | 0.419, miss, 0.331 | 26124.680, 25656.343, 26124.680 |
| meta_assassin_shadestepper_spellsteal_remove_6 | 54110.606 | 53569.500 | 0.088, 0.105, 0.088 | 0.084, 0.088, 0.083 | 54110.606, 54110.606, 54110.606 |
| meta_assassin_trickster_spell_remove_6 | 79185.811 | 78393.953 | 0.093, 0.098, 0.109 | 0.083, 0.078, 0.083 | 79750.241, 79750.241, 79750.241 |
| meta_mage_arcanist_meteor_remove_6 | 43293.548 | 42860.612 | miss, miss, miss | 1.712, 2.467, 3.225 | 44361.718, 44084.395, 44328.482 |
| meta_mage_light_bender_healing_remove_6 | 15807.210 | 15649.138 | miss, miss, miss | 1.056, 0.332, 1.192 | 21047.760, 22459.500, 22459.500 |
| meta_mage_riftwalker_cancelstack_remove_6 | 361768.684 | 358150.997 | miss, miss, miss | miss, 1.209, miss | 347270.684, 360527.182, 337738.849 |
| meta_shaman_acolyte_healing_remove_6 | 12244.163 | 12121.721 | miss, miss, miss | 3.223, 3.732, 3.823 | 13239.900, 13382.400, 13054.837 |
| meta_shaman_ritualist_tierstack_remove_6 | 267253.981 | 264581.442 | miss, miss, miss | 0.698, miss, 0.800 | 267253.981, 233277.965, 267253.981 |
| meta_shaman_summoner_aura_remove_6 | 8226.539 | 8144.274 | 0.100, 0.126, 0.104 | 0.089, 0.083, 0.095 | 8240.620, 8240.620, 8240.620 |
| meta_warrior_battle_monk_uppercut_remove_6 | 25939.547 | 25680.152 | 0.318, 0.310, 0.314 | 0.846, 0.251, 0.804 | 25939.547, 25939.547, 25939.547 |
| meta_warrior_fallen_uppercut_remove_6 | 38088.405 | 37707.521 | miss, miss, miss | 0.214, 0.389, 2.040 | 37937.814, 38088.405, 37854.133 |
| meta_warrior_paladin_tank_remove_6 | 16445.297 | 16280.844 | miss, miss, miss | 0.314, 0.740, 0.607 | 16556.752, 16517.326, 16517.326 |
| fam_cancelstack_large | 510715.008 | 505607.858 | 0.063, 0.062, 0.064 | 0.059, 0.060, 0.059 | 510715.008, 510715.008, 510715.008 |
| fam_heavy_melee_large | 37822.406 | 37444.182 | 0.073, 0.077, 0.073 | 0.042, 0.042, 0.042 | 37822.406, 37822.406, 37822.406 |
| fam_tierstack_large | 196090.089 | 194129.188 | 0.063, 0.057, 0.062 | 0.048, 0.036, 0.036 | 196090.089, 196090.089, 196090.089 |
| fam_spellsteal_large | 93162.663 | 92231.036 | miss, miss, miss | 0.613, 1.039, 1.174 | 93162.663, 93162.663, 93162.663 |
| fam_spell_sustained_large | 1573331.106 | 1557597.795 | 0.052, 0.052, 0.062 | 0.047, 0.052, 0.047 | 1573331.106, 1573331.106, 1573331.106 |
| fam_hybrid_large | 261937.072 | 259317.701 | 0.244, 0.244, 0.254 | miss, 0.234, miss | 255637.603, 262855.806, 256472.976 |
| gaia_all_free | 324111.661 | 320870.544 | 0.052, 0.058, 0.058 | miss, miss, miss | miss, miss, miss |

### Gaia and the warm-start confound

Gaia all-free remains a material ALNS weakness: the baseline returned a score of 324111.660694 and reached the frozen target in 0.052–0.058 s on every seed; both ALNS warm depths returned no build on all three five-second runs. Warm-six is therefore not a universal replacement for the existing warm-start path.

Equal warm depth does not mean equal warm coverage. ALNS runs its warm domain through the 100000-credited-tuple repair budget and retains a diversity archive; the production warm search has no such credited-tuple cap and builds a dense subtree bound. With eight free slots, six candidates per slot imply up to 6^8 = 1679616 raw tuples. Increasing the outer repair-count limit does not increase this initial warm-domain budget.

Another limitation is premature repair-count exhaustion: 113/132 ALNS confirmation runs stopped at 1000 repairs, at a median of about 1.68 s despite a five-second allowance. Among capped warm-six runs, roughly 98.9% of operator calls were perturbations/restarts, which produced 20 global improvements; pair/triple/crossover calls produced 83. This supports testing the remaining time budget and diagnosing operator scheduling; it does not itself demonstrate that a larger limit improves quality.

## Screen-selected diagnostics

These three queries were selected after screening because every one showed a censored lower-bound T99 signal of at least 10×. They are a separate discovery-follow-up cohort, not part of the prespecified 22-query confirmation. Fifteen seconds per run; fresh seeds 1001, 2002, 3003; 27 runs. No process failed and every run returned a valid scored build. Baseline and warm-six hit T99 on 9/9 runs; warm-three hit 7/9.

| Query / arm | T95 baseline → ALNS median seconds | T95 median ratio | T99 baseline → ALNS median seconds | T99 median ratio | T99 paired seed ratios |
|---|---|---:|---|---:|---|
| meta_warrior_battle_monk_uppercut_remove_4 / alns1 | 2.032 → 0.358 | 5.67 | 8.244 → 0.534 | 15.43 | 15.63, 17.66, 10.46 |
| meta_warrior_battle_monk_uppercut_remove_4 / alns_warm6 | 2.032 → 0.337 | 6.03 | 8.244 → 0.568 | 14.52 | 14.71, 17.60, 10.88 |
| fam_spellsteal_small / alns1 | 3.187 → miss | miss | 3.187 → miss | miss | miss, miss, 8.34 |
| fam_spellsteal_small / alns_warm6 | 3.187 → 0.464 | 6.87 | 3.187 → 0.464 | 6.87 | 3.35, 6.88, 9.80 |
| ehp / alns1 | 1.504 → 0.337 | 4.46 | 7.263 → 1.093 | 6.64 | 2.72, 85.28, 6.80 |
| ehp / alns_warm6 | 1.504 → 0.313 | 4.81 | 7.263 → 0.365 | 19.89 | 23.22, 19.70, 6.94 |

All reported diagnostic ratios above are finite observed ratios. The baseline reached every target before the fifteen-second deadline, so the earlier screening lower bounds are no longer needed. For spellsteal warm-three, two misses are retained and no overall ratio is reported. EHP warm-six has a 19.89× median, but not a 10× gain on every seed. Battle Monk exceeds 10× on every paired seed for both ALNS warm depths.

### Diagnostic targets and endpoint scores

| Query | Frozen reference | 99% target | Baseline endpoint scores | Warm-three endpoint scores | Warm-six endpoint scores |
|---|---:|---:|---|---|---|
| meta_warrior_battle_monk_uppercut_remove_4 | 25222.691078 | 24970.464167 | 25222.691078, 25222.691078, 25222.691078 | 25037.292095, 25222.691078, 25422.766520 | 25037.292095, 25222.691078, 25422.766520 |
| fam_spellsteal_small | 91703.870178 | 90786.831476 | 91703.870178, 91703.870178, 91703.870178 | 85927.088921, 85927.088921, 91703.870178 | 91703.870178, 91703.870178, 91703.870178 |
| ehp | 56207.730340 | 55645.653037 | 56207.730340, 56207.730340, 56207.730340 | 57646.937420, 57646.937420, 57646.937420 | 57646.937420, 56881.708515, 57646.937420 |

## Default decisions supported by screening

Across the 117 search queries, preserving warm witnesses and top-one selection had approximately neutral median time to the common target; retaining witnesses is still useful for immediate result availability. Enabling wide-key bounds was slower: 0.688× median T99 speedup on 88 common-attainment queries and 0.773× median completion speedup on 33 same-result-count completed query pairs. Keep the wide-bound experiment opt-in until a workload-specific benefit is established.

## Validation evidence retained

Widening the suite initially exposed 15 scoring mismatches: seven Light Bender snapshots, seven Acolyte snapshots, and the legacy healing query. The Rust evaluator returned zero where JavaScript expected positive healing, while skill-point assignment and stat assembly matched. The healing-schema repair recognizes current `power` fields and the legacy alias, aligns dynamic/compiled classification, and applies the current healing multiplier semantics. The initial 117-pass/15-fail record and baseline mismatch log remain available. The final recheck passed 132/132 exported scoring cases, one sampled witness per query; 45 small native/WASM comparisons also passed. These checks establish the tested backend consistency, not game-global or exhaustive skill-point optimality.

## Measurement limits

Time is parent-observed process wall time, including launch, parsing, compilation and warm search, with five-millisecond observation polling. Fixture generation is separate. Targets are same-query frozen screening scores, not optimality certificates. T99 speedups are conditional on attaining the exact common target; timeout/no-result runs stay in denominators. A median over the small subset where both algorithms succeed should not be presented as an all-workload multiplier. Three fresh seeds provide replication, not a high-confidence population estimate.
