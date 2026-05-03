//! Phase 3 — coder role + Phase 5 — Subagent primitive.
//!
//! Phase 3: `CoderInvocation` carries the per-step description, tool allowlist,
//! and message context. `run_step()` calls the coder model and returns its
//! events directly so the outer ConversationRuntime can dispatch any
//! resulting tool_use blocks.
//!
//! Phase 5 transition: this module gains a `Subagent` type that runs its OWN
//! nested coder+tool loop using a shared `Arc<dyn ToolExecutor>`. After
//! Phase 5 lands, tool_uses no longer bubble up to the outer runtime —
//! they're consumed inside the subagent. Document this transition in
//! `docs/claude-code-divergences.md` when it lands.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderInvocation {
    pub description: String,
    /// Subset of registered tools this invocation may call. Empty = chat-only.
    pub allowed_tools: Vec<String>,
    /// Files relevant to this step (passed as context to the model).
    pub context_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentReport {
    pub ok: bool,
    pub summary: String,
    /// Tool names invoked during this subagent run, with serialized inputs.
    pub tool_uses: Vec<(String, String)>,
    /// Files the subagent touched (parsed from Write/Edit tool inputs).
    pub files_touched: Vec<PathBuf>,
}
