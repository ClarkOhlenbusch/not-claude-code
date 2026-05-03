//! Role-tag helpers.
//!
//! The orchestrator surfaces role transitions to the user as plain text
//! deltas with a `[role:NAME]` sentinel prefix. Existing renderers already
//! display text correctly; no new `StreamEvent` variants needed.
//!
//! In future phases we may filter `[role:*]` lines out of the persisted
//! assistant message so they don't pollute next-turn context. Phase 1 keeps
//! it simple — emit and persist as-is.

use runtime::AssistantEvent;

/// Marker characters for a role banner. Wrapped in newlines so the renderer
/// shows it on its own line.
fn banner(role: &str) -> String {
    format!("\n[role:{role}]\n")
}

/// Prepend a role-tag text-delta event to the front of an event list.
/// Mutates in place; idempotent if `events` is already prefixed (no-op).
pub fn prepend_role_banner(role: &str, events: &mut Vec<AssistantEvent>) {
    if events
        .first()
        .is_some_and(|first| matches!(first, AssistantEvent::TextDelta(t) if t.starts_with("[role:") || t.starts_with("\n[role:")))
    {
        return;
    }
    events.insert(0, AssistantEvent::TextDelta(banner(role)));
}
