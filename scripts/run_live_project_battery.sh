#!/usr/bin/env bash
set -Eeuo pipefail

CODDY_BIN="${CODDY_BIN:-target/debug/coddy}"
MODEL_PROVIDER="${MODEL_PROVIDER:-openrouter}"
MODEL_NAME="${MODEL_NAME:-deepseek/deepseek-v4-flash}"
DOCUMENTS_ROOT="${DOCUMENTS_ROOT:-/home/aethyr/Documents}"
OUTPUT_ROOT="${OUTPUT_ROOT:-/tmp/coddy-live-project-battery-$(date +%Y%m%d-%H%M%S)}"
SOCKET_ROOT="${SOCKET_ROOT:-/tmp/coddy-live-battery-sockets-$$}"
CODDY_CLIENT_REQUEST_TIMEOUT_MS="${CODDY_CLIENT_REQUEST_TIMEOUT_MS:-420000}"
export CODDY_CLIENT_REQUEST_TIMEOUT_MS
PROJECT_FILTER="${PROJECT_FILTER:-}"
CATEGORY_FILTER="${CATEGORY_FILTER:-}"
PROJECTS_CSV="${PROJECTS_CSV:-}"

PROJECTS=(
  "$DOCUMENTS_ROOT/apex"
  "$DOCUMENTS_ROOT/Guardian"
  "$DOCUMENTS_ROOT/maker"
  "$DOCUMENTS_ROOT/visionclip"
  "$DOCUMENTS_ROOT/coddy"
  "$DOCUMENTS_ROOT/pytorch"
)

PROMPT_CATEGORIES=(
  "architecture"
  "code_review_security"
  "codegen_tdd"
  "tests_docs_ci"
  "deep_entrypoint_trace"
  "cross_stack_codegen"
  "performance_complexity"
  "adversarial_security"
  "research_capability"
  "database_query_review"
  "ci_cd_release_automation"
  "long_context_reverse_engineering"
  "scientific_math_reasoning"
  "agentic_coding_quality"
)

prompt_for_category() {
  case "$1" in
    architecture)
      printf '%s' 'Analise profundamente esta codebase em modo read-only. Use no máximo 8 tools. Use max_bytes em leituras grandes. Mapeie estrutura, stack, entrypoints, módulos principais e fluxo de execução. Responda com evidências de arquivos lidos, arquitetura, riscos técnicos, complexidade/Big-O quando aplicável e 5 melhorias priorizadas. Não edite arquivos.'
      ;;
    code_review_security)
      printf '%s' 'Faça um code review criterioso e uma revisão de segurança em modo read-only. Use no máximo 10 tools e max_bytes em leituras grandes. Leia arquivos atuais de implementação e testes antes de classificar severidade. Não leia arquivos de secrets como .env. Liste achados por severidade, evidências, impacto, testes ausentes e correções recomendadas. Não classifique High/Critical sem evidência direta de código lido; marque como potencial/não verificado quando a evidência for parcial. Não edite arquivos.'
      ;;
    codegen_tdd)
      printf '%s' 'Gere uma proposta de implementação TDD para uma melhoria pequena e realista nesta codebase. Use no máximo 6 tools e max_bytes em leituras grandes para entender padrões locais. Não edite arquivos. Entregue testes primeiro, depois patch conceitual com código compilável, riscos, validação e análise de complexidade.'
      ;;
    tests_docs_ci)
      printf '%s' 'Revise qualidade de testes, documentação e CI/CD desta codebase em modo read-only. Use no máximo 6 tools e max_bytes em leituras grandes. Localize manifests, scripts, docs e testes. Responda com lacunas, métricas qualitativas, recomendações de testes, documentação a atualizar e comandos de validação. Não edite arquivos.'
      ;;
    deep_entrypoint_trace)
      printf '%s' 'Faça uma análise profunda de fluxo de execução em modo read-only. Use no máximo 10 tools e max_bytes em leituras grandes. Identifique entrypoints, caminhos críticos, estados compartilhados, fluxos assíncronos, limites de contexto e riscos de concorrência. Cite somente arquivos realmente inspecionados e marque incertezas.'
      ;;
    cross_stack_codegen)
      printf '%s' 'Gere uma proposta de código TDD para uma melhoria cross-stack pequena, mantendo arquitetura limpa. Use no máximo 8 tools e max_bytes em leituras grandes. Não edite arquivos. Entregue: teste falhando, patch conceitual em diff ou blocos por arquivo, validação, riscos de integração e rollback.'
      ;;
    performance_complexity)
      printf '%s' 'Analise performance, Big-O, uso de memória, gargalos de I/O e baixa latência nesta codebase em modo read-only. Use no máximo 8 tools e max_bytes em leituras grandes. Traga evidências de arquivos lidos, hotspots prováveis, métricas a coletar, benchmarks sugeridos e melhorias priorizadas.'
      ;;
    adversarial_security)
      printf '%s' 'Faça uma revisão adversarial de segurança em modo read-only. Use no máximo 8 tools e max_bytes em leituras grandes. Não leia secrets como .env. Avalie prompt injection, permissões, execução de comandos, exposição de chaves, path traversal e supply chain. Liste achados por severidade com evidência.'
      ;;
    research_capability)
      printf '%s' 'Avalie a capacidade desta ferramenta/codebase de realizar pesquisa técnica atualizada. Use no máximo 6 tools. Se não houver ferramenta de web/research disponível, diga explicitamente essa limitação em vez de inventar fontes. Verifique docs/código de tools e proponha integração segura para pesquisa com citações.'
      ;;
    database_query_review)
      printf '%s' 'Analise banco de dados, queries, migrations, modelos, índices, vetorização/search e riscos de consistência nesta codebase em modo read-only. Use no máximo 8 tools e max_bytes em leituras grandes. Se o projeto não possuir banco de dados, demonstre como verificou isso e proponha o menor próximo passo mensurável. Cite arquivos realmente inspecionados, Big-O/complexidade de queries quando aplicável e testes/benchmarks necessários. Não edite arquivos.'
      ;;
    ci_cd_release_automation)
      printf '%s' 'Revise CI/CD, automações de build/release, scripts de instalação, empacotamento e supply chain em modo read-only. Use no máximo 8 tools e max_bytes em leituras grandes. Localize manifests, workflows, scripts e docs. Responda com riscos, gaps, comandos de validação, sugestões de hardening e um plano incremental de automação com rollback. Não edite arquivos.'
      ;;
    long_context_reverse_engineering)
      printf '%s' 'Faça reverse engineering de alto nível desta codebase como faria um coding agent sênior em contexto longo. Use no máximo 12 tools e max_bytes em leituras grandes. Priorize entrypoints, contratos, tools, agentes, estado, execução assíncrona e integração frontend/backend. Entregue mapa de dependências, fluxo crítico, riscos arquiteturais, incertezas explícitas e próximos arquivos de maior sinal. Não edite arquivos.'
      ;;
    scientific_math_reasoning)
      printf '%s' 'Avalie se esta codebase possui componentes de matemática, física, ML, processamento de dados, vetores/matrizes, métricas, ranking, embeddings ou inferência. Use no máximo 8 tools e max_bytes em leituras grandes. Quando encontrar algoritmos, explique fórmulas, Big-O, estabilidade numérica e testes de propriedades necessários. Se não encontrar, diga como verificou e proponha integração segura de benchmarks matemáticos/científicos. Não edite arquivos.'
      ;;
    agentic_coding_quality)
      printf '%s' 'Avalie a capacidade desta codebase como ferramenta de agentic coding comparável a Codex CLI e Claude Code. Use no máximo 10 tools e max_bytes em leituras grandes. Inspecione agentes, tools, prompts, políticas, approvals, sandbox, evals, memória/contexto, roteamento e geração de código. Responda com matriz de capacidades, lacunas mensuráveis, métricas atuais, benchmarks necessários e plano TDD priorizado. Não edite arquivos.'
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

unique_path_mentions() {
  local answer_path="$1"
  grep -Eo '([[:alnum:]_.-]+/)+[[:alnum:]_.-]+\.(rs|ts|tsx|js|jsx|mjs|json|toml|md|py|yml|yaml|sh|sql|html|css)' "$answer_path" \
    | sort -u \
    | wc -l \
    | tr -d ' ' || true
}

score_answer() {
  local ask_exit="$1"
  local final_phase="$2"
  local tool_count="$3"
  local tool_failures="$4"
  local answer_chars="$5"
  local provider_errors="$6"
  local pseudo_tool_markup="$7"
  local incomplete_answers="$8"
  local secret_hits="$9"
  local grounding_checks="${10}"
  local unverified_claims="${11}"
  local path_mentions="${12}"
  local category="${13}"

  local score=100
  if [[ "$ask_exit" -ne 0 ]]; then score=$((score - 40)); fi
  if [[ "$final_phase" != "Completed" ]]; then score=$((score - 40)); fi
  score=$((score - provider_errors * 25))
  local tool_failure_penalty=10
  if [[ "$category" == "adversarial_security" ]]; then
    tool_failure_penalty=5
  fi
  score=$((score - tool_failures * tool_failure_penalty))
  score=$((score - pseudo_tool_markup * 30))
  score=$((score - incomplete_answers * 20))
  score=$((score - grounding_checks * 15))
  local unverified_penalty=$((unverified_claims * 5))
  if [[ "$category" == "adversarial_security" || "$category" == "code_review_security" || "$category" == "codegen_tdd" || "$category" == "long_context_reverse_engineering" || "$category" == "agentic_coding_quality" ]]; then
    unverified_penalty=$((unverified_claims * 2))
    local unverified_penalty_cap=10
    if [[ "$category" == "code_review_security" ]]; then
      unverified_penalty_cap=15
    elif [[ "$category" == "agentic_coding_quality" || "$category" == "long_context_reverse_engineering" ]]; then
      unverified_penalty_cap=15
    elif [[ "$category" == "codegen_tdd" ]]; then
      unverified_penalty_cap=10
    fi
    if [[ "$unverified_penalty" -gt "$unverified_penalty_cap" ]]; then
      unverified_penalty="$unverified_penalty_cap"
    fi
  fi
  score=$((score - unverified_penalty))
  if [[ "$secret_hits" -gt 0 ]]; then score=0; fi
  if [[ "$answer_chars" -lt 800 ]]; then score=$((score - 15)); fi
  if [[ "$category" != "research_capability" && "$tool_count" -eq 0 ]]; then score=$((score - 15)); fi
  if [[ "$category" != "research_capability" && "$path_mentions" -eq 0 ]]; then score=$((score - 10)); fi
  if [[ "$score" -lt 0 ]]; then score=0; fi
  if [[ "$score" -gt 100 ]]; then score=100; fi
  printf '%s' "$score"
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

mkdir -p "$OUTPUT_ROOT" "$SOCKET_ROOT"
SUMMARY_JSONL="$OUTPUT_ROOT/summary.jsonl"

write_summary() {
  jq -s '
    def completed_records:
      map(select((.status // "completed") == "completed"));
    def missing_records:
      map(select(.status == "missing"));
    def sum_of($field):
      map(.[$field] // 0) | add // 0;
    def average_of($field):
      if length == 0 then 0 else (sum_of($field) / length) end;

    . as $records
    | ($records | completed_records) as $runs
    | ($records | missing_records) as $missing
    | {
      outputRoot: $outputRoot,
      model: { provider: $provider, name: $model },
      clientRequestTimeoutMs: ($clientRequestTimeoutMs | tonumber),
      records: ($records | length),
      prompts: ($runs | length),
      missingProjects: $missing,
      cliCompleted: ($runs | map(select(.askExit == 0)) | length),
      agentCompleted: ($runs | map(select(.finalPhase == "Completed")) | length),
      providerErrors: ($runs | sum_of("providerErrors")),
      incompleteAnswers: ($runs | sum_of("incompleteAnswers")),
      toolCount: ($runs | sum_of("toolCount")),
      toolFailures: ($runs | sum_of("toolFailures")),
      permissionCount: ($runs | sum_of("permissionCount")),
      secretHits: ($runs | sum_of("secretHits")),
      groundingChecks: ($runs | sum_of("groundingChecks")),
      unverifiedClaims: ($runs | sum_of("unverifiedClaims")),
      pseudoToolMarkup: ($runs | sum_of("pseudoToolMarkup")),
      averageQualityScore: ($runs | average_of("qualityScore")),
      qualityPassed: ($runs | map(select((.qualityScore // 0) >= 80 and .finalPhase == "Completed" and .providerErrors == 0 and .secretHits == 0 and .pseudoToolMarkup == 0 and .incompleteAnswers == 0)) | length),
      averageDurationMs: ($runs | average_of("durationMs")),
      byCategory: (
        $runs |
        group_by(.category) |
        map({
          category: .[0].category,
          prompts: length,
          agentCompleted: map(select(.finalPhase == "Completed")) | length,
          providerErrors: sum_of("providerErrors"),
          toolFailures: sum_of("toolFailures"),
          groundingChecks: sum_of("groundingChecks"),
          secretHits: sum_of("secretHits"),
          incompleteAnswers: sum_of("incompleteAnswers"),
          pseudoToolMarkup: sum_of("pseudoToolMarkup"),
          averageQualityScore: average_of("qualityScore")
        })
      ),
      byProject: (
        $runs |
        group_by(.project) |
        map({
          project: .[0].project,
          prompts: length,
          cliCompleted: map(select(.askExit == 0)) | length,
          agentCompleted: map(select(.finalPhase == "Completed")) | length,
          providerErrors: sum_of("providerErrors"),
          incompleteAnswers: sum_of("incompleteAnswers"),
          toolCount: sum_of("toolCount"),
          toolFailures: sum_of("toolFailures"),
          permissionCount: sum_of("permissionCount"),
          secretHits: sum_of("secretHits"),
          groundingChecks: sum_of("groundingChecks"),
          pseudoToolMarkup: sum_of("pseudoToolMarkup"),
          averageQualityScore: average_of("qualityScore"),
          averageDurationMs: average_of("durationMs")
        })
      )
    }
  ' --arg outputRoot "$OUTPUT_ROOT" --arg provider "$MODEL_PROVIDER" --arg model "$MODEL_NAME" \
    --arg clientRequestTimeoutMs "$CODDY_CLIENT_REQUEST_TIMEOUT_MS" \
    "$SUMMARY_JSONL" > "$OUTPUT_ROOT/summary.json"

  cat "$OUTPUT_ROOT/summary.json"
}

if [[ "${CODDY_LIVE_PROJECT_BATTERY_SUMMARY_ONLY:-0}" == "1" ]]; then
  mkdir -p "$OUTPUT_ROOT"
  if [[ ! -f "$SUMMARY_JSONL" ]]; then
    : > "$SUMMARY_JSONL"
  fi
  write_summary
  exit 0
fi

: > "$SUMMARY_JSONL"

if [[ -n "$PROJECTS_CSV" ]]; then
  PROJECTS=()
  IFS=',' read -ra project_items <<< "$PROJECTS_CSV"
  for project_item in "${project_items[@]}"; do
    project_item="$(printf '%s' "$project_item" | xargs)"
    if [[ -n "$project_item" ]]; then
      PROJECTS+=("$project_item")
    fi
  done
fi

runtime_pids=()
cleanup() {
  for pid in "${runtime_pids[@]:-}"; do
    if kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
    fi
  done
  rm -rf "$SOCKET_ROOT"
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
    jq -cn --arg project "$project_name" --arg project_path "$project_path" \
      '{status: "missing", project: $project, projectPath: $project_path}' >> "$SUMMARY_JSONL"
    continue
  fi

  safe_project_name="$(printf '%s' "$project_name" | tr -cs 'A-Za-z0-9._-' '_')"
  project_output="$OUTPUT_ROOT/$safe_project_name"
  socket_path="$SOCKET_ROOT/$safe_project_name.sock"
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
      "$CODDY_BIN" ask "$prompt" > "$prompt_output/answer.md" 2> "$prompt_output/ask-stderr.log"
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

    metrics_text="$prompt_output/metrics-text.log"
    cat "$prompt_output/answer.md" "$prompt_output/ask-stderr.log" > "$metrics_text"

    answer_chars="$(wc -m < "$prompt_output/answer.md" | tr -d ' ')"
    provider_error_count="$(grep -Eci 'Coddy could not|get a response from|Provider returned error|timed out reading response|could not build a valid chat request|did not return a valid response|daemon request timed out' "$metrics_text" || true)"
    pseudo_tool_count="$(grep -Eci 'Tool observations:|Tool call [0-9]+:|Tool [0-9]+([*/][0-9]+)?[*[:space:]]*:|Call [0-9]+([ /]of[ /]|/)[0-9]+|Chamada [0-9]+[[:space:]]*:|textual tool-call attempt|Request:[[:space:]]*(filesystem\\.(read_file|list_files|search_files|apply_edit)|shell\\.run|subagent\\.)|```tool|DSML.*(tool_calls|invoke name=)|<filesystem\.|<read_file|<function name="(filesystem\\.(read_file|list_files|search_files|apply_edit)|shell\\.run|subagent\\.)|<param name="(path|file_path|query)"|filesystem\\.(read_file|list_files|search_files|apply_edit)[[:space:]]*\\{|```json[[:space:]]*\{[[:space:]]*"tool|\"calls\"[[:space:]]*:|\"action\"[[:space:]]*:[[:space:]]*\"(filesystem\\.(read_file|list_files|search_files|apply_edit)|shell\\.run|subagent\\.)|\"parameters\"[[:space:]]*:|\"name\"[[:space:]]*:[[:space:]]*\"(filesystem\\.(read_file|list_files|search_files|apply_edit)|shell\\.run|subagent\\.)|\"file_path\"|\"max_bytes\"|\"max_entries\"|\"max_matches\"' "$prompt_output/answer.md" || true)"
    incomplete_answer_count="$(grep -Eci '(^|[[:space:]])(vou continuar|vou agora|vou focar|vou me concentrar|inspecionarei|analisarei|verificarei|mapearei|preciso identificar|a revis[aã]o continua|continuando a inspe[cç][aã]o|continuando a explora[cç][aã]o|continuando a revis[aã]o|i will continue|i will now|i'\''ll now|i'\''ll search|let me now|let me inspect|continuarei|preciso continuar|did not return a valid response|daemon request timed out|resposta parcial|partial answer|requires approval before)' "$metrics_text" || true)"
    secret_hits="$(scan_secret_hits "$metrics_text")"
    grounding_check_count="$(grep -Eci 'Coddy grounding check|Treat the conclusion below as unverified' "$prompt_output/answer.md" || true)"
    unverified_claim_count="$(grep -Eci 'unverified|não verificado|nao verificado|não foi lido|nao foi lido|desconhecido|unknown|parcial|partial' "$prompt_output/answer.md" || true)"
    path_mention_count="$(unique_path_mentions "$prompt_output/answer.md")"
    quality_score="$(score_answer "$ask_exit" "$final_phase" "$tool_count" "$tool_failures" "$answer_chars" "$provider_error_count" "$pseudo_tool_count" "$incomplete_answer_count" "$secret_hits" "$grounding_check_count" "$unverified_claim_count" "$path_mention_count" "$category")"

    jq -cn \
      --arg project "$project_name" \
      --arg projectPath "$project_path" \
      --arg status "completed" \
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
      --argjson groundingChecks "$grounding_check_count" \
      --argjson unverifiedClaims "$unverified_claim_count" \
      --argjson pathMentions "$path_mention_count" \
      --argjson qualityScore "$quality_score" \
      --argjson failureMessagePresent "$failure_message_present" \
      '{
        project: $project,
        projectPath: $projectPath,
        status: $status,
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
        groundingChecks: $groundingChecks,
        unverifiedClaims: $unverifiedClaims,
        pathMentions: $pathMentions,
        qualityScore: $qualityScore,
        failureMessagePresent: $failureMessagePresent,
        outputPath: $outputPath
      }' >> "$SUMMARY_JSONL"
  done

  kill "$runtime_pid" >/dev/null 2>&1 || true
done

write_summary
