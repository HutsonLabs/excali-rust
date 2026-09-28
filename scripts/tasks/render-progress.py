#!/usr/bin/env python3
"""Render .beads/issues.jsonl into site/content/plan/progress.md.

Reads only the tracked JSONL, so it runs in CI without bd or a database.
"""
from __future__ import annotations

import json
import subprocess
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True).stdout.strip())
SRC = ROOT / ".beads" / "issues.jsonl"
OUT = ROOT / "site" / "content" / "plan" / "progress.md"

ICON = {"open": "open", "in_progress": "in progress", "blocked": "blocked", "closed": "closed", "deferred": "deferred"}


def load() -> list[dict]:
    rows = []
    if SRC.exists():
        for line in SRC.read_text(encoding="utf-8").splitlines():
            try:
                o = json.loads(line)
            except json.JSONDecodeError:
                continue
            if o.get("_type", "issue") == "issue":
                rows.append(o)
    return rows


def main() -> int:
    rows = load()
    by_id = {r["id"]: r for r in rows}
    parent: dict[str, str] = {}
    blockers: dict[str, list[str]] = defaultdict(list)
    for r in rows:
        for d in r.get("dependencies", []) or []:
            if d.get("type") == "parent-child":
                parent[r["id"]] = d["depends_on_id"]
            elif d.get("type") == "blocks":
                blockers[r["id"]].append(d["depends_on_id"])
    epics = sorted((r for r in rows if r.get("issue_type") == "epic"), key=lambda r: r["id"])
    counts = defaultdict(int)
    for r in rows:
        counts[r.get("status", "open")] += 1
    total = len(rows)
    closed = counts.get("closed", 0)
    pct = round(100 * closed / total) if total else 0
    lines = [
        "+++",
        'title = "Progress"',
        'description = "The task graph as tracked in beads, rendered from .beads/issues.jsonl at build time."',
        "weight = 4",
        "+++",
        "",
        f"Generated {datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M UTC')} from `.beads/issues.jsonl`. Edit `plan/tasks.json` and run `scripts/tasks/seed.py` to change the graph; claim and close work with `bd`.",
        "",
        f"**{total} issues** · {closed} closed ({pct}%) · {counts.get('in_progress', 0)} in progress · {counts.get('blocked', 0)} blocked · {counts.get('open', 0)} open",
        "",
        "Status words: open (ready or waiting on a blocker), in progress (claimed), blocked, closed. A task is *ready* when every issue it depends on is closed.",
        "",
    ]
    for e in epics:
        children = sorted((r for r in rows if parent.get(r["id"]) == e["id"]), key=lambda r: (r.get("issue_type") == "milestone", r["id"]))
        done = sum(1 for c in children if c.get("status") == "closed")
        lines.append(f"## {e['id']} · {e['title']}")
        lines.append("")
        lines.append(f"<span class=\"status {e.get('status','open')}\">{ICON.get(e.get('status','open'), e.get('status'))}</span> {done}/{len(children)} children closed")
        lines.append("")
        desc = (e.get("description") or "").split("\n\nEvidence:")[0].strip()
        if desc:
            lines.append(desc)
            lines.append("")
        lines.append("| id | title | type | P | status | ready | blocked by |")
        lines.append("|---|---|---|---|---|---|---|")
        for c in children:
            bl = blockers.get(c["id"], [])
            open_bl = [b for b in bl if by_id.get(b, {}).get("status") != "closed"]
            ready = "yes" if c.get("status") == "open" and not open_bl else ""
            blocked_txt = ", ".join(f"`{b}`" for b in open_bl[:6]) + (" …" if len(open_bl) > 6 else "")
            title = c["title"].replace("|", "\\|")
            lines.append(f"| `{c['id']}` | {title} | {c.get('issue_type','task')} | P{c.get('priority',2)} | <span class=\"status {c.get('status','open')}\">{ICON.get(c.get('status','open'), c.get('status'))}</span> | {ready} | {blocked_txt} |")
        lines.append("")
    orphans = [r for r in rows if r["id"] not in parent and r.get("issue_type") != "epic"]
    if orphans:
        lines.append("## Unfiled")
        lines.append("")
        for r in sorted(orphans, key=lambda r: r["id"]):
            lines.append(f"- `{r['id']}` {r['title']} ({r.get('status','open')})")
        lines.append("")
    OUT.write_text("\n".join(lines), encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)} ({total} issues)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
