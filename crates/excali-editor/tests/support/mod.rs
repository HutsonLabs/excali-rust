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
use excali_editor::mutate::new_element_with;
use excali_editor::store::HistoryEnv;
use serde_json::{Map, Value};

/// `reseed(7)` and `isTestEnv()`: nonces and ids from counters, and
/// `getUpdatedTimestamp()` answering 1 as it does in tests
/// (`packages/common/src/utils.ts:552`). Layout (`redrawElements`) does
/// nothing unless a test installs a hook.
#[derive(Default)]
pub struct TestEnv {
    pub nonce: f64,
    pub ids: u32,
    pub redraw: Option<RedrawHook>,
    pub redraw_calls: u32,
}

pub type RedrawHook =
    fn(&mut dyn ChangeStamp, &mut SceneElementsMap, &SceneElementsMap) -> Result<(), String>;

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
    fn random_id(&mut self) -> String {
        self.ids += 1;
        format!("delta-{}", self.ids)
    }

    fn redraw_elements(
        &mut self,
        elements: &mut SceneElementsMap,
        changed: &SceneElementsMap,
    ) -> Result<(), String> {
        self.redraw_calls += 1;
        match self.redraw {
            Some(hook) => {
                let mut stamp = Stamp {
                    nonce: &mut self.nonce,
                };
                hook(&mut stamp, elements, changed)
            }
            None => Ok(()),
        }
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

/// `API.createElement({ type: "text", id, text, x, y })`.
pub fn text(id: &str, content: &str, x: f64, y: f64) -> Element {
    element(
        ElementKind::Text(TextFields::new(content, FontFamily::default(), 1.25)),
        id,
        x,
        y,
        100.0,
        100.0,
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
