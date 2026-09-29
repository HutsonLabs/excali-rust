//! `TextField` (`components/TextField.tsx`, `TextField.scss`): a labelled
//! input, optionally with an icon, read-only, full width, or redacted with
//! an eye button that reveals the value.

use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use web_sys::{Event, HtmlInputElement};

use crate::dom::{class_names, create_detached, Element, Handler, Node};

use super::button::{button, ButtonProps};
use super::icons::{eye_closed_icon, eye_icon};

/// The input's value: controlled (`value`) or initial (`defaultValue`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextFieldValue {
    Controlled(String),
    Default(String),
}

impl Default for TextFieldValue {
    fn default() -> TextFieldValue {
        TextFieldValue::Default(String::new())
    }
}

/// The input's `type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFieldType {
    Text,
    Search,
}

/// TextField's props.
#[derive(Clone, Default)]
pub struct TextFieldProps {
    pub value: TextFieldValue,
    /// Called with the input's value on every `input` event.
    pub on_change: Option<Rc<dyn Fn(String)>>,
    pub on_key_down: Option<Handler>,
    pub readonly: bool,
    pub full_width: bool,
    /// Focuses and selects the input once mounted.
    pub select_on_render: bool,
    pub icon: Option<Node>,
    pub label: Option<String>,
    pub class_name: Option<String>,
    pub placeholder: Option<String>,
    /// Hides a controlled, non-empty value behind `is-redacted` until the
    /// eye button reveals it.
    pub is_redacted: bool,
    pub kind: Option<TextFieldType>,
}

/// The input's class: `is-redacted` while a non-empty controlled value is
/// redacted (`TextField.tsx:82-89`).
fn input_class(value: &TextFieldValue, is_redacted: bool, unredacted: bool) -> &'static str {
    let has_value = matches!(value, TextFieldValue::Controlled(v) if !v.is_empty());
    if has_value && is_redacted && !unredacted {
        "is-redacted"
    } else {
        ""
    }
}

fn eye(unredacted: bool) -> Element {
    if unredacted {
        eye_closed_icon()
    } else {
        eye_icon()
    }
}

/// `div.ExcTextField` (`TextField.tsx:67-114`).
pub fn text_field(props: TextFieldProps) -> Element {
    let value = match &props.value {
        TextFieldValue::Controlled(v) | TextFieldValue::Default(v) => v.clone(),
    };
    let mut input = Element::new("input")
        .attr("class", input_class(&props.value, props.is_redacted, false))
        .flag("readonly", props.readonly)
        .attr("value", value)
        .attr_opt("placeholder", props.placeholder)
        .attr_opt(
            "type",
            props.kind.map(|k| match k {
                TextFieldType::Text => "text",
                TextFieldType::Search => "search",
            }),
        );
    if let Some(on_change) = props.on_change {
        input = input.on("input", move |e| {
            if let Some(el) = target_input(e) {
                on_change(el.value());
            }
        });
    }
    if let Some(on_key_down) = props.on_key_down {
        input = input.on("keydown", move |e| on_key_down(e));
    }
    if props.select_on_render {
        input = input.on_mount(|el| {
            if let Some(input) = el.dyn_ref::<HtmlInputElement>() {
                let _ = input.focus();
                input.select();
            }
        });
    }
    let redact = props.is_redacted.then(|| {
        // isTemporarilyUnredacted: the eye swaps the input's class and icon
        let unredacted = Rc::new(Cell::new(false));
        let value = props.value.clone();
        button(
            ButtonProps {
                style: vec![
                    // React's `border: 0`, as the DOM serializes it
                    ("border".into(), "0px".into()),
                    ("user-select".into(), "none".into()),
                ],
                ..ButtonProps::default()
            },
            vec![eye(false).into()],
        )
        .on("click", move |e| {
            let now = !unredacted.get();
            unredacted.set(now);
            let Some(button) = e
                .current_target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            else {
                return;
            };
            if let Some(input) = button.previous_element_sibling() {
                let _ = input.set_attribute("class", input_class(&value, true, now));
            }
            if let Some(document) = button.owner_document() {
                if let Ok(icon) = create_detached(&eye(now).into(), &document) {
                    button.set_text_content(None);
                    let _ = button.append_child(&icon);
                }
            }
        })
    });
    Element::new("div")
        .attr(
            "class",
            class_names([
                ("ExcTextField", true),
                (props.class_name.as_deref().unwrap_or(""), true),
                ("ExcTextField--fullWidth", props.full_width),
                ("ExcTextField--hasIcon", props.icon.is_some()),
            ]),
        )
        .on("click", |e| {
            if let Some(input) = e
                .current_target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .and_then(|el| el.query_selector("input").ok().flatten())
                .and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
            {
                let _ = input.focus();
            }
        })
        .child_opt(props.icon)
        .child_opt(props.label.filter(|l| !l.is_empty()).map(|l| {
            Element::new("div")
                .attr("class", "ExcTextField__label")
                .child(l)
        }))
        .child(
            Element::new("div")
                .attr(
                    "class",
                    class_names([
                        ("ExcTextField__input", true),
                        ("ExcTextField__input--readonly", props.readonly),
                    ]),
                )
                .child(input)
                .child_opt(redact),
        )
}

fn target_input(e: &Event) -> Option<HtmlInputElement> {
    e.current_target()?.dyn_into::<HtmlInputElement>().ok()
}
