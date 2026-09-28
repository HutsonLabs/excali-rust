#!/usr/bin/env node
// Font metadata goldens for excali-text (ex-301): upstream's own
// FONT_METADATA, getVerticalOffset and getLineHeight
// (packages/common/src/font-metadata.ts:35-181), getLineHeightInPx
// (packages/element/src/textMeasurements.ts:91-96), the family constants and
// fallbacks (packages/common/src/constants.ts:127-197) and getFontFamilyString
// / getFontString (packages/common/src/utils.ts:123-147), run from the pinned
// checkout under plain Node.
//
//   node tools/goldens/font-metadata.mjs            write the fixture
//   node tools/goldens/font-metadata.mjs --check    exit 1 if it is stale
//   node tools/goldens/font-metadata.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-text/tests/fixtures/font-metadata.json:
//
// - fontFamily, fallbacks, genericFallbacks: FONT_FAMILY,
//   FONT_FAMILY_FALLBACKS and FONT_FAMILY_GENERIC_FALLBACKS (key order kept);
// - metadata: Object.entries(FONT_METADATA), ids as numbers, in JS key order;
// - googleFontsRanges, localFontProtocol: GOOGLE_FONTS_RANGES and
//   LOCAL_FONT_PROTOCOL;
// - families: for every id in FAMILY_IDS (named, fallback, unused and
//   custom ids), getLineHeight, getGenericFontFamilyFallback,
//   getFontFamilyFallbacks and getFontFamilyString;
// - lineHeightInPx: getLineHeightInPx for SIZES x LINE_HEIGHTS;
// - verticalOffset: getVerticalOffset(fontFamily, fontSize, lineHeightPx) for
//   every id and size, with the line heights upstream renders with
//   (getLineHeightInPx of the family's own and of LINE_HEIGHTS) and a few
//   pixel line heights that are no multiple of the size;
// - fontString: getFontString for every id and a few sizes.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures");
export const FILE = "font-metadata.json";

const ENTRY = `
export {
  FONT_METADATA,
  GOOGLE_FONTS_RANGES,
  LOCAL_FONT_PROTOCOL,
  getVerticalOffset,
  getLineHeight,
  FONT_FAMILY,
  FONT_FAMILY_FALLBACKS,
  FONT_FAMILY_GENERIC_FALLBACKS,
  CJK_HAND_DRAWN_FALLBACK_FONT,
  WINDOWS_EMOJI_FALLBACK_FONT,
  SANS_SERIF_GENERIC_FONT,
  MONOSPACE_GENERIC_FONT,
  DEFAULT_FONT_FAMILY,
  getGenericFontFamilyFallback,
  getFontFamilyFallbacks,
  getFontFamilyString,
  getFontString,
} from "@excalidraw/common";
export { getLineHeightInPx } from "./packages/element/src/textMeasurements";
`;

/** Every named and fallback id, the unused 4, and ids with no metadata. */
export const FAMILY_IDS = [
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 42, 100, 998, 999, 1000, 1001, 65535,
];

/** Font sizes: the picker's (S/M/L/XL: 16, 20, 28, 36), odd and extreme ones. */
const SIZES = [0, 0.1, 0.5, 1, 7, 8, 12, 13.37, 16, 20, 24, 28, 36, 48, 72, 100, 123.456, 1000];

/** Unitless line heights besides each family's own. */
const LINE_HEIGHTS = [0, 1, 1.15, 1.2, 1.25, 1.5, 2, 0.8333333333333334];

/** Pixel line heights that are no multiple of the size. */
const LINE_HEIGHTS_PX = [0, 17, 33.3];

const usage = () => {
  process.stderr.write("usage: font-metadata.mjs [--check] [--out DIR]\n");
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

const metadata = (up) =>
  Object.entries(up.FONT_METADATA).map(([id, meta]) => ({ id: Number(id), ...meta }));

const families = (up) =>
  FAMILY_IDS.map((id) => ({
    id,
    lineHeight: up.getLineHeight(id),
    genericFallback: up.getGenericFontFamilyFallback(id),
    fallbacks: up.getFontFamilyFallbacks(id),
    fontFamilyString: up.getFontFamilyString({ fontFamily: id }),
  }));

const lineHeightInPx = (up) =>
  SIZES.flatMap((fontSize) =>
    LINE_HEIGHTS.map((lineHeight) => ({
      fontSize,
      lineHeight,
      px: up.getLineHeightInPx(fontSize, lineHeight),
    })),
  );

const verticalOffsets = (up) =>
  FAMILY_IDS.flatMap((fontFamily) => {
    const heights = [...new Set([up.getLineHeight(fontFamily), ...LINE_HEIGHTS])];
    return SIZES.map((fontSize) => {
      const lineHeightPx = [
        ...heights.map((h) => up.getLineHeightInPx(fontSize, h)),
        ...LINE_HEIGHTS_PX,
      ];
      return {
        fontFamily,
        fontSize,
        lineHeightPx,
        offset: lineHeightPx.map((px) => up.getVerticalOffset(fontFamily, fontSize, px)),
      };
    });
  });

const fontStrings = (up) =>
  FAMILY_IDS.flatMap((fontFamily) =>
    [20, 13.37, 0.1, 1000].map((fontSize) => ({
      fontSize,
      fontFamily,
      font: up.getFontString({ fontSize, fontFamily }),
    })),
  );

const build = async (upstream) => {
  const up = await loadUpstream(upstream, { entry: ENTRY });
  return format({
    description:
      "Upstream FONT_METADATA, getVerticalOffset, getLineHeight (common/src/font-metadata.ts:35-181), getLineHeightInPx (element/src/textMeasurements.ts:91-96), the font family constants and fallbacks (common/src/constants.ts:127-197), getFontFamilyString and getFontString (common/src/utils.ts:123-147) at the pinned commit (tools/goldens/font-metadata.mjs).",
    upstream: upstream.commit,
    fontFamily: up.FONT_FAMILY,
    fallbacks: up.FONT_FAMILY_FALLBACKS,
    genericFallbacks: up.FONT_FAMILY_GENERIC_FALLBACKS,
    defaultFontFamily: up.DEFAULT_FONT_FAMILY,
    names: {
      cjk: up.CJK_HAND_DRAWN_FALLBACK_FONT,
      emoji: up.WINDOWS_EMOJI_FALLBACK_FONT,
      sansSerif: up.SANS_SERIF_GENERIC_FONT,
      monospace: up.MONOSPACE_GENERIC_FONT,
    },
    metadata: metadata(up),
    googleFontsRanges: up.GOOGLE_FONTS_RANGES,
    localFontProtocol: up.LOCAL_FONT_PROTOCOL,
    families: families(up),
    lineHeightInPx: lineHeightInPx(up),
    verticalOffset: verticalOffsets(up),
    fontString: fontStrings(up),
  });
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating font-metadata goldens");
  };
  try {
    return await fn();
  } finally {
    Math.random = random;
  }
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`font-metadata: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out, FILE);
  const where = relative(process.cwd(), path) || FILE;
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(
        `stale: ${where}\nfont-metadata goldens are out of date: run node tools/goldens/font-metadata.mjs\n`,
      );
      process.exit(1);
    }
    process.stdout.write(`font-metadata goldens up to date: ${where}\n`);
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
