// excali-ui's phone layout fixture and stylesheet (ex-702) are upstream's
// output: tools/goldens/mobile-menu.mjs regenerates both from the pinned
// checkout, byte-stable across runs, and --check fails when a committed
// file differs. The checks below restate what the issue asks for
// (research/ui-design-system.md 3.7: the mobile toolbar's order, its 36 px
// buttons, text, image and frame shown as the width allows, the "…" menu's
// entries; the top and bottom bars) from MobileToolbar.tsx and
// MobileMenu.tsx and hold the recorded DOM to them.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "mobile-menu.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "mobile-menu.json"), join("src", "mobile_menu", "mobile_menu.css")];

const scratch = mkdtempSync(join(tmpdir(), "mobile-menu-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));
const toolbarCase = (name) => committed().toolbar.find((c) => c.name === name);
const menuCase = (name) => committed().menu.find((c) => c.name === name);

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
const classes = (c) => elements(c.dom).map((n) => n.attrs.class ?? "");

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/mobile-menu.mjs`);
  }
});

test("toolbar order (MobileToolbar.tsx:168-240, research 3.7)", () => {
  assert.deepEqual(committed().order, ["hand", "selection", "freedraw", "eraser", "rectangle", "arrow", "text", "extra-tools"]);
  // MIN_WIDTH = 7 * 36 + 6 * 4; text, image, frame each need 40 more
  const outside = (name) => {
    const ids = testIds(toolbarCase(name));
    return ["toolbar-text", "toolbar-image", "toolbar-frame"].filter((id) => ids.includes(id));
  };
  assert.deepEqual(outside("width-315"), []);
  assert.deepEqual(outside("width-316"), ["toolbar-text"]);
  assert.deepEqual(outside("width-356"), ["toolbar-text", "toolbar-image"]);
  assert.deepEqual(outside("width-396"), ["toolbar-text", "toolbar-image", "toolbar-frame"]);
});

test("the '…' menu holds what is not outside, then Generate", () => {
  const menu = (name) => {
    const ids = testIds(toolbarCase(name));
    return ids.slice(ids.indexOf("dropdown-menu") + 1);
  };
  assert.deepEqual(menu("extra-open-0"), [
    "toolbar-text",
    "toolbar-image",
    "toolbar-stickynote",
    "toolbar-frame",
    "toolbar-embeddable",
    "toolbar-autoshape",
    "toolbar-laser",
    "toolbar-bucketfill",
    "toolbar-embeddable",
  ]);
  assert.equal(menu("extra-open-442").includes("toolbar-frame"), false);
  assert.equal(menu("extra-open-magic-frame").at(-1), "toolbar-magicframe");
  assert.equal(menu("extra-open-magic-frame-ai-disabled").includes("toolbar-magicframe"), false);
  // a phone's dropdown is a column aligned to the trigger's start
  const content = elements(toolbarCase("extra-open-0").dom).find((n) => n.attrs["data-testid"] === "dropdown-menu");
  assert.equal(content.attrs["data-align"], "start");
  assert.ok(content.attrs.class.includes("dropdown-menu--mobile"));
});

test("popovers show their options and the trigger shows the remembered one", () => {
  const ids = testIds(toolbarCase("open-rectangle"));
  assert.deepEqual(ids.slice(4, 8), ["toolbar-rectangle", "toolbar-rectangle", "toolbar-diamond", "toolbar-ellipse"]);
  assert.ok(classes(toolbarCase("open-rectangle")).includes("tool-popover-content"));
  const trigger = elements(toolbarCase("active-diamond").dom).find((n) => n.attrs["data-testid"] === "toolbar-rectangle");
  assert.deepEqual(trigger.children[0].children[0], { icon: "DiamondIcon" });
  assert.equal(trigger.attrs["aria-pressed"], "true");
});

test("MobileMenu: top bar and bottom bar (MobileMenu.tsx:66-201)", () => {
  const dom = menuCase("default").dom;
  const top = dom.find((n) => n.attrs.class?.includes("App-top-bar"));
  const bottom = dom.find((n) => n.attrs.class === "App-bottom-bar");
  assert.ok(top && bottom);
  assert.equal(bottom.attrs["data-viewport-ui"], "bottom");
  const slots = (n) => elements([n]).map((e) => e.attrs["data-slot"]).filter(Boolean);
  assert.deepEqual(slots(top), ["MainMenu", "DefaultSidebarTrigger"]);
  assert.deepEqual(slots(bottom), ["MobileShapeActions", "MobileToolbar"]);
  // view mode: no bottom bar, the exit button top right
  const view = menuCase("view-mode").dom;
  assert.equal(view.some((n) => n.attrs.class === "App-bottom-bar"), false);
  assert.ok(classes(menuCase("view-mode")).includes("disable-view-mode"));
});
