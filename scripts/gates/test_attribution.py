#!/usr/bin/env python3
"""Self-test for scripts/gates/attribution.py (ex-g001).

Milestone M0 requires that the gate rejects a planted attribution line in CI.
Each case below builds a scratch git repository in a temporary directory
(never inside this repository), plants exactly one violation, runs the real
gate script against it and asserts a non-zero exit naming the rule id. A clean
control repository must pass with exit 0, so a gate that rejects everything
cannot satisfy the suite either.

Commits are made with `git commit-tree` and `git update-ref`, so no hook is
involved and nothing planted can reach this repository's index or history.

The vendor/tool names and invisible code points are assembled at runtime so
this source file stays clean under the gate it tests.

Run: python3 scripts/gates/test_attribution.py -v
"""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
GATE = HERE / "attribution.py"
REPO = HERE.parents[1]
GATES_WORKFLOW = REPO / ".github" / "workflows" / "gates.yml"

# Assembled so the literal names never appear in this file.
TOOL = "Cl" + "aude"
VENDOR = "Anth" + "ropic"
VENDOR_NOREPLY = "noreply@" + VENDOR.lower() + ".com"
ZERO_WIDTH_SPACE = chr(0x200B)

HUMAN_NAME = "Test Human"
HUMAN_EMAIL = "human@example.invalid"


def clean_env(**extra: str) -> dict[str, str]:
    """Environment for scratch git calls.

    Drops every GIT_* variable (a hook runs with GIT_DIR / GIT_INDEX_FILE set,
    which would point git back at this repository) and ignores the user's
    global and system config (hooksPath, signing, templates).
    """
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(
        GIT_CONFIG_NOSYSTEM="1",
        GIT_CONFIG_GLOBAL=os.devnull,
        GIT_AUTHOR_NAME=HUMAN_NAME,
        GIT_AUTHOR_EMAIL=HUMAN_EMAIL,
        GIT_COMMITTER_NAME=HUMAN_NAME,
        GIT_COMMITTER_EMAIL=HUMAN_EMAIL,
    )
    env.update(extra)
    return env


class Scratch:
    """A throwaway git repository with a root commit and helpers to plant."""

    def __init__(self, tmp: Path):
        self.dir = tmp / "scratch"
        self.dir.mkdir()
        self.git("init", "-q", "-b", "main")
        self.write("README.md", "A clean scratch repository.\n")
        self.base = self.commit("base: clean root commit")

    def git(self, *args: str, env: dict[str, str] | None = None, stdin: str | None = None) -> str:
        return subprocess.run(
            ["git", *args],
            cwd=self.dir,
            env=env or clean_env(),
            input=stdin,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()

    def write(self, rel: str, text: str) -> None:
        path = self.dir / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        self.git("add", "--", rel)

    def commit(self, message: str, **env_overrides: str) -> str:
        """commit-tree + update-ref: a real commit, no hooks run."""
        env = clean_env(**env_overrides)
        tree = self.git("write-tree", env=env)
        parents: list[str] = []
        head = subprocess.run(
            ["git", "rev-parse", "-q", "--verify", "HEAD"],
            cwd=self.dir, env=env, capture_output=True, text=True,
        ).stdout.strip()
        if head:
            parents = ["-p", head]
        sha = self.git("commit-tree", tree, *parents, "-F", "-", env=env, stdin=message)
        self.git("update-ref", "HEAD", sha, env=env)
        return sha

    def gate(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(GATE), *args],
            cwd=self.dir,
            env=clean_env(),
            capture_output=True,
            text=True,
        )


class PlantedViolations(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory(prefix="attribution-selftest-")
        self.addCleanup(self._tmp.cleanup)
        self.repo = Scratch(Path(self._tmp.name).resolve())

    def assertRejected(self, result: subprocess.CompletedProcess[str], *rules: str) -> None:
        self.assertEqual(result.returncode, 1, f"gate should reject; stdout={result.stdout!r} stderr={result.stderr!r}")
        self.assertIn("attribution gate FAILED", result.stderr)
        for rule in rules:
            self.assertRegex(result.stderr, rf"(?m)^\s+\S.*: {rule} ", f"rule {rule} not reported:\n{result.stderr}")

    def assertClean(self, result: subprocess.CompletedProcess[str]) -> None:
        self.assertEqual(result.returncode, 0, f"gate should pass; stdout={result.stdout!r} stderr={result.stderr!r}")
        self.assertIn("attribution gate OK", result.stdout)

    # -- isolation --------------------------------------------------------

    def test_scratch_repo_is_outside_this_repository(self) -> None:
        top = Path(self.repo.git("rev-parse", "--show-toplevel")).resolve()
        self.assertEqual(top, self.repo.dir.resolve())
        self.assertNotEqual(top, REPO.resolve())
        self.assertFalse(top.is_relative_to(REPO.resolve()), top)

    # -- control ----------------------------------------------------------

    def test_clean_control_passes_files_and_history(self) -> None:
        self.repo.write("src/lib.rs", "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n")
        self.repo.commit("feat: add\n\nA plain, human-authored message.\n")
        self.assertClean(self.repo.gate("files", "--all"))
        self.assertClean(self.repo.gate("history", f"{self.repo.base}..HEAD"))

    # -- planted cases ----------------------------------------------------

    def test_tool_attribution_line_in_tracked_file_fails_files(self) -> None:
        self.repo.write("NOTES.md", f"Some notes.\n\nGenerated with [{TOOL} Code](https://example.invalid)\n")
        result = self.repo.gate("files", "--all")
        self.assertRejected(result, "R2", "R4")
        self.assertIn("NOTES.md:3:", result.stderr)

    def test_co_authored_by_trailer_in_commit_message_fails_history(self) -> None:
        self.repo.write("src/lib.rs", "pub fn one() -> i32 {\n    1\n}\n")
        self.repo.commit(f"feat: one\n\nCo-Authored-By: {TOOL} <{VENDOR_NOREPLY}>\n")
        # the tree itself is clean; only the message carries the trailer
        self.assertClean(self.repo.gate("files", "--all"))
        self.assertRejected(self.repo.gate("history", f"{self.repo.base}..HEAD"), "R1", "R3", "R4")

    def test_zero_width_character_in_tracked_file_fails_files(self) -> None:
        self.repo.write("doc.md", f"An innocent{ZERO_WIDTH_SPACE} sentence.\n")
        result = self.repo.gate("files", "--all")
        self.assertRejected(result, "R5")
        self.assertIn("U+200B", result.stderr)

    def test_zero_width_character_in_commit_message_fails_history(self) -> None:
        self.repo.write("a.txt", "a\n")
        self.repo.commit(f"chore: a{ZERO_WIDTH_SPACE}\n")
        self.assertRejected(self.repo.gate("history", f"{self.repo.base}..HEAD"), "R5")

    def test_non_human_author_fails_history(self) -> None:
        self.repo.write("b.txt", "b\n")
        self.repo.commit(
            "chore: b\n",
            GIT_AUTHOR_NAME=TOOL,
            GIT_AUTHOR_EMAIL=VENDOR_NOREPLY,
        )
        result = self.repo.gate("history", f"{self.repo.base}..HEAD")
        self.assertRejected(result, "R3", "R4")
        self.assertIn("author name:", result.stderr)
        self.assertIn("author email:", result.stderr)

    def test_non_human_committer_fails_history(self) -> None:
        self.repo.write("c.txt", "c\n")
        self.repo.commit(
            "chore: c\n",
            GIT_COMMITTER_NAME=TOOL,
            GIT_COMMITTER_EMAIL=VENDOR_NOREPLY,
        )
        result = self.repo.gate("history", f"{self.repo.base}..HEAD")
        self.assertRejected(result, "R3", "R4")
        self.assertIn("committer email:", result.stderr)

    def test_violation_below_a_clean_commit_is_still_found(self) -> None:
        # history scans the whole range, not just the tip
        self.repo.write("d.txt", "d\n")
        self.repo.commit(f"chore: d\n\nCo-Authored-By: {TOOL} <{VENDOR_NOREPLY}>\n")
        self.repo.write("e.txt", "e\n")
        self.repo.commit("chore: e\n")
        self.assertRejected(self.repo.gate("history", f"{self.repo.base}..HEAD"), "R1")


class Wiring(unittest.TestCase):
    """The gates workflow runs this self-test on every PR and on main."""

    def test_gates_workflow_runs_the_self_test(self) -> None:
        text = GATES_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("pull_request:", text)
        self.assertRegex(text, r"push:\s*\n\s*branches:\s*\[main\]")
        self.assertIn("python3 scripts/gates/test_attribution.py", text)

    def test_this_file_is_clean_under_the_gate(self) -> None:
        result = subprocess.run(
            [sys.executable, str(GATE), "files", "scripts/gates/test_attribution.py"],
            cwd=REPO, env=clean_env(), capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
