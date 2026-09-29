// excali-ui's canvas layer fixture (ex-503) is upstream's output:
// tools/goldens/canvas-layers.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate the rules independently
// (renderer/helpers.ts:47-127, renderNewElementScene.ts, the canvases'
// components) and hold the recorded values to them, so a generator that
// lost cases or draws would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "canvas-layers.mjs");
const FILE = "canvas-layers.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-ui", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "canvas-layers-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(COMMITTED, "utf8"));

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/canvas-layers.mjs");
});

test("snapped scrolls are round(scroll × zoom × dpr) / (zoom × dpr)", () => {
  const cases = committed().snapScroll;
  assert.ok(cases.length >= 20);
  for (const c of cases) {
    const d = c.zoom * c.scale;
    const want = d > 0 ? [Math.round(c.scrollX * d) / d, Math.round(c.scrollY * d) / d] : [c.scrollX, c.scrollY];
    // === : JSON writes -0 as 0
    assert.ok(c.result.scrollX === want[0] && c.result.scrollY === want[1], JSON.stringify(c));
    assert.equal(c.identity, want[0] === c.scrollX && want[1] === c.scrollY, JSON.stringify(c));
  }
  // pixelSnap.test.tsx: 3.3 × 1.5 × 2 = 9.9 → 10; -7.77 × 3 = -23.31 → -23
  const first = cases[0];
  assert.ok(Math.abs(first.result.scrollX * 3 - 10) < 1e-9);
  assert.ok(Math.abs(first.result.scrollY * 3 + 23) < 1e-9);
  assert.equal(cases[1].identity, true);
});

test("backing sizes are the CSS size times the device pixel ratio, in whole pixels", () => {
  const { bootstrap, newElementScenes } = committed();
  for (const c of [...bootstrap, ...newElementScenes]) {
    assert.equal(c.canvasWidth, Math.trunc(c.width * c.scale), c.name);
    assert.equal(c.canvasHeight, Math.trunc(c.height * c.scale), c.name);
  }
  assert.ok(bootstrap.some((c) => !Number.isInteger(c.width * c.scale)), "a fractional backing size is covered");
});

test("each layer bootstraps: scale by the dpr, clear unless an opaque hex background", () => {
  const cases = committed().bootstrap;
  assert.deepEqual([...new Set(cases.map((c) => c.layer))].sort(), ["interactive", "new-element", "static"]);
  for (const c of cases) {
    const clears = c.events.filter((e) => e.op === "clear");
    const bg = c.appState?.viewBackgroundColor;
    const opaque = typeof bg === "string" && /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(bg);
    assert.equal(clears.length, opaque ? 0 : 1, c.name);
    for (const e of c.events) {
      assert.deepEqual(e.m, [c.scale, 0, 0, c.scale, 0, 0], c.name);
      assert.deepEqual(e.rect, [0, 0, c.canvasWidth / c.scale, c.canvasHeight / c.scale], c.name);
    }
    const fills = c.events.filter((e) => e.op === "fillRect");
    assert.equal(fills.length, typeof bg === "string" && bg !== "transparent" ? 1 : 0, c.name);
  }
});

test("the new-element canvas draws under dpr × zoom at the snapped scroll", () => {
  const scenes = committed().newElementScenes;
  for (const s of scenes) {
    const [first, ...rest] = s.events;
    assert.equal(first.op, "clear", s.name);
    assert.deepEqual(first.m, [s.scale, 0, 0, s.scale, 0, 0], s.name);
    const k = s.scale * s.appState.zoom.value;
    if (!s.newElement || s.newElement.type === "selection") {
      assert.equal(rest.length, 1, s.name);
      assert.equal(rest[0].op, "clear", s.name);
      assert.deepEqual(rest[0].m, [k, 0, 0, k, 0, 0], s.name);
    }
    for (const e of rest.filter((x) => x.m)) {
      assert.ok(Math.abs(Math.hypot(e.m[0], e.m[1]) - k) < 1e-9 || e.op === "clear", `${s.name} ${e.op}`);
    }
  }
  // a rectangle drawn at the snapped scroll: rough.js's first move of the
  // stroke is under translate(x + snapped scrollX, y + snapped scrollY)
  const s = scenes.find((x) => x.name === "rectangle-zoom-1.5-dpr-2");
  const d = 1.5 * 2;
  const sx = Math.round(3.3 * d) / d;
  const stroke = s.events.find((e) => e.op === "stroke");
  assert.ok(Math.abs(stroke.m[4] - d * (s.newElement.x + sx)) < 1e-9);
});
