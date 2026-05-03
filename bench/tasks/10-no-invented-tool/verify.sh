#!/usr/bin/env bash
# Regression test: the model should NOT invent a tool name (e.g. SendUserMessage,
# ToolWebSearch, FunctionCall) for a plain chat reply. Should also not call any
# real tool because we explicitly said not to.
set -e
if grep -qE "● [A-Za-z_-]+" "$OUTPUT"; then
  bad=$(grep -oE "● [A-Za-z_-]+" "$OUTPUT" | head -1)
  echo "expected zero tool calls, got: $bad"
  exit 1
fi
# Sanity: the answer should contain the word "moon" or "lunar".
grep -qiE "(moon|lunar)" "$OUTPUT" || { echo "answer didn't mention the moon"; exit 1; }
exit 0
