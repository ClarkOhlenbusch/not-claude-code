//! Per-role model configuration.
//!
//! The orchestrator picks a different model per role. Phase 1 just stores
//! the model names; later phases consume them when constructing the role-
//! specific `ProviderClient` calls.
//!
//! Defaults are deliberately conservative — they all point at the bench-
//! proven `qwen2.5-coder:14b` so a Phase-1 swarm run produces the same
//! pass rate as the single-model baseline.

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
    /// Phase-1 default: every role points at the bench-proven 14B coder.
    /// Specialization comes from per-role prompts, not different weights —
    /// proves the orchestration path before introducing model swaps.
    fn default() -> Self {
        let baseline = "qwen2.5-coder:14b".to_string();
        Self {
            intent_model: baseline.clone(),
            planner_model: baseline.clone(),
            coder_model: baseline.clone(),
            reviewer_model: baseline,
        }
    }
}

impl RoleConfig {
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
