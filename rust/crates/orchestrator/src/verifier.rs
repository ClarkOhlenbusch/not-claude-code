//! Phase 3 — deterministic post-step checks.
//!
//! No LLM here — these are mechanical checks that the step's declared
//! side-effects actually happened: file existence, exit codes, no error
//! markers in tool results. Deterministic verification is the orchestrator's
//! ground truth; the reviewer LLM (Phase 4) is a second judgment on top.

use std::path::Path;

use crate::plan::Step;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    Pass,
    Fail { reason: String },
}

/// Phase 3: check that every declared `context_path` for the step exists
/// after execution. More checks (exit codes, lint) layered in later.
#[must_use]
pub fn verify(step: &Step) -> VerifyOutcome {
    for path in &step.context_paths {
        if !Path::new(path).exists() {
            return VerifyOutcome::Fail {
                reason: format!(
                    "step `{}` declared context_path `{}` but it was not created",
                    step.id,
                    path.display()
                ),
            };
        }
    }
    VerifyOutcome::Pass
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn step_with_no_context_paths_passes_trivially() {
        let step = Step {
            id: "noop".to_string(),
            description: "do nothing".to_string(),
            success_criteria: "n/a".to_string(),
            tool_allowlist: vec![],
            context_paths: vec![],
        };
        assert_eq!(verify(&step), VerifyOutcome::Pass);
    }

    #[test]
    fn step_with_missing_path_fails_with_helpful_reason() {
        let step = Step {
            id: "expect-file".to_string(),
            description: "should create something".to_string(),
            success_criteria: "n/a".to_string(),
            tool_allowlist: vec![],
            context_paths: vec![PathBuf::from("/tmp/notclaude-orch-verify-nonexistent")],
        };
        match verify(&step) {
            VerifyOutcome::Fail { reason } => {
                assert!(reason.contains("expect-file"));
                assert!(reason.contains("not created"));
            }
            VerifyOutcome::Pass => panic!("expected fail, got pass"),
        }
    }
}
