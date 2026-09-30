#!/usr/bin/env node
// Hyperlink editor goldens for excali-ui (ex-543): upstream's own Hyperlink
// (packages/excalidraw/components/hyperlink/Hyperlink.tsx, buttons from
// IconButton.tsx) rendered by React 19.0.0 into jsdom 22.1.0 as App mounts
// it (App.tsx:2549-2565: one element selected, showHyperlinkPopup set),
// driven through its input and buttons, and its stylesheet compiled from
// upstream's SCSS with sass 1.51.0.
//
//   node tools/goldens/hyperlink.mjs            write the fixture and CSS
//   node tools/goldens/hyperlink.mjs --check    exit 1 if either is stale
//   node tools/goldens/hyperlink.mjs --out DIR  write (or --check) in DIR
//
// Writes crates/excali-ui/tests/fixtures/hyperlink.json:
//
// - `cases`: per case the element, the app state keys over the defaults
//   (showHyperlinkPopup, scroll, zoom, offsets), and per step (the first is
//   the mount) what the step did and then: the DOM React leaves in the
//   editor container as tools/goldens/toolbar.mjs records it
//   (`{tag, attrs, style, children}`, an icon's <svg> as `{icon: name}`),
//   the input's value, the element's link and showHyperlinkPopup, and for
//   a key whether the input's handler prevented its default.
//
// Steps: `{type: text}` (the input's value, an input event), `{key, ctrl}`
// (a keydown on the input), `{click: "edit" | "remove"}`, and `{close:
// true}` (showHyperlinkPopup false, which unmounts it as App does).
//
// and crates/excali-ui/src/hyperlink/hyperlink.css: Hyperlink.scss
// compiled (expanded).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { installDom, makeTree, staticIcons } from "./toolbar.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-ui");
export const FIXTURE = join("tests", "fixtures", "hyperlink.json");
export const STYLESHEET = join("src", "hyperlink", "hyperlink.css");

const SCSS = "components/hyperlink/Hyperlink.scss";

const ENTRY = `
export { Hyperlink } from "./packages/excalidraw/components/hyperlink/Hyperlink";
export * as icons from "./packages/excalidraw/components/icons";
export { Scene } from "./packages/element/src/Scene";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export { reseed } from "./packages/common/src/random";
export {
  ROUNDNESS,
  DEFAULT_VERTICAL_ALIGN,
  getStrokeWidthByKey,
  getUpdatedTimestamp,
} from "./packages/common/src/index";
export {
  newElement,
  newEmbeddableElement,
  newIframeElement,
  newStickyNoteElement,
  newFrameElement,
  newMagicFrameElement,
  newTextElement,
  newArrowElement,
  newLinearElement,
  newFreeDrawElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { default as React } from "react";
export { act } from "react";
export { createRoot } from "react-dom/client";
`;

const SHIMS = {
  "packages/excalidraw/components/App": `
    module.exports = {
      useAppProps: () => ({}),
      useEditorInterface: () => ({ formFactor: "desktop", isTouchScreen: false }),
      useExcalidrawAppState: () => globalThis.__ui.state,
    };`,
  "packages/excalidraw/editor-jotai": "module.exports = { atom: (init) => ({ init }) };",
  "packages/excalidraw/analytics": "module.exports = { trackEvent: () => {} };",
};

const usage = () => {
  process.stderr.write("usage: hyperlink.mjs [--check] [--out DIR]\n");
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

const clone = (value) => (value === undefined ? null : JSON.parse(JSON.stringify(value)));

const el = (up, opts) => apiCreateElement(up, { roughness: 0, ...opts });

// API.createElement leaves link out: set on the new element
const withLink = (element, link) => (link === undefined ? element : Object.assign(element, { link }));

const rect = (up, { link, ...extra } = {}) =>
  withLink(el(up, { type: "rectangle", id: "r", x: 10, y: 20, width: 100, height: 60, ...extra }), link);

/** { name, element(up), appState, steps } */
const CASES = [
  {
    name: "create",
    element: (up) => rect(up),
    appState: { showHyperlinkPopup: "editor" },
    steps: [{ type: "excalidraw.com" }, { key: "Enter" }, { click: "edit" }, { type: "  https://example.com/a?b=c  " }, { key: "Enter" }],
  },
  {
    name: "edit-existing",
    element: (up) => rect(up, { link: "https://excalidraw.com" }),
    appState: { showHyperlinkPopup: "editor" },
    steps: [{ key: "k", ctrl: true }, { type: "https://docs.excalidraw.com" }, { key: "Escape" }],
  },
  {
    name: "clear-by-empty-input",
    element: (up) => rect(up, { link: "https://excalidraw.com" }),
    appState: { showHyperlinkPopup: "editor" },
    steps: [{ type: "   " }, { key: "Enter" }],
  },
  {
    name: "remove",
    element: (up) => rect(up, { link: "https://excalidraw.com" }),
    appState: { showHyperlinkPopup: "info", scrollX: 30, scrollY: -12.5, zoom: { value: 1.5 }, offsetLeft: 20, offsetTop: 40 },
    steps: [{ click: "remove" }],
  },
  {
    name: "remove-while-editing",
    element: (up) => rect(up, { link: "https://excalidraw.com" }),
    appState: { showHyperlinkPopup: "editor" },
    steps: [{ type: "https://docs.excalidraw.com" }, { click: "remove" }],
  },
  {
    name: "info-without-link",
    element: (up) => rect(up),
    appState: { showHyperlinkPopup: "info" },
    steps: [{ click: "edit" }, { type: "example.com" }, { close: true }],
  },
  {
    name: "sanitized",
    element: (up) => rect(up),
    appState: { showHyperlinkPopup: "editor" },
    steps: [{ type: 'javascript:alert("x")' }, { key: "Enter" }, { click: "edit" }, { type: 'https://a.com/"q"' }, { key: "Enter" }],
  },
  {
    name: "close-without-typing",
    element: (up) => rect(up, { link: "https://excalidraw.com" }),
    appState: { showHyperlinkPopup: "editor" },
    steps: [{ key: "a" }, { close: true }],
  },
  {
    name: "arrow-zoomed-out",
    element: (up) => el(up, { type: "arrow", id: "a", x: 200, y: 150, points: [[0, 0], [-120, -80]] }),
    appState: { showHyperlinkPopup: "editor", zoom: { value: 0.5 }, scrollX: -40, scrollY: 10 },
    steps: [{ type: "https://excalidraw.com" }, { key: "Enter" }],
  },
  {
    name: "rotated",
    element: (up) => rect(up, { x: 100, y: 100, width: 120, height: 40, angle: 0.5 }),
    appState: { showHyperlinkPopup: "editor" },
    steps: [],
  },
  {
    name: "local-link",
    element: (up) => rect(up, { link: "http://localhost/#room" }),
    appState: { showHyperlinkPopup: "info" },
    steps: [],
  },
  {
    name: "view-mode",
    element: (up) => rect(up, { link: "https://excalidraw.com" }),
    appState: { showHyperlinkPopup: "info", viewModeEnabled: true },
    steps: [],
  },
  {
    name: "context-menu-open",
    element: (up) => rect(up, { link: "https://excalidraw.com" }),
    appState: { showHyperlinkPopup: "info", openMenu: "canvas" },
    steps: [],
  },
];

const BUTTONS = {
  edit: ".excalidraw-hyperlinkContainer--edit",
  remove: ".excalidraw-hyperlinkContainer--remove",
};

const render = async (up, window, iconNames, c) => {
  const { React, act, createRoot, Hyperlink } = up;
  const { document } = window;
  document.body.innerHTML = "";
  const container = document.createElement("div");
  container.className = "excalidraw";
  document.body.appendChild(container);
  up.reseed(1);
  const element = c.element(up);
  const scene = new up.Scene([element], { skipValidation: true });
  // App measures the container's offsets (getDefaultAppState leaves them out)
  const initial = {
    ...up.getDefaultAppState(),
    offsetLeft: 0,
    offsetTop: 0,
    ...c.appState,
    selectedElementIds: { [element.id]: true },
  };
  let setState = null;
  // App's render of the popup (App.tsx:2549-2565), its state in a hook
  const Host = () => {
    const [state, set] = React.useState(initial);
    setState = set;
    globalThis.__ui = { state };
    if (!state.showHyperlinkPopup) return null;
    return React.createElement(Hyperlink, {
      key: element.id,
      element,
      scene,
      setAppState: (patch) => set((prev) => ({ ...prev, ...(typeof patch === "function" ? patch(prev) : patch) })),
      onLinkOpen: undefined,
      setToast: () => {
        throw new Error(`${c.name}: a toast`);
      },
      updateEmbedValidationStatus: () => {},
    });
  };
  const root = createRoot(container);
  await act(async () => root.render(React.createElement(Host)));
  const tree = makeTree(iconNames, new Map(), {});
  const snapshot = (step, extra = {}) => {
    const input = container.querySelector("input");
    return {
      step,
      ...extra,
      dom: [...container.childNodes].map(tree),
      value: input ? input.value : null,
      focused: input ? document.activeElement === input : false,
      selected: input ? [input.selectionStart, input.selectionEnd] : null,
      link: element.link,
      showHyperlinkPopup: globalThis.__ui.state.showHyperlinkPopup,
    };
  };
  const steps = [snapshot(null)];
  for (const step of c.steps) {
    let extra = {};
    if ("type" in step) {
      const input = container.querySelector("input");
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      await act(async () => {
        setter.call(input, step.type);
        input.dispatchEvent(new window.Event("input", { bubbles: true }));
      });
    } else if ("key" in step) {
      const input = container.querySelector("input");
      const event = new window.KeyboardEvent("keydown", {
        key: step.key,
        ctrlKey: !!step.ctrl,
        bubbles: true,
        cancelable: true,
      });
      await act(async () => input.dispatchEvent(event));
      extra = { defaultPrevented: event.defaultPrevented };
    } else if ("click" in step) {
      const button = container.querySelector(BUTTONS[step.click]);
      if (!button) throw new Error(`${c.name}: no ${step.click} button`);
      await act(async () => button.dispatchEvent(new window.MouseEvent("click", { bubbles: true })));
    } else if (step.close) {
      await act(async () => setState((prev) => ({ ...prev, showHyperlinkPopup: false })));
    }
    steps.push(snapshot(step, extra));
  }
  await act(async () => root.unmount());
  return {
    name: c.name,
    element: clone(c.element === undefined ? null : scene.getElementsIncludingDeleted()[0]),
    appState: c.appState,
    steps,
  };
};

const stylesheet = async (upstream) => {
  const sass = (await import("sass")).default;
  const root = join(upstream.dir, "packages", "excalidraw");
  const compiled = sass.compileString(`@use "${SCSS.replace(/\.scss$/, "")}";\n`, {
    loadPaths: [root],
    style: "expanded",
  }).css;
  return (
    "/* Generated by tools/goldens/hyperlink.mjs; do not edit. Upstream's\n" +
    ` * packages/excalidraw/${SCSS} at the pin, compiled with sass 1.51.0\n` +
    " * (expanded). */\n" +
    `${compiled}\n`
  );
};

export const build = async (upstream) => {
  const css = await stylesheet(upstream);
  const window = installDom();
  // isLocalLink reads location.origin (http://localhost)
  globalThis.location = window.location;
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    shims: SHIMS,
    jsx: "automatic",
    // test mode: timestamps 1, as upstream's tests
    define: { "import.meta.env.MODE": '"test"' },
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
  window.close();
  const fixture = { upstream: upstream.commit, cases };
  return { [FIXTURE]: format(fixture), [STYLESHEET]: css };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`hyperlink: ${error.message}\n`);
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
      process.stderr.write("hyperlink goldens are out of date: run node tools/goldens/hyperlink.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`hyperlink goldens up to date: ${Object.keys(files).length} files\n`);
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
  // React's act() keeps MessageChannel ports open (see toolbar.mjs)
  process.exit(0);
}
