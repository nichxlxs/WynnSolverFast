# Follow-up profile: descriptive quality review

Post-hoc combined scheduler/budget profile, fixed22 queries and fresh707/808/909 seeds, same frozen targets. Endpoint/count comparison with earlier seeds is descriptive only; no paired runtime or isolated parameter attribution.

Runs with valid builds: 66/66. Process failures: 0. Outcomes: {'timeout': 66}.

| Target | Follow-up runs | Follow-up all-three queries | Earlier warm-six runs | Earlier baseline runs |
|---|---:|---:|---:|---:|
| 95% | 66/66 | 22/22 | 61/66 | 45/66 |
| 99% | 66/66 | 22/22 | 55/66 | 36/66 |
| 99.9% | 65/66 | 21/22 | 52/66 | 33/66 |

| Query | Frozen 99% target | New target hits | New endpoint scores (707,808,909) | Earlier warm-six hits | Earlier baseline hits | Descriptive score median gain over earlier baseline |
|---|---:|---:|---|---:|---:|---:|
| meta_archer_boltslinger_fast_hybrid_remove_6 | 154107.141974 | 3/3 | 155663.780, 155663.780, 155663.780 | 1/3 | 0/3 | +4.98% |
| meta_archer_sharpshooter_heavy_melee_remove_6 | 38223.197342 | 3/3 | 38609.290, 38614.549, 38614.549 | 3/3 | 3/3 | +0.01% |
| meta_archer_trapper_slow_melee_remove_6 | 25635.533032 | 3/3 | 25894.478, 25894.478, 25894.478 | 3/3 | 3/3 | +0.00% |
| meta_assassin_acrobat_spellspam_remove_6 | 25863.433344 | 3/3 | 26208.868, 26208.868, 26124.680 | 2/3 | 0/3 | +7.14% |
| meta_assassin_shadestepper_spellsteal_remove_6 | 53569.500182 | 3/3 | 54110.606, 54110.606, 54110.606 | 3/3 | 3/3 | +0.00% |
| meta_assassin_trickster_spell_remove_6 | 78393.953193 | 3/3 | 79750.241, 79750.241, 79750.241 | 3/3 | 3/3 | +0.71% |
| meta_mage_arcanist_meteor_remove_6 | 42860.612097 | 3/3 | 44004.812, 44734.318, 44636.208 | 3/3 | 0/3 | +38.16% |
| meta_mage_light_bender_healing_remove_6 | 15649.137900 | 3/3 | 21589.485, 22343.715, 22459.500 | 3/3 | 0/3 | +51.19% |
| meta_mage_riftwalker_cancelstack_remove_6 | 358150.997293 | 3/3 | 361768.684, 361768.684, 361768.684 | 1/3 | 0/3 | +13.14% |
| meta_shaman_acolyte_healing_remove_6 | 12121.720875 | 3/3 | 13536.075, 12441.600, 12516.000 | 3/3 | 0/3 | +21.20% |
| meta_shaman_ritualist_tierstack_remove_6 | 264581.441579 | 3/3 | 270347.232, 270347.232, 270347.232 | 2/3 | 0/3 | +27.99% |
| meta_shaman_summoner_aura_remove_6 | 8144.273981 | 3/3 | 8338.179, 8240.620, 8240.620 | 3/3 | 3/3 | +0.62% |
| meta_warrior_battle_monk_uppercut_remove_6 | 25680.151807 | 3/3 | 25939.547, 26553.015, 26553.015 | 3/3 | 3/3 | +2.36% |
| meta_warrior_fallen_uppercut_remove_6 | 37707.521237 | 3/3 | 38406.485, 38088.405, 37937.814 | 3/3 | 0/3 | +5.21% |
| meta_warrior_paladin_tank_remove_6 | 16280.843581 | 3/3 | 16556.752, 16556.752, 16556.752 | 3/3 | 0/3 | +5.94% |
| fam_cancelstack_large | 505607.857778 | 3/3 | 510715.008, 510715.008, 510715.008 | 3/3 | 3/3 | +0.00% |
| fam_heavy_melee_large | 37444.181754 | 3/3 | 37822.406, 37822.406, 37822.406 | 3/3 | 3/3 | +0.00% |
| fam_tierstack_large | 194129.188362 | 3/3 | 196090.089, 196090.089, 196090.089 | 3/3 | 3/3 | +0.00% |
| fam_spellsteal_large | 92231.036029 | 3/3 | 93162.663, 93162.663, 93162.663 | 3/3 | 0/3 | +17.17% |
| fam_spell_sustained_large | 1557597.794585 | 3/3 | 1573331.106, 1573331.106, 1573331.106 | 3/3 | 3/3 | +0.00% |
| fam_hybrid_large | 259317.701263 | 3/3 | 262855.806, 262855.806, 262855.806 | 1/3 | 3/3 | +0.35% |
| gaia_all_free | 320870.544087 | 3/3 | 324111.661, 324111.661, 324111.661 | 0/3 | 3/3 | +0.00% |

These counts and score changes describe different seed sets. The combined experiment changes warm budget, repair limit and scheduling together, so it cannot assign improvement to one parameter or support same-seed timing ratios. Earlier diagnostic timing gains belong to the original scheduler/profile, not this combined follow-up. Full JSON retains absolute references, all endpoint scores, first-result times and misses.

The combined profile is the preferred tested experimental configuration because it returned a build and reached the frozen 99% target in all 66 runs. It should not be presented as a universal production replacement. All new endpoints were at least the maximum earlier baseline endpoint for the same query; comparing medians against that earlier baseline gives 15 improvements and seven ties. Against original warm-six ALNS, two medians declined: Light Bender by 0.516% and Acolyte by 5.468%. Both still exceeded the earlier baseline and attained the frozen target on every seed.

Gaia recovered: first build appeared in 0.118–0.124 seconds, and the frozen 99% target was reached in 0.154–0.159 seconds, with endpoint score 324111.660694 on all three runs. These absolute times characterize the follow-up; no paired speedup against different earlier seeds is asserted.

The sole 99.9% miss was Fallen seed 909, ending at 37937.813586 against frozen reference 38088.405289. Every run still reached 99%.
