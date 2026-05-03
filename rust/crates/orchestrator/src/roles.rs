//! Per-role model configuration.
//!
//! The orchestrator picks a different model per role. Phase 1 just stores
//! the model names; later phases consume them when constructing the role-
//! specific `ProviderClient` calls.
//!
//! Defaults are deliberately conservative — they all point at the bench-
//! proven `qwen2.5-coder:14b` so a Phase-1 swarm run produces the same
//! pass rate as the single-model baseline.

use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleConfig {
    /// Tiny conversational model that classifies user intent.
    /// Recommended: `qwen2.5:3b-instruct` or `llama3.2:3b`.
    pub intent_model: String,
    /// Reasoning-strong model that produces structured Plans.
    /// Recommended: `glm-4.7-flash` (~17 GB MoE).
    pub planner_model: String,
    /// Code-tuned model that executes each plan step's tool work.
    /// Recommended: `qwen3-coder:30b-a3b` (~17 GB MoE).
    pub coder_model: String,
    /// LLM judge that evaluates whether a step succeeded vs. its
    /// success criteria. Same model as planner by design.
    pub reviewer_model: String,
}

impl Default for RoleConfig {
    /// Default: every role shares Qwen3.6-35B-A3B (May 2026 frontier
    /// agentic coder — 3B active MoE, ~20 GB Q4, 73.4 on SWE-Bench Verified).
    /// Specialization comes from per-role prompts, not different weights —
    /// keeps a single model resident in memory (no swap cost).
    fn default() -> Self {
        let baseline = "qwen3.6:35b-a3b".to_string();
        Self {
            intent_model: baseline.clone(),
            planner_model: baseline.clone(),
            coder_model: baseline.clone(),
            reviewer_model: baseline,
        }
    }
}

impl RoleConfig {
    /// Build role config from environment overrides, falling back to the
    /// default single-model baseline. The `NOTCLAUDE_SWARM_*` names match the
    /// launcher-era swarm path so existing coinflip invocations keep working.
    #[must_use]
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(model) = env::var("NOTCLAUDE_SWARM_ORCHESTRATOR") {
            if !model.trim().is_empty() {
                let model = model.trim().to_string();
                config.intent_model = model.clone();
                config.planner_model = model.clone();
                config.reviewer_model = model;
            }
        }

        if let Some(model) = env::var("NOTCLAUDE_SWARM_MODELS").ok().and_then(|models| {
            models
                .split(',')
                .map(str::trim)
                .find(|model| !model.is_empty())
                .map(str::to_string)
        }) {
            config.coder_model = model;
        }

        if let Ok(model) = env::var("NOTCLAUDE_SWARM_INTENT_MODEL") {
            if !model.trim().is_empty() {
                config.intent_model = model.trim().to_string();
            }
        }
        if let Ok(model) = env::var("NOTCLAUDE_SWARM_PLANNER_MODEL") {
            if !model.trim().is_empty() {
                config.planner_model = model.trim().to_string();
            }
        }
        if let Ok(model) = env::var("NOTCLAUDE_SWARM_CODER_MODEL") {
            if !model.trim().is_empty() {
                config.coder_model = model.trim().to_string();
            }
        }
        if let Ok(model) = env::var("NOTCLAUDE_SWARM_REVIEWER_MODEL") {
            if !model.trim().is_empty() {
                config.reviewer_model = model.trim().to_string();
            }
        }

        config
    }

    /// Configuration matching the user's design choice (3 distinct models).
    /// Enable explicitly for the Phase 6 bake-off; not the Phase 1 default.
    #[must_use]
    pub fn distinct_models() -> Self {
        Self {
            intent_model: "qwen2.5:3b-instruct".to_string(),
            planner_model: "glm-4.7-flash".to_string(),
            coder_model: "qwen3-coder:30b-a3b".to_string(),
            reviewer_model: "glm-4.7-flash".to_string(),
        }
    }
}
