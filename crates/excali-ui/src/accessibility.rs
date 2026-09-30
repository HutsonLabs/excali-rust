//! Where the element goes past upstream's markup and styles so that axe
//! finds no WCAG 2.1 A/AA violation (ADR-012, ex-709): accessible names
//! for the icon-only controls upstream renders without one, a focusable
//! command list, and [`ACCESSIBILITY_CSS`]'s contrast.
//!
//! Radix's `DropdownMenu.Trigger` and `Tabs.Trigger` render a `<button>`
//! whose only child is an `aria-hidden` icon, so the main menu's trigger
//! (`main-menu/MainMenu.tsx:43-55`), the default sidebar's tab triggers
//! (`DefaultSidebar.tsx`, `Sidebar/SidebarTabTrigger.tsx`) and the library
//! header's menu trigger (`LibraryMenuHeaderContent.tsx:195-261`) have no
//! accessible name, which axe reports as `button-name`. The components
//! keep upstream's markup (their fixtures compare it); the element names
//! these controls after mounting them, each with the English string
//! upstream uses for the same thing.

use wasm_bindgen::{JsCast, JsValue};

/// The contrast fixes: secondary text upstream draws below 4.5:1 (the
/// menu's shortcuts at `opacity: 0.5`, `dropdownMenu/DropdownMenu.scss`;
/// the command palette's key hints in `--color-gray-50`,
/// `CommandPalette.scss:64-67`; the empty library's hint in
/// `--color-border-outline`, `LibraryMenuItems.scss:35-37`), darkened
/// just past 4.5:1 on the light island and left as they are where they
/// already pass.
pub const ACCESSIBILITY_CSS: &str = include_str!("accessibility.css");

/// Scrolling regions without a focusable descendant (the command
/// palette's list: its items are picked with the arrow keys from the
/// search field, `CommandPalette.tsx:914`): each gets `tabindex="0"` so a
/// keyboard can reach and scroll it.
pub const SCROLL_REGIONS: &[&str] = &[".command-palette-dialog .commands"];

/// A control matched by `selector` and the `aria-label` it gets: the
/// English text of locale key `key` (`locales/en.json`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessibleName {
    pub selector: &'static str,
    pub key: &'static str,
    pub text: &'static str,
}

/// The unnamed controls and their names.
pub const ACCESSIBLE_NAMES: &[AccessibleName] = &[
    // en.json:247
    AccessibleName {
        selector: ".main-menu-trigger",
        key: "buttons.menu",
        text: "Menu",
    },
    // CANVAS_SEARCH_TAB's trigger; en.json:222, the search menu's title
    AccessibleName {
        selector: ".sidebar-tab-trigger[id$=\"-trigger-search\"]",
        key: "search.title",
        text: "Find on canvas",
    },
    // LIBRARY_SIDEBAR_TAB's trigger; en.json:334
    AccessibleName {
        selector: ".sidebar-tab-trigger[id$=\"-trigger-library\"]",
        key: "toolBar.library",
        text: "Library",
    },
    // en.json:68
    AccessibleName {
        selector: ".library-menu-dropdown-container .dropdown-menu-button",
        key: "labels.more_options",
        text: "More options",
    },
];

/// Gives every control of [`ACCESSIBLE_NAMES`] under `root` its name,
/// unless it already has an `aria-label`, and every [`SCROLL_REGIONS`]
/// region a tab stop.
pub fn name_controls(root: &web_sys::Element) -> Result<(), JsValue> {
    for selector in SCROLL_REGIONS {
        let found = root.query_selector_all(selector)?;
        for i in 0..found.length() {
            if let Some(el) = found
                .item(i)
                .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
            {
                if !el.has_attribute("tabindex") {
                    el.set_attribute("tabindex", "0")?;
                }
            }
        }
    }
    for name in ACCESSIBLE_NAMES {
        let found = root.query_selector_all(name.selector)?;
        for i in 0..found.length() {
            let Some(el) = found
                .item(i)
                .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
            else {
                continue;
            };
            if !el.has_attribute("aria-label") {
                el.set_attribute("aria-label", name.text)?;
            }
        }
    }
    Ok(())
}
