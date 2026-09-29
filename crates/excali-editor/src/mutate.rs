//! Element updates that bump the version: `mutateElement`, `newElementWith`
//! and `bumpVersion` (`packages/element/src/mutateElement.ts:40-196`).
//!
//! Every change to an element goes through one of these, and each change
//! sets `version` to `version + 1` (or the `version` the update names),
//! `versionNonce` to a fresh random integer (or the one the update names)
//! and `updated` to the current time. Collaborators reconcile on the pair
//! (`reconcile.ts:23-40`), and the store notices changes by the version
//! (`store.ts`, `detectChangedElements`).
//!
//! An update is a JSON object of element properties, as upstream's
//! `ElementUpdate` is a partial element: each key is applied to the
//! element's JSON form ([`Element::to_map`]), and the result is read back
//! into the model. A key the element did not have is appended, as a JS
//! property assignment appends it. Updating `id`, `updated` or `created`
//! is not an element update upstream (the type leaves them out).
//!
//! `mutateElement` also clears the element's cached shape
//! (`ShapeCache.delete`); the port's renderer builds shapes from the
//! element each time, so there is no cache to clear.

use std::fmt;

use excali_core::element::{Element, ElementKind, FixedPointBinding, FixedSegment};
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use serde_json::{Map, Value};

use crate::elbow_arrow::{
    update_elbow_arrow_points, ElbowArrowError, ElbowArrowUpdates, ElementsMap,
};
use crate::js_value::{deep_equal, is_object, num};

/// `ElementUpdate<TElement>`: the properties to set, by their JSON names.
pub type ElementUpdate = Map<String, Value>;

/// Why an update could not be applied.
#[derive(Debug, Clone, PartialEq)]
pub enum MutateError {
    /// The updated element is not a valid element of its type (a value of
    /// the wrong type, say); upstream would store it as given.
    Element(String),
    /// Routing an elbow arrow failed where upstream's router throws.
    ElbowArrow(ElbowArrowError),
}

impl fmt::Display for MutateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MutateError::Element(message) => write!(f, "invalid element update: {message}"),
            MutateError::ElbowArrow(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for MutateError {}

/// `getSizeFromPoints(points)` (`packages/common/src/points.ts`): the
/// width and height of the points' bounding box.
pub fn get_size_from_points(points: &[[f64; 2]]) -> (f64, f64) {
    let extent = |axis: usize| {
        let max = points
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        let min = points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        max - min
    };
    (extent(0), extent(1))
}

fn read_element(map: Map<String, Value>) -> Result<Element, MutateError> {
    Element::from_map(map).map_err(|e| MutateError::Element(e.to_string()))
}

fn points_of(value: &Value) -> Option<Vec<[f64; 2]>> {
    value
        .as_array()?
        .iter()
        .map(|p| {
            let p = p.as_array()?;
            Some([p.first()?.as_f64()?, p.get(1)?.as_f64()?])
        })
        .collect()
}

/// `a ?? b` for a number the update may name.
fn number_or(updates: &ElementUpdate, key: &str, fallback: impl FnOnce() -> f64) -> f64 {
    match updates.get(key) {
        Some(Value::Null) | None => fallback(),
        Some(v) => v.as_f64().unwrap_or_else(fallback),
    }
}

/// `version`, `versionNonce` and `updated` for a changed element
/// (`mutateElement.ts:142-144`).
fn stamp_version(
    map: &mut Map<String, Value>,
    current_version: f64,
    updates: &ElementUpdate,
    stamp: &mut dyn ChangeStamp,
) {
    let version = number_or(updates, "version", || current_version + 1.0);
    let nonce = number_or(updates, "versionNonce", || stamp.version_nonce());
    map.insert("version".into(), num(version));
    map.insert("versionNonce".into(), num(nonce));
    map.insert("updated".into(), num(stamp.updated()));
}

/// The update an elbow arrow gets (`mutateElement.ts:58-78`): with no keys
/// (normalisation), new points or new fixed segments, the router's points,
/// size and position, and `angle` 0.
fn elbow_arrow_update(
    element: &Element,
    elements: &SceneElementsMap,
    updates: &ElementUpdate,
) -> Result<ElementUpdate, MutateError> {
    let parse = |key: &str| -> Result<Option<Value>, MutateError> { Ok(updates.get(key).cloned()) };
    let points = match parse("points")? {
        Some(v) => Some(
            points_of(&v)
                .ok_or_else(|| MutateError::Element("points must be [x, y] pairs".into()))?,
        ),
        None => None,
    };
    let fixed_segments = match parse("fixedSegments")? {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(
            serde_json::from_value::<Vec<FixedSegment>>(v)
                .map_err(|e| MutateError::Element(e.to_string()))?,
        )),
    };
    let binding = |key: &str| -> Result<Option<Option<FixedPointBinding>>, MutateError> {
        match updates.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(v) => Ok(Some(Some(
                serde_json::from_value(v.clone())
                    .map_err(|e| MutateError::Element(e.to_string()))?,
            ))),
        }
    };
    let routed_keys = ["points", "fixedSegments", "startBinding", "endBinding"];
    let request = ElbowArrowUpdates {
        points,
        fixed_segments,
        start_binding: binding("startBinding")?,
        end_binding: binding("endBinding")?,
        other_keys: updates.keys().any(|k| !routed_keys.contains(&k.as_str())),
    };
    // {...element, x: updates.x || element.x, y: updates.y || element.y}
    let mut arrow = element.clone();
    let or = |key: &str, current: f64| match updates.get(key).and_then(Value::as_f64) {
        Some(v) if v != 0.0 && !v.is_nan() => v,
        _ => current,
    };
    arrow.base.x = or("x", element.base.x);
    arrow.base.y = or("y", element.base.y);
    let map = ElementsMap::new(elements.values());
    let routed =
        update_elbow_arrow_points(&arrow, &map, &request).map_err(MutateError::ElbowArrow)?;
    let mut next = updates.clone();
    next.insert("angle".into(), num(0.0));
    for (key, value) in routed.to_map() {
        next.insert(key, value);
    }
    Ok(next)
}

/// `mutateElement(element, elementsMap, updates)`
/// (`mutateElement.ts:40-147`): apply `updates` to `element` in place.
///
/// - An elbow arrow updated with no keys, with `points` or with
///   `fixedSegments` is re-routed first ([`update_elbow_arrow_points`]
///   over `elements`), which also sets `angle` to 0.
/// - Otherwise new `points` bring their `width` and `height`
///   ([`get_size_from_points`]) unless the update names them.
/// - A primitive value equal to the current one is skipped; an object or
///   array value is always applied, except `groupIds` and `scale` equal to
///   the current ones, and `points` of the same length whose pairs after
///   the first are equal (upstream's `while (--index)` never compares the
///   first pair).
///
/// Returns whether anything changed; only then are `version`,
/// `versionNonce` and `updated` bumped.
pub fn mutate_element(
    element: &mut Element,
    elements: &SceneElementsMap,
    updates: ElementUpdate,
    stamp: &mut dyn ChangeStamp,
) -> Result<bool, MutateError> {
    let is_elbow = matches!(&element.kind, ElementKind::Arrow(a) if a.elbowed);
    let updates = if is_elbow
        && (updates.is_empty()
            || updates.contains_key("points")
            || updates.contains_key("fixedSegments"))
    {
        elbow_arrow_update(element, elements, &updates)?
    } else if let Some(points) = updates.get("points").and_then(points_of) {
        // {...getSizeFromPoints(points), ...updates}
        let (width, height) = get_size_from_points(&points);
        let mut sized = Map::new();
        sized.insert("width".into(), num(width));
        sized.insert("height".into(), num(height));
        for (key, value) in updates {
            sized.insert(key, value);
        }
        sized
    } else {
        updates
    };

    let mut map = element.to_map();
    let mut did_change = false;
    for (key, value) in &updates {
        let current = map.get(key);
        match key.as_str() {
            "groupIds" | "scale" if current.is_some_and(|c| deep_equal(c, value)) => continue,
            "points" => {
                if let (Some(Value::Array(prev)), Value::Array(next)) = (current, value) {
                    if prev.len() == next.len()
                        && (1..prev.len()).all(|i| deep_equal(&prev[i], &next[i]))
                    {
                        continue;
                    }
                }
            }
            _ => {
                if !is_object(value) && current.is_some_and(|c| deep_equal(c, value)) {
                    continue;
                }
            }
        }
        map.insert(key.clone(), value.clone());
        did_change = true;
    }
    if !did_change {
        return Ok(false);
    }
    stamp_version(&mut map, element.base.version, &updates, stamp);
    *element = read_element(map)?;
    Ok(true)
}

/// `newElementWith(element, updates, force)` (`mutateElement.ts:149-181`):
/// a copy of `element` with `updates` applied and the version bumped, or
/// the element as it is when no value differs (an object or array value
/// always differs) and `force` is false.
pub fn new_element_with(
    element: &Element,
    updates: ElementUpdate,
    force: bool,
    stamp: &mut dyn ChangeStamp,
) -> Result<Element, MutateError> {
    let map = element.to_map();
    let did_change = updates.iter().any(|(key, value)| {
        is_object(value) || !map.get(key).is_some_and(|c| deep_equal(c, value))
    });
    if !did_change && !force {
        return Ok(element.clone());
    }
    let mut next = map;
    for (key, value) in &updates {
        next.insert(key.clone(), value.clone());
    }
    stamp_version(&mut next, element.base.version, &updates, stamp);
    read_element(next)
}

/// `bumpVersion(element, version)` (`mutateElement.ts:188-196`):
/// `version` becomes `(version ?? element.version) + 1`, with a fresh
/// `versionNonce` and `updated` now.
pub fn bump_version(element: &mut Element, version: Option<f64>, stamp: &mut dyn ChangeStamp) {
    element.base.version = version.unwrap_or(element.base.version) + 1.0;
    element.base.version_nonce = stamp.version_nonce();
    element.base.updated = stamp.updated();
}

/// An element with properties of a JSON partial applied and version,
/// nonce and timestamp taken from the partial or drawn: the spread
/// `{...element, ...updates, version, versionNonce, updated}` of
/// `newElementWith(element, updates, true)`, where a `None` value is
/// `undefined` (the key is dropped, as `JSON.stringify` drops it).
pub(crate) fn element_with_partial(
    mut map: Map<String, Value>,
    current_version: f64,
    updates: &indexmap::IndexMap<String, Option<Value>>,
    stamp: &mut dyn ChangeStamp,
) -> Result<Element, MutateError> {
    let mut named = ElementUpdate::new();
    for (key, value) in updates {
        match value {
            Some(v) => {
                map.insert(key.clone(), v.clone());
                named.insert(key.clone(), v.clone());
            }
            None => {
                map.shift_remove(key);
            }
        }
    }
    stamp_version(&mut map, current_version, &named, stamp);
    read_element(map)
}
