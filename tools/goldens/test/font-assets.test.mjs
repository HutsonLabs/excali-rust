// excali-text's font asset goldens (ex-307) are upstream's output:
// tools/goldens/font-assets.mjs regenerates them from the pinned checkout,
// byte-stable across runs, --check fails when the committed file differs,
// and the fixture pins facts readable in upstream's font sources.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";
import { build } from "esbuild";

import { fontUri, upstreamDir } from "../lib/upstream.mjs";

const GENERATOR = join(TOOL_DIR, "font-assets.mjs");
const FILE = "font-assets.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures", FILE);

const scratch = mkdtempSync(join(tmpdir(), "font-assets-test-"));
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
  const first = readFileSync(join(outs[0], FILE));
  assert.ok(first.equals(readFileSync(join(outs[1], FILE))), "differs between runs");
  assert.ok(first.equals(readFileSync(COMMITTED)), "stale: run node tools/goldens/font-assets.mjs");
});

test("the registry is Fonts.init's, in its order, naming every upstream font file once", () => {
  const g = JSON.parse(readFileSync(COMMITTED, "utf8"));
  // Fonts.ts:398-411
  assert.deepEqual(
    g.registered.map((f) => [f.id, f.family]),
    [
      [3, "Cascadia"],
      [8, "Comic Shanns"],
      [5, "Excalifont"],
      [2, "Helvetica"],
      [9, "Liberation Sans"],
      [7, "Lilita One"],
      [6, "Nunito"],
      [1, "Virgil"],
      [100, "Xiaolai"],
      [1000, "Segoe UI Emoji"],
    ],
  );
  const byId = Object.fromEntries(g.registered.map((f) => [f.id, f]));
  assert.equal(byId[5].faces.length, 7);
  assert.equal(byId[100].faces.length, 209);
  assert.equal(byId[2].local, true);
  assert.equal(byId[1000].local, true);
  assert.equal(byId[2].faces[0].file, null);
  // Nunito/index.ts:14-40: weight 500 on every face
  assert.ok(byId[6].faces.every((f) => f.descriptors.weight === "500"));
  // Excalifont/index.ts:122-125
  assert.match(byId[5].faces[0].unicodeRange, /^U\+20-7e,U\+a0-a3,/);
  // Every .woff2 under the non-UI font directories, and no other file.
  const fontsDir = join(upstreamDir(), "packages", "excalidraw", "fonts");
  const onDisk = readdirSync(fontsDir, { withFileTypes: true })
    .filter((d) => d.isDirectory() && d.name !== "Assistant")
    .flatMap((d) =>
      readdirSync(join(fontsDir, d.name))
        .filter((f) => f.endsWith(".woff2"))
        .map((f) => `${d.name}/${f}`),
    )
    .sort();
  const named = g.registered.flatMap((f) => f.faces.map((face) => face.file)).filter(Boolean).sort();
  assert.deepEqual(named, onDisk);
});

test("each face's url is where upstream's package build puts its file", async () => {
  // scripts/buildPackage.js: esbuild with the file loader for .woff2,
  // assetNames "[dir]/[name]" and entry points index.tsx and the *.chunk.ts
  // files, whose common directory (esbuild's outbase) is packages/excalidraw.
  // Bundle every family module with those options into dist/prod and check
  // the uri each font import becomes, where the file is written, and that
  // createUrls' resolution of it against ASSETS_FALLBACK_URL is the url the
  // fixture records (and the harness's fontUri gives the same uri).
  const upstream = upstreamDir();
  const pkg = join(upstream, "packages", "excalidraw");
  const script = readFileSync(join(upstream, "scripts", "buildPackage.js"), "utf8");
  assert.match(script, /assetNames: "\[dir\]\/\[name\]"/);
  assert.match(script, /"\.woff2": "file"/);
  assert.match(script, /entryPoints: \["index\.tsx", "\*\*\/\*\.chunk\.ts"\]/);
  assert.match(script, /getConfig\("dist\/prod"\)/);
  // The family modules are bundled into dist/prod/index.js, so a root
  // entry importing each of them stands in for index.tsx.
  const families = readdirSync(join(pkg, "fonts"), { withFileTypes: true })
    .filter((d) => d.isDirectory() && d.name !== "Assistant")
    .map((d) => d.name);
  const outdir = join(scratch, "package", "dist", "prod");
  const result = await build({
    absWorkingDir: pkg,
    stdin: {
      contents: families.map((f, i) => `export * as f${i} from "./fonts/${f}";`).join("\n"),
      resolveDir: pkg,
      sourcefile: "index.tsx",
      loader: "ts",
    },
    outbase: pkg,
    outdir,
    bundle: true,
    format: "esm",
    packages: "external",
    assetNames: "[dir]/[name]",
    loader: { ".woff2": "file" },
    metafile: true,
    logLevel: "silent",
  });
  const g = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const urls = new Map(
    g.registered.flatMap((f) => f.faces.filter((face) => face.file).map((face) => [face.file, face.url])),
  );
  let checked = 0;
  for (const [out, meta] of Object.entries(result.metafile.outputs)) {
    if (!out.endsWith(".js")) continue;
    const code = readFileSync(join(pkg, out), "utf8");
    for (const input of Object.keys(meta.inputs).filter((i) => i.endsWith(".woff2"))) {
      const file = relative(join(pkg, "fonts"), join(pkg, input));
      const uri = `./fonts/${file}`;
      assert.ok(code.includes(JSON.stringify(uri)), `${out} imports ${input} as ${uri}`);
      assert.equal(fontUri(file), uri);
      assert.ok(readdirSync(join(outdir, "fonts", file, "..")).includes(file.split("/").pop()), uri);
      assert.equal(new URL(uri, g.assetsFallbackUrl).href, urls.get(file), file);
      assert.ok(urls.get(file).startsWith(`${g.assetsFallbackUrl}fonts/`), file);
      checked += 1;
    }
  }
  assert.equal(checked, urls.size);
});

test("containsCJK holds on Han, kana and Hangul and not on Latin", () => {
  const g = JSON.parse(readFileSync(COMMITTED, "utf8"));
  const inCjk = (cp) => g.cjk.some(([a, b]) => cp >= a && cp <= b);
  for (const ch of "你好かなカナ한글") assert.ok(inCjk(ch.codePointAt(0)), ch);
  for (const ch of "Az0 ÿ—") assert.ok(!inCjk(ch.codePointAt(0)), ch);
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, FILE);
  writeFileSync(file, readFileSync(file, "utf8").replace('"weight":"500"', '"weight":"400"'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /font-assets\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
