// excali-scene's static scene fixture (ex-402) is upstream's output:
// tools/goldens/static-scene.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate staticScene.ts's grid rules
// (packages/excalidraw/renderer/staticScene.ts:57-163) independently and
// hold the recorded draws to them, so a recorder that lost draws or styles
// would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "static-scene.mjs");
const FILE = "static-scene.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "static-scene-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(COMMITTED, "utf8"));
const scene = (name) => committed().scenes.find((s) => s.name === name);

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/static-scene.mjs");
});

const strokes = (s) => s.events.filter((e) => e.op === "stroke");
const last = (list) => list[list.length - 1];

test("the background comes first, then the grid", () => {
  const s = scene("grid");
  // bootstrapCanvas's fillRect, recorded as a rectangle, not a path
  assert.equal(s.events[0].op, "fillRect");
  assert.deepEqual(s.events[0].rect, [0, 0, 400, 300]);
  // COLOR_WHITE, then the background
  assert.deepEqual(s.events[0].fillStyle, ["#ffffff", "#ffffff"]);
  assert.ok(s.events.slice(1).every((e) => e.op === "stroke"));
});

test("grid colours: bold #dddddd solid, regular #e5e5e5 dashed", () => {
  for (const e of strokes(scene("grid"))) {
    const colour = last(e.strokeStyle);
    if (colour === "#dddddd") assert.deepEqual(e.dash, []);
    else {
      assert.equal(colour, "#e5e5e5");
      // [lineWidth * 3, space + (lineWidth + space)], space = 1 / zoom
      assert.deepEqual(e.dash, [3, 3]);
    }
  }
  const dark = strokes(scene("grid-dark")).map((e) => last(e.strokeStyle));
  assert.ok(!dark.includes("#dddddd") && !dark.includes("#e5e5e5"));
});

test("regular lines are left out when gridSize × zoom < 10", () => {
  const colours = (name) => new Set(strokes(scene(name)).map((e) => last(e.strokeStyle)));
  assert.ok(colours("grid-zoom-0.5").has("#e5e5e5"));
  assert.deepEqual([...colours("grid-zoom-0.45")], ["#dddddd"]);
  assert.deepEqual([...colours("grid-zoom-0.1")], ["#dddddd"]);
});

test("iframes and embeddables are drawn after every other element", () => {
  const s = scene("elements-exporting");
  const labels = s.events.filter((e) => e.op === "text" && e.font.includes("Helvetica"));
  assert.ok(labels.length > 0);
  const firstLabel = s.events.indexOf(labels[0]);
  // after the first label only placeholder text and the iframe-likes' own
  // strokes and fills follow: no image, no other font
  assert.ok(s.events.slice(firstLabel).every((e) => e.op !== "image" && (e.op !== "text" || e.font.includes("Helvetica"))));
});

test("upstream's fillRect calls are recorded as fillRect: background, link icon canvas, image placeholder", () => {
  const all = JSON.parse(readFileSync(COMMITTED, "utf8")).scenes.flatMap((s) => s.events);
  const rects = all.filter((e) => e.op === "fillRect");
  assert.ok(rects.length > 0);
  for (const e of rects) {
    assert.equal(e.rect.length, 4);
    assert.equal(e.path, undefined);
  }
  // drawImagePlaceholder's box in #E7E7E7 before the placeholder icon
  assert.ok(rects.some((e) => e.fillStyle.at(-1) === "#E7E7E7"));
});
