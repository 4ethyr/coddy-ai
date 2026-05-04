#!/usr/bin/env bash
set -Eeuo pipefail

CODDY_BIN="${CODDY_BIN:-target/debug/coddy}"
MODEL_PROVIDER="${MODEL_PROVIDER:-openrouter}"
MODEL_NAME="${MODEL_NAME:-deepseek/deepseek-v4-flash}"
OUTPUT_ROOT="${OUTPUT_ROOT:-/tmp/coddy-patch-task-battery-$(date +%Y%m%d-%H%M%S)}"
CODDY_CLIENT_REQUEST_TIMEOUT_MS="${CODDY_CLIENT_REQUEST_TIMEOUT_MS:-300000}"
export CODDY_CLIENT_REQUEST_TIMEOUT_MS

SUMMARY_JSONL="$OUTPUT_ROOT/summary.jsonl"
mkdir -p "$OUTPUT_ROOT"
: > "$SUMMARY_JSONL"

runtime_pids=()
cleanup() {
  for pid in "${runtime_pids[@]:-}"; do
    if kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
    fi
  done
}
trap cleanup EXIT

wait_for_socket() {
  local socket_path="$1"
  for _ in $(seq 1 50); do
    if [[ -S "$socket_path" ]]; then
      return 0
    fi
    sleep 0.1
  done
  return 1
}

extract_patch() {
  local answer_path="$1"
  local patch_path="$2"
  awk '
    /^```diff[[:space:]]*$/ { in_diff = 1; next }
    /^```[[:space:]]*$/ && in_diff { in_diff = 0; exit }
    in_diff { print }
  ' "$answer_path" > "$patch_path"
  if [[ ! -s "$patch_path" ]]; then
    sed -n '/^diff --git /,$p' "$answer_path" > "$patch_path"
  fi
}

setup_python_slug_task() {
  local workspace="$1"
  cat > "$workspace/string_tools.py" <<'PY'
def slugify(value: str) -> str:
    return value.strip().lower().replace(" ", "-")
PY
  cat > "$workspace/test_string_tools.py" <<'PY'
import unittest

from string_tools import slugify


class SlugifyTests(unittest.TestCase):
    def test_collapses_punctuation_whitespace_and_accents(self):
        self.assertEqual(slugify(" Hello,   Café Déjà Vu!! "), "hello-cafe-deja-vu")

    def test_returns_empty_for_symbol_only_input(self):
        self.assertEqual(slugify(" ... !!! "), "")


if __name__ == "__main__":
    unittest.main()
PY
  cat > "$workspace/ISSUE.md" <<'MD'
`slugify` should produce URL-safe ASCII slugs. It must lowercase, strip accents,
remove punctuation, collapse whitespace/separator runs into a single `-`, trim
leading/trailing separators, and return an empty string for symbol-only input.
MD
}

setup_node_duration_task() {
  local workspace="$1"
  cat > "$workspace/package.json" <<'JSON'
{"type":"module","scripts":{"test":"node --test"}}
JSON
  cat > "$workspace/duration.js" <<'JS'
export function parseDuration(input) {
  const match = /^(\d+)(ms|s|m)$/.exec(input)
  if (!match) return 0
  const value = Number(match[1])
  const unit = match[2]
  if (unit === 'ms') return value
  if (unit === 's') return value * 100
  if (unit === 'm') return value * 60_000
  return 0
}
JS
  cat > "$workspace/duration.test.js" <<'JS'
import assert from 'node:assert/strict'
import test from 'node:test'

import { parseDuration } from './duration.js'

test('parses supported duration units into milliseconds', () => {
  assert.equal(parseDuration('250ms'), 250)
  assert.equal(parseDuration('3s'), 3000)
  assert.equal(parseDuration('2m'), 120000)
})

test('throws for malformed durations instead of silently returning zero', () => {
  assert.throws(() => parseDuration('abc'), /invalid duration/i)
  assert.throws(() => parseDuration('-1s'), /invalid duration/i)
})
JS
  cat > "$workspace/ISSUE.md" <<'MD'
`parseDuration` should convert `ms`, `s`, and `m` to milliseconds. Seconds are
currently scaled incorrectly. Invalid or negative inputs should throw a helpful
`invalid duration` error instead of silently returning `0`.
MD
}

setup_rust_path_task() {
  local workspace="$1"
  mkdir -p "$workspace/src" "$workspace/tests"
  cat > "$workspace/Cargo.toml" <<'TOML'
[package]
name = "coddy_patch_task_path"
version = "0.1.0"
edition = "2021"
TOML
  cat > "$workspace/src/lib.rs" <<'RS'
pub fn safe_join(base: &str, child: &str) -> Result<String, String> {
    Ok(format!(
        "{}/{}",
        base.trim_end_matches('/'),
        child.trim_start_matches('/')
    ))
}
RS
  cat > "$workspace/tests/safe_join.rs" <<'RS'
use coddy_patch_task_path::safe_join;

#[test]
fn joins_simple_relative_paths() {
    assert_eq!(safe_join("/workspace", "src/lib.rs").unwrap(), "/workspace/src/lib.rs");
}

#[test]
fn rejects_absolute_and_parent_traversal_paths() {
    assert!(safe_join("/workspace", "/etc/passwd").is_err());
    assert!(safe_join("/workspace", "../secret.txt").is_err());
    assert!(safe_join("/workspace", "src/../../secret.txt").is_err());
}
RS
  cat > "$workspace/ISSUE.md" <<'MD'
`safe_join` is used before workspace file access. It must reject absolute child
paths and any path containing parent traversal (`..`) while preserving normal
relative joins. Return `Err` with a useful message for unsafe paths.
MD
}

run_task() {
  local task_name="$1"
  local setup_function="$2"
  local test_command="$3"
  local task_output="$OUTPUT_ROOT/$task_name"
  local workspace="$task_output/workspace"
  local socket_path="$task_output/daemon.sock"
  mkdir -p "$workspace"

  "$setup_function" "$workspace"
  git -C "$workspace" init -q
  git -C "$workspace" add .
  git -C "$workspace" -c user.name=Coddy -c user.email=coddy@example.invalid commit -q -m "fixture"

  CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$workspace" \
    "$CODDY_BIN" runtime serve > "$task_output/runtime.log" 2>&1 &
  local runtime_pid=$!
  runtime_pids+=("$runtime_pid")
  wait_for_socket "$socket_path"

  CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$workspace" \
    "$CODDY_BIN" model select --provider "$MODEL_PROVIDER" --name "$MODEL_NAME" \
    > "$task_output/model-select.txt" 2>&1

  local issue
  issue="$(cat "$workspace/ISSUE.md")"
  local prompt
  prompt="Solve this SWE-bench-style bug in the current workspace. Use no more than 8 tools, inspect source and tests, do not edit files directly, and return only a unified diff patch with diff --git headers. Do not include prose, markdown fences, summaries, or next steps. Issue:\n\n${issue}"

  local start_ms end_ms duration_ms ask_exit apply_exit test_exit patch_extracted resolved
  start_ms="$(date +%s%3N)"
  set +e
  CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$workspace" \
    "$CODDY_BIN" ask "$prompt" > "$task_output/answer.md" 2> "$task_output/ask-stderr.log"
  ask_exit=$?
  set -e
  end_ms="$(date +%s%3N)"
  duration_ms=$((end_ms - start_ms))

  extract_patch "$task_output/answer.md" "$task_output/model.patch"
  if [[ -s "$task_output/model.patch" ]]; then
    patch_extracted=1
  else
    patch_extracted=0
  fi

  set +e
  git -C "$workspace" apply "$task_output/model.patch" > "$task_output/git-apply.log" 2>&1
  apply_exit=$?
  if [[ "$apply_exit" -ne 0 ]]; then
    git -C "$workspace" apply --recount "$task_output/model.patch" >> "$task_output/git-apply.log" 2>&1
    apply_exit=$?
  fi
  if [[ "$apply_exit" -eq 0 ]]; then
    (cd "$workspace" && bash -lc "$test_command") > "$task_output/test.log" 2>&1
    test_exit=$?
  else
    test_exit=99
  fi
  set -e

  if [[ "$ask_exit" -eq 0 && "$patch_extracted" -eq 1 && "$apply_exit" -eq 0 && "$test_exit" -eq 0 ]]; then
    resolved=1
  else
    resolved=0
  fi

  CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$workspace" \
    "$CODDY_BIN" session events --after 0 > "$task_output/events.json" 2>&1 || true

  local tool_count tool_failures provider_errors pseudo_tool_count secret_hits answer_chars metrics_text
  if jq -e '.events' "$task_output/events.json" >/dev/null 2>&1; then
    tool_count="$(jq '[.events[].event.ToolCompleted? | select(.)] | length' "$task_output/events.json")"
    tool_failures="$(jq '[.events[].event.ToolCompleted? | select(.status == "Failed")] | length' "$task_output/events.json")"
  else
    tool_count=0
    tool_failures=0
  fi
  metrics_text="$task_output/metrics-text.log"
  cat "$task_output/answer.md" "$task_output/ask-stderr.log" > "$metrics_text"

  provider_errors="$(grep -Eci 'Coddy could not|get a response from|Provider returned error|timed out reading response|daemon request timed out' "$metrics_text" || true)"
  pseudo_tool_count="$(grep -Eci 'Tool observations:|Tool call [0-9]+:|Tool [0-9]+([*/][0-9]+)?[*[:space:]]*:|Call [0-9]+([ /]of[ /]|/)[0-9]+|textual tool-call attempt|```tool|<filesystem\.|<read_file|filesystem\\.(read_file|list_files|search_files|apply_edit)[[:space:]]*\\{|\"file_path\"|\"max_bytes\"|\"max_entries\"|\"max_matches\"' "$task_output/answer.md" || true)"
  secret_hits="$(grep -Eci '(sk-or-[A-Za-z0-9_-]{12,}|nvapi-[A-Za-z0-9_-]{12,}|AIza[0-9A-Za-z_-]{20,}|OPENROUTER_API_KEY=[^[:space:]]+|NVIDIA_API_KEY=[^[:space:]]+)' "$metrics_text" || true)"
  answer_chars="$(wc -m < "$task_output/answer.md" | tr -d ' ')"

  jq -cn \
    --arg task "$task_name" \
    --arg outputPath "$task_output" \
    --argjson askExit "$ask_exit" \
    --argjson durationMs "$duration_ms" \
    --argjson patchExtracted "$patch_extracted" \
    --argjson applyExit "$apply_exit" \
    --argjson testExit "$test_exit" \
    --argjson resolved "$resolved" \
    --argjson toolCount "$tool_count" \
    --argjson toolFailures "$tool_failures" \
    --argjson providerErrors "$provider_errors" \
    --argjson pseudoToolMarkup "$pseudo_tool_count" \
    --argjson secretHits "$secret_hits" \
    --argjson answerChars "$answer_chars" \
    '{
      task: $task,
      askExit: $askExit,
      durationMs: $durationMs,
      patchExtracted: $patchExtracted,
      applyExit: $applyExit,
      testExit: $testExit,
      resolved: $resolved,
      toolCount: $toolCount,
      toolFailures: $toolFailures,
      providerErrors: $providerErrors,
      pseudoToolMarkup: $pseudoToolMarkup,
      secretHits: $secretHits,
      answerChars: $answerChars,
      outputPath: $outputPath
    }' >> "$SUMMARY_JSONL"

  kill "$runtime_pid" >/dev/null 2>&1 || true
}

printf 'output_root=%s\n' "$OUTPUT_ROOT"
printf 'model=%s/%s\n' "$MODEL_PROVIDER" "$MODEL_NAME"

run_task "python_slugify" setup_python_slug_task "python3 -m unittest -q"
run_task "node_duration" setup_node_duration_task "npm test"
run_task "rust_safe_join" setup_rust_path_task "cargo test --quiet"

jq -s '
  {
    outputRoot: $outputRoot,
    model: { provider: $provider, name: $model },
    tasks: length,
    resolved: map(.resolved // 0) | add,
    resolutionRate: ((map(.resolved // 0) | add) / (length | if . == 0 then 1 else . end) * 100),
    patchExtracted: map(.patchExtracted // 0) | add,
    applyFailures: map(select((.applyExit // 0) != 0)) | length,
    testFailures: map(select((.testExit // 0) != 0)) | length,
    providerErrors: map(.providerErrors // 0) | add,
    toolFailures: map(.toolFailures // 0) | add,
    pseudoToolMarkup: map(.pseudoToolMarkup // 0) | add,
    secretHits: map(.secretHits // 0) | add,
    averageDurationMs: ((map(.durationMs // 0) | add) / (length | if . == 0 then 1 else . end)),
    cases: .
  }
' --arg outputRoot "$OUTPUT_ROOT" --arg provider "$MODEL_PROVIDER" --arg model "$MODEL_NAME" \
  "$SUMMARY_JSONL" > "$OUTPUT_ROOT/summary.json"

cat "$OUTPUT_ROOT/summary.json"
