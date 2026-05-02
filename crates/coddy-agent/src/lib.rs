pub mod agent_loop;
pub mod agent_run_v2;
pub mod command_guard;
pub mod context;
pub mod eval;
pub mod model;
pub mod plan_executor;
pub mod router;
pub mod runtime;
pub mod shell_executor;
pub mod shell_plan;
pub mod subagent;
pub mod subagent_executor;

use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

pub use agent_loop::{
    model_tool_call_may_run, AgenticLoopConfig, AgenticLoopOutcome, AgenticLoopRequest,
    AgenticLoopStop, AgenticModelLoop,
};
pub use agent_run_v2::{
    AgentRunAction, AgentRunFailure, AgentRunTransition, AgentRunTransitionError, AgentRunV2,
};
pub use coddy_core::{AgentRunPhase, AgentRunStopReason, AgentRunSummary};
pub use command_guard::{
    BlockedCommandReason, CommandAssessment, CommandDecision, CommandGuard, CommandRisk,
    SHELL_RUN_TOOL,
};
pub use context::{ContextObservation, ContextPlanItem, ContextSnapshot, ContextTool};
pub use eval::{
    default_prompt_battery_cases, extract_prompt_battery_members, guard_prompt_battery_members,
    prompt_battery_routing_messages, run_default_prompt_battery, run_live_prompt_battery_cases,
    run_prompt_battery, EvalCase, EvalExpectations, EvalGateReport, EvalGateStatus,
    EvalQualityGate, EvalReport, EvalRunner, EvalStatus, EvalSuiteReport,
    LivePromptBatteryCaseResult, LivePromptBatteryReport, MultiagentEvalBaselineComparison,
    MultiagentEvalBaselineError, MultiagentEvalCase, MultiagentEvalReport, MultiagentEvalRunner,
    MultiagentEvalSuiteReport, MultiagentExecutionMetrics, PromptBatteryCase, PromptBatteryFailure,
    PromptBatteryReport,
};
pub use model::{
    decode_provider_safe_tool_name, ChatFinishReason, ChatMessage, ChatMessageRole,
    ChatModelClient, ChatModelError, ChatModelResult, ChatRequest, ChatResponse, ChatToolCall,
    ChatToolSpec, DefaultChatModelClient, OllamaChatModelClient, UnavailableChatModelClient,
};
pub use plan_executor::{
    DeterministicPlanExecutor, DeterministicPlanItem, DeterministicPlanReport,
    DeterministicPlanStatus,
};
pub use router::{LocalToolRouteOutcome, LocalToolRouter};
pub use runtime::{
    AgentRunStatus, AgentStep, AgentStepKind, AgentStepStatus, LocalAgentRuntime, Observation,
    PlanItem, RunState,
};
pub use shell_executor::{ShellExecutionConfig, ShellExecutor, DEFAULT_SHELL_OUTPUT_LIMIT_BYTES};
pub use shell_plan::{
    ShellApprovalState, ShellPlan, ShellPlanRequest, ShellPlanner, DEFAULT_SHELL_TIMEOUT_MS,
    MAX_SHELL_TIMEOUT_MS,
};
pub use subagent::{
    SubagentDefinition, SubagentHandoffPlan, SubagentMode, SubagentRecommendation,
    SubagentRegistry, SubagentTeamGateStatus, SubagentTeamMember, SubagentTeamMetrics,
    SubagentTeamPlan, SUBAGENT_LIST_TOOL, SUBAGENT_PREPARE_TOOL, SUBAGENT_REDUCE_OUTPUTS_TOOL,
    SUBAGENT_ROUTE_TOOL, SUBAGENT_TEAM_PLAN_TOOL,
};
pub use subagent_executor::{
    SubagentExecutionCompletionPlan, SubagentExecutionCoordinator, SubagentExecutionGate,
    SubagentExecutionHandoff, SubagentExecutionOutcomeStatus, SubagentExecutionOutputStatus,
    SubagentExecutionRecord, SubagentExecutionStartPlan, SubagentExecutionStartStatus,
    SubagentExecutionSummary, SubagentOutputContract,
};

use coddy_core::{
    ApprovalPolicy, PermissionReply, PermissionRequest, ReplEvent, ToolCall, ToolCategory,
    ToolDefinition, ToolError, ToolName, ToolOutput, ToolPermission, ToolResult, ToolResultStatus,
    ToolRiskLevel, ToolSchema, ToolStatus,
};
use serde_json::{json, Value};
use thiserror::Error;

pub const LIST_FILES_TOOL: &str = "filesystem.list_files";
pub const APPLY_EDIT_TOOL: &str = "filesystem.apply_edit";
pub const PREVIEW_EDIT_TOOL: &str = "filesystem.preview_edit";
pub const READ_FILE_TOOL: &str = "filesystem.read_file";
pub const SEARCH_FILES_TOOL: &str = "filesystem.search_files";

const DEFAULT_MAX_READ_BYTES: u64 = 128 * 1024;
const DEFAULT_MAX_LIST_ENTRIES: usize = 200;
const DEFAULT_MAX_SEARCH_MATCHES: usize = 100;
const MAX_SEARCH_FILE_BYTES: u64 = 512 * 1024;
const MAX_SEARCH_MATCH_TEXT_CHARS: usize = 240;
const SENSITIVE_KEY_MARKERS: &[&str] = &[
    "api_key",
    "apikey",
    "auth",
    "credential",
    "password",
    "secret",
    "token",
];
const SEARCH_IGNORED_DIRS: &[&str] = &[
    ".git",
    ".next",
    ".turbo",
    ".cache",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "target",
];

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),

    #[error("invalid tool input: {0}")]
    InvalidInput(String),

    #[error("workspace root must be an existing directory: {0}")]
    InvalidWorkspaceRoot(String),

    #[error("absolute paths are not allowed in workspace tools: {0}")]
    AbsolutePath(String),

    #[error("path traversal is not allowed: {0}")]
    PathTraversal(String),

    #[error("path escapes workspace root: {0}")]
    OutsideWorkspace(String),

    #[error("path is not a file: {0}")]
    NotFile(String),

    #[error("path is not a directory: {0}")]
    NotDirectory(String),

    #[error("file must be read before editing: {0}")]
    FileNotRead(String),

    #[error("file changed since it was read: {0}")]
    StaleRead(String),

    #[error("old string was not found in file: {0}")]
    OldStringNotFound(String),

    #[error("old string appears more than once in file: {0}")]
    OldStringNotUnique(String),

    #[error("edit preview would not change file: {0}")]
    NoChanges(String),

    #[error("permission request could not be created: {0}")]
    PermissionContract(String),

    #[error("permission was rejected for edit: {0}")]
    PermissionRejected(String),

    #[error("io error for {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl AgentError {
    fn code(&self) -> &'static str {
        match self {
            Self::UnknownTool(_) => "unknown_tool",
            Self::InvalidInput(_) => "invalid_input",
            Self::InvalidWorkspaceRoot(_) => "invalid_workspace_root",
            Self::AbsolutePath(_) => "absolute_path",
            Self::PathTraversal(_) => "path_traversal",
            Self::OutsideWorkspace(_) => "outside_workspace",
            Self::NotFile(_) => "not_file",
            Self::NotDirectory(_) => "not_directory",
            Self::FileNotRead(_) => "file_not_read",
            Self::StaleRead(_) => "stale_read",
            Self::OldStringNotFound(_) => "old_string_not_found",
            Self::OldStringNotUnique(_) => "old_string_not_unique",
            Self::NoChanges(_) => "no_changes",
            Self::PermissionContract(_) => "permission_contract",
            Self::PermissionRejected(_) => "permission_rejected",
            Self::Io { .. } => "io_error",
        }
    }

    fn retryable(&self) -> bool {
        matches!(self, Self::Io { .. })
    }

    pub(crate) fn into_tool_error(self) -> ToolError {
        ToolError::new(self.code(), self.to_string(), self.retryable())
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceRoot {
    root: PathBuf,
}

impl WorkspaceRoot {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, AgentError> {
        let root = root.as_ref();
        let canonical = root.canonicalize().map_err(|source| AgentError::Io {
            path: root.display().to_string(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(AgentError::InvalidWorkspaceRoot(
                canonical.display().to_string(),
            ));
        }
        Ok(Self { root: canonical })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn resolve_existing_path(&self, relative_path: &str) -> Result<PathBuf, AgentError> {
        self.resolve_existing(relative_path)
    }

    pub fn relative_path(&self, path: &Path) -> String {
        self.relative_display(path)
    }

    fn resolve_existing(&self, relative_path: &str) -> Result<PathBuf, AgentError> {
        let relative_path = relative_path.trim();
        let relative_path = if relative_path.is_empty() {
            "."
        } else {
            relative_path
        };
        let requested = Path::new(relative_path);

        if requested.is_absolute() {
            return Err(AgentError::AbsolutePath(relative_path.to_string()));
        }
        if requested
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(AgentError::PathTraversal(relative_path.to_string()));
        }

        let candidate = self.root.join(requested);
        let canonical = candidate.canonicalize().map_err(|source| AgentError::Io {
            path: candidate.display().to_string(),
            source,
        })?;
        if !canonical.starts_with(&self.root) {
            return Err(AgentError::OutsideWorkspace(relative_path.to_string()));
        }
        Ok(canonical)
    }

    fn relative_display(&self, path: &Path) -> String {
        let relative = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if relative.is_empty() {
            ".".to_string()
        } else {
            relative
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReadOnlyToolRegistry {
    definitions: Vec<ToolDefinition>,
}

#[derive(Debug, Clone)]
pub struct AgentToolRegistry {
    definitions: Vec<ToolDefinition>,
}

struct LocalToolDefinitionSpec {
    name: &'static str,
    description: &'static str,
    category: ToolCategory,
    input_schema: Value,
    output_schema: Value,
    risk_level: ToolRiskLevel,
    permissions: Vec<ToolPermission>,
    timeout_ms: u64,
    approval_policy: ApprovalPolicy,
}

impl Default for ReadOnlyToolRegistry {
    fn default() -> Self {
        Self {
            definitions: vec![
                tool_definition(
                    LIST_FILES_TOOL,
                    "List files inside the active workspace",
                    json!({
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "path": { "type": "string" },
                            "max_entries": { "type": "integer", "minimum": 1 }
                        }
                    }),
                ),
                tool_definition(
                    READ_FILE_TOOL,
                    "Read a UTF-8 text file inside the active workspace",
                    json!({
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["path"],
                        "properties": {
                            "path": { "type": "string" },
                            "max_bytes": { "type": "integer", "minimum": 1 }
                        }
                    }),
                ),
                tool_definition(
                    SEARCH_FILES_TOOL,
                    "Search UTF-8 text files inside the active workspace",
                    json!({
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["query"],
                        "properties": {
                            "path": { "type": "string" },
                            "query": { "type": "string" },
                            "max_matches": { "type": "integer", "minimum": 1 }
                        }
                    }),
                ),
            ],
        }
    }
}

impl Default for AgentToolRegistry {
    fn default() -> Self {
        let mut definitions = ReadOnlyToolRegistry::default().definitions().to_vec();
        definitions.extend([
            local_tool_definition(LocalToolDefinitionSpec {
                name: PREVIEW_EDIT_TOOL,
                description: "Preview a replace edit after validating read-before-edit safety",
                category: ToolCategory::Filesystem,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["path", "old_string", "new_string"],
                    "properties": {
                        "path": { "type": "string" },
                        "old_string": { "type": "string" },
                        "new_string": { "type": "string" },
                        "replace_all": { "type": "boolean" }
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": { "type": "string" },
                        "diff": { "type": "string" },
                        "additions": { "type": "integer" },
                        "removals": { "type": "integer" }
                    }
                }),
                risk_level: ToolRiskLevel::Medium,
                permissions: vec![
                    ToolPermission::ReadWorkspace,
                    ToolPermission::WriteWorkspace,
                ],
                timeout_ms: 5_000,
                approval_policy: ApprovalPolicy::AskOnUse,
            }),
            local_tool_definition(LocalToolDefinitionSpec {
                name: APPLY_EDIT_TOOL,
                description: "Apply a previously approved edit with stale-read revalidation",
                category: ToolCategory::Filesystem,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["permission_request_id", "reply"],
                    "properties": {
                        "permission_request_id": { "type": "string" },
                        "reply": { "type": "string", "enum": ["once", "always", "reject"] }
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": { "type": "string" },
                        "diff": { "type": "string" },
                        "additions": { "type": "integer" },
                        "removals": { "type": "integer" }
                    }
                }),
                risk_level: ToolRiskLevel::High,
                permissions: vec![ToolPermission::WriteWorkspace],
                timeout_ms: 10_000,
                approval_policy: ApprovalPolicy::AlwaysAsk,
            }),
            local_tool_definition(LocalToolDefinitionSpec {
                name: SHELL_RUN_TOOL,
                description: "Execute a workspace-scoped shell command through command guard, approval policy, timeout and output truncation",
                category: ToolCategory::Shell,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["command"],
                    "properties": {
                        "command": { "type": "string" },
                        "description": { "type": "string" },
                        "cwd": { "type": "string" },
                        "timeout_ms": { "type": "integer", "minimum": 1 }
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "command": { "type": "string" },
                        "cwd": { "type": "string" },
                        "exit_code": { "type": ["integer", "null"] },
                        "success": { "type": "boolean" },
                        "duration_ms": { "type": "integer" },
                        "stdout": { "type": "string" },
                        "stderr": { "type": "string" },
                        "stdout_truncated": { "type": "boolean" },
                        "stderr_truncated": { "type": "boolean" }
                    }
                }),
                risk_level: ToolRiskLevel::Medium,
                permissions: vec![ToolPermission::ExecuteCommand],
                timeout_ms: DEFAULT_SHELL_TIMEOUT_MS,
                approval_policy: ApprovalPolicy::AskOnUse,
            }),
            local_tool_definition(LocalToolDefinitionSpec {
                name: SUBAGENT_LIST_TOOL,
                description: "List declarative subagent roles, modes, allowed tools and response contracts",
                category: ToolCategory::Subagent,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["read-only", "workspace-write", "evaluation"]
                        }
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "mode": { "type": ["string", "null"] },
                        "subagents": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "description": { "type": "string" },
                                    "mode": { "type": "string" },
                                    "allowedTools": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    },
                                    "routingSignals": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    },
                                    "timeoutMs": { "type": "integer" },
                                    "maxContextTokens": { "type": "integer" },
                                    "outputSchema": { "type": "object" }
                                }
                            }
                        }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                permissions: vec![ToolPermission::DelegateSubagent],
                timeout_ms: 2_000,
                approval_policy: ApprovalPolicy::AutoApprove,
            }),
            local_tool_definition(LocalToolDefinitionSpec {
                name: SUBAGENT_PREPARE_TOOL,
                description: "Prepare a safe subagent handoff contract with allowed tools, prompt, checklist and output schema without executing the subagent",
                category: ToolCategory::Subagent,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "goal"],
                    "properties": {
                        "name": { "type": "string" },
                        "goal": { "type": "string" }
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "handoff": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "description": { "type": "string" },
                                "mode": { "type": "string" },
                                "goal": { "type": "string" },
                                "allowedTools": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "timeoutMs": { "type": "integer" },
                                "maxContextTokens": { "type": "integer" },
                                "approvalRequired": { "type": "boolean" },
                                "handoffPrompt": { "type": "string" },
                                "validationChecklist": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "safetyNotes": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "readinessScore": { "type": "integer" },
                                "readinessIssues": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "outputSchema": { "type": "object" }
                            }
                        }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                permissions: vec![ToolPermission::DelegateSubagent],
                timeout_ms: 2_000,
                approval_policy: ApprovalPolicy::AutoApprove,
            }),
            local_tool_definition(LocalToolDefinitionSpec {
                name: SUBAGENT_ROUTE_TOOL,
                description: "Recommend focused subagent roles for a task using deterministic routing scores and safety metadata",
                category: ToolCategory::Subagent,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["goal"],
                    "properties": {
                        "goal": { "type": "string" },
                        "mode": {
                            "type": "string",
                            "enum": ["read-only", "workspace-write", "evaluation"]
                        },
                        "limit": { "type": "integer", "minimum": 1 }
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "goal": { "type": "string" },
                        "mode": { "type": ["string", "null"] },
                        "recommendations": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "score": { "type": "integer" },
                                    "mode": { "type": "string" },
                                    "matchedSignals": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    },
                                    "rationale": { "type": "string" },
                                    "allowedTools": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    },
                                    "timeoutMs": { "type": "integer" },
                                    "maxContextTokens": { "type": "integer" },
                                    "outputSchema": { "type": "object" }
                                }
                            }
                        }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                permissions: vec![ToolPermission::DelegateSubagent],
                timeout_ms: 2_000,
                approval_policy: ApprovalPolicy::AutoApprove,
            }),
            local_tool_definition(LocalToolDefinitionSpec {
                name: SUBAGENT_TEAM_PLAN_TOOL,
                description: "Compose a deterministic multiagent team plan with readiness, gate status, output contracts and measurable hardness metrics without executing subagents",
                category: ToolCategory::Subagent,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["goal"],
                    "properties": {
                        "goal": { "type": "string" },
                        "max_members": { "type": "integer", "minimum": 1 }
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "team": {
                            "type": "object",
                            "properties": {
                                "goal": { "type": "string" },
                                "members": { "type": "array" },
                                "metrics": { "type": "object" },
                                "risks": { "type": "array", "items": { "type": "string" } },
                                "validationStrategy": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                }
                            }
                        }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                permissions: vec![ToolPermission::DelegateSubagent],
                timeout_ms: 2_000,
                approval_policy: ApprovalPolicy::AutoApprove,
            }),
            local_tool_definition(LocalToolDefinitionSpec {
                name: SUBAGENT_REDUCE_OUTPUTS_TOOL,
                description: "Validate and consolidate structured subagent outputs against prepared handoff contracts without executing subagents",
                category: ToolCategory::Subagent,
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["goal", "outputs"],
                    "properties": {
                        "goal": { "type": "string" },
                        "max_members": { "type": "integer", "minimum": 1 },
                        "approved_subagents": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "outputs": {
                            "type": "object",
                            "additionalProperties": { "type": "object" }
                        }
                    }
                }),
                output_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "team": { "type": "object" },
                        "summary": {
                            "type": "object",
                            "properties": {
                                "total": { "type": "integer" },
                                "completed": { "type": "integer" },
                                "failed": { "type": "integer" },
                                "blocked": { "type": "integer" },
                                "awaitingApproval": { "type": "integer" },
                                "acceptedOutputs": { "type": "integer" },
                                "rejectedOutputs": { "type": "integer" },
                                "missingOutputs": { "type": "integer" },
                                "unexpectedOutputs": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "records": { "type": "array" }
                            }
                        }
                    }
                }),
                risk_level: ToolRiskLevel::Low,
                permissions: vec![ToolPermission::DelegateSubagent],
                timeout_ms: 2_000,
                approval_policy: ApprovalPolicy::AutoApprove,
            }),
        ]);

        Self { definitions }
    }
}

impl ReadOnlyToolRegistry {
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn get(&self, name: &ToolName) -> Option<&ToolDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.name == *name)
    }
}

impl AgentToolRegistry {
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn get(&self, name: &ToolName) -> Option<&ToolDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.name == *name)
    }
}

#[derive(Debug, Clone)]
pub struct ReadOnlyToolExecutor {
    workspace: WorkspaceRoot,
    registry: ReadOnlyToolRegistry,
    read_tracker: Arc<Mutex<FileReadTracker>>,
}

#[derive(Debug, Clone)]
pub struct ToolExecution {
    pub result: ToolResult,
    pub events: Vec<ReplEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditPreview {
    pub path: String,
    pub old_content: String,
    pub new_content: String,
    pub old_string: String,
    pub new_string: String,
    pub replace_all: bool,
    pub diff: String,
    pub additions: usize,
    pub removals: usize,
    pub read_record: FileReadRecord,
    pub permission_request: PermissionRequest,
}

impl ReadOnlyToolExecutor {
    pub fn new(workspace_root: impl AsRef<Path>) -> Result<Self, AgentError> {
        Ok(Self {
            workspace: WorkspaceRoot::new(workspace_root)?,
            registry: ReadOnlyToolRegistry::default(),
            read_tracker: Arc::new(Mutex::new(FileReadTracker::default())),
        })
    }

    pub fn registry(&self) -> &ReadOnlyToolRegistry {
        &self.registry
    }

    pub fn last_read(
        &self,
        session_id: uuid::Uuid,
        path: &str,
    ) -> Result<Option<FileReadRecord>, AgentError> {
        let file_path = self.workspace.resolve_existing(path)?;
        let relative_path = self.workspace.relative_display(&file_path);
        Ok(self
            .read_tracker
            .lock()
            .expect("read tracker mutex poisoned")
            .last_read(session_id, &relative_path)
            .cloned())
    }

    pub fn validate_recent_read(
        &self,
        session_id: uuid::Uuid,
        path: &str,
    ) -> Result<FileReadRecord, AgentError> {
        let file_path = self.workspace.resolve_existing(path)?;
        if !file_path.is_file() {
            return Err(AgentError::NotFile(
                self.workspace.relative_display(&file_path),
            ));
        }
        let relative_path = self.workspace.relative_display(&file_path);
        let current = file_fingerprint(&file_path)?;
        let record = self
            .read_tracker
            .lock()
            .expect("read tracker mutex poisoned")
            .last_read(session_id, &relative_path)
            .cloned()
            .ok_or_else(|| AgentError::FileNotRead(relative_path.clone()))?;

        if record.fingerprint != current {
            return Err(AgentError::StaleRead(relative_path));
        }

        Ok(record)
    }

    pub fn execute(&self, call: &ToolCall) -> ToolResult {
        let started_at = unix_ms_now();
        let result = match call.tool_name.as_str() {
            LIST_FILES_TOOL => self.list_files(&call.input),
            READ_FILE_TOOL => self.read_file(&call.input, call.session_id),
            SEARCH_FILES_TOOL => self.search_files(&call.input),
            other => Err(AgentError::UnknownTool(other.to_string())),
        };
        let completed_at = unix_ms_now();

        match result {
            Ok(output) => ToolResult::succeeded(call.id, output, started_at, completed_at),
            Err(error) => {
                ToolResult::failed(call.id, error.into_tool_error(), started_at, completed_at)
            }
        }
    }

    pub fn sensitive_read_permission_request(
        &self,
        call: &ToolCall,
    ) -> Result<Option<PermissionRequest>, AgentError> {
        let path = required_string_field(&call.input, "path")?;
        let file_path = self.workspace.resolve_existing(path)?;
        if !file_path.is_file() {
            return Err(AgentError::NotFile(
                self.workspace.relative_display(&file_path),
            ));
        }
        let relative_path = self.workspace.relative_display(&file_path);
        if !path_looks_sensitive(&relative_path) {
            return Ok(None);
        }

        PermissionRequest::new(
            call.session_id,
            call.run_id,
            Some(call.id),
            ToolName::new(READ_FILE_TOOL).expect("built-in tool name is valid"),
            ToolPermission::ReadWorkspace,
            vec![relative_path.clone()],
            ToolRiskLevel::High,
            json!({
                "path": relative_path,
                "reason": "sensitive_file_read",
            }),
            call.requested_at_unix_ms,
        )
        .map(Some)
        .map_err(|error| AgentError::PermissionContract(error.to_string()))
    }

    pub fn execute_with_events(&self, call: &ToolCall) -> ToolExecution {
        let result = self.execute(call);
        let events = vec![
            ReplEvent::ToolStarted {
                name: call.tool_name.to_string(),
            },
            ReplEvent::ToolCompleted {
                name: call.tool_name.to_string(),
                status: tool_status(result.status),
            },
        ];
        ToolExecution { result, events }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn preview_edit(
        &self,
        session_id: uuid::Uuid,
        run_id: uuid::Uuid,
        tool_call_id: Option<uuid::Uuid>,
        path: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Result<EditPreview, AgentError> {
        if old_string.is_empty() {
            return Err(AgentError::InvalidInput(
                "old_string cannot be empty for edit preview".to_string(),
            ));
        }
        if old_string == new_string {
            return Err(AgentError::NoChanges(path.to_string()));
        }

        let read_record = self.validate_recent_read(session_id, path)?;
        let file_path = self.workspace.resolve_existing(path)?;
        let old_content = fs::read_to_string(&file_path).map_err(|source| AgentError::Io {
            path: file_path.display().to_string(),
            source,
        })?;
        let relative_path = self.workspace.relative_display(&file_path);
        let match_count = old_content.matches(old_string).count();
        if match_count == 0 {
            return Err(AgentError::OldStringNotFound(relative_path));
        }
        if match_count > 1 && !replace_all {
            return Err(AgentError::OldStringNotUnique(relative_path));
        }

        let new_content = if replace_all {
            old_content.replace(old_string, new_string)
        } else {
            old_content.replacen(old_string, new_string, 1)
        };
        if old_content == new_content {
            return Err(AgentError::NoChanges(relative_path));
        }

        let (diff, additions, removals) = build_edit_diff(
            &relative_path,
            &old_content,
            &new_content,
            old_string,
            new_string,
        );
        let permission_request = PermissionRequest::new(
            session_id,
            run_id,
            tool_call_id,
            ToolName::new(APPLY_EDIT_TOOL).expect("built-in tool name is valid"),
            ToolPermission::WriteWorkspace,
            vec![relative_path.clone()],
            ToolRiskLevel::High,
            json!({
                "path": relative_path,
                "diff": diff,
                "additions": additions,
                "removals": removals,
                "replace_all": replace_all,
            }),
            unix_ms_now(),
        )
        .map_err(|error| AgentError::PermissionContract(error.to_string()))?;

        Ok(EditPreview {
            path: relative_path,
            old_content,
            new_content,
            old_string: old_string.to_string(),
            new_string: new_string.to_string(),
            replace_all,
            diff,
            additions,
            removals,
            read_record,
            permission_request,
        })
    }

    pub fn apply_approved_edit(
        &self,
        preview: &EditPreview,
        reply: PermissionReply,
    ) -> ToolExecution {
        let started_at = unix_ms_now();
        let call_id = preview
            .permission_request
            .tool_call_id
            .unwrap_or(preview.permission_request.id);

        let result = match reply {
            PermissionReply::Reject => ToolResult::denied(
                call_id,
                AgentError::PermissionRejected(preview.path.clone()).into_tool_error(),
                started_at,
                unix_ms_now(),
            ),
            PermissionReply::Once | PermissionReply::Always => {
                match self.apply_edit_after_approval(preview) {
                    Ok(output) => ToolResult::succeeded(call_id, output, started_at, unix_ms_now()),
                    Err(error) => ToolResult::failed(
                        call_id,
                        error.into_tool_error(),
                        started_at,
                        unix_ms_now(),
                    ),
                }
            }
        };

        let events = vec![
            ReplEvent::ToolStarted {
                name: APPLY_EDIT_TOOL.to_string(),
            },
            ReplEvent::ToolCompleted {
                name: APPLY_EDIT_TOOL.to_string(),
                status: tool_status(result.status),
            },
        ];

        ToolExecution { result, events }
    }

    fn apply_edit_after_approval(&self, preview: &EditPreview) -> Result<ToolOutput, AgentError> {
        let file_path = self.workspace.resolve_existing(&preview.path)?;
        if !file_path.is_file() {
            return Err(AgentError::NotFile(preview.path.clone()));
        }

        let current = file_fingerprint(&file_path)?;
        if current != preview.read_record.fingerprint {
            return Err(AgentError::StaleRead(preview.path.clone()));
        }

        write_file_atomically(&file_path, &preview.new_content)?;
        self.record_read(preview.read_record.session_id, &file_path)?;

        Ok(ToolOutput {
            text: format!("Edit applied: {}", preview.path),
            metadata: json!({
                "path": preview.path,
                "diff": preview.diff,
                "additions": preview.additions,
                "removals": preview.removals,
            }),
            truncated: false,
        })
    }

    fn list_files(&self, input: &Value) -> Result<ToolOutput, AgentError> {
        let path = string_field(input, "path")?.unwrap_or(".");
        let max_entries = usize_field(input, "max_entries")?.unwrap_or(DEFAULT_MAX_LIST_ENTRIES);
        let directory = self.workspace.resolve_existing(path)?;
        if !directory.is_dir() {
            return Err(AgentError::NotDirectory(
                self.workspace.relative_display(&directory),
            ));
        }

        let mut entries = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|source| AgentError::Io {
            path: directory.display().to_string(),
            source,
        })? {
            let entry = entry.map_err(|source| AgentError::Io {
                path: directory.display().to_string(),
                source,
            })?;
            let path = entry.path();
            let kind = if path.is_dir() {
                "directory"
            } else if path.is_file() {
                "file"
            } else {
                "other"
            };
            entries.push(json!({
                "path": self.workspace.relative_display(&path),
                "kind": kind
            }));
        }

        entries.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
        let truncated = entries.len() > max_entries;
        entries.truncate(max_entries);

        let text = entries
            .iter()
            .filter_map(|entry| entry["path"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolOutput {
            text,
            metadata: json!({
                "path": self.workspace.relative_display(&directory),
                "entries": entries
            }),
            truncated,
        })
    }

    fn read_file(&self, input: &Value, session_id: uuid::Uuid) -> Result<ToolOutput, AgentError> {
        let path = required_string_field(input, "path")?;
        let max_bytes = u64_field(input, "max_bytes")?.unwrap_or(DEFAULT_MAX_READ_BYTES);
        let file_path = self.workspace.resolve_existing(path)?;
        if !file_path.is_file() {
            return Err(AgentError::NotFile(
                self.workspace.relative_display(&file_path),
            ));
        }

        let mut file = fs::File::open(&file_path).map_err(|source| AgentError::Io {
            path: file_path.display().to_string(),
            source,
        })?;
        let mut bytes = Vec::new();
        file.by_ref()
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|source| AgentError::Io {
                path: file_path.display().to_string(),
                source,
            })?;
        let truncated = bytes.len() as u64 > max_bytes;
        if truncated {
            bytes.truncate(max_bytes as usize);
        }
        let relative_path = self.workspace.relative_display(&file_path);
        let sensitive = path_looks_sensitive(&relative_path);
        let text = String::from_utf8_lossy(&bytes).to_string();
        let text = if sensitive {
            redact_sensitive_file_text(&text)
        } else {
            text
        };
        self.record_read(session_id, &file_path)?;

        Ok(ToolOutput {
            text,
            metadata: json!({
                "path": relative_path,
                "bytes": bytes.len(),
                "sensitive": sensitive,
            }),
            truncated,
        })
    }

    fn record_read(&self, session_id: uuid::Uuid, file_path: &Path) -> Result<(), AgentError> {
        let relative_path = self.workspace.relative_display(file_path);
        let record = FileReadRecord {
            session_id,
            path: relative_path,
            fingerprint: file_fingerprint(file_path)?,
            read_at_unix_ms: unix_ms_now(),
        };
        self.read_tracker
            .lock()
            .expect("read tracker mutex poisoned")
            .record(record);
        Ok(())
    }

    fn search_files(&self, input: &Value) -> Result<ToolOutput, AgentError> {
        let query = required_string_field(input, "query")?;
        if query.trim().is_empty() {
            return Err(AgentError::InvalidInput(
                "query cannot be empty".to_string(),
            ));
        }
        let path = string_field(input, "path")?.unwrap_or(".");
        let max_matches = usize_field(input, "max_matches")?.unwrap_or(DEFAULT_MAX_SEARCH_MATCHES);
        let root = self.workspace.resolve_existing(path)?;
        let query_lowercase = query.to_lowercase();
        let mut matches = Vec::new();

        self.search_path(&root, &query_lowercase, max_matches, &mut matches)?;

        let truncated = matches.len() >= max_matches;
        let text = matches
            .iter()
            .filter_map(|entry| {
                Some(format!(
                    "{}:{}:{}",
                    entry["path"].as_str()?,
                    entry["line"].as_u64()?,
                    entry["text"].as_str()?
                ))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolOutput {
            text,
            metadata: json!({
                "query": query,
                "path": self.workspace.relative_display(&root),
                "matches": matches,
            }),
            truncated,
        })
    }

    fn search_path(
        &self,
        path: &Path,
        query_lowercase: &str,
        max_matches: usize,
        matches: &mut Vec<Value>,
    ) -> Result<(), AgentError> {
        if matches.len() >= max_matches {
            return Ok(());
        }
        if path.is_dir() {
            if self.workspace.root != path && search_ignored_dir(path) {
                return Ok(());
            }
            let mut children = fs::read_dir(path)
                .map_err(|source| AgentError::Io {
                    path: path.display().to_string(),
                    source,
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|source| AgentError::Io {
                    path: path.display().to_string(),
                    source,
                })?;
            children.sort_by_key(|entry| entry.path());
            for child in children {
                self.search_path(&child.path(), query_lowercase, max_matches, matches)?;
                if matches.len() >= max_matches {
                    break;
                }
            }
            return Ok(());
        }
        if !path.is_file() {
            return Ok(());
        }
        if path_looks_sensitive(&self.workspace.relative_display(path)) {
            return Ok(());
        }
        let metadata = fs::metadata(path).map_err(|source| AgentError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if metadata.len() > MAX_SEARCH_FILE_BYTES {
            return Ok(());
        }
        let Ok(content) = fs::read_to_string(path) else {
            return Ok(());
        };
        for (line_index, line) in content.lines().enumerate() {
            if !line.to_lowercase().contains(query_lowercase) {
                continue;
            }
            matches.push(json!({
                "path": self.workspace.relative_display(path),
                "line": line_index + 1,
                "text": truncate_search_match_text(line.trim()),
            }));
            if matches.len() >= max_matches {
                break;
            }
        }
        Ok(())
    }
}

fn search_ignored_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| SEARCH_IGNORED_DIRS.contains(&name))
        .unwrap_or(false)
}

fn path_looks_sensitive(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase();
    normalized == ".env"
        || normalized.ends_with("/.env")
        || normalized.contains(".env.")
        || normalized == ".npmrc"
        || normalized.ends_with("/.npmrc")
        || normalized == ".netrc"
        || normalized.ends_with("/.netrc")
        || normalized == ".pypirc"
        || normalized.ends_with("/.pypirc")
        || normalized.ends_with("/.aws/credentials")
        || normalized.ends_with("/.ssh/config")
        || normalized.contains("/.ssh/")
        || normalized.contains("/.gnupg/")
        || normalized.contains("credential")
        || normalized.contains("secret")
        || normalized.contains("token")
        || normalized.ends_with(".pem")
        || normalized.ends_with(".key")
        || normalized.ends_with(".p12")
        || normalized.ends_with(".pfx")
        || normalized.ends_with("id_rsa")
        || normalized.ends_with("id_ed25519")
}

fn redact_sensitive_file_text(text: &str) -> String {
    text.lines()
        .map(redact_sensitive_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_sensitive_line(line: &str) -> String {
    let lowercase = line.to_ascii_lowercase();
    if !SENSITIVE_KEY_MARKERS
        .iter()
        .any(|marker| lowercase.contains(marker))
    {
        return line.to_string();
    }

    if let Some((left, _)) = line.split_once('=') {
        return format!("{}=[REDACTED]", left.trim_end());
    }
    if let Some((left, _)) = line.split_once(':') {
        return format!("{}: [REDACTED]", left.trim_end());
    }
    "[REDACTED]".to_string()
}

fn truncate_search_match_text(text: &str) -> String {
    if text.chars().count() <= MAX_SEARCH_MATCH_TEXT_CHARS {
        return text.to_string();
    }
    let mut truncated = text
        .chars()
        .take(MAX_SEARCH_MATCH_TEXT_CHARS.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileFingerprint {
    pub len: u64,
    pub modified_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReadRecord {
    pub session_id: uuid::Uuid,
    pub path: String,
    pub fingerprint: FileFingerprint,
    pub read_at_unix_ms: u64,
}

#[derive(Debug, Default)]
pub struct FileReadTracker {
    records: HashMap<(uuid::Uuid, String), FileReadRecord>,
}

impl FileReadTracker {
    pub fn record(&mut self, record: FileReadRecord) {
        self.records
            .insert((record.session_id, record.path.clone()), record);
    }

    pub fn last_read(&self, session_id: uuid::Uuid, path: &str) -> Option<&FileReadRecord> {
        self.records.get(&(session_id, path.to_string()))
    }
}

fn file_fingerprint(path: &Path) -> Result<FileFingerprint, AgentError> {
    let metadata = fs::metadata(path).map_err(|source| AgentError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let modified_at_unix_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    Ok(FileFingerprint {
        len: metadata.len(),
        modified_at_unix_ms,
    })
}

fn write_file_atomically(path: &Path, content: &str) -> Result<(), AgentError> {
    let parent = path.parent().ok_or_else(|| {
        AgentError::InvalidInput(format!("path has no parent: {}", path.display()))
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            AgentError::InvalidInput(format!("invalid file name: {}", path.display()))
        })?;
    let temp_path = parent.join(format!(".{file_name}.coddy-tmp-{}", uuid::Uuid::new_v4()));

    fs::write(&temp_path, content).map_err(|source| AgentError::Io {
        path: temp_path.display().to_string(),
        source,
    })?;

    if let Err(source) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(AgentError::Io {
            path: path.display().to_string(),
            source,
        });
    }

    Ok(())
}

fn build_edit_diff(
    path: &str,
    old_content: &str,
    new_content: &str,
    old_string: &str,
    new_string: &str,
) -> (String, usize, usize) {
    let additions = new_string
        .lines()
        .count()
        .max(usize::from(!new_string.is_empty()));
    let removals = old_string
        .lines()
        .count()
        .max(usize::from(!old_string.is_empty()));
    let old_lines = old_content.lines().count().max(1);
    let new_lines = new_content.lines().count().max(1);
    let mut diff = format!("--- a/{path}\n+++ b/{path}\n@@ -1,{old_lines} +1,{new_lines} @@\n");
    for line in old_content.lines() {
        diff.push('-');
        diff.push_str(line);
        diff.push('\n');
    }
    for line in new_content.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    (diff, additions, removals)
}

fn tool_status(status: ToolResultStatus) -> ToolStatus {
    match status {
        ToolResultStatus::Succeeded => ToolStatus::Succeeded,
        ToolResultStatus::Failed => ToolStatus::Failed,
        ToolResultStatus::Cancelled => ToolStatus::Cancelled,
        ToolResultStatus::Denied => ToolStatus::Denied,
    }
}

fn tool_definition(
    name: &'static str,
    description: &'static str,
    input_schema: Value,
) -> ToolDefinition {
    local_tool_definition(LocalToolDefinitionSpec {
        name,
        description,
        category: ToolCategory::Filesystem,
        input_schema,
        output_schema: ToolSchema::empty_object().schema,
        risk_level: ToolRiskLevel::Low,
        permissions: vec![ToolPermission::ReadWorkspace],
        timeout_ms: 5_000,
        approval_policy: ApprovalPolicy::AutoApprove,
    })
}

fn local_tool_definition(spec: LocalToolDefinitionSpec) -> ToolDefinition {
    ToolDefinition::new(
        ToolName::new(spec.name).expect("built-in tool names are valid"),
        spec.description,
        spec.category,
        ToolSchema::new(spec.input_schema),
        ToolSchema::new(spec.output_schema),
        spec.risk_level,
        spec.permissions,
        spec.timeout_ms,
        spec.approval_policy,
    )
    .expect("built-in tool definitions are valid")
}

fn required_string_field<'a>(input: &'a Value, key: &str) -> Result<&'a str, AgentError> {
    string_field(input, key)?.ok_or_else(|| AgentError::InvalidInput(format!("{key} is required")))
}

fn string_field<'a>(input: &'a Value, key: &str) -> Result<Option<&'a str>, AgentError> {
    match input.get(key) {
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(AgentError::InvalidInput(format!("{key} must be a string"))),
        None => Ok(None),
    }
}

fn usize_field(input: &Value, key: &str) -> Result<Option<usize>, AgentError> {
    match input.get(key) {
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(|| AgentError::InvalidInput(format!("{key} must be a positive integer"))),
        Some(_) => Err(AgentError::InvalidInput(format!(
            "{key} must be an integer"
        ))),
        None => Ok(None),
    }
}

fn u64_field(input: &Value, key: &str) -> Result<Option<u64>, AgentError> {
    match input.get(key) {
        Some(Value::Number(value)) => value
            .as_u64()
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(|| AgentError::InvalidInput(format!("{key} must be a positive integer"))),
        Some(_) => Err(AgentError::InvalidInput(format!(
            "{key} must be an integer"
        ))),
        None => Ok(None),
    }
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("coddy-agent-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp workspace");
            Self { path }
        }

        fn write(&self, relative_path: &str, content: &str) {
            let path = self.path.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent directory");
            }
            fs::write(path, content).expect("write fixture file");
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn call(tool_name: &str, input: Value) -> ToolCall {
        call_for_session(Uuid::new_v4(), tool_name, input)
    }

    fn call_for_session(session_id: Uuid, tool_name: &str, input: Value) -> ToolCall {
        ToolCall::new(
            session_id,
            Uuid::new_v4(),
            ToolName::new(tool_name).expect("valid tool name"),
            input,
            1_775_000_000_000,
        )
    }

    #[test]
    fn registry_defines_only_read_only_low_risk_tools() {
        let registry = ReadOnlyToolRegistry::default();

        assert_eq!(registry.definitions().len(), 3);
        for definition in registry.definitions() {
            assert_eq!(definition.risk_level, ToolRiskLevel::Low);
            assert_eq!(definition.permissions, vec![ToolPermission::ReadWorkspace]);
            assert_eq!(definition.approval_policy, ApprovalPolicy::AutoApprove);
        }
    }

    #[test]
    fn agent_registry_defines_local_contracts_without_execution() {
        let registry = AgentToolRegistry::default();

        assert_eq!(registry.definitions().len(), 11);
        assert!(registry
            .get(&ToolName::new(LIST_FILES_TOOL).expect("tool name"))
            .is_some());
        assert_eq!(
            registry
                .get(&ToolName::new(APPLY_EDIT_TOOL).expect("tool name"))
                .expect("apply edit definition")
                .approval_policy,
            ApprovalPolicy::AlwaysAsk
        );
        let shell = registry
            .get(&ToolName::new(SHELL_RUN_TOOL).expect("tool name"))
            .expect("shell definition");
        assert_eq!(shell.category, ToolCategory::Shell);
        assert_eq!(shell.permissions, vec![ToolPermission::ExecuteCommand]);
        assert_eq!(shell.timeout_ms, DEFAULT_SHELL_TIMEOUT_MS);
        assert_eq!(shell.approval_policy, ApprovalPolicy::AskOnUse);

        let subagent_list = registry
            .get(&ToolName::new(SUBAGENT_LIST_TOOL).expect("tool name"))
            .expect("subagent list definition");
        assert_eq!(subagent_list.category, ToolCategory::Subagent);
        assert_eq!(subagent_list.risk_level, ToolRiskLevel::Low);
        assert_eq!(subagent_list.approval_policy, ApprovalPolicy::AutoApprove);
        assert_eq!(
            subagent_list.permissions,
            vec![ToolPermission::DelegateSubagent]
        );

        let subagent_prepare = registry
            .get(&ToolName::new(SUBAGENT_PREPARE_TOOL).expect("tool name"))
            .expect("subagent prepare definition");
        assert_eq!(subagent_prepare.category, ToolCategory::Subagent);
        assert_eq!(subagent_prepare.risk_level, ToolRiskLevel::Low);
        assert_eq!(
            subagent_prepare.approval_policy,
            ApprovalPolicy::AutoApprove
        );
        assert_eq!(
            subagent_prepare.permissions,
            vec![ToolPermission::DelegateSubagent]
        );

        let subagent_route = registry
            .get(&ToolName::new(SUBAGENT_ROUTE_TOOL).expect("tool name"))
            .expect("subagent route definition");
        assert_eq!(subagent_route.category, ToolCategory::Subagent);
        assert_eq!(subagent_route.risk_level, ToolRiskLevel::Low);
        assert_eq!(subagent_route.approval_policy, ApprovalPolicy::AutoApprove);
        assert_eq!(
            subagent_route.permissions,
            vec![ToolPermission::DelegateSubagent]
        );

        let subagent_team_plan = registry
            .get(&ToolName::new(SUBAGENT_TEAM_PLAN_TOOL).expect("tool name"))
            .expect("subagent team plan definition");
        assert_eq!(subagent_team_plan.category, ToolCategory::Subagent);
        assert_eq!(subagent_team_plan.risk_level, ToolRiskLevel::Low);
        assert_eq!(
            subagent_team_plan.approval_policy,
            ApprovalPolicy::AutoApprove
        );
        assert_eq!(
            subagent_team_plan.permissions,
            vec![ToolPermission::DelegateSubagent]
        );

        let subagent_reduce_outputs = registry
            .get(&ToolName::new(SUBAGENT_REDUCE_OUTPUTS_TOOL).expect("tool name"))
            .expect("subagent reduce outputs definition");
        assert_eq!(subagent_reduce_outputs.category, ToolCategory::Subagent);
        assert_eq!(subagent_reduce_outputs.risk_level, ToolRiskLevel::Low);
        assert_eq!(
            subagent_reduce_outputs.approval_policy,
            ApprovalPolicy::AutoApprove
        );
        assert_eq!(
            subagent_reduce_outputs.permissions,
            vec![ToolPermission::DelegateSubagent]
        );
    }

    #[test]
    fn default_subagent_registry_defines_required_roles_safely() {
        let registry = SubagentRegistry::default();

        let names = registry
            .definitions()
            .iter()
            .map(|definition| definition.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "explorer",
                "planner",
                "coder",
                "reviewer",
                "security-reviewer",
                "test-writer",
                "eval-runner",
                "docs-writer",
            ]
        );

        let explorer = registry.get("explorer").expect("explorer definition");
        assert_eq!(explorer.mode, SubagentMode::ReadOnly);
        assert!(explorer.allowed_tools.contains(&READ_FILE_TOOL.to_string()));
        assert!(explorer
            .allowed_tools
            .contains(&LIST_FILES_TOOL.to_string()));
        assert!(!explorer
            .allowed_tools
            .contains(&PREVIEW_EDIT_TOOL.to_string()));
        assert!(!explorer.allowed_tools.contains(&SHELL_RUN_TOOL.to_string()));
    }

    #[test]
    fn list_files_returns_workspace_relative_entries() {
        let workspace = TempWorkspace::new();
        workspace.write("src/lib.rs", "pub fn coddy() {}\n");
        workspace.write("README.md", "# Coddy\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let result = executor.execute(&call(LIST_FILES_TOOL, json!({ "path": "." })));

        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let output = result.output.expect("tool output");
        assert!(output.text.contains("README.md"));
        assert!(output.text.contains("src"));
        assert!(!output.truncated);
    }

    #[test]
    fn read_file_blocks_path_traversal() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let result = executor.execute(&call(READ_FILE_TOOL, json!({ "path": "../secret.txt" })));

        assert_eq!(result.status, ToolResultStatus::Failed);
        assert_eq!(
            result.error.expect("tool error").code,
            "path_traversal".to_string()
        );
    }

    #[test]
    fn read_file_returns_text_and_metadata() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let result = executor.execute(&call(READ_FILE_TOOL, json!({ "path": "docs/repl.md" })));

        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let output = result.output.expect("tool output");
        assert_eq!(output.text, "Coddy REPL\n");
        assert_eq!(output.metadata["path"], "docs/repl.md");
        assert_eq!(output.metadata["sensitive"], json!(false));
    }

    #[test]
    fn read_file_redacts_sensitive_workspace_files() {
        let workspace = TempWorkspace::new();
        workspace.write(
            ".env",
            "GOOGLE_API_KEY=super-secret-token\nPUBLIC_FLAG=true\n",
        );
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let result = executor.execute(&call(READ_FILE_TOOL, json!({ "path": ".env" })));

        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let output = result.output.expect("tool output");
        assert_eq!(output.text, "GOOGLE_API_KEY=[REDACTED]\nPUBLIC_FLAG=true");
        assert_eq!(output.metadata["path"], ".env");
        assert_eq!(output.metadata["sensitive"], json!(true));
        assert!(!output.text.contains("super-secret-token"));
    }

    #[test]
    fn read_file_redacts_common_credential_files() {
        let workspace = TempWorkspace::new();
        workspace.write(
            ".npmrc",
            "//registry.npmjs.org/:_authToken=npm-secret-token\n",
        );
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let result = executor.execute(&call(READ_FILE_TOOL, json!({ "path": ".npmrc" })));

        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let output = result.output.expect("tool output");
        assert_eq!(output.text, "//registry.npmjs.org/:_authToken=[REDACTED]");
        assert_eq!(output.metadata["sensitive"], json!(true));
        assert!(!output.text.contains("npm-secret-token"));
    }

    #[test]
    fn read_file_records_last_read_for_session() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");
        let session_id = Uuid::new_v4();

        let result = executor.execute(&call_for_session(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "docs/repl.md" }),
        ));

        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let record = executor
            .last_read(session_id, "docs/repl.md")
            .expect("last read lookup")
            .expect("recorded read");
        assert_eq!(record.path, "docs/repl.md");
        assert_eq!(
            executor
                .validate_recent_read(session_id, "docs/repl.md")
                .expect("recent read")
                .path,
            "docs/repl.md"
        );
    }

    #[test]
    fn validate_recent_read_requires_prior_read() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let error = executor
            .validate_recent_read(Uuid::new_v4(), "docs/repl.md")
            .expect_err("read should be required before edit");

        assert!(matches!(error, AgentError::FileNotRead(path) if path == "docs/repl.md"));
    }

    #[test]
    fn validate_recent_read_detects_modified_file() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");
        let session_id = Uuid::new_v4();

        let result = executor.execute(&call_for_session(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "docs/repl.md" }),
        ));
        assert_eq!(result.status, ToolResultStatus::Succeeded);

        workspace.write("docs/repl.md", "Coddy REPL changed\n");
        let error = executor
            .validate_recent_read(session_id, "docs/repl.md")
            .expect_err("modified file should require another read");

        assert!(matches!(error, AgentError::StaleRead(path) if path == "docs/repl.md"));
    }

    #[test]
    fn preview_edit_requires_prior_read() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let error = executor
            .preview_edit(
                Uuid::new_v4(),
                Uuid::new_v4(),
                None,
                "docs/repl.md",
                "Coddy",
                "Coddy Agent",
                false,
            )
            .expect_err("preview requires read-before-edit");

        assert!(matches!(error, AgentError::FileNotRead(path) if path == "docs/repl.md"));
    }

    #[test]
    fn preview_edit_detects_stale_read() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");
        let session_id = Uuid::new_v4();

        let result = executor.execute(&call_for_session(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "docs/repl.md" }),
        ));
        assert_eq!(result.status, ToolResultStatus::Succeeded);

        workspace.write("docs/repl.md", "Coddy REPL changed\n");
        let error = executor
            .preview_edit(
                session_id,
                Uuid::new_v4(),
                None,
                "docs/repl.md",
                "Coddy",
                "Coddy Agent",
                false,
            )
            .expect_err("stale file should require another read");

        assert!(matches!(error, AgentError::StaleRead(path) if path == "docs/repl.md"));
    }

    #[test]
    fn preview_edit_creates_permission_request_without_writing_file() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");
        let session_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let tool_call_id = Uuid::new_v4();

        let result = executor.execute(&call_for_session(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "docs/repl.md" }),
        ));
        assert_eq!(result.status, ToolResultStatus::Succeeded);

        let preview = executor
            .preview_edit(
                session_id,
                run_id,
                Some(tool_call_id),
                "docs/repl.md",
                "Coddy REPL",
                "Coddy Agent REPL",
                false,
            )
            .expect("edit preview");

        assert_eq!(preview.path, "docs/repl.md");
        assert_eq!(preview.old_content, "Coddy REPL\n");
        assert_eq!(preview.new_content, "Coddy Agent REPL\n");
        assert_eq!(preview.additions, 1);
        assert_eq!(preview.removals, 1);
        assert!(preview.diff.contains("--- a/docs/repl.md"));
        assert!(preview.diff.contains("-Coddy REPL"));
        assert!(preview.diff.contains("+Coddy Agent REPL"));
        assert_eq!(
            preview.permission_request.permission,
            ToolPermission::WriteWorkspace
        );
        assert_eq!(preview.permission_request.risk_level, ToolRiskLevel::High);
        assert_eq!(
            preview.permission_request.tool_name.as_str(),
            APPLY_EDIT_TOOL
        );
        assert_eq!(preview.permission_request.patterns, vec!["docs/repl.md"]);
        assert_eq!(preview.permission_request.run_id, run_id);
        assert_eq!(preview.permission_request.tool_call_id, Some(tool_call_id));
        assert_eq!(
            preview.permission_request.metadata["path"],
            json!("docs/repl.md")
        );
        assert_eq!(
            fs::read_to_string(workspace.path.join("docs/repl.md")).expect("read fixture"),
            "Coddy REPL\n"
        );
    }

    #[test]
    fn preview_edit_rejects_ambiguous_replacements_without_replace_all() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\nCoddy CLI\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");
        let session_id = Uuid::new_v4();

        let result = executor.execute(&call_for_session(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "docs/repl.md" }),
        ));
        assert_eq!(result.status, ToolResultStatus::Succeeded);

        let error = executor
            .preview_edit(
                session_id,
                Uuid::new_v4(),
                None,
                "docs/repl.md",
                "Coddy",
                "Coddy Agent",
                false,
            )
            .expect_err("ambiguous replacement should be rejected");

        assert!(matches!(error, AgentError::OldStringNotUnique(path) if path == "docs/repl.md"));
    }

    #[test]
    fn apply_approved_edit_rejects_without_writing() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");
        let session_id = Uuid::new_v4();

        let result = executor.execute(&call_for_session(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "docs/repl.md" }),
        ));
        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let preview = executor
            .preview_edit(
                session_id,
                Uuid::new_v4(),
                Some(Uuid::new_v4()),
                "docs/repl.md",
                "Coddy REPL",
                "Coddy Agent REPL",
                false,
            )
            .expect("edit preview");

        let execution = executor.apply_approved_edit(&preview, PermissionReply::Reject);

        assert_eq!(execution.result.status, ToolResultStatus::Denied);
        assert_eq!(
            execution.result.error.expect("denied error").code,
            "permission_rejected"
        );
        assert_eq!(
            fs::read_to_string(workspace.path.join("docs/repl.md")).expect("read fixture"),
            "Coddy REPL\n"
        );
        assert_eq!(
            execution.events,
            vec![
                ReplEvent::ToolStarted {
                    name: APPLY_EDIT_TOOL.to_string()
                },
                ReplEvent::ToolCompleted {
                    name: APPLY_EDIT_TOOL.to_string(),
                    status: ToolStatus::Denied
                }
            ]
        );
    }

    #[test]
    fn apply_approved_edit_revalidates_stale_file_before_writing() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");
        let session_id = Uuid::new_v4();

        let result = executor.execute(&call_for_session(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "docs/repl.md" }),
        ));
        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let preview = executor
            .preview_edit(
                session_id,
                Uuid::new_v4(),
                Some(Uuid::new_v4()),
                "docs/repl.md",
                "Coddy REPL",
                "Coddy Agent REPL",
                false,
            )
            .expect("edit preview");

        workspace.write("docs/repl.md", "Coddy REPL changed by user\n");
        let execution = executor.apply_approved_edit(&preview, PermissionReply::Once);

        assert_eq!(execution.result.status, ToolResultStatus::Failed);
        assert_eq!(
            execution.result.error.expect("stale error").code,
            "stale_read"
        );
        assert_eq!(
            fs::read_to_string(workspace.path.join("docs/repl.md")).expect("read fixture"),
            "Coddy REPL changed by user\n"
        );
    }

    #[test]
    fn apply_approved_edit_writes_file_and_records_new_read() {
        let workspace = TempWorkspace::new();
        workspace.write("docs/repl.md", "Coddy REPL\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");
        let session_id = Uuid::new_v4();
        let tool_call_id = Uuid::new_v4();

        let result = executor.execute(&call_for_session(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "docs/repl.md" }),
        ));
        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let preview = executor
            .preview_edit(
                session_id,
                Uuid::new_v4(),
                Some(tool_call_id),
                "docs/repl.md",
                "Coddy REPL",
                "Coddy Agent REPL",
                false,
            )
            .expect("edit preview");

        let execution = executor.apply_approved_edit(&preview, PermissionReply::Once);

        assert_eq!(execution.result.call_id, tool_call_id);
        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("edit output");
        assert_eq!(output.metadata["path"], json!("docs/repl.md"));
        assert_eq!(output.metadata["additions"], json!(1));
        assert_eq!(output.metadata["removals"], json!(1));
        assert_eq!(
            fs::read_to_string(workspace.path.join("docs/repl.md")).expect("read fixture"),
            "Coddy Agent REPL\n"
        );
        assert_eq!(
            executor
                .validate_recent_read(session_id, "docs/repl.md")
                .expect("fresh read after apply")
                .path,
            "docs/repl.md"
        );
        assert_eq!(
            execution.events,
            vec![
                ReplEvent::ToolStarted {
                    name: APPLY_EDIT_TOOL.to_string()
                },
                ReplEvent::ToolCompleted {
                    name: APPLY_EDIT_TOOL.to_string(),
                    status: ToolStatus::Succeeded
                }
            ]
        );
    }

    #[test]
    fn search_files_finds_text_matches() {
        let workspace = TempWorkspace::new();
        workspace.write("src/lib.rs", "pub struct AgentRuntime;\n");
        workspace.write("docs/readme.md", "agent runtime notes\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let result = executor.execute(&call(
            SEARCH_FILES_TOOL,
            json!({ "query": "runtime", "path": "." }),
        ));

        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let output = result.output.expect("tool output");
        assert!(output
            .text
            .contains("src/lib.rs:1:pub struct AgentRuntime;"));
        assert!(output.text.contains("docs/readme.md:1:agent runtime notes"));
    }

    #[test]
    fn search_files_skips_generated_directories() {
        let workspace = TempWorkspace::new();
        workspace.write("src/lib.rs", "pub struct AgentRuntime;\n");
        workspace.write("target/debug/build.rs", "generated AgentRuntime\n");
        workspace.write("apps/web/dist/index.js", "bundled AgentRuntime\n");
        workspace.write(".git/logs/HEAD", "commit AgentRuntime\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let result = executor.execute(&call(
            SEARCH_FILES_TOOL,
            json!({ "query": "AgentRuntime", "path": ".", "max_matches": 20 }),
        ));

        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let output = result.output.expect("tool output");
        assert!(output
            .text
            .contains("src/lib.rs:1:pub struct AgentRuntime;"));
        assert!(!output.text.contains("target/debug/build.rs"));
        assert!(!output.text.contains("apps/web/dist/index.js"));
        assert!(!output.text.contains(".git/logs/HEAD"));
    }

    #[test]
    fn search_files_skips_sensitive_files() {
        let workspace = TempWorkspace::new();
        workspace.write(".env", "GOOGLE_API_KEY=super-secret-token\n");
        workspace.write("src/lib.rs", "pub const LABEL: &str = \"safe\";\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let result = executor.execute(&call(
            SEARCH_FILES_TOOL,
            json!({ "query": "super-secret-token", "path": ".", "max_matches": 20 }),
        ));

        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let output = result.output.expect("tool output");
        assert!(output.text.is_empty());
        assert!(!output.text.contains(".env"));
        assert!(!output.text.contains("super-secret-token"));
    }

    #[test]
    fn search_files_truncates_long_match_lines() {
        let workspace = TempWorkspace::new();
        workspace.write("src/lib.rs", &format!("match {}\n", "x".repeat(400)));
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let result = executor.execute(&call(
            SEARCH_FILES_TOOL,
            json!({ "query": "match", "path": "." }),
        ));

        assert_eq!(result.status, ToolResultStatus::Succeeded);
        let output = result.output.expect("tool output");
        let line = output.text.lines().next().expect("match line");
        assert!(line.ends_with("..."));
        assert!(line.len() < 280);
    }

    #[test]
    fn execute_with_events_reports_tool_lifecycle_for_ui() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let executor = ReadOnlyToolExecutor::new(&workspace.path).expect("executor");

        let execution =
            executor.execute_with_events(&call(READ_FILE_TOOL, json!({ "path": "README.md" })));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        assert_eq!(
            execution.events,
            vec![
                ReplEvent::ToolStarted {
                    name: READ_FILE_TOOL.to_string()
                },
                ReplEvent::ToolCompleted {
                    name: READ_FILE_TOOL.to_string(),
                    status: ToolStatus::Succeeded
                }
            ]
        );
    }
}
