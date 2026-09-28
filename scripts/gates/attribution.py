#!/usr/bin/env python3
"""Deterministic authorship gate.

Dr. Hutson has full responsibility for everything in this repository. Tooling
is a tool, not an author, so nothing tracked here may carry a tool's
attribution, a session link, or an invisible watermark. This script is the
single implementation behind every enforcement point:

  .githooks/pre-commit      -> attribution.py files --staged
  .githooks/commit-msg      -> attribution.py message <file>   (strips, then verifies)
  .githooks/pre-push        -> attribution.py history <range>
  .github/workflows/gates.yml -> files --all  and  history <base>..HEAD
  scripts/gates/test_attribution.py -> self-test: planted violations in a
                                 scratch repo must fail (run in gates.yml)

Exit status 0 = clean, 1 = violation(s) found, 2 = usage error.

Rules (all case-insensitive):
  R1 attribution trailer   Co-Authored-By / Claude-Session / Signed-off-by naming a tool vendor
  R2 tool footer           "Generated with|by [tool]" footers, tool session URLs
  R3 vendor identity       vendor no-reply e-mail addresses, vendor domains
  R4 vendor mention        any bare mention of the vendor/tool names outside the allowlist
  R5 invisible watermark   zero-width / bidi / tag / private-use code points in text files

R4 is deliberately blunt: a mention that is legitimately needed is opted in per
line with the marker `gate:allow-mention` or per file in attribution-allow.txt.
Everything else fails closed.
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True).stdout.strip())
ALLOW_FILE = ROOT / "scripts" / "gates" / "attribution-allow.txt"
ALLOW_MARK = "gate:allow-mention"

# The vendor/tool names are spelled with character classes so this file does
# not itself trip R4 when scanned.
_V = r"(?:c[l]aude|an[t]hropic)"
RULES = [
    ("R1", re.compile(r"^\s*(co-authored-by|claude-session|signed-off-by)\s*:.*" + _V, re.I | re.M)),
    ("R1", re.compile(r"^\s*c[l]aude-session\s*:", re.I | re.M)),
    ("R2", re.compile(r"generated\s+(with|by)\s+\[?\s*" + _V, re.I)),
    ("R2", re.compile(r"c[l]aude\.(ai|com)/(code|claude-code)", re.I)),
    ("R3", re.compile(r"noreply@an[t]hropic\.com", re.I)),
    ("R3", re.compile(r"@an[t]hropic\.com", re.I)),
    ("R4", re.compile(r"\b" + _V + r"\b", re.I)),
]

# Code points that carry no ink. BOM (FEFF) is allowed only at byte offset 0.
# Built from integers so this source file stays pure ASCII and passes itself.
_INVISIBLE_RANGES = [
    (0x00AD, 0x00AD), (0x034F, 0x034F), (0x061C, 0x061C), (0x115F, 0x1160),
    (0x180E, 0x180E), (0x200B, 0x200F), (0x2028, 0x202E), (0x2060, 0x2064),
    (0x206A, 0x206F), (0x3164, 0x3164), (0xFE00, 0xFE0F), (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0), (0xE0000, 0xE007F),
]
INVISIBLE = re.compile("[" + "".join(f"{chr(a)}-{chr(b)}" for a, b in _INVISIBLE_RANGES) + "]")
BOM = chr(0xFEFF)

BINARY_EXT = {
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".woff", ".woff2", ".ttf", ".otf",
    ".pdf", ".zip", ".gz", ".tar", ".wasm", ".dmg", ".lock", ".sqlite", ".sqlite3",
}


def allowlist() -> set[str]:
    if not ALLOW_FILE.exists():
        return set()
    out = set()
    for line in ALLOW_FILE.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            out.add(line)
    return out


def is_binary(path: Path, data: bytes) -> bool:
    return path.suffix.lower() in BINARY_EXT or b"\x00" in data[:8000]


def scan_text(text: str, label: str, allow_r4: bool) -> list[str]:
    problems: list[str] = []
    lines = text.splitlines()
    for rule, rx in RULES:
        if rule == "R4" and allow_r4:
            continue
        for m in rx.finditer(text):
            lineno = text.count("\n", 0, m.start()) + 1
            line = lines[lineno - 1] if lineno - 1 < len(lines) else ""
            if rule == "R4" and ALLOW_MARK in line:
                continue
            problems.append(f"{label}:{lineno}: {rule} {m.group(0)!r}")
    for m in INVISIBLE.finditer(text):
        if m.start() == 0 and m.group(0) == BOM:
            continue
        lineno = text.count("\n", 0, m.start()) + 1
        problems.append(f"{label}:{lineno}: R5 invisible code point U+{ord(m.group(0)):04X}")
    return problems


def scan_file(rel: str, data: bytes, allowed: set[str]) -> list[str]:
    path = ROOT / rel
    if is_binary(path, data):
        return []
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return [f"{rel}: not valid UTF-8 (text files must be UTF-8 so the gate can read them)"]
    # The gate and its allowlist name the patterns they hunt; they are exempt from R4 only.
    allow_r4 = rel in allowed or rel == "scripts/gates/attribution.py" or rel == "scripts/gates/attribution-allow.txt"
    return scan_text(text, rel, allow_r4)


def git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout


def cmd_files(argv: list[str]) -> int:
    allowed = allowlist()
    problems: list[str] = []
    if argv[:1] == ["--staged"]:
        names = [n for n in git("diff", "--cached", "--name-only", "--diff-filter=ACMR", "-z").split("\0") if n]
        for rel in names:
            data = subprocess.run(["git", "show", f":{rel}"], cwd=ROOT, capture_output=True, check=True).stdout
            problems += scan_file(rel, data, allowed)
    elif argv[:1] == ["--all"] or not argv:
        names = [n for n in git("ls-files", "-z").split("\0") if n]
        for rel in names:
            p = ROOT / rel
            if p.is_file():
                problems += scan_file(rel, p.read_bytes(), allowed)
    else:
        for rel in argv:
            p = ROOT / rel
            if p.is_file():
                problems += scan_file(rel, p.read_bytes(), allowed)
    return report(problems, "files")


def strip_message(text: str) -> str:
    kept = []
    for line in text.splitlines():
        if re.match(r"^\s*(co-authored-by|claude-session)\s*:.*" + _V, line, re.I) or re.match(r"^\s*c[l]aude-session\s*:", line, re.I):
            continue
        if re.search(r"generated\s+(with|by)\s+\[?\s*" + _V, line, re.I):
            continue
        if re.search(r"c[l]aude\.(ai|com)/(code|claude-code)", line, re.I):
            continue
        kept.append(line)
    # collapse trailing blank lines left behind by stripped trailers
    while kept and not kept[-1].strip():
        kept.pop()
    return "\n".join(kept) + "\n"


def cmd_message(argv: list[str]) -> int:
    if len(argv) != 1:
        print("usage: attribution.py message <commit-msg-file>", file=sys.stderr)
        return 2
    path = Path(argv[0])
    original = path.read_text(encoding="utf-8")
    stripped = strip_message(original)
    if stripped != original:
        path.write_text(stripped, encoding="utf-8")
        print("attribution gate: stripped tool attribution lines from the commit message", file=sys.stderr)
    problems = scan_text(stripped, "commit-message", allow_r4=False)
    return report(problems, "message")


def cmd_history(argv: list[str]) -> int:
    rng = argv[0] if argv else "@{upstream}..HEAD"
    fmt = "%H%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%B%x1e"
    try:
        raw = git("log", f"--format={fmt}", rng)
    except subprocess.CalledProcessError as e:
        print(f"attribution gate: cannot read range {rng}: {e.stderr.strip()}", file=sys.stderr)
        return 2
    problems: list[str] = []
    for rec in raw.split("\x1e"):
        if not rec.strip():
            continue
        sha, an, ae, cn, ce, body = rec.lstrip("\n").split("\x1f", 5)
        short = sha[:10]
        for who, val in (("author name", an), ("author email", ae), ("committer name", cn), ("committer email", ce)):
            problems += [f"{short} {who}: {p}" for p in scan_text(val, who, allow_r4=False)]
        problems += [f"{short} {p}" for p in scan_text(body, "message", allow_r4=False)]
    return report(problems, f"history {rng}")


def report(problems: list[str], what: str) -> int:
    if problems:
        print(f"attribution gate FAILED ({what}): {len(problems)} violation(s)", file=sys.stderr)
        for p in problems:
            print("  " + p, file=sys.stderr)
        print("Fix: remove the attribution/watermark, or opt a needed mention in with "
              f"'{ALLOW_MARK}' on that line or a path in {ALLOW_FILE.relative_to(ROOT)}.", file=sys.stderr)
        return 1
    print(f"attribution gate OK ({what})")
    return 0


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 2
    cmd, rest = argv[0], argv[1:]
    if cmd == "files":
        return cmd_files(rest)
    if cmd == "message":
        return cmd_message(rest)
    if cmd == "history":
        return cmd_history(rest)
    print(f"unknown subcommand {cmd!r}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
