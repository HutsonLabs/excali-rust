+++
title = "Overview"
description = "Primitives first: the crate graph, what each crate may depend on, and how a scene flows from JSON to pixels."
weight = 1
+++

## Principle

Build from primitives up. Each crate below is a pure layer over the one beneath it, tested on its own, and the web runtime is the last, thinnest layer. Nothing in the first four layers knows about the DOM, the file system or Tauri.

```
                 ┌────────────────────────────────────────────┐
 hosts           │ term.hut (vanilla JS)   Tauri app   any web │
                 └──────────────┬─────────────────┬───────────┘
                                │ ES module        │ plugin commands
 web runtime     ┌──────────────▼──────────────┐ ┌─▼──────────────────┐
                 │ excali-wasm  <excali-editor> │ │ tauri-plugin-excali │
                 │ excali-ui    DOM chrome      │ └─┬──────────────────┘
                 │ excali-canvas2d              │   │
                 └──────────────┬──────────────┘   │
 editor          ┌──────────────▼──────────────┐   │
                 │ excali-editor  tools, hit    │   │
                 │   testing, transforms,       │   │
                 │   binding, snapping, history │   │
                 └──────────────┬──────────────┘   │
 render          ┌──────────────▼──────────────┐ ┌─▼──────────────────┐
                 │ excali-scene   display list  │ │ excali-raster (png) │
                 │ excali-text    metrics, wrap │ │ excali-svg          │
                 │ excali-rough   sketch paths  │ │ excali-cli          │
                 │ excali-freehand              │ └─┬──────────────────┘
                 └──────────────┬──────────────┘   │
 model           ┌──────────────▼──────────────────▼───────────────────┐
                 │ excali-core   types, serde, restore, index, library, │
                 │               payload codecs                         │
                 │ excali-math   points, vectors, curves, polygons      │
                 └─────────────────────────────────────────────────────┘
```

## Crates

| Crate | Upstream counterpart | May depend on | Targets |
|---|---|---|---|
| `excali-math` | `packages/math` | `std` only | native, wasm32 |
| `excali-core` | `packages/element/src/types.ts`, `packages/excalidraw/data/*`, `packages/fractional-indexing` | `excali-math`, `serde`, `serde_json`, `flate2`, `png` (chunk read/write), `base64`, `nanoid` | native, wasm32 |
| `excali-rough` | `roughjs` 4.6.4 as used by `packages/element/src/shape.ts` | `excali-math`; `serde_json` and `sha2` only behind the test-only `goldens` feature (the golden harness) | native, wasm32 |
| `excali-freehand` | `perfect-freehand` 1.2.0 and `packages/laser-pointer` | `excali-math` | native, wasm32 |
| `excali-text` | `packages/common/src/font-metadata.ts`, `packages/element/src/textWrapping.ts`, `textMeasurements.ts` | `excali-core`, `ttf-parser`, `rustybuzz` | native, wasm32 |
| `excali-scene` | `packages/element/src/shape.ts`, `renderElement.ts`, `packages/excalidraw/renderer/staticScene.ts` | `excali-core`, `excali-rough`, `excali-freehand`, `excali-text` | native, wasm32 |
| `excali-raster` | `exportToCanvas` path of `scene/export.ts` | `excali-scene`, `tiny-skia`, `image` | native |
| `excali-svg` | `renderer/staticSvgScene.ts`, `exportToSvg` | `excali-scene` | native, wasm32 |
| `excali-cli` | none (new) | `excali-raster`, `excali-svg`, `clap` | native |
| `excali-editor` | `App.tsx` interaction code, `collision.ts`, `transformHandles.ts`, `binding.ts`, `snapping.ts`, `linearElementEditor.ts`, `store.ts`, `history.ts`, `actions/*` | `excali-scene` | native (tests), wasm32 |
| `excali-canvas2d` | `renderElement.ts` canvas paths | `excali-scene`, `web-sys` | wasm32 |
| `excali-ui` | `packages/excalidraw/components/*` | `excali-editor`, `excali-canvas2d`, `web-sys` | wasm32 |
| `excali-wasm` | the `Excalidraw` React component's public props | `excali-ui`, `wasm-bindgen` | wasm32 |
| `tauri-plugin-excali` | none (new) | `excali-raster`, `excali-core`, `tauri`, `tauri-plugin-dialog` | native |

The dependency direction is enforced in CI: `excali-core`, `excali-math`, `excali-rough`, `excali-freehand`, `excali-text`, `excali-scene` and `excali-editor` are built with `--target wasm32-unknown-unknown` and must not pull `std::fs`, `tokio` or `web-sys`.

## Data flow

1. **Load.** Host passes the file text to `load()`. `excali-core` parses JSON into `Document { elements, app_state, files, extra }`, runs the restore rules, and syncs fractional indices. Unknown keys are kept in `extra` maps at document and element level.
2. **Scene.** `excali-editor` owns the `Scene` (elements, non-deleted map, frames, nonce) and the `Store` (snapshots and deltas for history). Every mutation goes through `mutate(element, patch)`, which bumps `version`, `versionNonce` and `updated` as upstream's `mutateElement` does.
3. **Shapes.** `excali-scene` turns an element into a `Drawable`: rough op-sets for shapes, an outline path for freedraw, text runs for text. Results are cached per element by object identity and invalidated on the same conditions as upstream's `ShapeCache`.
4. **Display list.** The static scene assembles `DisplayItem`s in upstream's order: background, grid, elements with bound text, iframes last.
5. **Backend.** `excali-canvas2d` (browser) or `excali-raster` (native) consumes the list. `excali-svg` writes it as SVG with upstream's document structure.
6. **Interaction.** Pointer and keyboard events reach `excali-editor` as plain data; it returns a list of `Effect`s (repaint layers, open textarea, set cursor, emit event) that `excali-ui` applies. No DOM calls inside the editor crate, which keeps it testable natively.
7. **Save.** `save()` serialises through `excali-core` with `JSON.stringify(data, null, 2)` semantics and returns the string; the host writes it.

## Why not a Rust web framework

Leptos, Dioxus, Yew and Sycamore all exist and are maintained (versions in the [evidence log](../../evidence/)). They solve reactive DOM diffing. This editor has one canvas that repaints on its own schedule and a chrome of a few dozen panels whose state is already a Rust struct; a framework would add a second state model and a compile-time story the host (term.hut) explicitly does not want. `excali-ui` is a small typed DOM builder over `web-sys` instead: elements are created once, updated by direct property sets from the editor's effects, and the actions registry generates the panels. [ADR-002](../../decisions/adr-002-frontend-runtime/) records the comparison.

## Why not a native GUI toolkit

egui, iced, Slint, Vello and others render outside a webview. Tauri's UI *is* a webview ("WKWebView on macOS & iOS, WebView2 on Windows, WebKitGTK on Linux", from Tauri's README), and term.hut's whole UI is HTML/CSS/JS. A native toolkit would mean a second window or a second rendering stack; a WASM module in the webview means the host keeps one surface. The native crates (`excali-raster`, `excali-cli`, the plugin) cover headless needs without a toolkit.
