# Example Tauri app

`<excali-editor>` in a Tauri v2 window, with `tauri-plugin-excali` doing what the editor leaves to its host: the file dialogs, reading and writing files, headless PNG and SVG export and fetching libraries ([Tauri page](../../site/content/architecture/tauri.md)). No bundler and no npm: `ui/` is plain HTML and ES modules, and the web runtime is copied into `ui/excali/` before each build.

| Path | What |
|---|---|
| `ui/index.html`, `ui/app.js`, `ui/app.css` | The page: a toolbar (Open, Save, Save as, Export PNG, Export SVG, Import library; Ctrl/Cmd+O, S, Shift+S) and the editor |
| `ui/excali/` | The web runtime, `scripts/web/build.sh`'s release (ignored by git; `scripts/web-runtime.sh` writes it) |
| `src-tauri/` | The Rust app: `tauri.conf.json` (window `main`, CSP, bundle), `capabilities/excali.json` (the plugin's, unchanged), `src/lib.rs` |
| `src-tauri/icons/` | Drawn by the port: `scripts/icons.sh` renders `icon.excalidraw` with `excali render` and runs `cargo tauri icon` |
| `smoke/open.excalidraw` | The scene the smoke run opens (a rectangle, an ellipse and a text element from the chrome-export fixtures) |

## Run it

Once: the Tauri CLI, and `wasm-bindgen-cli` at the version `Cargo.lock` pins (the web build checks it and prints the install line).

```sh
cargo install tauri-cli --version "^2" --locked
cd examples/tauri-app/src-tauri
cargo tauri dev
```

`cargo tauri dev` runs `beforeDevCommand` (`sh scripts/web-runtime.sh`: `scripts/web/build.sh` into `ui/excali/`), builds the app and opens the window with the editor. To reuse an existing web build instead of rebuilding it, set `EXCALI_WEB_DIST` to its directory (for example `EXCALI_WEB_DIST=$PWD/../../../dist`).

## Build a macOS disk image

```sh
cd examples/tauri-app/src-tauri
cargo tauri build --bundles dmg
```

In a session without Finder (SSH, CI), prefix `CI=true`, which skips the window layout step of Tauri's `bundle_dmg.sh` that otherwise fails there. The image is `target/release/bundle/dmg/Excali Example_26.9.1_<arch>.dmg` (under `$CARGO_TARGET_DIR` when that is set). It is not signed or notarized: after copying the app to Applications, open it once with right-click, Open (or `xattr -dr com.apple.quarantine "/Applications/Excali Example.app"`). Pre-release images are attached to the GitHub releases tagged `v26.9.1-rc.N`.

## What the host does

- **Open** (`plugin:excali|open`): the native dialog with upstream's "Excalidraw files" filter; a `.png` or `.svg` with an embedded scene opens as its scene. The text goes to `editor.load`.
- **Save** (`plugin:excali|save`): `editor.save()` written back to the opened file (the open granted it), or where the save dialog says (`<name>.excalidraw`). Cmd+S inside the editor arrives as `save-request`.
- **Export** (`plugin:excali|export`): the scene exported natively by `excali-cli`'s code, no webview involved, to `<name>.png` or `<name>.svg`, in the editor's theme. The fonts come from the bundle's `fonts/` resource (`crates/excali-text/assets/fonts`).
- **Libraries**: Import library opens a `.excalidrawlib`; a library URL is fetched natively through `plugin:excali|library_fetch`, held to upstream's allow-list, from the editor's `library-fetch` event.

The CSP in `tauri.conf.json` allows scripts from the app only, plus `'wasm-unsafe-eval'`, without which WebKit refuses to compile the module ("Refused to create a WebAssembly object because 'unsafe-eval' or 'wasm-unsafe-eval' is not an allowed source of script", seen in the smoke run with it removed). The [integration guide](../../site/content/architecture/integration.md) quotes this app's CSP, capability and handlers, and `scripts/site/snippets.py` holds it to these files.

## Tests

```sh
cd examples/tauri-app/src-tauri
cargo test --locked          # tests/app.rs, on Tauri's mock runtime
../scripts/smoke.sh          # the real app in the platform webview
```

`tests/app.rs` builds the app from its own context (the configuration and the capabilities compiled into it) and calls what `ui/app.js` calls through the IPC and the ACL of the `main` window: open, save, save as, both exports, a refused path and a refused window. `scripts/smoke.sh` builds the app as `cargo tauri build --no-bundle` does and runs it with `EXCALI_EXAMPLE_SMOKE=<dir>`, where the dialogs answer paths in `<dir>`: `ui/app.js` mounts the editor, opens, saves, saves as and exports through its own handlers and reports, and `scripts/check-smoke.py` checks the report (no CSP violation or error, the editor's elements) and the files written. CI runs both on macOS (`tauri-example` in `.github/workflows/rust.yml`).
