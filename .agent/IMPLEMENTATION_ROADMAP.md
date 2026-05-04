# Implementation Roadmap

## Phase 0: Immediate Security

- Redact subprocess errors.
- Sanitize tool and runtime environments.
- Redact shell output.
- Block destructive commands.
- Add CI gates and dependency audit.

## Phase 1: Execution Hardening

- Add OS-level sandbox execution profile.
- Add durable structured audit log.
- Add resource limits and network policy for tools.
- Add tests for sandbox escape attempts and descendant cleanup.

## Phase 2: Architecture Stabilization

- Break large runtime and agent files into cohesive modules.
- Move durable logging, context policy and permission persistence behind clear
  interfaces.
- Document stable contracts for CLI, IPC and UI.

## Phase 3: Subagents

- Implement executable isolated subagent sessions.
- Add per-role tool permissions and handoff logs.
- Add reducer tests and multiagent eval gates.

## Phase 4: MCP, RAG And Memory

- Add MCP runtime adapter behind permission bridge.
- Add repository RAG with citations and trusted-source controls.
- Add durable scoped memory with provenance, expiry and secret policy.

## Phase 5: Production Readiness

- Add SBOM/signing where appropriate.
- Add release evidence bundle.
- Add dashboards or exported metrics for agent runs.
