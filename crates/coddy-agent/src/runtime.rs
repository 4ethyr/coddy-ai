use std::path::Path;

use coddy_core::{
    redact_conversation_text, PermissionReply, ReplEvent, ToolCall, ToolExecutionRecord, ToolName,
    ToolResult, ToolResultStatus, ToolStatus,
};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    AgentError, LocalToolRouteOutcome, LocalToolRouter, ShellExecutionConfig, WorkspaceRoot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunStatus {
    Planned,
    Running,
    AwaitingApproval,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStepKind {
    Plan,
    ToolCall,
    PermissionReply,
    Observation,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStepStatus {
    Pending,
    Running,
    AwaitingApproval,
    Succeeded,
    Failed,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub id: Uuid,
    pub description: String,
    pub tool_name: Option<ToolName>,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStep {
    pub id: Uuid,
    pub kind: AgentStepKind,
    pub status: AgentStepStatus,
    pub summary: String,
    pub tool_name: Option<ToolName>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub tool_name: ToolName,
    pub status: ToolResultStatus,
    pub text: String,
    pub metadata: Value,
    pub error_code: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunState {
    pub session_id: Uuid,
    pub run_id: Uuid,
    pub goal: String,
    pub status: AgentRunStatus,
    pub plan: Vec<PlanItem>,
    pub steps: Vec<AgentStep>,
    pub observations: Vec<Observation>,
    pub events: Vec<ReplEvent>,
}

#[derive(Debug, Clone)]
pub struct LocalAgentRuntime {
    workspace: WorkspaceRoot,
    router: LocalToolRouter,
}

impl LocalAgentRuntime {
    pub fn new(workspace_root: impl AsRef<Path>) -> Result<Self, AgentError> {
        Self::with_shell_config(workspace_root, ShellExecutionConfig::default())
    }

    pub fn with_shell_config(
        workspace_root: impl AsRef<Path>,
        shell_config: ShellExecutionConfig,
    ) -> Result<Self, AgentError> {
        let workspace = WorkspaceRoot::new(workspace_root.as_ref())?;
        let router = LocalToolRouter::with_shell_config(workspace.path(), shell_config)?;
        Ok(Self { workspace, router })
    }

    pub fn workspace(&self) -> &WorkspaceRoot {
        &self.workspace
    }

    pub fn router(&self) -> &LocalToolRouter {
        &self.router
    }

    pub fn start_run(&self, session_id: Uuid, goal: impl Into<String>) -> RunState {
        self.start_run_with_id(session_id, Uuid::new_v4(), goal)
    }

    pub fn start_run_with_id(
        &self,
        session_id: Uuid,
        run_id: Uuid,
        goal: impl Into<String>,
    ) -> RunState {
        RunState {
            session_id,
            run_id,
            goal: goal.into(),
            status: AgentRunStatus::Planned,
            plan: Vec::new(),
            steps: Vec::new(),
            observations: Vec::new(),
            events: vec![ReplEvent::RunStarted { run_id }],
        }
    }

    pub fn add_plan_item(
        &self,
        state: &mut RunState,
        description: impl Into<String>,
        tool_name: Option<ToolName>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let description = description.into();
        state.plan.push(PlanItem {
            id,
            description: description.clone(),
            tool_name: tool_name.clone(),
            completed: false,
        });
        state.steps.push(AgentStep {
            id: Uuid::new_v4(),
            kind: AgentStepKind::Plan,
            status: AgentStepStatus::Pending,
            summary: description,
            tool_name,
        });
        id
    }

    pub fn execute_tool_call(
        &self,
        state: &mut RunState,
        call: &ToolCall,
    ) -> LocalToolRouteOutcome {
        state.status = AgentRunStatus::Running;
        let mut outcome = self.router.route(call);
        append_tool_execution_record_event(&call.tool_name, &mut outcome);
        record_outcome(
            state,
            call.tool_name.clone(),
            AgentStepKind::ToolCall,
            &outcome,
        );
        outcome
    }

    pub fn reply_permission(
        &self,
        state: &mut RunState,
        request_id: Uuid,
        reply: PermissionReply,
    ) -> LocalToolRouteOutcome {
        state.status = AgentRunStatus::Running;
        let mut outcome = self.router.reply_permission(request_id, reply);
        let tool_name = outcome
            .result
            .as_ref()
            .and_then(|_| infer_tool_from_events(&outcome.events))
            .unwrap_or_else(|| ToolName::new("permission.reply").expect("tool name"));
        append_tool_execution_record_event(&tool_name, &mut outcome);
        record_outcome(state, tool_name, AgentStepKind::PermissionReply, &outcome);
        outcome
    }

    pub fn complete_run(&self, state: &mut RunState) {
        state.status = AgentRunStatus::Completed;
        state.events.push(ReplEvent::RunCompleted {
            run_id: state.run_id,
        });
    }
}

fn record_outcome(
    state: &mut RunState,
    tool_name: ToolName,
    kind: AgentStepKind,
    outcome: &LocalToolRouteOutcome,
) {
    state.events.extend(outcome.events.clone());

    let status = match outcome.result.as_ref().map(|result| result.status) {
        Some(ToolResultStatus::Succeeded) => AgentStepStatus::Succeeded,
        Some(ToolResultStatus::Failed) => AgentStepStatus::Failed,
        Some(ToolResultStatus::Cancelled) => AgentStepStatus::Failed,
        Some(ToolResultStatus::Denied) => AgentStepStatus::Denied,
        None if outcome.permission_request.is_some() => AgentStepStatus::AwaitingApproval,
        None => AgentStepStatus::Running,
    };

    if let Some(result) = &outcome.result {
        state
            .observations
            .push(observation_from_result(tool_name.clone(), result));
    }

    state.steps.push(AgentStep {
        id: Uuid::new_v4(),
        kind,
        status,
        summary: step_summary(&tool_name, status),
        tool_name: Some(tool_name),
    });

    state.status = match status {
        AgentStepStatus::AwaitingApproval => AgentRunStatus::AwaitingApproval,
        AgentStepStatus::Failed | AgentStepStatus::Denied => AgentRunStatus::Failed,
        _ => AgentRunStatus::Running,
    };
}

fn append_tool_execution_record_event(tool_name: &ToolName, outcome: &mut LocalToolRouteOutcome) {
    if let Some(result) = &outcome.result {
        outcome.events.push(ReplEvent::ToolExecutionRecorded {
            record: tool_execution_record(tool_name, result),
        });
    }
}

fn observation_from_result(tool_name: ToolName, result: &ToolResult) -> Observation {
    let (text, metadata, truncated) = result
        .output
        .as_ref()
        .map(|output| {
            (
                output.text.clone(),
                output.metadata.clone(),
                output.truncated,
            )
        })
        .unwrap_or_else(|| (String::new(), Value::Object(Default::default()), false));
    let error_code = result.error.as_ref().map(|error| error.code.clone());
    let text = if text.is_empty() {
        result
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_default()
    } else {
        text
    };

    Observation {
        tool_name,
        status: result.status,
        text,
        metadata,
        error_code,
        truncated,
    }
}

fn tool_execution_record(tool_name: &ToolName, result: &ToolResult) -> ToolExecutionRecord {
    let (output_chars, truncated, metadata) = result
        .output
        .as_ref()
        .map(|output| {
            (
                output.text.chars().count(),
                output.truncated,
                audit_metadata(&output.metadata),
            )
        })
        .unwrap_or_else(|| (0, false, Value::Object(Map::new())));

    ToolExecutionRecord {
        tool_name: tool_name.to_string(),
        call_id: result.call_id,
        status: tool_status(result.status),
        started_at_unix_ms: result.started_at_unix_ms,
        completed_at_unix_ms: result.completed_at_unix_ms,
        duration_ms: result
            .completed_at_unix_ms
            .saturating_sub(result.started_at_unix_ms),
        output_chars,
        truncated,
        error_code: result.error.as_ref().map(|error| error.code.clone()),
        retryable: result.error.as_ref().map(|error| error.retryable),
        metadata,
    }
}

fn tool_status(status: ToolResultStatus) -> ToolStatus {
    match status {
        ToolResultStatus::Succeeded => ToolStatus::Succeeded,
        ToolResultStatus::Failed => ToolStatus::Failed,
        ToolResultStatus::Cancelled => ToolStatus::Cancelled,
        ToolResultStatus::Denied => ToolStatus::Denied,
    }
}

fn audit_metadata(metadata: &Value) -> Value {
    sanitize_audit_metadata_value(metadata, 0)
}

fn sanitize_audit_metadata_value(value: &Value, depth: usize) -> Value {
    const MAX_DEPTH: usize = 4;
    const MAX_ARRAY_ITEMS: usize = 25;

    if depth >= MAX_DEPTH {
        return Value::String("[truncated]".to_string());
    }

    match value {
        Value::String(text) => Value::String(redact_and_truncate(text)),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .take(MAX_ARRAY_ITEMS)
                .map(|value| sanitize_audit_metadata_value(value, depth + 1))
                .collect(),
        ),
        Value::Object(map) => {
            let mut sanitized = Map::new();
            for (key, value) in map {
                if audit_metadata_key_is_omitted(key) {
                    continue;
                }
                sanitized.insert(key.clone(), sanitize_audit_metadata_value(value, depth + 1));
            }
            Value::Object(sanitized)
        }
        _ => value.clone(),
    }
}

fn audit_metadata_key_is_omitted(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "body"
            | "content"
            | "diff"
            | "failures"
            | "handoff"
            | "matches"
            | "output"
            | "outputs"
            | "raw"
            | "raw_failures"
            | "recommendations"
            | "response"
            | "stderr"
            | "stdout"
            | "subagents"
            | "team"
            | "text"
    )
}

fn redact_and_truncate(text: &str) -> String {
    const MAX_CHARS: usize = 512;

    let redacted = redact_conversation_text(text);
    if redacted.chars().count() <= MAX_CHARS {
        return redacted;
    }

    let mut truncated = redacted.chars().take(MAX_CHARS).collect::<String>();
    truncated.push_str("...[truncated]");
    truncated
}

fn step_summary(tool_name: &ToolName, status: AgentStepStatus) -> String {
    format!("{}: {status:?}", tool_name.as_str())
}

fn infer_tool_from_events(events: &[ReplEvent]) -> Option<ToolName> {
    events.iter().find_map(|event| match event {
        ReplEvent::ToolStarted { name } | ReplEvent::ToolCompleted { name, .. } => {
            ToolName::new(name.clone()).ok()
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use coddy_core::ToolResultStatus;
    use serde_json::json;

    use crate::{
        ShellSandboxPolicy, ShellSandboxProviderDiscovery, READ_FILE_TOOL, SHELL_RUN_TOOL,
    };

    use super::*;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("coddy-runtime-test-{}", Uuid::new_v4()));
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

    fn call(state: &RunState, tool_name: &str, input: Value) -> ToolCall {
        ToolCall {
            id: Uuid::new_v4(),
            session_id: state.session_id,
            run_id: state.run_id,
            tool_name: ToolName::new(tool_name).expect("tool name"),
            input,
            requested_at_unix_ms: 1_775_000_000_000,
        }
    }

    #[test]
    fn records_tool_observations_and_events() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let runtime = LocalAgentRuntime::new(&workspace.path).expect("runtime");
        let mut state = runtime.start_run(Uuid::new_v4(), "read project docs");

        runtime.add_plan_item(
            &mut state,
            "Read README",
            Some(ToolName::new(READ_FILE_TOOL).expect("tool name")),
        );
        let read_call = call(&state, READ_FILE_TOOL, json!({ "path": "README.md" }));
        let outcome = runtime.execute_tool_call(&mut state, &read_call);

        assert_eq!(outcome.status(), Some(ToolResultStatus::Succeeded));
        assert_eq!(state.status, AgentRunStatus::Running);
        assert_eq!(state.observations.len(), 1);
        assert_eq!(state.observations[0].text, "# Coddy\n");
        assert!(state
            .events
            .iter()
            .any(|event| matches!(event, ReplEvent::ToolCompleted { .. })));
        let record = state
            .events
            .iter()
            .find_map(|event| match event {
                ReplEvent::ToolExecutionRecorded { record }
                    if record.tool_name == READ_FILE_TOOL =>
                {
                    Some(record)
                }
                _ => None,
            })
            .expect("read file execution record");
        assert_eq!(record.call_id, read_call.id);
        assert_eq!(record.status, ToolStatus::Succeeded);
        assert!(record.output_chars > 0);
        assert_eq!(record.metadata["path"], json!("README.md"));
        assert!(record.metadata.get("stdout").is_none());
    }

    #[test]
    fn waits_for_shell_permission_and_records_reply_execution() {
        let workspace = TempWorkspace::new();
        let runtime = LocalAgentRuntime::new(&workspace.path).expect("runtime");
        let mut state = runtime.start_run(Uuid::new_v4(), "run shell command");

        let shell_call = call(&state, SHELL_RUN_TOOL, json!({ "command": "printf coddy" }));
        let pending = runtime.execute_tool_call(&mut state, &shell_call);

        assert_eq!(state.status, AgentRunStatus::AwaitingApproval);
        assert!(pending.result.is_none());
        let request = pending.permission_request.expect("permission request");

        let executed = runtime.reply_permission(&mut state, request.id, PermissionReply::Once);

        assert_eq!(executed.status(), Some(ToolResultStatus::Succeeded));
        assert_eq!(state.status, AgentRunStatus::Running);
        assert_eq!(
            state.observations.last().expect("observation").metadata["stdout"],
            json!("coddy")
        );
        let record = state
            .events
            .iter()
            .rev()
            .find_map(|event| match event {
                ReplEvent::ToolExecutionRecorded { record }
                    if record.tool_name == SHELL_RUN_TOOL =>
                {
                    Some(record)
                }
                _ => None,
            })
            .expect("shell execution record");
        assert_eq!(record.status, ToolStatus::Succeeded);
        assert_eq!(record.metadata["command"], json!("printf coddy"));
        assert_eq!(record.metadata["success"], json!(true));
        assert_eq!(record.metadata["sandbox_policy"], json!("process"));
        assert_eq!(
            record.metadata["sandbox"]["no_new_privileges"],
            json!(cfg!(target_os = "linux"))
        );
        assert_eq!(record.metadata["sandbox"]["policy"], json!("process"));
        assert_eq!(
            record.metadata["sandbox"]["providers"]["selected"],
            json!("process")
        );
        assert_eq!(
            record.metadata["sandbox"]["providers"]["kernel_isolation_active"],
            json!(false)
        );
        assert!(record.metadata.get("stdout").is_none());
        assert!(record.metadata.get("stderr").is_none());
    }

    #[test]
    fn shell_network_policy_denial_is_recorded_after_approval() {
        let workspace = TempWorkspace::new();
        let runtime = LocalAgentRuntime::new(&workspace.path).expect("runtime");
        let mut state = runtime.start_run(Uuid::new_v4(), "check network command policy");

        let shell_call = call(
            &state,
            SHELL_RUN_TOOL,
            json!({ "command": "curl --version" }),
        );
        let pending = runtime.execute_tool_call(&mut state, &shell_call);
        let request = pending.permission_request.expect("permission request");

        let executed = runtime.reply_permission(&mut state, request.id, PermissionReply::Once);

        assert_eq!(executed.status(), Some(ToolResultStatus::Denied));
        assert_eq!(state.status, AgentRunStatus::Failed);
        let observation = state.observations.last().expect("observation");
        assert_eq!(observation.tool_name.as_str(), SHELL_RUN_TOOL);
        assert_eq!(observation.status, ToolResultStatus::Denied);
        assert_eq!(observation.error_code.as_deref(), Some("network_disabled"));
        let record = state
            .events
            .iter()
            .rev()
            .find_map(|event| match event {
                ReplEvent::ToolExecutionRecorded { record }
                    if record.tool_name == SHELL_RUN_TOOL =>
                {
                    Some(record)
                }
                _ => None,
            })
            .expect("shell execution record");
        assert_eq!(record.status, ToolStatus::Denied);
        assert_eq!(record.error_code.as_deref(), Some("network_disabled"));
        assert_eq!(record.metadata["policy"], json!("network_disabled"));
        assert_eq!(record.metadata["command"], json!("curl --version"));
        assert_eq!(record.metadata["cwd"], json!("."));
        assert_eq!(record.metadata["network_policy"], json!("disabled"));
        assert_eq!(record.metadata["sandbox_policy"], json!("process"));
        assert_eq!(
            record.metadata["sandbox"]["no_new_privileges"],
            json!(cfg!(target_os = "linux"))
        );
        assert_eq!(record.metadata["sandbox"]["policy"], json!("process"));
        assert_eq!(
            record.metadata["sandbox"]["providers"]["selected"],
            json!("process")
        );
        assert_eq!(
            record.metadata["sandbox"]["providers"]["kernel_isolation_active"],
            json!(false)
        );
        assert!(record.metadata.get("stdout").is_none());
        assert!(record.metadata.get("stderr").is_none());
    }

    #[test]
    fn strict_shell_sandbox_config_is_enforced_through_local_runtime() {
        let workspace = TempWorkspace::new();
        let runtime = LocalAgentRuntime::with_shell_config(
            &workspace.path,
            ShellExecutionConfig {
                sandbox_policy: ShellSandboxPolicy::RequireKernelIsolation,
                sandbox_provider_discovery: ShellSandboxProviderDiscovery::Disabled,
                ..ShellExecutionConfig::default()
            },
        )
        .expect("runtime");
        let mut state = runtime.start_run(Uuid::new_v4(), "run strict shell command");

        let shell_call = call(&state, SHELL_RUN_TOOL, json!({ "command": "printf coddy" }));
        let pending = runtime.execute_tool_call(&mut state, &shell_call);
        let request = pending.permission_request.expect("permission request");

        let executed = runtime.reply_permission(&mut state, request.id, PermissionReply::Once);

        assert_eq!(executed.status(), Some(ToolResultStatus::Denied));
        assert_eq!(state.status, AgentRunStatus::Failed);
        let observation = state.observations.last().expect("observation");
        assert_eq!(
            observation.error_code.as_deref(),
            Some("sandbox_unavailable")
        );
        let record = state
            .events
            .iter()
            .rev()
            .find_map(|event| match event {
                ReplEvent::ToolExecutionRecorded { record }
                    if record.tool_name == SHELL_RUN_TOOL =>
                {
                    Some(record)
                }
                _ => None,
            })
            .expect("shell execution record");
        assert_eq!(record.status, ToolStatus::Denied);
        assert_eq!(record.error_code.as_deref(), Some("sandbox_unavailable"));
        assert_eq!(
            record.metadata["sandbox_policy"],
            json!("require-kernel-isolation")
        );
        assert_eq!(
            record.metadata["sandbox"]["providers"]["kernel_isolation_active"],
            json!(false)
        );
        assert!(record.metadata.get("stdout").is_none());
        assert!(record.metadata.get("stderr").is_none());
    }

    #[test]
    fn audit_metadata_omits_raw_output_and_redacts_strings() {
        let metadata = audit_metadata(&json!({
            "command": "OPENAI_API_KEY=sk-secret cargo test",
            "stdout": "token=sk-secret",
            "stderr": "password=secret",
            "path": "src/main.rs",
            "nested": {
                "diff": "-old\n+new",
                "safe": "Bearer sk-secret"
            }
        }));

        assert_eq!(metadata["path"], json!("src/main.rs"));
        assert!(metadata.get("stdout").is_none());
        assert!(metadata.get("stderr").is_none());
        assert!(metadata["nested"].get("diff").is_none());
        assert!(!metadata.to_string().contains("sk-secret"));
    }

    #[test]
    fn complete_run_emits_run_completed_event() {
        let workspace = TempWorkspace::new();
        let runtime = LocalAgentRuntime::new(&workspace.path).expect("runtime");
        let mut state = runtime.start_run(Uuid::new_v4(), "finish task");

        runtime.complete_run(&mut state);

        assert_eq!(state.status, AgentRunStatus::Completed);
        assert!(matches!(
            state.events.last(),
            Some(ReplEvent::RunCompleted { run_id }) if *run_id == state.run_id
        ));
    }

    #[test]
    fn start_run_with_id_preserves_external_run_id() {
        let workspace = TempWorkspace::new();
        let runtime = LocalAgentRuntime::new(&workspace.path).expect("runtime");
        let session_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();

        let state = runtime.start_run_with_id(session_id, run_id, "correlate ui run");

        assert_eq!(state.session_id, session_id);
        assert_eq!(state.run_id, run_id);
        assert!(matches!(
            state.events.first(),
            Some(ReplEvent::RunStarted { run_id: event_run_id }) if *event_run_id == run_id
        ));
    }
}
