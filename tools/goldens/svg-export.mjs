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
// Writes two files:
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
// - crates/excali-svg/tests/fixtures/svg-export.json: per scene, the
//   arguments exportToSvg was called with and the document shell it built,
//   as svgRoot.outerHTML serializes it (what exportCanvas saves after
//   SVG_DOCUMENT_PREAMBLE, packages/excalidraw/data/index.ts:123-146): the
//   root element with its attributes, the svg-source comment, <metadata>
//   with the embedded scene, <defs> with the frame clip paths and the
//   style-fonts block, and the background <rect>. The element nodes
//   renderSceneToSvg appends after them are left out (ex-407).
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
export const BOUNDS_FILE = "export-bounds.json";
export const SVG_FILE = "svg-export.json";
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
    startArrowhead: "dot",
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
  ];
};

// -- running upstream -------------------------------------------------------------

/** JSON round trip: what a scene file holds and the Rust side reads. */
const plain = (value) => JSON.parse(JSON.stringify(value));

/**
 * The shell: the root with its attributes and the nodes exportToSvg appends
 * before renderSceneToSvg (comment, metadata, defs, background rect).
 */
const shell = (svg, hasBackground) => {
  const count = 3 + (hasBackground ? 1 : 0);
  const nodes = [...svg.childNodes].slice(0, count);
  const kinds = nodes.map((n) => (n.nodeType === 8 ? "#comment" : n.nodeName));
  const expected = ["#comment", "metadata", "defs", ...(hasBackground ? ["rect"] : [])];
  if (JSON.stringify(kinds) !== JSON.stringify(expected)) {
    throw new Error(`unexpected document shell ${kinds.join(", ")}`);
  }
  const root = svg.cloneNode(false);
  for (const node of nodes) root.appendChild(node.cloneNode(true));
  return root.outerHTML;
};

const pathOf = (p) => ({
  d: p.getAttribute("d"),
  stroke: p.getAttribute("stroke"),
  strokeWidth: p.getAttribute("stroke-width"),
  fill: p.getAttribute("fill"),
});

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
    ? { ...(exportingFrame ? { exportingFrame } : {}), ...(scene.opts.skipInliningFonts ? { skipInliningFonts: true } : {}) }
    : undefined;

  const svg = await up.exportToSvg(elements, appState, files, opts);

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
      shell: shell(svg, Boolean(appState.exportBackground && appState.viewBackgroundColor)),
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
      coords: elements.map((e) => up.getElementAbsoluteCoords(e, elementsMap)),
      bounds: elements.map((e) => up.getElementBounds(e, elementsMap)),
      commonBounds: up.getCommonBounds(elements),
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
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
    fontUris: true,
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  const base = up.ExcalidrawFontFace.ASSETS_FALLBACK_URL;
  up.ExcalidrawFontFace.prototype.getContent = async function getContent(codePoints) {
    const href = this.urls[this.urls.length - 1].toString();
    if (!href.startsWith(base)) throw new Error(`font url ${href} is not under ${base}`);
    return `font:${decodeURIComponent(href.slice(base.length))}#${String.fromCodePoint(...codePoints)}`;
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
    [SVG_FILE]: format({
      description:
        "Upstream exportToSvg (packages/excalidraw/scene/export.ts:293-508) at the pinned commit under jsdom 22.1.0 (tools/goldens/svg-export.mjs): the arguments and the document shell (svgRoot.outerHTML without the element nodes). getExportSource() is `source`; a font face's content is font:<file>#<characters>; text measures 10 px per UTF-16 code unit.",
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
    const dir = args.out ?? (file === BOUNDS_FILE ? SCENE_DIR : SVG_DIR);
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
