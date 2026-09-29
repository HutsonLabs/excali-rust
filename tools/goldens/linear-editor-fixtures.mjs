#!/usr/bin/env node
// Linear element editor fixtures for excali-editor (ex-511): upstream's own
// LinearElementEditor (packages/element/src/linearElementEditor.ts), run
// from the pinned checkout under plain Node on hand-written and seeded
// random lines and arrows.
//
//   node tools/goldens/linear-editor-fixtures.mjs            write the fixture
//   node tools/goldens/linear-editor-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/linear-editor-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/linear-editor.json:
//
//   { "description", "pointHandleSize", "draggingThreshold", "upstream",
//     "cases": [ { id, elements, target, ... } ] }
//
// Every case holds the scene (`elements`, as `new Scene(elements)` holds
// them), the line or arrow it edits (`target`) and what upstream answered:
//
// - pointsGlobal: getPointsGlobalCoordinates(element, elementsMap).
// - pointHandles: isPointHandle(element, i) for i = -1 .. points.length.
// - views: for each zoom and isEditing (appState.zoom.value and
//   appState.selectedLinearElement.isEditing), { zoom, editing, midPoints,
//   tooShort, probes, midPointIndex }: getEditorMidPoints; the
//   isSegmentTooShort call getEditorMidPoints makes for each segment
//   (element.points[i], element.points[i + 1], i); probes, each { point,
//   hovered, pointIndex, hit }: getPointIndexUnderCursor(element,
//   elementsMap, zoom, x, y) and getSegmentMidpointHitCoords({ ...editor,
//   segmentMidPointHoveredCoords: hovered }, point, appState, elementsMap);
//   midPointIndex, each { point, result }: getSegmentMidPointIndex(editor,
//   appState, point, elementsMap).
// - shouldAdd: each { segmentMidpoint, origin, pointer, zoom, editing,
//   result }: shouldAddMidpoint with that initialState.
// - ops: in order on a fresh copy of the scene each, { op, ..., result?,
//   after } where after is the whole scene afterwards:
//   - "addMidpoint": { index, pointer, gridSize } addMidpoint(editor with
//     initialState.segmentMidpoint.index = index, pointer, app, snapToGrid
//     = gridSize !== null, scene); result { lastClickedPoint,
//     selectedPointsIndices }.
//   - "deletePoints": { indices, uncommitted } deletePoints(element, app,
//     indices) where app.state.selectedLinearElement is editing and its
//     lastUncommittedPoint is the element's last point (uncommitted) or
//     null.
//   - "addPoints": { points } addPoints(element, scene, points).
//   - "dragLabel": { pointer, offset, result } handleBoundTextDragging({
//     ...editor, pointerOffset: offset }, scene, x, y); result is whether it
//     answered an editor (not null).
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), reseed(1) before each case and each op,
// and random scenes come from a Park-Miller generator seeded per case.
// Text is measured at 10 px per character. Math.random throws while
// generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "linear-editor.json";

const ENTRY = `
export { LinearElementEditor } from "./packages/element/src/linearElementEditor";
export { redrawTextBoundingBox } from "./packages/element/src/textElement";
export { Scene } from "./packages/element/src/Scene";
export { reseed } from "./packages/common/src/random";
export {
  arrayToMap,
  DEFAULT_VERTICAL_ALIGN,
  DRAGGING_THRESHOLD,
  ROUNDNESS,
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
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

const usage = () => {
  process.stderr.write("usage: linear-editor-fixtures.mjs [--check] [--out DIR]\n");
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

/** Rounds to 1/64 so random geometry stays exactly representable. */
const q = (v) => Math.round(v * 64) / 64;

const point = (p) => (p ? [p[0], p[1]] : null);

const ZOOMS = [0.5, 1, 4];

// -- scenes ----------------------------------------------------------------------

/** API.createElement with the "Architect" roughness. */
const el = (up, opts) => apiCreateElement(up, { roughness: 0, ...opts });

const linear = (up, type, points, rest = {}) => {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  return el(up, {
    type,
    points,
    width: Math.max(...xs) - Math.min(...xs),
    height: Math.max(...ys) - Math.min(...ys),
    ...rest,
  });
};

/** A label bound to `container`, laid out as the text editor commits it. */
const bindLabel = (up, elements, container, id, value) => {
  const label = up.newTextElement({
    x: 0,
    y: 0,
    strokeColor: "#1e1e1e",
    backgroundColor: "transparent",
    fillStyle: "solid",
    strokeWidth: 2,
    strokeStyle: "solid",
    roughness: 0,
    opacity: 100,
    fontSize: 20,
    fontFamily: 5,
    text: value,
    originalText: value,
    containerId: container.id,
    textAlign: "center",
    verticalAlign: "middle",
  });
  label.id = id;
  container.boundElements = [...(container.boundElements ?? []), { id, type: "text" }];
  elements.push(label);
  const scene = new up.Scene(elements, { skipValidation: true });
  up.redrawTextBoundingBox(label, container, scene);
  return label;
};

/** Routes the elbow arrows as mutateElement does when their points change. */
const routeElbows = (up, elements) => {
  const scene = new up.Scene(elements, { skipValidation: true });
  for (const e of elements) {
    if (e.type === "arrow" && e.elbowed) scene.mutateElement(e, { points: e.points });
  }
};

const handWritten = (up) => {
  const round = { type: up.ROUNDNESS.PROPORTIONAL_RADIUS };
  return [
    ["line-two", () => [linear(up, "line", [[0, 0], [100, 0]], { id: "l" })]],
    ["line-short-segments", () => [linear(up, "line", [[0, 0], [30, 0], [30, 5], [200, 5], [200, 45]], { id: "l", x: 10, y: -20 })]],
    ["line-round", () => [linear(up, "line", [[0, 0], [80, 90], [160, -10], [240, 40]], { id: "l", roundness: round })]],
    ["line-round-tiny", () => [linear(up, "line", [[0, 0], [20, 15], [45, 0]], { id: "l", roundness: round })]],
    ["line-rotated", () => [linear(up, "line", [[0, 0], [120, 30], [60, 140]], { id: "l", x: 50, y: 50, angle: 0.6 })]],
    [
      "line-polygon",
      () => [linear(up, "line", [[0, 0], [100, 0], [100, 100], [0, 100], [0, 0]], { id: "l", polygon: true, backgroundColor: "#ffc9c9" })],
    ],
    [
      "arrow-labelled",
      () => {
        const a = linear(up, "arrow", [[0, 0], [150, 60], [300, 0]], { id: "a", endArrowhead: "arrow" });
        const elements = [a];
        bindLabel(up, elements, a, "t", "label");
        return elements;
      },
    ],
    [
      "arrow-labelled-two",
      () => {
        const a = linear(up, "arrow", [[0, 0], [260, 90]], { id: "a", x: -30, y: 40, endArrowhead: "arrow" });
        const elements = [a];
        bindLabel(up, elements, a, "t", "a longer arrow label");
        return elements;
      },
    ],
    [
      "arrow-round-labelled",
      () => {
        const a = linear(up, "arrow", [[0, 0], [100, 120], [220, -40], [330, 30]], { id: "a", roundness: round });
        const elements = [a];
        bindLabel(up, elements, a, "t", "curve");
        return elements;
      },
    ],
    [
      "arrow-elbow",
      () => {
        const a = linear(up, "arrow", [[0, 0], [200, 120]], { id: "a", elbowed: true, endArrowhead: "arrow" });
        const elements = [a];
        routeElbows(up, elements);
        return elements;
      },
    ],
  ];
};

const randomScenes = (up) => {
  const scenes = [];
  for (let n = 0; n < 16; n++) {
    const r = rng(511 + n * 7919);
    scenes.push([
      `random-${n}`,
      () => {
        const type = r.pick(["line", "arrow"]);
        const count = r.int(2, 6);
        const points = [[0, 0]];
        for (let i = 1; i < count; i++) {
          const [px, py] = points[i - 1];
          const step = r.chance(0.3) ? r.range(2, 45) : r.range(40, 200);
          const angle = r.range(-Math.PI, Math.PI);
          points.push([q(px + step * Math.cos(angle)), q(py + step * Math.sin(angle))]);
        }
        const rest = {
          id: "l",
          x: q(r.range(-200, 200)),
          y: q(r.range(-200, 200)),
          angle: r.chance(0.4) ? q(r.range(0, 2 * Math.PI)) : 0,
          strokeWidth: r.pick([1, 2, 4]),
        };
        if (r.chance(0.5)) rest.roundness = { type: up.ROUNDNESS.PROPORTIONAL_RADIUS };
        if (type === "arrow") rest.endArrowhead = "arrow";
        const e = linear(up, type, points, rest);
        const elements = [e];
        if (type === "arrow" && r.chance(0.5)) bindLabel(up, elements, e, "t", r.pick(["x", "label", "a longer label"]));
        return elements;
      },
      r,
    ]);
  }
  return scenes;
};

// -- calls -------------------------------------------------------------------------

const withScene = (up, elements) => {
  const copy = clone(elements);
  const scene = new up.Scene(copy, { skipValidation: true });
  return { copy, scene, map: scene.getNonDeletedElementsMap() };
};

const appStateOf = (zoom, editing) => ({
  zoom: { value: zoom },
  selectedLinearElement: editing ? { isEditing: true } : null,
});

const editorOf = (up, element, map, editing) => new up.LinearElementEditor(element, map, editing);

const views = (up, elements, target, r) => {
  const out = [];
  const { map } = withScene(up, elements);
  const element = map.get(target);
  const LEE = up.LinearElementEditor;
  const globals = LEE.getPointsGlobalCoordinates(element, map);
  for (const zoom of ZOOMS) {
    for (const editing of [false, true]) {
      const appState = appStateOf(zoom, editing);
      const midPoints = LEE.getEditorMidPoints(element, map, appState);
      const tooShort = [];
      for (let i = 0; i < element.points.length - 1; i++) {
        tooShort.push(LEE.isSegmentTooShort(element, element.points[i], element.points[i + 1], i, appState.zoom, map));
      }
      const editor = editorOf(up, element, map, editing);
      const candidates = [];
      const near = (p) => {
        candidates.push([p[0], p[1]]);
        for (const d of [10.5, 11.5]) candidates.push([q(p[0] + d / zoom), p[1]]);
        candidates.push([q(p[0] + r.range(-20, 20) / zoom), q(p[1] + r.range(-20, 20) / zoom)]);
      };
      globals.forEach(near);
      midPoints.filter(Boolean).forEach(near);
      const probes = [];
      for (const p of candidates) {
        const hovers = r.chance(0.2) ? [null, [q(p[0] + 4 / zoom), p[1]], [q(p[0] + 30 / zoom), p[1]]] : [null];
        for (const hovered of hovers) {
          probes.push({
            point: p,
            hovered,
            pointIndex: LEE.getPointIndexUnderCursor(element, map, appState.zoom, p[0], p[1]),
            hit: point(
              LEE.getSegmentMidpointHitCoords(
                { ...editor, segmentMidPointHoveredCoords: hovered },
                { x: p[0], y: p[1] },
                appState,
                map,
              ),
            ),
          });
        }
      }
      const midPointIndex = [...midPoints.filter(Boolean), [q(globals[0][0] + 1), globals[0][1]]].map((p) => ({
        point: point(p),
        result: LEE.getSegmentMidPointIndex(editor, appState, p, map),
      }));
      out.push({ zoom, editing, midPoints: midPoints.map(point), tooShort, probes, midPointIndex });
    }
  }
  return {
    pointsGlobal: globals.map(point),
    pointHandles: [...Array(element.points.length + 2).keys()].map((i) => LEE.isPointHandle(element, i - 1)),
    views: out,
  };
};

const shouldAdd = (up, elements, target, r) => {
  const { map } = withScene(up, elements);
  const element = map.get(target);
  const out = [];
  const midPoints = up.LinearElementEditor.getEditorMidPoints(element, map, appStateOf(1, true));
  for (let n = 0; n < 12; n++) {
    const zoom = r.pick(ZOOMS);
    const editing = r.chance(0.5);
    const index = r.chance(0.85) ? r.int(1, Math.max(1, element.points.length - 1)) : null;
    const value = index !== null && r.chance(0.9) ? midPoints[index - 1] ?? [0, 0] : null;
    const origin = r.chance(0.9) ? [q(r.range(-100, 300)), q(r.range(-100, 300))] : null;
    const d = r.pick([0, 5, 9.99, 10, 20]) / zoom;
    const pointer = origin ? [q(origin[0] + d), origin[1]] : [0, 0];
    const segmentMidpoint = { value: point(value), index, added: r.chance(0.15) };
    const editor = {
      ...editorOf(up, element, map, editing),
    };
    editor.initialState = { ...editor.initialState, segmentMidpoint, origin };
    out.push({
      segmentMidpoint,
      origin,
      pointer,
      zoom,
      editing,
      result: up.LinearElementEditor.shouldAddMidpoint(editor, { x: pointer[0], y: pointer[1] }, appStateOf(zoom, editing), map),
    });
  }
  return out;
};

const ops = (up, elements, target, r) => {
  const out = [];
  const LEE = up.LinearElementEditor;
  const base = withScene(up, elements).map.get(target);
  const isElbow = base.type === "arrow" && base.elbowed;
  const run = (op, fn) => {
    up.reseed(1);
    const { scene, map } = withScene(up, elements);
    const element = map.get(target);
    const result = fn(scene, map, element);
    out.push({ op: op.op, ...op, ...(result === undefined ? {} : { result }), after: clone(scene.getElementsIncludingDeleted()) });
  };
  if (!isElbow) {
    const midPoints = LEE.getEditorMidPoints(base, withScene(up, elements).map, appStateOf(1, true));
    midPoints.forEach((m, i) => {
      if (!m) return;
      const index = i + 1;
      for (const gridSize of [null, 20]) {
        const pointer = [q(m[0] + r.range(-15, 15)), q(m[1] + r.range(-15, 15))];
        run({ op: "addMidpoint", index, pointer, gridSize }, (scene, map, element) => {
          const editor = editorOf(up, element, map, true);
          editor.initialState = {
            ...editor.initialState,
            segmentMidpoint: { value: m, index, added: false },
            origin: m,
          };
          const app = { getEffectiveGridSize: () => gridSize, state: {} };
          const ret = LEE.addMidpoint(editor, { x: pointer[0], y: pointer[1] }, app, gridSize !== null, scene);
          return { lastClickedPoint: ret.pointerDownState.lastClickedPoint, selectedPointsIndices: ret.selectedPointsIndices };
        });
      }
    });
    const count = base.points.length;
    const subsets = [[0], [count - 1], [Math.floor(count / 2)]];
    if (count > 2) subsets.push([0, count - 1]);
    for (let i = 0; i < 2; i++) subsets.push([...new Set([r.int(0, count - 1), r.int(0, count - 1)])]);
    for (const indices of subsets) {
      if (indices.length >= count) continue;
      for (const uncommitted of [false, true]) {
        run({ op: "deletePoints", indices, uncommitted }, (scene, map, element) => {
          const app = {
            scene,
            state: {
              selectedLinearElement: {
                isEditing: true,
                lastUncommittedPoint: uncommitted ? element.points[element.points.length - 1] : null,
              },
            },
          };
          LEE.deletePoints(element, app, indices);
        });
      }
    }
    for (let i = 0; i < 3; i++) {
      const points = [...Array(i + 1).keys()].map(() => [q(r.range(-150, 250)), q(r.range(-150, 250))]);
      run({ op: "addPoints", points }, (scene, map, element) => {
        LEE.addPoints(element, scene, points);
      });
    }
  }
  if (base.type === "arrow" && (base.boundElements ?? []).some((b) => b.type === "text")) {
    const globals = LEE.getPointsGlobalCoordinates(base, withScene(up, elements).map);
    for (let i = 0; i < 6; i++) {
      const g = r.pick(globals);
      const pointer = [q(g[0] + r.range(-80, 80)), q(g[1] + r.range(-80, 80))];
      const offset = i % 2 ? { x: q(r.range(-10, 10)), y: q(r.range(-10, 10)) } : { x: 0, y: 0 };
      run({ op: "dragLabel", pointer, offset }, (scene, map, element) => {
        const editor = { ...editorOf(up, element, map, false), pointerOffset: offset };
        return LEE.handleBoundTextDragging(editor, scene, pointer[0], pointer[1]) !== null;
      });
    }
  }
  return out;
};

const buildCases = (up) => {
  const cases = [];
  const all = [...handWritten(up).map(([id, build]) => [id, build, rng(id.length * 104729)]), ...randomScenes(up)];
  for (const [id, build, r] of all) {
    up.reseed(1);
    const elements = clone(new up.Scene(build(), { skipValidation: true }).getElementsIncludingDeleted());
    const target = elements[0].id;
    cases.push({
      id,
      elements,
      target,
      ...views(up, elements, target, r),
      shouldAdd: shouldAdd(up, elements, target, r),
      ops: ops(up, elements, target, r),
    });
  }
  return cases;
};

// -- output -------------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating linear editor fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const buildFixture = (up, commit) => {
  const cases = buildCases(up);
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
  }
  return format({
    description:
      "LinearElementEditor point handles, midpoints (getEditorMidPoints, isSegmentTooShort, " +
      "getSegmentMidpointHitCoords, getSegmentMidPointIndex, shouldAddMidpoint, addMidpoint), " +
      "getPointIndexUnderCursor, isPointHandle, deletePoints, addPoints and handleBoundTextDragging " +
      "(packages/element/src/linearElementEditor.ts) in upstream's test mode (ids id0.., timestamps 1, " +
      "reseed(1) before each case and op, text 10 px per character). Generated by " +
      "tools/goldens/linear-editor-fixtures.mjs.",
    pointHandleSize: up.LinearElementEditor.POINT_HANDLE_SIZE,
    draggingThreshold: up.DRAGGING_THRESHOLD,
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
    process.stderr.write(`linear-editor-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (text) => text.length * 10 });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("linear editor fixture is out of date: run node tools/goldens/linear-editor-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`linear editor fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
