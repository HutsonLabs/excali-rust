//! The scene the editor mutates, and `mutateElement`
//! (`packages/element/src/mutateElement.ts`, `Scene.mutateElement`,
//! `packages/element/src/Scene.ts:411-445`).
//!
//! Every change to an element goes through [`Scene::mutate_element`] with
//! an [`ElementUpdate`], which keeps upstream's rules: a key whose value is
//! unchanged is not written; any change bumps `version`, draws a new
//! `versionNonce` and stamps `updated` ([`MutationEnv`]); new points of a
//! line or freedraw give its `width` and `height` unless the update sets
//! them; and an elbow arrow whose points or fixed segments change is
//! re-routed (`updateElbowArrowPoints`, [`crate::elbow_arrow`]).

use excali_core::element::{
    BoundElement, Element, ElementKind, FixedPointBinding, FixedSegment, LocalPoint, Radians,
};
use excali_math::js;
use excali_scene::bounds::ElementsMap;
use serde_json::Value;

use crate::elbow_arrow::{self, ElbowArrowUpdates};

/// Where `mutateElement` draws its `versionNonce` (`randomInteger()`) and
/// `updated` timestamp (`getUpdatedTimestamp()`).
pub trait MutationEnv {
    /// `randomInteger()` (`common/src/random.ts:9`).
    fn random_integer(&mut self) -> f64;
    /// `getUpdatedTimestamp()`: `Date.now()`, or 1 in upstream's tests.
    fn now(&mut self) -> f64;
}

/// `ElementUpdate<ExcalidrawElement>`: the keys to assign. `None` is a key
/// left out (or holding `undefined`, which `mutateElement` skips); for a
/// nullable key, `Some(None)` assigns `null`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementUpdate {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub angle: Option<f64>,
    /// A line's, arrow's or freedraw's points.
    pub points: Option<Vec<LocalPoint>>,
    pub font_size: Option<f64>,
    pub base_font_size: Option<Option<f64>>,
    pub text: Option<String>,
    pub auto_resize: Option<bool>,
    /// An image's `scale`.
    pub scale: Option<[f64; 2]>,
    pub start_binding: Option<Option<FixedPointBinding>>,
    pub end_binding: Option<Option<FixedPointBinding>>,
    pub fixed_segments: Option<Option<Vec<FixedSegment>>>,
    pub start_is_special: Option<Option<bool>>,
    pub end_is_special: Option<Option<bool>>,
    pub bound_elements: Option<Option<Vec<BoundElement>>>,
    /// A sticky note's `baseHeight`.
    pub base_height: Option<f64>,
    /// `moveMidPointsWithElement`: not an element field upstream, but
    /// `LinearElementEditor.movePoints` passes its options on to
    /// `mutateElement` (`...otherUpdates`), which writes the key onto the
    /// arrow like any other; the port keeps it with the element's unknown
    /// keys ([`Element::extra`]).
    pub move_mid_points_with_element: Option<bool>,
    /// `frameId` (`Some(None)` is `null`).
    pub frame_id: Option<Option<String>>,
    /// A text's `containerId` (`Some(None)` is `null`).
    pub container_id: Option<Option<String>>,
}

impl ElementUpdate {
    /// `Object.keys(updates).length === 0`.
    pub fn is_empty(&self) -> bool {
        *self == ElementUpdate::default()
    }

    /// An update of `x` and `y`.
    pub fn position(x: f64, y: f64) -> ElementUpdate {
        ElementUpdate {
            x: Some(x),
            y: Some(y),
            ..ElementUpdate::default()
        }
    }

    /// Whether the update has keys other than `points`, `fixedSegments`,
    /// `startBinding` and `endBinding` (`restOfTheUpdates`,
    /// `elbowArrow.ts:1020`).
    fn has_other_keys(&self) -> bool {
        let rest = ElementUpdate {
            points: None,
            fixed_segments: None,
            start_binding: None,
            end_binding: None,
            ..self.clone()
        };
        !rest.is_empty()
    }
}

/// The scene: every element in order, deleted ones included
/// (`Scene.getElementsIncludingDeleted`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    elements: Vec<Element>,
}

/// `getSizeFromPoints(points)` (`common/src/points.ts:10-19`).
pub fn get_size_from_points(points: &[LocalPoint]) -> (f64, f64) {
    let fold = |f: fn(f64, f64) -> f64, init: f64, i: usize| {
        points.iter().fold(init, |acc, p| f(acc, p[i]))
    };
    (
        fold(js::max, f64::NEG_INFINITY, 0) - fold(js::min, f64::INFINITY, 0),
        fold(js::max, f64::NEG_INFINITY, 1) - fold(js::min, f64::INFINITY, 1),
    )
}

/// JavaScript truthiness of a number (`updates.x || element.x`).
fn truthy(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}

impl Scene {
    pub fn new(elements: Vec<Element>) -> Scene {
        Scene { elements }
    }

    /// Every element, deleted ones included, in scene order.
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    /// The elements that are not deleted (`getNonDeletedElements`).
    pub fn non_deleted(&self) -> Vec<&Element> {
        self.elements
            .iter()
            .filter(|e| !e.base.is_deleted)
            .collect()
    }

    /// `getNonDeletedElementsMap()`.
    pub fn elements_map(&self) -> ElementsMap<'_> {
        ElementsMap::new(self.elements.iter().filter(|e| !e.base.is_deleted))
    }

    /// The element with `id` (deleted or not).
    pub fn get(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.base.id == id)
    }

    /// The non-deleted element with `id` (`getNonDeletedElementsMap().get`).
    pub fn get_non_deleted(&self, id: &str) -> Option<&Element> {
        self.get(id).filter(|e| !e.base.is_deleted)
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.elements.iter().position(|e| e.base.id == id)
    }

    /// Puts `element` in place of the element with its id (or appends it):
    /// for callers that computed the element whole.
    pub fn replace_element(&mut self, element: Element) {
        match self.index_of(&element.base.id) {
            Some(i) => self.elements[i] = element,
            None => self.elements.push(element),
        }
    }

    /// `scene.getSelectedElements({ selectedElementIds })`: the non-deleted
    /// elements whose id is in `ids`, in scene order.
    pub fn selected<'a>(&'a self, ids: &[String]) -> Vec<&'a Element> {
        self.elements
            .iter()
            .filter(|e| !e.base.is_deleted && ids.contains(&e.base.id))
            .collect()
    }

    /// `scene.mutateElement(element, updates)` (`Scene.ts:411-445`,
    /// `mutateElement.ts:40-138`). Returns whether anything changed. An
    /// element not in the scene is left alone.
    pub fn mutate_element(
        &mut self,
        id: &str,
        update: ElementUpdate,
        env: &mut dyn MutationEnv,
    ) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        let mut update = update;
        let element = &self.elements[index];

        let is_elbow = matches!(&element.kind, ElementKind::Arrow(a) if a.elbowed);
        if is_elbow
            && (update.is_empty() || update.points.is_some() || update.fixed_segments.is_some())
        {
            // updateElbowArrowPoints({ ...element, x: updates.x || element.x,
            // y: updates.y || element.y }, elementsMap, updates)
            let mut arrow = element.clone();
            arrow.base.x = update.x.filter(|&v| truthy(v)).unwrap_or(element.base.x);
            arrow.base.y = update.y.filter(|&v| truthy(v)).unwrap_or(element.base.y);
            let routed = {
                let map = elbow_arrow::ElementsMap::new(
                    self.elements.iter().filter(|e| !e.base.is_deleted),
                );
                elbow_arrow::update_elbow_arrow_points(
                    &arrow,
                    &map,
                    &ElbowArrowUpdates {
                        points: update.points.clone(),
                        fixed_segments: update.fixed_segments.clone(),
                        start_binding: update.start_binding.clone(),
                        end_binding: update.end_binding.clone(),
                        other_keys: update.has_other_keys(),
                    },
                )
            };
            update.angle = Some(0.0);
            if let Ok(routed) = routed {
                if routed.points.is_some() {
                    update.points = routed.points;
                }
                if routed.x.is_some() {
                    update.x = routed.x;
                }
                if routed.y.is_some() {
                    update.y = routed.y;
                }
                if routed.width.is_some() {
                    update.width = routed.width;
                }
                if routed.height.is_some() {
                    update.height = routed.height;
                }
                if routed.fixed_segments.is_some() {
                    update.fixed_segments = routed.fixed_segments;
                }
                if routed.start_is_special.is_some() {
                    update.start_is_special = routed.start_is_special;
                }
                if routed.end_is_special.is_some() {
                    update.end_is_special = routed.end_is_special;
                }
                if routed.start_binding.is_some() {
                    update.start_binding = routed.start_binding;
                }
                if routed.end_binding.is_some() {
                    update.end_binding = routed.end_binding;
                }
            }
        } else if let Some(points) = &update.points {
            // { ...getSizeFromPoints(points), ...updates }
            let (width, height) = get_size_from_points(points);
            update.width = update.width.or(Some(width));
            update.height = update.height.or(Some(height));
        }

        let element = &mut self.elements[index];
        let changed = apply(element, update);
        if changed {
            element.base.version += 1.0;
            element.base.version_nonce = env.random_integer();
            element.base.updated = env.now();
        }
        changed
    }
}

/// Assigns a number unless it is `===` the current one.
fn set_number(slot: &mut f64, value: Option<f64>) -> bool {
    match value {
        Some(v) if !(*slot == v) => {
            *slot = v;
            true
        }
        _ => false,
    }
}

/// Assigns a nullable object: `null` over `null` is no change; any object
/// is (upstream compares objects by reference).
fn set_object<T>(slot: &mut Option<T>, value: Option<Option<T>>) -> bool {
    match value {
        None => false,
        Some(None) if slot.is_none() => false,
        Some(v) => {
            *slot = v;
            true
        }
    }
}

/// Assigns a nullable string unless it is `===` the current one.
fn set_string(slot: &mut Option<String>, value: Option<Option<String>>) -> bool {
    match value {
        Some(v) if *slot != v => {
            *slot = v;
            true
        }
        _ => false,
    }
}

/// Assigns an optional (absent-able) nullable object key.
fn set_absent_object<T>(slot: &mut Option<Option<T>>, value: Option<Option<T>>) -> bool {
    match value {
        None => false,
        Some(None) if matches!(slot, Some(None)) => false,
        Some(v) => {
            *slot = Some(v);
            true
        }
    }
}

/// `points`: unchanged when the lengths match and every point but the
/// first is equal (`while (--index)` never compares index 0).
fn set_points(slot: &mut Vec<LocalPoint>, value: Option<Vec<LocalPoint>>) -> bool {
    let Some(next) = value else {
        return false;
    };
    if slot.len() == next.len()
        && slot
            .iter()
            .zip(&next)
            .skip(1)
            .all(|(a, b)| a[0] == b[0] && a[1] == b[1])
    {
        return false;
    }
    *slot = next;
    true
}

/// `mutateElement`'s loop over the keys (`mutateElement.ts:78-120`).
fn apply(element: &mut Element, update: ElementUpdate) -> bool {
    let mut changed = false;
    let b = &mut element.base;
    changed |= set_number(&mut b.x, update.x);
    changed |= set_number(&mut b.y, update.y);
    changed |= set_number(&mut b.width, update.width);
    changed |= set_number(&mut b.height, update.height);
    if let Some(angle) = update.angle {
        if !(b.angle.0 == angle) {
            b.angle = Radians(angle);
            changed = true;
        }
    }
    changed |= set_object(&mut b.bound_elements, update.bound_elements);
    changed |= set_string(&mut b.frame_id, update.frame_id);
    if let Some(v) = update.move_mid_points_with_element {
        let key = "moveMidPointsWithElement";
        if element.extra.get(key) != Some(&Value::Bool(v)) {
            element.extra.insert(key.to_owned(), Value::Bool(v));
            changed = true;
        }
    }

    match &mut element.kind {
        ElementKind::Line(line) => {
            let l = &mut line.linear;
            changed |= set_points(&mut l.points, update.points);
            changed |= set_object(&mut l.start_binding, update.start_binding);
            changed |= set_object(&mut l.end_binding, update.end_binding);
        }
        ElementKind::Arrow(arrow) => {
            let l = &mut arrow.linear;
            changed |= set_points(&mut l.points, update.points);
            changed |= set_object(&mut l.start_binding, update.start_binding);
            changed |= set_object(&mut l.end_binding, update.end_binding);
            changed |= set_absent_object(&mut arrow.fixed_segments, update.fixed_segments);
            if let Some(v) = update.start_is_special {
                if arrow.start_is_special != Some(v) {
                    arrow.start_is_special = Some(v);
                    changed = true;
                }
            }
            if let Some(v) = update.end_is_special {
                if arrow.end_is_special != Some(v) {
                    arrow.end_is_special = Some(v);
                    changed = true;
                }
            }
        }
        ElementKind::Freedraw(freedraw) => {
            changed |= set_points(&mut freedraw.points, update.points);
        }
        ElementKind::Text(text) => {
            changed |= set_string(&mut text.container_id, update.container_id);
            changed |= set_number(&mut text.font_size, update.font_size);
            if let Some(v) = update.base_font_size {
                if !(text.base_font_size == v) {
                    text.base_font_size = v;
                    changed = true;
                }
            }
            if let Some(v) = update.text {
                if text.text != v {
                    text.text = v;
                    changed = true;
                }
            }
            if let Some(v) = update.auto_resize {
                if text.auto_resize != v {
                    text.auto_resize = v;
                    changed = true;
                }
            }
        }
        ElementKind::Image(image) => {
            if let Some(scale) = update.scale {
                if image.scale[0] != scale[0] || image.scale[1] != scale[1] {
                    image.scale = scale;
                    changed = true;
                }
            }
        }
        ElementKind::StickyNote(note) => {
            changed |= set_number(&mut note.base_height, update.base_height);
        }
        _ => {}
    }
    changed
}
