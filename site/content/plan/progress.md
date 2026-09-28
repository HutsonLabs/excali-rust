+++
title = "Progress"
description = "The task graph as tracked in beads, rendered from .beads/issues.jsonl at build time."
weight = 4
+++

Generated 2026-09-28 13:53 UTC from `.beads/issues.jsonl`. Edit `plan/tasks.json` and run `scripts/tasks/seed.py` to change the graph; claim and close work with `bd`.

**136 issues** · 43 closed (32%) · 0 in progress · 0 blocked · 92 open

Status words: open (ready or waiting on a blocker), in progress (claimed), blocked, closed. A task is *ready* when every issue it depends on is closed.

## ex-e0 · Phase 0: Foundations

<span class="status open">open</span> 10/10 children closed

Site, gates, tracker, upstream pin, fixture corpus, golden generator, Cargo workspace and CI. Everything later phases depend on to be reproducible.

| id | title | type | P | status | ready | blocked by |
|---|---|---|---|---|---|---|
| `ex-001` | Cargo workspace skeleton and CI (fmt, clippy -D warnings, test) | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-002` | Upstream pin script: check out excalidraw at the pinned commit into .tools/upstream | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-003` | Fixture corpus with manifest (upstream test fixtures + 232 public libraries) | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-004` | Golden generator: node script producing rough.js 4.6.4 path output for fixture elements | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-005` | Enable GitHub Pages source = GitHub Actions and confirm first deploy | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-006` | Playwright smoke test for the site and mockups | task | P3 | <span class="status closed">closed</span> |  |  |
| `ex-007` | scripts/site/zola.sh works on macOS (bash 3.2, shasum) with the aarch64-apple-darwin digest pinned | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-008` | Record the owner decisions of 2026-09-27 (calendar versioning, agent-closed milestones, fonts, strictly a port) | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-g001` | Attribution gate self-test in CI: a planted attribution line must fail the gate | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-m0` | M0: site live, gates enforced, workspace green | milestone | P2 | <span class="status closed">closed</span> |  |  |

## ex-e1 · Phase 1: Core model and file format (excali-core)

<span class="status open">open</span> 16/19 children closed

Element types, serde with unknown-field preservation, restore/migration rules, AppState, fractional indexing, library formats, payload codecs, conformance corpus.

| id | title | type | P | status | ready | blocked by |
|---|---|---|---|---|---|---|
| `ex-101` | Element model: enums, structs and shared base fields | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-102` | Serde with unknown-field preservation and 2-space JSON output | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-103` | Restore: base normalisation rules | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-104` | Restore: per-type rules (text, freedraw, image, line/draw, arrow, stickynote, frame) | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-105` | Restore: scene-level repairs (ids, indices, frames, bound text, bindings, sticky notes) | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-106` | AppState: exported keys, defaults and restoreAppState legacy handling | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-107` | Fractional indexing port (base-62 keys, generateNKeysBetween, syncInvalidIndices) | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-108` | Library formats: v1 `library` and v2 `libraryItems`, restoreLibraryItems, merge | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-109` | Library import model: URL allow-list and #addLibrary token parsing | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-110` | Payload codec: byte-string encoding and zlib compression (encode/decode) | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-111` | PNG tEXt scene payload read/write | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-112` | SVG metadata scene payload read/write | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-113` | Clipboard JSON format (excalidraw/clipboard) parse and emit | task | P2 | <span class="status closed">closed</span> |  |  |
| `ex-114` | Corpus test: 232 catalogue libraries round-trip | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-115` | JSON Schema generation for .excalidraw and .excalidrawlib (schemars) | task | P3 | <span class="status closed">closed</span> |  |  |
| `ex-116` | Restore: legacy arrow binding migration (bindings without mode) through RestoreEnv::migrate_legacy_binding | task | P0 | <span class="status open">open</span> |  | `ex-507`, `ex-510` |
| `ex-117` | Typed element model: keep field values of a type the model has no form for (string strokeWidth) as upstream's restore does | task | P0 | <span class="status open">open</span> | yes |  |
| `ex-g101` | D1 conformance: upstream diagramFixture document round-trips through Document and restore against an upstream golden | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-m1` | M1: round trip of all fixtures and 232 catalogue libraries | milestone | P2 | <span class="status open">open</span> |  | `ex-116`, `ex-117` |

## ex-e2 · Phase 2: Geometry and sketch renderer

<span class="status open">open</span> 17/19 children closed

excali-math, excali-rough (rough.js 4.6.4 semantics), excali-freehand, the display list, and golden tests against upstream output.

| id | title | type | P | status | ready | blocked by |
|---|---|---|---|---|---|---|
| `ex-201` | excali-math: points, vectors, segments, angles, ranges, rectangles, polygons | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-202` | excali-math: cubic curves, Catmull-Rom approximation, length (Legendre-Gauss N=24), closest point | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-203` | excali-rough: Park-Miller RNG and core generator (line, rectangle, polygon, ellipse, curve, path) | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-204` | excali-rough: fill styles hachure, cross-hatch, zigzag, solid | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-205` | excali-rough: dashes, multi-stroke, curve fitting, preserveVertices | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-206` | Spike: evaluate the roughr crate (0.14.0) against the goldens | spike | P1 | <span class="status closed">closed</span> |  |  |
| `ex-207` | Option mapping: generateRoughOptions and adjustRoughness with all constants | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-208` | Shape construction: rectangle (adaptive radius path), diamond (rounded C corners), ellipse (curveFitting 1), iframe defaults | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-209` | Shape construction: line and arrow (sharp, curved, polygon), loop fill | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-210` | Elbow arrow path from fixed points (radius 16) and validation | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-211` | Elbow arrow routing: A* over the non-uniform grid | task | P2 | <span class="status open">open</span> | yes |  |
| `ex-212` | Arrowheads: all fourteen kinds with sizes, angles and roughness rules | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-213` | excali-freehand: perfect-freehand 1.2.0 port (variable width) | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-214` | excali-freehand: laser-pointer constant-width variant | task | P2 | <span class="status closed">closed</span> |  |  |
| `ex-215` | Outline to path string with quadratic midpoints and 2-decimal trimming | task | P1 | <span class="status closed">closed</span> |  |  |
| `ex-216` | Display list type: renderer-independent paths, fills, dashes, images, text runs, clips, opacity | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-217` | Golden harness in CI (cargo test feature `goldens`) | task | P0 | <span class="status closed">closed</span> |  |  |
| `ex-218` | Dark-mode colour filter maths (invert 93% hue-rotate 180deg) and reverse | task | P2 | <span class="status closed">closed</span> |  |  |
| `ex-m2` | M2: golden parity with rough.js for all shapes | milestone | P2 | <span class="status open">open</span> |  | `ex-211` |

## ex-e3 · Phase 3: Text and fonts (excali-text)

<span class="status open">open</span> 0/9 children closed

Font metadata, measurement from font files, wrapping port, bound-text sizing, lazy font assets with verified licences.

| id | title | type | P | status | ready | blocked by |
|---|---|---|---|---|---|---|
| `ex-301` | Font metadata table and vertical offset formula | task | P0 | <span class="status open">open</span> | yes |  |
| `ex-302` | Advance-width measurement from font files (ttf-parser, rustybuzz where shaping matters) | task | P0 | <span class="status open">open</span> |  | `ex-301`, `ex-306` |
| `ex-303` | Wrapping port (textWrapping.ts) with upstream tests | task | P0 | <span class="status open">open</span> |  | `ex-302` |
| `ex-304` | Bound-text sizing: padding 5, ellipse and diamond insets, arrow label width | task | P1 | <span class="status open">open</span> |  | `ex-303` |
| `ex-305` | Text element sizing on edit: autoResize, originalText/text, anchor growth by align | task | P1 | <span class="status open">open</span> |  | `ex-303` |
| `ex-306` | Verify and record the licence of every font family before vendoring | decision | P0 | <span class="status open">open</span> | yes |  |
| `ex-307` | Font asset pipeline: range-split woff2 manifest and lazy loading | task | P1 | <span class="status open">open</span> |  | `ex-306` |
| `ex-308` | Corpus test: stored vs measured text widths across fixtures and libraries | task | P1 | <span class="status open">open</span> |  | `ex-302` |
| `ex-m3` | M3: text measurement and wrapping parity | milestone | P2 | <span class="status open">open</span> |  | `ex-301`, `ex-302`, `ex-303`, `ex-304`, `ex-305`, `ex-306` … |

## ex-e4 · Phase 4: Headless rendering and export

<span class="status open">open</span> 0/11 children closed

tiny-skia raster backend, SVG writer, PNG/SVG payload embedding, CLI.

| id | title | type | P | status | ready | blocked by |
|---|---|---|---|---|---|---|
| `ex-401` | excali-raster: display list to tiny-skia (paths, fills, dashes, opacity, clips) | task | P0 | <span class="status open">open</span> | yes |  |
| `ex-402` | Static scene assembly: background, grid, element order, bound text after container, iframes last | task | P0 | <span class="status open">open</span> |  | `ex-401` |
| `ex-403` | Frames: clipping with radius 8/zoom, stroke #bbb, names as Helvetica text on export | task | P1 | <span class="status open">open</span> |  | `ex-402` |
| `ex-404` | Image elements: decode data URLs (image crate), crop, scale flip, rounded clip, placeholder | task | P1 | <span class="status open">open</span> |  | `ex-401` |
| `ex-405` | PNG export: padding 10, scale, background, embedded payload | task | P0 | <span class="status open">open</span> |  | `ex-402` |
| `ex-406` | SVG writer: document structure (source comment, metadata, defs clipPaths, font style block, background rect) | task | P0 | <span class="status open">open</span> | yes |  |
| `ex-407` | SVG elements: rough paths, freedraw, text per line, images as symbol/use, arrow-label masks | task | P0 | <span class="status open">open</span> |  | `ex-403`, `ex-406` |
| `ex-408` | Spike: font subsetting for SVG export (allsorts, hb-subset, or ship full woff2) | spike | P2 | <span class="status open">open</span> |  | `ex-307` |
| `ex-409` | excali-cli: validate, render (png), export (svg), lib (list/merge), with exit codes | task | P1 | <span class="status open">open</span> |  | `ex-405`, `ex-406` |
| `ex-410` | Corpus render: every catalogue library item renders to PNG without panic | task | P1 | <span class="status open">open</span> |  | `ex-409` |
| `ex-m4` | M4: SVG/PNG export parity | milestone | P2 | <span class="status open">open</span> |  | `ex-401`, `ex-402`, `ex-403`, `ex-404`, `ex-405`, `ex-406` … |

## ex-e5 · Phase 5: Web runtime and editor

<span class="status open">open</span> 0/33 children closed

Canvas2D backend, interaction state machine, history, DOM chrome without a framework, keyboard shortcuts, the <excali-editor> custom element.

| id | title | type | P | status | ready | blocked by |
|---|---|---|---|---|---|---|
| `ex-501` | WASM build pipeline: wasm-bindgen --target web, wasm-opt, size check in CI | task | P0 | <span class="status open">open</span> | yes |  |
| `ex-502` | excali-canvas2d: display list to CanvasRenderingContext2D via web-sys | task | P0 | <span class="status open">open</span> |  | `ex-501` |
| `ex-503` | Layered canvases at device-pixel scale (static, new-element, interactive) and scroll snapping | task | P0 | <span class="status open">open</span> |  | `ex-502` |
| `ex-504` | Per-element bitmap cache with padding rules, size caps and pixel snapping | task | P1 | <span class="status open">open</span> |  | `ex-503` |
| `ex-505` | Viewport: zoom limits, wheel formula, scroll, coordinate transforms, zoom-to-fit | task | P0 | <span class="status open">open</span> |  | `ex-503` |
| `ex-506` | Editor state machine: tools registry (keys, fillable, toggle), active tool, tool lock, pen mode | task | P0 | <span class="status open">open</span> | yes |  |
| `ex-507` | Hit testing (collision.ts): thresholds, inside/outline rules, per-shape intersections | task | P0 | <span class="status open">open</span> | yes |  |
| `ex-508` | Selection and transform handles: sizes by pointer type, resize, rotate, aspect lock, centre resize | task | P0 | <span class="status open">open</span> |  | `ex-507` |
| `ex-509` | Snapping: point and gap snaps at 8/zoom, snap lines rendering | task | P1 | <span class="status open">open</span> |  | `ex-508` |
| `ex-510` | Arrow binding: gap 5+sw/2, max distance 15..30 by zoom, fixed points, modes inside/orbit/skip, highlight | task | P0 | <span class="status open">open</span> |  | `ex-508` |
| `ex-511` | Linear element editor: point handles (size 10), midpoints, segment length rule, label position | task | P1 | <span class="status open">open</span> |  | `ex-509` |
| `ex-512` | Text editing overlay: textarea with dir=auto wrap=off, transform formula, 5% height buffer | task | P0 | <span class="status open">open</span> |  | `ex-305`, `ex-503` |
| `ex-513` | History: store snapshots, element and appState deltas, undo/redo stacks, version bumps | task | P0 | <span class="status open">open</span> | yes |  |
| `ex-514` | Actions registry as data: the 99 action names with predicates and key tests | task | P0 | <span class="status open">open</span> |  | `ex-506` |
| `ex-515` | Keyboard handling: App.onKeyDown table, arrow nudges, tool letters, modifiers | task | P0 | <span class="status open">open</span> |  | `ex-514` |
| `ex-516` | excali-ui DOM builder and primitives: Island, Stack, Button, ToolIcon, RadioGroup, Range, TextField, Popover, Dialog, Tooltip | task | P0 | <span class="status open">open</span> |  | `ex-501` |
| `ex-517` | Icons module generated from upstream icons.tsx (MIT) with tabler 24/20 presets | task | P1 | <span class="status open">open</span> | yes |  |
| `ex-518` | Shapes toolbar (desktop order) and extra-tools dropdown | task | P0 | <span class="status open">open</span> |  | `ex-506`, `ex-516`, `ex-517` |
| `ex-519` | Styles panel, full mode: all sixteen groups with visibility predicates | task | P0 | <span class="status open">open</span> |  | `ex-514`, `ex-516` |
| `ex-520` | Main menu with default items and preferences submenu | task | P1 | <span class="status open">open</span> |  | `ex-516` |
| `ex-521` | Footer: zoom actions, undo/redo, help button, exit zen | task | P1 | <span class="status open">open</span> |  | `ex-505`, `ex-516` |
| `ex-522` | Help dialog with the three shortcut islands | task | P2 | <span class="status open">open</span> |  | `ex-516` |
| `ex-523` | Colour picker: top picks, palette 5x3, shades, hex input, eyedropper, keyboard map | task | P1 | <span class="status open">open</span> |  | `ex-516` |
| `ex-524` | Font picker: three top picks, scene/available groups, deprecated badge, search | task | P2 | <span class="status open">open</span> |  | `ex-301`, `ex-516` |
| `ex-525` | Context menus (canvas and element) generated from the actions registry | task | P1 | <span class="status open">open</span> |  | `ex-514`, `ex-516` |
| `ex-526` | Library sidebar: tabs, header menu, personal/excalidraw sections, drag to canvas, add to library | task | P1 | <span class="status open">open</span> |  | `ex-516` |
| `ex-527` | Command palette with category order and item lists | task | P2 | <span class="status open">open</span> |  | `ex-514`, `ex-516` |
| `ex-528` | Hints, tooltips, cursor hints, welcome screen | task | P2 | <span class="status open">open</span> |  | `ex-516` |
| `ex-529` | Stats panel (general and element properties) | task | P3 | <span class="status open">open</span> |  | `ex-516` |
| `ex-530` | <excali-editor> custom element: load/save/export/importLibrary/getState, events, host adapter | task | P0 | <span class="status open">open</span> |  | `ex-501`, `ex-513`, `ex-518`, `ex-519` |
| `ex-531` | Playwright parity suite against the checklist | task | P0 | <span class="status open">open</span> |  | `ex-530` |
| `ex-532` | Theme tokens: light and dark CSS custom properties embedded and overridable by the host | task | P0 | <span class="status open">open</span> |  | `ex-516` |
| `ex-m5` | M5: editor usable in a browser | milestone | P2 | <span class="status open">open</span> |  | `ex-501`, `ex-502`, `ex-503`, `ex-504`, `ex-505`, `ex-506` … |

## ex-e6 · Phase 6: Host integration (term.hut and Tauri)

<span class="status open">open</span> 0/8 children closed

Vendored module in term.hut with CRUD and library import; tauri-plugin-excali with dialogs, headless export and allow-listed fetch; example app.

| id | title | type | P | status | ready | blocked by |
|---|---|---|---|---|---|---|
| `ex-601` | term.hut: vendor the module and route .excalidraw to the editor in preview.js | task | P0 | <span class="status open">open</span> |  | `ex-530` |
| `ex-602` | term.hut: save through fs_write_text with dirty state and conflict handling | task | P0 | <span class="status open">open</span> |  | `ex-601` |
| `ex-603` | term.hut: New drawing and delete flows in the tree | task | P1 | <span class="status open">open</span> |  | `ex-602` |
| `ex-604` | term.hut: Import library from URL or file with the allow-list | task | P1 | <span class="status open">open</span> |  | `ex-601` |
| `ex-605` | tauri-plugin-excali: dialogs, headless export, allow-listed library fetch, capability file | task | P0 | <span class="status open">open</span> |  | `ex-409`, `ex-530` |
| `ex-606` | Example Tauri app embedding the editor | task | P1 | <span class="status open">open</span> |  | `ex-605` |
| `ex-607` | Integration docs: CSP, capabilities, module loading without a bundler | task | P2 | <span class="status open">open</span> |  | `ex-605` |
| `ex-m6` | M6: term.hut CRUD and library import end to end | milestone | P2 | <span class="status open">open</span> |  | `ex-601`, `ex-602`, `ex-603`, `ex-604`, `ex-605`, `ex-606` … |

## ex-e7 · Phase 7: Parity and polish

<span class="status open">open</span> 0/13 children closed

Tablet and phone layouts, remaining tools, accessibility, performance budgets, locale loader, parity checklist to 100%.

| id | title | type | P | status | ready | blocked by |
|---|---|---|---|---|---|---|
| `ex-701` | Compact styles panel and tablet form factor | task | P1 | <span class="status open">open</span> |  | `ex-519` |
| `ex-702` | Phone layout: mobile menu, bottom bar, mobile toolbar order | task | P1 | <span class="status open">open</span> |  | `ex-701` |
| `ex-703` | Sticky notes: element, rendering (shadow, edge, footer), label fitting | task | P2 | <span class="status open">open</span> |  | `ex-304`, `ex-402` |
| `ex-704` | Bucket fill tool | task | P2 | <span class="status open">open</span> |  | `ex-507` |
| `ex-705` | Lasso selection | task | P2 | <span class="status open">open</span> |  | `ex-507` |
| `ex-706` | Autoshape (draw-shape) recognition | task | P3 | <span class="status open">open</span> | yes |  |
| `ex-707` | Image crop editor | task | P2 | <span class="status open">open</span> |  | `ex-404`, `ex-508` |
| `ex-708` | Search sidebar (frames and texts) | task | P3 | <span class="status open">open</span> |  | `ex-526` |
| `ex-709` | Accessibility: focus order, ARIA on controls, reduced motion, RTL mirroring of icons | task | P2 | <span class="status open">open</span> |  | `ex-518` |
| `ex-710` | Performance budgets in CI (pan at 1,000 elements, first paint) | task | P1 | <span class="status open">open</span> |  | `ex-504` |
| `ex-711` | Locale loader using upstream JSON files (58 locales, 633 keys) | task | P3 | <span class="status open">open</span> |  | `ex-516` |
| `ex-712` | Parity checklist to 100% for the v1 scope | task | P0 | <span class="status open">open</span> |  | `ex-531` |
| `ex-m7` | M7: parity checklist 100% for v1 scope | milestone | P2 | <span class="status open">open</span> |  | `ex-701`, `ex-702`, `ex-703`, `ex-704`, `ex-705`, `ex-706` … |

## ex-e8 · Phase 8: Release

<span class="status open">open</span> 0/5 children closed

First calendar release v26.9.1 (ADR-009): ES module + WASM tarball as a GitHub release asset, integration guide walked by an agent in a fresh clone, tag and gh release. crates.io publishing (ex-801) is deferred by the owner.

| id | title | type | P | status | ready | blocked by |
|---|---|---|---|---|---|---|
| `ex-801` | Publish crates to crates.io under excali-* | task | P1 | <span class="status deferred">deferred</span> |  | `ex-712` |
| `ex-802` | Release tarball of the ES module and WASM as a GitHub release asset | task | P1 | <span class="status open">open</span> |  | `ex-712` |
| `ex-803` | Integration guide walked by an agent in a fresh clone | task | P2 | <span class="status open">open</span> |  | `ex-607` |
| `ex-804` | Tag v26.9.1 and publish the GitHub release | task | P2 | <span class="status open">open</span> |  | `ex-802`, `ex-803` |
| `ex-m8` | M8: v26.9.1 released on GitHub | milestone | P2 | <span class="status open">open</span> |  | `ex-802`, `ex-803`, `ex-804` |
