//! NOT Claude Code — multi-model orchestrator.
//!
//! This crate replaces the single-model agent loop with a chain of role-
//! specialized models: intent → planner → coder/tools → reviewer. Each role
//! is configurable via [`RoleConfig`]. The whole orchestrator runs as one
//! [`runtime::ApiClient`] implementation, so the surrounding `ConversationRuntime`
//! at `crates/runtime/src/conversation.rs:166-255` keeps driving the
//! tool-dispatch loop without changes.
//!
//! # Phase status
//!
//! - **Phase 1 (this commit):** skeleton. `OrchestratorRuntime` wraps a single
//!   inner `ApiClient` and prefixes its first text emission with a role tag.
//!   No real role specialization yet — the goal is to prove the integration
//!   point works end-to-end.
//! - **Phase 2:** intent classifier (small chat model routes chat vs. multi-step).
//! - **Phase 3:** structured planner + per-step coder + deterministic verifier.
//! - **Phase 4:** reviewer LLM + retry/replan loop.
//! - **Phase 5:** subagent primitive with isolated message threads.
//! - **Phase 6:** bake-off bench across single-model baselines.
//!
//! See `docs/claude-code-divergences.md` for the full design rationale.

pub mod coder;
pub mod events;
pub mod intent;
pub mod plan;
pub mod planner;
pub mod reviewer;
pub mod roles;
pub mod runtime;
pub mod verifier;

pub use roles::RoleConfig;
pub use runtime::OrchestratorRuntime;
