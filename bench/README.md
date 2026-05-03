# Bench

Behavioral test suite for `notclaude`. Each test runs the binary end-to-end against a real local model (via Ollama) and verifies an observable side-effect — file content, output text, tool dispatch — instead of asserting against an exact string.

The point isn't language-model accuracy benchmarking. The point is **regression detection**: did our last change break tool dispatch? Did the new model still know how to use `edit_file`? Did the system prompt change make the model start inventing tool names again?

## Quick start

```bash
# Default model (qwen2.5-coder:14b), all tasks, 120s per test
bench/run.sh

# Specific model
bench/run.sh --model qwen2.5-coder:7b

# Single task by id substring
bench/run.sh --only multi-step

# Custom per-test timeout
bench/run.sh --timeout 180
```

Output:
- One line per task: `task-id   STATUS   <wall-clock>   <reason if not PASS>`
- Summary: `passed: X / Y`
- Per-test outputs in `/tmp/notclaude-bench/<task-id>.{out,plain,err,verify}`
- Markdown report at `/tmp/notclaude-bench/report.md`

## Adding a test

Each test is a directory under `tasks/`:

```
tasks/12-my-new-test/
├── prompt.txt        # required — what the user types
├── setup.sh          # optional — runs before the test (fixtures, cleanup)
└── verify.sh         # required — exit 0 = pass
```

`verify.sh` receives env vars:
- `OUTPUT` — path to the captured `notclaude` stdout, ANSI-stripped
- `TASK_ID` — the directory name

Conventions:
- Use a unique `BENCH_MARKER_<id>_<word>` string so verify scripts can grep precisely without matching unrelated output.
- Put fixtures under `/tmp/notclaude-bench-<id>-...` so cleanup is obvious.
- Test ONE behavior per task. If you want to test "write a file" and "run the file", that's two tests OR a multi-step test that explicitly tests the multi-step capability.

## Current task list

| ID | Tests |
|---|---|
| 01-plain-chat | Math question → answer in chat, no tool dispatch |
| 02-code-in-chat | Code request with explicit "no file" → code in chat, no `write_file` |
| 03-write-file | Create a file with a marker → file exists with marker, `write_file` was called |
| 04-read-file | Read a file with a known marker → marker appears in answer, `read_file` was called |
| 05-edit-file | String replacement → file content matches expected, `edit_file` was called |
| 06-bash-exec | Run shell command → marker in output, `bash` was called |
| 07-glob-search | Find files by pattern → count of 3, `glob_search` was called |
| 08-grep-search | Find which file contains a string → correct filename in answer, `grep_search` was called |
| 09-multi-step | Write a Python file then run it → both file exists with marker AND output marker shown |
| 10-no-invented-tool | Plain chat with explicit no-tool instruction → zero tool calls of any name |

## Baseline: qwen2.5-coder:14b

```
  01-plain-chat                    PASS       0.3s
  02-code-in-chat                  PASS       4.4s
  03-write-file                    PASS       4.9s
  04-read-file                     PASS       3.9s
  05-edit-file                     PASS       5.9s
  06-bash-exec                     PASS       7.4s
  07-glob-search                   PASS       9.2s
  08-grep-search                   PASS       5.7s
  09-multi-step                    PASS      15.0s
  10-no-invented-tool              PASS       3.4s
  passed: 10 / 10
```

Run on Qwen2.5-Coder-14B-Instruct-Q4 via Ollama, M4 Pro 24GB. ~60s wall total.

## What this suite intentionally doesn't test

- **Subjective quality of output** — we don't grade prose or code style. We only check observable side-effects.
- **Streaming vs non-streaming** — the harness streams; we check the final captured output.
- **Permission prompts** — tests run in `--permission-mode danger-full-access` (the wrapper default). Permission UX needs separate testing.
- **REPL multi-turn** — each test is one-shot. Multi-turn behavior would need a different harness.
- **The TUI rendering** — the bordered banner, input frame, etc. We test what the model does, not what the screen looks like.

## What to do when a test fails

1. Look at `/tmp/notclaude-bench/<task-id>.plain` for the model's output (ANSI stripped).
2. Look at `/tmp/notclaude-bench/<task-id>.verify` for what the verify script complained about.
3. Re-run that single test: `bench/run.sh --only <task-id>`.
4. If it's a model regression: bisect against the last passing commit; the offending change is usually in `crates/runtime/src/prompt.rs` or `crates/tools/src/lib.rs`.
5. If it's flake (the same test passes sometimes): tighten the prompt to be less ambiguous, or relax the verify if the success criterion was too strict.
