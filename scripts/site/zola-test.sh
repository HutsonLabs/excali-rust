#!/usr/bin/env bash
# Tests for scripts/site/zola.sh. Offline: `ensure` is exercised against a
# local file:// "release" whose tarball holds a fake zola binary, so the
# download, SHA-256 verification, extraction and install paths all run without
# network access. Written for bash 3.2 (macOS /bin/bash) as well as bash 5.
#
# Usage: scripts/site/zola-test.sh [path-to-bash]
#   path-to-bash defaults to /bin/bash so macOS runs the script under 3.2,
#   which is the interpreter that broke `declare -A`.
# Single-quoted $ in generated stub scripts is intentional.
# shellcheck disable=SC2016
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
zola_sh="$here/zola.sh"
BASH_UNDER_TEST="${1:-/bin/bash}"
work="$(mktemp -d "${TMPDIR:-/tmp}/zola-test.XXXXXX")"
trap 'rm -rf "$work"' EXIT

pass=0
fail=0
ok() { pass=$((pass + 1)); echo "ok   - $*"; }
bad() { fail=$((fail + 1)); echo "FAIL - $*"; }

# Digests published by GitHub for the v0.22.0 release assets
# (gh api repos/getzola/zola/releases/tags/v0.22.0 -> assets[].digest,
# read 2026-09-27) and recomputed locally from the downloaded tarballs.
expect_digest() {
  case "$1" in
    x86_64-unknown-linux-gnu) echo f1d491f8956b94384c27d75cb6b2bf60d3916d1ade9564bcbfe7c03f0258aebf ;;
    aarch64-unknown-linux-gnu) echo 14ef16bfb36ff3911a0fcddbbd20e74d1383ac87cce4c2b097c9055fc1c98e87 ;;
    aarch64-apple-darwin) echo 96015a922a7d83827e381e273aef6be916711d43e89e65e8e82b4da0350fc425 ;;
    x86_64-apple-darwin) echo f0268e7559d8b6b79d50cef1cd6025a41819cbab920c3c5b0854e0de3a6584b9 ;;
  esac
}

sha256_of() {
  python3 -c 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$1"
}

# A PATH holding only the tools zola.sh needs, so the tests control which
# sha256 implementation is visible and no system zola short-circuits ensure.
# $1 = dir, $2 = which sha tools to expose: "sha256sum", "shasum", "both", "none"
make_path() {
  local dir="$1" which="$2" t p
  mkdir -p "$dir"
  for t in git uname curl tar mkdir mv rm grep cat dirname env chmod mktemp cut tr head sed awk printf; do
    p="$(command -v "$t" 2>/dev/null)" || continue
    case "$p" in /*) ln -sf "$p" "$dir/$t" ;; esac
  done
  local real_sum real_shasum
  real_sum="$(command -v sha256sum 2>/dev/null || true)"
  real_shasum="$(command -v shasum 2>/dev/null || true)"
  if [ "$which" = sha256sum ] || [ "$which" = both ]; then
    if [ -n "$real_sum" ]; then
      ln -sf "$real_sum" "$dir/sha256sum"
    else
      # macOS has no sha256sum: stand one in with the coreutils calling form.
      printf '#!/bin/sh\nexec %s -a 256 "$@"\n' "$real_shasum" >"$dir/sha256sum"
      chmod +x "$dir/sha256sum"
    fi
  fi
  if [ "$which" = shasum ] || [ "$which" = both ]; then
    if [ -n "$real_shasum" ]; then
      ln -sf "$real_shasum" "$dir/shasum"
    else
      # Linux images without perl's shasum: stand one in over sha256sum.
      printf '#!/bin/sh\n[ "$1" = -a ] && shift 2\nexec %s "$@"\n' "$real_sum" >"$dir/shasum"
      chmod +x "$dir/shasum"
    fi
  fi
}

# Fake release: <rel>/v0.22.0/zola-v0.22.0-<target>.tar.gz containing `zola`.
target="$("$BASH_UNDER_TEST" "$zola_sh" target 2>/dev/null)"
rel="$work/release"
mkdir -p "$rel/v0.22.0" "$work/stage"
cat >"$work/stage/zola" <<'EOF'
#!/bin/sh
if [ "${1:-}" = --version ]; then echo "zola 0.22.0"; exit 0; fi
echo "fake-zola $*"
EOF
chmod +x "$work/stage/zola"
tgz="$rel/v0.22.0/zola-v0.22.0-${target:-unknown}.tar.gz"
tar -czf "$tgz" -C "$work/stage" zola
good_sha="$(sha256_of "$tgz")"
bad_sha="0000000000000000000000000000000000000000000000000000000000000000"

# run_ensure <case-name> <sha-tools> [VAR=value ...]; sets out, rc, tools
run_ensure() {
  local name="$1" which="$2"
  shift 2
  tools="$work/$name/tools"
  make_path "$work/$name/bin" "$which"
  out="$(cd "$here" && env -i HOME="$HOME" PATH="$work/$name/bin" \
    ZOLA_TOOLS_DIR="$tools" ZOLA_BASE_URL="file://$rel" "$@" \
    "$BASH_UNDER_TEST" "$zola_sh" ensure 2>&1)"
  rc=$?
}

echo "# bash under test: $("$BASH_UNDER_TEST" -c 'echo $BASH_VERSION')"

# 1. The script loads and prints usage under this bash (3.2 died at load with
#    "x86_64: unbound variable" from declare -A).
out="$("$BASH_UNDER_TEST" "$zola_sh" 2>&1)"; rc=$?
if [ $rc -eq 2 ] && echo "$out" | grep -q '^usage:' && ! echo "$out" | grep -q 'unbound variable'; then
  ok "usage exits 2 without unbound-variable errors"
else
  bad "usage: rc=$rc out=$out"
fi

# 2. target maps this host to a release triple.
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) want_t=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) want_t=aarch64-unknown-linux-gnu ;;
  Darwin-arm64) want_t=aarch64-apple-darwin ;;
  Darwin-x86_64) want_t=x86_64-apple-darwin ;;
  *) want_t="" ;;
esac
if [ -n "$want_t" ] && [ "$target" = "$want_t" ]; then ok "target is $target"; else bad "target '$target' != '$want_t'"; fi

# 3. Every published unix target has its digest pinned, byte for byte.
for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu aarch64-apple-darwin x86_64-apple-darwin; do
  got="$("$BASH_UNDER_TEST" "$zola_sh" digest "$t" 2>&1)"; rc=$?
  if [ $rc -eq 0 ] && [ "$got" = "$(expect_digest "$t")" ]; then ok "digest $t"; else bad "digest $t: rc=$rc got=$got"; fi
done

# 4. An unknown target has no digest and says so.
got="$("$BASH_UNDER_TEST" "$zola_sh" digest riscv64-unknown-linux-gnu 2>&1)"; rc=$?
if [ $rc -ne 0 ] && echo "$got" | grep -q 'no pinned sha256'; then ok "unknown target has no digest"; else bad "unknown digest: rc=$rc got=$got"; fi

# 5. Pinned digests for this host's target are used when ZOLA_SHA256 is unset:
#    the fake tarball does not match the real release digest, so ensure must
#    refuse it and install nothing.
run_ensure pinned both
if [ $rc -ne 0 ] && echo "$out" | grep -qi 'sha256 mismatch' && [ ! -e "$tools/zola-0.22.0" ]; then
  ok "pinned digest rejects a tarball that is not the release"
else
  bad "pinned digest: rc=$rc out=$out"
fi

# 6-8. Verification works with whichever sha256 tool the host has.
for which in shasum sha256sum both; do
  run_ensure "good-$which" "$which" ZOLA_SHA256="$good_sha"
  if [ $rc -eq 0 ] && echo "$out" | grep -q 'sha256 verified' && [ -x "$tools/zola-0.22.0" ] \
    && [ "$("$tools/zola-0.22.0" --version)" = "zola 0.22.0" ] && [ ! -e "$tools/zola" ] \
    && ! ls "$tools"/*.tar.gz >/dev/null 2>&1; then
    ok "ensure verifies and installs with $which"
  else
    bad "ensure with $which: rc=$rc out=$out"
  fi
done

# 9. A wrong digest fails, installs nothing and leaves no tarball behind.
run_ensure badsha both ZOLA_SHA256="$bad_sha"
if [ $rc -ne 0 ] && echo "$out" | grep -qi 'sha256 mismatch' && [ ! -e "$tools/zola-0.22.0" ] \
  && ! ls "$tools"/*.tar.gz >/dev/null 2>&1; then
  ok "wrong digest is rejected"
else
  bad "wrong digest: rc=$rc out=$out"
fi

# 10. No sha256 tool at all is an error, never a silent skip.
run_ensure nosha none ZOLA_SHA256="$good_sha"
if [ $rc -ne 0 ] && echo "$out" | grep -q 'no sha256 tool' && [ ! -e "$tools/zola-0.22.0" ]; then
  ok "missing sha256 tool is an error"
else
  bad "no sha tool: rc=$rc out=$out"
fi

# 11. A target with no pinned digest and no ZOLA_SHA256 is refused (it used to
#     install unverified with a warning).
run_ensure unpinned both ZOLA_TARGET=riscv64-unknown-linux-gnu
if [ $rc -ne 0 ] && echo "$out" | grep -q 'no pinned sha256' && [ ! -e "$tools/zola-0.22.0" ]; then
  ok "unpinned target is refused"
else
  bad "unpinned: rc=$rc out=$out"
fi

# 12. Second ensure is a no-op that reports the installed binary.
run_ensure good-both both ZOLA_SHA256="$bad_sha"
if [ $rc -eq 0 ] && echo "$out" | grep -q "$tools/zola-0.22.0"; then
  ok "ensure is idempotent"
else
  bad "idempotent: rc=$rc out=$out"
fi

# 13. build runs the installed binary from site/ with arguments passed through
#     (an empty argument list must also survive set -u on bash 3.2).
tools="$work/good-both/tools"
out="$(env -i HOME="$HOME" PATH="$work/good-both/bin" ZOLA_TOOLS_DIR="$tools" \
  "$BASH_UNDER_TEST" "$zola_sh" build 2>&1)"; rc=$?
if [ $rc -eq 0 ] && [ "$out" = "fake-zola build" ]; then ok "build with no extra args"; else bad "build: rc=$rc out=$out"; fi
out="$(env -i HOME="$HOME" PATH="$work/good-both/bin" ZOLA_TOOLS_DIR="$tools" \
  "$BASH_UNDER_TEST" "$zola_sh" check --drafts 2>&1)"; rc=$?
if [ $rc -eq 0 ] && [ "$out" = "fake-zola check --drafts" ]; then ok "check passes args through"; else bad "check: rc=$rc out=$out"; fi

# 14. The default tools dir is the main clone's .tools, shared by worktrees.
common="$(cd "$here" && cd "$(git rev-parse --git-common-dir)" && pwd -P)"
got="$(cd "$here" && "$BASH_UNDER_TEST" "$zola_sh" tools-dir 2>&1)"; rc=$?
if [ $rc -eq 0 ] && [ "$got" = "$(dirname "$common")/.tools" ]; then ok "tools dir is $got"; else bad "tools-dir: rc=$rc got=$got"; fi

# 15. Linked worktrees share the main clone's .tools even on git < 2.31, which
#     has no `rev-parse --path-format` (it used to fall back to
#     <worktree>/.tools). A wrapper git rejects that option like old git does.
repo="$work/wt-repo"
mkdir -p "$repo"
(
  cd "$repo" && git init -q . && git -c user.name=t -c user.email=t@example.invalid \
    -c commit.gpgsign=false -c core.hooksPath=/dev/null commit -q --allow-empty -m init \
    && git worktree add -q "$work/wt-linked" 2>/dev/null
) || bad "could not create a linked worktree fixture"
repo_abs="$(cd "$repo" && pwd -P)"
make_path "$work/oldgit/bin" both
real_git="$(command -v git)"
rm -f "$work/oldgit/bin/git"
printf '#!/bin/sh\nfor a in "$@"; do\n  case "$a" in --path-format*) echo "error: unknown option $a" >&2; exit 129 ;; esac\ndone\nexec %s "$@"\n' \
  "$real_git" >"$work/oldgit/bin/git"
chmod +x "$work/oldgit/bin/git"
for where in "$work/wt-linked" "$work/wt-linked/sub" "$repo" "$repo/sub"; do
  mkdir -p "$where"
  for gitdir in "$work/oldgit/bin" "$(dirname "$real_git")"; do
    got="$(cd "$where" && env -i HOME="$HOME" PATH="$gitdir:$work/oldgit/bin" \
      "$BASH_UNDER_TEST" "$zola_sh" tools-dir 2>&1)"; rc=$?
    got_abs="$(cd "$(dirname "$got")" 2>/dev/null && pwd -P)/.tools"
    if [ "$gitdir" = "$work/oldgit/bin" ]; then which_git="old git"; else which_git="current git"; fi
    label="tools-dir from ${where#"$work"/} with $which_git"
    if [ $rc -eq 0 ] && [ "$got_abs" = "$repo_abs/.tools" ] && [ "${got#/}" != "$got" ]; then
      ok "$label is the main clone's .tools"
    else
      bad "$label: rc=$rc got=$got want=$repo_abs/.tools"
    fi
  done
done

echo "# $pass passed, $fail failed"
[ $fail -eq 0 ]
