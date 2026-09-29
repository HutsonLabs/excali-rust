+++
title = "Command line"
description = "excali: validate, render (PNG), export (SVG) and lib (list, merge, preview), with their exit codes."
weight = 7
+++

`excali` is the `excali-cli` crate's binary (task `ex-409`). It runs the port's loading, export and library code on files, for scripting and CI; the corpus tests drive it (`crates/excali-cli/tests/corpus.rs`). Upstream has no command line: each command is the code path of an editor action, cited below at the pinned commit.

```sh
cargo run -p excali-cli -- --help
cargo install --path crates/excali-cli   # installs `excali`
```

## Commands

| Command | What it does | Upstream |
|---|---|---|
| `excali validate [--json] FILE...` | Loads each file as the editor opens a file: a scene (`.excalidraw`, `.json`), a scene embedded in a `.png` or `.svg`, or a library (`.excalidrawlib`), whose items are then restored as an import restores them. Prints one line per file (kind, element, file or item counts), or with `--json` an array with one report per file: `kind`, `elements`, `types` (count per element type), `texts` and `files` for a scene, `items` and `elements` for a library, `error` for a file that does not load. Every file is checked; the exit code is the worst. | `loadSceneOrLibraryFromBlob` (`packages/excalidraw/data/blob.ts:138-196`), `parseFileContents` (`:32-80`), `parseLibraryJSON` (`:218-228`) |
| `excali render FILE [options]` | Writes the PNG the editor's "Export image" saves: the non-deleted elements (or one frame, `--frame`), the canvas sized and drawn by `exportToCanvas`, text filled from the vendored font files, images decoded from the scene's files, the scene embedded in a `tEXt` chunk with `--embed-scene`. | `exportCanvas("png")` (`data/index.ts:98-192`), `exportToCanvas` (`scene/export.ts:180-285`), `encodePngMetadata` (`data/image.ts:25-47`) |
| `excali export FILE [options] [--no-fonts]` | Writes the `.svg` file the same export saves: the `exportToSvg` document with the scene in `<metadata>` (`--embed-scene`) and the fonts inlined, subset to the scene's characters (off with `--no-fonts`, upstream's `skipInliningFonts`). The elements themselves are written once the SVG element renderer (`ex-407`) lands. | `exportCanvas("svg")`, `exportToSvg` (`scene/export.ts:293-508`) |
| `excali lib list [--json] FILE` | Lists a library's items: id, status, element count and name, tab-separated; with `--json` an array of `{id, name, status, created, elements}`. | `loadLibraryFromBlob` (`data/blob.ts:230-235`) |
| `excali lib merge FILE... -o OUT` | Imports each library in turn into the first, as the editor imports a library: the imported items not already present (same element ids and `versionNonce`s, in order) go first, then the library's own. Writes the result as the editor saves a library. Nothing is written if any input fails to load. | `mergeLibraryItems` (`data/library.ts:145-157`), `serializeLibraryAsJSON` (`data/json.ts:137-145`) |
| `excali lib preview FILE -o OUT [--items DIR] [--json] [--fonts DIR]` | Writes the preview image the publish dialog makes of a library, a contact sheet of its items (task `ex-410`): rows of six 128 px boxes with 8 px of padding on a white canvas, each box outlined 2 px in `#ced4da`, each item drawn centred in its box by the utils `exportToCanvas` with `maxWidthOrHeight: 128` (the elements restored again with `deleteInvisibleElements`, the default app state: white background, padding 10; no files, so an image element draws the placeholder; the canvas brought down to 128 px on its longer side, or at scale 1 when smaller). `--items DIR` also writes each item's canvas as `item-0001.png`, `item-0002.png`, ... Prints one line per item (id, canvas size, name, tab-separated), or with `--json` `{items: [{index, id, name, width, height, file?}], width, height}`. Upstream re-encodes the image as a JPEG of at most 5000 px for the upload; the CLI writes the PNG it starts from. An empty library has no preview (exit 4; upstream's `chunk` of no items has no first row). | `generatePreviewImage` (`components/PublishLibrary.tsx:38-105`), utils `exportToCanvas` (`packages/utils/src/export.ts:43-104`) |

A `-` output is standard output. A gzip file is read through (its name without `.gz` gives its type), so the catalogue in `fixtures/libraries` validates as stored.

### Export options (`render`, `export`)

| Option | Effect | App state key |
|---|---|---|
| `-o, --output FILE` | Where to write. Default: the input's name with `.png` or `.svg`, or `.excalidraw.png` or `.excalidraw.svg` with `--embed-scene` (upstream's file extensions). A default that would overwrite the input is a usage error. | |
| `--scale N` | Size multiplier (any positive number; the dialog offers 1, 2 and 3). | `exportScale` |
| `--padding N` | Padding in scene pixels. Default 10, and 0 when exporting a frame. | `exportPadding` |
| `--no-background` | Transparent background. | `exportBackground` |
| `--dark` | Dark theme. | `exportWithDarkMode` |
| `--embed-scene` | Embed the scene so Excalidraw can open the image. | `exportEmbedScene` |
| `--frame ID` | Export that frame alone: its overlapping elements, cropped to it (`prepareElementsForExport` with the frame alone selected). | |
| `--source URL` | The `source` of an embedded scene or merged library. Default `https://github.com/HutsonLabs/excali-rust`, or `EXCALIDRAW_EXPORT_SOURCE`. | `getExportSource()` |
| `--fonts DIR` | The font directory. Default: the `crates/excali-text/assets/fonts` the binary was built from, or `EXCALI_FONTS_DIR`. | |

A file's own export settings are not read: upstream does not restore `exportBackground`, `exportScale`, `exportWithDarkMode` or `exportEmbedScene` from a file (`APP_STATE_STORAGE_CONF`, `appState.ts:153-291`), so a loaded scene has the defaults (background on, scale 1, light, not embedded) and the options set them as the export dialog does. `viewBackgroundColor` is read from the file.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Every file loaded, everything was written. |
| 1 | A file is not what the command reads: not a scene or library (`Error: invalid file`), an image without a scene (`Image doesn't contain scene`), an image whose scene cannot be read (`Error: cannot restore image`), not a library (`Invalid library`), or a library whose items do not restore. |
| 2 | Usage error: unknown command or option, a bad value, a missing argument. |
| 3 | A file could not be read or written, or the font directory is missing. |
| 4 | The scene loaded but cannot be exported: no elements (`Cannot export empty canvas.`), a canvas too big to encode (`Error: Canvas too big`: a side over 32767 px or more than 268,435,456 px, the limits browsers put on a canvas), or no frame with the id given. |

Messages go to standard error, prefixed `excali:` and naming the file; the quoted parts are upstream's messages.

## Text and fonts

The PNG's text is drawn from the same vendored faces that measure it ([ADR-004](../../decisions/adr-004-fonts/), [ADR-007](../../decisions/adr-007-text-metrics/)). As the browser fetches only the faces whose `unicode-range` holds a character in use, the CLI loads each vendored face whose range holds a character of the scene's texts or frame names, or printable ASCII. Each `fillText` run is shaped by `FontStore::shape_line` (the runs, faces and advances `measureText` gives), placed by `textAlign` on the alphabetic baseline, and its glyph outlines are filled non-zero and anti-aliased in the run's colour. With `direction` rtl the font runs are laid out from the right. Glyph coverage is tiny-skia's, not Chrome's, so text pixels are not held to the 5-level raster tolerance of the [rendering fidelity](../rendering-fidelity/) page.

## Restore environment

Restore needs the time, fresh ids and random `versionNonce`s. The CLI uses the clock and a generator seeded from it. With `SOURCE_DATE_EPOCH` set, the time is that many seconds and the seed comes from it, so the same input gives the same bytes.

## Tests

- `crates/excali-cli/tests/cli.rs`: each command, option and exit code. Upstream's `export.test.tsx` fixtures (`test_embedded_v1.png`, `smiley_embedded_v2.png` and their SVGs) load with one text element each, and `smiley.png` is rejected. 17 scenes of excali-scene's canvas export golden (upstream's `exportToCanvas`, `tools/goldens/png-export.mjs`), rendered from files, give upstream's canvas size. Embedded PNG and SVG scenes read back. The library merge follows `mergeLibraryItems`.
- `crates/excali-cli/tests/preview.rs`: `lib preview` against `generatePreviewImage`'s layout: the sheet size for 1, 6, 7 and 13 items, the white fill and `#ced4da` outlines, an item smaller than its box drawn at scale 1 and copied pixel for pixel at its centred position, a larger one scaled to 128 px, rows of six, upstream's `fixture_library.excalidrawlib`, and the failures (an empty library, a scene, no output).
- `crates/excali-cli/tests/corpus.rs`: all 232 catalogue libraries validate, list and merge into one library that validates again. Every one of the 4187 catalogue items renders to PNG through `lib preview`, each canvas non-empty and within 128 px, each sheet the size upstream's layout gives. The rust workflow's `corpus-render` job does the same with the release binary (`scripts/fixtures/contact_sheets.py`, which fails unless all 232 libraries render with at least one item each) and keeps the 232 contact sheets, `report.json` and `index.html` as the `corpus-contact-sheets` artifact. Every upstream fixture validates or is rejected as upstream does, and each fixture scene renders and exports to files that validate.
- `crates/excali-text/tests/glyph_outlines.rs`: the shaped line's width is `line_width`, glyphs are the face's outlines scaled onto the baseline, fallback faces and right-to-left runs.
