//! `Hyperlink` (`components/hyperlink/Hyperlink.tsx:69-354`,
//! `Hyperlink.scss`): the popup above the selected element that edits its
//! link (the input) or shows it (the link, or "No link is set"), with the
//! edit, "Link to object" and remove buttons.
//! [`excali_editor::hyperlink::hyperlink_panel`] decides whether it shows,
//! in which mode and where; the host mounts it in the editor container,
//! keeps the input's value while it stays mounted, and runs what the
//! handlers report ([`HyperlinkEvent`]).

use std::rc::Rc;

use excali_core::json::number_to_string;
use excali_core::link::{is_local_link, normalize_link};
use excali_editor::hyperlink::{HyperlinkMode, HyperlinkPanel, POPUP_PADDING, POPUP_WIDTH};

pub use excali_editor::hyperlink::HyperlinkEvent;

use crate::dom::{Element, EventResponse, Node};
use crate::primitives::{icon_button, IconButtonKind, IconButtonProps};
use crate::toolbar::icon_node;

/// `Hyperlink.scss`, compiled.
pub const HYPERLINK_CSS: &str = include_str!("hyperlink.css");

/// The popup's root class.
pub const HYPERLINK_CONTAINER: &str = "excalidraw-hyperlinkContainer";

/// What a handler of the popup reports.
pub type OnHyperlinkEvent = Rc<dyn Fn(HyperlinkEvent)>;

/// `t(key)` in English for the popup's strings (`locales/en.json`);
/// `tests/hyperlink.rs` holds them to upstream's rendered popup.
fn t(key: &str) -> &'static str {
    match key {
        "labels.link.hint" => "Type or paste your link here",
        "labels.link.empty" => "No link is set",
        "labels.linkToElement" => "Link to object",
        "buttons.edit" => "Edit",
        "buttons.remove" => "Remove",
        _ => "",
    }
}

pub struct HyperlinkProps<'a> {
    pub panel: &'a HyperlinkPanel,
    /// The input's value (`inputVal`): the element's link when the popup
    /// mounted, then what was typed.
    pub input: &'a str,
    /// `location.origin`, for `isLocalLink`.
    pub origin: &'a str,
    /// Cmd rather than Ctrl is `CTRL_OR_CMD`.
    pub darwin: bool,
    /// The input's text is selected when it mounts: not on phones and
    /// touch screens (`Hyperlink.tsx:188-196`).
    pub select: bool,
    pub on_event: Option<OnHyperlinkEvent>,
}

fn px(x: f64) -> String {
    format!("{}px", number_to_string(x))
}

fn button(
    key: &str,
    class: &str,
    icon: &str,
    event: HyperlinkEvent,
    on_event: Option<&OnHyperlinkEvent>,
) -> Node {
    let on_event = on_event.cloned();
    let label = t(key);
    Node::Element(icon_button(IconButtonProps {
        kind: IconButtonKind::Button,
        icon: Some(icon_node(icon)),
        label: Some(label.to_owned()),
        title: Some(label.to_owned()),
        aria_label: label.to_owned(),
        class_name: class.to_owned(),
        on_click: on_event
            .map(|f| Rc::new(move |_: &web_sys::Event| f(event.clone())) as crate::dom::Handler),
        ..IconButtonProps::default()
    }))
}

/// The input of the editor mode (`Hyperlink.tsx:264-290`): every key stays
/// in it (`stopPropagation`), Ctrl/Cmd+K does not reach the browser, and
/// Enter or Escape submits. Mounted, it takes the focus (`autoFocus`),
/// its text selected when [`HyperlinkProps::select`] holds
/// (`inputRef.current.select()`, `:188-196`).
fn input(props: &HyperlinkProps<'_>) -> Element {
    let on_input = props.on_event.clone();
    let on_key = props.on_event.clone();
    let darwin = props.darwin;
    let select = props.select;
    Element::new("input")
        .attr("class", "excalidraw-hyperlinkContainer-input")
        .attr("placeholder", t("labels.link.hint"))
        .attr("value", props.input)
        .on_data("input", move |data| {
            if let (Some(f), Some(value)) = (&on_input, &data.value) {
                f(HyperlinkEvent::Input(value.clone()));
            }
            EventResponse::default()
        })
        .on_data("keydown", move |data| {
            let ctrl_or_cmd = if darwin { data.meta_key } else { data.ctrl_key };
            if data.key == "Enter" || data.key == "Escape" {
                if let Some(f) = &on_key {
                    f(HyperlinkEvent::Submit(
                        data.value.clone().unwrap_or_default(),
                    ));
                }
            }
            EventResponse {
                prevent_default: ctrl_or_cmd && data.key == "k",
                stop_propagation: true,
            }
        })
        .on_mount(move |el| {
            if let Some(input) = wasm_bindgen::JsCast::dyn_ref::<web_sys::HtmlInputElement>(el) {
                let _ = input.focus();
                if select {
                    input.select();
                }
            }
        })
}

/// The popup (`Hyperlink.tsx:251-353`).
pub fn hyperlink(props: HyperlinkProps<'_>) -> Element {
    let panel = props.panel;
    let editing = panel.mode == HyperlinkMode::Editor;
    let link = panel.link.as_deref().unwrap_or("");
    let content = if editing {
        input(&props)
    } else if !link.is_empty() {
        Element::new("a")
            .attr("href", normalize_link(link))
            .attr("class", "excalidraw-hyperlinkContainer-link")
            .attr(
                "target",
                if is_local_link(link, props.origin) {
                    "_self"
                } else {
                    "_blank"
                },
            )
            .attr("rel", "noopener noreferrer")
            .child(Node::text(link))
    } else {
        Element::new("div")
            .attr("class", "excalidraw-hyperlinkContainer-link")
            .child(Node::text(t("labels.link.empty")))
    };
    let on_event = props.on_event.as_ref();
    let mut buttons = Vec::new();
    if !editing {
        buttons.push(button(
            "buttons.edit",
            "excalidraw-hyperlinkContainer--edit",
            "FreedrawIcon",
            HyperlinkEvent::Edit,
            on_event,
        ));
    }
    buttons.push(button(
        "labels.linkToElement",
        "",
        "elementLinkIcon",
        HyperlinkEvent::LinkToElement,
        on_event,
    ));
    if !link.is_empty() && !panel.embeddable {
        buttons.push(button(
            "buttons.remove",
            "excalidraw-hyperlinkContainer--remove",
            "TrashIcon",
            HyperlinkEvent::Remove,
            on_event,
        ));
    }
    Element::new("div")
        .attr("class", HYPERLINK_CONTAINER)
        .style("top", px(panel.top))
        .style("left", px(panel.left))
        .style("width", px(POPUP_WIDTH))
        .style("padding", px(POPUP_PADDING))
        .child(content)
        .child(
            Element::new("div")
                .attr("class", "excalidraw-hyperlinkContainer__buttons")
                .children_from(buttons),
        )
}
