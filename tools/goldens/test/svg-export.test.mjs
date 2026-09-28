// excali-scene's export bounds and excali-svg's document shells (ex-406) are
// upstream's output: tools/goldens/svg-export.mjs regenerates them from the
// pinned checkout, byte-stable across runs, --check fails when a committed
// file differs, and the shells agree with upstream's own vitest snapshot of
// exportToSvg (packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap).

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";
import { inflateSync } from "node:zlib";

import { REPO_ROOT, TOOL_DIR, upstreamDir } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "svg-export.mjs");
const BOUNDS = "export-bounds.json";
const SVG = "svg-export.json";
const COMMITTED = {
  [BOUNDS]: join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures", BOUNDS),
  [SVG]: join(REPO_ROOT, "crates", "excali-svg", "tests", "fixtures", SVG),
};

const scratch = mkdtempSync(join(tmpdir(), "svg-export-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args, env = {}) =>
  spawnSync(process.execPath, [GENERATOR, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });

const committed = (file) => JSON.parse(readFileSync(COMMITTED[file], "utf8"));
const scene = (name) => committed(SVG).scenes.find((s) => s.name === name);

/** One snapshot of export.test.ts.snap, as vitest wrote it. */
const snapshot = (name) => {
  const text = readFileSync(
    join(upstreamDir(), "packages", "excalidraw", "tests", "scene", "__snapshots__", "export.test.ts.snap"),
    "utf8",
  );
  const start = text.indexOf(`exports[\`${name}\`] = \``);
  assert.notEqual(start, -1, `no snapshot ${name}`);
  const body = text.slice(text.indexOf("`", text.indexOf("=", start)) + 1);
  return body.slice(0, body.indexOf("`;"));
};

/** The font families of a document's style block, in order, each once. */
const families = (markup) => [...new Set([...markup.matchAll(/font-family: ([^;]+);/g)].map((m) => m[1]))];

/** The scene JSON embedded between the payload comments. */
const payload = (markup) => {
  const base64 = markup.match(/<!-- payload-start -->\s*(.+?)\s*<!-- payload-end -->/)[1];
  const wrapper = JSON.parse(Buffer.from(base64, "base64").toString("latin1"));
  assert.equal(wrapper.encoding, "bstring");
  assert.equal(wrapper.compressed, true);
  return JSON.parse(inflateSync(Buffer.from(wrapper.encoded, "latin1")).toString("utf8"));
};

test("two runs are byte-identical and equal to the committed fixtures", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const file of [BOUNDS, SVG]) {
    const first = readFileSync(join(outs[0], file));
    assert.ok(first.equals(readFileSync(join(outs[1], file))), `${file} differs between runs`);
    assert.ok(first.equals(readFileSync(COMMITTED[file])), `stale ${file}: run node tools/goldens/svg-export.mjs`);
  }
});

test("the fixture's shell has the structure of upstream's snapshot", () => {
  const snap = snapshot("exportToSvg > with default arguments 1");
  const { shell } = scene("fixture");
  // pretty-format sorts attributes; the values are the same
  for (const [name, value] of [
    ["height", "120"],
    ["version", "1.1"],
    ["viewBox", "0 0 120 120"],
    ["width", "120"],
    ["xmlns", "http://www.w3.org/2000/svg"],
  ]) {
    assert.ok(snap.includes(`  ${name}="${value}"`), `snapshot ${name}`);
    assert.ok(shell.includes(` ${name}="${value}"`), `shell ${name}`);
  }
  assert.ok(
    shell.startsWith(
      '<svg version="1.1" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120" width="120" height="120"><!-- svg-source:excalidraw --><metadata></metadata><defs><style class="style-fonts">\n      @font-face { font-family: Excalifont; src: url(',
    ),
  );
  assert.match(snap, /<!-- svg-source:excalidraw -->\n {2}<metadata \/>\n {2}<defs>\n {4}<style\n {6}class="style-fonts"/);
  // upstream's test FontFace gives every face the range U+0000-00FF
  // (setupTests.ts:65-83), so it inlines more faces of each family; the
  // families and their order are the same.
  assert.deepEqual(families(shell), ["Excalifont", "Nunito"]);
  assert.deepEqual(families(snap), families(shell));
  assert.deepEqual(families(scene("fixture-cjk").shell), ["Xiaolai", "Excalifont", "Nunito"]);
  assert.deepEqual(families(snapshot("exportToSvg > with a CJK font 1")), ["Xiaolai", "Excalifont", "Nunito"]);
});

test("the embedded scene is upstream's snapshot payload but for the export source", () => {
  const ours = payload(scene("fixture-embed").shell);
  const theirs = payload(snapshot("exportToSvg > with exportEmbedScene 1"));
  assert.equal(theirs.source, "http://localhost:3000");
  assert.equal(ours.source, committed(SVG).source);
  assert.deepEqual({ ...ours, source: null }, { ...theirs, source: null });
});

test("upstream's export test expectations hold in the shells", () => {
  // "with background color": the first rect is the background
  assert.match(scene("fixture-background").shell, /<\/defs><rect x="0" y="0" width="120" height="120" fill="#abcdef"><\/rect><\/svg>$/);
  // "with exportPadding" and "with scale"
  assert.match(scene("fixture-padding-0").shell, /viewBox="0 0 100 100" width="100" height="100"/);
  assert.match(scene("fixture-scale-2").shell, /viewBox="0 0 100 100" width="200" height="200"/);
  // "with elements that have a link": no fonts, an empty style block
  assert.ok(snapshot("exportToSvg > with elements that have a link 1").startsWith(
    '\n"<!-- svg-source:excalidraw --><metadata></metadata><defs><style class="style-fonts">\n      </style></defs>',
  ));
  assert.match(scene("link").shell, /<metadata><\/metadata><defs><style class="style-fonts">\n {6}<\/style><\/defs><\/svg>$/);
});

test("the documents are upstream's snapshots where the test environments agree", () => {
  // "with elements that have a link" snapshots svgElement.innerHTML: no
  // fonts, so vitest's FontFace mock and export source play no part
  const link = scene("link").document;
  const inner = link.slice(link.indexOf(">") + 1, link.lastIndexOf("</svg>"));
  assert.equal(`\n"${inner}"\n`, snapshot("exportToSvg > with elements that have a link 1"));
  // the elements after the style block of "with exportEmbedScene": the
  // same nodes, whatever the fonts and the embedded source
  const elements = (markup) => markup.slice(markup.indexOf("</defs>"));
  const embed = snapshot("exportToSvg > with exportEmbedScene 1");
  assert.equal(`${elements(scene("fixture-embed").document)}`, `${elements(embed).slice(0, -2)}</svg>`);
  // every document starts with its shell's root, comment and metadata
  for (const s of committed(SVG).scenes) {
    const shell = s.shell.slice(0, s.shell.indexOf("<defs>"));
    assert.ok(s.document.startsWith(shell), s.name);
  }
});

test("the bounds fixture sizes the documents of the svg fixture", () => {
  const bounds = committed(BOUNDS).scenes;
  const svg = committed(SVG).scenes;
  assert.deepEqual(
    bounds.map((s) => s.name),
    svg.map((s) => s.name),
  );
  for (const [i, s] of bounds.entries()) {
    const [, , width, height] = s.canvas;
    const scale = s.appState.exportScale ?? 1;
    assert.ok(svg[i].shell.startsWith(
      `<svg version="1.1" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width * scale}" height="${height * scale}">`,
    ), s.name);
    assert.equal(s.coords.length, s.elements.length);
    assert.equal(s.bounds.length, s.elements.length);
  }
  const frames = bounds.find((s) => s.name === "frames");
  assert.deepEqual(
    frames.labels.map((l) => l.text),
    // newFrameElement stores an empty name as null (newElement.ts:263-278);
    // truncateText keeps the longest prefix that fits with "..." (export.ts:64-96)
    ["Named frame", "Frame", "AI Frame", "Rotated", "A frame...", "two\nlines", "Frame"],
  );
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, SVG);
  writeFileSync(file, readFileSync(file, "utf8").replace('viewBox=\\"0 0 120 120\\"', 'viewBox=\\"0 0 120 121\\"'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /svg-export\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
