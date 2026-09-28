#!/usr/bin/env python3
"""Fixture corpus: build, check and re-verify fixtures/ (task ex-003).

fixtures/ holds byte-exact copies of third-party test inputs that the port's
conformance tests read:

  fixtures/upstream/<path>     upstream Excalidraw at the pinned commit
                               (site/config.toml extra.upstream_commit):
                               every file in packages/excalidraw/tests/fixtures,
                               the restore/reconcile tests and the restore
                               snapshot, the export test and the two export
                               snapshots with an embedded SVG payload
                               (research/data-model.md section 9), and the
                               MIT LICENSE.
  fixtures/libraries/          the public library catalogue from
                               excalidraw-libraries at the pinned commit
                               (extra.libraries_commit): libraries.json, the
                               MIT LICENSE, and every catalogue .excalidrawlib
                               stored gzip-compressed as <source>.gz.
  fixtures/manifest.json       every other file under fixtures/ with its
                               sha256, size and origin (raw URL at the pinned
                               commit plus the path inside that repository).

Libraries are stored gzip-compressed because the catalogue is ~90 MB of JSON
(~10 MB compressed) and because some library text contains emoji variation
selectors, which the authorship gate (rule R5) rejects in tracked text files.
The manifest records the hash of the stored .gz file and of the original
bytes (content_sha256), so the original is still pinned byte for byte;
read_fixture() returns the original bytes for either layout.

  corpus.py check            manifest matches disk (offline; the acceptance test)
  corpus.py verify-upstream  upstream copies equal the pinned upstream checkout
  corpus.py verify-remote    every file equals its origin URL (network)
  corpus.py sync             rebuild fixtures/ from the pins (network for libraries)

Exit status: 0 clean, 1 problems found, 2 usage error.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import datetime
import gzip
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path, PurePosixPath
from typing import Callable, NamedTuple

ROOT = Path(__file__).resolve().parents[2]
CONFIG = ROOT / "site" / "config.toml"
FIXTURES = ROOT / "fixtures"
CHECKOUT = ROOT / "scripts" / "upstream" / "checkout.sh"
MANIFEST = "manifest.json"
SCHEMA = 1

# Upstream: the whole fixtures directory plus these files (data-model.md s9).
UPSTREAM_FIXTURE_DIR = "packages/excalidraw/tests/fixtures"
UPSTREAM_EXTRA = (
    "LICENSE",
    "packages/excalidraw/tests/data/restore.test.ts",
    "packages/excalidraw/tests/data/reconcile.test.ts",
    "packages/excalidraw/tests/data/__snapshots__/restore.test.ts.snap",
    "packages/excalidraw/tests/export.test.tsx",
    "packages/excalidraw/tests/__snapshots__/export.test.tsx.snap",
    "packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap",
)
# excalidraw-libraries: catalogue and licence; the catalogue names the rest.
LIBRARIES_EXTRA = ("libraries.json", "LICENSE")
LIBRARY_DIR = "libraries"
LIBRARY_SUFFIX = ".excalidrawlib"


class CorpusError(Exception):
    pass


class Pins(NamedTuple):
    upstream_repo: str
    upstream_commit: str
    libraries_repo: str
    libraries_commit: str


def config_pins(config: Path = CONFIG) -> Pins:
    text = config.read_text(encoding="utf-8")

    def value(key: str) -> str:
        m = re.search(rf'^{key}\s*=\s*"([^"]*)"', text, re.M)
        if not m:
            raise CorpusError(f"{config} has no extra.{key}")
        return m.group(1)

    return Pins(value("upstream"), value("upstream_commit"), value("libraries"), value("libraries_commit"))


def raw_base(repo: str) -> str:
    """https://github.com/<org>/<repo> -> https://raw.githubusercontent.com/<org>/<repo>."""
    m = re.fullmatch(r"https://github\.com/([^/]+)/([^/]+?)(?:\.git)?/?", repo)
    if not m:
        raise CorpusError(f"not a GitHub repository URL: {repo}")
    return f"https://raw.githubusercontent.com/{m.group(1)}/{m.group(2)}"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def dump_manifest(manifest: dict) -> str:
    return json.dumps(manifest, indent=2, sort_keys=True, ensure_ascii=True) + "\n"


def stored_path(source: str, origin_path: str, gz: bool) -> str:
    if source == "upstream":
        rel = origin_path
    else:
        rel = origin_path[len(LIBRARY_DIR) + 1:] if origin_path.startswith(LIBRARY_DIR + "/") else origin_path
    return f"{source}/{rel}" + (".gz" if gz else "")


def safe_relpath(rel: str) -> str:
    p = PurePosixPath(rel)
    if not rel or p.is_absolute() or any(part in ("", ".", "..") for part in rel.split("/")) or "\\" in rel:
        raise CorpusError(f"unsafe path {rel!r}")
    return rel


# -- fetching -----------------------------------------------------------------


def fetch(url: str, attempts: int = 4) -> bytes:
    last: Exception | None = None
    for i in range(attempts):
        try:
            with urllib.request.urlopen(url, timeout=60) as r:
                return r.read()
        except urllib.error.HTTPError as e:
            if e.code < 500 and e.code != 429:
                raise
            last = e
        except (urllib.error.URLError, OSError) as e:
            if url.startswith("file:"):
                raise
            last = e
        time.sleep(2 ** i)
    assert last is not None
    raise last


def fetch_all(urls: list[str], fetcher: Callable[[str], bytes] = fetch) -> dict[str, bytes | Exception]:
    out: dict[str, bytes | Exception] = {}

    def one(u: str):
        try:
            return u, fetcher(u)
        except Exception as e:  # reported per URL by the caller
            return u, e

    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as ex:
        for u, res in ex.map(one, urls):
            out[u] = res
    return out


# -- upstream -----------------------------------------------------------------


def upstream_file_list(upstream_dir: Path) -> list[str]:
    """Upstream-relative paths the corpus copies from a checkout."""
    fx = upstream_dir / UPSTREAM_FIXTURE_DIR
    if not fx.is_dir():
        raise CorpusError(f"{fx} is not a directory (is {upstream_dir} an upstream checkout?)")
    files = [p.relative_to(upstream_dir).as_posix() for p in fx.rglob("*") if p.is_file()]
    for rel in UPSTREAM_EXTRA:
        if not (upstream_dir / rel).is_file():
            raise CorpusError(f"upstream file {rel} missing in {upstream_dir}")
        files.append(rel)
    return sorted(set(files))


# -- sync ---------------------------------------------------------------------


def _entry(source: str, repo: str, commit: str, origin_path: str, original: bytes, gz: bool) -> tuple[dict, bytes]:
    stored = gzip.compress(original, compresslevel=9, mtime=0) if gz else original
    e = {
        "path": stored_path(source, origin_path, gz),
        "source": source,
        "origin": f"{raw_base(repo)}/{commit}/{origin_path}",
        "origin_path": origin_path,
        "bytes": len(stored),
        "sha256": sha256(stored),
    }
    if gz:
        e.update(encoding="gzip", content_bytes=len(original), content_sha256=sha256(original))
    return e, stored


def sync(
    dest: Path,
    upstream_dir: Path,
    pins: Pins,
    libraries_raw_base: str | None = None,
    retrieved: str | None = None,
    fetcher: Callable[[str], bytes] = fetch,
) -> dict:
    """Rebuild dest from the upstream checkout and the pinned library catalogue.

    Everything is assembled in a temporary directory first; dest is only
    replaced once every file has been fetched, so a failed sync leaves the
    previous corpus untouched.
    """
    dest = Path(dest)
    lib_base = (libraries_raw_base or raw_base(pins.libraries_repo)).rstrip("/") + "/" + pins.libraries_commit
    entries: list[tuple[dict, bytes]] = []

    for rel in upstream_file_list(upstream_dir):
        data = (upstream_dir / rel).read_bytes()
        entries.append(_entry("upstream", pins.upstream_repo, pins.upstream_commit, rel, data, False))

    head = fetch_all([f"{lib_base}/{rel}" for rel in LIBRARIES_EXTRA], fetcher)
    extras: dict[str, bytes] = {}
    for rel in LIBRARIES_EXTRA:
        res = head[f"{lib_base}/{rel}"]
        if isinstance(res, Exception):
            raise CorpusError(f"fetch {lib_base}/{rel} failed: {res}")
        extras[rel] = res
        entries.append(_entry("libraries", pins.libraries_repo, pins.libraries_commit, rel, res, False))

    sources = catalogue_sources(extras["libraries.json"])
    urls = {s: f"{lib_base}/{LIBRARY_DIR}/{s}" for s in sources}
    got = fetch_all(list(urls.values()), fetcher)
    failed = [f"{u}: {got[u]}" for u in urls.values() if isinstance(got[u], Exception)]
    if failed:
        raise CorpusError(f"{len(failed)} library download(s) failed:\n  " + "\n  ".join(failed[:20]))
    for s in sources:
        entries.append(_entry(
            "libraries", pins.libraries_repo, pins.libraries_commit, f"{LIBRARY_DIR}/{s}", got[urls[s]], True,
        ))

    entries.sort(key=lambda t: t[0]["path"])
    manifest = {
        "schema": SCHEMA,
        "sources": {
            "upstream": {"repo": pins.upstream_repo, "commit": pins.upstream_commit},
            "libraries": {
                "repo": pins.libraries_repo,
                "commit": pins.libraries_commit,
                "retrieved": retrieved or datetime.datetime.now(datetime.timezone.utc).date().isoformat(),
            },
        },
        "files": [e for e, _ in entries],
    }

    dest.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".fixtures-sync-", dir=dest.parent))
    try:
        for e, data in entries:
            p = staging / e["path"]
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(data)
        (staging / MANIFEST).write_text(dump_manifest(manifest), encoding="utf-8")
        problems = check(staging, pins)
        if problems:
            raise CorpusError("staged corpus failed its own check:\n  " + "\n  ".join(problems[:20]))
        if dest.exists():
            shutil.rmtree(dest)
        os.replace(staging, dest)
    finally:
        if staging.exists():
            shutil.rmtree(staging, ignore_errors=True)
    return manifest


def catalogue_sources(data: bytes) -> list[str]:
    try:
        catalogue = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        raise CorpusError(f"libraries.json is not JSON: {e}")
    if not isinstance(catalogue, list):
        raise CorpusError("libraries.json is not a list")
    sources: list[str] = []
    for item in catalogue:
        s = item.get("source") if isinstance(item, dict) else None
        if not isinstance(s, str):
            raise CorpusError(f"libraries.json entry without a source: {item!r}")
        safe_relpath(s)
        if not s.endswith(LIBRARY_SUFFIX):
            raise CorpusError(f"libraries.json source {s!r} is not a {LIBRARY_SUFFIX} file")
        sources.append(s)
    if len(set(sources)) != len(sources):
        raise CorpusError("libraries.json lists a source twice")
    return sources


# -- reading ------------------------------------------------------------------


def read_fixture(fixtures: Path, rel: str) -> bytes:
    """Original bytes of a fixture, transparently un-gzipping stored libraries.

    rel is the logical path (e.g. libraries/<author>/<name>.excalidrawlib)."""
    p = Path(fixtures) / rel
    if p.is_file():
        return p.read_bytes()
    gz = p.with_name(p.name + ".gz")
    if gz.is_file():
        return gzip.decompress(gz.read_bytes())
    raise FileNotFoundError(str(p))


def load_manifest(fixtures: Path) -> dict:
    return json.loads((Path(fixtures) / MANIFEST).read_text(encoding="utf-8"))


# -- check --------------------------------------------------------------------

REQUIRED = ("path", "source", "origin", "origin_path", "bytes", "sha256")


def check(fixtures: Path, pins: Pins) -> list[str]:
    """Problems with fixtures/ against its manifest; [] means clean."""
    fixtures = Path(fixtures)
    mpath = fixtures / MANIFEST
    if not mpath.is_file():
        return [f"{MANIFEST} missing in {fixtures}"]
    text = mpath.read_text(encoding="utf-8")
    try:
        manifest = json.loads(text)
    except json.JSONDecodeError as e:
        return [f"{MANIFEST} is not JSON: {e}"]
    problems: list[str] = []
    if text != dump_manifest(manifest):
        problems.append(f"{MANIFEST} is not canonical (regenerate with scripts/fixtures/corpus.py sync)")
    if manifest.get("schema") != SCHEMA:
        problems.append(f"{MANIFEST} schema is {manifest.get('schema')!r}, expected {SCHEMA}")

    src = manifest.get("sources", {})
    pinned = {
        "upstream": (pins.upstream_repo, pins.upstream_commit),
        "libraries": (pins.libraries_repo, pins.libraries_commit),
    }
    for name, (repo, commit) in pinned.items():
        s = src.get(name, {})
        if s.get("commit") != commit:
            problems.append(f"{MANIFEST}: {name} commit {s.get('commit')!r} is not the pinned {commit}")
        if s.get("repo") != repo:
            problems.append(f"{MANIFEST}: {name} repo {s.get('repo')!r} is not {repo}")

    files = manifest.get("files", [])
    paths = [e.get("path") for e in files]
    seen: set = set()
    for p in paths:
        if p in seen:
            problems.append(f"{p}: duplicate manifest entry")
        seen.add(p)
    if paths != sorted(paths, key=str):
        problems.append(f"{MANIFEST}: files are not sorted by path")

    for e in files:
        path = e.get("path", "?")
        missing = [k for k in REQUIRED if not e.get(k) and e.get(k) != 0]
        if missing:
            problems.append(f"{path}: manifest entry lacks {', '.join(missing)}")
            continue
        source = e["source"]
        if source not in pinned:
            problems.append(f"{path}: unknown source {source!r}")
            continue
        repo, commit = pinned[source]
        gz = e.get("encoding") == "gzip"
        if e.get("encoding") not in (None, "gzip"):
            problems.append(f"{path}: unknown encoding {e.get('encoding')!r}")
        try:
            safe_relpath(e["origin_path"])
            expected_origin = f"{raw_base(repo)}/{commit}/{e['origin_path']}"
        except CorpusError as err:
            problems.append(f"{path}: {err}")
            continue
        if e["origin"] != expected_origin:
            problems.append(f"{path}: origin {e['origin']!r} is not {expected_origin}")
        if path != stored_path(source, e["origin_path"], gz):
            problems.append(f"{path}: path does not match origin_path {e['origin_path']}")
        f = fixtures / path
        if not f.is_file():
            problems.append(f"{path}: listed in manifest but missing on disk")
            continue
        data = f.read_bytes()
        if len(data) != e["bytes"]:
            problems.append(f"{path}: {len(data)} bytes on disk, manifest says {e['bytes']}")
        if sha256(data) != e["sha256"]:
            problems.append(f"{path}: sha256 {sha256(data)} on disk, manifest says {e['sha256']}")
        if gz:
            try:
                raw = gzip.decompress(data)
            except (OSError, EOFError) as err:
                problems.append(f"{path}: not gzip: {err}")
                continue
            if sha256(raw) != e.get("content_sha256"):
                problems.append(f"{path}: content_sha256 {sha256(raw)} after gunzip, manifest says {e.get('content_sha256')}")
            if len(raw) != e.get("content_bytes"):
                problems.append(f"{path}: {len(raw)} bytes after gunzip, manifest says {e.get('content_bytes')}")

    on_disk = sorted(
        p.relative_to(fixtures).as_posix()
        for p in fixtures.rglob("*")
        if (p.is_file() or p.is_symlink()) and p != mpath
    )
    for p in on_disk:
        if p not in seen:
            problems.append(f"{p}: on disk but not in manifest")

    problems += _check_catalogue(fixtures, files)
    return problems


def _check_catalogue(fixtures: Path, files: list[dict]) -> list[str]:
    cat = fixtures / "libraries" / "libraries.json"
    libs = {
        e["origin_path"][len(LIBRARY_DIR) + 1:]
        for e in files
        if e.get("source") == "libraries" and str(e.get("origin_path", "")).startswith(LIBRARY_DIR + "/")
    }
    if not cat.is_file():
        return ["libraries/libraries.json: catalogue missing"] if libs else []
    try:
        sources = catalogue_sources(cat.read_bytes())
    except CorpusError as err:
        return [f"libraries/libraries.json: {err}"]
    out = [f"libraries/{s}: in catalogue but not in manifest" for s in sources if s not in libs]
    out += [f"libraries/{s}: in manifest but not in catalogue" for s in sorted(libs - set(sources))]
    return out


# -- verify against origins ---------------------------------------------------


def verify_upstream(fixtures: Path, upstream_dir: Path) -> list[str]:
    """Upstream copies equal the checkout, and the checkout has nothing new."""
    manifest = load_manifest(fixtures)
    entries = {e["origin_path"]: e for e in manifest["files"] if e["source"] == "upstream"}
    problems: list[str] = []
    try:
        wanted = upstream_file_list(Path(upstream_dir))
    except CorpusError as err:
        return [str(err)]
    for rel in wanted:
        if rel not in entries:
            problems.append(f"upstream/{rel}: in the upstream checkout but not in the corpus (run sync)")
    for rel, e in sorted(entries.items()):
        f = Path(upstream_dir) / rel
        if not f.is_file():
            problems.append(f"{e['path']}: {rel} missing from the upstream checkout")
            continue
        if sha256(f.read_bytes()) != e["sha256"]:
            problems.append(f"{e['path']}: differs from {rel} in the upstream checkout")
    return problems


def verify_remote(
    fixtures: Path,
    raw_bases: dict[str, str] | None = None,
    fetcher: Callable[[str], bytes] = fetch,
) -> list[str]:
    """Every file's original bytes equal its origin at the pinned commit."""
    manifest = load_manifest(fixtures)
    bases = {name: raw_base(s["repo"]) for name, s in manifest["sources"].items()}
    bases.update(raw_bases or {})
    commits = {name: s["commit"] for name, s in manifest["sources"].items()}
    urls = {
        e["path"]: f"{bases[e['source']].rstrip('/')}/{commits[e['source']]}/{e['origin_path']}"
        for e in manifest["files"]
    }
    got = fetch_all(list(urls.values()), fetcher)
    problems: list[str] = []
    for e in manifest["files"]:
        url = urls[e["path"]]
        res = got[url]
        if isinstance(res, Exception):
            problems.append(f"{e['path']}: fetch {url} failed: {res}")
            continue
        want = e.get("content_sha256") or e["sha256"]
        if sha256(res) != want:
            problems.append(f"{e['path']}: {url} has sha256 {sha256(res)}, corpus has {want}")
    return problems


# -- CLI ----------------------------------------------------------------------


def default_upstream_dir() -> Path:
    out = subprocess.run(["bash", str(CHECKOUT), "--print-dir"], capture_output=True, text=True, check=True)
    return Path(out.stdout.strip())


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("command", choices=["check", "sync", "verify-upstream", "verify-remote"])
    ap.add_argument("--fixtures", type=Path, default=FIXTURES)
    ap.add_argument("--upstream-dir", type=Path, help="default: scripts/upstream/checkout.sh --print-dir")
    ap.add_argument("--upstream-commit", help="override extra.upstream_commit (tests)")
    ap.add_argument("--libraries-commit", help="override extra.libraries_commit (tests)")
    ap.add_argument("--libraries-raw-base", help="override the raw URL base for excalidraw-libraries")
    args = ap.parse_args(argv)

    try:
        pins = config_pins()
    except CorpusError as err:
        print(f"fixture corpus: {err}", file=sys.stderr)
        return 2
    pins = pins._replace(
        upstream_commit=args.upstream_commit or pins.upstream_commit,
        libraries_commit=args.libraries_commit or pins.libraries_commit,
    )

    def report(problems: list[str], ok: str) -> int:
        if problems:
            print(f"fixture corpus FAILED ({args.command}): {len(problems)} problem(s)", file=sys.stderr)
            for p in problems:
                print("  " + p, file=sys.stderr)
            return 1
        print(ok)
        return 0

    try:
        if args.command == "check":
            problems = check(args.fixtures, pins)
            n = len(load_manifest(args.fixtures)["files"]) if not problems else 0
            return report(problems, f"fixture corpus OK: {n} files match {args.fixtures / MANIFEST}")
        if args.command == "verify-upstream":
            up = args.upstream_dir or default_upstream_dir()
            return report(verify_upstream(args.fixtures, up), f"fixture corpus OK: upstream copies equal {up}")
        if args.command == "verify-remote":
            bases = {"libraries": args.libraries_raw_base} if args.libraries_raw_base else None
            return report(verify_remote(args.fixtures, bases), "fixture corpus OK: every file equals its origin")
        # sync
        if args.upstream_dir is None:
            subprocess.run(["bash", str(CHECKOUT), "--verify"], check=True)
            up = default_upstream_dir()
        else:
            up = args.upstream_dir
        m = sync(args.fixtures, up, pins, libraries_raw_base=args.libraries_raw_base)
        print(f"fixture corpus synced: {len(m['files'])} files in {args.fixtures}")
        return 0
    except (CorpusError, subprocess.CalledProcessError, OSError, json.JSONDecodeError) as err:
        print(f"fixture corpus: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
