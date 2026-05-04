use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ShortcutSource {
    GnomeMediaKeys,
    TauriGlobalShortcut,
    Cli,
    SystemdUserService,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ReplIntent {
    AskTechnicalQuestion,
    ExplainScreen,
    ExplainCode,
    DebugCode,
    SolvePracticeQuestion,
    MultipleChoiceAssist,
    GenerateTestCases,
    ExplainTerminalError,
    SearchDocs,
    OpenApplication,
    OpenWebsite,
    ConfigureModel,
    ManageContext,
    AgenticCodeChange,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolStatus {
    Succeeded,
    Failed,
    Cancelled,
    Denied,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AgentRunPhase {
    Received,
    Planning,
    Inspecting,
    Editing,
    Testing,
    Reviewing,
    Completed,
    Cancelled,
    Failed,
}

impl AgentRunPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AgentRunStopReason {
    UserInterrupt,
    Timeout,
    Superseded,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRunSummary {
    pub goal: String,
    pub last_phase: AgentRunPhase,
    pub completed_steps: usize,
    pub stop_reason: Option<AgentRunStopReason>,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
    pub recoverable_failure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubagentRouteRecommendation {
    pub name: String,
    pub score: u8,
    pub mode: String,
    pub matched_signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubagentHandoffPrepared {
    pub name: String,
    pub mode: String,
    pub approval_required: bool,
    pub allowed_tools: Vec<String>,
    pub required_output_fields: Vec<String>,
    #[serde(default = "default_true")]
    pub output_additional_properties_allowed: bool,
    pub timeout_ms: u64,
    pub max_context_tokens: u32,
    pub validation_checklist: Vec<String>,
    pub safety_notes: Vec<String>,
    pub readiness_score: u8,
    pub readiness_issues: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SubagentLifecycleStatus {
    Prepared,
    Approved,
    Running,
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubagentLifecycleUpdate {
    pub name: String,
    pub mode: String,
    pub status: SubagentLifecycleStatus,
    pub readiness_score: u8,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolExecutionRecord {
    pub tool_name: String,
    pub call_id: Uuid,
    pub status: ToolStatus,
    pub started_at_unix_ms: u64,
    pub completed_at_unix_ms: u64,
    pub duration_ms: u64,
    pub output_chars: usize,
    pub truncated: bool,
    pub error_code: Option<String>,
    pub retryable: Option<bool>,
    #[serde(with = "crate::json_value_wire")]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReplEvent {
    SessionStarted {
        session_id: Uuid,
    },
    RunStarted {
        run_id: Uuid,
    },
    ShortcutTriggered {
        binding: String,
        source: ShortcutSource,
    },
    OverlayShown {
        mode: crate::ReplMode,
    },
    VoiceListeningStarted,
    VoiceTranscriptPartial {
        text: String,
    },
    VoiceTranscriptFinal {
        text: String,
    },
    ScreenCaptured {
        source: crate::ExtractionSource,
        bytes: usize,
    },
    OcrCompleted {
        chars: usize,
        language_hint: Option<String>,
    },
    IntentDetected {
        intent: ReplIntent,
        confidence: f32,
    },
    AgentRunUpdated {
        run_id: Uuid,
        summary: AgentRunSummary,
    },
    PolicyEvaluated {
        policy: crate::AssessmentPolicy,
        allowed: bool,
    },
    ConfirmationDismissed,
    ModelSelected {
        model: crate::ModelRef,
        role: crate::ModelRole,
    },
    SearchStarted {
        query: String,
        provider: String,
    },
    SearchContextExtracted {
        provider: String,
        organic_results: usize,
        ai_overview_present: bool,
    },
    ContextItemAdded {
        item: crate::ContextItem,
    },
    TokenDelta {
        run_id: Uuid,
        text: String,
    },
    MessageAppended {
        message: crate::ReplMessage,
    },
    ToolStarted {
        name: String,
    },
    ToolCompleted {
        name: String,
        status: ToolStatus,
    },
    ToolExecutionRecorded {
        record: ToolExecutionRecord,
    },
    SubagentRouted {
        recommendations: Vec<SubagentRouteRecommendation>,
    },
    SubagentHandoffPrepared {
        handoff: SubagentHandoffPrepared,
    },
    SubagentLifecycleUpdated {
        update: SubagentLifecycleUpdate,
    },
    PermissionRequested {
        request: crate::PermissionRequest,
    },
    PermissionReplied {
        request_id: Uuid,
        reply: crate::PermissionReply,
    },
    TtsQueued,
    TtsStarted,
    TtsCompleted,
    RunCompleted {
        run_id: Uuid,
    },
    Error {
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn tool_status_supports_stable_ordering_for_metric_maps() {
        let mut counts = BTreeMap::new();
        counts.insert(ToolStatus::Succeeded, 2_usize);
        counts.insert(ToolStatus::Failed, 1_usize);

        assert_eq!(counts.get(&ToolStatus::Succeeded), Some(&2));
        assert_eq!(counts.get(&ToolStatus::Failed), Some(&1));
    }

    #[test]
    fn tool_execution_record_roundtrips_as_repl_event() {
        let record = ToolExecutionRecord {
            tool_name: "shell.run".to_string(),
            call_id: Uuid::new_v4(),
            status: ToolStatus::Succeeded,
            started_at_unix_ms: 1_775_000_000_000,
            completed_at_unix_ms: 1_775_000_000_025,
            duration_ms: 25,
            output_chars: 6,
            truncated: false,
            error_code: None,
            retryable: None,
            metadata: serde_json::json!({
                "command": "printf coddy",
                "cwd": ".",
                "exit_code": 0
            }),
        };
        let event = ReplEvent::ToolExecutionRecorded {
            record: record.clone(),
        };

        let encoded = serde_json::to_string(&event).expect("serialize event");
        let decoded: ReplEvent = serde_json::from_str(&encoded).expect("deserialize event");

        assert_eq!(decoded, event);
    }
}
