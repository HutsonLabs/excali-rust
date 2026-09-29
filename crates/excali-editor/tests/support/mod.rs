//! Shared helpers for the history tests: a deterministic environment and
//! the element factory of upstream's `API.createElement`
//! (`packages/excalidraw/tests/helpers/api.ts:160-420`: `x = 0`, `y = x`,
//! `width = 100`, `height = width`, `version` 1, no index).

#![allow(dead_code)]

use excali_core::element::{
    ArrowFields, Element, ElementBase, ElementKind, FontFamily, FrameFields, ImageFields,
    LinearFields, TextFields,
};
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_editor::binding::{update_bound_elements_in_map, BindingEnv};
use excali_editor::mutate::{mutate_element, new_element_with};
use excali_editor::scene::MutationEnv;
use excali_editor::store::HistoryEnv;
use excali_editor::text_layout::TextLayouter;
use excali_text::font_metadata::get_font_string;
use excali_text::text_measurements::{
    measure_text, CharCountTextMetrics, CharWidthCache, TextMetricsProvider,
};
use serde_json::{json, Map, Value};

/// `reseed(7)` and `isTestEnv()`: nonces and ids from counters, and
/// `getUpdatedTimestamp()` answering 1 as it does in tests
/// (`packages/common/src/utils.ts:552`).
///
/// The two leaf layout calls of `redrawElements` are recorded in
/// `text_redraws` (`(text, container)`) and `bound_updates` (the bindable
/// element, with the ids of the changed elements passed along), and do
/// nothing else unless a test installs a layout:
/// [`TextLayoutMode::Upstream`] is the editor's `redrawTextBoundingBox`
/// ([`TextLayouter::redraw_text_bounding_box`]) under upstream's test
/// metric (10 px per UTF-16 code unit); the arrow layout is the port's own
/// `updateBoundElements` ([`ArrowLayout::Upstream`], the default of
/// `HistoryEnv::update_bound_elements`) or a test's own.
#[derive(Default)]
pub struct TestEnv {
    pub nonce: f64,
    pub ids: u32,
    pub text_layout: TextLayoutMode,
    pub arrow_layout: Option<ArrowLayout>,
    pub text_redraws: Vec<(String, String)>,
    pub bound_updates: Vec<(String, Vec<String>)>,
    /// Upstream's production build (`isTestEnv() || isDevEnv()` false):
    /// applying a delta carries on past errors instead of failing.
    pub production: bool,
    /// Upstream's test metric (10 px per UTF-16 code unit) for arrow labels.
    pub char_widths: CharWidthCache,
    /// Text layout's state: the metric, the wrapping cache and the original
    /// container heights.
    pub layouter: TextLayouter<CharCountTextMetrics>,
}

/// What `redrawTextBoundingBox` does in a test.
#[derive(Default, Clone, Copy)]
pub enum TextLayoutMode {
    /// Nothing (the call is only recorded).
    #[default]
    Off,
    /// The editor's own layout ([`TextLayouter::redraw_text_bounding_box`]).
    Upstream,
    /// A test's layout.
    Custom(TextLayout),
}

/// `redrawTextBoundingBox(text, container)` over the scene.
pub type TextLayout =
    fn(&mut dyn ChangeStamp, &mut SceneElementsMap, &str, &str) -> Result<(), String>;

/// `updateBoundElements(element, scene, { changedElements })`.
#[derive(Clone, Copy)]
pub enum ArrowLayout {
    /// The port's `updateBoundElements`
    /// (`excali_editor::binding::update_bound_elements_in_map`).
    Upstream,
    /// A test's own layout.
    Custom(
        fn(
            &mut dyn ChangeStamp,
            &mut SceneElementsMap,
            &str,
            &SceneElementsMap,
        ) -> Result<(), String>,
    ),
}

impl TestEnv {
    /// Both layouts installed: the editor's text layout and the port's
    /// arrow layout.
    pub fn with_layout() -> TestEnv {
        TestEnv {
            text_layout: TextLayoutMode::Upstream,
            arrow_layout: Some(ArrowLayout::Upstream),
            ..TestEnv::default()
        }
    }
}

impl MutationEnv for TestEnv {
    fn random_integer(&mut self) -> f64 {
        self.version_nonce()
    }

    fn now(&mut self) -> f64 {
        self.updated()
    }
}

impl BindingEnv for TestEnv {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (&CharCountTextMetrics, &mut self.char_widths)
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

/// The test environment's stamp and metric as a [`BindingEnv`], for the
/// arrows a sticky note's layout moves.
struct StampBinding<'a> {
    stamp: &'a mut dyn ChangeStamp,
    char_widths: &'a mut CharWidthCache,
}

impl MutationEnv for StampBinding<'_> {
    fn random_integer(&mut self) -> f64 {
        self.stamp.version_nonce()
    }

    fn now(&mut self) -> f64 {
        self.stamp.updated()
    }
}

impl BindingEnv for StampBinding<'_> {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (&CharCountTextMetrics, self.char_widths)
    }
}

impl HistoryEnv for TestEnv {
    fn dev_checks(&self) -> bool {
        !self.production
    }

    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        (&CharCountTextMetrics, &mut self.char_widths)
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
        let mut stamp = Stamp {
            nonce: &mut self.nonce,
        };
        match self.text_layout {
            TextLayoutMode::Off => Ok(()),
            TextLayoutMode::Custom(layout) => layout(&mut stamp, elements, text_id, container_id),
            TextLayoutMode::Upstream => {
                // a sticky note's arrows follow it (updateStickyNoteLayout)
                let arrows = self.arrow_layout;
                let char_widths = &mut self.char_widths;
                self.layouter.redraw_text_bounding_box(
                    &mut stamp,
                    elements,
                    text_id,
                    Some(container_id),
                    &mut |stamp, elements, id| match arrows {
                        Some(ArrowLayout::Upstream) => {
                            let mut env = StampBinding {
                                stamp,
                                char_widths: &mut *char_widths,
                            };
                            update_bound_elements_in_map(
                                elements,
                                id,
                                &SceneElementsMap::new(),
                                &mut env,
                            );
                            Ok(())
                        }
                        Some(ArrowLayout::Custom(layout)) => {
                            layout(stamp, elements, id, &SceneElementsMap::new())
                        }
                        None => Ok(()),
                    },
                )
            }
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
            Some(ArrowLayout::Upstream) => {
                update_bound_elements_in_map(elements, element_id, changed, self);
                Ok(())
            }
            Some(ArrowLayout::Custom(layout)) => {
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
