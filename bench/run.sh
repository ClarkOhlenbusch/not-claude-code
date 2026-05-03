#!/usr/bin/env bash
# bench/run.sh — runs every test under bench/tasks/ against `notclaude` and
# reports pass/fail. Each test is a directory with:
#   prompt.txt       (required)  the user message to send
#   setup.sh         (optional)  runs before the test, e.g. to create fixtures
#   verify.sh        (required)  exit 0 = pass, anything else = fail
#                                receives env vars: $OUTPUT (path to captured
#                                stdout) and $TASK_ID
#   description.txt  (optional)  shown in the report
#
# Usage:
#   bench/run.sh                            # default model
#   bench/run.sh --model qwen2.5-coder:7b   # specific model
#   bench/run.sh --only write-file          # filter by id substring
#   bench/run.sh --timeout 180              # per-test seconds (default 120)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TASKS_DIR="$REPO_ROOT/bench/tasks"
RESULTS_DIR="${BENCH_RESULTS_DIR:-/tmp/notclaude-bench}"
mkdir -p "$RESULTS_DIR"

MODEL="qwen2.5-coder:14b"
ONLY=""
PER_TEST_TIMEOUT=120

while [ $# -gt 0 ]; do
  case "$1" in
    --model) MODEL="$2"; shift 2 ;;
    --only) ONLY="$2"; shift 2 ;;
    --timeout) PER_TEST_TIMEOUT="$2"; shift 2 ;;
    -h|--help)
      head -19 "$0" | tail -18 | sed 's/^# //;s/^#//'
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if ! command -v notclaude >/dev/null 2>&1; then
  echo "notclaude not on PATH. Install via:" >&2
  echo "  ln -sf \"$REPO_ROOT/scripts/notclaude\" ~/.local/bin/notclaude" >&2
  exit 2
fi

# Strip ANSI sequences so verify scripts can grep cleanly.
strip_ansi() {
  sed $'s/\x1b\\[[0-9;]*[a-zA-Z]//g; s/\x1b\\[[?][0-9;]*[hl]//g; s/\x1b[78]//g'
}

results=()
total=0
passed=0

# Sort tasks by directory name so order is stable.
for task_dir in $(ls -d "$TASKS_DIR"/*/ 2>/dev/null | sort); do
  task_id="$(basename "$task_dir")"
  if [ -n "$ONLY" ] && ! echo "$task_id" | grep -q "$ONLY"; then
    continue
  fi
  if [ ! -f "$task_dir/prompt.txt" ]; then
    echo "[skip] $task_id — no prompt.txt"
    continue
  fi
  if [ ! -f "$task_dir/verify.sh" ]; then
    echo "[skip] $task_id — no verify.sh"
    continue
  fi

  total=$((total + 1))
  prompt="$(cat "$task_dir/prompt.txt")"
  out_file="$RESULTS_DIR/$task_id.out"
  err_file="$RESULTS_DIR/$task_id.err"

  printf "  %-32s " "$task_id"

  # Setup
  if [ -x "$task_dir/setup.sh" ] || [ -f "$task_dir/setup.sh" ]; then
    bash "$task_dir/setup.sh" >/dev/null 2>&1 || true
  fi

  # Run notclaude with the prompt; cap each test
  start_ns=$(python3 -c 'import time; print(int(time.time()*1e9))')
  set +e
  timeout "$PER_TEST_TIMEOUT" notclaude --model "$MODEL" "$prompt" > "$out_file" 2> "$err_file"
  exit_code=$?
  set -e
  end_ns=$(python3 -c 'import time; print(int(time.time()*1e9))')
  elapsed_s=$(python3 -c "print(f'{($end_ns - $start_ns)/1e9:.1f}')")

  # Strip ANSI from captured output for the verify script's convenience.
  strip_ansi < "$out_file" > "$out_file.plain"

  # Verify
  if [ "$exit_code" -eq 124 ]; then
    status="TIMEOUT"
    reason="hit ${PER_TEST_TIMEOUT}s wall clock"
  elif [ "$exit_code" -ne 0 ]; then
    status="CRASH"
    reason="notclaude exit $exit_code"
  else
    set +e
    OUTPUT="$out_file.plain" TASK_ID="$task_id" \
      bash "$task_dir/verify.sh" > "$RESULTS_DIR/$task_id.verify" 2>&1
    verify_code=$?
    set -e
    if [ "$verify_code" -eq 0 ]; then
      status="PASS"
      reason=""
      passed=$((passed + 1))
    else
      status="FAIL"
      reason="$(head -1 "$RESULTS_DIR/$task_id.verify" 2>/dev/null || echo "verify exited $verify_code")"
    fi
  fi

  printf "%-8s %5ss" "$status" "$elapsed_s"
  if [ -n "$reason" ]; then
    printf "  %s" "$reason"
  fi
  printf "\n"
  results+=("$task_id|$status|$elapsed_s|$reason")
done

echo
echo "──────────────────────────────────────────────────────────────"
echo "model: $MODEL"
echo "passed: $passed / $total"
echo "outputs: $RESULTS_DIR"
echo "──────────────────────────────────────────────────────────────"

# Markdown table for README pasting.
{
  echo
  echo "| Task | Status | Time |"
  echo "|---|---|---|"
  for r in "${results[@]}"; do
    IFS='|' read -r id st time _ <<< "$r"
    echo "| $id | $st | ${time}s |"
  done
} > "$RESULTS_DIR/report.md"
echo "markdown report: $RESULTS_DIR/report.md"

# Exit non-zero if any failed.
[ "$passed" -eq "$total" ]
