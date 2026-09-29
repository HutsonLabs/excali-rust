//! A [`RestoreEnv`] with the element geometry restore needs.
//!
//! Two steps of upstream's restore need code that lives here, so
//! `excali-core` asks its environment for them:
//!
//! - `restoreElements` with `repairBindings` re-routes an unbound elbow
//!   arrow whose segments are not all axis-aligned from `[0, 0]` to its
//!   last point (`restore.ts:1076-1093`,
//!   [`RestoreEnv::update_elbow_arrow_points`]), answered with
//!   [`update_elbow_arrow_points`];
//! - `restoreElement` migrates an arrow binding saved without `mode` whose
//!   target exists (`restore.ts:347-418`,
//!   [`RestoreEnv::migrate_legacy_binding`]), answered with
//!   [`migrate_legacy_binding`] (hit testing, binding geometry).
//!
//! [`RoutingEnv`] wraps another environment, answers those two requests
//! and hands every other one to the wrapped environment.

use excali_core::element::{BindMode, Element, FixedPointBinding};
use excali_core::json::number_to_string;
use excali_core::restore::{
    BindingEnd as CoreEnd, ElbowArrowRequest, LegacyBinding, LegacyBindingRequest, RestoreEnv,
    StickyNoteLayout, StickyNoteLayoutRequest, TextDimensionsRequest,
};
use excali_scene::bounds::ElementsMap as SceneMap;
use serde_json::{Map, Value};

use crate::binding::{
    calculate_fixed_point_for_non_elbow_arrow_binding, normalize_fixed_point,
    project_fixed_point_onto_diagonal, BindingEnd,
};
use crate::collision::is_point_in_element;
use crate::elbow_arrow::{update_elbow_arrow_points, ElbowArrowUpdates, ElementsMap};
use crate::js_value::num;
use crate::linear_element_editor::get_point_at_index_global_coordinates;

/// `inner` with the elbow arrow router and the legacy binding migration.
#[derive(Debug, Clone, Default)]
pub struct RoutingEnv<E> {
    pub inner: E,
}

impl<E: RestoreEnv> RoutingEnv<E> {
    pub fn new(inner: E) -> RoutingEnv<E> {
        RoutingEnv { inner }
    }
}

fn point(value: &Value) -> Option<[f64; 2]> {
    let items = value.as_array()?;
    Some([items.first()?.as_f64()?, items.get(1)?.as_f64()?])
}

impl<E: RestoreEnv> RestoreEnv for RoutingEnv<E> {
    fn now(&mut self) -> f64 {
        self.inner.now()
    }

    fn random_id(&mut self) -> String {
        self.inner.random_id()
    }

    fn random_integer(&mut self) -> f64 {
        self.inner.random_integer()
    }

    /// [`migrate_legacy_binding`]: upstream computes the migration from
    /// element geometry, so the wrapped environment is not asked.
    fn migrate_legacy_binding(
        &mut self,
        request: LegacyBindingRequest<'_>,
    ) -> Option<LegacyBinding> {
        migrate_legacy_binding(&request)
    }

    fn refresh_text_dimensions(
        &mut self,
        request: TextDimensionsRequest<'_>,
    ) -> Option<Map<String, Value>> {
        self.inner.refresh_text_dimensions(request)
    }

    fn sticky_note_layout(
        &mut self,
        request: StickyNoteLayoutRequest<'_>,
    ) -> Option<StickyNoteLayout> {
        self.inner.sticky_note_layout(request)
    }

    /// `updateElbowArrowPoints(arrow, restoredElementsMap, {points})`: the
    /// arrow and the restored elements read into the element model (an
    /// element it cannot read still counts towards the map's size but is
    /// no binding target), routed by [`update_elbow_arrow_points`].
    ///
    /// `None`, which keeps the arrow as restored, when the arrow or its
    /// points cannot be read, or where upstream's router would throw (and
    /// `restoreElements` with it).
    fn update_elbow_arrow_points(
        &mut self,
        request: ElbowArrowRequest<'_>,
    ) -> Option<Map<String, Value>> {
        let arrow = Element::from_map(request.arrow.clone()).ok()?;
        let points = request
            .points
            .iter()
            .map(point)
            .collect::<Option<Vec<[f64; 2]>>>()?;
        let elements: Vec<Element> = request
            .elements
            .iter()
            .filter_map(|e| Element::from_map(e.clone()).ok())
            .collect();
        let mut ids: Vec<&Value> = request
            .elements
            .iter()
            .filter_map(|e| e.get("id"))
            .collect();
        ids.sort_by_key(|id| id.to_string());
        ids.dedup();
        let map = ElementsMap::new(&elements).with_size(ids.len());
        let update = update_elbow_arrow_points(
            &arrow,
            &map,
            &ElbowArrowUpdates {
                points: Some(points),
                ..ElbowArrowUpdates::default()
            },
        )
        .ok()?;
        Some(update.to_map())
    }
}

/// `DEFAULT_ZOOM.value` (`packages/common/src/constants.ts:366-368`), the
/// zoom the migration projects at.
const DEFAULT_ZOOM: f64 = 1.0;

/// The legacy arrow binding migration of `repairBinding`
/// (`packages/excalidraw/data/restore.ts:347-418`): the binding end's
/// global position (`LinearElementEditor.getPointAtIndexGlobalCoordinates`
/// on the arrow with its restored points) gives `mode` (`inside` when
/// [`is_point_in_element`] of the target, else `orbit`); the focus point is
/// that position for `inside` and otherwise its projection
/// ([`project_fixed_point_onto_diagonal`] at `DEFAULT_ZOOM`, with midpoint
/// snapping), or the position where there is none; the `fixedPoint` is
/// [`calculate_fixed_point_for_non_elbow_arrow_binding`] of the focus
/// point.
///
/// The projection sees the arrow as upstream's `safeElement` has it: each
/// binding with an `elementId` given the computed `mode` and its
/// `fixedPoint` normalised (so the other end of a two-point arrow projects
/// from its fixed point when its target is in the map), the others `null`.
///
/// The arrow, the target and the map's elements are the objects as read,
/// read into the element model with [`Element::from_restored`]; an id that
/// is not a string (`7`) is a key of its own, as in a JS `Map`. `None`,
/// which drops the binding, where upstream throws: an orbit end on an
/// arrow of fewer than two points (`projectFixedPointOntoDiagonal`'s
/// invariant), and where the model cannot read the arrow or the target as
/// an element at all.
pub fn migrate_legacy_binding(request: &LegacyBindingRequest<'_>) -> Option<LegacyBinding> {
    let scene: Vec<Element> = reachable(request).iter().filter_map(read_keyed).collect();
    let map = SceneMap::new(&scene);
    let target = read_keyed(request.bound_element)?;
    let end = match request.end {
        CoreEnd::Start => BindingEnd::Start,
        CoreEnd::End => BindingEnd::End,
    };

    let unbound = read_arrow(request.arrow, None)?;
    let points = unbound.kind.points().map_or(0, <[_]>::len);
    let index = match end {
        BindingEnd::Start => 0,
        BindingEnd::End => points as isize - 1,
    };
    let p = get_point_at_index_global_coordinates(&unbound, index, &map);
    let mode = if is_point_in_element(p, &target, &map) {
        BindMode::Inside
    } else {
        BindMode::Orbit
    };
    let safe = read_arrow(request.arrow, Some(mode))?;
    let focus = if mode == BindMode::Inside {
        p
    } else {
        if points < 2 {
            // invariant(arrow.points.length >= 2, ...) throws
            return None;
        }
        project_fixed_point_onto_diagonal(&safe, p, &target, end, &map, DEFAULT_ZOOM, true)
            .unwrap_or(p)
    };
    let fixed_point =
        calculate_fixed_point_for_non_elbow_arrow_binding(&safe, &target, end, &map, Some(focus));
    Some(LegacyBinding {
        mode: Value::from(if mode == BindMode::Inside {
            "inside"
        } else {
            "orbit"
        }),
        fixed_point: Value::Array(fixed_point.iter().map(|&x| num(x)).collect()),
    })
}

/// The elements of the request's map the geometry can look up: those an id
/// held by the arrow or the target names (a binding's `elementId`,
/// `containerId`, `frameId`, a `boundElements` entry's `id`), and those
/// the elements found name in turn. The geometry looks elements up only by
/// such ids, so the map holds all it can reach without reading every
/// element of the scene for each binding end.
fn reachable(request: &LegacyBindingRequest<'_>) -> Vec<Map<String, Value>> {
    fn references(element: &Map<String, Value>) -> Vec<Value> {
        let mut ids = Vec::new();
        for key in ["startBinding", "endBinding"] {
            if let Some(id) = element.get(key).and_then(|b| b.get("elementId")) {
                ids.push(id.clone());
            }
        }
        for key in ["containerId", "frameId"] {
            if let Some(id) = element.get(key) {
                ids.push(id.clone());
            }
        }
        if let Some(Value::Array(bound)) = element.get("boundElements") {
            ids.extend(bound.iter().filter_map(|b| b.get("id")).cloned());
        }
        ids
    }
    let mut seen: Vec<Value> = Vec::new();
    let mut found = Vec::new();
    let mut queue = references(request.arrow);
    queue.extend(references(request.bound_element));
    if let Some(id) = request.bound_element.get("id") {
        queue.push(id.clone());
    }
    while let Some(id) = queue.pop() {
        if seen.contains(&id) {
            continue;
        }
        if let Some(element) = request.elements.get(&id) {
            queue.extend(references(&element));
            found.push(element);
        }
        seen.push(id);
    }
    found
}

/// The string a `Map` key is here: a string id as it is, any other
/// primitive (SameValueZero: `-0` is `0`) in a form no file writes. `None`
/// for an object or array, which no id read from another element finds.
fn map_key(id: Option<&Value>) -> Option<String> {
    const TAG: char = '\u{0}';
    Some(match id {
        Some(Value::String(s)) => s.clone(),
        None => format!("{TAG}undefined"),
        Some(Value::Null) => format!("{TAG}null"),
        Some(Value::Bool(b)) => format!("{TAG}{b}"),
        Some(Value::Number(n)) => {
            let x = n.as_f64()?;
            format!("{TAG}{}", number_to_string(if x == 0.0 { 0.0 } else { x }))
        }
        Some(Value::Array(_) | Value::Object(_)) => return None,
    })
}

/// An element as read, with its `id` as its map key.
fn read_keyed(element: &Map<String, Value>) -> Option<Element> {
    let mut element = element.clone();
    let key = map_key(element.get("id"))?;
    element.insert("id".to_owned(), Value::String(key));
    Element::from_restored(element).ok()
}

/// JS truthiness.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

/// `normalizeFixedPoint(fixedPoint)` of a value as read: `[0.5001, 0.5001]`
/// unless it is two finite numbers (`isFixedPoint`, `binding.ts:2739-2747`).
fn fixed_point_of(value: Option<&Value>) -> [f64; 2] {
    let pair = value
        .and_then(Value::as_array)
        .and_then(|items| match items.as_slice() {
            [x, y] => Some([x.as_f64()?, y.as_f64()?]),
            _ => None,
        });
    normalize_fixed_point(pair.unwrap_or([f64::NAN, f64::NAN]))
}

/// The arrow as the migration reads it, keyed like the map. With `mode`,
/// its bindings are `safeElement`'s (`restore.ts:381-396`): a binding
/// whose `elementId` is truthy gets `mode` and its normalised
/// `fixedPoint`, any other is `null`. Without, both are `null` (the end's
/// position does not depend on them).
fn read_arrow(arrow: &Map<String, Value>, mode: Option<BindMode>) -> Option<Element> {
    let mut arrow = arrow.clone();
    for key in ["startBinding", "endBinding"] {
        let safe = mode.and_then(|mode| {
            let binding = arrow.get(key)?.as_object()?;
            let element_id = binding.get("elementId");
            if !truthy(element_id) {
                return None;
            }
            let binding = FixedPointBinding {
                element_id: map_key(element_id)?,
                fixed_point: fixed_point_of(binding.get("fixedPoint")),
                mode,
            };
            serde_json::to_value(binding).ok()
        });
        arrow.insert(key.to_owned(), safe.unwrap_or(Value::Null));
    }
    read_keyed(&arrow)
}
