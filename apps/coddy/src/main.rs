mod config;
mod repl_terminal;
mod shortcut;
mod voice_overlay;

use crate::config::CoddyRuntimeConfig;
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use coddy_agent::{
    run_default_prompt_battery, MultiagentEvalCase, MultiagentEvalRunner, MultiagentEvalSuiteReport,
};
use coddy_client::CoddyClient;
use coddy_core::{
    AssessmentPolicy, ContextPolicy, ModelCredential, ModelRef, ModelRole, PermissionReply,
    ReplCommand, ReplMode,
};
use coddy_core::{ReplShellContext, ScreenAssistMode, SessionStatus};
use coddy_ipc::CoddyResult;
use serde::Deserialize;
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
};
use tokio::net::UnixListener;
use tokio::process::Command as TokioCommand;
use tracing::{info, warn};

const CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV: &str = "CODDY_EPHEMERAL_MODEL_CREDENTIAL";

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
    Permission {
        #[command(subcommand)]
        command: PermissionCommand,
    },
    Shortcuts {
        #[command(subcommand)]
        command: ShortcutCommand,
    },
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    Eval {
        #[command(subcommand)]
        command: EvalCommand,
    },
    Doctor {
        #[command(subcommand)]
        command: DoctorCommand,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
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

#[derive(Debug, Subcommand)]
enum PermissionCommand {
    Reply {
        #[arg(long)]
        request_id: uuid::Uuid,

        #[arg(long, value_enum)]
        reply: CliPermissionReply,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliPermissionReply {
    Once,
    Always,
    Reject,
}

impl From<CliPermissionReply> for PermissionReply {
    fn from(value: CliPermissionReply) -> Self {
        match value {
            CliPermissionReply::Once => Self::Once,
            CliPermissionReply::Always => Self::Always,
            CliPermissionReply::Reject => Self::Reject,
        }
    }
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
    Tools,
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
enum EvalCommand {
    Multiagent {
        #[arg(long)]
        baseline: Option<PathBuf>,

        #[arg(long)]
        write_baseline: Option<PathBuf>,

        #[arg(long, default_value_t = false)]
        json: bool,
    },
    PromptBattery {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum DoctorCommand {
    Shortcuts,
}

#[derive(Debug, Subcommand)]
enum RuntimeCommand {
    Serve {
        #[arg(long)]
        socket: Option<std::path::PathBuf>,
    },
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
                open_desktop_ui_or_send_runtime_command(
                    &config,
                    ReplMode::FloatingTerminal,
                    cli.speak,
                )
                .await
            }
        }
        Some(Command::Ask { text }) => {
            let text = join_command_text(text);
            let model_credential = load_ephemeral_model_credential_from_env()?;
            let result = send_repl_command(
                &config,
                ReplCommand::Ask {
                    text,
                    context_policy: ContextPolicy::NoScreen,
                    model_credential,
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
        }) => open_desktop_ui_or_send_runtime_command(&config, mode.into(), cli.speak).await,
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
        Some(Command::Permission {
            command: PermissionCommand::Reply { request_id, reply },
        }) => {
            let result = send_repl_command(
                &config,
                ReplCommand::ReplyPermission {
                    request_id,
                    reply: reply.into(),
                },
                cli.speak,
            )
            .await?;
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
            command: SessionCommand::Tools,
        }) => run_session_tools(&config).await,
        Some(Command::Session {
            command: SessionCommand::Events { after },
        }) => run_session_events(&config, after).await,
        Some(Command::Session {
            command: SessionCommand::Watch { after, limit },
        }) => run_session_watch(&config, after, limit).await,
        Some(Command::Eval {
            command:
                EvalCommand::Multiagent {
                    baseline,
                    write_baseline,
                    json,
                },
        }) => run_eval_multiagent(baseline, write_baseline, json),
        Some(Command::Eval {
            command: EvalCommand::PromptBattery { json },
        }) => run_eval_prompt_battery(json),
        Some(Command::Doctor {
            command: DoctorCommand::Shortcuts,
        }) => run_shortcuts_doctor(&config).await,
        Some(Command::Runtime {
            command: RuntimeCommand::Serve { socket },
        }) => run_runtime_serve(&config, socket).await,
        None => {
            println!("Use `coddy repl`, `coddy ask`, `coddy voice`, `coddy screen explain`, `coddy permission reply`, `coddy model select`, `coddy ui open`, `coddy stop-speaking`, `coddy stop-active-run`, `coddy session snapshot`, `coddy runtime serve`, `coddy shortcuts test` ou `coddy doctor shortcuts`.");
            Ok(())
        }
    }
}

#[derive(Debug, Deserialize)]
struct EphemeralModelCredential {
    provider: String,
    token: String,
    endpoint: Option<String>,
    #[serde(default)]
    metadata: std::collections::BTreeMap<String, String>,
}

fn load_ephemeral_model_credential_from_env() -> Result<Option<ModelCredential>> {
    let Some(raw) = env::var_os(CODDY_EPHEMERAL_MODEL_CREDENTIAL_ENV) else {
        return Ok(None);
    };
    let raw = raw
        .into_string()
        .map_err(|_| anyhow::anyhow!("ephemeral model credential is not valid UTF-8"))?;
    let parsed: EphemeralModelCredential = serde_json::from_str(&raw)
        .map_err(|_| anyhow::anyhow!("ephemeral model credential is not valid JSON"))?;

    let provider = parsed.provider.trim().to_string();
    let token = parsed.token.trim().to_string();
    let endpoint = parsed
        .endpoint
        .map(|endpoint| endpoint.trim().to_string())
        .filter(|endpoint| !endpoint.is_empty());

    if provider.is_empty() {
        bail!("ephemeral model credential provider is required");
    }
    if token.is_empty() {
        bail!("ephemeral model credential token is required");
    }

    Ok(Some(ModelCredential {
        provider,
        token,
        endpoint,
        metadata: parsed.metadata,
    }))
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

async fn open_desktop_ui_or_send_runtime_command(
    config: &CoddyRuntimeConfig,
    mode: ReplMode,
    speak: bool,
) -> Result<()> {
    if let Some(launcher) = launch_desktop_app()? {
        println!("Coddy Desktop started: {}", launcher.display());
        if let Ok(result) = send_repl_command(config, ReplCommand::OpenUi { mode }, speak).await {
            print_job_result(result)?;
        }
        return Ok(());
    }

    let result = send_repl_command(config, ReplCommand::OpenUi { mode }, speak).await?;
    print_job_result(result)
}

fn launch_desktop_app() -> Result<Option<PathBuf>> {
    let Some(launcher) = resolve_desktop_launcher() else {
        return Ok(None);
    };

    StdCommand::new(&launcher)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start Coddy Desktop {}", launcher.display()))?;

    Ok(Some(launcher))
}

fn resolve_desktop_launcher() -> Option<PathBuf> {
    resolve_desktop_launcher_from(
        env::var_os("CODDY_DESKTOP_BIN"),
        env::current_exe().ok(),
        env::var_os("HOME"),
        env::var_os("PATH"),
        Path::exists,
    )
}

fn resolve_desktop_launcher_from(
    explicit: Option<OsString>,
    current_exe: Option<PathBuf>,
    home: Option<OsString>,
    path: Option<OsString>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let explicit = explicit
        .map(PathBuf::from)
        .filter(|candidate| exists(candidate));
    if explicit.is_some() {
        return explicit;
    }

    if let Some(current_exe) = current_exe {
        if let Some(parent) = current_exe.parent() {
            let sibling = parent.join("coddy-desktop");
            if exists(&sibling) {
                return Some(sibling);
            }
        }
    }

    if let Some(home) = home {
        let local = PathBuf::from(home).join(".local/bin/coddy-desktop");
        if exists(&local) {
            return Some(local);
        }
    }

    path.as_deref()
        .into_iter()
        .flat_map(env::split_paths)
        .map(|dir| dir.join("coddy-desktop"))
        .find(|candidate| exists(candidate))
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

async fn run_session_tools(config: &CoddyRuntimeConfig) -> Result<()> {
    let tools = coddy_client(config)?.tool_catalog().await?;
    println!("{}", serde_json::to_string_pretty(&tools)?);
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

fn run_eval_multiagent(
    baseline: Option<PathBuf>,
    write_baseline: Option<PathBuf>,
    json: bool,
) -> Result<()> {
    let suite = default_multiagent_eval_suite();
    let comparison = baseline
        .as_ref()
        .map(|path| suite.compare_to_baseline_file(path))
        .transpose()?;

    if let Some(path) = write_baseline.as_ref() {
        suite.write_baseline(path)?;
    }

    if json {
        let mut output = serde_json::json!({
            "suite": suite.public_metadata(),
            "baselineWritten": write_baseline.as_ref().map(|path| path.display().to_string()),
        });
        if let Some(comparison) = &comparison {
            output["comparison"] = comparison.public_metadata();
        }
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!(
        "Multiagent eval: score {} | passed {} | failed {}",
        suite.score, suite.passed, suite.failed
    );
    for report in &suite.reports {
        println!(
            "- {}: {} ({})",
            report.case_name,
            eval_status_label(&report.status),
            report.score
        );
        for failure in &report.failures {
            println!("  failure: {failure}");
        }
    }
    if let Some(comparison) = &comparison {
        println!(
            "Baseline comparison: {} | previous {} | current {} | delta {}",
            eval_gate_status_label(comparison.status),
            comparison.previous_score,
            comparison.current_score,
            i16::from(comparison.current_score) - i16::from(comparison.previous_score)
        );
        for regression in &comparison.regressions {
            println!("  regression: {regression}");
        }
        for improvement in &comparison.improvements {
            println!("  improvement: {improvement}");
        }
    }
    if let Some(path) = write_baseline {
        println!("Baseline written: {}", path.display());
    }
    Ok(())
}

fn run_eval_prompt_battery(json: bool) -> Result<()> {
    let report = run_default_prompt_battery();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report.public_metadata())?
        );
        return Ok(());
    }

    println!(
        "Prompt battery: score {} | prompts {} | stacks {} | knowledge areas {} | passed {} | failed {}",
        report.score,
        report.prompt_count,
        report.stack_count,
        report.knowledge_area_count,
        report.passed,
        report.failed
    );
    println!("Member coverage:");
    for (member, count) in &report.member_coverage {
        println!("- {member}: {count}");
    }
    for failure in &report.failures {
        println!(
            "failure: {} [{} / {}]: {}",
            failure.id,
            failure.stack,
            failure.knowledge_area,
            failure.failures.join("; ")
        );
    }
    Ok(())
}

fn default_multiagent_eval_suite() -> MultiagentEvalSuiteReport {
    let runner = MultiagentEvalRunner::default();
    runner.run_suite(&[
        MultiagentEvalCase::new(
            "hardness-multiagent",
            "revise, aprimore e teste multiagents, harness, prompts e metricas",
        )
        .expected_members(&[
            "explorer",
            "coder",
            "test-writer",
            "eval-runner",
            "reviewer",
        ])
        .min_hardness_score(100)
        .max_blocked(0),
        MultiagentEvalCase::new(
            "security-sensitive-routing",
            "revise seguranca, secrets e sandbox",
        )
        .expected_members(&["security-reviewer"])
        .min_hardness_score(100)
        .max_blocked(0)
        .max_awaiting_approval(0),
        MultiagentEvalCase::new(
            "execution-reducer-contracts",
            "revise, aprimore e teste multiagents, harness, prompts e metricas",
        )
        .expected_members(&[
            "explorer",
            "coder",
            "test-writer",
            "eval-runner",
            "reviewer",
        ])
        .min_hardness_score(100)
        .max_blocked(0)
        .validate_execution_reducer(),
    ])
}

fn eval_status_label(status: &coddy_agent::EvalStatus) -> &'static str {
    match status {
        coddy_agent::EvalStatus::Passed => "passed",
        coddy_agent::EvalStatus::Failed => "failed",
    }
}

fn eval_gate_status_label(status: coddy_agent::EvalGateStatus) -> &'static str {
    match status {
        coddy_agent::EvalGateStatus::Passed => "passed",
        coddy_agent::EvalGateStatus::Failed => "failed",
    }
}

async fn run_runtime_serve(config: &CoddyRuntimeConfig, socket: Option<PathBuf>) -> Result<()> {
    let socket_path = socket.unwrap_or(config.socket_path()?);
    prepare_runtime_socket_path(&socket_path)?;
    let listener = UnixListener::bind(&socket_path).with_context(|| {
        format!(
            "failed to bind Coddy runtime socket {}",
            socket_path.display()
        )
    })?;
    let runtime = coddy_runtime::CoddyRuntime::default();

    info!(socket = %socket_path.display(), "serving local Coddy runtime");
    runtime.serve_unix_listener(listener).await?;
    Ok(())
}

fn prepare_runtime_socket_path(socket_path: &std::path::Path) -> Result<()> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create socket dir {}", parent.display()))?;
    }
    if socket_path.exists() {
        if std::os::unix::net::UnixStream::connect(socket_path).is_ok() {
            anyhow::bail!(
                "Coddy runtime socket already exists and is accepting connections: {}. Stop the running runtime or choose --socket.",
                socket_path.display()
            );
        }
        fs::remove_file(socket_path).with_context(|| {
            format!(
                "failed to remove stale Coddy runtime socket {}",
                socket_path.display()
            )
        })?;
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
        CoddyResult::ReplToolCatalog { tools, .. } => {
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
    fn parses_session_tools_command() {
        let cli = Cli::try_parse_from(["coddy", "session", "tools"]).expect("parse session tools");

        assert!(matches!(
            cli.command,
            Some(Command::Session {
                command: SessionCommand::Tools
            })
        ));
    }

    #[test]
    fn parses_runtime_serve_command() {
        let cli = Cli::try_parse_from(["coddy", "runtime", "serve", "--socket", "/tmp/coddy.sock"])
            .expect("parse runtime serve");

        match cli.command {
            Some(Command::Runtime {
                command: RuntimeCommand::Serve { socket },
            }) => {
                assert_eq!(socket, Some(std::path::PathBuf::from("/tmp/coddy.sock")));
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn parses_eval_multiagent_command() {
        let cli = Cli::try_parse_from([
            "coddy",
            "eval",
            "multiagent",
            "--baseline",
            "evals/baselines/main.json",
            "--write-baseline",
            "evals/reports/latest.json",
            "--json",
        ])
        .expect("parse eval multiagent");

        match cli.command {
            Some(Command::Eval {
                command:
                    EvalCommand::Multiagent {
                        baseline,
                        write_baseline,
                        json,
                    },
            }) => {
                assert_eq!(baseline, Some(PathBuf::from("evals/baselines/main.json")));
                assert_eq!(
                    write_baseline,
                    Some(PathBuf::from("evals/reports/latest.json"))
                );
                assert!(json);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn default_multiagent_eval_suite_is_ci_ready() {
        let suite = default_multiagent_eval_suite();

        assert!(suite.is_success());
        assert_eq!(suite.score, 100);
        assert_eq!(suite.passed, 3);
        assert_eq!(suite.failed, 0);
        assert!(suite
            .reports
            .iter()
            .any(|report| report.case_name == "hardness-multiagent"));
        assert!(suite
            .reports
            .iter()
            .any(|report| report.case_name == "security-sensitive-routing"));
        assert!(suite.reports.iter().any(|report| {
            report.case_name == "execution-reducer-contracts" && report.execution_metrics.is_some()
        }));
    }

    #[test]
    fn parses_eval_prompt_battery_command() {
        let cli = Cli::try_parse_from(["coddy", "eval", "prompt-battery", "--json"])
            .expect("parse prompt battery");

        match cli.command {
            Some(Command::Eval {
                command: EvalCommand::PromptBattery { json },
            }) => {
                assert!(json);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn default_prompt_battery_eval_is_ci_ready() {
        let report = run_default_prompt_battery();

        assert!(report.is_success());
        assert_eq!(report.prompt_count, 300);
        assert_eq!(report.stack_count, 30);
        assert_eq!(report.knowledge_area_count, 10);
        assert_eq!(report.passed, 300);
        assert_eq!(report.failed, 0);
        assert_eq!(report.score, 100);
        assert!(report.member_coverage.contains_key("coder"));
        assert!(report.member_coverage.contains_key("security-reviewer"));
        assert!(report.member_coverage.contains_key("eval-runner"));
    }

    #[test]
    fn runtime_socket_preparation_creates_parent_and_removes_stale_socket() {
        let root = std::env::temp_dir().join(format!("coddy-runtime-cli-{}", uuid::Uuid::new_v4()));
        let socket_path = root.join("nested").join("coddy.sock");

        prepare_runtime_socket_path(&socket_path).expect("prepare missing socket path");
        assert!(root.join("nested").exists());

        std::fs::write(&socket_path, "").expect("create stale socket placeholder");
        prepare_runtime_socket_path(&socket_path).expect("remove stale socket placeholder");

        assert!(!socket_path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_socket_preparation_rejects_live_runtime_socket() {
        let root = std::env::temp_dir().join(format!("coddy-runtime-cli-{}", uuid::Uuid::new_v4()));
        let socket_path = root.join("nested").join("coddy.sock");
        std::fs::create_dir_all(socket_path.parent().expect("socket parent"))
            .expect("create socket parent");
        let _listener =
            std::os::unix::net::UnixListener::bind(&socket_path).expect("bind live runtime socket");

        let error = prepare_runtime_socket_path(&socket_path).expect_err("live socket rejected");

        assert!(error.to_string().contains("accepting connections"));
        let _ = std::fs::remove_dir_all(root);
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
    fn desktop_launcher_resolution_prefers_explicit_env_path() {
        let resolved = resolve_desktop_launcher_from(
            Some(OsString::from("/opt/coddy/bin/coddy-desktop")),
            Some(PathBuf::from("/home/demo/.local/bin/coddy")),
            Some(OsString::from("/home/demo")),
            Some(OsString::from("/usr/bin")),
            |candidate| candidate == Path::new("/opt/coddy/bin/coddy-desktop"),
        );

        assert_eq!(
            resolved,
            Some(PathBuf::from("/opt/coddy/bin/coddy-desktop"))
        );
    }

    #[test]
    fn desktop_launcher_resolution_uses_installed_sibling() {
        let resolved = resolve_desktop_launcher_from(
            None,
            Some(PathBuf::from("/home/demo/.local/bin/coddy")),
            Some(OsString::from("/home/demo")),
            Some(OsString::from("/usr/bin")),
            |candidate| candidate == Path::new("/home/demo/.local/bin/coddy-desktop"),
        );

        assert_eq!(
            resolved,
            Some(PathBuf::from("/home/demo/.local/bin/coddy-desktop"))
        );
    }

    #[test]
    fn parses_permission_reply_command() {
        let request_id = uuid::Uuid::new_v4();
        let cli = Cli::try_parse_from([
            "coddy",
            "permission",
            "reply",
            "--request-id",
            &request_id.to_string(),
            "--reply",
            "once",
        ])
        .expect("parse permission reply");

        match cli.command {
            Some(Command::Permission {
                command:
                    PermissionCommand::Reply {
                        request_id: id,
                        reply,
                    },
            }) => {
                assert_eq!(id, request_id);
                assert!(matches!(reply, CliPermissionReply::Once));
            }
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
    fn formats_repl_tool_catalog_results_for_cli_and_terminal() {
        let output = format_job_result(CoddyResult::ReplToolCatalog {
            request_id: uuid::Uuid::nil(),
            tools: vec![coddy_ipc::ReplToolCatalogItem {
                name: "filesystem.read_file".to_string(),
                description: "Read a file".to_string(),
                category: coddy_core::ToolCategory::Filesystem,
                input_schema: serde_json::json!({
                    "type": "object",
                    "required": ["path"]
                }),
                output_schema: serde_json::json!({
                    "type": "object"
                }),
                risk_level: coddy_core::ToolRiskLevel::Low,
                permissions: vec![coddy_core::ToolPermission::ReadWorkspace],
                timeout_ms: 5_000,
                approval_policy: coddy_core::ApprovalPolicy::AutoApprove,
            }],
        })
        .expect("format tool catalog");

        assert!(output.contains("filesystem.read_file"));
        assert!(output.contains("ReadWorkspace"));
        assert!(output.contains("AutoApprove"));
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
