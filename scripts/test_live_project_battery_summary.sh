#!/usr/bin/env sh
set -eu

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
tmp_dir="$(mktemp -d)"
output_root="$tmp_dir/live-battery"

cleanup() {
  rm -r "$tmp_dir"
}
trap cleanup EXIT INT TERM

mkdir -p "$output_root"

cat > "$output_root/summary.jsonl" <<'JSONL'
{"project":"coddy","projectPath":"/home/aethyr/Documents/coddy","category":"codegen_tdd","askExit":0,"durationMs":1200,"intent":"AgenticCodeChange","finalPhase":"Completed","toolCount":6,"toolFailures":0,"permissionCount":0,"answerChars":4000,"providerErrors":0,"pseudoToolMarkup":0,"incompleteAnswers":0,"secretHits":0,"groundingChecks":0,"unverifiedClaims":0,"pathMentions":3,"qualityScore":98,"failureMessagePresent":0,"outputPath":"/tmp/coddy/codegen_tdd"}
{"status":"missing","project":"pytorch","projectPath":"/home/aethyr/Documents/pytorch"}
JSONL

OUTPUT_ROOT="$output_root" CODDY_LIVE_PROJECT_BATTERY_SUMMARY_ONLY=1 \
  "$ROOT_DIR/scripts/run_live_project_battery.sh" > "$output_root/stdout.json"

if [ "$(jq -r '.records' "$output_root/summary.json")" != "2" ]; then
  echo "Expected summary to include both completed and missing records" >&2
  exit 1
fi

if [ "$(jq -r '.prompts' "$output_root/summary.json")" != "1" ]; then
  echo "Missing projects must not be counted as prompt executions" >&2
  exit 1
fi

if [ "$(jq -r '.missingProjects | length' "$output_root/summary.json")" != "1" ]; then
  echo "Expected one missing project record" >&2
  exit 1
fi

if [ "$(jq -r '.missingProjects[0].project' "$output_root/summary.json")" != "pytorch" ]; then
  echo "Expected missing project to be named pytorch" >&2
  exit 1
fi

if [ "$(jq -r '.byProject | length' "$output_root/summary.json")" != "1" ]; then
  echo "Missing projects must not create byProject metric groups" >&2
  exit 1
fi

if [ "$(jq -r '.byProject[0].project' "$output_root/summary.json")" != "coddy" ]; then
  echo "Expected only completed prompt records in byProject" >&2
  exit 1
fi

if [ "$(jq -r '.qualityPassed' "$output_root/summary.json")" != "1" ]; then
  echo "Expected completed passing prompt to count as qualityPassed" >&2
  exit 1
fi

echo "Coddy live project battery summary smoke passed"
