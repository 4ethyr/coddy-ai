use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

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
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl fmt::Debug for ModelCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let metadata_keys = self.metadata.keys().collect::<Vec<_>>();
        formatter
            .debug_struct("ModelCredential")
            .field("provider", &self.provider)
            .field("token", &"<redacted>")
            .field("endpoint", &self.endpoint)
            .field("metadata_keys", &metadata_keys)
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
    ReplyPermission {
        request_id: uuid::Uuid,
        reply: crate::PermissionReply,
    },
    DismissConfirmation,
    NewSession,
    StopActiveRun,
    StopSpeaking,
}
