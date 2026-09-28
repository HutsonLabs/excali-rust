+++
title = "Keyboard shortcuts"
description = "Every shortcut the port must honour, grouped as the Help dialog groups them. Ctrl means Cmd on macOS."
weight = 6
+++

Source: `packages/excalidraw/components/HelpDialog.tsx`, `actions/*` `keyTest`s and `App.onKeyDown` ([research §5–6](../../research/ui-design-system/#5-actions-registry-pactions)). `CTRL_OR_CMD` is `metaKey` on Darwin, `ctrlKey` elsewhere.

## Tools

| Key | Tool |
|---|---|
| H | hand (toggle) |
| V or 1 | selection |
| R or 2 | rectangle |
| D or 3 | diamond |
| O or 4 | ellipse |
| A or 5 | arrow (press again: sharp → round → elbow) |
| L or 6 | line |
| P, X or 7 | freedraw |
| T or 8 | text |
| N | sticky note |
| 9 | image |
| E or 0 | eraser (toggle) |
| F | frame |
| Shift+X | autoshape |
| K | laser |
| B | bucket fill (press again cycles colour) |
| Q | tool lock |
| I, Shift+S, Shift+G | eyedropper (any, stroke, background) |
| S / G | open stroke / background picker |
| Shift+F | font picker |
| Ctrl+Enter | edit line points |
| Enter | edit text / enter frame |
| Esc or Ctrl+Enter | finish text |
| Tab / Shift+Tab | convert element type |
| Ctrl (held) | prevent binding |
| Ctrl+K | link |

## View

| Key | Action |
|---|---|
| Ctrl + `+` / `−` / `0` | zoom in / out / reset (also with Shift, and numpad) |
| Shift+1 | zoom to fit all |
| Shift+2 | zoom to fit selection in viewport |
| Shift+3 | zoom to fit selection |
| PgUp / PgDn (Shift: horizontal) | page scroll |
| Space + drag, wheel + drag | pan |
| Alt+Z | zen mode |
| Alt+S | object snap |
| Ctrl+' | grid |
| Alt+R | view mode |
| Alt+Shift+D | theme |
| Alt+/ | stats |
| Ctrl+F | search |
| Ctrl+/ or Ctrl+Shift+P | command palette |

## Editor

| Key | Action |
|---|---|
| Ctrl+Arrow | create flowchart node; Alt+Arrow navigates |
| Arrow keys | nudge 1 px; Shift 5 px; grid size when grid on |
| Delete / Backspace | delete |
| Ctrl+Backspace / Ctrl+Delete | clear canvas (confirm) |
| Ctrl+X / C / V | cut / copy / paste; Ctrl+Shift+V plain paste |
| Ctrl+A | select all |
| Shift+click | add to selection; Ctrl+click deep select; Ctrl+drag deep box select |
| Alt+Shift+C | copy as PNG |
| Ctrl+Alt+C / V | copy / paste styles |
| Ctrl+[ / ] | send backward / bring forward |
| Ctrl+Alt+[ / ] (Mac) or Ctrl+Shift+[ / ] | send to back / bring to front |
| Ctrl+Shift+↑ ↓ ← → | align top / bottom / left / right |
| Alt+H / Alt+V | distribute horizontally / vertically |
| Ctrl+D or Alt+drag | duplicate |
| Ctrl+Shift+L | lock / unlock |
| Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y | undo, redo |
| Ctrl+G / Ctrl+Shift+G | group / ungroup |
| Shift+H / Shift+V | flip horizontal / vertical |
| Ctrl+Shift+< / > | decrease / increase font size |
| Shift (while resizing) | keep aspect ratio; Alt resize from centre; Shift while rotating snaps angle |
| Esc | deselect / finish |
| ? | help |
| Ctrl+O / Ctrl+S / Ctrl+Shift+S / Ctrl+Shift+E | open / save / save as / export image |

## Colour picker

`q w e r t / a s d f g / z x c v b` palette cells; `1–5` custom colours; `Shift+1–5` shades; `i` eyedropper; Tab cycles sections; Esc closes.
