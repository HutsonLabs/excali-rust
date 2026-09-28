+++
title = "Icons and components"
description = "Where the icons come from, how big they are, and the component kit the chrome is built from."
weight = 7
+++

## Icons

- 207 icons exported from `packages/excalidraw/components/icons.tsx` (2,593 lines), all inline SVG produced by `createIcon(d | children, opts)` with `aria-hidden` and `role="img"`.
- Presets: `tablerIconProps` 24×24, stroke 2, `currentColor`, round caps (≈106 uses); `modifiedTablerIconProps` 20×20 (≈48); `arrowheadPreviewIconProps` 40×20. The header comment mentions FontAwesome; most icons are Tabler.
- Rendered at `--default-icon-size` (1rem; 1.25rem at ≥ 1921 px). Colour from `--icon-fill-color` (`--color-on-surface`). Icons flagged `mirror` get `.rtl-mirror`.
- The port generates its icon module from that file (MIT) in task `ex-517`, so glyphs are identical rather than redrawn. The mockups on this site use simplified stand-ins and say so.

## Component kit

There is no Storybook upstream; the kit is hand-rolled under `packages/excalidraw/components`:

| Component | Notes |
|---|---|
| Island | container with `--island-bg-color`, `--shadow-island`, radius lg, padding in `--space-factor` units |
| Stack.Row / Stack.Col | gap in `--space-factor` units |
| ToolIcon / ToolButton | `--default-button-size` square, checked state uses the primary container colours |
| Button, FilledButton, IconButton | outline style: 1 px border, radius lg, `0.625rem` padding, `--button-*` hooks |
| RadioGroup, RadioSelection, Switch, Range, TextField | form controls with component-scoped tokens |
| Dialog, Modal, Popover, PropertiesPopover | dialog widths 550/800/1024; Radix Popover upstream, a DOM popover in the port |
| Tooltip, Toast, Card | tooltip is a portal, bottom or top |
| dropdownMenu/*, Sidebar/* | main menu, extra tools, library |

Each has a counterpart in `excali-ui` (task `ex-516`) emitting the same class names, so the shipped stylesheet can be derived from upstream's SCSS with minimal edits.

## Actions

99 action names, 95 registered, handled by an `ActionManager` that sorts by `keyPriority` and runs each `keyTest`. The port keeps the registry as data (`ex-514`) and generates the panel, context menus and command palette from it.

## i18n

58 locale JSON files with 633 leaf keys under `packages/excalidraw/locales`; English is the static fallback; languages under 85 % complete are hidden; RTL locales set `dir`. The port loads the same JSON files (`ex-711`).
