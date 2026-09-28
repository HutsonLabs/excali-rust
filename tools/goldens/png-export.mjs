#!/usr/bin/env node
// PNG export goldens for excali-scene and excali-raster (ex-405): upstream's
// own exportToCanvas (packages/excalidraw/scene/export.ts:180-285) and the
// utils wrapper's (packages/utils/src/export.ts:42-105) run from the pinned
// checkout under Node, drawing on the recording 2D context of
// lib/recording-context.mjs.
//
//   node tools/goldens/png-export.mjs                write the fixture
//   node tools/goldens/png-export.mjs --check        exit 1 if it is stale
//   node tools/goldens/png-export.mjs --out DIR      write (or --check) DIR
//   node tools/goldens/png-export.mjs --reimport DIR have upstream load the
//                                                    PNGs the port wrote
//
// Writes crates/excali-scene/tests/fixtures/canvas-export.json: per scene
// the inputs (elements, the app state, files, the options object and how
// the canvas is sized) and what upstream made of them: the canvas's width
// and height as the canvas element holds them (the `width` attribute
// reflects an unsigned long: truncated, and 300 when out of range), and
// every draw on it, in order, as static-scene.mjs records them. A scene
// with `exportEmbedScene` also records the scene text a PNG export embeds:
// serializeAsJSON(elements, appState, files, "local") of the editor's
// exportCanvas (data/index.ts:173-186), or of the utils exportToBlob
// (utils/src/export.ts:136-160) for a utils scene.
//
// Sizing: `{ kind: "exportScale" }` is exportToCanvas's default
// createCanvas (width and height times appState.exportScale, drawn at that
// scale); `{ kind: "utils", ... }` is the utils wrapper's: maxWidthOrHeight
// (the scale that fits the larger side, or appState.exportScale ?? 1 when
// the content is smaller), else getDimensions, recorded as `{ factor,
// scale }` for `(w, h) => ({ width: w * factor, height: h * factor, scale
// })`, else the content size at scale 1. For a utils scene `elements` and
// `appState` are what the wrapper hands exportToCanvas (restoreElements
// with deleteInvisibleElements, restoreAppState with offsets and size 0),
// computed with upstream's own restore; `utilsAppState` is what the caller
// passed.
//
// --reimport DIR reads every <name>.png the port wrote (the png_export
// example of excali-raster) with <name>.json beside it ({ scene, width,
// height, metadata }: the golden scene it came from, its canvas size and
// the scene text it embedded), and requires of each, from upstream's own
// code: the IHDR size is the scene's canvas size; decodePngMetadata gives
// the embedded text back exactly; and loadFromBlob of the PNG restores the
// same scene (serializeAsJSON of the result) as loadFromBlob of upstream's
// own embedded text for that scene.
//
// Frames here have no children except in the exporting-frame scene, where
// exportToCanvas turns frame clipping off: clipping is ex-403's.
//
// Deterministic: Math.random throws while generating, elements are built
// with fixed ids and seeds after reseed(), text measures 10 px per UTF-16
// code unit, and images load from a fixed table of data URLs.

import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { contexts, installDom } from "./lib/recording-context.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
export const OUT_FILE = "canvas-export.json";
/** window.location of the page: getExportSource() when embedding. */
export const ORIGIN = "https://excalidraw.com";
export const RANDOM_SEED = 1700000000000;

const ENTRY = `
export { exportToCanvas } from "./packages/excalidraw/scene/export";
export { exportToCanvas as utilsExportToCanvas } from "./packages/utils/src/export";
export { serializeAsJSON } from "./packages/excalidraw/data/json";
export { restoreElements, restoreAppState } from "./packages/excalidraw/data/restore";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { getNonDeletedElements } from "@excalidraw/element";
export { Fonts } from "./packages/excalidraw/fonts";
export {
  newElement,
  newEmbeddableElement,
  newIframeElement,
  newFrameElement,
  newTextElement,
  newArrowElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
`;

const REIMPORT_ENTRY = `
export { loadFromBlob } from "./packages/excalidraw/data/blob";
export { decodePngMetadata } from "./packages/excalidraw/data/image";
export { serializeAsJSON } from "./packages/excalidraw/data/json";
export { reseed } from "./packages/common/src/random";
`;

// data/blob and data/filesystem: the file dialogs and image resizing, which
// nothing here reaches; subset-main starts the font subsetting worker.
const STUBS = [
  "packages/excalidraw/data/blob",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/subset/subset-main",
];

// As document-fixtures.mjs: loadFromBlob of a scene file reaches none of
// these.
const REIMPORT_STUBS = [
  "browser-fs-access",
  "pica",
  "image-blob-reduce",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/fonts",
  "packages/excalidraw/renderer/staticScene",
  "packages/excalidraw/renderer/staticSvgScene",
];

const usage = () => {
  process.stderr.write("usage: png-export.mjs [--check] [--out DIR] [--reimport DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: null, reimport: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else if (argv[i] === "--reimport" && argv[i + 1]) args.reimport = resolve(argv[++i]);
    else usage();
  }
  return args;
};

// -- images -----------------------------------------------------------------------

const PNG_URL = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
const BROKEN_URL = "data:image/png;base64,AAAA";

/** The data URLs images load from: fileId and natural size. */
const LOADABLE = { [PNG_URL]: { file: "file-png", naturalWidth: 64, naturalHeight: 48 } };

/** The files of the image scene (BinaryFiles). */
const FILES = {
  "file-png": { mimeType: "image/png", id: "file-png", dataURL: PNG_URL, created: 1 },
  "file-broken": { mimeType: "image/png", id: "file-broken", dataURL: BROKEN_URL, created: 1 },
  "file-binary": { mimeType: "application/octet-stream", id: "file-binary", dataURL: PNG_URL, created: 1 },
};

/**
 * HTMLImageElement for loadHTMLImageElement (packages/element/src/image.ts):
 * a data URL of LOADABLE loads with its natural size, any other fails.
 */
class FakeImage {
  set src(url) {
    this._src = url;
    queueMicrotask(() => {
      const known = LOADABLE[url];
      if (known) {
        this.__file = known.file;
        this.naturalWidth = known.naturalWidth;
        this.naturalHeight = known.naturalHeight;
        this.width = known.naturalWidth;
        this.height = known.naturalHeight;
        this.onload?.();
      } else {
        this.onerror?.(new Error(`cannot load ${url.slice(0, 40)}`));
      }
    });
  }
  get src() {
    return this._src;
  }
}

// -- scenes ---------------------------------------------------------------------

const basic = (up) => [
  up.newElement({ type: "rectangle", id: "rect", x: 10, y: 20, width: 120, height: 60, seed: 1, backgroundColor: "#a5d8ff", fillStyle: "hachure" }),
  up.newElement({ type: "ellipse", id: "ellipse", x: 160, y: 40, width: 100, height: 50, seed: 2, angle: 0.3, strokeStyle: "dashed" }),
  up.newTextElement({ id: "text", x: 20, y: 120, text: "export me", seed: 3, fontSize: 20 }),
  up.newArrowElement({ type: "arrow", id: "arrow", x: 150, y: 130, seed: 4, points: [[0, 0], [100, 30]], endArrowhead: "arrow", elbowed: false }),
  up.newElement({ type: "diamond", id: "diamond", x: -40, y: 90, width: 60, height: 60, seed: 5, roughness: 2, opacity: 70 }),
];

const fractional = (up) => [
  up.newElement({ type: "rectangle", id: "frac-rect", x: 0.3, y: -7.65, width: 100.45, height: 33.3, seed: 6 }),
  up.newElement({ type: "ellipse", id: "frac-ellipse", x: 120.15, y: 10.05, width: 41.7, height: 20.9, seed: 7 }),
];

const frames = (up) => [
  up.newFrameElement({ id: "frame-short", x: 300, y: 50, width: 200, height: 120, seed: 10, name: "Short" }),
  up.newFrameElement({ id: "frame-long", x: 20, y: 250, width: 80, height: 60, seed: 11, name: "A frame name far wider than its frame" }),
  up.newFrameElement({ id: "frame-unnamed", x: 150, y: 260, width: 100, height: 40, seed: 12 }),
  up.newElement({ type: "rectangle", id: "outside", x: 10, y: 10, width: 60, height: 40, seed: 13 }),
];

const exportingFrame = (up) => [
  up.newFrameElement({ id: "target", x: 0, y: 0, width: 200, height: 150, seed: 20, name: "Target" }),
  up.newElement({ type: "rectangle", id: "inside", x: 20, y: 20, width: 60, height: 40, seed: 21, frameId: "target" }),
  up.newElement({ type: "ellipse", id: "sticking-out", x: 150, y: 100, width: 100, height: 100, seed: 22, frameId: "target" }),
  up.newElement({ type: "diamond", id: "overlapping-free", x: -30, y: 120, width: 60, height: 60, seed: 23 }),
  up.newFrameElement({ id: "other", x: 400, y: 0, width: 100, height: 100, seed: 24 }),
  up.newElement({ type: "rectangle", id: "other-child", x: 120, y: 30, width: 50, height: 30, seed: 25, frameId: "other" }),
  up.newElement({ type: "rectangle", id: "far", x: 600, y: 600, width: 20, height: 20, seed: 26 }),
];

const images = (up) => [
  up.newImageElement({ type: "image", id: "image", x: 0, y: 0, width: 64, height: 48, seed: 30, fileId: "file-png", status: "saved" }),
  up.newImageElement({ type: "image", id: "image-broken", x: 80, y: 0, width: 50, height: 50, seed: 31, fileId: "file-broken", status: "saved" }),
  up.newImageElement({ type: "image", id: "image-binary", x: 150, y: 0, width: 40, height: 40, seed: 32, fileId: "file-binary", status: "saved" }),
  up.newImageElement({ type: "image", id: "image-missing", x: 0, y: 70, width: 60, height: 30, seed: 33, fileId: "file-missing", status: "saved" }),
  up.newImageElement({ type: "image", id: "image-no-file", x: 80, y: 70, width: 30, height: 30, seed: 34, fileId: null }),
];

const embeds = (up) => [
  up.newEmbeddableElement({ type: "embeddable", id: "embed", x: 0, y: 0, width: 160, height: 90, seed: 40, link: "https://www.youtube.com/watch?v=abc" }),
  up.newIframeElement({ type: "iframe", id: "iframe", x: 180, y: 0, width: 120, height: 80, seed: 41 }),
  up.newElement({ type: "rectangle", id: "under", x: 100, y: 40, width: 120, height: 80, seed: 42, backgroundColor: "#ffc9c9", fillStyle: "solid" }),
];

const small = (up) => [up.newElement({ type: "rectangle", id: "small", x: 5, y: 5, width: 50, height: 50, seed: 50 })];

/** An editor scene: exportToCanvas with the default createCanvas. */
const scene = (name, elements, { appState = {}, opts = {}, files = null } = {}) => ({ name, kind: "editor", elements, appState, opts, files });

/** A utils scene: the wrapper with `options` besides elements and files. */
const utils = (name, elements, { appState = {}, maxWidthOrHeight, getDimensions, exportPadding, files = null } = {}) => ({
  name,
  kind: "utils",
  elements,
  utilsAppState: appState,
  maxWidthOrHeight,
  getDimensions,
  exportPadding,
  files,
});

const scenes = (up) => [
  scene("default", basic(up)),
  scene("scale-2", basic(up), { appState: { exportScale: 2 } }),
  scene("scale-3-embed", basic(up), { appState: { exportScale: 3, exportEmbedScene: true } }),
  scene("fractional-scale-1.5", fractional(up), { appState: { exportScale: 1.5 } }),
  scene("padding-0", basic(up), { opts: { exportPadding: 0 } }),
  scene("padding-25.5", fractional(up), { opts: { exportPadding: 25.5 } }),
  scene("no-background", basic(up), { appState: { exportBackground: false, exportEmbedScene: true } }),
  scene("background-transparent", basic(up), { appState: { viewBackgroundColor: "transparent" } }),
  scene("background-coloured", basic(up), { appState: { viewBackgroundColor: "#fff9db", exportScale: 2 } }),
  scene("dark", basic(up), { appState: { exportWithDarkMode: true, viewBackgroundColor: "#ffc9c9", exportEmbedScene: true } }),
  scene("frames", frames(up), { appState: { exportEmbedScene: true } }),
  scene("frames-dark", frames(up), { appState: { exportWithDarkMode: true } }),
  scene("frames-names-off", frames(up), { appState: { frameRendering: { enabled: true, clip: true, name: false, outline: true } } }),
  scene("frames-disabled", frames(up), { appState: { frameRendering: { enabled: false, clip: true, name: true, outline: true } } }),
  scene("exporting-frame", exportingFrame(up), { opts: { exportingFrame: "target", exportPadding: 30 }, appState: { exportScale: 2, exportEmbedScene: true } }),
  scene("images", images(up), { files: FILES, appState: { exportEmbedScene: true } }),
  scene("images-dark", images(up), { files: FILES, appState: { exportWithDarkMode: true } }),
  scene("embeddables", embeds(up)),
  scene("negative-size", small(up), { opts: { exportPadding: -100 } }),
  utils("utils-max-smaller", basic(up), { maxWidthOrHeight: 100, appState: { exportScale: 2 } }),
  utils("utils-max-larger", basic(up), { maxWidthOrHeight: 5000, appState: { exportScale: 2, exportEmbedScene: true } }),
  utils("utils-max-larger-no-scale", basic(up), { maxWidthOrHeight: 5000 }),
  utils("utils-get-dimensions", basic(up), { getDimensions: { factor: 0.5, scale: 0.5 }, exportPadding: 4 }),
  utils("utils-get-dimensions-no-scale", fractional(up), { getDimensions: { factor: 2 } }),
  utils("utils-max-and-get-dimensions", basic(up), { maxWidthOrHeight: 120, getDimensions: { factor: 3, scale: 3 } }),
  utils("utils-plain", basic(up), { appState: { exportScale: 3, exportBackground: false } }),
];

// -- running upstream -------------------------------------------------------------

/** JSON round trip: what a scene file holds and the Rust side reads. */
const plain = (value) => (value === undefined ? undefined : JSON.parse(JSON.stringify(value)));

const dimensions = (spec) =>
  spec && ((width, height) => ({ width: width * spec.factor, height: height * spec.factor, ...("scale" in spec ? { scale: spec.scale } : {}) }));

const recorded = (canvas) => ({
  width: canvas.width,
  height: canvas.height,
  events: contexts.get(canvas)?.events ?? [],
});

const runEditor = async (up, s) => {
  const elements = plain(s.elements);
  const appState = plain({ ...up.getDefaultAppState(), ...s.appState });
  const files = plain(s.files ?? {});
  const frame = s.opts.exportingFrame ? elements.find((e) => e.id === s.opts.exportingFrame) : null;
  const opts = {
    exportBackground: appState.exportBackground,
    viewBackgroundColor: appState.viewBackgroundColor,
    exportingFrame: frame,
    ...("exportPadding" in s.opts ? { exportPadding: s.opts.exportPadding } : {}),
  };
  const canvas = await up.exportToCanvas(elements, appState, files, opts, undefined, async () => {});
  return {
    name: s.name,
    elements,
    appState,
    files,
    opts: {
      exportBackground: opts.exportBackground,
      viewBackgroundColor: opts.viewBackgroundColor,
      exportPadding: s.opts.exportPadding ?? null,
      exportingFrame: s.opts.exportingFrame ?? null,
    },
    sizing: { kind: "exportScale" },
    ...recorded(canvas),
    ...(appState.exportEmbedScene ? { metadata: up.serializeAsJSON(elements, appState, files, "local") } : {}),
  };
};

const runUtils = async (up, s) => {
  const input = plain(s.elements);
  const utilsAppState = plain(s.utilsAppState);
  const files = plain(s.files);
  // what the wrapper hands exportToCanvas (utils/src/export.ts:52-62)
  const restoredElements = up.getNonDeletedElements(up.restoreElements(plain(input), null, { deleteInvisibleElements: true }));
  const restoredAppState = up.restoreAppState(plain(utilsAppState), null);
  const appState = plain({ ...restoredAppState, offsetTop: 0, offsetLeft: 0, width: 0, height: 0 });
  const canvas = await up.utilsExportToCanvas({
    elements: plain(input),
    appState: plain(utilsAppState),
    files,
    maxWidthOrHeight: s.maxWidthOrHeight,
    getDimensions: dimensions(s.getDimensions),
    exportPadding: s.exportPadding,
  });
  return {
    name: s.name,
    elements: plain(restoredElements),
    appState,
    files: files ?? {},
    opts: {
      exportBackground: appState.exportBackground,
      viewBackgroundColor: appState.viewBackgroundColor,
      exportPadding: s.exportPadding ?? null,
      exportingFrame: null,
    },
    sizing: {
      kind: "utils",
      maxWidthOrHeight: s.maxWidthOrHeight ?? null,
      exportScale: utilsAppState.exportScale ?? null,
      getDimensions: s.getDimensions ?? null,
    },
    utilsAppState,
    ...recorded(canvas),
    // exportToBlob (utils/src/export.ts:136-160)
    ...(utilsAppState.exportEmbedScene
      ? { metadata: up.serializeAsJSON(up.restoreElements(plain(input), null), utilsAppState, files || {}, "local") }
      : {}),
  };
};

const load = (upstream, entry, stubs) =>
  loadUpstream(upstream, {
    entry,
    stubs,
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
    fontUris: true,
  });

const installGlobals = (window) => {
  window.EXCALIDRAW_EXPORT_SOURCE = ORIGIN;
  globalThis.Image = FakeImage;
  window.Image = FakeImage;
};

const build = async (upstream) => {
  const window = installDom(ORIGIN);
  installGlobals(window);
  const up = await load(upstream, ENTRY, STUBS);
  // Fonts.loadElementsFonts waits on the FontFace API; nothing is drawn
  // with a loaded face here (fillText is recorded, not rasterised)
  up.Fonts.loadElementsFonts = async () => {};
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  const out = [];
  for (const name of scenes(up).map((s) => s.name)) {
    up.reseed(RANDOM_SEED);
    const s = scenes(up).find((x) => x.name === name);
    out.push(s.kind === "editor" ? await runEditor(up, s) : await runUtils(up, s));
  }
  return format({
    description:
      "Upstream exportToCanvas (packages/excalidraw/scene/export.ts:180-285) and the utils wrapper (packages/utils/src/export.ts:42-105) at the pinned commit on a recording 2D context (tools/goldens/png-export.mjs): per scene the inputs, the canvas width and height, every draw in order, and with exportEmbedScene the scene text a PNG export embeds. Text measures 10 px per UTF-16 code unit.",
    upstream: upstream.commit,
    origin: ORIGIN,
    scenes: out,
  });
};

// -- re-importing the port's PNGs -----------------------------------------------------

/** FileReader#readAsText for parseFileContents (see document-fixtures.mjs). */
class NodeFileReader {
  static DONE = 2;
  readAsText(blob) {
    blob.text().then((text) => {
      this.readyState = NodeFileReader.DONE;
      this.result = text;
      this.onloadend?.();
    });
  }
}

/** The IHDR width and height of a PNG. */
const ihdr = (bytes) => {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const name = String.fromCharCode(...bytes.subarray(12, 16));
  if (name !== "IHDR") throw new Error("first chunk is not IHDR");
  return [view.getUint32(16), view.getUint32(20)];
};

const reimport = async (upstream, dir) => {
  const fixture = JSON.parse(readFileSync(join(OUT_DIR, OUT_FILE), "utf8"));
  const byName = new Map(fixture.scenes.map((s) => [s.name, s]));
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  Object.assign(globalThis.window, { EXCALIDRAW_EXPORT_SOURCE: ORIGIN, atob: globalThis.atob, btoa: globalThis.btoa });
  globalThis.FileReader = NodeFileReader;
  const up = await load(upstream, REIMPORT_ENTRY, REIMPORT_STUBS);
  const loadAndSave = async (blob) => {
    up.reseed(1);
    const { elements, appState, files } = await up.loadFromBlob(blob, null, null);
    return up.serializeAsJSON(elements, appState, files, "local");
  };
  const pngs = readdirSync(dir).filter((f) => f.endsWith(".png")).sort();
  if (!pngs.length) throw new Error(`no PNGs in ${dir}`);
  const failures = [];
  let embedded = 0;
  for (const file of pngs) {
    const name = file.slice(0, -4);
    const meta = JSON.parse(readFileSync(join(dir, `${name}.json`), "utf8"));
    const scene = byName.get(meta.scene);
    if (!scene) {
      failures.push(`${file}: no golden scene ${meta.scene}`);
      continue;
    }
    const bytes = new Uint8Array(readFileSync(join(dir, file)));
    const [w, h] = ihdr(bytes);
    if (w !== scene.width || h !== scene.height) {
      failures.push(`${file}: ${w}x${h}, upstream's canvas is ${scene.width}x${scene.height}`);
    }
    if (meta.metadata == null) continue;
    const blob = new Blob([bytes], { type: "image/png" });
    const decoded = await up.decodePngMetadata(blob);
    if (decoded !== meta.metadata) {
      failures.push(`${file}: decodePngMetadata does not give back the embedded text`);
      continue;
    }
    const fromPng = await loadAndSave(new Blob([bytes], { type: "image/png" }));
    const fromUpstream = await loadAndSave(new Blob([scene.metadata], { type: "application/json" }));
    if (fromPng !== fromUpstream) {
      failures.push(`${file}: loadFromBlob restores a different scene than upstream's own export of ${scene.name}`);
      continue;
    }
    embedded += 1;
  }
  if (failures.length) {
    for (const f of failures) process.stderr.write(`${f}\n`);
    process.exit(1);
  }
  if (!embedded) throw new Error("no PNG with an embedded scene");
  process.stdout.write(`upstream re-imports the port's PNGs: ${pngs.length} checked, ${embedded} scenes restored\n`);
};

/** Runs fn with Math.random disabled and Date.now fixed at 1. */
const deterministic = async (fn) => {
  const random = Math.random;
  const now = Date.now;
  const error = console.error;
  const warn = console.warn;
  Math.random = () => {
    throw new Error("Math.random called while generating png-export goldens");
  };
  Date.now = () => 1;
  // restore logs what it repairs; the utils wrapper warns about ignored
  // options
  console.error = () => {};
  console.warn = () => {};
  try {
    return await fn();
  } finally {
    Math.random = random;
    Date.now = now;
    console.error = error;
    console.warn = warn;
  }
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`png-export: ${error.message}\n`);
    process.exit(1);
  }
  if (args.reimport) {
    await deterministic(() => reimport(upstream, args.reimport));
    return;
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("png-export goldens are out of date: run node tools/goldens/png-export.mjs\n");
      process.exit(1);
    }
    process.stdout.write("png-export goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
