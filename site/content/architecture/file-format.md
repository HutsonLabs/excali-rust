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
| Scene: invisibly small elements deleted; duplicate ids regenerated; `syncInvalidIndices`; frame membership, container/bound-text pairs and linear bindings repaired; bound text ordered right after its container | `restore.ts:946-1138` |

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

Import from a URL is allowed only for `excalidraw.com` and `raw.githubusercontent.com/excalidraw/excalidraw-libraries` (suffix match on the host at a subdomain boundary, prefix match on the path). The `#addLibrary=<url>&token=<id>` hash form and the legacy `?addLibrary=` query form are both parsed.

Libraries cannot contain `iframe`, `embeddable` or `image` elements.

## Embedded scenes

- **PNG:** a `tEXt` chunk before `IEND` with keyword `application/vnd.excalidraw+json` and text `JSON.stringify(encode({text: sceneJSON, compress: true}))`. Legacy files hold raw scene JSON in the chunk.
- **SVG:** `<!-- svg-source:excalidraw -->` then `<metadata>` containing `<!-- payload-type:application/vnd.excalidraw+json --><!-- payload-version:2 --><!-- payload-start -->BASE64<!-- payload-end -->`. Without `payload-version` the base64 decodes to UTF-8 text (v1).
- **Wrapper:** `{ version: "1", encoding: "bstring", compressed: bool, encoded: string }`, where `encoded` is a byte string (one char per byte) of the zlib-deflated or raw UTF-8 text.
- **Clipboard:** `{ type: "excalidraw/clipboard", elements, files? }` under `application/vnd.excalidraw.clipboard+json` and `text/plain`.

## Fixtures

The corpus for conformance tests is listed in the research page section 9 and assembled by task `ex-003`. Upstream has no JSON Schema; task `ex-115` generates one from the Rust types and publishes it here.
