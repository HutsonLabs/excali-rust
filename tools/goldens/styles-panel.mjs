#!/usr/bin/env node
// Styles panel goldens for excali-ui (ex-519, ex-701, ex-702): upstream's own
// getShapeActionPredicates (packages/excalidraw/components/
// shapeActionPredicates.ts), SelectedShapeActions, the full styles panel
// (components/Actions.tsx:63-217), CompactShapeActions, the compact one
// (:219-717), and the form factor rules (common/src/editorInterface.ts),
// run from the pinned checkout under Node.
//
//   node tools/goldens/styles-panel.mjs            write the fixture
//   node tools/goldens/styles-panel.mjs --check    exit 1 if it is stale
//   node tools/goldens/styles-panel.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-ui/tests/fixtures/styles-panel.json:
//
// - `scenes`: element lists built with upstream's newElement functions;
// - `cases`: an active tool, a selection and other app state keys over a
//   scene, the document direction, the predicates upstream answers, and
//   the tree SelectedShapeActions renders for them, as LayerUI renders it
//   (`app.scene.getNonDeletedElementsMap()` as `elementsMap`). The
//   hand-written cases cover every tool and every rule of the predicates;
//   the rest are seeded random selections.
// - `compact` on every case: the tree CompactShapeActions renders for it
//   on a tablet (the case's `openPopup` opens one of its popovers);
// - `mobile` on every hand-written case: the tree MobileShapeActions
//   (:719-876), the phone's styles row, renders for it at the width it
//   measures on its first render (0); `mobileWidths`: the tree for some
//   selections at widths either side of the thresholds where duplicate and
//   delete move out of the "…" popover (its ref answers the width);
// - `compactWrapper`: LayerUI's section and island around the compact
//   panel (LayerUI.tsx:249-275);
// - `formFactor`: the breakpoints, getFormFactor, isMobileBreakpoint and
//   isTabletBreakpoint over a grid of editor sizes, and
//   deriveStylesPanelMode for every form factor and desktop UI mode;
// - `colors` on every case: what the PanelComponents of
//   actionChangeStrokeColor and actionChangeBackgroundColor
//   (actions/actionProperties.tsx:358-551) render in the full panel, the
//   heading and `{ colorPicker }` with the ColorPicker's props (type,
//   label, color, topPicks, customizableTopPicks, excludedColors), as
//   resolveColorTarget (actions/colorTargets.ts) and getFormValue decide
//   them;
// - `locale`: the English strings of the legends and titles
//   (locales/en.json).
//
// The tree: `{ tag, class?, children? }` for a DOM element, a string for
// text, and `{ action }` where the panel calls `renderAction(action)`.
// renderAction is stubbed to render every action it is asked for (its
// own gate, the registered PanelComponent and `UIOptions.canvasActions`,
// is ex-514's ActionManager.can_render), so the tree is the panel's
// structure and the predicates' gates. JSX is evaluated by a stand-in
// runtime (react/jsx-runtime shimmed below) that calls function
// components and flattens fragments; the hooks the compact panel calls
// are inert (its popovers open from `appState.openPopup`). An icons.tsx
// icon is `{ icon: name }`, `renderAction(name, data)` is
// `{ action, data }`, event handlers are left out, and radix's Popover is
// a stand-in: Root, Trigger and Portal render their children in place,
// Content is `{ tag: "popover", class, attrs: { side, align, sideOffset,
// alignOffset }, style }` (PropertiesPopover's placement) and Arrow
// renders nothing.
//
// Deterministic: Math.random throws while generating, elements are built
// with fixed ids after reseed(), `updated` is fixed, and the random cases
// come from a Park-Miller generator.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { installDom } from "./lib/recording-context.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { ORIGIN, RANDOM_SEED } from "./static-scene.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui", "tests", "fixtures");
export const OUT_FILE = "styles-panel.json";

const ENTRY = `
export { SelectedShapeActions, CompactShapeActions, MobileShapeActions } from "./packages/excalidraw/components/Actions";
export * as editorInterface from "./packages/common/src/editorInterface";
export * as icons from "./packages/excalidraw/components/icons";
export { getShapeActionPredicates } from "./packages/excalidraw/components/shapeActionPredicates";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { Section } from "./packages/excalidraw/components/Section";
export { Island } from "./packages/excalidraw/components/Island";
export { getTargetElements } from "./packages/excalidraw/scene";
export { t } from "./packages/excalidraw/i18n";
export { CLASSES } from "./packages/common/src/constants";
export { showSelectedShapeActions } from "./packages/element/src/showSelectedShapeActions";
export { actionChangeStrokeColor, actionChangeBackgroundColor } from "./packages/excalidraw/actions/actionProperties";
export { ColorPicker } from "./packages/excalidraw/components/ColorPicker/ColorPicker";
export { Scene } from "./packages/element/src/Scene";
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
`;

export const CONTAINER_ID = "excali-id";

export const FRAGMENT = Symbol.for("excali-rust.fragment");

// Packages the module graph of Actions.tsx (through actions/index and the
// components it imports) reaches but the full panel never calls.
export const STUBS = [
  "fuzzy",
  "pica",
  "react-dom",
  "browser-fs-access",
  "image-blob-reduce",
];

export const SHIMS = {
  // Section reads the container's id (App.tsx:593-594).
  // The compact panel's PropertiesPopover reads the editor interface.
  "packages/excalidraw/components/App": `module.exports = {
  useExcalidrawContainer: () => ({ id: "${CONTAINER_ID}", container: null }),
  useEditorInterface: () => ({ formFactor: "tablet", desktopUIMode: "full", isTouchScreen: true, isLandscape: true, canFitSidebar: false }),
};`,
  "radix-ui": `const F = Symbol.for("excali-rust.fragment");
const pass = ({ children }) => ({ type: F, props: { children } });
const Content = ({ className, side, align, sideOffset, alignOffset, style, children }) => ({
  type: "popover",
  props: { className, side, align, sideOffset: String(sideOffset), alignOffset: String(alignOffset), style, children },
});
module.exports = { Popover: { Root: pass, Trigger: pass, Portal: pass, Content, Arrow: () => null } };`,
  "react/jsx-runtime": `const F = Symbol.for("excali-rust.fragment");
const jsx = (type, props) => ({ type, props });
module.exports = { jsx, jsxs: jsx, Fragment: F };`,
  react: `const F = Symbol.for("excali-rust.fragment");
const id = (f) => f;
module.exports = {
  Fragment: F,
  createContext: () => ({ Provider: id }),
  forwardRef: id,
  memo: id,
  useCallback: id,
  useContext: () => undefined,
  useEffect: () => {},
  useLayoutEffect: () => {},
  useMemo: (f) => f(),
  // MobileShapeActions measures its row through its only ref (Actions.tsx:745-748)
  useRef: (v) => ({
    current: v === null && globalThis.__mobileActionsWidth !== undefined
      ? { getBoundingClientRect: () => ({ width: globalThis.__mobileActionsWidth }) }
      : v,
  }),
  useState: (v) => [v, () => {}],
};`,
  // clsx 1.1.1's rules: strings and numbers, arrays recursively, the truthy
  // keys of objects, joined by spaces.
  clsx: `const c = (...args) => {
  const out = [];
  for (const x of args) {
    if (!x) continue;
    if (typeof x === "string" || typeof x === "number") out.push(String(x));
    else if (Array.isArray(x)) { const s = c(...x); if (s) out.push(s); }
    else if (typeof x === "object") for (const k of Object.keys(x)) if (x[k]) out.push(k);
  }
  return out.join(" ");
};
module.exports = c; module.exports.default = c; module.exports.clsx = c;`,
  "jotai-scope": `module.exports = { createIsolation: () => ({ Provider() {}, useAtom() { return []; }, useAtomValue() {}, useSetAtom() {}, useStore() {} }) };`,
  jotai: `module.exports = { atom: (init) => ({ init }), createStore: () => ({ get() {}, set() {}, sub() {} }) };`,
};

const usage = () => {
  process.stderr.write("usage: styles-panel.mjs [--check] [--out DIR]\n");
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

/** What JSON.stringify writes, read back. */
export const json = (value) => JSON.parse(JSON.stringify(value));

// -- scenes ---------------------------------------------------------------------------

/** Every element fixed: `created` and `updated` would be Date.now(). */
export const fixed = (element) => ({ ...element, created: 1, updated: 1 });

export const bind = (container, text) => {
  container.boundElements = [...(container.boundElements ?? []), { type: "text", id: text.id }];
};

export const SCENES = {
  main: (up) => {
    const at = (x, y) => ({ x, y, width: 100, height: 60 });
    const r1 = up.newElement({ type: "rectangle", id: "r1", ...at(0, 0), seed: 1 });
    const r2 = up.newElement({ type: "rectangle", id: "r2", ...at(120, 0), seed: 2, backgroundColor: "#ffc9c9", roundness: { type: 3 } });
    const e1 = up.newElement({ type: "ellipse", id: "e1", ...at(240, 0), seed: 3, backgroundColor: "#a5d8ff", fillStyle: "solid" });
    const d1 = up.newElement({ type: "diamond", id: "d1", ...at(360, 0), seed: 4 });
    const l1 = up.newLinearElement({ type: "line", id: "l1", x: 0, y: 100, seed: 5, points: [[0, 0], [80, 20]] });
    const a1 = up.newArrowElement({ type: "arrow", id: "a1", x: 120, y: 100, seed: 6, points: [[0, 0], [80, 20]], elbowed: false });
    const a2 = up.newArrowElement({ type: "arrow", id: "a2", x: 240, y: 100, seed: 7, points: [[0, 0], [80, 40]], elbowed: true });
    const f1 = up.newFreeDrawElement({ type: "freedraw", id: "f1", x: 360, y: 100, seed: 8, points: [[0, 0], [10, 10]], simulatePressure: true });
    const t1 = up.newTextElement({ id: "t1", x: 0, y: 200, text: "free", seed: 9 });
    const c1 = up.newElement({ type: "rectangle", id: "c1", ...at(120, 200), seed: 10, backgroundColor: "#b2f2bb" });
    const ct1 = up.newTextElement({ id: "ct1", x: 130, y: 210, text: "label", seed: 11, containerId: "c1", verticalAlign: "middle", textAlign: "center" });
    bind(c1, ct1);
    const a3 = up.newArrowElement({ type: "arrow", id: "a3", x: 240, y: 200, seed: 12, points: [[0, 0], [100, 0]], elbowed: false });
    const at1 = up.newTextElement({ id: "at1", x: 270, y: 190, text: "arrow", seed: 13, containerId: "a3", verticalAlign: "middle", textAlign: "center" });
    bind(a3, at1);
    const s1 = up.newStickyNoteElement({ type: "stickynote", id: "s1", ...at(360, 200), seed: 14 });
    const st1 = up.newTextElement({ id: "st1", x: 370, y: 210, text: "note", seed: 15, containerId: "s1", verticalAlign: "middle", textAlign: "center" });
    bind(s1, st1);
    const s2 = up.newStickyNoteElement({ type: "stickynote", id: "s2", ...at(480, 200), seed: 16 });
    const i1 = up.newImageElement({ type: "image", id: "i1", ...at(0, 300), seed: 17, status: "saved", fileId: "file1" });
    const i2 = up.newImageElement({ type: "image", id: "i2", ...at(120, 300), seed: 18, status: "saved", fileId: "file2" });
    const fr1 = up.newFrameElement({ id: "fr1", x: 0, y: 400, width: 300, height: 200, seed: 19, name: "Frame" });
    const fc1 = up.newElement({ type: "ellipse", id: "fc1", ...at(20, 420), seed: 20, frameId: "fr1" });
    const mf1 = up.newMagicFrameElement({ id: "mf1", x: 320, y: 400, width: 200, height: 200, seed: 21 });
    const em1 = up.newEmbeddableElement({ type: "embeddable", id: "em1", ...at(0, 620), seed: 22, link: "https://example.com" });
    const if1 = up.newIframeElement({ type: "iframe", id: "if1", ...at(120, 620), seed: 23 });
    const g1a = up.newElement({ type: "rectangle", id: "g1a", ...at(0, 700), seed: 24, groupIds: ["g1"] });
    const g1b = up.newElement({ type: "ellipse", id: "g1b", ...at(120, 700), seed: 25, groupIds: ["g1"] });
    const g2a = up.newElement({ type: "rectangle", id: "g2a", ...at(0, 800), seed: 26, groupIds: ["g2i", "g2o"] });
    const g2b = up.newElement({ type: "diamond", id: "g2b", ...at(120, 800), seed: 27, groupIds: ["g2i", "g2o"] });
    const g2c = up.newElement({ type: "ellipse", id: "g2c", ...at(240, 800), seed: 28, groupIds: ["g2o"] });
    const x1 = { ...up.newElement({ type: "rectangle", id: "x1", ...at(240, 300), seed: 29 }), isDeleted: true };
    const l2 = up.newLinearElement({ type: "line", id: "l2", x: 360, y: 300, seed: 30, points: [[0, 0], [60, 0], [60, 60], [0, 0]], backgroundColor: "#ffec99", polygon: true });
    const a4 = up.newArrowElement({ type: "arrow", id: "a4", x: 480, y: 300, seed: 31, points: [[0, 0], [80, 20]], elbowed: false, roundness: { type: 2 } });
    return [r1, r2, e1, d1, l1, a1, a2, f1, t1, c1, ct1, a3, at1, s1, st1, s2, i1, i2, fr1, fc1, mf1, em1, if1, g1a, g1b, g2a, g2b, g2c, x1, l2, a4];
  },
};

// -- cases ----------------------------------------------------------------------------

export const TOOLS = [
  "selection",
  "lasso",
  "rectangle",
  "diamond",
  "ellipse",
  "arrow",
  "line",
  "freedraw",
  "text",
  "image",
  "eraser",
  "hand",
  "frame",
  "magicframe",
  "stickynote",
  "embeddable",
  "laser",
  "autoshape",
  "bucketfill",
];

export const tool = (type) => (type === "custom" ? { type: "custom", customType: "comment" } : { type, customType: null });

export const selecting = (ids) => Object.fromEntries(ids.map((id) => [id, true]));

/** [id, { tool?, select?, state?, rtl? }] */
export const handCases = () => {
  const cases = [];
  for (const t of [...TOOLS, "custom"]) {
    cases.push([`tool-${t}`, { tool: t }]);
    cases.push([`tool-${t}-background`, { tool: t, state: { currentItemBackgroundColor: "#ffc9c9" } }]);
  }
  const singles = ["r1", "r2", "e1", "d1", "l1", "l2", "a1", "a2", "f1", "t1", "c1", "ct1", "a3", "at1", "s1", "st1", "s2", "i1", "fr1", "fc1", "mf1", "em1", "if1", "x1"];
  for (const id of singles) cases.push([`select-${id}`, { select: [id] }]);
  const groups = [
    ["r1", "r2"],
    ["r1", "e1", "d1"],
    ["r1", "t1"],
    ["l1", "a1"],
    ["a1", "a2"],
    ["f1", "r1"],
    ["f1", "t1", "r2"],
    ["c1", "ct1"],
    ["a3", "at1"],
    ["s1", "st1"],
    ["s1", "s2"],
    ["st1", "t1"],
    ["i1", "i2"],
    ["i1", "r1"],
    ["fr1", "mf1"],
    ["fr1", "fc1"],
    ["fr1", "r1"],
    ["em1", "if1"],
    ["x1", "r1"],
    ["c1", "ct1", "r1"],
    ["r1", "r2", "e1", "d1"],
  ];
  for (const ids of groups) cases.push([`select-${ids.join("-")}`, { select: ids }]);
  // groups: one unit, two units, nested
  cases.push(["group-g1", { select: ["g1a", "g1b"], state: { selectedGroupIds: { g1: true } } }]);
  cases.push(["group-g1-and-r1", { select: ["g1a", "g1b", "r1"], state: { selectedGroupIds: { g1: true } } }]);
  cases.push(["group-g1-and-r1-r2", { select: ["g1a", "g1b", "r1", "r2"], state: { selectedGroupIds: { g1: true } } }]);
  cases.push(["group-g2o", { select: ["g2a", "g2b", "g2c"], state: { selectedGroupIds: { g2o: true } } }]);
  cases.push(["group-g2i", { select: ["g2a", "g2b"], state: { selectedGroupIds: { g2i: true } } }]);
  cases.push(["group-editing-g1", { select: ["g1a"], state: { editingGroupId: "g1" } }]);
  cases.push(["group-g1-unselected-group", { select: ["g1a", "g1b"] }]);
  // tools over selections
  for (const [t, ids] of [
    ["rectangle", ["i1"]],
    ["rectangle", ["i1", "i2"]],
    ["rectangle", ["fr1"]],
    ["arrow", ["fr1", "mf1"]],
    ["text", ["r1"]],
    ["freedraw", ["r1"]],
    ["freedraw", ["f1"]],
    ["autoshape", []],
    ["autoshape", ["r1"]],
    ["autoshape", ["f1"]],
    ["line", ["st1"]],
    ["selection", ["st1"]],
    ["bucketfill", ["r1"]],
    ["stickynote", ["t1"]],
    ["image", ["r1", "r2"]],
    ["hand", ["a1"]],
  ]) {
    cases.push([`tool-${t}-select-${ids.join("-") || "none"}`, { tool: t, select: ids }]);
  }
  // editing, new element, cropping, line editor
  const find = (id) => ({ ref: id });
  cases.push(["editing-t1", { tool: "text", state: { editingTextElement: find("t1") } }]);
  cases.push(["editing-ct1", { select: ["c1"], state: { editingTextElement: find("ct1") } }]);
  cases.push(["editing-st1", { state: { editingTextElement: find("st1") } }]);
  cases.push(["editing-at1", { state: { editingTextElement: find("at1") } }]);
  cases.push(["new-r1", { tool: "rectangle", state: { newElement: find("r1") } }]);
  cases.push(["new-r1-selected", { tool: "rectangle", select: ["r1"], state: { newElement: find("r1") } }]);
  cases.push(["new-f1-freedraw", { tool: "freedraw", state: { newElement: find("f1") } }]);
  cases.push(["new-e1-autoshape", { tool: "autoshape", state: { newElement: find("e1") } }]);
  cases.push(["new-e1-autoshape-selected", { tool: "autoshape", select: ["r1"], state: { newElement: find("e1") } }]);
  cases.push(["cropping-i1", { select: ["i1"], state: { croppingElementId: "i1" } }]);
  cases.push(["cropping-other", { select: ["i1"], state: { croppingElementId: "i2" } }]);
  cases.push(["line-editing-l1", { select: ["l1"], state: { selectedLinearElement: { elementId: "l1", isEditing: true } } }]);
  cases.push(["line-not-editing-l1", { select: ["l1"], state: { selectedLinearElement: { elementId: "l1", isEditing: false } } }]);
  cases.push(["background-transparent-r1", { select: ["r1"], state: { currentItemBackgroundColor: "#ffc9c9" } }]);
  cases.push(["background-rectangle-tool-r1", { tool: "rectangle", select: ["r1"], state: { currentItemBackgroundColor: "transparent" } }]);
  // showSelectedShapeActions
  cases.push(["view-mode-r1", { select: ["r1"], state: { viewModeEnabled: true } }]);
  cases.push(["view-mode-rectangle-tool", { tool: "rectangle", state: { viewModeEnabled: true } }]);
  cases.push(["link-selector-r1", { select: ["r1"], state: { openDialog: { name: "elementLinkSelector", sourceElementId: "r1" } } }]);
  cases.push(["help-dialog-r1", { select: ["r1"], state: { openDialog: { name: "help" } } }]);
  cases.push(["custom-tool-r1", { tool: "custom", select: ["r1"] }]);
  cases.push(["custom-tool-editing-t1", { tool: "custom", state: { editingTextElement: find("t1") } }]);
  cases.push(["selection-editing-t1", { state: { editingTextElement: find("t1") } }]);
  cases.push(["selection-deleted-x1", { select: ["x1"] }]);
  // the compact panel: the arrow type trigger's icon (the selected
  // arrows' common type, else currentItemArrowType), and each popover
  // open over selections and tools
  for (const arrowType of ["sharp", "round", "elbow"]) {
    cases.push([`arrow-type-tool-${arrowType}`, { tool: "arrow", state: { currentItemArrowType: arrowType } }]);
    cases.push([`arrow-type-a1-${arrowType}`, { select: ["a1"], state: { currentItemArrowType: arrowType } }]);
  }
  for (const ids of [["a4"], ["a1", "a4"], ["a2", "a4"], ["a4", "r1"]]) cases.push([`arrow-type-${ids.join("-")}`, { select: ids }]);
  cases.push(["arrow-type-editing-at1", { select: ["a3"], state: { editingTextElement: find("at1"), currentItemArrowType: "elbow" } }]);
  const popups = ["compactStrokeStyles", "compactArrowProperties", "compactTextProperties", "compactOtherProperties"];
  for (const [id, c] of [
    ["r1", { select: ["r1"] }],
    ["a2", { select: ["a2"] }],
    ["t1", { select: ["t1"] }],
    ["c1", { select: ["c1"] }],
    ["f1", { select: ["f1"] }],
    ["l1", { select: ["l1"] }],
    ["i1", { select: ["i1"] }],
    ["r1-r2", { select: ["r1", "r2"] }],
    ["r1-e1-d1", { select: ["r1", "e1", "d1"] }],
    ["l1-a1", { select: ["l1", "a1"] }],
    ["g1", { select: ["g1a", "g1b"], state: { selectedGroupIds: { g1: true } } }],
    ["tool-selection", {}],
    ["tool-rectangle", { tool: "rectangle" }],
    ["tool-arrow", { tool: "arrow" }],
    ["tool-text", { tool: "text" }],
    ["tool-hand", { tool: "hand" }],
    ["editing-t1", { tool: "text", state: { editingTextElement: find("t1") } }],
  ]) {
    for (const popup of popups) cases.push([`popup-${popup}-${id}`, { ...c, state: { ...(c.state ?? {}), openPopup: popup } }]);
  }
  cases.push(["popup-elementStroke-r1", { select: ["r1"], state: { openPopup: "elementStroke" } }]);
  // right to left
  for (const [id, c] of cases.filter(([id]) =>
    ["select-r1-r2", "select-r1-e1-d1", "group-g1-and-r1", "tool-bucketfill", "popup-compactOtherProperties-r1-e1-d1"].includes(id),
  )) {
    cases.push([`${id}-rtl`, { ...c, rtl: true }]);
  }
  return cases;
};

/** Park-Miller (minimal standard) over 1..2^31-2. */
const parkMiller = (seed) => {
  let s = seed;
  return () => {
    s = (s * 16807) % 2147483647;
    return s;
  };
};

const RANDOM_CASES = 160;

const randomCases = (elements) => {
  const next = parkMiller(519);
  const pick = (list) => list[next() % list.length];
  const ids = elements.map((e) => e.id);
  const cases = [];
  for (let i = 0; i < RANDOM_CASES; i++) {
    const count = next() % 5;
    const select = [];
    for (let k = 0; k < count; k++) {
      const id = pick(ids);
      if (!select.includes(id)) select.push(id);
    }
    const state = {};
    if (next() % 3 === 0) state.currentItemBackgroundColor = pick(["transparent", "#ffc9c9", "#00000000"]);
    if (next() % 5 === 0) state.selectedGroupIds = { [pick(["g1", "g2i", "g2o"])]: true };
    if (next() % 9 === 0) state.editingTextElement = { ref: pick(["t1", "ct1", "at1", "st1"]) };
    if (next() % 9 === 0) state.newElement = { ref: pick(ids) };
    if (next() % 9 === 0) state.croppingElementId = pick(["i1", "i2"]);
    if (next() % 9 === 0) state.selectedLinearElement = { elementId: pick(["l1", "a1", "a2"]), isEditing: next() % 2 === 0 };
    cases.push([`random-${i}`, { tool: pick([...TOOLS, "custom"]), select, state, rtl: next() % 7 === 0 }]);
  }
  return cases;
};

// -- running upstream -----------------------------------------------------------------

/** icons.tsx's icons (JSX from the stand-in runtime) → export name; the
 * first export names an alias. */
const iconNames = new Map();

/** The ColorPicker component (`memo` is the identity here). */
let colorPicker = null;

/** The rendered JSX as a plain tree (see the header). */
const flatten = (node) => {
  if (node === null || node === undefined || node === false || node === true) return [];
  if (Array.isArray(node)) return node.flatMap(flatten);
  if (typeof node === "string" || typeof node === "number") return [String(node)];
  if (typeof node.action === "string") return [node.data ? { action: node.action, data: node.data } : { action: node.action }];
  if (iconNames.has(node)) return [{ icon: iconNames.get(node) }];
  const { type, props } = node;
  if (type === FRAGMENT) return flatten(props.children);
  if (type === colorPicker) {
    const { type: pickerType, label, color, topPicks, customizableTopPicks, excludedColors } = props;
    return [{ colorPicker: { type: pickerType, label, color, topPicks, customizableTopPicks, excludedColors: excludedColors ?? null } }];
  }
  if (typeof type === "function") return flatten(type(props));
  if (typeof type !== "string") throw new Error(`unexpected element type ${String(type)}`);
  const out = { tag: type };
  if (props.className) out.class = props.className;
  const attrs = Object.entries(props).filter(
    ([k, v]) => !["children", "className", "style"].includes(k) && v !== undefined && typeof v !== "function",
  );
  for (const [k, v] of attrs) if (typeof v !== "string") throw new Error(`<${type}> ${k} is not a string`);
  if (attrs.length) out.attrs = Object.fromEntries(attrs);
  // React writes no declaration for an undefined value (Island's
  // `--padding` without a padding prop)
  if (props.style) out.style = Object.fromEntries(Object.entries(props.style).filter(([, v]) => v !== undefined));
  const children = flatten(props.children);
  if (children.length) out.children = children;
  return [out];
};

const PREDICATE_KEYS = [
  "hasSelection",
  "showExtraActions",
  "strokeColor",
  "backgroundColor",
  "fill",
  "strokeWidth",
  "freedrawMode",
  "strokeStyle",
  "sloppiness",
  "roundness",
  "arrowType",
  "arrowheads",
  "text",
  "textAlign",
  "verticalAlign",
  "opacity",
  "layers",
  "align",
  "distribute",
  "link",
  "linkSingleOnly",
  "cropEditor",
  "lineEditor",
];

/** MobileShapeActions' tree at a measured `width`. */
const mobileTree = (up, appState, elementsMap, app, width) => {
  globalThis.__mobileActionsWidth = width;
  try {
    return flatten(
      up.MobileShapeActions({
        appState,
        elementsMap,
        renderAction: (action, data) => ({ action, data }),
        app,
        setAppState: () => {},
      }),
    );
  } finally {
    delete globalThis.__mobileActionsWidth;
  }
};

/** MobileShapeActions' thresholds (MIN_WIDTH 336, +38 for delete, +76
 * for duplicate), either side, 0 and the widest bar's row. */
const MOBILE_WIDTHS = [0, 373, 374, 411, 412, 442];

const MOBILE_WIDTH_CASES = ["select-r1", "select-t1", "select-a1", "tool-rectangle", "select-r1-e1-d1"];

const runCase = (up, window, sceneName, elements, [id, c], mobile = false) => {
  const byId = new Map(elements.map((e) => [e.id, e]));
  const resolveRefs = (state) =>
    Object.fromEntries(Object.entries(state).map(([k, v]) => [k, v && typeof v === "object" && "ref" in v ? byId.get(v.ref) : v]));
  const patch = {
    activeTool: { ...up.getDefaultAppState().activeTool, ...tool(c.tool ?? "selection") },
    selectedElementIds: selecting(c.select ?? []),
    ...(c.state ?? {}),
  };
  const appState = { ...up.getDefaultAppState(), width: 1440, height: 900, ...resolveRefs(patch) };
  // skipValidation: the fractional index check only logs, and its 60 s
  // throttle would keep the process alive (Scene.ts:67-80, 271-283).
  const scene = new up.Scene(elements, { skipValidation: true });
  const app = { scene, state: appState };
  const elementsMap = scene.getNonDeletedElementsMap();
  window.document.documentElement.setAttribute("dir", c.rtl ? "rtl" : "ltr");
  const targets = up.getTargetElements(elementsMap, appState);
  const predicates = up.getShapeActionPredicates(appState, targets, elementsMap, app);
  const keys = Object.keys(predicates);
  if (keys.join() !== PREDICATE_KEYS.join()) throw new Error(`getShapeActionPredicates keys changed: ${keys.join()}`);
  const show = up.showSelectedShapeActions(appState, scene.getNonDeletedElements());
  const tree = flatten(
    up.SelectedShapeActions({ appState, elementsMap, renderAction: (action) => ({ action }), app }),
  );
  const compact = flatten(
    up.CompactShapeActions({
      appState,
      elementsMap,
      renderAction: (action, data) => ({ action, data }),
      app,
      setAppState: () => {},
    }),
  );
  // the two colour actions' PanelComponents as the full panel renders them
  // (renderAction passes the elements including deleted ones,
  // manager.tsx:222-224; getStylesPanelInfo reads a desktop in full mode)
  const panelApp = { ...app, editorInterface: { formFactor: "desktop", desktopUIMode: "full" } };
  // getSelectedElements logs its invariant for a selected deleted element
  // (the x1 cases); the panel renders as it does without it
  const error = console.error;
  console.error = (...args) => {
    if (!String(args[0]).startsWith("[NONDELETED][INVARIANT]")) error(...args);
  };
  let colors;
  try {
    colors = Object.fromEntries(
      [up.actionChangeStrokeColor, up.actionChangeBackgroundColor].map((action) => [
        action.name,
        flatten(action.PanelComponent({ elements, appState, updateData: () => {}, app: panelApp, data: undefined })),
      ]),
    );
  } finally {
    console.error = error;
  }
  const mobileTrees = mobile ? { mobile: mobileTree(up, appState, elementsMap, app, 0) } : {};
  const widths =
    mobile && MOBILE_WIDTH_CASES.includes(id)
      ? { mobileWidths: MOBILE_WIDTHS.map((width) => ({ width, tree: mobileTree(up, appState, elementsMap, app, width) })) }
      : {};
  window.document.documentElement.removeAttribute("dir");
  const statePatch = json(patch);
  return {
    id,
    scene: sceneName,
    rtl: Boolean(c.rtl),
    appState: statePatch,
    show,
    predicates: json(predicates),
    tree,
    compact,
    colors,
    ...mobileTrees,
    ...widths,
  };
};

/** LayerUI's renderSelectedShapeActions around the full panel
 * (LayerUI.tsx:249-297): the Section and the Island with its max height;
 * `{ slot: "panel" }` stands for SelectedShapeActions. */
const wrapperCase = (up, [height, zenModeEnabled]) => {
  const appState = { ...up.getDefaultAppState(), width: 1440, height, zenModeEnabled };
  const tree = flatten(
    up.Section({
      heading: "selectedShapeActions",
      className: ["selected-shape-actions zen-mode-transition", appState.zenModeEnabled ? "transition-left" : ""].filter(Boolean).join(" "),
      children: up.Island({
        className: up.CLASSES.SHAPE_ACTIONS_MENU,
        padding: 2,
        style: { maxHeight: `${appState.height - 166}px` },
        "data-viewport-ui": "side",
        "data-viewport-ui-name": "stylesPanel",
        children: { action: "<panel>" },
      }),
    }),
  );
  return { height, zenModeEnabled, tree };
};

/** LayerUI's renderSelectedShapeActions around the compact panel
 * (LayerUI.tsx:249-275). */
const compactWrapperCase = (up, [height, zenModeEnabled]) => {
  const tree = flatten(
    up.Section({
      heading: "selectedShapeActions",
      className: ["selected-shape-actions zen-mode-transition", zenModeEnabled ? "transition-left" : ""].filter(Boolean).join(" "),
      children: up.Island({
        className: "compact-shape-actions-island",
        padding: 0,
        "data-viewport-ui": "side",
        "data-viewport-ui-name": "stylesPanel",
        style: { maxHeight: `${height - 166}px` },
        children: { action: "<panel>" },
      }),
    }),
  );
  return { height, zenModeEnabled, tree };
};

/** Editor sides for the form factor grid: each breakpoint, either side
 * of it, and common screens. */
const SIDES = [0, 320, 375, 390, 499, 499.5, 500, 568, 599, 599.5, 600, 601, 667, 744, 768, 820, 834, 844, 999, 1000, 1024, 1112, 1180, 1180.5, 1181, 1229, 1366, 1440, 1920];

const formFactorFixture = (up) => {
  const e = up.editorInterface;
  const sizes = SIDES.flatMap((width) => SIDES.map((height) => ({
    width,
    height,
    formFactor: e.getFormFactor(width, height),
    mobile: e.isMobileBreakpoint(width, height),
    tablet: e.isTabletBreakpoint(width, height),
  })));
  const modes = ["phone", "tablet", "desktop"].flatMap((formFactor) =>
    ["compact", "full"].map((desktopUIMode) => ({
      formFactor,
      desktopUIMode,
      mode: e.deriveStylesPanelMode({ formFactor, desktopUIMode }),
    })),
  );
  const constants = Object.fromEntries(
    ["MQ_MAX_MOBILE", "MQ_MAX_WIDTH_LANDSCAPE", "MQ_MAX_HEIGHT_LANDSCAPE", "MQ_MIN_TABLET", "MQ_MAX_TABLET", "MQ_MIN_WIDTH_DESKTOP", "MQ_RIGHT_SIDEBAR_MIN_WIDTH"].map((k) => [k, e[k]]),
  );
  return { constants, sizes, modes };
};

const build = async (upstream) => {
  const window = installDom(ORIGIN);
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    define: {
      "import.meta.env.MODE": '"production"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  colorPicker = up.ColorPicker;
  iconNames.clear();
  for (const [name, value] of Object.entries(up.icons)) {
    if (value && typeof value === "object" && value.type === "svg" && !iconNames.has(value)) iconNames.set(value, name);
  }
  const scenes = {};
  const cases = [];
  for (const [name, make] of Object.entries(SCENES)) {
    up.reseed(RANDOM_SEED);
    const elements = make(up).map(fixed);
    scenes[name] = json(elements);
    for (const c of handCases()) cases.push(runCase(up, window, name, elements, c, true));
    for (const c of randomCases(elements)) cases.push(runCase(up, window, name, elements, c));
  }
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case ${c.id}`);
    ids.add(c.id);
  }
  window.close();
  return format({
    description:
      "Upstream getShapeActionPredicates (packages/excalidraw/components/shapeActionPredicates.ts), the full styles panel SelectedShapeActions (components/Actions.tsx:63-217), the compact one CompactShapeActions (:219-717), the phone's MobileShapeActions (:719-876) and the form factor rules (common/src/editorInterface.ts) at the pinned commit (tools/goldens/styles-panel.mjs): per case an active tool, a selection and app state keys over a scene of upstream-built elements, the document direction, the predicates, and the rendered full, compact and (hand-written cases) mobile trees ({tag, class, children}, text, {icon} for an icons.tsx icon, {tag: popover} for a radix Popover.Content, and {action, data?} where renderAction is called, stubbed to render every action).",
    upstream: upstream.commit,
    containerId: CONTAINER_ID,
    locale: Object.fromEntries(
      ["labels.layers", "labels.align", "labels.actions", "headings.selectedShapeActions", "labels.stroke", "labels.arrowtypes", "labels.textAlign", "labels.textColor", "labels.background"].map((k) => [k, up.t(k)]),
    ),
    wrapper: [[900, false], [768, true], [600.5, false]].map((c) => wrapperCase(up, c)),
    compactWrapper: [[900, false], [768, true], [600.5, false]].map((c) => compactWrapperCase(up, c)),
    formFactor: formFactorFixture(up),
    scenes,
    cases,
  });
};

/** Runs fn with Math.random disabled. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating styles-panel goldens");
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
    process.stderr.write(`styles-panel: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("styles-panel goldens are out of date: run node tools/goldens/styles-panel.mjs\n");
      process.exit(1);
    }
    process.stdout.write("styles-panel goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();
