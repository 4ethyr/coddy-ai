use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ToolContractError {
    #[error("tool name cannot be empty")]
    EmptyName,

    #[error("tool name contains an invalid character: {0}")]
    InvalidNameCharacter(char),

    #[error("tool description cannot be empty")]
    EmptyDescription,

    #[error("tool timeout must be greater than zero")]
    InvalidTimeout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ToolName(String);

impl ToolName {
    pub fn new(value: impl Into<String>) -> Result<Self, ToolContractError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ToolContractError::EmptyName);
        }
        for character in trimmed.chars() {
            if !(character.is_ascii_alphanumeric()
                || character == '_'
                || character == '-'
                || character == '.')
            {
                return Err(ToolContractError::InvalidNameCharacter(character));
            }
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ToolName {
    type Error = ToolContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for ToolName {
    type Error = ToolContractError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl std::fmt::Display for ToolName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    Filesystem,
    Search,
    Shell,
    Git,
    Network,
    Memory,
    Eval,
    Mcp,
    Subagent,
    Repl,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolPermission {
    ReadWorkspace,
    WriteWorkspace,
    ReadExternalPath,
    WriteExternalPath,
    ExecuteCommand,
    AccessNetwork,
    ManageMemory,
    UseMcp,
    DelegateSubagent,
    RequestUserInput,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ApprovalPolicy {
    AutoApprove,
    AskOnUse,
    AlwaysAsk,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSchema {
    #[serde(with = "crate::json_value_wire")]
    pub schema: Value,
}

impl ToolSchema {
    pub fn new(schema: Value) -> Self {
        Self { schema }
    }

    pub fn empty_object() -> Self {
        Self::new(serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: ToolName,
    pub description: String,
    pub category: ToolCategory,
    pub input_schema: ToolSchema,
    pub output_schema: ToolSchema,
    pub risk_level: ToolRiskLevel,
    pub permissions: Vec<ToolPermission>,
    pub timeout_ms: u64,
    pub approval_policy: ApprovalPolicy,
}

impl ToolDefinition {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: ToolName,
        description: impl Into<String>,
        category: ToolCategory,
        input_schema: ToolSchema,
        output_schema: ToolSchema,
        risk_level: ToolRiskLevel,
        permissions: Vec<ToolPermission>,
        timeout_ms: u64,
        approval_policy: ApprovalPolicy,
    ) -> Result<Self, ToolContractError> {
        let description = description.into();
        if description.trim().is_empty() {
            return Err(ToolContractError::EmptyDescription);
        }
        if timeout_ms == 0 {
            return Err(ToolContractError::InvalidTimeout);
        }
        Ok(Self {
            name,
            description,
            category,
            input_schema,
            output_schema,
            risk_level,
            permissions,
            timeout_ms,
            approval_policy,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: Uuid,
    pub session_id: Uuid,
    pub run_id: Uuid,
    pub tool_name: ToolName,
    #[serde(with = "crate::json_value_wire")]
    pub input: Value,
    pub requested_at_unix_ms: u64,
}

impl ToolCall {
    pub fn new(
        session_id: Uuid,
        run_id: Uuid,
        tool_name: ToolName,
        input: Value,
        requested_at_unix_ms: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            run_id,
            tool_name,
            input,
            requested_at_unix_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolResultStatus {
    Succeeded,
    Failed,
    Cancelled,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl ToolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolOutput {
    pub text: String,
    #[serde(with = "crate::json_value_wire")]
    pub metadata: Value,
    pub truncated: bool,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            metadata: Value::Object(Default::default()),
            truncated: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub call_id: Uuid,
    pub status: ToolResultStatus,
    pub output: Option<ToolOutput>,
    pub error: Option<ToolError>,
    pub started_at_unix_ms: u64,
    pub completed_at_unix_ms: u64,
}

impl ToolResult {
    pub fn succeeded(
        call_id: Uuid,
        output: ToolOutput,
        started_at_unix_ms: u64,
        completed_at_unix_ms: u64,
    ) -> Self {
        Self {
            call_id,
            status: ToolResultStatus::Succeeded,
            output: Some(output),
            error: None,
            started_at_unix_ms,
            completed_at_unix_ms,
        }
    }

    pub fn failed(
        call_id: Uuid,
        error: ToolError,
        started_at_unix_ms: u64,
        completed_at_unix_ms: u64,
    ) -> Self {
        Self {
            call_id,
            status: ToolResultStatus::Failed,
            output: None,
            error: Some(error),
            started_at_unix_ms,
            completed_at_unix_ms,
        }
    }

    pub fn denied(
        call_id: Uuid,
        error: ToolError,
        started_at_unix_ms: u64,
        completed_at_unix_ms: u64,
    ) -> Self {
        Self {
            call_id,
            status: ToolResultStatus::Denied,
            output: None,
            error: Some(error),
            started_at_unix_ms,
            completed_at_unix_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_name_rejects_empty_and_invalid_names() {
        assert_eq!(ToolName::new(""), Err(ToolContractError::EmptyName));
        assert_eq!(
            ToolName::new("read file"),
            Err(ToolContractError::InvalidNameCharacter(' '))
        );
        assert!(ToolName::new("filesystem.read_file").is_ok());
        assert!(ToolName::new("search-files").is_ok());
    }

    #[test]
    fn tool_definition_requires_description_and_timeout() {
        let name = ToolName::new("read_file").expect("valid tool name");
        let schema = ToolSchema::empty_object();

        assert_eq!(
            ToolDefinition::new(
                name.clone(),
                "",
                ToolCategory::Filesystem,
                schema.clone(),
                schema.clone(),
                ToolRiskLevel::Low,
                vec![ToolPermission::ReadWorkspace],
                1000,
                ApprovalPolicy::AutoApprove,
            ),
            Err(ToolContractError::EmptyDescription)
        );

        assert_eq!(
            ToolDefinition::new(
                name,
                "Read a file from the active workspace",
                ToolCategory::Filesystem,
                schema.clone(),
                schema,
                ToolRiskLevel::Low,
                vec![ToolPermission::ReadWorkspace],
                0,
                ApprovalPolicy::AutoApprove,
            ),
            Err(ToolContractError::InvalidTimeout)
        );
    }

    #[test]
    fn tool_call_and_result_are_json_serializable() {
        let call = ToolCall::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            ToolName::new("filesystem.read_file").expect("valid tool name"),
            serde_json::json!({ "path": "Cargo.toml" }),
            1_775_000_000_000,
        );
        let result = ToolResult::succeeded(
            call.id,
            ToolOutput::text("[workspace]"),
            1_775_000_000_001,
            1_775_000_000_002,
        );

        let encoded = serde_json::to_string(&result).expect("serialize result");
        let decoded: ToolResult = serde_json::from_str(&encoded).expect("deserialize result");

        assert_eq!(decoded, result);
        assert_eq!(call.tool_name.as_str(), "filesystem.read_file");
    }
}
