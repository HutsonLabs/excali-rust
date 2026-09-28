+++
title = "Tauri"
description = "How the same module runs in any Tauri v2 app, and what the plugin adds."
weight = 5
+++

## Tauri hosts the same module

Tauri renders the frontend in the platform webview and lets "developers integrate any front-end framework that compiles to HTML, JS and CSS" (Tauri README, read 2026-09-28). The `<excali-editor>` module is plain ES + WASM, so a Tauri app includes it the way term.hut does: copy the release directory into the frontend dist and import it. `withGlobalTauri` is optional; the editor does not call Tauri APIs itself.

Versions on crates.io on 2026-09-28: `tauri` 2.12.0, `tauri-build` 2.7.0, `wry` 0.57.0, `tao` 0.37.1, `tauri-plugin-fs` 2.6.0, `tauri-plugin-dialog` 2.8.0 (see [evidence](../../evidence/)).

## `tauri-plugin-excali`

A thin plugin for hosts that do not already have file commands like term.hut does.

| Command | Does |
|---|---|
| `excali_open` | Open dialog filtered to `.excalidraw`, `.excalidraw.png`, `.excalidraw.svg`; returns path and text (PNG/SVG payloads decoded by `excali-core`) |
| `excali_save` | Save dialog or path; writes the text |
| `excali_export` | Headless PNG/SVG through `excali-raster` / `excali-svg` (no webview needed, usable from CLI-style commands) |
| `excali_library_fetch` | Fetches a library URL after the allow-list check; the JS side calls it from the `library-fetch` event |
| `excali_validate` | Parses and reports restore diagnostics |

Capability file shipped with the plugin (mirrors term.hut's `capabilities/default.json` style):

```json
{ "identifier": "excali", "windows": ["main"],
  "permissions": ["excali:default", "dialog:default"] }
```

CSP: the module needs `wasm-unsafe-eval` in `script-src` on WebView2 and WebKit builds that enforce it; the integration docs (task `ex-607`) give the exact line per platform after testing in the example app (`ex-606`).

## Native headless use

`excali-cli` and the plugin share `excali-raster`, so a Tauri backend can render thumbnails or previews of `.excalidraw` files without a webview, for example for a file tree, exactly as term.hut's `preview.js` could use a server-side thumbnail over SSH.
