use std::{
    collections::HashSet,
    env, fs, io,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use coddy_core::{
    PermissionReply, ReplEvent, ToolError, ToolOutput, ToolResult, ToolResultStatus, ToolStatus,
};
use serde_json::json;

use crate::{
    command_guard::command_uses_network, AgentError, ShellApprovalState, ShellPlan, ToolExecution,
    WorkspaceRoot, SHELL_RUN_TOOL,
};

pub const DEFAULT_SHELL_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
pub const DEFAULT_SHELL_MAX_CPU_TIME_SECONDS: u64 = 600;
pub const DEFAULT_SHELL_MAX_FILE_SIZE_BYTES: u64 = 512 * 1024 * 1024;
const SANDBOX_PROVIDER_PROBE_TIMEOUT_MS: u64 = 1_000;
#[cfg(unix)]
const DEFAULT_SHELL_UMASK: libc::mode_t = 0o077;
const DEFAULT_SHELL_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const SHELL_ENV_ALLOWLIST: &[&str] = &[
    "CARGO_HOME",
    "HOME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "NPM_CONFIG_CACHE",
    "PATH",
    "RUSTUP_HOME",
    "TERM",
    "TMPDIR",
    "TZ",
    "XDG_CACHE_HOME",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellResourceLimits {
    pub max_cpu_time_seconds: Option<u64>,
    pub max_file_size_bytes: Option<u64>,
    pub max_virtual_memory_bytes: Option<u64>,
    pub max_open_files: Option<u64>,
}

impl ShellResourceLimits {
    pub fn unrestricted() -> Self {
        Self {
            max_cpu_time_seconds: None,
            max_file_size_bytes: None,
            max_virtual_memory_bytes: None,
            max_open_files: None,
        }
    }
}

impl Default for ShellResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_time_seconds: Some(DEFAULT_SHELL_MAX_CPU_TIME_SECONDS),
            max_file_size_bytes: Some(DEFAULT_SHELL_MAX_FILE_SIZE_BYTES),
            max_virtual_memory_bytes: None,
            max_open_files: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShellNetworkPolicy {
    #[default]
    Disabled,
    Allowed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShellSandboxPolicy {
    #[default]
    Process,
    RequireKernelIsolation,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShellSandboxProviderDiscovery {
    Disabled,
    #[default]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ShellSandboxProvider {
    Process,
    Bubblewrap { executable: PathBuf },
    Unshare { executable: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShellSandboxProviderSelection {
    provider: ShellSandboxProvider,
    diagnostics: ShellSandboxProviderDiagnostics,
}

impl ShellSandboxProviderSelection {
    fn without_probe_diagnostics(provider: ShellSandboxProvider, path: &str) -> Self {
        Self {
            provider,
            diagnostics: ShellSandboxProviderDiagnostics::from_path(path),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShellSandboxProviderDiagnostics {
    bubblewrap: ShellSandboxProviderProbeDiagnostics,
    unshare: ShellSandboxProviderProbeDiagnostics,
}

impl ShellSandboxProviderDiagnostics {
    fn from_path(path: &str) -> Self {
        Self {
            bubblewrap: ShellSandboxProviderProbeDiagnostics {
                candidate_available: executable_exists_in_path("bwrap", path),
                probe_succeeded: None,
            },
            unshare: ShellSandboxProviderProbeDiagnostics {
                candidate_available: cfg!(target_os = "linux")
                    && executable_exists_in_path("unshare", path),
                probe_succeeded: None,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShellSandboxProviderProbeDiagnostics {
    candidate_available: bool,
    probe_succeeded: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellExecutionConfig {
    pub output_limit_bytes: usize,
    pub poll_interval_ms: u64,
    pub resource_limits: ShellResourceLimits,
    pub network_policy: ShellNetworkPolicy,
    pub sandbox_policy: ShellSandboxPolicy,
    pub sandbox_provider_discovery: ShellSandboxProviderDiscovery,
}

impl Default for ShellExecutionConfig {
    fn default() -> Self {
        Self {
            output_limit_bytes: DEFAULT_SHELL_OUTPUT_LIMIT_BYTES,
            poll_interval_ms: 10,
            resource_limits: ShellResourceLimits::default(),
            network_policy: ShellNetworkPolicy::default(),
            sandbox_policy: ShellSandboxPolicy::default(),
            sandbox_provider_discovery: ShellSandboxProviderDiscovery::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShellExecutor {
    workspace: WorkspaceRoot,
    config: ShellExecutionConfig,
}

impl ShellExecutor {
    pub fn new(workspace_root: impl AsRef<Path>) -> Result<Self, AgentError> {
        Self::with_config(workspace_root, ShellExecutionConfig::default())
    }

    pub fn with_config(
        workspace_root: impl AsRef<Path>,
        config: ShellExecutionConfig,
    ) -> Result<Self, AgentError> {
        if config.output_limit_bytes == 0 {
            return Err(AgentError::InvalidInput(
                "shell output limit must be greater than zero".to_string(),
            ));
        }
        if config.poll_interval_ms == 0 {
            return Err(AgentError::InvalidInput(
                "shell poll interval must be greater than zero".to_string(),
            ));
        }
        validate_resource_limits(config.resource_limits)?;

        Ok(Self {
            workspace: WorkspaceRoot::new(workspace_root)?,
            config,
        })
    }

    pub fn execute(&self, plan: &ShellPlan, approval: Option<PermissionReply>) -> ToolExecution {
        if matches!(plan.approval_state, ShellApprovalState::Blocked(_)) {
            if let Some(result) = &plan.denied_result {
                return ToolExecution {
                    result: result.clone(),
                    events: plan.events.clone(),
                };
            }
        }

        let result = match (&plan.approval_state, approval) {
            (ShellApprovalState::NotRequired, _) => self.run_plan(plan),
            (
                ShellApprovalState::Pending(_),
                Some(PermissionReply::Once | PermissionReply::Always),
            ) => self.run_plan(plan),
            (ShellApprovalState::Pending(_), Some(PermissionReply::Reject)) => denied_result(
                plan.tool_call_id,
                "permission_rejected",
                "shell command was rejected by permission reply",
            ),
            (ShellApprovalState::Pending(_), None) => denied_result(
                plan.tool_call_id,
                "permission_required",
                "shell command requires approval before execution",
            ),
            (ShellApprovalState::Blocked(reason), _) => denied_result(
                plan.tool_call_id,
                "command_blocked",
                format!("command blocked by guard: {reason:?}"),
            ),
        };

        execution(result)
    }

    fn run_plan(&self, plan: &ShellPlan) -> ToolResult {
        let started_at = unix_ms_now();
        let sandbox_selection = shell_sandbox_provider_selection_for_config(self.config);
        if !shell_sandbox_policy_satisfied(self.config.sandbox_policy, &sandbox_selection.provider)
        {
            return denied_shell_policy_result(
                plan.tool_call_id,
                "sandbox_unavailable",
                "shell sandbox policy requires active kernel isolation, but no supported kernel sandbox provider is available",
                shell_policy_metadata(
                    plan,
                    "sandbox_unavailable",
                    self.config.resource_limits,
                    self.config.network_policy,
                    self.config.sandbox_policy,
                    &sandbox_selection,
                ),
                started_at,
                unix_ms_now(),
            );
        }

        if self.config.network_policy == ShellNetworkPolicy::Disabled
            && command_uses_network(&plan.normalized_command)
        {
            return denied_shell_policy_result(
                plan.tool_call_id,
                "network_disabled",
                "shell command requires network access, but shell network policy is disabled",
                shell_policy_metadata(
                    plan,
                    "network_disabled",
                    self.config.resource_limits,
                    self.config.network_policy,
                    self.config.sandbox_policy,
                    &sandbox_selection,
                ),
                started_at,
                unix_ms_now(),
            );
        }

        let result = self.run_shell(plan, &sandbox_selection);
        let completed_at = unix_ms_now();

        match result {
            Ok(output) => {
                ToolResult::succeeded(plan.tool_call_id, output, started_at, completed_at)
            }
            Err(error) => ToolResult::failed(plan.tool_call_id, error, started_at, completed_at),
        }
    }

    fn run_shell(
        &self,
        plan: &ShellPlan,
        sandbox_selection: &ShellSandboxProviderSelection,
    ) -> Result<ToolOutput, ToolError> {
        let cwd = self
            .workspace
            .resolve_existing_path(&plan.cwd)
            .map_err(AgentError::into_tool_error)?;
        if !cwd.is_dir() {
            return Err(ToolError::new(
                "not_directory",
                format!("shell cwd is not a directory: {}", plan.cwd),
                false,
            ));
        }

        let started = Instant::now();
        let mut command = shell_command(plan, &cwd, self.config, &sandbox_selection.provider);
        configure_shell_command(&mut command, self.config);

        let mut child = command.spawn().map_err(|source| {
            ToolError::new(
                "command_spawn_failed",
                format!("failed to spawn shell command: {source}"),
                true,
            )
        })?;

        let stdout = child
            .stdout
            .take()
            .map(|reader| spawn_limited_reader(reader, self.config.output_limit_bytes));
        let stderr = child
            .stderr
            .take()
            .map(|reader| spawn_limited_reader(reader, self.config.output_limit_bytes));

        let timeout = Duration::from_millis(plan.timeout_ms);
        let poll_interval = Duration::from_millis(self.config.poll_interval_ms);
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|source| {
                ToolError::new(
                    "command_wait_failed",
                    format!("failed to wait for shell command: {source}"),
                    true,
                )
            })? {
                break status;
            }

            if started.elapsed() >= timeout {
                timed_out = true;
                terminate_shell_child(&mut child);
                break child.wait().map_err(|source| {
                    ToolError::new(
                        "command_wait_failed",
                        format!("failed to wait for timed out shell command: {source}"),
                        true,
                    )
                })?;
            }

            thread::sleep(poll_interval.min(timeout.saturating_sub(started.elapsed())));
        };

        if timed_out {
            // Avoid blocking on pipe reader threads if a descendant process keeps
            // stdout/stderr open after the shell process is killed.
            return Err(ToolError::new(
                "command_timeout",
                format!("shell command timed out after {} ms", plan.timeout_ms),
                true,
            ));
        }

        let stdout = join_limited_output(stdout);
        let stderr = join_limited_output(stderr);
        let stdout_text = redact_shell_output(&stdout.text);
        let stderr_text = redact_shell_output(&stderr.text);
        let duration_ms = started.elapsed().as_millis() as u64;

        Ok(ToolOutput {
            text: shell_text(&stdout_text, &stderr_text),
            metadata: json!({
                "command": redact_shell_output(&plan.normalized_command),
                "cwd": plan.cwd,
                "exit_code": status.code(),
                "success": status.success(),
                "duration_ms": duration_ms,
                "stdout": stdout_text,
                "stderr": stderr_text,
                "stdout_truncated": stdout.truncated,
                "stderr_truncated": stderr.truncated,
                "resource_limits": shell_resource_limits_metadata(self.config.resource_limits),
                "network_policy": shell_network_policy_label(self.config.network_policy),
                "sandbox_policy": shell_sandbox_policy_label(self.config.sandbox_policy),
                "sandbox": shell_sandbox_metadata(
                    self.config.network_policy,
                    self.config.sandbox_policy,
                    sandbox_selection,
                ),
            }),
            truncated: stdout.truncated || stderr.truncated,
        })
    }
}

fn shell_command(
    plan: &ShellPlan,
    cwd: &Path,
    config: ShellExecutionConfig,
    sandbox_provider: &ShellSandboxProvider,
) -> Command {
    match sandbox_provider {
        ShellSandboxProvider::Process => {
            let mut command = Command::new("/bin/sh");
            command
                .arg("-lc")
                .arg(&plan.normalized_command)
                .current_dir(cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command
        }
        ShellSandboxProvider::Bubblewrap { executable } => {
            let mut command = Command::new(executable);
            command.arg("--die-with-parent").arg("--unshare-all");
            if config.network_policy == ShellNetworkPolicy::Allowed {
                command.arg("--share-net");
            }
            command
                .arg("--ro-bind")
                .arg("/")
                .arg("/")
                .arg("--bind")
                .arg(cwd)
                .arg(cwd)
                .arg("--dev")
                .arg("/dev")
                .arg("--proc")
                .arg("/proc")
                .arg("--tmpfs")
                .arg("/tmp")
                .arg("--chdir")
                .arg(cwd)
                .arg("--setenv")
                .arg("PATH")
                .arg(current_sanitized_shell_path())
                .arg("--")
                .arg("/bin/sh")
                .arg("-lc")
                .arg(&plan.normalized_command)
                .current_dir(cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command
        }
        ShellSandboxProvider::Unshare { executable } => {
            let mut command = Command::new(executable);
            command
                .arg("--user")
                .arg("--map-root-user")
                .arg("--mount")
                .arg("--pid")
                .arg("--fork");
            if config.network_policy == ShellNetworkPolicy::Disabled {
                command.arg("--net");
            }
            command
                .arg("--")
                .arg("/bin/sh")
                .arg("-lc")
                .arg(&plan.normalized_command)
                .current_dir(cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command
        }
    }
}

fn configure_shell_command(command: &mut Command, config: ShellExecutionConfig) {
    command.env_clear();
    for (key, value) in sanitized_shell_environment(env::vars()) {
        command.env(key, value);
    }

    #[cfg(unix)]
    {
        command.process_group(0);
        // SAFETY: the pre-exec closure only calls async-signal-safe libc
        // operations through apply_unix_shell_pre_exec_policy.
        unsafe {
            command.pre_exec(move || apply_unix_shell_pre_exec_policy(config));
        }
    }
}

fn validate_resource_limits(limits: ShellResourceLimits) -> Result<(), AgentError> {
    for (name, value) in [
        ("shell max CPU time", limits.max_cpu_time_seconds),
        ("shell max file size", limits.max_file_size_bytes),
        ("shell max virtual memory", limits.max_virtual_memory_bytes),
        ("shell max open files", limits.max_open_files),
    ] {
        if value == Some(0) {
            return Err(AgentError::InvalidInput(format!(
                "{name} must be greater than zero"
            )));
        }
    }

    Ok(())
}

#[cfg(unix)]
fn apply_unix_shell_pre_exec_policy(config: ShellExecutionConfig) -> io::Result<()> {
    apply_unix_resource_limits(config.resource_limits)?;
    disable_unix_core_dumps()?;
    apply_private_umask();
    apply_no_new_privileges()?;
    Ok(())
}

#[cfg(unix)]
fn apply_unix_resource_limits(limits: ShellResourceLimits) -> io::Result<()> {
    set_unix_resource_limit(libc::RLIMIT_CPU, limits.max_cpu_time_seconds)?;
    set_unix_resource_limit(libc::RLIMIT_FSIZE, limits.max_file_size_bytes)?;
    set_unix_resource_limit(libc::RLIMIT_AS, limits.max_virtual_memory_bytes)?;
    set_unix_resource_limit(libc::RLIMIT_NOFILE, limits.max_open_files)?;
    Ok(())
}

#[cfg(unix)]
fn set_unix_resource_limit(
    resource: libc::__rlimit_resource_t,
    limit: Option<u64>,
) -> io::Result<()> {
    let Some(limit) = limit else {
        return Ok(());
    };
    let value = limit as libc::rlim_t;
    let rlimit = libc::rlimit {
        rlim_cur: value,
        rlim_max: value,
    };
    let status = unsafe { libc::setrlimit(resource, &rlimit) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn disable_unix_core_dumps() -> io::Result<()> {
    set_unix_resource_limit(libc::RLIMIT_CORE, Some(0))
}

#[cfg(unix)]
fn apply_private_umask() {
    unsafe {
        libc::umask(DEFAULT_SHELL_UMASK);
    }
}

#[cfg(target_os = "linux")]
fn apply_no_new_privileges() -> io::Result<()> {
    let status = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
fn apply_no_new_privileges() -> io::Result<()> {
    Ok(())
}

fn shell_resource_limits_metadata(limits: ShellResourceLimits) -> serde_json::Value {
    json!({
        "cpu_time_seconds": limits.max_cpu_time_seconds,
        "file_size_bytes": limits.max_file_size_bytes,
        "virtual_memory_bytes": limits.max_virtual_memory_bytes,
        "open_files": limits.max_open_files,
    })
}

fn shell_network_policy_label(policy: ShellNetworkPolicy) -> &'static str {
    match policy {
        ShellNetworkPolicy::Disabled => "disabled",
        ShellNetworkPolicy::Allowed => "allowed",
    }
}

fn shell_sandbox_policy_label(policy: ShellSandboxPolicy) -> &'static str {
    match policy {
        ShellSandboxPolicy::Process => "process",
        ShellSandboxPolicy::RequireKernelIsolation => "require-kernel-isolation",
    }
}

fn shell_sandbox_policy_satisfied(
    policy: ShellSandboxPolicy,
    provider: &ShellSandboxProvider,
) -> bool {
    match policy {
        ShellSandboxPolicy::Process => true,
        ShellSandboxPolicy::RequireKernelIsolation => provider.kernel_isolation_active(),
    }
}

fn shell_sandbox_metadata(
    network_policy: ShellNetworkPolicy,
    sandbox_policy: ShellSandboxPolicy,
    selection: &ShellSandboxProviderSelection,
) -> serde_json::Value {
    shell_sandbox_metadata_with_selection(network_policy, sandbox_policy, selection)
}

fn shell_sandbox_metadata_with_selection(
    network_policy: ShellNetworkPolicy,
    sandbox_policy: ShellSandboxPolicy,
    selection: &ShellSandboxProviderSelection,
) -> serde_json::Value {
    let provider = &selection.provider;
    json!({
        "profile": shell_sandbox_profile_label(provider),
        "policy": shell_sandbox_policy_label(sandbox_policy),
        "no_new_privileges": cfg!(target_os = "linux"),
        "process_group": cfg!(unix),
        "core_dumps_disabled": cfg!(unix),
        "private_umask": cfg!(unix),
        "umask": shell_sandbox_umask_label(),
        "network_isolation": shell_sandbox_network_isolation_label(network_policy, provider),
        "filesystem_isolation": shell_sandbox_filesystem_isolation_label(provider),
        "namespace_isolation": shell_sandbox_namespace_isolation_label(provider),
        "seccomp": false,
        "providers": shell_sandbox_provider_metadata(selection),
    })
}

fn shell_sandbox_provider_selection_for_config(
    config: ShellExecutionConfig,
) -> ShellSandboxProviderSelection {
    match config.sandbox_provider_discovery {
        ShellSandboxProviderDiscovery::Disabled => {
            ShellSandboxProviderSelection::without_probe_diagnostics(
                ShellSandboxProvider::Process,
                &current_sanitized_shell_path(),
            )
        }
        ShellSandboxProviderDiscovery::Auto => {
            shell_sandbox_provider_selection_for_policy_from_path(
                config.sandbox_policy,
                &current_sanitized_shell_path(),
            )
        }
    }
}

fn shell_sandbox_provider_selection_for_policy_from_path(
    policy: ShellSandboxPolicy,
    path: &str,
) -> ShellSandboxProviderSelection {
    let mut diagnostics = ShellSandboxProviderDiagnostics::from_path(path);

    match policy {
        ShellSandboxPolicy::Process => ShellSandboxProviderSelection {
            provider: ShellSandboxProvider::Process,
            diagnostics,
        },
        ShellSandboxPolicy::RequireKernelIsolation => {
            if cfg!(target_os = "linux") {
                if let Some(executable) = find_executable_in_path("bwrap", path) {
                    let probe_succeeded = bubblewrap_probe_succeeds(&executable);
                    diagnostics.bubblewrap.probe_succeeded = Some(probe_succeeded);
                    if probe_succeeded {
                        return ShellSandboxProviderSelection {
                            provider: ShellSandboxProvider::Bubblewrap { executable },
                            diagnostics,
                        };
                    }
                }

                if let Some(executable) = find_executable_in_path("unshare", path) {
                    let probe_succeeded = unshare_probe_succeeds(&executable);
                    diagnostics.unshare.probe_succeeded = Some(probe_succeeded);
                    if probe_succeeded {
                        return ShellSandboxProviderSelection {
                            provider: ShellSandboxProvider::Unshare { executable },
                            diagnostics,
                        };
                    }
                }
            }

            ShellSandboxProviderSelection {
                provider: ShellSandboxProvider::Process,
                diagnostics,
            }
        }
    }
}

fn shell_sandbox_provider_metadata(selection: &ShellSandboxProviderSelection) -> serde_json::Value {
    let diagnostics = selection.diagnostics;
    json!({
        "selected": selection.provider.label(),
        "kernel_isolation_active": selection.provider.kernel_isolation_active(),
        "bubblewrap_available": diagnostics.bubblewrap.candidate_available,
        "unshare_available": diagnostics.unshare.candidate_available,
        "bubblewrap": shell_sandbox_probe_diagnostics_metadata(diagnostics.bubblewrap),
        "unshare": shell_sandbox_probe_diagnostics_metadata(diagnostics.unshare),
    })
}

#[cfg(test)]
fn shell_sandbox_provider_metadata_from_path(path: &str) -> serde_json::Value {
    shell_sandbox_provider_metadata_with_path(&ShellSandboxProvider::Process, path)
}

#[cfg(test)]
fn shell_sandbox_provider_metadata_with_path(
    provider: &ShellSandboxProvider,
    path: &str,
) -> serde_json::Value {
    let selection =
        ShellSandboxProviderSelection::without_probe_diagnostics(provider.clone(), path);
    shell_sandbox_provider_metadata(&selection)
}

fn shell_sandbox_probe_diagnostics_metadata(
    diagnostics: ShellSandboxProviderProbeDiagnostics,
) -> serde_json::Value {
    json!({
        "candidate_available": diagnostics.candidate_available,
        "probe_succeeded": diagnostics.probe_succeeded,
    })
}

fn current_sanitized_shell_path() -> String {
    env::var("PATH")
        .map(|path| sanitize_shell_path(&path))
        .unwrap_or_else(|_| DEFAULT_SHELL_PATH.to_string())
}

fn executable_exists_in_path(command_name: &str, path: &str) -> bool {
    find_executable_in_path(command_name, path).is_some()
}

fn find_executable_in_path(command_name: &str, path: &str) -> Option<PathBuf> {
    path.split(':')
        .filter(|part| !part.is_empty())
        .map(|part| Path::new(part).join(command_name))
        .find(|candidate| is_executable_file(candidate))
}

fn bubblewrap_probe_succeeds(executable: &Path) -> bool {
    let mut command = Command::new(executable);
    command
        .arg("--die-with-parent")
        .arg("--unshare-all")
        .arg("--ro-bind")
        .arg("/")
        .arg("/")
        .arg("--dev")
        .arg("/dev")
        .arg("--proc")
        .arg("/proc")
        .arg("--tmpfs")
        .arg("/tmp")
        .arg("--")
        .arg("/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    command_status_succeeds_with_timeout(command, SANDBOX_PROVIDER_PROBE_TIMEOUT_MS)
}

fn unshare_probe_succeeds(executable: &Path) -> bool {
    let mut command = Command::new(executable);
    command
        .arg("--user")
        .arg("--map-root-user")
        .arg("--mount")
        .arg("--pid")
        .arg("--fork")
        .arg("--net")
        .arg("--")
        .arg("/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    command_status_succeeds_with_timeout(command, SANDBOX_PROVIDER_PROBE_TIMEOUT_MS)
}

fn command_status_succeeds_with_timeout(mut command: Command, timeout_ms: u64) -> bool {
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    let started = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn shell_sandbox_network_isolation_label(
    policy: ShellNetworkPolicy,
    provider: &ShellSandboxProvider,
) -> &'static str {
    match (policy, provider.kernel_isolation_active()) {
        (ShellNetworkPolicy::Disabled, true) => "namespace",
        (ShellNetworkPolicy::Disabled, false) => "command-policy",
        (ShellNetworkPolicy::Allowed, _) => "none",
    }
}

fn shell_sandbox_umask_label() -> &'static str {
    if cfg!(unix) {
        "077"
    } else {
        "inherited"
    }
}

fn shell_sandbox_profile_label(provider: &ShellSandboxProvider) -> &'static str {
    if matches!(provider, ShellSandboxProvider::Bubblewrap { .. }) {
        "linux-bubblewrap"
    } else if matches!(provider, ShellSandboxProvider::Unshare { .. }) {
        "linux-unshare"
    } else if cfg!(target_os = "linux") {
        "linux-no-new-privileges"
    } else if cfg!(unix) {
        "unix-process-group"
    } else {
        "process"
    }
}

fn shell_sandbox_filesystem_isolation_label(provider: &ShellSandboxProvider) -> &'static str {
    if matches!(provider, ShellSandboxProvider::Bubblewrap { .. }) {
        "host-readonly-workspace-write"
    } else {
        "none"
    }
}

fn shell_sandbox_namespace_isolation_label(provider: &ShellSandboxProvider) -> &'static str {
    if matches!(provider, ShellSandboxProvider::Bubblewrap { .. }) {
        "bubblewrap"
    } else if matches!(provider, ShellSandboxProvider::Unshare { .. }) {
        "unshare"
    } else {
        "none"
    }
}

impl ShellSandboxProvider {
    fn label(&self) -> &'static str {
        match self {
            ShellSandboxProvider::Process => "process",
            ShellSandboxProvider::Bubblewrap { .. } => "bubblewrap",
            ShellSandboxProvider::Unshare { .. } => "unshare",
        }
    }

    fn kernel_isolation_active(&self) -> bool {
        matches!(
            self,
            ShellSandboxProvider::Bubblewrap { .. } | ShellSandboxProvider::Unshare { .. }
        )
    }
}

fn shell_policy_metadata(
    plan: &ShellPlan,
    policy: &str,
    resource_limits: ShellResourceLimits,
    network_policy: ShellNetworkPolicy,
    sandbox_policy: ShellSandboxPolicy,
    sandbox_selection: &ShellSandboxProviderSelection,
) -> serde_json::Value {
    json!({
        "policy": policy,
        "command": redact_shell_output(&plan.normalized_command),
        "cwd": plan.cwd,
        "resource_limits": shell_resource_limits_metadata(resource_limits),
        "network_policy": shell_network_policy_label(network_policy),
        "sandbox_policy": shell_sandbox_policy_label(sandbox_policy),
        "sandbox": shell_sandbox_metadata(network_policy, sandbox_policy, sandbox_selection),
    })
}

fn sanitized_shell_environment<I>(vars: I) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut sanitized = Vec::new();
    let mut seen = HashSet::new();

    for (key, value) in vars {
        if !shell_env_key_allowed(&key) || shell_env_key_sensitive(&key) {
            continue;
        }

        let value = if key == "PATH" {
            sanitize_shell_path(&value)
        } else {
            value
        };
        if value.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        sanitized.push((key, value));
    }

    if !seen.contains("PATH") {
        sanitized.push(("PATH".to_string(), DEFAULT_SHELL_PATH.to_string()));
    }

    sanitized
}

fn shell_env_key_allowed(key: &str) -> bool {
    SHELL_ENV_ALLOWLIST.contains(&key) || key.starts_with("LC_")
}

fn shell_env_key_sensitive(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    [
        "auth",
        "credential",
        "cookie",
        "key",
        "password",
        "secret",
        "session",
        "token",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn sanitize_shell_path(path: &str) -> String {
    let sanitized = path
        .split(':')
        .filter(|part| !part.is_empty() && Path::new(part).is_absolute())
        .collect::<Vec<_>>()
        .join(":");

    if sanitized.is_empty() {
        DEFAULT_SHELL_PATH.to_string()
    } else {
        sanitized
    }
}

fn terminate_shell_child(child: &mut Child) {
    #[cfg(unix)]
    {
        terminate_unix_process_group(child.id(), "TERM");
        thread::sleep(Duration::from_millis(20));
        terminate_unix_process_group(child.id(), "KILL");
    }

    let _ = child.kill();
}

#[cfg(unix)]
fn terminate_unix_process_group(pid: u32, signal: &str) {
    let target = format!("-{pid}");
    let _ = Command::new("/bin/kill")
        .arg(format!("-{signal}"))
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn redact_shell_output(text: &str) -> String {
    text.split('\n')
        .map(redact_shell_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_shell_line(line: &str) -> String {
    if let Some(redacted) = redact_sensitive_key_value_line(line) {
        return redacted;
    }

    let bearer_redacted = redact_bearer_tokens(line);
    if bearer_redacted != line {
        return redact_prefixed_secret_tokens(&bearer_redacted);
    }

    redact_prefixed_secret_tokens(line)
}

fn redact_sensitive_key_value_line(line: &str) -> Option<String> {
    for separator in ['=', ':'] {
        let Some((left, _right)) = line.split_once(separator) else {
            continue;
        };
        let normalized_key = left.to_ascii_lowercase();
        if [
            "api_key",
            "apikey",
            "credential",
            "password",
            "secret",
            "token",
        ]
        .iter()
        .any(|marker| normalized_key.contains(marker))
        {
            return Some(format!("{}{separator}[REDACTED]", left.trim_end()));
        }
    }

    None
}

fn redact_bearer_tokens(line: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    let lower = line.to_ascii_lowercase();

    while let Some(relative_start) = lower[cursor..].find("bearer ") {
        let start = cursor + relative_start;
        let token_start = start + "bearer ".len();
        let token_end = secret_token_end(line, token_start);
        output.push_str(&line[cursor..start]);
        output.push_str("Bearer [REDACTED]");
        cursor = token_end;
    }

    output.push_str(&line[cursor..]);
    output
}

fn redact_prefixed_secret_tokens(line: &str) -> String {
    [
        ("sk-or-", "sk-or-[REDACTED]"),
        ("sk-", "sk-[REDACTED]"),
        ("nvapi-", "nvapi-[REDACTED]"),
        ("ya29.", "ya29.[REDACTED]"),
    ]
    .into_iter()
    .fold(line.to_string(), |redacted, (prefix, replacement)| {
        redact_prefixed_token(&redacted, prefix, replacement)
    })
}

fn redact_prefixed_token(text: &str, prefix: &str, replacement: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;

    while let Some(relative_start) = text[cursor..].find(prefix) {
        let start = cursor + relative_start;
        let token_end = secret_token_end(text, start);
        output.push_str(&text[cursor..start]);
        output.push_str(replacement);
        cursor = token_end;
    }

    output.push_str(&text[cursor..]);
    output
}

fn secret_token_end(text: &str, start: usize) -> usize {
    text[start..]
        .char_indices()
        .find(|(_, character)| !secret_token_character(*character))
        .map(|(offset, _)| start + offset)
        .unwrap_or(text.len())
}

fn secret_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LimitedOutput {
    text: String,
    truncated: bool,
}

fn spawn_limited_reader<R>(mut reader: R, limit: usize) -> thread::JoinHandle<LimitedOutput>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut stored = Vec::new();
        let mut buffer = [0_u8; 8192];
        let mut truncated = false;

        loop {
            let read = reader.read(&mut buffer).unwrap_or_default();
            if read == 0 {
                break;
            }

            let available = limit.saturating_sub(stored.len());
            if available > 0 {
                let to_store = available.min(read);
                stored.extend_from_slice(&buffer[..to_store]);
                truncated |= to_store < read;
            } else {
                truncated = true;
            }
        }

        LimitedOutput {
            text: String::from_utf8_lossy(&stored).to_string(),
            truncated,
        }
    })
}

fn join_limited_output(handle: Option<thread::JoinHandle<LimitedOutput>>) -> LimitedOutput {
    handle
        .map(|handle| {
            handle.join().unwrap_or_else(|_| LimitedOutput {
                text: String::new(),
                truncated: true,
            })
        })
        .unwrap_or_else(|| LimitedOutput {
            text: String::new(),
            truncated: false,
        })
}

fn shell_text(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (false, false) => format!("stdout:\n{stdout}\nstderr:\n{stderr}"),
        (true, true) => String::new(),
    }
}

fn execution(result: ToolResult) -> ToolExecution {
    let events = vec![
        ReplEvent::ToolStarted {
            name: SHELL_RUN_TOOL.to_string(),
        },
        ReplEvent::ToolCompleted {
            name: SHELL_RUN_TOOL.to_string(),
            status: tool_status(result.status),
        },
    ];
    ToolExecution { result, events }
}

fn denied_result(call_id: uuid::Uuid, code: &str, message: impl Into<String>) -> ToolResult {
    let now = unix_ms_now();
    ToolResult::denied(call_id, ToolError::new(code, message, false), now, now)
}

fn denied_shell_policy_result(
    call_id: uuid::Uuid,
    code: &str,
    message: &str,
    metadata: serde_json::Value,
    started_at: u64,
    completed_at: u64,
) -> ToolResult {
    ToolResult {
        call_id,
        status: ToolResultStatus::Denied,
        output: Some(ToolOutput {
            text: message.to_string(),
            metadata,
            truncated: false,
        }),
        error: Some(ToolError::new(code, message, false)),
        started_at_unix_ms: started_at,
        completed_at_unix_ms: completed_at,
    }
}

fn tool_status(status: ToolResultStatus) -> ToolStatus {
    match status {
        ToolResultStatus::Succeeded => ToolStatus::Succeeded,
        ToolResultStatus::Failed => ToolStatus::Failed,
        ToolResultStatus::Cancelled => ToolStatus::Cancelled,
        ToolResultStatus::Denied => ToolStatus::Denied,
    }
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use coddy_core::PermissionReply;
    use uuid::Uuid;

    use crate::{ShellPlanRequest, ShellPlanner};

    use super::*;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("coddy-shell-exec-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp workspace");
            Self { path }
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn request(command: &str) -> ShellPlanRequest {
        ShellPlanRequest {
            session_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            tool_call_id: Some(Uuid::new_v4()),
            command: command.to_string(),
            description: Some("test command".to_string()),
            cwd: None,
            timeout_ms: Some(1_000),
            requested_at_unix_ms: 1_775_000_000_000,
        }
    }

    #[test]
    fn executes_read_only_plan_without_explicit_approval() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("pwd")).expect("plan");

        let execution = executor.execute(&plan, None);

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert!(output
            .text
            .contains(workspace.path.to_string_lossy().as_ref()));
        assert_eq!(output.metadata["cwd"], json!("."));
    }

    #[test]
    fn executes_pending_plan_only_after_approval() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("printf coddy")).expect("plan");

        let denied = executor.execute(&plan, None);
        assert_eq!(denied.result.status, ToolResultStatus::Denied);
        assert_eq!(
            denied.result.error.expect("denied error").code,
            "permission_required"
        );

        let approved = executor.execute(&plan, Some(PermissionReply::Once));
        assert_eq!(approved.result.status, ToolResultStatus::Succeeded);
        let output = approved.result.output.expect("output");
        assert_eq!(output.metadata["stdout"], json!("coddy"));
        assert_eq!(output.metadata["success"], json!(true));
    }

    #[test]
    fn rejects_pending_plan_when_permission_is_rejected() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("printf coddy")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Reject));

        assert_eq!(execution.result.status, ToolResultStatus::Denied);
        assert_eq!(
            execution.result.error.expect("denied error").code,
            "permission_rejected"
        );
    }

    #[test]
    fn denies_network_commands_when_network_policy_is_disabled() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("curl --version")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Denied);
        assert_eq!(
            execution
                .result
                .error
                .as_ref()
                .expect("network policy error")
                .code,
            "network_disabled"
        );
        let output = execution.result.output.expect("policy output");
        assert_eq!(
            output.text,
            "shell command requires network access, but shell network policy is disabled"
        );
        assert_eq!(output.metadata["policy"], json!("network_disabled"));
        assert_eq!(output.metadata["command"], json!("curl --version"));
        assert_eq!(output.metadata["cwd"], json!("."));
        assert_eq!(output.metadata["network_policy"], json!("disabled"));
        assert_eq!(output.metadata["sandbox_policy"], json!("process"));
        assert_eq!(
            output.metadata["sandbox"]["profile"],
            json!(shell_sandbox_profile_label(&ShellSandboxProvider::Process))
        );
        assert_eq!(output.metadata["sandbox"]["policy"], json!("process"));
        assert_eq!(
            output.metadata["sandbox"]["network_isolation"],
            json!("command-policy")
        );
        assert_eq!(
            output.metadata["sandbox"]["filesystem_isolation"],
            json!("none")
        );
        assert!(output.metadata.get("stdout").is_none());
        assert!(output.metadata.get("stderr").is_none());
    }

    #[test]
    fn requires_kernel_shell_sandbox_policy_denies_without_active_kernel_isolation() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::with_config(
            &workspace.path,
            ShellExecutionConfig {
                sandbox_policy: ShellSandboxPolicy::RequireKernelIsolation,
                sandbox_provider_discovery: ShellSandboxProviderDiscovery::Disabled,
                ..ShellExecutionConfig::default()
            },
        )
        .expect("executor");
        let plan = planner.plan(request("printf coddy")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Denied);
        assert_eq!(
            execution
                .result
                .error
                .as_ref()
                .expect("sandbox policy error")
                .code,
            "sandbox_unavailable"
        );
        let output = execution.result.output.expect("policy output");
        assert_eq!(output.metadata["policy"], json!("sandbox_unavailable"));
        assert_eq!(
            output.metadata["sandbox_policy"],
            json!("require-kernel-isolation")
        );
        assert_eq!(
            output.metadata["sandbox"]["providers"]["selected"],
            json!("process")
        );
        assert_eq!(
            output.metadata["sandbox"]["providers"]["kernel_isolation_active"],
            json!(false)
        );
        assert!(output.metadata.get("stdout").is_none());
        assert!(output.metadata.get("stderr").is_none());
    }

    #[test]
    fn shell_command_network_detection_covers_package_and_vcs_commands() {
        for command in [
            "curl --version",
            "git fetch origin",
            "npm ci",
            "python -m pip install pytest",
        ] {
            assert!(command_uses_network(command), "{command}");
        }

        for command in ["npm test", "cargo test --workspace", "git status --short"] {
            assert!(!command_uses_network(command), "{command}");
        }
    }

    #[test]
    fn preserves_blocked_plan_denial_without_running_command() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("rm -rf target")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Always));

        assert_eq!(execution.result.status, ToolResultStatus::Denied);
        assert_eq!(
            execution.result.error.expect("blocked error").code,
            "command_blocked"
        );
    }

    #[test]
    fn truncates_large_output() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::with_config(
            &workspace.path,
            ShellExecutionConfig {
                output_limit_bytes: 4,
                poll_interval_ms: 10,
                ..ShellExecutionConfig::default()
            },
        )
        .expect("executor");
        let plan = planner.plan(request("printf 1234567890")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert_eq!(output.metadata["stdout"], json!("1234"));
        assert!(output.truncated);
    }

    #[test]
    fn fails_when_command_times_out() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::with_config(
            &workspace.path,
            ShellExecutionConfig {
                output_limit_bytes: DEFAULT_SHELL_OUTPUT_LIMIT_BYTES,
                poll_interval_ms: 5,
                ..ShellExecutionConfig::default()
            },
        )
        .expect("executor");
        let mut timeout_request = request("while true; do :; done");
        timeout_request.timeout_ms = Some(20);
        let plan = planner.plan(timeout_request).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Failed);
        assert_eq!(
            execution.result.error.expect("timeout error").code,
            "command_timeout"
        );
    }

    #[test]
    fn does_not_expose_parent_secret_environment_to_shell_commands() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let env_name = format!("CODDY_AGENT_SHELL_SECRET_{}", Uuid::new_v4().simple());
        std::env::set_var(&env_name, "should-not-leak");
        let plan = planner
            .plan(request(&format!(
                "if test -n \"${env_name}\"; then printf leaked; else printf clean; fi"
            )))
            .expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));
        std::env::remove_var(&env_name);

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert_eq!(output.metadata["stdout"], json!("clean"));
    }

    #[test]
    fn sanitized_shell_environment_drops_sensitive_keys() {
        let env = sanitized_shell_environment([
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            ("HOME".to_string(), "/home/coddy".to_string()),
            ("OPENAI_API_KEY".to_string(), "sk-secret".to_string()),
            (
                "CODDY_EPHEMERAL_MODEL_CREDENTIAL".to_string(),
                "{\"token\":\"secret\"}".to_string(),
            ),
        ]);

        assert!(env
            .iter()
            .any(|(key, value)| key == "PATH" && value == "/usr/bin:/bin"));
        assert!(env
            .iter()
            .any(|(key, value)| key == "HOME" && value == "/home/coddy"));
        assert!(!env.iter().any(|(key, _)| key == "OPENAI_API_KEY"));
        assert!(!env
            .iter()
            .any(|(key, _)| key == "CODDY_EPHEMERAL_MODEL_CREDENTIAL"));
    }

    #[test]
    fn rejects_zero_shell_resource_limits() {
        let workspace = TempWorkspace::new();
        let result = ShellExecutor::with_config(
            &workspace.path,
            ShellExecutionConfig {
                resource_limits: ShellResourceLimits {
                    max_cpu_time_seconds: Some(0),
                    ..ShellResourceLimits::unrestricted()
                },
                ..ShellExecutionConfig::default()
            },
        );

        assert!(
            matches!(result, Err(AgentError::InvalidInput(message)) if message.contains("CPU"))
        );
    }

    #[test]
    fn shell_output_metadata_includes_resource_limits() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("printf coddy")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert_eq!(
            output.metadata["resource_limits"]["cpu_time_seconds"],
            json!(DEFAULT_SHELL_MAX_CPU_TIME_SECONDS)
        );
        assert_eq!(
            output.metadata["resource_limits"]["file_size_bytes"],
            json!(DEFAULT_SHELL_MAX_FILE_SIZE_BYTES)
        );
        assert_eq!(output.metadata["network_policy"], json!("disabled"));
        assert_eq!(output.metadata["sandbox_policy"], json!("process"));
        assert_eq!(
            output.metadata["sandbox"]["profile"],
            json!(shell_sandbox_profile_label(&ShellSandboxProvider::Process))
        );
        assert_eq!(output.metadata["sandbox"]["policy"], json!("process"));
        assert_eq!(
            output.metadata["sandbox"]["no_new_privileges"],
            json!(cfg!(target_os = "linux"))
        );
        assert_eq!(
            output.metadata["sandbox"]["process_group"],
            json!(cfg!(unix))
        );
        assert_eq!(
            output.metadata["sandbox"]["core_dumps_disabled"],
            json!(cfg!(unix))
        );
        assert_eq!(
            output.metadata["sandbox"]["private_umask"],
            json!(cfg!(unix))
        );
        assert_eq!(
            output.metadata["sandbox"]["umask"],
            json!(if cfg!(unix) { "077" } else { "inherited" })
        );
        assert_eq!(
            output.metadata["sandbox"]["network_isolation"],
            json!("command-policy")
        );
        assert_eq!(
            output.metadata["sandbox"]["filesystem_isolation"],
            json!("none")
        );
        assert_eq!(
            output.metadata["sandbox"]["namespace_isolation"],
            json!("none")
        );
        assert_eq!(output.metadata["sandbox"]["seccomp"], json!(false));
        assert_eq!(
            output.metadata["sandbox"]["providers"]["selected"],
            json!("process")
        );
        assert_eq!(
            output.metadata["sandbox"]["providers"]["kernel_isolation_active"],
            json!(false)
        );
    }

    #[test]
    fn shell_sandbox_metadata_marks_network_unisolated_when_network_policy_allows_network() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::with_config(
            &workspace.path,
            ShellExecutionConfig {
                network_policy: ShellNetworkPolicy::Allowed,
                ..ShellExecutionConfig::default()
            },
        )
        .expect("executor");
        let plan = planner.plan(request("printf coddy")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert_eq!(output.metadata["network_policy"], json!("allowed"));
        assert_eq!(
            output.metadata["sandbox"]["network_isolation"],
            json!("none")
        );
        assert_eq!(
            output.metadata["sandbox"]["filesystem_isolation"],
            json!("none")
        );
    }

    #[test]
    fn shell_sandbox_provider_metadata_reports_missing_providers_from_empty_path() {
        let metadata = shell_sandbox_provider_metadata_from_path("");

        assert_eq!(metadata["selected"], json!("process"));
        assert_eq!(metadata["kernel_isolation_active"], json!(false));
        assert_eq!(metadata["bubblewrap_available"], json!(false));
        assert_eq!(metadata["unshare_available"], json!(false));
    }

    #[cfg(unix)]
    #[test]
    fn shell_sandbox_provider_metadata_detects_executable_candidates_on_path() {
        use std::os::unix::fs::PermissionsExt;

        let workspace = TempWorkspace::new();
        let bin_dir = workspace.path.join("bin");
        fs::create_dir(&bin_dir).expect("create bin dir");
        for name in ["bwrap", "unshare"] {
            let path = bin_dir.join(name);
            fs::write(&path, "#!/bin/sh\n").expect("write executable");
            let mut permissions = fs::metadata(&path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("chmod executable");
        }

        let metadata =
            shell_sandbox_provider_metadata_from_path(bin_dir.to_str().expect("utf-8 path"));

        assert_eq!(metadata["selected"], json!("process"));
        assert_eq!(metadata["kernel_isolation_active"], json!(false));
        assert_eq!(metadata["bubblewrap_available"], json!(true));
        assert_eq!(
            metadata["unshare_available"],
            json!(cfg!(target_os = "linux"))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn strict_shell_sandbox_policy_selects_bubblewrap_when_available() {
        use std::os::unix::fs::PermissionsExt;

        let workspace = TempWorkspace::new();
        let bin_dir = workspace.path.join("bin");
        fs::create_dir(&bin_dir).expect("create bin dir");
        let bwrap = bin_dir.join("bwrap");
        fs::write(&bwrap, "#!/bin/sh\n").expect("write executable");
        let mut permissions = fs::metadata(&bwrap).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&bwrap, permissions).expect("chmod executable");

        let selection = shell_sandbox_provider_selection_for_policy_from_path(
            ShellSandboxPolicy::RequireKernelIsolation,
            bin_dir.to_str().expect("utf-8 path"),
        );
        let metadata = shell_sandbox_metadata_with_selection(
            ShellNetworkPolicy::Disabled,
            ShellSandboxPolicy::RequireKernelIsolation,
            &selection,
        );

        assert_eq!(metadata["profile"], json!("linux-bubblewrap"));
        assert_eq!(metadata["namespace_isolation"], json!("bubblewrap"));
        assert_eq!(
            metadata["filesystem_isolation"],
            json!("host-readonly-workspace-write")
        );
        assert_eq!(metadata["network_isolation"], json!("namespace"));
        assert_eq!(metadata["providers"]["selected"], json!("bubblewrap"));
        assert_eq!(
            metadata["providers"]["kernel_isolation_active"],
            json!(true)
        );
        assert_eq!(
            metadata["providers"]["bubblewrap"]["candidate_available"],
            json!(true)
        );
        assert_eq!(
            metadata["providers"]["bubblewrap"]["probe_succeeded"],
            json!(true)
        );
        assert_eq!(
            metadata["providers"]["unshare"]["candidate_available"],
            json!(false)
        );
        assert!(metadata["providers"]["unshare"]["probe_succeeded"].is_null());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn strict_shell_sandbox_policy_ignores_bubblewrap_when_probe_fails() {
        use std::os::unix::fs::PermissionsExt;

        let workspace = TempWorkspace::new();
        let bin_dir = workspace.path.join("bin");
        fs::create_dir(&bin_dir).expect("create bin dir");
        let bwrap = bin_dir.join("bwrap");
        fs::write(&bwrap, "#!/bin/sh\nexit 1\n").expect("write executable");
        let mut permissions = fs::metadata(&bwrap).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&bwrap, permissions).expect("chmod executable");

        let selection = shell_sandbox_provider_selection_for_policy_from_path(
            ShellSandboxPolicy::RequireKernelIsolation,
            bin_dir.to_str().expect("utf-8 path"),
        );
        let metadata = shell_sandbox_metadata_with_selection(
            ShellNetworkPolicy::Disabled,
            ShellSandboxPolicy::RequireKernelIsolation,
            &selection,
        );

        assert_eq!(metadata["profile"], json!("linux-no-new-privileges"));
        assert_eq!(metadata["namespace_isolation"], json!("none"));
        assert_eq!(metadata["network_isolation"], json!("command-policy"));
        assert_eq!(metadata["providers"]["selected"], json!("process"));
        assert_eq!(
            metadata["providers"]["kernel_isolation_active"],
            json!(false)
        );
        assert_eq!(
            metadata["providers"]["bubblewrap"]["candidate_available"],
            json!(true)
        );
        assert_eq!(
            metadata["providers"]["bubblewrap"]["probe_succeeded"],
            json!(false)
        );
        assert_eq!(
            metadata["providers"]["unshare"]["candidate_available"],
            json!(false)
        );
        assert!(metadata["providers"]["unshare"]["probe_succeeded"].is_null());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn strict_shell_sandbox_policy_falls_back_to_unshare_when_bubblewrap_probe_fails() {
        use std::os::unix::fs::PermissionsExt;

        let workspace = TempWorkspace::new();
        let bin_dir = workspace.path.join("bin");
        fs::create_dir(&bin_dir).expect("create bin dir");
        let bwrap = bin_dir.join("bwrap");
        fs::write(&bwrap, "#!/bin/sh\nexit 1\n").expect("write bwrap executable");
        let unshare = bin_dir.join("unshare");
        fs::write(&unshare, "#!/bin/sh\nexit 0\n").expect("write unshare executable");
        for path in [&bwrap, &unshare] {
            let mut permissions = fs::metadata(path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).expect("chmod executable");
        }

        let selection = shell_sandbox_provider_selection_for_policy_from_path(
            ShellSandboxPolicy::RequireKernelIsolation,
            bin_dir.to_str().expect("utf-8 path"),
        );
        let metadata = shell_sandbox_metadata_with_selection(
            ShellNetworkPolicy::Disabled,
            ShellSandboxPolicy::RequireKernelIsolation,
            &selection,
        );

        assert_eq!(metadata["profile"], json!("linux-unshare"));
        assert_eq!(metadata["namespace_isolation"], json!("unshare"));
        assert_eq!(metadata["filesystem_isolation"], json!("none"));
        assert_eq!(metadata["network_isolation"], json!("namespace"));
        assert_eq!(metadata["providers"]["selected"], json!("unshare"));
        assert_eq!(
            metadata["providers"]["kernel_isolation_active"],
            json!(true)
        );
        assert_eq!(
            metadata["providers"]["bubblewrap"]["candidate_available"],
            json!(true)
        );
        assert_eq!(
            metadata["providers"]["bubblewrap"]["probe_succeeded"],
            json!(false)
        );
        assert_eq!(
            metadata["providers"]["unshare"]["candidate_available"],
            json!(true)
        );
        assert_eq!(
            metadata["providers"]["unshare"]["probe_succeeded"],
            json!(true)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn shell_sandbox_enforces_no_new_privileges_on_linux() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner
            .plan(request("grep '^NoNewPrivs:' /proc/self/status"))
            .expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        let stdout = output.metadata["stdout"].as_str().expect("stdout");
        assert!(
            stdout.split_whitespace().last() == Some("1"),
            "expected NoNewPrivs to be 1, got {stdout:?}"
        );
        assert_eq!(
            output.metadata["sandbox"]["profile"],
            json!("linux-no-new-privileges")
        );
        assert_eq!(output.metadata["sandbox"]["no_new_privileges"], json!(true));
    }

    #[cfg(unix)]
    #[test]
    fn shell_sandbox_disables_core_dumps_on_unix() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("ulimit -c")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert_eq!(output.metadata["stdout"], json!("0\n"));
        assert_eq!(
            output.metadata["sandbox"]["core_dumps_disabled"],
            json!(true)
        );
    }

    #[cfg(unix)]
    #[test]
    fn shell_sandbox_sets_private_umask_on_unix() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner.plan(request("umask")).expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert_eq!(output.metadata["stdout"], json!("0077\n"));
        assert_eq!(output.metadata["sandbox"]["private_umask"], json!(true));
        assert_eq!(output.metadata["sandbox"]["umask"], json!("077"));
    }

    #[cfg(unix)]
    #[test]
    fn enforces_shell_file_size_resource_limit() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::with_config(
            &workspace.path,
            ShellExecutionConfig {
                resource_limits: ShellResourceLimits {
                    max_file_size_bytes: Some(1024),
                    ..ShellResourceLimits::unrestricted()
                },
                ..ShellExecutionConfig::default()
            },
        )
        .expect("executor");
        let plan = planner
            .plan(request("head -c 4096 /dev/zero > limited.bin"))
            .expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert_eq!(output.metadata["success"], json!(false));
        assert_eq!(
            output.metadata["resource_limits"]["file_size_bytes"],
            json!(1024)
        );
        let len = fs::metadata(workspace.path.join("limited.bin"))
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        assert!(len <= 1024);
    }

    #[test]
    fn redacts_secret_like_shell_output() {
        let workspace = TempWorkspace::new();
        let planner = ShellPlanner::new(&workspace.path).expect("planner");
        let executor = ShellExecutor::new(&workspace.path).expect("executor");
        let plan = planner
            .plan(request(
                "printf 'OPENAI_API_KEY=sk-live-secret\\nAuthorization: Bearer ya29.oauth-token'",
            ))
            .expect("plan");

        let execution = executor.execute(&plan, Some(PermissionReply::Once));

        assert_eq!(execution.result.status, ToolResultStatus::Succeeded);
        let output = execution.result.output.expect("output");
        assert!(!output.text.contains("sk-live-secret"));
        assert!(!output.text.contains("ya29.oauth-token"));
        assert_eq!(
            output.metadata["stdout"],
            json!("OPENAI_API_KEY=[REDACTED]\nAuthorization: Bearer [REDACTED]")
        );
    }
}
