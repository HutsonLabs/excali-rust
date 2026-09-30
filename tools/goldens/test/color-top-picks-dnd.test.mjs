// excali-ui's colour top-picks customisation fixture and stylesheet
// (ex-536) are upstream's output: tools/goldens/color-top-picks-dnd.mjs
// regenerates both from the pinned checkout, byte-stable across runs, and
// --check fails when a committed file differs. The checks below restate
// what the issue asks for (a swatch or the active colour dropped on the
// strip pins it, a pick dropped on another slot reorders, the strip's
// context menu resets it, the popup shows the tip) and hold the recorded
// output to them, so a generator that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "color-top-picks-dnd.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "color-top-picks-dnd.json"), join("src", "top_picks_dnd", "top_picks_dnd.css")];

const scratch = mkdtempSync(join(tmpdir(), "color-top-picks-dnd-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));

const drag = (name) => committed().drags.find((d) => d.name === name);

const updates = (d) => d.steps.flatMap((s) => s.calls).filter((c) => c.updateData);

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/color-top-picks-dnd.mjs`);
  }
});

test("a swatch or the active colour dropped on the strip pins it", () => {
  assert.deepEqual(updates(drag("pin-palette-colour")), [
    { updateData: { colorTopPicks: { elementStroke: ["#1e1e1e", "#6741d9", "#2f9e44", "#1971c2", "#f08c00"] } } },
  ]);
  assert.deepEqual(updates(drag("pin-active-colour")), [
    { updateData: { colorTopPicks: { elementStroke: ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#123456"] } } },
  ]);
  // the drop suppresses the click that follows it
  const click = drag("pin-palette-colour").steps.find((s) => s.step.click);
  assert.equal(click.clickReached, false);
  // an already pinned colour is refused
  assert.deepEqual(updates(drag("pin-duplicate")), []);
  assert.deepEqual(updates(drag("pin-duplicate-other-notation")), []);
});

test("a pick dropped on another slot reorders the strip", () => {
  assert.deepEqual(updates(drag("reorder-forward")), [
    { updateData: { colorTopPicks: { elementStroke: ["#e03131", "#2f9e44", "#1971c2", "#1e1e1e", "#f08c00"] } } },
  ]);
  const preview = drag("reorder-forward").steps.find((s) => s.step.move?.[0] === 116).strip;
  const offsets = preview.children.filter((c) => c.tag === "button").map((c) => c.style.transform ?? null);
  assert.deepEqual(offsets, ["translateX(84px)", "translateX(-28px)", "translateX(-28px)", "translateX(-28px)", null]);
});

test("the context menu resets the strip and the popup shows the tip", () => {
  const menu = committed().menus.find((m) => m.name === "stroke-customized");
  assert.deepEqual(menu.calls, [{ updateData: { colorTopPicks: { elementStroke: null, elementBackground: ["#abcdef"] } } }]);
  const tip = (name) => {
    const out = [];
    const walk = (n) => {
      if (typeof n !== "object" || n.icon) return;
      if (n.attrs.class === "top-picks-dnd__tip") out.push(n);
      n.children.forEach(walk);
    };
    committed().cases.find((c) => c.name === name).dom.forEach(walk);
    return out;
  };
  assert.deepEqual(tip("open-stroke")[0].children, ["Tip: drag any color onto your top picks to pin it"]);
  assert.equal(tip("open-stroke-customized")[0].children[2].children[0], "Reset");
  assert.deepEqual(tip("open-compact-stroke"), []);
});
