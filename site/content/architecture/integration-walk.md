+++
title = "Integration guide, walked"
description = "The transcript of an agent following the integration guide step by step in a fresh clone in a temporary directory: what diverged, how the guide changed, and the clean second walk."
weight = 8
+++

The [integration guide](../integration/) was followed step by step, as written, by an agent (task `ex-803`, owner decision (e) of 2026-09-27), in a fresh clone of this repository in a temporary directory (`mktemp -d`, written `$W` below). It built two hosts that are not in the repository: a plain web page served with the guide's Content-Security-Policy header (with the runtime at the site root, then on another origin), and a new, minimal Tauri v2 app that depends on `tauri-plugin-excali` by git tag, as a host in another repository does. Then it ran the page's own checks.

The first walk, on `main` at `8f88d8e` (2026-09-29), diverged from the guide in the places below. Each was fixed in the guide, and each fix is held by a test, so the page cannot drift back. The second walk, a fresh clone of the fixed branch in a new temporary directory, went through with no divergence.

## What diverged and what changed

| Where | What the walk found | Change to the guide | Held by |
|---|---|---|---|
| 2, browser page | The HTML block had no element with `id="host"`, which the script looks up: `Cannot read properties of null (reading 'style')` and no editor. The example app's HTML block had the same gap. | Both HTML blocks show the `<main id="host">` their scripts use, and the text says the runtime is served at the site root | `test_every_element_a_script_looks_up_is_in_the_hosts_html` |
| 3, runtime on another origin | With the origin in the four directives as the guide said, Chromium refused the module: "blocked by CORS policy: No 'Access-Control-Allow-Origin' header is present on the requested resource". | The runtime's server must send `Access-Control-Allow-Origin`; why (module scripts, the wasm `fetch` and web fonts are CORS requests) | `csp.spec.mjs`: the runtime from a second origin with and without the header, under the policy built from the guide's sentence; `test_a_cross_origin_runtime_needs_cors` |
| 4, fonts resource | The bundle resource `../../../crates/excali-text/assets/fonts/` exists only inside this repository; in another app `tauri-build` fails: "resource path `../../../crates/excali-text/assets/fonts` doesn't exist". | A host elsewhere bundles the runtime's own `fonts/` (the same files; `diff -r` in the transcript): `"../ui/excali/fonts/": "fonts/"` | `test_a_host_outside_this_repository_bundles_the_runtimes_fonts` |
| 4, Rust registration | The block calls `app.path()`, which needs `tauri::Manager` in scope, and `fonts_dir`, the example's own function, which the page did not describe; the walker had to write it. | `use tauri::Manager;` and what `fonts_dir` returns | the guide's text (the block is still checked against the example) |
| 1, the files | The build reported 1,176,076 bytes gzip for the wasm; the guide said "about 1.0 MB". The page did not say where `build.sh` writes or what it needs. | The budget the build enforces (1.5 MB, from the phases page) and the measured size with its date; `OUT` defaults to `dist/`; the target, the pinned `wasm-bindgen-cli`, Python and binaryen; the release archive's layout and `SHA256SUMS` | `test_the_wasm_size_quoted_is_the_budget_the_build_enforces` |
| Checking a change | `npx playwright test` in `tests/web` needs `npm ci` and a Chromium first. | Both steps named | the guide's text |

What the walk did that is not in the guide, and is marked as such in the files: `window.editor = editor;` at the end of the page's script, for the check; a Playwright script (`check.mjs`) that loads the page, reports console errors, CSP violations and failed requests, and opens, saves and exports a scene; small Python servers that send the header (`serve.py`) and the CORS header (`cors.py`); and in the Tauri app, a `walk_report` command and a check at the end of `app.js` that exports PNG and SVG and validates through the plugin and prints the result. The Tauri app starts from a minimal skeleton (`Cargo.toml`, `build.rs`, `main.rs`, the example's icons), since the guide is for an existing app. On macOS, running the bundle's binary directly exited 0 with no output, so the bundle was started with `open -W`; that is LaunchServices, not the guide.

The Rust builds used a fresh `target/` in each clone in the second walk (`CARGO_TARGET_DIR` unset); the first walk's runtime build used a shared cache. Both were on macOS 27 (arm64) with Rust 1.97.1, `wasm-bindgen-cli` 0.2.129, tauri-cli 2.11.4 and Playwright 1.63.0's Chromium.

## Second walk: clean

`ex-803/integration-guide-walked-by` at `9b62268`, 2026-09-29 (the branch before its rebase onto a later `main`; the guide is the same at `290ca559`).

```text
# Walk 2 of the integration guide (ex-803), after the fixes, in $W

[21:27:35] $W
$ env | grep -c CARGO_TARGET_DIR; git clone -b ex-803/integration-guide-walked-by https://github.com/HutsonLabs/excali-rust fresh 2>&1 | tail -1; git -C fresh log --oneline -1
0
Cloning into 'fresh'...
9b62268 ex-803: integration guide fixed where the fresh-clone walk diverged
[exit 0]

[21:27:39] $W/fresh
$ ./scripts/web/build.sh 2>&1 | grep -v '^ *Compiling' | tail -6
binaryen: downloading https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-arm64-macos.tar.gz
binaryen: sha256 verified (arm64-macos ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8)
    Finished `web-release` profile [optimized] target(s) in 28.72s
web build: $W/fresh/dist (wasm-bindgen 0.2.129, wasm-opt version 133 (version_133))
excali_editor_bg.wasm: 1,176,076 bytes gzip (2,754,175 raw), budget 1,500,000 (78.4%) ok
excali_editor.js: 14,712 bytes gzip (82,213 raw), budget 20,000 (73.6%) ok
[exit 0]

# ---- Browser host (sections 1-3): the page and script from section 2, the runtime at the site root, the header from section 3. serve.py sends the header; check.mjs (Playwright, from the clone's tests/web) is the walker's check.

[21:28:20] $W
$ cp -R fresh/dist/. host/www/ && ls host/www; grep -m1 '^Content-Security-Policy:' fresh/site/content/architecture/integration.md > host/csp-header.txt; for f in host/www/index.html host/www/csp-app.js host/csp-header.txt host/serve.py host/cors.py host/check.mjs; do echo "--- $f"; cat $f; done
csp-app.js
excali_editor_bg.wasm
excali_editor.js
excali.css
fonts
index.html
--- host/www/index.html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>host</title>
<link rel="stylesheet" href="/excali.css">
<script type="module" src="/csp-app.js"></script>
</head>
<body>
<main id="host"></main>
</body>
</html>
--- host/www/csp-app.js
import init, { defineExcaliEditor } from "/excali_editor.js";
await init();
defineExcaliEditor();
const host = document.getElementById("host");
host.style.width = "1000px";
host.style.height = "700px";
const editor = document.createElement("excali-editor");
host.append(editor);
window.editor = editor;
--- host/csp-header.txt
Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'
--- host/serve.py
# Serves the host page and the runtime at "/" with the guide's CSP header.
import http.server, sys, os
CSP = open(sys.argv[1]).read().strip()
class H(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        name, value = CSP.split(": ", 1)
        self.send_header(name, value)
        super().end_headers()
os.chdir(sys.argv[2])
http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[3])), H).serve_forever()
--- host/cors.py
# static server that sends Access-Control-Allow-Origin: *
import http.server, sys, os
class H(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Access-Control-Allow-Origin', '*'); super().end_headers()
os.chdir(sys.argv[1]); http.server.ThreadingHTTPServer(('127.0.0.1', int(sys.argv[2])), H).serve_forever()
--- host/check.mjs
// The walker's check: load the host page in Chromium, report console errors,
// CSP violations, failed requests, then open a scene, save and export.
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
const require = createRequire(process.argv[2] + "/");
const { chromium } = require("@playwright/test");
const [, , , url, scenePath] = process.argv;
const browser = await chromium.launch();
const page = await browser.newPage();
const problems = [];
page.on("console", (m) => { if (m.type() === "error" || m.type() === "warning") problems.push(`console.${m.type()}: ${m.text()}`); });
page.on("pageerror", (e) => problems.push(`pageerror: ${e.message}`));
page.on("requestfailed", (r) => problems.push(`requestfailed: ${r.url()}`));
page.on("response", (r) => { if (r.status() >= 400) problems.push(`HTTP ${r.status()}: ${r.url()}`); });
await page.addInitScript(() => document.addEventListener("securitypolicyviolation", (e) => console.error(`CSP violation: ${e.violatedDirective} ${e.blockedURI || "inline"}`)));
const res = await page.goto(url);
console.log("content-security-policy:", res.headers()["content-security-policy"]);
try {
  await page.waitForFunction(() => window.editor?.querySelector("canvas"), null, { timeout: 15000 });
  const result = await page.evaluate(async (scene) => {
    const ed = window.editor;
    await ed.load(scene);
    await document.fonts.ready;
    const svg = ed.export("svg", { embedScene: true });
    const png = await ed.export("png", { scale: 1, background: true });
    return { state: ed.getState(), saved: JSON.parse(ed.save()).elements.length, svg: svg.includes("<svg"), png: `${png.type} ${png.size > 0}` };
  }, readFileSync(scenePath, "utf8"));
  const box = await page.evaluate(() => { const r = window.editor.getBoundingClientRect(); return `${r.width}x${r.height}`; });
  console.log("editor box:", box);
  console.log("result:", JSON.stringify(result));
} catch (e) {
  console.log("FAILED:", e.message.split("\n")[0]);
}
console.log(problems.length ? problems.join("\n") : "no console errors, violations or failed requests");
await browser.close();
[exit 0]

[21:28:21] $W/fresh/tests/web
$ npm ci --no-audit --no-fund 2>&1 | tail -1
added 3 packages in 2s
[exit 0]

[21:28:24] $W/host
$ node check.mjs $W/fresh/tests/web http://127.0.0.1:8741/index.html $W/fresh/examples/tauri-app/smoke/open.excalidraw
content-security-policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'
editor box: 1000x700
result: {"state":{"dirty":false,"elementCount":3,"zoom":1,"selectionCount":0,"activeTool":"selection"},"saved":3,"svg":true,"png":"image/png true"}
no console errors, violations or failed requests
[exit 0]

# ---- Runtime on another origin (section 3, 'In a browser'): the runtime at http://127.0.0.1:8742 from a server that sends Access-Control-Allow-Origin (cors.py), that origin in script-src, style-src, connect-src and font-src; the page at :8743.

[21:28:32] $W/host
$ head -8 cdnpage/index.html | tail -3; head -1 cdnpage/csp-app.js; cat csp-cdn.txt; node check.mjs $W/fresh/tests/web http://127.0.0.1:8743/index.html $W/fresh/examples/tauri-app/smoke/open.excalidraw
<link rel="stylesheet" href="http://127.0.0.1:8742/excali.css">
<script type="module" src="/csp-app.js"></script>
</head>
import init, { defineExcaliEditor } from "http://127.0.0.1:8742/excali_editor.js";
Content-Security-Policy: default-src 'self'; script-src 'self' http://127.0.0.1:8742 'wasm-unsafe-eval'; style-src 'self' http://127.0.0.1:8742 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' http://127.0.0.1:8742 data:; connect-src 'self' http://127.0.0.1:8742
content-security-policy: default-src 'self'; script-src 'self' http://127.0.0.1:8742 'wasm-unsafe-eval'; style-src 'self' http://127.0.0.1:8742 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' http://127.0.0.1:8742 data:; connect-src 'self' http://127.0.0.1:8742
editor box: 1000x700
result: {"state":{"dirty":false,"elementCount":3,"zoom":1,"selectionCount":0,"activeTool":"selection"},"saved":3,"svg":true,"png":"image/png true"}
no console errors, violations or failed requests
[exit 0]

[21:28:33] $W/rel
$ gh release download v26.9.1-rc.1 -R HutsonLabs/excali-rust -p 'excali-web_*' -p SHA256SUMS && shasum -a 256 -c SHA256SUMS --ignore-missing && tar tzf excali-web_26.9.1-rc.1.tar.gz | grep -v 'fonts/.*/' 
excali-web_26.9.1-rc.1.tar.gz: OK
excali_editor.js
excali_editor_bg.wasm
excali.css
fonts/
fonts/manifest.json
[exit 0]


# ---- Tauri host (sections 1-6): a new, minimal Tauri v2 app outside the repository ($W/tauri-host). The walker's own additions, not from the guide, are marked in the files: the walk_report command and the check at the end of app.js. The bundle resource is the path section 4 gives for a host outside the repository.

[21:28:43] $W/tauri-host
$ for f in src-tauri/Cargo.toml src-tauri/build.rs src-tauri/src/main.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json src-tauri/capabilities/excali.json ui/index.html ui/app.css ui/app.js; do echo "--- $f"; cat $f; done; ls ui/excali
--- src-tauri/Cargo.toml
# An existing minimal Tauri v2 app (the starting point; not from the guide),
# plus the guide's section 4 dependencies with the git form of the plugin.
[package]
name = "walk-host"
version = "0.1.0"
edition = "2021"

[lib]
name = "walk_host"
path = "src/lib.rs"

[dependencies]
tauri = { version = "2.12.0", features = [] }
serde_json = "1"
tauri-plugin-dialog = "2.8.0"
tauri-plugin-excali = { git = "https://github.com/HutsonLabs/excali-rust", tag = "v26.9.1-rc.1" }

[build-dependencies]
tauri-build = { version = "2.7.0", features = [] }

[profile.dev.package."*"]
debug = false
--- src-tauri/build.rs
fn main() { tauri_build::build(); }
--- src-tauri/src/main.rs
fn main() { walk_host::run(); }
--- src-tauri/src/lib.rs
// Section 4 of the guide: the dialog plugin, then the excali plugin with the
// font directory headless exports read (the bundle's fonts/ resource).
use tauri::Manager;

fn fonts_dir(resources: Option<&std::path::Path>) -> std::path::PathBuf {
    resources.map(|dir| dir.join("fonts")).unwrap_or_default()
}

/// The walker's check (not from the guide): prints the page's report and exits.
#[tauri::command]
fn walk_report(app: tauri::AppHandle, report: serde_json::Value) {
    println!("WALK_REPORT {report}");
    let ok = report.get("error").is_none()
        && report["problems"].as_array().is_some_and(|p| p.is_empty());
    app.exit(if ok { 0 } else { 1 });
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![walk_report])
        .setup(|app| {
            let resources = app.path().resource_dir().ok();
            let excali =
                tauri_plugin_excali::Builder::new().fonts_dir(fonts_dir(resources.as_deref()));
            app.handle().plugin(excali.build())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the app");
}
--- src-tauri/tauri.conf.json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Walk Host",
  "version": "0.1.0",
  "identifier": "com.example.walkhost",
  "build": { "frontendDist": "../ui" },
  "app": {
    "withGlobalTauri": true,
    "windows": [{ "label": "main", "title": "Walk Host", "width": 1200, "height": 800 }],
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
  },
  "bundle": {
    "active": true,
    "targets": ["app"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"],
    "resources": { "../ui/excali/fonts/": "fonts/" }
  }
}
--- src-tauri/capabilities/excali.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "excali",
  "description": "The excali plugin's commands in the window that embeds <excali-editor>",
  "windows": ["main"],
  "permissions": ["excali:default"]
}
--- ui/index.html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Walk Host</title>
<link rel="stylesheet" href="./excali/excali.css">
<link rel="stylesheet" href="./app.css">
</head>
<body>
<main id="host"></main>
<script type="module" src="./app.js"></script>
</body>
</html>
--- ui/app.css
html, body { margin: 0; height: 100%; } #host { position: fixed; inset: 0; }
--- ui/app.js
// Sections 2 and 6 of the guide, then the walker's check (not from the
// guide): load a scene, export both formats and validate through the plugin,
// and hand the result to the host's walk_report command.
import init, { defineExcaliEditor } from "./excali/excali_editor.js";

const { invoke } = window.__TAURI__.core;
const problems = [];
document.addEventListener("securitypolicyviolation", (e) => problems.push(`CSP ${e.violatedDirective}: ${e.blockedURI || "inline"}`));
window.addEventListener("error", (e) => problems.push(`error: ${e.message}`));
window.addEventListener("unhandledrejection", (e) => problems.push(`rejection: ${e.reason}`));

try {
  await init();
} catch (e) {
  await invoke("walk_report", { report: { problems, error: `init: ${e}` } });
}
defineExcaliEditor();
const editor = document.createElement("excali-editor");
editor.setAttribute("theme", matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
document.getElementById("host").append(editor);

editor.addEventListener("library-fetch", (e) => {
  e.preventDefault();
  e.detail.respond(invoke("plugin:excali|library_fetch", { url: e.detail.url }));
});

const report = { problems };
try {
  const text = await (await fetch("./walk.excalidraw")).text();
  await editor.load(text);
  await document.fonts.ready;
  report.state = editor.getState();
  const png = await invoke("plugin:excali|export", { scene: editor.save(), format: "png", options: { darkMode: false } });
  report.png = png.byteLength ?? png.length;
  const svg = await invoke("plugin:excali|export", { scene: editor.save(), format: "svg" });
  report.svg = { start: svg.slice(0, 5), fontFace: svg.includes("@font-face") };
  report.validate = await invoke("plugin:excali|validate", { text: editor.save() });
  report.fontsLoaded = [...document.fonts].filter((f) => f.status === "loaded").map((f) => f.family);
} catch (e) {
  report.error = String(e);
}
await new Promise((r) => setTimeout(r, 500));
await invoke("walk_report", { report });
excali_editor_bg.wasm
excali_editor.js
excali.css
fonts
[exit 0]

[21:28:43] $W/tauri-host/src-tauri
$ CI=true cargo tauri build --bundles app 2>&1 | grep -v '^ *\(Compiling\|Downloaded\|Downloading\|Locking\|Adding\|Updating\|Fetching\)' | tail -6; ls 'target/release/bundle/macos/Walk Host.app/Contents/Resources/fonts'
    Finished `release` profile [optimized] target(s) in 1m 10s
       Built application at: $W/tauri-host/src-tauri/target/release/walk-host
    Bundling Walk Host.app ($W/tauri-host/src-tauri/target/release/bundle/macos/Walk Host.app)
    Finished 1 bundle at:
        $W/tauri-host/src-tauri/target/release/bundle/macos/Walk Host.app

Assistant
Cascadia
ComicShanns
Excalifont
Liberation
Lilita
manifest.json
Nunito
Virgil
Xiaolai
[exit 0]

[21:29:57] $W/tauri-host/src-tauri
$ rm -f ob.log; perl -e 'alarm 90; exec @ARGV' open -W --stdout $PWD/ob.log --stderr $PWD/ob.log 'target/release/bundle/macos/Walk Host.app'; echo "open exit $?"; cat ob.log
open exit 0
WALK_REPORT {"problems":[],"state":{"dirty":false,"elementCount":3,"zoom":1,"selectionCount":0,"activeTool":"selection"},"png":154015,"svg":{"start":"<?xml","fontFace":true},"validate":{"kind":"scene","elements":3,"types":{"rectangle":1,"ellipse":1,"text":1},"texts":["original text"],"files":0},"fontsLoaded":["Excalifont"]}
[exit 0]

# ---- Checking a change to this page (last section)

[21:30:03] $W/fresh
$ python3 scripts/site/snippets.py check
snippets: site/content/architecture/integration.md ok
[exit 0]

[21:30:03] $W/fresh/tests/web
$ npx playwright install chromium 2>&1 | tail -2; npx playwright test specs/csp.spec.mjs 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | tail -11

  ✓  3 [chromium] › specs/csp.spec.mjs:66:1 › the guide's header is a policy: scripts from the origin and wasm only (14ms)
  ✓  4 [chromium] › specs/csp.spec.mjs:97:1 › without 'wasm-unsafe-eval' the module does not compile (282ms)
  ✓  1 [chromium] › specs/csp.spec.mjs:201:1 › from another origin that sends Access-Control-Allow-Origin, under the guide's policy for it, the editor works (358ms)
  ✓  2 [chromium] › specs/csp.spec.mjs:111:1 › a nonce in style-src turns 'unsafe-inline' off and refuses the editor's styles (371ms)
  ✓  5 [chromium] › specs/csp.spec.mjs:73:1 › under the guide's header the editor opens, renders text, saves and exports with no violation (374ms)
  ✓  7 [chromium] › specs/csp.spec.mjs:222:1 › from another origin without Access-Control-Allow-Origin the module is refused (149ms)
  ✓  6 [chromium] › specs/csp.spec.mjs:105:1 › without style-src 'unsafe-inline' the editor's own styles are refused (275ms)
  ✓  8 [chromium] › specs/csp.spec.mjs:121:1 › without img-src data: the canvas's built-in images are refused (251ms)

  8 passed (1.8s)
[exit 0]

[21:30:07] $W/fresh
$ examples/tauri-app/scripts/smoke.sh 2>&1 | grep -v '^ *\(Compiling\|Downloaded\|Downloading\|Locking\|Adding\|Updating\)' | tail -4
    Blocking waiting for file lock on package cache
    Blocking waiting for file lock on package cache
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 36.10s
smoke: ok (opened, saved, saved as, exported png 147816 bytes and svg 19935 bytes; 3 elements; no CSP violations)
[exit 0]
```

## First walk: the divergences

`main` at `8f88d8e`, 2026-09-29. Lines starting `# DIVERGENCE` are where the guide and reality parted, and what the walker did to go on.

```text
[21:13:48] $W
$ git clone https://github.com/HutsonLabs/excali-rust fresh 2>&1 | tail -3; git -C fresh log --oneline -1
Cloning into 'fresh'...
8f88d8e ex-705: close in tracker, build log
[exit 0]

[21:15:50] $W/fresh
$ ./scripts/web/build.sh 2>&1 | grep -v '^ *Compiling' | tail -15
binaryen: downloading https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-arm64-macos.tar.gz
binaryen: sha256 verified (arm64-macos ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8)
    Finished `web-release` profile [optimized] target(s) in 12.37s
web build: $W/fresh/dist (wasm-bindgen 0.2.129, wasm-opt version 133 (version_133))
excali_editor_bg.wasm: 1,176,076 bytes gzip (2,754,175 raw), budget 1,500,000 (78.4%) ok
excali_editor.js: 14,712 bytes gzip (82,213 raw), budget 20,000 (73.6%) ok
[exit 0]

[21:16:27] $W
$ mkdir -p host/www && cp -R fresh/dist/. host/www/ && ls host/www
excali_editor_bg.wasm
excali_editor.js
excali.css
fonts
[exit 0]

[21:16:27] $W
$ grep -m1 '^Content-Security-Policy:' fresh/site/content/architecture/integration.md > host/csp-header.txt; cat host/csp-header.txt
Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'
[exit 0]

[21:16:27] $W
$ mv host/index.html host/csp-app.js host/www/; cat host/www/index.html host/www/csp-app.js host/serve.py
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>host</title>
<link rel="stylesheet" href="/excali.css">
<script type="module" src="/csp-app.js"></script>
</head>
<body>
</body>
</html>
import init, { defineExcaliEditor } from "/excali_editor.js";
await init();
defineExcaliEditor();
const host = document.getElementById("host");
host.style.width = "1000px";
host.style.height = "700px";
const editor = document.createElement("excali-editor");
host.append(editor);
window.editor = editor;
# Serves the host page and the runtime at "/" with the guide's CSP header.
import http.server, sys, os
CSP = open(sys.argv[1]).read().strip()
class H(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        name, value = CSP.split(": ", 1)
        self.send_header(name, value)
        super().end_headers()
os.chdir(sys.argv[2])
http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[3])), H).serve_forever()
[exit 0]

[21:16:40] $W/fresh/tests/web
$ npm ci --no-audit --no-fund 2>&1 | tail -2

added 3 packages in 153ms
[exit 0]

[21:16:44] $W/host
$ node check.mjs $W/fresh/tests/web http://127.0.0.1:8731/index.html $W/fresh/examples/tauri-app/smoke/open.excalidraw
content-security-policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'
FAILED: page.waitForFunction: Timeout 15000ms exceeded.
pageerror: Cannot read properties of null (reading 'style')
[exit 0]

# DIVERGENCE 1: the guide's HTML block has no element with id="host", which its script queries (getElementById("host") -> null). Adding <main id="host"></main> to the body to continue.

[21:17:11] $W/host
$ cat www/index.html; node check.mjs $W/fresh/tests/web http://127.0.0.1:8731/index.html $W/fresh/examples/tauri-app/smoke/open.excalidraw
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>host</title>
<link rel="stylesheet" href="/excali.css">
<script type="module" src="/csp-app.js"></script>
</head>
<body>
<main id="host"></main>
</body>
</html>
content-security-policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'
editor box: 1000x700
result: {"state":{"dirty":false,"elementCount":3,"zoom":1,"selectionCount":0,"activeTool":"selection"},"saved":3,"svg":true,"png":"image/png true"}
no console errors, violations or failed requests
[exit 0]

# Checking 'If the runtime is on another origin (a CDN), put that origin in script-src, style-src, connect-src and font-src': runtime served from http://127.0.0.1:8732 by a plain static server, page from :8733.

[21:17:21] $W/host
$ cat csp-cdn.txt; node check.mjs $W/fresh/tests/web http://127.0.0.1:8733/index.html $W/fresh/examples/tauri-app/smoke/open.excalidraw
Content-Security-Policy: default-src 'self'; script-src 'self' http://127.0.0.1:8732 'wasm-unsafe-eval'; style-src 'self' http://127.0.0.1:8732 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' http://127.0.0.1:8732 data:; connect-src 'self' http://127.0.0.1:8732
content-security-policy: default-src 'self'; script-src 'self' http://127.0.0.1:8732 'wasm-unsafe-eval'; style-src 'self' http://127.0.0.1:8732 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' http://127.0.0.1:8732 data:; connect-src 'self' http://127.0.0.1:8732
FAILED: page.waitForFunction: Timeout 15000ms exceeded.
console.error: Access to script at 'http://127.0.0.1:8732/excali_editor.js' from origin 'http://127.0.0.1:8733' has been blocked by CORS policy: No 'Access-Control-Allow-Origin' header is present on the requested resource.
requestfailed: http://127.0.0.1:8732/excali_editor.js
console.error: Failed to load resource: net::ERR_FAILED
[exit 0]

# DIVERGENCE 2: a module script, its wasm fetch and fonts from another origin are CORS requests; the guide does not say the runtime's server must send Access-Control-Allow-Origin. Retrying with a server that sends it.

[21:17:43] $W/host
$ node check.mjs $W/fresh/tests/web http://127.0.0.1:8733/index.html $W/fresh/examples/tauri-app/smoke/open.excalidraw
content-security-policy: default-src 'self'; script-src 'self' http://127.0.0.1:8732 'wasm-unsafe-eval'; style-src 'self' http://127.0.0.1:8732 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' http://127.0.0.1:8732 data:; connect-src 'self' http://127.0.0.1:8732
editor box: 1000x700
result: {"state":{"dirty":false,"elementCount":3,"zoom":1,"selectionCount":0,"activeTool":"selection"},"saved":3,"svg":true,"png":"image/png true"}
no console errors, violations or failed requests
[exit 0]

[21:17:47] $W/rel
$ gh release download v26.9.1-rc.1 -R HutsonLabs/excali-rust -p 'excali-web_*' -p SHA256SUMS && shasum -a 256 -c SHA256SUMS --ignore-missing && tar tzf excali-web_26.9.1-rc.1.tar.gz | grep -v '^.*fonts/.*/' | head -20
excali-web_26.9.1-rc.1.tar.gz: OK
excali_editor.js
excali_editor_bg.wasm
excali.css
fonts/
fonts/manifest.json
[exit 0]


# ---- Tauri host: a new, minimal Tauri v2 app outside the repository ($W/tauri-host), built from sections 1-6 of the guide. The walker's own additions, not from the guide, are marked in the files: the walk_report command and the check at the end of app.js.

[21:18:51] $W/tauri-host
$ for f in src-tauri/Cargo.toml src-tauri/build.rs src-tauri/src/main.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json src-tauri/capabilities/excali.json ui/index.html ui/app.css ui/app.js; do echo "--- $f"; cat $f; done; ls ui/excali
--- src-tauri/Cargo.toml
# An existing minimal Tauri v2 app (the starting point; not from the guide),
# plus the guide's section 4 dependencies with the git form of the plugin.
[package]
name = "walk-host"
version = "0.1.0"
edition = "2021"

[lib]
name = "walk_host"
path = "src/lib.rs"

[dependencies]
tauri = { version = "2.12.0", features = [] }
serde_json = "1"
tauri-plugin-dialog = "2.8.0"
tauri-plugin-excali = { git = "https://github.com/HutsonLabs/excali-rust", tag = "v26.9.1-rc.1" }

[build-dependencies]
tauri-build = { version = "2.7.0", features = [] }

[profile.dev.package."*"]
debug = false
--- src-tauri/build.rs
fn main() { tauri_build::build(); }
--- src-tauri/src/main.rs
fn main() { walk_host::run(); }
--- src-tauri/src/lib.rs
// Section 4 of the guide: the dialog plugin, then the excali plugin with the
// font directory headless exports read (the bundle's fonts/ resource).
use tauri::Manager;

fn fonts_dir(resources: Option<&std::path::Path>) -> std::path::PathBuf {
    resources.map(|dir| dir.join("fonts")).unwrap_or_default()
}

/// The walker's check (not from the guide): prints the page's report and exits.
#[tauri::command]
fn walk_report(app: tauri::AppHandle, report: serde_json::Value) {
    println!("WALK_REPORT {report}");
    let ok = report.get("error").is_none()
        && report["problems"].as_array().is_some_and(|p| p.is_empty());
    app.exit(if ok { 0 } else { 1 });
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![walk_report])
        .setup(|app| {
            let resources = app.path().resource_dir().ok();
            let excali =
                tauri_plugin_excali::Builder::new().fonts_dir(fonts_dir(resources.as_deref()));
            app.handle().plugin(excali.build())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the app");
}
--- src-tauri/tauri.conf.json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Walk Host",
  "version": "0.1.0",
  "identifier": "com.example.walkhost",
  "build": { "frontendDist": "../ui" },
  "app": {
    "withGlobalTauri": true,
    "windows": [{ "label": "main", "title": "Walk Host", "width": 1200, "height": 800 }],
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
  },
  "bundle": {
    "active": true,
    "targets": ["app"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"],
    "resources": { "../../../crates/excali-text/assets/fonts/": "fonts/" }
  }
}
--- src-tauri/capabilities/excali.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "excali",
  "description": "The excali plugin's commands in the window that embeds <excali-editor>",
  "windows": ["main"],
  "permissions": ["excali:default"]
}
--- ui/index.html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Walk Host</title>
<link rel="stylesheet" href="./excali/excali.css">
<link rel="stylesheet" href="./app.css">
</head>
<body>
<main id="host"></main>
<script type="module" src="./app.js"></script>
</body>
</html>
--- ui/app.css
html, body { margin: 0; height: 100%; } #host { position: fixed; inset: 0; }
--- ui/app.js
// Sections 2 and 6 of the guide, then the walker's check (not from the
// guide): load a scene, export both formats and validate through the plugin,
// and hand the result to the host's walk_report command.
import init, { defineExcaliEditor } from "./excali/excali_editor.js";

const { invoke } = window.__TAURI__.core;
const problems = [];
document.addEventListener("securitypolicyviolation", (e) => problems.push(`CSP ${e.violatedDirective}: ${e.blockedURI || "inline"}`));
window.addEventListener("error", (e) => problems.push(`error: ${e.message}`));
window.addEventListener("unhandledrejection", (e) => problems.push(`rejection: ${e.reason}`));

try {
  await init();
} catch (e) {
  await invoke("walk_report", { report: { problems, error: `init: ${e}` } });
}
defineExcaliEditor();
const editor = document.createElement("excali-editor");
editor.setAttribute("theme", matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
document.getElementById("host").append(editor);

editor.addEventListener("library-fetch", (e) => {
  e.preventDefault();
  e.detail.respond(invoke("plugin:excali|library_fetch", { url: e.detail.url }));
});

const report = { problems };
try {
  const text = await (await fetch("./walk.excalidraw")).text();
  await editor.load(text);
  await document.fonts.ready;
  report.state = editor.getState();
  const png = await invoke("plugin:excali|export", { scene: editor.save(), format: "png", options: { darkMode: false } });
  report.png = png.byteLength ?? png.length;
  const svg = await invoke("plugin:excali|export", { scene: editor.save(), format: "svg" });
  report.svg = svg.slice(0, 40);
  report.validate = await invoke("plugin:excali|validate", { text: editor.save() });
  report.fontsLoaded = [...document.fonts].filter((f) => f.status === "loaded").map((f) => f.family);
} catch (e) {
  report.error = String(e);
}
await new Promise((r) => setTimeout(r, 500));
await invoke("walk_report", { report });
excali_editor_bg.wasm
excali_editor.js
excali.css
fonts
[exit 0]

[21:18:51] $W/tauri-host/src-tauri
$ CARGO_TARGET_DIR=$W/tauri-host/src-tauri/target cargo build 2>&1 | grep -v '^ *\(Compiling\|Downloaded\|Downloading\|Locking\|Adding\|Updating\|Fetching\)' | tail -20
error: failed to run custom build command for `walk-host v0.1.0 ($W/tauri-host/src-tauri)`

Caused by:
  process didn't exit successfully: `$W/tauri-host/src-tauri/target/debug/build/walk-host-d723c8bccac98122/build-script-build` (exit status: 1)
  --- stdout
  cargo:rerun-if-env-changed=TAURI_CONFIG
  cargo:rustc-check-cfg=cfg(desktop)
  cargo:rustc-cfg=desktop
  cargo:rustc-check-cfg=cfg(mobile)
  cargo:rerun-if-changed=$W/tauri-host/src-tauri/tauri.conf.json
  cargo:rustc-env=TAURI_ANDROID_PACKAGE_NAME_APP_NAME=walkhost
  cargo:rustc-env=TAURI_ANDROID_PACKAGE_NAME_PREFIX=com_example
  cargo:rustc-check-cfg=cfg(dev)
  cargo:rustc-cfg=dev
  cargo:PERMISSION_FILES_PATH=$W/tauri-host/src-tauri/target/debug/build/walk-host-a85f6bcdb0185f56/out/app-manifest/__app__-permission-files
  cargo:rerun-if-changed=$W/tauri-host/src-tauri/capabilities
  cargo:rerun-if-env-changed=REMOVE_UNUSED_COMMANDS
  cargo:rustc-env=TAURI_ENV_TARGET_TRIPLE=aarch64-apple-darwin
  resource path `../../../crates/excali-text/assets/fonts` doesn't exist
warning: build failed, waiting for other jobs to finish...
[exit 0]

# DIVERGENCE 3: section 4's bundle resource "../../../crates/excali-text/assets/fonts/" is the example's path inside this repository; a host elsewhere has no such directory and tauri-build fails. The web runtime's fonts/ holds the same files (scripts/web/build.sh copies that directory), so the host points the resource there: "../ui/excali/fonts/": "fonts/".

[21:19:26] $W
$ diff -r fresh/crates/excali-text/assets/fonts fresh/dist/fonts && echo identical
identical
[exit 0]

[21:19:26] $W/tauri-host/src-tauri
$ grep -A2 resources tauri.conf.json; CARGO_TARGET_DIR=$W/tauri-host/src-tauri/target cargo build 2>&1 | grep -v '^ *\(Compiling\|Downloaded\|Downloading\|Locking\|Adding\|Updating\|Fetching\)' | tail -20
    "resources": { "../ui/excali/fonts/": "fonts/" }
  }
}
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.39s
[exit 0]

[21:19:35] $W/tauri-host/src-tauri
$ timeout 120 ./target/debug/walk-host 2>&1 | tail -5; echo "app exit ${pipestatus[1]:-$?}"
$W/rec.sh: line 6: timeout: command not found
app exit 0
[exit 0]

[21:19:39] $W/tauri-host/src-tauri
$ perl -e 'alarm 120; exec @ARGV' ./target/debug/walk-host > run.log 2>&1; echo "app exit $?"; tail -5 run.log
app exit 0
WALK_REPORT {"problems":[],"state":{"dirty":false,"elementCount":3,"zoom":1,"selectionCount":0,"activeTool":"selection"},"png":154015,"svg":"<?xml version=\"1.0\" standalone=\"no\"?>\n<!","validate":{"kind":"scene","elements":3,"types":{"rectangle":1,"ellipse":1,"text":1},"texts":["original text"],"files":0},"fontsLoaded":["Excalifont"]}
[exit 0]

[21:19:48] $W/tauri-host/src-tauri
$ CARGO_TARGET_DIR=$W/tauri-host/src-tauri/target CI=true cargo tauri build --bundles app 2>&1 | grep -v '^ *\(Compiling\|Downloaded\)' | tail -6; ls 'target/release/bundle/macos/Walk Host.app/Contents/Resources/fonts' | head; perl -e 'alarm 120; exec @ARGV' 'target/release/bundle/macos/Walk Host.app/Contents/MacOS/walk-host' > run-bundle.log 2>&1; echo "app exit $?"; tail -3 run-bundle.log
    Finished `release` profile [optimized] target(s) in 1m 35s
       Built application at: $W/tauri-host/src-tauri/target/release/walk-host
    Bundling Walk Host.app ($W/tauri-host/src-tauri/target/release/bundle/macos/Walk Host.app)
    Finished 1 bundle at:
        $W/tauri-host/src-tauri/target/release/bundle/macos/Walk Host.app

Assistant
Cascadia
ComicShanns
Excalifont
Liberation
Lilita
manifest.json
Nunito
Virgil
Xiaolai
app exit 0
[exit 0]

# Executing the bundle's binary directly exits 0 with no output (LaunchServices); launching the bundle with open(1) instead:

[21:21:54] $W/tauri-host/src-tauri
$ rm -f ob.log; perl -e 'alarm 90; exec @ARGV' open -W --stdout $PWD/ob.log --stderr $PWD/ob.log 'target/release/bundle/macos/Walk Host.app'; echo "open exit $?"; cat ob.log
open exit 0
WALK_REPORT {"problems":[],"state":{"dirty":false,"elementCount":3,"zoom":1,"selectionCount":0,"activeTool":"selection"},"png":154015,"svg":"<?xml version=\"1.0\" standalone=\"no\"?>\n<!","validate":{"kind":"scene","elements":3,"types":{"rectangle":1,"ellipse":1,"text":1},"texts":["original text"],"files":0},"fontsLoaded":["Excalifont"]}
[exit 0]

[21:21:59] $W/fresh
$ python3 scripts/site/snippets.py check
snippets: site/content/architecture/integration.md ok
[exit 0]

[21:22:00] $W/fresh/tests/web
$ npx playwright test specs/csp.spec.mjs 2>&1 | tail -6
  ✓  5 [chromium] › specs/csp.spec.mjs:120:1 › without img-src data: the canvas's built-in images are refused (330ms)
  ✓  3 [chromium] › specs/csp.spec.mjs:104:1 › without style-src 'unsafe-inline' the editor's own styles are refused (353ms)
  ✓  2 [chromium] › specs/csp.spec.mjs:110:1 › a nonce in style-src turns 'unsafe-inline' off and refuses the editor's styles (355ms)
  ✓  1 [chromium] › specs/csp.spec.mjs:72:1 › under the guide's header the editor opens, renders text, saves and exports with no violation (385ms)

  6 passed (1.7s)
[exit 0]

[21:22:11] $W/fresh
$ EXCALI_WEB_DIST=dist CARGO_TARGET_DIR=$W/example-target examples/tauri-app/scripts/smoke.sh 2>&1 | grep -v '^ *\(Compiling\|Downloaded\|Downloading\|Locking\|Adding\|Updating\)' | tail -8
web runtime: $W/fresh/examples/tauri-app/ui/excali
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 27.52s
smoke: ok (opened, saved, saved as, exported png 147816 bytes and svg 19935 bytes; 3 elements; no CSP violations)
[exit 0]
```
