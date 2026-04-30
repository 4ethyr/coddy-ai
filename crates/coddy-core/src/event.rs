use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolStatus {
    Succeeded,
    Failed,
    Cancelled,
    Denied,
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
    pub timeout_ms: u64,
    pub max_context_tokens: u32,
    pub validation_checklist: Vec<String>,
    pub safety_notes: Vec<String>,
    pub readiness_score: u8,
    pub readiness_issues: Vec<String>,
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
