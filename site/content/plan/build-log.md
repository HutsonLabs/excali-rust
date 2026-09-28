+++
title = "Build log"
description = "What landed on main, newest first."
weight = 5
+++

Each entry records a task that merged to main: the date, the task id and title, what now works, and the pull request that landed it. The newest entries are at the top. The [progress page](@/plan/progress.md) has the full status of the task graph.

## 2026-09-28 · ex-m0 · Milestone check: M0 reached

The M0 acceptance check was rerun end to end on deddc2d and all four criteria pass. **Pages:** the pages workflow for deddc2d built, deployed and passed its post-deploy smoke ([run](https://github.com/HutsonLabs/excali-rust/actions/runs/36388405351)), and the site returns HTTP 200. **Bootstrap:** `scripts/bootstrap.sh` under `/bin/bash` 3.2 in a fresh clone on macOS arm64 completed, and the `bootstrap-and-site` job passed on ubuntu and macOS. **Attribution:** the `attribution` job of the gates run for deddc2d ran the 11 planted-violation cases of `scripts/gates/test_attribution.py`, and every one was rejected with exit 1 while the clean control passed ([run](https://github.com/HutsonLabs/excali-rust/actions/runs/36388405346)). **Workspace:** fmt, clippy `-D warnings` and `cargo test --workspace --locked` pass (50 tests, 0 failed, 0 ignored), and so do the workspace, version and wasm32 gates ([run](https://github.com/HutsonLabs/excali-rust/actions/runs/36388405357)). The Playwright smoke suite (140), the corpus checks and the goldens `--check` are green. No gap tasks. PR: see the ex-m0 milestone PR.

## 2026-09-28 · ex-g001 · Attribution gate self-test in CI: a planted attribution line must fail the gate

`scripts/gates/test_attribution.py` plants violations one at a time in a scratch git repository outside this one (a tool-attribution line, a Co-Authored-By trailer, zero-width characters in a file and in a message, a non-human author and committer, and a violation below a clean tip) and asserts the attribution gate exits 1 with the matching rule id, while a clean control exits 0. The `attribution` job in the gates workflow runs it on every PR and on `main`; the first run was green ([gates run](https://github.com/HutsonLabs/excali-rust/actions/runs/36387970984)). PR: [#14](https://github.com/HutsonLabs/excali-rust/pull/14).

## 2026-09-28 · ex-m0 · Milestone check: M0 not yet reached

The M0 acceptance check ran end to end on 25602c2. **Pages:** the pages workflow for 25602c2 succeeded, and the github-pages deployment for that SHA serves the home, phases, progress and architecture pages with HTTP 200. **Bootstrap:** `scripts/bootstrap.sh` under `/bin/bash` 3.2 in a fresh clone (macOS arm64) installed hooks, seeded the tracker, downloaded Zola with its SHA-256 verified, checked out upstream at 438d898 and passed the gate self-check. **Workspace:** fmt, clippy `-D warnings` and `cargo test --workspace --locked` pass (50 tests, 0 failed, 0 ignored), and so do the workspace, version and wasm32 gates. The Playwright smoke suite (140), the plan and script tests, the corpus check, `verify-upstream` and the goldens `--check` are also green, and every CI workflow on main is green. **Attribution:** `attribution.py` rejects a planted line locally. On a tracked file it exits 1 (R2, R4), and on a commit with a Co-Authored-By trailer it exits 1 (R1, R3, R4). The commit-msg hook strips such a trailer. But no CI job plants a violation, so the requirement that CI rejects a planted line is not shown. A partial pass is a fail. Gap task `ex-g001` adds a gate self-test to the gates workflow; M0 closes once it is green. PR: see the ex-m0 milestone PR.

## 2026-09-28 · ex-008 · Record the owner decisions of 2026-09-27 (calendar versioning, agent-closed milestones, fonts, strictly a port)

The workspace is versioned 26.9.1 under ADR-009 (calendar YY.M.BUILD; GitHub releases only for now), and a version gate in CI checks the format, workspace inheritance, publish = false and Cargo.lock. Agents now close milestones on green CI with evidence posted and merge term.hut PRs. ADR-004 maps font families without a confirmed licence to licensed fallbacks. The upstream checkout's push URL is disabled because this is strictly a port. Phase 8 now ends with v26.9.1: ex-801 is deferred and ex-804 tags the GitHub release. PR: [#12](https://github.com/HutsonLabs/excali-rust/pull/12).

## 2026-09-28 · ex-101 · Element model: enums, structs and shared base fields

`excali-core` now has the full Excalidraw element model: `ElementBase` with every `_ExcalidrawElementBase` field under upstream's JSON names, and `ElementKind` tagged on `type` for all 14 element types with their per-type fields. Supporting enums (fill and stroke styles, roundness, font families, alignment, arrowheads with legacy names, bindings, fixed segments, crop), type groupings, arrow subtypes and upstream's construction defaults and constants are in place, pinned by 29 tests. PR: [#11](https://github.com/HutsonLabs/excali-rust/pull/11).

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
