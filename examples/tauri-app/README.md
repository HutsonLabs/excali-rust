# Example Tauri app

`<excali-editor>` in a Tauri v2 window, with `tauri-plugin-excali` doing what the editor leaves to its host: the file dialogs, reading and writing files, headless PNG and SVG export and fetching libraries ([Tauri page](../../site/content/architecture/tauri.md)). No bundler and no npm: `ui/` is plain HTML and ES modules, and the web runtime is copied into `ui/excali/` before each build.

| Path | What |
|---|---|
| `ui/index.html`, `ui/app.js`, `ui/app.css` | The page: a toolbar (Open, Save, Save as, Export PNG, Export SVG, Import library; Ctrl/Cmd+O, S, Shift+S) and the editor |
| `ui/excali/` | The web runtime, `scripts/web/build.sh`'s release (ignored by git; `scripts/web-runtime.sh` writes it) |
| `src-tauri/` | The Rust app: `tauri.conf.json` (window `main`, CSP, bundle, updater), `capabilities/excali.json` (the plugin's, unchanged), `capabilities/app.json` (the app's own commands, in `main` only; `build.rs` generates their permissions), `src/lib.rs` |
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

### Signed and notarized (a release)

```sh
scripts/release/macos-dmg.sh 26.9.2 --dry-run          # print every step, run none
scripts/release/macos-dmg.sh 26.9.2                    # build, sign, notarize, stage
scripts/release/macos-dmg.sh 26.9.2 --upload v26.9.2   # and attach to the GitHub release
```

From the repository root, on an Apple silicon Mac, at the commit of the release (the version must be `tauri.conf.json`'s). The script, after term.hut's `scripts/release.sh` and `scripts/publish-release.sh`:

1. bundles the release's web runtime (`excali-web_<version>.tar.gz`, fetched and checked by `scripts/release/fetch.sh`), or `EXCALI_WEB_DIST` if set;
2. reads the App Store Connect API key variables from `EXCALI_RELEASE_ENV` (default `~/code/term.hut/.env`) and unlocks the dedicated signing keychain (`~/Library/Keychains/term-hut-signing.keychain-db`, `SIGNING_KEYCHAIN_PASSWORD`) if it is there;
3. checks the Developer ID identity (`APPLE_SIGNING_IDENTITY`, default `FE9B9ADD91CB67176BFE80FF725F77539D75BD96`) is listed by `security find-identity -v -p codesigning`, and passes it to the build with `--config`: the identity is never in `tauri.conf.json`, and the app needs no entitlements;
4. takes the updater's minisign private key from the keychain item `excali-example-updater-key` (`TAURI_SIGNING_PRIVATE_KEY`, empty password) and runs `CI=true cargo tauri build --bundles app,dmg`; the bundler notarizes the app;
5. verifies the signature (`codesign --verify --deep --strict`), notarizes and staples the image (`notarytool submit --wait`, `stapler staple`, `stapler validate`) and checks Gatekeeper accepts it (`spctl`);
6. stages in `target/macos-dmg/<version>/` (or `--out DIR`): `Excali.Example_<version>_aarch64.dmg`, `Excali.Example_<version>_aarch64.app.tar.gz` and its `.sig`, `latest.json` and their `SHA256SUMS` lines;
7. with `--upload v<version>`, merges `SHA256SUMS` with the release's (the web tarball's line stays) and runs `gh release upload v<version> --clobber`.

### Unsigned (development)

```sh
cd examples/tauri-app/src-tauri
cargo tauri build --bundles dmg --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

`bundle.createUpdaterArtifacts` is on in `tauri.conf.json`, so a plain `cargo tauri build` asks for the updater's private key (`TAURI_SIGNING_PRIVATE_KEY`); the override turns the updater tarball off for a build without it. `cargo tauri dev`, `cargo test` and `scripts/smoke.sh` (plain `cargo build`) do not need it. In a session without Finder (SSH, CI), prefix `CI=true`, which skips the window layout step of Tauri's `bundle_dmg.sh` that otherwise fails there. The image is `target/release/bundle/dmg/Excali Example_<version>_<arch>.dmg` (under `$CARGO_TARGET_DIR` when that is set). It is not signed or notarized: after copying the app to Applications, open it once with right-click, Open (or `xattr -dr com.apple.quarantine "/Applications/Excali Example.app"`).

## Updates

A release build checks for a newer version in the background once the editor is up: `ui/app.js` calls the app's `update_check` command, and the app, with `tauri-plugin-updater`, reads `latest.json` from the latest GitHub release (`plugins.updater.endpoints`). If it offers a newer version, a small prompt in the corner says "Update to <version>" with **Install and restart** and **Later**; Install and restart calls `update_install`, which downloads the `.app.tar.gz`, checks its signature against `plugins.updater.pubkey` (minisign key `A58BE9CA1F6D8AE2`), replaces the app and restarts it. The check and the download are native, so the page's CSP allows no new source and the window has no `updater:` permission; the app's four commands are allowed in `main` only (`capabilities/app.json`). `cargo tauri dev` (a debug build) and smoke mode never check, and a failed check (offline, no `latest.json` yet) goes to the console only. `latest.json` and the signed tarball are written by `scripts/release/macos-dmg.sh`.

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

`tests/app.rs` builds the app from its own context (the configuration and the capabilities compiled into it) and calls what `ui/app.js` calls through the IPC and the ACL of the `main` window: open, save, save as, both exports, a refused path and a refused window; the app's commands in `main` and nowhere else; the updater's configuration (public key, endpoint, `createUpdaterArtifacts`, no signing identity), and a check against a `latest.json` served on localhost, newer and not. `scripts/smoke.sh` builds the app as `cargo tauri build --no-bundle` does and runs it with `EXCALI_EXAMPLE_SMOKE=<dir>`, where the dialogs answer paths in `<dir>`: `ui/app.js` mounts the editor, opens, saves, saves as and exports through its own handlers and reports, and `scripts/check-smoke.py` checks the report (no CSP violation or error, the editor's elements, an update check that answers nothing) and the files written. CI runs both on macOS (`tauri-example` in `.github/workflows/rust.yml`).
