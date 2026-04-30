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
STAGE_DIR="$OUT_DIR/coddy-linux-$CODDY_ARCH"
APPIMAGE_NAME="Coddy.AppImage"
ASSET="$OUT_DIR/coddy-linux-$CODDY_ARCH.tar.gz"

mkdir -p "$STAGE_DIR/bin" "$STAGE_DIR/share/coddy" "$STAGE_DIR/share/applications"

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
exec "$APPIMAGE" "$@"
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

tar -C "$OUT_DIR" -czf "$ASSET" "coddy-linux-$CODDY_ARCH"
sha256sum "$ASSET" > "$ASSET.sha256"

echo "Built $ASSET"
echo "Built $ASSET.sha256"
echo "Version: $VERSION"
