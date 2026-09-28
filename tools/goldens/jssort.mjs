// Inputs for goldens/js-sort.json: where upstream's result depends on the
// order Array.prototype.sort produces for an inconsistent comparator. V8
// sorts with TimSort (third_party/v8/builtins/array-sort.tq); for a
// consistent comparator any stable sort gives the same order, but when the
// comparator answers NaN (a NaN or infinite coordinate in convexHull,
// packages/math/src/polygon.ts:126-128) the permutation is TimSort's own.
// excali-math's js::sort ports it; these cases pin it.
//
// JSON has no NaN or infinity, so non-finite inputs are the strings "NaN",
// "Infinity" and "-Infinity". Results are indices into the input:
//   { id, kind: "sort", values, result }: [0..n).sort((i, j) => v[i] - v[j])
//   { id, kind: "convexHull", points, result }: math.convexHull(points), each
//     returned point given as the index of the input point it is.

import { rng } from "./math.mjs";

const decode = (v) => (typeof v === "string" ? Number(v) : v);
const encode = (v) => (Number.isFinite(v) ? v : String(v));

export const jsSortCases = () => {
  const next = rng(1041657908);
  const int = (n) => Math.floor(next() * n);
  const value = () => Math.round((next() * 2 - 1) * 1000) / 10;
  const maybeNaN = (density) => (v) => (next() < density ? NaN : v);

  // Value patterns, each exercising a different part of TimSort: random
  // data (binary insertion sort, merges), ascending and descending runs
  // (CountAndMakeRun), two long sorted halves (galloping in MergeLow and
  // MergeHigh), few distinct values (ties), and infinities (Infinity -
  // Infinity is NaN).
  const patterns = {
    random: (n, d) => Array.from({ length: n }, value).map(maybeNaN(d)),
    runs: (n, d) => {
      const out = [];
      while (out.length < n) {
        const run = Array.from({ length: 1 + int(40) }, value).sort((a, b) => a - b);
        if (next() < 0.4) run.reverse();
        out.push(...run);
      }
      return out.slice(0, n).map(maybeNaN(d));
    },
    halves: (n, d) => {
      const a = Array.from({ length: n >> 1 }, value).sort((x, y) => x - y);
      const b = Array.from({ length: n - (n >> 1) }, value).sort((x, y) => x - y);
      return [...a, ...b].map(maybeNaN(d));
    },
    fewDistinct: (n, d) => Array.from({ length: n }, () => int(4)).map(maybeNaN(d)),
    infinities: (n, d) =>
      Array.from({ length: n }, () => [Infinity, -Infinity, value()][int(3)]).map(maybeNaN(d)),
  };
  const lengths = [2, 3, 5, 8, 16, 17, 31, 32, 33, 63, 64, 65, 100, 128, 129, 200, 257, 400];

  const cases = [];
  const counts = new Map();
  const add = (kind, fields) => {
    const n = counts.get(kind) ?? 0;
    counts.set(kind, n + 1);
    cases.push({ id: `${kind}/${n}`, kind, ...fields });
  };

  for (const n of lengths) {
    for (const [name, make] of Object.entries(patterns)) {
      for (const density of name === "random" ? [0, 0.05, 0.3, 1] : [0.1]) {
        add("sort", { pattern: name, values: make(n, density).map(encode) });
      }
    }
  }

  const coord = () => {
    const r = next();
    if (r < 0.15) return NaN;
    if (r < 0.2) return [Infinity, -Infinity][int(2)];
    return value();
  };
  for (const n of [3, 4, 5, 8, 12, 16, 17, 24, 33, 48, 64, 65, 80, 130]) {
    for (let k = 0; k < 4; k++) {
      const points = Array.from({ length: n }, () => [coord(), coord()]);
      // Every hull case has at least one non-finite coordinate.
      if (points.flat().every(Number.isFinite)) points[int(n)][int(2)] = NaN;
      add("convexHull", { points: points.map((p) => p.map(encode)) });
    }
  }
  return cases;
};

/** The upstream result for one case (see header). */
export const jsSortResult = (up, c) => {
  if (c.kind === "sort") {
    const v = c.values.map(decode);
    return [...v.keys()].sort((i, j) => v[i] - v[j]);
  }
  const points = c.points.map((p) => p.map(decode));
  return up.math.convexHull(points).map((p) => {
    const i = points.indexOf(p);
    if (i < 0) throw new Error(`js-sort.json ${c.id}: convexHull returned a point it was not given`);
    return i;
  });
};
