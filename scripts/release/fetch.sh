#!/bin/sh
# Fetch the web runtime of an excali-rust release (ex-802) into a directory,
# as a host's vendor script does (term.hut: scripts/vendor-excali.sh, in the
# pattern of its scripts/vendor-catppuccin-icons.sh):
#
#   scripts/release/fetch.sh VERSION OUT      e.g. 26.9.1 ui/vendor/excali
#
# Downloads excali-web_<VERSION>.tar.gz and SHA256SUMS from the GitHub
# release v<VERSION>, checks the tarball against its line in SHA256SUMS,
# unpacks it (excali_editor.js, excali_editor_bg.wasm, excali.css, fonts/)
# and replaces OUT with it. Nothing in OUT changes unless every step passed.
#
# EXCALI_RELEASE_URL overrides the download root (default
# https://github.com/HutsonLabs/excali-rust/releases/download; the tests use
# a file:// URL). Needs curl, tar and shasum or sha256sum.
# The assets are written by scripts/release/package.py.
# Tests: scripts/release/test_release.py
set -eu

usage="usage: scripts/release/fetch.sh VERSION OUT"
version="${1:?$usage}"
out="${2:?$usage}"
base="${EXCALI_RELEASE_URL:-https://github.com/HutsonLabs/excali-rust/releases/download}"
asset="excali-web_$version.tar.gz"
url="$base/v$version"

stage="$(mktemp -d "${TMPDIR:-/tmp}/excali-fetch.XXXXXX")"
trap 'rm -rf "$stage"' EXIT

echo "==> Fetching excali-rust v$version web runtime from $url"
curl -fsSL -o "$stage/$asset" "$url/$asset"
curl -fsSL -o "$stage/SHA256SUMS" "$url/SHA256SUMS"

want="$(awk -v f="$asset" '$2 == f { print $1; exit }' "$stage/SHA256SUMS")"
if [ -z "$want" ]; then
  echo "fetch: $asset is not listed in the release's SHA256SUMS" >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  have="$(sha256sum "$stage/$asset" | awk '{ print $1 }')"
else
  have="$(shasum -a 256 "$stage/$asset" | awk '{ print $1 }')"
fi
if [ "$have" != "$want" ]; then
  echo "fetch: $asset has SHA-256 $have, the release's SHA256SUMS says $want" >&2
  exit 1
fi

mkdir "$stage/excali"
tar -xzf "$stage/$asset" -C "$stage/excali"
for entry in excali_editor.js excali_editor_bg.wasm excali.css fonts/manifest.json; do
  if [ ! -f "$stage/excali/$entry" ]; then
    echo "fetch: $asset has no $entry" >&2
    exit 1
  fi
done

mkdir -p "$(dirname "$out")"
rm -rf "$out"
mv "$stage/excali" "$out"
echo "==> $out: excali-rust $version web runtime (sha256 $have)"
