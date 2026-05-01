use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplShellInput {
    Empty,
    Help,
    Status,
    Config,
    Tools,
    Exit,
    Workflow {
        kind: CodingWorkflowKind,
        goal: String,
    },
    Ask(String),
    UnknownSlash {
        command: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CodingWorkflowKind {
    Plan,
    Review,
    Test,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplShellAction {
    Noop,
    Exit,
    SendCommand(crate::ReplCommand),
    Render(ReplShellResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplShellContext {
    pub session_status: crate::SessionStatus,
    pub selected_model: crate::ModelRef,
    pub config_path: Option<String>,
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplShellResponse {
    pub title: String,
    pub lines: Vec<String>,
}

pub fn parse_repl_shell_input(input: &str) -> ReplShellInput {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ReplShellInput::Empty;
    }

    if !trimmed.starts_with('/') {
        return ReplShellInput::Ask(trimmed.to_string());
    }

    let command = trimmed
        .split_whitespace()
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    let goal = trimmed
        .split_once(char::is_whitespace)
        .map(|(_, goal)| goal.trim().to_string())
        .unwrap_or_default();

    match command.as_str() {
        "/help" | "/?" => ReplShellInput::Help,
        "/status" => ReplShellInput::Status,
        "/config" => ReplShellInput::Config,
        "/tools" => ReplShellInput::Tools,
        "/exit" | "/quit" => ReplShellInput::Exit,
        "/plan" => ReplShellInput::Workflow {
            kind: CodingWorkflowKind::Plan,
            goal,
        },
        "/review" => ReplShellInput::Workflow {
            kind: CodingWorkflowKind::Review,
            goal,
        },
        "/test" | "/tests" => ReplShellInput::Workflow {
            kind: CodingWorkflowKind::Test,
            goal,
        },
        _ => ReplShellInput::UnknownSlash { command },
    }
}

pub fn handle_repl_shell_input(input: &str, context: &ReplShellContext) -> ReplShellAction {
    match parse_repl_shell_input(input) {
        ReplShellInput::Empty => ReplShellAction::Noop,
        ReplShellInput::Help => ReplShellAction::Render(help_response()),
        ReplShellInput::Status => ReplShellAction::Render(status_response(context)),
        ReplShellInput::Config => ReplShellAction::Render(config_response(context)),
        ReplShellInput::Tools => ReplShellAction::Render(tools_response(context)),
        ReplShellInput::Exit => ReplShellAction::Exit,
        ReplShellInput::Workflow { kind, goal } => {
            if goal.is_empty() {
                return ReplShellAction::Render(workflow_usage_response(kind));
            }
            ReplShellAction::SendCommand(crate::ReplCommand::Ask {
                text: coding_workflow_prompt(kind, &goal),
                context_policy: crate::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            })
        }
        ReplShellInput::Ask(text) => ReplShellAction::SendCommand(crate::ReplCommand::Ask {
            text,
            context_policy: crate::ContextPolicy::WorkspaceOnly,
            model_credential: None,
        }),
        ReplShellInput::UnknownSlash { command } => {
            ReplShellAction::Render(unknown_command_response(&command))
        }
    }
}

fn help_response() -> ReplShellResponse {
    ReplShellResponse {
        title: "Coddy REPL Help".to_string(),
        lines: vec![
            "/help    Show available REPL commands.".to_string(),
            "/status  Show the current session and model.".to_string(),
            "/tools   Show registered local tools.".to_string(),
            "/plan    Build an evidence-based implementation plan.".to_string(),
            "/review  Review code, diffs or architecture risks.".to_string(),
            "/test    Design or run focused validation for a change.".to_string(),
            "/config  Show active configuration source.".to_string(),
            "/exit    Leave the REPL.".to_string(),
        ],
    }
}

fn status_response(context: &ReplShellContext) -> ReplShellResponse {
    ReplShellResponse {
        title: "Coddy REPL Status".to_string(),
        lines: vec![
            format!("Session: {:?}", context.session_status),
            format!(
                "Model: {}/{}",
                context.selected_model.provider, context.selected_model.name
            ),
        ],
    }
}

fn config_response(context: &ReplShellContext) -> ReplShellResponse {
    let source = context
        .config_path
        .as_deref()
        .unwrap_or("default configuration");

    ReplShellResponse {
        title: "Coddy REPL Config".to_string(),
        lines: vec![format!("Source: {source}")],
    }
}

fn tools_response(context: &ReplShellContext) -> ReplShellResponse {
    let mut tool_names = context.tool_names.clone();
    tool_names.sort();

    let lines = if tool_names.is_empty() {
        vec!["No tools registered.".to_string()]
    } else {
        tool_names
            .into_iter()
            .map(|tool_name| format!("- {tool_name}"))
            .collect()
    };

    ReplShellResponse {
        title: "Coddy REPL Tools".to_string(),
        lines,
    }
}

fn unknown_command_response(command: &str) -> ReplShellResponse {
    ReplShellResponse {
        title: "Unknown Command".to_string(),
        lines: vec![
            format!("Unsupported REPL command: {command}"),
            "Use /help to list available commands.".to_string(),
        ],
    }
}

fn workflow_usage_response(kind: CodingWorkflowKind) -> ReplShellResponse {
    let command = match kind {
        CodingWorkflowKind::Plan => "/plan",
        CodingWorkflowKind::Review => "/review",
        CodingWorkflowKind::Test => "/test",
    };
    ReplShellResponse {
        title: "Coding Workflow".to_string(),
        lines: vec![
            format!("Usage: {command} <goal or scope>"),
            "Provide a concrete task so Coddy can inspect, reason and validate with a bounded workflow.".to_string(),
        ],
    }
}

pub fn coding_workflow_prompt(kind: CodingWorkflowKind, goal: &str) -> String {
    let goal = goal.trim();
    match kind {
        CodingWorkflowKind::Plan => format!(
            "Plan-only coding workflow.\n\nGoal: {goal}\n\nInstructions:\n- Inspect safe read-only workspace context before making code claims when tools are available.\n- Do not edit files or run mutating commands in this workflow.\n- Return objective, assumptions, relevant files to inspect, ordered steps, risks, validation plan and approval needs.\n- If evidence is missing, state exactly which read-only inspection is needed."
        ),
        CodingWorkflowKind::Review => format!(
            "Code review workflow.\n\nScope: {goal}\n\nInstructions:\n- Inspect the relevant diff, files or workspace context before making claims when tools are available.\n- Prioritize correctness bugs, regressions, security risks and missing tests.\n- Report findings first with concrete file/function evidence; keep summaries secondary.\n- Do not edit files unless the user explicitly asks for fixes."
        ),
        CodingWorkflowKind::Test => format!(
            "Focused validation workflow.\n\nGoal: {goal}\n\nInstructions:\n- Identify the smallest useful test, lint, type-check or build command for this goal.\n- Inspect project scripts and relevant files before recommending commands when tools are available.\n- Run safe validation when permitted; otherwise explain the exact command and why it was not run.\n- Report pass/fail status, failure cause and next corrective step."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ReplShellContext {
        ReplShellContext {
            session_status: crate::SessionStatus::Idle,
            selected_model: crate::ModelRef {
                provider: "ollama".to_string(),
                name: "qwen2.5:0.5b".to_string(),
            },
            config_path: Some("/tmp/coddy.toml".to_string()),
            tool_names: vec!["filesystem.read_file".to_string(), "shell.run".to_string()],
        }
    }

    #[test]
    fn parses_basic_slash_commands() {
        assert_eq!(parse_repl_shell_input(" /help "), ReplShellInput::Help);
        assert_eq!(parse_repl_shell_input("/?"), ReplShellInput::Help);
        assert_eq!(parse_repl_shell_input("/status"), ReplShellInput::Status);
        assert_eq!(parse_repl_shell_input("/config"), ReplShellInput::Config);
        assert_eq!(parse_repl_shell_input("/tools"), ReplShellInput::Tools);
        assert_eq!(parse_repl_shell_input("/exit"), ReplShellInput::Exit);
        assert_eq!(parse_repl_shell_input("/quit"), ReplShellInput::Exit);
        assert_eq!(
            parse_repl_shell_input("/plan implement workspace support"),
            ReplShellInput::Workflow {
                kind: CodingWorkflowKind::Plan,
                goal: "implement workspace support".to_string(),
            }
        );
        assert_eq!(
            parse_repl_shell_input("/review recent changes"),
            ReplShellInput::Workflow {
                kind: CodingWorkflowKind::Review,
                goal: "recent changes".to_string(),
            }
        );
        assert_eq!(
            parse_repl_shell_input("/tests cancellation flow"),
            ReplShellInput::Workflow {
                kind: CodingWorkflowKind::Test,
                goal: "cancellation flow".to_string(),
            }
        );
    }

    #[test]
    fn empty_input_is_noop() {
        assert_eq!(parse_repl_shell_input("   "), ReplShellInput::Empty);
        assert_eq!(
            handle_repl_shell_input("   ", &context()),
            ReplShellAction::Noop
        );
    }

    #[test]
    fn parses_text_as_ask_action_input() {
        assert_eq!(
            parse_repl_shell_input(" explain this error "),
            ReplShellInput::Ask("explain this error".to_string())
        );
    }

    #[test]
    fn handles_help_status_config_tools_and_exit() {
        assert!(matches!(
            handle_repl_shell_input("/help", &context()),
            ReplShellAction::Render(response) if response.title == "Coddy REPL Help"
        ));
        assert!(matches!(
            handle_repl_shell_input("/status", &context()),
            ReplShellAction::Render(response)
                if response.lines.iter().any(|line| line.contains("Idle"))
        ));
        assert!(matches!(
            handle_repl_shell_input("/config", &context()),
            ReplShellAction::Render(response)
                if response.lines.iter().any(|line| line.contains("/tmp/coddy.toml"))
        ));
        assert!(matches!(
            handle_repl_shell_input("/tools", &context()),
            ReplShellAction::Render(response)
                if response.lines.iter().any(|line| line.contains("shell.run"))
        ));
        assert_eq!(
            handle_repl_shell_input("/exit", &context()),
            ReplShellAction::Exit
        );
    }

    #[test]
    fn handles_user_text_as_structured_ask_command() {
        let action = handle_repl_shell_input("explain this error", &context());

        assert!(matches!(
            action,
            ReplShellAction::SendCommand(crate::ReplCommand::Ask {
                text,
                context_policy: crate::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            }) if text == "explain this error"
        ));
    }

    #[test]
    fn workflow_commands_dispatch_coding_guidance_as_ask_commands() {
        let action = handle_repl_shell_input("/plan add provider fallback", &context());

        assert!(matches!(
            action,
            ReplShellAction::SendCommand(crate::ReplCommand::Ask {
                text,
                context_policy: crate::ContextPolicy::WorkspaceOnly,
                model_credential: None,
            }) if text.contains("Plan-only coding workflow")
                && text.contains("Goal: add provider fallback")
                && text.contains("Do not edit files")
        ));

        let action = handle_repl_shell_input("/review auth module", &context());

        assert!(matches!(
            action,
            ReplShellAction::SendCommand(crate::ReplCommand::Ask { text, .. })
                if text.contains("Code review workflow")
                    && text.contains("Report findings first")
        ));

        let action = handle_repl_shell_input("/test model routing", &context());

        assert!(matches!(
            action,
            ReplShellAction::SendCommand(crate::ReplCommand::Ask { text, .. })
                if text.contains("Focused validation workflow")
                    && text.contains("smallest useful test")
        ));
    }

    #[test]
    fn workflow_commands_without_goal_render_usage() {
        assert!(matches!(
            handle_repl_shell_input("/plan", &context()),
            ReplShellAction::Render(response)
                if response.title == "Coding Workflow"
                    && response.lines.iter().any(|line| line.contains("/plan <goal"))
        ));
    }

    #[test]
    fn unknown_slash_command_returns_recoverable_response() {
        assert!(matches!(
            handle_repl_shell_input("/wat", &context()),
            ReplShellAction::Render(response)
                if response.title == "Unknown Command"
                    && response.lines.iter().any(|line| line.contains("/help"))
        ));
    }

    #[test]
    fn tools_response_is_sorted_and_handles_empty_registry() {
        let mut context = context();
        context.tool_names = vec![
            "shell.run".to_string(),
            "filesystem.read_file".to_string(),
            "edit.preview".to_string(),
        ];

        assert!(matches!(
            handle_repl_shell_input("/tools", &context),
            ReplShellAction::Render(response)
                if response.lines == vec![
                    "- edit.preview".to_string(),
                    "- filesystem.read_file".to_string(),
                    "- shell.run".to_string(),
                ]
        ));

        context.tool_names.clear();

        assert!(matches!(
            handle_repl_shell_input("/tools", &context),
            ReplShellAction::Render(response)
                if response.lines == vec!["No tools registered.".to_string()]
        ));
    }
}
