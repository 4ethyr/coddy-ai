#!/usr/bin/env sh
set -eu

REPO="${CODDY_REPO:-4ethyr/coddy-ai}"
VERSION="${CODDY_VERSION:-latest}"
INSTALL_PREFIX="${CODDY_INSTALL_PREFIX:-$HOME/.local}"
BIN_DIR="${CODDY_BIN_DIR:-$INSTALL_PREFIX/bin}"
APP_DIR="${CODDY_APP_DIR:-$INSTALL_PREFIX/share/coddy}"
DESKTOP_DIR="${CODDY_DESKTOP_DIR:-$HOME/.local/share/applications}"
DESKTOP_SHORTCUT_DIR="${CODDY_DESKTOP_SHORTCUT_DIR:-$HOME/Desktop}"
ICON_DIR="${CODDY_ICON_DIR:-$INSTALL_PREFIX/share/icons/hicolor/512x512/apps}"
APP_ID="ai.coddy.Coddy"

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

install_system_dependencies_if_requested() {
  if [ "${CODDY_INSTALL_SYSTEM_DEPS:-0}" != "1" ]; then
    return
  fi

  if [ "$(id -u)" != "0" ] && ! command -v sudo >/dev/null 2>&1; then
    echo "CODDY_INSTALL_SYSTEM_DEPS=1 requires root or sudo." >&2
    exit 1
  fi

  if command -v apt-get >/dev/null 2>&1; then
    sudo_cmd=""
    if [ "$(id -u)" != "0" ]; then
      sudo_cmd="sudo"
    fi
    $sudo_cmd apt-get update
    $sudo_cmd apt-get install -y \
      ca-certificates curl tar desktop-file-utils xdg-utils \
      pipewire-bin alsa-utils libfuse2
    return
  fi

  if command -v dnf >/dev/null 2>&1; then
    sudo_cmd=""
    if [ "$(id -u)" != "0" ]; then
      sudo_cmd="sudo"
    fi
    $sudo_cmd dnf install -y \
      ca-certificates curl tar desktop-file-utils xdg-utils \
      pipewire-utils alsa-utils fuse-libs
    return
  fi

  echo "Automatic dependency install is not supported for this distro." >&2
  echo "Install: curl or wget, tar, desktop-file-utils, xdg-utils, pw-record or arecord, and FUSE/AppImage support." >&2
}

warn_missing_runtime_dependencies() {
  missing=""
  if ! command -v tar >/dev/null 2>&1; then
    missing="$missing tar"
  fi
  if ! command -v update-desktop-database >/dev/null 2>&1; then
    missing="$missing desktop-file-utils"
  fi
  if ! command -v pw-record >/dev/null 2>&1 && ! command -v arecord >/dev/null 2>&1; then
    missing="$missing pipewire-bin-or-alsa-utils"
  fi

  if [ -n "$missing" ]; then
    echo "Coddy installed, but these optional runtime dependencies may be missing:$missing" >&2
    echo "Rerun with CODDY_INSTALL_SYSTEM_DEPS=1 to let the installer try apt/dnf package installation." >&2
  fi
}

write_desktop_entry() {
  entry_path="$1"
  exec_path="$2"
  icon_path="$3"

  {
    echo "[Desktop Entry]"
    echo "Type=Application"
    echo "Name=Coddy"
    echo "Comment=Coddy agentic coding REPL"
    echo "Exec=$exec_path"
    if [ -n "$icon_path" ]; then
      echo "Icon=$icon_path"
    fi
    echo "Terminal=false"
    echo "Categories=Development;IDE;"
    echo "StartupWMClass=Coddy"
  } > "$entry_path"
}

tmp_dir="$(mktemp -d)"
archive="$tmp_dir/$asset"
checksum="$archive.sha256"

cleanup() {
  rm -r "$tmp_dir"
}
trap cleanup EXIT INT TERM

install_system_dependencies_if_requested

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

require_payload_file() {
  candidate="$1"
  label="$2"
  if [ ! -f "$candidate" ]; then
    echo "Invalid Coddy archive: missing $label" >&2
    exit 1
  fi
}

require_payload_file "$payload/bin/coddy" "bin/coddy"
require_payload_file "$payload/share/coddy/Coddy.AppImage" "share/coddy/Coddy.AppImage"

mkdir -p "$BIN_DIR" "$APP_DIR" "$DESKTOP_DIR" "$DESKTOP_SHORTCUT_DIR"
cp "$payload/bin/coddy" "$BIN_DIR/coddy"
cp "$payload/share/coddy/Coddy.AppImage" "$APP_DIR/Coddy.AppImage"

desktop_icon_path=""
if [ -f "$payload/share/coddy/logo.png" ]; then
  mkdir -p "$ICON_DIR"
  cp "$payload/share/coddy/logo.png" "$APP_DIR/logo.png"
  cp "$payload/share/coddy/logo.png" "$ICON_DIR/$APP_ID.png"
  desktop_icon_path="$APP_DIR/logo.png"
fi

cat > "$BIN_DIR/coddy-desktop" <<WRAPPER
#!/usr/bin/env sh
set -eu

APPIMAGE="\${CODDY_APPIMAGE:-$APP_DIR/Coddy.AppImage}"
LOG_DIR="\${XDG_STATE_HOME:-\$HOME/.local/state}/coddy"
LOG_FILE="\$LOG_DIR/coddy-desktop.log"

has_fuse2() {
  if command -v ldconfig >/dev/null 2>&1 && ldconfig -p 2>/dev/null | grep -q 'libfuse.so.2'; then
    return 0
  fi
  for candidate in /lib*/libfuse.so.2 /usr/lib*/libfuse.so.2 /lib/*/libfuse.so.2 /usr/lib/*/libfuse.so.2; do
    if [ -e "\$candidate" ]; then
      return 0
    fi
  done
  return 1
}

mkdir -p "\$LOG_DIR"

if ! has_fuse2; then
  export APPIMAGE_EXTRACT_AND_RUN="\${APPIMAGE_EXTRACT_AND_RUN:-1}"
fi

export ELECTRON_DISABLE_SANDBOX="\${ELECTRON_DISABLE_SANDBOX:-1}"

if [ "\${CODDY_DESKTOP_FOREGROUND:-0}" = "1" ]; then
  exec "\$APPIMAGE" "\$@" >>"\$LOG_FILE" 2>&1
fi

if command -v setsid >/dev/null 2>&1 && setsid --help 2>/dev/null | grep -q -- ' --fork'; then
  setsid -f "\$APPIMAGE" "\$@" >>"\$LOG_FILE" 2>&1 < /dev/null
elif command -v setsid >/dev/null 2>&1; then
  setsid "\$APPIMAGE" "\$@" >>"\$LOG_FILE" 2>&1 < /dev/null &
else
  nohup "\$APPIMAGE" "\$@" >>"\$LOG_FILE" 2>&1 < /dev/null &
fi
echo "Coddy Desktop started. Logs: \$LOG_FILE"
WRAPPER

write_desktop_entry "$DESKTOP_DIR/$APP_ID.desktop" "$BIN_DIR/coddy-desktop" "$desktop_icon_path"
write_desktop_entry "$DESKTOP_SHORTCUT_DIR/Coddy.desktop" "$BIN_DIR/coddy-desktop" "$desktop_icon_path"

chmod 755 "$BIN_DIR/coddy" "$BIN_DIR/coddy-desktop" "$APP_DIR/Coddy.AppImage"
chmod 644 "$DESKTOP_DIR/$APP_ID.desktop"
chmod 755 "$DESKTOP_SHORTCUT_DIR/Coddy.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -f "$ICON_DIR/$APP_ID.png" ]; then
  gtk-update-icon-cache "$(dirname "$(dirname "$(dirname "$ICON_DIR")")")" >/dev/null 2>&1 || true
fi

warn_missing_runtime_dependencies

echo "Coddy installed:"
echo "  CLI:     $BIN_DIR/coddy"
echo "  Desktop: $BIN_DIR/coddy-desktop"
echo "  Menu:    $DESKTOP_DIR/$APP_ID.desktop"
echo "  Shortcut:$DESKTOP_SHORTCUT_DIR/Coddy.desktop"
echo
echo "Add $BIN_DIR to PATH if needed."
