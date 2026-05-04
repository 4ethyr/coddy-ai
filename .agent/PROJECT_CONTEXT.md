# Project Context

Coddy is a Rust-first agentic coding system with an Electron/React desktop UI.
It provides a local REPL, model routing, tool execution, permission prompts,
subagent planning contracts and eval harnesses.

## Current Structure

- `apps/coddy`: Rust CLI and local runtime server commands.
- `apps/coddy-electron`: Electron main process, preload bridge and React UI.
- `crates/coddy-agent`: tool registry, shell guard, filesystem tools, subagent
  contracts and eval runners.
- `crates/coddy-runtime`: runtime loop, model prompting, event streaming and
  session orchestration.
- `crates/coddy-core`: shared contracts, permissions, events and history.
- `crates/coddy-ipc`: local IPC protocol.
- `crates/coddy-client`: runtime client.
- `scripts`: validation, packaging and eval helpers.
- `docs/repl`: design notes and historical implementation plans.

## Known State

Implemented:

- guarded filesystem reads/searches/edits;
- guarded shell execution with approvals;
- secret redaction for history, model errors, subprocess errors and shell output;
- Electron UI with secure credential storage;
- local eval suites and prompt batteries;
- CI and release validation gates.

Still evolving:

- OS-level sandboxing beyond guardrails and process isolation;
- executable isolated subagents;
- durable audit logs;
- MCP runtime integration;
- RAG and durable agent memory.
