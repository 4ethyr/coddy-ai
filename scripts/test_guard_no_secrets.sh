#!/usr/bin/env sh
set -eu

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
tmp_dir="$(mktemp -d)"
repo="$tmp_dir/repo"
output="$tmp_dir/output"

cleanup() {
  rm -r "$tmp_dir"
}
trap cleanup EXIT INT TERM

mkdir -p "$repo"
git -C "$repo" init -q
git -C "$repo" config user.email "coddy-test@example.invalid"
git -C "$repo" config user.name "Coddy Test"

printf '%s\n' "API keys belong in local credential storage, not source files." > "$repo/README.md"
git -C "$repo" add README.md

(cd "$repo" && "$ROOT_DIR/scripts/guard_no_secrets.sh") >/dev/null

synthetic_key="$(printf '%s%s\n' 'AIza' 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA')"
printf 'GOOGLE_API_KEY=%s\n' "$synthetic_key" > "$repo/leak.env"
git -C "$repo" add leak.env

if (cd "$repo" && "$ROOT_DIR/scripts/guard_no_secrets.sh") >"$output" 2>&1; then
  echo "Secret guard did not reject a synthetic Google API key" >&2
  exit 1
fi

if ! grep -Fq "google_api_key" "$output"; then
  echo "Secret guard did not report the matching pattern name" >&2
  exit 1
fi

if ! grep -Fq "leak.env" "$output"; then
  echo "Secret guard did not report the matching file" >&2
  exit 1
fi

if grep -Fq "$synthetic_key" "$output"; then
  echo "Secret guard printed a secret-like value" >&2
  exit 1
fi

echo "Coddy secret guard smoke passed"
