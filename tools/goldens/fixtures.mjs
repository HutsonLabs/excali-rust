// Fixture inputs for the goldens. Only inputs live here: every output number
// comes from upstream's code (element goldens) or the pinned packages
// (rough.js / perfect-freehand goldens).
//
// Element fixtures start from the same base as upstream's
// packages/excalidraw/tests/fixtures/elementFixture.ts (seed 1041657908) and
// vary one property family at a time so a failing golden in the Rust port
// points at one rule.

export const SEEDS = [1, 7, 1041657908];
export const ROUGHNESSES = [0, 1, 2]; // architect, artist, cartoonist
export const STROKE_WIDTHS = [1, 2, 4]; // thin, bold, extra bold
export const FILL_STYLES = ["hachure", "cross-hatch", "zigzag", "solid"];
// rough.js's "dots" filler is left out: it jitters every dot with Math.random
// (roughjs 4.6.4 bin/fillers/dot-filler.js:33-34), so it has no stable output,
// and Excalidraw never uses it (element/src/types.ts:19).
export const ROUGH_FILL_STYLES = [...FILL_STYLES, "dashed", "zigzag-line"];
export const ARROWHEADS = [
  "arrow",
  "bar",
  "circle",
  "circle_outline",
  "triangle",
  "triangle_outline",
  "diamond",
  "diamond_outline",
  "cardinality_one",
  "cardinality_many",
  "cardinality_one_or_many",
  "cardinality_exactly_one",
  "cardinality_zero_or_one",
  "cardinality_zero_or_many",
];

const FIXTURE_SEED = 1041657908;
const BACKGROUND = "#a5d8ff";

/** Render configurations, as passed to ShapeCache.generateElementShape. */
export const RENDER = {
  export: { isExporting: true, canvasBackgroundColor: "#ffffff", theme: "light", validatedEmbeds: [] },
  exportDark: { isExporting: true, canvasBackgroundColor: "#ffffff", theme: "dark", validatedEmbeds: [] },
  exportTinted: { isExporting: true, canvasBackgroundColor: "#fff9db", theme: "light", validatedEmbeds: [] },
  editor: { isExporting: false, canvasBackgroundColor: "#ffffff", theme: "light", validatedEmbeds: [] },
};

const base = (id, type, props = {}) => ({
  id,
  type,
  x: 0,
  y: 0,
  width: 200,
  height: 120,
  angle: 0,
  strokeColor: "#1e1e1e",
  backgroundColor: "transparent",
  fillStyle: "solid",
  strokeWidth: 2,
  strokeStyle: "solid",
  roughness: 1,
  opacity: 100,
  groupIds: [],
  frameId: null,
  roundness: null,
  index: null,
  seed: FIXTURE_SEED,
  version: 1,
  versionNonce: 0,
  isDeleted: false,
  boundElements: null,
  updated: 1,
  created: null,
  link: null,
  locked: false,
  ...props,
});

const bounds = (points) => {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  return {
    width: Math.max(...xs) - Math.min(...xs),
    height: Math.max(...ys) - Math.min(...ys),
  };
};

const linear = (id, type, points, props = {}) =>
  base(id, type, {
    ...bounds(points),
    points,
    startBinding: null,
    endBinding: null,
    startArrowhead: null,
    endArrowhead: type === "arrow" ? "arrow" : null,
    ...(type === "arrow" ? { elbowed: false } : { polygon: false }),
    ...props,
  });

const elementCase = (id, element, renderConfig = RENDER.export, extra = {}) => ({
  id,
  element: { ...element, id: id.replace(/[^A-Za-z0-9_-]/g, "_") },
  renderConfig,
  ...extra,
});

// -- rectangle, diamond, ellipse ---------------------------------------------

const closedShapeCases = (type) => {
  const cases = [];
  const add = (id, props, render) => cases.push(elementCase(`${type}/${id}`, base("", type, props), render));
  for (const seed of SEEDS) {
    for (const roughness of ROUGHNESSES) {
      add(`seed${seed}-r${roughness}`, { seed, roughness });
    }
  }
  for (const fillStyle of FILL_STYLES) {
    for (const strokeWidth of STROKE_WIDTHS) {
      add(`fill-${fillStyle}-sw${strokeWidth}`, { fillStyle, strokeWidth, backgroundColor: BACKGROUND });
    }
  }
  for (const strokeStyle of ["dashed", "dotted"]) {
    for (const strokeWidth of STROKE_WIDTHS) {
      add(`${strokeStyle}-sw${strokeWidth}`, { strokeStyle, strokeWidth });
    }
    add(`${strokeStyle}-hachure`, { strokeStyle, fillStyle: "hachure", backgroundColor: BACKGROUND });
  }
  // adjustRoughness (shape.ts:172-193): unchanged, halved, thirded, capped
  add("small-30x12", { width: 30, height: 12 });
  add("small-30x12-r2", { width: 30, height: 12, roughness: 2 });
  add("tiny-8x6", { width: 8, height: 6 });
  add("tiny-8x6-r2", { width: 8, height: 6, roughness: 2 });
  add("narrow-60x18", { width: 60, height: 18 });
  add("square-20x50", { width: 20, height: 50 });
  add("odd-101x77", { width: 101, height: 77, fillStyle: "hachure", backgroundColor: BACKGROUND });
  add("large-640x420", { width: 640, height: 420, fillStyle: "cross-hatch", backgroundColor: BACKGROUND });
  add("stroke-colour", { strokeColor: "#e03131", backgroundColor: "#ffc9c9", fillStyle: "hachure" });
  // theme: colours go through applyDarkModeFilter
  add("dark", { backgroundColor: BACKGROUND, fillStyle: "hachure" }, RENDER.exportDark);
  add("dark-solid", { backgroundColor: "#ffec99" }, RENDER.exportDark);
  add("editor", { backgroundColor: BACKGROUND, fillStyle: "hachure" }, RENDER.editor);
  if (type !== "ellipse") {
    // getCornerRadius (utils.ts:528-549)
    add("round-legacy", { roundness: { type: 1 } });
    add("round-proportional", { roundness: { type: 2 } });
    add("round-proportional-small", { width: 40, height: 16, roundness: { type: 2 } });
    add("round-adaptive-120", { roundness: { type: 3 } });
    add("round-adaptive-300x200", { width: 300, height: 200, roundness: { type: 3 } });
    add("round-adaptive-value10", { width: 300, height: 200, roundness: { type: 3, value: 10 } });
    add("round-adaptive-hachure", { roundness: { type: 3 }, fillStyle: "hachure", backgroundColor: BACKGROUND });
    add("round-adaptive-dashed", { roundness: { type: 3 }, strokeStyle: "dashed" });
    add("round-adaptive-r2", { roundness: { type: 3 }, roughness: 2 });
    add("round-small-16x40", { width: 16, height: 40, roundness: { type: 3 } });
    for (const seed of SEEDS) add(`round-seed${seed}`, { seed, roundness: { type: 3 } });
  }
  return cases;
};

// -- line ---------------------------------------------------------------------

const ZIGZAG = [[0, 0], [60, 40], [120, -10], [200, 50]];
const LOOP = [[0, 0], [120, -20], [180, 80], [40, 110], [4, 3]]; // last within 8px of first
const TRIANGLE = [[0, 0], [100, 0], [50, 80], [0, 0]];

const lineCases = () => {
  const cases = [];
  const add = (id, points, props, render) =>
    cases.push(elementCase(`line/${id}`, linear("", "line", points, props), render));
  add("two-point", [[0, 0], [200, 80]]);
  add("zigzag", ZIGZAG);
  add("curve", ZIGZAG, { roundness: { type: 2 } });
  for (const seed of SEEDS) {
    for (const roughness of ROUGHNESSES) {
      add(`zigzag-seed${seed}-r${roughness}`, ZIGZAG, { seed, roughness });
      add(`curve-seed${seed}-r${roughness}`, ZIGZAG, { seed, roughness, roundness: { type: 2 } });
    }
  }
  add("loop-transparent", LOOP);
  for (const fillStyle of FILL_STYLES) {
    add(`loop-${fillStyle}`, LOOP, { fillStyle, backgroundColor: BACKGROUND });
    add(`loop-curve-${fillStyle}`, LOOP, { fillStyle, backgroundColor: BACKGROUND, roundness: { type: 2 } });
  }
  add("polygon-triangle", TRIANGLE, { polygon: true, backgroundColor: BACKGROUND, fillStyle: "hachure" });
  add("polygon-triangle-curve", TRIANGLE, { polygon: true, backgroundColor: BACKGROUND, roundness: { type: 2 } });
  for (const strokeStyle of ["dashed", "dotted"]) {
    for (const strokeWidth of STROKE_WIDTHS) {
      add(`${strokeStyle}-sw${strokeWidth}`, ZIGZAG, { strokeStyle, strokeWidth });
    }
  }
  add("short-30", [[0, 0], [30, 10]]);
  add("short-30-r2", [[0, 0], [30, 10]], { roughness: 2 });
  add("single-point", [[0, 0]]);
  add("dark-loop", LOOP, { backgroundColor: BACKGROUND, fillStyle: "hachure" }, RENDER.exportDark);
  return cases;
};

// -- arrow --------------------------------------------------------------------

const arrowCases = () => {
  const cases = [];
  const add = (id, points, props, render) =>
    cases.push(elementCase(`arrow/${id}`, linear("", "arrow", points, props), render));
  add("two-point", [[0, 0], [200, 80]]);
  add("zigzag", ZIGZAG);
  add("curve", ZIGZAG, { roundness: { type: 2 } });
  add("no-heads", ZIGZAG, { endArrowhead: null });
  add("both-heads", ZIGZAG, { startArrowhead: "arrow" });
  for (const seed of SEEDS) {
    for (const roughness of ROUGHNESSES) {
      add(`seed${seed}-r${roughness}`, ZIGZAG, { seed, roughness });
      add(`curve-seed${seed}-r${roughness}`, ZIGZAG, { seed, roughness, roundness: { type: 2 } });
    }
  }
  for (const strokeStyle of ["dashed", "dotted"]) {
    for (const strokeWidth of STROKE_WIDTHS) {
      add(`${strokeStyle}-sw${strokeWidth}`, ZIGZAG, { strokeStyle, strokeWidth });
    }
  }
  // a looped arrow with a background still never fills (shape.ts:254-255)
  add("loop-with-background", LOOP, { backgroundColor: BACKGROUND, fillStyle: "hachure" });
  add("short-30", [[0, 0], [30, 10]]);
  add("short-30-r2", [[0, 0], [30, 10]], { roughness: 2 });
  add("dark", ZIGZAG, { strokeColor: "#1971c2" }, RENDER.exportDark);
  // arrows saved before arrowheads existed have no keys: end defaults to "arrow"
  const legacy = linear("", "arrow", ZIGZAG);
  delete legacy.startArrowhead;
  delete legacy.endArrowhead;
  cases.push(elementCase("arrow/legacy-missing-arrowhead-keys", legacy));
  return cases;
};

const arrowheadCases = () => {
  const cases = [];
  const add = (id, points, props, render) =>
    cases.push(elementCase(`arrowhead/${id}`, linear("", "arrow", points, props), render));
  const LONG = [[0, 0], [100, 30], [220, 40]];
  for (const head of ARROWHEADS) {
    for (const strokeWidth of STROKE_WIDTHS) {
      add(`${head}-end-sw${strokeWidth}`, LONG, { strokeWidth, endArrowhead: head });
      add(`${head}-start-sw${strokeWidth}`, LONG, { strokeWidth, startArrowhead: head, endArrowhead: null });
    }
    add(`${head}-curve`, LONG, { endArrowhead: head, roundness: { type: 2 } });
    add(`${head}-dotted`, LONG, { endArrowhead: head, strokeStyle: "dotted" });
    add(`${head}-dashed`, LONG, { endArrowhead: head, strokeStyle: "dashed" });
    add(`${head}-short`, [[0, 0], [24, 8]], { endArrowhead: head });
    add(`${head}-both-r2`, LONG, { startArrowhead: head, endArrowhead: head, roughness: 2 });
  }
  for (const head of ["circle_outline", "triangle_outline", "diamond_outline"]) {
    add(`${head}-tinted-canvas`, LONG, { endArrowhead: head }, RENDER.exportTinted);
    add(`${head}-dark`, LONG, { endArrowhead: head }, RENDER.exportDark);
  }
  return cases;
};

const elbowArrowCases = () => {
  const cases = [];
  const add = (id, points, props) =>
    cases.push(
      elementCase(
        `elbow/${id}`,
        linear("", "arrow", points, {
          elbowed: true,
          fixedSegments: null,
          startIsSpecial: null,
          endIsSpecial: null,
          ...props,
        }),
      ),
    );
  add("L", [[0, 0], [160, 0], [160, 120]]);
  add("S", [[0, 0], [100, 0], [100, 80], [220, 80]]);
  add("U", [[0, 0], [0, -60], [180, -60], [180, 20]]);
  add("tight-corners", [[0, 0], [20, 0], [20, 12], [60, 12], [60, 40]]);
  add("long-sw4", [[0, 0], [300, 0], [300, 200], [520, 200]], { strokeWidth: 4 });
  add("dashed", [[0, 0], [160, 0], [160, 120]], { strokeStyle: "dashed" });
  add("both-heads", [[0, 0], [160, 0], [160, 120]], { startArrowhead: "circle", endArrowhead: "triangle" });
  add("r2", [[0, 0], [100, 0], [100, 80], [220, 80]], { roughness: 2 });
  // shape.ts:908-918: any |coordinate| > 1e6 is not rendered
  add("extreme-coordinates", [[0, 0], [2e6, 0]]);
  return cases;
};

// -- freedraw -----------------------------------------------------------------

const wave = (n, amplitude, length) =>
  Array.from({ length: n }, (_, i) => {
    const t = i / (n - 1);
    return [Math.round(t * length * 100) / 100, Math.round(Math.sin(t * Math.PI * 2) * amplitude * 100) / 100];
  });

const loop = (n, r) =>
  Array.from({ length: n }, (_, i) => {
    const a = (i / (n - 1)) * Math.PI * 2;
    return [Math.round((r + Math.cos(a) * r) * 100) / 100, Math.round((r + Math.sin(a) * r) * 100) / 100];
  });

const pressuresFor = (points) =>
  points.map((_, i) => Math.round((0.25 + 0.6 * Math.abs(Math.sin(i / 3))) * 1000) / 1000);

export const FREEHAND_POINTS = {
  wave: wave(24, 30, 220),
  loop: loop(20, 50),
  scribble: [[0, 0], [3, 1], [9, 4], [18, 10], [30, 14], [41, 13], [50, 8], [55, 1], [57, -6], [62, -10], [75, -9], [90, -2], [104, 9], [110, 16]],
  dot: [[0, 0]],
  pair: [[0, 0], [12, 5]],
};

const EXCALIDRAW_FREEHAND = (strokeWidth, streamline, simulatePressure) => ({
  size: strokeWidth * 4.25,
  thinning: 0.6,
  smoothing: 0.5,
  streamline,
  easing: "easeOutSine",
  simulatePressure,
  last: true,
});

/** Direct perfect-freehand cases; `easing` is named, see EASINGS. */
export const freehandCases = () => {
  const cases = [];
  const withPressure = (points) => points.map((p, i) => [...p, pressuresFor(points)[i]]);
  for (const [name, points] of Object.entries(FREEHAND_POINTS)) {
    for (const sw of STROKE_WIDTHS) {
      cases.push({ id: `excalidraw/${name}-sw${sw}-simulated`, points, options: EXCALIDRAW_FREEHAND(sw, 0.5, true) });
    }
    cases.push({
      id: `excalidraw/${name}-sw2-pressure`,
      points: withPressure(points),
      options: EXCALIDRAW_FREEHAND(2, 0.5, false),
    });
    cases.push({ id: `excalidraw/${name}-sw2-streamline0.2`, points, options: EXCALIDRAW_FREEHAND(2, 0.2, true) });
  }
  // perfect-freehand's own defaults and taper options, for the port's unit tests
  cases.push({ id: "defaults/wave", points: FREEHAND_POINTS.wave, options: {} });
  cases.push({
    id: "defaults/scribble-linear-easing",
    points: FREEHAND_POINTS.scribble,
    options: { size: 12, thinning: 0.5, smoothing: 0.5, streamline: 0.5, easing: "linear", last: false },
  });
  cases.push({
    id: "defaults/wave-tapered",
    points: FREEHAND_POINTS.wave,
    options: { size: 10, thinning: -0.4, start: { taper: 40 }, end: { taper: 30 }, last: true },
  });
  cases.push({
    id: "defaults/scribble-no-thinning",
    points: withPressure(FREEHAND_POINTS.scribble),
    options: { size: 6, thinning: 0, simulatePressure: false, last: true },
  });
  // Branches Excalidraw's options never reach, so the port is checked over
  // the whole library: taper true/false, flat caps, cap easings, single
  // points with a taper, object points, missing or negative pressures,
  // reversals, exact duplicates, streamline extremes, and empty results.
  const { wave: w, scribble: s, dot, pair } = FREEHAND_POINTS;
  const reversal = [[0, 0], [20, 0], [40, 0], [60, 0], [80, 0], [60, 1], [40, 2], [20, 3], [0, 4], [20, 5], [40, 6]];
  const edge = (id, points, options) => cases.push({ id: `edge/${id}`, points, options });
  edge("wave-taper-true", w, { size: 8, start: { taper: true }, end: { taper: true }, last: true });
  edge("wave-taper-false-flat-caps", w, {
    size: 8,
    start: { taper: false, cap: false },
    end: { taper: false, cap: false },
    last: true,
  });
  edge("scribble-flat-caps", s, { size: 12, start: { cap: false }, end: { cap: false } });
  edge("wave-taper-cap-easings", w, {
    size: 9,
    start: { taper: 60, easing: "linear" },
    end: { taper: 45, easing: "easeOutSine" },
    last: true,
  });
  edge("wave-start-taper-only", w, { size: 9, start: { taper: 25 }, last: true });
  edge("wave-end-taper-only", w, { size: 9, end: { taper: 25 }, last: false });
  edge("dot-start-taper", dot, { size: 10, start: { taper: 20 } });
  edge("dot-start-taper-last", dot, { size: 10, start: { taper: 20 }, last: true });
  edge("dot-end-taper", dot, { size: 10, end: { taper: 20 } });
  edge("dot-with-pressure", [[3, 4, 0.8]], { size: 10, simulatePressure: false });
  edge("pair-with-pressure", withPressure(pair), { size: 10, simulatePressure: false });
  edge("pair-not-last", pair, { size: 10 });
  edge(
    "scribble-objects",
    s.map(([x, y], i) => (i % 3 === 0 ? { x, y } : { x, y, pressure: pressuresFor(s)[i] })),
    { size: 7, simulatePressure: false, last: true },
  );
  edge(
    "scribble-missing-and-negative-pressure",
    s.map((p, i) => (i % 4 === 0 ? p : [...p, i % 4 === 1 ? -1 : pressuresFor(s)[i]])),
    { size: 7, thinning: 0.7, simulatePressure: false, easing: "linear", last: true },
  );
  edge("reversal", reversal, { size: 6, streamline: 0, smoothing: 0.2, last: true });
  edge("reversal-streamline1", reversal, { size: 6, streamline: 1, last: true });
  edge(
    "duplicates",
    [[0, 0], [0, 0], [10, 0], [10, 0], [10, 0], [25, 5], [25, 5], [40, 20], [40, 20]],
    { size: 5, streamline: 0, last: true },
  );
  // Three identical points leave a single stroke point (one or two points
  // are padded to five or two first), the only way into
  // getStrokeOutlinePoints' one-point branch through getStroke.
  const same = [[7, 3], [7, 3], [7, 3]];
  edge("all-duplicates", same, { size: 10 });
  edge("all-duplicates-start-taper", same, { size: 10, start: { taper: 20 } });
  edge("all-duplicates-end-taper-last", same, { size: 10, end: { taper: 20 }, last: true });
  edge("scribble-size-0", s, { size: 0, last: true });
  edge("empty", [], {});
  return cases;
};

/** Named easings used by the direct perfect-freehand cases. */
export const EASINGS = {
  easeOutSine: (t) => Math.sin((t * Math.PI) / 2), // shape.ts:1241
  linear: (t) => t,
};

// -- laser pointer (ex-214) ---------------------------------------------------

/**
 * Named `sizeMapping`s for the laser-pointer cases (a function cannot be
 * written to JSON). `constantWidth` is getConstantWidthFreedrawOutline's
 * (packages/element/src/shape.ts:1247-1253); `trail` is laserTrails.ts:28-40
 * with the timestamp in milliseconds carried as the pressure, as the laser
 * tool does with performance.now(), read against a fixed clock of 1000 ms.
 */
export const SIZE_MAPPINGS = {
  one: () => 1,
  constantWidth: (d) => Math.max(0.1, d.pressure),
  pressure: (d) => d.pressure,
  tiny: () => 0.1,
  zero: () => 0,
  zeroBefore3: (d) => (d.currentIndex < 3 ? 0 : 1),
  firstIndexTiny: (d) => (d.currentIndex === 0 ? 0.01 : 1),
  trail: (d) => {
    const easeOut = (k) => 1 - Math.pow(1 - k, 4); // common/src/utils.ts:232-234
    const t = Math.max(0, 1 - (1000 - d.pressure) / 1000);
    const l = (50 - Math.min(50, d.totalLength - d.currentIndex)) / 50;
    return Math.min(easeOut(l), easeOut(t));
  },
};

/** A laser trail: the pressure of point i is its timestamp. */
const timed = (points) => points.map(([x, y], i) => [x, y, 400 + i * 25]);
const withR = (points, r) => points.map(([x, y]) => [x, y, r]);

/**
 * Direct cases for the vendored laser-pointer (packages/laser-pointer/src):
 * `new LaserPointer(options)`, `addPoint` for each point, then `close()` if
 * `close`, `options.keepHead = keepHeadAfter` if set (animatedTrail.ts:131-132),
 * and `getStrokeOutline(sizeOverride)`. `sizeMapping` is named, see
 * SIZE_MAPPINGS.
 */
export const laserPointerCases = () => {
  const cases = [];
  const add = (id, points, options, extra = {}) => cases.push({ id, points, options, ...extra });
  const excalidraw = (sw, streamline) => ({ size: sw * 1.4, streamline, simplify: 0, sizeMapping: "constantWidth" });
  // getConstantWidthFreedrawOutline's calls: [x, y, 1] with Excalidraw's options
  for (const [name, points] of Object.entries(FREEHAND_POINTS)) {
    for (const sw of STROKE_WIDTHS) add(`excalidraw/${name}-sw${sw}`, withR(points, 1), excalidraw(sw, 0.5));
    add(`excalidraw/${name}-sw2-streamline0.2`, withR(points, 1), excalidraw(2, 0.2));
  }
  // Every other branch of the library: the defaults (size 2, streamline
  // 0.45, simplify 0.1 on the output), the three simplify phases (tail
  // throws once the tail is stabilised), corners turning either way and at
  // speed, zero sizes before the visible start, keepHead, size overrides,
  // one and two points, duplicates and no points.
  const { wave: w, scribble: s, loop: l, dot, pair } = FREEHAND_POINTS;
  // spikes of about 28 degrees, turning left and right (corners below 75)
  const zigzag = [[0, 0], [10, 40], [20, 0], [30, 40], [40, 0], [50, 40], [60, 0]];
  const hairpins = [[0, 0], [30, 0], [60, 0], [58, 4], [30, 6], [0, 8], [2, 12], [30, 14]];
  // at a smoothed speed over 35 the corner angle halves to 37.5 degrees: the
  // 45-degree turn at (400, 0) is not a corner, the spike at (440, 60) is
  const fast = [[0, 0], [100, 0], [200, 0], [300, 0], [400, 0], [340, 60], [440, 60], [340, 70], [360, 150]];
  const reversal = [[0, 0], [20, 0], [40, 0], [20, 0], [0, 0], [20, 0]];
  const edge = (id, points, options, extra) => add(`edge/${id}`, points, options, extra);
  edge("defaults-wave", withR(w, 1), {});
  edge("defaults-scribble", withR(s, 1), {});
  edge("defaults-loop", withR(l, 1), {});
  edge("simplify-output-1.5", withR(w, 1), { size: 6, simplify: 1.5 });
  edge("simplify-output-8", withR(l, 1), { size: 6, simplify: 8 });
  edge("simplify-input-2", withR(w, 1), { size: 4, simplify: 2, simplifyPhase: "input" });
  edge("simplify-input-pair", withR(pair, 1), { size: 4, simplify: 2, simplifyPhase: "input" });
  edge("simplify-tail-short", withR(s.slice(0, 5), 1), { size: 4, simplify: 1, simplifyPhase: "tail" });
  edge("simplify-tail-long", withR(w, 1), { size: 4, simplify: 1, simplifyPhase: "tail" });
  edge("simplify-tail-close", withR(s.slice(0, 5), 1), { size: 4, simplify: 1, simplifyPhase: "tail" }, { close: true });
  edge("simplify-tail-off", withR(w, 1), { size: 4, simplify: 0, simplifyPhase: "tail" }, { close: true });
  edge("streamline-0", withR(s, 1), { size: 3, streamline: 0, simplify: 0 });
  edge("streamline-0.9", withR(s, 1), { size: 3, streamline: 0.9, simplify: 0 });
  edge("zigzag", withR(zigzag, 1), { size: 5, streamline: 0, simplify: 0 });
  edge("zigzag-reversed", withR([...zigzag].reverse(), 1), { size: 5, streamline: 0, simplify: 0 });
  edge("hairpins", withR(hairpins, 1), { size: 3, streamline: 0, simplify: 0 });
  edge("fast-corners", withR(fast, 1), { size: 8, streamline: 0, simplify: 0 });
  edge("reversal", withR(reversal, 1), { size: 4, streamline: 0, simplify: 0 });
  edge(
    "duplicates",
    withR([[0, 0], [0, 0], [10, 0], [10, 0], [10, 0], [25, 5], [25, 5], [40, 20], [40, 20]], 1),
    { size: 4, streamline: 0.3, simplify: 0 },
  );
  edge("same-xy-new-pressure", [[5, 5, 1], [5, 5, 0.2], [15, 5, 1], [15, 5, 0.5], [25, 9, 1]], {
    size: 4,
    simplify: 0,
    sizeMapping: "pressure",
  });
  edge("pressure", s.map(([x, y], i) => [x, y, 0.2 + ((i * 7) % 10) / 10]), { size: 5, simplify: 0, sizeMapping: "pressure" });
  edge("pressure-constant-width", s.map(([x, y], i) => [x, y, ((i * 3) % 5) / 10]), {
    size: 5,
    simplify: 0,
    sizeMapping: "constantWidth",
  });
  edge("zero-before-3", withR(s, 1), { size: 4, simplify: 0, sizeMapping: "zeroBefore3" });
  edge("zero-before-3-short", withR(s.slice(0, 5), 1), { size: 4, simplify: 0, sizeMapping: "zeroBefore3" });
  edge("zero", withR(s, 1), { size: 4, simplify: 0, sizeMapping: "zero" });
  edge("zero-keep-head", withR(s, 1), { size: 4, simplify: 0, sizeMapping: "zero", keepHead: true });
  edge("keep-head", timed(w), { size: 4, simplify: 0, keepHead: true, sizeMapping: "trail" });
  edge("keep-head-closed", timed(w), { size: 4, simplify: 0, keepHead: true, sizeMapping: "trail" }, {
    close: true,
    keepHeadAfter: false,
  });
  edge("first-index-tiny", withR(w, 1), { size: 4, simplify: 0, sizeMapping: "firstIndexTiny" });
  edge("trail", timed(w), { size: 4, streamline: 0.4, simplify: 0, sizeMapping: "trail" });
  edge("trail-size-override", timed(w), { size: 4, streamline: 0.4, simplify: 0, sizeMapping: "trail" }, { sizeOverride: 2.5 });
  edge("size-override", withR(s, 1), { size: 4, simplify: 0 }, { sizeOverride: 1.5 });
  edge("dot", withR(dot, 1), { size: 3 });
  edge("dot-tiny", withR(dot, 1), { size: 3, sizeMapping: "tiny" });
  edge("dot-size-override", withR(dot, 1), { size: 3 }, { sizeOverride: 7 });
  edge("pair", withR(pair, 1), { size: 3 });
  edge("pair-vertical", withR([[0, 0], [0, 12]], 1), { size: 3, streamline: 0 });
  edge("pair-tiny", withR(pair, 1), { size: 3, sizeMapping: "tiny" });
  edge("pair-one-tiny", [[0, 0, 1], [12, 5, 0.1]], { size: 3, streamline: 0, sizeMapping: "pressure" });
  edge("all-duplicates", withR([[7, 3], [7, 3], [7, 3]], 1), { size: 3 });
  edge("empty", [], {});
  return cases;
};

const freedrawCases = () => {
  const cases = [];
  const add = (id, points, props, extra) =>
    cases.push(
      elementCase(
        `freedraw/${id}`,
        base("", "freedraw", {
          ...bounds(points),
          points,
          pressures: [],
          simulatePressure: true,
          strokeOptions: { variability: "variable", streamline: 0.5 },
          ...props,
        }),
        RENDER.export,
        extra,
      ),
    );
  for (const [name, points] of Object.entries(FREEHAND_POINTS)) {
    for (const sw of STROKE_WIDTHS) {
      add(`${name}-sw${sw}`, points, { strokeWidth: sw }, { freehandCase: `excalidraw/${name}-sw${sw}-simulated` });
    }
    add(
      `${name}-pressure`,
      points,
      { simulatePressure: false, pressures: pressuresFor(points) },
      { freehandCase: `excalidraw/${name}-sw2-pressure` },
    );
    add(
      `${name}-streamline0.2`,
      points,
      { strokeOptions: { variability: "variable", streamline: 0.2 } },
      { freehandCase: `excalidraw/${name}-sw2-streamline0.2` },
    );
    for (const sw of STROKE_WIDTHS) {
      add(`${name}-constant-sw${sw}`, points, {
        strokeWidth: sw,
        strokeOptions: { variability: "constant", streamline: 0.5 },
      });
    }
  }
  // closed loops get a rough fill behind the stroke (shape.ts:980-989)
  for (const fillStyle of FILL_STYLES) {
    add(`loop-fill-${fillStyle}`, FREEHAND_POINTS.loop, { backgroundColor: BACKGROUND, fillStyle });
  }
  add("loop-fill-constant", FREEHAND_POINTS.loop, {
    backgroundColor: BACKGROUND,
    fillStyle: "hachure",
    strokeOptions: { variability: "constant", streamline: 0.5 },
  });
  add("loop-transparent", FREEHAND_POINTS.loop, {});
  // elements saved before strokeOptions existed use DEFAULT_STROKE_STREAMLINE
  add("legacy-no-stroke-options", FREEHAND_POINTS.wave, { strokeOptions: undefined });
  add("pressure-empty-points", [], { simulatePressure: false, pressures: [], width: 0, height: 0 });
  return cases;
};

// -- iframe / embeddable (modifyIframeLikeForRoughOptions, shape.ts:262-293) ---

const iframeLikeCases = () => {
  const cases = [];
  const add = (id, type, props, render) => cases.push(elementCase(`${type}/${id}`, base("", type, props), render));
  const transparent = { strokeColor: "transparent", backgroundColor: "transparent" };
  for (const type of ["embeddable", "iframe"]) {
    add("transparent-export", type, transparent, RENDER.export);
    add("transparent-editor", type, transparent, RENDER.editor);
    add("coloured", type, { strokeColor: "#1971c2", backgroundColor: "#e7f5ff", fillStyle: "hachure" });
    add("round-transparent-export", type, { ...transparent, roundness: { type: 3 } }, RENDER.export);
    add("stroke-only", type, { backgroundColor: "transparent" }, RENDER.editor);
    add("dark-transparent-export", type, transparent, RENDER.exportDark);
  }
  cases.push(
    elementCase(
      "embeddable/transparent-editor-validated",
      base("", "embeddable", transparent),
      { ...RENDER.editor, validatedEmbeds: ["embeddable_transparent-editor-validated"] },
    ),
  );
  return cases;
};

// -- the M2 matrix (ex-g202) ---------------------------------------------------

const freedrawElement = (points, variability) =>
  base("", "freedraw", {
    ...bounds(points),
    points,
    pressures: [],
    simulatePressure: true,
    strokeOptions: { variability, streamline: 0.5 },
  });

/**
 * Every element type, each with a background colour, at every fill style x
 * roughness x seed. Types with more than one rendering path appear once per
 * path: sharp and rounded boxes (roundness 3 for rectangles, 2 for
 * diamonds), line loops sharp and curved and a polygon line, arrows sharp
 * and curved with a filled (triangle) end head, freedraw loops at variable
 * and constant width. Text, image, frame, magicframe and stickynote have no
 * rough shape (shape.ts:996-1006) and are recorded to prove it.
 */
const MATRIX_VARIANTS = {
  rectangle: () => base("", "rectangle"),
  "rectangle-round": () => base("", "rectangle", { roundness: { type: 3 } }),
  diamond: () => base("", "diamond"),
  "diamond-round": () => base("", "diamond", { roundness: { type: 2 } }),
  ellipse: () => base("", "ellipse"),
  iframe: () => base("", "iframe"),
  embeddable: () => base("", "embeddable"),
  "line-loop": () => linear("", "line", LOOP),
  "line-loop-curve": () => linear("", "line", LOOP, { roundness: { type: 2 } }),
  "line-polygon": () => linear("", "line", TRIANGLE, { polygon: true }),
  arrow: () => linear("", "arrow", ZIGZAG, { endArrowhead: "triangle" }),
  "arrow-curve": () => linear("", "arrow", ZIGZAG, { endArrowhead: "triangle", roundness: { type: 2 } }),
  "freedraw-variable": () => freedrawElement(FREEHAND_POINTS.loop, "variable"),
  "freedraw-constant": () => freedrawElement(FREEHAND_POINTS.loop, "constant"),
  // packages/excalidraw/tests/fixtures/elementFixture.ts textFixture
  // (fontFamily 5 is DEFAULT_FONT_FAMILY, Excalifont)
  text: () =>
    base("", "text", {
      fontSize: 20,
      baseFontSize: null,
      fontFamily: 5,
      text: "original text",
      originalText: "original text",
      textAlign: "left",
      verticalAlign: "top",
      containerId: null,
      lineHeight: 1.25,
      autoResize: false,
    }),
  image: () => base("", "image", { fileId: null, status: "pending", scale: [1, 1], crop: null }),
  frame: () => base("", "frame", { name: null }),
  magicframe: () => base("", "magicframe", { name: null }),
  stickynote: () => base("", "stickynote", { baseHeight: 120 }),
};

const matrixCases = () => {
  const cases = [];
  for (const [variant, make] of Object.entries(MATRIX_VARIANTS)) {
    for (const fillStyle of FILL_STYLES) {
      for (const roughness of ROUGHNESSES) {
        for (const seed of SEEDS) {
          const element = { ...make(), backgroundColor: BACKGROUND, fillStyle, roughness, seed };
          cases.push(elementCase(`matrix/${variant}/${fillStyle}-r${roughness}-seed${seed}`, element));
        }
      }
    }
  }
  return cases;
};

/** Upstream's own fixtures (tests/fixtures/elementFixture.ts). */
const upstreamFixtureCases = (fixtures) => {
  const cases = [];
  for (const [name, element] of Object.entries(fixtures)) {
    cases.push({ id: `elementFixture/${name}`, element, renderConfig: RENDER.export });
  }
  // tests/scene/export.test.ts:32-60 resizes them to 100x100
  for (const [name, index] of [["diamondFixture", "a0"], ["ellipseFixture", "a1"], ["rectangleFixture", "a2"], ["embeddableFixture", "a3"]]) {
    cases.push({
      id: `export-test/${name}`,
      element: { ...fixtures[name], width: 100, height: 100, index },
      renderConfig: RENDER.export,
    });
  }
  return cases;
};

export const elementGoldens = (upstream) => [
  {
    name: "elements-upstream-fixtures.json",
    description:
      "Upstream's test fixtures (packages/excalidraw/tests/fixtures/elementFixture.ts) and the 100x100 variants used by tests/scene/export.test.ts, through upstream's ShapeCache.generateElementShape.",
    cases: upstreamFixtureCases(upstream.elementFixtures),
  },
  {
    name: "elements-rectangle.json",
    description: "Rectangles: seeds x roughness, fills, stroke styles, adjustRoughness sizes, corner radius, theme.",
    cases: closedShapeCases("rectangle"),
  },
  {
    name: "elements-diamond.json",
    description: "Diamonds: sharp polygons and rounded cubic-corner paths, fills, stroke styles, sizes, theme.",
    cases: closedShapeCases("diamond"),
  },
  {
    name: "elements-ellipse.json",
    description: "Ellipses (curveFitting 1): seeds x roughness, fills, stroke styles, sizes, theme.",
    cases: closedShapeCases("ellipse"),
  },
  {
    name: "elements-line.json",
    description: "Lines: linearPath, filled polygon loops, curves, polygon lines, stroke styles.",
    cases: lineCases(),
  },
  {
    name: "elements-arrow.json",
    description: "Arrows: linearPath and curve bodies with default heads, seeds x roughness, stroke styles.",
    cases: arrowCases(),
  },
  {
    name: "elements-arrowheads.json",
    description: "All fourteen arrowheads at start and end, stroke widths 1/2/4, curved, dashed, dotted, short, outline fills.",
    cases: arrowheadCases(),
  },
  {
    name: "elements-elbow-arrow.json",
    description: "Elbow arrows: generateElbowArrowShape paths (corner radius 16) and the extreme-coordinate guard.",
    cases: elbowArrowCases(),
  },
  {
    name: "elements-freedraw.json",
    description:
      "Freedraw: variable (perfect-freehand) and constant (laser-pointer) outlines, the trimmed SVG path, and rough fills for closed loops.",
    cases: freedrawCases(),
  },
  {
    name: "elements-iframe-like.json",
    description: "Iframes and embeddables: placeholder and default colours from modifyIframeLikeForRoughOptions.",
    cases: iframeLikeCases(),
  },
  {
    name: "elements-matrix.json",
    description:
      "The M2 matrix: every element type with a background colour at every fillStyle (hachure, cross-hatch, zigzag, solid) x roughness 0, 1, 2 x seed 1, 7, 1041657908. Rectangles sharp and at roundness 3, diamonds sharp and at roundness 2, ellipses, iframes, embeddables, line loops (sharp, curved) and polygon lines, arrows (sharp, curved) with a filled triangle end head, freedraw loops at variable and constant width; text, image, frame, magicframe and stickynote have no shape.",
    cases: matrixCases(),
  },
];

// -- raw rough.js generator calls ---------------------------------------------

const PATHS = {
  quad: "M 10 10 Q 90 10, 90 60 T 170 110",
  cubic: "M 0 0 C 40 -40, 120 40, 160 0 S 240 -40, 280 20",
  arcs: "M 20 80 A 45 45 0 0 1 110 80 L 160 140 a 30 20 20 1 0 40 -20 Z",
  relative: "m 10 10 h 120 v 60 l -60 30 z",
  smallArcs: "M 10 40 A 25 25 0 0 1 60 40 L 90 80 a 20 12 20 1 0 20 -10 Z",
};

// Fill and option cases use smaller shapes: fill ops grow with area / gap.
const SMALL = {
  rectangle: [10, 10, 100, 60],
  polygon: [[[0, 0], [80, 10], [90, 60], [15, 50]]],
  ellipse: [60, 40, 100, 60],
  curve: [[[0, 0], [30, 30], [60, 5], [100, 40], [130, 10]]],
  path: [PATHS.smallArcs],
};

const PRIMITIVES = [
  ["line", [10, 20, 220, 140]],
  ["rectangle", [10, 10, 200, 120]],
  ["polygon", [[[0, 0], [150, 20], [180, 120], [30, 100]]]],
  ["ellipse", [110, 70, 200, 120]],
  ["circle", [60, 60, 100]],
  ["arc", [100, 100, 160, 120, 0, Math.PI * 1.25, false]],
  ["arc", [100, 100, 160, 120, Math.PI / 4, Math.PI * 1.5, true]],
  ["curve", [[[0, 0], [60, 60], [120, 10], [200, 80], [260, 20]]]],
  ["linearPath", [[[0, 0], [60, 60], [120, 10], [200, 80]]]],
  ["path", [PATHS.quad]],
  ["path", [PATHS.cubic]],
  ["path", [PATHS.arcs]],
  ["path", [PATHS.relative]],
];

// Generator edge cases (ex-203): the parts of RoughGenerator, renderer.js and
// path-data-parser the primitives above do not reach. Stroke only; fills are
// in rough-fills.json.
const GENERATOR = [
  // path-data-parser: implicit repeats, missing leading M, compact numbers
  ["path/implicit-lineto", "path", ["M 0 0 40 30 80 0 L 120 40 160 0"]],
  ["path/implicit-relative", "path", ["m 10 10 20 20 20 -20 l 10 10 10 -10"]],
  ["path/no-leading-move", "path", ["L 50 50 100 20"]],
  ["path/compact-numbers", "path", ["M10-20l.5.5-3e1,4E+1L+60 1.e1"]],
  ["path/minus-space", "path", ["M 10 10 L - 20 30 L 40 -\t50"]],
  ["path/newlines", "path", ["M 0 0\nL 40 40\nL 80 0"]],
  ["path/absolute-hv", "path", ["M 0 0 H 50 V 50 H 0 Z"]],
  ["path/subpaths", "path", ["M 0 0 L 50 0 L 50 50 Z M 100 100 h 20 v 20 h -20 z L 10 90"]],
  ["path/close-then-draw", "path", ["M 10 10 L 60 10 L 60 60 Z L 90 90"]],
  // S and T with and without a preceding C / Q
  ["path/smooth-cubic-alone", "path", ["M 0 0 S 40 40 80 0"]],
  ["path/smooth-quad-alone", "path", ["M 0 0 T 50 50 T 100 0"]],
  ["path/quad-chain", "path", ["M 0 0 Q 25 50 50 0 T 100 0 T 150 0"]],
  ["path/relative-curves", "path", ["m 10 10 c 20 -20 40 20 60 0 s 40 -20 60 0 q 20 20 40 0 t 40 0"]],
  ["path/smooth-after-quad", "path", ["M 0 0 Q 30 40 60 0 S 100 -40 120 0"]],
  // arcs: zero radius, zero length, radii too small, > 120 degrees, rotation
  ["path/arc-zero-radius", "path", ["M 0 0 A 0 10 0 0 1 50 50"]],
  ["path/arc-same-point", "path", ["M 10 10 A 5 5 0 0 1 10 10 L 20 20"]],
  ["path/arc-scaled-radii", "path", ["M 0 0 A 5 5 0 0 0 100 0"]],
  ["path/arc-large", "path", ["M 0 50 A 50 50 0 1 1 100 50"]],
  ["path/arc-large-ccw", "path", ["M 0 50 A 50 50 0 1 0 100 50"]],
  ["path/arc-rotated", "path", ["M 0 0 a 40 20 45 0 1 80 40 a 40 20 -30 1 0 -60 30"]],
  ["path/arc-negative-radii", "path", ["M 0 0 A -30 -20 0 0 1 50 0"]],
  ["path/elbow", "path", ["M 0 0 L 84 0 Q 100 0, 100 16 L 100 84 Q 100 100, 116 100 L 200 100"]],
  ["path/empty", "path", [""]],
  ["path/move-only", "path", ["M 10 10"]],
  // simplification: stroke from pointsOnPath instead of svgPath
  ["path/simplification-0.5", "path", [PATHS.arcs], { simplification: 0.5 }],
  ["path/simplification-0.9", "path", [PATHS.cubic], { simplification: 0.9 }],
  ["path/simplification-subpaths", "path", ["M 0 0 L 50 0 L 50 50 Z M 100 100 Q 150 150 200 100"], { simplification: 0.2 }],
  ["path/simplification-1", "path", [PATHS.quad], { simplification: 1 }],
  ["path/simplification-0", "path", [PATHS.quad], { simplification: 0 }],
  // stroke "none": the outline is computed (and draws random numbers) but dropped
  ["none/line", "line", [0, 0, 100, 50], { stroke: "none" }],
  ["none/rectangle", "rectangle", [10, 10, 100, 60], { stroke: "none" }],
  ["none/ellipse", "ellipse", [60, 40, 100, 60], { stroke: "none" }],
  ["none/polygon", "polygon", [[[0, 0], [80, 10], [90, 60]]], { stroke: "none" }],
  ["none/curve", "curve", [[[0, 0], [30, 30], [60, 5]]], { stroke: "none" }],
  ["none/arc", "arc", [100, 100, 160, 120, 0, Math.PI, true], { stroke: "none" }],
  ["none/path", "path", [PATHS.cubic], { stroke: "none" }],
  // single stroke
  ["single/line", "line", [0, 0, 100, 50], { disableMultiStroke: true }],
  ["single/polygon", "polygon", [[[0, 0], [80, 10], [90, 60]]], { disableMultiStroke: true }],
  ["single/ellipse", "ellipse", [60, 40, 100, 60], { disableMultiStroke: true }],
  ["single/arc", "arc", [100, 100, 160, 120, 0, Math.PI, true], { disableMultiStroke: true }],
  ["single/curve", "curve", [[[0, 0], [30, 30], [60, 5], [100, 40]]], { disableMultiStroke: true }],
  ["single/path", "path", [PATHS.arcs], { disableMultiStroke: true }],
  // preserveVertices on lines and beziers
  ["vertices/line", "line", [0, 0, 100, 50], { preserveVertices: true }],
  ["vertices/linearPath", "linearPath", [[[0, 0], [60, 60], [120, 10]]], { preserveVertices: true }],
  ["vertices/path", "path", [PATHS.cubic], { preserveVertices: true }],
  // short point lists
  ["short/curve-4", "curve", [[[0, 0], [30, 30], [60, 5], [100, 40]]]],
  ["short/curve-3", "curve", [[[0, 0], [30, 30], [60, 5]]]],
  ["short/curve-2", "curve", [[[0, 0], [30, 30]]]],
  ["short/curve-1", "curve", [[[10, 10]]]],
  ["short/linearPath-2", "linearPath", [[[0, 0], [30, 30]]]],
  ["short/linearPath-1", "linearPath", [[[10, 10]]]],
  ["short/linearPath-0", "linearPath", [[]]],
  ["short/polygon-2", "polygon", [[[0, 0], [30, 30]]]],
  ["short/polygon-3", "polygon", [[[0, 0], [30, 30], [60, 0]]]],
  // _line's length bands: offset shrink below 20, gain 1 / interpolated / 0.4
  ["line/short", "line", [0, 0, 6, 8]],
  ["line/zero-length", "line", [5, 5, 5, 5]],
  ["line/mid", "line", [0, 0, 300, 200]],
  ["line/long", "line", [0, 0, 600, 300]],
  ["line/max-offset-0", "line", [0, 0, 100, 50], { maxRandomnessOffset: 0 }],
  ["path/max-offset-0", "path", [PATHS.cubic], { maxRandomnessOffset: 0 }],
  ["line/bowing-3", "line", [0, 0, 150, 40], { bowing: 3 }],
  // ellipse and arc geometry
  ["ellipse/negative-size", "ellipse", [60, 40, -100, -60]],
  ["ellipse/tiny", "ellipse", [10, 10, 4, 3]],
  ["ellipse/large", "ellipse", [400, 300, 800, 500]],
  ["ellipse/step-count-30", "ellipse", [60, 40, 100, 60], { curveStepCount: 30 }],
  ["ellipse/curve-fitting-1", "ellipse", [60, 40, 100, 60], { curveFitting: 1 }],
  ["circle/tiny", "circle", [10, 10, 2]],
  ["arc/negative-start", "arc", [100, 100, 160, 120, -Math.PI / 2, Math.PI / 3, false]],
  ["arc/over-full-turn", "arc", [100, 100, 160, 120, 0, Math.PI * 3, false]],
  ["arc/closed", "arc", [100, 100, 160, 120, Math.PI / 6, Math.PI * 1.75, true]],
  ["arc/tiny-span", "arc", [100, 100, 160, 120, 1, 1.05, false]],
  ["rectangle/negative-size", "rectangle", [110, 70, -100, -60]],
  // curve options
  ["curve/tightness-1", "curve", [[[0, 0], [30, 30], [60, 5], [100, 40], [130, 10]]], { curveTightness: 1 }],
  // seeds: the curve's second stroke uses seed + 1, which wraps past 2^31 - 1
  ["seed/curve-2147483647", "curve", [[[0, 0], [30, 30], [60, 5], [100, 40]]], { seed: 2147483647 }],
  ["seed/curve-negative", "curve", [[[0, 0], [30, 30], [60, 5], [100, 40]]], { seed: -5 }],
  ["seed/rectangle-negative", "rectangle", [10, 10, 100, 60], { seed: -123456 }],
];

const primitiveId = (method, args, i) => `${method}${method === "path" || method === "arc" ? `#${i}` : ""}`;

export const roughGoldens = () => {
  const primitives = [];
  PRIMITIVES.forEach(([method, args], i) => {
    for (const seed of SEEDS) {
      for (const roughness of ROUGHNESSES) {
        primitives.push({
          id: `${primitiveId(method, args, i)}/seed${seed}-r${roughness}`,
          method,
          args,
          options: { seed, roughness },
        });
      }
    }
  });

  const generator = [];
  for (const [id, method, args, extra = {}] of GENERATOR) {
    for (const roughness of ROUGHNESSES) {
      generator.push({
        id: `${id}/r${roughness}`,
        method,
        args,
        options: { seed: FIXTURE_SEED, roughness, ...extra },
      });
    }
  }

  const fills = [];
  const FILL_TARGETS = ["rectangle", "polygon", "ellipse", "path"].map((m) => [m, SMALL[m]]);
  for (const fillStyle of ROUGH_FILL_STYLES) {
    for (const sw of STROKE_WIDTHS) {
      for (const [method, args] of FILL_TARGETS) {
        fills.push({
          id: `${fillStyle}/${method}-sw${sw}`,
          method,
          args,
          options: {
            seed: FIXTURE_SEED,
            roughness: 1,
            strokeWidth: sw,
            fill: BACKGROUND,
            fillStyle,
            fillWeight: sw / 2,
            hachureGap: sw * 4,
          },
        });
      }
    }
    for (const seed of SEEDS) {
      for (const roughness of ROUGHNESSES) {
        fills.push({
          id: `${fillStyle}/rectangle-seed${seed}-r${roughness}`,
          method: "rectangle",
          args: SMALL.rectangle,
          options: { seed, roughness, fill: BACKGROUND, fillStyle, fillWeight: 1, hachureGap: 8 },
        });
      }
    }
  }

  // Fill edge cases (ex-204): the parts of the fillers, hachure-fill and the
  // generator's fill branches the grid above does not reach.
  const edge = (id, method, args, extra = {}) =>
    fills.push({
      id: `edge/${id}`,
      method,
      args,
      options: { seed: FIXTURE_SEED, roughness: 1, fill: BACKGROUND, fillWeight: 1, hachureGap: 8, ...extra },
    });
  const STAR = [[[50, 0], [62, 35], [100, 38], [70, 60], [80, 98], [50, 75], [20, 98], [30, 60], [0, 38], [38, 35]]];
  const TWO_TRIANGLES = "M 0 0 L 60 0 L 60 50 Z M 80 10 L 130 10 L 110 60 Z";
  const LONE_MOVE = "M 5 5 M 0 0 L 60 0 L 60 50 Z";
  for (const fillStyle of ROUGH_FILL_STYLES) {
    // curve: curveToBezier + pointsOnBezierCurves (pattern), or a single
    // merged stroke at roughness + fillShapeRoughnessGain (solid)
    edge(`curve-${fillStyle}`, "curve", SMALL.curve, { fillStyle });
    edge(`curve-3-${fillStyle}`, "curve", [[[0, 0], [60, 60], [120, 0]]], { fillStyle });
    // closed arc: patternFillArc, or a single-stroke arc without rough closure
    edge(`arc-${fillStyle}`, "arc", [100, 100, 160, 120, Math.PI / 6, Math.PI * 1.75, true], { fillStyle });
    // several subpaths: solidFillPolygon over pointsOnPath's sets
    edge(`path-subpaths-${fillStyle}`, "path", [TWO_TRIANGLES], { fillStyle });
    // concave polygon: several active edge pairs per scan line
    edge(`polygon-star-${fillStyle}`, "polygon", STAR, { fillStyle });
    // hachure-fill skips the rotation when hachureAngle + 90 is 0; x.5 edges
    // exercise Math.round
    edge(`rectangle-angle-90-${fillStyle}`, "rectangle", [10.5, -20.5, 60, 41], { fillStyle, hachureAngle: -90 });
    // hachureGap < 0: the gap is strokeWidth * 4
    edge(`rectangle-default-gap-${fillStyle}`, "rectangle", SMALL.rectangle, { fillStyle, strokeWidth: 2, hachureGap: -1 });
    // single fill stroke
    edge(`ellipse-single-fill-${fillStyle}`, "ellipse", SMALL.ellipse, { fillStyle, disableMultiStrokeFill: true });
    // roughness 0: no skipOffset draw, core ellipse points
    edge(`ellipse-r0-${fillStyle}`, "ellipse", SMALL.ellipse, { fillStyle, roughness: 0 });
    edge(`curve-r0-${fillStyle}`, "curve", SMALL.curve, { fillStyle, roughness: 0 });
    // roughness 2
    edge(`polygon-r2-${fillStyle}`, "polygon", SMALL.polygon, { fillStyle, roughness: 2 });
    // stroke "none": the outline still draws first
    edge(`polygon-no-stroke-${fillStyle}`, "polygon", SMALL.polygon, { fillStyle, stroke: "none" });
  }
  // gap below the 0.1 floor
  edge("rectangle-tiny-gap-hachure", "rectangle", [0, 0, 2, 2], { fillStyle: "hachure", hachureGap: 0.05 });
  edge("rectangle-tiny-gap-zigzag", "rectangle", [0, 0, 2, 2], { fillStyle: "zigzag", hachureGap: 0.05 });
  // a two-point polygon (closed by hachure-fill) and a polygon already closed
  edge("polygon-2-hachure", "polygon", [[[0, 0], [60, 40]]], { fillStyle: "hachure" });
  edge("polygon-2-solid", "polygon", [[[0, 0], [60, 40]]], { fillStyle: "solid" });
  edge("polygon-closed-hachure", "polygon", [[[0, 0], [80, 10], [40, 60], [0, 0]]], { fillStyle: "hachure" });
  // solid fill on a one-set path goes through svgPath; fillShapeRoughnessGain 0
  edge("path-solid-gain-0", "path", [PATHS.smallArcs], { fillStyle: "solid", fillShapeRoughnessGain: 0 });
  edge("curve-solid-gain-0", "curve", SMALL.curve, { fillStyle: "solid", fillShapeRoughnessGain: 0 });
  edge("path-solid-r0", "path", [PATHS.smallArcs], { fillStyle: "solid", roughness: 0 });
  // a lone moveto: pointsOnPath gives a one-point set, simplify doubles it
  // (the same point object twice), and the simplified stroke draws from it
  // after the fill has rotated it in place
  edge("path-lone-move-simplified", "path", [LONE_MOVE], { fillStyle: "hachure", simplification: 0.5 });
  edge("path-lone-move-cross-hatch", "path", [LONE_MOVE], { fillStyle: "cross-hatch", simplification: 0.5 });
  edge("path-move-only-solid", "path", ["M 10 10"], { fillStyle: "solid" });
  // (at roughness 0: before any draw there is no randomizer, and at
  // roughness >= 1 polygonHachureLines would fall back to Math.random)
  edge("path-move-only-hachure", "path", ["M 10 10"], { fillStyle: "hachure", roughness: 0 });
  // simplified stroke after a fill that rotated the sets in place
  edge("path-simplified-hachure", "path", [PATHS.smallArcs], { fillStyle: "hachure", simplification: 0.5 });
  edge("path-simplified-solid", "path", [TWO_TRIANGLES], { fillStyle: "solid", simplification: 0.5 });
  // fill "none" still fills rectangles, polygons, ellipses and arcs (only
  // curve and path test for it); negative seeds; angles that are not
  // multiples of 90
  edge("rectangle-fill-none", "rectangle", SMALL.rectangle, { fillStyle: "hachure", fill: "none" });
  edge("rectangle-seed-negative", "rectangle", SMALL.rectangle, { fillStyle: "cross-hatch", seed: -123456 });
  edge("ellipse-angle-17-dashed", "ellipse", SMALL.ellipse, { fillStyle: "dashed", hachureAngle: 17 });
  edge("ellipse-angle-17-zigzag-line", "ellipse", SMALL.ellipse, { fillStyle: "zigzag-line", hachureAngle: 17 });
  edge("polygon-zigzag-offset-3", "polygon", SMALL.polygon, { fillStyle: "zigzag-line", zigzagOffset: 3 });
  edge("polygon-dash-6-4", "polygon", SMALL.polygon, { fillStyle: "dashed", dashOffset: 6, dashGap: 4 });
  // an unknown fillStyle falls back to hachure (fillers/filler.js default)
  edge("rectangle-unknown-style", "rectangle", SMALL.rectangle, { fillStyle: "sketchy" });
  // no fill: too few curve points, an open arc, a transparent or "none" path
  edge("nofill/curve-2", "curve", [[[0, 0], [30, 30]]], { fillStyle: "hachure" });
  edge("nofill/arc-open", "arc", [100, 100, 160, 120, 0, Math.PI, false], { fillStyle: "solid" });
  edge("nofill/path-transparent", "path", [PATHS.smallArcs], { fillStyle: "hachure", fill: "transparent" });
  edge("nofill/path-none", "path", [PATHS.smallArcs], { fillStyle: "hachure", fill: "none" });
  edge("nofill/curve-none", "curve", SMALL.curve, { fillStyle: "hachure", fill: "none" });

  const options = [];
  const variants = {
    disableMultiStroke: [true, false],
    disableMultiStrokeFill: [true, false],
    preserveVertices: [true, false],
    curveFitting: [0.95, 1],
    strokeWidth: [1.5, 2.5, 4.5],
    bowing: [0, 1, 5],
    curveTightness: [0, 0.5],
    curveStepCount: [9, 20],
    maxRandomnessOffset: [2, 6],
    hachureAngle: [-41, 0, 60],
    fillShapeRoughnessGain: [0.8, 0],
    strokeLineDash: [[8, 9], [1.5, 7]],
    dashOffset: [-1, 6],
    dashGap: [-1, 10],
    zigzagOffset: [-1, 5],
  };
  const OPTION_TARGETS = ["rectangle", "ellipse", "curve", "path"].map((m) => [m, SMALL[m]]);
  for (const [key, values] of Object.entries(variants)) {
    for (const value of values) {
      for (const [method, args] of OPTION_TARGETS) {
        const fillStyle = key.startsWith("dash") ? "dashed" : key === "zigzagOffset" ? "zigzag-line" : "hachure";
        options.push({
          id: `${key}=${JSON.stringify(value)}/${method}`,
          method,
          args,
          options: {
            seed: FIXTURE_SEED,
            roughness: 1,
            fill: method === "curve" ? undefined : BACKGROUND,
            fillStyle,
            fillWeight: 1,
            hachureGap: 8,
            [key]: value,
          },
        });
      }
    }
  }
  // Excalidraw's non-solid stroke rule: disableMultiStroke with sw + 0.5
  for (const [style, dash] of [["dashed", [8, 8 + 2]], ["dotted", [1.5, 6 + 2]]]) {
    for (const [method, args] of OPTION_TARGETS) {
      options.push({
        id: `excalidraw-${style}/${method}`,
        method,
        args,
        options: {
          seed: FIXTURE_SEED,
          roughness: 1,
          strokeWidth: 2.5,
          disableMultiStroke: true,
          strokeLineDash: dash,
          fillWeight: 1,
          hachureGap: 8,
        },
      });
    }
  }
  const strip = (cs) =>
    cs.map((c) => ({ ...c, options: Object.fromEntries(Object.entries(c.options).filter(([, v]) => v !== undefined)) }));

  return [
    {
      name: "rough-primitives.json",
      description:
        "rough.js 4.6.4 RoughGenerator primitives (line, rectangle, polygon, ellipse, circle, arc, curve, linearPath, path) at seeds 1, 7, 1041657908 and roughness 0, 1, 2.",
      cases: strip(primitives),
    },
    {
      name: "rough-generator.json",
      description:
        "rough.js 4.6.4 RoughGenerator edge cases at roughness 0, 1, 2: SVG path syntax (path-data-parser 0.1.0), simplification (points-on-path 0.2.1), stroke none, single stroke, preserveVertices, short point lists, line length bands, ellipse and arc geometry, seed wrap-around.",
      cases: strip(generator),
    },
    {
      name: "rough-fills.json",
      description:
        "rough.js 4.6.4 fill styles on rectangle, polygon, ellipse and path with Excalidraw's fillWeight = sw/2 and hachureGap = sw*4, plus seeds x roughness, and fill edge cases (curve and arc fills, subpaths, concave polygons, gaps, angles, single fill stroke, fill none/transparent).",
      cases: strip(fills),
    },
    {
      name: "rough-options.json",
      description:
        "rough.js 4.6.4 option variations (multi-stroke, preserveVertices, curve fitting, bowing, dashes, hachure angle) and Excalidraw's dashed/dotted stroke rule.",
      cases: strip(options),
    },
  ];
};

// -- stroke styles (ex-205) -----------------------------------------------------
//
// Excalidraw's stroke-style rule (shape.ts:168-170, :202-216): dashed strokes
// carry strokeLineDash [8, 8 + sw], dotted [1.5, 6 + sw], and every non-solid
// stroke is drawn single-stroke (disableMultiStroke) at sw + 0.5, while
// fillWeight (sw / 2) and hachureGap (sw * 4) stay on the element's own
// width. preserveVertices is on below cartoonist roughness or for a
// continuous path (:224-225); curveFitting is 1 for ellipses (:238-240).
//
// Each case is one rough.js generator call made the way upstream's shape
// builders make it: the options are upstream's own
// generateRoughOptions(element, continuousPath) (generate.mjs), and the
// method is the one that element type is drawn with (shape.ts:787-975):
// rectangle; polygon (diamond, filled sharp loop); path with continuousPath
// (rounded rectangle, elbow arrow); ellipse; linearPath and curve (lines and
// arrows); line and circle (arrowheads). The grid is solid, dashed and
// dotted x sw 1, 2, 4 x roughness 0, 1, 2 at the fixture seed, plus seeds 1
// and 7 at sw 2 for every roughness.

const STROKE_STYLES = ["solid", "dashed", "dotted"];
const STROKE_SIZE = { width: 100, height: 60 };
const STROKE_ZIGZAG = [[0, 0], [30, 20], [60, -5], [100, 25]];
const STROKE_LOOP = [[0, 0], [60, -10], [90, 40], [20, 55], [2, 2]]; // closes within 8px
const ROUNDED_RECT =
  "M 15 0 L 85 0 Q 100 0, 100 15 L 100 45 Q 100 60, 85 60 L 15 60 Q 0 60, 0 45 L 0 15 Q 0 0, 15 0";
const ELBOW = "M 0 0 L 84 0 Q 100 0, 100 16 L 100 84 Q 100 100, 116 100 L 200 100";
const FILLED = (fillStyle) => ({ fillStyle, backgroundColor: BACKGROUND });

/** [target, element type, element props, continuousPath, method, args] */
const STROKE_TARGETS = [
  ["rectangle", "rectangle", FILLED("hachure"), false, "rectangle", [0, 0, 100, 60]],
  ["rectangle-solid", "rectangle", FILLED("solid"), false, "rectangle", [0, 0, 100, 60]],
  ["rectangle-rounded", "rectangle", { ...FILLED("cross-hatch"), roundness: { type: 3 } }, true, "path", [ROUNDED_RECT]],
  ["diamond", "diamond", FILLED("zigzag"), false, "polygon", [[[51, 0], [100, 31], [51, 60], [0, 31]]]],
  ["ellipse", "ellipse", FILLED("hachure"), false, "ellipse", [50, 30, 100, 60]],
  ["ellipse-solid", "ellipse", FILLED("solid"), false, "ellipse", [50, 30, 100, 60]],
  ["line-sharp", "line", { points: STROKE_ZIGZAG }, false, "linearPath", [STROKE_ZIGZAG]],
  ["line-round", "line", { points: STROKE_ZIGZAG, roundness: { type: 2 } }, false, "curve", [STROKE_ZIGZAG]],
  ["line-loop-solid", "line", { points: STROKE_LOOP, roundness: { type: 2 }, ...FILLED("solid") }, false, "curve", [STROKE_LOOP]],
  ["line-loop-hachure", "line", { points: STROKE_LOOP, ...FILLED("hachure") }, false, "polygon", [STROKE_LOOP]],
  ["arrow-round", "arrow", { points: STROKE_ZIGZAG, roundness: { type: 2 } }, false, "curve", [STROKE_ZIGZAG]],
  ["arrow-elbow", "arrow", { points: [[0, 0], [200, 100]], elbowed: true }, true, "path", [ELBOW]],
  ["arrowhead-line", "arrow", { points: STROKE_ZIGZAG }, false, "line", [100, 25, 82, 12]],
  ["arrowhead-circle", "arrow", { points: STROKE_ZIGZAG }, false, "circle", [100, 25, 12]],
];

const strokeElement = (type, props) =>
  props.points ? linear("", type, props.points, props) : base("", type, { ...STROKE_SIZE, ...props });

/** { id, element, continuousPath, method, args }; generate.mjs adds options and the drawable. */
export const strokeCases = () => {
  const cases = [];
  const add = (style, sw, roughness, seed) => {
    const variant = `sw${sw}-r${roughness}${seed === FIXTURE_SEED ? "" : `-seed${seed}`}`;
    for (const [target, type, props, continuousPath, method, args] of STROKE_TARGETS) {
      const id = `${style}/${variant}/${target}`;
      const element = {
        ...strokeElement(type, props),
        id: id.replace(/[^A-Za-z0-9_-]/g, "_"),
        strokeStyle: style,
        strokeWidth: sw,
        roughness,
        seed,
      };
      cases.push({ id, element, continuousPath, method, args });
    }
  };
  for (const style of STROKE_STYLES) {
    for (const sw of STROKE_WIDTHS) {
      for (const roughness of ROUGHNESSES) add(style, sw, roughness, FIXTURE_SEED);
    }
    for (const seed of [1, 7]) {
      for (const roughness of ROUGHNESSES) add(style, 2, roughness, seed);
    }
  }
  return cases;
};

export const RANDOM_SEEDS = [1, 7, 42, 48271, 1041657908, 2147483646, 2147483647];
