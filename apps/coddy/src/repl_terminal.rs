use coddy_core::{
    handle_repl_shell_input, ReplCommand, ReplShellAction, ReplShellContext, ReplShellResponse,
};
use std::{fs, io, path::Path};

pub(crate) const REPL_PROMPT: &str = "coddy> ";
pub(crate) const WELCOME_MESSAGE: &str = "Coddy terminal REPL. Use /help for commands.\n";
pub(crate) const DEFAULT_HISTORY_LIMIT: usize = 500;
pub(crate) const EXIT_MESSAGE: &str = "Bye.\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalHistory {
    entries: Vec<String>,
    limit: usize,
}

impl TerminalHistory {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            limit: limit.max(1),
        }
    }

    pub(crate) fn from_file_text(raw: &str, limit: usize) -> Self {
        let mut history = Self::new(limit);
        for line in raw.lines() {
            history.record(line);
        }
        history
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[String] {
        &self.entries
    }

    pub(crate) fn record(&mut self, input: &str) -> bool {
        let normalized = input.trim();
        if !is_history_entry(normalized)
            || self.entries.last().is_some_and(|last| last == normalized)
        {
            return false;
        }

        self.entries.push(normalized.to_string());
        let extra_entries = self.entries.len().saturating_sub(self.limit);
        if extra_entries > 0 {
            self.entries.drain(..extra_entries);
        }

        true
    }

    pub(crate) fn to_file_text(&self) -> String {
        if self.entries.is_empty() {
            String::new()
        } else {
            format!("{}\n", self.entries.join("\n"))
        }
    }
}

fn is_history_entry(input: &str) -> bool {
    !input.is_empty() && !matches!(input, "/exit" | "/quit")
}

pub(crate) fn load_history(path: &Path, limit: usize) -> io::Result<TerminalHistory> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(TerminalHistory::from_file_text(&raw, limit)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(TerminalHistory::new(limit)),
        Err(error) => Err(error),
    }
}

pub(crate) fn save_history(path: &Path, history: &TerminalHistory) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, history.to_file_text())
}

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
        ReplShellAction::Exit => TerminalReplDecision::Exit(EXIT_MESSAGE.to_string()),
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
    use std::{
        env, fs,
        time::{SystemTime, UNIX_EPOCH},
    };

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

    #[test]
    fn history_records_normalized_non_exit_inputs() {
        let mut history = TerminalHistory::new(10);

        assert!(history.record("  explain this error  "));
        assert!(history.record("/help"));
        assert!(!history.record("   "));
        assert!(!history.record("/exit"));
        assert!(!history.record("/quit"));

        assert_eq!(
            history.entries(),
            &["explain this error".to_string(), "/help".to_string()]
        );
    }

    #[test]
    fn history_skips_consecutive_duplicates_and_keeps_recent_limit() {
        let mut history = TerminalHistory::new(2);

        assert!(history.record("one"));
        assert!(!history.record("one"));
        assert!(history.record("two"));
        assert!(history.record("three"));

        assert_eq!(history.entries(), &["two".to_string(), "three".to_string()]);
    }

    #[test]
    fn history_roundtrips_through_file_text() {
        let history = TerminalHistory::from_file_text("\none\n\n/two\n/exit\n", 10);

        assert_eq!(history.to_file_text(), "one\n/two\n");
    }

    #[test]
    fn history_load_and_save_create_parent_directory() {
        let root = unique_temp_dir();
        let path = root.join("nested").join("repl-history.txt");
        let mut history = TerminalHistory::new(10);
        history.record("first");
        history.record("second");

        save_history(&path, &history).expect("save history");
        let loaded = load_history(&path, 10).expect("load history");

        assert_eq!(
            loaded.entries(),
            &["first".to_string(), "second".to_string()]
        );

        let _ = fs::remove_dir_all(root);
    }

    fn unique_temp_dir() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        env::temp_dir().join(format!("coddy-repl-history-{suffix}"))
    }
}
