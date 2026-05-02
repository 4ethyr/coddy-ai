use coddy_agent::{
    decode_provider_safe_tool_name, model_tool_call_may_run as agent_model_tool_call_may_run,
    AgentRunAction, AgentRunStopReason, AgentRunSummary, AgentRunV2, AgentToolRegistry,
    ChatMessage, ChatModelClient, ChatModelError, ChatModelResult, ChatRequest, ChatResponse,
    ChatToolCall, ChatToolSpec, DefaultChatModelClient, LocalAgentRuntime, SubagentExecutionGate,
    SubagentExecutionHandoff, SubagentExecutionStartPlan, SubagentExecutionStartStatus,
    SubagentOutputContract, LIST_FILES_TOOL, READ_FILE_TOOL, SEARCH_FILES_TOOL,
    SUBAGENT_PREPARE_TOOL, SUBAGENT_ROUTE_TOOL, SUBAGENT_TEAM_PLAN_TOOL,
};
use coddy_core::{
    ContextItem, ContextPolicy, ConversationRecord, ModelCredential, ModelRef, ModelRole,
    PermissionReply, ReplCommand, ReplEvent, ReplEventBroker, ReplEventEnvelope, ReplIntent,
    ReplMessage, ReplMode, ReplSession, ReplSessionSnapshot, SubagentHandoffPrepared,
    SubagentLifecycleStatus, SubagentLifecycleUpdate, SubagentRouteRecommendation, ToolCall,
    ToolDefinition, ToolName, ToolOutput, ToolResultStatus,
};
use coddy_ipc::{
    read_frame, write_frame, CoddyIpcResult, CoddyRequest, CoddyResult, CoddyWireRequest,
    CoddyWireResult, ReplCommandJob, ReplEventStreamJob, ReplToolCatalogItem,
};
use serde_json::json;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixListener;
use uuid::Uuid;

const MAX_MODEL_TOOL_ROUNDS: usize = 5;
const MAX_MODEL_REQUEST_ATTEMPTS: usize = 3;
const MODEL_RETRY_BASE_DELAY_MS: u64 = 250;

#[derive(Debug, Clone)]
pub struct CoddyRuntime {
    tool_registry: AgentToolRegistry,
    agent_runtime: Option<LocalAgentRuntime>,
    chat_client: Arc<dyn ChatModelClient>,
    state: Arc<Mutex<RuntimeState>>,
}

#[derive(Debug)]
struct RuntimeState {
    session: ReplSession,
    broker: ReplEventBroker,
    agent_runs: HashMap<Uuid, AgentRunV2>,
    conversation_history: ConversationHistoryStore,
}

#[derive(Debug)]
struct ConversationHistoryStore {
    path: Option<PathBuf>,
    records: Vec<ConversationRecord>,
}

struct ModelBackedTurn<'a> {
    session_id: Uuid,
    run_id: Uuid,
    selected_model: &'a ModelRef,
    context_policy: ContextPolicy,
    session_context: &'a ReplSession,
    model_credential: Option<ModelCredential>,
    user_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToolUsePolicy {
    max_tool_calls: Option<usize>,
}

struct ModelResponseContext<'a> {
    session_id: Uuid,
    run_id: Uuid,
    selected_model: &'a ModelRef,
    model_credential: Option<ModelCredential>,
    system_prompt: &'a str,
    tool_use_policy: ToolUsePolicy,
    goal: String,
}

struct ToolRoundOutcome {
    response: AssistantResponse,
    executed_tool_calls: usize,
    pending_permission: bool,
}

enum OpenConversationError {
    NotFound,
    HistoryWriteFailed(String),
}

impl CoddyRuntime {
    pub fn new(tool_registry: AgentToolRegistry) -> Self {
        let agent_runtime =
            default_workspace_root().and_then(|workspace| LocalAgentRuntime::new(workspace).ok());
        Self::new_with_agent_runtime(tool_registry, agent_runtime)
    }

    pub fn with_workspace(
        tool_registry: AgentToolRegistry,
        workspace_root: impl AsRef<Path>,
    ) -> Result<Self, coddy_agent::AgentError> {
        Ok(Self::new_with_agent_runtime(
            tool_registry,
            Some(LocalAgentRuntime::new(workspace_root)?),
        ))
    }

    fn new_with_agent_runtime(
        tool_registry: AgentToolRegistry,
        agent_runtime: Option<LocalAgentRuntime>,
    ) -> Self {
        Self::new_with_agent_runtime_and_chat_client(
            tool_registry,
            agent_runtime,
            Arc::new(DefaultChatModelClient::default()),
        )
    }

    pub fn with_chat_client(
        tool_registry: AgentToolRegistry,
        chat_client: Arc<dyn ChatModelClient>,
    ) -> Self {
        let agent_runtime =
            default_workspace_root().and_then(|workspace| LocalAgentRuntime::new(workspace).ok());
        Self::new_with_agent_runtime_and_chat_client(tool_registry, agent_runtime, chat_client)
    }

    pub fn with_workspace_and_chat_client(
        tool_registry: AgentToolRegistry,
        workspace_root: impl AsRef<Path>,
        chat_client: Arc<dyn ChatModelClient>,
    ) -> Result<Self, coddy_agent::AgentError> {
        Ok(Self::new_with_agent_runtime_and_chat_client(
            tool_registry,
            Some(LocalAgentRuntime::new(workspace_root)?),
            chat_client,
        ))
    }

    fn new_with_agent_runtime_and_chat_client(
        tool_registry: AgentToolRegistry,
        agent_runtime: Option<LocalAgentRuntime>,
        chat_client: Arc<dyn ChatModelClient>,
    ) -> Self {
        Self {
            tool_registry,
            agent_runtime,
            chat_client,
            state: Arc::new(Mutex::new(RuntimeState::new(default_session()))),
        }
    }

    pub fn with_conversation_history_path(self, path: impl Into<PathBuf>) -> Self {
        self.with_state_mut(|state| {
            state.conversation_history = ConversationHistoryStore::open(Some(path.into()));
        });
        self
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
            CoddyRequest::ConversationHistory(job) => CoddyResult::ReplConversationHistory {
                request_id: job.request_id,
                conversations: self.conversation_history(job.limit),
            },
            CoddyRequest::EventStream(job) => CoddyResult::Error {
                request_id: job.request_id,
                code: "unsupported_request".to_string(),
                message: "Use the streaming runtime connection for event streams.".to_string(),
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
            ReplCommand::Ask {
                text,
                context_policy,
                model_credential,
            } => self.handle_ask(request_id, text, context_policy, model_credential, speak),
            ReplCommand::VoiceTurn {
                transcript_override,
            } => match normalize_text(transcript_override.unwrap_or_default()) {
                Some(transcript) => {
                    self.publish_event_now(ReplEvent::VoiceTranscriptFinal {
                        text: transcript.clone(),
                    });
                    self.handle_ask(
                        request_id,
                        transcript,
                        ContextPolicy::WorkspaceOnly,
                        None,
                        speak,
                    )
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
            ReplCommand::ReplyPermission {
                request_id: permission_request_id,
                reply,
            } => self.handle_permission_reply(request_id, permission_request_id, reply, speak),
            ReplCommand::DismissConfirmation => {
                self.publish_event_now(ReplEvent::ConfirmationDismissed);
                CoddyResult::ActionStatus {
                    request_id,
                    message: "Confirmation dismissed".to_string(),
                    spoken: speak,
                }
            }
            ReplCommand::NewSession => self.handle_new_session(request_id, speak),
            ReplCommand::OpenConversation { session_id } => {
                self.handle_open_conversation(request_id, session_id, speak)
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
                    self.transition_agent_run(
                        run_id,
                        AgentRunAction::Cancel {
                            reason: AgentRunStopReason::UserInterrupt,
                        },
                    );
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

    fn handle_permission_reply(
        &self,
        request_id: Uuid,
        permission_request_id: Uuid,
        reply: PermissionReply,
        speak: bool,
    ) -> CoddyResult {
        let snapshot = self.snapshot();
        let Some(permission_request) = snapshot.session.pending_permission.clone() else {
            return CoddyResult::Error {
                request_id,
                code: "permission_not_pending".to_string(),
                message: "No tool permission request is pending.".to_string(),
            };
        };

        if permission_request.id != permission_request_id {
            return CoddyResult::Error {
                request_id,
                code: "permission_request_mismatch".to_string(),
                message: format!("Pending permission request is {}.", permission_request.id),
            };
        }

        let Some(agent_runtime) = &self.agent_runtime else {
            return CoddyResult::Error {
                request_id,
                code: "agent_runtime_unavailable".to_string(),
                message: "Coddy cannot reply to tool permissions without a workspace runtime."
                    .to_string(),
            };
        };

        let run_id = permission_request.run_id;
        let mut state = agent_runtime.start_run_with_id(
            permission_request.session_id,
            run_id,
            format!("Reply to permission request {permission_request_id}"),
        );
        let outcome = agent_runtime.reply_permission(&mut state, permission_request_id, reply);

        for event in outcome.events {
            self.publish_event_with_run_now(event, run_id);
        }
        let result = outcome.result;
        if let Some(result) = result.as_ref() {
            if result.status == ToolResultStatus::Succeeded {
                if let Some(output) = result.output.as_ref() {
                    if let Some(item) =
                        context_item_from_tool_output(&permission_request.tool_name, output)
                    {
                        self.publish_event_with_run_now(
                            ReplEvent::ContextItemAdded { item },
                            run_id,
                        );
                    }
                }
            }
        }
        self.publish_event_with_run_now(ReplEvent::RunCompleted { run_id }, run_id);

        match result {
            Some(result) => match result.status {
                ToolResultStatus::Succeeded => CoddyResult::ActionStatus {
                    request_id,
                    message: format!(
                        "Permission {reply:?} accepted for {}.",
                        permission_request.tool_name
                    ),
                    spoken: speak,
                },
                ToolResultStatus::Denied => CoddyResult::ActionStatus {
                    request_id,
                    message: format!(
                        "Permission {reply:?} denied for {}.",
                        permission_request.tool_name
                    ),
                    spoken: speak,
                },
                ToolResultStatus::Failed | ToolResultStatus::Cancelled => {
                    let message = result
                        .error
                        .map(|error| error.message)
                        .unwrap_or_else(|| "permission reply failed".to_string());
                    CoddyResult::Error {
                        request_id,
                        code: "permission_reply_failed".to_string(),
                        message,
                    }
                }
            },
            None => CoddyResult::Error {
                request_id,
                code: "permission_reply_incomplete".to_string(),
                message: "Permission reply did not produce a tool result.".to_string(),
            },
        }
    }

    fn handle_new_session(&self, request_id: Uuid, speak: bool) -> CoddyResult {
        match self.start_new_session() {
            Ok((previous_session_id, new_session_id)) => CoddyResult::ActionStatus {
                request_id,
                message: format!(
                    "Started a new Coddy session {new_session_id}; archived {previous_session_id}."
                ),
                spoken: speak,
            },
            Err(message) => CoddyResult::Error {
                request_id,
                code: "conversation_history_write_failed".to_string(),
                message,
            },
        }
    }

    fn handle_open_conversation(
        &self,
        request_id: Uuid,
        session_id: Uuid,
        speak: bool,
    ) -> CoddyResult {
        match self.open_conversation(session_id) {
            Ok(()) => CoddyResult::ActionStatus {
                request_id,
                message: format!("Opened Coddy conversation {session_id}."),
                spoken: speak,
            },
            Err(OpenConversationError::NotFound) => CoddyResult::Error {
                request_id,
                code: "conversation_not_found".to_string(),
                message: format!("No persisted Coddy conversation found for {session_id}."),
            },
            Err(OpenConversationError::HistoryWriteFailed(message)) => CoddyResult::Error {
                request_id,
                code: "conversation_history_write_failed".to_string(),
                message,
            },
        }
    }

    fn start_new_session(&self) -> Result<(Uuid, Uuid), String> {
        self.with_state_mut(|state| {
            let now = unix_ms_now();
            let current_session = state.broker.snapshot(state.session.clone()).session;
            state
                .conversation_history
                .sync_session(&current_session, now)?;

            let previous_session_id = current_session.id;
            let mut next_session =
                ReplSession::new(current_session.mode, current_session.selected_model);
            let next_session_id = next_session.id;
            state.broker.reset_session(next_session_id, now);
            next_session = state.broker.replay(next_session);
            state.session = next_session;
            state.agent_runs.clear();

            Ok((previous_session_id, next_session_id))
        })
    }

    fn open_conversation(&self, session_id: Uuid) -> Result<(), OpenConversationError> {
        self.with_state_mut(|state| {
            let now = unix_ms_now();
            let current_session = state.broker.snapshot(state.session.clone()).session;
            if current_session.id == session_id {
                return Ok(());
            }

            state
                .conversation_history
                .sync_session(&current_session, now)
                .map_err(OpenConversationError::HistoryWriteFailed)?;

            let Some(record) = state
                .conversation_history
                .records
                .iter()
                .find(|record| record.summary.session_id == session_id)
                .cloned()
            else {
                return Err(OpenConversationError::NotFound);
            };

            state.broker.reset_session(record.summary.session_id, now);
            state.broker.publish(
                ReplEvent::OverlayShown {
                    mode: record.summary.mode,
                },
                None,
                now,
            );
            state.broker.publish(
                ReplEvent::ModelSelected {
                    model: record.summary.selected_model.clone(),
                    role: ModelRole::Chat,
                },
                None,
                now,
            );
            for message in &record.messages {
                state.broker.publish(
                    ReplEvent::MessageAppended {
                        message: message.clone(),
                    },
                    None,
                    now,
                );
            }

            let base_session = ReplSession::new(record.summary.mode, record.summary.selected_model);
            state.session = state.broker.replay(base_session);
            state.agent_runs.clear();
            Ok(())
        })
    }

    fn conversation_history(&self, limit: Option<usize>) -> Vec<ConversationRecord> {
        let current_snapshot = self.snapshot().session;
        let current_record = ConversationRecord::from_session(&current_snapshot, unix_ms_now());

        self.with_state(|state| {
            let mut records = state.conversation_history.records.clone();
            if let Some(current_record) = current_record {
                upsert_history_record(&mut records, current_record);
            }
            records.sort_by_key(|record| std::cmp::Reverse(record.summary.updated_at_unix_ms));
            if let Some(limit) = limit {
                records.truncate(limit);
            }
            records
        })
    }

    fn persist_current_conversation(&self) {
        let session = self.snapshot().session;
        self.with_state_mut(|state| {
            let _ = state
                .conversation_history
                .sync_session(&session, unix_ms_now());
        });
    }

    fn handle_ask(
        &self,
        request_id: Uuid,
        text: String,
        context_policy: ContextPolicy,
        model_credential: Option<ModelCredential>,
        speak: bool,
    ) -> CoddyResult {
        let Some(text) = normalize_text(text) else {
            return invalid_command(request_id, "ask text is required");
        };

        let session_id = self.session_id();
        let session_context = self.snapshot().session;
        let selected_model = session_context.selected_model.clone();
        let action = plan_ask_action(&text);
        let run_id = Uuid::new_v4();
        let (intent, confidence) = classify_ask_intent(&text, &action);

        self.publish_event_with_run_now(
            ReplEvent::MessageAppended {
                message: repl_message("user", text.clone()),
            },
            run_id,
        );
        self.publish_event_with_run_now(ReplEvent::RunStarted { run_id }, run_id);
        self.start_agent_run(run_id, text.clone());
        self.transition_agent_run(run_id, AgentRunAction::Plan);
        self.publish_event_with_run_now(ReplEvent::IntentDetected { intent, confidence }, run_id);

        let assistant_response = match action {
            AskAction::ListWorkspace { path } => {
                self.transition_agent_run(run_id, AgentRunAction::Inspect);
                self.execute_workspace_listing(session_id, run_id, &text, &path, selected_model)
            }
            AskAction::ModelBackedResponse => {
                self.transition_agent_run(run_id, AgentRunAction::Inspect);
                self.execute_model_backed_response(ModelBackedTurn {
                    session_id,
                    run_id,
                    selected_model: &selected_model,
                    context_policy,
                    session_context: &session_context,
                    model_credential,
                    user_text: text.clone(),
                })
            }
        };

        for delta in assistant_response.deltas() {
            self.publish_event_with_run_now(
                ReplEvent::TokenDelta {
                    run_id,
                    text: delta.clone(),
                },
                run_id,
            );
        }
        self.publish_event_with_run_now(
            ReplEvent::MessageAppended {
                message: repl_message("assistant", assistant_response.text.clone()),
            },
            run_id,
        );
        self.complete_agent_run_if_active(run_id);
        self.publish_event_with_run_now(ReplEvent::RunCompleted { run_id }, run_id);
        self.persist_current_conversation();

        CoddyResult::Text {
            request_id,
            text: assistant_response.text,
            spoken: speak,
        }
    }

    fn execute_workspace_listing(
        &self,
        session_id: Uuid,
        run_id: Uuid,
        goal: &str,
        path: &str,
        selected_model: ModelRef,
    ) -> AssistantResponse {
        let Some(agent_runtime) = &self.agent_runtime else {
            return AssistantResponse::from_text(
                "Coddy cannot access the workspace from this runtime process yet.",
            );
        };

        let mut state = agent_runtime.start_run_with_id(session_id, run_id, goal.to_string());
        agent_runtime.add_plan_item(
            &mut state,
            format!("List workspace files in {path}"),
            Some(ToolName::new(LIST_FILES_TOOL).expect("built-in tool name")),
        );
        let call = ToolCall::new(
            session_id,
            run_id,
            ToolName::new(LIST_FILES_TOOL).expect("built-in tool name"),
            json!({
                "path": path,
                "max_entries": 80,
            }),
            unix_ms_now(),
        );
        let outcome = agent_runtime.execute_tool_call(&mut state, &call);
        for event in outcome.events {
            self.publish_event_with_run_now(event, run_id);
        }

        let Some(result) = outcome.result else {
            return AssistantResponse::from_text(
                "Coddy started a workspace listing but did not receive a tool result.",
            );
        };

        let text = match result.status {
            ToolResultStatus::Succeeded => {
                let Some(output) = result.output else {
                    return AssistantResponse::from_text(format!(
                        "Workspace entries under `{path}`: no structured output."
                    ));
                };
                let entries = output.text.trim();
                let scope = if path == "." { "workspace" } else { path };
                let mut response = if entries.is_empty() {
                    format!("No entries found under `{scope}`.")
                } else {
                    format!("Workspace entries under `{scope}`:\n{entries}")
                };
                if output.truncated {
                    response.push_str("\n\nResult truncated. Narrow the path to inspect more.");
                }
                if selected_model.name == "unselected" {
                    response.push_str(
                        "\n\nNo chat model is selected yet; this answer used only the safe local filesystem tool.",
                    );
                }
                response
            }
            ToolResultStatus::Failed | ToolResultStatus::Cancelled | ToolResultStatus::Denied => {
                let message = result
                    .error
                    .map(|error| error.message)
                    .unwrap_or_else(|| "unknown tool failure".to_string());
                format!("I could not list workspace entries under `{path}`: {message}")
            }
        };
        AssistantResponse::from_text(text)
    }

    fn execute_model_backed_response(&self, turn: ModelBackedTurn<'_>) -> AssistantResponse {
        let ModelBackedTurn {
            session_id,
            run_id,
            selected_model,
            context_policy,
            session_context,
            model_credential,
            user_text,
        } = turn;

        let tool_use_policy = tool_use_policy_from_text(&user_text);
        let mut system_prompt = build_model_system_prompt(
            context_policy,
            session_context,
            self.tool_registry.definitions(),
            tool_use_policy,
        );
        if tool_use_policy.max_tool_calls.is_none() {
            if let Some(routing_context) =
                self.prepare_subagent_routing_context(session_id, run_id, &user_text)
            {
                system_prompt.push_str("\n\n");
                system_prompt.push_str(&routing_context);
            }
        }
        let request = match ChatRequest::new(
            selected_model.clone(),
            vec![
                ChatMessage::system(system_prompt.clone()),
                ChatMessage::user(user_text.clone()),
            ],
        ) {
            Ok(request) => match request.with_model_credential(model_credential.clone()) {
                Ok(request) => request.with_tools(self.chat_tool_specs_for_policy(tool_use_policy)),
                Err(error) => {
                    self.fail_agent_run(run_id, &error);
                    return AssistantResponse::from_text(model_error_message(
                        &error,
                        selected_model,
                        self.tool_registry.definitions().len(),
                    ));
                }
            },
            Err(error) => {
                self.fail_agent_run(run_id, &error);
                return AssistantResponse::from_text(model_error_message(
                    &error,
                    selected_model,
                    self.tool_registry.definitions().len(),
                ));
            }
        };

        match self.complete_model_request_with_retry(request) {
            Ok(response) => self.assistant_response_from_model(
                ModelResponseContext {
                    session_id,
                    run_id,
                    selected_model,
                    model_credential,
                    system_prompt: &system_prompt,
                    tool_use_policy,
                    goal: user_text,
                },
                response,
            ),
            Err(error) => {
                self.fail_agent_run(run_id, &error);
                AssistantResponse::from_text(model_error_message(
                    &error,
                    selected_model,
                    self.tool_registry.definitions().len(),
                ))
            }
        }
    }

    fn assistant_response_from_model(
        &self,
        context: ModelResponseContext<'_>,
        response: ChatResponse,
    ) -> AssistantResponse {
        if response.tool_calls.is_empty() {
            return AssistantResponse::from_chat_response(response);
        }

        self.execute_model_tool_calls(context, response)
    }

    fn execute_model_tool_calls(
        &self,
        context: ModelResponseContext<'_>,
        response: ChatResponse,
    ) -> AssistantResponse {
        let Some(agent_runtime) = &self.agent_runtime else {
            return AssistantResponse::from_chat_response(response);
        };

        let mut state = agent_runtime.start_run_with_id(
            context.session_id,
            context.run_id,
            context.goal.clone(),
        );
        let mut messages = vec![
            ChatMessage::system(build_tool_followup_system_prompt(context.system_prompt)),
            ChatMessage::user(context.goal.clone()),
        ];
        let mut response = response;
        let mut last_tool_observations = None;
        let mut remaining_tool_calls = context.tool_use_policy.max_tool_calls;

        for _ in 0..MAX_MODEL_TOOL_ROUNDS {
            let round = self.execute_model_tool_round(
                agent_runtime,
                &mut state,
                &context,
                &response,
                remaining_tool_calls,
            );
            if round.executed_tool_calls == 0 {
                return round.response;
            }
            if let Some(remaining) = remaining_tool_calls.as_mut() {
                *remaining = remaining.saturating_sub(round.executed_tool_calls);
            }
            if round.pending_permission {
                return round.response;
            }
            last_tool_observations = Some(round.response.text.clone());

            if !response.text.trim().is_empty() {
                messages.push(ChatMessage::assistant(response.text.clone()));
            }
            messages.push(ChatMessage::tool(round.response.text.clone()));

            let next_response = match self.complete_after_tool_messages(
                context.selected_model,
                context.model_credential.clone(),
                messages.clone(),
                remaining_tool_calls.is_none_or(|remaining| remaining > 0),
            ) {
                Ok(response) => response,
                Err(error) => {
                    self.fail_agent_run(context.run_id, &error);
                    let mut text = round.response.text;
                    text.push_str("\n\n");
                    text.push_str(
                        "Coddy could not get a follow-up response after tool observations: ",
                    );
                    text.push_str(&model_error_message(
                        &error,
                        context.selected_model,
                        self.tool_registry.definitions().len(),
                    ));
                    return AssistantResponse::from_text(redact_context_text(&text));
                }
            };
            if next_response.tool_calls.is_empty() {
                return AssistantResponse::from_chat_response(next_response);
            }
            response = next_response;
        }

        let tool_summary = summarize_chat_tool_calls(&response.tool_calls);
        let text = build_tool_round_limit_response(
            response.text.trim(),
            &tool_summary,
            last_tool_observations.as_deref(),
        );
        AssistantResponse::from_text(redact_context_text(&text))
    }

    fn prepare_subagent_routing_context(
        &self,
        session_id: Uuid,
        run_id: Uuid,
        goal: &str,
    ) -> Option<String> {
        let agent_runtime = self.agent_runtime.as_ref()?;
        let mut state = agent_runtime.start_run_with_id(
            session_id,
            run_id,
            format!("Route subagents for: {goal}"),
        );
        agent_runtime.add_plan_item(
            &mut state,
            "Recommend focused subagents for this turn",
            Some(ToolName::new(SUBAGENT_ROUTE_TOOL).expect("built-in tool name")),
        );
        let call = ToolCall::new(
            session_id,
            run_id,
            ToolName::new(SUBAGENT_ROUTE_TOOL).expect("built-in tool name"),
            json!({
                "goal": goal,
                "limit": 3,
            }),
            unix_ms_now(),
        );
        let outcome = agent_runtime.execute_tool_call(&mut state, &call);
        for event in outcome.events {
            self.publish_event_with_run_now(event, run_id);
        }

        let result = outcome.result?;
        if result.status != ToolResultStatus::Succeeded {
            return None;
        }
        let output = result.output.as_ref()?;
        let recommendations = subagent_recommendations_from_output(output);
        let top_recommendation = recommendations
            .first()
            .map(|recommendation| recommendation.name.clone());
        if !recommendations.is_empty() {
            self.publish_event_with_run_now(ReplEvent::SubagentRouted { recommendations }, run_id);
        }
        let mut sections = vec![format_subagent_routing_context(output)];
        if let Some(subagent_name) = top_recommendation {
            if let Some(handoff_context) = self.prepare_subagent_handoff_context(
                agent_runtime,
                &mut state,
                session_id,
                run_id,
                goal,
                &subagent_name,
            ) {
                sections.push(handoff_context);
            }
        }
        if let Some(team_context) =
            self.prepare_subagent_team_context(agent_runtime, &mut state, session_id, run_id, goal)
        {
            sections.push(team_context);
        }

        Some(sections.join("\n\n"))
    }

    fn prepare_subagent_team_context(
        &self,
        agent_runtime: &LocalAgentRuntime,
        state: &mut coddy_agent::RunState,
        session_id: Uuid,
        run_id: Uuid,
        goal: &str,
    ) -> Option<String> {
        agent_runtime.add_plan_item(
            state,
            "Compose measurable multiagent team plan",
            Some(ToolName::new(SUBAGENT_TEAM_PLAN_TOOL).expect("built-in tool name")),
        );
        let call = ToolCall::new(
            session_id,
            run_id,
            ToolName::new(SUBAGENT_TEAM_PLAN_TOOL).expect("built-in tool name"),
            json!({
                "goal": goal,
                "max_members": 6,
            }),
            unix_ms_now(),
        );
        let outcome = agent_runtime.execute_tool_call(state, &call);
        for event in outcome.events {
            self.publish_event_with_run_now(event, run_id);
        }

        let result = outcome.result?;
        if result.status != ToolResultStatus::Succeeded {
            return None;
        }
        let output = result.output.as_ref()?;
        Some(format_subagent_team_context(output))
    }

    fn prepare_subagent_handoff_context(
        &self,
        agent_runtime: &LocalAgentRuntime,
        state: &mut coddy_agent::RunState,
        session_id: Uuid,
        run_id: Uuid,
        goal: &str,
        subagent_name: &str,
    ) -> Option<String> {
        agent_runtime.add_plan_item(
            state,
            format!("Prepare handoff contract for {subagent_name}"),
            Some(ToolName::new(SUBAGENT_PREPARE_TOOL).expect("built-in tool name")),
        );
        let call = ToolCall::new(
            session_id,
            run_id,
            ToolName::new(SUBAGENT_PREPARE_TOOL).expect("built-in tool name"),
            json!({
                "name": subagent_name,
                "goal": goal,
            }),
            unix_ms_now(),
        );
        let outcome = agent_runtime.execute_tool_call(state, &call);
        for event in outcome.events {
            self.publish_event_with_run_now(event, run_id);
        }

        let result = outcome.result?;
        if result.status != ToolResultStatus::Succeeded {
            return None;
        }
        let output = result.output.as_ref()?;
        let mut execution_gate_context = None;
        let mut output_contract_context = None;
        if let Some(handoff) = subagent_handoff_prepared_from_output(output) {
            let execution_handoff = SubagentExecutionHandoff::from(&handoff);
            let output_contract = SubagentOutputContract::from(&handoff);
            let execution_plan = SubagentExecutionGate.plan_start_for(&execution_handoff, false);
            let update = execution_plan
                .lifecycle_updates
                .first()
                .cloned()
                .unwrap_or_else(|| subagent_lifecycle_blocked_update(&handoff));
            self.publish_event_with_run_now(ReplEvent::SubagentHandoffPrepared { handoff }, run_id);
            self.publish_event_with_run_now(ReplEvent::SubagentLifecycleUpdated { update }, run_id);
            execution_gate_context = Some(format_subagent_execution_gate_context(&execution_plan));
            output_contract_context =
                Some(format_subagent_output_contract_context(&output_contract));
        }
        Some(
            [
                Some(format_subagent_handoff_context(output)),
                execution_gate_context,
                output_contract_context,
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n\n"),
        )
    }

    fn execute_model_tool_round(
        &self,
        agent_runtime: &LocalAgentRuntime,
        state: &mut coddy_agent::RunState,
        context: &ModelResponseContext<'_>,
        response: &ChatResponse,
        mut remaining_tool_calls: Option<usize>,
    ) -> ToolRoundOutcome {
        let mut observations = Vec::new();
        let mut executed_tool_calls = 0_usize;
        let mut pending_permission = false;

        for tool_call in response.tool_calls.iter().take(3) {
            let requested_tool_name = decode_provider_safe_tool_name(&tool_call.name);
            let tool_name = match ToolName::new(&requested_tool_name) {
                Ok(tool_name) => tool_name,
                Err(error) => {
                    observations.push(format!(
                        "- `{}` was rejected because the tool name is invalid: {error}.",
                        tool_call.name
                    ));
                    continue;
                }
            };

            if remaining_tool_calls == Some(0) {
                observations.push(format!(
                    "- `{tool_name}` was not executed because this turn reached the user-requested tool budget."
                ));
                continue;
            }

            let Some(definition) = self.tool_registry.get(&tool_name) else {
                observations.push(format!(
                    "- `{tool_name}` was rejected because it is not registered in the local tool registry."
                ));
                continue;
            };

            if !agent_model_tool_call_may_run(&tool_name, definition) {
                observations.push(format!(
                    "- `{tool_name}` was not executed because model-initiated tools must be auto-approved and low risk, except edit previews that only prepare an approval request."
                ));
                continue;
            }

            agent_runtime.add_plan_item(
                state,
                format!("Run model-requested tool {tool_name}"),
                Some(tool_name.clone()),
            );
            let arguments =
                normalize_model_initiated_tool_input(&tool_name, tool_call.arguments.clone());
            let call = ToolCall::new(
                context.session_id,
                context.run_id,
                tool_name.clone(),
                arguments,
                unix_ms_now(),
            );
            let outcome = agent_runtime.execute_tool_call(state, &call);
            executed_tool_calls += 1;
            if let Some(remaining) = remaining_tool_calls.as_mut() {
                *remaining = remaining.saturating_sub(1);
            }
            for event in outcome.events {
                self.publish_event_with_run_now(event, context.run_id);
            }

            if outcome.permission_request.is_some() && outcome.result.is_none() {
                pending_permission = true;
                let patterns = outcome
                    .permission_request
                    .as_ref()
                    .map(|request| request.patterns.join(", "))
                    .filter(|patterns| !patterns.is_empty())
                    .unwrap_or_else(|| "requested target".to_string());
                observations.push(format!(
                    "- `{tool_name}` requires approval before accessing sensitive workspace content: {patterns}."
                ));
                continue;
            }

            let Some(result) = outcome.result else {
                observations.push(format!(
                    "- `{tool_name}` did not return a tool result from the local runtime."
                ));
                continue;
            };

            match result.status {
                ToolResultStatus::Succeeded => {
                    if let Some(output) = result.output.as_ref() {
                        if let Some(item) = context_item_from_tool_output(&tool_name, output) {
                            self.publish_event_with_run_now(
                                ReplEvent::ContextItemAdded { item },
                                context.run_id,
                            );
                        }
                    }
                    let text = result
                        .output
                        .map(|output| {
                            let mut text = output.text.trim().to_string();
                            if output.truncated {
                                text.push_str("\n  Result truncated.");
                            }
                            text
                        })
                        .filter(|text| !text.is_empty())
                        .unwrap_or_else(|| "no structured output".to_string());
                    observations.push(format!("- `{tool_name}` succeeded:\n{text}"));
                }
                ToolResultStatus::Failed
                | ToolResultStatus::Cancelled
                | ToolResultStatus::Denied => {
                    let message = result
                        .error
                        .map(|error| error.message)
                        .unwrap_or_else(|| "unknown tool failure".to_string());
                    observations.push(format!("- `{tool_name}` failed: {message}"));
                }
            }
        }

        if response.tool_calls.len() > 3 {
            observations.push(format!(
                "- {} additional model-requested tool calls were not executed in this turn.",
                response.tool_calls.len() - 3
            ));
        }

        let mut text = response.text.trim().to_string();
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str("Tool observations:\n");
        text.push_str(&observations.join("\n"));
        let text = redact_context_text(&text);

        ToolRoundOutcome {
            response: AssistantResponse::from_text(text),
            executed_tool_calls,
            pending_permission,
        }
    }

    fn complete_after_tool_messages(
        &self,
        selected_model: &ModelRef,
        model_credential: Option<ModelCredential>,
        messages: Vec<ChatMessage>,
        tools_enabled: bool,
    ) -> ChatModelResult {
        let mut request = ChatRequest::new(selected_model.clone(), messages)
            .and_then(|request| request.with_model_credential(model_credential))?;
        if tools_enabled {
            request = request.with_tools(self.chat_tool_specs());
        }
        self.complete_model_request_with_retry(request)
    }

    fn complete_model_request_with_retry(&self, request: ChatRequest) -> ChatModelResult {
        let mut last_error = None;
        for attempt in 0..MAX_MODEL_REQUEST_ATTEMPTS {
            match self.chat_client.complete(request.clone()) {
                Ok(response) => return Ok(response),
                Err(error)
                    if attempt + 1 < MAX_MODEL_REQUEST_ATTEMPTS
                        && should_retry_model_request_error(&error) =>
                {
                    last_error = Some(error);
                    sleep_before_model_retry(attempt);
                }
                Err(error) => return Err(error),
            }
        }

        Err(
            last_error.unwrap_or_else(|| ChatModelError::InvalidProviderResponse {
                provider: request.model.provider,
                message: "model retry exhausted without provider response".to_string(),
            }),
        )
    }

    fn chat_tool_specs(&self) -> Vec<ChatToolSpec> {
        self.tool_registry
            .definitions()
            .iter()
            .filter(|definition| agent_model_tool_call_may_run(&definition.name, definition))
            .map(ChatToolSpec::from_tool_definition)
            .collect()
    }

    fn chat_tool_specs_for_policy(&self, policy: ToolUsePolicy) -> Vec<ChatToolSpec> {
        if policy.max_tool_calls == Some(0) {
            Vec::new()
        } else {
            self.chat_tool_specs()
        }
    }

    pub fn snapshot(&self) -> ReplSessionSnapshot {
        self.with_state(|state| state.broker.snapshot(state.session.clone()))
    }

    fn session_id(&self) -> Uuid {
        self.with_state(|state| state.session.id)
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

    pub async fn serve_unix_listener(&self, listener: UnixListener) -> CoddyIpcResult<()> {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let runtime = self.clone();
            tokio::spawn(async move {
                let _ = runtime.handle_connection(&mut stream).await;
            });
        }
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

    pub fn agent_run_summary(&self, run_id: Uuid) -> Option<AgentRunSummary> {
        self.with_state(|state| state.agent_runs.get(&run_id).map(AgentRunV2::summary))
    }

    fn start_agent_run(&self, run_id: Uuid, goal: impl Into<String>) {
        let summary = self.with_state_mut(|state| {
            let run = AgentRunV2::start(goal);
            let summary = run.summary();
            state.agent_runs.insert(run_id, run);
            summary
        });
        self.publish_agent_run_update(run_id, summary);
    }

    fn transition_agent_run(&self, run_id: Uuid, action: AgentRunAction) {
        let outcome = self.with_state_mut(|state| {
            state
                .agent_runs
                .get_mut(&run_id)
                .map(|run| run.transition(action).map(|_| run.summary()))
        });

        match outcome {
            Some(Ok(summary)) => self.publish_agent_run_update(run_id, summary),
            Some(Err(error)) => {
                self.publish_event_with_run_now(
                    ReplEvent::Error {
                        code: error.code().to_string(),
                        message: error.to_string(),
                    },
                    run_id,
                );
            }
            None => {}
        }
    }

    fn publish_agent_run_update(&self, run_id: Uuid, summary: AgentRunSummary) {
        self.publish_event_with_run_now(ReplEvent::AgentRunUpdated { run_id, summary }, run_id);
    }

    fn complete_agent_run_if_active(&self, run_id: Uuid) {
        let should_complete = self.with_state(|state| {
            state
                .agent_runs
                .get(&run_id)
                .is_some_and(|run| !run.phase().is_terminal())
        });
        if should_complete {
            self.transition_agent_run(run_id, AgentRunAction::Complete);
        }
    }

    fn fail_agent_run(&self, run_id: Uuid, error: &ChatModelError) {
        self.transition_agent_run(
            run_id,
            AgentRunAction::Fail {
                code: error.code().to_string(),
                message: error.to_string(),
                recoverable: error.retryable(),
            },
        );
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
        Self {
            session,
            broker,
            agent_runs: HashMap::new(),
            conversation_history: ConversationHistoryStore::open(None),
        }
    }
}

impl ConversationHistoryStore {
    fn open(path: Option<PathBuf>) -> Self {
        let records = path
            .as_ref()
            .and_then(|path| read_history_records(path).ok())
            .unwrap_or_default();

        Self { path, records }
    }

    fn sync_session(
        &mut self,
        session: &ReplSession,
        captured_at_unix_ms: u64,
    ) -> Result<Option<ConversationRecord>, String> {
        let Some(record) = ConversationRecord::from_session(session, captured_at_unix_ms) else {
            return Ok(None);
        };

        upsert_history_record(&mut self.records, record.clone());
        if let Some(path) = &self.path {
            write_history_records(path, &self.records)
                .map_err(|error| format!("failed to write conversation history: {error}"))?;
        }

        Ok(Some(record))
    }
}

fn upsert_history_record(records: &mut Vec<ConversationRecord>, mut record: ConversationRecord) {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.summary.session_id == record.summary.session_id)
    {
        record.summary.created_at_unix_ms = existing.summary.created_at_unix_ms;
        *existing = record;
    } else {
        records.push(record);
    }
}

fn read_history_records(path: &Path) -> io::Result<Vec<ConversationRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str(&raw).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn write_history_records(path: &Path, records: &[ConversationRecord]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options.open(&tmp_path)?;
    serde_json::to_writer_pretty(&mut file, records).map_err(io::Error::other)?;
    fs::rename(&tmp_path, path)?;

    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;

    Ok(())
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum AskAction {
    ListWorkspace { path: String },
    ModelBackedResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssistantResponse {
    text: String,
    deltas: Vec<String>,
}

impl AssistantResponse {
    fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            deltas: vec![text.clone()],
            text,
        }
    }

    fn from_chat_response(response: ChatResponse) -> Self {
        if !response.tool_calls.is_empty() {
            let tool_summary = summarize_chat_tool_calls(&response.tool_calls);
            let text = if response.text.trim().is_empty() {
                format!(
                    "Coddy received tool calls from the model: {tool_summary}. Automatic model-initiated tool execution is not enabled yet."
                )
            } else {
                format!(
                    "{}\n\nModel requested tools: {tool_summary}. Automatic model-initiated tool execution is not enabled yet.",
                    response.text
                )
            };
            return Self {
                deltas: vec![text.clone()],
                text,
            };
        }
        if looks_like_textual_tool_call(&response.text) {
            let text = "Coddy received a textual tool-call attempt from the model instead of a native structured tool call. The request was not executed for safety. Retry with a narrower prompt, ask for an answer without tools, or switch to a model/provider with reliable OpenAI-compatible tool calling.".to_string();
            return Self {
                deltas: vec![text.clone()],
                text,
            };
        }

        Self {
            text: response.text,
            deltas: response.deltas,
        }
    }

    fn deltas(&self) -> Vec<&String> {
        if self.deltas.is_empty() {
            vec![&self.text]
        } else {
            self.deltas.iter().collect()
        }
    }
}

fn looks_like_textual_tool_call(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    if contains_any(
        &normalized,
        &[
            "<｜dsml｜tool_calls>",
            "<|tool_calls|>",
            "<tool_call",
            "</tool_call",
            "<｜dsml｜invoke",
            "</｜dsml｜invoke",
            "<invoke name=",
            "<read_file",
            "</read_file",
            "<list_files",
            "</list_files",
            "<search_files",
            "</search_files",
            "<apply_edit",
            "</apply_edit",
            "<filesystem.read_file",
            "</filesystem.read_file",
            "<filesystem.list_files",
            "</filesystem.list_files",
            "<filesystem.search_files",
            "</filesystem.search_files",
            "<filesystem.apply_edit",
            "</filesystem.apply_edit",
        ],
    ) {
        return true;
    }

    if normalized.contains("\"tool_calls\"") && normalized.contains("\"arguments\"") {
        return true;
    }

    if contains_any(
        &normalized,
        &[
            "tool call:",
            "tool_call:",
            "tool-call:",
            "tool calls:",
            "requested tool:",
        ],
    ) && contains_any(
        &normalized,
        &[
            "filesystem.read_file",
            "filesystem.list_files",
            "filesystem.search_files",
            "filesystem.apply_edit",
            "shell.run",
            "subagent.",
        ],
    ) {
        return true;
    }

    if normalized.contains("\"name\"")
        && normalized.contains("\"arguments\"")
        && contains_any(
            &normalized,
            &[
                "filesystem.read_file",
                "filesystem.list_files",
                "filesystem.search_files",
                "filesystem.apply_edit",
                "shell.run",
                "subagent.",
            ],
        )
    {
        return true;
    }

    normalized.contains("subprocess.run(")
        && contains_any(
            &normalized,
            &[
                "filesystem.read_file",
                "filesystem.list_files",
                "filesystem.search_files",
                "shell.run",
            ],
        )
}

fn summarize_chat_tool_calls(tool_calls: &[ChatToolCall]) -> String {
    tool_calls
        .iter()
        .map(|call| call.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn normalize_model_initiated_tool_input(
    tool_name: &ToolName,
    mut input: serde_json::Value,
) -> serde_json::Value {
    if matches!(
        tool_name.as_str(),
        LIST_FILES_TOOL | READ_FILE_TOOL | SEARCH_FILES_TOOL
    ) {
        let uses_workspace_root_alias =
            input.get("path").and_then(serde_json::Value::as_str) == Some("/");
        if uses_workspace_root_alias {
            input["path"] = json!(".");
        }
    }
    input
}

fn build_tool_round_limit_response(
    model_text: &str,
    pending_tool_summary: &str,
    last_tool_observations: Option<&str>,
) -> String {
    let mut sections = Vec::new();
    if !model_text.trim().is_empty() {
        sections.push(model_text.trim().to_string());
    }
    sections.push(format!(
        "Coddy reached the bounded tool loop limit after {MAX_MODEL_TOOL_ROUNDS} tool observation rounds. Pending model-requested tools were not executed: {pending_tool_summary}."
    ));
    if let Some(observations) = last_tool_observations
        .map(str::trim)
        .filter(|observations| !observations.is_empty())
    {
        sections.push(format!("Last tool observations:\n{observations}"));
    }
    sections.push(
        "Continue with a narrower prompt or explicitly ask Coddy to proceed with the next safe inspection step.".to_string(),
    );
    sections.join("\n\n")
}

fn build_model_system_prompt(
    context_policy: ContextPolicy,
    session: &ReplSession,
    tool_definitions: &[ToolDefinition],
    tool_use_policy: ToolUsePolicy,
) -> String {
    let mut sections = vec![
        "You are Coddy, a secure AI coding agent.".to_string(),
        [
            "Agent loop:",
            "- Understand the user's goal and restate constraints only when useful.",
            "- Inspect relevant context with runtime tools before making code claims.",
            "- Act with least privilege; never invent tool results.",
            "- Validate changes with focused tests or explain why validation was not run.",
            "- Reply with the result, important evidence, and next concrete step.",
        ]
        .join("\n"),
        [
            "Security rules:",
            "- Use tools only through the Coddy runtime.",
            "- When tools are available, call them through the provider's native structured tool_calls field only.",
            "- Never print textual tool-call markup, XML/DSML tags, JSON tool-call snippets, Python subprocess code, or shell commands as a substitute for a native tool call.",
            "- Model-initiated tools may execute automatically only when low-risk and auto-approved.",
            "- Higher-risk filesystem writes and shell commands require explicit user approval.",
            "- Do not expose secrets, tokens, credentials, or hidden configuration values.",
        ]
        .join("\n"),
        format!("Context policy: {context_policy:?}"),
    ];

    sections.push(format!(
        "Selected chat model: {}/{}",
        session.selected_model.provider, session.selected_model.name
    ));
    if tool_use_policy.max_tool_calls == Some(0) {
        sections.push(format_no_tools_context_boundary(&session.messages));
    } else {
        sections.push(format_workspace_context(&session.workspace_context));
        sections.push(format_recent_session_messages(&session.messages));
        sections.push(format_tool_context(tool_definitions));
    }
    sections.join("\n\n")
}

fn build_tool_followup_system_prompt(base_prompt: &str) -> String {
    let followup = [
        "Tool observation follow-up:",
        "- Treat tool observations as the latest grounded evidence.",
        "- Do not claim files changed unless an edit/apply tool succeeded.",
        "- If observations are incomplete or redacted, state the limitation briefly.",
        "- Keep the final answer concise and include validation status when relevant.",
    ]
    .join("\n");
    format!("{base_prompt}\n\n{followup}")
}

fn context_item_from_tool_output(tool_name: &ToolName, output: &ToolOutput) -> Option<ContextItem> {
    let path = output.metadata.get("path").and_then(|value| value.as_str());
    let item = match tool_name.as_str() {
        LIST_FILES_TOOL => {
            let path = path.unwrap_or(".");
            ContextItem {
                id: context_item_id(tool_name.as_str(), path),
                label: format!("{}: {}", tool_name.as_str(), safe_context_label(path)),
                sensitive: path_looks_sensitive(path),
            }
        }
        READ_FILE_TOOL => {
            let path = path?;
            ContextItem {
                id: context_item_id(tool_name.as_str(), path),
                label: format!("{}: {}", tool_name.as_str(), safe_context_label(path)),
                sensitive: path_looks_sensitive(path),
            }
        }
        SEARCH_FILES_TOOL => {
            let path = path.unwrap_or(".");
            let query = output
                .metadata
                .get("query")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let safe_query = safe_context_label(query);
            ContextItem {
                id: context_item_id(tool_name.as_str(), &format!("{path}:{safe_query}")),
                label: format!(
                    "{}: {} query `{}`",
                    tool_name.as_str(),
                    safe_context_label(path),
                    truncate_context_text(&safe_query, 80)
                ),
                sensitive: path_looks_sensitive(path) || safe_query != query,
            }
        }
        _ => return None,
    };
    Some(item)
}

fn context_item_id(tool_name: &str, source: &str) -> String {
    format!(
        "tool:{tool_name}:{}",
        truncate_context_text(&safe_context_label(source), 160)
    )
}

fn safe_context_label(label: &str) -> String {
    redact_context_text(label.trim())
}

fn path_looks_sensitive(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase();
    normalized == ".env"
        || normalized.ends_with("/.env")
        || normalized.contains(".env.")
        || normalized.contains("secret")
        || normalized.contains("credential")
        || normalized.contains("token")
        || normalized.ends_with(".pem")
        || normalized.ends_with(".p12")
        || normalized.ends_with(".pfx")
        || normalized.ends_with("id_rsa")
        || normalized.ends_with("id_ed25519")
}

fn format_workspace_context(items: &[coddy_core::ContextItem]) -> String {
    if items.is_empty() {
        return "Workspace context: none loaded yet.".to_string();
    }

    let mut recent = items.iter().rev().take(8).collect::<Vec<_>>();
    recent.reverse();

    let mut lines = vec!["Workspace context:".to_string()];
    for item in recent {
        let label = if item.sensitive {
            "[sensitive context item redacted]".to_string()
        } else {
            truncate_context_text(&redact_context_text(&item.label), 160)
        };
        lines.push(format!("- {label}"));
    }
    if items.len() > 8 {
        lines.push(format!(
            "- {} additional context items omitted.",
            items.len() - 8
        ));
    }
    lines.join("\n")
}

fn format_recent_session_messages(messages: &[ReplMessage]) -> String {
    if messages.is_empty() {
        return "Recent session messages: none before this turn.".to_string();
    }

    let mut recent = messages.iter().rev().take(4).collect::<Vec<_>>();
    recent.reverse();
    let mut lines = vec!["Recent session messages before this turn:".to_string()];
    for message in recent {
        let text = truncate_context_text(&redact_context_text(&message.text), 240);
        lines.push(format!("- {}: {text}", message.role));
    }
    lines.join("\n")
}

fn format_no_tools_context_boundary(messages: &[ReplMessage]) -> String {
    let mut sections = vec![
        [
            "No-tools mode:",
            "- The user disabled tool use for this turn.",
            "- Do not claim that you listed, read, searched, edited, or executed project files unless that evidence is explicitly quoted in the current user message.",
            "- Treat prior tool observations as unavailable for new codebase claims; if inspection is required, state that tools are disabled and ask to enable them.",
            "- Answer from the current prompt and clearly label limitations.",
        ]
        .join("\n"),
        "Workspace context: withheld because tools are disabled for this turn.".to_string(),
        "Available runtime tools: disabled for this turn by user request.".to_string(),
    ];

    let mut recent = messages
        .iter()
        .filter(|message| !looks_like_tool_observation_message(&message.text))
        .rev()
        .take(4)
        .collect::<Vec<_>>();
    recent.reverse();

    if recent.is_empty() {
        sections.push(
            "Recent session messages before this no-tools turn: none, or prior tool-observation messages were withheld.".to_string(),
        );
    } else {
        let mut lines = vec!["Recent session messages before this no-tools turn:".to_string()];
        for message in recent {
            let text = truncate_context_text(&redact_context_text(&message.text), 240);
            lines.push(format!("- {}: {text}", message.role));
        }
        sections.push(lines.join("\n"));
    }

    sections.join("\n\n")
}

fn looks_like_tool_observation_message(text: &str) -> bool {
    text.contains("Tool observations:")
        || text.contains("Tool observation follow-up:")
        || text.contains("model-requested tool")
}

fn format_tool_context(tool_definitions: &[ToolDefinition]) -> String {
    if tool_definitions.is_empty() {
        return "Available runtime tools: none registered.".to_string();
    }

    let mut definitions = tool_definitions.iter().collect::<Vec<_>>();
    definitions.sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
    let mut lines = vec![format!(
        "Available runtime tools ({}):",
        tool_definitions.len()
    )];
    for definition in definitions.iter().take(8) {
        lines.push(format!(
            "- {} [{:?}, {:?}]: {}",
            definition.name.as_str(),
            definition.risk_level,
            definition.approval_policy,
            truncate_context_text(&definition.description, 180)
        ));
    }
    if definitions.len() > 8 {
        lines.push(format!(
            "- {} additional tools omitted.",
            definitions.len() - 8
        ));
    }
    lines.join("\n")
}

fn format_subagent_routing_context(output: &ToolOutput) -> String {
    let recommendations = subagent_recommendation_values(output);

    if recommendations.is_empty() {
        return "Subagent routing guidance: no focused recommendation was available for this turn."
            .to_string();
    }

    let mut lines = vec![
        "Subagent routing guidance:".to_string(),
        "- Treat these as planning and validation hints; do not claim a subagent executed unless a runtime event confirms it.".to_string(),
    ];
    for recommendation in recommendations.iter().take(3) {
        let name = recommendation
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let score = recommendation
            .get("score")
            .and_then(|value| value.as_u64())
            .unwrap_or_default();
        let mode = recommendation
            .get("mode")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let matched_signals = recommendation
            .get("matchedSignals")
            .and_then(|value| value.as_array())
            .map(|signals| {
                signals
                    .iter()
                    .filter_map(|signal| signal.as_str())
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        lines.push(format!(
            "- {name} [{mode}] score {score}; matched: {}",
            truncate_context_text(&redact_context_text(&matched_signals), 120)
        ));
    }
    lines.join("\n")
}

fn format_subagent_team_context(output: &ToolOutput) -> String {
    let Some(team) = output
        .metadata
        .get("team")
        .and_then(|value| value.as_object())
    else {
        return "Multiagent team plan: no structured team plan was available.".to_string();
    };
    let metrics = team.get("metrics").and_then(|value| value.as_object());
    let hardness_score = metrics
        .and_then(|metrics| metrics.get("hardnessScore"))
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let average_readiness = metrics
        .and_then(|metrics| metrics.get("averageReadiness"))
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let awaiting_approval = metrics
        .and_then(|metrics| metrics.get("awaitingApproval"))
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let blocked = metrics
        .and_then(|metrics| metrics.get("blocked"))
        .and_then(|value| value.as_u64())
        .unwrap_or_default();

    let members = team
        .get("members")
        .and_then(|value| value.as_array())
        .map(|members| {
            members
                .iter()
                .take(6)
                .filter_map(|member| {
                    let name = member.get("name")?.as_str()?;
                    let mode = member
                        .get("mode")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unknown");
                    let gate = member
                        .get("gateStatus")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unknown");
                    let readiness = member
                        .get("readinessScore")
                        .and_then(|value| value.as_u64())
                        .unwrap_or_default();
                    Some(format!("{name} [{mode}, {gate}, readiness {readiness}]"))
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|members| !members.is_empty())
        .unwrap_or_else(|| "none".to_string());
    let risks = array_string_preview(team.get("risks"), 3);
    let validation = array_string_preview(team.get("validationStrategy"), 3);

    [
        "Multiagent team plan:".to_string(),
        "- This is a measurable orchestration plan only; no subagent execution has started."
            .to_string(),
        format!(
            "- Metrics: hardness score {hardness_score}; average readiness {average_readiness}; awaiting approval {awaiting_approval}; blocked {blocked}."
        ),
        format!("- Members: {members}"),
        format!("- Risks: {risks}"),
        format!("- Validation strategy: {validation}"),
    ]
    .join("\n")
}

fn format_subagent_handoff_context(output: &ToolOutput) -> String {
    let Some(handoff) = output
        .metadata
        .get("handoff")
        .and_then(|value| value.as_object())
    else {
        return "Subagent handoff preview: no structured handoff was available.".to_string();
    };

    let name = handoff
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let mode = handoff
        .get("mode")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let approval_required = handoff
        .get("approvalRequired")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let handoff_prompt = handoff
        .get("handoffPrompt")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let checklist = array_string_preview(handoff.get("validationChecklist"), 3);
    let safety_notes = array_string_preview(handoff.get("safetyNotes"), 2);
    let readiness_score = handoff
        .get("readinessScore")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let readiness_issues = array_string_preview(handoff.get("readinessIssues"), 3);

    [
        "Subagent handoff preview:".to_string(),
        "- Use this handoff as planning guidance only; do not claim the subagent executed."
            .to_string(),
        format!("- Prepared `{name}` in {mode} mode; approval required: {approval_required}."),
        format!("- Readiness score: {readiness_score}; issues: {readiness_issues}."),
        format!(
            "- Handoff prompt: {}",
            truncate_context_text(&redact_context_text(handoff_prompt), 700)
        ),
        format!("- Validation checklist: {checklist}"),
        format!("- Safety notes: {safety_notes}"),
    ]
    .join("\n")
}

fn format_subagent_execution_gate_context(plan: &SubagentExecutionStartPlan) -> String {
    let status = match plan.status {
        SubagentExecutionStartStatus::AwaitingApproval => "awaiting approval",
        SubagentExecutionStartStatus::ReadyToStart => "ready to start",
        SubagentExecutionStartStatus::Blocked => "blocked",
    };
    let lifecycle = if plan.lifecycle_updates.is_empty() {
        "none".to_string()
    } else {
        plan.lifecycle_updates
            .iter()
            .map(|update| format!("{:?}", update.status))
            .collect::<Vec<_>>()
            .join(" -> ")
    };
    let reason = plan.reason.as_deref().unwrap_or("none");

    [
        "Subagent execution gate preview:".to_string(),
        "- This is a readiness decision only; no subagent execution has started.".to_string(),
        format!("- Gate status: {status}."),
        format!("- Lifecycle plan: {lifecycle}."),
        format!(
            "- Reason: {}.",
            truncate_context_text(&redact_context_text(reason), 240)
        ),
    ]
    .join("\n")
}

fn format_subagent_output_contract_context(contract: &SubagentOutputContract) -> String {
    let required_fields = if contract.required_fields.is_empty() {
        "none".to_string()
    } else {
        contract.required_fields.join(", ")
    };
    let extra_fields_policy = if contract.additional_properties_allowed {
        "allowed"
    } else {
        "rejected"
    };

    [
        "Subagent output contract:".to_string(),
        format!("- Role: `{}` in {} mode.", contract.name, contract.mode),
        format!("- Required JSON fields: {required_fields}."),
        format!("- Additional properties: {extra_fields_policy}."),
        "- Free-form prose outside the structured JSON output is not accepted.".to_string(),
    ]
    .join("\n")
}

fn subagent_handoff_prepared_from_output(output: &ToolOutput) -> Option<SubagentHandoffPrepared> {
    let handoff = output.metadata.get("handoff")?.as_object()?;
    let name = handoff.get("name")?.as_str()?.to_string();
    let mode = handoff
        .get("mode")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown")
        .to_string();
    let approval_required = handoff
        .get("approvalRequired")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let allowed_tools = array_string_values(handoff.get("allowedTools"), 16);
    let output_schema = handoff.get("outputSchema");
    let required_output_fields =
        array_string_values(output_schema.and_then(|schema| schema.get("required")), 32);
    let output_additional_properties_allowed = output_schema
        .and_then(|schema| schema.get("additionalProperties"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let timeout_ms = handoff
        .get("timeoutMs")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let max_context_tokens = handoff
        .get("maxContextTokens")
        .and_then(|value| value.as_u64())
        .unwrap_or_default()
        .min(u32::MAX as u64) as u32;
    let validation_checklist = array_string_values(handoff.get("validationChecklist"), 12);
    let safety_notes = array_string_values(handoff.get("safetyNotes"), 12);
    let readiness_score = handoff
        .get("readinessScore")
        .and_then(|value| value.as_u64())
        .unwrap_or_default()
        .min(100) as u8;
    let readiness_issues = array_string_values(handoff.get("readinessIssues"), 12);

    Some(SubagentHandoffPrepared {
        name,
        mode,
        approval_required,
        allowed_tools,
        required_output_fields,
        output_additional_properties_allowed,
        timeout_ms,
        max_context_tokens,
        validation_checklist,
        safety_notes,
        readiness_score,
        readiness_issues,
    })
}

fn subagent_lifecycle_blocked_update(handoff: &SubagentHandoffPrepared) -> SubagentLifecycleUpdate {
    SubagentLifecycleUpdate {
        name: handoff.name.clone(),
        mode: handoff.mode.clone(),
        status: SubagentLifecycleStatus::Blocked,
        readiness_score: handoff.readiness_score,
        reason: Some("execution gate did not return a lifecycle update".to_string()),
    }
}

fn subagent_recommendations_from_output(output: &ToolOutput) -> Vec<SubagentRouteRecommendation> {
    subagent_recommendation_values(output)
        .iter()
        .filter_map(|recommendation| {
            let name = recommendation.get("name")?.as_str()?.to_string();
            let score = recommendation
                .get("score")
                .and_then(|value| value.as_u64())
                .unwrap_or_default()
                .min(100) as u8;
            let mode = recommendation
                .get("mode")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string();
            let matched_signals = recommendation
                .get("matchedSignals")
                .and_then(|value| value.as_array())
                .map(|signals| {
                    signals
                        .iter()
                        .filter_map(|signal| signal.as_str())
                        .take(8)
                        .map(|signal| truncate_context_text(&redact_context_text(signal), 80))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            Some(SubagentRouteRecommendation {
                name,
                score,
                mode,
                matched_signals,
            })
        })
        .collect()
}

fn subagent_recommendation_values(output: &ToolOutput) -> Vec<serde_json::Value> {
    output
        .metadata
        .get("recommendations")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
}

fn array_string_preview(value: Option<&serde_json::Value>, limit: usize) -> String {
    let values = value
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .take(limit)
                .map(|item| truncate_context_text(&redact_context_text(item), 160))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(" | ")
    }
}

fn array_string_values(value: Option<&serde_json::Value>, limit: usize) -> Vec<String> {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .take(limit)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn redact_context_text(text: &str) -> String {
    let markers = ["Bearer ", "sk-or-", "ya29.", "sk-"];
    let mut output = String::with_capacity(text.len());
    let mut index = 0;

    while index < text.len() {
        if let Some(marker) = markers
            .iter()
            .find(|candidate| text[index..].starts_with(**candidate))
        {
            output.push_str(marker);
            output.push_str("[REDACTED]");
            let token_start = index + marker.len();
            index = text[token_start..]
                .char_indices()
                .find_map(|(offset, character)| {
                    (!is_secret_token_character(character)).then_some(token_start + offset)
                })
                .unwrap_or(text.len());
            continue;
        }

        let character = text[index..]
            .chars()
            .next()
            .expect("index remains on a UTF-8 boundary");
        output.push(character);
        index += character.len_utf8();
    }

    output
}

fn is_secret_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
}

fn truncate_context_text(text: &str, max_chars: usize) -> String {
    let mut result = String::new();
    for character in text.chars().take(max_chars) {
        result.push(character);
    }
    if text.chars().count() > max_chars {
        result.push_str("...");
    }
    result
}

fn plan_ask_action(text: &str) -> AskAction {
    let normalized = text.trim();
    let normalized_ascii = normalized.to_ascii_lowercase();

    if normalized_ascii == "ls" {
        return AskAction::ListWorkspace {
            path: ".".to_string(),
        };
    }

    if normalized_ascii.starts_with("ls ") {
        let rest = &normalized[3..];
        return AskAction::ListWorkspace {
            path: normalize_workspace_path(rest),
        };
    }

    let list_triggers = [
        "list files",
        "list workspace",
        "show files",
        "listar arquivos",
        "liste arquivos",
        "mostrar arquivos",
        "mostre arquivos",
    ];
    if list_triggers
        .iter()
        .any(|trigger| normalized_ascii.contains(trigger))
    {
        return AskAction::ListWorkspace {
            path: extract_requested_workspace_path(normalized),
        };
    }

    AskAction::ModelBackedResponse
}

fn extract_requested_workspace_path(text: &str) -> String {
    let ascii_lowercase = text.to_ascii_lowercase();
    for marker in [" in ", " under ", " from ", " at ", " em ", " dentro de "] {
        if let Some(index) = ascii_lowercase.rfind(marker) {
            let start = index + marker.len();
            return normalize_workspace_path(&text[start..]);
        }
    }
    ".".to_string()
}

fn normalize_workspace_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed == "." {
        return ".".to_string();
    }
    let trimmed = trimmed.trim_matches(|character: char| {
        character.is_whitespace()
            || matches!(character, '"' | '\'' | '`' | ',' | ';' | ':' | '?' | '!')
    });
    if trimmed.is_empty() {
        ".".to_string()
    } else {
        trimmed.to_string()
    }
}

fn classify_ask_intent(text: &str, action: &AskAction) -> (ReplIntent, f32) {
    if matches!(action, AskAction::ListWorkspace { .. }) {
        return (ReplIntent::ManageContext, 0.88);
    }

    let normalized = text.to_ascii_lowercase();
    if contains_any(
        &normalized,
        &["debug", "erro", "error", "stack trace", "falha"],
    ) {
        return (ReplIntent::DebugCode, 0.72);
    }
    if contains_any(&normalized, &["test", "teste", "spec", "coverage"]) {
        return (ReplIntent::GenerateTestCases, 0.68);
    }
    if contains_any(
        &normalized,
        &[
            "implement",
            "implemente",
            "fix",
            "corrija",
            "refactor",
            "refatore",
            "revise",
            "continue",
            "commit",
        ],
    ) {
        return (ReplIntent::AgenticCodeChange, 0.7);
    }
    (ReplIntent::AskTechnicalQuestion, 0.55)
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn tool_use_policy_from_text(text: &str) -> ToolUsePolicy {
    let normalized = text.to_ascii_lowercase();
    if contains_any(
        &normalized,
        &[
            "sem chamar ferramentas",
            "sem usar ferramentas",
            "sem ferramentas",
            "sem tools",
            "no tools",
            "without tools",
        ],
    ) {
        return ToolUsePolicy {
            max_tool_calls: Some(0),
        };
    }

    ToolUsePolicy {
        max_tool_calls: parse_requested_tool_limit(&normalized),
    }
}

fn parse_requested_tool_limit(text: &str) -> Option<usize> {
    let markers = [
        "no maximo",
        "no máximo",
        "maximo",
        "máximo",
        "at most",
        "maximum",
    ];
    for marker in markers {
        let Some(index) = text.find(marker) else {
            continue;
        };
        let after_marker = &text[index + marker.len()..];
        if contains_any(
            after_marker,
            &[
                " ferramenta",
                " ferramentas",
                " tool",
                " tools",
                " busca",
                " buscas",
                " leitura",
                " leituras",
                " read",
                " reads",
            ],
        ) {
            if let Some(limit) = first_number(after_marker) {
                return Some(limit);
            }
        }
    }
    None
}

fn first_number(text: &str) -> Option<usize> {
    let mut digits = String::new();
    for character in text.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
        } else if !digits.is_empty() {
            break;
        }
    }
    digits.parse::<usize>().ok()
}

fn should_retry_model_request_error(error: &ChatModelError) -> bool {
    match error {
        ChatModelError::ProviderError { retryable, .. } => *retryable,
        ChatModelError::Transport {
            retryable, message, ..
        } => *retryable && !is_timeout_transport_error(message),
        ChatModelError::InvalidProviderResponse { message, .. } => {
            is_retryable_invalid_provider_response(message)
        }
        _ => false,
    }
}

fn is_timeout_transport_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("timeout")
        || normalized.contains("timed out")
        || normalized.contains("deadline")
}

fn is_retryable_invalid_provider_response(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("did not include assistant content or tool calls")
        || normalized.contains("did not include choices")
        || normalized.contains("finish_reason=error")
}

fn sleep_before_model_retry(attempt: usize) {
    if cfg!(test) {
        return;
    }
    thread::sleep(Duration::from_millis(
        MODEL_RETRY_BASE_DELAY_MS * (attempt as u64 + 1),
    ));
}

fn model_error_message(
    error: &ChatModelError,
    selected_model: &ModelRef,
    tool_count: usize,
) -> String {
    let message = match error {
        ChatModelError::UnselectedModel => format!(
            "Coddy received the request, but no chat model is selected yet. Select a provider/model to enable model-backed coding responses. {tool_count} local tools are available for safe workspace actions."
        ),
        ChatModelError::ProviderUnavailable { provider } => format!(
            "Coddy received the request for {}/{}. The {provider} chat provider is not connected in the Rust runtime yet; the current runtime can synchronize sessions, stream events and execute safe local tools.",
            selected_model.provider, selected_model.name
        ),
        ChatModelError::UnsupportedModel { provider, model } => format!(
            "Coddy received the request for {}/{}. The selected model {provider}/{model} is not supported by the current runtime adapter yet.",
            selected_model.provider, selected_model.name
        ),
        ChatModelError::InvalidRequest(message) => {
            format!("Coddy could not build a valid chat request: {message}")
        }
        ChatModelError::ProviderError {
            provider,
            message,
            retryable,
        } => format!(
            "Coddy could not get a response from {provider} for {}/{}: {message}",
            selected_model.provider, selected_model.name,
        ) + model_recovery_hint(provider, *retryable),
        ChatModelError::Transport {
            provider,
            message,
            retryable,
        } => format!(
            "Coddy could not get a response from {provider} for {}/{}: {message}",
            selected_model.provider, selected_model.name,
        ) + model_recovery_hint(provider, *retryable),
        ChatModelError::InvalidProviderResponse { provider, message } => format!(
            "Coddy could not get a response from {provider} for {}/{}: {message}. Retry the request; if it keeps happening, reduce the prompt/tool output size or switch to another model/provider.",
            selected_model.provider, selected_model.name
        ),
    };
    redact_context_text(&message)
}

fn model_recovery_hint(provider: &str, retryable: bool) -> &'static str {
    if retryable && provider == "openrouter" {
        " This looks recoverable; retry the request, reduce large tool outputs/context, or switch OpenRouter routing/model. If it persists, check OpenRouter credits, rate limits and provider availability."
    } else if retryable {
        " This looks recoverable; retry the request or switch to another model/provider if it persists."
    } else if provider == "openrouter" {
        " Check the OpenRouter API key, account credits, model availability and request compatibility."
    } else {
        ""
    }
}

fn default_workspace_root() -> Option<PathBuf> {
    std::env::var_os("CODDY_WORKSPACE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use coddy_agent::{
        APPLY_EDIT_TOOL, PREVIEW_EDIT_TOOL, READ_FILE_TOOL, SHELL_RUN_TOOL, SUBAGENT_PREPARE_TOOL,
        SUBAGENT_REDUCE_OUTPUTS_TOOL, SUBAGENT_ROUTE_TOOL,
    };
    use coddy_client::CoddyClient;
    use coddy_core::{
        ApprovalPolicy, ModelRef, ModelRole, PermissionReply, ReplEvent, ToolCategory,
        ToolPermission, ToolRiskLevel, ToolStatus,
    };
    use coddy_ipc::{
        ReplCommandJob, ReplConversationHistoryJob, ReplEventStreamJob, ReplEventsJob,
        ReplSessionSnapshotJob, ReplToolsJob,
    };
    use std::{collections::VecDeque, env, fs, path::PathBuf};
    use uuid::Uuid;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path = env::temp_dir().join(format!("coddy-runtime-workspace-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp workspace");
            Self { path }
        }

        fn write(&self, relative_path: &str, content: &str) {
            let path = self.path.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent");
            }
            fs::write(path, content).expect("write fixture");
        }

        fn mkdir(&self, relative_path: &str) {
            fs::create_dir_all(self.path.join(relative_path)).expect("create fixture dir");
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[derive(Debug)]
    struct RecordingChatClient {
        requests: Arc<Mutex<Vec<ChatRequest>>>,
        response: ChatResponse,
    }

    impl RecordingChatClient {
        fn new(response: ChatResponse) -> (Self, Arc<Mutex<Vec<ChatRequest>>>) {
            let requests = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    requests: Arc::clone(&requests),
                    response,
                },
                requests,
            )
        }
    }

    impl ChatModelClient for RecordingChatClient {
        fn complete(&self, request: ChatRequest) -> coddy_agent::ChatModelResult {
            self.requests
                .lock()
                .expect("requests mutex poisoned")
                .push(request);
            Ok(self.response.clone())
        }
    }

    #[derive(Debug)]
    struct QueuedChatClient {
        requests: Arc<Mutex<Vec<ChatRequest>>>,
        responses: Mutex<VecDeque<ChatResponse>>,
    }

    impl QueuedChatClient {
        fn new(responses: Vec<ChatResponse>) -> (Self, Arc<Mutex<Vec<ChatRequest>>>) {
            let requests = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    requests: Arc::clone(&requests),
                    responses: Mutex::new(responses.into()),
                },
                requests,
            )
        }
    }

    impl ChatModelClient for QueuedChatClient {
        fn complete(&self, request: ChatRequest) -> coddy_agent::ChatModelResult {
            self.requests
                .lock()
                .expect("requests mutex poisoned")
                .push(request);
            self.responses
                .lock()
                .expect("responses mutex poisoned")
                .pop_front()
                .ok_or_else(|| {
                    coddy_agent::ChatModelError::InvalidRequest(
                        "missing queued response".to_string(),
                    )
                })
        }
    }

    #[derive(Debug)]
    struct QueuedChatResultClient {
        requests: Arc<Mutex<Vec<ChatRequest>>>,
        results: Mutex<VecDeque<ChatModelResult>>,
    }

    impl QueuedChatResultClient {
        fn new(results: Vec<ChatModelResult>) -> (Self, Arc<Mutex<Vec<ChatRequest>>>) {
            let requests = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    requests: Arc::clone(&requests),
                    results: Mutex::new(results.into()),
                },
                requests,
            )
        }
    }

    impl ChatModelClient for QueuedChatResultClient {
        fn complete(&self, request: ChatRequest) -> coddy_agent::ChatModelResult {
            self.requests
                .lock()
                .expect("requests mutex poisoned")
                .push(request);
            self.results
                .lock()
                .expect("results mutex poisoned")
                .pop_front()
                .unwrap_or_else(|| {
                    Err(coddy_agent::ChatModelError::InvalidRequest(
                        "missing queued result".to_string(),
                    ))
                })
        }
    }

    #[derive(Debug)]
    struct FailingChatClient {
        error: ChatModelError,
    }

    impl ChatModelClient for FailingChatClient {
        fn complete(&self, _request: ChatRequest) -> coddy_agent::ChatModelResult {
            Err(self.error.clone())
        }
    }

    #[test]
    fn conversation_history_persists_redacted_current_session() {
        let workspace = TempWorkspace::new();
        let history_path = workspace.path.join("conversation-history.json");
        let runtime = CoddyRuntime::default().with_conversation_history_path(history_path.clone());
        let request_id = Uuid::new_v4();

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "Analyze this with OPENROUTER_API_KEY=sk-or-secret-token".to_string(),
                context_policy: ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let result = runtime.handle_request(CoddyRequest::ConversationHistory(
            ReplConversationHistoryJob {
                request_id: Uuid::new_v4(),
                limit: None,
            },
        ));

        let CoddyResult::ReplConversationHistory { conversations, .. } = result else {
            panic!("expected conversation history");
        };
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].summary.message_count, 2);
        assert!(conversations[0].summary.title.contains("[redacted]"));

        let raw = fs::read_to_string(history_path).expect("history file");
        assert!(!raw.contains("sk-or-secret-token"));
        assert!(raw.contains("[redacted]"));
    }

    #[test]
    fn new_session_archives_and_resets_conversation_state() {
        let runtime = CoddyRuntime::default();
        let model = ModelRef {
            provider: "openrouter".to_string(),
            name: "deepseek/deepseek-v4-flash".to_string(),
        };

        runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::OpenUi {
                mode: ReplMode::DesktopApp,
            },
            speak: false,
        }));
        runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::SelectModel {
                model: model.clone(),
                role: ModelRole::Chat,
            },
            speak: false,
        }));
        runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::Ask {
                text: "Explain the workspace".to_string(),
                context_policy: ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));
        let previous_snapshot = runtime.snapshot();
        assert_eq!(previous_snapshot.session.messages.len(), 2);

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::NewSession,
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::ActionStatus { .. }));
        let snapshot = runtime.snapshot();
        assert_ne!(snapshot.session.id, previous_snapshot.session.id);
        assert_eq!(snapshot.session.mode, ReplMode::DesktopApp);
        assert_eq!(snapshot.session.selected_model, model);
        assert!(snapshot.session.messages.is_empty());
        assert!(snapshot.session.workspace_context.is_empty());
        assert!(snapshot.session.active_run.is_none());

        let result = runtime.handle_request(CoddyRequest::ConversationHistory(
            ReplConversationHistoryJob {
                request_id: Uuid::new_v4(),
                limit: Some(10),
            },
        ));
        let CoddyResult::ReplConversationHistory { conversations, .. } = result else {
            panic!("expected conversation history");
        };
        assert_eq!(conversations.len(), 1);
        assert_eq!(
            conversations[0].summary.session_id,
            previous_snapshot.session.id
        );
    }

    #[test]
    fn open_conversation_restores_archived_messages_model_and_mode() {
        let runtime = CoddyRuntime::default();
        let model = ModelRef {
            provider: "openrouter".to_string(),
            name: "deepseek/deepseek-v4-flash".to_string(),
        };

        runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::OpenUi {
                mode: ReplMode::DesktopApp,
            },
            speak: false,
        }));
        runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::SelectModel {
                model: model.clone(),
                role: ModelRole::Chat,
            },
            speak: false,
        }));
        runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::Ask {
                text: "Analise esta codebase".to_string(),
                context_policy: ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));
        let archived_snapshot = runtime.snapshot();

        runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::NewSession,
            speak: false,
        }));

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::OpenConversation {
                session_id: archived_snapshot.session.id,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::ActionStatus { .. }));
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.session.id, archived_snapshot.session.id);
        assert_eq!(snapshot.session.mode, ReplMode::DesktopApp);
        assert_eq!(snapshot.session.selected_model, model);
        assert_eq!(snapshot.session.messages.len(), 2);
        assert_eq!(snapshot.session.messages[0].text, "Analise esta codebase");
        assert_eq!(snapshot.session.status, coddy_core::SessionStatus::Idle);
    }

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
                "subagent.list",
                "subagent.prepare",
                "subagent.reduce_outputs",
                "subagent.route",
                "subagent.team_plan",
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

        let subagent_list = tools
            .iter()
            .find(|tool| tool.name == "subagent.list")
            .expect("subagent list tool");
        assert_eq!(subagent_list.category, ToolCategory::Subagent);
        assert_eq!(subagent_list.risk_level, ToolRiskLevel::Low);
        assert_eq!(
            subagent_list.permissions,
            vec![ToolPermission::DelegateSubagent]
        );
        assert_eq!(subagent_list.approval_policy, ApprovalPolicy::AutoApprove);

        let subagent_prepare = tools
            .iter()
            .find(|tool| tool.name == "subagent.prepare")
            .expect("subagent prepare tool");
        assert_eq!(subagent_prepare.category, ToolCategory::Subagent);
        assert_eq!(subagent_prepare.risk_level, ToolRiskLevel::Low);
        assert_eq!(
            subagent_prepare.permissions,
            vec![ToolPermission::DelegateSubagent]
        );
        assert_eq!(
            subagent_prepare.approval_policy,
            ApprovalPolicy::AutoApprove
        );

        let subagent_route = tools
            .iter()
            .find(|tool| tool.name == "subagent.route")
            .expect("subagent route tool");
        assert_eq!(subagent_route.category, ToolCategory::Subagent);
        assert_eq!(subagent_route.risk_level, ToolRiskLevel::Low);
        assert_eq!(
            subagent_route.permissions,
            vec![ToolPermission::DelegateSubagent]
        );
        assert_eq!(subagent_route.approval_policy, ApprovalPolicy::AutoApprove);

        let subagent_team_plan = tools
            .iter()
            .find(|tool| tool.name == "subagent.team_plan")
            .expect("subagent team plan tool");
        assert_eq!(subagent_team_plan.category, ToolCategory::Subagent);
        assert_eq!(subagent_team_plan.risk_level, ToolRiskLevel::Low);
        assert_eq!(
            subagent_team_plan.permissions,
            vec![ToolPermission::DelegateSubagent]
        );
        assert_eq!(
            subagent_team_plan.approval_policy,
            ApprovalPolicy::AutoApprove
        );

        let subagent_reduce_outputs = tools
            .iter()
            .find(|tool| tool.name == "subagent.reduce_outputs")
            .expect("subagent reduce outputs tool");
        assert_eq!(subagent_reduce_outputs.category, ToolCategory::Subagent);
        assert_eq!(subagent_reduce_outputs.risk_level, ToolRiskLevel::Low);
        assert_eq!(
            subagent_reduce_outputs.permissions,
            vec![ToolPermission::DelegateSubagent]
        );
        assert_eq!(
            subagent_reduce_outputs.approval_policy,
            ApprovalPolicy::AutoApprove
        );
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
        assert!(message.contains("streaming runtime connection"));
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
                model_credential: None,
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
        assert!(text.contains("no chat model is selected yet"));
        assert!(spoken);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].text, "explain this module");
        assert_eq!(messages[1].role, "assistant");
        assert!(messages[1].text.contains("model-backed coding responses"));
        assert_eq!(snapshot.session.active_run, None);
        assert!(events
            .iter()
            .any(|event| matches!(event.event, ReplEvent::RunStarted { .. })));
        assert!(events.iter().any(|event| matches!(
            event.event,
            ReplEvent::IntentDetected {
                intent: coddy_core::ReplIntent::AskTechnicalQuestion,
                ..
            }
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event.event, ReplEvent::RunCompleted { .. })));
    }

    #[test]
    fn ask_command_streams_assistant_text_before_final_message() {
        let request_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "explain streaming".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));

        let events = runtime.events_after(1).0;
        let delta_index = events
            .iter()
            .position(|event| {
                matches!(
                    &event.event,
                    ReplEvent::TokenDelta { text, .. }
                        if text.contains("model-backed coding responses")
                )
            })
            .expect("assistant token delta");
        let assistant_message_index = events
            .iter()
            .position(|event| {
                matches!(
                    &event.event,
                    ReplEvent::MessageAppended { message }
                        if message.role == "assistant"
                            && message.text.contains("model-backed coding responses")
                )
            })
            .expect("final assistant message");

        assert!(delta_index < assistant_message_index);
    }

    #[test]
    fn ask_command_uses_injected_chat_client_for_selected_model() {
        let request_id = Uuid::new_v4();
        let (chat_client, requests) = RecordingChatClient::new(
            ChatResponse::from_deltas(vec!["hello".to_string(), " world".to_string()])
                .expect("chat response"),
        );
        let runtime =
            CoddyRuntime::with_chat_client(AgentToolRegistry::default(), Arc::new(chat_client));
        let model = ModelRef {
            provider: "openai".to_string(),
            name: "gpt-test".to_string(),
        };
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: model.clone(),
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "explain this module".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        let events = runtime.events_after(2).0;
        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|event| match &event.event {
                ReplEvent::TokenDelta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(text, "hello world");
        assert_eq!(captured_requests.len(), 1);
        assert_eq!(captured_requests[0].model, model);
        assert_eq!(captured_requests[0].messages.len(), 2);
        assert_eq!(
            captured_requests[0].messages[1].content,
            "explain this module"
        );
        assert!(captured_requests[0]
            .tools
            .iter()
            .any(|tool| tool.name == LIST_FILES_TOOL));
        assert_eq!(deltas, vec!["hello", " world"]);
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::MessageAppended { message }
                if message.role == "assistant" && message.text == "hello world"
        )));
    }

    #[test]
    fn ask_command_builds_contextual_agent_prompt_for_model() {
        let request_id = Uuid::new_v4();
        let (chat_client, requests) =
            RecordingChatClient::new(ChatResponse::from_text("context accepted"));
        let runtime =
            CoddyRuntime::with_chat_client(AgentToolRegistry::default(), Arc::new(chat_client));
        let model = ModelRef {
            provider: "openai".to_string(),
            name: "gpt-test".to_string(),
        };
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: model.clone(),
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );
        runtime.publish_event(
            ReplEvent::MessageAppended {
                message: ReplMessage {
                    id: Uuid::new_v4(),
                    role: "user".to_string(),
                    text: "Use this prior note but hide sk-secret-token".to_string(),
                },
            },
            None,
            1_775_000_000_110,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "continue the implementation".to_string(),
                context_policy: coddy_core::ContextPolicy::ScreenAndWorkspace,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        let system_prompt = &captured_requests[0].messages[0].content;

        assert!(system_prompt.contains("Agent loop:"));
        assert!(system_prompt.contains("Security rules:"));
        assert!(system_prompt.contains("native structured tool_calls field only"));
        assert!(system_prompt.contains("Never print textual tool-call markup"));
        assert!(system_prompt.contains("Context policy: ScreenAndWorkspace"));
        assert!(system_prompt.contains("Recent session messages before this turn:"));
        assert!(system_prompt.contains("sk-[REDACTED]"));
        assert!(!system_prompt.contains("sk-secret-token"));
        assert!(system_prompt.contains("Available runtime tools"));
        assert!(system_prompt.contains(LIST_FILES_TOOL));
        assert!(system_prompt.contains("filesystem.preview_edit"));
        assert_eq!(
            captured_requests[0].messages[1].content,
            "continue the implementation"
        );
    }

    #[test]
    fn assistant_response_blocks_textual_tool_call_markup() {
        let response = ChatResponse::from_text(
            r#"Let me call<｜DSML｜tool_calls>
<｜DSML｜invoke name="filesystem.read_file">
<｜DSML｜parameter name="file_path">README.md</｜DSML｜parameter>
</｜DSML｜invoke>
</｜DSML｜tool_calls>"#,
        );

        let response = AssistantResponse::from_chat_response(response);

        assert!(response
            .text
            .contains("textual tool-call attempt from the model"));
        assert!(response.text.contains("not executed for safety"));
        assert!(!response.text.contains("README.md"));
    }

    #[test]
    fn assistant_response_blocks_simple_xml_tool_markup() {
        let response = ChatResponse::from_text(
            r#"Search results show relevant files.

<read_file>
<path>apps/coddy-electron/src/domain/services/toolSafety.ts</path>
</read_file>"#,
        );

        let response = AssistantResponse::from_chat_response(response);

        assert!(response
            .text
            .contains("textual tool-call attempt from the model"));
        assert!(response.text.contains("not executed for safety"));
        assert!(!response.text.contains("toolSafety.ts"));
    }

    #[test]
    fn assistant_response_blocks_markdown_pseudo_tool_calls() {
        let response = ChatResponse::from_text(
            r#"### Search 1

**Tool call: `filesystem.search_files`**
- Query: `Critical`
- Paths: `crates/`"#,
        );

        let response = AssistantResponse::from_chat_response(response);

        assert!(response
            .text
            .contains("textual tool-call attempt from the model"));
        assert!(response.text.contains("not executed for safety"));
        assert!(!response.text.contains("Critical"));
    }

    #[test]
    fn assistant_response_allows_plain_tool_explanations() {
        let response = ChatResponse::from_text(
            "The filesystem.read_file tool has parameters like path and max_bytes, but this is only documentation.",
        );

        let response = AssistantResponse::from_chat_response(response);

        assert!(response.text.contains("filesystem.read_file tool"));
        assert!(!response.text.contains("textual tool-call attempt"));
    }

    #[test]
    fn ask_command_injects_subagent_routing_guidance_for_model() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        let (chat_client, requests) =
            RecordingChatClient::new(ChatResponse::from_text("routing accepted"));
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "run eval baseline score regression harness for this change".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        let system_prompt = &captured_requests[0].messages[0].content;
        let events = runtime.events_after(0).0;

        assert!(system_prompt.contains("Subagent routing guidance:"));
        assert!(system_prompt.contains("eval-runner [evaluation]"));
        assert!(system_prompt.contains("matched: eval"));
        assert!(system_prompt.contains("do not claim a subagent executed"));
        assert!(system_prompt.contains("Subagent handoff preview:"));
        assert!(system_prompt.contains("Prepared `eval-runner` in evaluation mode"));
        assert!(system_prompt.contains("Readiness score: 100"));
        assert!(system_prompt.contains("Validation checklist:"));
        assert!(system_prompt.contains("Subagent execution gate preview:"));
        assert!(system_prompt.contains("no subagent execution has started"));
        assert!(system_prompt.contains("Gate status: awaiting approval"));
        assert!(system_prompt.contains("Subagent output contract:"));
        assert!(system_prompt.contains(
            "Required JSON fields: score, passed, failedChecks, metrics, recommendations."
        ));
        assert!(system_prompt.contains("Additional properties: rejected."));
        assert!(system_prompt
            .contains("Free-form prose outside the structured JSON output is not accepted."));
        assert!(system_prompt.contains("Multiagent team plan:"));
        assert!(system_prompt.contains("hardness score"));
        assert!(system_prompt.contains("eval-runner [evaluation"));
        assert!(system_prompt.contains("no subagent execution has started"));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolStarted { name } if name == SUBAGENT_ROUTE_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolCompleted { name, .. } if name == SUBAGENT_ROUTE_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolStarted { name } if name == SUBAGENT_PREPARE_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolCompleted { name, .. } if name == SUBAGENT_PREPARE_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolStarted { name } if name == SUBAGENT_TEAM_PLAN_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolCompleted { name, .. } if name == SUBAGENT_TEAM_PLAN_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::SubagentRouted { recommendations }
                if recommendations
                    .first()
                    .is_some_and(|recommendation| recommendation.name == "eval-runner")
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::SubagentHandoffPrepared { handoff }
                if handoff.name == "eval-runner"
                    && handoff.mode == "evaluation"
                    && handoff.readiness_score == 100
                    && handoff.readiness_issues.is_empty()
                    && handoff.allowed_tools.iter().any(|tool| tool == "shell.run")
                    && !handoff.output_additional_properties_allowed
                    && handoff.required_output_fields == [
                        "score".to_string(),
                        "passed".to_string(),
                        "failedChecks".to_string(),
                        "metrics".to_string(),
                        "recommendations".to_string()
                    ]
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::SubagentLifecycleUpdated { update }
                if update.name == "eval-runner"
                    && update.mode == "evaluation"
                    && update.status == SubagentLifecycleStatus::Prepared
                    && update.readiness_score == 100
                    && update.reason.is_none()
        )));
        assert!(!events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::SubagentLifecycleUpdated { update }
                if update.status == SubagentLifecycleStatus::Running
        )));
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.session.subagent_activity.len(), 1);
        assert_eq!(snapshot.session.subagent_activity[0].name, "eval-runner");
        assert_eq!(
            snapshot.session.subagent_activity[0].status,
            SubagentLifecycleStatus::Prepared
        );
        assert_eq!(snapshot.session.subagent_activity[0].readiness_score, 100);
        assert_eq!(
            snapshot.session.subagent_activity[0].required_output_fields,
            vec![
                "score".to_string(),
                "passed".to_string(),
                "failedChecks".to_string(),
                "metrics".to_string(),
                "recommendations".to_string()
            ]
        );
        assert!(!snapshot.session.subagent_activity[0].output_additional_properties_allowed);
    }

    #[test]
    fn subagent_handoff_event_preserves_full_values_while_prompt_preview_is_truncated() {
        let long_issue = "x".repeat(220);
        let output = ToolOutput {
            text: "prepared".to_string(),
            metadata: serde_json::json!({
                "handoff": {
                    "name": "eval-runner",
                    "mode": "evaluation",
                    "approvalRequired": true,
                    "allowedTools": ["shell.run"],
                    "timeoutMs": 60000,
                    "maxContextTokens": 8000,
                    "validationChecklist": [long_issue],
                    "safetyNotes": ["Do not expose secrets."],
                    "readinessScore": 80,
                    "readinessIssues": [long_issue],
                    "outputSchema": {}
                }
            }),
            truncated: false,
        };

        let handoff = subagent_handoff_prepared_from_output(&output).expect("handoff event");
        assert_eq!(handoff.validation_checklist[0].len(), 220);
        assert_eq!(handoff.readiness_issues[0].len(), 220);
        assert!(handoff.required_output_fields.is_empty());
        assert!(handoff.output_additional_properties_allowed);

        let preview = format_subagent_handoff_context(&output);
        assert!(preview.contains("Readiness score: 80"));
        assert!(!preview.contains(&"x".repeat(220)));
    }

    #[test]
    fn subagent_lifecycle_blocks_handoffs_below_readiness_threshold() {
        let handoff = SubagentHandoffPrepared {
            name: "coder".to_string(),
            mode: "workspace-write".to_string(),
            approval_required: true,
            allowed_tools: vec!["filesystem.apply_edit".to_string()],
            required_output_fields: vec!["changedFiles".to_string(), "summary".to_string()],
            output_additional_properties_allowed: false,
            timeout_ms: 60_000,
            max_context_tokens: 8_000,
            validation_checklist: vec!["Preview edits before applying.".to_string()],
            safety_notes: vec!["Do not expose secrets.".to_string()],
            readiness_score: 80,
            readiness_issues: vec![
                "workspace-write handoff must include preview edit capability".to_string(),
            ],
        };

        let execution_handoff = SubagentExecutionHandoff::from(&handoff);
        let plan = SubagentExecutionGate.plan_start_for(&execution_handoff, true);
        let update = plan.lifecycle_updates.first().expect("blocked update");

        assert_eq!(update.name, "coder");
        assert_eq!(update.status, SubagentLifecycleStatus::Blocked);
        assert_eq!(update.readiness_score, 80);
        assert_eq!(
            update.reason.as_deref(),
            Some(
                "readiness score 80 does not meet execution threshold 100; workspace-write handoff must include preview edit capability"
            )
        );
    }

    #[test]
    fn ask_command_prioritizes_recent_workspace_context_in_model_prompt() {
        let request_id = Uuid::new_v4();
        let (chat_client, requests) =
            RecordingChatClient::new(ChatResponse::from_text("recent context accepted"));
        let runtime =
            CoddyRuntime::with_chat_client(AgentToolRegistry::default(), Arc::new(chat_client));
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );
        for index in 1..=10 {
            runtime.publish_event(
                ReplEvent::ContextItemAdded {
                    item: coddy_core::ContextItem {
                        id: format!("context-{index}"),
                        label: format!("src/context-{index}.rs"),
                        sensitive: false,
                    },
                },
                None,
                1_775_000_000_100 + index,
            );
        }

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "use the latest context".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        let system_prompt = &captured_requests[0].messages[0].content;

        assert!(system_prompt.contains("src/context-10.rs"));
        assert!(system_prompt.contains("src/context-3.rs"));
        assert!(!system_prompt.contains("src/context-1.rs"));
        assert!(!system_prompt.contains("src/context-2.rs"));
        assert!(system_prompt.contains("2 additional context items omitted."));
    }

    #[test]
    fn context_prompt_redaction_preserves_secret_markers_without_values() {
        let redacted = redact_context_text(
            "openai sk-secret-token openrouter sk-or-router-token oauth ya29.google-token auth Bearer abc.DEF_123",
        );

        assert!(redacted.contains("sk-[REDACTED]"));
        assert!(redacted.contains("sk-or-[REDACTED]"));
        assert!(redacted.contains("ya29.[REDACTED]"));
        assert!(redacted.contains("Bearer [REDACTED]"));
        assert!(!redacted.contains("secret-token"));
        assert!(!redacted.contains("router-token"));
        assert!(!redacted.contains("google-token"));
        assert!(!redacted.contains("abc.DEF_123"));
    }

    #[test]
    fn model_error_message_redacts_provider_secret_tokens() {
        let model = ModelRef {
            provider: "openrouter".to_string(),
            name: "deepseek/deepseek-v4-flash".to_string(),
        };
        let message = model_error_message(
            &ChatModelError::ProviderError {
                provider: "openrouter".to_string(),
                message: "upstream included sk-or-router-token in error".to_string(),
                retryable: false,
            },
            &model,
            11,
        );

        assert!(message.contains("sk-or-[REDACTED]"));
        assert!(!message.contains("router-token"));
    }

    #[test]
    fn ask_command_forwards_ephemeral_model_credential_to_chat_client() {
        let request_id = Uuid::new_v4();
        let (chat_client, requests) =
            RecordingChatClient::new(ChatResponse::from_text("credential accepted"));
        let runtime =
            CoddyRuntime::with_chat_client(AgentToolRegistry::default(), Arc::new(chat_client));
        let model = ModelRef {
            provider: "openai".to_string(),
            name: "gpt-test".to_string(),
        };
        let credential = coddy_core::ModelCredential {
            provider: "openai".to_string(),
            token: "sk-secret-token".to_string(),
            endpoint: Some("https://api.openai.com/v1".to_string()),
            metadata: Default::default(),
        };
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model,
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "explain this module".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: Some(credential.clone()),
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        assert_eq!(captured_requests.len(), 1);
        assert_eq!(captured_requests[0].model_credential, Some(credential));
    }

    #[test]
    fn ask_command_exposes_only_model_safe_tools_to_chat_client() {
        let request_id = Uuid::new_v4();
        let (chat_client, requests) =
            RecordingChatClient::new(ChatResponse::from_text("safe catalog accepted"));
        let runtime =
            CoddyRuntime::with_chat_client(AgentToolRegistry::default(), Arc::new(chat_client));
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "inspect the workspace".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        let tool_names = captured_requests[0]
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();

        assert!(tool_names.contains(&LIST_FILES_TOOL));
        assert!(tool_names.contains(&READ_FILE_TOOL));
        assert!(tool_names.contains(&SEARCH_FILES_TOOL));
        assert!(tool_names.contains(&PREVIEW_EDIT_TOOL));
        assert!(!tool_names.contains(&APPLY_EDIT_TOOL));
        assert!(!tool_names.contains(&SHELL_RUN_TOOL));
    }

    #[test]
    fn ask_command_does_not_auto_execute_unsafe_model_tool_calls() {
        let request_id = Uuid::new_v4();
        let (chat_client, _requests) = RecordingChatClient::new(ChatResponse {
            text: String::new(),
            deltas: Vec::new(),
            finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
            tool_calls: vec![ChatToolCall {
                id: Some("call-1".to_string()),
                name: "shell.run".to_string(),
                arguments: json!({ "command": "ls" }),
            }],
        });
        let runtime =
            CoddyRuntime::with_chat_client(AgentToolRegistry::default(), Arc::new(chat_client));
        let model = ModelRef {
            provider: "openai".to_string(),
            name: "gpt-test".to_string(),
        };
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model,
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "inspect the workspace".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let events = runtime.events_after(2).0;

        assert!(text.contains("shell.run"));
        assert!(text.contains("was not executed"));
        assert!(!events.iter().any(
            |event| matches!(&event.event, ReplEvent::ToolStarted { name } if name == "shell.run")
        ));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolStarted { name } if name == SUBAGENT_ROUTE_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::MessageAppended { message }
                if message.role == "assistant" && message.text.contains("shell.run")
        )));
    }

    #[test]
    fn ask_command_executes_safe_model_tool_calls_through_agent_runtime() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("src/main.rs", "fn main() {}\n");
        let (chat_client, requests) = QueuedChatClient::new(vec![
            ChatResponse {
                text: "I will inspect the workspace.".to_string(),
                deltas: vec!["I will inspect the workspace.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-1".to_string()),
                    name: LIST_FILES_TOOL.to_string(),
                    arguments: json!({ "path": ".", "max_entries": 20 }),
                }],
            },
            ChatResponse::from_text("The workspace contains a Rust source directory."),
        ]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        let model = ModelRef {
            provider: "openai".to_string(),
            name: "gpt-test".to_string(),
        };
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model,
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "inspect the workspace".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let events = runtime.events_after(2).0;
        let captured_requests = requests.lock().expect("requests mutex poisoned");

        assert_eq!(text, "The workspace contains a Rust source directory.");
        assert_eq!(captured_requests.len(), 2);
        assert!(captured_requests[1]
            .tools
            .iter()
            .any(|tool| tool.name == LIST_FILES_TOOL));
        let followup_system_prompt = &captured_requests[1].messages[0].content;
        assert!(followup_system_prompt.contains("Agent loop:"));
        assert!(followup_system_prompt.contains("Security rules:"));
        assert!(followup_system_prompt.contains("Context policy: WorkspaceOnly"));
        assert!(captured_requests[1].messages.iter().any(|message| {
            message.role == coddy_agent::ChatMessageRole::Tool
                && message.content.contains("filesystem.list_files")
                && message.content.contains("src")
        }));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolStarted { name } if name == LIST_FILES_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolCompleted { name, .. } if name == LIST_FILES_TOOL
        )));
    }

    #[test]
    fn ask_command_executes_provider_safe_tool_aliases_through_agent_runtime() {
        for (alias, expected_tool, arguments) in [
            (
                "coddy_tool__filesystem__dot__list_files",
                LIST_FILES_TOOL,
                json!({ "path": ".", "max_entries": 20 }),
            ),
            (
                "filesystem__dot__list_files",
                LIST_FILES_TOOL,
                json!({ "path": ".", "max_entries": 20 }),
            ),
            (
                "filesystem::list_files",
                LIST_FILES_TOOL,
                json!({ "path": ".", "max_entries": 20 }),
            ),
            (
                "filesystem._list_files",
                LIST_FILES_TOOL,
                json!({ "path": ".", "max_entries": 20 }),
            ),
            (
                "filesystem_list_files",
                LIST_FILES_TOOL,
                json!({ "path": ".", "max_entries": 20 }),
            ),
            (
                "filesystem__dot__read_file",
                READ_FILE_TOOL,
                json!({ "path": "src/main.rs", "max_bytes": 200 }),
            ),
            (
                "coddy_tool__filesystem__dot__read_file",
                READ_FILE_TOOL,
                json!({ "path": "src/main.rs", "max_bytes": 200 }),
            ),
            (
                "filesystem_search_files",
                SEARCH_FILES_TOOL,
                json!({ "path": ".", "query": "fn main", "max_matches": 5 }),
            ),
        ] {
            let request_id = Uuid::new_v4();
            let workspace = TempWorkspace::new();
            workspace.write("src/main.rs", "fn main() {}\n");
            let (chat_client, _requests) = QueuedChatClient::new(vec![
                ChatResponse {
                    text: "I will inspect the workspace.".to_string(),
                    deltas: vec!["I will inspect the workspace.".to_string()],
                    finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                    tool_calls: vec![ChatToolCall {
                        id: Some("call-1".to_string()),
                        name: alias.to_string(),
                        arguments,
                    }],
                },
                ChatResponse::from_text(format!("Final answer for {alias}.")),
            ]);
            let runtime = CoddyRuntime::with_workspace_and_chat_client(
                AgentToolRegistry::default(),
                &workspace.path,
                Arc::new(chat_client),
            )
            .expect("runtime");
            runtime.publish_event(
                ReplEvent::ModelSelected {
                    model: ModelRef {
                        provider: "openai".to_string(),
                        name: "gpt-test".to_string(),
                    },
                    role: ModelRole::Chat,
                },
                None,
                1_775_000_000_100,
            );

            let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
                request_id,
                command: ReplCommand::Ask {
                    text: "inspect the workspace".to_string(),
                    context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                    model_credential: None,
                },
                speak: false,
            }));

            let CoddyResult::Text { text, .. } = result else {
                panic!("expected text result");
            };
            let events = runtime.events_after(0).0;

            assert_eq!(text, format!("Final answer for {alias}."));
            assert!(events.iter().any(|event| matches!(
                &event.event,
                ReplEvent::ToolStarted { name } if name == expected_tool
            )));
            assert!(!events.iter().any(|event| matches!(
                &event.event,
                ReplEvent::MessageAppended { message }
                    if message.text.contains("not registered in the local tool registry")
            )));
        }
    }

    #[test]
    fn model_initiated_filesystem_root_alias_maps_to_workspace_root() {
        let list_files = ToolName::new(LIST_FILES_TOOL).expect("tool name");
        let search_files = ToolName::new(SEARCH_FILES_TOOL).expect("tool name");
        let subagent_route = ToolName::new(SUBAGENT_ROUTE_TOOL).expect("tool name");

        assert_eq!(
            normalize_model_initiated_tool_input(&list_files, json!({ "path": "/" }))["path"],
            "."
        );
        assert_eq!(
            normalize_model_initiated_tool_input(&search_files, json!({ "path": "/" }))["path"],
            "."
        );
        assert_eq!(
            normalize_model_initiated_tool_input(&list_files, json!({ "path": "/tmp" }))["path"],
            "/tmp"
        );
        assert_eq!(
            normalize_model_initiated_tool_input(&subagent_route, json!({ "path": "/" }))["path"],
            "/"
        );
    }

    #[test]
    fn runtime_uses_shared_tool_alias_decoder() {
        assert_eq!(
            decode_provider_safe_tool_name("filesystem_search_files"),
            SEARCH_FILES_TOOL
        );
        assert_eq!(
            decode_provider_safe_tool_name("subagent_team_plan"),
            SUBAGENT_TEAM_PLAN_TOOL
        );
        assert_eq!(
            decode_provider_safe_tool_name("filesystem._list_files"),
            LIST_FILES_TOOL
        );
        assert_eq!(
            decode_provider_safe_tool_name("unknown_tool"),
            "unknown_tool"
        );
    }

    #[test]
    fn ask_command_respects_user_requested_no_tools_budget() {
        let request_id = Uuid::new_v4();
        let (chat_client, requests) =
            RecordingChatClient::new(ChatResponse::from_text("Answer without tools."));
        let runtime =
            CoddyRuntime::with_chat_client(AgentToolRegistry::default(), Arc::new(chat_client));
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );
        runtime.publish_event(
            ReplEvent::MessageAppended {
                message: ReplMessage {
                    id: Uuid::new_v4(),
                    role: "assistant".to_string(),
                    text: "Tool observations:\n- `filesystem.list_files` succeeded:\nsrc/main.rs"
                        .to_string(),
                },
            },
            None,
            1_775_000_000_110,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "Sem chamar ferramentas nesta resposta, explique a arquitetura.".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");

        assert_eq!(text, "Answer without tools.");
        assert_eq!(captured_requests.len(), 1);
        assert!(captured_requests[0].tools.is_empty());
        assert!(captured_requests[0].messages[0]
            .content
            .contains("No-tools mode:"));
        assert!(!captured_requests[0].messages[0]
            .content
            .contains("src/main.rs"));
        assert!(!captured_requests[0].messages[0]
            .content
            .contains("Subagent routing guidance:"));
        let events = runtime.events_after(0).0;
        assert!(!events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolStarted { name }
                if name == SUBAGENT_ROUTE_TOOL
                    || name == SUBAGENT_PREPARE_TOOL
                    || name == SUBAGENT_TEAM_PLAN_TOOL
        )));
    }

    #[test]
    fn ask_command_disables_followup_tools_after_user_requested_budget_is_spent() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("src/main.rs", "fn main() {}\n");
        let (chat_client, requests) = QueuedChatClient::new(vec![
            ChatResponse {
                text: "I will inspect once.".to_string(),
                deltas: vec!["I will inspect once.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![
                    ChatToolCall {
                        id: Some("call-1".to_string()),
                        name: LIST_FILES_TOOL.to_string(),
                        arguments: json!({ "path": ".", "max_entries": 20 }),
                    },
                    ChatToolCall {
                        id: Some("call-2".to_string()),
                        name: READ_FILE_TOOL.to_string(),
                        arguments: json!({ "path": "src/main.rs", "max_bytes": 100 }),
                    },
                ],
            },
            ChatResponse::from_text("Final answer after one tool."),
        ]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "Use no maximo 1 ferramenta read-only e responda.".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");

        assert_eq!(text, "Final answer after one tool.");
        assert_eq!(captured_requests.len(), 2);
        assert!(captured_requests[0]
            .tools
            .iter()
            .any(|tool| tool.name == LIST_FILES_TOOL));
        assert!(captured_requests[1].tools.is_empty());
        let events = runtime.events_after(0).0;
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(&event.event, ReplEvent::ToolStarted { name } if name == LIST_FILES_TOOL || name == READ_FILE_TOOL || name == SUBAGENT_ROUTE_TOOL || name == SUBAGENT_PREPARE_TOOL || name == SUBAGENT_TEAM_PLAN_TOOL))
                .count(),
            1
        );
        assert!(captured_requests[1].messages.iter().any(|message| {
            message.role == coddy_agent::ChatMessageRole::Tool
                && message
                    .content
                    .contains("reached the user-requested tool budget")
        }));
    }

    #[test]
    fn ask_command_surfaces_tool_followup_model_errors() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("src/main.rs", "fn main() {}\n");
        let (chat_client, requests) = QueuedChatClient::new(vec![ChatResponse {
            text: "I will inspect the workspace.".to_string(),
            deltas: vec!["I will inspect the workspace.".to_string()],
            finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
            tool_calls: vec![ChatToolCall {
                id: Some("call-1".to_string()),
                name: LIST_FILES_TOOL.to_string(),
                arguments: json!({ "path": ".", "max_entries": 20 }),
            }],
        }]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "inspect the workspace".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");

        assert_eq!(captured_requests.len(), 2);
        assert!(text.contains("Tool observations:"));
        assert!(text.contains("filesystem.list_files"));
        assert!(text.contains("could not get a follow-up response after tool observations"));
        assert!(text.contains("missing queued response"));
    }

    #[test]
    fn ask_command_retries_recoverable_tool_followup_model_errors() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("src/main.rs", "fn main() {}\n");
        let (chat_client, requests) = QueuedChatResultClient::new(vec![
            Ok(ChatResponse {
                text: "I will inspect the workspace.".to_string(),
                deltas: vec!["I will inspect the workspace.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-1".to_string()),
                    name: LIST_FILES_TOOL.to_string(),
                    arguments: json!({ "path": ".", "max_entries": 20 }),
                }],
            }),
            Err(ChatModelError::ProviderError {
                provider: "openrouter".to_string(),
                message: "Provider returned error (HTTP 502; upstream provider: DeepSeek)"
                    .to_string(),
                retryable: true,
            }),
            Ok(ChatResponse::from_text("Recovered after retry.")),
        ]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openrouter".to_string(),
                    name: "deepseek/deepseek-v4-flash".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "inspect the workspace".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: Some(ModelCredential {
                    provider: "openrouter".to_string(),
                    token: "sk-or-test-token".to_string(),
                    endpoint: None,
                    metadata: Default::default(),
                }),
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");

        assert_eq!(text, "Recovered after retry.");
        assert_eq!(captured_requests.len(), 3);
        assert!(captured_requests[1].messages.iter().any(|message| {
            message.role == coddy_agent::ChatMessageRole::Tool
                && message.content.contains("filesystem.list_files")
        }));
        assert_eq!(captured_requests[1], captured_requests[2]);
    }

    #[test]
    fn ask_command_does_not_retry_transport_timeouts_past_client_budget() {
        let request_id = Uuid::new_v4();
        let (chat_client, requests) = QueuedChatResultClient::new(vec![
            Err(ChatModelError::Transport {
                provider: "openrouter".to_string(),
                message: "request timed out".to_string(),
                retryable: true,
            }),
            Ok(ChatResponse::from_text("late retry")),
        ]);
        let runtime =
            CoddyRuntime::with_chat_client(AgentToolRegistry::default(), Arc::new(chat_client));
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openrouter".to_string(),
                    name: "deepseek/deepseek-v4-flash".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "inspect".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: Some(ModelCredential {
                    provider: "openrouter".to_string(),
                    token: "sk-or-test-token".to_string(),
                    endpoint: None,
                    metadata: Default::default(),
                }),
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");

        assert_eq!(captured_requests.len(), 1);
        assert!(text.contains("request timed out"));
        assert!(!text.contains("late retry"));
    }

    #[test]
    fn ask_command_requires_approval_before_sensitive_file_tool_observation() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write(".env", "OPENAI_API_KEY=sk-secret-token\n");
        let (chat_client, requests) = QueuedChatClient::new(vec![ChatResponse {
            text: "I will inspect the requested file.".to_string(),
            deltas: vec!["I will inspect the requested file.".to_string()],
            finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
            tool_calls: vec![ChatToolCall {
                id: Some("call-1".to_string()),
                name: READ_FILE_TOOL.to_string(),
                arguments: json!({ "path": ".env", "max_bytes": 120 }),
            }],
        }]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "inspect the env file".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        let snapshot = runtime.snapshot();
        let pending = snapshot
            .session
            .pending_permission
            .as_ref()
            .expect("pending sensitive read permission");

        assert_eq!(captured_requests.len(), 1);
        assert!(text.contains("requires approval before accessing sensitive workspace content"));
        assert!(text.contains(".env"));
        assert!(!text.contains("sk-secret-token"));
        assert_eq!(pending.tool_name.as_str(), READ_FILE_TOOL);
        assert_eq!(pending.patterns, vec![".env"]);
        assert_eq!(pending.risk_level, ToolRiskLevel::High);
    }

    #[test]
    fn ask_command_executes_model_subagent_output_reducer_tool() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        let (chat_client, requests) = QueuedChatClient::new(vec![
            ChatResponse {
                text: "I will validate the subagent outputs before summarizing.".to_string(),
                deltas: vec!["I will validate the subagent outputs before summarizing.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-reduce".to_string()),
                    name: SUBAGENT_REDUCE_OUTPUTS_TOOL.to_string(),
                    arguments: json!({
                        "goal": "revise seguranca, secrets e sandbox",
                        "max_members": 2,
                        "outputs": {
                            "security-reviewer": {
                                "riskLevel": "low",
                                "findings": [],
                                "requiredFixes": [],
                                "recommendations": []
                            },
                            "reviewer": {
                                "approved": true,
                                "issues": [],
                                "suggestions": [],
                                "blockingProblems": [],
                                "nonBlockingProblems": []
                            }
                        }
                    }),
                }],
            },
            ChatResponse::from_text("The subagent outputs passed reducer validation."),
        ]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "validate multiagent security review outputs".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        let tool_message = captured_requests[1]
            .messages
            .iter()
            .find(|message| message.role == coddy_agent::ChatMessageRole::Tool)
            .expect("tool observation message");
        let events = runtime.events_after(0).0;

        assert_eq!(text, "The subagent outputs passed reducer validation.");
        assert_eq!(captured_requests.len(), 2);
        assert!(tool_message.content.contains("subagent.reduce_outputs"));
        assert!(tool_message.content.contains("2 accepted"));
        assert!(tool_message.content.contains("0 rejected"));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolStarted { name } if name == SUBAGENT_REDUCE_OUTPUTS_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolCompleted { name, status }
                if name == SUBAGENT_REDUCE_OUTPUTS_TOOL && *status == ToolStatus::Succeeded
        )));
        assert!(runtime.snapshot().session.pending_permission.is_none());
    }

    #[test]
    fn ask_command_records_read_tool_context_without_secret_content() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write(".env", "OPENAI_API_KEY=sk-secret-token\n");
        let (chat_client, _requests) = QueuedChatClient::new(vec![ChatResponse {
            text: "I will inspect the requested file.".to_string(),
            deltas: vec!["I will inspect the requested file.".to_string()],
            finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
            tool_calls: vec![ChatToolCall {
                id: Some("call-1".to_string()),
                name: READ_FILE_TOOL.to_string(),
                arguments: json!({ "path": ".env", "max_bytes": 120 }),
            }],
        }]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "inspect the env file".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let pending_request_id = runtime
            .snapshot()
            .session
            .pending_permission
            .as_ref()
            .map(|request| request.id)
            .expect("pending sensitive read");
        let reply = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::ReplyPermission {
                request_id: pending_request_id,
                reply: PermissionReply::Once,
            },
            speak: false,
        }));
        assert!(matches!(reply, CoddyResult::ActionStatus { .. }));
        let snapshot = runtime.snapshot();
        let context_item = snapshot
            .session
            .workspace_context
            .iter()
            .find(|item| item.id == "tool:filesystem.read_file:.env")
            .expect("read file context item");

        assert_eq!(context_item.label, "filesystem.read_file: .env");
        assert!(context_item.sensitive);
        assert!(!context_item.label.contains("sk-secret-token"));
    }

    #[test]
    fn ask_command_continues_safe_tool_loop_after_followup_requests_more_context() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("src/main.rs", "fn main() {\n    println!(\"hi\");\n}\n");
        let (chat_client, requests) = QueuedChatClient::new(vec![
            ChatResponse {
                text: "I will inspect the workspace.".to_string(),
                deltas: vec!["I will inspect the workspace.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-1".to_string()),
                    name: LIST_FILES_TOOL.to_string(),
                    arguments: json!({ "path": ".", "max_entries": 20 }),
                }],
            },
            ChatResponse {
                text: "I found source files and need to read the entrypoint.".to_string(),
                deltas: vec!["I found source files and need to read the entrypoint.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-2".to_string()),
                    name: READ_FILE_TOOL.to_string(),
                    arguments: json!({ "path": "src/main.rs", "max_bytes": 200 }),
                }],
            },
            ChatResponse::from_text("The entrypoint prints hi from main."),
        ]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "inspect the entrypoint".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");

        assert_eq!(text, "The entrypoint prints hi from main.");
        assert_eq!(captured_requests.len(), 3);
        assert!(captured_requests[2]
            .tools
            .iter()
            .any(|tool| tool.name == READ_FILE_TOOL));
        assert!(captured_requests[2].messages.iter().any(|message| {
            message.role == coddy_agent::ChatMessageRole::Tool
                && message.content.contains("filesystem.read_file")
                && message.content.contains("println!")
        }));
        let events = runtime.events_after(2).0;
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(&event.event, ReplEvent::ToolStarted { name } if name == LIST_FILES_TOOL || name == READ_FILE_TOOL))
                .count(),
            2
        );
    }

    #[test]
    fn ask_command_allows_model_preview_edit_to_request_approval_after_read() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("src/lib.rs", "pub fn answer() -> i32 {\n    1\n}\n");
        let (chat_client, requests) = QueuedChatClient::new(vec![
            ChatResponse {
                text: "I will read the target file first.".to_string(),
                deltas: vec!["I will read the target file first.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-read".to_string()),
                    name: READ_FILE_TOOL.to_string(),
                    arguments: json!({ "path": "src/lib.rs", "max_bytes": 400 }),
                }],
            },
            ChatResponse {
                text: "I can prepare the requested change for approval.".to_string(),
                deltas: vec!["I can prepare the requested change for approval.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-preview".to_string()),
                    name: PREVIEW_EDIT_TOOL.to_string(),
                    arguments: json!({
                        "path": "src/lib.rs",
                        "old_string": "    1",
                        "new_string": "    2"
                    }),
                }],
            },
            ChatResponse::from_text("I prepared a diff and it is waiting for approval."),
        ]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "change answer to 2".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        let events = runtime.events_after(2).0;
        let snapshot = runtime.snapshot();
        let file_text = fs::read_to_string(workspace.path.join("src/lib.rs")).expect("read file");

        assert_eq!(text, "I prepared a diff and it is waiting for approval.");
        assert_eq!(captured_requests.len(), 3);
        assert!(captured_requests[2].messages.iter().any(|message| {
            message.role == coddy_agent::ChatMessageRole::Tool
                && message.content.contains(PREVIEW_EDIT_TOOL)
                && message.content.contains("-    1")
                && message.content.contains("+    2")
        }));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::PermissionRequested { request }
                if request.tool_name.as_str() == "filesystem.apply_edit"
                    && request.patterns == vec!["src/lib.rs"]
        )));
        assert_eq!(
            snapshot.session.status,
            coddy_core::SessionStatus::AwaitingToolApproval
        );
        assert!(snapshot.session.pending_permission.is_some());
        assert_eq!(file_text, "pub fn answer() -> i32 {\n    1\n}\n");
    }

    #[test]
    fn reply_permission_command_applies_pending_model_edit_and_clears_approval() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("src/lib.rs", "pub fn answer() -> i32 {\n    1\n}\n");
        let (chat_client, _requests) = QueuedChatClient::new(vec![
            ChatResponse {
                text: "Reading before edit.".to_string(),
                deltas: vec!["Reading before edit.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-read".to_string()),
                    name: READ_FILE_TOOL.to_string(),
                    arguments: json!({ "path": "src/lib.rs", "max_bytes": 400 }),
                }],
            },
            ChatResponse {
                text: "Preparing edit for approval.".to_string(),
                deltas: vec!["Preparing edit for approval.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-preview".to_string()),
                    name: PREVIEW_EDIT_TOOL.to_string(),
                    arguments: json!({
                        "path": "src/lib.rs",
                        "old_string": "    1",
                        "new_string": "    2"
                    }),
                }],
            },
            ChatResponse::from_text("Approval is ready."),
        ]);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "change answer to 2".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));
        let pending_request_id = runtime
            .snapshot()
            .session
            .pending_permission
            .as_ref()
            .map(|request| request.id)
            .expect("pending permission");

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id: Uuid::new_v4(),
            command: ReplCommand::ReplyPermission {
                request_id: pending_request_id,
                reply: PermissionReply::Once,
            },
            speak: false,
        }));

        let CoddyResult::ActionStatus { message, .. } = result else {
            panic!("expected action status");
        };
        let snapshot = runtime.snapshot();
        let file_text = fs::read_to_string(workspace.path.join("src/lib.rs")).expect("read file");

        assert!(message.contains("Permission Once accepted"));
        assert_eq!(file_text, "pub fn answer() -> i32 {\n    2\n}\n");
        assert_eq!(snapshot.session.status, coddy_core::SessionStatus::Idle);
        assert!(snapshot.session.pending_permission.is_none());
    }

    #[test]
    fn ask_command_stops_safe_tool_loop_after_round_limit() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let repeated_tool_response = |id: &str| ChatResponse {
            text: format!("Need another workspace pass {id}."),
            deltas: vec![format!("Need another workspace pass {id}.")],
            finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
            tool_calls: vec![ChatToolCall {
                id: Some(id.to_string()),
                name: LIST_FILES_TOOL.to_string(),
                arguments: json!({ "path": ".", "max_entries": 20 }),
            }],
        };
        let responses = (1..=(MAX_MODEL_TOOL_ROUNDS + 1))
            .map(|index| repeated_tool_response(&format!("call-{index}")))
            .collect::<Vec<_>>();
        let (chat_client, requests) = QueuedChatClient::new(responses);
        let runtime = CoddyRuntime::with_workspace_and_chat_client(
            AgentToolRegistry::default(),
            &workspace.path,
            Arc::new(chat_client),
        )
        .expect("runtime");
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "keep inspecting".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        let events = runtime.events_after(2).0;

        assert_eq!(captured_requests.len(), MAX_MODEL_TOOL_ROUNDS + 1);
        assert!(text.contains("bounded tool loop limit"));
        assert!(text.contains("filesystem.list_files"));
        assert!(text.contains("Last tool observations:"));
        assert!(text.contains("README.md"));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(&event.event, ReplEvent::ToolStarted { name } if name == LIST_FILES_TOOL))
                .count(),
            MAX_MODEL_TOOL_ROUNDS
        );
    }

    #[test]
    fn ask_command_routes_workspace_listing_through_read_only_tool() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        workspace.mkdir("crates");
        let runtime = CoddyRuntime::with_workspace(AgentToolRegistry::default(), &workspace.path)
            .expect("runtime");

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "list files".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let events = runtime.events_after(1).0;
        let snapshot = runtime.snapshot();

        assert!(text.contains("Workspace entries under `workspace`"));
        assert!(text.contains("README.md"));
        assert!(text.contains("crates"));
        assert!(events.iter().any(|event| matches!(
            event.event,
            ReplEvent::IntentDetected {
                intent: coddy_core::ReplIntent::ManageContext,
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolStarted { name } if name == LIST_FILES_TOOL
        )));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolCompleted { name, status: coddy_core::ToolStatus::Succeeded }
                if name == LIST_FILES_TOOL
        )));
        assert!(snapshot
            .session
            .messages
            .last()
            .is_some_and(|message| message.text.contains("README.md")));
    }

    #[test]
    fn ask_command_tracks_agent_run_v2_for_workspace_listing() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let runtime = CoddyRuntime::with_workspace(AgentToolRegistry::default(), &workspace.path)
            .expect("runtime");

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "list files".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let run_id = runtime
            .events_after(1)
            .0
            .iter()
            .find_map(|event| match event.event {
                ReplEvent::RunStarted { run_id } => Some(run_id),
                _ => None,
            })
            .expect("run started");
        let summary = runtime.agent_run_summary(run_id).expect("run summary");
        let snapshot = runtime.snapshot();

        assert_eq!(summary.goal, "list files");
        assert_eq!(summary.last_phase, coddy_agent::AgentRunPhase::Completed);
        assert_eq!(summary.completed_steps, 3);
        assert!(summary.failure_code.is_none());
        assert_eq!(
            snapshot.session.agent_run.as_ref().map(|run| run.run_id),
            Some(run_id)
        );
        assert_eq!(
            snapshot
                .session
                .agent_run
                .as_ref()
                .map(|run| run.summary.last_phase),
            Some(coddy_agent::AgentRunPhase::Completed)
        );
    }

    #[test]
    fn ask_command_tracks_recoverable_model_failure_in_agent_run_v2() {
        let request_id = Uuid::new_v4();
        let chat_client = FailingChatClient {
            error: ChatModelError::Transport {
                provider: "openai".to_string(),
                message: "timeout".to_string(),
                retryable: true,
            },
        };
        let runtime =
            CoddyRuntime::with_chat_client(AgentToolRegistry::default(), Arc::new(chat_client));
        runtime.publish_event(
            ReplEvent::ModelSelected {
                model: ModelRef {
                    provider: "openai".to_string(),
                    name: "gpt-test".to_string(),
                },
                role: ModelRole::Chat,
            },
            None,
            1_775_000_000_100,
        );

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "debug this timeout".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let run_id = runtime
            .events_after(1)
            .0
            .iter()
            .find_map(|event| match event.event {
                ReplEvent::RunStarted { run_id } => Some(run_id),
                _ => None,
            })
            .expect("run started");
        let summary = runtime.agent_run_summary(run_id).expect("run summary");
        let snapshot = runtime.snapshot();

        assert_eq!(summary.last_phase, coddy_agent::AgentRunPhase::Failed);
        assert_eq!(summary.failure_code.as_deref(), Some("transport_error"));
        assert!(summary.recoverable_failure);
        assert_eq!(
            snapshot
                .session
                .agent_run
                .as_ref()
                .and_then(|run| run.summary.failure_code.as_deref()),
            Some("transport_error")
        );
    }

    #[test]
    fn workspace_listing_does_not_allow_path_traversal() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        let runtime = CoddyRuntime::with_workspace(AgentToolRegistry::default(), &workspace.path)
            .expect("runtime");

        let result = runtime.handle_request(CoddyRequest::Command(ReplCommandJob {
            request_id,
            command: ReplCommand::Ask {
                text: "list files in ..".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        let CoddyResult::Text { text, .. } = result else {
            panic!("expected text result");
        };
        let events = runtime.events_after(1).0;

        assert!(text.contains("path traversal is not allowed"));
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::ToolCompleted { name, status: coddy_core::ToolStatus::Failed }
                if name == LIST_FILES_TOOL
        )));
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
                model_credential: None,
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
    fn stop_active_run_publishes_cancelled_agent_run_summary() {
        let request_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let runtime = CoddyRuntime::default();
        runtime.publish_event(
            ReplEvent::RunStarted { run_id },
            Some(run_id),
            1_775_000_000_060,
        );
        runtime.start_agent_run(run_id, "stop long running command");
        runtime.transition_agent_run(run_id, AgentRunAction::Plan);
        let before_stop_sequence = runtime.snapshot().last_sequence;

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
        let events = runtime.events_after(before_stop_sequence).0;
        let snapshot = runtime.snapshot();
        let agent_run = snapshot
            .session
            .agent_run
            .expect("cancelled agent run summary");

        assert_eq!(agent_run.run_id, run_id);
        assert_eq!(
            agent_run.summary.last_phase,
            coddy_agent::AgentRunPhase::Cancelled,
        );
        assert_eq!(
            agent_run.summary.stop_reason,
            Some(AgentRunStopReason::UserInterrupt),
        );
        assert!(events.iter().any(|event| matches!(
            &event.event,
            ReplEvent::AgentRunUpdated { run_id: updated, summary }
                if *updated == run_id
                    && summary.last_phase == coddy_agent::AgentRunPhase::Cancelled
                    && summary.stop_reason == Some(AgentRunStopReason::UserInterrupt)
        )));
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

    #[tokio::test]
    async fn unix_listener_loop_serves_command_while_event_stream_is_open() {
        let socket_path = test_socket_path("runtime-loop-concurrent");
        let listener = UnixListener::bind(&socket_path).expect("bind runtime socket");
        let runtime = CoddyRuntime::default();
        let server_runtime = runtime.clone();
        let server = tokio::spawn(async move {
            server_runtime
                .serve_unix_listener(listener)
                .await
                .expect("serve unix listener loop");
        });
        let client = CoddyClient::new(&socket_path);
        let mut stream = client.event_stream(1).await.expect("open runtime stream");
        let model = ModelRef {
            provider: "ollama".to_string(),
            name: "qwen2.5-coder:7b".to_string(),
        };

        let result = client
            .send_command(
                ReplCommand::SelectModel {
                    model: model.clone(),
                    role: ModelRole::Chat,
                },
                false,
            )
            .await
            .expect("send command while stream is open");
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .expect("stream event before timeout")
            .expect("stream frame")
            .expect("stream event");
        let snapshot = client.snapshot().await.expect("session snapshot");

        assert!(matches!(result, CoddyResult::ActionStatus { .. }));
        assert!(matches!(
            frame.event.event,
            ReplEvent::ModelSelected {
                model: streamed_model,
                role: ModelRole::Chat,
            } if streamed_model == model
        ));
        assert_eq!(snapshot.session.selected_model, model);

        server.abort();
        let _ = server.await;
        let _ = std::fs::remove_file(socket_path);
    }

    fn test_socket_path(label: &str) -> PathBuf {
        env::temp_dir().join(format!("coddy-runtime-{label}-{}.sock", Uuid::new_v4()))
    }
}
