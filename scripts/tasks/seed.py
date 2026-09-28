#!/usr/bin/env python3
"""Seed (upsert) the beads tracker from plan/tasks.json.

plan/tasks.json is the human-authored task graph. This script turns it into
the JSONL that `bd import` accepts (the same schema `bd export` emits), imports
it with upsert semantics, then refreshes the tracked export. Re-running is
safe: existing issues are updated, never deleted, and a claimed or closed
status in the database is left alone (the seed only sets status on creation).

Usage: scripts/tasks/seed.py [--dry-run]
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True).stdout.strip())
PLAN = ROOT / "plan" / "tasks.json"
EXPORT = ROOT / ".beads" / "issues.jsonl"


def actor() -> str:
    r = subprocess.run(["git", "config", "user.name"], cwd=ROOT, capture_output=True, text=True)
    return r.stdout.strip() or "unknown"


def existing_ids() -> set[str]:
    if not EXPORT.exists():
        return set()
    ids = set()
    for line in EXPORT.read_text(encoding="utf-8").splitlines():
        try:
            o = json.loads(line)
        except json.JSONDecodeError:
            continue
        if o.get("_type", "issue") == "issue" and o.get("id"):
            ids.add(o["id"])
    return ids


def evidence_block(ev: list[str]) -> str:
    if not ev:
        return ""
    return "\n\nEvidence:\n" + "\n".join(f"- {e}" for e in ev)


def build(plan: dict, who: str, known: set[str]) -> list[dict]:
    rows: list[dict] = []

    def base(item: dict, kind: str, labels: list[str]) -> dict:
        row = {
            "id": item["id"],
            "title": item["title"],
            "issue_type": kind,
            "priority": int(item.get("priority", 2)),
            "labels": labels,
            "created_by": who,
            "description": item.get("description", "") + evidence_block(item.get("evidence", [])),
        }
        if item["id"] not in known:
            row["status"] = "open"
        if item.get("acceptance"):
            row["acceptance_criteria"] = item["acceptance"]
        if item.get("design"):
            row["design"] = item["design"]
        return row

    for e in plan["epics"]:
        rows.append(base(e, "epic", [f"phase-{e['phase']}"]))
    tasks_by_epic: dict[str, list[str]] = {}
    for t in plan["tasks"]:
        labels = [f"phase-{next(e['phase'] for e in plan['epics'] if e['id']==t['epic'])}"] + [f"crate:{c}" for c in t.get("crates", [])] + t.get("labels", [])
        row = base(t, t.get("type", "task"), labels)
        deps = [{"issue_id": t["id"], "depends_on_id": t["epic"], "type": "parent-child"}]
        for b in t.get("blocked_by", []):
            deps.append({"issue_id": t["id"], "depends_on_id": b, "type": "blocks"})
        row["dependencies"] = deps
        rows.append(row)
        tasks_by_epic.setdefault(t["epic"], []).append(t["id"])
    for m in plan["milestones"]:
        row = base(m, "milestone", [f"phase-{next(e['phase'] for e in plan['epics'] if e['id']==m['epic'])}", "needs-human"])
        deps = [{"issue_id": m["id"], "depends_on_id": m["epic"], "type": "parent-child"}]
        for tid in tasks_by_epic.get(m["epic"], []):
            deps.append({"issue_id": m["id"], "depends_on_id": tid, "type": "blocks"})
        for b in m.get("blocked_by", []):
            deps.append({"issue_id": m["id"], "depends_on_id": b, "type": "blocks"})
        row["dependencies"] = deps
        rows.append(row)
    return rows


def main(argv: list[str]) -> int:
    plan = json.loads(PLAN.read_text(encoding="utf-8"))
    ids = [x["id"] for x in plan["epics"] + plan["tasks"] + plan["milestones"]]
    dupes = {i for i in ids if ids.count(i) > 1}
    if dupes:
        print(f"duplicate ids in tasks.json: {sorted(dupes)}", file=sys.stderr)
        return 1
    idset = set(ids)
    for t in plan["tasks"] + plan["milestones"]:
        for b in t.get("blocked_by", []):
            if b not in idset:
                print(f"{t['id']} blocked_by unknown id {b}", file=sys.stderr)
                return 1
    rows = build(plan, actor(), existing_ids())
    jsonl = "\n".join(json.dumps(r, ensure_ascii=False) for r in rows) + "\n"
    if "--dry-run" in argv:
        sys.stdout.write(jsonl)
        return 0
    tmp = ROOT / ".beads" / "seed.tmp.jsonl"
    tmp.write_text(jsonl, encoding="utf-8")
    try:
        subprocess.run(["bd", "import", str(tmp)], cwd=ROOT, check=True)
    finally:
        tmp.unlink(missing_ok=True)
    subprocess.run(["bd", "export", "-o", ".beads/issues.jsonl"], cwd=ROOT, check=True)
    subprocess.run([sys.executable, str(ROOT / "scripts" / "tasks" / "render-progress.py")], cwd=ROOT, check=True)
    print(f"seeded {len(rows)} issues")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
