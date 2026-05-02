use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ReplMode {
    FloatingTerminal,
    DesktopApp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SessionStatus {
    Idle,
    Listening,
    Transcribing,
    CapturingScreen,
    BuildingContext,
    Thinking,
    Streaming,
    Speaking,
    AwaitingConfirmation,
    AwaitingToolApproval,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceState {
    pub enabled: bool,
    pub speaking: bool,
    pub muted: bool,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            enabled: true,
            speaking: false,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextItem {
    pub id: String,
    pub label: String,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplMessage {
    pub id: Uuid,
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubagentActivity {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub status: crate::SubagentLifecycleStatus,
    pub readiness_score: u8,
    #[serde(default)]
    pub required_output_fields: Vec<String>,
    #[serde(default = "default_true")]
    pub output_additional_properties_allowed: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRunActivity {
    pub run_id: Uuid,
    pub summary: crate::AgentRunSummary,
}

const SUBAGENT_READY_SCORE: u8 = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplSession {
    pub id: Uuid,
    pub mode: ReplMode,
    pub status: SessionStatus,
    pub policy: crate::AssessmentPolicy,
    pub selected_model: crate::ModelRef,
    pub voice: VoiceState,
    pub screen_context: Option<crate::ScreenUnderstandingContext>,
    pub workspace_context: Vec<ContextItem>,
    pub messages: Vec<ReplMessage>,
    pub active_run: Option<Uuid>,
    pub pending_permission: Option<crate::PermissionRequest>,
    #[serde(default)]
    pub agent_run: Option<AgentRunActivity>,
    pub subagent_activity: Vec<SubagentActivity>,
}

impl ReplSession {
    pub fn new(mode: ReplMode, selected_model: crate::ModelRef) -> Self {
        Self {
            id: Uuid::new_v4(),
            mode,
            status: SessionStatus::Idle,
            policy: crate::AssessmentPolicy::UnknownAssessment,
            selected_model,
            voice: VoiceState::default(),
            screen_context: None,
            workspace_context: Vec::new(),
            messages: Vec::new(),
            active_run: None,
            pending_permission: None,
            agent_run: None,
            subagent_activity: Vec::new(),
        }
    }

    pub fn transition_to(&mut self, status: SessionStatus) {
        self.status = status;
    }

    pub fn apply_event(&mut self, event: &crate::ReplEvent) {
        match event {
            crate::ReplEvent::SessionStarted { session_id } => {
                let mode = self.mode;
                let selected_model = self.selected_model.clone();
                *self = Self::new(mode, selected_model);
                self.id = *session_id;
            }
            crate::ReplEvent::RunStarted { run_id } => {
                self.active_run = Some(*run_id);
                self.agent_run = None;
                self.subagent_activity.clear();
                self.status = SessionStatus::Thinking;
            }
            crate::ReplEvent::ShortcutTriggered { .. } => {}
            crate::ReplEvent::OverlayShown { mode } => {
                self.mode = *mode;
            }
            crate::ReplEvent::VoiceListeningStarted => {
                self.status = SessionStatus::Listening;
            }
            crate::ReplEvent::VoiceTranscriptPartial { .. } => {
                self.status = SessionStatus::Transcribing;
            }
            crate::ReplEvent::VoiceTranscriptFinal { .. } => {
                self.status = SessionStatus::Thinking;
            }
            crate::ReplEvent::ScreenCaptured { .. } => {
                self.status = SessionStatus::CapturingScreen;
            }
            crate::ReplEvent::OcrCompleted { .. } => {
                self.status = SessionStatus::BuildingContext;
            }
            crate::ReplEvent::IntentDetected { .. } => {
                self.status = SessionStatus::Thinking;
            }
            crate::ReplEvent::AgentRunUpdated { run_id, summary } => {
                self.agent_run = Some(AgentRunActivity {
                    run_id: *run_id,
                    summary: summary.clone(),
                });
            }
            crate::ReplEvent::PolicyEvaluated { policy, allowed } => {
                self.policy = *policy;
                if !allowed && *policy == crate::AssessmentPolicy::UnknownAssessment {
                    self.status = SessionStatus::AwaitingConfirmation;
                }
            }
            crate::ReplEvent::ConfirmationDismissed => {
                if self.status == SessionStatus::AwaitingConfirmation {
                    self.status = SessionStatus::Idle;
                }
            }
            crate::ReplEvent::ModelSelected { model, role } => {
                if *role == crate::ModelRole::Chat {
                    self.selected_model = model.clone();
                }
            }
            crate::ReplEvent::SearchStarted { .. } => {
                self.status = SessionStatus::Thinking;
            }
            crate::ReplEvent::SearchContextExtracted { .. } => {
                self.status = SessionStatus::BuildingContext;
            }
            crate::ReplEvent::ContextItemAdded { item } => {
                if let Some(existing) = self
                    .workspace_context
                    .iter_mut()
                    .find(|existing| existing.id == item.id)
                {
                    *existing = item.clone();
                } else {
                    self.workspace_context.push(item.clone());
                }
                self.status = SessionStatus::BuildingContext;
            }
            crate::ReplEvent::TokenDelta { run_id, .. } => {
                self.active_run.get_or_insert(*run_id);
                self.status = SessionStatus::Streaming;
            }
            crate::ReplEvent::MessageAppended { message } => {
                self.messages.push(message.clone());
            }
            crate::ReplEvent::ToolStarted { .. } => {
                self.status = SessionStatus::Thinking;
            }
            crate::ReplEvent::ToolCompleted { .. } => {
                self.status = SessionStatus::Thinking;
            }
            crate::ReplEvent::SubagentRouted { .. } => {
                self.status = SessionStatus::Thinking;
            }
            crate::ReplEvent::SubagentHandoffPrepared { handoff } => {
                let existing_index = self
                    .subagent_activity
                    .iter()
                    .position(|existing| existing.id == SubagentActivity::id_for_handoff(handoff));
                let previous = existing_index.and_then(|index| self.subagent_activity.get(index));
                let activity = SubagentActivity::from_handoff_prepared(previous, handoff);
                if let Some(index) = existing_index {
                    self.subagent_activity[index] = activity;
                } else {
                    self.subagent_activity.push(activity);
                }
                self.status = SessionStatus::Thinking;
            }
            crate::ReplEvent::SubagentLifecycleUpdated { update } => {
                let existing_index = self
                    .subagent_activity
                    .iter()
                    .position(|existing| existing.id == SubagentActivity::id_for(update));
                let previous = existing_index.and_then(|index| self.subagent_activity.get(index));
                let activity = SubagentActivity::from_lifecycle_update(previous, update);
                if let Some(index) = existing_index {
                    self.subagent_activity[index] = activity;
                } else {
                    self.subagent_activity.push(activity);
                }
                self.status = SessionStatus::Thinking;
            }
            crate::ReplEvent::PermissionRequested { request } => {
                self.pending_permission = Some(request.clone());
                self.status = SessionStatus::AwaitingToolApproval;
            }
            crate::ReplEvent::PermissionReplied { request_id, .. } => {
                if self
                    .pending_permission
                    .as_ref()
                    .is_some_and(|request| request.id == *request_id)
                {
                    self.pending_permission = None;
                }
                if self.status == SessionStatus::AwaitingToolApproval {
                    self.status = if self.active_run.is_some() {
                        SessionStatus::Thinking
                    } else {
                        SessionStatus::Idle
                    };
                }
            }
            crate::ReplEvent::TtsQueued => {}
            crate::ReplEvent::TtsStarted => {
                self.voice.speaking = true;
                self.status = SessionStatus::Speaking;
            }
            crate::ReplEvent::TtsCompleted => {
                self.voice.speaking = false;
                self.status = if self.active_run.is_some() {
                    SessionStatus::Streaming
                } else {
                    SessionStatus::Idle
                };
            }
            crate::ReplEvent::RunCompleted { run_id } => {
                if self.active_run == Some(*run_id) {
                    self.active_run = None;
                }
                self.status = if self.pending_permission.is_some() {
                    SessionStatus::AwaitingToolApproval
                } else if self.voice.speaking {
                    SessionStatus::Speaking
                } else {
                    SessionStatus::Idle
                };
            }
            crate::ReplEvent::Error { .. } => {
                self.status = SessionStatus::Error;
            }
        }
    }
}

impl SubagentActivity {
    fn from_handoff_prepared(
        previous: Option<&Self>,
        handoff: &crate::SubagentHandoffPrepared,
    ) -> Self {
        let reason = handoff_readiness_reason(handoff);
        let status = reason
            .as_ref()
            .map(|_| crate::SubagentLifecycleStatus::Blocked)
            .unwrap_or_else(|| {
                previous
                    .map(|activity| activity.status)
                    .unwrap_or(crate::SubagentLifecycleStatus::Prepared)
            });

        Self {
            id: Self::id_for_handoff(handoff),
            name: handoff.name.clone(),
            mode: handoff.mode.clone(),
            status,
            readiness_score: handoff.readiness_score,
            required_output_fields: handoff.required_output_fields.clone(),
            output_additional_properties_allowed: handoff.output_additional_properties_allowed,
            reason: reason.or_else(|| previous.and_then(|activity| activity.reason.clone())),
        }
    }

    fn from_lifecycle_update(
        previous: Option<&Self>,
        update: &crate::SubagentLifecycleUpdate,
    ) -> Self {
        let (status, reason) = normalize_subagent_lifecycle_transition(
            previous.map(|activity| activity.status),
            update,
        );

        Self {
            id: Self::id_for(update),
            name: update.name.clone(),
            mode: update.mode.clone(),
            status,
            readiness_score: update.readiness_score,
            required_output_fields: previous
                .map(|activity| activity.required_output_fields.clone())
                .unwrap_or_default(),
            output_additional_properties_allowed: previous
                .map(|activity| activity.output_additional_properties_allowed)
                .unwrap_or(true),
            reason,
        }
    }

    fn id_for(update: &crate::SubagentLifecycleUpdate) -> String {
        format!("{}:{}", update.name, update.mode)
    }

    fn id_for_handoff(handoff: &crate::SubagentHandoffPrepared) -> String {
        format!("{}:{}", handoff.name, handoff.mode)
    }
}

fn default_true() -> bool {
    true
}

fn handoff_readiness_reason(handoff: &crate::SubagentHandoffPrepared) -> Option<String> {
    let mut reasons = Vec::new();
    if handoff.readiness_score < SUBAGENT_READY_SCORE {
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

fn normalize_subagent_lifecycle_transition(
    previous: Option<crate::SubagentLifecycleStatus>,
    update: &crate::SubagentLifecycleUpdate,
) -> (crate::SubagentLifecycleStatus, Option<String>) {
    if let Some(reason) = readiness_block_reason(update) {
        return (crate::SubagentLifecycleStatus::Blocked, Some(reason));
    }

    if !is_allowed_subagent_transition(previous, update.status) {
        let previous_label = previous
            .map(|status| format!("{status:?}"))
            .unwrap_or_else(|| "None".to_string());
        return (
            crate::SubagentLifecycleStatus::Blocked,
            Some(format!(
                "invalid subagent lifecycle transition: {previous_label} -> {:?}",
                update.status
            )),
        );
    }

    (update.status, update.reason.clone())
}

fn readiness_block_reason(update: &crate::SubagentLifecycleUpdate) -> Option<String> {
    if !requires_ready_handoff(update.status) {
        return None;
    }

    let mut reasons = Vec::new();
    if update.readiness_score < SUBAGENT_READY_SCORE {
        reasons.push(format!(
            "readiness score {} is below execution threshold",
            update.readiness_score
        ));
    }
    if let Some(reason) = update.reason.as_deref().filter(|reason| !reason.is_empty()) {
        reasons.push(reason.to_string());
    }

    if reasons.is_empty() {
        None
    } else {
        Some(reasons.join("; "))
    }
}

fn requires_ready_handoff(status: crate::SubagentLifecycleStatus) -> bool {
    matches!(
        status,
        crate::SubagentLifecycleStatus::Prepared
            | crate::SubagentLifecycleStatus::Approved
            | crate::SubagentLifecycleStatus::Running
            | crate::SubagentLifecycleStatus::Completed
    )
}

fn is_allowed_subagent_transition(
    previous: Option<crate::SubagentLifecycleStatus>,
    next: crate::SubagentLifecycleStatus,
) -> bool {
    use crate::SubagentLifecycleStatus::{Approved, Blocked, Completed, Failed, Prepared, Running};

    match (previous, next) {
        (None, Prepared | Blocked) => true,
        (Some(current), next) if current == next => true,
        (Some(Prepared), Approved | Blocked) => true,
        (Some(Approved), Running | Blocked) => true,
        (Some(Running), Completed | Failed | Blocked) => true,
        (Some(Completed | Failed | Blocked), _) => false,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_item_added_upserts_workspace_context() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
        );
        let first = ContextItem {
            id: "tool:filesystem.read_file:src/main.rs".to_string(),
            label: "filesystem.read_file: src/main.rs".to_string(),
            sensitive: false,
        };
        let updated = ContextItem {
            id: first.id.clone(),
            label: "filesystem.read_file: src/main.rs".to_string(),
            sensitive: true,
        };

        session.apply_event(&crate::ReplEvent::ContextItemAdded { item: first });
        session.apply_event(&crate::ReplEvent::ContextItemAdded { item: updated });

        assert_eq!(session.workspace_context.len(), 1);
        assert!(session.workspace_context[0].sensitive);
        assert_eq!(session.status, SessionStatus::BuildingContext);
    }

    #[test]
    fn session_started_resets_conversation_state_but_keeps_ui_context() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openrouter".to_string(),
                name: "deepseek/deepseek-v4-flash".to_string(),
            },
        );
        session.messages.push(crate::ReplMessage {
            id: uuid::Uuid::new_v4(),
            role: "user".to_string(),
            text: "old prompt".to_string(),
        });
        session.workspace_context.push(ContextItem {
            id: "tool:filesystem.read_file:Cargo.toml".to_string(),
            label: "Cargo.toml".to_string(),
            sensitive: false,
        });
        session.pending_permission = Some(
            crate::PermissionRequest::new(
                session.id,
                uuid::Uuid::new_v4(),
                None,
                crate::ToolName::new("filesystem.read_file").expect("valid tool name"),
                crate::ToolPermission::ReadWorkspace,
                vec![".env".to_string()],
                crate::ToolRiskLevel::High,
                serde_json::json!({"path": ".env"}),
                1_777_000_000_000,
            )
            .expect("valid permission request"),
        );
        let new_session_id = uuid::Uuid::new_v4();

        session.apply_event(&crate::ReplEvent::SessionStarted {
            session_id: new_session_id,
        });

        assert_eq!(session.id, new_session_id);
        assert_eq!(session.mode, ReplMode::DesktopApp);
        assert_eq!(session.selected_model.provider, "openrouter");
        assert_eq!(session.status, SessionStatus::Idle);
        assert!(session.messages.is_empty());
        assert!(session.workspace_context.is_empty());
        assert!(session.pending_permission.is_none());
    }

    #[test]
    fn subagent_lifecycle_update_upserts_activity() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
        );

        session.apply_event(&crate::ReplEvent::SubagentLifecycleUpdated {
            update: crate::SubagentLifecycleUpdate {
                name: "coder".to_string(),
                mode: "workspace-write".to_string(),
                status: crate::SubagentLifecycleStatus::Prepared,
                readiness_score: 100,
                reason: None,
            },
        });
        session.apply_event(&crate::ReplEvent::SubagentLifecycleUpdated {
            update: crate::SubagentLifecycleUpdate {
                name: "coder".to_string(),
                mode: "workspace-write".to_string(),
                status: crate::SubagentLifecycleStatus::Blocked,
                readiness_score: 80,
                reason: Some(
                    "workspace-write handoff must include preview edit capability".to_string(),
                ),
            },
        });

        assert_eq!(session.subagent_activity.len(), 1);
        assert_eq!(session.subagent_activity[0].id, "coder:workspace-write");
        assert_eq!(
            session.subagent_activity[0].status,
            crate::SubagentLifecycleStatus::Blocked
        );
        assert_eq!(session.subagent_activity[0].readiness_score, 80);
        assert_eq!(session.status, SessionStatus::Thinking);
    }

    #[test]
    fn subagent_handoff_prepared_preserves_output_contract_in_activity() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
        );

        session.apply_event(&crate::ReplEvent::SubagentHandoffPrepared {
            handoff: crate::SubagentHandoffPrepared {
                name: "eval-runner".to_string(),
                mode: "evaluation".to_string(),
                approval_required: true,
                allowed_tools: vec!["shell.run".to_string()],
                required_output_fields: vec![
                    "score".to_string(),
                    "passed".to_string(),
                    "failedChecks".to_string(),
                ],
                output_additional_properties_allowed: false,
                timeout_ms: 60_000,
                max_context_tokens: 8_000,
                validation_checklist: vec!["Report deterministic metrics.".to_string()],
                safety_notes: vec!["Do not expose secrets.".to_string()],
                readiness_score: 100,
                readiness_issues: Vec::new(),
            },
        });
        session.apply_event(&crate::ReplEvent::SubagentLifecycleUpdated {
            update: crate::SubagentLifecycleUpdate {
                name: "eval-runner".to_string(),
                mode: "evaluation".to_string(),
                status: crate::SubagentLifecycleStatus::Prepared,
                readiness_score: 100,
                reason: None,
            },
        });

        assert_eq!(session.subagent_activity.len(), 1);
        assert_eq!(
            session.subagent_activity[0].required_output_fields,
            vec![
                "score".to_string(),
                "passed".to_string(),
                "failedChecks".to_string()
            ]
        );
        assert!(!session.subagent_activity[0].output_additional_properties_allowed);
        assert_eq!(
            session.subagent_activity[0].status,
            crate::SubagentLifecycleStatus::Prepared
        );
    }

    #[test]
    fn run_started_clears_subagent_activity_for_next_run() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
        );
        let run_id = Uuid::new_v4();

        session.apply_event(&crate::ReplEvent::SubagentLifecycleUpdated {
            update: crate::SubagentLifecycleUpdate {
                name: "eval-runner".to_string(),
                mode: "evaluation".to_string(),
                status: crate::SubagentLifecycleStatus::Prepared,
                readiness_score: 100,
                reason: None,
            },
        });
        session.apply_event(&crate::ReplEvent::RunStarted { run_id });

        assert!(session.subagent_activity.is_empty());
        assert_eq!(session.active_run, Some(run_id));
    }

    #[test]
    fn agent_run_updated_records_observable_summary() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
        );
        let run_id = Uuid::new_v4();

        session.apply_event(&crate::ReplEvent::AgentRunUpdated {
            run_id,
            summary: crate::AgentRunSummary {
                goal: "list files".to_string(),
                last_phase: crate::AgentRunPhase::Completed,
                completed_steps: 3,
                stop_reason: None,
                failure_code: None,
                failure_message: None,
                recoverable_failure: false,
            },
        });

        let agent_run = session.agent_run.expect("agent run summary");
        assert_eq!(agent_run.run_id, run_id);
        assert_eq!(agent_run.summary.goal, "list files");
        assert_eq!(
            agent_run.summary.last_phase,
            crate::AgentRunPhase::Completed
        );
    }

    #[test]
    fn run_started_clears_previous_agent_run_summary() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
        );
        let previous_run_id = Uuid::new_v4();
        let next_run_id = Uuid::new_v4();

        session.apply_event(&crate::ReplEvent::AgentRunUpdated {
            run_id: previous_run_id,
            summary: crate::AgentRunSummary {
                goal: "old task".to_string(),
                last_phase: crate::AgentRunPhase::Completed,
                completed_steps: 3,
                stop_reason: None,
                failure_code: None,
                failure_message: None,
                recoverable_failure: false,
            },
        });
        session.apply_event(&crate::ReplEvent::RunStarted {
            run_id: next_run_id,
        });

        assert!(session.agent_run.is_none());
        assert_eq!(session.active_run, Some(next_run_id));
    }

    #[test]
    fn subagent_lifecycle_blocks_running_without_approval() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
        );

        session.apply_event(&crate::ReplEvent::SubagentLifecycleUpdated {
            update: crate::SubagentLifecycleUpdate {
                name: "coder".to_string(),
                mode: "workspace-write".to_string(),
                status: crate::SubagentLifecycleStatus::Running,
                readiness_score: 100,
                reason: None,
            },
        });

        assert_eq!(session.subagent_activity.len(), 1);
        assert_eq!(
            session.subagent_activity[0].status,
            crate::SubagentLifecycleStatus::Blocked
        );
        assert_eq!(
            session.subagent_activity[0].reason.as_deref(),
            Some("invalid subagent lifecycle transition: None -> Running")
        );
    }

    #[test]
    fn subagent_lifecycle_blocks_unready_executable_status() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
        );

        session.apply_event(&crate::ReplEvent::SubagentLifecycleUpdated {
            update: crate::SubagentLifecycleUpdate {
                name: "coder".to_string(),
                mode: "workspace-write".to_string(),
                status: crate::SubagentLifecycleStatus::Prepared,
                readiness_score: 80,
                reason: Some("missing preview edit capability".to_string()),
            },
        });

        assert_eq!(
            session.subagent_activity[0].status,
            crate::SubagentLifecycleStatus::Blocked
        );
        assert_eq!(
            session.subagent_activity[0].reason.as_deref(),
            Some(
                "readiness score 80 is below execution threshold; missing preview edit capability"
            )
        );
    }

    #[test]
    fn subagent_lifecycle_allows_approved_running_completion_sequence() {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            crate::ModelRef {
                provider: "openai".to_string(),
                name: "gpt-test".to_string(),
            },
        );

        for status in [
            crate::SubagentLifecycleStatus::Prepared,
            crate::SubagentLifecycleStatus::Approved,
            crate::SubagentLifecycleStatus::Running,
            crate::SubagentLifecycleStatus::Completed,
        ] {
            session.apply_event(&crate::ReplEvent::SubagentLifecycleUpdated {
                update: crate::SubagentLifecycleUpdate {
                    name: "eval-runner".to_string(),
                    mode: "evaluation".to_string(),
                    status,
                    readiness_score: 100,
                    reason: None,
                },
            });
        }

        assert_eq!(session.subagent_activity.len(), 1);
        assert_eq!(
            session.subagent_activity[0].status,
            crate::SubagentLifecycleStatus::Completed
        );
        assert!(session.subagent_activity[0].reason.is_none());
    }
}
