// excali-ui's shapes toolbar fixture and stylesheet (ex-518) are
// upstream's output: tools/goldens/toolbar.mjs regenerates both from the
// pinned checkout, byte-stable across runs, and --check fails when a
// committed file differs. The checks below restate what the issue asks for
// (research/ui-design-system.md 3.1: the desktop order and grouping, the
// extra-tools dropdown's entries, tooltips reading "Label — R or 2") from
// Toolbar.tsx and Tools.tsx and hold the recorded DOM to them, so a
// generator that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "toolbar.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "toolbar.json"), join("src", "toolbar", "toolbar.css")];

const scratch = mkdtempSync(join(tmpdir(), "toolbar-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));
const byName = (name) => committed().cases.find((c) => c.name === name);

/** Every element of a case's DOM, depth first. */
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

const testIds = (c) => elements(c.dom).map((n) => n.attrs["data-testid"]).filter(Boolean);

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/toolbar.mjs`);
  }
});

test("desktop order and grouping (Toolbar.tsx:262-323, research 3.1)", () => {
  assert.deepEqual(committed().order, [
    "lock",
    "|",
    "hand",
    "selection",
    "rectangle",
    "diamond",
    "ellipse",
    "arrow",
    "line",
    "freedraw",
    "text",
    "stickynote",
    "eraser",
    "|",
    "extra-tools",
  ]);
  // the pen mode button leads once a pen is detected
  assert.equal(testIds(byName("pen-detected")).includes("toolbar-lock"), true);
  const pen = elements(byName("pen-detected").dom).find((n) => n.attrs.class?.includes("ToolIcon__penMode"));
  assert.ok(pen, "pen mode button");
  // a host-forced tool hides the lock and its divider
  assert.equal(testIds(byName("forced-rectangle")).includes("toolbar-lock"), false);
  // lasso replaces selection when preferred
  const lasso = testIds(byName("preferred-lasso"));
  assert.ok(lasso.includes("toolbar-lasso") && !lasso.includes("toolbar-selection"));
});

test("tooltips read 'Label — R or 2' (getToolShortcut, Tools.tsx:183-190)", () => {
  const titles = Object.fromEntries(
    elements(byName("default").dom)
      .filter((n) => n.attrs["data-testid"]?.startsWith("toolbar-"))
      .map((n) => [n.attrs["data-testid"].slice(8), n.attrs.title]),
  );
  assert.deepEqual(titles, {
    lock: "Keep selected tool active after drawing — Q",
    hand: "Hand (panning tool) — H",
    selection: "Selection — V or 1",
    rectangle: "Rectangle — R or 2",
    diamond: "Diamond — D or 3",
    ellipse: "Ellipse — O or 4",
    arrow: "Arrow — A or 5",
    line: "Line — L or 6",
    freedraw: "Draw — P or 7",
    text: "Text — T or 8",
    stickynote: "Sticky note — N",
    eraser: "Eraser — E or 0",
  });
});

test("extra tools: image, frame, embeddable, autoshape, laser, bucket fill, lasso, then Generate", () => {
  const ids = testIds(byName("open-magic-frame"));
  const menu = ids.slice(ids.indexOf("dropdown-menu") + 1);
  assert.deepEqual(menu, [
    "toolbar-image",
    "toolbar-frame",
    "toolbar-embeddable",
    "toolbar-autoshape",
    "toolbar-laser",
    "toolbar-bucketfill",
    "toolbar-lasso",
    // Mermaid→Excalidraw carries the embeddable test id upstream (Toolbar.tsx:196-202)
    "toolbar-embeddable",
    "toolbar-magicframe",
  ]);
  const shortcuts = elements(byName("open").dom)
    .filter((n) => n.attrs.class === "dropdown-menu-item__shortcut")
    .map((n) => n.children[0]);
  assert.deepEqual(shortcuts, ["9", "F", "Shift+X", "K", "B"]);
  // no image entry when the host disables the tool
  assert.equal(testIds(byName("open-no-image-tool")).includes("toolbar-image"), false);
  // with lasso preferred, the toolbar's lasso button is the active one and
  // the menu's lasso entry is not selected (Toolbar.tsx:74-77)
  const items = elements(byName("open-lasso-preferred").dom).filter((n) => n.attrs["data-testid"] === "toolbar-lasso");
  assert.equal(items.length, 2);
  assert.equal(items[0].attrs["aria-pressed"], "true");
  assert.equal(items[1].attrs.class.includes("dropdown-menu-item--selected"), false);
  // closed, the dropdown renders only its trigger
  assert.equal(testIds(byName("default")).includes("dropdown-menu"), false);
});
