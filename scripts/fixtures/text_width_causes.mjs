#!/usr/bin/env node
// Text width causes (ex-g303): what upstream writes and keeps for the width
// of every KNOWN_DEVIATIONS text of crates/excali-text/tests/text_width_corpus.rs,
// run in Chrome, with upstream's own code where upstream still has it.
//
//   node scripts/fixtures/text_width_causes.mjs            write the fixture
//   node scripts/fixtures/text_width_causes.mjs --check    exit 1 if this
//                                                          Chrome gives other
//                                                          numbers
//
// Usually run through scripts/fixtures/text-width-causes.sh, which installs
// what it needs. The texts are the `known_deviations` of
// crates/excali-text/tests/fixtures/text-width-corpus-report.json (file,
// element id, stored width, cause), each read from its library under
// fixtures/. For each one, in a Chrome page:
//
// - `restored`: its width after upstream's restoreLibraryItems at the pin
//   (packages/excalidraw/data/restore.ts:1374-1415, bundled from the pinned
//   checkout by tools/goldens/lib/upstream.mjs). That is how a library is
//   loaded; restoreElements runs without `refreshDimensions`, so the stored
//   width is kept (restore.ts:1033-1046). File loading and the initial
//   scene pass keep it too (data/blob.ts:161-164, components/App.tsx:3653-3656),
//   and at the pin a font load re-renders texts but does not re-measure
//   them (fonts/Fonts.ts:106-146).
// - `refreshed`: its width after upstream's restoreElements(item.elements,
//   null, { refreshDimensions: true, repairBindings: true }) at the pin:
//   refreshTextDimensions, upstream's measuring path, with upstream's
//   CanvasTextMetricsProvider (packages/element/src/textMeasurements.ts)
//   on this Chrome's canvas.
// - `ink_box_62228e0b`: getLineWidth of 62228e0b (2024-07-25) to
//   e3060dfb^ (2025-02-11), `packages/excalidraw/element/textElement.ts:345-385`
//   at 62228e0b, quoted below as it was (the pinned checkout has no history):
//   max(|actualBoundingBoxLeft| + |actualBoundingBoxRight|, width) per
//   line, the widest line. Measured with the fonts upstream served then
//   where they measure differently from the vendored ones: Comic Shanns is
//   the single file upstream shipped from 62228e0b to b479f3bd65^
//   (2024-10-17), blob efa4f1c7 of packages/excalidraw/fonts/assets/
//   ComicShanns-Regular.woff2, vendored as
//   crates/excali-text/tests/fixtures/fonts/ComicShanns/ComicShanns-Regular.woff2.
//   The Virgil and Excalifont files of then measure every corpus text as the
//   vendored ones do (ADR-007), so the page uses the vendored ones.
//
// The fonts are the vendored faces (crates/excali-text/assets/fonts/
// manifest.json) of every family a listed text is set in, under their
// upstream family names, each with its unicode-range, loaded before
// anything is measured.
//
// Chrome's canvas measures differently per platform: on macOS the ink box is
// the outlines' (fractional), on Linux advances snap to whole pixels. The
// committed fixture is macOS arm64. --check ignores the recorded browser
// and platform, holds every number to the committed one within TOLERANCE
// (0.001 px, what the Rust test text_width_corpus.rs holds the port to
// against Chrome) and everything else exactly, so another macOS or Chrome
// build may round the last bits differently.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { arch, platform } from "node:os";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

import { format } from "../../tools/goldens/lib/format.mjs";
import { bundleUpstream, REPO_ROOT, verifyUpstream } from "../../tools/goldens/lib/upstream.mjs";

const FIXTURES = join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures");
const REPORT = join(FIXTURES, "text-width-corpus-report.json");
export const OUT = join(FIXTURES, "text-width-causes.json");
const FONTS = join(REPO_ROOT, "crates", "excali-text", "assets", "fonts");
const FIRST_COMIC_SHANNS = join(FIXTURES, "fonts", "ComicShanns", "ComicShanns-Regular.woff2");

/** How far --check lets a number move: 0.001 px. */
export const TOLERANCE = 0.001;

const ENTRY = `
export { restoreLibraryItems, restoreElements } from "./packages/excalidraw/data/restore";
export { getFontString } from "@excalidraw/common";
`;

// getLineWidth, packages/excalidraw/element/textElement.ts:345-385 at
// 62228e0bbb (2024-07-25), unchanged but for the canvas it is handed; the
// test-environment branch after it is unreachable in a browser.
const GET_LINE_WIDTH_62228E0B = `(canvas) => (text, font, forceAdvanceWidth) => {
  const canvas2dContext = canvas.getContext("2d");
  canvas2dContext.font = font;
  const metrics = canvas2dContext.measureText(text);

  const advanceWidth = metrics.width;

  // retrieve the actual bounding box width if these metrics are available (as of now > 95% coverage)
  if (
    !forceAdvanceWidth &&
    window.TextMetrics &&
    "actualBoundingBoxLeft" in window.TextMetrics.prototype &&
    "actualBoundingBoxRight" in window.TextMetrics.prototype
  ) {
    // could be negative, therefore getting the absolute value
    const actualWidth =
      Math.abs(metrics.actualBoundingBoxLeft) +
      Math.abs(metrics.actualBoundingBoxRight);

    // fallback to advance width if the actual width is zero, i.e. on text editing start
    // or when actual width does not respect whitespace chars, i.e. spaces
    // otherwise actual width should always be bigger
    return Math.max(actualWidth, advanceWidth);
  }

  return advanceWidth;
}`;

const usage = () => {
  process.stderr.write("usage: text_width_causes.mjs [--check]\n");
  process.exit(2);
};

/** The known deviations, each with its library item. */
export const texts = () => {
  const report = JSON.parse(readFileSync(REPORT, "utf8"));
  const libraries = new Map();
  return report.known_deviations.map((k) => {
    if (!libraries.has(k.file)) {
      const data = JSON.parse(gunzipSync(readFileSync(join(REPO_ROOT, "fixtures", k.file))).toString("utf8"));
      libraries.set(k.file, data.libraryItems ?? data.library);
    }
    const items = libraries.get(k.file).filter((item) =>
      (Array.isArray(item) ? item : item.elements).some((e) => e.id === k.id),
    );
    if (items.length !== 1) throw new Error(`${k.file} ${k.id}: in ${items.length} library items`);
    return { file: k.file, id: k.id, cause: k.cause, stored: k.stored_width, item: items[0] };
  });
};

/** The fontFamily of `id` in a library item. */
const fontFamily = (item, id) => (Array.isArray(item) ? item : item.elements).find((e) => e.id === id).fontFamily;

/** The font families the texts are set in, ascending. */
export const familiesOf = (list) => [...new Set(list.map((t) => fontFamily(t.item, t.id)))].sort((a, b) => a - b);

/** Font faces of `families` for the page: [family, base64, unicodeRange | undefined]. */
const faces = (families, firstBuild) => {
  const manifest = JSON.parse(readFileSync(join(FONTS, "manifest.json"), "utf8"));
  const out = [];
  for (const family of manifest.families) {
    if (!families.includes(family.id)) continue;
    if (firstBuild && family.id === 8) {
      out.push([family.family, readFileSync(FIRST_COMIC_SHANNS).toString("base64"), undefined]);
      continue;
    }
    for (const face of family.faces) {
      out.push([family.family, readFileSync(join(FONTS, face.file)).toString("base64"), face.unicodeRange]);
    }
  }
  return out;
};

const loadFonts = async (page, list) =>
  page.evaluate(async (list) => {
    for (const [family, data, unicodeRange] of list) {
      const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
      const face = new FontFace(family, bytes, unicodeRange ? { unicodeRange } : {});
      await face.load();
      document.fonts.add(face);
    }
  }, list);

const measure = async () => {
  const upstream = verifyUpstream();
  const bundle = await bundleUpstream(upstream, {
    entry: ENTRY,
    platform: "browser",
    format: "iife",
    globalName: "__upstream",
    define: { "import.meta.env.MODE": '"production"' },
  });
  const require = createRequire(join(REPO_ROOT, "tests", "web", "package.json"));
  const { chromium } = require("@playwright/test");
  const browser = await chromium.launch();
  try {
    const list = texts();
    const input = list.map(({ id, item }) => ({ id, item }));
    const pin = await browser.newPage();
    await pin.setContent("<!doctype html><html><body></body></html>");
    const families = familiesOf(list);
    await loadFonts(pin, faces(families, false));
    await pin.addScriptTag({ content: bundle });
    const atPin = await pin.evaluate((input) => {
      const up = globalThis.__upstream;
      const width = (elements, id) => elements.find((e) => e.id === id).width;
      return input.map(({ id, item }) => {
        const restored = up.restoreLibraryItems([structuredClone(item)], "published")[0].elements;
        const elements = structuredClone(Array.isArray(item) ? item : item.elements);
        const refreshed = up.restoreElements(elements, null, { refreshDimensions: true, repairBindings: true });
        return { restored: width(restored, id), refreshed: width(refreshed, id) };
      });
    }, input);

    const then = await browser.newPage();
    await then.setContent("<!doctype html><html><body></body></html>");
    await loadFonts(then, faces(families, true));
    await then.addScriptTag({ content: bundle });
    const inkBox = await then.evaluate(
      ({ input, source }) => {
        const getLineWidth = eval(source)(document.createElement("canvas"));
        return input.map(({ id, item }) => {
          const e = (Array.isArray(item) ? item : item.elements).find((x) => x.id === id);
          const font = globalThis.__upstream.getFontString(e);
          // measureText of 62228e0b: empty lines measure as a space, the widest line wins.
          return e.text
            .split("\n")
            .map((line) => getLineWidth(line || " ", font))
            .reduce((a, b) => Math.max(a, b), 0);
        });
      },
      { input, source: GET_LINE_WIDTH_62228E0B },
    );

    return {
      generator: "scripts/fixtures/text_width_causes.mjs",
      description:
        "Every known deviation of text_width_corpus.rs in Chrome. restored: its width after upstream's " +
        "restoreLibraryItems at the pin (restore keeps a stored width). refreshed: after upstream's " +
        "restoreElements with refreshDimensions at the pin (refreshTextDimensions measures it on the canvas). " +
        "ink_box_62228e0b: getLineWidth of 62228e0b to e3060dfb^ with the fonts served then " +
        "(Comic Shanns: the single file of 62228e0b to b479f3bd65^).",
      upstream: upstream.commit,
      browser: `Chromium ${browser.version()} (@playwright/test, tests/web/package-lock.json), ${platform()} ${arch()}`,
      texts: list.map((t, i) => ({
        file: t.file,
        id: t.id,
        cause: t.cause,
        stored: t.stored,
        restored: atPin[i].restored,
        refreshed: atPin[i].refreshed,
        ink_box_62228e0b: inkBox[i],
      })),
    };
  } finally {
    await browser.close();
  }
};

/**
 * Where `actual` differs from `expected`, as `path: expected != actual`:
 * numbers by more than TOLERANCE, anything else at all. The recorded
 * browser is not compared.
 */
export const differences = (expected, actual) => {
  const out = [];
  const walk = (a, b, path) => {
    if (typeof a === "number" && typeof b === "number") {
      if (!(Math.abs(a - b) <= TOLERANCE)) out.push(`${path}: ${a} != ${b}`);
      return;
    }
    if (a && b && typeof a === "object" && typeof b === "object" && Array.isArray(a) === Array.isArray(b)) {
      const keys = [...new Set([...Object.keys(a), ...Object.keys(b)])];
      for (const key of keys) {
        if (path === "" && key === "browser") continue;
        if (!(key in a) || !(key in b)) {
          out.push(`${path}${path && "."}${key}: ${JSON.stringify(a[key])} != ${JSON.stringify(b[key])}`);
          continue;
        }
        walk(a[key], b[key], `${path}${path && "."}${key}`);
      }
      return;
    }
    if (a !== b) out.push(`${path}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`);
  };
  walk(expected, actual, "");
  return out;
};

const main = async () => {
  const args = process.argv.slice(2);
  if (args.length > 1 || (args.length === 1 && args[0] !== "--check")) usage();
  const check = args[0] === "--check";
  const fixture = await measure();
  const where = relative(process.cwd(), OUT) || OUT;
  if (check) {
    if (!existsSync(OUT)) {
      process.stderr.write(`missing: ${where}\n`);
      process.exit(1);
    }
    const committed = JSON.parse(readFileSync(OUT, "utf8"));
    const found = differences(committed, fixture);
    if (found.length > 0) {
      process.stderr.write(
        `${where}: ${fixture.browser} measures differently than ${committed.browser}:\n` +
          found.map((d) => `  ${d}\n`).join(""),
      );
      process.exit(1);
    }
    process.stdout.write(`text width causes reproduced by ${fixture.browser}: ${where}\n`);
    return;
  }
  writeFileSync(OUT, format(fixture));
  process.stdout.write(`wrote ${where} with ${fixture.browser}\n`);
};

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
