// The scene of the performance budgets (ex-710): 1,000 elements on a 40 × 25
// grid that fills the 1000 × 700 editor at zoom 1, so every element is in
// the viewport and none is culled while the view pans. The types, fills and
// roughness cycle over what a drawing holds (upstream's element types,
// packages/element/src/types.ts): rectangles sharp and rounded, diamonds,
// ellipses, lines, arrows with a head, freedraw strokes and text, with
// hachure, cross-hatch and solid fills at roughness 0, 1 and 2. Everything
// is derived from the element's index, so the scene is the same on every
// run and every machine.

export const COLS = 40;
export const ROWS = 25;
export const CELL_W = 25;
export const CELL_H = 28;
export const COUNT = COLS * ROWS;

const TYPES = [
  "rectangle",
  "diamond",
  "ellipse",
  "line",
  "arrow",
  "freedraw",
  "text",
  "rectangle-rounded",
];
const FILLS = ["hachure", "cross-hatch", "solid"];
const STROKES = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];
const BACKGROUNDS = ["transparent", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"];

/** The fractional index of the i-th element: "a" and a base-36 key. */
const fractionalIndex = (i) => `a${i.toString(36).padStart(2, "0")}`;

/** Element `i` of the scene. */
export function element(i) {
  const col = i % COLS;
  const row = Math.floor(i / COLS);
  const kind = TYPES[i % TYPES.length];
  const x = col * CELL_W + 3;
  const y = row * CELL_H + 2;
  const w = 19;
  const h = 20;
  const base = {
    id: `perf-${i}`,
    type: kind === "rectangle-rounded" ? "rectangle" : kind,
    x,
    y,
    width: w,
    height: h,
    angle: 0,
    strokeColor: STROKES[i % STROKES.length],
    backgroundColor: BACKGROUNDS[Math.floor(i / TYPES.length) % BACKGROUNDS.length],
    fillStyle: FILLS[i % FILLS.length],
    strokeWidth: 1 + (i % 3),
    strokeStyle: "solid",
    roughness: i % 3,
    opacity: 100,
    groupIds: [],
    frameId: null,
    index: fractionalIndex(i),
    roundness: kind === "rectangle-rounded" ? { type: 3 } : null,
    seed: 1 + i * 7919,
    version: 1,
    versionNonce: 1 + i,
    isDeleted: false,
    boundElements: null,
    updated: 1,
    link: null,
    locked: false,
  };
  switch (kind) {
    case "line":
    case "arrow":
      return {
        ...base,
        roundness: kind === "arrow" ? { type: 2 } : null,
        points: [
          [0, 0],
          [w / 2, h],
          [w, h / 3],
        ],
        lastCommittedPoint: null,
        startBinding: null,
        endBinding: null,
        startArrowhead: null,
        endArrowhead: kind === "arrow" ? "arrow" : null,
        elbowed: false,
        polygon: false,
      };
    case "freedraw": {
      const points = [];
      for (let k = 0; k <= 12; k++) {
        points.push([(w * k) / 12, (h / 2) * (1 - Math.cos((k * Math.PI) / 3))]);
      }
      return { ...base, points, pressures: [], simulatePressure: true, lastCommittedPoint: null };
    }
    case "text":
      return {
        ...base,
        backgroundColor: "transparent",
        width: 17,
        height: 25,
        text: String.fromCharCode(65 + (i % 26)),
        originalText: String.fromCharCode(65 + (i % 26)),
        fontSize: 20,
        fontFamily: 5,
        textAlign: "left",
        verticalAlign: "top",
        containerId: null,
        autoResize: true,
        lineHeight: 1.25,
      };
    default:
      return base;
  }
}

/** The scene as .excalidraw JSON text. */
export function sceneJson() {
  const elements = [];
  for (let i = 0; i < COUNT; i++) elements.push(element(i));
  return JSON.stringify({
    type: "excalidraw",
    version: 2,
    source: "https://excalidraw.com",
    elements,
    appState: { viewBackgroundColor: "#ffffff", gridSize: 20 },
    files: {},
  });
}
