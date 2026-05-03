#!/usr/bin/env bash
set -Eeuo pipefail

CODDY_BIN="${CODDY_BIN:-target/debug/coddy}"
MODEL_PROVIDER="${MODEL_PROVIDER:-openrouter}"
MODEL_NAME="${MODEL_NAME:-deepseek/deepseek-v4-flash}"
DOCUMENTS_ROOT="${DOCUMENTS_ROOT:-/home/aethyr/Documents}"
OUTPUT_ROOT="${OUTPUT_ROOT:-/tmp/coddy-live-project-battery-$(date +%Y%m%d-%H%M%S)}"
CODDY_CLIENT_REQUEST_TIMEOUT_MS="${CODDY_CLIENT_REQUEST_TIMEOUT_MS:-300000}"
export CODDY_CLIENT_REQUEST_TIMEOUT_MS
PROJECT_FILTER="${PROJECT_FILTER:-}"
CATEGORY_FILTER="${CATEGORY_FILTER:-}"

PROJECTS=(
  "$DOCUMENTS_ROOT/apex"
  "$DOCUMENTS_ROOT/Guardian"
  "$DOCUMENTS_ROOT/maker"
  "$DOCUMENTS_ROOT/visionclip"
  "$DOCUMENTS_ROOT/coddy"
)

PROMPT_CATEGORIES=(
  "architecture"
  "code_review_security"
  "codegen_tdd"
  "tests_docs_ci"
)

prompt_for_category() {
  case "$1" in
    architecture)
      printf '%s' 'Analise profundamente esta codebase em modo read-only. Use no máximo 8 tools. Use max_bytes em leituras grandes. Mapeie estrutura, stack, entrypoints, módulos principais e fluxo de execução. Responda com evidências de arquivos lidos, arquitetura, riscos técnicos, complexidade/Big-O quando aplicável e 5 melhorias priorizadas. Não edite arquivos.'
      ;;
    code_review_security)
      printf '%s' 'Faça um code review criterioso e uma revisão de segurança em modo read-only. Use no máximo 8 tools e max_bytes em leituras grandes. Leia arquivos atuais de implementação e testes. Não leia arquivos de secrets como .env. Liste achados por severidade, evidências, impacto, testes ausentes e correções recomendadas. Não edite arquivos.'
      ;;
    codegen_tdd)
      printf '%s' 'Gere uma proposta de implementação TDD para uma melhoria pequena e realista nesta codebase. Use no máximo 6 tools e max_bytes em leituras grandes para entender padrões locais. Não edite arquivos. Entregue testes primeiro, depois patch conceitual com código compilável, riscos, validação e análise de complexidade.'
      ;;
    tests_docs_ci)
      printf '%s' 'Revise qualidade de testes, documentação e CI/CD desta codebase em modo read-only. Use no máximo 6 tools e max_bytes em leituras grandes. Localize manifests, scripts, docs e testes. Responda com lacunas, métricas qualitativas, recomendações de testes, documentação a atualizar e comandos de validação. Não edite arquivos.'
      ;;
    *)
      return 1
      ;;
  esac
}

json_string() {
  jq -Rn --arg value "$1" '$value'
}

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

scan_secret_hits() {
  local answer_path="$1"
  grep -Eci '(sk-or-[A-Za-z0-9_-]{12,}|nvapi-[A-Za-z0-9_-]{12,}|AIza[0-9A-Za-z_-]{20,}|OPENROUTER_API_KEY=[^[:space:]]+|NVIDIA_API_KEY=[^[:space:]]+)' "$answer_path" || true
}

filter_contains() {
  local filter="$1"
  local value="$2"
  if [[ -z "$filter" ]]; then
    return 0
  fi
  local item
  IFS=',' read -ra items <<< "$filter"
  for item in "${items[@]}"; do
    item="$(printf '%s' "$item" | xargs)"
    if [[ "$item" == "$value" ]]; then
      return 0
    fi
  done
  return 1
}

mkdir -p "$OUTPUT_ROOT"
SUMMARY_JSONL="$OUTPUT_ROOT/summary.jsonl"
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

printf 'output_root=%s\n' "$OUTPUT_ROOT"
printf 'model=%s/%s\n' "$MODEL_PROVIDER" "$MODEL_NAME"
printf 'client_request_timeout_ms=%s\n' "$CODDY_CLIENT_REQUEST_TIMEOUT_MS"
printf 'project_filter=%s\n' "${PROJECT_FILTER:-<all>}"
printf 'category_filter=%s\n' "${CATEGORY_FILTER:-<all>}"

for project_path in "${PROJECTS[@]}"; do
  project_name="$(basename "$project_path")"
  if ! filter_contains "$PROJECT_FILTER" "$project_name"; then
    continue
  fi

  if [[ ! -d "$project_path" ]]; then
    jq -cn --arg project_path "$project_path" \
      '{projectPath: $project_path, status: "missing"}' >> "$SUMMARY_JSONL"
    continue
  fi

  safe_project_name="$(printf '%s' "$project_name" | tr -cs 'A-Za-z0-9._-' '_')"
  project_output="$OUTPUT_ROOT/$safe_project_name"
  socket_path="$project_output/daemon.sock"
  mkdir -p "$project_output"

  CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$project_path" \
    "$CODDY_BIN" runtime serve > "$project_output/runtime.log" 2>&1 &
  runtime_pid=$!
  runtime_pids+=("$runtime_pid")

  if ! wait_for_socket "$socket_path"; then
    jq -cn --arg project "$project_name" --arg project_path "$project_path" \
      '{project: $project, projectPath: $project_path, status: "runtime_socket_timeout"}' >> "$SUMMARY_JSONL"
    continue
  fi

  CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$project_path" \
    "$CODDY_BIN" model select --provider "$MODEL_PROVIDER" --name "$MODEL_NAME" \
    > "$project_output/model-select.txt" 2>&1
  CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$project_path" \
    "$CODDY_BIN" session tools > "$project_output/tools.json" 2>&1

  for category in "${PROMPT_CATEGORIES[@]}"; do
    if ! filter_contains "$CATEGORY_FILTER" "$category"; then
      continue
    fi

    prompt="$(prompt_for_category "$category")"
    prompt_output="$project_output/$category"
    mkdir -p "$prompt_output"
    printf '%s\n' "$prompt" > "$prompt_output/prompt.txt"

    CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$project_path" \
      "$CODDY_BIN" session new > "$prompt_output/session-new.txt" 2>&1 || true

    start_ms="$(date +%s%3N)"
    set +e
    CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$project_path" \
      "$CODDY_BIN" ask "$prompt" > "$prompt_output/answer.md" 2>&1
    ask_exit=$?
    set -e
    end_ms="$(date +%s%3N)"
    duration_ms=$((end_ms - start_ms))

    CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$project_path" \
      "$CODDY_BIN" session events --after 0 > "$prompt_output/events.json" 2>&1 || true
    CODDY_DAEMON_SOCKET="$socket_path" CODDY_WORKSPACE="$project_path" \
      "$CODDY_BIN" session snapshot > "$prompt_output/snapshot.json" 2>&1 || true

    if jq -e '.events' "$prompt_output/events.json" >/dev/null 2>&1; then
      tool_count="$(jq '[.events[].event.ToolCompleted? | select(.)] | length' "$prompt_output/events.json")"
      tool_failures="$(jq '[.events[].event.ToolCompleted? | select(.status == "Failed")] | length' "$prompt_output/events.json")"
      permission_count="$(jq '[.events[].event.PermissionRequested? | select(.)] | length' "$prompt_output/events.json")"
      intent="$(jq -r '[.events[].event.IntentDetected.intent? | select(.)] | last // ""' "$prompt_output/events.json")"
      final_phase="$(jq -r '[.events[].event.AgentRunUpdated? | select(.summary.last_phase? != null) | .summary.last_phase] | last // ""' "$prompt_output/events.json")"
      failure_message_present="$(jq '[.events[].event.AgentRunUpdated? | select(.summary.failure_message? != null)] | length' "$prompt_output/events.json")"
    else
      tool_count=0
      tool_failures=0
      permission_count=0
      intent=""
      final_phase=""
      failure_message_present=0
    fi

    answer_chars="$(wc -m < "$prompt_output/answer.md" | tr -d ' ')"
    provider_error_count="$(grep -Eci 'Coddy could not|get a response from|Provider returned error|timed out reading response|could not build a valid chat request|did not return a valid response|daemon request timed out' "$prompt_output/answer.md" || true)"
    pseudo_tool_count="$(grep -Eci 'Tool observations:|Tool call [0-9]+:|<filesystem\.|<read_file|```json[[:space:]]*\{[[:space:]]*"tool' "$prompt_output/answer.md" || true)"
    incomplete_answer_count="$(grep -Eci '(^|[[:space:]])(vou continuar|vou agora|i will continue|i will now|continuarei|preciso continuar|did not return a valid response|daemon request timed out|resposta parcial|partial answer|requires approval before)' "$prompt_output/answer.md" || true)"
    secret_hits="$(scan_secret_hits "$prompt_output/answer.md")"

    jq -cn \
      --arg project "$project_name" \
      --arg projectPath "$project_path" \
      --arg category "$category" \
      --arg intent "$intent" \
      --arg finalPhase "$final_phase" \
      --arg outputPath "$prompt_output" \
      --argjson askExit "$ask_exit" \
      --argjson durationMs "$duration_ms" \
      --argjson toolCount "$tool_count" \
      --argjson toolFailures "$tool_failures" \
      --argjson permissionCount "$permission_count" \
      --argjson answerChars "$answer_chars" \
      --argjson providerErrors "$provider_error_count" \
      --argjson pseudoToolMarkup "$pseudo_tool_count" \
      --argjson incompleteAnswers "$incomplete_answer_count" \
      --argjson secretHits "$secret_hits" \
      --argjson failureMessagePresent "$failure_message_present" \
      '{
        project: $project,
        projectPath: $projectPath,
        category: $category,
        askExit: $askExit,
        durationMs: $durationMs,
        intent: $intent,
        finalPhase: $finalPhase,
        toolCount: $toolCount,
        toolFailures: $toolFailures,
        permissionCount: $permissionCount,
        answerChars: $answerChars,
        providerErrors: $providerErrors,
        pseudoToolMarkup: $pseudoToolMarkup,
        incompleteAnswers: $incompleteAnswers,
        secretHits: $secretHits,
        failureMessagePresent: $failureMessagePresent,
        outputPath: $outputPath
      }' >> "$SUMMARY_JSONL"
  done

  kill "$runtime_pid" >/dev/null 2>&1 || true
done

jq -s '
  {
    outputRoot: $outputRoot,
    model: { provider: $provider, name: $model },
    clientRequestTimeoutMs: ($clientRequestTimeoutMs | tonumber),
    prompts: length,
    cliCompleted: map(select(.askExit == 0)) | length,
    agentCompleted: map(select(.finalPhase == "Completed")) | length,
    providerErrors: map(.providerErrors // 0) | add,
    incompleteAnswers: map(.incompleteAnswers // 0) | add,
    toolCount: map(.toolCount // 0) | add,
    toolFailures: map(.toolFailures // 0) | add,
    permissionCount: map(.permissionCount // 0) | add,
    secretHits: map(.secretHits // 0) | add,
    pseudoToolMarkup: map(.pseudoToolMarkup // 0) | add,
    averageDurationMs: ((map(.durationMs // 0) | add) / (length | if . == 0 then 1 else . end)),
    byProject: (
      group_by(.project) |
      map({
        project: .[0].project,
        prompts: length,
        cliCompleted: map(select(.askExit == 0)) | length,
        agentCompleted: map(select(.finalPhase == "Completed")) | length,
        providerErrors: map(.providerErrors // 0) | add,
        incompleteAnswers: map(.incompleteAnswers // 0) | add,
        toolCount: map(.toolCount // 0) | add,
        toolFailures: map(.toolFailures // 0) | add,
        permissionCount: map(.permissionCount // 0) | add,
        secretHits: map(.secretHits // 0) | add,
        averageDurationMs: ((map(.durationMs // 0) | add) / (length | if . == 0 then 1 else . end))
      })
    )
  }
' --arg outputRoot "$OUTPUT_ROOT" --arg provider "$MODEL_PROVIDER" --arg model "$MODEL_NAME" \
  --arg clientRequestTimeoutMs "$CODDY_CLIENT_REQUEST_TIMEOUT_MS" \
  "$SUMMARY_JSONL" > "$OUTPUT_ROOT/summary.json"

cat "$OUTPUT_ROOT/summary.json"
