# Evals And Metrics

Coddy must be evaluated as an engineering system, not only as a chat interface.

## Required Eval Types

- read-only repository analysis;
- safe edit with approval;
- edit rejection preserving files;
- shell approval and shell denial;
- grounded response with file citations;
- prompt-injection resistance;
- secret redaction;
- subagent routing;
- capability benchmark for SWE-Bench-like, Terminal-Bench-like, polyglot,
  security, RAG, memory, skills/tools, MCP, UI and performance prompts;
- deep-context eval for long context, RAG, memory, tools, subagents,
  prompt-injection resistance and coding orchestration;
- fixture benchmark contracts with setup command, allowed tools, expected
  files, test command, security assertions and timeout;
- fixture benchmark rejection for unknown exact `coddy-agent` and
  `coddy-runtime` verifier tests;
- fixture benchmark JSONL metrics with summary and per-case records;
- fixture smoke materialization with read-only shell verifier metrics and
  RAG/memory tag coverage;
- live project battery missing-workspace records that do not distort prompt
  metrics;
- live project and patch battery detectors for pseudo-tool markup and
  incomplete action promises in English and Portuguese;
- patch battery application with hunk recount for model-generated diffs whose
  line counts are stale;
- eval baseline regression;
- UI e2e smoke for model, tool approval and subagent activity.

## Metrics

- pass rate;
- regression rate;
- tool error rate;
- permission denial rate;
- false-confidence rate;
- secret exposure count;
- command block rate;
- time to accepted change;
- test pass rate;
- audit finding count;
- long-context bytes covered;
- RAG citation/provenance pass rate;
- memory conflict handling pass rate;
- untrusted tool-output escalation pass rate.

## Gate Policy

CI must fail on deterministic regressions, high-severity dependency audit
findings, secret scan failures, type errors, lint failures and build failures.
Live provider evals may be tracked separately when credentials or network
availability make them unsuitable for every PR.
Deep-context evals are deterministic gates for routing, context contracts and
safety escalation. They do not replace live model reasoning evals, patch
attempts against fixture repositories or retrieval precision/recall tests.
