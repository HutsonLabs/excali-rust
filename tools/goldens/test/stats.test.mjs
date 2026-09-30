// excali-ui's stats panel fixture and stylesheet (ex-529) are upstream's
// output: tools/goldens/stats.mjs regenerates them from the pinned checkout,
// byte-stable across runs, and --check fails when a committed file differs.
// The checks below restate what the issue asks for (research/
// ui-design-system.md 3.9) from the cited upstream files and hold the
// recorded output to them, so a generator that lost sections or cases
// would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "stats.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "stats.json"), join("src", "stats", "stats.css")];

const scratch = mkdtempSync(join(tmpdir(), "stats-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));
const text = (n) => (n == null ? "" : typeof n === "string" ? n : (n.children ?? []).map(text).join(""));
const byName = (name) => committed().cases.find((c) => c.name === name);
const find = (n, pred) => {
  if (!n || typeof n !== "object" || n.icon) return null;
  if (pred(n)) return n;
  for (const c of n.children) {
    const r = find(c, pred);
    if (r) return r;
  }
  return null;
};
const inputs = (n, out = []) => {
  if (!n || typeof n !== "object" || n.icon) return out;
  if (n.attrs["data-testid"] && n.attrs.class?.startsWith("drag-input-container")) {
    out.push([n.attrs["data-testid"], find(n, (x) => x.tag === "input").value]);
  }
  n.children.forEach((c) => inputs(c, out));
  return out;
};

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/stats.mjs`);
  }
});

test("STATS_PANELS is the bitmask {generalStats: 1, elementProperties: 2} (constants.ts:584)", () => {
  assert.deepEqual(committed().panels, { generalStats: 1, elementProperties: 2 });
});

test("the general section: shapes, width, height, then the grid step in grid mode (index.tsx:195-236)", () => {
  const general = (name) => find(byName(name).dom[0], (n) => n.attrs.class === "exc-stats__rows");
  assert.equal(text(general("nothing-selected")), "SceneShapes21Width850Height1000");
  assert.equal(text(general("empty-scene")), "SceneShapes0Width0Height0");
  assert.equal(text(general("grid-mode")), "SceneShapes21Width850Height1000CanvasGrid step");
  assert.deepEqual(inputs(general("grid-mode")), [["Grid step", "20"]]);
  assert.equal(text(general("grid-prop-off")), "SceneShapes21Width850Height1000");
});

test("a single element: type, X, Y, W, H, angle and font size (index.tsx:248-345)", () => {
  assert.deepEqual(inputs(byName("text").dom[0]), [["X", "0"], ["Y", "100"], ["W", "40"], ["H", "25"], ["A", "0"], ["F", "20"]]);
  assert.deepEqual(inputs(byName("rotated").dom[0]), [["X", "219.28"], ["Y", "24.5"], ["W", "80"], ["H", "60"], ["A", "28.65"]]);
  // a frame's angle is not editable, so its input is not rendered
  assert.deepEqual(inputs(byName("frame").dom[0]).map(([k]) => k), ["X", "Y", "W", "H"]);
  const type = find(byName("cropping").dom[0], (n) => n.attrs["data-testid"] === "stats-element-type");
  assert.equal(text(type), "Image cropping");
  assert.match(text(byName("cropping").dom[0]), /Uncropped dimensionWidth200Height150/);
});

test("several elements: count, then Mixed where they differ (index.tsx:362-415)", () => {
  assert.deepEqual(inputs(byName("multi-mixed").dom[0]), [["X", "Mixed"], ["Y", "Mixed"], ["W", "Mixed"], ["H", "Mixed"], ["A", "Mixed"]]);
  assert.match(text(byName("group").dom[0]), /Shape propertiesGroupShapes2/);
  // a frame selected with its child hides the element section
  assert.equal(find(byName("frame-and-child").dom[0], (n) => n.attrs.id === "elementStats"), null);
});

test("each section header toggles its bit; the close button calls onClose", () => {
  const clicks = committed().clicks;
  assert.equal(clicks.length, 15);
  for (const c of clicks) {
    const panels = c.appState.stats?.panels ?? 3;
    if (c.control === "close") {
      assert.equal(c.closed, 1);
      assert.deepEqual(c.patches, []);
    } else {
      const bit = c.control === "generalStats" ? 1 : 2;
      assert.deepEqual(c.patches, [{ stats: { open: true, panels: panels ^ bit } }]);
    }
  }
});
