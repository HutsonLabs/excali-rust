#!/usr/bin/env bash
# PNG scene payload goldens (ex-111): run upstream's own `encodePngMetadata`,
# `decodePngMetadata` and `getTEXtChunk` (packages/excalidraw/data/image.ts at
# the pinned commit, loaded verbatim) on top of the exact png-chunk-text,
# png-chunks-encode, png-chunks-extract, crc-32, sliced and pako releases
# upstream locks, and write what they produce to
# crates/excali-core/tests/fixtures/png/goldens.json.
#
#   scripts/fixtures/png-goldens.sh           regenerate the goldens file
#   scripts/fixtures/png-goldens.sh --check   exit 1 if regenerating would change
#                                             it, then have upstream decode the
#                                             PNGs the Rust port writes
#
# --check builds the `png_embed` example of excali-core (cargo), which writes
# PNGs with `encode_png_metadata` next to the scene text it embedded, and
# runs upstream's `decodePngMetadata` on every one: each must give back the
# embedded text exactly.
#
# No npm package is vendored. Versions, tarball URLs and SHA-512 integrities
# are read from the upstream checkout (packages/excalidraw/package.json and
# yarn.lock, transitive dependencies included); tarballs are downloaded once
# into the shared .tools directory and refused unless the digest matches.
#
# Environment:
#   UPSTREAM_DIR  upstream checkout; default <main clone>/.tools/upstream
#                 (create it with scripts/upstream/checkout.sh)
#   TOOLS_DIR     where packages are unpacked; default <main clone>/.tools
#   NODE          node binary (>= 22.15: type stripping and module.registerHooks)
#   CARGO         cargo binary for --check; default cargo
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
common="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)"
shared="$(dirname "$common")/.tools"
upstream="${UPSTREAM_DIR:-$shared/upstream}"
tools="${TOOLS_DIR:-$shared}"
node="${NODE:-node}"
cargo="${CARGO:-cargo}"
out="$root/crates/excali-core/tests/fixtures/png/goldens.json"

die() { printf 'png goldens: %s\n' "$*" >&2; exit 1; }

mode=write
case "${1:-}" in
  "") ;;
  --check) mode=check ;;
  -h | --help) sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

[ -f "$upstream/packages/excalidraw/data/image.ts" ] ||
  die "no upstream checkout at $upstream (run scripts/upstream/checkout.sh)"

pkg_json="$upstream/packages/excalidraw/package.json"
lockfile="$upstream/yarn.lock"

# The yarn.lock block whose header names `name@spec` (headers may list several
# specs, quoted or not).
lock_block() {
  awk -v want="$1@$2" '
    /^[^ #]/ {
      inblock = 0
      line = $0; sub(/:$/, "", line); gsub(/"/, "", line)
      n = split(line, specs, /, /)
      for (i = 1; i <= n; i++) if (specs[i] == want) inblock = 1
      next
    }
    inblock { print }
  ' "$lockfile"
}

# fetch NAME SPEC: download, verify and unpack the locked release of NAME@SPEC
# and, recursively, its dependencies. Prints "name dir" lines.
fetch() {
  local name="$1" spec="$2" block version url integrity dir tgz got
  block="$(lock_block "$name" "$spec")"
  [ -n "$block" ] || die "$name@$spec not in yarn.lock"
  version="$(printf '%s\n' "$block" | sed -n 's/^  version "\(.*\)"$/\1/p')"
  url="$(printf '%s\n' "$block" | sed -n 's/^  resolved "\([^"#]*\).*"$/\1/p')"
  integrity="$(printf '%s\n' "$block" | sed -n 's/^  integrity sha512-\(.*\)$/\1/p')"
  [ -n "$version" ] && [ -n "$url" ] && [ -n "$integrity" ] ||
    die "cannot read version/resolved/integrity for $name@$spec"
  dir="$tools/$name-$version"
  if [ ! -f "$dir/package/package.json" ]; then
    mkdir -p "$dir"
    tgz="$dir/$name-$version.tgz"
    echo "downloading $url" >&2
    curl -sSfL "$url" -o "$tgz.part"
    got="$(openssl dgst -sha512 -binary "$tgz.part" | openssl base64 -A)"
    [ "$got" = "$integrity" ] || { rm -f "$tgz.part"; die "sha512 mismatch for $url: got $got, want $integrity"; }
    mv "$tgz.part" "$tgz"
    tar -xzf "$tgz" -C "$dir"
  fi
  printf '%s %s\n' "$name" "$dir/package"
  printf '%s\n' "$block" | sed -n 's/^    \([^ ]*\) "\(.*\)"$/\1 \2/p' |
    while read -r dep depspec; do fetch "$dep" "$depspec" || exit 1; done || exit 1
}

direct_spec() {
  sed -n "s/^    \"$1\": \"\\([^\"]*\\)\",*\$/\\1/p" "$pkg_json" | head -n 1
}

args=(--upstream "$upstream" --root "$root")
for name in pako png-chunk-text png-chunks-encode png-chunks-extract; do
  spec="$(direct_spec "$name")"
  [ -n "$spec" ] || die "$name is not a dependency in packages/excalidraw/package.json"
  list="$(fetch "$name" "$spec")" || exit 1
  while read -r pkg dir; do
    args+=(--package "$pkg=$dir")
  done <<< "$list"
done

if [ "$mode" = write ]; then
  mkdir -p "$(dirname "$out")"
  "$node" "$here/png_goldens.mjs" "${args[@]}" > "$out.tmp"
  mv "$out.tmp" "$out"
  echo "wrote ${out#"$root"/}" >&2
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
"$node" "$here/png_goldens.mjs" "${args[@]}" > "$tmp/goldens.json"
if ! cmp -s "$tmp/goldens.json" "$out"; then
  diff -u "$out" "$tmp/goldens.json" | head -n 40 >&2 || true
  die "${out#"$root"/} is stale: run scripts/fixtures/png-goldens.sh"
fi
echo "png goldens match upstream image.ts" >&2

(cd "$root" && "$cargo" run --quiet --locked -p excali-core --example png_embed -- "$tmp/written")
"$node" "$here/png_goldens.mjs" "${args[@]}" --decode-dir "$tmp/written"
