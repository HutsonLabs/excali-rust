#!/usr/bin/env python3
"""Tracker files stay out of pull requests.

Owner decision 2026-09-29 (task ex-010): a pull request carries code, tests
and docs only. The tracker export, the progress page rendered from it and the
build log change on main alone, in the integrator's commit
`<id>: close in tracker, build log`, pushed from the main clone after the
merge. Every branch touching them conflicted with every other one.

Usage:
  scripts/gates/tracker_files.py check <base-ref>

Fails (exit 1) when `git diff --name-only <base-ref>...HEAD` lists one of
TRACKER_FILES, naming each. The diff starts at the merge base, so the
integrator's tracker commits already on the base never count. gates.yml runs
it on pull_request events only; pushes to main are where those commits land.
Exit status 0 = clean, 1 = tracker files in the diff, 2 = usage or git error.
"""
from __future__ import annotations

import subprocess
import sys

TRACKER_FILES = (
    ".beads/issues.jsonl",
    "site/content/plan/progress.md",
    "site/content/plan/build-log.md",
)

CLOSE_SUBJECT = "close in tracker, build log"


def violations(changed: list[str]) -> list[str]:
    return [p for p in changed if p in TRACKER_FILES]


def changed_files(base: str) -> list[str]:
    out = subprocess.run(
        ["git", "diff", "--name-only", f"{base}...HEAD"], capture_output=True, text=True, check=True
    ).stdout
    return [line for line in out.splitlines() if line]


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[0] != "check":
        print(__doc__, file=sys.stderr)
        return 2
    try:
        changed = changed_files(argv[1])
    except (subprocess.CalledProcessError, OSError) as e:
        print(f"tracker_files.py: {(getattr(e, 'stderr', '') or str(e)).strip()}", file=sys.stderr)
        return 2
    bad = violations(changed)
    if not bad:
        print(f"tracker files: none in the diff against {argv[1]} ({len(changed)} files changed)")
        return 0
    for p in bad:
        print(f"TRACKER {p}: changed in this pull request", file=sys.stderr)
    print(
        "A pull request carries code, tests and docs only. Drop these files from the branch\n"
        f"(git checkout {argv[1]} -- <file>); after the merge the integrator closes the issue\n"
        f"from the main clone and pushes '<id>: {CLOSE_SUBJECT}' to main\n"
        "(site/content/plan/agent-workflow.md, loop step 8).",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
