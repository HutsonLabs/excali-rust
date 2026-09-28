+++
title = "Build log"
description = "What landed on main, newest first."
weight = 5
+++

Each entry records a task that merged to main: the date, the task id and title, what now works, and the pull request that landed it. The newest entries are at the top. The [progress page](@/plan/progress.md) has the full status of the task graph.

## 2026-09-28 · ex-004 · Golden generator: node script producing rough.js 4.6.4 path output for fixture elements

`tools/goldens/generate.mjs` runs upstream's own `ShapeCache.generateElementShape` from the pinned checkout under plain Node, with roughjs 4.6.4 and perfect-freehand 1.2.0 pinned to upstream's `yarn.lock` hashes. It writes 16 byte-stable files to `goldens/` (465 element shapes, raw rough.js primitives and fills, `Random.next` sequences, freehand strokes) with a sha256 manifest, and the goldens reproduce upstream's export snapshot paths exactly. The new `goldens` CI job runs the 28 tests and `generate.mjs --check` on every PR. PR: [#10](https://github.com/HutsonLabs/excali-rust/pull/10).

## 2026-09-28 · ex-006 · Playwright smoke test for the site and mockups

Every content page, every mockup and the 404 page now load in Chromium at 1440x900, 1024x768 and 390x844 from a local build served like GitHub Pages. Any console error, uncaught exception, failed request or HTTP error fails the run, and inventory checks keep the tested pages in step with the sitemap and the mockups index. The new `site-smoke` CI job runs the unit tests and the 137-test smoke suite on every PR (`cd tests/site && npm ci && npm run test:smoke` locally). PR: [#8](https://github.com/HutsonLabs/excali-rust/pull/8).

## 2026-09-28 · ex-003 · Fixture corpus with manifest (upstream test fixtures + 232 public libraries)

`fixtures/` now holds 249 pinned files. The 15 upstream files are byte-exact copies at 438d898: the test fixtures, the restore and reconcile tests, the restore snapshot, and the upstream LICENSE. The other 234 come from excalidraw-libraries at 297a349: `libraries.json`, its LICENSE, and all 232 catalogue libraries, stored gzipped. `fixtures/manifest.json` records each file's sha256, size and origin URL. `scripts/fixtures/corpus.py check` confirms the manifest matches disk, and the new `fixtures` CI job also checks the copies against the pinned upstream checkout and the live origin URLs. PR: [#7](https://github.com/HutsonLabs/excali-rust/pull/7).

## 2026-09-28 · ex-001 · Cargo workspace skeleton and CI (fmt, clippy -D warnings, test)

The Cargo workspace now holds the 14 crates named in the architecture overview, with a pinned stable toolchain and a recorded MSRV. `excali-core` writes JSON exactly as `JSON.stringify` does (numbers, lone surrogates, key order), and a round-trip test covers an empty scene. The new `rust` CI workflow runs fmt, clippy with `-D warnings`, the tests, the MSRV check, a wasm32 build, and a crate-graph gate. The gate checks the workspace against the overview page and rejects `std::fs` in the wasm-pure crates. PR: [#6](https://github.com/HutsonLabs/excali-rust/pull/6).

## 2026-09-28 · ex-007 · scripts/site/zola.sh works on macOS (bash 3.2, shasum) with the aarch64-apple-darwin digest pinned

`scripts/site/zola.sh` now runs under the `/bin/bash` 3.2 that ships with macOS and checks downloads with `sha256sum` or `shasum -a 256`. The v0.22.0 digests for all four unix targets are pinned. A download with no pinned digest, or with a digest that does not match, is refused and nothing is installed. The new `bootstrap-and-site` CI job runs the offline tests, a real SHA-verified download, `bootstrap.sh` and the site build under `/bin/bash` on both ubuntu and macos. PR: [#5](https://github.com/HutsonLabs/excali-rust/pull/5).

## 2026-09-27 · ex-002 · Upstream pin script: check out excalidraw at the pinned commit into .tools/upstream

`scripts/upstream/checkout.sh` checks out excalidraw at the commit pinned in `site/config.toml` (438d898). The checkout goes into the main clone's `.tools/upstream`, which every worktree shares. It verifies HEAD and is idempotent. It refuses dirty or foreign checkouts, and it will not use any other commit unless `PIN` is set explicitly. `bootstrap.sh` runs it, and the `upstream-pin` CI job runs both the offline test suite and a live checkout. PR: [#4](https://github.com/HutsonLabs/excali-rust/pull/4).

## 2026-09-27 · ex-005 · Enable GitHub Pages source = GitHub Actions and confirm first deploy

GitHub Pages now deploys from GitHub Actions, and the site is live at its configured `base_url`. A new `smoke` job runs after each deploy to main. It fails the workflow unless the home page returns HTTP 200 with the expected title. `scripts/site/zola.sh` now also works with the bash 3.2 that ships with macOS. PR: [#3](https://github.com/HutsonLabs/excali-rust/pull/3).
