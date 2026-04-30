use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentExecutionOutcomeStatus {
    Completed,
    Failed,
    Blocked,
    AwaitingApproval,
}

impl SubagentExecutionOutcomeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::AwaitingApproval => "awaiting-approval",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentExecutionOutputStatus {
    Accepted,
    Rejected,
    Missing,
    NotRequested,
}

impl SubagentExecutionOutputStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Missing => "missing",
            Self::NotRequested => "not-requested",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentExecutionRecord {
    pub name: String,
    pub mode: String,
    pub start_status: SubagentExecutionStartStatus,
    pub outcome_status: SubagentExecutionOutcomeStatus,
    pub output_status: SubagentExecutionOutputStatus,
    pub accepted: bool,
    pub missing_fields: Vec<String>,
    pub unexpected_fields: Vec<String>,
    pub reason: Option<String>,
    pub lifecycle_updates: Vec<SubagentLifecycleUpdate>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SubagentExecutionSummary {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub blocked: usize,
    pub awaiting_approval: usize,
    pub accepted_outputs: usize,
    pub rejected_outputs: usize,
    pub missing_outputs: usize,
    pub unexpected_outputs: Vec<String>,
    pub accepted_output_values: BTreeMap<String, Value>,
    pub lifecycle_updates: Vec<SubagentLifecycleUpdate>,
    pub records: Vec<SubagentExecutionRecord>,
}

impl SubagentExecutionSummary {
    fn push_record(&mut self, record: SubagentExecutionRecord) {
        self.total += 1;
        match record.outcome_status {
            SubagentExecutionOutcomeStatus::Completed => self.completed += 1,
            SubagentExecutionOutcomeStatus::Failed => self.failed += 1,
            SubagentExecutionOutcomeStatus::Blocked => self.blocked += 1,
            SubagentExecutionOutcomeStatus::AwaitingApproval => self.awaiting_approval += 1,
        }

        match record.output_status {
            SubagentExecutionOutputStatus::Accepted => self.accepted_outputs += 1,
            SubagentExecutionOutputStatus::Rejected => self.rejected_outputs += 1,
            SubagentExecutionOutputStatus::Missing => self.missing_outputs += 1,
            SubagentExecutionOutputStatus::NotRequested => {}
        }

        self.lifecycle_updates
            .extend(record.lifecycle_updates.iter().cloned());
        self.records.push(record);
    }
}

#[derive(Debug, Default, Clone)]
pub struct SubagentExecutionCoordinator {
    gate: SubagentExecutionGate,
}

impl SubagentExecutionCoordinator {
    pub fn new(gate: SubagentExecutionGate) -> Self {
        Self { gate }
    }

    pub fn reduce_handoffs(
        &self,
        handoffs: &[SubagentHandoffPlan],
        outputs: &BTreeMap<String, Value>,
        approved_subagents: &BTreeSet<String>,
    ) -> SubagentExecutionSummary {
        let expected_names = handoffs
            .iter()
            .map(|handoff| handoff.name.clone())
            .collect::<BTreeSet<_>>();
        let mut summary = SubagentExecutionSummary {
            unexpected_outputs: outputs
                .keys()
                .filter(|name| !expected_names.contains(*name))
                .cloned()
                .collect(),
            ..SubagentExecutionSummary::default()
        };

        for handoff in handoffs {
            let execution_handoff = SubagentExecutionHandoff::from(handoff);
            let start_plan = self.gate.plan_start_for(
                &execution_handoff,
                approved_subagents.contains(&handoff.name),
            );
            let mut lifecycle_updates = start_plan.lifecycle_updates.clone();

            match start_plan.status {
                SubagentExecutionStartStatus::Blocked => {
                    summary.push_record(SubagentExecutionRecord {
                        name: handoff.name.clone(),
                        mode: handoff.mode.as_str().to_string(),
                        start_status: start_plan.status,
                        outcome_status: SubagentExecutionOutcomeStatus::Blocked,
                        output_status: SubagentExecutionOutputStatus::NotRequested,
                        accepted: false,
                        missing_fields: Vec::new(),
                        unexpected_fields: Vec::new(),
                        reason: start_plan.reason,
                        lifecycle_updates,
                    });
                }
                SubagentExecutionStartStatus::AwaitingApproval => {
                    summary.push_record(SubagentExecutionRecord {
                        name: handoff.name.clone(),
                        mode: handoff.mode.as_str().to_string(),
                        start_status: start_plan.status,
                        outcome_status: SubagentExecutionOutcomeStatus::AwaitingApproval,
                        output_status: SubagentExecutionOutputStatus::NotRequested,
                        accepted: false,
                        missing_fields: Vec::new(),
                        unexpected_fields: Vec::new(),
                        reason: start_plan.reason,
                        lifecycle_updates,
                    });
                }
                SubagentExecutionStartStatus::ReadyToStart => {
                    let Some(output) = outputs.get(&handoff.name) else {
                        let reason = "missing subagent output".to_string();
                        lifecycle_updates.push(SubagentLifecycleUpdate {
                            name: handoff.name.clone(),
                            mode: handoff.mode.as_str().to_string(),
                            status: SubagentLifecycleStatus::Failed,
                            readiness_score: handoff.readiness_score,
                            reason: Some(reason.clone()),
                        });
                        summary.push_record(SubagentExecutionRecord {
                            name: handoff.name.clone(),
                            mode: handoff.mode.as_str().to_string(),
                            start_status: start_plan.status,
                            outcome_status: SubagentExecutionOutcomeStatus::Failed,
                            output_status: SubagentExecutionOutputStatus::Missing,
                            accepted: false,
                            missing_fields: Vec::new(),
                            unexpected_fields: Vec::new(),
                            reason: Some(reason),
                            lifecycle_updates,
                        });
                        continue;
                    };

                    let contract = SubagentOutputContract::from(handoff);
                    let completion_plan = self.gate.plan_completion(&contract, output);
                    lifecycle_updates.push(completion_plan.lifecycle_update.clone());

                    let output_status = if completion_plan.accepted {
                        SubagentExecutionOutputStatus::Accepted
                    } else {
                        SubagentExecutionOutputStatus::Rejected
                    };
                    let outcome_status = if completion_plan.accepted {
                        summary
                            .accepted_output_values
                            .insert(handoff.name.clone(), output.clone());
                        SubagentExecutionOutcomeStatus::Completed
                    } else {
                        SubagentExecutionOutcomeStatus::Failed
                    };

                    summary.push_record(SubagentExecutionRecord {
                        name: handoff.name.clone(),
                        mode: handoff.mode.as_str().to_string(),
                        start_status: start_plan.status,
                        outcome_status,
                        output_status,
                        accepted: completion_plan.accepted,
                        missing_fields: completion_plan.missing_fields,
                        unexpected_fields: completion_plan.unexpected_fields,
                        reason: completion_plan.reason,
                        lifecycle_updates,
                    });
                }
            }
        }

        summary
    }
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

impl From<&SubagentHandoffPrepared> for SubagentOutputContract {
    fn from(handoff: &SubagentHandoffPrepared) -> Self {
        Self {
            name: handoff.name.clone(),
            mode: handoff.mode.clone(),
            readiness_score: handoff.readiness_score,
            required_fields: handoff.required_output_fields.clone(),
            additional_properties_allowed: handoff.output_additional_properties_allowed,
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
    use std::collections::{BTreeMap, BTreeSet};

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
            output_additional_properties_allowed: false,
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
    fn builds_output_contract_from_core_handoff() {
        let handoff = SubagentHandoffPrepared {
            name: "explorer".to_string(),
            mode: "read-only".to_string(),
            approval_required: false,
            allowed_tools: vec![READ_FILE_TOOL.to_string()],
            required_output_fields: vec!["summary".to_string(), "risks".to_string()],
            output_additional_properties_allowed: false,
            timeout_ms: 60_000,
            max_context_tokens: 8_000,
            validation_checklist: vec!["Ground findings in repository evidence.".to_string()],
            safety_notes: vec!["Do not expose secrets.".to_string()],
            readiness_score: 100,
            readiness_issues: Vec::new(),
        };

        let contract = SubagentOutputContract::from(&handoff);
        let plan = SubagentExecutionGate.plan_completion(
            &contract,
            &json!({
                "summary": "Mapped the repository.",
                "risks": [],
                "extra": "not allowed"
            }),
        );

        assert!(!plan.accepted);
        assert_eq!(plan.unexpected_fields, vec!["extra".to_string()]);
        assert_eq!(
            plan.reason.as_deref(),
            Some("unexpected output fields: extra")
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

    #[test]
    fn coordinator_reduces_ready_handoffs_into_consolidated_summary() {
        let registry = SubagentRegistry::default();
        let explorer = registry
            .prepare_handoff("explorer", "map repository context")
            .expect("explorer handoff");
        let security_reviewer = registry
            .prepare_handoff("security-reviewer", "review secret handling")
            .expect("security handoff");
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "explorer".to_string(),
            json!({
                "summary": "Mapped repository entrypoints.",
                "importantFiles": ["crates/coddy-agent/src/subagent_executor.rs"],
                "entrypoints": ["coddy"],
                "testFiles": ["subagent_executor.rs"],
                "commands": ["cargo test -p coddy-agent subagent_executor"],
                "risks": [],
                "recommendations": []
            }),
        );
        outputs.insert(
            "security-reviewer".to_string(),
            json!({
                "riskLevel": "low",
                "findings": [],
                "requiredFixes": [],
                "recommendations": []
            }),
        );
        outputs.insert("orphan".to_string(), json!({"summary": "ignored"}));

        let summary = SubagentExecutionCoordinator::default().reduce_handoffs(
            &[explorer, security_reviewer],
            &outputs,
            &BTreeSet::new(),
        );

        assert_eq!(summary.total, 2);
        assert_eq!(summary.completed, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.blocked, 0);
        assert_eq!(summary.awaiting_approval, 0);
        assert_eq!(summary.accepted_outputs, 2);
        assert_eq!(summary.rejected_outputs, 0);
        assert_eq!(summary.missing_outputs, 0);
        assert_eq!(summary.unexpected_outputs, vec!["orphan".to_string()]);
        assert_eq!(
            summary
                .accepted_output_values
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["explorer".to_string(), "security-reviewer".to_string()]
        );
        assert_eq!(summary.lifecycle_updates.len(), 8);
        assert!(summary.records.iter().all(|record| {
            record.start_status == SubagentExecutionStartStatus::ReadyToStart
                && record.outcome_status == SubagentExecutionOutcomeStatus::Completed
                && record.output_status == SubagentExecutionOutputStatus::Accepted
                && record.accepted
        }));
    }

    #[test]
    fn coordinator_reports_rejected_and_missing_outputs() {
        let registry = SubagentRegistry::default();
        let reviewer = registry
            .prepare_handoff("reviewer", "review the current diff")
            .expect("reviewer handoff");
        let coder = registry
            .prepare_handoff("coder", "implement a parser fix")
            .expect("coder handoff");
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "reviewer".to_string(),
            json!({
                "approved": false,
                "issues": [],
                "suggestions": [],
                "extraNarrative": "not part of the contract"
            }),
        );
        let approvals = BTreeSet::from(["coder".to_string()]);

        let summary = SubagentExecutionCoordinator::default().reduce_handoffs(
            &[reviewer, coder],
            &outputs,
            &approvals,
        );

        assert_eq!(summary.total, 2);
        assert_eq!(summary.completed, 0);
        assert_eq!(summary.failed, 2);
        assert_eq!(summary.rejected_outputs, 1);
        assert_eq!(summary.missing_outputs, 1);
        assert_eq!(summary.accepted_outputs, 0);
        assert!(summary.accepted_output_values.is_empty());
        assert_eq!(
            summary
                .records
                .iter()
                .map(|record| (record.name.as_str(), record.output_status))
                .collect::<Vec<_>>(),
            vec![
                ("reviewer", SubagentExecutionOutputStatus::Rejected),
                ("coder", SubagentExecutionOutputStatus::Missing),
            ]
        );
        assert_eq!(
            summary.records[0].reason.as_deref(),
            Some(
                "missing required output fields: blockingProblems, nonBlockingProblems; unexpected output fields: extraNarrative"
            )
        );
        assert_eq!(
            summary.records[1].reason.as_deref(),
            Some("missing subagent output")
        );
    }

    #[test]
    fn coordinator_does_not_accept_outputs_before_required_approval() {
        let coder = SubagentRegistry::default()
            .prepare_handoff("coder", "implement a guarded change")
            .expect("coder handoff");
        let mut outputs = BTreeMap::new();
        outputs.insert(
            "coder".to_string(),
            json!({
                "changedFiles": ["src/lib.rs"],
                "summary": "Implemented a change.",
                "testsAdded": [],
                "risks": [],
                "nextSteps": []
            }),
        );

        let summary = SubagentExecutionCoordinator::default().reduce_handoffs(
            &[coder],
            &outputs,
            &BTreeSet::new(),
        );

        assert_eq!(summary.total, 1);
        assert_eq!(summary.completed, 0);
        assert_eq!(summary.awaiting_approval, 1);
        assert_eq!(summary.accepted_outputs, 0);
        assert!(summary.accepted_output_values.is_empty());
        assert_eq!(
            summary.records[0].start_status,
            SubagentExecutionStartStatus::AwaitingApproval
        );
        assert_eq!(
            summary.records[0].outcome_status,
            SubagentExecutionOutcomeStatus::AwaitingApproval
        );
        assert_eq!(
            summary.records[0].output_status,
            SubagentExecutionOutputStatus::NotRequested
        );
        assert_eq!(
            summary.records[0].reason.as_deref(),
            Some("approval required before running subagent")
        );
    }
}
