// excali-scene's image element goldens (ex-404) are upstream's output:
// tools/goldens/image-elements.mjs regenerates them from the pinned
// checkout, byte-stable across runs, --check fails when the committed file
// differs, and the recorded calls restate what renderElement.ts says about
// images (placeholder colours and icon size, crop, scale after rotation,
// rounded clip, the dark filter for SVG only).

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR, upstreamDir } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "image-elements.mjs");
const FILE = "image-elements.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "image-elements-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args, env = {}) =>
  spawnSync(process.execPath, [GENERATOR, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });

const golden = () => JSON.parse(readFileSync(COMMITTED, "utf8"));
const byId = (g) => Object.fromEntries(g.cases.map((c) => [c.id, c]));
const draws = (c) => c.calls.filter(([name]) => name === "drawImage" || name === "fillRect");

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/image-elements.mjs");
});

test("the placeholders are the SVGs renderElement.ts inlines", () => {
  const source = readFileSync(join(upstreamDir(), "packages", "element", "src", "renderElement.ts"), "utf8");
  const g = golden();
  for (const svg of Object.values(g.placeholders)) {
    assert.ok(source.includes(svg), "placeholder SVG not in renderElement.ts");
    assert.match(svg, /fill(="|:)#888/);
  }
  assert.match(g.placeholders.image, /viewBox="0 0 512 512"/);
  assert.match(g.placeholders.error, /viewBox="0 0 668 668"/);
});

test("placeholder: #E7E7E7 or #2E2E2E box and an icon of min(min(w, h) * 0.4, 100)", () => {
  const cases = golden().cases.filter((c) => draws(c).some(([n, img]) => n === "drawImage" && img.placeholder));
  assert.ok(cases.length >= 8);
  for (const c of cases) {
    const { width: w, height: h, status } = c.element;
    const [[, ...box], [, image, ...dest]] = draws(c);
    assert.deepEqual(box, [0, 0, w, h], c.id);
    const fill = c.calls.findLast(([n, p]) => n === "set" && p === "fillStyle")[2];
    assert.equal(fill, c.theme === "dark" ? "#2E2E2E" : "#E7E7E7", c.id);
    const m = Math.min(w, h);
    const size = Math.min(m, Math.min(m * 0.4, 100));
    assert.deepEqual(dest, [w / 2 - size / 2, h / 2 - size / 2, size, size], c.id);
    assert.equal(image.placeholder, status === "error" ? "error" : "image", c.id);
  }
  const all = byId(golden());
  assert.equal(draws(all["placeholder-icon-cap"])[1][4], 100);
  assert.equal(draws(all["placeholder-small"])[1][4], 12 * 0.4);
});

test("loaded images: crop or natural size, scale after rotate, rounded clip, filter for dark SVG", () => {
  const g = golden();
  const all = byId(g);
  const png = g.files.png;
  assert.deepEqual(draws(all.png)[0].slice(2), [0, 0, png.naturalWidth, png.naturalHeight, 0, 0, 72, 48]);
  assert.deepEqual(draws(all["png-crop"])[0].slice(2), [4, 2, 12, 10, 0, 0, 60, 50]);
  const order = all["png-rotated-flipped"].calls.map(([n]) => n).filter((n) => ["translate", "rotate", "scale"].includes(n));
  assert.deepEqual(order, ["translate", "rotate", "scale", "translate"]);
  assert.deepEqual(all["png-flip-x"].calls.find(([n]) => n === "scale"), ["scale", -1, 1]);
  const radius = (id) => all[id].calls.find(([n]) => n === "roundRect")?.[5];
  assert.equal(radius("png-round-adaptive"), 16); // 64 * 0.25
  assert.equal(radius("png-round-adaptive-large"), 32); // DEFAULT_ADAPTIVE_RADIUS
  assert.equal(radius("png-round-adaptive-value"), 12);
  assert.equal(radius("png-round-proportional"), 16);
  assert.equal(radius("png-round-legacy"), 16);
  assert.equal(radius("png"), undefined);
  const filter = (id) => all[id].calls.find(([n, p]) => n === "set" && p === "filter")?.[2];
  assert.equal(filter("svg-dark"), "invert(93%) hue-rotate(180deg)");
  assert.equal(filter("svg"), undefined);
  assert.equal(filter("png-dark"), undefined);
  assert.equal(all["png-opacity"].calls.find(([n, p]) => n === "set" && p === "globalAlpha")[2], 0.45);
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, FILE);
  writeFileSync(file, readFileSync(file, "utf8").replace('"#E7E7E7"', '"#E7E7E8"'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /image-elements\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
