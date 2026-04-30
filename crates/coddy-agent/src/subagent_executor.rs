use coddy_core::{SubagentHandoffPrepared, SubagentLifecycleStatus, SubagentLifecycleUpdate};
use serde_json::Value;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentOutputContract {
    pub name: String,
    pub mode: String,
    pub readiness_score: u8,
    pub required_fields: Vec<String>,
    pub additional_properties_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentExecutionCompletionPlan {
    pub accepted: bool,
    pub missing_fields: Vec<String>,
    pub unexpected_fields: Vec<String>,
    pub reason: Option<String>,
    pub lifecycle_update: SubagentLifecycleUpdate,
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

    pub fn plan_completion(
        &self,
        contract: &SubagentOutputContract,
        output: &Value,
    ) -> SubagentExecutionCompletionPlan {
        let validation = validate_output_contract(contract, output);
        let accepted = validation.reason.is_none();
        let status = if accepted {
            SubagentLifecycleStatus::Completed
        } else {
            SubagentLifecycleStatus::Failed
        };
        let lifecycle_update = SubagentLifecycleUpdate {
            name: contract.name.clone(),
            mode: contract.mode.clone(),
            status,
            readiness_score: contract.readiness_score,
            reason: validation.reason.clone(),
        };

        SubagentExecutionCompletionPlan {
            accepted,
            missing_fields: validation.missing_fields,
            unexpected_fields: validation.unexpected_fields,
            reason: validation.reason,
            lifecycle_update,
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

impl From<&SubagentHandoffPlan> for SubagentOutputContract {
    fn from(handoff: &SubagentHandoffPlan) -> Self {
        Self {
            name: handoff.name.clone(),
            mode: handoff.mode.as_str().to_string(),
            readiness_score: handoff.readiness_score,
            required_fields: required_output_fields(&handoff.output_schema),
            additional_properties_allowed: additional_properties_allowed(&handoff.output_schema),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct OutputValidation {
    missing_fields: Vec<String>,
    unexpected_fields: Vec<String>,
    reason: Option<String>,
}

fn validate_output_contract(contract: &SubagentOutputContract, output: &Value) -> OutputValidation {
    let Some(object) = output.as_object() else {
        return OutputValidation {
            missing_fields: contract.required_fields.clone(),
            unexpected_fields: Vec::new(),
            reason: Some("subagent output must be a JSON object".to_string()),
        };
    };

    let mut reasons = Vec::new();
    let missing_fields = contract
        .required_fields
        .iter()
        .filter(|field| !object.contains_key(*field))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_fields.is_empty() {
        reasons.push(format!(
            "missing required output fields: {}",
            missing_fields.join(", ")
        ));
    }

    let unexpected_fields = if contract.additional_properties_allowed {
        Vec::new()
    } else {
        object
            .keys()
            .filter(|field| !contract.required_fields.contains(field))
            .cloned()
            .collect::<Vec<_>>()
    };
    if !unexpected_fields.is_empty() {
        reasons.push(format!(
            "unexpected output fields: {}",
            unexpected_fields.join(", ")
        ));
    }

    if contract.readiness_score != READY_SCORE {
        reasons.push(format!(
            "readiness score {} does not meet completion threshold {}",
            contract.readiness_score, READY_SCORE
        ));
    }

    OutputValidation {
        missing_fields,
        unexpected_fields,
        reason: if reasons.is_empty() {
            None
        } else {
            Some(reasons.join("; "))
        },
    }
}

fn required_output_fields(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|fields| {
            fields
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn additional_properties_allowed(schema: &Value) -> bool {
    schema
        .get("additionalProperties")
        .and_then(Value::as_bool)
        .unwrap_or(true)
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
    use serde_json::json;

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
            required_output_fields: vec!["summary".to_string()],
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

    #[test]
    fn accepts_subagent_completion_that_matches_output_contract() {
        let handoff = SubagentRegistry::default()
            .prepare_handoff("coder", "implement a focused parser fix")
            .expect("handoff");
        let contract = SubagentOutputContract::from(&handoff);
        let output = json!({
            "changedFiles": ["src/parser.rs"],
            "summary": "Implemented the parser fix.",
            "testsAdded": ["parser_handles_edge_case"],
            "risks": [],
            "nextSteps": []
        });

        let plan = SubagentExecutionGate.plan_completion(&contract, &output);

        assert!(plan.accepted);
        assert!(plan.reason.is_none());
        assert!(plan.missing_fields.is_empty());
        assert!(plan.unexpected_fields.is_empty());
        assert_eq!(
            plan.lifecycle_update.status,
            SubagentLifecycleStatus::Completed
        );
    }

    #[test]
    fn fails_subagent_completion_with_missing_or_unexpected_output_fields() {
        let handoff = SubagentRegistry::default()
            .prepare_handoff("reviewer", "review the current diff")
            .expect("handoff");
        let contract = SubagentOutputContract::from(&handoff);
        let output = json!({
            "approved": false,
            "issues": [],
            "suggestions": [],
            "extraNarrative": "free-form output should stay out of structured reports"
        });

        let plan = SubagentExecutionGate.plan_completion(&contract, &output);

        assert!(!plan.accepted);
        assert_eq!(
            plan.missing_fields,
            vec![
                "blockingProblems".to_string(),
                "nonBlockingProblems".to_string()
            ]
        );
        assert_eq!(plan.unexpected_fields, vec!["extraNarrative".to_string()]);
        assert_eq!(
            plan.reason.as_deref(),
            Some(
                "missing required output fields: blockingProblems, nonBlockingProblems; unexpected output fields: extraNarrative"
            )
        );
        assert_eq!(
            plan.lifecycle_update.status,
            SubagentLifecycleStatus::Failed
        );
    }
}
