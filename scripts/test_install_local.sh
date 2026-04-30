#!/usr/bin/env sh
set -eu

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
tmp_dir="$(mktemp -d)"

cleanup() {
  rm -r "$tmp_dir"
}
trap cleanup EXIT INT TERM

archive="$tmp_dir/coddy-linux-x64.tar.gz"
prefix="$tmp_dir/install"
desktop_dir="$prefix/share/applications"
desktop_shortcut_dir="$prefix/Desktop"
payload="$tmp_dir/payload/coddy-linux-x64"

mkdir -p "$payload/bin" "$payload/share/coddy"

cat > "$payload/bin/coddy" <<'CODDY'
#!/usr/bin/env sh
echo "coddy smoke"
CODDY

cat > "$payload/share/coddy/Coddy.AppImage" <<'APPIMAGE'
#!/usr/bin/env sh
echo "coddy desktop smoke"
APPIMAGE

cat > "$payload/share/coddy/logo.png" <<'LOGO'
fake png bytes for installer smoke
LOGO

chmod 755 "$payload/bin/coddy" "$payload/share/coddy/Coddy.AppImage"
tar -C "$tmp_dir/payload" -czf "$archive" "coddy-linux-x64"
sha256sum "$archive" > "$archive.sha256"

CODDY_ARCHIVE="$archive" \
CODDY_INSTALL_PREFIX="$prefix" \
CODDY_DESKTOP_DIR="$desktop_dir" \
CODDY_DESKTOP_SHORTCUT_DIR="$desktop_shortcut_dir" \
sh "$ROOT_DIR/scripts/install.sh" >/dev/null

if [ "$("$prefix/bin/coddy")" != "coddy smoke" ]; then
  echo "Installed Coddy CLI smoke failed" >&2
  exit 1
fi

if [ ! -x "$prefix/bin/coddy-desktop" ]; then
  echo "Installed Coddy desktop launcher is not executable" >&2
  exit 1
fi

desktop_output="$(HOME="$prefix/home" "$prefix/bin/coddy-desktop")"
if ! printf '%s\n' "$desktop_output" | grep -Fq "Coddy Desktop started"; then
  echo "Installed Coddy desktop launcher did not report startup" >&2
  exit 1
fi

if ! grep -Fq "coddy desktop smoke" "$prefix/home/.local/state/coddy/coddy-desktop.log"; then
  echo "Installed Coddy desktop launcher did not write AppImage output to the Coddy log" >&2
  exit 1
fi

if ! grep -Fq "Exec=$prefix/bin/coddy-desktop" "$desktop_dir/ai.coddy.Coddy.desktop"; then
  echo "Desktop entry does not point at the installed launcher" >&2
  exit 1
fi

if ! grep -Fq "Exec=$prefix/bin/coddy-desktop" "$desktop_shortcut_dir/Coddy.desktop"; then
  echo "Desktop shortcut does not point at the installed launcher" >&2
  exit 1
fi

if ! grep -Fq "Icon=$prefix/share/coddy/logo.png" "$desktop_dir/ai.coddy.Coddy.desktop"; then
  echo "Desktop menu entry does not point at the installed icon" >&2
  exit 1
fi

if [ ! -f "$prefix/share/icons/hicolor/512x512/apps/ai.coddy.Coddy.png" ]; then
  echo "Installed hicolor app icon is missing" >&2
  exit 1
fi

if ! grep -Fq "$prefix/share/coddy/Coddy.AppImage" "$prefix/bin/coddy-desktop"; then
  echo "Desktop launcher does not point at the installed AppImage" >&2
  exit 1
fi

echo "Local Coddy installer smoke passed"
