+++
title = "Data model and file formats"
description = "Element types, enumerations, .excalidraw and .excalidrawlib formats, restore/migration rules, clipboard and embedding formats, fractional indexing, binding model, text measurement, and reusable fixtures."
weight = 10
[extra]
source_commit = "438d89861f53d8a90ad566113ecac1b83761098f"
source_repo = "https://github.com/excalidraw/excalidraw"
captured = "2026-09-28"
+++

**Repo:** `<checkout>` at commit `438d898` (2026-09-28). Every path below is relative to that root.

**Read this first.** This commit is ahead of the last release. `packages/excalidraw/package.json:3` says `0.18.0`, and `CHANGELOG.md:14` has an `## Unreleased` section. The unreleased schema changes include:
- a new `stickynote` element (`CHANGELOG.md:23-26`);
- new element fields `created`, `baseFontSize`, `labelPosition` and freedraw `strokeOptions`;
- `FixedPointBinding.mode` on bindings.

Files written by older builds will lack these fields, and `restore.ts` fills them in.

---

## 1. Element type union

### 1.1 Type strings
The element union is at `packages/element/src/types.ts:223-234`. `isExcalidrawElement` lists every type string (`packages/element/src/typeChecks.ts:255-284`):

`"selection" | "rectangle" | "stickynote" | "diamond" | "ellipse" | "embeddable" | "iframe" | "image" | "frame" | "magicframe" | "text" | "line" | "arrow" | "freedraw"`

- **Legacy `"draw"`**: accepted on restore and turned into `"line"` (`packages/excalidraw/data/restore.ts:612-637`).
- **`"selection"`**: dropped on restore (`restore.ts:969-971`).
- **Unknown types**: dropped, because `restoreElement` returns `null` (`restore.ts:747-751`).

### 1.2 Base fields (`_ExcalidrawElementBase`), verbatim from `packages/element/src/types.ts:40-87`
```ts
type _ExcalidrawElementBase = Readonly<{
  id: string;
  x: number;
  y: number;
  strokeColor: string;
  backgroundColor: string;
  fillStyle: FillStyle;
  strokeWidth: number;
  strokeStyle: StrokeStyle;
  roundness: null | { type: RoundnessType; value?: number };
  roughness: number;
  opacity: number;
  width: number;
  height: number;
  angle: Radians;
  /** Random integer used to seed shape generation so that the roughjs shape
      doesn't differ across renders. */
  seed: number;
  /** Integer that is sequentially incremented on each change. Used to reconcile
      elements during collaboration or when saving to server. */
  version: number;
  /** Random integer that is regenerated on each change.
      Used for deterministic reconciliation of updates during collaboration,
      in case the versions (see above) are identical. */
  versionNonce: number;
  /** String in a fractional form defined by https://github.com/rocicorp/fractional-indexing.
      Used for ordering in multiplayer scenarios, such as during reconciliation or undo / redo.
      Always kept in sync with the array order by `syncMovedIndices` and `syncInvalidIndices`.
      Could be null, i.e. for new elements which were not yet assigned to the scene. */
  index: FractionalIndex | null;
  isDeleted: boolean;
  /** List of groups the element belongs to.
      Ordered from deepest to shallowest. */
  groupIds: readonly GroupId[];
  frameId: string | null;
  /** other elements that are bound to this element */
  boundElements: readonly BoundElement[] | null;
  /** epoch (ms) timestamp of last element update */
  updated: number;
  /** Client wall-clock creation time in epoch milliseconds; null if unknown.
      Preserved for this element's lifetime, including edits and undo/redo,
      and excluded from `ElementUpdate` (mutateElement / newElementWith).
      Duplicating an element starts a new lifetime. Not an ordering clock. */
  created: number | null;
  link: string | null;
  locked: boolean;
  customData?: Record<string, any>;
}>;
```

Supporting types:
- `BoundElement = { id: string; type: "arrow" | "text" }` (`types.ts:35-38`).
- `FractionalIndex` is a branded string (`types.ts:33`); `GroupId = string` (`types.ts:24`).
- `Radians` is a branded number (`packages/math/src/types.ts:9`).
- `LocalPoint` and `GlobalPoint` are `[x: number, y: number]` tuples (`packages/math/src/types.ts:34,52`). In JSON a point is a 2-element array.

### 1.3 Type-specific fields

| type | Interface (`packages/element/src/types.ts`) | Extra fields |
|---|---|---|
| `selection` | 89-91 | none (never persisted) |
| `rectangle` | 93-95 | none |
| `diamond` | 107-109 | none |
| `ellipse` | 111-113 | none |
| `stickynote` | 97-105 | `baseHeight: number` (the height the user set; `height` may grow above it to fit the label) |
| `embeddable` | 115-118 | none; the URL lives in base `link` |
| `iframe` | 131-136 | `customData?: { generationData?: MagicGenerationData }` |
| `image` | 161-171 | `fileId: FileId \| null`, `status: "pending" \| "saved" \| "error"`, `scale: [number, number]` (values -1..1, used for flipping), `crop: ImageCrop \| null` |
| `frame` | 178-181 | `name: string \| null` |
| `magicframe` | 183-186 | `name: string \| null` |
| `text` | 253-291 | see below |
| `line` / `arrow` (shared linear fields) | 369-377 | `points: LocalPoint[]`, `startBinding`, `endBinding: FixedPointBinding \| null`, `startArrowhead`, `endArrowhead: Arrowhead \| null` |
| `line` | 379-383 | plus `polygon: boolean` |
| `arrow` | 391-395 | plus `elbowed: boolean` |
| elbow arrow (`elbowed: true`) | 397-421 | plus `fixedSegments: FixedSegment[] \| null`, `startIsSpecial: boolean \| null`, `endIsSpecial: boolean \| null` |
| `freedraw` | 430-437 | `points: LocalPoint[]`, `pressures: number[]`, `simulatePressure: boolean`, `strokeOptions: { variability: "variable" \| "constant"; streamline: number }` (`423-428`) |

Nested types used above:
- `MagicGenerationData` (`types.ts:120-129`): `{status:"pending"} | {status:"done"; html:string} | {status:"error"; message?:string; code:string}`.
- `ImageCrop` (`types.ts:152-159`): `{x, y, width, height, naturalWidth, naturalHeight}`.
- `FixedSegment` (`types.ts:385-389`): `{ start: LocalPoint; end: LocalPoint; index: number }`.
- `FixedPointBinding` (`types.ts:320-333`): `{ elementId: string; fixedPoint: [number, number]; mode: "inside" | "orbit" | "skip" }` (`BindMode` at `types.ts:318`).
- `IframeData` (`types.ts:142-150`) is runtime-only and not serialized.

**Text element fields** (`types.ts:253-291`):
- `fontSize: number`
- `fontFamily: FontFamilyValues` (a number id)
- `baseFontSize: number | null` (only meaningful for sticky-note labels)
- `text: string` (the wrapped text as rendered)
- `textAlign`, `verticalAlign`
- `containerId: string | null`
- `originalText: string` (unwrapped source text)
- `autoResize: boolean` (default true)
- `lineHeight: number` (unitless)
- `labelPosition?: number | null` (0–1 arc-length position on an arrow)

**Type groupings:**
- Bindable targets: `types.ts:293-303`. At runtime, text only counts as bindable when it has no `containerId` (`typeChecks.ts:184-202`).
- Text containers: `types.ts:305-310`, runtime set `packages/element/src/textElement.ts:508-514`: `rectangle`, `stickynote`, `diamond`, `ellipse`, `arrow`.
- Only `arrow` can bind to other elements (`typeChecks.ts:178-182`).
- Arrow subtypes (`typeChecks.ts:130-157`):
  - elbow: `elbowed == true`;
  - sharp: not elbowed and `roundness` is null;
  - curved: not elbowed and `roundness !== null`.
- Library items cannot contain `iframe`, `embeddable` or `image` (`packages/common/src/constants.ts:542-546`).

### 1.4 Defaults at construction
`_newElementBase` is at `packages/element/src/newElement.ts:87-172`. Defaults:
- `width`, `height`, `angle` = 0
- `groupIds` = `[]`
- `frameId`, `index`, `roundness`, `boundElements`, `link` = `null`
- `seed` = `randomInteger()`, i.e. `floor(rand * 2^31)` (`packages/common/src/random.ts:9`)
- `version` = 1, `versionNonce` = 0
- `updated` = `created` = `Date.now()`
- `id` = nanoid (`random.ts:16`)

Per-type constructors:
- `newTextElement`: `newElement.ts:335-391`.
- Freedraw (default `strokeOptions {variability:"variable", streamline:0.5}`): `newElement.ts:583-602`.
- Linear: `newElement.ts:604-631`.
- Arrow (an elbow arrow starts with `fixedSegments: []` and `start/endIsSpecial: false`): `newElement.ts:633-671`.
- Image (forces `strokeColor: "transparent"`, `status: "pending"`, `scale: [1,1]`): `newElement.ts:673-692`.
- Sticky note (normalizes to solid fill with non-transparent colors): `newElement.ts:186-243`.
- Frame / magicframe: `newElement.ts:263-295`.

Shared default style (`DEFAULT_ELEMENT_PROPS`, `constants.ts:514-532`): `strokeColor "#1e1e1e"`, `backgroundColor "transparent"` (`packages/common/src/colors.ts:194-196`), `fillStyle "solid"`, `strokeWidth 2`, `strokeStyle "solid"`, `roughness 1`, `opacity 100`, `locked false`.

---

## 2. Enumerations

- **fillStyle**: `"hachure" | "cross-hatch" | "solid" | "zigzag"` (`types.ts:19`).
- **strokeStyle**: `"solid" | "dashed" | "dotted"` (`types.ts:28`).
- **roughness**: `architect: 0`, `artist: 1`, `cartoonist: 2` (`constants.ts:466-470`).
- **strokeWidth**:
  - Regular elements: `thin 1`, `medium 2`, `bold 4`, `extraBold 8` (`constants.ts:480-487`).
  - Freedraw uses half those values: `0.5 / 1 / 2 / 4` (`constants.ts:489-501`).
  - The key is picked per element type at `constants.ts:503-510`.
- **roundness.type** (`ROUNDNESS`, `constants.ts:447-464`):
  - `LEGACY = 1`
  - `PROPORTIONAL_RADIUS = 2` (lines and diamonds)
  - `ADAPTIVE_RADIUS = 3` (rectangles)
  - Constants: `DEFAULT_PROPORTIONAL_RADIUS = 0.25` and `DEFAULT_ADAPTIVE_RADIUS = 32` px (`constants.ts:443-445`).
  - Which elements get which type:
    - adaptive: `rectangle`, `embeddable`, `iframe`, `image` (`typeChecks.ts:322-326`);
    - proportional: `line`, `arrow`, `diamond`, `stickynote` (`typeChecks.ts:328-332`);
    - defaults are chosen at `typeChecks.ts:357-373`.
  - Legacy `strokeSharpness` is `"round" | "sharp"` (`types.ts:26`).
- **Arrowheads** (`types.ts:342-367`):
  - Current: `arrow`, `bar`, `circle`, `circle_outline`, `triangle`, `triangle_outline`, `diamond`, `diamond_outline`, `cardinality_one`, `cardinality_many`, `cardinality_one_or_many`, `cardinality_exactly_one`, `cardinality_zero_or_one`, `cardinality_zero_or_many`.
  - Legacy mapping in `normalizeArrowhead` (`packages/element/src/arrowheads.ts:3-21`): `dot → circle`, `crowfoot_one → cardinality_one`, `crowfoot_many → cardinality_many`, `crowfoot_one_or_many → cardinality_one_or_many`.
- **textAlign**: `left | center | right` (`constants.ts:431-435`); default `left` (`constants.ts:274`).
- **verticalAlign**: `top | middle | bottom` (`constants.ts:425-429`); default `top` (`constants.ts:275`).
- **fontFamily ids** (`FONT_FAMILY`, `constants.ts:140-151`):

  | id | font | line height (`packages/common/src/font-metadata.ts:35-134`) | notes |
  |---|---|---|---|
  | 1 | Virgil | 1.25 | deprecated |
  | 2 | Helvetica | 1.15 | deprecated, local system font (`packages/excalidraw/fonts/Helvetica/index.ts`) |
  | 3 | Cascadia | 1.2 | deprecated |
  | 4 | — | — | reserved: historically Assistant / Obsidian custom font (`constants.ts:144`) |
  | 5 | Excalifont | 1.25 | the default (`constants.ts:268`) |
  | 6 | Nunito | 1.25 | |
  | 7 | Lilita One | 1.15 | |
  | 8 | Comic Shanns | 1.25 | |
  | 9 | Liberation Sans | 1.15 | private |
  | 10 | Assistant | 1.25 | private |

  - Fallback ids: Xiaolai `100`, sans-serif `998`, monospace `999`, Segoe UI Emoji `1000` (`constants.ts:158-167`). The fallback chains are built at `constants.ts:169-197`.
  - Font metrics (`unitsPerEm`, `ascender`, `descender`) are listed per font in `font-metadata.ts:35-134`.
  - Fonts are registered by name at `packages/excalidraw/fonts/Fonts.ts:397-411`.
  - Font sizes: `sm 16`, `md 20`, `lg 28`, `xl 36` (`constants.ts:122-127`); default 20 (`constants.ts:223`).
- **theme**: `"light" | "dark"` (`constants.ts:199-202`).
- **Element link**:
  - Stored in base `link: string | null`.
  - Cleaned by `normalizeLink`: trim, escape double quotes, then `@braintree/sanitize-url` (`packages/common/src/url.ts:5-11`; applied on restore at `restore.ts:491`).
  - A link that points to an element is the current URL with `?element=<id>`. The key is `ELEMENT_LINK_KEY = "element"` (`constants.ts:595`); see `packages/element/src/elementLink.ts:13-29` and parsing at `elementLink.ts:82-104`. The id can also be a groupId (`elementLink.ts:31-66`).
- **Image `status`**: `pending | saved | error` (`types.ts:166`).
- **MIME types** (`constants.ts:296-330`):
  - `application/vnd.excalidraw+json`
  - `application/vnd.excalidraw.clipboard+json`
  - `application/vnd.excalidrawlib+json`
  - `application/vnd.excalidrawlib.ids+json`
  - image types: svg, png, jpeg, gif, webp, bmp, x-icon, avif, jfif
- **EXPORT_DATA_TYPES** (`constants.ts:344-349`): `"excalidraw"`, `"excalidraw/clipboard"`, `"excalidrawlib"`, `"excalidraw-api/clipboard"`.
- **Sticky-note constants**: `constants.ts:224-267`. Default background `#ffdf6b` (`colors.ts:268`).

---

## 3. The `.excalidraw` file format

### 3.1 Top-level keys
Written by `serializeAsJSON` (`packages/excalidraw/data/json.ts:52-75`) with `JSON.stringify(data, null, 2)`:
```
{ "type": "excalidraw", "version": 2, "source": <origin>, "elements": [...], "appState": {...}, "files": {...} }
```
- `version` is `VERSIONS.excalidraw = 2`, and the library version is `VERSIONS.excalidrawLibrary = 2` (`packages/common/src/constants.ts:416-419`).
- `source` is `window.EXCALIDRAW_EXPORT_SOURCE || location.origin` (`constants.ts:351-352`).
- The TypeScript shapes are `ExportedDataState` (`packages/excalidraw/data/types.ts:14-21`) and `ImportedDataState` (`data/types.ts:35-50`; every key optional, plus `scrollToContent` and `libraryItems`).
- For `"local"` saves, `files` only includes files referenced by non-deleted image elements (`json.ts:34-50`). For `"database"` saves `files` is `undefined`, so the key disappears from the JSON (`json.ts:67-71`).
- **Deleted elements are not filtered out** of `elements` here.
- Validation (`isValidExcalidrawData`, `json.ts:115-126`) requires `type === "excalidraw"`. `elements` is optional but must be an array if present; `appState` must be an object if present.

### 3.2 Which appState keys are saved
`APP_STATE_STORAGE_CONF` (`packages/excalidraw/appState.ts:153-291`) gives three flags per key: `browser`, `export` and `server`. For `.excalidraw` export (`cleanAppStateForExport`, `appState.ts:321-323`) only five keys have `export: true`:
- `gridSize` (`appState.ts:221`)
- `gridStep` (222)
- `gridModeEnabled` (223)
- `viewBackgroundColor` (273)
- `lockedMultiSelections` (286)

The same five are kept for `server`. `theme`, `name`, `zoom`, `scroll*` and similar are browser-only. Default values are in `getDefaultAppState` (`appState.ts:24-147`), e.g. `viewBackgroundColor "#ffffff"`, `gridSize 20`, `gridStep 5` (`constants.ts:293-294`). The full `AppState` interface is `packages/excalidraw/types.ts:326-587`.

On load, `loadSceneOrLibraryFromBlob` (`packages/excalidraw/data/blob.ts:138-196`) runs:
1. `restoreElements(data.elements, localElements, {repairBindings: true, deleteInvisibleElements: true})` (`blob.ts:161-164`);
2. `restoreAppState({theme, fileHandle, ...cleanAppStateForExport(data.appState), ...scrollToContent}, localAppState)` (`blob.ts:169-179`);
3. `files: data.files || {}` (`blob.ts:180`).

### 3.3 The `files` map (`BinaryFiles`)
`BinaryFiles = Record<FileId, BinaryFileData>` (`packages/excalidraw/types.ts:146`). `BinaryFileData` (`types.ts:118-142`) is:
```ts
{ mimeType: <image mime> | "application/octet-stream"; id: FileId; dataURL: DataURL /* "data:<mime>;base64,..." */;
  created: number /* epoch ms */; lastRetrieved?: number; version?: number }
```
- `FileId` is the SHA-1 hex of the file bytes, falling back to a 40-character nanoid (`blob.ts:259-273`).
- Data URLs are produced by `getDataURL_sync` (`blob.ts:288-296`).
- An image element links to its file through `fileId` (`types.ts:164`).

### 3.4 Restore and migration (`packages/excalidraw/data/restore.ts`)
There is no single `restore()` function at this commit. Callers use `restoreElements` and `restoreAppState` directly.

**Base normalization** (`restoreElementWithProperties`, `restore.ts:430-515`):
- `version = version || 1`
- `versionNonce ?? 0`
- `index ?? null`
- `isDeleted ?? false`
- `id || randomId()`
- `fillStyle || "solid"`
- `strokeWidth || 2`
- `strokeStyle ?? "solid"`
- `roughness ?? 1`
- `opacity == null ? 100 : opacity`
- `angle || 0`
- `x`, `y` `?? 0`
- `strokeColor || "#1e1e1e"`
- `backgroundColor || "transparent"`
- `width`, `height` `|| 0`
- `seed ?? 1`
- `groupIds ?? []`
- `frameId ?? null`
- `updated ?? now`
- `created ?? null`
- `link` normalized, else `null`
- `locked ?? false`

Legacy fields handled here:
- `roundness`: if missing and legacy `strokeSharpness === "round"`, set to `{type: LEGACY (1)}` for adaptive-radius types or `{type: PROPORTIONAL (2)}` otherwise (`restore.ts:475-485`).
- `boundElements`: legacy `boundElementIds` becomes `[{type:"arrow", id}]`, else `boundElements ?? []` (`restore.ts:486-488`).
- `customData` is kept only if present (`restore.ts:495-498`).
- Negative width/height are flipped to positive and `x`/`y` adjusted (`getNormalizedDimensions`, `packages/element/src/sizeHelpers.ts:256-283`).
- Unknown properties are spread back in for forward compatibility (`restore.ts:500-508`).
- `strokeSharpness` and `boundElementIds` are then deleted (`restore.ts:510-512`).

**Per-type handling** (`restoreElement`, `restore.ts:517-752`):

*text* (`restore.ts:531-591`):
- Deletes the Obsidian `rawText` field.
- Legacy `font: "20px Virgil"` is parsed into `fontSize` and `fontFamily` (unknown name → Excalifont; `restore.ts:289-296`).
- A non-finite `fontSize` becomes 20.
- `lineHeight = lineHeight || (height ? detectLineHeight(el) : getLineHeight(el.fontFamily))`. The fallback uses `element.fontFamily`, not a family parsed from the legacy `font` string.
- `textAlign || "left"`, `verticalAlign || "top"`, `containerId ?? null`, `originalText || text`, `autoResize ?? true`.
- `labelPosition` is clamped to 0..1, else `null`.
- `baseFontSize` is clamped to 1..512 (`packages/element/src/stickyNote.ts:379-384`).
- Empty text is marked deleted when `deleteInvisibleElements` is set.

*freedraw* (`restore.ts:592-604`):
- Invalid points are dropped and `pressures` kept aligned with them; a non-finite pressure becomes 0.5 (`restore.ts:184-218`).
- `strokeOptions` defaults to `{variability:"variable", streamline:0.5}` (`restore.ts:273-287`; `constants.ts:622`).
- `simulatePressure` is passed through with no default.

*image* (`restore.ts:605-611`): `status || "pending"`, `scale || [1,1]`, `crop ?? null`.

*line / draw* (`restore.ts:612-655`):
- Arrowheads are normalized.
- Fewer than 2 valid points are replaced by `[[0,0],[width,height]]` (`restore.ts:159-182`).
- Points are re-based so `points[0] == [0,0]`, adjusting `x`/`y` (`restore.ts:626-634`).
- Bindings are forced to `null`.
- `polygon` stays only if the shape is a valid polygon (more than 3 points and first == last; `packages/element/src/typeChecks.ts:397-401`).
- `width`/`height` are recomputed from the points (`packages/common/src/points.ts:10-19`).
- Elements over 75,000 px are marked deleted (`restore.ts:126-157`).

*arrow* (`restore.ts:656-723`):
- A missing `endArrowhead` defaults to `"arrow"`.
- Bindings go through `repairBinding` (`restore.ts:298-428`):
  - elbow arrows: `fixedPoint` normalized and `mode ?? "orbit"`;
  - bindings with `mode`: kept (`elementId` required);
  - legacy bindings without `mode`: `mode` becomes `inside` or `orbit` depending on whether the endpoint lies inside the target, and `fixedPoint` is recomputed.
- Elbow arrows keep `fixedSegments` only when non-empty and the arrow has at least 4 points, otherwise `null`.
- Points are re-based as for lines.

*generic types* (`restore.ts:726-731`): no extra handling.

*stickynote* (`restore.ts:732-740`): `baseHeight ?? maxHeight (legacy) ?? height`, then `normalizeStickyNote` (`newElement.ts:224-228`).

*frame / magicframe* (`restore.ts:741-745`): `name ?? null`.

**Scene-level passes** (`restoreElements`, `restore.ts:946-1138`):
1. Invisibly small elements are marked deleted (`packages/element/src/sizeHelpers.ts:61-78`).
2. Duplicate ids get a fresh id (`restore.ts:1000-1003`).
3. `syncInvalidIndices` assigns or repairs `index` (`restore.ts:965`).
4. When `repairBindings` is set:
   - `repairFrameMembership` (`restore.ts:876-887`);
   - `repairBoundElement` (`restore.ts:809-841`): a text's angle is set to its container's angle (0 for arrows);
   - `repairContainerElement` (`restore.ts:761-801`);
   - linear bindings are dropped if the target is missing or the element is not an arrow (`restore.ts:1048-1063`);
   - `restoreStickyNotes` (`restore.ts:898-944`);
   - bound text is moved right after its container (`restore.ts:849-869`);
   - broken or self-bound elbow arrows are fixed up (`restore.ts:1072-1137`).

**AppState restore** (`restoreAppState`, `restore.ts:1254-1372`):
- Each key comes from the imported file, then local state, then the default (`restore.ts:1277-1292`).
- Legacy `isSidebarDocked` becomes `defaultSidebarDockedPreference` (`restore.ts:1187-1204`; `data/types.ts:30-33`).
- Numeric `zoom` becomes `{value}` (`restore.ts:1346-1352`).
- A string `openSidebar` becomes `{name:"default"}` (`restore.ts:1353-1357`).
- Legacy `currentItemStrokeWidth` becomes `currentItemStrokeWidthKey` (`restore.ts:1320-1325`).
- `gridSize` / `gridStep` are normalized (`restore.ts:1358-1363`).
- The active tool is limited to `AllowedExcalidrawActiveTools` (`restore.ts:220-244`, `1334-1344`).
- `colorTopPicks` and `fontTopPicks` are sanitized (`restore.ts:1206-1252`).

**Versioning helpers:**
- `bumpVersion`: `version + 1`, new random `versionNonce`, `updated = now` (`packages/element/src/mutateElement.ts:188-196`).
- `bumpElementVersions` (`restore.ts:1150-1173`).
- Collaboration conflict rule (`packages/excalidraw/data/reconcile.ts:23-44`): the higher `version` wins; on a tie, the lower `versionNonce` wins.
- `hashElementsVersion` is djb2 over `versionNonce` values (`packages/element/src/index.ts:21-28`).

---

## 4. The `.excalidrawlib` library format

**Envelope** (`serializeLibraryAsJSON`, `packages/excalidraw/data/json.ts:137-145`):
```
{ "type": "excalidrawlib", "version": 2, "source": <origin>, "libraryItems": LibraryItem[] }
```
- TypeScript shape: `packages/excalidraw/data/types.ts:52-57`.
- `isValidLibrary` accepts `version === 1 || version === 2` (`json.ts:128-135`).
- Saved with the `.excalidrawlib` extension and MIME `application/vnd.excalidrawlib+json` (`json.ts:147-159`).

**Version 1:**
- Top-level key `library` (`ImportedLibraryData.library`, `data/types.ts:59-62`).
- Its value is an array of items, and each item is a bare array of elements (`LibraryItem_v1`, `packages/excalidraw/types.ts:647-649`).
- Example fixture: `packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib`. It uses `"version": 1` and `"library": [[{...rectangle with strokeSharpness, boundElementIds}]]`.

**Version 2:**
- Top-level key `libraryItems`. Each `LibraryItem` (`packages/excalidraw/types.ts:652-660`) is:
  ```ts
  { id: string; status: "published" | "unpublished"; elements: NonDeleted<ExcalidrawElement>[];
    created: number /* epoch ms */; name?: string; error?: string }
  ```

**Parsing** (`parseLibraryJSON`, `packages/excalidraw/data/blob.ts:218-228`):
- Reads `data.libraryItems || data.library`, then calls `restoreLibraryItems` (`packages/excalidraw/data/restore.ts:1381-1415`).
- A v1 array item gets a random `id`, the default `status` and `created = now`.
- For v2 items, missing `id`, `status` or `created` are filled the same way.
- Each item's elements go through `restoreElements(…, null)`. Deleted elements are removed, and items left empty are dropped (`restore.ts:1374-1379`).

**Merging:**
- Two items are the same when their elements match by `id` and `versionNonce`, in order (`packages/excalidraw/data/library.ts:122-141`).
- `mergeLibraryItems` prepends new unique items (`library.ts:145-157`).
- The item hash is `id:name:hashElementsVersion` (`library.ts:594-596`).
- The persistence adapter stores `{ libraryItems }` (`library.ts:70`, `library.ts:78-95`).

**Import from a URL (`#addLibrary`):**
- `parseLibraryTokensFromUrl` reads `addLibrary` from the URL hash, or from the query string (legacy), plus `token` from the hash (`library.ts:530-543`). The keys are defined at `packages/common/src/constants.ts:376-382`.
- `importLibraryFromURL` (`library.ts:718-779`):
  1. `decodeURIComponent`, then `toValidURL` (`packages/common/src/url.ts:21-37`);
  2. `validateLibraryUrl` against the allow-list `["excalidraw.com", "raw.githubusercontent.com/excalidraw/excalidraw-libraries"]` (`library.ts:54-58`, `497-528`). The hostname is matched as a suffix on subdomain boundaries, the path as a prefix;
  3. `fetch` the URL to a Blob;
  4. `updateLibrary({merge: true, defaultStatus: "published", prompt: token !== api.id})` (`library.ts:287-349`).
- `hashchange` events trigger the same flow (`library.ts:780-793`).

**libraries.excalidraw.com:**
- Configured by `VITE_APP_LIBRARY_URL=https://libraries.excalidraw.com` and `VITE_APP_LIBRARY_BACKEND=https://us-central1-excalidraw-room-persistence.cloudfunctions.net/libraries` (`.env.production:6-7`).
- The Browse button links to `${LIBRARY_URL}?target=<window.name|_blank>&referrer=<returnUrl>&useHash=true&token=<api.id>&theme=<theme>&version=2` (`packages/excalidraw/components/LibraryMenuBrowseButton.tsx:16-25`). The site then calls back with `#addLibrary=<url>&token=…`.
- Publishing POSTs multipart form data to `${LIBRARY_BACKEND}/submit` (`packages/excalidraw/components/PublishLibrary.tsx:280-302`). Fields: `excalidrawLib` (a v2 JSON blob), `previewImage`, `previewImageType`, `title`, `authorName`, `githubHandle`, `name`, `description`, `twitterHandle`, `website`.

**Drag and drop:** dragging items sets MIME `application/vnd.excalidrawlib.ids+json` with `{itemIds: string[]}` (`packages/excalidraw/components/LibraryMenuItems.tsx:215-228`; type at `data/types.ts:64-66`). The legacy form is full library JSON under `application/vnd.excalidrawlib+json` (`packages/excalidraw/components/App.tsx:13192-13210`).

---

## 5. Clipboard and embedding formats

**Text encoding wrapper** (`packages/excalidraw/data/encode.ts`):
- `EncodedData = { version?: "1"; encoding: "bstring"; compressed: boolean; encoded: string }` (`encode.ts:87-94`).
- `encode` (`encode.ts:99-121`):
  - with compression: `encoded` = pako `deflate` (zlib) output as a byte string, one char per byte;
  - otherwise: the UTF-8 bytes of the text as a byte string.
  - Returns `{version:"1", encoding:"bstring", compressed, encoded}`.
- `decode` (`encode.ts:123-144`) reverses this with `inflate`.
- Byte string helpers: `encode.ts:14-39`.

**PNG embedding** (`packages/excalidraw/data/image.ts`):
- `encodePngMetadata` (`image.ts:25-47`) inserts a `tEXt` chunk just before `IEND`.
  - keyword: `application/vnd.excalidraw+json`;
  - text: `JSON.stringify(encode({text: sceneJSON, compress: true}))`.
- Decoding (`image.ts:49-71`) uses the first `tEXt` chunk. It accepts:
  - the encoded wrapper (has an `encoded` key), or
  - legacy v1: raw scene JSON with `type: "excalidraw"`.
- The payload is `serializeAsJSON(..., "local")`, embedded when `exportEmbedScene` is on (`packages/excalidraw/data/index.ts:176-190`; `packages/utils/src/export.ts:144-159`). Files are saved as `.excalidraw.png` (`data/index.ts:190`).

**SVG embedding** (`packages/excalidraw/scene/export.ts`):
- The root `<svg>` starts with `<!-- svg-source:excalidraw -->` followed by a `<metadata>` element (`export.ts:368-370`).
- `encodeSvgBase64Payload` (`export.ts:510-529`) writes into `<metadata>`:
  ```
  <!-- payload-type:application/vnd.excalidraw+json --><!-- payload-version:2 --><!-- payload-start -->BASE64<!-- payload-end -->
  ```
  - `BASE64 = btoa(JSON.stringify(encode({text: sceneJSON})))`. Compression is on by default, and the JSON is already a byte string, so there is no UTF-8 re-encoding.
- Decoding (`export.ts:531-563`) uses the regex `/<!-- payload-start -->\s*(.+?)\s*<!-- payload-end -->/`.
  - A missing `payload-version` means v1: the base64 decodes to UTF-8 text rather than a byte string.
  - It then accepts either the encoded wrapper or raw scene JSON.
- Files are saved as `.excalidraw.svg` (`data/index.ts:150`).
- In this repository (ex-112): `excali_core::svg_payload::{encode_svg_base64_payload, decode_svg_base64_payload}`, with `btoa` / `atob` in `excali_core::encode`. `crates/excali-core/tests/svg_payload.rs` decodes both SVG fixtures and reproduces the payload in upstream's two export snapshots byte for byte.
- Detection on load: `packages/excalidraw/data/blob.ts:32-80`. Extension-to-MIME mapping: `blob.ts:82-104`, `520-543`.

**Clipboard** (`packages/excalidraw/clipboard.ts`):
- Copy writes `JSON.stringify({type: "excalidraw/clipboard", elements, files})` (`clipboard.ts:39-43`, `143-193`). `files` holds only the files of the copied images, or is `undefined`.
- Elements whose frame is not also being copied have `frameId` cleared.
- The JSON is written under both `application/vnd.excalidraw.clipboard+json` and `text/plain` (`clipboard.ts:195-210`).
- Paste accepts `type` of `excalidraw`, `excalidraw/clipboard` or `excalidraw-api/clipboard` with an `elements` array (`clipboard.ts:74-88`, `523-555`).
- PNG images are copied via a `ClipboardItem` (`clipboard.ts:557-585`).

**Share links** (excalidraw.com app, for reference):
- `compressData` (`encode.ts:309-354`) builds `concatBuffers(encodingMetadataJSON, iv, AES-GCM(deflate(concatBuffers(metadataJSON, data))))`.
  - `concatBuffers` layout: `[u32 version=1][u32 len][bytes]…`, big-endian via `DataView` (`encode.ts:161-291`).
  - Encoding metadata: `FileEncodingInfo {version: 2, compression: "pako@1", encryption: "AES-GCM"}` (`encode.ts:150-158`).
  - AES-128-GCM with a 12-byte IV; the key is the JWK `k` string (`packages/excalidraw/data/encryption.ts:5-47`).
- The URL hash is `#json=<id>,<key>` (`excalidraw-app/data/index.ts:249-285`); the collaboration link is `#room=<id>,<key>` (`excalidraw-app/data/index.ts:131`).

---

## 6. Fractional indexing (`index`)

- The vendored rocicorp algorithm is in `packages/fractional-indexing/src/index.ts`.
  - Digits: `BASE_62_DIGITS = "0-9A-Za-z"` (`index.ts:5-6`).
  - Integer-part length comes from the head character: `a`–`z` gives 2..27, `A`–`Z` gives 27..2 (`index.ts:80-88`).
  - `validateOrderKey` rejects bad characters, a trailing `"0"`, and `"A" + "0"*26` (`index.ts:109-125`).
  - `generateKeyBetween(null, null) = "a0"` (`index.ts:212-268`); `generateNKeysBetween` is at `index.ts:284-322`.
  - Keys compare as plain strings (JS `<` on code units).
- `packages/element/src/fractionalIndex.ts`:
  - Design notes (`fractionalIndex.ts:27-42`): the array order is the source of truth for rendering, and indices must stay in sync with it.
  - `isValidFractionalIndex` (`fractionalIndex.ts:396-428`): the key must be well-formed and strictly between its neighbours.
  - `syncInvalidIndices` (`fractionalIndex.ts:223-235`) finds runs of invalid indices (`getInvalidIndicesGroups`, `fractionalIndex.ts:296-394`) and fills them with `generateNKeysBetween(lower, upper, n)` (`fractionalIndex.ts:430-459`).
  - `syncMovedIndices` re-indexes only the moved elements and falls back to a full sync (`fractionalIndex.ts:175-216`).
  - `orderByFractionalIndex` sorts by index and breaks ties by `id` (`fractionalIndex.ts:150-169`). It is used after reconciliation (`packages/excalidraw/data/reconcile.ts:109-115`).
  - A bound text's index must be greater than its container's; this is checked but not auto-fixed (`fractionalIndex.ts:87-111`).
- Restore always calls `syncInvalidIndices`, so files without indices get fresh ones (`packages/excalidraw/data/restore.ts:965`).

---

## 7. Binding and containment model

- **Arrow to shape (two-way):**
  - The arrow stores `startBinding` / `endBinding: {elementId, fixedPoint:[rx, ry], mode}` (`packages/element/src/types.ts:320-333`).
  - The target stores `boundElements: [{id: arrowId, type: "arrow"}]` (`packages/element/src/binding.ts:1120-1138`).
  - Unbinding removes the back-reference unless the arrow's other end is bound to the same element (`binding.ts:1187-1215`).
  - `fixedPoint` is a ratio of the target's width and height. The global point is `(x + w*rx, y + h*ry)` rotated about the element centre by `angle` (`binding.ts:2670-2685`).
  - `normalizeFixedPoint` (`binding.ts:~2739-2778`):
    - non-finite or missing values become `[0.5001, 0.5001]`;
    - each ratio is clamped to [-10, 10];
    - an exact 0.5 is nudged to 0.5001.
  - Binding gap = `5 + strokeWidth/2` (`binding.ts:117-131`).
  - Elbow arrows always bind with `mode: "orbit"` (`binding.ts:1141-1185`).
  - Only arrows bind; lines have their bindings cleared on restore (`packages/excalidraw/data/restore.ts:636-639`, `1048-1063`).
- **Text in a container (two-way):**
  - The text stores `containerId`; the container's `boundElements` includes `{id, type: "text"}`.
  - At most one text per container is used (`getBoundTextElementId`, `packages/element/src/textElement.ts:326-330`).
  - Valid containers are rectangle, stickynote, diamond, ellipse and arrow (`textElement.ts:508-519`).
  - Restore repairs both sides (`restore.ts:761-841`), and bound text is kept immediately after its container (`packages/element/src/sortElements.ts:57-110`).
  - An arrow label's stored x/y can be stale. The real position comes from `labelPosition`, default 0.5 (`textElement.ts:452-473`, `617`).
  - Text layout inside a container: `textElement.ts:249-324`, `396-417`, `540-599`.
- **Frames:**
  - Children point to their frame via `frameId` (`packages/element/src/frame.ts:242-253`, `436-444`). The frame does not list its children.
  - Allowed child types: `packages/element/src/typeChecks.ts:413-430`.
  - A `frameId` pointing to a missing frame is cleared on restore (`restore.ts:876-887`).
- **Groups:** `groupIds` is ordered deepest to shallowest (`types.ts:71-73`); there is no separate group entity. The app state's `lockedMultiSelections` is keyed by groupId (`packages/excalidraw/types.ts:561-565`).
- **Elbow arrows:**
  - `elbowed: true`.
  - `fixedSegments[]` holds user-pinned segments by point index (`types.ts:385-389`).
  - `startIsSpecial` / `endIsSpecial` hide a first or last segment (`types.ts:404-419`).
  - Every segment must be axis-aligned (`validateElbowPoints`, `packages/element/src/elbowArrow.ts:2293-2304`).
  - Points are re-routed with A* (`updateElbowArrowPoints`, `elbowArrow.ts:907`).

---

## 8. Text measurement and cross-implementation fidelity

- **Height** = `fontSize * lineHeight * lineCount` (`packages/element/src/textMeasurements.ts:91-96`, `170-177`).
  - Line count comes from `\n` after normalizing EOLs and replacing each tab with 8 spaces (`textMeasurements.ts:64-74`; `packages/common/src/utils.ts:1038-1040`).
  - `measureText` counts empty lines as `" "` (`textMeasurements.ts:12-27`).
- **Width** = the widest line's canvas advance width (`context.measureText(line).width`) in the CSS font string `"<size>px <Family>, <fallbacks>"` (`textMeasurements.ts:121-168`; `packages/common/src/utils.ts:123-147`).
  - It can be replaced with `setCustomTextMetricsProvider` (`textMeasurements.ts:113-119`).
  - In tests, width is char count × 10 (`textMeasurements.ts:144-146`).
- **`lineHeight`** is stored per element and unitless. Defaults per font are in `packages/common/src/font-metadata.ts:175-181`. Old files without it get `height / lines / fontSize` (`textMeasurements.ts:80-85`).
- **Baseline for rendering**: `getVerticalOffset` uses the font's ascender, descender and unitsPerEm (`font-metadata.ts:155-170`).
- **`autoResize`**:
  - `true`: width follows the text.
  - `false`: text wraps at the stored `width` (`packages/element/src/newElement.ts:533-581`).
  - Wrapping is `wrapText` (`packages/element/src/textWrapping.ts:397-406`).
  - `text` holds the wrapped output and `originalText` the source text.
  - Bound text wraps at `getBoundTextMaxWidth` (`packages/element/src/textElement.ts:540-569`). That is container width − 2×5 px padding (`BOUND_TEXT_PADDING = 5`, `packages/common/src/constants.ts:421`), with special cases for ellipse, diamond, arrow (0.7×width, min 11×fontSize; `constants.ts:422-423`) and sticky note (16 px padding).
- **Why this matters for a Rust port:**
  - `width`, `height` and `text` are all saved values that came from browser font shaping.
  - A Rust renderer should trust the stored values rather than re-measure.
  - When it does re-measure (restore with `refreshDimensions`, editing, sticky auto-fit at `packages/excalidraw/data/restore.ts:1032-1046` and `931-943`), it needs the same fonts (woff2 files under `packages/excalidraw/fonts/`), the same advance-width metrics and the same wrap rules. Otherwise line breaks and container sizes will differ.
  - Positioning grows the box from the anchor set by `textAlign` / `verticalAlign` (`newElement.ts:303-333`, `393-484`).

---

## 9. Fixtures to reuse for conformance tests

- `packages/excalidraw/tests/fixtures/fixture_library.excalidrawlib` — a v1 library with the legacy `strokeSharpness` and `boundElementIds` fields.
- Embedded-scene decoding (used by `packages/excalidraw/tests/export.test.tsx:96-150`):
  - `packages/excalidraw/tests/fixtures/test_embedded_v1.png` — legacy raw-JSON tEXt chunk; expects one text element "test".
  - `packages/excalidraw/tests/fixtures/smiley_embedded_v2.png` — expects a text element "😀".
  - `packages/excalidraw/tests/fixtures/test_embedded_v1.svg` — no `payload-version`, so UTF-8 base64.
  - `packages/excalidraw/tests/fixtures/smiley_embedded_v2.svg` — `payload-version:2`, compressed byte string.
- `packages/excalidraw/tests/fixtures/svg-image-exporting-reference.svg` — reference SVG export.
- `packages/excalidraw/tests/fixtures/smiley.png`, `packages/excalidraw/tests/fixtures/deer.png` — plain images (no scene).
- `packages/excalidraw/tests/fixtures/elementFixture.ts` — complete base and text element objects with every current field.
- `packages/excalidraw/tests/fixtures/diagramFixture.ts` — a full `.excalidraw` document shape.
- `packages/excalidraw/tests/data/restore.test.ts` and `packages/excalidraw/tests/data/__snapshots__/restore.test.ts.snap` — restore and migration cases: legacy `draw`, crowfoot arrowheads, zoom-as-number, `openSidebar` string, `created`, sticky notes, freedraw pressures.
- `packages/element/tests/fractionalIndex.test.ts` — valid and invalid order keys and sync cases.
- `packages/excalidraw/tests/data/reconcile.test.ts` — version / versionNonce / index reconciliation.
- `packages/excalidraw/tests/clipboard.test.tsx`, `packages/excalidraw/clipboard.test.ts` — clipboard parsing.
- `packages/excalidraw/tests/library.test.tsx`, `packages/excalidraw/data/library.test.ts` — library import and merge, and URL validation.
- `packages/excalidraw/tests/scene/__snapshots__/export.test.ts.snap`, `packages/excalidraw/tests/__snapshots__/export.test.tsx.snap` — SVG output with embedded payload.
- `packages/element/tests/textWrapping.test.ts`, `packages/element/tests/textElement.test.ts` — wrapping and measurement. These use the test-env metric of 10 px per character.

**In this repository (ex-003).** Every file in `packages/excalidraw/tests/fixtures/`, the restore and reconcile tests, the restore snapshot, `packages/excalidraw/tests/export.test.tsx` with the two export snapshots that hold an embedded SVG payload (added by ex-112), and the upstream `LICENSE` are copied byte for byte into `fixtures/upstream/<upstream path>`. The public library catalogue (`libraries.json` and all 232 `.excalidrawlib` files from `excalidraw/excalidraw-libraries` at `extra.libraries_commit` in `site/config.toml`, 71 catalogue version 1 and 161 version 2 on 2026-09-28) lives in `fixtures/libraries/`, each library stored as `<source>.gz`. `fixtures/manifest.json` records every file's sha256, size and origin URL at the pinned commit (plus `content_sha256` of the original bytes for gzip-stored libraries). `scripts/fixtures/corpus.py check` asserts the manifest matches disk; `verify-upstream` and `verify-remote` re-derive every file from its origin; the `fixtures` CI job runs all three. The other test sources above are read from the pinned checkout in `.tools/upstream`.

The repo has **no JSON Schema** for either format. `find` found no `*schema*.json` and no `.excalidraw` scene files, so the TypeScript types above are the only specification.
