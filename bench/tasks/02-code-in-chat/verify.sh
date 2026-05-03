#!/usr/bin/env bash
# Should output Python code in chat. Should NOT call write_file (we said no file).
set -e
grep -qE "def +(fib|fibonacci)" "$OUTPUT" || { echo "expected def fib/fibonacci in output"; exit 1; }
if grep -q "● write_file" "$OUTPUT"; then
  echo "model wrote a file when prompt said not to"
  exit 1
fi
exit 0
