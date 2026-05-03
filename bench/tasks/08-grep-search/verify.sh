#!/usr/bin/env bash
set -e
grep -qE "● grep_search" "$OUTPUT" || { echo "expected grep_search tool call"; exit 1; }
grep -q "needle.txt" "$OUTPUT" || { echo "expected needle.txt to be named in answer"; exit 1; }
exit 0
