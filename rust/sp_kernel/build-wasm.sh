#!/usr/bin/env bash
# Build the browser engine: cargo -> wasm-bindgen -> wasm-opt -O3.
# Output lands in js/solver/wasm/ (loaded by search.js's Rust path).
set -euo pipefail
cd "$(dirname "$0")"
OUT="../../js/solver/wasm"

cargo build --release --target wasm32-unknown-unknown --features wasm --lib
wasm-bindgen target/wasm32-unknown-unknown/release/sp_kernel.wasm \
    --out-dir "$OUT" --target web

# Speed pass; binaryen ships wasm-opt. -O3 rather than the size preset
# -Oz: 1.051x geometric mean, 18/20 faster on the family suite plus
# spell_wide and heal (fixed work, 5 repeats, results identical) for +0.5%
# module size. simd128 on top measured 1.069x, 20/20, but would break
# browsers without it (Safari before 16.4), so it is not enabled.
OPT="$(command -v wasm-opt || echo ../../node_modules/binaryen/bin/wasm-opt)"
if [ -x "$OPT" ] || command -v wasm-opt >/dev/null 2>&1; then
    "$OPT" -O3 "$OUT/sp_kernel_bg.wasm" -o "$OUT/sp_kernel_bg.wasm.opt"
    mv "$OUT/sp_kernel_bg.wasm.opt" "$OUT/sp_kernel_bg.wasm"
    echo "wasm-opt applied"
else
    echo "wasm-opt not found — skipping the -O3 pass (npm i binaryen)"
fi
ls -la "$OUT"/*.wasm
