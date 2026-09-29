#!/bin/sh
# The text editor in the browser (ex-512): excali_ui::text_editor's textarea
# mounted in Chromium on every session of upstream's text editing fixture
# (crates/excali-editor/tests/fixtures/text-editing.json), typed into with
# the keyboard, and the elements, app state and textarea held to upstream's
# records by Playwright.
#
#   scripts/web/text-editing.sh
#
# 1. Builds the wasm harness tools/text-editing (a fixture session in an
#    excali_editor session, the textarea mounted) for wasm32-unknown-unknown
#    and runs wasm-bindgen --target web into TEXT_EDITING_HARNESS.
# 2. Runs the Playwright suite tests/web/text-editing
#    (playwright.text-editing.config.mjs).
#
# CI runs this on every PR (rust.yml, job text-editing).
#
# Needs: the wasm32-unknown-unknown target, wasm-bindgen-cli at the version
# Cargo.lock pins, and `npm ci` plus `npx playwright install chromium` in
# tests/web.
#
# Environment: TEXT_EDITING_HARNESS (default target/text-editing-harness),
# CARGO_TARGET_DIR.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
harness="${TEXT_EDITING_HARNESS:-$root/target/text-editing-harness}"
manifest="$root/tools/text-editing/Cargo.toml"
target_dir="${CARGO_TARGET_DIR:-$root/tools/text-editing/target}"

die() {
  echo "text editing: $*" >&2
  exit 1
}

want="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
[ -n "$want" ] || die "wasm-bindgen is not in Cargo.lock"
command -v wasm-bindgen >/dev/null 2>&1 ||
  die "wasm-bindgen not found; cargo install --locked wasm-bindgen-cli --version $want"
have="$(wasm-bindgen --version | awk '{ print $2 }')"
[ "$have" = "$want" ] ||
  die "wasm-bindgen $have, Cargo.lock pins $want; cargo install --locked wasm-bindgen-cli --version $want"
[ -x "$root/tests/web/node_modules/.bin/playwright" ] ||
  die "Playwright missing; npm ci --prefix tests/web && (cd tests/web && npx playwright install chromium)"

cargo build --locked --release --target wasm32-unknown-unknown --manifest-path "$manifest"
rm -rf "$harness"
mkdir -p "$harness"
wasm-bindgen --target web --no-typescript --out-dir "$harness" --out-name text_editing \
  "$target_dir/wasm32-unknown-unknown/release/text_editing.wasm"

(
  cd "$root/tests/web"
  TEXT_EDITING_HARNESS="$harness" ./node_modules/.bin/playwright test --config playwright.text-editing.config.mjs
)
echo "text editing: typing into the textarea in Chromium gives upstream's elements"
