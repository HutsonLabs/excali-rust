#!/usr/bin/env node
// Upstream's SVG font subsetting, for the ex-408 spike (ADR-010): what
// `Fonts.generateFontFaceDeclarations` (packages/excalidraw/fonts/Fonts.ts:182-217)
// inlines for a scene when every font file can be read, run from the pinned
// checkout under plain Node.
//
//   node tools/goldens/font-subset.mjs            write the fixture
//   node tools/goldens/font-subset.mjs --check    exit 1 if it is stale
//   node tools/goldens/font-subset.mjs --out DIR  write (or --check) DIR
//
// Writes tools/font-subset-eval/upstream-subsets.json: per scene, the text
// elements and every `@font-face` rule upstream writes, split into the
// family, the face's file (under packages/excalidraw/fonts), the code points
// it was asked to keep and the woff2 it inlined (base64, the data URL's
// payload). tools/font-subset-eval measures the port's candidates against it.
//
// Nothing is replaced but the fetch. As upstream's own test setup does
// (setupTests.ts:101-125, "mock the font fetch only, so that everything
// else, as font subsetting, can run"), ExcalidrawFontFace#fetchFont reads
// the face's file from the checkout; getContent, subset-main and
// subset-shared.chunk (woff2 decompress, HarfBuzz subset from harfbuzzjs
// 0.3.6, woff2 compress, all upstream's inlined wasm) run unchanged. Node
// has no Worker, so subset-main subsets on the main thread, as it does under
// vitest. The export test scenes reproduce upstream's snapshot
// (tests/scene/__snapshots__/export.test.ts.snap), which
// test/font-subset.test.mjs checks rule for rule.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import pako from "pako";

import { SCENES as ASSET_SCENES } from "./font-assets.mjs";
import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "tools", "font-subset-eval");
export const FILE = "upstream-subsets.json";

const ENTRY = `
export { Fonts } from "./packages/excalidraw/fonts/Fonts";
export { ExcalidrawFontFace } from "./packages/excalidraw/fonts/ExcalidrawFontFace";
export { FONT_FAMILY } from "@excalidraw/common";
`;

const text = (fontFamily, originalText, extra = {}) => ({
  type: "text",
  fontFamily,
  originalText,
  isDeleted: false,
  ...extra,
});

/** The characters U+first..U+last. */
const range = (first, last) =>
  Array.from({ length: last - first + 1 }, (_, i) => String.fromCodePoint(first + i)).join("");

const ASCII = range(0x20, 0x7e);

// tests/scene/export.test.ts:32-62, 78-98 (textFixture's text and family:
// tests/fixtures/elementFixture.ts:58-75, DEFAULT_FONT_FAMILY Excalifont).
const FIXTURE_TEXT = "original text";
export const CJK_TEXT =
  "中国你好！这是一个测试。中国你好！日本こんにちは！これはテストです。한국 안녕하세요! 이것은 테스트입니다.";

const LABELS = [
  "User",
  "Web app",
  "API Gateway",
  "Auth service",
  "Orders DB",
  "Cache (Redis)",
  "Queue → Worker",
  "Retry after 30s?",
  "TODO: add metrics & alerts",
  "v2.1 — shipped 2026-09-28",
];

const CJK_PARAGRAPH =
  "我们用手绘风格的白板画流程图、架构图和草图。每个元素都有自己的颜色、线条和填充样式，" +
  "导出时字体会被嵌入到文件里，这样别人打开时看到的文字和你画的时候完全一样。" +
  "日本語の説明：図形を選んで、矢印でつなぎ、ラベルを書きます。" +
  "한국어 설명: 도형을 그리고 화살표로 연결한 다음 글자를 씁니다.";

/** Upstream's export test, run with its test setup's FontFace (checked against its snapshot). */
export const VITEST_SCENES = [
  { name: "export-test-default", elements: [text(5, FIXTURE_TEXT), text(6, FIXTURE_TEXT)] },
  {
    name: "export-test-cjk",
    elements: [text(5, FIXTURE_TEXT), text(6, FIXTURE_TEXT), text(5, CJK_TEXT)],
  },
];

/**
 * Scenes in a browser: upstream's export test elements, the font
 * asset scenes (every family, range and fallback case), and scenes the size
 * of real drawings: diagram labels, printable ASCII in every family upstream inlines,
 * Latin-1 and a paragraph of Chinese, Japanese and Korean.
 */
export const SCENES = [
  { name: "fixture-default", elements: [text(5, FIXTURE_TEXT), text(6, FIXTURE_TEXT)] },
  { name: "fixture-cjk", elements: [text(5, FIXTURE_TEXT), text(6, FIXTURE_TEXT), text(5, CJK_TEXT)] },
  ...ASSET_SCENES.map((s) => ({ name: `assets-${s.name}`, elements: s.elements })),
  { name: "labels-excalifont", elements: LABELS.map((l) => text(5, l)) },
  { name: "labels-virgil", elements: LABELS.map((l) => text(1, l)) },
  { name: "labels-nunito", elements: LABELS.map((l) => text(6, l)) },
  ...[
    [5, "excalifont"],
    [1, "virgil"],
    [3, "cascadia"],
    [6, "nunito"],
    [7, "lilita"],
    [8, "comic-shanns"],
    [9, "liberation"],
  ].map(([family, name]) => ({ name: `ascii-${name}`, elements: [text(family, ASCII)] })),
  { name: "latin1-excalifont", elements: [text(5, range(0xa1, 0xff))] },
  { name: "cjk-paragraph", elements: [text(5, CJK_PARAGRAPH)] },
];

const usage = () => {
  process.stderr.write("usage: font-subset.mjs [--check] [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: OUT_DIR };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

/** Records what upstream passes to `new FontFace` (font-assets.mjs). */
class RecordingFontFace {
  constructor(family, source, descriptors = {}) {
    this.family = family;
    this.source = source;
    this.unicodeRange = descriptors.unicodeRange ?? "U+0-10FFFF";
    this.descriptors = descriptors;
  }
}

const RULE = /^@font-face \{ font-family: (.*); src: url\(data:font\/woff2;base64,([A-Za-z0-9+/=]*)\); \}$/;

/**
 * The FontFace of upstream's test setup (setupTests.ts:65-86): every face's
 * unicodeRange is "U+0000-00FF", so under vitest a face is inlined whenever
 * the family's text has a Latin-1 character, whatever its real range. The
 * export test snapshot was written under it.
 */
class VitestFontFace {
  constructor(family, source, descriptors) {
    this.family = family;
    this.source = source;
    this.descriptors = descriptors;
    this.status = "unloaded";
    this.unicodeRange = "U+0000-00FF";
  }
}

const FONT_FACES = { browser: RecordingFontFace, vitest: VitestFontFace };

/** Upstream's generateFontFaceDeclarations on each scene, faces made by fontFace's class. */
const subsetScenes = async (upstream, fontFace, scenes) => {
  // Fonts.init registers every face once, at load: one bundle per class.
  globalThis.FontFace = FONT_FACES[fontFace];
  globalThis.window = {};
  delete globalThis.document;
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.PKG_NAME": "undefined", "import.meta.env.PKG_VERSION": "undefined" },
    fontUris: true,
  });
  // The emscripten woff2 bindings take `window` for a browser and then read
  // document.currentScript (woff2-bindings.ts:160-164), which jsdom answers
  // with null under vitest. Defined after loading: modules that create
  // elements at load time see no document, as in font-assets.mjs.
  globalThis.document = { currentScript: null };
  const base = `${up.ExcalidrawFontFace.ASSETS_FALLBACK_URL}fonts/`;
  const fontsDir = join(upstream.dir, "packages", "excalidraw", "fonts");
  const fileOf = (url) => {
    const href = url.toString();
    if (!href.startsWith(base)) throw new Error(`font url ${href} is not under ${base}`);
    return decodeURIComponent(href.slice(base.length));
  };
  // setupTests.ts:113-121: read the file instead of fetching it. Record the
  // file and code points each rule was made from.
  const calls = [];
  up.ExcalidrawFontFace.prototype.fetchFont = async function fetchFont(url) {
    const bytes = readFileSync(join(fontsDir, fileOf(url)));
    return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  };
  const getContent = up.ExcalidrawFontFace.prototype.getContent;
  up.ExcalidrawFontFace.prototype.getContent = async function recorded(codePoints) {
    const call = { file: fileOf(this.urls[this.urls.length - 1]), codePoints };
    calls.push(call);
    call.content = await getContent.call(this, codePoints);
    return call.content;
  };
  const out = [];
  for (const { name, elements } of scenes) {
    calls.length = 0;
    const rules = await up.Fonts.generateFontFaceDeclarations(elements);
    const declarations = rules.map((rule) => {
      const [, family, woff2] = rule.match(RULE) ?? [];
      if (woff2 === undefined) throw new Error(`${name}: not a subset woff2 data URL: ${rule.slice(0, 120)}`);
      const call = calls.find((c) => c.content === `data:font/woff2;base64,${woff2}`);
      if (!call) throw new Error(`${name}: no getContent call made ${rule.slice(0, 80)}`);
      return { family, file: call.file, codePoints: call.codePoints, size: Buffer.from(woff2, "base64").length, woff2 };
    });
    out.push({
      name,
      fontFace,
      texts: elements
        .filter((e) => e.type === "text" && !e.isDeleted)
        .map((e) => ({ fontFamily: e.fontFamily, text: e.originalText })),
      declarations,
    });
  }
  return out;
};

/**
 * The wasm modules upstream's subsetting loads (subset-shared.chunk.ts:47-48),
 * inlined as base64 in subset/harfbuzz/harfbuzz-wasm.ts and
 * subset/woff2/woff2-wasm.ts: their bytes, raw and gzipped at level 9.
 * Gzipped with pako (zlib's deflate in plain JavaScript), not node:zlib:
 * Node bundles Chromium's zlib, whose level-9 output differs between x64 and
 * arm64, so the sizes would depend on the machine that ran this.
 */
export const wasmModules = (dir) =>
  Object.fromEntries(
    [
      ["harfbuzz", "packages/excalidraw/subset/harfbuzz/harfbuzz-wasm.ts"],
      ["woff2", "packages/excalidraw/subset/woff2/woff2-wasm.ts"],
    ].map(([name, file]) => {
      const source = readFileSync(join(dir, file), "utf8");
      const base64 = source.match(/`([A-Za-z0-9+/=]+)`/);
      if (!base64) throw new Error(`no base64 module in ${file}`);
      const bytes = Buffer.from(base64[1], "base64");
      if (bytes.subarray(0, 4).toString("latin1") !== "\0asm") throw new Error(`${file} is not a wasm module`);
      return [name, { file, bytes: bytes.length, gzip: pako.gzip(bytes, { level: 9 }).length }];
    }),
  );

const build = async (upstream) => {
  const scenes = [
    ...(await subsetScenes(upstream, "vitest", VITEST_SCENES)),
    ...(await subsetScenes(upstream, "browser", SCENES)),
  ];
  const text = format({
    description:
      "Upstream's SVG font subsetting at the pinned commit (tools/goldens/font-subset.mjs): per scene, the text elements and the @font-face rules Fonts.generateFontFaceDeclarations (packages/excalidraw/fonts/Fonts.ts:182-217) writes, with ExcalidrawFontFace#fetchFont reading the face's file from the checkout as setupTests.ts:101-125 does and everything else (getContent, subset-main, subset-shared.chunk: woff2 decompress, harfbuzzjs 0.3.6 hb-subset with every layout feature, woff2 compress) unchanged. fontFace is browser (each face has its real unicode range) or vitest (setupTests.ts:65-86 gives every face U+0000-00FF, as when upstream's export test snapshot was written). file is the face's path under packages/excalidraw/fonts, codePoints what getContent was asked to keep, woff2 the data URL's base64 payload and size its length in bytes. wasm: the two wasm modules the subsetting loads, bytes raw and gzipped at level 9.",
    upstream: upstream.commit,
    wasm: wasmModules(upstream.dir),
    scenes,
  });
  // Every non-ASCII code unit as a \u escape (as url-host-fixtures.mjs
  // writes): the same JSON value, and the file stays free of the invisible
  // code points the attribution gate rejects (U+00AD is in latin1-excalifont).
  return text.replace(/[\u0080-\uffff]/g, (ch) => `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`);
};

/** Runs fn with Math.random disabled and console.error captured. */
const deterministic = async (fn) => {
  const random = Math.random;
  const error = console.error;
  const errors = [];
  Math.random = () => {
    throw new Error("Math.random called while generating font-subset goldens");
  };
  // generateFontFaceDeclarations reports unregistered families (42, and
  // Assistant, 10, which Fonts.init does not register) on console.error and
  // continues; anything else is a failure (a subset that fell back to the
  // whole file logs "Skipped glyph subsetting").
  console.error = (...args) => errors.push(args.map(String).join(" "));
  try {
    const out = await fn();
    const unexpected = errors.filter((e) => !e.startsWith("Couldn't find registered fonts for font-family"));
    if (unexpected.length) throw new Error(`upstream logged errors:\n${unexpected.join("\n")}`);
    return out;
  } finally {
    Math.random = random;
    console.error = error;
  }
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`font-subset: ${error.message}\n`);
    process.exit(1);
  }
  const out = await deterministic(() => build(upstream));
  const path = join(args.out, FILE);
  const where = relative(process.cwd(), path) || FILE;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== out) {
      process.stderr.write(`stale: ${where}\nfont-subset goldens are out of date: run node tools/goldens/font-subset.mjs\n`);
      process.exit(1);
    }
    process.stdout.write(`font-subset goldens up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, out);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (import.meta.url === pathToFileURL(process.argv[1]).href) await main();
