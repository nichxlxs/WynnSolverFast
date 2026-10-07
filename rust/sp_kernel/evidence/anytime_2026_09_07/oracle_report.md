# Time-to-quality benchmark

Process wall time includes launch, parsing, compilation, and warm search. Incumbent traces are observed by the parent process every 5 ms; delivery and trace overhead are included. Fixture generation is recorded separately.

References are labelled best-known unless independently established for the identical query. Default references are the maximum found by this campaign, so percentages do not certify proximity to the global optimum. Timeouts and no-result runs remain in all denominators.

| Scenario | Variant | T99 successes | T99 median (s) | No result | Completed | T99 speedup vs first arm |
|---|---|---:|---:|---:|---:|---:|
| meta_archer_boltslinger_fast_hybrid_remove_1 | independent_enumeration | 1/1 | 0.0329 | 0 | 1 | — |
| meta_archer_boltslinger_fast_hybrid_remove_2 | independent_enumeration | 1/1 | 0.0470 | 0 | 1 | — |
| meta_archer_boltslinger_fast_hybrid_remove_3 | independent_enumeration | 1/1 | 0.0780 | 0 | 1 | — |
| meta_archer_sharpshooter_heavy_melee_remove_1 | independent_enumeration | 1/1 | 0.0262 | 0 | 1 | — |
| meta_archer_sharpshooter_heavy_melee_remove_2 | independent_enumeration | 1/1 | 0.0519 | 0 | 1 | — |
| meta_archer_sharpshooter_heavy_melee_remove_3 | independent_enumeration | 1/1 | 0.1549 | 0 | 1 | — |
| meta_archer_trapper_slow_melee_remove_1 | independent_enumeration | 1/1 | 0.0261 | 0 | 1 | — |
| meta_archer_trapper_slow_melee_remove_2 | independent_enumeration | 1/1 | 0.0314 | 0 | 1 | — |
| meta_archer_trapper_slow_melee_remove_3 | independent_enumeration | 1/1 | 0.2835 | 0 | 1 | — |
| meta_assassin_acrobat_spellspam_remove_1 | independent_enumeration | 1/1 | 0.0158 | 0 | 1 | — |
| meta_assassin_acrobat_spellspam_remove_2 | independent_enumeration | 1/1 | 0.0312 | 0 | 1 | — |
| meta_assassin_acrobat_spellspam_remove_3 | independent_enumeration | 1/1 | 0.4471 | 0 | 1 | — |
| meta_assassin_shadestepper_spellsteal_remove_1 | independent_enumeration | 1/1 | 0.0209 | 0 | 1 | — |
| meta_assassin_shadestepper_spellsteal_remove_2 | independent_enumeration | 1/1 | 0.0364 | 0 | 1 | — |
| meta_assassin_shadestepper_spellsteal_remove_3 | independent_enumeration | 1/1 | 0.7715 | 0 | 1 | — |
| meta_assassin_trickster_spell_remove_1 | independent_enumeration | 1/1 | 0.0160 | 0 | 1 | — |
| meta_assassin_trickster_spell_remove_2 | independent_enumeration | 1/1 | 0.0312 | 0 | 1 | — |
| meta_assassin_trickster_spell_remove_3 | independent_enumeration | 1/1 | 0.0522 | 0 | 1 | — |
| meta_mage_arcanist_meteor_remove_1 | independent_enumeration | 1/1 | 0.0160 | 0 | 1 | — |
| meta_mage_arcanist_meteor_remove_2 | independent_enumeration | 1/1 | 0.0466 | 0 | 1 | — |
| meta_mage_arcanist_meteor_remove_3 | independent_enumeration | 1/1 | 0.0521 | 0 | 1 | — |
| meta_mage_light_bender_healing_remove_1 | independent_enumeration | 1/1 | 0.0211 | 0 | 1 | — |
| meta_mage_light_bender_healing_remove_2 | independent_enumeration | 1/1 | 0.0470 | 0 | 1 | — |
| meta_mage_light_bender_healing_remove_3 | independent_enumeration | 1/1 | 0.1192 | 0 | 0 | — |
| meta_mage_riftwalker_cancelstack_remove_1 | independent_enumeration | 1/1 | 0.0160 | 0 | 1 | — |
| meta_mage_riftwalker_cancelstack_remove_2 | independent_enumeration | 1/1 | 0.0363 | 0 | 1 | — |
| meta_mage_riftwalker_cancelstack_remove_3 | independent_enumeration | 1/1 | 1.9174 | 0 | 1 | — |
| meta_shaman_acolyte_healing_remove_1 | independent_enumeration | 1/1 | 0.0386 | 0 | 1 | — |
| meta_shaman_acolyte_healing_remove_2 | independent_enumeration | 1/1 | 0.9943 | 0 | 1 | — |
| meta_shaman_acolyte_healing_remove_3 | independent_enumeration | 1/1 | 17.9247 | 0 | 0 | — |
| meta_shaman_ritualist_tierstack_remove_1 | independent_enumeration | 1/1 | 0.0264 | 0 | 1 | — |
| meta_shaman_ritualist_tierstack_remove_2 | independent_enumeration | 1/1 | 0.0835 | 0 | 1 | — |
| meta_shaman_ritualist_tierstack_remove_3 | independent_enumeration | 1/1 | 0.5798 | 0 | 1 | — |
| meta_shaman_summoner_aura_remove_1 | independent_enumeration | 1/1 | 0.0262 | 0 | 1 | — |
| meta_shaman_summoner_aura_remove_2 | independent_enumeration | 1/1 | 0.0173 | 0 | 1 | — |
| meta_shaman_summoner_aura_remove_3 | independent_enumeration | 1/1 | 0.0588 | 0 | 1 | — |
| meta_warrior_battle_monk_uppercut_remove_1 | independent_enumeration | 1/1 | 0.0211 | 0 | 1 | — |
| meta_warrior_battle_monk_uppercut_remove_2 | independent_enumeration | 1/1 | 0.0519 | 0 | 1 | — |
| meta_warrior_battle_monk_uppercut_remove_3 | independent_enumeration | 1/1 | 0.0521 | 0 | 1 | — |
| meta_warrior_fallen_uppercut_remove_1 | independent_enumeration | 1/1 | 0.0211 | 0 | 1 | — |
| meta_warrior_fallen_uppercut_remove_2 | independent_enumeration | 1/1 | 0.0473 | 0 | 1 | — |
| meta_warrior_fallen_uppercut_remove_3 | independent_enumeration | 1/1 | 0.0519 | 0 | 1 | — |
| meta_warrior_paladin_tank_remove_1 | independent_enumeration | 1/1 | 0.0261 | 0 | 1 | — |
| meta_warrior_paladin_tank_remove_2 | independent_enumeration | 1/1 | 0.0316 | 0 | 1 | — |
| meta_warrior_paladin_tank_remove_3 | independent_enumeration | 1/1 | 0.0567 | 0 | 1 | — |
| meta_archer_boltslinger_fast_hybrid_known_good | independent_enumeration | 1/1 | 0.0056 | 0 | 1 | — |
| meta_archer_sharpshooter_heavy_melee_known_good | independent_enumeration | 1/1 | 0.0055 | 0 | 1 | — |
| meta_archer_trapper_slow_melee_known_good | independent_enumeration | 1/1 | 0.0055 | 0 | 1 | — |
| meta_assassin_acrobat_spellspam_known_good | independent_enumeration | 1/1 | 0.0055 | 0 | 1 | — |
| meta_assassin_shadestepper_spellsteal_known_good | independent_enumeration | 1/1 | 0.0056 | 0 | 1 | — |
| meta_assassin_trickster_spell_known_good | independent_enumeration | 1/1 | 0.0066 | 0 | 1 | — |
| meta_mage_arcanist_meteor_known_good | independent_enumeration | 1/1 | 0.0055 | 0 | 1 | — |
| meta_mage_light_bender_healing_known_good | independent_enumeration | 1/1 | 0.0058 | 0 | 1 | — |
| meta_mage_riftwalker_cancelstack_known_good | independent_enumeration | 1/1 | 0.0058 | 0 | 1 | — |
| meta_shaman_acolyte_healing_known_good | independent_enumeration | 1/1 | 0.0055 | 0 | 1 | — |
| meta_shaman_ritualist_tierstack_known_good | independent_enumeration | 1/1 | 0.0056 | 0 | 1 | — |
| meta_shaman_summoner_aura_known_good | independent_enumeration | 1/1 | 0.0055 | 0 | 1 | — |
| meta_warrior_battle_monk_uppercut_known_good | independent_enumeration | 1/1 | 0.0056 | 0 | 1 | — |
| meta_warrior_fallen_uppercut_known_good | independent_enumeration | 1/1 | 0.0055 | 0 | 1 | — |
| meta_warrior_paladin_tank_known_good | independent_enumeration | 1/1 | 0.0056 | 0 | 1 | — |

A censored T99 median means the target was not reached in at least half the runs. Speedup is omitted unless every repeat on both arms reached the same target. Completion time is never substituted for baseline T99.
