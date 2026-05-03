//! Intent classifier role.
//!
//! Quick classification of "what kind of request is this" before we burn
//! planner + reviewer cycles. For simple chat ("what's 2+2", "what is a
//! Vec in Rust"), the orchestrator should skip planning and route straight
//! to the coder for a one-shot text reply.
//!
//! Calls the intent model directly (bypass the streaming UI). Designed to
//! be cheap — the smallest model in the lineup.

use api::{InputContentBlock, InputMessage, MessageRequest, OutputContentBlock, ProviderClient};

use crate::roles::RoleConfig;

const INTENT_SYSTEM_PROMPT: &str = r#"You classify a user's request into ONE of four categories so the host can route it efficiently:

- "chat"        — conversational reply needed; no files, no tools, no commands. (e.g., "what is X?", "hi", "explain Y")
- "single_edit" — one targeted file or shell action. (e.g., "create file X", "run command Y", "edit file Z to do A")
- "multi_step"  — multiple files or steps required. (e.g., "build a website with HTML and CSS", "scaffold a project")
- "search"      — read-heavy investigation. (e.g., "find files matching X", "what's in directory Y")

Respond with ONLY this JSON object, nothing else:
{"intent": "chat" | "single_edit" | "multi_step" | "search"}"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Chat,
    SingleEdit,
    MultiStep,
    Search,
}

impl Intent {
    /// Whether this intent benefits from running the planner pre-pass.
    /// Chat doesn't — it's just an answer.
    #[must_use]
    pub fn needs_planning(self) -> bool {
        !matches!(self, Self::Chat)
    }
}

#[derive(Debug, serde::Deserialize)]
struct IntentResponse {
    intent: String,
}

/// Classify the user's message. Falls back to `MultiStep` on any failure
/// (parse, network, malformed model output) — the safer assumption is that
/// a request is non-trivial.
pub fn classify(roles: &RoleConfig, user_message: &str) -> Intent {
    let request = MessageRequest {
        model: roles.intent_model.clone(),
        max_tokens: 32,
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: user_message.to_string(),
            }],
        }],
        system: Some(INTENT_SYSTEM_PROMPT.to_string()),
        tools: None,
        tool_choice: None,
        stream: false,
    };
    let Ok(client) = ProviderClient::from_model_with_default_auth(&roles.intent_model, None) else {
        return Intent::MultiStep;
    };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Intent::MultiStep;
    };
    let Ok(response) = rt.block_on(async { client.send_message(&request).await }) else {
        return Intent::MultiStep;
    };
    parse_intent(&response.content).unwrap_or(Intent::MultiStep)
}

fn parse_intent(blocks: &[OutputContentBlock]) -> Option<Intent> {
    let mut text = String::new();
    for block in blocks {
        if let OutputContentBlock::Text { text: t } = block {
            text.push_str(t);
        }
    }
    let cleaned = strip_code_fences(text.trim());
    let normalized = normalize_smart_quotes(cleaned);
    let parsed: IntentResponse = serde_json::from_str(&normalized).ok()?;
    match parsed.intent.as_str() {
        "chat" => Some(Intent::Chat),
        "single_edit" => Some(Intent::SingleEdit),
        "multi_step" => Some(Intent::MultiStep),
        "search" => Some(Intent::Search),
        _ => None,
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
    fn chat_intent_skips_planning() {
        assert!(!Intent::Chat.needs_planning());
    }

    #[test]
    fn non_chat_intents_need_planning() {
        for intent in [Intent::SingleEdit, Intent::MultiStep, Intent::Search] {
            assert!(intent.needs_planning(), "{intent:?} should plan");
        }
    }

    #[test]
    fn parse_recognises_each_intent() {
        for (name, expected) in [
            ("chat", Intent::Chat),
            ("single_edit", Intent::SingleEdit),
            ("multi_step", Intent::MultiStep),
            ("search", Intent::Search),
        ] {
            let blocks = vec![OutputContentBlock::Text {
                text: format!("{{\"intent\":\"{name}\"}}"),
            }];
            assert_eq!(parse_intent(&blocks), Some(expected));
        }
    }

    #[test]
    fn parse_handles_code_fenced_output() {
        let blocks = vec![OutputContentBlock::Text {
            text: "```json\n{\"intent\":\"chat\"}\n```".to_string(),
        }];
        assert_eq!(parse_intent(&blocks), Some(Intent::Chat));
    }

    #[test]
    fn parse_returns_none_on_unknown_string() {
        let blocks = vec![OutputContentBlock::Text {
            text: r#"{"intent":"refactor"}"#.to_string(),
        }];
        assert_eq!(parse_intent(&blocks), None);
    }
}
