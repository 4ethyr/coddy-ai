# Subagents

Subagents are specialized execution roles. They must not be treated as free-form
prompts with unlimited tool access.

## Required Roles

- `explorer`: read-only codebase inspection.
- `planner`: task decomposition, risks and acceptance criteria.
- `coder`: scoped implementation after plan approval.
- `test-writer`: unit, integration, e2e and eval coverage.
- `reviewer`: regression, maintainability and diff review.
- `security-reviewer`: secrets, permissions, command safety and prompt injection.
- `docs-writer`: user and developer documentation.
- `eval-runner`: deterministic eval and quality gate execution.

## Role Contract

Each subagent must declare:

- name and purpose;
- allowed tools;
- allowed filesystem scope;
- approval policy;
- input schema;
- output schema;
- stop condition;
- failure modes;
- required logs;
- tests or evals.

## Current Implementation Direction

The current codebase has subagent role definitions, routing and output reduction.
The target is isolated executable subagent sessions with explicit permission
inheritance, independent logs and deterministic reducers.
