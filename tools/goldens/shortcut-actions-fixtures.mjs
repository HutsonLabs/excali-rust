#!/usr/bin/env node
// Shortcut action fixtures for excali-editor (ex-541): upstream's own
// `perform` of the actions whose keys the editor's keyboard names but
// whose perform is neither a style action (ex-540) nor an earlier edit
// action, run from the pinned checkout under plain Node on hand-written
// scenes.
//
//   node tools/goldens/shortcut-actions-fixtures.mjs            write the fixture
//   node tools/goldens/shortcut-actions-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/shortcut-actions-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/shortcut-actions.json:
//
//   { "description", "upstream", "cases": [ { id, action, value, props,
//     copied, elements, appState, result } ] }
//
// - `action`: deselect (actionDeselect.ts), flipHorizontal and
//   flipVertical (actionFlip.ts), toggleElementLock (actionElementLock.ts),
//   copyStyles and pasteStyles (actionStyles.ts), viewMode
//   (actionToggleViewMode.tsx) and toggleTheme (actionCanvas.tsx);
// - `value`: the `value` perform is called with (toggleTheme's theme);
// - `props`: { onThemeChange } when the host handles the theme;
// - `copied`: actionStyles' module-level `copiedStyles` (a JSON string)
//   before perform: "{}" until a copyStyles ran; each pasteStyles case
//   runs copyStyles on its `copyFrom` selection first;
// - `elements`: the scene (JSON as upstream holds it, deleted elements
//   included, indexed by upstream's Scene);
// - `appState`: the keys set over getDefaultAppState() in test mode
//   (selectedElementIds, selectedGroupIds and editingGroupId always;
//   selectedLinearElement as { elementId, isEditing });
// - `result`: null when perform returned false, else { elements (every
//   key; null when the result has none), appState (every key whose JSON
//   differs from the app state perform was given, selectedLinearElement
//   as { elementId, isEditing }), captureUpdate, copied (copiedStyles
//   after perform) }.
//
// perform runs as the action manager calls it, action.perform(elements,
// appState, value, app), on an app of { scene, state: appState, props,
// cursor } (the cursor's applyForTool does nothing).
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
export const FIXTURE = "shortcut-actions.json";

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
export { actionDeselect } from "./packages/excalidraw/actions/actionDeselect";
export { actionFlipHorizontal, actionFlipVertical } from "./packages/excalidraw/actions/actionFlip";
export { actionToggleElementLock } from "./packages/excalidraw/actions/actionElementLock";
export { actionCopyStyles, actionPasteStyles, copiedStyles } from "./packages/excalidraw/actions/actionStyles";
export { actionToggleViewMode } from "./packages/excalidraw/actions/actionToggleViewMode";
export { actionToggleTheme } from "./packages/excalidraw/actions/actionCanvas";
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
  process.stderr.write("usage: shortcut-actions-fixtures.mjs [--check] [--out DIR]\n");
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
  selection: (up) => [
    el(up, "rectangle", "r", 100, 100),
    el(up, "rectangle", "g1a", 300, 100, 100, 100, { groupIds: ["g1"] }),
    el(up, "rectangle", "g1b", 450, 150, 100, 100, { groupIds: ["g1"] }),
    el(up, "rectangle", "n1", 900, 100, 50, 50, { groupIds: ["inner", "outer"] }),
    el(up, "rectangle", "n2", 1000, 200, 50, 50, { groupIds: ["inner", "outer"] }),
    el(up, "rectangle", "n3", 950, 400, 50, 50, { groupIds: ["outer"] }),
    linear(up, "line", "l", 100, 300, [[0, 0], [100, 50]]),
    el(up, "rectangle", "del", 900, 600, 100, 100, { isDeleted: true, groupIds: ["g1"] }),
  ],
  flip: (up) => {
    const box = el(up, "rectangle", "box", 100, 100, 200, 100);
    const ell = el(up, "ellipse", "ell", 400, 100, 120, 60);
    const arrLabelled = linear(up, "arrow", "arrl", 100, 700, [[0, 0], [240, 40]]);
    const a = el(up, "rectangle", "a", 100, 1000);
    const b = el(up, "rectangle", "b", 400, 1200);
    const ab = linear(up, "arrow", "ab", 205, 1050, [[0, 0], [190, 200]], {
      startBinding: binding("a", [1, 0.5]),
      endBinding: binding("b", [0, 0.5]),
      endArrowhead: "arrow",
    });
    const eb = linear(up, "arrow", "eb", 150, 1105, [[0, 0], [0, 145], [245, 145]], {
      elbowed: true,
      startBinding: binding("a", [0.5, 1]),
      endBinding: binding("b", [0, 0.5]),
      startArrowhead: "circle",
    });
    boundArrow(a, "ab");
    boundArrow(b, "ab");
    boundArrow(a, "eb");
    boundArrow(b, "eb");
    return [
      box,
      label(up, box, "box-label", "label"),
      ell,
      el(up, "diamond", "d", 600, 100, 80, 120),
      el(up, "rectangle", "rot", 800, 100, 100, 50, { angle: 0.5 }),
      linear(up, "line", "l", 100, 300, [[0, 0], [100, 50], [150, -20]]),
      linear(up, "arrow", "arr", 300, 300, [[0, 0], [200, 80]], { startArrowhead: "bar", endArrowhead: "arrow" }),
      linear(up, "freedraw", "fd", 600, 300, [[0, 0], [10, 10], [30, 5]]),
      text(up, "t", "hello", 800, 300),
      el(up, "image", "img", 100, 500, 100, 80, { scale: [1, 1] }),
      el(up, "rectangle", "g1a", 300, 500, 100, 100, { groupIds: ["g1"] }),
      el(up, "ellipse", "g1b", 450, 550, 80, 60, { groupIds: ["g1"] }),
      arrLabelled,
      label(up, arrLabelled, "arrl-label", "via"),
      up.newFrameElement({ id: "f", x: 500, y: 650, width: 400, height: 250 }),
      el(up, "rectangle", "in1", 550, 700, 100, 60, { frameId: "f" }),
      el(up, "ellipse", "in2", 700, 780, 80, 80, { frameId: "f" }),
      a,
      b,
      ab,
      eb,
      el(up, "rectangle", "del", 900, 1400, 100, 100, { isDeleted: true }),
    ];
  },
  lock: (up) => {
    const box = el(up, "rectangle", "box", 100, 400, 200, 100);
    return [
      el(up, "rectangle", "r1", 100, 100),
      el(up, "rectangle", "r2", 250, 100),
      el(up, "ellipse", "r3", 400, 100),
      el(up, "rectangle", "g1a", 100, 250, 100, 100, { groupIds: ["g1"] }),
      el(up, "rectangle", "g1b", 250, 250, 100, 100, { groupIds: ["g1"] }),
      el(up, "rectangle", "lk1", 600, 100, 100, 100, { locked: true, groupIds: ["lkg"] }),
      el(up, "rectangle", "lk2", 750, 100, 100, 100, { locked: true, groupIds: ["lkg"] }),
      el(up, "rectangle", "lk3", 900, 100, 100, 100, { locked: true }),
      el(up, "rectangle", "lg1", 600, 250, 100, 100, { locked: true, groupIds: ["lgroup"] }),
      el(up, "rectangle", "lg2", 750, 250, 100, 100, { locked: true, groupIds: ["lgroup"] }),
      box,
      label(up, box, "box-label", "label"),
      linear(up, "line", "l", 400, 400, [[0, 0], [100, 50]]),
      up.newFrameElement({ id: "f", x: 100, y: 600, width: 400, height: 250 }),
      el(up, "rectangle", "in1", 150, 650, 100, 60, { frameId: "f" }),
      el(up, "rectangle", "del", 900, 600, 100, 100, { isDeleted: true }),
    ];
  },
  styles: (up) => {
    const box = el(up, "rectangle", "box", 100, 400, 200, 100, { strokeColor: "#2f9e44", backgroundColor: "#b2f2bb" });
    const ell = el(up, "ellipse", "ell", 400, 400, 200, 120);
    const note = sticky(up, "s1", 100, 700);
    const note2 = sticky(up, "s2", 400, 700, 200, { strokeColor: "#1971c2", backgroundColor: "#d0bfff" });
    const bare = sticky(up, "s3", 700, 700);
    return [
      el(up, "rectangle", "rs", 100, 100, 100, 100, {
        strokeColor: "#e03131",
        backgroundColor: "#ffc9c9",
        strokeWidth: 4,
        strokeStyle: "dashed",
        fillStyle: "cross-hatch",
        opacity: 60,
        roughness: 2,
        roundness: { type: 3 },
      }),
      el(up, "ellipse", "e", 250, 100),
      el(up, "diamond", "d", 400, 100),
      el(up, "rectangle", "rt", 550, 100, 100, 100, { backgroundColor: "transparent", strokeColor: "transparent" }),
      text(up, "t", "hello", 100, 250, { fontSize: 36, fontFamily: 1, textAlign: "center", strokeColor: "#f08c00" }),
      text(up, "t2", "world", 300, 250),
      linear(up, "arrow", "arr", 500, 250, [[0, 0], [150, 50]], { startArrowhead: "circle", endArrowhead: "triangle", strokeColor: "#9c36b5" }),
      linear(up, "arrow", "arr2", 700, 250, [[0, 0], [150, 0]]),
      linear(up, "line", "ln", 900, 250, [[0, 0], [100, 100]], { roundness: { type: 2 } }),
      box,
      label(up, box, "box-label", "label", { fontSize: 28, fontFamily: 6, strokeColor: "#2f9e44" }),
      ell,
      label(up, ell, "ell-label", "ellipse label"),
      up.newFrameElement({ id: "f", x: 700, y: 400, width: 300, height: 200 }),
      note,
      stickyLabel(up, note, "s1-label", "a note"),
      note2,
      stickyLabel(up, note2, "s2-label", "a second note that wraps"),
      bare,
      el(up, "rectangle", "del", 900, 900, 100, 100, { isDeleted: true, strokeColor: "#1971c2" }),
    ];
  },
};

/**
 * A scene built after reseed(1), its labels laid out (redrawTextBoundingBox
 * of each bound text), indexed by upstream's Scene.
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

const tool = (type, lastActiveTool = null) => ({
  type,
  customType: null,
  locked: false,
  fromSelection: false,
  lastActiveTool,
});

/**
 * [id, scene, action, app state keys, { value, props, copyFrom }]; a
 * pasteStyles case copies `copyFrom`'s styles first.
 */
const CASES = [
  // pasteStyles before any copyStyles: copiedStyles is still "{}"
  ["paste-nothing-copied", "styles", "pasteStyles", { selectedElementIds: selecting("e") }],

  // deselect
  ["deselect-selected", "selection", "deselect", { selectedElementIds: selecting("r") }],
  ["deselect-group", "selection", "deselect", { selectedElementIds: selecting("g1a", "g1b"), selectedGroupIds: { g1: true } }],
  ["deselect-tool", "selection", "deselect", { activeTool: tool("rectangle") }],
  ["deselect-lasso", "selection", "deselect", { activeTool: tool("rectangle"), preferredSelectionTool: { type: "lasso", initialized: true } }],
  ["deselect-hand-last", "selection", "deselect", { activeTool: tool("hand", tool("ellipse")) }],
  ["deselect-eraser", "selection", "deselect", { activeTool: tool("eraser") }],
  ["deselect-linear", "selection", "deselect", {
    selectedElementIds: selecting("l"),
    selectedLinearElement: "l",
    showHyperlinkPopup: "info",
    frameToHighlight: null,
  }],
  ["deselect-editing-group-nested", "selection", "deselect", {
    selectedElementIds: selecting("n1"),
    selectedGroupIds: {},
    editingGroupId: "inner",
  }],
  ["deselect-editing-group-outer", "selection", "deselect", {
    selectedElementIds: selecting("n1", "n2"),
    selectedGroupIds: { inner: true },
    editingGroupId: "outer",
  }],
  ["deselect-editing-group-empty", "selection", "deselect", { editingGroupId: "g1" }],

  // flipHorizontal and flipVertical
  ...[
    ["flip-shape", ["ell"]],
    ["flip-two", ["ell", "d"]],
    ["flip-rotated", ["rot"]],
    ["flip-line", ["l"]],
    ["flip-arrow", ["arr"]],
    ["flip-freedraw", ["fd"]],
    ["flip-text", ["t"]],
    ["flip-image", ["img"]],
    ["flip-labelled", ["box"]],
    ["flip-labelled-and-shape", ["box", "ell", "d"]],
    ["flip-arrow-label", ["arrl"]],
    ["flip-group", ["g1a", "g1b"], { selectedGroupIds: { g1: true } }],
    ["flip-frame", ["f"]],
    ["flip-frame-child", ["in1", "in2"]],
    ["flip-bound-arrow-only", ["ab"]],
    ["flip-bound-arrows-only", ["ab", "eb"]],
    ["flip-bound-all", ["a", "b", "ab"]],
    ["flip-bound-elbow", ["a", "b", "eb"]],
    ["flip-bound-shape", ["a"]],
    ["flip-mixed", ["arr", "fd", "t", "rot", "l"]],
    ["flip-nothing", []],
  ].flatMap(([id, ids, keys = {}]) => [
    [`${id}-h`, "flip", "flipHorizontal", { selectedElementIds: selecting(...ids), ...keys }],
    [`${id}-v`, "flip", "flipVertical", { selectedElementIds: selecting(...ids), ...keys }],
  ]),

  // toggleElementLock
  ["lock-single", "lock", "toggleElementLock", { selectedElementIds: selecting("r1") }],
  ["lock-multi", "lock", "toggleElementLock", { selectedElementIds: selecting("r1", "r2", "r3") }],
  ["lock-group", "lock", "toggleElementLock", { selectedElementIds: selecting("g1a", "g1b"), selectedGroupIds: { g1: true } }],
  ["lock-labelled", "lock", "toggleElementLock", { selectedElementIds: selecting("box") }],
  ["lock-frame", "lock", "toggleElementLock", { selectedElementIds: selecting("f") }],
  ["lock-linear", "lock", "toggleElementLock", { selectedElementIds: selecting("l"), selectedLinearElement: "l" }],
  ["lock-mixed", "lock", "toggleElementLock", { selectedElementIds: selecting("r1", "lk3") }],
  ["lock-with-deleted", "lock", "toggleElementLock", { selectedElementIds: selecting("r1", "del") }],
  ["lock-nothing", "lock", "toggleElementLock", {}],
  ["unlock-single", "lock", "toggleElementLock", { selectedElementIds: selecting("lk3") }],
  ["unlock-multi", "lock", "toggleElementLock", {
    selectedElementIds: selecting("lk1", "lk2"),
    selectedGroupIds: { lkg: true },
    lockedMultiSelections: { lkg: true },
  }],
  ["unlock-group", "lock", "toggleElementLock", {
    selectedElementIds: selecting("lg1", "lg2"),
    selectedGroupIds: { lgroup: true },
    lockedMultiSelections: { lkg: true },
  }],
  ["unlock-multi-and-group", "lock", "toggleElementLock", {
    selectedElementIds: selecting("lk1", "lk2", "lg1", "lg2"),
    lockedMultiSelections: { lkg: true },
  }],

  // copyStyles
  ["copy-shape", "styles", "copyStyles", { selectedElementIds: selecting("rs") }],
  ["copy-labelled", "styles", "copyStyles", { selectedElementIds: selecting("box") }],
  ["copy-first-selected", "styles", "copyStyles", { selectedElementIds: selecting("e", "rs") }],
  ["copy-deleted", "styles", "copyStyles", { selectedElementIds: selecting("del") }],
  ["copy-nothing", "styles", "copyStyles", {}],

  // pasteStyles
  ["paste-shape", "styles", "pasteStyles", { selectedElementIds: selecting("e", "d") }, { copyFrom: ["rs"] }],
  ["paste-shape-onto-text", "styles", "pasteStyles", { selectedElementIds: selecting("t2") }, { copyFrom: ["rs"] }],
  ["paste-shape-onto-line", "styles", "pasteStyles", { selectedElementIds: selecting("ln", "arr2") }, { copyFrom: ["rs"] }],
  ["paste-shape-onto-frame", "styles", "pasteStyles", { selectedElementIds: selecting("f") }, { copyFrom: ["rs"] }],
  ["paste-shape-onto-labelled", "styles", "pasteStyles", { selectedElementIds: selecting("ell") }, { copyFrom: ["rs"] }],
  ["paste-shape-onto-sticky", "styles", "pasteStyles", { selectedElementIds: selecting("s1") }, { copyFrom: ["rs"] }],
  ["paste-transparent-onto-sticky", "styles", "pasteStyles", { selectedElementIds: selecting("s1", "s3") }, { copyFrom: ["rt"] }],
  ["paste-text", "styles", "pasteStyles", { selectedElementIds: selecting("t2", "e") }, { copyFrom: ["t"] }],
  ["paste-text-onto-sticky", "styles", "pasteStyles", { selectedElementIds: selecting("s1") }, { copyFrom: ["t"] }],
  ["paste-labelled", "styles", "pasteStyles", { selectedElementIds: selecting("ell", "t2", "e") }, { copyFrom: ["box"] }],
  ["paste-labelled-onto-sticky", "styles", "pasteStyles", { selectedElementIds: selecting("s1") }, { copyFrom: ["box"] }],
  ["paste-sticky", "styles", "pasteStyles", { selectedElementIds: selecting("s1", "e") }, { copyFrom: ["s2"] }],
  ["paste-sticky-onto-labelled", "styles", "pasteStyles", { selectedElementIds: selecting("box") }, { copyFrom: ["s2"] }],
  ["paste-bare-sticky-onto-sticky", "styles", "pasteStyles", { selectedElementIds: selecting("s2") }, { copyFrom: ["s3"] }],
  ["paste-arrow", "styles", "pasteStyles", { selectedElementIds: selecting("arr2", "ln", "e") }, { copyFrom: ["arr"] }],
  ["paste-line", "styles", "pasteStyles", { selectedElementIds: selecting("rs", "arr2") }, { copyFrom: ["ln"] }],
  ["paste-deleted", "styles", "pasteStyles", { selectedElementIds: selecting("e") }, { copyFrom: ["del"] }],
  ["paste-onto-nothing", "styles", "pasteStyles", {}, { copyFrom: ["rs"] }],

  // viewMode
  ["view-mode-on", "selection", "viewMode", {}],
  ["view-mode-off", "selection", "viewMode", { viewModeEnabled: true }],

  // toggleTheme
  ["theme-dark", "selection", "toggleTheme", {}],
  ["theme-light", "selection", "toggleTheme", { theme: "dark" }],
  ["theme-value", "selection", "toggleTheme", { theme: "dark" }, { value: "dark" }],
  ["theme-host", "selection", "toggleTheme", {}, { props: { onThemeChange: true } }],
];

const ACTIONS = (up) => ({
  deselect: up.actionDeselect,
  flipHorizontal: up.actionFlipHorizontal,
  flipVertical: up.actionFlipVertical,
  toggleElementLock: up.actionToggleElementLock,
  copyStyles: up.actionCopyStyles,
  pasteStyles: up.actionPasteStyles,
  viewMode: up.actionToggleViewMode,
  toggleTheme: up.actionToggleTheme,
});

/** The app state keys a case sets, as upstream's perform reads them. */
const caseAppState = (keys) => {
  const out = { selectedElementIds: {}, selectedGroupIds: {}, editingGroupId: null, ...keys };
  if (keys.selectedLinearElement) out.selectedLinearElement = { elementId: keys.selectedLinearElement, isEditing: false };
  return out;
};

const reduceAppStateValue = (key, value) =>
  key === "selectedLinearElement" && value ? { elementId: value.elementId, isEditing: value.isEditing } : value;

/** The keys whose JSON differs from the app state perform was given. */
const changedAppState = (before, after) => {
  const out = {};
  for (const key of Object.keys(after ?? {})) {
    const value = clone(reduceAppStateValue(key, after[key]));
    if (JSON.stringify(value) !== JSON.stringify(clone(reduceAppStateValue(key, before[key])))) {
      out[key] = value;
    }
  }
  return out;
};

const appFor = (scene, appState, props = {}) => ({
  scene,
  state: appState,
  props: {
    ...(props.onThemeChange ? { onThemeChange: () => {} } : {}),
  },
  cursor: { applyForTool: () => {} },
});

const runCase = (up, [id, sceneName, action, keys, { value = null, props = {}, copyFrom = null } = {}]) => {
  const scene = buildScene(up, sceneName);
  const elements = scene.getElementsIncludingDeleted();
  const recordedElements = clone(elements);
  const overrides = caseAppState(keys);
  const appState = { ...up.getDefaultAppState(), ...overrides };
  if (copyFrom) {
    const copyState = { ...appState, selectedElementIds: selecting(...copyFrom) };
    up.reseed(1);
    up.actionCopyStyles.perform(elements, copyState, null, appFor(scene, copyState));
  }
  const copied = up.copiedStyles;
  const app = appFor(scene, appState, props);
  up.reseed(1);
  const result = ACTIONS(up)[action].perform(elements, appState, value ?? undefined, app);
  return {
    id,
    action,
    value,
    props,
    copied,
    elements: recordedElements,
    appState: clone(overrides),
    result:
      result === false
        ? null
        : {
            elements: result.elements ? clone(result.elements) : null,
            appState: changedAppState(appState, result.appState),
            captureUpdate: result.captureUpdate,
            copied: up.copiedStyles,
          },
  };
};

// -- the fixture --------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating shortcut action fixtures");
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
      "The perform of the actions behind the deselect, flip, element lock, copy and paste styles, " +
      "view mode and theme shortcuts (packages/excalidraw/actions/actionDeselect.ts, actionFlip.ts, " +
      "actionElementLock.ts, actionStyles.ts, actionToggleViewMode.tsx, actionCanvas.tsx " +
      "actionToggleTheme) on hand-written scenes, in upstream's test mode (ids id0.., timestamps 1, " +
      "reseed(1) before each perform, text 10 px per code unit). " +
      "Generated by tools/goldens/shortcut-actions-fixtures.mjs.",
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
    process.stderr.write(`shortcut-actions-fixtures: ${error.message}\n`);
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
      process.stderr.write("shortcut action fixture is out of date: run node tools/goldens/shortcut-actions-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`shortcut action fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
