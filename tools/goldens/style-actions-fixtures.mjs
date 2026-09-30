#!/usr/bin/env node
// Style action fixtures for excali-editor (ex-540): upstream's own
// `perform` of every action the styles panel (and its keys) triggers, run
// from the pinned checkout under plain Node on hand-written scenes.
//
//   node tools/goldens/style-actions-fixtures.mjs            write the fixture
//   node tools/goldens/style-actions-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/style-actions-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/style-actions.json:
//
//   { "description", "upstream", "cases": [ { id, action, value,
//     elements, appState, result } ] }
//
// - `action`: the action's name (changeStrokeColor, changeBackgroundColor,
//   changeBucketFillBackgroundColor, changeFillStyle, changeStrokeWidth,
//   changeSloppiness, changeFreedrawMode, changeStrokeStyle, changeOpacity,
//   changeFontSize, increaseFontSize, decreaseFontSize, changeFontFamily,
//   changeTextAlign, changeVerticalAlign, changeRoundness, changeArrowhead,
//   changeArrowType of packages/excalidraw/actions/actionProperties.tsx;
//   togglePolygon of actionLinearEditor.tsx; alignTop, alignBottom,
//   alignLeft, alignRight, alignVerticallyCentered, alignHorizontallyCentered
//   of actionAlign.tsx; distributeHorizontally and distributeVertically of
//   actionDistribute.tsx; hyperlink of actionLink.tsx; cropEditor of
//   actionCropEditor.tsx);
// - `value`: the `value` perform is called with, as JSON (changeFontFamily's
//   `cachedElements` Map is an object of element JSON by id);
// - `elements`: the scene (JSON as upstream holds it, deleted elements
//   included, indexed by upstream's Scene);
// - `appState`: the keys set over getDefaultAppState() in test mode
//   (selectedElementIds, selectedGroupIds and editingGroupId always;
//   editingTextElement as the scene's element; selectedLinearElement as
//   { elementId, isEditing });
// - `result`: null when perform returned false, else { elements (every
//   key; null when the result has none), appState (every key whose JSON
//   differs from the app state perform was given, selectedLinearElement
//   as { elementId, isEditing }), captureUpdate }.
//
// perform runs as the action manager calls it, action.perform(elements,
// appState, value, app), on an app of { scene, state: appState, props: {
// ownerDocument } } whose document answers fonts.check with true (the font
// is loaded: changeFontFamily redraws synchronously) and whose
// dismissLinearEditor does nothing (upstream defers it to a timeout).
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), text measures 10 px per UTF-16 code
// unit (setCustomTextMetricsProvider), each scene is built after reseed(1)
// and every perform runs after reseed(1). Math.random throws while
// generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { installDom } from "./lib/recording-context.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "style-actions.json";

const ORIGIN = "http://localhost";

const ENTRY = `
export { reseed } from "./packages/common/src/random";
export {
  DEFAULT_VERTICAL_ALIGN,
  getStrokeWidthByKey,
  getUpdatedTimestamp,
  ROUNDNESS,
} from "./packages/common/src/index";
export {
  newElement,
  newTextElement,
  newArrowElement,
  newLinearElement,
  newFreeDrawElement,
  newFrameElement,
  newStickyNoteElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { mutateElement } from "./packages/element/src/mutateElement";
export { redrawTextBoundingBox } from "./packages/element/src/textElement";
export { Scene } from "./packages/element/src/Scene";
export { syncInvalidIndices } from "./packages/element/src/fractionalIndex";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export {
  actionChangeStrokeColor,
  actionChangeBackgroundColor,
  actionChangeBucketFillBackgroundColor,
  actionChangeFillStyle,
  actionChangeStrokeWidth,
  actionChangeSloppiness,
  actionChangeFreedrawMode,
  actionChangeStrokeStyle,
  actionChangeOpacity,
  actionChangeFontSize,
  actionIncreaseFontSize,
  actionDecreaseFontSize,
  actionChangeFontFamily,
  actionChangeTextAlign,
  actionChangeVerticalAlign,
  actionChangeRoundness,
  actionChangeArrowhead,
  actionChangeArrowType,
} from "./packages/excalidraw/actions/actionProperties";
export { actionTogglePolygon } from "./packages/excalidraw/actions/actionLinearEditor";
export {
  actionAlignTop,
  actionAlignBottom,
  actionAlignLeft,
  actionAlignRight,
  actionAlignVerticallyCentered,
  actionAlignHorizontallyCentered,
} from "./packages/excalidraw/actions/actionAlign";
export { distributeHorizontally, distributeVertically } from "./packages/excalidraw/actions/actionDistribute";
export { actionLink } from "./packages/excalidraw/actions/actionLink";
export { actionToggleCropEditor } from "./packages/excalidraw/actions/actionCropEditor";
`;

// Packages the actions' module graph (their panels, through the components
// they import) reaches but perform never calls.
const STUBS = ["fuzzy", "pica", "react-dom", "browser-fs-access", "image-blob-reduce"];

const SHIMS = {
  "packages/excalidraw/components/App": `module.exports = {
  useExcalidrawContainer: () => ({ id: "excali-id", container: null }),
  useEditorInterface: () => ({ formFactor: "desktop", desktopUIMode: "full" }),
  useApp: () => ({}),
  useAppProps: () => ({}),
  useExcalidrawAppState: () => ({}),
  useExcalidrawElements: () => [],
  useExcalidrawSetAppState: () => () => {},
  useExcalidrawActionManager: () => ({}),
};`,
  "react/jsx-runtime": "module.exports = { jsx: () => null, jsxs: () => null, Fragment: 'fragment' };",
  react: `const id = (f) => f;
module.exports = {
  Fragment: "fragment",
  createContext: () => ({ Provider: id }),
  forwardRef: id,
  memo: id,
  useCallback: id,
  useContext: () => undefined,
  useEffect: () => {},
  useLayoutEffect: () => {},
  useMemo: (f) => f(),
  useRef: (v) => ({ current: v }),
  useState: (v) => [v, () => {}],
};`,
  "jotai-scope": `module.exports = { createIsolation: () => ({ Provider() {}, useAtom() { return []; }, useAtomValue() {}, useSetAtom() {}, useStore() {} }) };`,
  jotai: `module.exports = { atom: (init) => ({ init }), createStore: () => ({ get() {}, set() {}, sub() {} }) };`,
};

const usage = () => {
  process.stderr.write("usage: style-actions-fixtures.mjs [--check] [--out DIR]\n");
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

const clone = (value) => (value === undefined ? null : JSON.parse(JSON.stringify(value)));

// -- scenes -------------------------------------------------------------------

const NO_ELEMENTS = new Map();

/** A filled shape as API.createElement leaves it. */
const el = (up, type, id, x, y, width = 100, height = 100, rest = {}) =>
  apiCreateElement(up, { type, id, x, y, width, height, backgroundColor: "#a5d8ff", ...rest });

/** A linear element from (x, y) through `points` (relative). */
const linear = (up, type, id, x, y, points, rest = {}) => {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  return apiCreateElement(up, {
    type,
    id,
    x,
    y,
    width: Math.max(...xs) - Math.min(...xs),
    height: Math.max(...ys) - Math.min(...ys),
    points,
    ...rest,
  });
};

/** A text measured as upstream measures it (10 px per code unit). */
const text = (up, id, content, x, y, rest = {}) => {
  const t = up.newTextElement({ id, text: content, x, y, fontSize: 20, fontFamily: 5, ...rest });
  if (rest.groupIds) t.groupIds = rest.groupIds;
  return t;
};

/** `text` bound into `container` (the layout is done once the scene exists). */
const label = (up, container, id, content, rest = {}) => {
  const t = text(up, id, content, 0, 0, { textAlign: "center", verticalAlign: "middle", ...rest });
  container.boundElements = [...(container.boundElements ?? []), { type: "text", id }];
  t.containerId = container.id;
  return t;
};

const sticky = (up, id, x, y, size = 200, rest = {}) =>
  up.newStickyNoteElement({ type: "stickynote", id, x, y, width: size, height: size, ...rest });

/** A sticky note's label as the app creates it (the note's ink, a ceiling). */
const stickyLabel = (up, note, id, content, rest = {}) =>
  label(up, note, id, content, { fontSize: 28, baseFontSize: 28, strokeColor: note.strokeColor, ...rest });

const binding = (elementId, fixedPoint = [0.5, 0.5]) => ({ elementId, fixedPoint, mode: "orbit" });

const boundArrow = (element, arrowId) => {
  element.boundElements = [...(element.boundElements ?? []), { type: "arrow", id: arrowId }];
};

const SCENES = {
  shapes: (up) => [
    el(up, "rectangle", "r", 100, 100),
    // legacy roundness (ROUNDNESS.LEGACY)
    Object.assign(el(up, "rectangle", "rl", 250, 100), { roundness: { type: 1 } }),
    el(up, "ellipse", "e", 400, 100),
    el(up, "diamond", "d", 550, 100),
    linear(up, "line", "l", 100, 300, [[0, 0], [100, 50]]),
    linear(up, "line", "lp", 250, 300, [[0, 0], [100, 0], [100, 100], [5, 5]], { backgroundColor: "transparent" }),
    linear(up, "line", "l3", 400, 300, [[0, 0], [100, 0], [50, 80]]),
    linear(up, "line", "l3c", 550, 300, [[0, 0], [100, 0], [0, 0]]),
    linear(up, "line", "lpoly", 700, 300, [[0, 0], [100, 0], [100, 100], [0, 0]], { polygon: true }),
    linear(up, "freedraw", "fd", 100, 500, [[0, 0], [10, 10], [20, 5]]),
    text(up, "t", "hello", 300, 500),
    text(up, "tr", "right side", 500, 500, { textAlign: "right" }),
    el(up, "image", "img", 700, 500),
    el(up, "rectangle", "lk", 800, 100, 100, 100, { locked: true }),
    el(up, "rectangle", "del", 900, 100, 100, 100, { isDeleted: true }),
    up.newFrameElement({ id: "f", x: 0, y: 700, width: 300, height: 200 }),
  ],
  labels: (up) => {
    const box = el(up, "rectangle", "box", 100, 100, 200, 100);
    const ell = el(up, "ellipse", "ell", 400, 100, 200, 120);
    const arr = linear(up, "arrow", "arr", 100, 400, [[0, 0], [240, 0]]);
    const fixed = text(up, "fixed", "a fixed width text", 100, 600, { textAlign: "center" });
    fixed.autoResize = false;
    fixed.width = 60;
    return [
      box,
      label(up, box, "box-label", "label"),
      ell,
      label(up, ell, "ell-label", "ellipse label"),
      arr,
      label(up, arr, "arr-label", "via"),
      fixed,
      text(up, "free", "free", 500, 600),
    ];
  },
  sticky: (up) => {
    const s1 = sticky(up, "s1", 100, 100);
    const s2 = sticky(up, "s2", 400, 100);
    const s3 = sticky(up, "s3", 700, 100);
    const s4 = sticky(up, "s4", 100, 400);
    // data that drifted from the invariants
    s4.backgroundColor = "transparent";
    s4.fillStyle = "hachure";
    s4.width = 50;
    const sa = linear(up, "arrow", "sa", -100, 200, [[0, 0], [195, 0]], { endBinding: binding("s1", [0, 0.5]) });
    boundArrow(s1, "sa");
    return [
      s1,
      stickyLabel(up, s1, "s1-label", "note"),
      s2,
      s3,
      stickyLabel(up, s3, "s3-label", "ink", { strokeColor: "transparent" }),
      s4,
      sa,
      el(up, "rectangle", "r", 400, 400),
    ];
  },
  arrows: (up) => {
    const a = el(up, "rectangle", "a", 100, 100);
    const b = el(up, "rectangle", "b", 400, 300);
    const ab = linear(up, "arrow", "ab", 205, 150, [[0, 0], [190, 200]], {
      startBinding: binding("a", [1, 0.5]),
      endBinding: binding("b", [0, 0.5]),
      endArrowhead: "arrow",
    });
    const eb = linear(up, "arrow", "eb", 150, 205, [[0, 0], [0, 145], [245, 145]], {
      elbowed: true,
      startBinding: binding("a", [0.5, 1]),
      endBinding: binding("b", [0, 0.5]),
    });
    boundArrow(a, "ab");
    boundArrow(b, "ab");
    boundArrow(a, "eb");
    boundArrow(b, "eb");
    return [
      a,
      b,
      ab,
      eb,
      linear(up, "arrow", "free", 600, 100, [[0, 0], [100, 50], [200, 0]], { roundness: true }),
      linear(up, "arrow", "rot", 600, 300, [[0, 0], [150, 0]], { angle: 0.5 }),
      linear(up, "line", "ln", 600, 500, [[0, 0], [100, 100]]),
    ];
  },
  frames: (up) => [
    up.newFrameElement({ id: "f", x: 0, y: 0, width: 500, height: 400 }),
    el(up, "rectangle", "in1", 50, 50, 100, 100, { frameId: "f" }),
    el(up, "rectangle", "in2", 300, 200, 100, 100, { frameId: "f" }),
    el(up, "rectangle", "out", 700, 100),
  ],
  groups: (up) => [
    el(up, "rectangle", "g1a", 100, 100, 100, 100, { groupIds: ["g1"] }),
    el(up, "rectangle", "g1b", 250, 150, 100, 100, { groupIds: ["g1"] }),
    el(up, "ellipse", "g2a", 500, 300, 80, 60, { groupIds: ["g2"] }),
    el(up, "ellipse", "g2b", 620, 420, 80, 60, { groupIds: ["g2"] }),
    el(up, "rectangle", "loose", 50, 500, 60, 60),
    el(up, "rectangle", "n1", 900, 100, 50, 50, { groupIds: ["inner", "outer"] }),
    el(up, "rectangle", "n2", 1000, 200, 50, 50, { groupIds: ["inner", "outer"] }),
    el(up, "rectangle", "n3", 950, 400, 50, 50, { groupIds: ["outer"] }),
    el(up, "rectangle", "wide", 100, 700, 400, 50),
    el(up, "rectangle", "mid", 200, 800, 50, 50),
  ],
  fonts: (up) => {
    const box = el(up, "rectangle", "box", 100, 300, 120, 60);
    const note = sticky(up, "s", 500, 300);
    return [
      text(up, "t1", "small", 100, 100, { fontSize: 16 }),
      text(up, "t2", "centered", 300, 100, { textAlign: "center" }),
      text(up, "t3", "right", 500, 100, { textAlign: "right", fontSize: 36 }),
      text(up, "t4", "virgil", 700, 100, { fontFamily: 1 }),
      box,
      label(up, box, "box-label", "a label"),
      note,
      stickyLabel(up, note, "s-label", "a sticky note label"),
    ];
  },
  manyTexts: (up) =>
    Array.from({ length: 201 }, (_, i) => text(up, `m${i}`, "x", (i % 20) * 30, Math.floor(i / 20) * 30)),
  longText: (up) => [text(up, "long", "abcdefghij".repeat(501), 0, 0), text(up, "short", "short", 0, 100)],
};

/**
 * A scene built after reseed(1), its labels laid out (redrawTextBoundingBox
 * of each bound text, the sticky note fit for a note's), indexed by
 * upstream's Scene.
 */
const buildScene = (up, name) => {
  up.reseed(1);
  const scene = new up.Scene(up.syncInvalidIndices(SCENES[name](up)));
  for (const element of scene.getElementsIncludingDeleted()) {
    if (element.type === "text" && element.containerId) {
      up.redrawTextBoundingBox(element, scene.getElement(element.containerId), scene);
    }
  }
  return scene;
};

// -- cases --------------------------------------------------------------------

const selecting = (...ids) => Object.fromEntries(ids.map((id) => [id, true]));

/** [id, scene, action, value, app state keys (editingTextElement by id)] */
const CASES = [
  // changeStrokeColor
  ["stroke-rectangle", "shapes", "changeStrokeColor", { color: "#e03131" }, { selectedElementIds: selecting("r") }],
  ["stroke-with-deleted", "shapes", "changeStrokeColor", { color: "#e03131" }, { selectedElementIds: selecting("r", "del", "lk") }],
  ["stroke-no-color", "shapes", "changeStrokeColor", { currentItemStrokeColor: "#2f9e44" }, { selectedElementIds: selecting("r") }],
  ["stroke-nothing-selected", "shapes", "changeStrokeColor", { color: "#1971c2" }, {}],
  ["stroke-many", "shapes", "changeStrokeColor", { color: "#1971c2" }, { selectedElementIds: selecting("e", "l", "fd", "t", "img", "f") }],
  ["stroke-labelled", "labels", "changeStrokeColor", { color: "#e03131" }, { selectedElementIds: selecting("box", "arr") }],
  ["stroke-sticky", "sticky", "changeStrokeColor", { color: "#e03131" }, { selectedElementIds: selecting("s1") }],
  ["stroke-sticky-transparent", "sticky", "changeStrokeColor", { color: "transparent" }, { selectedElementIds: selecting("s1", "s2") }],
  ["stroke-sticky-label-editing", "sticky", "changeStrokeColor", { color: "#1971c2" }, { editingTextElement: "s1-label" }],
  ["stroke-sticky-label-transparent", "sticky", "changeStrokeColor", { color: "transparent" }, { editingTextElement: "s1-label" }],
  ["stroke-sticky-drifted-ink", "sticky", "changeStrokeColor", { color: "#2f9e44" }, { selectedElementIds: selecting("s3") }],
  ["stroke-sticky-other", "sticky", "changeStrokeColor", { color: "#2f9e44" }, { selectedElementIds: selecting("r") }],
  ["stroke-mixed", "sticky", "changeStrokeColor", { color: "#f08c00" }, { selectedElementIds: selecting("s2", "r") }],
  ["stroke-sticky-unnormalized", "sticky", "changeStrokeColor", { color: "#f08c00" }, { selectedElementIds: selecting("s4") }],
  ["stroke-sticky-tool", "shapes", "changeStrokeColor", { color: "#f08c00" }, { activeTool: { type: "stickynote", customType: null, locked: false, fromSelection: false, lastActiveTool: null } }],

  // changeBackgroundColor
  ["background-rectangle", "shapes", "changeBackgroundColor", { color: "#ffc9c9" }, { selectedElementIds: selecting("r", "e") }],
  ["background-no-color", "shapes", "changeBackgroundColor", { currentItemBackgroundColor: "#ffc9c9" }, { selectedElementIds: selecting("r") }],
  ["background-polygon", "shapes", "changeBackgroundColor", { color: "#b2f2bb" }, { selectedElementIds: selecting("lp") }],
  ["background-polygon-three", "shapes", "changeBackgroundColor", { color: "#b2f2bb" }, { selectedElementIds: selecting("l3", "lp") }],
  ["background-polygon-already", "shapes", "changeBackgroundColor", { color: "#b2f2bb" }, { selectedElementIds: selecting("lpoly") }],
  ["background-polygon-transparent", "shapes", "changeBackgroundColor", { color: "transparent" }, { selectedElementIds: selecting("lp") }],
  ["background-line-two-points", "shapes", "changeBackgroundColor", { color: "#b2f2bb" }, { selectedElementIds: selecting("l") }],
  ["background-line-collinear", "shapes", "changeBackgroundColor", { color: "#b2f2bb" }, { selectedElementIds: selecting("l3c") }],
  ["background-lines-and-shape", "shapes", "changeBackgroundColor", { color: "#b2f2bb" }, { selectedElementIds: selecting("lp", "r") }],
  ["background-text", "shapes", "changeBackgroundColor", { color: "#b2f2bb" }, { selectedElementIds: selecting("t", "fd") }],
  ["background-labelled", "labels", "changeBackgroundColor", { color: "#ffec99" }, { selectedElementIds: selecting("box", "ell") }],
  ["background-sticky", "sticky", "changeBackgroundColor", { color: "#ffc9c9" }, { selectedElementIds: selecting("s1") }],
  ["background-sticky-transparent", "sticky", "changeBackgroundColor", { color: "transparent" }, { selectedElementIds: selecting("s1", "s2") }],
  ["background-sticky-label-editing", "sticky", "changeBackgroundColor", { color: "#a5d8ff" }, { editingTextElement: "s1-label" }],
  ["background-mixed", "sticky", "changeBackgroundColor", { color: "#a5d8ff" }, { selectedElementIds: selecting("s2", "r") }],
  ["background-sticky-unnormalized", "sticky", "changeBackgroundColor", { color: "#a5d8ff" }, { selectedElementIds: selecting("s4") }],

  // changeBucketFillBackgroundColor
  ["bucket-fill", "shapes", "changeBucketFillBackgroundColor", { currentItemBackgroundColor: "#ffec99" }, { selectedElementIds: selecting("r") }],

  // changeFillStyle
  ["fill-zigzag", "shapes", "changeFillStyle", "zigzag", { selectedElementIds: selecting("r", "e", "t", "l") }],
  ["fill-cross-hatch", "shapes", "changeFillStyle", "cross-hatch", { selectedElementIds: selecting("d") }],
  ["fill-sticky", "sticky", "changeFillStyle", "hachure", { selectedElementIds: selecting("s1", "r") }],
  ["fill-sticky-unnormalized", "sticky", "changeFillStyle", "solid", { selectedElementIds: selecting("s4") }],

  // changeStrokeWidth
  ["stroke-width-bold", "shapes", "changeStrokeWidth", "bold", { selectedElementIds: selecting("r", "fd", "l") }],
  ["stroke-width-thin", "shapes", "changeStrokeWidth", "thin", { selectedElementIds: selecting("fd", "t") }],
  ["stroke-width-same", "shapes", "changeStrokeWidth", "medium", { selectedElementIds: selecting("r") }],

  // changeSloppiness
  ["sloppiness", "shapes", "changeSloppiness", 2, { selectedElementIds: selecting("r", "e", "del") }],
  ["sloppiness-same", "shapes", "changeSloppiness", 1, { selectedElementIds: selecting("d") }],
  ["sloppiness-sticky", "sticky", "changeSloppiness", 0, { selectedElementIds: selecting("s1") }],

  // changeFreedrawMode
  ["freedraw-variable", "shapes", "changeFreedrawMode", "variable", { selectedElementIds: selecting("fd", "r") }],
  ["freedraw-null", "shapes", "changeFreedrawMode", null, { selectedElementIds: selecting("fd") }],

  // changeStrokeStyle
  ["stroke-style-dashed", "shapes", "changeStrokeStyle", "dashed", { selectedElementIds: selecting("r", "l") }],
  ["stroke-style-dotted", "labels", "changeStrokeStyle", "dotted", { selectedElementIds: selecting("box") }],

  // changeOpacity
  ["opacity", "labels", "changeOpacity", 50, { selectedElementIds: selecting("box", "free") }],
  ["opacity-editing", "labels", "changeOpacity", 30, { editingTextElement: "free" }],

  // changeFontSize
  ["font-size-free", "fonts", "changeFontSize", 28, { selectedElementIds: selecting("t1") }],
  ["font-size-two", "fonts", "changeFontSize", 28, { selectedElementIds: selecting("t1", "t2", "t3") }],
  ["font-size-label", "fonts", "changeFontSize", 36, { selectedElementIds: selecting("box") }],
  ["font-size-sticky", "fonts", "changeFontSize", 36, { selectedElementIds: selecting("s") }],
  ["font-size-sticky-large", "fonts", "changeFontSize", 200, { selectedElementIds: selecting("s") }],
  ["font-size-fixed-width", "labels", "changeFontSize", 28, { selectedElementIds: selecting("fixed", "free") }],
  ["font-size-arrow-label", "labels", "changeFontSize", 36, { selectedElementIds: selecting("arr") }],
  ["font-size-editing", "fonts", "changeFontSize", 24, { editingTextElement: "t2" }],
  ["font-size-no-text", "shapes", "changeFontSize", 20, { selectedElementIds: selecting("r") }],

  // increaseFontSize and decreaseFontSize
  ["increase-font-size", "fonts", "increaseFontSize", null, { selectedElementIds: selecting("t1") }],
  ["increase-font-size-mixed", "fonts", "increaseFontSize", null, { selectedElementIds: selecting("t1", "t3", "box") }],
  ["increase-font-size-sticky", "fonts", "increaseFontSize", null, { selectedElementIds: selecting("s") }],
  ["decrease-font-size", "fonts", "decreaseFontSize", null, { selectedElementIds: selecting("t3", "t2") }],
  ["decrease-font-size-sticky", "fonts", "decreaseFontSize", null, { selectedElementIds: selecting("s") }],
  ["decrease-font-size-editing", "fonts", "decreaseFontSize", null, { editingTextElement: "t2" }],

  // changeFontFamily
  ["font-family-select", "fonts", "changeFontFamily", { currentItemFontFamily: 6 }, { selectedElementIds: selecting("t1", "t4") }],
  ["font-family-select-same", "fonts", "changeFontFamily", { currentItemFontFamily: 1 }, { selectedElementIds: selecting("t4") }],
  ["font-family-label", "fonts", "changeFontFamily", { currentItemFontFamily: 8 }, { selectedElementIds: selecting("box", "s") }],
  ["font-family-hover", "fonts", "changeFontFamily", { currentHoveredFontFamily: 7, openPopup: "fontFamily" }, { selectedElementIds: selecting("t1", "t4", "box") }],
  ["font-family-hover-same", "fonts", "changeFontFamily", { currentHoveredFontFamily: 1 }, { selectedElementIds: selecting("t4", "t1") }],
  ["font-family-hover-reset-containers", "fonts", "changeFontFamily", "resetContainers", { selectedElementIds: selecting("box") }],
  ["font-family-reset-all", "fonts", "changeFontFamily", "resetAll", { selectedElementIds: selecting("t1", "box") }],
  ["font-family-popup-only", "fonts", "changeFontFamily", { openPopup: "fontFamily" }, { selectedElementIds: selecting("t1") }],
  ["font-family-hover-many", "manyTexts", "changeFontFamily", { currentHoveredFontFamily: 6 }, { selectedElementIds: selecting(...Array.from({ length: 201 }, (_, i) => `m${i}`)) }],
  ["font-family-hover-long", "longText", "changeFontFamily", { currentHoveredFontFamily: 6 }, { selectedElementIds: selecting("long", "short") }],
  ["font-family-select-long", "longText", "changeFontFamily", { currentItemFontFamily: 6 }, { selectedElementIds: selecting("short") }],
  ["font-family-editing", "fonts", "changeFontFamily", { currentItemFontFamily: 3 }, { editingTextElement: "t2" }],

  // changeTextAlign and changeVerticalAlign
  ["text-align-free", "fonts", "changeTextAlign", "center", { selectedElementIds: selecting("t1", "t3") }],
  ["text-align-label", "labels", "changeTextAlign", "left", { selectedElementIds: selecting("box", "arr") }],
  ["text-align-fixed", "labels", "changeTextAlign", "right", { selectedElementIds: selecting("fixed") }],
  ["text-align-sticky", "sticky", "changeTextAlign", "right", { selectedElementIds: selecting("s1") }],
  ["vertical-align-top", "labels", "changeVerticalAlign", "top", { selectedElementIds: selecting("box", "ell") }],
  ["vertical-align-bottom", "fonts", "changeVerticalAlign", "bottom", { selectedElementIds: selecting("box", "t1") }],
  ["vertical-align-sticky", "sticky", "changeVerticalAlign", "top", { selectedElementIds: selecting("s1") }],

  // changeRoundness
  ["roundness-round", "shapes", "changeRoundness", "round", { selectedElementIds: selecting("r", "e", "d", "l", "t", "img") }],
  ["roundness-legacy-round", "shapes", "changeRoundness", "round", { selectedElementIds: selecting("rl") }],
  ["roundness-sharp", "shapes", "changeRoundness", "sharp", { selectedElementIds: selecting("rl", "r") }],
  ["roundness-arrows", "arrows", "changeRoundness", "round", { selectedElementIds: selecting("ab", "eb") }],
  ["roundness-sticky", "sticky", "changeRoundness", "round", { selectedElementIds: selecting("s1") }],

  // changeArrowhead
  ["arrowhead-start", "arrows", "changeArrowhead", { position: "start", type: "triangle" }, { selectedElementIds: selecting("ab", "a", "ln") }],
  ["arrowhead-end-none", "arrows", "changeArrowhead", { position: "end", type: null }, { selectedElementIds: selecting("ab", "eb") }],
  ["arrowhead-end-cardinality", "arrows", "changeArrowhead", { position: "end", type: "cardinality_many" }, { selectedElementIds: selecting("free") }],

  // changeArrowType
  ["arrow-type-elbow", "arrows", "changeArrowType", "elbow", { selectedElementIds: selecting("ab"), selectedLinearElement: "ab" }],
  ["arrow-type-round", "arrows", "changeArrowType", "round", { selectedElementIds: selecting("ab") }],
  ["arrow-type-sharp-from-elbow", "arrows", "changeArrowType", "sharp", { selectedElementIds: selecting("eb"), selectedLinearElement: "eb" }],
  ["arrow-type-round-from-elbow", "arrows", "changeArrowType", "round", { selectedElementIds: selecting("eb") }],
  ["arrow-type-sharp-inside", "arrows", "changeArrowType", "sharp", { selectedElementIds: selecting("eb", "ab"), bindMode: "inside" }],
  ["arrow-type-elbow-free", "arrows", "changeArrowType", "elbow", { selectedElementIds: selecting("free", "ln") }],
  ["arrow-type-elbow-rotated", "arrows", "changeArrowType", "elbow", { selectedElementIds: selecting("rot") }],
  ["arrow-type-elbow-again", "arrows", "changeArrowType", "elbow", { selectedElementIds: selecting("eb"), selectedLinearElement: "eb" }],
  ["arrow-type-elbow-binding-off", "arrows", "changeArrowType", "elbow", { selectedElementIds: selecting("ab"), isBindingEnabled: false, zoom: { value: 2 } }],
  ["arrow-type-elbow-sticky", "sticky", "changeArrowType", "elbow", { selectedElementIds: selecting("sa") }],
  ["arrow-type-elbow-label", "labels", "changeArrowType", "elbow", { selectedElementIds: selecting("arr") }],
  ["arrow-type-other-selected", "arrows", "changeArrowType", "round", { selectedElementIds: selecting("free"), selectedLinearElement: "ab" }],

  // togglePolygon
  ["polygon-enable", "shapes", "togglePolygon", null, { selectedElementIds: selecting("lp", "l3") }],
  ["polygon-disable", "shapes", "togglePolygon", null, { selectedElementIds: selecting("lpoly") }],
  ["polygon-mixed-state", "shapes", "togglePolygon", null, { selectedElementIds: selecting("lpoly", "lp") }],
  ["polygon-cannot", "shapes", "togglePolygon", null, { selectedElementIds: selecting("l3c", "l") }],
  ["polygon-not-lines", "shapes", "togglePolygon", null, { selectedElementIds: selecting("lp", "r") }],

  // the aligns
  ...["alignTop", "alignBottom", "alignLeft", "alignRight", "alignVerticallyCentered", "alignHorizontallyCentered"].flatMap((action) => [
    [`${action}-bound`, "arrows", action, null, { selectedElementIds: selecting("a", "b") }],
    [`${action}-groups`, "groups", action, null, { selectedElementIds: selecting("g1a", "g1b", "g2a", "g2b", "loose"), selectedGroupIds: selecting("g1", "g2") }],
    [`${action}-labels`, "labels", action, null, { selectedElementIds: selecting("box", "ell") }],
  ]),
  ["alignLeft-frame", "frames", "alignLeft", null, { selectedElementIds: selecting("in1", "out") }],
  ["alignRight-frame", "frames", "alignRight", null, { selectedElementIds: selecting("in1", "out") }],
  ["alignTop-frame-inside", "frames", "alignTop", null, { selectedElementIds: selecting("in1", "in2") }],
  ["alignLeft-editing-group", "groups", "alignLeft", null, { selectedElementIds: selecting("g1a", "g1b"), editingGroupId: "g1" }],
  ["alignBottom-nested", "groups", "alignBottom", null, { selectedElementIds: selecting("n1", "n2", "n3"), selectedGroupIds: selecting("outer") }],
  ["alignLeft-one", "shapes", "alignLeft", null, { selectedElementIds: selecting("r") }],

  // the distributes
  ["distributeHorizontally-groups", "groups", "distributeHorizontally", null, { selectedElementIds: selecting("g1a", "g1b", "g2a", "g2b", "loose"), selectedGroupIds: selecting("g1", "g2") }],
  ["distributeVertically-groups", "groups", "distributeVertically", null, { selectedElementIds: selecting("g1a", "g1b", "g2a", "g2b", "loose"), selectedGroupIds: selecting("g1", "g2") }],
  ["distributeHorizontally-overlap", "groups", "distributeHorizontally", null, { selectedElementIds: selecting("wide", "mid", "loose") }],
  ["distributeVertically-shapes", "shapes", "distributeVertically", null, { selectedElementIds: selecting("r", "l", "fd", "img") }],
  ["distributeHorizontally-bound", "arrows", "distributeHorizontally", null, { selectedElementIds: selecting("a", "b", "free") }],
  ["distributeHorizontally-labels", "labels", "distributeHorizontally", null, { selectedElementIds: selecting("box", "ell", "free") }],
  ["distributeVertically-frame", "frames", "distributeVertically", null, { selectedElementIds: selecting("in1", "in2", "out") }],

  // hyperlink and cropEditor
  ["hyperlink", "shapes", "hyperlink", null, { selectedElementIds: selecting("r"), openMenu: "canvas" }],
  ["hyperlink-open", "shapes", "hyperlink", null, { selectedElementIds: selecting("r"), showHyperlinkPopup: "editor" }],
  ["hyperlink-info", "shapes", "hyperlink", null, { selectedElementIds: selecting("r"), showHyperlinkPopup: "info" }],
  ["crop-editor", "shapes", "cropEditor", null, { selectedElementIds: selecting("img"), isCropping: true }],
];

const ACTIONS = (up) => ({
  changeStrokeColor: up.actionChangeStrokeColor,
  changeBackgroundColor: up.actionChangeBackgroundColor,
  changeBucketFillBackgroundColor: up.actionChangeBucketFillBackgroundColor,
  changeFillStyle: up.actionChangeFillStyle,
  changeStrokeWidth: up.actionChangeStrokeWidth,
  changeSloppiness: up.actionChangeSloppiness,
  changeFreedrawMode: up.actionChangeFreedrawMode,
  changeStrokeStyle: up.actionChangeStrokeStyle,
  changeOpacity: up.actionChangeOpacity,
  changeFontSize: up.actionChangeFontSize,
  increaseFontSize: up.actionIncreaseFontSize,
  decreaseFontSize: up.actionDecreaseFontSize,
  changeFontFamily: up.actionChangeFontFamily,
  changeTextAlign: up.actionChangeTextAlign,
  changeVerticalAlign: up.actionChangeVerticalAlign,
  changeRoundness: up.actionChangeRoundness,
  changeArrowhead: up.actionChangeArrowhead,
  changeArrowType: up.actionChangeArrowType,
  togglePolygon: up.actionTogglePolygon,
  alignTop: up.actionAlignTop,
  alignBottom: up.actionAlignBottom,
  alignLeft: up.actionAlignLeft,
  alignRight: up.actionAlignRight,
  alignVerticallyCentered: up.actionAlignVerticallyCentered,
  alignHorizontallyCentered: up.actionAlignHorizontallyCentered,
  distributeHorizontally: up.distributeHorizontally,
  distributeVertically: up.distributeVertically,
  hyperlink: up.actionLink,
  cropEditor: up.actionToggleCropEditor,
});

/**
 * The font picker's values that reset what hovering changed: the selected
 * texts and their containers as cached when the popup opened (here: the
 * scene's, a container's height halved, a text's family and size changed).
 */
const fontReset = (scene, kind) => {
  const cached = {};
  for (const element of scene.getNonDeletedElements()) {
    const copy = clone(element);
    if (copy.type === "text") {
      copy.fontFamily = 2;
      copy.fontSize = 24;
    } else if (copy.boundElements?.some((b) => b.type === "text")) {
      copy.height /= 2;
    }
    cached[copy.id] = copy;
  }
  return kind === "resetAll"
    ? { openPopup: null, currentHoveredFontFamily: null, cachedElements: cached, resetAll: true }
    : { currentHoveredFontFamily: 7, cachedElements: cached, resetContainers: true };
};

/** The value as JSON and as upstream's perform receives it. */
const actionValue = (scene, value) => {
  const json = typeof value === "string" && value.startsWith("reset") ? fontReset(scene, value) : value;
  if (json && typeof json === "object" && json.cachedElements) {
    return [json, { ...clone(json), cachedElements: new Map(Object.entries(clone(json.cachedElements))) }];
  }
  return [json, clone(json)];
};

/** The app state keys a case sets, as upstream's perform reads them. */
const caseAppState = (up, scene, keys) => {
  const out = { selectedElementIds: {}, selectedGroupIds: {}, editingGroupId: null, ...keys };
  if (keys.editingTextElement) out.editingTextElement = scene.getElement(keys.editingTextElement);
  if (keys.selectedLinearElement) out.selectedLinearElement = { elementId: keys.selectedLinearElement, isEditing: false };
  return out;
};

const reduceAppStateValue = (key, value) =>
  key === "selectedLinearElement" && value ? { elementId: value.elementId, isEditing: value.isEditing } : value;

/** The keys whose JSON differs from the app state perform was given. */
const changedAppState = (before, after) => {
  const out = {};
  for (const key of Object.keys(after)) {
    const value = clone(reduceAppStateValue(key, after[key]));
    if (JSON.stringify(value) !== JSON.stringify(clone(reduceAppStateValue(key, before[key])))) {
      out[key] = value;
    }
  }
  return out;
};

const runCase = (up, [id, sceneName, action, rawValue, keys]) => {
  const scene = buildScene(up, sceneName);
  const elements = scene.getElementsIncludingDeleted();
  const recordedElements = clone(elements);
  const [jsonValue, value] = actionValue(scene, rawValue);
  const overrides = caseAppState(up, scene, keys);
  const appState = { ...up.getDefaultAppState(), ...overrides };
  const recordedAppState = clone(overrides);
  const app = {
    scene,
    state: appState,
    props: { ownerDocument: { fonts: { check: () => true } } },
    editorInterface: { formFactor: "desktop", desktopUIMode: "full" },
    dismissLinearEditor: () => {},
  };
  up.reseed(1);
  // changeProperty logs its invariant for a selected deleted element
  const error = console.error;
  console.error = (...args) => {
    if (!String(args[0]).startsWith("[NONDELETED][INVARIANT]")) error(...args);
  };
  let result;
  try {
    result = ACTIONS(up)[action].perform(elements, appState, value, app);
  } finally {
    console.error = error;
  }
  return {
    id,
    action,
    value: jsonValue,
    elements: recordedElements,
    appState: recordedAppState,
    result:
      result === false
        ? null
        : {
            elements: result.elements ? clone(result.elements) : null,
            appState: changedAppState(appState, result.appState),
            captureUpdate: result.captureUpdate,
          },
  };
};

// -- the fixture --------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating style action fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const asciiJson = (fixture) =>
  format(fixture).replace(
    /[\u0080-￿]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );

const buildFixture = (up, commit) => {
  const ids = new Set();
  const cases = [];
  for (const c of CASES) {
    if (ids.has(c[0])) throw new Error(`duplicate case id ${c[0]}`);
    ids.add(c[0]);
    cases.push(runCase(up, c));
  }
  return asciiJson({
    description:
      "The perform of the styles panel's actions (packages/excalidraw/actions/actionProperties.tsx, " +
      "actionLinearEditor.tsx togglePolygon, actionAlign.tsx, actionDistribute.tsx, actionLink.tsx, " +
      "actionCropEditor.tsx) on hand-written scenes, in upstream's test mode (ids id0.., timestamps 1, " +
      "reseed(1) before each perform, text 10 px per code unit). " +
      "Generated by tools/goldens/style-actions-fixtures.mjs.",
    upstream: commit,
    cases,
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`style-actions-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  installDom(ORIGIN);
  globalThis.requestAnimationFrame = () => 0;
  globalThis.cancelAnimationFrame = () => {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
      // trackEvent (changeFillStyle) returns before tracking
      "import.meta.env.VITE_WORKER_ID": "undefined",
      "import.meta.env.VITE_APP_ENABLE_TRACKING": "undefined",
    },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("style action fixture is out of date: run node tools/goldens/style-actions-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`style action fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
