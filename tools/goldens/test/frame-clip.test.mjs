// excali-scene's frame clipping fixture (ex-403) is upstream's output:
// tools/goldens/frame-clip.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate frame.ts's clip rule
// (shouldApplyFrameClip, packages/element/src/frame.ts:911-972) from the
// recorded parts, and hold the static scene fixture's clips to the
// elements that rule clips.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "frame-clip.mjs");
const FILE = "frame-clip.json";
const FIXTURES = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
const COMMITTED = join(FIXTURES, FILE);

const scratch = mkdtempSync(join(tmpdir(), "frame-clip-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/frame-clip.mjs");
});

test("an element clips when it crosses or contains the frame, or by group membership", () => {
  for (const s of committed().scenes) {
    const byId = new Map(s.elements.map((e) => [e.id, e]));
    for (const r of s.results) {
      const element = byId.get(r.id);
      for (const [frameId, d] of Object.entries(r.frames)) {
        if (!s.appState.frameRendering.clip) {
          assert.equal(d.shouldClip, false, `${s.name} ${r.id}`);
          continue;
        }
        if (d.intersecting || d.containing) {
          assert.equal(d.shouldClip, true, `${s.name} ${r.id} in ${frameId}`);
        } else if (element.groupIds.length === 0 || d.inBounds) {
          assert.equal(d.shouldClip, false, `${s.name} ${r.id} in ${frameId}`);
        } else if (!s.appState.selectedElementsAreBeingDragged) {
          assert.equal(d.shouldClip, element.frameId === frameId, `${s.name} ${r.id} in ${frameId}`);
        } else {
          assert.equal(d.shouldClip, d.inFrame, `${s.name} ${r.id} in ${frameId}`);
        }
        assert.equal(d.overlaps, d.inBounds || d.intersecting || d.containing);
      }
    }
  }
});

test("elements inside the frame are not clipped, crossing ones are", () => {
  const frames = Object.fromEntries(scene("frame-clip").results.map((r) => [r.id, r.frames["clip-frame"]]));
  assert.equal(frames["fc-inside"].shouldClip, false);
  assert.equal(frames["fc-crossing"].shouldClip, true);
  assert.equal(frames["fc-backdrop"].containing, true);
  assert.equal(frames["fc-grouped-out"].shouldClip, true);
});

test("the static scene clips each frame child once per draw, radius 8 / zoom", () => {
  const staticScene = JSON.parse(readFileSync(join(FIXTURES, "static-scene.json"), "utf8"));
  const find = (name) => staticScene.scenes.find((s) => s.name === name);
  const frameClips = (s, radius) =>
    s.events.filter((e) => e.op === "clip" && e.path.length === 1 && e.path[0][0] === "roundRect" && e.path[0][5] === radius);
  // every child of clip-frame the rule clips, except bound text, which is
  // drawn with its container inside the container's clip; elements
  // without a frame are never clipped
  const s0 = scene("frame-clip");
  const byId = new Map(s0.elements.map((e) => [e.id, e]));
  const expected = s0.results.filter((r) => {
    const e = byId.get(r.id);
    const bound = e.containerId && byId.has(e.containerId);
    return e.frameId === "clip-frame" && !bound && r.frames["clip-frame"].shouldClip;
  });
  assert.ok(expected.length > 15);
  const rounded = frameClips(find("frame-clip"), 8).filter((e) => e.path[0][3] === 240);
  assert.equal(rounded.length, expected.length);
  assert.equal(frameClips(find("frame-clip-zoom-0.5"), 16).filter((e) => e.path[0][3] === 240).length, expected.length);
  assert.equal(frameClips(find("frame-clip-off"), 8).length, 0);
  assert.equal(frameClips(find("frame-clip-disabled"), 8).length, 0);
});
