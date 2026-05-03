#!/usr/bin/env bash
set -e
grep -q "PINEAPPLE" "$OUTPUT" || { echo "model didn't surface the marker word"; exit 1; }
grep -q "● read_file" "$OUTPUT" || { echo "expected read_file tool call in output"; exit 1; }
exit 0
