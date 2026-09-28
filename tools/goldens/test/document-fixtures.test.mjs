// The D1 round-trip golden of excali-core (ex-g101) is upstream's output:
// tools/goldens/document-fixtures.mjs regenerates it from the pinned
// checkout, byte-stable across runs, and --check fails when the committed
// file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "document-fixtures.mjs");
const NAME = "document-round-trip.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures", NAME);
const UPSTREAM_FIXTURES = "fixtures/upstream/packages/excalidraw/tests/fixtures";

const scratch = mkdtempSync(join(tmpdir(), "document-fixtures-test-"));
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
  assert.ok(first.equals(readFileSync(COMMITTED)), `${NAME} is stale: run node tools/goldens/document-fixtures.mjs`);
});

test("every scene-bearing upstream fixture has a case", () => {
  const { cases } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const covered = new Set(cases.flatMap((c) => c.files));
  const sceneBearing = readdirSync(join(REPO_ROOT, UPSTREAM_FIXTURES)).filter(
    (name) =>
      name.endsWith(".excalidrawlib") ||
      (name.includes("_embedded_") && /\.(png|svg)$/.test(name)) ||
      name === "diagramFixture.ts" ||
      name === "elementFixture.ts",
  );
  assert.equal(sceneBearing.length, 7);
  for (const name of sceneBearing) assert.ok(covered.has(`${UPSTREAM_FIXTURES}/${name}`), name);
});

test("output is upstream's load and save in test mode", () => {
  const { cases, edges } = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const byId = new Map(cases.map((c) => [c.id, c]));

  // diagramFixture: three elements sharing one id; restoreElements gives
  // the duplicates randomId() (id0, id1 in test mode), syncInvalidIndices
  // gives indices a0..a2 and bumps each version with a nonce from
  // Random(1) (48271 first).
  const diagram = JSON.parse(byId.get("diagramFixture").output);
  assert.deepEqual(Object.keys(diagram), ["type", "version", "source", "elements", "appState", "files"]);
  assert.deepEqual(diagram.elements.map((e) => e.id), ["vWrqOAfkind2qcm7LDAGZ", "id0", "id1"]);
  assert.deepEqual(diagram.elements.map((e) => e.type), ["diamond", "ellipse", "rectangle"]);
  assert.deepEqual(diagram.elements.map((e) => e.index), ["a0", "a1", "a2"]);
  assert.deepEqual(diagram.elements.map((e) => e.version), [121, 121, 121]);
  assert.equal(diagram.elements[0].versionNonce, 48271);
  assert.deepEqual(diagram.appState, {
    gridSize: 20,
    gridStep: 5,
    gridModeEnabled: false,
    viewBackgroundColor: "#ffffff",
    lockedMultiSelections: {},
  });

  // Upstream keeps an unknown element key and drops an unknown top-level one.
  const unknown = byId.get("diagramFixture-unknown-keys");
  assert.ok("futureTopLevel" in JSON.parse(unknown.input));
  const written = JSON.parse(unknown.output);
  assert.ok(!("futureTopLevel" in written));
  assert.deepEqual(written.elements[0].futureElementKey, { nested: [1, 2, { z: 1, a: 2 }] });

  // Every fixture's reload is a fixed point.
  for (const c of cases) assert.equal(c.reload, c.output, c.id);

  const edge = new Map(edges.map((c) => [c.id, c]));
  assert.equal(edge.get("not-a-scene").error, "Error: invalid file");
  assert.deepEqual(Object.keys(JSON.parse(edge.get("files-of-live-images-only").output).files), ["42", "f2"]);
  // files[fileId] as JS indexes an array, a string, a number and an object
  // with an own __proto__ key (blob.ts keeps any truthy files).
  const files = (id) => JSON.parse(edge.get(id).output).files;
  assert.deepEqual(files("files-array"), {
    0: { mimeType: "image/png", id: "f0", dataURL: "data:image/png;base64,AA==", created: 1 },
    2: { mimeType: "image/png", id: "f2", dataURL: "data:image/png;base64,AA==", created: 1 },
    length: 3,
  });
  assert.deepEqual(files("files-string"), { 0: "a", 1: "\ud83d", 2: "\ude00", length: 4 });
  assert.match(edge.get("files-string").output, /"1": "\\ud83d"/);
  assert.deepEqual(files("files-number"), {});
  assert.deepEqual(files("files-own-proto-key"), { f1: { id: "f1" } });
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, NAME);
  writeFileSync(file, readFileSync(file, "utf8").replace("vWrqOAfkind2qcm7LDAGZ", "changed"));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /document-round-trip\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
