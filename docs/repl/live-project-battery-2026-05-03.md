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
