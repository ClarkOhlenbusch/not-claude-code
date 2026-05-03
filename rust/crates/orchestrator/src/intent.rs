//! Phase 2 — intent classifier.
//!
//! TODO: implement `classify(user_msg: &str) -> Intent` calling the small
//! intent model with a strict-JSON prompt that returns one of the enum
//! variants below. On parse failure, default to `MultiStep` (safe).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// Plain conversation; no tools needed. Skip planning, route direct to coder.
    Chat,
    /// Single targeted file or shell action. Skip detailed planning.
    SingleEdit,
    /// Multi-file or multi-step work. Full planner → coder → reviewer cycle.
    MultiStep,
    /// Read-heavy investigation. Plan with grep/glob/web tool emphasis.
    Search,
}

impl Intent {
    #[must_use]
    pub fn needs_planning(self) -> bool {
        !matches!(self, Self::Chat)
    }
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
}
