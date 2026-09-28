#!/usr/bin/env bash
# Payload codec goldens (ex-110): run upstream's own `encode`/`decode`
# (packages/excalidraw/data/encode.ts at the pinned commit, loaded verbatim)
# on top of the exact pako release upstream locks, and write what they
# produce to crates/excali-core/tests/fixtures/payload/goldens.json.
#
#   scripts/fixtures/payload-goldens.sh           regenerate the goldens file
#   scripts/fixtures/payload-goldens.sh --check   exit 1 if regenerating would change it
#
# pako is not vendored. Its version, tarball URL and SHA-512 integrity are
# read from the upstream checkout (packages/excalidraw/package.json and
# yarn.lock); the tarball is downloaded once into the shared .tools directory
# and refused unless its digest matches the lock file.
#
# Environment:
#   UPSTREAM_DIR  upstream checkout; default <main clone>/.tools/upstream
#                 (create it with scripts/upstream/checkout.sh)
#   TOOLS_DIR     where pako is unpacked; default <main clone>/.tools
#   NODE          node binary (>= 22.15: type stripping and module.registerHooks)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
common="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)"
shared="$(dirname "$common")/.tools"
upstream="${UPSTREAM_DIR:-$shared/upstream}"
tools="${TOOLS_DIR:-$shared}"
node="${NODE:-node}"
out="$root/crates/excali-core/tests/fixtures/payload/goldens.json"

die() { printf 'payload goldens: %s\n' "$*" >&2; exit 1; }

mode=write
case "${1:-}" in
  "") ;;
  --check) mode=check ;;
  -h | --help) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

[ -f "$upstream/packages/excalidraw/data/encode.ts" ] ||
  die "no upstream checkout at $upstream (run scripts/upstream/checkout.sh)"

version="$(sed -n 's/^    "pako": "\([^"]*\)",*$/\1/p' "$upstream/packages/excalidraw/package.json" | head -n 1)"
[ -n "$version" ] || die "pako is not a dependency in packages/excalidraw/package.json"
lock="$(grep -A3 "^pako@$version:" "$upstream/yarn.lock")" || die "pako@$version not in yarn.lock"
url="$(printf '%s\n' "$lock" | sed -n 's/^  resolved "\([^"#]*\).*"$/\1/p')"
integrity="$(printf '%s\n' "$lock" | sed -n 's/^  integrity sha512-\(.*\)$/\1/p')"
[ -n "$url" ] && [ -n "$integrity" ] || die "cannot read resolved/integrity for pako@$version"

pako="$tools/pako-$version"
if [ ! -f "$pako/package/index.js" ]; then
  mkdir -p "$pako"
  tgz="$pako/pako-$version.tgz"
  echo "downloading $url" >&2
  curl -sSfL "$url" -o "$tgz.part"
  got="$(openssl dgst -sha512 -binary "$tgz.part" | openssl base64 -A)"
  [ "$got" = "$integrity" ] || { rm -f "$tgz.part"; die "sha512 mismatch for $url: got $got, want $integrity"; }
  mv "$tgz.part" "$tgz"
  tar -xzf "$tgz" -C "$pako"
fi

args=(--upstream "$upstream" --pako "$pako/package" --root "$root" --pako-version "$version")
if [ "$mode" = write ]; then
  mkdir -p "$(dirname "$out")"
  "$node" "$here/payload_goldens.mjs" "${args[@]}" > "$out.tmp"
  mv "$out.tmp" "$out"
  echo "wrote ${out#"$root"/}" >&2
else
  tmp="$(mktemp)"
  trap 'rm -f "$tmp"' EXIT
  "$node" "$here/payload_goldens.mjs" "${args[@]}" > "$tmp"
  if ! cmp -s "$tmp" "$out"; then
    diff -u "$out" "$tmp" | head -n 40 >&2 || true
    die "${out#"$root"/} is stale: run scripts/fixtures/payload-goldens.sh"
  fi
  echo "payload goldens match upstream encode.ts + pako@$version" >&2
fi
