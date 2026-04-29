use coddy_core::{ApprovalPolicy, ModelRef, ToolDefinition, ToolRiskLevel};
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
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatFinishReason {
    Stop,
    Length,
    ToolCalls,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub text: String,
    pub deltas: Vec<String>,
    pub finish_reason: ChatFinishReason,
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
}

pub type ChatModelResult = Result<ChatResponse, ChatModelError>;

pub trait ChatModelClient: std::fmt::Debug + Send + Sync {
    fn complete(&self, request: ChatRequest) -> ChatModelResult;
}

#[derive(Debug, Default)]
pub struct UnavailableChatModelClient;

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
            temperature: None,
            max_output_tokens: None,
        })
    }

    pub fn with_tools(mut self, tools: Vec<ChatToolSpec>) -> Self {
        self.tools = tools;
        self
    }
}

impl ChatResponse {
    pub fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            deltas: vec![text.clone()],
            text,
            finish_reason: ChatFinishReason::Stop,
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
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, Self::ProviderUnavailable { .. })
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

#[cfg(test)]
mod tests {
    use coddy_core::{
        ApprovalPolicy, ToolCategory, ToolName, ToolPermission, ToolRiskLevel, ToolSchema,
    };
    use serde_json::json;

    use super::*;

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
}
