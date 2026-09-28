+++
title = "ADR-006: Review site with Zola on GitHub Pages"
description = "Why the plan is published as a static site built by a pinned Zola through the official Pages action."
weight = 6
+++

**Status.** Accepted, 2026-09-28.

## Evidence

- Zola 0.22.0 is the version the official `getzola/github-pages` action documents (`zola_version: v0.22.0`; "GitHub Pages must be configured to use GitHub Actions in the repository settings"). Its release tarball for x86_64 Linux was downloaded on 2026-09-28 with SHA-256 `f1d491f8956b94384c27d75cb6b2bf60d3916d1ade9564bcbfe7c03f0258aebf`, recorded in `scripts/site/zola.sh`.
- `actions/deploy-pages@v4` with `pages: write` and `id-token: write` and the `github-pages` environment is the documented deployment (README of `actions/deploy-pages`); `actions/upload-pages-artifact@v3` is the documented companion; `actions/checkout@v7` is current and is what term.hut's workflows use.
- HutsonLabs' existing site (termhut.hutsonlabs.com) is a hand-written static page on Cloudflare with a design-system directory whose tokens this site reuses for its chrome.

## Decision

`site/` is a Zola project with its own minimal templates (no third-party theme to keep the authorship gate simple); `scripts/site/zola.sh` pins and verifies the binary locally; `.github/workflows/pages.yml` builds with the official action and deploys through the Actions path. The progress page is generated from the tracker before every build.

## Consequences

One repository setting is needed by a human (`ex-005`). Site chrome follows HutsonLabs tokens; editor mockups follow Excalidraw tokens; the two are never mixed on one page except in the term.hut embedding mockup, where that is the point.
