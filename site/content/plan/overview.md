+++
title = "Overview"
description = "What the port is, what it is not, what done means, and the constraints it inherits from its first host."
weight = 1
+++

## The job

Build a reusable Excalidraw-style editor in Rust that:

1. **mirrors Excalidraw's UI** closely enough that an Excalidraw user needs no relearning: the same tools, panels, shortcuts, palettes, fonts and hand-drawn look;
2. **reads and writes `.excalidraw` files** losslessly, including files written by older Excalidraw builds, and **imports `.excalidrawlib` libraries** (both version 1 and version 2, including the public catalogue at libraries.excalidraw.com);
3. **does not require React**, or any JavaScript framework, bundler or package manager on the host side;
4. **runs inside a Tauri v2 webview** and can also render headlessly (PNG/SVG export, validation) from native Rust;
5. **drops into term.hut** as a vendored static asset so term.hut can create, read, update and delete `.excalidraw` files and import libraries.

The upstream project is `excalidraw/excalidraw`, MIT licensed (its README states "Excalidraw is released under the MIT license"; see [Evidence](../../evidence/)). The port keeps that licence for anything derived and adds nothing that would prevent a downstream MIT consumer.

## Why a port rather than embedding

term.hut already renders `.excalidraw` files without Excalidraw. Its `ui/src/excalidrawScene.js` opens with the reason: Excalidraw "is a React application; pulling it in would have meant React, a bundler this frontend doesn't have, and ~3.9 MB", so term.hut vendored Rough.js and perfect-freehand (32 KB together) and re-implemented the option mapping. That viewer is read-only "by design" (`ui/src/excalidrawView.js:3`). Editing needs the rest of the editor: selection, transforms, binding, text editing, history, the properties panel, the library. Upstream's `App.tsx` alone is 14,138 lines, with about 130,000 non-test lines across its packages (see [Research: rendering](../../research/rendering/#10-lines-of-code-ts-tsx)). Writing that in Rust once, and shipping it as one WASM module plus a small ES-module shim, gives every Rust or Tauri host the same editor without React.

## Non-goals for version 1

These are in upstream and deliberately out of scope until the parity checklist in Phase 7 is green:

- live collaboration, share links, encryption (`#room=`, `#json=`);
- the AI features: text-to-diagram, Mermaid import, magic frames;
- live `embeddable` / `iframe` content (rendered as placeholders, as term.hut does today);
- Excalidraw+ and account features;
- non-English locales (the JSON locale files are reusable later; the loader design keeps that door open).

## Definition of done

The project is done when all of the following are true and each is backed by an automated check in CI:

| # | Criterion | How it is measured |
|---|---|---|
| D1 | Round trip | Every fixture in upstream's test suite and all 232 public libraries parse, re-serialise, and re-parse to an identical element set; unknown fields survive. |
| D2 | Visual fidelity | SVG export of the upstream fixtures matches upstream's snapshot to two decimals per path number, the precision upstream itself uses (`MAX_DECIMALS_FOR_SVG_EXPORT = 2`). PNG export matches within a perceptual tolerance recorded per fixture. |
| D3 | UI mirror | The desktop, tablet and phone layouts match the [mockups](../../mockups/) and the [design system](../../design-system/); every shortcut in the [shortcut table](../../design-system/shortcuts/) works. |
| D4 | Host integration | term.hut can create, open, edit, save and delete a `.excalidraw` file and import a library from the public catalogue, with the editor loaded as a vendored ES module and no build step. |
| D5 | Tauri | A minimal Tauri v2 example app embeds the same module and uses the plugin for file dialogs and headless export. |
| D6 | Weight | The WASM module without fonts stays under the budget in the [phases page](../phases/); fonts load lazily by unicode range as upstream does. |
| D7 | Authorship | Every commit and tracked file passes the authorship gate; CI enforces it. |

## Constraints inherited from term.hut

term.hut's `PRODUCT.md` fixes these, and the editor must fit them rather than the other way round:

- "No build step, no framework. Vanilla JS, no bundler, no package manager, no `node_modules`. Tauri's API arrives via `withGlobalTauri`. Third-party code is vendored under `ui/vendor/` as UMD/static files. The shipped dmg is ~5.4 MB" (`PRODUCT.md:106-108`).
- "Catppuccin Mocha is the binding palette" for the app chrome (`PRODUCT.md:110`). The editor keeps Excalidraw's own tokens inside its container and exposes the same CSS custom properties upstream documents for theming, so the host can tint it.
- File access goes through term.hut's existing commands (`fs_read_text`, `fs_write_text`, `fs_create`, `fs_rename`, `fs_trash`, and the SSH-aware variants), not through a second file layer. The editor therefore talks to a small host adapter interface and never touches the file system itself.

## Reading order

[Phases](../phases/) sets the sequence and milestones. [Agent workflow](../agent-workflow/) is the operating manual for whoever, or whatever, picks up a task. [Progress](../progress/) is generated from the tracker on every build.
