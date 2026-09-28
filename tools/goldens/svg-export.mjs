#!/usr/bin/env node
// SVG export goldens for excali-scene and excali-svg (ex-406): upstream's own
// exportToSvg (packages/excalidraw/scene/export.ts:293-508) run from the
// pinned checkout under Node, with jsdom 22.1.0 (the DOM upstream's own
// vitest suite runs in, root package.json) as the document.
//
//   node tools/goldens/svg-export.mjs            write the fixtures
//   node tools/goldens/svg-export.mjs --check    exit 1 if they are stale
//   node tools/goldens/svg-export.mjs --out DIR  write (or --check) DIR
//
// Writes three files:
//
// - crates/excali-scene/tests/fixtures/export-bounds.json: per scene, the
//   elements, what upstream's getElementAbsoluteCoords and getElementBounds
//   (packages/element/src/bounds.ts:84-297, 997-1003) give for each of them,
//   getCommonBounds over them (:1005-1029), and what exportToSvg computes
//   before it builds the document: the frame rendering config
//   (getFrameRenderingConfig, export.ts:141-154), the frame name labels
//   prepareElementsForRender adds (addFrameLabelsAsTextElements and
//   truncateText, :64-139, 156-183), and getCanvasSize (:566-576) of the
//   root elements with the padding.
// - crates/excali-core/tests/fixtures/embed-links.json: what upstream's
//   getEmbedLink (packages/element/src/embeddable.ts:171-400) returns for
//   links of every kind, which renderElementToSvg reads for embeddables.
// - crates/excali-svg/tests/fixtures/svg-export.json: per scene, the
//   arguments exportToSvg was called with, the document shell it built
//   before rendering the elements (ex-406: the root element with its
//   attributes, the svg-source comment, <metadata> with the embedded scene,
//   <defs> with the frame clip paths and the style-fonts block, and the
//   background <rect>) and the whole document with the elements
//   renderSceneToSvg adds (ex-407, renderer/staticSvgScene.ts: rough.js
//   paths, freedraw outlines, one <text> per line, image <symbol>s and
//   <use>s, arrow label masks, frame clips and outlines, links), each as
//   svgRoot.outerHTML serializes it (what exportCanvas saves after
//   SVG_DOCUMENT_PREAMBLE, packages/excalidraw/data/index.ts:123-146).
//   Upstream runs in test mode, so every element's node carries its
//   data-id (isTestEnv, staticSvgScene.ts:140-145), and the text elements
//   export makes (frame name labels, embeddable placeholder labels) are
//   named by randomId's test sequence, id0, id1, ..., which restarts for
//   each scene. `stickyNoteNodes` lists the nodes upstream draws for sticky
//   notes (their data-id is the note's), which ex-703 ports.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"), Math.random throws while generating, and every input is fixed.
// Text is measured at 10 px per UTF-16 code unit, both by the metrics
// provider newTextElement reads (setCustomTextMetricsProvider,
// textMeasurements.ts:113-158) and by the canvas truncateText draws on
// (a 2d context whose measureText answers the same), since Node has no
// canvas. `FontFace` records its arguments (as in font-assets.mjs), and
// ExcalidrawFontFace#getContent, which fetches and subsets a font file,
// answers `font:<file>#<characters>` (the file the face names and the
// characters it was asked to keep), so the style block shows which faces
// upstream inlines, in which order, with which characters.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const SCENE_DIR = join(REPO_ROOT, "crates", "excali-scene", "tests", "fixtures");
export const SVG_DIR = join(REPO_ROOT, "crates", "excali-svg", "tests", "fixtures");
export const CORE_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
export const BOUNDS_FILE = "export-bounds.json";
export const SVG_FILE = "svg-export.json";
export const EMBED_LINKS_FILE = "embed-links.json";
/** getExportSource() answers window.location.origin (common/src/utils.ts). */
export const EXPORT_SOURCE = "https://excalidraw.com";
export const RANDOM_SEED = 1700000000000;

const ENTRY = `
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
export {
  getCommonBounds,
  getElementAbsoluteCoords,
  getElementBounds,
} from "./packages/element/src/bounds";
export { getRootElements } from "./packages/element/src/frame";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
export { arrayToMap } from "@excalidraw/common";
export {
  exportToSvg,
  getCanvasSize,
  getFrameRenderingConfig,
  prepareElementsForRender,
} from "./packages/excalidraw/scene/export";
export { ExcalidrawFontFace } from "./packages/excalidraw/fonts/ExcalidrawFontFace";
export { getEmbedLink } from "./packages/element/src/embeddable";
export * as fixtures from "./packages/excalidraw/tests/fixtures/elementFixture";
`;

// data/json.ts imports the browser file dialogs (./filesystem) and blob
// loading (./blob) for loadFromJSON/saveAsJSON; serializeAsJSON uses
// neither. subset-main starts the font subsetting worker; getContent, its
// only caller, is replaced below.
const STUBS = [
  "packages/excalidraw/data/blob",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/subset/subset-main",
];

const usage = () => {
  process.stderr.write("usage: svg-export.mjs [--check] [--out DIR]\n");
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

const SIZE = 100;

/** The elements of upstream's export test (tests/scene/export.test.ts:32-62). */
const exportTestElements = (up) => [
  { ...up.fixtures.diamondFixture, height: SIZE, width: SIZE, index: "a0" },
  { ...up.fixtures.ellipseFixture, height: SIZE, width: SIZE, index: "a1" },
  { ...up.fixtures.textFixture, height: SIZE, width: SIZE, index: "a2" },
  { ...up.fixtures.textFixture, fontFamily: 6, height: SIZE, width: SIZE, index: "a3" },
];

const CJK_TEXT =
  "中国你好！这是一个测试。中国你好！日本こんにちは！これはテストです。한국 안녕하세요! 이것은 테스트입니다.";

const DEFAULT_OPTIONS = { exportBackground: false, viewBackgroundColor: "#ffffff" };

/** A bound label: the container lists it, it names the container. */
const bind = (container, label) => {
  container.boundElements = [...(container.boundElements ?? []), { type: "text", id: label.id }];
  label.containerId = container.id;
  return [container, label];
};

const shapes = (up) => [
  up.newElement({ type: "rectangle", id: "rect", x: 10, y: 20, width: 120, height: 60, seed: 1 }),
  up.newElement({
    type: "rectangle",
    id: "rect-rotated",
    x: 200,
    y: -40,
    width: 80,
    height: 30,
    angle: 0.7,
    seed: 2,
    roundness: { type: 3 },
  }),
  up.newElement({ type: "diamond", id: "diamond-rotated", x: -50, y: 150, width: 90, height: 40, angle: 2.2, seed: 3 }),
  up.newElement({ type: "ellipse", id: "ellipse-rotated", x: 300, y: 200, width: 150, height: 50, angle: 5.9, seed: 4 }),
  up.newStickyNoteElement({ type: "stickynote", id: "sticky", x: -120, y: -90, width: 200, height: 200, seed: 5, angle: 0.3 }),
  up.newIframeElement({ type: "iframe", id: "iframe", x: 500, y: 0, width: 160, height: 90, seed: 6 }),
  up.newEmbeddableElement({ type: "embeddable", id: "embed", x: 520, y: 150, width: 100, height: 100, seed: 7, angle: 1 }),
  up.newTextElement({ id: "text-rotated", x: 40, y: 320, text: "rotated\ntext", angle: 1.2, seed: 8 }),
];

const linear = (up) => [
  up.newLinearElement({
    type: "line",
    id: "line",
    x: 0,
    y: 0,
    seed: 11,
    points: [
      [0, 0],
      [60, -40],
      [130, 10],
    ],
  }),
  up.newLinearElement({
    type: "line",
    id: "line-curved",
    x: 50,
    y: 100,
    seed: 12,
    roundness: { type: 2 },
    points: [
      [0, 0],
      [80, 90],
      [160, -30],
      [200, 40],
    ],
  }),
  up.newLinearElement({
    type: "line",
    id: "line-loop",
    x: -80,
    y: 60,
    seed: 13,
    backgroundColor: "#ffc9c9",
    points: [
      [0, 0],
      [40, 30],
      [-10, 50],
      [0, 0],
    ],
  }),
  up.newLinearElement({ type: "line", id: "line-point", x: 400, y: 400, seed: 14, points: [[0, 0]] }),
  up.newArrowElement({
    type: "arrow",
    id: "arrow",
    x: 250,
    y: 30,
    seed: 15,
    points: [
      [0, 0],
      [-90, 70],
    ],
    startArrowhead: "circle",
    endArrowhead: "arrow",
    elbowed: false,
  }),
  up.newArrowElement({
    type: "arrow",
    id: "arrow-curved-rotated",
    x: 300,
    y: 250,
    seed: 16,
    angle: 0.9,
    roundness: { type: 2 },
    points: [
      [0, 0],
      [70, -60],
      [140, 30],
    ],
    endArrowhead: "triangle",
    elbowed: false,
  }),
  up.newArrowElement({
    type: "arrow",
    id: "elbow",
    x: -200,
    y: -100,
    seed: 17,
    points: [
      [0, 0],
      [0, 80],
      [120, 80],
      [120, 160],
    ],
    endArrowhead: "arrow",
    elbowed: true,
  }),
  up.newFreeDrawElement({
    type: "freedraw",
    id: "freedraw",
    x: 600,
    y: -50,
    seed: 18,
    angle: 0.4,
    points: [
      [0, 0],
      [12, 5],
      [30, 22],
      [41, 60],
      [20, 71],
    ],
    simulatePressure: true,
  }),
];

const arrowLabels = (up) => {
  const label = (id, text, extra = {}) =>
    up.newTextElement({ id, x: 0, y: 0, text, textAlign: "center", verticalAlign: "middle", seed: 40, ...extra });
  const arrow = (id, points, extra = {}) =>
    up.newArrowElement({ type: "arrow", id, x: 0, y: 0, seed: 30, points, endArrowhead: "arrow", elbowed: false, ...extra });
  return [
    // odd point count: the label centres on the middle point
    ...bind(arrow("a-odd", [[0, 0], [100, -60], [220, 0]], { x: 10, y: 400 }), label("l-odd", "middle point")),
    // even point count, straight: the middle segment's centre
    ...bind(arrow("a-even", [[0, 0], [120, 0]], { x: 0, y: 0 }), label("l-even", "a label wider than its arrow")),
    // even point count, curved: the middle curve at half its length
    ...bind(
      arrow("a-curved", [[0, 0], [60, 80], [140, 40], [220, 120]], { x: 300, y: 0, roundness: { type: 2 } }),
      label("l-curved", "curved"),
    ),
    // labelPosition: a point along the whole path
    ...bind(
      arrow("a-position", [[0, 0], [90, 90], [180, 0]], { x: -300, y: 100 }),
      label("l-position", "at 0.2", { labelPosition: 0.2 }),
    ),
    ...bind(
      arrow("a-position-curved", [[0, 0], [90, 90], [180, 0]], { x: -300, y: 300, roundness: { type: 2 } }),
      label("l-position-curved", "at 0.85", { labelPosition: 0.85 }),
    ),
    // rotated arrows: the label box counter-rotated into the arrow's frame
    ...bind(arrow("a-rotated", [[0, 0], [160, 0]], { x: 100, y: 600, angle: 0.6 }), label("l-rotated", "rotated")),
    ...bind(arrow("a-rotated-2", [[0, 0], [160, 40]], { x: 400, y: 600, angle: 2.4 }), label("l-rotated-2", "rotated 2")),
    ...bind(arrow("a-rotated-3", [[0, 0], [160, 40]], { x: 700, y: 600, angle: 4.0 }), label("l-rotated-3", "rotated 3")),
    ...bind(arrow("a-rotated-4", [[0, 0], [160, 40]], { x: 1000, y: 600, angle: 5.5 }), label("l-rotated-4", "rotated 4")),
    // an elbow arrow: its unrounded logical path
    ...bind(
      up.newArrowElement({
        type: "arrow",
        id: "a-elbow",
        x: 600,
        y: 100,
        seed: 31,
        points: [
          [0, 0],
          [0, 90],
          [150, 90],
          [150, 180],
        ],
        endArrowhead: "arrow",
        elbowed: true,
      }),
      label("l-elbow", "elbow label"),
    ),
    ...bind(
      up.newArrowElement({
        type: "arrow",
        id: "a-elbow-position",
        x: 900,
        y: 100,
        seed: 32,
        points: [
          [0, 0],
          [0, 90],
          [150, 90],
        ],
        endArrowhead: "arrow",
        elbowed: true,
      }),
      label("l-elbow-position", "elbow at 0.9", { labelPosition: 0.9 }),
    ),
    // a label bound to a rectangle: positioned by its own x and y
    ...bind(
      up.newElement({ type: "rectangle", id: "box", x: 1200, y: 0, width: 100, height: 80, seed: 33 }),
      label("l-box", "in a box", { x: 1210, y: 30 }),
    ),
  ];
};

const frames = (up) => {
  const child = (id, frameId, x, y, extra = {}) =>
    up.newElement({ type: "rectangle", id, x, y, width: 60, height: 40, seed: 50, frameId, ...extra });
  return [
    up.newFrameElement({ id: "frame-named", x: 0, y: 0, width: 300, height: 200, seed: 51, name: "Named frame" }),
    child("in-named", "frame-named", 20, 20),
    // a child sticking out of its frame: not a root element, so not sized
    child("outside-named", "frame-named", 280, 180, { width: 200, height: 150 }),
    up.newFrameElement({ id: "frame-default", x: 400, y: -50, width: 150, height: 120, seed: 52 }),
    up.newMagicFrameElement({ id: "magic", x: 600, y: 0, width: 200, height: 100, seed: 53 }),
    up.newFrameElement({
      id: "frame-rotated",
      x: -250,
      y: 300,
      width: 180,
      height: 90,
      seed: 54,
      angle: 0.5,
      name: "Rotated",
    }),
    // wider than its frame: truncated with "..."
    up.newFrameElement({
      id: "frame-long-name",
      x: 0,
      y: 400,
      width: 100,
      height: 100,
      seed: 55,
      name: "A frame name much too long to fit",
    }),
    up.newFrameElement({ id: "frame-multiline", x: 200, y: 400, width: 200, height: 100, seed: 56, name: "two\nlines" }),
    up.newFrameElement({ id: "frame-empty-name", x: 500, y: 400, width: 80, height: 60, seed: 57, name: "" }),
    // a child of a frame that is not exported: a root element
    child("orphan", "not-exported", 50, 250),
  ];
};

const fontText = (up, id, fontFamily, text, y) =>
  up.newTextElement({ id, x: 0, y, text, fontFamily, seed: 60 });

const fonts = (up) => [
  fontText(up, "t-code", 8, "code & <tags>", 0),
  fontText(up, "t-hand", 5, "hand 手 ÿ", 40),
  fontText(up, "t-nunito", 6, "Nunito ǅ nbsp", 80),
  fontText(up, "t-local", 2, "local Helvetica", 120),
  fontText(up, "t-lilita", 7, "Lilita", 160),
  fontText(up, "t-hand-2", 5, "more hand", 200),
];

const FILES = {
  "file-kept": {
    mimeType: "image/png",
    id: "file-kept",
    dataURL: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    created: RANDOM_SEED,
    lastRetrieved: RANDOM_SEED,
  },
  "file-unused": {
    mimeType: "image/png",
    id: "file-unused",
    dataURL: "data:image/png;base64,AA==",
    created: RANDOM_SEED,
  },
};

const PNG_DATA_URL =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
const SVG_DATA_URL = `data:image/svg+xml;base64,${Buffer.from(
  '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#e03131"/></svg>',
).toString("base64")}`;

const IMAGE_FILES = {
  png: { mimeType: "image/png", id: "png", dataURL: PNG_DATA_URL, created: RANDOM_SEED },
  "png-2": { mimeType: "image/png", id: "png-2", dataURL: PNG_DATA_URL, created: RANDOM_SEED },
  "png-3": { mimeType: "image/png", id: "png-3", dataURL: PNG_DATA_URL, created: RANDOM_SEED },
  svg: { mimeType: "image/svg+xml", id: "svg", dataURL: SVG_DATA_URL, created: RANDOM_SEED },
  // a file id that is no plain CSS identifier (querySelector("#image-a.b")
  // reads an id and a class, and matches nothing)
  "a.b": { mimeType: "image/png", id: "a.b", dataURL: PNG_DATA_URL, created: RANDOM_SEED },
};

const CROP = { x: 10, y: 5, width: 50, height: 40, naturalWidth: 100, naturalHeight: 80 };

/** Image elements: symbols and uses, crops, flips, rounded clips, links. */
const images = (up) => {
  const image = (id, fileId, x, y, more = {}) =>
    up.newImageElement({ type: "image", id, x, y, width: 40, height: 30, seed: 100, fileId, status: "saved", ...more });
  return [
    image("img", "png", 0, 0, { width: 40.4, height: 30.6 }),
    // the same file again: another <use> of the same symbol
    image("img-again", "png", 60, 0, { width: 20.5, height: 20.5, angle: 0.5 }),
    image("img-flip-x", "png", 100, 0, { scale: [-1, 1] }),
    image("img-flip-y", "png", 150, 0, { scale: [1, -1], angle: 1 }),
    image("img-crop", "png", 0, 60, { width: 50, height: 40, crop: CROP }),
    // the same crop of the same file: the same symbol
    image("img-crop-2", "png", 60, 60, { width: 25, height: 20, crop: CROP }),
    image("img-crop-round", "png-2", 120, 60, {
      width: 50,
      height: 40,
      crop: CROP,
      roundness: { type: 3 },
      scale: [-1, -1],
      angle: 2,
    }),
    image("img-round", "png-2", 0, 120, { roundness: { type: 3 } }),
    image("img-round-small", "png-2", 60, 120, { width: 12, height: 9.5, roundness: { type: 2 } }),
    image("img-svg", "svg", 120, 120, { opacity: 40 }),
    image("img-odd-id", "a.b", 180, 120),
    image("img-odd-id-2", "a.b", 180, 170),
    // no such file, and no file at all: nothing is drawn
    image("img-missing", "missing", 0, 180),
    up.newImageElement({ type: "image", id: "img-pending", x: 60, y: 180, width: 30, height: 30, seed: 101 }),
    // a link: the anchor is the root, which has no <defs>, so the symbol
    // goes first in the anchor
    image("img-link", "png-3", 120, 180, { link: 'https://example.com/?a=1&b="2"' }),
    image("img-link-2", "svg", 180, 220, { link: "https://excalidraw.com" }),
  ];
};

/** Text elements: lines, alignment, direction, fonts, containers. */
const texts = (up) => {
  const text = (id, x, y, value, more = {}) => up.newTextElement({ id, x, y, text: value, seed: 110, ...more });
  const container = up.newElement({ type: "rectangle", id: "t-box", x: 400, y: 0, width: 160, height: 80, seed: 111 });
  const boxed = text("t-boxed", 410, 20, "in a\nbox", { textAlign: "center", verticalAlign: "middle" });
  // a label listed before its container: drawn with the container
  const early = text("t-early", 410, 120, "early", { textAlign: "center" });
  const late = up.newElement({ type: "ellipse", id: "t-late-box", x: 400, y: 100, width: 160, height: 80, seed: 112 });
  // a label whose container is not exported: drawn on its own
  const orphan = text("t-orphan", 400, 220, "orphan", { containerId: "not-exported" });
  // a deleted label: skipped, its container drawn
  const deleted = { ...text("t-deleted", 410, 330, "deleted"), isDeleted: true };
  const withDeleted = up.newElement({ type: "rectangle", id: "t-deleted-box", x: 400, y: 300, width: 100, height: 60, seed: 113 });
  bind(late, early);
  bind(withDeleted, deleted);
  return [
    text("t-multi", 0, 0, "first\nsecond line\n\nfourth"),
    { ...text("t-crlf", 0, 120, "a b c"), text: "a\r\nb\rc" },
    text("t-center", 0, 200, "centred\ntext", { textAlign: "center" }),
    text("t-right", 0, 260, "right\naligned", { textAlign: "right", angle: 2 }),
    text("t-rtl", 0, 320, "שלום עולם"),
    text("t-rtl-center", 0, 360, "مرحبا\nworld", { textAlign: "center" }),
    text("t-virgil", 200, 0, "Virgil", { fontFamily: 1, fontSize: 36 }),
    text("t-helvetica", 200, 60, "Helvetica", { fontFamily: 2, strokeColor: "#e03131" }),
    text("t-cascadia", 200, 100, "Cascadia", { fontFamily: 3, lineHeight: 1.5 }),
    text("t-comic", 200, 140, "Comic Shanns", { fontFamily: 8, fontSize: 16.5 }),
    text("t-liberation", 200, 180, "Liberation", { fontFamily: 9, opacity: 50 }),
    text("t-lilita", 200, 220, "Lilita", { fontFamily: 7 }),
    text("t-escape", 200, 260, "<a> & \"b\" 'c'"),
    text("t-spaces", 200, 300, "  padded  \n\ttab"),
    text("t-empty", 200, 360, ""),
    text("t-link", 200, 400, "linked", { link: "https://excalidraw.com" }),
    ...bind(container, boxed),
    early,
    late,
    orphan,
    withDeleted,
    deleted,
  ];
};

/** Freedraw: a loop with a fill, open strokes, pressures, constant width. */
const freedraws = (up) => {
  const freedraw = (id, x, y, points, more = {}) =>
    up.newFreeDrawElement({ type: "freedraw", id, x, y, seed: 120, points, simulatePressure: true, ...more });
  const loop = [
    [0, 0],
    [30, -10],
    [60, 5],
    [55, 40],
    [20, 45],
    [2, 20],
    [0, 0],
  ];
  return [
    freedraw("fd-loop", 0, 0, loop, { backgroundColor: "#a5d8ff", fillStyle: "solid" }),
    freedraw("fd-loop-hachure", 100, 0, loop, { backgroundColor: "#ffc9c9", fillStyle: "hachure", angle: 0.8 }),
    freedraw(
      "fd-open",
      0,
      100,
      [
        [0, 0],
        [10, 4],
        [25, 12],
        [40, 30],
        [44, 60],
      ],
      { strokeWidth: 4, opacity: 60 },
    ),
    freedraw(
      "fd-pressure",
      100,
      100,
      [
        [0, 0],
        [20, 10],
        [40, 0],
        [60, 20],
      ],
      { simulatePressure: false, pressures: [0.2, 0.9, 0.5, 0.1], strokeColor: "#1971c2" },
    ),
    freedraw(
      "fd-constant",
      200,
      100,
      [
        [0, 0],
        [15, 15],
        [30, 0],
        [45, 15],
      ],
      { strokeOptions: { variability: "constant", streamline: 0.5 } },
    ),
    freedraw("fd-point", 300, 100, [[0, 0]]),
    freedraw("fd-link", 300, 0, loop, { link: "https://excalidraw.com/#json=1", backgroundColor: "#b2f2bb" }),
  ];
};

/** Every Arrowhead (packages/element/src/types.ts:356-365). */
const ARROWHEADS = [
  "arrow",
  "bar",
  "circle",
  "circle_outline",
  "triangle",
  "triangle_outline",
  "diamond",
  "diamond_outline",
  "cardinality_one",
  "cardinality_many",
  "cardinality_one_or_many",
  "cardinality_exactly_one",
  "cardinality_zero_or_one",
  "cardinality_zero_or_many",
];

const TRIANGLE = [
  [0, 0],
  [60, 10],
  [30, 50],
  [0, 0],
];

/** Stroke and fill styles, opacity, roundness and every arrowhead. */
const styles = (up) => {
  const box = (id, type, x, y, more = {}) =>
    up.newElement({ type, id, x, y, width: 80, height: 60, seed: 130, backgroundColor: "#ffec99", ...more });
  const fills = ["hachure", "cross-hatch", "zigzag", "solid"].map((fillStyle, i) =>
    box(`fill-${fillStyle}`, "rectangle", i * 100, 0, { fillStyle }),
  );
  const line = (id, x, more) => up.newLinearElement({ type: "line", id, x, y: 200, seed: 131, points: TRIANGLE, ...more });
  const strokes = [
    box("dashed", "rectangle", 0, 100, { strokeStyle: "dashed", backgroundColor: "transparent" }),
    box("dotted", "ellipse", 100, 100, { strokeStyle: "dotted", strokeWidth: 4 }),
    box("dashed-diamond", "diamond", 200, 100, { strokeStyle: "dashed", roundness: { type: 2 }, fillStyle: "cross-hatch" }),
    box("round", "rectangle", 300, 100, { roundness: { type: 3 }, roughness: 0, fillStyle: "solid" }),
    box("round-value", "rectangle", 400, 100, { roundness: { type: 3, value: 20 }, roughness: 2, strokeWidth: 1 }),
    box("faint", "ellipse", 0, 200, { opacity: 30, fillStyle: "zigzag", angle: 4 }),
    box("round-diamond", "diamond", 100, 200, { roundness: { type: 2 }, opacity: 70 }),
    line("loop-filled", 200, { backgroundColor: "#d0bfff", fillStyle: "solid" }),
    line("loop-transparent", 300, {}),
    line("loop-curved", 400, { backgroundColor: "#96f2d7", roundness: { type: 2 } }),
    up.newLinearElement({
      type: "line",
      id: "line-dashed",
      x: 500,
      y: 200,
      seed: 134,
      strokeStyle: "dashed",
      points: [
        [0, 0],
        [80, 40],
      ],
    }),
  ];
  const heads = ARROWHEADS.map((head, i) =>
    up.newArrowElement({
      type: "arrow",
      id: `head-${head}`,
      x: (i % 4) * 120,
      y: 300 + Math.floor(i / 4) * 80,
      seed: 140 + i,
      points: [
        [0, 0],
        [90, 30],
      ],
      startArrowhead: i % 2 ? head : null,
      endArrowhead: head,
      strokeStyle: i % 3 === 1 ? "dotted" : "solid",
      elbowed: false,
    }),
  );
  return [...fills, ...strokes, ...heads];
};

/** Links on every kind of element; the anchor wraps what the element draws. */
const links = (up) => [
  up.newElement({ type: "rectangle", id: "l-rect", x: 0, y: 0, width: 60, height: 40, seed: 150, link: "excalidraw.com" }),
  up.newElement({ type: "diamond", id: "l-js", x: 100, y: 0, width: 60, height: 40, seed: 151, link: "javascript:alert(1)" }),
  up.newElement({
    type: "ellipse",
    id: "l-quotes",
    x: 200,
    y: 0,
    width: 60,
    height: 40,
    seed: 152,
    link: '  https://example.com/?q="x"&r=<y>  ',
  }),
  ...bind(
    up.newArrowElement({
      type: "arrow",
      id: "l-arrow",
      x: 0,
      y: 100,
      seed: 153,
      points: [
        [0, 0],
        [160, 20],
      ],
      endArrowhead: "arrow",
      elbowed: false,
      link: "https://excalidraw.com/#l",
    }),
    up.newTextElement({ id: "l-label", x: 0, y: 0, text: "label", textAlign: "center", verticalAlign: "middle", seed: 154 }),
  ),
  up.newLinearElement({
    type: "line",
    id: "l-line",
    x: 200,
    y: 100,
    seed: 155,
    points: [
      [0, 0],
      [50, 50],
    ],
    link: "/relative/path",
  }),
  up.newFrameElement({ id: "l-frame", x: 300, y: 0, width: 120, height: 90, seed: 156, link: "https://excalidraw.com/frame", name: "linked" }),
  up.newElement({ type: "rectangle", id: "l-empty", x: 0, y: 200, width: 60, height: 40, seed: 157, link: "" }),
];

/** Iframes and embeddables: the placeholder, its label, the link. */
const embeds = (up) => {
  const embeddable = (id, x, y, link, more = {}) =>
    up.newEmbeddableElement({ type: "embeddable", id, x, y, width: 160, height: 90, seed: 160, link, ...more });
  return [
    up.newIframeElement({ type: "iframe", id: "e-iframe", x: 0, y: 0, width: 160, height: 90, seed: 160 }),
    embeddable("e-empty", 200, 0, null, { width: 120, height: 80 }),
    embeddable("e-youtube", 0, 150, "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1m30s", {
      width: 320,
      height: 180,
      roundness: { type: 3 },
    }),
    embeddable("e-twitter", 400, 150, "https://twitter.com/excalidraw/status/1234567890", { width: 200, height: 200, angle: 0.3 }),
    embeddable("e-generic", 700, 150, "https://example.com/page?x=1", {
      width: 100,
      height: 100,
      opacity: 50,
      strokeColor: "transparent",
    }),
    embeddable("e-vimeo", 0, 400, "https://vimeo.com/12345"),
    embeddable("e-figma", 200, 400, "https://www.figma.com/file/abc"),
    embeddable("e-gist", 400, 400, "https://gist.github.com/user/0123abcd"),
    embeddable("e-drive", 600, 400, "https://drive.google.com/file/d/abc_DEF-123/view?resourcekey=key-1&t=90"),
    embeddable("e-valtown", 0, 550, "https://www.val.town/v/user.fn"),
    embeddable("e-msforms", 200, 550, "https://forms.microsoft.com/r/abc"),
    embeddable("e-reddit", 400, 550, "https://www.reddit.com/r/excalidraw/comments/abc123/some_title/"),
    embeddable("e-shorts", 600, 550, "youtube.com/shorts/xyz_12-3?start=42", { width: 90, height: 160 }),
    embeddable("e-playlist", 800, 550, "https://youtube.com/playlist?list=PL123"),
    embeddable("e-quote", 1000, 550, 'https://example.com/"quoted"', { roundness: { type: 3 } }),
    // a rectangle after the embeddables in the scene: embeddables draw last
    up.newElement({
      type: "rectangle",
      id: "e-front",
      x: 50,
      y: 50,
      width: 60,
      height: 40,
      seed: 174,
      backgroundColor: "#ffc9c9",
      fillStyle: "solid",
    }),
  ];
};

/** A frame with children of every kind: each is clipped to the frame. */
const frameChildren = (up) => {
  const frameId = "fc-frame";
  const child = (element) => ({ ...element, frameId });
  return [
    up.newFrameElement({ id: frameId, x: 0, y: 0, width: 400, height: 300, seed: 180, opacity: 50, name: "children" }),
    child(up.newElement({ type: "rectangle", id: "fc-rect", x: 20, y: 20, width: 80, height: 60, seed: 181, link: "https://excalidraw.com" })),
    child(up.newTextElement({ id: "fc-text", x: 120, y: 20, text: "clipped\ntext", seed: 182, opacity: 50 })),
    ...bind(
      child(
        up.newArrowElement({
          type: "arrow",
          id: "fc-arrow",
          x: 20,
          y: 150,
          seed: 183,
          points: [
            [0, 0],
            [200, 40],
          ],
          endArrowhead: "arrow",
          elbowed: false,
        }),
      ),
      up.newTextElement({ id: "fc-label", x: 0, y: 0, text: "label", textAlign: "center", verticalAlign: "middle", seed: 184 }),
    ),
    child(
      up.newLinearElement({
        type: "line",
        id: "fc-line",
        x: 250,
        y: 150,
        seed: 185,
        points: [
          [0, 0],
          [60, 60],
        ],
      }),
    ),
    child(
      up.newFreeDrawElement({
        type: "freedraw",
        id: "fc-freedraw",
        x: 300,
        y: 30,
        seed: 186,
        points: [
          [0, 0],
          [20, 30],
          [40, 10],
        ],
        simulatePressure: true,
      }),
    ),
    child(
      up.newImageElement({
        type: "image",
        id: "fc-image",
        x: 330,
        y: 200,
        width: 100,
        height: 80,
        seed: 187,
        fileId: "png",
        status: "saved",
        roundness: { type: 3 },
      }),
    ),
    child(up.newIframeElement({ type: "iframe", id: "fc-iframe", x: 200, y: 220, width: 100, height: 60, seed: 188 })),
  ];
};

/** What upstream refuses to draw: a selection, alone and with a link. */
const refused = (up) => [
  up.newElement({ type: "selection", id: "sel", x: 0, y: 0, width: 50, height: 50, seed: 190 }),
  up.newElement({ type: "selection", id: "sel-link", x: 100, y: 0, width: 50, height: 50, seed: 191, link: "https://excalidraw.com" }),
  up.newElement({ type: "rectangle", id: "after", x: 200, y: 0, width: 50, height: 50, seed: 192 }),
];

/** Links for getEmbedLink: every rule, its edges, and what falls through. */
const EMBED_LINKS = [
  "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1m30s",
  "https://youtube.com/watch?v=abc&t=1h2m3s",
  "https://youtube.com/watch?v=abc&t=&start=7",
  "https://youtube.com/watch?v=abc&t=2x",
  "youtube.com/shorts/xyz_12-3?start=42",
  "https://youtube.com/playlist?list=PL123",
  "https://youtube.com/embed/videoseries?list=PL1",
  "https://www.youtube.com/embed/abc",
  "https://www.youtube.com/live/abc?t=10",
  "https://youtu.be/abc?t=5",
  "http://youtu.be/abc",
  "https://youtube.com/watch?x=1",
  "https://youtube.com/",
  "https://vimeo.com/12345",
  "https://vimeo.com/video/42?h=abc",
  "https://www.player.vimeo.com/video/7?x=1",
  "https://vimeo.com/channels/staffpicks/1",
  "https://vimeo.com/abc def",
  "https://drive.google.com/file/d/abc_DEF-123/view?resourcekey=key-1&t=90",
  "drive.google.com/open?id=X1",
  "https://www.drive.google.com/uc?export=download&id=Y2&t=1m",
  "https://drive.google.com/file/d/bad.id/view",
  "https://drive.google.com/file/d/ok/view?resourcekey=bad key",
  "https://drive.google.com/other",
  "https://www.figma.com/file/abc?node-id=1:2",
  "https://figma.com/proto/x y",
  "https://www.val.town/v/user.fn",
  "https://val.town/v/user.fn",
  "https://www.val.town/embed/user.fn?x=1",
  "https://val.town/v/1user.fn",
  "https://forms.microsoft.com/r/abc",
  "https://forms.microsoft.com/r/abc?x=1",
  "forms.microsoft.com/r/abc?embed=true",
  "https://twitter.com/excalidraw/status/1234567890",
  "https://x.com/a/status/1?s=20",
  "see x.com/a/status/1 please",
  "https://www.reddit.com/r/excalidraw/comments/abc123/some_title/",
  "https://reddit.com/r/a/comments/b/c?utm=1#frag",
  "https://reddit.com/r/a/comments/b/c/d",
  "https://gist.github.com/user/0123abcd",
  "https://gist.github.com/user",
  "https://example.com/page?x=1",
  "https://stackblitz.com/edit/x",
  "about:blank",
  "https://excalidraw.com/relative",
];

/**
 * Scenes: exportToSvg's arguments. `opts.exportingFrame` names the frame by
 * id (the element itself is passed).
 */
const scenes = (up) => {
  const fixture = exportTestElements(up);
  const shapeElements = shapes(up);
  const frameElements = frames(up);
  return [
    // tests/scene/export.test.ts "with default arguments" and "with a CJK font"
    { name: "fixture", elements: fixture, appState: DEFAULT_OPTIONS, paths: true },
    {
      name: "fixture-cjk",
      elements: [
        ...fixture,
        { ...up.fixtures.textFixture, height: SIZE, width: SIZE, text: CJK_TEXT, originalText: CJK_TEXT, index: "a4" },
      ],
      appState: DEFAULT_OPTIONS,
    },
    // "with background color", "with dark mode", "with exportPadding", "with scale", "with exportEmbedScene"
    { name: "fixture-background", elements: fixture, appState: { ...DEFAULT_OPTIONS, exportBackground: true, viewBackgroundColor: "#abcdef" } },
    {
      name: "fixture-dark-background",
      elements: fixture,
      appState: { ...DEFAULT_OPTIONS, exportBackground: true, exportWithDarkMode: true },
    },
    { name: "fixture-padding-0", elements: fixture, appState: { ...DEFAULT_OPTIONS, exportPadding: 0 } },
    { name: "fixture-scale-2", elements: fixture, appState: { ...DEFAULT_OPTIONS, exportPadding: 0, exportScale: 2 } },
    { name: "fixture-embed", elements: fixture, appState: { ...DEFAULT_OPTIONS, exportEmbedScene: true } },
    // "with elements that have a link"
    { name: "link", elements: [up.fixtures.rectangleWithLinkFixture], appState: DEFAULT_OPTIONS },
    { name: "empty", elements: [], appState: DEFAULT_OPTIONS },
    { name: "empty-background", elements: [], appState: { exportBackground: true, viewBackgroundColor: "#ffc9c9" } },
    {
      name: "background-empty-string",
      elements: fixture,
      appState: { exportBackground: true, viewBackgroundColor: "" },
    },
    {
      name: "background-escaping",
      elements: fixture,
      appState: { exportBackground: true, viewBackgroundColor: 'a"b<c>&d e\'f' },
    },
    {
      name: "fractional",
      elements: [
        up.newElement({ type: "rectangle", id: "frac", x: 0.1, y: -0.2, width: 33.333, height: 0.7, seed: 70 }),
        up.newElement({ type: "ellipse", id: "frac-2", x: 1e-7, y: 12.5, width: 1 / 3, height: 2 / 3, seed: 71 }),
      ],
      appState: { ...DEFAULT_OPTIONS, exportPadding: 7.25, exportScale: 1.5 },
    },
    {
      name: "fractional-dark",
      elements: [up.newElement({ type: "rectangle", id: "frac", x: 3, y: 4, width: 10.1, height: 20.2, seed: 72 })],
      appState: { exportBackground: true, exportWithDarkMode: true, viewBackgroundColor: "#1e1e1e", exportScale: 3 },
    },
    { name: "shapes", elements: shapeElements, appState: { ...DEFAULT_OPTIONS, exportBackground: true } },
    { name: "linear", elements: linear(up), appState: DEFAULT_OPTIONS },
    { name: "arrow-labels", elements: arrowLabels(up), appState: DEFAULT_OPTIONS },
    { name: "frames", elements: frameElements, appState: { ...DEFAULT_OPTIONS, exportBackground: true } },
    {
      name: "frames-dark",
      elements: frameElements,
      appState: { ...DEFAULT_OPTIONS, exportWithDarkMode: true },
    },
    {
      name: "frames-no-name",
      elements: frameElements,
      appState: { ...DEFAULT_OPTIONS, frameRendering: { enabled: true, name: false, outline: true, clip: true } },
    },
    {
      name: "frames-disabled",
      elements: frameElements,
      appState: { ...DEFAULT_OPTIONS, frameRendering: { enabled: false, name: true, outline: true, clip: true } },
    },
    {
      name: "exporting-frame",
      elements: frameElements,
      appState: { ...DEFAULT_OPTIONS, exportBackground: true, exportPadding: 30 },
      opts: { exportingFrame: "frame-named" },
    },
    {
      name: "exporting-rotated-frame",
      elements: frameElements,
      appState: DEFAULT_OPTIONS,
      opts: { exportingFrame: "frame-rotated" },
    },
    {
      name: "frame-rotated-label",
      elements: [
        up.newFrameElement({
          id: "frame-wide-rotated",
          x: 0,
          y: 0,
          width: 300,
          height: 50,
          seed: 58,
          angle: 1.5,
          name: "a wide rotated label",
        }),
      ],
      appState: DEFAULT_OPTIONS,
    },
    {
      name: "frame-escaping",
      elements: [
        up.newFrameElement({ id: 'id "&<> ', x: 0, y: 0, width: 50, height: 50, seed: 80, name: "<&>" }),
      ],
      appState: DEFAULT_OPTIONS,
    },
    { name: "fonts", elements: fonts(up), appState: DEFAULT_OPTIONS },
    { name: "fonts-skipped", elements: fonts(up), appState: DEFAULT_OPTIONS, opts: { skipInliningFonts: true } },
    {
      name: "embed-files",
      elements: [
        up.newImageElement({ type: "image", id: "image", x: 0, y: 0, width: 40, height: 40, seed: 90, fileId: "file-kept", status: "saved" }),
        up.newElement({ type: "rectangle", id: "r", x: 50, y: 0, width: 40, height: 40, seed: 91 }),
      ],
      appState: {
        exportBackground: true,
        exportWithDarkMode: false,
        viewBackgroundColor: "#ffffff",
        exportPadding: 10,
        exportScale: 1,
        exportEmbedScene: true,
      },
      files: FILES,
    },
    {
      name: "embed-frames",
      elements: frameElements,
      appState: { ...DEFAULT_OPTIONS, exportEmbedScene: true, frameRendering: { enabled: true, name: true, outline: false, clip: false } },
    },
    // ex-407: the elements
    { name: "images", elements: images(up), appState: DEFAULT_OPTIONS, files: IMAGE_FILES },
    { name: "images-dark", elements: images(up), appState: { ...DEFAULT_OPTIONS, exportWithDarkMode: true }, files: IMAGE_FILES },
    { name: "images-no-reuse", elements: images(up), appState: DEFAULT_OPTIONS, files: IMAGE_FILES, opts: { reuseImages: false } },
    { name: "images-no-files", elements: images(up), appState: DEFAULT_OPTIONS },
    { name: "texts", elements: texts(up), appState: DEFAULT_OPTIONS },
    { name: "texts-dark", elements: texts(up), appState: { ...DEFAULT_OPTIONS, exportWithDarkMode: true } },
    { name: "freedraws", elements: freedraws(up), appState: DEFAULT_OPTIONS },
    { name: "freedraws-dark", elements: freedraws(up), appState: { ...DEFAULT_OPTIONS, exportWithDarkMode: true } },
    { name: "styles", elements: styles(up), appState: { ...DEFAULT_OPTIONS, viewBackgroundColor: "#f8f9fa" } },
    { name: "styles-dark", elements: styles(up), appState: { ...DEFAULT_OPTIONS, exportWithDarkMode: true } },
    { name: "links", elements: links(up), appState: DEFAULT_OPTIONS },
    { name: "embeds", elements: embeds(up), appState: DEFAULT_OPTIONS },
    { name: "embeds-dark", elements: embeds(up), appState: { ...DEFAULT_OPTIONS, exportWithDarkMode: true } },
    { name: "embeds-rendered", elements: embeds(up), appState: DEFAULT_OPTIONS, opts: { renderEmbeddables: true } },
    { name: "frame-children", elements: frameChildren(up), appState: DEFAULT_OPTIONS, files: IMAGE_FILES },
    {
      name: "frame-children-unclipped",
      elements: frameChildren(up),
      appState: { ...DEFAULT_OPTIONS, frameRendering: { enabled: true, name: true, outline: true, clip: false } },
      files: IMAGE_FILES,
    },
    { name: "frame-children-exported", elements: frameChildren(up), appState: DEFAULT_OPTIONS, files: IMAGE_FILES, opts: { exportingFrame: "fc-frame" } },
    { name: "arrow-labels-dark", elements: arrowLabels(up), appState: { ...DEFAULT_OPTIONS, exportWithDarkMode: true } },
    { name: "refused", elements: refused(up), appState: DEFAULT_OPTIONS },
  ];
};

// -- running upstream -------------------------------------------------------------

/** JSON round trip: what a scene file holds and the Rust side reads. */
const plain = (value) => JSON.parse(JSON.stringify(value));

/**
 * The shell: a copy of the root exportToSvg made just before it renders the
 * elements (the probe PATCH inserts), so it holds the root's attributes,
 * the comment, metadata, defs and background rect, and nothing the element
 * renderer adds to them (image symbols go into defs). Its nodes must also
 * be where the finished document has them.
 */
const shell = (svg, captured, hasBackground) => {
  if (!captured) throw new Error("exportToSvg did not reach renderSceneToSvg");
  const kinds = [...captured.childNodes].map((n) => (n.nodeType === 8 ? "#comment" : n.nodeName));
  const expected = ["#comment", "metadata", "defs", ...(hasBackground ? ["rect"] : [])];
  if (JSON.stringify(kinds) !== JSON.stringify(expected)) {
    throw new Error(`unexpected document shell ${kinds.join(", ")}`);
  }
  for (const [i, node] of [...captured.childNodes].entries()) {
    const final = svg.childNodes[i];
    if (node.nodeName !== final.nodeName || (node.nodeName !== "defs" && !node.isEqualNode(final))) {
      throw new Error(`document shell node ${i} changed while rendering`);
    }
  }
  return captured.outerHTML;
};

/**
 * Hands a copy of the root to globalThis.__svgExportShell right before
 * exportToSvg renders the elements (export.ts:475).
 */
const PATCH = {
  "packages/excalidraw/scene/export": (source) => {
    const anchor = "  const rsvg = rough.svg(svgRoot);";
    if (!source.includes(anchor)) throw new Error("export.ts changed: no rough.svg(svgRoot)");
    return source.replace(anchor, `  globalThis.__svgExportShell?.(svgRoot.cloneNode(true));\n${anchor}`);
  },
};

const pathOf = (p) => ({
  d: p.getAttribute("d"),
  stroke: p.getAttribute("stroke"),
  strokeWidth: p.getAttribute("stroke-width"),
  fill: p.getAttribute("fill"),
});

/**
 * Numbers for JSON: a non-finite one (a line with no curves has infinite
 * extremes) as its String(), "Infinity", "-Infinity" or "NaN".
 */
const numbers = (values) => values.map((v) => (Number.isFinite(v) ? v : String(v)));

const labelOf = (e) => ({
  text: e.text,
  x: e.x,
  y: e.y,
  width: e.width,
  height: e.height,
  fontSize: e.fontSize,
  fontFamily: e.fontFamily,
  lineHeight: e.lineHeight,
  strokeColor: e.strokeColor,
});

const run = async (up, scene) => {
  const elements = plain(scene.elements);
  const appState = plain(scene.appState);
  const files = scene.files ? plain(scene.files) : null;
  const exportingFrame = scene.opts?.exportingFrame
    ? elements.find((e) => e.id === scene.opts.exportingFrame)
    : null;
  const opts = scene.opts
    ? {
        ...(exportingFrame ? { exportingFrame } : {}),
        ...(scene.opts.skipInliningFonts ? { skipInliningFonts: true } : {}),
        ...("renderEmbeddables" in scene.opts ? { renderEmbeddables: scene.opts.renderEmbeddables } : {}),
        ...("reuseImages" in scene.opts ? { reuseImages: scene.opts.reuseImages } : {}),
      }
    : undefined;

  let captured = null;
  globalThis.__svgExportShell = (root) => {
    captured = root;
  };
  // randomId's test sequence starts again: the labels export makes are id0,
  // id1, ... in each scene (common/src/random.ts)
  up.reseed(RANDOM_SEED);
  // renderSceneToSvg logs what it refuses to draw (a selection) and goes on
  // (staticSvgScene.ts:906-908, 928-930); the document is what is recorded
  const consoleError = console.error;
  console.error = () => {};
  let svg;
  try {
    svg = await up.exportToSvg(elements, appState, files, opts);
  } finally {
    console.error = consoleError;
  }
  delete globalThis.__svgExportShell;
  const stickyNotes = new Set(elements.filter((e) => e.type === "stickynote").map((e) => e.id));
  const stickyNoteNodes = [...svg.querySelectorAll("[data-id]")]
    .filter((n) => stickyNotes.has(n.getAttribute("data-id")))
    .map((n) => n.outerHTML);

  // What exportToSvg computes before building the document, recomputed with
  // its own (exposed) helpers on the same inputs.
  const frameRendering = up.getFrameRenderingConfig(exportingFrame, appState.frameRendering ?? null);
  const elementsForRender = up.prepareElementsForRender({
    elements,
    exportingFrame,
    exportWithDarkMode: appState.exportWithDarkMode ?? false,
    frameRendering,
  });
  const padding = exportingFrame ? 0 : appState.exportPadding ?? 10;
  const sized = exportingFrame ? [exportingFrame] : up.getRootElements(elementsForRender);
  const canvas = up.getCanvasSize(sized, padding);
  const elementsMap = up.arrayToMap(elements);
  const inputs = new Set(elements);

  return {
    svg: {
      name: scene.name,
      elements,
      appState,
      files,
      opts: scene.opts ?? null,
      shell: shell(svg, captured, Boolean(appState.exportBackground && appState.viewBackgroundColor)),
      document: svg.outerHTML,
      ...(stickyNoteNodes.length ? { stickyNoteNodes } : {}),
      // The rough.js paths RoughSVG.draw writes with fixedDecimalPlaceDigits
      // MAX_DECIMALS_FOR_SVG_EXPORT (staticSvgScene.ts:60-74), in document
      // order: the two-decimal numbers of the upstream test's scene.
      ...(scene.paths ? { paths: [...svg.querySelectorAll("path")].map(pathOf) } : {}),
    },
    bounds: {
      name: scene.name,
      elements,
      appState,
      exportingFrame: exportingFrame?.id ?? null,
      coords: elements.map((e) => numbers(up.getElementAbsoluteCoords(e, elementsMap))),
      bounds: elements.map((e) => numbers(up.getElementBounds(e, elementsMap))),
      commonBounds: numbers(up.getCommonBounds(elements)),
      frameRendering,
      labels: elementsForRender.filter((e) => !inputs.has(e)).map(labelOf),
      rootElements: sized.filter((e) => inputs.has(e)).map((e) => e.id),
      canvas,
    },
  };
};

/** A document for upstream's code: jsdom 22.1.0 at the export source. */
const installDom = () => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", {
    url: `${EXPORT_SOURCE}/`,
  });
  const { window } = dom;
  // truncateText (export.ts:64-96) measures on a 2d canvas context.
  window.HTMLCanvasElement.prototype.getContext = function getContext() {
    return {
      font: "",
      measureText: (text) => ({ width: text.length * 10 }),
    };
  };
  const globals = {
    window,
    document: window.document,
    navigator: window.navigator,
    Node: window.Node,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    SVGElement: window.SVGElement,
    devicePixelRatio: 1,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
};

/** Records what upstream passes to `new FontFace` (see font-assets.mjs). */
class RecordingFontFace {
  constructor(family, source, descriptors = {}) {
    this.family = family;
    this.source = source;
    this.unicodeRange = descriptors.unicodeRange ?? "U+0-10FFFF";
    this.descriptors = descriptors;
  }
}

const build = async (upstream) => {
  installDom();
  globalThis.FontFace = RecordingFontFace;
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    expose: {
      "packages/excalidraw/scene/export": ["getCanvasSize", "getFrameRenderingConfig", "prepareElementsForRender"],
    },
    patch: PATCH,
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
    fontUris: true,
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  // getContent fetches and subsets; answer with the face's last url (its
  // file under ASSETS_FALLBACK_URL, which getContent returns when no url can
  // be fetched) and the characters it was asked to keep.
  const base = `${up.ExcalidrawFontFace.ASSETS_FALLBACK_URL}fonts/`;
  up.ExcalidrawFontFace.prototype.getContent = async function getContent(codePoints) {
    const href = this.urls[this.urls.length - 1].toString();
    if (!href.startsWith(base)) throw new Error(`font url ${href} is not under ${base}`);
    return `font:${href}#${String.fromCodePoint(...codePoints)}`;
  };
  up.reseed(RANDOM_SEED);
  const svgScenes = [];
  const boundsScenes = [];
  for (const scene of scenes(up)) {
    const { svg, bounds } = await run(up, scene);
    svgScenes.push(svg);
    boundsScenes.push(bounds);
  }
  return {
    [BOUNDS_FILE]: format({
      description:
        "Upstream getElementAbsoluteCoords, getElementBounds and getCommonBounds (packages/element/src/bounds.ts) per element, and exportToSvg's frame rendering config, frame name labels (addFrameLabelsAsTextElements, truncateText) and getCanvasSize over the root elements (packages/excalidraw/scene/export.ts) per scene, at the pinned commit (tools/goldens/svg-export.mjs). Text measures 10 px per UTF-16 code unit.",
      upstream: upstream.commit,
      scenes: boundsScenes,
    }),
    [EMBED_LINKS_FILE]: format({
      description:
        "Upstream getEmbedLink (packages/element/src/embeddable.ts:171-400) at the pinned commit (tools/goldens/svg-export.mjs), which exportToSvg asks for an embeddable's iframe source: per link, the link, type and intrinsic size it returns (null for none).",
      upstream: upstream.commit,
      links: EMBED_LINKS.map((link) => {
        const r = up.getEmbedLink(link);
        return { link, result: r ? { link: r.link ?? null, type: r.type, intrinsicSize: r.intrinsicSize } : null };
      }),
    }),
    [SVG_FILE]: format({
      description:
        "Upstream exportToSvg (packages/excalidraw/scene/export.ts:293-508) at the pinned commit under jsdom 22.1.0 in test mode (tools/goldens/svg-export.mjs): the arguments, the document shell (svgRoot.outerHTML before the elements are rendered) and the whole document. getExportSource() is `source`; a font face's content is font:<its last url, upstream's asset fallback>#<characters>; text measures 10 px per UTF-16 code unit.",
      upstream: upstream.commit,
      source: EXPORT_SOURCE,
      scenes: svgScenes,
    }),
  };
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating svg-export goldens");
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
    process.stderr.write(`svg-export: ${error.message}\n`);
    process.exit(1);
  }
  const out = await deterministic(() => build(upstream));
  const targets = Object.entries(out).map(([file, text]) => {
    const dir = args.out ?? { [BOUNDS_FILE]: SCENE_DIR, [SVG_FILE]: SVG_DIR, [EMBED_LINKS_FILE]: CORE_DIR }[file];
    return { path: join(dir, file), text };
  });
  if (args.check) {
    const stale = targets.filter(({ path, text }) => !existsSync(path) || readFileSync(path, "utf8") !== text);
    if (stale.length) {
      for (const { path } of stale) process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("svg-export goldens are out of date: run node tools/goldens/svg-export.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`svg-export goldens up to date: ${targets.length} files\n`);
    return;
  }
  for (const { path, text } of targets) {
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, text);
    process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
  }
};

await main();
