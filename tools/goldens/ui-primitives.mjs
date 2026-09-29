#!/usr/bin/env node
// UI primitive goldens for excali-ui (ex-516): upstream's own Island,
// Stack, Button, IconButton (the ToolIcon button), RadioGroup, Range,
// TextField, Popover, Modal, Dialog and Tooltip
// (packages/excalidraw/components/*.tsx) rendered by React 19.0.0 (the
// version excalidraw-app pins) into jsdom 22.1.0, and their stylesheets
// compiled from upstream's SCSS with sass 1.51.0 (the version
// packages/excalidraw pins).
//
//   node tools/goldens/ui-primitives.mjs            write the fixture and CSS
//   node tools/goldens/ui-primitives.mjs --check    exit 1 if either is stale
//   node tools/goldens/ui-primitives.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/ui-primitives.json:
//
// - `tokens`, `largeScreenTokens`: the sizing custom properties theme.scss
//   declares on `.excalidraw` (button and icon sizes, the space factor, the
//   radii), and those its `min-device-width: 1921px` query redeclares;
// - `render`: per case the component, its props (children as text or a
//   `{tag, className, text}` element) and the DOM React leaves: the
//   component's root (for Modal and Dialog, the portal container appended
//   to the body), as a tree of `{tag, attrs, style, children}` with the
//   attributes sorted by name and the inline style as a map (the order of
//   neither is observable), `props` holding an input's `value` and
//   `checked` properties;
// - `tooltipPosition`: updateTooltipPosition on item and tooltip rects in a
//   viewport, the `top` and `left` it writes;
// - `showTooltip`: showTooltip (undelayed) on an item: the tooltip div's
//   class, inline style and text;
// - `popoverFit`: Popover with `fitInViewport` for a content size: the
//   inline style its layout effect writes;
// - `focusTrap`: the Tab handling of Popover and Dialog on focusable counts,
//   the index focused and whether the key was prevented.
//
// and crates/excali-ui/src/primitives/primitives.css: theme.scss (the
// light and dark custom properties, sizes included) and the primitives' own
// stylesheets (Island.scss ... Tooltip.scss) compiled in `expanded` style as
// one entry, in the order of STYLESHEETS.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "ui-primitives.json");
export const STYLESHEET = join("src", "primitives", "primitives.css");

const ENTRY = `
export { Island } from "./packages/excalidraw/components/Island";
export { default as Stack } from "./packages/excalidraw/components/Stack";
export { Button } from "./packages/excalidraw/components/Button";
export { IconButton } from "./packages/excalidraw/components/IconButton";
export { RadioGroup } from "./packages/excalidraw/components/RadioGroup";
export { Range } from "./packages/excalidraw/components/Range";
export { TextField } from "./packages/excalidraw/components/TextField";
export { Popover } from "./packages/excalidraw/components/Popover";
export { Modal } from "./packages/excalidraw/components/Modal";
export { Dialog } from "./packages/excalidraw/components/Dialog";
export {
  Tooltip,
  showTooltip,
  hideTooltip,
  updateTooltipPosition,
} from "./packages/excalidraw/components/Tooltip";
export { UIAppStateContext } from "./packages/excalidraw/context/ui-appState";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

// App.tsx (the whole editor) supplies three hooks to Dialog and
// useCreatePortalContainer: the editor container and its id, the form
// factor, and setAppState. The shim answers them from globalThis.__ui.
const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useExcalidrawContainer: () => globalThis.__ui.container,
      useEditorInterface: () => globalThis.__ui.editorInterface,
      useExcalidrawSetAppState: () => () => {},
    };`,
  // jotai-scope's isolation, for Dialog's useSetAtom (called on close only)
  // and i18n's language atom (read by useI18n, which nothing here calls).
  "packages/excalidraw/editor-jotai": `
    const atom = (init) => ({ init });
    module.exports = {
      atom,
      useSetAtom: () => () => {},
      useAtomValue: (a) => a.init,
      useAtom: (a) => [a.init, () => {}],
      editorJotaiStore: { get: (a) => a.init, set: () => {}, sub: () => () => {} },
    };`,
  "packages/excalidraw/components/LibraryMenu": `module.exports = { isLibraryMenuOpenAtom: { init: false } };`,
};

// Upstream's SCSS for the primitives, in this order in primitives.css.
export const STYLESHEETS = [
  "components/Island.scss",
  "components/Stack.scss",
  "components/Button.scss",
  "components/ToolIcon.scss",
  "components/RadioGroup.scss",
  "components/Range.scss",
  "components/TextField.scss",
  "components/Popover.scss",
  "components/Modal.scss",
  "components/Dialog.scss",
  "components/Tooltip.scss",
];

// The sizing tokens (theme.scss:46-49, 79, 148-149 and 171-176).
export const SIZE_TOKENS = [
  "--default-button-size",
  "--default-icon-size",
  "--lg-button-size",
  "--lg-icon-size",
  "--space-factor",
  "--border-radius-md",
  "--border-radius-lg",
];

const usage = () => {
  process.stderr.write("usage: ui-primitives.mjs [--check] [--out DIR]\n");
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

// -- cases --------------------------------------------------------------------

const icon = { tag: "svg", className: "icon", text: "" };

const RENDER_CASES = [
  ["island-default", "Island", { children: ["content"] }],
  ["island-padding", "Island", { padding: 2, className: "App-toolbar", children: ["content"] }],
  ["island-false-class", "Island", { padding: 1, className: false, children: ["a", "b"] }],
  [
    "island-viewport-ui",
    "Island",
    {
      padding: 1,
      className: "App-menu__left",
      style: { "--padding": 3 },
      "data-viewport-ui": "left",
      "data-viewport-ui-name": "styles-panel",
      children: [{ tag: "span", className: "x", text: "styles" }],
    },
  ],
  ["stack-row", "Stack.Row", { children: ["a"] }],
  ["stack-row-gap", "Stack.Row", { gap: 1, align: "center", justifyContent: "space-between", className: "shapes", children: ["a", "b"] }],
  ["stack-col", "Stack.Col", { gap: 4, children: ["a"] }],
  ["stack-col-align", "Stack.Col", { gap: 2, align: "start", justifyContent: "center", className: "panel", children: ["a"] }],
  ["stack-col-false-class", "Stack.Col", { className: false, align: "end", children: ["a"] }],
  ["button", "Button", { children: ["Save"] }],
  ["button-selected", "Button", { selected: true, className: "Card-button", children: ["Open"] }],
  ["button-submit", "Button", { type: "submit", title: "Submit", selected: false, children: ["Go"] }],
  ["button-style", "Button", { style: { border: 0, userSelect: "none" }, children: [icon] }],
  ["icon-button", "IconButton", { type: "button", "aria-label": "Undo", icon }],
  [
    "icon-button-full",
    "IconButton",
    {
      type: "button",
      "aria-label": "Zoom in",
      title: "Zoom in — Ctrl+",
      icon,
      size: "small",
      className: "zoom-in-button",
      keyBindingLabel: "Z",
      showAriaLabel: true,
      "data-testid": "zoom-in",
      children: ["extra"],
    },
  ],
  ["icon-button-label", "IconButton", { type: "button", "aria-label": "Label", label: "L" }],
  ["icon-button-hidden", "IconButton", { type: "button", "aria-label": "Hidden", icon, hidden: true }],
  ["icon-button-invisible", "IconButton", { type: "button", "aria-label": "Invisible", icon, visible: false }],
  ["icon-button-disabled", "IconButton", { type: "button", "aria-label": "Disabled", icon, disabled: true }],
  ["icon-button-plain", "IconButton", { type: "icon", "aria-label": "Plain", icon, style: { marginLeft: 4 } }],
  ["icon-button-no-icon", "IconButton", { type: "button", "aria-label": "Nothing", showAriaLabel: true }],
  ["tool-toggle", "IconButton", { type: "toggle", "aria-label": "Rectangle", checked: false, icon }],
  [
    "tool-toggle-checked",
    "IconButton",
    {
      type: "toggle",
      "aria-label": "Rectangle — R or 2",
      "aria-keyshortcuts": "R 2",
      title: "Rectangle — R or 2",
      checked: true,
      icon,
      keyBindingLabel: "2",
      className: "Shape fillable",
      "data-testid": "toolbar-rectangle",
      size: "medium",
    },
  ],
  ["tool-toggle-small-disabled", "IconButton", { type: "toggle", "aria-label": "Lock", checked: false, icon, size: "small", disabled: true }],
  [
    "radio-group",
    "RadioGroup",
    {
      name: "canvasBackground",
      value: "b",
      choices: [
        { value: "a", label: "A", ariaLabel: "First" },
        { value: "b", label: "B", ariaLabel: "Second" },
        { value: "c", label: "C" },
      ],
    },
  ],
  ["radio-group-none", "RadioGroup", { name: "g", value: "z", choices: [{ value: "x", label: "X" }] }],
  ["range", "Range", { label: "Opacity", value: 50 }],
  ["range-min", "Range", { label: "Opacity", value: 0 }],
  ["range-full", "Range", { label: "Roundness", value: 30, min: 10, max: 60, step: 5, minLabel: "min", hasCommonValue: false, testId: "roundness" }],
  ["range-degenerate", "Range", { label: "Degenerate", value: 7, min: 7, max: 7 }],
  ["range-fraction", "Range", { label: "Third", value: 1, min: 0, max: 3, step: 1 }],
  ["text-field", "TextField", { value: "hello" }],
  ["text-field-default", "TextField", { defaultValue: "seed", placeholder: "Search…", type: "search" }],
  [
    "text-field-full",
    "TextField",
    { value: "x", label: "Link", fullWidth: true, readonly: true, className: "ShareableLink", icon, placeholder: "p", type: "text" },
  ],
  ["text-field-redacted", "TextField", { value: "secret", isRedacted: true }],
  ["text-field-redacted-empty", "TextField", { value: "", isRedacted: true }],
  ["popover", "Popover", { children: ["menu"] }],
  ["popover-class", "Popover", { className: "color-picker-popover", top: 10, left: 20, children: ["menu"] }],
  ["modal", "Modal", { labelledBy: "dialog-title", maxWidth: 800, children: ["body"] }],
  ["modal-class", "Modal", { className: "HelpDialog", labelledBy: "x", children: ["body"] }],
  ["modal-dark-phone", "Modal", { labelledBy: "x", maxWidth: 550, children: ["body"] }, { theme: "dark", formFactor: "phone" }],
  ["dialog", "Dialog", { title: "Help", children: ["content"] }],
  ["dialog-small", "Dialog", { title: "Export", size: "small", className: "ExportDialog", children: ["content"] }],
  ["dialog-wide", "Dialog", { title: "Library", size: "wide", children: ["content"] }],
  ["dialog-number", "Dialog", { title: "Custom", size: 640, children: ["content"] }],
  ["dialog-no-title", "Dialog", { title: false, size: "regular", children: ["content"] }],
  ["dialog-phone", "Dialog", { title: "Help", children: ["content"] }, { formFactor: "phone" }],
  ["dialog-dark", "Dialog", { title: "Help", children: ["content"] }, { theme: "dark" }],
  ["tooltip", "Tooltip", { label: "Hi", children: ["target"] }],
  ["tooltip-class", "Tooltip", { label: "Hi", long: true, className: "tip", style: { width: "100%" }, children: ["target"] }],
  ["tooltip-disabled", "Tooltip", { label: "Hi", disabled: true, children: ["target"] }],
];

/** [item {left, top, width, height}, tooltip [width, height], viewport [w, h], position] */
const TOOLTIP_POSITIONS = [
  [{ left: 100, top: 100, width: 32, height: 32 }, [80, 20], [1024, 768], "bottom"],
  [{ left: 100, top: 100, width: 32, height: 32 }, [80, 20], [1024, 768], "top"],
  [{ left: 0, top: 10, width: 20, height: 20 }, [80, 20], [1024, 768], "bottom"],
  [{ left: 1000, top: 10, width: 20, height: 20 }, [80, 20], [1024, 768], "bottom"],
  [{ left: 990, top: 10, width: 20, height: 20 }, [40, 20], [1024, 768], "bottom"],
  [{ left: 500, top: 740, width: 20, height: 20 }, [80, 20], [1024, 768], "bottom"],
  [{ left: 500, top: 738, width: 20, height: 5 }, [80, 20], [1024, 768], "bottom"],
  [{ left: 500, top: 10, width: 20, height: 20 }, [80, 20], [1024, 768], "top"],
  [{ left: 500, top: 25, width: 20, height: 20 }, [80, 20], [1024, 768], "top"],
  [{ left: 500, top: 26, width: 20, height: 20 }, [80, 21], [1024, 768], "top"],
  [{ left: 38.5, top: 12.25, width: 36.4, height: 36.4 }, [101.3, 27.7], [390, 844], "bottom"],
  [{ left: 20, top: 100, width: 10, height: 10 }, [51, 20], [390, 844], "bottom"],
  [{ left: 0, top: 0, width: 1, height: 1 }, [2, 2], [1, 1], "bottom"],
];

/** [item rect, label, {long, position}] */
const SHOW_TOOLTIPS = [
  [{ left: 100, top: 100, width: 32, height: 32 }, "Rectangle — R or 2", {}],
  [{ left: 100, top: 100, width: 32, height: 32 }, "A long description of a tool", { long: true }],
  [{ left: 300, top: 700, width: 32, height: 32 }, "Up", { position: "top" }],
];

/** [top, left, content [w, h], viewport [w, h]] */
const POPOVER_FITS = [
  [100, 200, [300, 200], [1024, 768]],
  [5, 5, [300, 200], [1024, 768]],
  [700, 900, [300, 200], [1024, 768]],
  [100, 100, [1100, 200], [1024, 768]],
  [100, 100, [300, 800], [1024, 768]],
  [100, 100, [1004, 748], [1024, 768]],
  [100, 100, [1003.5, 747.5], [1024, 768]],
  [12.5, -40.25, [120.5, 60.75], [390, 844]],
  [0, 0, [10, 10], [15, 15]],
];

/** [focusable count, index of the focused one (-1 the container), shift] */
const FOCUS_TRAPS = [
  [3, -1, false],
  [3, -1, true],
  [3, 0, true],
  [3, 0, false],
  [3, 1, false],
  [3, 2, false],
  [3, 2, true],
  [1, 0, false],
  [1, 0, true],
];

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
    SVGElement: window.SVGElement,
    MutationObserver: window.MutationObserver,
    requestAnimationFrame: window.requestAnimationFrame.bind(window),
    cancelAnimationFrame: window.cancelAnimationFrame.bind(window),
    IS_REACT_ACT_ENVIRONMENT: true,
  };
  for (const [key, value] of Object.entries(globals)) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  return window;
};

/** An element's inline style, property → value. */
const styleOf = (node) => {
  const style = node.style;
  if (!style) return {};
  return sorted(Array.from({ length: style.length }, (_, i) => style.item(i)).map((p) => [p, style.getPropertyValue(p)]));
};

const sorted = (entries) => Object.fromEntries([...entries].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));

/** An element as {tag, attrs, style, children}; text as a string. */
const tree = (node) => {
  if (node.nodeType === 3) return node.data;
  const out = {
    tag: node.localName,
    attrs: sorted([...node.attributes].filter((a) => a.name !== "style").map((a) => [a.name, a.value])),
    style: styleOf(node),
  };
  if (node.localName === "input") out.props = { value: node.value, checked: node.checked };
  out.children = [...node.childNodes].filter((c) => c.nodeType === 1 || c.nodeType === 3).map(tree);
  return out;
};

const childElement = (React, spec) =>
  typeof spec === "string" ? spec : React.createElement(spec.tag, { className: spec.className }, spec.text || undefined);

/** The props React receives: children and icon specs as elements. */
const reactProps = (React, props) => {
  const out = { ...props };
  if (props.children) {
    out.children = props.children.map((c, i) =>
      typeof c === "string" ? c : React.cloneElement(childElement(React, c), { key: i }),
    );
  }
  if (props.icon) out.icon = childElement(React, props.icon);
  if (props.choices) out.choices = props.choices.map((c) => ({ ...c }));
  if (props.label && typeof props.label === "object") out.label = childElement(React, props.label);
  return out;
};

const component = (up, name) => (name.startsWith("Stack.") ? up.Stack[name.slice(6)] : up[name]);

/** The callbacks each component requires. */
const HANDLERS = {
  Button: { onSelect: () => {} },
  RadioGroup: { onChange: () => {} },
  Range: { onChange: () => {} },
  TextField: { onChange: () => {} },
  Modal: { onCloseRequest: () => {} },
  Dialog: { onCloseRequest: () => {} },
};

const uiAppState = (theme) => ({ theme, openMenu: null });

const render = async (up, window, [name, componentName, props, env = {}]) => {
  const { React, act, createRoot, UIAppStateContext } = up;
  const document = window.document;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  globalThis.__ui = {
    container: { container, id: "excalidraw-id" },
    editorInterface: { formFactor: env.formFactor ?? "desktop" },
  };
  const root = createRoot(container);
  const element = React.createElement(
    UIAppStateContext.Provider,
    { value: uiAppState(env.theme ?? "light") },
    React.createElement(component(up, componentName), {
      ...HANDLERS[componentName],
      ...reactProps(React, props),
    }),
  );
  await act(async () => root.render(element));
  const portal = componentName === "Modal" || componentName === "Dialog";
  const nodes = portal ? [document.body.lastElementChild] : [...container.childNodes];
  const dom = nodes.map(tree);
  await act(async () => root.unmount());
  return { name, component: componentName, props, env, dom };
};

const rect = ({ left, top, width, height }) => ({
  left,
  top,
  width,
  height,
  x: left,
  y: top,
  right: left + width,
  bottom: top + height,
});

const setViewport = (window, [width, height]) => {
  Object.defineProperty(window, "innerWidth", { value: width, configurable: true });
  Object.defineProperty(window, "innerHeight", { value: height, configurable: true });
};

const tooltipPositions = (up, window) =>
  TOOLTIP_POSITIONS.map(([item, [width, height], viewport, position]) => {
    setViewport(window, viewport);
    const tooltip = window.document.createElement("div");
    tooltip.getBoundingClientRect = () => rect({ left: 0, top: 0, width, height });
    up.updateTooltipPosition(tooltip, item, position);
    return {
      item,
      tooltip: { width, height },
      viewport: { width: viewport[0], height: viewport[1] },
      position,
      top: tooltip.style.top,
      left: tooltip.style.left,
    };
  });

const showTooltips = (up, window) => {
  setViewport(window, [1024, 768]);
  return SHOW_TOOLTIPS.map(([item, label, options]) => {
    const { document } = window;
    document.body.innerHTML = "";
    const tooltip = document.createElement("div");
    tooltip.classList.add("excalidraw-tooltip");
    tooltip.getBoundingClientRect = () => rect({ left: 0, top: 0, width: 80, height: 20 });
    document.body.appendChild(tooltip);
    const target = document.createElement("button");
    target.getBoundingClientRect = () => rect(item);
    document.body.appendChild(target);
    up.showTooltip(target, label, options);
    const shown = tree(tooltip);
    up.hideTooltip();
    const hidden = tree(tooltip);
    return { item, label, options, tooltipSize: { width: 80, height: 20 }, shown, hidden };
  });
};

const popoverFits = async (up, window) => {
  const out = [];
  for (const [top, left, [width, height], [viewportWidth, viewportHeight]] of POPOVER_FITS) {
    const { React, act, createRoot } = up;
    const { document } = window;
    document.body.innerHTML = "";
    const container = document.createElement("div");
    document.body.appendChild(container);
    const original = window.HTMLElement.prototype.getBoundingClientRect;
    window.HTMLElement.prototype.getBoundingClientRect = function () {
      return this.classList.contains("popover") ? rect({ left: 0, top: 0, width, height }) : original.call(this);
    };
    const root = createRoot(container);
    await act(async () =>
      root.render(React.createElement(up.Popover, { top, left, fitInViewport: true, viewportWidth, viewportHeight })),
    );
    const popover = container.firstElementChild;
    out.push({
      top,
      left,
      content: { width, height },
      viewport: { width: viewportWidth, height: viewportHeight },
      style: styleOf(popover),
    });
    await act(async () => root.unmount());
    window.HTMLElement.prototype.getBoundingClientRect = original;
  }
  return out;
};

/** Tab on Popover's container (Popover.tsx:52-83) and Dialog's island (Dialog.tsx:70-89). */
const focusTraps = async (up, window) => {
  const { React, act, createRoot, UIAppStateContext } = up;
  const out = [];
  for (const which of ["Popover", "Dialog"]) {
    for (const [count, focused, shift] of FOCUS_TRAPS) {
      const { document } = window;
      document.body.innerHTML = "";
      const container = document.createElement("div");
      container.className = "excalidraw";
      document.body.appendChild(container);
      globalThis.__ui = { container: { container, id: "excalidraw-id" }, editorInterface: { formFactor: "desktop" } };
      const buttons = Array.from({ length: count }, (_, i) => React.createElement("button", { key: i, "data-i": i }, `b${i}`));
      const root = createRoot(container);
      const props = which === "Dialog" ? { title: false, autofocus: false, onCloseRequest: () => {} } : {};
      await act(async () =>
        root.render(
          React.createElement(
            UIAppStateContext.Provider,
            { value: uiAppState("light") },
            React.createElement(up[which], props, ...buttons),
          ),
        ),
      );
      const trap = which === "Popover" ? container.querySelector(".popover") : document.querySelector(".Island");
      const focusables = [...trap.querySelectorAll("button")];
      if (focused === -1) trap.focus();
      else focusables[focused].focus();
      const event = new window.KeyboardEvent("keydown", { key: "Tab", shiftKey: shift, bubbles: true, cancelable: true });
      (document.activeElement === document.body ? trap : document.activeElement).dispatchEvent(event);
      const active = document.activeElement;
      out.push({
        component: which,
        count,
        focused,
        shift,
        // -1: focus stayed on the container, null: on no focusable (the
        // Dialog's Island takes no focus, so the body keeps it)
        result: active === trap ? -1 : focusables.includes(active) ? focusables.indexOf(active) : null,
        prevented: event.defaultPrevented,
      });
      await act(async () => root.unmount());
    }
  }
  return out;
};

// -- stylesheets --------------------------------------------------------------

const compile = async (upstream, path) => {
  const sass = (await import("sass")).default;
  return sass.compile(join(upstream.dir, "packages", "excalidraw", path), { style: "expanded" }).css;
};

/** The declarations of `.excalidraw` rules in compiled CSS, top level and in @media blocks. */
const tokenBlocks = (css) => {
  const blocks = { top: {}, media: {} };
  let depth = 0;
  let media = null;
  let rule = null;
  for (const raw of css.split("\n")) {
    const line = raw.trim();
    if (line.endsWith("{")) {
      const selector = line.slice(0, -1).trim();
      if (selector.startsWith("@media")) media = selector;
      else rule = selector;
      depth++;
      continue;
    }
    if (line === "}") {
      depth--;
      if (rule !== null) rule = null;
      else media = null;
      continue;
    }
    const m = line.match(/^(--[\w-]+):\s*(.*);$/);
    if (m && rule === ".excalidraw") {
      const target = media ? (blocks.media[media] ??= {}) : blocks.top;
      target[m[1]] = m[2];
    }
  }
  return blocks;
};

const LARGE_SCREEN = "@media screen and (min-device-width: 1921px)";

/**
 * The primitives' stylesheet: theme.scss and STYLESHEETS compiled as one
 * Sass entry that `@use`s each in turn, so theme.scss (which Button.scss
 * `@use`s too) is emitted once, first.
 */
const stylesheets = async (upstream) => {
  const theme = tokenBlocks(await compile(upstream, "css/theme.scss"));
  const pick = (block) => Object.fromEntries(SIZE_TOKENS.filter((t) => t in block).map((t) => [t, block[t]]));
  const tokens = pick(theme.top);
  const large = theme.media[LARGE_SCREEN];
  if (!large) throw new Error(`theme.scss changed: no ${LARGE_SCREEN} block`);
  const largeScreenTokens = pick(large);
  if (Object.keys(tokens).length !== SIZE_TOKENS.length) throw new Error("theme.scss changed: a size token is missing");
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const sources = ["css/theme.scss", ...STYLESHEETS];
  const entry = sources.map((path) => `@use "${path.replace(/\.scss$/, "")}";\n`).join("");
  const compiled = sass.compileString(entry, { loadPaths: [root], style: "expanded" }).css;
  const css =
    "/* Generated by tools/goldens/ui-primitives.mjs; do not edit. Upstream's\n" +
    ` * ${sources.map((p) => `packages/excalidraw/${p}`).join(",\n * ")}\n` +
    " * at the pin, compiled with sass 1.51.0 (expanded) as one entry that\n" +
    " * @uses each in this order. */\n" +
    `${compiled}\n`;
  return { tokens, largeScreenTokens, css };
};

// -- main -----------------------------------------------------------------------

const build = async (upstream) => {
  // dart-sass takes a global `window` for a browser, so compile first
  const styles = await stylesheets(upstream);
  const window = installDom();
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    shims: SHIMS,
    jsx: "automatic",
    define: { "import.meta.env.MODE": '"production"' },
  });
  const renders = [];
  for (const c of RENDER_CASES) renders.push(await render(up, window, c));
  const fixture = {
    upstream: upstream.commit,
    ...styles,
    render: renders,
    tooltipPosition: tooltipPositions(up, window),
    showTooltip: showTooltips(up, window),
    popoverFit: await popoverFits(up, window),
    focusTrap: await focusTraps(up, window),
  };
  window.close();
  const { css, ...json } = fixture;
  return { [FIXTURE]: format(json), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`ui-primitives: ${error.message}\n`);
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
      process.stderr.write("ui-primitives goldens are out of date: run node tools/goldens/ui-primitives.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`ui-primitives goldens up to date: ${Object.keys(files).length} files\n`);
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
  // React's act() queues its tasks on MessageChannels when it cannot
  // require("timers") (as in this bundle), and their ports stay open.
  process.exit(0);
}
