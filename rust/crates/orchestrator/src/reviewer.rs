//! Reviewer role.
//!
//! After the coder completes a turn (no more tool_use events), the reviewer
//! judges whether the result satisfies the spec produced by the planner.
//! On Fail, the orchestrator re-runs the coder with a critique appended to
//! its context. Bounded retries (`MAX_RETRIES_PER_TURN`).
//!
//! Like the planner, this is a quick reasoning call that bypasses the
//! streaming UI — user shouldn't see the reviewer's deliberation.

use api::{InputContentBlock, InputMessage, MessageRequest, OutputContentBlock, ProviderClient};

use crate::planner::Spec;
use crate::roles::RoleConfig;

const REVIEWER_SYSTEM_PROMPT: &str = r#"You judge whether a coder's output satisfies a task specification.

You will receive: (1) the original task spec with required content items, and (2) a summary of what the coder did and produced. Your job is a strict quality check.

Output a single JSON object:
{
  "pass": true | false,
  "reason": "one sentence explaining why"
}

If ANY requirement was skipped, replaced with placeholder text (e.g. "Project Name 1" instead of an explicit name from the spec), or otherwise unsatisfied, return pass=false with a specific reason. If all requirements are met, return pass=true.

ONLY emit the JSON object — no preamble, no explanation, no code fences."#;

pub const MAX_RETRIES_PER_TURN: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewOutcome {
    Pass,
    Fail { reason: String },
}

#[derive(Debug, serde::Deserialize)]
struct ReviewerResponse {
    pass: bool,
    #[serde(default)]
    reason: String,
}

/// Synchronously call the reviewer model and return its verdict. Returns
/// `Pass` (rather than an error) on parse failure to avoid breaking the
/// turn — the reviewer is advisory; if it can't judge, let the response
/// through.
pub fn review(roles: &RoleConfig, spec: &Spec, coder_summary: &str) -> ReviewOutcome {
    let user_msg = format!(
        "Task spec:\n{}\n\nCoder output summary:\n{}",
        spec.render_for_coder(),
        coder_summary
    );
    let request = MessageRequest {
        model: roles.reviewer_model.clone(),
        max_tokens: 256,
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text { text: user_msg }],
        }],
        system: Some(REVIEWER_SYSTEM_PROMPT.to_string()),
        tools: None,
        tool_choice: None,
        stream: false,
    };
    let Ok(client) = ProviderClient::from_model_with_default_auth(&roles.reviewer_model, None)
    else {
        return ReviewOutcome::Pass; // can't construct client → don't block
    };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return ReviewOutcome::Pass;
    };
    let Ok(response) = rt.block_on(async { client.send_message(&request).await }) else {
        return ReviewOutcome::Pass;
    };
    parse_review(&response.content)
}

fn parse_review(blocks: &[OutputContentBlock]) -> ReviewOutcome {
    let mut text = String::new();
    for block in blocks {
        if let OutputContentBlock::Text { text: t } = block {
            text.push_str(t);
        }
    }
    let cleaned = strip_code_fences(text.trim());
    let normalized = normalize_smart_quotes(cleaned);
    match serde_json::from_str::<ReviewerResponse>(&normalized) {
        Ok(resp) if resp.pass => ReviewOutcome::Pass,
        Ok(resp) => ReviewOutcome::Fail {
            reason: resp.reason,
        },
        // Fallback: if reviewer JSON is malformed, let the turn through
        // rather than blocking on the model's failure to follow format.
        Err(_) => ReviewOutcome::Pass,
    }
}

fn strip_code_fences(input: &str) -> &str {
    let trimmed = input.trim();
    let Some(stripped) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let after_lang = match stripped.find('\n') {
        Some(idx) => &stripped[idx + 1..],
        None => stripped,
    };
    after_lang.strip_suffix("```").unwrap_or(after_lang).trim()
}

fn normalize_smart_quotes(s: &str) -> String {
    s.replace('\u{201C}', "\"")
        .replace('\u{201D}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pass_response() {
        let blocks = vec![OutputContentBlock::Text {
            text: r#"{"pass":true,"reason":"all good"}"#.to_string(),
        }];
        assert_eq!(parse_review(&blocks), ReviewOutcome::Pass);
    }

    #[test]
    fn parse_fail_with_reason() {
        let blocks = vec![OutputContentBlock::Text {
            text: r#"{"pass":false,"reason":"missing project names"}"#.to_string(),
        }];
        match parse_review(&blocks) {
            ReviewOutcome::Fail { reason } => assert!(reason.contains("project names")),
            _ => panic!("expected fail"),
        }
    }

    #[test]
    fn parse_malformed_defaults_to_pass() {
        let blocks = vec![OutputContentBlock::Text {
            text: "not json at all".to_string(),
        }];
        assert_eq!(parse_review(&blocks), ReviewOutcome::Pass);
    }
}
