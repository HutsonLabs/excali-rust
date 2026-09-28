#!/usr/bin/env python3
"""Tests for scripts/upstream/checkout.sh (task ex-002).

Every test runs against a throwaway local "upstream" repository served over
file://, so the suite needs no network. UPSTREAM_URL and UPSTREAM_DIR point
the script at the fixture; PIN is only set where a test exercises the
explicit override path, because the default pin must come from
site/config.toml (extra.upstream_commit).

Run: python3 scripts/upstream/test_checkout.py
"""
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "upstream" / "checkout.sh"
CONFIG = ROOT / "site" / "config.toml"


def config_pin() -> str:
    m = re.search(r'^upstream_commit\s*=\s*"([0-9a-f]{40})"', CONFIG.read_text(), re.M)
    assert m, "site/config.toml has no extra.upstream_commit"
    return m.group(1)


def git(cwd, *args, env=None) -> str:
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True, env=env
    ).stdout.strip()


GIT_ENV = {
    **os.environ,
    "GIT_AUTHOR_NAME": "fixture",
    "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
    "GIT_COMMITTER_NAME": "fixture",
    "GIT_COMMITTER_EMAIL": "fixture@example.invalid",
    "GIT_CONFIG_NOSYSTEM": "1",
}
for _k in ("PIN", "UPSTREAM_DIR", "UPSTREAM_URL"):
    GIT_ENV.pop(_k, None)

# Force the old wire protocol, where upload-pack enforces
# uploadpack.allow*SHA1InWant, to simulate a server that refuses
# fetch-by-commit-id.
PROTOCOL_V0 = {
    "GIT_CONFIG_COUNT": "1",
    "GIT_CONFIG_KEY_0": "protocol.version",
    "GIT_CONFIG_VALUE_0": "0",
}


class CheckoutTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="ex-upstream-test-"))
        self.remote = self.tmp / "remote"
        self.remote.mkdir()
        git(self.remote, "init", "-q", "-b", "master", env=GIT_ENV)
        # Serve arbitrary SHAs the way GitHub does (fetch by commit id).
        git(self.remote, "config", "uploadpack.allowAnySHA1InWant", "true")
        self.commits = []
        for i in range(3):
            (self.remote / "file.txt").write_text(f"rev {i}\n")
            git(self.remote, "add", "file.txt", env=GIT_ENV)
            git(self.remote, "commit", "-q", "-m", f"rev {i}", env=GIT_ENV)
            self.commits.append(git(self.remote, "rev-parse", "HEAD"))
        self.dest = self.tmp / "tools" / "upstream"
        self.url = self.remote.as_uri()

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_script(self, pin=None, url=None, extra_env=None, args=()):
        env = {
            **GIT_ENV,
            "UPSTREAM_URL": url or self.url,
            "UPSTREAM_DIR": str(self.dest),
        }
        if pin is not None:
            env["PIN"] = pin
        if extra_env:
            env.update(extra_env)
        return subprocess.run(
            ["bash", str(SCRIPT), *args], cwd=ROOT, env=env, capture_output=True, text=True
        )

    def head(self):
        return git(self.dest, "rev-parse", "HEAD")

    # --- pin source ------------------------------------------------------

    def test_print_pin_reads_site_config(self):
        r = self.run_script(args=("--print-pin",))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout.strip(), config_pin())

    def test_print_pin_honours_override(self):
        r = self.run_script(pin=self.commits[0], args=("--print-pin",))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout.strip(), self.commits[0])

    def test_default_pin_is_the_config_commit(self):
        # The fixture does not contain the real pinned commit, so the default
        # run must try exactly that commit and fail to find it -- proving no
        # other commit (e.g. the remote's HEAD) is ever checked out silently.
        r = self.run_script()
        self.assertNotEqual(r.returncode, 0)
        self.assertIn(config_pin(), r.stderr)
        self.assertNotIn("PIN override", r.stderr)
        if (self.dest / ".git").exists():
            probe = subprocess.run(
                ["git", "rev-parse", "-q", "--verify", "HEAD"],
                cwd=self.dest, capture_output=True, text=True,
            )
            self.assertNotIn(probe.stdout.strip(), self.commits)

    # --- fresh clone -----------------------------------------------------

    def test_fresh_checkout_lands_on_pin(self):
        pin = self.commits[1]  # not the remote HEAD
        r = self.run_script(pin=pin)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.head(), pin)
        self.assertEqual((self.dest / "file.txt").read_text(), "rev 1\n")
        self.assertEqual(git(self.dest, "remote", "get-url", "origin"), self.url)
        detached = subprocess.run(
            ["git", "symbolic-ref", "-q", "HEAD"], cwd=self.dest, capture_output=True
        )
        self.assertNotEqual(detached.returncode, 0, "HEAD should be detached at the pin")
        self.assertIn(pin, r.stdout)

    def test_override_is_announced(self):
        r = self.run_script(pin=self.commits[0])
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("PIN override", r.stderr)
        self.assertIn(config_pin(), r.stderr)

    def test_override_equal_to_config_is_silent(self):
        r = self.run_script(pin=config_pin(), args=("--print-pin",))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("PIN override", r.stderr)

    def test_verify_mode(self):
        pin = self.commits[2]
        r = self.run_script(pin=pin, args=("--verify",))
        self.assertNotEqual(r.returncode, 0, "verify before any checkout must fail")
        self.assertFalse(self.dest.exists(), "verify must not create the checkout")
        self.assertEqual(self.run_script(pin=pin).returncode, 0)
        r = self.run_script(pin=pin, args=("--verify",))
        self.assertEqual(r.returncode, 0, r.stderr)
        r = self.run_script(pin=self.commits[0], args=("--verify",))
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("HEAD", r.stderr)
        self.assertEqual(self.head(), pin, "verify must not move HEAD")

    def test_verify_rejects_dirty_tree(self):
        pin = self.commits[2]
        self.assertEqual(self.run_script(pin=pin).returncode, 0)
        (self.dest / "file.txt").write_text("edited\n")
        r = self.run_script(pin=pin, args=("--verify",))
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("modified", r.stderr)

    # --- idempotence -----------------------------------------------------

    def test_second_run_is_a_noop_without_network(self):
        pin = self.commits[1]
        self.assertEqual(self.run_script(pin=pin).returncode, 0)
        marker = self.dest / ".git" / "marker"
        marker.write_text("x")
        # Make the remote unreachable: a second run must not need it.
        shutil.move(str(self.remote), str(self.tmp / "moved"))
        r = self.run_script(pin=pin)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("already at", r.stdout)
        self.assertEqual(self.head(), pin)
        self.assertTrue(marker.exists(), "checkout must not be re-cloned")

    def test_existing_checkout_moves_to_new_pin(self):
        self.assertEqual(self.run_script(pin=self.commits[0]).returncode, 0)
        r = self.run_script(pin=self.commits[2])
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.head(), self.commits[2])
        r = self.run_script(pin=self.commits[1])
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.head(), self.commits[1])

    def test_pin_commit_added_after_first_clone_is_fetched(self):
        self.assertEqual(self.run_script(pin=self.commits[0]).returncode, 0)
        (self.remote / "file.txt").write_text("rev 3\n")
        git(self.remote, "commit", "-q", "-am", "rev 3", env=GIT_ENV)
        new = git(self.remote, "rev-parse", "HEAD")
        r = self.run_script(pin=new)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.head(), new)

    def test_works_without_sha_fetch_support(self):
        # A server that refuses fetch-by-commit-id of a non-tip commit: the
        # script must fall back to fetching the refs and still land on it.
        git(self.remote, "config", "uploadpack.allowAnySHA1InWant", "false")
        pin = self.commits[0]
        r = self.run_script(pin=pin, extra_env=PROTOCOL_V0)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("fetching all refs", r.stdout, "fallback path was not exercised")
        self.assertEqual(self.head(), pin)

    # --- refusals --------------------------------------------------------

    def test_refuses_non_sha_pin(self):
        for bad in ("master", "HEAD", "438d898", "g" * 40, "A" * 40, ""):
            with self.subTest(pin=bad):
                r = self.run_script(pin=bad)
                self.assertNotEqual(r.returncode, 0)
                self.assertIn("40-character", r.stderr)
                self.assertFalse(self.dest.exists())

    def test_refuses_unknown_commit(self):
        r = self.run_script(pin="0" * 40)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("0" * 40, r.stderr)
        self.assertFalse(
            (self.dest / "file.txt").exists(), "no commit may be checked out on failure"
        )

    def test_refuses_dirty_checkout(self):
        pin = self.commits[1]
        self.assertEqual(self.run_script(pin=pin).returncode, 0)
        (self.dest / "file.txt").write_text("local edit\n")
        r = self.run_script(pin=self.commits[2])
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("modified", r.stderr)
        self.assertEqual((self.dest / "file.txt").read_text(), "local edit\n")
        self.assertEqual(self.head(), pin)

    def test_refuses_dirty_checkout_even_at_pin(self):
        pin = self.commits[1]
        self.assertEqual(self.run_script(pin=pin).returncode, 0)
        (self.dest / "new.txt").write_text("untracked\n")
        r = self.run_script(pin=pin)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("modified", r.stderr)

    def test_refuses_non_git_directory(self):
        self.dest.mkdir(parents=True)
        (self.dest / "keep.txt").write_text("mine\n")
        r = self.run_script(pin=self.commits[0])
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("not a git checkout", r.stderr)
        self.assertEqual((self.dest / "keep.txt").read_text(), "mine\n")

    def test_refuses_foreign_origin(self):
        self.assertEqual(self.run_script(pin=self.commits[0]).returncode, 0)
        other = (self.tmp / "other").as_uri()
        r = self.run_script(pin=self.commits[0], url=other)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("origin", r.stderr)

    def test_empty_existing_directory_is_cloned_into(self):
        self.dest.mkdir(parents=True)
        r = self.run_script(pin=self.commits[2])
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.head(), self.commits[2])

    def test_unknown_argument(self):
        r = self.run_script(pin=self.commits[0], args=("--bogus",))
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("usage", r.stderr.lower())


class DefaultLocationTests(unittest.TestCase):
    def test_default_dir_is_main_clone_tools(self):
        # From any worktree the default lands in the main clone's .tools so
        # every worktree shares one checkout.
        env = {k: v for k, v in os.environ.items() if k not in ("UPSTREAM_DIR", "PIN")}
        r = subprocess.run(
            ["bash", str(SCRIPT), "--print-dir"], cwd=ROOT, env=env,
            capture_output=True, text=True,
        )
        self.assertEqual(r.returncode, 0, r.stderr)
        common = Path(git(ROOT, "rev-parse", "--path-format=absolute", "--git-common-dir"))
        self.assertEqual(Path(r.stdout.strip()), common.parent / ".tools" / "upstream")


if __name__ == "__main__":
    unittest.main(verbosity=2)
