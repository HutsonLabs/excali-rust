+++
title = "File format specification"
description = "The .excalidraw and .excalidrawlib formats as the port must read and write them, condensed from the data-model research."
weight = 2
+++

This page is the normative summary. The [data-model research](../../research/data-model/) has the line-by-line evidence; where they disagree, the research page and the upstream source win.

## `.excalidraw`

```json
{
  "type": "excalidraw",
  "version": 2,
  "source": "https://excalidraw.com",
  "elements": [ ... ],
  "appState": { "gridSize": 20, "gridStep": 5, "gridModeEnabled": false,
                "viewBackgroundColor": "#ffffff", "lockedMultiSelections": {} },
  "files": { "<fileId>": { "mimeType": "image/png", "id": "<fileId>",
                           "dataURL": "data:image/png;base64,...", "created": 1700000000000 } }
}
```

- Written with two-space indentation (`JSON.stringify(data, null, 2)`, `data/json.ts:52-75`). The port preserves key order: `type, version, source, elements, appState, files`.
- `version` is `2`; `source` is the writer's origin. The port writes `source: "https://github.com/HutsonLabs/excali-rust"` unless the host overrides it.
- `elements` keeps deleted elements (`isDeleted: true`). Readers must not draw them; writers keep them, as upstream does for undo and reconciliation.
- Only five `appState` keys are exported; everything else is dropped on save and defaulted on load.
- `files` holds only files referenced by non-deleted image elements when saving locally.
- Validation: `type === "excalidraw"`; `elements`, if present, must be an array; `appState`, if present, an object.

### Element base fields

Every element has: `id, x, y, strokeColor, backgroundColor, fillStyle, strokeWidth, strokeStyle, roundness, roughness, opacity, width, height, angle, seed, version, versionNonce, index, isDeleted, groupIds, frameId, boundElements, updated, created, link, locked` and optional `customData`. Per-type fields, enumerations and defaults are in the research page, sections 1 to 2.

### Rust model

```rust
pub struct Element {
    pub base: ElementBase,          // the 26 shared fields
    pub kind: ElementKind,          // Rectangle, Diamond, Ellipse, Text(TextFields), Linear(LinearFields, LinearKind), Freedraw(...), Image(...), Frame(...), MagicFrame(...), Embeddable, Iframe(...), StickyNote(...)
    pub extra: serde_json::Map<String, Value>,  // unknown keys, written back verbatim
}
```

`ElementKind` is chosen by the JSON `type` string. Unknown `type` values are dropped on restore, as upstream does; `selection` is dropped; legacy `draw` becomes `line`.

Key order and unknown keys follow upstream's object semantics (`excali_core::document::Document`, `excali_core::element::Element`):

- A file read and written back without edits gives what `JSON.stringify(JSON.parse(text), null, 2)` gives. Unknown keys stay where they were, at the top level, in elements, in `appState` and in `files`. Upstream keeps them too: restore spreads the original element first (`restore.ts:500-508`).
- A key added by an edit goes at the end of its object, the way a JS property assignment in `mutateElement` (`mutateElement.ts:80-100`) adds it.
- Every object is written in JS property order, which `JSON.stringify` follows for any object: array-index keys (`"0"` to `"4294967294"`, such as a file id `"42"` or an unknown key `"7"`) first in ascending order, then the other keys by the rules above. This holds for documents built by the port, for edited ones, and at every level (top level, `appState`, `files`, elements, `customData`, unknown keys).
- A known value the typed model reads in a normalised form (`customData: null`, an unknown key inside `boundElements`) is written back as it was read until the field changes.
- An element built by the port has its keys in the order of upstream's constructors: `id`, `type`, the base fields, `customData`, then the per-type fields (`newElement.ts:87-692`). A document built by the port uses `serializeAsJSON`'s order.
- A lone UTF-16 surrogate in a string (`"\ud83d"`, half of a split emoji), which `JSON.parse` keeps and `JSON.stringify` writes back as its escape, reads as U+FFFD in the typed model and is written back as the escape until that value changes. In `appState` and `files` this holds per key: changing `appState.b` leaves an escape in an untouched `appState.a` as read, as upstream's in-place edit does; a changed key is written from the model as a whole.
- A file is accepted exactly when `isValidExcalidrawData` (`packages/excalidraw/data/json.ts:115-126`) accepts it: `type` is `"excalidraw"`, and `elements` is falsy, or an array with `appState` falsy or `typeof "object"` (so `appState: []` passes). `version`, `source` and `files` are not checked. A value the model has no type for (falsy `elements`, `appState: []`, `version: "2"`) is written back as read. The typed codec is stricter in one way: every element must be an object of a known type with its required fields; anything else is restore's job.
- The fixtures of these rules are generated from upstream's own constructors, `mutateElement` and `serializeAsJSON` by `tools/goldens/scene-fixtures.mjs`; CI runs it with `--check`.

### Restore rules (must-implement)

| Rule | Upstream |
|---|---|
| Defaults for missing base fields (`version 1`, `versionNonce 0`, `fillStyle solid`, `strokeWidth 2`, `roughness 1`, `opacity 100`, `strokeColor #1e1e1e`, `backgroundColor transparent`, `seed 1`, …) | `restore.ts:430-515` |
| `strokeSharpness: "round"` → `roundness {type: 1}` for rectangle-like, `{type: 2}` otherwise; then delete `strokeSharpness` | `restore.ts:475-485, 510-512` |
| `boundElementIds` → `boundElements: [{type:"arrow", id}]` | `restore.ts:486-488` |
| Negative width/height normalised, x/y adjusted | `sizeHelpers.ts:256-283` |
| Text: legacy `font` string parsed; `lineHeight` detected from height or defaulted per family; `autoResize` default true; `baseFontSize` clamped 1..512; `labelPosition` clamped 0..1 | `restore.ts:531-591` |
| Freedraw: invalid points dropped, pressures aligned, non-finite pressure → 0.5; `strokeOptions` default `{variability:"variable", streamline:0.5}` | `restore.ts:592-604` |
| Image: `status` default `pending`, `scale [1,1]`, `crop null` | `restore.ts:605-611` |
| Line/arrow: arrowheads normalised (`dot→circle`, `crowfoot_*→cardinality_*`); fewer than 2 points → `[[0,0],[w,h]]`; points re-based so `points[0] == [0,0]`; lines get `null` bindings; elements over 75,000 px marked deleted; arrow `endArrowhead` default `arrow`; bindings repaired with `mode` | `restore.ts:612-723` |
| Sticky note: `baseHeight ?? maxHeight ?? height`; transparent colours replaced (tinycolor alpha 0), fill solid, at least 75×75, `height >= baseHeight` | `restore.ts:732-740`, `newElement.ts:185-228` |
| Frame: `name ?? null` | `restore.ts:741-745` |
| Scene: invisibly small elements deleted; duplicate ids regenerated; `syncInvalidIndices`; frame membership, container/bound-text pairs and linear bindings repaired; sticky note labels given a font ceiling and one stroke colour with their note; bound text ordered right after its container; broken elbow arrows fixed up | `restore.ts:946-1138` |
| Versions bumped past local copies (same id, higher version, or same version and another `versionNonce`) | `restore.ts:1150-1173` |

`excali-core` implements the per-element rules as `restore::restore_element`. One step needs element geometry: an arrow binding saved before bindings had a `mode` whose target exists is migrated by testing whether the arrow's end lies inside the target and computing a `fixedPoint` against it (`restore.ts:362-418`). That uses shapes, bounds and hit testing (`binding.ts`, `collision.ts`, `linearElementEditor.ts`), which the [crate table](../overview/) places in `excali-editor` above `excali-core`, so restore asks its `RestoreEnv` (`migrate_legacy_binding`). Without an answer the binding is dropped, which is what upstream does only when that computation fails.

**Known gap (tracked as ex-116).** No environment answers yet: the default `RestoreEnv` has no geometry, so a legacy arrow binding whose target exists is dropped on load, where upstream keeps it with a computed `mode` and `fixedPoint`. That loses data from real legacy files. ex-116 implements the migration in `excali-editor` once hit testing (ex-507) and binding geometry (ex-510) exist, and milestone M1 depends on it, so M1 cannot close while this gap remains. It is also listed under tracked exceptions in [ADR-008](../../decisions/adr-008-crate-boundaries/).

The scene-level passes are `restore::restore_elements` (`restoreElements`), with the options `repair_bindings`, `delete_invisible_elements` and `refresh_dimensions`, and `restore::bump_element_versions` (`bumpElementVersions`). File loading and the initial scene use `repair_bindings` and `delete_invisible_elements` (`data/blob.ts:161-164`, `components/App.tsx:3653-3656`). The passes run in upstream's order on the elements as upstream mutates them, and they follow its JS semantics, including the inputs where upstream throws out of the whole call. The output is checked element by element and in order against `tests/fixtures/restore-elements.json`, which `tools/goldens/restore-elements-fixtures.mjs` generates from upstream's own `restoreElements` on 117 scenes. Three steps need code that the crate table puts above `excali-core`, so restore asks its `RestoreEnv` for them:

- `update_elbow_arrow_points`: the elbow arrow router (`updateElbowArrowPoints`, `excali-editor`, ex-211), for an unbound elbow arrow with a segment that is not axis-aligned;
- `refresh_text_dimensions`: text wrapping and measurement (`refreshTextDimensions`, `excali-text`, ex-304), only with `refresh_dimensions`;
- `sticky_note_layout`: the sticky note label fit (`getStickyNoteLayout`, ex-703), only with `refresh_dimensions`.

The fixture records what upstream passed to each of these and what it returned. The port's test checks that restore makes the same calls, and it answers them with the recorded results. The default environment answers `None` to all three. Without an answer the elbow arrow keeps its restored points and the text keeps its size. Upstream's own callers never pass `refresh_dimensions`. Each gap is listed under tracked exceptions in [ADR-008](../../decisions/adr-008-crate-boundaries/) until its task implements the answer.

### `appState` on save and load

`excali_core::app_state` ports `packages/excalidraw/appState.ts` and `restoreAppState` (`data/restore.ts:1175-1372`).

- **Save.** `clean_app_state_for_export` keeps the keys flagged `export` in `APP_STATE_STORAGE_CONF` (`appState.ts:153-291`), in the order they were given: `gridSize`, `gridStep`, `gridModeEnabled`, `viewBackgroundColor` and `lockedMultiSelections`. The server keeps the same five. Browser storage keeps 60 keys; unknown keys are dropped everywhere.
- **Defaults.** `get_default_app_state` is `getDefaultAppState()` in upstream's key order. Two values depend on the environment: `exportScale` is the device pixel ratio when it is 1, 2 or 3 and 1 otherwise, and `currentItemRoundness` is `"sharp"` only in upstream's test build.
- **Load.** `restore_app_state(file, local)` takes each default key from the file, then from the local state, then from the default. `null` counts as a value, and unknown keys are dropped. Then the legacy and sanitising rules apply:
  - legacy `isSidebarDocked` puts `defaultSidebarDockedPreference` first;
  - a numeric `zoom` becomes `{value}`, clamped to 0.1..30 and rounded to six places;
  - a string `openSidebar` becomes `{name: "default"}`;
  - legacy `currentItemStrokeWidth` becomes `currentItemStrokeWidthKey`;
  - `gridSize` and `gridStep` are rounded and clamped to 1..100;
  - `activeTool` is limited to `AllowedExcalidrawActiveTools`;
  - `colorTopPicks` and `fontTopPicks` are deduped and capped;
  - transparent sticky-note colours are reset;
  - `cursorButton` and `penDetected` come from the local state.
- **Malformed values.** Upstream does not check the types of imported values, so the port reproduces what it computes for them. `zoom: {value: "5"}` concatenates with `Number.EPSILON` and clamps to 0.1. An object with its own `toString` key throws `TypeError: Cannot convert object to primitive value`, and `activeTool: null` throws the `Cannot read properties of null` error; `restore_app_state` returns those errors.
- **Colours.** Deduping top picks and detecting transparent colours need `colorToHex` and `isTransparent`, both on tinycolor2 1.6.0. `excali_core::color` ports that parser: named colours; `#rgb` to `#rrggbbaa` with or without `#`; `rgb[a]`, `hsl[a]` and `hsv[a]` with optional parentheses and commas, matched anywhere in the string.
- **Goldens.** `crates/excali-core/tests/fixtures/app-state.json` is upstream's own output, written by `tools/goldens/app-state.mjs` from the pinned checkout and re-checked in CI. It holds the defaults per environment, the kept keys per storage target, the colour cases and more than 250 `restoreAppState` cases. Those cover upstream's tests and every branch above, including the thrown errors.

### Ordering

`index` is a fractional index over base-62 digits `0-9A-Za-z`, compared as plain strings. The array order is the source of truth for rendering; indices are kept in sync with it, and files without indices get fresh ones on load. The port ports the vendored implementation (`packages/fractional-indexing/src/index.ts`, 322 lines) rather than depending on a crate with different key strings.

### Versions and conflicts

`version` increments on every change; `versionNonce` is a fresh random integer on every change; `updated` is epoch milliseconds. Reconciliation rule: higher `version` wins, tie broken by lower `versionNonce` (`reconcile.ts:23-44`). The port keeps these semantics so files edited alternately by the port and by Excalidraw merge the way Excalidraw expects.

## `.excalidrawlib`

Version 2 (current):

```json
{ "type": "excalidrawlib", "version": 2, "source": "...",
  "libraryItems": [ { "id": "...", "status": "published" | "unpublished",
                      "created": 1659863337886, "name": "Stick man",
                      "elements": [ ... ] } ] }
```

Version 1 (71 of the 232 public libraries on 2026-09-28): the key is `library` and its value is an array of element arrays. On load, v1 items get a random `id`, the default status and `created = now`. Both versions go through `restoreElements`; empty items are dropped.

Merge rule: two items are equal when their element `id`/`versionNonce` pairs match in order; new unique items are prepended.

`excali_core::library` ports this:

- **Read.** `parse_library_json` is `parseLibraryJSON` (`data/blob.ts:218-228`). It runs `JSON.parse`, then `isValidLibrary` (`type` `"excalidrawlib"`, `version` the number 1 or 2), then `restore_library_items` on `libraryItems || library` (`restore.ts:1374-1415`). The JS semantics carry over:
  - `||` is truthiness, so `libraryItems: []` wins over `library` while `libraryItems: null` falls back to it.
  - A string of items is iterated by code point and every item is dropped.
  - `null`, `false`, numbers and objects are not iterable and throw `libraryItems is not iterable`.
  - An item that is `null`, an `elements` value that is truthy but not an array, or a `null` element throws out of the whole parse with V8's message.
  - Other items are `{...item, id: item.id || randomId(), status: item.status || defaultStatus, created: item.created || Date.now()}`, so keys keep their place, unknown ones included.
- **Elements.** Each item's elements go through `restoreElements(elements, null)` without its repair pass:
  - each element is restored by `restore_element`, and one that throws, or is `selection` or of an unknown type, is dropped;
  - a repeated id gets a fresh one;
  - `syncInvalidIndices` runs;
  - deleted elements are then removed, and an item left empty is dropped.
- **Write.** `serialize_library_as_json` writes the v2 envelope byte for byte as `serializeLibraryAsJSON` does. `merge_library_items` is `mergeLibraryItems`, and `library_items_hash` is `getLibraryItemsHash`: djb2 in JS arithmetic over UTF-16 code units, sorted by code unit.
- **The typed item.** `LibraryItem` keeps unknown keys and key order. A value it has no form for (a numeric `id`, a `status` other than the two, a string `created`) is written back as read until the field changes.
- **Arrows and lines.** The typed element model reads a missing `elbowed` (arrows from before elbow arrows) or `polygon` (a legacy `draw` restored to `line`) as `false` and writes it back absent, because restore leaves both out (`restore.ts:645-650, 697`).
- **Two differences from upstream.** Both come from the typed model:
  - An element whose restored object the typed model cannot read is dropped. Restore keeps such an object as it is: `restore.ts:459` copies a truthy `strokeWidth` unchanged, so upstream loads a string `"3"`. In the catalogue this happens only to the 24 lines of `aarondiel/logic-gates` whose `strokeWidth` is that string. Tracked as ex-117, which keeps such values and writes them back as read, the way `LibraryItem` keeps an odd `id` or `created`.
  - A legacy arrow binding to an existing element needs geometry, so it is dropped (ex-116, the known gap under the restore rules above). In the catalogue this affects 51 libraries with 1,245 binding ends.
- **Goldens.** `crates/excali-core/tests/fixtures/library.json` is upstream's own output, written by `tools/goldens/library-fixtures.mjs` and re-checked in CI. It holds the parse, merge and hash tables, and for every one of the 232 catalogue libraries the hash of what upstream writes after parsing, with the legacy binding migration and without it.

Import from a URL is allowed only for `excalidraw.com` and `raw.githubusercontent.com/excalidraw/excalidraw-libraries` (suffix match on the host at a subdomain boundary, prefix match on the path). The `#addLibrary=<url>&token=<id>` hash form and the legacy `?addLibrary=` query form are both parsed.

Libraries cannot contain `iframe`, `embeddable` or `image` elements.

## Embedded scenes

- **PNG:** a `tEXt` chunk before `IEND` with keyword `application/vnd.excalidraw+json` and text `JSON.stringify(encode({text: sceneJSON, compress: true}))`. Legacy files hold raw scene JSON in the chunk.
- **SVG:** `<!-- svg-source:excalidraw -->` then `<metadata>` containing `<!-- payload-type:application/vnd.excalidraw+json --><!-- payload-version:2 --><!-- payload-start -->BASE64<!-- payload-end -->`. Without `payload-version` the base64 decodes to UTF-8 text (v1).
- **Wrapper:** `{ version: "1", encoding: "bstring", compressed: bool, encoded: string }`, where `encoded` is a byte string (one char per byte) of the zlib-deflated or raw UTF-8 text.
- **Compression:** the deflated bytes must equal pako 2.0.3 `deflate` at its defaults (level 6), which is what upstream writes. `flate2`'s miniz_oxide backend picks different matches (for the empty-scene fixture its output differs from byte 11), so `excali_core::encode` ports pako's `deflate_slow` and Huffman coder.
- **Decompression:** `excali_core::encode` also ports pako's inflate (`lib/zlib/inflate.js`, `inffast.js`, `inftrees.js` and the `Inflate` driver), because general-purpose inflaters differ from pako on corrupt input. miniz_oxide, for instance, accepts a back-reference before the start of the output and reads zeros where pako throws `invalid distance too far back`, and its errors are not pako's messages. So decoding gives what upstream's `decode` gives for every input: the text, the same thrown message (all 19 of pako's, from `incorrect header check` to `invalid distance too far back`), or `undefined` for truncated input. It keeps the same behaviour: gzip or zlib per stream, concatenated streams, pako's own UTF-8 decoder for compressed text and `TextDecoder` for uncompressed text. It even keeps pako's window bug: a stream that follows one whose output passed 64 KiB cannot refer back past its current 64 KiB output buffer.
- **Reading the wrapper:** as lenient as upstream's `decode`. `encoding` may be missing or any JSON value (`decode: unknown encoding "undefined"`, `"null"`, `"[object Object]"`, ...), `compressed` is tested for truthiness (`1` means compressed), and `version` is ignored. The one difference is that `encoded` must be a string. For other values upstream's behaviour is an accident of its byte-string helpers.
- **Goldens:** `crates/excali-core/tests/fixtures/payload/goldens.json`, produced from upstream `encode.ts` and pako at the pin by `scripts/fixtures/payload-goldens.sh` and re-checked in CI. It holds the encode and deflate cases, the decode cases (wrapper shapes, gzip header and trailer errors, and hand-built streams for every block-data error) and 1,600 corrupted streams inflated by pako as bytes and as a string.
- **PNG chunks:** `excali_core::png` ports `image.ts` (`getTEXtChunk`, `encodePngMetadata`, `decodePngMetadata`) and the three packages it uses at the locked 1.0.0 releases: png-chunks-extract, png-chunks-encode and png-chunk-text. Their behaviour is kept: the first `tEXt` chunk wins; keyword and text are Latin-1; everything after `IEND` is dropped on re-encode; a truncated file reads as zero bytes past its end, so the cut chunk fails its CRC check. Errors are upstream's: the package message, `INVALID` (no scene chunk) or `FAILED` (not a payload); `undefined` for incomplete zlib data. Goldens: `crates/excali-core/tests/fixtures/png/goldens.json`, produced from upstream `image.ts` at the pin by `scripts/fixtures/png-goldens.sh`. CI regenerates them and has upstream decode the PNGs the port writes.
- **Clipboard:** `{ type: "excalidraw/clipboard", elements, files? }` under `application/vnd.excalidraw.clipboard+json` and `text/plain`. See the next section.

## Clipboard

`excali_core::clipboard` ports the JSON half of upstream's `packages/excalidraw/clipboard.ts`.

- **Copy:** `serialize_as_clipboard_json` is `serializeAsClipboardJSON` (`clipboard.ts:143-193`). It writes compact `JSON.stringify({type: "excalidraw/clipboard", elements, files})` output with no indent. `files` holds only the entries for copied image elements that have a `fileId`, in element order. Falsy and missing entries are skipped. The key is left out when no files map is given. `clipboard_items` gives the two MIME types the string is written under (`clipboard.ts:195-210`).
- **Orphaned children:** upstream looks up each element's `frameId` among the *copied* elements (`arrayToMap(elements)`, `clipboard.ts:150`; `getContainingFrame`, `packages/element/src/frame.ts:436-445`). When the id resolves to a copied element that is not a frame or magic frame, the child is written with `frameId: null`. This goes through `mutateElement` on a `deepCopyElement` copy, so `version` goes up by 1, `versionNonce` is fresh and `updated` is now. The deep copy leaves out the element's own `shape` and `canvas` keys, which are render caches upstream (`packages/element/src/duplicate.ts:640-646`). Elements written as given keep them, and so do nested keys with those names. A child whose frame is not among the copied elements is not found, so it keeps its `frameId`. Restore clears that one on paste (`restore.ts:876-887`). The port does exactly the same, and the caller's elements are not changed.
- **Paste:** `parse_clipboard` is `parseClipboard` (`clipboard.ts:523-555`) for `text/plain`, trimmed as `String.prototype.trim` does. Data counts as elements when it parses to an object with `type` `"excalidraw"`, `"excalidraw/clipboard"` or `"excalidraw-api/clipboard"` and an `elements` array (`clipboard.ts:74-88`). The last of these types also sets `programmatic_api`. A plain paste also returns `JSON.stringify(elements, null, 2)` as text. Everything else, including invalid JSON, `null` and `excalidrawlib`, comes back as text. A number literal beyond the f64 range (`1e400`) is `Infinity` to `JSON.parse` and `null` to `JSON.stringify`. `crate::json` reads it as `null`, so such a paste still gives elements and the plain-paste text matches. Only a reader that tells `Infinity` from `null` sees the difference, for example restore's `x ?? 0`. The elements stay untyped for restore. Turning `text/html` into text or mixed content needs a DOM and is left to the host. `parse_clipboard_text` takes the resulting value as it is.
- **Goldens:** `crates/excali-core/tests/fixtures/clipboard.json` is produced by `tools/goldens/clipboard-fixtures.mjs` from upstream's own functions at the pin, and CI re-checks it. It has 26 copy cases and 78 paste cases. Copy cases are compared byte for byte. The nonces come from upstream's `reseed(seed)`, and `updated` is 1 in test mode.

## Fixtures

The corpus for conformance tests is listed in the research page section 9 and assembled by task `ex-003`. Upstream has no JSON Schema; task `ex-115` generates one from the Rust types and publishes it here.
