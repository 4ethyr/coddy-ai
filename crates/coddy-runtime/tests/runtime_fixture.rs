use std::{fs, path::PathBuf};

use coddy_agent::{AgentToolRegistry, ShellExecutionConfig, ShellSandboxPolicy};
use coddy_core::{ApprovalPolicy, ToolPermission, ToolRiskLevel};
use coddy_ipc::{CoddyRequest, CoddyResult, ReplToolsJob};
use coddy_runtime::CoddyRuntime;
use uuid::Uuid;

struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("coddy-runtime-fixture-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temp workspace");
        Self { path }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn runtime_fixture_regression() {
    let workspace = TempWorkspace::new();
    let runtime = CoddyRuntime::with_workspace_and_shell_config(
        AgentToolRegistry::default(),
        &workspace.path,
        ShellExecutionConfig {
            sandbox_policy: ShellSandboxPolicy::Process,
            ..ShellExecutionConfig::default()
        },
    )
    .expect("runtime with fixture workspace");
    let request_id = Uuid::new_v4();

    let result = runtime.handle_request(CoddyRequest::Tools(ReplToolsJob { request_id }));

    let CoddyResult::ReplToolCatalog {
        request_id: actual_request_id,
        tools,
    } = result
    else {
        panic!("expected runtime tool catalog");
    };
    assert_eq!(actual_request_id, request_id);

    let shell = tools
        .iter()
        .find(|tool| tool.name == "shell.run")
        .expect("shell tool");
    assert_eq!(shell.risk_level, ToolRiskLevel::Medium);
    assert_eq!(shell.permissions, vec![ToolPermission::ExecuteCommand]);
    assert_eq!(shell.approval_policy, ApprovalPolicy::AskOnUse);

    let apply_edit = tools
        .iter()
        .find(|tool| tool.name == "filesystem.apply_edit")
        .expect("apply edit tool");
    assert_eq!(apply_edit.risk_level, ToolRiskLevel::High);
    assert_eq!(apply_edit.permissions, vec![ToolPermission::WriteWorkspace]);
    assert_eq!(apply_edit.approval_policy, ApprovalPolicy::AlwaysAsk);

    let subagent_team_plan = tools
        .iter()
        .find(|tool| tool.name == "subagent.team_plan")
        .expect("subagent team plan tool");
    assert_eq!(subagent_team_plan.risk_level, ToolRiskLevel::Low);
    assert_eq!(
        subagent_team_plan.permissions,
        vec![ToolPermission::DelegateSubagent]
    );
}
