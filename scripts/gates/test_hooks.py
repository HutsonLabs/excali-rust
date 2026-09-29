#!/usr/bin/env python3
"""Self-test for .githooks/pre-commit: where the beads export is refreshed.

The pre-commit hook re-exports the tracker (`bd export`) and re-renders the
progress page, then stages both, so a commit made in the main clone carries
the task graph as the database holds it. The database lives only in the main
clone. A commit made in a linked worktree (every builder works in one) must
not do this: it would stage a snapshot of the main clone's tracker into an
unrelated branch, which then conflicts with main and reports issues closed
before their pull requests merge.

Each case builds a scratch repository in a temporary directory with a copy of
the real hook and gate, a stub `bd` on PATH that records every call, and a
stub progress renderer, then commits and asserts what the commit contains.

Run: python3 scripts/gates/test_hooks.py -v
"""
from __future__ import annotations

import os
import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
HOOK = REPO / ".githooks" / "pre-commit"

STUB_BD = """#!/bin/sh
# Records its arguments; `export -o FILE` writes a fresh one-line export.
echo "$@" >> "$BD_LOG"
out=""
while [ $# -gt 0 ]; do
  case "$1" in -o) out="$2"; shift 2 ;; *) shift ;; esac
done
[ -n "$out" ] && printf '{"id":"ex-000","status":"closed"}\\n' > "$out"
exit 0
"""

STUB_RENDER = """import pathlib
root = pathlib.Path(__file__).resolve().parents[2]
(root / "site/content/plan").mkdir(parents=True, exist_ok=True)
(root / "site/content/plan/progress.md").write_text("rendered\\n")
"""


def run(args, cwd, env=None, check=True):
    return subprocess.run(args, cwd=cwd, env=env, check=check, capture_output=True, text=True)


class PreCommitExport(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.main = self.tmp / "main"
        self.bin = self.tmp / "bin"
        self.log = self.tmp / "bd.log"
        self.bin.mkdir()
        bd = self.bin / "bd"
        bd.write_text(STUB_BD)
        bd.chmod(bd.stat().st_mode | stat.S_IEXEC)
        self.env = dict(os.environ, PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}", BD_LOG=str(self.log))
        self.env.pop("EXCALI_SKIP_BD_EXPORT", None)

        m = self.main
        (m / ".githooks").mkdir(parents=True)
        shutil.copy(HOOK, m / ".githooks" / "pre-commit")
        (m / "scripts" / "gates").mkdir(parents=True)
        shutil.copy(REPO / "scripts" / "gates" / "attribution.py", m / "scripts" / "gates")
        allow = REPO / "scripts" / "gates" / "attribution-allow.txt"
        if allow.exists():
            shutil.copy(allow, m / "scripts" / "gates")
        (m / "scripts" / "tasks").mkdir(parents=True)
        (m / "scripts" / "tasks" / "render-progress.py").write_text(STUB_RENDER)
        (m / ".beads").mkdir()
        (m / ".beads" / "issues.jsonl").write_text('{"id":"ex-000","status":"open"}\n')
        (m / "site" / "content" / "plan").mkdir(parents=True)
        (m / "site" / "content" / "plan" / "progress.md").write_text("old\n")
        run(["git", "init", "-q", "-b", "main"], m)
        for k, v in (("user.name", "Test Human"), ("user.email", "human@example.invalid"),
                     ("core.hooksPath", ".githooks"), ("commit.gpgsign", "false")):
            run(["git", "config", k, v], m)
        run(["git", "add", "-A"], m)
        run(["git", "commit", "-q", "--no-verify", "-m", "base"], m)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def commit_file(self, where, env=None):
        (where / "work.txt").write_text("change\n")
        run(["git", "add", "work.txt"], where)
        run(["git", "commit", "-q", "-m", "work"], where, env=env or self.env)
        return set(run(["git", "show", "--name-only", "--format=", "HEAD"], where).stdout.split())

    def bd_calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_main_clone_refreshes_and_stages_the_export(self):
        files = self.commit_file(self.main)
        self.assertTrue(any(c.startswith("-C") and "export" in c for c in self.bd_calls()), self.bd_calls())
        self.assertEqual(files, {"work.txt", ".beads/issues.jsonl", "site/content/plan/progress.md"})

    def test_linked_worktree_does_not_touch_the_tracker(self):
        wt = self.tmp / "wt"
        run(["git", "worktree", "add", "-q", "-b", "feature", str(wt)], self.main)
        files = self.commit_file(wt)
        self.assertEqual(self.bd_calls(), [])
        self.assertEqual(files, {"work.txt"})
        self.assertEqual((wt / ".beads" / "issues.jsonl").read_text(), '{"id":"ex-000","status":"open"}\n')

    def test_linked_worktree_keeps_an_explicitly_staged_export(self):
        # The integrator exports the tracker into its worktree on purpose and
        # commits it; the hook must leave that alone.
        wt = self.tmp / "wt"
        run(["git", "worktree", "add", "-q", "-b", "feature", str(wt)], self.main)
        (wt / ".beads" / "issues.jsonl").write_text('{"id":"ex-000","status":"closed"}\n')
        run(["git", "add", ".beads/issues.jsonl"], wt)
        files = self.commit_file(wt)
        self.assertEqual(files, {"work.txt", ".beads/issues.jsonl"})
        self.assertEqual(self.bd_calls(), [])

    def test_skip_variable_still_disables_the_export_in_the_main_clone(self):
        env = dict(self.env, EXCALI_SKIP_BD_EXPORT="1")
        files = self.commit_file(self.main, env=env)
        self.assertEqual(self.bd_calls(), [])
        self.assertEqual(files, {"work.txt"})

    def test_gate_still_runs_in_a_linked_worktree(self):
        wt = self.tmp / "wt"
        run(["git", "worktree", "add", "-q", "-b", "feature", str(wt)], self.main)
        (wt / "bad.txt").write_text("Co-Authored-By: " + "Cl" + "aude <noreply@" + "anth" + "ropic.com>\n")
        run(["git", "add", "bad.txt"], wt)
        r = run(["git", "commit", "-q", "-m", "bad"], wt, env=self.env, check=False)
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)


if __name__ == "__main__":
    unittest.main()
