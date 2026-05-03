#!/usr/bin/env bash
# Some models ignore the "do not save to any file" instruction and write
# a fibonacci.py to the repo root. Pre-clean any stragglers so this test
# can detect that behavior cleanly via verify.sh.
rm -f "$PWD/fibonacci.py" "$PWD/rust/fibonacci.py" "$PWD/fib.py"
