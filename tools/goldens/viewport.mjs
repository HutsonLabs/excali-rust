#!/usr/bin/env node
// Viewport goldens for excali-editor (ex-505): upstream's own zoom
// normalisation, coordinate transforms, scroll constraints, zoom-to-fit,
// wheel handler and zoom actions, run from the pinned checkout under Node.
//
//   node tools/goldens/viewport.mjs            write the fixture
//   node tools/goldens/viewport.mjs --check    exit 1 if it is stale
//   node tools/goldens/viewport.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/viewport.json:
//
// - `constants`: MIN_ZOOM, MAX_ZOOM, ZOOM_STEP (packages/common/src/
//   constants.ts:362-364) and DEFAULT_OVERSCROLL (packages/excalidraw/
//   viewport.ts:41);
// - `normalizedZoom`: getNormalizedZoom (scene/normalize.ts:7-9);
// - `coords`: viewportCoordsToSceneCoords and sceneCoordsToViewportCoords
//   (packages/common/src/utils.ts:317-358);
// - `constrain`: constrainScrollState (viewport.ts), including every case of
//   tests/scrollConstraints.test.tsx's pure suites;
// - `zoomAt`: getViewportForZoomWithScrollConstraints (viewport.ts);
// - `translate`: AppViewport.translate (components/App.viewport.ts:771-825)
//   on a stand-in App: the state it commits and what it calls;
// - `setViewport`: AppViewport.setViewport (components/App.viewport.ts:
//   672-760) on a stand-in App with AnimationController's frames run on a
//   controlled clock: after each navigation, animation frame or user
//   translate, the state, what was called and whether a locked transition
//   is pending;
// - `zoomToFitBounds`, `centerScrollOn`, `scrollBoundsIntoView` (including
//   tests/viewport.test.ts), `closestElementBounds`, `scrollToContent`
//   (getClosestElementBounds, getScrollToContentState): viewport.ts;
// - `wheel`: AppWheel.handle (components/App.wheel.ts) with upstream's
//   AppViewport on a stand-in App, event by event: whether the event's
//   default was prevented, what the handler called, and the state after.
//   The sequences include every scenario of tests/wheel.test.tsx that does
//   not need React's scheduler;
// - `actions`: actionZoomIn, actionZoomOut, actionResetZoom, actionZoomToFit,
//   actionZoomToFitSelection and actionZoomToFitSelectionInViewport
//   (actions/actionCanvas.tsx) performed on a stand-in App, and each
//   action's keyTest on a table of key events.
//
// Numbers JSON cannot hold (NaN, ±Infinity) are written as the strings
// "NaN", "Infinity" and "-Infinity".
//
// Deterministic: Math.random throws while generating, and every element is
// built with a fixed id and seed after reseed().

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { format } from "./lib/format.mjs";
import { installDom } from "./lib/recording-context.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";
import { ORIGIN, RANDOM_SEED } from "./static-scene.mjs";

export const OUT_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const OUT_FILE = "viewport.json";

const ENTRY = `
export { AppWheel } from "./packages/excalidraw/components/App.wheel";
export {
  AppViewport,
  SCROLL_CONSTRAINTS_SNAP_BACK_ANIMATION_KEY,
} from "./packages/excalidraw/components/App.viewport";
export { AnimationController } from "./packages/excalidraw/renderer/animation";
export {
  DEFAULT_OVERSCROLL,
  centerScrollOn,
  constrainScrollState,
  getClosestElementBounds,
  getScrollToContentState,
  getViewportForZoomWithScrollConstraints,
  scrollBoundsIntoView,
  zoomToFitBounds,
} from "./packages/excalidraw/viewport";
export { getNormalizedZoom } from "./packages/excalidraw/scene/normalize";
export {
  actionResetZoom,
  actionZoomIn,
  actionZoomOut,
  actionZoomToFit,
  actionZoomToFitSelection,
  actionZoomToFitSelectionInViewport,
} from "./packages/excalidraw/actions/actionCanvas";
export {
  CLASSES,
  CODES,
  KEYS,
  MAX_ZOOM,
  MIN_ZOOM,
  ZOOM_STEP,
  sceneCoordsToViewportCoords,
  viewportCoordsToSceneCoords,
} from "@excalidraw/common";
export { getSelectedElements } from "./packages/element/src/selection";
export {
  newElement,
  newFreeDrawElement,
  newLinearElement,
  newArrowElement,
  newTextElement,
} from "./packages/element/src/newElement";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
`;

// actionCanvas.tsx's panels are React components; performing an action and
// its keyTest never reach them.
const STUBS = [
  "react",
  "react/jsx-runtime",
  "packages/excalidraw/components/ColorPicker/ColorPicker",
  "packages/excalidraw/components/IconButton",
  "packages/excalidraw/components/Tooltip",
  "packages/excalidraw/components/icons",
  "packages/excalidraw/hooks/useAppStateValue",
  "packages/excalidraw/i18n",
  "packages/excalidraw/shortcut",
];

const SHIMS = {
  // React's batching (unstable_batchedUpdates) only groups renders
  "packages/excalidraw/reactUtils":
    "module.exports = { withBatchedUpdates: (fn) => fn, withBatchedUpdatesThrottled: (fn) => fn, isRenderThrottlingEnabled: () => false };",
  // register() only adds the action to the registry and returns it
  "packages/excalidraw/actions/register": "module.exports = { register: (action) => action };",
};

const usage = () => {
  process.stderr.write("usage: viewport.mjs [--check] [--out DIR]\n");
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

/** Non-finite numbers as strings, everything else as is. */
const encode = (value) => {
  if (typeof value === "number") {
    if (Number.isNaN(value)) return "NaN";
    if (value === Infinity) return "Infinity";
    if (value === -Infinity) return "-Infinity";
    return value;
  }
  if (Array.isArray(value)) return value.map(encode);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, encode(v)]));
  }
  return value;
};

/** JSON round trip: what the fixture holds and the Rust side reads. */
const plain = (value) => JSON.parse(JSON.stringify(value));

const viewportOf = (s) => ({ scrollX: s.scrollX, scrollY: s.scrollY, zoom: s.zoom.value });

// -- getNormalizedZoom ------------------------------------------------------------

const ZOOM_INPUTS = [
  1, 0, -0, -1, 0.1, 0.05, 0.1000004, 0.1000005, 0.0999996, 0.09999949, 30, 30.5, 29.9999996, 30.0000004,
  1.23456749, 1.2345675, 1.23456751, 0.1 + 0.2, 1 / 3, 2 / 3, 100, 1.0000005, 0.9999995, 0.9999994,
  2.5e-7, 1e21, 1.1 + 0.1, 0.30000000000000004 * 3, 12.3456785, 29.9999995, 0.1999995, NaN, Infinity, -Infinity,
];

const normalizedZoom = (up) => ZOOM_INPUTS.map((input) => ({ input, output: up.getNormalizedZoom(input) }));

// -- coordinate transforms ---------------------------------------------------------

const COORD_STATES = [
  { zoom: 1, offsetLeft: 0, offsetTop: 0, scrollX: 0, scrollY: 0 },
  { zoom: 2, offsetLeft: 0, offsetTop: 0, scrollX: 12, scrollY: 34 },
  { zoom: 0.5, offsetLeft: 13, offsetTop: 7, scrollX: -31.7, scrollY: 44.2 },
  { zoom: 1.37, offsetLeft: 120.5, offsetTop: 64.25, scrollX: 1234.5678, scrollY: -987.654 },
  { zoom: 0.1, offsetLeft: 0, offsetTop: 0, scrollX: 5000, scrollY: -5000 },
  { zoom: 30, offsetLeft: 3, offsetTop: 5, scrollX: -0.123, scrollY: 0.456 },
];

const COORD_POINTS = [
  [0, 0],
  [100, 100],
  [137.5, 91.25],
  [-40, 820.75],
  [1919.999, 1079.001],
];

const coords = (up) =>
  COORD_STATES.flatMap((s) => {
    const appState = { ...s, zoom: { value: s.zoom } };
    return COORD_POINTS.map(([x, y]) => ({
      state: s,
      point: [x, y],
      toScene: up.viewportCoordsToSceneCoords({ clientX: x, clientY: y }, appState),
      toViewport: up.sceneCoordsToViewportCoords({ sceneX: x, sceneY: y }, appState),
    }));
  });

// -- constrainScrollState -----------------------------------------------------------

const lock = (overrides) => ({ lockScroll: false, lockZoom: false, zoom: 1, overscroll: 0, ...overrides });

const BOX = { x: 0, y: 0, width: 1000, height: 1000 };

/** [name, state (200 x 100 viewport unless given), overscroll] */
const CONSTRAIN_CASES = [
  // scrollConstraints.test.tsx, "constrainScrollState (pure)"
  ["upstream-no-lock", { scrollX: 123, scrollY: -45, zoom: 0.5, scrollConstraints: null }, undefined],
  ["upstream-zoom-lock-only", { scrollX: 9999, scrollY: 9999, scrollConstraints: lock({ ...BOX, lockZoom: true, zoom: 0.2 }) }, undefined],
  ["upstream-scroll-lock-only", { zoom: 0.15, scrollConstraints: lock({ ...BOX, lockScroll: true }) }, undefined],
  ["upstream-corner", { scrollX: 100, scrollY: 100, scrollConstraints: lock({ ...BOX, lockScroll: true }) }, undefined],
  ["upstream-far-edge", { scrollX: -5000, scrollY: -5000, scrollConstraints: lock({ ...BOX, lockScroll: true }) }, undefined],
  ["upstream-in-bounds", { scrollX: -100, scrollY: -100, scrollConstraints: lock({ ...BOX, lockScroll: true }) }, undefined],
  ["upstream-centered", { scrollX: 999, zoom: 0.1, scrollConstraints: lock({ ...BOX, lockScroll: true }) }, undefined],
  // "zoom lock (pure)"
  ["upstream-zoom-floor", { zoom: 0.1, scrollConstraints: lock({ ...BOX, lockZoom: true, zoom: 0.5 }) }, undefined],
  ["upstream-zoom-max", { zoom: 30, scrollConstraints: lock({ ...BOX, lockZoom: true, zoom: 0.5 }) }, undefined],
  ["upstream-zoom-in-range", { zoom: 1.5, scrollConstraints: lock({ ...BOX, lockZoom: true, zoom: 0.5 }) }, undefined],
  // "offsets (pure)"
  ["upstream-offsets-top-left", { scrollX: 999, scrollY: 999, scrollConstraints: lock({ ...BOX, lockScroll: true, offsets: { top: 10, right: 20, bottom: 30, left: 40 } }) }, undefined],
  ["upstream-offsets-far-edge", { scrollX: -5000, scrollY: -5000, scrollConstraints: lock({ ...BOX, lockScroll: true, offsets: { top: 10, right: 20, bottom: 30, left: 40 } }) }, undefined],
  ["upstream-offsets-zoom-2", { scrollY: 999, zoom: 2, scrollConstraints: lock({ ...BOX, lockScroll: true, offsets: { top: 40 } }) }, undefined],
  ["upstream-offsets-default", { scrollX: 999, scrollConstraints: lock({ ...BOX, lockScroll: true }) }, undefined],
  ["upstream-offsets-with-overscroll", { scrollX: 999, scrollConstraints: lock({ ...BOX, lockScroll: true, overscroll: 30, offsets: { left: 40 } }) }, 30],
  // "rubberband overscroll (pure)"
  ["upstream-overscroll", { scrollX: 999, scrollConstraints: lock({ ...BOX, lockScroll: true, overscroll: 30 }) }, 30],
  ["upstream-overscroll-zoom-2", { scrollX: 999, zoom: 2, scrollConstraints: lock({ ...BOX, lockScroll: true, overscroll: 30 }) }, 30],
  ["upstream-overscroll-hard", { scrollX: 999, scrollConstraints: lock({ ...BOX, lockScroll: true }) }, undefined],
  ["upstream-overscroll-center", { scrollX: 9999, zoom: 0.1, scrollConstraints: lock({ ...BOX, lockScroll: true, overscroll: 30 }) }, 30],
  ["upstream-overscroll-center-hard", { scrollX: 9999, zoom: 0.1, scrollConstraints: lock({ ...BOX, lockScroll: true, overscroll: 30 }) }, undefined],
  // more: zoom and scroll locks together, boxes off the origin, negative
  // overscroll, a zoom outside the limits, both axes centred
  ["both-locks", { scrollX: 50, scrollY: -2000, zoom: 0.3, scrollConstraints: lock({ x: -250, y: 120, width: 800, height: 600, lockScroll: true, lockZoom: true, zoom: 0.4 }) }, undefined],
  ["both-locks-overscroll", { scrollX: 400, scrollY: -2000, zoom: 0.3, scrollConstraints: lock({ x: -250, y: 120, width: 800, height: 600, lockScroll: true, lockZoom: true, zoom: 0.4, overscroll: 150 }) }, 150],
  ["negative-overscroll", { scrollX: 999, scrollConstraints: lock({ ...BOX, lockScroll: true, overscroll: 30 }) }, -30],
  ["zoom-above-max", { zoom: 45, scrollConstraints: lock({ ...BOX, lockScroll: true }) }, undefined],
  ["zoom-below-min", { zoom: 0.01, scrollConstraints: lock({ ...BOX }) }, undefined],
  ["zoom-unrounded", { zoom: 1.23456789, scrollConstraints: lock({ ...BOX }) }, undefined],
  ["both-axes-centred", { scrollX: 3, scrollY: -7, zoom: 0.1, width: 1920, height: 1080, scrollConstraints: lock({ x: 30, y: 40, width: 100, height: 50, lockScroll: true, overscroll: 20, offsets: { left: 5, right: 7, top: 11, bottom: 13 } }) }, 20],
  ["fractional", { scrollX: -123.456, scrollY: 78.9, zoom: 1.37, width: 801, height: 601, scrollConstraints: lock({ x: -12.5, y: 33.3, width: 1234.5, height: 987.6, lockScroll: true, offsets: { left: 17.5, bottom: 42.25 } }) }, undefined],
  ["fractional-overscroll", { scrollX: 400, scrollY: -1500, zoom: 1.37, width: 801, height: 601, scrollConstraints: lock({ x: -12.5, y: 33.3, width: 1234.5, height: 987.6, lockScroll: true, overscroll: 150 }) }, 150],
];

const constrainState = (s) => ({
  scrollX: 0,
  scrollY: 0,
  width: 200,
  height: 100,
  ...s,
  zoom: { value: s.zoom ?? 1 },
});

const stateOut = (s) => ({ ...s, zoom: s.zoom.value });

const constrain = (up) =>
  CONSTRAIN_CASES.map(([name, s, overscroll]) => {
    const state = constrainState(s);
    const out = overscroll === undefined ? up.constrainScrollState(state) : up.constrainScrollState(state, overscroll);
    return { name, state: stateOut(state), ...(overscroll === undefined ? {} : { overscroll }), result: viewportOf(out) };
  });

// -- getViewportForZoomWithScrollConstraints ----------------------------------------

const APP = { scrollX: 0, scrollY: 0, width: 1000, height: 800, offsetLeft: 0, offsetTop: 0, scrollConstraints: null };

/** [name, state, viewportX, viewportY, nextZoom (normalised here)] */
const ZOOM_AT_CASES = [
  // scrollConstraints.test.tsx "preserves the screen-space overscroll distance while zooming"
  ["upstream-overscroll-preserved", { scrollY: 50, width: 200, height: 100, scrollConstraints: lock({ ...BOX, lockScroll: true, overscroll: 50 }) }, 0, 0, 2],
  ["centre-in", { zoom: 1 }, 500, 400, 1.1],
  ["centre-out", { zoom: 1 }, 500, 400, 0.9],
  ["pointer", { zoom: 1.5, scrollX: -31.7, scrollY: 44.2 }, 137.5, 91.25, 2.25],
  ["pointer-offsets", { zoom: 0.5, scrollX: 12, scrollY: 34, offsetLeft: 13, offsetTop: 7 }, 137.5, 91.25, 0.35],
  ["same-zoom", { zoom: 2, scrollX: 12, scrollY: 34 }, 300, 200, 2],
  ["to-max", { zoom: 29.5, scrollX: -1000.25, scrollY: 2000.75 }, 999, 799, 30],
  ["to-min", { zoom: 0.15, scrollX: 5000, scrollY: -5000 }, 1, 1, 0.1],
  ["fractional", { zoom: 1.37, scrollX: 1234.5678, scrollY: -987.654, offsetLeft: 120.5, offsetTop: 64.25, width: 801, height: 601 }, 400.5, 300.25, 1.507],
  ["zoom-lock-floor", { zoom: 0.6, scrollConstraints: lock({ x: 0, y: 0, width: 4000, height: 3000, lockZoom: true, zoom: 0.5 }) }, 500, 400, 0.4],
  ["scroll-lock-clamp", { zoom: 1, scrollX: -10, scrollY: -10, scrollConstraints: lock({ x: 0, y: 0, width: 2000, height: 1600, lockScroll: true }) }, 900, 700, 0.8],
  ["scroll-lock-overscroll-x", { zoom: 1, scrollX: 40, scrollY: -10, scrollConstraints: lock({ x: 0, y: 0, width: 2000, height: 1600, lockScroll: true, overscroll: 150 }) }, 100, 100, 1.5],
  ["scroll-lock-overscroll-both", { zoom: 2, scrollX: 60, scrollY: -1600, scrollConstraints: lock({ x: 0, y: 0, width: 2000, height: 1600, lockScroll: true, overscroll: 80, offsets: { top: 20, left: 30 } }) }, 250, 250, 1.2],
  ["scroll-lock-no-overscroll-held", { zoom: 1, scrollX: 40, scrollY: -10, scrollConstraints: lock({ x: 0, y: 0, width: 2000, height: 1600, lockScroll: true, overscroll: 0 }) }, 100, 100, 1.5],
  ["zoom-only-lock-overscroll", { zoom: 1, scrollX: 40, scrollY: -10, scrollConstraints: lock({ x: 0, y: 0, width: 2000, height: 1600, lockZoom: true, zoom: 0.5, overscroll: 150 }) }, 100, 100, 1.5],
];

const appState = (s) => ({ ...APP, ...s, zoom: { value: s.zoom ?? 1 } });

const zoomAt = (up) =>
  ZOOM_AT_CASES.map(([name, s, viewportX, viewportY, z]) => {
    const state = appState(s);
    const nextZoom = up.getNormalizedZoom(z);
    const out = up.getViewportForZoomWithScrollConstraints({ viewportX, viewportY, nextZoom }, state);
    return { name, state: stateOut(state), viewportX, viewportY, nextZoom, result: viewportOf(out) };
  });

// -- a stand-in App ------------------------------------------------------------------

/**
 * What AppViewport, AppWheel and the zoom actions read off App, with
 * setState applied synchronously (React would batch, which none of the
 * recorded paths depends on). `calls` records, in order, the App and
 * viewport side effects a port has to reproduce.
 */
const makeApp = (up, window, { state, navigation = true, panActive = false, lastPosition = { x: 0, y: 0 }, elements = [], offsets = {} }) => {
  const calls = [];
  const app = {
    state,
    ownerWindow: window,
    unmounted: true,
    isNavigationEnabled: () => navigation,
    requestUnfollow: () => calls.push("requestUnfollow"),
    setState(update) {
      const next = typeof update === "function" ? update(app.state, {}) : update;
      if (next) app.state = { ...app.state, ...next };
    },
    pan: { isActive: () => panActive, flushMove: () => calls.push("flushMove") },
    resetShouldCacheIgnoreZoomDebounced: () => calls.push("resetShouldCacheIgnoreZoom"),
    scene: {
      getSelectedElements: (appState) => up.getSelectedElements(elements.filter((e) => !e.isDeleted), appState),
    },
  };
  const viewport = new up.AppViewport(app, {
    getContainer: () => null,
    getStylesPanelMode: () => "full",
    isGestureActive: () => false,
  });
  viewport.lastPosition = { ...lastPosition };
  // the debounced rubberband snap-back (App.viewport.ts:853-856): recorded
  // instead of scheduled
  const debounced = viewport.snapBackDebounced;
  viewport.snapBackDebounced = Object.assign(() => calls.push("scheduleSnapBack"), { cancel: debounced.cancel, flush: debounced.flush });
  // the UI offsets are measured from the DOM; the case gives them
  viewport.getOffsets = () => offsets;
  app.viewport = viewport;
  return { app, calls };
};

/** Records AnimationController.cancel of the snap-back animation. */
const withAnimationLog = (up, calls, fn) => {
  const cancel = up.AnimationController.cancel;
  up.AnimationController.cancel = (key) => {
    if (key === up.SCROLL_CONSTRAINTS_SNAP_BACK_ANIMATION_KEY) calls.push("cancelSnapBack");
    return cancel.call(up.AnimationController, key);
  };
  try {
    return fn();
  } finally {
    up.AnimationController.cancel = cancel;
  }
};

// -- AppViewport.translate --------------------------------------------------------------

const EDITOR = { ...APP, inputDevice: "auto", shouldCacheIgnoreZoom: false, selectedElementIds: {} };

const editorState = (s = {}) => ({ ...EDITOR, ...s, zoom: { value: s.zoom ?? 1 } });

const editorOut = (s) => ({
  scrollX: s.scrollX,
  scrollY: s.scrollY,
  zoom: s.zoom.value,
  width: s.width,
  height: s.height,
  offsetLeft: s.offsetLeft,
  offsetTop: s.offsetTop,
  scrollConstraints: s.scrollConstraints,
});

const SCROLL_LOCK = lock({ x: 0, y: 0, width: 2000, height: 1600, lockScroll: true, overscroll: 150 });

/** [name, state, update ({scrollX?, scrollY?, zoom?} or null), opts] */
const TRANSLATE_CASES = [
  ["free-scroll", {}, { scrollX: -40, scrollY: 25.5 }, undefined],
  ["free-zoom", {}, { scrollX: -40, scrollY: 25.5, zoom: 1.7 }, undefined],
  ["null-update", {}, null, undefined],
  ["null-update-preserve", {}, null, { preserveScrollConstraintsSnapBack: true }],
  ["lock-in-bounds", { scrollConstraints: SCROLL_LOCK }, { scrollX: -100, scrollY: -100 }, undefined],
  ["lock-overscroll-give", { scrollConstraints: SCROLL_LOCK }, { scrollX: 90, scrollY: -100 }, undefined],
  ["lock-overscroll-past-give", { scrollConstraints: SCROLL_LOCK }, { scrollX: 400, scrollY: -2000 }, undefined],
  ["lock-zoom-change-hard-clamps", { scrollConstraints: SCROLL_LOCK }, { scrollX: 90, scrollY: -100, zoom: 1.2 }, undefined],
  ["lock-zoom-pre-constrained", { scrollConstraints: SCROLL_LOCK }, { scrollX: 90, scrollY: -100, zoom: 1.2 }, { zoomPreConstrained: true, preserveScrollConstraintsSnapBack: true }],
  ["lock-no-overscroll", { scrollConstraints: lock({ x: 0, y: 0, width: 2000, height: 1600, lockScroll: true }) }, { scrollX: 90, scrollY: 5 }, undefined],
  ["zoom-lock-floor", { scrollConstraints: lock({ x: 0, y: 0, width: 2000, height: 1600, lockZoom: true, zoom: 0.5 }) }, { zoom: 0.2 }, undefined],
  ["zoom-lock-overscroll-scheduled", { scrollConstraints: lock({ x: 0, y: 0, width: 2000, height: 1600, lockZoom: true, zoom: 0.5, overscroll: 150 }) }, { scrollX: 12 }, undefined],
];

const translate = (up, window) =>
  TRANSLATE_CASES.map(([name, s, update, opts]) => {
    const state = editorState(s);
    const { app, calls } = makeApp(up, window, { state });
    const payload = update && { ...update, ...(update.zoom === undefined ? {} : { zoom: { value: update.zoom } }) };
    const returned = withAnimationLog(up, calls, () => app.viewport.translate(() => payload, opts));
    return {
      name,
      state: editorOut(state),
      update,
      ...(opts ? { opts } : {}),
      returned,
      calls,
      result: viewportOf(app.state),
    };
  });

// -- AppViewport.setViewport ------------------------------------------------------------

/**
 * Runs fn with AnimationController's clock in hand: its frames
 * (setTimeout, render throttling being off) are queued instead of
 * scheduled, and performance.now() reads `clock.now`. `clock.frame(t)`
 * runs the frames queued so far at time t.
 */
const withClock = (fn) => {
  const queue = [];
  let nextId = 1;
  const clock = {
    now: 0,
    frame(t) {
      clock.now = t;
      for (const entry of queue.splice(0)) entry.cb();
    },
  };
  const { setTimeout: st, clearTimeout: ct } = globalThis;
  const now = performance.now;
  globalThis.setTimeout = (cb) => {
    const id = nextId++;
    queue.push({ id, cb });
    return id;
  };
  globalThis.clearTimeout = (id) => {
    const i = queue.findIndex((e) => e.id === id);
    if (i >= 0) queue.splice(i, 1);
  };
  performance.now = () => clock.now;
  try {
    return fn(clock);
  } finally {
    globalThis.setTimeout = st;
    globalThis.clearTimeout = ct;
    performance.now = now;
  }
};

const SEARCH_OFFSETS = { top: 24, right: 326, bottom: 24, left: 24 };

/** Frames every `step` ms from `from` to `to` inclusive. */
const frames = (from, to, step = 16) => {
  const out = [];
  for (let t = from; t <= to; t += step) out.push(["frame", t]);
  return out;
};

/**
 * [name, state, steps]: a step is ["set", t, opts] (setViewport at time
 * t), ["frame", t] (an animation frame at t) or ["translate", t, update]
 * (a user pan or zoom).
 */
const SET_VIEWPORT_CASES = [
  // the search menu's navigation (SearchMenu.tsx:230-235): 300 ms,
  // scale-down, the UI's offsets
  ["search-scale-down", {}, [["set", 1000, { target: [2000, 1500, 2100, 1540], fit: "scale-down", animation: { duration: 300 }, offsets: SEARCH_OFFSETS }], ...frames(1000, 1320)]],
  ["search-contain-tiny-text", { zoom: 0.5, scrollX: 40, scrollY: -30 }, [["set", 1000, { target: [120, 80, 170, 90], fit: "contain", animation: { duration: 300 }, offsets: SEARCH_OFFSETS }], ...frames(1000, 1320, 20)]],
  ["search-zoom-out", { zoom: 3, scrollX: -500, scrollY: -700 }, [["set", 1000, { target: [-4000, -3000, 5000, 3500], fit: "scale-down", animation: { duration: 300 } }], ...frames(1000, 1320, 25)]],
  ["pure-pan", { scrollX: 12, scrollY: 34 }, [["set", 1000, { target: [800, 900, 850, 950], fit: "none", animation: { duration: 300 } }], ...frames(1000, 1320, 30)]],
  ["uneven-frames", { zoom: 1.3 }, [["set", 5, { target: [300, 200, 900, 700], animation: { duration: 300 } }], ["frame", 20], ["frame", 21], ["frame", 150.5], ["frame", 151], ["frame", 304.9], ["frame", 305], ["frame", 400]]],
  ["clears-lock", { scrollConstraints: SCROLL_LOCK }, [["set", 1000, { target: [300, 200, 500, 300], animation: { duration: 300 } }], ...frames(1000, 1400, 50)]],
  ["installs-lock", {}, [["set", 1000, { target: [0, 0, 1500, 1200], lock: { scroll: true, zoom: true }, animation: { duration: 300 } }], ...frames(1000, 1400, 50)]],
  ["installs-lock-rigid", { zoom: 2 }, [["set", 1000, { target: [100, 100, 3100, 2100], fit: "contain", lock: { scroll: true, overscroll: false }, animation: { duration: 300 } }], ...frames(1000, 1400, 100)]],
  ["installs-lock-give", {}, [["set", 1000, { target: [100, 100, 600, 400], lock: { zoom: true, overscroll: 40 }, animation: { duration: 300 }, offsets: { right: 302 } }], ...frames(1000, 1400, 100)]],
  ["installs-lock-negative-give", {}, [["set", 1000, { target: [100, 100, 600, 400], lock: { scroll: true, overscroll: -5 }, animation: false }]]],
  ["replaces-lock", { scrollConstraints: SCROLL_LOCK }, [["set", 1000, { target: [0, 0, 400, 300], lock: { scroll: true }, animation: { duration: 300 } }], ...frames(1000, 1400, 100)]],
  ["no-animation", { scrollConstraints: SCROLL_LOCK, shouldCacheIgnoreZoom: true }, [["set", 1000, { target: [2000, 1500, 2100, 1540], animation: false }], ["frame", 1100]]],
  ["no-animation-lock", {}, [["set", 1000, { target: [0, 0, 400, 300], lock: { scroll: true, zoom: true }, animation: false }]]],
  ["default-duration", {}, [["set", 1000, { target: [2000, 1500, 2100, 1540] }], ...frames(1000, 1600, 50)]],
  ["animation-true", {}, [["set", 1000, { target: [2000, 1500, 2100, 1540], animation: true }], ...frames(1000, 1600, 100)]],
  ["animation-no-duration", {}, [["set", 1000, { target: [2000, 1500, 2100, 1540], animation: {} }], ...frames(1000, 1600, 100)]],
  ["zero-duration", {}, [["set", 1000, { target: [2000, 1500, 2100, 1540], animation: { duration: 0 } }], ["frame", 1016]]],
  // a second navigation takes over from the viewport the first reached
  ["superseded", {}, [
    ["set", 1000, { target: [2000, 1500, 2100, 1540], animation: { duration: 300 }, offsets: SEARCH_OFFSETS }],
    ["frame", 1000], ["frame", 1016], ["frame", 1100],
    ["set", 1110, { target: [-600, -400, -500, -380], animation: { duration: 300 }, offsets: SEARCH_OFFSETS }],
    ...frames(1116, 1500, 32),
  ]],
  ["superseded-by-immediate", {}, [
    ["set", 1000, { target: [2000, 1500, 2100, 1540], animation: { duration: 300 } }],
    ["frame", 1000], ["frame", 1100],
    ["set", 1110, { target: [-600, -400, -500, -380], animation: false }],
    ["frame", 1200],
  ]],
  // a user pan takes over from an unlocked transition
  ["user-pan-interrupts", {}, [
    ["set", 1000, { target: [2000, 1500, 2100, 1540], animation: { duration: 300 } }],
    ["frame", 1000], ["frame", 1100],
    ["translate", 1105, { scrollX: -20, scrollY: 10 }],
    ["frame", 1200], ["frame", 1400],
  ]],
  // ... but not from a transition into a locked viewport
  ["user-pan-ignored-while-locking", {}, [
    ["set", 1000, { target: [0, 0, 1500, 1200], lock: { scroll: true }, animation: { duration: 300 } }],
    ["frame", 1000], ["frame", 1100],
    ["translate", 1105, { scrollX: -20, scrollY: 10 }],
    ["frame", 1200], ["frame", 1300], ["frame", 1316],
    ["translate", 1320, { scrollX: 5000, scrollY: 10 }],
  ]],
  ["unresolved-target", { scrollX: 3 }, [["set", 1000, { target: "missing", animation: { duration: 300 } }], ["frame", 1100]]],
];

const setViewportOut = (app) => ({
  ...viewportOf(app.state),
  scrollConstraints: app.state.scrollConstraints,
  shouldCacheIgnoreZoom: app.state.shouldCacheIgnoreZoom,
});

const setViewport = (up, window) =>
  SET_VIEWPORT_CASES.map(([name, s, steps]) => {
    const state = editorState(s);
    const { app, calls } = makeApp(up, window, { state });
    app.scene.getNonDeletedElementsMap = () => new Map();
    const out = withClock((clock) =>
      steps.map(([kind, t, arg]) => {
        clock.now = t;
        let returned;
        withAnimationLog(up, calls, () => {
          if (kind === "set") app.viewport.setViewport(arg);
          else if (kind === "frame") clock.frame(t);
          else {
            const payload = { ...arg, ...(arg.zoom === undefined ? {} : { zoom: { value: arg.zoom } }) };
            returned = app.viewport.translate(() => payload);
          }
        });
        const step = {
          step: [kind, t, ...(arg === undefined ? [] : [arg])],
          ...(returned === undefined ? {} : { returned }),
          calls: calls.splice(0),
          lockedTransitionPending: app.viewport.isLockedTransitionPending,
          animating: app.viewport.isAnimating,
          state: setViewportOut(app),
        };
        return step;
      }),
    );
    // nothing left running for the next case
    app.viewport.destroy();
    return { name, state: { ...editorOut(state), shouldCacheIgnoreZoom: state.shouldCacheIgnoreZoom }, steps: out };
  });

// -- zoomToFitBounds ------------------------------------------------------------------------

const FIT_BOUNDS = [
  [0, 0, 100, 100],
  [50, 100, 100, 200],
  [-250.5, 30.25, 1750.75, 900.125],
  [1000, 1000, 1050, 1050],
  [-12000, -9000, 12000, 9000],
  [10, 10, 10.0001, 10.0001],
  [0, 0, 0, 0],
  [5, 5, 5, 205],
];

const FIT_STATES = [
  { zoom: 1, width: 1000, height: 800 },
  { zoom: 0.5, width: 100, height: 100, scrollX: 3, scrollY: 4 },
  { zoom: 2.5, width: 1366, height: 768, offsetLeft: 13, offsetTop: 7 },
  { zoom: 1, width: 10, height: 10 },
  { zoom: 1, width: 0, height: 0 },
];

const FIT_OFFSETS = [undefined, { left: 216, top: 60, right: 302, bottom: 50 }, { right: 40 }];

const FIT_OPTIONS = [
  {},
  { fit: "contain" },
  { fit: "none" },
  { fit: "scale-down", steppedZoom: true },
  { fit: "contain", steppedZoom: true },
  { fit: "none", steppedZoom: true },
  { fit: "contain", minZoom: 0.25, maxZoom: 4 },
  { fit: "scale-down", minZoom: 0.5 },
];

const zoomToFitBounds = (up) => {
  const out = [];
  for (const bounds of FIT_BOUNDS) {
    for (const s of FIT_STATES) {
      for (const offsets of FIT_OFFSETS) {
        for (const options of FIT_OPTIONS) {
          const state = appState(s);
          const r = up.zoomToFitBounds({ bounds, appState: state, ...(offsets ? { canvasOffsets: offsets } : {}), ...options });
          out.push({
            bounds,
            state: stateOut(state),
            ...(offsets ? { offsets } : {}),
            options,
            result: viewportOf(r.appState),
            captureUpdate: r.captureUpdate,
          });
        }
      }
    }
  }
  return out;
};

// -- centerScrollOn, scrollBoundsIntoView --------------------------------------------------------

const centerScrollOn = (up) => {
  const out = [];
  const points = [
    { x: 0, y: 0 },
    { x: 1025, y: 1025 },
    { x: -333.3, y: 77.7 },
  ];
  const dims = [
    { width: 100, height: 100 },
    { width: 1366, height: 768 },
  ];
  for (const scenePoint of points) {
    for (const viewportDimensions of dims) {
      for (const zoom of [0.5, 1, 2.75]) {
        for (const offsets of [undefined, { left: 216, top: 60, right: 302, bottom: 50 }, { top: 10 }]) {
          const r = up.centerScrollOn({ scenePoint, viewportDimensions, zoom: { value: zoom }, ...(offsets ? { offsets } : {}) });
          out.push({ scenePoint, viewportDimensions, zoom, ...(offsets ? { offsets } : {}), result: r });
        }
      }
    }
  }
  return out;
};

const INTO_VIEW_STATE = (zoom = 1) => ({ scrollX: 0, scrollY: 0, zoom, width: 1000, height: 800 });

/** [name, bounds, state, offsets, tooLarge] */
const INTO_VIEW_CASES = [
  // tests/viewport.test.ts
  ["upstream-in-view", [100, 100, 200, 200], INTO_VIEW_STATE(), undefined, undefined],
  ["upstream-least-movement", [900, -30, 1050, 100], INTO_VIEW_STATE(), undefined, undefined],
  ["upstream-offsets-clear", [900, 100, 1000, 200], INTO_VIEW_STATE(), { right: 320 }, undefined],
  ["upstream-zoom-2", [450, 100, 550, 150], INTO_VIEW_STATE(2), undefined, undefined],
  ["upstream-too-tall", [100, 500, 200, 2000], INTO_VIEW_STATE(), undefined, undefined],
  ["upstream-too-tall-leave", [100, 500, 200, 2000], INTO_VIEW_STATE(), undefined, "leave"],
  ["upstream-no-room", [900, 0, 1100, 10], INTO_VIEW_STATE(), { left: 600, right: 600 }, undefined],
  // more
  ["left-and-below", [-80, 790, 20, 850], INTO_VIEW_STATE(), undefined, undefined],
  ["scrolled-zoomed", [10, 10, 60, 60], { scrollX: -40, scrollY: 25, zoom: 0.75, width: 640, height: 480 }, { left: 30, top: 12 }, undefined],
  ["too-wide-align-start", [-500, 100, 1500, 200], INTO_VIEW_STATE(), undefined, "alignStart"],
  ["too-wide-leave", [-500, 100, 1500, 200], INTO_VIEW_STATE(), undefined, "leave"],
  ["exact-fit", [0, 0, 1000, 800], INTO_VIEW_STATE(), undefined, undefined],
  ["no-vertical-room", [0, 0, 10, 10], INTO_VIEW_STATE(), { top: 400, bottom: 400 }, undefined],
  ["fractional", [123.456, -78.9, 234.567, 12.34], { scrollX: 0.5, scrollY: 0.25, zoom: 1.37, width: 801, height: 601 }, { right: 17.5 }, undefined],
];

const scrollBoundsIntoView = (up) =>
  INTO_VIEW_CASES.map(([name, bounds, s, offsets, tooLarge]) => ({
    name,
    bounds,
    state: s,
    ...(offsets ? { offsets } : {}),
    ...(tooLarge ? { tooLarge } : {}),
    result: up.scrollBoundsIntoView({
      bounds,
      appState: { ...s, zoom: { value: s.zoom } },
      ...(offsets ? { offsets } : {}),
      ...(tooLarge ? { tooLarge } : {}),
    }),
  }));

// -- elements ---------------------------------------------------------------------------------

/** A fixed set of elements: every kind whose bounds differ in shape. */
const ELEMENTS = (up) => {
  const deleted = up.newElement({ type: "rectangle", id: "deleted", x: -5000, y: -5000, width: 10, height: 10, seed: 9 });
  return [
    up.newElement({ type: "rectangle", id: "rect", x: 100, y: 100, width: 200, height: 120, seed: 1 }),
    up.newElement({ type: "ellipse", id: "ellipse", x: 420, y: -60, width: 160, height: 90, angle: 0.6, seed: 2 }),
    up.newElement({ type: "diamond", id: "diamond", x: -300, y: 380, width: 140, height: 140, seed: 3 }),
    up.newLinearElement({ type: "line", id: "line", x: 700, y: 300, seed: 4, points: [[0, 0], [120, 40], [60, 160]] }),
    up.newArrowElement({ type: "arrow", id: "arrow", x: 50, y: 600, seed: 5, points: [[0, 0], [300, -80]], elbowed: false }),
    up.newFreeDrawElement({ type: "freedraw", id: "freedraw", x: 900, y: 900, seed: 6, points: [[0, 0], [20, -10], [45, 5], [30, 40]], simulatePressure: true }),
    up.newTextElement({ id: "text", x: -120, y: -200, text: "hello\nworld", fontSize: 20, seed: 7 }),
    // invisibly small: getVisibleElements drops it
    up.newElement({ type: "rectangle", id: "dot", x: 3000, y: 3000, width: 0, height: 0, seed: 8 }),
    { ...deleted, isDeleted: true },
  ].map(plain);
};

const byIds = (elements, ids) => elements.filter((e) => ids.includes(e.id));

const ELEMENT_SETS = [
  ["none", []],
  ["rect", ["rect"]],
  ["far-apart", ["rect", "freedraw"]],
  ["all", ["rect", "ellipse", "diamond", "line", "arrow", "freedraw", "text", "dot", "deleted"]],
  ["rotated-ellipse", ["ellipse"]],
  ["linear", ["line", "arrow"]],
  ["invisible-and-deleted", ["dot", "deleted"]],
  ["text", ["text"]],
];

const closestElementBounds = (up, elements) => {
  const out = [];
  for (const [set, ids] of ELEMENT_SETS) {
    for (const from of [
      { x: 0, y: 0 },
      { x: 1000, y: 1000 },
      { x: -400, y: 500 },
    ]) {
      out.push({ set, from, result: up.getClosestElementBounds(byIds(elements, ids), from) });
    }
  }
  return out;
};

const TO_CONTENT_STATES = [
  { zoom: 1, width: 1000, height: 800 },
  { zoom: 1, width: 300, height: 200, scrollX: -50, scrollY: 40 },
  { zoom: 0.25, width: 1366, height: 768, offsetLeft: 13, offsetTop: 7 },
  { zoom: 3, width: 1000, height: 800, scrollX: -900, scrollY: -850 },
];

const scrollToContent = (up, elements) => {
  const out = [];
  for (const [set, ids] of ELEMENT_SETS) {
    for (const s of TO_CONTENT_STATES) {
      const state = appState(s);
      out.push({ set, state: stateOut(state), result: up.getScrollToContentState(byIds(elements, ids), state) });
    }
  }
  return out;
};

// -- AppWheel.handle ------------------------------------------------------------------------

const WHEEL_BUTTON = 4;

const target = (window, up, kind) => {
  const { document } = window;
  switch (kind) {
    case "canvas":
      return document.createElement("canvas");
    case "textarea":
      return document.createElement("textarea");
    case "iframe":
      return document.createElement("iframe");
    case "frameName": {
      const div = document.createElement("div");
      div.classList.add(up.CLASSES.FRAME_NAME);
      return div;
    }
    case "other":
      return document.createElement("div");
    default:
      throw new Error(`target ${kind}`);
  }
};

/** A wheel event as the handler reads it; `ctrlOrCmd` is not a field. */
const wheelEvent = (window, up, e, record) => ({
  target: target(window, up, e.target ?? "canvas"),
  deltaX: e.deltaX ?? 0,
  deltaY: e.deltaY ?? 0,
  ctrlKey: !!e.ctrlKey,
  metaKey: !!e.metaKey,
  shiftKey: !!e.shiftKey,
  altKey: false,
  buttons: e.buttons ?? 0,
  preventDefault: () => {
    record.prevented = true;
  },
});

const wheelTick = (e, n) => Array.from({ length: n }, () => e);

/** [name, context, events] */
const WHEEL_SEQUENCES = [
  // wheel.test.tsx "pans on plain wheel, horizontally with shift, and zooms on ctrl/cmd+wheel"
  ["upstream-pan-shift-zoom", {}, [{ deltaX: 30, deltaY: 40 }, { deltaY: 40, shiftKey: true }, { deltaY: -100, ctrlKey: true }, { deltaY: 100, metaKey: true }]],
  // "does nothing on a zoom tick at the zoom limit"
  ["upstream-zoom-limit", { state: { zoom: 0.1 } }, [{ deltaY: 100, ctrlKey: true }]],
  ["zoom-limit-max", { state: { zoom: 30 } }, [{ deltaY: -100, ctrlKey: true }, { deltaY: -1, metaKey: true }]],
  // "wheel button held down: zooms instead of panning, whatever the modifiers"
  ["upstream-wheel-button", {}, [{ deltaY: -100, buttons: WHEEL_BUTTON }, { deltaY: 100, buttons: WHEEL_BUTTON }, { deltaY: -100, buttons: WHEEL_BUTTON, shiftKey: true }]],
  // "zooms during the wheel-button drag-pan the button started"
  ["upstream-pan-active", { panActive: true, lastPosition: { x: 100, y: 100 } }, [{ deltaY: 40 }, { deltaY: -100, buttons: WHEEL_BUTTON }, { deltaX: 30, deltaY: 0, buttons: WHEEL_BUTTON }]],
  // "inputDevice preference": shift+wheel with each device
  ...["auto", "mouse", "trackpad"].flatMap((inputDevice) =>
    [
      ["shift", {}],
      ["ctrl-shift", { ctrlKey: true }],
      ["cmd-shift", { metaKey: true }],
    ].map(([label, mods]) => [
      `upstream-${inputDevice}-${label}`,
      { state: { inputDevice, zoom: 2, scrollX: 12, scrollY: 34 } },
      [{ deltaX: 10, deltaY: 40, shiftKey: true, ...mods }, { deltaX: -40, shiftKey: true, ...mods }],
    ]),
  ),
  // "mouse": zooms around the pointer on plain, ctrl and cmd wheel
  ...[
    ["plain", {}],
    ["ctrl", { ctrlKey: true }],
    ["cmd", { metaKey: true }],
  ].map(([label, mods]) => [
    `upstream-mouse-${label}`,
    { state: { inputDevice: "mouse" }, lastPosition: { x: 100, y: 100 } },
    [{ deltaY: -100, ...mods }, { deltaY: 100, ...mods }],
  ]),
  ["upstream-mouse-wheel-button", { state: { inputDevice: "mouse" } }, [{ deltaY: -100, buttons: WHEEL_BUTTON, ctrlKey: true, shiftKey: true }]],
  ["upstream-mouse-horizontal-only", { state: { inputDevice: "mouse" } }, [{ deltaX: 30 }]],
  // "wheel over a frame label" and the other surfaces
  ["targets", {}, [
    { deltaY: 40, target: "frameName" },
    { deltaY: 40, target: "textarea" },
    { deltaY: 40, target: "iframe" },
    { deltaY: 40, target: "other" },
    { deltaY: -100, ctrlKey: true, target: "other" },
    { deltaY: -100, metaKey: true, target: "other" },
    { deltaY: -100, ctrlKey: true, target: "iframe" },
  ]],
  ["navigation-disabled", { navigation: false }, [{ deltaY: 40 }, { deltaY: -100, ctrlKey: true }, { deltaY: -100, ctrlKey: true, target: "other" }]],
  ["zero-delta", {}, [{}, { ctrlKey: true }, { buttons: WHEEL_BUTTON }]],
  ["horizontal-ctrl", {}, [{ deltaX: 25, ctrlKey: true }, { deltaX: 25, metaKey: true, shiftKey: true }]],
  // repeated ticks: to the limits and back, a trackpad pinch's small deltas
  ["ticks-in-to-max", { lastPosition: { x: 137.5, y: 91.25 }, state: { offsetLeft: 13, offsetTop: 7, scrollX: -31.7, scrollY: 44.2 } }, wheelTick({ deltaY: -100, ctrlKey: true }, 40)],
  ["ticks-out-to-min", { lastPosition: { x: 812, y: 603 }, state: { zoom: 4, scrollX: 250, scrollY: -120 } }, wheelTick({ deltaY: 100, ctrlKey: true }, 25)],
  ["ticks-pinch", { lastPosition: { x: 400, y: 300 }, state: { zoom: 1.3 } }, [...wheelTick({ deltaY: -3.5, ctrlKey: true }, 12), ...wheelTick({ deltaY: 2.25, ctrlKey: true }, 12)]],
  ["ticks-mouse", { lastPosition: { x: 50, y: 700 }, state: { inputDevice: "mouse", zoom: 0.7 } }, [...wheelTick({ deltaY: -120 }, 8), ...wheelTick({ deltaY: 53 }, 8)]],
  // scroll constraints: panning into the rubberband, zooming under a zoom lock
  ["lock-pan", { state: { scrollConstraints: SCROLL_LOCK } }, [{ deltaX: -60 }, { deltaX: -60 }, { deltaX: -100, deltaY: -300 }, { deltaY: 5000 }]],
  ["lock-zoom", { lastPosition: { x: 500, y: 400 }, state: { zoom: 0.6, scrollConstraints: lock({ x: 0, y: 0, width: 4000, height: 3000, lockZoom: true, zoom: 0.5 }) } }, wheelTick({ deltaY: 100, ctrlKey: true }, 4)],
  ["lock-zoom-scroll-overscrolled", { lastPosition: { x: 100, y: 100 }, state: { scrollX: 40, scrollY: -10, scrollConstraints: lock({ x: 0, y: 0, width: 2000, height: 1600, lockScroll: true, lockZoom: true, zoom: 0.5, overscroll: 150 }) } }, [
    { deltaY: -100, ctrlKey: true },
    { deltaY: 100, ctrlKey: true },
    { deltaY: 100, ctrlKey: true },
    { deltaY: 100, ctrlKey: true },
    { deltaX: -30 },
  ]],
];

/** The zoom formula: one tick per zoom and delta. */
const SWEEP_ZOOMS = [0.1, 0.5, 0.95, 1, 1.5, 2, 5, 10, 29, 30];
const SWEEP_DELTAS = [-1000, -120, -100, -53, -20.5, -20, -10, -4, -1, -0.5, 0.5, 1, 4, 10, 20, 20.5, 53, 100, 120, 1000];

const runWheel = (up, window, name, ctx, events) => {
  const state = editorState(ctx.state);
  const { app, calls } = makeApp(up, window, { state, navigation: ctx.navigation, panActive: ctx.panActive, lastPosition: ctx.lastPosition });
  const wheel = new up.AppWheel(app);
  const steps = events.map((e) => {
    const record = { prevented: false };
    calls.length = 0;
    withAnimationLog(up, calls, () => wheel.handle(wheelEvent(window, up, e, record)));
    return {
      event: e,
      prevented: record.prevented,
      calls: [...calls],
      state: { ...viewportOf(app.state), shouldCacheIgnoreZoom: app.state.shouldCacheIgnoreZoom },
    };
  });
  return {
    name,
    state: { ...editorOut(state), inputDevice: state.inputDevice },
    navigation: ctx.navigation ?? true,
    panActive: !!ctx.panActive,
    lastPosition: ctx.lastPosition ?? { x: 0, y: 0 },
    steps,
  };
};

const wheel = (up, window) => {
  const sequences = WHEEL_SEQUENCES.map(([name, ctx, events]) => runWheel(up, window, name, ctx, events));
  const sweep = [];
  for (const zoom of SWEEP_ZOOMS) {
    for (const deltaY of SWEEP_DELTAS) {
      sweep.push(
        runWheel(
          up,
          window,
          `sweep-${zoom}-${deltaY}`,
          { lastPosition: { x: 137.5, y: 91.25 }, state: { zoom, offsetLeft: 13, offsetTop: 7, scrollX: -31.7, scrollY: 44.2 } },
          [{ deltaY, ctrlKey: true }],
        ),
      );
    }
  }
  return { ctrlOrCmd: up.KEYS.CTRL_OR_CMD, sequences: [...sequences, ...sweep] };
};

// -- zoom actions ---------------------------------------------------------------------------

const ACTION_NAMES = ["zoomIn", "zoomOut", "resetZoom", "zoomToFit", "zoomToFitSelection", "zoomToFitSelectionInViewport"];

const actionOf = (up, name) =>
  ({
    zoomIn: up.actionZoomIn,
    zoomOut: up.actionZoomOut,
    resetZoom: up.actionResetZoom,
    zoomToFit: up.actionZoomToFit,
    zoomToFitSelection: up.actionZoomToFitSelection,
    zoomToFitSelectionInViewport: up.actionZoomToFitSelectionInViewport,
  })[name];

const ACTION_STATES = [
  ["default", {}],
  ["zoom-0.1", { zoom: 0.1 }],
  ["zoom-0.15", { zoom: 0.15 }],
  ["zoom-0.95", { zoom: 0.95, scrollX: 12, scrollY: 34 }],
  ["zoom-1.05", { zoom: 1.05 }],
  ["zoom-2.3333", { zoom: 2.333333, scrollX: -31.7, scrollY: 44.2, offsetLeft: 13, offsetTop: 7 }],
  ["zoom-29.95", { zoom: 29.95 }],
  ["zoom-30", { zoom: 30 }],
  ["small-viewport", { width: 10, height: 10 }],
  ["zoom-lock", { zoom: 0.6, scrollConstraints: lock({ x: 0, y: 0, width: 4000, height: 3000, lockZoom: true, zoom: 0.5 }) }],
  ["scroll-lock", { zoom: 1, scrollX: -100, scrollY: -100, scrollConstraints: lock({ x: -200, y: -150, width: 2000, height: 1600, lockScroll: true, overscroll: 150 }) }],
  ["both-locks", { zoom: 0.9, scrollX: 30, scrollY: -30, scrollConstraints: lock({ x: -200, y: -150, width: 2000, height: 1600, lockScroll: true, lockZoom: true, zoom: 0.75, overscroll: 150 }) }],
];

const ACTION_SELECTIONS = [
  ["none", {}],
  ["rect", { rect: true }],
  ["rect-and-arrow", { rect: true, arrow: true }],
  ["deleted-only", { deleted: true }],
  ["freedraw-and-text", { freedraw: true, text: true }],
];

const ACTION_OFFSETS = [
  ["none", {}],
  ["ui", { left: 216, top: 60, right: 302, bottom: 50 }],
];

const actions = (up, window, elements) => {
  const performed = [];
  for (const name of ACTION_NAMES) {
    const fits = name.startsWith("zoomToFit");
    for (const [stateName, s] of ACTION_STATES) {
      const sets = fits ? ELEMENT_SETS.filter(([set]) => ["none", "all", "rect", "text"].includes(set)) : [["none", []]];
      for (const [set, ids] of sets) {
        const selections = fits ? ACTION_SELECTIONS : [["none", {}]];
        for (const [selection, selectedElementIds] of selections) {
          for (const [offsetsName, offsets] of fits ? ACTION_OFFSETS : [["none", {}]]) {
            const scene = byIds(elements, ids);
            const state = editorState({ ...s, selectedElementIds });
            const { app, calls } = makeApp(up, window, { state, elements: scene, offsets });
            const r = actionOf(up, name).perform(scene, state, null, app);
            performed.push({
              action: name,
              state: stateName,
              elements: set,
              selection,
              offsets: offsetsName,
              calls,
              result: viewportOf(r.appState),
              captureUpdate: r.captureUpdate,
            });
          }
        }
      }
    }
  }
  const keys = [];
  const codes = ["Equal", "NumpadAdd", "Minus", "NumpadSubtract", "Digit0", "Numpad0", "Digit1", "Digit2", "Digit3", "Digit4", "KeyZ"];
  const mods = [
    {},
    { shiftKey: true },
    { ctrlOrCmd: true },
    { ctrlOrCmd: true, shiftKey: true },
    { shiftKey: true, altKey: true },
    { altKey: true },
  ];
  for (const code of codes) {
    for (const m of mods) {
      const event = { code, shiftKey: !!m.shiftKey, altKey: !!m.altKey, ctrlKey: false, metaKey: false };
      event[up.KEYS.CTRL_OR_CMD] = !!m.ctrlOrCmd;
      keys.push({
        code,
        shiftKey: !!m.shiftKey,
        altKey: !!m.altKey,
        ctrlOrCmd: !!m.ctrlOrCmd,
        matches: ACTION_NAMES.filter((name) => actionOf(up, name).keyTest(event)),
      });
    }
  }
  return { elementStates: Object.fromEntries(ACTION_STATES.map(([n, s]) => [n, stateOut(editorState(s))])), performed, keys };
};

// -- running upstream --------------------------------------------------------------------------

const build = async (upstream) => {
  const window = installDom(ORIGIN);
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
  up.reseed(RANDOM_SEED);
  const elements = ELEMENTS(up);
  return format(
    encode({
      description:
        "Upstream's viewport code at the pinned commit (tools/goldens/viewport.mjs): getNormalizedZoom, the viewport/scene coordinate transforms, constrainScrollState, getViewportForZoomWithScrollConstraints, AppViewport.translate, AppViewport.setViewport step by step (animation frames on a controlled clock), zoomToFitBounds, centerScrollOn, scrollBoundsIntoView, getClosestElementBounds, getScrollToContentState, AppWheel.handle event by event and the zoom actions (perform and keyTest), on stand-in Apps. Text measures 10 px per UTF-16 code unit. Non-finite numbers are the strings NaN, Infinity and -Infinity.",
      upstream: upstream.commit,
      constants: { MIN_ZOOM: up.MIN_ZOOM, MAX_ZOOM: up.MAX_ZOOM, ZOOM_STEP: up.ZOOM_STEP, DEFAULT_OVERSCROLL: up.DEFAULT_OVERSCROLL },
      normalizedZoom: normalizedZoom(up),
      coords: coords(up),
      constrain: constrain(up),
      zoomAt: zoomAt(up),
      translate: translate(up, window),
      setViewport: setViewport(up, window),
      zoomToFitBounds: zoomToFitBounds(up),
      centerScrollOn: centerScrollOn(up),
      scrollBoundsIntoView: scrollBoundsIntoView(up),
      elements,
      elementSets: Object.fromEntries(ELEMENT_SETS),
      closestElementBounds: closestElementBounds(up, elements),
      scrollToContent: scrollToContent(up, elements),
      wheel: wheel(up, window),
      actions: actions(up, window, elements),
    }),
  );
};

/** Runs fn with Math.random disabled. */
const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating viewport goldens");
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
    process.stderr.write(`viewport: ${error.message}\n`);
    process.exit(1);
  }
  const text = await deterministic(() => build(upstream));
  const path = join(args.out ?? OUT_DIR, OUT_FILE);
  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${relative(process.cwd(), path) || path}\n`);
      process.stderr.write("viewport goldens are out of date: run node tools/goldens/viewport.mjs\n");
      process.exit(1);
    }
    process.stdout.write("viewport goldens up to date: 1 file\n");
    return;
  }
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${relative(process.cwd(), path) || path} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) await main();
