#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT_DIR/apps/coddy-electron"
OUT_DIR="$ROOT_DIR/dist"
ARCH="$(uname -m)"

case "$ARCH" in
  x86_64 | amd64) CODDY_ARCH="x64" ;;
  aarch64 | arm64) CODDY_ARCH="arm64" ;;
  *)
    echo "Unsupported Linux architecture: $ARCH" >&2
    exit 1
    ;;
esac

VERSION="$(node -p "require('$APP_DIR/package.json').version")"
APPIMAGE_NAME="Coddy.AppImage"
ASSET="$OUT_DIR/coddy-linux-$CODDY_ARCH.tar.gz"
STAGE_PARENT="$(mktemp -d)"
STAGE_DIR="$STAGE_PARENT/coddy-linux-$CODDY_ARCH"

cleanup() {
  rm -r "$STAGE_PARENT"
}
trap cleanup EXIT INT TERM

mkdir -p "$STAGE_DIR/bin" "$STAGE_DIR/share/coddy" "$STAGE_DIR/share/applications"
mkdir -p "$OUT_DIR"

if [[ "${CODDY_SKIP_SECRET_GUARD:-0}" != "1" ]]; then
  "$ROOT_DIR/scripts/guard_no_secrets.sh"
fi

echo "Building Coddy backend..."
cargo build --release -p coddy

echo "Building Coddy Electron frontend..."
npm --prefix "$APP_DIR" run build
npm --prefix "$APP_DIR" run electron:build -- --linux AppImage

APPIMAGE_PATH="$(
  find "$APP_DIR/release" -maxdepth 1 -type f -name 'Coddy-*.AppImage' -printf '%T@ %p\n' \
    | sort -nr \
    | awk 'NR == 1 { $1=""; sub(/^ /, ""); print }'
)"

if [[ -z "$APPIMAGE_PATH" ]]; then
  echo "Could not find generated Coddy AppImage under $APP_DIR/release" >&2
  exit 1
fi

cp "$ROOT_DIR/target/release/coddy" "$STAGE_DIR/bin/coddy"
cp "$APPIMAGE_PATH" "$STAGE_DIR/share/coddy/$APPIMAGE_NAME"
cp "$ROOT_DIR/scripts/install.sh" "$STAGE_DIR/install.sh"

cat > "$STAGE_DIR/bin/coddy-desktop" <<'WRAPPER'
#!/usr/bin/env sh
set -eu

APPIMAGE="${CODDY_APPIMAGE:-$HOME/.local/share/coddy/Coddy.AppImage}"
LOG_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/coddy"
LOG_FILE="$LOG_DIR/coddy-desktop.log"

has_fuse2() {
  if command -v ldconfig >/dev/null 2>&1 && ldconfig -p 2>/dev/null | grep -q 'libfuse.so.2'; then
    return 0
  fi
  for candidate in /lib*/libfuse.so.2 /usr/lib*/libfuse.so.2 /lib/*/libfuse.so.2 /usr/lib/*/libfuse.so.2; do
    if [ -e "$candidate" ]; then
      return 0
    fi
  done
  return 1
}

mkdir -p "$LOG_DIR"

if ! has_fuse2; then
  export APPIMAGE_EXTRACT_AND_RUN="${APPIMAGE_EXTRACT_AND_RUN:-1}"
fi

export ELECTRON_DISABLE_SANDBOX="${ELECTRON_DISABLE_SANDBOX:-1}"

if [ "${CODDY_DESKTOP_FOREGROUND:-0}" = "1" ]; then
  exec "$APPIMAGE" "$@" >>"$LOG_FILE" 2>&1
fi

if command -v setsid >/dev/null 2>&1 && setsid --help 2>/dev/null | grep -q -- ' --fork'; then
  setsid -f "$APPIMAGE" "$@" >>"$LOG_FILE" 2>&1 < /dev/null
elif command -v setsid >/dev/null 2>&1; then
  setsid "$APPIMAGE" "$@" >>"$LOG_FILE" 2>&1 < /dev/null &
else
  nohup "$APPIMAGE" "$@" >>"$LOG_FILE" 2>&1 < /dev/null &
fi
echo "Coddy Desktop started. Logs: $LOG_FILE"
WRAPPER

cat > "$STAGE_DIR/share/applications/ai.coddy.Coddy.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Coddy
Comment=Coddy agentic coding REPL
Exec=coddy-desktop
Terminal=false
Categories=Development;IDE;
StartupWMClass=Coddy
DESKTOP

chmod 755 "$STAGE_DIR/bin/coddy" "$STAGE_DIR/bin/coddy-desktop" "$STAGE_DIR/share/coddy/$APPIMAGE_NAME"

tar -C "$STAGE_PARENT" -czf "$ASSET" "coddy-linux-$CODDY_ARCH"
sha256sum "$ASSET" > "$ASSET.sha256"

echo "Built $ASSET"
echo "Built $ASSET.sha256"
echo "Version: $VERSION"
