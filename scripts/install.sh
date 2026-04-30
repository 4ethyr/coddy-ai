#!/usr/bin/env sh
set -eu

REPO="${CODDY_REPO:-4ethyr/coddy-ai}"
VERSION="${CODDY_VERSION:-latest}"
INSTALL_PREFIX="${CODDY_INSTALL_PREFIX:-$HOME/.local}"
BIN_DIR="${CODDY_BIN_DIR:-$INSTALL_PREFIX/bin}"
APP_DIR="${CODDY_APP_DIR:-$INSTALL_PREFIX/share/coddy}"
DESKTOP_DIR="${CODDY_DESKTOP_DIR:-$HOME/.local/share/applications}"

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"

if [ "$os" != "linux" ]; then
  echo "Coddy installer currently supports Linux only." >&2
  exit 1
fi

case "$arch" in
  x86_64 | amd64) coddy_arch="x64" ;;
  aarch64 | arm64) coddy_arch="arm64" ;;
  *)
    echo "Unsupported architecture: $arch" >&2
    exit 1
    ;;
esac

asset="coddy-linux-$coddy_arch.tar.gz"
if [ "$VERSION" = "latest" ]; then
  base_url="https://github.com/$REPO/releases/latest/download"
else
  base_url="https://github.com/$REPO/releases/download/$VERSION"
fi

tmp_dir="$(mktemp -d)"
archive="$tmp_dir/$asset"
checksum="$archive.sha256"

cleanup() {
  rm -r "$tmp_dir"
}
trap cleanup EXIT INT TERM

download() {
  url="$1"
  output="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$output"
    return
  fi
  if command -v wget >/dev/null 2>&1; then
    wget -qO "$output" "$url"
    return
  fi
  echo "Install curl or wget to download Coddy." >&2
  exit 1
}

verify_checksum() {
  checksum_file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    expected="$(awk '{print $1}' "$checksum_file")"
    actual="$(sha256sum "$archive" | awk '{print $1}')"
    if [ "$expected" != "$actual" ]; then
      echo "Checksum mismatch for $asset" >&2
      exit 1
    fi
  fi
}

if [ -n "${CODDY_ARCHIVE:-}" ]; then
  archive="$CODDY_ARCHIVE"
  checksum="${CODDY_ARCHIVE_SHA256:-$CODDY_ARCHIVE.sha256}"
  if [ ! -f "$archive" ]; then
    echo "Local Coddy archive not found: $archive" >&2
    exit 1
  fi
  echo "Installing Coddy from local archive $archive"
  if [ -f "$checksum" ]; then
    verify_checksum "$checksum"
  fi
else
  echo "Downloading Coddy from $base_url/$asset"
  download "$base_url/$asset" "$archive"

  if download "$base_url/$asset.sha256" "$checksum" 2>/dev/null; then
    verify_checksum "$checksum"
  fi
fi

tar -xzf "$archive" -C "$tmp_dir"
payload="$tmp_dir/coddy-linux-$coddy_arch"

mkdir -p "$BIN_DIR" "$APP_DIR" "$DESKTOP_DIR"
cp "$payload/bin/coddy" "$BIN_DIR/coddy"
cp "$payload/share/coddy/Coddy.AppImage" "$APP_DIR/Coddy.AppImage"

cat > "$BIN_DIR/coddy-desktop" <<WRAPPER
#!/usr/bin/env sh
set -eu

APPIMAGE="\${CODDY_APPIMAGE:-$APP_DIR/Coddy.AppImage}"
exec "\$APPIMAGE" "\$@"
WRAPPER

cat > "$DESKTOP_DIR/ai.coddy.Coddy.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Coddy
Comment=Coddy agentic coding REPL
Exec=$BIN_DIR/coddy-desktop
Terminal=false
Categories=Development;IDE;
StartupWMClass=Coddy
DESKTOP

chmod 755 "$BIN_DIR/coddy" "$BIN_DIR/coddy-desktop" "$APP_DIR/Coddy.AppImage"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
fi

echo "Coddy installed:"
echo "  CLI:     $BIN_DIR/coddy"
echo "  Desktop: $BIN_DIR/coddy-desktop"
echo
echo "Add $BIN_DIR to PATH if needed."
