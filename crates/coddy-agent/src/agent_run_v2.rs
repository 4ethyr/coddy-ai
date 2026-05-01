use coddy_core::{AgentRunPhase, AgentRunStopReason, AgentRunSummary};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunAction {
    Plan,
    Inspect,
    Edit,
    Test,
    Review,
    Retry,
    Complete,
    Cancel {
        reason: AgentRunStopReason,
    },
    Fail {
        code: String,
        message: String,
        recoverable: bool,
    },
}

impl AgentRunAction {
    fn label(&self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Inspect => "inspect",
            Self::Edit => "edit",
            Self::Test => "test",
            Self::Review => "review",
            Self::Retry => "retry",
            Self::Complete => "complete",
            Self::Cancel { .. } => "cancel",
            Self::Fail { .. } => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunFailure {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunTransition {
    pub from: AgentRunPhase,
    pub to: AgentRunPhase,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunV2 {
    goal: String,
    phase: AgentRunPhase,
    history: Vec<AgentRunTransition>,
    stop_reason: Option<AgentRunStopReason>,
    failure: Option<AgentRunFailure>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct AgentRunTransitionError {
    code: &'static str,
    message: String,
    from: AgentRunPhase,
    action: String,
}

impl AgentRunTransitionError {
    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn from(&self) -> AgentRunPhase {
        self.from
    }

    pub fn action(&self) -> &str {
        &self.action
    }
}

impl AgentRunV2 {
    pub fn start(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            phase: AgentRunPhase::Received,
            history: Vec::new(),
            stop_reason: None,
            failure: None,
        }
    }

    pub fn goal(&self) -> &str {
        &self.goal
    }

    pub fn phase(&self) -> AgentRunPhase {
        self.phase
    }

    pub fn history(&self) -> &[AgentRunTransition] {
        &self.history
    }

    pub fn completed_steps(&self) -> usize {
        self.history.len()
    }

    pub fn stop_reason(&self) -> Option<AgentRunStopReason> {
        self.stop_reason
    }

    pub fn failure(&self) -> Option<&AgentRunFailure> {
        self.failure.as_ref()
    }

    pub fn transition(
        &mut self,
        action: AgentRunAction,
    ) -> Result<AgentRunPhase, AgentRunTransitionError> {
        if self.phase == AgentRunPhase::Cancelled
            && matches!(&action, AgentRunAction::Cancel { .. })
        {
            return Ok(self.phase);
        }

        let from = self.phase;
        let to = self.next_phase(&action)?;
        match &action {
            AgentRunAction::Cancel { reason } => {
                self.stop_reason = Some(*reason);
                self.failure = None;
            }
            AgentRunAction::Fail {
                code,
                message,
                recoverable,
            } => {
                self.failure = Some(AgentRunFailure {
                    code: code.clone(),
                    message: message.clone(),
                    recoverable: *recoverable,
                });
                self.stop_reason = None;
            }
            AgentRunAction::Retry => {
                self.failure = None;
                self.stop_reason = None;
            }
            _ => {}
        }
        self.phase = to;
        self.history.push(AgentRunTransition {
            from,
            to,
            action: action.label().to_string(),
        });
        Ok(to)
    }

    pub fn summary(&self) -> AgentRunSummary {
        AgentRunSummary {
            goal: self.goal.clone(),
            last_phase: self.phase,
            completed_steps: self.completed_steps(),
            stop_reason: self.stop_reason,
            failure_code: self.failure.as_ref().map(|failure| failure.code.clone()),
            failure_message: self.failure.as_ref().map(|failure| failure.message.clone()),
            recoverable_failure: self
                .failure
                .as_ref()
                .map(|failure| failure.recoverable)
                .unwrap_or(false),
        }
    }

    fn next_phase(
        &self,
        action: &AgentRunAction,
    ) -> Result<AgentRunPhase, AgentRunTransitionError> {
        if self.phase == AgentRunPhase::Failed && matches!(action, AgentRunAction::Retry) {
            let Some(failure) = &self.failure else {
                return Err(invalid_transition(self.phase, action));
            };
            if failure.recoverable {
                return Ok(AgentRunPhase::Planning);
            }
            return Err(invalid_transition(self.phase, action));
        }

        if self.phase.is_terminal() {
            return Err(invalid_transition(self.phase, action));
        }

        let next = match (self.phase, action) {
            (AgentRunPhase::Received, AgentRunAction::Plan) => AgentRunPhase::Planning,
            (AgentRunPhase::Planning, AgentRunAction::Inspect) => AgentRunPhase::Inspecting,
            (AgentRunPhase::Inspecting, AgentRunAction::Edit) => AgentRunPhase::Editing,
            (AgentRunPhase::Editing, AgentRunAction::Test) => AgentRunPhase::Testing,
            (AgentRunPhase::Testing, AgentRunAction::Review) => AgentRunPhase::Reviewing,
            (
                AgentRunPhase::Planning
                | AgentRunPhase::Inspecting
                | AgentRunPhase::Editing
                | AgentRunPhase::Testing
                | AgentRunPhase::Reviewing,
                AgentRunAction::Complete,
            ) => AgentRunPhase::Completed,
            (_, AgentRunAction::Cancel { .. }) => AgentRunPhase::Cancelled,
            (_, AgentRunAction::Fail { .. }) => AgentRunPhase::Failed,
            _ => return Err(invalid_transition(self.phase, action)),
        };
        Ok(next)
    }
}

fn invalid_transition(from: AgentRunPhase, action: &AgentRunAction) -> AgentRunTransitionError {
    AgentRunTransitionError {
        code: "invalid_agent_run_transition",
        message: format!(
            "cannot run agent action `{}` from phase `{from:?}`",
            action.label()
        ),
        from,
        action: action.label().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_full_coding_cycle_in_order() {
        let mut run = AgentRunV2::start("implement safer edits");

        run.transition(AgentRunAction::Plan).expect("plan");
        run.transition(AgentRunAction::Inspect).expect("inspect");
        run.transition(AgentRunAction::Edit).expect("edit");
        run.transition(AgentRunAction::Test).expect("test");
        run.transition(AgentRunAction::Review).expect("review");
        run.transition(AgentRunAction::Complete).expect("complete");

        assert_eq!(run.phase(), AgentRunPhase::Completed);
        assert_eq!(run.completed_steps(), 6);
        assert_eq!(run.summary().goal, "implement safer edits");
        assert_eq!(run.summary().last_phase, AgentRunPhase::Completed);
    }

    #[test]
    fn rejects_out_of_order_transitions() {
        let mut run = AgentRunV2::start("skip planning");

        let error = run
            .transition(AgentRunAction::Edit)
            .expect_err("edit before planning must fail");

        assert_eq!(error.code(), "invalid_agent_run_transition");
        assert_eq!(run.phase(), AgentRunPhase::Received);
        assert!(run.history().is_empty());
    }

    #[test]
    fn completes_read_only_flow_after_inspection() {
        let mut run = AgentRunV2::start("list workspace files");

        run.transition(AgentRunAction::Plan).expect("plan");
        run.transition(AgentRunAction::Inspect).expect("inspect");
        run.transition(AgentRunAction::Complete).expect("complete");

        assert_eq!(run.phase(), AgentRunPhase::Completed);
        assert_eq!(run.completed_steps(), 3);
    }

    #[test]
    fn cancellation_is_terminal_and_idempotent() {
        let mut run = AgentRunV2::start("cancel safely");

        run.transition(AgentRunAction::Plan).expect("plan");
        run.transition(AgentRunAction::Cancel {
            reason: AgentRunStopReason::UserInterrupt,
        })
        .expect("cancel");
        run.transition(AgentRunAction::Cancel {
            reason: AgentRunStopReason::UserInterrupt,
        })
        .expect("second cancel is a no-op");

        assert_eq!(run.phase(), AgentRunPhase::Cancelled);
        assert_eq!(run.history().len(), 2);
        assert_eq!(run.stop_reason(), Some(AgentRunStopReason::UserInterrupt));
    }

    #[test]
    fn failure_records_recoverability() {
        let mut run = AgentRunV2::start("recover from provider timeout");

        run.transition(AgentRunAction::Plan).expect("plan");
        run.transition(AgentRunAction::Fail {
            code: "provider_timeout".to_string(),
            message: "model provider timed out".to_string(),
            recoverable: true,
        })
        .expect("fail");

        let summary = run.summary();
        assert_eq!(summary.last_phase, AgentRunPhase::Failed);
        assert_eq!(summary.failure_code.as_deref(), Some("provider_timeout"));
        assert!(summary.recoverable_failure);
    }

    #[test]
    fn retry_returns_recoverable_failure_to_planning() {
        let mut run = AgentRunV2::start("retry provider timeout");

        run.transition(AgentRunAction::Plan).expect("plan");
        run.transition(AgentRunAction::Fail {
            code: "provider_timeout".to_string(),
            message: "model provider timed out".to_string(),
            recoverable: true,
        })
        .expect("fail");
        run.transition(AgentRunAction::Retry).expect("retry");

        assert_eq!(run.phase(), AgentRunPhase::Planning);
        assert!(run.failure().is_none());
        assert_eq!(
            run.history().last().expect("last transition").action,
            "retry"
        );
    }
}
