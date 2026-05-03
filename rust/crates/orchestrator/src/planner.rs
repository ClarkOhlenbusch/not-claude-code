//! Phase 3 — planner role.
//!
//! TODO: implement `plan(user_goal, seen_failures) -> Result<Plan, PlannerError>`.
//! Calls the planner model (default: GLM-4.7-Flash) with `PLAN_JSON_SCHEMA`
//! injected into the system prompt. Uses Ollama's `format: "json"` if the
//! provider exposes it; otherwise relies on the existing tool-call-text
//! synthesis layer in `crates/api/src/providers/openai_compat.rs`.
//!
//! Phase 4 will extend the signature to accept `seen_failures: Vec<String>`
//! to prevent replan oscillation.

use std::fmt::{self, Display, Formatter};

use crate::plan::Plan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannerError {
    UnparseableJson(String),
    EmptyPlan,
}

impl Display for PlannerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnparseableJson(msg) => write!(f, "planner returned unparseable JSON: {msg}"),
            Self::EmptyPlan => write!(f, "planner emitted an empty plan"),
        }
    }
}

impl std::error::Error for PlannerError {}

/// Phase 3 entry point — placeholder until the planner is wired.
pub fn plan(_user_goal: &str) -> Result<Plan, PlannerError> {
    // Phase 1 placeholder. Real implementation lands in Phase 3.
    Err(PlannerError::EmptyPlan)
}
