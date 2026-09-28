#!/usr/bin/env bash
# Pinned Zola for site/. `ensure` downloads the release tarball into .tools/
# and verifies its SHA-256 (a download with no pinned digest is refused);
# `build`/`serve`/`check` run it. Portable to macOS /bin/bash 3.2 (no
# associative arrays) and to hosts with either sha256sum or shasum.
# The version matches .github/workflows/pages.yml (zola_version) exactly.
# Tests: scripts/site/zola-test.sh
#
# Overrides: ZOLA_VERSION, ZOLA_SHA256 (digest for an unpinned target or
# version), ZOLA_TARGET, ZOLA_TOOLS_DIR, ZOLA_BASE_URL (release mirror).
set -euo pipefail
root="$(git rev-parse --show-toplevel)"
ZOLA_VERSION="${ZOLA_VERSION:-0.22.0}"
ZOLA_BASE_URL="${ZOLA_BASE_URL:-https://github.com/getzola/zola/releases/download}"

# Pinned digests for v0.22.0. Each was read from the GitHub release asset
# metadata (gh api repos/getzola/zola/releases/tags/v0.22.0 -> assets[].digest)
# on 2026-09-27 and matched against `shasum -a 256` of the downloaded tarball;
# x86_64 linux gnu was first recorded on 2026-09-28 (adr-006).
pinned_sha256() {
  [ "$ZOLA_VERSION" = 0.22.0 ] || return 1
  case "$1" in
    x86_64-unknown-linux-gnu) echo f1d491f8956b94384c27d75cb6b2bf60d3916d1ade9564bcbfe7c03f0258aebf ;;
    aarch64-unknown-linux-gnu) echo 14ef16bfb36ff3911a0fcddbbd20e74d1383ac87cce4c2b097c9055fc1c98e87 ;;
    aarch64-apple-darwin) echo 96015a922a7d83827e381e273aef6be916711d43e89e65e8e82b4da0350fc425 ;;
    x86_64-apple-darwin) echo f0268e7559d8b6b79d50cef1cd6025a41819cbab920c3c5b0854e0de3a6584b9 ;;
    *) return 1 ;;
  esac
}

# Downloads are shared by every worktree of a clone: .tools/ sits next to the
# common .git directory (the main clone), not inside each worktree.
tools_dir() {
  if [ -n "${ZOLA_TOOLS_DIR:-}" ]; then echo "$ZOLA_TOOLS_DIR"; return; fi
  local common
  common="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || common="$root/.git"
  echo "$(dirname "$common")/.tools"
}
tools="$(tools_dir)"
bin="$tools/zola-$ZOLA_VERSION"

target() {
  if [ -n "${ZOLA_TARGET:-}" ]; then echo "$ZOLA_TARGET"; return; fi
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) echo x86_64-unknown-linux-gnu ;;
    Linux-aarch64 | Linux-arm64) echo aarch64-unknown-linux-gnu ;;
    Darwin-arm64) echo aarch64-apple-darwin ;;
    Darwin-x86_64) echo x86_64-apple-darwin ;;
    *) echo "unsupported platform $(uname -s)-$(uname -m)" >&2; return 1 ;;
  esac
}

# Prints the SHA-256 of $1 with whichever tool the host has.
sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    echo "no sha256 tool: install coreutils (sha256sum) or perl (shasum)" >&2
    return 1
  fi
}

want_sha256() {
  if [ -n "${ZOLA_SHA256:-}" ]; then echo "$ZOLA_SHA256"; return; fi
  if ! pinned_sha256 "$1"; then
    echo "no pinned sha256 for zola $ZOLA_VERSION $1; verify the tarball and set ZOLA_SHA256" >&2
    return 1
  fi
}

on_path() {
  command -v zola >/dev/null 2>&1 && zola --version 2>/dev/null | grep -q " $ZOLA_VERSION\$"
}

ensure() {
  if on_path; then echo "   zola $ZOLA_VERSION on PATH"; return; fi
  if [ -x "$bin" ]; then echo "   $bin"; return; fi
  local t want got
  t="$(target)"
  want="$(want_sha256 "$t")"
  local url="$ZOLA_BASE_URL/v$ZOLA_VERSION/zola-v$ZOLA_VERSION-$t.tar.gz"
  mkdir -p "$tools"
  local tmp
  tmp="$(mktemp -d "$tools/.zola-download.XXXXXX")"
  local tgz="$tmp/zola.tar.gz"
  echo "   downloading $url"
  if ! curl -fsSL -o "$tgz" "$url" || ! got="$(sha256_file "$tgz")"; then
    rm -rf "$tmp"
    return 1
  fi
  if [ "$got" != "$want" ]; then
    echo "sha256 mismatch for $url: expected $want, got $got" >&2
    rm -rf "$tmp"
    return 1
  fi
  echo "   sha256 verified ($t $got)"
  tar -xzf "$tgz" -C "$tmp" zola
  chmod +x "$tmp/zola"
  mv "$tmp/zola" "$bin"
  rm -rf "$tmp"
  echo "   $("$bin" --version)"
}

run() {
  if on_path; then
    (cd "$root/site" && zola "$@")
  else
    ensure >/dev/null
    (cd "$root/site" && "$bin" "$@")
  fi
}

case "${1:-}" in
  ensure) ensure ;;
  build | serve | check) run "$@" ;;
  target) target ;;
  digest) pinned_sha256 "${2:?usage: $0 digest <target>}" || { echo "no pinned sha256 for zola $ZOLA_VERSION ${2}" >&2; exit 1; } ;;
  tools-dir) echo "$tools" ;;
  *) echo "usage: $0 ensure|build|serve|check [zola args] | target | digest <target> | tools-dir" >&2; exit 2 ;;
esac
