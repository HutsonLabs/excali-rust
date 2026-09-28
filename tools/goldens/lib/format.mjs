// Deterministic, diff-friendly JSON for the goldens.
//
// Plain JSON.stringify semantics for every scalar (numbers use ECMAScript's
// shortest round-trip form, so every double survives a parse exactly), with a
// layout that keeps one op or one point per line:
//   - arrays of numbers, and arrays of arrays of numbers, stay on one line;
//   - any other array or object stays on one line when that line is short;
//   - everything else is expanded with two-space indentation.
// Key order is insertion order, which the generator fixes.

const INLINE_WIDTH = 160;

const isNumberArray = (v) => Array.isArray(v) && v.every((x) => typeof x === "number");

const isPointList = (v) => Array.isArray(v) && v.length > 0 && v.every(isNumberArray);

const write = (value, indent) => {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value);
  }
  const flat = JSON.stringify(value);
  if (isNumberArray(value) || (isPointList(value) && flat.length <= INLINE_WIDTH * 4)) {
    return flat;
  }
  if (flat.length + indent.length <= INLINE_WIDTH) {
    return flat;
  }
  const inner = `${indent}  `;
  if (Array.isArray(value)) {
    if (value.length === 0) return "[]";
    return `[\n${value.map((v) => inner + write(v, inner)).join(",\n")}\n${indent}]`;
  }
  const entries = Object.entries(value);
  if (entries.length === 0) return "{}";
  return `{\n${entries
    .map(([k, v]) => `${inner}${JSON.stringify(k)}: ${write(v, inner)}`)
    .join(",\n")}\n${indent}}`;
};

export const format = (value) => `${write(value, "")}\n`;
