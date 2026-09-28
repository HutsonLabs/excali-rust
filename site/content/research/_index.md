+++
title = "Research"
description = "Inventories of the upstream Excalidraw source, produced by reading it file by file. Every claim carries a path and line."
sort_by = "weight"
weight = 6
+++

These three pages are the primary evidence for the architecture, the design system and the task graph. They were produced on 2026-09-28 from a shallow clone of `excalidraw/excalidraw` at commit `438d89861f53d8a90ad566113ecac1b83761098f` (2026-09-27). Paths are relative to that checkout, shown as `<checkout>` where a page spells it out.

Two caveats the reports themselves raise:

- The commit is ahead of the last npm release (`@excalidraw/excalidraw` 0.18.1 on the registry on 2026-09-28) and contains unreleased schema fields (`stickynote`, `created`, `baseFontSize`, `labelPosition`, freedraw `strokeOptions`, `FixedPointBinding.mode`). The port targets this commit and must also read files written by older builds, which is what upstream's `restore.ts` does.
- The rendering report's statements about rough.js internals were checked afterwards against `rough-stuff/rough` `master` (`package.json` version 4.6.6): the `Random.next()` formula and the `defaultOptions` block match what the report states. See the [evidence log](../evidence/).
