#!/usr/bin/env bash
# Should answer "45" without using any tool.
set -e
grep -q "45" "$OUTPUT" || { echo "expected '45' in output"; exit 1; }
# A bare chat answer should not trigger any tool dispatch.
if grep -qE "● (bash|read_file|write_file|edit_file|glob_search|grep_search|WebFetch|WebSearch)" "$OUTPUT"; then
  echo "unexpected tool call for a plain math question"
  exit 1
fi
exit 0
