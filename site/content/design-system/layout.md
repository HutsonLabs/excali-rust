+++
title = "Layout"
description = "The editor chrome region by region, with the component that owns each and the order of its items."
weight = 4
+++

Desktop and tablet use a fixed three-column top grid; the phone uses a top bar and a bottom bar. Regions are named as upstream names them so the mockups, the tasks and the code use one vocabulary. Line references: [research §3](../../research/ui-design-system/#3-editor-chrome).

## Desktop

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ [≡] [styles panel]     [🔒 ✎ | ✋ ▭ ◇ ◯ → ─ ✎ T N ◌ | ⋯]   [👤] [library ▸] [stats] │  App-menu_top (1fr 2fr 1fr)
│                                                                               │
│                                                                               │
│                              canvas (static + interactive layers)            │
│                                                                               │
│ [−] 100% [+]  [↶] [↷]                                                     [?] │  footer
└──────────────────────────────────────────────────────────────────────────────┘
```

**Left column.** Main-menu trigger (hamburger) above the styles panel (an Island, max height `appState.height − 166`). Panel groups in full mode: stroke, background, fill, stroke width, stroke style, freedraw mode, sloppiness, edges, arrow type, font family/size/align, vertical align, arrowheads, opacity, layers, align, actions (duplicate, delete, group, ungroup, link, crop, line editor). Groups appear only when the selection allows them ([controls](../controls/)).

**Centre column.** The shapes toolbar Island: pen mode, lock, divider, hand, selection (or lasso), rectangle, diamond, ellipse, arrow, line, freedraw, text, sticky note, eraser, divider, "extra tools" dropdown (image, frame, embeddable, autoshape, laser, bucket fill, lasso; "Generate" group). A hint viewer sits under the toolbar.

**Right column.** User list (collaboration), host-provided UI, the library trigger ("Library", `sidebarRightIcon`), stats when enabled.

**Footer.** Zoom out / reset (shows percentage) / zoom in, undo, redo on the left; a centre tunnel for host content; the help button on the right; an "exit zen mode" button when relevant.

**Sidebar.** Docks on the right at 302 px when the container is wider than 1229 px, otherwise floats. Tabs: Search and Library. Library header menu: Load, Export, Publish, Reset/Remove. Sections: Personal library, Excalidraw library, search.

**Dialogs.** Help (three shortcut islands), image export, JSON export, paste chart, overwrite confirm, error, eye dropper; widths 550/800/1024.

## Tablet and compact desktop

Same grid. The styles panel collapses to a row: stroke swatch, background swatch, freedraw-mode button, then popovers for stroke properties (fill, width, style, sloppiness, edges, opacity), arrow type, text (size, align, vertical), font family, duplicate, delete, and a "…" popover for layers, align, group, link, crop. Selection/lasso and freedraw/autoshape become paired popovers in the toolbar. Pen mode is hidden.

## Phone

```
┌────────────────────────────┐
│ [≡]              [✎][lib][⤴]│  top bar
│                            │
│          canvas            │
│                            │
│ ── styles bar (scroll) ──  │
│ [✋][V/lasso][✎/auto][E][▭◇◯][→─][T][img][F][⋯] │  bottom bar ≤ 450 px
└────────────────────────────┘
```

Buttons 36 px with 4 px gaps; text, image, frame appear when width allows; the "…" menu holds the rest and the Generate group. Duplicate and delete are promoted out of the styles popover when at least nine actions fit at 32 px + 6 px gap. Default tool on phone is lasso; the eyedropper is not shown; z-order items are absent from the context menu.

## Welcome screen

Centre: logo, heading, "Load scene", "Help" (plus host items). Hints point at the main menu, the toolbar and the help button.

## Islands and stacks

`Island`: background `--island-bg-color`, shadow `--shadow-island`, radius `--border-radius-lg`, padding `--padding × --space-factor`. `Stack`: gap `--gap × --space-factor`. Tool buttons are `--default-button-size` squares with `--default-icon-size` icons; a checked tool uses `--color-surface-primary-container` behind `--color-on-primary-container`.
