+++
title = "ADR-012: Accessibility over upstream"
description = "Where upstream's chrome fails axe's WCAG 2.1 A/AA rules, the element adds accessible names, a tab stop and contrast, and keeps upstream's markup, layout and behaviour otherwise."
weight = 12
+++

**Status.** Accepted, 2026-09-29 (`ex-709`).

## Question

`ex-709` asks for axe checks to pass in the Playwright suite. The port reproduces upstream's markup and styles (its fixtures compare them byte for byte), and upstream's own chrome fails some of axe's WCAG 2.0/2.1 A and AA rules. Does the port keep those failures, or go past upstream, and how far?

## Evidence

axe-core 4.13.0 (`@axe-core/playwright` 4.13.0) on `<excali-editor>` in Chromium, tags `wcag2a`, `wcag2aa`, `wcag21a`, `wcag21aa`, light and dark, empty and with a scene, and with the main menu, the help dialog, the command palette and the library sidebar open (`tests/web/specs/accessibility.spec.mjs`, run 2026-09-29), after the port-side gaps were fixed (the container's font and text colour, `css/styles.scss:41-54`; the layer UI's DOM order, `components/LayerUI.tsx:647-660`):

- `button-name` (critical): the main menu's trigger (`main-menu/MainMenu.tsx:43-55`), the default sidebar's search and library tab triggers (`Sidebar/SidebarTabTrigger.tsx`) and the library header's menu trigger (`LibraryMenuHeaderContent.tsx:195-261`). Each is radix's `DropdownMenu.Trigger` or `Tabs.Trigger` with an `aria-hidden` icon as its only child and no `aria-label` or `title`.
- `color-contrast` (serious): the main menu's shortcuts, `opacity: 0.5` over the item's text (`dropdownMenu/DropdownMenu.scss`): 3.31:1 light, 4.14:1 dark; the command palette's key hints in `--color-gray-50` (`CommandPalette/CommandPalette.scss:64-67`): 2.84:1 light; the empty library's hint in `--color-border-outline` (`LibraryMenuItems.scss:35-37`): 4.49:1 light. AA asks 4.5:1 for text below 18pt.
- `scrollable-region-focusable` (serious): the command palette's list (`CommandPalette.tsx:914`) scrolls and has no focusable descendant; its items are picked with the arrow keys, read on the window.

## Options

1. **Keep upstream's failures** and run axe with those rules off, or with the failures listed as expected. The port stays pixel- and markup-identical; the acceptance check is weaker than axe's rule set.
2. **Change the components** to carry the names and the contrast. Their markup fixtures (`tools/goldens/main-menu.mjs`, `library-sidebar.mjs`, `command-palette.mjs`) stop matching upstream's output.
3. **Leave the components as upstream renders them and add a layer in the element**: names and a tab stop set on the mounted DOM, contrast rules in a stylesheet of their own, each written where upstream falls short and nowhere else.

## Decision

Option 3. `excali_ui::accessibility`:

1. `ACCESSIBLE_NAMES`: the four triggers get an `aria-label` with the English string upstream uses for the same thing (`buttons.menu` "Menu", `search.title` "Find on canvas", `toolBar.library` "Library", `labels.more_options` "More options"; the fixtures read each from `locales/en.json`). A control that already has an `aria-label` keeps it.
2. `SCROLL_REGIONS`: the command palette's list gets `tabindex="0"`. Tab moves from the search field to the list and back; the palette's keys work from either, since the element reads them on the window.
3. `ACCESSIBILITY_CSS`: the menu's shortcuts at `opacity: 0.7` (6.4:1 light, 6.7:1 dark), and the palette's key hints and the empty library's hint in `--color-gray-70` on the light island (6.7:1). The dark theme's hints already pass and are left as they are.

`name_controls` runs after the element renders its chrome and after the palette mounts or re-mounts its list. The Playwright suite runs axe with every WCAG A/AA rule and expects no violation in every state above.

## Consequences

- The components' fixtures still compare with upstream's markup; the deviations are in one module and one stylesheet, and this page lists all of them.
- Three secondary texts are darker than upstream's in the light theme, and the menu's shortcuts in both. Nothing else looks different.
- The command palette has one more tab stop than upstream's.

## What would reverse it

Upstream naming these controls or raising this contrast itself: the fixtures then carry the change, and the matching entry here goes. An owner decision to keep upstream's pixels exactly would move the contrast rules to option 1's expected list.
