// excali-ui's command palette fixture and stylesheet (ex-527) are upstream's
// output: tools/goldens/command-palette.mjs regenerates both from the pinned
// checkout, byte-stable across runs, and --check fails when a committed
// file differs. The checks below restate what the issue asks for
// (research/ui-design-system.md 3.14: Ctrl/Cmd+/ and Ctrl/Cmd+Shift+P; the
// categories App, Export, Editor, Tools, Elements, Links, then Library; the
// Elements, Editor and Export lists) from CommandPalette.tsx and hold the
// recorded palette to them, so a generator that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "command-palette.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [
  join("tests", "fixtures", "command-palette.json"),
  join("src", "command_palette", "command_palette.css"),
];

const scratch = mkdtempSync(join(tmpdir(), "command-palette-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));
const kase = (name) => committed().cases.find((c) => c.name === name);
const titles = (name) => kase(name).categories.map((c) => c.title);
const labels = (name, title) =>
  kase(name)
    .categories.find((c) => c.title === title)
    .items.map((i) => i.label);

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/command-palette.mjs`);
  }
});

test("category order: App, Export, Editor, Tools, Elements, Links (CommandPalette.tsx:97-114)", () => {
  assert.deepEqual(titles("links"), ["App", "Export", "Editor", "Tools", "Elements", "Links"]);
  assert.deepEqual(titles("selection"), ["App", "Export", "Editor", "Tools", "Elements"]);
  // library items only while searching, ordered by score
  assert.ok(titles("search-library").includes("Library"));
  assert.ok(!titles("library").includes("Library"));
  assert.ok(!titles("search-one-letter").includes("Library"));
});

test("the Elements, Editor and Export lists (CommandPalette.tsx:310-383)", () => {
  assert.deepEqual(labels("selection-two", "Elements").slice(0, 8), [
    "Group selection",
    "Cut",
    "Copy",
    "Delete",
    "Wrap selection in frame",
    "Copy styles",
    "Paste styles",
    "Bring to front",
  ]);
  assert.deepEqual(labels("default", "Editor").slice(0, 6), [
    "Undo",
    "Redo",
    "Zoom in",
    "Zoom out",
    "Reset zoom",
    "Zoom to fit all elements",
  ]);
  assert.deepEqual(labels("scene", "Export"), [
    "Export image...",
    "Save to disk",
    "Copy to clipboard as PNG",
    "Copy to clipboard as SVG",
  ]);
});

test("Ctrl/Cmd+/ and Ctrl/Cmd+Shift+P toggle the palette", () => {
  const toggle = committed().toggle;
  const find = (darwin, open, key, mods) =>
    toggle.find(
      (t) =>
        t.darwin === darwin &&
        t.open === open &&
        t.key === key &&
        ["ctrlKey", "metaKey", "shiftKey", "altKey"].every((m) => t[m] === !!mods[m]),
    );
  assert.deepEqual(find(false, false, "/", { ctrlKey: true }).openDialog, { name: "commandPalette" });
  assert.deepEqual(find(true, false, "/", { metaKey: true }).openDialog, { name: "commandPalette" });
  assert.deepEqual(find(false, false, "P", { ctrlKey: true, shiftKey: true }).openDialog, { name: "commandPalette" });
  assert.equal(find(false, true, "/", { ctrlKey: true }).openDialog, null);
  assert.equal(find(false, false, "/", { ctrlKey: true, altKey: true }).openDialog, "unchanged");
  assert.equal(find(false, false, "p", { ctrlKey: true }).openDialog, "unchanged");
});

test("recents, search misses and the phone layout are recorded", () => {
  assert.equal(kase("last-used").recents.label, "Zoom in");
  assert.equal(kase("last-used-unavailable").recents.disabled, true);
  assert.equal(kase("search-none").noMatch, true);
  assert.ok(kase("phone").categories.every((c) => c.items.every((i) => i.shortcut === null)));
  const perform = committed().perform.flatMap((p) => p.commands);
  assert.ok(perform.length >= 100, "the fixture lost perform cases");
  assert.ok(perform.every((p) => JSON.stringify(p.effects[0]) === JSON.stringify({ setAppState: { openDialog: null } })));
});
