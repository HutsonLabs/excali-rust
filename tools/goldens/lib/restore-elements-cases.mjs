// Inputs for restore-elements.json (ex-105): the scene-level passes of
// upstream's restoreElements (packages/excalidraw/data/restore.ts:946-1138)
// and bumpElementVersions (restore.ts:1150-1173).
//
// Two kinds of input, as in restore-element-cases.mjs:
//
// - `upstream-*` cases are the scene-level inputs of upstream's own
//   packages/excalidraw/tests/data/restore.test.ts ("restoreElements" and
//   "repairing bindings"), built the way that test builds them
//   (apiCreateElement is API.createElement). A test that needs a JS value a
//   file cannot hold (a throwing getter, a mocked helper) gets the nearest
//   file input with the same effect, noted where it happens.
// - the other cases are tables over each pass: duplicate ids, index repair,
//   invisibly small elements, frames, bound text in both directions, linear
//   bindings, sticky notes, bound text order, elbow arrow fix-ups, and the
//   inputs upstream throws on.
//
// A case is { id, call, elements, existing?, opts? } where call is
// "restoreElements" (restoreElements(elements, existing ?? null, opts)) or
// "bumpElementVersions" (bumpElementVersions(restoreElements(elements,
// null), existing)). elements and existing are arrays or builders (functions
// of the upstream module, run after reseed(1), so constructor output comes
// from upstream).

import {
  apiCreateElement,
  saved,
  savedArrow,
  savedElbow,
  savedFreedraw,
  savedLinear,
  savedSticky,
  savedText,
  without,
} from "./restore-element-cases.mjs";

const scene = (id, elements, more = {}) => ({ id, call: "restoreElements", elements, ...more });
const repair = { repairBindings: true };
const repairDelete = { repairBindings: true, deleteInvisibleElements: true };

const rect = (id, rest = {}) => saved("rectangle", { id, boundElements: [], ...rest });
const label = (id, containerId, rest = {}) => savedText({ id, containerId, ...rest });

// -- upstream restore.test.ts -----------------------------------------------------

const upstreamCases = () => [
  // "basic restoreElements" (restore.test.ts:52-59)
  scene("upstream-basic", (up) => [
    apiCreateElement(up, { type: "text" }),
    apiCreateElement(up, { type: "rectangle" }),
  ]),
  // "restores created=%s ..." (restore.test.ts:61-92): JSON drops undefined
  ...[
    ["123", 123],
    ["0", 0],
    ["null", null],
    ["undefined"],
  ].map(([name, ...created]) =>
    scene(`upstream-created-${name}`, (up) => {
      const element = {
        ...apiCreateElement(up, { type: "rectangle", index: "a0" }),
        updated: 456,
      };
      if (created.length) element.created = created[0];
      else delete element.created;
      return [element];
    }),
  ),
  // "preserves creation metadata when repairing duplicate element IDs"
  // (restore.test.ts:94-103)
  scene("upstream-duplicate-ids-created", (up) => {
    const element = apiCreateElement(up, { type: "rectangle", created: 123 });
    return [element, { ...element, created: null }];
  }),
  // "should restore transparent sticky note stroke as black" (105-114)
  scene("upstream-sticky-transparent-stroke", (up) => [
    { ...apiCreateElement(up, { type: "stickynote" }), strokeColor: "transparent" },
  ]),
  // "should not call isInvisiblySmallElement when element is a selection
  // element" (125-130): the selection is dropped even with
  // deleteInvisibleElements
  scene("upstream-selection", [{ type: "selection" }], { opts: { deleteInvisibleElements: true } }),
  // "should return empty array when input type is not supported" (132-141)
  scene("upstream-not-supported", (up) => [
    { ...apiCreateElement(up, { type: "text" }), type: "not supported" },
  ]),
  // "should return empty array when isInvisiblySmallElement is true"
  // (143-152) mocks the helper; a zero-size rectangle is invisibly small
  scene("upstream-invisibly-small-rectangle", (up) => [
    apiCreateElement(up, { type: "rectangle", width: 0, height: 0 }),
  ], { opts: { deleteInvisibleElements: true } }),
  // "should clamp restored font ceilings to the sticky note maximum" (232-245)
  scene("upstream-text-font-ceiling-clamped", (up) => [
    { ...apiCreateElement(up, { type: "text", text: "text" }), baseFontSize: 1e20 },
  ]),
  // "should clear a font ceiling on text that is not bound to a sticky note"
  // (247-262)
  scene("upstream-text-font-ceiling-cleared", (up) => [
    { ...apiCreateElement(up, { type: "text", text: "text", fontSize: 20 }), baseFontSize: 28 },
  ], { opts: repair }),
  // "should seed the font ceiling and a visible stroke on a sticky note
  // label" (264-291)
  scene("upstream-sticky-label-seeded", (up) => {
    const note = apiCreateElement(up, {
      type: "stickynote",
      id: "sticky",
      boundElements: [{ type: "text", id: "label" }],
    });
    const text = {
      ...apiCreateElement(up, {
        type: "text",
        id: "label",
        text: "text",
        fontSize: 20,
        containerId: "sticky",
      }),
      strokeColor: "transparent",
    };
    delete text.baseFontSize;
    return [note, text];
  }, { opts: repair }),
  // "should give a note its label's color when the two drifted apart"
  // (293-319)
  scene("upstream-sticky-color-drift", (up) => [
    apiCreateElement(up, {
      type: "stickynote",
      id: "sticky",
      strokeColor: "#1e1e1e",
      boundElements: [{ type: "text", id: "label" }],
    }),
    apiCreateElement(up, {
      type: "text",
      id: "label",
      text: "text",
      fontSize: 20,
      containerId: "sticky",
      strokeColor: "#e03131",
    }),
  ], { opts: repair }),
  // "should refit a sticky note together with its label when refreshing
  // dimensions" (342-379)
  scene("upstream-sticky-refit", (up) => [
    apiCreateElement(up, {
      type: "stickynote",
      id: "sticky",
      width: 250,
      height: 250,
      baseHeight: 250,
      boundElements: [{ type: "text", id: "label" }],
    }),
    apiCreateElement(up, {
      type: "text",
      id: "label",
      text: Array(40).fill("abcdefghijklmnopqrstuvwx").join("\n"),
      fontSize: 28,
      containerId: "sticky",
    }),
  ], { opts: { repairBindings: true, refreshDimensions: true } }),
  // "should strip element if restore fails" (539-567): a throwing getter
  // cannot be in a file; a non-string link throws in restoreElement the same
  // way
  scene("upstream-strip-failed-element", (up) => [
    apiCreateElement(up, { type: "rectangle", boundElements: [] }),
    { ...apiCreateElement(up, { type: "rectangle", boundElements: [] }), link: 7 },
  ]),
  // "should remove imperceptibly small elements" (569-590)
  scene("upstream-imperceptibly-small-arrow", (up) => [
    apiCreateElement(up, {
      type: "arrow",
      points: [
        [0, 0],
        [0.02, 0.05],
      ],
      x: 0,
      y: 0,
    }),
  ], { opts: { deleteInvisibleElements: true } }),
  // "should keep 'imperceptibly' small freedraw/line elements" (592-621)
  scene("upstream-keep-small-freedraw-line", (up) => [
    apiCreateElement(up, {
      type: "freedraw",
      points: [
        [0, 0],
        [0.0001, 0.0001],
      ],
      x: 0,
      y: 0,
    }),
    apiCreateElement(up, {
      type: "line",
      points: [
        [0, 0],
        [0.0001, 0.0001],
      ],
      x: 0,
      y: 0,
    }),
  ]),
  // "bump versions of local duplicate elements when supplied" (909-933)
  {
    id: "upstream-bump-versions-local-duplicates",
    call: "bumpElementVersions",
    elements: (up) => [
      apiCreateElement(up, { type: "rectangle", id: "rect" }),
      apiCreateElement(up, { type: "ellipse", id: "ellipse" }),
    ],
    existing: (up) => [
      up.newElementWith(apiCreateElement(up, { type: "rectangle", id: "rect" }), {
        isDeleted: true,
      }),
    ],
  },
  // "... even if both have same version" (935-970): the second restore of a
  // restored element keeps its version, and a local copy with the same
  // version but another versionNonce is bumped past
  scene("upstream-restore-twice-first", (up) => [
    apiCreateElement(up, { type: "rectangle", id: "rect" }),
  ]),
  {
    id: "upstream-bump-versions-same-version",
    call: "bumpElementVersions",
    elements: [
      saved("rectangle", { id: "rect", index: "a0", version: 2, versionNonce: 11, width: 500 }),
    ],
    existing: [
      saved("rectangle", { id: "rect", index: "a0", version: 2, versionNonce: 22, width: 600 }),
    ],
  },

  // "repairing bindings" (restore.test.ts:1223-1553)
  ...["arrow", "rectangle"].map((containerType) =>
    scene(`upstream-repair-label-order-${containerType}`, (up) => {
      const container = apiCreateElement(up, {
        type: containerType,
        id: "container",
        index: "b2f",
        boundElements: [{ type: "text", id: "label" }],
      });
      const text = apiCreateElement(up, {
        type: "text",
        id: "label",
        index: "b2a",
        containerId: container.id,
      });
      return [text, container];
    }, { opts: repair }),
  ),
  scene("upstream-strip-arrow-binding", (up) => {
    const container = apiCreateElement(up, { type: "rectangle", boundElements: [] });
    const arrow = apiCreateElement(up, {
      type: "arrow",
      id: "id-arrow01",
      endBinding: { elementId: container.id, fixedPoint: [0.5, 0.5], mode: "inside" },
    });
    Object.assign(container, { boundElements: [{ type: "arrow", id: arrow.id }] });
    Object.assign(arrow, { endBinding: { elementId: 42 } });
    return [arrow, container];
  }),
  ...[
    ["no-repair", undefined],
    ["repair", repair],
  ].flatMap(([name, opts]) => [
    scene(`upstream-container-bound-elements-${name}`, (up) => {
      const container = apiCreateElement(up, { type: "rectangle", boundElements: [] });
      const text = apiCreateElement(up, { type: "text", containerId: container.id });
      return [container, text];
    }, opts ? { opts } : {}),
    scene(`upstream-bound-element-container-id-${name}`, (up) => {
      const text = apiCreateElement(up, { type: "text", containerId: null });
      const container = apiCreateElement(up, {
        type: "rectangle",
        boundElements: [{ type: text.type, id: text.id }],
      });
      return [container, text];
    }, opts ? { opts } : {}),
    scene(`upstream-missing-container-${name}`, (up) => [
      apiCreateElement(up, { type: "text", containerId: "non-existent" }),
      apiCreateElement(up, { type: "text", containerId: "non-existent", isDeleted: true }),
    ], opts ? { opts } : {}),
  ]),
  scene("upstream-ignore-deleted-bound-element", (up) => {
    const container = apiCreateElement(up, { type: "rectangle", boundElements: [] });
    const text = apiCreateElement(up, {
      type: "text",
      containerId: container.id,
      isDeleted: true,
    });
    return [container, text];
  }),
  ...[
    ["no-repair", { deleteInvisibleElements: true }],
    ["repair", repairDelete],
  ].map(([name, opts]) =>
    scene(`upstream-remove-deleted-bindings-${name}`, (up) => {
      const container = apiCreateElement(up, { type: "rectangle", boundElements: [] });
      const deleted = apiCreateElement(up, {
        type: "text",
        containerId: container.id,
        isDeleted: true,
      });
      const invisible = apiCreateElement(up, {
        type: "text",
        containerId: container.id,
        width: 0,
        height: 0,
      });
      container.boundElements = [
        { type: deleted.type, id: deleted.id },
        { type: invisible.type, id: invisible.id },
        { type: "text", id: "non-existent" },
      ];
      return [container, invisible, deleted];
    }, { opts }),
  ),
];

// -- duplicate ids ----------------------------------------------------------------

const idCases = () => [
  scene("ids-three-duplicates", [rect("r"), rect("r", { x: 50 }), rect("r", { x: 90 })]),
  // 7 and "7" are different Set keys
  scene("ids-number-and-string", [rect(7), rect("7"), rect(7)]),
  // a missing id takes randomId() (id0), and the later explicit "id0" is then
  // a duplicate that takes id1
  scene("ids-random-id-collides", [without(rect("x"), "id"), rect("id0")]),
  scene("ids-duplicate-text-and-container", [
    rect("r", { boundElements: [{ type: "text", id: "t" }] }),
    label("t", "r"),
    label("t", "r", { text: "second" }),
  ], { opts: repair }),
  // the dropped element's id is still in the targets map, but never in the Set
  scene("ids-dropped-element-not-counted", [{ id: "a", type: "unknown" }, rect("a")]),
];

// -- indices ------------------------------------------------------------------------

const indexCases = () => [
  scene("index-none", [rect("a", { index: null }), rect("b", { index: null }), rect("c", { index: null })]),
  scene("index-all-valid", [rect("a", { index: "a0" }), rect("b", { index: "a1" }), rect("c", { index: "a2" })]),
  scene("index-out-of-order", [rect("a", { index: "a2" }), rect("b", { index: "a1" }), rect("c", { index: "a3" })]),
  scene("index-duplicates", [rect("a", { index: "a1" }), rect("b", { index: "a1" }), rect("c", { index: "a1" })]),
  scene("index-malformed", [rect("a", { index: "a0" }), rect("b", { index: "a10" }), rect("c", { index: "zzz" })]),
  scene("index-gap-filled", [
    rect("a", { index: "a0" }),
    rect("b", { index: null }),
    rect("c", { index: null }),
    rect("d", { index: "a1" }),
  ]),
  scene("index-empty-string", [rect("a", { index: "" }), rect("b", { index: "a5" })]),
  scene("index-version-string", [rect("a", { index: null, version: "3" })]),
  scene("index-version-missing", [without(rect("a", { index: null }), "version")]),
  scene("index-with-existing-elements", [rect("a", { index: null })], {
    existing: [rect("a", { index: "a0", version: 9 })],
  }),
];

// -- invisibly small elements --------------------------------------------------------

const invisibleCases = () => [
  scene("invisible-zero-rectangle", [rect("r", { width: 0, height: 0 })], {
    opts: { deleteInvisibleElements: true },
  }),
  scene("invisible-zero-width-only", [rect("r", { width: 0, height: 5 })], {
    opts: { deleteInvisibleElements: true },
  }),
  scene("invisible-not-without-option", [rect("r", { width: 0, height: 0 })]),
  scene("invisible-bump-past-local-version", [rect("r", { width: 0, height: 0, version: 2 })], {
    existing: [rect("r", { version: 40 })],
    opts: { deleteInvisibleElements: true },
  }),
  scene("invisible-local-version-string", [rect("r", { width: 0, height: 0 })], {
    existing: [rect("r", { version: "40" })],
    opts: { deleteInvisibleElements: true },
  }),
  scene("invisible-arrow-same-points", [
    savedArrow({ id: "a", points: [[0, 0], [0.05, -0.05]] }),
    savedArrow({ id: "b", points: [[0, 0], [0.1, 0]] }),
    savedArrow({ id: "c", points: [[0, 0], [0.05, 0], [0, 0]] }),
  ], { opts: { deleteInvisibleElements: true } }),
  scene("invisible-single-point-linear", [
    savedLinear("line", { id: "l", points: [[0, 0]] }),
    savedArrow({ id: "a", points: [[3, 4]] }),
    savedFreedraw({ id: "f", points: [[1, 1]], pressures: [0.5] }),
  ], { opts: { deleteInvisibleElements: true } }),
  // the check reads the element as given: a draw element is not linear, and
  // a zero width and height counts even though restore turns it into a line
  scene("invisible-legacy-draw", [
    saved("draw", { id: "d", width: 0, height: 0, points: [[0, 0], [10, 10]] }),
  ], { opts: { deleteInvisibleElements: true } }),
  // already deleted: bumped and deleted again
  scene("invisible-already-deleted", [rect("r", { width: 0, height: 0, isDeleted: true })], {
    opts: { deleteInvisibleElements: true },
  }),
  // an empty text is deleted by restoreElement and again here
  scene("invisible-empty-text-zero-size", [savedText({ id: "t", text: "", width: 0, height: 0 })], {
    opts: { deleteInvisibleElements: true },
  }),
];

// -- frames -------------------------------------------------------------------------

const frameCases = () => [
  scene("frame-membership", [
    saved("frame", { id: "f", name: "F" }),
    rect("in", { frameId: "f" }),
    rect("missing", { frameId: "nope" }),
    rect("not-a-frame", { frameId: "in" }),
    rect("empty", { frameId: "" }),
    rect("number", { frameId: 5 }),
  ], { opts: repair }),
  scene("frame-membership-no-repair", [rect("missing", { frameId: "nope" })]),
  // a frame deleted as invisibly small still exists
  scene("frame-deleted-frame-kept", [
    saved("frame", { id: "f", width: 0, height: 0 }),
    rect("in", { frameId: "f" }),
  ], { opts: repairDelete }),
];

// -- bound text ---------------------------------------------------------------------

const boundTextCases = () => [
  scene("bound-text-angle-from-container", [
    rect("r", { angle: 1.5, boundElements: [{ type: "text", id: "t" }] }),
    label("t", "r", { angle: 0.3 }),
  ], { opts: repair }),
  scene("bound-text-angle-arrow-container", [
    savedArrow({ id: "a", angle: 1.5, boundElements: [{ type: "text", id: "t" }] }),
    label("t", "a", { angle: 0.3 }),
  ], { opts: repair }),
  scene("bound-text-angle-missing-container", [label("t", "gone", { angle: 0.3 })], {
    opts: repair,
  }),
  scene("bound-text-deleted-not-added", [
    rect("r", { boundElements: [] }),
    label("t", "r", { isDeleted: true, angle: 2 }),
  ], { opts: repair }),
  scene("bound-text-container-null-bound-elements", [
    rect("r", { boundElements: null }),
    label("t", "r"),
  ], { opts: repair }),
  scene("bound-text-appended-to-others", [
    rect("r", { boundElements: [{ type: "arrow", id: "a" }] }),
    savedArrow({ id: "a" }),
    label("t", "r"),
  ], { opts: repair }),
  scene("container-dedupes-and-drops", [
    rect("r", {
      boundElements: [
        { type: "text", id: "t" },
        { type: "arrow", id: "a" },
        { type: "text", id: "t" },
        { type: "arrow", id: "missing" },
        { type: "arrow", id: "a", extra: 1 },
        { type: "arrow", id: "d" },
      ],
    }),
    label("t", "r"),
    savedArrow({ id: "a" }),
    savedArrow({ id: "d", isDeleted: true }),
  ], { opts: repair }),
  // a text listed by a container without containerId gets it; one with
  // another container keeps it
  scene("container-sets-missing-container-id", [
    rect("r", { boundElements: [{ type: "text", id: "t1" }, { type: "text", id: "t2" }] }),
    rect("other", { boundElements: [] }),
    label("t1", null),
    label("t2", "other"),
  ], { opts: repair }),
  // a text that is itself listed as a container's bound element: only the
  // containerId branch runs for it
  scene("text-with-container-and-bound-elements", [
    rect("r", { boundElements: [{ type: "text", id: "t" }] }),
    label("t", "r", { boundElements: [{ type: "arrow", id: "nope" }] }),
  ], { opts: repair }),
  scene("text-without-container-bound-elements", [
    label("t", null, { boundElements: [{ type: "arrow", id: "nope" }, { type: "arrow", id: "a" }] }),
    savedArrow({ id: "a" }),
  ], { opts: repair }),
  scene("container-empty-string-container-id", [label("t", "", { angle: 1 })], { opts: repair }),
];

// -- linear bindings -----------------------------------------------------------------

const bindingCases = () => [
  scene("arrow-binding-target-only-in-existing", [
    savedArrow({
      id: "a",
      startBinding: { elementId: "r", fixedPoint: [0.25, 0.75], mode: "orbit" },
      endBinding: { elementId: "s", fixedPoint: [0.25, 0.75], mode: "inside" },
    }),
    rect("s"),
  ], { existing: [rect("r")], opts: repair }),
  scene("arrow-binding-target-dropped", [
    savedArrow({
      id: "a",
      endBinding: { elementId: "r", fixedPoint: [0.25, 0.75], mode: "orbit" },
    }),
    rect("r", { link: 5 }),
  ], { opts: repair }),
  scene("arrow-binding-kept-without-repair", [
    savedArrow({
      id: "a",
      endBinding: { elementId: "r", fixedPoint: [0.25, 0.75], mode: "orbit" },
    }),
  ], { existing: [rect("r")] }),
  scene("arrow-binding-to-deleted-target-kept", [
    savedArrow({
      id: "a",
      endBinding: { elementId: "r", fixedPoint: [0.25, 0.75], mode: "orbit" },
    }),
    rect("r", { isDeleted: true }),
  ], { opts: repair }),
];

// -- sticky notes ---------------------------------------------------------------------

const stickyCases = () => [
  scene("sticky-label-transparent-takes-note-color", [
    savedSticky({ id: "n", strokeColor: "#2f9e44", boundElements: [{ type: "text", id: "t" }] }),
    label("t", "n", { strokeColor: "transparent", fontSize: 24, baseFontSize: null }),
  ], { opts: repair }),
  scene("sticky-label-and-note-transparent", [
    savedSticky({ id: "n", boundElements: [{ type: "text", id: "t" }] }),
    label("t", "n", { strokeColor: "#ff000000" }),
  ], { opts: repair }),
  scene("sticky-label-base-font-size-kept", [
    savedSticky({ id: "n", boundElements: [{ type: "text", id: "t" }] }),
    label("t", "n", { fontSize: 12, baseFontSize: 36 }),
  ], { opts: repair }),
  scene("sticky-label-deleted-skipped", [
    savedSticky({ id: "n", strokeColor: "#1971c2", boundElements: [] }),
    label("t", "n", { strokeColor: "transparent", baseFontSize: 30, isDeleted: true }),
  ], { opts: repair }),
  scene("sticky-base-font-size-cleared-off-note", [
    rect("r", { boundElements: [{ type: "text", id: "t" }] }),
    label("t", "r", { baseFontSize: 30 }),
    savedText({ id: "free", baseFontSize: 40 }),
    savedText({ id: "free-null", baseFontSize: null }),
  ], { opts: repair }),
  // the container is found through the text's containerId after repair, so a
  // label whose note is missing is not a sticky label
  scene("sticky-note-missing", [label("t", "n", { baseFontSize: 30 })], { opts: repair }),
  scene("sticky-no-repair-no-pass", [
    savedSticky({ id: "n", strokeColor: "#1971c2", boundElements: [{ type: "text", id: "t" }] }),
    label("t", "n", { strokeColor: "transparent", baseFontSize: 30 }),
  ]),
  scene("sticky-refresh-dimensions", [
    savedSticky({ id: "n", boundElements: [{ type: "text", id: "t" }] }),
    label("t", "n", { text: "a short note", originalText: "a short note" }),
    savedSticky({ id: "empty", boundElements: [] }),
    savedSticky({ id: "deleted", isDeleted: true }),
    rect("r", { boundElements: [{ type: "text", id: "rt" }] }),
    label("rt", "r", { text: "x".repeat(30), originalText: "x".repeat(30) }),
    savedText({ id: "free", text: "free\ntext", originalText: "free\ntext" }),
    savedText({ id: "gone", isDeleted: true }),
  ], { opts: { repairBindings: true, refreshDimensions: true } }),
  scene("sticky-refresh-without-repair", [
    savedSticky({ id: "n", boundElements: [{ type: "text", id: "t" }] }),
    label("t", "n"),
  ], { opts: { refreshDimensions: true } }),
];

// -- bound text order ---------------------------------------------------------------

const orderCases = () => [
  scene("order-label-before-container", [
    label("t", "r", { index: "a0" }),
    rect("r", { index: "a1", boundElements: [{ type: "text", id: "t" }] }),
    rect("s", { index: "a2" }),
  ], { opts: repair }),
  scene("order-label-far-from-container", [
    rect("r", { index: "a0", boundElements: [{ type: "text", id: "t" }] }),
    rect("s", { index: "a1" }),
    rect("u", { index: "a2" }),
    label("t", "r", { index: "a3" }),
  ], { opts: repair }),
  scene("order-several-containers", [
    label("t2", "r2", { index: null }),
    label("t1", "r1", { index: null }),
    rect("r1", { index: null, boundElements: [{ type: "text", id: "t1" }] }),
    rect("r2", { index: null, boundElements: [{ type: "text", id: "t2" }] }),
  ], { opts: repair }),
  // the container does not list the text: the text stays where it is (the
  // repair adds it to the container first, so it moves after all)
  scene("order-container-does-not-list", [
    label("t", "r", { index: "a0" }),
    rect("r", { index: "a1", boundElements: null }),
  ], { opts: repair }),
  scene("order-listed-as-arrow-type", [
    label("t", "r", { index: "a0" }),
    rect("r", { index: "a1", boundElements: [{ type: "arrow", id: "t" }] }),
  ], { opts: repair }),
  scene("order-two-labels-one-container", [
    label("t1", "r", { index: "a0" }),
    label("t2", "r", { index: "a1" }),
    rect("r", {
      index: "a2",
      boundElements: [
        { type: "text", id: "t1" },
        { type: "text", id: "t2" },
      ],
    }),
  ], { opts: repair }),
  // the moved label's neighbours leave no room: syncMovedIndices falls back
  // to syncInvalidIndices
  scene("order-moved-fallback", [
    label("t", "r", { index: "a0" }),
    rect("r", { index: "a1", boundElements: [{ type: "text", id: "t" }] }),
    rect("s", { index: "a1V" }),
  ], { opts: repair }),
  scene("order-deleted-label-moves", [
    label("t", "r", { index: "a0", isDeleted: true }),
    rect("r", { index: "a1", boundElements: [{ type: "text", id: "t" }] }),
  ], { opts: repair }),
  scene("order-label-of-arrow", [
    label("t", "a", { index: "a0" }),
    savedArrow({ id: "a", index: "a1", boundElements: [{ type: "text", id: "t" }] }),
  ], { opts: repair }),
];

// -- elbow arrows ---------------------------------------------------------------------

const elbowCases = () => [
  // unbound with a diagonal segment: re-routed from [0, 0] to its last point
  scene("elbow-unbound-invalid-rerouted", [
    savedElbow({ id: "e", index: "a0", points: [[0, 0], [60, 40], [100, 50]] }),
  ], { opts: repair }),
  scene("elbow-unbound-valid-kept", [savedElbow({ id: "e", index: "a0" })], { opts: repair }),
  scene("elbow-unbound-invalid-no-repair", [
    savedElbow({ id: "e", points: [[0, 0], [60, 40], [100, 50]] }),
  ]),
  scene("elbow-bound-invalid-kept", [
    rect("r", { x: 200, y: 0 }),
    savedElbow({
      id: "e",
      points: [[0, 0], [60, 40], [100, 50]],
      endBinding: { elementId: "r", fixedPoint: [0, 0.5001], mode: "orbit" },
    }),
  ], { opts: repair }),
  scene("elbow-self-bound-fixed", [
    rect("r", { x: 100, y: 200, width: 80, height: 60 }),
    savedElbow({
      id: "e",
      points: [[0, 0], [0, -2e6], [50, -2e6], [50, 0]],
      startBinding: { elementId: "r", fixedPoint: [0.5001, 0], mode: "orbit" },
      endBinding: { elementId: "r", fixedPoint: [1, 0.5001], mode: "orbit" },
    }),
  ], { opts: repair }),
  scene("elbow-self-bound-small-kept", [
    rect("r", { x: 100, y: 200 }),
    savedElbow({
      id: "e",
      startBinding: { elementId: "r", fixedPoint: [0.5001, 0], mode: "orbit" },
      endBinding: { elementId: "r", fixedPoint: [1, 0.5001], mode: "orbit" },
    }),
  ], { opts: repair }),
  scene("elbow-plain-arrow-not-touched", [
    savedArrow({ id: "a", points: [[0, 0], [60, 40], [100, 50]] }),
  ], { opts: repair }),
];

// -- errors ---------------------------------------------------------------------------

const errorCases = () => [
  scene("throws-null-element", [rect("r"), null]),
  scene("throws-invisible-check-without-points", [
    without(savedLinear("line", { id: "l" }), "points"),
  ], { opts: { deleteInvisibleElements: true } }),
  scene("throws-bound-elements-string", [rect("r", { boundElements: "t" })], { opts: repair }),
  scene("throws-bound-elements-object", [rect("r", { boundElements: { id: "t" } })], {
    opts: repair,
  }),
  scene("throws-bound-element-null", [rect("r", { boundElements: [null] })], { opts: repair }),
  scene("throws-container-bound-elements-not-array", [
    rect("r", { boundElements: 5 }),
    label("t", "r"),
  ], { opts: repair }),
  scene("bound-elements-not-array-without-repair", [rect("r", { boundElements: "t" })]),
];

// -- whole scenes ------------------------------------------------------------------------

const sceneCases = () => [
  scene("scene-mixed", [
    saved("frame", { id: "frame", index: "a0", name: "Frame" }),
    rect("box", {
      index: "a1",
      frameId: "frame",
      angle: 0.25,
      boundElements: [
        { type: "arrow", id: "arrow" },
        { type: "text", id: "box-label" },
      ],
    }),
    savedArrow({
      id: "arrow",
      index: "a2",
      startBinding: { elementId: "box", fixedPoint: [1, 0.5001], mode: "orbit" },
      endBinding: { elementId: "gone", fixedPoint: [0, 0.5001], mode: "orbit" },
      boundElements: [{ type: "text", id: "arrow-label" }],
    }),
    label("arrow-label", "arrow", { index: "a3", angle: 1 }),
    savedSticky({ id: "note", index: "a4", boundElements: [{ type: "text", id: "note-label" }] }),
    label("note-label", "note", { index: "a5", strokeColor: "transparent" }),
    label("box-label", "box", { index: "a6" }),
    rect("box", { index: "a7", x: 500 }),
    savedLinear("line", {
      id: "line",
      index: "a8",
      frameId: "frame-gone",
      startBinding: { elementId: "box", fixedPoint: [0, 0], mode: "orbit" },
    }),
    rect("tiny", { index: "a9", width: 0, height: 0 }),
  ], { opts: repairDelete }),
];

export const buildScenesCases = () => [
  ...upstreamCases(),
  ...idCases(),
  ...indexCases(),
  ...invisibleCases(),
  ...frameCases(),
  ...boundTextCases(),
  ...bindingCases(),
  ...stickyCases(),
  ...orderCases(),
  ...elbowCases(),
  ...errorCases(),
  ...sceneCases(),
];
