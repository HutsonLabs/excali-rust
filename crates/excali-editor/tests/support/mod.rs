//! Shared helpers for the history tests: a deterministic environment and
//! the element factory of upstream's `API.createElement`
//! (`packages/excalidraw/tests/helpers/api.ts:160-420`: `x = 0`, `y = x`,
//! `width = 100`, `height = width`, `version` 1, no index).

#![allow(dead_code)]

use excali_core::element::{
    ArrowFields, BoundElementType, Element, ElementBase, ElementKind, FixedPointBinding,
    FontFamily, FrameFields, ImageFields, LinearFields, TextFields,
};
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_editor::mutate::{mutate_element, new_element_with};
use excali_editor::store::HistoryEnv;
use excali_text::font_metadata::get_font_string;
use excali_text::text_element::{
    compute_bound_text_position, compute_container_dimension_for_bound_text,
    get_bound_text_max_height, get_bound_text_max_width, NoArrowGeometry,
};
use excali_text::text_measurements::{measure_text, CharCountTextMetrics, CharWidthCache};
use excali_text::text_wrapping::wrap_text;
use serde_json::{json, Map, Value};

/// `reseed(7)` and `isTestEnv()`: nonces and ids from counters, and
/// `getUpdatedTimestamp()` answering 1 as it does in tests
/// (`packages/common/src/utils.ts:552`).
///
/// The two leaf layout calls of `redrawElements` are recorded in
/// `text_redraws` (`(text, container)`) and `bound_updates` (the bindable
/// element, with the ids of the changed elements passed along), and do
/// nothing else unless a test installs a layout: [`centre_label`] and
/// [`follow_bindings`] are deterministic stand-ins for upstream's text and
/// arrow layout, which live with text editing and arrow binding.
#[derive(Default)]
pub struct TestEnv {
    pub nonce: f64,
    pub ids: u32,
    pub text_layout: Option<TextLayout>,
    pub arrow_layout: Option<ArrowLayout>,
    pub text_redraws: Vec<(String, String)>,
    pub bound_updates: Vec<(String, Vec<String>)>,
    /// Upstream's production build (`isTestEnv() || isDevEnv()` false):
    /// applying a delta carries on past errors instead of failing.
    pub production: bool,
}

/// `redrawTextBoundingBox(text, container)` over the scene.
pub type TextLayout =
    fn(&mut dyn ChangeStamp, &mut SceneElementsMap, &str, &str) -> Result<(), String>;

/// `updateBoundElements(element, scene, { changedElements })`.
pub type ArrowLayout =
    fn(&mut dyn ChangeStamp, &mut SceneElementsMap, &str, &SceneElementsMap) -> Result<(), String>;

impl TestEnv {
    /// Both deterministic layouts installed.
    pub fn with_layout() -> TestEnv {
        TestEnv {
            text_layout: Some(centre_label),
            arrow_layout: Some(follow_bindings),
            ..TestEnv::default()
        }
    }
}

impl ChangeStamp for TestEnv {
    fn version_nonce(&mut self) -> f64 {
        self.nonce += 1.0;
        1000.0 + self.nonce
    }

    fn updated(&mut self) -> f64 {
        1.0
    }
}

struct Stamp<'a> {
    nonce: &'a mut f64,
}

impl ChangeStamp for Stamp<'_> {
    fn version_nonce(&mut self) -> f64 {
        *self.nonce += 1.0;
        1000.0 + *self.nonce
    }

    fn updated(&mut self) -> f64 {
        1.0
    }
}

impl HistoryEnv for TestEnv {
    fn dev_checks(&self) -> bool {
        !self.production
    }

    fn random_id(&mut self) -> String {
        self.ids += 1;
        format!("delta-{}", self.ids)
    }

    fn redraw_text_bounding_box(
        &mut self,
        elements: &mut SceneElementsMap,
        text_id: &str,
        container_id: &str,
    ) -> Result<(), String> {
        self.text_redraws
            .push((text_id.to_owned(), container_id.to_owned()));
        match self.text_layout {
            Some(layout) => {
                let mut stamp = Stamp {
                    nonce: &mut self.nonce,
                };
                layout(&mut stamp, elements, text_id, container_id)
            }
            None => Ok(()),
        }
    }

    fn update_bound_elements(
        &mut self,
        elements: &mut SceneElementsMap,
        element_id: &str,
        changed: &SceneElementsMap,
    ) -> Result<(), String> {
        self.bound_updates
            .push((element_id.to_owned(), changed.keys().cloned().collect()));
        match self.arrow_layout {
            Some(layout) => {
                let mut stamp = Stamp {
                    nonce: &mut self.nonce,
                };
                layout(&mut stamp, elements, element_id, changed)
            }
            None => Ok(()),
        }
    }
}

/// `mutateElement(elements[id], updates)` written back into `elements`.
pub fn mutate_in(
    stamp: &mut dyn ChangeStamp,
    elements: &mut SceneElementsMap,
    id: &str,
    updates: Value,
) -> Result<(), String> {
    let mut element = elements
        .get(id)
        .cloned()
        .ok_or_else(|| format!("no element {id}"))?;
    mutate_element(&mut element, elements, obj(updates), stamp).map_err(|e| e.to_string())?;
    elements.insert(id.to_owned(), element);
    Ok(())
}

/// `redrawTextBoundingBox(text, container, scene)` (`textElement.ts:51-152`)
/// under upstream's test metric (10 px per UTF-16 code unit): the
/// `originalText` wrapped to the container's room and measured, the
/// container grown when the text no longer fits, and the text placed by
/// `computeBoundTextPosition` with the container's angle (0 in an arrow).
/// An arrow label keeps its place (no arrow geometry here) and a sticky
/// note's layout (`updateStickyNoteLayout`) is not reproduced.
pub fn centre_label(
    stamp: &mut dyn ChangeStamp,
    elements: &mut SceneElementsMap,
    text_id: &str,
    container_id: &str,
) -> Result<(), String> {
    let container = elements[container_id].clone();
    let label = elements[text_id].clone();
    let ElementKind::Text(fields) = &label.kind else {
        return Err(format!("{text_id} is not a text"));
    };
    if matches!(container.kind, ElementKind::StickyNote(_)) {
        return Ok(());
    }
    let is_arrow = matches!(container.kind, ElementKind::Arrow(_));
    let font = get_font_string(fields.font_size, fields.font_family);
    let max_width = get_bound_text_max_width(&container, Some(&label));
    let wrapped = wrap_text(
        &fields.original_text,
        &font,
        max_width,
        &CharCountTextMetrics,
        &mut CharWidthCache::new(),
    );
    let metrics = measure_text(&wrapped, &font, fields.line_height, &CharCountTextMetrics);
    let width = if fields.auto_resize {
        metrics.width
    } else {
        label.base.width
    };
    let angle = if is_arrow {
        0.0
    } else {
        container.base.angle.0
    };

    let max_container_height = get_bound_text_max_height(&container, &label);
    if !is_arrow && metrics.height > max_container_height {
        let height =
            compute_container_dimension_for_bound_text(metrics.height, container.element_type());
        mutate_in(stamp, elements, container_id, json!({"height": height}))?;
    }
    if metrics.width > max_width {
        let width =
            compute_container_dimension_for_bound_text(metrics.width, container.element_type());
        mutate_in(stamp, elements, container_id, json!({"width": width}))?;
    }

    let mut updates = json!({
        "text": wrapped,
        "width": width,
        "height": metrics.height,
        "angle": angle,
    });
    let mut updated = label.clone();
    updated.base.width = width;
    updated.base.height = metrics.height;
    let container = elements[container_id].clone();
    let scene: Vec<Element> = elements.values().cloned().collect();
    if let Some([x, y]) =
        compute_bound_text_position(&container, &updated, &scene, &mut NoArrowGeometry)
    {
        updates["x"] = json!(x);
        updates["y"] = json!(y);
    }
    mutate_in(stamp, elements, text_id, updates)
}

/// Where `binding` puts an arrow end on `target`: its fixed point, in
/// scene coordinates.
fn fixed_point_of(target: &Element, binding: &FixedPointBinding) -> [f64; 2] {
    [
        target.base.x + binding.fixed_point[0] * target.base.width,
        target.base.y + binding.fixed_point[1] * target.base.height,
    ]
}

/// A stand-in for `updateBoundElements` without routing: every
/// non-deleted arrow in `boundElements` of `element_id` (read from
/// `changed` first, as upstream does) gets the ends bound to that element
/// moved onto their fixed points; the other points keep their place in
/// the scene.
pub fn follow_bindings(
    stamp: &mut dyn ChangeStamp,
    elements: &mut SceneElementsMap,
    element_id: &str,
    changed: &SceneElementsMap,
) -> Result<(), String> {
    let lookup = |elements: &SceneElementsMap, id: &str| -> Option<Element> {
        changed
            .get(id)
            .or_else(|| elements.get(id))
            .filter(|e| !e.base.is_deleted)
            .cloned()
    };
    let Some(target) = lookup(elements, element_id) else {
        return Ok(());
    };
    for bound in target.base.bound_elements.iter().flatten() {
        if bound.kind != BoundElementType::Arrow {
            continue;
        }
        let Some(arrow) = lookup(elements, &bound.id) else {
            continue;
        };
        let Some(linear) = arrow.kind.linear() else {
            continue;
        };
        let (x, y) = (arrow.base.x, arrow.base.y);
        let mut absolute: Vec<[f64; 2]> =
            linear.points.iter().map(|p| [x + p[0], y + p[1]]).collect();
        let last = absolute.len() - 1;
        if let Some(b) = linear
            .start_binding
            .as_ref()
            .filter(|b| b.element_id == element_id)
        {
            absolute[0] = fixed_point_of(&target, b);
        }
        if let Some(b) = linear
            .end_binding
            .as_ref()
            .filter(|b| b.element_id == element_id)
        {
            absolute[last] = fixed_point_of(&target, b);
        }
        let origin = absolute[0];
        let points: Vec<[f64; 2]> = absolute
            .iter()
            .map(|p| [p[0] - origin[0], p[1] - origin[1]])
            .collect();
        mutate_in(
            stamp,
            elements,
            &bound.id,
            json!({"x": origin[0], "y": origin[1], "points": points}),
        )?;
    }
    Ok(())
}

fn element(kind: ElementKind, id: &str, x: f64, y: f64, width: f64, height: f64) -> Element {
    let mut base = ElementBase::new(id, x, y, 1.0, 1.0);
    base.width = width;
    base.height = height;
    Element::new(base, kind)
}

/// `API.createElement({ type: "rectangle", id, x, y })`.
pub fn rect(id: &str, x: f64, y: f64) -> Element {
    element(ElementKind::Rectangle, id, x, y, 100.0, 100.0)
}

/// A rectangle of the given size.
pub fn rect_sized(id: &str, x: f64, y: f64, width: f64, height: f64) -> Element {
    element(ElementKind::Rectangle, id, x, y, width, height)
}

/// `API.createElement({ type: "text", id, text, x, y })`: measured as
/// `newTextElement` measures it, under upstream's test metric.
pub fn text(id: &str, content: &str, x: f64, y: f64) -> Element {
    let fields = TextFields::new(content, FontFamily::default(), 1.25);
    let font = get_font_string(fields.font_size, fields.font_family);
    let metrics = measure_text(content, &font, fields.line_height, &CharCountTextMetrics);
    element(
        ElementKind::Text(fields),
        id,
        x,
        y,
        metrics.width,
        metrics.height,
    )
}

/// `API.createElement({ type: "arrow", id })` with the given points.
pub fn arrow(id: &str, points: Vec<[f64; 2]>) -> Element {
    element(
        ElementKind::Arrow(ArrowFields::new(LinearFields::new(points), false)),
        id,
        0.0,
        0.0,
        100.0,
        100.0,
    )
}

/// `API.createElement({ type: "image", id })`: scale `[1, 1]`.
pub fn image(id: &str) -> Element {
    element(
        ElementKind::Image(ImageFields::default()),
        id,
        0.0,
        0.0,
        100.0,
        100.0,
    )
}

/// `API.createElement({ type: "frame", id, x, width })`.
pub fn frame(id: &str, x: f64, width: f64) -> Element {
    element(
        ElementKind::Frame(FrameFields { name: None }),
        id,
        x,
        x,
        width,
        width,
    )
}

/// A JSON object from a `json!` literal.
pub fn obj(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("not an object: {other}"),
    }
}

/// `newElementWith(element, updates)`.
pub fn with(element: &Element, updates: Value, env: &mut TestEnv) -> Element {
    new_element_with(element, obj(updates), false, env).expect("newElementWith")
}

/// `element[key]`, `null` when absent, with integral numbers as integers
/// so that it compares equal to a `json!` literal (the model writes an
/// `f64` field as a float).
pub fn prop(element: &Element, key: &str) -> Value {
    normalize(element.to_map().get(key).cloned().unwrap_or(Value::Null))
}

fn normalize(value: Value) -> Value {
    match value {
        Value::Number(n) => match n.as_f64() {
            Some(x) if x.fract() == 0.0 && x.abs() < 9e15 => Value::from(x as i64),
            _ => Value::Number(n),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(normalize).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, normalize(v))).collect())
        }
        other => other,
    }
}
