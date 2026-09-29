//! The context menu: `ContextMenu` (`components/ContextMenu.tsx`,
//! `ContextMenu.scss`) in its `Popover`, as a [`dom`](crate::dom) tree.
//!
//! Which rows show is [`excali_editor::actions::get_context_menu_items`]
//! (`App.getContextMenuItems`, `App.tsx:13835-13936`: the canvas and
//! element menus, their view-mode lists and the desktop-only z-order rows)
//! filtered by [`excali_editor::actions::build_context_menu`] (each
//! action's predicate, the separators that would lead or repeat, labels,
//! shortcuts and checkmarks). This module renders those entries: a row
//! closes the menu and then runs its action (`onClose(() =>
//! actionManager.executeAction(item, "contextMenu"))`), a press outside
//! the popover closes it, and the list suppresses the browser's own menu.
//! What a row does is a list of [`ContextMenuEffect`]s the caller applies;
//! [`ContextMenu::handlers`] keeps every handler bound, in document order,
//! so the menu's behaviour is checkable without a browser. See
//! `site/content/research/ui-design-system.md` section 3.10.

use std::rc::Rc;

use excali_editor::actions::{ActionName, ContextMenuEntry, ContextMenuRow};
use wasm_bindgen::JsValue;
use web_sys::Document;

use crate::dom::{class_names, Element, Node};
use crate::primitives::{popover, PopoverProps};

/// `ContextMenu.scss` compiled (`tools/goldens/context-menu.mjs`).
pub const CONTEXT_MENU_CSS: &str = include_str!("context_menu.css");

/// What a handler of the menu does.
#[derive(Clone, Debug, PartialEq)]
pub enum ContextMenuEffect {
    /// `onClose()`: `setState({ contextMenu: null })`.
    Close,
    /// `actionManager.executeAction(action, "contextMenu")`, run once the
    /// menu is closed.
    ExecuteAction(ActionName),
    /// `event.preventDefault()`.
    PreventDefault,
}

/// Applies an effect.
pub type OnContextMenuEffect = Rc<dyn Fn(ContextMenuEffect)>;

/// ContextMenu's props: where it opens, relative to the editor container
/// (`contextMenu.top`, `.left`), and the viewport it fits in
/// (`appState.width`, `.height`).
#[derive(Clone)]
pub struct ContextMenuProps {
    pub top: f64,
    pub left: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub on_effect: Option<OnContextMenuEffect>,
}

/// A rendered menu: its tree and, in document order, each handler's event
/// and effects.
pub struct ContextMenu {
    pub element: Element,
    pub handlers: Vec<(&'static str, Vec<ContextMenuEffect>)>,
}

/// The English labels of the menus' actions (`locales/en.json`); the key
/// itself for any other, as `t` answers a missing key.
pub fn context_menu_text(key: &str) -> &str {
    match key {
        "buttons.objectsSnapMode" => "Snap to objects",
        "buttons.zenMode" => "Zen mode",
        "helpDialog.cropStart" => "Crop image",
        "labels.addToLibrary" => "Add to library",
        "labels.arrowBinding" => "Arrow binding",
        "labels.autoResize" => "Enable text auto-resizing",
        "labels.bindText" => "Bind text to the container",
        "labels.bringForward" => "Bring forward",
        "labels.bringToFront" => "Bring to front",
        "labels.copy" => "Copy",
        "labels.copyAsPng" => "Copy to clipboard as PNG",
        "labels.copyAsSvg" => "Copy to clipboard as SVG",
        "labels.copyElementLink" => "Copy link to object",
        "labels.copyStyles" => "Copy styles",
        "labels.copyText" => "Copy to clipboard as text",
        "labels.createContainerFromText" => "Wrap text in a container",
        "labels.cut" => "Cut",
        "labels.delete" => "Delete",
        "labels.duplicateSelection" => "Duplicate",
        "labels.elementLock.lock" => "Lock",
        "labels.elementLock.unlock" => "Unlock",
        "labels.elementLock.unlockAll" => "Unlock all",
        "labels.flipHorizontal" => "Flip horizontal",
        "labels.flipVertical" => "Flip vertical",
        "labels.group" => "Group selection",
        "labels.lineEditor.edit" => "Edit line",
        "labels.lineEditor.editArrow" => "Edit arrow",
        "labels.link.create" => "Add link",
        "labels.link.edit" => "Edit link",
        "labels.link.editEmbed" => "Edit embeddable link",
        "labels.midpointSnapping" => "Snap to midpoints",
        "labels.paste" => "Paste",
        "labels.pasteStyles" => "Paste styles",
        "labels.removeAllElementsFromFrame" => "Remove all elements from frame",
        "labels.selectAll" => "Select all",
        "labels.selectAllElementsInFrame" => "Select all elements in frame",
        "labels.sendBackward" => "Send backward",
        "labels.sendToBack" => "Send to back",
        "labels.toggleGrid" => "Toggle grid",
        "labels.unbindText" => "Unbind text",
        "labels.ungroup" => "Ungroup selection",
        "labels.viewMode" => "View mode",
        "labels.wrapSelectionInFrame" => "Wrap selection in frame",
        "stats.fullTitle" => "Canvas & Shape properties",
        other => other,
    }
}

/// `ContextMenu` (`ContextMenu.tsx:34-122`) over the filtered entries.
pub fn context_menu(entries: &[ContextMenuEntry], props: ContextMenuProps) -> ContextMenu {
    let mut handlers = Vec::new();
    let on_effect = props.on_effect.clone();
    let mut bind = |el: Element, event: &'static str, effects: Vec<ContextMenuEffect>| {
        handlers.push((event, effects.clone()));
        match &on_effect {
            Some(on_effect) => {
                let on_effect = on_effect.clone();
                el.on(event, move |e| {
                    for effect in &effects {
                        if *effect == ContextMenuEffect::PreventDefault {
                            e.prevent_default();
                        }
                        on_effect(effect.clone());
                    }
                })
            }
            // the list still keeps the browser's menu away
            None if effects.contains(&ContextMenuEffect::PreventDefault) => {
                el.on(event, |e| e.prevent_default())
            }
            None => el,
        }
    };
    let mut list = bind(
        Element::new("ul").attr("class", "context-menu"),
        "contextmenu",
        vec![ContextMenuEffect::PreventDefault],
    );
    for entry in entries {
        list = list.child(match entry {
            ContextMenuEntry::Separator => {
                Element::new("hr").attr("class", "context-menu-item-separator")
            }
            ContextMenuEntry::Item(row) => bind(
                context_menu_row(row),
                "click",
                vec![
                    ContextMenuEffect::Close,
                    ContextMenuEffect::ExecuteAction(row.name),
                ],
            ),
        });
    }
    let close = props.on_effect.clone();
    let element = popover(
        PopoverProps {
            top: Some(props.top),
            left: Some(props.left),
            on_close_request: close.map(|close| {
                Rc::new(move |_: &web_sys::Event| close(ContextMenuEffect::Close))
                    as crate::dom::Handler
            }),
            fit_in_viewport: true,
            viewport_width: Some(props.viewport_width),
            viewport_height: Some(props.viewport_height),
            class_name: Some("context-menu-popover".into()),
        },
        vec![list.into()],
    );
    ContextMenu { element, handlers }
}

/// A row: `li[data-testid]` holding the button with its label and
/// shortcut (`ContextMenu.tsx:88-117`).
fn context_menu_row(row: &ContextMenuRow) -> Element {
    Element::new("li")
        .attr("data-testid", row.name.as_str())
        .child(
            Element::new("button")
                .attr("type", "button")
                .attr(
                    "class",
                    class_names([
                        ("context-menu-item", true),
                        ("dangerous", row.dangerous),
                        ("checkmark", row.checked),
                    ]),
                )
                .child(
                    Element::new("div")
                        .attr("class", "context-menu-item__label")
                        .child(Node::text(context_menu_text(row.label))),
                )
                .child(
                    Element::new("kbd")
                        .attr("class", "context-menu-item__shortcut")
                        .child(Node::text(row.shortcut.clone())),
                ),
        )
}

const STYLESHEET_ID: &str = "context-menu";

/// Adds [`CONTEXT_MENU_CSS`] to the document's head once, after the
/// primitives' stylesheet.
pub fn install_stylesheet(document: &Document) -> Result<(), JsValue> {
    let selector = format!("style[data-excali-ui=\"{STYLESHEET_ID}\"]");
    if document.query_selector(&selector)?.is_some() {
        return Ok(());
    }
    let style = document.create_element("style")?;
    style.set_attribute("data-excali-ui", STYLESHEET_ID)?;
    style.set_text_content(Some(CONTEXT_MENU_CSS));
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
