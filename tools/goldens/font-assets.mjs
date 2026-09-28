#!/usr/bin/env node
// Font asset goldens for excali-text (ex-307): upstream's own font registry
// and the functions that decide which of its range-split files a scene
// needs, run from the pinned checkout under plain Node.
//
//   node tools/goldens/font-assets.mjs            write the fixture
//   node tools/goldens/font-assets.mjs --check    exit 1 if it is stale
//   node tools/goldens/font-assets.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-text/tests/fixtures/font-assets.json:
//
// - registered: `Fonts.registered` (packages/excalidraw/fonts/Fonts.ts:63-77,
//   375-416), in its Map order: every family's id, FontFace family name,
//   metadata flags and font faces. Each face has the file its `uri` imports
//   (the path under packages/excalidraw/fonts, null for a local face), the
//   CSS format `getFormat` derives, the descriptors upstream passes to
//   `new FontFace` (ExcalidrawFontFace.ts:17-30) with its `unicodeRange`
//   (null when upstream gives none: the face covers U+0-10FFFF) apart, and
//   `probes`: `getUnicodeRangeRegex()` (ExcalidrawFontFace.ts:114-131)
//   tested on the code points either side of both ends of every range of
//   the FontFace's range, one "0"/"1" each.
// - cjk: `containsCJK` (packages/element/src/textWrapping.ts:30-36) on every
//   code point, as inclusive ranges of the code points where it is true.
// - scenes: named element lists, each with `getUniqueFamilies`,
//   `getCharsPerFamily` / `getCharacters` and `getFontString(fontFamily,
//   FONT_SIZES.sm)` (what `loadSceneFonts` / `loadElementsFonts` hand to
//   `document.fonts.load`, Fonts.ts:153-177, 249-284), both over all the
//   elements (loadElementsFonts) and over the non-deleted ones (the scene's
//   getNonDeletedElements, loadSceneFonts), and the font faces
//   `generateFontFaceDeclarations` (Fonts.ts:182-217) inlines, in order.
//
// No browser is involved. `FontFace` is replaced by a class that records its
// constructor arguments, with the CSS Font Loading defaults for descriptors
// upstream leaves out (unicodeRange "U+0-10FFFF"); `window` is an empty
// object (no EXCALIDRAW_ASSET_PATH); ExcalidrawFontFace#getContent, which
// fetches and subsets the file, returns the face's file and the code points
// it was asked for instead, so the declarations name the files upstream
// would fetch and the characters it would subset them to.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures");
export const FILE = "font-assets.json";

const ENTRY = `
export { Fonts } from "./packages/excalidraw/fonts/Fonts";
export { ExcalidrawFontFace } from "./packages/excalidraw/fonts/ExcalidrawFontFace";
export { containsCJK } from "./packages/element/src/textWrapping";
export { FONT_SIZES, getFontString } from "@excalidraw/common";
`;

/** A text element with the fields the font code reads. */
const text = (fontFamily, originalText, extra = {}) => ({
  type: "text",
  fontFamily,
  originalText,
  isDeleted: false,
  ...extra,
});

/**
 * Scenes: every registered family, the local ones, ids with no registration,
 * every Excalifont range, CJK in and outside Excalifont, the Google Fonts
 * ranges of Nunito and Lilita One, emoji, empty and deleted text.
 */
export const SCENES = [
  { name: "empty", elements: [] },
  { name: "no-text", elements: [{ type: "rectangle", isDeleted: false }] },
  { name: "excalifont-latin", elements: [text(5, "Hello world")] },
  { name: "excalifont-cjk", elements: [text(5, "Hello 你好 こんにちは 안녕하세요")] },
  { name: "excalifont-cjk-only", elements: [text(5, "漢字かなカナ한글")] },
  { name: "excalifont-cyrillic-greek", elements: [text(5, "Привет Ωμέγα №")] },
  { name: "excalifont-latin-ext", elements: [text(5, "Łódź Ŵales ǽ ẞ")] },
  { name: "excalifont-marks", elements: [text(5, "à́ ã ẽ ŏ ș ﬁ ∑ ℮")] },
  { name: "excalifont-cyrillic-ext", elements: [text(5, "Ѣ ѳ Ә ӣ Ӧ Ӯ")] },
  { name: "excalifont-punctuation", elements: [text(5, "“quoted” — €100 ™ … ‰ −")] },
  { name: "excalifont-fullwidth", elements: [text(5, "（ＡＢＣ）「引用」。")] },
  { name: "excalifont-emoji", elements: [text(5, "ok 😀 👍🏽")] },
  { name: "excalifont-hangul-jamo", elements: [text(5, "ᄀᄁᄂ")] },
  { name: "excalifont-empty-text", elements: [text(5, "")] },
  { name: "virgil", elements: [text(1, "Virgil draws 你好")] },
  { name: "helvetica", elements: [text(2, "Helvetica is local")] },
  { name: "cascadia", elements: [text(3, "fn main() { println!(\"ü\"); }")] },
  { name: "nunito-vietnamese-cyrillic", elements: [text(6, "Xin chào Việt Nam, Привет, Ѣ")] },
  { name: "nunito-latin", elements: [text(6, "Nunito")] },
  { name: "nunito-cjk", elements: [text(6, "中文")] },
  { name: "lilita", elements: [text(7, "Ärger Ōsaka ŧ")] },
  { name: "comic-shanns", elements: [text(8, "Comic Ĳ ŀ ↑ ꝛ")] },
  { name: "liberation", elements: [text(9, "Liberation · ‒")] },
  { name: "assistant", elements: [text(10, "Assistant")] },
  { name: "unknown-family", elements: [text(42, "unknown 你好")] },
  { name: "xiaolai-element", elements: [text(100, "直接 Xiaolai")] },
  { name: "emoji-element", elements: [text(1000, "😀")] },
  {
    name: "multi-family",
    elements: [
      text(8, "code"),
      text(5, "hand 手"),
      { type: "rectangle", isDeleted: false },
      text(6, "Nunito ǅ"),
      text(5, "more ÿ"),
      text(2, "local"),
    ],
  },
  {
    name: "deleted",
    elements: [text(5, "kept"), text(5, "deleted 删除", { isDeleted: true }), text(6, "gone", { isDeleted: true })],
  },
  { name: "duplicates", elements: [text(5, "aaa"), text(5, "aba"), text(5, "")] },
];

const usage = () => {
  process.stderr.write("usage: font-assets.mjs [--check] [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: FIXTURES_DIR };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

/** Records what upstream passes to `new FontFace`. */
class RecordingFontFace {
  constructor(family, source, descriptors = {}) {
    this.family = family;
    this.source = source;
    this.style = descriptors.style ?? "normal";
    this.weight = descriptors.weight ?? "normal";
    this.display = descriptors.display ?? "auto";
    this.unicodeRange = descriptors.unicodeRange ?? "U+0-10FFFF";
    this.descriptors = descriptors;
  }
}

const MAX_CODE_POINT = 0x10ffff;

/** The code points either side of both ends of each range in `unicodeRange`, split as getUnicodeRangeRegex splits it. */
export const probeCodePoints = (unicodeRange) =>
  unicodeRange.split(/,\s*/).flatMap((range) => {
    const [start, end = start] = range.replace("U+", "").split("-");
    const a = parseInt(start, 16);
    const b = parseInt(end, 16);
    return [a - 1, a, b, b + 1].filter((cp) => cp >= 0 && cp <= MAX_CODE_POINT);
  });

const fileOf = (up, url) => {
  const base = up.ExcalidrawFontFace.ASSETS_FALLBACK_URL;
  const href = url.toString();
  if (!href.startsWith(base)) throw new Error(`font url ${href} is not under ${base}`);
  return decodeURIComponent(href.slice(base.length));
};

const registered = (up) =>
  [...up.Fonts.registered.entries()].map(([id, { metadata, fontFaces }]) => ({
    id,
    family: fontFaces[0]?.fontFace.family ?? null,
    local: metadata.local ?? false,
    fallback: metadata.fallback ?? false,
    private: metadata.private ?? false,
    deprecated: metadata.deprecated ?? false,
    faces: fontFaces.map((face) => {
      const regex = face.getUnicodeRangeRegex();
      const probes = probeCodePoints(face.fontFace.unicodeRange);
      const { unicodeRange = null, ...descriptors } = face.fontFace.descriptors;
      return {
        file: face.urls.length ? fileOf(up, face.urls[face.urls.length - 1]) : null,
        format: face.urls.length ? up.ExcalidrawFontFace.getFormat(face.urls[0]) : null,
        descriptors,
        unicodeRange,
        probes: probes.map((cp) => (regex.test(String.fromCodePoint(cp)) ? "1" : "0")).join(""),
      };
    }),
  }));

const cjkRanges = (up) => {
  const ranges = [];
  let start = -1;
  for (let cp = 0; cp <= MAX_CODE_POINT + 1; cp++) {
    const hit = cp <= MAX_CODE_POINT && up.containsCJK(String.fromCodePoint(cp));
    if (hit && start < 0) start = cp;
    if (!hit && start >= 0) {
      ranges.push([start, cp - 1]);
      start = -1;
    }
  }
  return ranges;
};

const DECLARATION = /^@font-face \{ font-family: (.*); src: url\((.*)\); \}$/;

const loads = (up, elements) => {
  const families = up.Fonts.getUniqueFamilies(elements);
  const charsPerFamily = up.Fonts.getCharsPerFamily(elements);
  return families.map((fontFamily) => ({
    fontFamily,
    characters: up.Fonts.getCharacters(charsPerFamily, fontFamily),
    font: up.getFontString({ fontFamily, fontSize: up.FONT_SIZES.sm }),
  }));
};

const scene = async (up, { name, elements }) => {
  const css = await up.Fonts.generateFontFaceDeclarations(elements);
  return {
    name,
    elements,
    elementsFonts: loads(up, elements),
    sceneFonts: loads(
      up,
      elements.filter((element) => !element.isDeleted),
    ),
    declarations: css.map((rule) => {
      const [, family, content] = rule.match(DECLARATION) ?? [];
      if (content === undefined) throw new Error(`unexpected font-face rule ${rule}`);
      const { file, codePoints } = JSON.parse(content);
      return { family, file, codePoints };
    }),
  };
};

const build = async (upstream) => {
  globalThis.FontFace = RecordingFontFace;
  globalThis.window = {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: ["packages/excalidraw/subset/subset-main"],
    define: { "import.meta.env.PKG_NAME": "undefined", "import.meta.env.PKG_VERSION": "undefined" },
    fontUris: true,
  });
  // getContent fetches and subsets; answer with what it was asked for.
  up.ExcalidrawFontFace.prototype.getContent = async function getContent(codePoints) {
    const url = this.urls[this.urls.length - 1];
    return JSON.stringify({ file: fileOf(up, url), codePoints });
  };
  const scenes = [];
  for (const s of SCENES) scenes.push(await scene(up, s));
  return format({
    description:
      "Upstream's font registry (Fonts.registered, packages/excalidraw/fonts/Fonts.ts:63-77, 375-416; ExcalidrawFontFace.ts:17-30, 114-131, 172-189), containsCJK (packages/element/src/textWrapping.ts:30-36) on every code point, and per scene getUniqueFamilies, getCharsPerFamily, getCharacters, getFontString (Fonts.ts:153-177, 249-284, 418-470) and generateFontFaceDeclarations (Fonts.ts:182-217) at the pinned commit (tools/goldens/font-assets.mjs).",
    upstream: upstream.commit,
    assetsFallbackUrl: up.ExcalidrawFontFace.ASSETS_FALLBACK_URL,
    fontSize: up.FONT_SIZES.sm,
    registered: registered(up),
    cjk: cjkRanges(up),
    scenes,
  });
};

/** Runs fn with Math.random disabled and console.error captured. */
const deterministic = async (fn) => {
  const random = Math.random;
  const error = console.error;
  Math.random = () => {
    throw new Error("Math.random called while generating font-assets goldens");
  };
  // generateFontFaceDeclarations reports unregistered families (42) on
  // console.error and continues; keep the output clean.
  console.error = () => {};
  try {
    return await fn();
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
    process.stderr.write(`font-assets: ${error.message}\n`);
    process.exit(1);
  }
  const out = await deterministic(() => build(upstream));
  const path = join(args.out, FILE);
  const where = relative(process.cwd(), path) || FILE;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== out) {
      process.stderr.write(
        `stale: ${where}\nfont-assets goldens are out of date: run node tools/goldens/font-assets.mjs\n`,
      );
      process.exit(1);
    }
    process.stdout.write(`font-assets goldens up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, out);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (import.meta.url === pathToFileURL(process.argv[1]).href) await main();
