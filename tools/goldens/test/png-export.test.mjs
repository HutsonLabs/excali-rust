// excali-scene's canvas export fixture (ex-405) is upstream's output:
// tools/goldens/png-export.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate exportToCanvas's sizing
// (packages/excalidraw/scene/export.ts:180-285, 566-576, and the utils
// wrapper, packages/utils/src/export.ts:64-105) independently and hold the
// recorded canvases to them, so a generator that lost the scale, the
// padding or the background would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "png-export.mjs");
const FILE = "canvas-export.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "png-export-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(COMMITTED, "utf8"));
const scene = (name) => committed().scenes.find((s) => s.name === name);

/** The scale the canvas was drawn at: the first draw's matrix. */
const drawScale = (s) => s.events.find((e) => e.m).m[0];

/**
 * HTML's reflection of an unsigned long attribute: the canvas's width
 * defaults to 300, its height to 150.
 */
const reflect = (v, fallback = 300) => {
  const n = Number.isFinite(v) ? Math.trunc(v) : 0;
  const u = ((n % 2 ** 32) + 2 ** 32) % 2 ** 32;
  return u > 2147483647 ? fallback : u;
};

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/png-export.mjs");
});

test("the default padding is 10 on every side", () => {
  const padded = scene("default");
  const bare = scene("padding-0");
  assert.equal(padded.width, bare.width + 20);
  assert.equal(padded.height, bare.height + 20);
});

test("the canvas is the content size times exportScale, drawn at that scale", () => {
  const base = scene("default");
  for (const [name, k] of [["scale-2", 2], ["scale-3-embed", 3]]) {
    const s = scene(name);
    assert.equal(drawScale(s), k);
    assert.ok(Math.abs(s.width - base.width * k) <= k, `${name} width`);
    assert.ok(Math.abs(s.height - base.height * k) <= k, `${name} height`);
  }
  // a fractional size is truncated by the width attribute
  const frac = scene("fractional-scale-1.5");
  assert.equal(drawScale(frac), 1.5);
  assert.equal(frac.width, reflect(frac.events[0].rect[2] * 1.5));
});

test("the background is a rectangle over the whole canvas, or none", () => {
  for (const name of ["default", "scale-2", "background-coloured", "dark", "negative-size"]) {
    const s = scene(name);
    const k = drawScale(s);
    assert.equal(s.events[0].op, "fillRect", name);
    assert.deepEqual(s.events[0].rect, [0, 0, s.width / k, s.height / k], name);
  }
  for (const name of ["no-background", "background-transparent", "utils-plain"]) {
    assert.ok(!scene(name).events.some((e) => e.op === "fillRect" && e.rect[0] === 0 && e.rect[1] === 0), name);
  }
});

test("a negative size falls back to the attributes' defaults, 300 × 150", () => {
  // a 50 × 50 rectangle with padding -100: -150 × -150
  const s = scene("negative-size");
  assert.equal(s.width, reflect(-150, 300));
  assert.equal(s.height, reflect(-150, 150));
  assert.deepEqual([s.width, s.height], [300, 150]);
});

test("utils: maxWidthOrHeight fits the larger side, else the export scale", () => {
  const base = scene("utils-max-larger-no-scale");
  const smaller = scene("utils-max-smaller");
  assert.equal(Math.max(smaller.width, smaller.height), 100);
  // the scale is maxWidthOrHeight over the unscaled content size, which
  // the width attribute truncated: within one pixel of it
  const k = drawScale(smaller);
  assert.ok(Math.abs(100 / k - Math.max(base.width, base.height)) < 1);
  assert.equal(drawScale(scene("utils-max-larger")), 2);
  assert.equal(drawScale(base), 1);
  // maxWidthOrHeight wins over getDimensions
  assert.equal(Math.max(scene("utils-max-and-get-dimensions").width, scene("utils-max-and-get-dimensions").height), 120);
  // no maxWidthOrHeight and no getDimensions: scale 1 whatever exportScale is
  const plain = scene("utils-plain");
  assert.equal(drawScale(plain), 1);
  assert.equal(plain.sizing.exportScale, 3);
});

test("an exported frame has no padding and no clip", () => {
  const s = scene("exporting-frame");
  // the 200 × 150 frame at scale 2, padding 0 although 30 was asked
  assert.equal(s.width, 400);
  assert.equal(s.height, 300);
  assert.ok(!s.events.some((e) => e.op === "clip"));
});

test("embedding scenes record the serialized scene", () => {
  const embedded = committed().scenes.filter((s) => s.metadata);
  assert.ok(embedded.length >= 6);
  for (const s of embedded) {
    const doc = JSON.parse(s.metadata);
    assert.equal(doc.type, "excalidraw");
    assert.equal(doc.elements.length, s.elements.length, s.name);
  }
});
