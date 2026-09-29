+++
title = "Integration guide"
description = "Embedding <excali-editor> in a page or a Tauri v2 app: the files, loading the module without a bundler, the Content-Security-Policy, the plugin and its capability. Every snippet is checked against a host that runs it."
weight = 6
+++

This page is what a host needs to embed the editor: a plain web page, or a Tauri v2 app such as term.hut ([term.hut integration](../termhut-integration/)). The API of the element is on that page; the plugin's commands are on the [Tauri page](../tauri/).

Every code block below is a copy of a file that runs. `scripts/site/snippets.py` checks each block against the file named in the comment before it (the example Tauri app, `examples/tauri-app`, or the browser page of the web suite, `tests/web/page/csp.html`), and fails the gates job if a block and its file drift apart. The example app runs in WKWebView in CI (`tauri-example`: it opens, saves, saves as and exports with no CSP violation), and `tests/web/specs/csp.spec.mjs` serves the browser page in Chromium with the header this page gives.

## 1. The files

The web runtime is four entries, as `scripts/web/build.sh` builds them (or unpacked from a release's `excali-web_<version>.tar.gz`):

| File | What |
|---|---|
| `excali_editor.js` | One plain ES module: wasm-bindgen's `--target web` glue and the `<excali-editor>` custom element |
| `excali_editor_bg.wasm` | The module (about 1.0 MB gzipped) |
| `excali.css` | The element's stylesheet |
| `fonts/` | The range-split font files, their licences and `manifest.json` |

Keep them together in one directory: the module finds the wasm at `new URL('excali_editor_bg.wasm', import.meta.url)` and the fonts at `new URL("./fonts/", import.meta.url)`, so the directory can live at any path and nothing needs configuring. Serve the `.wasm` as `application/wasm`; any other type still works, but `WebAssembly.instantiateStreaming` fails and the glue falls back to the slower `WebAssembly.instantiate` with a console warning. `fonts/Xiaolai` (the CJK fallback, 12 MB of the 14 MB of fonts) may be left out: CJK text then falls back to a system font.

The example app copies the build into its frontend before each `cargo tauri dev` and `cargo tauri build` (`beforeDevCommand`, `beforeBuildCommand`):

<!-- snippet: examples/tauri-app/scripts/web-runtime.sh -->
```sh
"$root/scripts/web/build.sh" "$out"
```

## 2. Loading the module without a bundler

Link the stylesheet and load a module script from your page. `init()` fetches and compiles the wasm; `defineExcaliEditor()` registers the element; from then on `<excali-editor>` is an ordinary element.

<!-- snippet: tests/web/page/csp.html -->
```html
<link rel="stylesheet" href="/excali.css">
<script type="module" src="/csp-app.js"></script>
```

<!-- snippet: tests/web/page/csp-app.js -->
```js
import init, { defineExcaliEditor } from "/excali_editor.js";
// …
  await init();
// …
defineExcaliEditor();
const host = document.getElementById("host");
host.style.width = "1000px";
host.style.height = "700px";
const editor = document.createElement("excali-editor");
host.append(editor);
```

The element fills its parent, so give the parent a size. Import the module by a relative or absolute URL: there is no package name to resolve and no import map is needed. Keep the script in a file rather than inline in the page; with a CSP (below) an inline script needs its own hash or nonce.

The example app does the same from `ui/` with the runtime in `ui/excali/`:

<!-- snippet: examples/tauri-app/ui/index.html -->
```html
<link rel="stylesheet" href="./excali/excali.css">
<link rel="stylesheet" href="./app.css">
<!-- … -->
<script type="module" src="./app.js"></script>
```

<!-- snippet: examples/tauri-app/ui/app.js -->
```js
import init, { defineExcaliEditor } from "./excali/excali_editor.js";

const { invoke } = window.__TAURI__.core;
// …
  await init();
// …
defineExcaliEditor();
const editor = document.createElement("excali-editor");
editor.setAttribute("theme", matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
document.getElementById("host").append(editor);
```

`window.__TAURI__` exists because the app sets `"withGlobalTauri": true`; the editor itself never calls Tauri.

## 3. Content-Security-Policy

What the module needs from a policy:

| Directive | Source | Why |
|---|---|---|
| `script-src` | `'wasm-unsafe-eval'` | Compiling WebAssembly. Without it the module does not load: "If a page has a CSP header and `'wasm-unsafe-eval'` isn't specified in the `script-src` directive, WebAssembly is blocked from loading and executing on the page" (MDN, `script-src`, read 2026-09-29). WKWebView reports "Refused to create a WebAssembly object because 'unsafe-eval' or 'wasm-unsafe-eval' is not an allowed source of script" (example app, 2026-09-29), and `csp.spec.mjs` checks the same in Chromium. `'unsafe-eval'` is not needed and should not be added: it also allows `eval()`. |
| `style-src` | `'unsafe-inline'` | The editor's UI adds `<style>` elements and `style` attributes (`crates/excali-ui`, for example `toolbar/mod.rs`, `dom.rs`). Without it they are refused (`csp.spec.mjs`). |
| `connect-src` | `'self'` (the runtime's origin) | `init()` fetches the wasm with `fetch`. |
| `font-src` | `'self'` | The faces under `fonts/`, loaded by unicode range as text needs them. |
| `img-src` | `data:` | The built-in canvas images are `data:image/svg+xml` URLs, loaded when the canvas is created (`excali-canvas2d`, `builtin_image_src`); without it they are refused (`csp.spec.mjs`). |

The example app also allows `blob:` in `img-src` and `data:` in `font-src`; the editor does not need them today, and they do no harm.

### In a browser

Send it as a response header on the page:

<!-- snippet: web-csp examples/tauri-app/src-tauri/tauri.conf.json#/app/security/csp -->
```text
Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'
```

`tests/web/specs/csp.spec.mjs` reads this line from this page, serves `tests/web/page/csp.html` with it and checks, in Chromium, that the editor opens a scene with text, loads its font, saves and exports SVG and PNG with no violation; and that taking out `'wasm-unsafe-eval'`, `style-src`'s `'unsafe-inline'` or `img-src`'s `data:` breaks it, as does a nonce in `style-src` (see Tauri below). If the runtime is on another origin (a CDN), put that origin in `script-src`, `style-src`, `connect-src` and `font-src` in place of, or next to, `'self'`.

### In Tauri

The same policy as a `tauri.conf.json` object, plus Tauri's IPC in `connect-src`:

<!-- snippet: examples/tauri-app/src-tauri/tauri.conf.json#/app/security -->
```json
"security": {
  "csp": {
    "default-src": "'self'",
    "script-src": "'self' 'wasm-unsafe-eval'",
    "style-src": "'self' 'unsafe-inline'",
    "img-src": "'self' data: blob:",
    "font-src": "'self' data:",
    "connect-src": "'self' ipc: http://ipc.localhost"
  }
}
```

It goes under `app` in `tauri.conf.json`. Per platform, with `tauri` 2.12.0 (`scripts/core.js`, `convertFileSrc`, and `scripts/ipc-protocol.js`):

| Platform | Webview | Page origin (`'self'`) | IPC requests | Needs |
|---|---|---|---|---|
| macOS, iOS | WKWebView | `tauri://localhost` | `ipc://localhost/<command>` | `ipc:` |
| Linux | WebKitGTK | `tauri://localhost` | `ipc://localhost/<command>` | `ipc:` |
| Windows | WebView2 | `http://tauri.localhost` | `http://ipc.localhost/<command>` | `http://ipc.localhost` |
| Android | Android WebView | `http://tauri.localhost` | `window.ipc.postMessage` ("Android does not have support to reading the request body"); channel data from `http://ipc.localhost` | `http://ipc.localhost` |

Listing both `ipc:` and `http://ipc.localhost`, as above, covers every platform with one file. Leaving the IPC source out does not fail loudly: Tauri catches the refused request, warns "IPC custom protocol failed, Tauri will now use the postMessage interface instead" in the console and carries on over `postMessage`, which is slower for large payloads such as a PNG export. With `app.windows[].useHttpsScheme` set, Windows and Android use `https://` for both, so write `https://ipc.localhost` instead.

Tauri adds its own sources to this policy at build time: "Tauri appends its nonces and hashes to the relevant CSP attributes automatically to bundled code and assets" (Tauri, [Content Security Policy](https://v2.tauri.app/security/csp/), read 2026-09-29), and the same page says to "include `'wasm-unsafe-eval'` as a `script-src`" for WebAssembly. One consequence matters for the editor: Tauri gives every `<style>` element in your HTML files a nonce (`tauri-utils` 2.10.0 `html.rs`, `inject_nonce_token`), and once `style-src` holds a nonce the browser ignores its `'unsafe-inline'`, which refuses the editor's own styles. So keep the host page's styles in stylesheet files, as the example's `ui/app.css` is, and add no `<style>` element to the HTML. Inline `<script>` elements get a hash (`tauri-codegen` 2.7.0 `context.rs`, `inject_script_hashes`), which does no harm.

## 4. The plugin on the Rust side

A host with its own file commands (term.hut) needs nothing on the Rust side. Otherwise, add `tauri-plugin-excali` and `tauri-plugin-dialog`. The example app depends on the plugin by path:

<!-- snippet: examples/tauri-app/src-tauri/Cargo.toml -->
```toml
tauri = { version = "2.12.0", features = [] }
tauri-plugin-dialog = "2.8.0"
tauri-plugin-excali = { path = "../../../crates/tauri-plugin-excali" }
```

From another repository, the path becomes a git dependency, `tauri-plugin-excali = { git = "https://github.com/HutsonLabs/excali-rust", tag = "v26.9.1-rc.1" }` (the plugin is not on crates.io). Register the dialog plugin, then the excali plugin with the font directory headless exports read:

<!-- snippet: examples/tauri-app/src-tauri/src/lib.rs -->
```rust
    builder
        .plugin(tauri_plugin_dialog::init())
// …
        .setup(|app| {
            let resources = app.path().resource_dir().ok();
// …
            let mut excali =
                tauri_plugin_excali::Builder::new().fonts_dir(fonts_dir(resources.as_deref()));
// …
            app.handle().plugin(excali.build())?;
            Ok(())
        })
```

`fonts_dir` is the bundle's `fonts/` resource, which `tauri.conf.json` copies from the repository's font assets:

<!-- snippet: examples/tauri-app/src-tauri/tauri.conf.json#/bundle/resources -->
```json
"resources": {
  "../../../crates/excali-text/assets/fonts/": "fonts/"
}
```

`tauri_plugin_excali::init()` is the same plugin with the defaults (no font directory: exports fall back to system fonts; upstream's library allow-list).

## 5. Capabilities

The plugin's commands are allowed per window by a capability. Copy the plugin's file, unchanged, into the app's `src-tauri/capabilities/`:

<!-- snippet: crates/tauri-plugin-excali/capabilities/excali.json -->
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "excali",
  "description": "The excali plugin's commands in the window that embeds <excali-editor>",
  "windows": ["main"],
  "permissions": ["excali:default"]
}
```

It is the example app's file byte for byte:

<!-- snippet: examples/tauri-app/src-tauri/capabilities/excali.json -->
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "excali",
  "description": "The excali plugin's commands in the window that embeds <excali-editor>",
  "windows": ["main"],
  "permissions": ["excali:default"]
}
```

`windows` names the window labels that may call the commands (`app.windows[].label` in `tauri.conf.json`; the example's is `main`). `excali:default` allows all five commands (`open`, `save`, `export`, `library_fetch`, `validate`); to allow fewer, list `excali:allow-<command>` entries instead (with `-` for `_`: `excali:allow-library-fetch`). No `dialog:` permission is needed, since the plugin opens its dialogs from Rust, and no `fs:` permission, since it reads and writes the files itself. `tests/app.rs` in the example checks that the compiled capability allows the commands in `main` and refuses them in another window.

## 6. Calling it from the page

Open, save and export are `invoke` calls; a cancelled dialog resolves to `null`:

<!-- snippet: examples/tauri-app/ui/app.js -->
```js
async function openScene() {
  const file = await invoke("plugin:excali|open");
  if (!file) return null;
  await editor.load(file.text);
// …
async function saveScene({ as = false } = {}) {
  const path = await invoke("plugin:excali|save", {
    text: editor.save(),
    path: as ? null : (current?.path ?? null),
    name: current?.name ?? null,
  });
// …
async function exportScene(format) {
  const path = await invoke("plugin:excali|export", {
    scene: editor.save(),
    format,
    options: { darkMode: editor.getAttribute("theme") === "dark" },
    save: { name: current?.name ?? null },
  });
```

Library URLs (`#addLibrary` links) are fetched natively, held to the allow-list; the editor asks through its `library-fetch` event:

<!-- snippet: examples/tauri-app/ui/app.js -->
```js
editor.addEventListener("library-fetch", (e) => {
  e.preventDefault();
  e.detail.respond(invoke("plugin:excali|library_fetch", { url: e.detail.url }));
});
```

Cmd+S inside the editor arrives as `save-request`:

<!-- snippet: examples/tauri-app/ui/app.js -->
```js
editor.addEventListener("save-request", () => run("save"));
```

## Checking a change to this page

<!-- snippet: scripts/site/snippets.py -->
```sh
python3 scripts/site/snippets.py check
```

Run it from the repository root; `tests/web` (`npx playwright test specs/csp.spec.mjs`, after `scripts/web/build.sh`) serves the browser page with the header, and `examples/tauri-app/scripts/smoke.sh` runs the example app in the platform webview.
