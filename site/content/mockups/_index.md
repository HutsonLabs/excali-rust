+++
title = "Mockups"
description = "Static HTML mockups of the editor chrome, built from Excalidraw's own token values."
weight = 4
template = "mockups.html"
[extra]
mockups = [
 { file = "01-desktop-light.html", title = "01 Desktop, light, rectangle selected", summary = "Full styles panel on the left, shapes toolbar centred, library trigger right, zoom/undo footer. Tokens from theme.scss light." },
 { file = "02-desktop-dark.html", title = "02 Desktop, dark, arrow selected", summary = "theme--dark token set; canvas colours pass through --theme-filter as upstream does; arrow type and arrowhead controls replace fill and edges." },
 { file = "03-main-menu.html", title = "03 Main menu and preferences", summary = "Default menu items with shortcuts, theme radio, preferences submenu, canvas background top picks." },
 { file = "04-library-sidebar.html", title = "04 Library sidebar docked", summary = "302 px sidebar docked at widths above 1229 px; Search and Library tabs; personal and public sections; import from URL or file." },
 { file = "05-tablet-compact.html", title = "05 Tablet, compact styles panel", summary = "Tablet rule min(w,h) ≥ 600 and max ≤ 1180: swatches plus popovers instead of the full panel; paired tool popovers in the toolbar." },
 { file = "06-phone.html", title = "06 Phone", summary = "width ≤ 599: top bar, scrolling styles bar with undo/redo, bottom toolbar of 36 px buttons capped at 450 px, lasso as default tool." },
 { file = "07-termhut-embedded.html", title = "07 Embedded in term.hut", summary = "The editor as a pane inside term.hut's Catppuccin Mocha chrome, dark tokens tinted by the host, with Fit, zoom, Import library and JSON toggle in the pane header." },
]
+++

Every mockup is plain HTML and CSS under `site/static/mockups/`. Layout dimensions and colours are copied from upstream's stylesheets and components (`theme.scss`, `styles.scss`, `Island.scss`, `ToolIcon.scss`, `Toolbar.tsx`, `Actions.tsx`, `MobileToolbar.tsx`, `LayerUI.tsx`) and cited in the CSS comments; the [tokens page](../design-system/tokens/) is the reference. Two things are stand-ins and say so in the source:

- **Icons** are simplified 24×24 glyphs in the Tabler style. The port generates the real set from upstream's `icons.tsx` (task `ex-517`).
- **The sample scene** is hand-drawn SVG that imitates Rough.js output (hachure at −41°, gap 8 for stroke width 2, dashed `[8, 10]`). Real output comes from `excali-rough` and is verified by goldens, not by these pictures.

Open a mockup full-size to read tooltips: every tool button carries upstream's tooltip text and shortcut. Compare against excalidraw.com at the same viewport width; differences you find are review findings for the [task graph](../plan/progress/).
