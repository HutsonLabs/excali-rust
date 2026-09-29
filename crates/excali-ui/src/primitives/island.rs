//! `Island` (`components/Island.tsx`, `Island.scss`): a raised surface,
//! padded by `--padding` × `--space-factor`.

use crate::dom::{class_names, Element, Node};

use super::number;

/// Island's props.
#[derive(Clone, Debug, Default)]
pub struct IslandProps {
    /// `--padding`, in space-factor units (the stylesheet's default is 0).
    pub padding: Option<f64>,
    pub class_name: Option<String>,
    /// Inline style, after `--padding` (so it may override it).
    pub style: Vec<(String, String)>,
    /// `data-viewport-ui`: marks a canvas-occluding surface (its dock).
    pub viewport_ui: Option<String>,
    /// `data-viewport-ui-name`: names that surface.
    pub viewport_ui_name: Option<String>,
}

/// `<div class="Island ...">` holding `children` (`Island.tsx:22-42`).
pub fn island(props: IslandProps, children: Vec<Node>) -> Element {
    Element::new("div")
        .attr(
            "class",
            class_names([
                ("Island", true),
                (props.class_name.as_deref().unwrap_or(""), true),
            ]),
        )
        .style_opt("--padding", props.padding.map(number))
        .styles(props.style)
        .attr_opt("data-viewport-ui", props.viewport_ui)
        .attr_opt("data-viewport-ui-name", props.viewport_ui_name)
        .children_from(children)
}
