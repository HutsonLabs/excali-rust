// The clipboard fixtures of excali-core's clipboard codec tests are
// upstream's output: tools/goldens/clipboard-fixtures.mjs regenerates them
// from the pinned checkout, byte-stable across runs, and --check fails when
// the committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "clipboard-fixtures.mjs");
const NAME = "clipboard.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "clipboard-fixtures-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args, env = {}) =>
  spawnSync(process.execPath, [GENERATOR, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], NAME));
  assert.ok(first.equals(readFileSync(join(outs[1], NAME))), "differs between runs");
  assert.ok(
    first.equals(readFileSync(COMMITTED)),
    `${NAME} is stale: run node tools/goldens/clipboard-fixtures.mjs`,
  );
});

test("output is upstream's clipboard codec in test mode", () => {
  const out = join(scratch, "c");
  assert.equal(run(["--out", out]).status, 0);
  const { serialize, parse } = JSON.parse(readFileSync(join(out, NAME), "utf8"));
  const ser = new Map(serialize.map((c) => [c.id, JSON.parse(c.output)]));
  const par = new Map(parse.map((c) => [c.id, c.result]));

  assert.equal(ser.get("single-rectangle-no-files").type, "excalidraw/clipboard");
  assert.ok(!("files" in ser.get("single-rectangle-no-files")));
  assert.deepEqual(Object.keys(ser.get("every-type-with-files").files), ["file1"]);
  // Orphaned: the frameId resolves to a copied element that is not a frame.
  const cleared = ser.get("frameId-to-copied-non-frame-cleared").elements[1];
  assert.equal(cleared.frameId, null);
  assert.equal(cleared.version, 2);
  assert.equal(cleared.updated, 1);
  assert.equal(ser.get("child-of-copied-frame-kept").elements[1].frameId, "frame");
  assert.deepEqual(Object.keys(ser.get("images-files-in-element-order-js-key-order").files), [
    "42",
    "zeta",
    "alpha",
  ]);

  // deepCopyElement drops own shape/canvas on the detached copy only.
  const [kept, detached, , keptChild] = ser.get("shape-and-canvas-dropped-only-when-cleared").elements;
  assert.deepEqual([kept.shape, kept.canvas], [1, { a: 1 }]);
  assert.ok(!("shape" in detached) && !("canvas" in detached));
  assert.deepEqual(detached.customData, { shape: 3, canvas: 4 });
  assert.equal(keptChild.canvas, "c");
  // JSON.parse reads 1e400 as Infinity: elements, written back as null.
  assert.deepEqual(par.get("number-overflow-elements").elements[0].x, null);
  assert.match(par.get("number-overflow-elements-plain-paste").text, /"x": null/);
  assert.deepEqual(par.get("number-overflow-invalid-json-is-text").keys, ["text"]);

  assert.equal(par.get("type-excalidraw-api-clipboard").programmaticAPI, true);
  assert.equal(par.get("type-excalidraw").programmaticAPI, false);
  assert.deepEqual(par.get("type-excalidrawlib-is-text").keys, ["text"]);
  assert.equal(par.get("upstream-plain-number").text, "123");
  for (const c of parse) assert.ok(c.result.keys.length > 0, c.id);
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, NAME);
  writeFileSync(file, readFileSync(file, "utf8").replace('"seed": 1000', '"seed": 1001'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /clipboard\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
