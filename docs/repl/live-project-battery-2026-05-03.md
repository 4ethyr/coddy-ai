# Live Project Battery - 2026-05-03

## Scope

Live validation using Coddy with `openrouter/deepseek/deepseek-v4-flash` against
five local workspaces under `/home/aethyr/Documents`:

- `Guardian`
- `apex`
- `coddy`
- `maker`
- `visionclip`

The OpenRouter credential was loaded from the local `.env` without printing the
value. Captured command output redacted provider tokens as `<redacted>`.

Artifacts for this run were written outside the repository:

```text
/tmp/coddy-project-battery-20260503-022023
```

## Prompt Set

Each workspace received four read-only prompts:

- architecture analysis;
- quality/testability review;
- security review without reading sensitive files;
- TDD implementation plan without editing files.

Each prompt ran in a clean runtime session, reselected the OpenRouter model, and
captured:

- answer text;
- runtime events;
- session snapshot;
- tool counts;
- intent routing;
- tool failures;
- grounding guard hits;
- high-confidence secret-pattern scan over captured outputs.

## Metrics

| Metric | Result |
| --- | ---: |
| Prompts executed | 20 |
| Prompt command exits | 20/20 successful |
| Tool completions | 105 |
| Tool failures | 2 |
| Tool success rate | 98.1% |
| `Coddy grounding check` hits | 1 |
| High-confidence secret pattern files in captured outputs | 0 |
| Actual provider errors during main battery | 0 |
| Average prompt duration | 43.2s |

Tool distribution:

| Tool | Count |
| --- | ---: |
| `filesystem.list_files` | 61 |
| `filesystem.read_file` | 43 |
| `filesystem.search_files` | 1 |

## Findings

### Strong Behavior

- The runtime executed all 20 prompts without CLI-level failure.
- No high-confidence API key/private-key patterns appeared in captured outputs.
- The model generally respected read-only constraints and did not edit files.
- The new grounding guard triggered on a weak Coddy TDD plan that admitted it had
  not read source/tests before proposing implementation details.
- For architecture prompts with explicit file-reading instructions, the model
  usually used a useful mix of `list_files` and `read_file`.

### Issues Found

1. Intent routing was too sensitive to the words `test`/`teste`.
   - 15/20 prompts were routed as `GenerateTestCases` even when the user asked
     for architecture, quality, security, or planning.
   - Fix: architecture/codebase/quality/security analysis now routes as
     `ExplainCode`; TDD implementation planning now routes as
     `AgenticCodeChange`; direct test generation still routes as
     `GenerateTestCases`.

2. The model produced pseudo tool calls in final text in one Coddy security run.
   - Example shape: `Tool call 4: ...` and "I will now perform two additional
     tool calls".
   - Fix: textual tool-call guard now blocks numbered pseudo tool-call headers
     and expired pseudo tool-call transcripts.

3. Some model-initiated filesystem calls used numeric arguments as strings.
   - Example: `max_entries` or `max_bytes` sent as `"20"` instead of `20`.
   - Fix: runtime now safely coerces positive numeric strings for
     `max_entries`, `max_bytes`, and `max_matches`; invalid strings still fail.

4. Planning prompts often spent too much budget on directory listing.
   - In multiple `coding_plan` cases, the model listed folders several times and
     read only one source/config file.
   - This is now partially mitigated by stronger routing, but the next product
     improvement should be an active planning preset that reserves reads for
     source and tests when the user asks for TDD plans.

5. A post-fix live smoke hit an OpenRouter follow-up timeout.
   - The runtime returned a recoverable message with retry/context-reduction
     guidance.
   - This is provider/runtime reliability, not a local tool failure.

## Post-Fix Validation

Focused tests added and passed:

- `classify_architecture_analysis_as_code_explanation_despite_test_mentions`
- `classify_tdd_plan_as_agentic_code_change_not_test_generation`
- `classify_direct_test_request_as_test_generation`
- `assistant_response_blocks_numbered_pseudo_tool_calls`
- `assistant_response_blocks_expired_pseudo_tool_calls`
- `model_initiated_filesystem_tool_limits_accept_numeric_strings`
- `model_initiated_filesystem_tool_limits_leave_invalid_strings_unchanged`

Validation commands:

```bash
cargo fmt --check
cargo test -p coddy-runtime -- --test-threads=1
cargo clippy -p coddy-runtime --all-targets -- -D warnings
cargo build -p coddy
./scripts/guard_no_secrets.sh
```

Post-fix live smoke:

```text
workspace=/home/aethyr/Documents/coddy
model=openrouter/deepseek/deepseek-v4-flash
intent=ExplainCode
tool_failures=0
textual_tool_call_block=0
result=recoverable OpenRouter follow-up timeout
```

## Next Improvements

- Add an active recovery loop for `Coddy grounding check` instead of only
  warning the user.
- Add a planning preset that reserves tool budget for source and test reads in
  TDD/coding-plan prompts.
- Add richer tool failure metadata to `ToolCompleted` events so UI/eval reports
  can explain which path or argument failed without parsing final text.
- Add a deterministic eval case for provider-safe numeric string coercion.

## Follow-Up Hardening: Long-Context Coding Prompts

Additional secure live testing was run with `openrouter/deepseek/deepseek-v4-flash`
against local projects in `Documents/` (`apex`, `Guardian`, `maker`,
`visionclip`, `coddy`). API keys were loaded from local configuration and were
not printed in outputs.

New failure modes found and fixed:

1. Follow-up responses after tool observations could end with pending tool
   requests, unregistered aliases or raw `Tool observations:` text.
   - Fix: synthesize a final answer when the model requests unexecuted tools,
     when the user tool budget is exhausted, or when a requested tool is unsafe
     or unavailable.

2. Some providers emitted legacy tool aliases such as `filesystem.dot_read_file`.
   - Fix: provider-safe tool-name decoding now maps these aliases back to
     canonical tools such as `filesystem.read_file`.

3. Final answers could contain pseudo-tools in several formats:
   - `Tool 1/10: ...`
   - `Tool 1: ...`
   - `Call 1 of 8 ...`
   - fenced ````tool_call` blocks.
   - Fix: textual tool-call detection blocks these outputs and triggers
     recovery/synthesis rather than showing them to the user.

4. Portuguese action promises such as "vou priorizar leituras" or "vou
   continuar a exploracao" were not always detected.
   - Fix: incomplete-action recovery now covers these Portuguese variants and
     forces a grounded synthesis from existing observations.

5. Architecture/security reviews sometimes guessed conventional paths after a
   directory listing.
   - Fix: task-specific guidance now instructs models to read only observed
     paths and mark missing conventional files as gaps.

6. OpenRouter follow-up turns could time out on larger agentic contexts.
   - Fix: OpenRouter OpenAI-compatible timeout is extended to 300 seconds for
     agentic follow-ups.

Validation artifacts:

```text
/tmp/coddy-patch-task-battery-final-20260503-221925
/tmp/coddy-live-project-battery-adversarial-metric-20260503-221647
/tmp/coddy-live-project-battery-maker-architecture-guidance-20260503-220652
```

Measured results:

- Deterministic quality eval: score 100.
- Prompt battery: 1200/1200 passed.
- Local SWE-bench-style patch battery: 3/3 resolved across Python, Node and
  Rust fixtures.
- Adversarial security revalidation for `maker` and `visionclip`: average score
  91, provider errors 0, tool failures 0, pseudo-tool markup 0, secret hits 0.

The local patch battery is a controlled SWE-bench-style harness, not an
official SWE-bench score. Official SWE-bench still requires the upstream Docker
harness and Docker daemon access.

### Official SWE-bench preflight

Added `scripts/run_swebench_official.sh` as a reproducible wrapper around the
upstream `swebench.harness.run_evaluation` entrypoint. The wrapper records a
JSON report before execution so official benchmark readiness is measured instead
of inferred from local machine state.

Current preflight on this machine:

```text
status=preflight_failed
docker.accessible=false
docker.socket="srw-rw---- 1 root docker ... /var/run/docker.sock"
disk.freeGb=33
disk.requiredGb=120
swebenchPythonPackage=false
uv=true
```

Interpretation:

- Docker is running on the host, but this process is not in the `docker` group
  and non-interactive `sudo` cannot be used here.
- The default cache-backed filesystem has about 33 GB free, which is below the
  configured 120 GB threshold for real SWE-bench work.
- `uv` is available, so the wrapper can bootstrap the Python `swebench` package
  once Docker permissions and storage are fixed.

Official SWE-bench can be attempted after the environment passes:

```bash
SWE_BENCH_PREFLIGHT_ONLY=1 ./scripts/run_swebench_official.sh
./scripts/run_swebench_official.sh
```

Use `SWE_BENCH_PREDICTIONS=/path/to/coddy-predictions.jsonl` for Coddy-scored
runs. The default `gold` prediction mode is only a harness validation smoke.

### Follow-up hardening

Additional live prompts against OpenRouter/DeepSeek V4 Flash exposed several
agent-output edge cases that were converted into deterministic tests and runtime
guards:

- `maker/codegen_tdd` returned generic XML
  `<function name="filesystem.list_files">` instead of a final answer.
  - Fix: textual tool-call guard now blocks generic function XML markup.
  - Revalidation: `maker/codegen_tdd` improved to score 95/100, then 100/100
    on the broader rerun with no pseudo-tool markup.
- `visionclip/code_review_security` returned variant DSML
  `<｜｜DSML｜｜tool_calls>`.
  - Fix: DSML detection now matches variant DSML tool-call wrappers by
    semantics, not only exact glyph spelling.
- `visionclip/architecture` returned an action promise in Portuguese
  (`vou focar...`) instead of a synthesis.
  - Fix: incomplete action-promise detection covers `vou focar`,
    `vou me concentrar`, `vou me ater`, and review-continuation phrases.
- Security reviews were spending too much budget on listings and then
  speculating.
  - Fix: added a `security-review` deterministic evidence bootstrap that reads
    manifest, high-signal security/config/action files and an observed test
    file before the first model turn.
  - Fix: security guidance now disallows High/Critical/Confirmed labels without
    direct source or test evidence and tells the model not to claim tests are
    absent unless test paths were actually searched/read.
- Live harness socket paths could exceed Unix `SUN_LEN` when the artifact path
  was long.
  - Fix: live and patch batteries use short sockets under `/tmp` while keeping
    artifacts in the configured output directory.
- Follow-up tool observations did not identify the executed path/query in their
  header.
  - Fix: successful tool observations now include the path/query, improving
    grounded synthesis after multiple tool rounds.

Latest focused live artifacts:

```text
/tmp/coddy-patch-final-234026
/tmp/coddy-sec-coddy-synth-235547
/tmp/coddy-live-coddy-smoke-234521
/tmp/coddy-sec-vc-233409
```

Latest focused measured results:

- Local patch/SWE-style battery: 3/3 resolved, patch extraction 3/3,
  `git apply` failures 0, test failures 0, provider errors 0, pseudo-tool
  markup 0, secret hits 0.
- `coddy/code_review_security` after pending-permission synthesis: score 85,
  provider errors 0, tool failures 0, pseudo-tool markup 0, secret hits 0.
- SWE-bench official preflight remains blocked on this machine:
  Docker socket access is denied for the current process and the cache-backed
  filesystem has about 32 GB free versus the 120 GB threshold.

### 2026-05-04 expanded live hardening

Additional live prompts used OpenRouter with `deepseek/deepseek-v4-flash`
against the local workspaces `apex`, `Guardian`, `maker`, `visionclip` and
`coddy`. Provider credentials were loaded from the local environment and only
redacted credential metadata appeared in logs.

New issues reproduced:

1. Some models emitted JSON pseudo-tools with a top-level `"calls"` array.
   - Fix: runtime and live harness now detect `{"calls":[{"name":
     "filesystem.*","args":...}]}` as unsafe textual tool markup.
2. Some models emitted `Request: filesystem.read_file ...` lines after tool
   observations instead of using native structured tool calls.
   - Fix: textual tool-call guard and harness detection now cover
     `Request: filesystem.*`, `Request: shell.run` and `Request: subagent.*`.
3. Formal Portuguese and English action promises were not always recovered.
   - Fix: action-promise detection now covers `inspecionarei`, `analisarei`,
     `verificarei`, `mapearei`, `let me now inspect`, `I'll now inspect`, and
     agentic/security object terms such as policies, subagents, orchestration,
     components and ML/math references.
4. Long agentic follow-ups could still time out after many successful tool
   calls, even with individual tool-output compaction.
   - Fix: Coddy now compacts at three levels before provider follow-ups:
     individual tool output, per-round tool observations and accumulated tool
     messages across rounds. The most recent evidence is preserved first.
5. The local Coddy client timeout could expire before the provider timeout.
   - Fix: default client request timeout is now 420 seconds; live and patch
     batteries inherit that value unless `CODDY_CLIENT_REQUEST_TIMEOUT_MS` is
     explicitly set.
6. Security-review grounding checks were over-triggered for scoped claims such
   as “no direct evidence found in inspected files.”
   - Fix: the guard still blocks absolute implementation absence claims made
     without source inspection, but allows scoped no-direct-evidence language.

Focused revalidation:

```text
/tmp/coddy-live-guardian-agentic-scorefix-20260504-011649
/tmp/coddy-live-visionclip-agentic-contextfix-20260504-014806
/tmp/coddy-live-coddy-security-groundingfix-20260504-015245
```

Measured focused results:

- `Guardian/agentic_coding_quality`: score 96, provider errors 0, tool failures
  0, pseudo-tool markup 0, secret hits 0.
- `visionclip/agentic_coding_quality`: reproduced provider timeout at score 31
  before accumulated compaction; after the fix, score 96, provider errors 0,
  tool failures 0, pseudo-tool markup 0, secret hits 0.
- `coddy/code_review_security`: grounding-check false positive removed; focused
  rerun score 92, provider errors 0, tool failures 0, pseudo-tool markup 0,
  secret hits 0.

Intermediate five-project matrix before the final focused fixes:

```text
/tmp/coddy-live-five-projects-agentic-security-20260504-011822
```

Summary:

- Prompts: 10.
- CLI completions: 10/10.
- Agent completed: 9/10 before accumulated compaction, with the remaining
  failure isolated to `visionclip/agentic_coding_quality`.
- Tool calls: 100.
- Secret hits: 0.
- Pseudo-tool markup: 0.
- Provider errors: 1 before accumulated compaction; 0 on the focused
  `visionclip` rerun.
- Average score before final focused fixes: 80.7.

This remains live provider sampling, not an official benchmark score. Official
SWE-bench remains blocked by Docker socket permissions and the configured disk
threshold on this machine.
