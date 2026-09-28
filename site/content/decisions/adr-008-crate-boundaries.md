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
