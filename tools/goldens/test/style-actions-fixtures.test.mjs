// excali-editor's style action fixture is upstream's output:
// tools/goldens/style-actions-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "style-actions-fixtures.mjs");
const NAME = "style-actions.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "style-actions-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/style-actions-fixtures.mjs");
});

test("--check passes on the committed fixture", () => {
  const r = run(["--check"]);
  assert.equal(r.status, 0, r.stderr);
});

test("style-actions.json holds upstream's answers", () => {
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = (id) => cases.find((c) => c.id === id);
  const element = (c, id) => c.result.elements.find((e) => e.id === id);

  // a stroke colour pick on a rectangle: the element and the current item
  const stroke = byId("stroke-rectangle");
  assert.equal(element(stroke, "r").strokeColor, "#e03131");
  assert.deepEqual(stroke.result.appState, { currentItemStrokeColor: "#e03131" });
  assert.equal(stroke.result.captureUpdate, "IMMEDIATELY");

  // a fill on a line that can close turns it into a polygon
  const polygon = element(byId("background-polygon"), "lp");
  assert.equal(polygon.polygon, true);
  assert.deepEqual(polygon.points.at(-1), polygon.points[0]);

  // the font picker's reset is never captured; hovering, eventually
  assert.equal(byId("font-family-reset-all").result.captureUpdate, "NEVER");
  assert.equal(byId("font-family-hover").result.captureUpdate, "EVENTUALLY");

  // togglePolygon with a non-line selected, hyperlink while editing: false
  assert.equal(byId("polygon-not-lines").result, null);
  assert.equal(byId("hyperlink-open").result, null);
});
