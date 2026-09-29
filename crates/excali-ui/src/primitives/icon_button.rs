//! `IconButton` (`components/IconButton.tsx`, `ToolIcon.scss`): the
//! `ToolIcon` button of toolbars, menus and the footer, a plain or
//! "icon" button or a pressed/unpressed tool toggle, sized
//! `--default-button-size` with a `--default-icon-size` icon.
//!
//! Not ported: the loading state (`isLoading` and a promise-returning
//! `onClick`, which show upstream's `Spinner`).

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::PointerEvent;

use crate::dom::{class_names, Element, Handler, Node};

/// `size`: the `ToolIcon_size_*` class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconButtonSize {
    Small,
    #[default]
    Medium,
}

impl IconButtonSize {
    fn class(self) -> &'static str {
        match self {
            IconButtonSize::Small => "ToolIcon_size_small",
            IconButtonSize::Medium => "ToolIcon_size_medium",
        }
    }
}

/// `type`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconButtonKind {
    /// A button (`type: "button"`).
    #[default]
    Button,
    /// A button without chrome (`type: "icon"`, `ToolIcon--plain`).
    Icon,
    /// A stateful tool button (`type: "toggle"`), `aria-pressed`.
    Toggle { checked: bool },
}

/// Called on a toggle's activation with the pointer type of the press
/// (`mouse`, `pen`, `touch`), `None` for keyboard or assistive activation.
pub type OnToolSelect = Rc<dyn Fn(Option<String>)>;

/// IconButton's props.
#[derive(Clone)]
pub struct IconButtonProps {
    pub kind: IconButtonKind,
    pub icon: Option<Node>,
    /// Shown instead of an icon when there is none.
    pub label: Option<String>,
    pub aria_label: String,
    pub aria_keyshortcuts: Option<String>,
    pub test_id: Option<String>,
    pub title: Option<String>,
    pub size: IconButtonSize,
    /// The shortcut hint in the icon's corner (`ToolIcon__keybinding`).
    pub key_binding_label: Option<String>,
    /// Shows `aria_label` as a visible label (`ToolIcon__label`).
    pub show_aria_label: bool,
    pub hidden: bool,
    pub visible: bool,
    pub disabled: bool,
    pub class_name: String,
    pub style: Vec<(String, String)>,
    /// After the icon and label (buttons only).
    pub children: Vec<Node>,
    /// A button's click.
    pub on_click: Option<Handler>,
    /// A toggle's activation.
    pub on_select: Option<OnToolSelect>,
    /// A toggle's press, before it may become a click.
    pub on_pointer_down: Option<Handler>,
}

impl Default for IconButtonProps {
    fn default() -> IconButtonProps {
        IconButtonProps {
            kind: IconButtonKind::Button,
            icon: None,
            label: None,
            aria_label: String::new(),
            aria_keyshortcuts: None,
            test_id: None,
            title: None,
            size: IconButtonSize::Medium,
            key_binding_label: None,
            show_aria_label: false,
            hidden: false,
            visible: true,
            disabled: false,
            class_name: String::new(),
            style: Vec::new(),
            children: Vec::new(),
            on_click: None,
            on_select: None,
            on_pointer_down: None,
        }
    }
}

fn keybinding(label: Option<String>) -> Option<Element> {
    label.filter(|l| !l.is_empty()).map(|l| {
        Element::new("span")
            .attr("class", "ToolIcon__keybinding")
            .child(l)
    })
}

/// The ToolIcon button (`IconButton.tsx:113-209`).
pub fn icon_button(props: IconButtonProps) -> Element {
    match props.kind {
        IconButtonKind::Button | IconButtonKind::Icon => plain(props),
        IconButtonKind::Toggle { checked } => toggle(props, checked),
    }
}

fn plain(props: IconButtonProps) -> Element {
    let shown = props.visible && !props.hidden;
    let class = class_names([
        ("ToolIcon_type_button", true),
        (props.size.class(), true),
        (props.class_name.as_str(), true),
        ("ToolIcon_type_button--show", shown),
        ("ToolIcon_type_button--hide", !shown),
        ("ToolIcon", !props.hidden),
        ("ToolIcon--plain", props.kind == IconButtonKind::Icon),
    ]);
    let icon = props
        .icon
        .or_else(|| props.label.filter(|l| !l.is_empty()).map(Node::Text));
    let mut el = Element::new("button")
        .attr("class", class)
        .styles(props.style)
        .attr_opt("data-testid", props.test_id)
        .flag("hidden", props.hidden)
        .attr_opt("title", props.title)
        .attr("aria-label", props.aria_label.clone())
        .attr("type", "button")
        .flag("disabled", props.disabled)
        .child_opt(icon.map(|icon| {
            Element::new("div")
                .attr("class", "ToolIcon__icon")
                .attr("aria-hidden", "true")
                .attr("aria-disabled", props.disabled.to_string())
                .child(icon)
                .child_opt(keybinding(props.key_binding_label))
        }))
        .child_opt(props.show_aria_label.then(|| {
            Element::new("div")
                .attr("class", "ToolIcon__label")
                .child(format!("{} ", props.aria_label))
        }))
        .children_from(props.children);
    if let Some(on_click) = props.on_click {
        el = el.on("click", move |e| on_click(e));
    }
    el
}

fn toggle(props: IconButtonProps, checked: bool) -> Element {
    let class = class_names([
        ("ToolIcon", true),
        ("ToolIcon_type_toggle", true),
        (props.size.class(), true),
        (props.class_name.as_str(), true),
        ("ToolIcon--checked", checked),
    ]);
    // the pointer type of the press, read on click (IconButton.tsx:161,
    // 186-198): a press cancelled by sliding off never clicks
    let pointer_type: Rc<RefCell<Option<String>>> = Rc::default();
    let on_down = pointer_type.clone();
    let on_up = pointer_type.clone();
    let on_click = pointer_type;
    let on_pointer_down = props.on_pointer_down;
    let on_select = props.on_select;
    Element::new("button")
        .attr("class", class)
        .attr("type", "button")
        .styles(props.style)
        .attr_opt("title", props.title)
        .attr("aria-label", props.aria_label)
        .attr_opt("aria-keyshortcuts", props.aria_keyshortcuts)
        .attr("aria-pressed", checked.to_string())
        .attr_opt("data-testid", props.test_id)
        .flag("disabled", props.disabled)
        .attr("aria-disabled", props.disabled.to_string())
        .on("pointerdown", move |e| {
            let kind = e
                .dyn_ref::<PointerEvent>()
                .map(PointerEvent::pointer_type)
                .filter(|t| !t.is_empty());
            *on_down.borrow_mut() = kind;
            if let Some(f) = &on_pointer_down {
                f(e);
            }
        })
        .on("pointerup", move |_| {
            let slot = on_up.clone();
            let clear = Closure::once_into_js(move || *slot.borrow_mut() = None);
            if let Some(window) = web_sys::window() {
                let _ = window.request_animation_frame(clear.unchecked_ref());
            }
        })
        .on("click", move |_| {
            if let Some(f) = &on_select {
                f(on_click.borrow().clone());
            }
        })
        .child(
            Element::new("div")
                .attr("class", "ToolIcon__icon")
                .child_opt(props.icon)
                .child_opt(keybinding(props.key_binding_label)),
        )
}
