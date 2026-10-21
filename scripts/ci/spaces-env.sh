#!/bin/sh
# The first step of every cargo job in .workflows/ (Spaces CI, ex-e0.1).
#
#   scripts/ci/spaces-env.sh <key> [<workspace dir>...]
#
# Spaces' runner (spacer) runs a job on this Mac's host, in a fresh
# directory that is removed afterwards, with no cache service. So that a
# job does not rebuild every dependency from nothing, each <workspace dir>'s
# target/ (default: the repository root) is a symlink into a cache that
# outlives the job, under <key> (one per workflow and toolchain, so jobs
# with other RUSTFLAGS or toolchains do not evict each other's builds).
# Tools installed by a job (wasm-bindgen-cli at the Cargo.lock version,
# when the host's differs) go into the cache's bin/, first on PATH, never
# into the host's ~/.cargo/bin; the pinned binaryen download goes into the
# cache as well. Cache root: $EXCALI_CI_CACHE, default
# ~/Library/Caches/excali-rust-ci.
set -eu
[ $# -ge 1 ] || { echo "usage: $0 <key> [<workspace dir>...]" >&2; exit 2; }
key=$1
shift
[ $# -ge 1 ] || set -- .
cache="${EXCALI_CI_CACHE:-$HOME/Library/Caches/excali-rust-ci}"
mkdir -p "$cache/bin" "$cache/binaryen"
for ws in "$@"; do
  dir="$cache/target/$key/$(printf '%s' "$ws" | tr '/' '_')"
  mkdir -p "$dir"
  if [ -L "$ws/target" ] || [ ! -e "$ws/target" ]; then
    ln -sfn "$dir" "$ws/target"
    echo "$ws/target -> $dir"
  fi
done
if [ -n "${GITHUB_ENV:-}" ]; then
  echo "CARGO_INSTALL_ROOT=$cache" >> "$GITHUB_ENV"
  echo "BINARYEN_TOOLS_DIR=$cache/binaryen" >> "$GITHUB_ENV"
  echo "$cache/bin" >> "$GITHUB_PATH"
fi
