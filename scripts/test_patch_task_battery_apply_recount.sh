#!/usr/bin/env sh
set -eu

tmp_dir="$(mktemp -d)"
patch_file="$tmp_dir/model.patch"

cleanup() {
  rm -r "$tmp_dir"
}
trap cleanup EXIT INT TERM

cat > "$tmp_dir/string_tools.py" <<'PY'
def slugify(value: str) -> str:
    return value.strip().lower().replace(" ", "-")
PY

git -C "$tmp_dir" init -q
git -C "$tmp_dir" config user.email "coddy-test@example.invalid"
git -C "$tmp_dir" config user.name "Coddy Test"
git -C "$tmp_dir" add string_tools.py
git -C "$tmp_dir" commit -q -m fixture

cat > "$patch_file" <<'PATCH'
diff --git a/string_tools.py b/string_tools.py
--- a/string_tools.py
+++ b/string_tools.py
@@ -1,2 +1,14 @@
+import re
+import unicodedata
+
+
 def slugify(value: str) -> str:
-    return value.strip().lower().replace(" ", "-")
+    value = value.lower()
+    value = unicodedata.normalize("NFKD", value)
+    value = value.encode("ascii", "ignore").decode("ascii")
+    value = re.sub(r"[^a-z0-9]+", "-", value)
+    value = value.strip("-")
+    return value
PATCH

git -C "$tmp_dir" apply --recount "$patch_file"

if ! grep -Fq "return value" "$tmp_dir/string_tools.py"; then
  echo "git apply --recount did not preserve the final return line" >&2
  exit 1
fi

(cd "$tmp_dir" && python3 - <<'PY'
from string_tools import slugify

assert slugify(" Hello,   Café Déjà Vu!! ") == "hello-cafe-deja-vu"
assert slugify(" ... !!! ") == ""
PY
)

echo "Coddy patch task apply recount smoke passed"
