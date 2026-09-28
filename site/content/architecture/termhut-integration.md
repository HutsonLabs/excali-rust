+++
title = "term.hut integration"
description = "The contract between the editor module and term.hut: how it is loaded, mounted, saved and fed libraries, with no bundler and no framework."
weight = 4
+++

## What term.hut is

term.hut is "a local, agent-aware terminal for macOS. Tauri v2 shell, Rust PTY backend (`portable-pty`), xterm.js frontend" (its README). Its UI is one `ui/index.html` (2,321 lines) plus ES modules under `ui/src/` and vendored libraries under `ui/vendor/` (xterm, codemirror, perfect-freehand, roughjs, bpmn-js, catppuccin icons). `tauri.conf.json` serves `../ui` directly as `frontendDist`, with `withGlobalTauri: true`. There is no bundler and no `node_modules` (`PRODUCT.md:106-108`).

term.hut already opens `.excalidraw` files: `ui/src/preview.js` routes the `excalidraw` extension to a `canvas` viewer backed by `excalidrawScene.js` (parse and option mapping, 260 lines) and `excalidrawView.js` (draw, pan, zoom, 442 lines). The view is "read-only by design". Files are read and written with the `fs_read_text` and `fs_write_text` Tauri commands (six call sites each in `ui/src`), which also work over SSH through `hut-core`.

## What changes in term.hut

Three things, all in `ui/`:

1. `ui/vendor/excali/` gains the release tarball: `excali_editor.js` (wasm-bindgen `--target web` output plus the custom-element shim), `excali_editor_bg.wasm`, `excali.css`, and `fonts/` with the range-split woff2 files and a manifest.
2. `ui/src/preview.js` routes `excalidraw` to a new `excalidrawEditor.js` (editable) instead of `excalidrawView.js`, keeping the JSON source toggle it already has for `TEXT_BACKED` types. The old view stays as the fallback when the module fails to load.
3. `ui/src/excalidrawEditor.js` (new, small) mounts `<excali-editor>`, wires save to `fs_write_text` with the code editor's dirty and conflict handling, and adds "Import library" to the pane header.

Nothing changes in `src-tauri` for the basic flow. Library import from a URL uses term.hut's existing HTTP path or the plugin's allow-listed fetch.

## Module contract

Loading (plain ES module, no bundler):

```html
<link rel="stylesheet" href="vendor/excali/excali.css">
<script type="module">
  import init, { defineExcaliEditor } from "./vendor/excali/excali_editor.js";
  await init();               // fetches excali_editor_bg.wasm relative to the module
  defineExcaliEditor();       // registers <excali-editor>
</script>
```

Mounting and API (the surface `excali-wasm` exposes; names are final for v1):

```js
const ed = document.createElement("excali-editor");
ed.setAttribute("theme", "dark");            // "light" | "dark" | "system"
ed.setAttribute("ui", "full");               // "full" | "compact" | "mobile" | "auto"
host.appendChild(ed);

await ed.load(text);                          // .excalidraw JSON text; throws with a one-sentence reason
const text2 = ed.save();                      // JSON text, 2-space, upstream key order
const png = await ed.export("png", { scale: 2, background: true, embedScene: true }); // Blob
const svg = ed.export("svg", { embedScene: true });                                 // string
await ed.importLibrary(libJsonTextOrUrl, { merge: true });
const st = ed.getState();                     // { dirty, elementCount, zoom, selectionCount, activeTool }

ed.addEventListener("change", e => { /* e.detail.dirty */ });
ed.addEventListener("save-request", e => { /* Cmd+S inside the editor */ });
ed.addEventListener("open-link", e => { /* e.detail.href; host decides */ });
ed.addEventListener("library-fetch", e => { /* e.detail.url; host may e.preventDefault() and supply e.detail.respond(text) */ });
```

Host adapter rules:

- The editor never touches the file system, the network or the clipboard beyond the DOM Clipboard API. Every side effect is an event the host handles.
- `theme` maps to upstream's `.theme--dark` tokens. term.hut is dark-only, so it sets `theme="dark"` and may override `--color-primary*` and `--island-bg-color` with Catppuccin values inside the element's scope, as upstream's theming doc allows.
- Fonts are fetched relative to the module URL by unicode range, on demand.

## Save flow in term.hut

1. Editor emits `change` → tab shows the dirty dot as the code editor does.
2. Cmd+S in the pane or `save-request` from the editor → `fs_write_text(path, ed.save(), conn)`.
3. If the file changed on disk since load (term.hut's `fs_watch_files` already reports this), term.hut prompts exactly as it does for text files; on "reload", it calls `ed.load(newText)`.
4. On SSH workspaces nothing differs: `fs_write_text` takes the connection.

## CRUD in term.hut

| Operation | term.hut command | Editor call |
|---|---|---|
| Create | `fs_create(path)` then `fs_write_text` with an empty scene (`elements: []`, default `appState`) | `ed.load(emptyScene)` |
| Read | `fs_read_text(path)` | `ed.load(text)` |
| Update | `fs_write_text(path, text)` | `ed.save()` |
| Delete | `fs_trash(path)` with its refusal rules | none |
| Rename | `fs_rename` | none |

## Library import in term.hut

- "Import library" in the pane header accepts a URL or a `.excalidrawlib` file (via `tauri-plugin-dialog`, already a term.hut dependency).
- URLs are checked against the upstream allow-list before any fetch. A libraries.excalidraw.com link of the form `...#addLibrary=<url>&token=<id>` is parsed the same way upstream's `parseLibraryTokensFromUrl` parses it.
- The personal library is stored under `~/.term-hut/excalidraw/library.excalidrawlib` (v2) on the host that owns the workspace, so it follows the anchored-workspace model.

## Size budget in this host

term.hut ships a 5.4 MB dmg and states "Weight is a feature" (`PRODUCT.md:165`). The module budget on the [phases page](../../plan/phases/#budgets) is set against that number.
