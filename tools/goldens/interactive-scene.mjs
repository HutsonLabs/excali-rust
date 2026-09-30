#!/usr/bin/env node
// Interactive scene goldens for excali-editor (ex-713): upstream's own
// renderInteractiveScene (packages/excalidraw/renderer/interactiveScene.ts,
// with renderSnaps.ts and renderer/helpers.ts) run from the pinned checkout
// under Node, drawing on the recording 2D context static-scene.mjs uses.
//
//   node tools/goldens/interactive-scene.mjs            write the fixture
//   node tools/goldens/interactive-scene.mjs --check    exit 1 if it is stale
//   node tools/goldens/interactive-scene.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/interactive-scene.json: per
// scene the inputs (canvas size and device pixel ratio, elements, the ids
// of the selected elements, the app state the interactive canvas reads,
// the pointer, the editor interface and the selection colour) and every
// draw the scene made, in order, recorded as static-scene.mjs describes
// (lib/recording-context.mjs). The interactive canvas has no background:
// bootstrapCanvas clears it (`clear`), and a display list starts clear.
//
// renderInteractiveScene takes `app` for the pointer only (the binding
// highlight's `lastPointerMoveCoords`, and `lastPointerMoveEvent` for the
// angle lock), so `app` here is a stub holding those two; the scenes never
// set `textToolHover`, the one other reader. The render config holds no
// collaborators (empty maps), no scrollbars (`renderScrollbars: false`, as
// App renders them only when its prop asks) and the default selection
// colour; `animationState` is none and `deltaTime` 0, and the
// COMPLEX_BINDINGS feature flag is off (its default), so the binding
// highlight is renderBindingHighlightForBindableElement_simple.
//
// The module pulls in the editor's jotai store and React-facing modules
// the renderer never calls: they are stubbed (binding-fixtures.mjs does the
// same). Nothing in upstream's rendering code is patched.
//
// Deterministic: Math.random throws while generating, every element is
// built with a fixed id and seed after reseed(), and text measures 10 px
// per UTF-16 code unit (setCustomTextMetricsProvider).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { installDom } from "./lib/recording-context.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const OUT_FILE = "interactive-scene.json";
const ORIGIN = "https://excalidraw.com";
const RANDOM_SEED = 1700000000000;
/** DEFAULT_SELECTION_COLOR (renderer/helpers.ts:6): no container to read. */
const SELECTION_COLOR = "#6965db";

const ENTRY = `
export { renderInteractiveScene } from "./packages/excalidraw/renderer/interactiveScene";
export {
  newElement,
  newEmbeddableElement,
  newFrameElement,
  newTextElement,
  newLinearElement,
  newArrowElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
export { arrayToMap } from "@excalidraw/common";
`;

const STUBS = [
  "react",
  "jotai",
  "jotai-scope",
  "packages/excalidraw/data/blob",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/subset/subset-main",
];
const SHIMS = {
  "packages/excalidraw/editor-jotai": "module.exports = { atom: (init) => ({ init }) };",
};

const usage = () => {
  process.stderr.write("usage: interactive-scene.mjs [--check] [--out DIR]\n");
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

/** What renderInteractiveScene reads of the app state, at its defaults. */
const APP_STATE = {
  zoom: { value: 1 },
  scrollX: 0,
  scrollY: 0,
  theme: "light",
  viewModeEnabled: false,
  zenModeEnabled: false,
  selectionElement: null,
  isCropping: false,
  croppingElementId: null,
  editingTextElement: null,
  selectedElementIds: {},
  selectedGroupIds: {},
  editingGroupId: null,
  selectedLinearElement: null,
  multiElement: null,
  newElement: null,
  isBindingEnabled: true,
  suggestedBinding: null,
  isMidpointSnappingEnabled: true,
  gridModeEnabled: false,
  activeTool: { type: "selection" },
  currentItemArrowType: "round",
  textToolHover: null,
  frameToHighlight: null,
  elementsToHighlight: null,
  activeLockedId: null,
  activeEmbeddable: null,
  isRotating: false,
  searchMatches: null,
  snapLines: [],
};

const DESKTOP = { formFactor: "desktop", userAgent: { isMobileDevice: false } };

/** `selectedLinearElement`: the fields the renderer reads of a LinearElementEditor. */
const linear = (elementId, extra = {}) => ({
  elementId,
  isEditing: false,
  isDragging: false,
  selectedPointsIndices: null,
  hoverPointIndex: -1,
  segmentMidPointHoveredCoords: null,
  hoveredFocusPointBinding: null,
  draggedFocusPointBinding: null,
  ...extra,
});

const ids = (...list) => Object.fromEntries(list.map((id) => [id, true]));

const rect = (up, id, x, y, width, height, extra = {}) =>
  up.newElement({ type: "rectangle", id, x, y, width, height, seed: 1, ...extra });

const scene = (name, { width = 400, height = 300, scale = 1, elements = [], appState = {}, pointer = null, angleLocked = false, editorInterface = DESKTOP } = {}) => ({
  name,
  width,
  height,
  scale,
  elements,
  appState: { ...APP_STATE, ...appState },
  pointer,
  angleLocked,
  editorInterface,
  selectionColor: SELECTION_COLOR,
});

const scenes = (up) => {
  const three = () => [
    rect(up, "a", 20, 30, 100, 60),
    up.newElement({ type: "ellipse", id: "b", x: 160, y: 40, width: 80, height: 90, seed: 2 }),
    up.newElement({ type: "diamond", id: "c", x: 60, y: 150, width: 120, height: 70, seed: 3, angle: 0.3 }),
  ];
  const grouped = () => [
    rect(up, "g-a", 20, 30, 80, 50, { groupIds: ["inner", "outer"] }),
    rect(up, "g-b", 120, 60, 60, 60, { groupIds: ["inner", "outer"] }),
    rect(up, "g-c", 60, 160, 90, 40, { groupIds: ["outer"] }),
    rect(up, "free", 250, 40, 70, 70),
  ];
  const line = (id, points, extra = {}) => up.newLinearElement({ type: "line", id, x: 60, y: 60, seed: 4, points, ...extra });
  const arrow = (id, points, extra = {}) =>
    up.newArrowElement({ type: "arrow", id, x: 40, y: 200, seed: 5, points, elbowed: false, endArrowhead: "arrow", ...extra });
  const polygon = () => ({ ...line("poly", [[0, 0], [120, 10], [60, 100], [0, 0]]), polygon: true });
  const elbow = () =>
    up.newArrowElement({ type: "arrow", id: "elbow", x: 40, y: 40, seed: 6, points: [[0, 0], [120, 0], [120, 90], [240, 90]], elbowed: true, endArrowhead: "arrow" });
  const text = (extra = {}) =>
    up.newTextElement({ id: "text", x: 80, y: 90, text: "fixed width\ntext", seed: 7, fontSize: 20, autoResize: false, ...extra });
  const target = (type = "rectangle", extra = {}) =>
    up.newElement({ type, id: "target", x: 180, y: 60, width: 140, height: 100, seed: 8, ...extra });
  const bindingArrow = () => arrow("drawing", [[0, 0], [120, -60]]);
  const snapLines = [
    { type: "points", points: [[20, 30], [120, 30], [260, 30]] },
    { type: "points", points: [[70, 20], [70, 180]] },
    { type: "gap", direction: "horizontal", points: [[120, 60], [160, 60]] },
    { type: "gap", direction: "vertical", points: [[200, 130], [200, 170]] },
    { type: "pointer", points: [[20, 30], [20, 220]], direction: "vertical" },
  ];

  return [
    // selection borders and transform handles
    scene("rectangle-selected", { elements: [rect(up, "a", 50, 60, 160, 90)], appState: { selectedElementIds: ids("a") } }),
    scene("rectangle-rotated", { elements: [rect(up, "a", 50, 60, 160, 90, { angle: 0.6 })], appState: { selectedElementIds: ids("a") } }),
    scene("small-rectangle", { elements: [rect(up, "a", 50, 60, 30, 20)], appState: { selectedElementIds: ids("a") } }),
    scene("negative-size", { elements: [rect(up, "a", 150, 160, -80, -60)], appState: { selectedElementIds: ids("a") } }),
    scene("multi-selection", { elements: three(), appState: { selectedElementIds: ids("a", "b", "c") } }),
    scene("multi-selection-rotating", { elements: three(), appState: { selectedElementIds: ids("a", "b", "c"), isRotating: true } }),
    scene("group-selected", {
      elements: grouped(),
      appState: { selectedElementIds: ids("g-a", "g-b", "g-c"), selectedGroupIds: { outer: true } },
    }),
    scene("group-and-element", {
      elements: grouped(),
      appState: { selectedElementIds: ids("g-a", "g-b", "g-c", "free"), selectedGroupIds: { outer: true, inner: false } },
    }),
    scene("editing-group", {
      elements: grouped(),
      appState: { selectedElementIds: ids("g-a", "g-b"), selectedGroupIds: { inner: true }, editingGroupId: "outer" },
    }),
    scene("locked-selected", { elements: [rect(up, "a", 50, 60, 160, 90, { locked: true, angle: 0.2 })], appState: { selectedElementIds: ids("a") } }),
    scene("locked-in-multi", {
      elements: [rect(up, "a", 30, 40, 100, 60, { locked: true }), rect(up, "b", 180, 90, 90, 90)],
      appState: { selectedElementIds: ids("a", "b") },
    }),
    scene("locked-group", {
      elements: [rect(up, "a", 30, 40, 100, 60, { locked: true, groupIds: ["lg"] }), rect(up, "b", 180, 90, 90, 90, { groupIds: ["lg"] })],
      appState: { selectedElementIds: ids("a", "b"), selectedGroupIds: { lg: true } },
    }),
    scene("frame-and-element", {
      elements: [up.newFrameElement({ id: "f", x: 20, y: 20, width: 200, height: 150, seed: 9 }), rect(up, "a", 240, 60, 80, 80)],
      appState: { selectedElementIds: ids("f", "a") },
    }),
    scene("frame-selected", {
      elements: [up.newFrameElement({ id: "f", x: 20, y: 20, width: 200, height: 150, seed: 9 })],
      appState: { selectedElementIds: ids("f") },
    }),
    scene("image-selected", {
      elements: [up.newImageElement({ type: "image", id: "img", x: 60, y: 50, width: 120, height: 90, seed: 10, status: "saved", fileId: "file" })],
      appState: { selectedElementIds: ids("img") },
    }),
    scene("image-cropping", {
      elements: [up.newImageElement({ type: "image", id: "img", x: 60, y: 50, width: 120, height: 90, seed: 10, status: "saved", fileId: "file", angle: 0.4 })],
      appState: { selectedElementIds: ids("img"), croppingElementId: "img" },
    }),
    scene("image-cropping-small-zoomed", {
      elements: [up.newImageElement({ type: "image", id: "img", x: 60, y: 50, width: 24, height: 16, seed: 10, status: "saved", fileId: "file" })],
      appState: { selectedElementIds: ids("img"), croppingElementId: "img", zoom: { value: 0.5 } },
    }),
    scene("image-cropping-active", {
      elements: [up.newImageElement({ type: "image", id: "img", x: 60, y: 50, width: 120, height: 90, seed: 10, status: "saved", fileId: "file" })],
      appState: { selectedElementIds: ids("img"), croppingElementId: "img", isCropping: true },
    }),
    scene("active-embeddable", {
      elements: [up.newEmbeddableElement({ type: "embeddable", id: "emb", x: 40, y: 40, width: 200, height: 120, seed: 11 })],
      appState: {
        selectedElementIds: ids("emb"),
        activeEmbeddable: { element: up.newEmbeddableElement({ type: "embeddable", id: "emb", x: 40, y: 40, width: 200, height: 120, seed: 11 }), state: "active" },
      },
    }),
    scene("view-mode", { elements: [rect(up, "a", 50, 60, 160, 90)], appState: { selectedElementIds: ids("a"), viewModeEnabled: true } }),
    scene("phone", {
      elements: [rect(up, "a", 50, 60, 160, 90)],
      appState: { selectedElementIds: ids("a") },
      editorInterface: { formFactor: "phone", userAgent: { isMobileDevice: true } },
    }),
    scene("multi-element", {
      elements: [rect(up, "a", 50, 60, 160, 90), line("l", [[0, 0], [80, 40]])],
      appState: { selectedElementIds: ids("a"), multiElement: line("l", [[0, 0], [80, 40]]) },
    }),
    // the selection box
    scene("selection-box", {
      elements: [rect(up, "a", 50, 60, 160, 90)],
      appState: { selectionElement: up.newElement({ type: "selection", id: "sel", x: 30.5, y: 40, width: 200, height: 130, seed: 12 }) },
    }),
    scene("selection-box-dark", {
      appState: { theme: "dark", selectionElement: up.newElement({ type: "selection", id: "sel", x: 30, y: 40, width: 200, height: 130, seed: 12 }) },
    }),
    scene("selection-box-cropping", {
      appState: { isCropping: true, selectionElement: up.newElement({ type: "selection", id: "sel", x: 30, y: 40, width: 200, height: 130, seed: 12 }) },
    }),
    // zoom, scroll and device pixel ratio
    scene("zoom-2-dpr-2", {
      width: 800,
      height: 600,
      scale: 2,
      elements: three(),
      appState: { zoom: { value: 2 }, scrollX: 13.3, scrollY: -7.26, selectedElementIds: ids("a") },
    }),
    scene("zoom-0.6-scrolled-multi", {
      elements: three(),
      appState: { zoom: { value: 0.6 }, scrollX: 40.2, scrollY: 25.7, selectedElementIds: ids("a", "c") },
    }),
    scene("dpr-1.5-group", {
      width: 600,
      height: 450,
      scale: 1.5,
      elements: grouped(),
      appState: { zoom: { value: 1.37 }, scrollX: -3.3, scrollY: 8.9, selectedElementIds: ids("g-a", "g-b", "g-c"), selectedGroupIds: { outer: true } },
    }),
    scene("dark", {
      elements: three(),
      appState: { theme: "dark", selectedElementIds: ids("a", "b"), snapLines },
    }),
    // linear elements
    scene("line-selected", {
      elements: [line("l", [[0, 0], [100, 40], [180, -20]])],
      appState: { selectedElementIds: ids("l"), selectedLinearElement: linear("l") },
    }),
    scene("line-two-points", {
      elements: [line("l", [[0, 0], [160, 80]])],
      appState: { selectedElementIds: ids("l"), selectedLinearElement: linear("l", { hoverPointIndex: 1 }) },
    }),
    scene("line-editing", {
      elements: [line("l", [[0, 0], [100, 40], [180, -20], [182, -21]], { angle: 0.2 })],
      appState: { selectedElementIds: ids("l"), selectedLinearElement: linear("l", { isEditing: true, selectedPointsIndices: [1], hoverPointIndex: 2 }) },
    }),
    scene("line-editing-hover-selected", {
      elements: [line("l", [[0, 0], [100, 40], [180, -20]], { roundness: { type: 2 } })],
      appState: {
        zoom: { value: 1.5 },
        selectedElementIds: ids("l"),
        selectedLinearElement: linear("l", { isEditing: true, selectedPointsIndices: [0, 1], hoverPointIndex: 1 }),
      },
    }),
    scene("polygon-editing", {
      elements: [polygon()],
      appState: { theme: "dark", selectedElementIds: ids("poly"), selectedLinearElement: linear("poly", { isEditing: true, selectedPointsIndices: [0] }) },
    }),
    scene("line-dragging", {
      elements: [line("l", [[0, 0], [100, 40], [180, -20]])],
      appState: { selectedElementIds: ids("l"), selectedLinearElement: linear("l", { isDragging: true, hoverPointIndex: 1 }) },
    }),
    scene("line-locked", {
      elements: [line("l", [[0, 0], [100, 40], [180, -20]], { locked: true })],
      appState: { selectedElementIds: ids("l"), selectedLinearElement: linear("l") },
    }),
    scene("elbow-arrow-selected", {
      elements: [elbow()],
      appState: { selectedElementIds: ids("elbow"), selectedLinearElement: linear("elbow", { hoverPointIndex: 0 }) },
    }),
    scene("elbow-arrow-midpoint-hovered", {
      elements: [{ ...elbow(), fixedSegments: [{ index: 2, start: [120, 0], end: [120, 90] }] }],
      appState: { selectedElementIds: ids("elbow"), selectedLinearElement: linear("elbow", { hoverPointIndex: 1, segmentMidPointHoveredCoords: [160, 40] }) },
    }),
    // arrows: the focus point and the binding highlight
    scene("focus-point", {
      elements: [
        target(),
        arrow("bound", [[0, 0], [150, -80]], {
          x: 30,
          y: 220,
          startBinding: null,
          endBinding: { elementId: "target", fixedPoint: [0.3, 0.4], mode: "orbit" },
        }),
      ],
      appState: { selectedElementIds: ids("bound"), selectedLinearElement: linear("bound", { hoveredFocusPointBinding: "end" }) },
    }),
    scene("suggested-binding", {
      elements: [target(), bindingArrow()],
      appState: { newElement: bindingArrow(), suggestedBinding: { element: target(), midPoint: null } },
      pointer: [165, 115],
    }),
    scene("suggested-binding-midpoint", {
      elements: [target("rectangle", { roundness: { type: 3 }, strokeWidth: 4, angle: 0.3 }), bindingArrow()],
      appState: {
        zoom: { value: 1.25 },
        scrollX: 5,
        newElement: bindingArrow(),
        suggestedBinding: { element: target("rectangle", { roundness: { type: 3 }, strokeWidth: 4, angle: 0.3 }), midPoint: [250, 50] },
      },
      pointer: [252, 40],
    }),
    scene("suggested-binding-elbow-in-frame", {
      elements: [
        up.newFrameElement({ id: "frame", x: 150, y: 30, width: 200, height: 160, seed: 13 }),
        target("diamond", { roundness: { type: 2 }, frameId: "frame" }),
        bindingArrow(),
      ],
      appState: {
        newElement: bindingArrow(),
        activeTool: { type: "arrow" },
        currentItemArrowType: "elbow",
        suggestedBinding: { element: target("diamond", { roundness: { type: 2 }, frameId: "frame" }), midPoint: [320, 110] },
      },
      pointer: [250, 110],
    }),
    scene("suggested-binding-ellipse-dark", {
      elements: [target("ellipse", { angle: 0.5 }), bindingArrow()],
      appState: { theme: "dark", newElement: bindingArrow(), suggestedBinding: { element: target("ellipse", { angle: 0.5 }), midPoint: null } },
    }),
    scene("suggested-binding-frame", {
      elements: [up.newFrameElement({ id: "frame", x: 150, y: 30, width: 200, height: 160, seed: 13 }), bindingArrow()],
      appState: {
        zoom: { value: 2 },
        newElement: bindingArrow(),
        suggestedBinding: { element: up.newFrameElement({ id: "frame", x: 150, y: 30, width: 200, height: 160, seed: 13 }), midPoint: null },
      },
      pointer: [140, 110],
    }),
    scene("suggested-binding-angle-locked", {
      elements: [target(), bindingArrow()],
      appState: { newElement: bindingArrow(), suggestedBinding: { element: target(), midPoint: null } },
      pointer: [165, 115],
      angleLocked: true,
    }),
    scene("suggested-binding-disabled", {
      elements: [target(), bindingArrow()],
      appState: { isBindingEnabled: false, newElement: bindingArrow(), suggestedBinding: { element: target(), midPoint: null } },
    }),
    // highlights
    scene("frame-to-highlight", {
      elements: [up.newFrameElement({ id: "frame", x: 40, y: 30, width: 220, height: 170, seed: 13 })],
      appState: { zoom: { value: 1.5 }, frameToHighlight: up.newFrameElement({ id: "frame", x: 40, y: 30, width: 220, height: 170, seed: 13 }) },
    }),
    scene("elements-to-highlight", {
      elements: grouped(),
      appState: { elementsToHighlight: grouped() },
    }),
    scene("elements-to-highlight-editing-group", {
      elements: grouped(),
      appState: { elementsToHighlight: grouped(), editingGroupId: "outer", theme: "dark" },
    }),
    scene("active-locked-element", {
      elements: [rect(up, "a", 50, 60, 160, 90, { locked: true })],
      appState: { activeLockedId: "a" },
    }),
    scene("active-locked-group", {
      elements: [rect(up, "a", 30, 40, 100, 60, { locked: true, groupIds: ["lg"] }), rect(up, "b", 180, 90, 90, 90, { locked: true, groupIds: ["lg"] })],
      appState: { activeLockedId: "lg" },
    }),
    // snap lines
    scene("snap-lines", { elements: three(), appState: { selectedElementIds: ids("a"), snapLines } }),
    scene("snap-lines-zen-dark-zoomed", { elements: three(), appState: { zenModeEnabled: true, theme: "dark", zoom: { value: 0.8 }, scrollX: 12, snapLines } }),
    // text
    scene("text-editing", {
      elements: [text({ angle: 0.25 })],
      appState: { selectedElementIds: ids("text"), editingTextElement: text({ angle: 0.25 }) },
    }),
    scene("text-editing-auto-resize", {
      elements: [text({ autoResize: true })],
      appState: { selectedElementIds: ids("text"), editingTextElement: text({ autoResize: true }) },
    }),
    scene("text-selected-fixed-width", {
      elements: [text()],
      appState: { zoom: { value: 2 }, selectedElementIds: ids("text") },
    }),
    scene("text-selected-short", {
      elements: [text({ text: "one", fontSize: 8 })],
      appState: { selectedElementIds: ids("text") },
    }),
    scene("text-selected-tablet", {
      elements: [text()],
      appState: { selectedElementIds: ids("text") },
      editorInterface: { formFactor: "tablet", userAgent: { isMobileDevice: true } },
    }),
  ];
};

// -- running upstream -------------------------------------------------------------

/** JSON round trip: what a scene file holds and the Rust side reads. */
const plain = (value) => JSON.parse(JSON.stringify(value));

const run = (up, window, s) => {
  const elements = plain(s.elements);
  const appState = plain(s.appState);
  window.devicePixelRatio = s.scale;
  globalThis.devicePixelRatio = s.scale;
  const canvas = window.document.createElement("canvas");
  canvas.width = s.width;
  canvas.height = s.height;
  const map = up.arrayToMap(elements);
  const selectedElements = elements.filter((e) => appState.selectedElementIds[e.id]);
  // the elements the app state holds by reference are the scene's, as in
  // the editor (activeEmbeddable is compared by identity)
  const scoped = (element) => element && map.get(element.id);
  const live = {
    ...appState,
    collaborators: new Map(),
    multiElement: scoped(appState.multiElement),
    newElement: scoped(appState.newElement),
    activeEmbeddable: appState.activeEmbeddable && { ...appState.activeEmbeddable, element: scoped(appState.activeEmbeddable.element) },
  };
  up.renderInteractiveScene({
    app: {
      lastPointerMoveCoords: s.pointer && { x: s.pointer[0], y: s.pointer[1] },
      // shouldRotateWithDiscreteAngle reads the event's shiftKey
      lastPointerMoveEvent: s.angleLocked ? { shiftKey: true } : null,
    },
    canvas,
    elementsMap: map,
    visibleElements: elements,
    selectedElements,
    allElementsMap: map,
    scale: s.scale,
    appState: live,
    renderConfig: {
      remotePointerViewportCoords: new Map(),
      remotePointerButton: new Map(),
      remoteSelectedElementIds: new Map(),
      remotePointerUsernames: new Map(),
      remotePointerUserStates: new Map(),
      selectionColor: s.selectionColor,
      renderScrollbars: false,
      lastViewportPosition: { x: 0, y: 0 },
    },
    editorInterface: s.editorInterface,
    callback: () => {},
    animationState: undefined,
    deltaTime: 0,
  });
  return {
    name: s.name,
    width: s.width,
    height: s.height,
    scale: s.scale,
    elements,
    appState,
    pointer: s.pointer,
    angleLocked: s.angleLocked,
    editorInterface: s.editorInterface,
    selectionColor: s.selectionColor,
    events: canvas.getContext("2d").events,
  };
};

const build = async (upstream) => {
  const window = installDom(ORIGIN);
  // getFeatureFlag reads the page's storage (none set: the defaults)
  Object.defineProperty(globalThis, "localStorage", { value: window.localStorage, configurable: true, writable: true });
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  const out = [];
  const names = (up.reseed(RANDOM_SEED), scenes(up)).map((s) => s.name);
  if (new Set(names).size !== names.length) throw new Error("duplicate scene names");
  for (const name of names) {
    // every scene's elements from the same seed, whatever came before
    up.reseed(RANDOM_SEED);
    const s = scenes(up).find((x) => x.name === name);
    out.push(run(up, window, s));
  }
  return format({
    description:
      "Upstream renderInteractiveScene (packages/excalidraw/renderer/interactiveScene.ts) at the pinned commit on a recording 2D context (tools/goldens/interactive-scene.mjs): per scene the inputs and every draw in order with its path, matrix, alpha and styles. No collaborators, no scrollbars, no animation (deltaTime 0); text measures 10 px per UTF-16 code unit.",
    upstream: upstream.commit,
    scenes: out,
  });
};

/** Runs fn with Math.random disabled: nothing here may draw. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating interactive-scene goldens");
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
    process.stderr.write(`interactive-scene: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("interactive-scene goldens are out of date: run node tools/goldens/interactive-scene.mjs\n");
      process.exit(1);
    }
    process.stdout.write("interactive-scene goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
