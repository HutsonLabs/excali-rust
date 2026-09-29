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

On a Mac, Cmd+Alt+[ and Cmd+Alt+] also pass the keyTests of send backward and bring forward (Cmd without Shift, `actionZindex.tsx:17-21, 47-51`), so the action manager finds two actions and does nothing (`manager.tsx:113-118`). The port keeps upstream's behaviour.

Every row of the three tables above has a test in `tests/web/keyboard` (Playwright, `scripts/web/keyboard.sh`), which fails when a row has none.

## Help dialog

What the Help dialog (`?`) shows, row for row, as upstream renders it on Linux with the async clipboard and the theme action enabled (`HelpDialog.tsx:145-516`). `tests/help_dialog.rs` fails when this section and the port's dialog differ.

### Tools

| Action | Keys |
|---|---|
| Hand (panning tool) | `H` |
| Selection | `V` or `1` |
| Rectangle | `R` or `2` |
| Diamond | `D` or `3` |
| Ellipse | `O` or `4` |
| Arrow | `A` or `5` |
| Line | `L` or `6` |
| Draw | `P` or `7` |
| Text | `T` or `8` |
| Sticky note | `N` |
| Insert image | `9` |
| Eraser | `E` or `0` |
| Frame tool | `F` |
| Laser pointer | `K` |
| Bucket fill | `B` |
| Pick color from canvas | `I` or `Shift` `S` or `Shift` `G` |
| Edit line/arrow points | `Ctrl` `Enter` |
| Edit text / add label | `Enter` |
| Add new line (text editor) | `Enter` or `Shift` `Enter` |
| Finish editing (text editor) | `Esc` or `Ctrl` `Enter` |
| Curved arrow | `A` `click` `click` `click` |
| Curved line | `L` `click` `click` `click` |
| Crop image | `double-click` or `Enter` |
| Finish image cropping | `Enter` or `Esc` |
| Keep selected tool active after drawing | `Q` |
| Prevent arrow binding | `Ctrl` |
| Add / Update link for a selected shape | `Ctrl` `K` |
| Toggle shape type | `Tab` or `Shift` `Tab` |

### View

| Action | Keys |
|---|---|
| Zoom in | `Ctrl` `+` |
| Zoom out | `Ctrl` `-` |
| Reset zoom | `Ctrl` `0` |
| Zoom to fit all elements | `Shift` `1` |
| Zoom to selection | `Shift` `2` |
| Move page up/down | `PgUp/PgDn` |
| Move page left/right | `Shift` `PgUp/PgDn` |
| Zen mode | `Alt` `Z` |
| Snap to objects | `Alt` `S` |
| Toggle grid | `Ctrl` `'` |
| View mode | `Alt` `R` |
| Toggle light/dark theme | `Alt` `Shift` `D` |
| Canvas & Shape properties | `Alt` `/` |
| Find on canvas | `Ctrl` `F` |
| Command palette | `Ctrl` `/` or `Ctrl` `Shift` `P` |

### Editor

| Action | Keys |
|---|---|
| Create a flowchart from a generic element | `Ctrl` `Arrow Key` |
| Navigate a flowchart | `Alt` `Arrow Key` |
| Move canvas | `Space` `drag` or `Wheel` `drag` |
| Reset the canvas | `Ctrl` `Delete` |
| Delete | `Delete` |
| Cut | `Ctrl` `X` |
| Copy | `Ctrl` `C` |
| Paste | `Ctrl` `V` |
| Paste as plaintext | `Ctrl` `Shift` `V` |
| Select all | `Ctrl` `A` |
| Add element to selection | `Shift` `click` |
| Deep select | `Ctrl` `click` |
| Deep select within box, and prevent dragging | `Ctrl` `drag` |
| Copy to clipboard as PNG | `Shift` `Alt` `C` |
| Copy styles | `Ctrl` `Alt` `C` |
| Paste styles | `Ctrl` `Alt` `V` |
| Send to back | `Ctrl` `Shift` `[` |
| Bring to front | `Ctrl` `Shift` `]` |
| Send backward | `Ctrl` `[` |
| Bring forward | `Ctrl` `]` |
| Align top | `Ctrl` `Shift` `Up` |
| Align bottom | `Ctrl` `Shift` `Down` |
| Align left | `Ctrl` `Shift` `Left` |
| Align right | `Ctrl` `Shift` `Right` |
| Duplicate | `Ctrl` `D` or `Alt` `drag` |
| Lock/unlock selection | `Ctrl` `Shift` `L` |
| Undo | `Ctrl` `Z` |
| Redo | `Ctrl` `Shift` `Z` |
| Group selection | `Ctrl` `G` |
| Ungroup selection | `Ctrl` `Shift` `G` |
| Flip horizontal | `Shift` `H` |
| Flip vertical | `Shift` `V` |
| Show stroke color picker | `S` |
| Show background color picker | `G` |
| Show font picker | `Shift` `F` |
| Decrease font size | `Ctrl` `Shift` `<` |
| Increase font size | `Ctrl` `Shift` `>` |

On a Mac every `Ctrl` reads `Cmd` and every `Alt` reads `Option`, and send to back and bring to front are `Cmd` `Option` `[` and `Cmd` `Option` `]`. Windows adds `Ctrl` `Y` before redo's `Ctrl` `Shift` `Z`. Firefox lists only `Ctrl` `/` for the command palette. Copy to clipboard as PNG is shown only with the async clipboard or in Firefox, and toggle theme only when the host enables the theme action.

## Colour picker

`q w e r t / a s d f g / z x c v b` palette cells; `1–5` custom colours; `Shift+1–5` shades; `i` eyedropper; Tab cycles sections; Esc closes.
