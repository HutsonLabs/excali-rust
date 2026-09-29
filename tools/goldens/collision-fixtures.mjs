#!/usr/bin/env node
// Hit testing fixtures for excali-editor (ex-507): upstream's own
// packages/element/src/collision.ts and distance.ts, run from the pinned
// checkout under plain Node on the scenes of upstream's collision tests and
// on seeded random scenes of every element type.
//
//   node tools/goldens/collision-fixtures.mjs            write the fixture
//   node tools/goldens/collision-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/collision-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/collision.json:
//
//   { "description", "upstream", "cases": [ { id, kind, elements, ... } ] }
//
// Every case holds the scene (`elements`, JSON as upstream holds it; the
// elements map is arrayToMap(elements)) and what upstream answered. Kinds:
//
// - "probe": `target` (an element id) and `probes`, each
//   { point, threshold, frameNameBound?, overrideShouldTestInside?,
//     hit, distance, inside, shouldTestInside, boundingBox, boundText,
//     boundingBoxOnly }:
//   hitElementItself({ point, element, threshold, elementsMap,
//   frameNameBound, overrideShouldTestInside }) with the result cache reset
//   first, distanceToElement(element, elementsMap, point),
//   isPointInElement(point, element, elementsMap), shouldTestInside(element),
//   hitElementBoundingBox(point, element, elementsMap, threshold),
//   hitElementBoundText(point, element, elementsMap) and
//   hitElementBoundingBoxOnly({ ... }, elementsMap). A distance of Infinity
//   is written as the string "Infinity".
// - "intersect": `target` and `segments`, each { segment, offset, onlyFirst,
//   result }: intersectElementWithLineSegment(element, elementsMap, segment,
//   offset, onlyFirst).
// - "binding": `points`, each { point, zoom, hovered, all }:
//   getHoveredElementForBinding(point, elements, elementsMap, { value: zoom })
//   (its id, or null) and getAllHoveredElementAtPoint (ids, front to back).
// - "inside": `pairs`, each { inner, outer, result }:
//   isBindableElementInsideOtherBindable(inner, outer, elementsMap).
//
// Cases:
// - upstream-*: the scenes of packages/element/tests/collision.test.tsx
//   built with API.createElement (apiCreateElement, h.state undefined), and
//   newFreeDrawElement where the test calls it.
// - random-*: seeded random elements of every type (sharp and round, filled
//   and transparent, rotated or not, bound text in containers and on
//   arrows, frames), probed near their outlines, inside, and outside, at
//   the thresholds getElementHitThreshold gives for zooms 0.1 to 30.
// - intersect-*, binding-*, inside-*: seeded random scenes for the other
//   entry points.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), reseed(1) before each case, and random
// scenes come from a Park-Miller generator seeded per case. Math.random
// throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "collision.json";

const ENTRY = `
export {
  hitElementItself,
  hitElementBoundingBox,
  hitElementBoundingBoxOnly,
  hitElementBoundText,
  getAllHoveredElementAtPoint,
  getHoveredElementForBinding,
  intersectElementWithLineSegment,
  isPointInElement,
  isBindableElementInsideOtherBindable,
  shouldTestInside,
} from "./packages/element/src/collision";
export { distanceToElement } from "./packages/element/src/distance";
export { getElementBounds } from "./packages/element/src/bounds";
export { getAllMidpoints } from "./packages/element/src/utils";
export { mutateElement } from "./packages/element/src/mutateElement";
export { reseed } from "./packages/common/src/random";
export {
  arrayToMap,
  DEFAULT_COLLISION_THRESHOLD,
  DEFAULT_VERTICAL_ALIGN,
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
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { setCustomTextMetricsProvider } from "./packages/element/src/textMeasurements";
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

const usage = () => {
  process.stderr.write("usage: collision-fixtures.mjs [--check] [--out DIR]\n");
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

/** App.getElementHitThreshold (App.tsx:6927-6934). */
const hitThreshold = (up, strokeWidth, zoom) =>
  Math.max(strokeWidth / 2 + 0.1, 0.85 * (up.DEFAULT_COLLISION_THRESHOLD / zoom));

const ZOOMS = [0.1, 0.25, 0.5, 1, 2, 5, 10, 30];

const number = (v) => (Number.isFinite(v) ? v : String(v));

// -- the result cache ---------------------------------------------------------

let resetCounter = 0;
/**
 * Replaces hitElementItself's single-entry cache with a miss on a throwaway
 * element (as collision.test.tsx's beforeEach does), so every probe is
 * computed afresh.
 */
const resetCache = (up) => {
  const element = up.newElement({
    type: "rectangle",
    x: 0,
    y: 0,
    width: 100,
    height: 100,
    backgroundColor: "#ffffff",
  });
  element.id = `cache-reset-${resetCounter++}`;
  up.hitElementItself({
    point: [50, 50],
    element,
    threshold: Infinity,
    elementsMap: new Map(),
  });
};

// -- probes -------------------------------------------------------------------

const probe = (up, elements, target, p) => {
  const elementsMap = up.arrayToMap(elements);
  const element = elementsMap.get(target);
  const args = {
    point: p.point,
    element,
    threshold: p.threshold,
    elementsMap,
    ...(p.frameNameBound ? { frameNameBound: p.frameNameBound } : {}),
    ...(p.overrideShouldTestInside ? { overrideShouldTestInside: true } : {}),
  };
  resetCache(up);
  const hit = up.hitElementItself(args);
  resetCache(up);
  const boundingBoxOnly = up.hitElementBoundingBoxOnly(args, elementsMap);
  return {
    ...p,
    hit,
    distance: number(up.distanceToElement(element, elementsMap, p.point)),
    inside: up.isPointInElement(p.point, element, elementsMap),
    shouldTestInside: up.shouldTestInside(element),
    boundingBox: up.hitElementBoundingBox(p.point, element, elementsMap, p.threshold),
    boundText: up.hitElementBoundText(p.point, element, elementsMap),
    boundingBoxOnly,
  };
};

/**
 * The elements as fresh objects, as a file or the clipboard gives them:
 * upstream caches bounds and shapes per element object and version, so a
 * builder's in-place edits (which keep the version) must not reach the
 * functions under test through a stale cache entry.
 */
const fresh = (elements) => clone(elements);

const probeCase = (up, id, elements, target, probes) => {
  const scene = fresh(elements);
  return {
    id,
    kind: "probe",
    elements: clone(scene),
    target,
    probes: probes.map((p) => probe(up, scene, target, p)),
  };
};

// -- upstream's scenes (collision.test.tsx) ------------------------------------

const createLoop = (up, { angle = 0, backgroundColor = "#ffc9c9", variability = "variable", points } = {}) => {
  const pts =
    points ??
    Array.from({ length: 81 }, (_, i) => [
      80 + 80 * Math.cos((i / 80) * 2 * Math.PI),
      40 + 40 * Math.sin((i / 80) * 2 * Math.PI),
    ]);
  return apiCreateElement(up, {
    type: "freedraw",
    x: 300,
    y: 200,
    ...up.getSizeFromPoints(pts),
    angle,
    strokeWidth: 2,
    backgroundColor,
    fillStyle: "solid",
    points: pts,
    strokeOptions: { variability, streamline: 0.5 },
  });
};

const rotate = ([x, y], [cx, cy], angle) => [
  (x - cx) * Math.cos(angle) - (y - cy) * Math.sin(angle) + cx,
  (x - cx) * Math.sin(angle) + (y - cy) * Math.cos(angle) + cy,
];

const straightStroke = (up, variability) =>
  apiCreateElement(up, {
    type: "freedraw",
    x: 0,
    y: 0,
    strokeWidth: 10,
    points: Array.from({ length: 21 }, (_, i) => [i * 5, 0]),
    strokeOptions: { variability, streamline: 0.5 },
  });

const upstreamCases = () => {
  const cases = [];
  const add = (id, build) => cases.push({ id, build });

  // "check rotated elements can be hit: arrow". UI.createElement clicks the
  // points with the arrow tool, whose default arrow type is round
  // (appState.ts:45) with an arrow end head, then sets the angle.
  add("upstream-rotated-arrow", (up) => {
    const arrow = apiCreateElement(up, {
      type: "arrow",
      roundness: { type: up.ROUNDNESS.PROPORTIONAL_RADIUS },
      endArrowhead: "arrow",
      x: 0,
      y: 0,
      width: 124,
      height: 302,
      angle: 1.8700426423973724,
      points: [
        [0, 0],
        [120, -198],
        [-4, -302],
      ],
    });
    return probeCase(up, "upstream-rotated-arrow", [arrow], arrow.id, [{ point: [88, -68], threshold: 10 }]);
  });

  // "hitElementItself cache": the four probes of the rectangle tests
  add("upstream-cache-filled-rectangle", (up) => {
    const r = apiCreateElement(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 100, backgroundColor: "#ffffff" });
    return probeCase(up, "upstream-cache-filled-rectangle", [r], r.id, [
      { point: [100.5, 50], threshold: 1 },
      { point: [100.5, 50], threshold: 10 },
    ]);
  });
  add("upstream-cache-transparent-rectangle", (up) => {
    const r = apiCreateElement(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 100, backgroundColor: "transparent" });
    return probeCase(up, "upstream-cache-transparent-rectangle", [r], r.id, [
      { point: [105, 50], threshold: 10 },
      { point: [105, 50], threshold: 6 },
      { point: [50, 50], threshold: 10 },
      { point: [50, 50], threshold: 10, overrideShouldTestInside: true },
    ]);
  });
  // "rechecks a bound label when only its arrow changes": the label before
  // and after the arrow moves 300px right
  for (const [suffix, arrowX] of [
    ["before", 200],
    ["after", 500],
  ]) {
    const id = `upstream-cache-bound-label-${suffix}`;
    add(id, (up) => {
      const arrow = apiCreateElement(up, {
        type: "arrow",
        x: 200,
        y: 200,
        width: 300,
        height: 0,
        points: [
          [0, 0],
          [300, 0],
        ],
      });
      const label = apiCreateElement(up, {
        type: "text",
        text: "label",
        x: 325,
        y: 187.5,
        width: 50,
        height: 25,
        containerId: arrow.id,
      });
      arrow.boundElements = [{ id: label.id, type: "text" }];
      arrow.x = arrowX;
      return probeCase(up, id, [arrow, label], label.id, [
        { point: [350, 200], threshold: 1 },
        { point: [650, 200], threshold: 1 },
      ]);
    });
  }

  // "freedraw collision matches the rendered stroke width"
  for (const variability of ["variable", "constant"]) {
    const id = `upstream-freedraw-straight-${variability}`;
    add(id, (up) => {
      const e = straightStroke(up, variability);
      return probeCase(
        up,
        id,
        [e],
        e.id,
        [
          [50, 0],
          [50, 10],
          [50, 20],
          [50, -20],
          [50, 45],
        ].map((point) => ({ point, threshold: 1 })),
      );
    });
  }
  add("upstream-freedraw-short-start-cap", (up) => {
    const e = up.newFreeDrawElement({
      type: "freedraw",
      x: 0,
      y: 0,
      strokeWidth: 1,
      simulatePressure: false,
      pressures: [1, 1, 1],
      points: [
        [0, 0],
        [0.01, 0],
        [2.9, 0],
      ],
      strokeOptions: { variability: "variable", streamline: 0.5 },
    });
    return probeCase(up, "upstream-freedraw-short-start-cap", [e], e.id, [{ point: [0.1, 4.9], threshold: 0.6 }]);
  });

  // "%s freedraw loop containment" and "freedraw loop fill containment"
  for (const variability of ["variable", "constant"]) {
    for (const [angleName, angle] of [
      ["0", 0],
      ["quarter-pi", Math.PI / 4],
    ]) {
      for (const backgroundColor of ["#ffc9c9", "transparent"]) {
        const fill = backgroundColor === "transparent" ? "transparent" : "filled";
        const id = `upstream-freedraw-loop-${variability}-${angleName}-${fill}`;
        add(id, (up) => {
          const e = createLoop(up, { angle, backgroundColor, variability });
          const center = [380, 240];
          return probeCase(
            up,
            id,
            [e],
            e.id,
            [
              rotate([400, 250], center, e.angle),
              rotate(center, center, e.angle),
              rotate([400, 240], center, e.angle),
              rotate([380, 250], center, e.angle),
              [310, 210],
              [415, 255],
            ].map((point) => ({ point, threshold: 1 })),
          );
        });
      }
    }
  }
  for (const [angleName, angle] of [
    ["0", 0],
    ["quarter-pi", Math.PI / 4],
  ]) {
    const id = `upstream-freedraw-loop-unsnapped-${angleName}`;
    add(id, (up) => {
      const e = createLoop(up, { angle });
      up.mutateElement(e, up.arrayToMap([e]), {
        points: [...e.points.slice(0, -1), [160, 36]],
      });
      return probeCase(up, id, [e], e.id, [{ point: rotate([440, 239.5], [380, 240], e.angle), threshold: 1 }]);
    });
  }
  const triangle = [
    [0, 0],
    [200, 0],
    [0, 100],
    [0, 0],
  ];
  add("upstream-freedraw-asymmetric-loop", (up) => {
    const e = createLoop(up, { angle: Math.PI / 4, points: triangle });
    return probeCase(up, "upstream-freedraw-asymmetric-loop", [e], e.id, [
      { point: rotate([330, 205], [400, 250], e.angle), threshold: 1 },
    ]);
  });
  add("upstream-freedraw-smoothed-fill-outside-bounds", (up) => {
    const e = createLoop(up, { angle: (2 * Math.PI) / 3, points: triangle });
    return probeCase(up, "upstream-freedraw-smoothed-fill-outside-bounds", [e], e.id, [
      { point: rotate([436, 240], [400, 250], e.angle), threshold: 1 },
    ]);
  });
  for (const [name, points, point] of [
    [
      "retraced",
      [
        [0, 0],
        [100, 100],
        [0, 0],
      ],
      [350, 250],
    ],
    [
      "two-identical-points",
      [
        [0, 0],
        [0.3, 0],
        [0.3, 0.3],
        [0, 0.3],
        [0, 0],
      ],
      [300.15, 200.15],
    ],
    [
      "nearly-closed-three-points",
      [
        [0, 0],
        [100, 100],
        [3, 0],
      ],
      [350, 250],
    ],
    [
      "smoothed-beyond-raw-polygon",
      [
        [200, 100],
        [100, 200],
        [0, 100],
        [100, 0],
        [200, 100],
      ],
      [305, 285],
    ],
    [
      "self-intersecting",
      Array.from({ length: 6 }, (_, i) => {
        const a = -Math.PI / 2 + (((i * 2) % 5) / 5) * 2 * Math.PI;
        return [100 + 95 * Math.cos(a), 100 + 95 * Math.sin(a)];
      }),
      [400, 300],
    ],
  ]) {
    const id = `upstream-freedraw-${name}`;
    add(id, (up) => {
      const e = createLoop(up, { points });
      return probeCase(up, id, [e], e.id, [{ point, threshold: 1 }]);
    });
  }
  add("upstream-freedraw-points-changed", (up) => {
    const e = createLoop(up);
    up.mutateElement(e, up.arrayToMap([e]), {
      points: [
        [0, 0],
        [160, 0],
        [0, 80],
        [0, 0],
      ],
    });
    return probeCase(up, "upstream-freedraw-points-changed", [e], e.id, [{ point: [415, 255], threshold: 1 }]);
  });

  // "ellipse outline hit test"
  add("upstream-ellipse-center-line", (up) => {
    const e = apiCreateElement(up, {
      type: "ellipse",
      x: 0,
      y: 0,
      width: 400,
      height: 100,
      backgroundColor: "transparent",
    });
    return probeCase(up, "upstream-ellipse-center-line", [e], e.id, [{ point: [210, 50], threshold: 10 }]);
  });

  // "intersectElementWithLineSegment": a rounded diamond's left corner
  add("upstream-intersect-rounded-diamond", (up) => {
    const d = apiCreateElement(up, {
      type: "diamond",
      x: 100,
      y: -100,
      width: 200,
      height: 200,
      roundness: { type: up.ROUNDNESS.PROPORTIONAL_RADIUS },
    });
    const map = up.arrayToMap([d]);
    const [, , left] = up.getAllMidpoints(d, map);
    const segment = [
      [200, left[1]],
      [-200, left[1]],
    ];
    return {
      id: "upstream-intersect-rounded-diamond",
      kind: "intersect",
      elements: clone([d]),
      target: d.id,
      // getAllMidpoints(diamond)[2], the midpoint of the left corner
      leftMidpoint: clone(left),
      segments: [
        { segment, offset: 6, onlyFirst: false, result: clone(up.intersectElementWithLineSegment(d, map, segment, 6)) },
      ],
    };
  });

  // "binding hit tests"
  const bindingCase = (up, id, built, points) => {
    const elements = fresh(built);
    const map = up.arrayToMap(elements);
    return {
      id,
      kind: "binding",
      elements: clone(elements),
      points: points.map(([point, zoom]) => ({
        point,
        zoom,
        hovered: up.getHoveredElementForBinding(point, elements, map, { value: zoom })?.id ?? null,
        all: up.getAllHoveredElementAtPoint(point, elements, map, { value: zoom }).map((e) => e.id),
      })),
    };
  };
  add("upstream-binding-zoom", (up) => {
    const rect = apiCreateElement(up, { id: "rect", type: "rectangle", x: 0, y: 0, width: 100, height: 100 });
    return bindingCase(up, "upstream-binding-zoom", [rect], [
      [[120, 50], 1],
      [[120, 50], 0.4],
    ]);
  });
  for (const bg of ["#ffc9c9", "transparent"]) {
    const id = `upstream-binding-cover-${bg === "transparent" ? "transparent" : "opaque"}`;
    add(id, (up) => {
      const hidden = apiCreateElement(up, { id: "hidden", type: "rectangle", x: 30, y: 30, width: 40, height: 40, index: "a0" });
      const cover = apiCreateElement(up, {
        id: "cover",
        type: "rectangle",
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        backgroundColor: bg,
        index: "a1",
      });
      return bindingCase(up, id, [hidden, cover], [[[50, 50], 1]]);
    });
  }
  add("upstream-binding-image", (up) => {
    const hidden = apiCreateElement(up, { id: "hidden", type: "rectangle", x: 30, y: 30, width: 40, height: 40, index: "a0" });
    const image = apiCreateElement(up, { id: "image", type: "image", x: 0, y: 0, width: 100, height: 100, index: "a1" });
    return bindingCase(up, "upstream-binding-image", [hidden, image], [[[50, 50], 1]]);
  });
  for (const bg of ["#ffc9c9", "transparent"]) {
    const id = `upstream-binding-locked-${bg === "transparent" ? "transparent" : "opaque"}`;
    add(id, (up) => {
      const hidden = apiCreateElement(up, { id: "hidden", type: "rectangle", x: 30, y: 30, width: 40, height: 40, index: "a0" });
      const locked = apiCreateElement(up, {
        id: "locked",
        type: "rectangle",
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        backgroundColor: bg,
        locked: true,
        index: "a1",
      });
      return bindingCase(up, id, [hidden, locked], [[[50, 50], 1]]);
    });
  }
  add("upstream-binding-nested", (up) => {
    const container = apiCreateElement(up, { id: "container", type: "rectangle", x: 0, y: 0, width: 200, height: 200 });
    const child = apiCreateElement(up, { id: "child", type: "rectangle", x: 18, y: 80, width: 60, height: 40 });
    return bindingCase(up, "upstream-binding-nested", [container, child], [
      [[5, 100], 1],
      [[10, 100], 1],
    ]);
  });
  add("upstream-binding-badge", (up) => {
    const container = apiCreateElement(up, { id: "container", type: "rectangle", x: 0, y: 0, width: 200, height: 200 });
    const badge = apiCreateElement(up, { id: "badge", type: "rectangle", x: -30, y: 80, width: 60, height: 40 });
    return bindingCase(up, "upstream-binding-badge", [container, badge], [
      [[3, 92], 1],
      [[-5, 108], 1],
    ]);
  });
  add("upstream-binding-circle-transparent", (up) => {
    const circle = apiCreateElement(up, {
      id: "circle",
      type: "ellipse",
      x: 0,
      y: 0,
      width: 200,
      height: 200,
      backgroundColor: "transparent",
      index: "a1",
    });
    return bindingCase(up, "upstream-binding-circle-transparent", [circle], [[[100, 100], 1]]);
  });
  add("upstream-binding-circle-opaque", (up) => {
    const hidden = apiCreateElement(up, { id: "hidden", type: "rectangle", x: 90, y: 90, width: 20, height: 20, index: "a0" });
    const circle = apiCreateElement(up, {
      id: "circle",
      type: "ellipse",
      x: 0,
      y: 0,
      width: 200,
      height: 200,
      backgroundColor: "#ffc9c9",
      index: "a1",
    });
    return bindingCase(up, "upstream-binding-circle-opaque", [hidden, circle], [[[100, 100], 1]]);
  });

  return cases.map(({ id, build }) => ({ id, build }));
};

// -- random scenes ------------------------------------------------------------

const KINDS = [
  "rectangle",
  "rectangle-round",
  "diamond",
  "diamond-round",
  "ellipse",
  "text",
  "text-in-container",
  "arrow-label",
  "image",
  "frame",
  "magicframe",
  "iframe",
  "embeddable",
  "stickynote",
  "line",
  "line-round",
  "line-loop",
  "line-loop-round",
  "arrow",
  "arrow-round",
  "arrow-elbow",
  "freedraw",
  "freedraw-constant",
  "freedraw-loop",
  "freedraw-loop-constant",
];

const BACKGROUNDS = ["transparent", "#ffc9c9", "#a5d8ff00", "#ffec99"];

const randomPoints = (r, n, spread) => {
  const pts = [[0, 0]];
  for (let i = 1; i < n; i++) pts.push([r.int(-spread, spread), r.int(-spread, spread)]);
  return pts;
};

const loopPoints = (r, n, rx, ry) => {
  const pts = [];
  const wobble = r.range(0, 0.25);
  for (let i = 0; i < n; i++) {
    const a = (i / n) * 2 * Math.PI;
    const k = 1 + wobble * Math.sin(3 * a);
    pts.push([rx + rx * k * Math.cos(a), ry + ry * k * Math.sin(a)]);
  }
  pts.push([...pts[0]]);
  const [x0, y0] = pts[0];
  return pts.map(([x, y]) => [x - x0, y - y0]);
};

/** The scene of one random element of `kind` (the target is the last). */
const randomScene = (up, r, kind) => {
  const x = r.int(-300, 300);
  const y = r.int(-300, 300);
  const width = r.int(10, 260);
  const height = r.pick([r.int(10, 260), width]);
  const angle = r.chance(0.4) ? r.pick([Math.PI / 4, Math.PI / 2, 0.3, 5.5, Math.PI, r.range(0, 2 * Math.PI)]) : 0;
  const strokeWidth = r.pick([1, 2, 4]);
  const backgroundColor = r.pick(BACKGROUNDS);
  const common = { x, y, width, height, angle, strokeWidth, backgroundColor };
  const round = { type: up.ROUNDNESS.PROPORTIONAL_RADIUS };
  const adaptive = { type: up.ROUNDNESS.ADAPTIVE_RADIUS };
  const linear = (type, points, rest = {}) =>
    apiCreateElement(up, { type, ...common, ...up.getSizeFromPoints(points), points, roundness: null, ...rest });
  switch (kind) {
    case "rectangle":
      return [apiCreateElement(up, { type: "rectangle", ...common, roundness: null })];
    case "rectangle-round":
      return [apiCreateElement(up, { type: "rectangle", ...common, roundness: r.pick([round, adaptive]) })];
    case "diamond":
      return [apiCreateElement(up, { type: "diamond", ...common, roundness: null })];
    case "diamond-round":
      return [apiCreateElement(up, { type: "diamond", ...common, roundness: round })];
    case "ellipse":
      return [apiCreateElement(up, { type: "ellipse", ...common })];
    case "text":
      return [apiCreateElement(up, { type: "text", text: "hit me", ...common })];
    case "text-in-container": {
      const container = apiCreateElement(up, {
        type: r.pick(["rectangle", "diamond", "ellipse"]),
        ...common,
        backgroundColor: "transparent",
      });
      const label = apiCreateElement(up, {
        type: "text",
        text: "inside",
        x: x + width / 4,
        y: y + height / 3,
        width: width / 2,
        height: height / 3,
        angle,
        containerId: container.id,
      });
      container.boundElements = [{ id: label.id, type: "text" }];
      // the container is the target; the label is found through it
      return [label, container];
    }
    case "arrow-label": {
      const arrow = linear("arrow", randomPoints(r, r.int(2, 4), 200), {
        roundness: r.chance(0.5) ? round : null,
      });
      const label = apiCreateElement(up, {
        type: "text",
        text: "label",
        x: x + 20,
        y: y + 20,
        width: 50,
        height: 25,
        containerId: arrow.id,
      });
      arrow.boundElements = [{ id: label.id, type: "text" }];
      return r.chance(0.5) ? [arrow, label] : [label, arrow];
    }
    case "image":
      return [apiCreateElement(up, { type: "image", ...common })];
    case "frame":
      return [apiCreateElement(up, { type: "frame", ...common })];
    case "magicframe":
      return [apiCreateElement(up, { type: "magicframe", ...common })];
    case "iframe":
      return [apiCreateElement(up, { type: "iframe", ...common })];
    case "embeddable":
      return [apiCreateElement(up, { type: "embeddable", ...common })];
    case "stickynote":
      return [apiCreateElement(up, { type: "stickynote", ...common, roundness: r.chance(0.5) ? round : null })];
    case "line":
      return [linear("line", randomPoints(r, r.int(2, 5), 200))];
    case "line-round":
      return [linear("line", randomPoints(r, r.int(2, 5), 200), { roundness: round })];
    case "line-loop":
      return [linear("line", loopPoints(r, r.int(3, 8), r.int(20, 120), r.int(20, 120)))];
    case "line-loop-round":
      return [linear("line", loopPoints(r, r.int(4, 10), r.int(20, 120), r.int(20, 120)), { roundness: round })];
    case "arrow":
      return [linear("arrow", randomPoints(r, r.int(2, 4), 200))];
    case "arrow-round":
      return [linear("arrow", randomPoints(r, r.int(2, 4), 200), { roundness: round })];
    case "arrow-elbow": {
      const a = r.int(-200, 200);
      const b = r.int(-200, 200);
      const points = [
        [0, 0],
        [a, 0],
        [a, b],
        [a + r.int(-100, 100), b],
      ];
      return [linear("arrow", points, { elbowed: true, angle: 0 })];
    }
    case "freedraw":
    case "freedraw-constant": {
      const points = [[0, 0]];
      for (let i = 1; i < r.int(3, 30); i++) {
        const [px, py] = points[points.length - 1];
        points.push([px + r.range(-12, 12), py + r.range(-12, 12)]);
      }
      return [
        apiCreateElement(up, {
          type: "freedraw",
          ...common,
          ...up.getSizeFromPoints(points),
          points,
          strokeOptions: { variability: kind === "freedraw" ? "variable" : "constant", streamline: 0.5 },
        }),
      ];
    }
    case "freedraw-loop":
    case "freedraw-loop-constant": {
      const points = loopPoints(r, r.int(8, 40), r.int(20, 100), r.int(20, 100));
      return [
        apiCreateElement(up, {
          type: "freedraw",
          ...common,
          ...up.getSizeFromPoints(points),
          points,
          fillStyle: "solid",
          strokeOptions: { variability: kind === "freedraw-loop" ? "variable" : "constant", streamline: 0.5 },
        }),
      ];
    }
  }
  throw new Error(`unknown kind ${kind}`);
};

/** Probe points around an element: its outline, its inside, and beyond. */
const probePoints = (up, r, element, elementsMap, n) => {
  const [x1, y1, x2, y2] = up.getElementBounds(element, elementsMap);
  const w = Math.max(x2 - x1, 1);
  const h = Math.max(y2 - y1, 1);
  const cx = (x1 + x2) / 2;
  const cy = (y1 + y2) / 2;
  const points = [[cx, cy]];
  for (let i = 1; i < n; i++) {
    switch (r.int(0, 3)) {
      case 0: // anywhere in the bounds, a margin around
        points.push([r.range(x1 - 20, x2 + 20), r.range(y1 - 20, y2 + 20)]);
        break;
      case 1: {
        // near the bounds' edges
        const t = r.next();
        const edge = r.int(0, 3);
        const off = r.range(-6, 6);
        points.push(
          edge === 0
            ? [x1 + t * w, y1 + off]
            : edge === 1
            ? [x2 + off, y1 + t * h]
            : edge === 2
            ? [x1 + t * w, y2 + off]
            : [x1 + off, y1 + t * h],
        );
        break;
      }
      case 2: // on a ray from the centre, near where an outline would be
        {
          const a = r.range(0, 2 * Math.PI);
          const k = r.range(0.3, 0.6);
          points.push([cx + Math.cos(a) * w * k, cy + Math.sin(a) * h * k]);
        }
        break;
      default: // integer points inside
        points.push([r.int(Math.floor(x1), Math.ceil(x2)), r.int(Math.floor(y1), Math.ceil(y2))]);
    }
  }
  return points;
};

const randomProbeCases = () => {
  const cases = [];
  const PER_KIND = 8;
  KINDS.forEach((kind, k) => {
    for (let n = 0; n < PER_KIND; n++) {
      const id = `random-${kind}-${n}`;
      cases.push({
        id,
        build: (up) => {
          const r = rng(20000 + k * 100 + n);
          const elements = fresh(randomScene(up, r, kind));
          const target = elements[elements.length - 1];
          const map = up.arrayToMap(elements);
          const points = probePoints(up, r, target, map, 12);
          const probes = points.map((point) => {
            const zoom = r.pick(ZOOMS);
            const p = { point, threshold: hitThreshold(up, target.strokeWidth, zoom) };
            if (r.chance(0.1)) p.overrideShouldTestInside = true;
            if ((target.type === "frame" || target.type === "magicframe") && r.chance(0.5)) {
              p.frameNameBound = { x: target.x, y: target.y - 24, width: 80, height: 20 };
            }
            return p;
          });
          return probeCase(up, id, elements, target.id, probes);
        },
      });
    }
  });
  return cases;
};

const randomIntersectCases = () => {
  const cases = [];
  KINDS.forEach((kind, k) => {
    for (let n = 0; n < 4; n++) {
      const id = `intersect-${kind}-${n}`;
      cases.push({
        id,
        build: (up) => {
          const r = rng(40000 + k * 100 + n);
          const elements = fresh(randomScene(up, r, kind));
          const target = elements[elements.length - 1];
          const map = up.arrayToMap(elements);
          const [x1, y1, x2, y2] = up.getElementBounds(target, map);
          const cx = (x1 + x2) / 2;
          const cy = (y1 + y2) / 2;
          const segments = [];
          for (let i = 0; i < 8; i++) {
            const from =
              i % 2 === 0
                ? [cx + r.range(-10, 10), cy + r.range(-10, 10)]
                : [r.range(x1 - 50, x2 + 50), r.range(y1 - 50, y2 + 50)];
            const a = r.range(0, 2 * Math.PI);
            const len = r.range(50, 600);
            const segment = [from, [from[0] + Math.cos(a) * len, from[1] + Math.sin(a) * len]];
            const offset = r.pick([0, 0, 3, 6.5]);
            const onlyFirst = r.chance(0.3);
            segments.push({
              segment,
              offset,
              onlyFirst,
              result: clone(up.intersectElementWithLineSegment(target, map, segment, offset, onlyFirst)),
            });
          }
          return { id, kind: "intersect", elements: clone(elements), target: target.id, segments };
        },
      });
    }
  });
  return cases;
};

const BINDABLE_KINDS = [
  "rectangle",
  "rectangle-round",
  "diamond",
  "diamond-round",
  "ellipse",
  "text",
  "image",
  "frame",
  "iframe",
  "embeddable",
  "stickynote",
];

const randomBindingCases = () => {
  const cases = [];
  for (let n = 0; n < 60; n++) {
    const id = `binding-${String(n).padStart(3, "0")}`;
    cases.push({
      id,
      build: (up) => {
        const r = rng(60000 + n);
        const count = r.int(1, 4);
        const elements = [];
        for (let i = 0; i < count; i++) {
          const [e] = randomScene(up, r, r.pick(BINDABLE_KINDS));
          e.id = `e${i}`;
          e.index = `a${i}`;
          e.x = r.int(-150, 150);
          e.y = r.int(-150, 150);
          if (r.chance(0.15)) e.locked = true;
          elements.push(e);
        }
        // a frame's children follow it in z-order
        const frame = elements.find((e) => e.type === "frame");
        if (frame) {
          for (const e of elements) {
            if (e !== frame && elements.indexOf(e) > elements.indexOf(frame) && r.chance(0.6)) e.frameId = frame.id;
          }
        }
        const scene = fresh(elements);
        const map = up.arrayToMap(scene);
        // a copy: getElementBounds answers its cached array
        const all = [...up.getElementBounds(scene[0], map)];
        for (const e of scene) {
          const b = up.getElementBounds(e, map);
          all[0] = Math.min(all[0], b[0]);
          all[1] = Math.min(all[1], b[1]);
          all[2] = Math.max(all[2], b[2]);
          all[3] = Math.max(all[3], b[3]);
        }
        const points = [];
        for (let i = 0; i < 10; i++) {
          points.push([
            [r.range(all[0] - 30, all[2] + 30), r.range(all[1] - 30, all[3] + 30)],
            r.pick([0.2, 0.4, 0.5, 1, 2, 4]),
          ]);
        }
        return {
          id,
          kind: "binding",
          elements: clone(scene),
          points: points.map(([point, zoom]) => ({
            point,
            zoom,
            hovered: up.getHoveredElementForBinding(point, scene, map, { value: zoom })?.id ?? null,
            all: up.getAllHoveredElementAtPoint(point, scene, map, { value: zoom }).map((e) => e.id),
          })),
        };
      },
    });
  }
  return cases;
};

const INSIDE_KINDS = ["rectangle", "rectangle-round", "diamond", "diamond-round", "ellipse", "image", "frame"];

const randomInsideCases = () => {
  const cases = [];
  for (let n = 0; n < 40; n++) {
    const id = `inside-${String(n).padStart(3, "0")}`;
    cases.push({
      id,
      build: (up) => {
        const r = rng(80000 + n);
        const [outer] = randomScene(up, r, r.pick(INSIDE_KINDS));
        outer.id = "outer";
        outer.x = 0;
        outer.y = 0;
        outer.width = r.int(150, 300);
        outer.height = r.int(150, 300);
        const pairs = [];
        const elements = [outer];
        for (let i = 0; i < 4; i++) {
          const [inner] = randomScene(up, r, r.pick(INSIDE_KINDS));
          inner.id = `inner${i}`;
          inner.width = r.int(10, 160);
          inner.height = r.int(10, 160);
          inner.x = r.int(-40, outer.width - inner.width + 40);
          inner.y = r.int(-40, outer.height - inner.height + 40);
          elements.push(inner);
        }
        const scene = fresh(elements);
        const map = up.arrayToMap(scene);
        const [outerElement] = scene;
        for (const inner of scene.slice(1)) {
          pairs.push({
            inner: inner.id,
            outer: outerElement.id,
            result: up.isBindableElementInsideOtherBindable(inner, outerElement, map),
          });
          pairs.push({
            inner: outerElement.id,
            outer: inner.id,
            result: up.isBindableElementInsideOtherBindable(outerElement, inner, map),
          });
        }
        return { id, kind: "inside", elements: clone(scene), pairs };
      },
    });
  }
  return cases;
};

const buildCases = () => [
  ...upstreamCases(),
  ...randomProbeCases(),
  ...randomIntersectCases(),
  ...randomBindingCases(),
  ...randomInsideCases(),
];

// -- output -------------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  // getFreedrawFillPolygon (shape.ts:592-619) draws its curve with an
  // unseeded RoughGenerator at roughness 0: rough.js multiplies every draw
  // by the roughness, so the draws cannot reach the output. Anywhere else
  // Math.random throws.
  Math.random = () => {
    if (new Error().stack.includes("getFreedrawFillPolygon")) return 0.5;
    throw new Error("Math.random called while generating collision fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
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

// Every non-ASCII code unit as a \u escape (see restore-fixtures.mjs).
const asciiJson = (fixture) => {
  const text = format(fixture).replace(
    /[\u0080-￿]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return text;
};

const buildFixture = (up, commit) => {
  const ids = new Set();
  const cases = [];
  for (const c of buildCases()) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
    up.reseed(1);
    resetCounter = 0;
    const recorded = quietly(() => c.build(up));
    if (recorded.id !== c.id) throw new Error(`case ${c.id} recorded as ${recorded.id}`);
    cases.push(recorded);
  }
  return asciiJson({
    description:
      "hitElementItself, distanceToElement, isPointInElement, shouldTestInside, hitElementBoundingBox, " +
      "hitElementBoundText, hitElementBoundingBoxOnly, intersectElementWithLineSegment, " +
      "getHoveredElementForBinding, getAllHoveredElementAtPoint and isBindableElementInsideOtherBindable " +
      "(packages/element/src/collision.ts, distance.ts) in upstream's test mode (ids id0.., timestamps 1, " +
      "reseed(1) before each case). Generated by tools/goldens/collision-fixtures.mjs.",
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
    process.stderr.write(`collision-fixtures: ${error.message}\n`);
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
      process.stderr.write("collision fixture is out of date: run node tools/goldens/collision-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`collision fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
