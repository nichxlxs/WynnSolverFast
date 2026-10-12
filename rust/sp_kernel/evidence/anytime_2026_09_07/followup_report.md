# Time-to-quality benchmark

Process wall time includes launch, parsing, compilation, and warm search. Incumbent traces are observed by the parent process every 5 ms; delivery and trace overhead are included. Fixture generation is recorded separately.

References are labelled best-known unless independently established for the identical query. Default references are the maximum found by this campaign, so percentages do not certify proximity to the global optimum. Timeouts and no-result runs remain in all denominators.

| Scenario | Variant | T99 successes | T99 median (s) | No result | Completed | T99 speedup vs first arm |
|---|---|---:|---:|---:|---:|---:|
| meta_archer_boltslinger_fast_hybrid_remove_6 | alns_cyclic_warm6 | 3/3 | 0.6481 | 0 | 0 | — |
| meta_archer_sharpshooter_heavy_melee_remove_6 | alns_cyclic_warm6 | 3/3 | 0.0977 | 0 | 0 | — |
| meta_archer_trapper_slow_melee_remove_6 | alns_cyclic_warm6 | 3/3 | 0.0732 | 0 | 0 | — |
| meta_assassin_acrobat_spellspam_remove_6 | alns_cyclic_warm6 | 3/3 | 0.8106 | 0 | 0 | — |
| meta_assassin_shadestepper_spellsteal_remove_6 | alns_cyclic_warm6 | 3/3 | 0.0731 | 0 | 0 | — |
| meta_assassin_trickster_spell_remove_6 | alns_cyclic_warm6 | 3/3 | 0.0825 | 0 | 0 | — |
| meta_mage_arcanist_meteor_remove_6 | alns_cyclic_warm6 | 3/3 | 2.2210 | 0 | 0 | — |
| meta_mage_light_bender_healing_remove_6 | alns_cyclic_warm6 | 3/3 | 0.2090 | 0 | 0 | — |
| meta_mage_riftwalker_cancelstack_remove_6 | alns_cyclic_warm6 | 3/3 | 0.7834 | 0 | 0 | — |
| meta_shaman_acolyte_healing_remove_6 | alns_cyclic_warm6 | 3/3 | 2.3347 | 0 | 0 | — |
| meta_shaman_ritualist_tierstack_remove_6 | alns_cyclic_warm6 | 3/3 | 0.2971 | 0 | 0 | — |
| meta_shaman_summoner_aura_remove_6 | alns_cyclic_warm6 | 3/3 | 0.0779 | 0 | 0 | — |
| meta_warrior_battle_monk_uppercut_remove_6 | alns_cyclic_warm6 | 3/3 | 0.2145 | 0 | 0 | — |
| meta_warrior_fallen_uppercut_remove_6 | alns_cyclic_warm6 | 3/3 | 1.4649 | 0 | 0 | — |
| meta_warrior_paladin_tank_remove_6 | alns_cyclic_warm6 | 3/3 | 0.9083 | 0 | 0 | — |
| fam_cancelstack_large | alns_cyclic_warm6 | 3/3 | 0.0586 | 0 | 0 | — |
| fam_heavy_melee_large | alns_cyclic_warm6 | 3/3 | 0.0417 | 0 | 0 | — |
| fam_tierstack_large | alns_cyclic_warm6 | 3/3 | 0.0366 | 0 | 0 | — |
| fam_spellsteal_large | alns_cyclic_warm6 | 3/3 | 0.6777 | 0 | 0 | — |
| fam_spell_sustained_large | alns_cyclic_warm6 | 3/3 | 0.0471 | 0 | 0 | — |
| fam_hybrid_large | alns_cyclic_warm6 | 3/3 | 0.7831 | 0 | 0 | — |
| gaia_all_free | alns_cyclic_warm6 | 3/3 | 0.1547 | 0 | 0 | — |

A censored T99 median means the target was not reached in at least half the runs. Speedup is omitted unless every repeat on both arms reached the same target. Completion time is never substituted for baseline T99.
