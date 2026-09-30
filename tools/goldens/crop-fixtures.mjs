#!/usr/bin/env node
// Image cropping fixtures for excali-editor (ex-713): upstream's own
// packages/element/src/cropElement.ts run from the pinned checkout under
// plain Node.
//
//   node tools/goldens/crop-fixtures.mjs            write the fixture
//   node tools/goldens/crop-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/crop-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/crop.json:
//
//   { "description", "upstream", "minimalCropSize", "images", "cases",
//     "sequences" }
//
// - images: [{ id, element, naturalWidth, naturalHeight, uncroppedSize,
//   uncropped, flipAdjusted, flipAdjustedNatural }]. `element` is the image
//   ({ x, y, width, height, angle, scale, crop }, built with
//   newImageElement); uncroppedSize is getUncroppedWidthAndHeight(element),
//   uncropped the { x, y, width, height, crop } of
//   getUncroppedImageElement(element, map), flipAdjusted and
//   flipAdjustedNatural getFlipAdjustedCropPosition(element, false / true).
// - cases: [{ id, image, handle, pointer, widthAspectRatio, result }]:
//   cropElement(image, map, handle, naturalWidth, naturalHeight,
//   pointer[0], pointer[1], widthAspectRatio ?? undefined) on a fresh copy
//   of the image (cropElement changes an existing crop in place); result is
//   the returned { x, y, width, height, crop }. The pointer is a handle's
//   position on the rotated box plus an offset, including offsets far
//   beyond the image.
// - sequences: the UI.crop chains of upstream's
//   packages/element/tests/cropElement.test.tsx (lines 120-213 and
//   216-260), with the natural size at fixed multiples of 200x100 instead of
//   a random one. Each step is { op: "create", image, width, height } (an
//   image at 0,0 as API.createElement makes it), { op: "duplicate", image,
//   from } or { op: "crop", image, handle, move, keepAspectRatio, pointer }
//   as UI.crop does it (tests/helpers/ui.ts:618-649: the centre of
//   getTransformHandles(element, zoom 1, map, "mouse", {})[handle] plus
//   move, aspect ratio width / height when kept, the result assigned to
//   the element); `state` is the image's { x, y, width, height, crop }
//   after the step.
//
// Deterministic: fixed inputs, upstream in its test mode, reseed(1);
// Math.random throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "crop.json";

const ENTRY = `
export {
  cropElement,
  getUncroppedImageElement,
  getUncroppedWidthAndHeight,
  getFlipAdjustedCropPosition,
  MINIMAL_CROP_SIZE,
} from "./packages/element/src/cropElement";
export { getTransformHandles } from "./packages/element/src/transformHandles";
export { newImageElement } from "./packages/element/src/newElement";
export { duplicateElement } from "./packages/element/src/duplicate";
export { reseed } from "./packages/common/src/random";
export { arrayToMap } from "./packages/common/src/index";
`;

const usage = () => {
  process.stderr.write("usage: crop-fixtures.mjs [--check] [--out DIR]\n");
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

const HANDLES = ["n", "s", "e", "w", "ne", "nw", "se", "sw", "rotation"];

const SCALES = [
  [1, 1],
  [-1, 1],
  [1, -1],
  [-1, -1],
];

// Pointer offsets from the handle: none, small moves inwards and outwards,
// and far beyond the image in every diagonal direction.
const OFFSETS = [
  [0, 0],
  [23.5, 11.25],
  [-31, -17.5],
  [55, -40],
  [-1000, -1000],
  [1000, 1000],
  [1000, -1000],
];

// { id, x, y, width, height, naturalWidth, naturalHeight, crop }
const GEOMETRIES = [
  { id: "plain", x: 0, y: 0, width: 200, height: 100, naturalWidth: 400, naturalHeight: 200, crop: null },
  { id: "offset", x: 37.5, y: -12.25, width: 180, height: 120, naturalWidth: 543, naturalHeight: 362, crop: null },
  {
    id: "cropped",
    x: 40,
    y: 20,
    width: 120,
    height: 60,
    naturalWidth: 400,
    naturalHeight: 200,
    crop: { x: 80, y: 40, width: 240, height: 120, naturalWidth: 400, naturalHeight: 200 },
  },
  {
    id: "cropped-asymmetric",
    x: 10,
    y: 5,
    width: 90,
    height: 35,
    naturalWidth: 1000,
    naturalHeight: 500,
    crop: { x: 100, y: 250, width: 300, height: (35 * 500) / 150, naturalWidth: 1000, naturalHeight: 500 },
  },
  {
    id: "cropped-corner",
    x: -60,
    y: 80,
    width: 150,
    height: 70,
    naturalWidth: 640,
    naturalHeight: 480,
    crop: { x: 0, y: 0, width: 480, height: 280, naturalWidth: 640, naturalHeight: 480 },
  },
  {
    id: "cropped-min",
    x: 5,
    y: 5,
    width: 10,
    height: 10,
    naturalWidth: 300,
    naturalHeight: 300,
    crop: { x: 145, y: 30, width: 20, height: 20, naturalWidth: 300, naturalHeight: 300 },
  },
];

const variants = () => {
  const out = [];
  for (const scale of SCALES) out.push({ scale, angle: 0 });
  for (const scale of SCALES) out.push({ scale, angle: 0.7 });
  out.push({ scale: [1, 1], angle: Math.PI / 2 });
  out.push({ scale: [-1, 1], angle: 3.5 });
  return out;
};

const imageId = (g, v) => `${g.id}/scale${v.scale.join(",")}/angle${v.angle === Math.PI / 2 ? "pi2" : v.angle}`;

const newImage = (up, { x, y, width, height, angle = 0, scale = [1, 1], crop = null }) =>
  up.newImageElement({ type: "image", x, y, width, height, angle, scale: [...scale], crop: clone(crop) });

const geometry = (element) => ({
  x: element.x,
  y: element.y,
  width: element.width,
  height: element.height,
  crop: clone(element.crop),
});

// The handle's point on the unrotated box, turned with the element.
const handlePoint = (element, handle) => {
  const { x, y, width: w, height: h, angle } = element;
  const cx = x + w / 2;
  const cy = y + h / 2;
  let px = cx;
  let py = cy;
  if (handle.includes("n")) py = y;
  if (handle.includes("s")) py = y + h;
  if (handle.includes("w")) px = x;
  if (handle.includes("e")) px = x + w;
  if (handle === "rotation") {
    px = cx;
    py = y - 16;
  }
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  return [(px - cx) * cos - (py - cy) * sin + cx, (px - cx) * sin + (py - cy) * cos + cy];
};

const imageCases = (up) => {
  const images = [];
  const cases = [];
  for (const g of GEOMETRIES) {
    for (const v of variants()) {
      const id = imageId(g, v);
      const make = () => newImage(up, { ...g, ...v });
      const element = make();
      const map = up.arrayToMap([element]);
      images.push({
        id,
        element: {
          x: element.x,
          y: element.y,
          width: element.width,
          height: element.height,
          angle: element.angle,
          scale: element.scale,
          crop: clone(element.crop),
        },
        naturalWidth: g.naturalWidth,
        naturalHeight: g.naturalHeight,
        uncroppedSize: up.getUncroppedWidthAndHeight(element),
        uncropped: geometry(up.getUncroppedImageElement(element, map)),
        flipAdjusted: clone(up.getFlipAdjustedCropPosition(element)),
        flipAdjustedNatural: clone(up.getFlipAdjustedCropPosition(element, true)),
      });
      const ratios = [null, element.width / element.height];
      if (v.angle === 0) ratios.push(1.6, 0);
      for (const handle of HANDLES) {
        const [hx, hy] = handlePoint(element, handle);
        OFFSETS.forEach(([dx, dy], o) => {
          for (const widthAspectRatio of ratios) {
            if ((widthAspectRatio === 1.6 || widthAspectRatio === 0) && o > 2) continue;
            const fresh = make();
            const pointer = [hx + dx, hy + dy];
            const result = up.cropElement(
              fresh,
              up.arrayToMap([fresh]),
              handle,
              g.naturalWidth,
              g.naturalHeight,
              pointer[0],
              pointer[1],
              widthAspectRatio ?? undefined,
            );
            cases.push({
              id: `${id}/${handle}/offset${o}/ratio${widthAspectRatio === null ? "none" : widthAspectRatio}`,
              image: id,
              handle,
              pointer,
              widthAspectRatio,
              result: { x: result.x, y: result.y, width: result.width, height: result.height, crop: clone(result.crop) },
            });
          }
        });
      }
    }
  }
  return { images, cases };
};

// The UI.crop chains of cropElement.test.tsx; natural sizes fixed.
const SEQUENCES = [
  {
    // "Cropping changes the dimension" (cropElement.test.tsx:120-133)
    id: "changes-dimension",
    steps: [
      { op: "create", image: "a", width: 200, height: 100 },
      { op: "crop", image: "a", handle: "w", move: [100, 0] },
      { op: "crop", image: "a", handle: "n", move: [0, 50] },
    ],
  },
  {
    // "Cropping has minimal sizes" (cropElement.test.tsx:135-150)
    id: "minimal-sizes",
    steps: [
      { op: "create", image: "a", width: 200, height: 100 },
      { op: "crop", image: "a", handle: "w", move: [200, 0] },
      { op: "crop", image: "a", handle: "w", move: [-200, 0] },
      { op: "crop", image: "a", handle: "n", move: [0, 100] },
    ],
  },
  {
    // "Preserve aspect ratio" (cropElement.test.tsx:152-213)
    id: "preserve-aspect-ratio",
    steps: [
      { op: "create", image: "a", width: 200, height: 100 },
      { op: "crop", image: "a", handle: "w", move: [200 / 3, 0] },
      { op: "crop", image: "a", handle: "w", move: [-200 / 3, 0], keepAspectRatio: true },
      { op: "crop", image: "a", handle: "w", move: [-200 / 3, 0] },
      { op: "crop", image: "a", handle: "s", move: [0, -100 / 2] },
      { op: "crop", image: "a", handle: "e", move: [-200 / 3, 0] },
      { op: "crop", image: "a", handle: "se", move: [200, 0], keepAspectRatio: true },
      { op: "create", image: "a", width: 200, height: 100 },
      { op: "crop", image: "a", handle: "nw", move: [150, 50] },
      { op: "crop", image: "a", handle: "n", move: [0, -100], keepAspectRatio: true },
      { op: "crop", image: "a", handle: "nw", move: [-150, -100], keepAspectRatio: true },
    ],
  },
  {
    // "Cropping works independently of duplication" (cropElement.test.tsx:217-259)
    id: "duplication",
    steps: [
      { op: "create", image: "a", width: 200, height: 100 },
      { op: "crop", image: "a", handle: "nw", move: [100, 50] },
      { op: "duplicate", image: "b", from: "a" },
      { op: "crop", image: "b", handle: "nw", move: [-100, -50] },
      { op: "crop", image: "b", handle: "se", move: [-200 / 1.5, -100 / 1.5] },
    ],
  },
  {
    // "Resizing should not affect crop", its crops (cropElement.test.tsx:261-285)
    id: "resize-keeps-crop",
    steps: [
      { op: "create", image: "a", width: 200, height: 100 },
      { op: "crop", image: "a", handle: "nw", move: [100, 50] },
      { op: "crop", image: "a", handle: "e", move: [200, 0] },
    ],
  },
];

const NATURAL_SCALES = [1, 2.5, 4.321, 6];

const sequenceCases = (up) => {
  const out = [];
  for (const seq of SEQUENCES) {
    for (const scale of NATURAL_SCALES) {
      const naturalWidth = 200 * scale;
      const naturalHeight = 100 * scale;
      const images = new Map();
      const steps = [];
      for (const step of seq.steps) {
        const record = { ...step };
        if (step.op === "create") {
          images.set(step.image, newImage(up, { x: 0, y: 0, width: step.width, height: step.height }));
        } else if (step.op === "duplicate") {
          images.set(step.image, up.duplicateElement(null, new Map(), images.get(step.from)));
        } else {
          const element = images.get(step.image);
          const map = up.arrayToMap([...images.values()]);
          const coords = up.getTransformHandles(element, { value: 1 }, map, "mouse", {})[step.handle];
          const pointer = [coords[0] + coords[2] / 2 + step.move[0], coords[1] + coords[3] / 2 + step.move[1]];
          const mutations = up.cropElement(
            element,
            map,
            step.handle,
            naturalWidth,
            naturalHeight,
            pointer[0],
            pointer[1],
            step.keepAspectRatio ? element.width / element.height : undefined,
          );
          Object.assign(element, mutations);
          record.keepAspectRatio = !!step.keepAspectRatio;
          record.pointer = pointer;
        }
        record.state = geometry(images.get(step.image));
        steps.push(record);
      }
      out.push({ id: `${seq.id}/natural${scale}`, sequence: seq.id, naturalWidth, naturalHeight, steps });
    }
  }
  return out;
};

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating crop fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const buildFixture = (up, commit) => {
  up.reseed(1);
  const { images, cases } = imageCases(up);
  up.reseed(1);
  const sequences = sequenceCases(up);
  const ids = new Set();
  for (const c of [...images, ...cases, ...sequences]) {
    if (ids.has(c.id)) throw new Error(`duplicate case ${c.id}`);
    ids.add(c.id);
  }
  return format({
    description:
      "cropElement, getUncroppedImageElement, getUncroppedWidthAndHeight and getFlipAdjustedCropPosition (packages/element/src/cropElement.ts) on images at every flip, several angles and existing crops, from every handle with pointers near and far beyond the image, with and without a kept aspect ratio; and the UI.crop chains of packages/element/tests/cropElement.test.tsx at fixed natural sizes. Generated by tools/goldens/crop-fixtures.mjs.",
    upstream: commit,
    minimalCropSize: up.MINIMAL_CROP_SIZE,
    images,
    cases,
    sequences,
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`crop-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
  });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("crop fixture is out of date: run node tools/goldens/crop-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`crop fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
