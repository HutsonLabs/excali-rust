+++
title = "Tokens"
description = "Excalidraw's CSS custom properties, light and dark, plus spacing, radii, z-index and breakpoints, read from packages/excalidraw/css/theme.scss."
weight = 1
+++

All tokens are declared on `.excalidraw`; dark values under `.excalidraw.theme--dark` (`theme.scss:184-278`). The port emits the same names so upstream's theming guidance ("override CSS variables on `.excalidraw` and `.excalidraw.theme--dark`") applies unchanged. Full table with line numbers: [research](../../research/ui-design-system/#11-css-custom-properties-theme-pcssthemescss).

In the port, `excali_ui::primitives::install_stylesheet` embeds theme.scss compiled at the pin (every token below, light and dark), as the first child of the document's head, so a host's own `.excalidraw { --color-primary: … }` and `.excalidraw.theme--dark { … }` rules win at equal specificity wherever its stylesheet is. `excali_ui::theme::apply_theme` toggles `theme--dark` on the container as `App.tsx:4434-4437` does, `apply_container_tokens` writes `--right-sidebar-width` into its inline style (`App.tsx:2453`), and `light_tokens`, `dark_tokens` and `tokens(theme)` read the embedded declarations. `crates/excali-ui/tests/theme.rs` holds every token in the tables below (names and values) to the stylesheet, and the Chromium suite `tests/web/primitives/theme.spec.mjs` checks them computed and overridden by a host.

## Core

| Token | Light | Dark |
|---|---|---|
| `--color-primary` | <span class="swatch" style="background:#6965db"></span>`#6965db` | <span class="swatch" style="background:#a8a5ff"></span>`#a8a5ff` |
| `--color-primary-darker` | `#5b57d1` | `#b2aeff` |
| `--color-primary-darkest` | `#4a47b1` | `#beb9ff` |
| `--color-primary-light` | `#e3e2fe` | `#4f4d6f` |
| `--color-primary-light-darker` | `#d7d5ff` | `#43415e` |
| `--color-primary-hover` | `#5753d0` | `#bbb8ff` |
| `--color-brand-hover` | `#5753d0` | `#bbb8ff` |
| `--color-brand-active` | `#4440bf` | `#d0ccff` |
| `--color-on-primary-container` | `#030064` | `#e0dfff` |
| `--color-surface-primary-container` | `#e0dfff` | `#403e6a` |
| `--color-surface-high` | `#f1f0ff` | `#2e2d39` |
| `--color-surface-mid` | `#f6f6f9` | `hsl(240 6% 10%)` |
| `--color-surface-low` | `#ececf4` | `hsl(240 8% 15%)` |
| `--color-surface-lowest` | `#ffffff` | `hsl(0 0% 7%)` |
| `--color-on-surface` | `#1b1b1f` | `#e3e3e8` |
| `--color-border-outline` | `#767680` | `#8e8d9c` |
| `--color-border-outline-variant` | `#c5c5d0` | `#46464f` |
| `--color-selection` | `#6965db` | `#b4b0ff` |
| `--island-bg-color` | `#ffffff` | `#232329` |
| `--island-bg-color-alt` | `#fff` | `hsl(240,12%,12%)` |
| `--default-bg-color` / `--input-bg-color` | `#fff` | `#121212` |
| `--input-border-color` | `#ced4da` | `#2e2e2e` |
| `--input-hover-bg-color` | `#f1f3f5` | `#181818` |
| `--input-label-color` | `#495057` | `#e9ecef` |
| `--overlay-bg-color` | `rgba(12,12,14,.35)` | `rgba(12,12,14,.65)` |
| `--popup-secondary-bg-color` | `#f1f3f5` | `#222` |
| `--popup-text-color` | `#000` | `#ced4da` |
| `--popup-text-inverted-color` | `#fff` | `#2c2c2c` |
| `--select-highlight-color` | `#339af0` | `#4dabf7` |
| `--focus-highlight-color` | `#a5d8ff` | `#228be6` |
| `--link-color` | blue-7 `#1c7ed6` | blue-4 `#4dabf7` |
| `--keybinding-color` | `--color-gray-40` | `--color-gray-60` |
| `--button-gray-1/2/3` | `#e9ecef` / `#ced4da` / `#adb5bd` | `#363636` / `#272727` / `#222` |
| `--button-destructive-bg-color` / `--button-destructive-color` | `#ffe3e3` / `#c92a2a` | `#5a0000` / `#ffa8a8` |
| `--color-disabled` | gray-40 | gray-70 |
| `--theme-filter` | `none` | `invert(93%) hue-rotate(180deg)` |

Gray scale (`--color-gray-N`): 10 `#f5f5f5`, 20 `#ebebeb`, 30 `#d6d6d6`, 40 `#b8b8b8`, 50 `#999`, 60 `#7a7a7a`, 70 `#5c5c5c`, 80 `#3d3d3d`, 85 `#242424`, 90 `#1e1e1e`, 100 `#121212`.

Semantic sets: `--color-warning*` (`#fceeca`, `#f5c354`, `#f3ab2c`, `#ec8b14`), `--color-danger*` (`#db6965`, `#d65550`, `#d1413c`; dark `#ffa8a5`, `#672120`, `#8f2625`, `#ac2b29`), `--color-success*` (`#cafccc` … `#6edf74`), `--color-muted*`, `--color-badge` `#0b6513` on `#d3ffd2`.

## Shape and space

| Token | Value |
|---|---|
| `--space-factor` | `0.25rem` (every gap and padding is a multiple) |
| `--border-radius-md` | `0.375rem` |
| `--border-radius-lg` | `0.5rem` |
| `--default-button-size` | `2rem` (`2.25rem` at ≥ 1921 px device width) |
| `--default-icon-size` | `1rem` (`1.25rem` at ≥ 1921 px) |
| `--lg-button-size` / `--lg-icon-size` | `2.25rem` / `1rem` |
| `--editor-container-padding` | `1rem` (`0.75rem` on phone) |
| `--mobile-action-button-size` | `2rem` |
| `--right-sidebar-width` | `302px` |
| `--shadow-island` | `0px 0px 1px 0px rgba(0,0,0,.17), 0px 0px 3px 0px rgba(0,0,0,.08), 0px 7px 14px 0px rgba(0,0,0,.05)` |
| Dialog widths | small 550, regular 800, wide 1024 px |
| Focus ring | `box-shadow: 0 0 0 1px var(--color-brand-hover)` |
| Button font size | `0.8333rem` |

## z-index

canvas 1 · interactiveCanvas 2 · svgLayer/wysiwyg/canvasButtons 3 · layerUI 4 · viewportStatusFrame/eyeDropperBackdrop 5 · eyeDropperPreview 6 · hyperlinkContainer 7 · cursorHint 8 · ui-bottom 60 · ui-context-menu 90 · ui-styles-popup/ui-top 100 · ui-main-menu 110 · ui-library 120 · fileDropOverlay 130 · modal 1000 · popup 1001 · toast 999999.

## Breakpoints and form factor

| Rule | Value |
|---|---|
| Phone | `width ≤ 599` or (`height < 500` and `width < 1000`) |
| Tablet | `min(w,h) ≥ 600` and `max(w,h) ≤ 1180` |
| Desktop | otherwise |
| Sidebar can dock | `width > 1229` |
| Styles panel mode | phone → mobile; tablet → compact; desktop → stored preference (`full` or `compact`) |
| Top menu grid | `1fr 2fr 1fr`, 1rem gap; `1fr 1fr 1fr`, 3rem gap at ≥ 1536 px |
| Mobile bottom bar | max width 450 px, centred |

Source: `packages/common/src/editorInterface.ts:19-32, 64-79, 137-164`.
