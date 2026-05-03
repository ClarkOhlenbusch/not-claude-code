#!/usr/bin/env bash
rm -rf /tmp/notclaude-bench-grep
mkdir -p /tmp/notclaude-bench-grep
echo "nothing to see" > /tmp/notclaude-bench-grep/a.txt
echo "BENCH_MARKER_GREP is here" > /tmp/notclaude-bench-grep/needle.txt
echo "also nothing" > /tmp/notclaude-bench-grep/c.txt
