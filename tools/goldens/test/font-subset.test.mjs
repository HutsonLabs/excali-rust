// The upstream side of the ex-408 font subsetting spike (ADR-010):
// tools/goldens/font-subset.mjs runs upstream's own subsetting from the
// pinned checkout, byte-stable across runs, --check fails when the committed
// file differs, and its export test scenes are upstream's vitest snapshot
// (packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap), whose
// fonts upstream's test setup subsets for real (setupTests.ts:101-125).

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import pako from "pako";

import { wasmModules } from "../font-subset.mjs";

import { REPO_ROOT, TOOL_DIR, upstreamDir } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "font-subset.mjs");
const FILE = "upstream-subsets.json";
const COMMITTED = join(REPO_ROOT, "tools", "font-subset-eval", FILE);

const scratch = mkdtempSync(join(tmpdir(), "font-subset-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(COMMITTED, "utf8"));
const scene = (name) => committed().scenes.find((s) => s.name === name);

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

const rules = (markup) => [...markup.matchAll(/@font-face \{ font-family: [^;]+; src: url\([^)]*\); \}/g)].map((m) => m[0]);

const rule = (d) => `@font-face { font-family: ${d.family}; src: url(data:font/woff2;base64,${d.woff2}); }`;

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/font-subset.mjs");
});

test("--check passes on a fresh output and fails on a stale one", () => {
  const out = join(scratch, "check");
  assert.equal(run(["--out", out]).status, 0);
  assert.equal(run(["--check", "--out", out]).status, 0);
  writeFileSync(join(out, FILE), "{}\n");
  const r = run(["--check", "--out", out]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /stale/);
});

test("the export test scenes are upstream's snapshot, rule for rule and byte for byte", () => {
  for (const [name, snap] of [
    ["export-test-default", "exportToSvg > with default arguments 1"],
    ["export-test-cjk", "exportToSvg > with a CJK font 1"],
  ]) {
    const theirs = rules(snapshot(snap));
    const ours = scene(name).declarations.map(rule);
    assert.ok(theirs.length > 0, snap);
    assert.equal(ours.length, theirs.length, `${name}: rule count`);
    ours.forEach((r, i) => assert.equal(r, theirs[i], `${name}: rule ${i}`));
  }
  // The CJK case: Xiaolai first (Fonts.ts:196-205), then Excalifont, Nunito.
  const families = [...new Set(scene("export-test-cjk").declarations.map((d) => d.family))];
  assert.deepEqual(families, ["Xiaolai", "Excalifont", "Nunito"]);
});

test("every rule is a subset woff2 of the face's file, smaller than the file", () => {
  const fonts = join(upstreamDir(), "packages", "excalidraw", "fonts");
  const g = committed();
  assert.ok(g.scenes.length >= 40);
  let count = 0;
  for (const s of g.scenes) {
    for (const d of s.declarations) {
      const bytes = Buffer.from(d.woff2, "base64");
      assert.equal(bytes.subarray(0, 4).toString("latin1"), "wOF2", `${s.name} ${d.file}`);
      assert.equal(bytes.length, d.size);
      assert.ok(d.size < readFileSync(join(fonts, d.file)).length, `${s.name} ${d.file} not smaller`);
      assert.ok(d.codePoints.length > 0);
      count++;
    }
  }
  assert.ok(count >= 100, `${count} declarations`);
});

test("the scenes the size of real drawings are there, in every inlined family", () => {
  const names = committed().scenes.map((s) => s.name);
  for (const n of ["excalifont", "virgil", "cascadia", "nunito", "lilita", "comic-shanns", "liberation"]) {
    assert.ok(names.includes(`ascii-${n}`), n);
    const d = scene(`ascii-${n}`).declarations;
    assert.ok(d.length >= 1, n);
    // 95 printable ASCII code points, split over the family's range files.
    assert.equal(new Set(d.flatMap((x) => x.codePoints)).size, 95, n);
  }
  assert.ok(scene("cjk-paragraph").declarations.filter((d) => d.family === "Xiaolai").length > 20);
});

test("the wasm gzip sizes are pako's, the same on every machine", () => {
  // node:zlib is Chromium's zlib, whose level-9 output differs between x64
  // and arm64; pako is zlib's deflate in plain JavaScript.
  assert.doesNotMatch(readFileSync(GENERATOR, "utf8"), /from "node:zlib"/);
  const modules = wasmModules(upstreamDir());
  for (const [name, m] of Object.entries(committed().wasm)) {
    const source = readFileSync(join(upstreamDir(), m.file), "utf8");
    const bytes = Buffer.from(source.match(/`([A-Za-z0-9+/=]+)`/)[1], "base64");
    assert.equal(m.bytes, bytes.length, name);
    assert.equal(m.gzip, pako.gzip(bytes, { level: 9 }).length, name);
    assert.deepEqual(modules[name], m, name);
  }
});
