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
| `excali-math` | `packages/math`; V8's `Math` functions (`excali_math::js`) | `std`, `pxfm` (the correctly rounded `pow`, [ADR-011](../../decisions/adr-011-platform-independent-float-maths/)) | native, wasm32 |
| `excali-core` | `packages/element/src/types.ts`, `packages/excalidraw/data/*`, `packages/fractional-indexing` | `excali-math`, `serde`, `serde_json`, `flate2`, `png` (chunk read/write), `base64`, `nanoid` | native, wasm32 |
| `excali-rough` | `roughjs` 4.6.4 as used by `packages/element/src/shape.ts` | `excali-math`; `serde_json` and `sha2` only behind the test-only `goldens` feature (the golden harness) | native, wasm32 |
| `excali-freehand` | `perfect-freehand` 1.2.0 and `packages/laser-pointer` | `excali-math` | native, wasm32 |
| `excali-text` | `packages/common/src/font-metadata.ts`, `packages/element/src/textWrapping.ts`, `textMeasurements.ts` | `excali-core`, `ttf-parser`, `rustybuzz`, `wuff` (WOFF2 decoding), `unicode-properties`, `yoke` (a parsed face kept with its bytes) | native, wasm32 |
| `excali-scene` | `packages/element/src/shape.ts`, `renderElement.ts`, `packages/excalidraw/renderer/staticScene.ts` | `excali-core`, `excali-rough`, `excali-freehand`, `excali-text` | native, wasm32 |
| `excali-raster` | `exportToCanvas` path of `scene/export.ts` | `excali-scene`, `tiny-skia`, `png` (PNG encoding with the scene's tEXt chunk), `image` (raster image files), `resvg` (SVG image files), `base64` (data URLs) | native |
| `excali-svg` | `renderer/staticSvgScene.ts`, `exportToSvg` | `excali-scene`, `skera` and `skrifa` (font subsetting), `ttf2woff2` (WOFF2 encoding), `wuff` ([ADR-010](../../decisions/adr-010-svg-font-subsetting/)) | native, wasm32 |
| `excali-cli` | none (new); each command is an editor action ([command line](../cli/)) | `excali-raster`, `excali-svg` (and beneath them `excali-scene`, `excali-text` for the font files, `excali-core` for loading and libraries), `clap` | native |
| `excali-editor` | `App.tsx` interaction code, `collision.ts`, `transformHandles.ts`, `binding.ts`, `snapping.ts`, `linearElementEditor.ts`, `store.ts`, `history.ts`, `actions/*` | `excali-scene` | native (tests), wasm32 |
| `excali-canvas2d` | `renderElement.ts` canvas paths | `excali-scene`, `web-sys`, `js-sys` | wasm32 |
| `excali-ui` | `packages/excalidraw/components/*`; scene font loading of `packages/excalidraw/fonts/Fonts.ts` | `excali-editor`, `excali-canvas2d`, `web-sys`, `js-sys`, `wasm-bindgen-futures` | wasm32 |
| `excali-wasm` | the `Excalidraw` React component's public props | `excali-ui`, `wasm-bindgen`, `wasm-bindgen-futures`, `web-sys` | wasm32 |
| `tauri-plugin-excali` | none (new) | `excali-raster`, `excali-core`, `tauri`, `tauri-plugin-dialog` | native |

The dependency direction is enforced in CI: `excali-core`, `excali-math`, `excali-rough`, `excali-freehand`, `excali-text`, `excali-scene` and `excali-editor` are built with `--target wasm32-unknown-unknown` and must not pull `std::fs`, `tokio` or `web-sys`.

## Float maths

Every transcendental function goes through `excali_math::js`, never through the `f64`/`f32` methods ([ADR-011](../../decisions/adr-011-platform-independent-float-maths/)). Upstream's numbers come from V8, whose `Math.sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `log`, `log2`, `log10` and `cbrt` are its own fdlibm (`src/base/ieee754.cc`) on every OS; Rust's methods call the platform's libm, which is an ulp away on a few percent of arguments, differently on macOS and glibc. `excali_math::js` ports V8's code with the fused multiply-adds clang puts in it on arm64 (where the goldens are generated), keeps V8's `Math.hypot`, and answers `pow` with V8's special cases and the correctly rounded power (V8 itself calls the platform's `pow`). The results are the same doubles on every platform, so goldens compare them exactly. The workspace `clippy.toml` disallows the std methods (`sin`, `cos`, `tan`, `sin_cos`, `asin`, `acos`, `atan`, `atan2`, `exp`, `ln`, `log`, `log2`, `log10`, `powf`, `hypot`, `cbrt` and the hyperbolic and `exp2`/`exp_m1`/`ln_1p` ones) in every crate and `tools/` harness; `sqrt`, `floor`, `ceil`, `round`, `abs`, `mul_add` and `powi` stay allowed. Backends reach the functions through `excali_scene::display::js`, so they still see the scene only through the display list. CI runs the tests on Linux x86-64, Linux arm64 and macOS arm64.

## Data flow

1. **Load.** Host passes the file text to `load()`. `excali-core` parses JSON into `Document { elements, app_state, files, extra }`, runs the restore rules, and syncs fractional indices. Unknown keys are kept in `extra` maps at document and element level.
2. **Scene.** `excali-editor` owns the scene with its store and history (`excali_editor::session::Session`, wired as upstream's `App` wires them: `updateScene`, `syncActionResult`, and a store commit after each). Every element change goes through `excali_editor::mutate` (`mutateElement`, `newElementWith`, `bumpVersion`), which bumps `version`, `versionNonce` and `updated` as upstream does.
   - The `Store` (`excali_editor::store`, `store.ts`) keeps the last captured snapshot, with elements shared between snapshots through `Rc` where upstream shares object references. An update captured `IMMEDIATELY` emits a durable increment with a `StoreDelta` (`ElementsDelta` and `AppStateDelta`, `excali_editor::delta`, `delta.ts`); `NEVER` (remote updates, scene load) only moves the snapshot; `EVENTUALLY` waits for the next capture.
   - `History` (`excali_editor::history`, `history.ts`) records the inverse of each durable delta. Undo and redo apply entries without their `version` and `versionNonce`, so each is a new version for collaborators. Binding repair runs as upstream's (`resolveConflicts`), and entries are brought up to date with remote changes (`applyLatestChanges`). Entries that change nothing visible are skipped.
   - After a delta, `excali_editor::delta::redraw_elements` picks what to lay out as upstream's `ElementsDelta.redrawElements` does: each container and bound text pair reached from either side (skipped when either is deleted), then the arrows of every changed, non-deleted bindable element. The two leaf calls, `redrawTextBoundingBox` and `updateBoundElements`, come from the host environment through `HistoryEnv::redraw_text_bounding_box` and `HistoryEnv::update_bound_elements`, as restore takes its text layout from `RestoreEnv`; text editing (ex-512) and arrow binding (ex-510) implement them and the custom element (ex-530) wires them in. As in upstream's development build, debug builds (`HistoryEnv::dev_checks`) fail an undo or redo whose delta cannot be applied, whose layout fails, or whose layout changes an element the delta does not reach; release builds carry on as upstream's production build does, applying the entry as a visible change.
3. **Shapes.** `excali-scene` turns an element into a `Drawable`: rough op-sets for shapes, an outline path for freedraw, text runs for text. Results are cached per element by object identity and invalidated on the same conditions as upstream's `ShapeCache`.
4. **Display list.** The static scene assembles `DisplayItem`s in upstream's order: background, grid, elements with bound text, iframes last. The [display list](../display-list/) page defines the items and their canvas semantics.
5. **Backend.** `excali-canvas2d` (browser) or `excali-raster` (native) consumes the list. `excali-svg` writes it as SVG with upstream's document structure; for SVG the scene hands over the markup `renderSceneToSvg` builds (`display::SvgNode`, `excali_scene::svg_scene`).
6. **Interaction.** Pointer and keyboard events reach `excali-editor` as plain data; it returns a list of `Effect`s (repaint layers, open textarea, set cursor, emit event) that `excali-ui` applies. No DOM calls inside the editor crate, which keeps it testable natively.
7. **Save.** `save()` serialises through `excali-core` with `JSON.stringify(data, null, 2)` semantics and returns the string; the host writes it.

## Why not a Rust web framework

Leptos, Dioxus, Yew and Sycamore all exist and are maintained (versions in the [evidence log](../../evidence/)). They solve reactive DOM diffing. This editor has one canvas that repaints on its own schedule and a chrome of a few dozen panels whose state is already a Rust struct; a framework would add a second state model and a compile-time story the host (term.hut) explicitly does not want. `excali-ui` is a small typed DOM builder over `web-sys` instead: elements are created once, updated by direct property sets from the editor's effects, and the actions registry generates the panels. [ADR-002](../../decisions/adr-002-frontend-runtime/) records the comparison.

## Why not a native GUI toolkit

egui, iced, Slint, Vello and others render outside a webview. Tauri's UI *is* a webview ("WKWebView on macOS & iOS, WebView2 on Windows, WebKitGTK on Linux", from Tauri's README), and term.hut's whole UI is HTML/CSS/JS. A native toolkit would mean a second window or a second rendering stack; a WASM module in the webview means the host keeps one surface. The native crates (`excali-raster`, `excali-cli`, the plugin) cover headless needs without a toolkit.
