#!/bin/sh
# The example's copy of the web runtime (ex-606): the release directory of
# scripts/web/build.sh (excali_editor.js, excali_editor_bg.wasm, excali.css,
# fonts/) in ui/excali/, where ui/app.js imports it, as term.hut vendors the
# module (site/content/architecture/tauri.md). tauri.conf.json runs this
# before `cargo tauri dev` and `cargo tauri build`.
#
#   scripts/web-runtime.sh          builds the runtime (scripts/web/build.sh)
#   EXCALI_WEB_DIST=dir scripts/web-runtime.sh
#                                   copies an existing build instead
set -eu
app="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$app/../.." && pwd)"
out="$app/ui/excali"
if [ -n "${EXCALI_WEB_DIST:-}" ]; then
  src="$(cd "$EXCALI_WEB_DIST" && pwd)"
  for f in excali_editor.js excali_editor_bg.wasm excali.css fonts/manifest.json; do
    if [ ! -f "$src/$f" ]; then
      echo "web runtime: $src/$f is missing; run scripts/web/build.sh" >&2
      exit 1
    fi
  done
  rm -rf "$out"
  mkdir -p "$out"
  cp -R "$src/excali_editor.js" "$src/excali_editor_bg.wasm" "$src/excali.css" "$src/fonts" "$out/"
else
  "$root/scripts/web/build.sh" "$out"
fi
echo "web runtime: $out"
