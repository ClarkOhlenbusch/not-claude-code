# NOT Claude Code

Local-first coding agent. Replaces the single cloud-LLM call with a swarm of role-specialized small local models (router, planner, patcher, reviewer) coordinated by deterministic repo maps, git worktrees, structured outputs, and test-loop verification.

**Thesis:** specialization + verification beats one bigger local model on coding tasks. Kill criterion: must beat DeepSeek-Coder-V2-Lite (16B MoE) running alone, on the same hardware and task suite.

## Status

Phase 0 — base harness routing local models end-to-end via Ollama.

| | |
|---|---|
| Build | green (`cargo build`) |
| `claw --help` | works |
| `claw "prompt"` against a local Ollama model | works |
| Tool dispatch from local models | works (text-JSON synthesis layer in OpenAI provider) |
| Single-model agent loop converges reliably | no — small models pick wrong tools, loop without terminating |
| Multi-model orchestrator | not started |
| Benchmark harness | not started |

## Quick start

Prereqs: Rust 1.90+, Ollama (`brew install ollama`), ~5 GB disk for the model.

```bash
git clone https://github.com/ClarkOhlenbusch/not-claude-code
cd not-claude-code

# Build optimized binary
(cd rust && cargo build --release)

# Install the wrapper command (creates ~/.local/bin/notclaude → scripts/notclaude)
mkdir -p ~/.local/bin && ln -sf "$PWD/scripts/notclaude" ~/.local/bin/notclaude

# Pull the default model (~4.7 GB, one-time)
ollama pull qwen2.5-coder:7b

# Use it (Ollama auto-starts if not already running)
notclaude "say hi"
notclaude                              # interactive REPL
notclaude --model some-other-model "…" # override default
```

Make sure `~/.local/bin` is on your `$PATH` (it usually is on macOS).

The `notclaude` wrapper sets `OPENAI_API_KEY=ollama`, `OPENAI_BASE_URL=http://localhost:11434/v1`, and defaults `--model` to `qwen2.5-coder:7b`. Equivalent without it:

```bash
OPENAI_API_KEY=ollama OPENAI_BASE_URL=http://localhost:11434/v1 \
  ./rust/target/release/claw --model qwen2.5-coder:7b "say hi"
```

## How the routing works

`claw` ships with three providers in `crates/api/src/providers/`: `claw_provider` (Anthropic), `openai_compat` (OpenAI / xAI / anything that speaks OpenAI chat-completions). The CLI calls `ProviderClient::from_model(&model)` which picks a provider by:

1. If model name matches `MODEL_REGISTRY` (claude-*, grok-*, opus, sonnet, haiku) → that provider's metadata.
2. Else if `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` / saved Claw OAuth available → `ClawApi`.
3. Else if `OPENAI_API_KEY` set → `OpenAi`.
4. Else if `XAI_API_KEY` set → `Xai`.
5. Default → `ClawApi` (will then error on missing creds).

Ollama exposes an OpenAI-compatible API at `localhost:11434/v1`, so any model name not in the registry (e.g. `qwen2.5-coder:7b`) routes through the OpenAI provider when `OPENAI_API_KEY` is set. The key value is irrelevant to Ollama but the provider requires it non-empty — set anything (`ollama` is conventional).

## Hardware target

Reference: MacBook Pro M4 Pro, 24 GB unified memory. Practical model lineup at Q4 quantization:

| Role | Model | Footprint |
|---|---|---|
| Router (intent classification) | Qwen2.5-3B-Instruct | ~2 GB |
| Planner + Patcher | Qwen2.5-Coder-14B-Instruct | ~9 GB |
| Reviewer + Critic | Qwen2.5-Coder-7B-Instruct | ~4.5 GB |
| Embeddings (repo retrieval) | nomic-embed-text-v1.5 | ~250 MB |
| Single-model baseline (A/B) | DeepSeek-Coder-V2-Lite-Instruct (16B MoE, 2.4B active) | ~10 GB |

~16 GB hot, fits with ~7 GB OS overhead. Memory bandwidth ~273 GB/s is shared between concurrent inference — keep models loaded but inference 1–2 at a time.

## Architecture (planned)

```
User prompt
   ↓
[Router 3B] ── classify: question | single-edit | multi-step
   ↓
[Planner 14B] ── emit step list + target files (structured JSON)
   ↓
For each step:
   [Mapper, deterministic] ── tree-sitter repo map + embedding retrieval
   [Patch writer 14B] ── structured diff output (search/replace blocks)
   [Verifier, deterministic] ── apply in worktree, run tests + typecheck + lint
   ├── FAIL → [Critic 7B] ── explain failure → retry ≤2x or replan
   └── PASS → [Reviewer 7B] ── does diff match intent?
   [Compressor 7B] ── summarize step → rolling context
```

Surgery site: `crates/claw-cli/src/main.rs` `DefaultRuntimeClient::stream` (~line 3088). Currently calls `self.client.stream_message(&request)` against a single `ProviderClient`. The orchestrator will be a new variant of `ProviderClient` (or a parallel construct) that dispatches across multiple Ollama models per role.

## Known issues

**Single-model agent loops don't converge.** Qwen-7B picks the wrong tool for a given task (e.g., chooses `SendUserMessage` to "answer" a "read this file" prompt) and re-emits the same tool call repeatedly without terminating. This is exactly what the multi-model orchestrator is designed to solve — a small router decides "this is a single-step task, stop after one tool call," and a bigger planner picks tools deliberately. Workaround for now: keep prompts conversational rather than agentic; don't ask single-model claw to do multi-step coding.

**Tool-call format synthesis is heuristic.** The OpenAI provider's stream parser detects text whose first non-whitespace char is `{` or ` ``` ` and tries to parse it as `{"name": ..., "arguments": ...}`. Works for Qwen2.5-Coder's typical output but won't catch every shape (e.g., a tool call preceded by chatty text like "Sure, I'll read that file: {json}" — the `{` isn't first). If the model's intent is ambiguous, set `OLLAMA_*` env vars to test other models. Long-term fix is GBNF grammar enforcement via llama.cpp server — slower path, cleaner result.

## Roadmap

- [x] Build harness; route through Ollama; pass smoke test
- [x] Synthesize structured tool_use events from text-JSON model output
- [ ] Implement `LocalSwarmClient` that dispatches router → planner → patcher → reviewer per turn
- [ ] Add tree-sitter repo map (port from Aider's algorithm)
- [ ] Wire git worktrees per task for safe rollback
- [ ] Build a 5–10 task benchmark suite with deterministic pass/fail
- [ ] Run head-to-head: swarm vs DeepSeek-Coder-V2-Lite alone

## Provenance

Forked from [soongenwong/claudecode](https://github.com/soongenwong/claudecode) — an MIT-licensed Rust clean-room reimplementation of Claude Code (independent, no leaked source copied per the upstream `PARITY.md`). The upstream is an MVP scaffold of Claude Code, not feature-complete; that's fine, we're swapping the LLM layer regardless. We pruned the Python parity sketch and top-level tests to focus on the Rust workspace under `rust/`.

License: MIT (inherited from upstream). See `LICENSE`.
