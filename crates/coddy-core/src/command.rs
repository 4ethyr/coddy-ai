use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ModelRef {
    pub provider: String,
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ModelCredential {
    pub provider: String,
    pub token: String,
    pub endpoint: Option<String>,
}

impl fmt::Debug for ModelCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCredential")
            .field("provider", &self.provider)
            .field("token", &"<redacted>")
            .field("endpoint", &self.endpoint)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ModelRole {
    Chat,
    Ocr,
    Asr,
    Tts,
    Embedding,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ContextPolicy {
    NoScreen,
    VisibleScreen,
    WorkspaceOnly,
    ScreenAndWorkspace,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ScreenAssistMode {
    ExplainVisibleScreen,
    ExplainCode,
    DebugError,
    MultipleChoice,
    SummarizeDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplCommand {
    Ask {
        text: String,
        context_policy: ContextPolicy,
        model_credential: Option<ModelCredential>,
    },
    CaptureAndExplain {
        mode: ScreenAssistMode,
        policy: crate::AssessmentPolicy,
    },
    VoiceTurn {
        transcript_override: Option<String>,
    },
    OpenUi {
        mode: crate::ReplMode,
    },
    SelectModel {
        model: ModelRef,
        role: ModelRole,
    },
    DismissConfirmation,
    StopActiveRun,
    StopSpeaking,
}
