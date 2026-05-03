#!/usr/bin/env bash
set -e
content="$(cat /tmp/notclaude-bench-05.txt)"
[ "$content" = "the word here is after" ] || { echo "expected exactly 'the word here is after', got: $content"; exit 1; }
grep -qE "● edit_file" "$OUTPUT" || { echo "expected edit_file tool call"; exit 1; }
exit 0
