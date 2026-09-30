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
- From 26.9.1 the same directory can come from the GitHub release rather than a checkout (ex-802): `scripts/release/fetch.sh 26.9.1 "$stage/excali"` downloads `excali-web_26.9.1.tar.gz` and `SHA256SUMS`, checks the digest and unpacks, which is the `curl -fsSL` of `scripts/vendor-catppuccin-icons.sh` plus the checksum. The vendor script then drops `fonts/Xiaolai` and writes `LICENSE`, `SOURCE.json` and its own `SHA256SUMS` as it does now.
- `fonts/Xiaolai`, upstream's CJK fallback, is not vendored. It is 12 MB of the release's 14 MB of fonts, which would triple term.hut's dmg (`PRODUCT.md`: ~5.4 MB, "should stay in that class"). The font registry is compiled into the module, so a missing face only fails its own unicode range, and CJK text falls back to the webview's system font. The vendored directory is 3.0 MB.
- `preview.js` routes `excalidraw` to a `drawing` view (`excalidrawEditor.js`), which loads the module when the first drawing opens. `TEXT_BACKED` keeps the JSON toggle. The read-only `excalidrawView.js` canvas is what the `excalidrawReadOnly` setting shows (Settings ▸ Editor ▸ Read-only drawings, off by default), and it is also the fallback when the module fails to load. `save-request` and the pane header's Save button write `ed.save()` with `fs_write_text`.

The third landed in term.hut PR #88 (ex-602, merge `8ec4b28`, 2026-09-29), except "Import library", which is ex-604. The save flow below is what it does.

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
ed.addEventListener("open-request", e => { /* Cmd+O (Open in the main menu): e.preventDefault(), show the host's open dialog, then ed.load(text) */ });
ed.addEventListener("save-as-request", e => { /* Cmd+Shift+S (Save to... in the main menu): e.preventDefault(), show the host's save dialog for ed.save(); e.detail.name */ });
ed.addEventListener("open-link", e => { /* e.detail.href; host decides */ });
ed.addEventListener("library-fetch", e => { /* e.detail.url; host may e.preventDefault() and supply e.detail.respond(text) */ });
ed.addEventListener("library-publish", e => { /* the publish dialog's Submit: e.detail.library (.excalidrawlib text), e.detail.fields ([name, value] pairs); host may e.preventDefault() and call e.detail.respond(url) or e.detail.reject(error) */ });
```

The element is `excali_wasm::web::EditorCore` behind a small shim (`crates/excali-wasm/js/excali-editor.js`, appended to `excali_editor.js` by `scripts/web/build.sh`); the editor itself, without the DOM, is `excali_wasm::editor::Editor`, tested natively (`crates/excali-wasm/tests/editor.rs`) and in Chromium from the page above (`tests/web/specs/editor.spec.mjs`, ex-530). What each call does:

- `load(text)` restores the file as upstream's `loadFromBlob` does and resets the history; it rejects with one sentence ("The text is not valid JSON (…).", "The JSON is not an Excalidraw scene.", "The scene could not be restored (…)."), leaving the scene as it was.
- `save()` is `serializeAsJSON(elements, appState, files, "local")` with `source` the page's origin; the text it returns becomes the clean state, so `getState().dirty` and `change`'s `detail.dirty` say whether the scene differs from what was loaded or last saved.
- `export("png", …)` paints `exportToCanvas` into a canvas and adds the scene as upstream's `tEXt` chunk with `embedScene`; `export("svg", …)` is `exportToSvg` with each `@font-face` pointing at the release's `fonts/` files (the call is synchronous, so nothing is fetched to inline). Options: `scale`, `background`, `dark`, `embedScene`, `padding`. Image elements export as placeholders until the element decodes images.
- `importLibrary(textOrUrl, { merge })` merges as `updateLibrary({ merge: true })` does (`mergeLibraryItems`) and resolves to the library's item count; `exportLibrary()` returns the library as `.excalidrawlib` text for the host to store. A URL (or a `#addLibrary=` link) must pass upstream's allow-list; the host fetches it through `library-fetch`, and an unhandled `library-fetch` rejects the import.
- The library sidebar's header menu does what upstream's does (`LibraryMenuHeaderContent.tsx`): Open picks a `.excalidrawlib` with the browser's file input and merges it (`errors.importLibraryError` in `appState.errorMessage` when it is no library), Save to... downloads the selection or the whole library as `library.excalidrawlib`, Reset library and Remove ask first with upstream's `ConfirmDialog`, and Rename or publish opens `PublishLibrary`, whose fields are kept in `localStorage` (`publish-library-data`) as upstream keeps them. Its Submit is a cancelable `library-publish` event: the host sends the library and the fields to a library backend of its choosing and answers `respond(url)`, which shows the success dialog and marks the items published, or `reject(error)`, which is alerted; unhandled, the submission fails. The editor never posts to upstream's backend, and does not draw upstream's JPEG preview of the items.
- `ui`: `full`, `compact`, `mobile` and `auto` show the desktop chrome: the toolbar, the main menu, the footer's zoom and history buttons, and the context menu a right-click on the canvas opens (upstream's canvas or element menu, the element under the pointer selected; its rows run the actions the element has ported: select all, delete, duplicate, group and ungroup, z-order, copy and cut, zen mode) (the compact and mobile layouts are Phase 7); `none` hides it and the context menu. `theme="system"` follows `prefers-color-scheme` when set.
- Keys go through upstream's `App.onKeyDown` on the element's container (upstream's default, `handleKeyboardGlobally` off), and the actions they name run: undo and redo (which lay bound text and bound arrows out again with the real layouts), the zoom keys, select all, delete, duplicate, group and ungroup, and the z-order keys. The pointer does what upstream's does: with the selection tool a press selects, a drag moves the selection with its bound text and arrows, a drag on empty canvas draws the selection box, and the handles resize and rotate; the drawing tools draw (an arrow binds its ends), the eraser erases, the text tool and a double-click edit text in upstream's textarea; the wheel, Space or the hand tool pan and Ctrl+wheel zooms. Copy, cut and paste use the document's clipboard events (upstream's JSON as `text/plain`); a paste lands at the pointer. The [parity checklist](@/plan/parity.md) lists each behaviour with its test. Pressing an element's link icon emits `open-link`.
- Shift+Alt+C (and Copy to clipboard as PNG in the context menu) is upstream's `actionCopyAsPng`: the selection, or the whole canvas when nothing is selected (`prepareElementsForExport`), exported as `exportCanvas("clipboard")` draws it with the scene's `exportBackground`, `exportWithDarkMode` and `exportScale`, written to the clipboard as an `image/png` `ClipboardItem`, and `appState.toast` says what was copied; a failed write is upstream's `alerts.couldNotCopyToClipboard` in `appState.errorMessage`.
- Cmd+O (Ctrl+O elsewhere) and Cmd+Shift+S, and the main menu's Open and Save to..., are upstream's `actionLoadScene` and `actionSaveFileToDisk`, whose file dialogs belong to the host: the element fires the cancelable `open-request` (detail `{}`) and `save-as-request` (detail `{ name }`, `app.getName()`: the scene's `name` or `Untitled-<date>-<time>`). A host that handles one calls `preventDefault()`, shows its dialog and answers with `load(text)` or writes `save()`. Unhandled, the element does what upstream does in a browser without the File System Access API: Open picks a file with the browser's file input and loads its text as `load(text)` does, Save as downloads `save()` as `<name>.excalidraw`. A PNG or SVG with an embedded scene is not opened yet.

Host adapter rules:

- The editor never touches the file system, the network or the clipboard beyond the DOM Clipboard API. Every side effect is an event the host handles.
- `theme` maps to upstream's `.theme--dark` tokens. term.hut is dark-only, so it sets `theme="dark"` and may override `--color-primary*` and `--island-bg-color` with Catppuccin values inside the element's scope, as upstream's theming doc allows.
- Fonts are fetched relative to the module URL by unicode range, on demand.

## Save flow in term.hut

1. Editor emits `change` → tab shows the dirty dot as the code editor does. The edit autosaves after 800 ms, the delay term.hut's code editor and BPMN modeller use.
2. Cmd+S in the pane or `save-request` from the editor → `fs_write_text(path, ed.save(), conn)`. A clean drawing is not rewritten, as a clean text buffer isn't. Closing the tab, switching away, toggling to the JSON and switching workspace save first.
3. If the file changed on disk since load (term.hut's `fs_watch_files` already reports this), term.hut prompts exactly as it does for text files; on "reload", it calls `ed.load(newText)`. A clean drawing reloads without asking, and the echo of term.hut's own write within 2 s is ignored, as for text files. Every save also reads the file first and compares it with the text it was loaded or last saved as, so a change the watcher missed is the same prompt, never an overwrite.
4. On SSH workspaces nothing differs: `fs_write_text` takes the connection. A host's files are not watched, so there the save's own check is what raises the prompt.

In term.hut, `ui/src/drawingDoc.js` holds these rules for a drawing tab (tested without the DOM in `ui/test/drawingDoc.test.js`), and `ui/e2e/excalidraw.test.js` drives them in Chromium against the real element. Hosts should not call the element synchronously from inside its own events: `change` is dispatched from inside the wasm editor, and `getState()` there is a re-entrant borrow that aborts. Use `change`'s `detail.dirty` instead.

## CRUD in term.hut

| Operation | term.hut command | Editor call |
|---|---|---|
| Create | `fs_create(path)` then `fs_write_text` with an empty scene (`elements: []`, default `appState`) | `ed.load(emptyScene)` |
| Read | `fs_read_text(path)` | `ed.load(text)` |
| Update | `fs_write_text(path, text)` | `ed.save()` |
| Delete | `fs_trash(path)` with its refusal rules | none |
| Rename | `fs_rename` | none |

Create and Delete landed in term.hut PR #89 (ex-603, merge `ff0faff`, 2026-09-29). The file tree's menus offer New drawing beside New file and New folder. term.hut's `ui/src/newDrawing.js` adds `.excalidraw` to the typed name unless it is there, calls `fs_create`, which never clobbers (a taken name is its `exists:` refusal, shown in the inline row), and then `fs_write_text` of upstream's empty scene against the empty file's hash, so a file something wrote in between is a conflict, not overwritten. The scene is what `serializeAsJSON([], getDefaultAppState(), {}, "local")` writes (the `export: true` app state keys, `source` the page's origin, as `<excali-editor>` stamps its own saves), and the file opens clean in the editor. Delete is the tree's Delete for any file: the same confirmation and `fs_trash` behind its guard, whose refusal is shown and leaves the file and an open drawing untouched. Both calls carry the workspace's connection, so SSH workspaces work the same way.

## Library import in term.hut

This landed in term.hut#90 (ex-604).

- The drawing's pane header has an "Import library" button. It takes a libraries.excalidraw.com `#addLibrary=<url>&token=<id>` link, a library URL, or a `.excalidrawlib` file. The app picks the file with `tauri-plugin-dialog` and reads it with a local `fs_read_text`; a browser page uses its own file input.
- The editor checks a URL, and it does so before anything is fetched: `libraryUrl` resolves the `#addLibrary` link as upstream's `parseLibraryTokensFromUrl` does, then applies upstream's allow-list. Only a URL that passes becomes a `library-fetch`, which term.hut answers with `fetch`. A refused URL is shown in a toast and nothing is written.
- The personal library is `~/.term-hut/excalidraw/library.excalidrawlib` (v2) on the host that owns the workspace, so it follows the anchored-workspace model. Locally, `~` is `resolve_local_dir("~")`; on an SSH host, hut-server resolves `~/…` against the login home. The first import creates the directories and the file with `fs_create`, which never overwrites.
- An import runs in three steps:
  1. It loads the stored file into the editor with `importLibrary(text, { merge: false })`.
  2. It merges the input with `{ merge: true }`, which is `mergeLibraryItems`.
  3. It writes `exportLibrary()` through `fs_write_text` against the hash of what it read.

  If another window wrote the library in the meantime, the write is a conflict; the import reads the file again and merges once more instead of overwriting it. A drawing opens with the stored library loaded.

## Size budget in this host

term.hut ships a 5.4 MB dmg and states "Weight is a feature" (`PRODUCT.md:165`). The module budget on the [phases page](../../plan/phases/#budgets) is set against that number.
