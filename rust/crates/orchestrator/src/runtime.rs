//! Orchestrator runtime — implements `runtime::ApiClient`.
//!
//! Phase 3-4: planner pre-pass + reviewer post-check + retry loop.
//!
//! Per-turn flow on the FIRST `stream()` call (when `state.spec.is_none()`):
//! 1. Extract the user's last message.
//! 2. Call the planner model → get a `Spec` of explicit requirements.
//! 3. Inject `spec.render_for_coder()` into the inner client's system prompt.
//! 4. Call inner client → return its events with `[role:coder]` banner.
//!
//! On the FINAL coder response (no tool_use, retries remaining):
//! 5. Summarize what the coder did from the event stream.
//! 6. Call the reviewer model → Pass | Fail { reason }.
//! 7. On Fail with retries left: append a critique message and re-run inner
//!    client. Concatenate events. Repeat up to MAX_RETRIES_PER_TURN.
//!
//! State persists across `stream()` calls so the conversation loop's
//! tool-dispatch round-trips don't reset the spec or retry counter.
//!
//! "New turn" detection: when the request's last user-role message changes
//! (different content) AND there's no preceding tool_result for the current
//! plan, reset state.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use runtime::{
    ApiClient, ApiRequest, AssistantEvent, ContentBlock, ConversationMessage, RuntimeError,
};

use crate::events::prepend_role_banner;
use crate::intent::{self, Intent};
use crate::planner::{self, Spec};
use crate::reviewer::{self, ReviewOutcome, MAX_RETRIES_PER_TURN};
use crate::roles::RoleConfig;

#[derive(Default)]
struct State {
    /// Classified intent for this turn. None = haven't classified yet.
    intent: Option<Intent>,
    /// Spec extracted by planner for the current turn. None = no spec
    /// (either chat intent, or planner failed gracefully).
    spec: Option<Spec>,
    /// Number of reviewer-driven retries used this turn.
    retries_used: u32,
    /// Hash of the last user message we planned for. New value = new turn → reset.
    last_user_msg_hash: Option<u64>,
}

/// Multi-model orchestrator. Phase 1 (passthrough) + Phase 3-4 (planner +
/// reviewer + retry) when constructed via `with_orchestration_enabled`.
pub struct OrchestratorRuntime {
    inner: Box<dyn ApiClient + Send>,
    roles: RoleConfig,
    state: State,
    enabled: bool,
}

impl OrchestratorRuntime {
    /// Phase 1 transparent passthrough. Adds a `[role:coder]` event tag but
    /// does NOT call planner or reviewer.
    pub fn new(inner: Box<dyn ApiClient + Send>, roles: RoleConfig) -> Self {
        Self {
            inner,
            roles,
            state: State::default(),
            enabled: false,
        }
    }

    /// Phase 1 convenience — same as `new` with default RoleConfig.
    pub fn with_default_roles(inner: Box<dyn ApiClient + Send>) -> Self {
        Self::new(inner, RoleConfig::default())
    }

    /// Phase 3+ active mode: enables planner pre-pass and reviewer retry loop.
    pub fn with_orchestration_enabled(inner: Box<dyn ApiClient + Send>, roles: RoleConfig) -> Self {
        Self {
            inner,
            roles,
            state: State::default(),
            enabled: true,
        }
    }

    /// Read-only access for callers that want to inspect role config.
    #[must_use]
    pub fn roles(&self) -> &RoleConfig {
        &self.roles
    }
}

impl ApiClient for OrchestratorRuntime {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        if !self.enabled {
            // Phase 1 transparent path — keep behavior identical to direct
            // single-model use. Banner is informational only.
            let mut events = self.inner.stream(request)?;
            prepend_role_banner("coder", &mut events);
            return Ok(events);
        }

        // Active orchestration path.
        let user_msg = extract_last_user_text(&request.messages);
        let user_hash = user_msg.as_deref().map(hash_str);

        // New-turn detection: user message hash changed (or never set) →
        // reset state, classify intent, and (if non-chat) re-plan.
        if user_hash != self.state.last_user_msg_hash {
            self.state = State::default();
            self.state.last_user_msg_hash = user_hash;
            if let Some(msg) = &user_msg {
                // Phase 2: intent classifier. Chat prompts (e.g. "what's 2+2",
                // "explain X") skip the planner+reviewer overhead entirely.
                let intent = intent::classify(&self.roles, msg);
                self.state.intent = Some(intent);
                if intent.needs_planning() {
                    match planner::extract_spec(&self.roles, msg) {
                        Ok(spec) => self.state.spec = Some(spec),
                        Err(e) => {
                            // Planner failure is non-fatal — proceed without a
                            // spec. Coder gets the original prompt only. Better
                            // degraded than failed.
                            eprintln!("\n[role:planner] failed: {e}\n");
                        }
                    }
                }
            }
        }

        // Inject orchestration context into the inner client's system prompt.
        let mut augmented_request = request;
        augmented_request
            .system_prompt
            .push(render_orchestration_identity_context(&self.roles));
        if let Some(spec) = &self.state.spec {
            augmented_request
                .system_prompt
                .push(spec.render_for_coder());
        }

        // Run the coder.
        let mut events = self.inner.stream(augmented_request.clone())?;

        // Reviewer retry loop. Only triggers when the coder has finished
        // its turn (no tool_use in events — meaning it's not waiting on a
        // tool_result). Otherwise the conversation loop is mid-step and
        // reviewing prematurely would be wrong.
        let coder_done = !events
            .iter()
            .any(|e| matches!(e, AssistantEvent::ToolUse { .. }));
        if coder_done
            && !request_has_tool_result(&augmented_request)
            && self.state.retries_used < MAX_RETRIES_PER_TURN
        {
            if let Some(spec) = self.state.spec.clone() {
                let summary = summarize_events(&events);
                let outcome = reviewer::review(&self.roles, &spec, &summary);
                if let ReviewOutcome::Fail { reason } = outcome {
                    self.state.retries_used += 1;
                    eprintln!(
                        "\n[role:reviewer] retry {}/{}: {}\n",
                        self.state.retries_used, MAX_RETRIES_PER_TURN, reason
                    );
                    // Append critique to the request and re-run inner.
                    let mut retry_request = augmented_request;
                    retry_request.messages.push(ConversationMessage::user_text(
                        format!("The previous attempt did not satisfy the spec. Reviewer said: {reason}\n\nPlease redo and address the gap."),
                    ));
                    let retry_events = self.inner.stream(retry_request)?;
                    events.extend(retry_events);
                }
            }
        }

        Ok(events)
    }
}

fn extract_last_user_text(messages: &[ConversationMessage]) -> Option<String> {
    use runtime::MessageRole;
    for msg in messages.iter().rev() {
        if msg.role == MessageRole::User {
            for block in &msg.blocks {
                if let ContentBlock::Text { text } = block {
                    return Some(text.clone());
                }
            }
        }
    }
    None
}

fn hash_str(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn request_has_tool_result(request: &ApiRequest) -> bool {
    request.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolResult { .. }))
    })
}

fn render_orchestration_identity_context(roles: &RoleConfig) -> String {
    format!(
        "# NOT Claude Code coinflip swarm context\n\
         - Runtime mode: coinflip swarm.\n\
         - Orchestrator model: {}.\n\
         - Current worker model: {}.\n\
         - If the user asks what model/system is running, say this is NOT Claude Code coinflip swarm mode and include the orchestrator and current worker model from this section.\n\
         - Do not claim to be the generic frontier model from the base environment prompt unless that exact model is listed in this section.",
        roles.planner_model, roles.coder_model
    )
}

/// Build a short summary of what the coder did, suitable for the reviewer.
/// Includes any tool_use names and the text content (first 1500 chars).
fn summarize_events(events: &[AssistantEvent]) -> String {
    let mut text = String::new();
    let mut tools: Vec<&str> = Vec::new();
    for e in events {
        match e {
            AssistantEvent::TextDelta(t) => text.push_str(t),
            AssistantEvent::ToolUse { name, .. } => tools.push(name),
            _ => {}
        }
    }
    let text_truncated: String = text.chars().take(1500).collect();
    let mut summary = String::new();
    if !tools.is_empty() {
        summary.push_str(&format!("Tools called: {}\n", tools.join(", ")));
    }
    if !text_truncated.is_empty() {
        summary.push_str("Text output:\n");
        summary.push_str(&text_truncated);
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Minimal scripted ApiClient used in tests — returns events from `script`
    /// in order, errors when exhausted.
    struct ScriptedClient {
        script: Vec<Vec<AssistantEvent>>,
    }

    impl ScriptedClient {
        fn new(script: Vec<Vec<AssistantEvent>>) -> Self {
            Self { script }
        }
    }

    impl ApiClient for ScriptedClient {
        fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            if self.script.is_empty() {
                return Err(RuntimeError::new("scripted client exhausted"));
            }
            Ok(self.script.remove(0))
        }
    }

    struct CapturingClient {
        seen: Arc<Mutex<Vec<ApiRequest>>>,
    }

    impl ApiClient for CapturingClient {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request);
            Ok(vec![
                AssistantEvent::TextDelta("hi".into()),
                AssistantEvent::MessageStop,
            ])
        }
    }

    #[test]
    fn passthrough_mode_prepends_banner_only() {
        let inner = ScriptedClient::new(vec![vec![
            AssistantEvent::TextDelta("hi".into()),
            AssistantEvent::MessageStop,
        ]]);
        let mut rt = OrchestratorRuntime::with_default_roles(Box::new(inner));
        let events = rt
            .stream(ApiRequest {
                system_prompt: vec![],
                messages: vec![],
            })
            .unwrap();
        assert_eq!(events.len(), 3, "banner + 2 inner events");
    }

    #[test]
    fn active_mode_skips_planning_when_no_user_message() {
        let inner = ScriptedClient::new(vec![vec![AssistantEvent::TextDelta("hi".into())]]);
        let mut rt =
            OrchestratorRuntime::with_orchestration_enabled(Box::new(inner), RoleConfig::default());
        // No user message → no planner call (which is good because there's
        // no Ollama running in unit tests anyway).
        let events = rt
            .stream(ApiRequest {
                system_prompt: vec![],
                messages: vec![],
            })
            .unwrap();
        assert_eq!(events, vec![AssistantEvent::TextDelta("hi".into())]);
    }

    #[test]
    fn active_mode_injects_coinflip_identity_context() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let inner = CapturingClient {
            seen: Arc::clone(&seen),
        };
        let roles = RoleConfig {
            planner_model: "gpt-5.5".into(),
            coder_model: "runpod-qwen36".into(),
            ..RoleConfig::default()
        };
        let mut rt = OrchestratorRuntime::with_orchestration_enabled(Box::new(inner), roles);

        let events = rt
            .stream(ApiRequest {
                system_prompt: vec!["Model family: Opus 4.6".into()],
                messages: vec![],
            })
            .unwrap();

        assert_eq!(
            events.first(),
            Some(&AssistantEvent::TextDelta("hi".into()))
        );
        let requests = seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let system = requests[0].system_prompt.join("\n");
        assert!(system.contains("Runtime mode: coinflip swarm"));
        assert!(system.contains("Orchestrator model: gpt-5.5"));
        assert!(system.contains("Current worker model: runpod-qwen36"));
        assert!(system.contains("Do not claim to be the generic frontier model"));
    }

    #[test]
    fn active_mode_does_not_inject_text_before_tool_use() {
        let tool_event = AssistantEvent::ToolUse {
            id: "call-1".into(),
            name: "write_file".into(),
            input: "{}".into(),
        };
        let inner =
            ScriptedClient::new(vec![vec![tool_event.clone(), AssistantEvent::MessageStop]]);
        let mut rt =
            OrchestratorRuntime::with_orchestration_enabled(Box::new(inner), RoleConfig::default());

        let events = rt
            .stream(ApiRequest {
                system_prompt: vec![],
                messages: vec![],
            })
            .unwrap();

        assert_eq!(events.first(), Some(&tool_event));
    }

    #[test]
    fn active_mode_skips_reviewer_after_tool_result_turns() {
        let inner = ScriptedClient::new(vec![vec![
            AssistantEvent::TextDelta("done".into()),
            AssistantEvent::MessageStop,
        ]]);
        let mut rt =
            OrchestratorRuntime::with_orchestration_enabled(Box::new(inner), RoleConfig::default());

        let events = rt
            .stream(ApiRequest {
                system_prompt: vec![],
                messages: vec![ConversationMessage::tool_result(
                    "call-1",
                    "write_file",
                    "ok",
                    false,
                )],
            })
            .unwrap();

        assert_eq!(
            events,
            vec![
                AssistantEvent::TextDelta("done".into()),
                AssistantEvent::MessageStop
            ]
        );
    }

    #[test]
    fn summarize_events_includes_tools_and_text() {
        let events = vec![
            AssistantEvent::TextDelta("done".into()),
            AssistantEvent::ToolUse {
                id: "1".into(),
                name: "write_file".into(),
                input: "{}".into(),
            },
        ];
        let summary = summarize_events(&events);
        assert!(summary.contains("write_file"));
        assert!(summary.contains("done"));
    }
}
