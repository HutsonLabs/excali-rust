#!/usr/bin/env python3
"""Tests for scripts/gates/version.py (calendar versioning, ADR-009).

Run: python3 scripts/gates/test_version.py
"""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import version  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]

ROOT_MANIFEST = """
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "{v}"
edition = "2021"

[workspace.dependencies]
serde = "1.0.229"
"""

MEMBER = """[package]
name = "{name}"
{version_line}
edition.workspace = true
publish = false
"""

LOCK = """version = 4

[[package]]
name = "excali-core"
version = "{core}"

[[package]]
name = "excali-math"
version = "{math}"

[[package]]
name = "serde"
version = "1.0.229"
"""


class Parse(unittest.TestCase):
    def test_first_release(self):
        self.assertEqual(version.parse("26.9.1"), (26, 9, 1))

    def test_two_digit_month(self):
        self.assertEqual(version.parse("27.12.14"), (27, 12, 14))

    def test_rejects_non_calver(self):
        for bad in (
            "0.1.0",      # semver-looking, year 0
            "1.0.0",
            "2026.9.1",   # four-digit year
            "6.9.1",      # one-digit year
            "26.09.1",    # zero-padded month
            "26.0.1",     # month 0
            "26.13.1",    # month 13
            "26.9.0",     # builds start at 1
            "26.9.01",    # zero-padded build
            "26.9",       # missing build
            "26.9.1.1",
            "26.9.1-rc1",  # pre-release suffixes are not part of the scheme
            "v26.9.1",    # the v belongs to the tag, not the version
            "",
        ):
            with self.subTest(v=bad):
                with self.assertRaises(ValueError):
                    version.parse(bad)


class Tag(unittest.TestCase):
    def test_tag_is_v_prefixed(self):
        self.assertEqual(version.tag("26.9.1"), "v26.9.1")

    def test_tag_rejects_invalid(self):
        with self.assertRaises(ValueError):
            version.tag("0.1.0")


class Next(unittest.TestCase):
    def test_same_month_bumps_build(self):
        self.assertEqual(version.next_version("26.9.1", 2026, 9), "26.9.2")
        self.assertEqual(version.next_version("26.9.9", 2026, 9), "26.9.10")

    def test_new_month_resets_build(self):
        self.assertEqual(version.next_version("26.9.4", 2026, 10), "26.10.1")

    def test_new_year_resets_build(self):
        self.assertEqual(version.next_version("26.12.3", 2027, 1), "27.1.1")

    def test_no_previous_release(self):
        self.assertEqual(version.next_version(None, 2026, 9), "26.9.1")

    def test_clock_before_last_release_is_an_error(self):
        with self.assertRaises(ValueError):
            version.next_version("26.9.1", 2026, 8)


class Check(unittest.TestCase):
    def files(self, v="26.9.1", core="26.9.1", math="26.9.1", math_line="version.workspace = true"):
        return {
            "root": ROOT_MANIFEST.format(v=v),
            "members": {
                "excali-core": MEMBER.format(name="excali-core", version_line="version.workspace = true"),
                "excali-math": MEMBER.format(name="excali-math", version_line=math_line),
            },
            "lock": LOCK.format(core=core, math=math),
        }

    def run_check(self, **kw):
        f = self.files(**kw)
        return version.check(f["root"], f["members"], f["lock"])

    def test_clean(self):
        self.assertEqual(self.run_check(), [])

    def test_workspace_version_must_be_calver(self):
        problems = self.run_check(v="0.1.0", core="0.1.0", math="0.1.0")
        self.assertTrue(any("0.1.0" in p and "YY.M.BUILD" in p for p in problems), problems)

    def test_member_must_inherit_the_workspace_version(self):
        problems = self.run_check(math_line='version = "26.9.1"')
        self.assertTrue(any("excali-math" in p and "version.workspace" in p for p in problems), problems)

    def test_registry_hold_requires_publish_false(self):
        # ADR-009 item 4: nothing is published to a registry while ex-801 is
        # held, so every crate must say publish = false.
        f = self.files()
        f["members"]["excali-core"] = f["members"]["excali-core"].replace("publish = false\n", "")
        problems = version.check(f["root"], f["members"], f["lock"])
        self.assertTrue(any("excali-core" in p and "publish = false" in p for p in problems), problems)

    def test_lock_must_agree(self):
        problems = self.run_check(math="0.1.0")
        self.assertTrue(any("excali-math" in p and "Cargo.lock" in p for p in problems), problems)

    def test_missing_workspace_version(self):
        problems = version.check("[workspace]\nmembers = []\n", {}, "")
        self.assertTrue(any("[workspace.package]" in p for p in problems), problems)


class Repository(unittest.TestCase):
    """The real workspace is on the first calendar release (ADR-009)."""

    def test_workspace_version_is_the_first_release(self):
        self.assertEqual(version.workspace_version((ROOT / "Cargo.toml").read_text()), "26.9.1")

    def test_repository_passes_the_gate(self):
        self.assertEqual(version.main(["check"]), 0)

    def test_adr_009_records_the_scheme(self):
        adr = (ROOT / "site" / "content" / "decisions" / "adr-009-calendar-versioning.md").read_text()
        for needle in ("YY.M.BUILD", "26.9.1", "v26.9.1", "crates.io", "GitHub release"):
            self.assertIn(needle, adr)


if __name__ == "__main__":
    unittest.main()
