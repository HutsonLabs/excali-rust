//! `Stack.Row` and `Stack.Col` (`components/Stack.tsx`, `Stack.scss`):
//! grid stacks spaced by `--gap` × `--space-factor`.

use crate::dom::{class_names, Element, Node};

use super::number;

/// `align`: `align-items` of a row, `justify-items` of a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    Baseline,
}

impl Align {
    fn css(self) -> &'static str {
        match self {
            Align::Start => "start",
            Align::Center => "center",
            Align::End => "end",
            Align::Baseline => "baseline",
        }
    }
}

/// `justifyContent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JustifyContent {
    Center,
    SpaceAround,
    SpaceBetween,
}

impl JustifyContent {
    fn css(self) -> &'static str {
        match self {
            JustifyContent::Center => "center",
            JustifyContent::SpaceAround => "space-around",
            JustifyContent::SpaceBetween => "space-between",
        }
    }
}

/// A stack's props.
#[derive(Clone, Debug, Default)]
pub struct StackProps {
    /// `--gap`, in space-factor units.
    pub gap: Option<f64>,
    pub align: Option<Align>,
    pub justify_content: Option<JustifyContent>,
    pub class_name: Option<String>,
    /// Inline style, after the stack's own.
    pub style: Vec<(String, String)>,
}

fn stack(base: &str, align_property: &str, props: StackProps, children: Vec<Node>) -> Element {
    Element::new("div")
        .attr(
            "class",
            class_names([
                (base, true),
                (props.class_name.as_deref().unwrap_or(""), true),
            ]),
        )
        .style_opt("--gap", props.gap.map(number))
        .style_opt(align_property, props.align.map(Align::css))
        .style_opt(
            "justify-content",
            props.justify_content.map(JustifyContent::css),
        )
        .styles(props.style)
        .children_from(children)
}

/// `Stack.Row`: `div.Stack.Stack_horizontal` (`Stack.tsx:15-35`).
pub fn stack_row(props: StackProps, children: Vec<Node>) -> Element {
    stack("Stack Stack_horizontal", "align-items", props, children)
}

/// `Stack.Col`: `div.Stack.Stack_vertical` (`Stack.tsx:37-57`).
pub fn stack_col(props: StackProps, children: Vec<Node>) -> Element {
    stack("Stack Stack_vertical", "justify-items", props, children)
}
