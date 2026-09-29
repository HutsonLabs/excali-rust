#!/usr/bin/env node
// Colour picker goldens for excali-ui (ex-523): upstream's own ColorPicker
// (packages/excalidraw/components/ColorPicker/*: the top-picks strip, the
// trigger swatch and the popup's Picker with its most-used custom colours,
// 5x3 palette, shades, hex input and eye-dropper trigger) rendered by React
// 19.0.0 into jsdom 22.1.0 with radix-ui 1.4.3, its keyboard map
// (keyboardNavHandlers.ts) and helpers (colorPickerUtils.ts), the palettes
// and colour helpers of packages/common/src/colors.ts, the eye dropper's
// cursor and preview placement (EyeDropper.tsx,
// positionElementBesideCursor.ts), and the picker's stylesheets compiled
// with sass 1.51.0.
//
//   node tools/goldens/color-picker.mjs            write the fixture and CSS
//   node tools/goldens/color-picker.mjs --check    exit 1 if either is stale
//   node tools/goldens/color-picker.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/color-picker.json:
//
// - `locale`: the English strings the picker reads (locales/en.json);
// - `palettes`: COLOR_PALETTE, the stroke and background palettes (entries
//   in key order), the top picks, the hotkeys and the constants;
// - `isColorDark`, `normalizeInputColor`: each helper per input;
// - `colorNameAndShade`, `mostUsedCustomColors`: colorPickerUtils per input;
// - `keyNav`: colorPickerKeyNavHandler per state and key: whether it
//   handled the key, the colours it passed to onChange, the sections it
//   set, the eye-dropper toggles (`force` or null), Escape calls and
//   preventDefault (each left out when empty), with the modifiers held;
// - `hexInput`: ColorInput per typed value (a React change event): the
//   colours passed to onChange, the input's value, aria-invalid and the
//   error message shown; then a blur;
// - `eyeDropper`: the preview's cursor style and positionElementBesideCursor
//   per case;
// - `cases`: per case the ColorPicker's props and the DOM React leaves in
//   the editor container: `{tag, attrs, style, children}` with attributes
//   and inline style sorted by name, text as a string, an icon's <svg> as
//   `{icon: name}`, radix's useId ids as `radix-N`, and what floating-ui
//   computes from layout (jsdom has none) left out: the popper wrapper's
//   and the arrow's style, and the content's data-side and data-align.
//
// and crates/excali-ui/src/color_picker/color_picker.css: ColorPicker.scss
// and EyeDropper.scss compiled (expanded) as one entry.
//
// Top-picks customisation (drag and drop, its context menu and tip,
// customizableTopPicks) is not rendered: no case passes
// customizableTopPicks.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "color-picker.json");
export const STYLESHEET = join("src", "color_picker", "color_picker.css");

export const STYLESHEETS = ["components/ColorPicker/ColorPicker.scss", "components/EyeDropper.scss"];

const ENTRY = `
export { ColorPicker } from "./packages/excalidraw/components/ColorPicker/ColorPicker";
export { ColorInput } from "./packages/excalidraw/components/ColorPicker/ColorInput";
export * as utils from "./packages/excalidraw/components/ColorPicker/colorPickerUtils";
export { colorPickerKeyNavHandler } from "./packages/excalidraw/components/ColorPicker/keyboardNavHandlers";
export { positionElementBesideCursor } from "./packages/excalidraw/components/positionElementBesideCursor";
export { eyeDropperCursor } from "./packages/excalidraw/components/EyeDropper";
export * as colors from "./packages/common/src/colors";
export * as icons from "./packages/excalidraw/components/icons";
export { t } from "./packages/excalidraw/i18n";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx (the whole editor) supplies the hooks the picker reads; the shim
// answers them from globalThis.__ui.
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useApp: () => globalThis.__ui.app,
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.editorInterface,
      useStylesPanelMode: () => globalThis.__ui.stylesPanelMode,
      useExcalidrawAppState: () => ({}),
      useExcalidrawElements: () => [],
      useExcalidrawSetAppState: () => () => {},
    };`,
  "packages/excalidraw/editor-jotai": `
    const atom = (init) => ({ init });
    module.exports = {
      atom,
      useSetAtom: () => () => {},
      useAtomValue: (a) => a.init,
      useAtom: (a) => [a.init, () => {}],
      editorJotaiStore: { get: (a) => a.init, set: () => {}, sub: () => () => {} },
    };`,
  "packages/excalidraw/analytics": `module.exports = { trackEvent: () => {} };`,
};

const STUBS = ["fuzzy", "pica", "image-blob-reduce", "browser-fs-access"];

const usage = () => {
  process.stderr.write("usage: color-picker.mjs [--check] [--out DIR]\n");
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

// -- DOM ----------------------------------------------------------------------

const installDom = () => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", {
    url: "http://localhost/",
    pretendToBeVisual: true,
  });
  const { window } = dom;
  const globals = {
    window,
    document: window.document,
    navigator: window.navigator,
    Node: window.Node,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    HTMLDivElement: window.HTMLDivElement,
    HTMLInputElement: window.HTMLInputElement,
    SVGElement: window.SVGElement,
    MutationObserver: window.MutationObserver,
    ResizeObserver: class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
    DOMRect: window.DOMRect,
    devicePixelRatio: 1,
    Event: window.Event,
    CustomEvent: window.CustomEvent,
    MouseEvent: window.MouseEvent,
    KeyboardEvent: window.KeyboardEvent,
    FocusEvent: window.FocusEvent,
    NodeFilter: window.NodeFilter,
    PointerEvent: window.MouseEvent,
    getComputedStyle: window.getComputedStyle.bind(window),
    requestAnimationFrame: window.requestAnimationFrame.bind(window),
    cancelAnimationFrame: window.cancelAnimationFrame.bind(window),
    IS_REACT_ACT_ENVIRONMENT: true,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  window.ResizeObserver = globals.ResizeObserver;
  recordStyles(window);
  return window;
};

// The inline style React (and radix) write, per declaration object: jsdom
// 22's CSSStyleDeclaration normalises what it keeps (`#fff` reads back as
// `rgb(255, 255, 255)`, `0` as `0px`) and drops what it cannot parse (a
// `background-color` or `color` of `var(...)`), so the fixture records the
// values as they were set, which is what excali-ui's builder holds too.
const written = new WeakMap();

const recordStyles = (window) => {
  const proto = window.CSSStyleDeclaration.prototype;
  const hyphenate = (p) => p.replace(/[A-Z]/g, (m) => `-${m.toLowerCase()}`);
  const note = (style, property, value) => {
    if (!written.has(style)) written.set(style, new Map());
    const map = written.get(style);
    if (value === null || value === undefined || value === "") map.delete(property);
    else map.set(property, String(value));
  };
  for (const name of Object.getOwnPropertyNames(proto)) {
    const desc = Object.getOwnPropertyDescriptor(proto, name);
    if (!desc?.set || ["cssText", "cssFloat", "length", "parentRule"].includes(name)) continue;
    Object.defineProperty(proto, name, {
      ...desc,
      set(value) {
        desc.set.call(this, value);
        note(this, hyphenate(name), value);
      },
    });
  }
  const setProperty = proto.setProperty;
  proto.setProperty = function (property, value, priority) {
    setProperty.call(this, property, value, priority);
    note(this, property, value);
  };
  const removeProperty = proto.removeProperty;
  proto.removeProperty = function (property) {
    note(this, property, null);
    return removeProperty.call(this, property);
  };
};

const sorted = (entries) => Object.fromEntries([...entries].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));

const styleOf = (node) => {
  const style = node.style;
  if (!style) return {};
  const map = written.get(style);
  if (map) return sorted(map.entries());
  if (style.length) throw new Error(`a style nobody wrote through the recorder: ${node.outerHTML.slice(0, 120)}`);
  return {};
};

const staticIcons = (icons) =>
  Object.entries(icons).filter(
    ([, value]) => value && value.$$typeof === Symbol.for("react.transitional.element") && value.type === "svg",
  );

/** React's useId ids (`«r0»` in React 19.0, `:r0:` before) → radix-N. */
const renameIds = (value, ids) =>
  value.replace(/radix-(«r[0-9a-z]+»|:r[0-9a-z]+:)/g, (id) => {
    if (!ids.has(id)) ids.set(id, `radix-${ids.size + 1}`);
    return ids.get(id);
  });

const makeTree = (iconNames, ids) => {
  const tree = (node) => {
    if (node.nodeType === 3) return node.data;
    if (node.localName === "svg") {
      const name = iconNames.get(node.outerHTML);
      if (name) return { icon: name };
      // radix's Popover.Arrow is the one other svg
      if (!node.querySelector(":scope > polygon")) {
        throw new Error(`an svg that is no icons.tsx export: ${node.outerHTML.slice(0, 120)}`);
      }
    }
    const attrs = [...node.attributes]
      .filter((a) => a.name !== "style")
      .map((a) => [a.name, a.name === "id" || a.name.startsWith("aria-") ? renameIds(a.value, ids) : a.value]);
    // floating-ui's placement from layout (jsdom has none): the popper
    // wrapper's style, the side and alignment it settled on, the arrow's
    // offsets
    const popper = node.hasAttribute("data-radix-popper-content-wrapper");
    const placed = node.parentElement?.hasAttribute("data-radix-popper-content-wrapper");
    const arrow = node.localName === "span" && node.firstElementChild?.localName === "svg" && !iconNames.has(node.firstElementChild.outerHTML);
    return {
      tag: node.localName,
      attrs: sorted(placed ? attrs.filter(([n]) => n !== "data-side" && n !== "data-align") : attrs),
      style: popper || arrow ? {} : styleOf(node),
      children: [...node.childNodes].filter((c) => c.nodeType === 1 || c.nodeType === 3).map(tree),
    };
  };
  return tree;
};

// -- cases --------------------------------------------------------------------

/** Elements for the most-used custom colours: stroke and background colours. */
const customElements = (colors, property) =>
  colors.map(([color, deleted]) => ({
    strokeColor: property === "strokeColor" ? color : "#1e1e1e",
    backgroundColor: property === "backgroundColor" ? color : "transparent",
    isDeleted: !!deleted,
  }));

const CUSTOM = [
  ["#123456"],
  ["#abcdef"],
  ["#123456"],
  ["#fedcba"],
  ["#abcdef"],
  ["#123456"],
  ["#0000ff", true],
  ["#e03131"],
  ["#010203"],
  ["#040506"],
  ["#070809"],
];

/**
 * A case: the picker's `type`, `color`, `palette` (a colors.ts name or
 * null), `topPicks` (a colors.ts name), `excludedColors`, `theme`,
 * `open` (appState.openPopup === type), the styles panel mode, the
 * elements for the most-used custom colours, whether a text element is
 * being edited, and the form factor.
 */
const CASES = [
  { name: "stroke", type: "elementStroke", color: "#1e1e1e", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS" },
  { name: "stroke-red", type: "elementStroke", color: "#e03131", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS" },
  { name: "stroke-mixed", type: "elementStroke", color: null, palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS" },
  { name: "stroke-dark", type: "elementStroke", color: "#1e1e1e", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", theme: "dark" },
  { name: "stroke-no-top-picks-prop", type: "elementStroke", color: "#2f9e44", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE" },
  { name: "background", type: "elementBackground", color: "transparent", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS" },
  { name: "background-blue", type: "elementBackground", color: "#a5d8ff", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS" },
  { name: "background-light-yellow", type: "elementBackground", color: "#fff9db", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS" },
  { name: "canvas", type: "canvasBackground", color: "#ffffff", palette: null, topPicks: "DEFAULT_CANVAS_BACKGROUND_PICKS" },
  { name: "canvas-dark", type: "canvasBackground", color: "#fdf8f6", palette: null, topPicks: "DEFAULT_CANVAS_BACKGROUND_PICKS", theme: "dark" },
  { name: "open-stroke", type: "elementStroke", color: "#1e1e1e", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", open: true },
  { name: "open-stroke-red", type: "elementStroke", color: "#e03131", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", open: true },
  { name: "open-stroke-red-light", type: "elementStroke", color: "#ffc9c9", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", open: true },
  { name: "open-stroke-mixed", type: "elementStroke", color: null, palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", open: true },
  { name: "open-stroke-dark", type: "elementStroke", color: "#1971c2", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", open: true, theme: "dark" },
  { name: "open-stroke-custom", type: "elementStroke", color: "#123456", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", open: true, custom: CUSTOM },
  { name: "open-stroke-custom-unlisted", type: "elementStroke", color: "#777777", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", open: true, custom: CUSTOM },
  { name: "open-stroke-alpha", type: "elementStroke", color: "#e0313180", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", open: true },
  { name: "open-background", type: "elementBackground", color: "transparent", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS", open: true },
  { name: "open-background-teal", type: "elementBackground", color: "#96f2d7", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS", open: true },
  { name: "open-background-custom", type: "elementBackground", color: "#abcdef", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS", open: true, custom: CUSTOM },
  { name: "open-background-white", type: "elementBackground", color: "#ffffff", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS", open: true },
  { name: "open-sticky-background", type: "elementBackground", color: "#ffdf6b", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "STICKY_NOTE_BACKGROUND_PICKS", excludedColors: ["transparent"], open: true },
  { name: "open-bucket-fill", type: "elementBackground", color: "#ffec99", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "BUCKET_FILL_BACKGROUND_PICKS", excludedColors: ["transparent"], open: true },
  { name: "open-canvas", type: "canvasBackground", color: "#ffffff", palette: null, topPicks: "DEFAULT_CANVAS_BACKGROUND_PICKS", open: true },
  { name: "open-canvas-palette", type: "canvasBackground", color: "#f8f9fa", palette: "COLOR_PALETTE", topPicks: "DEFAULT_CANVAS_BACKGROUND_PICKS", open: true, custom: CUSTOM },
  { name: "open-editing-text", type: "elementStroke", color: "#1e1e1e", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", open: true, editingText: true },
  { name: "compact-stroke", type: "elementStroke", color: "#1e1e1e", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", mode: "compact" },
  { name: "compact-stroke-light", type: "elementStroke", color: "#fff5f5", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", mode: "compact", theme: "dark" },
  { name: "compact-background", type: "elementBackground", color: "#b2f2bb", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS", mode: "compact" },
  { name: "open-compact-stroke", type: "elementStroke", color: "#1e1e1e", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", mode: "compact", open: true },
  { name: "open-compact-background", type: "elementBackground", color: "#b2f2bb", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS", mode: "compact", open: true },
  { name: "open-mobile-background", type: "elementBackground", color: "#b2f2bb", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_BACKGROUND_PICKS", mode: "mobile", formFactor: "phone", open: true },
  { name: "open-phone-stroke", type: "elementStroke", color: "#1e1e1e", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", topPicks: "DEFAULT_ELEMENT_STROKE_PICKS", formFactor: "phone", open: true },
];

const LABELS = { elementStroke: "labels.stroke", elementBackground: "labels.background", canvasBackground: "labels.canvasBackground" };

const render = async (up, window, iconNames, c) => {
  const { React, act, createRoot, ColorPicker, colors } = up;
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  const formFactor = c.formFactor ?? "desktop";
  globalThis.__ui = {
    app: { ownerWindow: window, ownerDocument: document },
    container: { container, id: "excalidraw-id" },
    editorInterface: { formFactor, desktopUIMode: "full", userAgent: {}, isTouchScreen: false, isLandscape: false },
    stylesPanelMode: c.mode ?? "full",
  };
  const property = c.type === "elementStroke" ? "strokeColor" : "backgroundColor";
  const elements = customElements(c.custom ?? [], property);
  const appState = {
    openPopup: c.open ? c.type : null,
    theme: c.theme ?? "light",
    colorTopPicks: {},
    editingTextElement: c.editingText ? { id: "text" } : null,
  };
  const root = createRoot(container);
  await act(async () =>
    root.render(
      React.createElement(ColorPicker, {
        type: c.type,
        color: c.color,
        onChange: () => {},
        label: up.t(LABELS[c.type]),
        elements,
        appState,
        palette: c.palette === null ? null : colors[c.palette],
        ...(c.topPicks ? { topPicks: colors[c.topPicks] } : {}),
        updateData: () => {},
        ...(c.excludedColors ? { excludedColors: c.excludedColors } : {}),
      }),
    ),
  );
  const ids = new Map();
  const dom = [...container.childNodes].map(makeTree(iconNames, ids));
  await act(async () => root.unmount());
  return {
    name: c.name,
    type: c.type,
    color: c.color,
    palette: c.palette,
    topPicks: c.topPicks ?? null,
    excludedColors: c.excludedColors ?? [],
    theme: c.theme ?? "light",
    open: !!c.open,
    mode: c.mode ?? "full",
    formFactor,
    editingText: !!c.editingText,
    elements,
    dom,
  };
};

// -- helpers ------------------------------------------------------------------

const COLOR_INPUTS = [
  "",
  "transparent",
  "TRANSPARENT",
  "#1e1e1e",
  "#ffffff",
  "#fff",
  "#ffff",
  "#ffffff80",
  "#ffffff00",
  "white",
  "black",
  "red",
  "#ff0000",
  "#e03131",
  "#ffc9c9",
  "#a5d8ff",
  "#868e96",
  "#343a40",
  "#ced4da",
  "#fab005",
  "#ffec99",
  "#f5faff",
  "rgb(12, 34, 56)",
  "rgba(255, 255, 255, 0.5)",
  "rgba(0, 0, 0, 0)",
  "hsl(120, 100%, 25%)",
  "not a color",
  "#12",
  "12345",
  "abc",
  "ABCDEF",
  "  #abc  ",
  " ff0000 ",
  "#eaeaea",
  "#f0f0f0",
  "#a0a0a0",
  "#9f9f9f",
  "#a1a1a1",
];

const THRESHOLDS = [undefined, 160, 240, 255];

const colorHelpers = (up) => {
  const { isColorDark, normalizeInputColor } = up.colors;
  const dark = [];
  for (const color of COLOR_INPUTS) {
    for (const threshold of THRESHOLDS) {
      dark.push({ color, threshold: threshold ?? null, dark: isColorDark(color, threshold) });
    }
  }
  const normalized = COLOR_INPUTS.map((input) => ({ input, output: normalizeInputColor(input) }));
  return { dark, normalized };
};

const palettes = (up) => {
  const c = up.colors;
  const entries = (p) =>
    Object.entries(p).map(([name, value]) => ({ name, value: Array.isArray(value) ? [...value] : value }));
  return {
    COLOR_PALETTE: entries(c.COLOR_PALETTE),
    DEFAULT_ELEMENT_STROKE_COLOR_PALETTE: entries(c.DEFAULT_ELEMENT_STROKE_COLOR_PALETTE),
    DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE: entries(c.DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE),
    DEFAULT_ELEMENT_STROKE_PICKS: [...c.DEFAULT_ELEMENT_STROKE_PICKS],
    DEFAULT_ELEMENT_BACKGROUND_PICKS: [...c.DEFAULT_ELEMENT_BACKGROUND_PICKS],
    BUCKET_FILL_BACKGROUND_PICKS: [...c.BUCKET_FILL_BACKGROUND_PICKS],
    STICKY_NOTE_STROKE_PICKS: [...c.STICKY_NOTE_STROKE_PICKS],
    STICKY_NOTE_BACKGROUND_PICKS: [...c.STICKY_NOTE_BACKGROUND_PICKS],
    DEFAULT_CANVAS_BACKGROUND_PICKS: [...c.DEFAULT_CANVAS_BACKGROUND_PICKS],
    COLOR_TOP_PICKS_SLOTS: c.COLOR_TOP_PICKS_SLOTS,
    COLORS_PER_ROW: c.COLORS_PER_ROW,
    MAX_CUSTOM_COLORS_USED_IN_CANVAS: c.MAX_CUSTOM_COLORS_USED_IN_CANVAS,
    DEFAULT_ELEMENT_STROKE_COLOR_INDEX: c.DEFAULT_ELEMENT_STROKE_COLOR_INDEX,
    DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX: c.DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX,
    COLOR_OUTLINE_CONTRAST_THRESHOLD: c.COLOR_OUTLINE_CONTRAST_THRESHOLD,
    DEFAULT_STICKY_NOTE_BG: c.DEFAULT_STICKY_NOTE_BG,
    allColorsSpecificShade: [0, 1, 2, 3, 4].map((i) => c.getAllColorsSpecificShade(i)),
    colorPickerHotkeyBindings: [...up.utils.colorPickerHotkeyBindings],
  };
};

const PICKER_COLORS = [
  null,
  "",
  "transparent",
  "#1e1e1e",
  "#ffffff",
  "#e03131",
  "#fff5f5",
  "#a5d8ff",
  "#846358",
  "#123456",
  "#E03131",
];

const utilCases = (up) => {
  const c = up.colors;
  const { getColorNameAndShadeFromColor, getMostUsedCustomColors, isCustomColor } = up.utils;
  const names = [];
  for (const palette of ["COLOR_PALETTE", "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE"]) {
    for (const color of PICKER_COLORS) {
      names.push({
        palette,
        color,
        result: getColorNameAndShadeFromColor({ palette: c[palette], color }),
        isCustom: color === null ? null : isCustomColor({ color, palette: c[palette] }),
      });
    }
  }
  const mostUsed = [];
  const sets = {
    none: [],
    custom: CUSTOM,
    palette: [["#e03131"], ["#1e1e1e"], ["transparent"]],
    alpha: [["#12345600"], ["#12345680"], ["#12345680"], ["rgba(0,0,0,0)"]],
    ties: [["#aaaaaa"], ["#bbbbbb"], ["#cccccc"], ["#dddddd"], ["#eeeeee"], ["#ababab"], ["#bbbbbb"]],
  };
  for (const [set, colors] of Object.entries(sets)) {
    for (const type of ["elementStroke", "elementBackground"]) {
      const property = type === "elementStroke" ? "strokeColor" : "backgroundColor";
      for (const palette of ["DEFAULT_ELEMENT_STROKE_COLOR_PALETTE", "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE"]) {
        const elements = customElements(colors, property);
        mostUsed.push({ set, type, palette, elements, result: getMostUsedCustomColors(elements, type, c[palette]) });
      }
    }
  }
  return { names, mostUsed };
};

// -- keyboard -------------------------------------------------------------------

const KEY_EVENTS = [
  { key: "q", code: "KeyQ" },
  { key: "w", code: "KeyW" },
  { key: "t", code: "KeyT" },
  { key: "a", code: "KeyA" },
  { key: "g", code: "KeyG" },
  { key: "z", code: "KeyZ" },
  { key: "b", code: "KeyB" },
  { key: "Q", code: "KeyQ", shiftKey: true },
  { key: "y", code: "KeyY" },
  { key: "1", code: "Digit1" },
  { key: "3", code: "Digit3" },
  { key: "5", code: "Digit5" },
  { key: "!", code: "Digit1", shiftKey: true },
  { key: "#", code: "Digit3", shiftKey: true },
  { key: "%", code: "Digit5", shiftKey: true },
  { key: "End", code: "Numpad1", shiftKey: true },
  { key: "i", code: "KeyI" },
  { key: "Alt", code: "AltLeft", altKey: true },
  { key: "Escape", code: "Escape" },
  { key: "q", code: "KeyQ", ctrlKey: true },
  { key: "q", code: "KeyQ", metaKey: true },
  { key: "Enter", code: "Enter" },
];

const NAV_EVENTS = [
  { key: "ArrowLeft", code: "ArrowLeft" },
  { key: "ArrowRight", code: "ArrowRight" },
  { key: "ArrowUp", code: "ArrowUp" },
  { key: "ArrowDown", code: "ArrowDown" },
  { key: "Tab", code: "Tab" },
  { key: "Tab", code: "Tab", shiftKey: true },
];

const KEY_COLORS = ["#1e1e1e", "transparent", "#e03131", "#ffc9c9", "#846358", "#123456", null];

const SECTIONS = [null, "custom", "baseColors", "shades", "hex"];

const CUSTOMS = [[], ["#123456", "#abcdef", "#fedcba"]];

const PICKERS = [
  { picker: "stroke", type: "elementStroke", palette: "DEFAULT_ELEMENT_STROKE_COLOR_PALETTE" },
  { picker: "background", type: "elementBackground", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE" },
  { picker: "sticky", type: "elementBackground", palette: "DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE", excludedColors: ["transparent"] },
  { picker: "full", type: "canvasBackground", palette: "COLOR_PALETTE" },
];

const keyNav = (up) => {
  const c = up.colors;
  const out = [];
  const run = (p, color, section, customColors, ev) => {
    const palette = c[p.palette];
    const colorObj = up.utils.getColorNameAndShadeFromColor({ color, palette });
    // Picker.tsx:125-130
    const activeShade =
      colorObj?.shade ??
      (p.type === "elementBackground" ? c.DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX : c.DEFAULT_ELEMENT_STROKE_COLOR_INDEX);
    const record = { changes: [], sections: [], eyeDropper: [], escape: 0, prevented: false };
    const event = {
      key: ev.key,
      code: ev.code,
      shiftKey: !!ev.shiftKey,
      ctrlKey: !!ev.ctrlKey,
      metaKey: !!ev.metaKey,
      altKey: !!ev.altKey,
      preventDefault: () => (record.prevented = true),
      stopPropagation: () => {},
    };
    const handled = up.colorPickerKeyNavHandler({
      event,
      activeColorPickerSection: section,
      palette,
      color,
      onChange: (x) => record.changes.push(x ?? null),
      customColors,
      setActiveColorPickerSection: (s) => record.sections.push(s),
      updateData: () => {
        throw new Error("updateData");
      },
      activeShade,
      onEyeDropperToggle: (force) => record.eyeDropper.push(force ?? null),
      onEscape: () => record.escape++,
      excludedColors: p.excludedColors,
    });
    // empty outcomes are left out
    const mods = ["shift", "ctrl", "meta", "alt"].filter((m) => ev[`${m}Key`]);
    out.push({
      picker: p.picker,
      color,
      section,
      custom: customColors.length > 0,
      activeShade,
      key: ev.key,
      code: ev.code,
      ...(mods.length ? { mods } : {}),
      handled,
      ...Object.fromEntries(Object.entries(record).filter(([, v]) => v && (!Array.isArray(v) || v.length))),
    });
  };
  for (const p of PICKERS) {
    for (const color of KEY_COLORS) {
      for (const customColors of CUSTOMS) {
        for (const ev of KEY_EVENTS) run(p, color, null, customColors, ev);
        for (const section of SECTIONS) for (const ev of NAV_EVENTS) run(p, color, section, customColors, ev);
      }
    }
  }
  return out;
};

// -- hex input ------------------------------------------------------------------

const HEX_TYPED = [
  "ff0000",
  "#ff0000",
  "FF0000",
  " #AbC ",
  "f00f",
  "12345",
  "1234567",
  "#12",
  "zz",
  "red",
  "transparent",
  "rgb(1,2,3)",
  "",
  "12345678",
  "#",
];

const hexInput = async (up, window) => {
  const { React, act, createRoot, ColorInput } = up;
  const { document } = window;
  const out = [];
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
  for (const typed of HEX_TYPED) {
    document.body.innerHTML = "";
    const container = document.createElement("div");
    document.body.appendChild(container);
    globalThis.__ui = {
      app: { ownerWindow: window, ownerDocument: document },
      container: { container, id: "excalidraw-id" },
      editorInterface: { formFactor: "desktop", isTouchScreen: false },
      stylesPanelMode: "full",
    };
    const changes = [];
    const root = createRoot(container);
    await act(async () =>
      root.render(
        React.createElement(ColorInput, {
          color: "#1e1e1e",
          onChange: (x) => changes.push(x),
          label: "Stroke",
          colorPickerType: "elementStroke",
          placeholder: up.t("colorPicker.color"),
        }),
      ),
    );
    const input = container.querySelector("input");
    const initial = input.value;
    await act(async () => {
      setter.call(input, typed);
      input.dispatchEvent(new window.Event("input", { bubbles: true }));
    });
    const error = container.querySelector(".color-picker__error-message");
    const after = {
      value: input.value,
      ariaInvalid: input.getAttribute("aria-invalid"),
      hasError: container.querySelector(".color-picker__input-label").classList.contains("has-error"),
      error: error ? error.textContent : null,
    };
    await act(async () => {
      input.dispatchEvent(new window.FocusEvent("focusout", { bubbles: true }));
      input.dispatchEvent(new window.FocusEvent("blur"));
    });
    const blurred = {
      value: input.value,
      ariaInvalid: input.getAttribute("aria-invalid"),
      error: container.querySelector(".color-picker__error-message")?.textContent ?? null,
    };
    await act(async () => root.unmount());
    out.push({ typed, initial, changes, after, blurred });
  }
  return out;
};

// -- eye dropper ------------------------------------------------------------------

const POSITIONS = [
  { cursor: { x: 100, y: 100 }, element: { width: 40, height: 40 }, container: { left: 0, top: 0, width: 1000, height: 800 }, gap: 7 },
  { cursor: { x: 980, y: 790 }, element: { width: 40, height: 40 }, container: { left: 0, top: 0, width: 1000, height: 800 }, gap: 7 },
  { cursor: { x: 120, y: 60 }, element: { width: 40, height: 40 }, container: { left: 100, top: 50, width: 200, height: 100 }, gap: 7 },
  { cursor: { x: 20, y: 20 }, element: { width: 40, height: 40 }, container: { left: 0, top: 0, width: 50, height: 50 }, gap: 7 },
  { cursor: { x: 25, y: 25 }, element: { width: 60, height: 60 }, container: { left: 0, top: 0, width: 50, height: 50 }, gap: 7 },
  { cursor: { x: 953, y: 753 }, element: { width: 40, height: 40 }, container: { left: 0, top: 0, width: 1000, height: 800 }, gap: 7 },
  { cursor: { x: 954, y: 754 }, element: { width: 40, height: 40 }, container: { left: 0, top: 0, width: 1000, height: 800 }, gap: 7 },
  { cursor: { x: 10.5, y: 5.25 }, element: { width: 40, height: 40 }, container: { left: 0.5, top: 0.25, width: 100, height: 100 }, gap: 7 },
];

const eyeDropper = (up) => ({
  cursor: up.eyeDropperCursor,
  positions: POSITIONS.map((p) => ({ ...p, result: up.positionElementBesideCursor(p) })),
});

// -- stylesheet ---------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const entry = STYLESHEETS.map((path) => `@use "${path.replace(/\.scss$/, "")}";\n`).join("");
  const compiled = sass.compileString(entry, { loadPaths: [root], style: "expanded" }).css;
  return (
    "/* Generated by tools/goldens/color-picker.mjs; do not edit. Upstream's\n" +
    ` * ${STYLESHEETS.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded) as one entry that\n" +
    " * @uses each in this order. */\n" +
    `${compiled}\n`
  );
};

// -- main -----------------------------------------------------------------------

const LOCALE_KEYS = [
  "labels.stroke",
  "labels.background",
  "labels.canvasBackground",
  "labels.colorPicker",
  "labels.showStroke",
  "labels.showBackground",
  "labels.eyeDropper",
  "colorPicker.color",
  "colorPicker.mostUsedCustomColors",
  "colorPicker.colors",
  "colorPicker.shades",
  "colorPicker.hexCode",
  "colorPicker.noShades",
  "colorPicker.invalidColor",
  "colorPicker.invalidHexLength",
  "colors.transparent",
  "colors.black",
  "colors.white",
  "colors.gray",
  "colors.red",
  "colors.pink",
  "colors.grape",
  "colors.violet",
  "colors.blue",
  "colors.cyan",
  "colors.teal",
  "colors.green",
  "colors.yellow",
  "colors.orange",
  "colors.bronze",
];

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom();
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    jsx: "automatic",
    expose: { "packages/excalidraw/components/EyeDropper": ["eyeDropperCursor"] },
    define: {
      "import.meta.env.MODE": '"production"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  const iconNames = new Map();
  for (const [name, value] of staticIcons(up.icons)) {
    const host = window.document.createElement("div");
    const root = up.createRoot(host);
    await up.act(async () => root.render(value));
    const markup = host.innerHTML;
    await up.act(async () => root.unmount());
    if (iconNames.has(markup)) continue;
    iconNames.set(markup, name);
  }
  const cases = [];
  for (const c of CASES) cases.push(await render(up, window, iconNames, c));
  const hex = await hexInput(up, window);
  window.close();
  const helpers = colorHelpers(up);
  const utils = utilCases(up);
  const locale = Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)]));
  const fixture = {
    upstream: upstream.commit,
    locale,
    palettes: palettes(up),
    isColorDark: helpers.dark,
    normalizeInputColor: helpers.normalized,
    colorNameAndShade: utils.names,
    mostUsedCustomColors: utils.mostUsed,
    keyNav: keyNav(up),
    hexInput: hex,
    eyeDropper: eyeDropper(up),
    cases,
  };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`color-picker: ${error.message}\n`);
    process.exit(1);
  }
  const files = await build(upstream);
  const dir = args.out ?? OUT_DIR;
  if (args.check) {
    const stale = Object.entries(files).filter(([f, text]) => {
      const path = join(dir, f);
      return !existsSync(path) || readFileSync(path, "utf8") !== text;
    });
    if (stale.length) {
      for (const [f] of stale) process.stderr.write(`stale: ${relative(process.cwd(), join(dir, f))}\n`);
      process.stderr.write("color picker goldens are out of date: run node tools/goldens/color-picker.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`color picker goldens up to date: ${Object.keys(files).length} files\n`);
    return;
  }
  for (const [f, text] of Object.entries(files)) {
    const path = join(dir, f);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, text);
    process.stdout.write(`wrote ${relative(process.cwd(), path)} from upstream ${upstream.commit.slice(0, 7)}\n`);
  }
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
  process.exit(0);
}
