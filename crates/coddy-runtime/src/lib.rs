use coddy_agent::{
    AgentToolRegistry, ChatMessage, ChatModelClient, ChatModelError, ChatRequest, ChatResponse,
    ChatToolCall, ChatToolSpec, DefaultChatModelClient, LocalAgentRuntime, SubagentExecutionGate,
    SubagentExecutionHandoff, SubagentExecutionStartPlan, SubagentExecutionStartStatus,
    LIST_FILES_TOOL, PREVIEW_EDIT_TOOL, READ_FILE_TOOL, SEARCH_FILES_TOOL, SUBAGENT_PREPARE_TOOL,
    SUBAGENT_ROUTE_TOOL,
};
use coddy_core::{
    ApprovalPolicy, ContextItem, ContextPolicy, ModelCredential, ModelRef, PermissionReply,
    ReplCommand, ReplEvent, ReplEventBroker, ReplEventEnvelope, ReplIntent, ReplMessage, ReplMode,
    ReplSession, ReplSessionSnapshot, SubagentHandoffPrepared, SubagentLifecycleStatus,
    SubagentLifecycleUpdate, SubagentRouteRecommendation, ToolCall, ToolDefinition, ToolName,
    ToolOutput, ToolResultStatus, ToolRiskLevel,
};
use coddy_ipc::{
    read_frame, write_frame, CoddyIpcResult, CoddyRequest, CoddyResult, CoddyWireRequest,
    CoddyWireResult, ReplCommandJob, ReplEventStreamJob, ReplToolCatalogItem,
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixListener;
use uuid::Uuid;

const MAX_MODEL_TOOL_ROUNDS: usize = 3;

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

struct ModelResponseContext<'a> {
    session_id: Uuid,
    run_id: Uuid,
    selected_model: &'a ModelRef,
    model_credential: Option<ModelCredential>,
    system_prompt: &'a str,
    goal: String,
}

struct ToolRoundOutcome {
    response: AssistantResponse,
    executed_tool_calls: usize,
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
        let mut state = agent_runtime.start_run(
            permission_request.session_id,
            format!("Reply to permission request {permission_request_id}"),
        );
        state.run_id = run_id;
        let outcome = agent_runtime.reply_permission(&mut state, permission_request_id, reply);

        for event in outcome.events {
            self.publish_event_with_run_now(event, run_id);
        }
        self.publish_event_with_run_now(ReplEvent::RunCompleted { run_id }, run_id);

        match outcome.result {
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
        self.publish_event_with_run_now(ReplEvent::IntentDetected { intent, confidence }, run_id);

        let assistant_response = match action {
            AskAction::ListWorkspace { path } => {
                self.execute_workspace_listing(session_id, run_id, &text, &path, selected_model)
            }
            AskAction::ModelBackedResponse => self.execute_model_backed_response(ModelBackedTurn {
                session_id,
                run_id,
                selected_model: &selected_model,
                context_policy,
                session_context: &session_context,
                model_credential,
                user_text: text.clone(),
            }),
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
        self.publish_event_with_run_now(ReplEvent::RunCompleted { run_id }, run_id);

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

        let mut state = agent_runtime.start_run(session_id, goal.to_string());
        state.run_id = run_id;
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

        let mut system_prompt = build_model_system_prompt(
            context_policy,
            session_context,
            self.tool_registry.definitions(),
        );
        if let Some(routing_context) =
            self.prepare_subagent_routing_context(session_id, run_id, &user_text)
        {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&routing_context);
        }
        let request = match ChatRequest::new(
            selected_model.clone(),
            vec![
                ChatMessage::system(system_prompt.clone()),
                ChatMessage::user(user_text.clone()),
            ],
        ) {
            Ok(request) => match request.with_model_credential(model_credential.clone()) {
                Ok(request) => request.with_tools(self.chat_tool_specs()),
                Err(error) => {
                    return AssistantResponse::from_text(model_error_message(
                        &error,
                        selected_model,
                        self.tool_registry.definitions().len(),
                    ));
                }
            },
            Err(error) => {
                return AssistantResponse::from_text(model_error_message(
                    &error,
                    selected_model,
                    self.tool_registry.definitions().len(),
                ));
            }
        };

        match self.chat_client.complete(request) {
            Ok(response) => self.assistant_response_from_model(
                ModelResponseContext {
                    session_id,
                    run_id,
                    selected_model,
                    model_credential,
                    system_prompt: &system_prompt,
                    goal: user_text,
                },
                response,
            ),
            Err(error) => AssistantResponse::from_text(model_error_message(
                &error,
                selected_model,
                self.tool_registry.definitions().len(),
            )),
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

        let mut state = agent_runtime.start_run(context.session_id, context.goal.clone());
        state.run_id = context.run_id;
        let mut messages = vec![
            ChatMessage::system(build_tool_followup_system_prompt(context.system_prompt)),
            ChatMessage::user(context.goal.clone()),
        ];
        let mut response = response;

        for _ in 0..MAX_MODEL_TOOL_ROUNDS {
            let round =
                self.execute_model_tool_round(agent_runtime, &mut state, &context, &response);
            if round.executed_tool_calls == 0 {
                return round.response;
            }

            if !response.text.trim().is_empty() {
                messages.push(ChatMessage::assistant(response.text.clone()));
            }
            messages.push(ChatMessage::tool(round.response.text.clone()));

            let Some(next_response) = self.complete_after_tool_messages(
                context.selected_model,
                context.model_credential.clone(),
                messages.clone(),
            ) else {
                return round.response;
            };
            if next_response.tool_calls.is_empty() {
                return AssistantResponse::from_chat_response(next_response);
            }
            response = next_response;
        }

        let tool_summary = summarize_chat_tool_calls(&response.tool_calls);
        let text = if response.text.trim().is_empty() {
            format!(
                "Model requested additional tools after {MAX_MODEL_TOOL_ROUNDS} tool observation rounds: {tool_summary}. Coddy stopped the automatic loop to avoid an unbounded run."
            )
        } else {
            format!(
                "{}\n\nModel requested additional tools after {MAX_MODEL_TOOL_ROUNDS} tool observation rounds: {tool_summary}. Coddy stopped the automatic loop to avoid an unbounded run.",
                response.text.trim()
            )
        };
        AssistantResponse::from_text(redact_context_text(&text))
    }

    fn prepare_subagent_routing_context(
        &self,
        session_id: Uuid,
        run_id: Uuid,
        goal: &str,
    ) -> Option<String> {
        let agent_runtime = self.agent_runtime.as_ref()?;
        let mut state = agent_runtime.start_run(session_id, format!("Route subagents for: {goal}"));
        state.run_id = run_id;
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

        Some(sections.join("\n\n"))
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
        if let Some(handoff) = subagent_handoff_prepared_from_output(output) {
            let execution_handoff = SubagentExecutionHandoff::from(&handoff);
            let execution_plan = SubagentExecutionGate.plan_start_for(&execution_handoff, false);
            let update = execution_plan
                .lifecycle_updates
                .first()
                .cloned()
                .unwrap_or_else(|| subagent_lifecycle_blocked_update(&handoff));
            self.publish_event_with_run_now(ReplEvent::SubagentHandoffPrepared { handoff }, run_id);
            self.publish_event_with_run_now(ReplEvent::SubagentLifecycleUpdated { update }, run_id);
            execution_gate_context = Some(format_subagent_execution_gate_context(&execution_plan));
        }
        Some(match execution_gate_context {
            Some(context) => format!("{}\n\n{context}", format_subagent_handoff_context(output)),
            None => format_subagent_handoff_context(output),
        })
    }

    fn execute_model_tool_round(
        &self,
        agent_runtime: &LocalAgentRuntime,
        state: &mut coddy_agent::RunState,
        context: &ModelResponseContext<'_>,
        response: &ChatResponse,
    ) -> ToolRoundOutcome {
        let mut observations = Vec::new();
        let mut executed_tool_calls = 0_usize;

        for tool_call in response.tool_calls.iter().take(3) {
            let tool_name = match ToolName::new(&tool_call.name) {
                Ok(tool_name) => tool_name,
                Err(error) => {
                    observations.push(format!(
                        "- `{}` was rejected because the tool name is invalid: {error}.",
                        tool_call.name
                    ));
                    continue;
                }
            };

            let Some(definition) = self.tool_registry.get(&tool_name) else {
                observations.push(format!(
                    "- `{tool_name}` was rejected because it is not registered in the local tool registry."
                ));
                continue;
            };

            if !model_tool_call_may_run(&tool_name, definition) {
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
            let call = ToolCall::new(
                context.session_id,
                context.run_id,
                tool_name.clone(),
                tool_call.arguments.clone(),
                unix_ms_now(),
            );
            let outcome = agent_runtime.execute_tool_call(state, &call);
            executed_tool_calls += 1;
            for event in outcome.events {
                self.publish_event_with_run_now(event, context.run_id);
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
        }
    }

    fn complete_after_tool_messages(
        &self,
        selected_model: &ModelRef,
        model_credential: Option<ModelCredential>,
        messages: Vec<ChatMessage>,
    ) -> Option<ChatResponse> {
        let request = ChatRequest::new(selected_model.clone(), messages)
            .ok()?
            .with_model_credential(model_credential)
            .ok()?
            .with_tools(self.chat_tool_specs());
        self.chat_client.complete(request).ok()
    }

    fn chat_tool_specs(&self) -> Vec<ChatToolSpec> {
        self.tool_registry
            .definitions()
            .iter()
            .map(ChatToolSpec::from_tool_definition)
            .collect()
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

fn summarize_chat_tool_calls(tool_calls: &[ChatToolCall]) -> String {
    tool_calls
        .iter()
        .map(|call| call.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn build_model_system_prompt(
    context_policy: ContextPolicy,
    session: &ReplSession,
    tool_definitions: &[ToolDefinition],
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
    sections.push(format_workspace_context(&session.workspace_context));
    sections.push(format_recent_session_messages(&session.messages));
    sections.push(format_tool_context(tool_definitions));
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

fn model_tool_call_may_run(tool_name: &ToolName, definition: &ToolDefinition) -> bool {
    (definition.approval_policy == ApprovalPolicy::AutoApprove
        && definition.risk_level <= ToolRiskLevel::Low)
        || tool_name.as_str() == PREVIEW_EDIT_TOOL
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
    let required_output_fields = array_string_values(
        handoff
            .get("outputSchema")
            .and_then(|schema| schema.get("required")),
        32,
    );
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

fn model_error_message(
    error: &ChatModelError,
    selected_model: &ModelRef,
    tool_count: usize,
) -> String {
    match error {
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
            provider, message, ..
        }
        | ChatModelError::Transport {
            provider, message, ..
        }
        | ChatModelError::InvalidProviderResponse { provider, message } => format!(
            "Coddy could not get a response from {provider} for {}/{}: {message}",
            selected_model.provider, selected_model.name
        ),
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
        PREVIEW_EDIT_TOOL, READ_FILE_TOOL, SUBAGENT_PREPARE_TOOL, SUBAGENT_ROUTE_TOOL,
    };
    use coddy_client::CoddyClient;
    use coddy_core::{
        ApprovalPolicy, ModelRef, ModelRole, PermissionReply, ReplEvent, ToolCategory,
        ToolPermission, ToolRiskLevel,
    };
    use coddy_ipc::{
        ReplCommandJob, ReplEventStreamJob, ReplEventsJob, ReplSessionSnapshotJob, ReplToolsJob,
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
                "subagent.route",
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
    fn ask_command_redacts_secret_like_tool_observations_before_model_followup() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write(".env", "OPENAI_API_KEY=sk-secret-token\n");
        let (chat_client, requests) = QueuedChatClient::new(vec![
            ChatResponse {
                text: "I will inspect the requested file.".to_string(),
                deltas: vec!["I will inspect the requested file.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-1".to_string()),
                    name: READ_FILE_TOOL.to_string(),
                    arguments: json!({ "path": ".env", "max_bytes": 120 }),
                }],
            },
            ChatResponse::from_text("The file contains a redacted API key setting."),
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
                text: "inspect the env file".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
        let captured_requests = requests.lock().expect("requests mutex poisoned");
        assert_eq!(captured_requests.len(), 2);
        let tool_message = captured_requests[1]
            .messages
            .iter()
            .find(|message| message.role == coddy_agent::ChatMessageRole::Tool)
            .expect("tool observation message");

        assert!(tool_message
            .content
            .contains("OPENAI_API_KEY=sk-[REDACTED]"));
        assert!(!tool_message.content.contains("sk-secret-token"));
    }

    #[test]
    fn ask_command_records_read_tool_context_without_secret_content() {
        let request_id = Uuid::new_v4();
        let workspace = TempWorkspace::new();
        workspace.write(".env", "OPENAI_API_KEY=sk-secret-token\n");
        let (chat_client, _requests) = QueuedChatClient::new(vec![
            ChatResponse {
                text: "I will inspect the requested file.".to_string(),
                deltas: vec!["I will inspect the requested file.".to_string()],
                finish_reason: coddy_agent::ChatFinishReason::ToolCalls,
                tool_calls: vec![ChatToolCall {
                    id: Some("call-1".to_string()),
                    name: READ_FILE_TOOL.to_string(),
                    arguments: json!({ "path": ".env", "max_bytes": 120 }),
                }],
            },
            ChatResponse::from_text("The env file was inspected safely."),
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
                text: "inspect the env file".to_string(),
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            },
            speak: false,
        }));

        assert!(matches!(result, CoddyResult::Text { .. }));
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
        let (chat_client, requests) = QueuedChatClient::new(vec![
            repeated_tool_response("call-1"),
            repeated_tool_response("call-2"),
            repeated_tool_response("call-3"),
            repeated_tool_response("call-4"),
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
        assert!(text.contains("stopped the automatic loop"));
        assert!(text.contains("filesystem.list_files"));
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
