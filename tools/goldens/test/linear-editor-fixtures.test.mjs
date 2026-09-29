// excali-editor's linear element editor fixture is upstream's output:
// tools/goldens/linear-editor-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "linear-editor-fixtures.mjs");
const NAME = "linear-editor.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "linear-editor-fixtures-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) =>
  spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8", env: process.env });

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], NAME));
  assert.ok(first.equals(readFileSync(join(outs[1], NAME))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/linear-editor-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("linear-editor.json covers the handle and midpoint rules", () => {
  const fixture = JSON.parse(readFileSync(COMMITTED, "utf8"));
  assert.equal(fixture.pointHandleSize, 10);
  assert.equal(fixture.draggingThreshold, 10);
  const byId = new Map(fixture.cases.map((c) => [c.id, c]));
  const view = (id, zoom, editing) => byId.get(id).views.find((v) => v.zoom === zoom && v.editing === editing);

  // a segment is too short under 4 handles (40 px) on screen: 30 and 5 px
  // segments at zoom 1, and every one of them at zoom 0.5 but the 170 px one
  assert.deepEqual(view("line-short-segments", 1, true).tooShort, [true, true, false, false]);
  assert.deepEqual(view("line-short-segments", 0.5, true).tooShort, [true, true, false, true]);
  // outside the editor a line of three or more points has no midpoints ...
  assert.deepEqual(view("line-rotated", 1, false).midPoints, []);
  // ... unless it carries a label
  assert.equal(view("arrow-labelled", 1, false).midPoints.length, 2);
  // an elbow arrow has handles at its ends only (pointHandles runs from
  // index -1 to points.length)
  const elbow = byId.get("arrow-elbow").pointHandles;
  assert.deepEqual([elbow[0], elbow[1], elbow.at(-2), elbow.at(-1)], [false, true, true, false]);
  assert.ok(elbow.slice(2, -2).every((h) => h === false));

  // every op ran, and a label drag always moved the label
  const ops = fixture.cases.flatMap((c) => c.ops);
  for (const op of ["addMidpoint", "deletePoints", "addPoints", "dragLabel"]) {
    assert.ok(ops.some((o) => o.op === op), `no ${op}`);
  }
  assert.ok(ops.filter((o) => o.op === "dragLabel").every((o) => o.result === true));
  // probes hit both point handles and midpoint handles
  const probes = fixture.cases.flatMap((c) => c.views.flatMap((v) => v.probes));
  assert.ok(probes.some((p) => p.pointIndex >= 0));
  assert.ok(probes.some((p) => p.hit !== null && p.pointIndex < 0));
});
