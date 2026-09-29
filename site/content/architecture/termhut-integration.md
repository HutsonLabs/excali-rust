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

1. `ui/vendor/excali/` gains the release tarball: `excali_editor.js` (wasm-bindgen `--target web` output plus the custom-element shim), `excali_editor_bg.wasm`, `excali.css`, and `fonts/` with the range-split font files, their licences and `manifest.json` (built by `scripts/web/build.sh` from `crates/excali-text/assets/fonts/`).
2. `ui/src/preview.js` routes `excalidraw` to a new `excalidrawEditor.js` (editable) instead of `excalidrawView.js`, keeping the JSON source toggle it already has for `TEXT_BACKED` types. The old view stays as the fallback when the module fails to load.
3. `ui/src/excalidrawEditor.js` (new, small) mounts `<excali-editor>`, wires save to `fs_write_text` with the code editor's dirty and conflict handling, and adds "Import library" to the pane header.

The first two landed in term.hut PR #87 (ex-601, merge `7330c73`, 2026-09-29):

- `ui/vendor/excali/` is written by term.hut's `scripts/vendor-excali.sh <excali-rust checkout>`, which runs `scripts/web/build.sh` and adds `LICENSE`, `SOURCE.json` (repository, commit, version) and `SHA256SUMS`. `ui/test/excaliVendor.test.js` holds the directory to those checksums. The release in it is 26.9.1 at `2da1897`.
- `fonts/Xiaolai`, upstream's CJK fallback, is not vendored. It is 12 MB of the release's 14 MB of fonts, which would triple term.hut's dmg (`PRODUCT.md`: ~5.4 MB, "should stay in that class"). The font registry is compiled into the module, so a missing face only fails its own unicode range, and CJK text falls back to the webview's system font. The vendored directory is 3.0 MB.
- `preview.js` routes `excalidraw` to a `drawing` view (`excalidrawEditor.js`), which loads the module when the first drawing opens. `TEXT_BACKED` keeps the JSON toggle. The read-only `excalidrawView.js` canvas is what the `excalidrawReadOnly` setting shows (Settings ▸ Editor ▸ Read-only drawings, off by default), and it is also the fallback when the module fails to load. `save-request` and the pane header's Save button write `ed.save()` with `fs_write_text`. Dirty state and conflict handling are ex-602.

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

The element is `excali_wasm::web::EditorCore` behind a small shim (`crates/excali-wasm/js/excali-editor.js`, appended to `excali_editor.js` by `scripts/web/build.sh`); the editor itself, without the DOM, is `excali_wasm::editor::Editor`, tested natively (`crates/excali-wasm/tests/editor.rs`) and in Chromium from the page above (`tests/web/specs/editor.spec.mjs`, ex-530). What each call does:

- `load(text)` restores the file as upstream's `loadFromBlob` does and resets the history; it rejects with one sentence ("The text is not valid JSON (…).", "The JSON is not an Excalidraw scene.", "The scene could not be restored (…)."), leaving the scene as it was.
- `save()` is `serializeAsJSON(elements, appState, files, "local")` with `source` the page's origin; the text it returns becomes the clean state, so `getState().dirty` and `change`'s `detail.dirty` say whether the scene differs from what was loaded or last saved.
- `export("png", …)` paints `exportToCanvas` into a canvas and adds the scene as upstream's `tEXt` chunk with `embedScene`; `export("svg", …)` is `exportToSvg` with each `@font-face` pointing at the release's `fonts/` files (the call is synchronous, so nothing is fetched to inline). Options: `scale`, `background`, `dark`, `embedScene`, `padding`. Image elements export as placeholders until the element decodes images.
- `importLibrary(textOrUrl, { merge })` merges as `updateLibrary({ merge: true })` does (`mergeLibraryItems`) and resolves to the library's item count; `exportLibrary()` returns the library as `.excalidrawlib` text for the host to store. A URL (or a `#addLibrary=` link) must pass upstream's allow-list; the host fetches it through `library-fetch`, and an unhandled `library-fetch` rejects the import.
- `ui`: `full`, `compact`, `mobile` and `auto` show the desktop chrome: the toolbar, the main menu, the footer's zoom and history buttons, and the context menu a right-click on the canvas opens (upstream's canvas or element menu, the element under the pointer selected; its rows run the actions the element has ported: select all, delete, duplicate, group and ungroup, z-order, copy and cut, zen mode) (the compact and mobile layouts are Phase 7); `none` hides it and the context menu. `theme="system"` follows `prefers-color-scheme` when set.
- Keys go through upstream's `App.onKeyDown` on the element's container (upstream's default, `handleKeyboardGlobally` off), and the actions they name run: undo and redo (which lay bound text and bound arrows out again with the real layouts), the zoom keys, select all, delete, duplicate, group and ungroup, and the z-order keys. The pointer does what upstream's does: with the selection tool a press selects, a drag moves the selection with its bound text and arrows, a drag on empty canvas draws the selection box, and the handles resize and rotate; the drawing tools draw (an arrow binds its ends), the eraser erases, the text tool and a double-click edit text in upstream's textarea; the wheel, Space or the hand tool pan and Ctrl+wheel zooms. Copy, cut and paste use the document's clipboard events (upstream's JSON as `text/plain`); a paste lands at the pointer. The [parity checklist](@/plan/parity.md) lists each behaviour with its test. Pressing an element's link icon emits `open-link`.

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
