#!/usr/bin/env bash
# Profile-guided native build of enum_kernel (R25). Native only: the wasm
# build is untouched.
#
# The profile is regenerated from the current source on every run and never
# committed, so it cannot go stale against the code it was taken from.
#
#   ./build-pgo.sh            # binary at target/pgo-use/release/enum_kernel
#   PGO_TRAIN_SECS=30 ./build-pgo.sh
#
# Needs: rustup component add llvm-tools, and the family fixtures in
# fixtures/ (see BENCHMARKING.md, "Fixtures first").
#
# PGO changes code layout and inlining only, never results. Check that with
#   python3 benchmark_ab.py --b-bin target/pgo-use/release/enum_kernel \
#       --scenarios families --repeat 3
# which compares the top results and every counter as well as timing.
set -euo pipefail
cd "$(dirname "$0")"

TRAIN_SECS="${PGO_TRAIN_SECS:-15}"
PROF_DIR="$PWD/target/pgo-profiles"
# RUSTFLAGS replaces .cargo/config.toml's rustflags, so restate target-cpu.
BASE_FLAGS="-C target-cpu=native"

PROFDATA="$(command -v llvm-profdata || true)"
if [ -z "$PROFDATA" ]; then
    SYSROOT="$(rustc --print sysroot)"
    PROFDATA="$(find "$SYSROOT" -name llvm-profdata -type f 2>/dev/null | head -1)"
fi
if [ -z "$PROFDATA" ]; then
    echo "llvm-profdata not found: rustup component add llvm-tools" >&2
    exit 1
fi

# Training set: every scoring family, so no one objective's hot path is
# favoured. Small runs cover the whole search including the proof tail;
# medium and large runs are capped and cover the deep bound/leaf loops.
TRAIN=()
for fam in cancelstack heavy_melee tierstack spellsteal spell_sustained hybrid; do
    for size in small medium large; do
        TRAIN+=("fam_${fam}_${size}")
    done
done
for s in "${TRAIN[@]}"; do
    if [ ! -f "fixtures/enum_$s.txt" ] || [ ! -f "fixtures/score_$s.json" ]; then
        echo "missing fixtures/enum_$s.txt or score_$s.json (see BENCHMARKING.md)" >&2
        exit 1
    fi
done
# Objectives the families do not cover. PGO lays out untrained code as cold:
# without heal in the set, total_healing ran 8% slower than a plain build.
# Optional, since not every checkout exports them; spell_wide stays out as
# the held-out check.
for s in heal tome_all tome_guild setwep; do
    if [ -f "fixtures/enum_$s.txt" ] && [ -f "fixtures/score_$s.json" ]; then
        TRAIN+=("$s")
    else
        echo "note: fixtures for $s not found, its objective is untrained" >&2
    fi
done

rm -rf "$PROF_DIR"
mkdir -p "$PROF_DIR"

echo "== instrumented build"
CARGO_TARGET_DIR=target/pgo-gen \
RUSTFLAGS="$BASE_FLAGS -Cprofile-generate=$PROF_DIR" \
    cargo build --release --bin enum_kernel
GEN=target/pgo-gen/release/enum_kernel

echo "== training (${#TRAIN[@]} runs, ${TRAIN_SECS}s cap each)"
for s in "${TRAIN[@]}"; do
    # One thread: the profile then reflects the per-worker hot path without
    # scheduler noise, and runs are reproducible.
    ENUM_TIME_CAP_SECS="$TRAIN_SECS" "$GEN" "fixtures/enum_$s.txt" 1 \
        "fixtures/score_$s.json" > /dev/null
    echo "   $s"
done

"$PROFDATA" merge -o "$PROF_DIR/merged.profdata" "$PROF_DIR"/*.profraw

echo "== optimised build"
CARGO_TARGET_DIR=target/pgo-use \
RUSTFLAGS="$BASE_FLAGS -Cprofile-use=$PROF_DIR/merged.profdata" \
    cargo build --release --bin enum_kernel
echo "built target/pgo-use/release/enum_kernel"
