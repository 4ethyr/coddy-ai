use std::path::Path;

use coddy_core::PermissionReply;
use uuid::Uuid;

use crate::{
    AgentError, DeterministicPlanExecutor, DeterministicPlanItem, DeterministicPlanReport,
    DeterministicPlanStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalStatus {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalCase {
    pub name: String,
    pub goal: String,
    pub plan: Vec<DeterministicPlanItem>,
    pub approvals: Vec<PermissionReply>,
    pub expectations: EvalExpectations,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalExpectations {
    pub final_status: DeterministicPlanStatus,
    pub approvals_requested: usize,
    pub required_observation_substrings: Vec<String>,
    pub required_error_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalReport {
    pub case_name: String,
    pub status: EvalStatus,
    pub final_plan_status: DeterministicPlanStatus,
    pub approvals_requested: usize,
    pub failures: Vec<String>,
    pub plan_report: DeterministicPlanReport,
}

#[derive(Debug, Clone)]
pub struct EvalRunner {
    executor: DeterministicPlanExecutor,
}

impl EvalRunner {
    pub fn new(workspace_root: impl AsRef<Path>) -> Result<Self, AgentError> {
        Ok(Self {
            executor: DeterministicPlanExecutor::new(workspace_root)?,
        })
    }

    pub fn run_case(&self, case: &EvalCase) -> EvalReport {
        let session_id = Uuid::new_v4();
        let mut approvals_requested = 0_usize;
        let mut approval_cursor = 0_usize;
        let mut report = self
            .executor
            .execute(session_id, case.goal.clone(), &case.plan);

        while report.status == DeterministicPlanStatus::AwaitingApproval {
            approvals_requested += 1;
            let Some(request) = report.pending_permission.clone() else {
                break;
            };
            let Some(reply) = case.approvals.get(approval_cursor).copied() else {
                break;
            };
            approval_cursor += 1;
            report = self.executor.resume_after_permission(
                report.state,
                request.id,
                reply,
                &case.plan,
                report.next_item_index,
            );
        }

        let failures = evaluate_expectations(case, &report, approvals_requested);
        EvalReport {
            case_name: case.name.clone(),
            status: if failures.is_empty() {
                EvalStatus::Passed
            } else {
                EvalStatus::Failed
            },
            final_plan_status: report.status,
            approvals_requested,
            failures,
            plan_report: report,
        }
    }

    pub fn run_suite(&self, cases: &[EvalCase]) -> EvalSuiteReport {
        let reports = cases.iter().map(|case| self.run_case(case)).collect();
        EvalSuiteReport::new(reports)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalSuiteReport {
    pub reports: Vec<EvalReport>,
    pub passed: usize,
    pub failed: usize,
}

impl EvalSuiteReport {
    fn new(reports: Vec<EvalReport>) -> Self {
        let passed = reports
            .iter()
            .filter(|report| report.status == EvalStatus::Passed)
            .count();
        let failed = reports.len().saturating_sub(passed);
        Self {
            reports,
            passed,
            failed,
        }
    }

    pub fn is_success(&self) -> bool {
        self.failed == 0
    }
}

impl EvalCase {
    pub fn new(
        name: impl Into<String>,
        goal: impl Into<String>,
        plan: Vec<DeterministicPlanItem>,
        approvals: Vec<PermissionReply>,
        expectations: EvalExpectations,
    ) -> Self {
        Self {
            name: name.into(),
            goal: goal.into(),
            plan,
            approvals,
            expectations,
        }
    }
}

impl EvalExpectations {
    pub fn final_status(final_status: DeterministicPlanStatus) -> Self {
        Self {
            final_status,
            approvals_requested: 0,
            required_observation_substrings: Vec::new(),
            required_error_codes: Vec::new(),
        }
    }

    pub fn approvals_requested(mut self, approvals_requested: usize) -> Self {
        self.approvals_requested = approvals_requested;
        self
    }

    pub fn observation_contains(mut self, value: impl Into<String>) -> Self {
        self.required_observation_substrings.push(value.into());
        self
    }

    pub fn error_code(mut self, value: impl Into<String>) -> Self {
        self.required_error_codes.push(value.into());
        self
    }
}

fn evaluate_expectations(
    case: &EvalCase,
    report: &DeterministicPlanReport,
    approvals_requested: usize,
) -> Vec<String> {
    let mut failures = Vec::new();

    if report.status != case.expectations.final_status {
        failures.push(format!(
            "expected final status {:?}, got {:?}",
            case.expectations.final_status, report.status
        ));
    }

    if approvals_requested != case.expectations.approvals_requested {
        failures.push(format!(
            "expected {} approval requests, got {approvals_requested}",
            case.expectations.approvals_requested
        ));
    }

    for expected in &case.expectations.required_observation_substrings {
        if !report
            .state
            .observations
            .iter()
            .any(|observation| observation.text.contains(expected))
        {
            failures.push(format!("missing observation substring: {expected}"));
        }
    }

    for expected in &case.expectations.required_error_codes {
        if !report
            .state
            .observations
            .iter()
            .any(|observation| observation.error_code.as_deref() == Some(expected.as_str()))
        {
            failures.push(format!("missing error code: {expected}"));
        }
    }

    failures
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use coddy_core::{ToolName, ToolResultStatus};
    use serde_json::json;

    use crate::{PREVIEW_EDIT_TOOL, READ_FILE_TOOL, SHELL_RUN_TOOL};

    use super::*;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("coddy-eval-test-{}", Uuid::new_v4()));
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

    fn item(description: &str, tool_name: &str, input: serde_json::Value) -> DeterministicPlanItem {
        DeterministicPlanItem::new(
            description,
            ToolName::new(tool_name).expect("tool name"),
            input,
        )
    }

    #[test]
    fn read_only_eval_passes() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "read-only",
            "read project docs",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Completed)
                .observation_contains("# Coddy"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(report.approvals_requested, 0);
    }

    #[test]
    fn shell_approval_eval_passes_after_reply() {
        let workspace = TempWorkspace::new();
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "shell-approval",
            "run approved shell",
            vec![item(
                "Print marker",
                SHELL_RUN_TOOL,
                json!({ "command": "printf coddy" }),
            )],
            vec![PermissionReply::Once],
            EvalExpectations::final_status(DeterministicPlanStatus::Completed)
                .approvals_requested(1)
                .observation_contains("coddy"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(report.approvals_requested, 1);
    }

    #[test]
    fn shell_block_eval_passes_with_denied_observation() {
        let workspace = TempWorkspace::new();
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "shell-block",
            "block destructive shell",
            vec![item(
                "Try destructive command",
                SHELL_RUN_TOOL,
                json!({ "command": "rm -rf target" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Failed)
                .error_code("command_blocked"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(
            report
                .plan_report
                .state
                .observations
                .last()
                .expect("observation")
                .status,
            ToolResultStatus::Denied
        );
    }

    #[test]
    fn edit_approval_eval_applies_change() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "Coddy REPL\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "edit-approval",
            "edit project docs",
            vec![
                item(
                    "Read README before edit",
                    READ_FILE_TOOL,
                    json!({ "path": "README.md" }),
                ),
                item(
                    "Preview README edit",
                    PREVIEW_EDIT_TOOL,
                    json!({
                        "path": "README.md",
                        "old_string": "Coddy",
                        "new_string": "Coddy Agent"
                    }),
                ),
            ],
            vec![PermissionReply::Once],
            EvalExpectations::final_status(DeterministicPlanStatus::Completed)
                .approvals_requested(1)
                .observation_contains("Edit applied: README.md"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(
            fs::read_to_string(workspace.path.join("README.md")).expect("read edited file"),
            "Coddy Agent REPL\n"
        );
    }

    #[test]
    fn edit_reject_eval_preserves_file() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "Coddy REPL\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "edit-reject",
            "reject project docs edit",
            vec![
                item(
                    "Read README before edit",
                    READ_FILE_TOOL,
                    json!({ "path": "README.md" }),
                ),
                item(
                    "Preview README edit",
                    PREVIEW_EDIT_TOOL,
                    json!({
                        "path": "README.md",
                        "old_string": "Coddy",
                        "new_string": "Coddy Agent"
                    }),
                ),
            ],
            vec![PermissionReply::Reject],
            EvalExpectations::final_status(DeterministicPlanStatus::Failed)
                .approvals_requested(1)
                .error_code("permission_rejected"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(
            fs::read_to_string(workspace.path.join("README.md")).expect("read preserved file"),
            "Coddy REPL\n"
        );
    }

    #[test]
    fn suite_counts_passed_and_failed_cases() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let passing = EvalCase::new(
            "passing",
            "read docs",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Completed),
        );
        let failing = EvalCase::new(
            "failing",
            "expect wrong status",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Failed),
        );

        let suite = runner.run_suite(&[passing, failing]);

        assert_eq!(suite.passed, 1);
        assert_eq!(suite.failed, 1);
        assert!(!suite.is_success());
    }
}
