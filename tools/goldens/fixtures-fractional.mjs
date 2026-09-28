// Fixture inputs for the fractional indexing goldens (ex-107). Only inputs
// live here: every output comes from upstream's code at the pin.
//
// fractional-indexing.json: the vendored rocicorp algorithm,
//   packages/fractional-indexing/src/index.ts.
// fractional-index.json: the element-level helpers,
//   packages/element/src/fractionalIndex.ts.
//
// Keys compare as JS strings, so the exact key strings upstream produces are
// the spec; a crate with a different key scheme cannot pass these.

export const BASE_10_DIGITS = "0123456789";
// The base-95 alphabet of the rocicorp test suite (src/test.js at v3.2.0).
export const BASE_95_DIGITS =
  " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";

/** mulberry32: a small deterministic PRNG for choosing fixture inputs. */
export const mulberry32 = (seed) => {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
};

const Z26 = "0".repeat(26);

/** Keys to pair up: valid keys at the edges of the key space, and malformed ones. */
export const ORDER_KEY_POOL = [
  "a0", "a1", "a2", "a0V", "a1V", "a0l", "a0G", "a08", "a01", "a02", "a035",
  "Zz", "Zy", "ZzV", "Z0", "Y00", "Yzz", "Xzzz", "X000", "b00", "b0Z", "bzz",
  "c000", "c4BZ", "XvoR", "b125", "b127", "b129", "b99", "b999", "az", "azV",
  "a1111", "a0000001", "a0zzzzz", `A${Z26}1`, `A${Z26}V`, `A${Z26}01`,
  `B${"0".repeat(25)}`, `B${"z".repeat(25)}`, `y${"z".repeat(25)}`,
  "z".repeat(27), `${"z".repeat(27)}V`, `${"z".repeat(26)}y`,
  // malformed: bad characters (ASCII, non-ASCII, astral), trailing zero, too
  // short, the smallest key, the empty string, bad heads
  "a!", "a_", "a1!", "a1_", "aé", "a1\u{1F600}", "zd0032", "a00", "a10", "b0", "A", "a", "",
  `A${Z26}`, "0", "1", "!0", "a0 ", "ZzV0",
];

/** Inputs for fractional-indexing.json. */
export const orderKeyCases = () => {
  const cases = [];
  const keys = [...new Set([...ORDER_KEY_POOL, "z".padEnd(28, "z"), "a1V"])];
  keys.forEach((key, i) => cases.push({ id: `validate-${i}`, fn: "validateOrderKey", key }));

  // every ordered pair of pool keys, with null (START / END) on either side
  let n = 0;
  const withNull = [null, ...ORDER_KEY_POOL];
  for (const a of withNull) {
    for (const b of withNull) {
      cases.push({ id: `between-${n++}`, fn: "generateKeyBetween", a, b });
    }
  }
  // the rocicorp suite's other alphabets
  const base10 = [null, "a0", "a1", "a4", "a9", "a01", "a05", "Z9", "Z5", "b00", "b99", "a00", "c000"];
  for (const a of base10) {
    for (const b of base10) {
      cases.push({ id: `between-b10-${n++}`, fn: "generateKeyBetween", a, b, digits: BASE_10_DIGITS });
    }
  }
  const base95 = [
    null, "a ", "a!", "a~", "Z~", "a00", "a01", "a0/", "a0", "a0V", "a  1", "a  2", "b  ",
    "b   ", "a0 ", `A${" ".repeat(26)}0`, `A${" ".repeat(26)}`,
  ];
  for (const a of base95) {
    for (const b of base95) {
      cases.push({ id: `between-b95-${n++}`, fn: "generateKeyBetween", a, b, digits: BASE_95_DIGITS });
    }
  }

  // generateNKeysBetween: every n from 0 to 40 between a few bounds, then a
  // few long runs that cross integer-part boundaries
  let k = 0;
  const bounds = [
    [null, null], ["a0", null], [null, "a0"], ["a0", "a1"], ["a0", "a2"], ["a1", "a1V"],
    ["Zz", "a0"], ["a0V", "a0l"], ["z".repeat(26) + "y", null],
    [null, `A${Z26}1`], ["a1", "a0"], ["a!", null], [null, "a00"],
  ];
  for (const [a, b] of bounds) {
    for (let count = 0; count <= 40; count++) {
      cases.push({ id: `n-${k++}`, fn: "generateNKeysBetween", a, b, n: count });
    }
  }
  for (const [a, b, count] of [
    [null, null, 4000], ["a0", null, 4000], [null, "a0", 4000], ["a0", "a1", 3000], ["Zz", "a0V", 2000],
  ]) {
    cases.push({ id: `n-${k++}`, fn: "generateNKeysBetween", a, b, n: count });
  }
  for (const [a, b, count] of [
    [null, null, 5], ["a4", null, 10], [null, "a0", 5], ["a0", "a2", 20], ["a0", "a1", 50],
  ]) {
    cases.push({ id: `n-b10-${k++}`, fn: "generateNKeysBetween", a, b, n: count, digits: BASE_10_DIGITS });
  }

  // random walks: insert between random neighbours of a growing sorted list;
  // `picks` are the insertion slots, the generator records every key
  const rand = mulberry32(1107);
  for (let w = 0; w < 4; w++) {
    const picks = [];
    for (let size = 0; size < 400; size++) picks.push(Math.floor(rand() * (size + 1)));
    cases.push({ id: `walk-${w}`, fn: "walk", picks });
  }
  return cases;
};

/** The element lists of packages/element/tests/fractionalIndex.test.ts. */
const UPSTREAM_SYNC_SCENARIOS = [
  { elements: [], moved: [] },
  { elements: [["A", "a1"]], moved: [] },
  { elements: [["A", "a1"], ["B", "a2"], ["C", "a3"]], moved: [] },
  { elements: [["A"]], moved: ["A"] },
  { elements: [["A", "zd0032"]] },
  { elements: [["A", "a1"], ["B", "zd0032"], ["C", "a3"]] },
  { elements: [["A", "a!"]] },
  { elements: [["A", "a1"], ["B", "a!"], ["C", "a2"]] },
  { elements: [["A", "a1"], ["B", "a1"]] },
  { elements: [["A", "a2"], ["B", "a1"]], moved: ["B"] },
  { elements: [["A", "a2"], ["B", "a1"]], moved: ["A"] },
  { elements: [["A", "a3"], ["B", "a2"], ["C", "a1"]], moved: ["B", "C"] },
  { elements: [["A", "a1"], ["B", "a0"], ["C", "a2"]], moved: ["B"] },
  { elements: [["A", "a1"], ["B", "a2"], ["C", "a1"]], moved: ["C"] },
  { elements: [["A", "a0"], ["B", "a2"], ["C", "a1"], ["D", "a1"], ["E", "a2"]], moved: ["C", "D", "E"] },
  {
    elements: [["A"], ["B"], ["C", "a0"], ["D", "a2"], ["E"], ["F", "a3"], ["G"], ["H", "a1"], ["I", "a2"], ["J"]],
    moved: ["A", "B", "E", "G", "H", "I", "J"],
  },
  { elements: [["A", "a2"], ["B", "a4"]], moved: ["A"] },
  { elements: [["A", "a2"], ["B", "a4"]], moved: ["B"] },
  { elements: [["C", "a2"], ["D", "a3"], ["A", "a0"], ["B", "a1"]], moved: ["C", "D"] },
  {
    elements: [
      ["A", "a1"], ["B", "a2"], ["D", "a4"], ["C", "a3"], ["F", "a6"], ["E", "a5"], ["H", "a8"], ["G", "a7"], ["I", "a9"],
    ],
    moved: ["D", "F", "H"],
  },
  { elements: [["A", "a1"], ["B", "a0"], ["C", "a2"]], moved: ["B", "C"] },
  { elements: [["A", "a1"], ["B", "a0"], ["C", "a2"]], moved: ["A", "B"] },
  { elements: [["A", "a0"], ["B", "a2"], ["C", "a1"], ["D", "a1"], ["E", "a2"]], moved: ["B", "D", "E"] },
  {
    elements: [["A"], ["B"], ["C", "a0"], ["D", "a2"], ["E"], ["F", "a3"], ["G"], ["H", "a1"], ["I", "a2"], ["J"]],
    moved: ["A", "B", "D", "E", "F", "G", "J"],
  },
  { elements: [["A", "a1"], ["B", "a1"], ["C", "a2"]], moved: ["B"] },
  {
    elements: [["A", "a01"], ["B", "a01"], ["C", "a01"], ["D", "a01"], ["E", "a02"], ["F", "a02"], ["G", "a02"]],
    moved: ["B", "C", "D", "E", "F"],
  },
  {
    elements: [["A", "a01"], ["B", "a01"], ["C", "a01"], ["D", "a01"], ["E", "a02"], ["F", "a02"], ["G", "a02"]],
    moved: ["A", "C", "D", "E", "G"],
  },
  {
    elements: [["A", "a01"], ["B", "a01"], ["C", "a01"], ["D", "a01"], ["E", "a02"], ["F", "a02"], ["G", "a02"]],
    moved: ["B", "C", "D", "F", "G"],
  },
  { elements: [["A", "a1"], ["B", "a1"], ["C", "a1"]], moved: [] },
  { elements: [["A", "a1"], ["B"], ["C", "a0"]], moved: [] },
  { elements: [["A", "a1"], ["B", "a2"], ["C", "a1"]], moved: ["A"] },
  { elements: [["A", "a2"], ["B"], ["C", "a1"]], moved: ["B"] },
  {
    elements: [["A", "a01"], ["B"], ["C"], ["D", "a01"], ["E", "a01"], ["F", "a01"], ["G"], ["I", "a03"], ["H"]],
    moved: ["B", "C", "D", "F", "G", "H"],
  },
];

const SYNC_INDEX_POOL = [
  null, null, null, "a0", "a1", "a1", "a2", "a3", "a0V", "a1V", "Zz", "a01", "a02", "b00", "a!",
  "zd0032", "", "a00", `A${Z26}`, "aé", "\u{1F600}",
];

/** Inputs for fractional-index.json. */
export const fractionalIndexCases = () => {
  const cases = [];
  const toElements = (list) => list.map(([id, index]) => ({ id, index: index ?? null }));
  UPSTREAM_SYNC_SCENARIOS.forEach((s, n) => {
    const elements = toElements(s.elements);
    cases.push({ id: `upstream-${n}-invalid`, fn: "syncInvalidIndices", elements });
    if (s.moved) {
      cases.push({ id: `upstream-${n}-moved`, fn: "syncMovedIndices", elements, moved: s.moved });
    }
  });

  // The upstream test's large arrays, at 600 elements to keep the file small:
  // all empty; empty but the last, which holds the 600th key counting up from
  // a0; every key counting down from a0.
  const length = 600;
  const ids = Array.from({ length }, (_, i) => `A_${i}`);
  const empty = ids.map((id) => ({ id, index: null }));
  cases.push({ id: "empty-600-invalid", fn: "syncInvalidIndices", elements: empty });
  cases.push({ id: "empty-600-moved", fn: "syncMovedIndices", elements: empty, moved: ids });
  cases.push({ id: "growing-600-invalid", fn: "syncInvalidIndices", elements: empty, chain: "up" });
  cases.push({ id: "growing-600-moved", fn: "syncMovedIndices", elements: empty, chain: "up", moved: ids.slice(0, -1) });
  cases.push({ id: "declining-600-invalid", fn: "syncInvalidIndices", elements: empty, chain: "down" });
  cases.push({ id: "declining-600-moved", fn: "syncMovedIndices", elements: empty, chain: "down", moved: ids.slice(1) });

  // random lists over valid, duplicated, missing and malformed indices
  const rand = mulberry32(107);
  const pick = (list) => list[Math.floor(rand() * list.length)];
  for (let c = 0; c < 400; c++) {
    const size = Math.floor(rand() * 13);
    const elements = Array.from({ length: size }, (_, i) => ({ id: `e${i}`, index: pick(SYNC_INDEX_POOL) }));
    cases.push({ id: `random-${c}-invalid`, fn: "syncInvalidIndices", elements });
    const moved = elements.filter(() => rand() < 0.35).map((e) => e.id);
    cases.push({ id: `random-${c}-moved`, fn: "syncMovedIndices", elements, moved });
  }

  // validateFractionalIndices messages, with and without the bound text check
  const el = (id, index, extra = {}) => ({
    id, type: "rectangle", index, isDeleted: false, version: 1, versionNonce: 0, boundElements: null, locked: false, ...extra,
  });
  const text = (id, index, containerId, extra = {}) => el(id, index, { type: "text", containerId, ...extra });
  const bound = (textId) => ({ boundElements: [{ type: "arrow", id: "x" }, { type: "text", id: textId }] });
  const validations = [
    [],
    [el("A", "a1")],
    [el("A", "a1"), el("B", "a1", { version: 3, versionNonce: 123456789, isDeleted: true })],
    [el("A", null), el("B", "a0")],
    [el("A", "a!"), el("B", "a2")],
    [el("A", "a2", { version: 2.5 }), el("B", "a1"), el("C", "a0")],
    [el("C", "a1", bound("T")), text("T", "a2", "C")],
    [text("T", "a0", "C"), el("C", "a1", bound("T"))],
    [el("C", "a1", { ...bound("T"), type: "diamond" }), text("T", "a1", "C")],
    [el("C", "a1", { ...bound("T"), type: "ellipse" }), text("T", "a0V", "C")],
    [el("C", "a1", { ...bound("T"), type: "arrow" }), text("T", "a0", "C")],
    [el("C", "a1", { ...bound("T"), type: "stickynote" }), text("T", "a0", "C")],
    [el("C", "a1", { ...bound("T"), type: "line" }), text("T", "a0", "C")],
    [el("C", "a1", { ...bound("T"), isDeleted: true }), text("T", "a0", "C")],
    [el("C", "a1", bound("T")), text("T", "a0", "C", { isDeleted: true })],
    [el("C", "a1", bound("missing")), text("T", "a0", "C")],
    [el("C", "a1", bound("")), text("", "a0", "C")],
    [el("C", "a1", { boundElements: [] }), text("T", "a0", "C")],
    [el("C", null, bound("T")), text("T", null, "C")],
    [el("C", "a1", bound("T")), text("T", null, "C")],
    [el("C", null, bound("T")), text("T", "a1", "C")],
    [el("C", "", bound("T")), text("T", null, "C")],
    [el("C", "a1", { ...bound("T"), locked: true }), text("T", "a0", "C")],
    [el("C", "a1", bound("T")), text("T", "a0", "C"), text("T", "a2", "C")],
  ];
  validations.forEach((elements, i) => {
    for (const includeBoundTextValidation of [false, true]) {
      cases.push({
        id: `validate-${i}-${includeBoundTextValidation ? "bound" : "plain"}`,
        fn: "validateFractionalIndices",
        elements,
        includeBoundTextValidation,
      });
    }
  });

  // orderByFractionalIndex over fully indexed lists with unique ids (its
  // comparator is a consistent order only when every element has an index)
  const orderPool = ["a0", "a1", "a1", "a2", "a0V", "Zz", "a01", "b00", "a1V", "a1"];
  for (let c = 0; c < 60; c++) {
    const size = 1 + Math.floor(rand() * (c < 50 ? 12 : 120));
    const elements = Array.from({ length: size }, (_, i) => ({
      id: `${pick(["x", "y", "a", "B"])}${i}`,
      index: pick(orderPool),
    }));
    cases.push({ id: `order-${c}`, fn: "orderByFractionalIndex", elements });
  }
  return cases;
};
