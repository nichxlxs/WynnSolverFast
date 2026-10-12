# Test fixtures

Exported fixtures the unit tests read (`fixtures/` is generated and ignored).

- `score_ehp_tome_all.json`: the `solver_bench_ehp_tome_all` score fixture
  (tome optimisation mode 2), read by the mana-rescue regression test in
  `src/scoring.rs`, which pins a score computed from this exact file.
- `score_fam_tierstack_small.json`: the tierstack small family score fixture,
  read by the crit-floor regression in `src/scoring.rs` (a Ruinous build
  whose damage falls as Dex rises, so the all-150 ceiling needs the floor).
