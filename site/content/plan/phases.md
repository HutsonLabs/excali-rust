+++
title = "Phases and milestones"
description = "Eight phases, each ending in a milestone with an automated acceptance check. Phase 0 is this site."
weight = 2
+++

Each phase is an epic in the tracker (`ex-e0` … `ex-e8`). A milestone is reached when its acceptance check runs green in CI, not when the code is merged. Later phases may start before earlier ones close where the [task graph](../progress/) shows no blocking edge.

## Phase 0 — Foundations (`ex-e0`)

**Status (2026-09-29, recheck): M0 reached.** Checked again on `main` at b98ca56 with every agent-doable phase 0 task (ex-001 to ex-009 and ex-g001) merged. Pages deploys from `main`: the pages run for b98ca56 (<https://github.com/HutsonLabs/excali-rust/actions/runs/36540943921>) built, deployed and passed its post-deploy smoke, and the site returns 200. `scripts/bootstrap.sh` completes in a fresh clone of b98ca56 under macOS `/bin/bash` 3.2 (Zola 0.22.0 SHA-256 verified, upstream at 438d898, gate self-check OK), and the `bootstrap-and-site` jobs pass on ubuntu and macOS. CI rejects a planted attribution line: the `attribution` job of the gates run for b98ca56 (<https://github.com/HutsonLabs/excali-rust/actions/runs/36540943949>) ran `scripts/gates/test_attribution.py`, whose 11 cases plant a violation in a scratch repository and assert the gate exits 1, with a clean control exiting 0. The workspace is no longer empty: `cargo test --workspace --locked --no-fail-fast` passes 1679 tests (0 failed, 0 ignored), and the rust workflow for b98ca56 (<https://github.com/HutsonLabs/excali-rust/actions/runs/36540943984>) is green. To verify, run `scripts/bootstrap.sh` in a fresh clone, then `cargo test --workspace --locked` and `python3 scripts/gates/test_attribution.py -v`, and `bd show ex-m0`.

**Deliverables.** This site; the authorship gate and hooks; the beads tracker seeded with the task graph; GitHub Pages deployment; a Cargo workspace skeleton with CI running `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test` on stable.

**Milestone M0.** The site builds and deploys from `main`; `scripts/bootstrap.sh` works on a fresh clone; the gate rejects a planted attribution line in CI.

## Phase 1 — Core model and file format (`ex-e1`)

**Status (2026-09-29, third check): M1 reached.** On `main` at c0b1565 every phase 1 task (ex-101 to ex-117 and ex-g101) is merged and CI is green (rust <https://github.com/HutsonLabs/excali-rust/actions/runs/36659243637>, gates <https://github.com/HutsonLabs/excali-rust/actions/runs/36659243645>). `cargo test --workspace --locked` passes 2212 tests (0 failed, 0 ignored). The fixture corpus (252 files) and the upstream copies verify, the 290 golden generator tests pass and the round-trip goldens `--check` steps are up to date, including the document round trip (`document-fixtures.mjs`). Every scene-bearing upstream test fixture, `diagramFixture` included, round-trips to upstream's bytes with unknown keys kept, and all 232 catalogue libraries parse, write and parse again byte for byte against upstream's output. `library-corpus-report.json` now shows 55113 elements in and 55113 out: the aarondiel/logic-gates lines with the string `strokeWidth` "3" are kept as upstream keeps them (ex-117). The 1245 legacy arrow bindings saved without `mode` are migrated by excali-editor's `RoutingEnv` exactly as upstream's `repairBinding` does; `crates/excali-editor/tests/legacy_binding.rs` checks all 51 libraries that hold them against upstream's output (ex-116). The report still counts them as cleared because excali-core's own test environment has no geometry, which is the documented crate boundary, not a loss. To verify, run `cargo test -p excali-core --locked --test library_corpus` and `cargo test -p excali-editor --locked --test legacy_binding`.

**Crate.** `excali-core` (no `std::fs`, no DOM, `wasm32`-clean).

**Deliverables.**
- Element types as Rust enums and structs mirroring `packages/element/src/types.ts` ([spec](../../architecture/file-format/)), with `serde` and an `extra: Map` on every element so unknown fields round-trip exactly as upstream's `restoreElementWithProperties` spreads them back.
- The restore rules: defaults, legacy `strokeSharpness` → `roundness`, `boundElementIds` → `boundElements`, legacy `font` string, `draw` → `line`, arrowhead renames, point re-basing, negative-size normalisation, invisible-element deletion, duplicate-id repair, binding repair.
- `AppState` with the five exported keys and defaults.
- Fractional indexing (port of the vendored base-62 implementation) with `syncInvalidIndices`.
- Library formats: v1 `library` and v2 `libraryItems`; merge by `id:versionNonce` set; URL allow-list identical to upstream.
- The clipboard, PNG `tEXt` and SVG `<metadata>` payload codecs (zlib: pako's deflate and inflate ported for byte-identical output and pako's errors; byte-string encoding; v1 and v2 variants).
- Conformance fixtures copied from upstream tests plus every file in `excalidraw-libraries` as a corpus test.

**Milestone M1.** D1 passes: all fixtures and all 232 catalogue libraries round-trip; the restore snapshot tests ported from `tests/data/restore.test.ts` pass.

## Phase 2 — Geometry and sketch renderer (`ex-e2`)

**Status (2026-09-28, second check): M2 reached.** On `main` at 0eb233c every agent-doable phase 2 task (ex-201 to ex-218, ex-g201 and ex-g202) is merged and the rust workflow's `goldens` job is green in CI (<https://github.com/HutsonLabs/excali-rust/actions/runs/36459520512>). `cargo test --workspace --locked --no-fail-fast --features excali-scene/goldens` passes 1093 tests (0 failed, 0 ignored), the 105 golden generator tests pass and all sixteen goldens `--check` steps are up to date. `goldens/elements-matrix.json` holds upstream's `generateElementShape` output for 684 cases: 19 element variants (rectangle and diamond sharp and rounded, ellipse, iframe, embeddable, looped, curved and polygon lines, sharp and curved arrows with a filled head, looped freedraw at variable and constant width, and the shapeless text, image, frame, magicframe and stickynote) at hachure, cross-hatch, zigzag and solid fills, roughness 0, 1 and 2 and seeds 1, 7 and 1041657908. The port matches every case, the looped freedraw background fill included, and `element_matrix` fails CI if any cell of the matrix has no case. To verify, run `node tools/goldens/generate.mjs --check` and `cargo test --workspace --locked --features goldens --test element_matrix`, then `bd show ex-m2`.

**Crates.** `excali-math` (points, vectors, curves, segments, polygons, ellipses, ranges: the `packages/math` surface), `excali-rough` (Rough.js 4.6.4 semantics), `excali-freehand` (perfect-freehand 1.2.0 semantics and the laser-pointer constant-width variant), `excali-scene` (display list: a renderer-independent list of paths, fills, dashes, images and text runs).

**Deliverables.**
- The Park–Miller generator exactly as rough.js (`seed = imul(48271, seed) & (2^31 − 1)`), so a stored `seed` produces the same wobble.
- Fill styles hachure, cross-hatch, solid, zigzag; dashes; multi-stroke; curve fitting; `preserveVertices`.
- The option mapping in `generateRoughOptions` and `adjustRoughness` with every constant from the [rendering research](../../research/rendering/#shapecache-and-roughjs-packageselementsrcshapets).
- Shape construction per element type: adaptive/proportional corner radius, diamond points, ellipse with `curveFitting = 1`, linear paths and curves, elbow-arrow path with radius 16, all fourteen arrowheads with their sizes and angles, freedraw outline to path with 2-decimal trimming.
- A golden-test harness that runs upstream's `rough.js` under Node on the fixture set and stores its path output; the Rust output must match number-for-number at two decimals.

**Milestone M2.** Golden tests pass for every element type and fill style at roughness 0, 1 and 2, for seeds 1, 7 and 1041657908 (the fixture seed).

## Phase 3 — Text and fonts (`ex-e3`)

**Status (2026-09-28, second check): M3 reached.** On `main` at eec2b7b every agent-doable phase 3 task (ex-301 to ex-308, ex-g301, ex-g302 and ex-g303) is merged. `cargo test -p excali-text --locked` passes 217 tests (0 failed, 0 ignored), including `textWrapping.test.ts` ported case for case and the bound-text cases of `textElement.test.ts`. The font metadata, font asset, text wrapping and text-element-sizing goldens `--check` are up to date against upstream at the pin (438d898), and the text-width and glyph-advance fixtures check OK. The whole-corpus width gate (`text_width_corpus`) now holds all six vendored families (Virgil, Cascadia, Excalifont, Nunito, Lilita One, Comic Shanns) to 0.5 px on every text whose stored width is a browser measurement: 1243 Virgil, 35 Cascadia, 198 Excalifont, 20 Nunito, 3 Lilita One and 39 Comic Shanns texts. The 114 texts outside 0.5 px are pinned, and for 113 of them the test recomputes the stored width exactly (within 0.001 px) from the older upstream measurement that wrote it (DOM `offsetWidth`, with and without the +1, scaled, container width, whole-pixel font size, or the 2024 ink-box `getLineWidth`). The remaining one, a Lilita One text, keeps a width from an earlier state of the element. `scripts/fixtures/text-width-causes.sh --check` runs upstream's own restore and `getLineWidth` in Playwright's Chromium and reproduces the committed causes fixture. Helvetica (465 texts) is `local:` only upstream, so it has no vendored font to measure with and is counted but not measured (ADR-004, ADR-007). To verify, run `cargo test -p excali-text --locked --test text_width_corpus`, read `crates/excali-text/tests/fixtures/text-width-corpus-report.json`, then `bd show ex-m3`.

**Crate.** `excali-text`.

**Deliverables.**
- Font metadata table (unitsPerEm, ascender, descender, lineHeight) for the ten families and the fallbacks; the vertical-offset formula.
- Advance-width measurement from the actual font files via `ttf-parser` (and `rustybuzz` where shaping matters), replacing `canvas.measureText`.
- Wrapping ported from `packages/element/src/textWrapping.ts`, with upstream's `textWrapping.test.ts` cases as fixtures (the test-environment metric of 10 px per character reproduced for those tests).
- Bound-text sizing rules (padding 5, ellipse and diamond insets, arrow label width).
- Lazy font loading by unicode range, mirroring upstream's split woff2 files; licences recorded per family before any file is vendored ([ADR-004](../../decisions/adr-004-fonts/)).

**Milestone M3.** Wrapping and measurement tests pass; a text element measured in Rust matches upstream's stored `width`/`height` for the fixture corpus within 0.5 px.

## Phase 4 — Headless rendering and export (`ex-e4`)

**Status (2026-09-29): M4 reached.** Checked on `main` at ae23249, with every agent-doable phase 4 task (ex-401 to ex-410) and both gap tasks (ex-g401, ex-g402) merged. `cargo test --workspace --locked --no-fail-fast` passes 1669 tests (0 failed, 0 ignored), the 171 golden generator tests pass and all 27 goldens `--check` steps are up to date. The SVG half of D2: the port's `exportToSvg` equals upstream's snapshots byte for byte, the four of `tests/scene/export.test.ts` and the two of `tests/export.test.tsx` (the svg-embedded scene and the four transformed images), and 48 whole `exportToSvg` documents. The PNG half of D2: `scripts/fixtures/chrome-png-export.sh --check` redraws 35 fixture scenes with upstream's own `exportToCanvas` in the local Chromium 153 and holds the port's PNG export to them, canvas size exactly and every pixel within the tolerance recorded per scene in `crates/excali-cli/tests/fixtures/chrome-export/tolerances.json`; 34 scenes are compared, and `smiley_embedded_v2` (an emoji with no vendored face, ADR-004) is excluded, with only its size and the pixels outside its text checked. `scripts/fixtures/raster-references.sh --check` holds all raster fixtures to the local Chrome, and the release CLI renders all 4187 items of the 232 catalogue libraries with 0 failures. To verify, run `cargo test -p excali-svg --locked --test upstream_snapshot --test export_test_tsx`, `scripts/fixtures/chrome-png-export.sh --check` (macOS arm64, needs the upstream checkout and `tests/web` installed), `cargo build --release --locked -p excali-cli && python3 scripts/fixtures/contact_sheets.py --excali target/release/excali --out target/contact-sheets`, then `bd show ex-m4`.

**Crates.** `excali-raster` (tiny-skia backend), `excali-svg` (SVG writer), `excali-cli`.

**Deliverables.**
- Static-scene renderer over the display list: background, grid, element order, bound text, frames with clipping, opacity, dark-mode filter maths.
- PNG export with padding 10 and scale, embedding the scene in a `tEXt` chunk; SVG export with the upstream document structure (`svg-source` comment, `<metadata>` payload, `<defs>` clip paths, font-face style block, per-element `<g>` transforms, two-decimal numbers).
- `excali-cli validate | render | export | lib` for scripting and CI.

**Milestone M4.** D2 passes for the fixture set; the CLI renders all 232 catalogue libraries' preview items without panics.

## Phase 5 — Web runtime and editor (`ex-e5`)

**Crates.** `excali-canvas2d` (display list → `CanvasRenderingContext2D` via `web-sys`), `excali-editor` (interaction state machine and history), `excali-ui` (DOM chrome built with `web-sys`, no framework), `excali-wasm` (the `<excali-editor>` custom element, built with `wasm-bindgen --target web` so it loads as a plain ES module).

**Deliverables.**
- Static and interactive canvas layers at device-pixel scale; per-element bitmap cache with the same padding rules and snapping.
- Tools: hand, selection, rectangle, diamond, ellipse, arrow, line, freedraw, text, image, eraser, frame, laser; tool lock; lasso, bucket fill, sticky note and autoshape follow in Phase 7.
- Selection, transform handles (8/16/28 px by pointer type), rotation, resize from centre, aspect lock, snapping (8/zoom), arrow binding with gap 5 + width/2, elbow routing, text editing via a positioned `<textarea>` overlay, linear-point editing.
- History as store deltas with undo/redo; element `version`/`versionNonce`/`updated` bumps.
- Chrome: main menu, styles panel (full, compact and mobile modes), shapes toolbar, extra-tools dropdown, footer (zoom, undo/redo, help), help dialog, library sidebar, context menu, colour and font pickers, hints, stats, command palette, welcome screen.
- Keyboard: every shortcut in the [table](../../design-system/shortcuts/).
- Public JS API of the custom element: `load(json)`, `save() → json`, `export(kind, opts)`, `importLibrary(json | url)`, `getState()`, events `change`, `save-request`, `open-link`; host adapter hooks for file dialogs and fetch.

**Milestone M5.** D3 for desktop; a Playwright suite drives the element in Chromium against the [parity checklist](@/plan/parity.md); WASM size within budget.

## Phase 6 — Host integration (`ex-e6`)

**Deliverables.**
- term.hut: replace the read-only view in `ui/src/preview.js` with the editor for `.excalidraw`; wire save to `fs_write_text` with the existing conflict handling; add "New drawing" to the tree's create menu; add "Import library" (URL or file) using the allow-list; keep the JSON source toggle.
- `tauri-plugin-excali`: commands for open/save dialogs (via `tauri-plugin-dialog`), headless export through `excali-raster`, and an allow-listed library fetch; a capability file; an example app.

**Milestone M6.** D4 and D5 pass end to end.

## Phase 7 — Parity and polish (`ex-e7`)

Tablet and phone layouts; compact styles panel; sticky notes, bucket fill, lasso, autoshape, image crop, frames with names, search; accessibility (focus order, ARIA on controls, reduced motion); performance budgets (60 fps pan at 1,000 elements on a 2020 laptop; first paint under 300 ms after module load); locale loader with the upstream JSON files.

**Milestone M7.** [Parity checklist](@/plan/parity.md) at 100% for the v1 scope; budgets met in CI on a pinned runner.

## Phase 8 — Release (`ex-e8`)

The first release is **26.9.1**, numbered by calendar `YY.M.BUILD` ([ADR-009](../../decisions/adr-009-calendar-versioning/)).

**Deliverables.**
- The ES module + WASM tarball, with its SHA-256, as a GitHub release asset that term.hut's vendor script can fetch (`ex-802`).
- The integration guide, walked by an agent step by step in a fresh clone in a temp directory, with the transcript recorded on the issue (`ex-803`).
- Tag `v26.9.1` and the GitHub release (`gh release create v26.9.1`), created by an agent (`ex-804`).
- Publishing the `excali-*` crates to crates.io (`ex-801`) is deferred by the owner (2026-09-27); nothing goes to crates.io, npm or any other registry, and the release does not wait on it.

**Milestone M8.** `v26.9.1` is tagged and its GitHub release carries the tarball and checksum; the guide walk transcript is linked. An agent closes M8 once that check is green in CI with the evidence posted.

## Budgets

| Budget | Value | Basis |
|---|---|---|
| WASM module, no fonts, gzip | ≤ 1.5 MB | term.hut ships a 5.4 MB dmg and treats weight as a feature (`PRODUCT.md:165`); upstream's bundle is ~3.9 MB (`excalidrawScene.js:9`). Half of that is the ceiling; the number is a target to measure against, not a fact. |
| ES-module JS (wasm-bindgen glue and shim), gzip | ≤ 20 KB | The shim only mounts the element and forwards host callbacks. |
| First paint after module load | ≤ 300 ms | Upstream shows its canvas immediately; a slower start would read as a regression in term.hut. |
| Fonts | lazy, per unicode range | Upstream ships Excalifont in 7 range-split files and Xiaolai in ~209; the port keeps that split. |

The two size rows are enforced on every pull request (`ex-501`): `scripts/web/build.sh` builds `dist/excali_editor.js` and `dist/excali_editor_bg.wasm` (profile `web-release`, then `wasm-opt -Oz` from the pinned binaryen), and `scripts/gates/wasm_size.py` reads the budgets from this table and fails the build when either file's gzip size (level 9, no header timestamp) is over. Units are decimal: 1 KB is 1,000 bytes, 1 MB is 1,000,000 bytes.
