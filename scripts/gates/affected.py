#!/usr/bin/env python3
"""Which workspace crates a pull request can affect, as cargo arguments.

Owner decision 2026-09-29 (task ex-010): pull-request CI runs `cargo test` on
the affected crates only; every push to main still runs the whole workspace.

Usage:
  scripts/gates/affected.py <base-ref> [--metadata FILE]

Prints one line to stdout and exits 0:
  --workspace         a global file changed (see GLOBAL), or a file that is
                      neither in a crate, nor read by one, nor docs/site-only
  -p a -p b ...       the crates that own a changed file or read it by path,
                      plus every workspace crate that depends on one whose
                      library changed (normal, dev or build dependency),
                      transitively
  (nothing)           only docs/site files that no crate reads changed

The changed files are `git diff --name-only <base-ref>...HEAD` (from the
merge base, so commits that reached the base since the branch forked are not
counted). Crates come from `cargo metadata --no-deps --format-version 1`
(`--metadata FILE` reads that JSON from a file instead; the tests use it).

A crate "reads a file by path" when one of its .rs files has a string literal
ending in the file's repository path, or in the path from the crates
directory, or in one of their parent directories with at least two
components: `include_str!("../../excali-core/tests/fixtures/library.json")`,
`repo().join("site/config.toml")`, `.join("../excali-text/assets/fonts")`.

A crate's library changed when the file is in the crate but not under its
tests/ or benches/, or when the literal naming the file is in its src/ or
build.rs; only then are its dependents added. A file under tests/ changes the
owning crate's tests (and those of crates that read it), nothing else.
Exit status 2 is a usage or git/cargo error.
"""
from __future__ import annotations

import fnmatch
import json
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

# Any change here can affect every crate's tests.
GLOBAL = (
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "clippy.toml",
    ".github/workflows/*",
    ".cargo/*",
    "goldens/*",
    "fixtures/*",
    "tests/fixtures/*",
    "scripts/gates/*",
)

# Outside every crate and read by no crate: no Rust test can see these.
DOCS = (
    "site/*",
    "plan/*",
    ".beads/*",
    "tests/site/*",
    "*.md",
    "LICENSE*",
    ".gitignore",
    ".gitattributes",
)


def matches(path: str, patterns: tuple[str, ...]) -> bool:
    # fnmatch's * crosses "/", so "site/*" is the whole tree.
    return any(fnmatch.fnmatchcase(path, p) for p in patterns)


def is_global(path: str) -> bool:
    return matches(path, GLOBAL)


def is_docs(path: str) -> bool:
    return matches(path, DOCS)


class Crate:
    def __init__(self, name: str, dir: str, deps: set[str]):
        self.name = name
        self.dir = dir  # repository-relative, posix, no trailing slash
        self.deps = deps  # workspace crate names this one depends on

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return f"Crate({self.name!r}, {self.dir!r})"


def crates_from_metadata(meta: dict) -> list[Crate]:
    root = PurePosixPath(Path(meta["workspace_root"]).as_posix())
    members = set(meta.get("workspace_members") or [p["id"] for p in meta["packages"]])
    pkgs = [p for p in meta["packages"] if p["id"] in members]
    names = {p["name"] for p in pkgs}
    out = []
    for p in pkgs:
        d = PurePosixPath(Path(p["manifest_path"]).as_posix()).parent
        rel = d.relative_to(root).as_posix()
        deps = {x["name"] for x in p.get("dependencies", []) if x.get("path") and x["name"] in names}
        out.append(Crate(p["name"], "" if rel == "." else rel, deps))
    return out


def owner(path: str, crates: list[Crate]) -> Crate | None:
    best = None
    for c in crates:
        if c.dir and (path == c.dir or path.startswith(c.dir + "/")):
            if best is None or len(c.dir) > len(best.dir):
                best = c
    return best


def needles(path: str, crate: Crate | None) -> list[str]:
    """The literal path tails that name `path` or a parent directory of it."""
    forms = [path]
    if crate is not None and "/" in crate.dir:
        # "crates/excali-core/tests/x.json" -> "excali-core/tests/x.json"
        forms.append(path[len(crate.dir.rsplit("/", 1)[0]) + 1:])
    out: list[str] = []
    for f in forms:
        parts = f.split("/")
        for n in range(len(parts), 1, -1):
            s = "/".join(parts[:n])
            if s not in out:
                out.append(s)
    return out


def literal_pattern(needle: str) -> re.Pattern[str]:
    # The needle ends a string literal (optionally with a trailing slash) and
    # starts at a path boundary: `"site/config.toml"`, `"../../goldens/x"`.
    return re.compile(r'(?:^|["/])' + re.escape(needle) + r'/?"')


# Per crate: (library sources: src/ and build.rs; every other .rs file).
Sources = dict[str, tuple[str, str]]


def readers(path: str, crate: Crate | None, sources: Sources) -> tuple[set[str], set[str]]:
    """Crates whose .rs sources name `path` by a literal: (all, in library)."""
    pats = [literal_pattern(n) for n in needles(path, crate)]
    found = lambda text: any(p.search(text) for p in pats)  # noqa: E731
    lib = {n for n, (l, _) in sources.items() if found(l)}
    return lib | {n for n, (_, t) in sources.items() if found(t)}, lib


def is_library_file(rel: str) -> bool:
    """`rel` is relative to the crate directory."""
    return rel == "build.rs" or rel.startswith("src/")


def in_tests(path: str, crate: Crate) -> bool:
    rel = path[len(crate.dir) + 1:] if crate.dir else path
    return rel.startswith(("tests/", "benches/"))


def load_sources(root: Path, crates: list[Crate]) -> Sources:
    out = {}
    for c in crates:
        base = root / c.dir if c.dir else root
        lib, other = [], []
        for f in sorted(base.rglob("*.rs")):
            rel = f.relative_to(base).as_posix()
            if rel.split("/", 1)[0] == "target":
                continue
            try:
                text = f.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            (lib if is_library_file(rel) else other).append(text)
        out[c.name] = ("\n".join(lib), "\n".join(other))
    return out


def with_dependents(names: set[str], crates: list[Crate]) -> set[str]:
    rdeps: dict[str, set[str]] = {c.name: set() for c in crates}
    for c in crates:
        for d in c.deps:
            rdeps.setdefault(d, set()).add(c.name)
    out = set(names)
    todo = list(names)
    while todo:
        for r in rdeps.get(todo.pop(), ()):
            if r not in out:
                out.add(r)
                todo.append(r)
    return out


def affected(changed: list[str], crates: list[Crate], sources: Sources) -> str | set[str]:
    """"workspace", or the set of affected crate names (possibly empty)."""
    hit: set[str] = set()
    libs: set[str] = set()
    for path in changed:
        if is_global(path):
            return "workspace"
        c = owner(path, crates)
        if c is not None:
            hit.add(c.name)
            if not in_tests(path, c):
                libs.add(c.name)
        refs, lib_refs = readers(path, c, sources)
        hit |= refs
        libs |= lib_refs
        if c is None and not refs and not is_docs(path):
            return "workspace"
    return hit | with_dependents(libs, crates)


def cargo_args(result: str | set[str]) -> str:
    if result == "workspace":
        return "--workspace"
    return " ".join(f"-p {n}" for n in sorted(result))


def git(root: Path, *args: str) -> str:
    return subprocess.run(["git", *args], cwd=root, capture_output=True, text=True, check=True).stdout


def main(argv: list[str]) -> int:
    args = list(argv)
    meta_file = None
    if "--metadata" in args:
        i = args.index("--metadata")
        if i + 1 >= len(args):
            print(__doc__, file=sys.stderr)
            return 2
        meta_file = args[i + 1]
        del args[i : i + 2]
    if len(args) != 1 or args[0].startswith("-"):
        print(__doc__, file=sys.stderr)
        return 2
    base = args[0]
    try:
        root = Path(git(Path.cwd(), "rev-parse", "--show-toplevel").strip())
        changed = [l for l in git(root, "diff", "--name-only", f"{base}...HEAD").splitlines() if l]
        if meta_file is not None:
            meta = json.loads(Path(meta_file).read_text(encoding="utf-8"))
        else:
            meta = json.loads(
                subprocess.run(
                    ["cargo", "metadata", "--no-deps", "--format-version", "1"],
                    cwd=root, capture_output=True, text=True, check=True,
                ).stdout
            )
    except (subprocess.CalledProcessError, OSError, ValueError) as e:
        detail = getattr(e, "stderr", "") or str(e)
        print(f"affected.py: {detail.strip()}", file=sys.stderr)
        return 2
    crates = crates_from_metadata(meta)
    result = affected(changed, crates, load_sources(root, crates))
    line = cargo_args(result)
    for path in changed:
        print(f"changed: {path}", file=sys.stderr)
    print(f"cargo args: {line or '(none)'}", file=sys.stderr)
    if line:
        print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
