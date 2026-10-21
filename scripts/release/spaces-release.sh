#!/bin/sh
# The Spaces release of a tag (ex-e0.1), for .workflows/release.yml: the
# counterpart of the `gh release` calls in .github/workflows/release.yml,
# through Spaces' (Gitea-compatible) releases API.
#
#   scripts/release/spaces-release.sh download-sums DIR
#       DIR/SHA256SUMS from the release of $TAG, when it has one (so lines for
#       other assets are kept); nothing when there is no release yet
#   scripts/release/spaces-release.sh upload NOTES FILE...
#       create the release of $TAG (title $TAG, body NOTES) when there is
#       none, else replace its body with NOTES when NOTES is
#       scripts/release/notes/$TAG.md; then upload each FILE as an asset,
#       replacing an asset of the same name
#
# Environment: TAG, SPACES_TOKEN (the job's token), SERVER (e.g.
# https://spaces.hutson.cloud), REPO (owner/name). Needs curl and jq.
set -eu
: "${TAG:?}" "${SPACES_TOKEN:?}" "${SERVER:?}" "${REPO:?}"
api="$SERVER/api/v1/repos/$REPO"
auth="Authorization: token $SPACES_TOKEN"

# The release of $TAG as JSON, or nothing when there is none.
release() {
  code="$(curl -sS -o "$tmp/release.json" -w '%{http_code}' -H "$auth" "$api/releases/tags/$TAG")"
  case "$code" in
    200) cat "$tmp/release.json" ;;
    404) ;;
    *) echo "spaces-release: GET release $TAG: HTTP $code" >&2; cat "$tmp/release.json" >&2; exit 1 ;;
  esac
}

tmp="$(mktemp -d "${TMPDIR:-/tmp}/spaces-release.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

case "${1:-}" in
  download-sums)
    dir="${2:?usage: $0 download-sums DIR}"
    mkdir -p "$dir"
    rel="$(release)"
    [ -n "$rel" ] || { echo "no release $TAG yet"; exit 0; }
    url="$(printf '%s' "$rel" | jq -r '.assets[]? | select(.name == "SHA256SUMS") | .browser_download_url' | head -n 1)"
    if [ -n "$url" ]; then
      curl -fsSL -H "$auth" -o "$dir/SHA256SUMS" "$url"
      echo "kept SHA256SUMS of release $TAG"
    fi
    ;;
  upload)
    notes="${2:?usage: $0 upload NOTES FILE...}"
    shift 2
    rel="$(release)"
    if [ -z "$rel" ]; then
      jq -n --arg t "$TAG" --rawfile b "$notes" '{tag_name: $t, name: $t, body: $b}' > "$tmp/create.json"
      rel="$(curl -fsS -H "$auth" -H 'Content-Type: application/json' --data @"$tmp/create.json" "$api/releases")"
      echo "created release $TAG"
    elif [ "$notes" = "scripts/release/notes/$TAG.md" ]; then
      id="$(printf '%s' "$rel" | jq -r .id)"
      jq -n --rawfile b "$notes" '{body: $b}' > "$tmp/edit.json"
      curl -fsS -o /dev/null -X PATCH -H "$auth" -H 'Content-Type: application/json' --data @"$tmp/edit.json" "$api/releases/$id"
      echo "release $TAG notes from $notes"
    fi
    id="$(printf '%s' "$rel" | jq -r .id)"
    for f in "$@"; do
      name="$(basename "$f")"
      for old in $(printf '%s' "$rel" | jq -r --arg n "$name" '.assets[]? | select(.name == $n) | .id'); do
        curl -fsS -o /dev/null -X DELETE -H "$auth" "$api/releases/$id/assets/$old"
      done
      curl -fsS -o /dev/null -H "$auth" -F "attachment=@$f" "$api/releases/$id/assets?name=$name"
      echo "uploaded $name to release $TAG"
    done
    ;;
  *)
    sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
    ;;
esac
