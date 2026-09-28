// goldens/math.json covers packages/math/src function for function (ex-201):
// every function exported by the math package, except curve.ts (ex-202) and
// pca.ts (shape recognition), has cases, and nothing else does. The export
// list is read from the pinned checkout, so an upstream function the golden
// misses fails here.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

import { MATH_FUNCTIONS } from "../math.mjs";
import { golden, upstreamDir } from "./helpers.mjs";

const EXCLUDED = new Set(["curve.ts", "pca.ts"]);
const EXPORTED_FN = /^export (?:function (\w+)|const (\w+)\s*=\s*(?:<[^=]*?>\s*)?\()/gm;

const upstreamExports = () => {
  const src = join(upstreamDir(), "packages", "math", "src");
  const out = {};
  for (const file of readdirSync(src).sort()) {
    if (!file.endsWith(".ts") || EXCLUDED.has(file)) continue;
    const text = readFileSync(join(src, file), "utf8");
    const names = [...text.matchAll(EXPORTED_FN)].map((m) => m[1] ?? m[2]);
    if (names.length) out[file] = [...new Set(names)].sort();
  }
  return out;
};

test("math.json lists exactly the functions packages/math exports (curve.ts and pca.ts aside)", () => {
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
});
