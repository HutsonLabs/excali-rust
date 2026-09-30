// excali-ui's icon set (ex-517) is upstream's output: tools/goldens/icons.mjs
// renders every icons.tsx export from the pinned checkout into
// tests/fixtures/icons.json and src/icons/generated.rs, byte-stable across
// runs, and --check fails when either committed file differs. The checks
// below restate what the source says (research/ui-design-system.md section
// 4, icons.tsx at the pin) so a generator that lost or mislabelled icons
// would be noticed.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR, upstreamDir } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "icons.mjs");
const CRATE = join(REPO_ROOT, "crates", "excali-ui");
const FILES = [
  join("tests", "fixtures", "icons.json"),
  join("src", "icons", "generated.rs"),
  join("src", "icons", "icons.css"),
];

const scratch = mkdtempSync(join(tmpdir(), "icons-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args) => spawnSync(process.execPath, [GENERATOR, ...args], { encoding: "utf8" });

const committed = () => JSON.parse(readFileSync(join(CRATE, FILES[0]), "utf8"));

const source = () =>
  readFileSync(join(upstreamDir(), "packages", "excalidraw", "components", "icons.tsx"), "utf8");

test("two runs are byte-identical and equal to the committed files", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  for (const file of FILES) {
    const first = readFileSync(join(outs[0], file));
    assert.ok(first.equals(readFileSync(join(outs[1], file))), `${file} differs between runs`);
    assert.ok(first.equals(readFileSync(join(CRATE, file))), `${file} stale: run node tools/goldens/icons.mjs`);
  }
  assert.equal(run(["--check"]).status, 0);
});

test("207 icons: every `export const` of icons.tsx but the two helpers, in order", () => {
  const exports = [...source().matchAll(/^export const (\w+)/gm)].map((m) => m[1]);
  assert.equal(exports.length, 209);
  const icons = exports.filter((n) => n !== "iconFillColor" && n !== "createIcon");
  assert.equal(icons.length, 207);
  assert.deepEqual(
    committed().icons.map((i) => i.name),
    icons,
  );
});

test("mirror: the icons created with `mirror: true`", () => {
  // Each `mirror: true` closes the createIcon call of the nearest export
  // above it.
  const src = source();
  const mirrored = [];
  for (const m of src.matchAll(/mirror: true/g)) {
    const before = [...src.slice(0, m.index).matchAll(/^export const (\w+)/gm)];
    mirrored.push(before.at(-1)[1]);
  }
  assert.deepEqual(mirrored, ["questionCircle", "clone", "GroupIcon", "UngroupIcon"]);
  const icons = committed().icons;
  assert.deepEqual(
    icons.filter((i) => i.mirror).map((i) => i.name).sort(),
    [...mirrored].sort(),
  );
  for (const icon of icons) {
    const markup = icon.markup ?? icon.light ?? "";
    assert.equal(markup.includes('class="rtl-mirror"'), icon.mirror, icon.name);
  }
});

test("presets: tabler 24×24, modified tabler 20×20, arrowhead preview 40×20", () => {
  const icons = committed().icons;
  const count = (p) => icons.filter((i) => i.preset === p).length;
  // Uses of each preset object in icons.tsx, directly or spread.
  const src = source();
  const uses = (name) => (src.match(new RegExp(`\\b${name}\\b`, "g")) ?? []).length - 1;
  // FontFamilyCodeIcon = codeIcon (icons.tsx:1748): one use, two icons.
  assert.equal(count("tabler"), uses("tablerIconProps") + 1);
  assert.equal(count("modifiedTabler"), uses("modifiedTablerIconProps"));
  assert.equal(count("arrowheadPreview"), uses("arrowheadPreviewIconProps"));
  const box = { tabler: "0 0 24 24", modifiedTabler: "0 0 20 20", arrowheadPreview: "0 0 40 20" };
  for (const icon of icons.filter((i) => i.preset)) {
    const markup = icon.markup ?? icon.light;
    assert.ok(markup.includes(`viewBox="${box[icon.preset]}"`), icon.name);
  }
  const tabler = icons.find((i) => i.name === "PlusPromoIcon").markup;
  assert.ok(
    tabler.startsWith(
      '<svg aria-hidden="true" focusable="false" role="img" viewBox="0 0 24 24" class="" fill="none" stroke-width="2" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round">',
    ),
    tabler,
  );
  // The preset tag never reaches the SVG.
  for (const icon of icons) assert.ok(!JSON.stringify(icon).includes("__preset"), icon.name);
});

test("kinds: themed components, path arrays and the empty icon", () => {
  const icons = committed().icons;
  const byName = Object.fromEntries(icons.map((i) => [i.name, i]));
  assert.equal(icons.filter((i) => i.kind === "themed").length, 21);
  assert.equal(icons.filter((i) => i.kind === "paths").length, 2);
  // handlerColor: #fff light, #1e1e1e dark (icons.tsx:18-19).
  for (const name of ["GroupIcon", "UngroupIcon"]) {
    assert.ok(byName[name].light.includes('"#fff"'), name);
    assert.ok(byName[name].dark.includes('"#1e1e1e"'), name);
  }
  assert.equal(byName.bucketFillIconSvgPaths.paths.length, 3);
  assert.equal(byName.emptyIcon.markup, '<div style="width:1rem;height:1rem"></div>');
  // FontFamilyCodeIcon = codeIcon (icons.tsx:1748).
  assert.equal(byName.FontFamilyCodeIcon.preset, "tabler");
  for (const icon of icons.filter((i) => i.kind === "static" && i.name !== "emptyIcon")) {
    assert.ok(icon.markup.startsWith('<svg aria-hidden="true" focusable="false" role="img" viewBox="0 0 '), icon.name);
  }
});

test("icons.css: styles.scss's `.rtl-mirror` rule flips a mirrored icon in an rtl document", () => {
  // css/styles.scss:679-683: `.rtl-mirror { :root[dir="rtl"] & { transform: scaleX(-1); } }`
  // inside `.excalidraw`.
  const scss = readFileSync(join(upstreamDir(), "packages", "excalidraw", "css", "styles.scss"), "utf8");
  assert.match(scss, /\.rtl-mirror \{\n\s+:root\[dir="rtl"\] & \{\n\s+transform: scaleX\(-1\);/);
  const css = readFileSync(join(CRATE, FILES[2]), "utf8");
  assert.ok(css.endsWith(":root[dir=rtl] .excalidraw .rtl-mirror {\n  transform: scaleX(-1);\n}\n"), css);
});
