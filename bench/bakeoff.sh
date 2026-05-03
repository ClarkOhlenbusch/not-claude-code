#!/usr/bin/env bash
# bench/bakeoff.sh — head-to-head benchmark across models.
#
# Runs the full bench/run.sh suite against each model on the list and
# emits a markdown report with pass-rate, mean wall time per task, and
# total wall time. The decision criterion (per the orchestrator plan):
# the swarm earns its keep iff pass-rate ≥ baseline AND wall-time-per-task
# ≤ 1.8× baseline. Anything else means the role-swap cost wins.
#
# Usage:
#   bench/bakeoff.sh                                 # all default models
#   bench/bakeoff.sh swarm qwen2.5-coder:14b         # specific subset

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RESULTS_DIR="${BENCH_RESULTS_DIR:-/tmp/notclaude-bench/bakeoff}"
mkdir -p "$RESULTS_DIR"

DATE=$(date +%Y%m%d-%H%M%S)
REPORT="$RESULTS_DIR/bakeoff-$DATE.md"

# Default candidate set. Override by passing model names on the command line.
DEFAULT_MODELS=(
  "swarm"
  "qwen2.5-coder:14b"
  "qwen2.5-coder:7b"
)

if [ $# -gt 0 ]; then
  MODELS=("$@")
else
  MODELS=("${DEFAULT_MODELS[@]}")
fi

echo "# Bakeoff $DATE" > "$REPORT"
echo "" >> "$REPORT"
echo "| Model | Passed / Total | Total time | Mean / task |" >> "$REPORT"
echo "|---|---|---|---|" >> "$REPORT"

for model in "${MODELS[@]}"; do
  echo
  echo "════════════════════════════════════════════════════════════════"
  echo "  Running bench against:  $model"
  echo "════════════════════════════════════════════════════════════════"
  start=$(python3 -c 'import time; print(int(time.time()*1000))')
  set +e
  output=$(BENCH_RESULTS_DIR="$RESULTS_DIR/$model" "$REPO_ROOT/bench/run.sh" --model "$model" --timeout 240 2>&1)
  exit_code=$?
  set -e
  end=$(python3 -c 'import time; print(int(time.time()*1000))')
  elapsed_s=$(python3 -c "print(f'{($end - $start)/1000:.1f}')")
  echo "$output"

  passed_line=$(echo "$output" | grep -E "passed:")
  passed=$(echo "$passed_line" | awk '{print $2}')
  total=$(echo "$passed_line" | awk '{print $4}')
  if [ -n "${total:-}" ] && [ "$total" -gt 0 ]; then
    mean=$(python3 -c "print(f'{$elapsed_s/$total:.1f}')")
  else
    mean="n/a"
  fi
  echo "| \`$model\` | $passed / $total | ${elapsed_s}s | ${mean}s |" >> "$REPORT"
done

echo
echo "──────────────────────────────────────────────────────────────"
echo "report: $REPORT"
echo "──────────────────────────────────────────────────────────────"
echo
cat "$REPORT"
