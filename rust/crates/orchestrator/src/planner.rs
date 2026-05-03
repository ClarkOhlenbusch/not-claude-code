//! Planner role.
//!
//! Pulls structured task requirements from the user's prompt before the
//! coder runs. The output is a free-form `Spec` (text), injected into the
//! coder's system prompt as additional context. This forces small models
//! to follow specific content instructions ("use these project names",
//! "include these sections") that they otherwise drift away from.
//!
//! Calls the planner model directly via `api::ProviderClient` (bypassing
//! the streaming UI — the planner pass is a quick reasoning step the user
//! shouldn't have to see).

use api::{InputContentBlock, InputMessage, MessageRequest, OutputContentBlock, ProviderClient};

use crate::roles::RoleConfig;

const PLANNER_SYSTEM_PROMPT: &str = r#"You extract a structured task specification from a user's coding request.

Read the user's prompt carefully. List EVERY concrete requirement they mentioned: specific names, sections, features, file paths, content elements, styling, behaviors. Do not invent requirements they didn't state. Do not add commentary.

Output a single JSON object with this exact shape:
{
  "goal": "one-sentence summary of what the user wants",
  "requirements": ["specific requirement 1", "specific requirement 2", ...],
  "files_expected": ["path/to/file/that/should/exist", ...]
}

ONLY emit the JSON object — no preamble, no explanation, no code fences."#;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Spec {
    pub goal: String,
    pub requirements: Vec<String>,
    #[serde(default)]
    pub files_expected: Vec<String>,
}

impl Spec {
    /// Render the spec as a system-prompt fragment the coder will receive.
    /// Explicitly framed as a checklist the coder MUST satisfy.
    pub fn render_for_coder(&self) -> String {
        let mut out = String::from("# Task specification (you MUST satisfy ALL items)\n\n");
        out.push_str(&format!("Goal: {}\n\n", self.goal));
        out.push_str("Required content:\n");
        for req in &self.requirements {
            out.push_str(&format!("- {req}\n"));
        }
        if !self.files_expected.is_empty() {
            out.push_str("\nExpected file paths:\n");
            for path in &self.files_expected {
                out.push_str(&format!("- {path}\n"));
            }
        }
        out.push_str(
            "\nDo not skip items. Do not substitute placeholder text for items the user named explicitly.\n",
        );
        out
    }
}

#[derive(Debug)]
pub enum PlannerError {
    Api(String),
    Parse(String),
}

impl std::fmt::Display for PlannerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(msg) => write!(f, "planner API call failed: {msg}"),
            Self::Parse(msg) => write!(f, "planner JSON parse failed: {msg}"),
        }
    }
}

impl std::error::Error for PlannerError {}

/// Call the planner model and return a parsed `Spec`. Synchronous wrapper
/// around the async `ProviderClient::send_message` — uses a per-call
/// current-thread tokio runtime to mirror `DefaultRuntimeClient`'s pattern.
pub fn extract_spec(roles: &RoleConfig, user_message: &str) -> Result<Spec, PlannerError> {
    let request = MessageRequest {
        model: roles.planner_model.clone(),
        max_tokens: 1024,
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: user_message.to_string(),
            }],
        }],
        system: Some(PLANNER_SYSTEM_PROMPT.to_string()),
        tools: None,
        tool_choice: None,
        stream: false,
    };
    let client = ProviderClient::from_model_with_default_auth(&roles.planner_model, None)
        .map_err(|e| PlannerError::Api(e.to_string()))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| PlannerError::Api(format!("tokio runtime: {e}")))?;
    let response = rt
        .block_on(async { client.send_message(&request).await })
        .map_err(|e| PlannerError::Api(e.to_string()))?;
    parse_spec_from_response(&response.content)
}

fn parse_spec_from_response(blocks: &[OutputContentBlock]) -> Result<Spec, PlannerError> {
    let mut text = String::new();
    for block in blocks {
        if let OutputContentBlock::Text { text: t } = block {
            text.push_str(t);
        }
    }
    let cleaned = strip_code_fences(text.trim());
    let normalized = normalize_smart_quotes(cleaned);
    serde_json::from_str::<Spec>(&normalized).map_err(|e| {
        PlannerError::Parse(format!("{e}\nplanner output was: {}", text.trim()))
    })
}

/// Strip surrounding ```json ... ``` (Qwen sometimes wraps even when told not to).
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

/// Normalize Unicode smart quotes to ASCII so serde_json parses them.
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
    fn spec_renders_with_all_sections() {
        let spec = Spec {
            goal: "build a website".to_string(),
            requirements: vec![
                "use these project names: A, B, C".to_string(),
                "include CSS gradient".to_string(),
            ],
            files_expected: vec!["/tmp/site.html".to_string()],
        };
        let rendered = spec.render_for_coder();
        assert!(rendered.contains("Goal: build a website"));
        assert!(rendered.contains("- use these project names"));
        assert!(rendered.contains("- include CSS gradient"));
        assert!(rendered.contains("- /tmp/site.html"));
    }

    #[test]
    fn parse_accepts_clean_json() {
        let blocks = vec![OutputContentBlock::Text {
            text: r#"{"goal":"x","requirements":["a","b"],"files_expected":["/tmp/z"]}"#.to_string(),
        }];
        let spec = parse_spec_from_response(&blocks).expect("parse");
        assert_eq!(spec.goal, "x");
        assert_eq!(spec.requirements.len(), 2);
    }

    #[test]
    fn parse_strips_markdown_fences() {
        let blocks = vec![OutputContentBlock::Text {
            text: "```json\n{\"goal\":\"x\",\"requirements\":[],\"files_expected\":[]}\n```"
                .to_string(),
        }];
        let spec = parse_spec_from_response(&blocks).expect("parse");
        assert_eq!(spec.goal, "x");
    }

    #[test]
    fn parse_normalizes_smart_quotes() {
        let blocks = vec![OutputContentBlock::Text {
            text: "{\u{201C}goal\u{201D}:\u{201C}x\u{201D},\u{201C}requirements\u{201D}:[],\u{201C}files_expected\u{201D}:[]}".to_string(),
        }];
        let spec = parse_spec_from_response(&blocks).expect("parse");
        assert_eq!(spec.goal, "x");
    }
}
