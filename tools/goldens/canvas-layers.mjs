#!/usr/bin/env node
// Canvas layer goldens for excali-ui (ex-503): upstream's own
// snapScrollToDevicePixels, getNormalizedCanvasDimensions and
// bootstrapCanvas (packages/excalidraw/renderer/helpers.ts:47-127) and
// renderNewElementScene (renderer/renderNewElementScene.ts), run from the
// pinned checkout under Node on the recording 2D context of
// static-scene.mjs.
//
//   node tools/goldens/canvas-layers.mjs            write the fixture
//   node tools/goldens/canvas-layers.mjs --check    exit 1 if it is stale
//   node tools/goldens/canvas-layers.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-ui/tests/fixtures/canvas-layers.json:
//
// - `snapScroll`: snapScrollToDevicePixels({scrollX, scrollY, zoom}, scale)
//   on a table of scrolls, zooms and device pixel ratios: the scroll it
//   returns, and whether it returned its argument itself (`identity`);
// - `bootstrap`: bootstrapCanvas on a fresh canvas per case, as each layer
//   calls it: the static canvas with the app state's theme and view
//   background colour (staticScene.ts:299-307), the new-element and
//   interactive canvases with neither (renderNewElementScene.ts:38-43,
//   interactiveScene.ts:1645-1650). The canvas's backing size is set the
//   way that layer's component sets it: the static canvas by assigning
//   `canvas.width = appState.width * scale` (StaticCanvas.tsx:38-41), the
//   other two through React's `width={appState.width * scale}` attribute
//   (NewElementCanvas.tsx:56-57, InteractiveCanvas.tsx:208-209), and the
//   normalized size is getNormalizedCanvasDimensions of it. Every draw
//   (clearRect as `clear`, fillRect) is recorded;
// - `newElementScenes`: renderNewElementScene (unthrottled) per scene: the
//   inputs (the canvas's CSS size, device pixel ratio, scene elements, the
//   new element, app state and render config) and every draw, in order.
//
// As in static-scene.mjs (see PATCH there), elements are drawn as vectors,
// renderElement's export path, instead of through the per-element bitmap
// cache, which is ex-504's.
//
// Deterministic: Math.random throws while generating, every element is
// built with a fixed id and seed after reseed(), and text measures 10 px
// per UTF-16 code unit (setCustomTextMetricsProvider).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { installDom } from "./lib/recording-context.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { ORIGIN, RANDOM_SEED } from "./static-scene.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui", "tests", "fixtures");
export const OUT_FILE = "canvas-layers.json";

const ENTRY = `
export {
  snapScrollToDevicePixels,
  getNormalizedCanvasDimensions,
  bootstrapCanvas,
} from "./packages/excalidraw/renderer/helpers";
export { renderNewElementScene } from "./packages/excalidraw/renderer/renderNewElementScene";
export {
  newElement,
  newFrameElement,
  newTextElement,
  newFreeDrawElement,
  newLinearElement,
  newArrowElement,
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

/** static-scene.mjs's patch: every element drawn as vectors. */
const PATCH = {
  "packages/element/src/renderElement": (source) => {
    const exporting = "      if (renderConfig.isExporting) {";
    const count = source.split(exporting).length - 1;
    if (count !== 2) throw new Error(`renderElement.ts changed: ${count} isExporting branches in drawElement`);
    const offset = "    (renderConfig.isExporting || isFrameLikeElement(element)) &&";
    if (!source.includes(offset)) throw new Error("renderElement.ts changed: no offset translation");
    return source
      .replaceAll(exporting, "      if (renderConfig.isExporting || globalThis.__vectorElements) {")
      .replace(offset, "    (renderConfig.isExporting || globalThis.__vectorElements || isFrameLikeElement(element)) &&");
  },
};

const usage = () => {
  process.stderr.write("usage: canvas-layers.mjs [--check] [--out DIR]\n");
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

// -- snapScrollToDevicePixels -------------------------------------------------

/** [scrollX, scrollY, zoom, scale] */
const SNAP_CASES = [
  // pixelSnap.test.tsx "rounds the scroll to whole device pixels"
  [3.3, -7.77, 1.5, 2],
  // … "and is identity when it already is"
  [4, -6, 1.5, 2],
  [0, 0, 1, 1],
  [3.3, -7.77, 1, 1],
  [3.3 + 0.1, -7.77, 1, 1],
  [3.3, -7.77, 1.5, 1],
  [3.3 + 0.1 / 1.5, -7.77, 1.5, 1],
  [3.3, -7.77, 0.73, 1],
  [3.3, -7.77, 2.2, 1],
  [10.3, -4.21, 1.37, 2],
  [0.4, 0.9, 1, 1.5],
  [0.3, 0.7, 0.73, 1.25],
  [7.3, -3.6, 1.5, 2],
  // Math.round's halves go up: -0.5 → -0, 2.5 → 3, -2.5 → -2
  [-0.5, 2.5, 1, 1],
  [-2.5, 0.5, 1, 1],
  [-0.25, 0.25, 2, 1],
  [1 / 3, -2 / 3, 3, 1],
  [123456.789, -98765.4321, 0.1, 3],
  [-1234.5678, 0.001, 30, 2],
  [5.5, -5.5, 0.1, 1],
  [0.123, 0.456, 1, 2.625],
  [-37.25, 113.5, 1.25, 1.75],
  // no device pixels: the app state itself
  [3.3, -7.77, 0, 1],
  [3.3, -7.77, 1, 0],
  [3.3, -7.77, -1, 1],
  [3.3, -7.77, 1, -2],
];

const snapScroll = (up) =>
  SNAP_CASES.map(([scrollX, scrollY, zoom, scale]) => {
    const appState = { scrollX, scrollY, zoom: { value: zoom } };
    const out = up.snapScrollToDevicePixels(appState, scale);
    return { scrollX, scrollY, zoom, scale, result: { scrollX: out.scrollX, scrollY: out.scrollY }, identity: out === appState };
  });

// -- bootstrapCanvas ------------------------------------------------------------

/** [name, layer, width, height, scale, appState] */
const BOOTSTRAP_CASES = [
  ["static-white", "static", 400, 300, 1, { theme: "light", viewBackgroundColor: "#ffffff" }],
  ["static-white-dpr-2", "static", 400, 300, 2, { theme: "light", viewBackgroundColor: "#ffffff" }],
  ["static-fractional-dpr", "static", 801, 601, 1.25, { theme: "light", viewBackgroundColor: "#ffffff" }],
  ["static-fractional-size", "static", 333.3, 222.2, 1.5, { theme: "light", viewBackgroundColor: "#ffffff" }],
  ["static-short-hex", "static", 400, 300, 1, { theme: "light", viewBackgroundColor: "#abc" }],
  ["static-upper-hex", "static", 400, 300, 1, { theme: "light", viewBackgroundColor: "#FFF9DB" }],
  ["static-hex-alpha", "static", 400, 300, 2, { theme: "light", viewBackgroundColor: "#ffffff80" }],
  ["static-rgba", "static", 400, 300, 1, { theme: "light", viewBackgroundColor: "rgba(255, 0, 0, 0.5)" }],
  ["static-named", "static", 400, 300, 1, { theme: "light", viewBackgroundColor: "white" }],
  ["static-transparent", "static", 400, 300, 3, { theme: "light", viewBackgroundColor: "transparent" }],
  ["static-null", "static", 400, 300, 1, { theme: "light", viewBackgroundColor: null }],
  ["static-empty", "static", 400, 300, 1, { theme: "light", viewBackgroundColor: "" }],
  ["static-invalid", "static", 400, 300, 1.5, { theme: "light", viewBackgroundColor: "0000" }],
  ["static-dark", "static", 400, 300, 2, { theme: "dark", viewBackgroundColor: "#ffffff" }],
  ["static-dark-filtered", "static", 400, 300, 1, { theme: "dark", viewBackgroundColor: "#ffc9c9" }],
  ["new-element", "new-element", 400, 300, 1, null],
  ["new-element-dpr-2", "new-element", 400, 300, 2, null],
  ["new-element-fractional", "new-element", 801, 601, 1.25, null],
  ["interactive", "interactive", 400, 300, 1, null],
  ["interactive-dpr-3", "interactive", 400, 300, 3, null],
  ["interactive-fractional", "interactive", 333.3, 222.2, 1.5, null],
];

/**
 * A canvas of the layer's CSS size at `scale`, sized as that layer's
 * component sizes it (see the header).
 */
const layerCanvas = (window, layer, width, height, scale) => {
  const canvas = window.document.createElement("canvas");
  if (layer === "static") {
    canvas.width = width * scale;
    canvas.height = height * scale;
  } else {
    // React writes a number attribute as String(value)
    canvas.setAttribute("width", String(width * scale));
    canvas.setAttribute("height", String(height * scale));
  }
  return canvas;
};

const bootstrap = (up, window) =>
  BOOTSTRAP_CASES.map(([name, layer, width, height, scale, appState]) => {
    const canvas = layerCanvas(window, layer, width, height, scale);
    const [normalizedWidth, normalizedHeight] = up.getNormalizedCanvasDimensions(canvas, scale);
    up.bootstrapCanvas({
      canvas,
      scale,
      normalizedWidth,
      normalizedHeight,
      ...(appState ? { theme: appState.theme, isExporting: false, viewBackgroundColor: appState.viewBackgroundColor } : {}),
    });
    return {
      name,
      layer,
      width,
      height,
      scale,
      appState,
      canvasWidth: canvas.width,
      canvasHeight: canvas.height,
      normalized: [normalizedWidth, normalizedHeight],
      events: canvas.getContext("2d").events,
    };
  });

// -- renderNewElementScene -------------------------------------------------------

const APP_STATE = {
  zoom: { value: 1 },
  scrollX: 0,
  scrollY: 0,
  theme: "light",
  viewBackgroundColor: "#ffffff",
  gridSize: 20,
  gridStep: 5,
  frameToHighlight: null,
  selectedElementIds: {},
  hoveredElementIds: {},
  frameRendering: { enabled: true, clip: true, name: true, outline: true },
  openDialog: null,
  selectedElementsAreBeingDragged: false,
  shouldCacheIgnoreZoom: false,
  croppingElementId: null,
  editingGroupId: null,
};

/** App.tsx:2700-2716: the new-element canvas's render config. */
const RENDER_CONFIG = {
  renderGrid: false,
  isExporting: false,
  theme: "light",
  canvasBackgroundColor: "#ffffff",
  embedsValidationStatus: {},
  elementsPendingErasure: [],
  pendingFlowchartNodes: null,
  elementRenderOverrides: null,
};

const FRAME = (up) => up.newFrameElement({ id: "frame", x: 50, y: 40, width: 240, height: 160, seed: 71, name: "Frame" });

const rect = (up, extra = {}) =>
  up.newElement({ type: "rectangle", id: "new", x: 10.37, y: 20.61, width: 120, height: 60, seed: 3, backgroundColor: "#a5d8ff", fillStyle: "hachure", ...extra });

const scene = (name, { width = 400, height = 300, scale = 1, elements = [], newElement = null, appState = {}, renderConfig = {} } = {}) => ({
  name,
  width,
  height,
  scale,
  elements,
  newElement,
  appState: { ...APP_STATE, ...appState },
  renderConfig: { ...RENDER_CONFIG, ...renderConfig },
});

const scenes = (up) => [
  scene("rectangle", { newElement: rect(up) }),
  scene("rectangle-scrolled", { newElement: rect(up), appState: { scrollX: 3.3, scrollY: -7.77 } }),
  scene("rectangle-zoom-1.5-dpr-2", { width: 800, height: 600, scale: 2, newElement: rect(up), appState: { zoom: { value: 1.5 }, scrollX: 3.3, scrollY: -7.77 } }),
  scene("rectangle-zoom-0.73-dpr-1.25", { width: 801, height: 601, scale: 1.25, newElement: rect(up), appState: { zoom: { value: 0.73 }, scrollX: 0.3, scrollY: 0.7 } }),
  scene("rectangle-fractional-size", { width: 333.3, height: 222.2, scale: 1.5, newElement: rect(up), appState: { scrollX: -12.34, scrollY: 5.678 } }),
  scene("ellipse-dark", {
    newElement: up.newElement({ type: "ellipse", id: "new", x: 40, y: 30, width: 90, height: 70, seed: 5, backgroundColor: "#ffc9c9", fillStyle: "solid" }),
    appState: { theme: "dark", zoom: { value: 1.25 }, scrollX: -20.2, scrollY: 15.15 },
    renderConfig: { theme: "dark", canvasBackgroundColor: "#121212" },
  }),
  scene("diamond-dpr-3", {
    scale: 3,
    newElement: up.newElement({ type: "diamond", id: "new", x: 60, y: 50, width: 80, height: 100, seed: 6, roundness: { type: 2 } }),
    appState: { scrollX: 0.1, scrollY: 0.2 },
  }),
  scene("arrow", {
    newElement: up.newArrowElement({ type: "arrow", id: "new", x: 30, y: 40, seed: 7, points: [[0, 0], [150, 60]], endArrowhead: "arrow", elbowed: false }),
    appState: { scrollX: 1.25, scrollY: -2.5 },
  }),
  scene("line", {
    newElement: up.newLinearElement({ type: "line", id: "new", x: 30, y: 40, seed: 8, points: [[0, 0], [80, 20], [120, 90]] }),
  }),
  scene("freedraw", {
    newElement: up.newFreeDrawElement({ type: "freedraw", id: "new", x: 100, y: 100, seed: 9, points: [[0, 0], [20, -10], [45, 5], [30, 40]], simulatePressure: true }),
    appState: { zoom: { value: 2 }, scrollX: -60.3, scrollY: -60.7 },
  }),
  scene("text", {
    newElement: up.newTextElement({ id: "new", x: 20, y: 30, text: "hello", fontSize: 20, seed: 10 }),
    appState: { scrollX: 0.5, scrollY: 0.5 },
  }),
  // what is not drawn: no new element, a selection box, and the elements
  // isInvisiblySmallElement rejects (sizeHelpers.ts:61-78)
  scene("none", { appState: { zoom: { value: 1.5 }, scrollX: 3.3 } }),
  scene("none-dpr-2", { scale: 2, appState: { zoom: { value: 0.5 } } }),
  scene("selection", { newElement: up.newElement({ type: "selection", id: "new", x: 10, y: 10, width: 100, height: 80, seed: 11 }), appState: { zoom: { value: 1.25 } } }),
  scene("arrow-below-drag-threshold", {
    newElement: up.newArrowElement({ type: "arrow", id: "new", x: 30, y: 40, seed: 12, points: [[0, 0], [0.05, -0.05]], elbowed: false }),
  }),
  scene("arrow-one-point", { newElement: up.newArrowElement({ type: "arrow", id: "new", x: 30, y: 40, seed: 13, points: [[0, 0]], elbowed: false }) }),
  scene("line-two-equal-points", {
    // only arrows are invisibly small with two points
    newElement: up.newLinearElement({ type: "line", id: "new", x: 30, y: 40, seed: 14, points: [[0, 0], [0.05, 0.05]] }),
  }),
  scene("freedraw-one-point", { newElement: up.newFreeDrawElement({ type: "freedraw", id: "new", x: 30, y: 40, seed: 15, points: [[0, 0]], simulatePressure: true }) }),
  scene("rectangle-zero-size", { newElement: rect(up, { width: 0, height: 0 }) }),
  scene("rectangle-zero-width", { newElement: rect(up, { width: 0, height: 30 }) }),
  // frame clipping (renderNewElementScene.ts:59-72)
  scene("frame-crossing", { elements: [FRAME(up)], newElement: rect(up, { x: 250, y: 150, frameId: "frame" }), appState: { scrollX: 7.3, scrollY: -3.6 } }),
  scene("frame-crossing-zoom-1.5-dpr-2", {
    width: 800,
    height: 600,
    scale: 2,
    elements: [FRAME(up)],
    newElement: rect(up, { x: 250, y: 150, frameId: "frame" }),
    appState: { zoom: { value: 1.5 }, scrollX: 7.3, scrollY: -3.6 },
  }),
  scene("frame-inside", { elements: [FRAME(up)], newElement: rect(up, { x: 80, y: 60, width: 50, height: 40, frameId: "frame" }) }),
  scene("frame-clip-off", {
    elements: [FRAME(up)],
    newElement: rect(up, { x: 250, y: 150, frameId: "frame" }),
    appState: { frameRendering: { enabled: true, clip: false, name: true, outline: true } },
  }),
  scene("frame-rendering-disabled", {
    elements: [FRAME(up)],
    newElement: rect(up, { x: 250, y: 150, frameId: "frame" }),
    appState: { frameRendering: { enabled: false, clip: true, name: true, outline: true } },
  }),
  scene("frame-missing", { newElement: rect(up, { x: 250, y: 150, frameId: "frame" }) }),
  scene("frame-highlighted-dragged", {
    elements: [FRAME(up)],
    newElement: rect(up, { x: 250, y: 150 }),
    appState: { frameToHighlight: FRAME(up), selectedElementIds: { new: true }, selectedElementsAreBeingDragged: true },
  }),
  scene("frame-highlighted-not-dragged", {
    elements: [FRAME(up)],
    newElement: rect(up, { x: 250, y: 150 }),
    appState: { frameToHighlight: FRAME(up) },
  }),
];

// -- running upstream -------------------------------------------------------------

/** JSON round trip: what a scene file holds and the Rust side reads. */
const plain = (value) => JSON.parse(JSON.stringify(value));

const runScene = (up, window, s) => {
  const elements = plain(s.elements);
  const newElement = s.newElement ? plain(s.newElement) : null;
  const appState = plain(s.appState);
  const rc = plain(s.renderConfig);
  window.devicePixelRatio = s.scale;
  globalThis.devicePixelRatio = s.scale;
  const canvas = layerCanvas(window, "new-element", s.width, s.height, s.scale);
  const map = up.arrayToMap(elements);
  up.renderNewElementScene({
    canvas,
    scale: s.scale,
    newElement,
    elementsMap: map,
    allElementsMap: map,
    rc: up.rough.canvas(canvas),
    renderConfig: {
      ...rc,
      imageCache: new Map(),
      embedsValidationStatus: new Map(),
      elementsPendingErasure: new Set(),
      elementRenderOverrides: undefined,
    },
    appState,
  });
  return {
    name: s.name,
    width: s.width,
    height: s.height,
    scale: s.scale,
    canvasWidth: canvas.width,
    canvasHeight: canvas.height,
    elements,
    newElement,
    appState,
    renderConfig: rc,
    images: {},
    events: canvas.getContext("2d").events,
  };
};

const build = async (upstream) => {
  const window = installDom(ORIGIN);
  const load = () =>
    loadUpstream(upstream, {
      entry: ENTRY,
      stubs: STUBS,
      patch: PATCH,
      define: {
        "import.meta.env.MODE": '"test"',
        "import.meta.env.PKG_NAME": "undefined",
        "import.meta.env.PKG_VERSION": "undefined",
      },
    });
  const fresh = async () => {
    const up = await load();
    up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
    up.reseed(RANDOM_SEED);
    return up;
  };
  globalThis.__vectorElements = true;
  const helpers = await fresh();
  const snap = snapScroll(helpers);
  const boot = bootstrap(helpers, window);
  const out = [];
  const names = scenes(await fresh()).map((s) => s.name);
  for (const name of names) {
    const up = await fresh();
    out.push(runScene(up, window, scenes(up).find((x) => x.name === name)));
  }
  delete globalThis.__vectorElements;
  return format({
    description:
      "Upstream snapScrollToDevicePixels, getNormalizedCanvasDimensions and bootstrapCanvas (packages/excalidraw/renderer/helpers.ts) and renderNewElementScene (renderer/renderNewElementScene.ts) at the pinned commit on a recording 2D context (tools/goldens/canvas-layers.mjs): snapped scrolls, each layer's bootstrap draws on a canvas sized as its component sizes it, and every draw of the new-element canvas per scene. Elements are drawn as vectors (renderElement's export path) instead of through the per-element bitmap cache; text measures 10 px per UTF-16 code unit.",
    upstream: upstream.commit,
    origin: ORIGIN,
    snapScroll: snap,
    bootstrap: boot,
    newElementScenes: out,
  });
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating canvas-layers goldens");
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
    process.stderr.write(`canvas-layers: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("canvas-layers goldens are out of date: run node tools/goldens/canvas-layers.mjs\n");
      process.exit(1);
    }
    process.stdout.write("canvas-layers goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();
