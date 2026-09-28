+++
title = "Evidence"
description = "Every source consulted while producing this site, what it established, and how it was obtained. Dates are when the source was read."
weight = 7
+++

Method: every claim on this site traces to one of the rows below. Upstream source was read from a shallow clone at a pinned commit and inventoried file by file by three research passes ([Research](../research/)). Web sources were fetched on the date shown; where a page could not be reached from the build container, that is recorded rather than guessed around. Command outputs were captured in this session and are reproducible with the scripts in the repository.

## Upstream Excalidraw

| Source | Established | Obtained |
|---|---|---|
| `github.com/excalidraw/excalidraw` at `438d89861f53d8a90ad566113ecac1b83761098f` (committed 2026-09-27) | Everything in the three research pages: element types, formats, restore rules, rendering constants, UI tokens, actions, shortcuts | `git clone --depth 1`, 2026-09-28 |
| `README.md` (raw, master) | "Excalidraw is released under the MIT license"; describes itself as "an open source virtual hand-drawn style whiteboard"; exports a ".excalidraw json file" | fetched 2026-09-28 |
| `packages/excalidraw/package.json:3` and `CHANGELOG.md:14` | Local version 0.18.0 with an "Unreleased" section; the analysed tree is ahead of the last release | research pass |
| npm registry `@excalidraw/excalidraw` | latest published 0.18.1, "Excalidraw as a React component" | `curl registry.npmjs.org`, 2026-09-28 |
| `github.com/excalidraw/excalidraw-libraries` `libraries.json` | 232 public libraries: 71 with `version: 1`, 161 with `version: 2`; index fields `name, description, authors[], source, preview, created, updated, version, id` | fetched and counted with `jq`, 2026-09-28 |
| `libraries/jumpingrivers/r.excalidrawlib` | v1 shape: top-level `library` array of element arrays; elements carry legacy `strokeSharpness` and `boundElementIds` | fetched, `jq keys`, 2026-09-28 |
| `libraries/youritjang/stick-figures.excalidrawlib` | v2 shape: `libraryItems[]` with `id, status, created, name, elements` | fetched, `jq`, 2026-09-28 |
| `excalidraw-libraries/README.md` | Contribution flow (group, add via library menu key `9`, publish); "at least 3 items" guideline | fetched 2026-09-28; the README does not document the index format or the `#addLibrary` scheme (those come from upstream source) |

## Rough.js and perfect-freehand

| Source | Established | Obtained |
|---|---|---|
| `github.com/rough-stuff/rough` `master`: `package.json`, `src/math.ts`, `src/generator.ts` | version 4.6.6 on master; `Random.next()` is `((2**31-1) & (seed = Math.imul(48271, seed))) / 2**31` with `Math.random()` fallback for seed 0; `defaultOptions` block as quoted on the [rendering fidelity page](../architecture/rendering-fidelity/) | fetched raw, 2026-09-28 (`main` branch returned 404; `master` is the default) |
| `packages/excalidraw/package.json:114, :106` | upstream pins `roughjs 4.6.4` and `perfect-freehand 1.2.0` | research pass |
| `crates.io/api/v1/crates/roughr` | 0.14.0, updated 2026-09-24, 36,825 downloads; `rough_tiny_skia` 0.14.0, `rough_vello` 0.16.0, `points_on_curve` 0.7.0, `svg_path_ops` 0.12.0 from the same workspace | `curl` JSON, 2026-09-28 |
| `orhanbalci/rough-rs` README | Workspace crates and adapters; `OptionsBuilder` fields; "roughr generates the sketchy shapes; an adapter crate draws them"; no statement of rough.js version tracked | fetched 2026-09-28 |
| `roughr` 0.14.0 crate sources as Cargo resolves them for `tools/roughr-eval` (`src/core.rs` `Options`, `Options::random`; `src/renderer.rs` `_line`, `_compute_ellipse_points`, `_arc`, `clone_options_alter_seed`, `solid_fill_polygon`; `src/generator.rs` `path`, `curve`; `src/filler/scan_line_hachure.rs`) against roughjs 4.6.4 `bin/` (`generator.js`, `renderer.js`, `math.js`, `fillers/scan-line-hachure.js` on `hachure-fill` 0.5.2) | `StdRng` seeded by `seed_from_u64`, draws narrowed to f32; f32 options and constants (`f32::PI()` loop bounds); older scan-line hachure; extra `move` per SVG `M`; fill before stroke in `path`; curve second stroke not reseeded; `u64` seeds. Replayed on all 1346 rough.js goldens (with ex-205's `rough-strokes.json`): 101 match at two decimals (7.5 %), 501 (37.2 %) with `park-miller.patch`; ADR-003 accepted as port | `tools/roughr-eval` (`cargo test`, `fork.py --check`), 2026-09-28 |
| `crates.io/api/v1/crates/perfect-freehand` | 0.1.1, updated 2026-01-27, 948 downloads, MIT, "based on and improves upon previous Rust ports" | `curl` JSON and README fetch, 2026-09-28 |
| `static.crates.io/crates/perfect_freehand/perfect_freehand-0.1.1.crate` (`src/get_stroke_points.rs`, `src/get_stroke_outline_points.rs`, `src/vec.rs`, `src/types.rs`) | Not bit-compatible with 1.2.0: a first point without pressure gets 0.5 (1.2.0: 0.25), cap loops use `i / 13` where 1.2.0 accumulates `t += 1/13`, lengths are `powi(2).sqrt()` rather than V8's `Math.hypot`, and it adds a `closed` option 1.2.0 lacks. ex-213 ports 1.2.0's `dist/esm/index.js` into `excali-freehand` instead (ADR-008 allows that crate no external dependencies) | crate tarball read, 2026-09-28 |

## term.hut (first host)

| Source | Established | Obtained |
|---|---|---|
| `github.com/HutsonLabs/term.hut` at `e60cd8bbbcd9b79bacb153c0973e27dadd42e0f9` | Tauri v2 shell, Rust PTY backend, xterm.js frontend; `ui/` served as `frontendDist` with `withGlobalTauri`; vendored roughjs and perfect-freehand; `excalidrawScene.js` (260 lines) and `excalidrawView.js` (442 lines, "Read-only by design"); `preview.js` routes `excalidraw` to a canvas viewer; commands `fs_read_text`/`fs_write_text` (6 call sites each), `fs_create`, `fs_rename`, `fs_trash`, `fs_watch_files`; `hut-core/src/fs.rs` API | private clone, 2026-09-28 |
| `PRODUCT.md:106-110, 165` | "No build step, no framework … no `node_modules` … vendored under `ui/vendor/` … dmg is ~5.4 MB"; "Catppuccin Mocha is the binding palette"; "Weight is a feature" | read 2026-09-28 |
| `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json` | workspace with `hut-core`, `hut-proto`, `hut-server`, `hut-client`; `tauri = "2"` with `tauri-plugin-dialog`, `-opener`, `-updater`, `-window-state`, `-single-instance`; capability style | read 2026-09-28 |
| `docs/plans/agentic-os.md`, `docs/mockups/agentic-os/*` | House conventions: self-contained plan prompts, one PR per epic, static HTML mockups with a shared `shell.css` and an index of scaled iframes | read 2026-09-28 |
| `github.com/HutsonLabs/termhut.hutsonlabs.com` at `7a99c78e6da7d2c180264d9fa5a64a8e63e118a9` | `design/tokens/*.css` (Catppuccin Mocha palette, semantic aliases, spacing, typography) reused for this site's chrome; `.githooks/commit-msg` strips attribution trailers; site is static on Cloudflare (`wrangler.jsonc`), not Zola | public clone, 2026-09-28 |

## Rust and web toolchain (crates.io, 2026-09-28)

| Crate | Version | Updated |
|---|---|---|
| tauri | 2.12.0 | 2026-09-26 |
| tauri-build | 2.7.0 | 2026-09-26 |
| wry / tao | 0.57.0 / 0.37.1 | 2026-09 |
| tauri-plugin-fs / tauri-plugin-dialog | 2.6.0 / 2.8.0 | 2026-09-26 |
| wasm-bindgen / js-sys / web-sys | 0.2.129 / 0.3.106 / 0.3.106 | 2026-09-25 |
| wasm-pack / trunk | 0.15.0 / 0.21.14 | 2026 |
| leptos / dioxus / yew / sycamore | 0.8.21 / 0.7.10 / 0.23.0 / 0.9.3 | 2026 |
| egui / iced / slint / vello / wgpu | 0.36.2 / 0.14.0 / 1.18.1 / 0.10.0 / 30.0.1 | 2025–2026 |
| tiny-skia / resvg / usvg | 0.12.0 / 0.48.1 / 0.48.1 | 2026 |
| kurbo / peniko / lyon | 0.13.1 / 0.6.1 / 1.0.19 | 2026 |
| ttf-parser / rustybuzz / swash / fontdue / cosmic-text / parley | 0.25.1 / 0.20.1 / 0.2.10 / 0.9.4 / 0.19.0 / 0.11.1 | 2024–2026 |
| serde_json / flate2 / base64 / png / image / nanoid / schemars | 1.0.151 / 1.1.10 / 0.23.1 / 0.18.1 / 0.25.10 / 0.5.0 / 1.2.2 | 2026 |
| fractional_index | 2.0.2 | 2024-09-17 |
| loro / automerge / yrs (for a future collaboration phase) | 1.16.2 / 0.12.0 / 0.28.0 | 2026-09 |

Local toolchain in the build container: `cargo 1.94.1`, `rustc 1.94.1` (2026-03-25), `node v22.22.2`, `python3 3.11.15`.

Web-framework framing (secondary sources, read 2026-09-28): the Tauri README quoted above; Leptos README ("Easy to use with Trunk … or with a simple wasm-bindgen setup"); Dioxus README ("Render directly to the DOM using WebAssembly"; desktop "using Webview"); a Tauri discussion thread titled "Is there plans to integrate Dioxus and Tauri?" exists (`tauri-apps/tauri` discussion 9169), which is itself evidence that the two are not integrated by default. Two comparison articles found by search (rustify.rs, pistack.xyz) were not used: one was unreachable from the container and neither is a primary source.

## Browser behaviour

| Source | Established | Obtained |
|---|---|---|
| `packages/element/src/renderElement.ts:504-507, 637-640` and `packages/common/src/colors.ts:86-89` at the pinned commit | Element colours reach `fillStyle`/`strokeStyle` as stored; `applyDarkModeFilter` returns the string unchanged unless the theme is dark, so the browser's CSS parser decides what a colour paints | read 2026-09-28 |
| Google Chrome 153.0.8010.53, headless (`--headless=new --dump-dom`) on macOS arm64 | 475 colour strings through a canvas `fillStyle` and `fillRect`: which assignments are ignored, the 8-bit RGBA painted, the serialised style. Recorded in `crates/excali-scene/tests/fixtures/css-colors.json` by `scripts/fixtures/css-color-goldens.sh`; `display/css_color.rs` reproduces every case except `color-mix()`, relative colours and `round()`, which are listed as gaps. Chrome clips out-of-gamut colours to sRGB, applies ProPhoto's transfer as a pure 1.8 power, rejects escapes in keywords and comments outside functions, and closes an unterminated function at the end of the string | run 2026-09-28 |
| Google Chrome 153.0.8010.53, headless (`--headless=new --disable-gpu --force-color-profile=srgb --dump-dom`) on macOS arm64 | What a canvas paints for the 20 raster fixture display lists (`crates/excali-raster/tests/fixtures/chrome/`, `scripts/fixtures/raster-references.sh`); the port's tiny-skia backend is within 5 levels of every pixel. Measured along the way: Chrome fills with analytic anti-aliasing (a shallow edge ramps 16 levels per pixel), fills a lone `arc()` path of a whole turn as an oval starting at angle 0, and draws `rect()` with an infinite size as nothing | run 2026-09-28 |
| Skia `chrome/m153` at `f8b66b7597c4cc859d3ed190e9c6872241e6721c` (committed 2026-09-16): `src/core/SkScan_AAAPath.cpp`, `SkAnalyticEdge.cpp`, `SkEdgeBuilder.cpp`, `SkEdgeClipper.cpp`, `SkLineClipper.cpp`, `SkGeometry.cpp`, `SkPathBuilder.cpp`, `SkPathPriv.cpp`, `SkPathRawShapes.cpp`, `SkStroke.cpp`, `SkStrokerPriv.cpp`, `SkContourMeasure.cpp`, `SkScan_Antihair.cpp`, `SkScan_AntiPath.cpp`, `SkBlitter.cpp`, `SkDraw.cpp`, `SkAAClip.cpp`, `SkRasterClip.cpp`; `src/utils/SkDashPath.cpp`; `src/effects/SkDashPathEffect.cpp` | The scan converter, edge builder, clipper, stroker, dasher and blitters `excali-raster` ports (`aaa.rs`, `edges.rs`, `stroke.rs`, `dash.rs`); `SkScan::AntiFillPath` always takes the analytic path | fetched raw from GitHub, 2026-09-28 |
| Chromium `main`: `third_party/blink/renderer/modules/canvas/canvas2d/canvas_path.cc`, `canvas_2d_recorder_context.cc`, `platform/geometry/path_builder.cc`, `platform/geometry/skia_geometry_utils.h`, `skia/config/SkUserConfig.h` | Blink's `arc()` in `f32` (`CanonicalizeAngle`, `AdjustEndAngle`, `AddEllipse` splitting a whole turn in two), the `drawArc` fast path for a path that is one arc, `ShouldDrawImageAntialiased`; Chromium does not define `SK_RASTERIZE_EVEN_ROUNDING` | fetched raw from GitHub, 2026-09-28; `main` rather than the 153 branch, so the Chrome references above are what the port is held to |
| CSS Color Module Level 4 (`w3.org/TR/css-color-4`) | Syntax of the colour functions and the conversion matrices of its sample code; not fetched from the build machine, so every conversion is checked against the Chrome goldens above instead | from the specification text as known, checked 2026-09-28 |

## Task tracking

| Source | Established | Obtained |
|---|---|---|
| `github.com/steveyegge/beads` README (mirrored at `gastownhall/beads`) | "Distributed graph issue tracker for AI agents, powered by Dolt"; hash-based ids; `bd ready`, `bd update --claim`, `bd dep add`; `.beads/issues.jsonl` "is an export … not the source of truth or a backup"; `bd setup claude` installs hooks (not used here) gate:allow-mention | fetched 2026-09-28 |
| `npm install -g @beads/bd` | `bd version 1.3.0 (f45b249ce)` | run 2026-09-28 |
| Scratch-repo exercise | `bd init --prefix ex`, `bd create --id`, `bd update --parent`, `bd dep add`, `bd import` of hand-written JSONL with `parent-child` and `blocks` dependencies (upsert, idempotent on re-run), `bd export`, `bd ready`, `bd dep cycles`, `bd metrics off`; `--id` and `--parent` cannot be combined on `create`; `bd init` writes `sync.remote` pointing at the git origin (removed here) | run 2026-09-28 |
| `bd hooks --help` | "prepare-commit-msg: Add agent identity trailers for forensics" | run 2026-09-28; reason the hooks are not installed |

## Site and deployment

| Source | Established | Obtained |
|---|---|---|
| `getzola/github-pages` README and `action.yml` (master) | Inputs `zola_version` (required), `working_directory`, `output_dir`, `build_flags`, `check_links`, `check_flags`; example pins `zola_version: v0.22.0` and `actions/deploy-pages@v4`; "GitHub Pages must be configured to use GitHub Actions" | fetched 2026-09-28 (`main` returned 404) |
| `getzola/zola` release `v0.22.0`, x86_64 linux gnu tarball | `zola 0.22.0`; SHA-256 `f1d491f8956b94384c27d75cb6b2bf60d3916d1ade9564bcbfe7c03f0258aebf` | downloaded via GitHub release redirect, 2026-09-28 |
| `getzola/zola` `docs/.../configuration.md` (master) | 0.22 moved highlighting under `[markdown.highlighting]`; the pre-0.22 `highlight_code` key is rejected | fetched after a build error, 2026-09-28 |
| `actions/deploy-pages` README | `actions/deploy-pages@v4`; permissions `pages: write`, `id-token: write`; environment `github-pages` | fetched 2026-09-28 |
| `actions/upload-pages-artifact` README | `@v3` in the example; inputs `path`, `name`, `retention-days`, `include-hidden-files` | fetched 2026-09-28 |
| `actions/checkout` README | v7 current; also used by term.hut's workflows | fetched 2026-09-28 |
| `shalzz/zola-deploy-action` | Alternative that pushes a `gh-pages` branch; not chosen | search result 2026-09-28 |

## Unreachable from the build container

`getzola.org`, `docs.rs`, `v2.tauri.app`, `api.github.com` (direct), `pistack.xyz`. Each fact those pages would have provided was taken from the corresponding GitHub repository's raw files or from crates.io instead, as listed above.
