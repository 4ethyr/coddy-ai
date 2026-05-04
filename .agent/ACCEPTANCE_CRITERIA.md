# Acceptance Criteria

A change is acceptable when it satisfies all relevant criteria below.

## General

- Scope is small and reviewable.
- Existing behavior is preserved or migration is documented.
- Tests cover the changed behavior.
- Relevant checks pass.
- Diff has no whitespace errors.
- No secrets are added to tracked files.

## Security

- Sensitive values are redacted in logs, model context, UI errors and persisted
  history.
- Dangerous commands are blocked or require explicit approval.
- Tool environments use least privilege.
- High-risk tools produce audit evidence.

## Agentic Coding

- The agent inspects before editing.
- The agent plans complex tasks.
- The agent uses approvals for writes and shell.
- The agent validates changes before claiming success.
- The agent reports residual risk honestly.

## CI Gate

Required local or CI checks:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `npm audit --audit-level=high`
- `npm run typecheck`
- `npm run typecheck:main`
- `npm run lint`
- `npm test`
- `npm run test:e2e`
- `npm run build`
- `./scripts/guard_no_secrets.sh`
