#!/usr/bin/env node
// Element type conversion fixtures for excali-editor (ex-535): upstream's
// own packages/excalidraw/components/ConvertElementTypePopup.tsx
// (convertElementTypes, getConversionTypeFromElements and the
// module-private convertLineToElbow and conversion caches) run from the
// pinned checkout under plain Node.
//
//   node tools/goldens/convert-element-type-fixtures.mjs            write the fixture
//   node tools/goldens/convert-element-type-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/convert-element-type-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/convert-element-type.json:
//
//   { "description", "upstream", "conversionType", "lineToElbow", "sessions" }
//
// - conversionType: [{ id, elements, result }]:
//   getConversionTypeFromElements(elements), "generic", "linear" or null.
// - lineToElbow: [{ id, points, result }]: convertLineToElbow({ points }).
// - sessions: one open popup per case on the case's scene (`elements`, as
//   the Scene constructor left them) with `selected` selected and
//   `appState` { currentItemStartArrowhead, currentItemEndArrowhead, zoom,
//   activeTool }; each step first runs the Panel's effects
//   (ConvertElementTypePopup.tsx:229-255: the linear element and font size
//   caches primed from the selection), then
//   convertElementTypes(app, { conversionType, nextType, direction }) with
//   the step's `conversionType` (getConversionTypeFromElements of the
//   selection, as App.tsx:5655-5662 passes it, unless the step names one),
//   and records { nextType, direction, conversionType, result, elements,
//   selectedElementIds, selectedLinearElement (the editor's element id or
//   null), activeTool }; `elements` is the whole scene after the step.
//
// Scenes are built as upstream's tests build them (API.createElement,
// tests/helpers/api.ts), including the cases of
// tests/convertElementType.test.tsx and
// tests/convertElementType.binding.test.tsx. Text is measured at 10 px per
// UTF-16 code unit (setCustomTextMetricsProvider), as in upstream's tests.
// Deterministic: upstream runs in its test mode (timestamps 1), reseed(1)
// before each case; Math.random throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "convert-element-type.json";

const POPUP = "packages/excalidraw/components/ConvertElementTypePopup";

const ENTRY = `
export {
  convertElementTypes,
  getConversionTypeFromElements,
  convertLineToElbow,
  FONT_SIZE_CONVERSION_CACHE,
  LINEAR_ELEMENT_CONVERSION_CACHE,
  toCacheKey,
  getConvertibleType,
  filterGenericConvetibleElements,
  filterLinearConvertibleElements,
} from "./${POPUP}";
export { Scene } from "./packages/element/src/Scene";
export { getBoundTextElement, redrawTextBoundingBox } from "./packages/element/src/textElement";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
export {
  ROUNDNESS,
  DEFAULT_VERTICAL_ALIGN,
  getStrokeWidthByKey,
  getUpdatedTimestamp,
  updateActiveTool,
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
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

// module-private: exported as they are
const EXPOSE = {
  [POPUP]: [
    "convertLineToElbow",
    "FONT_SIZE_CONVERSION_CACHE",
    "LINEAR_ELEMENT_CONVERSION_CACHE",
    "toCacheKey",
    "getConvertibleType",
    "filterGenericConvetibleElements",
    "filterLinearConvertibleElements",
  ],
};

// the popup's React side is never rendered
const STUBS = [
  "packages/excalidraw/components/IconButton",
  "packages/excalidraw/components/icons",
];
const SHIMS = {
  "packages/excalidraw/editor-jotai": "module.exports = { atom: (init) => ({ init }) };",
  "packages/excalidraw/analytics": "module.exports = { trackEvent: () => {} };",
};

const usage = () => {
  process.stderr.write("usage: convert-element-type-fixtures.mjs [--check] [--out DIR]\n");
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

const el = (up, opts) => apiCreateElement(up, { roughness: 0, ...opts });

// -- getConversionTypeFromElements --------------------------------------------

const conversionTypeCases = (up) => {
  const out = [];
  const add = (id, elements) =>
    out.push({ id, elements: elements.map(clone), result: up.getConversionTypeFromElements(elements) });
  const rect = () => el(up, { type: "rectangle", id: "r" });
  const line = () => el(up, { type: "line", id: "l" });
  const arrow = (o = {}) => el(up, { type: "arrow", id: "a", ...o });
  const bound = { elementId: "r", fixedPoint: [0.5, 0.5], mode: "orbit" };
  add("empty", []);
  add("rectangle", [rect()]);
  add("diamond", [el(up, { type: "diamond", id: "d" })]);
  add("ellipse", [el(up, { type: "ellipse", id: "e" })]);
  add("line", [line()]);
  add("arrow", [arrow()]);
  add("elbow-arrow", [arrow({ elbowed: true })]);
  add("arrow-start-bound", [arrow({ startBinding: bound })]);
  add("arrow-end-bound", [arrow({ endBinding: bound })]);
  add("arrow-bound-text", [{ ...arrow(), boundElements: [{ id: "t", type: "text" }] }]);
  add("arrow-bound-arrow-only", [{ ...arrow(), boundElements: [{ id: "x", type: "arrow" }] }]);
  add("line-bound-text", [{ ...line(), boundElements: [{ id: "t", type: "text" }] }]);
  add("generic-wins", [line(), rect()]);
  add("generic-wins-bound-arrow", [arrow({ endBinding: bound }), rect()]);
  add("linear-and-text", [el(up, { type: "text", id: "t" }), line()]);
  add("text", [el(up, { type: "text", id: "t" })]);
  add("freedraw", [el(up, { type: "freedraw", id: "f", points: [[0, 0], [10, 10]] })]);
  add("frame", [el(up, { type: "frame", id: "fr" })]);
  add("image", [el(up, { type: "image", id: "i" })]);
  add("stickynote", [el(up, { type: "stickynote", id: "s" })]);
  add("iframe", [el(up, { type: "iframe", id: "if" })]);
  add("embeddable", [el(up, { type: "embeddable", id: "em" })]);
  add("bound-arrow-only", [arrow({ endBinding: bound })]);
  return out;
};

// -- convertLineToElbow ---------------------------------------------------------

const lineToElbowCases = (up) => {
  const out = [];
  const add = (id, points) => {
    const input = clone(points);
    out.push({ id, points: input, result: clone(up.convertLineToElbow({ points: clone(points) })) });
  };
  add("two-points", [[0, 0], [100, 100]]);
  add("horizontal", [[0, 0], [100, 0]]);
  add("vertical", [[0, 0], [0, 100]]);
  add("near-horizontal", [[0, 0], [100, 19]]);
  add("near-vertical", [[0, 0], [19, 100]]);
  add("threshold-x", [[0, 0], [20, 100]]);
  add("threshold-y", [[0, 0], [100, 20]]);
  add("duplicates", [[0, 0], [0, 0], [50, 50], [50, 50], [100, 0]]);
  add("zigzag", [[0, 0], [100, 50], [200, 0], [300, 50]]);
  add("colinear", [[0, 0], [50, 0], [100, 0], [100, 100]]);
  add("jog-short-second", [[0, 0], [100, 0], [100, 10], [200, 10]]);
  add("jog-short-first", [[0, 0], [10, 0], [10, 100], [10, 200]]);
  add("staircase", [[0, 0], [30, 30], [60, 60], [90, 90], [120, 120]]);
  add("single", [[0, 0]]);
  add("backwards", [[0, 0], [-100, -60], [-40, -200]]);
  const r = rng(424242);
  for (let i = 0; i < 60; i++) {
    const n = r.int(2, 7);
    const points = [[0, 0]];
    for (let k = 1; k < n; k++) {
      const [px, py] = points[k - 1];
      const step = r.pick([5, 15, 25, 80, 150]);
      points.push([q(px + r.range(-step, step)), q(py + r.range(-step, step))]);
    }
    add(`random-${i}`, points);
  }
  return out;
};

// -- convertElementTypes --------------------------------------------------------

/** The app convertElementTypes reads and sets state on. */
const appOf = (up, scene, state) => {
  const app = {
    scene,
    state,
    setState: (update) => {
      const patch = typeof update === "function" ? update(app.state) : update;
      app.state = { ...app.state, ...patch };
    },
  };
  return app;
};

/**
 * The Panel's effects (ConvertElementTypePopup.tsx:229-255): every
 * convertible selected element's current form into the linear cache, every
 * generic container's label font size into the font size cache, first
 * sighting only.
 */
const primeCaches = (up, app) => {
  const selected = app.scene.getSelectedElements(app.state);
  const conversionType = up.getConversionTypeFromElements(selected);
  const generic = conversionType === "generic" ? up.filterGenericConvetibleElements(selected) : [];
  const linear = conversionType === "linear" ? up.filterLinearConvertibleElements(selected) : [];
  for (const element of linear) {
    const key = up.toCacheKey(element.id, up.getConvertibleType(element));
    if (!up.LINEAR_ELEMENT_CONVERSION_CACHE.has(key)) {
      up.LINEAR_ELEMENT_CONVERSION_CACHE.set(key, element);
    }
  }
  for (const element of generic) {
    if (!up.FONT_SIZE_CONVERSION_CACHE.has(element.id)) {
      const boundText = up.getBoundTextElement(element, app.scene.getNonDeletedElementsMap());
      if (boundText) {
        up.FONT_SIZE_CONVERSION_CACHE.set(element.id, { fontSize: boundText.fontSize });
      }
    }
  }
};

const APP_STATE = {
  currentItemStartArrowhead: null,
  currentItemEndArrowhead: "arrow",
  zoom: 1,
  activeTool: "selection",
};

const runSession = (up, id, build, selected, a, script) => {
  up.reseed(1);
  const elements = build();
  const scene = new up.Scene(elements, { skipValidation: true });
  // the popup mounts empty (the caches are cleared when it unmounts)
  up.FONT_SIZE_CONVERSION_CACHE.clear();
  up.LINEAR_ELEMENT_CONVERSION_CACHE.clear();
  const initial = scene.getElementsIncludingDeleted().map(clone);
  const appState = { ...APP_STATE, ...a };
  const defaults = up.getDefaultAppState();
  const state = {
    ...defaults,
    currentItemStartArrowhead: appState.currentItemStartArrowhead,
    currentItemEndArrowhead: appState.currentItemEndArrowhead,
    zoom: { value: appState.zoom },
    activeTool: up.updateActiveTool(defaults, { type: appState.activeTool }),
    selectedElementIds: Object.fromEntries(selected.map((s) => [s, true])),
  };
  const app = appOf(up, scene, state);
  const steps = [];
  up.reseed(1);
  for (const step of script) {
    primeCaches(up, app);
    const conversionType =
      step.conversionType !== undefined
        ? step.conversionType
        : up.getConversionTypeFromElements(app.scene.getSelectedElements(app.state));
    const result = up.convertElementTypes(app, {
      conversionType,
      nextType: step.nextType,
      direction: step.direction,
    });
    steps.push({
      nextType: step.nextType ?? null,
      direction: step.direction ?? null,
      conversionType,
      result,
      elements: app.scene.getElementsIncludingDeleted().map(clone),
      selectedElementIds: clone(app.state.selectedElementIds),
      selectedLinearElement: app.state.selectedLinearElement?.elementId ?? null,
      activeTool: clone(app.state.activeTool),
    });
  }
  return { id, elements: initial, selected, appState, steps };
};

const right = { direction: "right" };
const left = { direction: "left" };
const to = (nextType) => ({ nextType });

/** A container with a label, laid out as the text editor leaves it. */
const labelled = (up, container, text, extra = {}) => {
  const t = el(up, {
    type: "text",
    id: `${container.id}-label`,
    text,
    fontSize: extra.fontSize ?? 20,
    containerId: container.id,
    textAlign: "center",
    verticalAlign: "middle",
  });
  t.originalText = text;
  t.autoResize = true;
  const c = { ...container, boundElements: [...(container.boundElements ?? []), { id: t.id, type: "text" }] };
  const scene = new up.Scene([c, t], { skipValidation: true });
  up.redrawTextBoundingBox(t, c, scene);
  return scene.getElementsIncludingDeleted();
};

const genericSessions = (up) => {
  const out = [];
  const rect = (o = {}) => el(up, { type: "rectangle", id: "r", x: 10, y: 20, width: 200, height: 120, ...o });

  // tests/convertElementType.test.tsx: the roundness type follows the shape
  out.push(
    runSession(up, "roundness", () => [rect({ roundness: true })], ["r"], {}, [to("diamond"), to("rectangle"), to("ellipse")]),
  );
  out.push(runSession(up, "cycle-right", () => [rect({ roundness: false })], ["r"], {}, [right, right, right, right]));
  out.push(runSession(up, "cycle-left", () => [rect({ roundness: true })], ["r"], {}, [left, left, left, left]));
  out.push(
    runSession(up, "same-type-noop", () => [rect()], ["r"], {}, [to("rectangle"), right]),
  );
  out.push(
    runSession(
      up,
      "mixed-right",
      () => [rect(), el(up, { type: "ellipse", id: "e", x: 300, y: 0, width: 80, height: 60 })],
      ["r", "e"],
      {},
      [right, right, left],
    ),
  );
  out.push(
    runSession(
      up,
      "mixed-left",
      () => [rect(), el(up, { type: "diamond", id: "d", x: 300, y: 0, width: 80, height: 60 })],
      ["r", "d"],
      {},
      [left, left],
    ),
  );
  out.push(
    runSession(
      up,
      "with-line",
      () => [rect(), el(up, { type: "line", id: "l", x: 300, y: 0, points: [[0, 0], [60, 40]] })],
      ["r", "l"],
      { activeTool: "rectangle" },
      [right, right],
    ),
  );
  out.push(
    runSession(up, "rotated", () => [rect({ angle: 0.7, roundness: true })], ["r"], {}, [right, right, right]),
  );
  out.push(
    runSession(
      up,
      "unselected-untouched",
      () => [rect(), el(up, { type: "ellipse", id: "e", x: 300 })],
      ["r"],
      {},
      [right],
    ),
  );

  // tests/convertElementType.binding.test.tsx: bound arrows stay attached
  const bindingCases = [
    ["rectangle", "diamond", [1, 0.25]],
    ["rectangle", "ellipse", [1, 0.1]],
    ["ellipse", "rectangle", [0.9, 0.2]],
    ["diamond", "rectangle", [0.75, 0.25]],
  ];
  for (const elbowed of [false, true]) {
    for (const [fromType, toType, fixedPoint] of bindingCases) {
      out.push(
        runSession(
          up,
          `binding-${elbowed ? "elbow" : "simple"}-${fromType}-${toType}`,
          () => {
            const shape = el(up, { type: fromType, id: "s", x: 0, y: 0, width: 200, height: 200 });
            const arrow = el(up, {
              type: "arrow",
              id: "a",
              elbowed,
              x: 400,
              y: 50,
              width: 200,
              height: 0,
              points: [
                [0, 0],
                [-200, 0],
              ],
              endBinding: { elementId: "s", fixedPoint, mode: "orbit" },
              startBinding: null,
            });
            return [{ ...shape, boundElements: [{ id: "a", type: "arrow" }] }, arrow];
          },
          ["s"],
          {},
          [to(toType), right, right],
        ),
      );
    }
  }
  // both ends on the shape, and an inside binding
  out.push(
    runSession(
      up,
      "binding-both-ends",
      () => {
        const shape = el(up, { type: "rectangle", id: "s", x: 0, y: 0, width: 160, height: 100 });
        const a = el(up, {
          type: "arrow",
          id: "a",
          x: 160,
          y: 20,
          width: 100,
          height: 60,
          points: [[0, 0], [100, 0], [100, 60], [0, 60]],
          startBinding: { elementId: "s", fixedPoint: [1, 0.2], mode: "orbit" },
          endBinding: { elementId: "s", fixedPoint: [1, 0.8], mode: "orbit" },
        });
        const b = el(up, {
          type: "arrow",
          id: "b",
          x: 300,
          y: 50,
          width: 220,
          height: 0,
          points: [[0, 0], [-220, 0]],
          endBinding: { elementId: "s", fixedPoint: [0.5, 0.5], mode: "inside" },
        });
        return [{ ...shape, boundElements: [{ id: "a", type: "arrow" }, { id: "b", type: "arrow" }] }, a, b];
      },
      ["s"],
      {},
      [right, right, right],
    ),
  );

  // labels: shrunk to fit a smaller shape, their first size restored
  for (const [name, text, w, h] of [
    ["label-short", "hello", 200, 120],
    ["label-long", "the quick brown fox jumps over the lazy dog", 200, 100],
    ["label-tight", "abcdefghijklmnop", 170, 60],
  ]) {
    out.push(
      runSession(
        up,
        name,
        () => labelled(up, rect({ width: w, height: h }), text),
        ["r"],
        {},
        [right, right, right, right, left],
      ),
    );
  }
  out.push(
    runSession(
      up,
      "label-two-shapes",
      () => [
        ...labelled(up, rect(), "first label"),
        ...labelled(up, el(up, { type: "ellipse", id: "e", x: 300, y: 0, width: 120, height: 120 }), "second one here"),
      ],
      ["r", "e"],
      {},
      [right, right, right],
    ),
  );
  return out;
};

const linearSessions = (up) => {
  const out = [];
  const line = (o = {}) =>
    el(up, {
      type: "line",
      id: "l",
      x: 10,
      y: 20,
      width: 200,
      height: 100,
      points: [
        [0, 0],
        [200, 100],
      ],
      ...o,
    });
  const arrow = (o = {}) =>
    el(up, {
      type: "arrow",
      id: "a",
      x: 10,
      y: 20,
      width: 200,
      height: 100,
      points: [
        [0, 0],
        [200, 100],
      ],
      endArrowhead: "arrow",
      ...o,
    });

  out.push(runSession(up, "line-cycle-right", () => [line()], ["l"], {}, [right, right, right, right, right]));
  out.push(
    runSession(
      up,
      "line-cycle-left",
      () => [line()],
      ["l"],
      { currentItemStartArrowhead: "circle_outline", currentItemEndArrowhead: "triangle" },
      [left, left, left, left, left],
    ),
  );
  out.push(
    runSession(
      up,
      "multi-point-line",
      () => [
        line({
          width: 300,
          height: 150,
          points: [
            [0, 0],
            [90, 10],
            [110, 140],
            [300, 150],
          ],
        }),
      ],
      ["l"],
      {},
      [to("elbowArrow"), to("sharpArrow"), to("curvedArrow"), to("line")],
    ),
  );
  out.push(
    runSession(
      up,
      "curved-elbow-line",
      () => [arrow({ roundness: true, points: [[0, 0], [80, 60], [200, 100]] })],
      ["a"],
      {},
      [to("elbowArrow"), to("line"), to("curvedArrow"), to("elbowArrow"), to("sharpArrow")],
    ),
  );
  out.push(
    runSession(
      up,
      "elbow-to-simple",
      () => [
        arrow({
          elbowed: true,
          roundness: false,
          width: 200,
          height: 100,
          points: [
            [0, 0],
            [100, 0],
            [100, 100],
            [200, 100],
          ],
        }),
      ],
      ["a"],
      { currentItemEndArrowhead: null },
      [right, right, right, right],
    ),
  );
  out.push(
    runSession(
      up,
      "two-lines",
      () => [line(), line({ id: "m", x: 300, points: [[0, 0], [50, 120]], width: 50, height: 120 })],
      ["l", "m"],
      { activeTool: "line" },
      [right, right, right, right],
    ),
  );
  out.push(
    runSession(
      up,
      "mixed-subtypes",
      () => [arrow({ roundness: false }), arrow({ id: "b", roundness: true, x: 300 })],
      ["a", "b"],
      {},
      [right, left],
    ),
  );
  out.push(
    runSession(
      up,
      "bound-arrow-skipped",
      () => {
        const shape = el(up, { type: "rectangle", id: "s", x: 400, y: 0, width: 100, height: 100 });
        const bound = arrow({
          id: "b",
          x: 300,
          y: 50,
          width: 100,
          height: 0,
          points: [[0, 0], [100, 0]],
          endBinding: { elementId: "s", fixedPoint: [0, 0.5], mode: "orbit" },
        });
        return [{ ...shape, boundElements: [{ id: "b", type: "arrow" }] }, bound, line()];
      },
      ["b", "l"],
      {},
      [right, right],
    ),
  );
  out.push(
    runSession(up, "polygon-line", () => [line({ polygon: true, points: [[0, 0], [200, 0], [100, 100], [0, 0]], width: 200, height: 100 })], ["l"], {}, [right, right, right, right]),
  );
  out.push(
    runSession(up, "short-line-elbow", () => [line({ points: [[0, 0], [5, 3]], width: 5, height: 3 })], ["l"], {}, [to("elbowArrow"), to("line")]),
  );
  // nothing convertible: convertElementTypes returns false
  out.push(
    runSession(up, "none", () => [el(up, { type: "text", id: "t" })], ["t"], {}, [right, { conversionType: null, nextType: "diamond" }]),
  );

  const r = rng(271828);
  for (let i = 0; i < 16; i++) {
    const n = r.int(2, 6);
    const points = [[0, 0]];
    for (let k = 1; k < n; k++) {
      const [px, py] = points[k - 1];
      points.push([q(px + r.range(-160, 160)), q(py + r.range(-160, 160))]);
    }
    const kind = r.pick(["line", "sharp", "curved", "elbow"]);
    const base = { x: q(r.range(-300, 300)), y: q(r.range(-300, 300)), points, strokeWidth: r.pick([1, 2, 4]) };
    const script = [];
    for (let s = r.int(3, 6); s > 0; s--) {
      script.push(r.chance(0.3) ? to(r.pick(["line", "sharpArrow", "curvedArrow", "elbowArrow"])) : r.pick([right, left]));
    }
    const build = () => {
      const xs = points.map((p) => p[0]);
      const ys = points.map((p) => p[1]);
      const size = { width: Math.max(...xs) - Math.min(...xs), height: Math.max(...ys) - Math.min(...ys) };
      if (kind === "line") return [line({ ...base, ...size })];
      if (kind === "elbow") {
        // an elbow arrow's points are orthogonal
        const ortho = up.convertLineToElbow({ points: clone(points) });
        const oxs = ortho.map((p) => p[0]);
        const oys = ortho.map((p) => p[1]);
        return [
          arrow({
            ...base,
            points: ortho,
            elbowed: true,
            roundness: false,
            width: Math.max(...oxs) - Math.min(...oxs),
            height: Math.max(...oys) - Math.min(...oys),
          }),
        ];
      }
      return [arrow({ ...base, ...size, roundness: kind === "curved" })];
    };
    const target = kind === "line" ? "l" : "a";
    out.push(
      runSession(
        up,
        `random-${i}`,
        build,
        [target],
        { currentItemStartArrowhead: r.pick([null, "bar"]), currentItemEndArrowhead: r.pick([null, "arrow", "circle"]) },
        script,
      ),
    );
  }
  return out;
};

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating convert element type fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const buildFixture = (up, commit) => {
  up.reseed(1);
  up.setCustomTextMetricsProvider({ getLineWidth: (t) => t.length * 10 });
  const conversionType = conversionTypeCases(up);
  const lineToElbow = lineToElbowCases(up);
  const sessions = [...genericSessions(up), ...linearSessions(up)];
  return format({
    description:
      "convertElementTypes, getConversionTypeFromElements and convertLineToElbow (packages/excalidraw/components/ConvertElementTypePopup.tsx), with the Panel's cache priming before every conversion, on the scenes of tests/convertElementType.test.tsx and tests/convertElementType.binding.test.tsx and more (Tab cycles both ways, mixed selections, labels, bindings, line and arrow sub-types, seeded random polylines), text measured at 10 px per UTF-16 code unit. Generated by tools/goldens/convert-element-type-fixtures.mjs.",
    upstream: commit,
    conversionType,
    lineToElbow,
    sessions,
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`convert-element-type-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    expose: EXPOSE,
    stubs: STUBS,
    shims: SHIMS,
    jsx: "automatic",
    define: { "import.meta.env.MODE": '"test"' },
  });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write(
        "convert element type fixture is out of date: run node tools/goldens/convert-element-type-fixtures.mjs\n",
      );
      process.exit(1);
    }
    process.stdout.write(`convert element type fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
