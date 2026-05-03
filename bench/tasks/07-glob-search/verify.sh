#!/usr/bin/env bash
set -e
grep -qE "● glob_search" "$OUTPUT" || { echo "expected glob_search tool call"; exit 1; }
# Either "3" or "three" should appear somewhere in the answer.
grep -qiE "(^| |\")3( |\.|\")|three" "$OUTPUT" || { echo "expected count of 3 in answer"; exit 1; }
exit 0
