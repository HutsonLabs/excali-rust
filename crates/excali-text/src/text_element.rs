//! Bound-text sizing and placement: how much room a container gives its
//! text, where the text sits, and how big a container must grow to hold
//! its text.
//!
//! Upstream: `packages/element/src/textElement.ts` at the pinned commit:
//! `computeBoundTextPosition` (249-324), `getContainerCoords` (396-417),
//! `computeContainerDimensionForBoundText` (521-538),
//! `getBoundTextMaxWidth` (540-569) and `getBoundTextMaxHeight` (571-599).
//! Research page `site/content/research/rendering.md`, section 2, "Bound
//! text": padding `BOUND_TEXT_PADDING = 5`, an ellipse insets its text by
//! `(w/2)(1 - √2/2)` and a diamond by `w/4`, an ellipse's text may be
//! `round(w/2 · √2) - 10` wide, and an arrow label `max(0.7 · w,
//! fontSize · 11)`.
//!
//! An arrow label's position follows the arrow's path
//! (`LinearElementEditor.getBoundTextElementPosition`,
//! `linearElementEditor.ts:2068-2131`), which needs the arrow's shape and
//! belongs to `excali-editor` (crate table of
//! `site/content/architecture/overview.md`); callers hand it in as an
//! [`ArrowLabelGeometry`].

use excali_core::constants::{
    ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO, ARROW_LABEL_WIDTH_FRACTION, BOUND_TEXT_PADDING,
    DEFAULT_FONT_SIZE, STICKY_NOTE_BODY_INSET_Y, STICKY_NOTE_PADDING,
};
use excali_core::element::{Element, ElementKind, ElementType, TextAlign, VerticalAlign};

/// `LinearElementEditor.getBoundTextElementPosition(arrow, text,
/// elementsMap)` (`linearElementEditor.ts:2068-2131`): the top-left corner
/// of an arrow's label, placed at its `labelPosition` along the arrow's
/// path or else at the arrow's middle. The arrow geometry lives in
/// `excali-editor`, which implements this.
pub trait ArrowLabelGeometry {
    /// `None` when the geometry cannot place the label; the callers here
    /// then give no answer rather than a wrong one.
    fn bound_text_element_position(
        &mut self,
        arrow: &Element,
        text: &Element,
        elements: &[Element],
    ) -> Option<[f64; 2]>;
}

/// No arrow geometry: every arrow label is left unplaced (`None`).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoArrowGeometry;

impl ArrowLabelGeometry for NoArrowGeometry {
    fn bound_text_element_position(
        &mut self,
        _arrow: &Element,
        _text: &Element,
        _elements: &[Element],
    ) -> Option<[f64; 2]> {
        None
    }
}

/// `Math.round(x)`: the nearest integer, ties towards positive infinity
/// (`Math.round(-2.5)` is -2, where `f64::round` gives -3).
pub fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    // exact: x and its floor are within 1 of each other
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// `Math.min(a, b)`: `NaN` if either is.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// `Math.max(a, b)`: `NaN` if either is.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

fn element_type(element: &Element) -> ElementType {
    element.kind.element_type()
}

fn is_arrow(element: &Element) -> bool {
    element_type(element) == ElementType::Arrow
}

fn is_sticky_note(element: &Element) -> bool {
    element_type(element) == ElementType::StickyNote
}

/// The text fields of a text element.
fn text_fields(element: &Element) -> Option<&excali_core::element::TextFields> {
    match &element.kind {
        ElementKind::Text(text) => Some(text),
        _ => None,
    }
}

/// `getContainerCoords(container)` (`textElement.ts:396-417`): the
/// top-left corner of the room a container gives its text. Padded by
/// `STICKY_NOTE_PADDING` for a sticky note and `BOUND_TEXT_PADDING`
/// otherwise; an ellipse insets further by `(w/2)(1 - √2/2)` (the corner of
/// the largest inscribed rectangle) and a diamond by a quarter of its size.
pub fn get_container_coords(container: &Element) -> [f64; 2] {
    let padding = if is_sticky_note(container) {
        STICKY_NOTE_PADDING
    } else {
        BOUND_TEXT_PADDING
    };
    let (width, height) = (container.base.width, container.base.height);
    let mut offset_x = padding;
    let mut offset_y = padding;
    match element_type(container) {
        ElementType::Ellipse => {
            offset_x += (width / 2.0) * (1.0 - 2f64.sqrt() / 2.0);
            offset_y += (height / 2.0) * (1.0 - 2f64.sqrt() / 2.0);
        }
        ElementType::Diamond => {
            offset_x += width / 4.0;
            offset_y += height / 4.0;
        }
        _ => {}
    }
    [container.base.x + offset_x, container.base.y + offset_y]
}

/// `computeContainerDimensionForBoundText(dimension, containerType)`
/// (`textElement.ts:521-538`): the container width or height that holds a
/// text `dimension` wide or high, the inverse of the insets above. The
/// dimension is rounded up first; the padding is `2 * BOUND_TEXT_PADDING`
/// (for a sticky note too, which takes the rectangle rule).
pub fn compute_container_dimension_for_bound_text(
    dimension: f64,
    container_type: ElementType,
) -> f64 {
    let dimension = dimension.ceil();
    let padding = BOUND_TEXT_PADDING * 2.0;
    match container_type {
        ElementType::Ellipse => js_round(((dimension + padding) / 2f64.sqrt()) * 2.0),
        ElementType::Arrow => dimension + padding * 8.0,
        ElementType::Diamond => 2.0 * (dimension + padding),
        _ => dimension + padding,
    }
}

/// `getBoundTextMaxWidth(container, boundTextElement)`
/// (`textElement.ts:540-569`): the widest a container's text may be.
///
/// - Arrow: `max(ARROW_LABEL_WIDTH_FRACTION * width, fontSize *
///   ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO)`, the label's font size or
///   `DEFAULT_FONT_SIZE` without a label.
/// - Ellipse: `round(width / 2 * √2) - 2 * BOUND_TEXT_PADDING`.
/// - Diamond: `round(width / 2) - 2 * BOUND_TEXT_PADDING`.
/// - Sticky note: `width - 2 * STICKY_NOTE_PADDING`.
/// - Anything else: `width - 2 * BOUND_TEXT_PADDING`.
pub fn get_bound_text_max_width(container: &Element, bound_text: Option<&Element>) -> f64 {
    let width = container.base.width;
    if is_arrow(container) {
        let font_size = bound_text
            .and_then(text_fields)
            .map_or(DEFAULT_FONT_SIZE, |t| t.font_size);
        let min_width = font_size * ARROW_LABEL_FONT_SIZE_TO_MIN_WIDTH_RATIO;
        return js_max(ARROW_LABEL_WIDTH_FRACTION * width, min_width);
    }
    match element_type(container) {
        ElementType::Ellipse => js_round((width / 2.0) * 2f64.sqrt()) - BOUND_TEXT_PADDING * 2.0,
        ElementType::Diamond => js_round(width / 2.0) - BOUND_TEXT_PADDING * 2.0,
        ElementType::StickyNote => width - STICKY_NOTE_PADDING * 2.0,
        _ => width - BOUND_TEXT_PADDING * 2.0,
    }
}

/// `getBoundTextMaxHeight(container, boundTextElement)`
/// (`textElement.ts:571-599`): the tallest a container's text may be.
///
/// - Sticky note: the label body, `max(0, height - STICKY_NOTE_BODY_INSET_Y)`.
/// - Arrow: its height, or the text's own when the arrow is no taller than
///   `16 * BOUND_TEXT_PADDING`.
/// - Ellipse: `round(height / 2 * √2) - 2 * BOUND_TEXT_PADDING`.
/// - Diamond: `round(height / 2) - 2 * BOUND_TEXT_PADDING`.
/// - Anything else: `height - 2 * BOUND_TEXT_PADDING`.
pub fn get_bound_text_max_height(container: &Element, bound_text: &Element) -> f64 {
    let height = container.base.height;
    match element_type(container) {
        ElementType::StickyNote => js_max(0.0, height - STICKY_NOTE_BODY_INSET_Y),
        ElementType::Arrow => {
            let container_height = height - BOUND_TEXT_PADDING * 8.0 * 2.0;
            if container_height <= 0.0 {
                bound_text.base.height
            } else {
                height
            }
        }
        ElementType::Ellipse => js_round((height / 2.0) * 2f64.sqrt()) - BOUND_TEXT_PADDING * 2.0,
        ElementType::Diamond => js_round(height / 2.0) - BOUND_TEXT_PADDING * 2.0,
        _ => height - BOUND_TEXT_PADDING * 2.0,
    }
}

/// `pointRotateRads(point, center, angle)` (`packages/math/src/point.ts:125-139`).
fn point_rotate_rads(point: [f64; 2], center: [f64; 2], angle: f64) -> [f64; 2] {
    if angle == 0.0 || angle.is_nan() {
        return point;
    }
    let [x, y] = point;
    let [cx, cy] = center;
    [
        (x - cx) * angle.cos() - (y - cy) * angle.sin() + cx,
        (x - cx) * angle.sin() + (y - cy) * angle.cos() + cy,
    ]
}

/// `computeBoundTextPosition(container, boundTextElement, elementsMap)`
/// (`textElement.ts:249-324`): where a bound text's top-left corner goes.
///
/// In the container's room ([`get_container_coords`],
/// [`get_bound_text_max_width`], [`get_bound_text_max_height`]) the text is
/// placed by its `verticalAlign` and `textAlign`; a middle-aligned sticky
/// label is centred in the whole padded note while it clears the footer.
/// A rotated container turns the text's centre about the room's centre
/// (the note's centre for a sticky note). An arrow label is placed by
/// `geometry`; `None` when it cannot.
pub fn compute_bound_text_position(
    container: &Element,
    bound_text: &Element,
    elements: &[Element],
    geometry: &mut dyn ArrowLabelGeometry,
) -> Option<[f64; 2]> {
    if is_arrow(container) {
        return geometry.bound_text_element_position(container, bound_text, elements);
    }
    let text = text_fields(bound_text)?;
    let [coords_x, coords_y] = get_container_coords(container);
    let max_height = get_bound_text_max_height(container, bound_text);
    let max_width = get_bound_text_max_width(container, Some(bound_text));
    let (width, height) = (bound_text.base.width, bound_text.base.height);

    let y = match text.vertical_align {
        VerticalAlign::Top => coords_y,
        VerticalAlign::Bottom => coords_y + (max_height - height),
        VerticalAlign::Middle if is_sticky_note(container) => {
            let padded_height = container.base.height - STICKY_NOTE_PADDING * 2.0;
            coords_y + js_min((padded_height - height) / 2.0, max_height - height)
        }
        VerticalAlign::Middle => coords_y + (max_height / 2.0 - height / 2.0),
    };
    let x = match text.text_align {
        TextAlign::Left => coords_x,
        TextAlign::Right => coords_x + (max_width - width),
        TextAlign::Center => coords_x + (max_width / 2.0 - width / 2.0),
    };

    let angle = container.base.angle.0;
    // `angle !== 0` holds for NaN, which pointRotateRads then leaves alone
    if angle != 0.0 {
        let content_center = if is_sticky_note(container) {
            [
                container.base.x + container.base.width / 2.0,
                container.base.y + container.base.height / 2.0,
            ]
        } else {
            [coords_x + max_width / 2.0, coords_y + max_height / 2.0]
        };
        let text_center = [x + width / 2.0, y + height / 2.0];
        let [rx, ry] = point_rotate_rads(text_center, content_center, angle);
        return Some([rx - width / 2.0, ry - height / 2.0]);
    }
    Some([x, y])
}
