// The scene fixtures of excali-core's typed codec tests are upstream's
// output: tools/goldens/scene-fixtures.mjs regenerates them from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// files differ.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "scene-fixtures.mjs");
const FIXTURES = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
const NAMES = ["every-type.excalidraw", "unknown-keys-edited.excalidraw", "unknown-keys.excalidraw"];

const scratch = mkdtempSync(join(tmpdir(), "scene-fixtures-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args, env = {}) =>
  spawnSync(process.execPath, [GENERATOR, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });

test("two runs are byte-identical and equal to the committed fixtures", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
    assert.deepEqual(readdirSync(out).sort(), NAMES);
  }
  for (const name of NAMES) {
    const first = readFileSync(join(outs[0], name));
    assert.ok(first.equals(readFileSync(join(outs[1], name))), `${name} differs between runs`);
    assert.ok(
      first.equals(readFileSync(join(FIXTURES, name))),
      `${name} is stale: run node tools/goldens/scene-fixtures.mjs`,
    );
  }
});

test("output is what upstream's calls give: header, types, reseeded nonce", () => {
  const out = join(scratch, "c");
  assert.equal(run(["--out", out]).status, 0);
  const scene = JSON.parse(readFileSync(join(out, "every-type.excalidraw"), "utf8"));
  assert.deepEqual(Object.keys(scene), ["type", "version", "source", "elements", "appState", "files"]);
  assert.equal(scene.source, "https://excalidraw.com");
  assert.deepEqual(
    scene.elements.map((e) => e.type),
    ["rectangle", "diamond", "ellipse", "embeddable", "iframe", "stickynote", "frame",
     "magicframe", "text", "freedraw", "line", "arrow", "arrow", "image"],
  );
  // Only newStickyNoteElement draws a nonce (newElementWith in
  // normalizeStickyNoteStyle); it comes from the reseeded generator.
  const sticky = scene.elements.find((e) => e.id === "sticky");
  assert.equal(sticky.version, 2);
  assert.equal(sticky.versionNonce, 428152832);
  for (const e of scene.elements) if (e !== sticky) assert.equal(e.versionNonce, 0);
});

test("--check reports a stale fixture by name", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, "unknown-keys.excalidraw");
  writeFileSync(file, readFileSync(file, "utf8").replace('"seed": 1,', '"seed": 2,'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /unknown-keys\.excalidraw/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
