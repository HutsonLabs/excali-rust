+++
title = "ADR-006: Review site with Zola on GitHub Pages"
description = "Why the plan is published as a static site built by a pinned Zola through the official Pages action."
weight = 6
+++

**Status.** Accepted, 2026-09-28.

## Evidence

- Zola 0.22.0 is the version the official `getzola/github-pages` action documents (`zola_version: v0.22.0`; "GitHub Pages must be configured to use GitHub Actions in the repository settings"). Its release tarball for x86_64 Linux was downloaded on 2026-09-28 with SHA-256 `f1d491f8956b94384c27d75cb6b2bf60d3916d1ade9564bcbfe7c03f0258aebf`, recorded in `scripts/site/zola.sh`.
- The other unix v0.22.0 assets were pinned on 2026-09-27 (`ex-007`) from the digests GitHub publishes for the release (`gh api repos/getzola/zola/releases/tags/v0.22.0`, `assets[].digest`), each re-checked with `shasum -a 256` on the downloaded tarball: aarch64-apple-darwin `96015a922a7d83827e381e273aef6be916711d43e89e65e8e82b4da0350fc425`, x86_64-apple-darwin `f0268e7559d8b6b79d50cef1cd6025a41819cbab920c3c5b0854e0de3a6584b9`, aarch64-unknown-linux-gnu `14ef16bfb36ff3911a0fcddbbd20e74d1383ac87cce4c2b097c9055fc1c98e87`. The script runs under macOS `/bin/bash` 3.2 (a `case` lookup instead of `declare -A`), verifies with `sha256sum` or `shasum -a 256`, and refuses a download it has no digest for. `scripts/site/zola-test.sh` covers it offline (including linked worktrees on git older than 2.31, which share the main clone's `.tools/`). The `bootstrap-and-site` job in `.github/workflows/gates.yml` runs on ubuntu-latest and macos-latest; there it runs those tests under `/bin/bash`, then `/bin/bash scripts/site/zola.sh ensure` on a clean checkout, which must download the real release and print `sha256 verified` with the pinned digest. That step runs before `bootstrap.sh` because bootstrap starts `zola.sh` through its `#!/usr/bin/env bash` shebang, which on the runners (and on a Mac with Homebrew) picks bash 5 rather than `/bin/bash` 3.2. `bootstrap.sh` and `zola.sh build` then run under `/bin/bash` against the verified binary.
- `actions/deploy-pages@v4` with `pages: write` and `id-token: write` and the `github-pages` environment is the documented deployment (README of `actions/deploy-pages`); `actions/upload-pages-artifact@v3` is the documented companion, but the Zola action runs that upload itself, so the workflow must not repeat it (the first run on `main` failed with a 409 artifact conflict until it was removed); `actions/checkout@v7` is current and is what term.hut's workflows use.
- HutsonLabs' existing site (termhut.hutsonlabs.com) is a hand-written static page on Cloudflare with a design-system directory whose tokens this site reuses for its chrome.

## Decision

`site/` is a Zola project with its own minimal templates (no third-party theme to keep the authorship gate simple); `scripts/site/zola.sh` pins and verifies the binary locally; `.github/workflows/pages.yml` builds with the official action and deploys through the Actions path. The progress page is generated from the tracker before every build.

## Consequences

One repository setting is needed by a human (`ex-005`): Settings > Pages > Source: GitHub Actions. It was in place by 2026-09-28 (the Pages API reports `build_type: workflow`, and the first successful deploy served the home page). A `smoke` job in `pages.yml` runs `scripts/site/smoke.py` after every deploy, so a reverted setting or a broken deploy fails the workflow. Site chrome follows HutsonLabs tokens; editor mockups follow Excalidraw tokens; the two are never mixed on one page except in the term.hut embedding mockup, where that is the point.
