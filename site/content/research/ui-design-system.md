+++
title = "UI and design system"
description = "CSS custom properties (light and dark), palettes, fonts, breakpoints, editor chrome regions, actions registry, keyboard shortcuts, i18n, property controls, icons."
weight = 30
[extra]
source_commit = "438d89861f53d8a90ad566113ecac1b83761098f"
source_repo = "https://github.com/excalidraw/excalidraw"
captured = "2026-09-28"
+++

**Scope.** Checkout at `<checkout>` (commit 438d8986). Paths below are relative to that root. `P` = `packages/excalidraw`, `C` = `packages/common/src`.

**This tree is newer than the Excalidraw most people know.** It has sticky notes, a bucket-fill tool, an "autoshape" (draw-shape) tool, a compact styles-panel mode and user Preferences. Mirror what is in this tree, not older docs.

---

## 1. Design tokens

### 1.1 CSS custom properties: theme (`P/css/theme.scss`)
All are set on `.excalidraw`. Dark overrides live under `&.theme--dark` (`theme.scss:184-278`).

| Token | Light | Dark |
|---|---|---|
| `--theme-filter` | `none` (:6) | `invert(93%) hue-rotate(180deg)` (:185) |
| `--button-destructive-bg-color` / `--button-destructive-color` | `$color-red-1` #ffe3e3 / `$color-red-9` #c92a2a (:7-8) | `#5a0000` / `$color-red-3` #ffa8a8 (:186-187) |
| `--button-gray-1/2/3` | gray-2 #e9ecef / gray-4 #ced4da / gray-5 #adb5bd (:9-11) | `#363636` / `#272727` / `#222` (:189-191) |
| `--mobile-action-button-bg` | `rgba(255,255,255,.35)` (:12) | `var(--island-bg-color)` (:192) |
| `--button-special-active-bg-color` | green-0 #ebfbee (:13) | `#204624` (:193) |
| `--dialog-border-color` | `var(--color-gray-20)` (:14) | `var(--color-gray-80)` (:194) |
| `--dropdown-icon` | inline SVG caret, no fill (:15) | same caret, fill `#ced4da` (:195) |
| `--focus-highlight-color` | blue-2 #a5d8ff (:16) | blue-6 #228be6 (:196) |
| `--icon-fill-color` | `var(--color-on-surface)` (:17) | (inherits) |
| `--icon-green-fill-color` | green-9 #2b8a3e (:18) | green-4 #69db7c (:197) |
| `--default-bg-color`, `--input-bg-color` | `#fff` (:19-20) | `#121212` (:198-199) |
| `--input-border-color` | gray-4 (:21) | `#2e2e2e` (:200) |
| `--input-hover-bg-color` | gray-1 #f1f3f5 (:22) | `#181818` (:201) |
| `--input-label-color` | gray-7 #495057 (:23) | gray-2 (:202) |
| `--island-bg-color` | `#ffffff` (:24) | `#232329` (:203) |
| `--island-bg-color-alt` | `#fff` (:25) | `hsl(240,12%,12%)` (:204) |
| `--keybinding-color` | `var(--color-gray-40)` (:26) | `var(--color-gray-60)` (:205) |
| `--link-color` / `-hover` / `-active` | blue-7 / blue-8 / blue-2 (:27-29) | `--link-color`: blue-4 (:206) |
| `--overlay-bg-color` | `rgba(12,12,14,.35)` (:30) | `rgba(12,12,14,.65)` (:207) |
| `--popup-bg-color` | `var(--island-bg-color)` (:31) | (inherits) |
| `--popup-secondary-bg-color` | gray-1 (:32) | `#222` (:208) |
| `--popup-text-color` / `-inverted-color` | `#000` / `#fff` (:33-34) | gray-4 / `#2c2c2c` (:209-210) |
| `--select-highlight-color` | blue-5 #339af0 (:35) | blue-4 (:211) |
| `--shadow-island`, `--shadow-island-stronger` | 3-layer shadows (:36-39) | (inherits) |
| `--button-hover-bg`, `--button-active-bg` | `var(--color-surface-high)` (:41-42) | (inherits) |
| `--button-active-border` | `var(--color-brand-active)` (:43) | (inherits) |
| `--default-border-color` | `var(--color-surface-high)` (:44) | (inherits) |
| `--default-button-size` / `--default-icon-size` | 2rem / 1rem (:46-47); 2.25rem / 1.25rem when `min-device-width:1921px` (:171-176) | |
| `--lg-button-size` / `--lg-icon-size` | 2.25rem / 1rem (:48-49); 2.375rem / 1.25rem at ≥1921px | |
| `--editor-container-padding` | 1rem (:50); 0.75rem on mobile (:167-169) | |
| `--mobile-action-button-size` | 2rem (:51) | |
| `--scrollbar-thumb` / `-hover` | button-gray-2 / button-gray-3 (:53-54) | gray-8 / gray-7 (:221-222) |
| `--color-slider-track` / `-thumb` | `hsl(240,100%,90%)` / gray-80 (:56-57) | `hsl(244,23%,39%)` (:224) |
| `--modal-shadow`, `--sidebar-shadow` | 6-layer shadows (:59-71) | modal re-declared, same values (:213-218) |
| `--avatar-border-color` | gray-20 (:65) | gray-85 (:219) |
| `--sidebar-border-color` / `--sidebar-bg-color` | surface-high / island-bg (:72-73) | (inherits) |
| `--library-dropdown-shadow`, `--chat-msg-shadow` | (:74-77) | |
| `--space-factor` | **0.25rem**, the spacing unit (:79) | |
| `--text-primary-color` | `var(--color-on-surface)` (:80) | |
| `--color-selection` | `#6965db` (:82) | `#b4b0ff` (:227) |
| `--color-icon-white` | `#fff` (:84) | gray-90 (:229) |
| `--color-primary` / `-darker` / `-darkest` / `-light` / `-light-darker` / `-hover` | #6965db / #5b57d1 / #4a47b1 / #e3e2fe / #d7d5ff / #5753d0 (:86-91) | #a8a5ff / #b2aeff / #beb9ff / #4f4d6f / #43415e / #bbb8ff (:231-236) |
| `--color-gray-10…100` | 10 #f5f5f5, 20 #ebebeb, 30 #d6d6d6, 40 #b8b8b8, 50 #999, 60 #7a7a7a, 70 #5c5c5c, 80 #3d3d3d, 85 #242424, 90 #1e1e1e, 100 #121212 (:93-103) | |
| `--color-disabled` | gray-40 (:105) | gray-70 (:238) |
| `--color-warning` / `-dark` / `-darker` / `-darkest` | #fceeca / #f5c354 / #f3ab2c / #ec8b14 (:107-110) | |
| `--color-text-warning` | text-primary (:111) | gray-80 (:240) |
| `--color-danger` / `-dark` / `-darker` / `-darkest` / `-text` | #db6965 / #db6965 / #d65550 / #d1413c / black (:113-117) | #ffa8a5 / #672120 / #8f2625 / #ac2b29 / #fbcbcc (:242-246) |
| `--color-danger-background` / `-icon-background` / `-color` / `-icon-color` | #fff0f0 / #ffdad6 / #700000 / #700000 (:119-122) | #fbcbcc / #672120 / #261919 / #fbcbcc (:248-251) |
| `--color-warning-background` / `-icon-background` / `-color` / `-icon-color` | (:124-127) | (:253-256) |
| `--color-muted*` (5 tokens) | (:129-133) | (:258-262) |
| `--color-promo` | primary (:135) | |
| `--color-success*` (7 tokens) | #cafccc … #6edf74 (:137-143) | |
| `--color-logo-icon` / `--color-logo-text` | primary / #190064 (:145-146) | text: #e2dfff (:264) |
| `--border-radius-md` / `--border-radius-lg` | **0.375rem / 0.5rem** (:148-149) | |
| `--color-surface-high` / `-mid` / `-low` / `-lowest` | #f1f0ff / #f6f6f9 / #ececf4 / #ffffff (:151-154) | #2e2d39 / hsl(240 6% 10%) / hsl(240,8%,15%) / hsl(0,0%,7%) (:266-269) |
| `--color-on-surface` | #1b1b1f (:155) | #e3e3e8 (:270) |
| `--color-brand-hover` / `--color-brand-active` | #5753d0 / #4440bf (:156,159) | #bbb8ff / #d0ccff (:271,274) |
| `--color-on-primary-container` / `--color-surface-primary-container` | #030064 / #e0dfff (:157-158) | #e0dfff / #403e6a (:272-273) |
| `--color-border-outline` / `-outline-variant` | #767680 / #c5c5d0 (:160-161) | #8e8d9c / #46464f (:275-276) |
| `--color-badge` / `--background-color-badge` | #0b6513 / #d3ffd2 (:164-165) | |

**SCSS palette** (open-color subset): red/gray/green/blue variables at `P/css/variables.module.scss:1-24`.

**SCSS mixins**
- `isMobile` targets `.excalidraw--mobile` (`variables.module.scss:26-30`).
- `toolbarButtonColorStates`: a checked tool uses surface-primary-container bg and on-primary-container icon (:34-105).
- `outlineButtonStyles`: 1px border, radius-lg, 0.625rem padding, `--button-*` override hooks (:107-173).
- Also `outlineButtonIconStyles` (:181-192), `avatarStyles` (1.5rem circle, :194-237) and `filledButtonOnCanvas` (:239-252).

**Global tokens** (`P/css/styles.scss`)
- `:root` z-index scale and safe-area insets `--sab/--sal/--sar/--sat` (:4-33).
- `--ui-font` (:42-43).
- `--viewport-status-frame-border-width`: 0, or 4px with `.excalidraw--viewport-status-border` (:44-48).
- Buttons use font-size 0.8333rem (:65).
- Focus ring is `box-shadow: 0 0 0 1px var(--color-brand-hover)` (:243-250).

**Component-scoped tokens.** 30 component SCSS files declare local vars. Examples: `--RadioGroup-*`, `--Switch-*`, `--ExcTextField--*`, `--logo-icon--*`, `--userlist-*`, and color-picker `--size`/`--radius` (e.g. `P/components/ColorPicker/ColorPicker.scss:52-53`).

**Spacing helpers**
- Island: bg `--island-bg-color`, `--shadow-island`, radius-lg, padding = `--padding` × `--space-factor` (`P/components/Island.scss:2-13`).
- Stack: gap = `--space-factor` × `--gap` (`P/components/Stack.scss:3-5`, `Stack.tsx:24`).

**Dialog widths.** small 550, regular 800, wide 1024 px (`P/components/Dialog.tsx:34-47`).

**Sidebar width.** `RIGHT_SIDEBAR_WIDTH = 302` px (`P/components/App.viewport.ts:62`), exposed as `--right-sidebar-width` (`App.tsx:2453`).

### 1.2 z-index layering (`styles.scss:5-27`)
| Layer | z-index |
|---|---|
| canvas | 1 |
| interactiveCanvas | 2 |
| svgLayer / wysiwyg / canvasButtons | 3 |
| layerUI | 4 |
| viewportStatusFrame / eyeDropperBackdrop | 5 |
| eyeDropperPreview | 6 |
| hyperlinkContainer | 7 |
| cursorHint | 8 |
| ui-bottom | 60 |
| ui-context-menu | 90 |
| ui-styles-popup / ui-top | 100 |
| ui-main-menu | 110 |
| ui-library | 120 |
| fileDropOverlay | 130 |
| modal | 1000 |
| popup | 1001 |
| toast | 999999 |

LoadingMessage uses 999 (`P/css/app.scss:17`).

### 1.3 Fonts
**UI font**
- `--ui-font: Assistant, system-ui, BlinkMacSystemFont, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif` (`styles.scss:42-43`).
- Assistant weights 400/500/600/700 as woff2 (`P/fonts/fonts.css:5-35`).

**Canvas font IDs** (`C/constants.ts:140-151`)
- Virgil 1, Helvetica 2, Cascadia 3, (4 reserved), Excalifont 5, Nunito 6, "Lilita One" 7, "Comic Shanns" 8, "Liberation Sans" 9, Assistant 10.
- Fallback IDs: Xiaolai 100, sans-serif 998, monospace 999, Segoe UI Emoji 1000 (:158-167).
- Fallback chains: Excalifont → [Xiaolai, generic, Segoe UI Emoji]; others → [generic, emoji] (:182-197).
- Cascadia and Comic Shanns use the monospace generic (:169-180).
- Default font is Excalifont (:268). Default size 20 (:223).
- `FONT_SIZES` sm 16 / md 20 / lg 28 / xl 36 (:122-127).

**Registration.** `Fonts.init()` registers Cascadia, Comic Shanns, Excalifont, Helvetica (local), Liberation Sans, Lilita One, Nunito, Virgil, plus the Xiaolai and Emoji fallbacks (`P/fonts/Fonts.ts:375-416`).

**Deprecated fonts** (hidden from the picker unless already used in the scene)
- Virgil, Helvetica and Cascadia are marked `deprecated` in `C/font-metadata.ts` (~:68-94).
- Liberation Sans and Assistant are `private` (:96-113).
- The picker filter is at `P/components/FontPicker/FontPickerList.tsx:139-176`.

**Font files** (`P/fonts/*`)
| Font | Files | Notes | License |
|---|---|---|---|
| Assistant | 4 woff2 | | none in repo |
| Cascadia | CascadiaCode-Regular.woff2 | | none in repo |
| ComicShanns | 4 subset woff2 + `.sfd` source | subsets made with cn-font-split | MIT (`ComicShanns/index.ts:11-14,45`) |
| Excalifont | 7 subset woff2 | designer dizajndesign.sk | SIL OFL 1.1 (`Excalifont/index.ts:11-117`) |
| Liberation | LiberationSans-Regular.woff2 | | none in repo |
| Lilita | latin + latin-ext woff2 | Google Fonts ranges (`Lilita/index.ts:5-15`) | none in repo |
| Nunito | 5 woff2 (cyrillic, cyrillic-ext, latin, latin-ext, vietnamese) | `Nunito/index.ts:5-20` | none in repo |
| Virgil | Virgil-Regular.woff2 | | none in repo |
| Xiaolai (CJK) | ~209 subset woff2 | cn-font-split | SIL OFL 1.1 (`Xiaolai/index.ts:216-232`) |
| Helvetica, Emoji | none | `LOCAL_FONT_PROTOCOL`, system fonts (`Helvetica/index.ts:7`, `Emoji/index.ts:7`) | n/a |

No LICENSE/OFL files exist under `P/fonts`. The repo root `LICENSE` is the project license.

### 1.4 Breakpoints and device detection (`C/editorInterface.ts`)
**Constants**
- `MQ_MAX_MOBILE = 599`, `MQ_MAX_WIDTH_LANDSCAPE = 1000`, `MQ_MAX_HEIGHT_LANDSCAPE = 500` (:19-22).
- `MQ_MIN_TABLET = 600`, `MQ_MAX_TABLET = 1180` (:25-26).
- `MQ_MIN_WIDTH_DESKTOP = 1440`, not used for form-factor detection (:29).
- `MQ_RIGHT_SIDEBAR_MIN_WIDTH = 1229` (:32).

**Rules**
- Phone: `width <= 599 || (height < 500 && width < 1000)` (`isMobileBreakpoint`, :64-69).
- Tablet: `min(w,h) >= 600 && max(w,h) <= 1180` (:71-79).
- `getFormFactor` returns phone, tablet or desktop (:137-150).
- `EditorInterface` fields: `{formFactor, desktopUIMode: compact|full, userAgent{isMobileDevice, platform}, isTouchScreen, canFitSidebar, isLandscape}` (:3-13).
- Styles-panel mode: phone → "mobile", tablet → "compact", desktop → the stored `desktopUIMode` (:152-164). The preference is stored in localStorage key `excalidraw.desktopUIMode` (:16, :186-222).
- UA sniffing (isDarwin, isWindows, isAndroid, isFirefox, isChrome, isSafari, isIOS, isBrave) is at :37-54. `isMobileOrTablet` uses UA client hints with pointer/hover media fallbacks (:81-135).

**Where App.tsx applies it**
- `refreshEditorInterface` measures the container rect. `canFitSidebar = width > dockedSidebarBreakpoint ?? 1229`. The host can override via `UIOptions.getFormFactor` (`P/components/App.tsx:3752-3787`).
- Root classes: `excalidraw--mobile` (phone), `--mobile-toolbar`, `--view-mode`, `--zen-mode`, `--ui-hidden`, `--viewport-status-border`, etc. (`App.tsx:2415-2447`).

**CSS-only breakpoints**
- Top menu grid is `1fr 2fr 1fr` with 1rem gap, and `1fr 1fr 1fr` with 3rem gap at ≥1536px (`styles.scss:408-423`).
- ToolIcon shrinks at ≤450px and ≤379px (`ToolIcon.scss:180-186`).
- `$verticalBreakpoint: 861px` (TTDDialog, Chat, CommandPalette SCSS) and `$fullScreenModalBreakpoint: 600px` (`TTDDialog.scss:4-5`).

---

## 2. Color palettes (`C/colors.ts`)
**Structure**
- `ColorTuple` is a 5-shade tuple (:179).
- `COLORS_PER_ROW = 5`, `MAX_CUSTOM_COLORS_USED_IN_CANVAS = 5` (:185-186).
- Default stroke shade index 4, background shade index 1 (:190-191).

**`COLOR_PALETTE`** (:193-212). Open-color shades are indexes [0,2,4,6,8]; bronze uses radix [3,5,7,9,11].
| Name | Shades |
|---|---|
| transparent | `transparent` |
| black | `#1e1e1e` |
| white | `#ffffff` |
| gray | #f8f9fa #e9ecef #ced4da #868e96 #343a40 |
| red | #fff5f5 #ffc9c9 #ff8787 #fa5252 #e03131 |
| pink | #fff0f6 #fcc2d7 #f783ac #e64980 #c2255c |
| grape | #f8f0fc #eebefa #da77f2 #be4bdb #9c36b5 |
| violet | #f3f0ff #d0bfff #9775fa #7950f2 #6741d9 |
| blue | #e7f5ff #a5d8ff #4dabf7 #228be6 #1971c2 |
| cyan | #e3fafc #99e9f2 #3bc9db #15aabf #0c8599 |
| teal | #e6fcf5 #96f2d7 #38d9a9 #12b886 #099268 |
| green | #ebfbee #b2f2bb #69db7c #40c057 #2f9e44 |
| yellow | #fff9db #ffec99 #ffd43b #fab005 #f08c00 |
| orange | #fff4e6 #ffd8a8 #ffa94d #fd7e14 #e8590c |
| bronze | #f8f1ee #eaddd7 #d2bab0 #a18072 #846358 |

**Top-picks strip.** `COLOR_TOP_PICKS_SLOTS = 5` (:236).
| Picks | Colors |
|---|---|
| Stroke (:239-245) | black, red[4], green[4], blue[4], yellow[4] |
| Background (:248-254) | transparent, red[1], green[1], blue[1], yellow[1] |
| Bucket fill (:259-265) | white, red[1], green[1], blue[1], yellow[1] |
| Sticky note background (:268-281) | `#ffdf6b`, pink[1], green[1], blue[1], orange[1] |
| Canvas background (:284-294) | #ffffff, #f8f9fa, #f5faff, #fffce8, #fdf8f6 |

**Full palette grid (5 × 3)**
- Stroke and background palettes (:299-319): row 1 is transparent, white, gray, black, bronze.
- Rows 2-3 (:325-339): cyan, blue, violet, grape, pink / green, teal, yellow, orange, red.
- Palette choice per target (regular vs sticky) is in `P/actions/colorTargets.ts:146-174`.

**Element defaults** (`C/constants.ts:514-532`): stroke `#1e1e1e`, bg transparent, fill `solid`, strokeWidth 2 (medium), strokeStyle solid, roughness 1 (artist), opacity 100.

**Dark mode.** Canvas colors are transformed with CSS-equivalent `invert(93%) hue-rotate(180deg)` math (`applyDarkModeFilter`, `colors.ts:86-122`; reverse at :141-160; `DARK_THEME_FILTER` at `C/constants.ts:204`).

**Other color helpers.** `isColorDark` uses YIQ with threshold 160 (:416-434). `COLOR_OUTLINE_CONTRAST_THRESHOLD = 240` (:408).

---

## 3. Editor chrome

**Overall layout** (`P/components/LayerUI.tsx`)
- Desktop and tablet use `FixedSideContainer` + `.App-menu_top`, a 3-column grid (`LayerUI.tsx:312-438`):
  - Left column: main-menu trigger, then the styles panel (:315-348).
  - Centre column: the shapes toolbar (:349-399).
  - Right column: user list, `renderTopRightUI`, library sidebar trigger, Stats (:400-436).
- Then Footer (:657-664), a floating toast / "scroll back to content" stack (:665-699), and sidebars (:701).
- Phone renders `MobileMenu` instead (:612-629).
- Dialogs are mounted at :577-611: Help, ElementLink, ImageExport, JSONExport, PasteChart, TTD, OverwriteConfirm, ActiveConfirm, ErrorDialog, EyeDropper.

### 3.1 Top toolbar (`P/components/Toolbar.tsx`, tool data in `P/components/Tools.tsx`)
**Tool registry.** `TOOLS` is the single source of truth for icon, letter key, number key, `fillable` and `toggle` (`Tools.tsx:69-160`).
| Tool | Keys | Notes |
|---|---|---|
| hand | H | toggle |
| selection | V / 1 | |
| rectangle | R / 2 | |
| diamond | D / 3 | |
| ellipse | O / 4 | |
| arrow | A / 5 | |
| line | L / 6 | |
| freedraw | P or X / 7 | |
| text | T / 8 | |
| stickynote | N | |
| image | 9 | |
| eraser | E / 0 | toggle |
| frame | F | |
| autoshape | Shift+X | |
| embeddable | none | |
| laser | K | |
| bucketfill | B | |
| lasso | none | reached via the selection key when preferred |

- `findShapeByKey` is caps-lock insensitive (:192-223).
- Tooltip text is "Label — R or 2" (`getToolShortcut`, :184-190).

**Desktop toolbar order** (`Toolbar.tsx:262-323`)
1. PenModeButton (not in compact mode)
2. LockButton
3. divider
4. Hand
5. Selection, or Lasso, or a Selection/Lasso popover in compact mode
6. Rectangle, Diamond, Ellipse, Arrow, Line
7. Freedraw (a Freedraw/Autoshape popover in compact mode)
8. Text, StickyNote, Eraser
9. divider
10. "Extra tools" dropdown

**Extra tools dropdown** (`Toolbar.tsx:54-220`)
- Image (9), Frame (F), Embeddable, Autoshape (Shift+X), Laser (K), Bucket fill (B), Lasso (full mode only).
- "Generate" group: Text-to-diagram (tunnel), Mermaid→Excalidraw, Magic frame (AI badge).

**Other toolbar behavior**
- A HintViewer is rendered inside the toolbar island (:255-260).
- While collaborating, a separate Laser button island is shown (`LayerUI.tsx:374-393`).

### 3.2 Left styles / properties panel (`P/components/Actions.tsx`)
**Visibility rules.** `getShapeActionPredicates` decides which controls show (`P/components/shapeActionPredicates.ts:88-189`).

**Full mode** (`SelectedShapeActions`, `Actions.tsx:129-217`), in this order:
1. stroke color
2. background color
3. fill style
4. stroke width
5. stroke style
6. freedraw pressure mode
7. sloppiness
8. roundness (edges)
9. arrow type
10. font family / font size / text align
11. vertical align
12. arrowheads
13. opacity
14. Layers fieldset: sendToBack, sendBackward, bringForward, bringToFront (:65-79)
15. Align fieldset: left / center-H / right (+ distribute-H), top / center-V / bottom (+ distribute-V), with order mirrored for RTL (:85-123)
16. Actions fieldset: duplicate, delete, group, ungroup, link, crop, line editor (:201-214)

With the bucket-fill tool active, the panel shows only bucket background color, fill style and opacity (:150-158).

**Compact mode** (tablet or desktop "compact"; `CompactShapeActions`, :605-717)
- Always shown: stroke swatch, background swatch, freedraw-mode cycle button.
- Popovers:
  - "stroke" (adjustments icon): fill, width, style, sloppiness, roundness, opacity (:219-305).
  - Arrow-type popover (:307-401).
  - Line editor button.
  - Font family and a text-properties popover: size, align, vertical align (:403-487).
  - Duplicate and delete buttons.
  - "…" popover: layers, align, group/ungroup, link, crop (:489-581).
- The panel island max height is `appState.height - 166` (`LayerUI.tsx:257-296`).

**Mobile** (`MobileShapeActions`, `Actions.tsx:724-875`)
- A horizontal bar with the same combined popovers plus undo/redo.
- Duplicate and delete are promoted out of the popover when width allows: 32px buttons, 6px gap, minimum 9 actions (:747-762).

### 3.3 Main (hamburger) menu
**Trigger.** `MainMenu` is a DropdownMenu with `HamburgerMenuIcon`, toggling `openMenu:"canvas"` (`P/components/main-menu/MainMenu.tsx:40-79`). On phones it also lists collaborators (:65-76).

**Default package menu** (`LayerUI.tsx:111-136`)
- LoadScene, SaveToActiveFile, Export, SaveAsImage, SearchMenu, Help, ClearCanvas
- Group "Excalidraw links": Socials
- ToggleTheme, ChangeCanvasBackground

**Item implementations** (`P/components/main-menu/DefaultItems.tsx`)
| Item | Shortcut / action | Lines |
|---|---|---|
| LoadScene | Ctrl/Cmd+O, overwrite confirm | :70-110 |
| SaveToActiveFile | Ctrl/Cmd+S | :113-130 |
| SaveAsImage | Ctrl/Cmd+Shift+E | :133-147 |
| CommandPalette | Ctrl/Cmd+/ | :150-169 |
| SearchMenu | Ctrl/Cmd+F | :172-190 |
| Help | ? | :193-209 |
| ClearCanvas | confirm dialog | :212-232 |
| ToggleTheme | item, or Light/Dark/System radio | :235-320 |
| ChangeCanvasBackground | color picker | :323-352 |
| Export | JSON export dialog | :355-370 |
| Socials | GitHub, X, Discord | :373-401 |
| LiveCollaborationTrigger | | :404-424 |

**Preferences submenu** (:659-692): box-selection mode (contain/overlap), input device (trackpad/mouse), tool lock (Q), object snap (Alt+S), grid (Ctrl+'), zen (Alt+Z), view mode (Alt+R), element properties/stats (Alt+/), arrow binding, midpoint snapping, show hints.

**Hosted app menu** (`excalidraw-app/components/AppMainMenu.tsx:28-87`) adds Live collaboration, Command palette (highlighted), Excalidraw+ link, Sign in/up, Preferences, System-theme radio, and a Language list.

### 3.4 Footer: zoom, undo, help (`P/components/footer/Footer.tsx`)
- Left: ZoomActions (zoomOut, resetZoom, zoomIn; `Actions.tsx:877-889`) and UndoRedoActions with tooltips (`Actions.tsx:891-910`) (`Footer.tsx:37-65`).
- Centre: `FooterCenterTunnel` (:66).
- Right: HelpButton (:67-85).
- Exit-zen-mode button (:86-91).

### 3.5 Help dialog (`P/components/HelpDialog.tsx`)
- Header links: Docs, Blog, GitHub issues, YouTube (:21-60).
- Three shortcut islands: Tools (:145-261), View (:262-335), Editor (:336-516). Full list in §6.

### 3.6 Library / search sidebar
- `DefaultSidebar` has tab triggers for Search (`CANVAS_SEARCH_TAB`) and Library (`LIBRARY_SIDEBAR_TAB`) (`P/components/DefaultSidebar.tsx:99-118`). The search tab forces docking (:77).
- Trigger uses `sidebarRightIcon` with the title "Library" (`LayerUI.tsx:481-499`).
- The sidebar docks only if `canFitSidebar` (`LayerUI.tsx:463-468`).
- Library header menu: Load, Export, Publish, Reset/Remove (`P/components/LibraryMenuHeaderContent.tsx:206-238`).
- Library item sections: Personal library, Excalidraw library, library search (`LibraryMenuItems.tsx:271-404`).
- Search menu shows Frames and Texts result groups (`SearchMenu.tsx:340-519`).

### 3.7 Mobile layout (`P/components/MobileMenu.tsx`, `MobileToolbar.tsx`)
- Top bar: main menu and top-left UI on the left; PenMode, library trigger and Exit-view-mode on the right (`MobileMenu.tsx:66-116, 199-201`).
- Bottom bar holds MobileShapeActions and a toolbar island (`MobileMenu.tsx:160-187`). CSS `.App-bottom-bar` is max 450px wide and centred (`styles.scss:322-346`).

**Mobile toolbar** (`MobileToolbar.tsx`)
- Buttons are 36px with 4px gap (:104-108).
- Order from :168:
  1. Hand
  2. Selection/Lasso popover
  3. Freedraw/Autoshape popover
  4. Eraser
  5. Rect/Diamond/Ellipse popover
  6. Arrow/Line popover
  7. Text, Image, Frame when width allows
  8. "…" dropdown: text, image, stickynote, frame, embeddable, autoshape, laser, bucketfill, Generate (TTD, Mermaid, Magic)
- Default tool on phone is lasso (`App.tsx:3663`).
- EyeDropper is not shown on phone (`LayerUI.tsx:514-516`).
- Element context menu has no z-order items unless desktop (`App.tsx:13885-13894`).

### 3.8 Welcome screen (`P/components/welcome-screen/*`)
- `WelcomeScreen.Center` shows Logo, Heading, and menu items LoadScene and Help, plus optional Live collaboration and links (`WelcomeScreen.Center.tsx:91-194`).
- `WelcomeScreen.Hints`: MenuHint, ToolbarHint, HelpHint (`WelcomeScreen.Hints.tsx:16-44`).
- Hosted app usage: `excalidraw-app/components/AppWelcomeScreen.tsx:49-79`.

### 3.9 Stats panel (`P/components/Stats/index.tsx`)
- General section: shapes, width, height, canvas grid step (:195-236), then custom stats (:238).
- Element properties, single element: type label, uncropped dims, X/Y position, width/height, angle, font size (:248-345).
- Element properties, multiple: shape count, MultiPosition, MultiDimension, MultiAngle, MultiFontSize (:362-415).
- Section open state is a bitmask `STATS_PANELS` {generalStats: 1, elementProperties: 2} (`C/constants.ts:584`).

### 3.10 Context menu (`App.tsx:13835-13936`; renderer `P/components/ContextMenu.tsx`, shortcuts via `getShortcutFromShortcutName` :119)
**Canvas**
- Paste | copyAsPng, copyAsSvg, copyText | selectAll, unlockAllElements | gridMode, objectsSnapMode, arrowBinding, midpointSnapping, zenMode, viewMode, stats.
- In view mode: copyAsPng/Svg, grid, zen, viewMode, stats.

**Element**
- cut, copy, paste
- selectAllElementsInFrame, removeAllElementsFromFrame, wrapSelectionInFrame
- cropEditor
- copyAsPng, copyAsSvg, copyText
- copyStyles, pasteStyles
- group, autoResize, unbindText, bindText, wrapTextInContainer, ungroup
- addToLibrary
- z-order (desktop only)
- flipH, flipV
- toggleLinearEditor
- hyperlink, copyElementLink
- duplicate, toggleElementLock
- delete

### 3.11 Hints and tooltips
- `HintViewer` shows about 30 contextual hint keys, e.g. `hints.arrowTool`, `freeDraw`, `text`, `resize`, `rotate`, `lineEditor_*`, `canvasPanning`, `createFlowchart` (`P/components/HintViewer.tsx:53-246`).
- `CursorHint.tsx` provides cursor-side hints.
- `Tooltip.tsx` is a portal tooltip with bottom/top position (:6-45).
- Hints can be turned off via the `showHints` preference (`DefaultItems.tsx:566-581`).

### 3.12 Color picker (`P/components/ColorPicker/*`)
- Types: `canvasBackground | elementBackground | elementStroke` (`colorPickerUtils.ts:104-107`).
- Trigger shows the top-picks strip plus a swatch button of 1.375rem with 4px radius (`ColorPicker.scss:50-66`).
- Popup sections: Most-used custom colors, Colors (palette grid), Shades, Hex code input with eyedropper, and a top-picks drag-and-drop tip (`Picker.tsx:152-226`, `ColorPicker.tsx:111-120`).

**Picker keys**
- Palette colors: q w e r t / a s d f g / z x c v b (`colorPickerUtils.ts:38-42`).
- `1`–`5`: custom colors (`keyboardNavHandlers.ts:87-94`).
- `Shift+1`–`Shift+5`: shades (:76-84).
- `i`: eyedropper; Esc; Tab cycles sections; arrow keys navigate (:156-305).

### 3.13 Font picker (`P/components/FontPicker/*`)
- Top picks (3 slots): Excalifont "Hand-drawn", Nunito "Normal", Comic Shanns "Code" (`FontPicker.tsx:42-63`; `FONT_TOP_PICKS_SLOTS = 3` at `C/constants.ts:273`).
- List groups "Scene fonts" and "Available fonts", with an "old" badge for deprecated fonts and a search box (`FontPickerList.tsx:139-176, 361-425`).
- Opened with Shift+F (`App.tsx:6022-6050`).

### 3.14 Command palette (`P/components/CommandPalette/CommandPalette.tsx`)
- Toggle: Ctrl/Cmd+/ or Ctrl/Cmd+Shift+P (:141-148). Ctrl/Cmd+P shows a toast hint instead (`App.tsx:5679-5692`).
- Category order: App, Export, Editor, Tools, Elements, Links, then Library (:87-114).

**Elements category** (:310-341)
- group, ungroup, cut, copy, delete, wrapSelectionInFrame
- copyStyles, pasteStyles
- bringToFront, bringForward, sendBackward, sendToBack
- align ×6, duplicate, flipH, flipV
- zoomToFitSelection, zoomToFitSelectionInViewport
- increaseFontSize, decreaseFontSize
- toggleLinearEditor, cropEditor, togglePolygon
- hyperlink, copyElementLink, linkToElement

**Editor category** (:360-376): undo, redo, zoomIn, zoomOut, resetZoom, zoomToFit, zen, viewMode, grid, snap, shortcuts, selectAll, toggleElementLock, unlockAll, stats.

**Export category** (:378-383): save, saveToDisk, copyAsPng, copyAsSvg.

**Other items**
- Theme toggle, library, search, shape switch, stroke/background/canvas colors, each tool, tool lock (:390-579).
- Library items as commands (:223-248).

---

## 4. Icons (`P/components/icons.tsx`, 2593 lines)
- 209 `export const`s. Excluding the `iconFillColor` and `createIcon` helpers, that is about 207 icon exports, including two raw path exports: `bucketFillIconSvgPaths` and `eyeDropperIconSvgPaths`.
- All are inline React SVG built with `createIcon(d | children, opts)`. Output is `<svg aria-hidden role="img" viewBox="0 0 w h">`, with a string path drawn as `fill="currentColor"`. The default viewBox is 512 (FontAwesome-style) (`icons.tsx:27-51`).
- The header comment says the icons come from FontAwesome (`icons.tsx:1-4`), but most are Tabler icons.
- Size presets:
  - `tablerIconProps`: 24×24, stroke 2, currentColor, round caps (:53-61); about 106 uses.
  - `modifiedTablerIconProps`: 20×20 (:63-70); about 48 uses.
  - `arrowheadPreviewIconProps`: 40×20 (:72-75).
- Icons can be RTL-mirrored (`mirror` → `.rtl-mirror`, :22-25).
- Rendered size comes from CSS: `--default-icon-size` 1rem (1.25rem ≥1921px); `.ToolIcon__icon` uses `--default-button-size` (`P/components/ToolIcon.scss:48-66`).
- Icon color comes from `--icon-fill-color`. `GroupIcon`/`UngroupIcon` use `handlerColor` (#fff light, #1e1e1e dark) (`icons.tsx:16-19`).

---

## 5. Actions registry (`P/actions/*`)
**Mechanics**
- `register()` appends to a global list (`P/actions/register.ts:1-15`).
- The `ActionName` union has 99 names (`P/actions/types.ts:45-144`).
- 95 are registered. `toggleFullScreen`, `elementStats`, `createContainerFromText` and `commandPalette` are in the union with no `register()` call (verified by diffing names).
- `ActionManager.handleKeyDown` sorts by `keyPriority` then runs `keyTest` (`P/actions/manager.tsx:92-99`).
- `CTRL_OR_CMD` = metaKey on Darwin, ctrlKey elsewhere (`C/keys.ts:39`).

| File | Actions (keyTest) |
|---|---|
| actionAddToLibrary.ts | addToLibrary (:11) |
| actionAlign.tsx | alignTop Ctrl+Shift+↑ (:80,96); alignBottom Ctrl+Shift+↓ (:114,130); alignLeft Ctrl+Shift+← (:148,164); alignRight Ctrl+Shift+→ (:182,198); alignVerticallyCentered (:216); alignHorizontallyCentered (:246) |
| actionBoundText.tsx | unbindText (:61), bindText (:125), wrapTextInContainer (:259) |
| actionCanvas.tsx | changeViewBackgroundColor (:48); clearCanvas (:85); zoomIn Ctrl/Shift + `=`/Numpad+ (:130,176); zoomOut Ctrl/Shift + `-` (:182,228); resetZoom Ctrl/Shift + 0 (:234,286); zoomToFitSelectionInViewport Shift+2 (:308,343); zoomToFitSelection Shift+3 (:351,385); zoomToFit Shift+1 (:393,421); toggleTheme Alt+Shift+D (:429,457) |
| actionClipboard.tsx | copy (:24, native event); paste (:56, native); cut Ctrl+X (:113,121); copyAsSvg (:125); copyAsPng Alt+Shift+C (:193,250); copyText (:255) |
| actionCropEditor.tsx | cropEditor (:14) |
| actionDeleteSelected.tsx | deleteSelectedElements Backspace/Delete without Ctrl (:209,305) |
| actionDeselect.ts | deselect Esc (:65,130) |
| actionDistribute.tsx | distributeHorizontally Alt+H (:74,87); distributeVertically Alt+V (:105,118) |
| actionDuplicateSelection.tsx | duplicateSelection Ctrl+D (:35,117) |
| actionElementLink.ts | copyElementLink (:17), linkToElement (:74) |
| actionElementLock.ts | toggleElementLock Ctrl+Shift+L (:26,147); unlockAllElements (:161) |
| actionExport.tsx | changeProjectName (:38), changeExportScale (:58), changeExportBackground (:72), changeExportEmbedScene (:94), saveToActiveFile Ctrl+S (:254,324), saveFileToDisk Ctrl+Shift+S (:329,375), loadScene Ctrl+O (:394,428), exportWithDarkMode (:434) |
| actionFinalize.tsx | finalize: Esc while line-editing, or Esc/Enter during a multi-point element (:54,421) |
| actionFlip.ts | flipHorizontal Shift+H (:30,51); flipVertical Shift+V (:55,76) |
| actionFrame.ts | selectAllElementsInFrame (:37), removeAllElementsFromFrame (:74), updateFrameRendering (:105), wrapSelectionInFrame (:126) |
| actionGroup.tsx | group Ctrl+G (:87,202); ungroup Ctrl+Shift+G (:218,306) |
| actionHistory.tsx | undo Ctrl+Z (:70,79); redo Ctrl+Shift+Z or Ctrl+Y (:109,118) |
| actionLinearEditor.tsx | toggleLinearEditor (:28; Ctrl+Enter handled in App.tsx:5943-5959); togglePolygon (:107) |
| actionLink.tsx | hyperlink Ctrl+K (:20,40) |
| actionMenu.tsx | toggleShortcuts `?` (:10,34) |
| actionNavigate.tsx | goToCollaborator (:22) |
| actionProperties.tsx | changeStrokeColor (:361), changeBackgroundColor (:440), changeBucketFillBackgroundColor (:556), changeFillStyle (:607), changeStrokeWidth (:709), changeSloppiness (:767), changeFreedrawMode (:821), changeStrokeStyle (:904), changeOpacity (:957), changeFontSize (:1001), decreaseFontSize Ctrl+Shift+`<`/`,` (:1096,1110), increaseFontSize Ctrl+Shift+`>`/`.` (:1121,1133), changeFontFamily (:1164), changeTextAlign (:1548), changeVerticalAlign (:1649), changeRoundness (:1749), changeArrowhead (:1948), changeArrowProperties (:2039), changeArrowType (:2058) |
| actionSelectAll.ts | selectAll Ctrl+A (:22,69) |
| actionStyles.ts | copyStyles Ctrl+Alt+C (:52,78); pasteStyles Ctrl+Alt+V (:83,229) |
| actionTextAutoResize.ts | autoResize (:23) |
| actionToggleArrowBinding.tsx | arrowBinding (:6) |
| actionToggleGridMode.tsx | gridMode Ctrl+' (:10,33) |
| actionToggleMidpointSnapping.tsx | midpointSnapping (:6) |
| actionToggleObjectsSnapMode.tsx | objectsSnapMode Alt+S (:10,32) |
| actionToggleSearchMenu.ts | searchMenu Ctrl+F (:15,57) |
| actionToggleShapeSwitch.tsx | toggleShapeSwitch (:14) |
| actionToggleStats.tsx | stats Alt+/ (:10,26) |
| actionToggleViewMode.tsx | viewMode Alt+R (:10,34) |
| actionToggleZenMode.tsx | zenMode Alt+Z (:10,34) |
| actionZindex.tsx | sendBackward Ctrl+[ (keyPriority 40) (:24,36-40); bringForward Ctrl+] (:54,66-70); sendToBack Ctrl+Alt+[ on Mac, Ctrl+Shift+[ elsewhere (:84,96-103); bringToFront same pattern with ] (:121,134-141) |

**Display labels.** `P/actions/shortcuts.ts:58-116` maps shortcut names to display strings. Extra non-action entries: saveScene, imageExport (Ctrl+Shift+E), commandPalette (Ctrl+/, Ctrl+Shift+P), searchMenu, toolLock (Q).

---

## 6. Keyboard shortcuts
**Key constants and formatting**
- `C/keys.ts` defines `CODES` (physical codes, :5-28) and `KEYS` (:30-87).
- `matchKey` falls back to `event.code` for Z/Y on non-Latin layouts (:92-137).
- Shift = maintain aspect ratio / discrete rotation; Alt = resize from centre (:145-153).
- `getShortcutKey` localizes Alt/Option, Shift, Enter, Ctrl/Cmd, Esc, Space and Delete via `keys.*` locale strings (`P/shortcut.ts:5-19`).

**Handled in `App.onKeyDown`** (`App.tsx:5585-6073`) rather than by actions
| Key | Behavior | Lines |
|---|---|---|
| Tab / Shift+Tab | convert element type | :5640-5672 |
| `?` | Help dialog | :5725-5729 |
| Ctrl+Shift+E | image export dialog | :5730-5738 |
| PgUp / PgDn (Shift = horizontal) | page scroll | :5740 |
| Alt | bucket-fill temporary eyedropper / bind mode | :5752-5762 |
| Tool letters and numbers | pressing A again cycles arrow type sharp→round→elbow; B again cycles bucket color | :5780-5841 |
| Q | tool lock | :5842 |
| Ctrl held | disables binding | :5853-5869 |
| Arrow keys | nudge 1px, Shift = 5px, or grid size | :5871-5942 |
| Enter | edit text / enter frame / line editor (Ctrl+Enter) | :5943-5987 |
| Space | pan | :5989-5993 |
| S / G | open stroke / background picker | :5995-6020 |
| Shift+F | font picker | :6022-6050 |
| Ctrl+Backspace/Delete | clear-canvas confirm | :6052-6057 |
| I, Shift+S, Shift+G | eyedropper | :6059-6071 |

**Help dialog list** (`HelpDialog.tsx`)
- Tools (:149-260): H; V/1; R/2; D/3; O/4; A/5; L/6; P/7; T/8; N; 9; E/0; F; K; B; eyedropper I / Shift+S / Shift+G; Ctrl+Enter edit points; Enter edit text; Enter or Shift+Enter newline; Esc or Ctrl+Enter finish text; A/L + clicks for curved arrow/line; crop start/finish; Q lock; Ctrl prevent binding; Ctrl+K link; Tab/Shift+Tab convert.
- View (:266-334): Ctrl+ `+` / `-` / 0; Shift+1; Shift+2; PgUp/PgDn; Shift+PgUp/PgDn; Alt+Z; Alt+S; Ctrl+'; Alt+R; Alt+Shift+D; Alt+/; Ctrl+F; Ctrl+/ or Ctrl+Shift+P.
- Editor (:340-515): Ctrl+Arrow creates a flowchart; Alt+Arrow navigates it; Space+drag or wheel+drag pans; Ctrl+Delete; Delete; cut/copy/paste; Ctrl+Shift+V plain paste; Ctrl+A; Shift+click; Ctrl+click deep select; Ctrl+drag deep box select; Shift+Alt+C; Ctrl+Alt+C/V; z-order; align; Ctrl+D or Alt+drag duplicate; Ctrl+Shift+L; undo/redo; group/ungroup; Shift+H/V; S; G; Shift+F; Ctrl+Shift+< / >.

---

## 7. i18n
- Locales live in `P/locales/`: 58 JSON files (`en.json`, `percentages.json` and 56 translations) plus a `README.md`.
- `en.json` has 41 top-level namespaces and 633 leaf keys (counted with a script). Namespaces include labels, toolBar, hints, helpDialog, colorPicker, fontList, commandPalette, keys, stats and welcomeScreen.

**Loader** (`P/i18n.ts`, 172 lines)
- `en.json` is statically imported as the fallback (:6).
- A language is listed only if `percentages.json` ≥ `COMPLETION_THRESHOLD` 85 (:7-9, :71-72).
- RTL languages (ar-SA, fa-IR, he-IL) are flagged `rtl: true` (:24-36).
- `setLanguage` sets `document.documentElement.dir` (:94) and dynamically imports `./locales/${code}.json` (:92-104).
- `t(path, replacements)` falls back to English (:127-142).
- Crowdin config is at repo root `crowdin.yml`.

---

## 8. Element property controls (`P/actions/actionProperties.tsx`)
**Which element types get which property** (`packages/element/src/comparisons.ts:3-68`)
| Property | Element / tool types |
|---|---|
| Background | rectangle, stickynote, iframe, embeddable, ellipse, diamond, line, freedraw, autoshape, bucketfill (:3-14) |
| Fill style | background types except stickynote (:16-17) |
| Stroke color | rectangle, stickynote, ellipse, diamond, freedraw, arrow, line, text, embeddable, autoshape (:19-29) |
| Stroke width | rectangle, iframe, embeddable, ellipse, diamond, freedraw, arrow, line, autoshape (:31-40) |
| Stroke style | as stroke width minus freedraw (:42-50) |
| Sloppiness | stroke style types + stickynote (:52-53) |
| Freedraw mode | freedraw (:55) |
| Roundness | rectangle, iframe, embeddable, line, diamond, stickynote, image (:57-64) |
| Arrow type, arrowheads | arrow (:66-68) |

**Panel-level rules** (`shapeActionPredicates.ts:117-188`)
- Stroke color is hidden for image/frame selections.
- Fill shows only when the background is non-transparent.
- Text controls show when a text tool or text element is involved. Vertical align shows only for bound text.
- Layers are hidden while the freedraw/autoshape tool is active.
- Align needs a multi-selection; distribute needs more than 2 elements.
- Link needs a single element (or container + text); crop needs a single image; line editor needs a single non-elbow linear element.

**Control values**
| Control | Options | Lines |
|---|---|---|
| Stroke / Background | ColorPicker, element palette + top picks; label is "Text color" for sticky notes | :405-432, :520-545 |
| Fill | hachure (alt-click toggles zigzag), cross-hatch, solid | :639-679 |
| Stroke width | thin 1 / medium 2 / bold 4; freedraw uses 0.5 / 1 / 2 | :729-748; `C/constants.ts:480-501` |
| Sloppiness | architect 0 / artist 1 / cartoonist 2 | :786-802 |
| Pressure | constant / variable | :878-889 |
| Stroke style | solid / dashed / dotted | :922-938 |
| Opacity | Range 0–100, step 10 | :985-993 |
| Font size | S 16 / M 20 / L 28 / XL 36 | :1023-1048 |
| Font family | FontPicker | :1443 |
| Text align | left / center / right | :1589-1608 |
| Vertical align | top / middle / bottom | :1687-1706 |
| Edges (roundness) | sharp / round | :1793-1803 |
| Arrowheads, start and end | none, arrow, triangle, triangle_outline, circle, circle_outline, diamond, diamond_outline, bar, cardinality_one / many / one_or_many / exactly_one / zero_or_one / zero_or_many | :1837-1934; IconPickers :1998, :2015 |
| Arrow type | sharp / round / elbow | :2256-2275 |

**Layers, align, group, link, duplicate, delete** are rendered from actionZindex, actionAlign, actionDistribute, actionGroup, actionLink, actionDuplicateSelection and actionDeleteSelected (`Actions.tsx:65-123, 201-214`).

**Lock** is exposed in the context menu and via Ctrl+Shift+L (`App.tsx:13932`; `actionElementLock.ts:147`), not in the panel.

---

## 9. Component library / storybook
- There is **no Storybook**: no `*.stories.*` or `.storybook` files were found.
- The UI kit is hand-rolled in `P/components`: Island, Stack, Button, FilledButton, IconButton, ToolIcon, RadioGroup, RadioSelection, Switch, Range, TextField, Dialog, Modal, Popover, PropertiesPopover, Tooltip, Toast, Card, dropdownMenu/*, Sidebar/*.
- Radix UI is used for Popover (`Actions.tsx:3`).

**Documentation** is a Docusaurus site in `dev-docs/`:
- Children-component APIs: MainMenu, WelcomeScreen, Sidebar, Footer, LiveCollaborationTrigger (`dev-docs/docs/@excalidraw/excalidraw/api/children-components/*.mdx`).
- `ui-options.mdx` and `render-props.mdx`.
- `customizing-styles.mdx` covers overriding CSS variables on `.excalidraw` and `.excalidraw.theme--dark`, and names `--color-primary*` as the main customization points (:1-40). It mentions `--color-primary-contrast-offset`, which is not declared in `theme.scss`.
