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
        }
    }

    pub fn transition_to(&mut self, status: SessionStatus) {
        self.status = status;
    }

    pub fn apply_event(&mut self, event: &crate::ReplEvent) {
        match event {
            crate::ReplEvent::SessionStarted { session_id } => {
                self.id = *session_id;
                self.status = SessionStatus::Idle;
            }
            crate::ReplEvent::RunStarted { run_id } => {
                self.active_run = Some(*run_id);
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
            crate::ReplEvent::PermissionRequested { .. } => {
                self.status = SessionStatus::AwaitingToolApproval;
            }
            crate::ReplEvent::PermissionReplied { .. } => {
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
                self.status = if self.voice.speaking {
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
}
