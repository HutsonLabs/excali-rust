#!/usr/bin/env node
// Action panel goldens for excali-ui (ex-540): what the PanelComponent of
// every action the styles panels render through `renderAction` renders
// (packages/excalidraw/actions/*.tsx, components/RadioSelection.tsx,
// RadioButton.tsx, IconButton.tsx, Range.tsx, IconPicker.tsx), and what
// its handlers pass to `updateData`, run from the pinned checkout under
// Node over the styles panel's scenes and cases (styles-panel.mjs) and
// more (zigzag fills, legacy roundness, polygons, arrowheads, sticky
// notes, links, history).
//
//   node tools/goldens/action-panels.mjs            write the fixture
//   node tools/goldens/action-panels.mjs --check    exit 1 if it is stale
//   node tools/goldens/action-panels.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-ui/tests/fixtures/action-panels.json:
//
// - `columns`: the rendered actions, `name` or `name+cycle` for
//   `renderAction(name, { cycle: true })`;
// - `modes`: the styles panel modes each case is rendered in, with the
//   editor interface that derives them (full: a desktop, compact: a
//   tablet, mobile: a phone);
// - `trees`: the distinct trees; a case's `panels[mode][i]` is the index
//   of what `columns[i]`'s PanelComponent renders;
// - `scenes`: element lists built with upstream's newElement functions
//   (`main` is styles-panel.mjs's);
// - `cases`: the app state keys over a scene, the language's direction,
//   the IconPicker state (`iconPicker`: the open picker's label and the
//   "more options" atom), the undo and redo stacks' emptiness, and the
//   trees per mode;
// - `flippedIcons`: the markup React renders for the arrowhead icons
//   with `flip` (icons.json has them without).
//
// A tree is a list of nodes: `{ tag, class?, attrs?, style?, children?,
// on? }` for a DOM element (React's props: a boolean `aria-*` attribute
// as its string, any other boolean attribute present and empty when true
// and absent otherwise; `style` with React's names), a string for text,
// `{ icon, theme?, flip? }` for an icons.tsx icon (with the props of a
// themed or flippable one), `{ action, data? }` where the component calls
// `renderAction`, `{ colorPicker }` and `{ fontPicker }` for the stateful
// pickers the host renders (their props), and `{ tag: "popover" }` for
// radix's Popover.Content (IconPicker's picker: its attributes include
// the placement props). Popover.Trigger renders radix's button with
// `aria-haspopup`, `aria-expanded` and `data-state`.
//
// `on` holds what each handler does, run with a stand-in event: `click`
// (and `altClick` with `altKey`, when it differs), `change` (the input's
// value "30") and `keydown` per key (`Shift+Tab` etc.), each a list of
// `{ update: value }` (updateData's argument; "[event]" when it is the
// event itself) and `{ iconPicker: { open, more } }` (IconPicker's state
// after its setActive or the "more options" atom's setter: the open
// picker's label or null, and the atom), and for a key whether it was
// `prevented` and its propagation `stopped`. Effects run after a render and the
// component renders again while they change state (IconPicker opens its
// hidden section when the value is in it).
//
// JSX is evaluated by the same stand-in runtime as styles-panel.mjs; hooks
// are inert except as above. Deterministic: Math.random throws while
// generating and elements are built with fixed ids after reseed().

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { installDom } from "./lib/recording-context.mjs";
import { loadUpstream, verifyUpstream } from "./lib/upstream.mjs";
import { ORIGIN, RANDOM_SEED } from "./static-scene.mjs";
import {
  CONTAINER_ID,
  FRAGMENT,
  OUT_DIR,
  SCENES,
  SHIMS,
  STUBS,
  bind,
  fixed,
  handCases,
  json,
  selecting,
  tool,
} from "./styles-panel.mjs";

export const OUT_FILE = "action-panels.json";

const ENTRY = `
import "./packages/excalidraw/actions";
export { actions } from "./packages/excalidraw/actions/register";
export { createUndoAction, createRedoAction } from "./packages/excalidraw/actions/actionHistory";
export * as icons from "./packages/excalidraw/components/icons";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { ColorPicker } from "./packages/excalidraw/components/ColorPicker/ColorPicker";
export { FontPicker } from "./packages/excalidraw/components/FontPicker/FontPicker";
export { AppBucketFill } from "./packages/excalidraw/components/App.bucketFill";
export { __setLanguage } from "./packages/excalidraw/i18n";
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

// React itself, to render the flipped arrowhead icons as icons.mjs renders
// the rest.
const ICON_ENTRY = `
export * as icons from "./packages/excalidraw/components/icons";
export { renderToStaticMarkup } from "react-dom/server.browser";
export { default as React } from "react";
`;

const replaceOnce = (source, from, to) => {
  const at = source.indexOf(from);
  if (at < 0 || source.indexOf(from, at + 1) >= 0) {
    throw new Error(`expected exactly one ${JSON.stringify(from)}`);
  }
  return source.slice(0, at) + to + source.slice(at + from.length);
};

const PANEL_SHIMS = {
  ...SHIMS,
  // useStylesPanelMode (App.tsx:591-592) derives the mode being rendered
  "packages/excalidraw/components/App": `module.exports = {
  useExcalidrawContainer: () => ({ id: "${CONTAINER_ID}", container: null }),
  useEditorInterface: () => globalThis.__editorInterface,
  useStylesPanelMode: () => globalThis.__stylesPanelMode,
};`,
  // radix's Popover: Root passes its open state to the Trigger, which
  // renders radix's button; Content is a `popover` node (see the header)
  "radix-ui": `const F = Symbol.for("excali-rust.fragment");
const pass = ({ children }) => ({ type: F, props: { children } });
const Root = ({ open, children }) => { globalThis.__popoverOpen = !!open; return { type: F, props: { children } }; };
const Trigger = ({ children, ...props }) => ({
  type: "button",
  props: {
    type: "button",
    "aria-haspopup": "dialog",
    "aria-expanded": globalThis.__popoverOpen,
    "data-state": globalThis.__popoverOpen ? "open" : "closed",
    ...props,
    children,
  },
});
const Content = ({ className, side, align, sideOffset, alignOffset, style, children, collisionBoundary, ...rest }) => ({
  type: "popover",
  props: { className, ...rest, side, align, sideOffset: String(sideOffset), alignOffset: String(alignOffset), style, children },
});
module.exports = { Popover: { Root, Trigger, Portal: pass, Content, Arrow: () => null } };`,
  // effects are collected and run after the render
  react: replaceOnce(
    replaceOnce(SHIMS.react, "useEffect: () => {},", "useEffect: (f) => { globalThis.__effects.push(f); },"),
    "useLayoutEffect: () => {},",
    "useLayoutEffect: () => {},\n  useImperativeHandle: () => {},",
  ),
};

// IconPicker's state: `isActive` (React state) and the "more options"
// atom, read from and recorded by the harness.
const PATCH = {
  "packages/excalidraw/components/IconPicker": (source) =>
    replaceOnce(
      replaceOnce(
        source,
        "const [showMoreOptions, setShowMoreOptions] = useAtom(moreOptionsAtom);",
        "const [showMoreOptions, setShowMoreOptions] = globalThis.__moreOptions();",
      ),
      "const [isActive, setActive] = React.useState(false);",
      "const [isActive, setActive] = globalThis.__iconPickerActive(label);",
    ),
  "packages/excalidraw/i18n": (source) => `${source}\nexport const __setLanguage = (lang: any) => { currentLang = lang; };\n`,
};

const usage = () => {
  process.stderr.write("usage: action-panels.mjs [--check] [--out DIR]\n");
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

// -- columns and modes ----------------------------------------------------------------

const COLUMNS = [
  "changeFillStyle",
  "changeStrokeWidth",
  "changeSloppiness",
  "changeFreedrawMode",
  "changeFreedrawMode+cycle",
  "changeStrokeStyle",
  "changeOpacity",
  "changeFontSize",
  "changeFontFamily",
  "changeTextAlign",
  "changeVerticalAlign",
  "changeRoundness",
  "togglePolygon",
  "changeArrowhead",
  "changeArrowProperties",
  "changeArrowType",
  "changeBucketFillBackgroundColor",
  "toggleLinearEditor",
  "alignLeft",
  "alignHorizontallyCentered",
  "alignRight",
  "alignTop",
  "alignVerticallyCentered",
  "alignBottom",
  "distributeHorizontally",
  "distributeVertically",
  "sendToBack",
  "sendBackward",
  "bringForward",
  "bringToFront",
  "group",
  "ungroup",
  "deleteSelectedElements",
  "duplicateSelection",
  "hyperlink",
  "cropEditor",
  "undo",
  "redo",
];

const MODES = {
  full: { formFactor: "desktop", desktopUIMode: "full", isTouchScreen: false, isLandscape: true, canFitSidebar: true },
  compact: { formFactor: "tablet", desktopUIMode: "full", isTouchScreen: true, isLandscape: true, canFitSidebar: false },
  mobile: { formFactor: "phone", desktopUIMode: "full", isTouchScreen: true, isLandscape: false, canFitSidebar: false },
};

// -- the extra scene ------------------------------------------------------------------

const EXTRA = (up) => {
  const at = (x, y) => ({ x, y, width: 100, height: 60 });
  const em2 = up.newEmbeddableElement({ type: "embeddable", id: "em2", ...at(0, 0), seed: 1, link: "https://example.com/embed" });
  const z1 = up.newElement({ type: "rectangle", id: "z1", ...at(120, 0), seed: 2, fillStyle: "zigzag", backgroundColor: "#ffc9c9" });
  const z2 = up.newElement({ type: "ellipse", id: "z2", ...at(240, 0), seed: 3, fillStyle: "zigzag", backgroundColor: "#a5d8ff" });
  const h1 = up.newElement({ type: "rectangle", id: "h1", ...at(360, 0), seed: 4, fillStyle: "hachure", backgroundColor: "#b2f2bb" });
  const ch1 = up.newElement({ type: "diamond", id: "ch1", ...at(480, 0), seed: 5, fillStyle: "cross-hatch" });
  const lr1 = up.newElement({ type: "rectangle", id: "lr1", ...at(0, 100), seed: 6, roundness: { type: 1 } });
  const lr2 = up.newElement({ type: "diamond", id: "lr2", ...at(120, 100), seed: 7, roundness: { type: 2 } });
  const p1 = up.newLinearElement({ type: "line", id: "p1", x: 240, y: 100, seed: 8, points: [[0, 0], [60, 0], [60, 60], [0, 0]], polygon: true, backgroundColor: "#ffec99" });
  const p2 = up.newLinearElement({ type: "line", id: "p2", x: 320, y: 100, seed: 9, points: [[0, 0], [40, 10], [20, 50], [0, 0]], polygon: true });
  const p3 = up.newLinearElement({ type: "line", id: "p3", x: 400, y: 100, seed: 10, points: [[0, 0], [60, 0], [60, 60], [0, 0]] });
  const p4 = up.newLinearElement({ type: "line", id: "p4", x: 480, y: 100, seed: 11, points: [[0, 0], [40, 40]], polygon: true });
  const ar1 = up.newArrowElement({ type: "arrow", id: "ar1", x: 0, y: 200, seed: 12, points: [[0, 0], [80, 0]], startArrowhead: "circle", endArrowhead: "cardinality_one" });
  const ar2 = up.newArrowElement({ type: "arrow", id: "ar2", x: 120, y: 200, seed: 13, points: [[0, 0], [80, 0]], startArrowhead: null, endArrowhead: "bar" });
  const ar3 = up.newArrowElement({ type: "arrow", id: "ar3", x: 240, y: 200, seed: 14, points: [[0, 0], [80, 0]], startArrowhead: "triangle_outline", endArrowhead: "diamond", roundness: { type: 2 } });
  const ar4 = up.newArrowElement({ type: "arrow", id: "ar4", x: 360, y: 200, seed: 15, points: [[0, 0], [80, 40]], elbowed: true, endArrowhead: "arrow" });
  const ar5 = up.newArrowElement({ type: "arrow", id: "ar5", x: 480, y: 200, seed: 16, points: [[0, 0], [80, 0]], startArrowhead: "cardinality_zero_or_many", endArrowhead: "cardinality_one" });
  const lk1 = up.newElement({ type: "rectangle", id: "lk1", ...at(0, 300), seed: 17, link: "https://example.com" });
  const tx1 = { ...up.newTextElement({ id: "tx1", x: 120, y: 300, text: "small", seed: 18, fontSize: 16, textAlign: "left" }), roughness: 0 };
  const tx2 = up.newTextElement({ id: "tx2", x: 240, y: 300, text: "huge", seed: 19, fontSize: 36, textAlign: "right", fontFamily: 6 });
  const tx3 = up.newTextElement({ id: "tx3", x: 360, y: 300, text: "large", seed: 20, fontSize: 28, textAlign: "center", fontFamily: 8 });
  const sn1 = up.newStickyNoteElement({ type: "stickynote", id: "sn1", ...at(0, 400), seed: 21 });
  const snt1 = { ...up.newTextElement({ id: "snt1", x: 10, y: 410, text: "note", seed: 22, fontSize: 14, containerId: "sn1", verticalAlign: "top", textAlign: "left" }), baseFontSize: 28 };
  bind(sn1, snt1);
  const c2 = up.newElement({ type: "rectangle", id: "c2", ...at(120, 400), seed: 23 });
  const ct2 = up.newTextElement({ id: "ct2", x: 130, y: 410, text: "top", seed: 24, containerId: "c2", verticalAlign: "top", textAlign: "right" });
  bind(c2, ct2);
  const fd1 = up.newFreeDrawElement({ type: "freedraw", id: "fd1", x: 240, y: 400, seed: 25, points: [[0, 0], [10, 10]], simulatePressure: true, strokeOptions: { variability: "constant", streamline: 0.5 } });
  const fd2 = up.newFreeDrawElement({ type: "freedraw", id: "fd2", x: 360, y: 400, seed: 26, points: [[0, 0], [10, 10]], simulatePressure: true });
  const o1 = up.newElement({ type: "rectangle", id: "o1", ...at(0, 500), seed: 27, opacity: 0 });
  const o2 = up.newElement({ type: "rectangle", id: "o2", ...at(120, 500), seed: 28, opacity: 50, roughness: 0, strokeStyle: "dashed", strokeWidth: 4 });
  const o3 = up.newElement({ type: "ellipse", id: "o3", ...at(240, 500), seed: 29, opacity: 50, roughness: 2, strokeStyle: "dotted", strokeWidth: 1 });
  const img1 = up.newImageElement({ type: "image", id: "img1", ...at(360, 500), seed: 30, status: "saved", fileId: "file1" });
  return [em2, z1, z2, h1, ch1, lr1, lr2, p1, p2, p3, p4, ar1, ar2, ar3, ar4, ar5, lk1, tx1, tx2, tx3, sn1, snt1, c2, ct2, fd1, fd2, o1, o2, o3, img1];
};

/** [id, { tool?, select?, state?, rtl?, iconPicker?, history? }] */
const extraCases = (elements) => {
  const cases = [];
  const find = (id) => ({ ref: id });
  for (const e of elements) cases.push([`x-select-${e.id}`, { select: [e.id] }]);
  for (const ids of [
    ["z1", "z2"],
    ["z1", "h1"],
    ["z1", "tx1"],
    ["h1", "ch1"],
    ["lr1", "lr2"],
    ["lr1", "p1"],
    ["p1", "p2"],
    ["p1", "p3"],
    ["p1", "p4"],
    ["ar1", "ar2"],
    ["ar1", "ar5"],
    ["ar3", "ar4"],
    ["ar1", "p3"],
    ["tx1", "tx2"],
    ["tx2", "tx3", "c2"],
    ["sn1", "snt1"],
    ["sn1", "c2"],
    ["fd1", "fd2"],
    ["fd1", "o2"],
    ["o1", "o2"],
    ["o2", "o3"],
    ["lk1", "z1"],
    ["img1", "lk1"],
    ["em2", "lk1"],
  ]) {
    cases.push([`x-select-${ids.join("-")}`, { select: ids }]);
  }
  cases.push(["x-editing-tx1", { tool: "text", state: { editingTextElement: find("tx1"), currentItemRoughness: 2, currentItemOpacity: 40 } }]);
  cases.push(["x-editing-snt1", { state: { editingTextElement: find("snt1") } }]);
  cases.push(["x-editing-ct2", { select: ["c2"], state: { editingTextElement: find("ct2") } }]);
  for (const [id, state] of [
    ["fill-zigzag", { currentItemFillStyle: "zigzag" }],
    ["fill-cross-hatch", { currentItemFillStyle: "cross-hatch" }],
    ["roundness-round", { currentItemRoundness: "round" }],
    ["variability-constant", { currentItemStrokeVariability: "constant" }],
    ["font", { currentItemFontFamily: 6, currentItemFontSize: 36, currentItemTextAlign: "right" }],
    ["font-unset", { currentItemFontFamily: 0, currentItemFontSize: 0 }],
    ["arrowheads", { currentItemStartArrowhead: "circle", currentItemEndArrowhead: null, currentItemArrowType: "elbow" }],
    ["opacity-0", { currentItemOpacity: 0 }],
    ["opacity-40", { currentItemOpacity: 40, currentItemRoughness: 0, currentItemStrokeStyle: "dotted", currentItemStrokeWidthKey: "bold" }],
    ["hovered-font", { currentHoveredFontFamily: 8, fontTopPicks: [8, 6] }],
    ["bucket-transparent", { currentItemBackgroundColor: "transparent" }],
    ["bucket-color", { currentItemBackgroundColor: "#ffc9c9" }],
  ]) {
    cases.push([`x-state-${id}`, { tool: "selection", state }]);
    cases.push([`x-state-${id}-tool`, { tool: id.startsWith("bucket") ? "bucketfill" : "arrow", state }]);
  }
  // themes
  for (const ids of [["tx1"], ["c2"], [], ["lr1", "z1"]]) {
    cases.push([`x-dark-${ids.join("-") || "none"}`, { select: ids, state: { theme: "dark" } }]);
  }
  cases.push(["x-dark-group", { select: ["z1", "z2"], state: { theme: "dark", selectedGroupIds: {} } }]);
  // right to left
  for (const ids of [["ar1"], ["ar2", "ar3"], []]) {
    cases.push([`x-rtl-${ids.join("-") || "none"}`, { select: ids, rtl: true }]);
  }
  // IconPicker open, the "more options" atom either way
  for (const [id, ids, open, more, rtl] of [
    ["start-ar1", ["ar1"], "arrowhead_start", false, false],
    ["end-ar1", ["ar1"], "arrowhead_end", false, false],
    ["start-ar2", ["ar2"], "arrowhead_start", false, false],
    ["end-ar2", ["ar2"], "arrowhead_end", false, false],
    ["start-ar3", ["ar3"], "arrowhead_start", false, false],
    ["start-ar3-more", ["ar3"], "arrowhead_start", true, false],
    ["end-ar3-more", ["ar3"], "arrowhead_end", true, false],
    ["start-mixed", ["ar1", "ar2"], "arrowhead_start", false, false],
    ["end-ar5", ["ar5"], "arrowhead_end", false, false],
    ["start-ar5-rtl", ["ar5"], "arrowhead_start", false, true],
    ["end-ar4-rtl", ["ar4"], "arrowhead_end", true, true],
    ["start-tool", [], "arrowhead_start", false, false],
  ]) {
    cases.push([`x-picker-${id}`, { select: ids, rtl, iconPicker: { open, more } }]);
  }
  // history
  cases.push(["x-history-undo", { select: ["z1"], history: { undoEmpty: false, redoEmpty: true } }]);
  cases.push(["x-history-both", { history: { undoEmpty: false, redoEmpty: false } }]);
  // the "…" popover open (duplicate and delete lose the mobile background)
  cases.push(["x-popup-other-z1", { select: ["z1"], state: { openPopup: "compactOtherProperties" } }]);
  cases.push(["x-popup-text-tx1", { select: ["tx1"], state: { openPopup: "compactTextProperties" } }]);
  // groups and the linear editor
  cases.push(["x-editing-group", { select: ["z1", "z2"], state: { editingGroupId: "gx" } }]);
  cases.push(["x-line-editing-p3", { select: ["p3"], state: { selectedLinearElement: { elementId: "p3", isEditing: true } } }]);
  cases.push(["x-deleted-selection", { select: ["gone"] }]);
  return cases;
};

// -- running upstream -----------------------------------------------------------------

/** icons.tsx's icons → export name (elements and memo components). */
const iconNames = new Map();

/** The flippable icons rendered with `flip`. */
const flipped = new Set();

let colorPicker = null;
let fontPicker = null;

/** The handler recorder, while a handler runs. */
let recorder = null;

/** The event a handler is running with. */
let currentEvent = null;

/** IconPicker's state for the render: the open picker and the atom. */
const picker = { open: null, more: false, pendingMore: undefined };

const BOOLEAN_ATTRIBUTES = new Set(["disabled", "hidden", "checked"]);

/** A stand-in event: the modifiers, the key and the target's value. */
const standInEvent = ({ key = "", altKey = false, shiftKey = false, metaKey = false, ctrlKey = false, value = "" } = {}) => {
  const event = {
    key,
    altKey,
    shiftKey,
    metaKey,
    ctrlKey,
    prevented: false,
    stopped: false,
    target: { value, checked: true },
    currentTarget: { value, checked: true },
    preventDefault() {
      event.prevented = true;
    },
    stopPropagation() {
      event.stopped = true;
    },
    nativeEvent: {
      stopImmediatePropagation() {
        event.stopped = true;
      },
    },
  };
  return event;
};

/** Runs `handler` with `event`, returning what it recorded. */
const run = (handler, event) => {
  recorder = [];
  currentEvent = event;
  try {
    handler(event);
    return recorder;
  } finally {
    recorder = null;
    currentEvent = null;
  }
};

const KEYS = [
  "q", "w", "e", "r", "a", "s", "d", "f", "z", "x", "c", "v", "b", "Q",
  "Tab", "Shift+Tab", "ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp",
  "Escape", "Enter", "Meta+w", "Alt+w", "Ctrl+w",
];

const keyEvent = (spec) => {
  const parts = spec.split("+");
  const key = parts.pop();
  return standInEvent({
    key,
    shiftKey: parts.includes("Shift"),
    metaKey: parts.includes("Meta"),
    altKey: parts.includes("Alt"),
    ctrlKey: parts.includes("Ctrl"),
  });
};

/** What a host element's handlers do (see the header). */
const handlers = (props) => {
  const on = {};
  if (typeof props.onClick === "function") {
    const click = run(props.onClick, standInEvent());
    const altClick = run(props.onClick, standInEvent({ altKey: true }));
    on.click = click;
    if (JSON.stringify(altClick) !== JSON.stringify(click)) on.altClick = altClick;
  }
  if (typeof props.onChange === "function") on.change = run(props.onChange, standInEvent({ value: "30" }));
  if (typeof props.onKeyDown === "function") {
    on.keydown = Object.fromEntries(
      KEYS.map((spec) => {
        const event = keyEvent(spec);
        const calls = run(props.onKeyDown, event);
        return [spec, { calls, prevented: event.prevented, stopped: event.stopped }];
      }),
    );
  }
  return Object.keys(on).length ? on : null;
};

const attrValue = (type, k, v) => {
  if (typeof v === "boolean") {
    if (k.startsWith("aria-") || k.startsWith("data-")) return String(v);
    if (BOOLEAN_ATTRIBUTES.has(k)) return v ? "" : undefined;
    throw new Error(`<${type}> ${k} is a boolean`);
  }
  if (typeof v === "number") return String(v);
  if (typeof v === "string") return v;
  throw new Error(`<${type}> ${k} is ${typeof v}`);
};

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
  if (type === fontPicker) {
    const { isOpened, selectedFontFamily, hoveredFontFamily, topPicks, compactMode } = props;
    return [{ fontPicker: json({ isOpened, selectedFontFamily, hoveredFontFamily, topPicks: topPicks ?? null, compactMode }) }];
  }
  if (typeof type === "function" && iconNames.has(type)) {
    const out = { icon: iconNames.get(type) };
    if (props.theme !== undefined) out.theme = props.theme;
    if (props.flip !== undefined) {
      out.flip = props.flip;
      if (props.flip) flipped.add(out.icon);
    }
    return [out];
  }
  if (typeof type === "function") return flatten(type(props));
  if (typeof type !== "string") throw new Error(`unexpected element type ${String(type)}`);
  const out = { tag: type };
  if (props.className) out.class = props.className;
  const attrs = [];
  for (const [k, v] of Object.entries(props)) {
    if (["children", "className", "style", "key", "ref"].includes(k) || v === undefined || v === null || typeof v === "function") continue;
    const value = attrValue(type, k, v);
    if (value !== undefined) attrs.push([k, value]);
  }
  if (attrs.length) out.attrs = Object.fromEntries(attrs);
  if (props.style) {
    const style = Object.entries(props.style).filter(([, v]) => v !== undefined).map(([k, v]) => [k, String(v)]);
    if (style.length) out.style = Object.fromEntries(style);
  }
  const children = flatten(props.children);
  if (children.length) out.children = children;
  const on = handlers(props);
  if (on) out.on = on;
  return [out];
};

/** Renders `render()` with effects run after each render until the
 * IconPicker atom settles. */
const renderSettled = (render) => {
  for (let pass = 0; pass < 4; pass++) {
    globalThis.__effects = [];
    picker.pendingMore = undefined;
    const tree = render();
    for (const effect of globalThis.__effects) effect();
    globalThis.__effects = [];
    if (picker.pendingMore === undefined || picker.pendingMore === picker.more) return tree;
    picker.more = picker.pendingMore;
  }
  throw new Error("IconPicker state did not settle");
};

const runCase = (up, window, sceneName, elements, [id, c], trees, treeIndex) => {
  const byId = new Map(elements.map((e) => [e.id, e]));
  const resolveRefs = (state) =>
    Object.fromEntries(Object.entries(state).map(([k, v]) => [k, v && typeof v === "object" && "ref" in v ? byId.get(v.ref) : v]));
  const patch = {
    activeTool: { ...up.getDefaultAppState().activeTool, ...tool(c.tool ?? "selection") },
    selectedElementIds: selecting(c.select ?? []),
    ...(c.state ?? {}),
  };
  const appState = { ...up.getDefaultAppState(), width: 1440, height: 900, ...resolveRefs(patch) };
  const scene = new up.Scene(elements, { skipValidation: true });
  const history = c.history ?? { undoEmpty: true, redoEmpty: true };
  const historyStub = {
    isUndoStackEmpty: history.undoEmpty,
    isRedoStackEmpty: history.redoEmpty,
    onHistoryChangedEmitter: { on: () => () => {} },
  };
  const actions = new Map(up.actions.map((a) => [a.name, a]));
  actions.set("undo", up.createUndoAction(historyStub));
  actions.set("redo", up.createRedoAction(historyStub));
  up.__setLanguage({ code: "en", label: "English", rtl: Boolean(c.rtl) });
  const iconPicker = c.iconPicker ?? { open: null, more: false };
  const panels = {};
  // getSelectedElements logs its invariant for a selected deleted element;
  // the panels render as they do without it
  const error = console.error;
  console.error = (...args) => {
    if (!String(args[0]).startsWith("[NONDELETED][INVARIANT]")) error(...args);
  };
  try {
    for (const [mode, editorInterface] of Object.entries(MODES)) {
      globalThis.__editorInterface = editorInterface;
      globalThis.__stylesPanelMode = mode;
      const app = { scene, state: appState, editorInterface, props: { UIOptions: { canvasActions: {} } } };
      app.bucketFill = new up.AppBucketFill(app);
      panels[mode] = COLUMNS.map((column) => {
        const [name, variant] = column.split("+");
        const action = actions.get(name);
        if (!action?.PanelComponent) throw new Error(`${name} has no PanelComponent`);
        picker.open = iconPicker.open;
        picker.more = iconPicker.more;
        const tree = renderSettled(() =>
          flatten(
            action.PanelComponent({
              elements,
              appState,
              updateData: (value) => {
                if (!recorder) throw new Error(`${name}: updateData outside a handler`);
                recorder.push({ update: value === currentEvent ? "[event]" : value === undefined ? null : json(value) });
              },
              appProps: app.props,
              app,
              data: variant === "cycle" ? { cycle: true } : undefined,
              renderAction: (n, d) => ({ action: n, data: d }),
            }),
          ),
        );
        const key = JSON.stringify(tree);
        if (!treeIndex.has(key)) {
          treeIndex.set(key, trees.length);
          trees.push(tree);
        }
        return treeIndex.get(key);
      });
    }
  } finally {
    console.error = error;
    up.__setLanguage({ code: "en", label: "English" });
  }
  return {
    id,
    scene: sceneName,
    rtl: Boolean(c.rtl),
    appState: json(patch),
    iconPicker,
    history,
    panels,
  };
};

const build = async (upstream) => {
  const window = installDom(ORIGIN);
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: PANEL_SHIMS,
    patch: PATCH,
    define: {
      "import.meta.env.MODE": '"production"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  colorPicker = up.ColorPicker;
  fontPicker = up.FontPicker;
  iconNames.clear();
  flipped.clear();
  for (const [name, value] of Object.entries(up.icons)) {
    if (!value || iconNames.has(value)) continue;
    if ((typeof value === "object" && value.type === "svg") || typeof value === "function") iconNames.set(value, name);
  }
  globalThis.__moreOptions = () => [
    picker.more,
    (v) => {
      const next = typeof v === "function" ? v(picker.more) : v;
      if (recorder) recorder.push({ iconPicker: { open: picker.open, more: next } });
      else picker.pendingMore = next;
    },
  ];
  globalThis.__iconPickerActive = (label) => [
    picker.open === label,
    (open) => {
      if (!recorder) throw new Error("IconPicker setActive outside a handler");
      recorder.push({ iconPicker: { open: open ? label : null, more: picker.more } });
    },
  ];
  const scenes = {};
  const cases = [];
  const trees = [];
  const treeIndex = new Map();
  const sceneMakers = { main: SCENES.main, extra: EXTRA };
  for (const [name, make] of Object.entries(sceneMakers)) {
    up.reseed(RANDOM_SEED);
    const elements = make(up).map(fixed);
    scenes[name] = json(elements);
    const list = name === "main" ? handCases() : extraCases(elements);
    for (const c of list) cases.push(runCase(up, window, name, elements, c, trees, treeIndex));
  }
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case ${c.id}`);
    ids.add(c.id);
  }
  window.close();
  delete globalThis.__moreOptions;
  delete globalThis.__iconPickerActive;

  const react = await loadUpstream(upstream, { entry: ICON_ENTRY, jsx: "automatic" });
  const flippedIcons = Object.fromEntries(
    [...flipped].sort().map((name) => [name, react.renderToStaticMarkup(react.React.createElement(react.icons[name], { flip: true }))]),
  );

  return format({
    description:
      "Upstream's action PanelComponents (packages/excalidraw/actions/*.tsx) as the styles panels render them through renderAction, at the pinned commit (tools/goldens/action-panels.mjs): per case app state keys over a scene of upstream-built elements, the language's direction, IconPicker's state and the history's, and per styles panel mode the index into `trees` of what each column's PanelComponent renders ({tag, class, attrs, style, children, on}, text, {icon, theme?, flip?}, {action, data?} for a nested renderAction, {colorPicker} and {fontPicker} props, {tag: popover} for radix's Popover.Content); `on` records what each handler passes to updateData.",
    upstream: upstream.commit,
    columns: COLUMNS,
    modes: MODES,
    flippedIcons,
    trees,
    scenes,
    cases,
  });
};

/** Runs fn with Math.random disabled. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating action-panels goldens");
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
    process.stderr.write(`action-panels: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("action-panels goldens are out of date: run node tools/goldens/action-panels.mjs\n");
      process.exit(1);
    }
    process.stdout.write("action-panels goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
  // React's server renderer leaves a scheduler handle open.
  process.exit(0);
}
