use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};

use coddy_core::{ApprovalPolicy, ModelCredential, ModelRef, ToolDefinition, ToolRiskLevel};
use serde_json::Value;
use thiserror::Error;

const DEFAULT_OPENAI_COMPATIBLE_TIMEOUT: Duration = Duration::from_secs(120);
const OPENROUTER_OPENAI_COMPATIBLE_TIMEOUT: Duration = Duration::from_secs(300);
const NVIDIA_OPENAI_COMPATIBLE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatMessageRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub risk_level: ToolRiskLevel,
    pub approval_policy: ApprovalPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: ModelRef,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ChatToolSpec>,
    pub model_credential: Option<ModelCredential>,
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatToolCall {
    pub id: Option<String>,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatFinishReason {
    Stop,
    Length,
    ToolCalls,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatResponse {
    pub text: String,
    pub deltas: Vec<String>,
    pub finish_reason: ChatFinishReason,
    pub tool_calls: Vec<ChatToolCall>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ChatModelError {
    #[error("chat model is not selected")]
    UnselectedModel,

    #[error("chat provider is not available yet: {provider}")]
    ProviderUnavailable { provider: String },

    #[error("chat model is not supported yet: {provider}/{model}")]
    UnsupportedModel { provider: String, model: String },

    #[error("invalid chat request: {0}")]
    InvalidRequest(String),

    #[error("chat provider returned an error from {provider}: {message}")]
    ProviderError {
        provider: String,
        message: String,
        retryable: bool,
    },

    #[error("chat provider transport failed for {provider}: {message}")]
    Transport {
        provider: String,
        message: String,
        retryable: bool,
    },

    #[error("invalid chat provider response from {provider}: {message}")]
    InvalidProviderResponse { provider: String, message: String },
}

pub type ChatModelResult = Result<ChatResponse, ChatModelError>;

pub trait ChatModelClient: std::fmt::Debug + Send + Sync {
    fn complete(&self, request: ChatRequest) -> ChatModelResult;
}

#[derive(Debug, Default)]
pub struct UnavailableChatModelClient;

#[derive(Debug)]
pub struct DefaultChatModelClient {
    ollama: OllamaChatModelClient,
    openai: OpenAiCompatibleChatModelClient,
    openrouter: OpenAiCompatibleChatModelClient,
    nvidia: OpenAiCompatibleChatModelClient,
    gemini_api: GeminiApiChatModelClient,
    vertex_gemini: VertexGeminiChatModelClient,
    vertex_anthropic: VertexAnthropicChatModelClient,
    azure_openai: AzureOpenAiChatModelClient,
    unavailable: UnavailableChatModelClient,
}

#[derive(Debug, Clone)]
pub struct OllamaChatModelClient {
    transport: Arc<dyn OllamaTransport>,
}

#[derive(Debug, Clone)]
struct HttpOllamaTransport {
    base_url: String,
    timeout: Duration,
}

trait OllamaTransport: std::fmt::Debug + Send + Sync {
    fn chat(&self, body: &Value) -> ChatModelResult;
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleChatModelClient {
    provider: String,
    default_base_url: String,
    transport: Arc<dyn OpenAiCompatibleTransport>,
}

#[derive(Debug, Clone)]
struct HttpOpenAiCompatibleTransport {
    timeout: Duration,
}

trait OpenAiCompatibleTransport: std::fmt::Debug + Send + Sync {
    fn chat(
        &self,
        provider: &str,
        endpoint: &str,
        credential: &ModelCredential,
        body: &Value,
    ) -> ChatModelResult;
}

#[derive(Debug, Clone)]
pub struct GeminiApiChatModelClient {
    base_url: String,
    transport: Arc<dyn GeminiApiTransport>,
}

#[derive(Debug, Clone)]
struct HttpGeminiApiTransport {
    timeout: Duration,
}

trait GeminiApiTransport: std::fmt::Debug + Send + Sync {
    fn generate_content(
        &self,
        endpoint: &str,
        credential: &ModelCredential,
        body: &Value,
    ) -> ChatModelResult;
}

#[derive(Debug, Clone)]
pub struct VertexGeminiChatModelClient {
    transport: Arc<dyn VertexGeminiTransport>,
}

#[derive(Debug, Clone)]
struct HttpVertexGeminiTransport {
    timeout: Duration,
}

trait VertexGeminiTransport: std::fmt::Debug + Send + Sync {
    fn generate_content(
        &self,
        endpoint: &str,
        credential: &ModelCredential,
        body: &Value,
    ) -> ChatModelResult;
}

#[derive(Debug, Clone)]
pub struct VertexAnthropicChatModelClient {
    transport: Arc<dyn VertexAnthropicTransport>,
}

#[derive(Debug, Clone)]
struct HttpVertexAnthropicTransport {
    timeout: Duration,
}

trait VertexAnthropicTransport: std::fmt::Debug + Send + Sync {
    fn raw_predict(
        &self,
        endpoint: &str,
        credential: &ModelCredential,
        body: &Value,
    ) -> ChatModelResult;
}

#[derive(Debug, Clone)]
pub struct AzureOpenAiChatModelClient {
    transport: Arc<dyn AzureOpenAiTransport>,
}

#[derive(Debug, Clone)]
struct HttpAzureOpenAiTransport {
    timeout: Duration,
}

trait AzureOpenAiTransport: std::fmt::Debug + Send + Sync {
    fn chat(&self, endpoint: &str, credential: &ModelCredential, body: &Value) -> ChatModelResult;
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatMessageRole::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatMessageRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatMessageRole::Assistant,
            content: content.into(),
        }
    }

    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: ChatMessageRole::Tool,
            content: content.into(),
        }
    }
}

impl ChatToolSpec {
    pub fn from_tool_definition(definition: &ToolDefinition) -> Self {
        Self {
            name: definition.name.to_string(),
            description: definition.description.clone(),
            input_schema: definition.input_schema.schema.clone(),
            risk_level: definition.risk_level,
            approval_policy: definition.approval_policy,
        }
    }
}

impl ChatRequest {
    pub fn new(model: ModelRef, messages: Vec<ChatMessage>) -> Result<Self, ChatModelError> {
        if messages.is_empty() {
            return Err(ChatModelError::InvalidRequest(
                "at least one chat message is required".to_string(),
            ));
        }
        Ok(Self {
            model,
            messages,
            tools: Vec::new(),
            model_credential: None,
            temperature: None,
            max_output_tokens: None,
        })
    }

    pub fn with_tools(mut self, tools: Vec<ChatToolSpec>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_model_credential(
        mut self,
        credential: Option<ModelCredential>,
    ) -> Result<Self, ChatModelError> {
        if let Some(credential) = credential {
            if credential.provider != self.model.provider {
                return Err(ChatModelError::InvalidRequest(
                    "model credential provider does not match selected model provider".to_string(),
                ));
            }
            self.model_credential = Some(credential);
        }
        Ok(self)
    }
}

impl ChatResponse {
    pub fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            deltas: vec![text.clone()],
            text,
            finish_reason: ChatFinishReason::Stop,
            tool_calls: Vec::new(),
        }
    }

    pub fn from_deltas(deltas: Vec<String>) -> Result<Self, ChatModelError> {
        if deltas.is_empty() {
            return Err(ChatModelError::InvalidRequest(
                "at least one chat delta is required".to_string(),
            ));
        }
        Ok(Self {
            text: deltas.concat(),
            deltas,
            finish_reason: ChatFinishReason::Stop,
            tool_calls: Vec::new(),
        })
    }
}

pub fn is_empty_assistant_response_error(error: &ChatModelError) -> bool {
    match error {
        ChatModelError::InvalidProviderResponse { message, .. } => {
            let normalized = message.to_ascii_lowercase();
            normalized.contains("did not include assistant content or tool calls")
        }
        _ => false,
    }
}

pub fn should_retry_chat_model_request_error(error: &ChatModelError) -> bool {
    match error {
        ChatModelError::ProviderError { retryable, .. } => *retryable,
        ChatModelError::Transport {
            retryable, message, ..
        } => *retryable && !is_timeout_transport_error(message),
        ChatModelError::InvalidProviderResponse { message, .. } => {
            is_retryable_invalid_provider_response(message)
        }
        _ => false,
    }
}

fn is_timeout_transport_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("timeout")
        || normalized.contains("timed out")
        || normalized.contains("deadline")
}

fn is_retryable_invalid_provider_response(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("did not include assistant content or tool calls")
        || normalized.contains("did not include choices")
        || normalized.contains("finish_reason=error")
}

pub fn with_empty_response_retry_guidance(mut request: ChatRequest) -> ChatRequest {
    request.messages.push(ChatMessage::user(
        "The previous provider attempt returned empty assistant content. Return a non-empty concise response now. If this is a routing task, return only the requested JSON shape.",
    ));
    request
}

impl ChatModelError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnselectedModel => "unselected_model",
            Self::ProviderUnavailable { .. } => "provider_unavailable",
            Self::UnsupportedModel { .. } => "unsupported_model",
            Self::InvalidRequest(_) => "invalid_request",
            Self::ProviderError { .. } => "provider_error",
            Self::Transport { .. } => "transport_error",
            Self::InvalidProviderResponse { .. } => "invalid_provider_response",
        }
    }

    pub fn retryable(&self) -> bool {
        match self {
            Self::ProviderUnavailable { .. } => true,
            Self::ProviderError { retryable, .. } | Self::Transport { retryable, .. } => *retryable,
            Self::InvalidProviderResponse { message, .. } => {
                is_retryable_invalid_provider_response(message)
            }
            _ => false,
        }
    }
}

impl Default for DefaultChatModelClient {
    fn default() -> Self {
        Self {
            ollama: OllamaChatModelClient::default(),
            openai: OpenAiCompatibleChatModelClient::openai(),
            openrouter: OpenAiCompatibleChatModelClient::openrouter(),
            nvidia: OpenAiCompatibleChatModelClient::nvidia(),
            gemini_api: GeminiApiChatModelClient::default(),
            vertex_gemini: VertexGeminiChatModelClient::default(),
            vertex_anthropic: VertexAnthropicChatModelClient::default(),
            azure_openai: AzureOpenAiChatModelClient::default(),
            unavailable: UnavailableChatModelClient,
        }
    }
}

impl Default for OllamaChatModelClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaChatModelClient {
    pub fn new() -> Self {
        Self {
            transport: Arc::new(HttpOllamaTransport::new(default_ollama_base_url())),
        }
    }

    #[cfg(test)]
    fn with_transport(transport: Arc<dyn OllamaTransport>) -> Self {
        Self { transport }
    }
}

impl HttpOllamaTransport {
    fn new(base_url: String) -> Self {
        Self {
            base_url,
            timeout: Duration::from_secs(120),
        }
    }
}

impl OpenAiCompatibleChatModelClient {
    pub fn openai() -> Self {
        Self::new(
            "openai",
            "https://api.openai.com/v1",
            Arc::new(HttpOpenAiCompatibleTransport::new()),
        )
    }

    pub fn openrouter() -> Self {
        Self::new(
            "openrouter",
            "https://openrouter.ai/api/v1",
            Arc::new(HttpOpenAiCompatibleTransport::with_timeout(
                OPENROUTER_OPENAI_COMPATIBLE_TIMEOUT,
            )),
        )
    }

    pub fn nvidia() -> Self {
        Self::new(
            "nvidia",
            "https://integrate.api.nvidia.com/v1",
            Arc::new(HttpOpenAiCompatibleTransport::with_timeout(
                NVIDIA_OPENAI_COMPATIBLE_TIMEOUT,
            )),
        )
    }

    fn new(
        provider: impl Into<String>,
        default_base_url: impl Into<String>,
        transport: Arc<dyn OpenAiCompatibleTransport>,
    ) -> Self {
        Self {
            provider: provider.into(),
            default_base_url: default_base_url.into(),
            transport,
        }
    }

    #[cfg(test)]
    fn with_transport(
        provider: impl Into<String>,
        default_base_url: impl Into<String>,
        transport: Arc<dyn OpenAiCompatibleTransport>,
    ) -> Self {
        Self::new(provider, default_base_url, transport)
    }
}

impl HttpOpenAiCompatibleTransport {
    fn new() -> Self {
        Self::with_timeout(DEFAULT_OPENAI_COMPATIBLE_TIMEOUT)
    }

    fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for GeminiApiChatModelClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiApiChatModelClient {
    pub fn new() -> Self {
        Self {
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            transport: Arc::new(HttpGeminiApiTransport::new()),
        }
    }

    #[cfg(test)]
    fn with_transport(base_url: impl Into<String>, transport: Arc<dyn GeminiApiTransport>) -> Self {
        Self {
            base_url: base_url.into(),
            transport,
        }
    }
}

impl HttpGeminiApiTransport {
    fn new() -> Self {
        Self {
            timeout: Duration::from_secs(120),
        }
    }
}

impl Default for VertexGeminiChatModelClient {
    fn default() -> Self {
        Self::new()
    }
}

impl VertexGeminiChatModelClient {
    pub fn new() -> Self {
        Self {
            transport: Arc::new(HttpVertexGeminiTransport::new()),
        }
    }

    #[cfg(test)]
    fn with_transport(transport: Arc<dyn VertexGeminiTransport>) -> Self {
        Self { transport }
    }
}

impl HttpVertexGeminiTransport {
    fn new() -> Self {
        Self {
            timeout: Duration::from_secs(120),
        }
    }
}

impl Default for VertexAnthropicChatModelClient {
    fn default() -> Self {
        Self::new()
    }
}

impl VertexAnthropicChatModelClient {
    pub fn new() -> Self {
        Self {
            transport: Arc::new(HttpVertexAnthropicTransport::new()),
        }
    }

    #[cfg(test)]
    fn with_transport(transport: Arc<dyn VertexAnthropicTransport>) -> Self {
        Self { transport }
    }
}

impl HttpVertexAnthropicTransport {
    fn new() -> Self {
        Self {
            timeout: Duration::from_secs(120),
        }
    }
}

impl Default for AzureOpenAiChatModelClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AzureOpenAiChatModelClient {
    pub fn new() -> Self {
        Self {
            transport: Arc::new(HttpAzureOpenAiTransport::new()),
        }
    }

    #[cfg(test)]
    fn with_transport(transport: Arc<dyn AzureOpenAiTransport>) -> Self {
        Self { transport }
    }
}

impl HttpAzureOpenAiTransport {
    fn new() -> Self {
        Self {
            timeout: Duration::from_secs(120),
        }
    }
}

impl ChatModelClient for DefaultChatModelClient {
    fn complete(&self, request: ChatRequest) -> ChatModelResult {
        match request.model.provider.as_str() {
            "ollama" => self.ollama.complete(request),
            "openai" => self.openai.complete(request),
            "openrouter" => self.openrouter.complete(request),
            "nvidia" => self.nvidia.complete(request),
            "vertex" if is_vertex_anthropic_model(&request.model.name) => {
                self.vertex_anthropic.complete(request)
            }
            "vertex"
                if request.model_credential.as_ref().is_some_and(|credential| {
                    looks_like_google_oauth_token(credential.token.trim())
                }) =>
            {
                self.vertex_gemini.complete(request)
            }
            "vertex" => self.gemini_api.complete(request),
            "azure" => self.azure_openai.complete(request),
            _ => self.unavailable.complete(request),
        }
    }
}

impl ChatModelClient for OllamaChatModelClient {
    fn complete(&self, request: ChatRequest) -> ChatModelResult {
        if request.model.provider != "ollama" {
            return Err(ChatModelError::UnsupportedModel {
                provider: request.model.provider,
                model: request.model.name,
            });
        }

        self.transport.chat(&ollama_chat_body(&request))
    }
}

impl ChatModelClient for UnavailableChatModelClient {
    fn complete(&self, request: ChatRequest) -> ChatModelResult {
        if request.model.name == "unselected" || request.model.provider == "coddy" {
            return Err(ChatModelError::UnselectedModel);
        }

        Err(ChatModelError::ProviderUnavailable {
            provider: request.model.provider,
        })
    }
}

impl ChatModelClient for OpenAiCompatibleChatModelClient {
    fn complete(&self, request: ChatRequest) -> ChatModelResult {
        if request.model.provider != self.provider {
            return Err(ChatModelError::UnsupportedModel {
                provider: request.model.provider,
                model: request.model.name,
            });
        }

        let credential = request.model_credential.as_ref().ok_or_else(|| {
            ChatModelError::InvalidRequest(format!(
                "{} chat execution requires a provider credential",
                self.provider
            ))
        })?;
        if credential.token.trim().is_empty() {
            return Err(ChatModelError::InvalidRequest(format!(
                "{} chat execution requires a non-empty provider credential",
                self.provider
            )));
        }

        let endpoint =
            openai_compatible_chat_url(&self.default_base_url, credential.endpoint.as_deref())?;
        self.transport.chat(
            &self.provider,
            &endpoint,
            credential,
            &openai_compatible_chat_body(&request),
        )
    }
}

impl ChatModelClient for GeminiApiChatModelClient {
    fn complete(&self, request: ChatRequest) -> ChatModelResult {
        if request.model.provider != "vertex" || is_vertex_anthropic_model(&request.model.name) {
            return Err(ChatModelError::UnsupportedModel {
                provider: request.model.provider,
                model: request.model.name,
            });
        }
        reject_unsupported_gemini_chat_model(&request.model.name)?;

        let credential = request.model_credential.as_ref().ok_or_else(|| {
            ChatModelError::InvalidRequest(
                "Gemini API chat execution requires a Google API key".to_string(),
            )
        })?;
        let token = credential.token.trim();
        if token.is_empty() {
            return Err(ChatModelError::InvalidRequest(
                "Gemini API chat execution requires a non-empty Google API key".to_string(),
            ));
        }
        if looks_like_google_oauth_token(token) {
            return Err(ChatModelError::InvalidRequest(
                "Gemini API chat execution requires a Google API key; OAuth/ADC credentials must use the Vertex AI runtime route".to_string(),
            ));
        }

        let endpoint = gemini_api_generate_content_url(
            &self.base_url,
            credential.endpoint.as_deref(),
            &request.model.name,
        )?;
        self.transport
            .generate_content(&endpoint, credential, &gemini_api_chat_body(&request)?)
    }
}

impl ChatModelClient for VertexGeminiChatModelClient {
    fn complete(&self, request: ChatRequest) -> ChatModelResult {
        if request.model.provider != "vertex" || is_vertex_anthropic_model(&request.model.name) {
            return Err(ChatModelError::UnsupportedModel {
                provider: request.model.provider,
                model: request.model.name,
            });
        }
        reject_unsupported_gemini_chat_model(&request.model.name)?;

        let credential = request.model_credential.as_ref().ok_or_else(|| {
            ChatModelError::InvalidRequest(
                "Vertex Gemini chat execution requires a Google OAuth credential".to_string(),
            )
        })?;
        let token = credential.token.trim();
        if token.is_empty() {
            return Err(ChatModelError::InvalidRequest(
                "Vertex Gemini chat execution requires a non-empty Google OAuth credential"
                    .to_string(),
            ));
        }
        if !looks_like_google_oauth_token(token) {
            return Err(ChatModelError::InvalidRequest(
                "Vertex Gemini chat execution requires Google OAuth, ADC or gcloud credentials; Google API keys use the Gemini API route".to_string(),
            ));
        }

        let endpoint = vertex_gemini_generate_content_url(&request.model.name, credential)?;
        self.transport
            .generate_content(&endpoint, credential, &gemini_api_chat_body(&request)?)
    }
}

impl ChatModelClient for VertexAnthropicChatModelClient {
    fn complete(&self, request: ChatRequest) -> ChatModelResult {
        if request.model.provider != "vertex" {
            return Err(ChatModelError::UnsupportedModel {
                provider: request.model.provider,
                model: request.model.name,
            });
        }
        if !is_vertex_anthropic_model(&request.model.name) {
            return Err(ChatModelError::UnsupportedModel {
                provider: request.model.provider,
                model: request.model.name,
            });
        }

        let credential = request.model_credential.as_ref().ok_or_else(|| {
            ChatModelError::InvalidRequest(
                "Vertex Anthropic chat execution requires a Google OAuth credential".to_string(),
            )
        })?;
        if credential.token.trim().is_empty() {
            return Err(ChatModelError::InvalidRequest(
                "Vertex Anthropic chat execution requires a non-empty Google OAuth credential"
                    .to_string(),
            ));
        }

        let endpoint = vertex_anthropic_raw_predict_url(&request.model.name, credential)?;
        self.transport.raw_predict(
            &endpoint,
            credential,
            &vertex_anthropic_chat_body(&request)?,
        )
    }
}

impl ChatModelClient for AzureOpenAiChatModelClient {
    fn complete(&self, request: ChatRequest) -> ChatModelResult {
        if request.model.provider != "azure" {
            return Err(ChatModelError::UnsupportedModel {
                provider: request.model.provider,
                model: request.model.name,
            });
        }

        let credential = request.model_credential.as_ref().ok_or_else(|| {
            ChatModelError::InvalidRequest(
                "Azure OpenAI chat execution requires an API key and endpoint".to_string(),
            )
        })?;
        if credential.token.trim().is_empty() {
            return Err(ChatModelError::InvalidRequest(
                "Azure OpenAI chat execution requires a non-empty API key".to_string(),
            ));
        }

        let endpoint = azure_openai_chat_url(&request.model.name, credential)?;
        self.transport
            .chat(&endpoint, credential, &azure_openai_chat_body(&request))
    }
}

impl OllamaTransport for HttpOllamaTransport {
    fn chat(&self, body: &Value) -> ChatModelResult {
        let target = OllamaHttpTarget::parse(&self.base_url)?;
        let address = resolve_socket_addr(&target.host, target.port)?;
        let mut stream = TcpStream::connect_timeout(&address, self.timeout).map_err(|error| {
            ChatModelError::Transport {
                provider: "ollama".to_string(),
                message: error.to_string(),
                retryable: true,
            }
        })?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(ollama_transport_error)?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(ollama_transport_error)?;

        let body = serde_json::to_string(body).map_err(|error| {
            ChatModelError::InvalidRequest(format!("failed to encode Ollama request: {error}"))
        })?;
        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/x-ndjson, application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            target.path,
            target.host_header(),
            body.len(),
            body
        );
        stream
            .write_all(request.as_bytes())
            .map_err(ollama_transport_error)?;

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(ollama_transport_error)?;
        parse_http_ollama_response(&response)
    }
}

impl OpenAiCompatibleTransport for HttpOpenAiCompatibleTransport {
    fn chat(
        &self,
        provider: &str,
        endpoint: &str,
        credential: &ModelCredential,
        body: &Value,
    ) -> ChatModelResult {
        let body = serde_json::to_string(body).map_err(|error| {
            ChatModelError::InvalidRequest(format!(
                "failed to encode {provider} chat request: {error}"
            ))
        })?;
        let authorization = format!("Bearer {}", credential.token.trim());
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let response = agent
            .post(endpoint)
            .set("Accept", "application/json")
            .set("Authorization", &authorization)
            .set("Content-Type", "application/json")
            .send_string(&body);

        match response {
            Ok(response) => {
                let body = response
                    .into_string()
                    .map_err(|error| openai_compatible_transport_error(provider, error))?;
                parse_openai_compatible_chat_body(provider, &body)
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(ChatModelError::ProviderError {
                    provider: provider.to_string(),
                    message: openai_compatible_provider_error_message(provider, &body, status),
                    retryable: is_retryable_provider_status(status),
                })
            }
            Err(ureq::Error::Transport(error)) => Err(ChatModelError::Transport {
                provider: provider.to_string(),
                message: error.to_string(),
                retryable: true,
            }),
        }
    }
}

impl GeminiApiTransport for HttpGeminiApiTransport {
    fn generate_content(
        &self,
        endpoint: &str,
        credential: &ModelCredential,
        body: &Value,
    ) -> ChatModelResult {
        let body = serde_json::to_string(body).map_err(|error| {
            ChatModelError::InvalidRequest(format!("failed to encode Gemini API request: {error}"))
        })?;
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let response = agent
            .post(endpoint)
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("x-goog-api-key", credential.token.trim())
            .send_string(&body);

        match response {
            Ok(response) => {
                let body = response.into_string().map_err(gemini_api_transport_error)?;
                parse_gemini_api_response(&body)
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(ChatModelError::ProviderError {
                    provider: "vertex".to_string(),
                    message: gemini_api_provider_error_message(&body, status),
                    retryable: status == 429 || status >= 500,
                })
            }
            Err(ureq::Error::Transport(error)) => Err(ChatModelError::Transport {
                provider: "vertex".to_string(),
                message: error.to_string(),
                retryable: true,
            }),
        }
    }
}

impl VertexGeminiTransport for HttpVertexGeminiTransport {
    fn generate_content(
        &self,
        endpoint: &str,
        credential: &ModelCredential,
        body: &Value,
    ) -> ChatModelResult {
        let body = serde_json::to_string(body).map_err(|error| {
            ChatModelError::InvalidRequest(format!(
                "failed to encode Vertex Gemini request: {error}"
            ))
        })?;
        let authorization = format!("Bearer {}", credential.token.trim());
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let mut request = agent
            .post(endpoint)
            .set("Accept", "application/json")
            .set("Authorization", &authorization)
            .set("Content-Type", "application/json; charset=utf-8");
        if let Some(quota_project_id) = vertex_quota_project_id(credential) {
            request = request.set("x-goog-user-project", quota_project_id);
        }
        let response = request.send_string(&body);

        match response {
            Ok(response) => {
                let body = response
                    .into_string()
                    .map_err(vertex_gemini_transport_error)?;
                parse_gemini_api_response(&body)
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(ChatModelError::ProviderError {
                    provider: "vertex".to_string(),
                    message: vertex_gemini_provider_error_message(&body, status),
                    retryable: status == 429 || status >= 500,
                })
            }
            Err(ureq::Error::Transport(error)) => Err(ChatModelError::Transport {
                provider: "vertex".to_string(),
                message: error.to_string(),
                retryable: true,
            }),
        }
    }
}

impl VertexAnthropicTransport for HttpVertexAnthropicTransport {
    fn raw_predict(
        &self,
        endpoint: &str,
        credential: &ModelCredential,
        body: &Value,
    ) -> ChatModelResult {
        let body = serde_json::to_string(body).map_err(|error| {
            ChatModelError::InvalidRequest(format!(
                "failed to encode Vertex Anthropic request: {error}"
            ))
        })?;
        let authorization = format!("Bearer {}", credential.token.trim());
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let mut request = agent
            .post(endpoint)
            .set("Accept", "application/json")
            .set("Authorization", &authorization)
            .set("Content-Type", "application/json; charset=utf-8");
        if let Some(quota_project_id) = vertex_quota_project_id(credential) {
            request = request.set("x-goog-user-project", quota_project_id);
        }
        let response = request.send_string(&body);

        match response {
            Ok(response) => {
                let body = response
                    .into_string()
                    .map_err(vertex_anthropic_transport_error)?;
                parse_vertex_anthropic_response(&body)
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(ChatModelError::ProviderError {
                    provider: "vertex".to_string(),
                    message: vertex_anthropic_provider_error_message(&body, status),
                    retryable: status == 429 || status >= 500,
                })
            }
            Err(ureq::Error::Transport(error)) => Err(ChatModelError::Transport {
                provider: "vertex".to_string(),
                message: error.to_string(),
                retryable: true,
            }),
        }
    }
}

impl AzureOpenAiTransport for HttpAzureOpenAiTransport {
    fn chat(&self, endpoint: &str, credential: &ModelCredential, body: &Value) -> ChatModelResult {
        let body = serde_json::to_string(body).map_err(|error| {
            ChatModelError::InvalidRequest(format!(
                "failed to encode Azure OpenAI chat request: {error}"
            ))
        })?;
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let response = agent
            .post(endpoint)
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("api-key", credential.token.trim())
            .send_string(&body);

        match response {
            Ok(response) => {
                let body = response
                    .into_string()
                    .map_err(azure_openai_transport_error)?;
                parse_openai_compatible_chat_body("azure", &body)
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(ChatModelError::ProviderError {
                    provider: "azure".to_string(),
                    message: azure_openai_provider_error_message(&body, status),
                    retryable: status == 429 || status >= 500,
                })
            }
            Err(ureq::Error::Transport(error)) => Err(ChatModelError::Transport {
                provider: "azure".to_string(),
                message: error.to_string(),
                retryable: true,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OllamaHttpTarget {
    host: String,
    port: u16,
    path: String,
}

impl OllamaHttpTarget {
    fn parse(base_url: &str) -> Result<Self, ChatModelError> {
        let trimmed = base_url.trim();
        if trimmed.contains("://") && !trimmed.starts_with("http://") {
            return Err(ChatModelError::InvalidRequest(
                "Ollama base URL must use http:// or host:port".to_string(),
            ));
        }
        let without_scheme = trimmed
            .strip_prefix("http://")
            .unwrap_or(trimmed)
            .trim_end_matches('/');
        let (authority, path) = without_scheme
            .split_once('/')
            .map(|(authority, path)| {
                let path = format!("/{path}");
                let path = if path == "/" {
                    "/api/chat".to_string()
                } else {
                    path
                };
                (authority, path)
            })
            .unwrap_or((without_scheme, "/api/chat".to_string()));
        let (host, port) = authority
            .rsplit_once(':')
            .and_then(|(host, port)| Some((host.to_string(), port.parse::<u16>().ok()?)))
            .unwrap_or_else(|| (authority.to_string(), 11434));

        if host.trim().is_empty() {
            return Err(ChatModelError::InvalidRequest(
                "Ollama host cannot be empty".to_string(),
            ));
        }

        Ok(Self { host, port, path })
    }

    fn host_header(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn default_ollama_base_url() -> String {
    std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://127.0.0.1:11434".to_string())
}

fn resolve_socket_addr(host: &str, port: u16) -> Result<SocketAddr, ChatModelError> {
    (host, port)
        .to_socket_addrs()
        .map_err(ollama_transport_error)?
        .next()
        .ok_or_else(|| ChatModelError::Transport {
            provider: "ollama".to_string(),
            message: format!("could not resolve {host}:{port}"),
            retryable: true,
        })
}

fn ollama_transport_error(error: std::io::Error) -> ChatModelError {
    ChatModelError::Transport {
        provider: "ollama".to_string(),
        message: error.to_string(),
        retryable: true,
    }
}

fn ollama_chat_body(request: &ChatRequest) -> Value {
    let messages = request
        .messages
        .iter()
        .map(|message| {
            serde_json::json!({
                "role": chat_role_name(message.role),
                "content": message.content,
            })
        })
        .collect::<Vec<_>>();
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect::<Vec<_>>();

    let mut body = serde_json::json!({
        "model": request.model.name,
        "messages": messages,
        "stream": true,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    if request.temperature.is_some() || request.max_output_tokens.is_some() {
        let mut options = serde_json::Map::new();
        if let Some(temperature) = request.temperature {
            options.insert("temperature".to_string(), serde_json::json!(temperature));
        }
        if let Some(max_output_tokens) = request.max_output_tokens {
            options.insert(
                "num_predict".to_string(),
                serde_json::json!(max_output_tokens),
            );
        }
        body["options"] = Value::Object(options);
    }
    body
}

fn openai_compatible_chat_body(request: &ChatRequest) -> Value {
    let messages = request
        .messages
        .iter()
        .map(openai_compatible_message_body)
        .collect::<Vec<_>>();
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": provider_safe_tool_name(&tool.name),
                    "description": provider_safe_tool_description(tool),
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect::<Vec<_>>();

    let mut body = serde_json::json!({
        "model": request.model.name,
        "messages": messages,
        "stream": false,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = Value::String("auto".to_string());
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }
    if let Some(max_output_tokens) = request.max_output_tokens {
        body["max_tokens"] = serde_json::json!(max_output_tokens);
    }
    body
}

fn provider_safe_tool_name(name: &str) -> String {
    if is_provider_safe_function_name(name) {
        return name.to_string();
    }

    format!("coddy_tool__{}", name.replace('.', "__dot__"))
}

fn provider_safe_tool_description(tool: &ChatToolSpec) -> String {
    let name = provider_safe_tool_name(&tool.name);
    if name == tool.name {
        tool.description.clone()
    } else {
        format!("Coddy tool `{}`. {}", tool.name, tool.description)
    }
}

pub fn decode_provider_safe_tool_name(name: &str) -> String {
    let alias = name.strip_prefix("coddy_tool__").unwrap_or(name);
    let decoded = alias
        .replace("__dot__", ".")
        .replace(".dot.", ".")
        .replace("::", ".");
    if decoded != alias {
        return normalize_provider_tool_method_alias(&decoded);
    }

    for namespace in ["filesystem", "subagent", "shell"] {
        if let Some(method) = alias.strip_prefix(&format!("{namespace}._")) {
            if !method.is_empty() {
                return format!(
                    "{namespace}.{}",
                    camel_or_kebab_to_snake(strip_legacy_dot_method_prefix(method))
                );
            }
        }
        if let Some(method) = alias.strip_prefix(&format!("{namespace}.")) {
            if !method.is_empty() {
                return format!(
                    "{namespace}.{}",
                    camel_or_kebab_to_snake(strip_legacy_dot_method_prefix(method))
                );
            }
        }
        if let Some(method) = alias.strip_prefix(&format!("{namespace}_")) {
            if !method.is_empty() {
                return format!(
                    "{namespace}.{}",
                    camel_or_kebab_to_snake(strip_legacy_dot_method_prefix(method))
                );
            }
        }
    }

    name.to_string()
}

fn normalize_provider_tool_method_alias(name: &str) -> String {
    for namespace in ["filesystem", "subagent", "shell"] {
        if let Some(method) = name.strip_prefix(&format!("{namespace}.")) {
            if !method.is_empty() {
                return format!(
                    "{namespace}.{}",
                    camel_or_kebab_to_snake(strip_legacy_dot_method_prefix(method))
                );
            }
        }
    }
    name.to_string()
}

fn strip_legacy_dot_method_prefix(method: &str) -> &str {
    method.strip_prefix("dot_").unwrap_or(method)
}

fn camel_or_kebab_to_snake(method: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_separator = true;
    for character in method.chars() {
        if matches!(character, '-' | '.' | ' ' | ':') {
            if !normalized.is_empty() && !previous_was_separator {
                normalized.push('_');
                previous_was_separator = true;
            }
            continue;
        }
        if character.is_ascii_uppercase() {
            if !normalized.is_empty() && !previous_was_separator {
                normalized.push('_');
            }
            normalized.push(character.to_ascii_lowercase());
            previous_was_separator = false;
            continue;
        }
        normalized.push(character);
        previous_was_separator = character == '_';
    }
    normalized
}

fn is_provider_safe_function_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn openai_compatible_message_body(message: &ChatMessage) -> Value {
    let role = match message.role {
        ChatMessageRole::System => "system",
        ChatMessageRole::User | ChatMessageRole::Tool => "user",
        ChatMessageRole::Assistant => "assistant",
    };
    let content = if message.role == ChatMessageRole::Tool {
        normalized_tool_observation_content(&message.content)
    } else {
        message.content.clone()
    };
    serde_json::json!({
        "role": role,
        "content": content,
    })
}

fn normalized_tool_observation_content(content: &str) -> String {
    let trimmed = content.trim_start();
    if trimmed.starts_with("Tool observations:") {
        content.to_string()
    } else {
        format!("Tool observations:\n{content}")
    }
}

fn openai_compatible_chat_url(
    default_base_url: &str,
    endpoint_override: Option<&str>,
) -> Result<String, ChatModelError> {
    let endpoint = endpoint_override
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .unwrap_or(default_base_url)
        .trim_end_matches('/');

    if !endpoint.starts_with("https://") {
        return Err(ChatModelError::InvalidRequest(
            "OpenAI-compatible runtime endpoints must use HTTPS".to_string(),
        ));
    }

    if endpoint.ends_with("/chat/completions") {
        Ok(endpoint.to_string())
    } else {
        Ok(format!("{endpoint}/chat/completions"))
    }
}

fn gemini_api_chat_body(request: &ChatRequest) -> Result<Value, ChatModelError> {
    let (system_instruction, contents) = gemini_api_contents(&request.messages)?;
    let function_declarations = request
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": provider_safe_tool_name(&tool.name),
                "description": provider_safe_tool_description(tool),
                "parameters": gemini_api_tool_parameters(&tool.input_schema),
            })
        })
        .collect::<Vec<_>>();

    let mut body = serde_json::json!({
        "contents": contents,
    });
    if let Some(system_instruction) = system_instruction {
        body["systemInstruction"] = serde_json::json!({
            "parts": [
                { "text": system_instruction }
            ]
        });
    }
    if !function_declarations.is_empty() {
        body["tools"] = serde_json::json!([
            {
                "functionDeclarations": function_declarations
            }
        ]);
    }
    if request.temperature.is_some() || request.max_output_tokens.is_some() {
        let mut generation_config = serde_json::Map::new();
        if let Some(temperature) = request.temperature {
            generation_config.insert("temperature".to_string(), serde_json::json!(temperature));
        }
        if let Some(max_output_tokens) = request.max_output_tokens {
            generation_config.insert(
                "maxOutputTokens".to_string(),
                serde_json::json!(max_output_tokens),
            );
        }
        body["generationConfig"] = Value::Object(generation_config);
    }
    Ok(body)
}

fn gemini_api_tool_parameters(schema: &Value) -> Value {
    let mut schema = schema.clone();
    strip_gemini_unsupported_schema_keywords(&mut schema, false);
    schema
}

fn strip_gemini_unsupported_schema_keywords(value: &mut Value, is_properties_map: bool) {
    match value {
        Value::Object(map) => {
            if !is_properties_map {
                map.remove("additionalProperties");
            }

            for (key, child) in map.iter_mut() {
                if key == "properties" {
                    if let Value::Object(properties) = child {
                        for property_schema in properties.values_mut() {
                            strip_gemini_unsupported_schema_keywords(property_schema, false);
                        }
                    }
                    continue;
                }

                strip_gemini_unsupported_schema_keywords(child, false);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_gemini_unsupported_schema_keywords(item, false);
            }
        }
        _ => {}
    }
}

fn gemini_api_contents(
    messages: &[ChatMessage],
) -> Result<(Option<String>, Vec<Value>), ChatModelError> {
    let mut system = Vec::new();
    let mut normalized = Vec::<(String, String)>::new();

    for message in messages {
        match message.role {
            ChatMessageRole::System => system.push(message.content.trim().to_string()),
            ChatMessageRole::User => normalized.push(("user".to_string(), message.content.clone())),
            ChatMessageRole::Assistant => {
                normalized.push(("model".to_string(), message.content.clone()))
            }
            ChatMessageRole::Tool => normalized.push((
                "user".to_string(),
                normalized_tool_observation_content(&message.content),
            )),
        }
    }

    let mut collapsed = Vec::<(String, String)>::new();
    for (role, content) in normalized {
        if content.trim().is_empty() {
            continue;
        }
        if let Some((last_role, last_content)) = collapsed.last_mut() {
            if *last_role == role {
                last_content.push_str("\n\n");
                last_content.push_str(&content);
                continue;
            }
        }
        collapsed.push((role, content));
    }

    if collapsed.is_empty() || collapsed[0].0 != "user" {
        return Err(ChatModelError::InvalidRequest(
            "Gemini API requests require at least one user message".to_string(),
        ));
    }

    let contents = collapsed
        .into_iter()
        .map(|(role, content)| {
            serde_json::json!({
                "role": role,
                "parts": [
                    { "text": content }
                ]
            })
        })
        .collect();
    let system = system
        .into_iter()
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let system = if system.is_empty() {
        None
    } else {
        Some(system)
    };
    Ok((system, contents))
}

fn gemini_api_generate_content_url(
    default_base_url: &str,
    endpoint_override: Option<&str>,
    model: &str,
) -> Result<String, ChatModelError> {
    let base_url = match endpoint_override.map(str::trim) {
        Some(endpoint) if endpoint.starts_with("https://") => endpoint,
        Some(endpoint) if endpoint.contains("://") => {
            return Err(ChatModelError::InvalidRequest(
                "Gemini API runtime endpoint must use HTTPS".to_string(),
            ));
        }
        _ => default_base_url,
    }
    .trim_end_matches('/');
    if !base_url.starts_with("https://") {
        return Err(ChatModelError::InvalidRequest(
            "Gemini API runtime endpoint must use HTTPS".to_string(),
        ));
    }

    let model = model.trim();
    validate_gemini_model_name(model)?;
    let model = model.strip_prefix("models/").unwrap_or(model);
    Ok(format!("{base_url}/models/{model}:generateContent"))
}

fn validate_gemini_model_name(model: &str) -> Result<(), ChatModelError> {
    if model.is_empty()
        || model.chars().any(|character| character.is_whitespace())
        || (model.contains('/') && !model.starts_with("models/"))
    {
        return Err(ChatModelError::InvalidRequest(
            "Gemini API model name is invalid".to_string(),
        ));
    }
    Ok(())
}

fn reject_unsupported_gemini_chat_model(model: &str) -> Result<(), ChatModelError> {
    if is_gemini_live_api_model(model) {
        return Err(ChatModelError::InvalidRequest(format!(
            "The selected Gemini model `{model}` does not support the standard text chat runtime. Reload models and choose a Gemini model that supports generateContent, or use a Live API/audio runtime when Coddy supports it."
        )));
    }
    Ok(())
}

fn is_gemini_live_api_model(model: &str) -> bool {
    model
        .trim()
        .split(['-', '_', '/'])
        .any(|segment| segment.eq_ignore_ascii_case("live"))
}

fn looks_like_google_oauth_token(token: &str) -> bool {
    token.starts_with("ya29.") || token.to_ascii_lowercase().starts_with("bearer ")
}

fn azure_openai_chat_body(request: &ChatRequest) -> Value {
    let mut body = openai_compatible_chat_body(request);
    if let Some(object) = body.as_object_mut() {
        object.remove("model");
    }
    body
}

fn azure_openai_chat_url(
    deployment: &str,
    credential: &ModelCredential,
) -> Result<String, ChatModelError> {
    let endpoint = credential
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .ok_or_else(|| {
            ChatModelError::InvalidRequest(
                "Azure OpenAI chat execution requires an HTTPS resource endpoint".to_string(),
            )
        })?;
    if !endpoint.starts_with("https://") {
        return Err(ChatModelError::InvalidRequest(
            "Azure OpenAI runtime endpoint must use HTTPS".to_string(),
        ));
    }

    let deployment = deployment.trim();
    validate_azure_deployment_name(deployment)?;
    let api_version = credential
        .metadata
        .get("api_version")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("2024-10-21");
    validate_azure_api_version(api_version)?;

    Ok(format!(
        "{}/openai/deployments/{}/chat/completions?api-version={}",
        endpoint.trim_end_matches('/'),
        deployment,
        api_version
    ))
}

fn validate_azure_deployment_name(deployment: &str) -> Result<(), ChatModelError> {
    if deployment.is_empty()
        || deployment.chars().any(|character| {
            character.is_whitespace() || matches!(character, '/' | '?' | '&' | '#')
        })
    {
        return Err(ChatModelError::InvalidRequest(
            "Azure OpenAI deployment name is invalid".to_string(),
        ));
    }
    Ok(())
}

fn validate_azure_api_version(api_version: &str) -> Result<(), ChatModelError> {
    if api_version.is_empty()
        || api_version
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '&' | '?' | '#'))
    {
        return Err(ChatModelError::InvalidRequest(
            "Azure OpenAI API version is invalid".to_string(),
        ));
    }
    Ok(())
}

fn vertex_anthropic_chat_body(request: &ChatRequest) -> Result<Value, ChatModelError> {
    let (system, messages) = vertex_anthropic_messages(&request.messages)?;
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": provider_safe_tool_name(&tool.name),
                "description": provider_safe_tool_description(tool),
                "input_schema": tool.input_schema,
            })
        })
        .collect::<Vec<_>>();

    let mut body = serde_json::json!({
        "anthropic_version": "vertex-2023-10-16",
        "max_tokens": request.max_output_tokens.unwrap_or(1024),
        "stream": false,
        "messages": messages,
    });
    if !system.is_empty() {
        body["system"] = Value::String(system);
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }
    Ok(body)
}

fn vertex_anthropic_messages(
    messages: &[ChatMessage],
) -> Result<(String, Vec<Value>), ChatModelError> {
    let mut system = Vec::new();
    let mut normalized = Vec::<(String, String)>::new();

    for message in messages {
        match message.role {
            ChatMessageRole::System => system.push(message.content.trim().to_string()),
            ChatMessageRole::User => normalized.push(("user".to_string(), message.content.clone())),
            ChatMessageRole::Assistant => {
                normalized.push(("assistant".to_string(), message.content.clone()))
            }
            ChatMessageRole::Tool => normalized.push((
                "user".to_string(),
                normalized_tool_observation_content(&message.content),
            )),
        }
    }

    let mut collapsed = Vec::<(String, String)>::new();
    for (role, content) in normalized {
        if content.trim().is_empty() {
            continue;
        }
        if let Some((last_role, last_content)) = collapsed.last_mut() {
            if *last_role == role {
                last_content.push_str("\n\n");
                last_content.push_str(&content);
                continue;
            }
        }
        collapsed.push((role, content));
    }

    if collapsed.is_empty() || collapsed[0].0 != "user" {
        return Err(ChatModelError::InvalidRequest(
            "Vertex Anthropic requests require at least one user message".to_string(),
        ));
    }

    let messages = collapsed
        .into_iter()
        .map(|(role, content)| {
            serde_json::json!({
                "role": role,
                "content": content,
            })
        })
        .collect();
    let system = system
        .into_iter()
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok((system, messages))
}

fn vertex_anthropic_raw_predict_url(
    model: &str,
    credential: &ModelCredential,
) -> Result<String, ChatModelError> {
    let project_id = credential
        .metadata
        .get("project_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ChatModelError::InvalidRequest(
                "Vertex Anthropic chat execution requires metadata.project_id from gcloud config"
                    .to_string(),
            )
        })?;
    validate_vertex_path_segment("project id", project_id)?;

    let region = credential
        .metadata
        .get("region")
        .map(String::as_str)
        .or_else(|| vertex_region_from_endpoint(credential.endpoint.as_deref()))
        .unwrap_or("us-east5")
        .trim();
    validate_vertex_path_segment("region", region)?;
    validate_vertex_path_segment("model", model)?;

    let base_url = vertex_base_url(region, credential.endpoint.as_deref())?;
    Ok(format!(
        "{base_url}/v1/projects/{project_id}/locations/{region}/publishers/anthropic/models/{model}:rawPredict"
    ))
}

fn vertex_gemini_generate_content_url(
    model: &str,
    credential: &ModelCredential,
) -> Result<String, ChatModelError> {
    let project_id = credential
        .metadata
        .get("project_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ChatModelError::InvalidRequest(
                "Vertex Gemini chat execution requires metadata.project_id from gcloud config"
                    .to_string(),
            )
        })?;
    validate_vertex_path_segment("project id", project_id)?;

    let region = credential
        .metadata
        .get("region")
        .map(String::as_str)
        .or_else(|| vertex_region_from_endpoint(credential.endpoint.as_deref()))
        .unwrap_or("global")
        .trim();
    validate_vertex_path_segment("region", region)?;

    let model = model.trim().strip_prefix("models/").unwrap_or(model.trim());
    validate_vertex_path_segment("model", model)?;

    let base_url = vertex_base_url(region, credential.endpoint.as_deref())?;
    Ok(format!(
        "{base_url}/v1/projects/{project_id}/locations/{region}/publishers/google/models/{model}:generateContent"
    ))
}

fn vertex_base_url(
    region: &str,
    endpoint_override: Option<&str>,
) -> Result<String, ChatModelError> {
    if let Some(endpoint) = endpoint_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if endpoint.starts_with("https://") {
            return Ok(endpoint.trim_end_matches('/').to_string());
        }
        if endpoint.contains("://") {
            return Err(ChatModelError::InvalidRequest(
                "Vertex runtime endpoint must use HTTPS".to_string(),
            ));
        }
    }

    if region == "global" {
        Ok("https://aiplatform.googleapis.com".to_string())
    } else {
        Ok(format!("https://{region}-aiplatform.googleapis.com"))
    }
}

fn vertex_quota_project_id(credential: &ModelCredential) -> Option<&str> {
    credential
        .metadata
        .get("quota_project_id")
        .or_else(|| credential.metadata.get("project_id"))
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn vertex_region_from_endpoint(endpoint: Option<&str>) -> Option<&str> {
    let value = endpoint?.trim();
    if value.is_empty() || value.starts_with("https://") {
        return None;
    }
    Some(value)
}

fn validate_vertex_path_segment(label: &str, value: &str) -> Result<(), ChatModelError> {
    if value.is_empty()
        || value
            .chars()
            .any(|character| character.is_whitespace() || character == '/')
    {
        return Err(ChatModelError::InvalidRequest(format!(
            "Vertex {label} is invalid"
        )));
    }
    Ok(())
}

fn is_vertex_anthropic_model(model: &str) -> bool {
    model.starts_with("claude-")
}

fn chat_role_name(role: ChatMessageRole) -> &'static str {
    match role {
        ChatMessageRole::System => "system",
        ChatMessageRole::User => "user",
        ChatMessageRole::Assistant => "assistant",
        ChatMessageRole::Tool => "tool",
    }
}

fn parse_http_ollama_response(response: &[u8]) -> ChatModelResult {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| ChatModelError::InvalidProviderResponse {
            provider: "ollama".to_string(),
            message: "HTTP response is missing header terminator".to_string(),
        })?;
    let head = String::from_utf8_lossy(&response[..header_end]);
    let body = &response[header_end + 4..];
    let status = parse_http_status(&head)?;
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        decode_chunked_body(body)?
    } else {
        String::from_utf8_lossy(body).to_string()
    };

    if !(200..300).contains(&status) {
        return Err(ChatModelError::ProviderError {
            provider: "ollama".to_string(),
            message: provider_error_message(&body, status),
            retryable: status == 429 || status >= 500,
        });
    }

    parse_ollama_chat_body(&body)
}

fn parse_http_status(head: &str) -> Result<u16, ChatModelError> {
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| ChatModelError::InvalidProviderResponse {
            provider: "ollama".to_string(),
            message: "HTTP status line is invalid".to_string(),
        })?;
    Ok(status)
}

fn decode_chunked_body(body: &[u8]) -> Result<String, ChatModelError> {
    let mut cursor = 0_usize;
    let mut decoded = Vec::new();
    loop {
        let line_end =
            find_crlf(&body[cursor..]).ok_or_else(|| ChatModelError::InvalidProviderResponse {
                provider: "ollama".to_string(),
                message: "chunked response is missing chunk size".to_string(),
            })?;
        let size_line = String::from_utf8_lossy(&body[cursor..cursor + line_end]);
        let size_text = size_line.split(';').next().unwrap_or(&size_line);
        let size = usize::from_str_radix(size_text.trim(), 16).map_err(|error| {
            ChatModelError::InvalidProviderResponse {
                provider: "ollama".to_string(),
                message: format!("invalid chunk size: {error}"),
            }
        })?;
        cursor += line_end + 2;
        if size == 0 {
            return Ok(String::from_utf8_lossy(&decoded).to_string());
        }
        if body.len() < cursor + size + 2 {
            return Err(ChatModelError::InvalidProviderResponse {
                provider: "ollama".to_string(),
                message: "chunked response ended before chunk payload".to_string(),
            });
        }
        decoded.extend_from_slice(&body[cursor..cursor + size]);
        cursor += size + 2;
    }
}

fn find_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|window| window == b"\r\n")
}

fn provider_error_message(body: &str, status: u16) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value["error"].as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("Ollama returned HTTP {status}"))
}

fn openai_compatible_transport_error(provider: &str, error: std::io::Error) -> ChatModelError {
    ChatModelError::Transport {
        provider: provider.to_string(),
        message: error.to_string(),
        retryable: true,
    }
}

fn openai_compatible_provider_error_message(provider: &str, body: &str, status: u16) -> String {
    let fallback = format!("{provider} returned HTTP {status}");
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            openai_compatible_json_error(&value)
                .map(|error| format_openai_compatible_error(provider, error, Some(status)))
        })
        .unwrap_or(fallback)
}

fn openai_compatible_json_error(value: &Value) -> Option<&Value> {
    value.get("error").filter(|error| !error.is_null())
}

fn openai_compatible_error_retryable(provider: &str, error: &Value, status: Option<u16>) -> bool {
    if status.is_some_and(is_retryable_provider_status) {
        return true;
    }
    if let Some(code) = openai_compatible_error_code(error) {
        if let Ok(status) = code.parse::<u16>() {
            return is_retryable_provider_status(status);
        }
        let normalized = code.to_ascii_lowercase();
        return normalized.contains("rate_limit")
            || normalized.contains("timeout")
            || normalized.contains("server_error")
            || normalized.contains("provider_error")
            || normalized.contains("temporar")
            || normalized.contains("overload")
            || normalized.contains("unavailable");
    }
    if provider == "openrouter" {
        return openai_compatible_error_message(error)
            .map(|message| {
                message
                    .to_ascii_lowercase()
                    .contains("provider returned error")
            })
            .unwrap_or(false);
    }
    false
}

fn is_retryable_provider_status(status: u16) -> bool {
    matches!(status, 408 | 429) || status >= 500
}

fn format_openai_compatible_error(provider: &str, error: &Value, status: Option<u16>) -> String {
    let base = openai_compatible_error_message(error)
        .unwrap_or_else(|| format!("{provider} returned an error"));
    let mut details = Vec::new();
    if let Some(status) = status {
        details.push(format!("HTTP {status}"));
    }
    if let Some(code) = openai_compatible_error_code(error) {
        if status.map(|status| status.to_string()) != Some(code.clone()) {
            details.push(format!("code {code}"));
        }
    }
    if let Some(provider_name) = error["metadata"]["provider_name"].as_str() {
        details.push(format!("upstream provider: {provider_name}"));
    }
    if let Some(raw) = openai_compatible_raw_error_message(error) {
        details.push(format!("upstream detail: {raw}"));
    }

    let message = if details.is_empty() {
        base
    } else {
        format!("{base} ({})", details.join("; "))
    };
    redact_provider_error_text(&truncate_provider_error_detail(&message, 720))
}

fn openai_compatible_error_message(error: &Value) -> Option<String> {
    error["message"]
        .as_str()
        .or_else(|| error.as_str())
        .map(ToOwned::to_owned)
}

fn openai_compatible_error_code(error: &Value) -> Option<String> {
    error["code"]
        .as_i64()
        .map(|code| code.to_string())
        .or_else(|| error["status"].as_i64().map(|code| code.to_string()))
        .or_else(|| error["code"].as_str().map(ToOwned::to_owned))
        .or_else(|| error["status"].as_str().map(ToOwned::to_owned))
}

fn openai_compatible_raw_error_message(error: &Value) -> Option<String> {
    let raw = &error["metadata"]["raw"];
    if raw.is_null() {
        return None;
    }
    raw["error"]["message"]
        .as_str()
        .or_else(|| raw["message"].as_str())
        .or_else(|| raw["error"].as_str())
        .map(ToOwned::to_owned)
        .or_else(|| raw.as_str().map(ToOwned::to_owned))
        .or_else(|| serde_json::to_string(raw).ok())
        .map(|text| truncate_provider_error_detail(&text, 360))
}

fn truncate_provider_error_detail(text: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for character in text.chars().take(max_chars) {
        output.push(character);
    }
    if text.chars().count() > max_chars {
        output.push_str("...");
    }
    output
}

fn redact_provider_error_text(text: &str) -> String {
    let markers = ["Bearer ", "sk-or-", "nvapi-", "ya29.", "sk-"];
    let mut output = String::with_capacity(text.len());
    let mut index = 0;

    while index < text.len() {
        if let Some(marker) = markers
            .iter()
            .find(|candidate| text[index..].starts_with(**candidate))
        {
            output.push_str(marker);
            output.push_str("[REDACTED]");
            let token_start = index + marker.len();
            index = text[token_start..]
                .char_indices()
                .find_map(|(offset, character)| {
                    (!is_provider_secret_token_character(character)).then_some(token_start + offset)
                })
                .unwrap_or(text.len());
            continue;
        }

        let character = text[index..]
            .chars()
            .next()
            .expect("index remains on a UTF-8 boundary");
        output.push(character);
        index += character.len_utf8();
    }

    output
}

fn is_provider_secret_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
}

fn gemini_api_transport_error(error: std::io::Error) -> ChatModelError {
    ChatModelError::Transport {
        provider: "vertex".to_string(),
        message: error.to_string(),
        retryable: true,
    }
}

fn gemini_api_provider_error_message(body: &str, status: u16) -> String {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value["error"]["message"]
                .as_str()
                .or_else(|| value["error"].as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("Gemini API returned HTTP {status}"));
    model_unavailable_hint(message, status)
}

fn vertex_gemini_transport_error(error: std::io::Error) -> ChatModelError {
    ChatModelError::Transport {
        provider: "vertex".to_string(),
        message: error.to_string(),
        retryable: true,
    }
}

fn vertex_gemini_provider_error_message(body: &str, status: u16) -> String {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value["error"]["message"]
                .as_str()
                .or_else(|| value["error"].as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("Vertex Gemini returned HTTP {status}"));
    model_unavailable_hint(message, status)
}

fn model_unavailable_hint(message: String, status: u16) -> String {
    if status == 404
        || message.contains("is not found")
        || message.contains("not supported for generateContent")
    {
        format!(
            "{message}. The selected model may be unavailable in this region, retired, or incompatible with the standard generateContent chat method. Reload models and choose a model that advertises generateContent or streamGenerateContent."
        )
    } else {
        message
    }
}

fn vertex_anthropic_transport_error(error: std::io::Error) -> ChatModelError {
    ChatModelError::Transport {
        provider: "vertex".to_string(),
        message: error.to_string(),
        retryable: true,
    }
}

fn vertex_anthropic_provider_error_message(body: &str, status: u16) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value["error"]["message"]
                .as_str()
                .or_else(|| value["error"].as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("Vertex Anthropic returned HTTP {status}"))
}

fn azure_openai_transport_error(error: std::io::Error) -> ChatModelError {
    ChatModelError::Transport {
        provider: "azure".to_string(),
        message: error.to_string(),
        retryable: true,
    }
}

fn azure_openai_provider_error_message(body: &str, status: u16) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value["error"]["message"]
                .as_str()
                .or_else(|| value["error"].as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("Azure OpenAI returned HTTP {status}"))
}

fn parse_ollama_chat_body(body: &str) -> ChatModelResult {
    let mut deltas = Vec::new();
    let mut tool_calls = Vec::new();
    let mut finish_reason = ChatFinishReason::Stop;

    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let value: Value = serde_json::from_str(line).map_err(|error| {
            ChatModelError::InvalidProviderResponse {
                provider: "ollama".to_string(),
                message: format!("invalid JSON line: {error}"),
            }
        })?;
        if let Some(error) = value["error"].as_str() {
            return Err(ChatModelError::ProviderError {
                provider: "ollama".to_string(),
                message: error.to_string(),
                retryable: false,
            });
        }
        if let Some(content) = value["message"]["content"].as_str() {
            if !content.is_empty() {
                deltas.push(content.to_string());
            }
        }
        tool_calls.extend(parse_ollama_tool_calls(&value["message"])?);
        if value["done"].as_bool() == Some(true) {
            finish_reason = match value["done_reason"].as_str() {
                Some("length") => ChatFinishReason::Length,
                _ => ChatFinishReason::Stop,
            };
        }
    }

    if !tool_calls.is_empty() {
        finish_reason = ChatFinishReason::ToolCalls;
    }

    if deltas.is_empty() && tool_calls.is_empty() {
        return Err(ChatModelError::InvalidProviderResponse {
            provider: "ollama".to_string(),
            message: "Ollama response did not include assistant content or tool calls".to_string(),
        });
    }

    Ok(ChatResponse {
        text: deltas.concat(),
        deltas,
        finish_reason,
        tool_calls,
    })
}

fn parse_ollama_tool_calls(message: &Value) -> Result<Vec<ChatToolCall>, ChatModelError> {
    let Some(calls) = message["tool_calls"].as_array() else {
        return Ok(Vec::new());
    };

    calls
        .iter()
        .map(|call| {
            let function = &call["function"];
            let name = function["name"].as_str().ok_or_else(|| {
                ChatModelError::InvalidProviderResponse {
                    provider: "ollama".to_string(),
                    message: "Ollama tool call is missing function.name".to_string(),
                }
            })?;
            Ok(ChatToolCall {
                id: call["id"].as_str().map(ToOwned::to_owned),
                name: name.to_string(),
                arguments: parse_tool_arguments(&function["arguments"])?,
            })
        })
        .collect()
}

fn parse_openai_compatible_chat_body(provider: &str, body: &str) -> ChatModelResult {
    let value: Value =
        serde_json::from_str(body).map_err(|error| ChatModelError::InvalidProviderResponse {
            provider: provider.to_string(),
            message: format!("invalid JSON response: {error}"),
        })?;
    if let Some(error) = openai_compatible_json_error(&value) {
        return Err(ChatModelError::ProviderError {
            provider: provider.to_string(),
            message: format_openai_compatible_error(provider, error, None),
            retryable: openai_compatible_error_retryable(provider, error, None),
        });
    }

    let choice = value["choices"]
        .as_array()
        .and_then(|choices| choices.first())
        .ok_or_else(|| ChatModelError::InvalidProviderResponse {
            provider: provider.to_string(),
            message: "response did not include choices".to_string(),
        })?;
    if let Some(error) = openai_compatible_json_error(choice) {
        return Err(ChatModelError::ProviderError {
            provider: provider.to_string(),
            message: format_openai_compatible_error(provider, error, None),
            retryable: openai_compatible_error_retryable(provider, error, None),
        });
    }
    if choice["finish_reason"].as_str() == Some("error") {
        return Err(ChatModelError::ProviderError {
            provider: provider.to_string(),
            message: format!("{provider} returned finish_reason=error without error details"),
            retryable: true,
        });
    }
    let message = &choice["message"];
    let text = openai_compatible_message_content(&message["content"]);
    let tool_calls = parse_openai_compatible_tool_calls(provider, message)?;
    if text.is_empty() && tool_calls.is_empty() {
        return Err(ChatModelError::InvalidProviderResponse {
            provider: provider.to_string(),
            message: "response did not include assistant content or tool calls".to_string(),
        });
    }

    let mut finish_reason = match choice["finish_reason"].as_str() {
        Some("length") => ChatFinishReason::Length,
        Some("tool_calls") => ChatFinishReason::ToolCalls,
        Some("content_filter") => ChatFinishReason::Error,
        _ => ChatFinishReason::Stop,
    };
    if !tool_calls.is_empty() {
        finish_reason = ChatFinishReason::ToolCalls;
    }

    let deltas = if text.is_empty() {
        Vec::new()
    } else {
        vec![text.clone()]
    };
    Ok(ChatResponse {
        text,
        deltas,
        finish_reason,
        tool_calls,
    })
}

fn openai_compatible_message_content(content: &Value) -> String {
    if content.is_null() {
        return String::new();
    }
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    if let Some(parts) = content.as_array() {
        return parts
            .iter()
            .filter_map(|part| {
                part["text"]
                    .as_str()
                    .or_else(|| part["content"].as_str())
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>()
            .concat();
    }
    String::new()
}

fn parse_openai_compatible_tool_calls(
    provider: &str,
    message: &Value,
) -> Result<Vec<ChatToolCall>, ChatModelError> {
    let Some(calls) = message["tool_calls"].as_array() else {
        return Ok(Vec::new());
    };

    calls
        .iter()
        .map(|call| {
            let function = &call["function"];
            let name = function["name"].as_str().ok_or_else(|| {
                ChatModelError::InvalidProviderResponse {
                    provider: provider.to_string(),
                    message: format!("{provider} tool call is missing function.name"),
                }
            })?;
            Ok(ChatToolCall {
                id: call["id"].as_str().map(ToOwned::to_owned),
                name: decode_provider_safe_tool_name(name),
                arguments: parse_tool_arguments_for_provider(provider, &function["arguments"])?,
            })
        })
        .collect()
}

fn parse_gemini_api_response(body: &str) -> ChatModelResult {
    let value: Value =
        serde_json::from_str(body).map_err(|error| ChatModelError::InvalidProviderResponse {
            provider: "vertex".to_string(),
            message: format!("invalid JSON response: {error}"),
        })?;
    if let Some(message) = value["error"]["message"]
        .as_str()
        .or_else(|| value["error"].as_str())
    {
        return Err(ChatModelError::ProviderError {
            provider: "vertex".to_string(),
            message: message.to_string(),
            retryable: false,
        });
    }

    let candidate = value["candidates"]
        .as_array()
        .and_then(|candidates| candidates.first())
        .ok_or_else(|| ChatModelError::InvalidProviderResponse {
            provider: "vertex".to_string(),
            message: "response did not include candidates".to_string(),
        })?;
    let parts = candidate["content"]["parts"].as_array().ok_or_else(|| {
        ChatModelError::InvalidProviderResponse {
            provider: "vertex".to_string(),
            message: "response candidate did not include content parts".to_string(),
        }
    })?;

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    for part in parts {
        if let Some(text) = part["text"].as_str() {
            if !text.is_empty() {
                text_parts.push(text.to_string());
            }
        }
        if let Some(function_call) = part.get("functionCall") {
            let name = function_call["name"].as_str().ok_or_else(|| {
                ChatModelError::InvalidProviderResponse {
                    provider: "vertex".to_string(),
                    message: "Gemini API function call is missing name".to_string(),
                }
            })?;
            tool_calls.push(ChatToolCall {
                id: None,
                name: decode_provider_safe_tool_name(name),
                arguments: parse_tool_arguments_for_provider("vertex", &function_call["args"])?,
            });
        }
    }

    if text_parts.is_empty() && tool_calls.is_empty() {
        return Err(ChatModelError::InvalidProviderResponse {
            provider: "vertex".to_string(),
            message: "response did not include assistant content or tool calls".to_string(),
        });
    }

    let mut finish_reason = match candidate["finishReason"].as_str() {
        Some("MAX_TOKENS") => ChatFinishReason::Length,
        Some("SAFETY") | Some("RECITATION") | Some("BLOCKLIST") | Some("PROHIBITED_CONTENT") => {
            ChatFinishReason::Error
        }
        _ => ChatFinishReason::Stop,
    };
    if !tool_calls.is_empty() {
        finish_reason = ChatFinishReason::ToolCalls;
    }
    let text = text_parts.concat();

    Ok(ChatResponse {
        text,
        deltas: text_parts,
        finish_reason,
        tool_calls,
    })
}

fn parse_vertex_anthropic_response(body: &str) -> ChatModelResult {
    let value: Value =
        serde_json::from_str(body).map_err(|error| ChatModelError::InvalidProviderResponse {
            provider: "vertex".to_string(),
            message: format!("invalid JSON response: {error}"),
        })?;
    if let Some(message) = value["error"]["message"]
        .as_str()
        .or_else(|| value["error"].as_str())
    {
        return Err(ChatModelError::ProviderError {
            provider: "vertex".to_string(),
            message: message.to_string(),
            retryable: false,
        });
    }

    let content =
        value["content"]
            .as_array()
            .ok_or_else(|| ChatModelError::InvalidProviderResponse {
                provider: "vertex".to_string(),
                message: "response did not include content".to_string(),
            })?;
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for item in content {
        match item["type"].as_str() {
            Some("text") => {
                if let Some(text) = item["text"].as_str() {
                    if !text.is_empty() {
                        text_parts.push(text.to_string());
                    }
                }
            }
            Some("tool_use") => {
                let name = item["name"].as_str().ok_or_else(|| {
                    ChatModelError::InvalidProviderResponse {
                        provider: "vertex".to_string(),
                        message: "Vertex Anthropic tool use is missing name".to_string(),
                    }
                })?;
                tool_calls.push(ChatToolCall {
                    id: item["id"].as_str().map(ToOwned::to_owned),
                    name: decode_provider_safe_tool_name(name),
                    arguments: item["input"].clone(),
                });
            }
            _ => {}
        }
    }

    if text_parts.is_empty() && tool_calls.is_empty() {
        return Err(ChatModelError::InvalidProviderResponse {
            provider: "vertex".to_string(),
            message: "response did not include assistant content or tool calls".to_string(),
        });
    }

    let mut finish_reason = match value["stop_reason"].as_str() {
        Some("max_tokens") => ChatFinishReason::Length,
        Some("tool_use") => ChatFinishReason::ToolCalls,
        _ => ChatFinishReason::Stop,
    };
    if !tool_calls.is_empty() {
        finish_reason = ChatFinishReason::ToolCalls;
    }
    let text = text_parts.concat();

    Ok(ChatResponse {
        text,
        deltas: text_parts,
        finish_reason,
        tool_calls,
    })
}

fn parse_tool_arguments(arguments: &Value) -> Result<Value, ChatModelError> {
    parse_tool_arguments_for_provider("ollama", arguments)
}

fn parse_tool_arguments_for_provider(
    provider: &str,
    arguments: &Value,
) -> Result<Value, ChatModelError> {
    if arguments.is_null() {
        return Ok(serde_json::json!({}));
    }

    if let Some(text) = arguments.as_str() {
        return serde_json::from_str(text).map_err(|error| {
            ChatModelError::InvalidProviderResponse {
                provider: provider.to_string(),
                message: format!("{provider} tool call arguments are invalid JSON: {error}"),
            }
        });
    }

    Ok(arguments.clone())
}

#[cfg(test)]
mod tests {
    use coddy_core::{
        ApprovalPolicy, ToolCategory, ToolName, ToolPermission, ToolRiskLevel, ToolSchema,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn retry_policy_retries_recoverable_provider_and_empty_responses_but_not_timeouts() {
        let empty_response_error = ChatModelError::InvalidProviderResponse {
            provider: "openrouter".to_string(),
            message: "response did not include assistant content or tool calls".to_string(),
        };
        let invalid_json_error = ChatModelError::InvalidProviderResponse {
            provider: "openrouter".to_string(),
            message: "failed to parse chat response JSON: expected value".to_string(),
        };

        assert!(should_retry_chat_model_request_error(
            &ChatModelError::ProviderError {
                provider: "openrouter".to_string(),
                message: "Provider returned error".to_string(),
                retryable: true,
            },
        ));
        assert!(should_retry_chat_model_request_error(&empty_response_error));
        assert!(empty_response_error.retryable());
        assert!(!should_retry_chat_model_request_error(&invalid_json_error));
        assert!(!invalid_json_error.retryable());
        assert!(!should_retry_chat_model_request_error(
            &ChatModelError::Transport {
                provider: "openrouter".to_string(),
                message: "request timed out".to_string(),
                retryable: true,
            },
        ));
        assert!(!should_retry_chat_model_request_error(
            &ChatModelError::ProviderError {
                provider: "openrouter".to_string(),
                message: "invalid api key".to_string(),
                retryable: false,
            },
        ));
    }

    #[derive(Debug)]
    struct StaticOllamaTransport {
        body: Value,
        response: ChatModelResult,
    }

    impl OllamaTransport for StaticOllamaTransport {
        fn chat(&self, body: &Value) -> ChatModelResult {
            assert_eq!(body, &self.body);
            self.response.clone()
        }
    }

    #[derive(Debug)]
    struct StaticOpenAiCompatibleTransport {
        provider: String,
        endpoint: String,
        token: String,
        body: Value,
        response: ChatModelResult,
    }

    impl OpenAiCompatibleTransport for StaticOpenAiCompatibleTransport {
        fn chat(
            &self,
            provider: &str,
            endpoint: &str,
            credential: &ModelCredential,
            body: &Value,
        ) -> ChatModelResult {
            assert_eq!(provider, self.provider);
            assert_eq!(endpoint, self.endpoint);
            assert_eq!(credential.token, self.token);
            assert_eq!(body, &self.body);
            assert!(!body.to_string().contains(&self.token));
            self.response.clone()
        }
    }

    #[derive(Debug)]
    struct StaticGeminiApiTransport {
        endpoint: String,
        token: String,
        body: Value,
        response: ChatModelResult,
    }

    impl GeminiApiTransport for StaticGeminiApiTransport {
        fn generate_content(
            &self,
            endpoint: &str,
            credential: &ModelCredential,
            body: &Value,
        ) -> ChatModelResult {
            assert_eq!(endpoint, self.endpoint);
            assert_eq!(credential.token, self.token);
            assert_eq!(body, &self.body);
            assert!(!body.to_string().contains(&self.token));
            assert!(!endpoint.contains(&self.token));
            self.response.clone()
        }
    }

    #[derive(Debug)]
    struct StaticVertexGeminiTransport {
        endpoint: String,
        token: String,
        body: Value,
        response: ChatModelResult,
    }

    impl VertexGeminiTransport for StaticVertexGeminiTransport {
        fn generate_content(
            &self,
            endpoint: &str,
            credential: &ModelCredential,
            body: &Value,
        ) -> ChatModelResult {
            assert_eq!(endpoint, self.endpoint);
            assert_eq!(credential.token, self.token);
            assert_eq!(body, &self.body);
            assert!(!body.to_string().contains(&self.token));
            assert!(!endpoint.contains(&self.token));
            self.response.clone()
        }
    }

    #[derive(Debug)]
    struct StaticVertexAnthropicTransport {
        endpoint: String,
        token: String,
        body: Value,
        response: ChatModelResult,
    }

    impl VertexAnthropicTransport for StaticVertexAnthropicTransport {
        fn raw_predict(
            &self,
            endpoint: &str,
            credential: &ModelCredential,
            body: &Value,
        ) -> ChatModelResult {
            assert_eq!(endpoint, self.endpoint);
            assert_eq!(credential.token, self.token);
            assert_eq!(body, &self.body);
            assert!(!body.to_string().contains(&self.token));
            self.response.clone()
        }
    }

    #[derive(Debug)]
    struct StaticAzureOpenAiTransport {
        endpoint: String,
        token: String,
        body: Value,
        response: ChatModelResult,
    }

    impl AzureOpenAiTransport for StaticAzureOpenAiTransport {
        fn chat(
            &self,
            endpoint: &str,
            credential: &ModelCredential,
            body: &Value,
        ) -> ChatModelResult {
            assert_eq!(endpoint, self.endpoint);
            assert_eq!(credential.token, self.token);
            assert_eq!(body, &self.body);
            assert!(!body.to_string().contains(&self.token));
            assert!(!endpoint.contains(&self.token));
            self.response.clone()
        }
    }

    #[test]
    fn chat_response_builds_streaming_text_from_deltas() {
        let response = ChatResponse::from_deltas(vec!["hello".to_string(), " world".to_string()])
            .expect("response");

        assert_eq!(response.text, "hello world");
        assert_eq!(response.deltas, vec!["hello", " world"]);
        assert_eq!(response.finish_reason, ChatFinishReason::Stop);
    }

    #[test]
    fn chat_request_requires_at_least_one_message() {
        let error = ChatRequest::new(
            ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
            Vec::new(),
        )
        .expect_err("empty request rejected");

        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn chat_request_rejects_mismatched_model_credential_provider() {
        let request = ChatRequest::new(
            ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request");

        let error = request
            .with_model_credential(Some(ModelCredential {
                provider: "vertex".to_string(),
                token: "secret-token".to_string(),
                endpoint: None,
                metadata: Default::default(),
            }))
            .expect_err("mismatched credential rejected");

        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn tool_spec_projects_public_tool_metadata_for_model_context() {
        let definition = ToolDefinition::new(
            ToolName::new("filesystem.read_file").expect("tool name"),
            "Read a file",
            ToolCategory::Filesystem,
            ToolSchema::new(json!({ "type": "object" })),
            ToolSchema::empty_object(),
            ToolRiskLevel::Low,
            vec![ToolPermission::ReadWorkspace],
            5_000,
            ApprovalPolicy::AutoApprove,
        )
        .expect("definition");

        let spec = ChatToolSpec::from_tool_definition(&definition);

        assert_eq!(spec.name, "filesystem.read_file");
        assert_eq!(spec.description, "Read a file");
        assert_eq!(spec.input_schema, json!({ "type": "object" }));
        assert_eq!(spec.risk_level, ToolRiskLevel::Low);
        assert_eq!(spec.approval_policy, ApprovalPolicy::AutoApprove);
    }

    #[test]
    fn unavailable_client_reports_unselected_or_unwired_provider() {
        let client = UnavailableChatModelClient;
        let unselected = ChatRequest::new(
            ModelRef {
                provider: "coddy".to_string(),
                name: "unselected".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request");

        assert_eq!(
            client.complete(unselected),
            Err(ChatModelError::UnselectedModel)
        );

        let selected = ChatRequest::new(
            ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request");

        assert_eq!(
            client.complete(selected),
            Err(ChatModelError::ProviderUnavailable {
                provider: "openai".to_string()
            })
        );
    }

    #[test]
    fn default_client_routes_ollama_requests_to_ollama_adapter() {
        let request = ChatRequest::new(
            ModelRef {
                provider: "ollama".to_string(),
                name: "qwen2.5:0.5b".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request");
        let expected_body = serde_json::json!({
            "model": "qwen2.5:0.5b",
            "messages": [
                { "role": "user", "content": "hello" }
            ],
            "stream": true
        });
        let client = OllamaChatModelClient::with_transport(Arc::new(StaticOllamaTransport {
            body: expected_body,
            response: Ok(ChatResponse::from_text("hi")),
        }));

        assert_eq!(client.complete(request), Ok(ChatResponse::from_text("hi")));
    }

    #[test]
    fn default_client_routes_openai_requests_to_openai_compatible_adapter() {
        let expected_body = serde_json::json!({
            "model": "gpt-4.1",
            "messages": [
                { "role": "user", "content": "hello" }
            ],
            "stream": false
        });
        let client = DefaultChatModelClient {
            ollama: OllamaChatModelClient::default(),
            openai: OpenAiCompatibleChatModelClient::with_transport(
                "openai",
                "https://api.openai.com/v1",
                Arc::new(StaticOpenAiCompatibleTransport {
                    provider: "openai".to_string(),
                    endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
                    token: "secret-openai-token".to_string(),
                    body: expected_body,
                    response: Ok(ChatResponse::from_text("hi from openai")),
                }),
            ),
            openrouter: OpenAiCompatibleChatModelClient::openrouter(),
            nvidia: OpenAiCompatibleChatModelClient::nvidia(),
            gemini_api: GeminiApiChatModelClient::default(),
            vertex_gemini: VertexGeminiChatModelClient::default(),
            vertex_anthropic: VertexAnthropicChatModelClient::default(),
            azure_openai: AzureOpenAiChatModelClient::default(),
            unavailable: UnavailableChatModelClient,
        };
        let request = ChatRequest::new(
            ModelRef {
                provider: "openai".to_string(),
                name: "gpt-4.1".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "openai".to_string(),
            token: "secret-openai-token".to_string(),
            endpoint: None,
            metadata: Default::default(),
        }))
        .expect("credential");

        assert_eq!(
            client.complete(request),
            Ok(ChatResponse::from_text("hi from openai"))
        );
    }

    #[test]
    fn default_client_routes_openrouter_requests_to_openai_compatible_adapter() {
        let expected_body = serde_json::json!({
            "model": "anthropic/claude-sonnet-4.5",
            "messages": [
                { "role": "user", "content": "hello" }
            ],
            "stream": false
        });
        let client = DefaultChatModelClient {
            ollama: OllamaChatModelClient::default(),
            openai: OpenAiCompatibleChatModelClient::openai(),
            openrouter: OpenAiCompatibleChatModelClient::with_transport(
                "openrouter",
                "https://openrouter.ai/api/v1",
                Arc::new(StaticOpenAiCompatibleTransport {
                    provider: "openrouter".to_string(),
                    endpoint: "https://openrouter.ai/api/v1/chat/completions".to_string(),
                    token: "sk-or-secret-token".to_string(),
                    body: expected_body,
                    response: Ok(ChatResponse::from_text("hi from openrouter")),
                }),
            ),
            nvidia: OpenAiCompatibleChatModelClient::nvidia(),
            gemini_api: GeminiApiChatModelClient::default(),
            vertex_gemini: VertexGeminiChatModelClient::default(),
            vertex_anthropic: VertexAnthropicChatModelClient::default(),
            azure_openai: AzureOpenAiChatModelClient::default(),
            unavailable: UnavailableChatModelClient,
        };
        let request = ChatRequest::new(
            ModelRef {
                provider: "openrouter".to_string(),
                name: "anthropic/claude-sonnet-4.5".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "openrouter".to_string(),
            token: "sk-or-secret-token".to_string(),
            endpoint: None,
            metadata: Default::default(),
        }))
        .expect("credential");

        assert_eq!(
            client.complete(request),
            Ok(ChatResponse::from_text("hi from openrouter"))
        );
    }

    #[test]
    fn default_client_routes_nvidia_requests_to_openai_compatible_adapter() {
        let expected_body = serde_json::json!({
            "model": "deepseek-ai/deepseek-v4-pro",
            "messages": [
                { "role": "user", "content": "hello" }
            ],
            "stream": false
        });
        let client = DefaultChatModelClient {
            ollama: OllamaChatModelClient::default(),
            openai: OpenAiCompatibleChatModelClient::openai(),
            openrouter: OpenAiCompatibleChatModelClient::openrouter(),
            nvidia: OpenAiCompatibleChatModelClient::with_transport(
                "nvidia",
                "https://integrate.api.nvidia.com/v1",
                Arc::new(StaticOpenAiCompatibleTransport {
                    provider: "nvidia".to_string(),
                    endpoint: "https://integrate.api.nvidia.com/v1/chat/completions".to_string(),
                    token: "nvapi-secret-token".to_string(),
                    body: expected_body,
                    response: Ok(ChatResponse::from_text("hi from nvidia")),
                }),
            ),
            gemini_api: GeminiApiChatModelClient::default(),
            vertex_gemini: VertexGeminiChatModelClient::default(),
            vertex_anthropic: VertexAnthropicChatModelClient::default(),
            azure_openai: AzureOpenAiChatModelClient::default(),
            unavailable: UnavailableChatModelClient,
        };
        let request = ChatRequest::new(
            ModelRef {
                provider: "nvidia".to_string(),
                name: "deepseek-ai/deepseek-v4-pro".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "nvidia".to_string(),
            token: "nvapi-secret-token".to_string(),
            endpoint: None,
            metadata: Default::default(),
        }))
        .expect("credential");

        assert_eq!(
            client.complete(request),
            Ok(ChatResponse::from_text("hi from nvidia"))
        );
    }

    #[test]
    fn nvidia_openai_compatible_timeout_is_extended_for_large_models() {
        let transport =
            HttpOpenAiCompatibleTransport::with_timeout(NVIDIA_OPENAI_COMPATIBLE_TIMEOUT);

        assert!(NVIDIA_OPENAI_COMPATIBLE_TIMEOUT > DEFAULT_OPENAI_COMPATIBLE_TIMEOUT);
        assert_eq!(transport.timeout, Duration::from_secs(300));
    }

    #[test]
    fn openrouter_openai_compatible_timeout_is_extended_for_agentic_followups() {
        let transport =
            HttpOpenAiCompatibleTransport::with_timeout(OPENROUTER_OPENAI_COMPATIBLE_TIMEOUT);

        assert!(OPENROUTER_OPENAI_COMPATIBLE_TIMEOUT > DEFAULT_OPENAI_COMPATIBLE_TIMEOUT);
        assert_eq!(transport.timeout, Duration::from_secs(300));
    }

    #[test]
    fn openai_compatible_client_requires_provider_credential() {
        let client = OpenAiCompatibleChatModelClient::openai();
        let request = ChatRequest::new(
            ModelRef {
                provider: "openai".to_string(),
                name: "gpt-4.1".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request");

        let error = client.complete(request).expect_err("credential required");

        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn openai_compatible_request_projects_tools_options_and_observations() {
        let request = ChatRequest {
            model: ModelRef {
                provider: "openrouter".to_string(),
                name: "anthropic/claude-3.5-sonnet".to_string(),
            },
            messages: vec![
                ChatMessage::system("system"),
                ChatMessage::user("user"),
                ChatMessage::assistant("assistant"),
                ChatMessage::tool("filesystem result"),
            ],
            tools: vec![ChatToolSpec {
                name: "filesystem.list_files".to_string(),
                description: "List files".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                approval_policy: ApprovalPolicy::AutoApprove,
            }],
            model_credential: None,
            temperature: Some(0.2),
            max_output_tokens: Some(128),
        };

        let body = openai_compatible_chat_body(&request);

        assert_eq!(body["model"], "anthropic/claude-3.5-sonnet");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][3]["role"], "user");
        assert_eq!(
            body["messages"][3]["content"],
            "Tool observations:\nfilesystem result"
        );
        assert_eq!(
            body["tools"][0]["function"]["name"],
            "coddy_tool__filesystem__dot__list_files"
        );
        assert_eq!(
            body["tools"][0]["function"]["description"],
            "Coddy tool `filesystem.list_files`. List files"
        );
        assert_eq!(body["tool_choice"], "auto");
        let temperature = body["temperature"].as_f64().expect("temperature");
        assert!((temperature - 0.2).abs() < 0.000_001);
        assert_eq!(body["max_tokens"], 128);
    }

    #[test]
    fn provider_safe_tool_name_decoder_accepts_prefixed_and_legacy_aliases() {
        assert_eq!(
            decode_provider_safe_tool_name("coddy_tool__filesystem__dot__read_file"),
            "filesystem.read_file"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem__dot__read_file"),
            "filesystem.read_file"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem.read_file"),
            "filesystem.read_file"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem::read_file"),
            "filesystem.read_file"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem.dot.search_files"),
            "filesystem.search_files"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem.dot_read_file"),
            "filesystem.read_file"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem_read_file"),
            "filesystem.read_file"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem._list_files"),
            "filesystem.list_files"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem.listFiles"),
            "filesystem.list_files"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem.readFile"),
            "filesystem.read_file"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem.searchFiles"),
            "filesystem.search_files"
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem.applyEdit"),
            "filesystem.apply_edit"
        );
        assert_eq!(
            decode_provider_safe_tool_name("subagent.teamPlan"),
            "subagent.team_plan"
        );
    }

    #[test]
    fn openai_compatible_endpoint_normalization_requires_https() {
        assert_eq!(
            openai_compatible_chat_url("https://api.openai.com/v1", None)
                .expect("default endpoint"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            openai_compatible_chat_url(
                "https://api.openai.com/v1",
                Some("https://example.test/v1/")
            )
            .expect("override endpoint"),
            "https://example.test/v1/chat/completions"
        );

        let error = openai_compatible_chat_url("https://api.openai.com/v1", Some("http://bad"))
            .expect_err("http rejected");

        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn default_client_routes_azure_requests_to_azure_openai_adapter() {
        let expected_body = serde_json::json!({
            "messages": [
                { "role": "user", "content": "hello" }
            ],
            "stream": false
        });
        let client = DefaultChatModelClient {
            ollama: OllamaChatModelClient::default(),
            openai: OpenAiCompatibleChatModelClient::openai(),
            openrouter: OpenAiCompatibleChatModelClient::openrouter(),
            nvidia: OpenAiCompatibleChatModelClient::nvidia(),
            gemini_api: GeminiApiChatModelClient::default(),
            vertex_gemini: VertexGeminiChatModelClient::default(),
            vertex_anthropic: VertexAnthropicChatModelClient::default(),
            azure_openai: AzureOpenAiChatModelClient::with_transport(Arc::new(
                StaticAzureOpenAiTransport {
                    endpoint: "https://coddy-resource.openai.azure.com/openai/deployments/gpt-4.1-coddy/chat/completions?api-version=2024-10-21".to_string(),
                    token: "azure-secret-key".to_string(),
                    body: expected_body,
                    response: Ok(ChatResponse::from_text("hi from azure")),
                },
            )),
            unavailable: UnavailableChatModelClient,
        };
        let request = ChatRequest::new(
            ModelRef {
                provider: "azure".to_string(),
                name: "gpt-4.1-coddy".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "azure".to_string(),
            token: "azure-secret-key".to_string(),
            endpoint: Some("https://coddy-resource.openai.azure.com".to_string()),
            metadata: Default::default(),
        }))
        .expect("credential");

        assert_eq!(
            client.complete(request),
            Ok(ChatResponse::from_text("hi from azure"))
        );
    }

    #[test]
    fn azure_openai_requires_endpoint() {
        let client = AzureOpenAiChatModelClient::default();
        let request = ChatRequest::new(
            ModelRef {
                provider: "azure".to_string(),
                name: "gpt-4.1-coddy".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "azure".to_string(),
            token: "azure-secret-key".to_string(),
            endpoint: None,
            metadata: Default::default(),
        }))
        .expect("credential");

        let error = client.complete(request).expect_err("endpoint required");

        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn azure_openai_request_projects_tools_and_options_without_model_body_field() {
        let request = ChatRequest {
            model: ModelRef {
                provider: "azure".to_string(),
                name: "gpt-4.1-coddy".to_string(),
            },
            messages: vec![
                ChatMessage::system("system"),
                ChatMessage::user("user"),
                ChatMessage::assistant("assistant"),
                ChatMessage::tool("filesystem result"),
            ],
            tools: vec![ChatToolSpec {
                name: "filesystem.list_files".to_string(),
                description: "List files".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                approval_policy: ApprovalPolicy::AutoApprove,
            }],
            model_credential: None,
            temperature: Some(0.2),
            max_output_tokens: Some(128),
        };

        let body = azure_openai_chat_body(&request);

        assert!(body["model"].is_null());
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][3]["role"], "user");
        assert_eq!(
            body["messages"][3]["content"],
            "Tool observations:\nfilesystem result"
        );
        assert_eq!(
            body["tools"][0]["function"]["name"],
            "coddy_tool__filesystem__dot__list_files"
        );
        assert_eq!(body["tool_choice"], "auto");
        let temperature = body["temperature"].as_f64().expect("temperature");
        assert!((temperature - 0.2).abs() < 0.000_001);
        assert_eq!(body["max_tokens"], 128);
    }

    #[test]
    fn azure_openai_endpoint_uses_deployment_and_api_version() {
        let credential = ModelCredential {
            provider: "azure".to_string(),
            token: "azure-secret-key".to_string(),
            endpoint: Some("https://coddy-resource.openai.azure.com/".to_string()),
            metadata: [("api_version".to_string(), "2025-01-01-preview".to_string())]
                .into_iter()
                .collect(),
        };

        assert_eq!(
            azure_openai_chat_url("gpt-4.1-coddy", &credential).expect("url"),
            "https://coddy-resource.openai.azure.com/openai/deployments/gpt-4.1-coddy/chat/completions?api-version=2025-01-01-preview"
        );

        let error = azure_openai_chat_url("bad/deployment", &credential)
            .expect_err("deployment slash rejected");

        assert_eq!(error.code(), "invalid_request");

        let error = azure_openai_chat_url("bad?deployment", &credential)
            .expect_err("deployment query rejected");

        assert_eq!(error.code(), "invalid_request");

        let bad_api_version_credential = ModelCredential {
            metadata: [(
                "api_version".to_string(),
                "2025-01-01-preview#beta".to_string(),
            )]
            .into_iter()
            .collect(),
            ..credential
        };
        let error = azure_openai_chat_url("gpt-4.1-coddy", &bad_api_version_credential)
            .expect_err("api version fragment rejected");

        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn default_client_routes_vertex_gemini_requests_to_gemini_api_adapter() {
        let expected_body = serde_json::json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        { "text": "hello" }
                    ]
                }
            ],
            "systemInstruction": {
                "parts": [
                    { "text": "system" }
                ]
            }
        });
        let client = DefaultChatModelClient {
            ollama: OllamaChatModelClient::default(),
            openai: OpenAiCompatibleChatModelClient::openai(),
            openrouter: OpenAiCompatibleChatModelClient::openrouter(),
            nvidia: OpenAiCompatibleChatModelClient::nvidia(),
            gemini_api: GeminiApiChatModelClient::with_transport(
                "https://generativelanguage.googleapis.com/v1beta",
                Arc::new(StaticGeminiApiTransport {
                    endpoint: "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent".to_string(),
                    token: "AIza-gemini-key".to_string(),
                    body: expected_body,
                    response: Ok(ChatResponse::from_text("hi from gemini")),
                }),
            ),
            vertex_gemini: VertexGeminiChatModelClient::default(),
            vertex_anthropic: VertexAnthropicChatModelClient::default(),
            azure_openai: AzureOpenAiChatModelClient::default(),
            unavailable: UnavailableChatModelClient,
        };
        let request = ChatRequest::new(
            ModelRef {
                provider: "vertex".to_string(),
                name: "gemini-2.5-flash".to_string(),
            },
            vec![ChatMessage::system("system"), ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "vertex".to_string(),
            token: "AIza-gemini-key".to_string(),
            endpoint: None,
            metadata: Default::default(),
        }))
        .expect("credential");

        assert_eq!(
            client.complete(request),
            Ok(ChatResponse::from_text("hi from gemini"))
        );
    }

    #[test]
    fn default_client_routes_vertex_gemini_oauth_requests_to_vertex_ai_adapter() {
        let expected_body = serde_json::json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        { "text": "hello" }
                    ]
                }
            ],
            "systemInstruction": {
                "parts": [
                    { "text": "system" }
                ]
            }
        });
        let client = DefaultChatModelClient {
            ollama: OllamaChatModelClient::default(),
            openai: OpenAiCompatibleChatModelClient::openai(),
            openrouter: OpenAiCompatibleChatModelClient::openrouter(),
            nvidia: OpenAiCompatibleChatModelClient::nvidia(),
            gemini_api: GeminiApiChatModelClient::default(),
            vertex_gemini: VertexGeminiChatModelClient::with_transport(Arc::new(
                StaticVertexGeminiTransport {
                    endpoint: "https://us-central1-aiplatform.googleapis.com/v1/projects/coddy-dev/locations/us-central1/publishers/google/models/gemini-2.5-flash:generateContent".to_string(),
                    token: "ya29.vertex-token".to_string(),
                    body: expected_body,
                    response: Ok(ChatResponse::from_text("hi from vertex gemini")),
                },
            )),
            vertex_anthropic: VertexAnthropicChatModelClient::default(),
            azure_openai: AzureOpenAiChatModelClient::default(),
            unavailable: UnavailableChatModelClient,
        };
        let request = ChatRequest::new(
            ModelRef {
                provider: "vertex".to_string(),
                name: "gemini-2.5-flash".to_string(),
            },
            vec![ChatMessage::system("system"), ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "vertex".to_string(),
            token: "ya29.vertex-token".to_string(),
            endpoint: Some("us-central1".to_string()),
            metadata: [("project_id".to_string(), "coddy-dev".to_string())]
                .into_iter()
                .collect(),
        }))
        .expect("credential");

        assert_eq!(
            client.complete(request),
            Ok(ChatResponse::from_text("hi from vertex gemini"))
        );
    }

    #[test]
    fn gemini_live_api_model_returns_friendly_chat_runtime_error() {
        let client = GeminiApiChatModelClient::with_transport(
            "https://generativelanguage.googleapis.com/v1beta",
            Arc::new(StaticGeminiApiTransport {
                endpoint: "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-live-preview:generateContent".to_string(),
                token: "AIza-gemini-key".to_string(),
                body: serde_json::json!({}),
                response: Ok(ChatResponse::from_text("unexpected")),
            }),
        );
        let request = ChatRequest::new(
            ModelRef {
                provider: "vertex".to_string(),
                name: "gemini-3.1-flash-live-preview".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "vertex".to_string(),
            token: "AIza-gemini-key".to_string(),
            endpoint: None,
            metadata: Default::default(),
        }))
        .expect("credential");

        let error = client.complete(request).expect_err("live model is gated");

        assert_eq!(error.code(), "invalid_request");
        assert!(error
            .to_string()
            .contains("does not support the standard text chat runtime"));
    }

    #[test]
    fn gemini_provider_404_message_adds_reload_and_method_hint() {
        let message = gemini_api_provider_error_message(
            r#"{"error":{"message":"models/gemini-live is not found for API version v1beta, or is not supported for generateContent"}}"#,
            404,
        );

        assert!(message.contains("Reload models"));
        assert!(message.contains("generateContent"));
        assert!(message.contains("streamGenerateContent"));
    }

    #[test]
    fn gemini_api_rejects_oauth_credentials() {
        let client = GeminiApiChatModelClient::default();
        let request = ChatRequest::new(
            ModelRef {
                provider: "vertex".to_string(),
                name: "gemini-2.5-flash".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "vertex".to_string(),
            token: "ya29.oauth-token".to_string(),
            endpoint: None,
            metadata: Default::default(),
        }))
        .expect("credential");

        let error = client.complete(request).expect_err("api key required");

        assert_eq!(error.code(), "invalid_request");
        assert!(error.to_string().contains("Vertex AI runtime route"));
    }

    #[test]
    fn gemini_api_request_projects_tools_options_and_observations() {
        let request = ChatRequest {
            model: ModelRef {
                provider: "vertex".to_string(),
                name: "gemini-2.5-pro".to_string(),
            },
            messages: vec![
                ChatMessage::system("system one"),
                ChatMessage::system("system two"),
                ChatMessage::user("user"),
                ChatMessage::assistant("assistant"),
                ChatMessage::tool("filesystem result"),
            ],
            tools: vec![ChatToolSpec {
                name: "filesystem.list_files".to_string(),
                description: "List files".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                approval_policy: ApprovalPolicy::AutoApprove,
            }],
            model_credential: None,
            temperature: Some(0.2),
            max_output_tokens: Some(128),
        };

        let body = gemini_api_chat_body(&request).expect("body");

        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "system one\n\nsystem two"
        );
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][1]["role"], "model");
        assert_eq!(body["contents"][2]["role"], "user");
        assert_eq!(
            body["contents"][2]["parts"][0]["text"],
            "Tool observations:\nfilesystem result"
        );
        assert_eq!(
            body["tools"][0]["functionDeclarations"][0]["name"],
            "coddy_tool__filesystem__dot__list_files"
        );
        assert_eq!(
            body["tools"][0]["functionDeclarations"][0]["description"],
            "Coddy tool `filesystem.list_files`. List files"
        );
        let temperature = body["generationConfig"]["temperature"]
            .as_f64()
            .expect("temperature");
        assert!((temperature - 0.2).abs() < 0.000_001);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 128);
    }

    #[test]
    fn gemini_api_removes_unsupported_tool_schema_keywords() {
        let request = ChatRequest {
            model: ModelRef {
                provider: "vertex".to_string(),
                name: "gemini-2.5-flash".to_string(),
            },
            messages: vec![ChatMessage::user("inspect")],
            tools: vec![ChatToolSpec {
                name: "filesystem.search_files".to_string(),
                description: "Search files".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "query": { "type": "string" },
                        "config": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "path": { "type": "string" }
                            }
                        },
                        "additionalProperties": { "type": "string" }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                approval_policy: ApprovalPolicy::AutoApprove,
            }],
            model_credential: None,
            temperature: None,
            max_output_tokens: None,
        };

        let body = gemini_api_chat_body(&request).expect("body");
        let parameters = &body["tools"][0]["functionDeclarations"][0]["parameters"];

        assert!(parameters.get("additionalProperties").is_none());
        assert!(parameters["properties"]["config"]
            .get("additionalProperties")
            .is_none());
        assert!(parameters["properties"]
            .get("additionalProperties")
            .is_some());
    }

    #[test]
    fn gemini_api_endpoint_uses_generate_content_without_key_in_url() {
        assert_eq!(
            gemini_api_generate_content_url(
                "https://generativelanguage.googleapis.com/v1beta",
                None,
                "models/gemini-2.5-flash",
            )
            .expect("url"),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );
        assert_eq!(
            gemini_api_generate_content_url(
                "https://generativelanguage.googleapis.com/v1beta",
                Some("us-east5"),
                "gemini-2.5-flash",
            )
            .expect("region ignored for Gemini API"),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );

        let error = gemini_api_generate_content_url(
            "https://generativelanguage.googleapis.com/v1beta",
            Some("http://bad"),
            "gemini-2.5-flash",
        )
        .expect_err("http endpoint rejected");

        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn default_client_routes_vertex_claude_requests_to_vertex_anthropic_adapter() {
        let expected_body = serde_json::json!({
            "anthropic_version": "vertex-2023-10-16",
            "max_tokens": 1024,
            "stream": false,
            "system": "system",
            "messages": [
                { "role": "user", "content": "hello" }
            ]
        });
        let client = DefaultChatModelClient {
            ollama: OllamaChatModelClient::default(),
            openai: OpenAiCompatibleChatModelClient::openai(),
            openrouter: OpenAiCompatibleChatModelClient::openrouter(),
            nvidia: OpenAiCompatibleChatModelClient::nvidia(),
            gemini_api: GeminiApiChatModelClient::default(),
            vertex_gemini: VertexGeminiChatModelClient::default(),
            vertex_anthropic: VertexAnthropicChatModelClient::with_transport(Arc::new(
                StaticVertexAnthropicTransport {
                    endpoint: "https://us-east5-aiplatform.googleapis.com/v1/projects/coddy-dev/locations/us-east5/publishers/anthropic/models/claude-sonnet-4-5@20250929:rawPredict".to_string(),
                    token: "ya29.vertex-token".to_string(),
                    body: expected_body,
                    response: Ok(ChatResponse::from_text("hi from vertex claude")),
                },
            )),
            azure_openai: AzureOpenAiChatModelClient::default(),
            unavailable: UnavailableChatModelClient,
        };
        let request = ChatRequest::new(
            ModelRef {
                provider: "vertex".to_string(),
                name: "claude-sonnet-4-5@20250929".to_string(),
            },
            vec![ChatMessage::system("system"), ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "vertex".to_string(),
            token: "ya29.vertex-token".to_string(),
            endpoint: Some("us-east5".to_string()),
            metadata: [("project_id".to_string(), "coddy-dev".to_string())]
                .into_iter()
                .collect(),
        }))
        .expect("credential");

        assert_eq!(
            client.complete(request),
            Ok(ChatResponse::from_text("hi from vertex claude"))
        );
    }

    #[test]
    fn vertex_anthropic_requires_project_metadata() {
        let client = VertexAnthropicChatModelClient::default();
        let request = ChatRequest::new(
            ModelRef {
                provider: "vertex".to_string(),
                name: "claude-sonnet-4-5@20250929".to_string(),
            },
            vec![ChatMessage::user("hello")],
        )
        .expect("request")
        .with_model_credential(Some(ModelCredential {
            provider: "vertex".to_string(),
            token: "ya29.vertex-token".to_string(),
            endpoint: Some("us-east5".to_string()),
            metadata: Default::default(),
        }))
        .expect("credential");

        let error = client.complete(request).expect_err("project required");

        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn vertex_anthropic_request_projects_tools_options_and_observations() {
        let request = ChatRequest {
            model: ModelRef {
                provider: "vertex".to_string(),
                name: "claude-sonnet-4-5@20250929".to_string(),
            },
            messages: vec![
                ChatMessage::system("system one"),
                ChatMessage::system("system two"),
                ChatMessage::user("user"),
                ChatMessage::assistant("assistant"),
                ChatMessage::tool("filesystem result"),
            ],
            tools: vec![ChatToolSpec {
                name: "filesystem.list_files".to_string(),
                description: "List files".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                approval_policy: ApprovalPolicy::AutoApprove,
            }],
            model_credential: None,
            temperature: Some(0.2),
            max_output_tokens: Some(128),
        };

        let body = vertex_anthropic_chat_body(&request).expect("body");

        assert_eq!(body["anthropic_version"], "vertex-2023-10-16");
        assert_eq!(body["system"], "system one\n\nsystem two");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][2]["role"], "user");
        assert_eq!(
            body["messages"][2]["content"],
            "Tool observations:\nfilesystem result"
        );
        assert_eq!(
            body["tools"][0]["name"],
            "coddy_tool__filesystem__dot__list_files"
        );
        assert_eq!(
            body["tools"][0]["description"],
            "Coddy tool `filesystem.list_files`. List files"
        );
        let temperature = body["temperature"].as_f64().expect("temperature");
        assert!((temperature - 0.2).abs() < 0.000_001);
        assert_eq!(body["max_tokens"], 128);
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn vertex_anthropic_endpoint_uses_project_region_and_model() {
        let credential = ModelCredential {
            provider: "vertex".to_string(),
            token: "ya29.vertex-token".to_string(),
            endpoint: Some("europe-west1".to_string()),
            metadata: [("project_id".to_string(), "coddy-dev".to_string())]
                .into_iter()
                .collect(),
        };

        assert_eq!(
            vertex_anthropic_raw_predict_url("claude-3-5-sonnet@20240620", &credential)
                .expect("url"),
            "https://europe-west1-aiplatform.googleapis.com/v1/projects/coddy-dev/locations/europe-west1/publishers/anthropic/models/claude-3-5-sonnet@20240620:rawPredict"
        );
    }

    #[test]
    fn vertex_quota_project_uses_explicit_quota_before_project_metadata() {
        let credential = ModelCredential {
            provider: "vertex".to_string(),
            token: "ya29.vertex-token".to_string(),
            endpoint: Some("us-east5".to_string()),
            metadata: [
                ("project_id".to_string(), "coddy-dev".to_string()),
                ("quota_project_id".to_string(), "quota-dev".to_string()),
            ]
            .into_iter()
            .collect(),
        };

        assert_eq!(vertex_quota_project_id(&credential), Some("quota-dev"));
    }

    #[test]
    fn ollama_request_projects_tools_and_options() {
        let request = ChatRequest {
            model: ModelRef {
                provider: "ollama".to_string(),
                name: "coder".to_string(),
            },
            messages: vec![
                ChatMessage::system("system"),
                ChatMessage::user("user"),
                ChatMessage::assistant("assistant"),
            ],
            tools: vec![ChatToolSpec {
                name: "filesystem.list_files".to_string(),
                description: "List files".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                approval_policy: ApprovalPolicy::AutoApprove,
            }],
            model_credential: None,
            temperature: Some(0.2),
            max_output_tokens: Some(128),
        };

        let body = ollama_chat_body(&request);

        assert_eq!(body["model"], "coder");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "user");
        assert_eq!(
            body["tools"][0]["function"]["name"],
            "filesystem.list_files"
        );
        let temperature = body["options"]["temperature"]
            .as_f64()
            .expect("temperature");
        assert!((temperature - 0.2).abs() < 0.000_001);
        assert_eq!(body["options"]["num_predict"], 128);
    }

    #[test]
    fn ollama_target_accepts_common_local_host_forms() {
        assert_eq!(
            OllamaHttpTarget::parse("127.0.0.1:11434").expect("host without scheme"),
            OllamaHttpTarget {
                host: "127.0.0.1".to_string(),
                port: 11434,
                path: "/api/chat".to_string(),
            }
        );
        assert_eq!(
            OllamaHttpTarget::parse("http://localhost:11434/").expect("trailing slash"),
            OllamaHttpTarget {
                host: "localhost".to_string(),
                port: 11434,
                path: "/api/chat".to_string(),
            }
        );
    }

    #[test]
    fn ollama_target_rejects_unsupported_schemes() {
        let error = OllamaHttpTarget::parse("https://localhost:11434")
            .expect_err("https is not supported by local transport");

        assert_eq!(error.code(), "invalid_request");
    }

    #[test]
    fn parses_ollama_streaming_ndjson_into_deltas() {
        let body = r#"{"message":{"role":"assistant","content":"hello"},"done":false}
{"message":{"role":"assistant","content":" world"},"done":false}
{"done":true,"done_reason":"stop"}"#;

        let response = parse_ollama_chat_body(body).expect("response");

        assert_eq!(response.text, "hello world");
        assert_eq!(response.deltas, vec!["hello", " world"]);
        assert_eq!(response.finish_reason, ChatFinishReason::Stop);
    }

    #[test]
    fn parses_ollama_tool_calls_without_assistant_content() {
        let body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"filesystem.list_files","arguments":{"path":"."}}}]},"done":true,"done_reason":"stop"}"#;

        let response = parse_ollama_chat_body(body).expect("response");

        assert_eq!(response.text, "");
        assert!(response.deltas.is_empty());
        assert_eq!(response.finish_reason, ChatFinishReason::ToolCalls);
        assert_eq!(
            response.tool_calls,
            vec![ChatToolCall {
                id: None,
                name: "filesystem.list_files".to_string(),
                arguments: serde_json::json!({ "path": "." }),
            }]
        );
    }

    #[test]
    fn parses_ollama_tool_call_arguments_from_json_string() {
        let body = r#"{"message":{"role":"assistant","tool_calls":[{"id":"call-1","function":{"name":"filesystem.read_file","arguments":"{\"path\":\"README.md\"}"}}]},"done":true}"#;

        let response = parse_ollama_chat_body(body).expect("response");

        assert_eq!(
            response.tool_calls,
            vec![ChatToolCall {
                id: Some("call-1".to_string()),
                name: "filesystem.read_file".to_string(),
                arguments: serde_json::json!({ "path": "README.md" }),
            }]
        );
        assert_eq!(response.finish_reason, ChatFinishReason::ToolCalls);
    }

    #[test]
    fn parses_openai_compatible_text_response() {
        let body = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "hello from cloud"
                    },
                    "finish_reason": "stop"
                }
            ]
        })
        .to_string();

        let response = parse_openai_compatible_chat_body("openai", &body).expect("response");

        assert_eq!(response.text, "hello from cloud");
        assert_eq!(response.deltas, vec!["hello from cloud"]);
        assert_eq!(response.finish_reason, ChatFinishReason::Stop);
    }

    #[test]
    fn parses_openai_compatible_tool_calls() {
        let body = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [
                            {
                                "id": "call-1",
                                "type": "function",
                                "function": {
                                    "name": "coddy_tool__filesystem__dot__list_files",
                                    "arguments": "{\"path\":\".\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ]
        })
        .to_string();

        let response = parse_openai_compatible_chat_body("openrouter", &body).expect("response");

        assert_eq!(response.text, "");
        assert!(response.deltas.is_empty());
        assert_eq!(response.finish_reason, ChatFinishReason::ToolCalls);
        assert_eq!(
            response.tool_calls,
            vec![ChatToolCall {
                id: Some("call-1".to_string()),
                name: "filesystem.list_files".to_string(),
                arguments: serde_json::json!({ "path": "." }),
            }]
        );
    }

    #[test]
    fn parses_openrouter_error_metadata_from_success_status_body() {
        let body = serde_json::json!({
            "error": {
                "code": 502,
                "message": "Provider returned error",
                "metadata": {
                    "provider_name": "DeepSeek",
                    "raw": {
                        "error": {
                            "message": "upstream overloaded sk-or-secret-token"
                        }
                    }
                }
            }
        })
        .to_string();

        let error =
            parse_openai_compatible_chat_body("openrouter", &body).expect_err("provider error");

        assert_eq!(error.code(), "provider_error");
        let ChatModelError::ProviderError {
            provider,
            message,
            retryable,
        } = error
        else {
            panic!("expected provider error");
        };
        assert_eq!(provider, "openrouter");
        assert!(retryable);
        assert!(message.contains("Provider returned error"));
        assert!(message.contains("code 502"));
        assert!(message.contains("upstream provider: DeepSeek"));
        assert!(message.contains("upstream detail: upstream overloaded"));
        assert!(message.contains("sk-or-[REDACTED]"));
        assert!(!message.contains("secret-token"));
    }

    #[test]
    fn provider_error_redaction_handles_nvidia_api_tokens() {
        let redacted = redact_provider_error_text(
            "NVIDIA returned nvapi-secret-token and Authorization: Bearer abc.DEF_123",
        );

        assert!(redacted.contains("nvapi-[REDACTED]"));
        assert!(redacted.contains("Bearer [REDACTED]"));
        assert!(!redacted.contains("secret-token"));
        assert!(!redacted.contains("abc.DEF_123"));
    }

    #[test]
    fn parses_openrouter_choice_error_as_provider_error() {
        let body = serde_json::json!({
            "choices": [
                {
                    "finish_reason": "error",
                    "error": {
                        "code": "server_error",
                        "message": "Provider returned error",
                        "metadata": {
                            "provider_name": "DeepSeek",
                            "raw": { "message": "provider disconnected" }
                        }
                    }
                }
            ]
        })
        .to_string();

        let error =
            parse_openai_compatible_chat_body("openrouter", &body).expect_err("provider error");

        let ChatModelError::ProviderError {
            message, retryable, ..
        } = error
        else {
            panic!("expected provider error");
        };
        assert!(retryable);
        assert!(message.contains("Provider returned error"));
        assert!(message.contains("code server_error"));
        assert!(message.contains("upstream provider: DeepSeek"));
        assert!(message.contains("provider disconnected"));
    }

    #[test]
    fn treats_openrouter_generic_provider_returned_error_as_retryable() {
        let body = serde_json::json!({
            "choices": [
                {
                    "finish_reason": "error",
                    "error": {
                        "message": "Provider returned error"
                    }
                }
            ]
        })
        .to_string();

        let error =
            parse_openai_compatible_chat_body("openrouter", &body).expect_err("provider error");

        let ChatModelError::ProviderError {
            message, retryable, ..
        } = error
        else {
            panic!("expected provider error");
        };
        assert!(retryable);
        assert!(message.contains("Provider returned error"));
    }

    #[test]
    fn parses_gemini_api_text_response() {
        let body = serde_json::json!({
            "candidates": [
                {
                    "content": {
                        "role": "model",
                        "parts": [
                            { "text": "hello" },
                            { "text": " from gemini" }
                        ]
                    },
                    "finishReason": "STOP"
                }
            ]
        })
        .to_string();

        let response = parse_gemini_api_response(&body).expect("response");

        assert_eq!(response.text, "hello from gemini");
        assert_eq!(response.deltas, vec!["hello", " from gemini"]);
        assert_eq!(response.finish_reason, ChatFinishReason::Stop);
    }

    #[test]
    fn parses_gemini_api_function_calls() {
        let body = serde_json::json!({
            "candidates": [
                {
                    "content": {
                        "role": "model",
                        "parts": [
                            {
                                "functionCall": {
                                    "name": "coddy_tool__filesystem__dot__list_files",
                                    "args": { "path": "." }
                                }
                            }
                        ]
                    },
                    "finishReason": "STOP"
                }
            ]
        })
        .to_string();

        let response = parse_gemini_api_response(&body).expect("response");

        assert_eq!(response.text, "");
        assert!(response.deltas.is_empty());
        assert_eq!(response.finish_reason, ChatFinishReason::ToolCalls);
        assert_eq!(
            response.tool_calls,
            vec![ChatToolCall {
                id: None,
                name: "filesystem.list_files".to_string(),
                arguments: serde_json::json!({ "path": "." }),
            }]
        );
    }

    #[test]
    fn parses_vertex_anthropic_text_response() {
        let body = serde_json::json!({
            "id": "msg-test",
            "type": "message",
            "role": "assistant",
            "content": [
                { "type": "text", "text": "hello" },
                { "type": "text", "text": " from vertex" }
            ],
            "stop_reason": "end_turn"
        })
        .to_string();

        let response = parse_vertex_anthropic_response(&body).expect("response");

        assert_eq!(response.text, "hello from vertex");
        assert_eq!(response.deltas, vec!["hello", " from vertex"]);
        assert_eq!(response.finish_reason, ChatFinishReason::Stop);
    }

    #[test]
    fn parses_vertex_anthropic_tool_use_response() {
        let body = serde_json::json!({
            "id": "msg-test",
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu-test",
                    "name": "coddy_tool__filesystem__dot__list_files",
                    "input": { "path": "." }
                }
            ],
            "stop_reason": "tool_use"
        })
        .to_string();

        let response = parse_vertex_anthropic_response(&body).expect("response");

        assert_eq!(response.text, "");
        assert!(response.deltas.is_empty());
        assert_eq!(response.finish_reason, ChatFinishReason::ToolCalls);
        assert_eq!(
            response.tool_calls,
            vec![ChatToolCall {
                id: Some("toolu-test".to_string()),
                name: "filesystem.list_files".to_string(),
                arguments: serde_json::json!({ "path": "." }),
            }]
        );
    }

    #[test]
    fn parses_chunked_ollama_http_response() {
        let chunk = r#"{"message":{"role":"assistant","content":"hi"},"done":false}
"#;
        let raw = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            chunk.len(),
            chunk
        );

        let response = parse_http_ollama_response(raw.as_bytes()).expect("response");

        assert_eq!(response.text, "hi");
        assert_eq!(response.deltas, vec!["hi"]);
    }

    #[test]
    fn maps_ollama_http_errors_to_provider_errors() {
        let raw = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 26\r\n\r\n{\"error\":\"model missing\"}";

        let error = parse_http_ollama_response(raw.as_bytes()).expect_err("provider error");

        assert_eq!(
            error,
            ChatModelError::ProviderError {
                provider: "ollama".to_string(),
                message: "model missing".to_string(),
                retryable: true,
            }
        );
    }
}
