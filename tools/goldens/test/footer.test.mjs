// excali-ui's footer fixture and stylesheet (ex-521) are upstream's output:
// tools/goldens/footer.mjs regenerates both from the pinned checkout,
// byte-stable across runs, and --check fails when a committed file
// differs. The checks below restate what the issue asks for
// (research/ui-design-system.md 3.4: zoom actions and undo/redo on the
// left, the centre tunnel, the help button on the right, the exit-zen-mode
// button) from Footer.tsx and Actions.tsx and hold the recorded DOM to
// them, so a generator that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "footer.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "footer.json"), join("src", "footer", "footer.css")];

const scratch = mkdtempSync(join(tmpdir(), "footer-test-"));
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

const has = (c, cls) => elements(c.dom).some((n) => (n.attrs.class ?? "").split(" ").includes(cls));
const button = (c, label) => elements(c.dom).find((n) => n.tag === "button" && n.attrs["aria-label"] === label);

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/footer.mjs`);
  }
});

test("order (Footer.tsx:37-91, research 3.4)", () => {
  assert.deepEqual(committed().order, [
    "zoomOut",
    "resetZoom",
    "zoomIn",
    "undo",
    "redo",
    "FooterCenter",
    "WelcomeScreenHelpHint",
    "help",
    "exitZenMode",
  ]);
  const footer = byName("default").dom[0];
  assert.equal(footer.tag, "footer");
  assert.equal(footer.attrs.role, "contentinfo");
  assert.equal(footer.attrs.class, "layer-ui__wrapper__footer App-menu App-menu_bottom");
});

test("tooltips (actionCanvas.tsx, Actions.tsx:891-910, HelpButton.tsx)", () => {
  assert.deepEqual(
    committed().tooltips.map((t) => [t.button, t.label]),
    [
      ["Zoom out", "Zoom out — Ctrl+-"],
      ["Reset zoom", "Reset zoom"],
      ["Zoom in", "Zoom in — Ctrl++"],
      ["Undo", "Undo"],
      ["Redo", "Redo"],
      ["Help", "Help — ?"],
    ],
  );
});

test("zoom label and limits (MIN_ZOOM 0.1, MAX_ZOOM 30)", () => {
  const label = (name) => button(byName(name), "Reset zoom").children.join("");
  assert.equal(label("default"), "100%");
  assert.equal(label("zoom-rounding-down"), "123%");
  assert.equal(label("zoom-half"), "50%");
  assert.equal(button(byName("zoom-min"), "Zoom out").attrs.disabled, "");
  assert.equal(button(byName("zoom-just-above-min"), "Zoom out").attrs.disabled, undefined);
  assert.equal(button(byName("zoom-max"), "Zoom in").attrs.disabled, "");
  assert.equal(button(byName("zoom-just-below-max"), "Zoom in").attrs.disabled, undefined);
});

test("undo and redo follow the history's stacks", () => {
  assert.equal(button(byName("default"), "Undo").attrs.disabled, "");
  assert.equal(button(byName("undo"), "Undo").attrs.disabled, undefined);
  assert.equal(button(byName("undo"), "Redo").attrs.disabled, "");
  assert.equal(button(byName("redo"), "Redo").attrs.disabled, undefined);
});

test("what each setting hides (Footer.tsx conditions)", () => {
  // view mode: no undo/redo
  assert.equal(has(byName("view-mode"), "undo-redo-buttons"), false);
  assert.equal(has(byName("view-mode"), "zoom-actions"), true);
  // navigation off: no zoom actions; without the default UI nothing left
  assert.equal(has(byName("navigation-disabled"), "zoom-actions"), false);
  assert.equal(has(byName("no-default-ui-navigation-disabled"), "layer-ui__wrapper__footer-left"), false);
  // no default UI: zoom only, no help, no exit-zen button
  const bare = byName("no-default-ui");
  assert.equal(has(bare, "zoom-actions"), true);
  assert.equal(has(bare, "undo-redo-buttons"), false);
  assert.equal(has(bare, "help-icon"), false);
  assert.equal(has(bare, "disable-zen-mode"), false);
  // the welcome screen keeps the right side for its hint
  assert.equal(has(byName("no-default-ui-welcome"), "layer-ui__wrapper__footer-right"), true);
  // zen mode: the transitions' classes; the exit button visible on request
  const zen = byName("zen-mode-exit-button");
  assert.equal(has(zen, "layer-ui__wrapper__footer-left--transition-left"), true);
  assert.equal(has(zen, "layer-ui__wrapper__footer-left--transition-bottom"), true);
  assert.equal(has(zen, "transition-right"), true);
  assert.equal(has(zen, "disable-zen-mode--visible"), true);
  assert.equal(has(byName("zen-mode"), "disable-zen-mode--visible"), false);
});

test("clicks run the controls' actions", () => {
  const ran = Object.fromEntries(committed().clicks.filter((c) => c.case === "default").map((c) => [c.control, c.action]));
  assert.deepEqual(ran, {
    zoomOut: "zoomOut",
    resetZoom: "resetZoom",
    zoomIn: "zoomIn",
    undo: "undo",
    redo: "redo",
    help: "toggleShortcuts",
    exitZenMode: "zenMode",
  });
});
