// goldens/math.json covers packages/math/src function for function (ex-201,
// ex-202 for curve.ts, ex-706 for pca.ts): every function exported by the
// math package has cases, and nothing else does. The export
// list is read from the pinned checkout, so an upstream function the golden
// misses fails here.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

import { fromBits, JS_MATH_FUNCTIONS, toBits } from "../js-math.mjs";
import { MATH_FUNCTIONS } from "../math.mjs";
import { golden, upstreamDir } from "./helpers.mjs";

const EXPORTED_FN = /^export (?:function (\w+)|const (\w+)\s*=\s*(?:<[^=]*?>\s*)?\()/gm;

const upstreamExports = () => {
  const src = join(upstreamDir(), "packages", "math", "src");
  const out = {};
  for (const file of readdirSync(src).sort()) {
    if (!file.endsWith(".ts")) continue;
    const text = readFileSync(join(src, file), "utf8");
    const names = [...text.matchAll(EXPORTED_FN)].map((m) => m[1] ?? m[2]);
    if (names.length) out[file] = [...new Set(names)].sort();
  }
  return out;
};

test("math.json lists exactly the functions packages/math exports", () => {
  const expected = upstreamExports();
  const listed = Object.fromEntries(
    Object.entries(MATH_FUNCTIONS)
      .map(([f, names]) => [f, [...names].sort()])
      .sort(([a], [b]) => a.localeCompare(b)),
  );
  assert.deepEqual(listed, expected);
});

test("every covered function has cases, and every case calls a covered function", () => {
  const covered = new Set(Object.values(MATH_FUNCTIONS).flat());
  const seen = new Map();
  for (const c of golden("math.json").cases) {
    assert.ok(covered.has(c.fn), `${c.id}: ${c.fn} is not a covered function`);
    assert.ok(Array.isArray(c.args), c.id);
    assert.ok("result" in c, c.id);
    seen.set(c.fn, (seen.get(c.fn) ?? 0) + 1);
  }
  for (const fn of covered) assert.ok(seen.get(fn) > 0, `no cases for ${fn}`);
});

test("math.json reproduces upstream's own math tests (packages/math/tests)", () => {
  const byCall = new Map(
    golden("math.json").cases.map((c) => [`${c.fn}${JSON.stringify(c.args)}`, c.result]),
  );
  const call = (fn, ...args) => {
    const key = `${fn}${JSON.stringify(args)}`;
    assert.ok(byCall.has(key), `missing case ${key}`);
    return byCall.get(key);
  };
  // line.test.ts
  assert.deepEqual(call("linesIntersectAt", [[-5, -5], [5, 5]], [[5, -5], [-5, 5]]), [0, 0]);
  assert.deepEqual(call("linesIntersectAt", [[0, 0], [10, 10]], [[10, 0], [0, 10]]), [5, 5]);
  assert.equal(call("linesIntersectAt", [[0, 0], [0, 10]], [[10, 0], [10, 10]]), null);
  // segment.test.ts
  assert.deepEqual(call("lineSegmentIntersectionPoints", [[0, 0], [5, 0]], [[2, -2], [3, 2]]), [2.5, 0]);
  assert.equal(call("lineSegmentIntersectionPoints", [[0, 0], [5, 0]], [[3, 1], [4, 4]]), null);
  assert.equal(call("isLineSegment", [[0, 0], [1, 1]]), true);
  assert.equal(call("isLineSegment", [[0, 0], "not-a-point"]), false);
  assert.equal(call("isLineSegment", [[0, 0]]), false);
  // ellipse.test.ts
  const e22 = { center: [0, 0], halfWidth: 2, halfHeight: 2 };
  assert.deepEqual(call("ellipseSegmentInterceptPoints", e22, [[-10, 0], [10, 0]]), [[-2, 0], [2, 0]]);
  assert.deepEqual(call("ellipseSegmentInterceptPoints", e22, [[-10, -2], [10, -2]]), [[0, -2]]);
  assert.deepEqual(call("ellipseLineIntersectionPoints", e22, [[0, -1], [0, 1]]), [[0, 2], [0, -2]]);
  assert.deepEqual(call("ellipseLineIntersectionPoints", e22, [[-2, -2], [2, -2]]), [[0, -2]]);
  const circle = { center: [0, 0], halfWidth: 100, halfHeight: 100 };
  assert.equal(call("ellipseDistanceFromPoint", [0, 0], circle), 100);
  assert.equal(call("ellipseDistanceFromPoint", [30, 40], circle), 50);
  // polygon.test.ts
  const square = [[0, 0], [10, 0], [10, 10], [0, 10]];
  assert.equal(call("polygonArea", square), 100);
  assert.equal(call("polygonSignedArea", [...square].reverse()), -100);
  assert.equal(call("convexHull", [...square, [5, 5]]).length, 4);
  // range.test.ts
  assert.deepEqual(call("rangeIntersection", [1, 4], [4, 5]), [4, 4]);
  assert.equal(call("rangeIntersection", [1, 4], [5, 7]), null);
  // point.test.ts
  assert.deepEqual(call("pointRotateRads", [10, 20], [20, 30], Math.PI / 2), [30, 20]);
  // curve.test.ts. Its toCloselyEqualPoints defaults to a window of
  // 10 ** 2 (utils/src/test-utils.ts), which is no check at all; these hold
  // to 0.01, the precision the test's expected values are written in.
  const closePoints = (actual, expected) => {
    assert.equal(actual.length, expected.length);
    expected.forEach(([x, y], i) => {
      assert.ok(Math.abs(actual[i][0] - x) < 0.01 && Math.abs(actual[i][1] - y) < 0.01, `${actual[i]} vs ${[x, y]}`);
    });
  };
  const cS = [[-50, -50], [10, -50], [10, 50], [50, 50]];
  closePoints(call("curveIntersectLineSegment", [[100, 0], [100, 100], [100, 100], [0, 100]], [[0, 0], [200, 200]]), [
    [87.5, 87.5],
  ]);
  closePoints(call("curveIntersectLineSegment", [[100, 0], [100, 60], [60, 100], [0, 100]], [[0, 0], [200, 200]]), [
    [72.5, 72.5],
  ]);
  closePoints(call("curveIntersectLineSegment", cS, [[10, -60], [10, 60]]), [[9.99, 5.05]]);
  closePoints(
    call(
      "curveIntersectLineSegment",
      [
        [41.028864759926016, 12.226249068355052],
        [41.028864759926016, 33.55958240168839],
        [30.362198093259348, 44.22624906835505],
        [9.028864759926016, 44.22624906835505],
      ],
      [
        [-82.30963544324186, -41.19949363038283],
        [188.2149592542487, 134.75505940984908],
      ],
    ),
    [[34.4, 34.71]],
  );
  closePoints([call("curveClosestPoint", cS, [0, 0], 1e-3)], [[5.965462100367372, -3.04104878946646]]);
  assert.ok(Math.abs(call("curvePointDistance", cS, [0, 0]) - 6.695873043213627) < 0.005);
});

test("js-sort.json pins V8's order for inconsistent comparators", () => {
  const cases = golden("js-sort.json").cases;
  const sorts = cases.filter((c) => c.kind === "sort");
  const hulls = cases.filter((c) => c.kind === "convexHull");
  const nonFinite = (v) => typeof v === "string";
  // Most cases must actually make the comparator answer NaN.
  assert.ok(sorts.filter((c) => c.values.some(nonFinite)).length >= 100);
  assert.ok(hulls.every((c) => c.points.flat().some(nonFinite)));
  // Lengths either side of V8's small-array (8) and min-run (64) cutoffs.
  const lengths = new Set(sorts.map((c) => c.values.length));
  for (const n of [5, 8, 63, 64, 65, 257]) assert.ok(lengths.has(n), `no sort of length ${n}`);
  for (const c of sorts) {
    assert.deepEqual([...c.result].sort((a, b) => a - b), [...c.values.keys()], `${c.id}: not a permutation`);
  }
  for (const c of hulls) {
    assert.ok(c.result.every((i) => Number.isInteger(i) && i >= 0 && i < c.points.length), c.id);
  }
});

test("js-math.json pins V8's Math functions bit for bit, tricky arguments included", () => {
  const cases = golden("js-math.json").cases;
  const byFn = new Map();
  for (const c of cases) {
    assert.ok(c.fn in JS_MATH_FUNCTIONS, `${c.id}: unknown function ${c.fn}`);
    assert.ok(c.args.every((a) => /^[0-9a-f]{16}$/.test(a)), `${c.id}: args are not IEEE bits`);
    assert.ok(c.result === "NaN" || /^[0-9a-f]{16}$/.test(c.result), `${c.id}: result is not IEEE bits`);
    // the committed answer is what this Node computes
    const result = JS_MATH_FUNCTIONS[c.fn](...c.args.map(fromBits));
    assert.equal(Number.isNaN(result) ? "NaN" : toBits(result), c.result, c.id);
    byFn.set(c.fn, (byFn.get(c.fn) ?? 0) + 1);
  }
  for (const fn of Object.keys(JS_MATH_FUNCTIONS)) assert.ok(byFn.get(fn) >= 40, `too few cases for ${fn}`);
  // the arguments that tell arm64 V8 from x64 V8 and from libm are there
  const calls = new Set(cases.map((c) => `${c.fn}(${c.args.join(",")})`));
  assert.ok(calls.has("sin(c005778a4a4a9f9b)"));
  assert.ok(calls.has(`cos(${toBits(0.982953340331056)})`), "Math.cos(0.982953340331056)");
  assert.ok(calls.has("atan2(3fa9b88fc40e2240,3fe60ce55df44f04)"));
});
