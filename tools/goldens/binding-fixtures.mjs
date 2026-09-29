#!/usr/bin/env node
// Arrow binding fixtures for excali-editor (ex-510): upstream's own
// packages/element/src/binding.ts (and the utils.ts helpers it calls), run
// from the pinned checkout under plain Node on hand-written scenes and on
// seeded random scenes of every bindable element type and arrow kind.
//
//   node tools/goldens/binding-fixtures.mjs            write the fixture
//   node tools/goldens/binding-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/binding-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/binding.json:
//
//   { "description", "upstream", "constants", "normalizeFixedPoint",
//     "cases": [ { id, elements, queries, ops } ] }
//
// - constants: BASE_BINDING_GAP, BASE_ARROW_MIN_LENGTH, FOCUS_POINT_SIZE,
//   getBindingGap for stroke widths and maxBindingDistance_simple for zooms
//   (`zoom` null is the argument left out).
// - normalizeFixedPoint: { input, isFixedPoint, result } with non-finite
//   inputs written as the strings "NaN", "Infinity" and "-Infinity".
// - cases: a scene (`elements`, as `new Scene(elements)` holds them) with
//   - queries, read-only calls on the scene (elementsMap is
//     scene.getNonDeletedElementsMap()), each { fn, ...arguments, result }:
//     - globalFixedPoint { element, fixedPoint }:
//       getGlobalFixedPointForBindableElement
//     - globalFixedPoints { arrow } and arrowLocalFixedPoints { arrow }
//     - updateBoundPoint { arrow, startOrEnd, bindable, dragging }: the
//       arrow's own binding at that end; result a local point or null
//     - snapToOutline { arrow, bindable, startOrEnd, zoom, intersector,
//       midpointSnapping }: bindPointToSnapToElementOutline
//     - avoidCorner { arrow, target, point }: avoidRectangularCorner
//     - fixedPointElbow { arrow, target, startOrEnd, zoom, snapToOutline,
//       midpointSnapping }: calculateFixedPointForElbowArrowBinding
//     - fixedPointSimple { arrow, target, startOrEnd, focusPoint }:
//       calculateFixedPointForNonElbowArrowBinding (focusPoint null = left
//       out)
//     - snapToGrid { outlinePoint, bindable, gridSize, arrow,
//       adjacentPoint }: the module-private snapBoundPointToGrid
//     - allMidpoints { element }: getAllMidpoints
//     - elbowSnapMidPoint { point, element, zoom }: getElbowArrowSnapMidPoint
//       (result null or { point, onAxis })
//     - snapOutlineMidPoint { point, element, zoom, elbowed }
//     - projectOntoDiagonal { arrow, point, element, startOrEnd, zoom,
//       midpointSnapping }: projectFixedPointOntoDiagonal
//     - sideMidPoint { binding }: getBindingSideMidPoint
//     - strategy { arrow, draggingPoints, pointer, appState, opts, complex }:
//       getBindingStrategyForDraggingBindingElementEndpoints with the
//       COMPLEX_BINDINGS feature flag as given; result { start, end }, each
//       { mode, element, focusPoint } where mode "keep" is upstream's
//       `undefined` (keep the binding) and null breaks it; or { error } with
//       the message upstream threw
//   - ops, mutating calls, each on a fresh copy of the scene given in the op
//     (`elements`, the scene before; null for the case's scene), each { op, ...arguments, changes }
//     where changes are the elements whose JSON differs after the call, in
//     full (versionNonce and updated come from upstream's random generator
//     and clock):
//     - updateBoundElements { changed, simultaneouslyUpdated,
//       changedElements }: ids (null = option left out)
//     - bind { arrow, target, mode, startOrEnd, zoom, focusPoint,
//       snapToOutline, midpointSnapping }: bindBindingElement
//     - bindToFixedPoint { arrow, target, startOrEnd, fixedPoint }
//     - unbind { arrow, startOrEnd, result }
//     - bindOrUnbind { arrow, draggingPoints, pointer, appState, opts,
//       complex, result }: bindOrUnbindBindingElement
//     - bindOrUnbindAll { arrows, appState }: bindOrUnbindBindingElements
//     - updateBindings { element, appState, simultaneouslyUpdated, error }
//     - reanchor { element, zoom }: reanchorBindingsToOutline
//     - fixAfterDeletion { deleted }: fixBindingsAfterDeletion(scene
//       elements, the deleted ones), which are already marked deleted in
//       `elements`
//     - fixAfterDuplication { duplicates, idMap, result }:
//       fixDuplicatedBindingsAfterDuplication on the given duplicates with
//       arrayToMap(duplicates) as the duplicate map; result the duplicates
//       after
//
//   - highlights, the binding highlight of each bindable element and frame
//     (interactiveScene.ts renderBindingHighlightForBindableElement_simple,
//     module-private, exported as it is) on a context recording every call:
//     { element, midPoint, zoom, theme, midpointSnapping, gridMode, elbow,
//     pointer, angleLocked, calls }, `elbow` the arrow tool with the elbow
//     arrow type, each call [name, ...arguments] (assignments to lineWidth,
//     strokeStyle and fillStyle as calls of that name)
//
// Text is measured with upstream's test metric (text.length * 10).
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"), reseed(1) before each case, and random scenes come from a
// Park-Miller generator seeded per case. Math.random throws while
// generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "binding.json";

const ENTRY = `
export {
  BASE_BINDING_GAP,
  BASE_ARROW_MIN_LENGTH,
  FOCUS_POINT_SIZE,
  getBindingGap,
  maxBindingDistance_simple,
  normalizeFixedPoint,
  isFixedPoint,
  getGlobalFixedPointForBindableElement,
  getGlobalFixedPoints,
  getArrowLocalFixedPoints,
  updateBoundPoint,
  updateBoundElements,
  bindPointToSnapToElementOutline,
  avoidRectangularCorner,
  calculateFixedPointForElbowArrowBinding,
  calculateFixedPointForNonElbowArrowBinding,
  bindBindingElement,
  bindBindingElementToFixedPoint,
  unbindBindingElement,
  getBindingStrategyForDraggingBindingElementEndpoints,
  bindOrUnbindBindingElement,
  bindOrUnbindBindingElements,
  updateBindings,
  reanchorBindingsToOutline,
  getBindingSideMidPoint,
  fixBindingsAfterDeletion,
  fixDuplicatedBindingsAfterDuplication,
  snapBoundPointToGrid,
} from "./packages/element/src/binding";
export {
  getAllMidpoints,
  getElbowArrowSnapMidPoint,
  getSnapOutlineMidPoint,
  projectFixedPointOntoDiagonal,
} from "./packages/element/src/utils";
export { redrawTextBoundingBox } from "./packages/element/src/textElement";
export { Scene } from "./packages/element/src/Scene";
export { reseed } from "./packages/common/src/random";
export {
  arrayToMap,
  DEFAULT_VERTICAL_ALIGN,
  ROUNDNESS,
  getStrokeWidthByKey,
  getUpdatedTimestamp,
  setFeatureFlag,
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

// snapBoundPointToGrid is module-private: exported as it is.
const EXPOSE = { "packages/element/src/binding": ["snapBoundPointToGrid"] };

const usage = () => {
  process.stderr.write("usage: binding-fixtures.mjs [--check] [--out DIR]\n");
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

const nonFinite = (v) => (Number.isNaN(v) ? "NaN" : v === Infinity ? "Infinity" : v === -Infinity ? "-Infinity" : v);

const point = (p) => (p ? [p[0], p[1]] : null);

// -- scenes ----------------------------------------------------------------------

/** API.createElement with the "Architect" roughness. */
const el = (up, opts) => apiCreateElement(up, { roughness: 0, ...opts });

const bindArrow = (arrow, startOrEnd, target, fixedPoint, mode = "orbit") => {
  arrow[startOrEnd === "start" ? "startBinding" : "endBinding"] = {
    elementId: target.id,
    fixedPoint,
    mode,
  };
  if (!(target.boundElements ?? []).some((b) => b.id === arrow.id)) {
    target.boundElements = [...(target.boundElements ?? []), { id: arrow.id, type: "arrow" }];
  }
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
    if (e.type === "arrow" && e.elbowed && !e.isDeleted) {
      scene.mutateElement(e, { points: e.points });
    }
  }
};

const BINDABLE_TYPES = [
  "rectangle",
  "rectangle",
  "diamond",
  "diamond",
  "ellipse",
  "ellipse",
  "text",
  "image",
  "frame",
  "iframe",
  "embeddable",
  "stickynote",
  "magicframe",
];

const randomBindable = (up, r, id) => {
  const type = r.pick(BINDABLE_TYPES);
  const tiny = r.chance(0.05);
  const width = tiny ? r.pick([0, 0.5, 3]) : q(r.range(20, 240));
  const height = tiny ? r.pick([0, 0.5, 3]) : q(r.range(20, 200));
  const frameLike = type === "frame" || type === "magicframe";
  const e = el(up, {
    type,
    id,
    x: q(r.range(-250, 350)),
    y: q(r.range(-250, 350)),
    width,
    height,
    angle: !frameLike && r.chance(0.3) ? q(r.range(0, 2 * Math.PI)) : 0,
    strokeWidth: r.pick([1, 2, 4]),
    backgroundColor: r.chance(0.5) ? "transparent" : "#ffc9c9",
    roundness: r.chance(0.4),
    ...(type === "text" ? { text: "bindable" } : {}),
  });
  return e;
};

const randomFixedPoint = (r) => {
  if (r.chance(0.15)) return [r.pick([0, 1, -0.05, 1.05]), 0.5001];
  if (r.chance(0.1)) return [0.5, r.pick([0, 1, 0.5])];
  return [q(r.range(-0.2, 1.2)), q(r.range(-0.2, 1.2))];
};

const randomArrow = (up, r, id, bindables) => {
  const elbowed = r.chance(0.25);
  const n = elbowed ? 2 : r.pick([2, 2, 2, 3, 4]);
  const points = [[0, 0]];
  for (let i = 1; i < n; i++) {
    points.push([q(r.range(-300, 300)), q(r.range(-300, 300))]);
  }
  const a = el(up, {
    type: "arrow",
    id,
    x: q(r.range(-200, 300)),
    y: q(r.range(-200, 300)),
    width: 100,
    height: 100,
    points,
    elbowed,
    angle: !elbowed && r.chance(0.15) ? q(r.range(0, 2 * Math.PI)) : 0,
    roundness: elbowed ? false : r.chance(0.5),
    strokeWidth: r.pick([1, 2, 4]),
    startArrowhead: r.pick([null, null, "arrow", "triangle"]),
    endArrowhead: r.pick([null, "arrow", "arrow", "bar"]),
  });
  // the size its points give
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  a.width = Math.max(...xs) - Math.min(...xs);
  a.height = Math.max(...ys) - Math.min(...ys);
  const live = bindables.filter((b) => !b.isDeleted);
  for (const startOrEnd of ["start", "end"]) {
    if (live.length && r.chance(0.75)) {
      const target =
        startOrEnd === "end" && a.startBinding && r.chance(0.2)
          ? live.find((b) => b.id === a.startBinding.elementId)
          : r.pick(live);
      const mode = elbowed ? "orbit" : r.chance(0.3) ? "inside" : "orbit";
      bindArrow(a, startOrEnd, target, randomFixedPoint(r), mode);
    }
  }
  return a;
};

const randomScene = (up, r) => {
  const elements = [];
  const bindables = [];
  const nb = r.int(1, 4);
  for (let i = 0; i < nb; i++) {
    const b = randomBindable(up, r, `b${i}`);
    bindables.push(b);
    elements.push(b);
  }
  const arrows = [];
  const na = r.int(1, 3);
  for (let i = 0; i < na; i++) {
    const a = randomArrow(up, r, `a${i}`, bindables);
    arrows.push(a);
    elements.push(a);
  }
  // a stale entry now and then: an arrow listed that is not bound, an id
  // that is not in the scene
  for (const b of bindables) {
    if (r.chance(0.1)) {
      const a = r.pick(arrows);
      if (!(b.boundElements ?? []).some((x) => x.id === a.id)) {
        b.boundElements = [...(b.boundElements ?? []), { id: a.id, type: "arrow" }];
      }
    }
    if (r.chance(0.05)) {
      b.boundElements = [...(b.boundElements ?? []), { id: "missing", type: "arrow" }];
    }
  }
  // a deleted arrow still listed
  if (r.chance(0.1)) {
    const a = r.pick(arrows);
    a.isDeleted = true;
  }
  routeElbows(up, elements);
  for (const [i, a] of arrows.entries()) {
    if (!a.elbowed && !a.isDeleted && r.chance(0.2)) {
      bindLabel(up, elements, a, `t${i}`, r.pick(["label", "a longer arrow label", "x"]));
    }
  }
  return elements;
};

// -- calls -------------------------------------------------------------------------

/** The elements as `new Scene(elements)` holds them (indices synced). */
const inScene = (up, elements) =>
  clone(new up.Scene(clone(elements), { skipValidation: true }).getElementsIncludingDeleted());

const withScene = (up, elements) => {
  const copy = clone(elements);
  const scene = new up.Scene(copy, { skipValidation: true });
  return { copy, scene, map: scene.getNonDeletedElementsMap() };
};

const diff = (before, scene) => {
  const prior = new Map(before.map((e) => [e.id, JSON.stringify(e)]));
  return scene
    .getElementsIncludingDeleted()
    .filter((e) => prior.get(e.id) !== JSON.stringify(e))
    .map(clone);
};

const strategyJson = (s) => ({
  mode: s.mode === undefined ? "keep" : s.mode,
  element: s.element ? s.element.id : null,
  focusPoint: point(s.focusPoint),
});

const draggingMap = (entries) => new Map(entries.map(([i, p]) => [i, { point: p }]));

const appStateOf = (up, a) => {
  const s = up.getDefaultAppState();
  return {
    ...s,
    zoom: { value: a.zoom },
    isBindingEnabled: a.isBindingEnabled,
    isMidpointSnappingEnabled: a.isMidpointSnappingEnabled,
    gridModeEnabled: a.gridModeEnabled,
    gridSize: a.gridSize,
    bindMode: a.bindMode,
    selectedLinearElement: a.selectedLinearElement
      ? { initialState: { ...a.selectedLinearElement } }
      : null,
  };
};

const optsOf = (o) => {
  const out = {};
  for (const [k, v] of Object.entries(o)) if (v !== null) out[k] = v;
  return out;
};

const withFlag = (up, complex, fn) => {
  up.setFeatureFlag("COMPLEX_BINDINGS", complex);
  try {
    return fn();
  } finally {
    up.setFeatureFlag("COMPLEX_BINDINGS", false);
  }
};

const QUERIES = {
  globalFixedPoint: (up, map, q) =>
    point(up.getGlobalFixedPointForBindableElement(q.fixedPoint, map.get(q.element), map)),
  globalFixedPoints: (up, map, q) => up.getGlobalFixedPoints(map.get(q.arrow), map).map(point),
  arrowLocalFixedPoints: (up, map, q) => up.getArrowLocalFixedPoints(map.get(q.arrow), map).map(point),
  updateBoundPoint: (up, map, q) => {
    const arrow = map.get(q.arrow);
    const prop = q.startOrEnd === "start" ? "startBinding" : "endBinding";
    return point(up.updateBoundPoint(arrow, prop, arrow[prop], map.get(q.bindable), map, q.dragging));
  },
  snapToOutline: (up, map, q) =>
    point(
      up.bindPointToSnapToElementOutline(
        map.get(q.arrow),
        map.get(q.bindable),
        q.startOrEnd,
        map,
        { value: q.zoom },
        q.intersector ?? undefined,
        q.midpointSnapping,
      ),
    ),
  avoidCorner: (up, map, q) =>
    point(up.avoidRectangularCorner(map.get(q.arrow), map.get(q.target), map, q.point)),
  fixedPointElbow: (up, map, q) =>
    up.calculateFixedPointForElbowArrowBinding(
      map.get(q.arrow),
      map.get(q.target),
      q.startOrEnd,
      map,
      { value: q.zoom },
      q.snapToOutline,
      q.midpointSnapping,
    ).fixedPoint,
  fixedPointSimple: (up, map, q) =>
    up.calculateFixedPointForNonElbowArrowBinding(
      map.get(q.arrow),
      map.get(q.target),
      q.startOrEnd,
      map,
      q.focusPoint ?? undefined,
    ).fixedPoint,
  snapToGrid: (up, map, q) =>
    point(
      up.snapBoundPointToGrid(
        q.outlinePoint,
        map.get(q.bindable),
        map,
        q.gridSize,
        map.get(q.arrow),
        q.adjacentPoint ?? undefined,
      ),
    ),
  allMidpoints: (up, map, q) => up.getAllMidpoints(map.get(q.element), map).map(point),
  elbowSnapMidPoint: (up, map, q) => {
    const s = up.getElbowArrowSnapMidPoint(q.point, map.get(q.element), map, { value: q.zoom });
    return s ? { point: point(s.point), onAxis: s.onAxis } : null;
  },
  snapOutlineMidPoint: (up, map, q) =>
    point(up.getSnapOutlineMidPoint(q.point, map.get(q.element), map, { value: q.zoom }, { elbowed: q.elbowed })),
  projectOntoDiagonal: (up, map, q) =>
    point(
      up.projectFixedPointOntoDiagonal(
        map.get(q.arrow),
        q.point,
        map.get(q.element),
        q.startOrEnd,
        map,
        { value: q.zoom },
        q.midpointSnapping,
      ),
    ),
  sideMidPoint: (up, map, q) => point(up.getBindingSideMidPoint(q.binding, map)),
  strategy: (up, map, q, scene) =>
    withFlag(up, q.complex, () => {
      try {
        const { start, end } = up.getBindingStrategyForDraggingBindingElementEndpoints(
          map.get(q.arrow),
          draggingMap(q.draggingPoints),
          q.pointer[0],
          q.pointer[1],
          map,
          scene.getNonDeletedElements(),
          appStateOf(up, q.appState),
          optsOf(q.opts),
        );
        return { start: strategyJson(start), end: strategyJson(end) };
      } catch (error) {
        return { error: error.message };
      }
    }),
};

const runQuery = (up, elements, query) => {
  const { scene, map } = withScene(up, elements);
  const result = QUERIES[query.fn](up, map, query, scene);
  return { ...query, result: clone(result) };
};

const OPS = {
  updateBoundElements: (up, scene, map, o) => {
    const options = {};
    if (o.simultaneouslyUpdated) {
      options.simultaneouslyUpdated = o.simultaneouslyUpdated.map((id) => map.get(id)).filter(Boolean);
    }
    if (o.changedElements) {
      options.changedElements = new Map(o.changedElements.map((id) => [id, map.get(id)]));
    }
    up.updateBoundElements(map.get(o.changed), scene, options);
    return {};
  },
  bind: (up, scene, map, o) => {
    up.bindBindingElement(
      map.get(o.arrow),
      map.get(o.target),
      o.mode,
      o.startOrEnd,
      scene,
      { value: o.zoom },
      o.focusPoint ?? undefined,
      o.snapToOutline,
      o.midpointSnapping,
    );
    return {};
  },
  bindToFixedPoint: (up, scene, map, o) => {
    up.bindBindingElementToFixedPoint(map.get(o.arrow), map.get(o.target), o.startOrEnd, o.fixedPoint, scene);
    return {};
  },
  unbind: (up, scene, map, o) => ({ result: up.unbindBindingElement(map.get(o.arrow), o.startOrEnd, scene) }),
  bindOrUnbind: (up, scene, map, o) =>
    withFlag(up, o.complex, () => {
      try {
        const { start, end } = up.bindOrUnbindBindingElement(
          map.get(o.arrow),
          draggingMap(o.draggingPoints),
          o.pointer[0],
          o.pointer[1],
          scene,
          appStateOf(up, o.appState),
          optsOf(o.opts),
        );
        return { result: { start: strategyJson(start), end: strategyJson(end) } };
      } catch (error) {
        return { result: { error: error.message } };
      }
    }),
  bindOrUnbindAll: (up, scene, map, o) => {
    up.bindOrUnbindBindingElements(
      o.arrows.map((id) => map.get(id)),
      scene,
      appStateOf(up, o.appState),
    );
    return {};
  },
  updateBindings: (up, scene, map, o) => {
    try {
      up.updateBindings(
        map.get(o.element),
        scene,
        appStateOf(up, o.appState),
        o.simultaneouslyUpdated
          ? { simultaneouslyUpdated: o.simultaneouslyUpdated.map((id) => map.get(id)).filter(Boolean) }
          : undefined,
      );
      return { error: null };
    } catch (error) {
      return { error: error.message };
    }
  },
  reanchor: (up, scene, map, o) => {
    up.reanchorBindingsToOutline(map.get(o.element), scene, { value: o.zoom });
    return {};
  },
  fixAfterDeletion: (up, scene, map, o) => {
    const all = scene.getElementsIncludingDeleted();
    up.fixBindingsAfterDeletion(
      all,
      all.filter((e) => o.deleted.includes(e.id)),
    );
    return {};
  },
};

const runOp = (up, op, caseElements) => {
  const elements = op.elements ? inScene(up, op.elements) : null;
  const { copy, scene, map } = withScene(up, elements ?? caseElements);
  const before = clone(copy);
  const extra = OPS[op.op](up, scene, map, op);
  return { ...op, elements, ...clone(extra), changes: diff(before, scene) };
};

const runDuplication = (up, op) => {
  const duplicates = clone(op.duplicates);
  up.fixDuplicatedBindingsAfterDuplication(
    duplicates,
    new Map(Object.entries(op.idMap)),
    up.arrayToMap(duplicates),
  );
  return { ...op, result: clone(duplicates) };
};

// -- random queries ------------------------------------------------------------------

const live = (elements) => elements.filter((e) => !e.isDeleted);
const bindablesOf = (elements) =>
  live(elements).filter((e) => e.type !== "arrow" && !(e.type === "text" && e.containerId));
const arrowsOf = (elements) => live(elements).filter((e) => e.type === "arrow");

const centreOf = (e) => [e.x + e.width / 2, e.y + e.height / 2];

/** A point around `e`: inside, on the outline or a little outside. */
const pointNear = (r, e) => {
  const [cx, cy] = centreOf(e);
  const reach = r.pick([0.2, 0.5, 0.55, 0.7, 1]);
  return [
    q(cx + (r.next() * 2 - 1) * (e.width / 2 + 20) * reach * 1.4),
    q(cy + (r.next() * 2 - 1) * (e.height / 2 + 20) * reach * 1.4),
  ];
};

const randomAppState = (r) => {
  const gridModeEnabled = r.chance(0.2);
  return {
    zoom: r.pick([1, 1, 1, 0.5, 2, 0.25]),
    isBindingEnabled: r.chance(0.9),
    isMidpointSnappingEnabled: r.chance(0.7),
    gridModeEnabled,
    gridSize: 20,
    bindMode: r.pick(["orbit", "orbit", "inside", "skip"]),
    selectedLinearElement: r.chance(0.5)
      ? {
          origin: r.chance(0.6) ? [q(r.range(-200, 300)), q(r.range(-200, 300))] : null,
          arrowStartIsInside: r.chance(0.3),
          altFocusPoint: r.chance(0.3) ? [q(r.range(-200, 300)), q(r.range(-200, 300))] : null,
          arrowOtherEndpointInitialBinding: r.chance(0.3) ? { mode: r.pick(["inside", "orbit"]) } : null,
        }
      : null,
  };
};

const randomOpts = (r, appState) => ({
  newArrow: r.chance(0.3) ? true : null,
  angleLocked: r.chance(0.2) ? true : null,
  altKey: r.chance(0.15) ? true : null,
  finalize: r.chance(0.5) ? true : null,
  initialBinding: r.chance(0.1) ? true : null,
  gridSize: appState.gridModeEnabled ? appState.gridSize : null,
  shiftKey: r.chance(0.2) ? true : null,
});

/** Where a dragged point goes, local to the arrow. */
const draggedTo = (r, arrow, bindables) => {
  const target = bindables.length && r.chance(0.8) ? r.pick(bindables) : null;
  const p = target ? pointNear(r, target) : [q(r.range(-300, 400)), q(r.range(-300, 400))];
  return [p[0] - arrow.x, p[1] - arrow.y];
};

const randomDragging = (r, arrow, bindables) => {
  const last = arrow.points.length - 1;
  const which = r.pick(["start", "end", "end", "start", "both", "none", "middle"]);
  const entries = [];
  if (which === "start" || which === "both") entries.push([0, draggedTo(r, arrow, bindables)]);
  if (which === "end" || which === "both") entries.push([last, draggedTo(r, arrow, bindables)]);
  if (which === "middle" && last > 1) entries.push([1, draggedTo(r, arrow, bindables)]);
  return entries;
};

const randomQueries = (up, r, elements) => {
  const bindables = bindablesOf(elements);
  const arrows = arrowsOf(elements);
  const map = new up.Scene(clone(elements), { skipValidation: true }).getNonDeletedElementsMap();
  const queries = [];
  const soe = () => r.pick(["start", "end"]);
  const zoom = () => r.pick([1, 1, 0.5, 2, 0.25]);
  for (const b of bindables) {
    queries.push({ fn: "globalFixedPoint", element: b.id, fixedPoint: randomFixedPoint(r) });
    queries.push({ fn: "allMidpoints", element: b.id });
    for (let i = 0; i < 3; i++) {
      queries.push({ fn: "elbowSnapMidPoint", point: pointNear(r, b), element: b.id, zoom: zoom() });
      queries.push({
        fn: "snapOutlineMidPoint",
        point: pointNear(r, b),
        element: b.id,
        zoom: zoom(),
        elbowed: r.chance(0.5),
      });
    }
    queries.push({
      fn: "sideMidPoint",
      binding: { elementId: b.id, fixedPoint: randomFixedPoint(r), mode: "orbit" },
    });
  }
  queries.push({ fn: "sideMidPoint", binding: { elementId: "missing", fixedPoint: [0.5, 0.5], mode: "orbit" } });
  for (const a of arrows) {
    queries.push({ fn: "globalFixedPoints", arrow: a.id });
    queries.push({ fn: "arrowLocalFixedPoints", arrow: a.id });
    for (const startOrEnd of ["start", "end"]) {
      const binding = a[startOrEnd === "start" ? "startBinding" : "endBinding"];
      const bound = binding && map.get(binding.elementId);
      const others = bound ? [bound, ...bindables] : bindables;
      if (others.length) {
        const target = r.chance(0.7) && bound ? bound : r.pick(others);
        queries.push({
          fn: "updateBoundPoint",
          arrow: a.id,
          startOrEnd,
          bindable: target.id,
          dragging: r.chance(0.3),
        });
      }
    }
    if (a.points.length < 2) continue;
    for (const target of bindables) {
      const startOrEnd = soe();
      queries.push({
        fn: "snapToOutline",
        arrow: a.id,
        bindable: target.id,
        startOrEnd,
        zoom: zoom(),
        intersector: r.chance(0.2) ? [pointNear(r, target), pointNear(r, target)] : null,
        midpointSnapping: r.chance(0.6),
      });
      queries.push({ fn: "avoidCorner", arrow: a.id, target: target.id, point: pointNear(r, target) });
      queries.push({
        fn: "fixedPointElbow",
        arrow: a.id,
        target: target.id,
        startOrEnd: soe(),
        zoom: zoom(),
        snapToOutline: r.chance(0.8),
        midpointSnapping: r.chance(0.6),
      });
      queries.push({
        fn: "fixedPointSimple",
        arrow: a.id,
        target: target.id,
        startOrEnd: soe(),
        focusPoint: r.chance(0.5) ? pointNear(r, target) : null,
      });
      queries.push({
        fn: "snapToGrid",
        outlinePoint: pointNear(r, target),
        bindable: target.id,
        gridSize: r.pick([null, 10, 20]),
        arrow: a.id,
        adjacentPoint: r.chance(0.5) ? pointNear(r, target) : null,
      });
      queries.push({
        fn: "projectOntoDiagonal",
        arrow: a.id,
        point: pointNear(r, target),
        element: target.id,
        startOrEnd: soe(),
        zoom: zoom(),
        midpointSnapping: r.chance(0.5),
      });
    }
    for (let i = 0; i < 4; i++) {
      const appState = randomAppState(r);
      const draggingPoints = randomDragging(r, a, bindables);
      const pointer = draggingPoints.length
        ? [draggingPoints[0][1][0] + a.x, draggingPoints[0][1][1] + a.y]
        : [q(r.range(-200, 300)), q(r.range(-200, 300))];
      queries.push({
        fn: "strategy",
        arrow: a.id,
        draggingPoints,
        pointer,
        appState,
        opts: randomOpts(r, appState),
        complex: r.chance(0.4),
      });
    }
  }
  return queries.map((query) => runQuery(up, elements, query));
};

/** The scene with `id` moved, resized or rotated, as a transform leaves it. */
const transformed = (r, elements, id) => {
  const copy = clone(elements);
  const e = copy.find((x) => x.id === id);
  const kind = r.pick(["move", "resize", "rotate", "none"]);
  if (kind === "move") {
    e.x = q(e.x + r.range(-150, 150));
    e.y = q(e.y + r.range(-150, 150));
  } else if (kind === "resize") {
    e.width = q(Math.max(1, e.width * r.range(0.3, 2)));
    e.height = q(Math.max(1, e.height * r.range(0.3, 2)));
  } else if (kind === "rotate" && e.type !== "frame" && e.type !== "magicframe") {
    e.angle = q(r.range(0, 2 * Math.PI));
  }
  if (kind !== "none") e.version += 1;
  return copy;
};

const randomOps = (up, r, elements) => {
  const bindables = bindablesOf(elements);
  const arrows = arrowsOf(elements);
  const ops = [];
  for (const b of bindables) {
    const before = transformed(r, elements, b.id);
    const others = live(before).filter((e) => e.id !== b.id);
    ops.push({
      op: "updateBoundElements",
      elements: before,
      changed: b.id,
      simultaneouslyUpdated: r.chance(0.3) && others.length ? [r.pick(others).id, b.id] : null,
      changedElements: r.chance(0.3) ? [b.id] : null,
    });
    ops.push({ op: "reanchor", elements: transformed(r, elements, b.id), element: b.id, zoom: r.pick([1, 0.5]) });
    ops.push({
      op: "updateBindings",
      elements: transformed(r, elements, b.id),
      element: b.id,
      appState: randomAppState(r),
      simultaneouslyUpdated: null,
    });
  }
  for (const a of arrows) {
    for (const target of bindables) {
      ops.push({
        op: "bind",
        elements: null,
        arrow: a.id,
        target: target.id,
        mode: r.pick(["inside", "orbit", "skip"]),
        startOrEnd: r.pick(["start", "end"]),
        zoom: r.pick([1, 0.5]),
        focusPoint: r.chance(0.5) ? pointNear(r, target) : null,
        snapToOutline: r.chance(0.8),
        midpointSnapping: r.chance(0.6),
      });
    }
    if (bindables.length) {
      ops.push({
        op: "bindToFixedPoint",
        elements: null,
        arrow: a.id,
        target: r.pick(bindables).id,
        startOrEnd: r.pick(["start", "end"]),
        fixedPoint: randomFixedPoint(r),
      });
    }
    for (const startOrEnd of ["start", "end"]) {
      ops.push({ op: "unbind", elements: null, arrow: a.id, startOrEnd });
    }
    for (let i = 0; i < 3; i++) {
      const appState = randomAppState(r);
      const draggingPoints = randomDragging(r, a, bindables);
      // the points already where the drag put them, as the linear editor
      // moves them before binding
      const before = clone(elements);
      const moved = before.find((e) => e.id === a.id);
      for (const [idx, p] of draggingPoints) moved.points[idx] = p;
      const pointer = draggingPoints.length
        ? [draggingPoints[0][1][0] + a.x, draggingPoints[0][1][1] + a.y]
        : [q(r.range(-200, 300)), q(r.range(-200, 300))];
      ops.push({
        op: "bindOrUnbind",
        elements: before,
        arrow: a.id,
        draggingPoints,
        pointer,
        appState,
        opts: randomOpts(r, appState),
        complex: r.chance(0.4),
      });
    }
    ops.push({
      op: "updateBindings",
      elements: transformed(r, elements, a.id),
      element: a.id,
      appState: randomAppState(r),
      simultaneouslyUpdated: null,
    });
  }
  if (arrows.length) {
    ops.push({ op: "bindOrUnbindAll", elements: null, arrows: arrows.map((a) => a.id), appState: randomAppState(r) });
  }
  // deletion: some elements marked deleted, then the bindings fixed
  const candidates = live(elements).filter((e) => !(e.type === "text" && e.containerId));
  if (candidates.length) {
    const deleted = candidates.filter(() => r.chance(0.4));
    if (!deleted.length) deleted.push(r.pick(candidates));
    const before = clone(elements);
    for (const e of before) if (deleted.some((d) => d.id === e.id)) e.isDeleted = true;
    ops.push({ op: "fixAfterDeletion", elements: before, deleted: deleted.map((e) => e.id) });
  }
  const results = ops.map((op) => runOp(up, op, elements));
  // duplication: a subset duplicated with new ids
  const chosen = live(elements).filter(() => r.chance(0.6));
  if (chosen.length) {
    const idMap = {};
    for (const e of chosen) idMap[e.id] = `${e.id}-copy`;
    const duplicates = clone(chosen).map((e) => ({ ...e, id: idMap[e.id] }));
    results.push(runDuplication(up, { op: "fixAfterDuplication", duplicates, idMap }));
  }
  return results;
};

// -- hand-written scenes --------------------------------------------------------------

/**
 * Two 100 x 100 rectangles with an arrow between them, as history.test.tsx
 * sets them up ("conflicts in arrows and their bindable elements":
 * rect1 at (-100, -50), rect2 at (100, -50)); the arrow as the arrow tool
 * leaves it (sharp corners off: round, an end arrowhead).
 */
const historyScenes = (up) => {
  const cases = [];
  const rects = (x1, y1, x2, y2) => [
    el(up, { type: "rectangle", id: "rect1", x: x1, y: y1, width: 100, height: 100, roughness: 1 }),
    el(up, { type: "rectangle", id: "rect2", x: x2, y: y2, width: 100, height: 100, roughness: 1 }),
  ];
  const arrowOf = (x, y, points) =>
    el(up, {
      type: "arrow",
      id: "arrow",
      x,
      y,
      width: 100,
      height: 0,
      points,
      roughness: 1,
      endArrowhead: "arrow",
    });
  // history.test.tsx:4578 (the arrow bound by dragging its ends onto the
  // rectangles, fixed points [1, 0.5001] and [0, 0.5001], both bindings
  // undone), after a remote update moved rect1, rect2 and the arrow to
  // x = rect2.x + 50: the two redos rebind the start, then the end, each
  // laying out the arrow bound to the rebound rectangle
  {
    const [r1, r2] = rects(150, -50, 150, -50);
    const a = arrowOf(150, 0, [
      [0, 0],
      [100, 0],
    ]);
    cases.push({
      id: "history-rebind-both",
      elements: [r1, r2, a],
      steps: [
        {
          bindings: [{ arrow: "arrow", startOrEnd: "start", elementId: "rect1", fixedPoint: [1, 0.5001] }],
          changed: ["rect1", "arrow"],
          redraw: ["rect1"],
        },
        {
          bindings: [{ arrow: "arrow", startOrEnd: "end", elementId: "rect2", fixedPoint: [0, 0.5001] }],
          changed: ["rect2", "arrow"],
          redraw: ["rect2"],
        },
      ],
    });
  }
  // history.test.tsx:5107: the arrow drawn from rect1 to rect2 (47 long,
  // from x = 0) comes back through the history after rect2 moved to
  // (500, -500): one redo restores the arrow and both rectangles' records
  {
    const [r1, r2] = rects(-100, -50, 500, -500);
    const a = arrowOf(0, 0, [
      [0, 0],
      [47, 0],
    ]);
    a.width = 47;
    bindArrow(a, "start", r1, [1, 0.5001]);
    bindArrow(a, "end", r2, [0, 0.5001]);
    cases.push({
      id: "history-remote-move",
      elements: [r1, r2, a],
      steps: [{ bindings: [], changed: ["rect1", "rect2", "arrow"], redraw: ["rect1", "rect2"] }],
    });
  }
  // ElementsDelta.redrawBoundArrows after each step's delta is applied:
  // updateBoundElements for each changed bindable element, with the
  // changed elements passed along. A step's bindings are assigned as the
  // delta assigns them (the arrow's binding, the target's record).
  return cases.map(({ id, elements: built, steps }) => {
    const elements = inScene(up, built);
    const copy = clone(elements);
    const scene = new up.Scene(copy, { skipValidation: true });
    const before = clone(copy);
    const byId = new Map(copy.map((e) => [e.id, e]));
    for (const step of steps) {
      for (const b of step.bindings) {
        bindArrow(byId.get(b.arrow), b.startOrEnd, byId.get(b.elementId), b.fixedPoint);
      }
      const changed = new Map(step.changed.map((cid) => [cid, byId.get(cid)]));
      const map = scene.getNonDeletedElementsMap();
      for (const rid of step.redraw) up.updateBoundElements(map.get(rid), scene, { changedElements: changed });
    }
    return {
      id,
      elements: clone(elements),
      queries: [],
      ops: [{ op: "redrawBoundArrows", elements: null, steps, changes: diff(before, scene) }],
    };
  });
};

/** Hand-picked scenes for the paths random scenes reach rarely. */
const handScenes = (up) => {
  const scenes = [];
  const add = (id, build) => scenes.push({ id, build });

  // a horizontal arrow between two rectangles, both ends orbiting
  add("orbit-both", () => {
    const r1 = el(up, { type: "rectangle", id: "r1", x: 0, y: 0, width: 100, height: 100 });
    const r2 = el(up, { type: "rectangle", id: "r2", x: 300, y: 20, width: 80, height: 60 });
    const a = el(up, {
      type: "arrow",
      id: "a",
      x: 105,
      y: 50,
      points: [
        [0, 0],
        [190, 0],
      ],
      endArrowhead: "arrow",
    });
    bindArrow(a, "start", r1, [1.05, 0.5001]);
    bindArrow(a, "end", r2, [-0.0625, 0.5001]);
    return [r1, r2, a];
  });
  // both ends inside the same ellipse; a line of three points through a
  // diamond; an arrow inside a rectangle with no arrowheads
  add("inside-and-same-element", () => {
    const e = el(up, { type: "ellipse", id: "e", x: 0, y: 0, width: 200, height: 120, backgroundColor: "#a5d8ff" });
    const d = el(up, { type: "diamond", id: "d", x: 300, y: 0, width: 120, height: 120, roundness: true });
    const a = el(up, {
      type: "arrow",
      id: "a",
      x: 40,
      y: 40,
      points: [
        [0, 0],
        [120, 30],
      ],
    });
    bindArrow(a, "start", e, [0.2, 0.33], "inside");
    bindArrow(a, "end", e, [0.8, 0.58], "inside");
    const b = el(up, {
      type: "arrow",
      id: "b",
      x: 200,
      y: 60,
      points: [
        [0, 0],
        [60, -40],
        [160, 0],
      ],
      endArrowhead: "triangle",
      startArrowhead: "arrow",
    });
    bindArrow(b, "start", e, [1.02, 0.5001]);
    bindArrow(b, "end", d, [0.5001, 0.5001], "inside");
    return [e, d, a, b];
  });
  // tiny and zero-size targets; a rotated rectangle with rounded corners
  add("tiny-and-rotated", () => {
    const t = el(up, { type: "rectangle", id: "tiny", x: 0, y: 0, width: 0.5, height: 0 });
    const r = el(up, {
      type: "rectangle",
      id: "rot",
      x: 200,
      y: 100,
      width: 120,
      height: 80,
      angle: 0.7,
      roundness: true,
      strokeWidth: 4,
    });
    const a = el(up, {
      type: "arrow",
      id: "a",
      x: 10,
      y: 0,
      points: [
        [0, 0],
        [180, 120],
      ],
      endArrowhead: "arrow",
      roundness: true,
    });
    bindArrow(a, "start", t, [0.5001, 0.5001]);
    bindArrow(a, "end", r, [-0.05, 0.4]);
    return [t, r, a];
  });
  // an elbow arrow bound on both ends, and one bound on one end
  add("elbow", () => {
    const r = el(up, { type: "rectangle", id: "r", x: 0, y: 0, width: 100, height: 100 });
    const d = el(up, { type: "diamond", id: "d", x: 250, y: 150, width: 120, height: 100 });
    const a = el(up, {
      type: "arrow",
      id: "a",
      x: 105,
      y: 50,
      width: 200,
      height: 100,
      elbowed: true,
      roundness: false,
      endArrowhead: "arrow",
      points: [
        [0, 0],
        [200, 100],
      ],
    });
    bindArrow(a, "start", r, [1.05, 0.5001]);
    bindArrow(a, "end", d, [0.5001, -0.05]);
    const b = el(up, {
      type: "arrow",
      id: "b",
      x: 50,
      y: 105,
      width: 100,
      height: 150,
      elbowed: true,
      roundness: false,
      points: [
        [0, 0],
        [100, 150],
      ],
    });
    bindArrow(b, "start", r, [0.5001, 1.05]);
    const elements = [r, d, a, b];
    routeElbows(up, elements);
    return elements;
  });
  // shapes in a frame (the highlight clips to it), an arrow from one to a
  // magic frame
  add("framed", () => {
    const f = el(up, { type: "frame", id: "f", x: 0, y: 0, width: 400, height: 300 });
    const r = el(up, { type: "rectangle", id: "r", x: 40, y: 40, width: 120, height: 80, frameId: "f", roundness: true });
    const e = el(up, { type: "ellipse", id: "e", x: 220, y: 150, width: 140, height: 90, frameId: "f", angle: 0.3 });
    const d = el(up, { type: "diamond", id: "d", x: 60, y: 180, width: 100, height: 100, frameId: "f", strokeWidth: 4 });
    const m = el(up, { type: "magicframe", id: "m", x: 500, y: 0, width: 200, height: 200 });
    const a = el(up, {
      type: "arrow",
      id: "a",
      x: 165,
      y: 80,
      points: [
        [0, 0],
        [330, 20],
      ],
      endArrowhead: "arrow",
    });
    bindArrow(a, "start", r, [1.04, 0.5001]);
    bindArrow(a, "end", m, [-0.025, 0.5]);
    return [f, r, e, d, m, a];
  });
  // an arrow with a label, bound on both ends
  add("labelled", () => {
    const r1 = el(up, { type: "rectangle", id: "r1", x: 0, y: 0, width: 100, height: 100 });
    const r2 = el(up, { type: "ellipse", id: "r2", x: 300, y: 200, width: 100, height: 80 });
    const a = el(up, {
      type: "arrow",
      id: "a",
      x: 105,
      y: 50,
      points: [
        [0, 0],
        [195, 190],
      ],
      endArrowhead: "arrow",
    });
    bindArrow(a, "start", r1, [1.05, 0.5001]);
    bindArrow(a, "end", r2, [0.2, 0.1]);
    const elements = [r1, r2, a];
    bindLabel(up, elements, a, "t", "a label that wraps on a short arrow");
    return elements;
  });
  return scenes;
};

// -- the fixture ---------------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating binding fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const constants = (up) => ({
  BASE_BINDING_GAP: up.BASE_BINDING_GAP,
  BASE_ARROW_MIN_LENGTH: up.BASE_ARROW_MIN_LENGTH,
  FOCUS_POINT_SIZE: up.FOCUS_POINT_SIZE,
  gaps: [0, 0.5, 1, 2, 4, 8, 13].map((strokeWidth) => ({
    strokeWidth,
    gap: up.getBindingGap({ strokeWidth }),
  })),
  distances: [null, 0.1, 0.25, 0.3, 0.5, 0.6667, 0.75, 0.9, 1, 1.5, 2, 10, 30].map((zoom) => ({
    zoom,
    distance: up.maxBindingDistance_simple(zoom === null ? undefined : { value: zoom }),
  })),
});

const normalizeCases = (up) => {
  const inputs = [
    [0.5, 0.5],
    [0.5, 0.2],
    [0.2, 0.5],
    [0.50005, 0.49995],
    [0.4999, 0.5],
    [0.5001, 0.5001],
    [0, 1],
    [-0.05, 0.5001],
    [20, -12],
    [10, -10],
    [10.5, 0.5],
    [NaN, 0],
    [0, Infinity],
    [-Infinity, 0.5],
    [1e-9, 0.99999],
  ];
  return inputs.map((input) => ({
    input: input.map(nonFinite),
    isFixedPoint: up.isFixedPoint(input),
    result: up.normalizeFixedPoint(input),
  }));
};

// -- the binding highlight -------------------------------------------------------------

// renderBindingHighlightForBindableElement_simple is module-private in
// interactiveScene.ts: exported as it is. The module pulls in the editor's
// jotai store and React-facing modules the function never calls.
const HIGHLIGHT_ENTRY = `export { renderBindingHighlightForBindableElement_simple } from "./packages/excalidraw/renderer/interactiveScene";`;
const HIGHLIGHT_EXPOSE = {
  "packages/excalidraw/renderer/interactiveScene": ["renderBindingHighlightForBindableElement_simple"],
};
const HIGHLIGHT_STUBS = [
  "react",
  "jotai",
  "jotai-scope",
  "packages/excalidraw/data/blob",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/subset/subset-main",
];
const HIGHLIGHT_SHIMS = {
  "packages/excalidraw/editor-jotai": "module.exports = { atom: (init) => ({ init }) };",
};

/** A 2D context that records every call and assignment as it is made. */
class CallContext {
  constructor() {
    this.calls = [];
  }
  record(name, args) {
    this.calls.push([name, ...args]);
  }
  set lineWidth(v) {
    this.record("lineWidth", [v]);
  }
  set strokeStyle(v) {
    this.record("strokeStyle", [v]);
  }
  set fillStyle(v) {
    this.record("fillStyle", [v]);
  }
}
for (const name of [
  "save",
  "restore",
  "translate",
  "rotate",
  "beginPath",
  "closePath",
  "moveTo",
  "lineTo",
  "bezierCurveTo",
  "ellipse",
  "arc",
  "roundRect",
  "rect",
  "clip",
  "stroke",
  "fill",
  "strokeRect",
]) {
  CallContext.prototype[name] = function call(...args) {
    this.record(name, args);
  };
}

/**
 * Highlights of each bindable element (and frame) of the scene: `{ element,
 * midPoint, zoom, theme, midpointSnapping, gridMode, elbow, pointer,
 * angleLocked, calls }`, the calls
 * renderBindingHighlightForBindableElement_simple makes on the context.
 * `elbow` is the arrow tool with the elbow arrow type active.
 */
const highlightCases = (up, hl, r, elements) => {
  const map = new up.Scene(clone(elements), { skipValidation: true }).getNonDeletedElementsMap();
  const out = [];
  for (const target of bindablesOf(elements)) {
    const element = map.get(target.id);
    const midpoints = up.getAllMidpoints(element, map);
    for (let i = 0; i < 3; i++) {
      const variant = {
        element: element.id,
        midPoint: r.chance(0.4) ? point(r.pick(midpoints)) : null,
        zoom: r.pick([1, 1, 0.5, 2, 0.2]),
        theme: r.pick(["light", "light", "dark"]),
        midpointSnapping: r.chance(0.8),
        gridMode: r.chance(0.1),
        elbow: r.chance(0.3),
        pointer: r.chance(0.15)
          ? null
          : r.chance(0.5)
          ? (() => {
              const m = r.pick(midpoints);
              return [q(m[0] + r.range(-25, 25)), q(m[1] + r.range(-25, 25))];
            })()
          : pointNear(r, element),
        angleLocked: r.chance(0.1),
      };
      const context = new CallContext();
      const appState = {
        zoom: { value: variant.zoom },
        theme: variant.theme,
        isMidpointSnappingEnabled: variant.midpointSnapping,
        gridModeEnabled: variant.gridMode,
        selectedLinearElement: null,
        activeTool: { type: variant.elbow ? "arrow" : "selection" },
        currentItemArrowType: variant.elbow ? "elbow" : "round",
      };
      hl.renderBindingHighlightForBindableElement_simple(
        context,
        { element, ...(variant.midPoint ? { midPoint: variant.midPoint } : {}) },
        map,
        appState,
        variant.pointer,
        variant.angleLocked,
      );
      out.push({ ...variant, calls: clone(context.calls) });
    }
  }
  return out;
};

const buildFixture = (up, hl, commit) => {
  const cases = [];
  for (const { id, build } of handScenes(up)) {
    up.reseed(1);
    const elements = inScene(up, build());
    const r = rng(7919 * (cases.length + 1));
    cases.push({
      id,
      elements,
      queries: randomQueries(up, r, elements),
      ops: randomOps(up, r, elements),
      highlights: highlightCases(up, hl, r, elements),
    });
  }
  for (let i = 0; i < 60; i++) {
    const id = `random-${String(i).padStart(3, "0")}`;
    up.reseed(1);
    const r = rng(1000003 + i * 7907);
    let elements;
    try {
      elements = inScene(up, randomScene(up, r));
    } catch (error) {
      throw new Error(`${id}: ${error.message}`);
    }
    cases.push({
      id,
      elements,
      queries: randomQueries(up, r, elements),
      ops: randomOps(up, r, elements),
      highlights: highlightCases(up, hl, r, elements),
    });
  }
  for (const c of historyScenes(up)) {
    cases.push({ ...c, highlights: [] });
  }
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case ${c.id}`);
    ids.add(c.id);
  }
  return format({
    description:
      "binding.ts (getBindingGap, maxBindingDistance_simple, normalizeFixedPoint, fixed points, updateBoundPoint, updateBoundElements, bindPointToSnapToElementOutline, avoidRectangularCorner, snapBoundPointToGrid, calculateFixedPointFor*ArrowBinding, bindBindingElement, unbindBindingElement, the binding strategies for dragged arrow ends, bindOrUnbindBindingElement(s), updateBindings, reanchorBindingsToOutline, getBindingSideMidPoint, fixBindingsAfterDeletion, fixDuplicatedBindingsAfterDuplication) and utils.ts (getAllMidpoints, getElbowArrowSnapMidPoint, getSnapOutlineMidPoint, projectFixedPointOntoDiagonal) and the binding highlight (interactiveScene.ts renderBindingHighlightForBindableElement_simple) on hand-written and seeded random scenes, in upstream's test mode (reseed(1) before each case, text measured as text.length * 10). Generated by tools/goldens/binding-fixtures.mjs.",
    upstream: commit,
    constants: constants(up),
    normalizeFixedPoint: normalizeCases(up),
    cases,
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`binding-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  // setFeatureFlag stores the flags; Node's own localStorage needs a file
  const storage = new Map();
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (key) => storage.get(key) ?? null,
      setItem: (key, value) => storage.set(key, String(value)),
      removeItem: (key) => storage.delete(key),
    },
  });
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
    expose: EXPOSE,
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (value) => value.length * 10 });
  const hl = await loadUpstream(upstream, {
    entry: HIGHLIGHT_ENTRY,
    expose: HIGHLIGHT_EXPOSE,
    stubs: HIGHLIGHT_STUBS,
    shims: HIGHLIGHT_SHIMS,
    define: {
      "import.meta.env.MODE": '"test"',
      "import.meta.env.PKG_NAME": "undefined",
      "import.meta.env.PKG_VERSION": "undefined",
    },
  });
  const text = deterministic(() => buildFixture(up, hl, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("binding fixture is out of date: run node tools/goldens/binding-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`binding fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
