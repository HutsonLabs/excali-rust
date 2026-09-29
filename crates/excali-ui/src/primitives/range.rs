//! `Range` (`components/Range.tsx`, `Range.scss`): a labelled slider with
//! its value in a bubble over the thumb and the minimum under the track.

use std::rc::Rc;

use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use crate::dom::{Element, Node};

use super::number;

/// Range's props.
#[derive(Clone)]
pub struct RangeProps {
    pub label: Node,
    pub value: f64,
    /// Called with the input's value on every `input` event (React's
    /// `onChange` for a range).
    pub on_change: Rc<dyn Fn(f64)>,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// Under the track's start; the minimum when `None`.
    pub min_label: Option<Node>,
    /// When false (the selection's values differ) the track takes
    /// `--button-bg`.
    pub has_common_value: bool,
    pub test_id: Option<String>,
}

impl Default for RangeProps {
    /// Upstream's defaults: 0 to 100 in steps of 10.
    fn default() -> RangeProps {
        RangeProps {
            label: Node::text(""),
            value: 0.0,
            on_change: Rc::new(|_| {}),
            min: 0.0,
            max: 100.0,
            step: 10.0,
            min_label: None,
            has_common_value: true,
            test_id: None,
        }
    }
}

/// `(value - min) / (max - min || 1)`, the `--range-progress` the track's
/// fill and the bubble follow (`Range.tsx:31`).
pub fn range_progress(value: f64, min: f64, max: f64) -> f64 {
    let span = max - min;
    // `||`: 0 and NaN are falsy
    let span = if span == 0.0 || span.is_nan() {
        1.0
    } else {
        span
    };
    (value - min) / span
}

/// `label.control-label` holding the label, the input, the value bubble
/// (empty at the minimum) and the minimum label (`Range.tsx:33-58`).
pub fn range(props: RangeProps) -> Element {
    let progress = range_progress(props.value, props.min, props.max);
    let on_change = props.on_change;
    let min_label = props
        .min_label
        .unwrap_or_else(|| Node::text(number(props.min)));
    Element::new("label")
        .attr("class", "control-label")
        .child(props.label)
        .child(
            Element::new("div")
                .attr("class", "range-wrapper")
                .style("--range-progress", number(progress))
                .child(
                    Element::new("input")
                        .style_opt(
                            "--color-slider-track",
                            (!props.has_common_value).then_some("var(--button-bg)"),
                        )
                        .attr("type", "range")
                        .attr("min", number(props.min))
                        .attr("max", number(props.max))
                        .attr("step", number(props.step))
                        .attr("value", number(props.value))
                        .attr("class", "range-input")
                        .attr_opt("data-testid", props.test_id)
                        .on("input", move |e| {
                            if let Some(input) = e
                                .current_target()
                                .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
                            {
                                on_change(input.value_as_number());
                            }
                        }),
                )
                .child(
                    Element::new("div")
                        .attr("class", "value-bubble")
                        .child_opt((props.value != props.min).then(|| number(props.value))),
                )
                .child(
                    Element::new("div")
                        .attr("class", "zero-label")
                        .child(min_label),
                ),
        )
}
