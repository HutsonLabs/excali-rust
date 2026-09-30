#!/usr/bin/env python3
"""Tests for the web runtime release asset (ex-802).

scripts/release/package.py packs a web build (scripts/web/build.sh's OUT)
into excali-web_<version>.tar.gz with its line in SHA256SUMS, the two assets
a GitHub release carries (ADR-009); scripts/release/fetch.sh is the other
end, what a host's vendor script (term.hut's scripts/vendor-excali.sh, in
the style of its scripts/vendor-catppuccin-icons.sh) runs to download a
release, check the digest and unpack it. The tests build a stand-in dist/,
pack it, and fetch it back through a file:// release URL, offline.

The example app's macOS release (ex-805): scripts/release/macos-dmg.sh's
argument handling and dry run, and the latest.json and SHA256SUMS lines it
writes through package.py's latest-json and sums.

Run: python3 scripts/release/test_release.py -v
"""
from __future__ import annotations

import gzip
import hashlib
import io
import json
import os
import shlex
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
MACOS_DMG = ROOT / "scripts" / "release" / "macos-dmg.sh"
TAURI_CONF = ROOT / "examples" / "tauri-app" / "src-tauri" / "tauri.conf.json"
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

    def test_release_notes_link_the_integration_walk_transcript(self):
        # M8 acceptance: the walk transcript from a fresh clone is linked.
        text = (NOTES / f"v{WORKSPACE_VERSION}.md").read_text(encoding="utf-8")
        self.assertIn(
            "https://hutsonlabs.github.io/excali-rust/architecture/integration-walk/", text
        )
        self.assertTrue(
            (ROOT / "site" / "content" / "architecture" / "integration-walk.md").is_file()
        )

    def test_published_release_notes_are_kept(self):
        # v26.9.1 stays as published (owner decision, 2026-09-30, ex-806).
        text = (NOTES / "v26.9.1.md").read_text(encoding="utf-8")
        self.assertIn("The first release of excali-rust", text)
        self.assertIn("excali-web_26.9.1.tar.gz", text)

    def test_v26_9_2_notes_name_the_signed_app_and_the_updater(self):
        text = (NOTES / "v26.9.2.md").read_text(encoding="utf-8")
        for needle in (
            "since v26.9.1",
            "Excali.Example_26.9.2_aarch64.dmg",
            "Excali.Example_26.9.2_aarch64.app.tar.gz",
            "Excali.Example_26.9.2_aarch64.app.tar.gz.sig",
            "latest.json",
            "notarized",
            "scripts/release/macos-dmg.sh",
            "releases/latest/download/latest.json",
        ):
            self.assertIn(needle, text)
        for pr in (155, 158, 159, 160, 161, 162):
            self.assertIn(f"/pull/{pr}", text)

    def test_v26_9_3_notes_name_host_theming_and_the_signed_app(self):
        # v26.9.2 stays as published (ex-808); its notes are checked above.
        text = (NOTES / "v26.9.3.md").read_text(encoding="utf-8")
        for needle in (
            "since v26.9.2",
            "v26.9.2 stays as published",
            "excali-web_26.9.3.tar.gz",
            "Excali.Example_26.9.3_aarch64.dmg",
            "Excali.Example_26.9.3_aarch64.app.tar.gz",
            "Excali.Example_26.9.3_aarch64.app.tar.gz.sig",
            "latest.json",
            "notarized",
            "scripts/release/macos-dmg.sh 26.9.3 --upload v26.9.3",
            "releases/latest/download/latest.json",
            "--excali-canvas-background",
            "theme",
        ):
            self.assertIn(needle, text)
        self.assertIn("/pull/164", text)

    def test_every_notes_file_is_named_for_a_calendar_tag(self):
        for p in sorted(NOTES.glob("*")):
            self.assertEqual(p.suffix, ".md", p.name)
            self.assertEqual(calver.tag(p.stem[1:]), p.stem, p.name)


def commands(text: str) -> list[list[str]]:
    """The commands a dry run prints (`+ ` and the argv, shell-quoted)."""
    return [shlex.split(line[2:]) for line in text.splitlines() if line.startswith("+ ")]


APP_VERSION = json.loads(TAURI_CONF.read_text(encoding="utf-8"))["version"]
NAME = f"Excali.Example_{APP_VERSION}_aarch64"


class MacosReleaseTest(unittest.TestCase):
    """scripts/release/macos-dmg.sh and the files it stages (ex-805)."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="excali-macos-test."))

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def dmg(self, *args: str, **env: str) -> subprocess.CompletedProcess[str]:
        # No credential can leak into a dry run: none is in its environment.
        base = {k: v for k, v in os.environ.items() if not k.startswith(("TAURI_SIGNING", "APPLE_"))}
        base.pop("EXCALI_WEB_DIST", None)
        return subprocess.run(
            ["bash", str(MACOS_DMG), *args],
            capture_output=True,
            text=True,
            env={**base, "EXCALI_RELEASE_ENV": str(self.tmp / "no.env"), **env},
        )

    def pack(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, str(PACKAGE), *args], capture_output=True, text=True)

    # arguments

    def test_usage_errors_exit_2_before_anything_runs(self):
        for args in (
            (),
            ("--dry-run",),
            (APP_VERSION, "--bogus"),
            (APP_VERSION, APP_VERSION),
            (APP_VERSION, "--upload"),
            (APP_VERSION, "--upload", "--dry-run"),
            (APP_VERSION, "--out"),
            ("26.09.1",),
            ("v" + APP_VERSION,),
        ):
            with self.subTest(args=args):
                r = self.dmg(*args)
                self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
                self.assertEqual(r.stdout, "", "nothing ran")

    def test_help_prints_the_usage(self):
        r = self.dmg("--help")
        self.assertEqual(r.returncode, 0)
        self.assertIn("usage: scripts/release/macos-dmg.sh <version> [--upload <tag>]", r.stdout)

    def test_the_version_must_be_the_apps(self):
        r = self.dmg("26.12.9", "--dry-run")
        self.assertEqual(r.returncode, 2)
        self.assertIn(f"tauri.conf.json is at {APP_VERSION}", r.stderr)

    def test_the_upload_tag_must_be_the_versions(self):
        r = self.dmg(APP_VERSION, "--upload", "v26.12.9", "--dry-run")
        self.assertEqual(r.returncode, 2)
        self.assertIn(f"the tag must be v{APP_VERSION}", r.stderr)

    # dry run

    def test_dry_run_prints_every_step_and_touches_nothing(self):
        out = self.tmp / "out"
        r = self.dmg(APP_VERSION, "--dry-run", "--out", str(out))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(out.exists())
        text = r.stdout
        steps = [
            f"scripts/release/fetch.sh {APP_VERSION} ",
            "credentials from " + str(self.tmp / "no.env"),
            "signing identity FE9B9ADD91CB67176BFE80FF725F77539D75BD96",
            "keychain item excali-example-updater-key",
            "+ codesign --verify --deep --strict ",
            "+ xcrun notarytool submit ",
            "+ xcrun stapler staple ",
            "+ xcrun stapler validate ",
            "+ spctl -a -t open --context context:primary-signature -v ",
            f"{out}/{NAME}.dmg",
            f"{out}/{NAME}.app.tar.gz",
            f"{out}/{NAME}.app.tar.gz.sig",
            f"package.py latest-json {out} {NAME}.app.tar.gz --version {APP_VERSION}",
            f"https://github.com/HutsonLabs/excali-rust/releases/download/v{APP_VERSION}/{NAME}.app.tar.gz",
            f"package.py sums {out} {NAME}.dmg {NAME}.app.tar.gz {NAME}.app.tar.gz.sig latest.json",
        ]
        at = 0
        for needle in steps:
            i = text.find(needle, at)
            self.assertNotEqual(i, -1, f"{needle!r} not in order in:\n{text}")
            at = i
        self.assertIn(
            ["cargo", "tauri", "build", "--bundles", "app,dmg", "--config",
             '{"bundle":{"macOS":{"signingIdentity":"FE9B9ADD91CB67176BFE80FF725F77539D75BD96"}}}'],
            commands(text),
        )
        self.assertLess(text.index("updater signing key"), text.index("+ cargo tauri build"))
        self.assertLess(text.index("+ cargo tauri build"), text.index("+ codesign"))
        self.assertNotIn("gh release", text, "no upload without --upload")
        # CI=true for bundle_dmg.sh without Finder; the private key never shows.
        self.assertIn("CI=true", MACOS_DMG.read_text())
        self.assertNotIn("TAURI_SIGNING_PRIVATE_KEY", text)

    def test_dry_run_upload_merges_sha256sums_and_clobbers(self):
        out = self.tmp / "out"
        r = self.dmg(APP_VERSION, "--upload", f"v{APP_VERSION}", "--out", str(out), "--dry-run")
        self.assertEqual(r.returncode, 0, r.stderr)
        text = r.stdout
        download = f"+ gh release download v{APP_VERSION} -R HutsonLabs/excali-rust -p SHA256SUMS -D {out}"
        sums = f"package.py sums {out} "
        upload = f"+ gh release upload v{APP_VERSION} -R HutsonLabs/excali-rust --clobber "
        self.assertLess(text.index(download), text.index(sums))
        self.assertLess(text.index(sums), text.index(upload))
        cmd = next(c for c in commands(text) if c[:3] == ["gh", "release", "upload"])
        self.assertEqual(cmd[:7], ["gh", "release", "upload", f"v{APP_VERSION}", "-R", "HutsonLabs/excali-rust", "--clobber"])
        self.assertEqual(
            cmd[7:],
            [str(out / a) for a in (f"{NAME}.dmg", f"{NAME}.app.tar.gz", f"{NAME}.app.tar.gz.sig",
                                    "latest.json", "SHA256SUMS")],
        )

    def test_dry_run_takes_the_identity_and_web_build_given(self):
        r = self.dmg(APP_VERSION, "--dry-run", APPLE_SIGNING_IDENTITY="ABC123", EXCALI_WEB_DIST="/tmp/web")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("signing identity ABC123", r.stdout)
        build = next(c for c in commands(r.stdout) if c[:3] == ["cargo", "tauri", "build"])
        self.assertEqual(json.loads(build[-1]), {"bundle": {"macOS": {"signingIdentity": "ABC123"}}})
        self.assertIn("web runtime: /tmp/web", r.stdout)
        self.assertNotIn("fetch.sh", r.stdout)

    def test_the_identity_is_never_in_tauri_conf_json(self):
        self.assertNotIn("signingIdentity", TAURI_CONF.read_text(encoding="utf-8"))

    # latest.json and SHA256SUMS

    def staged(self) -> Path:
        out = self.tmp / "stage"
        out.mkdir()
        (out / f"{NAME}.app.tar.gz").write_bytes(b"app tarball")
        (out / f"{NAME}.app.tar.gz.sig").write_text("dW50cnVzdGVkIGNvbW1lbnQ6IHNpZw==\n")
        (out / f"{NAME}.dmg").write_bytes(b"dmg")
        return out

    def test_latest_json_is_what_the_updater_reads(self):
        out = self.staged()
        r = self.pack("latest-json", str(out), f"{NAME}.app.tar.gz", "--version", APP_VERSION,
                      "--pub-date", "2026-09-30T12:00:00Z")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(
            json.loads((out / "latest.json").read_text()),
            {
                "version": APP_VERSION,
                "pub_date": "2026-09-30T12:00:00Z",
                "platforms": {
                    "darwin-aarch64": {
                        "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZw==",
                        "url": "https://github.com/HutsonLabs/excali-rust/releases/download/"
                        f"v{APP_VERSION}/{NAME}.app.tar.gz",
                    }
                },
            },
        )

    def test_latest_json_pub_date_defaults_to_now_in_utc(self):
        out = self.staged()
        r = self.pack("latest-json", str(out), f"{NAME}.app.tar.gz", "--version", APP_VERSION)
        self.assertEqual(r.returncode, 0, r.stderr)
        date = json.loads((out / "latest.json").read_text())["pub_date"]
        self.assertRegex(date, r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")

    def test_latest_json_refuses_a_missing_signature_or_a_bad_version(self):
        out = self.staged()
        (out / f"{NAME}.app.tar.gz.sig").unlink()
        r = self.pack("latest-json", str(out), f"{NAME}.app.tar.gz", "--version", APP_VERSION)
        self.assertEqual(r.returncode, 1)
        self.assertFalse((out / "latest.json").exists())
        for args in (("--version", "1.2"), ("--pub-date", "x"), ("--version", APP_VERSION, "--bogus", "x")):
            r = self.pack("latest-json", str(out), f"{NAME}.app.tar.gz", *args)
            self.assertEqual(r.returncode, 2, args)

    def test_sums_sets_the_lines_of_the_names_and_keeps_the_rest(self):
        out = self.staged()
        (out / "latest.json").write_text("{}")
        (out / "SHA256SUMS").write_text(
            "0" * 64 + f"  excali-web_{APP_VERSION}.tar.gz\n" + "1" * 64 + f"  {NAME}.dmg\n"
        )
        names = [f"{NAME}.dmg", f"{NAME}.app.tar.gz", f"{NAME}.app.tar.gz.sig", "latest.json"]
        r = self.pack("sums", str(out), *names)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(
            (out / "SHA256SUMS").read_text(),
            "0" * 64 + f"  excali-web_{APP_VERSION}.tar.gz\n"
            + "".join(f"{sha256(out / n)}  {n}\n" for n in names),
        )
        # verify checks exactly those lines (the web tarball is not here).
        self.assertEqual(self.pack("verify", str(out), *names).returncode, 0)

    def test_sums_refuses_a_missing_file(self):
        out = self.staged()
        r = self.pack("sums", str(out), "latest.json")
        self.assertEqual(r.returncode, 1)
        self.assertFalse((out / "SHA256SUMS").exists())


if __name__ == "__main__":
    unittest.main()
