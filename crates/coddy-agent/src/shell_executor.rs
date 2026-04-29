use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use coddy_core::{
    PermissionReply, ReplEvent, ToolError, ToolOutput, ToolResult, ToolResultStatus, ToolStatus,
};
use serde_json::json;

use crate::{
    AgentError, ShellApprovalState, ShellPlan, ToolExecution, WorkspaceRoot, SHELL_RUN_TOOL,
};

pub const DEFAULT_SHELL_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellExecutionConfig {
    pub output_limit_bytes: usize,
    pub poll_interval_ms: u64,
}

impl Default for ShellExecutionConfig {
    fn default() -> Self {
        Self {
            output_limit_bytes: DEFAULT_SHELL_OUTPUT_LIMIT_BYTES,
            poll_interval_ms: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShellExecutor {
    workspace: WorkspaceRoot,
    config: ShellExecutionConfig,
}

impl ShellExecutor {
    pub fn new(workspace_root: impl AsRef<Path>) -> Result<Self, AgentError> {
        Self::with_config(workspace_root, ShellExecutionConfig::default())
    }

    pub fn with_config(
        workspace_root: impl AsRef<Path>,
        config: ShellExecutionConfig,
    ) -> Result<Self, AgentError> {
        if config.output_limit_bytes == 0 {
            return Err(AgentError::InvalidInput(
                "shell output limit must be greater than zero".to_string(),
            ));
        }
        if config.poll_interval_ms == 0 {
            return Err(AgentError::InvalidInput(
                "shell poll interval must be greater than zero".to_string(),
            ));
        }

        Ok(Self {
            workspace: WorkspaceRoot::new(workspace_root)?,
            config,
        })
    }

    pub fn execute(&self, plan: &ShellPlan, approval: Option<PermissionReply>) -> ToolExecution {
        if matches!(plan.approval_state, ShellApprovalState::Blocked(_)) {
            if let Some(result) = &plan.denied_result {
                return ToolExecution {
                    result: result.clone(),
                    events: plan.events.clone(),
                };
            }
        }

        let result = match (&plan.approval_state, approval) {
            (ShellApprovalState::NotRequired, _) => self.run_plan(plan),
            (
                ShellApprovalState::Pending(_),
                Some(PermissionReply::Once | PermissionReply::Always),
            ) => self.run_plan(plan),
            (ShellApprovalState::Pending(_), Some(PermissionReply::Reject)) => denied_result(
                plan.tool_call_id,
                "permission_rejected",
                "shell command was rejected by permission reply",
            ),
            (ShellApprovalState::Pending(_), None) => denied_result(
                plan.tool_call_id,
                "permission_required",
                "shell command requires approval before execution",
            ),
            (ShellApprovalState::Blocked(reason), _) => denied_result(
                plan.tool_call_id,
                "command_blocked",
                format!("command blocked by guard: {reason:?}"),
            ),
        };

        execution(result)
    }

    fn run_plan(&self, plan: &ShellPlan) -> ToolResult {
        let started_at = unix_ms_now();
        let result = self.run_shell(plan);
        let completed_at = unix_ms_now();

        match result {
            Ok(output) => {
                ToolResult::succeeded(plan.tool_call_id, output, started_at, completed_at)
            }
            Err(error) => ToolResult::failed(plan.tool_call_id, error, started_at, completed_at),
        }
    }

    fn run_shell(&self, plan: &ShellPlan) -> Result<ToolOutput, ToolError> {
        let cwd = self
            .workspace
            .resolve_existing_path(&plan.cwd)
            .map_err(AgentError::into_tool_error)?;
        if !cwd.is_dir() {
            return Err(ToolError::new(
                "not_directory",
                format!("shell cwd is not a directory: {}", plan.cwd),
                false,
            ));
        }

        let started = Instant::now();
        let mut child = Command::new("/bin/sh")
            .arg("-lc")
            .arg(&plan.normalized_command)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| {
                ToolError::new(
                    "command_spawn_failed",
                    format!("failed to spawn shell command: {source}"),
                    true,
                )
            })?;

        let stdout = child
            .stdout
            .take()
            .map(|reader| spawn_limited_reader(reader, self.config.output_limit_bytes));
        let stderr = child
            .stderr
            .take()
            .map(|reader| spawn_limited_reader(reader, self.config.output_limit_bytes));

        let timeout = Duration::from_millis(plan.timeout_ms);
        let poll_interval = Duration::from_millis(self.config.poll_interval_ms);
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|source| {
                ToolError::new(
                    "command_wait_failed",
                    format!("failed to wait for shell command: {source}"),
                    true,
                )
            })? {
                break status;
            }

            if started.elapsed() >= timeout {
                timed_out = true;
                let _ = child.kill();
                break child.wait().map_err(|source| {
                    ToolError::new(
                        "command_wait_failed",
                        format!("failed to wait for timed out shell command: {source}"),
                        true,
                    )
                })?;
            }

            thread::sleep(poll_interval.min(timeout.saturating_sub(started.elapsed())));
        };

        if timed_out {
            // Avoid blocking on pipe reader threads if a descendant process keeps
            // stdout/stderr open after the shell process is killed.
            return Err(ToolError::new(
                "command_timeout",
                format!("shell command timed out after {} ms", plan.timeout_ms),
                true,
            ));
        }

        let stdout = join_limited_output(stdout);
        let stderr = join_limited_output(stderr);
        let duration_ms = started.elapsed().as_millis() as u64;

        Ok(ToolOutput {
            text: shell_text(&stdout.text, &stderr.text),
            metadata: json!({
                "command": plan.normalized_command,
                "cwd": plan.cwd,
                "exit_code": status.code(),
                "success": status.success(),
                "duration_ms": duration_ms,
                "stdout": stdout.text,
                "stderr": stderr.text,
                "stdout_truncated": stdout.truncated,
                "stderr_truncated": stderr.truncated,
            }),
            truncated: stdout.truncated || stderr.truncated,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LimitedOutput {
    text: String,
    truncated: bool,
}

fn spawn_limited_reader<R>(mut reader: R, limit: usize) -> thread::JoinHandle<LimitedOutput>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut stored = Vec::new();
        let mut buffer = [0_u8; 8192];
        let mut truncated = false;

        loop {
            let read = reader.read(&mut buffer).unwrap_or_default();
            if read == 0 {
                break;
            }

            let available = limit.saturating_sub(stored.len());
            if available > 0 {
                let to_store = available.min(read);
                stored.extend_from_slice(&buffer[..to_store]);
                truncated |= to_store < read;
            } else {
                truncated = true;
            }
        }

        LimitedOutput {
            text: String::from_utf8_lossy(&stored).to_string(),
            truncated,
        }
    })
}

fn join_limited_output(handle: Option<thread::JoinHandle<LimitedOutput>>) -> LimitedOutput {
    handle
        .map(|handle| {
            handle.join().unwrap_or_else(|_| LimitedOutput {
                text: String::new(),
                truncated: true,
            })
        })
        .unwrap_or_else(|| LimitedOutput {
            text: String::new(),
            truncated: false,
        })
}

fn shell_text(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (false, false) => format!("stdout:\n{stdout}\nstderr:\n{stderr}"),
        (true, true) => String::new(),
    }
}

fn execution(result: ToolResult) -> ToolExecution {
    let events = vec![
        ReplEvent::ToolStarted {
            name: SHELL_RUN_TOOL.to_string(),
        },
        ReplEvent::ToolCompleted {
            name: SHELL_RUN_TOOL.to_string(),
            status: tool_status(result.status),
        },
    ];
    ToolExecution { result, events }
}

fn denied_result(call_id: uuid::Uuid, code: &str, message: impl Into<String>) -> ToolResult {
    let now = unix_ms_now();
    ToolResult::denied(call_id, ToolError::new(code, message, false), now, now)
}

fn tool_status(status: ToolResultStatus) -> ToolStatus {
    match status {
        ToolResultStatus::Succeeded => ToolStatus::Succeeded,
        ToolResultStatus::Failed => ToolStatus::Failed,
        ToolResultStatus::Cancelled => ToolStatus::Cancelled,
        ToolResultStatus::Denied => ToolStatus::Denied,
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

    use coddy_core::PermissionReply;
    use uuid::Uuid;

    use crate::{ShellPlanRequest, ShellPlanner};

    use super::*;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("coddy-shell-exec-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp workspace");
            Self { path }
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
            timeout_ms: Some(1_000),
            requested_at_unix_ms: 1_775_000_000_000,
        }
    }

    #[test]
    fn executes_read_only_plan_without_explicit_approval() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("pwd")).expect("plan");

        let execution = executor.execute(&plan, None);

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert!(output
            .text
            .contains(workspace.path.to_string_lossy().as_ref()));
        assert_eq!(output.metadata["cwd"], json!("."));
    }

    #[test]
    fn executes_pending_plan_only_after_approval() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("printf coddy")).expect("plan");

        let denied = executor.execute(&plan, None);
        assert_eq!(denied.result.status, ToolResultStatus::Denied);
        assert_eq!(
            denied.result.error.expect("denied error").code,
            "permission_required"
        );

        let approved = executor.execute(&plan, Some(PermissionReply::Once));
        assert_eq!(approved.result.status, ToolResultStatus::Succeeded);
        let output = approved.result.output.expect("output");
        assert_eq!(output.metadata["stdout"], json!("coddy"));
        assert_eq!(output.metadata["success"], json!(true));
    }

    #[test]
    fn rejects_pending_plan_when_permission_is_rejected() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("printf coddy")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Reject));

        assert_eq!(execution.result.status, ToolResultStatus::Denied);
        assert_eq!(
            execution.result.error.expect("denied error").code,
            "permission_rejected"
        );
    }

    #[test]
    fn preserves_blocked_plan_denial_without_running_command() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("rm -rf target")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Always));

        assert_eq!(execution.result.status, ToolResultStatus::Denied);
        assert_eq!(
            execution.result.error.expect("blocked error").code,
            "command_blocked"
        );
    }

    #[test]
    fn truncates_large_output() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::with_config(
            &workspace.path,
            ShellExecutionConfig {
                output_limit_bytes: 4,
                poll_interval_ms: 10,
            },
        )
        .expect("executor");
        let plan = planner.plan(request("printf 1234567890")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert_eq!(output.metadata["stdout"], json!("1234"));
        assert!(output.truncated);
    }

    #[test]
    fn fails_when_command_times_out() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::with_config(
            &workspace.path,
            ShellExecutionConfig {
                output_limit_bytes: DEFAULT_SHELL_OUTPUT_LIMIT_BYTES,
                poll_interval_ms: 5,
            },
        )
        .expect("executor");
        let mut timeout_request = request("while true; do :; done");
        timeout_request.timeout_ms = Some(20);
        let plan = planner.plan(timeout_request).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Failed);
        assert_eq!(
            execution.result.error.expect("timeout error").code,
            "command_timeout"
        );
    }
}
