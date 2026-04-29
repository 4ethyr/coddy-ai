use coddy_core::{ApprovalPolicy, ToolCategory, ToolPermission, ToolResultStatus, ToolRiskLevel};
use serde_json::Value;

use crate::{AgentRunStatus, AgentToolRegistry, RunState, WorkspaceRoot};

const MAX_OBSERVATION_TEXT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct ContextSnapshot {
    pub workspace_root: String,
    pub goal: String,
    pub run_status: AgentRunStatus,
    pub plan: Vec<ContextPlanItem>,
    pub observations: Vec<ContextObservation>,
    pub available_tools: Vec<ContextTool>,
    pub event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPlanItem {
    pub description: String,
    pub tool_name: Option<String>,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextObservation {
    pub tool_name: String,
    pub status: ToolResultStatus,
    pub text: String,
    pub metadata: Value,
    pub error_code: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextTool {
    pub name: String,
    pub category: ToolCategory,
    pub risk_level: ToolRiskLevel,
    pub permissions: Vec<ToolPermission>,
    pub timeout_ms: u64,
    pub approval_policy: ApprovalPolicy,
}

impl ContextSnapshot {
    pub fn from_runtime_parts(
        workspace: &WorkspaceRoot,
        registry: &AgentToolRegistry,
        state: &RunState,
    ) -> Self {
        Self {
            workspace_root: workspace.path().display().to_string(),
            goal: state.goal.clone(),
            run_status: state.status,
            plan: state
                .plan
                .iter()
                .map(|item| ContextPlanItem {
                    description: item.description.clone(),
                    tool_name: item.tool_name.as_ref().map(ToString::to_string),
                    completed: item.completed,
                })
                .collect(),
            observations: state
                .observations
                .iter()
                .map(|observation| ContextObservation {
                    tool_name: observation.tool_name.to_string(),
                    status: observation.status,
                    text: truncate_text(&observation.text, MAX_OBSERVATION_TEXT_BYTES),
                    metadata: observation.metadata.clone(),
                    error_code: observation.error_code.clone(),
                    truncated: observation.truncated
                        || observation.text.len() > MAX_OBSERVATION_TEXT_BYTES,
                })
                .collect(),
            available_tools: registry
                .definitions()
                .iter()
                .map(|definition| ContextTool {
                    name: definition.name.to_string(),
                    category: definition.category,
                    risk_level: definition.risk_level,
                    permissions: definition.permissions.clone(),
                    timeout_ms: definition.timeout_ms,
                    approval_policy: definition.approval_policy,
                })
                .collect(),
            event_count: state.events.len(),
        }
    }
}

fn truncate_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use coddy_core::{ToolName, ToolResultStatus};
    use serde_json::json;
    use uuid::Uuid;

    use crate::{LocalAgentRuntime, READ_FILE_TOOL};

    use super::*;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("coddy-context-test-{}", Uuid::new_v4()));
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

    #[test]
    fn snapshot_includes_tools_plan_and_observations() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let runtime = LocalAgentRuntime::new(&workspace.path).expect("runtime");
        let mut state = runtime.start_run(Uuid::new_v4(), "inspect docs");
        runtime.add_plan_item(
            &mut state,
            "Read README",
            Some(ToolName::new(READ_FILE_TOOL).expect("tool name")),
        );
        state.observations.push(crate::Observation {
            tool_name: ToolName::new(READ_FILE_TOOL).expect("tool name"),
            status: ToolResultStatus::Succeeded,
            text: "# Coddy\n".to_string(),
            metadata: json!({ "path": "README.md" }),
            error_code: None,
            truncated: false,
        });

        let snapshot = ContextSnapshot::from_runtime_parts(
            runtime.workspace(),
            runtime.router().registry(),
            &state,
        );

        assert_eq!(snapshot.goal, "inspect docs");
        assert_eq!(snapshot.plan[0].tool_name.as_deref(), Some(READ_FILE_TOOL));
        assert_eq!(snapshot.observations[0].text, "# Coddy\n");
        assert!(snapshot
            .available_tools
            .iter()
            .any(|tool| tool.name == READ_FILE_TOOL));
    }

    #[test]
    fn snapshot_truncates_large_observation_text() {
        let workspace = TempWorkspace::new();
        let runtime = LocalAgentRuntime::new(&workspace.path).expect("runtime");
        let mut state = runtime.start_run(Uuid::new_v4(), "large output");
        state.observations.push(crate::Observation {
            tool_name: ToolName::new(READ_FILE_TOOL).expect("tool name"),
            status: ToolResultStatus::Succeeded,
            text: "x".repeat(MAX_OBSERVATION_TEXT_BYTES + 1),
            metadata: Value::Object(Default::default()),
            error_code: None,
            truncated: false,
        });

        let snapshot = ContextSnapshot::from_runtime_parts(
            runtime.workspace(),
            runtime.router().registry(),
            &state,
        );

        assert_eq!(
            snapshot.observations[0].text.len(),
            MAX_OBSERVATION_TEXT_BYTES
        );
        assert!(snapshot.observations[0].truncated);
    }
}
