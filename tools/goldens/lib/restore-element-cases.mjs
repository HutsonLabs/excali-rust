// Inputs for restore-element.json (ex-104): the per-type rules of upstream's
// restoreElement (packages/excalidraw/data/restore.ts:517-752). The
// isTransparent table behind the sticky note colours (colors.ts:389-391,
// tinycolor2 1.6.0) lives in app-state.json (tools/goldens/app-state.mjs),
// shared with restoreAppState's sticky note colours.
//
// Two kinds of input:
//
// - `upstream-*` cases are the per-element inputs of upstream's own
//   packages/excalidraw/tests/data/restore.test.ts, built the way that test
//   builds them: `apiCreateElement` below is API.createElement
//   (packages/excalidraw/tests/helpers/api.ts:166-436) over upstream's own
//   constructors and default app state. Inputs the test gives as JS values
//   JSON cannot hold (NaN, Infinity) are replaced by the value a file would
//   hold after JSON.stringify (null), and noted where it happens.
// - the other cases are tables over each rule: every falsy value, every
//   JSON type, boundaries, and the inputs upstream throws on.
//
// Each case is { id, call: "restoreElement", element, targets?, existing?,
// opts? }: restoreElement(element, arrayToMap(targets ?? [element]),
// existing ? arrayToMap(existing) : null, opts). Builders are functions of the upstream module so
// constructor output (ids, versionNonce draws) comes from upstream; each is
// run after reseed(1).

// -- API.createElement ----------------------------------------------------------

/** API.createElement (tests/helpers/api.ts:166-436) with h.state undefined. */
export const apiCreateElement = (up, opts = {}) => {
  const {
    type = "rectangle",
    id,
    x = 0,
    y = x,
    width = 100,
    height = width,
    isDeleted = false,
    groupIds = [],
    ...rest
  } = opts;
  const appState = up.getDefaultAppState();
  const base = {
    seed: 1,
    x,
    y,
    width,
    height,
    frameId: rest.frameId ?? null,
    index: rest.index ?? null,
    angle: rest.angle ?? 0,
    strokeColor:
      rest.strokeColor ??
      (type === "stickynote"
        ? appState.currentItemStickynoteStrokeColor
        : appState.currentItemStrokeColor),
    backgroundColor:
      rest.backgroundColor ??
      (type === "stickynote"
        ? appState.currentItemStickynoteBackgroundColor
        : appState.currentItemBackgroundColor),
    fillStyle: rest.fillStyle ?? appState.currentItemFillStyle,
    strokeWidth:
      rest.strokeWidth ?? up.getStrokeWidthByKey(type, appState.currentItemStrokeWidthKey),
    strokeStyle: rest.strokeStyle ?? appState.currentItemStrokeStyle,
    roundness: (
      rest.roundness === undefined ? appState.currentItemRoundness === "round" : rest.roundness
    )
      ? {
          type: up.isUsingAdaptiveRadius(type)
            ? up.ROUNDNESS.ADAPTIVE_RADIUS
            : up.ROUNDNESS.PROPORTIONAL_RADIUS,
        }
      : null,
    roughness: rest.roughness ?? appState.currentItemRoughness,
    opacity: rest.opacity ?? appState.currentItemOpacity,
    boundElements: rest.boundElements ?? null,
    locked: rest.locked ?? false,
    created: rest.created === undefined ? up.getUpdatedTimestamp() : rest.created,
  };
  let element;
  switch (type) {
    case "rectangle":
    case "diamond":
    case "ellipse":
      element = up.newElement({ type, ...base });
      break;
    case "embeddable":
      element = up.newEmbeddableElement({ type: "embeddable", ...base });
      break;
    case "iframe":
      element = up.newIframeElement({ type: "iframe", ...base });
      break;
    case "stickynote":
      element = up.newStickyNoteElement({
        ...base,
        width,
        height,
        type,
        baseHeight: rest.baseHeight ?? height,
      });
      break;
    case "text": {
      const fontSize = rest.fontSize ?? appState.currentItemFontSize;
      const fontFamily = rest.fontFamily ?? appState.currentItemFontFamily;
      element = up.newTextElement({
        ...base,
        text: rest.text || "test",
        fontSize,
        fontFamily,
        textAlign: rest.textAlign ?? appState.currentItemTextAlign,
        verticalAlign: rest.verticalAlign ?? up.DEFAULT_VERTICAL_ALIGN,
        containerId: rest.containerId ?? undefined,
      });
      element.width = width;
      element.height = height;
      break;
    }
    case "freedraw":
      element = up.newFreeDrawElement({
        type,
        simulatePressure: true,
        points: rest.points,
        strokeOptions: rest.strokeOptions,
        ...base,
      });
      break;
    case "arrow":
      element = up.newArrowElement({
        ...base,
        width,
        height,
        type,
        points: rest.points ?? [
          [0, 0],
          [100, 100],
        ],
        elbowed: rest.elbowed ?? false,
      });
      break;
    case "line":
      element = up.newLinearElement({
        ...base,
        width,
        height,
        type,
        points: rest.points ?? [
          [0, 0],
          [100, 100],
        ],
        polygon: rest.polygon,
      });
      break;
    case "image":
      element = up.newImageElement({
        ...base,
        width,
        height,
        type,
        fileId: rest.fileId ?? null,
        status: rest.status || "saved",
        scale: rest.scale || [1, 1],
      });
      break;
    case "frame":
      element = up.newFrameElement({ ...base, width, height });
      break;
    case "magicframe":
      element = up.newMagicFrameElement({ ...base, width, height });
      break;
    default:
      throw new Error(`apiCreateElement: ${type}`);
  }
  if (element.type === "arrow") {
    element.startBinding = rest.startBinding ?? null;
    element.endBinding = rest.endBinding ?? null;
    element.startArrowhead = rest.startArrowhead ?? null;
    element.endArrowhead = rest.endArrowhead ?? null;
  }
  if (id) element.id = id;
  if (isDeleted) element.isDeleted = isDeleted;
  if (groupIds) element.groupIds = groupIds;
  return element;
};

// -- helpers --------------------------------------------------------------------

export const without = (object, ...keys) => {
  const copy = { ...object };
  for (const key of keys) delete copy[key];
  return copy;
};

/** A saved element of `type` with every current field, as a file holds it. */
export const saved = (type, rest = {}) => ({
  id: `el-${type}`,
  type,
  x: 10,
  y: 20,
  width: 100,
  height: 50,
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
  index: "a0",
  roundness: null,
  seed: 1,
  version: 3,
  versionNonce: 7,
  isDeleted: false,
  boundElements: [],
  updated: 1700000000000,
  created: 1690000000000,
  link: null,
  locked: false,
  ...rest,
});

export const savedText = (rest = {}) =>
  saved("text", {
    text: "hello",
    fontSize: 20,
    fontFamily: 5,
    textAlign: "left",
    verticalAlign: "top",
    containerId: null,
    originalText: "hello",
    autoResize: true,
    lineHeight: 1.25,
    width: 50,
    height: 25,
    ...rest,
  });

export const savedLinear = (type, rest = {}) =>
  saved(type, {
    points: [
      [0, 0],
      [100, 50],
    ],
    startBinding: null,
    endBinding: null,
    startArrowhead: null,
    endArrowhead: null,
    ...rest,
  });

export const savedArrow = (rest = {}) =>
  savedLinear("arrow", { endArrowhead: "arrow", elbowed: false, ...rest });

export const savedElbow = (rest = {}) =>
  savedArrow({
    elbowed: true,
    points: [
      [0, 0],
      [50, 0],
      [50, 50],
      [100, 50],
    ],
    fixedSegments: null,
    startIsSpecial: null,
    endIsSpecial: null,
    ...rest,
  });

export const savedFreedraw = (rest = {}) =>
  saved("freedraw", {
    points: [
      [0, 0],
      [5, 5],
      [10, 0],
    ],
    pressures: [0.2, 0.4, 0.6],
    simulatePressure: false,
    strokeOptions: { variability: "constant", streamline: 0.3 },
    ...rest,
  });

export const savedSticky = (rest = {}) =>
  saved("stickynote", {
    width: 250,
    height: 250,
    baseHeight: 250,
    strokeColor: "#1e1e1e",
    backgroundColor: "#ffdf6b",
    ...rest,
  });

const re = (id, element, more = {}) => ({ id, call: "restoreElement", element, ...more });

// -- upstream restore.test.ts ------------------------------------------------------

const upstreamCases = () => [
  // "basic restoreElements" (restore.test.ts:52-59)
  re("upstream-basic-text", (up) => apiCreateElement(up, { type: "text" })),
  re("upstream-basic-rectangle", (up) => apiCreateElement(up, { type: "rectangle" })),
  // "restores created=%s ..." (restore.test.ts:61-92): undefined is absent
  ...[123, 0, null, undefined].map((created) =>
    re(`upstream-created-${created}`, (up) => {
      const element = {
        ...apiCreateElement(up, { type: "rectangle", index: "a0" }),
        created,
        updated: 456,
      };
      return JSON.parse(JSON.stringify(element));
    }),
  ),
  // "should restore transparent sticky note stroke as black" (105-114)
  re("upstream-sticky-transparent-stroke", (up) => ({
    ...apiCreateElement(up, { type: "stickynote" }),
    strokeColor: "transparent",
  })),
  // "should return empty array when input type is not supported" (132-141)
  re("upstream-not-supported-type", (up) => ({
    ...apiCreateElement(up, { type: "text" }),
    type: "not supported",
  })),
  re("upstream-selection", { type: "selection" }),
  // "should restore text element correctly passing value for each attribute" (154-174)
  re("upstream-text-each-attribute", (up) =>
    apiCreateElement(up, {
      type: "text",
      fontSize: 14,
      fontFamily: 1,
      text: "text",
      textAlign: "center",
      verticalAlign: "middle",
      id: "id-text01",
    }),
  ),
  // "should not delete empty text element when opts.deleteInvisibleElements is not defined" (176-191)
  re("upstream-text-empty-not-deleted", (up) =>
    apiCreateElement(up, { type: "text", text: "", isDeleted: false }),
  ),
  // "... unknown font family, null text and undefined alignment" (193-213)
  re(
    "upstream-text-unknown-font-null-text",
    (up) => {
      const element = apiCreateElement(up, {
        type: "text",
        textAlign: undefined,
        verticalAlign: undefined,
        id: "id-text01",
      });
      element.text = null;
      element.font = "10 unknown";
      return element;
    },
    { opts: { deleteInvisibleElements: true } },
  ),
  // "should sanitize non-finite font sizes on text elements" (215-230):
  // fontSize NaN is null in a file.
  re("upstream-text-non-finite-font-size", (up) => {
    const element = apiCreateElement(up, { type: "text", text: "text" });
    element.fontSize = null;
    element.baseFontSize = "abc";
    return element;
  }),
  // "should clamp restored font ceilings to the sticky note maximum" (232-245)
  re("upstream-text-base-font-size-clamped", (up) => {
    const element = apiCreateElement(up, { type: "text", text: "text" });
    element.baseFontSize = 1e20;
    return element;
  }),
  // "should restore freedraw element correctly" (381-397)
  re("upstream-freedraw", (up) =>
    apiCreateElement(up, {
      type: "freedraw",
      id: "id-freedraw01",
      points: [
        [0, 0],
        [10, 10],
      ],
    }),
  ),
  // "should restore only valid freedraw points and keep pressures aligned"
  // (399-430): Infinity and NaN are null in a file.
  re("upstream-freedraw-invalid-points", (up) => ({
    ...apiCreateElement(up, {
      type: "freedraw",
      id: "id-freedraw-invalid-points",
      points: [
        [0, 0],
        [10, 10],
      ],
    }),
    simulatePressure: false,
    points: [[0, 0], [null, 10], null, [20, 20], [null, 30], [40, null]],
    pressures: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6],
  })),
  // "should restore freedraw stroke variability" (432-477)
  ...[
    ["missing", undefined],
    ["bogusString", { variability: "scribble" }],
    ["bogusNumber", { variability: 42 }],
    ["valid", { variability: "constant", streamline: 0.8 }],
    ["variable", { variability: "variable", streamline: 0.8 }],
  ].map(([id, strokeOptions]) =>
    re(`upstream-freedraw-stroke-options-${id}`, (up) => {
      const element = {
        ...apiCreateElement(up, {
          type: "freedraw",
          id: "id-freedraw-mode",
          points: [
            [0, 0],
            [10, 10],
          ],
        }),
        id,
        strokeOptions,
      };
      return JSON.parse(JSON.stringify(element));
    }),
  ),
  // "should restore line and draw elements correctly" (479-504)
  re("upstream-line", (up) => apiCreateElement(up, { type: "line", id: "id-line01" })),
  re("upstream-draw", (up) => ({
    ...apiCreateElement(up, { type: "line", id: "id-draw01" }),
    type: "draw",
  })),
  // "should restore arrow element correctly" (506-517)
  re("upstream-arrow", (up) => apiCreateElement(up, { type: "arrow", id: "id-arrow01" })),
  // "should normalize legacy crowfoot arrowheads on restore" (519-537)
  re("upstream-arrow-crowfoot", (up) => ({
    ...apiCreateElement(up, { type: "arrow" }),
    startArrowhead: "crowfoot_one",
    endArrowhead: "crowfoot_one_or_many",
  })),
  // "should remove imperceptibly small elements" (569-590): the per-element
  // part; marking it deleted is restoreElements' (ex-105)
  re(
    "upstream-arrow-imperceptibly-small",
    (up) =>
      apiCreateElement(up, {
        type: "arrow",
        points: [
          [0, 0],
          [0.02, 0.05],
        ],
        x: 0,
        y: 0,
      }),
    { opts: { deleteInvisibleElements: true } },
  ),
  // "should keep 'imperceptibly' small freedraw/line elements" (592-621)
  re("upstream-freedraw-tiny", (up) =>
    apiCreateElement(up, {
      type: "freedraw",
      points: [
        [0, 0],
        [0.0001, 0.0001],
      ],
      x: 0,
      y: 0,
    }),
  ),
  re("upstream-line-tiny", (up) =>
    apiCreateElement(up, {
      type: "line",
      points: [
        [0, 0],
        [0.0001, 0.0001],
      ],
      x: 0,
      y: 0,
    }),
  ),
  // "should restore loop linears correctly" (623-673)
  re("upstream-loop-line", (up) =>
    apiCreateElement(up, {
      type: "line",
      points: [
        [0, 0],
        [100, 100],
        [100, 200],
        [0, 0],
      ],
      x: 0,
      y: 0,
    }),
  ),
  re("upstream-loop-arrow", (up) =>
    apiCreateElement(up, {
      type: "arrow",
      points: [
        [0, 0],
        [100, 100],
        [100, 200],
        [0, 0],
      ],
      x: 500,
      y: 500,
    }),
  ),
  // endArrowhead null (675-682) and undefined (684-695)
  re("upstream-arrow-end-arrowhead-null", (up) => apiCreateElement(up, { type: "arrow" })),
  re("upstream-arrow-end-arrowhead-undefined", (up) =>
    without(apiCreateElement(up, { type: "arrow" }), "endArrowhead"),
  ),
  // "when element.points of a line element is not an array" (697-717)
  re("upstream-line-points-not-array", (up) => ({
    ...apiCreateElement(up, { type: "line", width: 100, height: 200 }),
    points: "not an array",
  })),
  // "should restore only valid linear points" (719-763): Infinity and NaN
  // are null in a file.
  re("upstream-line-valid-points-only", (up) => ({
    ...apiCreateElement(up, { type: "line", x: 10, y: 20, width: 100, height: 200 }),
    points: [[2, 3], null, [null, 4], [5, 7], [null, 8], [9, null]],
  })),
  re("upstream-arrow-valid-points-only", (up) => ({
    ...apiCreateElement(up, { type: "arrow", width: 100, height: 200 }),
    points: [
      [null, 0],
      [null, 4],
    ],
  })),
  // "should mark extremely large linear elements as deleted" (765-810)
  re("upstream-huge-line", (up) => ({
    ...apiCreateElement(up, { type: "line", x: 419048829414166, y: 8484 }),
    points: [
      [0, 0],
      [-302985021938436, 0],
      [-838097658820234, 30],
    ],
  })),
  re("upstream-huge-arrow", (up) => ({
    ...apiCreateElement(up, { type: "arrow" }),
    points: [
      [0, 0],
      [900000, 0],
    ],
  })),
  re("upstream-huge-normal-line", (up) => ({
    ...apiCreateElement(up, { type: "line" }),
    points: [
      [0, 0],
      [100, 200],
    ],
  })),
  // "when the number of points of a line is greater or equal 2" (812-863)
  re("upstream-line-points-at-origin", (up) => ({
    ...apiCreateElement(up, { type: "line", width: 100, height: 200, x: 10, y: 20 }),
    points: [
      [0, 0],
      [1, 1],
    ],
  })),
  re("upstream-line-points-rebased", (up) => ({
    ...apiCreateElement(up, { type: "line", width: 200, height: 400, x: 30, y: 40 }),
    points: [
      [3, 4],
      [5, 6],
    ],
  })),
  // "should restore correctly with rectangle, ellipse and diamond elements" (865-907)
  ...["rectangle", "ellipse", "diamond"].map((type, i) =>
    re(`upstream-generic-${type}`, (up) =>
      apiCreateElement(up, {
        type,
        id: String(i + 1),
        fillStyle: "cross-hatch",
        strokeWidth: 2,
        strokeStyle: "dashed",
        roughness: 2,
        opacity: 10,
        x: 10,
        y: 20,
        strokeColor: "red",
        backgroundColor: "blue",
        width: 100,
        height: 200,
        groupIds: ["1", "2", "3"],
        roundness: { type: 2 },
      }),
    ),
  ),
  // "should strip arrow binding if repair throws" (1259-1309)
  re(
    "upstream-arrow-binding-invalid-reference",
    (up) => {
      const container = apiCreateElement(up, { type: "rectangle", boundElements: [] });
      const arrow = apiCreateElement(up, {
        type: "arrow",
        id: "id-arrow01",
        endBinding: { elementId: container.id, fixedPoint: [0.5, 0.5], mode: "inside" },
      });
      Object.assign(arrow, { endBinding: { elementId: 42 } });
      return arrow;
    },
    {
      targets: (up, arrow) => {
        const container = apiCreateElement(up, {
          type: "rectangle",
          boundElements: [{ type: "arrow", id: arrow.id }],
        });
        return [arrow, container];
      },
    },
  ),
];

// -- text ----------------------------------------------------------------------------

const textCases = () => [
  re("text-saved", savedText()),
  re("text-raw-text-deleted", savedText({ rawText: "obsidian" })),
  // legacy `font` (restore.ts:538-544)
  ...[
    ["virgil", "20px Virgil"],
    ["helvetica", "16px Helvetica"],
    ["cascadia", "36px Cascadia"],
    ["excalifont", "28px Excalifont"],
    ["nunito", "20px Nunito"],
    ["lilita-two-words", "20px Lilita One"],
    ["assistant", "20px Assistant"],
    ["liberation-two-words", "20px Liberation Sans"],
    ["lowercase-name", "20px virgil"],
    ["unknown", "20px Comic"],
    ["no-family", "20px"],
    ["empty", ""],
    ["no-px", "abc Virgil"],
    ["fractional", "12.5px Virgil"],
    ["leading-space", " 20px Virgil"],
    ["exponent", "1e2px Cascadia"],
    ["infinity", "Infinity Virgil"],
    ["prototype-key", "20px constructor"],
    ["tostring-key", "20px toString"],
    ["fallback-name", "20px Xiaolai"],
    ["double-space", "20px  Virgil"],
    ["tab", "20px\tVirgil"],
  ].map(([name, font]) =>
    re(`text-font-${name}`, without(savedText({ font }), "fontSize", "fontFamily")),
  ),
  re("text-font-overrides-fields", savedText({ font: "36px Cascadia", fontSize: 12, fontFamily: 1 })),
  re("text-font-null-throws", savedText({ font: null })),
  re("text-font-number-throws", savedText({ font: 20 })),
  re("text-font-array-throws", savedText({ font: ["20px", "Virgil"] })),
  re("text-font-object-throws", savedText({ font: {} })),
  re("text-font-first", { font: "20px Virgil", ...savedText() }),
  // fontSize (restore.ts:545-547)
  ...[
    ["missing"],
    ["null", null],
    ["zero", 0],
    ["negative", -5],
    ["string", "20"],
    ["fraction", 13.5],
    ["huge", 1e308],
  ].map(([name, ...value]) => {
    const element = without(savedText(), "fontSize");
    if (value.length) element.fontSize = value[0];
    return re(`text-font-size-${name}`, element);
  }),
  // text (restore.ts:548)
  ...[
    ["missing"],
    ["null", null],
    ["empty", ""],
    ["number", 5],
    ["array", ["a"]],
    ["multiline", "a\nb\r\nc\rd"],
  ].map(([name, ...value]) => {
    const element = without(savedText(), "text", "lineHeight");
    if (value.length) element.text = value[0];
    return re(`text-text-${name}`, element);
  }),
  // lineHeight (restore.ts:555-562): kept, detected from height, or per family
  re("text-line-height-kept", savedText({ lineHeight: 1.5 })),
  re("text-line-height-zero-detected", savedText({ lineHeight: 0, height: 60, text: "a\nb" })),
  re("text-line-height-detected-multiline", without(savedText({ height: 72, fontSize: 16, text: "a\r\nb\rc" }), "lineHeight")),
  re("text-line-height-detected-tabs", without(savedText({ height: 30, text: "\ta\t" }), "lineHeight")),
  re("text-line-height-detected-string-height", without(savedText({ height: "50" }), "lineHeight")),
  re("text-line-height-detected-no-font-size", without(savedText({ height: 50 }), "lineHeight", "fontSize")),
  re("text-line-height-detected-string-font-size", without(savedText({ height: 50, fontSize: "20" }), "lineHeight")),
  re("text-line-height-detected-font-string", without(savedText({ height: 50, font: "25px Virgil" }), "lineHeight", "fontSize")),
  re("text-line-height-detected-null-text-throws", without(savedText({ height: 50, text: null }), "lineHeight")),
  re("text-line-height-detected-missing-text-throws", without(savedText({ height: 50 }), "lineHeight", "text")),
  re("text-line-height-detected-number-text-throws", without(savedText({ height: 50, text: 7 }), "lineHeight")),
  re("text-line-height-detected-object-height", without(savedText({ height: { h: 1 } }), "lineHeight")),
  re("text-line-height-detected-array-height", without(savedText({ height: [40] }), "lineHeight")),
  ...[
    ["virgil", 1],
    ["helvetica", 2],
    ["cascadia", 3],
    ["four", 4],
    ["excalifont", 5],
    ["nunito", 6],
    ["lilita", 7],
    ["comic", 8],
    ["liberation", 9],
    ["assistant", 10],
    ["xiaolai", 100],
    ["sans-serif", 998],
    ["segoe", 1000],
    ["string-key", "2"],
    ["array-key", [7]],
    ["missing"],
    ["null", null],
    ["float-key", 2.0000001],
    ["bool", true],
  ].map(([name, ...value]) => {
    const element = without(savedText({ height: 0 }), "lineHeight", "fontFamily");
    if (value.length) element.fontFamily = value[0];
    return re(`text-line-height-family-${name}`, element);
  }),
  // The fallback reads element.fontFamily, not the family from `font`.
  re(
    "text-line-height-family-ignores-font",
    without(savedText({ height: 0, font: "20px Helvetica", fontFamily: 7 }), "lineHeight"),
  ),
  // alignment, container, originalText, autoResize
  re("text-align-falsy", savedText({ textAlign: "", verticalAlign: 0 })),
  re("text-align-missing", without(savedText(), "textAlign", "verticalAlign")),
  re("text-align-kept-verbatim", savedText({ textAlign: "justify", verticalAlign: 3 })),
  re("text-container-id-missing", without(savedText(), "containerId")),
  re("text-container-id-empty-kept", savedText({ containerId: "" })),
  re("text-container-id-set", savedText({ containerId: "box" })),
  re("text-original-text-empty", savedText({ originalText: "" })),
  re("text-original-text-missing", without(savedText(), "originalText")),
  re("text-original-text-other", savedText({ originalText: "hel lo" })),
  re("text-original-text-null-text-empty", savedText({ originalText: null, text: null })),
  re("text-auto-resize-false", savedText({ autoResize: false })),
  re("text-auto-resize-null", savedText({ autoResize: null })),
  re("text-auto-resize-zero", savedText({ autoResize: 0 })),
  // labelPosition (restore.ts:573-575)
  ...[
    ["missing"],
    ["null", null],
    ["half", 0.5],
    ["negative", -2],
    ["above-one", 3],
    ["negative-zero", -0],
    ["string", "0.5"],
    ["one", 1],
  ].map(([name, ...value]) => {
    const element = savedText();
    if (value.length) element.labelPosition = value[0];
    return re(`text-label-position-${name}`, element);
  }),
  // baseFontSize (restore.ts:578-580, stickyNote.ts:379-384)
  ...[
    ["missing"],
    ["null", null],
    ["kept", 28],
    ["below-min", 0.5],
    ["zero", 0],
    ["negative", -3],
    ["max", 512],
    ["above-max", 513],
    ["string", "28"],
    ["fraction", 17.25],
  ].map(([name, ...value]) => {
    const element = savedText();
    if (value.length) element.baseFontSize = value[0];
    return re(`text-base-font-size-${name}`, element);
  }),
  // deleteInvisibleElements (restore.ts:585-589)
  re("text-empty-delete-invisible", savedText({ text: "" }), {
    opts: { deleteInvisibleElements: true },
  }),
  re("text-empty-already-deleted", savedText({ text: "", isDeleted: true }), {
    opts: { deleteInvisibleElements: true },
  }),
  re("text-non-empty-delete-invisible", savedText(), { opts: { deleteInvisibleElements: true } }),
  re("text-empty-delete-invisible-string-version", savedText({ text: "", version: "4" }), {
    opts: { deleteInvisibleElements: true },
  }),
  re("text-empty-delete-invisible-no-version", without(savedText({ text: "" }), "version"), {
    opts: { deleteInvisibleElements: true },
  }),
  re("text-empty-delete-invisible-truthy-is-deleted", savedText({ text: "", isDeleted: "no" }), {
    opts: { deleteInvisibleElements: true },
  }),
  re("text-minimal", { type: "text" }),
  re("text-minimal-delete-invisible", { type: "text" }, { opts: { deleteInvisibleElements: true } }),
  re("text-link-throws", savedText({ link: 3 })),
];

// -- freedraw ------------------------------------------------------------------------

const freedrawCases = () => [
  re("freedraw-saved", savedFreedraw()),
  re("freedraw-points-missing", without(savedFreedraw(), "points")),
  re("freedraw-points-object", savedFreedraw({ points: { 0: [1, 1] } })),
  re("freedraw-points-invalid-shapes", savedFreedraw({
    points: [[1, 2, 3], [1], "12", [true, 1], ["1", 2], [1, 2], [[1], 2], [3, 4]],
    pressures: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
  })),
  re("freedraw-pressures-shorter", savedFreedraw({ pressures: [0.9] })),
  re("freedraw-pressures-longer", savedFreedraw({ pressures: [0.1, 0.2, 0.3, 0.4, 0.5] })),
  re("freedraw-pressures-non-finite", savedFreedraw({ pressures: [null, "0.5", 2] })),
  re("freedraw-pressures-not-array", savedFreedraw({ pressures: "0.5" })),
  re("freedraw-pressures-missing", without(savedFreedraw(), "pressures")),
  re("freedraw-simulate-pressure-missing", without(savedFreedraw(), "simulatePressure")),
  re("freedraw-simulate-pressure-null", savedFreedraw({ simulatePressure: null })),
  re("freedraw-points-not-rebased", savedFreedraw({ points: [[5, 5], [10, 10]] })),
  ...[
    ["null", null],
    ["array", ["constant"]],
    ["string", "constant"],
    ["number", 1],
    ["streamline-string", { variability: "constant", streamline: "0.8" }],
    ["streamline-null", { variability: "variable", streamline: null }],
    ["streamline-zero", { streamline: 0 }],
    ["extra-keys-dropped", { variability: "constant", streamline: 0.2, thinning: 0.7 }],
    ["variability-uppercase", { variability: "Constant" }],
  ].map(([name, strokeOptions]) =>
    re(`freedraw-stroke-options-${name}`, savedFreedraw({ strokeOptions })),
  ),
  re("freedraw-minimal", { type: "freedraw" }),
];

// -- image ---------------------------------------------------------------------------

const imageCases = () => [
  re("image-saved", saved("image", { status: "saved", fileId: "f1", scale: [1, -1], crop: null })),
  re("image-minimal", { type: "image" }),
  re("image-status-empty", saved("image", { status: "", fileId: "f1" })),
  re("image-status-kept", saved("image", { status: "error", fileId: null })),
  re("image-scale-null", saved("image", { scale: null })),
  re("image-scale-zero", saved("image", { scale: 0 })),
  re("image-scale-kept", saved("image", { scale: "big" })),
  re("image-crop", saved("image", {
    crop: { x: 1, y: 2, width: 3, height: 4, naturalWidth: 10, naturalHeight: 20 },
  })),
  re("image-crop-false", saved("image", { crop: false })),
  re("image-file-id-first", { fileId: "f", ...saved("image") }),
];

// -- line / draw -----------------------------------------------------------------------

const ARROWHEADS = [
  null,
  "arrow",
  "bar",
  "dot",
  "circle",
  "circle_outline",
  "triangle",
  "triangle_outline",
  "diamond",
  "diamond_outline",
  "crowfoot_one",
  "crowfoot_many",
  "crowfoot_one_or_many",
  "cardinality_one",
  "cardinality_exactly_one",
  "cardinality_zero_or_one",
  "cardinality_zero_or_many",
  "unknown",
  "",
  0,
  true,
];

const lineCases = () => [
  re("line-saved", savedLinear("line", { polygon: false })),
  re("line-minimal", { type: "line" }),
  re("draw-minimal", { type: "draw" }),
  re("draw-saved", savedLinear("draw", { polygon: true, strokeSharpness: "round" })),
  ...ARROWHEADS.map((head, i) =>
    re(`line-arrowheads-${i}-${String(head)}`, savedLinear("line", {
      startArrowhead: head,
      endArrowhead: head,
    })),
  ),
  re("line-arrowheads-missing", without(savedLinear("line"), "startArrowhead", "endArrowhead")),
  re("line-bindings-cleared", savedLinear("line", {
    startBinding: { elementId: "a", mode: "orbit", fixedPoint: [0, 0] },
    endBinding: { elementId: "b", focus: 0, gap: 1 },
  })),
  re("line-rebase", savedLinear("line", { points: [[10, -5], [20, 5], [0, 0]] })),
  re("line-rebase-negative-zero-no-rebase", savedLinear("line", { points: [[-0, 0], [20, 5]] })),
  re("line-rebase-missing-x", without(savedLinear("line", { points: [[10, 10], [20, 5]] }), "x")),
  re("line-no-rebase-missing-x", without(savedLinear("line"), "x", "y")),
  re("line-rebase-null-x", savedLinear("line", { x: null, points: [[1, 1], [2, 2]] })),
  re("line-rebase-string-x", savedLinear("line", { x: "10", y: "20", points: [[1, 2], [3, 4]] })),
  re("line-no-rebase-string-x", savedLinear("line", { x: "10", y: "20" })),
  re("line-rebase-fractional", savedLinear("line", { x: 0.1, y: 0.2, points: [[0.3, 0.7], [1.1, 2.9]] })),
  re("line-one-point", savedLinear("line", { points: [[5, 5]], width: 30, height: 40 })),
  re("line-no-points-size-fallback", savedLinear("line", { points: [], width: "30", height: null })),
  re("line-no-points-negative-size", savedLinear("line", { points: [], width: -30, height: -40 })),
  re("line-points-missing", without(savedLinear("line"), "points")),
  re("line-negative-width-stored", savedLinear("line", { width: -100 })),
  re("line-size-from-points", savedLinear("line", {
    width: 1,
    height: 1,
    points: [[0, 0], [-10, 30], [40, -20]],
  })),
  // polygon (restore.ts:645-651, typeChecks.ts:397-401, point.ts:108-115)
  re("line-polygon-valid", savedLinear("line", {
    polygon: true,
    points: [[0, 0], [10, 0], [10, 10], [0, 0]],
  })),
  re("line-polygon-three-points", savedLinear("line", {
    polygon: true,
    points: [[0, 0], [10, 0], [0, 0]],
  })),
  re("line-polygon-open", savedLinear("line", {
    polygon: true,
    points: [[0, 0], [10, 0], [10, 10], [0, 5]],
  })),
  re("line-polygon-within-tolerance", savedLinear("line", {
    polygon: true,
    points: [[0, 0], [10, 0], [10, 10], [0.00009, -0.00009]],
  })),
  re("line-polygon-outside-tolerance", savedLinear("line", {
    polygon: true,
    points: [[0, 0], [10, 0], [10, 10], [0.0001, 0]],
  })),
  re("line-polygon-missing-closed", without(savedLinear("line", {
    points: [[0, 0], [10, 0], [10, 10], [0, 0]],
  }), "polygon")),
  re("line-polygon-null-closed", savedLinear("line", {
    polygon: null,
    points: [[0, 0], [10, 0], [10, 10], [0, 0]],
  })),
  re("line-polygon-kept-verbatim", savedLinear("line", {
    polygon: "yes",
    points: [[0, 0], [10, 0], [10, 10], [0, 0]],
  })),
  re("line-polygon-rebased-closed", savedLinear("line", {
    polygon: true,
    points: [[5, 5], [15, 5], [15, 15], [5, 5]],
  })),
  re("draw-polygon-dropped", savedLinear("draw", {
    polygon: true,
    points: [[0, 0], [10, 0], [10, 10], [0, 0]],
  })),
  // 75,000 px cap (restore.ts:126-157)
  re("line-width-at-cap", savedLinear("line", { points: [[0, 0], [75000, 10]] })),
  re("line-width-over-cap", savedLinear("line", { points: [[0, 0], [75000.5, 10]] })),
  re("line-height-over-cap", savedLinear("line", { points: [[0, 0], [10, -80000]] })),
  re("line-over-cap-keeps-keys", savedLinear("line", {
    future: 1,
    points: [[3, 3], [100000, 3]],
    isDeleted: false,
  })),
  re("line-overflow-infinite-size", savedLinear("line", {
    points: [[-1.7e308, 0], [1.7e308, 0]],
  })),
  re("draw-over-cap", savedLinear("draw", { points: [[0, 0], [0, 90000]] })),
  re("line-link-throws", savedLinear("line", { link: {} })),
];

// -- arrow -----------------------------------------------------------------------------

const rect = (id, rest = {}) =>
  saved("rectangle", { id, x: 100, y: 0, width: 100, height: 100, ...rest });

/** An arrow at (0, 50) whose legacy bindings are migrated with geometry. */
const legacyArrow = (rest = {}) => savedArrow({ id: "arrow", x: 0, y: 50, ...rest });

/** Legacy binding cases: [name, arrow fields, targets besides the arrow, existing]. */
const legacyGeometryCases = () =>
  [
    ["rotated-target", { points: [[0, 0], [105, 10]], endBinding: { elementId: "r1", focus: 0.2, gap: 3 } },
      [rect("r1", { angle: 0.6 })]],
    ["rotated-target-inside", { points: [[0, 0], [140, -5]], endBinding: { elementId: "r1", focus: 0, gap: 1 } },
      [rect("r1", { type: "diamond", angle: 1.1 })]],
    ["ellipse-inside", { points: [[0, 0], [150, 5]], endBinding: { elementId: "r1", focus: 0, gap: 1 } },
      [rect("r1", { type: "ellipse" })]],
    ["ellipse-outside-corner", { points: [[0, 0], [104, -45]], endBinding: { elementId: "r1", focus: 0.5, gap: 8 } },
      [rect("r1", { type: "ellipse" })]],
    ["diamond-projection", { points: [[0, 0], [110, -40]], endBinding: { elementId: "r1", focus: -0.3, gap: 4 } },
      [rect("r1", { type: "diamond" })]],
    ["diamond-corner", { points: [[0, 0], [99, -49]], endBinding: { elementId: "r1", focus: 0, gap: 4 } },
      [rect("r1", { type: "diamond" })]],
    ["midpoint-snap", { points: [[0, 0], [97, 2]], endBinding: { elementId: "r1", focus: 0, gap: 3 } },
      [rect("r1")]],
    ["midpoint-snap-top", { x: 150, y: -60, points: [[0, 0], [2, 57]], startBinding: null,
      endBinding: { elementId: "r1", focus: 0, gap: 3 } }, [rect("r1")]],
    ["far-from-target", { points: [[0, 0], [40, 30]], endBinding: { elementId: "r1", focus: 0, gap: 60 } },
      [rect("r1")]],
    ["multi-point-both-ends", {
      points: [[0, 0], [60, -40], [120, 0]],
      startBinding: { elementId: "r0", focus: 0.1, gap: 2 },
      endBinding: { elementId: "r1", focus: -0.1, gap: 2 },
    }, [rect("r0", { x: -100, y: 20, width: 90, height: 70 }), rect("r1", { x: 125, y: 10 })]],
    ["rotated-arrow", { angle: 0.7, points: [[0, 0], [95, 20]], endBinding: { elementId: "r1", focus: 0, gap: 5 } },
      [rect("r1")]],
    ["other-end-fixed-point", {
      points: [[0, 0], [96, 20]],
      startBinding: { elementId: "r0", focus: 0, gap: 1, fixedPoint: [1, 0.3] },
      endBinding: { elementId: "r1", focus: 0, gap: 4 },
    }, [rect("r0", { x: -60, y: 20, width: 60, height: 60 }), rect("r1")]],
    ["other-end-bad-fixed-point", {
      points: [[0, 0], [96, 20]],
      startBinding: { elementId: "r0", focus: 0, gap: 1, fixedPoint: [0.5, "x"] },
      endBinding: { elementId: "r1", focus: 0, gap: 4 },
    }, [rect("r0", { x: -60, y: 20, width: 60, height: 60 }), rect("r1")]],
    ["tiny-target", { points: [[0, 0], [99, 0]], endBinding: { elementId: "r1", focus: 0, gap: 1 } },
      [rect("r1", { y: 49.8, width: 0.5, height: 0.5 })]],
    ["narrow-target", { points: [[0, 0], [98, 3]], endBinding: { elementId: "r1", focus: 0, gap: 1 } },
      [rect("r1", { width: 3, height: 200, y: -50 })]],
    ["short-arrow", { x: 96, y: 30, points: [[0, 0], [2, 2]], endBinding: { elementId: "r1", focus: 0, gap: 1 } },
      [rect("r1")]],
    ["rounded-target", { points: [[0, 0], [102, -48]], endBinding: { elementId: "r1", focus: 0, gap: 1 } },
      [rect("r1", { roundness: { type: 3 } })]],
    ["text-target", { points: [[0, 0], [95, 0]], endBinding: { elementId: "t1", focus: 0, gap: 5 } },
      [saved("text", { id: "t1", x: 100, y: 30, width: 80, height: 25, text: "label", fontSize: 20,
        fontFamily: 1, textAlign: "left", verticalAlign: "top", containerId: null, originalText: "label",
        lineHeight: 1.25, autoResize: true })]],
    ["frame-target", { points: [[0, 0], [130, 0]], endBinding: { elementId: "f1", focus: 0, gap: 5 } },
      [saved("frame", { id: "f1", x: 100, y: 0, width: 100, height: 100, name: null })]],
    ["image-target", { points: [[0, 0], [95, 0]], endBinding: { elementId: "i1", focus: 0, gap: 5 } },
      [saved("image", { id: "i1", x: 100, y: 0, width: 100, height: 100, fileId: null, status: "pending",
        scale: [1, 1], crop: null })]],
    ["line-target", { points: [[0, 0], [95, 0]], endBinding: { elementId: "l1", focus: 0, gap: 5 } },
      [savedLinear("line", { id: "l1", x: 100, y: 0, points: [[0, 0], [100, 100]], polygon: false })]],
    ["closed-line-target", { points: [[0, 0], [150, 0]], endBinding: { elementId: "l1", focus: 0, gap: 5 } },
      [savedLinear("line", { id: "l1", x: 100, y: 0, width: 100, height: 100,
        points: [[0, 0], [100, 0], [100, 100], [0, 100], [0, 0]], polygon: true })]],
    ["deleted-target", { points: [[0, 0], [95, 0]], endBinding: { elementId: "r1", focus: 0, gap: 5 } },
      [rect("r1", { isDeleted: true })]],
    ["target-in-a-frame", { points: [[0, 0], [95, 0]], endBinding: { elementId: "r1", focus: 0, gap: 5 } },
      [rect("r1", { frameId: "f1" }), saved("frame", { id: "f1", x: 50, y: -50, width: 300, height: 300, name: null })]],
    ["container-target", { points: [[0, 0], [95, 0]], endBinding: { elementId: "r1", focus: 0, gap: 5 } },
      [rect("r1", { boundElements: [{ id: "t1", type: "text" }] }),
        saved("text", { id: "t1", x: 120, y: 40, width: 60, height: 25, text: "in", fontSize: 20,
          fontFamily: 1, textAlign: "center", verticalAlign: "middle", containerId: "r1",
          originalText: "in", lineHeight: 1.25, autoResize: true })]],
    ["arrow-missing-size", { points: [[0, 0], [95, 0]], width: undefined, height: undefined,
      endBinding: { elementId: "r1", focus: 0, gap: 5 } }, [rect("r1")]],
    ["arrow-string-x", { x: "0", points: [[0, 0], [95, 0]], endBinding: { elementId: "r1", focus: 0, gap: 5 } },
      [rect("r1")]],
    ["thick-target", { points: [[0, 0], [85, 3]], endBinding: { elementId: "r1", focus: 0, gap: 5 } },
      [rect("r1", { strokeWidth: 16 })]],
    ["start-end-same-target", {
      points: [[0, 0], [30, -80], [140, 5]],
      startBinding: { elementId: "r1", focus: 0, gap: 5 },
      endBinding: { elementId: "r1", focus: 0, gap: 5 },
    }, [rect("r1", { x: 0, y: 0 })]],
    ["target-without-angle", { points: [[0, 0], [95, 20]], endBinding: { elementId: "r1", focus: 0, gap: 5 } },
      [without(rect("r1"), "angle")]],
    ["target-string-width", { points: [[0, 0], [95, 20]], endBinding: { elementId: "r1", focus: 0, gap: 5 } },
      [rect("r1", { width: "100" })]],
    ["both-in-existing", {
      points: [[0, 0], [95, 20]],
      startBinding: { elementId: "r0", focus: 0, gap: 1 },
      endBinding: { elementId: "r1", focus: 0, gap: 4 },
    }, [], [rect("r0", { x: -60, y: 20, width: 60, height: 60 }), rect("r1")]],
  ].map(([name, fields, targets, existing]) => {
    const arrow = legacyArrow(fields);
    for (const [key, value] of Object.entries(fields)) if (value === undefined) delete arrow[key];
    return re(`arrow-binding-legacy-geometry-${name}`, arrow, {
      targets: (up, a) => [a, ...targets],
      ...(existing ? { existing: () => existing } : {}),
    });
  });

const arrowCases = () => [
  re("arrow-saved", savedArrow()),
  re("arrow-minimal", { type: "arrow" }),
  re("arrow-end-arrowhead-missing", without(savedArrow(), "endArrowhead")),
  re("arrow-end-arrowhead-null", savedArrow({ endArrowhead: null })),
  re("arrow-arrowheads-legacy", savedArrow({ startArrowhead: "dot", endArrowhead: "crowfoot_many" })),
  re("arrow-arrowheads-unknown-kept", savedArrow({ startArrowhead: "harpoon", endArrowhead: 7 })),
  re("arrow-elbowed-missing", without(savedArrow(), "elbowed")),
  re("arrow-elbowed-null", savedArrow({ elbowed: null })),
  re("arrow-rebase", savedArrow({ points: [[10, 20], [30, 40], [0, 0]] })),
  re("arrow-rebase-string-x", savedArrow({ x: "10", y: "5", points: [[1, 1], [2, 2]] })),
  re("arrow-no-rebase-string-x", savedArrow({ x: "10", y: "5" })),
  re("arrow-missing-x", without(savedArrow(), "x", "y")),
  re("arrow-null-x", savedArrow({ x: null, y: null, points: [[2, 2], [4, 4]] })),
  re("arrow-points-missing-size-fallback", without(savedArrow({ width: 40, height: -30 }), "points")),
  re("arrow-over-cap", savedArrow({ points: [[0, 0], [0, 75001]] })),
  re("arrow-negative-width-stored", savedArrow({ width: -100, x: 50 })),
  re("arrow-strokeSharpness-round", savedArrow({ strokeSharpness: "round", roundness: null })),
  // bindings (restore.ts:298-428)
  re("arrow-binding-mode-kept", savedArrow({
    startBinding: { elementId: "r1", mode: "inside", fixedPoint: [0.25, 0.75], extra: 1 },
    endBinding: { elementId: "r2", mode: "orbit", fixedPoint: [0.5, 0.5] },
  }), { targets: (up, arrow) => [arrow] }),
  re("arrow-binding-mode-no-element-id", savedArrow({
    startBinding: { mode: "inside", fixedPoint: [0.1, 0.1] },
    endBinding: { elementId: "", mode: "orbit", fixedPoint: [0.1, 0.1] },
  })),
  re("arrow-binding-mode-fixed-point-variants", savedArrow({
    startBinding: { elementId: "a", mode: "skip", fixedPoint: [20, -20] },
    endBinding: { elementId: "b", mode: 1, fixedPoint: [0.50009, 0.2] },
  })),
  ...[
    ["missing"],
    ["null", null],
    ["short", [0.2]],
    ["long", [0.2, 0.3, 0.4]],
    ["strings", ["0.2", "0.3"]],
    ["nulls", [null, 0.3]],
    ["object", { 0: 0.2, 1: 0.3 }],
    ["half-exact", [0.5, 0.5]],
    ["half-one-axis", [0.5, 0.9]],
    ["near-half", [0.49995, 0.50004]],
    ["clamped", [11, -10.5]],
    ["negative-zero", [-0, 0.3]],
  ].map(([name, ...value]) => {
    const binding = { elementId: "r", mode: "orbit" };
    if (value.length) binding.fixedPoint = value[0];
    return re(`arrow-binding-fixed-point-${name}`, savedArrow({ startBinding: binding }));
  }),
  // legacy bindings (no mode): target not found anywhere -> null
  re("arrow-binding-legacy-no-target", savedArrow({
    startBinding: { elementId: "missing", focus: 0.2, gap: 4 },
    endBinding: { elementId: 42 },
  })),
  re("arrow-binding-legacy-primitive", savedArrow({ startBinding: 5, endBinding: "x" })),
  re("arrow-binding-falsy", savedArrow({ startBinding: 0, endBinding: "" })),
  re("arrow-binding-missing", without(savedArrow(), "startBinding", "endBinding")),
  // legacy bindings with a target: migrated by geometry (restore.ts:362-418)
  re(
    "arrow-binding-legacy-migrated-orbit",
    savedArrow({
      id: "arrow",
      x: 0,
      y: 50,
      points: [[0, 0], [95, 0]],
      endBinding: { elementId: "r1", focus: 0, gap: 5 },
    }),
    { targets: (up, arrow) => [arrow, rect("r1")] },
  ),
  re(
    "arrow-binding-legacy-migrated-inside",
    savedArrow({
      id: "arrow",
      x: 0,
      y: 50,
      points: [[0, 0], [150, 0]],
      endBinding: { elementId: "r1", focus: 0.1, gap: 1 },
      startBinding: { elementId: "r0", focus: 0, gap: 1 },
    }),
    {
      targets: (up, arrow) => [
        arrow,
        rect("r1"),
        rect("r0", { x: -50, y: 25, width: 40, height: 40, angle: 0.3 }),
      ],
    },
  ),
  re(
    "arrow-binding-legacy-migrated-existing",
    savedArrow({
      id: "arrow",
      x: 0,
      y: 50,
      points: [[0, 0], [95, 10]],
      endBinding: { elementId: "r1", focus: 0, gap: 5 },
    }),
    { existing: () => [rect("r1", { type: "ellipse" })] },
  ),
  re(
    "arrow-binding-legacy-target-map-wins",
    savedArrow({
      id: "arrow",
      x: 0,
      y: 50,
      points: [[0, 0], [95, 0]],
      startBinding: { elementId: "r1", focus: 0, gap: 5 },
    }),
    {
      targets: (up, arrow) => [arrow, rect("r1", { type: "diamond" })],
      existing: () => [rect("r1", { x: 1000 })],
    },
  ),
  re(
    "arrow-binding-legacy-migrated-duplicate-id-last-wins",
    savedArrow({
      id: "arrow",
      x: 0,
      y: 50,
      points: [[0, 0], [95, 0]],
      endBinding: { elementId: "r1", focus: 0, gap: 5 },
    }),
    { targets: (up, arrow) => [arrow, rect("r1", { x: 1000 }), rect("r1")] },
  ),
  re(
    "arrow-binding-legacy-numeric-id",
    savedArrow({
      id: "arrow",
      x: 0,
      y: 50,
      points: [[0, 0], [95, 0]],
      endBinding: { elementId: 7, focus: 0, gap: 5 },
    }),
    { targets: (up, arrow) => [arrow, rect(7), rect("7", { x: 1000 })] },
  ),
  // the migration's geometry (ex-116): mode from isPointInElement, focus
  // point from projectFixedPointOntoDiagonal at DEFAULT_ZOOM (midpoint
  // snapping, the diagonals, the other end's fixed point for a two-point
  // arrow), fixedPoint from calculateFixedPointForNonElbowArrowBinding
  ...legacyGeometryCases(),
  // elbow arrows (restore.ts:315-325, 702-712)
  re("elbow-saved", savedElbow()),
  re("elbow-bindings", savedElbow({
    startBinding: { elementId: "a", fixedPoint: [0.5, 1], mode: "inside", extra: true },
    endBinding: { elementId: "b", fixedPoint: null },
  })),
  re("elbow-binding-legacy-without-target", savedElbow({
    startBinding: { elementId: "missing", focus: 0, gap: 1 },
    endBinding: { mode: "" },
  })),
  re("elbow-binding-primitives", savedElbow({ startBinding: "ab", endBinding: 3 })),
  re("elbow-binding-array", savedElbow({ startBinding: [1, 2], endBinding: true })),
  re("elbow-binding-key-order", savedElbow({
    startBinding: { mode: "orbit", fixedPoint: [0.2, 0.2], elementId: "a" },
    endBinding: { fixedPoint: [0.2, 0.2], elementId: "b" },
  })),
  re("elbow-fixed-segments-kept", savedElbow({
    fixedSegments: [{ index: 2, start: [50, 0], end: [50, 50] }],
  })),
  re("elbow-fixed-segments-empty", savedElbow({ fixedSegments: [] })),
  re("elbow-fixed-segments-three-points", savedElbow({
    points: [[0, 0], [50, 0], [50, 50]],
    fixedSegments: [{ index: 1, start: [0, 0], end: [50, 0] }],
  })),
  re("elbow-fixed-segments-string", savedElbow({ fixedSegments: "ab" })),
  re("elbow-fixed-segments-number", savedElbow({ fixedSegments: 3 })),
  re("elbow-fixed-segments-length-object", savedElbow({ fixedSegments: { length: 1 } })),
  re("elbow-fixed-segments-missing", without(savedElbow(), "fixedSegments")),
  re("elbow-special-flags", savedElbow({ startIsSpecial: true, endIsSpecial: false })),
  re("elbow-special-flags-missing", without(savedElbow(), "startIsSpecial", "endIsSpecial")),
  re("elbow-elbowed-truthy", savedElbow({ elbowed: 1 })),
  re("elbow-rebase", savedElbow({
    points: [[10, 10], [60, 10], [60, 60], [110, 60]],
    fixedSegments: [{ index: 2, start: [60, 10], end: [60, 60] }],
  })),
  re("elbow-over-cap", savedElbow({
    points: [[0, 0], [80000, 0], [80000, 10], [80001, 10]],
  })),
];

// -- the rest ----------------------------------------------------------------------------

const genericCases = () => [
  ...["rectangle", "diamond", "ellipse", "iframe", "embeddable"].map((type) =>
    re(`generic-${type}`, saved(type, { future: [1] })),
  ),
  re("frame-saved", saved("frame", { name: "Frame 1" })),
  re("frame-name-missing", without(saved("frame"), "name")),
  re("frame-name-null", saved("frame", { name: null })),
  re("frame-name-empty-kept", saved("frame", { name: "" })),
  re("magicframe-name-missing", saved("magicframe")),
  re("magicframe-name-number", saved("magicframe", { name: 3 })),
  re("unknown-type", saved("hexagon")),
  re("unknown-type-case", saved("Rectangle")),
  re("missing-type", without(saved("rectangle"), "type")),
  re("selection-type", saved("selection")),
  re("type-number", saved(5)),
];

const TRANSPARENT_VARIANTS = [
  "transparent",
  " Transparent ",
  "TRANSPARENT",
  "#00000000",
  "#fff0",
  "#FFF0",
  "#fff1",
  "#ffffff00",
  "#ffffff01",
  "rgba(0,0,0,0)",
  "rgba(0, 0, 0, 0.0)",
  "hsla(0, 0%, 0%, 0)",
  "red",
  "#1e1e1e",
  "",
  "none",
];

const stickyCases = () => [
  re("sticky-saved", savedSticky()),
  re("sticky-minimal", { type: "stickynote" }),
  re("sticky-base-height-missing", without(savedSticky({ height: 300 }), "baseHeight")),
  re("sticky-base-height-legacy-max-height", without(savedSticky({ maxHeight: 280, height: 300 }), "baseHeight")),
  re("sticky-base-height-null-max-height", savedSticky({ baseHeight: null, maxHeight: 200 })),
  re("sticky-base-height-zero-kept", savedSticky({ baseHeight: 0, height: 120 })),
  re("sticky-base-height-above-height", savedSticky({ baseHeight: 400, height: 250 })),
  re("sticky-small", savedSticky({ width: 20, height: 30, baseHeight: 10 })),
  re("sticky-zero-size", savedSticky({ width: 0, height: 0, baseHeight: 0 })),
  re("sticky-string-sizes", savedSticky({ width: "300", height: "260", baseHeight: "250" })),
  re("sticky-object-width", savedSticky({ width: { w: 1 } })),
  re("sticky-fill-style-forced", savedSticky({ fillStyle: "hachure" })),
  re("sticky-already-normal-no-bump", savedSticky({ fillStyle: "solid" })),
  ...TRANSPARENT_VARIANTS.map((color, i) =>
    re(`sticky-stroke-${i}`, savedSticky({ strokeColor: color })),
  ),
  ...TRANSPARENT_VARIANTS.map((color, i) =>
    re(`sticky-background-${i}`, savedSticky({ backgroundColor: color })),
  ),
  re("sticky-colors-object-transparent", savedSticky({
    strokeColor: { r: 0, g: 0, b: 0, a: 0 },
    backgroundColor: { a: "0" },
  })),
  re("sticky-colors-object-opaque", savedSticky({
    strokeColor: { r: 1, g: 2, b: 3 },
    backgroundColor: { a: 0.5 },
  })),
  re("sticky-colors-number", savedSticky({ strokeColor: 5, backgroundColor: 7 })),
  re("sticky-colors-has-own-property-throws", savedSticky({
    strokeColor: { hasOwnProperty: 1, a: 0 },
  })),
  re("sticky-version-string", savedSticky({ version: "3", fillStyle: "hachure" })),
  re("sticky-version-object", savedSticky({ version: { v: 1 }, fillStyle: "hachure" })),
];

export const buildElementCases = () => [
  ...upstreamCases(),
  ...textCases(),
  ...freedrawCases(),
  ...imageCases(),
  ...lineCases(),
  ...arrowCases(),
  ...genericCases(),
  ...stickyCases(),
];
