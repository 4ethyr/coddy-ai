#!/usr/bin/env sh
set -eu

ROOT_DIR="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
tmp_dir="$(mktemp -d)"
findings="$tmp_dir/findings"

cleanup() {
  rm -r "$tmp_dir"
}
trap cleanup EXIT INT TERM

touch "$findings"
cd "$ROOT_DIR"

scan_pattern() {
  scope="$1"
  name="$2"
  regex="$3"

  if [ "$scope" = "cached" ]; then
    matches="$(git grep -IlE "$regex" --cached -- . ':!target/**' ':!dist/**' ':!apps/coddy-electron/dist/**' ':!apps/coddy-electron/release/**' 2>/dev/null || true)"
  else
    matches="$(git grep -IlE "$regex" -- . ':!target/**' ':!dist/**' ':!apps/coddy-electron/dist/**' ':!apps/coddy-electron/release/**' 2>/dev/null || true)"
  fi

  if [ -n "$matches" ]; then
    printf '%s\n' "$matches" | while IFS= read -r file; do
      printf '%s\t%s\t%s\n' "$scope" "$name" "$file" >> "$findings"
    done
  fi
}

scan_scope() {
  scope="$1"

  scan_pattern "$scope" "private_key" "BEGIN (RSA |DSA |EC |OPENSSH )?PRIVATE KEY"
  scan_pattern "$scope" "openai_key" "sk-[A-Za-z0-9]{20,}"
  scan_pattern "$scope" "anthropic_key" "sk-ant-[A-Za-z0-9_-]{20,}"
  scan_pattern "$scope" "google_api_key" "AIza[0-9A-Za-z_-]{35}"
  scan_pattern "$scope" "google_oauth_token" "ya29\\.[0-9A-Za-z_-]{20,}"
  scan_pattern "$scope" "github_token" "gh[pousr]_[0-9A-Za-z]{30,}"
  scan_pattern "$scope" "slack_token" "xox[baprs]-[0-9A-Za-z-]{20,}"
  scan_pattern "$scope" "aws_access_key" "(AKIA|ASIA)[0-9A-Z]{16}"
}

scan_scope "worktree"
scan_scope "cached"

if [ -s "$findings" ]; then
  echo "Potential secret-like values were found. Values are intentionally not printed." >&2
  sort -u "$findings" | while IFS="$(printf '\t')" read -r scope name file; do
    printf '  - %s %s %s\n' "$scope" "$name" "$file" >&2
  done
  echo "Remove the value, rotate it if it was real, or store it in local environment/credential storage." >&2
  exit 1
fi

echo "No high-confidence secrets found in tracked or staged files."
