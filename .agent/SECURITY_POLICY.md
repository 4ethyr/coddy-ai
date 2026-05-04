# Security Policy

Coddy must assume that model output, repository content, tool output and external
documents can be malicious.

## Defaults

- Least privilege by default.
- No production access by default.
- No network access for tools unless explicitly allowed.
- No secret values in model context, logs or UI errors.
- No destructive command without explicit approval.
- Workspace writes require prior read and approval.

## Shell Policy

Shell execution must:

- validate command risk before execution;
- require approval for mutating or complex commands;
- block destructive commands;
- deny network-using commands by default unless a dedicated shell network policy
  explicitly enables them;
- run with sanitized environment;
- bound stdout/stderr;
- redact secret-like output;
- enforce timeout;
- terminate process groups where supported;
- apply Linux `no_new_privs` where supported before shell exec;
- disable shell core dumps where supported;
- apply a private shell file creation umask where supported;
- record explicit sandbox capability metadata, including non-isolated controls;
- record available sandbox provider candidates separately from the active
  isolation profile;
- record namespace capability probe results separately from provider candidate
  presence, so an installed binary is not treated as an active sandbox;
- fail closed when configuration requires kernel isolation but the active shell
  executor is not kernel-isolated;
- allow operators to opt into strict shell sandbox enforcement with
  `[security].shell_sandbox_policy = "require-kernel-isolation"` or
  `CODDY_SHELL_SANDBOX_POLICY=require-kernel-isolation`;
- use `bubblewrap` as the first supported kernel sandbox provider when strict
  shell sandbox enforcement is enabled and `bwrap` is available on the sanitized
  shell path and passes a namespace capability probe;
- fall back to `unshare` only when `bubblewrap` is unavailable or fails its
  probe and `unshare` itself passes a namespace capability probe;
- record structured execution metadata.

## Secret Handling

Secrets must be:

- loaded only from approved stores or request-scoped environment;
- redacted before persistence or display;
- excluded from shell/tool environments unless explicitly required;
- never printed in tests, errors or eval reports.

## Human Escalation

Escalate to a human when:

- a command may destroy data;
- a tool touches external or sensitive paths;
- an operation may deploy, publish or modify infrastructure;
- requirements conflict with security policy.
