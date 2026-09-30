//! The main (hamburger) menu: `MainMenu`
//! (`components/main-menu/MainMenu.tsx`), the default menu LayerUI renders
//! (`components/LayerUI.tsx:111-136`), the items of
//! `main-menu/DefaultItems.tsx` and the DropdownMenu components they are
//! built from (`components/dropdownMenu/*`), as [`dom`](crate::dom) trees.
//!
//! Which default items show is [`excali_editor::actions::default_main_menu`]
//! (each item's own predicate and `UIOptions.canvasActions`). What an item
//! does is a list of [`MenuEffect`]s the caller's [`Dispatch`] applies;
//! an item that does not prevent it closes the menu afterwards (the
//! content's `onSelect`, `setAppState({ openMenu: null })`). The
//! [`MenuContext`] keeps every handler it binds, in document order, so the
//! menu's behaviour is checkable without a browser.
//!
//! Radix's DropdownMenu leaves the roles, `aria-haspopup` and
//! `aria-expanded` kept here; its placement (popper), roving focus and
//! generated ids are not ported. Escape and a press outside the menu close
//! it, as `DropdownMenuContent` does. See
//! `site/content/research/ui-design-system.md` section 3.3.

use std::cell::RefCell;
use std::rc::Rc;

use excali_editor::actions::{
    default_main_menu as default_menu_entries, get_shortcut_from_shortcut_name, ActionContext,
    ActionManager, ActionName, KeyLabels, MainMenuEntry, MainMenuItem, MainMenuRow,
};
use excali_scene::shape::Theme;
use serde_json::{json, Map, Value};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{AddEventListenerOptions, Document, Event, KeyboardEvent};

use crate::dom::{Element, Node};
use crate::icons::{self, Icon};
use crate::primitives::{
    island, radio_group, stack_col, IslandProps, RadioGroupChoice, RadioGroupProps, StackProps,
};

/// `DropdownMenu.scss` and `DefaultItems.scss` compiled
/// (`tools/goldens/main-menu.mjs`).
pub const MAIN_MENU_CSS: &str = include_str!("main_menu.css");

/// `CLASSES.DROPDOWN_MENU_EVENT_WRAPPER` (`common/src/constants.ts`).
pub const DROPDOWN_MENU_EVENT_WRAPPER: &str = "dropdown-menu-event-wrapper";

/// What ToggleTheme's radio warns without `props.onThemeChange`
/// (`DefaultItems.tsx:267-269`).
pub const THEME_CHANGE_WARNING: &str = "MainMenu.DefaultItems.ToggleTheme: `<Excalidraw/> props.onThemeChange` must be defined to use system theme selection.";

/// A theme the radio offers: `THEME.LIGHT`, `THEME.DARK` or `"system"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice {
    Light,
    Dark,
    System,
}

impl ThemeChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
            ThemeChoice::System => "system",
        }
    }
}

/// What selecting an item (or changing a radio) does.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEffect {
    /// `actionManager.executeAction(action)`.
    ExecuteAction(ActionName),
    /// `setAppState(patch)`.
    SetAppState(Map<String, Value>),
    /// `app.toggleLock()`.
    ToggleLock,
    /// `setActiveConfirmDialog(name)`.
    ConfirmDialog(&'static str),
    /// `openConfirmModal` for loading over a non-empty scene
    /// (`overwriteConfirm.modal.loadFromFile`); `then` runs once confirmed.
    ConfirmOverwrite { then: ActionName },
    /// `props.onThemeChange(theme)`.
    ThemeChange(ThemeChoice),
    /// `trackEvent(category, action, label)`.
    TrackEvent {
        category: &'static str,
        action: &'static str,
        label: &'static str,
    },
    /// The caller's own `onSelect` (LiveCollaborationTrigger).
    Select,
    /// `console.warn(message)`.
    Warn(&'static str),
}

/// Applies an effect.
pub type Dispatch = Rc<dyn Fn(MenuEffect)>;

/// A bound handler: the DOM event, the effects, and whether the menu
/// closes after them.
#[derive(Clone, Debug, PartialEq)]
pub struct Handler {
    pub event: &'static str,
    pub effects: Vec<MenuEffect>,
    pub close_menu: bool,
}

/// `setAppState({ openMenu: null })`: the content's `onSelect` and
/// `onClickOutside` (`MainMenu.tsx:55-61`).
pub fn close_menu_effect() -> MenuEffect {
    MenuEffect::SetAppState(patch([("openMenu", Value::Null)]))
}

fn patch<const N: usize>(entries: [(&str, Value); N]) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

/// What the menu is built for: the form factor (a phone hides shortcuts
/// and stacks the items), the platform and key names for shortcuts, the
/// icons' theme, and where effects go.
pub struct MenuContext<'a> {
    pub phone: bool,
    pub is_darwin: bool,
    pub labels: &'a KeyLabels<'a>,
    pub theme: Theme,
    dispatch: Dispatch,
    handlers: RefCell<Vec<Handler>>,
}

impl<'a> MenuContext<'a> {
    pub fn new(
        phone: bool,
        is_darwin: bool,
        labels: &'a KeyLabels<'a>,
        theme: Theme,
        dispatch: Dispatch,
    ) -> MenuContext<'a> {
        MenuContext {
            phone,
            is_darwin,
            labels,
            theme,
            dispatch,
            handlers: RefCell::default(),
        }
    }

    /// Every handler bound so far, in document order.
    pub fn handlers(&self) -> Vec<Handler> {
        self.handlers.borrow().clone()
    }

    /// `getShortcutFromShortcutName(name)`.
    pub fn shortcut(&self, name: &str) -> String {
        get_shortcut_from_shortcut_name(name, 0, self.is_darwin, self.labels)
    }

    fn icon(&self, icon: &Icon) -> Node {
        icon.element(self.theme)
            .expect("a menu icon is markup")
            .into()
    }

    /// Binds `effects` to `event` on `el`, then the menu's close unless
    /// `close_menu` is false (the item called `event.preventDefault()`).
    fn bind(
        &self,
        el: Element,
        event: &'static str,
        effects: Vec<MenuEffect>,
        close_menu: bool,
    ) -> Element {
        self.handlers.borrow_mut().push(Handler {
            event,
            effects: effects.clone(),
            close_menu,
        });
        let dispatch = self.dispatch.clone();
        el.on(event, move |_| {
            for effect in &effects {
                dispatch(effect.clone());
            }
            if close_menu {
                dispatch(close_menu_effect());
            }
        })
    }
}

/// The English strings the menu shows (`locales/en.json`); the key itself
/// for any other.
pub fn menu_text(key: &str) -> &str {
    match key {
        "buttons.load" => "Open",
        "buttons.save" => "Save to current file",
        "buttons.exportImage" => "Export image...",
        "buttons.export" => "Save to...",
        "buttons.clearReset" => "Reset the canvas",
        "buttons.lightMode" => "Light mode",
        "buttons.darkMode" => "Dark mode",
        "buttons.systemMode" => "System mode",
        "buttons.objectsSnapMode" => "Snap to objects",
        "buttons.zenMode" => "Zen mode",
        "buttons.menu" => "Menu",
        "commandPalette.title" => "Command palette",
        "search.title" => "Find on canvas",
        "helpDialog.title" => "Help",
        "labels.theme" => "Theme",
        "labels.canvasBackground" => "Canvas background",
        "labels.followUs" => "Follow us",
        "labels.discordChat" => "Discord chat",
        "labels.liveCollaboration" => "Live collaboration...",
        "labels.preferences" => "Preferences",
        "labels.preferences_toolLock" => "Tool lock",
        "labels.boxSelectionMode" => "Select on",
        "labels.boxSelectionContain" => "Wrap",
        "labels.boxSelectionOverlap" => "Overlap",
        "labels.inputDevice" => "Input",
        "labels.inputDeviceTrackpad" => "Trackpad",
        "labels.inputDeviceMouse" => "Mouse",
        "labels.arrowBinding" => "Arrow binding",
        "labels.midpointSnapping" => "Snap to midpoints",
        "labels.showHints" => "Show hints",
        "labels.toggleGrid" => "Toggle grid",
        "labels.viewMode" => "View mode",
        "labels.collaborators" => "Collaborators",
        "stats.fullTitle" => "Canvas & Shape properties",
        "overwriteConfirm.modal.loadFromFile.title" => "Load from file",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// DropdownMenu
// ---------------------------------------------------------------------------

/// `DropdownMenu` (`DropdownMenu.tsx`): the event wrapper, out of the box
/// layout, holding the trigger and, when open, the content.
pub fn dropdown_menu(trigger: Option<Element>, content: Option<Element>) -> Element {
    Element::new("div")
        .attr("class", DROPDOWN_MENU_EVENT_WRAPPER)
        .style("display", "contents")
        .child_opt(trigger)
        .child_opt(content)
}

/// `DropdownMenuTrigger` (`DropdownMenuTrigger.tsx`) with `className`,
/// `data-testid` and what its click does.
pub fn menu_trigger(
    cx: &MenuContext<'_>,
    open: bool,
    class_name: &str,
    test_id: &str,
    children: Vec<Node>,
    on_toggle: Vec<MenuEffect>,
) -> Element {
    let mut class = format!("dropdown-menu-button {class_name}");
    class = format!("{} zen-mode-transition", class.trim());
    if cx.phone {
        class.push_str(" dropdown-menu-button--mobile");
    }
    let el = Element::new("button")
        .attr("class", class)
        .attr("aria-haspopup", "menu")
        .attr("aria-expanded", if open { "true" } else { "false" })
        .attr("type", "button")
        .attr("data-testid", test_id)
        .children_from(children);
    cx.bind(el, "click", on_toggle, false)
}

/// The container inside a menu's content: a padded Island, or a column
/// on a phone.
fn menu_container(cx: &MenuContext<'_>, style: Vec<(String, String)>, children: Vec<Node>) -> Node {
    if cx.phone {
        stack_col(
            StackProps {
                class_name: Some("dropdown-menu-container".into()),
                ..StackProps::default()
            },
            children,
        )
        .into()
    } else {
        island(
            IslandProps {
                padding: Some(2.0),
                class_name: Some("dropdown-menu-container".into()),
                style,
                ..IslandProps::default()
            },
            children,
        )
        .into()
    }
}

/// `DropdownMenuContent` (`DropdownMenuContent.tsx`): Escape and a press
/// outside the event wrapper close the menu.
pub fn menu_content(cx: &MenuContext<'_>, class_name: &str, children: Vec<Node>) -> Element {
    let mut class = format!("dropdown-menu {class_name}").trim().to_owned();
    if cx.phone {
        class.push_str(" dropdown-menu--mobile");
    }
    let dispatch = cx.dispatch.clone();
    Element::new("div")
        .attr("class", class)
        .attr("role", "menu")
        .attr("data-testid", "dropdown-menu")
        .child(menu_container(cx, vec![], children))
        .on_mount(move |el| close_on_escape_or_outside(el, dispatch.clone()))
}

/// `getDropdownMenuItemClassName(className, selected)` (`common.ts`).
fn item_class(class_name: &str, selected: bool) -> String {
    let selected = if selected {
        "dropdown-menu-item--selected"
    } else {
        ""
    };
    format!("dropdown-menu-item dropdown-menu-item-base {class_name} {selected} ")
        .trim()
        .to_owned()
}

/// Radix's Slot joins its class before the child's.
fn radix_item_class(class_name: &str, selected: bool) -> String {
    format!("radix-menu-item {}", item_class(class_name, selected))
}

/// `Ellipsify` (`Ellipsify.tsx`).
fn ellipsify(children: Vec<Node>) -> Element {
    Element::new("span")
        .style("text-overflow", "ellipsis")
        .style("overflow", "hidden")
        .style("white-space", "nowrap")
        .children_from(children)
}

/// `MenuItemContent` (`DropdownMenuItemContent.tsx`): icon, text and, off
/// a phone, the shortcut.
fn item_content(
    cx: &MenuContext<'_>,
    icon: Option<&Icon>,
    label: &str,
    shortcut: &str,
) -> Vec<Node> {
    let mut out = Vec::new();
    if let Some(icon) = icon {
        out.push(
            Element::new("div")
                .attr("class", "dropdown-menu-item__icon")
                .child(cx.icon(icon))
                .into(),
        );
    }
    out.push(
        Element::new("div")
            .attr("class", "dropdown-menu-item__text")
            .child(ellipsify(vec![label.into()]))
            .into(),
    );
    if !shortcut.is_empty() && !cx.phone {
        out.push(
            Element::new("div")
                .attr("class", "dropdown-menu-item__shortcut")
                .child(shortcut)
                .into(),
        );
    }
    out
}

/// A `DropdownMenuItem`'s props.
#[derive(Clone, Debug, Default)]
pub struct MenuItemProps<'a> {
    pub icon: Option<&'a Icon>,
    pub label: String,
    pub shortcut: String,
    pub test_id: Option<&'a str>,
    /// `aria-label`, and the `title` when set.
    pub aria_label: Option<String>,
    pub class_name: String,
    pub selected: bool,
}

/// `DropdownMenuItem` (`DropdownMenuItem.tsx`): a button that runs
/// `effects` when selected, then closes the menu unless `close_menu` is
/// false.
pub fn menu_item(
    cx: &MenuContext<'_>,
    props: MenuItemProps<'_>,
    effects: Vec<MenuEffect>,
    close_menu: bool,
) -> Element {
    let el = Element::new("button")
        .attr("class", radix_item_class(&props.class_name, props.selected))
        .attr("role", "menuitem")
        .attr_opt("data-testid", props.test_id)
        .attr_opt("aria-label", props.aria_label.clone())
        .attr_opt("title", props.aria_label)
        .children_from(item_content(cx, props.icon, &props.label, &props.shortcut));
    cx.bind(el, "click", effects, close_menu)
}

/// `DropdownMenuItemCheckbox`: an item whose icon is a check when
/// `checked`.
pub fn menu_item_checkbox(
    cx: &MenuContext<'_>,
    checked: bool,
    label: &str,
    shortcut: String,
    effects: Vec<MenuEffect>,
    close_menu: bool,
) -> Element {
    let icon = if checked {
        &icons::checkIcon
    } else {
        &icons::emptyIcon
    };
    let props = MenuItemProps {
        icon: Some(icon),
        label: label.to_owned(),
        shortcut,
        ..MenuItemProps::default()
    };
    menu_item(cx, props, effects, close_menu)
}

/// `DropdownMenuItemLink` (`DropdownMenuItemLink.tsx`): a link opening in
/// a new tab.
pub fn menu_item_link(
    cx: &MenuContext<'_>,
    icon: &Icon,
    href: &str,
    label: &str,
    aria_label: &str,
) -> Element {
    let el = Element::new("a")
        .attr("class", radix_item_class("", false))
        .attr("role", "menuitem")
        .attr("aria-label", aria_label)
        .attr("href", href)
        .attr("target", "_blank")
        .attr("rel", "noopener noopener")
        .attr("title", aria_label)
        .children_from(item_content(cx, Some(icon), label, ""));
    cx.bind(el, "click", vec![], true)
}

/// A radio item's choice: value, label and `aria-label` (also its title).
pub struct RadioChoice<T> {
    pub value: T,
    pub label: Node,
    pub aria_label: String,
    /// What choosing it does.
    pub effects: Vec<MenuEffect>,
}

/// `DropdownMenuItemContentRadio` (`DropdownMenuItemContentRadio.tsx`): a
/// label and a RadioGroup, and an orphaned shortcut off a phone.
pub fn menu_item_content_radio<T: Clone + PartialEq + 'static>(
    cx: &MenuContext<'_>,
    name: &str,
    icon: Option<&Icon>,
    value: T,
    choices: Vec<RadioChoice<T>>,
    label: &str,
    shortcut: &str,
) -> Vec<Node> {
    let mut by_value = Vec::new();
    let mut group_choices = Vec::new();
    for choice in choices {
        cx.handlers.borrow_mut().push(Handler {
            event: "change",
            effects: choice.effects.clone(),
            close_menu: false,
        });
        by_value.push((choice.value.clone(), choice.effects));
        group_choices.push(RadioGroupChoice {
            value: choice.value,
            label: choice.label,
            aria_label: Some(choice.aria_label),
        });
    }
    let dispatch = cx.dispatch.clone();
    let group = radio_group(RadioGroupProps {
        choices: group_choices,
        value,
        on_change: Rc::new(move |v: T| {
            if let Some((_, effects)) = by_value.iter().find(|(x, _)| *x == v) {
                for effect in effects {
                    dispatch(effect.clone());
                }
            }
        }),
        name: name.to_owned(),
    });
    let mut row =
        Element::new("div").attr("class", "dropdown-menu-item-base dropdown-menu-item-bare");
    if let Some(icon) = icon {
        row = row.child(
            Element::new("div")
                .attr("class", "dropdown-menu-item__icon")
                .child(cx.icon(icon)),
        );
    }
    row = row
        .child(
            Element::new("label")
                .attr("class", "dropdown-menu-item__text")
                .child(ellipsify(vec![label.into()])),
        )
        .child(group);
    let mut out = vec![row.into()];
    if !shortcut.is_empty() && !cx.phone {
        out.push(
            Element::new("div")
                .attr(
                    "class",
                    "dropdown-menu-item__shortcut dropdown-menu-item__shortcut--orphaned",
                )
                .child(shortcut)
                .into(),
        );
    }
    out
}

/// `DropdownMenuGroup` (`DropdownMenuGroup.tsx`).
pub fn menu_group(title: Option<&str>, class_name: &str, children: Vec<Node>) -> Element {
    let mut el = Element::new("div").attr("class", format!("dropdown-menu-group {class_name}"));
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        el = el.child(
            Element::new("p")
                .attr("class", "dropdown-menu-group-title")
                .child(title),
        );
    }
    el.children_from(children)
}

/// `DropdownMenuSeparator` (`DropdownMenuSeparator.tsx`).
pub fn menu_separator() -> Element {
    Element::new("div")
        .style("height", "1px")
        .style("background-color", "var(--default-border-color)")
        .style("margin", "6px 0")
        .style("flex", "0 0 auto")
}

/// `DropdownMenuSub` with its `Trigger` and `Content`
/// (`DropdownMenuSub*.tsx`): the trigger opens the submenu on hover or
/// click, and a click toggles it; closed, the content is hidden.
pub fn menu_sub(
    cx: &MenuContext<'_>,
    icon: Option<&Icon>,
    label: &str,
    class_name: &str,
    open: bool,
    children: Vec<Node>,
) -> Vec<Node> {
    let trigger = Element::new("div")
        .attr(
            "class",
            format!("{} dropdown-menu__submenu-trigger", item_class("", false)),
        )
        .attr("role", "menuitem")
        .attr("aria-haspopup", "menu")
        .attr("aria-expanded", if open { "true" } else { "false" })
        .children_from(item_content(cx, icon, label, ""))
        .child(
            Element::new("div")
                .attr("class", "dropdown-menu__submenu-trigger-icon")
                .child(cx.icon(&icons::chevronRight)),
        )
        .on("click", |e| toggle_submenu(e, None))
        .on("pointerenter", |e| toggle_submenu(e, Some(true)));
    let mut class = format!("dropdown-menu dropdown-submenu {class_name}")
        .trim()
        .to_owned();
    if cx.phone {
        class.push_str(" dropdown-menu--mobile");
    }
    let content = Element::new("div")
        .attr("class", class)
        .attr("role", "menu")
        .flag("hidden", !open)
        .child(menu_container(
            cx,
            vec![("z-index".into(), "1".into())],
            children,
        ));
    vec![trigger.into(), content.into()]
}

/// Opens (`Some(true)`) or toggles the submenu after the trigger.
fn toggle_submenu(e: &Event, open: Option<bool>) {
    let Some(trigger) = e
        .current_target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
    else {
        return;
    };
    let Some(content) = trigger.next_element_sibling() else {
        return;
    };
    let open = open.unwrap_or_else(|| content.has_attribute("hidden"));
    let _ = trigger.set_attribute("aria-expanded", if open { "true" } else { "false" });
    let _ = if open {
        content.remove_attribute("hidden")
    } else {
        content.set_attribute("hidden", "")
    };
}

// ---------------------------------------------------------------------------
// MainMenu and its default items
// ---------------------------------------------------------------------------

/// `MainMenu` (`MainMenu.tsx:21-80`): the hamburger trigger toggling
/// `openMenu: "canvas"` and, when open, the content with `children`
/// (built only then).
pub fn main_menu(
    cx: &MenuContext<'_>,
    open: bool,
    children: impl FnOnce() -> Vec<Node>,
) -> Element {
    let toggle = MenuEffect::SetAppState(patch([
        ("openMenu", if open { Value::Null } else { json!("canvas") }),
        ("openPopup", Value::Null),
        ("openDialog", Value::Null),
    ]));
    let trigger = menu_trigger(
        cx,
        open,
        "main-menu-trigger",
        "main-menu-trigger",
        vec![cx.icon(&icons::HamburgerMenuIcon)],
        vec![toggle],
    );
    let content = open.then(|| menu_content(cx, "main-menu", children()));
    dropdown_menu(Some(trigger), content)
}

/// The default main menu (`LayerUI.tsx:111-136`) for the editor's state:
/// the items [`excali_editor::actions::default_main_menu`] shows, each as
/// `DefaultItems.tsx` renders it; `render_action` renders the canvas
/// background picker (`actionManager.renderAction`).
pub fn default_main_menu(
    cx: &MenuContext<'_>,
    manager: &ActionManager,
    ctx: &ActionContext<'_>,
    render_action: &dyn Fn(ActionName) -> Option<Node>,
) -> Element {
    let open = ctx.app_state.get("openMenu").and_then(Value::as_str) == Some("canvas");
    main_menu(cx, open, || {
        let entries = default_menu_entries(manager, ctx, cx.labels);
        let mut out = Vec::new();
        for entry in &entries {
            match entry {
                MainMenuEntry::Item(row) => out.extend(default_item(cx, ctx, row, render_action)),
                MainMenuEntry::Separator => out.push(menu_separator().into()),
                MainMenuEntry::Group { title, items } => {
                    let children = items
                        .iter()
                        .flat_map(|row| default_item(cx, ctx, row, render_action))
                        .collect();
                    out.push(menu_group(Some(title), "", children).into());
                }
            }
        }
        out
    })
}

fn open_dialog(name: &str) -> MenuEffect {
    MenuEffect::SetAppState(patch([("openDialog", json!({ "name": name }))]))
}

/// One of `DefaultItems.tsx`'s items for a row of the default menu.
fn default_item(
    cx: &MenuContext<'_>,
    ctx: &ActionContext<'_>,
    row: &MainMenuRow,
    render_action: &dyn Fn(ActionName) -> Option<Node>,
) -> Vec<Node> {
    let label = menu_text(row.label).to_owned();
    let item = |icon: &Icon, test_id, effects: Vec<MenuEffect>, close_menu| -> Vec<Node> {
        let props = MenuItemProps {
            icon: Some(icon),
            label: label.clone(),
            shortcut: row.shortcut.clone(),
            test_id: Some(test_id),
            aria_label: Some(label.clone()),
            ..MenuItemProps::default()
        };
        vec![menu_item(cx, props, effects, close_menu).into()]
    };
    let exec = |name: ActionName| vec![MenuEffect::ExecuteAction(name)];
    match row.item {
        // LoadScene (:70-110): confirm first over a non-empty scene
        MainMenuItem::LoadScene => {
            let effects = if ctx.elements.iter().any(|e| !e.base.is_deleted) {
                vec![MenuEffect::ConfirmOverwrite {
                    then: ActionName::LoadScene,
                }]
            } else {
                exec(ActionName::LoadScene)
            };
            item(&icons::LoadIcon, "load-button", effects, true)
        }
        MainMenuItem::SaveToActiveFile => item(
            &icons::save,
            "save-button",
            exec(ActionName::SaveToActiveFile),
            true,
        ),
        MainMenuItem::Export => item(
            &icons::ExportIcon,
            "json-export-button",
            vec![open_dialog("jsonExport")],
            true,
        ),
        MainMenuItem::SaveAsImage => item(
            &icons::ExportImageIcon,
            "image-export-button",
            vec![open_dialog("imageExport")],
            true,
        ),
        MainMenuItem::SearchMenu => item(
            &icons::searchIcon,
            "search-menu-button",
            exec(ActionName::SearchMenu),
            true,
        ),
        MainMenuItem::Help => item(
            &icons::HelpIcon,
            "help-menu-item",
            exec(ActionName::ToggleShortcuts),
            true,
        ),
        MainMenuItem::ClearCanvas => item(
            &icons::TrashIcon,
            "clear-canvas-button",
            vec![MenuEffect::ConfirmDialog("clearCanvas")],
            true,
        ),
        MainMenuItem::Socials => socials(cx),
        // ToggleTheme without the system theme (:285-318): the menu stays
        // open
        MainMenuItem::ToggleTheme => {
            let dark = ctx.app_state.get("theme").and_then(Value::as_str) == Some("dark");
            let icon = if dark {
                &icons::SunIcon
            } else {
                &icons::MoonIcon
            };
            item(
                icon,
                "toggle-dark-mode",
                exec(ActionName::ToggleTheme),
                false,
            )
        }
        MainMenuItem::ChangeCanvasBackground => {
            vec![change_canvas_background(render_action).into()]
        }
    }
}

/// `ChangeCanvasBackground` (`DefaultItems.tsx:323-352`), once its gate
/// holds.
pub fn change_canvas_background(render_action: &dyn Fn(ActionName) -> Option<Node>) -> Element {
    Element::new("div")
        .style("margin-top", "0.75rem")
        .child(
            Element::new("div")
                .attr("data-testid", "canvas-background-label")
                .style("font-size", "0.875rem")
                .style("margin-bottom", "0.25rem")
                .style("margin-left", "0.5rem")
                .child(menu_text("labels.canvasBackground")),
        )
        .child(
            Element::new("div")
                .style("padding", "0 0.625rem")
                .child_opt(render_action(ActionName::ChangeViewBackgroundColor)),
        )
}

/// `Socials` (`DefaultItems.tsx:373-401`): GitHub, X and Discord.
pub fn socials(cx: &MenuContext<'_>) -> Vec<Node> {
    vec![
        menu_item_link(
            cx,
            &icons::GithubIcon,
            "https://github.com/excalidraw/excalidraw",
            "GitHub",
            "GitHub",
        )
        .into(),
        menu_item_link(
            cx,
            &icons::XBrandIcon,
            "https://x.com/excalidraw",
            menu_text("labels.followUs"),
            "X",
        )
        .into(),
        menu_item_link(
            cx,
            &icons::DiscordIcon,
            "https://discord.gg/UexuTaE",
            menu_text("labels.discordChat"),
            "Discord",
        )
        .into(),
    ]
}

/// `ToggleTheme` with `allowSystemTheme` (`DefaultItems.tsx:235-283`): the
/// Light / Dark / System radio at `theme`; a choice calls
/// `props.onThemeChange` when the host has one (`has_handler`), else warns.
pub fn toggle_theme_radio(
    cx: &MenuContext<'_>,
    theme: ThemeChoice,
    has_handler: bool,
) -> Vec<Node> {
    let shortcut = cx.shortcut("toggleTheme");
    let effects = |v: ThemeChoice| {
        if has_handler {
            vec![MenuEffect::ThemeChange(v)]
        } else {
            vec![MenuEffect::Warn(THEME_CHANGE_WARNING)]
        }
    };
    let choice = |value, icon: &Icon, aria_label: String| RadioChoice {
        value,
        label: cx.icon(icon),
        aria_label,
        effects: effects(value),
    };
    let choices = vec![
        choice(
            ThemeChoice::Light,
            &icons::SunIcon,
            format!("{} - {shortcut}", menu_text("buttons.lightMode")),
        ),
        choice(
            ThemeChoice::Dark,
            &icons::MoonIcon,
            format!("{} - {shortcut}", menu_text("buttons.darkMode")),
        ),
        choice(
            ThemeChoice::System,
            &icons::DeviceDesktopIcon,
            menu_text("buttons.systemMode").to_owned(),
        ),
    ];
    menu_item_content_radio(
        cx,
        "theme",
        None,
        theme,
        choices,
        menu_text("labels.theme"),
        "",
    )
}

/// `CommandPalette` (`DefaultItems.tsx:150-169`).
pub fn command_palette(cx: &MenuContext<'_>, class_name: Option<&str>) -> Element {
    let label = menu_text("commandPalette.title");
    let props = MenuItemProps {
        icon: Some(&icons::boltIcon),
        label: label.to_owned(),
        shortcut: cx.shortcut("commandPalette"),
        test_id: Some("command-palette-button"),
        aria_label: Some(label.to_owned()),
        class_name: class_name.unwrap_or("").to_owned(),
        selected: false,
    };
    let effects = vec![
        MenuEffect::TrackEvent {
            category: "command_palette",
            action: "open",
            label: "menu",
        },
        open_dialog("commandPalette"),
    ];
    menu_item(cx, props, effects, true)
}

/// `LiveCollaborationTrigger` (`DefaultItems.tsx:404-424`): selecting it
/// calls the host's `onSelect` ([`MenuEffect::Select`]).
pub fn live_collaboration_trigger(cx: &MenuContext<'_>, is_collaborating: bool) -> Element {
    let props = MenuItemProps {
        icon: Some(&icons::usersIcon),
        label: menu_text("labels.liveCollaboration").to_owned(),
        test_id: Some("collab-button"),
        class_name: if is_collaborating {
            "active-collab"
        } else {
            ""
        }
        .to_owned(),
        ..MenuItemProps::default()
    };
    menu_item(cx, props, vec![MenuEffect::Select], true)
}

/// `Preferences` with its default items (`DefaultItems.tsx:427-692`): box
/// selection mode, input device, tool lock, object snapping, grid, zen
/// mode, view mode (when its action is enabled), element properties,
/// arrow binding, midpoint snapping and hints. Every item keeps the menu
/// open.
pub fn preferences(
    cx: &MenuContext<'_>,
    manager: &ActionManager,
    ctx: &ActionContext<'_>,
    open: bool,
) -> Vec<Node> {
    let state = |key: &str| ctx.app_state.get(key);
    let flag = |key: &str| state(key).and_then(Value::as_bool).unwrap_or(false);
    let set = |key: &str, value: Value| vec![MenuEffect::SetAppState(patch([(key, value)]))];
    let text_choice = |value: &'static str, key: &str| RadioChoice {
        value,
        label: menu_text(key).into(),
        aria_label: menu_text(key).to_owned(),
        effects: set(value_key(key), json!(value)),
    };
    fn value_key(label_key: &str) -> &'static str {
        if label_key.starts_with("labels.boxSelection") {
            "boxSelectionMode"
        } else {
            "inputDevice"
        }
    }
    let mut items: Vec<Node> = Vec::new();
    let box_mode = match state("boxSelectionMode").and_then(Value::as_str) {
        Some("overlap") => "overlap",
        _ => "contain",
    };
    items.extend(menu_item_content_radio(
        cx,
        "boxSelectionMode",
        Some(&icons::emptyIcon),
        box_mode,
        vec![
            text_choice("contain", "labels.boxSelectionContain"),
            text_choice("overlap", "labels.boxSelectionOverlap"),
        ],
        menu_text("labels.boxSelectionMode"),
        "",
    ));
    // resolveInputDevice (appState.ts:352-355): `auto` shows as trackpad
    let device = match state("inputDevice").and_then(Value::as_str) {
        Some("mouse") => "mouse",
        _ => "trackpad",
    };
    items.extend(menu_item_content_radio(
        cx,
        "inputDevice",
        Some(&icons::emptyIcon),
        device,
        vec![
            text_choice("trackpad", "labels.inputDeviceTrackpad"),
            text_choice("mouse", "labels.inputDeviceMouse"),
        ],
        menu_text("labels.inputDevice"),
        "",
    ));
    let mut checkbox = |checked: bool, label: &str, shortcut: &str, effects| {
        let shortcut = if shortcut.is_empty() {
            String::new()
        } else {
            cx.shortcut(shortcut)
        };
        items.push(
            menu_item_checkbox(cx, checked, menu_text(label), shortcut, effects, false).into(),
        );
    };
    let locked = state("activeTool")
        .and_then(|t| t.get("locked"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let exec = |name| vec![MenuEffect::ExecuteAction(name)];
    checkbox(
        locked,
        "labels.preferences_toolLock",
        "toolLock",
        vec![MenuEffect::ToggleLock],
    );
    checkbox(
        flag("objectsSnapModeEnabled"),
        "buttons.objectsSnapMode",
        "objectsSnapMode",
        exec(ActionName::ObjectsSnapMode),
    );
    checkbox(
        flag("gridModeEnabled"),
        "labels.toggleGrid",
        "gridMode",
        exec(ActionName::GridMode),
    );
    checkbox(
        flag("zenModeEnabled"),
        "buttons.zenMode",
        "zenMode",
        exec(ActionName::ZenMode),
    );
    if manager.is_action_enabled(ActionName::ViewMode, ctx) {
        checkbox(
            flag("viewModeEnabled"),
            "labels.viewMode",
            "viewMode",
            exec(ActionName::ViewMode),
        );
    }
    let stats_open = state("stats")
        .and_then(|s| s.get("open"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    checkbox(
        stats_open,
        "stats.fullTitle",
        "stats",
        exec(ActionName::Stats),
    );
    checkbox(
        state("bindingPreference").and_then(Value::as_str) == Some("enabled"),
        "labels.arrowBinding",
        "",
        exec(ActionName::ArrowBinding),
    );
    checkbox(
        flag("isMidpointSnappingEnabled"),
        "labels.midpointSnapping",
        "",
        exec(ActionName::MidpointSnapping),
    );
    let hints = flag("showHints");
    checkbox(
        hints,
        "labels.showHints",
        "",
        set("showHints", json!(!hints)),
    );
    menu_sub(
        cx,
        Some(&icons::settingsIcon),
        menu_text("labels.preferences"),
        "excalidraw-main-menu-preferences-submenu",
        open,
        items,
    )
}

// ---------------------------------------------------------------------------
// The document side
// ---------------------------------------------------------------------------

/// Listens on `el`'s document while `el` is connected (removing itself on
/// the first event after), as `DropdownMenuContent`'s effects do.
fn document_listener(
    el: &web_sys::Element,
    event: &'static str,
    capture: bool,
    f: impl Fn(&Event) + 'static,
) {
    let Some(document) = el.owner_document() else {
        return;
    };
    let slot: Rc<RefCell<Option<js_sys::Function>>> = Rc::default();
    let own = slot.clone();
    let menu = el.clone();
    let target = document.clone();
    let closure = Closure::<dyn FnMut(Event)>::new(move |e: Event| {
        if !menu.is_connected() {
            if let Some(f) = own.borrow_mut().take() {
                let _ = target.remove_event_listener_with_callback_and_bool(event, &f, capture);
            }
            return;
        }
        f(&e);
    });
    let f: js_sys::Function = closure.into_js_value().unchecked_into();
    let options = AddEventListenerOptions::new();
    options.set_capture(capture);
    let _ = document
        .add_event_listener_with_callback_and_add_event_listener_options(event, &f, &options);
    *slot.borrow_mut() = Some(f);
}

/// Escape (captured, so it stops before the editor's own key handling)
/// and a press outside the menu's event wrapper close the menu
/// (`DropdownMenuContent.tsx:44-88`).
fn close_on_escape_or_outside(el: &web_sys::Element, dispatch: Dispatch) {
    let on_escape = dispatch.clone();
    document_listener(el, "keydown", true, move |e| {
        if e.dyn_ref::<KeyboardEvent>()
            .is_some_and(|k| k.key() == "Escape")
        {
            e.prevent_default();
            e.stop_immediate_propagation();
            on_escape(close_menu_effect());
        }
    });
    let menu = el.clone();
    document_listener(el, "pointerdown", false, move |e| {
        let wrapper = menu
            .closest(&format!(".{DROPDOWN_MENU_EVENT_WRAPPER}"))
            .ok()
            .flatten();
        let target = e.target().and_then(|t| t.dyn_into::<web_sys::Node>().ok());
        let inside = wrapper.is_some_and(|w| w.contains(target.as_ref()));
        if !inside {
            dispatch(close_menu_effect());
        }
    });
}

/// The `data-excali-ui` value of the `<style>` holding [`MAIN_MENU_CSS`].
const STYLESHEET_ID: &str = "main-menu";

/// Adds [`MAIN_MENU_CSS`] to `document`'s head once, after the
/// primitives' stylesheet when it is there (else first), so a host's own
/// rules still come later.
pub fn install_stylesheet(document: &Document) -> Result<(), JsValue> {
    let selector = format!("style[data-excali-ui=\"{STYLESHEET_ID}\"]");
    if document.query_selector(&selector)?.is_some() {
        return Ok(());
    }
    let style = document.create_element("style")?;
    style.set_attribute("data-excali-ui", STYLESHEET_ID)?;
    style.set_text_content(Some(MAIN_MENU_CSS));
    let head = document
        .head()
        .ok_or_else(|| JsValue::from_str("the document has no head"))?;
    let after = document.query_selector("style[data-excali-ui=\"primitives\"]")?;
    let before = match after {
        Some(p) => p.next_sibling(),
        None => head.first_child(),
    };
    head.insert_before(&style, before.as_ref())?;
    Ok(())
}
