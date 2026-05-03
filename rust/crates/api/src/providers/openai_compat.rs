use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::ApiError;
use crate::types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest,
    MessageResponse, MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent,
    ToolChoice, ToolDefinition, ToolResultContentBlock, Usage,
};

use super::{Provider, ProviderFuture};

pub const DEFAULT_XAI_BASE_URL: &str = "https://api.x.ai/v1";
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_CODEX_RESPONSES_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434/v1";
pub const DEFAULT_COMPUTE_COMMUNITY_QWEN_BASE_URL: &str =
    "https://computecommunity.com/u/C7XfWXayLelTkySS7to8stLtwvV3Lj3J/nodes/runpod-qwen3-5-35b/v1";
pub const DEFAULT_COMPUTE_COMMUNITY_GEMMA4_BASE_URL: &str =
    "https://computecommunity.com/u/CTFaQ3cxUcRbXCpuqCASNw9Y5xrU0LdQ/nodes/runpod-gemma-4-31b/v1";
const OLLAMA_PLACEHOLDER_KEY: &str = "ollama";
const REQUEST_ID_HEADER: &str = "request-id";
const ALT_REQUEST_ID_HEADER: &str = "x-request-id";
const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_millis(200);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(2);
const DEFAULT_MAX_RETRIES: u32 = 2;
const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_CODEX_OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_AUTH_EXPIRY_SKEW_SECS: u64 = 5 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenAiCompatConfig {
    pub provider_name: &'static str,
    pub api_key_env: &'static str,
    pub base_url_env: &'static str,
    pub default_base_url: &'static str,
}

const XAI_ENV_VARS: &[&str] = &["XAI_API_KEY"];
const OPENAI_ENV_VARS: &[&str] = &["OPENAI_API_KEY", "~/.codex/auth.json"];
const COMPUTE_COMMUNITY_ENV_VARS: &[&str] = &[
    "COMPUTE_COMMUNITY_API_KEY",
    "COMPUTECOMMUNITY_API_KEY",
    "CC_API_KEY",
    "RUNPOD_QWEN_API_KEY",
];

impl OpenAiCompatConfig {
    #[must_use]
    pub const fn xai() -> Self {
        Self {
            provider_name: "xAI",
            api_key_env: "XAI_API_KEY",
            base_url_env: "XAI_BASE_URL",
            default_base_url: DEFAULT_XAI_BASE_URL,
        }
    }

    #[must_use]
    pub const fn openai() -> Self {
        Self {
            provider_name: "OpenAI",
            api_key_env: "OPENAI_API_KEY",
            base_url_env: "OPENAI_BASE_URL",
            default_base_url: DEFAULT_OPENAI_BASE_URL,
        }
    }

    #[must_use]
    pub const fn ollama() -> Self {
        Self {
            provider_name: "Ollama",
            api_key_env: "OLLAMA_API_KEY",
            base_url_env: "OLLAMA_BASE_URL",
            default_base_url: DEFAULT_OLLAMA_BASE_URL,
        }
    }

    #[must_use]
    pub const fn compute_community_qwen() -> Self {
        Self {
            provider_name: "ComputeCommunity",
            api_key_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_QWEN_BASE_URL",
            default_base_url: DEFAULT_COMPUTE_COMMUNITY_QWEN_BASE_URL,
        }
    }

    #[must_use]
    pub const fn compute_community_gemma4() -> Self {
        Self {
            provider_name: "ComputeCommunity",
            api_key_env: "COMPUTE_COMMUNITY_API_KEY",
            base_url_env: "COMPUTE_COMMUNITY_GEMMA4_BASE_URL",
            default_base_url: DEFAULT_COMPUTE_COMMUNITY_GEMMA4_BASE_URL,
        }
    }

    #[must_use]
    pub fn credential_env_vars(self) -> &'static [&'static str] {
        match self.provider_name {
            "xAI" => XAI_ENV_VARS,
            "OpenAI" => OPENAI_ENV_VARS,
            "ComputeCommunity" => COMPUTE_COMMUNITY_ENV_VARS,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatClient {
    http: reqwest::Client,
    credential: OpenAiCredential,
    base_url: String,
    max_retries: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
}

#[derive(Debug, Clone)]
enum OpenAiCredential {
    Bearer(String),
    CodexOAuth,
}

impl OpenAiCompatClient {
    #[must_use]
    pub fn new(api_key: impl Into<String>, config: OpenAiCompatConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            credential: OpenAiCredential::Bearer(api_key.into()),
            base_url: read_base_url(config),
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
        }
    }

    pub fn from_env(config: OpenAiCompatConfig) -> Result<Self, ApiError> {
        if config.provider_name == "Ollama" {
            let api_key = read_env_non_empty(config.api_key_env)?
                .unwrap_or_else(|| OLLAMA_PLACEHOLDER_KEY.to_string());
            return Ok(Self::new(api_key, config));
        }
        if let Some(api_key) = read_first_env_non_empty(config.credential_env_vars())? {
            return Ok(Self::new(api_key, config));
        }
        if config.provider_name == "OpenAI" && has_codex_openai_auth() {
            return Ok(Self {
                http: reqwest::Client::new(),
                credential: OpenAiCredential::CodexOAuth,
                base_url: read_codex_responses_base_url(),
                max_retries: DEFAULT_MAX_RETRIES,
                initial_backoff: DEFAULT_INITIAL_BACKOFF,
                max_backoff: DEFAULT_MAX_BACKOFF,
            });
        }
        Err(ApiError::missing_credentials(
            config.provider_name,
            config.credential_env_vars(),
        ))
    }

    #[must_use]
    fn uses_codex_oauth(&self) -> bool {
        matches!(self.credential, OpenAiCredential::CodexOAuth)
    }

    async fn resolve_auth(&self, force_refresh: bool) -> Result<ResolvedAuth, ApiError> {
        match &self.credential {
            OpenAiCredential::Bearer(token) => Ok(ResolvedAuth {
                access_token: token.clone(),
                account_id: None,
            }),
            OpenAiCredential::CodexOAuth => {
                resolve_codex_openai_auth(&self.http, force_refresh).await
            }
        }
    }

    async fn refresh_codex_auth(&self) -> Result<(), ApiError> {
        if self.uses_codex_oauth() {
            self.resolve_auth(true).await?;
        }
        Ok(())
    }

    fn unauthorized_error(error: &ApiError) -> bool {
        matches!(
            error,
            ApiError::Api {
                status,
                ..
            } if *status == reqwest::StatusCode::UNAUTHORIZED
        )
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    #[must_use]
    pub fn with_retry_policy(
        mut self,
        max_retries: u32,
        initial_backoff: Duration,
        max_backoff: Duration,
    ) -> Self {
        self.max_retries = max_retries;
        self.initial_backoff = initial_backoff;
        self.max_backoff = max_backoff;
        self
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        if self.uses_codex_oauth() {
            return self.send_message_via_stream(request).await;
        }
        let request = MessageRequest {
            stream: false,
            ..request.clone()
        };
        let response = self.send_with_retry(&request).await?;
        let request_id = request_id_from_headers(response.headers());
        let payload = response.json::<ChatCompletionResponse>().await?;
        let mut normalized = normalize_response(&request.model, payload)?;
        if normalized.request_id.is_none() {
            normalized.request_id = request_id;
        }
        Ok(normalized)
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        let response = self
            .send_with_retry(&request.clone().with_streaming())
            .await?;
        Ok(MessageStream {
            request_id: request_id_from_headers(response.headers()),
            response,
            parser: if self.uses_codex_oauth() {
                SseParserKind::CodexResponses(CodexResponsesSseParser::new())
            } else {
                SseParserKind::ChatCompletions(OpenAiSseParser::new())
            },
            pending: VecDeque::new(),
            done: false,
            state: StreamState::new(request.model.clone()),
        })
    }

    async fn send_with_retry(
        &self,
        request: &MessageRequest,
    ) -> Result<reqwest::Response, ApiError> {
        let mut attempts = 0;

        let last_error = loop {
            attempts += 1;
            let retryable_error = match self.send_raw_request(request).await {
                Ok(response) => match expect_success(response).await {
                    Ok(response) => return Ok(response),
                    Err(error)
                        if Self::unauthorized_error(&error)
                            && self.uses_codex_oauth()
                            && attempts <= self.max_retries + 1 =>
                    {
                        self.refresh_codex_auth().await?;
                        error
                    }
                    Err(error) if error.is_retryable() && attempts <= self.max_retries + 1 => error,
                    Err(error) => return Err(error),
                },
                Err(error) if error.is_retryable() && attempts <= self.max_retries + 1 => error,
                Err(error) => return Err(error),
            };

            if attempts > self.max_retries {
                break retryable_error;
            }

            tokio::time::sleep(self.backoff_for_attempt(attempts)?).await;
        };

        Err(ApiError::RetriesExhausted {
            attempts,
            last_error: Box::new(last_error),
        })
    }

    async fn send_raw_request(
        &self,
        request: &MessageRequest,
    ) -> Result<reqwest::Response, ApiError> {
        let request_url = if self.uses_codex_oauth() {
            codex_responses_endpoint(&self.base_url)
        } else {
            chat_completions_endpoint(&self.base_url)
        };
        let auth = self.resolve_auth(false).await?;
        let mut builder = self
            .http
            .post(&request_url)
            .header("content-type", "application/json")
            .bearer_auth(&auth.access_token);
        if let Some(account_id) = auth.account_id {
            builder = builder.header("ChatGPT-Account-Id", account_id);
        }
        let payload = if self.uses_codex_oauth() {
            build_codex_responses_request(request)
        } else {
            build_chat_completion_request(request)
        };
        builder.json(&payload).send().await.map_err(ApiError::from)
    }

    async fn send_message_via_stream(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        let mut stream = self.stream_message(request).await?;
        let mut response: Option<MessageResponse> = None;
        let mut text = String::new();
        let mut stop_reason = None;
        let mut usage = Usage {
            input_tokens: 0,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            output_tokens: 0,
        };

        while let Some(event) = stream.next_event().await? {
            match event {
                StreamEvent::MessageStart(event) => response = Some(event.message),
                StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    delta: ContentBlockDelta::TextDelta { text: delta },
                    ..
                }) => text.push_str(&delta),
                StreamEvent::MessageDelta(event) => {
                    stop_reason = event.delta.stop_reason;
                    usage = event.usage;
                }
                _ => {}
            }
        }

        let mut response = response.unwrap_or_else(|| MessageResponse {
            id: "response".to_string(),
            kind: "message".to_string(),
            role: "assistant".to_string(),
            content: Vec::new(),
            model: request.model.clone(),
            stop_reason: None,
            stop_sequence: None,
            usage: usage.clone(),
            request_id: None,
        });
        if !text.is_empty() {
            response.content = vec![OutputContentBlock::Text { text }];
        }
        response.stop_reason = stop_reason;
        response.usage = usage;
        Ok(response)
    }

    fn backoff_for_attempt(&self, attempt: u32) -> Result<Duration, ApiError> {
        let Some(multiplier) = 1_u32.checked_shl(attempt.saturating_sub(1)) else {
            return Err(ApiError::BackoffOverflow {
                attempt,
                base_delay: self.initial_backoff,
            });
        };
        Ok(self
            .initial_backoff
            .checked_mul(multiplier)
            .map_or(self.max_backoff, |delay| delay.min(self.max_backoff)))
    }
}

impl Provider for OpenAiCompatClient {
    type Stream = MessageStream;

    fn send_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, MessageResponse> {
        Box::pin(async move { self.send_message(request).await })
    }

    fn stream_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, Self::Stream> {
        Box::pin(async move { self.stream_message(request).await })
    }
}

#[derive(Debug)]
pub struct MessageStream {
    request_id: Option<String>,
    response: reqwest::Response,
    parser: SseParserKind,
    pending: VecDeque<StreamEvent>,
    done: bool,
    state: StreamState,
}

#[derive(Debug)]
enum SseParserKind {
    ChatCompletions(OpenAiSseParser),
    CodexResponses(CodexResponsesSseParser),
}

impl SseParserKind {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<ChatCompletionChunk>, ApiError> {
        match self {
            Self::ChatCompletions(parser) => parser.push(chunk),
            Self::CodexResponses(parser) => parser.push(chunk),
        }
    }
}

impl MessageStream {
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }

            if self.done {
                self.pending.extend(self.state.finish()?);
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                return Ok(None);
            }

            match self.response.chunk().await? {
                Some(chunk) => {
                    for parsed in self.parser.push(&chunk)? {
                        self.pending.extend(self.state.ingest_chunk(parsed)?);
                    }
                }
                None => {
                    self.done = true;
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct OpenAiSseParser {
    buffer: Vec<u8>,
}

#[derive(Debug, Default)]
struct CodexResponsesSseParser {
    buffer: Vec<u8>,
    response_id: Option<String>,
    model: Option<String>,
    tool_calls: BTreeMap<String, CodexToolCallState>,
}

impl CodexResponsesSseParser {
    fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<ChatCompletionChunk>, ApiError> {
        self.buffer.extend_from_slice(chunk);
        let mut chunks = Vec::new();

        while let Some(frame) = next_sse_frame(&mut self.buffer) {
            if let Some(event) = parse_codex_response_sse_frame(&frame)? {
                match event {
                    CodexResponseEvent::Created { id, model } => {
                        self.response_id = Some(id.clone());
                        self.model = Some(model.clone());
                        chunks.push(ChatCompletionChunk {
                            id,
                            model: Some(model),
                            choices: Vec::new(),
                            usage: None,
                        });
                    }
                    CodexResponseEvent::TextDelta { delta } => {
                        chunks.push(ChatCompletionChunk {
                            id: self
                                .response_id
                                .clone()
                                .unwrap_or_else(|| "codex-response".to_string()),
                            model: self.model.clone(),
                            choices: vec![ChunkChoice {
                                delta: ChunkDelta {
                                    content: Some(delta),
                                    tool_calls: Vec::new(),
                                },
                                finish_reason: None,
                            }],
                            usage: None,
                        });
                    }
                    CodexResponseEvent::ToolCallStarted {
                        item_id,
                        index,
                        call_id,
                        name,
                    } => {
                        self.tool_calls.insert(
                            item_id,
                            CodexToolCallState {
                                index,
                                call_id,
                                name,
                                arguments: String::new(),
                            },
                        );
                    }
                    CodexResponseEvent::ToolArgumentsDelta { item_id, delta } => {
                        if let Some(tool_call) = self.tool_calls.get_mut(&item_id) {
                            tool_call.arguments.push_str(&delta);
                            chunks.push(ChatCompletionChunk {
                                id: self
                                    .response_id
                                    .clone()
                                    .unwrap_or_else(|| "codex-response".to_string()),
                                model: self.model.clone(),
                                choices: vec![ChunkChoice {
                                    delta: ChunkDelta {
                                        content: None,
                                        tool_calls: vec![DeltaToolCall {
                                            index: tool_call.index,
                                            id: None,
                                            function: DeltaFunction {
                                                name: None,
                                                arguments: Some(delta),
                                            },
                                        }],
                                    },
                                    finish_reason: None,
                                }],
                                usage: None,
                            });
                        }
                    }
                    CodexResponseEvent::ToolCallDone {
                        item_id,
                        call_id,
                        name,
                        arguments,
                    } => {
                        let tool_call =
                            self.tool_calls
                                .remove(&item_id)
                                .unwrap_or(CodexToolCallState {
                                    index: 0,
                                    call_id,
                                    name,
                                    arguments,
                                });
                        chunks.push(ChatCompletionChunk {
                            id: self
                                .response_id
                                .clone()
                                .unwrap_or_else(|| "codex-response".to_string()),
                            model: self.model.clone(),
                            choices: vec![ChunkChoice {
                                delta: ChunkDelta {
                                    content: None,
                                    tool_calls: vec![DeltaToolCall {
                                        index: tool_call.index,
                                        id: Some(tool_call.call_id),
                                        function: DeltaFunction {
                                            name: Some(tool_call.name),
                                            arguments: Some(String::new()),
                                        },
                                    }],
                                },
                                finish_reason: Some("tool_calls".to_string()),
                            }],
                            usage: None,
                        });
                    }
                    CodexResponseEvent::Completed { usage } => {
                        chunks.push(ChatCompletionChunk {
                            id: self
                                .response_id
                                .clone()
                                .unwrap_or_else(|| "codex-response".to_string()),
                            model: self.model.clone(),
                            choices: vec![ChunkChoice {
                                delta: ChunkDelta::default(),
                                finish_reason: Some("stop".to_string()),
                            }],
                            usage,
                        });
                    }
                }
            }
        }

        Ok(chunks)
    }
}

#[derive(Debug)]
struct CodexToolCallState {
    index: u32,
    call_id: String,
    name: String,
    arguments: String,
}

impl OpenAiSseParser {
    fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<ChatCompletionChunk>, ApiError> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(frame) = next_sse_frame(&mut self.buffer) {
            if let Some(event) = parse_sse_frame(&frame)? {
                events.push(event);
            }
        }

        Ok(events)
    }
}

/// Some OpenAI-compatible servers (Ollama + Qwen, e.g.) don't emit structured
/// `tool_calls` for tool-trained models — they emit the call as JSON in the
/// text content. We buffer text that starts with `{` and try to parse it as a
/// tool call on stream finish, synthesizing the `tool_use` events the rest of
/// claw expects.
#[derive(Debug, PartialEq, Eq)]
enum TextBufferMode {
    /// Haven't seen the first non-whitespace char yet — decide on first non-WS.
    Pending,
    /// First non-WS char was `{` — accumulating to attempt tool-call parse on finish.
    Buffering,
    /// First non-WS char was something else — pass text deltas through normally.
    PassThrough,
}

#[derive(Debug)]
struct StreamState {
    model: String,
    message_started: bool,
    text_started: bool,
    text_finished: bool,
    finished: bool,
    stop_reason: Option<String>,
    usage: Option<Usage>,
    tool_calls: BTreeMap<u32, ToolCallState>,
    text_buffer: String,
    buffer_mode: TextBufferMode,
    synthesized_tool_use: bool,
}

impl StreamState {
    fn new(model: String) -> Self {
        Self {
            model,
            message_started: false,
            text_started: false,
            text_finished: false,
            finished: false,
            stop_reason: None,
            usage: None,
            tool_calls: BTreeMap::new(),
            text_buffer: String::new(),
            buffer_mode: TextBufferMode::Pending,
            synthesized_tool_use: false,
        }
    }

    fn ingest_chunk(&mut self, chunk: ChatCompletionChunk) -> Result<Vec<StreamEvent>, ApiError> {
        let mut events = Vec::new();
        if !self.message_started {
            self.message_started = true;
            events.push(StreamEvent::MessageStart(MessageStartEvent {
                message: MessageResponse {
                    id: chunk.id.clone(),
                    kind: "message".to_string(),
                    role: "assistant".to_string(),
                    content: Vec::new(),
                    model: chunk.model.clone().unwrap_or_else(|| self.model.clone()),
                    stop_reason: None,
                    stop_sequence: None,
                    usage: Usage {
                        input_tokens: 0,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        output_tokens: 0,
                    },
                    request_id: None,
                },
            }));
        }

        if let Some(usage) = chunk.usage {
            self.usage = Some(Usage {
                input_tokens: usage.prompt_tokens,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                output_tokens: usage.completion_tokens,
            });
        }

        for choice in chunk.choices {
            if let Some(content) = choice.delta.content.filter(|value| !value.is_empty()) {
                self.handle_text_delta(content, &mut events);
            }

            for tool_call in choice.delta.tool_calls {
                let state = self.tool_calls.entry(tool_call.index).or_default();
                state.apply(tool_call);
                let block_index = state.block_index();
                if !state.started {
                    if let Some(start_event) = state.start_event()? {
                        state.started = true;
                        events.push(StreamEvent::ContentBlockStart(start_event));
                    } else {
                        continue;
                    }
                }
                if let Some(delta_event) = state.delta_event() {
                    events.push(StreamEvent::ContentBlockDelta(delta_event));
                }
                if choice.finish_reason.as_deref() == Some("tool_calls") && !state.stopped {
                    state.stopped = true;
                    events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                        index: block_index,
                    }));
                }
            }

            if let Some(finish_reason) = choice.finish_reason {
                self.stop_reason = Some(normalize_finish_reason(&finish_reason));
                if finish_reason == "tool_calls" {
                    for state in self.tool_calls.values_mut() {
                        if state.started && !state.stopped {
                            state.stopped = true;
                            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                                index: state.block_index(),
                            }));
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    /// Route text deltas through buffer-mode logic so we can detect & synthesize
    /// tool calls that the upstream server emitted as text JSON.
    ///
    /// Three behaviors based on `buffer_mode`:
    /// * `Pending`: first non-whitespace chunk decides — `{` or ` ``` ` opens
    ///   `Buffering`; anything else opens `PassThrough`.
    /// * `Buffering`: accumulate everything silently; parse on `finish`.
    /// * `PassThrough`: emit as text deltas live, BUT also watch for
    ///   `{` / ` ``` ` appearing mid-stream — at that point split the chunk:
    ///   emit the prefix as text, switch to `Buffering` for the suffix.
    ///   This lets preamble like "Sure, here is the call:" stream normally
    ///   while still suppressing the JSON itself.
    fn handle_text_delta(&mut self, content: String, events: &mut Vec<StreamEvent>) {
        if self.buffer_mode == TextBufferMode::PassThrough {
            // Look for tool-call onset mid-stream.
            if let Some(idx) = find_tool_call_start(&content) {
                let (pre, post) = content.split_at(idx);
                if !pre.is_empty() {
                    self.emit_text_start_if_needed(events);
                    events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                        index: 0,
                        delta: ContentBlockDelta::TextDelta {
                            text: pre.to_string(),
                        },
                    }));
                }
                self.buffer_mode = TextBufferMode::Buffering;
                self.text_buffer.push_str(post);
            } else {
                self.emit_text_start_if_needed(events);
                events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    index: 0,
                    delta: ContentBlockDelta::TextDelta { text: content },
                }));
            }
            return;
        }
        // In Buffering or Pending mode — accumulate first, then decide.
        self.text_buffer.push_str(&content);
        if self.buffer_mode == TextBufferMode::Pending {
            let trimmed_start = self.text_buffer.trim_start();
            if trimmed_start.is_empty() {
                return; // still all whitespace; wait for more
            }
            self.buffer_mode = if trimmed_start.starts_with('{') || trimmed_start.starts_with("```")
            {
                TextBufferMode::Buffering
            } else {
                TextBufferMode::PassThrough
            };
            if self.buffer_mode == TextBufferMode::PassThrough {
                // Drain whatever we buffered as a normal text delta —
                // and re-check for mid-stream tool-call onset since the
                // preamble might already contain the `{`.
                let drained = std::mem::take(&mut self.text_buffer);
                if let Some(idx) = find_tool_call_start(&drained) {
                    let (pre, post) = drained.split_at(idx);
                    if !pre.is_empty() {
                        self.emit_text_start_if_needed(events);
                        events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                            index: 0,
                            delta: ContentBlockDelta::TextDelta {
                                text: pre.to_string(),
                            },
                        }));
                    }
                    self.buffer_mode = TextBufferMode::Buffering;
                    self.text_buffer.push_str(post);
                } else {
                    self.emit_text_start_if_needed(events);
                    events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                        index: 0,
                        delta: ContentBlockDelta::TextDelta { text: drained },
                    }));
                }
            }
        }
    }

    fn emit_text_start_if_needed(&mut self, events: &mut Vec<StreamEvent>) {
        if !self.text_started {
            self.text_started = true;
            events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: 0,
                content_block: OutputContentBlock::Text {
                    text: String::new(),
                },
            }));
        }
    }

    /// If we buffered text that started with `{` or a code fence, try to
    /// extract one or more tool calls. Returns true if at least one tool_use
    /// was synthesized (caller should skip text emission).
    ///
    /// Handles the case where a model emits multiple concatenated tool calls
    /// in a single response, e.g. `{...}{...}` for "write a file then run it".
    /// Each balanced top-level `{}` block is parsed independently.
    fn try_synthesize_tool_use(&mut self, events: &mut Vec<StreamEvent>) -> bool {
        if self.buffer_mode != TextBufferMode::Buffering || self.text_buffer.is_empty() {
            return false;
        }
        let cleaned = strip_code_fences(self.text_buffer.trim());
        let normalized = normalize_smart_quotes(cleaned);
        let blocks = extract_balanced_json_blocks(&normalized);
        if blocks.is_empty() {
            return false;
        }
        // Each successful parse becomes a content block at a fresh index.
        // We allocate sequentially starting at 0; text was suppressed in this
        // mode so there's no other block 0 to collide with.
        let mut next_index: u32 = 0;
        let mut any_emitted = false;
        for block in blocks {
            let Ok(value) = serde_json::from_str::<Value>(block) else {
                continue;
            };
            let name = value
                .get("name")
                .or_else(|| value.get("function"))
                .and_then(Value::as_str);
            let args = value
                .get("arguments")
                .or_else(|| value.get("parameters"))
                .or_else(|| value.get("args"));
            let (Some(name), Some(args)) = (name, args) else {
                continue;
            };
            let id = format!(
                "call_synth_{}_{next_index}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            );
            let idx = next_index;
            events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index: idx,
                content_block: OutputContentBlock::ToolUse {
                    id,
                    name: name.to_string(),
                    input: Value::Object(serde_json::Map::new()),
                },
            }));
            events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index: idx,
                delta: ContentBlockDelta::InputJsonDelta {
                    partial_json: args.to_string(),
                },
            }));
            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                index: idx,
            }));
            next_index += 1;
            any_emitted = true;
        }
        if !any_emitted {
            return false;
        }
        self.synthesized_tool_use = true;
        self.stop_reason = Some("tool_use".to_string());
        self.text_buffer.clear();
        true
    }

    fn finish(&mut self) -> Result<Vec<StreamEvent>, ApiError> {
        if self.finished {
            return Ok(Vec::new());
        }
        self.finished = true;

        let mut events = Vec::new();

        // First, attempt tool-call synthesis from buffered text.
        let synthesized = self.try_synthesize_tool_use(&mut events);
        // If buffering didn't yield a tool call, flush whatever we buffered as text.
        if !synthesized && !self.text_buffer.is_empty() {
            let drained = std::mem::take(&mut self.text_buffer);
            self.emit_text_start_if_needed(&mut events);
            events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                index: 0,
                delta: ContentBlockDelta::TextDelta { text: drained },
            }));
        }

        if self.text_started && !self.text_finished {
            self.text_finished = true;
            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                index: 0,
            }));
        }

        for state in self.tool_calls.values_mut() {
            if !state.started {
                if let Some(start_event) = state.start_event()? {
                    state.started = true;
                    events.push(StreamEvent::ContentBlockStart(start_event));
                    if let Some(delta_event) = state.delta_event() {
                        events.push(StreamEvent::ContentBlockDelta(delta_event));
                    }
                }
            }
            if state.started && !state.stopped {
                state.stopped = true;
                events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                    index: state.block_index(),
                }));
            }
        }

        if self.message_started {
            events.push(StreamEvent::MessageDelta(MessageDeltaEvent {
                delta: MessageDelta {
                    stop_reason: Some(
                        self.stop_reason
                            .clone()
                            .unwrap_or_else(|| "end_turn".to_string()),
                    ),
                    stop_sequence: None,
                },
                usage: self.usage.clone().unwrap_or(Usage {
                    input_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    output_tokens: 0,
                }),
            }));
            events.push(StreamEvent::MessageStop(MessageStopEvent {}));
        }
        Ok(events)
    }
}

#[derive(Debug, Default)]
struct ToolCallState {
    openai_index: u32,
    id: Option<String>,
    name: Option<String>,
    arguments: String,
    emitted_len: usize,
    started: bool,
    stopped: bool,
}

impl ToolCallState {
    fn apply(&mut self, tool_call: DeltaToolCall) {
        self.openai_index = tool_call.index;
        if let Some(id) = tool_call.id {
            self.id = Some(id);
        }
        if let Some(name) = tool_call.function.name {
            self.name = Some(name);
        }
        if let Some(arguments) = tool_call.function.arguments {
            self.arguments.push_str(&arguments);
        }
    }

    const fn block_index(&self) -> u32 {
        self.openai_index + 1
    }

    fn start_event(&self) -> Result<Option<ContentBlockStartEvent>, ApiError> {
        let Some(name) = self.name.clone() else {
            return Ok(None);
        };
        let id = self
            .id
            .clone()
            .unwrap_or_else(|| format!("tool_call_{}", self.openai_index));
        Ok(Some(ContentBlockStartEvent {
            index: self.block_index(),
            content_block: OutputContentBlock::ToolUse {
                id,
                name,
                input: json!({}),
            },
        }))
    }

    fn delta_event(&mut self) -> Option<ContentBlockDeltaEvent> {
        if self.emitted_len >= self.arguments.len() {
            return None;
        }
        let delta = self.arguments[self.emitted_len..].to_string();
        self.emitted_len = self.arguments.len();
        Some(ContentBlockDeltaEvent {
            index: self.block_index(),
            delta: ContentBlockDelta::InputJsonDelta {
                partial_json: delta,
            },
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    id: String,
    model: String,
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ResponseToolCall>,
}

#[derive(Debug, Deserialize)]
struct ResponseToolCall {
    id: String,
    function: ResponseToolFunction,
}

#[derive(Debug, Deserialize)]
struct ResponseToolFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    id: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<DeltaToolCall>,
}

#[derive(Debug, Deserialize)]
struct DeltaToolCall {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: DeltaFunction,
}

#[derive(Debug, Default, Deserialize)]
struct DeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(rename = "type")]
    error_type: Option<String>,
    message: Option<String>,
}

enum CodexResponseEvent {
    Created {
        id: String,
        model: String,
    },
    TextDelta {
        delta: String,
    },
    ToolCallStarted {
        item_id: String,
        index: u32,
        call_id: String,
        name: String,
    },
    ToolArgumentsDelta {
        item_id: String,
        delta: String,
    },
    ToolCallDone {
        item_id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    Completed {
        usage: Option<OpenAiUsage>,
    },
}

fn parse_codex_response_sse_frame(frame: &str) -> Result<Option<CodexResponseEvent>, ApiError> {
    let Some(value) = parse_sse_json_value(frame)? else {
        return Ok(None);
    };
    match value.get("type").and_then(Value::as_str) {
        Some("response.created") => {
            let Some(response) = value.get("response") else {
                return Ok(None);
            };
            let id = response
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("codex-response")
                .to_string();
            let model = response
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("gpt-5.5")
                .to_string();
            Ok(Some(CodexResponseEvent::Created { id, model }))
        }
        Some("response.output_text.delta") => Ok(value
            .get("delta")
            .and_then(Value::as_str)
            .filter(|delta| !delta.is_empty())
            .map(|delta| CodexResponseEvent::TextDelta {
                delta: delta.to_string(),
            })),
        Some("response.output_item.added") => {
            let Some(item) = value.get("item") else {
                return Ok(None);
            };
            if item.get("type").and_then(Value::as_str) != Some("function_call") {
                return Ok(None);
            }
            let item_id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("codex-tool-call")
                .to_string();
            let index = value
                .get("output_index")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or(&item_id)
                .to_string();
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("function")
                .to_string();
            Ok(Some(CodexResponseEvent::ToolCallStarted {
                item_id,
                index,
                call_id,
                name,
            }))
        }
        Some("response.function_call_arguments.delta") => {
            let item_id = value
                .get("item_id")
                .and_then(Value::as_str)
                .unwrap_or("codex-tool-call")
                .to_string();
            let delta = value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Ok(Some(CodexResponseEvent::ToolArgumentsDelta {
                item_id,
                delta,
            }))
        }
        Some("response.output_item.done") => {
            let Some(item) = value.get("item") else {
                return Ok(None);
            };
            if item.get("type").and_then(Value::as_str) != Some("function_call") {
                return Ok(None);
            }
            let item_id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("codex-tool-call")
                .to_string();
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or(&item_id)
                .to_string();
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("function")
                .to_string();
            let arguments = item
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Ok(Some(CodexResponseEvent::ToolCallDone {
                item_id,
                call_id,
                name,
                arguments,
            }))
        }
        Some("response.completed") => {
            let usage = value
                .get("response")
                .and_then(|response| response.get("usage"))
                .map(codex_usage_from_value);
            Ok(Some(CodexResponseEvent::Completed { usage }))
        }
        _ => Ok(None),
    }
}

fn codex_usage_from_value(value: &Value) -> OpenAiUsage {
    OpenAiUsage {
        prompt_tokens: value
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        completion_tokens: value
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    }
}

fn parse_sse_json_value(frame: &str) -> Result<Option<Value>, ApiError> {
    let trimmed = frame.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut data_lines = Vec::new();
    for line in trimmed.lines() {
        if line.starts_with(':') {
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start());
        }
    }
    if data_lines.is_empty() {
        return Ok(None);
    }
    let payload = data_lines.join("\n");
    if payload == "[DONE]" {
        return Ok(None);
    }
    serde_json::from_str(&payload)
        .map(Some)
        .map_err(ApiError::from)
}

fn build_chat_completion_request(request: &MessageRequest) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = request.system.as_ref().filter(|value| !value.is_empty()) {
        messages.push(json!({
            "role": "system",
            "content": system,
        }));
    }
    for message in &request.messages {
        messages.extend(translate_message(message));
    }

    let mut payload = json!({
        "model": super::resolve_model_alias(&request.model),
        "max_tokens": request.max_tokens,
        "messages": messages,
        "stream": request.stream,
    });

    if let Some(tools) = &request.tools {
        payload["tools"] =
            Value::Array(tools.iter().map(openai_tool_definition).collect::<Vec<_>>());
    }
    if let Some(tool_choice) = &request.tool_choice {
        payload["tool_choice"] = openai_tool_choice(tool_choice);
    }

    payload
}

fn build_codex_responses_request(request: &MessageRequest) -> Value {
    let mut input = Vec::new();
    for message in &request.messages {
        input.push(codex_response_input_message(message));
    }

    let mut payload = json!({
        "model": super::resolve_model_alias(&request.model),
        "instructions": request.system.clone().unwrap_or_default(),
        "input": input,
        "stream": true,
        "store": false,
    });

    if let Some(tools) = &request.tools {
        payload["tools"] = Value::Array(
            tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.input_schema,
                    })
                })
                .collect::<Vec<_>>(),
        );
    }

    payload
}

fn codex_response_input_message(message: &InputMessage) -> Value {
    if message.content.len() == 1 {
        if let InputContentBlock::ToolUse { id, name, input } = &message.content[0] {
            return json!({
                "type": "function_call",
                "call_id": id,
                "name": name,
                "arguments": input.to_string(),
            });
        }
        if let InputContentBlock::ToolResult {
            tool_use_id,
            content,
            ..
        } = &message.content[0]
        {
            let mut output = String::new();
            for result in content {
                match result {
                    ToolResultContentBlock::Text { text } => output.push_str(text),
                    ToolResultContentBlock::Json { value } => output.push_str(&value.to_string()),
                }
            }
            return json!({
                "type": "function_call_output",
                "call_id": tool_use_id,
                "output": output,
            });
        }
    }

    let mut text = String::new();
    for block in &message.content {
        match block {
            InputContentBlock::Text { text: value } => text.push_str(value),
            InputContentBlock::ToolResult { content, .. } => {
                for result in content {
                    match result {
                        ToolResultContentBlock::Text { text: value } => text.push_str(value),
                        ToolResultContentBlock::Json { value } => text.push_str(&value.to_string()),
                    }
                }
            }
            InputContentBlock::ToolUse { .. } => {}
        }
    }
    let content_type = if message.role == "assistant" {
        "output_text"
    } else {
        "input_text"
    };
    json!({
        "role": message.role,
        "content": [{
            "type": content_type,
            "text": text,
        }],
    })
}

fn translate_message(message: &InputMessage) -> Vec<Value> {
    match message.role.as_str() {
        "assistant" => {
            let mut text = String::new();
            let mut tool_calls = Vec::new();
            for block in &message.content {
                match block {
                    InputContentBlock::Text { text: value } => text.push_str(value),
                    InputContentBlock::ToolUse { id, name, input } => tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": input.to_string(),
                        }
                    })),
                    InputContentBlock::ToolResult { .. } => {}
                }
            }
            if text.is_empty() && tool_calls.is_empty() {
                Vec::new()
            } else {
                vec![json!({
                    "role": "assistant",
                    "content": (!text.is_empty()).then_some(text),
                    "tool_calls": tool_calls,
                })]
            }
        }
        _ => message
            .content
            .iter()
            .filter_map(|block| match block {
                InputContentBlock::Text { text } => Some(json!({
                    "role": "user",
                    "content": text,
                })),
                InputContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => Some(json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": flatten_tool_result_content(content),
                    "is_error": is_error,
                })),
                InputContentBlock::ToolUse { .. } => None,
            })
            .collect(),
    }
}

fn flatten_tool_result_content(content: &[ToolResultContentBlock]) -> String {
    content
        .iter()
        .map(|block| match block {
            ToolResultContentBlock::Text { text } => text.clone(),
            ToolResultContentBlock::Json { value } => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn openai_tool_definition(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

fn openai_tool_choice(tool_choice: &ToolChoice) -> Value {
    match tool_choice {
        ToolChoice::Auto => Value::String("auto".to_string()),
        ToolChoice::Any => Value::String("required".to_string()),
        ToolChoice::Tool { name } => json!({
            "type": "function",
            "function": { "name": name },
        }),
    }
}

fn normalize_response(
    model: &str,
    response: ChatCompletionResponse,
) -> Result<MessageResponse, ApiError> {
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or(ApiError::InvalidSseFrame(
            "chat completion response missing choices",
        ))?;
    let mut content = Vec::new();
    if let Some(text) = choice.message.content.filter(|value| !value.is_empty()) {
        content.push(OutputContentBlock::Text { text });
    }
    for tool_call in choice.message.tool_calls {
        content.push(OutputContentBlock::ToolUse {
            id: tool_call.id,
            name: tool_call.function.name,
            input: parse_tool_arguments(&tool_call.function.arguments),
        });
    }

    Ok(MessageResponse {
        id: response.id,
        kind: "message".to_string(),
        role: choice.message.role,
        content,
        model: response.model.if_empty_then(model.to_string()),
        stop_reason: choice
            .finish_reason
            .map(|value| normalize_finish_reason(&value)),
        stop_sequence: None,
        usage: Usage {
            input_tokens: response
                .usage
                .as_ref()
                .map_or(0, |usage| usage.prompt_tokens),
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            output_tokens: response
                .usage
                .as_ref()
                .map_or(0, |usage| usage.completion_tokens),
        },
        request_id: None,
    })
}

fn parse_tool_arguments(arguments: &str) -> Value {
    serde_json::from_str(arguments).unwrap_or_else(|_| json!({ "raw": arguments }))
}

fn next_sse_frame(buffer: &mut Vec<u8>) -> Option<String> {
    let separator = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2))
        .or_else(|| {
            buffer
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| (position, 4))
        })?;

    let (position, separator_len) = separator;
    let frame = buffer.drain(..position + separator_len).collect::<Vec<_>>();
    let frame_len = frame.len().saturating_sub(separator_len);
    Some(String::from_utf8_lossy(&frame[..frame_len]).into_owned())
}

fn parse_sse_frame(frame: &str) -> Result<Option<ChatCompletionChunk>, ApiError> {
    parse_sse_json_value(frame)?
        .map(serde_json::from_value)
        .transpose()
        .map_err(ApiError::from)
}

fn read_env_non_empty(key: &str) -> Result<Option<String>, ApiError> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(ApiError::from(error)),
    }
}

fn read_first_env_non_empty(keys: &[&str]) -> Result<Option<String>, ApiError> {
    for key in keys {
        if let Some(value) = read_env_non_empty(key)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

#[must_use]
pub fn has_api_key(key: &str) -> bool {
    if read_env_non_empty(key)
        .ok()
        .and_then(std::convert::identity)
        .is_some()
    {
        return true;
    }
    key == "OPENAI_API_KEY" && has_codex_openai_auth()
}

#[must_use]
pub fn read_base_url(config: OpenAiCompatConfig) -> String {
    std::env::var(config.base_url_env).unwrap_or_else(|_| config.default_base_url.to_string())
}

fn read_codex_responses_base_url() -> String {
    std::env::var("CODEX_RESPONSES_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_CODEX_RESPONSES_BASE_URL.to_string())
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn codex_responses_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/responses") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/responses")
    }
}

#[derive(Debug, Clone)]
struct ResolvedAuth {
    access_token: String,
    account_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CodexAuthFile {
    #[serde(rename = "OPENAI_API_KEY")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    openai_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tokens: Option<CodexAuthTokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_refresh: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CodexAuthTokens {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_id: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct CodexOAuthRefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

fn has_codex_openai_auth() -> bool {
    load_codex_auth_file().ok().flatten().is_some_and(|auth| {
        auth.tokens.as_ref().is_some_and(|tokens| {
            non_empty_ref(tokens.access_token.as_deref()).is_some()
                || non_empty_ref(tokens.refresh_token.as_deref()).is_some()
        })
    })
}

async fn resolve_codex_openai_auth(
    http: &reqwest::Client,
    force_refresh: bool,
) -> Result<ResolvedAuth, ApiError> {
    let path = codex_auth_path().ok_or_else(|| {
        ApiError::Auth("CODEX_HOME or HOME is required to locate Codex auth".to_string())
    })?;
    let mut auth = load_codex_auth_file_from(&path)?.ok_or_else(|| {
        ApiError::Auth("Codex login not found; run `codex login` first".to_string())
    })?;
    let Some(tokens) = auth.tokens.as_ref() else {
        return Err(ApiError::Auth(
            "Codex login does not contain OAuth tokens; run `codex login` again".to_string(),
        ));
    };
    let account_id = non_empty_ref(tokens.account_id.as_deref()).map(ToOwned::to_owned);
    let access_token = non_empty_ref(tokens.access_token.as_deref()).map(ToOwned::to_owned);
    let refresh_token = non_empty_ref(tokens.refresh_token.as_deref()).map(ToOwned::to_owned);

    if !force_refresh {
        if let Some(access_token) = access_token.as_ref() {
            if !oauth_token_expires_soon(access_token) {
                return Ok(ResolvedAuth {
                    access_token: access_token.clone(),
                    account_id,
                });
            }
        }
    }

    let Some(refresh_token) = refresh_token else {
        return access_token.map_or_else(
            || {
                Err(ApiError::Auth(
                    "Codex login does not contain a usable access or refresh token; run `codex login` again"
                        .to_string(),
                ))
            },
            |access_token| Ok(ResolvedAuth { access_token, account_id }),
        );
    };

    let refreshed = refresh_codex_oauth_token(http, &refresh_token).await?;
    let tokens = auth.tokens.get_or_insert_with(|| CodexAuthTokens {
        id_token: None,
        access_token: None,
        refresh_token: None,
        account_id: None,
        extra: Map::new(),
    });
    tokens.access_token = Some(refreshed.access_token.clone());
    if let Some(refresh_token) = refreshed.refresh_token {
        tokens.refresh_token = Some(refresh_token);
    }
    if let Some(id_token) = refreshed.id_token {
        tokens.id_token = Some(id_token);
    }
    if tokens.account_id.is_none() {
        tokens.account_id = account_id.clone();
    }
    let _ = refreshed.expires_in;
    auth.last_refresh = Some(unix_timestamp_string()?);
    save_codex_auth_file(&path, &auth)?;

    Ok(ResolvedAuth {
        access_token: refreshed.access_token,
        account_id,
    })
}

async fn refresh_codex_oauth_token(
    http: &reqwest::Client,
    refresh_token: &str,
) -> Result<CodexOAuthRefreshResponse, ApiError> {
    let token_url = std::env::var("CODEX_OAUTH_TOKEN_URL")
        .unwrap_or_else(|_| DEFAULT_CODEX_OAUTH_TOKEN_URL.to_string());
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CODEX_OAUTH_CLIENT_ID),
    ];
    let response = http
        .post(token_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .form(&form)
        .send()
        .await?;
    let response = expect_success(response).await?;
    response
        .json::<CodexOAuthRefreshResponse>()
        .await
        .map_err(ApiError::from)
}

fn load_codex_auth_file() -> Result<Option<CodexAuthFile>, ApiError> {
    let Some(path) = codex_auth_path() else {
        return Ok(None);
    };
    load_codex_auth_file_from(&path)
}

fn load_codex_auth_file_from(path: &PathBuf) -> Result<Option<CodexAuthFile>, ApiError> {
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str::<CodexAuthFile>(&contents)
            .map(Some)
            .map_err(ApiError::from),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ApiError::from(error)),
    }
}

fn save_codex_auth_file(path: &PathBuf, auth: &CodexAuthFile) -> Result<(), ApiError> {
    let contents = serde_json::to_string_pretty(auth)?;
    fs::write(path, format!("{contents}\n")).map_err(ApiError::from)
}

fn codex_auth_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_AUTH_FILE") {
        return Some(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("CODEX_HOME") {
        return Some(PathBuf::from(home).join("auth.json"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex").join("auth.json"))
}

fn non_empty_ref(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn oauth_token_expires_soon(access_token: &str) -> bool {
    let Some(exp) = jwt_exp(access_token) else {
        return false;
    };
    let Ok(now) = unix_timestamp() else {
        return false;
    };
    exp <= now.saturating_add(CODEX_AUTH_EXPIRY_SKEW_SECS)
}

fn jwt_exp(access_token: &str) -> Option<u64> {
    let payload = access_token.split('.').nth(1)?;
    let decoded = base64url_decode(payload)?;
    let value = serde_json::from_slice::<Value>(&decoded).ok()?;
    value.get("exp")?.as_u64()
}

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut normalized = input.replace('-', "+").replace('_', "/");
    while normalized.len() % 4 != 0 {
        normalized.push('=');
    }
    let bytes = normalized.as_bytes();
    let mut output = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut values = [0_u8; 4];
        let mut padding = 0;
        for (index, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                values[index] = 0;
                padding += 1;
            } else {
                values[index] = TABLE.iter().position(|value| value == byte)? as u8;
            }
        }
        output.push((values[0] << 2) | (values[1] >> 4));
        if padding < 2 {
            output.push((values[1] << 4) | (values[2] >> 2));
        }
        if padding < 1 {
            output.push((values[2] << 6) | values[3]);
        }
    }
    Some(output)
}

fn unix_timestamp() -> Result<u64, ApiError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| ApiError::Auth(format!("system clock is before UNIX epoch: {error}")))
}

fn unix_timestamp_string() -> Result<String, ApiError> {
    unix_timestamp().map(|timestamp| timestamp.to_string())
}

fn request_id_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(REQUEST_ID_HEADER)
        .or_else(|| headers.get(ALT_REQUEST_ID_HEADER))
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

async fn expect_success(response: reqwest::Response) -> Result<reqwest::Response, ApiError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response.text().await.unwrap_or_default();
    let parsed_error = serde_json::from_str::<ErrorEnvelope>(&body).ok();
    let retryable = is_retryable_status(status);

    Err(ApiError::Api {
        status,
        error_type: parsed_error
            .as_ref()
            .and_then(|error| error.error.error_type.clone()),
        message: parsed_error
            .as_ref()
            .and_then(|error| error.error.message.clone()),
        body,
        retryable,
    })
}

const fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 409 | 429 | 500 | 502 | 503 | 504)
}

fn normalize_finish_reason(value: &str) -> String {
    match value {
        "stop" => "end_turn",
        "tool_calls" => "tool_use",
        other => other,
    }
    .to_string()
}

/// Walk `s` and return each balanced top-level `{...}` block as a substring.
/// Skips braces inside JSON string literals (handles `\"` escapes correctly).
/// Anything outside of a balanced block (preamble whitespace, commentary
/// between blocks, etc.) is ignored.
fn extract_balanced_json_blocks(s: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut depth: u32 = 0;
    let mut start: Option<usize> = None;
    let mut in_string = false;
    let mut escape_next = false;
    for (i, c) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if in_string {
            match c {
                '\\' => escape_next = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth = depth.saturating_add(1);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(s_idx) = start.take() {
                        let end = i + c.len_utf8();
                        blocks.push(&s[s_idx..end]);
                    }
                }
            }
            _ => {}
        }
    }
    blocks
}

/// Earliest index in `s` of a tool-call onset marker. We look for either
/// `{` (a bare JSON object) or ` ``` ` (a markdown code fence). Returns the
/// byte offset of whichever appears first, or `None` if neither appears.
fn find_tool_call_start(s: &str) -> Option<usize> {
    let brace = s.find('{');
    let fence = s.find("```");
    match (brace, fence) {
        (Some(b), Some(f)) => Some(b.min(f)),
        (Some(b), None) => Some(b),
        (None, Some(f)) => Some(f),
        (None, None) => None,
    }
}

/// Replace Unicode curly quotes with ASCII so serde_json can parse the
/// model's output. Common Qwen-via-Ollama quirk.
fn normalize_smart_quotes(s: &str) -> String {
    s.replace('\u{201C}', "\"") // LEFT DOUBLE QUOTATION MARK
        .replace('\u{201D}', "\"") // RIGHT DOUBLE QUOTATION MARK
        .replace('\u{2018}', "'") // LEFT SINGLE QUOTATION MARK
        .replace('\u{2019}', "'") // RIGHT SINGLE QUOTATION MARK
}

/// Strip surrounding ```json ... ``` (or plain ``` ... ```) fences so we can
/// parse the inner JSON. Models often wrap tool-call JSON in markdown.
fn strip_code_fences(input: &str) -> &str {
    let trimmed = input.trim();
    let Some(stripped) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    // Drop optional language tag on the first line (```json, ```yaml, etc.)
    let after_lang = match stripped.find('\n') {
        Some(idx) => &stripped[idx + 1..],
        None => stripped,
    };
    after_lang.strip_suffix("```").unwrap_or(after_lang).trim()
}

trait StringExt {
    fn if_empty_then(self, fallback: String) -> String;
}

impl StringExt for String {
    fn if_empty_then(self, fallback: String) -> String {
        if self.is_empty() {
            fallback
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_chat_completion_request, chat_completions_endpoint, has_codex_openai_auth,
        normalize_finish_reason, openai_tool_choice, parse_tool_arguments, OpenAiCompatClient,
        OpenAiCompatConfig,
    };
    use crate::error::ApiError;
    use crate::types::{
        InputContentBlock, InputMessage, MessageRequest, ToolChoice, ToolDefinition,
        ToolResultContentBlock,
    };
    use serde_json::json;
    use std::ffi::OsString;
    use std::fs;
    use std::sync::{Mutex, OnceLock};

    #[test]
    fn request_translation_uses_openai_compatible_shape() {
        let payload = build_chat_completion_request(&MessageRequest {
            model: "grok-3".to_string(),
            max_tokens: 64,
            messages: vec![InputMessage {
                role: "user".to_string(),
                content: vec![
                    InputContentBlock::Text {
                        text: "hello".to_string(),
                    },
                    InputContentBlock::ToolResult {
                        tool_use_id: "tool_1".to_string(),
                        content: vec![ToolResultContentBlock::Json {
                            value: json!({"ok": true}),
                        }],
                        is_error: false,
                    },
                ],
            }],
            system: Some("be helpful".to_string()),
            tools: Some(vec![ToolDefinition {
                name: "weather".to_string(),
                description: Some("Get weather".to_string()),
                input_schema: json!({"type": "object"}),
            }]),
            tool_choice: Some(ToolChoice::Auto),
            stream: false,
        });

        assert_eq!(payload["messages"][0]["role"], json!("system"));
        assert_eq!(payload["messages"][1]["role"], json!("user"));
        assert_eq!(payload["messages"][2]["role"], json!("tool"));
        assert_eq!(payload["tools"][0]["type"], json!("function"));
        assert_eq!(payload["tool_choice"], json!("auto"));
    }

    #[test]
    fn tool_choice_translation_supports_required_function() {
        assert_eq!(openai_tool_choice(&ToolChoice::Any), json!("required"));
        assert_eq!(
            openai_tool_choice(&ToolChoice::Tool {
                name: "weather".to_string(),
            }),
            json!({"type": "function", "function": {"name": "weather"}})
        );
    }

    #[test]
    fn parses_tool_arguments_fallback() {
        assert_eq!(
            parse_tool_arguments("{\"city\":\"Paris\"}"),
            json!({"city": "Paris"})
        );
        assert_eq!(parse_tool_arguments("not-json"), json!({"raw": "not-json"}));
    }

    #[test]
    fn missing_xai_api_key_is_provider_specific() {
        let _lock = env_lock();
        std::env::remove_var("XAI_API_KEY");
        let error = OpenAiCompatClient::from_env(OpenAiCompatConfig::xai())
            .expect_err("missing key should error");
        assert!(matches!(
            error,
            ApiError::MissingCredentials {
                provider: "xAI",
                ..
            }
        ));
    }

    #[test]
    fn detects_codex_oauth_access_token() {
        let _lock = env_lock();
        let auth_path =
            std::env::temp_dir().join(format!("notclaude-codex-auth-{}.json", std::process::id()));
        fs::write(
            &auth_path,
            r#"{"OPENAI_API_KEY":null,"tokens":{"access_token":"codex-access-token"}}"#,
        )
        .expect("write auth fixture");
        let _auth_file = EnvVarGuard::set("CODEX_AUTH_FILE", Some(auth_path.to_string_lossy()));

        assert!(has_codex_openai_auth());

        let _ = fs::remove_file(auth_path);
    }

    #[test]
    fn endpoint_builder_accepts_base_urls_and_full_endpoints() {
        assert_eq!(
            chat_completions_endpoint("https://api.x.ai/v1"),
            "https://api.x.ai/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_endpoint("https://api.x.ai/v1/"),
            "https://api.x.ai/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_endpoint("https://api.x.ai/v1/chat/completions"),
            "https://api.x.ai/v1/chat/completions"
        );
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }

    struct EnvVarGuard {
        key: &'static str,
        original: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: Option<impl AsRef<str>>) -> Self {
            let original = std::env::var_os(key);
            match value {
                Some(value) => std::env::set_var(key, value.as_ref()),
                None => std::env::remove_var(key),
            }
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn normalizes_stop_reasons() {
        assert_eq!(normalize_finish_reason("stop"), "end_turn");
        assert_eq!(normalize_finish_reason("tool_calls"), "tool_use");
    }
}
