// excali-ui's help dialog fixture and stylesheet (ex-522) are upstream's
// output: tools/goldens/help-dialog.mjs regenerates both from the pinned
// checkout, byte-stable across runs, and --check fails when a committed
// file differs. The checks below restate what the issue asks for
// (research/ui-design-system.md 3.5: the header links and the Tools, View
// and Editor islands) from HelpDialog.tsx and hold the recorded DOM to
// them, so a generator that lost cases or platforms would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "help-dialog.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "help-dialog.json"), join("src", "help_dialog", "help_dialog.css")];

const scratch = mkdtempSync(join(tmpdir(), "help-dialog-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));
const islands = (name) => committed().islands.find((c) => c.name === name).islands;
const row = (name, label) =>
  islands(name)
    .flatMap((i) => i.rows)
    .find((r) => r.label === label);

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/help-dialog.mjs`);
  }
});

test("three islands with upstream's row counts (HelpDialog.tsx:145-516)", () => {
  const linux = islands("linux");
  assert.deepEqual(
    linux.map((i) => [i.caption, i.className, i.rows.length]),
    [
      ["Tools", "HelpDialog__island HelpDialog__island--tools", 28],
      ["View", "HelpDialog__island HelpDialog__island--view", 15],
      ["Editor", "HelpDialog__island HelpDialog__island--editor", 37],
    ],
  );
  const dialog = committed().cases.find((c) => c.name === "linux").dom[0].children[0];
  assert.equal(dialog.attrs.class, "Modal Dialog HelpDialog");
});

test("platform differences (isDarwin, isWindows, isFirefox, clipboard blob)", () => {
  assert.deepEqual(row("linux", "Send to back").keys, [["Ctrl", "Shift", "["]]);
  assert.deepEqual(row("darwin", "Send to back").keys, [["Cmd", "Option", "["]]);
  assert.deepEqual(row("darwin", "Zen mode").keys, [["Option", "Z"]]);
  assert.deepEqual(row("linux", "Redo").keys, [["Ctrl", "Shift", "Z"]]);
  assert.deepEqual(row("windows", "Redo").keys, [["Ctrl", "Y"], ["Ctrl", "Shift", "Z"]]);
  assert.deepEqual(row("firefox", "Command palette").keys, [["Ctrl", "/"]]);
  assert.equal(row("linux-no-clipboard", "Copy to clipboard as PNG"), undefined);
  assert.ok(row("firefox", "Copy to clipboard as PNG"));
  assert.equal(row("linux-no-theme-toggle", "Toggle light/dark theme"), undefined);
  assert.deepEqual(row("linux", "Toggle light/dark theme").keys, [["Alt", "Shift", "D"]]);
});

test("separators: 'or' between alternatives, none for sequences", () => {
  assert.deepEqual(row("linux", "Selection"), { label: "Selection", keys: [["V"], ["1"]], separators: ["or"] });
  assert.deepEqual(row("linux", "Curved arrow"), {
    label: "Curved arrow",
    keys: [["A", "click", "click", "click"]],
    separators: [],
  });
  // "CtrlOrCmd++" ends in "++": the last key is "+"
  assert.deepEqual(row("linux", "Zoom in").keys, [["Ctrl", "+"]]);
});

test("closing clears openMenu, then openDialog (Dialog.tsx:94-99, LayerUI.tsx:577-583)", () => {
  for (const close of committed().close) {
    assert.deepEqual(close.patches, [{ openMenu: null }, { openDialog: null }], close.how);
  }
});
