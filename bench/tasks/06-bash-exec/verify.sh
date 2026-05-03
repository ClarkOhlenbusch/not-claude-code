#!/usr/bin/env bash
set -e
grep -q "BENCH_MARKER_06_PARROT" "$OUTPUT" || { echo "marker missing from output (bash didn't run or wasn't reported)"; exit 1; }
grep -qE "● bash" "$OUTPUT" || { echo "expected bash tool call"; exit 1; }
exit 0
