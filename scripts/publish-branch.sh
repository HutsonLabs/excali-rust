#!/usr/bin/env bash
# Publish a branch delivered as a git bundle, as the human, with gh.
#
# Hosted agent containers cannot push to this repository with a human
# identity, so a session hands back a bundle and this script does the push
# and the pull request from a machine where `gh auth login` was done by the
# responsible human. Every commit is checked by the authorship gate before it
# leaves; the script refuses to push otherwise.
#
# Usage:
#   scripts/publish-branch.sh <bundle> [--repo OWNER/NAME] [--base BRANCH]
#                             [--dir CLONE_DIR] [--as LOGIN] [--no-pr] [--draft]
#
# Defaults: --repo HutsonLabs/excali-rust, --base main, --dir ./<repo name>
#           (or the current clone when run inside one), --as the login gh is
#           authenticated as.
#
# Requires: git >= 2.30, gh >= 2.40 (https://cli.github.com), python3.
set -euo pipefail

repo="HutsonLabs/excali-rust"; base="main"; dir=""; as=""; make_pr=1; draft=0; bundle=""
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) repo="$2"; shift 2 ;;
    --base) base="$2"; shift 2 ;;
    --dir) dir="$2"; shift 2 ;;
    --as) as="$2"; shift 2 ;;
    --no-pr) make_pr=0; shift ;;
    --draft) draft=1; shift ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) if [ -z "$bundle" ]; then bundle="$1"; shift; else echo "unexpected argument: $1" >&2; exit 2; fi ;;
  esac
done
[ -n "$bundle" ] || { echo "usage: $0 <bundle> [options]  (see --help)" >&2; exit 2; }
bundle="$(cd "$(dirname "$bundle")" && pwd)/$(basename "$bundle")"
[ -f "$bundle" ] || { echo "bundle not found: $bundle" >&2; exit 2; }

step() { printf '\n== %s\n' "$*"; }
need() { command -v "$1" >/dev/null 2>&1 || { echo "missing tool: $1" >&2; exit 2; }; }
need git; need gh; need python3

step "gh identity"
gh auth status >/dev/null 2>&1 || { echo "gh is not logged in; run: gh auth login" >&2; exit 2; }
login="$(gh api user -q .login)"
utype="$(gh api user -q .type)"
[ "$utype" = "User" ] || { echo "gh is authenticated as a $utype ($login), not a human user; refusing" >&2; exit 2; }
if [ -n "$as" ] && [ "$as" != "$login" ]; then
  echo "gh is authenticated as $login, expected $as; refusing" >&2; exit 2
fi
echo "   pushing as $login"
gh auth setup-git >/dev/null 2>&1 || true   # let git use gh's token for github.com

step "clone or reuse"
if [ -z "$dir" ]; then
  if git rev-parse --show-toplevel >/dev/null 2>&1 && \
     git remote get-url origin 2>/dev/null | grep -qi "github.com[:/]$repo\(\.git\)\?$"; then
    dir="$(git rev-parse --show-toplevel)"
  else
    dir="./${repo##*/}"
  fi
fi
if [ ! -d "$dir/.git" ]; then
  gh repo clone "$repo" "$dir" -- --quiet
fi
cd "$dir"
git remote get-url origin | grep -qi "github.com[:/]$repo\(\.git\)\?$" || { echo "$dir is not a clone of $repo" >&2; exit 2; }
git fetch --quiet origin "$base"
echo "   $dir"

step "bundle"
git bundle verify "$bundle"
heads="$(git bundle list-heads "$bundle" | awk '{print $2}' | grep '^refs/heads/' || true)"
[ "$(printf '%s\n' "$heads" | grep -c .)" = "1" ] || { echo "bundle must carry exactly one branch, found:" >&2; printf '%s\n' "$heads" >&2; exit 2; }
branch="${heads#refs/heads/}"
git fetch --quiet "$bundle" "refs/heads/$branch:refs/heads/$branch"
git switch --quiet "$branch"
echo "   branch $branch ($(git rev-list "origin/$base..$branch" --count) commit(s) ahead of $base)"

step "authorship gate"
if [ -x scripts/gates/attribution.py ] || [ -f scripts/gates/attribution.py ]; then
  python3 scripts/gates/attribution.py history "origin/$base..$branch"
  python3 scripts/gates/attribution.py files --all
else
  echo "   gate script not in this tree; checking identities only"
fi
bad="$(git log --format='%h %ae %ce' "origin/$base..$branch" | grep -iE 'anthropic|claude' || true)"  # gate:allow-mention
[ -z "$bad" ] || { echo "commits with a tool identity:" >&2; echo "$bad" >&2; exit 1; }

step "push"
git push -u origin "$branch"

if [ "$make_pr" = 1 ]; then
  step "pull request"
  existing="$(gh pr list --repo "$repo" --head "$branch" --state open --json url -q '.[0].url')"
  if [ -n "$existing" ]; then
    echo "   already open: $existing"
  else
    title="$(git log --format=%s -1 "$branch")"
    body="$(mktemp)"
    {
      git log --format='%b' "origin/$base..$branch" | sed '/^$/N;/^\n$/D'
      echo
      echo "## Evidence"
      echo
      echo "Every claim in this change cites a file, URL or command output; see \`site/content/evidence/_index.md\` and the research pages under \`site/content/research/\`."
      echo
      echo "Commits ($(git rev-list "origin/$base..$branch" --count)):"
      git log --format='- %h %s' "origin/$base..$branch"
    } > "$body"
    args=(--repo "$repo" --base "$base" --head "$branch" --title "$title" --body-file "$body")
    [ "$draft" = 1 ] && args+=(--draft)
    gh pr create "${args[@]}"
    rm -f "$body"
  fi
fi
echo
echo "done"
