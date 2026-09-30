#!/usr/bin/env node
// Lasso selection fixtures for excali-editor (ex-705): upstream's own
// packages/excalidraw/lasso (LassoTrail and getLassoSelectedElementIds),
// run from the pinned checkout under plain Node on the scenes and paths of
// upstream's lasso.test.tsx and on seeded random scenes and paths.
//
//   node tools/goldens/lasso-fixtures.mjs            write the fixture
//   node tools/goldens/lasso-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/lasso-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/lasso.json:
//
//   { "description", "upstream", "cases": [ { id, scenes, ops } ] }
//
// `scenes` are the scenes a case uses (JSON as upstream holds them; the
// elements map is the non-deleted elements'). `ops` is what happened, in
// order, each naming its scene by index:
//
// - { op: "start", scene, point, keep, zoom, mode, state }: a
//   LassoTrail.startPath(x, y, keepPreviousSelection);
// - { op: "add", scene, point, keep, zoom, mode, state }:
//   addPointToPath(x, y, keepPreviousSelection), which reselects from the
//   trail (`updateSelection`, the app's visible elements, simplify distance
//   5 / zoom);
// - { op: "select", scene, ids, state }: selectElementsFromIds(ids);
// - { op: "end" }: endPath();
// - { op: "ids", scene, path, elements, simplifyDistance, mode, result }:
//   one call of getLassoSelectedElementIds (whether the trail or the test
//   made it): the path, the ids of the elements it was given, and the ids
//   it returned.
//
// `state` is the app state after the op: selectedElementIds and
// selectedGroupIds (objects, as upstream holds them) and
// selectedLinearElement (the element id of the editor it holds, or null).
// The state before a start is `prev` on the start op.
//
// Cases:
// - upstream-*: every test of packages/excalidraw/tests/lasso.test.tsx,
//   the test file itself bundled and run, with its own assertions (a
//   minimal expect), against a stand-in for the App it renders: the app
//   state from getDefaultAppState(), setState, a scene over `h.elements`,
//   every non-deleted element visible, and the real LassoTrail. The
//   trail's drawing (AnimationController) is inert; it never feeds the
//   selection.
// - random-*: seeded random scenes (shapes of every kind, rotated or not;
//   labels in containers and on arrows; groups, nested groups; frames with
//   children, some overflowing; locked elements) and lasso paths (closed
//   and open blobs, loops, zigzags), in both modes, at several zooms, some
//   keeping the previous selection.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), reseed(1) before each case, and random
// scenes come from a Park-Miller generator seeded per case. Math.random
// throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { JSDOM } from "jsdom";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "lasso.json";

const ENTRY = `
export { LassoTrail } from "./packages/excalidraw/lasso/index";
export { reseed } from "./packages/common/src/random";
export {
  arrayToMap,
  ROUNDNESS,
  getSizeFromPoints,
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
export { getSelectedElements } from "./packages/element/src/selection";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { getDefaultAppState } from "./packages/excalidraw/appState";
export const runLassoTests = async () => {
  await import("./packages/excalidraw/tests/lasso.test.tsx");
};
`;

// Every call of getLassoSelectedElementIds, the trail's and the test's,
// goes through the recorder; the function itself is upstream's, unchanged.
const PATCH = {
  "packages/excalidraw/lasso/utils": (source) => {
    const from = "export const getLassoSelectedElementIds = (";
    if (!source.includes(from)) throw new Error("lasso/utils: getLassoSelectedElementIds not found");
    return (
      source.replace(from, "const upstreamGetLassoSelectedElementIds = (") +
      "\nexport const getLassoSelectedElementIds = (input) =>\n" +
      "  globalThis.__lasso.recordIds(input, upstreamGetLassoSelectedElementIds(input));\n"
    );
  },
};

// The modules lasso.test.tsx imports for the React app it renders, stood
// in for by the harness below (globalThis.__lasso), and the trail's
// drawing, which never feeds the selection.
const SHIMS = {
  "packages/excalidraw/index": "module.exports = { Excalidraw: () => null };",
  "packages/excalidraw/tests/helpers/api":
    "module.exports = { API: { createElement: (opts) => globalThis.__lasso.createElement(opts) } };",
  "packages/excalidraw/tests/test-utils":
    "module.exports = { act: (fn) => fn(), render: async () => globalThis.__lasso.render() };",
  "packages/excalidraw/scene": "module.exports = { getSelectedElements: (...a) => globalThis.__lasso.getSelectedElements(...a) };",
  "packages/excalidraw/renderer/animation":
    "module.exports = { AnimationController: { start() {}, cancel() {}, running: () => false } };",
  "react/jsx-runtime": "module.exports = { jsx: () => null, jsxs: () => null, Fragment: null };",
};

const usage = () => {
  process.stderr.write("usage: lasso-fixtures.mjs [--check] [--out DIR]\n");
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

/** roughjs' Random (Park-Miller, multiplier 48271), seeded per case. */
const rng = (seed) => {
  let state = seed;
  const next = () => {
    state = Math.imul(48271, state);
    return ((2 ** 31 - 1) & state) / 2 ** 31;
  };
  return {
    next,
    int: (lo, hi) => lo + Math.floor(next() * (hi - lo + 1)),
    pick: (items) => items[Math.floor(next() * items.length)],
    chance: (p) => next() < p,
    range: (lo, hi) => lo + next() * (hi - lo),
  };
};

// -- the app lasso.test.tsx renders ---------------------------------------------

/**
 * A stand-in for the App: its state, setState, a scene over `elements` and
 * the real LassoTrail, recording every op into `recorder`.
 */
const createApp = (up, recorder) => {
  let elements = [];
  const nonDeleted = () => elements.filter((e) => !e.isDeleted);
  const scene = {
    getNonDeletedElement: (id) => nonDeleted().find((e) => e.id === id),
    getNonDeletedElements: () => nonDeleted(),
    getNonDeletedElementsMap: () => up.arrayToMap(nonDeleted()),
    getElementsMapIncludingDeleted: () => up.arrayToMap(elements),
    // Scene.getSelectedElements without its cache (scene/Scene.ts)
    getSelectedElements: (opts) => up.getSelectedElements(opts.elements ?? nonDeleted(), opts),
  };
  const app = {
    state: { ...up.getDefaultAppState(), width: 1000, height: 1000 },
    scene,
    interactiveCanvas: null,
    get visibleElements() {
      return nonDeleted();
    },
    setState(update) {
      const next = typeof update === "function" ? update(app.state) : update;
      app.state = { ...app.state, ...next };
    },
    setActiveTool() {},
  };
  const trail = new up.LassoTrail(app);
  const selectionState = () => ({
    selectedElementIds: clone(app.state.selectedElementIds),
    selectedGroupIds: clone(app.state.selectedGroupIds),
    selectedLinearElement: app.state.selectedLinearElement?.elementId ?? null,
  });
  const where = () => ({ zoom: app.state.zoom.value, mode: app.state.boxSelectionMode });
  const { startPath, endPath } = trail;
  const { addPointToPath, selectElementsFromIds } = trail;
  let starting = false;
  trail.startPath = (x, y, keep = false) => {
    const prev = selectionState();
    const scene = recorder.scene(elements);
    starting = true;
    try {
      startPath.call(trail, x, y, keep);
    } finally {
      starting = false;
    }
    recorder.op({ op: "start", scene, point: [x, y], keep, ...where(), prev, state: selectionState() });
  };
  trail.addPointToPath = (x, y, keep = false) => {
    const scene = recorder.scene(elements);
    const at = recorder.mark();
    adding = true;
    recorder.adding = true;
    try {
      addPointToPath(x, y, keep);
    } finally {
      adding = false;
      recorder.adding = false;
    }
    recorder.opAt(at, { op: "add", scene, point: [x, y], keep, ...where(), state: selectionState() });
  };
  let adding = false;
  trail.selectElementsFromIds = (ids) => {
    if (adding) return selectElementsFromIds(ids);
    const scene = recorder.scene(elements);
    selectElementsFromIds(ids);
    recorder.op({ op: "select", scene, ids: [...ids], state: selectionState() });
  };
  trail.endPath = () => {
    endPath.call(trail);
    // startPath ends any trail first
    if (!starting) recorder.op({ op: "end" });
  };
  app.lassoTrail = trail;
  const h = {
    app,
    scene,
    get state() {
      return app.state;
    },
    get elements() {
      return elements;
    },
    set elements(next) {
      elements = [...next];
    },
  };
  return { app, h, trail, setElements: (next) => (h.elements = next) };
};

/** One case's ops and the scenes they name. */
const createRecorder = () => {
  const scenes = [];
  const sceneTexts = [];
  const ops = [];
  let pending = [];
  const recorder = {
    scenes,
    ops,
    scene(elements) {
      const text = JSON.stringify(elements);
      let i = sceneTexts.indexOf(text);
      if (i < 0) {
        i = sceneTexts.length;
        sceneTexts.push(text);
        scenes.push(JSON.parse(text));
      }
      return i;
    },
    op(op) {
      ops.push(...pending, op);
      pending = [];
    },
    // the ids calls an add makes are recorded after it
    mark() {
      return ops.length;
    },
    opAt(at, op) {
      ops.splice(at, 0, op);
      ops.push(...pending);
      pending = [];
    },
    recordIds(input, result, elements) {
      const ids = input.elements.map((e) => e.id);
      const visible = elements.filter((e) => !e.isDeleted).map((e) => e.id);
      pending.push({
        op: "ids",
        scene: recorder.currentScene,
        // the trail's own call takes its points so far and the visible
        // elements (both written as null)
        path: recorder.adding ? null : input.lassoPath.map((p) => [p[0], p[1]]),
        elements: deepEqual(ids, visible) ? null : ids,
        simplifyDistance: input.simplifyDistance ?? null,
        mode: input.mode ?? "contain",
        result: [...result.selectedElementIds],
      });
      return result;
    },
  };
  return recorder;
};

// -- a minimal describe / it / expect -----------------------------------------------

const deepEqual = (a, b) => JSON.stringify(a) === JSON.stringify(b);

const expectFn = (actual) => {
  const make = (negate) => {
    const check = (pass, what) => {
      if (pass === negate) {
        throw new Error(`expect(${JSON.stringify(actual)})${negate ? ".not" : ""}.${what} failed`);
      }
    };
    return {
      toBe: (v) => check(Object.is(actual, v), `toBe(${JSON.stringify(v)})`),
      toEqual: (v) => check(deepEqual(actual, v), `toEqual(${JSON.stringify(v)})`),
      toContain: (v) => check(actual.includes(v), `toContain(${JSON.stringify(v)})`),
      toBeDefined: () => check(actual !== undefined, "toBeDefined()"),
    };
  };
  return { ...make(false), not: make(true) };
};

const collectTests = async (up) => {
  const root = { name: "", before: [], children: [] };
  let current = root;
  globalThis.describe = (name, fn) => {
    const node = { name, before: [], children: [], parent: current };
    current.children.push(node);
    const outer = current;
    current = node;
    fn();
    current = outer;
  };
  globalThis.it = (name, fn) => current.children.push({ name, fn, parent: current });
  globalThis.beforeEach = (fn) => current.before.push(fn);
  globalThis.expect = expectFn;
  await up.runLassoTests();
  const tests = [];
  const walk = (node, path) => {
    for (const child of node.children) {
      if (child.fn) tests.push({ path: [...path, child.name], test: child });
      else walk(child, [...path, child.name]);
    }
  };
  walk(root, []);
  return tests;
};

const slug = (text) =>
  text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

const beforeChain = (node) => {
  const chain = [];
  for (let n = node.parent; n; n = n.parent) chain.unshift(...n.before);
  return chain;
};

const runUpstreamTest = async (up, { path, test }) => {
  const recorder = createRecorder();
  const { h, app } = createApp(up, recorder);
  globalThis.__lasso.h = h;
  globalThis.__lasso.render = () => {};
  globalThis.__lasso.createElement = (opts) => apiCreateElement(up, opts);
  globalThis.__lasso.getSelectedElements = up.getSelectedElements;
  globalThis.__lasso.recordIds = (input, result) => {
    recorder.currentScene = recorder.scene(h.elements);
    return recorder.recordIds(input, result, h.elements);
  };
  void app;
  for (const before of beforeChain(test)) await before();
  await test.fn();
  recorder.op(null);
  recorder.ops.pop();
  if (!recorder.ops.length) throw new Error(`upstream test ${path.join(" > ")} drew no lasso`);
  return { id: `upstream-${slug(path.join(" "))}`, scenes: recorder.scenes, ops: recorder.ops };
};

// -- random scenes --------------------------------------------------------------------

const BACKGROUNDS = ["transparent", "#ffc9c9"];

const randomPoints = (r, n, spread) => {
  const pts = [[0, 0]];
  for (let i = 1; i < n; i++) pts.push([r.int(-spread, spread), r.int(-spread, spread)]);
  return pts;
};

const loopPoints = (r, n, rx, ry) => {
  const pts = [];
  for (let i = 0; i < n; i++) {
    const a = (i / n) * 2 * Math.PI;
    pts.push([rx + rx * Math.cos(a), ry + ry * Math.sin(a)]);
  }
  pts.push([...pts[0]]);
  const [x0, y0] = pts[0];
  return pts.map(([x, y]) => [x - x0, y - y0]);
};

const SHAPES = [
  "rectangle",
  "rectangle-round",
  "diamond",
  "ellipse",
  "text",
  "container",
  "arrow-label",
  "image",
  "stickynote",
  "line",
  "line-round",
  "line-loop",
  "arrow",
  "arrow-elbow",
  "freedraw",
  "freedraw-dot",
  "embeddable",
];

/** One random element of `kind` near (cx, cy) (a container and its label). */
const randomShape = (up, r, kind, [cx, cy], extra = {}) => {
  const x = cx + r.int(-120, 120);
  const y = cy + r.int(-120, 120);
  const width = r.int(10, 160);
  const height = r.pick([r.int(10, 160), width]);
  const angle = r.chance(0.3) ? r.pick([Math.PI / 4, 0.3, 5.5, r.range(0, 2 * Math.PI)]) : 0;
  const common = {
    x,
    y,
    width,
    height,
    angle,
    strokeWidth: r.pick([1, 2, 4]),
    backgroundColor: r.pick(BACKGROUNDS),
    locked: r.chance(0.08),
    ...extra,
  };
  const round = { type: up.ROUNDNESS.PROPORTIONAL_RADIUS };
  const linear = (type, points, rest = {}) =>
    apiCreateElement(up, { type, ...common, ...up.getSizeFromPoints(points), points, roundness: null, ...rest });
  switch (kind) {
    case "rectangle":
    case "diamond":
    case "ellipse":
    case "image":
    case "embeddable":
      return [apiCreateElement(up, { type: kind, ...common, roundness: null })];
    case "rectangle-round":
      return [apiCreateElement(up, { type: "rectangle", ...common, roundness: round })];
    case "stickynote":
      return [apiCreateElement(up, { type: "stickynote", ...common })];
    case "text":
      return [apiCreateElement(up, { type: "text", text: "lasso me", ...common })];
    case "container": {
      const container = apiCreateElement(up, { type: r.pick(["rectangle", "diamond", "ellipse"]), ...common });
      const label = apiCreateElement(up, {
        type: "text",
        text: "inside",
        x: x + width / 4,
        y: y + height / 3,
        width: width / 2,
        height: height / 3,
        angle,
        containerId: container.id,
        groupIds: common.groupIds,
        frameId: common.frameId,
      });
      container.boundElements = [{ id: label.id, type: "text" }];
      return [container, label];
    }
    case "arrow-label": {
      const arrow = linear("arrow", randomPoints(r, r.int(2, 3), 150));
      const label = apiCreateElement(up, {
        type: "text",
        text: "label",
        x: x + 20,
        y: y + 20,
        width: 50,
        height: 25,
        containerId: arrow.id,
        groupIds: common.groupIds,
        frameId: common.frameId,
      });
      arrow.boundElements = [{ id: label.id, type: "text" }];
      return [arrow, label];
    }
    case "line":
      return [linear("line", randomPoints(r, r.int(2, 5), 150))];
    case "line-round":
      return [linear("line", randomPoints(r, r.int(3, 5), 150), { roundness: round })];
    case "line-loop":
      return [linear("line", loopPoints(r, r.int(3, 8), r.int(20, 80), r.int(20, 80)))];
    case "arrow":
      return [linear("arrow", randomPoints(r, r.int(2, 4), 150))];
    case "arrow-elbow": {
      const a = r.int(-150, 150);
      const b = r.int(-150, 150);
      return [linear("arrow", [[0, 0], [a, 0], [a, b]], { elbowed: true, angle: 0 })];
    }
    case "freedraw": {
      const points = [[0, 0]];
      for (let i = 1; i < r.int(3, 20); i++) {
        const [px, py] = points[points.length - 1];
        points.push([px + r.range(-15, 15), py + r.range(-15, 15)]);
      }
      return [apiCreateElement(up, { type: "freedraw", ...common, ...up.getSizeFromPoints(points), points })];
    }
    case "freedraw-dot":
      return [apiCreateElement(up, { type: "freedraw", ...common, width: 0, height: 0, points: [[0, 0]] })];
  }
  throw new Error(`unknown kind ${kind}`);
};

/** A random scene around the origin: loose shapes, groups and frames. */
const randomScene = (up, r) => {
  const elements = [];
  const at = () => [r.int(-300, 300), r.int(-300, 300)];
  const loose = r.int(2, 6);
  for (let i = 0; i < loose; i++) elements.push(...randomShape(up, r, r.pick(SHAPES), at()));
  // groups, some nested
  for (let g = 0; g < r.int(0, 2); g++) {
    const groupIds = r.chance(0.4) ? [`inner-${g}`, `outer-${g}`] : [`group-${g}`];
    const centre = at();
    for (let i = 0; i < r.int(2, 3); i++) {
      const ids = r.chance(0.5) ? groupIds : [groupIds[groupIds.length - 1]];
      elements.push(...randomShape(up, r, r.pick(SHAPES), centre, { groupIds: ids }));
    }
  }
  // a frame with children, some overflowing it
  if (r.chance(0.5)) {
    const [fx, fy] = at();
    const frame = apiCreateElement(up, {
      type: r.pick(["frame", "magicframe"]),
      x: fx,
      y: fy,
      width: r.int(120, 320),
      height: r.int(120, 320),
    });
    const children = [];
    for (let i = 0; i < r.int(1, 3); i++) {
      children.push(
        ...randomShape(up, r, r.pick(SHAPES), [fx + frame.width / 2, fy + frame.height / 2], { frameId: frame.id }),
      );
    }
    elements.push(...children, frame);
  }
  return elements;
};

/** A random lasso path: a closed or open blob, a loop, or a zigzag. */
const randomPath = (r) => {
  const [cx, cy] = [r.int(-300, 300), r.int(-300, 300)];
  const rx = r.int(20, 400);
  const ry = r.int(20, 400);
  const shape = r.pick(["blob", "blob", "open", "zigzag", "line"]);
  const pts = [];
  if (shape === "zigzag") {
    const n = r.int(3, 10);
    for (let i = 0; i < n; i++) pts.push([cx - rx + (2 * rx * i) / n, cy + (i % 2 ? ry : -ry) * r.range(0.3, 1)]);
  } else if (shape === "line") {
    pts.push([cx - rx, cy - ry], [cx + rx, cy + ry]);
  } else {
    const n = r.int(6, 60);
    const sweep = shape === "open" ? r.range(0.4, 0.9) : 1;
    const start = r.range(0, 2 * Math.PI);
    const wobble = r.range(0, 0.4);
    for (let i = 0; i <= n; i++) {
      const a = start + (i / n) * 2 * Math.PI * sweep;
      const k = 1 + wobble * Math.sin(5 * a);
      pts.push([cx + rx * k * Math.cos(a), cy + ry * k * Math.sin(a)]);
    }
  }
  // pointer positions are fractional in upstream's recorded paths
  return pts.map(([x, y]) => [Math.round(x * 16) / 16, Math.round(y * 16) / 16]);
};

const ZOOMS = [0.25, 0.5, 1, 1, 2, 4];

const runRandomCase = (up, seed) => {
  const r = rng(seed);
  const recorder = createRecorder();
  const { app, h, trail } = createApp(up, recorder);
  globalThis.__lasso.recordIds = (input, result) => {
    recorder.currentScene = recorder.scene(h.elements);
    return recorder.recordIds(input, result, h.elements);
  };
  h.elements = randomScene(up, r);
  const gestures = r.int(1, 3);
  for (let g = 0; g < gestures; g++) {
    app.setState({
      boxSelectionMode: r.pick(["contain", "overlap"]),
      zoom: { value: r.pick(ZOOMS) },
    });
    const keep = g > 0 && r.chance(0.4);
    const path = randomPath(r);
    trail.startPath(path[0][0], path[0][1], keep);
    for (const [x, y] of path.slice(1)) trail.addPointToPath(x, y, keep);
    trail.endPath();
  }
  if (recorder.ops.some((op) => op.op === "ids" && op.scene === undefined)) throw new Error("unscened ids op");
  return { id: `random-${seed}`, scenes: recorder.scenes, ops: recorder.ops };
};

const RANDOM_CASES = 120;

// -- output ---------------------------------------------------------------------------

const deterministic = async (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating lasso fixtures");
  };
  try {
    return await fn();
  } finally {
    Math.random = random;
  }
};

const asciiJson = (fixture) =>
  format(fixture).replace(/[\u0080-￿]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`);

const buildFixture = async (up, commit) => {
  const cases = [];
  const ids = new Set();
  const push = (c) => {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
    cases.push(c);
  };
  for (const t of await collectTests(up)) {
    up.reseed(1);
    push(await runUpstreamTest(up, t));
  }
  for (let seed = 1; seed <= RANDOM_CASES; seed++) {
    up.reseed(1);
    push(runRandomCase(up, seed));
  }
  return asciiJson({
    description:
      "LassoTrail (startPath, addPointToPath, selectElementsFromIds, endPath) and getLassoSelectedElementIds " +
      "(packages/excalidraw/lasso/index.ts, utils.ts) on lasso.test.tsx's tests and seeded random scenes, in " +
      "upstream's test mode (ids id0.., timestamps 1, reseed(1) before each case). Generated by " +
      "tools/goldens/lasso-fixtures.mjs.",
    upstream: commit,
    cases,
  });
};

const installDom = () => {
  const dom = new JSDOM("<!doctype html><html><head></head><body></body></html>", { url: "http://localhost/" });
  for (const [key, value] of Object.entries({ window: dom.window, document: dom.window.document, localStorage: dom.window.localStorage, devicePixelRatio: 1 })) {
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }
  // lasso.test.tsx reads `const { h } = window` once, as it loads: a
  // stand-in for each test's h
  globalThis.__lasso = {};
  const current = () => globalThis.__lasso.h;
  dom.window.h = {
    get app() {
      return current().app;
    },
    get scene() {
      return current().scene;
    },
    get state() {
      return current().state;
    },
    get elements() {
      return current().elements;
    },
    set elements(next) {
      current().elements = next;
    },
  };
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`lasso-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  installDom();
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    shims: SHIMS,
    patch: PATCH,
    define: { "import.meta.env.MODE": '"test"' },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  const text = await deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("lasso fixture is out of date: run node tools/goldens/lasso-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`lasso fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
