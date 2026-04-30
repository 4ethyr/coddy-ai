use coddy_core::{SubagentHandoffPrepared, SubagentLifecycleStatus, SubagentLifecycleUpdate};

use crate::{
    SubagentHandoffPlan, SubagentMode, APPLY_EDIT_TOOL, PREVIEW_EDIT_TOOL, SHELL_RUN_TOOL,
};

const READY_SCORE: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentExecutionStartStatus {
    AwaitingApproval,
    ReadyToStart,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentExecutionStartPlan {
    pub status: SubagentExecutionStartStatus,
    pub reason: Option<String>,
    pub lifecycle_updates: Vec<SubagentLifecycleUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentExecutionHandoff {
    pub name: String,
    pub mode: String,
    pub allowed_tools: Vec<String>,
    pub approval_required: bool,
    pub readiness_score: u8,
    pub readiness_issues: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct SubagentExecutionGate;

impl SubagentExecutionGate {
    pub fn plan_start(
        &self,
        handoff: &SubagentHandoffPlan,
        approval_granted: bool,
    ) -> SubagentExecutionStartPlan {
        let handoff = SubagentExecutionHandoff::from(handoff);
        self.plan_start_for(&handoff, approval_granted)
    }

    pub fn plan_start_for(
        &self,
        handoff: &SubagentExecutionHandoff,
        approval_granted: bool,
    ) -> SubagentExecutionStartPlan {
        if let Some(reason) = blocked_reason(handoff) {
            return SubagentExecutionStartPlan {
                status: SubagentExecutionStartStatus::Blocked,
                reason: Some(reason.clone()),
                lifecycle_updates: vec![lifecycle_update(
                    handoff,
                    SubagentLifecycleStatus::Blocked,
                    Some(reason),
                )],
            };
        }

        if handoff.approval_required && !approval_granted {
            return SubagentExecutionStartPlan {
                status: SubagentExecutionStartStatus::AwaitingApproval,
                reason: Some("approval required before running subagent".to_string()),
                lifecycle_updates: vec![lifecycle_update(
                    handoff,
                    SubagentLifecycleStatus::Prepared,
                    None,
                )],
            };
        }

        SubagentExecutionStartPlan {
            status: SubagentExecutionStartStatus::ReadyToStart,
            reason: None,
            lifecycle_updates: vec![
                lifecycle_update(handoff, SubagentLifecycleStatus::Prepared, None),
                lifecycle_update(handoff, SubagentLifecycleStatus::Approved, None),
                lifecycle_update(handoff, SubagentLifecycleStatus::Running, None),
            ],
        }
    }
}

impl From<&SubagentHandoffPlan> for SubagentExecutionHandoff {
    fn from(handoff: &SubagentHandoffPlan) -> Self {
        Self {
            name: handoff.name.clone(),
            mode: handoff.mode.as_str().to_string(),
            allowed_tools: handoff.allowed_tools.clone(),
            approval_required: handoff.approval_required,
            readiness_score: handoff.readiness_score,
            readiness_issues: handoff.readiness_issues.clone(),
        }
    }
}

impl From<&SubagentHandoffPrepared> for SubagentExecutionHandoff {
    fn from(handoff: &SubagentHandoffPrepared) -> Self {
        Self {
            name: handoff.name.clone(),
            mode: handoff.mode.clone(),
            allowed_tools: handoff.allowed_tools.clone(),
            approval_required: handoff.approval_required,
            readiness_score: handoff.readiness_score,
            readiness_issues: handoff.readiness_issues.clone(),
        }
    }
}

fn blocked_reason(handoff: &SubagentExecutionHandoff) -> Option<String> {
    let mut reasons = Vec::new();
    let parsed_mode = SubagentMode::parse(&handoff.mode);

    if handoff.name.trim().is_empty() {
        reasons.push("subagent name is required".to_string());
    }
    if parsed_mode.is_none() {
        reasons.push(format!("unknown subagent mode `{}`", handoff.mode));
    }
    if handoff.allowed_tools.is_empty() {
        reasons.push("no allowed tools configured".to_string());
    }
    if handoff.readiness_score != READY_SCORE {
        reasons.push(format!(
            "readiness score {} does not meet execution threshold {}",
            handoff.readiness_score, READY_SCORE
        ));
    }
    if handoff
        .allowed_tools
        .iter()
        .any(|tool| tool == APPLY_EDIT_TOOL || tool == SHELL_RUN_TOOL)
        && !handoff.approval_required
    {
        reasons.push("handoffs with mutating or shell tools must require approval".to_string());
    }
    if parsed_mode == Some(SubagentMode::ReadOnly)
        && handoff.allowed_tools.iter().any(|tool| {
            tool == APPLY_EDIT_TOOL || tool == PREVIEW_EDIT_TOOL || tool == SHELL_RUN_TOOL
        })
    {
        reasons.push("read-only handoffs cannot include write or shell tools".to_string());
    }
    if parsed_mode == Some(SubagentMode::WorkspaceWrite)
        && !handoff
            .allowed_tools
            .iter()
            .any(|tool| tool == PREVIEW_EDIT_TOOL)
    {
        reasons.push("workspace-write handoff must include preview edit capability".to_string());
    }
    if parsed_mode == Some(SubagentMode::Evaluation)
        && !handoff.approval_required
        && handoff
            .allowed_tools
            .iter()
            .any(|tool| tool == SHELL_RUN_TOOL)
    {
        reasons.push(format!(
            "evaluation handoff with `{}` must require approval",
            SHELL_RUN_TOOL
        ));
    }
    for issue in &handoff.readiness_issues {
        if !reasons.contains(issue) {
            reasons.push(issue.clone());
        }
    }

    if reasons.is_empty() {
        None
    } else {
        Some(reasons.join("; "))
    }
}

fn lifecycle_update(
    handoff: &SubagentExecutionHandoff,
    status: SubagentLifecycleStatus,
    reason: Option<String>,
) -> SubagentLifecycleUpdate {
    SubagentLifecycleUpdate {
        name: handoff.name.clone(),
        mode: handoff.mode.clone(),
        status,
        readiness_score: handoff.readiness_score,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SubagentMode, SubagentRegistry, READ_FILE_TOOL};

    #[test]
    fn blocks_unready_handoffs_before_execution_start() {
        let mut handoff = SubagentRegistry::default()
            .prepare_handoff("coder", "change a workspace file")
            .expect("handoff");
        handoff.readiness_score = 80;
        handoff
            .readiness_issues
            .push("workspace-write handoff must include preview edit capability".to_string());

        let plan = SubagentExecutionGate.plan_start(&handoff, true);

        assert_eq!(plan.status, SubagentExecutionStartStatus::Blocked);
        assert_eq!(plan.lifecycle_updates.len(), 1);
        assert_eq!(
            plan.lifecycle_updates[0].status,
            SubagentLifecycleStatus::Blocked
        );
        assert_eq!(
            plan.reason.as_deref(),
            Some(
                "readiness score 80 does not meet execution threshold 100; workspace-write handoff must include preview edit capability"
            )
        );
    }

    #[test]
    fn blocks_handoffs_with_invalid_identity_or_mode() {
        let handoff = SubagentExecutionHandoff {
            name: " ".to_string(),
            mode: "privileged".to_string(),
            allowed_tools: vec![READ_FILE_TOOL.to_string()],
            approval_required: false,
            readiness_score: 100,
            readiness_issues: Vec::new(),
        };

        let plan = SubagentExecutionGate.plan_start_for(&handoff, false);

        assert_eq!(plan.status, SubagentExecutionStartStatus::Blocked);
        assert_eq!(
            plan.reason.as_deref(),
            Some("subagent name is required; unknown subagent mode `privileged`")
        );
    }

    #[test]
    fn blocks_handoffs_with_inconsistent_tool_policy() {
        let handoff = SubagentExecutionHandoff {
            name: "reviewer".to_string(),
            mode: "read-only".to_string(),
            allowed_tools: vec![READ_FILE_TOOL.to_string(), APPLY_EDIT_TOOL.to_string()],
            approval_required: false,
            readiness_score: 100,
            readiness_issues: Vec::new(),
        };

        let plan = SubagentExecutionGate.plan_start_for(&handoff, false);

        assert_eq!(plan.status, SubagentExecutionStartStatus::Blocked);
        assert_eq!(
            plan.reason.as_deref(),
            Some(
                "handoffs with mutating or shell tools must require approval; read-only handoffs cannot include write or shell tools"
            )
        );
    }

    #[test]
    fn blocks_handoffs_with_out_of_contract_readiness_score() {
        let handoff = SubagentExecutionHandoff {
            name: "explorer".to_string(),
            mode: "read-only".to_string(),
            allowed_tools: vec![READ_FILE_TOOL.to_string()],
            approval_required: false,
            readiness_score: 101,
            readiness_issues: Vec::new(),
        };

        let plan = SubagentExecutionGate.plan_start_for(&handoff, false);

        assert_eq!(plan.status, SubagentExecutionStartStatus::Blocked);
        assert_eq!(
            plan.reason.as_deref(),
            Some("readiness score 101 does not meet execution threshold 100")
        );
    }

    #[test]
    fn awaits_approval_for_ready_write_handoffs() {
        let handoff = SubagentRegistry::default()
            .prepare_handoff("coder", "change a workspace file")
            .expect("handoff");

        let plan = SubagentExecutionGate.plan_start(&handoff, false);

        assert_eq!(handoff.mode, SubagentMode::WorkspaceWrite);
        assert!(handoff.approval_required);
        assert_eq!(plan.status, SubagentExecutionStartStatus::AwaitingApproval);
        assert_eq!(
            plan.lifecycle_updates
                .iter()
                .map(|update| update.status)
                .collect::<Vec<_>>(),
            vec![SubagentLifecycleStatus::Prepared]
        );
        assert_eq!(
            plan.reason.as_deref(),
            Some("approval required before running subagent")
        );
    }

    #[test]
    fn plans_prepared_approved_running_for_approved_handoffs() {
        let handoff = SubagentRegistry::default()
            .prepare_handoff("coder", "change a workspace file")
            .expect("handoff");

        let plan = SubagentExecutionGate.plan_start(&handoff, true);

        assert_eq!(plan.status, SubagentExecutionStartStatus::ReadyToStart);
        assert!(plan.reason.is_none());
        assert_eq!(
            plan.lifecycle_updates
                .iter()
                .map(|update| update.status)
                .collect::<Vec<_>>(),
            vec![
                SubagentLifecycleStatus::Prepared,
                SubagentLifecycleStatus::Approved,
                SubagentLifecycleStatus::Running,
            ]
        );
    }

    #[test]
    fn auto_approves_ready_read_only_handoffs() {
        let handoff = SubagentRegistry::default()
            .prepare_handoff("security-reviewer", "review command guard changes")
            .expect("handoff");

        let plan = SubagentExecutionGate.plan_start(&handoff, false);

        assert_eq!(handoff.mode, SubagentMode::ReadOnly);
        assert!(!handoff.approval_required);
        assert_eq!(plan.status, SubagentExecutionStartStatus::ReadyToStart);
        assert_eq!(
            plan.lifecycle_updates
                .iter()
                .map(|update| update.status)
                .collect::<Vec<_>>(),
            vec![
                SubagentLifecycleStatus::Prepared,
                SubagentLifecycleStatus::Approved,
                SubagentLifecycleStatus::Running,
            ]
        );
    }

    #[test]
    fn accepts_core_handoff_contracts_from_runtime() {
        let handoff = SubagentHandoffPrepared {
            name: "security-reviewer".to_string(),
            mode: "read-only".to_string(),
            approval_required: false,
            allowed_tools: vec!["filesystem.read_file".to_string()],
            timeout_ms: 60_000,
            max_context_tokens: 8_000,
            validation_checklist: vec!["Ground findings in evidence.".to_string()],
            safety_notes: vec!["Do not expose secrets.".to_string()],
            readiness_score: 100,
            readiness_issues: Vec::new(),
        };
        let execution_handoff = SubagentExecutionHandoff::from(&handoff);

        let plan = SubagentExecutionGate.plan_start_for(&execution_handoff, false);

        assert_eq!(plan.status, SubagentExecutionStartStatus::ReadyToStart);
        assert_eq!(
            plan.lifecycle_updates
                .iter()
                .map(|update| update.status)
                .collect::<Vec<_>>(),
            vec![
                SubagentLifecycleStatus::Prepared,
                SubagentLifecycleStatus::Approved,
                SubagentLifecycleStatus::Running,
            ]
        );
    }
}
