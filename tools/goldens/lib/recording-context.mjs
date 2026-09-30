// The recording 2D context the canvas goldens draw on (static-scene.mjs,
// png-export.mjs, interactive-scene.mjs): a CanvasRenderingContext2D that keeps the state the
// canvas specification defines and records every draw with its path,
// matrix, alpha and styles. See static-scene.mjs for what each event holds.

import { JSDOM } from "jsdom";

/** m × n for canvas matrices [a, b, c, d, e, f] (DOMMatrix multiply). */
export const multiply = (m, n) => [
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

export const contexts = new WeakMap();

/**
 * A CanvasRenderingContext2D that records draws. It keeps the state the
 * canvas specification defines (assignments the canvas ignores are
 * ignored here too, except colours, see the header) and the path.
 */
export class RecordingContext {
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
  getLineDash() {
    return [...this.s.dash];
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
  ellipse(x, y, rx, ry, rotation, start, end, anticlockwise = false) {
    this.push(["ellipse", x, y, rx, ry, rotation, start, end, Boolean(anticlockwise)]);
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
export class Path2DRecord {
  constructor(d) {
    if (typeof d !== "string") throw new Error("Path2D without SVG data is not recorded");
    this.d = d;
  }
}

/** A document for upstream's code: jsdom 22.1.0 at `origin`, with canvases
 * whose 2D context is a RecordingContext. */
export const installDom = (origin) => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", { url: `${origin}/` });
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

