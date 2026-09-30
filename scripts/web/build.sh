#!/bin/sh
# Build the web runtime (excali-wasm) as the release ships it (ex-501):
# plain static files that load as an ES module with no bundler, as term.hut
# vendors third-party code (PRODUCT.md:106-108):
#
#   <out>/excali_editor.js          wasm-bindgen --target web output, then
#                                   the <excali-editor> custom element shim
#                                   (crates/excali-wasm/js/excali-editor.js,
#                                   ex-530), one plain ES module
#   <out>/excali_editor_bg.wasm     the module after wasm-opt -Os
#   <out>/excali.css                the element's stylesheet
#                                   (crates/excali-wasm/excali.css)
#   <out>/fonts/                    crates/excali-text/assets/fonts: the
#                                   range-split font files, their licences
#                                   and manifest.json (ex-307)
#
#   scripts/web/build.sh [OUT]      default OUT: dist (at the repository root)
#
# Steps: cargo build --profile web-release (Cargo.toml: opt-level "s", "z"
# for excali-ui, excali-editor and excali-wasm, fat LTO, one codegen unit,
# panic = "abort"); wasm-bindgen --target web; wasm-opt
# -Os from the pinned binaryen (scripts/web/binaryen.py downloads and
# verifies it into .tools/ when it is not on PATH); then the gzip sizes are
# checked against the budgets on site/content/plan/phases.md
# (scripts/gates/wasm_size.py), and the build fails if either is over.
#
# wasm-bindgen-cli must be the version Cargo.lock pins for the wasm-bindgen
# crate (the generated glue and the module must agree):
#   cargo install --locked wasm-bindgen-cli --version <that version>
#
# CARGO_TARGET_DIR is honoured. Tests: scripts/web/test_build.py
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
out="${1:-$root/dist}"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
profile=web-release

want="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
if [ -z "$want" ]; then
  echo "web build: wasm-bindgen is not in Cargo.lock" >&2
  exit 1
fi
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "web build: wasm-bindgen not found; cargo install --locked wasm-bindgen-cli --version $want" >&2
  exit 1
fi
have="$(wasm-bindgen --version | awk '{ print $2 }')"
if [ "$have" != "$want" ]; then
  echo "web build: wasm-bindgen $have, Cargo.lock pins $want; cargo install --locked wasm-bindgen-cli --version $want" >&2
  exit 1
fi
wasm_opt="$(python3 "$root/scripts/web/binaryen.py" ensure)"

cargo build --locked --profile "$profile" --target wasm32-unknown-unknown -p excali-wasm
rm -rf "$out"
mkdir -p "$out"
stage="$(mktemp -d "${TMPDIR:-/tmp}/excali-web.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
wasm-bindgen --target web --no-typescript --out-dir "$stage" --out-name excali_editor \
  "$target_dir/wasm32-unknown-unknown/$profile/excali_wasm.wasm"
# wasm-opt takes the enabled features from the module's target_features
# section (rustc writes it, wasm-bindgen and wasm-opt keep it), so no
# --enable-* flags are needed.
"$wasm_opt" -Os --strip-debug --strip-producers \
  -o "$out/excali_editor_bg.wasm" "$stage/excali_editor_bg.wasm"
cat "$stage/excali_editor.js" "$root/crates/excali-wasm/js/excali-editor.js" >"$out/excali_editor.js"
cp "$root/crates/excali-wasm/excali.css" "$out/excali.css"
cp -R "$root/crates/excali-text/assets/fonts" "$out/fonts"
echo "web build: $out (wasm-bindgen $have, $("$wasm_opt" --version))"
python3 "$root/scripts/gates/wasm_size.py" check "$out"
