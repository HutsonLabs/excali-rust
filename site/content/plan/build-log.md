+++
title = "Build log"
description = "What landed on main, newest first."
weight = 5
+++

Each entry records a task that merged to main: the date, the task id and title, what now works, and the pull request that landed it. The newest entries are at the top. The [progress page](@/plan/progress.md) has the full status of the task graph.

## 2026-09-27 · ex-002 · Upstream pin script: check out excalidraw at the pinned commit into .tools/upstream

`scripts/upstream/checkout.sh` checks out excalidraw at the commit pinned in `site/config.toml` (438d898). The checkout goes into the main clone's `.tools/upstream`, which every worktree shares. It verifies HEAD and is idempotent. It refuses dirty or foreign checkouts, and it will not use any other commit unless `PIN` is set explicitly. `bootstrap.sh` runs it, and the `upstream-pin` CI job runs both the offline test suite and a live checkout. PR: [#4](https://github.com/HutsonLabs/excali-rust/pull/4).

## 2026-09-27 · ex-005 · Enable GitHub Pages source = GitHub Actions and confirm first deploy

GitHub Pages now deploys from GitHub Actions, and the site is live at its configured `base_url`. A new `smoke` job runs after each deploy to main. It fails the workflow unless the home page returns HTTP 200 with the expected title. `scripts/site/zola.sh` now also works with the bash 3.2 that ships with macOS. PR: [#3](https://github.com/HutsonLabs/excali-rust/pull/3).
