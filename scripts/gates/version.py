#!/usr/bin/env python3
"""Calendar-version gate (ADR-009).

The workspace version is YY.M.BUILD: two-digit year, month without a leading
zero, and a build counter starting at 1 within that month. The release tag is
the version with a leading "v" (v26.9.1). Every crate inherits the version
from [workspace.package], so one number describes a release.

Subcommands:
  check                 the workspace version is calendar, every crate under
                        crates/ inherits it and sets publish = false (the
                        registry hold on ex-801), Cargo.lock agrees (exit 0
                        clean, 1 violations, 2 usage error)
  print                 print the workspace version
  tag                   print the release tag for the workspace version
  next [YYYY-MM]        print the version the next release gets in that month
                        (default: the current UTC month), given the workspace
                        version as the latest release
"""
from __future__ import annotations

import datetime
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

CALVER = re.compile(r"^(\d{2})\.([1-9]|1[0-2])\.([1-9]\d*)$")
SCHEME = "YY.M.BUILD"


def parse(version: str) -> tuple[int, int, int]:
    """(year % 100, month, build) of a calendar version, or ValueError."""
    m = CALVER.match(version)
    if not m:
        raise ValueError(f"'{version}' is not a calendar version {SCHEME} (e.g. 26.9.1)")
    return int(m.group(1)), int(m.group(2)), int(m.group(3))


def tag(version: str) -> str:
    parse(version)
    return f"v{version}"


def next_version(previous: str | None, year: int, month: int) -> str:
    """The version for a release made in year-month after `previous`."""
    if not (2000 <= year <= 2099 and 1 <= month <= 12):
        raise ValueError(f"{year}-{month:02d} is not a month this scheme can name")
    yy = year % 100
    if previous is None:
        return f"{yy}.{month}.1"
    py, pm, pb = parse(previous)
    if (yy, month) == (py, pm):
        return f"{yy}.{month}.{pb + 1}"
    if (yy, month) < (py, pm):
        raise ValueError(f"{year}-{month:02d} is before the last release {previous}")
    return f"{yy}.{month}.1"


def _section(toml: str, name: str) -> str | None:
    """Body of a [name] table in a TOML file (flat keys only)."""
    out: list[str] = []
    inside = False
    found = False
    for line in toml.splitlines():
        s = line.strip()
        if s.startswith("["):
            inside = s == f"[{name}]"
            found = found or inside
            continue
        if inside:
            out.append(s)
    return "\n".join(out) if found else None


def workspace_version(root_manifest: str) -> str | None:
    body = _section(root_manifest, "workspace.package")
    if body is None:
        return None
    m = re.search(r'^version\s*=\s*"([^"]*)"', body, re.M)
    return m.group(1) if m else None


def _inherits_version(member_manifest: str) -> bool:
    body = _section(member_manifest, "package") or ""
    return bool(re.search(r"^version\.workspace\s*=\s*true\b", body, re.M)) or bool(
        re.search(r"^version\s*=\s*\{\s*workspace\s*=\s*true\s*\}", body, re.M)
    )


def _lock_versions(lock: str) -> dict[str, str]:
    out: dict[str, str] = {}
    for block in lock.split("[[package]]")[1:]:
        name = re.search(r'^name\s*=\s*"([^"]*)"', block, re.M)
        ver = re.search(r'^version\s*=\s*"([^"]*)"', block, re.M)
        if name and ver:
            out[name.group(1)] = ver.group(1)
    return out


def check(root_manifest: str, members: dict[str, str], lock: str) -> list[str]:
    """Problems with the workspace version; members maps crate name -> manifest."""
    problems: list[str] = []
    v = workspace_version(root_manifest)
    if v is None:
        return ["Cargo.toml: no version in [workspace.package]"]
    try:
        parse(v)
    except ValueError as e:
        problems.append(f"Cargo.toml: workspace version {e}")
    locked = _lock_versions(lock)
    for name in sorted(members):
        if not _inherits_version(members[name]):
            problems.append(f"crates/{name}/Cargo.toml: must use version.workspace = true")
        if not re.search(r"^publish\s*=\s*false\b", _section(members[name], "package") or "", re.M):
            problems.append(
                f"crates/{name}/Cargo.toml: must set publish = false while the registry hold (ex-801, ADR-009) stands"
            )
        if name in locked and locked[name] != v:
            problems.append(f"Cargo.lock: {name} is {locked[name]}, workspace is {v} (run cargo update -w)")
        elif name not in locked:
            problems.append(f"Cargo.lock: no entry for {name} (run cargo update -w)")
    return problems


def _read_repo() -> tuple[str, dict[str, str], str]:
    root = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    members: dict[str, str] = {}
    for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        text = manifest.read_text(encoding="utf-8")
        m = re.search(r'^name\s*=\s*"([^"]*)"', _section(text, "package") or "", re.M)
        members[m.group(1) if m else manifest.parent.name] = text
    lock_path = ROOT / "Cargo.lock"
    lock = lock_path.read_text(encoding="utf-8") if lock_path.exists() else ""
    return root, members, lock


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__, file=sys.stderr)
        return 2
    cmd, rest = argv[0], argv[1:]
    root, members, lock = _read_repo()
    v = workspace_version(root)
    if cmd == "check" and not rest:
        problems = check(root, members, lock)
        for p in problems:
            print(f"version gate: {p}", file=sys.stderr)
        if problems:
            return 1
        print(f"version gate OK: {v} (tag {tag(v)}), {len(members)} crates")
        return 0
    if v is None:
        print("version gate: no version in [workspace.package]", file=sys.stderr)
        return 1
    try:
        if cmd == "print" and not rest:
            parse(v)
            print(v)
            return 0
        if cmd == "tag" and not rest:
            print(tag(v))
            return 0
        if cmd == "next" and len(rest) <= 1:
            if rest:
                m = re.match(r"^(\d{4})-(\d{2})$", rest[0])
                if not m:
                    print("version gate: month must be YYYY-MM", file=sys.stderr)
                    return 2
                year, month = int(m.group(1)), int(m.group(2))
            else:
                today = datetime.datetime.now(datetime.timezone.utc)
                year, month = today.year, today.month
            print(next_version(v, year, month))
            return 0
    except ValueError as e:
        print(f"version gate: {e}", file=sys.stderr)
        return 1
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
