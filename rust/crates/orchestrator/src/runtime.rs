//! Orchestrator runtime — implements `runtime::ApiClient`.
//!
//! Phase 1: thin wrapper around an inner `ApiClient`. It delegates the
//! `stream()` call straight through and prefixes the resulting events with
//! a `[role:coder]` banner. No real specialization yet — this is the
//! integration scaffold so later phases can land incrementally.
//!
//! Later phases will replace the single inner client with role-specific
//! `ProviderClient` calls (intent → planner → coder → reviewer) plus
//! orchestrator state (current Plan, step index, retry counts) on `&mut self`.

use runtime::{ApiClient, ApiRequest, AssistantEvent, RuntimeError};

use crate::events::prepend_role_banner;
use crate::roles::RoleConfig;

/// Multi-model orchestrator. Phase 1: single-model passthrough with role tags.
pub struct OrchestratorRuntime {
    inner: Box<dyn ApiClient + Send>,
    roles: RoleConfig,
}

impl OrchestratorRuntime {
    /// Construct an orchestrator that delegates every call to `inner`.
    /// Phase 1 ignores `roles` aside from carrying it for later phases —
    /// every model invocation goes through the inner client regardless.
    pub fn new(inner: Box<dyn ApiClient + Send>, roles: RoleConfig) -> Self {
        Self { inner, roles }
    }

    /// Convenience: build with default roles. Useful for the smallest possible
    /// integration in `claw-cli/main.rs` once Phase 1 wiring lands.
    pub fn with_default_roles(inner: Box<dyn ApiClient + Send>) -> Self {
        Self::new(inner, RoleConfig::default())
    }

    /// Read-only access to the role configuration. Later phases will use this
    /// to decide which model to call for which sub-step.
    #[must_use]
    pub fn roles(&self) -> &RoleConfig {
        &self.roles
    }
}

impl ApiClient for OrchestratorRuntime {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let mut events = self.inner.stream(request)?;
        // Phase 1: every model call is the "coder" role since there's only one
        // call per outer-loop iteration. Phase 2+ will branch here based on
        // orchestrator state (intent classification, current plan step, etc).
        prepend_role_banner("coder", &mut events);
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal scripted ApiClient used in tests — returns the events handed
    /// to `new()` once, then errors on subsequent calls.
    struct ScriptedClient {
        events: Option<Vec<AssistantEvent>>,
    }

    impl ScriptedClient {
        fn new(events: Vec<AssistantEvent>) -> Self {
            Self {
                events: Some(events),
            }
        }
    }

    impl ApiClient for ScriptedClient {
        fn stream(&mut self, _request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            self.events
                .take()
                .ok_or_else(|| RuntimeError::new("scripted client called twice"))
        }
    }

    #[test]
    fn stream_prepends_role_banner_for_coder() {
        let inner = ScriptedClient::new(vec![
            AssistantEvent::TextDelta("hello".to_string()),
            AssistantEvent::MessageStop,
        ]);
        let mut runtime = OrchestratorRuntime::with_default_roles(Box::new(inner));
        let events = runtime
            .stream(ApiRequest {
                system_prompt: vec![],
                messages: vec![],
            })
            .expect("stream succeeded");

        assert_eq!(events.len(), 3, "expected banner + original 2 events");
        match &events[0] {
            AssistantEvent::TextDelta(text) => {
                assert!(
                    text.contains("[role:coder]"),
                    "first event should be a role banner, got: {text:?}"
                );
            }
            other => panic!("expected text delta banner, got {other:?}"),
        }
    }

    #[test]
    fn stream_does_not_duplicate_banner_on_already_tagged_events() {
        // If the inner client somehow already returned a tagged first event
        // (won't happen in Phase 1 but guards against re-entrancy in later
        // phases), we should not double-tag.
        let inner = ScriptedClient::new(vec![
            AssistantEvent::TextDelta("[role:already]\n".to_string()),
            AssistantEvent::TextDelta("body".to_string()),
        ]);
        let mut runtime = OrchestratorRuntime::with_default_roles(Box::new(inner));
        let events = runtime
            .stream(ApiRequest {
                system_prompt: vec![],
                messages: vec![],
            })
            .expect("stream succeeded");

        assert_eq!(events.len(), 2, "should not have added a second banner");
    }

    #[test]
    fn role_config_default_uses_baseline_model() {
        let config = RoleConfig::default();
        assert_eq!(config.intent_model, "qwen2.5-coder:14b");
        assert_eq!(config.planner_model, "qwen2.5-coder:14b");
        assert_eq!(config.coder_model, "qwen2.5-coder:14b");
        assert_eq!(config.reviewer_model, "qwen2.5-coder:14b");
    }

    #[test]
    fn role_config_distinct_models_uses_three_different_models() {
        let config = RoleConfig::distinct_models();
        assert_ne!(config.intent_model, config.planner_model);
        assert_ne!(config.planner_model, config.coder_model);
        // planner and reviewer share by design — the user's stated preference.
        assert_eq!(config.planner_model, config.reviewer_model);
    }
}
