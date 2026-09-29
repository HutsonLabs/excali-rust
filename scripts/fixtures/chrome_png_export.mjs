#!/usr/bin/env node
// Chrome PNG export references (ex-g402): upstream's own exportToCanvas at
// the pinned commit, run in Playwright's Chromium with the vendored fonts,
// on every fixture scene; the PNG the canvas encodes is kept for the port's
// full PNG export to be compared with (crates/excali-cli/tests/chrome_export.rs).
//
//   node scripts/fixtures/chrome_png_export.mjs              write the references
//   node scripts/fixtures/chrome_png_export.mjs --out DIR    write them to DIR
//
// Usually run through scripts/fixtures/chrome-png-export.sh, which checks
// what it needs and has the --check mode. The scenes:
//
// - every scene of crates/excali-scene/tests/fixtures/canvas-export.json
//   (tools/goldens/png-export.mjs), exported from its recorded inputs as
//   the golden generator exported it: an editor scene with
//   exportToCanvas(elements, appState, files, opts) and its default
//   createCanvas and font loading (packages/excalidraw/scene/export.ts:
//   180-285), a utils scene with the utils wrapper (packages/utils/src/
//   export.ts:42-105) given the caller's app state, maxWidthOrHeight,
//   getDimensions and exportPadding;
// - each element of upstream's tests/fixtures/elementFixture.ts, and its
//   textFixture in Nunito as tests/scene/export.test.ts exports it, written
//   as a scene file (serializeAsJSON(elements, getDefaultAppState(), {},
//   "local")) under scenes/;
// - upstream's scene-bearing PNG fixtures, tests/fixtures/test_embedded_v1.png
//   and smiley_embedded_v2.png (fixtures/upstream/).
//
// A scene file is loaded as the editor loads a dropped file (loadFromBlob,
// packages/excalidraw/data/blob.ts:138-196) and exported as "Export image"
// exports it (exportCanvas("png"), data/index.ts:98-192: the non-deleted
// elements, the loaded app state, exportBackground and viewBackgroundColor
// from it, no frame): what `excali render` does with the same file.
//
// The PNG is canvas.toBlob("image/png") of the canvas exportToCanvas
// returns, the image data exportCanvas and exportToBlob encode. No scene is
// embedded: the pixels are compared, the payload is ex-405's
// (png-export.mjs --reimport).
//
// Fonts: the page is served from a fixed origin and window.EXCALIDRAW_ASSET_PATH
// points upstream's own ExcalidrawFontFace urls at the vendored files
// (crates/excali-text/assets/fonts, the same paths as upstream's
// packages/excalidraw/fonts), so upstream's Fonts.loadElementsFonts loads
// the faces a scene needs by unicode range exactly as it does from its CDN.
// Every other request is refused and fails the run. Helvetica and Segoe UI
// Emoji are `local:` upstream (ADR-004): Chromium draws them with the
// machine's fonts (on macOS, Helvetica and Apple Color Emoji).
//
// Chromium draws the canvas in software in sRGB (--disable-gpu
// --force-color-profile=srgb, as the raster references), at device pixel
// ratio 1 (so the default exportScale is 1). Writes <name>.png per scene,
// scenes/<name>.excalidraw for the element fixtures, and manifest.json:
// the upstream commit, the browser and platform, and per scene where it
// comes from, the SHA-256 of that source, the canvas size and the PNG's
// SHA-256.

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { arch, platform } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { bundleUpstream, REPO_ROOT, verifyUpstream } from "../../tools/goldens/lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-cli", "tests", "fixtures", "chrome-export");
const CANVAS_EXPORT = "crates/excali-scene/tests/fixtures/canvas-export.json";
const UPSTREAM_FIXTURES = "fixtures/upstream/packages/excalidraw/tests/fixtures";
const ELEMENT_FIXTURE = `${UPSTREAM_FIXTURES}/elementFixture.ts`;
/** Upstream's scene-bearing PNG fixtures, by scene name. */
const EMBEDDED_PNGS = {
  test_embedded_v1: `${UPSTREAM_FIXTURES}/test_embedded_v1.png`,
  smiley_embedded_v2: `${UPSTREAM_FIXTURES}/smiley_embedded_v2.png`,
};
const FONTS = join(REPO_ROOT, "crates", "excali-text", "assets", "fonts");

/** The page's origin: canvas-export.json's (getExportSource()). */
const ORIGIN = "https://excalidraw.com";
/** Where upstream's font urls point (window.EXCALIDRAW_ASSET_PATH). */
const ASSET_PATH = `${ORIGIN}/vendored/`;

const ENTRY = `
export { exportToCanvas } from "./packages/excalidraw/scene/export";
export { exportToCanvas as utilsExportToCanvas } from "./packages/utils/src/export";
export { loadFromBlob } from "./packages/excalidraw/data/blob";
export { serializeAsJSON } from "./packages/excalidraw/data/json";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { getNonDeletedElements } from "@excalidraw/element";
export { FONT_FAMILY } from "@excalidraw/common";
export * as elementFixtures from "./packages/excalidraw/tests/fixtures/elementFixture";
`;

// The file dialogs, image resizing and the font subsetting worker: loading
// a scene file and drawing a canvas reach none of them.
const STUBS = [
  "browser-fs-access",
  "pica",
  "image-blob-reduce",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/subset/subset-main",
];

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

const usage = () => {
  process.stderr.write("usage: chrome_png_export.mjs [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { out: OUT_DIR };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

/**
 * The element fixture scenes: [name, elements], each element of
 * elementFixture.ts alone, and the export test's Nunito text.
 */
const ELEMENT_SCENES = `(up) => {
  const f = up.elementFixtures;
  return [
    ["element-rectangle", [f.rectangleFixture]],
    ["element-embeddable", [f.embeddableFixture]],
    ["element-ellipse", [f.ellipseFixture]],
    ["element-diamond", [f.diamondFixture]],
    ["element-rectangle-with-link", [f.rectangleWithLinkFixture]],
    ["element-text", [f.textFixture]],
    ["element-text-nunito", [{ ...f.textFixture, fontFamily: up.FONT_FAMILY.Nunito }]],
  ];
}`;

/** Runs in the page: every scene exported, as { name: { width, height, png } }. */
const exportAll = async ({ canvasScenes, files, elementScenes }) => {
  const up = globalThis.__upstream;
  const toPng = (canvas) =>
    new Promise((done, fail) =>
      canvas.toBlob(async (blob) => {
        if (!blob) return fail(new Error(`toBlob gave no blob for a ${canvas.width}x${canvas.height} canvas`));
        const bytes = new Uint8Array(await blob.arrayBuffer());
        let binary = "";
        for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
        done({ width: canvas.width, height: canvas.height, png: btoa(binary) });
      }, "image/png"),
    );
  const dimensions = (spec) =>
    spec && ((width, height) => ({ width: width * spec.factor, height: height * spec.factor, ...("scale" in spec ? { scale: spec.scale } : {}) }));
  // exportCanvas("png") of a loaded file (data/index.ts:98-192, as
  // App.onExportImage calls it)
  const exportFile = async (blob) => {
    const { elements, appState, files } = await up.loadFromBlob(blob, null, null);
    const canvas = await up.exportToCanvas(up.getNonDeletedElements(elements), appState, files, {
      exportBackground: appState.exportBackground,
      viewBackgroundColor: appState.viewBackgroundColor,
      exportingFrame: null,
    });
    return toPng(canvas);
  };
  const out = {};
  const sceneFiles = {};
  for (const s of canvasScenes) {
    let canvas;
    if (s.sizing.kind === "utils") {
      canvas = await up.utilsExportToCanvas({
        elements: s.elements,
        appState: s.utilsAppState,
        files: s.files,
        maxWidthOrHeight: s.sizing.maxWidthOrHeight ?? undefined,
        getDimensions: dimensions(s.sizing.getDimensions),
        exportPadding: s.opts.exportPadding ?? undefined,
      });
    } else {
      const frame = s.opts.exportingFrame ? s.elements.find((e) => e.id === s.opts.exportingFrame) : null;
      canvas = await up.exportToCanvas(s.elements, s.appState, s.files, {
        exportBackground: s.opts.exportBackground,
        viewBackgroundColor: s.opts.viewBackgroundColor,
        exportingFrame: frame,
        ...(s.opts.exportPadding == null ? {} : { exportPadding: s.opts.exportPadding }),
      });
    }
    out[s.name] = await toPng(canvas);
  }
  for (const [name, elements] of eval(elementScenes)(up)) {
    const text = up.serializeAsJSON(elements, up.getDefaultAppState(), {}, "local");
    sceneFiles[name] = text;
    out[name] = await exportFile(new Blob([text], { type: "application/json" }));
  }
  for (const [name, b64] of Object.entries(files)) {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    out[name] = await exportFile(new File([bytes], `${name}.png`, { type: "image/png" }));
  }
  return { out, sceneFiles };
};

const draw = async () => {
  const upstream = verifyUpstream();
  const bundle = await bundleUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    platform: "browser",
    format: "iife",
    globalName: "__upstream",
    fontUris: true,
    define: {
      "import.meta.env.MODE": '"production"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  const require = createRequire(join(REPO_ROOT, "tests", "web", "package.json"));
  const { chromium } = require("@playwright/test");
  const browser = await chromium.launch({ args: ["--disable-gpu", "--force-color-profile=srgb"] });
  try {
    const page = await browser.newPage({ deviceScaleFactor: 1 });
    const refused = [];
    const fontsServed = new Set();
    await page.route("**/*", (route) => {
      const url = new URL(route.request().url());
      if (url.origin === ORIGIN && url.pathname === "/") {
        return route.fulfill({ contentType: "text/html", body: "<!doctype html><html><body></body></html>" });
      }
      const prefix = new URL(ASSET_PATH).pathname + "fonts/";
      if (url.origin === ORIGIN && url.pathname.startsWith(prefix)) {
        const file = decodeURIComponent(url.pathname.slice(prefix.length));
        const path = join(FONTS, file);
        if (!file.includes("..") && file.endsWith(".woff2")) {
          try {
            const body = readFileSync(path);
            fontsServed.add(file);
            return route.fulfill({ contentType: "font/woff2", body });
          } catch {
            // not vendored: refused below
          }
        }
      }
      refused.push(url.href);
      return route.abort();
    });
    await page.addInitScript(
      ({ origin, assets }) => {
        window.EXCALIDRAW_EXPORT_SOURCE = origin;
        window.EXCALIDRAW_ASSET_PATH = assets;
      },
      { origin: ORIGIN, assets: ASSET_PATH },
    );
    await page.goto(`${ORIGIN}/`);
    const errors = [];
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(m.text());
    });
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.addScriptTag({ content: bundle });
    const canvasExport = JSON.parse(readFileSync(join(REPO_ROOT, CANVAS_EXPORT), "utf8"));
    const files = Object.fromEntries(
      Object.entries(EMBEDDED_PNGS).map(([name, path]) => [name, readFileSync(join(REPO_ROOT, path)).toString("base64")]),
    );
    const { out, sceneFiles } = await page.evaluate(exportAll, {
      canvasScenes: canvasExport.scenes,
      files,
      elementScenes: ELEMENT_SCENES,
    });
    if (refused.length) throw new Error(`the page requested what is not vendored:\n  ${refused.join("\n  ")}`);
    if (errors.length) throw new Error(`the page logged errors:\n  ${errors.join("\n  ")}`);
    if (!fontsServed.size) throw new Error("no vendored font was loaded");
    return {
      upstream,
      browser: `Chromium ${browser.version()} (@playwright/test, tests/web/package-lock.json), ${platform()} ${arch()}`,
      canvasScenes: canvasExport.scenes.map((s) => s.name),
      out,
      sceneFiles,
      fontsServed: [...fontsServed].sort(),
    };
  } finally {
    await browser.close();
  }
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  const drawn = await draw();
  rmSync(args.out, { recursive: true, force: true });
  mkdirSync(join(args.out, "scenes"), { recursive: true });
  const sourceHash = (path) => sha256(readFileSync(join(REPO_ROOT, path)));
  const scenes = {};
  const add = (name, source, bytes) => {
    const { width, height, png } = drawn.out[name];
    const data = Buffer.from(png, "base64");
    writeFileSync(join(args.out, `${name}.png`), data);
    scenes[name] = { source, sourceSha256: sha256(bytes), width, height, pngSha256: sha256(data) };
  };
  for (const name of drawn.canvasScenes) {
    add(name, `${CANVAS_EXPORT}#${name}`, readFileSync(join(REPO_ROOT, CANVAS_EXPORT)));
  }
  for (const [name, text] of Object.entries(drawn.sceneFiles)) {
    const file = join(args.out, "scenes", `${name}.excalidraw`);
    writeFileSync(file, text);
    add(name, `crates/excali-cli/tests/fixtures/chrome-export/scenes/${name}.excalidraw`, Buffer.from(text));
  }
  for (const [name, path] of Object.entries(EMBEDDED_PNGS)) add(name, path, readFileSync(join(REPO_ROOT, path)));
  const manifest = {
    generator: "scripts/fixtures/chrome_png_export.mjs",
    description:
      "Upstream's exportToCanvas at the pinned commit in Playwright's Chromium (software canvas, sRGB, device pixel ratio 1) " +
      "with the vendored fonts, per scene: its source and that source's SHA-256, the canvas size and the SHA-256 of the PNG " +
      "canvas.toBlob wrote. Element fixture scenes were written from upstream's elementFixture.ts " +
      `(${ELEMENT_FIXTURE}, SHA-256 ${sourceHash(ELEMENT_FIXTURE)}).`,
    upstream: drawn.upstream.commit,
    browser: drawn.browser,
    fonts: drawn.fontsServed,
    scenes,
  };
  writeFileSync(join(args.out, "manifest.json"), `${JSON.stringify(manifest, null, 1)}\n`);
  process.stderr.write(
    `chrome png export: ${Object.keys(scenes).length} scenes drawn by ${drawn.browser} into ${args.out}\n`,
  );
};

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
