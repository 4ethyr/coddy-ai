use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};

use coddy_core::{ApprovalPolicy, ModelCredential, ModelRef, ToolDefinition, ToolRiskLevel};
use serde_json::Value;
use thiserror::Error;

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
            Arc::new(HttpOpenAiCompatibleTransport::new()),
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
                    retryable: status == 429 || status >= 500,
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
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value["error"]["message"]
                .as_str()
                .or_else(|| value["error"].as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("{provider} returned HTTP {status}"))
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
    if let Some(message) = value["error"]["message"]
        .as_str()
        .or_else(|| value["error"].as_str())
    {
        return Err(ChatModelError::ProviderError {
            provider: provider.to_string(),
            message: message.to_string(),
            retryable: false,
        });
    }

    let choice = value["choices"]
        .as_array()
        .and_then(|choices| choices.first())
        .ok_or_else(|| ChatModelError::InvalidProviderResponse {
            provider: provider.to_string(),
            message: "response did not include choices".to_string(),
        })?;
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
                name: name.to_string(),
                arguments: parse_tool_arguments_for_provider(provider, &function["arguments"])?,
            })
        })
        .collect()
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
        }))
        .expect("credential");

        assert_eq!(
            client.complete(request),
            Ok(ChatResponse::from_text("hi from openai"))
        );
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
            "filesystem.list_files"
        );
        assert_eq!(body["tool_choice"], "auto");
        let temperature = body["temperature"].as_f64().expect("temperature");
        assert!((temperature - 0.2).abs() < 0.000_001);
        assert_eq!(body["max_tokens"], 128);
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
                                    "name": "filesystem.list_files",
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
