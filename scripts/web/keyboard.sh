#!/bin/sh
# Keyboard handling in the browser (ex-515): excali_editor::keyboard
# (App.onKeyDown, App.onKeyUp) driven by Chromium's own key, clipboard and
# pointer events, one Playwright test per row of the shortcuts page
# (site/content/design-system/shortcuts.md).
#
#   scripts/web/keyboard.sh
#
# 1. Builds the wasm harness tools/keyboard for wasm32-unknown-unknown and
#    runs wasm-bindgen --target web into KEYBOARD_HARNESS.
# 2. Runs the Playwright suite tests/web/keyboard
#    (playwright.keyboard.config.mjs).
#
# CI runs this on every PR (rust.yml, job keyboard).
#
# Needs: the wasm32-unknown-unknown target, wasm-bindgen-cli at the version
# Cargo.lock pins, and `npm ci` plus `npx playwright install chromium` in
# tests/web.
#
# Environment: KEYBOARD_HARNESS (default target/keyboard-harness),
# CARGO_TARGET_DIR.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
harness="${KEYBOARD_HARNESS:-$root/target/keyboard-harness}"
manifest="$root/tools/keyboard/Cargo.toml"
target_dir="${CARGO_TARGET_DIR:-$root/tools/keyboard/target}"

die() {
  echo "keyboard: $*" >&2
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
wasm-bindgen --target web --no-typescript --out-dir "$harness" --out-name keyboard \
  "$target_dir/wasm32-unknown-unknown/release/keyboard.wasm"

(
  cd "$root/tests/web"
  KEYBOARD_HARNESS="$harness" ./node_modules/.bin/playwright test --config playwright.keyboard.config.mjs
)
echo "keyboard: every row of the shortcuts page holds in Chromium"
