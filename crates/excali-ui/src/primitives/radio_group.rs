//! `RadioGroup` (`components/RadioGroup.tsx`, `RadioGroup.scss`): a
//! segmented control of radio inputs.

use std::rc::Rc;

use crate::dom::{class_names, Element, Node};

/// One choice.
#[derive(Clone, Debug)]
pub struct RadioGroupChoice<T> {
    pub value: T,
    pub label: Node,
    /// The choice's `title` and its input's `aria-label`.
    pub aria_label: Option<String>,
}

/// RadioGroup's props.
#[derive(Clone)]
pub struct RadioGroupProps<T> {
    pub choices: Vec<RadioGroupChoice<T>>,
    pub value: T,
    /// Called with a choice's value when its input changes.
    pub on_change: Rc<dyn Fn(T)>,
    /// The inputs' `name`.
    pub name: String,
}

/// `div.RadioGroup` of `div.RadioGroup__choice`, the chosen one `active`
/// and its input checked (`RadioGroup.tsx:17-46`).
pub fn radio_group<T: Clone + PartialEq + 'static>(props: RadioGroupProps<T>) -> Element {
    let choices = props.choices.into_iter().map(|choice| {
        let active = choice.value == props.value;
        let on_change = props.on_change.clone();
        let value = choice.value.clone();
        Node::from(
            Element::new("div")
                .attr(
                    "class",
                    class_names([("RadioGroup__choice", true), ("active", active)]),
                )
                .attr_opt("title", choice.aria_label.clone())
                .child(
                    Element::new("input")
                        .attr("name", props.name.clone())
                        .attr("type", "radio")
                        .flag("checked", active)
                        .attr_opt("aria-label", choice.aria_label)
                        .on("change", move |_| on_change(value.clone())),
                )
                .child(choice.label),
        )
    });
    Element::new("div")
        .attr("class", "RadioGroup")
        .children_from(choices.collect::<Vec<_>>())
}
