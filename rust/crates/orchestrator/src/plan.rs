//! Phase 3 — Plan and Step types.
//!
//! Output schema for the planner role. Each step carries its own success
//! criteria (consumed by the reviewer) plus a tool allowlist (consumed by
//! the coder). Context paths help the verifier check expected side-effects.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub description: String,
    /// One-sentence statement the reviewer can check this step against.
    pub success_criteria: String,
    /// Tool names the coder is allowed to call for this step.
    /// Empty = no tools (chat-style step).
    #[serde(default)]
    pub tool_allowlist: Vec<String>,
    /// Files relevant to this step. Verifier checks they exist post-step.
    #[serde(default)]
    pub context_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// Overall success criteria — reviewer's exit check after all steps pass.
    pub success_criteria: String,
}

/// JSON schema injected into the planner's system prompt. Strict, no
/// additionalProperties so the parser fails fast on shape drift.
pub const PLAN_JSON_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "steps": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "description": { "type": "string" },
          "success_criteria": { "type": "string" },
          "tool_allowlist": { "type": "array", "items": { "type": "string" } },
          "context_paths": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["id", "description", "success_criteria"],
        "additionalProperties": false
      }
    },
    "success_criteria": { "type": "string" }
  },
  "required": ["steps", "success_criteria"],
  "additionalProperties": false
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_round_trips_through_json() {
        let plan = Plan {
            steps: vec![Step {
                id: "step1".to_string(),
                description: "write a file".to_string(),
                success_criteria: "file exists".to_string(),
                tool_allowlist: vec!["write_file".to_string()],
                context_paths: vec!["/tmp/example.txt".into()],
            }],
            success_criteria: "task complete".to_string(),
        };
        let json = serde_json::to_string(&plan).expect("serialize");
        let parsed: Plan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(plan, parsed);
    }

    #[test]
    fn plan_schema_is_valid_json() {
        let _: serde_json::Value =
            serde_json::from_str(PLAN_JSON_SCHEMA).expect("schema is valid JSON");
    }
}
