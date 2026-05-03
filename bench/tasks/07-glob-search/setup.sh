#!/usr/bin/env bash
rm -f /tmp/notclaude-bench-glob-*.txt
for i in 1 2 3; do
  echo "file $i" > /tmp/notclaude-bench-glob-$i.txt
done
