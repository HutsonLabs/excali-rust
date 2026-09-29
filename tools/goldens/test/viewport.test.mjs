// excali-editor's viewport fixture (ex-505) is upstream's output:
// tools/goldens/viewport.mjs regenerates it from the pinned checkout,
// byte-stable across runs, and --check fails when the committed file
// differs. The checks below restate the rules independently
// (constants.ts:362-364, scene/normalize.ts, App.wheel.ts, the coordinate
// transforms) and hold the recorded values to them, so a generator that
// lost cases or recorded the wrong thing would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "viewport.mjs");
const FILE = "viewport.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "viewport-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(COMMITTED, "utf8"));

const num = (v) => (v === "NaN" ? NaN : v === "Infinity" ? Infinity : v === "-Infinity" ? -Infinity : v);

const normalize = (z) => Math.min(Math.max(Math.round((z + Number.EPSILON) * 1e6) / 1e6, 0.1), 30);

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale fixture: run node tools/goldens/viewport.mjs");
});

test("zoom limits and step", () => {
  assert.deepEqual(committed().constants, { MIN_ZOOM: 0.1, MAX_ZOOM: 30, ZOOM_STEP: 0.1, DEFAULT_OVERSCROLL: 150 });
});

test("normalised zoom is clamp(round(z, 6), 0.1, 30)", () => {
  const cases = committed().normalizedZoom;
  assert.ok(cases.length >= 30);
  for (const c of cases) {
    const want = normalize(num(c.input));
    assert.ok(Object.is(num(c.output), want) || num(c.output) === want, JSON.stringify(c));
  }
});

test("the wheel sweep follows the zoom formula around the pointer", () => {
  const sweep = committed().wheel.sequences.filter((s) => s.name.startsWith("sweep-"));
  assert.equal(sweep.length, 200);
  for (const s of sweep) {
    const [step] = s.steps;
    const z = s.state.zoom;
    const dy = step.event.deltaY;
    const sign = Math.sign(dy);
    const delta = Math.abs(dy) > 10 ? 10 * sign : dy;
    const next = normalize(Math.max(z - delta / 100 + Math.log10(Math.max(1, z)) * -sign * Math.min(1, Math.abs(dy) / 20), 0.1));
    if (next === z) {
      assert.equal(step.state.zoom, z, s.name);
      assert.equal(step.state.shouldCacheIgnoreZoom, false, s.name);
      continue;
    }
    // ZOOM_STEP * 100 is 10.000000000000002: a delta of exactly 10 is not capped
    assert.ok(Math.abs(step.state.zoom - next) < 1e-12, `${s.name}: ${step.state.zoom} vs ${next}`);
    assert.equal(step.state.shouldCacheIgnoreZoom, true, s.name);
    // the scene point under the pointer stays put
    const px = s.lastPosition.x - s.state.offsetLeft;
    const py = s.lastPosition.y - s.state.offsetTop;
    const before = [px / z - s.state.scrollX, py / z - s.state.scrollY];
    const after = [px / step.state.zoom - step.state.scrollX, py / step.state.zoom - step.state.scrollY];
    assert.ok(Math.abs(before[0] - after[0]) < 1e-9 && Math.abs(before[1] - after[1]) < 1e-9, s.name);
  }
});

test("coordinate transforms invert each other", () => {
  for (const c of committed().coords) {
    const { zoom, offsetLeft, offsetTop, scrollX, scrollY } = c.state;
    const [x, y] = c.point;
    assert.equal(c.toScene.x, (x - offsetLeft) / zoom - scrollX);
    assert.equal(c.toScene.y, (y - offsetTop) / zoom - scrollY);
    assert.equal(c.toViewport.x, (x + scrollX) * zoom + offsetLeft);
    assert.equal(c.toViewport.y, (y + scrollY) * zoom + offsetTop);
  }
});

test("every recorded zoom is within the limits", () => {
  const f = committed();
  const zooms = [
    ...f.wheel.sequences.flatMap((s) => s.steps.map((x) => x.state.zoom)),
    ...f.actions.performed.map((a) => a.result.zoom),
    ...f.zoomAt.map((c) => c.result.zoom),
  ];
  for (const z of zooms) assert.ok(z >= 0.1 && z <= 30, String(z));
});
