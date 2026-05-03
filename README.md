# NOT Claude Code

A local-first coding agent that looks and feels like Claude Code, but runs entirely on local models via Ollama. The harness is a Rust clean-room reimplementation; the model is whatever you have pulled. Default lineup uses Qwen2.5-Coder-14B (~9 GB).

**Project thesis (longer term):** specialization + verification beats one bigger local model on coding tasks — a swarm of role-specialized small models (router, planner, patcher, reviewer) coordinated by deterministic repo maps, git worktrees, structured outputs, and test-loop verification. Kill criterion: must beat DeepSeek-Coder-V2-Lite (16B MoE) running alone, on the same hardware and task suite.

**Current phase:** single-model baseline working. The orchestrator is a future enhancement, not a foundation requirement — see "What we learned" below.

## Status

| | |
|---|---|
| Build | green (`cargo build --release`, ~10s after first compile) |
| `notclaude --help`, `--version` | works, NOT Claude Code branded |
| Chat against a local model (any size) | works |
| Single tool call (write/read/edit) at 14B | works reliably |
| Multi-step agentic turn at 14B | works (writes file, runs bash, recovers from errors, reports result) |
| TUI feel | mimics Claude Code: orange `●` tool bullets, `⎿` continuations, `✻ Thinking…` spinner, silent finish |
| Multi-model orchestrator | not started — gated on benchmark first |
| Benchmark harness | not started |

## Quick start

### One-shot install (macOS)

```bash
git clone https://github.com/ClarkOhlenbusch/not-claude-code
cd not-claude-code
./scripts/install.sh
```

The installer is idempotent and prompts before any heavy step (Homebrew install, 9 GB model pull). It checks/installs Xcode CLT, Homebrew, Rust, and Ollama; starts the Ollama daemon; builds the release binary; symlinks `notclaude` into `~/.local/bin`; and pulls the default model.

### Manual install

Prereqs: Rust 1.90+, Ollama (`brew install ollama`), ~10 GB free disk.

```bash
git clone https://github.com/ClarkOhlenbusch/not-claude-code
cd not-claude-code

# Build optimized binary (~16s first time, ~3s incremental)
(cd rust && cargo build --release)

# Install the wrapper command (creates ~/.local/bin/notclaude -> scripts/notclaude)
mkdir -p ~/.local/bin && ln -sf "$PWD/scripts/notclaude" ~/.local/bin/notclaude

# Pull the recommended model (~9 GB, one-time)
ollama pull qwen2.5-coder:14b

# Use it (Ollama auto-starts if not already running)
notclaude "say hi"                     # one-shot prompt
notclaude                              # interactive REPL
notclaude --model qwen2.5-coder:7b "…" # smaller / faster (less reliable for agentic tasks)
```

Make sure `~/.local/bin` is on your `$PATH`. The wrapper sets `OPENAI_API_KEY=ollama`, `OPENAI_BASE_URL=http://localhost:11434/v1`, and defaults `--model` to `swarm`.

### Coinflip launcher

Install the dedicated coinflip entry point when you want an explicit command for the project orchestration logic:

```bash
./scripts/install-notclaude-coinflip
```

If `COMPUTE_COMMUNITY_API_KEY`, `COMPUTECOMMUNITY_API_KEY`, or `CC_API_KEY` is already in the environment, the installer also writes the local ignored compute config. Otherwise provision it explicitly once:

```bash
provision-notclaude-coinflip --key cc_your_api_key
```

`notclaude-coinflip` uses the existing swarm path in the Rust CLI: GPT-5.5 distills and evaluates, then local/remote worker models attempt the task. The launcher configures Ollama for worker models while leaving your real `OPENAI_API_KEY` available for the orchestrator. Tune the lineup with env vars:

```bash
notclaude-coinflip
notclaude-coinflip run "inspect this repo and propose the next benchmark"
NOTCLAUDE_COINFLIP_MODEL=swarm notclaude-coinflip
```

`notclaude-coinflip` does not run interactive setup during startup. Compute credentials must already be provisioned through the environment or `~/.notclaude/coinflip.env`, then normal usage is just:

```bash
notclaude-coinflip
```

The default coinflip lineup is GPT-5.5 as orchestrator and `runpod-qwen36` as the compute worker. If compute credentials are missing, the launcher fails clearly instead of silently using GPT-only. To intentionally bypass compute and run GPT-only fallback:

```bash
NOTCLAUDE_COINFLIP_ALLOW_GPT_FALLBACK=1 notclaude-coinflip
```

### Remote Ollama compute

Point the launcher at another machine running Ollama with `NOTCLAUDE_OLLAMA_URL`. The CLI still behaves the same locally; only model inference moves to that endpoint.

```bash
export NOTCLAUDE_OLLAMA_URL="http://other-compute-host:11434"
notclaude --model qwen-coder
notclaude --model qwen3-coder:30b "summarize this repo"
```

`qwen-coder` is an alias for `qwen3-coder:30b`. You can set `NOTCLAUDE_DEFAULT_MODEL=qwen-coder` if you want the remote Qwen model to be the default.

### Runpod Qwen3.6 35B

The Compute Community Runpod model is wired as an OpenAI-compatible endpoint:

```bash
notclaude-coinflip
notclaude --model qwen36
notclaude --model qwen3.6 "summarize this repo"
```

Aliases `qwen36`, `qwen3.6`, and `runpod-qwen36` resolve to `Qwen/Qwen3.6-35B-A3B-FP8` at `https://computecommunity.com/u/C7XfWXayLelTkySS7to8stLtwvV3Lj3J/nodes/runpod-qwen3-5-35b/v1`. `notclaude-coinflip` reads `~/.notclaude/coinflip.env` automatically, and you can still use `COMPUTE_COMMUNITY_API_KEY`, `COMPUTECOMMUNITY_API_KEY`, or `CC_API_KEY` if you want to override the saved key for a single shell.

## Scriptable command-line runs

Use `notclaude run` when you want this agent in a shell script, Makefile, CI-ish local loop, or editor command. It builds the same workspace-aware runtime as the REPL, so project instructions, `.notclaude` config, tools, permissions, and the current working directory all stay connected.

```bash
notclaude run "summarize the current diff and suggest tests"
notclaude run --cwd /path/to/repo "fix the failing unit tests"
git diff --stat | notclaude run "turn this into a PR summary"
printf '%s\n' "inspect this repo and make the smallest safe fix" | notclaude run --cwd "$PWD"
notclaude --output-format json run "list the tool calls you used"
```

`run` reads the prompt from arguments, from `-`, or from piped stdin when no prompt is provided. `--cwd` changes into the target project before runtime setup, which is the important bit when calling it from outside the repo.

## What works at each model size

We have actual evidence here, not vibes. Same three test prompts across model sizes:

| Test | 7B | 14B |
|---|---|---|
| `"say hi"` (chat) | ✓ | ✓ |
| `"create /tmp/x.html with …"` (single tool call) | inconsistent — sometimes calls tool, sometimes asks permission, sometimes generates code in chat | ✓ reliably |
| `"edit /tmp/x.html — add …"` (edit at correct position) | inconsistent — occasionally invalid HTML | ✓ correct position |
| `"write a python function, save to /tmp/y.py, run it"` (multi-step write+run) | sometimes refuses tools entirely; when it tries, sequencing flaky | ✓ writes file, runs it, recovers from errors |

**Recommendation:** use 14B as default. 7B is too small for consistent agentic decision-making in this harness.

## What we learned (system prompt was the missing piece)

The harness was holding back the model as much as the model was. Initial 7B tests had it refusing to use tools at all ("I can't access files on your system, you do it manually"). Reading Claude Code's leaked source ([codeaashu/claude-code](https://github.com/codeaashu/claude-code)) revealed claw was missing the entire **"Using your tools"** section of Claude Code's system prompt — the part that explicitly tells the model:

- Use `read_file` (not cat/head/tail/sed) for reads
- Use `edit_file` (not sed/awk) for edits
- Use `write_file` (not heredoc) for creation
- Use `glob_search`/`grep_search` (not find/grep) for searches
- Reserve bash for genuine system commands
- That tools have permissions and the model SHOULD call them, not narrate intent
- Multiple parallel tool calls are OK when independent

Porting that section into `crates/runtime/src/prompt.rs` (commit `6edf819`) was the single biggest behavior-change of the project so far. Same model, same harness, same prompts — went from "I can't do that" to actually doing the multi-step task.

Lesson: every gap between claw's prompt scaffolding and Claude Code's makes the local model behave more like an inert API.

## How the routing works

`claw` ships with three providers in `crates/api/src/providers/`: `claw_provider` (Anthropic), `openai_compat` (OpenAI / xAI / anything that speaks OpenAI chat-completions). The CLI calls `ProviderClient::from_model(&model)` which picks a provider by:

1. If model name matches `MODEL_REGISTRY` (claude-*, grok-*, opus, sonnet, haiku) → that provider's metadata.
2. Else if `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` / saved Claw OAuth available → `ClawApi`.
3. Else if `OPENAI_API_KEY` set → `OpenAi`.
4. Else if `XAI_API_KEY` set → `Xai`.
5. Default → `ClawApi` (errors on missing creds).

Ollama exposes an OpenAI-compatible API at `localhost:11434/v1`, so any model name not in the registry (e.g. `qwen2.5-coder:14b`) routes through the OpenAI provider when `OPENAI_API_KEY` is set.

## Tool-call synthesis layer

Ollama's OpenAI-compat layer (with Qwen models) doesn't always emit structured `tool_calls` — it sometimes emits the tool call as JSON in the text content. The OpenAI provider's stream parser detects this and synthesizes proper `ToolUse` events. Handles:

- Text JSON: `{"name": "read_file", "arguments": {...}}`
- Markdown-fenced JSON: ` ```json {…} ``` `
- Smart-quoted JSON (Qwen Unicode quirk): `{"name": …}` → normalized to ASCII before parse
- Preamble text: model says "Sure, here is: {…}" — the JSON is detected mid-stream and split out
- **Multiple concatenated tool calls**: `{tool1}{tool2}` — each balanced top-level `{}` block is parsed independently, each emits its own ToolUse event

See `crates/api/src/providers/openai_compat.rs` `handle_text_delta` and `extract_balanced_json_blocks`.

## Hardware target

Practical model lineup:

| Model | Footprint | Use |
|---|---|---|
| `qwen36` / `runpod-qwen36` | remote | preferred agentic default when a compute key is available |
| `gemma4:e2b` | ~7 GB | local fallback for Intel Macs and quick offline checks |
| `qwen2.5-coder:14b` | ~9 GB | too slow for Intel Macs; use only on faster Apple Silicon or remote Ollama |
| `nomic-embed-text:v1.5` | ~250 MB | future repo retrieval |

On Intel Macs, avoid 14B local models for interactive use. Prefer Compute Qwen 3.6, or keep local testing to `gemma4:e2b`.

## Future architecture (the project's thesis)

```
User prompt
   ↓
[Router 3B] ── classify: question | single-edit | multi-step
   ↓
[Planner 14B] ── emit step list + target files (structured JSON)
   ↓
For each step:
   [Mapper, deterministic] ── tree-sitter repo map + embedding retrieval
   [Patch writer 14B] ── structured diff output
   [Verifier, deterministic] ── apply in worktree, run tests + typecheck + lint
   ├── FAIL → [Critic 7B] ── explain failure → retry ≤2x or replan
   └── PASS → [Reviewer 7B] ── does diff match intent?
   [Compressor 7B] ── summarize step → rolling context
```

This is **future work, not foundation**. We confirmed (above) that single-model 14B handles real agentic tasks. The orchestrator earns its keep by being faster (smaller models for routing/judging), more specialized (deterministic verification, planner that knows tools), and ideally better than 14B on hard tasks. We need a benchmark before building it.

Surgery site for the orchestrator: `crates/claw-cli/src/main.rs` `DefaultRuntimeClient::stream` — currently calls `self.client.stream_message(&request)` against a single `ProviderClient`. The orchestrator becomes a new `ProviderClient` variant that dispatches across multiple Ollama calls per turn.

## Known issues

**Tool-call sequencing at smaller model sizes.** When a single response includes multiple tool calls (e.g. write file + run it), small models occasionally emit them in the wrong order. The harness dispatches in emission order. 14B is reliable; 7B sometimes fails. Mitigations: prompt the user to break into single steps, or implement the orchestrator's planner role.

**Default bash timeout is 10 ms.** This is from the upstream Rust harness, almost certainly a typo (probably meant 10 seconds). Even 14B retries when it hits this, but it adds a turn. Should bump to a sane default.

## Roadmap

- [x] Build harness; route through Ollama; pass smoke test
- [x] Synthesize structured tool_use events from text-JSON model output (with smart-quote, code-fence, preamble-text, multi-call handling)
- [x] Port "Using your tools" system prompt section from Claude Code source
- [x] Visual rebrand: NOT Claude Code (red NOT, orange Claude Code, ✻ spinner, ● tool bullets)
- [x] `notclaude` wrapper command for one-line invocation
- [x] Verify single-model 14B baseline handles real agentic tasks
- [ ] Fix the 10ms bash timeout
- [ ] Build a 5–10 task benchmark suite with deterministic pass/fail (the kill-criterion infrastructure)
- [ ] Implement `LocalSwarmClient` that dispatches router → planner → patcher → reviewer per turn
- [ ] Run head-to-head: swarm vs single-model 14B vs DeepSeek-Coder-V2-Lite on the benchmark
- [ ] Add tree-sitter repo map (port from Aider's algorithm)
- [ ] Wire git worktrees per task for safe rollback

## Provenance

Forked from [soongenwong/claudecode](https://github.com/soongenwong/claudecode) — an MIT-licensed Rust clean-room reimplementation of Claude Code (independent, no leaked source copied per the upstream `PARITY.md`). The upstream is an MVP scaffold of Claude Code, not feature-complete; that's fine, we're swapping the LLM layer regardless. We pruned the Python parity sketch and top-level tests to focus on the Rust workspace under `rust/`.

System-prompt content for the "Using your tools" section was ported from [codeaashu/claude-code](https://github.com/codeaashu/claude-code), the leaked Claude Code TypeScript source — that repo was used as a *reference* only, not a code base.

License: MIT (inherited from upstream). See `LICENSE`.
