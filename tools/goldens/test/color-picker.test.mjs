// excali-ui's colour picker fixture and stylesheet (ex-523) are upstream's
// output: tools/goldens/color-picker.mjs regenerates both from the pinned
// checkout, byte-stable across runs, and --check fails when a committed
// file differs. The checks below restate what the issue asks for
// (research/ui-design-system.md 2 and 3.12: keys q..b, 1-5, Shift+1-5 and
// i; the top picks and the 5x3 palette of colors.ts) and hold the recorded
// output to them, so a generator that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "color-picker.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "color-picker.json"), join("src", "color_picker", "color_picker.css")];

const scratch = mkdtempSync(join(tmpdir(), "color-picker-test-"));
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

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/color-picker.mjs`);
  }
});

test("palettes and top picks (colors.ts:193-339, research 2)", () => {
  const p = committed().palettes;
  assert.deepEqual(p.colorPickerHotkeyBindings.join(""), "qwertasdfgzxcvb");
  assert.deepEqual(
    p.DEFAULT_ELEMENT_STROKE_COLOR_PALETTE.map((e) => e.name),
    ["transparent", "white", "gray", "black", "bronze", "cyan", "blue", "violet", "grape", "pink", "green", "teal", "yellow", "orange", "red"],
  );
  assert.deepEqual(p.DEFAULT_ELEMENT_STROKE_PICKS, ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"]);
  assert.deepEqual(p.DEFAULT_ELEMENT_BACKGROUND_PICKS, ["transparent", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"]);
  assert.deepEqual(p.DEFAULT_CANVAS_BACKGROUND_PICKS, ["#ffffff", "#f8f9fa", "#f5faff", "#fffce8", "#fdf8f6"]);
  assert.deepEqual(p.STICKY_NOTE_BACKGROUND_PICKS, ["#ffdf6b", "#fcc2d7", "#b2f2bb", "#a5d8ff", "#ffd8a8"]);
  assert.equal(p.COLOR_TOP_PICKS_SLOTS, 5);
});

test("keys q..b, 1-5, Shift+1-5 and i (keyboardNavHandlers.ts)", () => {
  const nav = committed().keyNav;
  const find = (pred) => nav.find(pred);
  const plain = (c) => c.picker === "stroke" && c.section === null && !c.mods;
  // q..b pick the palette entry at the active shade
  const b = find((c) => plain(c) && c.color === "#1e1e1e" && c.key === "b" && !c.custom);
  assert.deepEqual([b.handled, b.changes, b.sections], [true, ["#e03131"], ["baseColors"]]);
  // 1-5 pick a most-used custom colour
  const three = find((c) => plain(c) && c.color === "#1e1e1e" && c.key === "3" && c.custom);
  assert.deepEqual([three.handled, three.changes, three.sections], [true, ["#fedcba"], ["custom"]]);
  // Shift+1-5 pick a shade of the current colour
  const shade = find(
    (c) => c.picker === "stroke" && c.section === null && c.color === "#e03131" && c.code === "Digit1" && c.mods?.includes("shift"),
  );
  assert.deepEqual([shade.handled, shade.changes, shade.sections], [true, ["#fff5f5"], ["shades"]]);
  // i toggles the eye dropper
  const i = find((c) => plain(c) && c.key === "i");
  assert.deepEqual([i.handled, i.eyeDropper], [true, [null]]);
  // an excluded colour's hotkey is dead but handled
  const q = find((c) => c.picker === "sticky" && c.section === null && c.key === "q" && !c.mods);
  assert.deepEqual([q.handled, q.changes ?? []], [true, []]);
});

test("open popup sections (Picker.tsx:176-226)", () => {
  const open = committed().cases.find((c) => c.name === "open-stroke-custom");
  const headings = elements(open.dom)
    .filter((n) => n.attrs.class === "color-picker__heading")
    .map((n) => n.children[0]);
  assert.deepEqual(headings, ["Most used custom colors", "Colors", "Shades", "Hex code"]);
  const palette = elements(open.dom).filter((n) => (n.attrs["data-testid"] ?? "").startsWith("color-") && !n.attrs["data-testid"].startsWith("color-top-pick"));
  assert.equal(palette.length, 15);
});
