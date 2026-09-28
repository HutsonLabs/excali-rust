#!/usr/bin/env python3
"""Seed (upsert) the beads tracker from plan/tasks.json.

plan/tasks.json is the human-authored task graph. This script turns it into
the JSONL that `bd import` accepts (the same schema `bd export` emits), imports
it with upsert semantics, then refreshes the tracked export. Re-running is
safe: existing issues are updated, never deleted, and a claimed or closed
status in the database is left alone (the seed only sets status on creation).

Because the import never removes anything, the seed then reconciles: a
dependency edge between two plan items that the plan no longer lists is
removed, and so is any label an item lists under "withdraw_labels". A task
marked "deferred": true stays in its epic but does not block the milestone,
no live task may be blocked by it, and it is put in beads' native deferred
status (`bd defer`), which `bd ready` excludes. A label alone would not keep
it out of the ready queue. Like closed status, deferred status is kept on
re-seed; lifting the hold is `bd undefer` plus dropping "deferred" from the
plan.

Usage: scripts/tasks/seed.py [--dry-run]
"""
from __future__ import annotations

import json
import subprocess
from datetime import datetime, timedelta, timezone
import sys
from pathlib import Path

ROOT = Path(subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True).stdout.strip())
PLAN = ROOT / "plan" / "tasks.json"
EXPORT = ROOT / ".beads" / "issues.jsonl"


def actor() -> str:
    r = subprocess.run(["git", "config", "user.name"], cwd=ROOT, capture_output=True, text=True)
    return r.stdout.strip() or "unknown"


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
            row["status"] = "deferred" if item.get("deferred") else "open"
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
        # A task the owner has deferred ("deferred": true) stays in its epic
        # but does not hold up the epic's milestone.
        if not t.get("deferred"):
            tasks_by_epic.setdefault(t["epic"], []).append(t["id"])
    for m in plan["milestones"]:
        # Milestones are closed by agents once the acceptance check is green
        # in CI with evidence posted (owner decision, 2026-09-27), so they
        # carry no needs-human label.
        row = base(m, "milestone", [f"phase-{next(e['phase'] for e in plan['epics'] if e['id']==m['epic'])}"])
        deps = [{"issue_id": m["id"], "depends_on_id": m["epic"], "type": "parent-child"}]
        for tid in tasks_by_epic.get(m["epic"], []):
            deps.append({"issue_id": m["id"], "depends_on_id": tid, "type": "blocks"})
        # A milestone may also name a task of its own epic (a tracked
        # exception it must wait for, ADR-008); that is one edge, not two.
        for b in m.get("blocked_by", []):
            if b not in tasks_by_epic.get(m["epic"], []):
                deps.append({"issue_id": m["id"], "depends_on_id": b, "type": "blocks"})
        row["dependencies"] = deps
        rows.append(row)
    return rows


PLAN_FIELDS = ("title", "issue_type", "priority", "description", "acceptance_criteria", "design")


STAMP = "%Y-%m-%dT%H:%M:%SZ"


def _stamp_after(current: str | None, now: str) -> str:
    """`now`, or one second past `current` if the clock is not ahead of it.

    bd import takes a row only when its updated_at is strictly newer than the
    database's; updated_at has second granularity and a tie keeps the local
    row.
    """
    if not current:
        return now
    cur = datetime.strptime(current[:19] + "Z", STAMP).replace(tzinfo=timezone.utc)
    new = datetime.strptime(now, STAMP).replace(tzinfo=timezone.utc)
    return max(new, cur + timedelta(seconds=1)).strftime(STAMP)


def _edges(deps: list[dict] | None) -> set[tuple[str, str]]:
    return {(d["depends_on_id"], d["type"]) for d in deps or []}


def merge_existing(plan_rows: list[dict], current_rows: list[dict], plan_ids: set[str], now: str | None = None) -> list[dict]:
    """Rows to import: plan fields laid over the tracker's current rows.

    `bd import` resets fields a row omits (a closed issue comes back open), so
    an issue that already exists is imported as its full current row with
    only the plan-owned fields replaced: tracker state (status, close reason,
    assignee, notes, external ref) is carried through, labels are the union,
    and dependency edges to issues outside the plan are kept. New issues are
    imported exactly as the plan builds them.

    `bd import` also skips a row whose updated_at is not strictly newer than
    the database's, so a merged row the plan changed gets a fresh updated_at;
    an unchanged row keeps the tracker's, and re-seeding it is a no-op.
    """
    now = now or datetime.now(timezone.utc).strftime(STAMP)
    current = {r["id"]: r for r in current_rows}
    out: list[dict] = []
    for row in plan_rows:
        cur = current.get(row["id"])
        if cur is None:
            out.append(row)
            continue
        merged = {k: v for k, v in cur.items() if not k.endswith("_count")}
        for k in PLAN_FIELDS:
            if k in row:
                merged[k] = row[k]
        merged["labels"] = sorted(set(cur.get("labels") or []) | set(row.get("labels", [])))
        foreign = [
            {k: d[k] for k in ("issue_id", "depends_on_id", "type")}
            for d in cur.get("dependencies") or []
            if d["depends_on_id"] not in plan_ids
        ]
        merged["dependencies"] = list(row.get("dependencies", [])) + foreign
        changed = (
            any((merged.get(k) or "") != (cur.get(k) or "") for k in PLAN_FIELDS)
            or set(merged["labels"]) != set(cur.get("labels") or [])
            or not _edges(merged["dependencies"]) <= _edges(cur.get("dependencies"))
        )
        if changed:
            merged["updated_at"] = _stamp_after(cur.get("updated_at"), now)
        out.append(merged)
    return out


def withdrawn_labels(plan: dict) -> dict[str, list[str]]:
    """Labels the plan takes back from an item ("withdraw_labels"), by id."""
    out: dict[str, list[str]] = {}
    for x in plan["epics"] + plan["tasks"] + plan["milestones"]:
        if x.get("withdraw_labels"):
            out[x["id"]] = list(x["withdraw_labels"])
    return out


def deferred_ids(plan: dict) -> set[str]:
    """Tasks the owner has deferred ("deferred": true)."""
    return {t["id"] for t in plan["tasks"] if t.get("deferred")}


def reconcile(
    want: list[dict],
    have: list[dict],
    plan_ids: set[str],
    withdrawn: dict[str, list[str]],
    deferred: set[str] = frozenset(),
) -> list[list[str]]:
    """bd subcommands that remove what the plan dropped, and defer what it holds.

    `bd import` is an upsert: it adds labels and dependency edges but never
    removes them. Two removals are the plan's to make:
      - a dependency edge between two plan items that the plan no longer
        lists (edges to issues agents filed themselves are left alone);
      - a label listed in the item's "withdraw_labels" (other labels an
        agent may have added at runtime are left alone).
    A task the plan defers whose tracker status is still open is put in
    deferred status; closed, claimed or already deferred issues are left
    as they are.
    """
    wanted = {r["id"]: r for r in want}
    cmds: list[list[str]] = []
    for cur in sorted(have, key=lambda r: r["id"]):
        iid = cur["id"]
        if iid not in wanted:
            continue
        keep = {(d["depends_on_id"], d["type"]) for d in wanted[iid].get("dependencies", [])}
        for d in sorted(cur.get("dependencies") or [], key=lambda d: d["depends_on_id"]):
            target = d["depends_on_id"]
            if target in plan_ids and (target, d["type"]) not in keep:
                cmds.append(["dep", "remove", iid, target])
        labels = set(cur.get("labels") or [])
        for label in withdrawn.get(iid, []):
            if label in labels:
                cmds.append(["label", "remove", iid, label])
        if iid in deferred and cur.get("status", "open") == "open":
            cmds.append(["defer", iid])
    return cmds


def _read_jsonl(path: Path) -> list[dict]:
    out = []
    for line in path.read_text(encoding="utf-8").splitlines():
        try:
            o = json.loads(line)
        except json.JSONDecodeError:
            continue
        if o.get("_type", "issue") == "issue" and o.get("id"):
            out.append(o)
    return out


def validate(plan: dict) -> list[str]:
    """Structural problems in a plan; empty when it can be seeded."""
    ids = [x["id"] for x in plan["epics"] + plan["tasks"] + plan["milestones"]]
    dupes = {i for i in ids if ids.count(i) > 1}
    if dupes:
        return [f"duplicate ids in tasks.json: {sorted(dupes)}"]
    problems: list[str] = []
    idset = set(ids)
    deferred = deferred_ids(plan)
    for t in plan["tasks"] + plan["milestones"]:
        for b in t.get("blocked_by", []):
            if b not in idset:
                problems.append(f"{t['id']} blocked_by unknown id {b}")
            elif b in deferred and t["id"] not in deferred:
                problems.append(f"{t['id']} blocked_by deferred task {b}; deferred work cannot hold up live work")
    for t in plan["tasks"]:
        both = set(t.get("labels", [])) & set(t.get("withdraw_labels", []))
        if both:
            problems.append(f"{t['id']} both applies and withdraws {sorted(both)}")
    return problems


def main(argv: list[str]) -> int:
    plan = json.loads(PLAN.read_text(encoding="utf-8"))
    problems = validate(plan)
    for p in problems:
        print(p, file=sys.stderr)
    if problems:
        return 1
    dry = "--dry-run" in argv
    tmp = ROOT / ".beads" / "seed.tmp.jsonl"
    if dry:
        # No database access: the tracked export stands in for it.
        current = _read_jsonl(EXPORT) if EXPORT.exists() else []
    else:
        # The database, not the tracked export, is the current state: it may
        # hold claims and closes the export has not caught up with yet.
        try:
            subprocess.run(["bd", "export", "-o", str(tmp)], cwd=ROOT, check=True)
            current = _read_jsonl(tmp)
        finally:
            tmp.unlink(missing_ok=True)
    rows = build(plan, actor(), {r["id"] for r in current})
    plan_ids = {r["id"] for r in rows}
    jsonl = "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in merge_existing(rows, current, plan_ids))
    if dry:
        sys.stdout.write(jsonl)
        return 0
    tmp.write_text(jsonl, encoding="utf-8")
    try:
        subprocess.run(["bd", "import", str(tmp)], cwd=ROOT, check=True)
        # Read back what the database holds and remove what the plan dropped.
        subprocess.run(["bd", "export", "-o", str(tmp)], cwd=ROOT, check=True)
        cmds = reconcile(rows, _read_jsonl(tmp), plan_ids, withdrawn_labels(plan), deferred_ids(plan))
    finally:
        tmp.unlink(missing_ok=True)
    for cmd in cmds:
        print("bd " + " ".join(cmd))
        subprocess.run(["bd", *cmd], cwd=ROOT, check=True)
    subprocess.run(["bd", "export", "-o", ".beads/issues.jsonl"], cwd=ROOT, check=True)
    subprocess.run([sys.executable, str(ROOT / "scripts" / "tasks" / "render-progress.py")], cwd=ROOT, check=True)
    print(f"seeded {len(rows)} issues")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
