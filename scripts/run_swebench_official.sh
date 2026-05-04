#!/usr/bin/env bash
set -Eeuo pipefail

CODDY_CACHE_ROOT="${CODDY_CACHE_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/coddy}"
OUTPUT_ROOT="${OUTPUT_ROOT:-$CODDY_CACHE_ROOT/swebench-official-$(date +%Y%m%d-%H%M%S)}"
SWE_BENCH_DATASET="${SWE_BENCH_DATASET:-princeton-nlp/SWE-bench_Lite}"
SWE_BENCH_SPLIT="${SWE_BENCH_SPLIT:-test}"
SWE_BENCH_PREDICTIONS="${SWE_BENCH_PREDICTIONS:-gold}"
SWE_BENCH_INSTANCE_IDS="${SWE_BENCH_INSTANCE_IDS:-sympy__sympy-20590}"
SWE_BENCH_MAX_WORKERS="${SWE_BENCH_MAX_WORKERS:-1}"
SWE_BENCH_RUN_ID="${SWE_BENCH_RUN_ID:-coddy-swebench-$(date +%Y%m%d-%H%M%S)}"
SWE_BENCH_CACHE_LEVEL="${SWE_BENCH_CACHE_LEVEL:-env}"
SWE_BENCH_CLEAN="${SWE_BENCH_CLEAN:-True}"
SWE_BENCH_MIN_FREE_GB="${SWE_BENCH_MIN_FREE_GB:-120}"
SWE_BENCH_WORKDIR="${SWE_BENCH_WORKDIR:-$OUTPUT_ROOT/work}"
SWE_BENCH_PREFLIGHT_ONLY="${SWE_BENCH_PREFLIGHT_ONLY:-0}"
SWE_BENCH_SKIP_DISK_CHECK="${SWE_BENCH_SKIP_DISK_CHECK:-0}"

REPORT_JSON="$OUTPUT_ROOT/report.json"
DOCKER_STDERR="$OUTPUT_ROOT/docker-info.stderr.log"
SWE_STDOUT="$OUTPUT_ROOT/swebench.stdout.log"
SWE_STDERR="$OUTPUT_ROOT/swebench.stderr.log"

mkdir -p "$OUTPUT_ROOT" "$SWE_BENCH_WORKDIR"

json_report() {
  local status="$1"
  local message="$2"
  local docker_accessible="$3"
  local docker_exit="$4"
  local free_gb="$5"
  local disk_ok="$6"
  local swebench_available="$7"
  local uv_available="$8"
  local command_json="$9"

  local docker_error=""
  if [[ -s "$DOCKER_STDERR" ]]; then
    docker_error="$(sed -n '1,8p' "$DOCKER_STDERR")"
  fi

  jq -n \
    --arg status "$status" \
    --arg message "$message" \
    --arg outputRoot "$OUTPUT_ROOT" \
    --arg workdir "$SWE_BENCH_WORKDIR" \
    --arg dataset "$SWE_BENCH_DATASET" \
    --arg split "$SWE_BENCH_SPLIT" \
    --arg predictions "$SWE_BENCH_PREDICTIONS" \
    --arg instanceIds "$SWE_BENCH_INSTANCE_IDS" \
    --arg runId "$SWE_BENCH_RUN_ID" \
    --arg cacheLevel "$SWE_BENCH_CACHE_LEVEL" \
    --arg clean "$SWE_BENCH_CLEAN" \
    --arg minFreeGb "$SWE_BENCH_MIN_FREE_GB" \
    --arg freeGb "$free_gb" \
    --arg diskOk "$disk_ok" \
    --arg dockerAccessible "$docker_accessible" \
    --arg dockerExit "$docker_exit" \
    --arg dockerSocket "$(ls -l /var/run/docker.sock 2>/dev/null || true)" \
    --arg user "$(id -un 2>/dev/null || true)" \
    --arg groups "$(id -Gn 2>/dev/null || true)" \
    --arg dockerError "$docker_error" \
    --arg swebenchAvailable "$swebench_available" \
    --arg uvAvailable "$uv_available" \
    --argjson command "$command_json" \
    --arg stdout "$SWE_STDOUT" \
    --arg stderr "$SWE_STDERR" \
    '{
      kind: "coddy.swebenchOfficial",
      status: $status,
      message: $message,
      config: {
        dataset: $dataset,
        split: $split,
        predictions: $predictions,
        instanceIds: $instanceIds,
        maxWorkers: ($ENV.SWE_BENCH_MAX_WORKERS | tonumber? // 1),
        runId: $runId,
        cacheLevel: $cacheLevel,
        clean: $clean,
        minFreeGb: ($minFreeGb | tonumber)
      },
      environment: {
        outputRoot: $outputRoot,
        workdir: $workdir,
        user: $user,
        groups: $groups,
        docker: {
          accessible: ($dockerAccessible == "true"),
          exitCode: ($dockerExit | tonumber?),
          socket: $dockerSocket,
          stderrPreview: $dockerError
        },
        disk: {
          freeGb: ($freeGb | tonumber?),
          requiredGb: ($minFreeGb | tonumber),
          ok: ($diskOk == "true")
        },
        dependencies: {
          swebenchPythonPackage: ($swebenchAvailable == "true"),
          uv: ($uvAvailable == "true")
        }
      },
      command: $command,
      logs: {
        stdout: $stdout,
        stderr: $stderr
      }
    }' > "$REPORT_JSON"
}

docker_accessible="false"
docker_exit=0
if docker info >/dev/null 2> "$DOCKER_STDERR"; then
  docker_accessible="true"
else
  docker_exit=$?
fi

free_kb="$(df -Pk "$SWE_BENCH_WORKDIR" | awk 'NR == 2 {print $4}')"
free_gb="$((free_kb / 1024 / 1024))"
disk_ok="true"
if [[ "$SWE_BENCH_SKIP_DISK_CHECK" != "1" && "$free_gb" -lt "$SWE_BENCH_MIN_FREE_GB" ]]; then
  disk_ok="false"
fi

swebench_available="false"
if python3 -c 'import swebench' >/dev/null 2>&1; then
  swebench_available="true"
fi

uv_available="false"
if command -v uv >/dev/null 2>&1; then
  uv_available="true"
fi

if [[ "$swebench_available" == "true" ]]; then
  runner=(python3 -m swebench.harness.run_evaluation)
elif [[ "$uv_available" == "true" ]]; then
  runner=(uv run --with swebench python -m swebench.harness.run_evaluation)
else
  runner=(python3 -m swebench.harness.run_evaluation)
fi

command=(
  "${runner[@]}"
  --dataset_name "$SWE_BENCH_DATASET"
  --split "$SWE_BENCH_SPLIT"
  --predictions_path "$SWE_BENCH_PREDICTIONS"
  --max_workers "$SWE_BENCH_MAX_WORKERS"
  --run_id "$SWE_BENCH_RUN_ID"
  --cache_level "$SWE_BENCH_CACHE_LEVEL"
  --clean "$SWE_BENCH_CLEAN"
)

if [[ -n "$SWE_BENCH_INSTANCE_IDS" ]]; then
  IFS=', ' read -r -a instance_ids <<< "$SWE_BENCH_INSTANCE_IDS"
  command+=(--instance_ids)
  for instance_id in "${instance_ids[@]}"; do
    if [[ -n "$instance_id" ]]; then
      command+=("$instance_id")
    fi
  done
fi

command_json="$(printf '%s\n' "${command[@]}" | jq -R . | jq -s .)"

if [[ "$docker_accessible" != "true" ]]; then
  json_report \
    "preflight_failed" \
    "Docker is installed but this process cannot access the Docker daemon socket. Add the user to the docker group and start a new login session, or run the benchmark wrapper from a context that can access Docker." \
    "$docker_accessible" "$docker_exit" "$free_gb" "$disk_ok" "$swebench_available" "$uv_available" "$command_json"
  cat "$REPORT_JSON"
  exit 2
fi

if [[ "$disk_ok" != "true" ]]; then
  json_report \
    "preflight_failed" \
    "Available disk space is below the configured SWE-bench threshold. Free Docker/storage space, point SWE_BENCH_WORKDIR at a larger filesystem, or lower SWE_BENCH_MIN_FREE_GB for a constrained smoke run." \
    "$docker_accessible" "$docker_exit" "$free_gb" "$disk_ok" "$swebench_available" "$uv_available" "$command_json"
  cat "$REPORT_JSON"
  exit 3
fi

if [[ "$swebench_available" != "true" && "$uv_available" != "true" ]]; then
  json_report \
    "preflight_failed" \
    "Neither the swebench Python package nor uv is available. Install SWE-bench or uv before running the official harness." \
    "$docker_accessible" "$docker_exit" "$free_gb" "$disk_ok" "$swebench_available" "$uv_available" "$command_json"
  cat "$REPORT_JSON"
  exit 4
fi

if [[ "$SWE_BENCH_PREFLIGHT_ONLY" == "1" ]]; then
  json_report \
    "ready" \
    "SWE-bench official preflight passed; set SWE_BENCH_PREFLIGHT_ONLY=0 to run the upstream harness." \
    "$docker_accessible" "$docker_exit" "$free_gb" "$disk_ok" "$swebench_available" "$uv_available" "$command_json"
  cat "$REPORT_JSON"
  exit 0
fi

set +e
(
  cd "$SWE_BENCH_WORKDIR"
  "${command[@]}"
) > "$SWE_STDOUT" 2> "$SWE_STDERR"
run_exit=$?
set -e

if [[ "$run_exit" -eq 0 ]]; then
  json_report \
    "completed" \
    "SWE-bench official harness completed. Inspect evaluation_results and logs under the configured workdir." \
    "$docker_accessible" "$docker_exit" "$free_gb" "$disk_ok" "$swebench_available" "$uv_available" "$command_json"
else
  json_report \
    "failed" \
    "SWE-bench official harness exited with a non-zero status. Inspect the captured stdout/stderr and upstream logs." \
    "$docker_accessible" "$docker_exit" "$free_gb" "$disk_ok" "$swebench_available" "$uv_available" "$command_json"
fi

cat "$REPORT_JSON"
exit "$run_exit"
