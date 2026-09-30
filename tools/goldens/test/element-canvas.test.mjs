// excali-scene's per-element bitmap cache fixture (ex-504) is upstream's
// output: tools/goldens/element-canvas.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs. The checks below restate the rules independently
// (renderElement.ts: getCanvasPadding :102-116, cappedElementCanvasSize
// :216-269, generateElementWithCanvas :687-728, canSnapElement :747-752)
// and hold the recorded values to them, so a generator that lost cases or
// took the vector path would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "element-canvas.mjs");
const FILE = "element-canvas.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "element-canvas-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(COMMITTED, "utf8"));

const drawn = (c) => c.elements.find((e) => e.id === c.draw);

const padding = (e) => {
  if (e.type === "freedraw") return e.strokeWidth * 12;
  if (e.type === "text") return e.fontSize / 2;
  if (e.type === "arrow") return e.endArrowhead ? 40 : 20;
  return 20;
};

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/element-canvas.mjs");
});

test("the crop editor's cases are all there", () => {
  const names = committed().cases.map((c) => c.name).filter((n) => n.startsWith("crop-editor"));
  assert.equal(names.length, 8);
});

test("every drawn element is blitted from a bitmap, none drawn as vectors", () => {
  for (const c of committed().cases) {
    const ops = c.events.map((e) => e.op);
    if (!c.cached) {
      assert.deepEqual(ops, [], c.name);
      continue;
    }
    // the image being cropped is blitted after its uncropped preview, at
    // alpha 0.1 (renderElement.ts:1220-1251)
    const cropping = c.appState.croppingElementId === c.draw && drawn(c).crop;
    assert.equal(ops.filter((op) => op === "blit").length, cropping ? 2 : 1, c.name);
    if (cropping) {
      assert.equal(c.events[0].alpha, 0.1, c.name);
      assert.ok(c.events[0].uncropped.length > 0, c.name);
    }
    assert.equal(c.events.filter((e) => e.uncropped).length, cropping ? 1 : 0, c.name);
    assert.deepEqual(
      ops.filter((op) => !["blit", "clip", "unclip"].includes(op)),
      [],
      c.name,
    );
  }
});

test("bitmap sizes are (size × dpr + 2 × padding) × scale, floored, within the caps", () => {
  let capped = 0;
  for (const c of committed().cases) {
    if (!c.cached) continue;
    const e = drawn(c);
    const p = padding(e);
    const { width, height, scale } = c.cached;
    assert.ok(width <= 32767 && height <= 32767, c.name);
    assert.ok(width * height <= 16777216, c.name);
    if (scale !== c.appState.zoom.value) capped++;
    if (["line", "arrow", "freedraw"].includes(e.type)) continue;
    assert.equal(width, Math.floor((e.width * c.scale + p * 2) * scale), c.name);
    assert.equal(height, Math.floor((e.height * c.scale + p * 2) * scale), c.name);
  }
  assert.equal(capped, 5);
});

test("snappable blits land on whole device pixels with smoothing off", () => {
  for (const c of committed().cases) {
    const blit = c.events.find((e) => e.op === "blit");
    if (!blit) continue;
    const angle = drawn(c).angle;
    const snappable = !c.appState.shouldCacheIgnoreZoom && (!angle || Math.abs(Math.sin(2 * angle)) < 1e-4);
    if (snappable) {
      assert.ok(Number.isInteger(blit.m[4]) && Number.isInteger(blit.m[5]), c.name);
      assert.deepEqual(blit.args.slice(0, 2), [0, 0], c.name);
      assert.equal(blit.smoothing, drawn(c).type === "freedraw", c.name);
    } else {
      assert.equal(blit.smoothing, true, c.name);
    }
  }
});

test("the cache regenerates on zoom, theme, crop and frame opacity, not on scroll or a zoom gesture", () => {
  const seqs = Object.fromEntries(committed().sequences.map((q) => [q.name, q]));
  const flags = (name) => Object.fromEntries(seqs[name].steps.map((s) => [s.label, s.regenerated]));
  const zoom = flags("zoom-theme-scroll-dpr");
  assert.equal(zoom.scroll, false);
  assert.equal(zoom.zoom, true);
  assert.equal(zoom["zoom gesture"], false);
  assert.equal(zoom["gesture ends"], true);
  assert.equal(zoom.dark, true);
  assert.equal(zoom["device pixel ratio"], false);
  const frame = flags("frame-opacity");
  assert.equal(frame["frame at 50"], true);
  assert.equal(frame["frame still at 50"], false);
  assert.equal(frame["frame at 0 reads as 100"], true);
  const crop = flags("image-crop");
  assert.equal(crop.cropped, true);
  assert.equal(crop["same crop"], false);
  assert.equal(crop.uncropped, true);
});
