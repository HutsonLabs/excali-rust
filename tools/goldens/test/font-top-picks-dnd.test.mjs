// excali-ui's font top-picks customisation fixture (ex-538) is upstream's
// output: tools/goldens/font-top-picks-dnd.mjs regenerates it from the
// pinned checkout, byte-stable across runs, and --check fails when the
// committed file differs. The checks below restate FontPicker.test.tsx's
// "top picks drag & drop" cases (a list font dropped on the strip pins it,
// an already pinned one is refused, a pick dropped on another slot
// reorders, a short customised list is padded with the defaults, the tip's
// reset link resets) and hold the recorded output to them, so a generator
// that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "font-top-picks-dnd.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILE = join("tests", "fixtures", "font-top-picks-dnd.json");

const scratch = mkdtempSync(join(tmpdir(), "font-top-picks-dnd-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILE), "utf8"));

const drag = (name) => committed().drags.find((d) => d.name === name);

const changes = (d) => d.steps.flatMap((s) => s.calls).filter((c) => c[0] === "topPicksChange");

const titles = (strip) => strip.children[0].children.filter((c) => c.tag === "button").map((b) => b.attrs.title);

const last = (d) => d.steps[d.steps.length - 1];

test("two runs are byte-identical and equal to the committed file", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), `${FILE} differs between runs`);
  assert.ok(first.equals(readFileSync(join(CRATE, FILE))), `stale ${FILE}: run node tools/goldens/font-top-picks-dnd.mjs`);
});

test("pins a font dragged from the list, replacing the hovered slot", () => {
  const d = drag("pin-list-font");
  assert.deepEqual(changes(d), [["topPicksChange", [5, 7, 8]]]);
  assert.deepEqual(titles(last(d).strip), ["Hand-drawn", "Lilita One", "Code"]);
  // pinning doesn't pick the font, and the rows it passes aren't previewed
  assert.equal(last(d).state.selectedFontFamily, 5);
  assert.deepEqual(d.steps.find((s) => s.step.hover !== undefined).calls, []);
});

test("refuses pinning an already pinned font", () => {
  const d = drag("pin-duplicate");
  assert.deepEqual(changes(d), []);
  assert.equal(last(d).state.fontTopPicks, null);
});

test("reorders picks dragged within the strip", () => {
  const d = drag("reorder-forward");
  assert.deepEqual(changes(d), [["topPicksChange", [6, 8, 5]]]);
  assert.deepEqual(titles(last(d).strip), ["Normal", "Code", "Hand-drawn"]);
});

test("pads a short customized list with unpicked defaults", () => {
  const d = drag("pads-short-list");
  assert.deepEqual(titles(d.steps[0].strip), ["Lilita One", "Hand-drawn", "Normal"]);
  assert.deepEqual(changes(d), [["topPicksChange", [7, 8, 6]]]);
});

test("resets customized picks from the tip's reset link and the strip's menu", () => {
  const tip = committed().tips.find((t) => t.name === "open-customized");
  assert.deepEqual(tip.calls, [["topPicksChange", null]]);
  assert.equal(tip.state.openPopup, "fontFamily");
  const menu = committed().menus.find((m) => m.name === "customized");
  assert.deepEqual(menu.calls, [["topPicksChange", null]]);
  assert.deepEqual(committed().menus.find((m) => m.name === "defaults").calls, []);
});
