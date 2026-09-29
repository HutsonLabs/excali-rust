#!/usr/bin/env node
// Transform handle and resize fixtures for excali-editor (ex-508): upstream's
// own packages/element/src/transformHandles.ts, resizeTest.ts and
// resizeElements.ts, run from the pinned checkout under plain Node on the
// scenes of upstream's resize tests and on seeded random scenes.
//
//   node tools/goldens/transform-fixtures.mjs            write the fixture
//   node tools/goldens/transform-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/transform-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-editor/tests/fixtures/transform.json:
//
//   { "description", "upstream", "cases": [ { id, kind, ... } ] }
//
// Kinds:
//
// - "handles": `elements` (the scene; the map is arrayToMap(elements)),
//   `target`, and `variants`, each { zoom, pointerType, omitSides, result }:
//   getTransformHandles(element, { value: zoom }, elementsMap, pointerType,
//   omitSides). omitSides "default" leaves the argument out (the function's
//   DEFAULT_OMIT_SIDES); otherwise it is the object passed. result maps each
//   handle upstream returns (keys holding undefined dropped) to its
//   [x, y, width, height].
// - "handlesFromCoords": `variants`, each { coords, angle, zoom, pointerType,
//   omitSides, margin, spacing, result }: getTransformHandlesFromCoords with
//   those arguments (margin/spacing null = left out, the defaults 4 and
//   DEFAULT_TRANSFORM_HANDLE_SPACING).
// - "resizeTest": `elements`, `selected` (ids), and `variants`, each
//   { zoom, pointerType, editor, probes: [{ point, results, hit }] }:
//   resizeTest(element, elementsMap, { selectedElementIds }, x, y, zoom,
//   pointerType, editorInterface) for every element (results, by id, the
//   handle or false), and getElementWithTransformHandleType(elements, ...)
//   (hit: { element, handle } or null). `editor` names an editor interface
//   (EDITORS below).
// - "handleTypeFromCoords": `variants`, each { bounds, zoom, pointerType,
//   editor, probes: [{ point, result }] }:
//   getTransformHandleTypeFromCoords(bounds, x, y, zoom, pointerType,
//   editorInterface).
// - "cursor": `items`, each { element | null, handle, result }:
//   getCursorForResizingElement({ element, transformHandleType }).
// - "hasBoundingBox": `items`, each { elements, selectedLinearElement,
//   editor, result }: hasBoundingBox(elements, appState, editorInterface).
// - "resizeOffset": `items`, each { elements, selected, handle, point,
//   result }: getResizeOffsetXY(handle, selectedElements, elementsMap, x, y);
//   and { element, handle, direction } for getResizeArrowDirection.
// - "session": a pointer gesture on a scene, as the editor runs it.
//   `elements` is the scene before; `steps` are, in order:
//   - { type: "begin", selected, origin, zoom, pointerType, editor, result }:
//     App.initialPointerDownState and handleSelectionOnPointerDown
//     (App.tsx:9389-9430, 9515-9600): the original elements (deep copies of
//     the scene), the selection's common bounds centre, the handle
//     (getElementWithTransformHandleType for one element that is not an
//     elbow arrow nor a two-point line, getTransformHandleTypeFromCoords for
//     several) and getResizeOffsetXY. result: { handle, offset, center,
//     arrowDirection } (handle false when none).
//   - { type: "move", point, shift, alt, ctrl, gridSize, result, changed,
//     hooks }: App.maybeHandleResize (App.tsx:13684-13833) without snapping:
//     transformElements(originalElements, handle, selectedElements, scene,
//     shift, alt, proportionalByDefault ? !shift : shift, pointer - offset
//     (on the grid unless ctrl), center). result is what it returned (false
//     when maybeHandleResize refuses: a frame rotated, a lone elbow arrow).
//   - { type: "resizeSingle", target, nextWidth, nextHeight, handle,
//     options, orig: "current" | "snapshot", changed, hooks }: a direct
//     resizeSingleElement(nextWidth, nextHeight, latest, orig, origMap,
//     scene, handle, options) as resize.test.tsx calls it; "current" passes
//     a copy of the scene as it is (the element as its own original),
//     "snapshot" the copies taken by the last { type: "snapshot" } step.
//   - { type: "snapshot" }: copies the scene for later "snapshot" steps.
//   - { type: "setSelection" }: nothing but a marker between gestures.
//   changed: every element whose JSON differs after the step, by id, in
//   full (versionNonce and updated included; the Rust test ignores those
//   two, which come from upstream's random and clock).
//   hooks: the calls transformElements made to the parts that belong to
//   other tasks, in order, as the Rust port's TransformEnv is asked them:
//   - { hook: "updateBoundElements", changed, simultaneouslyUpdated,
//       changes }: binding.ts updateBoundElements (ex-510); changes are the
//       elements it rewrote, in full.
//   - { hook: "stickyNoteLayout", container, text, opts, result }:
//       stickyNote.ts getStickyNoteLayout (ex-703), with the arguments
//       updateStickyNoteLayout passed and what it returned.
//
// Cases:
// - upstream-*: the scenes and gestures of packages/element/tests/
//   resize.test.tsx (UI.resize presses the pointer on the centre of the
//   handle getTransformHandles(element, zoom, map, "mouse", {}) gives, or
//   the multi-selection handle of getTransformHandlesFromCoords, moves it by
//   the offset and releases), with "Architect" (roughness 0) as its
//   beforeEach picks.
// - rotate-*, rotated-*, sticky-*, zoom-*, group-*, frame-*, bound-*,
//   elbow-*: rotation (free and 15 degree steps, bound text, bindings),
//   rotated elements on every handle, sticky notes, zoom and pointer types,
//   groups, frames, bound arrows and elbow arrows.
// - random-*: seeded random scenes, selections, handles and pointer paths.
//
// Text is measured with upstream's test metric (text.length * 10), and the
// per-character width cache starts each gesture empty of every font (as on
// a fresh page; the patched charWidth.reset()), so a gesture's minimum label
// widths (getApproxMinLineWidth) depend on that gesture alone.
//
// Where UI.resize would press a side handle that is not laid out (a side
// too short, or a multi-selection, whose side handles are omitted), the
// pointer goes down on the middle of that side of the selection border,
// where the editor resizes from sides.
//
// Deterministic: upstream runs in its test mode (import.meta.env.MODE
// "test"; ids id0.., timestamps 1), reseed(1) before each case, and random
// scenes come from a Park-Miller generator seeded per case. Math.random
// throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { format } from "./lib/format.mjs";
import { apiCreateElement } from "./lib/restore-element-cases.mjs";
import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-editor", "tests", "fixtures");
export const FIXTURE = "transform.json";

const ENTRY = `
export {
  getTransformHandles,
  getTransformHandlesFromCoords,
  hasBoundingBox,
  DEFAULT_OMIT_SIDES,
  OMIT_SIDES_FOR_FRAME,
  OMIT_SIDES_FOR_MULTIPLE_ELEMENTS,
} from "./packages/element/src/transformHandles";
export {
  resizeTest,
  getElementWithTransformHandleType,
  getTransformHandleTypeFromCoords,
  getCursorForResizingElement,
} from "./packages/element/src/resizeTest";
export {
  transformElements,
  resizeSingleElement,
  getResizeOffsetXY,
  getResizeArrowDirection,
} from "./packages/element/src/resizeElements";
export { getCommonBounds, getElementAbsoluteCoords } from "./packages/element/src/bounds";
export { deepCopyElement } from "./packages/element/src/duplicate";
export { redrawTextBoundingBox } from "./packages/element/src/textElement";
export { Scene } from "./packages/element/src/Scene";
export { isElbowArrow, isLinearElement, isFrameLikeElement, isImageElement, isStickyNoteElement } from "./packages/element/src/typeChecks";
export { reseed } from "./packages/common/src/random";
export {
  arrayToMap,
  DEFAULT_VERTICAL_ALIGN,
  ROUNDNESS,
  getGridPoint,
  getStrokeWidthByKey,
  getUpdatedTimestamp,
} from "./packages/common/src/index";
export {
  newElement,
  newEmbeddableElement,
  newIframeElement,
  newStickyNoteElement,
  newFrameElement,
  newMagicFrameElement,
  newTextElement,
  newArrowElement,
  newLinearElement,
  newFreeDrawElement,
  newImageElement,
} from "./packages/element/src/newElement";
export { isUsingAdaptiveRadius } from "./packages/element/src/typeChecks";
export { setCustomTextMetricsProvider, charWidth } from "./packages/element/src/textMeasurements";
export { getDefaultAppState } from "./packages/excalidraw/appState";
`;

// The calls into binding.ts and stickyNote.ts that belong to other tasks go
// through globalThis.__transformProbe, which records them while a gesture
// runs (and only calls through otherwise). The functions called are
// upstream's own; only the call sites are rewritten.
const PROBE = "globalThis.__transformProbe";
const replaceAll = (source, from, to, expected) => {
  const parts = source.split(from);
  if (parts.length - 1 !== expected) {
    throw new Error(`expected ${expected} occurrences of ${JSON.stringify(from)}, found ${parts.length - 1}`);
  }
  return parts.join(to);
};
const PATCH = {
  "packages/element/src/resizeElements": (source) =>
    replaceAll(source, "updateBoundElements(", `${PROBE}.updateBoundElements(updateBoundElements, `, 5),
  "packages/element/src/stickyNote": (source) =>
    replaceAll(
      replaceAll(source, "updateBoundElements(\n", `${PROBE}.updateBoundElements(updateBoundElements, \n`, 1),
      "const layout = getStickyNoteLayout(container, textElement, layoutOpts);",
      `const layout = ${PROBE}.stickyNoteLayout(getStickyNoteLayout, container, textElement, layoutOpts);`,
      1,
    ),
  // charWidth.reset() forgets every font, as a fresh page starts: no
  // cache at all (getMaxCharWidth 0), where clearCache leaves an empty one
  // (getMaxCharWidth -Infinity). The fixture resets before each gesture
  // and the Rust test starts each gesture with a new CharWidthCache.
  "packages/element/src/textMeasurements": (source) =>
    replaceAll(
      source,
      "    clearCache,\n  };",
      "    clearCache,\n    reset: () => {\n      for (const font of Object.keys(cachedCharWidth)) delete cachedCharWidth[font];\n    },\n  };",
      1,
    ),
};

const usage = () => {
  process.stderr.write("usage: transform-fixtures.mjs [--check] [--out DIR]\n");
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

/** roughjs' Random (Park-Miller, multiplier 48271), seeded per case. */
const rng = (seed) => {
  let state = seed;
  const next = () => {
    state = Math.imul(48271, state);
    return ((2 ** 31 - 1) & state) / 2 ** 31;
  };
  return {
    next,
    int: (lo, hi) => lo + Math.floor(next() * (hi - lo + 1)),
    pick: (items) => items[Math.floor(next() * items.length)],
    chance: (p) => next() < p,
    range: (lo, hi) => lo + next() * (hi - lo),
  };
};

/** Rounds to 1/64 so random geometry stays exactly representable. */
const q = (v) => Math.round(v * 64) / 64;

// -- editor interfaces ----------------------------------------------------------

const editorInterface = (formFactor, isMobileDevice) => ({
  formFactor,
  desktopUIMode: "full",
  userAgent: { isMobileDevice, platform: "other" },
  isTouchScreen: isMobileDevice,
  canFitSidebar: formFactor === "desktop",
  isLandscape: true,
});

const EDITORS = {
  desktop: editorInterface("desktop", false),
  "phone-mobile": editorInterface("phone", true),
  "phone-desktop-agent": editorInterface("phone", false),
  "tablet-mobile": editorInterface("tablet", true),
};

// -- the probe -------------------------------------------------------------------

const probe = {
  active: null,
  snapshot(scene) {
    return new Map(scene.getElementsIncludingDeleted().map((e) => [e.id, JSON.stringify(e)]));
  },
  updateBoundElements(fn, changedElement, scene, options) {
    if (!this.active) return fn(changedElement, scene, options);
    const before = this.snapshot(scene);
    const call = {
      hook: "updateBoundElements",
      changed: changedElement.id,
      simultaneouslyUpdated: options?.simultaneouslyUpdated
        ? options.simultaneouslyUpdated.map((e) => e.id)
        : null,
    };
    this.active.push(call);
    const result = fn(changedElement, scene, options);
    call.changes = scene
      .getElementsIncludingDeleted()
      .filter((e) => before.get(e.id) !== JSON.stringify(e))
      .map(clone);
    return result;
  },
  stickyNoteLayout(fn, container, textElement, opts) {
    const result = fn(container, textElement, opts);
    if (this.active) {
      this.active.push({
        hook: "stickyNoteLayout",
        container: clone(container),
        text: clone(textElement),
        opts: clone(opts ?? {}),
        result: clone(result),
      });
    }
    return result;
  },
};

// -- scenes ----------------------------------------------------------------------

/** A pristine character width cache (see the textMeasurements patch). */
const clearCharWidths = (up) => up.charWidth.reset();

/** A text element measured as newTextElement measures it. */
const text = (up, opts) =>
  up.newTextElement({
    x: 0,
    y: 0,
    strokeColor: "#1e1e1e",
    backgroundColor: "transparent",
    fillStyle: "solid",
    strokeWidth: 2,
    strokeStyle: "solid",
    roughness: 0,
    opacity: 100,
    fontSize: 20,
    fontFamily: 5,
    textAlign: "left",
    verticalAlign: up.DEFAULT_VERTICAL_ALIGN,
    ...opts,
  });

/** API.createElement with the "Architect" roughness resize.test.tsx picks. */
const el = (up, opts) => apiCreateElement(up, { roughness: 0, ...opts });

/**
 * Binds `label` (a text) to `container` and lays it out as the text editor
 * does when it commits (redrawTextBoundingBox).
 */
const bindLabel = (up, elements, container, labelText, opts = {}) => {
  const label = text(up, {
    text: labelText,
    originalText: labelText,
    containerId: container.id,
    textAlign: "center",
    verticalAlign: "middle",
    ...opts,
  });
  container.boundElements = [...(container.boundElements ?? []), { id: label.id, type: "text" }];
  elements.push(label);
  const scene = new up.Scene(elements, { skipValidation: true });
  up.redrawTextBoundingBox(label, container, scene);
  return label;
};

const bindArrow = (arrow, startOrEnd, target, fixedPoint, mode = "orbit") => {
  arrow[startOrEnd === "start" ? "startBinding" : "endBinding"] = {
    elementId: target.id,
    fixedPoint,
    mode,
  };
  target.boundElements = [...(target.boundElements ?? []), { id: arrow.id, type: "arrow" }];
};

// -- handle geometry -----------------------------------------------------------

const handlesJson = (handles) => {
  const out = {};
  for (const [key, value] of Object.entries(handles)) {
    if (value !== undefined) out[key] = value;
  }
  return out;
};

const handleCentre = (handle) => [handle[0] + handle[2] / 2, handle[1] + handle[3] / 2];

/** Where UI.resize presses the pointer (ui.ts:346-399). */
const uiHandlePoint = (up, elements, selected, handle, zoom = 1) => {
  const sel = elements.filter((e) => selected.includes(e.id));
  let coords;
  if (sel.length === 1) {
    coords = up.getTransformHandles(sel[0], { value: zoom }, up.arrayToMap(elements), "mouse", {})[handle];
  } else {
    const [x1, y1, x2, y2] = up.getCommonBounds(sel);
    const isFrameSelected = sel.some(up.isFrameLikeElement);
    coords = up.getTransformHandlesFromCoords(
      [x1, y1, x2, y2, (x1 + x2) / 2, (y1 + y2) / 2],
      0,
      { value: zoom },
      "mouse",
      isFrameSelected ? up.OMIT_SIDES_FOR_FRAME : up.OMIT_SIDES_FOR_MULTIPLE_ELEMENTS,
    )[handle];
  }
  if (!coords && handle.length === 1) {
    // no side handle (too small, or a multi-selection, whose side handles
    // are omitted): press on the middle of that side of the selection
    // border, where resizeTest and getTransformHandleTypeFromCoords look
    // (the box pushed out by SIDE_RESIZING_THRESHOLD / zoom, not for one
    // image); on the line, not at the edge of its threshold
    const [x1, y1, x2, y2] =
      sel.length === 1 ? up.getElementAbsoluteCoords(sel[0], up.arrayToMap(elements)) : up.getCommonBounds(sel);
    const cx = (x1 + x2) / 2;
    const cy = (y1 + y2) / 2;
    const out = sel.length === 1 && up.isImageElement(sel[0]) ? 0 : 4 / zoom;
    const side = { n: [cx, y1 - out], s: [cx, y2 + out], w: [x1 - out, cy], e: [x2 + out, cy] }[handle];
    const angle = sel.length === 1 ? sel[0].angle : 0;
    return [
      (side[0] - cx) * Math.cos(angle) - (side[1] - cy) * Math.sin(angle) + cx,
      (side[0] - cx) * Math.sin(angle) + (side[1] - cy) * Math.cos(angle) + cy,
    ];
  }
  if (!coords) throw new Error(`no ${handle} handle`);
  return handleCentre(coords);
};

// -- sessions --------------------------------------------------------------------

/**
 * Runs a gesture script on `elements`: see the "session" kind above. Each
 * step of `script` is { begin: { selected, handle | origin, zoom,
 * pointerType, editor } } | { move: [dx, dy] | { to }, shift, alt, ctrl,
 * gridSize } | { resizeSingle } | { snapshot: true }.
 */
const session = (up, id, elements, script) => {
  const scene = new up.Scene(elements, { skipValidation: true });
  // the scene syncs fractional indices as it takes the elements
  const initial = clone(scene.getElementsIncludingDeleted());
  const steps = [];
  let state = null;
  let snapshot = null;

  const run = (fn) => {
    const before = probe.snapshot(scene);
    const hooks = [];
    probe.active = hooks;
    let result;
    try {
      result = fn();
    } finally {
      probe.active = null;
    }
    const changed = scene
      .getElementsIncludingDeleted()
      .filter((e) => before.get(e.id) !== JSON.stringify(e))
      .map(clone);
    return { result, changed, hooks };
  };

  for (const step of script) {
    if (step.begin) {
      const { selected, zoom = 1, pointerType = "mouse", editor = "desktop" } = step.begin;
      // appState.selectedLinearElement: only the two fields
      // handleSelectionOnPointerDown reads (App.tsx:9524-9537)
      const selectedLinearElement = step.begin.selectedLinearElement ?? null;
      const editorIf = EDITORS[editor];
      const pressAt = () => {
        try {
          return uiHandlePoint(up, scene.getNonDeletedElements(), selected, step.begin.handle, zoom);
        } catch (error) {
          if (!step.begin.lenient) throw error;
          // random gestures: the handle is not there (a frame's rotation
          // handle, a locked element); press on the selection's centre
          const [x1, y1, x2, y2] = up.getCommonBounds(scene.getNonDeletedElements().filter((e) => selected.includes(e.id)));
          return [(x1 + x2) / 2, (y1 + y2) / 2];
        }
      };
      const origin = step.begin.origin ?? pressAt();
      const all = scene.getNonDeletedElements();
      const elementsMap = scene.getNonDeletedElementsMap();
      const sel = all.filter((e) => selected.includes(e.id));
      const [minX, minY, maxX, maxY] = up.getCommonBounds(sel);
      const originalElements = all.reduce((acc, e) => {
        acc.set(e.id, up.deepCopyElement(e));
        return acc;
      }, new Map());
      const appState = { selectedElementIds: Object.fromEntries(selected.map((s) => [s, true])) };
      let handle = false;
      if (
        sel.length === 1 &&
        !selectedLinearElement?.isEditing &&
        !up.isElbowArrow(sel[0]) &&
        !(up.isLinearElement(sel[0]) && (editorIf.userAgent.isMobileDevice || sel[0].points.length === 2)) &&
        !(selectedLinearElement && selectedLinearElement.hoverPointIndex !== -1)
      ) {
        const hit = up.getElementWithTransformHandleType(
          all,
          appState,
          origin[0],
          origin[1],
          { value: zoom },
          pointerType,
          elementsMap,
          editorIf,
        );
        if (hit) handle = hit.transformHandleType;
      } else if (sel.length > 1) {
        handle = up.getTransformHandleTypeFromCoords(
          up.getCommonBounds(sel),
          origin[0],
          origin[1],
          { value: zoom },
          pointerType,
          editorIf,
        );
      }
      let offset = [0, 0];
      let arrowDirection = "origin";
      if (handle) {
        offset = up.getResizeOffsetXY(handle, sel, elementsMap, origin[0], origin[1]);
        if (sel.length === 1 && up.isLinearElement(sel[0]) && sel[0].points.length === 2) {
          arrowDirection = up.getResizeArrowDirection(handle, sel[0]);
        }
      }
      const center = [(maxX + minX) / 2, (maxY + minY) / 2];
      state = { selected, originalElements, handle, offset, center, origin };
      clearCharWidths(up);
      steps.push({
        type: "begin",
        selected,
        origin,
        zoom,
        pointerType,
        editor,
        selectedLinearElement,
        result: { handle, offset, center, arrowDirection },
      });
    } else if (step.move) {
      const { shift = false, alt = false, ctrl = false, gridSize = null } = step;
      const point = Array.isArray(step.move)
        ? [state.origin[0] + step.move[0], state.origin[1] + step.move[1]]
        : step.move.to;
      const { result, changed, hooks } = run(() => {
        // App only calls maybeHandleResize while resizing (App.tsx:10824)
        if (!state.handle) return false;
        const sel = scene.getSelectedElements({
          selectedElementIds: Object.fromEntries(state.selected.map((s) => [s, true])),
        });
        const handle = state.handle;
        const frames = sel.filter(up.isFrameLikeElement);
        if ((frames.length > 0 && handle === "rotation") || (sel.length === 1 && up.isElbowArrow(sel[0]))) {
          return false;
        }
        const [resizeX, resizeY] = up.getGridPoint(
          point[0] - state.offset[0],
          point[1] - state.offset[1],
          ctrl ? null : gridSize,
        );
        const proportionalByDefault =
          sel.some((e) => up.isImageElement(e)) ||
          (sel.length === 1 && up.isStickyNoteElement(sel[0]) && typeof handle === "string" && handle.length === 2);
        return up.transformElements(
          state.originalElements,
          handle,
          sel,
          scene,
          shift,
          alt,
          proportionalByDefault ? !shift : shift,
          resizeX,
          resizeY,
          state.center[0],
          state.center[1],
        );
      });
      steps.push({ type: "move", point, shift, alt, ctrl, gridSize, result, changed, hooks });
    } else if (step.snapshot) {
      snapshot = up.arrayToMap(scene.getNonDeletedElements().map((e) => ({ ...e })));
      clearCharWidths(up);
      steps.push({ type: "snapshot" });
    } else if (step.resizeSingle) {
      const { target, handle, options = {}, orig = "current" } = step.resizeSingle;
      const current = scene.getNonDeletedElementsMap().get(target);
      const size = (v) => (typeof v === "function" ? v(current) : v);
      const nextWidth = size(step.resizeSingle.nextWidth);
      const nextHeight = size(step.resizeSingle.nextHeight);
      const origMap =
        orig === "snapshot" ? snapshot : up.arrayToMap(scene.getNonDeletedElements().map((e) => ({ ...e })));
      if (orig === "current") clearCharWidths(up);
      const { changed, hooks } = run(() =>
        up.resizeSingleElement(
          nextWidth,
          nextHeight,
          scene.getNonDeletedElementsMap().get(target),
          origMap.get(target),
          origMap,
          scene,
          handle,
          options,
        ),
      );
      steps.push({ type: "resizeSingle", target, nextWidth, nextHeight, handle, options, orig, changed, hooks });
    } else {
      throw new Error(`unknown step in ${id}`);
    }
  }
  return { id, kind: "session", elements: initial, steps };
};

/** One UI.resize: select, press on the handle, move by `move`, release. */
const resize = (selected, handle, move, modifiers = {}, begin = {}) => [
  { begin: { selected, handle, ...begin } },
  { move, ...modifiers },
];

// -- upstream's scenes (resize.test.tsx) -------------------------------------------

const upstreamCases = () => {
  const cases = [];
  const add = (id, build) => cases.push({ id, build });

  const resizes = [
    ["n", [10, -27]],
    ["e", [67, -45]],
    ["s", [-50, -39]],
    ["w", [20, 90]],
    ["ne", [5, -33]],
    ["se", [-30, -81]],
    ["sw", [37, 25]],
    ["nw", [-34, 42]],
  ];
  const flips = [
    ["n", [15, 139]],
    ["e", [-245, 67]],
    ["s", [-26, -210]],
    ["w", [241, 0]],
    ["ne", [-250, 125]],
    ["se", [-283, -58]],
    ["sw", [40, -123]],
    ["nw", [270, 133]],
  ];
  for (const [name, table] of [
    ["resize", resizes],
    ["flip", flips],
  ]) {
    for (const [handle, move] of table) {
      const id = `upstream-generic-${name}-${handle}`;
      add(id, (up) => {
        const r = el(up, { type: "rectangle", width: 200, height: 100 });
        return session(up, id, [r], resize([r.id], handle, move));
      });
    }
  }
  add("upstream-generic-aspect", (up) => {
    const r = el(up, { type: "rectangle", width: 200, height: 100 });
    return session(up, "upstream-generic-aspect", [r], [
      ...resize([r.id], "se", [100, 10], { shift: true }),
      ...resize([r.id], "n", [30, 50], { shift: true }),
    ]);
  });
  add("upstream-generic-center", (up) => {
    const r = el(up, { type: "rectangle", width: 200, height: 100 });
    return session(up, "upstream-generic-center", [r], [
      ...resize([r.id], "nw", [20, 10], { alt: true }),
      ...resize([r.id], "e", [15, 43], { alt: true }),
    ]);
  });
  add("upstream-generic-label", (up) => {
    const r = el(up, { type: "rectangle", width: 200, height: 100 });
    const elements = [r];
    bindLabel(up, elements, r, "Hello world");
    return session(up, "upstream-generic-label", elements, [
      ...resize([r.id], "se", [50, 50]),
      ...resize([r.id], "w", [190, 0]),
    ]);
  });
  for (const type of ["rectangle", "ellipse", "diamond"]) {
    for (const proportional of [false, true]) {
      const id = `upstream-${type}-label-flip-${proportional ? "proportional" : "free"}`;
      add(id, (up) => {
        const c = el(up, { type, width: 200, height: 100 });
        const elements = [c];
        bindLabel(up, elements, c, "Hello");
        return session(up, id, elements, resize([c.id], proportional ? "se" : "e", [-500, 0], { shift: proportional }));
      });
    }
  }
  for (const [handle, move] of [
    ["n", [0, 100]],
    ["s", [0, -100]],
  ]) {
    const id = `upstream-multiline-label-center-${handle}`;
    add(id, (up) => {
      const r = el(up, { type: "rectangle", width: 200, height: 100 });
      const elements = [r];
      bindLabel(up, elements, r, "hello\nhello\nhello\nhello\nhello");
      return session(up, id, elements, resize([r.id], handle, move, { alt: true }));
    });
  }
  add("upstream-flipped-corner-anchored", (up) => {
    const r = el(up, { type: "rectangle", width: 200, height: 200 });
    const elements = [r];
    bindLabel(up, elements, r, "first second third fourth");
    return session(up, "upstream-flipped-corner-anchored", elements, [
      { snapshot: true },
      ...[-10, -40, -80, -200].map((nextHeight) => ({
        resizeSingle: { target: r.id, nextWidth: 80, nextHeight, handle: "se", orig: "snapshot" },
      })),
    ]);
  });

  const polyPoints = {
    line: [
      [0, 0],
      [60, -20],
      [20, 40],
      [-40, 0],
    ],
    freedraw: [
      [0, 0],
      [-2.474600807561444, 41.021700699972],
      [3.6627956000014024, 47.84174560617245],
      [40.495224145598115, 47.15909710753482],
    ],
  };
  for (const type of ["line", "freedraw"]) {
    const make = (up) =>
      el(up, { type, points: polyPoints[type], ...up_size(polyPoints[type]), roundness: null });
    for (const [name, handle, move, mods] of [
      ["resize", "ne", [30, -60], {}],
      ["flip", "sw", [140, -80], {}],
      ["aspect", "ne", [30, -60], { shift: true }],
      ["center", "nw", [-20, -30], { alt: true }],
    ]) {
      const id = `upstream-${type}-${name}`;
      add(id, (up) => {
        const e = make(up);
        return session(up, id, [e], resize([e.id], handle, move, mods));
      });
    }
  }
  add("upstream-line-direct-resize", (up) => {
    const e = el(up, { type: "line", points: polyPoints.line, ...up_size(polyPoints.line), roundness: null });
    return session(up, "upstream-line-direct-resize", [e], [
      { resizeSingle: { target: e.id, nextWidth: e.width + 30, nextHeight: e.height + 30, handle: "ne" } },
    ]);
  });
  add("upstream-line-direct-flip", (up) => {
    const e = el(up, { type: "line", points: polyPoints.line, ...up_size(polyPoints.line), roundness: null });
    return session(up, "upstream-line-direct-flip", [e], [
      { resizeSingle: { target: e.id, nextWidth: -e.width, nextHeight: -e.height, handle: "se" } },
    ]);
  });
  add("upstream-line-direct-center", (up) => {
    const points = [
      [0, 0],
      [338.05644048727373, -180.4761618151104],
      [338.05644048727373, 180.4761618151104],
      [-338.05644048727373, 180.4761618151104],
      [-338.05644048727373, -180.4761618151104],
    ];
    const e = el(up, { type: "line", points, ...up_size(points), roundness: null });
    return session(up, "upstream-line-direct-center", [e], [
      {
        resizeSingle: {
          target: e.id,
          nextWidth: e.width + 20,
          nextHeight: e.height,
          handle: "e",
          options: { shouldResizeFromCenter: true },
        },
      },
    ]);
  });
  add("upstream-arrow-label", (up) => {
    const points = [
      [0, 0],
      [40, 140],
      [80, 60],
      [180, 20],
      [200, 120],
    ];
    const a = el(up, { type: "arrow", points, ...up_size(points), roundness: null, endArrowhead: "arrow" });
    const elements = [a];
    bindLabel(up, elements, a, "Hello");
    return session(up, "upstream-arrow-label", elements, [
      ...resize([a.id], "se", [50, 30]),
      ...resize([a.id], "w", [20, 0]),
      { snapshot: true },
      {
        resizeSingle: {
          target: a.id,
          nextWidth: (e) => -2 * e.width,
          nextHeight: (e) => 2 * e.height,
          handle: "se",
          options: { shouldMaintainAspectRatio: true },
          orig: "snapshot",
        },
      },
    ]);
  });
  for (const [name, selectBoth, handle, move] of [
    ["single", false, "se", [-200, -150]],
    ["group", true, "nw", [300, 350]],
  ]) {
    const id = `upstream-elbow-fixed-point-${name}`;
    add(id, (up) => {
      const r = el(up, { type: "rectangle", x: -100, y: -75, width: 95, height: 100 });
      const a = el(up, {
        type: "arrow",
        x: -5,
        y: 0,
        points: [
          [0, 0],
          [125, 200],
        ],
        width: 125,
        height: 200,
        elbowed: true,
        roundness: null,
        endArrowhead: "arrow",
      });
      bindArrow(a, "start", r, [1.0526315789473684, 0.75]);
      const elements = [r, a];
      const scene = new up.Scene(elements, { skipValidation: true });
      scene.mutateElement(a, { points: a.points });
      return session(up, id, elements, resize(selectBoth ? [r.id, a.id] : [r.id], handle, move));
    });
  }

  const newText = (up, value, opts = {}) => text(up, { text: value, originalText: value, ...opts });
  add("upstream-text-resize", (up) => {
    const t = newText(up, "hello\nworld");
    return session(up, "upstream-text-resize", [t], resize([t.id], "se", [30, 40]));
  });
  add("upstream-text-e", (up) => {
    const t = newText(up, "Excalidraw\nEditor");
    return session(up, "upstream-text-e", [t], [
      ...resize([t.id], "e", [30, 0]),
      ...resize([t.id], "e", [-30, 0]),
    ]);
  });
  add("upstream-text-w", (up) => {
    const t = newText(up, "Excalidraw\nEditor");
    return session(up, "upstream-text-w", [t], [
      ...resize([t.id], "w", [-50, 0]),
      ...resize([t.id], "w", [50, 0]),
    ]);
  });
  add("upstream-text-wraps", (up) => {
    const t = newText(up, "Excalidraw\nEditor");
    return session(up, "upstream-text-wraps", [t], [
      ...resize([t.id], "w", [50, 0]),
      ...resize([t.id], "w", [-50, 0]),
      ...resize([t.id], "e", [-20, 0]),
      ...resize([t.id], "e", [20, 0]),
      ...resize([t.id], "e", [-60, 0]),
      ...resize([t.id], "e", [60, 0]),
    ]);
  });
  add("upstream-text-min-width", (up) => {
    const t = newText(up, "Excalidraw\nEditor");
    const width = t.width;
    return session(up, "upstream-text-min-width", [t], [
      ...resize([t.id], "e", [-width, 0]),
      ...resize([t.id], "e", [width - 30, 0]),
      ...resize([t.id], "w", [width, 0]),
      ...resize([t.id], "w", [-width + 30, 0]),
    ]);
  });

  const image = (up, opts) => el(up, { type: "image", width: 100, height: 100, ...opts });
  for (const [name, handle, move, mods] of [
    ["resize", "ne", [-20, -30], {}],
    ["flip", "sw", [150, -150], {}],
    ["center", "nw", [25, 15], { alt: true }],
  ]) {
    const id = `upstream-image-${name}`;
    add(id, (up) => {
      const i = image(up);
      return session(up, id, [i], resize([i.id], handle, move, mods));
    });
  }
  add("upstream-image-aspect", (up) => {
    const i = image(up);
    return session(up, "upstream-image-aspect", [i], [
      ...resize([i.id], "ne", [30, -20]),
      ...resize([i.id], "ne", [-30, 50], { shift: true }),
    ]);
  });

  add("upstream-multi-text-container-anchored", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 200, height: 35 });
    const elements = [r];
    bindLabel(up, elements, r, "hello");
    const other = el(up, { type: "rectangle", x: 300, y: 0, width: 100, height: 100 });
    elements.push(other);
    return session(up, "upstream-multi-text-container-anchored", elements, resize([r.id, other.id], "se", [-200, -150], { shift: true }));
  });
  add("upstream-multi-generic", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 80 });
    const elements = [r];
    bindLabel(up, elements, r, "hello\nworld");
    const d = el(up, { type: "diamond", x: 140, y: 40, width: 80, height: 80 });
    const e = el(up, { type: "ellipse", x: 40, y: 100, width: 80, height: 60 });
    elements.push(d, e);
    return session(up, "upstream-multi-generic", elements, resize([r.id, d.id, e.id], "se", [50, 30], { shift: true }));
  });
  add("upstream-multi-linear", (up) => {
    const linePoints = [
      [0, 0],
      [-40, 40],
      [-60, 0],
      [0, -40],
      [40, 20],
      [0, 40],
    ];
    const drawPoints = [
      [0, 0],
      [-43.56072661326618, 18.15048126846341],
      [-43.56072661326618, 29.041198460587566],
      [-38.115368017204105, 42.652452795512204],
      [-19.964886748740696, 66.24829266003775],
      [19.056612930986716, 77.1390098521619],
    ];
    const line = el(up, { type: "line", x: 60, y: 40, points: linePoints, ...up_size(linePoints), roundness: null });
    const draw = el(up, { type: "freedraw", x: 63.56072661326618, y: 100, points: drawPoints, ...up_size(drawPoints) });
    return session(up, "upstream-multi-linear", [line, draw], resize([line.id, draw.id], "se", [-25, -25], { shift: true }));
  });
  add("upstream-multi-2-point-lines", (up) => {
    const two = (x, y, w, h) =>
      el(up, { type: "line", x, y, width: w, height: h, roundness: null, points: [[0, 0], [w, h]] });
    const lines = [two(0, 0, 120, 0), two(0, 20, 0, 80), two(40, 40, 60, 60)];
    return session(up, "upstream-multi-2-point-lines", lines, resize(lines.map((l) => l.id), "nw", [40, 40], { shift: true }));
  });
  add("upstream-multi-labeled-arrows", (up) => {
    const arrow = (y) =>
      el(up, {
        type: "arrow",
        x: 0,
        y,
        width: 220,
        height: 0,
        roundness: null,
        endArrowhead: "arrow",
        points: [
          [0, 0],
          [220, 0],
        ],
      });
    const top = arrow(20);
    const elements = [top];
    bindLabel(up, elements, top, "lorem ipsum");
    const bottom = arrow(80);
    elements.push(bottom);
    bindLabel(up, elements, bottom, "dolor\nsit amet", { fontSize: 28 });
    return session(up, "upstream-multi-labeled-arrows", elements, resize([top.id, bottom.id], "se", [80, 0], { shift: true }));
  });
  add("upstream-multi-text", (up) => {
    const top = newText(up, "lorem ipsum", { x: 0, y: 0 });
    const bottom = newText(up, "dolor\nsit amet", { x: 40, y: 40, fontSize: 28 });
    return session(up, "upstream-multi-text", [top, bottom], resize([top.id, bottom.id], "ne", [30, -40], { shift: true }));
  });
  add("upstream-multi-images", (up) => {
    const top = image(up, { x: 0, y: 0, width: 200, height: 100 });
    const bottom = image(up, { x: 30, y: 150, width: 120, height: 80 });
    return session(up, "upstream-multi-images", [top, bottom], resize([top.id, bottom.id], "se", [-50, -50]));
  });
  add("upstream-multi-center", (up) => {
    const r = el(up, { type: "rectangle", x: -200, y: -140, width: 120, height: 100 });
    const e = el(up, { type: "ellipse", x: 60, y: 60, width: 140, height: 80 });
    return session(up, "upstream-multi-center", [r, e], resize([r.id, e.id], "se", [-80, -80], { shift: true, alt: true }));
  });
  add("upstream-multi-flip", (up) => {
    const img = image(up, { x: 60, y: 100, width: 100, height: 100, angle: (Math.PI * 7) / 6 });
    const linePoints = [
      [0, 0],
      [-40, 40],
      [-20, 60],
      [20, 20],
      [40, 40],
      [-20, 100],
      [-60, 60],
    ];
    const line = el(up, { type: "line", x: 60, y: 0, points: linePoints, ...up_size(linePoints), roundness: null });
    const r = el(up, { type: "rectangle", x: 180, y: 60, width: 160, height: 80, angle: Math.PI / 6 });
    const elements = [img, line, r];
    bindLabel(up, elements, r, "hello\nworld");
    const a = el(up, {
      type: "arrow",
      x: 380,
      y: 240,
      width: 60,
      height: 80,
      roundness: null,
      endArrowhead: "arrow",
      points: [
        [0, 0],
        [-60, -80],
      ],
    });
    elements.push(a);
    bindLabel(up, elements, a, "test");
    return session(up, "upstream-multi-flip", elements, resize([line.id, img.id, r.id, a.id], "se", [-800, 0], { shift: true }));
  });
  return cases;
};

/** getSizeFromPoints. */
const up_size = (points) => {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  return { width: Math.max(...xs) - Math.min(...xs), height: Math.max(...ys) - Math.min(...ys) };
};

// -- more gestures -----------------------------------------------------------------

const HANDLES8 = ["n", "s", "w", "e", "nw", "ne", "sw", "se"];

const extraCases = () => {
  const cases = [];
  const add = (id, build) => cases.push({ id, build });

  // rotation: a lone element, free and in 15 degree steps, with a label
  for (const shift of [false, true]) {
    const id = `rotate-single-${shift ? "discrete" : "free"}`;
    add(id, (up) => {
      const r = el(up, { type: "rectangle", x: 10, y: 20, width: 200, height: 100 });
      const elements = [r];
      bindLabel(up, elements, r, "rotate me");
      return session(up, id, elements, [
        { begin: { selected: [r.id], handle: "rotation" } },
        { move: [80, 40], shift },
        { move: [150, 160], shift },
        { move: [-90, 200], shift },
        { move: [-200, -30], shift },
      ]);
    });
  }
  // a container whose boundElements still lists a deleted label: upstream
  // reads it with scene.getElement, deleted or not, and turns and moves it
  add("rotate-deleted-label", (up) => {
    const r = el(up, { type: "rectangle", x: 10, y: 20, width: 200, height: 100 });
    const elements = [r];
    const label = bindLabel(up, elements, r, "deleted label");
    label.isDeleted = true;
    return session(up, "rotate-deleted-label", elements, [
      { begin: { selected: [r.id], handle: "rotation" } },
      { move: [80, 40] },
      { move: [-90, 200] },
    ]);
  });
  // rotating a bound arrow unbinds it; rotating its label follows
  add("rotate-bound-arrow", (up) => {
    const a = el(up, {
      type: "arrow",
      x: 0,
      y: 0,
      width: 200,
      height: 100,
      roundness: null,
      endArrowhead: "arrow",
      points: [
        [0, 0],
        [120, 40],
        [200, 100],
      ],
    });
    const r1 = el(up, { type: "rectangle", x: -120, y: -50, width: 100, height: 100 });
    const r2 = el(up, { type: "ellipse", x: 210, y: 60, width: 100, height: 100 });
    bindArrow(a, "start", r1, [1.05, 0.5001]);
    bindArrow(a, "end", r2, [-0.05, 0.4]);
    const elements = [r1, r2, a];
    bindLabel(up, elements, a, "label");
    return session(up, "rotate-bound-arrow", elements, [
      { begin: { selected: [a.id], handle: "rotation" } },
      { move: [60, 30] },
    ]);
  });
  // an arrow bound at both ends to the same element
  add("rotate-arrow-same-target", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 100 });
    const a = el(up, {
      type: "arrow",
      x: 110,
      y: 20,
      width: 60,
      height: 60,
      roundness: null,
      points: [
        [0, 0],
        [60, 30],
        [0, 60],
      ],
    });
    bindArrow(a, "start", r, [1.1, 0.2]);
    bindArrow(a, "end", r, [1.1, 0.8]);
    return session(up, "rotate-arrow-same-target", [r, a], [
      { begin: { selected: [a.id], handle: "rotation" } },
      { move: [30, 10] },
    ]);
  });
  // several elements about the selection centre, elbow arrows re-routed
  for (const shift of [false, true]) {
    const id = `rotate-multi-${shift ? "discrete" : "free"}`;
    add(id, (up) => {
      const r = el(up, { type: "rectangle", x: 0, y: 0, width: 120, height: 80, angle: 0.3 });
      const elements = [r];
      bindLabel(up, elements, r, "one two");
      const d = el(up, { type: "diamond", x: 200, y: 50, width: 80, height: 100 });
      const t = text(up, { text: "free text", originalText: "free text", x: 40, y: 200 });
      const line = el(up, {
        type: "line",
        x: 150,
        y: 150,
        width: 60,
        height: 40,
        roundness: { type: 2 },
        points: [
          [0, 0],
          [30, 40],
          [60, 0],
        ],
      });
      const a = el(up, {
        type: "arrow",
        x: 120,
        y: 40,
        width: 80,
        height: 60,
        elbowed: true,
        roundness: null,
        endArrowhead: "arrow",
        points: [
          [0, 0],
          [80, 60],
        ],
      });
      bindArrow(a, "start", r, [1.0526, 0.5001]);
      bindArrow(a, "end", d, [-0.0625, 0.25]);
      const outside = el(up, { type: "ellipse", x: 400, y: 400, width: 50, height: 50 });
      const b = el(up, {
        type: "arrow",
        x: 280,
        y: 100,
        width: 120,
        height: 300,
        roundness: null,
        points: [
          [0, 0],
          [120, 300],
        ],
      });
      bindArrow(b, "start", d, [1.05, 0.5001]);
      bindArrow(b, "end", outside, [0.5001, -0.1]);
      elements.push(d, t, line, a, outside, b);
      const scene = new up.Scene(elements, { skipValidation: true });
      scene.mutateElement(a, { points: a.points });
      return session(up, id, elements, [
        { begin: { selected: [r.id, d.id, t.id, line.id, a.id, b.id], handle: "rotation" } },
        ...[
          [40, 20],
          [120, 150],
          [-60, 90],
        ].map((move) => ({ move, shift })),
      ]);
    });
  }
  // frames cannot be rotated; a single frame has no rotation handle
  add("frame-rotate-multi-refused", (up) => {
    const f = el(up, { type: "frame", x: 0, y: 0, width: 200, height: 200 });
    const r = el(up, { type: "rectangle", x: 250, y: 0, width: 80, height: 80 });
    return session(up, "frame-rotate-multi-refused", [f, r], [
      { begin: { selected: [f.id, r.id], origin: uiHandlePointFrom(up, [f, r], "rotation") } },
      { move: [50, 50] },
      ...resize([f.id, r.id], "se", [40, 30]),
    ]);
  });
  add("frame-single", (up) => {
    const f = el(up, { type: "magicframe", x: 0, y: 0, width: 200, height: 150 });
    return session(up, "frame-single", [f], [
      { begin: { selected: [f.id], origin: [100, -30] } },
      { move: [60, 60] },
      ...resize([f.id], "sw", [-40, 25]),
      ...resize([f.id], "e", [-300, 0]),
    ]);
  });
  // a lone multi-point line has handles, but not while the linear element
  // editor edits it or a point of it is hovered (App.tsx:9524-9537)
  for (const [name, selectedLinearElement] of [
    ["not-editing", { isEditing: false, hoverPointIndex: -1 }],
    ["editing", { isEditing: true, hoverPointIndex: -1 }],
    ["hover-point", { isEditing: false, hoverPointIndex: 1 }],
    ["editing-hover-point", { isEditing: true, hoverPointIndex: 2 }],
  ]) {
    const id = `line-selected-linear-${name}`;
    add(id, (up) => {
      const l = el(up, {
        type: "line",
        x: 0,
        y: 0,
        width: 160,
        height: 90,
        roundness: null,
        points: [
          [0, 0],
          [80, 90],
          [160, 20],
        ],
      });
      return session(up, id, [l], [
        { begin: { selected: [l.id], handle: "se", selectedLinearElement } },
        { move: [40, 30] },
        { move: [-20, 60] },
      ]);
    });
  }
  // a lone elbow arrow is never transformed
  add("elbow-single-refused", (up) => {
    const a = el(up, {
      type: "arrow",
      x: 0,
      y: 0,
      width: 100,
      height: 80,
      elbowed: true,
      roundness: null,
      points: [
        [0, 0],
        [100, 80],
      ],
    });
    const scene = new up.Scene([a], { skipValidation: true });
    scene.mutateElement(a, { points: a.points });
    return session(up, "elbow-single-refused", [a], [
      { begin: { selected: [a.id], origin: [100, 80] } },
      { move: [20, 20] },
    ]);
  });
  // rotated elements on every handle and modifier
  for (const [type, angle] of [
    ["rectangle", 0.4],
    ["ellipse", 2.2],
    ["diamond", 4.1],
    ["image", 5.5],
    ["text", 1.1],
    ["freedraw", 0.9],
    ["line", 3.3],
  ]) {
    for (const handle of HANDLES8) {
      const id = `rotated-${type}-${handle}`;
      add(id, (up) => {
        let e;
        if (type === "text") {
          e = text(up, { text: "rotated\ntext here", originalText: "rotated\ntext here", x: 50, y: 60, angle });
        } else if (type === "freedraw" || type === "line") {
          const points = [
            [0, 0],
            [40, 70],
            [110, 50],
            [150, 120],
          ];
          e = el(up, { type, x: 50, y: 60, angle, points, ...up_size(points), roundness: null });
        } else {
          e = el(up, { type, x: 50, y: 60, width: 160, height: 90, angle });
        }
        const elements = [e];
        if (type === "rectangle" || type === "ellipse" || type === "diamond") {
          bindLabel(up, elements, e, "a rotated label");
        }
        const steps = [];
        for (const [move, mods] of [
          [[35, -20], {}],
          [[-60, 45], { shift: true }],
          [[25, 70], { alt: true }],
          [[-250, -180], { shift: true, alt: true }],
        ]) {
          steps.push(...resize([e.id], handle, move, mods));
        }
        return session(up, id, elements, steps);
      });
    }
  }
  // sticky notes: corners proportional by default, edges free, Shift/Alt
  for (const [name, handle, move, mods] of [
    ["corner", "se", [60, 40], {}],
    ["corner-shift", "se", [60, 40], { shift: true }],
    ["corner-nw-alt", "nw", [-30, -50], { alt: true }],
    ["edge-e", "e", [80, 0], {}],
    ["edge-e-shift", "e", [80, 0], { shift: true }],
    ["edge-n", "n", [0, -90], {}],
    ["edge-s-shrink", "s", [0, -150], {}],
    ["flip", "sw", [300, -300], {}],
  ]) {
    const id = `sticky-${name}`;
    add(id, (up) => {
      const note = el(up, { type: "stickynote", x: 0, y: 0, width: 200, height: 200 });
      const elements = [note];
      bindLabel(up, elements, note, "a sticky note with some words on it", { fontSize: 28, baseFontSize: 28 });
      return session(up, id, elements, [
        { begin: { selected: [note.id], handle } },
        { move, ...mods },
        { move: [move[0] / 2, move[1] / 3], ...mods },
      ]);
    });
  }
  add("sticky-empty", (up) => {
    const note = el(up, { type: "stickynote", x: 0, y: 0, width: 180, height: 180 });
    return session(up, "sticky-empty", [note], [
      ...resize([note.id], "se", [-150, -120]),
      ...resize([note.id], "w", [40, 0]),
    ]);
  });
  add("sticky-multi", (up) => {
    const note = el(up, { type: "stickynote", x: 0, y: 0, width: 200, height: 200 });
    const elements = [note];
    bindLabel(up, elements, note, "note", { fontSize: 28, baseFontSize: 28 });
    const r = el(up, { type: "rectangle", x: 250, y: 20, width: 100, height: 60 });
    const a = el(up, {
      type: "arrow",
      x: 205,
      y: 100,
      width: 40,
      height: 0,
      roundness: null,
      points: [
        [0, 0],
        [40, 0],
      ],
    });
    bindArrow(a, "start", note, [1.02, 0.5001]);
    bindArrow(a, "end", r, [-0.05, 0.5001]);
    elements.push(r, a);
    return session(up, "sticky-multi", elements, [
      ...resize([note.id, r.id], "se", [100, 50], { shift: true }),
      ...resize([note.id, r.id], "e", [-500, 0]),
    ]);
  });
  // zoom and pointer types
  for (const [zoom, pointerType] of [
    [2, "mouse"],
    [0.5, "pen"],
    [1, "touch"],
    [3.5, "touch"],
  ]) {
    const id = `zoom-${zoom}-${pointerType}`;
    add(id, (up) => {
      const r = el(up, { type: "rectangle", x: 0, y: 0, width: 300, height: 200, angle: 0.2 });
      const s = el(up, { type: "ellipse", x: 400, y: 0, width: 80, height: 60 });
      const steps = [];
      for (const handle of ["nw", "e", "rotation"]) {
        steps.push({ begin: { selected: [r.id], handle, zoom, pointerType } }, { move: [30, 25] });
      }
      steps.push({ begin: { selected: [r.id, s.id], handle: "sw", zoom, pointerType } }, { move: [-20, 35] });
      steps.push({ begin: { selected: [r.id], origin: [150, -4 / zoom], zoom, pointerType } }, { move: [0, -30] });
      steps.push({ begin: { selected: [r.id], origin: [150, 100], zoom, pointerType } }, { move: [10, 10] });
      return session(up, id, [r, s], steps);
    });
  }
  add("editor-phone-mobile", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 300, height: 200 });
    const l = el(up, {
      type: "line",
      x: 0,
      y: 300,
      width: 120,
      height: 40,
      roundness: null,
      points: [
        [0, 0],
        [60, 40],
        [120, 0],
      ],
    });
    return session(up, "editor-phone-mobile", [r, l], [
      { begin: { selected: [r.id], origin: [150, -4], editor: "phone-mobile", pointerType: "touch" } },
      { move: [0, -30] },
      { begin: { selected: [r.id], handle: "se", editor: "phone-mobile", pointerType: "touch" } },
      { move: [30, 30] },
      { begin: { selected: [l.id], handle: "se", editor: "tablet-mobile", pointerType: "touch" } },
      { move: [30, 30] },
      { begin: { selected: [l.id], handle: "se", editor: "desktop" } },
      { move: [30, 30] },
    ]);
  });
  // grid
  add("grid", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 200, height: 100 });
    return session(up, "grid", [r], [
      { begin: { selected: [r.id], handle: "se" } },
      { move: [33, 17], gridSize: 20 },
      { move: [33, 17], gridSize: 20, ctrl: true },
      { move: [-7, 51], gridSize: 20 },
    ]);
  });
  // groups keep the aspect ratio; a locked element has no handles
  add("group-multi", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 50, groupIds: ["g1"] });
    const e = el(up, { type: "ellipse", x: 150, y: 80, width: 60, height: 90, groupIds: ["g1"] });
    return session(up, "group-multi", [r, e], [
      ...resize([r.id, e.id], "e", [100, 0]),
      ...resize([r.id, e.id], "s", [0, -300]),
    ]);
  });
  add("locked", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 50, locked: true });
    return session(up, "locked", [r], [{ begin: { selected: [r.id], origin: [104, 54] } }, { move: [20, 20] }]);
  });
  // bound arrows follow a resized shape (updateBoundElements); a selected
  // arrow bound to an unselected shape is unbound
  add("bound-shape-resize", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 100 });
    const a = el(up, {
      type: "arrow",
      x: -150,
      y: 50,
      width: 140,
      height: 0,
      roundness: null,
      endArrowhead: "arrow",
      points: [
        [0, 0],
        [140, 0],
      ],
    });
    bindArrow(a, "end", r, [-0.05, 0.5001]);
    return session(up, "bound-shape-resize", [r, a], [
      ...resize([r.id], "e", [40, 0]),
      ...resize([r.id], "w", [50, 20]),
      ...resize([r.id], "rotation", [60, 10]),
    ]);
  });
  add("bound-arrow-resize", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 100 });
    const r2 = el(up, { type: "diamond", x: 300, y: 150, width: 100, height: 100 });
    const a = el(up, {
      type: "arrow",
      x: 105,
      y: 50,
      width: 190,
      height: 150,
      roundness: { type: 2 },
      endArrowhead: "arrow",
      points: [
        [0, 0],
        [100, 20],
        [190, 150],
      ],
    });
    bindArrow(a, "start", r, [1.05, 0.5001]);
    bindArrow(a, "end", r2, [-0.02, 0.5001]);
    return session(up, "bound-arrow-resize", [r, r2, a], [
      ...resize([a.id], "se", [40, 30]),
      ...resize([r.id, a.id], "ne", [60, -20], { shift: true }),
    ]);
  });
  add("elbow-multi-resize", (up) => {
    const r = el(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 80 });
    const r2 = el(up, { type: "rectangle", x: 250, y: 150, width: 90, height: 90 });
    const a = el(up, {
      type: "arrow",
      x: 105,
      y: 40,
      width: 145,
      height: 155,
      elbowed: true,
      roundness: null,
      endArrowhead: "arrow",
      points: [
        [0, 0],
        [145, 155],
      ],
    });
    bindArrow(a, "start", r, [1.05, 0.5001]);
    bindArrow(a, "end", r2, [-0.05, 0.5001]);
    const elements = [r, r2, a];
    const scene = new up.Scene(elements, { skipValidation: true });
    scene.mutateElement(a, { points: a.points });
    // fix the middle segment, as a segment drag leaves it
    if (a.points.length > 3) {
      scene.mutateElement(a, {
        fixedSegments: [{ index: 2, start: a.points[1], end: a.points[2] }],
      });
    }
    return session(up, "elbow-multi-resize", elements, [
      ...resize([r.id, r2.id, a.id], "se", [60, 40]),
      ...resize([r.id, r2.id, a.id], "nw", [500, 400]),
      ...resize([r.id, a.id], "e", [30, 0]),
    ]);
  });
  // text resized from its sides and corners, bound text in a container
  // with a minimum size, and an arrow label resized proportionally
  add("text-sides", (up) => {
    const t = text(up, { text: "one two three four", originalText: "one two three four", x: 0, y: 0, angle: 0.3 });
    return session(up, "text-sides", [t], [
      ...resize([t.id], "e", [-90, 0]),
      ...resize([t.id], "w", [60, 0], { alt: true }),
      ...resize([t.id], "n", [0, -40]),
      ...resize([t.id], "s", [0, -300]),
      ...resize([t.id], "nw", [10, 20], { alt: true }),
    ]);
  });
  add("label-min-size", (up) => {
    const r = el(up, { type: "ellipse", x: 0, y: 0, width: 240, height: 120 });
    const elements = [r];
    bindLabel(up, elements, r, "label text");
    return session(up, "label-min-size", elements, [
      ...resize([r.id], "se", [-235, -115]),
      ...resize([r.id], "n", [0, 200]),
      ...resize([r.id], "se", [-235, -115], { shift: true }),
      ...resize([r.id], "e", [-500, 0], { shift: true }),
    ]);
  });
  return cases;
};

/** The centre of a multi-selection handle (for scenes UI.resize cannot press). */
const uiHandlePointFrom = (up, elements, handle) => {
  const [x1, y1, x2, y2] = up.getCommonBounds(elements);
  const coords = up.getTransformHandlesFromCoords(
    [x1, y1, x2, y2, (x1 + x2) / 2, (y1 + y2) / 2],
    0,
    { value: 1 },
    "mouse",
    up.OMIT_SIDES_FOR_MULTIPLE_ELEMENTS,
  )[handle];
  return handleCentre(coords);
};

// -- random gestures ------------------------------------------------------------------

const TYPES = ["rectangle", "diamond", "ellipse", "image", "text", "line", "arrow", "freedraw", "stickynote", "frame", "embeddable"];

const randomElement = (up, r, i) => {
  const type = r.pick(TYPES);
  const x = q(r.range(-300, 300));
  const y = q(r.range(-300, 300));
  const angle = r.chance(0.4) ? q(r.range(0, 2 * Math.PI)) : 0;
  const width = q(r.range(20, 260));
  const height = q(r.range(20, 200));
  const groupIds = r.chance(0.15) ? ["g"] : [];
  if (type === "text") {
    const words = ["alpha", "beta", "gamma", "delta", "epsilon"];
    const value = Array.from({ length: r.int(1, 5) }, () => r.pick(words)).join(r.chance(0.3) ? "\n" : " ");
    return { element: text(up, { text: value, originalText: value, x, y, angle, groupIds, fontSize: r.pick([16, 20, 28, 36]) }) };
  }
  if (type === "line" || type === "arrow" || type === "freedraw") {
    const n = type === "freedraw" ? r.int(3, 12) : r.int(2, 5);
    const points = [[0, 0]];
    for (let k = 1; k < n; k++) points.push([q(r.range(-150, 150)), q(r.range(-150, 150))]);
    const element = el(up, {
      type,
      x,
      y,
      angle: type === "arrow" && r.chance(0.5) ? 0 : angle,
      points,
      ...up_size(points),
      roundness: type === "freedraw" || r.chance(0.5) ? null : { type: 2 },
      groupIds,
      ...(type === "arrow" ? { endArrowhead: r.pick([null, "arrow", "triangle"]) } : {}),
    });
    return { element, label: type === "arrow" && r.chance(0.3) ? "arrow label" : null };
  }
  const element = el(up, {
    type,
    x,
    y,
    width,
    height,
    angle: type === "frame" ? 0 : angle,
    groupIds,
    ...(type === "stickynote" ? { baseHeight: height } : {}),
  });
  const label =
    ["rectangle", "diamond", "ellipse", "stickynote"].includes(type) && r.chance(0.5)
      ? r.pick(["hi", "a label", "some longer label text", "two\nlines"])
      : null;
  return { element, label };
};

const randomCases = (count) => {
  const cases = [];
  for (let n = 0; n < count; n++) {
    const id = `random-${String(n).padStart(3, "0")}`;
    cases.push({
      id,
      build: (up) => {
        const r = rng(1000 + n * 7919);
        const elements = [];
        const count = r.int(1, 4);
        const made = [];
        for (let i = 0; i < count; i++) {
          const { element, label } = randomElement(up, r, i);
          elements.push(element);
          made.push(element);
          if (label) {
            bindLabel(up, elements, element, label, element.type === "stickynote" ? { fontSize: 28, baseFontSize: 28 } : {});
          }
        }
        // bind some arrows to other shapes
        for (const a of made.filter((e) => e.type === "arrow")) {
          const targets = made.filter((e) => ["rectangle", "diamond", "ellipse", "stickynote", "image"].includes(e.type));
          if (targets.length && r.chance(0.6)) {
            bindArrow(a, "start", r.pick(targets), [q(r.range(-0.1, 1.1)), q(r.range(-0.1, 1.1))]);
          }
          if (targets.length && r.chance(0.4)) {
            bindArrow(a, "end", r.pick(targets), [q(r.range(-0.1, 1.1)), q(r.range(-0.1, 1.1))]);
          }
        }
        const selectable = made.filter((e) => !e.containerId);
        const selected =
          selectable.length > 1 && r.chance(0.6)
            ? selectable.filter(() => r.chance(0.7)).map((e) => e.id)
            : [r.pick(selectable).id];
        if (selected.length === 0) selected.push(selectable[0].id);
        const steps = [];
        const gestures = r.int(1, 3);
        for (let g = 0; g < gestures; g++) {
          const handle = r.pick([...HANDLES8, "rotation"]);
          const zoom = r.pick([1, 1, 0.5, 2]);
          steps.push({ begin: { selected, handle, zoom, pointerType: r.pick(["mouse", "mouse", "pen", "touch"]), lenient: true } });
          const moves = r.int(1, 3);
          for (let m = 0; m < moves; m++) {
            steps.push({
              move: [q(r.range(-250, 250)), q(r.range(-250, 250))],
              shift: r.chance(0.3),
              alt: r.chance(0.25),
            });
          }
        }
        return { elements, steps };
      },
    });
  }
  return cases;
};

// -- static cases ----------------------------------------------------------------------

const catalog = (up) => {
  const out = [];
  const push = (e) => out.push(e);
  for (const angle of [0, 0.7, 3.9]) {
    for (const [w, h] of [
      [200, 120],
      [30, 25],
      [45, 200],
    ]) {
      push(el(up, { type: "rectangle", x: 10, y: -20, width: w, height: h, angle }));
    }
    push(el(up, { type: "image", x: 5, y: 5, width: 120, height: 90, angle }));
    push(el(up, { type: "ellipse", x: -50, y: 30, width: 150, height: 70, angle }));
    const points = [
      [0, 0],
      [80, 30],
      [140, -40],
    ];
    push(el(up, { type: "line", x: 0, y: 0, points, ...up_size(points), angle, roundness: null }));
    push(el(up, { type: "freedraw", x: 20, y: 40, points, ...up_size(points), angle }));
    push(text(up, { text: "some text", originalText: "some text", x: 3, y: 4, angle }));
  }
  push(el(up, { type: "frame", x: 0, y: 0, width: 300, height: 200 }));
  push(el(up, { type: "magicframe", x: 0, y: 0, width: 300, height: 200 }));
  push(el(up, { type: "stickynote", x: 0, y: 0, width: 200, height: 200 }));
  push(el(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 100, locked: true }));
  const elbow = el(up, {
    type: "arrow",
    x: 0,
    y: 0,
    elbowed: true,
    roundness: null,
    points: [
      [0, 0],
      [50, 0],
      [50, 80],
    ],
    width: 50,
    height: 80,
  });
  push(elbow);
  for (const p1 of [
    [0, 50],
    [50, 0],
    [60, -40],
    [60, 40],
    [-60, 40],
    [-60, -40],
    [0, 0],
  ]) {
    for (const type of ["line", "arrow"]) {
      push(el(up, { type, x: 100, y: 100, points: [[0, 0], p1], ...up_size([[0, 0], p1]), roundness: null }));
    }
  }
  push(el(up, { type: "freedraw", x: 0, y: 0, points: [[0, 0], [30, -20]], width: 30, height: 20 }));
  const three = [
    [0, 0],
    [60, 90],
    [120, 10],
  ];
  push(el(up, { type: "arrow", x: 10, y: 10, points: three, ...up_size(three), roundness: { type: 2 }, angle: 0.5 }));
  return out;
};

const OMITS = {
  default: undefined,
  none: {},
  desktop: "DEFAULT_OMIT_SIDES",
  frame: "OMIT_SIDES_FOR_FRAME",
  corners: { nw: true, se: true, rotation: true },
};
const omitArg = (up, name) => {
  const value = OMITS[name];
  if (typeof value === "string") return up[value];
  return value;
};

const handlesCases = (up) => {
  const cases = [];
  const scene = catalog(up);
  // a labelled arrow: getElementAbsoluteCoords(element, map, true) takes
  // the label in
  const labelled = el(up, {
    type: "arrow",
    x: 0,
    y: 0,
    roundness: null,
    points: [
      [0, 0],
      [200, 0],
    ],
    width: 200,
    height: 0,
  });
  const labelledScene = [labelled];
  bindLabel(up, labelledScene, labelled, "a label wider than it");
  const all = [...scene.map((e) => [[e], e.id]), [labelledScene, labelled.id]];
  all.forEach(([elements, target], i) => {
    const variants = [];
    for (const zoom of [0.5, 1, 3]) {
      for (const pointerType of ["mouse", "pen", "touch"]) {
        for (const omit of Object.keys(OMITS)) {
          const element = elements.find((e) => e.id === target);
          const args = [element, { value: zoom }, up.arrayToMap(elements), pointerType];
          const omitValue = omitArg(up, omit);
          const result =
            omit === "default" ? up.getTransformHandles(...args) : up.getTransformHandles(...args, omitValue);
          variants.push({ zoom, pointerType, omitSides: omit, result: handlesJson(result) });
        }
      }
    }
    cases.push({ id: `handles-${String(i).padStart(2, "0")}-${elements[0].type}`, kind: "handles", elements: clone(elements), target, variants });
  });

  const fromCoords = [];
  const r = rng(4242);
  for (let i = 0; i < 60; i++) {
    const x1 = q(r.range(-200, 200));
    const y1 = q(r.range(-200, 200));
    const x2 = x1 + q(r.pick([0, 5, 39, 40, 41, 80, 300]) * r.pick([1, 1, 1, -1]));
    const y2 = y1 + q(r.pick([0, 5, 39, 40, 41, 80, 300]));
    const coords = [x1, y1, x2, y2, (x1 + x2) / 2, (y1 + y2) / 2];
    const angle = r.chance(0.5) ? 0 : q(r.range(-7, 7));
    const zoom = r.pick([0.1, 0.5, 1, 1, 2, 5]);
    const pointerType = r.pick(["mouse", "pen", "touch"]);
    const omit = r.pick(Object.keys(OMITS));
    const margin = r.chance(0.5) ? null : r.pick([0, 4, 10]);
    const spacing = r.chance(0.6) ? null : r.pick([0, 2, 5]);
    const args = [coords, angle, { value: zoom }, pointerType];
    let result;
    if (margin === null && spacing === null) {
      result = omit === "default" ? up.getTransformHandlesFromCoords(...args) : up.getTransformHandlesFromCoords(...args, omitArg(up, omit));
    } else {
      result = up.getTransformHandlesFromCoords(
        ...args,
        omit === "default" ? undefined : omitArg(up, omit),
        margin ?? undefined,
        spacing ?? undefined,
      );
    }
    fromCoords.push({ coords, angle, zoom, pointerType, omitSides: omit, margin, spacing, result: handlesJson(result) });
  }
  cases.push({ id: "handles-from-coords", kind: "handlesFromCoords", variants: fromCoords });
  return cases;
};

const probePoints = (up, element, elementsMap, zoom) => {
  const points = [];
  const handles = up.getTransformHandles(element, { value: zoom }, elementsMap, "mouse", {});
  for (const h of Object.values(handles)) {
    if (!h) continue;
    const [cx, cy] = handleCentre(h);
    points.push([cx, cy], [h[0], h[1]], [h[0] + h[2] + 0.5 / zoom, cy]);
  }
  const [x1, y1, x2, y2] = [element.x, element.y, element.x + element.width, element.y + element.height];
  const cx = (x1 + x2) / 2;
  const cy = (y1 + y2) / 2;
  for (const d of [-6, -4, -2, 0, 2, 4, 6]) {
    points.push([cx, y1 + d / zoom], [x2 + d / zoom, cy], [cx + 7, y2 + d / zoom], [x1 + d / zoom, cy - 9]);
  }
  points.push([cx, cy], [x1 - 50, y1 - 50], [x2 + 200, y2]);
  const angle = element.angle;
  return points.map(([x, y]) => {
    if (!angle) return [x, y];
    const c = Math.cos(angle);
    const s = Math.sin(angle);
    return [(x - cx) * c - (y - cy) * s + cx, (x - cx) * s + (y - cy) * c + cy];
  });
};

const resizeTestCases = (up) => {
  const cases = [];
  const scene = catalog(up);
  scene.forEach((element, i) => {
    const elements = [element];
    const elementsMap = up.arrayToMap(elements);
    const variants = [];
    for (const zoom of [1, 2]) {
      for (const pointerType of ["mouse", "touch"]) {
        for (const editor of ["desktop", "phone-mobile", "phone-desktop-agent"]) {
          const appState = { selectedElementIds: { [element.id]: true } };
          const probes = probePoints(up, element, elementsMap, zoom).map((point) => {
            const result = up.resizeTest(element, elementsMap, appState, point[0], point[1], { value: zoom }, pointerType, EDITORS[editor]);
            const hit = up.getElementWithTransformHandleType(
              elements,
              appState,
              point[0],
              point[1],
              { value: zoom },
              pointerType,
              elementsMap,
              EDITORS[editor],
            );
            return {
              point,
              results: { [element.id]: result },
              hit: hit ? { element: hit.element.id, handle: hit.transformHandleType } : null,
            };
          });
          variants.push({ zoom, pointerType, editor, probes });
        }
      }
    }
    cases.push({ id: `resize-test-${String(i).padStart(2, "0")}-${element.type}`, kind: "resizeTest", elements: clone(elements), selected: [element.id], variants });
  });

  // several elements, some selected: the first selected element answering wins
  const a = el(up, { type: "rectangle", x: 0, y: 0, width: 100, height: 100 });
  const b = el(up, { type: "rectangle", x: 50, y: 50, width: 100, height: 100 });
  const c = el(up, { type: "ellipse", x: 104, y: -40, width: 60, height: 60 });
  const elements = [a, b, c];
  const elementsMap = up.arrayToMap(elements);
  const variants = [];
  for (const selected of [[a.id], [a.id, b.id], [b.id, c.id], []]) {
    const appState = { selectedElementIds: Object.fromEntries(selected.map((s) => [s, true])) };
    const probes = [
      ...probePoints(up, a, elementsMap, 1),
      ...probePoints(up, b, elementsMap, 1),
    ].map((point) => {
      const results = {};
      for (const e of elements) {
        results[e.id] = up.resizeTest(e, elementsMap, appState, point[0], point[1], { value: 1 }, "mouse", EDITORS.desktop);
      }
      const hit = up.getElementWithTransformHandleType(elements, appState, point[0], point[1], { value: 1 }, "mouse", elementsMap, EDITORS.desktop);
      return { point, results, hit: hit ? { element: hit.element.id, handle: hit.transformHandleType } : null };
    });
    variants.push({ zoom: 1, pointerType: "mouse", editor: "desktop", selected, probes });
  }
  cases.push({ id: "resize-test-scene", kind: "resizeTest", elements: clone(elements), selected: null, variants });

  // getTransformHandleTypeFromCoords
  const fromCoords = [];
  for (const bounds of [
    [0, 0, 200, 100],
    [-50, 20, -10, 30],
    [0, 0, 39, 39],
    [10, 10, 10, 10],
  ]) {
    for (const zoom of [0.5, 1, 2]) {
      for (const pointerType of ["mouse", "touch"]) {
        for (const editor of ["desktop", "phone-mobile"]) {
          const [x1, y1, x2, y2] = bounds;
          const cx = (x1 + x2) / 2;
          const cy = (y1 + y2) / 2;
          const handles = up.getTransformHandlesFromCoords([x1, y1, x2, y2, cx, cy], 0, { value: zoom }, pointerType, {});
          const points = [];
          for (const h of Object.values(handles)) if (h) points.push(handleCentre(h), [h[0] - 0.01, h[1]]);
          for (const d of [-7, -4, -1, 0, 3, 5]) {
            points.push([cx, y1 + d / zoom], [x2 + d / zoom, cy + 1], [cx - 2, y2 + d / zoom], [x1 + d / zoom, cy]);
          }
          points.push([cx, cy], [x2 + 100, y2 + 100]);
          const probes = points.map((point) => ({
            point,
            result: up.getTransformHandleTypeFromCoords(bounds, point[0], point[1], { value: zoom }, pointerType, EDITORS[editor]),
          }));
          fromCoords.push({ bounds, zoom, pointerType, editor, probes });
        }
      }
    }
  }
  cases.push({ id: "handle-type-from-coords", kind: "handleTypeFromCoords", variants: fromCoords });
  return cases;
};

const cursorCases = (up) => {
  const items = [];
  for (const handle of [...HANDLES8, "rotation", false]) {
    items.push({ element: null, handle, result: up.getCursorForResizingElement({ transformHandleType: handle }) });
    for (const angle of [0, 0.3, Math.PI / 8, Math.PI / 4, 1, 2, 3.5, 5.9, 6.2]) {
      for (const [w, h] of [
        [100, 50],
        [-100, 50],
        [100, -50],
        [-100, -50],
        [0, 50],
      ]) {
        const element = el(up, { type: "rectangle", width: 100, height: 50, angle });
        element.width = w;
        element.height = h;
        items.push({
          element: clone(element),
          handle,
          result: up.getCursorForResizingElement({ element, transformHandleType: handle }),
        });
      }
    }
  }
  return [{ id: "cursors", kind: "cursor", items }];
};

const boundingBoxCases = (up) => {
  const items = [];
  const rect = el(up, { type: "rectangle" });
  const elbow = el(up, { type: "arrow", elbowed: true, roundness: null, points: [[0, 0], [100, 100]] });
  const two = el(up, { type: "line", points: [[0, 0], [100, 100]] });
  const three = el(up, { type: "arrow", points: [[0, 0], [50, 20], [100, 100]] });
  const scenes = [[rect], [elbow], [two], [three], [three, two], [elbow, rect]];
  for (const elements of scenes) {
    for (const linear of [null, { isEditing: true, isDragging: false }, { isEditing: false, isDragging: true }, { isEditing: false, isDragging: false }]) {
      for (const editor of ["desktop", "phone-mobile"]) {
        items.push({
          elements: clone(elements),
          selectedLinearElement: linear,
          editor,
          result: up.hasBoundingBox(elements, { selectedLinearElement: linear }, EDITORS[editor]),
        });
      }
    }
  }
  return [{ id: "has-bounding-box", kind: "hasBoundingBox", items }];
};

const offsetCases = (up) => {
  const items = [];
  const r = rng(99);
  const scene = catalog(up).filter((e) => !(e.type === "arrow" && e.elbowed));
  for (let i = 0; i < 80; i++) {
    const selected = r.chance(0.6) ? [r.pick(scene)] : [r.pick(scene), r.pick(scene), r.pick(scene)].filter((e, k, a) => a.indexOf(e) === k);
    const handle = r.pick([...HANDLES8, "rotation", false]);
    const point = [q(r.range(-300, 300)), q(r.range(-300, 300))];
    items.push({
      elements: clone(selected),
      selected: selected.map((e) => e.id),
      handle,
      point,
      result: up.getResizeOffsetXY(handle, selected, up.arrayToMap(selected), point[0], point[1]),
    });
  }
  const directions = [];
  for (const p1 of [
    [0, 50],
    [50, 0],
    [60, -40],
    [60, 40],
    [-60, 40],
    [-60, -40],
    [0, -30],
    [-30, 0],
    [0, 0],
  ]) {
    for (const handle of [...HANDLES8, "rotation"]) {
      const element = el(up, { type: "line", points: [[0, 0], p1] });
      directions.push({ element: clone(element), handle, direction: up.getResizeArrowDirection(handle, element) });
    }
  }
  return [{ id: "resize-offset", kind: "resizeOffset", items, directions }];
};

// -- the fixture -------------------------------------------------------------------------

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating transform fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const buildFixture = (up, commit) => {
  const cases = [];
  up.reseed(1);
  cases.push(...handlesCases(up));
  up.reseed(1);
  cases.push(...resizeTestCases(up));
  up.reseed(1);
  cases.push(...cursorCases(up));
  up.reseed(1);
  cases.push(...boundingBoxCases(up));
  up.reseed(1);
  cases.push(...offsetCases(up));
  for (const { id, build } of [...upstreamCases(), ...extraCases()]) {
    up.reseed(1);
    clearCharWidths(up);
    cases.push(build(up));
  }
  for (const { id, build } of randomCases(160)) {
    up.reseed(1);
    clearCharWidths(up);
    const { elements, steps } = build(up);
    cases.push(session(up, id, elements, steps));
  }
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case ${c.id}`);
    ids.add(c.id);
  }
  return format({
    description:
      "getTransformHandles, getTransformHandlesFromCoords, hasBoundingBox (packages/element/src/transformHandles.ts), resizeTest, getElementWithTransformHandleType, getTransformHandleTypeFromCoords, getCursorForResizingElement (resizeTest.ts), getResizeOffsetXY, getResizeArrowDirection, transformElements and resizeSingleElement (resizeElements.ts) driven as App.tsx's pointer handlers drive them, in upstream's test mode (ids id0.., timestamps 1, reseed(1) before each case, text measured as text.length * 10). Generated by tools/goldens/transform-fixtures.mjs.",
    upstream: commit,
    cases,
  });
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`transform-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  globalThis.__transformProbe = probe;
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    define: { "import.meta.env.MODE": '"test"' },
    patch: PATCH,
  });
  up.setCustomTextMetricsProvider({ getLineWidth: (value) => value.length * 10 });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("transform fixture is out of date: run node tools/goldens/transform-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`transform fixture up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
