#!/usr/bin/env python3
"""Tests for plan/tasks.json and scripts/tasks/seed.py.

They pin the owner decisions of 2026-09-27 (task ex-008) where the plan and
the process pages record them: calendar versioning with v26.9.1 as the first
release, crates.io deferred, milestones and the integration-guide walk closed
by agents, term.hut PRs merged by agents after green CI, and the font
fallback rule.

Run: python3 scripts/tasks/test_plan.py
"""
from __future__ import annotations

import io
import json
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import seed  # noqa: E402

ROOT = HERE.parents[1]
PLAN = json.loads((ROOT / "plan" / "tasks.json").read_text(encoding="utf-8"))
CONTENT = ROOT / "site" / "content"


def item(iid: str) -> dict:
    for x in PLAN["epics"] + PLAN["milestones"] + PLAN["tasks"]:
        if x["id"] == iid:
            return x
    raise KeyError(iid)


def rows() -> dict[str, dict]:
    return {r["id"]: r for r in seed.build(PLAN, "tester", set())}


def blockers(row: dict) -> list[str]:
    return sorted(d["depends_on_id"] for d in row.get("dependencies", []) if d["type"] == "blocks")


class OwnerDecisionsInThePlan(unittest.TestCase):
    def test_ex_008_is_a_phase_0_task(self):
        t = item("ex-008")
        self.assertEqual(t["epic"], "ex-e0")
        self.assertIn("owner decisions", t["title"].lower())
        self.assertIn("2026-09-27", t["description"])

    def test_ex_804_tags_the_first_calendar_release(self):
        t = item("ex-804")
        self.assertEqual(t["title"], "Tag v26.9.1 and publish the GitHub release")
        self.assertEqual(sorted(t["blocked_by"]), ["ex-802", "ex-803"])
        self.assertIn("gh release create", t["acceptance"])
        self.assertIn("v26.9.1", t["acceptance"])

    def test_ex_803_is_walked_by_an_agent_in_a_fresh_clone(self):
        t = item("ex-803")
        self.assertIn("fresh clone", t["acceptance"])
        self.assertIn("temp", t["acceptance"])
        self.assertIn("transcript", t["acceptance"])
        self.assertNotIn("needs-human", t["labels"])
        self.assertNotIn("human", t["title"].lower())

    def test_ex_801_is_deferred_by_the_owner(self):
        t = item("ex-801")
        self.assertTrue(t.get("deferred"))
        self.assertIn("Deferred by the owner", t["description"])
        self.assertIn("2026-09-27", t["description"])
        self.assertIn("deferred", t["labels"])

    def test_font_licences_do_not_wait_on_a_human(self):
        t = item("ex-306")
        self.assertNotIn("needs-human", t["labels"])
        self.assertIn("fallback", t["acceptance"])
        self.assertIn("Licence gaps", t["acceptance"])

    def test_m8_is_the_first_release_without_crates_io(self):
        m = item("ex-m8")
        self.assertIn("v26.9.1", m["title"] + m["acceptance"])
        self.assertNotIn("Crates published", m["acceptance"])
        self.assertNotIn("1.0", m["title"])

    def test_phase_8_epic_describes_the_release(self):
        e = item("ex-e8")
        self.assertIn("v26.9.1", e["description"])
        self.assertIn("deferred", e["description"])
        self.assertNotIn("1.0 tag", e["description"])

    def test_version_bump_is_ex_008_scope(self):
        self.assertIn("26.9.1", item("ex-008")["acceptance"])


class SeedRows(unittest.TestCase):
    def test_deferred_task_does_not_block_its_milestone(self):
        r = rows()
        self.assertNotIn("ex-801", blockers(r["ex-m8"]))
        self.assertEqual(blockers(r["ex-m8"]), ["ex-802", "ex-803", "ex-804"])

    def test_deferred_task_is_still_seeded_with_its_parent(self):
        r = rows()["ex-801"]
        parents = [d["depends_on_id"] for d in r["dependencies"] if d["type"] == "parent-child"]
        self.assertEqual(parents, ["ex-e8"])
        self.assertIn("deferred", r["labels"])

    def test_ex_804_rows(self):
        self.assertEqual(blockers(rows()["ex-804"]), ["ex-802", "ex-803"])

    def test_milestones_are_closed_by_agents(self):
        for m in PLAN["milestones"]:
            with self.subTest(m=m["id"]):
                self.assertNotIn("needs-human", rows()[m["id"]]["labels"])

    def test_ex_008_row(self):
        r = rows()["ex-008"]
        self.assertIn("phase-0", r["labels"])
        self.assertIn("ex-008", blockers(rows()["ex-m0"]))

    def test_live_work_may_not_wait_on_deferred_work(self):
        plan = {
            "epics": [{"id": "ex-e9", "phase": 9, "title": "e"}],
            "milestones": [],
            "tasks": [
                {"id": "ex-901", "epic": "ex-e9", "title": "held", "deferred": True},
                {"id": "ex-902", "epic": "ex-e9", "title": "live", "blocked_by": ["ex-901"]},
            ],
        }
        problems = seed.validate(plan)
        self.assertTrue(any("ex-902" in p and "deferred" in p for p in problems), problems)
        plan["tasks"][1]["blocked_by"] = []
        self.assertEqual(seed.validate(plan), [])

    def test_reconcile_removes_edges_the_plan_dropped(self):
        # bd import is an upsert: it adds edges but never removes them, so
        # dropping ex-801 from ex-804's blocked_by needs an explicit removal.
        want = [
            {"id": "ex-804", "labels": ["phase-8"], "dependencies": [
                {"issue_id": "ex-804", "depends_on_id": "ex-e8", "type": "parent-child"},
                {"issue_id": "ex-804", "depends_on_id": "ex-802", "type": "blocks"},
            ]},
        ]
        have = [
            {"id": "ex-804", "labels": ["phase-8"], "dependencies": [
                {"issue_id": "ex-804", "depends_on_id": "ex-e8", "type": "parent-child"},
                {"issue_id": "ex-804", "depends_on_id": "ex-801", "type": "blocks"},
                {"issue_id": "ex-804", "depends_on_id": "ex-802", "type": "blocks"},
                # An edge to an issue the plan does not own (a spike an agent
                # filed) is left alone.
                {"issue_id": "ex-804", "depends_on_id": "ex-a1b2", "type": "blocks"},
            ]},
        ]
        plan_ids = {"ex-e8", "ex-801", "ex-802", "ex-804"}
        self.assertEqual(
            seed.reconcile(want, have, plan_ids, {}),
            [["dep", "remove", "ex-804", "ex-801"]],
        )

    def test_reconcile_withdraws_listed_labels_only(self):
        want = [{"id": "ex-803", "labels": ["phase-8"], "dependencies": []}]
        have = [{"id": "ex-803", "labels": ["needs-human", "phase-8", "triage"], "dependencies": []}]
        self.assertEqual(
            seed.reconcile(want, have, {"ex-803"}, {"ex-803": ["needs-human"]}),
            [["label", "remove", "ex-803", "needs-human"]],
        )
        # Nothing to do once the label is gone.
        have[0]["labels"] = ["phase-8", "triage"]
        self.assertEqual(seed.reconcile(want, have, {"ex-803"}, {"ex-803": ["needs-human"]}), [])

    def test_merge_keeps_tracker_state_of_existing_issues(self):
        # bd import resets fields a row omits, so re-seeding a closed or
        # claimed issue must carry its tracker state through unchanged.
        current = [{
            "id": "ex-001", "title": "old title", "issue_type": "task", "priority": 0,
            "status": "closed", "closed_at": "2026-09-28T04:00:00Z",
            "close_reason": "merged in #6", "external_ref": "gh-6",
            "assignee": "Dr. Hutson", "notes": "agent note",
            "labels": ["phase-0", "triage"],
            "dependencies": [
                {"issue_id": "ex-001", "depends_on_id": "ex-e0", "type": "parent-child"},
                {"issue_id": "ex-001", "depends_on_id": "ex-z9y8", "type": "blocks"},
            ],
            "dependency_count": 2,
        }]
        plan_rows = [{
            "id": "ex-001", "title": "new title", "issue_type": "task", "priority": 1,
            "labels": ["phase-0"], "created_by": "tester", "description": "d",
            "acceptance_criteria": "a",
            "dependencies": [{"issue_id": "ex-001", "depends_on_id": "ex-e0", "type": "parent-child"}],
        }]
        [m] = seed.merge_existing(plan_rows, current, {"ex-001", "ex-e0"})
        for k, v in (("status", "closed"), ("closed_at", "2026-09-28T04:00:00Z"),
                     ("close_reason", "merged in #6"), ("external_ref", "gh-6"),
                     ("assignee", "Dr. Hutson"), ("notes", "agent note")):
            self.assertEqual(m[k], v, k)
        self.assertEqual((m["title"], m["priority"], m["description"], m["acceptance_criteria"]),
                         ("new title", 1, "d", "a"))
        self.assertEqual(m["labels"], ["phase-0", "triage"])
        self.assertEqual(
            sorted(d["depends_on_id"] for d in m["dependencies"]), ["ex-e0", "ex-z9y8"]
        )
        self.assertNotIn("dependency_count", m)

    def test_merge_leaves_new_issues_open(self):
        plan_rows = [{"id": "ex-999", "title": "t", "status": "open", "labels": [], "dependencies": []}]
        self.assertEqual(seed.merge_existing(plan_rows, [], {"ex-999"}), plan_rows)

    def test_withdrawn_labels_in_the_plan(self):
        w = seed.withdrawn_labels(PLAN)
        for m in PLAN["milestones"]:
            self.assertIn("needs-human", w.get(m["id"], []), m["id"])
        self.assertIn("needs-human", w["ex-803"])
        self.assertIn("needs-human", w["ex-306"])
        # A label the plan withdraws is never also applied by it.
        r = rows()
        for iid, labels in w.items():
            for label in labels:
                self.assertNotIn(label, r[iid]["labels"], iid)

    def test_plan_is_consistent(self):
        buf = io.StringIO()
        with redirect_stdout(buf):
            self.assertEqual(seed.main(["--dry-run"]), 0)
        self.assertTrue(buf.getvalue().strip())


class OwnerDecisionsOnTheSite(unittest.TestCase):
    def test_human_checkpoints(self):
        text = (CONTENT / "plan" / "agent-workflow.md").read_text()
        section = text.split("## Human checkpoints", 1)[1]
        self.assertNotIn("closed only by a human", section)
        self.assertIn("agent", section)
        self.assertIn("green", section)
        self.assertIn("bd close ex-m", section)
        self.assertIn("term.hut", section)
        self.assertIn("HutsonLabs/term.hut", section)

    def test_phase_8_text(self):
        text = (CONTENT / "plan" / "phases.md").read_text()
        section = text.split("## Phase 8", 1)[1].split("\n## ", 1)[0]
        self.assertIn("v26.9.1", section)
        self.assertIn("ADR-009", section)
        self.assertIn("gh release create", section)
        self.assertIn("deferred", section)
        self.assertIn("fresh clone", section)
        self.assertNotIn("tag 1.0", section)

    def test_adr_004_fallback_rule(self):
        text = (CONTENT / "decisions" / "adr-004-fonts.md").read_text()
        self.assertIn("fallback", text)
        self.assertIn("OFL", text)
        self.assertIn("MIT", text)
        self.assertIn("Apache", text)
        self.assertIn("## Licence gaps", text)
        self.assertIn("2026-09-27", text)

    def test_adr_009_exists(self):
        self.assertTrue((CONTENT / "decisions" / "adr-009-calendar-versioning.md").exists())


if __name__ == "__main__":
    unittest.main()
