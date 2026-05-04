use anyhow::{Context, Result};
use coddy_agent::{ShellExecutionConfig, ShellSandboxPolicy};
use coddy_voice_input::VoiceInputConfig;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{env, ffi::OsString, fs, path::PathBuf};

const CODDY_SHELL_SANDBOX_POLICY_ENV: &str = "CODDY_SHELL_SANDBOX_POLICY";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoddyRuntimeConfig {
    #[serde(default)]
    pub general: CoddyGeneralConfig,
    #[serde(default)]
    pub security: CoddySecurityConfig,
    #[serde(default)]
    pub voice: VoiceInputConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoddyGeneralConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoddySecurityConfig {
    #[serde(default)]
    pub shell_sandbox_policy: CoddyShellSandboxPolicy,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CoddyShellSandboxPolicy {
    #[default]
    Process,
    RequireKernelIsolation,
}

impl CoddyRuntimeConfig {
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read Coddy config {}", path.display()))?;
        Self::from_toml(&raw)
            .with_context(|| format!("failed to parse Coddy config {}", path.display()))
    }

    pub fn from_toml(raw: &str) -> Result<Self> {
        Ok(toml::from_str(raw)?)
    }

    pub fn config_path() -> Result<PathBuf> {
        if let Some(path) = explicit_config_path_from_env() {
            return Ok(path);
        }

        let visionclip_path = project_config_path("io", "4ethyr", "visionclip")?;
        let legacy_path = project_config_path("io", "openai", "ai-snap")?;

        if visionclip_path.exists() {
            Ok(visionclip_path)
        } else if legacy_path.exists() {
            Ok(legacy_path)
        } else {
            Ok(visionclip_path)
        }
    }

    pub fn repl_history_path() -> Result<PathBuf> {
        Ok(coddy_data_dir()?.join("repl-history.txt"))
    }

    pub fn conversation_history_path() -> Result<PathBuf> {
        Ok(coddy_data_dir()?.join("conversation-history.json"))
    }

    pub fn event_audit_log_path() -> Result<PathBuf> {
        Ok(coddy_data_dir()?.join("event-audit.jsonl"))
    }

    pub fn socket_path(&self) -> Result<PathBuf> {
        if let Some(path) = env::var_os("CODDY_DAEMON_SOCKET").map(PathBuf::from) {
            return Ok(path);
        }

        let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .context("XDG_RUNTIME_DIR is not set")?;
        socket_path_from_runtime_dir(runtime_dir)
    }

    pub fn log_level(&self) -> &str {
        self.general.log_level.as_str()
    }

    pub fn shell_execution_config(&self) -> Result<ShellExecutionConfig> {
        self.shell_execution_config_with_env(env::var_os(CODDY_SHELL_SANDBOX_POLICY_ENV))
    }

    fn shell_execution_config_with_env(
        &self,
        shell_sandbox_policy_env: Option<OsString>,
    ) -> Result<ShellExecutionConfig> {
        let sandbox_policy = match shell_sandbox_policy_env {
            Some(value) => parse_shell_sandbox_policy_env(&value)?,
            None => self.security.shell_sandbox_policy,
        };

        Ok(ShellExecutionConfig {
            sandbox_policy: sandbox_policy.into(),
            ..ShellExecutionConfig::default()
        })
    }
}

impl Default for CoddyGeneralConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
        }
    }
}

impl From<CoddyShellSandboxPolicy> for ShellSandboxPolicy {
    fn from(policy: CoddyShellSandboxPolicy) -> Self {
        match policy {
            CoddyShellSandboxPolicy::Process => ShellSandboxPolicy::Process,
            CoddyShellSandboxPolicy::RequireKernelIsolation => {
                ShellSandboxPolicy::RequireKernelIsolation
            }
        }
    }
}

fn parse_shell_sandbox_policy_env(value: &OsString) -> Result<CoddyShellSandboxPolicy> {
    let value = value
        .to_str()
        .context("CODDY_SHELL_SANDBOX_POLICY must be valid UTF-8")?;
    match value {
        "process" => Ok(CoddyShellSandboxPolicy::Process),
        "require-kernel-isolation" => Ok(CoddyShellSandboxPolicy::RequireKernelIsolation),
        other => anyhow::bail!(
            "invalid CODDY_SHELL_SANDBOX_POLICY value {other:?}; expected \"process\" or \"require-kernel-isolation\""
        ),
    }
}

fn explicit_config_path_from_env() -> Option<PathBuf> {
    explicit_config_path(
        env::var_os("CODDY_CONFIG"),
        env::var_os("VISIONCLIP_CONFIG"),
        env::var_os("AI_SNAP_CONFIG"),
    )
}

fn explicit_config_path(
    coddy_path: Option<OsString>,
    visionclip_path: Option<OsString>,
    legacy_path: Option<OsString>,
) -> Option<PathBuf> {
    coddy_path
        .or(visionclip_path)
        .or(legacy_path)
        .map(PathBuf::from)
}

fn project_config_path(qualifier: &str, organization: &str, application: &str) -> Result<PathBuf> {
    let dirs = ProjectDirs::from(qualifier, organization, application)
        .context("failed to resolve Coddy config directory")?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn coddy_data_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("io", "4ethyr", "coddy")
        .context("failed to resolve Coddy data directory")?;
    Ok(dirs.data_local_dir().to_path_buf())
}

fn socket_path_from_runtime_dir(runtime_dir: PathBuf) -> Result<PathBuf> {
    let socket_dir = runtime_dir.join("coddy");
    fs::create_dir_all(&socket_dir)
        .with_context(|| format!("failed to create socket dir {}", socket_dir.display()))?;
    Ok(socket_dir.join("daemon.sock"))
}

fn default_log_level() -> String {
    "info".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_config_matches_daemon_socket_contract() {
        let config = CoddyRuntimeConfig::default();

        assert_eq!(config.log_level(), "info");
        assert_eq!(config.voice.backend, "auto");
        assert_eq!(config.voice.record_duration_ms, 4_000);
    }

    #[test]
    fn parses_voice_section_from_existing_visionclip_toml() {
        let config = CoddyRuntimeConfig::from_toml(
            r#"
            [general]
            log_level = "debug"

            [voice]
            enabled = true
            backend = "pw-record"
            record_duration_ms = 2500
            sample_rate_hz = 48000
            channels = 2
            transcribe_command = "whisper {wav_path}"
            "#,
        )
        .expect("parse config");

        assert_eq!(config.log_level(), "debug");
        assert!(config.voice.enabled);
        assert_eq!(config.voice.backend, "pw-record");
        assert_eq!(config.voice.record_duration_ms, 2_500);
        assert_eq!(config.voice.sample_rate_hz, 48_000);
        assert_eq!(config.voice.channels, 2);
        assert_eq!(config.voice.transcribe_command, "whisper {wav_path}");
    }

    #[test]
    fn parses_security_shell_sandbox_policy_from_toml() {
        let config = CoddyRuntimeConfig::from_toml(
            r#"
            [security]
            shell_sandbox_policy = "require-kernel-isolation"
            "#,
        )
        .expect("parse config");

        assert_eq!(
            config
                .shell_execution_config_with_env(None)
                .expect("shell config")
                .sandbox_policy,
            ShellSandboxPolicy::RequireKernelIsolation
        );
    }

    #[test]
    fn shell_sandbox_policy_env_overrides_toml_config() {
        let config = CoddyRuntimeConfig::from_toml(
            r#"
            [security]
            shell_sandbox_policy = "process"
            "#,
        )
        .expect("parse config");

        let shell_config = config
            .shell_execution_config_with_env(Some(OsString::from("require-kernel-isolation")))
            .expect("shell config");

        assert_eq!(
            shell_config.sandbox_policy,
            ShellSandboxPolicy::RequireKernelIsolation
        );
    }

    #[test]
    fn invalid_shell_sandbox_policy_env_is_rejected() {
        let config = CoddyRuntimeConfig::default();

        let result = config.shell_execution_config_with_env(Some(OsString::from("strict")));

        assert!(result
            .expect_err("invalid env")
            .to_string()
            .contains("CODDY_SHELL_SANDBOX_POLICY"));
    }

    #[test]
    fn socket_path_uses_coddy_daemon_location() {
        let runtime_dir = unique_runtime_dir();
        let socket_path = socket_path_from_runtime_dir(runtime_dir.clone()).expect("socket path");

        assert_eq!(socket_path, runtime_dir.join("coddy").join("daemon.sock"));
        assert!(runtime_dir.join("coddy").exists());

        let _ = fs::remove_dir_all(runtime_dir);
    }

    #[test]
    fn coddy_config_env_takes_precedence_over_visionclip_config() {
        assert_eq!(
            explicit_config_path(
                Some(OsString::from("/home/demo/.config/coddy/config.toml")),
                Some(OsString::from("/home/demo/.config/visionclip/config.toml")),
                Some(OsString::from("/home/demo/.config/ai-snap/config.toml")),
            ),
            Some(PathBuf::from("/home/demo/.config/coddy/config.toml"))
        );
    }

    #[test]
    fn repl_history_path_uses_coddy_data_location() {
        let path = CoddyRuntimeConfig::repl_history_path().expect("history path");
        let rendered = path.to_string_lossy().to_ascii_lowercase();

        assert!(rendered.contains("coddy"));
        assert!(rendered.ends_with("repl-history.txt"));
    }

    #[test]
    fn conversation_history_path_uses_coddy_data_location() {
        let path = CoddyRuntimeConfig::conversation_history_path().expect("history path");
        let rendered = path.to_string_lossy().to_ascii_lowercase();

        assert!(rendered.contains("coddy"));
        assert!(rendered.ends_with("conversation-history.json"));
    }

    #[test]
    fn event_audit_log_path_uses_coddy_data_location() {
        let path = CoddyRuntimeConfig::event_audit_log_path().expect("audit path");
        let rendered = path.to_string_lossy().to_ascii_lowercase();

        assert!(rendered.contains("coddy"));
        assert!(rendered.ends_with("event-audit.jsonl"));
    }

    fn unique_runtime_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        env::temp_dir().join(format!("coddy-runtime-{suffix}"))
    }
}
