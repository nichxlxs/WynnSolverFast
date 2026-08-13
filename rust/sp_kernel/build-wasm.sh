#!/usr/bin/env bash
# Build the browser engine: cargo -> wasm-bindgen -> wasm-opt.
# Output lands in js/solver/wasm/ (loaded by search.js's Rust path).
set -euo pipefail
cd "$(dirname "$0")"
OUT="../../js/solver/wasm"

cargo build --release --target wasm32-unknown-unknown --features wasm --lib
wasm-bindgen target/wasm32-unknown-unknown/release/sp_kernel.wasm \
    --out-dir "$OUT" --target web

# Size pass (optional but worth ~12%); binaryen ships wasm-opt.
OPT="$(command -v wasm-opt || echo ../../node_modules/binaryen/bin/wasm-opt)"
if [ -x "$OPT" ] || command -v wasm-opt >/dev/null 2>&1; then
    "$OPT" -Oz "$OUT/sp_kernel_bg.wasm" -o "$OUT/sp_kernel_bg.wasm.opt"
    mv "$OUT/sp_kernel_bg.wasm.opt" "$OUT/sp_kernel_bg.wasm"
    echo "wasm-opt applied"
else
    echo "wasm-opt not found — skipping size pass (npm i binaryen)"
fi
ls -la "$OUT"/*.wasm
