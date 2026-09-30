#!/usr/bin/env node
// Text bound to a free arrow endpoint, fixtures for excali-editor (ex-713):
// upstream's own packages/element/src/arrowEndpointText.ts and
// dragNewTextElement (packages/element/src/dragElements.ts:227-292) run
// from the pinned checkout under plain Node.
//
//   node tools/goldens/arrow-endpoint-text-fixtures.mjs            write the fixture
//   node tools/goldens/arrow-endpoint-text-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/arrow-endpoint-text-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/arrow-endpoint-text.json:
//
//   { "description", "upstream", "endpointAt", "bindings", "boundText",
//     "dragAnchors", "drags" }
//
// - endpointAt: [{ id, elements, pointer, zoom, result }]:
//   getUnboundArrowEndpointAtPoint(pointer, elements, map, { value: zoom });
//   result is { arrow, startOrEnd } (the arrow's id) or null.
// - bindings: [{ id, arrow, startOrEnd, targetStrokeWidth, result }]:
//   getTextBindingForArrowEndpoint(arrow, startOrEnd, map,
//   targetStrokeWidth); result is { fixedPoint, textAlign, verticalAlign,
//   anchor } or null.
// - boundText: [{ id, elements, text, result }]: isEndpointBoundText(text,
//   map) with `text` the id of one of the elements.
// - dragAnchors: [{ id, text, result }]: getEndpointBoundTextDragAnchor.
// - drags: [{ id, text, anchorX, anchorRatio, pointerX, nextY, zoom,
//   result }]: dragNewTextElement on a scene whose mutateElement records the
//   update it is given (result), text measured at 10 px per UTF-16 code
//   unit (setCustomTextMetricsProvider), as in upstream's tests.
//
// Arrows are built as the upstream test builds them (createArrow,
// packages/excalidraw/tests/arrowEndpointTextBinding.test.tsx:27-47) with
// newArrowElement. Deterministic: fixed inputs, reseed(1); Math.random
// throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "arrow-endpoint-text.json";

const ENTRY = `
export {
  getUnboundArrowEndpointAtPoint,
  getTextBindingForArrowEndpoint,
  isEndpointBoundText,
  getEndpointBoundTextDragAnchor,
} from "./packages/element/src/arrowEndpointText";
export { dragNewTextElement } from "./packages/element/src/dragElements";
export {
  newArrowElement,
  newElement,
  newLinearElement,
  newTextElement,
} from "./packages/element/src/newElement";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { reseed } from "./packages/common/src/random";
export { arrayToMap } from "./packages/common/src/index";
`;

const usage = () => {
  process.stderr.write("usage: arrow-endpoint-text-fixtures.mjs [--check] [--out DIR]\n");
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

const binding = (elementId) => ({ elementId, fixedPoint: [0.5001, 0.5001], mode: "orbit" });

/** createArrow (arrowEndpointTextBinding.test.tsx:27-47), or explicit points */
const arrow = (up, id, [sx, sy], [ex, ey], overrides = {}) => {
  const points = overrides.points ?? [
    [0, 0],
    [ex - sx, ey - sy],
  ];
  const { startBinding, endBinding, points: _p, ...rest } = overrides;
  const el = up.newArrowElement({
    type: "arrow",
    x: sx,
    y: sy,
    width: Math.abs(ex - sx),
    height: Math.abs(ey - sy),
    points,
    ...rest,
  });
  return { ...el, id, startBinding: startBinding ?? null, endBinding: endBinding ?? null };
};

const endpointScenes = (up) => {
  const up100 = () => arrow(up, "a", [100, 300], [100, 100]);
  const short = (o = {}) => arrow(up, "a", [100, 100], [105, 100], o);
  const cases = [];
  const add = (id, elements, pointer, zoom = 1) => cases.push({ id, elements, pointer, zoom });

  add("end-exact", [up100()], [100, 100]);
  add("start-exact", [up100()], [100, 300]);
  add("away", [up100()], [100, 200]);
  add("near-end", [up100()], [108, 105]);
  add("edge-inside", [up100()], [110.9, 100]);
  add("edge-outside", [up100()], [111, 100]);
  add("zoom-2-outside", [up100()], [106, 100], 2);
  add("zoom-2-inside", [up100()], [105.4, 100], 2);
  add("zoom-half-inside", [up100()], [120, 100], 0.5);
  add("zoom-half-outside", [up100()], [122, 100], 0.5);
  add("short-prefers-end", [short()], [102, 100]);
  add("short-end-bound", [short({ endBinding: binding("r") })], [102, 100]);
  add("short-both-bound", [short({ startBinding: binding("r"), endBinding: binding("s") })], [102, 100]);
  add("start-bound-at-start", [up100()].map((a) => ({ ...a, startBinding: binding("r") })), [100, 300]);
  add("end-bound-at-end", [up100()].map((a) => ({ ...a, endBinding: binding("r") })), [100, 100]);
  add("locked", [{ ...up100(), locked: true }], [100, 100]);
  add(
    "top-most-wins",
    [arrow(up, "below", [300, 100], [100, 100]), arrow(up, "above", [100, 300], [100, 100])],
    [100, 100],
  );
  add(
    "top-most-bound-falls-through",
    [
      arrow(up, "below", [300, 100], [100, 100]),
      arrow(up, "above", [100, 300], [100, 100], { endBinding: binding("r") }),
    ],
    [100, 100],
  );
  add(
    "top-most-locked-falls-through",
    [arrow(up, "below", [300, 100], [100, 100]), { ...arrow(up, "above", [100, 300], [100, 100]), locked: true }],
    [100, 100],
  );
  add(
    "zero-length-tail-end",
    [arrow(up, "a", [0, 0], [50, 0], { points: [[0, 0], [50, 0], [50, 0]] })],
    [50, 0],
  );
  add(
    "zero-length-tail-start",
    [arrow(up, "a", [0, 0], [50, 0], { points: [[0, 0], [0, 0], [50, 0]] })],
    [0, 0],
  );
  add(
    "zero-length-tail-start-other-end",
    [arrow(up, "a", [0, 0], [50, 0], { points: [[0, 0], [0, 0], [50, 0]] })],
    [50, 0],
  );
  add("single-point", [arrow(up, "a", [0, 0], [0, 0], { points: [[0, 0]] })], [0, 0]);
  add(
    "multi-point-end",
    [arrow(up, "a", [0, 0], [100, 50], { points: [[0, 0], [60, 0], [60, 80], [100, 50]] })],
    [100, 50],
  );
  add(
    "multi-point-middle",
    [arrow(up, "a", [0, 0], [100, 50], { points: [[0, 0], [60, 0], [60, 80], [100, 50]] })],
    [60, 80],
  );
  add("rotated-end", [{ ...arrow(up, "a", [0, 0], [100, 0]), angle: Math.PI / 2 }], [50, 50]);
  add("rotated-unrotated-point", [{ ...arrow(up, "a", [0, 0], [100, 0]), angle: Math.PI / 2 }], [100, 0]);
  add("elbow-end", [arrow(up, "a", [0, 0], [100, 100], { elbowed: true, points: [[0, 0], [50, 0], [50, 100], [100, 100]] })], [100, 100]);
  add(
    "line-ignored",
    [{ ...up.newLinearElement({ type: "line", x: 0, y: 0, points: [[0, 0], [100, 0]] }), id: "l" }],
    [100, 0],
  );
  add(
    "rectangle-over-endpoint",
    [up100(), { ...up.newElement({ type: "rectangle", x: 80, y: 80, width: 40, height: 40 }), id: "r" }],
    [100, 100],
  );

  return cases.map(({ id, elements, pointer, zoom }) => {
    const map = up.arrayToMap(elements);
    const found = up.getUnboundArrowEndpointAtPoint(pointer, elements, map, { value: zoom });
    return {
      id,
      elements: clone(elements),
      pointer,
      zoom,
      result: found ? { arrow: found.arrow.id, startOrEnd: found.startOrEnd } : null,
    };
  });
};

const bindingCases = (up) => {
  const cases = [];
  const add = (id, a, startOrEnd, targetStrokeWidth = 2) => cases.push({ id, arrow: a, startOrEnd, targetStrokeWidth });

  // arrowEndpointTextBinding.test.tsx:255-322 (placement strategy)
  add("up", arrow(up, "a", [100, 300], [100, 100]), "end");
  add("right", arrow(up, "a", [100, 100], [300, 100]), "end");
  add("down", arrow(up, "a", [100, 100], [100, 300]), "end");
  add("left", arrow(up, "a", [300, 100], [100, 100]), "end");
  add("start-of-right", arrow(up, "a", [100, 100], [300, 100]), "start");
  add("start-of-up", arrow(up, "a", [100, 300], [100, 100]), "start");
  // :337-370 (diagonal arrows keep their tip)
  add("diagonal-45", arrow(up, "a", [100, 100], [300, 300]), "end");
  add("diagonal-steep", arrow(up, "a", [160, 100], [300, 300]), "end");
  add("diagonal-shallow", arrow(up, "a", [100, 160], [300, 300]), "end");
  add("diagonal-45-up-left", arrow(up, "a", [300, 300], [100, 100]), "end");
  add("diagonal-45-down-left", arrow(up, "a", [300, 100], [100, 300]), "end");
  add("diagonal-45-up-right", arrow(up, "a", [100, 300], [300, 100]), "end");
  add("diagonal-start", arrow(up, "a", [100, 160], [300, 300]), "start");
  // :375-397 (stroke width of the text, not the arrow)
  for (const [arrowStroke, textStroke] of [
    [2, 4],
    [4, 1],
    [1, 2],
  ]) {
    add(`stroke-${arrowStroke}-${textStroke}`, arrow(up, "a", [100, 300], [100, 100], { strokeWidth: arrowStroke }), "end", textStroke);
  }
  add("thin-diagonal", arrow(up, "a", [100, 160], [300, 300]), "end", 1);
  add("elbow", arrow(up, "a", [0, 0], [100, 100], { elbowed: true, points: [[0, 0], [50, 0], [50, 100], [100, 100]] }), "end");
  add("multi-point", arrow(up, "a", [0, 0], [100, 50], { points: [[0, 0], [60, 0], [60, 80], [100, 50]] }), "end");
  add("multi-point-start", arrow(up, "a", [0, 0], [100, 50], { points: [[0, 0], [60, 0], [60, 80], [100, 50]] }), "start");
  add("rotated", { ...arrow(up, "a", [0, 0], [100, 0]), angle: Math.PI / 2 }, "end");
  add("rotated-30", { ...arrow(up, "a", [0, 0], [100, 40]), angle: Math.PI / 6 }, "end");
  add("zero-length", arrow(up, "a", [0, 0], [50, 0], { points: [[0, 0], [50, 0], [50, 0]] }), "end");

  return cases.map(({ id, arrow: a, startOrEnd, targetStrokeWidth }) => {
    const map = up.arrayToMap([a]);
    const r = up.getTextBindingForArrowEndpoint(a, startOrEnd, map, targetStrokeWidth);
    return {
      id,
      arrow: clone(a),
      startOrEnd,
      targetStrokeWidth,
      result: r ? { fixedPoint: r.fixedPoint, textAlign: r.textAlign, verticalAlign: r.verticalAlign, anchor: [r.anchor[0], r.anchor[1]] } : null,
    };
  });
};

const text = (up, id, opts = {}) => ({
  ...up.newTextElement({ text: "", x: 0, y: 0, fontSize: 20, fontFamily: 5, ...opts }),
  id,
});

const boundTextCases = (up) => {
  const t = (boundElements) => ({ ...text(up, "t"), boundElements });
  const a = (o) => arrow(up, "a", [0, 0], [100, 0], o);
  const cases = [
    ["no-bound-elements", [t(null)]],
    ["empty-bound-elements", [t([])]],
    ["end-bound", [t([{ id: "a", type: "arrow" }]), a({ endBinding: binding("t") })]],
    ["start-bound", [t([{ id: "a", type: "arrow" }]), a({ startBinding: binding("t") })]],
    ["arrow-bound-elsewhere", [t([{ id: "a", type: "arrow" }]), a({ endBinding: binding("r") })]],
    ["arrow-missing", [t([{ id: "a", type: "arrow" }])]],
    ["listed-as-text", [t([{ id: "a", type: "text" }]), a({ endBinding: binding("t") })]],
    ["listed-arrow-is-line", [t([{ id: "l", type: "arrow" }]), { ...up.newLinearElement({ type: "line", x: 0, y: 0, points: [[0, 0], [10, 0]] }), id: "l" }]],
  ];
  return cases.map(([id, elements]) => ({
    id,
    elements: clone(elements),
    text: "t",
    result: up.isEndpointBoundText(elements[0], up.arrayToMap(elements)),
  }));
};

const dragAnchorCases = (up) =>
  [
    ["left", { text: "label", textAlign: "left", verticalAlign: "middle", x: 306, y: 90 }],
    ["center", { text: "label", textAlign: "center", verticalAlign: "top", x: 290, y: 306 }],
    ["right", { text: "a much longer label", textAlign: "right", verticalAlign: "middle", x: 274, y: 90 }],
    ["right-bottom", { text: "two\nlines", textAlign: "right", verticalAlign: "bottom", x: -12.5, y: 7.25 }],
  ].map(([id, opts]) => {
    const t = text(up, "t", opts);
    return { id, text: clone(t), result: up.getEndpointBoundTextDragAnchor(t) };
  });

const dragCases = (up) => {
  const cases = [];
  const add = (id, t, anchorX, anchorRatio, pointerX, nextY, zoom) =>
    cases.push({ id, t, anchorX, anchorRatio, pointerX, nextY, zoom });
  const left = text(up, "t", { textAlign: "left", verticalAlign: "middle", x: 306, y: 90 });
  const right = text(up, "t", { textAlign: "right", verticalAlign: "middle", x: 274, y: 90 });
  const center = text(up, "t", { textAlign: "center", verticalAlign: "top", x: 290, y: 306 });
  const small = text(up, "t", { fontSize: 8, fontFamily: 1, textAlign: "left", x: 0, y: 0 });
  // arrowEndpointTextBinding.test.tsx:420-468
  add("left-bound", left, 306, 0, 520, null, 1);
  add("right-bound", right, 294, 1, 80, null, 1);
  add("centred", center, 300, 0.5, 430, null, 1);
  add("back-over-arrow", left, 306, 0, 150, null, 1);
  // around the autowrap threshold and the minimum width
  add("left-small", left, 306, 0, 320, null, 1);
  add("left-threshold", left, 306, 0, 342, null, 1);
  add("left-past-threshold", left, 306, 0, 342.5, null, 1);
  add("left-zoom-2", left, 306, 0, 325, null, 2);
  add("left-zoom-half", left, 306, 0, 360, null, 0.5);
  add("right-back", right, 294, 1, 400, null, 1);
  add("centred-left", center, 300, 0.5, 200, null, 1);
  add("centred-small", center, 300, 0.5, 310, null, 1);
  // a free text re-tops itself (nextY)
  add("free-next-y", left, 100, 0, 250, 42.5, 1);
  add("free-min-width", small, 0, 0, 3, 7, 1);
  return cases.map(({ id, t, anchorX, anchorRatio, pointerX, nextY, zoom }) => {
    let result = null;
    const scene = {
      mutateElement: (_element, updates) => {
        result = clone(updates);
      },
    };
    up.dragNewTextElement({
      newElement: t,
      anchorX,
      anchorRatio,
      pointerX,
      nextY: nextY ?? undefined,
      zoom,
      scene,
    });
    return { id, text: clone(t), anchorX, anchorRatio, pointerX, nextY, zoom, result };
  });
};

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating arrow endpoint text fixtures");
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
  const endpointAt = endpointScenes(up);
  const bindings = bindingCases(up);
  const boundText = boundTextCases(up);
  const dragAnchors = dragAnchorCases(up);
  const drags = dragCases(up);
  return format({
    description:
      "getUnboundArrowEndpointAtPoint, getTextBindingForArrowEndpoint, isEndpointBoundText and getEndpointBoundTextDragAnchor (packages/element/src/arrowEndpointText.ts) and dragNewTextElement (packages/element/src/dragElements.ts:227-292), on the arrows of packages/excalidraw/tests/arrowEndpointTextBinding.test.tsx and more (bound, locked, stacked, rotated, elbow, multi-point and zero-length-tail arrows, zooms), text measured at 10 px per UTF-16 code unit. Generated by tools/goldens/arrow-endpoint-text-fixtures.mjs.",
    upstream: commit,
    endpointAt,
    bindings,
    boundText,
    dragAnchors,
    drags,
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`arrow-endpoint-text-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
  });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write(
        "arrow endpoint text fixture is out of date: run node tools/goldens/arrow-endpoint-text-fixtures.mjs\n",
      );
      process.exit(1);
    }
    process.stdout.write(`arrow endpoint text fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
