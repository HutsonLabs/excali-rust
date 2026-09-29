#!/usr/bin/env python3
"""Tests for scripts/gates/tracker_files.py (tracker files stay out of PRs).

Each command-line case builds a scratch git repository in a temporary
directory, commits on a topic branch and runs the gate against main.

Run: python3 scripts/gates/test_tracker_files.py -v
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import tracker_files  # noqa: E402

SCRIPT = HERE / "tracker_files.py"


def git(cwd: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-c", "user.name=T", "-c", "user.email=t@example.invalid", *args],
        cwd=cwd, capture_output=True, text=True, check=True,
    ).stdout


class Violations(unittest.TestCase):
    def test_the_three_files(self):
        self.assertEqual(
            tracker_files.violations(
                [
                    "crates/excali-core/src/lib.rs",
                    ".beads/issues.jsonl",
                    "site/content/plan/progress.md",
                    "site/content/plan/build-log.md",
                    "plan/tasks.json",
                    "site/content/plan/agent-workflow.md",
                ]
            ),
            [".beads/issues.jsonl", "site/content/plan/progress.md", "site/content/plan/build-log.md"],
        )

    def test_task_graph_and_other_pages_are_allowed(self):
        self.assertEqual(
            tracker_files.violations(["plan/tasks.json", "site/content/plan/phases.md", ".beads/config.yaml"]), []
        )


class CommandLine(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name).resolve()
        git(self.root, "init", "-q", "-b", "main")
        for p in tracker_files.TRACKER_FILES + ("src.rs",):
            self.write(p, "base\n")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "-m", "base")
        git(self.root, "switch", "-q", "-c", "topic")

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, path: str, text: str):
        p = self.root / path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)

    def commit(self, path: str, subject: str | None = None):
        self.write(path, f"{path} changed\n")
        git(self.root, "add", path)
        git(self.root, "commit", "-q", "-m", subject or f"ex-000: {path}")

    def check(self, base: str = "main") -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "check", base], cwd=self.root, capture_output=True, text=True
        )

    def test_code_only_passes(self):
        self.commit("src.rs")
        self.commit("plan/tasks.json")
        r = self.check()
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_each_tracker_file_fails(self):
        for path in tracker_files.TRACKER_FILES:
            with self.subTest(path=path):
                git(self.root, "switch", "-q", "-C", "topic", "main")
                self.commit(path)
                r = self.check()
                self.assertEqual(r.returncode, 1)
                self.assertIn(f"TRACKER {path}", r.stderr)

    def test_a_close_commit_on_a_branch_still_fails(self):
        # The close commit belongs on main, pushed by the integrator; inside a
        # pull request it is the conflict the rule exists to prevent.
        self.commit(".beads/issues.jsonl", "ex-000: close in tracker, build log")
        self.assertEqual(self.check().returncode, 1)

    def test_close_commits_already_on_the_base_do_not_count(self):
        self.commit("src.rs")
        git(self.root, "switch", "-q", "main")
        self.commit(".beads/issues.jsonl", "ex-999: close in tracker, build log")
        self.commit("site/content/plan/build-log.md", "ex-999: close in tracker, build log")
        git(self.root, "switch", "-q", "topic")
        r = self.check()
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_usage_errors(self):
        self.assertEqual(subprocess.run([sys.executable, str(SCRIPT)], cwd=self.root, capture_output=True).returncode, 2)
        self.assertEqual(self.check("no-such-ref").returncode, 2)


if __name__ == "__main__":
    unittest.main()
