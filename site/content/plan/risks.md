+++
title = "Risks"
description = "What could make the port miss its criteria, with the evidence that makes each risk real and the mitigation planned."
weight = 5
+++

| Risk | Evidence | Mitigation |
|---|---|---|
| **Text metrics diverge from the browser.** Upstream measures with `canvas.measureText` and stores the result in the file; the port measures from font files. Small differences change line breaks and container sizes. | `textMeasurements.ts:121-168`; research: [text measurement](../../research/data-model/#8-text-measurement-and-cross-implementation-fidelity). | Trust stored `width`/`height`/`text` on load and re-measure only on edit; use the same woff2 files; add a corpus test comparing stored and measured widths; allow a custom metrics provider like upstream's `setCustomTextMetricsProvider`. |
| **Rough.js port drifts.** Any change in randomness consumption order changes every shape. | rough.js `Random.next()` and `defaultOptions` verified from `rough-stuff/rough` master; upstream pins 4.6.4 while master is 4.6.6. | Golden tests generated with the pinned npm version; port from the 4.6.4 tag, not master; evaluate the `roughr` crate against the goldens before writing a port ([ADR-003](../../decisions/adr-003-sketch-renderer/)). |
| **Upstream moves.** The analysed commit already contains unreleased schema fields. | `CHANGELOG.md` "Unreleased" section; `packages/excalidraw/package.json` 0.18.0 vs npm 0.18.1. | Pin the commit in `site/config.toml` and in fixtures; re-run the three inventories when re-pinning and diff the research pages. |
| **Font licences.** Five families have no licence file in the upstream repo. | Research: [fonts table](../../research/ui-design-system/#13-fonts). | Verify each licence at its source before vendoring ([ADR-004](../../decisions/adr-004-fonts/)); until then load fonts from the host or fall back to local families as term.hut does. |
| **WASM weight.** A full port with fonts could exceed what a 5.4 MB app tolerates. | `PRODUCT.md:106-108, 165`. | Budgets in [phases](../phases/); `wasm-opt`, `panic = "abort"`, no `regex`/`chrono` style crates in the web build; fonts lazy. |
| **DOM chrome without a framework is slow to write.** | Upstream has 207 icons and ~99 actions (research: [actions](../../research/ui-design-system/#5-actions-registry-pactions)). | A tiny typed DOM builder in `excali-ui`, generated icon module from upstream's SVG paths (MIT), and the actions registry as data so panels are generated, not hand-written. |
| **term.hut constraints change.** | `PRODUCT.md` is the binding document and is versioned. | The integration is one file in term.hut (`preview.js` routing) plus a vendored directory; re-integration cost is bounded. |
| **Autonomous agents lose context between sessions.** | The reason beads exists (its README: persistent, structured memory with dependencies). | `bd prime`, `bd ready`, evidence in issues, milestone gates closed by a human. |
