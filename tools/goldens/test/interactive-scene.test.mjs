// excali-editor's interactive scene fixture (ex-713) is upstream's output:
// tools/goldens/interactive-scene.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs. The checks below restate a few of interactiveScene.ts's
// rules independently (bootstrapCanvas without a background, the handles
// getTransformHandles lays out on desktops and phones, the selection
// border's width and the locked border's dash, renderSnaps's colours) and
// hold the recorded draws to them, so a recorder that lost draws or styles
// would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "interactive-scene.mjs");
const FILE = "interactive-scene.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "interactive-scene-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(COMMITTED, "utf8"));
const scene = (name) => committed().scenes.find((s) => s.name === name);
const last = (list) => list[list.length - 1];

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/interactive-scene.mjs");
});

test("the canvas is cleared, never painted: no background", () => {
  // bootstrapCanvas without viewBackgroundColor (helpers.ts:73-127):
  // clearRect over the canvas in CSS pixels, after scale(dpr)
  for (const s of committed().scenes) {
    const [first, ...rest] = s.events;
    assert.equal(first.op, "clear", s.name);
    assert.deepEqual(first.rect, [0, 0, s.width / s.scale, s.height / s.scale], s.name);
    assert.deepEqual(first.m, [s.scale, 0, 0, s.scale, 0, 0], s.name);
    assert.ok(rest.every((e) => e.op !== "clear"), s.name);
  }
});

test("a lone element has four corner handles and a rotation handle; a phone adds the sides", () => {
  // getOmitSidesForEditorInterface (transformHandles.ts:112-131): desktops
  // resize from the border and omit the side handles; the rotation handle
  // is a circle (arc), the others round rects
  const handles = (name) => scene(name).events.filter((e) => e.op === "fill");
  const kinds = (name) => handles(name).map((e) => e.path[0][0]);
  assert.deepEqual(kinds("rectangle-selected"), ["roundRect", "roundRect", "roundRect", "roundRect", "arc"]);
  assert.equal(handles("phone").length, 9);
  // filled white, stroked in the selection colour, 1 / zoom wide
  for (const e of handles("rectangle-selected")) assert.equal(last(e.fillStyle), "#fff");
  const strokes = scene("rectangle-selected").events.filter((e) => e.op === "stroke");
  for (const e of strokes) {
    assert.equal(last(e.strokeStyle), "#6965db");
    assert.equal(e.lineWidth, 1);
  }
  // a locked element: one dashed border in #ced4da, no handles
  const locked = scene("locked-selected").events.filter((e) => e.op !== "clear");
  assert.equal(locked.length, 1);
  assert.equal(last(locked[0].strokeStyle), "#ced4da");
  assert.deepEqual(locked[0].dash, [8, 4]);
});

test("snap lines are #ff6b6b in the light theme and 1 / zoom wide", () => {
  // renderSnaps.ts:8-14, 29-38
  const snaps = scene("snap-lines").events.filter((e) => e.op === "stroke" && last(e.strokeStyle) === "#ff6b6b");
  assert.ok(snaps.length > 15);
  for (const e of snaps) assert.equal(e.lineWidth, 1);
  const zen = scene("snap-lines-zen-dark-zoomed").events.filter((e) => e.op === "stroke");
  assert.ok(zen.length > 0);
  for (const e of zen) {
    assert.equal(last(e.strokeStyle), "#da5b5b");
    assert.equal(e.lineWidth, 1.5 / 0.8);
  }
});
