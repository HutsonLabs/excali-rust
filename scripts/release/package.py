#!/usr/bin/env python3
"""The web runtime as a GitHub release asset (ex-802, ADR-009).

A release carries excali-web_<version>.tar.gz, the four entries
scripts/web/build.sh writes (excali_editor.js, excali_editor_bg.wasm,
excali.css, fonts/) at the top level of the archive, and SHA256SUMS with its
digest in shasum's format ("<hex>  <name>"), one line per asset. A host
fetches both, checks the line and unpacks (scripts/release/fetch.sh).

Subcommands:
  pack DIST OUT [--version V]   pack DIST (a build.sh OUT) into
                                OUT/excali-web_<V>.tar.gz and set its line in
                                OUT/SHA256SUMS, keeping the lines of other
                                assets. V defaults to the workspace version and
                                must be calendar (YY.M.BUILD). The archive is
                                reproducible: entries in byte order, owner
                                0:0 with no names, modes 0644 and 0755, every
                                mtime SOURCE_DATE_EPOCH (default: the commit
                                time of HEAD), and no timestamp in the gzip
                                header.
  verify OUT [NAME...]          check every line of OUT/SHA256SUMS, or only
                                the lines of the NAMEs (a release's SHA256SUMS
                                also lists assets built elsewhere)
  sums OUT NAME...              set the lines of the NAMEs (files in OUT) in
                                OUT/SHA256SUMS, keeping the lines of other
                                assets (the example app's DMG and updater
                                files, scripts/release/macos-dmg.sh)
  latest-json OUT TARBALL --version V [--pub-date RFC3339]
                                write OUT/latest.json, what the example app's
                                updater reads (tauri-plugin-updater's static
                                JSON): version V, the pub_date (default now,
                                UTC), and for darwin-aarch64 the signature in
                                OUT/TARBALL.sig and the URL of TARBALL on the
                                GitHub release v<V>

Exit 0 on success, 1 on a bad DIST or digest, 2 on a usage error.
Tests: scripts/release/test_release.py
"""
from __future__ import annotations

import datetime
import gzip
import hashlib
import io
import json
import os
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "gates"))
import version as calver  # noqa: E402

ENTRIES = ("excali.css", "excali_editor.js", "excali_editor_bg.wasm", "fonts")
SUMS = "SHA256SUMS"


def asset_name(version: str) -> str:
    return f"excali-web_{version}.tar.gz"


def dist_problems(dist: Path) -> list[str]:
    if not dist.is_dir():
        return [f"{dist}: not a directory"]
    problems = []
    for name in ENTRIES:
        p = dist / name
        if not (p.is_dir() if name == "fonts" else p.is_file()):
            problems.append(f"{dist}: missing {name}{'/' if name == 'fonts' else ''}")
    if (dist / "fonts").is_dir() and not (dist / "fonts" / "manifest.json").is_file():
        problems.append(f"{dist}: missing fonts/manifest.json")
    for p in sorted(dist.iterdir()):
        if p.name not in ENTRIES:
            problems.append(f"{dist}: unexpected entry {p.name} (the release is {', '.join(ENTRIES)})")
    for p in sorted(dist.rglob("*")):
        if p.is_symlink() or not (p.is_file() or p.is_dir()):
            problems.append(f"{dist}: {p.relative_to(dist).as_posix()} is not a regular file or directory")
    return problems


def source_date_epoch() -> int:
    env = os.environ.get("SOURCE_DATE_EPOCH")
    if env:
        return int(env)
    r = subprocess.run(["git", "-C", str(ROOT), "log", "-1", "--format=%ct"], capture_output=True, text=True)
    return int(r.stdout.strip()) if r.returncode == 0 and r.stdout.strip() else 0


def tarball(dist: Path, mtime: int) -> bytes:
    paths: list[Path] = []
    for name in ENTRIES:
        top = dist / name
        paths.append(top)
        if top.is_dir():
            paths.extend(top.rglob("*"))
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.USTAR_FORMAT) as tar:
        for p in sorted(paths, key=lambda p: p.relative_to(dist).as_posix().encode()):
            info = tarfile.TarInfo(p.relative_to(dist).as_posix())
            info.mtime = mtime
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            if p.is_dir():
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                tar.addfile(info)
            else:
                data = p.read_bytes()
                info.mode = 0o644
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
    out = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=out, mtime=0, compresslevel=9) as gz:
        gz.write(buf.getvalue())
    return out.getvalue()


def read_sums(path: Path) -> list[tuple[str, str]]:
    if not path.exists():
        return []
    lines = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            digest, _, name = line.partition("  ")
            lines.append((digest, name))
    return lines


def pack(dist: Path, out: Path, version: str) -> int:
    problems = dist_problems(dist)
    if problems:
        for p in problems:
            print(f"release: {p}", file=sys.stderr)
        return 1
    data = tarball(dist, source_date_epoch())
    name = asset_name(version)
    out.mkdir(parents=True, exist_ok=True)
    (out / name).write_bytes(data)
    digest = hashlib.sha256(data).hexdigest()
    lines = [(d, n) for d, n in read_sums(out / SUMS) if n != name] + [(digest, name)]
    (out / SUMS).write_text("".join(f"{d}  {n}\n" for d, n in lines), encoding="utf-8")
    print(f"release: {out / name} ({len(data)} bytes, sha256 {digest})")
    return 0


def verify(out: Path, names: list[str]) -> int:
    lines = read_sums(out / SUMS)
    if not lines:
        print(f"release: {out / SUMS} is missing or empty", file=sys.stderr)
        return 1
    bad = 0
    for name in names:
        if name not in {n for _, n in lines}:
            print(f"release: {name} is not in {SUMS}", file=sys.stderr)
            bad += 1
    if names:
        lines = [(d, n) for d, n in lines if n in names]
    for digest, name in lines:
        p = out / name
        if not p.is_file():
            print(f"release: {name} is in {SUMS} but not in {out}", file=sys.stderr)
            bad += 1
        elif hashlib.sha256(p.read_bytes()).hexdigest() != digest:
            print(f"release: {name} does not match its SHA-256 in {SUMS}", file=sys.stderr)
            bad += 1
        else:
            print(f"release: {name} OK")
    return 1 if bad else 0


def set_sums(out: Path, names: list[str]) -> int:
    missing = [n for n in names if not (out / n).is_file()]
    if missing or not names:
        for n in missing:
            print(f"release: {out / n} is missing", file=sys.stderr)
        return 1
    digests = {n: hashlib.sha256((out / n).read_bytes()).hexdigest() for n in names}
    lines = [(d, n) for d, n in read_sums(out / SUMS) if n not in digests] + [(digests[n], n) for n in names]
    (out / SUMS).write_text("".join(f"{d}  {n}\n" for d, n in lines), encoding="utf-8")
    for n in names:
        print(f"release: {digests[n]}  {n}")
    return 0


DOWNLOAD = "https://github.com/HutsonLabs/excali-rust/releases/download"
PLATFORM = "darwin-aarch64"


def latest_json(version: str, signature: str, tarball: str, pub_date: str) -> dict:
    return {
        "version": version,
        "pub_date": pub_date,
        "platforms": {
            PLATFORM: {
                "signature": signature.strip(),
                "url": f"{DOWNLOAD}/{calver.tag(version)}/{tarball}",
            }
        },
    }


def write_latest_json(out: Path, tarball: str, version: str, pub_date: str | None) -> int:
    sig = out / f"{tarball}.sig"
    if not (out / tarball).is_file() or not sig.is_file():
        print(f"release: {out / tarball} and its .sig are needed", file=sys.stderr)
        return 1
    signature = sig.read_text(encoding="utf-8").strip()
    if not signature:
        print(f"release: {sig} is empty", file=sys.stderr)
        return 1
    if pub_date is None:
        pub_date = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    doc = latest_json(version, signature, tarball, pub_date)
    (out / "latest.json").write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    print(f"release: {out / 'latest.json'} ({version}, {doc['platforms'][PLATFORM]['url']})")
    return 0


USAGE = (
    "usage: package.py pack DIST OUT [--version V] | package.py verify OUT [NAME...]"
    " | package.py sums OUT NAME... | package.py latest-json OUT TARBALL --version V [--pub-date D]"
)


def main(argv: list[str]) -> int:
    if argv[:1] == ["verify"] and len(argv) >= 2:
        return verify(Path(argv[1]), argv[2:])
    if argv[:1] == ["sums"] and len(argv) >= 3:
        return set_sums(Path(argv[1]), argv[2:])
    if argv[:1] == ["latest-json"] and len(argv) in (5, 7):
        opts = dict(zip(argv[3::2], argv[4::2]))
        if set(opts) - {"--version", "--pub-date"} or "--version" not in opts:
            print(USAGE, file=sys.stderr)
            return 2
        try:
            calver.parse(opts["--version"])
        except ValueError as e:
            print(f"release: {e}", file=sys.stderr)
            return 2
        return write_latest_json(Path(argv[1]), argv[2], opts["--version"], opts.get("--pub-date"))
    if argv[:1] == ["pack"] and len(argv) in (3, 5):
        if len(argv) == 5:
            if argv[3] != "--version":
                print(USAGE, file=sys.stderr)
                return 2
            version = argv[4]
        else:
            version = calver.workspace_version((ROOT / "Cargo.toml").read_text(encoding="utf-8")) or ""
        try:
            calver.parse(version)
        except ValueError as e:
            print(f"release: {e}", file=sys.stderr)
            return 2
        return pack(Path(argv[1]), Path(argv[2]), version)
    print(USAGE, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
