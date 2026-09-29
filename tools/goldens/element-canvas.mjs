#!/usr/bin/env node
// Per-element bitmap cache goldens for excali-scene (ex-504): upstream's own
// renderElement (packages/element/src/renderElement.ts) on its editor path,
// which rasterises each element once into a canvas of its own
// (generateElementWithCanvas, :687-728; generateElementCanvas, :271-339;
// cappedElementCanvasSize, :216-269; getCanvasPadding, :102-116) and blits
// it (drawElementFromCanvas, :762-934, snapped to whole device pixels with
// smoothing off by canSnapElement, :747-752, and SNAP_TIE_BIAS, :760),
// run from the pinned checkout under Node on the recording 2D context of
// static-scene.mjs, unpatched.
//
//   node tools/goldens/element-canvas.mjs            write the fixture
//   node tools/goldens/element-canvas.mjs --check    exit 1 if it is stale
//   node tools/goldens/element-canvas.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-scene/tests/fixtures/element-canvas.json:
//
// - `cases`: one element drawn by renderElement on a canvas whose context is
//   scaled by the device pixel ratio and the zoom, as renderStaticScene
//   leaves it (staticScene.ts:299-310). Recorded: the inputs, the matrix
//   renderElement starts from (`base`), what elementWithCanvasCache holds
//   for the element afterwards (`cached`: the bitmap's width, height,
//   scale, canvasOffsetX/Y, zoomValue, theme, imageCrop and
//   containingFrameOpacity, and every draw into the bitmap, `events`, or
//   null when no bitmap was made), and every draw on the main context
//   (`events`), where the blit of a bitmap is recorded as it is called,
//   `{op: "blit", m, alpha, smoothing, args, width, height}`, instead of
//   inlining the bitmap's draws;
// - `sequences`: one element drawn again and again as the app state, its
//   frame or the element change: per step whether the cache holds a new
//   bitmap (`regenerated`: a different object than before the step), and
//   what it holds.
//
// Not recorded: the crop editor's uncropped preview of the image being
// cropped (`croppingElementId`, :1220-1251), which is ex-707's.
//
// Deterministic: Math.random throws while generating, every element is
// built with a fixed id and seed after reseed(), and text measures 10 px
// per UTF-16 code unit (setCustomTextMetricsProvider).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { contexts, installDom } from "./lib/recording-context.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { ORIGIN, RANDOM_SEED } from "./static-scene.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
export const OUT_FILE = "element-canvas.json";

const ENTRY = `
export { renderElement, elementWithCanvasCache } from "./packages/element/src/renderElement";
export { ShapeCache } from "./packages/element/src/shape";
export {
  newElement,
  newFrameElement,
  newTextElement,
  newFreeDrawElement,
  newLinearElement,
  newArrowElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
export { arrayToMap } from "@excalidraw/common";
export { default as rough } from "roughjs/bin/rough";
`;

const STUBS = [
  "packages/excalidraw/data/blob",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/subset/subset-main",
];

const usage = () => {
  process.stderr.write("usage: element-canvas.mjs [--check] [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

// -- inputs ---------------------------------------------------------------------

/** The images of the image cache: fileId → mimeType and natural size. */
const IMAGES = {
  "file-png": { mimeType: "image/png", naturalWidth: 64, naturalHeight: 48 },
};

const APP_STATE = {
  zoom: { value: 1 },
  scrollX: 0,
  scrollY: 0,
  theme: "light",
  shouldCacheIgnoreZoom: false,
  frameRendering: { enabled: true, clip: true, name: true, outline: true },
  selectedElementIds: {},
  hoveredElementIds: {},
  openDialog: null,
};

const RENDER_CONFIG = {
  canvasBackgroundColor: "#ffffff",
  renderGrid: false,
  isExporting: false,
  embedsValidationStatus: {},
  elementsPendingErasure: [],
  pendingFlowchartNodes: null,
  theme: "light",
};

const bind = (container, label) => {
  container.boundElements = [...(container.boundElements ?? []), { type: "text", id: label.id }];
  label.containerId = container.id;
  return [container, label];
};

const label = (up, id, text, extra = {}) =>
  up.newTextElement({ id, x: 0, y: 0, text, textAlign: "center", verticalAlign: "middle", seed: 40, ...extra });

const rect = (up, extra = {}) =>
  up.newElement({ type: "rectangle", id: "r", x: 10.3, y: 20.7, width: 120, height: 60, seed: 1, ...extra });

/**
 * One case: `draw` is the id of the element renderElement draws, `scale`
 * the device pixel ratio, `appState` and `renderConfig` overrides.
 */
const c = (name, elements, draw, { scale = 1, appState = {}, renderConfig = {} } = {}) => ({
  name,
  elements,
  draw,
  scale,
  appState: { ...APP_STATE, ...appState },
  renderConfig: { ...RENDER_CONFIG, ...renderConfig },
});

const cases = (up) => {
  const boxed = () =>
    bind(
      up.newElement({ type: "rectangle", id: "box", x: 40.5, y: 30.25, width: 121, height: 67, seed: 3 }),
      label(up, "box-label", "in a box", { x: 61, y: 52.75, fontSize: 20 }),
    );
  const turned = () =>
    bind(
      up.newElement({ type: "rectangle", id: "turned", x: 40.5, y: 30.25, width: 121, height: 67, seed: 3, angle: 0.3 }),
      label(up, "turned-label", "turned", { x: 71, y: 52.75, fontSize: 20, angle: 0.3 }),
    );
  const labelled = () =>
    bind(
      up.newArrowElement({ type: "arrow", id: "arrow", x: 20.4, y: 100.6, seed: 9, points: [[0, 0], [100, -40], [220, 10]], endArrowhead: "arrow", elbowed: false }),
      label(up, "arrow-label", "label", { x: 105, y: 67.5 }),
    );
  const framed = (opacity) => [
    up.newFrameElement({ id: "frame", x: 0, y: 0, width: 300, height: 200, seed: 40, opacity }),
    rect(up, { frameId: "frame", opacity: 60 }),
  ];
  const image = (extra = {}) =>
    up.newImageElement({ type: "image", id: "img", x: 12.5, y: 8.25, width: 60, height: 40, seed: 15, fileId: "file-png", status: "saved", ...extra });
  const crop = { x: 8, y: 4, width: 48, height: 32, naturalWidth: 64, naturalHeight: 48 };
  return [
    c("rectangle", [rect(up)], "r"),
    c("rectangle-zoom-dpr-scroll", [rect(up)], "r", { scale: 2, appState: { zoom: { value: 1.5 }, scrollX: 3.3, scrollY: -7.7 } }),
    c("rectangle-dpr-1.5-zoom-0.7", [rect(up)], "r", { scale: 1.5, appState: { zoom: { value: 0.7 }, scrollX: -11.13, scrollY: 5.51 } }),
    c("rectangle-dark", [rect(up, { backgroundColor: "#a5d8ff", fillStyle: "solid" })], "r", { appState: { theme: "dark" }, renderConfig: { theme: "dark" } }),
    c("rectangle-angle-0.4", [rect(up, { angle: 0.4 })], "r", { scale: 2, appState: { scrollX: 0.25 } }),
    c("rectangle-angle-half-pi", [rect(up, { angle: Math.PI / 2 })], "r", { scale: 1.5, appState: { zoom: { value: 1.1 }, scrollX: 0.3 } }),
    c("rectangle-angle-pi", [rect(up, { angle: Math.PI })], "r", { scale: 2, appState: { zoom: { value: 0.9 } } }),
    c("rectangle-angle-three-halves-pi", [rect(up, { angle: (3 * Math.PI) / 2 })], "r", { scale: 3 }),
    c("rectangle-zoom-gesture", [rect(up)], "r", { scale: 2, appState: { zoom: { value: 1.3 }, shouldCacheIgnoreZoom: true } }),
    c("ellipse", [up.newElement({ type: "ellipse", id: "e", x: -30.7, y: 5, width: 90, height: 45, seed: 5 })], "e", { scale: 1.25 }),
    c("diamond", [up.newElement({ type: "diamond", id: "d", x: 3, y: 4, width: 70, height: 50, seed: 4, roundness: { type: 2 } })], "d", { appState: { zoom: { value: 2 } } }),
    c("text", [up.newTextElement({ id: "t", x: 13.4, y: 17.9, text: "two\nlines", seed: 6, fontSize: 20 })], "t", { scale: 2 }),
    c("text-rtl", [up.newTextElement({ id: "t", x: 13.4, y: 17.9, text: "123 שלום", seed: 8, fontSize: 36 })], "t", { scale: 1.5 }),
    c(
      "freedraw",
      [up.newFreeDrawElement({ type: "freedraw", id: "f", x: 50.5, y: 60.5, seed: 14, strokeWidth: 4, points: [[0, 0], [-20, -10], [25, 5], [10, 40]], simulatePressure: true })],
      "f",
      { scale: 2, appState: { zoom: { value: 1.2 }, scrollX: 1.1 } },
    ),
    c(
      "freedraw-thin",
      [up.newFreeDrawElement({ type: "freedraw", id: "f", x: 5, y: 6, seed: 14, strokeWidth: 1, points: [[0, 0], [20, 10], [45, -5]], simulatePressure: true })],
      "f",
    ),
    c("line-left-of-origin", [up.newLinearElement({ type: "line", id: "l", x: 100, y: 50, seed: 10, points: [[0, 0], [-60, 10], [-30, -50]] })], "l", { scale: 2 }),
    c("arrow-end-arrowhead", [up.newArrowElement({ type: "arrow", id: "a", x: 20, y: 30, seed: 12, points: [[0, 0], [120, 30]], endArrowhead: "triangle", elbowed: false })], "a"),
    c("arrow-no-arrowhead", [up.newArrowElement({ type: "arrow", id: "a", x: 20, y: 30, seed: 12, points: [[0, 0], [120, 30]], startArrowhead: null, endArrowhead: null, elbowed: false })], "a"),
    c(
      "arrow-start-arrowhead-only",
      [up.newArrowElement({ type: "arrow", id: "a", x: 20, y: 30, seed: 12, points: [[0, 0], [120, 30]], startArrowhead: "arrow", endArrowhead: null, elbowed: false })],
      "a",
    ),
    c("arrow-with-label", labelled(), "arrow", { scale: 1.5, appState: { zoom: { value: 1.25 }, scrollX: 2.2, scrollY: 0.9 } }),
    c("arrow-with-label-rotated", labelled().map((e) => (e.id === "arrow" ? { ...e, angle: 0.5 } : e)), "arrow", { scale: 2 }),
    c("arrow-label", labelled(), "arrow-label", { scale: 1.5, appState: { zoom: { value: 1.25 }, scrollX: 2.2, scrollY: 0.9 } }),
    c("container", boxed(), "box", { scale: 1.5, appState: { zoom: { value: 1.25 }, scrollX: 0.1, scrollY: 0.7 } }),
    c("container-label", boxed(), "box-label", { scale: 1.5, appState: { zoom: { value: 1.25 }, scrollX: 0.1, scrollY: 0.7 } }),
    c("container-label-half-pixel", boxed(), "box-label", { scale: 1.5, appState: { scrollX: 1 / 3 } }),
    c("rotated-container-label", turned(), "turned-label", { scale: 2 }),
    c("image", [image()], "img", { scale: 2 }),
    c("image-crop-flip", [image({ crop, scale: [-1, 1] })], "img", { scale: 2, appState: { zoom: { value: 1.5 } } }),
    c("image-pending-flip", [image({ fileId: "file-missing", status: "pending", scale: [-1, -1] })], "img"),
    c("side-cap", [rect(up, { width: 40000, height: 100 })], "r"),
    c("side-cap-zoom", [rect(up, { width: 9000, height: 300 })], "r", { scale: 2, appState: { zoom: { value: 2 } } }),
    c("area-cap", [rect(up, { width: 5000, height: 5000 })], "r"),
    c("area-cap-zoom", [rect(up, { width: 3000, height: 2000 })], "r", { scale: 2, appState: { zoom: { value: 1.5 } } }),
    c("both-caps", [rect(up, { width: 30000, height: 30000 })], "r", { scale: 3 }),
    c("too-small-to-rasterise", [rect(up, { width: 0, height: 0 })], "r", { appState: { zoom: { value: 0.01 } } }),
    c("frame-opacity", framed(50), "r", { scale: 2 }),
    c("frame-opacity-0", framed(0), "r"),
    c("offset", [rect(up)], "r", { scale: 1.5, renderConfig: { elementRenderOverrides: { r: { offset: { x: 0.5, y: 1 / 3 } } } } }),
    c("pending-erasure", [rect(up)], "r", { renderConfig: { elementsPendingErasure: ["r"] } }),
  ];
};

/**
 * Sequences: `steps` of `{ label, appState?, scale?, frameOpacity?,
 * mutate?, replace?, crop? }` applied in turn before each draw.
 */
const sequences = (up) => {
  const crop = { x: 8, y: 4, width: 48, height: 32, naturalWidth: 64, naturalHeight: 48 };
  return [
    {
      name: "zoom-theme-scroll-dpr",
      elements: [rect(up)],
      draw: "r",
      steps: [
        { label: "first draw" },
        { label: "again" },
        { label: "scroll", appState: { scrollX: 13.5, scrollY: -2 } },
        { label: "zoom", appState: { zoom: { value: 1.5 } } },
        { label: "zoom gesture", appState: { zoom: { value: 2 }, shouldCacheIgnoreZoom: true } },
        { label: "zoom gesture again", appState: { zoom: { value: 2.5 }, shouldCacheIgnoreZoom: true } },
        { label: "gesture ends", appState: { zoom: { value: 2.5 }, shouldCacheIgnoreZoom: false } },
        { label: "dark", appState: { theme: "dark" } },
        { label: "dark again", appState: { theme: "dark" } },
        { label: "device pixel ratio", scale: 2 },
        { label: "light", appState: { theme: "light" } },
        { label: "zoom back", appState: { zoom: { value: 1 } } },
      ],
    },
    {
      name: "frame-opacity",
      elements: [
        up.newFrameElement({ id: "frame", x: 0, y: 0, width: 300, height: 200, seed: 40, opacity: 100 }),
        rect(up, { frameId: "frame" }),
      ],
      draw: "r",
      steps: [
        { label: "first draw" },
        { label: "frame at 50", frameOpacity: 50 },
        { label: "frame still at 50", frameOpacity: 50 },
        { label: "frame at 0 reads as 100", frameOpacity: 0 },
        { label: "frame at 100", frameOpacity: 100 },
      ],
    },
    {
      name: "image-crop",
      elements: [up.newImageElement({ type: "image", id: "img", x: 12.5, y: 8.25, width: 60, height: 40, seed: 15, fileId: "file-png", status: "saved" })],
      draw: "img",
      steps: [
        { label: "first draw" },
        { label: "cropped", crop },
        { label: "same crop", crop: "same" },
        { label: "uncropped", crop: null },
        { label: "uncropped again", crop: null },
      ],
    },
    {
      name: "element-changes",
      elements: [rect(up)],
      draw: "r",
      steps: [
        { label: "first draw" },
        { label: "new version (newElementWith)", replace: { width: 150 } },
        { label: "unchanged" },
        { label: "mutated (ShapeCache.delete)", mutate: { height: 90 } },
        { label: "unchanged after mutation" },
      ],
    },
  ];
};

// -- running upstream -------------------------------------------------------------

/** JSON round trip: what a scene file holds and the Rust side reads. */
const plain = (value) => JSON.parse(JSON.stringify(value));

const fakeImage = (fileId) => ({ __file: fileId, naturalWidth: IMAGES[fileId].naturalWidth, naturalHeight: IMAGES[fileId].naturalHeight });

const renderConfigOf = (rc) => ({
  ...rc,
  imageCache: new Map(Object.entries(IMAGES).map(([id, { mimeType }]) => [id, { image: fakeImage(id), mimeType }])),
  embedsValidationStatus: new Map(Object.entries(rc.embedsValidationStatus)),
  elementsPendingErasure: new Set(rc.elementsPendingErasure),
  elementRenderOverrides: rc.elementRenderOverrides ? new Map(Object.entries(rc.elementRenderOverrides)) : undefined,
});

const setScale = (window, scale) => {
  window.devicePixelRatio = scale;
  globalThis.devicePixelRatio = scale;
};

/** A main canvas scaled as renderStaticScene leaves it; its bitmap blits are
 * recorded as calls. */
const mainContext = (window, scale, zoom) => {
  const canvas = window.document.createElement("canvas");
  canvas.width = 800;
  canvas.height = 600;
  const context = canvas.getContext("2d");
  context.scale(scale, scale);
  context.scale(zoom, zoom);
  const base = [...context.s.m];
  const drawImage = context.drawImage.bind(context);
  context.drawImage = (img, ...args) => {
    if (!contexts.has(img)) return drawImage(img, ...args);
    context.events.push({
      op: "blit",
      m: [...context.s.m],
      alpha: context.s.alpha,
      smoothing: context.s.smoothing,
      args,
      width: img.width,
      height: img.height,
    });
  };
  return { canvas, context, base };
};

const cachedOf = (up, element) => {
  const cached = up.elementWithCanvasCache.get(element);
  if (!cached) return null;
  return {
    width: cached.canvas.width,
    height: cached.canvas.height,
    scale: cached.scale,
    zoomValue: cached.zoomValue,
    theme: cached.theme,
    canvasOffsetX: cached.canvasOffsetX,
    canvasOffsetY: cached.canvasOffsetY,
    imageCrop: cached.imageCrop,
    containingFrameOpacity: cached.containingFrameOpacity,
  };
};

const draw = (up, window, { elements, id, scale, appState, renderConfig }) => {
  setScale(window, scale);
  const map = up.arrayToMap(elements);
  const element = map.get(id);
  const { canvas, context, base } = mainContext(window, scale, appState.zoom.value);
  up.renderElement(element, map, map, up.rough.canvas(canvas), context, renderConfigOf(renderConfig), appState);
  return { element, base, events: context.events };
};

const runCase = (up, window, s) => {
  const elements = plain(s.elements);
  const appState = plain(s.appState);
  const renderConfig = plain(s.renderConfig);
  const { element, base, events } = draw(up, window, { elements, id: s.draw, scale: s.scale, appState, renderConfig });
  const entry = up.elementWithCanvasCache.get(element);
  const cached = cachedOf(up, element);
  return {
    name: s.name,
    scale: s.scale,
    elements,
    draw: s.draw,
    appState,
    renderConfig,
    base,
    cached: cached && { ...cached, events: contexts.get(entry.canvas).events },
    events,
  };
};

const runSequence = (up, window, q) => {
  let elements = plain(q.elements);
  let appState = plain(APP_STATE);
  let scale = 1;
  const renderConfig = plain(RENDER_CONFIG);
  const steps = [];
  let before = null;
  for (const step of q.steps) {
    if (step.appState) appState = { ...appState, ...plain(step.appState) };
    if (step.scale) scale = step.scale;
    if (step.frameOpacity !== undefined) {
      // a new frame object; the drawn element stays the same object
      elements = elements.map((e) => (e.id === "frame" ? { ...e, opacity: step.frameOpacity, version: e.version + 1 } : e));
    }
    if (step.replace) {
      // newElementWith: a new object with a bumped version
      elements = elements.map((e) => (e.id === q.draw ? { ...e, ...step.replace, version: e.version + 1, versionNonce: e.versionNonce + 1 } : e));
    }
    const target = elements.find((e) => e.id === q.draw);
    if (step.mutate) {
      // mutateElement: in place, a bumped version, ShapeCache.delete
      Object.assign(target, step.mutate, { version: target.version + 1, versionNonce: target.versionNonce + 1 });
      up.ShapeCache.delete(target);
    }
    if (step.crop !== undefined && step.crop !== "same") {
      // a new crop object assigned in place, nothing else changed: the
      // cache's own crop comparison (the object stays the WeakMap key)
      target.crop = plain(step.crop);
    }
    const { element } = draw(up, window, { elements, id: q.draw, scale, appState, renderConfig: renderConfig });
    const entry = up.elementWithCanvasCache.get(element) ?? null;
    steps.push({
      label: step.label,
      scale,
      appState: plain(appState),
      elements: plain(elements),
      regenerated: entry !== before,
      cached: cachedOf(up, element),
    });
    before = entry;
  }
  return { name: q.name, draw: q.draw, renderConfig, steps };
};

const build = async (upstream) => {
  const window = installDom(ORIGIN);
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
      // the debug outline of text containers (drawElementFromCanvas,
      // :921-932) stays off, as in a production build
      "import.meta.env.VITE_APP_DEBUG_ENABLE_TEXT_CONTAINER_BOUNDING_BOX": "undefined",
    },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  up.reseed(RANDOM_SEED);
  const out = cases(up).map((s) => runCase(up, window, s));
  up.reseed(RANDOM_SEED);
  const seqs = sequences(up).map((q) => runSequence(up, window, q));
  return format({
    description:
      "Upstream renderElement (packages/element/src/renderElement.ts) at the pinned commit on its editor path, the per-element bitmap cache, on a recording 2D context (tools/goldens/element-canvas.mjs): per case the element's cached bitmap (size, scale, offsets, cache key and its draws) and the main context's draws with each blit as a call; per sequence whether each step regenerated the bitmap. Text measures 10 px per UTF-16 code unit.",
    upstream: upstream.commit,
    images: IMAGES,
    cases: out,
    sequences: seqs,
  });
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating element-canvas goldens");
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
    process.stderr.write(`element-canvas: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("element-canvas goldens are out of date: run node tools/goldens/element-canvas.mjs\n");
      process.exit(1);
    }
    process.stdout.write("element-canvas goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();
