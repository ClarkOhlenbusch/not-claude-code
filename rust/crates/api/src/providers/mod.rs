use std::future::Future;
use std::pin::Pin;

use crate::error::ApiError;
use crate::types::{MessageRequest, MessageResponse};

pub mod claw_provider;
pub mod openai_compat;

pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ApiError>> + Send + 'a>>;

pub trait Provider {
    type Stream;

    fn send_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, MessageResponse>;

    fn stream_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, Self::Stream>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    ClawApi,
    Xai,
    OpenAi,
    Ollama,
    ComputeCommunity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub provider: ProviderKind,
    pub auth_env: &'static str,
    pub base_url_env: &'static str,
    pub default_base_url: &'static str,
}

const MODEL_REGISTRY: &[(&str, ProviderMetadata)] = &[
    (
        "opus",
        ProviderMetadata {
            provider: ProviderKind::ClawApi,
            auth_env: "ANTHROPIC_API_KEY",
            base_url_env: "ANTHROPIC_BASE_URL",
            default_base_url: claw_provider::DEFAULT_BASE_URL,
        },
    ),
    (
        "sonnet",
        ProviderMetadata {
            provider: ProviderKind::ClawApi,
            auth_env: "ANTHROPIC_API_KEY",
            base_url_env: "ANTHROPIC_BASE_URL",
            default_base_url: claw_provider::DEFAULT_BASE_URL,
        },
    ),
    (
        "haiku",
        ProviderMetadata {
            provider: ProviderKind::ClawApi,
            auth_env: "ANTHROPIC_API_KEY",
            base_url_env: "ANTHROPIC_BASE_URL",
            default_base_url: claw_provider::DEFAULT_BASE_URL,
        },
    ),
    (
        "claude-opus-4-6",
        ProviderMetadata {
            provider: ProviderKind::ClawApi,
            auth_env: "ANTHROPIC_API_KEY",
            base_url_env: "ANTHROPIC_BASE_URL",
            default_base_url: claw_provider::DEFAULT_BASE_URL,
        },
    ),
    (
        "claude-sonnet-4-6",
        ProviderMetadata {
            provider: ProviderKind::ClawApi,
            auth_env: "ANTHROPIC_API_KEY",
            base_url_env: "ANTHROPIC_BASE_URL",
            default_base_url: claw_provider::DEFAULT_BASE_URL,
        },
    ),
    (
        "claude-haiku-4-5-20251213",
        ProviderMetadata {
            provider: ProviderKind::ClawApi,
            auth_env: "ANTHROPIC_API_KEY",
            base_url_env: "ANTHROPIC_BASE_URL",
            default_base_url: claw_provider::DEFAULT_BASE_URL,
        },
    ),
    (
        "grok",
        ProviderMetadata {
            provider: ProviderKind::Xai,
            auth_env: "XAI_API_KEY",
            base_url_env: "XAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        },
    ),
    (
        "grok-3",
        ProviderMetadata {
            provider: ProviderKind::Xai,
            auth_env: "XAI_API_KEY",
            base_url_env: "XAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        },
    ),
    (
        "grok-mini",
        ProviderMetadata {
            provider: ProviderKind::Xai,
            auth_env: "XAI_API_KEY",
            base_url_env: "XAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        },
    ),
    (
        "grok-3-mini",
        ProviderMetadata {
            provider: ProviderKind::Xai,
            auth_env: "XAI_API_KEY",
            base_url_env: "XAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        },
    ),
    (
        "grok-2",
        ProviderMetadata {
            provider: ProviderKind::Xai,
            auth_env: "XAI_API_KEY",
            base_url_env: "XAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        },
    ),
    (
        "gpt",
        ProviderMetadata {
            provider: ProviderKind::OpenAi,
            auth_env: "OPENAI_API_KEY",
            base_url_env: "OPENAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OPENAI_BASE_URL,
        },
    ),
    (
        "gpt-5.5",
        ProviderMetadata {
            provider: ProviderKind::OpenAi,
            auth_env: "OPENAI_API_KEY",
            base_url_env: "OPENAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OPENAI_BASE_URL,
        },
    ),
    (
        "gpt-5.5-pro",
        ProviderMetadata {
            provider: ProviderKind::OpenAi,
            auth_env: "OPENAI_API_KEY",
            base_url_env: "OPENAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OPENAI_BASE_URL,
        },
    ),
    (
        "gpt-5.4",
        ProviderMetadata {
            provider: ProviderKind::OpenAi,
            auth_env: "OPENAI_API_KEY",
            base_url_env: "OPENAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OPENAI_BASE_URL,
        },
    ),
    (
        "gpt-5.4-mini",
        ProviderMetadata {
            provider: ProviderKind::OpenAi,
            auth_env: "OPENAI_API_KEY",
            base_url_env: "OPENAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OPENAI_BASE_URL,
        },
    ),
    (
        "qwen36",
        ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_QWEN_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_QWEN_BASE_URL,
        },
    ),
    (
        "qwen3.6",
        ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_QWEN_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_QWEN_BASE_URL,
        },
    ),
    (
        "runpod-qwen36",
        ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_QWEN_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_QWEN_BASE_URL,
        },
    ),
    (
        "qwen/qwen3.6-35b-a3b-fp8",
        ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_QWEN_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_QWEN_BASE_URL,
        },
    ),
    (
        "gemma",
        ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_GEMMA4_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_GEMMA4_BASE_URL,
        },
    ),
    (
        "gemma4",
        ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_GEMMA4_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_GEMMA4_BASE_URL,
        },
    ),
    (
        "gemma4-31b",
        ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_GEMMA4_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_GEMMA4_BASE_URL,
        },
    ),
    (
        "runpod-gemma4-31b",
        ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_GEMMA4_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_GEMMA4_BASE_URL,
        },
    ),
    (
        "google/gemma-4-31b-it",
        ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_GEMMA4_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_GEMMA4_BASE_URL,
        },
    ),
    (
        "qwen-coder",
        ProviderMetadata {
            provider: ProviderKind::Ollama,
            auth_env: "OLLAMA_API_KEY",
            base_url_env: "OLLAMA_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OLLAMA_BASE_URL,
        },
    ),
    (
        "qwen3-coder:30b",
        ProviderMetadata {
            provider: ProviderKind::Ollama,
            auth_env: "OLLAMA_API_KEY",
            base_url_env: "OLLAMA_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OLLAMA_BASE_URL,
        },
    ),
    (
        "glm-flash",
        ProviderMetadata {
            provider: ProviderKind::Ollama,
            auth_env: "OLLAMA_API_KEY",
            base_url_env: "OLLAMA_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OLLAMA_BASE_URL,
        },
    ),
    (
        "glm-4.7-flash:q4",
        ProviderMetadata {
            provider: ProviderKind::Ollama,
            auth_env: "OLLAMA_API_KEY",
            base_url_env: "OLLAMA_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OLLAMA_BASE_URL,
        },
    ),
    (
        "qwen2.5-coder:14b",
        ProviderMetadata {
            provider: ProviderKind::Ollama,
            auth_env: "OLLAMA_API_KEY",
            base_url_env: "OLLAMA_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OLLAMA_BASE_URL,
        },
    ),
];

#[must_use]
pub fn resolve_model_alias(model: &str) -> String {
    let trimmed = model.trim();
    let lower = trimmed.to_ascii_lowercase();
    MODEL_REGISTRY
        .iter()
        .find_map(|(alias, metadata)| {
            (*alias == lower).then_some(match metadata.provider {
                ProviderKind::ClawApi => match *alias {
                    "opus" => "claude-opus-4-6",
                    "sonnet" => "claude-sonnet-4-6",
                    "haiku" => "claude-haiku-4-5-20251213",
                    _ => trimmed,
                },
                ProviderKind::Xai => match *alias {
                    "grok" | "grok-3" => "grok-3",
                    "grok-mini" | "grok-3-mini" => "grok-3-mini",
                    "grok-2" => "grok-2",
                    _ => trimmed,
                },
                ProviderKind::OpenAi => match *alias {
                    "gpt" => "gpt-5.5",
                    _ => trimmed,
                },
                ProviderKind::ComputeCommunity => match *alias {
                    "qwen36" | "qwen3.6" | "runpod-qwen36" => "Qwen/Qwen3.6-35B-A3B-FP8",
                    "gemma" | "gemma4" | "gemma4-31b" | "runpod-gemma4-31b" => {
                        "google/gemma-4-31B-it"
                    }
                    _ => trimmed,
                },
                ProviderKind::Ollama => match *alias {
                    "qwen-coder" => "qwen3-coder:30b",
                    "glm-flash" => "glm-4.7-flash:q4",
                    _ => trimmed,
                },
            })
        })
        .map_or_else(|| trimmed.to_string(), ToOwned::to_owned)
}

#[must_use]
pub fn metadata_for_model(model: &str) -> Option<ProviderMetadata> {
    let canonical = resolve_model_alias(model);
    let lower = canonical.to_ascii_lowercase();
    if let Some((_, metadata)) = MODEL_REGISTRY.iter().find(|(alias, _)| *alias == lower) {
        return Some(*metadata);
    }
    if lower.starts_with("grok") {
        return Some(ProviderMetadata {
            provider: ProviderKind::Xai,
            auth_env: "XAI_API_KEY",
            base_url_env: "XAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
        });
    }
    if lower.starts_with("gpt-") {
        return Some(ProviderMetadata {
            provider: ProviderKind::OpenAi,
            auth_env: "OPENAI_API_KEY",
            base_url_env: "OPENAI_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OPENAI_BASE_URL,
        });
    }
    if lower == "qwen/qwen3.6-35b-a3b-fp8" {
        return Some(ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_QWEN_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_QWEN_BASE_URL,
        });
    }
    if lower == "google/gemma-4-31b-it" {
        return Some(ProviderMetadata {
            provider: ProviderKind::ComputeCommunity,
            auth_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_GEMMA4_BASE_URL",
            default_base_url: openai_compat::DEFAULT_COMPUTE_COMMUNITY_GEMMA4_BASE_URL,
        });
    }
    if lower.contains(':') && !lower.starts_with("claude") {
        return Some(ProviderMetadata {
            provider: ProviderKind::Ollama,
            auth_env: "OLLAMA_API_KEY",
            base_url_env: "OLLAMA_BASE_URL",
            default_base_url: openai_compat::DEFAULT_OLLAMA_BASE_URL,
        });
    }
    None
}

#[must_use]
pub fn detect_provider_kind(model: &str) -> ProviderKind {
    if let Some(metadata) = metadata_for_model(model) {
        return metadata.provider;
    }
    if claw_provider::has_auth_from_env_or_saved().unwrap_or(false) {
        return ProviderKind::ClawApi;
    }
    if openai_compat::has_api_key("OPENAI_API_KEY") {
        return ProviderKind::OpenAi;
    }
    if openai_compat::has_api_key("XAI_API_KEY") {
        return ProviderKind::Xai;
    }
    ProviderKind::ClawApi
}

#[must_use]
pub fn max_tokens_for_model(model: &str) -> u32 {
    let canonical = resolve_model_alias(model);
    if canonical.eq_ignore_ascii_case("Qwen/Qwen3.6-35B-A3B-FP8") {
        16_000
    } else if canonical.eq_ignore_ascii_case("google/gemma-4-31B-it") {
        16_000
    } else if canonical.contains("opus") {
        32_000
    } else {
        64_000
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_provider_kind, max_tokens_for_model, resolve_model_alias, ProviderKind};

    #[test]
    fn resolves_grok_aliases() {
        assert_eq!(resolve_model_alias("grok"), "grok-3");
        assert_eq!(resolve_model_alias("grok-mini"), "grok-3-mini");
        assert_eq!(resolve_model_alias("grok-2"), "grok-2");
    }

    #[test]
    fn resolves_compute_qwen_aliases() {
        assert_eq!(resolve_model_alias("qwen36"), "Qwen/Qwen3.6-35B-A3B-FP8");
        assert_eq!(
            resolve_model_alias("runpod-qwen36"),
            "Qwen/Qwen3.6-35B-A3B-FP8"
        );
        assert_eq!(
            detect_provider_kind("qwen36"),
            ProviderKind::ComputeCommunity
        );
    }

    #[test]
    fn resolves_compute_gemma4_aliases() {
        assert_eq!(resolve_model_alias("gemma"), "google/gemma-4-31B-it");
        assert_eq!(
            resolve_model_alias("runpod-gemma4-31b"),
            "google/gemma-4-31B-it"
        );
        assert_eq!(
            detect_provider_kind("gemma4-31b"),
            ProviderKind::ComputeCommunity
        );
    }

    #[test]
    fn detects_provider_from_model_name_first() {
        assert_eq!(detect_provider_kind("grok"), ProviderKind::Xai);
        assert_eq!(
            detect_provider_kind("claude-sonnet-4-6"),
            ProviderKind::ClawApi
        );
    }

    #[test]
    fn keeps_existing_max_token_heuristic() {
        assert_eq!(max_tokens_for_model("opus"), 32_000);
        assert_eq!(max_tokens_for_model("grok-3"), 64_000);
    }
}
