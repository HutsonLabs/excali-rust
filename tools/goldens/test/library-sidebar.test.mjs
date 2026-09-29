// excali-ui's library sidebar fixture and stylesheet (ex-526) are
// upstream's output: tools/goldens/library-sidebar.mjs regenerates both
// from the pinned checkout, byte-stable across runs, and --check fails when
// a committed file differs. The checks below restate research/
// ui-design-system.md 3.6 (the Search and Library tab triggers, the header
// menu's Load, Export, Publish and Reset or Remove, the Personal and
// Excalidraw library sections) and what the issue asks for (drag to canvas,
// add to library) and hold the recorded DOM and effects to them, so a
// generator that lost cases would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "library-sidebar.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [join("tests", "fixtures", "library-sidebar.json"), join("src", "library_sidebar", "library_sidebar.css")];

const scratch = mkdtempSync(join(tmpdir(), "library-sidebar-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));
const byName = (list, name) => list.find((c) => c.name === name);

/** Every element of a DOM, depth first. */
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

const hasClass = (n, c) => (n.attrs.class ?? "").split(" ").includes(c);

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const f of FILES) {
    const first = readFileSync(join(outs[0], f));
    assert.ok(first.equals(readFileSync(join(outs[1], f))), `${f} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, f))), `stale ${f}: run node tools/goldens/library-sidebar.mjs`);
  }
});

test("the header, sections and menu are the research page's (3.6)", () => {
  const { order } = committed();
  assert.deepEqual(order.header.slice(2), ["sidebar-dock", "sidebar-close"]);
  assert.deepEqual(order.sections, ["Personal Library", "Excalidraw Library"]);
  assert.deepEqual(order.menu, ["Open", "Save to...", "Reset library"]);
  const both = byName(committed().cases, "both").dom;
  const triggers = elements(both).filter((n) => hasClass(n, "sidebar-tab-trigger"));
  assert.deepEqual(
    triggers.map((n) => n.children[0].icon),
    ["searchIcon", "LibraryIcon"],
  );
});

test("the search tab forces docking and drops the dock button", () => {
  const dom = byName(committed().cases, "search-tab").dom;
  const island = dom[0];
  assert.ok(hasClass(island, "sidebar--docked"));
  assert.equal(elements(dom).filter((n) => hasClass(n, "sidebar__dock")).length, 0);
});

test("dragging carries item ids; the pending item is added to the library", () => {
  const { interactions } = committed();
  const drag = byName(interactions, "drag-selection").steps[2].effects;
  assert.deepEqual(drag, [
    { dataTransfer: ["application/vnd.excalidrawlib.ids+json", '{"itemIds":["u1","u2","u3","u4","p1"]}'] },
  ]);
  assert.equal(byName(interactions, "drag-pending").steps[0].defaultPrevented, true);
  const add = byName(interactions, "add-to-library").steps[0].effects;
  assert.deepEqual(add[0], { trackEvent: ["element", "addToLibrary", "ui"] });
  assert.deepEqual(
    add[2].setLibrary.map((i) => i.id),
    ["id6", "u1", "u2", "u3", "u4", "p1", "p2"],
  );
});

test("every interaction records its steps' DOM", () => {
  const { interactions } = committed();
  assert.ok(interactions.length >= 25);
  for (const i of interactions) {
    assert.ok(i.steps.length > 0, i.name);
    for (const s of i.steps) assert.ok(Array.isArray(s.dom), `${i.name}: ${s.event}`);
  }
});
