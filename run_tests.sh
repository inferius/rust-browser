#!/usr/bin/env bash
# Run all tests, log do souboru, summary + failure extract.
# Pouziti:  ./run_tests.sh

set +e

log_dir="test_logs"
mkdir -p "$log_dir"
ts=$(date +%Y%m%d-%H%M%S)
log_file="$log_dir/test-$ts.log"
fail_file="$log_dir/failures-$ts.log"
build_file="$log_dir/build-$ts.log"

cat <<EOF | tee "$log_file"
============================================================
RustWebEngine - test runner
Cas:    $(date)
Vystup: $log_file
============================================================
EOF

# === BUILD ===
echo ""
echo "=== Build ==="
cargo build --color=never 2>&1 | tee "$build_file"
build_exit=${PIPESTATUS[0]}
cat "$build_file" >> "$log_file"

if [ "$build_exit" -ne 0 ]; then
    echo ""
    echo "BUILD FAILED (exit $build_exit) - viz $build_file"
    cp "$build_file" "$fail_file"
    exit 1
fi

# === TESTY ===
echo ""
echo "=== cargo test --no-fail-fast ==="
test_tmp="$log_dir/_test_tmp.log"
cargo test --no-fail-fast --color=never 2>&1 | tee "$test_tmp"
test_exit=${PIPESTATUS[0]}
cat "$test_tmp" >> "$log_file"
rm -f "$test_tmp"

# Extract failure regions
awk '
/^---- .+ stdout ----/ { inblock=1 }
/^test result:/ && inblock { inblock=0 }
inblock { print }
/FAILED|panicked at|assertion/ && !inblock { print }
' "$log_file" > "$fail_file"

# === SUMMARY ===
echo ""
echo "=== Summary ==="
grep "^test result:" "$log_file" || true

total_passed=0
total_failed=0
while IFS= read -r line; do
    p=$(echo "$line" | grep -oE "[0-9]+ passed" | head -1 | grep -oE "[0-9]+")
    f=$(echo "$line" | grep -oE "[0-9]+ failed" | head -1 | grep -oE "[0-9]+")
    [ -n "$p" ] && total_passed=$((total_passed + p))
    [ -n "$f" ] && total_failed=$((total_failed + f))
done < <(grep "^test result:" "$log_file")

echo ""
echo "Passed celkem: $total_passed"
echo "Failed celkem: $total_failed"
echo "Log:           $log_file"
echo "Failures log:  $fail_file"

if [ "$total_failed" -gt 0 ] || [ "$test_exit" -ne 0 ]; then
    echo ""
    echo "FAIL ($total_failed selhanych testu)"
    exit 1
fi

echo ""
echo "OK ($total_passed testu prosly)"
exit 0
