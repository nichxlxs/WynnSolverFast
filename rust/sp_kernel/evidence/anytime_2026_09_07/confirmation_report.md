# Time-to-quality benchmark

Process wall time includes launch, parsing, compilation, and warm search. Incumbent traces are observed by the parent process every 5 ms; delivery and trace overhead are included. Fixture generation is recorded separately.

References are labelled best-known unless independently established for the identical query. Default references are the maximum found by this campaign, so percentages do not certify proximity to the global optimum. Timeouts and no-result runs remain in all denominators.

| Scenario | Variant | T99 successes | T99 median (s) | No result | Completed | T99 speedup vs first arm |
|---|---|---:|---:|---:|---:|---:|
| meta_archer_boltslinger_fast_hybrid_remove_6 | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| meta_archer_boltslinger_fast_hybrid_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_archer_boltslinger_fast_hybrid_remove_6 | alns1 | 2/3 | 1.7681 | 0 | 0 | — |
| meta_archer_boltslinger_fast_hybrid_remove_6 | alns_warm6 | 1/3 | censored | 0 | 0 | — |
| meta_archer_sharpshooter_heavy_melee_remove_6 | safe_baseline15 | 3/3 | 0.1237 | 0 | 0 | — |
| meta_archer_sharpshooter_heavy_melee_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_archer_sharpshooter_heavy_melee_remove_6 | alns1 | 3/3 | 0.9835 | 0 | 0 | 0.13× |
| meta_archer_sharpshooter_heavy_melee_remove_6 | alns_warm6 | 3/3 | 0.0990 | 0 | 0 | 1.25× |
| meta_archer_trapper_slow_melee_remove_6 | safe_baseline15 | 3/3 | 0.0723 | 0 | 0 | — |
| meta_archer_trapper_slow_melee_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_archer_trapper_slow_melee_remove_6 | alns1 | 3/3 | 0.3966 | 0 | 0 | 0.18× |
| meta_archer_trapper_slow_melee_remove_6 | alns_warm6 | 3/3 | 0.0735 | 0 | 0 | 0.98× |
| meta_assassin_acrobat_spellspam_remove_6 | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| meta_assassin_acrobat_spellspam_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_assassin_acrobat_spellspam_remove_6 | alns1 | 2/3 | 1.2203 | 0 | 0 | — |
| meta_assassin_acrobat_spellspam_remove_6 | alns_warm6 | 2/3 | 0.4193 | 0 | 0 | — |
| meta_assassin_shadestepper_spellsteal_remove_6 | safe_baseline15 | 3/3 | 0.0882 | 0 | 0 | — |
| meta_assassin_shadestepper_spellsteal_remove_6 | best1_warm3 | 3/3 | 0.0881 | 0 | 0 | 1.00× |
| meta_assassin_shadestepper_spellsteal_remove_6 | alns1 | 3/3 | 0.0850 | 0 | 0 | 1.04× |
| meta_assassin_shadestepper_spellsteal_remove_6 | alns_warm6 | 3/3 | 0.0835 | 0 | 0 | 1.06× |
| meta_assassin_trickster_spell_remove_6 | safe_baseline15 | 3/3 | 0.0982 | 0 | 0 | — |
| meta_assassin_trickster_spell_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_assassin_trickster_spell_remove_6 | alns1 | 2/3 | 4.2065 | 0 | 0 | — |
| meta_assassin_trickster_spell_remove_6 | alns_warm6 | 3/3 | 0.0826 | 0 | 0 | 1.19× |
| meta_mage_arcanist_meteor_remove_6 | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| meta_mage_arcanist_meteor_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_mage_arcanist_meteor_remove_6 | alns1 | 3/3 | 2.4502 | 0 | 0 | — |
| meta_mage_arcanist_meteor_remove_6 | alns_warm6 | 3/3 | 2.4675 | 0 | 0 | — |
| meta_mage_light_bender_healing_remove_6 | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| meta_mage_light_bender_healing_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_mage_light_bender_healing_remove_6 | alns1 | 3/3 | 1.0935 | 0 | 0 | — |
| meta_mage_light_bender_healing_remove_6 | alns_warm6 | 3/3 | 1.0561 | 0 | 0 | — |
| meta_mage_riftwalker_cancelstack_remove_6 | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| meta_mage_riftwalker_cancelstack_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_mage_riftwalker_cancelstack_remove_6 | alns1 | 2/3 | 0.9856 | 0 | 0 | — |
| meta_mage_riftwalker_cancelstack_remove_6 | alns_warm6 | 1/3 | censored | 0 | 0 | — |
| meta_shaman_acolyte_healing_remove_6 | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| meta_shaman_acolyte_healing_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_shaman_acolyte_healing_remove_6 | alns1 | 3/3 | 3.1411 | 0 | 0 | — |
| meta_shaman_acolyte_healing_remove_6 | alns_warm6 | 3/3 | 3.7323 | 0 | 0 | — |
| meta_shaman_ritualist_tierstack_remove_6 | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| meta_shaman_ritualist_tierstack_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_shaman_ritualist_tierstack_remove_6 | alns1 | 0/3 | censored | 2 | 0 | — |
| meta_shaman_ritualist_tierstack_remove_6 | alns_warm6 | 2/3 | 0.8004 | 0 | 0 | — |
| meta_shaman_summoner_aura_remove_6 | safe_baseline15 | 3/3 | 0.1045 | 0 | 0 | — |
| meta_shaman_summoner_aura_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_shaman_summoner_aura_remove_6 | alns1 | 3/3 | 0.7738 | 0 | 0 | 0.14× |
| meta_shaman_summoner_aura_remove_6 | alns_warm6 | 3/3 | 0.0887 | 0 | 0 | 1.18× |
| meta_warrior_battle_monk_uppercut_remove_6 | safe_baseline15 | 3/3 | 0.3136 | 0 | 0 | — |
| meta_warrior_battle_monk_uppercut_remove_6 | best1_warm3 | 3/3 | 0.2937 | 0 | 0 | 1.07× |
| meta_warrior_battle_monk_uppercut_remove_6 | alns1 | 3/3 | 0.7604 | 0 | 0 | 0.41× |
| meta_warrior_battle_monk_uppercut_remove_6 | alns_warm6 | 3/3 | 0.8040 | 0 | 0 | 0.39× |
| meta_warrior_fallen_uppercut_remove_6 | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| meta_warrior_fallen_uppercut_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_warrior_fallen_uppercut_remove_6 | alns1 | 3/3 | 0.3326 | 0 | 0 | — |
| meta_warrior_fallen_uppercut_remove_6 | alns_warm6 | 3/3 | 0.3890 | 0 | 0 | — |
| meta_warrior_paladin_tank_remove_6 | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| meta_warrior_paladin_tank_remove_6 | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| meta_warrior_paladin_tank_remove_6 | alns1 | 3/3 | 0.6547 | 0 | 0 | — |
| meta_warrior_paladin_tank_remove_6 | alns_warm6 | 3/3 | 0.6074 | 0 | 0 | — |
| fam_cancelstack_large | safe_baseline15 | 3/3 | 0.0630 | 0 | 0 | — |
| fam_cancelstack_large | best1_warm3 | 3/3 | 0.2933 | 0 | 0 | 0.21× |
| fam_cancelstack_large | alns1 | 3/3 | 0.1597 | 0 | 0 | 0.39× |
| fam_cancelstack_large | alns_warm6 | 3/3 | 0.0590 | 0 | 0 | 1.07× |
| fam_heavy_melee_large | safe_baseline15 | 3/3 | 0.0728 | 0 | 0 | — |
| fam_heavy_melee_large | best1_warm3 | 3/3 | 0.0723 | 0 | 0 | 1.01× |
| fam_heavy_melee_large | alns1 | 3/3 | 0.0419 | 0 | 0 | 1.74× |
| fam_heavy_melee_large | alns_warm6 | 3/3 | 0.0418 | 0 | 0 | 1.74× |
| fam_tierstack_large | safe_baseline15 | 3/3 | 0.0623 | 0 | 0 | — |
| fam_tierstack_large | best1_warm3 | 3/3 | 0.4380 | 0 | 0 | 0.14× |
| fam_tierstack_large | alns1 | 3/3 | 0.4601 | 0 | 0 | 0.14× |
| fam_tierstack_large | alns_warm6 | 3/3 | 0.0365 | 0 | 0 | 1.71× |
| fam_spellsteal_large | safe_baseline15 | 0/3 | censored | 0 | 0 | — |
| fam_spellsteal_large | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| fam_spellsteal_large | alns1 | 2/3 | 1.1821 | 0 | 0 | — |
| fam_spellsteal_large | alns_warm6 | 3/3 | 1.0388 | 0 | 0 | — |
| fam_spell_sustained_large | safe_baseline15 | 3/3 | 0.0520 | 0 | 0 | — |
| fam_spell_sustained_large | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| fam_spell_sustained_large | alns1 | 3/3 | 0.7243 | 0 | 0 | 0.07× |
| fam_spell_sustained_large | alns_warm6 | 3/3 | 0.0470 | 0 | 0 | 1.11× |
| fam_hybrid_large | safe_baseline15 | 3/3 | 0.2442 | 0 | 0 | — |
| fam_hybrid_large | best1_warm3 | 3/3 | 0.2376 | 0 | 0 | 1.03× |
| fam_hybrid_large | alns1 | 1/3 | censored | 0 | 0 | — |
| fam_hybrid_large | alns_warm6 | 1/3 | censored | 0 | 0 | — |
| gaia_all_free | safe_baseline15 | 3/3 | 0.0575 | 0 | 0 | — |
| gaia_all_free | best1_warm3 | 0/3 | censored | 0 | 0 | — |
| gaia_all_free | alns1 | 0/3 | censored | 3 | 0 | — |
| gaia_all_free | alns_warm6 | 0/3 | censored | 3 | 0 | — |

A censored T99 median means the target was not reached in at least half the runs. Speedup is omitted unless every repeat on both arms reached the same target. Completion time is never substituted for baseline T99.
