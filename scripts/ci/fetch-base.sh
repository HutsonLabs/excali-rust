#!/bin/sh
# Spaces CI (ex-e0.1): fetch a pull request's base branch as
# refs/remotes/origin/<base>, as GitHub's checkout with fetch-depth: 0 leaves
# it. Spaces' checkout fetches the commit under test only, and the history
# gate, the tracker-files gate and scripts/gates/affected.py diff against
# origin/<base>. Authenticates with $TOKEN (the job's token) when it is set.
#
#   scripts/ci/fetch-base.sh <base branch>
set -eu
[ $# -eq 1 ] && [ -n "$1" ] || { echo "usage: $0 <base branch>" >&2; exit 2; }
base=$1
if [ -n "${TOKEN:-}" ]; then
  auth="AUTHORIZATION: basic $(printf 'x-access-token:%s' "$TOKEN" | base64 | tr -d '\n')"
  git -c http.extraheader="$auth" fetch -q origin "+refs/heads/$base:refs/remotes/origin/$base"
else
  git fetch -q origin "+refs/heads/$base:refs/remotes/origin/$base"
fi
echo "origin/$base at $(git rev-parse "origin/$base")"
