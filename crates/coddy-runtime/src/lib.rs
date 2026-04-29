use coddy_agent::AgentToolRegistry;
use coddy_core::{
    ModelRef, ReplCommand, ReplEvent, ReplEventBroker, ReplEventEnvelope, ReplMessage, ReplMode,
    ReplSession, ReplSessionSnapshot,
};
use coddy_ipc::{
    read_frame, write_frame, CoddyIpcResult, CoddyRequest, CoddyResult, CoddyWireRequest,
    CoddyWireResult, ReplCommandJob, ReplEventStreamJob, ReplToolCatalogItem,
};
use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixListener;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CoddyRuntime {
    tool_registry: AgentToolRegistry,
    state: Arc<Mutex<RuntimeState>>,
}

#[derive(Debug)]
struct RuntimeState {
    session: ReplSession,
    broker: ReplEventBroker,
}

impl CoddyRuntime {
    pub fn new(tool_registry: AgentToolRegistry) -> Self {
        Self {
            tool_registry,
            state: Arc::new(Mutex::new(RuntimeState::new(default_session()))),
        }
    }

    pub fn handle_request(&self, request: CoddyRequest) -> CoddyResult {
        match request {
            CoddyRequest::Command(job) => self.handle_command(job),
            CoddyRequest::SessionSnapshot(job) => CoddyResult::ReplSessionSnapshot {
                request_id: job.request_id,
                snapshot: Box::new(self.snapshot()),
            },
            CoddyRequest::Events(job) => {
                let (events, last_sequence) = self.events_after(job.after_sequence);
                CoddyResult::ReplEvents {
                    request_id: job.request_id,
                    events,
                    last_sequence,
                }
            }
            CoddyRequest::Tools(job) => CoddyResult::ReplToolCatalog {
                request_id: job.request_id,
                tools: self.tool_catalog(),
            },
            other => CoddyResult::Error {
                request_id: other.request_id(),
                code: "unsupported_request".to_string(),
                message: "Coddy runtime does not handle this request yet".to_string(),
            },
        }
    }

    fn handle_command(&self, job: ReplCommandJob) -> CoddyResult {
        let ReplCommandJob {
            request_id,
            command,
            speak,
        } = job;

        match command {
            ReplCommand::Ask { text, .. } => self.handle_ask(request_id, text, speak),
            ReplCommand::VoiceTurn {
                transcript_override,
            } => match normalize_text(transcript_override.unwrap_or_default()) {
                Some(transcript) => {
                    self.publish_event_now(ReplEvent::VoiceTranscriptFinal {
                        text: transcript.clone(),
                    });
                    self.handle_ask(request_id, transcript, speak)
                }
                None => invalid_command(request_id, "voice transcript is required"),
            },
            ReplCommand::OpenUi { mode } => {
                self.publish_event_now(ReplEvent::OverlayShown { mode });
                CoddyResult::ActionStatus {
                    request_id,
                    message: format!("UI mode opened: {mode:?}"),
                    spoken: speak,
                }
            }
            ReplCommand::SelectModel { model, role } => {
                self.publish_event_now(ReplEvent::ModelSelected {
                    model: model.clone(),
                    role,
                });
                CoddyResult::ActionStatus {
                    request_id,
                    message: format!(
                        "Model selected for {role:?}: {}/{}",
                        model.provider, model.name
                    ),
                    spoken: speak,
                }
            }
            ReplCommand::DismissConfirmation => {
                self.publish_event_now(ReplEvent::ConfirmationDismissed);
                CoddyResult::ActionStatus {
                    request_id,
                    message: "Confirmation dismissed".to_string(),
                    spoken: speak,
                }
            }
            ReplCommand::StopSpeaking => {
                self.publish_event_now(ReplEvent::TtsCompleted);
                CoddyResult::ActionStatus {
                    request_id,
                    message: "Speech stopped".to_string(),
                    spoken: false,
                }
            }
            ReplCommand::StopActiveRun => {
                if let Some(run_id) = self.snapshot().session.active_run {
                    self.publish_event_with_run_now(ReplEvent::RunCompleted { run_id }, run_id);
                    CoddyResult::ActionStatus {
                        request_id,
                        message: "Active run stopped".to_string(),
                        spoken: false,
                    }
                } else {
                    CoddyResult::ActionStatus {
                        request_id,
                        message: "No active run".to_string(),
                        spoken: false,
                    }
                }
            }
            ReplCommand::CaptureAndExplain { .. } => CoddyResult::Error {
                request_id,
                code: "unsupported_command".to_string(),
                message: "Coddy runtime does not handle screen capture commands yet".to_string(),
            },
        }
    }

    fn handle_ask(&self, request_id: Uuid, text: String, speak: bool) -> CoddyResult {
        let Some(text) = normalize_text(text) else {
            return invalid_command(request_id, "ask text is required");
        };

        let run_id = Uuid::new_v4();
        let assistant_text = format!("Coddy runtime received: {text}");

        self.publish_event_with_run_now(
            ReplEvent::MessageAppended {
                message: repl_message("user", text),
            },
            run_id,
        );
        self.publish_event_with_run_now(ReplEvent::RunStarted { run_id }, run_id);
        self.publish_event_with_run_now(
            ReplEvent::MessageAppended {
                message: repl_message("assistant", assistant_text.clone()),
            },
            run_id,
        );
        self.publish_event_with_run_now(ReplEvent::RunCompleted { run_id }, run_id);

        CoddyResult::Text {
            request_id,
            text: assistant_text,
            spoken: speak,
        }
    }

    pub fn snapshot(&self) -> ReplSessionSnapshot {
        self.with_state(|state| state.broker.snapshot(state.session.clone()))
    }

    pub fn events_after(&self, sequence: u64) -> (Vec<ReplEventEnvelope>, u64) {
        self.with_state(|state| {
            (
                state.broker.events_after(sequence),
                state.broker.last_sequence(),
            )
        })
    }

    pub fn publish_event(
        &self,
        event: ReplEvent,
        run_id: Option<Uuid>,
        captured_at_unix_ms: u64,
    ) -> ReplEventEnvelope {
        self.with_state_mut(|state| state.broker.publish(event, run_id, captured_at_unix_ms))
    }

    fn publish_event_now(&self, event: ReplEvent) -> ReplEventEnvelope {
        self.publish_event(event, None, unix_ms_now())
    }

    fn publish_event_with_run_now(&self, event: ReplEvent, run_id: Uuid) -> ReplEventEnvelope {
        self.publish_event(event, Some(run_id), unix_ms_now())
    }

    pub async fn handle_connection<IO>(&self, stream: &mut IO) -> CoddyIpcResult<()>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        let request: CoddyWireRequest = read_frame(stream).await?;
        request.ensure_compatible()?;
        match request.request {
            CoddyRequest::EventStream(job) => self.handle_event_stream(stream, job).await,
            request => {
                let response = CoddyWireResult::new(self.handle_request(request));
                write_frame(stream, &response).await
            }
        }
    }

    pub async fn serve_next_unix_connection(&self, listener: &UnixListener) -> CoddyIpcResult<()> {
        let (mut stream, _) = listener.accept().await?;
        self.handle_connection(&mut stream).await
    }

    async fn handle_event_stream<IO>(
        &self,
        stream: &mut IO,
        job: ReplEventStreamJob,
    ) -> CoddyIpcResult<()>
    where
        IO: AsyncWrite + Unpin,
    {
        let mut subscription = self.subscribe_after(job.after_sequence);
        while let Some(event) = subscription.next().await {
            let last_sequence = event.sequence;
            write_frame(
                stream,
                &CoddyWireResult::new(CoddyResult::ReplEvents {
                    request_id: job.request_id,
                    events: vec![event],
                    last_sequence,
                }),
            )
            .await?;
        }
        Ok(())
    }

    pub fn tool_catalog(&self) -> Vec<ReplToolCatalogItem> {
        let mut tools: Vec<_> = self
            .tool_registry
            .definitions()
            .iter()
            .map(ReplToolCatalogItem::from)
            .collect();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }

    fn subscribe_after(&self, sequence: u64) -> coddy_core::ReplEventSubscription {
        self.with_state(|state| state.broker.subscribe_after(sequence))
    }

    fn with_state<T>(&self, action: impl FnOnce(&RuntimeState) -> T) -> T {
        let state = self
            .state
            .lock()
            .expect("coddy runtime state mutex poisoned");
        action(&state)
    }

    fn with_state_mut<T>(&self, action: impl FnOnce(&mut RuntimeState) -> T) -> T {
        let mut state = self
            .state
            .lock()
            .expect("coddy runtime state mutex poisoned");
        action(&mut state)
    }
}

impl Default for CoddyRuntime {
    fn default() -> Self {
        Self::new(AgentToolRegistry::default())
    }
}

impl RuntimeState {
    fn new(session: ReplSession) -> Self {
        let session_id = session.id;
        let mut broker = ReplEventBroker::new(session_id, 1024);
        broker.publish(
            ReplEvent::SessionStarted { session_id },
            None,
            unix_ms_now(),
        );
        Self { session, broker }
    }
}

fn default_session() -> ReplSession {
    ReplSession::new(
        ReplMode::FloatingTerminal,
        ModelRef {
            provider: "coddy".to_string(),
            name: "unselected".to_string(),
        },
    )
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn normalize_text(text: impl Into<String>) -> Option<String> {
    let normalized = text.into().trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn repl_message(role: &str, text: String) -> ReplMessage {
    ReplMessage {
        id: Uuid::new_v4(),
        role: role.to_string(),
        text,
    }
}

fn invalid_command(request_id: Uuid, message: &str) -> CoddyResult {
    CoddyResult::Error {
        request_id,
        code: "invalid_command".to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coddy_client::CoddyClient;
    use coddy_core::{
        ApprovalPolicy, ModelRef, ModelRole, ReplEvent, ToolCategory, ToolPermission, ToolRiskLevel,
    };
    use coddy_ipc::{
        ReplCommandJob, ReplEventStreamJob, ReplEventsJob, ReplSessionSnapshotJob, ReplToolsJob,
    };
    use std::{env, path::PathBuf};
    use uuid::Uuid;

    #[test]
    fn tools_request_returns_sorted_rich_catalog_from_agent_registry() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();

        let result = runtime.handle_request(CoddyRequest::Tools(ReplToolsJob { request_id }));

        let CoddyResult::ReplToolCatalog {
            request_id: actual_request_id,
            tools,
        } = result
        else {
            panic!("expected tool catalog result");
        };
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();

        assert_eq!(actual_request_id, request_id);
        assert_eq!(
            names,
            vec![
                "filesystem.apply_edit",
                "filesystem.list_files",
                "filesystem.preview_edit",
                "filesystem.read_file",
                "filesystem.search_files",
                "shell.run",
            ]
        );

        let shell = tools
            .iter()
            .find(|tool| tool.name == "shell.run")
            .expect("shell tool");
        assert_eq!(shell.category, ToolCategory::Shell);
        assert_eq!(shell.risk_level, ToolRiskLevel::Medium);
        assert_eq!(shell.permissions, vec![ToolPermission::ExecuteCommand]);
        assert_eq!(shell.approval_policy, ApprovalPolicy::AskOnUse);

        let apply_edit = tools
            .iter()
            .find(|tool| tool.name == "filesystem.apply_edit")
            .expect("apply edit tool");
        assert_eq!(apply_edit.risk_level, ToolRiskLevel::High);
        assert_eq!(apply_edit.permissions, vec![ToolPermission::WriteWorkspace]);
        assert_eq!(apply_edit.approval_policy, ApprovalPolicy::AlwaysAsk);
    }

    #[test]
    fn unsupported_requests_return_structured_error_with_request_id() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();

        let result = runtime.handle_request(CoddyRequest::EventStream(ReplEventStreamJob {
            request_id,
            after_sequence: 7,
        }));

        let CoddyResult::Error {
            request_id: actual_request_id,
            code,
            message,
        } = result
        else {
            panic!("expected error result");
        };

        assert_eq!(actual_request_id, request_id);
        assert_eq!(code, "unsupported_request");
        assert!(message.contains("does not handle"));
    }

    #[test]
    fn snapshot_request_replays_runtime_events() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();
        let selected_model = ModelRef {
            provider: "ollama".to_string(),
            name: "qwen2.5-coder:7b".to_string(),
        };
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: selected_model.clone(),
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_010,
        );

        let result =
            runtime.handle_request(CoddyRequest::SessionSnapshot(ReplSessionSnapshotJob {
                request_id,
            }));

        let CoddyResult::ReplSessionSnapshot {
            request_id: actual_request_id,
            snapshot,
        } = result
        else {
            panic!("expected session snapshot result");
        };

        assert_eq!(actual_request_id, request_id);
        assert_eq!(snapshot.session.selected_model, selected_model);
        assert_eq!(snapshot.last_sequence, 2);
    }

    #[test]
    fn events_request_returns_incremental_runtime_events() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();
        runtime.publish_event(ReplEvent::VoiceListeningStarted, None, 1_775_000_000_020);

        let result = runtime.handle_request(CoddyRequest::Events(ReplEventsJob {
            request_id,
            after_sequence: 1,
        }));

        let CoddyResult::ReplEvents {
            request_id: actual_request_id,
            events,
            last_sequence,
        } = result
        else {
            panic!("expected repl events result");
        };

        assert_eq!(actual_request_id, request_id);
        assert_eq!(last_sequence, 2);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 2);
        assert!(matches!(events[0].event, ReplEvent::VoiceListeningStarted));
    }

    #[test]
    fn select_model_command_emits_model_event_and_updates_snapshot() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();
        let model = ModelRef {
            provider: "ollama".to_string(),
            name: "qwen2.5-coder:7b".to_string(),
        };

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::SelectModel {
                model: model.clone(),
                role: ModelRole::Chat,
            },
            speak: false,
        }));

        assert!(matches!(
            result,
            CoddyResult::ActionStatus {
                request_id: actual_request_id,
                ..
            } if actual_request_id == request_id
        ));
        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.session.selected_model, model);
        assert_eq!(snapshot.last_sequence, 2);
    }

    #[test]
    fn ask_command_records_minimal_run_messages() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "  explain this module  ".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
            },
            speak: true,
        }));

        let CoddyResult::Text {
            request_id: actual_request_id,
            text,
            spoken,
        } = result
        else {
            panic!("expected text result");
        };
        let snapshot = runtime.snapshot();
        let messages = snapshot.session.messages;
        let events = runtime.events_after(1).0;

        assert_eq!(actual_request_id, request_id);
        assert_eq!(text, "Coddy runtime received: explain this module");
        assert!(spoken);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].text, "explain this module");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(snapshot.session.active_run, None);
        assert!(events
            .iter()
            .any(|event| matches!(event.event, ReplEvent::RunStarted { .. })));
        assert!(events
            .iter()
            .any(|event| matches!(event.event, ReplEvent::RunCompleted { .. })));
    }

    #[test]
    fn empty_ask_command_returns_structured_error_without_events() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "   ".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
            },
            speak: false,
        }));

        let CoddyResult::Error {
            request_id: actual_request_id,
            code,
            message,
        } = result
        else {
            panic!("expected error result");
        };

        assert_eq!(actual_request_id, request_id);
        assert_eq!(code, "invalid_command");
        assert!(message.contains("ask text"));
        assert_eq!(runtime.snapshot().last_sequence, 1);
    }

    #[test]
    fn stop_active_run_completes_current_run_when_present() {
        let request_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();
        runtime.publish_event(
            ReplEvent::RunStarted { run_id },
            Some(run_id),
            1_775_000_000_060,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::StopActiveRun,
            speak: false,
        }));

        assert!(matches!(
            result,
            CoddyResult::ActionStatus {
                request_id: actual_request_id,
                ..
            } if actual_request_id == request_id
        ));
        let snapshot = runtime.snapshot();
        let events = runtime.events_after(1).0;

        assert_eq!(snapshot.session.active_run, None);
        assert!(events.iter().any(
            |event| matches!(event.event, ReplEvent::RunCompleted { run_id: completed } if completed == run_id)
        ));
    }

    #[test]
    fn voice_turn_records_transcript_then_minimal_run() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::VoiceTurn {
                transcript_override: Some("  summarize this error ".to_string()),
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let events = runtime.events_after(1).0;

        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::VoiceTranscriptFinal { text } if text == "summarize this error"
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event.event, ReplEvent::RunCompleted { .. })));
    }

    #[tokio::test]
    async fn connection_roundtrips_wire_tools_request() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();
        let (mut client_stream, mut server_stream) = tokio::io::duplex(64 * 1024);

        let server = tokio::spawn(async move {
            runtime
                .handle_connection(&mut server_stream)
                .await
                .expect("serve request");
        });

        write_frame(
            &mut client_stream,
            &CoddyWireRequest::new(CoddyRequest::Tools(ReplToolsJob { request_id })),
        )
        .await
        .expect("write request");

        let response: CoddyWireResult =
            read_frame(&mut client_stream).await.expect("read response");
        response.ensure_compatible().expect("compatible response");

        let CoddyResult::ReplToolCatalog {
            request_id: actual_request_id,
            tools,
        } = response.result
        else {
            panic!("expected tool catalog response");
        };

        assert_eq!(actual_request_id, request_id);
        assert!(tools.iter().any(|tool| tool.name == "shell.run"));
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn connection_rejects_incompatible_wire_request() {
        let runtime = CoddyRuntime::default();
        let (mut client_stream, mut server_stream) = tokio::io::duplex(64 * 1024);
        let mut request = CoddyWireRequest::new(CoddyRequest::Tools(ReplToolsJob {
            request_id: Uuid::new_v4(),
        }));
        request.protocol_version += 1;

        write_frame(&mut client_stream, &request)
            .await
            .expect("write request");

        let error = runtime
            .handle_connection(&mut server_stream)
            .await
            .expect_err("incompatible request must fail");

        assert!(matches!(
            error,
            coddy_ipc::CoddyIpcError::IncompatibleProtocolVersion { .. }
        ));
    }

    #[tokio::test]
    async fn unix_listener_serves_coddy_client_tool_catalog() {
        let socket_path = test_socket_path("runtime-tools");
        let listener = UnixListener::bind(&socket_path).expect("bind runtime socket");
        let runtime = CoddyRuntime::default();
        let server = tokio::spawn(async move {
            runtime
                .serve_next_unix_connection(&listener)
                .await
                .expect("serve unix request");
        });

        let client = CoddyClient::new(&socket_path);
        let tools = client.tool_catalog().await.expect("tool catalog");
        let names: Vec<_> = tools.iter().map(|tool| tool.name.as_str()).collect();

        assert!(names.contains(&"filesystem.read_file"));
        assert!(names.contains(&"shell.run"));
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn unix_listener_serves_coddy_client_snapshot_and_events() {
        let socket_path = test_socket_path("runtime-session");
        let listener = UnixListener::bind(&socket_path).expect("bind runtime socket");
        let runtime = CoddyRuntime::default();
        runtime.publish_event(ReplEvent::VoiceListeningStarted, None, 1_775_000_000_030);
        let server_runtime = runtime.clone();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                server_runtime
                    .serve_next_unix_connection(&listener)
                    .await
                    .expect("serve unix request");
            }
        });

        let client = CoddyClient::new(&socket_path);
        let snapshot = client.snapshot().await.expect("session snapshot");
        let batch = client.events_after(1).await.expect("runtime events");

        assert_eq!(snapshot.last_sequence, 2);
        assert_eq!(batch.last_sequence, 2);
        assert_eq!(batch.events.len(), 1);
        assert!(matches!(
            batch.events[0].event,
            ReplEvent::VoiceListeningStarted
        ));
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn unix_listener_serves_coddy_client_command_and_snapshot_replay() {
        let socket_path = test_socket_path("runtime-command-snapshot");
        let listener = UnixListener::bind(&socket_path).expect("bind runtime socket");
        let runtime = CoddyRuntime::default();
        let server_runtime = runtime.clone();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                server_runtime
                    .serve_next_unix_connection(&listener)
                    .await
                    .expect("serve unix request");
            }
        });
        let model = ModelRef {
            provider: "ollama".to_string(),
            name: "qwen2.5-coder:7b".to_string(),
        };

        let client = CoddyClient::new(&socket_path);
        let result = client
            .send_command(
                ReplCommand::SelectModel {
                    model: model.clone(),
                    role: ModelRole::Chat,
                },
                false,
            )
            .await
            .expect("send select model command");
        let snapshot = client.snapshot().await.expect("session snapshot");

        assert!(matches!(result, CoddyResult::ActionStatus { .. }));
        assert_eq!(snapshot.session.selected_model, model);
        server.await.expect("server task");
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn unix_listener_streams_replayed_runtime_events_to_coddy_client() {
        let socket_path = test_socket_path("runtime-stream-replay");
        let listener = UnixListener::bind(&socket_path).expect("bind runtime socket");
        let runtime = CoddyRuntime::default();
        runtime.publish_event(ReplEvent::VoiceListeningStarted, None, 1_775_000_000_040);
        let server_runtime = runtime.clone();
        let server = tokio::spawn(async move {
            server_runtime
                .serve_next_unix_connection(&listener)
                .await
                .expect("serve unix stream");
        });

        let client = CoddyClient::new(&socket_path);
        let mut stream = client.event_stream(1).await.expect("open runtime stream");
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .expect("stream frame before timeout")
            .expect("stream frame")
            .expect("stream event");

        assert_eq!(frame.last_sequence, 2);
        assert!(matches!(
            frame.event.event,
            ReplEvent::VoiceListeningStarted
        ));
        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test]
    async fn unix_listener_streams_live_runtime_events_to_coddy_client() {
        let socket_path = test_socket_path("runtime-stream-live");
        let listener = UnixListener::bind(&socket_path).expect("bind runtime socket");
        let runtime = CoddyRuntime::default();
        let server_runtime = runtime.clone();
        let server = tokio::spawn(async move {
            server_runtime
                .serve_next_unix_connection(&listener)
                .await
                .expect("serve unix stream");
        });

        let client = CoddyClient::new(&socket_path);
        let mut stream = client.event_stream(1).await.expect("open runtime stream");
        runtime.publish_event(ReplEvent::VoiceListeningStarted, None, 1_775_000_000_050);
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .expect("stream frame before timeout")
            .expect("stream frame")
            .expect("stream event");

        assert_eq!(frame.last_sequence, 2);
        assert!(matches!(
            frame.event.event,
            ReplEvent::VoiceListeningStarted
        ));
        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_file(socket_path);
    }

    fn test_socket_path(label: &str) -> PathBuf {
        env::temp_dir().join(format!("coddy-runtime-{label}-{}.sock", Uuid::new_v4()))
    }
}
