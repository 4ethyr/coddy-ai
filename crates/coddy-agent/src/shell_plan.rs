use std::path::Path;

use coddy_core::{PermissionRequest, ReplEvent, ToolError, ToolResult, ToolStatus};
use serde_json::json;
use uuid::Uuid;

use crate::{
    AgentError, BlockedCommandReason, CommandDecision, CommandGuard, CommandRisk, WorkspaceRoot,
    SHELL_RUN_TOOL,
};

pub const DEFAULT_SHELL_TIMEOUT_MS: u64 = 120_000;
pub const MAX_SHELL_TIMEOUT_MS: u64 = 600_000;

#[derive(Debug, Clone, PartialEq)]
pub enum ShellApprovalState {
    NotRequired,
    Pending(PermissionRequest),
    Blocked(BlockedCommandReason),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellPlanRequest {
    pub session_id: Uuid,
    pub run_id: Uuid,
    pub tool_call_id: Option<Uuid>,
    pub command: String,
    pub description: Option<String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
    pub requested_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellPlan {
    pub tool_call_id: Uuid,
    pub command: String,
    pub normalized_command: String,
    pub cwd: String,
    pub timeout_ms: u64,
    pub risk: CommandRisk,
    pub approval_state: ShellApprovalState,
    pub events: Vec<ReplEvent>,
    pub denied_result: Option<ToolResult>,
}

#[derive(Debug, Clone)]
pub struct ShellPlanner {
    workspace: WorkspaceRoot,
    guard: CommandGuard,
}

impl ShellPlanner {
    pub fn new(workspace_root: impl AsRef<Path>) -> Result<Self, AgentError> {
        Ok(Self {
            workspace: WorkspaceRoot::new(workspace_root)?,
            guard: CommandGuard,
        })
    }

    pub fn plan(&self, request: ShellPlanRequest) -> Result<ShellPlan, AgentError> {
        let call_id = request.tool_call_id.unwrap_or_else(Uuid::new_v4);
        let timeout_ms = validate_timeout(request.timeout_ms)?;
        let cwd = self.resolve_cwd(request.cwd.as_deref().unwrap_or("."))?;
        let assessment = self.guard.assess(
            request.session_id,
            request.run_id,
            Some(call_id),
            request.command.clone(),
            request.description,
            request.requested_at_unix_ms,
        );

        match assessment.decision {
            CommandDecision::AllowReadOnly => Ok(ShellPlan {
                tool_call_id: call_id,
                command: request.command,
                normalized_command: assessment.normalized,
                cwd,
                timeout_ms,
                risk: assessment.risk,
                approval_state: ShellApprovalState::NotRequired,
                events: Vec::new(),
                denied_result: None,
            }),
            CommandDecision::RequiresApproval(mut permission_request) => {
                attach_shell_metadata(&mut permission_request, &cwd, timeout_ms);
                Ok(ShellPlan {
                    tool_call_id: call_id,
                    command: request.command,
                    normalized_command: assessment.normalized,
                    cwd,
                    timeout_ms,
                    risk: assessment.risk,
                    approval_state: ShellApprovalState::Pending(permission_request.clone()),
                    events: vec![ReplEvent::PermissionRequested {
                        request: permission_request,
                    }],
                    denied_result: None,
                })
            }
            CommandDecision::Blocked(reason) => {
                let denied_result = ToolResult::denied(
                    call_id,
                    ToolError::new(
                        "command_blocked",
                        format!("command blocked by guard: {reason:?}"),
                        false,
                    ),
                    request.requested_at_unix_ms,
                    request.requested_at_unix_ms,
                );
                Ok(ShellPlan {
                    tool_call_id: call_id,
                    command: request.command,
                    normalized_command: assessment.normalized,
                    cwd,
                    timeout_ms,
                    risk: assessment.risk,
                    approval_state: ShellApprovalState::Blocked(reason),
                    events: vec![
                        ReplEvent::ToolStarted {
                            name: SHELL_RUN_TOOL.to_string(),
                        },
                        ReplEvent::ToolCompleted {
                            name: SHELL_RUN_TOOL.to_string(),
                            status: ToolStatus::Denied,
                        },
                    ],
                    denied_result: Some(denied_result),
                })
            }
        }
    }

    fn resolve_cwd(&self, cwd: &str) -> Result<String, AgentError> {
        let path = self.workspace.resolve_existing_path(cwd)?;
        if !path.is_dir() {
            return Err(AgentError::NotDirectory(
                self.workspace.relative_path(&path),
            ));
        }
        Ok(self.workspace.relative_path(&path))
    }
}

fn validate_timeout(timeout_ms: Option<u64>) -> Result<u64, AgentError> {
    let timeout_ms = timeout_ms.unwrap_or(DEFAULT_SHELL_TIMEOUT_MS);
    if timeout_ms == 0 {
        return Err(AgentError::InvalidInput(
            "shell timeout must be greater than zero".to_string(),
        ));
    }
    if timeout_ms > MAX_SHELL_TIMEOUT_MS {
        return Err(AgentError::InvalidInput(format!(
            "shell timeout must be <= {MAX_SHELL_TIMEOUT_MS} ms"
        )));
    }
    Ok(timeout_ms)
}

fn attach_shell_metadata(request: &mut PermissionRequest, cwd: &str, timeout_ms: u64) {
    let Some(metadata) = request.metadata.as_object_mut() else {
        request.metadata = json!({
            "cwd": cwd,
            "timeout_ms": timeout_ms,
        });
        return;
    };
    metadata.insert("cwd".to_string(), json!(cwd));
    metadata.insert("timeout_ms".to_string(), json!(timeout_ms));
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use coddy_core::{ToolPermission, ToolResultStatus};

    use super::*;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("coddy-shell-plan-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp workspace");
            Self { path }
        }

        fn mkdir(&self, relative_path: &str) {
            fs::create_dir_all(self.path.join(relative_path)).expect("create fixture directory");
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

    fn request(command: &str) -> ShellPlanRequest {
        ShellPlanRequest {
            session_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            tool_call_id: Some(Uuid::new_v4()),
            command: command.to_string(),
            description: Some("test command".to_string()),
            cwd: None,
            timeout_ms: None,
            requested_at_unix_ms: 1_775_000_000_000,
        }
    }

    #[test]
    fn plans_read_only_commands_without_approval_events() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");

        let plan = planner.plan(request("git status --short")).expect("plan");

        assert_eq!(plan.cwd, ".");
        assert_eq!(plan.timeout_ms, DEFAULT_SHELL_TIMEOUT_MS);
        assert_eq!(plan.risk, CommandRisk::Low);
        assert_eq!(plan.approval_state, ShellApprovalState::NotRequired);
        assert!(plan.events.is_empty());
        assert!(plan.denied_result.is_none());
    }

    #[test]
    fn plans_approval_for_non_read_only_commands() {
        let workspace = TempWorkspace::new();
        workspace.mkdir("crates/coddy-agent");
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let mut request = request("cargo build --release");
        request.cwd = Some("crates/coddy-agent".to_string());
        request.timeout_ms = Some(30_000);

        let plan = planner.plan(request).expect("plan");

        let ShellApprovalState::Pending(permission_request) = &plan.approval_state else {
            panic!("expected pending approval");
        };
        assert_eq!(plan.cwd, "crates/coddy-agent");
        assert_eq!(plan.timeout_ms, 30_000);
        assert_eq!(permission_request.tool_name.as_str(), SHELL_RUN_TOOL);
        assert_eq!(
            permission_request.permission,
            ToolPermission::ExecuteCommand
        );
        assert_eq!(permission_request.patterns, vec!["cargo build --release"]);
        assert_eq!(
            permission_request.metadata["cwd"],
            json!("crates/coddy-agent")
        );
        assert_eq!(permission_request.metadata["timeout_ms"], json!(30_000));
        assert_eq!(
            plan.events,
            vec![ReplEvent::PermissionRequested {
                request: permission_request.clone()
            }]
        );
    }

    #[test]
    fn plans_blocked_commands_as_denied_results() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");

        let plan = planner.plan(request("rm -rf target")).expect("plan");

        assert_eq!(
            plan.approval_state,
            ShellApprovalState::Blocked(BlockedCommandReason::DestructiveFilesystem)
        );
        let denied = plan.denied_result.expect("denied result");
        assert_eq!(denied.status, ToolResultStatus::Denied);
        assert_eq!(denied.error.expect("blocked error").code, "command_blocked");
        assert_eq!(
            plan.events,
            vec![
                ReplEvent::ToolStarted {
                    name: SHELL_RUN_TOOL.to_string()
                },
                ReplEvent::ToolCompleted {
                    name: SHELL_RUN_TOOL.to_string(),
                    status: ToolStatus::Denied
                }
            ]
        );
    }

    #[test]
    fn rejects_cwd_outside_workspace_or_not_directory() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let planner = ShellPlanner::new(&workspace.path).expect("planner");

        let mut traversal = request("pwd");
        traversal.cwd = Some("..".to_string());
        assert!(matches!(
            planner.plan(traversal).expect_err("path traversal must fail"),
            AgentError::PathTraversal(path) if path == ".."
        ));

        let mut file_cwd = request("pwd");
        file_cwd.cwd = Some("README.md".to_string());
        assert!(matches!(
            planner.plan(file_cwd).expect_err("file cwd must fail"),
            AgentError::NotDirectory(path) if path == "README.md"
        ));
    }

    #[test]
    fn validates_timeout_bounds() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");

        let mut zero = request("pwd");
        zero.timeout_ms = Some(0);
        assert!(matches!(
            planner.plan(zero).expect_err("zero timeout must fail"),
            AgentError::InvalidInput(message) if message.contains("greater than zero")
        ));

        let mut too_large = request("pwd");
        too_large.timeout_ms = Some(MAX_SHELL_TIMEOUT_MS + 1);
        assert!(matches!(
            planner.plan(too_large).expect_err("large timeout must fail"),
            AgentError::InvalidInput(message) if message.contains("shell timeout")
        ));
    }
}
