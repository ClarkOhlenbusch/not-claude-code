#!/usr/bin/env bash
# Multi-step: file should be created AND the marker should appear in the model's
# reported bash output.
set -e
[ -f /tmp/notclaude-bench-09.py ] || { echo "python file was not created"; exit 1; }
grep -q "BENCH_MARKER_09_OWL" /tmp/notclaude-bench-09.py || { echo "python file missing marker"; exit 1; }
grep -q "BENCH_MARKER_09_OWL" "$OUTPUT" || { echo "model didn't run the file or didn't surface its output"; exit 1; }
grep -qE "● write_file" "$OUTPUT" || { echo "expected write_file tool call"; exit 1; }
grep -qE "● bash" "$OUTPUT" || { echo "expected bash tool call"; exit 1; }
exit 0
