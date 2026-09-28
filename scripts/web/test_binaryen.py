#!/usr/bin/env python3
"""Tests for scripts/web/binaryen.py (the pinned wasm-opt, ex-501).

Offline: `ensure` runs against a file:// "release" whose tarball holds a
fake wasm-opt, so the download, SHA-256 check, unpacking and install run
without network access.

Run: python3 scripts/web/test_binaryen.py -v
"""
from __future__ import annotations

import hashlib
import io
import os
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import binaryen  # noqa: E402

# assets[].digest of the version_133 release as GitHub publishes them
# (gh api repos/WebAssembly/binaryen/releases/tags/version_133, 2026-09-28).
PUBLISHED = {
    "x86_64-linux": "2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489",
    "aarch64-linux": "89c07ea56faf38d0fbecf36ca8ec0721756716185f265b568e133d427f299bf8",
    "arm64-macos": "ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8",
    "x86_64-macos": "13a9b90be775c6389ce3d1f879cb8627bea56708ba8c122983941d53a8199b95",
}

FAKE_WASM_OPT = "#!/bin/sh\necho 'wasm-opt version {v} (version_{v})'\n"


def fake_release(root: Path, platform: str, version: str = "133", extra=()) -> str:
    """A release directory laid out like GitHub's; returns its SHA-256."""
    tag = f"version_{binaryen.VERSION}"
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:

        def add(name: str, data: bytes, mode: int = 0o644) -> None:
            info = tarfile.TarInfo(f"binaryen-{tag}/{name}")
            info.size = len(data)
            info.mode = mode
            tar.addfile(info, io.BytesIO(data))

        add("bin/wasm-opt", FAKE_WASM_OPT.format(v=version).encode(), 0o644)
        add("bin/wasm-dis", b"#!/bin/sh\n", 0o755)
        add("lib/libbinaryen.dylib", b"lib")
        add("include/binaryen-c.h", b"/* */")
        for name, data in extra:
            add(name, data)
    data = buf.getvalue()
    d = root / tag
    d.mkdir(parents=True, exist_ok=True)
    (d / f"binaryen-{tag}-{platform}.tar.gz").write_bytes(data)
    return hashlib.sha256(data).hexdigest()


class PinTest(unittest.TestCase):
    def test_version_is_133(self):
        self.assertEqual(binaryen.VERSION, "133")
        self.assertEqual(binaryen.TAG, "version_133")

    def test_pinned_digests_are_the_published_ones(self):
        self.assertEqual(binaryen.PINNED_SHA256, PUBLISHED)
        for name, digest in PUBLISHED.items():
            self.assertEqual(binaryen.pinned_digest(name), digest)

    def test_unpinned_platform_is_refused(self):
        with self.assertRaisesRegex(binaryen.BinaryenError, "no pinned sha256"):
            binaryen.pinned_digest("arm64-windows")

    def test_platform_names(self):
        cases = {
            ("Linux", "x86_64"): "x86_64-linux",
            ("Linux", "aarch64"): "aarch64-linux",
            ("Darwin", "arm64"): "arm64-macos",
            ("Darwin", "x86_64"): "x86_64-macos",
        }
        for (system, machine), want in cases.items():
            self.assertEqual(binaryen.platform_name(system, machine), want)
        with self.assertRaisesRegex(binaryen.BinaryenError, "no binaryen version_133 build"):
            binaryen.platform_name("Plan9", "mips")


class EnsureTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="binaryen-test."))
        self.release = self.tmp / "release"
        self.tools = self.tmp / "tools"
        self.empty_path = self.tmp / "empty-bin"
        self.empty_path.mkdir()
        self.env = mock.patch.dict(
            os.environ,
            {
                "BINARYEN_TOOLS_DIR": str(self.tools),
                "BINARYEN_BASE_URL": self.release.as_uri(),
                "BINARYEN_PLATFORM": "arm64-macos",
                "PATH": f"{self.empty_path}:/bin:/usr/bin",
            },
        )
        self.env.start()
        self.logs: list[str] = []

    def tearDown(self):
        self.env.stop()
        import shutil

        shutil.rmtree(self.tmp, ignore_errors=True)

    def ensure(self):
        return binaryen.ensure(log=self.logs.append)

    def pin(self, digest: str):
        return mock.patch.dict(binaryen.PINNED_SHA256, {"arm64-macos": digest})

    def test_downloads_verifies_and_installs_wasm_opt_and_lib(self):
        digest = fake_release(self.release, "arm64-macos")
        with self.pin(digest):
            path = self.ensure()
        dest = self.tools / "binaryen-version_133"
        self.assertEqual(path, dest / "bin" / "wasm-opt")
        self.assertTrue(os.access(path, os.X_OK))
        self.assertTrue((dest / "lib" / "libbinaryen.dylib").is_file())
        # Only wasm-opt and its library are unpacked.
        self.assertFalse((dest / "bin" / "wasm-dis").exists())
        self.assertFalse((dest / "include").exists())
        self.assertEqual(binaryen.version_of(str(path)), "133")
        self.assertTrue(any("sha256 verified" in m for m in self.logs))
        # No download leftovers.
        self.assertEqual(sorted(p.name for p in self.tools.iterdir()), ["binaryen-version_133"])

    def test_second_ensure_does_not_download(self):
        digest = fake_release(self.release, "arm64-macos")
        with self.pin(digest):
            first = self.ensure()
        (self.release / "version_133" / "binaryen-version_133-arm64-macos.tar.gz").unlink()
        self.logs.clear()
        with self.pin(digest):
            self.assertEqual(self.ensure(), first)
        self.assertEqual(self.logs, [])

    def test_digest_mismatch_installs_nothing(self):
        fake_release(self.release, "arm64-macos")
        with self.pin("0" * 64):
            with self.assertRaisesRegex(binaryen.BinaryenError, "sha256 mismatch"):
                self.ensure()
        self.assertEqual(list(self.tools.iterdir()), [])

    def test_wrong_version_in_tarball_is_refused(self):
        digest = fake_release(self.release, "arm64-macos", version="120")
        with self.pin(digest):
            with self.assertRaisesRegex(binaryen.BinaryenError, "reports version 120"):
                self.ensure()
            # Nothing is installed, so the next ensure cannot take the cached
            # branch: it downloads again and is refused again.
            self.assertEqual(list(self.tools.iterdir()), [])
            with self.assertRaisesRegex(binaryen.BinaryenError, "reports version 120"):
                self.ensure()
            self.assertEqual(list(self.tools.iterdir()), [])

    def install_cached(self, version: str) -> Path:
        """A wasm-opt already in .tools/ (a previous run or a CI cache restore)."""
        cached = self.tools / "binaryen-version_133" / "bin" / "wasm-opt"
        cached.parent.mkdir(parents=True)
        cached.write_text(FAKE_WASM_OPT.format(v=version))
        cached.chmod(0o755)
        return cached

    def test_cached_wrong_version_is_replaced_by_a_verified_download(self):
        cached = self.install_cached("120")
        digest = fake_release(self.release, "arm64-macos")
        with self.pin(digest):
            path = self.ensure()
        self.assertEqual(path, cached)
        self.assertEqual(binaryen.version_of(str(path)), "133")
        self.assertTrue(any("reports version 120" in m for m in self.logs))
        self.assertTrue(any("sha256 verified" in m for m in self.logs))

    def test_cached_wrong_version_with_no_good_download_is_refused(self):
        self.install_cached("120")
        digest = fake_release(self.release, "arm64-macos", version="121")
        with self.pin(digest):
            with self.assertRaisesRegex(binaryen.BinaryenError, "reports version 121"):
                self.ensure()
        self.assertEqual(list(self.tools.iterdir()), [])

    def test_cached_pinned_version_is_used_without_download(self):
        cached = self.install_cached("133")
        self.assertEqual(self.ensure(), cached)
        self.assertEqual(self.logs, [])

    def test_path_escape_in_tarball_is_refused(self):
        digest = fake_release(
            self.release, "arm64-macos", extra=[("lib/../../../escape", b"x")]
        )
        with self.pin(digest):
            with self.assertRaisesRegex(binaryen.BinaryenError, "refusing to unpack"):
                self.ensure()
        self.assertFalse((self.tmp / "escape").exists())

    def test_pinned_wasm_opt_on_path_is_used(self):
        stub = self.empty_path / "wasm-opt"
        stub.write_text(FAKE_WASM_OPT.format(v="133"))
        stub.chmod(0o755)
        self.assertEqual(self.ensure(), stub)
        self.assertFalse(self.tools.exists())

    def test_other_wasm_opt_on_path_is_ignored(self):
        stub = self.empty_path / "wasm-opt"
        stub.write_text(FAKE_WASM_OPT.format(v="116"))
        stub.chmod(0o755)
        digest = fake_release(self.release, "arm64-macos")
        with self.pin(digest):
            path = self.ensure()
        self.assertEqual(path, self.tools / "binaryen-version_133" / "bin" / "wasm-opt")

    def test_cli(self):
        self.assertEqual(binaryen.main(["digest", "x86_64-linux"]), 0)
        self.assertEqual(binaryen.main(["digest", "sparc-solaris"]), 1)
        self.assertEqual(binaryen.main(["bogus"]), 2)


if __name__ == "__main__":
    unittest.main()
