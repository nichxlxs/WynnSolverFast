# Test fixtures

Exported family fixtures (`fixtures/` is generated and ignored) that the
unit tests in `src/enumerate.rs` read (the R21 epsilon regressions). They are copies of `fixtures/enum_fam_{tierstack,heavy_melee}_small.txt`
and the matching `score_*.json`, produced by `gen_family_fixtures.sh`.

The tests pin results computed from these exact files (the tierstack and
heavy melee small optima), so regenerate them only together with those
constants.
