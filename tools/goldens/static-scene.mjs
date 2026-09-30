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
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { installDom } from "./lib/recording-context.mjs";
import { NOW, stickyNotes, withPinnedNow } from "./lib/sticky-notes.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

// A sticky note's footer date is in local time: pin the time zone (and,
// while rendering, the clock) so every machine writes the same draws.
process.env.TZ = "UTC";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
export const OUT_FILE = "static-scene.json";
/** window.location of the page: isElementLink compares hosts with it. */
export const ORIGIN = "https://excalidraw.com";
export const RANDOM_SEED = 1700000000000;

const ENTRY = `
export { renderStaticScene } from "./packages/excalidraw/renderer/staticScene";
export {
  newElement,
  newStickyNoteElement,
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

/** Every element kind the static scene draws but sticky notes (stickyNoteScenes). */
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

/**
 * Frame clipping (ex-403, clipElementToFrame, staticScene.ts:358-393, and
 * shouldApplyFrameClip, frame.ts:911-972): a frame's children of every
 * kind, clipped when they cross its edge or contain it, not when they sit
 * inside it; grouped children outside the frame clipped by membership;
 * children of a missing frame, elements without a frame, and a rotated
 * magic frame whose clip stays unrotated.
 */
export const frameClip = (up) => {
  const f = "clip-frame";
  const [box, boxLabel] = bind(
    up.newElement({ type: "rectangle", id: "fc-box", x: 250, y: 150, width: 90, height: 50, seed: 90, frameId: f }),
    label(up, "fc-box-label", "boxed", { x: 265, y: 165, frameId: f }),
  );
  const [arrow, arrowLabel] = bind(
    up.newArrowElement({ type: "arrow", id: "fc-arrow", x: 200, y: 60, seed: 91, frameId: f, points: [[0, 0], [80, -50], [160, -30]], endArrowhead: "arrow", elbowed: false }),
    label(up, "fc-arrow-label", "arr", { frameId: f }),
  );
  return [
    // bigger than the frame, behind it: contains it, clipped
    up.newElement({ type: "rectangle", id: "fc-backdrop", x: 30, y: 30, width: 300, height: 220, seed: 70, frameId: f, backgroundColor: "#e9ecef", fillStyle: "solid" }),
    up.newFrameElement({ id: f, x: 50, y: 40, width: 240, height: 160, seed: 71, name: "Clip" }),
    up.newElement({ type: "rectangle", id: "fc-inside", x: 70, y: 60, width: 60, height: 40, seed: 72, frameId: f }),
    up.newElement({ type: "rectangle", id: "fc-crossing", x: 240, y: 70, width: 90, height: 40, seed: 73, frameId: f, roundness: { type: 3 } }),
    // inside the frame's box, rotated across its edge
    up.newElement({ type: "rectangle", id: "fc-rotated", x: 150, y: 150, width: 120, height: 30, seed: 74, frameId: f, angle: 0.9 }),
    up.newElement({ type: "ellipse", id: "fc-ellipse", x: 20, y: 90, width: 80, height: 50, seed: 75, frameId: f, angle: 0.3 }),
    up.newElement({ type: "diamond", id: "fc-diamond", x: 150, y: 10, width: 70, height: 60, seed: 76, frameId: f, roundness: { type: 2 } }),
    up.newElement({ type: "diamond", id: "fc-diamond-sharp", x: 140, y: 90, width: 40, height: 40, seed: 77, frameId: f }),
    up.newLinearElement({ type: "line", id: "fc-line", x: 20, y: 180, seed: 78, frameId: f, points: [[0, 0], [60, -20], [90, 40]] }),
    up.newLinearElement({ type: "line", id: "fc-line-round", x: 260, y: 20, seed: 79, frameId: f, roundness: { type: 2 }, points: [[0, 0], [40, 30], [70, -10]] }),
    // closed lines with a background test their inside: polygon shapes
    up.newLinearElement({ type: "line", id: "fc-loop", x: 270, y: 180, seed: 80, frameId: f, backgroundColor: "#b2f2bb", points: [[0, 0], [50, 10], [20, 45], [0, 0]] }),
    up.newLinearElement({ type: "line", id: "fc-loop-round", x: 25, y: 20, seed: 81, frameId: f, roundness: { type: 2 }, backgroundColor: "#ffec99", points: [[0, 0], [50, 5], [30, 40], [0, 0]] }),
    // a polygon line without a background: a polycurve joined across curves
    { ...up.newLinearElement({ type: "line", id: "fc-polygon", x: 110, y: 185, seed: 82, frameId: f, points: [[0, 0], [40, 0], [20, 30], [0, 0]] }), polygon: true },
    box,
    boxLabel,
    arrow,
    arrowLabel,
    up.newArrowElement({ type: "arrow", id: "fc-elbow", x: 230, y: 120, seed: 83, frameId: f, points: [[0, 0], [0, 40], [90, 40]], endArrowhead: "triangle", elbowed: true }),
    up.newFreeDrawElement({ type: "freedraw", id: "fc-freedraw", x: 20, y: 130, seed: 84, frameId: f, points: [[0, 0], [20, -10], [45, 5], [60, 20]], simulatePressure: true }),
    up.newFreeDrawElement({
      type: "freedraw",
      id: "fc-freedraw-loop",
      x: 275,
      y: 100,
      seed: 85,
      frameId: f,
      backgroundColor: "#ffc9c9",
      points: [[0, 0], [30, -5], [35, 25], [5, 30], [0, 0]],
      simulatePressure: true,
    }),
    up.newTextElement({ id: "fc-text", x: 240, y: 45, text: "crossing text", seed: 86, frameId: f }),
    up.newTextElement({ id: "fc-text-inside", x: 70, y: 110, text: "in", seed: 87, frameId: f }),
    // a text naming a container that is not there: a polygon shape
    { ...up.newTextElement({ id: "fc-text-orphan", x: 30, y: 150, text: "orphan", seed: 88, frameId: f }), containerId: "fc-missing" },
    up.newImageElement({ type: "image", id: "fc-image", x: 260, y: 175, width: 50, height: 40, seed: 89, frameId: f, fileId: "file-png", status: "saved" }),
    up.newIframeElement({ type: "iframe", id: "fc-iframe", x: 20, y: 180, width: 80, height: 50, seed: 92, frameId: f }),
    up.newEmbeddableElement({ type: "embeddable", id: "fc-embed", x: 140, y: 180, width: 60, height: 40, seed: 93, frameId: f }),
    // grouped: outside the frame, in a group with a member inside
    up.newElement({ type: "rectangle", id: "fc-grouped-out", x: 320, y: 60, width: 30, height: 30, seed: 94, frameId: f, groupIds: ["fc-group"] }),
    up.newElement({ type: "rectangle", id: "fc-grouped-in", x: 90, y: 120, width: 30, height: 20, seed: 95, frameId: f, groupIds: ["fc-group"] }),
    // grouped, outside, not the frame's child
    up.newElement({ type: "rectangle", id: "fc-grouped-free", x: 320, y: 110, width: 30, height: 30, seed: 96, groupIds: ["fc-free-group"] }),
    up.newElement({ type: "rectangle", id: "fc-grouped-free-2", x: 100, y: 70, width: 20, height: 20, seed: 97, groupIds: ["fc-free-group"] }),
    // no frame, crossing: never clipped
    up.newElement({ type: "rectangle", id: "fc-no-frame", x: 20, y: 60, width: 50, height: 20, seed: 98 }),
    // a frame that is not in the scene
    up.newElement({ type: "rectangle", id: "fc-missing-frame", x: 100, y: 30, width: 40, height: 20, seed: 99, frameId: "fc-nowhere" }),
    // a rotated magic frame: its clip is not rotated
    up.newMagicFrameElement({ id: "clip-magic", x: 380, y: 60, width: 120, height: 100, seed: 100, angle: 0.4 }),
    up.newElement({ type: "ellipse", id: "fm-crossing", x: 440, y: 120, width: 90, height: 70, seed: 101, frameId: "clip-magic", backgroundColor: "#a5d8ff", fillStyle: "hachure" }),
    up.newElement({ type: "rectangle", id: "fm-inside", x: 400, y: 80, width: 30, height: 30, seed: 102, frameId: "clip-magic" }),
  ];
};

/** Dragging: the highlighted frame is the target of selected elements. */
export const frameDrag = (up) => {
  const f = "drag-frame";
  const frame = up.newFrameElement({ id: f, x: 40, y: 40, width: 200, height: 150, seed: 110 });
  return [
    frame,
    // selected, dragged over the highlighted frame, not its child
    up.newElement({ type: "rectangle", id: "fd-dragged", x: 200, y: 60, width: 80, height: 40, seed: 111 }),
    // selected and dragged, inside: no clip
    up.newElement({ type: "rectangle", id: "fd-dragged-inside", x: 60, y: 60, width: 40, height: 30, seed: 112 }),
    // a dragged group: one member crosses the frame, the other is outside
    up.newElement({ type: "rectangle", id: "fd-group-crossing", x: 210, y: 130, width: 60, height: 30, seed: 113, groupIds: ["fd-group"] }),
    up.newElement({ type: "rectangle", id: "fd-group-out", x: 270, y: 200, width: 30, height: 30, seed: 114, groupIds: ["fd-group"] }),
    // a dragged group far from the frame: not in it
    up.newElement({ type: "rectangle", id: "fd-far-a", x: 300, y: 20, width: 30, height: 30, seed: 115, groupIds: ["fd-far"], frameId: f }),
    up.newElement({ type: "rectangle", id: "fd-far-b", x: 340, y: 20, width: 30, height: 30, seed: 116, groupIds: ["fd-far"] }),
    // a dragged group holding a frame: never in another frame
    up.newFrameElement({ id: "fd-grouped-frame", x: 300, y: 220, width: 60, height: 40, seed: 117, groupIds: ["fd-with-frame"] }),
    up.newElement({ type: "rectangle", id: "fd-with-frame-out", x: 250, y: 100, width: 10, height: 10, seed: 118, groupIds: ["fd-with-frame"] }),
    // not selected: its own frame
    up.newElement({ type: "rectangle", id: "fd-idle", x: 20, y: 170, width: 60, height: 40, seed: 119, frameId: f }),
    // selected with its frame: its own frame
    up.newFrameElement({ id: "fd-other", x: 60, y: 230, width: 100, height: 50, seed: 120 }),
    up.newElement({ type: "rectangle", id: "fd-with-its-frame", x: 140, y: 250, width: 60, height: 20, seed: 121, frameId: "fd-other" }),
  ];
};

export const DRAG_SELECTION = {
  "fd-dragged": true,
  "fd-dragged-inside": true,
  "fd-group-crossing": true,
  "fd-group-out": true,
  "fd-far-a": true,
  "fd-far-b": true,
  "fd-grouped-frame": true,
  "fd-with-frame-out": true,
  "fd-other": true,
  "fd-with-its-frame": true,
};

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
  // frame clipping (ex-403)
  scene("frame-clip", { width: 560, height: 320, elements: frameClip(up) }),
  scene("frame-clip-zoomed-dpr-2", {
    width: 900,
    height: 600,
    scale: 2,
    elements: frameClip(up),
    appState: { zoom: { value: 1.5 }, scrollX: 7.3, scrollY: -3.6 },
  }),
  scene("frame-clip-zoom-0.5", { width: 560, height: 320, elements: frameClip(up), appState: { zoom: { value: 0.5 }, scrollX: 40, scrollY: 30 } }),
  scene("frame-clip-exporting", { width: 560, height: 320, elements: frameClip(up), appState: { scrollX: 0.25 }, renderConfig: { isExporting: true, renderGrid: false } }),
  scene("frame-clip-dark", {
    width: 560,
    height: 320,
    elements: frameClip(up),
    appState: { theme: "dark" },
    renderConfig: { theme: "dark", canvasBackgroundColor: "#121212", renderGrid: false },
  }),
  scene("frame-clip-off", {
    width: 560,
    height: 320,
    elements: frameClip(up),
    appState: { frameRendering: { enabled: true, clip: false, name: true, outline: true } },
    renderConfig: { renderGrid: false },
  }),
  scene("frame-clip-disabled", {
    width: 560,
    height: 320,
    elements: frameClip(up),
    appState: { frameRendering: { enabled: false, clip: true, name: true, outline: true } },
    renderConfig: { renderGrid: false },
  }),
  // a translated child is clipped even inside its frame; a translated
  // frame clips all its children, at its new place
  scene("frame-clip-offsets", {
    width: 560,
    height: 320,
    elements: frameClip(up),
    renderConfig: {
      renderGrid: false,
      elementRenderOverrides: {
        "fc-inside": { offset: { x: 5, y: 3 } },
        "fc-text-inside": { offset: { x: 0, y: 0 } },
        "clip-magic": { offset: { x: -20, y: 10 } },
        "fc-box": { offset: { x: 10, y: 0 } },
      },
    },
  }),
  scene("frame-drag", {
    width: 400,
    height: 300,
    elements: frameDrag(up),
    appState: { selectedElementIds: DRAG_SELECTION, selectedElementsAreBeingDragged: true, frameToHighlight: frameDrag(up)[0] },
    renderConfig: { renderGrid: false },
  }),
  scene("frame-drag-editing-group", {
    width: 400,
    height: 300,
    elements: frameDrag(up),
    appState: { selectedElementIds: DRAG_SELECTION, selectedElementsAreBeingDragged: true, frameToHighlight: frameDrag(up)[0], editingGroupId: "fd-far" },
    renderConfig: { renderGrid: false },
  }),
  // selected but not dragged: each element's own frame
  scene("frame-selected", {
    width: 400,
    height: 300,
    elements: frameDrag(up),
    appState: { selectedElementIds: DRAG_SELECTION, frameToHighlight: frameDrag(up)[0] },
    renderConfig: { renderGrid: false },
  }),
  // a highlighted frame with nothing dragged: no child of it clipped by it
  scene("frame-highlight-only", {
    width: 400,
    height: 300,
    elements: frameDrag(up),
    appState: { frameToHighlight: frameDrag(up)[0] },
    renderConfig: { renderGrid: false },
  }),
  // sticky notes (ex-703): shadow, fill, clipped edge and date footer
  scene("sticky-notes", { width: 1000, height: 760, elements: stickyNotes(up, { x: 10, y: 10 }), renderConfig: { renderGrid: false } }),
  scene("sticky-notes-dark", {
    width: 1000,
    height: 760,
    elements: stickyNotes(up, { x: 10, y: 10 }),
    appState: { theme: "dark", zoom: { value: 0.8 }, scrollX: 12.5, scrollY: -7.25 },
    renderConfig: { theme: "dark", canvasBackgroundColor: "#121212", renderGrid: false },
  }),
  scene("sticky-notes-exporting", {
    width: 1000,
    height: 760,
    scale: 2,
    elements: stickyNotes(up, { x: 10, y: 10 }),
    renderConfig: { isExporting: true, renderGrid: false },
  }),
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
    // Date.now() while rendering, in UTC (sticky note footers)
    now: NOW,
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
  const text = await deterministic(() => withPinnedNow(() => build(upstream)));
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

// frame-clip.mjs imports the frame scenes: run only as a script
if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();
