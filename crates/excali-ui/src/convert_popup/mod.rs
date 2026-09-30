//! `ConvertElementTypePopup` (`components/ConvertElementTypePopup.tsx:
//! 257-331`, `ConvertElementTypePopup.scss`): the panel under the
//! selection that lists the types it converts to, the current one pressed.
//! [`excali_editor::convert_element_type::ConvertElementTypePopup::panel`]
//! decides what it shows and where; the host mounts it in the editor
//! container and focuses it after a click (`panelRef.current?.focus()`),
//! so Tab keeps cycling from it.

use std::rc::Rc;

use excali_core::json::number_to_string;
use excali_editor::convert_element_type::{ConvertPanel, ConvertibleType};

use crate::dom::Element;
use crate::primitives::{icon_button, IconButtonKind, IconButtonProps};
use crate::toolbar::icon_node;

/// `ConvertElementTypePopup.scss`, compiled.
pub const CONVERT_POPUP_CSS: &str = include_str!("convert_popup.css");

/// `CLASSES.CONVERT_ELEMENT_TYPE_POPUP`.
pub use crate::keyboard::CONVERT_ELEMENT_TYPE_POPUP;

/// A click on the button of a type.
pub type OnConvert = Rc<dyn Fn(ConvertibleType)>;

/// The `icons.tsx` icon of each type (`SHAPES`, `:257-271`).
pub fn convertible_icon(kind: ConvertibleType) -> &'static str {
    match kind {
        ConvertibleType::Rectangle => "RectangleIcon",
        ConvertibleType::Diamond => "DiamondIcon",
        ConvertibleType::Ellipse => "EllipseIcon",
        ConvertibleType::Line => "LineIcon",
        ConvertibleType::SharpArrow => "sharpArrowIcon",
        ConvertibleType::CurvedArrow => "roundArrowIcon",
        ConvertibleType::ElbowArrow => "elbowArrowIcon",
    }
}

fn px(x: f64) -> String {
    format!("{}px", number_to_string(x))
}

/// The panel (`Panel`, `:273-331`): an unfocusable-by-Tab (`tabIndex -1`)
/// box at the panel's position with a toggle per type.
pub fn convert_popup(panel: &ConvertPanel, on_select: Option<OnConvert>) -> Element {
    let buttons = panel.shapes.iter().map(|shape| {
        let name = shape.kind.name();
        let kind = shape.kind;
        let on_select = on_select.clone();
        icon_button(IconButtonProps {
            kind: IconButtonKind::Toggle {
                checked: shape.checked,
            },
            icon: Some(icon_node(convertible_icon(kind))),
            title: Some(name.to_owned()),
            aria_label: name.to_owned(),
            test_id: Some(format!("toolbar-{name}")),
            on_select: on_select.map(|f| Rc::new(move |_| f(kind)) as Rc<dyn Fn(Option<String>)>),
            ..IconButtonProps::default()
        })
    });
    Element::new("div")
        .attr("tabindex", "-1")
        .style("position", "absolute")
        .style("top", px(panel.top))
        .style("left", px(panel.left))
        .style("z-index", "2")
        .attr("class", CONVERT_ELEMENT_TYPE_POPUP)
        .children_from(buttons.map(crate::dom::Node::Element))
}
