// excali-ui's main menu fixture and stylesheet (ex-520) are upstream's
// output: tools/goldens/main-menu.mjs regenerates both from the pinned
// checkout, byte-stable across runs, and --check fails when a committed
// file differs. The checks below restate research/ui-design-system.md 3.3
// (the default items and their shortcuts, the Light/Dark/System theme
// radio, the Preferences submenu) and hold the recorded trees to it, so a
// generator that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "main-menu.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "main-menu.json"), join("src", "main_menu.css")];

const scratch = mkdtempSync(join(tmpdir(), "main-menu-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));

/** Every element of a tree, depth first. */
const walk = function* (nodes) {
  for (const n of nodes) {
    if (typeof n !== "object" || !n.tag) continue;
    yield n;
    yield* walk(n.children ?? []);
  }
};

const text = (n) => (n.children ?? []).map((c) => (typeof c === "string" ? c : c.tag ? text(c) : "")).join("");

/** [label, shortcut] per menu item, in order. */
const rows = (tree) =>
  [...walk(tree)]
    .filter((n) => n.attrs?.role === "menuitem")
    .map((n) => {
      const label = (n.children ?? []).find((c) => c.class === "dropdown-menu-item__text");
      const shortcut = (n.children ?? []).find((c) => c.class === "dropdown-menu-item__shortcut");
      return [text(label), shortcut ? text(shortcut) : ""];
    });

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/main-menu.mjs`);
  }
});

test("the default menu's items and shortcuts (research 3.3)", () => {
  const menu = committed().menus.find((m) => m.id === "default");
  assert.deepEqual(rows(menu.tree), [
    ["Open", "Ctrl+O"],
    ["Save to...", ""],
    ["Export image...", "Ctrl+Shift+E"],
    ["Find on canvas", "Ctrl+F"],
    ["Help", "?"],
    ["Reset the canvas", ""],
    ["GitHub", ""],
    ["Follow us", ""],
    ["Discord chat", ""],
    ["Dark mode", "Shift+Alt+D"],
  ]);
  const withFile = committed().menus.find((m) => m.id === "file-handle");
  assert.deepEqual(rows(withFile.tree)[1], ["Save to current file", "Ctrl+S"]);
  const slots = JSON.stringify(menu.tree).match(/"action":"(\w+)"/g);
  assert.deepEqual(slots, ['"action":"changeViewBackgroundColor"']);
});

test("the theme radio offers light, dark and system", () => {
  for (const item of committed().items.filter((i) => i.component === "ToggleTheme")) {
    const inputs = [...walk(item.tree)].filter((n) => n.tag === "input");
    assert.deepEqual(
      inputs.map((n) => n.attrs["aria-label"]),
      ["Light mode - Shift+Alt+D", "Dark mode - Shift+Alt+D", "System mode"],
    );
    const checked = inputs.findIndex((n) => "checked" in n.attrs);
    assert.equal(checked, ["light", "dark", "system"].indexOf(item.props.theme), item.id);
    if (item.themeHandler) {
      assert.deepEqual(
        inputs.map((n) => n.on.change),
        [[{ onThemeChange: "light" }], [{ onThemeChange: "dark" }], [{ onThemeChange: "system" }]],
      );
    }
  }
});

test("the preferences submenu (research 3.3)", () => {
  const item = committed().items.find((i) => i.id === "preferences-default");
  const nodes = [...walk(item.tree)];
  const radios = nodes.filter((n) => n.class === "RadioGroup").map((g) => [...walk([g])].filter((n) => n.tag === "input").map((n) => n.attrs.name + ":" + n.attrs["aria-label"]));
  assert.deepEqual(radios, [
    ["boxSelectionMode:Wrap", "boxSelectionMode:Overlap"],
    ["inputDevice:Trackpad", "inputDevice:Mouse"],
  ]);
  assert.deepEqual(rows(item.tree), [
    ["Preferences", ""],
    ["Tool lock", "Q"],
    ["Snap to objects", "Alt+S"],
    ["Toggle grid", "Ctrl+'"],
    ["Zen mode", "Alt+Z"],
    ["View mode", "Alt+R"],
    ["Canvas & Shape properties", "Alt+/"],
    ["Arrow binding", ""],
    ["Snap to midpoints", ""],
    ["Show hints", ""],
  ]);
});
