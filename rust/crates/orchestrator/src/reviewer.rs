//! Phase 4 — reviewer role.
//!
//! TODO: implement `review(step, results) -> ReviewOutcome` and
//! `critique(step, results, reason) -> String`. Both call the reviewer
//! model (same as planner — GLM-4.7-Flash by default). Reviewer judges
//! whether `step.success_criteria` was met; critic explains the failure
//! mode for the coder's next attempt.

use crate::plan::Step;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewOutcome {
    Pass,
    Fail { reason: String },
}

/// Phase 4 entry point — placeholder until the reviewer is wired.
pub fn review(_step: &Step) -> ReviewOutcome {
    // Phase 1 default: optimistic pass. Real LLM check lands in Phase 4.
    ReviewOutcome::Pass
}
