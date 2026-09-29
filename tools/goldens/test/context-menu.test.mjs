// excali-ui's context menu fixture and stylesheet (ex-525) are upstream's
// output: tools/goldens/context-menu.mjs regenerates both from the pinned
// checkout, byte-stable across runs, and --check fails when a committed
// file differs. The checks below restate research/ui-design-system.md 3.10
// (the canvas and element menus, the view-mode lists, z-order on desktop
// only) and hold the recorded items to it, so a generator that lost cases
// would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "context-menu.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "context-menu.json"), join("src", "context_menu.css")];

const scratch = mkdtempSync(join(tmpdir(), "context-menu-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));

const items = (id) => {
  const c = committed().cases.find((x) => x.id === id);
  assert.ok(c, id);
  return c.items.join(" ");
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
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/context-menu.mjs`);
  }
});

test("the canvas menu (research 3.10)", () => {
  assert.equal(
    items("canvas-empty"),
    "paste | copyAsPng copyAsSvg copyText | selectAll unlockAllElements | " +
      "gridMode objectsSnapMode arrowBinding midpointSnapping zenMode viewMode stats",
  );
  assert.equal(items("canvas-view-mode"), "copyAsPng copyAsSvg gridMode zenMode viewMode stats");
});

test("the element menu, z-order on desktop only (research 3.10)", () => {
  const desktop =
    "| cut copy paste | selectAllElementsInFrame removeAllElementsFromFrame wrapSelectionInFrame | " +
    "cropEditor | copyAsPng copyAsSvg copyText | copyStyles pasteStyles | " +
    "group autoResize unbindText bindText wrapTextInContainer ungroup | addToLibrary | " +
    "sendBackward bringForward sendToBack bringToFront | flipHorizontal flipVertical | " +
    "toggleLinearEditor | hyperlink copyElementLink | duplicateSelection toggleElementLock | " +
    "deleteSelectedElements";
  assert.equal(items("element-rectangle"), desktop);
  const touch = desktop.replace("| sendBackward bringForward sendToBack bringToFront ", "");
  assert.equal(items("element-rectangle-phone"), touch);
  assert.equal(items("element-rectangle-tablet"), touch);
  assert.equal(items("element-view-mode"), "copy copyAsPng copyAsSvg copyText");
});

test("every case renders a row per item whose predicate holds", () => {
  for (const c of committed().cases) {
    const rows = JSON.stringify(c.tree).match(/"data-testid":"\w+"/g) ?? [];
    assert.ok(rows.length > 0, c.id);
    const names = rows.map((r) => r.slice(15, -1));
    for (const n of names) assert.ok(c.items.includes(n), `${c.id}: ${n}`);
  }
});
