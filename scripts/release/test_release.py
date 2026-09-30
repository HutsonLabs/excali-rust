#!/usr/bin/env python3
"""Tests for the web runtime release asset (ex-802).

scripts/release/package.py packs a web build (scripts/web/build.sh's OUT)
into excali-web_<version>.tar.gz with its line in SHA256SUMS, the two assets
a GitHub release carries (ADR-009); scripts/release/fetch.sh is the other
end, what a host's vendor script (term.hut's scripts/vendor-excali.sh, in
the style of its scripts/vendor-catppuccin-icons.sh) runs to download a
release, check the digest and unpack it. The tests build a stand-in dist/,
pack it, and fetch it back through a file:// release URL, offline.

Run: python3 scripts/release/test_release.py -v
"""
from __future__ import annotations

import gzip
import hashlib
import io
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / "scripts" / "release" / "package.py"
FETCH = ROOT / "scripts" / "release" / "fetch.sh"
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
NOTES = ROOT / "scripts" / "release" / "notes"

sys.path.insert(0, str(ROOT / "scripts" / "gates"))
import version as calver  # noqa: E402

WORKSPACE_VERSION = calver.workspace_version((ROOT / "Cargo.toml").read_text(encoding="utf-8"))

DIST = {
    "excali_editor.js": b"export default function init() {}\n",
    "excali_editor_bg.wasm": b"\0asm\1\0\0\0",
    "excali.css": b"excali-editor { display: block; }\n",
    "fonts/manifest.json": b'{"families": []}\n',
    "fonts/Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2": b"wOF2-excalifont",
    "fonts/Excalifont/LICENSE.txt": b"OFL-1.1\n",
    "fonts/Xiaolai/Xiaolai-Regular-09850c4077f3fffe707905872e0e2460.woff2": b"wOF2-xiaolai",
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_tree(root: Path, files: dict[str, bytes]) -> None:
    for rel, data in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)


def read_tree(root: Path) -> dict[str, bytes]:
    return {
        p.relative_to(root).as_posix(): p.read_bytes() for p in sorted(root.rglob("*")) if p.is_file()
    }


class ReleaseTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="excali-release-test."))
        self.dist = self.tmp / "dist"
        write_tree(self.dist, DIST)

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def pack(self, *args: str, **env: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(PACKAGE), *args],
            capture_output=True,
            text=True,
            env={**os.environ, **env},
        )

    def packed(self, out: Path, version: str = "26.9.1", **env: str) -> Path:
        r = self.pack("pack", str(self.dist), str(out), "--version", version, **env)
        self.assertEqual(r.returncode, 0, r.stderr)
        return out / f"excali-web_{version}.tar.gz"

    def fetch(self, version: str, out: Path, base: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["sh", str(FETCH), version, str(out)],
            capture_output=True,
            text=True,
            env={**os.environ, "EXCALI_RELEASE_URL": base.as_uri()},
        )

    # package.py

    def test_tarball_holds_the_four_entries_at_the_top_level(self):
        tarball = self.packed(self.tmp / "release")
        with tarfile.open(tarball, "r:gz") as tar:
            names = tar.getnames()
            top = sorted({n.split("/")[0] for n in names})
            self.assertEqual(top, ["excali.css", "excali_editor.js", "excali_editor_bg.wasm", "fonts"])
            files = {m.name: tar.extractfile(m).read() for m in tar.getmembers() if m.isfile()}
        self.assertEqual(files, DIST)
        self.assertEqual(names, sorted(names), "entries in byte order")

    def test_sha256sums_names_the_tarball_in_shasum_format(self):
        tarball = self.packed(self.tmp / "release")
        sums = (self.tmp / "release" / "SHA256SUMS").read_text()
        self.assertEqual(sums, f"{sha256(tarball)}  excali-web_26.9.1.tar.gz\n")
        r = self.pack("verify", str(self.tmp / "release"))
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_verify_fails_on_a_changed_asset(self):
        tarball = self.packed(self.tmp / "release")
        tarball.write_bytes(tarball.read_bytes() + b"\0")
        r = self.pack("verify", str(self.tmp / "release"))
        self.assertEqual(r.returncode, 1)
        self.assertIn("excali-web_26.9.1.tar.gz", r.stderr)

    def test_verify_names_checks_only_those_lines(self):
        out = self.tmp / "release"
        out.mkdir()
        (out / "SHA256SUMS").write_text("0" * 64 + "  excali-example_26.9.1_aarch64.dmg\n")
        self.packed(out)
        self.assertEqual(self.pack("verify", str(out)).returncode, 1, "the dmg is not here")
        r = self.pack("verify", str(out), "excali-web_26.9.1.tar.gz")
        self.assertEqual(r.returncode, 0, r.stderr)
        r = self.pack("verify", str(out), "excali-web_26.10.1.tar.gz")
        self.assertEqual(r.returncode, 1)
        self.assertIn("excali-web_26.10.1.tar.gz", r.stderr)

    def test_existing_sha256sums_lines_for_other_assets_are_kept(self):
        out = self.tmp / "release"
        out.mkdir()
        dmg = out / "excali-example_26.9.1_aarch64.dmg"
        dmg.write_bytes(b"dmg")
        (out / "SHA256SUMS").write_text(
            f"{sha256(dmg)}  {dmg.name}\n" + "0" * 64 + "  excali-web_26.9.1.tar.gz\n"
        )
        tarball = self.packed(out)
        self.assertEqual(
            (out / "SHA256SUMS").read_text(),
            f"{sha256(dmg)}  {dmg.name}\n{sha256(tarball)}  excali-web_26.9.1.tar.gz\n",
        )

    def test_same_build_packs_to_the_same_bytes(self):
        a = self.packed(self.tmp / "a", SOURCE_DATE_EPOCH="1790000000")
        os.utime(self.dist / "excali.css", (1, 1))
        (self.dist / "excali.css").chmod(0o600)
        b = self.packed(self.tmp / "b", SOURCE_DATE_EPOCH="1790000000")
        self.assertEqual(a.read_bytes(), b.read_bytes())

    def test_members_are_normalised(self):
        tarball = self.packed(self.tmp / "release", SOURCE_DATE_EPOCH="1790000000")
        raw = gzip.GzipFile(fileobj=io.BytesIO(tarball.read_bytes()))
        raw.read()
        self.assertEqual(raw.mtime, 0, "gzip header carries no timestamp")
        with tarfile.open(tarball, "r:gz") as tar:
            for m in tar.getmembers():
                self.assertEqual((m.uid, m.gid, m.uname, m.gname), (0, 0, "", ""), m.name)
                self.assertEqual(m.mtime, 1790000000, m.name)
                self.assertEqual(m.mode, 0o755 if m.isdir() else 0o644, m.name)
                self.assertTrue(m.isfile() or m.isdir(), m.name)

    def test_version_defaults_to_the_workspace_version(self):
        r = self.pack("pack", str(self.dist), str(self.tmp / "release"))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue((self.tmp / "release" / f"excali-web_{WORKSPACE_VERSION}.tar.gz").is_file())

    def test_refuses_a_version_that_is_not_calendar(self):
        r = self.pack("pack", str(self.dist), str(self.tmp / "release"), "--version", "1.0.0")
        self.assertEqual(r.returncode, 2)
        self.assertIn("calendar", r.stderr)

    def test_refuses_a_dist_missing_an_entry(self):
        (self.dist / "excali_editor_bg.wasm").unlink()
        r = self.pack("pack", str(self.dist), str(self.tmp / "release"), "--version", "26.9.1")
        self.assertEqual(r.returncode, 1)
        self.assertIn("excali_editor_bg.wasm", r.stderr)
        self.assertFalse((self.tmp / "release").exists())

    def test_refuses_a_dist_with_extra_entries(self):
        (self.dist / "index.html").write_text("<p>")
        r = self.pack("pack", str(self.dist), str(self.tmp / "release"), "--version", "26.9.1")
        self.assertEqual(r.returncode, 1)
        self.assertIn("index.html", r.stderr)

    def test_refuses_fonts_without_manifest(self):
        (self.dist / "fonts" / "manifest.json").unlink()
        r = self.pack("pack", str(self.dist), str(self.tmp / "release"), "--version", "26.9.1")
        self.assertEqual(r.returncode, 1)
        self.assertIn("fonts/manifest.json", r.stderr)

    # fetch.sh

    def publish(self, version: str = "26.9.1") -> Path:
        """A release directory laid out as github.com/<repo>/releases/download is."""
        base = self.tmp / "download"
        self.packed(base / f"v{version}", version)
        return base

    def test_fetch_downloads_verifies_and_unpacks(self):
        base = self.publish()
        out = self.tmp / "vendor" / "excali"
        r = self.fetch("26.9.1", out, base)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(read_tree(out), DIST)
        self.assertIn("26.9.1", r.stdout)

    def test_fetch_replaces_an_existing_directory(self):
        base = self.publish()
        out = self.tmp / "excali"
        write_tree(out, {"stale.js": b"old"})
        r = self.fetch("26.9.1", out, base)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(read_tree(out), DIST)

    def test_fetch_refuses_a_tarball_whose_digest_differs(self):
        base = self.publish()
        tarball = base / "v26.9.1" / "excali-web_26.9.1.tar.gz"
        tarball.write_bytes(tarball.read_bytes() + b"\0")
        out = self.tmp / "excali"
        write_tree(out, {"kept.js": b"kept"})
        r = self.fetch("26.9.1", out, base)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("SHA-256", r.stderr)
        self.assertEqual(read_tree(out), {"kept.js": b"kept"}, "a failed fetch leaves OUT alone")

    def test_fetch_refuses_a_release_without_the_tarball_in_sha256sums(self):
        base = self.publish()
        (base / "v26.9.1" / "SHA256SUMS").write_text("0" * 64 + "  other.tar.gz\n")
        r = self.fetch("26.9.1", self.tmp / "excali", base)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("SHA256SUMS", r.stderr)
        self.assertFalse((self.tmp / "excali").exists())

    def test_fetch_fails_on_a_missing_release(self):
        base = self.publish()
        r = self.fetch("26.10.1", self.tmp / "excali", base)
        self.assertNotEqual(r.returncode, 0)
        self.assertFalse((self.tmp / "excali").exists())

    def test_fetch_defaults_to_the_github_release_url(self):
        text = FETCH.read_text()
        self.assertIn("https://github.com/HutsonLabs/excali-rust/releases/download", text)

    # .github/workflows/release.yml

    def test_release_workflow_builds_packs_and_uploads_on_a_tag(self):
        text = WORKFLOW.read_text()
        for needle in (
            "tags:",
            "scripts/gates/version.py tag",
            "scripts/web/build.sh",
            "scripts/release/package.py pack",
            "scripts/release/package.py verify",
            "gh release upload",
            "contents: write",
        ):
            self.assertIn(needle, text)

    def test_release_workflow_takes_the_notes_of_the_tag(self):
        text = WORKFLOW.read_text()
        self.assertIn('notes="scripts/release/notes/$TAG.md"', text)
        self.assertIn('--notes-file "$notes"', text)
        self.assertIn("gh release edit", text)

    # scripts/release/notes (ex-804)

    def test_the_workspace_version_has_release_notes(self):
        notes = NOTES / f"v{WORKSPACE_VERSION}.md"
        self.assertTrue(notes.is_file(), f"{notes.relative_to(ROOT)} is missing")
        text = notes.read_text(encoding="utf-8")
        for needle in (
            f"excali-web_{WORKSPACE_VERSION}.tar.gz",
            "SHA256SUMS",
            f"scripts/release/fetch.sh {WORKSPACE_VERSION} ",
            f'tag = "v{WORKSPACE_VERSION}"',
            "ADR-009",
            "Not published to crates.io, npm or any other registry",
        ):
            self.assertIn(needle, text)

    def test_every_notes_file_is_named_for_a_calendar_tag(self):
        for p in sorted(NOTES.glob("*")):
            self.assertEqual(p.suffix, ".md", p.name)
            self.assertEqual(calver.tag(p.stem[1:]), p.stem, p.name)


if __name__ == "__main__":
    unittest.main()
