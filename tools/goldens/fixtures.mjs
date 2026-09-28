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
  return cases;
};

/** Named easings used by the direct perfect-freehand cases. */
export const EASINGS = {
  easeOutSine: (t) => Math.sin((t * Math.PI) / 2), // shape.ts:1241
  linear: (t) => t,
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
        "rough.js 4.6.4 fill styles on rectangle, polygon, ellipse and path with Excalidraw's fillWeight = sw/2 and hachureGap = sw*4, plus seeds x roughness.",
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

export const RANDOM_SEEDS = [1, 7, 42, 48271, 1041657908, 2147483646, 2147483647];
