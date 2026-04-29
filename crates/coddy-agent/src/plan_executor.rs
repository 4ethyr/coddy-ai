use coddy_core::{PermissionReply, PermissionRequest, ToolCall, ToolName, ToolResultStatus};
use serde_json::Value;
use uuid::Uuid;

use crate::{AgentError, AgentRunStatus, ContextSnapshot, LocalAgentRuntime, RunState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeterministicPlanStatus {
    Completed,
    AwaitingApproval,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeterministicPlanItem {
    pub description: String,
    pub tool_name: ToolName,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeterministicPlanReport {
    pub state: RunState,
    pub context: ContextSnapshot,
    pub status: DeterministicPlanStatus,
    pub next_item_index: usize,
    pub pending_permission: Option<PermissionRequest>,
}

#[derive(Debug, Clone)]
pub struct DeterministicPlanExecutor {
    runtime: LocalAgentRuntime,
}

impl DeterministicPlanExecutor {
    pub fn new(workspace_root: impl AsRef<std::path::Path>) -> Result<Self, AgentError> {
        Ok(Self {
            runtime: LocalAgentRuntime::new(workspace_root)?,
        })
    }

    pub fn runtime(&self) -> &LocalAgentRuntime {
        &self.runtime
    }

    pub fn execute(
        &self,
        session_id: Uuid,
        goal: impl Into<String>,
        items: &[DeterministicPlanItem],
    ) -> DeterministicPlanReport {
        let state = self.runtime.start_run(session_id, goal);
        self.execute_from(state, items, 0)
    }

    pub fn resume_after_permission(
        &self,
        mut state: RunState,
        request_id: Uuid,
        reply: PermissionReply,
        items: &[DeterministicPlanItem],
        next_item_index: usize,
    ) -> DeterministicPlanReport {
        let outcome = self.runtime.reply_permission(&mut state, request_id, reply);
        if matches!(
            outcome.status(),
            Some(ToolResultStatus::Failed | ToolResultStatus::Denied | ToolResultStatus::Cancelled)
        ) {
            state.status = AgentRunStatus::Failed;
            return self.report(
                state,
                DeterministicPlanStatus::Failed,
                next_item_index,
                None,
            );
        }

        self.execute_from(state, items, next_item_index)
    }

    fn execute_from(
        &self,
        mut state: RunState,
        items: &[DeterministicPlanItem],
        start_index: usize,
    ) -> DeterministicPlanReport {
        for (index, item) in items.iter().enumerate().skip(start_index) {
            self.runtime.add_plan_item(
                &mut state,
                item.description.clone(),
                Some(item.tool_name.clone()),
            );
            let call = ToolCall::new(
                state.session_id,
                state.run_id,
                item.tool_name.clone(),
                item.input.clone(),
                unix_ms_now(),
            );
            let outcome = self.runtime.execute_tool_call(&mut state, &call);

            if let Some(permission_request) = outcome.permission_request.clone() {
                return self.report(
                    state,
                    DeterministicPlanStatus::AwaitingApproval,
                    index + 1,
                    Some(permission_request),
                );
            }

            if matches!(
                outcome.status(),
                Some(
                    ToolResultStatus::Failed
                        | ToolResultStatus::Denied
                        | ToolResultStatus::Cancelled
                )
            ) {
                state.status = AgentRunStatus::Failed;
                return self.report(state, DeterministicPlanStatus::Failed, index + 1, None);
            }
        }

        self.runtime.complete_run(&mut state);
        self.report(state, DeterministicPlanStatus::Completed, items.len(), None)
    }

    fn report(
        &self,
        state: RunState,
        status: DeterministicPlanStatus,
        next_item_index: usize,
        pending_permission: Option<PermissionRequest>,
    ) -> DeterministicPlanReport {
        let context = ContextSnapshot::from_runtime_parts(
            self.runtime.workspace(),
            self.runtime.router().registry(),
            &state,
        );
        DeterministicPlanReport {
            state,
            context,
            status,
            next_item_index,
            pending_permission,
        }
    }
}

impl DeterministicPlanItem {
    pub fn new(description: impl Into<String>, tool_name: ToolName, input: Value) -> Self {
        Self {
            description: description.into(),
            tool_name,
            input,
        }
    }
}

fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use coddy_core::ToolResultStatus;
    use serde_json::json;

    use crate::{READ_FILE_TOOL, SHELL_RUN_TOOL};

    use super::*;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("coddy-plan-exec-test-{}", Uuid::new_v4()));
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

    fn item(description: &str, tool_name: &str, input: Value) -> DeterministicPlanItem {
        DeterministicPlanItem::new(
            description,
            ToolName::new(tool_name).expect("tool name"),
            input,
        )
    }

    #[test]
    fn completes_read_only_plan_and_builds_context() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let executor = DeterministicPlanExecutor::new(&workspace.path).expect("executor");
        let items = vec![item(
            "Read README",
            READ_FILE_TOOL,
            json!({ "path": "README.md" }),
        )];

        let report = executor.execute(Uuid::new_v4(), "inspect project", &items);

        assert_eq!(report.status, DeterministicPlanStatus::Completed);
        assert_eq!(report.state.status, AgentRunStatus::Completed);
        assert_eq!(report.next_item_index, 1);
        assert_eq!(report.context.observations[0].text, "# Coddy\n");
        assert!(report.pending_permission.is_none());
    }

    #[test]
    fn stops_on_pending_approval_and_resumes_after_reply() {
        let workspace = TempWorkspace::new();
        let executor = DeterministicPlanExecutor::new(&workspace.path).expect("executor");
        let items = vec![item(
            "Print from shell",
            SHELL_RUN_TOOL,
            json!({ "command": "printf coddy" }),
        )];

        let pending = executor.execute(Uuid::new_v4(), "run command", &items);

        assert_eq!(pending.status, DeterministicPlanStatus::AwaitingApproval);
        assert_eq!(pending.next_item_index, 1);
        let request = pending.pending_permission.clone().expect("permission");

        let resumed = executor.resume_after_permission(
            pending.state,
            request.id,
            PermissionReply::Once,
            &items,
            pending.next_item_index,
        );

        assert_eq!(resumed.status, DeterministicPlanStatus::Completed);
        assert_eq!(
            resumed
                .state
                .observations
                .last()
                .expect("observation")
                .metadata["stdout"],
            json!("coddy")
        );
    }

    #[test]
    fn stops_on_failed_step() {
        let workspace = TempWorkspace::new();
        let executor = DeterministicPlanExecutor::new(&workspace.path).expect("executor");
        let items = vec![item(
            "Read missing file",
            READ_FILE_TOOL,
            json!({ "path": "missing.md" }),
        )];

        let report = executor.execute(Uuid::new_v4(), "fail cleanly", &items);

        assert_eq!(report.status, DeterministicPlanStatus::Failed);
        assert_eq!(report.state.status, AgentRunStatus::Failed);
        assert_eq!(
            report
                .state
                .observations
                .last()
                .expect("observation")
                .status,
            ToolResultStatus::Failed
        );
    }
}
