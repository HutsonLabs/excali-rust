+++
title = "ADR-002: Rust to WASM with a DOM builder, no framework"
description = "Why the editor is a wasm-bindgen module driving Canvas2D and the DOM directly, rather than a Rust web framework or a native GUI toolkit."
weight = 2
+++

**Status.** Accepted, 2026-09-28. Revisit at M5 with measured bundle sizes.

## Question

What runs the editor's UI in the browser and in a Tauri webview, given that the host must not need React, a bundler or a framework?

## Evidence

- term.hut: "No build step, no framework. Vanilla JS, no bundler, no package manager, no `node_modules`. … Third-party code is vendored under `ui/vendor/` as UMD/static files" (`PRODUCT.md:106-108`).
- Tauri: the frontend is "any front-end framework that compiles to HTML, JS and CSS", rendered in "WKWebView on macOS & iOS, WebView2 on Windows, WebKitGTK on Linux and Android System WebView on Android" (Tauri README, 2026-09-28).
- crates.io on 2026-09-28: `wasm-bindgen` 0.2.129, `web-sys` 0.3.106, `leptos` 0.8.21, `dioxus` 0.7.10, `yew` 0.23.0, `sycamore` 0.9.3, `egui` 0.36.2, `iced` 0.14.0, `slint` 1.18.1, `vello` 0.10.0.
- Leptos README: "Easy to use with Trunk … or with a simple wasm-bindgen setup"; client-side rendering supported. Dioxus README: web "Render directly to the DOM using WebAssembly", desktop "using Webview". Neither README mentions Tauri or embedding as a library inside an existing page.
- Upstream's editor state is already one object tree (`AppState`, `Scene`, `Store`); its React tree is a view over it.

## Options

1. **Rust web framework (Leptos/Dioxus/Yew) compiled to WASM.** Gains reactive DOM diffing; costs a second state model, framework-specific build tooling (trunk / dx) that the host must not need, and larger modules.
2. **Native GUI toolkit (egui/iced/Slint) in its own window or surface.** Cannot live inside the host's webview pane; breaks term.hut's one-surface model.
3. **wasm-bindgen `--target web` module with `web-sys`: Canvas2D for the scene, a small typed DOM builder for the chrome, the actions registry as data.** No host tooling; one state model; the chrome is a few dozen panels generated from data.

## Decision

Option 3. The public surface is a custom element `<excali-editor>` registered by the module. Rendering goes through a display list so the same scene code drives `tiny-skia` natively.

## Consequences

- `excali-ui` must provide its own popover, dialog, tooltip and menu primitives; upstream's Radix Popover has no equivalent here. Bounded: the kit is listed on the [icons and components page](../../design-system/icons/).
- Testing the DOM layer needs a browser; Playwright with the pre-installed Chromium covers it (`ex-531`).
- If M5 shows the DOM-builder approach costing more than a framework would save, Option 1 can be adopted for the chrome only, since the editor and scene crates never touch the DOM.
