#!/usr/bin/env node
// Flowchart fixtures for excali-editor (ex-534): upstream's own
// packages/element/src/flowchart.ts (FlowChartCreator, FlowChartNavigator,
// addNewNodes, the module-private findNearestFreeSlot, placeCluster and
// getConnectedFlowchartNodes), run from the pinned checkout under plain
// Node on hand-written and seeded random scenes.
//
//   node tools/goldens/flowchart-fixtures.mjs            write the fixture
//   node tools/goldens/flowchart-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/flowchart-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/flowchart.json:
//
//   { "description", "upstream", "slots", "clusters", "create", "navigate",
//     "inFlowchart" }
//
// - slots: findNearestFreeSlot { ideal, size, occupied, result }
// - clusters: placeCluster { parent, direction, count, obstacles, sticky,
//   result: { positions, crossStart } } (parent an element, obstacles
//   bounds, sticky null or a number)
// - create: one FlowChartCreator per case on the case's scene
//   (`elements`), with `appState` { currentItemEndArrowhead, zoom }; the
//   steps, in order, are
//   - { op: "create", start, direction, pending, changed }: createNodes on
//     the element `start` of the scene; `pending` is creator.pendingNodes
//     after it and `changed` the scene elements whose JSON differs from the
//     case's `elements` after it (the start node gains the arrows in
//     boundElements)
//   - { op: "clear" }: creator.clear()
// - navigate: one FlowChartNavigator per case on its scene; the steps are
//   - { op: "explore", from, direction, result, isExploring }:
//     exploreByDirection(from, elementsMap, direction), the id it returns
//     (or null) and navigator.isExploring after it
//   - { op: "clear" }: navigator.clear()
// - inFlowchart: isNodeInFlowchart { elements, node, result }
//
// Deterministic: upstream runs in its test mode (ids id0, id1, ...,
// timestamps 1), reseed(1) right before each case's steps, and random
// scenes come from a Park-Miller generator seeded per case. Math.random
// throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "flowchart.json";

const ENTRY = `
export {
  FlowChartCreator,
  FlowChartNavigator,
  addNewNodes,
  isNodeInFlowchart,
  findNearestFreeSlot,
  placeCluster,
} from "./packages/element/src/flowchart";
export { Scene } from "./packages/element/src/Scene";
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
  newTextElement,
  newArrowElement,
  newLinearElement,
  newFreeDrawElement,
} from "./packages/element/src/newElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

// module-private: exported as they are
const EXPOSE = { "packages/element/src/flowchart": ["findNearestFreeSlot", "placeCluster"] };

const usage = () => {
  process.stderr.write("usage: flowchart-fixtures.mjs [--check] [--out DIR]\n");
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

/** Rounds to 1/16 so random geometry stays exactly representable. */
const q = (v) => Math.round(v * 16) / 16;

const DIRECTIONS = ["up", "right", "down", "left"];
const NODE_TYPES = ["rectangle", "rectangle", "diamond", "ellipse", "stickynote"];

const el = (up, opts) => apiCreateElement(up, { roughness: 0, ...opts });

const sceneOf = (up, elements) => new up.Scene(elements, { skipValidation: true });

const appStateOf = (up, a) => ({
  ...up.getDefaultAppState(),
  currentItemEndArrowhead: a.currentItemEndArrowhead,
  zoom: { value: a.zoom },
});

const randomNode = (up, r, id, extra = {}) => {
  const type = r.pick(NODE_TYPES);
  return el(up, {
    type,
    id,
    x: q(r.range(-300, 300)),
    y: q(r.range(-300, 300)),
    width: q(r.range(40, 220)),
    height: q(r.range(30, 160)),
    angle: r.chance(0.1) ? q(r.range(0, 2 * Math.PI)) : 0,
    strokeWidth: r.pick([1, 2, 4]),
    strokeStyle: r.pick(["solid", "solid", "dashed", "dotted"]),
    backgroundColor: type === "stickynote" ? undefined : r.pick(["transparent", "#ffc9c9"]),
    fillStyle: r.pick(["solid", "hachure"]),
    opacity: r.pick([100, 100, 60]),
    roundness: r.chance(0.5),
    ...extra,
  });
};

/**
 * Grows the flowchart from `startId` as App commits it: `count` Ctrl+Arrow
 * presses in `direction`, then the pending nodes appended to the scene.
 */
const grow = (up, elements, startId, direction, count, appState) => {
  const scene = sceneOf(up, elements);
  const start = scene.getNonDeletedElementsMap().get(startId);
  const creator = new up.FlowChartCreator();
  for (let i = 0; i < count; i++) {
    creator.createNodes(start, appState, direction, scene);
  }
  return [...scene.getElementsIncludingDeleted(), ...creator.pendingNodes].map(clone);
};

/** A hand-bound elbow arrow from `a` to `b` whose points are its ends. */
const handArrow = (up, id, a, b, fromSide, toSide) => {
  const at = (e, side) => {
    const [fx, fy] = { up: [0.5, 0], right: [1, 0.5], down: [0.5, 1], left: [0, 0.5] }[side];
    return [e.x + fx * e.width, e.y + fy * e.height, [fx, fy]];
  };
  const [sx, sy, sf] = at(a, fromSide);
  const [ex, ey, ef] = at(b, toSide);
  const arrow = el(up, {
    type: "arrow",
    id,
    x: sx,
    y: sy,
    width: Math.abs(ex - sx),
    height: Math.abs(ey - sy),
    points: [
      [0, 0],
      [ex - sx, ey - sy],
    ],
    elbowed: true,
    roundness: false,
  });
  arrow.startBinding = { elementId: a.id, fixedPoint: sf, mode: "orbit" };
  arrow.endBinding = { elementId: b.id, fixedPoint: ef, mode: "orbit" };
  a.boundElements = [...(a.boundElements ?? []), { id, type: "arrow" }];
  b.boundElements = [...(b.boundElements ?? []), { id, type: "arrow" }];
  return arrow;
};

/**
 * The ids upstream drew while building a scene (id0, id1, ...) renamed, so
 * the ids the case's steps draw after reseed(1) are new.
 */
const renameDrawnIds = (elements) => {
  const rename = (id) => (/^id\d+$/.test(id) ? `n${id}` : id);
  return elements.map((e) => {
    const next = { ...e, id: rename(e.id) };
    if (e.boundElements) next.boundElements = e.boundElements.map((b) => ({ ...b, id: rename(b.id) }));
    if (e.frameId) next.frameId = rename(e.frameId);
    for (const key of ["startBinding", "endBinding"]) {
      if (e[key]) next[key] = { ...e[key], elementId: rename(e[key].elementId) };
    }
    return next;
  });
};

// -- findNearestFreeSlot and placeCluster -------------------------------------

const slotCases = (up) => {
  const out = [];
  const add = (ideal, size, occupied) =>
    out.push({ ideal, size, occupied, result: up.findNearestFreeSlot(ideal, size, occupied) });
  add(0, 100, []);
  add(0, 100, [{ start: -50, end: 50 }]);
  add(0, 100, [{ start: -200, end: 60 }]);
  add(0, 100, [{ start: -60, end: 200 }]);
  add(0, 100, [{ start: -100, end: 100 }]);
  add(-50, 100, [{ start: -150, end: 0 }, { start: 100, end: 300 }]);
  add(-50, 100, [{ start: -150, end: 0 }, { start: 50, end: 300 }]);
  add(10, 40, [{ start: 0, end: 30 }, { start: 60, end: 90 }, { start: 130, end: 160 }]);
  const r = rng(271828);
  for (let i = 0; i < 40; i++) {
    const n = r.int(0, 5);
    let at = q(r.range(-400, -100));
    const occupied = [];
    for (let k = 0; k < n; k++) {
      const start = at + q(r.range(0, 120));
      const end = start + q(r.range(10, 200));
      occupied.push({ start, end });
      at = end;
    }
    add(q(r.range(-300, 300)), q(r.range(10, 250)), occupied);
  }
  return out;
};

const clusterCases = (up) => {
  const out = [];
  const r = rng(314159);
  for (let i = 0; i < 40; i++) {
    const parent = randomNode(up, r, `p${i}`);
    const obstacles = [];
    for (let k = r.int(0, 5); k > 0; k--) {
      const x = q(r.range(-600, 600));
      const y = q(r.range(-600, 600));
      obstacles.push([x, y, x + q(r.range(20, 250)), y + q(r.range(20, 200))]);
    }
    const direction = r.pick(DIRECTIONS);
    const count = r.int(1, 4);
    const sticky = r.chance(0.3) ? q(r.range(-400, 400)) : null;
    out.push({
      parent: clone(parent),
      direction,
      count,
      obstacles,
      sticky,
      result: up.placeCluster(parent, direction, count, obstacles, sticky),
    });
  }
  return out;
};

// -- the creator ----------------------------------------------------------------

const runCreate = (up, id, elements, a, script) => {
  up.reseed(1);
  const scene = sceneOf(up, renameDrawnIds(elements.map(clone)));
  // the scene as its constructor left it (fractional indices synced)
  const initial = scene.getElementsIncludingDeleted().map(clone);
  const appState = appStateOf(up, a);
  const creator = new up.FlowChartCreator();
  const steps = [];
  up.reseed(1);
  for (const step of script) {
    if (step === "clear") {
      creator.clear();
      steps.push({ op: "clear" });
      continue;
    }
    const [start, direction] = step;
    const node = scene.getNonDeletedElementsMap().get(start);
    creator.createNodes(node, appState, direction, scene);
    const prior = new Map(initial.map((e) => [e.id, JSON.stringify(e)]));
    steps.push({
      op: "create",
      start,
      direction,
      pending: creator.pendingNodes.map(clone),
      changed: scene
        .getElementsIncludingDeleted()
        .filter((e) => prior.get(e.id) !== JSON.stringify(e))
        .map(clone),
    });
  }
  return { id, elements: initial, appState: a, steps };
};

const handCreateCases = (up) => {
  const out = [];
  const A = { currentItemEndArrowhead: "arrow", zoom: 1 };
  const rect = (id, x, y, w = 100, h = 60, extra = {}) =>
    el(up, { type: "rectangle", id, x, y, width: w, height: h, ...extra });

  up.reseed(1);
  out.push(runCreate(up, "single-right", [rect("a", 0, 0)], A, [["a", "right"]]));
  for (const d of DIRECTIONS) {
    up.reseed(1);
    out.push(
      runCreate(up, `grow-${d}`, [rect("a", 0, 0)], A, [
        ["a", d],
        ["a", d],
        ["a", d],
      ]),
    );
  }
  up.reseed(1);
  out.push(
    runCreate(up, "turn", [rect("a", 0, 0)], A, [
      ["a", "right"],
      ["a", "right"],
      ["a", "down"],
      ["a", "down"],
      "clear",
      ["a", "down"],
    ]),
  );
  for (const type of ["diamond", "ellipse", "stickynote"]) {
    up.reseed(1);
    const node = el(up, { type, id: "a", x: 10, y: 20, width: 120, height: 80, roundness: true });
    out.push(
      runCreate(up, `template-${type}`, [node], { currentItemEndArrowhead: "triangle", zoom: 2 }, [
        ["a", "left"],
        ["a", "left"],
      ]),
    );
  }
  up.reseed(1);
  out.push(
    runCreate(up, "no-arrowhead", [rect("a", 0, 0)], { currentItemEndArrowhead: null, zoom: 0.5 }, [
      ["a", "up"],
    ]),
  );

  // an existing flowchart's nodes are obstacles in the band
  {
    up.reseed(1);
    let elements = [rect("a", 0, 0)];
    elements = grow(up, elements, "a", "right", 1, appStateOf(up, A));
    elements = grow(up, elements, "a", "down", 1, appStateOf(up, A));
    up.reseed(1);
    out.push(
      runCreate(up, "obstacles", elements, A, [
        ["a", "right"],
        ["a", "right"],
        ["a", "right"],
      ]),
    );
  }

  // an unconnected shape is not an obstacle
  up.reseed(1);
  out.push(
    runCreate(up, "unconnected", [rect("a", 0, 0), rect("b", 200, 0)], A, [["a", "right"]]),
  );

  // frames: the pending nodes join the start node's frame when every one
  // of them overlaps it
  const frame = (id, x, y, w, h) =>
    up.newFrameElement({ x, y, width: w, height: h, roughness: 0, strokeColor: "#bbb" });
  for (const [name, w] of [
    ["frame-inside", 800],
    ["frame-partial", 300],
    ["frame-outside", 120],
  ]) {
    up.reseed(1);
    const f = frame("f", -10, -10, w, 200);
    f.id = "f";
    const a = rect("a", 0, 0, 100, 60, { frameId: "f" });
    out.push(
      runCreate(up, name, [f, a], A, [
        ["a", "right"],
        ["a", "right"],
        ["a", "right"],
      ]),
    );
  }
  return out;
};

const randomCreateCases = (up) => {
  const out = [];
  for (let i = 0; i < 40; i++) {
    const r = rng(1000003 + i * 7907);
    up.reseed(1);
    let elements = [randomNode(up, r, "root")];
    const a = {
      currentItemEndArrowhead: r.pick(["arrow", "triangle", null, "bar"]),
      zoom: r.pick([1, 1, 0.5, 2]),
    };
    // grow a connected flowchart around the root and its children
    for (let k = r.int(0, 4); k > 0; k--) {
      const nodes = elements.filter((e) => e.type !== "arrow" && e.type !== "frame");
      const from = r.pick(nodes).id;
      elements = grow(up, elements, from, r.pick(DIRECTIONS), r.int(1, 3), appStateOf(up, a));
    }
    // unconnected shapes
    for (let k = r.int(0, 2); k > 0; k--) {
      elements.push(randomNode(up, r, `loose${k}`));
    }
    const nodes = elements.filter((e) => e.type !== "arrow");
    const script = [];
    let start = renameDrawnIds([r.pick(nodes)])[0].id;
    let direction = r.pick(DIRECTIONS);
    for (let s = r.int(1, 6); s > 0; s--) {
      if (r.chance(0.1)) {
        script.push("clear");
      }
      if (r.chance(0.25)) direction = r.pick(DIRECTIONS);
      if (r.chance(0.1)) start = renameDrawnIds([r.pick(nodes)])[0].id;
      script.push([start, direction]);
    }
    out.push(runCreate(up, `random-${String(i).padStart(3, "0")}`, elements, a, script));
  }
  return out;
};

// -- the navigator ----------------------------------------------------------------

const runNavigate = (up, id, elements, script) => {
  up.reseed(1);
  const scene = sceneOf(up, elements.map(clone));
  const map = scene.getNonDeletedElementsMap();
  const navigator = new up.FlowChartNavigator();
  const steps = [];
  let from = null;
  for (const step of script) {
    if (step === "clear") {
      navigator.clear();
      steps.push({ op: "clear" });
      continue;
    }
    const [start, direction] = step;
    from = start ?? from;
    const result = navigator.exploreByDirection(map.get(from), map, direction);
    steps.push({ op: "explore", from, direction, result, isExploring: navigator.isExploring });
    // App selects the node it found
    if (result) from = result;
  }
  return { id, elements: elements.map(clone), steps };
};

const handNavigateCases = (up) => {
  const out = [];
  const A = appStateOf(up, { currentItemEndArrowhead: "arrow", zoom: 1 });
  const rect = (id, x, y) => el(up, { type: "rectangle", id, x, y, width: 100, height: 60 });

  up.reseed(1);
  let tree = [rect("a", 0, 0)];
  tree = grow(up, tree, "a", "right", 3, A);
  tree = grow(up, tree, "a", "down", 2, A);
  const kids = tree.filter((e) => e.type === "rectangle" && e.id !== "a").map((e) => e.id);
  tree = grow(up, tree, kids[0], "right", 2, A);
  out.push(
    runNavigate(up, "tree", tree, [
      ["a", "right"],
      [null, "right"],
      [null, "right"],
      [null, "right"],
      "clear",
      [null, "left"],
      [null, "left"],
      "clear",
      ["a", "down"],
      [null, "down"],
      [null, "up"],
      [null, "up"],
      ["a", "left"],
      [null, "left"],
      [null, "left"],
      [null, "left"],
      "clear",
      ["a", "up"],
    ]),
  );

  // hand-bound arrows (predecessors come from the arrows' ends)
  up.reseed(1);
  const n = [rect("a", 0, 0), rect("b", 300, 0), rect("c", 0, 300), rect("d", -300, 0)];
  const arrows = [
    handArrow(up, "ab", n[0], n[1], "right", "left"),
    handArrow(up, "ca", n[2], n[0], "up", "down"),
    handArrow(up, "da", n[3], n[0], "right", "left"),
    handArrow(up, "bc", n[1], n[2], "down", "right"),
  ];
  out.push(
    runNavigate(up, "hand-arrows", [...n, ...arrows], [
      ["a", "right"],
      [null, "down"],
      [null, "up"],
      [null, "up"],
      "clear",
      ["a", "left"],
      [null, "left"],
      [null, "left"],
      "clear",
      ["b", "left"],
      [null, "left"],
      ["c", "right"],
      ["c", "right"],
    ]),
  );

  // not bindable: a line
  up.reseed(1);
  const line = el(up, { type: "line", id: "l", x: 0, y: 0, points: [[0, 0], [50, 50]] });
  out.push(runNavigate(up, "not-bindable", [line, rect("a", 0, 100)], [["l", "right"], ["a", "right"]]));
  return out;
};

const randomNavigateCases = (up) => {
  const out = [];
  for (let i = 0; i < 40; i++) {
    const r = rng(2000029 + i * 6007);
    up.reseed(1);
    const a = appStateOf(up, { currentItemEndArrowhead: "arrow", zoom: 1 });
    let elements = [randomNode(up, r, "root")];
    for (let k = r.int(1, 6); k > 0; k--) {
      const nodes = elements.filter((e) => e.type !== "arrow");
      elements = grow(up, elements, r.pick(nodes).id, r.pick(DIRECTIONS), r.int(1, 3), a);
    }
    // a hand-bound elbow arrow between two nodes
    if (r.chance(0.5)) {
      const nodes = elements.filter((e) => e.type !== "arrow");
      const from = r.pick(nodes);
      const to = r.pick(nodes);
      if (from !== to) {
        elements.push(handArrow(up, "hand", from, to, r.pick(DIRECTIONS), r.pick(DIRECTIONS)));
      }
    }
    const nodes = elements.filter((e) => e.type !== "arrow");
    const script = [];
    let direction = r.pick(DIRECTIONS);
    script.push([r.pick(nodes).id, direction]);
    for (let s = r.int(3, 12); s > 0; s--) {
      if (r.chance(0.1)) script.push("clear");
      if (r.chance(0.3)) direction = r.pick(DIRECTIONS);
      script.push([r.chance(0.1) ? r.pick(nodes).id : null, direction]);
    }
    out.push(runNavigate(up, `random-${String(i).padStart(3, "0")}`, elements, script));
  }
  return out;
};

// -- isNodeInFlowchart ---------------------------------------------------------------

const inFlowchartCases = (up) => {
  up.reseed(1);
  const A = appStateOf(up, { currentItemEndArrowhead: "arrow", zoom: 1 });
  const rect = (id, x, y) => el(up, { type: "rectangle", id, x, y, width: 100, height: 60 });
  let elements = [rect("a", 0, 0), rect("b", 400, 400)];
  elements = grow(up, elements, "a", "right", 1, A);
  const plain = el(up, {
    type: "arrow",
    id: "plain",
    x: 400,
    y: 300,
    points: [[0, 0], [10, 90]],
    elbowed: false,
  });
  const scene = sceneOf(up, [...elements.map(clone)]);
  const map = scene.getNonDeletedElementsMap();
  const out = [];
  for (const e of elements.filter((e) => e.type !== "arrow")) {
    out.push({ elements: elements.map(clone), node: e.id, result: up.isNodeInFlowchart(e, map) });
  }
  plain.endBinding = { elementId: "b", fixedPoint: [0.5, 0], mode: "orbit" };
  const withPlain = [...elements, plain];
  const map2 = sceneOf(up, withPlain.map(clone)).getNonDeletedElementsMap();
  out.push({ elements: withPlain.map(clone), node: "b", result: up.isNodeInFlowchart(map2.get("b"), map2) });
  return out;
};

// -- driver --------------------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating flowchart fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const buildFixture = (up, commit) => {
  up.reseed(1);
  return format({
    description:
      "flowchart.ts (findNearestFreeSlot, placeCluster, FlowChartCreator.createNodes with addNewNodes, createBindingArrow and the frame rule, FlowChartNavigator.exploreByDirection, isNodeInFlowchart) on hand-written and seeded random scenes, in upstream's test mode (reseed(1) before each case's steps). Generated by tools/goldens/flowchart-fixtures.mjs.",
    upstream: commit,
    slots: slotCases(up),
    clusters: clusterCases(up),
    create: [...handCreateCases(up), ...randomCreateCases(up)],
    navigate: [...handNavigateCases(up), ...randomNavigateCases(up)],
    inFlowchart: inFlowchartCases(up),
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`flowchart-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
    expose: EXPOSE,
  });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("flowchart fixture is out of date: run node tools/goldens/flowchart-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`flowchart fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
