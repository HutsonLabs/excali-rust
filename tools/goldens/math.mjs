// Inputs for goldens/math.json: calls into upstream's packages/math/src
// (every exported function except curve.ts and pca.ts, which are ex-202 and
// shape recognition). Only inputs live here; every result is what upstream's
// own function returns for them (generate.mjs).
//
// Each case is { id, fn, args }: the call was `math[fn](...args)`. Points,
// vectors, segments, lines, triangles, rectangles and ranges are the plain
// arrays upstream uses; an ellipse is upstream's { center, halfWidth,
// halfHeight } object. Random inputs come from a fixed Park-Miller sequence
// (Math.random is disabled while generating), so the file is byte-stable.

/** Park-Miller minimal standard, 48271 (the same generator as rough.js). */
const rng = (seed) => {
  let s = seed;
  return () => {
    s = Math.imul(48271, s) & 0x7fffffff;
    return s / 2 ** 31;
  };
};

/** The functions math.json covers, by upstream source file. */
export const MATH_FUNCTIONS = {
  "utils.ts": ["clamp", "round", "roundToStep", "average", "isFiniteNumber", "isCloseTo"],
  "angle.ts": [
    "normalizeRadians",
    "cartesian2Polar",
    "degreesToRadians",
    "radiansToDegrees",
    "isRightAngleRads",
    "radiansBetweenAngles",
    "radiansDifference",
  ],
  "point.ts": [
    "pointFrom",
    "pointFromArray",
    "pointFromPair",
    "pointFromVector",
    "isPoint",
    "pointsEqual",
    "pointRotateRads",
    "pointRotateDegs",
    "pointTranslate",
    "pointCenter",
    "pointDistance",
    "pointDistanceSq",
    "pointScaleFromOrigin",
    "isPointWithinBounds",
    "isValidPoint",
  ],
  "vector.ts": [
    "vector",
    "vectorFromPoint",
    "vectorCross",
    "vectorDot",
    "isVector",
    "vectorAdd",
    "vectorSubtract",
    "vectorScale",
    "vectorMagnitudeSq",
    "vectorMagnitude",
    "vectorNormalize",
    "vectorNormal",
  ],
  "line.ts": ["line", "linesIntersectAt"],
  "segment.ts": [
    "lineSegment",
    "isLineSegment",
    "lineSegmentRotate",
    "segmentsIntersectAt",
    "pointOnLineSegment",
    "distanceToLineSegment",
    "lineSegmentPointAt",
    "lineSegmentIntersectionPoints",
    "lineSegmentsDistance",
    "lineSegmentClosestParameter",
  ],
  "ellipse.ts": [
    "ellipse",
    "ellipseIncludesPoint",
    "ellipseTouchesPoint",
    "ellipseDistanceFromPoint",
    "ellipseSegmentInterceptPoints",
    "ellipseLineIntersectionPoints",
  ],
  "polygon.ts": [
    "polygon",
    "polygonFromPoints",
    "polygonIncludesPoint",
    "polygonIncludesPointNonZero",
    "polygonIsClosed",
    "polygonSignedArea",
    "polygonArea",
    "convexHull",
    "simplifyConvexPolygon",
  ],
  "range.ts": ["rangeInclusive", "rangeInclusiveFromPair", "rangesOverlap", "rangeIntersection", "rangeIncludesValue"],
  "rectangle.ts": [
    "rectangle",
    "rectangleFromNumberSequence",
    "rectangleIntersectLineSegment",
    "rectangleIntersectRectangle",
  ],
  "triangle.ts": ["triangleIncludesPoint"],
};

const RANDOM_PER_FUNCTION = 24;

export const mathCases = () => {
  const next = rng(20260928);
  // Coordinates with a few decimals and full-precision ones, both signs.
  const num = (scale = 200) => {
    const v = (next() * 2 - 1) * scale;
    return next() < 0.5 ? Math.round(v * 100) / 100 : v;
  };
  const pos = (scale = 100) => 0.5 + next() * scale;
  const pt = (scale) => [num(scale), num(scale)];
  const seg = (scale) => [pt(scale), pt(scale)];
  const angle = () => (next() * 2 - 1) * 4 * Math.PI;
  const ell = () => ({ center: pt(50), halfWidth: pos(), halfHeight: pos() });
  const ngon = (n, r, jitter) =>
    Array.from({ length: n }, (_, i) => {
      const a = (i / n) * Math.PI * 2;
      const rr = r + (next() * 2 - 1) * jitter;
      return [Math.cos(a) * rr, Math.sin(a) * rr];
    });
  const cloud = (n, scale) => Array.from({ length: n }, () => pt(scale));

  const cases = [];
  const counts = new Map();
  const add = (fn, ...args) => {
    const n = counts.get(fn) ?? 0;
    counts.set(fn, n + 1);
    cases.push({ id: `${fn}/${n}`, fn, args });
  };
  const repeat = (fn, make) => {
    for (let i = 0; i < RANDOM_PER_FUNCTION; i++) add(fn, ...make());
  };

  // utils.ts
  add("clamp", 5, 0, 10);
  add("clamp", -5, 0, 10);
  add("clamp", 15, 0, 10);
  repeat("clamp", () => [num(), -50, 50]);
  for (const func of ["round", "floor", "ceil"]) {
    for (const v of [1.005, 2.5, -2.5, 0.49999999999999994, -0.4, 1234.5678, 0.1 + 0.2]) {
      add("round", v, 2, func);
      add("round", v, 0, func);
    }
  }
  repeat("round", () => [num(1000), Math.floor(next() * 6), "round"]);
  for (const func of ["round", "floor", "ceil"]) {
    add("roundToStep", 13, 5, func);
    add("roundToStep", -13, 5, func);
    add("roundToStep", 0.37, 0.1, func);
  }
  repeat("roundToStep", () => [num(500), [1, 5, 10, 20, 0.1, 0.25][Math.floor(next() * 6)], "round"]);
  repeat("average", () => [num(), num()]);
  add("isCloseTo", 1, 1.00001);
  add("isCloseTo", 1, 1.001);
  add("isCloseTo", 1, 1.1, 0.2);
  repeat("isCloseTo", () => {
    const a = num();
    return [a, a + (next() - 0.5) * 4e-4];
  });

  // Values of every JSON kind for the shape predicates (NaN and undefined
  // cannot be written to JSON; the Rust unit tests cover them).
  const shapes = [null, 3, "3", true, [], [1], [1, 2], [1, "2"], ["1", 2], [1, 2, 3], [[1, 2]], { x: 1, y: 2 }];
  for (const v of [...shapes, -1e308, 0.5]) add("isFiniteNumber", v);

  // angle.ts
  for (const a of [0, Math.PI, 2 * Math.PI, -Math.PI, -2 * Math.PI, 7, -7]) add("normalizeRadians", a);
  repeat("normalizeRadians", () => [angle()]);
  for (const p of [[1, 0], [0, 1], [-1, 0], [0, -1], [0, 0], [3, 4], [-3, -4]]) add("cartesian2Polar", p);
  repeat("cartesian2Polar", () => [pt()]);
  for (const d of [0, 45, 90, 180, 270, 360, -90]) add("degreesToRadians", d);
  repeat("degreesToRadians", () => [num(720)]);
  for (const r of [0, Math.PI / 2, Math.PI, -Math.PI / 4]) add("radiansToDegrees", r);
  repeat("radiansToDegrees", () => [angle()]);
  for (const r of [0, Math.PI / 2, Math.PI, (3 * Math.PI) / 2, Math.PI / 4, 1e-5, 0.1]) add("isRightAngleRads", r);
  repeat("isRightAngleRads", () => [angle()]);
  add("radiansBetweenAngles", 1, 0, 2);
  add("radiansBetweenAngles", 3, 0, 2);
  add("radiansBetweenAngles", 0.1, 6, 0.5);
  add("radiansBetweenAngles", 5, 6, 0.5);
  add("radiansBetweenAngles", -0.1, 6, 0.5);
  repeat("radiansBetweenAngles", () => [angle(), angle(), angle()]);
  add("radiansDifference", 0.1, 6.2);
  add("radiansDifference", 6.2, 0.1);
  add("radiansDifference", 1, 2);
  add("radiansDifference", 0, Math.PI);
  repeat("radiansDifference", () => [angle(), angle()]);

  // point.ts
  add("pointFrom", 1, 2);
  add("pointFrom", { x: 3, y: 4 });
  repeat("pointFrom", () => [num(), num()]);
  for (const a of [[1, 2], [1], [1, 2, 3], []]) add("pointFromArray", a);
  add("pointFromPair", [5, 6]);
  for (const v of shapes) {
    add("isPoint", v);
    add("isValidPoint", v);
    add("isVector", v);
  }
  add("pointFromVector", [3, 4]);
  repeat("pointFromVector", () => [pt(), pt()]);
  add("pointsEqual", [1, 1], [1.00001, 1]);
  add("pointsEqual", [1, 1], [1.001, 1]);
  add("pointsEqual", [1, 1], [1.5, 1.5], 1);
  repeat("pointsEqual", () => {
    const p = pt();
    return [p, [p[0] + (next() - 0.5) * 4e-4, p[1] + (next() - 0.5) * 4e-4]];
  });
  add("pointRotateRads", [10, 20], [20, 30], Math.PI / 2);
  add("pointRotateRads", [30, 20], [20, 30], -Math.PI / 2);
  add("pointRotateRads", [5, 7], [1, 1], 0);
  repeat("pointRotateRads", () => [pt(), pt(), angle()]);
  add("pointRotateDegs", [10, 20], [20, 30], 90);
  add("pointRotateDegs", [5, 7], [1, 1], 0);
  repeat("pointRotateDegs", () => [pt(), pt(), num(720)]);
  add("pointTranslate", [1, 2]);
  repeat("pointTranslate", () => [pt(), pt()]);
  repeat("pointCenter", () => [pt(), pt()]);
  add("pointDistance", [0, 0], [3, 4]);
  add("pointDistance", [1, 1], [1, 1]);
  add("pointDistance", [0, 0], [1e-200, 1e-200]);
  add("pointDistance", [0, 0], [1e200, 1e200]);
  repeat("pointDistance", () => [pt(), pt()]);
  repeat("pointDistanceSq", () => [pt(), pt()]);
  repeat("pointScaleFromOrigin", () => [pt(), pt(), num(4)]);
  add("isPointWithinBounds", [0, 0], [5, 5], [10, 10]);
  add("isPointWithinBounds", [10, 10], [5, 5], [0, 0]);
  add("isPointWithinBounds", [0, 0], [0, 0], [10, 10]);
  add("isPointWithinBounds", [0, 0], [11, 5], [10, 10]);
  repeat("isPointWithinBounds", () => [pt(), pt(), pt()]);

  // vector.ts
  add("vector", 3, 4);
  repeat("vector", () => [num(), num(), num(), num()]);
  add("vectorFromPoint", [3, 4]);
  add("vectorFromPoint", [3, 4], [1, 1]);
  add("vectorFromPoint", [1.5, 1], [1, 1], 1);
  add("vectorFromPoint", [1.5, 1], [1, 1], 1, [7, 8]);
  add("vectorFromPoint", [3, 1], [1, 1], 1, [7, 8]);
  add("vectorFromPoint", [1, 1], [1, 1], 0);
  repeat("vectorFromPoint", () => [pt(), pt(), pos(200), pt()]);
  repeat("vectorCross", () => [pt(), pt()]);
  repeat("vectorDot", () => [pt(), pt()]);
  repeat("vectorAdd", () => [pt(), pt()]);
  repeat("vectorSubtract", () => [pt(), pt()]);
  repeat("vectorScale", () => [pt(), num(10)]);
  repeat("vectorMagnitudeSq", () => [pt()]);
  add("vectorMagnitude", [3, 4]);
  repeat("vectorMagnitude", () => [pt()]);
  add("vectorNormalize", [0, 0]);
  add("vectorNormalize", [3, 4]);
  repeat("vectorNormalize", () => [pt()]);
  repeat("vectorNormal", () => [pt()]);

  // line.ts
  add("line", [1, 2], [3, 4]);
  add("linesIntersectAt", [[-5, -5], [5, 5]], [[5, -5], [-5, 5]]);
  add("linesIntersectAt", [[0, 0], [10, 10]], [[10, 0], [0, 10]]);
  add("linesIntersectAt", [[0, 0], [0, 10]], [[10, 0], [10, 10]]);
  add("linesIntersectAt", [[0, 0], [10, 10]], [[1, 1], [5, 5]]);
  repeat("linesIntersectAt", () => [seg(), seg()]);

  // segment.ts
  add("lineSegment", [1, 2], [3, 4]);
  for (const v of [...shapes, [[0, 0], [1, 1]], [[0, 0], "not-a-point"], [[0, 0]], [[0, 0], [1, 1], [2, 2]], [[0, 0], [1, "1"]]]) {
    add("isLineSegment", v);
  }
  add("lineSegmentRotate", [[0, 0], [10, 0]], Math.PI / 2);
  add("lineSegmentRotate", [[0, 0], [10, 0]], Math.PI / 2, [0, 0]);
  add("lineSegmentRotate", [[0, 0], [10, 0]], 0);
  repeat("lineSegmentRotate", () => [seg(), angle(), pt()]);
  repeat("lineSegmentRotate", () => [seg(), angle()]);
  add("segmentsIntersectAt", [[0, 0], [10, 10]], [[10, 0], [0, 10]]);
  add("segmentsIntersectAt", [[0, 0], [10, 0]], [[0, 5], [10, 5]]);
  add("segmentsIntersectAt", [[0, 0], [10, 0]], [[5, 0], [5, 10]]);
  add("segmentsIntersectAt", [[0, 0], [10, 0]], [[10, -5], [10, 5]]);
  add("segmentsIntersectAt", [[0, 0], [10, 0]], [[0, -5], [0, 5]]);
  add("segmentsIntersectAt", [[0, 0], [10, 0]], [[5, 5], [5, 1]]);
  repeat("segmentsIntersectAt", () => [seg(), seg()]);
  add("pointOnLineSegment", [5, 0], [[0, 0], [10, 0]]);
  add("pointOnLineSegment", [5, 0.00001], [[0, 0], [10, 0]]);
  add("pointOnLineSegment", [5, 1], [[0, 0], [10, 0]]);
  add("pointOnLineSegment", [5, 1], [[0, 0], [10, 0]], 2);
  add("pointOnLineSegment", [11, 0], [[0, 0], [10, 0]]);
  repeat("pointOnLineSegment", () => {
    const s = seg();
    const t = next();
    return [[s[0][0] + t * (s[1][0] - s[0][0]), s[0][1] + t * (s[1][1] - s[0][1])], s];
  });
  repeat("pointOnLineSegment", () => [pt(), seg(), pos(50)]);
  add("distanceToLineSegment", [5, 5], [[0, 0], [10, 0]]);
  add("distanceToLineSegment", [-3, 4], [[0, 0], [10, 0]]);
  add("distanceToLineSegment", [3, 4], [[0, 0], [0, 0]]);
  repeat("distanceToLineSegment", () => [pt(), seg()]);
  add("lineSegmentPointAt", [[0, 0], [10, 20]], 0.5);
  add("lineSegmentPointAt", [[0, 0], [10, 20]], 1.5);
  repeat("lineSegmentPointAt", () => [seg(), num(2)]);
  add("lineSegmentIntersectionPoints", [[0, 0], [5, 0]], [[2, -2], [3, 2]]);
  add("lineSegmentIntersectionPoints", [[0, 0], [5, 0]], [[3, 1], [4, 4]]);
  add("lineSegmentIntersectionPoints", [[0, 0], [5, 0]], [[0, 1], [5, 1]]);
  add("lineSegmentIntersectionPoints", [[0, 0], [5, 0]], [[6, -1], [6, 1]], 1.5);
  repeat("lineSegmentIntersectionPoints", () => [seg(), seg()]);
  repeat("lineSegmentIntersectionPoints", () => [seg(50), seg(50), pos(5)]);
  add("lineSegmentsDistance", [[0, 0], [10, 0]], [[0, 5], [10, 5]]);
  add("lineSegmentsDistance", [[0, 0], [10, 10]], [[10, 0], [0, 10]]);
  repeat("lineSegmentsDistance", () => [seg(), seg()]);
  add("lineSegmentClosestParameter", [5, 5], [[0, 0], [10, 0]]);
  add("lineSegmentClosestParameter", [-5, 5], [[0, 0], [10, 0]]);
  add("lineSegmentClosestParameter", [15, 5], [[0, 0], [10, 0]]);
  add("lineSegmentClosestParameter", [15, 5], [[1, 1], [1, 1]]);
  repeat("lineSegmentClosestParameter", () => [pt(), seg()]);

  // ellipse.ts (the upstream test's ellipses first)
  add("ellipse", [1, 2], 3, 4);
  const e121 = { center: [1, 2], halfWidth: 2, halfHeight: 1 };
  for (const p of [[1, 3], [1, 1], [3, 2], [-1, 2], [0, 2.8], [2, 1.2], [-0.4, 2.7], [2, 1.14]]) {
    add("ellipseTouchesPoint", p, e121);
    add("ellipseTouchesPoint", p, e121, 0.1);
  }
  repeat("ellipseTouchesPoint", () => [pt(100), ell(), pos(10)]);
  const e021 = { center: [0, 0], halfWidth: 2, halfHeight: 1 };
  for (const p of [[0, 1], [0, -1], [2, 0], [-2, 0], [-1, 0.8], [1, -0.8], [-1, 1], [-1.4, 0.8]]) {
    add("ellipseIncludesPoint", p, e021);
  }
  repeat("ellipseIncludesPoint", () => [pt(100), ell()]);
  const circle = { center: [0, 0], halfWidth: 100, halfHeight: 100 };
  const wide = { center: [0, 0], halfWidth: 200, halfHeight: 50 };
  for (const p of [[0, 0], [1, 0], [30, 40]]) add("ellipseDistanceFromPoint", p, circle);
  for (const p of [[0, 0], [10, 0], [-150, 0], [0, 10], [250, 0], [0, -60], [150, 40]]) {
    add("ellipseDistanceFromPoint", p, wide);
  }
  repeat("ellipseDistanceFromPoint", () => [pt(150), ell()]);
  const e22 = { center: [0, 0], halfWidth: 2, halfHeight: 2 };
  add("ellipseSegmentInterceptPoints", e22, [[-100, 0], [-10, 0]]);
  add("ellipseSegmentInterceptPoints", e22, [[-10, 0], [10, 0]]);
  add("ellipseSegmentInterceptPoints", e22, [[-10, -2], [10, -2]]);
  add("ellipseSegmentInterceptPoints", e22, [[0, -1], [0, 1]]);
  add("ellipseSegmentInterceptPoints", e22, [[0, 0], [0, 10]]);
  repeat("ellipseSegmentInterceptPoints", () => [ell(), seg(150)]);
  add("ellipseLineIntersectionPoints", e22, [[-10, -10], [10, -10]]);
  add("ellipseLineIntersectionPoints", e22, [[0, -1], [0, 1]]);
  add("ellipseLineIntersectionPoints", e22, [[-100, 0], [-10, 0]]);
  add("ellipseLineIntersectionPoints", e22, [[-2, -2], [2, -2]]);
  repeat("ellipseLineIntersectionPoints", () => [ell(), seg(150)]);

  // polygon.ts
  const square = [[0, 0], [10, 0], [10, 10], [0, 10]];
  add("polygon", ...square);
  add("polygon", ...square, [0, 0]);
  add("polygon", ...square, [0.00001, 0]);
  add("polygonFromPoints", square);
  add("polygonFromPoints", [...square, [0, 0]]);
  repeat("polygonFromPoints", () => [cloud(5, 100)]);
  const closedSquare = [...square, [0, 0]];
  for (const p of [[5, 5], [0, 5], [10, 5], [5, 0], [5, 10], [11, 5], [-1, -1], [0, 0], [10, 10]]) {
    add("polygonIncludesPoint", p, closedSquare);
    add("polygonIncludesPointNonZero", p, square);
  }
  const bowtie = [[0, 0], [10, 10], [10, 0], [0, 10]];
  const doubleLoop = [[0, 0], [20, 0], [20, 20], [0, 20], [0, 0], [5, 5], [15, 5], [15, 15], [5, 15], [5, 5]];
  for (const p of [[2, 5], [5, 2], [10, 10], [7, 5]]) {
    add("polygonIncludesPoint", p, [...bowtie, bowtie[0]]);
    add("polygonIncludesPointNonZero", p, bowtie);
    add("polygonIncludesPoint", p, doubleLoop);
    add("polygonIncludesPointNonZero", p, doubleLoop);
  }
  repeat("polygonIncludesPoint", () => {
    const poly = ngon(7, 60, 30);
    return [pt(100), [...poly, poly[0]]];
  });
  repeat("polygonIncludesPointNonZero", () => [pt(100), cloud(6, 100)]);
  add("polygonIsClosed", square);
  add("polygonIsClosed", closedSquare);
  add("polygonIsClosed", [...square, [0.00001, 0]]);
  add("polygonIsClosed", [...square, [0.5, 0]], 1);
  add("polygonSignedArea", square);
  add("polygonSignedArea", [...square].reverse());
  add("polygonSignedArea", closedSquare);
  add("polygonSignedArea", [...square, [0.5, 0]], 1);
  repeat("polygonSignedArea", () => [cloud(6, 100)]);
  add("polygonArea", square);
  add("polygonArea", [...square].reverse());
  add("polygonArea", closedSquare);
  repeat("polygonArea", () => [ngon(9, 50, 20)]);
  add("convexHull", [...square, [5, 5]]);
  add("convexHull", [...square, [5, 0]]);
  add("convexHull", square.slice(0, 2));
  add("convexHull", [[0, 0], [1, 1], [2, 2]]);
  add("convexHull", [[0, 0], [0, 0], [0, 0], [1, 0]]);
  add(
    "convexHull",
    Array.from({ length: 30 }, (_, i) => [Math.cos(i * 2.4) * 10, Math.sin(i * 2.4) * 10]),
  );
  repeat("convexHull", () => [cloud(12, 100)]);
  const wobbly = [...square, [5, -0.2], [10.2, 5], [5, 10.2], [-0.2, 5]];
  add("simplifyConvexPolygon", square.slice(0, 2), 0.4);
  add("simplifyConvexPolygon", wobbly, (25 * Math.PI) / 180);
  // No regular polygons here: their vertex turns tie up to the last bit of
  // Math.atan2, which differs between platforms, so the sharpest-vertex start
  // (and with it the whole result) is platform-dependent upstream too. The
  // jittered ones keep the turns apart.
  add("simplifyConvexPolygon", ngon(64, 100, 1), (25 * Math.PI) / 180);
  add("simplifyConvexPolygon", square, 10);
  repeat("simplifyConvexPolygon", () => [ngon(24, 80, 4), next() * 1.2]);

  // range.ts
  add("rangeInclusive", 1, 4);
  add("rangeInclusiveFromPair", [2, 3]);
  for (const [a, b] of [
    [[1, 4], [2, 3]],
    [[1, 4], [1, 4]],
    [[2, 3], [1, 4]],
    [[1, 4], [2, 5]],
    [[1, 4], [4, 5]],
    [[1, 4], [5, 7]],
    [[5, 7], [1, 4]],
  ]) {
    add("rangesOverlap", a, b);
    add("rangeIntersection", a, b);
  }
  const range = () => {
    const a = num(50);
    return [a, a + pos(50)];
  };
  repeat("rangesOverlap", () => [range(), range()]);
  repeat("rangeIntersection", () => [range(), range()]);
  add("rangeIncludesValue", 1, [1, 4]);
  add("rangeIncludesValue", 4, [1, 4]);
  add("rangeIncludesValue", 4.0001, [1, 4]);
  repeat("rangeIncludesValue", () => [num(60), range()]);

  // rectangle.ts
  add("rectangle", [1, 2], [3, 4]);
  add("rectangleFromNumberSequence", 1, 2, 3, 4);
  repeat("rectangleFromNumberSequence", () => [num(), num(), num(), num()]);
  const rect = () => {
    const [x, y] = pt(80);
    return [[x, y], [x + pos(80), y + pos(80)]];
  };
  add("rectangleIntersectLineSegment", [[0, 0], [10, 10]], [[-5, 5], [15, 5]]);
  add("rectangleIntersectLineSegment", [[0, 0], [10, 10]], [[2, 2], [8, 8]]);
  add("rectangleIntersectLineSegment", [[0, 0], [10, 10]], [[-5, -5], [15, 15]]);
  repeat("rectangleIntersectLineSegment", () => [rect(), seg(120)]);
  add("rectangleIntersectRectangle", [[0, 0], [10, 10]], [[5, 5], [15, 15]]);
  add("rectangleIntersectRectangle", [[0, 0], [10, 10]], [[10, 0], [20, 10]]);
  add("rectangleIntersectRectangle", [[0, 0], [10, 10]], [[2, 2], [3, 3]]);
  repeat("rectangleIntersectRectangle", () => [rect(), rect()]);

  // triangle.ts
  const tri = [[0, 0], [10, 0], [0, 10]];
  for (const p of [[1, 1], [5, 0], [0, 0], [5, 5], [6, 6], [-1, 1]]) add("triangleIncludesPoint", tri, p);
  repeat("triangleIncludesPoint", () => [[pt(80), pt(80), pt(80)], pt(80)]);

  return cases;
};
