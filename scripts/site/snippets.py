"""Hold a page's code blocks to the files they were copied from (ex-607).

Every fenced block on a checked page is preceded by a marker naming its
source; a block without one is an error, so nothing copy-paste on the page
goes unverified:

    <!-- snippet: path/to/file -->
        The block's lines, stripped of indentation and with blank lines
        dropped, appear consecutively in the file (also stripped, blanks
        dropped). A line that is only `// …`, `# …` or `<!-- … -->` elides:
        the chunks around it must each appear, in order.

    <!-- snippet: path/to/file.json -->
    <!-- snippet: path/to/file.json#/json/pointer -->
        The block parses as JSON equal to the file (or the value at the
        pointer, RFC 6901). A block of the form `"name": value` is the member
        at the pointer, `name` being its last segment.

    <!-- snippet: web-csp path/to/tauri.conf.json#/app/security/csp -->
        The block is `Content-Security-Policy: <policy>`, the Tauri CSP at
        the pointer written as a header without Tauri's IPC sources
        (`web_csp`).

Paths are relative to the repository root.

    python3 scripts/site/snippets.py check
        checks site/content/architecture/integration.md
    python3 scripts/site/snippets.py check PAGE ...
        checks the pages given

Tests: scripts/site/test_snippets.py
"""
import json
import pathlib
import re
import sys
from typing import NamedTuple, Optional

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
PAGES = ["site/content/architecture/integration.md"]

MARKER = re.compile(r"^<!-- snippet: (.+?) -->$")
FENCE = re.compile(r"^(`{3,})(\S*)\s*$")
ELISION = {"// …", "# …", "<!-- … -->"}
# The sources a Tauri CSP needs for its IPC, which a browser page has not:
# `ipc://localhost/<cmd>` (macOS, iOS, Linux) and `http://ipc.localhost/<cmd>`
# (Windows, Android), tauri 2.12.0 scripts/core.js `convertFileSrc`.
TAURI_ONLY = {"ipc:", "http://ipc.localhost"}


class Block(NamedTuple):
    line: int  # 1-based line of the opening fence
    marker: Optional[str]
    lang: Optional[str]
    code: str


def blocks(text):
    """The fenced blocks of `text`, each with the marker before it (if the
    last non-blank line before the fence is one). A marker that is not
    followed by a fence is returned as a block with `lang` None."""
    lines = text.split("\n")
    out = []
    pending = None  # (line, marker)
    i = 0
    while i < len(lines):
        line = lines[i]
        m = MARKER.match(line.strip())
        f = FENCE.match(line)
        if m:
            if pending:
                out.append(Block(pending[0], pending[1], None, ""))
            pending = (i + 1, m.group(1))
        elif f:
            end = i + 1
            while end < len(lines) and lines[end].strip() != f.group(1):
                end += 1
            code = "".join(f"{x}\n" for x in lines[i + 1 : end])
            out.append(Block(i + 1, pending[1] if pending else None, f.group(2), code))
            pending = None
            i = end
        elif line.strip() and pending:
            out.append(Block(pending[0], pending[1], None, ""))
            pending = None
        i += 1
    if pending:
        out.append(Block(pending[0], pending[1], None, ""))
    return out


def _stripped(text):
    return [x.strip() for x in text.split("\n") if x.strip()]


def _find(haystack, needle, start):
    for i in range(start, len(haystack) - len(needle) + 1):
        if haystack[i : i + len(needle)] == needle:
            return i
    return -1


def _excerpt(code, text):
    """None if `code` is an excerpt of `text`, else the first chunk that is
    not found."""
    have = _stripped(text)
    chunks = [[]]
    for line in _stripped(code):
        if line in ELISION:
            chunks.append([])
        else:
            chunks[-1].append(line)
    at = 0
    for chunk in chunks:
        if not chunk:
            continue
        i = _find(have, chunk, at)
        if i < 0:
            return chunk
        at = i + len(chunk)
    return None


def _pointer(value, pointer):
    if pointer in ("", "/"):
        return value
    for raw in pointer.lstrip("/").split("/"):
        key = raw.replace("~1", "/").replace("~0", "~")
        if isinstance(value, list):
            value = value[int(key)]
        else:
            value = value[key]
    return value


def web_csp(csp):
    """A Tauri `app.security.csp` object as a browser header value: the same
    directives, in order, without the IPC sources."""
    out = []
    for name, sources in csp.items():
        kept = [s for s in sources.split() if s not in TAURI_ONLY]
        if kept:
            out.append(" ".join([name, *kept]))
    return "; ".join(out)


class Missing(Exception):
    """A marker names a file that does not exist."""


def _source(root, spec):
    path, _, pointer = spec.partition("#")
    file = root / path
    if not file.is_file():
        raise Missing(f"{path} does not exist")
    return path, pointer, file.read_text()


def check_block(block, root):
    """The errors of one block (an empty list when it matches)."""
    where = f"line {block.line}"
    if block.marker is None:
        return [f"{where}: code block has no snippet marker"]
    if block.lang is None:
        return [f"{where}: snippet marker is not followed by a code block"]
    kind, _, rest = block.marker.partition(" ")
    try:
        if kind == "web-csp":
            path, pointer, text = _source(root, rest)
            want = f"Content-Security-Policy: {web_csp(_pointer(json.loads(text), pointer))}"
            got = block.code.strip()
            if got != want:
                return [f"{where}: {path}#{pointer} as a header is\n  {want}\nnot\n  {got}"]
            return []
        path, pointer, text = _source(root, block.marker)
        if path.endswith(".json") and block.lang == "json":
            value = _pointer(json.loads(text), pointer)
            code = block.code.strip()
            if code.startswith('"'):
                name = pointer.rstrip("/").split("/")[-1]
                got = json.loads("{" + code + "}")
                value = {name: value}
            else:
                got = json.loads(code)
            if got != value:
                return [f"{where}: block differs from {path}#{pointer}"]
            return []
        missing = _excerpt(block.code, text)
        if missing is not None:
            shown = "\n  ".join(missing)
            return [f"{where}: not in {path}:\n  {shown}"]
        return []
    except Missing as e:
        return [f"{where}: {e}"]
    except (KeyError, IndexError, ValueError) as e:
        return [f"{where}: {block.marker}: {type(e).__name__} {e}"]


def check_text(text, root=ROOT):
    return [e for b in blocks(text) for e in check_block(b, root)]


def check_page(path, root=ROOT):
    path = pathlib.Path(path)
    return [f"{path}: {e}" for e in check_text(path.read_text(), root)]


def main(argv):
    if not argv or argv[0] != "check":
        print(__doc__.strip().split("\n\n")[-2], file=sys.stderr)
        return 2
    pages = argv[1:] or PAGES
    errors = [e for p in pages for e in check_page(ROOT / p)]
    for e in errors:
        print(e, file=sys.stderr)
    if not errors:
        print(f"snippets: {', '.join(pages)} ok")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
