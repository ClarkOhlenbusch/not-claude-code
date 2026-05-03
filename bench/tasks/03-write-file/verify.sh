#!/usr/bin/env bash
set -e
[ -f /tmp/notclaude-bench-03.txt ] || { echo "file was not created"; exit 1; }
grep -q "BENCH_MARKER_03_HELLO" /tmp/notclaude-bench-03.txt || { echo "marker missing from file"; exit 1; }
grep -q "● write_file" "$OUTPUT" || { echo "expected write_file tool call in output"; exit 1; }
exit 0
