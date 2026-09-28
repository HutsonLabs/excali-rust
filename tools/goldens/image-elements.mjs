#!/usr/bin/env node
// Image element goldens for excali-scene (ex-404): upstream's own
// renderElement (packages/element/src/renderElement.ts:963-1009) drawing
// image elements the way exportToCanvas does (renderConfig.isExporting,
// :1111-1190, and the "image" case of drawElementOnCanvas, :517-624, with
// drawImagePlaceholder, :361-385), run from the pinned checkout under Node
// with jsdom 22.1.0 as the document and a canvas context that records every
// call it gets.
//
//   node tools/goldens/image-elements.mjs            write the fixture
//   node tools/goldens/image-elements.mjs --check    exit 1 if it is stale
//   node tools/goldens/image-elements.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-scene/tests/fixtures/image-elements.json:
//
// - placeholders: the sources of upstream's two placeholder images
//   (IMAGE_PLACEHOLDER_IMG and IMAGE_ERROR_PLACEHOLDER_IMG, :342-359), the
//   SVGs its data URLs decode to;
// - files: the image files the cases draw, as BinaryFiles entries
//   (mimeType, dataURL) read from crates/excali-raster/tests/fixtures/images,
//   with the natural size a browser's <img> reports for each;
// - cases: per case the element (upstream's newImageElement), the theme,
//   the image cache state exportToCanvas would have (a loaded image, a
//   load still in progress, an errored file with no image, or no entry),
//   the scroll, and the calls upstream made on the context, in order:
//   ["save"], ["restore"], ["set", property, value], ["translate", x, y],
//   ["rotate", angle], ["scale", x, y], ["beginPath"], ["roundRect", x, y,
//   w, h, r], ["clip"], ["fillRect", x, y, w, h] and ["drawImage", image,
//   ...numbers], where image is {"file": fileId} or {"placeholder": "image"
//   | "error"}.
//
// The calls are canvas calls, so a browser can replay them as they are
// (the raster fixture image-elements.json, scripts/fixtures/raster_references.html).
// Deterministic: Math.random throws while generating and every input is
// fixed.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
export const FILE = "image-elements.json";
export const IMAGES_DIR = join(REPO_ROOT, "crates", "excali-raster", "tests", "fixtures", "images");

const ENTRY = `
export { renderElement } from "./packages/element/src/renderElement";
export { newImageElement } from "./packages/element/src/newElement";
export { arrayToMap, MIME_TYPES, THEME } from "@excalidraw/common";
`;

const usage = () => {
  process.stderr.write("usage: image-elements.mjs [--check] [--out DIR]\n");
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

// -- files ----------------------------------------------------------------------

/** The width and height a PNG's IHDR gives (what <img> reports). */
const pngSize = (bytes) => {
  if (bytes.readUInt32BE(12) !== 0x49484452) throw new Error("not a PNG IHDR");
  return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
};

/** The width and height attributes of an SVG's root (whole CSS pixels). */
const svgSize = (text) => {
  const root = text.match(/<svg\b[^>]*>/)[0];
  const attr = (name) => Number(root.match(new RegExp(`\\b${name}="([0-9.]+)"`))[1]);
  return [attr("width"), attr("height")];
};

const FILES = [
  { id: "png", name: "quad.png", mimeType: "image/png" },
  { id: "svg", name: "shape.svg", mimeType: "image/svg+xml" },
];

const readFiles = () =>
  Object.fromEntries(
    FILES.map(({ id, name, mimeType }) => {
      const bytes = readFileSync(join(IMAGES_DIR, name));
      const [naturalWidth, naturalHeight] =
        mimeType === "image/png" ? pngSize(bytes) : svgSize(bytes.toString("utf8"));
      return [
        id,
        {
          mimeType,
          id,
          dataURL: `data:${mimeType};base64,${bytes.toString("base64")}`,
          created: 1,
          naturalWidth,
          naturalHeight,
        },
      ];
    }),
  );

// -- cases ----------------------------------------------------------------------

// cache: "loaded" (the file's image in the cache), "pending" (the load's
// promise, as updateImageCache stores it before the image arrives and
// leaves it when the load fails), "none" (no entry).
const CASES = [
  // drawImagePlaceholder (:361-385): the grey box and the icon, min(min(w, h)
  // * 0.4, 100) and never above min(w, h).
  { id: "placeholder-pending", cache: "pending", el: { x: 10, y: 10, width: 120, height: 80, fileId: "png" } },
  { id: "placeholder-dark", theme: "dark", cache: "pending", el: { x: 140, y: 10, width: 120, height: 80, fileId: "png" } },
  { id: "placeholder-icon-cap", cache: "pending", el: { x: 270, y: 10, width: 300, height: 260, fileId: "png" } },
  { id: "placeholder-small", cache: "pending", el: { x: 10, y: 100, width: 12, height: 30, fileId: "png" } },
  { id: "placeholder-error", cache: "pending", el: { x: 30, y: 100, width: 100, height: 70, fileId: "png", status: "error" } },
  {
    id: "placeholder-error-dark",
    theme: "dark",
    cache: "pending",
    el: { x: 140, y: 100, width: 100, height: 70, fileId: "png", status: "error" },
  },
  { id: "placeholder-no-file", cache: "none", el: { x: 10, y: 180, width: 90, height: 60, fileId: "png", status: "saved" } },
  { id: "placeholder-uninitialized", cache: "loaded", el: { x: 110, y: 180, width: 90, height: 60, fileId: null } },
  {
    id: "placeholder-rotated-flipped-round",
    cache: "pending",
    el: { x: 10, y: 250, width: 110, height: 70, fileId: "png", angle: 0.4, scale: [-1, 1], roundness: { type: 3 } },
  },
  // Loaded images: the whole image at its natural size into the box.
  { id: "png", cache: "loaded", el: { x: 10, y: 340, width: 72, height: 48, fileId: "png", status: "saved" } },
  { id: "png-dark", theme: "dark", cache: "loaded", el: { x: 90, y: 340, width: 72, height: 48, fileId: "png" } },
  {
    id: "png-crop",
    cache: "loaded",
    el: {
      x: 170,
      y: 340,
      width: 60,
      height: 50,
      fileId: "png",
      crop: { x: 4, y: 2, width: 12, height: 10, naturalWidth: 24, naturalHeight: 16 },
    },
  },
  // scale is applied after rotating (:1181-1184).
  { id: "png-flip-x", cache: "loaded", el: { x: 240, y: 340, width: 72, height: 48, fileId: "png", scale: [-1, 1] } },
  { id: "png-flip-y", cache: "loaded", el: { x: 320, y: 340, width: 72, height: 48, fileId: "png", scale: [1, -1] } },
  { id: "png-flip-both", cache: "loaded", el: { x: 400, y: 340, width: 72, height: 48, fileId: "png", scale: [-1, -1] } },
  {
    id: "png-rotated-flipped",
    cache: "loaded",
    el: { x: 480, y: 330, width: 80, height: 50, fileId: "png", angle: 0.6, scale: [-1, 1] },
  },
  // Rounded images are clipped with roundRect(getCornerRadius(min(w, h))).
  { id: "png-round-adaptive", cache: "loaded", el: { x: 10, y: 400, width: 96, height: 64, fileId: "png", roundness: { type: 3 } } },
  {
    id: "png-round-adaptive-large",
    cache: "loaded",
    el: { x: 120, y: 400, width: 200, height: 150, fileId: "png", roundness: { type: 3 } },
  },
  {
    id: "png-round-adaptive-value",
    cache: "loaded",
    el: { x: 330, y: 400, width: 200, height: 150, fileId: "png", roundness: { type: 3, value: 12 } },
  },
  { id: "png-round-proportional", cache: "loaded", el: { x: 10, y: 480, width: 96, height: 64, fileId: "png", roundness: { type: 2 } } },
  { id: "png-round-legacy", cache: "loaded", el: { x: 10, y: 560, width: 96, height: 64, fileId: "png", roundness: { type: 1 } } },
  {
    id: "png-round-crop-rotated-flipped",
    cache: "loaded",
    el: {
      x: 130,
      y: 570,
      width: 90,
      height: 60,
      fileId: "png",
      angle: 5.5,
      scale: [-1, -1],
      roundness: { type: 3 },
      crop: { x: 6, y: 0, width: 18, height: 12, naturalWidth: 24, naturalHeight: 16 },
    },
  },
  { id: "png-opacity", cache: "loaded", el: { x: 240, y: 570, width: 72, height: 48, fileId: "png", opacity: 45 } },
  // SVG images in dark mode get DARK_THEME_FILTER (:549-609).
  { id: "svg", cache: "loaded", el: { x: 330, y: 570, width: 80, height: 60, fileId: "svg" } },
  { id: "svg-dark", theme: "dark", cache: "loaded", el: { x: 420, y: 570, width: 80, height: 60, fileId: "svg" } },
  {
    id: "svg-dark-crop-round",
    theme: "dark",
    cache: "loaded",
    el: {
      x: 510,
      y: 570,
      width: 60,
      height: 60,
      fileId: "svg",
      roundness: { type: 3 },
      crop: { x: 10, y: 5, width: 25, height: 25, naturalWidth: 40, naturalHeight: 30 },
    },
  },
  // The scroll the export translates by (appState.scrollX/Y).
  { id: "png-scrolled", cache: "loaded", scroll: [-3.5, 7.25], el: { x: 545, y: 460, width: 60, height: 40, fileId: "png" } },
];

// -- recording context --------------------------------------------------------------

/** A CanvasRenderingContext2D that records the calls renderElement makes. */
class RecordingContext {
  constructor(imageRef) {
    this.calls = [];
    this.imageRef = imageRef;
    for (const prop of ["globalAlpha", "fillStyle", "filter", "strokeStyle", "lineWidth", "font"]) {
      let value;
      Object.defineProperty(this, prop, {
        get: () => value,
        set: (v) => {
          value = v;
          this.calls.push(["set", prop, v]);
        },
      });
    }
  }
  record(name, args) {
    this.calls.push([name, ...args]);
  }
  save() {
    this.record("save", []);
  }
  restore() {
    this.record("restore", []);
  }
  translate(...a) {
    this.record("translate", a);
  }
  rotate(...a) {
    this.record("rotate", a);
  }
  scale(...a) {
    this.record("scale", a);
  }
  beginPath() {
    this.record("beginPath", []);
  }
  roundRect(...a) {
    this.record("roundRect", a);
  }
  rect(...a) {
    this.record("rect", a);
  }
  clip(...a) {
    this.record("clip", a);
  }
  fillRect(...a) {
    this.record("fillRect", a);
  }
  drawImage(image, ...a) {
    this.record("drawImage", [this.imageRef(image), ...a]);
  }
}

/** A document for upstream's code: jsdom 22.1.0. */
const installDom = () => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", {
    url: "https://excalidraw.com/",
  });
  const { window } = dom;
  const globals = {
    window,
    document: window.document,
    navigator: window.navigator,
    Node: window.Node,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    HTMLImageElement: window.HTMLImageElement,
    SVGElement: window.SVGElement,
    devicePixelRatio: 1,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
};

/** The SVG source of an `data:image/svg+xml,<encoded>` placeholder src. */
const svgOfDataUrl = (src) => {
  const prefix = "data:image/svg+xml,";
  if (!src.startsWith(prefix)) throw new Error(`placeholder src is not an SVG data URL: ${src.slice(0, 40)}`);
  return decodeURIComponent(src.slice(prefix.length));
};

const build = async (upstream) => {
  installDom();
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
  });
  const files = readFiles();

  // The placeholder images are the <img> elements renderElement.ts creates
  // at load; tell them apart by their SVG (the error one has a second path).
  const placeholders = {};
  const images = new Map();
  for (const [id, file] of Object.entries(files)) {
    // What updateImageCache stores once loadHTMLImageElement resolves.
    images.set(id, { tag: "file", id, naturalWidth: file.naturalWidth, naturalHeight: file.naturalHeight });
  }
  const imageRef = (image) => {
    if (image?.tag === "file") return { file: image.id };
    if (image instanceof window.HTMLImageElement) {
      const svg = svgOfDataUrl(image.src);
      const kind = svg.includes('data-icon="image"') ? "image" : "error";
      if (placeholders[kind] !== undefined && placeholders[kind] !== svg) {
        throw new Error(`two different ${kind} placeholders`);
      }
      placeholders[kind] = svg;
      return { placeholder: kind };
    }
    throw new Error(`drawImage of an unknown image ${image}`);
  };

  const cases = [];
  for (const c of CASES) {
    const theme = c.theme ?? "light";
    const scroll = c.scroll ?? [0, 0];
    const element = up.newImageElement({ type: "image", id: c.id, seed: 1, ...c.el });
    const imageCache = new Map();
    if (element.fileId !== null && c.cache !== "none") {
      const file = files[element.fileId];
      // A load in progress (or one that failed) leaves the promise.
      const image = c.cache === "loaded" ? images.get(element.fileId) : new Promise(() => {});
      imageCache.set(element.fileId, { image, mimeType: file.mimeType });
    }
    const context = new RecordingContext(imageRef);
    const elementsMap = up.arrayToMap([element]);
    up.renderElement(
      element,
      elementsMap,
      elementsMap,
      null,
      context,
      {
        canvasBackgroundColor: "#ffffff",
        imageCache,
        renderGrid: false,
        isExporting: true,
        embedsValidationStatus: new Map(),
        elementsPendingErasure: new Set(),
        pendingFlowchartNodes: null,
        theme,
      },
      {
        scrollX: scroll[0],
        scrollY: scroll[1],
        zoom: { value: 1 },
        theme,
        frameRendering: { enabled: true, name: true, outline: true, clip: true },
        selectedElementIds: {},
        hoveredElementIds: {},
        openDialog: null,
      },
    );
    // As JSON has it (customData is undefined).
    const json = JSON.parse(JSON.stringify(element));
    cases.push({ id: c.id, theme, cache: c.cache, scroll, element: json, calls: context.calls });
  }
  if (!placeholders.image || !placeholders.error) {
    throw new Error("the cases never drew both placeholders");
  }

  return format({
    description:
      "Upstream renderElement (packages/element/src/renderElement.ts:963-1009) exporting image elements (isExporting: the translate, rotate, scale, translate of :1111-1190; the image case of drawElementOnCanvas, :517-624; drawImagePlaceholder, :361-385) at the pinned commit under jsdom 22.1.0 (tools/goldens/image-elements.mjs): per case the element, theme, image cache state (loaded, pending, none), scroll and the canvas calls in order. placeholders are the SVG sources of upstream's placeholder images; files the BinaryFiles the cases draw, with the natural size their <img> reports.",
    upstream: upstream.commit,
    placeholders,
    files: Object.fromEntries(
      Object.entries(files).map(([id, f]) => [
        id,
        { mimeType: f.mimeType, id: f.id, dataURL: f.dataURL, created: f.created, naturalWidth: f.naturalWidth, naturalHeight: f.naturalHeight },
      ]),
    ),
    cases,
  });
};

/** Runs fn with Math.random disabled. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating image-elements goldens");
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
    process.stderr.write(`image-elements: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out, FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("image-elements goldens are out of date: run node tools/goldens/image-elements.mjs\n");
      process.exit(1);
    }
    process.stdout.write("image-elements goldens up to date\n");
    return;
  }
  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
