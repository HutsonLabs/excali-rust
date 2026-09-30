#!/usr/bin/env bash
# The example app's signed, notarized macOS release (ex-805), after term.hut's
# scripts/release.sh and scripts/publish-release.sh: builds
# examples/tauri-app with the Developer ID identity given here (never in
# tauri.conf.json), notarizes and staples the DMG, and stages what a GitHub
# release carries for it:
#
#   Excali.Example_<V>_aarch64.dmg              the disk image
#   Excali.Example_<V>_aarch64.app.tar.gz       what the in-app updater downloads
#   Excali.Example_<V>_aarch64.app.tar.gz.sig   its minisign signature
#   latest.json                                 the updater's endpoint document
#   SHA256SUMS                                  a line for each of the above
#
# Usage:
#   scripts/release/macos-dmg.sh <version> [--upload <tag>] [--out DIR] [--dry-run]
#
#   <version>        the app's calendar version (YY.M.BUILD); must be the
#                    version in examples/tauri-app/src-tauri/tauri.conf.json
#   --upload <tag>   then `gh release upload <tag> ... --clobber`, with
#                    SHA256SUMS merged into the release's existing one; the tag
#                    must be v<version>, where latest.json points
#   --out DIR        staging directory (default target/macos-dmg/<version>)
#   --dry-run        check the arguments and print every step; reads no
#                    credential, builds, signs and uploads nothing
#
# Environment:
#   EXCALI_WEB_DIST          a web runtime build to bundle (default: the
#                            release's excali-web_<version>.tar.gz, fetched and
#                            checked by scripts/release/fetch.sh)
#   EXCALI_RELEASE_ENV       the credentials file (default
#                            ~/code/term.hut/.env: APPLE_API_KEY_PATH,
#                            APPLE_KEY_ID, APPLE_API_ISSUER, APPLE_API_KEY,
#                            SIGNING_KEYCHAIN_PASSWORD)
#   APPLE_SIGNING_IDENTITY   the Developer ID Application identity (default
#                            FE9B9ADD91CB67176BFE80FF725F77539D75BD96)
#   EXCALI_UPDATER_KEY_ITEM  the keychain item holding the updater's minisign
#                            private key (default excali-example-updater-key,
#                            account $USER); its public key is plugins.updater
#                            in tauri.conf.json
#   CARGO_TARGET_DIR         as for cargo
#
# Tests: scripts/release/test_release.py (arguments, dry run, latest.json).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP="$ROOT/examples/tauri-app/src-tauri"
USAGE="usage: scripts/release/macos-dmg.sh <version> [--upload <tag>] [--out DIR] [--dry-run]"
REPO="HutsonLabs/excali-rust"

# Arguments are parsed in full before anything runs: a mistyped flag must
# never fall through to a build or an upload.
VERSION="" TAG="" OUT="" DRY_RUN=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --upload)  [[ $# -ge 2 && "$2" != -* ]] || { echo "$USAGE" >&2; echo "--upload needs a tag" >&2; exit 2; }
               TAG="$2"; shift ;;
    --out)     [[ $# -ge 2 && "$2" != -* ]] || { echo "$USAGE" >&2; echo "--out needs a directory" >&2; exit 2; }
               OUT="$2"; shift ;;
    -h|--help) echo "$USAGE"; exit 0 ;;
    -*)        echo "$USAGE" >&2; echo "unknown option: $1" >&2; exit 2 ;;
    *)         [[ -z "$VERSION" ]] || { echo "$USAGE" >&2; echo "unexpected argument: $1" >&2; exit 2; }
               VERSION="$1" ;;
  esac
  shift
done
[[ -n "$VERSION" ]] || { echo "$USAGE" >&2; exit 2; }
if ! python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import version; version.parse(sys.argv[2])' \
    "$ROOT/scripts/gates" "$VERSION" 2>/dev/null; then
  echo "'$VERSION' is not a calendar version YY.M.BUILD (e.g. 26.9.1)" >&2
  exit 2
fi
CONF_VERSION="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["version"])' "$APP/tauri.conf.json")"
if [[ "$CONF_VERSION" != "$VERSION" ]]; then
  echo "tauri.conf.json is at $CONF_VERSION, not $VERSION: build from the commit of the release" >&2
  exit 2
fi
if [[ -n "$TAG" && "$TAG" != "v$VERSION" ]]; then
  echo "--upload $TAG: latest.json points at v$VERSION, so the tag must be v$VERSION" >&2
  exit 2
fi
OUT="${OUT:-$ROOT/target/macos-dmg/$VERSION}"

ENV_FILE="${EXCALI_RELEASE_ENV:-$HOME/code/term.hut/.env}"
KEYCHAIN="$HOME/Library/Keychains/term-hut-signing.keychain-db"
SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:-FE9B9ADD91CB67176BFE80FF725F77539D75BD96}"
UPDATER_KEY_ITEM="${EXCALI_UPDATER_KEY_ITEM:-excali-example-updater-key}"
BUNDLE="${CARGO_TARGET_DIR:-$APP/target}/release/bundle"
PRODUCT="Excali Example"
NAME="Excali.Example_${VERSION}_aarch64"
DMG_NAME="$NAME.dmg"
TARBALL_NAME="$NAME.app.tar.gz"
BUILD_CONFIG="{\"bundle\":{\"macOS\":{\"signingIdentity\":\"$SIGNING_IDENTITY\"}}}"
PACKAGE=(python3 "$ROOT/scripts/release/package.py")

# Each step is printed; a dry run stops there.
step() { echo "==> $*"; }
run() {
  printf '+'; printf ' %q' "$@"; printf '\n'
  [[ -n "$DRY_RUN" ]] || "$@"
}

if [[ -z "$DRY_RUN" && "$(uname -sm)" != "Darwin arm64" ]]; then
  echo "builds the aarch64 image on an Apple silicon Mac (this is $(uname -sm))" >&2
  exit 1
fi
[[ -n "$DRY_RUN" ]] && step "dry run: $VERSION, staging in $OUT${TAG:+, uploading to $TAG}"

# --- web runtime -------------------------------------------------------------
if [[ -n "${EXCALI_WEB_DIST:-}" ]]; then
  step "web runtime: $EXCALI_WEB_DIST"
else
  WEB="$OUT/.web"
  step "web runtime: the release's excali-web_$VERSION.tar.gz"
  run sh "$ROOT/scripts/release/fetch.sh" "$VERSION" "$WEB"
  export EXCALI_WEB_DIST="$WEB"
fi

# --- credentials -------------------------------------------------------------
step "credentials from $ENV_FILE"
if [[ -z "$DRY_RUN" ]]; then
  [[ -f "$ENV_FILE" ]] || { echo "$ENV_FILE not found (EXCALI_RELEASE_ENV)" >&2; exit 1; }
  set -a
  # shellcheck source=/dev/null
  . "$ENV_FILE"
  set +a
  for var in APPLE_API_KEY_PATH APPLE_KEY_ID APPLE_API_ISSUER; do
    [[ -n "${!var:-}" ]] || { echo "$var is not set in $ENV_FILE" >&2; exit 1; }
  done
fi
# The dedicated keychain unlocks without a GUI session (SSH, a detached job).
if [[ -n "$DRY_RUN" ]]; then
  step "unlock $KEYCHAIN if present (SIGNING_KEYCHAIN_PASSWORD)"
elif [[ -f "$KEYCHAIN" && -n "${SIGNING_KEYCHAIN_PASSWORD:-}" ]]; then
  security unlock-keychain -p "$SIGNING_KEYCHAIN_PASSWORD" "$KEYCHAIN"
  step "unlocked $KEYCHAIN"
fi
step "signing identity $SIGNING_IDENTITY"
if [[ -z "$DRY_RUN" ]] && ! grep -q "$SIGNING_IDENTITY" <<<"$(security find-identity -v -p codesigning)"; then
  echo "$SIGNING_IDENTITY is not a valid codesigning identity (security find-identity -v -p codesigning)" >&2
  exit 1
fi
# The updater key (minisign) signs the .app.tar.gz; never printed.
step "updater signing key from keychain item $UPDATER_KEY_ITEM"
if [[ -z "$DRY_RUN" ]]; then
  TAURI_SIGNING_PRIVATE_KEY="$(security find-generic-password -s "$UPDATER_KEY_ITEM" -a "${USER:-$(id -un)}" -w)"
  export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
fi

# --- build, sign ---------------------------------------------------------------
# CI=true skips bundle_dmg.sh's Finder layout pass, which fails without a GUI
# session; the image, its Applications link and its signature are the same.
step "build and sign (the app is notarized by the bundler with APPLE_API_*)"
export CI=true
run cd "$APP"
run cargo tauri build --bundles app,dmg --config "$BUILD_CONFIG"
APP_BUNDLE="$BUNDLE/macos/$PRODUCT.app"
DMG="$BUNDLE/dmg/${PRODUCT}_${VERSION}_aarch64.dmg"
UPDATER="$BUNDLE/macos/$PRODUCT.app.tar.gz"
if [[ -z "$DRY_RUN" ]]; then
  for f in "$APP_BUNDLE" "$DMG" "$UPDATER" "$UPDATER.sig"; do
    [[ -e "$f" ]] || { echo "not built: $f" >&2; exit 1; }
  done
fi
run codesign --verify --deep --strict "$APP_BUNDLE"

# --- notarize the dmg ----------------------------------------------------------
# The bundler notarizes the .app, not the image: submit and staple it here.
step "notarize, staple and assess $DMG"
if [[ -n "$DRY_RUN" ]]; then
  # shellcheck disable=SC2016 # the variables' names, not their values
  run xcrun notarytool submit "$DMG" --key '$APPLE_API_KEY_PATH' --key-id '$APPLE_KEY_ID' --issuer '$APPLE_API_ISSUER' --wait
else
  run xcrun notarytool submit "$DMG" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_KEY_ID" --issuer "$APPLE_API_ISSUER" --wait
fi
run xcrun stapler staple "$DMG"
run xcrun stapler validate "$DMG"
run spctl -a -t open --context context:primary-signature -v "$DMG"

# --- stage ---------------------------------------------------------------------
step "stage in $OUT"
run mkdir -p "$OUT"
run cp "$DMG" "$OUT/$DMG_NAME"
run cp "$UPDATER" "$OUT/$TARBALL_NAME"
run cp "$UPDATER.sig" "$OUT/$TARBALL_NAME.sig"
run "${PACKAGE[@]}" latest-json "$OUT" "$TARBALL_NAME" --version "$VERSION"
step "latest.json url: https://github.com/$REPO/releases/download/v$VERSION/$TARBALL_NAME"
ASSETS=("$DMG_NAME" "$TARBALL_NAME" "$TARBALL_NAME.sig" latest.json)

# --- upload --------------------------------------------------------------------
if [[ -n "$TAG" ]]; then
  # SHA256SUMS keeps the release's other lines (the web tarball's).
  step "upload to $TAG, SHA256SUMS merged with the release's"
  run rm -f "$OUT/SHA256SUMS"
  run gh release download "$TAG" -R "$REPO" -p SHA256SUMS -D "$OUT"
fi
run "${PACKAGE[@]}" sums "$OUT" "${ASSETS[@]}"
if [[ -n "$TAG" ]]; then
  UPLOAD=()
  for a in "${ASSETS[@]}" SHA256SUMS; do UPLOAD+=("$OUT/$a"); done
  run gh release upload "$TAG" -R "$REPO" --clobber "${UPLOAD[@]}"
fi
step "done: $OUT"
