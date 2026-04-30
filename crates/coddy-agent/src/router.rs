use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use coddy_core::{
    PermissionReply, PermissionRequest, ReplEvent, ToolCall, ToolDefinition, ToolError, ToolOutput,
    ToolResult, ToolResultStatus, ToolStatus,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    AgentError, AgentToolRegistry, EditPreview, ReadOnlyToolExecutor, ShellApprovalState,
    ShellExecutionConfig, ShellExecutor, ShellPlan, ShellPlanRequest, ShellPlanner, SubagentMode,
    SubagentRegistry, ToolExecution, APPLY_EDIT_TOOL, LIST_FILES_TOOL, PREVIEW_EDIT_TOOL,
    READ_FILE_TOOL, SEARCH_FILES_TOOL, SHELL_RUN_TOOL, SUBAGENT_LIST_TOOL, SUBAGENT_ROUTE_TOOL,
};

#[derive(Debug, Clone)]
pub struct LocalToolRouteOutcome {
    pub result: Option<ToolResult>,
    pub events: Vec<ReplEvent>,
    pub permission_request: Option<PermissionRequest>,
}

#[derive(Debug, Clone)]
pub struct LocalToolRouter {
    registry: AgentToolRegistry,
    filesystem: ReadOnlyToolExecutor,
    shell_planner: ShellPlanner,
    shell_executor: ShellExecutor,
    subagents: SubagentRegistry,
    pending_edits: Arc<Mutex<HashMap<Uuid, EditPreview>>>,
    pending_shells: Arc<Mutex<HashMap<Uuid, ShellPlan>>>,
}

impl LocalToolRouter {
    pub fn new(workspace_root: impl AsRef<Path>) -> Result<Self, AgentError> {
        Self::with_shell_config(workspace_root, ShellExecutionConfig::default())
    }

    pub fn with_shell_config(
        workspace_root: impl AsRef<Path>,
        shell_config: ShellExecutionConfig,
    ) -> Result<Self, AgentError> {
        let workspace_root = workspace_root.as_ref();
        Ok(Self {
            registry: AgentToolRegistry::default(),
            filesystem: ReadOnlyToolExecutor::new(workspace_root)?,
            shell_planner: ShellPlanner::new(workspace_root)?,
            shell_executor: ShellExecutor::with_config(workspace_root, shell_config)?,
            subagents: SubagentRegistry::default(),
            pending_edits: Arc::new(Mutex::new(HashMap::new())),
            pending_shells: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn registry(&self) -> &AgentToolRegistry {
        &self.registry
    }

    pub fn pending_edit_count(&self) -> usize {
        self.pending_edits
            .lock()
            .expect("pending edits mutex poisoned")
            .len()
    }

    pub fn pending_shell_count(&self) -> usize {
        self.pending_shells
            .lock()
            .expect("pending shells mutex poisoned")
            .len()
    }

    pub fn route(&self, call: &ToolCall) -> LocalToolRouteOutcome {
        let Some(definition) = self.registry.get(&call.tool_name) else {
            return failed_outcome(
                call.id,
                call.tool_name.to_string(),
                AgentError::UnknownTool(call.tool_name.to_string()).into_tool_error(),
            );
        };
        if let Err(error) = validate_tool_input(definition, &call.input) {
            return failed_outcome(call.id, call.tool_name.to_string(), error.into_tool_error());
        }

        match call.tool_name.as_str() {
            LIST_FILES_TOOL | READ_FILE_TOOL | SEARCH_FILES_TOOL => {
                LocalToolRouteOutcome::from_execution(self.filesystem.execute_with_events(call))
            }
            PREVIEW_EDIT_TOOL => self.preview_edit(call),
            APPLY_EDIT_TOOL => self.apply_permission_reply_tool(call),
            SHELL_RUN_TOOL => self.plan_or_execute_shell(call),
            SUBAGENT_LIST_TOOL => self.list_subagents(call),
            SUBAGENT_ROUTE_TOOL => self.route_subagent(call),
            other => failed_outcome(
                call.id,
                call.tool_name.to_string(),
                AgentError::UnknownTool(other.to_string()).into_tool_error(),
            ),
        }
    }

    pub fn reply_permission(
        &self,
        request_id: Uuid,
        reply: PermissionReply,
    ) -> LocalToolRouteOutcome {
        let reply_event = ReplEvent::PermissionReplied { request_id, reply };

        if let Some(preview) = self
            .pending_edits
            .lock()
            .expect("pending edits mutex poisoned")
            .remove(&request_id)
        {
            let execution = self.filesystem.apply_approved_edit(&preview, reply);
            return LocalToolRouteOutcome::from_execution_with_prefix(execution, vec![reply_event]);
        }

        if let Some(plan) = self
            .pending_shells
            .lock()
            .expect("pending shells mutex poisoned")
            .remove(&request_id)
        {
            let execution = self.shell_executor.execute(&plan, Some(reply));
            return LocalToolRouteOutcome::from_execution_with_prefix(execution, vec![reply_event]);
        }

        LocalToolRouteOutcome {
            result: Some(ToolResult::failed(
                request_id,
                ToolError::new(
                    "permission_request_not_found",
                    format!("pending permission request not found: {request_id}"),
                    false,
                ),
                unix_ms_now(),
                unix_ms_now(),
            )),
            events: vec![
                reply_event,
                ReplEvent::Error {
                    code: "permission_request_not_found".to_string(),
                    message: format!("pending permission request not found: {request_id}"),
                },
            ],
            permission_request: None,
        }
    }

    fn preview_edit(&self, call: &ToolCall) -> LocalToolRouteOutcome {
        let started_at = unix_ms_now();
        let result = preview_edit_input(&call.input).and_then(|input| {
            self.filesystem.preview_edit(
                call.session_id,
                call.run_id,
                Some(call.id),
                input.path,
                input.old_string,
                input.new_string,
                input.replace_all,
            )
        });
        let completed_at = unix_ms_now();

        match result {
            Ok(preview) => {
                let permission_request = preview.permission_request.clone();
                self.pending_edits
                    .lock()
                    .expect("pending edits mutex poisoned")
                    .insert(permission_request.id, preview.clone());

                let output = ToolOutput {
                    text: preview.diff.clone(),
                    metadata: json!({
                        "path": preview.path,
                        "diff": preview.diff,
                        "additions": preview.additions,
                        "removals": preview.removals,
                        "permission_request_id": permission_request.id,
                    }),
                    truncated: false,
                };
                let result = ToolResult::succeeded(call.id, output, started_at, completed_at);
                LocalToolRouteOutcome {
                    result: Some(result),
                    events: vec![
                        ReplEvent::ToolStarted {
                            name: PREVIEW_EDIT_TOOL.to_string(),
                        },
                        ReplEvent::ToolCompleted {
                            name: PREVIEW_EDIT_TOOL.to_string(),
                            status: ToolStatus::Succeeded,
                        },
                        ReplEvent::PermissionRequested {
                            request: permission_request.clone(),
                        },
                    ],
                    permission_request: Some(permission_request),
                }
            }
            Err(error) => failed_outcome(
                call.id,
                PREVIEW_EDIT_TOOL.to_string(),
                error.into_tool_error(),
            ),
        }
    }

    fn apply_permission_reply_tool(&self, call: &ToolCall) -> LocalToolRouteOutcome {
        match apply_input(&call.input) {
            Ok(input) => self.reply_permission(input.permission_request_id, input.reply),
            Err(error) => failed_outcome(
                call.id,
                APPLY_EDIT_TOOL.to_string(),
                error.into_tool_error(),
            ),
        }
    }

    fn plan_or_execute_shell(&self, call: &ToolCall) -> LocalToolRouteOutcome {
        let plan = shell_input(&call.input).and_then(|input| {
            self.shell_planner.plan(ShellPlanRequest {
                session_id: call.session_id,
                run_id: call.run_id,
                tool_call_id: Some(call.id),
                command: input.command.to_string(),
                description: input.description.map(ToOwned::to_owned),
                cwd: input.cwd.map(ToOwned::to_owned),
                timeout_ms: input.timeout_ms,
                requested_at_unix_ms: call.requested_at_unix_ms,
            })
        });

        let plan = match plan {
            Ok(plan) => plan,
            Err(error) => {
                return failed_outcome(
                    call.id,
                    SHELL_RUN_TOOL.to_string(),
                    error.into_tool_error(),
                );
            }
        };

        match &plan.approval_state {
            ShellApprovalState::Pending(permission_request) => {
                self.pending_shells
                    .lock()
                    .expect("pending shells mutex poisoned")
                    .insert(permission_request.id, plan.clone());
                LocalToolRouteOutcome {
                    result: None,
                    events: plan.events.clone(),
                    permission_request: Some(permission_request.clone()),
                }
            }
            ShellApprovalState::NotRequired | ShellApprovalState::Blocked(_) => {
                LocalToolRouteOutcome::from_execution(self.shell_executor.execute(&plan, None))
            }
        }
    }

    fn list_subagents(&self, call: &ToolCall) -> LocalToolRouteOutcome {
        let started_at = unix_ms_now();
        let mode = match optional_subagent_mode(&call.input) {
            Ok(mode) => mode,
            Err(error) => {
                return failed_outcome(
                    call.id,
                    SUBAGENT_LIST_TOOL.to_string(),
                    error.into_tool_error(),
                );
            }
        };
        let subagents = self.subagents.public_definitions(mode);
        let names = subagents
            .iter()
            .filter_map(|metadata| metadata["name"].as_str())
            .collect::<Vec<_>>();
        let text = if names.is_empty() {
            "No subagents match the requested filter.".to_string()
        } else {
            names.join("\n")
        };
        let output = ToolOutput {
            text,
            metadata: json!({
                "mode": mode.map(SubagentMode::as_str),
                "subagents": subagents,
            }),
            truncated: false,
        };
        let result = ToolResult::succeeded(call.id, output, started_at, unix_ms_now());

        LocalToolRouteOutcome::from_execution(ToolExecution {
            result,
            events: vec![
                ReplEvent::ToolStarted {
                    name: SUBAGENT_LIST_TOOL.to_string(),
                },
                ReplEvent::ToolCompleted {
                    name: SUBAGENT_LIST_TOOL.to_string(),
                    status: ToolStatus::Succeeded,
                },
            ],
        })
    }

    fn route_subagent(&self, call: &ToolCall) -> LocalToolRouteOutcome {
        let started_at = unix_ms_now();
        let mode = match optional_subagent_mode(&call.input) {
            Ok(mode) => mode,
            Err(error) => {
                return failed_outcome(
                    call.id,
                    SUBAGENT_ROUTE_TOOL.to_string(),
                    error.into_tool_error(),
                );
            }
        };
        let goal = call.input["goal"].as_str().unwrap_or_default();
        let limit = optional_usize_field(&call.input, "limit").unwrap_or(3);
        let recommendations = self.subagents.recommend(goal, mode, limit);
        let recommendation_metadata = recommendations
            .iter()
            .map(|recommendation| recommendation.public_metadata())
            .collect::<Vec<_>>();
        let text = if recommendations.is_empty() {
            "No subagent recommendation available.".to_string()
        } else {
            recommendations
                .iter()
                .map(|recommendation| format!("{} ({})", recommendation.name, recommendation.score))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let output = ToolOutput {
            text,
            metadata: json!({
                "goal": goal,
                "mode": mode.map(SubagentMode::as_str),
                "recommendations": recommendation_metadata,
            }),
            truncated: false,
        };
        let result = ToolResult::succeeded(call.id, output, started_at, unix_ms_now());

        LocalToolRouteOutcome::from_execution(ToolExecution {
            result,
            events: vec![
                ReplEvent::ToolStarted {
                    name: SUBAGENT_ROUTE_TOOL.to_string(),
                },
                ReplEvent::ToolCompleted {
                    name: SUBAGENT_ROUTE_TOOL.to_string(),
                    status: ToolStatus::Succeeded,
                },
            ],
        })
    }
}

impl LocalToolRouteOutcome {
    pub fn from_execution(execution: ToolExecution) -> Self {
        Self {
            result: Some(execution.result),
            events: execution.events,
            permission_request: None,
        }
    }

    pub fn from_execution_with_prefix(
        execution: ToolExecution,
        mut prefix: Vec<ReplEvent>,
    ) -> Self {
        prefix.extend(execution.events);
        Self {
            result: Some(execution.result),
            events: prefix,
            permission_request: None,
        }
    }

    pub fn status(&self) -> Option<ToolResultStatus> {
        self.result.as_ref().map(|result| result.status)
    }
}

struct PreviewEditInput<'a> {
    path: &'a str,
    old_string: &'a str,
    new_string: &'a str,
    replace_all: bool,
}

struct ApplyInput {
    permission_request_id: Uuid,
    reply: PermissionReply,
}

struct ShellInput<'a> {
    command: &'a str,
    description: Option<&'a str>,
    cwd: Option<&'a str>,
    timeout_ms: Option<u64>,
}

fn preview_edit_input(input: &Value) -> Result<PreviewEditInput<'_>, AgentError> {
    Ok(PreviewEditInput {
        path: required_string_field(input, "path")?,
        old_string: required_string_field(input, "old_string")?,
        new_string: required_string_field(input, "new_string")?,
        replace_all: bool_field(input, "replace_all")?.unwrap_or(false),
    })
}

fn apply_input(input: &Value) -> Result<ApplyInput, AgentError> {
    let request_id = required_string_field(input, "permission_request_id")?;
    let permission_request_id = Uuid::parse_str(request_id).map_err(|error| {
        AgentError::InvalidInput(format!("permission_request_id must be a UUID: {error}"))
    })?;
    let reply = match required_string_field(input, "reply")? {
        "once" => PermissionReply::Once,
        "always" => PermissionReply::Always,
        "reject" => PermissionReply::Reject,
        other => {
            return Err(AgentError::InvalidInput(format!(
                "reply must be once, always or reject: {other}"
            )));
        }
    };
    Ok(ApplyInput {
        permission_request_id,
        reply,
    })
}

fn shell_input(input: &Value) -> Result<ShellInput<'_>, AgentError> {
    Ok(ShellInput {
        command: required_string_field(input, "command")?,
        description: string_field(input, "description")?,
        cwd: string_field(input, "cwd")?,
        timeout_ms: u64_field(input, "timeout_ms")?,
    })
}

fn optional_subagent_mode(input: &Value) -> Result<Option<SubagentMode>, AgentError> {
    let Some(mode) = string_field(input, "mode")? else {
        return Ok(None);
    };

    SubagentMode::parse(mode)
        .map(Some)
        .ok_or_else(|| AgentError::InvalidInput(format!("unsupported subagent mode: {mode}")))
}

fn optional_usize_field(input: &Value, field: &str) -> Option<usize> {
    input
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn validate_tool_input(definition: &ToolDefinition, input: &Value) -> Result<(), AgentError> {
    let object = input.as_object().ok_or_else(|| {
        AgentError::InvalidInput(format!("{} input must be an object", definition.name))
    })?;
    let schema = &definition.input_schema.schema;
    let properties = schema.get("properties").and_then(Value::as_object);

    if schema
        .get("additionalProperties")
        .and_then(Value::as_bool)
        .is_some_and(|allowed| !allowed)
    {
        let Some(properties) = properties else {
            if object.is_empty() {
                return Ok(());
            }
            return Err(AgentError::InvalidInput(format!(
                "{} does not accept input fields",
                definition.name
            )));
        };

        if let Some(key) = object.keys().find(|key| !properties.contains_key(*key)) {
            return Err(AgentError::InvalidInput(format!(
                "{} does not accept input field `{key}`",
                definition.name
            )));
        }
    }

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(field) {
                return Err(AgentError::InvalidInput(format!(
                    "{} requires input field `{field}`",
                    definition.name
                )));
            }
        }
    }

    if let Some(properties) = properties {
        for (key, value) in object {
            if let Some(property_schema) = properties.get(key) {
                validate_tool_property(definition.name.as_str(), key, property_schema, value)?;
            }
        }
    }

    Ok(())
}

fn validate_tool_property(
    tool_name: &str,
    key: &str,
    schema: &Value,
    value: &Value,
) -> Result<(), AgentError> {
    if let Some(allowed_values) = schema.get("enum").and_then(Value::as_array) {
        if !allowed_values.iter().any(|allowed| allowed == value) {
            return Err(AgentError::InvalidInput(format!(
                "{tool_name}.{key} must be one of the allowed enum values"
            )));
        }
    }

    if let Some(types) = json_schema_types(schema) {
        if !types
            .iter()
            .any(|schema_type| value_matches_schema_type(value, schema_type))
        {
            return Err(AgentError::InvalidInput(format!(
                "{tool_name}.{key} must be {}, got {}",
                types.join(" or "),
                json_value_type(value)
            )));
        }
    }

    if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64) {
        let Some(value) = value.as_f64() else {
            return Ok(());
        };
        if value < minimum {
            return Err(AgentError::InvalidInput(format!(
                "{tool_name}.{key} must be greater than or equal to {minimum}"
            )));
        }
    }

    Ok(())
}

fn json_schema_types(schema: &Value) -> Option<Vec<String>> {
    match schema.get("type") {
        Some(Value::String(value)) => Some(vec![value.clone()]),
        Some(Value::Array(values)) => Some(
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect(),
        ),
        _ => None,
    }
}

fn value_matches_schema_type(value: &Value, schema_type: &str) -> bool {
    match schema_type {
        "array" => value.is_array(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "object" => value.is_object(),
        "string" => value.is_string(),
        _ => true,
    }
}

fn json_value_type(value: &Value) -> &'static str {
    match value {
        Value::Array(_) => "array",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::Object(_) => "object",
        Value::String(_) => "string",
    }
}

fn failed_outcome(call_id: Uuid, tool_name: String, error: ToolError) -> LocalToolRouteOutcome {
    let now = unix_ms_now();
    let result = ToolResult::failed(call_id, error, now, now);
    LocalToolRouteOutcome {
        result: Some(result),
        events: vec![
            ReplEvent::ToolStarted {
                name: tool_name.clone(),
            },
            ReplEvent::ToolCompleted {
                name: tool_name,
                status: ToolStatus::Failed,
            },
        ],
        permission_request: None,
    }
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

fn bool_field(input: &Value, key: &str) -> Result<Option<bool>, AgentError> {
    match input.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(AgentError::InvalidInput(format!("{key} must be a boolean"))),
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
    use std::{fs, path::PathBuf};

    use coddy_core::{ToolName, ToolResultStatus};

    use super::*;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("coddy-router-test-{}", Uuid::new_v4()));
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

        fn read(&self, relative_path: &str) -> String {
            fs::read_to_string(self.path.join(relative_path)).expect("read fixture file")
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn call(session_id: Uuid, tool_name: &str, input: Value) -> ToolCall {
        ToolCall {
            id: Uuid::new_v4(),
            session_id,
            run_id: Uuid::new_v4(),
            tool_name: ToolName::new(tool_name).expect("tool name"),
            input,
            requested_at_unix_ms: 1_775_000_000_000,
        }
    }

    #[test]
    fn routes_read_only_tools_through_filesystem_executor() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let router = LocalToolRouter::new(&workspace.path).expect("router");

        let outcome = router.route(&call(
            Uuid::new_v4(),
            READ_FILE_TOOL,
            json!({ "path": "README.md" }),
        ));

        assert_eq!(outcome.status(), Some(ToolResultStatus::Succeeded));
        assert_eq!(
            outcome.result.expect("result").output.expect("output").text,
            "# Coddy\n"
        );
    }

    #[test]
    fn rejects_unknown_input_fields_before_tool_execution() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let router = LocalToolRouter::new(&workspace.path).expect("router");

        let outcome = router.route(&call(
            Uuid::new_v4(),
            READ_FILE_TOOL,
            json!({
                "path": "README.md",
                "absolute_path": "/tmp/README.md"
            }),
        ));

        assert_eq!(outcome.status(), Some(ToolResultStatus::Failed));
        let error = outcome.result.expect("result").error.expect("error");
        assert_eq!(error.code, "invalid_input");
        assert!(error
            .message
            .contains("filesystem.read_file does not accept input field `absolute_path`"));
    }

    #[test]
    fn rejects_wrong_input_types_before_tool_execution() {
        let workspace = TempWorkspace::new();
        let router = LocalToolRouter::new(&workspace.path).expect("router");

        let outcome = router.route(&call(
            Uuid::new_v4(),
            SHELL_RUN_TOOL,
            json!({ "command": ["printf", "coddy"] }),
        ));

        assert_eq!(outcome.status(), Some(ToolResultStatus::Failed));
        let error = outcome.result.expect("result").error.expect("error");
        assert_eq!(error.code, "invalid_input");
        assert!(error
            .message
            .contains("shell.run.command must be string, got array"));
        assert_eq!(router.pending_shell_count(), 0);
    }

    #[test]
    fn routes_subagent_list_as_read_only_metadata_tool() {
        let workspace = TempWorkspace::new();
        let router = LocalToolRouter::new(&workspace.path).expect("router");

        let outcome = router.route(&call(
            Uuid::new_v4(),
            SUBAGENT_LIST_TOOL,
            json!({ "mode": "read-only" }),
        ));

        assert_eq!(outcome.status(), Some(ToolResultStatus::Succeeded));
        let output = outcome.result.expect("result").output.expect("output");
        assert!(output.text.contains("explorer"));
        assert!(output.text.contains("security-reviewer"));
        assert!(!output.text.contains("coder"));
        assert_eq!(output.metadata["mode"], json!("read-only"));
        assert_eq!(output.metadata["subagents"][0]["name"], json!("explorer"));
        assert_eq!(
            output.metadata["subagents"][0]["allowedTools"],
            json!([
                "filesystem.list_files",
                "filesystem.read_file",
                "filesystem.search_files"
            ])
        );
    }

    #[test]
    fn routes_subagent_recommendations_as_scored_metadata() {
        let workspace = TempWorkspace::new();
        let router = LocalToolRouter::new(&workspace.path).expect("router");

        let outcome = router.route(&call(
            Uuid::new_v4(),
            SUBAGENT_ROUTE_TOOL,
            json!({
                "goal": "run eval baseline score and regression harness for integrations",
                "limit": 2
            }),
        ));

        assert_eq!(outcome.status(), Some(ToolResultStatus::Succeeded));
        let output = outcome.result.expect("result").output.expect("output");
        let recommendations = output.metadata["recommendations"]
            .as_array()
            .expect("recommendations");
        assert_eq!(recommendations.len(), 2);
        assert_eq!(recommendations[0]["name"], json!("eval-runner"));
        assert!(recommendations[0]["score"].as_u64().expect("score") >= 60);
        assert!(recommendations[0]["matchedSignals"]
            .as_array()
            .expect("matched signals")
            .contains(&json!("harness")));
    }

    #[test]
    fn preview_edit_stores_pending_permission_and_reply_applies_it() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "Coddy REPL\n");
        let router = LocalToolRouter::new(&workspace.path).expect("router");
        let session_id = Uuid::new_v4();

        let read = router.route(&call(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "README.md" }),
        ));
        assert_eq!(read.status(), Some(ToolResultStatus::Succeeded));

        let preview = router.route(&call(
            session_id,
            PREVIEW_EDIT_TOOL,
            json!({
                "path": "README.md",
                "old_string": "Coddy",
                "new_string": "Coddy Agent"
            }),
        ));

        assert_eq!(preview.status(), Some(ToolResultStatus::Succeeded));
        let request = preview.permission_request.expect("permission request");
        assert_eq!(router.pending_edit_count(), 1);
        assert!(preview
            .events
            .iter()
            .any(|event| matches!(event, ReplEvent::PermissionRequested { .. })));

        let applied = router.reply_permission(request.id, PermissionReply::Once);

        assert_eq!(applied.status(), Some(ToolResultStatus::Succeeded));
        assert_eq!(workspace.read("README.md"), "Coddy Agent REPL\n");
        assert_eq!(router.pending_edit_count(), 0);
        assert!(matches!(
            applied.events.first(),
            Some(ReplEvent::PermissionReplied { request_id, reply })
                if *request_id == request.id && *reply == PermissionReply::Once
        ));
    }

    #[test]
    fn apply_tool_can_reject_pending_edit() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "Coddy REPL\n");
        let router = LocalToolRouter::new(&workspace.path).expect("router");
        let session_id = Uuid::new_v4();

        router.route(&call(
            session_id,
            READ_FILE_TOOL,
            json!({ "path": "README.md" }),
        ));
        let preview = router.route(&call(
            session_id,
            PREVIEW_EDIT_TOOL,
            json!({
                "path": "README.md",
                "old_string": "Coddy",
                "new_string": "Coddy Agent"
            }),
        ));
        let request = preview.permission_request.expect("permission request");

        let rejected = router.route(&call(
            session_id,
            APPLY_EDIT_TOOL,
            json!({
                "permission_request_id": request.id.to_string(),
                "reply": "reject"
            }),
        ));

        assert_eq!(rejected.status(), Some(ToolResultStatus::Denied));
        assert_eq!(workspace.read("README.md"), "Coddy REPL\n");
        assert_eq!(router.pending_edit_count(), 0);
    }

    #[test]
    fn shell_requires_approval_then_executes_on_reply() {
        let workspace = TempWorkspace::new();
        let router = LocalToolRouter::new(&workspace.path).expect("router");
        let session_id = Uuid::new_v4();

        let pending = router.route(&call(
            session_id,
            SHELL_RUN_TOOL,
            json!({ "command": "printf coddy" }),
        ));

        assert!(pending.result.is_none());
        let request = pending.permission_request.expect("permission request");
        assert_eq!(router.pending_shell_count(), 1);

        let executed = router.reply_permission(request.id, PermissionReply::Once);

        assert_eq!(executed.status(), Some(ToolResultStatus::Succeeded));
        assert_eq!(
            executed
                .result
                .expect("result")
                .output
                .expect("output")
                .metadata["stdout"],
            json!("coddy")
        );
        assert_eq!(router.pending_shell_count(), 0);
    }

    #[test]
    fn blocked_shell_returns_denied_without_pending_permission() {
        let workspace = TempWorkspace::new();
        let router = LocalToolRouter::new(&workspace.path).expect("router");

        let outcome = router.route(&call(
            Uuid::new_v4(),
            SHELL_RUN_TOOL,
            json!({ "command": "rm -rf target" }),
        ));

        assert_eq!(outcome.status(), Some(ToolResultStatus::Denied));
        assert!(outcome.permission_request.is_none());
        assert_eq!(router.pending_shell_count(), 0);
    }
}
