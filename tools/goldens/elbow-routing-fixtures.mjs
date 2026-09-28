#!/usr/bin/env node
// Elbow arrow routing fixtures for excali-editor (ex-211): upstream's own
// updateElbowArrowPoints (packages/element/src/elbowArrow.ts:907-1167), run
// from the pinned checkout under plain Node on tables of scenes.
//
//   node tools/goldens/elbow-routing-fixtures.mjs            write the fixture
//   node tools/goldens/elbow-routing-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/elbow-routing-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/elbow-routing.json:
//
//   { "description", "upstream", "cases": [ { id, arrow, elements, updates,
//     result | error } ] }
//
// - arrow: the elbow arrow passed as the first argument.
// - elements: the scene, arrow included; the second argument is
//   arrayToMap(elements).
// - updates: the third argument. A binding in it that equals the arrow's own
//   is passed as the arrow's own object (the no-op short circuit at
//   elbowArrow.ts:1084-1096 compares bindings by reference).
// - result: JSON.parse(JSON.stringify(update)), the returned ElementUpdate
//   (keys holding undefined are dropped, as JSON.stringify drops them).
// - error: in place of result where upstream throws, the exception's message.
//
// No `options` are passed: the drag-time path (isDragging, hovered-element
// lookup and outline snapping) belongs to binding.ts and collision.ts.
//
// Cases (all built with upstream's constructors after reseed(1)):
// - upstream-*: the scenes of packages/element/tests/elbowArrow.test.tsx
//   ("elbow arrow routing"), the bound one with the fixed points upstream's
//   bindBindingElement computes (calculateFixedPointForElbowArrowBinding).
// - unbound-*: seeded random endpoints, including collinear and coincident
//   ones, arrows with more than two points and the 1e6 clamp.
// - bound-*: seeded random scenes of one or two bindable elements of every
//   bindable type (sharp and rounded rectangles and diamonds, ellipses,
//   text, image, frame, sticky note, iframe, embeddable), rotated and not,
//   overlapping and apart, with random fixed points (edges, inside, outside,
//   the 0.5 rule), arrowheads and stroke widths, bound at one or both ends.
// - fixed-*: fixed segments: a segment move (elbowArrow.ts:465-704), an
//   endpoint drag with fixed segments (706-900), a release (282-460), a
//   renormalisation (113-280) and a resize (1145-1147), chained from routed
//   arrows.
// - throw-*: scenes where upstream throws; `error` holds its message in
//   place of `result` (see throwCases).
// - edge-*: fewer than two points, a binding to a missing or non-bindable
//   element, an empty scene, the no-op short circuit.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), reseed(1) before each case, and scenes
// come from a Park-Miller generator seeded per case. Math.random throws
// while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "elbow-routing.json";

const ENTRY = `
export { updateElbowArrowPoints } from "./packages/element/src/elbowArrow";
export { calculateFixedPointForElbowArrowBinding } from "./packages/element/src/binding";
export { reseed } from "./packages/common/src/random";
export { arrayToMap, DEFAULT_ZOOM, ROUNDNESS } from "./packages/common/src/index";
export {
  newElement,
  newEmbeddableElement,
  newIframeElement,
  newStickyNoteElement,
  newFrameElement,
  newTextElement,
  newArrowElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { newElementWith } from "./packages/element/src/mutateElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { getStrokeWidthByKey, getUpdatedTimestamp } from "./packages/common/src/index";
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

const usage = () => {
  process.stderr.write("usage: elbow-routing-fixtures.mjs [--check] [--out DIR]\n");
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
  };
};

// -- builders ----------------------------------------------------------------

const BINDABLE = [
  "rectangle",
  "rectangle-round",
  "rectangle-legacy",
  "diamond",
  "diamond-round",
  "ellipse",
  "text",
  "image",
  "frame",
  "stickynote",
  "stickynote-round",
  "iframe",
  "embeddable",
];

/** A bindable element of `kind` built by upstream's constructor. */
const bindable = (up, kind, id, geometry) => {
  const opts = { id, ...geometry };
  switch (kind) {
    case "rectangle":
      return up.newElement({ type: "rectangle", ...opts });
    case "rectangle-round":
      return up.newElement({
        type: "rectangle",
        roundness: { type: up.ROUNDNESS.ADAPTIVE_RADIUS },
        ...opts,
      });
    case "rectangle-legacy":
      return up.newElement({
        type: "rectangle",
        roundness: { type: up.ROUNDNESS.PROPORTIONAL_RADIUS },
        ...opts,
      });
    case "diamond":
      return up.newElement({ type: "diamond", ...opts });
    case "diamond-round":
      return up.newElement({
        type: "diamond",
        roundness: { type: up.ROUNDNESS.PROPORTIONAL_RADIUS },
        ...opts,
      });
    case "ellipse":
      return up.newElement({ type: "ellipse", ...opts });
    case "text":
      return { ...up.newTextElement({ text: "label", ...opts }), ...geometry };
    case "image":
      return up.newImageElement({ type: "image", ...opts });
    case "frame":
      return up.newFrameElement(opts);
    case "stickynote":
      return up.newStickyNoteElement({ type: "stickynote", ...opts });
    case "stickynote-round":
      return up.newStickyNoteElement({
        type: "stickynote",
        roundness: { type: up.ROUNDNESS.PROPORTIONAL_RADIUS },
        ...opts,
      });
    case "iframe":
      return up.newIframeElement({ type: "iframe", ...opts });
    case "embeddable":
      return up.newEmbeddableElement({ type: "embeddable", ...opts });
  }
  throw new Error(`unknown bindable kind ${kind}`);
};

const elbow = (up, id, x, y, points, rest = {}) =>
  up.newArrowElement({ type: "arrow", elbowed: true, id, x, y, points, ...rest });

const binding = (elementId, fixedPoint) => ({ elementId, fixedPoint, mode: "orbit" });

const route = (up, arrow, elements, updates) =>
  up.updateElbowArrowPoints(arrow, up.arrayToMap(elements), updates);

/** The arrow with an update applied, as mutateElement applies it. */
const apply = (arrow, update) => {
  const next = { ...arrow };
  for (const [key, value] of Object.entries(update)) {
    if (value !== undefined) next[key] = value;
  }
  return next;
};

// -- cases -------------------------------------------------------------------

// The scenes of elbowArrow.test.tsx, built with API.createElement
// (apiCreateElement, tests/helpers/api.ts:166-436).
const upstreamCases = () => [
  // "can properly generate orthogonal arrow points" (elbowArrow.test.tsx:143-163):
  // the arrow is inserted into a fresh Scene but mutated through
  // h.app.scene, whose elements map is empty.
  {
    id: "upstream-orthogonal-points",
    build: (up) => {
      const arrow = apiCreateElement(up, { type: "arrow", elbowed: true });
      return {
        arrow,
        elements: [],
        updates: {
          points: [
            [-45 - arrow.x, -100.1 - arrow.y],
            [45 - arrow.x, 99.9 - arrow.y],
          ],
        },
      };
    },
  },
  // The scene of "can generate proper points for bound elbow arrow"
  // (elbowArrow.test.tsx:165-214) with getDefaultAppState() in place of the
  // mounted App's h.state (so the rectangles are sharp and the fixed points
  // differ from the test's; its expected points are the App's). The fixed
  // points are what bindBindingElement stores,
  // calculateFixedPointForElbowArrowBinding's (binding.ts:1141-1182); a
  // binding update alone does not re-route (mutateElement.ts:56-61).
  {
    id: "upstream-bound-scene",
    build: (up) => {
      const r1 = apiCreateElement(up, { type: "rectangle", x: -150, y: -150, width: 100, height: 100 });
      const r2 = apiCreateElement(up, { type: "rectangle", x: 50, y: 50, width: 100, height: 100 });
      let arrow = apiCreateElement(up, {
        type: "arrow",
        elbowed: true,
        x: -45,
        y: -100.1,
        width: 90,
        height: 200,
        points: [
          [0, 0],
          [90, 200],
        ],
      });
      const scene = () => up.arrayToMap([r1, r2, arrow]);
      const start = up.calculateFixedPointForElbowArrowBinding(arrow, r1, "start", scene(), up.DEFAULT_ZOOM);
      arrow = { ...arrow, startBinding: binding(r1.id, start.fixedPoint) };
      const end = up.calculateFixedPointForElbowArrowBinding(arrow, r2, "end", scene(), up.DEFAULT_ZOOM);
      arrow = { ...arrow, endBinding: binding(r2.id, end.fixedPoint) };
      return {
        arrow,
        elements: [r1, r2, arrow],
        updates: {
          points: [
            [0, 0],
            [90, 200],
          ],
        },
      };
    },
  },
];

const coordinate = (r) =>
  r.pick([() => r.int(-400, 400), () => r.int(-4000, 4000) / 10, () => r.next() * 600 - 300])();

const unboundCases = () => {
  const cases = [];
  for (let n = 0; n < 120; n++) {
    cases.push({
      id: `unbound-${String(n).padStart(3, "0")}`,
      build: (up) => {
        const r = rng(1000 + n);
        const x = coordinate(r);
        const y = coordinate(r);
        const shape = n % 6;
        let end = [coordinate(r), coordinate(r)];
        if (shape === 1) end = [end[0], 0]; // horizontal
        if (shape === 2) end = [0, end[1]]; // vertical
        if (shape === 3) end = [r.int(-3, 3) / 4, r.int(-3, 3) / 4]; // tiny or coincident
        const middle = shape === 4 ? [[end[0] / 2, 0], [end[0] / 2, end[1]]] : [];
        const points = [[0, 0], ...middle, [coordinate(r), coordinate(r)]];
        const arrow = elbow(up, "arrow", x, y, points, {
          startArrowhead: r.pick([null, "arrow", "circle"]),
          endArrowhead: r.pick([null, "arrow", "triangle"]),
        });
        return {
          arrow,
          elements: [arrow],
          updates: { points: [[0, 0], end] },
        };
      },
    });
  }
  // Coordinates past MAX_POS are clamped (elbowArrow.ts:2119-2153).
  cases.push({
    id: "unbound-clamped",
    build: (up) => {
      const arrow = elbow(up, "arrow", 999900, -999950, [
        [0, 0],
        [10, 10],
      ]);
      return { arrow, elements: [arrow], updates: { points: [[0, 0], [300, -300]] } };
    },
  });
  return cases;
};

/** A random fixed point: inside, on an edge, outside, or at the 0.5 rule. */
const fixedPoint = (r) => {
  const edge = () => r.pick([0, 1]);
  const ratio = () => r.pick([() => r.int(0, 100) / 100, () => r.next(), () => 0.5])();
  switch (r.int(0, 5)) {
    case 0:
      return [edge(), ratio()];
    case 1:
      return [ratio(), edge()];
    case 2:
      return [ratio(), ratio()];
    case 3:
      return [r.int(-50, 150) / 100, r.int(-50, 150) / 100];
    case 4:
      return [0.5, r.pick([0, 1])];
    default:
      return [r.pick([0, 1]), 0.5];
  }
};

const geometry = (r, spread) => ({
  x: r.int(-spread, spread),
  y: r.int(-spread, spread),
  width: r.int(20, 240),
  height: r.int(20, 240),
  angle: r.chance(0.3) ? r.pick([Math.PI / 4, Math.PI / 2, 0.3, 5.5, Math.PI]) : 0,
  strokeWidth: r.pick([1, 2, 4]),
});

const boundCases = () => {
  const cases = [];
  for (let n = 0; n < 260; n++) {
    cases.push({
      id: `bound-${String(n).padStart(3, "0")}`,
      build: (up) => {
        const r = rng(5000 + n);
        const spread = r.pick([150, 400, 800]);
        const ends = r.pick(["both", "both", "both", "start", "end"]);
        const a = bindable(up, BINDABLE[n % BINDABLE.length], "a", geometry(r, spread));
        const b = bindable(up, r.pick(BINDABLE), "b", geometry(r, spread));
        const startBinding = ends !== "end" ? binding("a", fixedPoint(r)) : null;
        const endBinding =
          ends !== "start" ? binding(ends === "both" && r.chance(0.08) ? "a" : "b", fixedPoint(r)) : null;
        const x = coordinate(r);
        const y = coordinate(r);
        const arrow = {
          ...elbow(up, "arrow", x, y, [[0, 0], [coordinate(r), coordinate(r)]], {
            startArrowhead: r.pick([null, "arrow", "bar"]),
            endArrowhead: r.pick([null, "arrow", "triangle"]),
          }),
          startBinding,
          endBinding,
        };
        const elements = [a, b, arrow];
        return {
          arrow,
          elements,
          updates: { points: [[0, 0], [coordinate(r), coordinate(r)]] },
        };
      },
    });
  }
  return cases;
};

/**
 * A routed arrow to build fixed-segment cases on: seed n decides whether it
 * is bound, and to what.
 */
const routedArrow = (up, n) => {
  const r = rng(9000 + n);
  const bound = n % 3 !== 0;
  const a = bindable(up, r.pick(["rectangle", "ellipse", "diamond"]), "a", {
    x: -300,
    y: -200 + r.int(-80, 80),
    width: 120,
    height: 100,
  });
  const b = bindable(up, r.pick(["rectangle", "rectangle-round", "ellipse"]), "b", {
    x: 200 + r.int(-60, 60),
    y: 150 + r.int(-200, 200),
    width: 140,
    height: 90,
  });
  const base = {
    ...elbow(up, "arrow", -170, -150, [[0, 0], [360, 330]], { endArrowhead: "arrow" }),
    startBinding: bound ? binding("a", r.pick([[1, 0.5001], [0.5001, 1], [1, 0.25]])) : null,
    endBinding: bound ? binding("b", r.pick([[0, 0.5001], [0.5001, 0], [0.75, 0]])) : null,
  };
  const elements = [a, b, base];
  const first = route(up, base, elements, { points: [[0, 0], [360, 330]] });
  return { arrow: apply(base, first), elements, r };
};

/** A fixed segment for the arrow's segment `index`, moved by `by`. */
const movedSegment = (arrow, index, by) => {
  const start = arrow.points[index - 1];
  const end = arrow.points[index];
  const horizontal = Math.abs(start[1] - end[1]) < Math.abs(start[0] - end[0]);
  const shift = (p) => (horizontal ? [p[0], p[1] + by] : [p[0] + by, p[1]]);
  return { start: shift(start), end: shift(end), index };
};

const fixedCases = () => {
  const cases = [];
  for (let n = 0; n < 24; n++) {
    const tag = String(n).padStart(2, "0");
    const scene = (up) => {
      const { arrow, elements, r } = routedArrow(up, n);
      const count = arrow.points.length;
      // any segment, the first and last included (their special handling)
      const index = 1 + (n % Math.max(1, count - 1));
      const by = r.pick([-60, -25, 15, 30, 80]);
      const segment = movedSegment(arrow, index, by);
      return { arrow, elements, segment, r };
    };
    // segment move: updates carry only the new fixed segments
    cases.push({
      id: `fixed-${tag}-move`,
      build: (up) => {
        const { arrow, elements, segment } = scene(up);
        return { arrow, elements, updates: { fixedSegments: [segment] } };
      },
    });
    // then an endpoint drag with the segment fixed
    cases.push({
      id: `fixed-${tag}-drag`,
      build: (up) => {
        const { arrow, elements, segment, r } = scene(up);
        const moved = apply(arrow, route(up, arrow, elements, { fixedSegments: [segment] }));
        if (!moved.fixedSegments) return null;
        const last = moved.points[moved.points.length - 1];
        const dx = r.pick([-40, -10, 0, 25, 70]);
        const dy = r.pick([-50, 0, 20, 45]);
        const scene2 = elements.map((e) => (e.id === moved.id ? moved : e));
        return {
          arrow: moved,
          elements: scene2,
          updates: { points: [[0, 0], [last[0] + dx, last[1] + dy]] },
        };
      },
    });
    // then releasing it
    cases.push({
      id: `fixed-${tag}-release`,
      build: (up) => {
        const { arrow, elements, segment } = scene(up);
        const moved = apply(arrow, route(up, arrow, elements, { fixedSegments: [segment] }));
        if (!moved.fixedSegments) return null;
        const scene2 = elements.map((e) => (e.id === moved.id ? moved : e));
        return { arrow: moved, elements: scene2, updates: { fixedSegments: [] } };
      },
    });
    // then renormalising it (no updates)
    cases.push({
      id: `fixed-${tag}-renormalize`,
      build: (up) => {
        const { arrow, elements, segment } = scene(up);
        const moved = apply(arrow, route(up, arrow, elements, { fixedSegments: [segment] }));
        const scene2 = elements.map((e) => (e.id === moved.id ? moved : e));
        return { arrow: moved, elements: scene2, updates: {} };
      },
    });
    // then resizing (points and fixed segments together pass through)
    cases.push({
      id: `fixed-${tag}-resize`,
      build: (up) => {
        const { arrow, elements, segment } = scene(up);
        const moved = apply(arrow, route(up, arrow, elements, { fixedSegments: [segment] }));
        if (!moved.fixedSegments) return null;
        const scene2 = elements.map((e) => (e.id === moved.id ? moved : e));
        return {
          arrow: moved,
          elements: scene2,
          updates: {
            points: moved.points.map(([x, y]) => [x * 2, y]),
            fixedSegments: moved.fixedSegments.map((s) => ({
              ...s,
              start: [s.start[0] * 2, s.start[1]],
              end: [s.end[0] * 2, s.end[1]],
            })),
          },
        };
      },
    });
  }
  // A staircase of five segments with segments 2 and 4 fixed, unbound or
  // bound (start on the right or top side, end on the left or bottom), then
  // every kind of update on it.
  const STAIR = [
    [0, 0],
    [100, 0],
    [100, 100],
    [200, 100],
    [200, 200],
    [300, 200],
  ];
  const stairScenes = {
    unbound: { start: null, end: null },
    "right-left": { start: [1, 0.5001], end: [0, 0.5001] },
    "top-bottom": { start: [0.5001, 0], end: [0.5001, 1] },
    "top-left": { start: [0.5001, 0], end: [0, 0.5001] },
  };
  const stair = (up, which) => {
    const { start, end } = stairScenes[which];
    // the boxes sit around the stair's ends, whatever side the binding is on
    const a = bindable(up, "rectangle", "a", { x: 80, y: 60, width: 60, height: 60 });
    const b = bindable(up, "rectangle-round", "b", { x: 360, y: 190, width: 80, height: 60 });
    const segments = [2, 4].map((index) => ({ index, start: STAIR[index - 1], end: STAIR[index] }));
    const arrow = {
      ...elbow(up, "arrow", 100, 50, STAIR, { endArrowhead: "arrow", fixedSegments: segments }),
      startBinding: start && binding("a", start),
      endBinding: end && binding("b", end),
    };
    return { arrow, elements: [a, b, arrow], segments };
  };
  for (const which of Object.keys(stairScenes)) {
    const at = (name, updates) =>
      cases.push({
        id: `fixed-stair-${which}-${name}`,
        build: (up) => {
          const s = stair(up, which);
          return { arrow: s.arrow, elements: s.elements, updates: updates(s) };
        },
      });
    at("release-first", ({ segments }) => ({ fixedSegments: [segments[1]] }));
    at("release-second", ({ segments }) => ({ fixedSegments: [segments[0]] }));
    at("release-all", () => ({ fixedSegments: [] }));
    at("renormalize", () => ({}));
    at("move-first-fixed", ({ segments }) => ({
      fixedSegments: [
        { ...segments[0], start: [130, 0], end: [130, 100] },
        segments[1],
      ],
    }));
    at("move-second-fixed", ({ segments }) => ({
      fixedSegments: [
        segments[0],
        { ...segments[1], start: [240, 100], end: [240, 200] },
      ],
    }));
    at("fix-first-segment", ({ segments }) => ({
      fixedSegments: [{ index: 1, start: [0, 30], end: [100, 30] }, ...segments],
    }));
    at("fix-last-segment", ({ segments }) => ({
      fixedSegments: [...segments, { index: 5, start: [200, 170], end: [300, 170] }],
    }));
    for (const [name, end] of [
      ["drag-end-right", [340, 230]],
      ["drag-end-back", [150, 260]],
      ["drag-end-up", [320, -40]],
    ]) {
      at(name, () => ({ points: [[0, 0], end] }));
    }
    at("drag-start", () => ({ points: [[-30, 40], [300, 200]] }));
    at("resize", ({ segments }) => ({
      points: STAIR.map(([x, y]) => [x, y * 1.5]),
      fixedSegments: segments.map((s) => ({
        ...s,
        start: [s.start[0], s.start[1] * 1.5],
        end: [s.end[0], s.end[1] * 1.5],
      })),
    }));
  }
  // a second drag of an arrow the first drag made special at both ends,
  // back to plain and again special
  for (const which of ["top-bottom", "top-left"]) {
    for (const [name, end] of [
      ["plain", [300, 200]],
      ["special", [380, 330]],
    ]) {
      cases.push({
        id: `fixed-stair-${which}-drag-twice-${name}`,
        build: (up) => {
          const s = stair(up, which);
          const once = apply(s.arrow, route(up, s.arrow, s.elements, { points: [[0, 0], [340, 230]] }));
          const elements = s.elements.map((e) => (e.id === once.id ? once : e));
          const last = [end[0] - (once.x - s.arrow.x), end[1] - (once.y - s.arrow.y)];
          return { arrow: once, elements, updates: { points: [[0, 0], last] } };
        },
      });
    }
  }
  // the stair with a collinear pair and a zero-length segment, renormalised
  // (both passes of handleSegmentRenormalization)
  for (const [name, points, fixed] of [
    [
      "collinear",
      [[0, 0], [50, 0], [100, 0], [100, 100], [200, 100], [200, 200]],
      [{ index: 2, start: [50, 0], end: [100, 0] }, { index: 4, start: [100, 100], end: [200, 100] }],
    ],
    [
      "short",
      [[0, 0], [100, 0], [100, 100], [100.5, 100], [100.5, 200], [300, 200]],
      [{ index: 2, start: [100, 0], end: [100, 100] }, { index: 4, start: [100.5, 100], end: [100.5, 200] }],
    ],
  ]) {
    cases.push({
      id: `fixed-renormalize-${name}`,
      build: (up) => {
        const arrow = elbow(up, "arrow", 10, 10, points, { fixedSegments: fixed });
        return { arrow, elements: [arrow], updates: {} };
      },
    });
  }
  // releasing a zero-length fixed segment between two fixed ones: the
  // restored sub-route starts where it ends (see throwCases)
  cases.push({
    id: "fixed-release-zero-length",
    build: (up) => {
      const points = [[0, 0], [100, 0], [100, 0], [100, 100], [200, 100]];
      const segments = [
        { index: 1, start: [0, 0], end: [100, 0] },
        { index: 2, start: [100, 0], end: [100, 0] },
        { index: 3, start: [100, 0], end: [100, 100] },
      ];
      const arrow = elbow(up, "arrow", 10, 20, points, { fixedSegments: segments });
      return {
        arrow,
        elements: [arrow],
        updates: { fixedSegments: segments.filter((s) => s.index !== 2) },
      };
    },
  });
  return cases;
};

const edgeCases = () => [
  {
    id: "edge-one-point",
    build: (up) => {
      const arrow = elbow(up, "arrow", 10, 20, [[0, 0]]);
      return { arrow, elements: [arrow], updates: { points: [[0, 0], [50, 60]] } };
    },
  },
  {
    id: "edge-one-point-no-updates",
    build: (up) => {
      const arrow = elbow(up, "arrow", 10, 20, [[0, 0]]);
      return { arrow, elements: [arrow], updates: {} };
    },
  },
  {
    id: "edge-missing-binding-target-valid",
    build: (up) => {
      const arrow = {
        ...elbow(up, "arrow", 10, 20, [[0, 0], [100, 0], [100, 80]]),
        startBinding: binding("gone", [1, 0.5001]),
      };
      return { arrow, elements: [arrow], updates: { points: [[0, 0], [100, 80]] } };
    },
  },
  {
    id: "edge-missing-binding-target-invalid",
    build: (up) => {
      const arrow = {
        ...elbow(up, "arrow", 10, 20, [[0, 0], [100, 80]]),
        endBinding: binding("gone", [0, 0.5001]),
      };
      return { arrow, elements: [arrow], updates: { points: [[0, 0], [100, 80]] } };
    },
  },
  {
    id: "edge-non-bindable-target",
    build: (up) => {
      // an arrow is not bindable (typeChecks.ts:184-202)
      const line = up.newArrowElement({ type: "arrow", id: "line", x: 0, y: 0, points: [[0, 0], [10, 10]] });
      const arrow = {
        ...elbow(up, "arrow", 10, 20, [[0, 0], [100, 80]]),
        endBinding: binding("line", [0, 0.5001]),
      };
      return { arrow, elements: [line, arrow], updates: { points: [[0, 0], [120, 80]] } };
    },
  },
  {
    id: "edge-bound-text-not-bindable",
    build: (up) => {
      const r = bindable(up, "rectangle", "box", { x: 200, y: 0, width: 100, height: 100 });
      const label = { ...bindable(up, "text", "label", { x: 220, y: 40, width: 50, height: 25 }), containerId: "box" };
      const arrow = {
        ...elbow(up, "arrow", 10, 20, [[0, 0], [100, 80]]),
        endBinding: binding("label", [0, 0.5001]),
      };
      return { arrow, elements: [r, label, arrow], updates: { points: [[0, 0], [150, 30]] } };
    },
  },
  {
    id: "edge-empty-scene-valid",
    build: (up) => {
      const arrow = elbow(up, "arrow", 10, 20, [[0, 0], [100, 0], [100, 80]]);
      return { arrow, elements: [], updates: { points: [[0, 0], [100, 90]] } };
    },
  },
  {
    id: "edge-empty-scene-invalid",
    build: (up) => {
      const arrow = elbow(up, "arrow", 10, 20, [[0, 0], [100, 80]]);
      return { arrow, elements: [], updates: { points: [[0, 0], [100, 90]] } };
    },
  },
  {
    id: "edge-noop-short-circuit",
    build: (up) => {
      const arrow = elbow(up, "arrow", 10, 20, [[0, 0], [100, 0], [100, 80]]);
      return {
        arrow,
        elements: [arrow],
        updates: { points: [[0, 0], [100, 0], [100, 80]], startBinding: null, endBinding: null },
      };
    },
  },
  {
    id: "edge-noop-invalid-points-reroutes",
    build: (up) => {
      const arrow = elbow(up, "arrow", 10, 20, [[0, 0], [100, 80]]);
      return {
        arrow,
        elements: [arrow],
        updates: { points: [[0, 0], [100, 80]], startBinding: null, endBinding: null },
      };
    },
  },
  {
    id: "edge-updated-bindings",
    build: (up) => {
      const a = bindable(up, "rectangle", "a", { x: -200, y: -50, width: 100, height: 100 });
      const b = bindable(up, "ellipse", "b", { x: 150, y: 120, width: 100, height: 60 });
      const arrow = elbow(up, "arrow", -100, 0, [[0, 0], [250, 150]]);
      return {
        arrow,
        elements: [a, b, arrow],
        updates: {
          points: [[0, 0], [250, 150]],
          startBinding: binding("a", [1, 0.5001]),
          endBinding: binding("b", [0.5001, 0]),
        },
      };
    },
  },
  {
    id: "edge-renormalize-no-fixed-segments",
    build: (up) => {
      const arrow = { ...elbow(up, "arrow", 10, 20, [[0, 0], [100, 0], [100, 80]]), fixedSegments: null };
      return { arrow, elements: [arrow], updates: {} };
    },
  },
  {
    id: "edge-other-update-keys",
    build: (up) => {
      const arrow = {
        ...elbow(up, "arrow", 10, 20, [[0, 0], [100, 0], [100, 80]]),
        startBinding: binding("gone", [1, 0.5001]),
      };
      return { arrow, elements: [arrow], updates: {} };
    },
  },
];

// Where upstream throws: each case sets `throws` and records the exception's
// message as `error` (and no `result`). Every scene passes upstream's
// development invariants (elbowArrow.ts:926-976), so the message is the
// throw's own.
//
// handleEndpointDrag, "Second and third points must exist"
// (elbowArrow.ts:752-757): an endpoint drag on a two point arrow with a
// fixed segment (the third point is missing), with startIsSpecial null and
// false, and on a three point arrow whose start is special (the fourth).
//
// Two of the throws cannot be reached through updateElbowArrowPoints, and
// excali-editor's unit tests cover their guards alone:
// - handleEndpointDrag's "Second and third to last points must exist"
//   (elbowArrow.ts:823-828): Array.prototype.at counts negative indices from
//   the end, so indices length - 2 .. length - 4 miss only when length < 2,
//   and the start check has already thrown for any length under 3.
// - handleSegmentRelease's "Property 'points' is required"
//   (elbowArrow.ts:363-367): routeElbowArrow answers null or at least two
//   points. getDonglePosition always answers a point (1908-1922) that
//   calculateGrid puts on the grid (1851-1878), so the path runs between the
//   two dongles and startGlobalPoint and endGlobalPoint bracket it
//   (1494-1501); getElbowArrowCornerPoints and
//   removeElbowArrowShortSegments keep the first and last points. A null
//   route becomes [] and normalizeArrowElementUpdate reads global[0][0]
//   (2108) before the length check. fixed-release-zero-length is the nearest
//   scene: the released segment's neighbours meet, and the restored
//   sub-route is still two points.
const throwCases = () =>
  [
    ["null", null, [[0, 0], [100, 0]]],
    ["false", false, [[0, 0], [100, 0]]],
    ["true", true, [[0, 0], [100, 0], [100, 80]]],
  ].map(([name, startIsSpecial, points]) => ({
    id: `throw-drag-start-special-${name}`,
    throws: true,
    build: (up) => {
      // newArrowElement does not take startIsSpecial
      const arrow = {
        ...elbow(up, "arrow", 10, 20, points, {
          fixedSegments: [{ index: 1, start: points[0], end: points[1] }],
        }),
        startIsSpecial,
      };
      const last = points[points.length - 1];
      return {
        arrow,
        elements: [arrow],
        updates: { points: [[0, 0], [last[0] + 50, last[1] + 40]] },
      };
    },
  }));

const buildCases = () => [
  ...upstreamCases(),
  ...unboundCases(),
  ...boundCases(),
  ...fixedCases(),
  ...edgeCases(),
  ...throwCases(),
];

/** The updates as passed: a binding equal to the arrow's is the arrow's own. */
const asPassed = (arrow, updates) => {
  const passed = { ...updates };
  for (const key of ["startBinding", "endBinding"]) {
    if (key in passed && JSON.stringify(passed[key]) === JSON.stringify(arrow[key])) {
      passed[key] = arrow[key];
    }
  }
  return passed;
};

const runCase = (up, c) => {
  up.reseed(1);
  const built = c.build(up);
  if (!built) return null;
  const arrow = clone(built.arrow);
  const elements = clone(built.elements).map((e) => (e.id === arrow.id ? arrow : e));
  // Recorded before the call: handleSegmentMove writes into the fixed
  // segments it is given (elbowArrow.ts:526-557).
  const updates = clone(built.updates);
  const recorded = { id: c.id, arrow: clone(built.arrow), elements: clone(built.elements), updates };
  let result;
  try {
    result = up.updateElbowArrowPoints(arrow, up.arrayToMap(elements), asPassed(arrow, clone(updates)));
  } catch (error) {
    if (!c.throws) throw error;
    return { ...recorded, error: error.message };
  }
  if (c.throws) throw new Error(`${c.id}: upstream did not throw`);
  return { ...recorded, result: clone(result) };
};

// Every non-ASCII code unit as a \u escape (see restore-fixtures.mjs).
const asciiJson = (fixture) => {
  const text = JSON.stringify(fixture, null, 2).replace(
    /[\u0080-￿]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `${text}\n`;
};

/** Runs fn with console.error silenced (the 1e6 guard logs). */
const quietly = (fn) => {
  const log = console.error;
  console.error = () => {};
  try {
    return fn();
  } finally {
    console.error = log;
  }
};

const buildFixture = (up, commit) => {
  const ids = new Set();
  const cases = [];
  for (const c of buildCases()) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
    const recorded = quietly(() => runCase(up, c));
    if (recorded) cases.push(recorded);
  }
  return asciiJson({
    description:
      "updateElbowArrowPoints(arrow, arrayToMap(elements), updates), packages/element/src/elbowArrow.ts, " +
      "in upstream's test mode (ids id0.., timestamps 1, reseed(1) before each case), no options. " +
      "Generated by tools/goldens/elbow-routing-fixtures.mjs.",
    upstream: commit,
    cases,
  });
};

/** Runs fn with Math.random disabled (see header). */
const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating elbow routing fixtures");
  };
  try {
    return fn();
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
    process.stderr.write(`elbow-routing-fixtures: ${error.message}\n`);
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
      process.stderr.write(
        "elbow routing fixture is out of date: run node tools/goldens/elbow-routing-fixtures.mjs\n",
      );
      process.exit(1);
    }
    process.stdout.write(`elbow routing fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
