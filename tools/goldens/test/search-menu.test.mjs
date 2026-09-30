// excali-ui's search menu fixture and stylesheet (ex-708) are upstream's
// output: tools/goldens/search-menu.mjs regenerates both from the pinned
// checkout, byte-stable across runs, and --check fails when a committed
// file differs. The checks below restate research/ui-design-system.md 3.6
// (the search menu shows Frames and Texts result groups) and what the issue
// asks for (Ctrl/Cmd+F opens it) and hold the recorded DOM and results to
// them, so a generator that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "search-menu.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "search-menu.json"), join("src", "search_menu", "search_menu.css")];

const scratch = mkdtempSync(join(tmpdir(), "search-menu-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));

const elements = (nodes) => {
  const out = [];
  const walk = (n) => {
    if (typeof n !== "object" || !n.tag) return;
    out.push(n);
    n.children.forEach(walk);
  };
  nodes.forEach(walk);
  return out;
};

const text = (n) => (typeof n === "string" ? n : n.children ? n.children.map(text).join("") : "");

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/search-menu.mjs`);
  }
});

test("the results are grouped Frames then Texts (research 3.6)", () => {
  const step = committed()
    .interactions.find((i) => i.name === "frames-and-texts")
    .steps.find((s) => s.event === "debounce" && s.effects.length > 1);
  const titles = elements(step.dom).filter((n) => n.attrs.class === "layer-ui__search-result-title");
  assert.deepEqual(titles.map(text), ["Frames", "Texts"]);
  const icons = titles.map((t) => t.children[0].children[0].icon);
  assert.deepEqual(icons, ["frameToolIcon", "TextIcon"]);
});

test("Ctrl/Cmd+F opens the search tab, or focuses its field when open", () => {
  const toggle = Object.fromEntries(committed().toggle.map((c) => [c.name, c]));
  assert.deepEqual(toggle.closed.result.appState.openSidebar, { name: "default", tab: "search" });
  assert.deepEqual(toggle["library-tab"].result.appState.openSidebar, { name: "default", tab: "search" });
  assert.equal(toggle["search-tab"].result, false);
  assert.ok(toggle["search-tab"].effects.includes("focus"));
  assert.equal(toggle.dialog.result, false);
  assert.deepEqual(toggle.dialog.effects, []);
});

test("every interaction step is recorded with its DOM", () => {
  for (const i of committed().interactions) {
    assert.equal(i.steps[0].event, "mount", i.name);
    for (const s of i.steps) {
      if (s.event !== "unmount") assert.equal(s.dom.length, 1, `${i.name} ${s.event}`);
    }
  }
});
