#!/usr/bin/env bash
# Exports Rust fixtures for the build-family benchmarks (PR #8).
#
# The families are defined as JS solver snapshots, so the Rust engine cannot
# run them until each is exported as an (enum, score) fixture pair. bench.py
# registers them as `fam_<family>_<size>`; without this script those rows are
# simply missing from the matrix, which is how they went unbenchmarked when
# the families first landed.
#
# Run from the repo root or from rust/sp_kernel. ~15 minutes for all 18.
set -euo pipefail
cd "$(dirname "$0")/../.."          # repo root
OUT=rust/sp_kernel/fixtures
for fam in cancelstack heavy_melee tierstack spellsteal spell_sustained hybrid; do
    for sz in small medium large; do
        printf '%-40s' "solver_family_${fam}_${sz}"
        SOLVER_BENCH_SECONDS=0.2 \
        SOLVER_EXPORT_RUST="$OUT/enum_fam_${fam}_${sz}.txt" \
        SOLVER_EXPORT_SCORE="$OUT/score_fam_${fam}_${sz}.json" \
            node js/solver/tests/test_solver_search.js "solver_family_${fam}_${sz}" \
            2>&1 | grep -oE 'search combinations: [0-9]+' || true
    done
done
