+++
title = "excali-rust"
description = "A plan, design system and mockups for porting the Excalidraw editor to Rust."
+++

# excali-rust

**A Rust port of the Excalidraw editor: no React, Tauri-ready, `.excalidraw` compatible.**

This site is the review surface for the port before any editor code is written. It holds the plan, the architecture, the design system to mirror, static mockups, the decisions taken so far, the research inventories those decisions rest on, and the evidence log behind every claim.

<div class="cards">
<a class="card" href="plan/"><h3>Plan</h3><p>Goal, non-goals, phases, milestones, the live task graph, and how an autonomous team works it.</p></a>
<a class="card" href="architecture/"><h3>Architecture</h3><p>Crate layout, file-format spec, rendering fidelity, the term.hut embedding contract, Tauri.</p></a>
<a class="card" href="design-system/"><h3>Design system</h3><p>Excalidraw's tokens, palettes, fonts, layout rules and controls, read out of its source.</p></a>
<a class="card" href="mockups/"><h3>Mockups</h3><p>Static HTML of the editor chrome on desktop, tablet, phone, and embedded in term.hut.</p></a>
<a class="card" href="decisions/"><h3>Decisions</h3><p>Architecture decision records, each with the evidence it was made on.</p></a>
<a class="card" href="research/"><h3>Research</h3><p>Three inventories of the upstream source with file and line citations.</p></a>
<a class="card" href="evidence/"><h3>Evidence</h3><p>Every source consulted: URL or path, what it established, when it was read.</p></a>
</div>

## Status

| Item | State |
|---|---|
| Upstream analysed | `excalidraw/excalidraw` at `438d8986`, 2026-09-27 |
| Target host | term.hut (Tauri v2, vanilla JS, no bundler) and any Tauri v2 app |
| Release | [v26.9.2](https://github.com/HutsonLabs/excali-rust/releases/tag/v26.9.2) (2026-09-30): the web runtime tarball, the example app's signed and notarized macOS disk image with its in-place updater (`latest.json`), and their `SHA256SUMS` on GitHub, nothing on a registry. The first calendar release was [v26.9.1](https://github.com/HutsonLabs/excali-rust/releases/tag/v26.9.1) ([ADR-009](decisions/adr-009-calendar-versioning/)) |
| Editor code | phases 0 to 7 reached; see the [phases](plan/phases/) |
| Task graph | tracked in-repo with beads, rendered on the [progress page](plan/progress/) |
| Authorship | every commit and file passes the [authorship gate](decisions/adr-005-authorship-gate/) |

## How to review

1. Read the [plan overview](plan/overview/) for scope and success criteria.
2. Check the [decisions](decisions/) you disagree with; each names the evidence it used, so a counter-argument can point at a source too.
3. Open the [mockups](mockups/) side by side with excalidraw.com and note what differs.
4. Look at the [task graph](plan/progress/) and reorder priorities by editing `plan/tasks.json` in the repository.
