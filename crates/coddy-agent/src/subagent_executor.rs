use coddy_core::{SubagentLifecycleStatus, SubagentLifecycleUpdate};

use crate::SubagentHandoffPlan;

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

#[derive(Debug, Default, Clone)]
pub struct SubagentExecutionGate;

impl SubagentExecutionGate {
    pub fn plan_start(
        &self,
        handoff: &SubagentHandoffPlan,
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

fn blocked_reason(handoff: &SubagentHandoffPlan) -> Option<String> {
    let mut reasons = Vec::new();

    if handoff.readiness_score < READY_SCORE {
        reasons.push(format!(
            "readiness score {} is below execution threshold",
            handoff.readiness_score
        ));
    }
    reasons.extend(handoff.readiness_issues.iter().cloned());

    if reasons.is_empty() {
        None
    } else {
        Some(reasons.join("; "))
    }
}

fn lifecycle_update(
    handoff: &SubagentHandoffPlan,
    status: SubagentLifecycleStatus,
    reason: Option<String>,
) -> SubagentLifecycleUpdate {
    SubagentLifecycleUpdate {
        name: handoff.name.clone(),
        mode: handoff.mode.as_str().to_string(),
        status,
        readiness_score: handoff.readiness_score,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SubagentMode, SubagentRegistry};

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
                "readiness score 80 is below execution threshold; workspace-write handoff must include preview edit capability"
            )
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
}
