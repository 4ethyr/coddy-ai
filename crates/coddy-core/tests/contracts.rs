use coddy_core::{
    evaluate_assistance, evaluate_shortcut_conflict, AssessmentPolicy, ContextPolicy,
    ExtractionSource, ModelCredential, ModelRef, ModelRole, PermissionReply, PermissionRequest,
    ReplCommand, ReplEvent, ReplEventEnvelope, ReplEventLog, ReplMessage, ReplMode, ReplSession,
    RequestedHelp, ScreenRegion, ScreenRegionKind, SearchProvider, SearchResultContext,
    ShortcutConflictPolicy, ShortcutDecision, ShortcutSource, SourceQuality, ToolName,
    ToolPermission, ToolRiskLevel,
};
use uuid::Uuid;

#[test]
fn voice_turn_command_roundtrips_through_json() {
    let command = ReplCommand::VoiceTurn {
        transcript_override: Some("quem foi rousseau?".to_string()),
    };

    let encoded = serde_json::to_string(&command).expect("serialize command");
    let decoded: ReplCommand = serde_json::from_str(&encoded).expect("deserialize command");

    assert_eq!(decoded, command);
}

#[test]
fn ask_command_keeps_context_policy() {
    let command = ReplCommand::Ask {
        text: "explique esse erro".to_string(),
        context_policy: ContextPolicy::VisibleScreen,
        model_credential: None,
    };

    let encoded = serde_json::to_string(&command).expect("serialize command");

    assert!(encoded.contains("VisibleScreen"));
}

#[test]
fn ask_command_debug_redacts_ephemeral_model_credential() {
    let command = ReplCommand::Ask {
        text: "explique esse erro".to_string(),
        context_policy: ContextPolicy::WorkspaceOnly,
        model_credential: Some(ModelCredential {
            provider: "openai".to_string(),
            token: "sk-secret-token".to_string(),
            endpoint: Some("https://api.openai.com/v1".to_string()),
            metadata: [("project_id".to_string(), "coddy-dev".to_string())]
                .into_iter()
                .collect(),
        }),
    };

    let rendered = format!("{command:?}");

    assert!(rendered.contains("ModelCredential"));
    assert!(rendered.contains("<redacted>"));
    assert!(rendered.contains("project_id"));
    assert!(!rendered.contains("sk-secret-token"));
    assert!(!rendered.contains("coddy-dev"));
}

#[test]
fn model_credential_accepts_legacy_json_without_metadata() {
    let decoded: ModelCredential =
        serde_json::from_str(r#"{"provider":"openai","token":"sk-secret-token","endpoint":null}"#)
            .expect("deserialize credential");

    assert_eq!(decoded.provider, "openai");
    assert_eq!(decoded.token, "sk-secret-token");
    assert!(decoded.endpoint.is_none());
    assert!(decoded.metadata.is_empty());
}

#[test]
fn reply_permission_command_roundtrips_through_json() {
    let request_id = Uuid::new_v4();
    let command = ReplCommand::ReplyPermission {
        request_id,
        reply: PermissionReply::Once,
    };

    let encoded = serde_json::to_string(&command).expect("serialize command");
    let decoded: ReplCommand = serde_json::from_str(&encoded).expect("deserialize command");

    assert_eq!(decoded, command);
    assert!(encoded.contains("ReplyPermission"));
}

#[test]
fn restricted_assessment_blocks_final_multiple_choice_answer() {
    let decision = evaluate_assistance(
        AssessmentPolicy::RestrictedAssessment,
        RequestedHelp::SolveMultipleChoice,
    );

    assert!(!decision.allowed);
    assert!(!decision.requires_confirmation);
}

#[test]
fn unknown_assessment_requires_confirmation() {
    let decision = evaluate_assistance(
        AssessmentPolicy::UnknownAssessment,
        RequestedHelp::GenerateCompleteCode,
    );

    assert!(!decision.allowed);
    assert!(decision.requires_confirmation);
}

#[test]
fn unknown_policy_evaluation_sets_session_confirmation_state() {
    let mut session = ReplSession::new(
        ReplMode::FloatingTerminal,
        ModelRef {
            provider: "ollama".to_string(),
            name: "gemma4-e2b".to_string(),
        },
    );

    session.apply_event(&ReplEvent::PolicyEvaluated {
        policy: AssessmentPolicy::UnknownAssessment,
        allowed: false,
    });

    assert_eq!(
        session.status,
        coddy_core::SessionStatus::AwaitingConfirmation
    );
}

#[test]
fn confirmation_dismissal_returns_session_to_idle() {
    let mut session = ReplSession::new(
        ReplMode::FloatingTerminal,
        ModelRef {
            provider: "ollama".to_string(),
            name: "gemma4-e2b".to_string(),
        },
    );

    session.apply_event(&ReplEvent::PolicyEvaluated {
        policy: AssessmentPolicy::UnknownAssessment,
        allowed: false,
    });
    session.apply_event(&ReplEvent::ConfirmationDismissed);

    assert_eq!(session.status, coddy_core::SessionStatus::Idle);
}

#[test]
fn shortcut_starts_when_no_run_is_active() {
    let decision = evaluate_shortcut_conflict(ShortcutConflictPolicy::IgnoreWhileBusy, None);

    assert!(decision.starts_listening());
}

#[test]
fn shortcut_can_ignore_active_run() {
    let active_run_id = Uuid::new_v4();
    let decision =
        evaluate_shortcut_conflict(ShortcutConflictPolicy::IgnoreWhileBusy, Some(active_run_id));

    assert_eq!(decision, ShortcutDecision::IgnoredBusy { active_run_id });
}

#[test]
fn shortcut_can_stop_speaking_and_start_next_run() {
    let previous_run_id = Uuid::new_v4();
    let decision = evaluate_shortcut_conflict(
        ShortcutConflictPolicy::StopSpeakingAndListen,
        Some(previous_run_id),
    );

    match decision {
        ShortcutDecision::StoppedSpeaking {
            previous_run_id: observed_previous,
            next_run_id,
        } => {
            assert_eq!(observed_previous, previous_run_id);
            assert_ne!(next_run_id, previous_run_id);
        }
        other => panic!("unexpected decision: {other:?}"),
    }
}

#[test]
fn overlay_event_is_serializable_before_asr_events() {
    let events = vec![
        ReplEvent::ShortcutTriggered {
            binding: "Shift+CapsLk".to_string(),
            source: ShortcutSource::GnomeMediaKeys,
        },
        ReplEvent::OverlayShown {
            mode: ReplMode::FloatingTerminal,
        },
        ReplEvent::VoiceListeningStarted,
    ];

    let encoded = serde_json::to_string(&events).expect("serialize events");

    assert!(encoded.find("OverlayShown").unwrap() < encoded.find("VoiceListeningStarted").unwrap());
}

#[test]
fn event_envelope_roundtrips_through_json_for_frontend_streaming() {
    let run_id = Uuid::new_v4();
    let envelope = ReplEventEnvelope::new(
        42,
        Uuid::new_v4(),
        Some(run_id),
        1_775_000_000_000,
        ReplEvent::RunStarted { run_id },
    );

    let encoded = serde_json::to_string(&envelope).expect("serialize event envelope");
    let decoded: ReplEventEnvelope =
        serde_json::from_str(&encoded).expect("deserialize event envelope");

    assert_eq!(decoded, envelope);
    assert!(encoded.contains("\"sequence\":42"));
    assert!(encoded.contains("RunStarted"));
}

#[test]
fn permission_events_roundtrip_and_update_session_status() {
    let selected_model = ModelRef {
        provider: "ollama".to_string(),
        name: "gemma4-e2b".to_string(),
    };
    let mut session = ReplSession::new(ReplMode::FloatingTerminal, selected_model);
    let run_id = Uuid::new_v4();
    let tool_call_id = Uuid::new_v4();
    session.apply_event(&ReplEvent::RunStarted { run_id });

    let request = PermissionRequest::new(
        session.id,
        run_id,
        Some(tool_call_id),
        ToolName::new("filesystem.read_file").expect("valid tool name"),
        ToolPermission::ReadWorkspace,
        vec!["docs/repl/architecture.md".to_string()],
        ToolRiskLevel::Low,
        serde_json::json!({ "path": "docs/repl/architecture.md" }),
        1_775_000_000_000,
    )
    .expect("valid permission request");

    let event = ReplEvent::PermissionRequested {
        request: request.clone(),
    };
    let encoded = serde_json::to_string(&event).expect("serialize permission event");
    let decoded: ReplEvent = serde_json::from_str(&encoded).expect("deserialize permission event");

    assert_eq!(decoded, event);

    session.apply_event(&event);
    assert_eq!(
        session.status,
        coddy_core::SessionStatus::AwaitingToolApproval
    );
    assert_eq!(
        session
            .pending_permission
            .as_ref()
            .map(|request| request.id),
        Some(request.id)
    );

    session.apply_event(&ReplEvent::PermissionReplied {
        request_id: request.id,
        reply: PermissionReply::Once,
    });

    assert_eq!(session.status, coddy_core::SessionStatus::Thinking);
    assert!(session.pending_permission.is_none());
}

#[test]
fn pending_permission_survives_run_completion_until_reply() {
    let selected_model = ModelRef {
        provider: "ollama".to_string(),
        name: "gemma4-e2b".to_string(),
    };
    let mut session = ReplSession::new(ReplMode::FloatingTerminal, selected_model);
    let run_id = Uuid::new_v4();
    let request = PermissionRequest::new(
        session.id,
        run_id,
        None,
        ToolName::new("filesystem.apply_edit").expect("valid tool name"),
        ToolPermission::WriteWorkspace,
        vec!["src/lib.rs".to_string()],
        ToolRiskLevel::High,
        serde_json::json!({ "path": "src/lib.rs" }),
        1_775_000_000_000,
    )
    .expect("valid permission request");

    session.apply_event(&ReplEvent::RunStarted { run_id });
    session.apply_event(&ReplEvent::PermissionRequested {
        request: request.clone(),
    });
    session.apply_event(&ReplEvent::RunCompleted { run_id });

    assert_eq!(
        session.status,
        coddy_core::SessionStatus::AwaitingToolApproval
    );
    assert_eq!(session.active_run, None);
    assert_eq!(
        session
            .pending_permission
            .as_ref()
            .map(|request| request.id),
        Some(request.id)
    );

    session.apply_event(&ReplEvent::PermissionReplied {
        request_id: request.id,
        reply: PermissionReply::Reject,
    });

    assert_eq!(session.status, coddy_core::SessionStatus::Idle);
    assert!(session.pending_permission.is_none());
}

#[test]
fn session_snapshot_replays_message_events_for_frontend_state() {
    let selected_model = ModelRef {
        provider: "ollama".to_string(),
        name: "gemma4-e2b".to_string(),
    };
    let session = ReplSession::new(ReplMode::FloatingTerminal, selected_model);
    let mut log = ReplEventLog::new(session.id);
    let message = ReplMessage {
        id: Uuid::new_v4(),
        role: "user".to_string(),
        text: "Explique este erro".to_string(),
    };

    log.append(
        ReplEvent::MessageAppended {
            message: message.clone(),
        },
        None,
        1_775_000_000_000,
    );

    let snapshot = log.snapshot(session);
    let encoded = serde_json::to_string(&snapshot).expect("serialize snapshot");

    assert_eq!(snapshot.last_sequence, 1);
    assert_eq!(snapshot.session.messages, vec![message]);
    assert!(encoded.contains("Explique este erro"));
}

#[test]
fn chat_model_selection_updates_session_model() {
    let initial_model = ModelRef {
        provider: "ollama".to_string(),
        name: "gemma4-e2b".to_string(),
    };
    let next_model = ModelRef {
        provider: "ollama".to_string(),
        name: "qwen2.5:0.5b".to_string(),
    };
    let mut session = ReplSession::new(ReplMode::FloatingTerminal, initial_model);

    session.apply_event(&ReplEvent::ModelSelected {
        model: next_model.clone(),
        role: ModelRole::Chat,
    });

    assert_eq!(session.selected_model, next_model);
}

#[test]
fn non_chat_model_selection_does_not_replace_session_chat_model() {
    let chat_model = ModelRef {
        provider: "ollama".to_string(),
        name: "gemma4-e2b".to_string(),
    };
    let ocr_model = ModelRef {
        provider: "ollama".to_string(),
        name: "glm-ocr".to_string(),
    };
    let mut session = ReplSession::new(ReplMode::FloatingTerminal, chat_model.clone());

    session.apply_event(&ReplEvent::ModelSelected {
        model: ocr_model,
        role: ModelRole::Ocr,
    });

    assert_eq!(session.selected_model, chat_model);
}

#[test]
fn screen_region_can_mark_ai_overview_from_ocr() {
    let region = ScreenRegion {
        id: "region-1".to_string(),
        kind: ScreenRegionKind::AiOverview,
        text: "JavaScript é uma linguagem de programação usada na web.".to_string(),
        bounding_box: coddy_core::BoundingBox {
            x: 10,
            y: 20,
            width: 640,
            height: 220,
        },
        confidence: 0.91,
        source: ExtractionSource::ScreenshotOcr,
    };

    assert_eq!(region.kind, ScreenRegionKind::AiOverview);
    assert!(region.confidence > 0.9);
}

#[test]
fn search_context_detects_ai_overview_and_counts_sources() {
    let context = SearchResultContext {
        query: "o que é javascript".to_string(),
        provider: SearchProvider::Google,
        organic_results: vec![coddy_core::SearchResultItem {
            title: "JavaScript | MDN".to_string(),
            url: "https://developer.mozilla.org/".to_string(),
            snippet: "JavaScript é uma linguagem de programação.".to_string(),
            rank: 1,
            source_quality: SourceQuality::Official,
        }],
        ai_overview_text: Some(
            "JavaScript é uma linguagem de programação usada para páginas interativas.".to_string(),
        ),
        ai_overview_sources: Vec::new(),
        visible_text: "Visão geral criada por IA".to_string(),
        captured_at_unix_ms: 1_775_000_000_000,
        confidence: 0.88,
        source_urls: vec!["https://developer.mozilla.org/".to_string()],
        extraction_method: ExtractionSource::ScreenshotOcr,
    };

    assert!(context.has_ai_overview());
    assert_eq!(context.source_count(), 1);
}
