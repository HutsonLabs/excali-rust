//! `Button` (`components/Button.tsx`, `Button.scss`): the design system's
//! outlined button, `selected` when active.

use std::rc::Rc;

use crate::dom::{class_names, Element, Node};

/// The `type` attribute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonType {
    #[default]
    Button,
    Submit,
    Reset,
}

impl ButtonType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ButtonType::Button => "button",
            ButtonType::Submit => "submit",
            ButtonType::Reset => "reset",
        }
    }
}

/// Button's props.
#[derive(Clone, Default)]
pub struct ButtonProps {
    pub kind: ButtonType,
    /// Adds the `selected` class.
    pub selected: bool,
    pub class_name: String,
    pub title: Option<String>,
    pub style: Vec<(String, String)>,
    /// Called on click.
    pub on_select: Option<Rc<dyn Fn()>>,
}

/// `<button class="excalidraw-button ...">` (`Button.tsx:26-46`).
pub fn button(props: ButtonProps, children: Vec<Node>) -> Element {
    let mut el = Element::new("button")
        .attr("type", props.kind.as_str())
        .attr(
            "class",
            class_names([
                ("excalidraw-button", true),
                (props.class_name.as_str(), true),
                ("selected", props.selected),
            ]),
        )
        .attr_opt("title", props.title)
        .styles(props.style)
        .children_from(children);
    if let Some(on_select) = props.on_select {
        el = el.on("click", move |_| on_select());
    }
    el
}
