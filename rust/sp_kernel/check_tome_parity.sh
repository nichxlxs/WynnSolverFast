#!/usr/bin/env bash
# Rust-vs-JS parity on tome-optimisation scenarios.
#
# The tome is a SEARCHED dimension: per leaf the engine tries each guild
# candidate (each needs its own SP solve) crossed with each weapon/armour
# bundle, and keeps the best. This checks the Rust engine reaches the same
# answer as the JS engine, which is the reference implementation.
#
# Run from the repo root or from rust/sp_kernel:
#   rust/sp_kernel/check_tome_parity.sh
#
# Exports the fixtures with SOLVER_EXPORT_SCORE_CASES=0: the per-case
# expectations are for `score_kernel`'s differential validator and take far
# longer to sample than the whole rest of this check. Parity here is measured
# on the solved top-N, not on sampled cases.
set -uo pipefail

cd "$(dirname "$0")/../.." || exit 1
ROOT="$PWD"
FIX="$ROOT/rust/sp_kernel/fixtures"
mkdir -p "$FIX"

cargo build --release --manifest-path rust/sp_kernel/Cargo.toml >/dev/null 2>&1 || {
    echo "FAIL: cargo build"; exit 1; }

fail=0
for scen in solver_oracle_tome_guild solver_oracle_tome_all; do
    short="${scen#solver_oracle_}"
    echo "== $scen =="

    SOLVER_EXPORT_SCORE_CASES=0 \
    SOLVER_EXPORT_RUST="$FIX/enum_$short.txt" \
    SOLVER_EXPORT_SCORE="$FIX/score_$short.json" \
        node js/solver/tests/test_solver_search.js "$scen" >/tmp/tome_exp.log 2>&1

    mode=$(grep -o "tome opt mode [0-9]*: [0-9]* guild candidates, [0-9]* bundles" /tmp/tome_exp.log | head -1)
    if [ -z "$mode" ]; then
        echo "  FAIL: export did not prepare tome optimisation (fixture would"
        echo "        describe a non-tome search under a tome name)"; fail=1; continue
    fi
    echo "  $mode"

    # The JS engine is the reference.
    node js/solver/tests/test_solver_search.js "$scen" >/tmp/tome_js.log 2>&1
    js_best=$(grep -o "best score: [0-9.]*" /tmp/tome_js.log | head -1 | awk '{print $3}')
    js_items=$(grep "best items:" /tmp/tome_js.log | head -1 | sed 's/.*best items: //')
    js_checked=$(grep -o "checked: [0-9]*" /tmp/tome_js.log | head -1 | awk '{print $2}')

    rust_out=$(./rust/sp_kernel/target/release/enum_kernel \
        "$FIX/enum_$short.txt" 1 "$FIX/score_$short.json" 2>&1)
    rust_best=$(echo "$rust_out" | grep "top15:" | head -1 | sed 's/top15: //' | awk -F' \\| ' '{print $1}')
    rust_items=$(echo "$rust_out" | grep "top15:" | head -1 | awk -F' \\| ' '{print $2}')
    rust_checked=$(echo "$rust_out" | grep -o "checked [0-9]*" | head -1 | awk '{print $2}')

    # Compare at the precision the JS log prints (it rounds for display).
    same=$(python3 -c "
import sys
try:
    j, r = float('$js_best'), float('$rust_best')
except Exception:
    print('parse'); sys.exit()
print('yes' if abs(j - r) <= max(1.0, abs(j) * 1e-9) else 'no')
")
    echo "  js:   $js_best  checked=$js_checked"
    echo "  rust: $rust_best  checked=$rust_checked"
    if [ "$same" != "yes" ]; then echo "  FAIL: top-1 score differs"; fail=1; fi
    if [ "$js_items" != "$rust_items" ]; then
        echo "  FAIL: top-1 build differs"
        echo "    js:   $js_items"
        echo "    rust: $rust_items"; fail=1
    fi
    if [ "$js_checked" != "$rust_checked" ]; then
        echo "  FAIL: leaves checked differ ($js_checked vs $rust_checked)"; fail=1
    fi
    [ $fail -eq 0 ] && echo "  OK: same score, same build, same space"
done

if [ $fail -ne 0 ]; then echo; echo "TOME PARITY: FAILED"; exit 1; fi
echo; echo "TOME PARITY: OK"
