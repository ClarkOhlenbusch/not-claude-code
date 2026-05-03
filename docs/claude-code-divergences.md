# NOT Claude Code vs Claude Code — Divergence Audit

A running record of places where this fork diverges from Claude Code (as observed in the leaked source at [codeaashu/claude-code](https://github.com/codeaashu/claude-code)), why it matters for local-model behavior, and what we've done about it.

This file is meant to be added to over time. Status legend:
- ✅ fixed — the divergence is closed
- 🔧 open — known divergence, not yet addressed
- ➖ won't-fix — divergence is intentional or the cost outweighs the benefit

---

## System prompt sections

### ✅ "Using your tools" section was missing entirely
**Claude Code:** `getUsingYourToolsSection` in `src/constants/prompts.ts` — lists every dedicated tool ("To read files use FileRead instead of cat...") and tells the model when bash is NOT the right answer.
**Original claw:** no such section. Tools were registered but the system prompt never told the model what each one was for.
**Symptom:** small models refused to use tools ("I can't access files on your system, you do it manually") or fell back to bash for everything.
**Fix:** ported the section into `crates/runtime/src/prompt.rs` `get_using_your_tools_section`. Single biggest behavior change of the project so far.

### ✅ Valid tool-name list wasn't pinned
**Claude Code:** the model is also constrained by Anthropic's own training to use only the tool names declared in the request.
**Original claw:** with smaller models, Qwen sometimes invented tool names like `SendUserMessage`, `ToolWebSearch`, `FunctionFormatAsAnswerWebSearch`. Some of these even matched stale tools registered in claw's registry (see `SendUserMessage` below).
**Fix:** added an explicit "the exact set of tool names is X, Y, Z; do not invent others; emit plain text if you don't need a tool" line at the top of the "Using your tools" section.

### 🔧 Doing tasks section is much shorter than Claude's
**Claude Code:** ~10 multi-sentence bullets covering code style, error handling, abstraction discipline, security, false-claims mitigation.
**Claw:** 6 short bullets, less specific.
Acceptable for now but worth porting more if we hit related bad behaviors.

### 🔧 No "Tone and style" section
**Claude Code:** explicit guidance on response length, when to use markdown, when to summarize.
**Claw:** no equivalent.
Likely contributes to small models being verbose.

### 🔧 No "Doing UI/UX work" section
**Claude Code:** has guidance about testing UI changes in a browser.
Out of scope for this hackathon project.

---

## Tool descriptions

### ✅ Bash tool description was a 7-word stub
**Claude Code:** multi-paragraph description with timeout defaults/limits, background-task notes, "use dedicated tools instead of grep/cat/sed" guidance, commit-and-PR workflow.
**Original claw:** `"Execute a shell command in the current workspace."` — that's it.
**Fix:** ported a tightened version of Claude's prose into the `bash` ToolSpec. Includes the dedicated-tool routing list, timeout unit clarification, background task note.

### ✅ Bash `timeout` parameter unit was undocumented
**Claude Code:** `getDefaultBashTimeoutMs` / `getMaxBashTimeoutMs` constants, schema description says "milliseconds".
**Original claw:** schema said `{"type": "integer", "minimum": 1}` with no unit and no default.
**Symptom:** small models sent `timeout: 10` (assuming seconds), got 10 *milliseconds* of execution, then "Command exceeded timeout of 10 ms".
**Fix:** schema description now reads "Timeout in MILLISECONDS. Defaults to 120000 (2 minutes). Max 600000." Default 120s + max 600s applied in `crates/runtime/src/bash.rs` when the model sends none.

### ✅ Read/Write/Edit/Glob/Grep/WebSearch/WebFetch descriptions were also stubs
**Claude Code:** each has 5-15 lines of usage guidance — "you must read before editing", "prefer edit over write", "include Sources section", "use this not bash grep".
**Original claw:** one-line summaries.
**Fix:** ported usage prose for all 8 model-facing tools. Each schema property now also has a description.

---

## Tool registry

### ✅ `SendUserMessage` tool didn't exist in Claude Code
**Claude Code:** plain text replies are *not* wrapped in any tool — the model emits text and the host renders it.
**Original claw:** had `SendUserMessage` (alias for the underlying Brief proactive-message handler) registered as a model-facing tool with description "Send a message to the user."
**Symptom:** Qwen used it for every reply and "thanked itself" for the resulting tool result.
**Fix:** removed the `SendUserMessage` ToolSpec. Dispatch handler kept for the `Brief` alias so future proactive features can use it without re-introducing the model confusion.

### ✅ Internal/legacy tools were exposed to the model
**Claude Code:** `ConfigTool`, `SkillTool`, `AgentTool`, etc. are exposed but only with detailed descriptions and only when relevant. Claude is good enough to ignore irrelevant tools.
**Original claw:** Config, Sleep, REPL, PowerShell, NotebookEdit, StructuredOutput, Skill, ToolSearch, Agent, TodoWrite all sent to the model. Small models pick at random.
**Fix:** added `MODEL_HIDDEN_TOOLS` filter in `crates/claw-cli/src/main.rs` `filter_tool_specs`. Dispatch handlers stay for internal use; they just aren't candidates for model selection.

### 🔧 Tools Claude has that we don't yet expose
- `AskUserQuestion` — clarification tool. Useful but currently nothing uses it; would also need a UI surface for the user to answer.
- `EnterPlanMode` / `ExitPlanMode` — planning sub-mode. Complex feature.
- `EnterWorktree` / `ExitWorktree` — git worktree management. Useful for the orchestrator's verification step.
- `LSPTool` — language server queries. Big feature, not needed for v0.
- `TaskCreate` / `TaskGet` / `TaskList` / `TaskOutput` / `TaskStop` / `TaskUpdate` — async task tracking. Not core to the agent loop.
- `Team*` — team collaboration features. Skip.
- `SyntheticOutputTool` — Anthropic-internal. Skip.
- `SchedulerCronTool`, `RemoteTriggerTool`, `MCP*Tool` — advanced features. Skip for v0.

---

## CLI surface

### ✅ Binary was named `claw`, not `notclaude`
**Fix:** Cargo.toml `[[bin]] name = "notclaude"`. Wrapper script updated. `--help`, `--version`, REPL banner all rebranded.

### ✅ Workspace memory file was `CLAW.md`
**Claude Code:** `CLAUDE.md`.
**Fix:** went with `NOTCLAUDE.md` (own brand, doesn't collide with Claude Code installs in same workspace).

### ✅ Per-project / per-user state dirs were `.claw/`
**Fix:** all `.claw/` paths now `.notclaude/`.

### 🔧 Slash commands are a smaller set than Claude's
Claude Code has ~50 slash commands; we have a subset. Not a behavior issue, just feature surface.

---

## REPL / TUI

### ✅ Welcome banner was a flat text list
**Claude Code:** bordered two-column box (LogoV2.tsx) with welcome / model info on left, tips & news feed on right, ASCII Anthropic logo.
**Fix:** ported the layout. Static news feed (Claude pulls dynamic from API).

### ✅ Spinner glyph + "Done" indicator
Claude uses `✻ Thinking…` and silent finish. We were using `🦀 Thinking…` and `✨ Done`. Now matches.

### ✅ Tool-call rendering was a `╭─ Name ─╮ │ ... │ ╰──────╯` box
Claude Code uses `● ToolName(args) ⎿ result`. Ported.

### ✅ Input prompt was bare `> `
Claude Code wraps the input in `─ ❯ ─` rules with a status hint line below. Ported (without bordered side `│ │` — would need line editor rewrite).

### 🔧 Placeholder ghost text in input
Claude shows `Try "fix typecheck errors"` as dim placeholder text when input is empty. Not implemented — needs line editor support.

### 🔧 Bottom status line is static
Claude's bottom status reflects current `/effort`, agent type, model context usage. Ours just shows model name.

---

## Provider routing

### ✅ CLI was hardcoded to `ClawApiClient`
**Symptom:** any model name, even a local one, demanded `ANTHROPIC_API_KEY`.
**Fix:** `DefaultRuntimeClient::new` now uses `ProviderClient::from_model_with_default_auth(&model, ...)` which routes by model name to the right provider.

### ✅ OpenAI-compat doesn't return structured tool_calls from Ollama+Qwen
**Symptom:** Qwen emits tool calls as JSON in text content; claw treated them as plain output.
**Fix:** stream parser in `crates/api/src/providers/openai_compat.rs` watches for `{` or ` ``` ` mid-stream, buffers, parses (with smart-quote normalization), synthesizes structured ToolUse events. Handles multiple concatenated calls.

---

## Inference-engine specifics (open work)

### 🔧 No GBNF grammar enforcement
Long-term, switching from Ollama to llama.cpp server with GBNF grammar would force tool-call output to be valid OpenAI `tool_calls` JSON, removing the need for our text-extraction heuristic. Bigger infra change.

### 🔧 No structured-output enforcement per role
The orchestrator design relies on each role (router, planner, patcher, reviewer) emitting structured JSON. We don't enforce schemas yet.

---

## How to use this doc

When investigating a bad model behavior, first check this doc for known divergences. If the bug isn't here, do the same workflow that closed the others:
1. Reproduce the bad behavior.
2. Find the equivalent code in `codeaashu/claude-code` (use `gh api repos/codeaashu/claude-code/contents/<path>`).
3. Compare. The gap is usually obvious.
4. Port the relevant prose / behavior.
5. Add the divergence (and the fix) to this doc.

Each system-prompt or tool-description gap closed has measurably improved local-model behavior in our tests.
