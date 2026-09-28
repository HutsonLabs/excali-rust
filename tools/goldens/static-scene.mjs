#!/usr/bin/env node
// Static scene goldens for excali-scene (ex-402): upstream's own
// renderStaticScene (packages/excalidraw/renderer/staticScene.ts) run from
// the pinned checkout under Node, drawing on a recording 2D context.
//
//   node tools/goldens/static-scene.mjs            write the fixture
//   node tools/goldens/static-scene.mjs --check    exit 1 if it is stale
//   node tools/goldens/static-scene.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-scene/tests/fixtures/static-scene.json: per scene,
// the inputs (canvas size and device pixel ratio, elements, the app state
// and render config the static canvas reads, the images in the image
// cache) and every draw the scene made, in order, as the canvas saw it:
//
// - `fill`, `stroke`, `clip`: the path (the calls since beginPath, in the
//   coordinates they were given in, or a Path2D's SVG data), the current
//   matrix and globalAlpha, the fill rule, and for strokes the line width,
//   caps, joins, miter limit and dash as the canvas holds them;
//   strokeRect is a stroke of its rectangle;
// - `fillRect`: its rectangle, the current matrix, globalAlpha and
//   fillStyle (kept apart from `fill`: the canvas draws it as a rectangle,
//   which it anti-aliases differently from a filled path);
// - `text`: fillText's arguments with the font, textAlign and the canvas's
//   `dir` attribute;
// - `image`: drawImage's arguments, the image's name (a file of the image
//   cache, or one of upstream's built-in icons with its src), filter and
//   imageSmoothingEnabled;
// - `unclip`: a restore() that dropped a clip;
// - `clear`: clearRect.
//
// Colours are recorded as the list of fillStyle/strokeStyle assignments in
// effect (a save() copies it), because the canvas ignores an assignment it
// cannot parse and keeps the previous style: the Rust test resolves the
// list with the port's CSS parser, which is checked against Chrome.
//
// A drawImage of a canvas (the link icon's cached canvas,
// staticScene.ts:201-273) is recorded as what that canvas holds: a clip to
// its bitmap and its own draws, mapped onto the destination rectangle.
//
// One change to upstream, marked in PATCH: drawElement draws every element
// as vectors, the path renderElement.ts:1066-1077 and :1111-1190 take when
// exporting, instead of blitting a per-element bitmap cache
// (generateElementWithCanvas, :687-728; drawElementFromCanvas, :762-934).
// The port's display list draws vectors in the editor too; the cache is a
// browser optimisation whose only visible effect is resampling and snapping
// each bitmap to whole device pixels (site/content/architecture/
// rendering-fidelity.md). Everything else runs as upstream wrote it.
//
// Deterministic: Math.random throws while generating, every element is
// built with a fixed id and seed after reseed(), and text measures 10 px
// per UTF-16 code unit (setCustomTextMetricsProvider).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
export const OUT_FILE = "static-scene.json";
/** window.location of the page: isElementLink compares hosts with it. */
export const ORIGIN = "https://excalidraw.com";
export const RANDOM_SEED = 1700000000000;

const ENTRY = `
export { renderStaticScene } from "./packages/excalidraw/renderer/staticScene";
export {
  newElement,
  newEmbeddableElement,
  newIframeElement,
  newFrameElement,
  newMagicFrameElement,
  newTextElement,
  newFreeDrawElement,
  newLinearElement,
  newArrowElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
export { arrayToMap, applyDarkModeFilter } from "@excalidraw/common";
export { default as rough } from "roughjs/bin/rough";
`;

const STUBS = [
  "packages/excalidraw/data/blob",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/subset/subset-main",
];

/**
 * The vector drawing path for every element (see the header): the two
 * `renderConfig.isExporting` tests in drawElement, and renderElement's
 * offset translation, which the cached path applies in the blit instead.
 */
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
  process.stderr.write("usage: static-scene.mjs [--check] [--out DIR]\n");
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

// -- the recording context ----------------------------------------------------

/** m × n for canvas matrices [a, b, c, d, e, f] (DOMMatrix multiply). */
const multiply = (m, n) => [
  m[0] * n[0] + m[2] * n[1],
  m[1] * n[0] + m[3] * n[1],
  m[0] * n[2] + m[2] * n[3],
  m[1] * n[2] + m[3] * n[3],
  m[0] * n[4] + m[2] * n[5] + m[4],
  m[1] * n[4] + m[3] * n[5] + m[5],
];

const finite = (...values) => values.every((v) => Number.isFinite(v));

/** Upstream's built-in images, told apart by their src. */
const BUILTIN_IMAGES = [
  ["image-placeholder", 'data-icon="image"'],
  ["image-error-placeholder", 'viewBox="0 0 668 668"'],
  ["external-link", "feather-external-link"],
  ["element-link", "icon-tabler-arrow-big-right-line"],
];

const imageName = (img) => {
  if (img.__file) return { file: img.__file };
  const src = decodeURIComponent(img.src ?? "");
  const found = BUILTIN_IMAGES.find(([, marker]) => src.includes(marker));
  if (!found) throw new Error(`drawImage of an unknown image ${String(img.src).slice(0, 80)}`);
  return { builtin: found[0], src: img.src };
};

const freshState = () => ({
  m: [1, 0, 0, 1, 0, 0],
  alpha: 1,
  fill: [],
  stroke: [],
  lineWidth: 1,
  lineCap: "butt",
  lineJoin: "miter",
  miterLimit: 10,
  dash: [],
  dashOffset: 0,
  font: "10px sans-serif",
  textAlign: "start",
  filter: "none",
  smoothing: true,
  clips: 0,
});

const copyState = (s) => ({ ...s, m: [...s.m], fill: [...s.fill], stroke: [...s.stroke], dash: [...s.dash] });

const contexts = new WeakMap();

/**
 * A CanvasRenderingContext2D that records draws. It keeps the state the
 * canvas specification defines (assignments the canvas ignores are
 * ignored here too, except colours, see the header) and the path.
 */
class RecordingContext {
  constructor(canvas) {
    this.canvas = canvas;
    this.events = [];
    this.s = freshState();
    this.stack = [];
    this.path = [];
    this.pathMatrix = null;
  }

  // state
  save() {
    this.stack.push(copyState(this.s));
  }
  restore() {
    if (!this.stack.length) return;
    const outer = this.stack.pop();
    for (let i = outer.clips; i < this.s.clips; i++) this.events.push({ op: "unclip" });
    this.s = outer;
  }
  set fillStyle(v) {
    this.s.fill.push(String(v));
  }
  get fillStyle() {
    return this.s.fill.at(-1) ?? "#000000";
  }
  set strokeStyle(v) {
    this.s.stroke.push(String(v));
  }
  get strokeStyle() {
    return this.s.stroke.at(-1) ?? "#000000";
  }
  set globalAlpha(v) {
    if (Number.isFinite(v) && v >= 0 && v <= 1) this.s.alpha = v;
  }
  get globalAlpha() {
    return this.s.alpha;
  }
  set lineWidth(v) {
    if (Number.isFinite(v) && v > 0) this.s.lineWidth = v;
  }
  get lineWidth() {
    return this.s.lineWidth;
  }
  set lineCap(v) {
    if (["butt", "round", "square"].includes(v)) this.s.lineCap = v;
  }
  get lineCap() {
    return this.s.lineCap;
  }
  set lineJoin(v) {
    if (["miter", "round", "bevel"].includes(v)) this.s.lineJoin = v;
  }
  get lineJoin() {
    return this.s.lineJoin;
  }
  set miterLimit(v) {
    if (Number.isFinite(v) && v > 0) this.s.miterLimit = v;
  }
  set lineDashOffset(v) {
    if (Number.isFinite(v)) this.s.dashOffset = v;
  }
  setLineDash(list) {
    if (list.some((v) => !Number.isFinite(v) || v < 0)) return;
    this.s.dash = list.length % 2 ? [...list, ...list] : [...list];
  }
  set font(v) {
    this.s.font = v;
  }
  get font() {
    return this.s.font;
  }
  set textAlign(v) {
    if (["start", "end", "left", "right", "center"].includes(v)) this.s.textAlign = v;
  }
  set textBaseline(v) {
    if (v !== "alphabetic") throw new Error(`textBaseline ${v} is not recorded`);
  }
  set filter(v) {
    this.s.filter = v;
  }
  set imageSmoothingEnabled(v) {
    this.s.smoothing = Boolean(v);
  }
  get imageSmoothingEnabled() {
    return this.s.smoothing;
  }

  // transforms
  transform(a, b, c, d, e, f) {
    if (finite(a, b, c, d, e, f)) this.s.m = multiply(this.s.m, [a, b, c, d, e, f]);
  }
  setTransform(a, b, c, d, e, f) {
    if (typeof a === "object") ({ a, b, c, d, e, f } = a);
    if (finite(a, b, c, d, e, f)) this.s.m = [a, b, c, d, e, f];
  }
  resetTransform() {
    this.s.m = [1, 0, 0, 1, 0, 0];
  }
  getTransform() {
    const [a, b, c, d, e, f] = this.s.m;
    return { a, b, c, d, e, f };
  }
  translate(x, y) {
    this.transform(1, 0, 0, 1, x, y);
  }
  scale(x, y) {
    this.transform(x, 0, 0, y, 0, 0);
  }
  rotate(angle) {
    if (Number.isFinite(angle)) this.transform(Math.cos(angle), Math.sin(angle), -Math.sin(angle), Math.cos(angle), 0, 0);
  }

  // paths
  beginPath() {
    this.path = [];
    this.pathMatrix = null;
  }
  push(command) {
    if (this.pathMatrix && this.pathMatrix.some((v, i) => v !== this.s.m[i])) {
      throw new Error("the matrix changed while a path was built: not recordable in local coordinates");
    }
    this.pathMatrix ??= [...this.s.m];
    this.path.push(command);
  }
  moveTo(x, y) {
    this.push(["moveTo", x, y]);
  }
  lineTo(x, y) {
    this.push(["lineTo", x, y]);
  }
  bezierCurveTo(...args) {
    this.push(["bezierCurveTo", ...args]);
  }
  quadraticCurveTo(...args) {
    this.push(["quadraticCurveTo", ...args]);
  }
  arc(x, y, r, start, end, anticlockwise = false) {
    this.push(["arc", x, y, r, start, end, Boolean(anticlockwise)]);
  }
  rect(...args) {
    this.push(["rect", ...args]);
  }
  roundRect(x, y, w, h, radius) {
    if (typeof radius !== "number") throw new Error("roundRect with radii lists is not recorded");
    this.push(["roundRect", x, y, w, h, radius]);
  }
  closePath() {
    this.push(["closePath"]);
  }

  // draws
  base(op) {
    return { op, m: [...this.s.m], alpha: this.s.alpha };
  }
  pathOf(arg) {
    return arg instanceof Path2DRecord ? [["svg", arg.d]] : [...this.path];
  }
  fill(a, b) {
    const rule = (a instanceof Path2DRecord ? b : a) ?? "nonzero";
    this.events.push({ ...this.base("fill"), fillStyle: [...this.s.fill], rule, path: this.pathOf(a) });
  }
  strokeEvent(path) {
    const s = this.s;
    return {
      ...this.base("stroke"),
      strokeStyle: [...s.stroke],
      lineWidth: s.lineWidth,
      lineCap: s.lineCap,
      lineJoin: s.lineJoin,
      miterLimit: s.miterLimit,
      dash: [...s.dash],
      dashOffset: s.dashOffset,
      path,
    };
  }
  stroke(p) {
    this.events.push(this.strokeEvent(this.pathOf(p)));
  }
  clip(a, b) {
    const rule = (a instanceof Path2DRecord ? b : a) ?? "nonzero";
    this.events.push({ op: "clip", m: [...this.s.m], rule, path: this.pathOf(a) });
    this.s.clips++;
  }
  fillRect(x, y, w, h) {
    if (!finite(x, y, w, h)) return;
    this.events.push({ ...this.base("fillRect"), fillStyle: [...this.s.fill], rect: [x, y, w, h] });
  }
  strokeRect(x, y, w, h) {
    if (!finite(x, y, w, h)) return;
    this.events.push(this.strokeEvent([["rect", x, y, w, h]]));
  }
  clearRect(x, y, w, h) {
    this.events.push({ op: "clear", m: [...this.s.m], rect: [x, y, w, h] });
  }
  fillText(text, x, y) {
    this.events.push({
      ...this.base("text"),
      fillStyle: [...this.s.fill],
      font: this.s.font,
      textAlign: this.s.textAlign,
      direction: this.canvas.getAttribute("dir") ?? "inherit",
      text,
      x,
      y,
    });
  }
  measureText(text) {
    return { width: text.length * 10 };
  }
  drawImage(img, ...args) {
    const inner = contexts.get(img);
    if (inner) {
      // a canvas: its content, clipped to its bitmap, mapped onto the
      // destination (only the 5-argument form is used on canvases)
      if (args.length !== 4) throw new Error("drawImage of a canvas with a source rectangle");
      const [dx, dy, dw, dh] = args;
      const map = multiply(this.s.m, [dw / img.width, 0, 0, dh / img.height, dx, dy]);
      this.events.push({ op: "clip", m: map, rule: "nonzero", path: [["rect", 0, 0, img.width, img.height]] });
      for (const event of inner.events) {
        if (event.op === "unclip") this.events.push(event);
        else this.events.push({ ...event, m: multiply(map, event.m), ...("alpha" in event ? { alpha: event.alpha * this.s.alpha } : {}) });
      }
      this.events.push({ op: "unclip" });
      return;
    }
    this.events.push({
      ...this.base("image"),
      image: imageName(img),
      args,
      filter: this.s.filter,
      smoothing: this.s.smoothing,
    });
  }
}

/** Path2D: upstream builds one from a freedraw's SVG path data. */
class Path2DRecord {
  constructor(d) {
    if (typeof d !== "string") throw new Error("Path2D without SVG data is not recorded");
    this.d = d;
  }
}

/** A document for upstream's code: jsdom 22.1.0 at ORIGIN. */
const installDom = () => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", { url: `${ORIGIN}/` });
  const { window } = dom;
  window.HTMLCanvasElement.prototype.getContext = function getContext(kind) {
    if (kind !== "2d") throw new Error(`getContext(${kind})`);
    let context = contexts.get(this);
    if (!context) {
      context = new RecordingContext(this);
      contexts.set(this, context);
    }
    return context;
  };
  const globals = {
    window,
    document: window.document,
    navigator: window.navigator,
    Node: window.Node,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    SVGElement: window.SVGElement,
    Path2D: Path2DRecord,
    devicePixelRatio: 1,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  return window;
};

// -- scenes ---------------------------------------------------------------------

/** The images of the image cache: fileId → mimeType and natural size. */
const IMAGES = {
  "file-png": { mimeType: "image/png", naturalWidth: 64, naturalHeight: 48 },
  "file-svg": { mimeType: "image/svg+xml", naturalWidth: 24, naturalHeight: 24 },
};

/** A bound label: the container lists it, it names the container. */
const bind = (container, label) => {
  container.boundElements = [...(container.boundElements ?? []), { type: "text", id: label.id }];
  label.containerId = container.id;
  return [container, label];
};

const label = (up, id, text, extra = {}) =>
  up.newTextElement({ id, x: 0, y: 0, text, textAlign: "center", verticalAlign: "middle", seed: 40, ...extra });

/** Every element kind the static scene draws (sticky notes are ex-703's). */
const allKinds = (up) => {
  const [box, boxLabel] = bind(
    up.newElement({ type: "rectangle", id: "box", x: 300, y: 20, width: 120, height: 70, seed: 3, backgroundColor: "#a5d8ff", fillStyle: "hachure" }),
    label(up, "box-label", "in a box", { x: 320, y: 45 }),
  );
  const [arrow, arrowLabel] = bind(
    up.newArrowElement({ type: "arrow", id: "labelled-arrow", x: 20, y: 300, seed: 9, points: [[0, 0], [100, -40], [220, 10]], endArrowhead: "arrow", elbowed: false }),
    label(up, "arrow-label", "label"),
  );
  return [
    // listed before its container: drawn with it, not here
    boxLabel,
    up.newElement({ type: "rectangle", id: "rect", x: 10, y: 20, width: 120, height: 60, seed: 1 }),
    up.newIframeElement({ type: "iframe", id: "iframe", x: 500, y: 20, width: 160, height: 90, seed: 20 }),
    up.newElement({ type: "rectangle", id: "rect-round", x: 150, y: 10, width: 90, height: 50, seed: 2, roundness: { type: 3 }, angle: 0.4, strokeStyle: "dashed" }),
    box,
    up.newElement({ type: "diamond", id: "diamond", x: 10, y: 120, width: 90, height: 60, seed: 4, backgroundColor: "#ffc9c9", fillStyle: "cross-hatch", roundness: { type: 2 } }),
    up.newEmbeddableElement({ type: "embeddable", id: "embed", x: 500, y: 150, width: 120, height: 80, seed: 21, angle: 0.2 }),
    up.newElement({ type: "ellipse", id: "ellipse", x: 130, y: 120, width: 100, height: 50, seed: 5, angle: 2.5, strokeStyle: "dotted", opacity: 60 }),
    up.newTextElement({ id: "text", x: 260, y: 130, text: "two\nlines", seed: 6, fontSize: 16 }),
    up.newTextElement({ id: "text-right", x: 260, y: 190, text: "right\r\naligned", seed: 7, textAlign: "right", fontFamily: 8 }),
    up.newTextElement({ id: "text-rtl", x: 380, y: 130, text: "123 שלום", seed: 8, fontFamily: 6, angle: 0.3 }),
    up.newLinearElement({ type: "line", id: "line-loop", x: 20, y: 220, seed: 10, backgroundColor: "#b2f2bb", points: [[0, 0], [60, 10], [30, 50], [0, 0]] }),
    up.newLinearElement({ type: "line", id: "line-curve", x: 120, y: 220, seed: 11, roundness: { type: 2 }, points: [[0, 0], [40, 40], [80, -10]] }),
    arrow,
    arrowLabel,
    up.newArrowElement({
      type: "arrow",
      id: "arrow-heads",
      x: 280,
      y: 260,
      seed: 12,
      angle: 0.5,
      points: [[0, 0], [120, 30]],
      startArrowhead: "circle_outline",
      endArrowhead: "triangle",
      elbowed: false,
    }),
    up.newArrowElement({
      type: "arrow",
      id: "elbow",
      x: 420,
      y: 260,
      seed: 13,
      points: [[0, 0], [0, 60], [100, 60]],
      endArrowhead: "bar",
      elbowed: true,
    }),
    up.newFreeDrawElement({
      type: "freedraw",
      id: "freedraw",
      x: 600,
      y: 280,
      seed: 14,
      angle: 0.7,
      backgroundColor: "#ffec99",
      points: [[0, 0], [20, -10], [45, 5], [30, 40], [5, 30], [0, 0]],
      simulatePressure: true,
    }),
    up.newImageElement({ type: "image", id: "image", x: 20, y: 380, width: 80, height: 60, seed: 15, fileId: "file-png", status: "saved" }),
    up.newImageElement({
      type: "image",
      id: "image-cropped-flipped",
      x: 120,
      y: 380,
      width: 60,
      height: 40,
      seed: 16,
      fileId: "file-png",
      status: "saved",
      scale: [-1, 1],
      crop: { x: 8, y: 4, width: 48, height: 32, naturalWidth: 64, naturalHeight: 48 },
      roundness: { type: 3 },
      angle: 0.25,
    }),
    up.newImageElement({ type: "image", id: "image-svg", x: 200, y: 380, width: 40, height: 40, seed: 17, fileId: "file-svg", status: "saved" }),
    up.newImageElement({ type: "image", id: "image-pending", x: 260, y: 380, width: 100, height: 50, seed: 18, fileId: "file-missing", status: "pending" }),
    up.newImageElement({ type: "image", id: "image-error", x: 380, y: 380, width: 30, height: 300, seed: 19, fileId: "file-missing", status: "error" }),
    up.newImageElement({ type: "image", id: "image-no-file", x: 430, y: 380, width: 40, height: 40, seed: 22, fileId: null }),
    up.newFrameElement({ id: "frame", x: 700, y: 20, width: 150, height: 100, seed: 23 }),
    up.newMagicFrameElement({ id: "magic", x: 700, y: 150, width: 150, height: 100, seed: 24, angle: 0.3 }),
  ];
};

/** Links: icons on unselected elements, none on selected ones. */
const links = (up) => [
  up.newElement({ type: "rectangle", id: "linked", x: 10, y: 10, width: 100, height: 60, seed: 30, link: "https://example.com" }),
  up.newElement({ type: "ellipse", id: "element-linked", x: 150, y: 10, width: 80, height: 80, seed: 31, angle: 0.6, link: `${ORIGIN}/?element=abc` }),
  up.newElement({ type: "diamond", id: "other-host", x: 260, y: 10, width: 80, height: 60, seed: 32, link: "https://example.com/?element=abc" }),
  up.newElement({ type: "rectangle", id: "selected-linked", x: 10, y: 120, width: 60, height: 40, seed: 33, link: "https://example.com" }),
  up.newEmbeddableElement({ type: "embeddable", id: "linked-embed", x: 120, y: 120, width: 90, height: 60, seed: 34, link: "https://www.youtube.com/watch?v=abc", opacity: 40 }),
  up.newTextElement({ id: "linked-text", x: 240, y: 130, text: "a link", seed: 35, link: "https://example.com", opacity: 70 }),
];

/** Opacity: frame × element, overrides, pending erasure. */
const opacity = (up) => [
  up.newFrameElement({ id: "half-frame", x: 0, y: 0, width: 300, height: 200, seed: 40, opacity: 50 }),
  up.newElement({ type: "rectangle", id: "in-half-frame", x: 20, y: 20, width: 80, height: 50, seed: 41, frameId: "half-frame", opacity: 50 }),
  up.newElement({ type: "rectangle", id: "erasing", x: 120, y: 20, width: 80, height: 50, seed: 42 }),
  up.newElement({ type: "rectangle", id: "overridden", x: 20, y: 100, width: 80, height: 50, seed: 43, opacity: 30 }),
  ...bind(
    up.newElement({ type: "ellipse", id: "moved", x: 120, y: 100, width: 80, height: 50, seed: 44 }),
    label(up, "moved-label", "moved", { x: 130, y: 112 }),
  ),
  up.newElement({ type: "diamond", id: "erasing-frame-child", x: 220, y: 100, width: 60, height: 60, seed: 45, frameId: "erasing-frame" }),
  up.newFrameElement({ id: "erasing-frame", x: 210, y: 90, width: 90, height: 90, seed: 46 }),
];

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

const RENDER_CONFIG = {
  renderGrid: true,
  isExporting: false,
  theme: "light",
  canvasBackgroundColor: "#ffffff",
  embedsValidationStatus: {},
  elementsPendingErasure: [],
  pendingFlowchartNodes: null,
  elementRenderOverrides: null,
};

const scene = (name, { width = 400, height = 300, scale = 1, elements = [], appState = {}, renderConfig = {} } = {}) => ({
  name,
  width,
  height,
  scale,
  elements,
  appState: { ...APP_STATE, ...appState },
  renderConfig: { ...RENDER_CONFIG, ...renderConfig },
});

const scenes = (up) => [
  // background and grid
  scene("grid"),
  scene("grid-scrolled", { appState: { scrollX: 37.25, scrollY: -113.5 } }),
  scene("grid-zoom-2", { appState: { zoom: { value: 2 }, scrollX: 13, scrollY: 7 } }),
  scene("grid-zoom-0.5", { appState: { zoom: { value: 0.5 } } }),
  scene("grid-zoom-0.45", { appState: { zoom: { value: 0.45 } } }),
  scene("grid-zoom-0.1", { width: 800, height: 600, appState: { zoom: { value: 0.1 } } }),
  scene("grid-zoom-1.37-dpr-2", { width: 800, height: 600, scale: 2, appState: { zoom: { value: 1.37 }, scrollX: 10.3, scrollY: -4.21 } }),
  scene("grid-dpr-1.5", { width: 600, height: 450, scale: 1.5, appState: { scrollX: 0.4, scrollY: 0.9 } }),
  scene("grid-step-1", { appState: { gridStep: 1, gridSize: 13 } }),
  scene("grid-size-50", { appState: { gridSize: 50, gridStep: 2, scrollX: -25, scrollY: 60 } }),
  scene("grid-dark", {
    appState: { theme: "dark", viewBackgroundColor: "#ffffff" },
    renderConfig: { theme: "dark" },
  }),
  scene("grid-off", { renderConfig: { renderGrid: false } }),
  scene("background-transparent", { appState: { viewBackgroundColor: "transparent" } }),
  scene("background-null", { appState: { viewBackgroundColor: null } }),
  scene("background-invalid", { appState: { viewBackgroundColor: "blue-ish" } }),
  scene("background-empty", { appState: { viewBackgroundColor: "" } }),
  scene("background-short-hex", { appState: { viewBackgroundColor: "#abc" } }),
  scene("background-rgba", { appState: { viewBackgroundColor: "rgba(255, 0, 0, 0.5)" } }),
  scene("background-dark-filtered", { appState: { theme: "dark", viewBackgroundColor: "#ffc9c9" }, renderConfig: { renderGrid: false } }),
  // exporting: no scroll snapping, no link icons, embeddable labels
  scene("exporting", {
    appState: { scrollX: 0.3, scrollY: 0.7 },
    renderConfig: { isExporting: true, renderGrid: false },
    elements: [
      up.newElement({ type: "rectangle", id: "exported", x: 10, y: 10, width: 100, height: 60, seed: 50, link: "https://example.com" }),
      up.newEmbeddableElement({ type: "embeddable", id: "exported-embed", x: 150, y: 10, width: 100, height: 100, seed: 51 }),
    ],
  }),
  // every element kind, in the editor and exporting, light and dark
  scene("elements", { width: 900, height: 700, elements: allKinds(up), appState: { scrollX: 12, scrollY: 8 }, renderConfig: { embedsValidationStatus: { embed: false } } }),
  scene("elements-validated-embed", { width: 900, height: 700, elements: allKinds(up), renderConfig: { renderGrid: false, embedsValidationStatus: { embed: true } } }),
  scene("elements-exporting", { width: 900, height: 700, elements: allKinds(up), renderConfig: { isExporting: true, renderGrid: false } }),
  scene("elements-dark", {
    width: 900,
    height: 700,
    elements: allKinds(up),
    appState: { theme: "dark", zoom: { value: 1.25 }, scrollX: -20, scrollY: 15 },
    renderConfig: { theme: "dark", canvasBackgroundColor: "#121212" },
  }),
  scene("elements-zoomed-dpr-2", {
    width: 1200,
    height: 900,
    scale: 2,
    elements: allKinds(up),
    appState: { zoom: { value: 0.75 }, scrollX: 5.3, scrollY: -2.2, frameRendering: { enabled: true, clip: true, name: true, outline: false } },
  }),
  scene("frames-disabled", {
    elements: allKinds(up).filter((e) => e.type === "frame" || e.type === "magicframe"),
    appState: { frameRendering: { enabled: false, clip: true, name: true, outline: true }, scrollX: -680 },
  }),
  // link icons
  scene("links", { elements: links(up), appState: { selectedElementIds: { "selected-linked": true } } }),
  scene("links-zoom-2", { width: 800, height: 600, elements: links(up), appState: { zoom: { value: 2 }, viewBackgroundColor: "#fff9db" } }),
  scene("links-zoom-0.5-dpr-2", { width: 800, height: 600, scale: 2, elements: links(up), appState: { zoom: { value: 0.5 } } }),
  scene("links-off", { elements: links(up), renderConfig: { renderLinks: false } }),
  // opacity and overrides
  scene("opacity", {
    elements: opacity(up),
    renderConfig: {
      elementsPendingErasure: ["erasing", "erasing-frame"],
      elementRenderOverrides: { overridden: { opacity: 80 }, moved: { offset: { x: 15, y: -10 } }, "moved-label": { offset: { x: 100, y: 100 } } },
    },
  }),
  scene("opacity-exporting", {
    elements: opacity(up),
    renderConfig: {
      isExporting: true,
      renderGrid: false,
      elementRenderOverrides: { overridden: { opacity: 250 }, moved: { offset: { x: 15, y: -10 } } },
    },
  }),
  scene("link-selector-dialog", {
    elements: links(up),
    appState: {
      openDialog: { name: "elementLinkSelector", sourceElementId: "linked" },
      selectedElementIds: { linked: true },
      hoveredElementIds: { "element-linked": true },
    },
  }),
  // pending flowchart nodes: after the embeddables, at the erasing opacity
  scene("pending-flowchart", {
    elements: [
      up.newEmbeddableElement({ type: "embeddable", id: "flow-embed", x: 200, y: 10, width: 80, height: 60, seed: 60 }),
      up.newElement({ type: "rectangle", id: "flow-source", x: 10, y: 10, width: 100, height: 60, seed: 61 }),
    ],
    renderConfig: {
      pendingFlowchartNodes: [
        up.newElement({ type: "rectangle", id: "flow-node", x: 10, y: 150, width: 100, height: 60, seed: 62 }),
        up.newElement({ type: "rectangle", id: "flow-node-2", x: 150, y: 150, width: 100, height: 60, seed: 63, opacity: 50 }),
      ],
    },
  }),
];

// -- running upstream -------------------------------------------------------------

/** JSON round trip: what a scene file holds and the Rust side reads. */
const plain = (value) => JSON.parse(JSON.stringify(value));

const fakeImage = (fileId) => ({ __file: fileId, naturalWidth: IMAGES[fileId].naturalWidth, naturalHeight: IMAGES[fileId].naturalHeight });

const run = (up, window, s) => {
  const elements = plain(s.elements);
  const pending = s.renderConfig.pendingFlowchartNodes ? plain(s.renderConfig.pendingFlowchartNodes) : null;
  const appState = plain(s.appState);
  const rc = plain(s.renderConfig);
  window.devicePixelRatio = s.scale;
  globalThis.devicePixelRatio = s.scale;
  const canvas = window.document.createElement("canvas");
  canvas.width = s.width;
  canvas.height = s.height;
  const map = up.arrayToMap(elements);
  const renderConfig = {
    ...rc,
    imageCache: new Map(Object.entries(IMAGES).map(([id, { mimeType }]) => [id, { image: fakeImage(id), mimeType }])),
    embedsValidationStatus: new Map(Object.entries(rc.embedsValidationStatus)),
    elementsPendingErasure: new Set(rc.elementsPendingErasure),
    pendingFlowchartNodes: pending,
    elementRenderOverrides: rc.elementRenderOverrides ? new Map(Object.entries(rc.elementRenderOverrides)) : undefined,
  };
  if (rc.renderLinks === undefined) delete renderConfig.renderLinks;
  up.renderStaticScene({
    canvas,
    rc: up.rough.canvas(canvas),
    elementsMap: map,
    allElementsMap: map,
    visibleElements: elements,
    scale: s.scale,
    appState,
    renderConfig,
  });
  return {
    name: s.name,
    width: s.width,
    height: s.height,
    scale: s.scale,
    elements,
    appState,
    renderConfig: { ...rc, pendingFlowchartNodes: pending },
    images: IMAGES,
    events: canvas.getContext("2d").events,
  };
};

const build = async (upstream) => {
  const window = installDom();
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
  globalThis.__vectorElements = true;
  const out = [];
  // Each scene gets a fresh module: the link icon canvases are cached in
  // module state (staticScene.ts:193-199) and would carry over.
  const names = scenes(await load()).map((s) => s.name);
  for (const name of names) {
    const up = await load();
    up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
    up.reseed(RANDOM_SEED);
    const s = scenes(up).find((x) => x.name === name);
    out.push(run(up, window, s));
  }
  delete globalThis.__vectorElements;
  return format({
    description:
      "Upstream renderStaticScene (packages/excalidraw/renderer/staticScene.ts) at the pinned commit on a recording 2D context (tools/goldens/static-scene.mjs): per scene the inputs and every draw in order with its path, matrix, alpha and styles. Elements are drawn as vectors (renderElement's export path) instead of through the per-element bitmap cache; text measures 10 px per UTF-16 code unit.",
    upstream: upstream.commit,
    origin: ORIGIN,
    scenes: out,
  });
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating static-scene goldens");
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
    process.stderr.write(`static-scene: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("static-scene goldens are out of date: run node tools/goldens/static-scene.mjs\n");
      process.exit(1);
    }
    process.stdout.write("static-scene goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
