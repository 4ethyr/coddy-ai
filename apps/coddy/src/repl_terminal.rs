use coddy_core::{
    handle_repl_shell_input, ReplCommand, ReplShellAction, ReplShellContext, ReplShellResponse,
};

pub(crate) const REPL_PROMPT: &str = "coddy> ";
pub(crate) const WELCOME_MESSAGE: &str = "Coddy terminal REPL. Use /help for commands.\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminalReplDecision {
    Continue,
    Exit(String),
    Render(String),
    DispatchCommand(ReplCommand),
}

pub(crate) fn decide_terminal_step(
    input: &str,
    context: &ReplShellContext,
) -> TerminalReplDecision {
    match handle_repl_shell_input(input, context) {
        ReplShellAction::Noop => TerminalReplDecision::Continue,
        ReplShellAction::Exit => TerminalReplDecision::Exit("Bye.\n".to_string()),
        ReplShellAction::Render(response) => {
            TerminalReplDecision::Render(render_shell_response(&response))
        }
        ReplShellAction::SendCommand(command) => TerminalReplDecision::DispatchCommand(command),
    }
}

pub(crate) fn render_shell_response(response: &ReplShellResponse) -> String {
    let mut output = String::new();
    output.push_str(&response.title);
    output.push('\n');

    for line in &response.lines {
        output.push_str(line);
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ReplShellContext {
        ReplShellContext {
            session_status: coddy_core::SessionStatus::Idle,
            selected_model: coddy_core::ModelRef {
                provider: "ollama".to_string(),
                name: "qwen2.5:0.5b".to_string(),
            },
            config_path: Some("/tmp/coddy.toml".to_string()),
            tool_names: vec!["shell.run".to_string()],
        }
    }

    #[test]
    fn renders_shell_responses_for_terminal_output() {
        let output = render_shell_response(&ReplShellResponse {
            title: "Title".to_string(),
            lines: vec!["first".to_string(), "second".to_string()],
        });

        assert_eq!(output, "Title\nfirst\nsecond\n");
    }

    #[test]
    fn slash_commands_render_without_dispatching() {
        let decision = decide_terminal_step("/help", &context());

        assert!(matches!(
            decision,
            TerminalReplDecision::Render(output)
                if output.contains("Coddy REPL Help") && output.contains("/status")
        ));
    }

    #[test]
    fn user_text_dispatches_structured_ask_command() {
        let decision = decide_terminal_step("explain this module", &context());

        assert!(matches!(
            decision,
            TerminalReplDecision::DispatchCommand(ReplCommand::Ask {
                text,
                context_policy: coddy_core::ContextPolicy::WorkspaceOnly,
            }) if text == "explain this module"
        ));
    }

    #[test]
    fn exit_command_stops_terminal_loop() {
        assert_eq!(
            decide_terminal_step("/exit", &context()),
            TerminalReplDecision::Exit("Bye.\n".to_string())
        );
    }
}
