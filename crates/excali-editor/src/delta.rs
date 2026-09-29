//! Deltas between two states of the elements or of the observed app state
//! (`packages/element/src/delta.ts` at the pinned commit).
//!
//! A [`Delta`] holds two partials of the same keys: `deleted`, the values
//! before, and `inserted`, the values after. Inverting a delta swaps them;
//! applying it writes `inserted`. [`ElementsDelta`] keeps one delta per
//! changed element, filed as added (was deleted or missing, now not),
//! removed (the reverse) or updated; [`AppStateDelta`] keeps one for the
//! observed app state.
//!
//! Applying an elements delta (`applyTo`) also repairs bindings, so that
//! no binding points from a live element into a deleted one and both
//! sides of a binding agree (`resolveConflicts`), reorders elements by
//! fractional index when an index changed, runs the layout of
//! [`redraw_elements`], and reports whether the result looks
//! any different, which history uses to skip entries with no visible
//! change.
//!
//! Partials are JSON objects keyed by property name; a `None` value is
//! upstream's `undefined` (a key the object did not have), which applying
//! removes. Values are compared by content (see [`crate::js_value`]).
//!
//! Upstream checks some invariants in development and test builds only
//! (`isDevEnv() || isTestEnv()`): the shape of each element delta
//! (`ElementsDelta.validate`) is checked here with `debug_assert!`. The
//! other failures return a [`DeltaError`] only when
//! [`HistoryEnv::dev_checks`] is on (debug builds by default); otherwise
//! they are handled as upstream's production build does: a delta that
//! cannot be applied leaves the elements as they were and reports a
//! visible change (`delta.ts:1431-1443`), a layout error is ignored with
//! the elements laid out so far (`delta.ts:2034-2057`), the check that
//! layout touched only elements the delta reaches is skipped
//! (`delta.ts:1500-1524`), and a failed reorder keeps the elements
//! unordered and reports a visible change (`delta.ts:1543-1552`).

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt;

use excali_core::app_state::AppState;
use excali_core::element::{Element, ElementKind};
use excali_core::fractional_index::{
    order_by_fractional_index, sync_moved_indices, SceneElementsMap,
};
use excali_core::order_key::OrderKeyError;
use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use crate::js_value::{compare_utf16, js_key_order, num, same_value, truthy};
use crate::mutate::{
    element_with_partial, mutate_element, new_element_with, ElementUpdate, MutateError,
};
use crate::store::{
    DynStamp, HistoryEnv, ObservedAppState, SnapshotElements, ELEMENTS_KEYS, STANDALONE_KEYS,
};
use excali_core::fractional_index::ChangeStamp;

/// A partial object: property name to value, `None` for `undefined`.
pub type Partial = IndexMap<String, Option<Value>>;

/// Which half of a delta a modifier applies to (`modifierOptions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Deleted,
    Inserted,
}

/// Why a delta could not be applied.
#[derive(Debug, Clone, PartialEq)]
pub enum DeltaError {
    /// An element with the delta applied is not a valid element.
    Element(String, MutateError),
    /// Syncing fractional indices after a reorder failed.
    Indices(OrderKeyError),
    /// The layout ([`redraw_elements`]) failed.
    Redraw(String),
    /// The layout changed an element the delta does not reach, so history
    /// cannot tell whether the change is visible (`delta.ts:1513-1524`).
    UntrackedRedraw(String),
}

impl fmt::Display for DeltaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeltaError::Element(id, e) => {
                write!(f, "Couldn't apply delta to element \"{id}\": {e}")
            }
            DeltaError::Indices(e) => write!(f, "{e}"),
            DeltaError::Redraw(e) => write!(f, "Couldn't redraw elements: {e}"),
            DeltaError::UntrackedRedraw(id) => {
                write!(f, "Redrawn element \"{id}\" is missing from idsToCheck")
            }
        }
    }
}

impl std::error::Error for DeltaError {}

/// The keys of a partial or of a JSON object, and their values.
trait Props {
    fn prop_keys(&self) -> Vec<&String>;
    fn prop(&self, key: &str) -> Option<&Value>;
    fn has(&self, key: &str) -> bool;
}

impl Props for Map<String, Value> {
    fn prop_keys(&self) -> Vec<&String> {
        self.keys().collect()
    }
    fn prop(&self, key: &str) -> Option<&Value> {
        self.get(key)
    }
    fn has(&self, key: &str) -> bool {
        self.contains_key(key)
    }
}

impl Props for Partial {
    fn prop_keys(&self) -> Vec<&String> {
        self.keys().collect()
    }
    fn prop(&self, key: &str) -> Option<&Value> {
        self.get(key).and_then(Option::as_ref)
    }
    fn has(&self, key: &str) -> bool {
        self.contains_key(key)
    }
}

#[derive(Clone, Copy)]
enum Join {
    Left,
    Right,
    Inner,
    Full,
}

/// `distinctKeysIterator(join, object1, object2)`: the keys (by the join)
/// whose values differ.
fn distinct_keys(join: Join, a: &impl Props, b: &impl Props) -> Vec<String> {
    let keys: Vec<String> = match join {
        Join::Left => a.prop_keys().into_iter().cloned().collect(),
        Join::Right => b.prop_keys().into_iter().cloned().collect(),
        Join::Inner => a
            .prop_keys()
            .into_iter()
            .filter(|k| b.has(k))
            .cloned()
            .collect(),
        Join::Full => {
            let mut keys: Vec<String> = a.prop_keys().into_iter().cloned().collect();
            for key in b.prop_keys() {
                if !a.has(key) {
                    keys.push(key.clone());
                }
            }
            keys
        }
    };
    keys.into_iter()
        .filter(|k| !same_value(a.prop(k), b.prop(k)))
        .collect()
}

fn sorted(mut keys: Vec<String>) -> Vec<String> {
    keys.sort_by(|a, b| compare_utf16(a, b));
    keys
}

/// `Delta<T>` (`delta.ts:77-497`).
#[derive(Debug, Clone, Default)]
pub struct Delta {
    pub deleted: Partial,
    pub inserted: Partial,
}

fn partials_equal(a: &Partial, b: &Partial) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|w| same_value(v.as_ref(), w.as_ref())))
}

impl PartialEq for Delta {
    fn eq(&self, other: &Delta) -> bool {
        partials_equal(&self.deleted, &other.deleted)
            && partials_equal(&self.inserted, &other.inserted)
    }
}

/// A partial from a JSON object.
pub fn partial_from_map(map: &Map<String, Value>) -> Partial {
    map.iter()
        .map(|(k, v)| (k.clone(), Some(v.clone())))
        .collect()
}

fn partial_to_value(partial: &Partial) -> Value {
    Value::Object(
        partial
            .iter()
            .filter_map(|(k, v)| v.clone().map(|v| (k.clone(), v)))
            .collect(),
    )
}

fn partial_from_value(value: &Value) -> Option<Partial> {
    Some(partial_from_map(value.as_object()?))
}

/// `arrayToObject(array, groupBy)`: each item under its key; a later item
/// with the same key replaces an earlier one in its place.
fn array_to_object(items: &[Value], key: fn(&Value) -> String) -> IndexMap<String, Value> {
    let mut object = IndexMap::new();
    for item in items {
        object.insert(key(item), item.clone());
    }
    object
}

/// The `id` of a `{id, type}` binding.
pub(crate) fn binding_id(value: &Value) -> String {
    match value.get("id") {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => "undefined".into(),
    }
}

fn as_array(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

impl Delta {
    /// `Delta.create(deleted, inserted)`.
    pub fn new(deleted: Partial, inserted: Partial) -> Delta {
        Delta { deleted, inserted }
    }

    /// `Delta.create(deleted, inserted, modifier, modifierOptions)`: the
    /// modifier applied to both halves, or only to the given one.
    fn create_with(
        deleted: &Partial,
        inserted: &Partial,
        modifier: &dyn Fn(&Partial, Side) -> Partial,
        options: Option<Side>,
    ) -> Delta {
        let deleted = if options != Some(Side::Inserted) {
            modifier(deleted, Side::Deleted)
        } else {
            deleted.clone()
        };
        let inserted = if options != Some(Side::Deleted) {
            modifier(inserted, Side::Inserted)
        } else {
            inserted.clone()
        };
        Delta { deleted, inserted }
    }

    /// `Delta.calculate(prevObject, nextObject, modifier, postProcess)`:
    /// every key whose value differs, sorted, with the previous values as
    /// `deleted` and the next ones as `inserted`.
    fn calculate(
        prev: &Map<String, Value>,
        next: &Map<String, Value>,
        modifier: fn(Partial) -> Partial,
        post_process: fn(&mut Partial, &mut Partial),
    ) -> Delta {
        let mut deleted = Partial::new();
        let mut inserted = Partial::new();
        for key in Delta::get_differences(prev, next) {
            deleted.insert(key.clone(), prev.get(&key).cloned());
            inserted.insert(key.clone(), next.get(&key).cloned());
        }
        post_process(&mut deleted, &mut inserted);
        Delta {
            deleted: modifier(deleted),
            inserted: modifier(inserted),
        }
    }

    /// `Delta.empty()`.
    pub fn empty() -> Delta {
        Delta::default()
    }

    /// `Delta.isEmpty(delta)`: both halves have no keys.
    pub fn is_empty(&self) -> bool {
        self.deleted.is_empty() && self.inserted.is_empty()
    }

    /// `Delta.merge(delta1, delta2, delta3)`: the halves spread in order.
    pub fn merge(d1: &Delta, d2: &Delta, d3: Option<&Delta>) -> Delta {
        let spread = |parts: [Option<&Partial>; 3]| {
            let mut merged = Partial::new();
            for part in parts.into_iter().flatten() {
                for (k, v) in part {
                    merged.insert(k.clone(), v.clone());
                }
            }
            merged
        };
        Delta {
            deleted: spread([Some(&d1.deleted), Some(&d2.deleted), d3.map(|d| &d.deleted)]),
            inserted: spread([
                Some(&d1.inserted),
                Some(&d2.inserted),
                d3.map(|d| &d.inserted),
            ]),
        }
    }

    /// `Delta.mergeObjects(prev, added, removed)`: `prev` without the keys
    /// of `removed`, then `added` spread over it.
    pub fn merge_objects(
        prev: &Map<String, Value>,
        added: &Map<String, Value>,
        removed: &Map<String, Value>,
    ) -> Map<String, Value> {
        let mut entries: IndexMap<String, Value> = prev
            .iter()
            .filter(|(k, _)| !removed.contains_key(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (k, v) in added {
            entries.insert(k.clone(), v.clone());
        }
        let order = js_key_order(entries.keys());
        order
            .into_iter()
            .filter_map(|k| entries.get(&k).cloned().map(|v| (k, v)))
            .collect()
    }

    /// `Delta.mergeArrays(prev, added, removed, predicate)`: the arrays as
    /// objects keyed by `predicate`, merged with [`Delta::merge_objects`],
    /// and the values in the object's order.
    pub fn merge_arrays(
        prev: &[Value],
        added: &[Value],
        removed: &[Value],
        predicate: fn(&Value) -> String,
    ) -> Vec<Value> {
        let to_map = |items: &[Value]| -> Map<String, Value> {
            array_to_object(items, predicate).into_iter().collect()
        };
        Delta::merge_objects(&to_map(prev), &to_map(added), &to_map(removed))
            .into_iter()
            .map(|(_, v)| v)
            .collect()
    }

    /// `Delta.diffObjects(deleted, inserted, property, setValue)`: reduce
    /// an object-valued property of both halves to the keys that differ.
    fn diff_objects(
        deleted: &mut Partial,
        inserted: &mut Partial,
        property: &str,
        set_value: fn(Option<&Value>) -> Value,
    ) {
        let d = deleted.prop(property).cloned();
        let i = inserted.prop(property).cloned();
        if !truthy(d.as_ref()) && !truthy(i.as_ref()) {
            return;
        }
        let is_object = |v: &Option<Value>| matches!(v, Some(Value::Object(_) | Value::Array(_)));
        if is_object(&d) || is_object(&i) {
            let as_object = |v: &Option<Value>| match v {
                Some(Value::Object(m)) => m.clone(),
                _ => Map::new(),
            };
            let deleted_object = as_object(&d);
            let inserted_object = as_object(&i);
            let deleted_differences: Map<String, Value> =
                Delta::get_left_differences(&deleted_object, &inserted_object)
                    .into_iter()
                    .map(|k| {
                        let v = set_value(deleted_object.get(&k));
                        (k, v)
                    })
                    .collect();
            let inserted_differences: Map<String, Value> =
                Delta::get_right_differences(&deleted_object, &inserted_object)
                    .into_iter()
                    .map(|k| {
                        let v = set_value(inserted_object.get(&k));
                        (k, v)
                    })
                    .collect();
            if !deleted_differences.is_empty() || !inserted_differences.is_empty() {
                deleted.insert(property.into(), Some(Value::Object(deleted_differences)));
                inserted.insert(property.into(), Some(Value::Object(inserted_differences)));
            } else {
                deleted.shift_remove(property);
                inserted.shift_remove(property);
            }
        } else if same_value(d.as_ref(), i.as_ref()) {
            deleted.shift_remove(property);
            inserted.shift_remove(property);
        }
    }

    /// `Delta.diffArrays(deleted, inserted, property, groupBy)`: reduce an
    /// array-valued property of both halves to the items (by key) that
    /// differ.
    fn diff_arrays(
        deleted: &mut Partial,
        inserted: &mut Partial,
        property: &str,
        group_by: fn(&Value) -> String,
    ) {
        let d = deleted.prop(property).cloned();
        let i = inserted.prop(property).cloned();
        if !truthy(d.as_ref()) && !truthy(i.as_ref()) {
            return;
        }
        let is_array = |v: &Option<Value>| matches!(v, Some(Value::Array(_)));
        if !is_array(&d) && !is_array(&i) {
            return;
        }
        let deleted_array = as_array(d.as_ref()).to_vec();
        let inserted_array = as_array(i.as_ref()).to_vec();
        let deleted_object: Map<String, Value> = array_to_object(&deleted_array, group_by)
            .into_iter()
            .collect();
        let inserted_object: Map<String, Value> = array_to_object(&inserted_array, group_by)
            .into_iter()
            .collect();
        let deleted_differences: HashSet<String> =
            Delta::get_left_differences(&deleted_object, &inserted_object)
                .into_iter()
                .collect();
        let inserted_differences: HashSet<String> =
            Delta::get_right_differences(&deleted_object, &inserted_object)
                .into_iter()
                .collect();
        if !deleted_differences.is_empty() || !inserted_differences.is_empty() {
            let deleted_value: Vec<Value> = deleted_array
                .into_iter()
                .filter(|x| deleted_differences.contains(&group_by(x)))
                .collect();
            let inserted_value: Vec<Value> = inserted_array
                .into_iter()
                .filter(|x| inserted_differences.contains(&group_by(x)))
                .collect();
            deleted.insert(property.into(), Some(Value::Array(deleted_value)));
            inserted.insert(property.into(), Some(Value::Array(inserted_value)));
        } else {
            deleted.shift_remove(property);
            inserted.shift_remove(property);
        }
    }

    /// `Delta.isRightDifferent(object1, object2)`: some key of `object2`
    /// has another value in `object1`.
    pub fn is_right_different(a: &Map<String, Value>, b: &Map<String, Value>) -> bool {
        !distinct_keys(Join::Right, a, b).is_empty()
    }

    /// `Delta.isInnerDifferent(object1, object2)`: some key both have has
    /// different values.
    pub fn is_inner_different(a: &Partial, b: &Partial) -> bool {
        !distinct_keys(Join::Inner, a, b).is_empty()
    }

    /// `Delta.getLeftDifferences(object1, object2)`, sorted.
    pub fn get_left_differences(a: &Map<String, Value>, b: &Map<String, Value>) -> Vec<String> {
        sorted(distinct_keys(Join::Left, a, b))
    }

    /// `Delta.getRightDifferences(object1, object2)`, sorted.
    pub fn get_right_differences(a: &Map<String, Value>, b: &Map<String, Value>) -> Vec<String> {
        sorted(distinct_keys(Join::Right, a, b))
    }

    /// `Delta.getDifferences(object1, object2)` (full join), sorted.
    pub fn get_differences(a: &Map<String, Value>, b: &Map<String, Value>) -> Vec<String> {
        sorted(distinct_keys(Join::Full, a, b))
    }

    fn to_dto(&self) -> Value {
        json!({
            "deleted": partial_to_value(&self.deleted),
            "inserted": partial_to_value(&self.inserted),
        })
    }

    fn from_dto(value: &Value) -> Option<Delta> {
        Some(Delta {
            deleted: partial_from_value(value.get("deleted")?)?,
            inserted: partial_from_value(value.get("inserted")?)?,
        })
    }
}

fn object_or_empty(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    }
}

// ---------------------------------------------------------------------------
// AppStateDelta

/// `AppStateDelta` (`delta.ts:526-1015`): the change to the observed app
/// state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppStateDelta {
    pub delta: Delta,
}

/// The selection-like keys whose values are merged as sets of ids.
const SET_KEYS: [&str; 3] = [
    "selectedElementIds",
    "selectedGroupIds",
    "lockedMultiSelections",
];

impl AppStateDelta {
    /// `AppStateDelta.create(delta)`.
    pub fn create(delta: Delta) -> AppStateDelta {
        AppStateDelta { delta }
    }

    /// `AppStateDelta.calculate(prevAppState, nextAppState)`: the keys
    /// that differ, `selectedElementIds`, `selectedGroupIds` and
    /// `lockedMultiSelections` reduced to the ids that differ, keys in
    /// sorted order.
    pub fn calculate(prev: &ObservedAppState, next: &ObservedAppState) -> AppStateDelta {
        AppStateDelta {
            delta: Delta::calculate(
                prev.as_map(),
                next.as_map(),
                AppStateDelta::order_keys,
                AppStateDelta::post_process,
            ),
        }
    }

    /// `AppStateDelta.empty()`.
    pub fn empty() -> AppStateDelta {
        AppStateDelta::default()
    }

    /// `inverse()`.
    pub fn inverse(&self) -> AppStateDelta {
        AppStateDelta {
            delta: Delta::new(self.delta.inserted.clone(), self.delta.deleted.clone()),
        }
    }

    /// `isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.delta.is_empty()
    }

    /// `squash(delta)`: `other` merged over this delta, the id sets of the
    /// selection keys merged rather than replaced.
    pub fn squash(&mut self, other: &AppStateDelta) -> &mut AppStateDelta {
        if other.is_empty() {
            return self;
        }
        let mut merged_deleted = Partial::new();
        let mut merged_inserted = Partial::new();
        for key in SET_KEYS {
            let deleted = Delta::merge_objects(
                &object_or_empty(self.delta.deleted.prop(key)),
                &object_or_empty(other.delta.deleted.prop(key)),
                &Map::new(),
            );
            let inserted = Delta::merge_objects(
                &object_or_empty(self.delta.inserted.prop(key)),
                &object_or_empty(other.delta.inserted.prop(key)),
                &Map::new(),
            );
            if !deleted.is_empty() || !inserted.is_empty() {
                merged_deleted.insert(key.into(), Some(Value::Object(deleted)));
                merged_inserted.insert(key.into(), Some(Value::Object(inserted)));
            }
        }
        self.delta = Delta::merge(
            &self.delta,
            &other.delta,
            Some(&Delta::new(merged_deleted, merged_inserted)),
        );
        self
    }

    /// `applyTo(appState, nextElements)`: the app state with `inserted`
    /// written (the selection keys as sets: `deleted` ids removed,
    /// `inserted` ids added), selections of elements and groups that are
    /// deleted in `next_elements` dropped, and whether the result is a
    /// visible change.
    pub fn apply_to(
        &self,
        app_state: &AppState,
        next_elements: &SceneElementsMap,
    ) -> (AppState, bool) {
        let deleted = &self.delta.deleted;
        let inserted = &self.delta.inserted;
        let mut next = app_state.as_map().clone();
        for (key, value) in inserted {
            if SET_KEYS.contains(&key.as_str()) || key == "selectedLinearElement" {
                continue;
            }
            match value {
                Some(v) => {
                    next.insert(key.clone(), v.clone());
                }
                None => {
                    next.shift_remove(key);
                }
            }
        }
        for key in SET_KEYS {
            let merged = Delta::merge_objects(
                &object_or_empty(app_state.get(key)),
                &object_or_empty(inserted.prop(key)),
                &object_or_empty(deleted.prop(key)),
            );
            next.insert(key.into(), Value::Object(merged));
        }
        // `typeof inserted.selectedLinearElement !== "undefined"`
        if inserted
            .get("selectedLinearElement")
            .is_some_and(Option::is_some)
        {
            let linear = inserted.prop("selectedLinearElement");
            let element_id = linear
                .filter(|v| truthy(Some(v)))
                .and_then(|v| v.get("elementId"))
                .and_then(Value::as_str);
            let value = match element_id {
                Some(id) if next_elements.contains_key(id) => json!({
                    "elementId": id,
                    "isEditing": truthy(linear.and_then(|v| v.get("isEditing"))),
                }),
                _ => Value::Null,
            };
            next.insert("selectedLinearElement".into(), value);
        }
        let mut next = AppState::from_map(next);
        let visible = AppStateDelta::filter_invisible_changes(app_state, &mut next, next_elements);
        (next, visible)
    }

    /// `filterInvisibleChanges(prevAppState, nextAppState, nextElements)`:
    /// drop from `next` the selection of elements and groups that are
    /// deleted, and answer whether a visible change is left.
    fn filter_invisible_changes(
        prev: &AppState,
        next: &mut AppState,
        elements: &SceneElementsMap,
    ) -> bool {
        let prev_observed = ObservedAppState::from_app_state(prev);
        let next_observed = ObservedAppState::from_app_state(next);
        let strip = |observed: &ObservedAppState, keys: &[&str]| -> Map<String, Value> {
            observed
                .as_map()
                .iter()
                .filter(|(k, _)| keys.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let standalone_difference = Delta::is_right_different(
            &strip(&prev_observed, &STANDALONE_KEYS),
            &strip(&next_observed, &STANDALONE_KEYS),
        );
        let prev_elements_props = strip(&prev_observed, &ELEMENTS_KEYS);
        let next_elements_props = strip(&next_observed, &ELEMENTS_KEYS);
        let changed_props =
            Delta::get_right_differences(&prev_elements_props, &next_elements_props);
        if !standalone_difference && changed_props.is_empty() {
            // no change in the app state was detected
            return false;
        }
        let mut visible = standalone_difference;
        let non_deleted_group_ids: HashSet<String> = if changed_props
            .iter()
            .any(|k| k == "editingGroupId" || k == "selectedGroupIds")
        {
            get_non_deleted_group_ids(elements)
        } else {
            HashSet::new()
        };
        let live = |id: &str| elements.get(id).is_some_and(|e| !e.base.is_deleted);
        for key in &changed_props {
            let value = next.get(key).cloned();
            match key.as_str() {
                "selectedElementIds" => {
                    let ids = object_or_empty(value.as_ref());
                    if ids.is_empty() {
                        // previously there were ids, now there are none
                        visible = true;
                        continue;
                    }
                    let mut kept = ids.clone();
                    for id in ids.keys() {
                        if live(id) {
                            visible = true;
                        } else {
                            kept.shift_remove(id);
                        }
                    }
                    next.insert(key.clone(), Value::Object(kept));
                }
                "selectedGroupIds" => {
                    let ids = object_or_empty(value.as_ref());
                    if ids.is_empty() {
                        visible = true;
                        continue;
                    }
                    let mut kept = ids.clone();
                    for id in ids.keys() {
                        if non_deleted_group_ids.contains(id) {
                            visible = true;
                        } else {
                            kept.shift_remove(id);
                        }
                    }
                    next.insert(key.clone(), Value::Object(kept));
                }
                "croppingElementId" => match value.as_ref().filter(|v| truthy(Some(v))) {
                    None => visible = true,
                    Some(id) => {
                        if id.as_str().is_some_and(live) {
                            visible = true;
                        } else {
                            next.insert(key.clone(), Value::Null);
                        }
                    }
                },
                "editingGroupId" => match value.as_ref().filter(|v| truthy(Some(v))) {
                    None => visible = true,
                    Some(id) => {
                        if id
                            .as_str()
                            .is_some_and(|id| non_deleted_group_ids.contains(id))
                        {
                            visible = true;
                        } else {
                            next.insert(key.clone(), Value::Null);
                        }
                    }
                },
                "selectedLinearElement" => match value.as_ref().filter(|v| truthy(Some(v))) {
                    None => visible = true,
                    Some(linear) => {
                        if linear
                            .get("elementId")
                            .and_then(Value::as_str)
                            .is_some_and(live)
                        {
                            visible = true;
                        } else {
                            next.insert(key.clone(), Value::Null);
                        }
                    }
                },
                "lockedMultiSelections" => {
                    let prev_units = object_or_empty(prev.get(key));
                    let next_units = object_or_empty(value.as_ref());
                    if !(prev_units.len() == next_units.len()
                        && prev_units
                            .iter()
                            .all(|(k, v)| same_value(Some(v), next_units.get(k))))
                    {
                        visible = true;
                    }
                }
                "activeLockedId" => {
                    let or_null = |v: Option<&Value>| {
                        if truthy(v) {
                            v.cloned().unwrap_or(Value::Null)
                        } else {
                            Value::Null
                        }
                    };
                    if !same_value(
                        Some(&or_null(prev.get(key))),
                        Some(&or_null(value.as_ref())),
                    ) {
                        visible = true;
                    }
                }
                _ => {}
            }
        }
        visible
    }

    /// `postProcess`: the selection keys reduced to the ids that differ.
    fn post_process(deleted: &mut Partial, inserted: &mut Partial) {
        Delta::diff_objects(deleted, inserted, "selectedElementIds", |_| {
            Value::Bool(true)
        });
        Delta::diff_objects(deleted, inserted, "selectedGroupIds", |prev| {
            prev.cloned().unwrap_or(Value::Bool(false))
        });
        Delta::diff_objects(deleted, inserted, "lockedMultiSelections", |prev| {
            prev.cloned().unwrap_or_else(|| json!({}))
        });
    }

    /// `orderAppStateKeys`: keys sorted, for a stable serialisation.
    fn order_keys(partial: Partial) -> Partial {
        let mut keys: Vec<String> = partial.keys().cloned().collect();
        keys.sort_by(|a, b| compare_utf16(a, b));
        let mut partial = partial;
        keys.into_iter()
            .filter_map(|k| partial.shift_remove(&k).map(|v| (k, v)))
            .collect()
    }

    /// The delta as `DTO<AppStateDelta>`: `{delta: {deleted, inserted}}`.
    pub fn to_dto(&self) -> Value {
        json!({ "delta": self.delta.to_dto() })
    }

    /// `AppStateDelta.restore(dto)`.
    pub fn from_dto(dto: &Value) -> Option<AppStateDelta> {
        Some(AppStateDelta {
            delta: Delta::from_dto(dto.get("delta")?)?,
        })
    }
}

/// `getNonDeletedGroupIds(elements)` (`groups.ts:358-374`).
fn get_non_deleted_group_ids(elements: &SceneElementsMap) -> HashSet<String> {
    elements
        .values()
        .filter(|e| !e.base.is_deleted)
        .flat_map(|e| e.base.group_ids.iter().cloned())
        .collect()
}

// ---------------------------------------------------------------------------
// ElementsDelta

/// `ApplyToOptions`: element properties not to apply (history leaves out
/// `version` and `versionNonce`, so every undo is a new version).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApplyToOptions {
    pub excluded_properties: HashSet<String>,
}

/// `bindingProperties` (`binding.ts:2362-2368`): the properties that bind
/// one element to another.
const BINDING_PROPERTIES: [&str; 5] = [
    "boundElements",
    "frameId",
    "containerId",
    "startBinding",
    "endBinding",
];

/// `ElementsDelta` (`delta.ts:1034-2213`): per element id, the delta of an
/// element that was added, removed or updated.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ElementsDelta {
    pub added: IndexMap<String, Delta>,
    pub removed: IndexMap<String, Delta>,
    pub updated: IndexMap<String, Delta>,
}

fn is_deleted_flag(partial: &Partial) -> Option<&Value> {
    partial.prop("isDeleted")
}

fn satisfies_addition(delta: &Delta) -> bool {
    is_deleted_flag(&delta.deleted) == Some(&Value::Bool(true))
        && !truthy(is_deleted_flag(&delta.inserted))
}

fn satisfies_removal(delta: &Delta) -> bool {
    !truthy(is_deleted_flag(&delta.deleted))
        && is_deleted_flag(&delta.inserted) == Some(&Value::Bool(true))
}

fn satisfies_update(delta: &Delta) -> bool {
    truthy(is_deleted_flag(&delta.deleted)) == truthy(is_deleted_flag(&delta.inserted))
}

fn satisfies_common_invariants(delta: &Delta) -> bool {
    let version = |p: &Partial| {
        p.prop("version")
            .and_then(Value::as_f64)
            .filter(|v| v.fract() == 0.0 && *v >= 0.0)
    };
    match (version(&delta.deleted), version(&delta.inserted)) {
        (Some(d), Some(i)) => d != i,
        _ => false,
    }
}

/// The element's type for the checks upstream makes by type.
fn is_text(element: &Element) -> bool {
    matches!(element.kind, ElementKind::Text(_))
}

fn is_arrow(element: &Element) -> bool {
    matches!(element.kind, ElementKind::Arrow(_))
}

fn text_container_id(element: &Element) -> Option<&str> {
    match &element.kind {
        ElementKind::Text(t) => t.container_id.as_deref(),
        _ => None,
    }
}

fn bound_ids(element: &Element) -> Vec<(String, &'static str)> {
    element
        .base
        .bound_elements
        .iter()
        .flatten()
        .map(|b| {
            (
                b.id.clone(),
                match b.kind {
                    excali_core::element::BoundElementType::Arrow => "arrow",
                    excali_core::element::BoundElementType::Text => "text",
                },
            )
        })
        .collect()
}

/// `stripIrrelevantProps`: `id` and `updated` are not part of a delta.
fn strip_irrelevant_props(mut partial: Partial) -> Partial {
    partial.shift_remove("id");
    partial.shift_remove("updated");
    partial
}

/// `ElementsDelta.postProcess`: `boundElements` reduced to the bindings
/// that differ; `points` dropped when equal.
fn post_process_elements(deleted: &mut Partial, inserted: &mut Partial) {
    Delta::diff_arrays(deleted, inserted, "boundElements", binding_id);
    let points = |p: &Partial| p.prop("points").cloned().unwrap_or_else(|| json!([]));
    if same_value(Some(&points(deleted)), Some(&points(inserted))) {
        deleted.shift_remove("points");
        inserted.shift_remove("points");
    }
}

/// The JavaScript text `trim()`.
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

impl ElementsDelta {
    /// `ElementsDelta.create(added, removed, updated, { shouldRedistribute })`:
    /// with `redistribute`, each delta is filed again by what it does to
    /// `isDeleted`.
    pub fn create(
        added: IndexMap<String, Delta>,
        removed: IndexMap<String, Delta>,
        updated: IndexMap<String, Delta>,
        redistribute: bool,
    ) -> ElementsDelta {
        let delta = if redistribute {
            let mut next = ElementsDelta::default();
            for (id, delta) in added.into_iter().chain(removed).chain(updated) {
                if satisfies_addition(&delta) {
                    next.added.insert(id, delta);
                } else if satisfies_removal(&delta) {
                    next.removed.insert(id, delta);
                } else {
                    next.updated.insert(id, delta);
                }
            }
            next
        } else {
            ElementsDelta {
                added,
                removed,
                updated,
            }
        };
        debug_assert!(delta.validate().is_ok(), "{:?}", delta.validate());
        delta
    }

    /// `ElementsDelta.validate`: every delta has integer, non-negative,
    /// different versions, is filed once, and does to `isDeleted` what its
    /// record says. Upstream checks this in development and test builds.
    pub fn validate(&self) -> Result<(), String> {
        type Record<'r> = (&'r str, &'r IndexMap<String, Delta>, fn(&Delta) -> bool);
        let records: [Record<'_>; 3] = [
            ("added", &self.added, satisfies_addition),
            ("removed", &self.removed, satisfies_removal),
            ("updated", &self.updated, satisfies_update),
        ];
        for (kind, record, special) in records {
            for (id, delta) in record {
                let unique = [&self.added, &self.removed, &self.updated]
                    .iter()
                    .filter(|r| r.contains_key(id))
                    .count()
                    == 1;
                if !satisfies_common_invariants(delta) || !unique || !special(delta) {
                    return Err(format!(
                        "ElementsDelta invariant broken for element \"{id}\" ({kind}): {}",
                        delta.to_dto()
                    ));
                }
            }
        }
        Ok(())
    }

    /// `ElementsDelta.empty()`.
    pub fn empty() -> ElementsDelta {
        ElementsDelta::default()
    }

    /// `isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.updated.is_empty()
    }

    /// `ElementsDelta.calculate(prevElements, nextElements)`.
    ///
    /// - An element missing from `next` is removed (or, if it was already
    ///   deleted, updated) to `{isDeleted: true, version + 1}`.
    /// - An element missing from `prev` is added (or, if it is deleted,
    ///   updated) from `{isDeleted: true, version - 1}`.
    /// - An element whose `versionNonce` changed gets the keys that differ;
    ///   a flip of `isDeleted` files it as added or removed.
    pub fn calculate<P, N>(
        prev: &IndexMap<String, P>,
        next: &IndexMap<String, N>,
        stamp: &mut dyn ChangeStamp,
    ) -> ElementsDelta
    where
        P: Borrow<Element>,
        N: Borrow<Element>,
    {
        let mut delta = ElementsDelta::default();
        for (id, prev_element) in prev {
            let prev_element: &Element = prev_element.borrow();
            if next.contains_key(id) {
                continue;
            }
            let deleted = strip_irrelevant_props(partial_from_map(&prev_element.to_map()));
            let mut inserted = Partial::new();
            inserted.insert("isDeleted".into(), Some(Value::Bool(true)));
            inserted.insert("version".into(), Some(num(prev_element.base.version + 1.0)));
            inserted.insert("versionNonce".into(), Some(num(stamp.version_nonce())));
            let d = Delta::new(deleted, strip_irrelevant_props(inserted));
            if !prev_element.base.is_deleted {
                delta.removed.insert(id.clone(), d);
            } else {
                delta.updated.insert(id.clone(), d);
            }
        }
        for (id, next_element) in next {
            let next_element: &Element = next_element.borrow();
            let Some(prev_element) = prev.get(id) else {
                let mut deleted = Partial::new();
                deleted.insert("isDeleted".into(), Some(Value::Bool(true)));
                deleted.insert("version".into(), Some(num(next_element.base.version - 1.0)));
                deleted.insert("versionNonce".into(), Some(num(stamp.version_nonce())));
                let inserted = strip_irrelevant_props(partial_from_map(&next_element.to_map()));
                let d = Delta::new(strip_irrelevant_props(deleted), inserted);
                // ignore updates which would "delete" an already deleted element
                if !next_element.base.is_deleted {
                    delta.added.insert(id.clone(), d);
                } else {
                    delta.updated.insert(id.clone(), d);
                }
                continue;
            };
            let prev_element: &Element = prev_element.borrow();
            if prev_element.base.version_nonce == next_element.base.version_nonce {
                continue;
            }
            let d = Delta::calculate(
                &prev_element.to_map(),
                &next_element.to_map(),
                strip_irrelevant_props,
                post_process_elements,
            );
            if prev_element.base.is_deleted != next_element.base.is_deleted {
                // notice that other props could have been updated as well
                if prev_element.base.is_deleted {
                    delta.added.insert(id.clone(), d);
                } else {
                    delta.removed.insert(id.clone(), d);
                }
                continue;
            }
            delta.updated.insert(id.clone(), d);
        }
        ElementsDelta::create(delta.added, delta.removed, delta.updated, false)
    }

    /// `inverse()`: every delta inverted, added and removed swapped.
    pub fn inverse(&self) -> ElementsDelta {
        let inverse = |record: &IndexMap<String, Delta>| -> IndexMap<String, Delta> {
            record
                .iter()
                .map(|(id, d)| {
                    (
                        id.clone(),
                        Delta::new(d.inserted.clone(), d.deleted.clone()),
                    )
                })
                .collect()
        };
        ElementsDelta::create(
            inverse(&self.removed),
            inverse(&self.added),
            inverse(&self.updated),
            false,
        )
    }

    /// `applyLatestChanges(prevElements, nextElements, modifierOptions)`:
    /// each partial's values replaced by the element's current ones (the
    /// `deleted` half from `prev`, the `inserted` half from `next`; only
    /// the given half with `modifier_options`), except `boundElements`,
    /// which stays a diff. Deltas left without a difference are dropped and
    /// the rest filed again by `isDeleted`.
    pub fn apply_latest_changes<P, N>(
        &self,
        prev: &IndexMap<String, P>,
        next: &IndexMap<String, N>,
        modifier_options: Option<Side>,
    ) -> ElementsDelta
    where
        P: Borrow<Element>,
        N: Borrow<Element>,
    {
        let latest = |record: &IndexMap<String, Delta>| -> IndexMap<String, Delta> {
            let mut modified = IndexMap::new();
            for (id, delta) in record {
                let prev_element = prev.get(id).map(|e| e.borrow().to_map());
                let next_element = next.get(id).map(|e| e.borrow().to_map());
                let latest = if prev_element.is_some() || next_element.is_some() {
                    let modifier = |partial: &Partial, side: Side| -> Partial {
                        let element = match side {
                            Side::Deleted => prev_element.as_ref(),
                            Side::Inserted => next_element.as_ref(),
                        };
                        // the element wasn't found -> don't update the partial
                        let Some(element) = element else {
                            return partial.clone();
                        };
                        partial
                            .iter()
                            .map(|(key, value)| {
                                let latest = if key == "boundElements" {
                                    value.clone()
                                } else {
                                    element.get(key).cloned()
                                };
                                (key.clone(), latest)
                            })
                            .collect()
                    };
                    Delta::create_with(&delta.deleted, &delta.inserted, &modifier, modifier_options)
                } else {
                    delta.clone()
                };
                // after applying latest changes the delta might not contain
                // any changes
                if Delta::is_inner_different(&latest.deleted, &latest.inserted) {
                    modified.insert(id.clone(), latest);
                }
            }
            modified
        };
        ElementsDelta::create(
            latest(&self.added),
            latest(&self.removed),
            latest(&self.updated),
            true,
        )
    }

    /// `squash(delta)`: `other`'s deltas merged into this one, each element
    /// ending up in `other`'s record for it (an update keeps the record it
    /// was in), `boundElements` merged by id rather than replaced.
    pub fn squash(&mut self, other: &ElementsDelta) -> &mut ElementsDelta {
        if other.is_empty() {
            return self;
        }
        fn merge_bound_elements(prev: &Delta, next: &Delta) -> Option<Delta> {
            let merge = |a: &Partial, b: &Partial| {
                Delta::merge_arrays(
                    as_array(a.prop("boundElements")),
                    as_array(b.prop("boundElements")),
                    &[],
                    binding_id,
                )
            };
            let deleted = merge(&prev.deleted, &next.deleted);
            let inserted = merge(&prev.inserted, &next.inserted);
            if deleted.is_empty() && inserted.is_empty() {
                return None;
            }
            let one = |v: Vec<Value>| -> Partial {
                [("boundElements".to_string(), Some(Value::Array(v)))]
                    .into_iter()
                    .collect()
            };
            Some(Delta::new(one(deleted), one(inserted)))
        }
        let find = |this: &ElementsDelta, id: &str| -> Option<Delta> {
            this.added
                .get(id)
                .or_else(|| this.removed.get(id))
                .or_else(|| this.updated.get(id))
                .cloned()
        };
        for (id, next_delta) in &other.added {
            match find(self, id) {
                None => {
                    self.added.insert(id.clone(), next_delta.clone());
                }
                Some(prev_delta) => {
                    let merged = merge_bound_elements(&prev_delta, next_delta);
                    self.removed.shift_remove(id);
                    self.updated.shift_remove(id);
                    self.added.insert(
                        id.clone(),
                        Delta::merge(&prev_delta, next_delta, merged.as_ref()),
                    );
                }
            }
        }
        for (id, next_delta) in &other.removed {
            match find(self, id) {
                None => {
                    self.removed.insert(id.clone(), next_delta.clone());
                }
                Some(prev_delta) => {
                    let merged = merge_bound_elements(&prev_delta, next_delta);
                    self.added.shift_remove(id);
                    self.updated.shift_remove(id);
                    self.removed.insert(
                        id.clone(),
                        Delta::merge(&prev_delta, next_delta, merged.as_ref()),
                    );
                }
            }
        }
        for (id, next_delta) in &other.updated {
            match find(self, id) {
                None => {
                    self.updated.insert(id.clone(), next_delta.clone());
                }
                Some(prev_delta) => {
                    let merged = merge_bound_elements(&prev_delta, next_delta);
                    let updated = Delta::merge(&prev_delta, next_delta, merged.as_ref());
                    if self.added.contains_key(id) {
                        self.added.insert(id.clone(), updated);
                    } else if self.removed.contains_key(id) {
                        self.removed.insert(id.clone(), updated);
                    } else {
                        self.updated.insert(id.clone(), updated);
                    }
                }
            }
        }
        debug_assert!(self.validate().is_ok(), "{:?}", self.validate());
        self
    }

    /// `applyTo(elements, snapshot, options)`: the elements with the deltas
    /// applied, and whether that is a visible change.
    ///
    /// An element missing from `elements` is taken from `snapshot` (a
    /// force-deleted element), or created from the partial. Each changed
    /// element is a new version (`version + 1` unless the partial names
    /// one). Bindings are then repaired, which may change more elements
    /// and is squashed into this delta; elements are reordered when an
    /// index changed; and [`redraw_elements`] runs.
    pub fn apply_to(
        &mut self,
        elements: &SceneElementsMap,
        snapshot: &SnapshotElements,
        options: &ApplyToOptions,
        env: &mut dyn HistoryEnv,
    ) -> Result<(SceneElementsMap, bool), DeltaError> {
        let mut applier = Applier {
            prev: elements,
            next: elements.clone(),
            touched: HashSet::new(),
            snapshot,
            options,
            contains_zindex_difference: false,
            direction: None,
            affected: SceneElementsMap::new(),
        };
        let dev_checks = env.dev_checks();
        let applied = (|| {
            let added = applier.apply_deltas(&self.added, env)?;
            let removed = applier.apply_deltas(&self.removed, env)?;
            let updated = applier.apply_deltas(&self.updated, env)?;
            applier.resolve_conflicts(self, env)?;
            Ok((added, removed, updated))
        })();
        let (added, removed, updated) = match applied {
            Ok(applied) => applied,
            Err(e) if dev_checks => return Err(e),
            // the previous elements, with a visible change so that history
            // does not skip past this entry (`delta.ts:1431-1443`)
            Err(_) => return Ok((elements.clone(), true)),
        };
        let affected = std::mem::take(&mut applier.affected);

        let mut changed = SceneElementsMap::new();
        for (id, element) in added
            .into_iter()
            .chain(removed)
            .chain(updated)
            .chain(affected)
        {
            changed.insert(id, element);
        }

        // Every element whose before and after get compared: the changed
        // ones and what layout can move for them (a text's container, the
        // bound elements), walked as the set grows.
        let mut ids: Vec<String> = changed.keys().cloned().collect();
        let mut seen: HashSet<String> = ids.iter().cloned().collect();
        let mut previous: HashMap<String, Element> = HashMap::new();
        let mut i = 0;
        while i < ids.len() {
            let id = ids[i].clone();
            let before = elements.get(&id);
            for element in [before, applier.next.get(&id)].into_iter().flatten() {
                let mut reach: Vec<String> = Vec::new();
                if let Some(container) = text_container_id(element).filter(|c| !c.is_empty()) {
                    reach.push(container.to_owned());
                }
                reach.extend(bound_ids(element).into_iter().map(|(id, _)| id));
                for other in reach {
                    if seen.insert(other.clone()) {
                        ids.push(other);
                    }
                }
            }
            if let Some(before) = before {
                previous.insert(id, before.clone());
            }
            i += 1;
        }

        let mut contains_visible_difference = false;
        let mut next = applier.next;
        if applier.contains_zindex_difference {
            match reorder_elements(&next, &changed, env) {
                Ok((reordered, moved)) => {
                    next = reordered;
                    if moved {
                        contains_visible_difference = true;
                    }
                }
                Err(e) if dev_checks => return Err(e),
                // upstream's outer catch (`delta.ts:1543-1552`)
                Err(_) => return Ok((next, true)),
            }
        }
        // the changed elements as they are now (a reorder changes indices)
        for (id, element) in changed.iter_mut() {
            if let Some(current) = next.get(id) {
                *element = current.clone();
            }
        }

        if dev_checks {
            let versions_before: HashMap<String, f64> = next
                .iter()
                .map(|(id, e)| (id.clone(), e.base.version))
                .collect();
            redraw_elements(&mut next, &mut changed, env).map_err(DeltaError::Redraw)?;
            for (id, element) in &next {
                if versions_before.get(id) != Some(&element.base.version) && !seen.contains(id) {
                    return Err(DeltaError::UntrackedRedraw(id.clone()));
                }
            }
        } else {
            // `redrawElements` logs and returns the elements laid out so far
            // (`delta.ts:2034-2057`)
            let _ = redraw_elements(&mut next, &mut changed, env);
        }

        if !contains_visible_difference {
            let before_lookup = |id: &str| previous.get(id);
            let after_lookup = |id: &str| next.get(id);
            contains_visible_difference = ids.iter().any(|id| {
                check_for_visible_difference(
                    previous.get(id),
                    next.get(id),
                    &before_lookup,
                    &after_lookup,
                )
            });
        }
        Ok((next, contains_visible_difference))
    }

    /// The delta as `DTO<ElementsDelta>`: `{added, removed, updated}`.
    pub fn to_dto(&self) -> Value {
        let record = |r: &IndexMap<String, Delta>| {
            Value::Object(r.iter().map(|(id, d)| (id.clone(), d.to_dto())).collect())
        };
        json!({
            "added": record(&self.added),
            "removed": record(&self.removed),
            "updated": record(&self.updated),
        })
    }

    /// `ElementsDelta.restore(dto)`.
    pub fn from_dto(dto: &Value) -> Option<ElementsDelta> {
        let record = |key: &str| -> Option<IndexMap<String, Delta>> {
            dto.get(key)?
                .as_object()?
                .iter()
                .map(|(id, d)| Some((id.clone(), Delta::from_dto(d)?)))
                .collect()
        };
        Some(ElementsDelta::create(
            record("added")?,
            record("removed")?,
            record("updated")?,
            false,
        ))
    }
}

/// `ElementsDelta.redrawElements(nextElements, changedElements)`
/// (`delta.ts:2034-2057`): the bound text boxes, then the bound arrows, of
/// the changed elements are laid out again, through the leaf layout of
/// [`HistoryEnv::redraw_text_bounding_box`] and
/// [`HistoryEnv::update_bound_elements`].
///
/// Upstream's `changedElements` holds the same instances as
/// `nextElements`, so what layout mutates shows through it; here `changed`
/// is re-read from `elements` before each leaf call to the same effect.
pub fn redraw_elements(
    elements: &mut SceneElementsMap,
    changed: &mut SceneElementsMap,
    env: &mut dyn HistoryEnv,
) -> Result<(), String> {
    redraw_text_bounding_boxes(elements, changed, env)?;
    // needs ordered nextElements to avoid z-index binding issues
    redraw_bound_arrows(elements, changed, env)
}

/// `changed` re-read from `elements` (upstream shares the instances).
fn refresh_changed(changed: &mut SceneElementsMap, elements: &SceneElementsMap) {
    for (id, element) in changed.iter_mut() {
        if let Some(current) = elements.get(id) {
            if current != element {
                *element = current.clone();
            }
        }
    }
}

/// `isBoundToContainer`: a text whose `containerId` is not `null`.
fn is_bound_to_container(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Text(t) if t.container_id.is_some())
}

/// `hasBoundTextElement`: a text container (`isTextBindableContainer`,
/// locked or not) with a `text` entry in `boundElements`.
fn has_bound_text_element(element: &Element) -> bool {
    element.element_type().is_text_container()
        && element.base.bound_elements.as_ref().is_some_and(|b| {
            b.iter()
                .any(|b| b.kind == excali_core::element::BoundElementType::Text)
        })
}

/// `redrawTextBoundingBoxes(scene, changed)` (`delta.ts:2059-2106`): for
/// every changed bound text, its container, and for every changed
/// container, its bound text, both looked up among the non-deleted
/// elements; one pair per container id (a later pair replaces an earlier
/// one, keeping its place), and a pair is skipped when either side is
/// deleted.
fn redraw_text_bounding_boxes(
    elements: &mut SceneElementsMap,
    changed: &mut SceneElementsMap,
    env: &mut dyn HistoryEnv,
) -> Result<(), String> {
    let non_deleted = |id: &str| elements.get(id).filter(|e| !e.base.is_deleted);
    // container id -> (container id, bound text id)
    let mut boxes: IndexMap<String, (String, String)> = IndexMap::new();
    for element in changed.values() {
        if is_bound_to_container(element) {
            let container = text_container_id(element)
                .filter(|id| !id.is_empty())
                .and_then(non_deleted);
            if let Some(container) = container {
                boxes.insert(
                    container.base.id.clone(),
                    (container.base.id.clone(), element.base.id.clone()),
                );
            }
        }
        if has_bound_text_element(element) {
            let bound_text =
                excali_scene::bounds::get_bound_text_element_id(element).and_then(non_deleted);
            if let Some(bound_text) = bound_text {
                boxes.insert(
                    element.base.id.clone(),
                    (element.base.id.clone(), bound_text.base.id.clone()),
                );
            }
        }
    }
    for (container_id, text_id) in boxes.into_values() {
        let deleted = |id: &str| elements.get(id).is_none_or(|e| e.base.is_deleted);
        if deleted(&container_id) || deleted(&text_id) {
            // skip redraw if one of them is deleted, as it would not result
            // in a meaningful redraw
            continue;
        }
        env.redraw_text_bounding_box(elements, &text_id, &container_id)?;
        refresh_changed(changed, elements);
    }
    Ok(())
}

/// `redrawBoundArrows(scene, changed)` (`delta.ts:2108-2124`): the arrows
/// bound to every changed, non-deleted bindable element are updated, with
/// the changed elements passed through.
fn redraw_bound_arrows(
    elements: &mut SceneElementsMap,
    changed: &mut SceneElementsMap,
    env: &mut dyn HistoryEnv,
) -> Result<(), String> {
    let ids: Vec<String> = changed.keys().cloned().collect();
    for id in ids {
        let Some(element) = changed.get(&id) else {
            continue;
        };
        if !element.base.is_deleted && element.is_bindable() {
            env.update_bound_elements(elements, &id, changed)?;
            refresh_changed(changed, elements);
        }
    }
    Ok(())
}

/// `reorderElements`: the elements in fractional index order with the
/// changed elements that moved given new indices, and whether any moved.
fn reorder_elements(
    elements: &SceneElementsMap,
    changed: &SceneElementsMap,
    env: &mut dyn HistoryEnv,
) -> Result<(SceneElementsMap, bool), DeltaError> {
    let unordered: Vec<Element> = elements.values().cloned().collect();
    let mut ordered = unordered.clone();
    order_by_fractional_index(&mut ordered);
    let moved: HashSet<String> = unordered
        .iter()
        .zip(&ordered)
        .filter(|(u, o)| u.base.id != o.base.id && changed.contains_key(&u.base.id))
        .map(|(u, _)| u.base.id.clone())
        .collect();
    sync_moved_indices(&mut ordered, &moved, &mut DynStamp(env)).map_err(DeltaError::Indices)?;
    let mut map = SceneElementsMap::new();
    for element in ordered {
        map.insert(element.base.id.clone(), element);
    }
    Ok((map, !moved.is_empty()))
}

/// `checkForVisibleDifference(previous, next, previousElements,
/// nextElements)` (`delta.ts:1809-1875`): whether the element paints
/// differently. A missing or deleted element paints nothing, and so does
/// an empty text with no arrow bindings that is not an arrow's label;
/// `index` (checked by the reorder), `version`, `versionNonce`, `updated`
/// and `created` do not count, and `boundElements` counts without the
/// references to empty labels.
fn check_for_visible_difference<'a>(
    previous: Option<&'a Element>,
    next: Option<&'a Element>,
    before_lookup: &dyn Fn(&str) -> Option<&'a Element>,
    after_lookup: &dyn Fn(&str) -> Option<&'a Element>,
) -> bool {
    let visible = |element: Option<&'a Element>, lookup: &dyn Fn(&str) -> Option<&'a Element>| {
        let element = element.filter(|e| !e.base.is_deleted)?;
        if let ElementKind::Text(t) = &element.kind {
            let container = lookup(t.container_id.as_deref().unwrap_or(""));
            if js_trim(&t.text).is_empty()
                && element
                    .base
                    .bound_elements
                    .as_ref()
                    .is_none_or(Vec::is_empty)
                && !container.is_some_and(is_arrow)
            {
                return None;
            }
        }
        Some(element)
    };
    let before = visible(previous, before_lookup);
    let after = visible(next, after_lookup);
    let (Some(before), Some(after)) = (before, after) else {
        return before.is_some() != after.is_some();
    };
    let visible_bindings = |element: &Element, lookup: &dyn Fn(&str) -> Option<&'a Element>| {
        bound_ids(element)
            .into_iter()
            .filter(|(id, kind)| {
                let label = lookup(id);
                *kind != "text"
                    || is_arrow(element)
                    || label.is_none_or(|l| match &l.kind {
                        ElementKind::Text(t) => !js_trim(&t.text).is_empty(),
                        _ => true,
                    })
            })
            .map(|(id, kind)| format!("{kind}:{id}"))
            .collect::<Vec<_>>()
    };
    Delta::get_differences(&before.to_map(), &after.to_map())
        .iter()
        .any(|key| match key.as_str() {
            "index" | "version" | "versionNonce" | "updated" | "created" => false,
            "boundElements" => {
                visible_bindings(before, before_lookup) != visible_bindings(after, after_lookup)
            }
            _ => true,
        })
}

// ---------------------------------------------------------------------------
// applying

/// A reference to an element as upstream's binding repair holds one: an
/// element that has not been replaced in the next elements yet is shared
/// with the previous elements, so an update replaces it and the reference
/// keeps seeing the old instance; one that was replaced already is updated
/// in place, and the reference sees every update.
struct Handle {
    id: String,
    stale: Option<Element>,
}

struct Applier<'a> {
    prev: &'a SceneElementsMap,
    next: SceneElementsMap,
    /// Ids whose element in `next` is no longer the instance in `prev`.
    touched: HashSet<String>,
    snapshot: &'a SnapshotElements,
    options: &'a ApplyToOptions,
    contains_zindex_difference: bool,
    direction: Option<bool>,
    affected: SceneElementsMap,
}

fn element_error(id: &str) -> impl Fn(MutateError) -> DeltaError + '_ {
    move |e| DeltaError::Element(id.to_owned(), e)
}

impl Applier<'_> {
    /// `createApplier(...)(deltas)`.
    fn apply_deltas(
        &mut self,
        deltas: &IndexMap<String, Delta>,
        env: &mut dyn HistoryEnv,
    ) -> Result<SceneElementsMap, DeltaError> {
        let mut applied = SceneElementsMap::new();
        for (id, delta) in deltas {
            let element = self.get_element(id, &delta.inserted, env)?;
            let next = self.apply_delta(element, delta, env)?;
            let next_id = next.base.id.clone();
            self.next.insert(next_id.clone(), next.clone());
            self.touched.insert(next_id.clone());
            applied.insert(next_id, next.clone());
            if self.direction.is_none() {
                if let Some(prev) = self.prev.get(id) {
                    // forward unless the version went down
                    self.direction = Some(prev.base.version <= next.base.version);
                }
            }
        }
        Ok(applied)
    }

    /// `createGetter(...)(id, partial)`.
    fn get_element(
        &mut self,
        id: &str,
        partial: &Partial,
        env: &mut dyn HistoryEnv,
    ) -> Result<Element, DeltaError> {
        if let Some(element) = self.next.get(id) {
            return Ok(element.clone());
        }
        if let Some(element) = self.snapshot.get(id) {
            // brought from the snapshot: a possible z-index difference
            self.contains_zindex_difference = true;
            return Ok((**element).clone());
        }
        // not in elements, not in the snapshot: added remotely
        let mut map = Map::new();
        map.insert("id".into(), Value::String(id.to_owned()));
        map.insert("version".into(), num(1.0));
        element_with_partial(map, 1.0, partial, env).map_err(element_error(id))
    }

    /// `applyDelta(element, delta, flags, options)`.
    fn apply_delta(
        &mut self,
        element: Element,
        delta: &Delta,
        env: &mut dyn HistoryEnv,
    ) -> Result<Element, DeltaError> {
        let mut partial = Partial::new();
        for (key, value) in &delta.inserted {
            if key == "boundElements" || self.options.excluded_properties.contains(key) {
                continue;
            }
            partial.insert(key.clone(), value.clone());
        }
        let deleted_bound = as_array(delta.deleted.prop("boundElements"));
        let inserted_bound = as_array(delta.inserted.prop("boundElements"));
        if !deleted_bound.is_empty() || !inserted_bound.is_empty() {
            let current: Vec<Value> = element
                .base
                .bound_elements
                .iter()
                .flatten()
                .filter_map(|b| serde_json::to_value(b).ok())
                .collect();
            let merged = Delta::merge_arrays(&current, inserted_bound, deleted_bound, binding_id);
            partial.insert("boundElements".into(), Some(Value::Array(merged)));
        }
        if !self.contains_zindex_difference {
            self.contains_zindex_difference =
                !same_value(delta.deleted.prop("index"), delta.inserted.prop("index"));
        }
        let id = element.base.id.clone();
        element_with_partial(element.to_map(), element.base.version, &partial, env)
            .map_err(element_error(&id))
    }

    // -- binding repair (resolveConflicts) ----------------------------------

    fn capture(&self, id: &str) -> Option<Handle> {
        let element = self.next.get(id)?;
        Some(Handle {
            id: id.to_owned(),
            stale: (!self.touched.contains(id)).then(|| element.clone()),
        })
    }

    fn previous(&self, id: &str) -> Option<Handle> {
        Some(Handle {
            id: id.to_owned(),
            stale: Some(self.prev.get(id)?.clone()),
        })
    }

    fn read<'h>(&'h self, handle: &'h Handle) -> &'h Element {
        match &handle.stale {
            Some(element) => element,
            None => &self.next[&handle.id],
        }
    }

    /// The `updater` of `resolveConflicts`: update the element in the next
    /// elements, with the version moved one step in the apply direction.
    fn update(
        &mut self,
        id: &str,
        mut updates: ElementUpdate,
        env: &mut dyn HistoryEnv,
    ) -> Result<(), DeltaError> {
        let Some(next) = self.next.get(id) else {
            return Ok(());
        };
        let prev = self.prev.get(id);
        let forward = self.direction.unwrap_or(true);
        let next_version = if forward {
            next.base.version + 1.0
        } else {
            next.base.version - 1.0
        };
        let affected = if prev.is_some() && !self.touched.contains(id) {
            // a new instance, as the element was not modified yet
            updates.insert("version".into(), num(next_version));
            new_element_with(next, updates, true, env).map_err(element_error(id))?
        } else {
            // don't modify the version further if it is already different
            let version = if prev.map(|p| p.base.version) != Some(next.base.version) {
                next.base.version
            } else {
                next_version
            };
            updates.insert("version".into(), num(version));
            let mut element = next.clone();
            mutate_element(&mut element, &self.next, updates, env).map_err(element_error(id))?;
            element
        };
        self.affected.insert(id.to_owned(), affected.clone());
        self.next.insert(id.to_owned(), affected);
        self.touched.insert(id.to_owned());
        Ok(())
    }

    /// `newBoundElements(boundElements, idsToRemove, elementsToAdd)` as an
    /// update of `boundElements`.
    fn bound_elements_update(
        element: &Element,
        remove: Option<&str>,
        add: Option<(&str, &str)>,
    ) -> ElementUpdate {
        let value = match &element.base.bound_elements {
            None => Value::Null,
            Some(bound) => {
                let mut next: Vec<Value> = bound
                    .iter()
                    .filter(|b| Some(b.id.as_str()) != remove)
                    .filter_map(|b| serde_json::to_value(b).ok())
                    .collect();
                if let Some((id, kind)) = add {
                    next.push(json!({"id": id, "type": kind}));
                }
                Value::Array(next)
            }
        };
        [("boundElements".to_string(), value)].into_iter().collect()
    }

    fn null_update(prop: &str) -> ElementUpdate {
        [(prop.to_string(), Value::Null)].into_iter().collect()
    }

    /// `bindableElementsVisitor`: the properties binding `element` to other
    /// elements (`frameId`, a text's `containerId`, an arrow's
    /// `startBinding` and `endBinding`) and the ids they name.
    fn bindable_refs(element: &Element) -> Vec<(&'static str, String)> {
        let mut refs = Vec::new();
        if let Some(frame) = element.base.frame_id.as_ref().filter(|f| !f.is_empty()) {
            refs.push(("frameId", frame.clone()));
        }
        if let Some(container) = text_container_id(element) {
            refs.push(("containerId", container.to_owned()));
        }
        if is_arrow(element) {
            if let Some(linear) = element.kind.linear() {
                if let Some(b) = &linear.start_binding {
                    refs.push(("startBinding", b.element_id.clone()));
                }
                if let Some(b) = &linear.end_binding {
                    refs.push(("endBinding", b.element_id.clone()));
                }
            }
        }
        refs
    }

    /// `boundElementsVisitor`: the ids in a bindable element's
    /// `boundElements`.
    fn bound_refs(element: &Element) -> Vec<(String, &'static str)> {
        if element.is_bindable() {
            bound_ids(element)
        } else {
            Vec::new()
        }
    }

    fn is_live(&self, handle: &Option<Handle>) -> bool {
        handle
            .as_ref()
            .is_some_and(|h| !self.read(h).base.is_deleted)
    }

    /// `BoundElement.unbindAffected`: remove the element from the
    /// `boundElements` of the live elements it is bound to.
    fn bound_unbind_affected(
        &mut self,
        bound: Option<Handle>,
        env: &mut dyn HistoryEnv,
    ) -> Result<(), DeltaError> {
        let Some(bound) = bound else {
            return Ok(());
        };
        let bound_id = self.read(&bound).base.id.clone();
        for (_, id) in Applier::bindable_refs(self.read(&bound)) {
            let bindable = self.capture(&id);
            if !self.is_live(&bindable) {
                continue;
            }
            let Some(bindable) = bindable else { continue };
            for (bid, _) in Applier::bound_refs(self.read(&bindable)) {
                if bid == bound_id {
                    let updates =
                        Applier::bound_elements_update(self.read(&bindable), Some(&bid), None);
                    self.update(&bindable.id, updates, env)?;
                }
            }
        }
        Ok(())
    }

    /// `BoundElement.rebindAffected`: add a live element to the
    /// `boundElements` of the elements it is bound to, or unbind it from
    /// deleted ones (and from a container that has another label).
    fn bound_rebind_affected(
        &mut self,
        bound: Option<Handle>,
        env: &mut dyn HistoryEnv,
    ) -> Result<(), DeltaError> {
        let Some(bound) = bound else {
            return Ok(());
        };
        if self.read(&bound).base.is_deleted {
            return Ok(());
        }
        let bound_id = self.read(&bound).base.id.clone();
        let bound_is_arrow = is_arrow(self.read(&bound));
        let bound_is_text = is_text(self.read(&bound));
        for (prop, id) in Applier::bindable_refs(self.read(&bound)) {
            let bindable = self.capture(&id);
            if !self.is_live(&bindable) {
                // bindings from non deleted elements into deleted ones are incorrect
                self.update(&bound_id, Applier::null_update(prop), env)?;
                continue;
            }
            let Some(bindable) = bindable else { continue };
            // frame bindings are unidirectional, there is nothing to rebind
            if prop == "frameId" {
                continue;
            }
            let already = bound_ids(self.read(&bindable))
                .iter()
                .any(|(bid, _)| *bid == bound_id);
            if already {
                continue;
            }
            if bound_is_arrow {
                let updates = Applier::bound_elements_update(
                    self.read(&bindable),
                    None,
                    Some((&bound_id, "arrow")),
                );
                self.update(&bindable.id, updates, env)?;
            }
            if bound_is_text {
                let has_text = bound_ids(self.read(&bindable))
                    .iter()
                    .any(|(_, kind)| *kind == "text");
                if !has_text {
                    // rebind only if there is no other text bound already
                    let updates = Applier::bound_elements_update(
                        self.read(&bindable),
                        None,
                        Some((&bound_id, "text")),
                    );
                    self.update(&bindable.id, updates, env)?;
                } else {
                    // unbind otherwise
                    self.update(&bound_id, Applier::null_update(prop), env)?;
                }
            }
        }
        Ok(())
    }

    /// `BindableElement.unbindAffected`: reset the bindings of the live
    /// elements bound to this one; with `only_text`, only texts (arrows
    /// cannot be rebound).
    fn bindable_unbind_affected(
        &mut self,
        bindable: Option<Handle>,
        only_text: bool,
        env: &mut dyn HistoryEnv,
    ) -> Result<(), DeltaError> {
        let Some(bindable) = bindable else {
            return Ok(());
        };
        let bindable_id = self.read(&bindable).base.id.clone();
        for (bid, _) in Applier::bound_refs(self.read(&bindable)) {
            let bound = self.capture(&bid);
            if !self.is_live(&bound) {
                continue;
            }
            let Some(bound) = bound else { continue };
            for (prop, id) in Applier::bindable_refs(self.read(&bound)) {
                if id == bindable_id && (!only_text || is_text(self.read(&bound))) {
                    self.update(&bound.id, Applier::null_update(prop), env)?;
                }
            }
        }
        Ok(())
    }

    /// `BindableElement.rebindAffected`: drop deleted elements from this
    /// element's `boundElements`, and bind its last text to it (unbinding
    /// any other text).
    fn bindable_rebind_affected(
        &mut self,
        bindable: Option<Handle>,
        env: &mut dyn HistoryEnv,
    ) -> Result<(), DeltaError> {
        let Some(bindable) = bindable else {
            return Ok(());
        };
        if self.read(&bindable).base.is_deleted {
            return Ok(());
        }
        let bindable_id = self.read(&bindable).base.id.clone();
        for (bid, _) in Applier::bound_refs(self.read(&bindable)) {
            let bound = self.capture(&bid);
            if !self.is_live(&bound) {
                let updates =
                    Applier::bound_elements_update(self.read(&bindable), Some(&bid), None);
                self.update(&bindable.id, updates, env)?;
                continue;
            }
            let Some(bound) = bound else { continue };
            if !is_text(self.read(&bound)) {
                continue;
            }
            let last_text = bound_ids(self.read(&bindable))
                .into_iter()
                .rev()
                .find(|(_, kind)| *kind == "text")
                .map(|(id, _)| id);
            let bound_id = self.read(&bound).base.id.clone();
            let container = text_container_id(self.read(&bound)).map(str::to_owned);
            if last_text.as_deref() == Some(bound_id.as_str()) {
                if container.as_deref() != Some(bindable_id.as_str()) {
                    // rebind if not bound already
                    let updates: ElementUpdate = [(
                        "containerId".to_string(),
                        Value::String(bindable_id.clone()),
                    )]
                    .into_iter()
                    .collect();
                    self.update(&bound_id, updates, env)?;
                }
            } else {
                if container.is_some() {
                    // unbind if not unbound already
                    self.update(&bound_id, Applier::null_update("containerId"), env)?;
                }
                // unbind from boundElements, as the element got bound to
                // another element in the meantime
                let updates =
                    Applier::bound_elements_update(self.read(&bindable), Some(&bound_id), None);
                self.update(&bindable.id, updates, env)?;
            }
        }
        Ok(())
    }

    /// `ElementsDelta.unbindAffected(prevElements, nextElements, id)`.
    fn unbind_affected(&mut self, id: &str, env: &mut dyn HistoryEnv) -> Result<(), DeltaError> {
        self.bound_unbind_affected(self.previous(id), env)?;
        self.bound_unbind_affected(self.capture(id), env)?;
        self.bindable_unbind_affected(self.previous(id), false, env)?;
        self.bindable_unbind_affected(self.capture(id), false, env)
    }

    /// `ElementsDelta.rebindAffected(prevElements, nextElements, id)`.
    fn rebind_affected(&mut self, id: &str, env: &mut dyn HistoryEnv) -> Result<(), DeltaError> {
        self.bound_unbind_affected(self.previous(id), env)?;
        self.bound_rebind_affected(self.capture(id), env)?;
        self.bindable_unbind_affected(self.previous(id), true, env)?;
        self.bindable_rebind_affected(self.capture(id), env)
    }

    /// `resolveConflicts(prevElements, nextElements, applyDirection)`:
    /// repair the bindings around the removed, added and binding-updated
    /// elements, and squash the changes this makes into `delta`.
    fn resolve_conflicts(
        &mut self,
        delta: &mut ElementsDelta,
        env: &mut dyn HistoryEnv,
    ) -> Result<(), DeltaError> {
        let removed: Vec<String> = delta.removed.keys().cloned().collect();
        let added: Vec<String> = delta.added.keys().cloned().collect();
        let updated: Vec<String> = delta
            .updated
            .iter()
            .filter(|(_, d)| {
                d.deleted
                    .keys()
                    .chain(d.inserted.keys())
                    .any(|k| BINDING_PROPERTIES.contains(&k.as_str()))
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &removed {
            self.unbind_affected(id, env)?;
        }
        for id in &added {
            self.rebind_affected(id, env)?;
        }
        for id in &updated {
            if self.next.get(id).is_none_or(|e| e.base.is_deleted) {
                // skip fixing bindings for updates on deleted elements
                continue;
            }
            self.rebind_affected(id, env)?;
        }
        let prev_affected: SceneElementsMap = self
            .prev
            .iter()
            .filter(|(id, _)| self.affected.contains_key(*id))
            .map(|(id, e)| (id.clone(), e.clone()))
            .collect();
        delta.squash(&ElementsDelta::calculate(
            &prev_affected,
            &self.affected,
            env,
        ));
        Ok(())
    }
}
