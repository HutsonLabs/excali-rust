// The strongest evidence that the goldens are upstream's own numbers:
// upstream's vitest snapshot of the SVG export
// (packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap) holds
// rough.js paths at two decimals for its fixture elements
// (tests/fixtures/elementFixture.ts, resized to 100x100 in
// tests/scene/export.test.ts:32-60). Formatting the golden ops the way
// upstream's SVG renderer does (opsToPath with 2 decimals) must reproduce
// every snapshot path exactly.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

import { golden, opsToPath, upstreamDir } from "./helpers.mjs";

const SNAPSHOT = join(
  "packages",
  "excalidraw",
  "tests",
  "scene",
  "__snapshots__",
  "export.test.ts.snap",
);

const snapshotBlock = (name) => {
  const text = readFileSync(join(upstreamDir(), SNAPSHOT), "utf8");
  const start = text.indexOf(`exports[\`${name}\`] = \``);
  assert.notEqual(start, -1, `snapshot ${name}`);
  const end = text.indexOf("\n`;", start);
  return text.slice(start, end);
};

/** d="..." attributes of every <path> in document order. */
const snapshotPaths = (block) => [...block.matchAll(/\bd="([^"]*)"/g)].map((m) => m[1]);

const caseById = (id) => {
  const found = golden("elements-upstream-fixtures.json").cases.find((c) => c.id === id);
  assert.ok(found, `golden case ${id}`);
  return found;
};

const goldenPaths = (c) =>
  c.shapes.flatMap((s) => {
    assert.equal(s.type, "rough");
    return s.drawable.sets.map((set) => opsToPath(set, 2));
  });

test("exportToSvg 'with default arguments': diamond and ellipse paths", () => {
  const paths = snapshotPaths(snapshotBlock("exportToSvg > with default arguments 1"));
  const ours = [
    ...goldenPaths(caseById("export-test/diamondFixture")),
    ...goldenPaths(caseById("export-test/ellipseFixture")),
  ];
  assert.equal(paths.length, 4, "snapshot has fill + stroke for two elements");
  assert.deepEqual(ours, paths);
});

test("exportToSvg 'with elements that have a link': 214px rectangle paths", () => {
  const paths = snapshotPaths(snapshotBlock("exportToSvg > with elements that have a link 1"));
  const ours = goldenPaths(caseById("elementFixture/rectangleWithLinkFixture"));
  assert.equal(paths.length, 2);
  assert.deepEqual(ours, paths);
});

test("export-test cases carry the export test's element changes", () => {
  for (const id of ["export-test/diamondFixture", "export-test/ellipseFixture"]) {
    const { element, renderConfig } = caseById(id);
    assert.equal(element.width, 100);
    assert.equal(element.height, 100);
    assert.equal(element.seed, 1041657908);
    assert.equal(renderConfig.isExporting, true);
    assert.equal(renderConfig.canvasBackgroundColor, "#ffffff");
  }
});

test("every elementFixture.ts export has a golden case", () => {
  const ids = new Set(golden("elements-upstream-fixtures.json").cases.map((c) => c.id));
  for (const name of [
    "rectangleFixture",
    "embeddableFixture",
    "ellipseFixture",
    "diamondFixture",
    "rectangleWithLinkFixture",
    "textFixture",
  ]) {
    assert.ok(ids.has(`elementFixture/${name}`), name);
  }
  const text = caseById("elementFixture/textFixture");
  assert.deepEqual(text.shapes, [], "text has no roughjs shape (shape.ts:998-1006)");
});
