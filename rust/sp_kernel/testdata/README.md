# Test fixtures

Exported fixtures the unit tests read (`fixtures/` is generated and
ignored). The tests pin results computed from these exact files, so
regenerate a file only together with the constants that depend on it.

- `enum_fam_{tierstack,heavy_melee}_small.txt` and the matching
  `score_*.json`: copies of the family fixtures produced by
  `gen_family_fixtures.sh`, read by the R21 epsilon regressions in
  `src/enumerate.rs` (they pin the tierstack and heavy melee small optima)
  and the R20 window test (it pins tierstack small's 3 builds within 2%).
- `score_ehp_tome_all.json`: the `solver_bench_ehp_tome_all` score fixture
  (tome optimisation mode 2), read by the mana-rescue regression test in
  `src/scoring.rs`.
- `enum_radiance.txt` and `score_radiance.json`: the
  `solver_indep_oracle_radiance` fixtures (spell oracle, Radiance and Divine
  Honor on), read by the Radiance item-SP tests in `src/scoring.rs` and
  `src/enumerate.rs` (the latter pins the JS top-15 bit for bit).
