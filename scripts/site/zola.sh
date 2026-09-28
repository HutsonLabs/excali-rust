#!/usr/bin/env bash
# Pinned Zola for site/. `ensure` downloads the release tarball into
# .tools/ and verifies its SHA-256; `build`/`serve`/`check` run it.
# The version matches .github/workflows/pages.yml (zola_version) exactly.
set -euo pipefail
root="$(git rev-parse --show-toplevel)"
ZOLA_VERSION="${ZOLA_VERSION:-0.22.0}"
# Digest recorded from the tarball downloaded on 2026-09-28 from
# github.com/getzola/zola/releases/download/v0.22.0/ (x86_64 linux gnu).
# Other platforms: set ZOLA_SHA256 to the digest you verified.
# A function, not `declare -A`: macOS ships bash 3.2 (no associative arrays).
pinned_sha() {
  case "$1" in
    x86_64-unknown-linux-gnu) echo f1d491f8956b94384c27d75cb6b2bf60d3916d1ade9564bcbfe7c03f0258aebf ;;
    # aarch64 darwin tarball, same release, downloaded and hashed 2026-09-28.
    aarch64-apple-darwin) echo 96015a922a7d83827e381e273aef6be916711d43e89e65e8e82b4da0350fc425 ;;
    *) echo "" ;;
  esac
}
sha256_check() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum -c -; else shasum -a 256 -c -; fi
}
tools="$root/.tools"
bin="$tools/zola-$ZOLA_VERSION"

target() {
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) echo x86_64-unknown-linux-gnu ;;
    Darwin-arm64) echo aarch64-apple-darwin ;;
    Darwin-x86_64) echo x86_64-apple-darwin ;;
    *) echo "unsupported platform $(uname -s)-$(uname -m)" >&2; exit 1 ;;
  esac
}

ensure() {
  if command -v zola >/dev/null 2>&1 && zola --version | grep -q " $ZOLA_VERSION\$"; then
    echo "   zola $ZOLA_VERSION on PATH"; return
  fi
  if [ -x "$bin" ]; then echo "   $bin"; return; fi
  local t; t="$(target)"
  local url="https://github.com/getzola/zola/releases/download/v$ZOLA_VERSION/zola-v$ZOLA_VERSION-$t.tar.gz"
  mkdir -p "$tools"
  local tgz="$tools/zola-$ZOLA_VERSION-$t.tar.gz"
  echo "   downloading $url"
  curl -fsSL -o "$tgz" "$url"
  local want="${ZOLA_SHA256:-$(pinned_sha "$t")}"
  if [ -n "$want" ]; then
    echo "$want  $tgz" | sha256_check >/dev/null
    echo "   sha256 verified"
  else
    echo "   WARNING: no pinned sha256 for $t; record one after verifying" >&2
  fi
  tar -xzf "$tgz" -C "$tools" zola
  mv "$tools/zola" "$bin"
  rm -f "$tgz"
  echo "   $("$bin" --version)"
}

run() {
  if command -v zola >/dev/null 2>&1 && zola --version | grep -q " $ZOLA_VERSION\$"; then
    (cd "$root/site" && zola "$@")
  else
    ensure >/dev/null
    (cd "$root/site" && "$bin" "$@")
  fi
}

case "${1:-}" in
  ensure) ensure ;;
  build) shift; run build "$@" ;;
  serve) shift; run serve "$@" ;;
  check) shift; run check "$@" ;;
  *) echo "usage: $0 ensure|build|serve|check [zola args]" >&2; exit 2 ;;
esac
