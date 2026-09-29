# excali-rust

A Rust port of the Excalidraw editor: no React, Tauri-ready, `.excalidraw` compatible, built to drop into [term.hut](https://github.com/HutsonLabs/term.hut) as a vendored module.

**Status:** Phase 0 milestone M0 reached (2026-09-28). Phase 1 milestone M1 is not yet reached (checked again 2026-09-28). Phase 2 milestone M2 reached (2026-09-28): the rough.js port and shape construction for every element type, including the looped freedraw fill, match upstream's `generateElementShape` on all 684 cases of the element type x fill style x roughness 0, 1, 2 x seed 1, 7, 1041657908 matrix, checked in CI. Phase 3 milestone M3 reached (2026-09-28): text wrapping, bound-text sizing and font-file measurement are ported and pass upstream's tests, and all six vendored families are held to 0.5 px across the corpus, with every older stored width outside it traced to the upstream measurement that wrote it. Phase 4 milestone M4 is not yet reached (checked 2026-09-28): PNG and SVG export and the `excali` CLI are merged, SVG export reproduces upstream's `export.test.ts` snapshots and 48 `exportToSvg` documents byte for byte, and the CLI renders all 4187 items of the 232 catalogue libraries; two upstream SVG snapshots and a PNG comparison with upstream in Chrome are still open (ex-g401, ex-g402). The core model, restore, AppState, fractional indexing, library, clipboard, PNG and SVG payload codecs and the JSON Schemas are merged. Every scene-bearing upstream test fixture and all 232 catalogue libraries round-trip byte for byte against upstream. The round trip still drops 24 lines with a string `strokeWidth` (ex-117) and clears 1245 legacy arrow bindings (ex-116, which waits on phase 5 hit testing). The review surface is the site in `site/`, published to GitHub Pages: plan, architecture, design system, mockups, decisions, research and the evidence log. The Cargo workspace (`crates/`, one crate per layer of the architecture overview) builds and tests green; the crates are being filled in phase by phase from the tracker.

## Layout

| Path | What |
|---|---|
| `Cargo.toml`, `crates/` | The Rust workspace: 14 crates named and layered as in Architecture → Overview. Toolchain pinned in `rust-toolchain.toml`; MSRV is `rust-version` in `Cargo.toml`. |
| `site/` | Zola site (the plan). `scripts/site/zola.sh build` renders it to `site/public/`. |
| `plan/tasks.json` | The authored task graph: 9 epics, 112 tasks, 9 milestones with dependencies and acceptance criteria. |
| `.beads/` | beads tracker. `issues.jsonl` is the tracked export; the database is local and ignored. |
| `scripts/bootstrap.sh` | Idempotent setup: git identity, hooks, `bd`, Zola. Run this first. |
| `scripts/gates/attribution.py` | The authorship gate (hooks and CI). |
| `scripts/gates/workspace.py` | The crate-graph gate: workspace members and dependency direction must match the architecture page; `wasm-build.sh` builds the wasm32 crates. |
| `scripts/tasks/seed.py` | Upserts `plan/tasks.json` into beads and refreshes the export and the progress page. |
| `scripts/tasks/render-progress.py` | Renders the export into `site/content/plan/progress.md`. |
| `tools/roughr-eval/` | The ex-206 spike behind ADR-003: the `roughr` crate replayed on the rough.js goldens (`report.json`, `report-fork.json`); outside the workspace, checked by the `roughr-eval` CI job. |
| `.githooks/` | `pre-commit`, `commit-msg`, `pre-push` (activated by bootstrap). |
| `.github/workflows/` | `gates.yml` (authorship gate on PRs), `rust.yml` (fmt, clippy `-D warnings`, test, crate-graph gate, wasm32 build, MSRV, goldens, roughr evaluation) and `pages.yml` (site build and deploy). |

## Start

```sh
./scripts/bootstrap.sh
bd ready                      # tasks with no open blockers
./scripts/site/zola.sh serve  # http://127.0.0.1:1111
cargo test --workspace        # the Rust suite
cargo run -p excali-cli -- --help  # the excali CLI: validate, render, export, lib
```

Working rules are on the site under Plan → Agent workflow. In one line: pick from `bd ready`, claim, branch per issue, cite evidence in the PR, pass the gates, close the issue in the merge commit.

## Licence

MIT, as upstream Excalidraw. See `LICENSE`.
