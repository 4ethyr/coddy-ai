mod config;
mod repl_terminal;
mod shortcut;
mod voice_overlay;

use crate::config::CoddyRuntimeConfig;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use coddy_client::CoddyClient;
use coddy_core::{AssessmentPolicy, ContextPolicy, ModelRef, ModelRole, ReplCommand, ReplMode};
use coddy_core::{ReplShellContext, ScreenAssistMode, SessionStatus};
use coddy_ipc::CoddyResult;
use std::{env, ffi::OsString, process::Stdio};
use tokio::process::Command as TokioCommand;
use tracing::{info, warn};

#[derive(Debug, Parser)]
#[command(name = "coddy")]
#[command(about = "Backend CLI do Coddy REPL")]
struct Cli {
    #[arg(long, global = true, default_value_t = false)]
    speak: bool,

    #[arg(long, hide = true, default_value_t = false)]
    voice_overlay_listening: bool,

    #[arg(long, hide = true, default_value_t = 4000)]
    voice_overlay_duration_ms: u64,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Repl {
        #[arg(long, default_value_t = false)]
        terminal: bool,
    },
    Ask {
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    Voice {
        #[arg(long)]
        transcript: Option<String>,

        #[arg(long, default_value_t = false)]
        overlay: bool,
    },
    StopSpeaking,
    StopActiveRun,
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    Ui {
        #[command(subcommand)]
        command: UiCommand,
    },
    Screen {
        #[command(subcommand)]
        command: ScreenCommand,
    },
    Shortcuts {
        #[command(subcommand)]
        command: ShortcutCommand,
    },
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    Doctor {
        #[command(subcommand)]
        command: DoctorCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ScreenCommand {
    Explain {
        #[arg(long, value_enum, default_value = "explain-visible-screen")]
        mode: CliScreenAssistMode,

        #[arg(long, value_enum, default_value = "unknown-assessment")]
        policy: CliAssessmentPolicy,
    },
    DismissConfirmation,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliScreenAssistMode {
    ExplainVisibleScreen,
    ExplainCode,
    DebugError,
    MultipleChoice,
    SummarizeDocument,
}

impl From<CliScreenAssistMode> for ScreenAssistMode {
    fn from(value: CliScreenAssistMode) -> Self {
        match value {
            CliScreenAssistMode::ExplainVisibleScreen => Self::ExplainVisibleScreen,
            CliScreenAssistMode::ExplainCode => Self::ExplainCode,
            CliScreenAssistMode::DebugError => Self::DebugError,
            CliScreenAssistMode::MultipleChoice => Self::MultipleChoice,
            CliScreenAssistMode::SummarizeDocument => Self::SummarizeDocument,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliAssessmentPolicy {
    Practice,
    PermittedAi,
    SyntaxOnly,
    RestrictedAssessment,
    UnknownAssessment,
}

impl From<CliAssessmentPolicy> for AssessmentPolicy {
    fn from(value: CliAssessmentPolicy) -> Self {
        match value {
            CliAssessmentPolicy::Practice => Self::Practice,
            CliAssessmentPolicy::PermittedAi => Self::PermittedAi,
            CliAssessmentPolicy::SyntaxOnly => Self::SyntaxOnly,
            CliAssessmentPolicy::RestrictedAssessment => Self::RestrictedAssessment,
            CliAssessmentPolicy::UnknownAssessment => Self::UnknownAssessment,
        }
    }
}

#[derive(Debug, Subcommand)]
enum ShortcutCommand {
    Test,
    Install {
        #[arg(long, default_value = "Shift+CapsLk")]
        binding: String,

        #[arg(long)]
        coddy_bin: Option<std::path::PathBuf>,

        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ModelCommand {
    Select {
        #[arg(long, default_value = "ollama")]
        provider: String,

        #[arg(long)]
        name: String,

        #[arg(long, value_enum, default_value = "chat")]
        role: CliModelRole,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliModelRole {
    Chat,
    Ocr,
    Asr,
    Tts,
    Embedding,
}

impl From<CliModelRole> for ModelRole {
    fn from(value: CliModelRole) -> Self {
        match value {
            CliModelRole::Chat => Self::Chat,
            CliModelRole::Ocr => Self::Ocr,
            CliModelRole::Asr => Self::Asr,
            CliModelRole::Tts => Self::Tts,
            CliModelRole::Embedding => Self::Embedding,
        }
    }
}

#[derive(Debug, Subcommand)]
enum UiCommand {
    Open {
        #[arg(long, value_enum, default_value = "floating-terminal")]
        mode: CliReplMode,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliReplMode {
    FloatingTerminal,
    DesktopApp,
}

impl From<CliReplMode> for ReplMode {
    fn from(value: CliReplMode) -> Self {
        match value {
            CliReplMode::FloatingTerminal => Self::FloatingTerminal,
            CliReplMode::DesktopApp => Self::DesktopApp,
        }
    }
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    Snapshot,
    Events {
        #[arg(long, default_value_t = 0)]
        after: u64,
    },
    Watch {
        #[arg(long, default_value_t = 0)]
        after: u64,

        #[arg(long)]
        limit: Option<usize>,
    },
}

#[derive(Debug, Subcommand)]
enum DoctorCommand {
    Shortcuts,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.voice_overlay_listening {
        return voice_overlay::run_listening_overlay(cli.voice_overlay_duration_ms);
    }

    let config = CoddyRuntimeConfig::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(env::var("RUST_LOG").unwrap_or_else(|_| config.log_level().to_string()))
        .init();

    match cli.command {
        Some(Command::Repl { terminal }) => {
            if terminal {
                run_terminal_repl(&config).await
            } else {
                let result = send_repl_command(
                    &config,
                    ReplCommand::OpenUi {
                        mode: ReplMode::FloatingTerminal,
                    },
                    cli.speak,
                )
                .await?;
                print_job_result(result)
            }
        }
        Some(Command::Ask { text }) => {
            let text = join_command_text(text);
            let result = send_repl_command(
                &config,
                ReplCommand::Ask {
                    text,
                    context_policy: ContextPolicy::NoScreen,
                },
                cli.speak,
            )
            .await?;
            print_job_result(result)
        }
        Some(Command::Voice {
            transcript,
            overlay,
        }) => {
            let (_lock, _overlay) = if overlay {
                (
                    Some(acquire_voice_shortcut_lock(&config)?),
                    start_listening_overlay(config.voice.record_duration_ms),
                )
            } else {
                (None, None)
            };
            let transcript = match normalize_transcript_override(transcript) {
                Some(transcript) => transcript,
                None => coddy_voice_input::capture_and_transcribe(&config.voice).await?,
            };
            info!(
                chars = transcript.chars().count(),
                "Coddy voice transcript resolved"
            );
            let result = send_repl_command(
                &config,
                ReplCommand::VoiceTurn {
                    transcript_override: Some(transcript),
                },
                cli.speak,
            )
            .await?;
            print_job_result(result)
        }
        Some(Command::StopSpeaking) => {
            let result = coddy_client(&config)?.stop_speaking().await?;
            print_job_result(result)
        }
        Some(Command::StopActiveRun) => {
            let result = coddy_client(&config)?.stop_active_run().await?;
            print_job_result(result)
        }
        Some(Command::Model {
            command:
                ModelCommand::Select {
                    provider,
                    name,
                    role,
                },
        }) => {
            let result = send_repl_command(
                &config,
                ReplCommand::SelectModel {
                    model: ModelRef { provider, name },
                    role: role.into(),
                },
                cli.speak,
            )
            .await?;
            print_job_result(result)
        }
        Some(Command::Ui {
            command: UiCommand::Open { mode },
        }) => {
            let result = send_repl_command(
                &config,
                ReplCommand::OpenUi { mode: mode.into() },
                cli.speak,
            )
            .await?;
            print_job_result(result)
        }
        Some(Command::Screen {
            command: ScreenCommand::Explain { mode, policy },
        }) => {
            let result = send_repl_command(
                &config,
                ReplCommand::CaptureAndExplain {
                    mode: mode.into(),
                    policy: policy.into(),
                },
                cli.speak,
            )
            .await?;
            print_job_result(result)
        }
        Some(Command::Screen {
            command: ScreenCommand::DismissConfirmation,
        }) => {
            let result =
                send_repl_command(&config, ReplCommand::DismissConfirmation, false).await?;
            print_job_result(result)
        }
        Some(Command::Shortcuts {
            command: ShortcutCommand::Test,
        }) => run_shortcuts_test(&config).await,
        Some(Command::Shortcuts {
            command:
                ShortcutCommand::Install {
                    binding,
                    coddy_bin,
                    dry_run,
                },
        }) => run_shortcuts_install(binding, coddy_bin, dry_run),
        Some(Command::Session {
            command: SessionCommand::Snapshot,
        }) => run_session_snapshot(&config).await,
        Some(Command::Session {
            command: SessionCommand::Events { after },
        }) => run_session_events(&config, after).await,
        Some(Command::Session {
            command: SessionCommand::Watch { after, limit },
        }) => run_session_watch(&config, after, limit).await,
        Some(Command::Doctor {
            command: DoctorCommand::Shortcuts,
        }) => run_shortcuts_doctor(&config).await,
        None => {
            println!("Use `coddy repl`, `coddy ask`, `coddy voice`, `coddy screen explain`, `coddy model select`, `coddy ui open`, `coddy stop-speaking`, `coddy stop-active-run`, `coddy session snapshot`, `coddy shortcuts test` ou `coddy doctor shortcuts`.");
            Ok(())
        }
    }
}

async fn send_repl_command(
    config: &CoddyRuntimeConfig,
    command: ReplCommand,
    speak: bool,
) -> Result<CoddyResult> {
    let client = coddy_client(config)?;

    info!(
        socket = %client.socket_path().display(),
        ?command,
        speak,
        "sending Coddy REPL command"
    );

    client.send_command(command, speak).await
}

async fn run_terminal_repl(config: &CoddyRuntimeConfig) -> Result<()> {
    use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};

    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin).lines();
    let mut stdout = io::stdout();
    let history_path = CoddyRuntimeConfig::repl_history_path()?;
    let mut history =
        match repl_terminal::load_history(&history_path, repl_terminal::DEFAULT_HISTORY_LIMIT) {
            Ok(history) => history,
            Err(error) => {
                warn!(
                    path = %history_path.display(),
                    ?error,
                    "failed to load Coddy terminal REPL history"
                );
                repl_terminal::TerminalHistory::new(repl_terminal::DEFAULT_HISTORY_LIMIT)
            }
        };

    stdout
        .write_all(repl_terminal::WELCOME_MESSAGE.as_bytes())
        .await?;
    stdout
        .write_all(repl_terminal::REPL_PROMPT.as_bytes())
        .await?;
    stdout.flush().await?;

    loop {
        tokio::select! {
            line = reader.next_line() => {
                let Some(line) = line? else {
                    return Ok(());
                };

                let recorded = history.record(&line);
                let context = load_repl_shell_context(config).await;
                match repl_terminal::decide_terminal_step(&line, &context) {
                    repl_terminal::TerminalReplDecision::Continue => {}
                    repl_terminal::TerminalReplDecision::Exit(message) => {
                        stdout.write_all(message.as_bytes()).await?;
                        stdout.flush().await?;
                        return Ok(());
                    }
                    repl_terminal::TerminalReplDecision::Render(output) => {
                        stdout.write_all(output.as_bytes()).await?;
                    }
                    repl_terminal::TerminalReplDecision::DispatchCommand(command) => {
                        let result = send_repl_command(config, command, false).await?;
                        stdout
                            .write_all(format_job_result(result)?.as_bytes())
                            .await?;
                    }
                }

                if recorded {
                    if let Err(error) = repl_terminal::save_history(&history_path, &history) {
                        warn!(
                            path = %history_path.display(),
                            ?error,
                            "failed to save Coddy terminal REPL history"
                        );
                    }
                }

                stdout
                    .write_all(repl_terminal::REPL_PROMPT.as_bytes())
                    .await?;
                stdout.flush().await?;
            }
            interrupt = tokio::signal::ctrl_c() => {
                interrupt?;
                stdout.write_all(b"\n").await?;
                stdout
                    .write_all(repl_terminal::EXIT_MESSAGE.as_bytes())
                    .await?;
                stdout.flush().await?;
                return Ok(());
            }
        }
    }
}

async fn load_repl_shell_context(config: &CoddyRuntimeConfig) -> ReplShellContext {
    let client = coddy_client(config).ok();
    let snapshot = match &client {
        Some(client) => client.snapshot().await.ok(),
        None => None,
    };
    let tool_names = match (&client, snapshot.is_some()) {
        (Some(client), true) => client.tools().await.unwrap_or_default(),
        _ => Vec::new(),
    };

    let (session_status, selected_model) = snapshot
        .as_ref()
        .map(|snapshot| {
            (
                snapshot.session.status,
                snapshot.session.selected_model.clone(),
            )
        })
        .unwrap_or_else(|| {
            (
                SessionStatus::Idle,
                ModelRef {
                    provider: "coddy".to_string(),
                    name: "unselected".to_string(),
                },
            )
        });

    ReplShellContext {
        session_status,
        selected_model,
        config_path: CoddyRuntimeConfig::config_path()
            .ok()
            .map(|path| path.display().to_string()),
        tool_names,
    }
}

async fn run_shortcuts_doctor(config: &CoddyRuntimeConfig) -> Result<()> {
    let environment = shortcut::ShortcutEnvironment::detect(config.socket_path()?);
    print!("{environment}");
    let status = shortcut::GnomeShortcutStatus::detect(&shortcut::default_wrapper_path()?);
    print!("{status}");
    environment.validate_for_shortcut()?;
    Ok(())
}

async fn run_shortcuts_test(config: &CoddyRuntimeConfig) -> Result<()> {
    let environment = shortcut::ShortcutEnvironment::detect(config.socket_path()?);
    print!("{environment}");
    environment.validate_for_shortcut()?;
    let lock = shortcut::VoiceShortcutLock::acquire(environment.lock_path()?)?;
    println!("lock_acquired: {}", lock.path().display());

    let result = coddy_client(config)?.stop_speaking().await?;
    print_job_result(result)?;
    println!("shortcut_test: ok");
    Ok(())
}

async fn run_session_snapshot(config: &CoddyRuntimeConfig) -> Result<()> {
    let snapshot = coddy_client(config)?.snapshot().await?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

async fn run_session_events(config: &CoddyRuntimeConfig, after_sequence: u64) -> Result<()> {
    let batch = coddy_client(config)?.events_after(after_sequence).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "last_sequence": batch.last_sequence,
            "events": batch.events,
        }))?
    );
    Ok(())
}

async fn run_session_watch(
    config: &CoddyRuntimeConfig,
    after_sequence: u64,
    limit: Option<usize>,
) -> Result<()> {
    let mut stream = coddy_client(config)?.event_stream(after_sequence).await?;
    let mut received = 0_usize;
    while let Some(frame) = stream.next().await? {
        received += 1;
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "last_sequence": frame.last_sequence,
                "event": frame.event,
            }))?
        );
        if session_watch_limit_reached(received, limit) {
            return Ok(());
        }
    }
    Ok(())
}

fn coddy_client(config: &CoddyRuntimeConfig) -> Result<CoddyClient> {
    Ok(CoddyClient::new(config.socket_path()?))
}

fn acquire_voice_shortcut_lock(config: &CoddyRuntimeConfig) -> Result<shortcut::VoiceShortcutLock> {
    let environment = shortcut::ShortcutEnvironment::detect(config.socket_path()?);
    environment.validate_for_shortcut()?;
    shortcut::VoiceShortcutLock::acquire(environment.lock_path()?)
}

fn start_listening_overlay(duration_ms: u64) -> Option<OverlayGuard> {
    if !voice_overlay::is_overlay_available() {
        warn!("coddy was built without the `gtk-overlay` feature; skipping voice overlay");
        return None;
    }
    if env::var_os("WAYLAND_DISPLAY").is_none() && env::var_os("DISPLAY").is_none() {
        warn!("no graphical display available; skipping voice overlay");
        return None;
    }

    let current_exe = env::current_exe().ok()?;
    let mut child = TokioCommand::new(current_exe);
    child
        .args(overlay_cli_args(duration_ms))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    match child.spawn() {
        Ok(child) => Some(OverlayGuard { child: Some(child) }),
        Err(error) => {
            warn!(?error, "failed to spawn Coddy listening overlay");
            None
        }
    }
}

fn overlay_cli_args(duration_ms: u64) -> Vec<OsString> {
    vec![
        OsString::from("--voice-overlay-listening"),
        OsString::from("--voice-overlay-duration-ms"),
        OsString::from(duration_ms.max(300).to_string()),
    ]
}

struct OverlayGuard {
    child: Option<tokio::process::Child>,
}

impl Drop for OverlayGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
}

fn run_shortcuts_install(
    binding: String,
    coddy_bin: Option<std::path::PathBuf>,
    dry_run: bool,
) -> Result<()> {
    let coddy_bin = match coddy_bin {
        Some(path) => path,
        None => env::current_exe().context("failed to resolve current coddy binary")?,
    };
    let plan = shortcut::ShortcutInstallPlan::new(binding, coddy_bin)?;

    shortcut::install_gnome_shortcut(&plan, dry_run)?;

    println!("Coddy shortcut configured.");
    println!("Binding: {}", plan.resolved_binding);
    println!("Command: {}", plan.wrapper_path.display());
    if dry_run {
        println!("Dry-run: no files or GNOME settings were changed.");
    }
    Ok(())
}

fn print_job_result(result: CoddyResult) -> Result<()> {
    print!("{}", format_job_result(result)?);
    Ok(())
}

fn format_job_result(result: CoddyResult) -> Result<String> {
    match result {
        CoddyResult::Text { text, .. } => Ok(format!("{text}\n")),
        CoddyResult::BrowserQuery { query, summary, .. } => {
            let mut output = format!("Pesquisa: {query}\n");
            if let Some(summary) = summary {
                output.push('\n');
                output.push_str(&summary);
                output.push('\n');
            }
            Ok(output)
        }
        CoddyResult::ActionStatus { message, .. } => Ok(format!("{message}\n")),
        CoddyResult::Error { code, message, .. } => {
            anyhow::bail!("daemon returned error {code}: {message}")
        }
        CoddyResult::ReplSessionSnapshot { snapshot, .. } => {
            Ok(format!("{}\n", serde_json::to_string_pretty(&snapshot)?))
        }
        CoddyResult::ReplEvents {
            events,
            last_sequence,
            ..
        } => Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::json!({
                "last_sequence": last_sequence,
                "events": events,
            }))?
        )),
        CoddyResult::ReplTools { tools, .. } => {
            Ok(format!("{}\n", serde_json::to_string_pretty(&tools)?))
        }
    }
}

fn join_command_text(text: Vec<String>) -> String {
    text.join(" ").trim().to_string()
}

fn normalize_transcript_override(transcript: Option<String>) -> Option<String> {
    transcript
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn session_watch_limit_reached(received: usize, limit: Option<usize>) -> bool {
    limit.is_some_and(|limit| received >= limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_trailing_text_arguments() {
        assert_eq!(
            join_command_text(vec!["quem".into(), "foi".into(), "rousseau?".into()]),
            "quem foi rousseau?"
        );
    }

    #[test]
    fn overlay_cli_args_match_hidden_overlay_command() {
        assert_eq!(
            overlay_cli_args(250).into_iter().collect::<Vec<_>>(),
            vec![
                OsString::from("--voice-overlay-listening"),
                OsString::from("--voice-overlay-duration-ms"),
                OsString::from("300"),
            ]
        );
    }

    #[test]
    fn empty_voice_transcript_override_is_ignored() {
        assert_eq!(normalize_transcript_override(Some("  ".into())), None);
        assert_eq!(
            normalize_transcript_override(Some("  Quem foi Rousseau? ".into())),
            Some("Quem foi Rousseau?".into())
        );
    }

    #[test]
    fn parses_session_watch_options() {
        let cli =
            Cli::try_parse_from(["coddy", "session", "watch", "--after", "7", "--limit", "2"])
                .expect("parse session watch");

        match cli.command {
            Some(Command::Session {
                command: SessionCommand::Watch { after, limit },
            }) => {
                assert_eq!(after, 7);
                assert_eq!(limit, Some(2));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn parses_stop_commands() {
        let stop_speaking =
            Cli::try_parse_from(["coddy", "stop-speaking"]).expect("parse stop-speaking");
        assert!(matches!(stop_speaking.command, Some(Command::StopSpeaking)));

        let stop_active_run =
            Cli::try_parse_from(["coddy", "stop-active-run"]).expect("parse stop-active-run");
        assert!(matches!(
            stop_active_run.command,
            Some(Command::StopActiveRun)
        ));
    }

    #[test]
    fn parses_model_select_command() {
        let cli = Cli::try_parse_from([
            "coddy",
            "model",
            "select",
            "--provider",
            "ollama",
            "--name",
            "qwen2.5:0.5b",
            "--role",
            "chat",
        ])
        .expect("parse model select");

        match cli.command {
            Some(Command::Model {
                command:
                    ModelCommand::Select {
                        provider,
                        name,
                        role,
                    },
            }) => {
                assert_eq!(provider, "ollama");
                assert_eq!(name, "qwen2.5:0.5b");
                assert!(matches!(role, CliModelRole::Chat));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn parses_ui_open_command() {
        let cli = Cli::try_parse_from(["coddy", "ui", "open", "--mode", "desktop-app"])
            .expect("parse ui open");

        match cli.command {
            Some(Command::Ui {
                command: UiCommand::Open { mode },
            }) => assert!(matches!(mode, CliReplMode::DesktopApp)),
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn parses_repl_command() {
        let cli = Cli::try_parse_from(["coddy", "repl"]).expect("parse repl");

        assert!(matches!(
            cli.command,
            Some(Command::Repl { terminal: false })
        ));
    }

    #[test]
    fn parses_terminal_repl_command() {
        let cli = Cli::try_parse_from(["coddy", "repl", "--terminal"]).expect("parse repl");

        assert!(matches!(
            cli.command,
            Some(Command::Repl { terminal: true })
        ));
    }

    #[test]
    fn formats_daemon_text_results_for_cli_and_terminal() {
        let output = format_job_result(CoddyResult::Text {
            request_id: uuid::Uuid::nil(),
            text: "done".to_string(),
            spoken: false,
        })
        .expect("format text");

        assert_eq!(output, "done\n");
    }

    #[test]
    fn formats_repl_tools_results_for_cli_and_terminal() {
        let output = format_job_result(CoddyResult::ReplTools {
            request_id: uuid::Uuid::nil(),
            tools: vec!["filesystem.read_file".to_string(), "shell.run".to_string()],
        })
        .expect("format tools");

        assert!(output.contains("filesystem.read_file"));
        assert!(output.contains("shell.run"));
    }

    #[test]
    fn parses_screen_explain_command() {
        let cli = Cli::try_parse_from([
            "coddy",
            "screen",
            "explain",
            "--mode",
            "multiple-choice",
            "--policy",
            "restricted-assessment",
        ])
        .expect("parse screen explain");

        match cli.command {
            Some(Command::Screen {
                command: ScreenCommand::Explain { mode, policy },
            }) => {
                assert!(matches!(mode, CliScreenAssistMode::MultipleChoice));
                assert!(matches!(policy, CliAssessmentPolicy::RestrictedAssessment));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn parses_screen_dismiss_confirmation_command() {
        let cli = Cli::try_parse_from(["coddy", "screen", "dismiss-confirmation"])
            .expect("parse screen dismiss-confirmation");

        assert!(matches!(
            cli.command,
            Some(Command::Screen {
                command: ScreenCommand::DismissConfirmation
            })
        ));
    }

    #[test]
    fn session_watch_limit_is_optional() {
        assert!(!session_watch_limit_reached(10, None));
        assert!(!session_watch_limit_reached(1, Some(2)));
        assert!(session_watch_limit_reached(2, Some(2)));
    }
}
