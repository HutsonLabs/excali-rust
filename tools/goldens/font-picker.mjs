#!/usr/bin/env node
// Font picker goldens for excali-ui (ex-524): upstream's own FontPicker
// (packages/excalidraw/components/FontPicker/*: the three top picks, the
// trigger and the popup's FontPickerList with its search, "In this scene"
// and "Available fonts" groups and the "old" badge of deprecated fonts)
// rendered by React 19.0.0 into jsdom 22.1.0 with radix-ui 1.4.3, driven by
// a host that updates the app state the way actionChangeFontFamily's
// PanelComponent and perform do (actions/actionProperties.tsx:1160-1540),
// the list's keyboard map (keyboardNavHandlers.ts), and the picker's
// stylesheets compiled with sass 1.51.0.
//
//   node tools/goldens/font-picker.mjs            write the fixture and CSS
//   node tools/goldens/font-picker.mjs --check    exit 1 if either is stale
//   node tools/goldens/font-picker.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/font-picker.json:
//
// - `locale`: the English strings the picker reads (locales/en.json);
// - `constants`: FONT_TOP_PICKS_SLOTS, DEFAULT_FONTS (value, label, test
//   id and icon name), and per family of Fonts.registered its icon name
//   (getFontFamilyIcon), label (getFontFamilyLabel), CSS family list
//   (getFontFamilyString) and metadata flags;
// - `isDefaultFont`: per value;
// - `keyNav`: fontPickerKeyHandler per list, hovered font and key: whether
//   it handled the key and the calls it made (onSelect, onHover, onClose,
//   focusing the search input);
// - `cases`: per case the host's initial state (openPopup,
//   selectedFontFamily, currentHoveredFontFamily, fontTopPicks), the
//   styles panel mode, the scene's font families, showDeprecatedFonts, the
//   theme, the form factor and whether text is being edited; then per step
//   (the first is the initial render) the step, the picker's callbacks it
//   made (a callback repeated back to back once), the host's state after
//   it and the DOM React leaves in the editor
//   container: `{tag, attrs, style, children}` with attributes and inline
//   style sorted by name, text as a string, an icon's <svg> as
//   `{icon: name}`, radix's useId ids as `radix-N`, and what floating-ui
//   computes from layout (jsdom has none) left out: the popper wrapper's
//   and the arrow's style, and the content's data-side and data-align.
//
// and crates/excali-ui/src/font_picker/font_picker.css: FontPicker.scss,
// QuickSearch.scss, ScrollableList.scss and TopPicksDnD/TopPicksDnD.scss
// compiled (expanded) as one entry.
//
// The top picks' drag and drop (fontTopPicksDnD.ts, TopPicksDnD/*) renders
// its strip, tip and context-menu trigger here but no case drags: the drags,
// the strip's context menu and the tip's reset link are
// font-top-picks-dnd.mjs's.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { installTouchCallout } from "./lib/top-picks-dnd.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "font-picker.json");
export const STYLESHEET = join("src", "font_picker", "font_picker.css");

export const STYLESHEETS = [
  "components/FontPicker/FontPicker.scss",
  "components/QuickSearch.scss",
  "components/ScrollableList.scss",
  "components/TopPicksDnD/TopPicksDnD.scss",
];

export const ENTRY = `
export { FontPicker, DEFAULT_FONTS, isDefaultFont } from "./packages/excalidraw/components/FontPicker/FontPicker";
export { getFontFamilyIcon, getFontFamilyLabel } from "./packages/excalidraw/components/FontPicker/FontPickerList";
export { fontPickerKeyHandler } from "./packages/excalidraw/components/FontPicker/keyboardNavHandlers";
export { Fonts } from "./packages/excalidraw/fonts/Fonts";
export { FONT_FAMILY, FONT_TOP_PICKS_SLOTS, getFontFamilyString, arrayToList } from "@excalidraw/common";
export * as icons from "./packages/excalidraw/components/icons";
export { t } from "./packages/excalidraw/i18n";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx (the whole editor) supplies the hooks the picker reads; the shim
// answers them from globalThis.__ui.
export const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useApp: () => globalThis.__ui.app,
      useAppProps: () => globalThis.__ui.appProps,
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.editorInterface,
      useStylesPanelMode: () => globalThis.__ui.stylesPanelMode,
      useExcalidrawAppState: () => globalThis.__ui.appState,
      useExcalidrawElements: () => [],
      useExcalidrawSetAppState: () => (update) => globalThis.__ui.setAppState(update),
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

export const STUBS = ["fuzzy", "pica", "image-blob-reduce", "browser-fs-access", "packages/excalidraw/subset/subset-main"];

const usage = () => {
  process.stderr.write("usage: font-picker.mjs [--check] [--out DIR]\n");
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

class RecordingFontFace {
  constructor(family, source, descriptors = {}) {
    this.family = family;
    this.source = source;
    this.descriptors = descriptors;
    this.unicodeRange = descriptors.unicodeRange ?? "U+0-10FFFF";
  }
}

export const installDom = () => {
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
    FontFace: RecordingFontFace,
    getComputedStyle: window.getComputedStyle.bind(window),
    requestAnimationFrame: window.requestAnimationFrame.bind(window),
    cancelAnimationFrame: window.cancelAnimationFrame.bind(window),
    IS_REACT_ACT_ENVIRONMENT: true,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  window.ResizeObserver = globals.ResizeObserver;
  window.HTMLElement.prototype.scrollIntoView = function () {};
  recordStyles(window);
  return window;
};

// The inline style React (and radix) write, per declaration object, as
// they were set (see color-picker.mjs).
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

export const staticIcons = (icons) =>
  Object.entries(icons).filter(
    ([, value]) => value && value.$$typeof === Symbol.for("react.transitional.element") && value.type === "svg",
  );

/** React's useId ids (`«r0»` in React 19.0, `:r0:` before) → radix-N. */
const renameIds = (value, ids) =>
  value.replace(/radix-(«r[0-9a-z]+»|:r[0-9a-z]+:)/g, (id) => {
    if (!ids.has(id)) ids.set(id, `radix-${ids.size + 1}`);
    return ids.get(id);
  });

export const makeTree = (iconNames, ids, plainSvg = () => false) => {
  const tree = (node) => {
    if (node.nodeType === 3) return node.data;
    if (node.localName === "svg") {
      const name = iconNames.get(node.outerHTML);
      if (name) return { icon: name };
      // radix's Popover.Arrow is the one other svg
      if (!plainSvg(node) && !node.querySelector(":scope > polygon")) {
        throw new Error(`an svg that is no icons.tsx export: ${node.outerHTML.slice(0, 120)}`);
      }
    }
    const attrs = [...node.attributes]
      .filter((a) => a.name !== "style")
      .map((a) => [a.name, a.name === "id" || a.name.startsWith("aria-") ? renameIds(a.value, ids) : a.value]);
    const popper = node.hasAttribute("data-radix-popper-content-wrapper");
    const placed = node.parentElement?.hasAttribute("data-radix-popper-content-wrapper");
    const arrow =
      node.localName === "span" &&
      node.firstElementChild?.localName === "svg" &&
      !iconNames.has(node.firstElementChild.outerHTML) &&
      !plainSvg(node.firstElementChild);
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

export const F = {
  Virgil: 1,
  Helvetica: 2,
  Cascadia: 3,
  Excalifont: 5,
  Nunito: 6,
  LilitaOne: 7,
  ComicShanns: 8,
  LiberationSans: 9,
};

/**
 * A case: the host's initial state (`open`: appState.openPopup is
 * "fontFamily"; `selected`: the selection's font family, null when mixed;
 * `hovered`: currentHoveredFontFamily; `topPicks`: fontTopPicks), the
 * styles panel `mode`, the scene's families (`scene`,
 * Fonts.getSceneFamilies), `showDeprecatedFonts`, `theme`, `formFactor`,
 * `editingText`, and the `steps` to take after the first render.
 */
const CASES = [
  { name: "closed", selected: F.Excalifont },
  { name: "closed-normal", selected: F.Nunito },
  { name: "closed-code", selected: F.ComicShanns },
  { name: "closed-mixed", selected: null },
  { name: "closed-lilita", selected: F.LilitaOne },
  { name: "closed-top-picks-lilita", selected: F.LilitaOne, topPicks: [F.LilitaOne, F.Excalifont] },
  { name: "closed-top-picks-cascadia", selected: F.Nunito, topPicks: [F.Cascadia, F.ComicShanns] },
  { name: "closed-top-picks-virgil", selected: F.Virgil, topPicks: [F.Virgil] },
  { name: "closed-top-picks-helvetica-dupes", selected: F.Excalifont, topPicks: [F.Helvetica, F.Helvetica, F.Nunito, F.Excalifont, F.LilitaOne] },
  { name: "closed-compact", selected: F.Excalifont, mode: "compact" },
  { name: "closed-mobile", selected: F.Excalifont, mode: "mobile", formFactor: "phone" },
  { name: "open", selected: F.Excalifont, open: true, scene: [F.Excalifont] },
  { name: "open-scene-deprecated", selected: F.Virgil, open: true, scene: [F.Virgil, F.Excalifont, F.Cascadia, F.Helvetica] },
  { name: "open-empty-scene", selected: F.Nunito, open: true, scene: [] },
  { name: "open-show-deprecated", selected: F.Excalifont, open: true, scene: [F.Excalifont], showDeprecatedFonts: true },
  { name: "open-hovered", selected: F.Excalifont, hovered: F.LilitaOne, open: true, scene: [F.Excalifont] },
  { name: "open-mixed", selected: null, open: true, scene: [F.Excalifont, F.Nunito] },
  { name: "open-dark", selected: F.Virgil, open: true, scene: [F.Virgil], theme: "dark" },
  { name: "open-customized", selected: F.LilitaOne, open: true, scene: [F.LilitaOne], topPicks: [F.LilitaOne] },
  { name: "open-compact", selected: F.Excalifont, open: true, scene: [F.Excalifont], mode: "compact" },
  { name: "open-phone", selected: F.Excalifont, open: true, scene: [F.Excalifont], mode: "mobile", formFactor: "phone" },
  { name: "open-editing-text", selected: F.Excalifont, open: true, scene: [F.Excalifont], editingText: true },
  {
    name: "trigger-open-close",
    selected: F.Excalifont,
    scene: [F.Excalifont],
    steps: [{ click: "trigger" }, { click: "trigger" }],
  },
  {
    name: "top-pick-select",
    selected: F.Excalifont,
    scene: [F.Excalifont],
    steps: [{ click: "topPick", index: 1 }, { click: "topPick", index: 2 }, { click: "topPick", index: 0 }],
  },
  {
    name: "keys",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    steps: [
      { key: "ArrowDown" },
      { key: "ArrowDown" },
      { key: "ArrowUp" },
      { key: "ArrowUp" },
      { key: "ArrowUp" },
      { key: "ArrowUp" },
      { key: "ArrowDown" },
      { key: "F", shiftKey: true },
      { key: "a" },
      { key: "Enter" },
    ],
  },
  {
    name: "keys-escape",
    selected: F.Nunito,
    open: true,
    scene: [F.Nunito, F.Virgil],
    steps: [{ key: "ArrowDown" }, { key: "Escape" }],
  },
  {
    name: "keys-mixed",
    selected: null,
    open: true,
    scene: [],
    steps: [{ key: "Enter" }, { key: "ArrowUp" }, { key: "Enter" }],
  },
  {
    name: "hover-and-leave",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    steps: [{ hover: F.ComicShanns }, { hover: F.ComicShanns }, { hover: F.Nunito }, { leave: true }, { click: "item", value: F.LilitaOne }],
  },
  {
    name: "search",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont, F.Virgil],
    steps: [
      { search: "i" },
      { search: "  NUN " },
      { search: "zzz" },
      { search: "" },
      { search: "c" },
      { key: "ArrowDown" },
      { key: "Enter" },
    ],
  },
  {
    name: "search-deprecated",
    selected: F.Excalifont,
    open: true,
    scene: [F.Excalifont],
    showDeprecatedFonts: true,
    steps: [{ search: "vir" }, { key: "Enter" }],
  },
];

export const KEY_CODES = { ArrowDown: "ArrowDown", ArrowUp: "ArrowUp", Enter: "Enter", Escape: "Escape", F: "KeyF", a: "KeyA" };

const render = async (up, window, iconNames, c) => {
  const { React, act, createRoot, FontPicker } = up;
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  const formFactor = c.formFactor ?? "desktop";
  const mode = c.mode ?? "full";
  const theme = c.theme ?? "light";
  let calls = [];
  const record = (call) => calls.push(call);
  const initial = {
    openPopup: c.open ? "fontFamily" : null,
    selectedFontFamily: c.selected,
    currentHoveredFontFamily: c.hovered ?? null,
    fontTopPicks: c.topPicks ?? null,
  };
  let setHost = null;
  let host = initial;
  globalThis.__ui = {
    app: {
      ownerWindow: window,
      ownerDocument: document,
      fonts: { getSceneFamilies: () => c.scene ?? [] },
      state: { editingTextElement: c.editingText ? { id: "text", type: "text" } : null },
    },
    appProps: { showDeprecatedFonts: !!c.showDeprecatedFonts },
    container: { container, id: "excalidraw-id" },
    editorInterface: { formFactor, desktopUIMode: "full", userAgent: {}, isTouchScreen: false, isLandscape: false },
    stylesPanelMode: mode,
    appState: { theme },
    // FontPickerTrigger's setAppState
    setAppState: (update) => {
      const next = typeof update === "function" ? update({ openPopup: host.openPopup }) : update;
      record(["setAppState", next]);
      setHost((s) => ({ ...s, ...next }));
    },
  };
  // the host: actionChangeFontFamily's PanelComponent batches the picker's
  // callbacks into updateData, and perform merges them into the app state
  // (actionProperties.tsx:1160-1200, 1440-1540); the selection's font
  // family follows the picked one
  const Host = () => {
    const [s, set] = React.useState(initial);
    setHost = set;
    host = s;
    return React.createElement(FontPicker, {
      isOpened: s.openPopup === "fontFamily",
      selectedFontFamily: s.selectedFontFamily,
      hoveredFontFamily: s.currentHoveredFontFamily,
      topPicks: s.fontTopPicks,
      compactMode: mode !== "full",
      onTopPicksChange: (fontTopPicks) => {
        record(["topPicksChange", fontTopPicks]);
        set((p) => ({ ...p, fontTopPicks }));
      },
      onSelect: (fontFamily) => {
        record(["select", fontFamily]);
        set((p) => ({ ...p, openPopup: null, currentHoveredFontFamily: null, selectedFontFamily: fontFamily }));
      },
      onHover: (fontFamily) => {
        record(["hover", fontFamily]);
        set((p) => ({ ...p, currentHoveredFontFamily: fontFamily }));
      },
      onLeave: () => {
        record(["leave"]);
        set((p) => ({ ...p, currentHoveredFontFamily: null }));
      },
      onPopupChange: (open) => {
        record(["popupChange", open]);
        set((p) => (open ? { ...p, openPopup: "fontFamily" } : { ...p, currentHoveredFontFamily: null }));
      },
    });
  };
  const root = createRoot(container);
  const snapshot = (step) => {
    const ids = new Map();
    // radix's dismissal (focus leaving, Escape) and the list's unmount both
    // close the popup; whether both land in one step depends on timers, so
    // a callback repeated back to back is recorded once (the host's state
    // is the same either way)
    const once = calls.filter((call, i) => i === 0 || JSON.stringify(call) !== JSON.stringify(calls[i - 1]));
    const out = {
      step,
      calls: once,
      state: { ...host },
      dom: [...container.childNodes].map(makeTree(iconNames, ids)),
    };
    calls = [];
    return out;
  };
  await act(async () => root.render(React.createElement(Host)));
  const steps = [snapshot(null)];
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
  for (const step of c.steps ?? []) {
    await act(async () => {
      if (step.click === "trigger") {
        container.querySelector('[data-testid="font-family-show-fonts"]').click();
      } else if (step.click === "topPick") {
        container.querySelector(`.FontPicker__top-picks [data-top-pick-index="${step.index}"]`).click();
      } else if (step.click === "item") {
        container.querySelector(`.dropdown-menu.fonts button[value="${step.value}"]`).click();
      } else if (step.key) {
        const target = document.activeElement && container.contains(document.activeElement)
          ? document.activeElement
          : container.querySelector(".properties-content");
        target.dispatchEvent(
          new window.KeyboardEvent("keydown", {
            key: step.key,
            code: KEY_CODES[step.key],
            shiftKey: !!step.shiftKey,
            bubbles: true,
            cancelable: true,
          }),
        );
      } else if (step.hover !== undefined) {
        container
          .querySelector(`.dropdown-menu.fonts button[value="${step.hover}"]`)
          .dispatchEvent(new window.MouseEvent("mousemove", { bubbles: true }));
      } else if (step.leave) {
        // React's onPointerLeave listens for pointerout at the root
        const content = container.querySelector(".properties-content");
        content.dispatchEvent(new window.MouseEvent("pointerout", { bubbles: true, relatedTarget: document.body }));
      } else if (step.search !== undefined) {
        const input = container.querySelector(".QuickSearch__input");
        setter.call(input, step.search);
        input.dispatchEvent(new window.Event("input", { bubbles: true }));
        // QuickSearch's onChange is debounced by 20 ms
        await new Promise((r) => setTimeout(r, 40));
      }
    });
    // the host's batched update, then what the picker does with it
    await act(async () => {});
    steps.push(snapshot(step));
  }
  await act(async () => root.unmount());
  return {
    name: c.name,
    initial,
    mode,
    scene: c.scene ?? [],
    showDeprecatedFonts: !!c.showDeprecatedFonts,
    theme,
    formFactor,
    editingText: !!c.editingText,
    steps,
  };
};

// -- keyboard -------------------------------------------------------------------

const KEY_EVENTS = [
  { key: "ArrowDown" },
  { key: "ArrowUp" },
  { key: "Enter" },
  { key: "Escape" },
  { key: "F", shiftKey: true },
  { key: "f", shiftKey: true },
  { key: "F", shiftKey: true, ctrlKey: true },
  { key: "F", shiftKey: true, metaKey: true },
  { key: "f" },
  { key: "a" },
  { key: "Tab" },
];

const LISTS = [[], [F.Excalifont], [F.Excalifont, F.Nunito, F.LilitaOne]];

const keyNav = (up) => {
  const out = [];
  for (const list of LISTS) {
    const filtered = up.arrayToList(list.map((value) => ({ value })));
    for (const hovered of [null, ...list.map((_, i) => i)]) {
      for (const ev of KEY_EVENTS) {
        const calls = [];
        let prevented = false;
        const event = {
          key: ev.key,
          shiftKey: !!ev.shiftKey,
          ctrlKey: !!ev.ctrlKey,
          metaKey: !!ev.metaKey,
          altKey: false,
          preventDefault: () => (prevented = true),
          stopPropagation: () => {},
        };
        const handled = up.fontPickerKeyHandler({
          event,
          inputRef: { current: { focus: () => calls.push(["focusSearch"]) } },
          hoveredFont: hovered === null ? undefined : filtered[hovered],
          filteredFonts: filtered,
          onClose: () => calls.push(["close"]),
          onSelect: (v) => calls.push(["select", v]),
          onHover: (v) => calls.push(["hover", v]),
        });
        const mods = ["shift", "ctrl", "meta"].filter((m) => ev[`${m}Key`]);
        out.push({
          list,
          hovered: hovered === null ? null : list[hovered],
          key: ev.key,
          ...(mods.length ? { mods } : {}),
          handled: !!handled,
          calls,
          ...(prevented ? { prevented } : {}),
        });
      }
    }
  }
  return out;
};

// -- constants ------------------------------------------------------------------

const constants = (up, iconName) => ({
  FONT_TOP_PICKS_SLOTS: up.FONT_TOP_PICKS_SLOTS,
  DEFAULT_FONTS: up.DEFAULT_FONTS.map((f) => ({ value: f.value, text: f.text, testId: f.testId, icon: iconName(f.icon) })),
  families: [...up.Fonts.registered.entries()].map(([id, { metadata, fontFaces }]) => ({
    id,
    icon: iconName(up.getFontFamilyIcon(id)),
    label: up.getFontFamilyLabel(id, fontFaces),
    css: up.getFontFamilyString({ fontFamily: id }),
    deprecated: !!metadata.deprecated,
    private: !!metadata.private,
    fallback: !!metadata.fallback,
  })),
});

// -- stylesheet ---------------------------------------------------------------

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const entry = STYLESHEETS.map((path) => `@use "${path.replace(/\.scss$/, "")}";\n`).join("");
  const compiled = sass.compileString(entry, { loadPaths: [root], style: "expanded" }).css;
  return (
    "/* Generated by tools/goldens/font-picker.mjs; do not edit. Upstream's\n" +
    ` * ${STYLESHEETS.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded) as one entry that\n" +
    " * @uses each in this order. */\n" +
    `${compiled}\n`
  );
};

// -- main -----------------------------------------------------------------------

const LOCALE_KEYS = [
  "labels.handDrawn",
  "labels.normal",
  "labels.code",
  "labels.showFonts",
  "labels.fontFamily",
  "quickSearch.placeholder",
  "fontList.badge.old",
  "fontList.sceneFonts",
  "fontList.availableFonts",
  "fontList.empty",
  "fontList.topPicksTip",
  "fontList.resetTopPicks",
  "buttons.reset",
];

// FontPickerList calls onHover/onLeave while rendering when a search
// leaves nothing hovered (FontPickerList.tsx:199-214), which React reports
// as a setState in render; that is upstream's behaviour, not a failure.
export const quietSetStateInRender = () => {
  const error = console.error;
  console.error = (...args) => {
    if (typeof args[0] === "string" && args[0].startsWith("Cannot update a component")) return;
    error(...args);
  };
  return () => (console.error = error);
};

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const restoreConsole = quietSetStateInRender();
  const window = installDom();
  installTouchCallout(window);
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    jsx: "automatic",
    fontUris: true,
    define: {
      "import.meta.env.MODE": '"production"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  const iconNames = new Map();
  const iconByElement = new Map();
  for (const [name, value] of staticIcons(up.icons)) {
    const host = window.document.createElement("div");
    const root = up.createRoot(host);
    await up.act(async () => root.render(value));
    const markup = host.innerHTML;
    await up.act(async () => root.unmount());
    if (!iconByElement.has(value)) iconByElement.set(value, name);
    if (iconNames.has(markup)) continue;
    iconNames.set(markup, name);
  }
  const iconName = (element) => {
    const name = iconByElement.get(element);
    if (!name) throw new Error("an icon that is no icons.tsx export");
    return name;
  };
  const cases = [];
  for (const c of CASES) cases.push(await render(up, window, iconNames, c));
  window.close();
  restoreConsole();
  const fixture = {
    upstream: upstream.commit,
    locale: Object.fromEntries(LOCALE_KEYS.map((k) => [k, up.t(k)])),
    constants: constants(up, iconName),
    isDefaultFont: [null, 0, ...Object.values(F), 100, 1000].map((v) => ({ value: v, result: up.isDefaultFont(v) })),
    keyNav: keyNav(up),
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
    process.stderr.write(`font-picker: ${error.message}\n`);
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
      process.stderr.write("font picker goldens are out of date: run node tools/goldens/font-picker.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`font picker goldens up to date: ${Object.keys(files).length} files\n`);
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
