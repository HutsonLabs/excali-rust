#!/bin/sh
# Build the web runtime (excali-wasm) as the release ships it (ex-307):
#
#   <out>/excali_editor.js          wasm-bindgen --target web output
#   <out>/excali_editor_bg.wasm
#   <out>/fonts/                    crates/excali-text/assets/fonts: the
#                                   range-split font files, their licences
#                                   and manifest.json
#
#   scripts/web/build.sh [OUT]      default OUT: target/web
#
# wasm-bindgen-cli must be the version Cargo.lock pins for the wasm-bindgen
# crate (the generated glue and the module must agree):
#   cargo install --locked wasm-bindgen-cli --version <that version>
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
out="${1:-$root/target/web}"

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

cargo build --locked --release --target wasm32-unknown-unknown -p excali-wasm
rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" --out-name excali_editor \
  "$root/target/wasm32-unknown-unknown/release/excali_wasm.wasm"
cp -R "$root/crates/excali-text/assets/fonts" "$out/fonts"
echo "web build: $out (wasm-bindgen $have)"
