#!/usr/bin/env bash
# Check out upstream Excalidraw at the pinned commit. Every fixture, golden
# and "upstream is the spec" citation in this repository derives from this
# checkout, so the script only ever lands on one exact commit.
#
#   scripts/upstream/checkout.sh               clone or fetch, then detach at the pin
#   scripts/upstream/checkout.sh --verify      exit 0 only if the checkout is clean at the pin
#   scripts/upstream/checkout.sh --print-pin   print the commit that would be used
#   scripts/upstream/checkout.sh --print-dir   print the checkout directory
#
# The pin is site/config.toml extra.upstream_commit (the same value the site
# footer shows). Any other commit is refused unless PIN=<40-hex sha> is set
# explicitly, and an override is announced on stderr every run.
#
# Environment:
#   PIN           explicit override of the pinned commit (full 40-hex sha)
#   UPSTREAM_DIR  checkout location; default <main clone>/.tools/upstream,
#                 shared by every git worktree of this repository
#   UPSTREAM_URL  remote; default site/config.toml extra.upstream
#   UPSTREAM_FETCH_ATTEMPTS  tries per fetch on a network error; default 4
#   UPSTREAM_FETCH_DELAY     seconds before the first retry, doubling; default 5
#
# Idempotent: when the checkout is already clean at the pin it exits without
# touching the network. It never discards local changes, never adopts a
# directory that is not its own checkout, and verifies HEAD before exiting.
#
# Read-only: this repository is strictly a port (owner decision, 2026-09-27).
# Every run sets the checkout's push URL to DISABLED-strictly-a-port so no
# push can reach upstream, and --verify fails unless that is the only push URL.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
config="$root/site/config.toml"

die() { printf 'upstream checkout: %s\n' "$*" >&2; exit 1; }
usage() {
  echo "usage: $0 [--verify | --print-pin | --print-dir]" >&2
  exit 2
}

mode=checkout
case "${1:-}" in
  "") ;;
  --verify) mode=verify ;;
  --print-pin) mode=print-pin ;;
  --print-dir) mode=print-dir ;;
  -h | --help) sed -n '2,29p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) usage ;;
esac
[ $# -le 1 ] || usage

config_value() {
  # Reads `key = "value"` from site/config.toml (flat string keys only).
  sed -n "s/^$1[[:space:]]*=[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" "$config" | head -n 1
}

config_pin="$(config_value upstream_commit)"
[ -n "$config_pin" ] || die "no extra.upstream_commit in $config"

if [ -n "${PIN+set}" ]; then
  pin="$PIN"
else
  pin="$config_pin"
fi
if ! printf '%s' "$pin" | grep -Eq '^[0-9a-f]{40}$'; then
  die "pin '$pin' is not a full 40-character lowercase commit sha (branches, tags and short shas are not pins)"
fi
if [ "$pin" != "$config_pin" ]; then
  printf 'upstream checkout: PIN override: using %s instead of the pinned %s (site/config.toml)\n' \
    "$pin" "$config_pin" >&2
fi

url="${UPSTREAM_URL:-$(config_value upstream)}"
[ -n "$url" ] || die "no extra.upstream in $config and UPSTREAM_URL unset"

attempts="${UPSTREAM_FETCH_ATTEMPTS:-4}"
printf '%s' "$attempts" | grep -Eq '^[1-9][0-9]*$' ||
  die "UPSTREAM_FETCH_ATTEMPTS '$attempts' is not a positive integer"
delay="${UPSTREAM_FETCH_DELAY:-5}"
printf '%s' "$delay" | grep -Eq '^[0-9]+$' ||
  die "UPSTREAM_FETCH_DELAY '$delay' is not a whole number of seconds"

if [ -n "${UPSTREAM_DIR:-}" ]; then
  dest="$UPSTREAM_DIR"
else
  common="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)"
  dest="$(dirname "$common")/.tools/upstream"
fi

case "$mode" in
  print-pin) echo "$pin"; exit 0 ;;
  print-dir) echo "$dest"; exit 0 ;;
esac

# -- helpers over an existing directory ---------------------------------------

is_own_checkout() {
  [ -e "$1/.git" ] || return 1
  local top
  top="$(git -C "$1" rev-parse --show-toplevel 2>/dev/null)" || return 1
  [ "$(cd "$top" && pwd -P)" = "$(cd "$1" && pwd -P)" ]
}

head_of() { git -C "$1" rev-parse -q --verify 'HEAD^{commit}' 2>/dev/null || true; }

require_clean() {
  local dirty
  dirty="$(git -C "$1" status --porcelain --untracked-files=normal)"
  if [ -n "$dirty" ]; then
    printf '%s\n' "$dirty" | head -n 20 >&2
    die "$1 has modified or untracked files; refusing to touch it (inspect and clean it by hand)"
  fi
}

require_origin() {
  local have
  have="$(git -C "$1" remote get-url origin 2>/dev/null || true)"
  [ "$have" = "$url" ] || die "$1 origin is '$have', expected '$url'; refusing to use a foreign checkout"
}

# Strictly a port: nothing is ever pushed upstream. An invalid push URL makes
# `git push` fail in this checkout while fetches keep using the real URL.
disabled_push_url=DISABLED-strictly-a-port

disable_push() {
  # set-url --push replaces the first pushurl only; clear every one first so
  # exactly one remains.
  git -C "$1" config --unset-all remote.origin.pushurl 2>/dev/null || true
  git -C "$1" remote set-url --push origin "$disabled_push_url"
}

require_push_disabled() {
  local have
  have="$(git -C "$1" config --get-all remote.origin.pushurl 2>/dev/null || true)"
  [ "$have" = "$disabled_push_url" ] ||
    die "$1 push URL is '$(printf '%s' "$have" | tr '\n' ' ')', expected only '$disabled_push_url' (strictly a port; run $0 to fix)"
}

# -- verify -------------------------------------------------------------------

if [ "$mode" = verify ]; then
  is_own_checkout "$dest" || die "$dest is not a git checkout (run $0 first)"
  require_origin "$dest"
  require_push_disabled "$dest"
  have="$(head_of "$dest")"
  [ "$have" = "$pin" ] || die "HEAD of $dest is '${have:-none}', expected $pin"
  require_clean "$dest"
  echo "upstream verified: $dest at $pin"
  exit 0
fi

# -- checkout -----------------------------------------------------------------

created=""
if [ -d "$dest" ] && [ -n "$(ls -A "$dest")" ]; then
  is_own_checkout "$dest" || die "$dest exists and is not a git checkout; refusing to overwrite it"
  require_origin "$dest"
  require_clean "$dest"
  disable_push "$dest"
  if [ "$(head_of "$dest")" = "$pin" ]; then
    echo "upstream already at $pin in $dest"
    exit 0
  fi
elif [ -e "$dest" ] && [ ! -d "$dest" ]; then
  die "$dest exists and is not a directory"
else
  if [ -d "$dest" ]; then created=git-only; else created=dir; fi
  mkdir -p "$dest"
  git -C "$dest" init -q
  git -C "$dest" remote add origin "$url"
  disable_push "$dest"
fi

cleanup_on_failure() {
  local status=$?
  if [ "$status" -ne 0 ]; then
    rm -f "${fetch_err:-}"
    case "$created" in
      dir) rm -rf "$dest" ;;
      git-only) rm -rf "$dest/.git" ;;
    esac
  fi
}
trap cleanup_on_failure EXIT

has_pin() { git -C "$dest" cat-file -e "$pin^{commit}" 2>/dev/null; }

# fetch <args>: git fetch from origin, retried with doubling delays while the
# error is a transport failure (DNS, connection, TLS, HTTP 5xx), which a hosted
# runner sees now and then. Returns 1 at once for any other error (e.g. a
# server refusing fetch-by-sha) and 2 once the network attempts are used up.
fetch_err="$(mktemp)"
fetch() {
  local try=1 wait="$delay"
  while :; do
    if git -C "$dest" fetch -q "$@" 2>"$fetch_err"; then
      return 0
    fi
    grep -Eqi 'could not resolve|unable to access|timed out|connection (refused|reset|closed|timed)|early eof|rpc failed|hung up unexpectedly|ssl|gnutls|returned error: 5[0-9][0-9]|network is unreachable' "$fetch_err" ||
      return 1
    cat "$fetch_err" >&2
    if [ "$try" -ge "$attempts" ]; then
      return 2
    fi
    printf 'upstream checkout: network error (attempt %s of %s), retrying in %ss\n' \
      "$try" "$attempts" "$wait" >&2
    sleep "$wait"
    try=$((try + 1))
    wait=$((wait * 2))
  done
}

if ! has_pin; then
  echo "fetching $pin from $url"
  # Fast path: fetch exactly the pinned commit (GitHub serves any sha).
  status=0
  fetch --depth 1 origin "$pin" || status=$?
  [ "$status" -ne 2 ] || die "fetch from $url failed after $attempts attempts"
  if [ "$status" -ne 0 ] || ! has_pin; then
    # Fallback for servers that refuse fetch-by-sha: fetch the refs and
    # history, then look for the commit locally.
    echo "fetch by sha refused; fetching all refs"
    unshallow=""
    if [ "$(git -C "$dest" rev-parse --is-shallow-repository)" = true ]; then
      unshallow="--unshallow"
    fi
    status=0
    fetch $unshallow --tags origin '+refs/heads/*:refs/remotes/origin/*' || status=$?
    case "$status" in
      0) ;;
      2) die "fetch from $url failed after $attempts attempts" ;;
      *) cat "$fetch_err" >&2; die "fetch from $url failed" ;;
    esac
  fi
  has_pin || die "commit $pin not found at $url"
fi

git -C "$dest" -c advice.detachedHead=false checkout -q --detach "$pin"

have="$(head_of "$dest")"
[ "$have" = "$pin" ] || die "HEAD of $dest is '$have' after checkout, expected $pin"
require_clean "$dest"
created=""
rm -f "$fetch_err"
echo "upstream at $pin in $dest"
