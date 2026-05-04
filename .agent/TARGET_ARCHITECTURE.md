# Target Architecture

Coddy should use explicit boundaries between domain contracts, runtime
orchestration, tool execution, provider integrations and UI.

## Target Layers

1. Core contracts: events, tools, permissions, sessions, history.
2. Agent domain: planning, tool routing, subagent contracts, eval scoring.
3. Runtime application: model loop, context policy, permission state, event bus.
4. Infrastructure: shell executor, filesystem executor, IPC, provider clients.
5. Interfaces: CLI, REPL terminal, Electron UI and preload bridge.
6. Security layer: command guard, sandbox, redaction, approvals and audit logs.
7. Evaluation layer: deterministic tests, prompt evals, live batteries and
   regression gates.

## Architectural Rules

- Domain contracts must not depend on UI.
- Tools must have schemas, risk levels, permissions and bounded output.
- Shell, network, filesystem writes and external paths are high-risk operations.
- Model-facing tools must default to read-only, low-risk operations.
- Subagents must have explicit role, inputs, outputs, allowed tools and success
  criteria.
- Long-running tasks must be resumable through observable state.
- Audit events must be structured, redacted and correlated by task/run/session.

## Target Capabilities

- sandboxed command execution;
- durable agent execution log;
- executable subagents with isolated workspaces;
- MCP adapter behind the same permission bridge as local tools;
- repository-aware RAG with citations and prompt-injection controls;
- memory scoped by user, project and session with expiry and provenance.
