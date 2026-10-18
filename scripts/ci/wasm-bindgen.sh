#!/bin/sh
# Spaces CI (ex-e0.1): wasm-bindgen-cli at the version Cargo.lock pins. The
# host's is used when it is that version; otherwise cargo installs it into
# $CARGO_INSTALL_ROOT (scripts/ci/spaces-env.sh: the CI cache, first on
# PATH), never into the host's ~/.cargo/bin.
set -eu
want="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
have="$(wasm-bindgen --version 2>/dev/null | awk '{ print $2 }' || true)"
if [ "$have" != "$want" ]; then
  [ -n "${CARGO_INSTALL_ROOT:-}" ] || { echo "CARGO_INSTALL_ROOT is not set (run scripts/ci/spaces-env.sh first)" >&2; exit 1; }
  cargo install --locked wasm-bindgen-cli --version "$want"
fi
echo "wasm-bindgen $(wasm-bindgen --version | awk '{ print $2 }') at $(command -v wasm-bindgen)"
