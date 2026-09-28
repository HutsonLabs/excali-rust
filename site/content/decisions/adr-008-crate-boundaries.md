+++
title = "ADR-008: Crate boundaries and the display list"
description = "The rule that keeps the model, the geometry and the editor testable without a browser."
weight = 8
+++

**Status.** Accepted, 2026-09-28.

## Decision

- `excali-math`, `excali-core`, `excali-rough`, `excali-freehand`, `excali-text`, `excali-scene`, `excali-editor` compile for `wasm32-unknown-unknown` with no `web-sys`, `std::fs` or async runtime. CI builds them for that target.
- Rendering goes through one `DisplayList` type owned by `excali-scene`. Backends (`excali-canvas2d`, `excali-raster`, `excali-svg`) consume it and know nothing about elements.
- The editor returns effects; it never calls the DOM. `excali-ui` applies effects.
- Every crate has its own tests; the golden and corpus suites live where the numbers are produced (`excali-rough`, `excali-scene`, `excali-core`).

## Evidence

term.hut's `hut-core` follows the same shape ("the Tauri-free command implementations … the same code hut-server runs on a remote host", `src-tauri/Cargo.toml`), and upstream separates `packages/math`, `element`, `common`, `excalidraw` and `utils` along similar lines.

## Consequences

A native contributor can work on Phases 1–4 without a browser; the web layer stays thin; the same scene renders identically headless and in the webview, which is what makes the PNG/SVG exports trustworthy.

## Tracked exceptions

Work that these boundaries split across crates, and places where excali-core's typed model loses data upstream's untyped objects keep. Each item is a task in `plan/tasks.json`, and the item stays here until that task closes.

| Gap | Where it shows | Task | Holds |
|---|---|---|---|
| Legacy arrow binding migration (`restore.ts:347-418`): a binding saved without `mode` whose target exists gets a `mode` and `fixedPoint` from element geometry (`isPointInElement`, `projectFixedPointOntoDiagonal`, `calculateFixedPointForNonElbowArrowBinding`). That geometry lives in `excali-editor`, so `excali-core`'s restore asks `RestoreEnv::migrate_legacy_binding`. The default environment answers `None`, and the binding is dropped on load. | `crates/excali-core/src/restore/mod.rs` (`RestoreEnv`), `crates/excali-core/src/library.rs` (library items restore through the same environment: 51 catalogue libraries, 1,245 binding ends), [file format](../../architecture/file-format/) | ex-116 | M1 (ex-m1) |
| Elbow arrow re-route on restore (`restore.ts:1076-1093`): with `repairBindings`, an unbound elbow arrow with a segment that is not axis-aligned is re-routed by `updateElbowArrowPoints`. The router lives in `excali-editor`, so `restoreElements` asks `RestoreEnv::update_elbow_arrow_points`. The default environment answers `None`, and the arrow keeps its restored points. | `crates/excali-core/src/restore/mod.rs` (`RestoreEnv`), [file format](../../architecture/file-format/) | ex-211 | M2 (ex-m2) |
| Arrow label refit on restore (`restore.ts:1032-1045`, `newElement.ts:393-483`): with `refreshDimensions`, `excali_text::restore_env::TextEnv` refits every text (ex-304), but an arrow label is anchored to the box `LinearElementEditor.getBoundTextElementPosition` gives it along the arrow's path. That geometry lives in `excali-editor`, so `TextEnv` takes an `ArrowLabelGeometry`. With `NoArrowGeometry` it answers `None` for an arrow label, and the label keeps its stored size. | `crates/excali-text/src/text_element.rs` (`ArrowLabelGeometry`), `crates/excali-text/src/restore_env.rs` | ex-511 | M5 (ex-m5) |
| Sticky note refit on restore (`restore.ts:931-941`): with `refreshDimensions`, each note and its label are fitted by `getStickyNoteLayout`, which measures text. `restoreElements` asks `RestoreEnv::sticky_note_layout`. The default environment answers `None`, and both keep their stored geometry. Upstream's own callers never set the option. | `crates/excali-core/src/restore/mod.rs` (`RestoreEnv`) | ex-703 | M7 (ex-m7) |
