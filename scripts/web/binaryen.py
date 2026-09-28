#!/usr/bin/env python3
"""Pinned binaryen (wasm-opt) for the web build (ex-501).

scripts/web/build.sh runs `wasm-opt -Oz` over the wasm-bindgen output. The
optimiser changes the bytes the size budget measures, so the version is
pinned like Zola's (scripts/site/zola.sh): `ensure` downloads the release
tarball for this host into .tools/, verifies its SHA-256 against the digest
recorded below (a download with no pinned digest is refused), unpacks
bin/wasm-opt and lib/ (the macOS binary links lib/libbinaryen.dylib), and
checks that the unpacked wasm-opt reports the pinned version before moving it
into place, and prints the path of wasm-opt. A wasm-opt of exactly the pinned
version already on PATH, or already installed in .tools/ (a previous run or a
CI cache restore), is used as is; an installed one of any other version is
removed and downloaded again.

    python3 scripts/web/binaryen.py ensure          path of wasm-opt
    python3 scripts/web/binaryen.py platform        this host's asset name
    python3 scripts/web/binaryen.py digest NAME     pinned SHA-256 of an asset

Overrides: BINARYEN_TOOLS_DIR (download directory), BINARYEN_BASE_URL
(release mirror; file:// in the tests), BINARYEN_PLATFORM.
Tests: scripts/web/test_binaryen.py
"""
from __future__ import annotations

import hashlib
import os
import platform as host
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

VERSION = "133"
TAG = f"version_{VERSION}"
BASE_URL = "https://github.com/WebAssembly/binaryen/releases/download"

# assets[].digest of the version_133 release (published 2026-09-21), read
# with `gh api repos/WebAssembly/binaryen/releases/latest` on 2026-09-28;
# arm64-macos was also recomputed with `shasum -a 256` from the download.
PINNED_SHA256 = {
    "x86_64-linux": "2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489",
    "aarch64-linux": "89c07ea56faf38d0fbecf36ca8ec0721756716185f265b568e133d427f299bf8",
    "arm64-macos": "ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8",
    "x86_64-macos": "13a9b90be775c6389ce3d1f879cb8627bea56708ba8c122983941d53a8199b95",
}

ROOT = Path(__file__).resolve().parents[2]


class BinaryenError(Exception):
    pass


def platform_name(system: str | None = None, machine: str | None = None) -> str:
    """The release asset's platform for this host (`binaryen-<tag>-<this>`)."""
    if system is None and machine is None and os.environ.get("BINARYEN_PLATFORM"):
        return os.environ["BINARYEN_PLATFORM"]
    system = system or host.system()
    machine = (machine or host.machine()).lower()
    table = {
        ("Linux", "x86_64"): "x86_64-linux",
        ("Linux", "amd64"): "x86_64-linux",
        ("Linux", "aarch64"): "aarch64-linux",
        ("Linux", "arm64"): "aarch64-linux",
        ("Darwin", "arm64"): "arm64-macos",
        ("Darwin", "aarch64"): "arm64-macos",
        ("Darwin", "x86_64"): "x86_64-macos",
    }
    try:
        return table[(system, machine)]
    except KeyError:
        raise BinaryenError(f"no binaryen {TAG} build for {system}-{machine}") from None


def pinned_digest(name: str) -> str:
    try:
        return PINNED_SHA256[name]
    except KeyError:
        raise BinaryenError(f"no pinned sha256 for binaryen {TAG} {name}") from None


def tools_dir() -> Path:
    """.tools/ beside the main clone's .git, shared by every worktree."""
    if os.environ.get("BINARYEN_TOOLS_DIR"):
        return Path(os.environ["BINARYEN_TOOLS_DIR"])
    try:
        common = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "--git-common-dir"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as e:
        raise BinaryenError(
            f"cannot find the common git directory of {ROOT}; set BINARYEN_TOOLS_DIR"
        ) from e
    path = Path(common)
    if not path.is_absolute():
        path = ROOT / path
    return path.resolve().parent / ".tools"


def version_of(wasm_opt: str) -> str | None:
    """The version wasm-opt reports (`wasm-opt version 133 (version_133)`)."""
    try:
        out = subprocess.run(
            [wasm_opt, "--version"], capture_output=True, text=True, timeout=30
        ).stdout
    except OSError:
        return None
    parts = out.split()
    if len(parts) >= 3 and parts[1] == "version":
        return parts[2]
    return None


def on_path() -> str | None:
    found = shutil.which("wasm-opt")
    if found and version_of(found) == VERSION:
        return found
    return None


def install_dir(tools: Path) -> Path:
    return tools / f"binaryen-{TAG}"


def ensure(log=lambda msg: print(msg, file=sys.stderr)) -> Path:
    found = on_path()
    if found:
        return Path(found)
    tools = tools_dir()
    dest = install_dir(tools)
    wasm_opt = dest / "bin" / "wasm-opt"
    if wasm_opt.is_file() and os.access(wasm_opt, os.X_OK):
        # A previous run or a CI cache restore; trust it only at the pin.
        have = version_of(str(wasm_opt))
        if have == VERSION:
            return wasm_opt
        log(f"binaryen: {wasm_opt} reports version {have}, expected {VERSION}; reinstalling")
    if dest.exists():
        shutil.rmtree(dest)
    name = platform_name()
    want = pinned_digest(name)
    base = os.environ.get("BINARYEN_BASE_URL", BASE_URL)
    url = f"{base}/{TAG}/binaryen-{TAG}-{name}.tar.gz"
    tools.mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix=".binaryen-download.", dir=tools))
    try:
        tgz = tmp / "binaryen.tar.gz"
        log(f"binaryen: downloading {url}")
        with urllib.request.urlopen(url) as resp, open(tgz, "wb") as f:
            shutil.copyfileobj(resp, f)
        got = hashlib.sha256(tgz.read_bytes()).hexdigest()
        if got != want:
            raise BinaryenError(f"sha256 mismatch for {url}: expected {want}, got {got}")
        log(f"binaryen: sha256 verified ({name} {got})")
        prefix = f"binaryen-{TAG}/"
        unpacked = tmp / "unpacked"
        with tarfile.open(tgz) as tar:
            members = [
                m
                for m in tar.getmembers()
                if m.name == f"{prefix}bin/wasm-opt" or m.name.startswith(f"{prefix}lib/")
            ]
            if not any(m.name == f"{prefix}bin/wasm-opt" for m in members):
                raise BinaryenError(f"{url} has no {prefix}bin/wasm-opt")
            for m in members:
                parts = Path(m.name).parts
                if m.name.startswith("/") or ".." in parts or not (m.isfile() or m.isdir()):
                    raise BinaryenError(f"{url}: refusing to unpack {m.name}")
            if hasattr(tarfile, "data_filter"):
                tar.extractall(unpacked, members=members, filter="data")
            else:
                tar.extractall(unpacked, members=members)
        extracted = unpacked / f"binaryen-{TAG}"
        candidate = extracted / "bin" / "wasm-opt"
        candidate.chmod(0o755)
        # Check the version before the install, so a wrong binary never
        # reaches dest (where the cached branch above would find it).
        have = version_of(str(candidate))
        if have != VERSION:
            raise BinaryenError(f"{url}: wasm-opt reports version {have}, expected {VERSION}")
        extracted.rename(dest)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return wasm_opt


def main(argv: list[str]) -> int:
    try:
        if argv == ["ensure"]:
            print(ensure())
        elif argv == ["platform"]:
            print(platform_name())
        elif len(argv) == 2 and argv[0] == "digest":
            print(pinned_digest(argv[1]))
        else:
            print("usage: binaryen.py ensure | platform | digest NAME", file=sys.stderr)
            return 2
    except BinaryenError as e:
        print(f"binaryen: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
