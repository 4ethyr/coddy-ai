# Agentic Eval Benchmark Strategy

This document maps Coddy's local deterministic evals to public agentic coding
benchmark families. It is intentionally practical: every benchmark family here
must either map to an existing deterministic Coddy check or define the next
verifier needed before it can be treated as a real live benchmark.

## Current Local Coverage

Run:

```bash
coddy eval quality --json
coddy eval capability-benchmark --json
coddy eval fixture-benchmark --json
coddy eval fixture-benchmark --json \
  --write-report evals/reports/fixture-benchmark.jsonl \
  --run-id local-fixture-smoke
coddy eval fixture-smoke --json \
  --workspace target/coddy-fixture-smoke/local-fixture-smoke \
  --run-id local-fixture-smoke
coddy eval deep-context --json
coddy eval multiagent --json
coddy eval prompt-battery --json
```

The current local quality gate includes:

- `multiagent`: deterministic subagent routing and reducer contracts.
- `prompt-battery`: 1200 deterministic routing prompts across 30 stacks,
  10 knowledge areas and 4 variants.
- `capability-benchmark`: SWE-Bench-like, Terminal-Bench-like, polyglot,
  security, RAG, memory, skills/tools, MCP, UI and performance prompts.
- `fixture-benchmark`: structured offline fixture contracts for SWE-Bench-like,
  Rust runtime, TypeScript/Electron, security, RAG/memory and skills/MCP tasks.
  It can also write JSONL records for run-level and case-level metrics.
- `fixture-smoke`: materializes a small synthetic workspace, runs a read-only
  shell verifier through Coddy's guard/executor path and reports pass/fail
  metrics.
- `deep-context`: deterministic long-context cases for RAG, memory, tool
  output, subagent orchestration, prompt-injection resistance and polyglot
  coding coordination.
- `grounded-response`: repository citation hallucination checks.

## Fixture JSONL Metrics

`coddy eval fixture-benchmark --write-report <path>` writes one summary record
followed by one record per fixture case. Records include:

- `runId`;
- `recordType`: `summary` or `case`;
- score and pass/fail status;
- benchmark family and stack for case records;
- command count;
- expected file count;
- security assertion count;
- structured failures.

This is deliberately offline today: it records fixture contract quality before
Coddy executes patch attempts against materialized fixture repositories.

Fixture benchmark contracts that declare exact Cargo test verifiers must use a
known test path or test name. The current executable Coddy fixture verifiers
are:

- `eval::tests::security_fixture_detects_path_traversal`;
- `eval::tests::rag_memory_fixture_retrieves_expected_context`;
- `eval::tests::skills_mcp_fixture_validates_permissions`.
- `runtime_fixture_regression`.

Unknown exact Coddy agent or Coddy runtime fixture verifier names fail the
deterministic fixture benchmark instead of being accepted as unverified shell
strings.

## Fixture Smoke

`coddy eval fixture-smoke` is the first executable fixture step. It currently:

- creates synthetic workspaces under the requested path;
- materializes source, test, docs and README files;
- rejects unsafe materialized paths before writing;
- runs read-only `find` and `rg` verifiers through the shell planner/executor
  with network disabled;
- reports materialized file count, verifier count, tag coverage, score and case
  status.

It is intentionally small. Its purpose is to verify the mechanics for safe
fixture materialization and read-only verifier execution before adding
model-driven patch attempts. The default smoke set now includes a Python unit
fixture plus a RAG/memory fixture with repository citations, memory provenance,
stale-context conflict markers and prompt-injection filtering markers.

## Deep Context Eval

`coddy eval deep-context` exercises Coddy's deterministic planning surface with
large prompts that combine repository context, RAG citations, memory
provenance, stale-context conflicts, untrusted tool output, subagent handoff
requirements and coding/security constraints.

The current gate measures:

- total long-context bytes across all cases;
- category coverage for RAG, memory, tools, subagents, coding and prompt
  injection;
- subagent coverage across planner, explorer, coder, reviewer,
  security-reviewer, test-writer, eval-runner and docs-writer;
- required term retention for critical long-context instructions;
- security escalation when untrusted tool output attempts to override policy.

This is not a live LLM reasoning benchmark yet. It is a deterministic regression
gate for Coddy's routing, context-shaping and safety contracts. Live
long-context quality still requires fixture-backed patch attempts, retrieval
precision/recall checks and provider-backed model evals.

## Benchmark Mapping

| Coddy area | Public reference | What Coddy measures now | Required next verifier |
| --- | --- | --- | --- |
| SWE-Bench-like issue repair | SWE-bench Verified / SWE-Bench Pro | Routing coverage plus a fixture contract with prompt, allowed tools, expected files, test command, timeout and security assertions | Materialized fixture repo with failing tests, patch application and deterministic pass/fail |
| Terminal work | Terminal-Bench | Routing coverage plus verifier command safety checks through Coddy's command guard | Containerized terminal task runner with verifier script execution |
| Polyglot editing | Aider polyglot | Routing coverage plus Rust/TypeScript/Python-style fixture contract fields | Multi-language fixture tasks with language-specific test commands |
| Security audit | Prompt injection, command injection, secret leakage and supply chain checks | Security routing plus vulnerable-fixture contract and no-secret assertions | Vulnerable fixture repos with expected findings and no secret disclosure |
| RAG/context retrieval | ContextBench and repository-level RAG research | RAG routing plus expected files, citations, context precision fixture contract and deep-context citation/provenance checks | Context recall/precision verifier over expected files, symbols and chunks |
| Memory/long context | SWE Context Bench and memory-agent benchmarks | Memory routing plus provenance, expiry, stale-context security assertions and deep-context conflict checks | Memory fixture with provenance, expiry, conflict and stale-memory tests |
| Skills/tools | Claude Code Skills and MCP extension patterns | Skill/tool routing plus skill manifest, allowed-tools and lifecycle fixture contract | Skill manifest parser, allowed-tools policy, fixture skills and lifecycle tests |
| MCP | MCP specification | MCP routing plus mock server, permission bridge and prompt-injection assertions | Mock MCP server with tools/resources/prompts, prompt-injection output and timeout tests |
| UI/UX | Desktop coding agent workflows | Electron/React routing plus expected renderer/main/e2e files and verifier command | Playwright/Electron flow for approvals, tool logs, diffs and subagent state |
| Performance | Low-latency agent loops | Runtime routing plus cargo verifier command and timeout budget contract | Latency budget harness for tool routing, shell execution, context compaction and UI events |

## Source Notes

- OpenAI's SWE-bench Verified announcement describes issue-to-patch evaluation
  over real repositories and tests: https://openai.com/index/introducing-swe-bench-verified/
- OpenAI later noted that SWE-bench Verified is no longer sufficient for
  frontier coding capability measurement, which is why Coddy should not rely on
  one static issue benchmark alone:
  https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/
- OpenAI's GPT-5 developer release references SWE-bench Verified and Aider
  polyglot as coding-agent signals:
  https://openai.com/index/introducing-gpt-5-for-developers/
- OpenAI's GPT-5.3-Codex release references SWE-Bench Pro, Terminal-Bench,
  OSWorld-Verified and cybersecurity CTFs as broader agentic/coding signals:
  https://openai.com/index/introducing-gpt-5-3-codex/
- Terminal-Bench evaluates terminal agents with verifier scripts:
  https://terminalbench.lol/
- Claude Code extension documentation separates always-on context, skills,
  subagents, hooks and MCP:
  https://code.claude.com/docs/en/features-overview
- Claude Code Skills define on-demand workflows through `SKILL.md` files:
  https://docs.claude.com/en/docs/claude-code/skills
- MCP exposes tools/resources/prompts and requires hosts to build clear consent
  and authorization flows:
  https://modelcontextprotocol.io/specification/2024-11-05/index
- MCP tools are schema-driven server capabilities:
  https://modelcontextprotocol.io/specification/2025-06-18/server/tools
- ContextBench frames context recall, precision and efficiency as explicit
  metrics for coding-agent retrieval:
  https://arxiv.org/abs/2602.05892
- SWE Context Bench motivates evaluating whether coding agents reuse experience
  across related tasks:
  https://arxiv.org/abs/2602.08316

## Next Implementation Steps

1. Materialize the fixture repositories described by `coddy eval
   fixture-benchmark --json`.
2. Extend `coddy eval fixture-smoke` from synthetic smoke fixtures into the
   fixture repository families described by `fixture-benchmark`.
3. Add patch-application and verifier execution in a temp workspace with the
   existing shell sandbox policy.
4. Extend the existing fixture JSONL records with model, tool calls, files
   touched, tests run, cost and residual risks once patch execution exists.
5. Add fixture-backed RAG and memory verifiers before enabling persistent
   memory writes.
6. Add live deep-context model evals only when credentials, cost budget and
   network availability are explicit in CI.
7. Treat live model prompt batteries as non-blocking unless credentials and
   network availability are explicit in CI.
