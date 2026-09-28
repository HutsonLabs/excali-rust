//! Refitting a text element to its text: `refreshTextDimensions` and the
//! anchoring it uses.
//!
//! Upstream: `packages/element/src/newElement.ts` at the pinned commit:
//! `getTextAnchorRatios` (303-315), `getTextElementPositionOffsets`
//! (317-333), `getAdjustedDimensions` (393-483), `adjustXYWithRotation`
//! (485-531) and `refreshTextDimensions` (533-580). `restoreElements` with
//! `refreshDimensions` calls the last for every text but a sticky label
//! (`packages/excalidraw/data/restore.ts:1032-1045`), which
//! [`crate::restore_env::TextEnv`] answers.
//!
//! Upstream reads the metrics provider and the per-character width cache
//! from module globals; [`TextLayout`] passes them, with the arrow geometry
//! an arrow label's box needs ([`ArrowLabelGeometry`]).

use excali_core::element::{
    Element, ElementKind, ElementType, TextAlign, TextFields, VerticalAlign,
};
use serde_json::{Map, Value};

use crate::font_metadata::get_font_string;
use crate::text_element::{get_bound_text_max_width, ArrowLabelGeometry};
use crate::text_measurements::{measure_text, CharWidthCache, TextMetricsProvider};
use crate::text_wrapping::wrap_text;

/// What measuring and placing text needs: the line-width provider, the
/// per-character width cache wrapping fills, and the arrow geometry.
pub struct TextLayout<'a> {
    pub provider: &'a dyn TextMetricsProvider,
    pub char_widths: &'a mut CharWidthCache,
    pub geometry: &'a mut dyn ArrowLabelGeometry,
}

/// `refreshTextDimensions`'s answer: the keys to assign to the text, in
/// upstream's order (`{ text, autoResize?, ...{ width, height, x, y } }`).
#[derive(Clone, Debug, PartialEq)]
pub struct RefreshedText {
    pub text: String,
    /// `Some(false)` when a growing text crossed its maximum width and
    /// wraps from now on; `None` leaves `autoResize` as it is.
    pub auto_resize: Option<bool>,
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
}

/// A number as `JSON.stringify` writes it: integers without a fraction,
/// `-0` as `0`, and non-finite numbers as `null`.
fn number(x: f64) -> Value {
    if !x.is_finite() {
        return Value::Null;
    }
    if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        return Value::from(x as i64);
    }
    serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
}

impl RefreshedText {
    /// The keys as a JSON object, in upstream's order.
    pub fn to_map(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("text".into(), Value::String(self.text.clone()));
        if let Some(auto_resize) = self.auto_resize {
            m.insert("autoResize".into(), Value::Bool(auto_resize));
        }
        m.insert("width".into(), number(self.width));
        m.insert("height".into(), number(self.height));
        m.insert("x".into(), number(self.x));
        m.insert("y".into(), number(self.y));
        m
    }
}

/// `getTextAnchorRatios({ textAlign, verticalAlign })`
/// (`newElement.ts:303-315`): the fraction of the width and height at which
/// a text is anchored, the point that stays put as it grows.
pub fn get_text_anchor_ratios(text_align: TextAlign, vertical_align: VerticalAlign) -> [f64; 2] {
    let x = match text_align {
        TextAlign::Center => 0.5,
        TextAlign::Right => 1.0,
        TextAlign::Left => 0.0,
    };
    let y = match vertical_align {
        VerticalAlign::Middle => 0.5,
        VerticalAlign::Bottom => 1.0,
        VerticalAlign::Top => 0.0,
    };
    [x, y]
}

/// The sides `adjustXYWithRotation` holds still.
#[derive(Clone, Copy)]
struct Sides {
    n: bool,
    e: bool,
    s: bool,
    w: bool,
}

/// `adjustXYWithRotation(sides, x, y, angle, deltaX1, deltaY1, deltaX2,
/// deltaY2)` (`newElement.ts:485-531`).
#[allow(clippy::too_many_arguments)]
fn adjust_xy_with_rotation(
    sides: Sides,
    mut x: f64,
    mut y: f64,
    angle: f64,
    delta_x1: f64,
    delta_y1: f64,
    delta_x2: f64,
    delta_y2: f64,
) -> [f64; 2] {
    let cos = angle.cos();
    let sin = angle.sin();
    if sides.e && sides.w {
        x += delta_x1 + delta_x2;
    } else if sides.e {
        x += delta_x1 * (1.0 + cos);
        y += delta_x1 * sin;
        x += delta_x2 * (1.0 - cos);
        y += delta_x2 * -sin;
    } else if sides.w {
        x += delta_x1 * (1.0 - cos);
        y += delta_x1 * -sin;
        x += delta_x2 * (1.0 + cos);
        y += delta_x2 * sin;
    }

    if sides.n && sides.s {
        y += delta_y1 + delta_y2;
    } else if sides.n {
        x += delta_y1 * sin;
        y += delta_y1 * (1.0 - cos);
        x += delta_y2 * -sin;
        y += delta_y2 * (1.0 + cos);
    } else if sides.s {
        x += delta_y1 * -sin;
        y += delta_y1 * (1.0 + cos);
        x += delta_y2 * sin;
        y += delta_y2 * (1.0 - cos);
    }
    [x, y]
}

fn text_fields(element: &Element) -> Option<&TextFields> {
    match &element.kind {
        ElementKind::Text(text) => Some(text),
        _ => None,
    }
}

/// `!element.containerId`: no container, or the empty id.
fn has_container_id(text: &TextFields) -> bool {
    text.container_id
        .as_deref()
        .is_some_and(|id| !id.is_empty())
}

/// The container `getElementAbsoluteCoords` finds for a text
/// (`getContainerElement(element, elementsMap)`): the element its
/// `containerId` names, among `elements`, or `container` when it is that
/// element.
fn find_container<'a>(
    text: &TextFields,
    container: Option<&'a Element>,
    elements: &'a [Element],
) -> Option<&'a Element> {
    let id = text.container_id.as_deref().filter(|id| !id.is_empty())?;
    elements
        .iter()
        .find(|e| e.base.id == id)
        .or_else(|| container.filter(|c| c.base.id == id))
}

/// `getAdjustedDimensions(element, elementsMap, nextText, nextFixedWidth)`
/// (`newElement.ts:393-483`): the size of `next_text` in the element's
/// font, and where the element must move so that its anchor stays put.
///
/// The width is `next_fixed_width` when given, the element's own when it
/// does not grow with its text (`autoResize` false), or else the measured
/// one. A free, growing text aligned centre and middle grows about its
/// centre. Any other text grows away from the sides its alignment anchors it
/// to, turned by its angle, measured from its box (an arrow label's box is
/// where the arrow puts it; `None` when the geometry cannot place it).
/// Non-finite positions fall back to the element's.
fn get_adjusted_dimensions(
    layout: &mut TextLayout<'_>,
    element: &Element,
    text: &TextFields,
    container: Option<&Element>,
    elements: &[Element],
    next_text: &str,
    next_fixed_width: Option<f64>,
) -> Option<[f64; 4]> {
    let font = get_font_string(text.font_size, text.font_family);
    let metrics = measure_text(next_text, &font, text.line_height, layout.provider);
    let next_height = metrics.height;
    let next_width = match next_fixed_width {
        Some(width) => width,
        None if !text.auto_resize => element.base.width,
        None => metrics.width,
    };

    let (text_align, vertical_align) = (text.text_align, text.vertical_align);
    let (x, y);
    if text_align == TextAlign::Center
        && vertical_align == VerticalAlign::Middle
        && !has_container_id(text)
        && text.auto_resize
    {
        let prev = measure_text(&text.text, &font, text.line_height, layout.provider);
        let [ratio_x, ratio_y] = get_text_anchor_ratios(text_align, vertical_align);
        let offset_x = (next_width - prev.width) * ratio_x;
        let offset_y = (next_height - prev.height) * ratio_y;
        x = element.base.x - offset_x;
        y = element.base.y - offset_y;
    } else {
        let (width, height) = (element.base.width, element.base.height);
        let arrow = find_container(text, container, elements)
            .filter(|c| c.kind.element_type() == ElementType::Arrow);
        let [x1, y1] = match arrow {
            Some(arrow) => layout
                .geometry
                .bound_text_element_position(arrow, element, elements)?,
            None => [element.base.x, element.base.y],
        };
        let (x2, y2) = (x1 + width, y1 + height);
        // getResizedElementAbsoluteCoords of a text: its own corner
        let (next_x1, next_y1) = (element.base.x, element.base.y);
        let (next_x2, next_y2) = (element.base.x + next_width, element.base.y + next_height);
        let delta_x1 = (x1 - next_x1) / 2.0;
        let delta_y1 = (y1 - next_y1) / 2.0;
        let delta_x2 = (x2 - next_x2) / 2.0;
        let delta_y2 = (y2 - next_y2) / 2.0;
        let sides = Sides {
            n: matches!(
                vertical_align,
                VerticalAlign::Middle | VerticalAlign::Bottom
            ),
            s: matches!(vertical_align, VerticalAlign::Middle | VerticalAlign::Top),
            e: matches!(text_align, TextAlign::Center | TextAlign::Left),
            w: matches!(text_align, TextAlign::Center | TextAlign::Right),
        };
        [x, y] = adjust_xy_with_rotation(
            sides,
            element.base.x,
            element.base.y,
            element.base.angle.0,
            delta_x1,
            delta_y1,
            delta_x2,
            delta_y2,
        );
    }

    Some([
        next_width,
        next_height,
        if x.is_finite() { x } else { element.base.x },
        if y.is_finite() { y } else { element.base.y },
    ])
}

/// `refreshTextDimensions(textElement, container, elementsMap, text,
/// maxWidth)` (`newElement.ts:533-580`): the text re-wrapped and measured,
/// and the element's new box.
///
/// - A deleted text gets no answer (`undefined`).
/// - `max_width`, for a free text that grows with its content
///   (`autoResize`) and is no wider than it: once `text` measures wider,
///   the text wraps at `max_width`, takes that width and stops growing
///   (`autoResize: false`).
/// - Otherwise a text in a container wraps to the container's
///   [`get_bound_text_max_width`], a fixed-width free text to its own
///   width, and a growing free text keeps its lines.
///
/// `text` defaults to the element's `text` (not `originalText`);
/// `elements` are the scene's (upstream's `elementsMap`). `None` also when
/// `text_element` is not a text, or is an arrow label the geometry cannot
/// place.
pub fn refresh_text_dimensions(
    layout: &mut TextLayout<'_>,
    text_element: &Element,
    container: Option<&Element>,
    elements: &[Element],
    text: Option<&str>,
    max_width: Option<f64>,
) -> Option<RefreshedText> {
    if text_element.base.is_deleted {
        return None;
    }
    let fields = text_fields(text_element)?;
    let mut text = text.unwrap_or(&fields.text).to_owned();
    let font = get_font_string(fields.font_size, fields.font_family);
    if let Some(max_width) = max_width {
        if container.is_none() && fields.auto_resize && text_element.base.width <= max_width {
            let width = measure_text(&text, &font, fields.line_height, layout.provider).width;
            if width > max_width {
                let wrapped =
                    wrap_text(&text, &font, max_width, layout.provider, layout.char_widths);
                let [width, height, x, y] = get_adjusted_dimensions(
                    layout,
                    text_element,
                    fields,
                    container,
                    elements,
                    &wrapped,
                    Some(max_width),
                )?;
                return Some(RefreshedText {
                    text: wrapped,
                    auto_resize: Some(false),
                    width,
                    height,
                    x,
                    y,
                });
            }
        }
    }
    if container.is_some() || !fields.auto_resize {
        let wrap_width = match container {
            Some(container) => get_bound_text_max_width(container, Some(text_element)),
            None => text_element.base.width,
        };
        text = wrap_text(
            &text,
            &font,
            wrap_width,
            layout.provider,
            layout.char_widths,
        );
    }
    let [width, height, x, y] = get_adjusted_dimensions(
        layout,
        text_element,
        fields,
        container,
        elements,
        &text,
        None,
    )?;
    Some(RefreshedText {
        text,
        auto_resize: None,
        width,
        height,
        x,
        y,
    })
}
