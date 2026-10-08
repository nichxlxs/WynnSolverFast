# Test fixtures

Exported fixtures the unit tests read (`fixtures/` is generated and
ignored). The tests pin results computed from these exact files, so
regenerate a file only together with the constants that depend on it.

- `enum_fam_{tierstack,heavy_melee}_small.txt` and the matching
  `score_*.json`: copies of the family fixtures produced by
  `gen_family_fixtures.sh`, read by the R21 epsilon regressions in
  `src/enumerate.rs` (they pin the tierstack and heavy melee small optima).
- `score_ehp_tome_all.json`: the `solver_bench_ehp_tome_all` score fixture
  (tome optimisation mode 2), read by the mana-rescue regression test in
  `src/scoring.rs`.
