# Test fixtures

Exported family fixtures (`fixtures/` is generated and ignored) that the
unit tests in `src/enumerate.rs` read: the R21 epsilon regressions and the
R20 window test. They are copies of `fixtures/enum_fam_{tierstack,heavy_melee}_small.txt`
and the matching `score_*.json`, produced by `gen_family_fixtures.sh`.

The tests pin results computed from these exact files (the tierstack small
optimum 2.20863848359218158e5, its three builds within 2%), so regenerate
them only together with those constants.
