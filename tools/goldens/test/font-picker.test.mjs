// excali-ui's font picker fixture and stylesheet (ex-524) are upstream's
// output: tools/goldens/font-picker.mjs regenerates both from the pinned
// checkout, byte-stable across runs, and --check fails when a committed
// file differs. The checks below restate what the issue asks for
// (research/ui-design-system.md 3.13: three top picks, the scene and
// available groups, the "old" badge and the search box) and hold the
// recorded output to them, so a generator that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "font-picker.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "font-picker.json"), join("src", "font_picker", "font_picker.css")];

const scratch = mkdtempSync(join(tmpdir(), "font-picker-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));

/** Every element of a DOM, depth first. */
const elements = (nodes) => {
  const out = [];
  const walk = (n) => {
    if (typeof n !== "object" || n.icon) return;
    out.push(n);
    n.children.forEach(walk);
  };
  nodes.forEach(walk);
  return out;
};

const text = (n) => (typeof n === "string" ? n : n.icon ? "" : n.children.map(text).join(""));

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/font-picker.mjs`);
  }
});

test("three top picks: Hand-drawn, Normal, Code (FontPicker.tsx:42-63)", () => {
  const c = committed();
  assert.equal(c.constants.FONT_TOP_PICKS_SLOTS, 3);
  assert.deepEqual(
    c.constants.DEFAULT_FONTS.map((f) => [f.value, f.text]),
    [[5, "Hand-drawn"], [6, "Normal"], [8, "Code"]],
  );
  const closed = c.cases.find((x) => x.name === "closed");
  const picks = elements(closed.steps[0].dom).filter((n) => n.attrs["data-top-pick-index"] !== undefined);
  assert.deepEqual(picks.map((n) => n.attrs.title), ["Hand-drawn", "Normal", "Code"]);
});

test("scene and available groups, the old badge and the search (FontPickerList.tsx)", () => {
  const open = committed().cases.find((x) => x.name === "open-scene-deprecated");
  const els = elements(open.steps[0].dom);
  assert.deepEqual(
    els.filter((n) => n.attrs.class === "dropdown-menu-group-title").map(text),
    ["In this scene", "Available fonts"],
  );
  assert.deepEqual(els.filter((n) => n.attrs.class === "DropDownMenuItemBadge").map(text), ["old", "old", "old"]);
  assert.equal(els.filter((n) => n.attrs.class === "QuickSearch__input").length, 1);
  const search = committed().cases.find((x) => x.name === "search");
  const empty = search.steps.find((s) => s.step?.search === "zzz");
  assert.deepEqual(elements(empty.dom).filter((n) => n.attrs.class === "empty").map(text), ["No fonts found"]);
});
