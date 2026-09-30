//! `restoreElements` (`packages/excalidraw/data/restore.ts:946-1138`) and
//! `bumpElementVersions` (`restore.ts:1150-1173`): the scene-level passes
//! on top of the per-element rules ([`super::restore_element`]). Research
//! page `site/content/research/data-model.md`, section 3.4 "Scene-level
//! passes", and section 7 (binding and containment).
//!
//! Like the per-element restore, it works on untyped objects in the
//! sentinel form of [`crate::json`] and reproduces the JS semantics that
//! decide the result: truthiness, `===`, `Map` and `Set` keys (`7` and `"7"`
//! differ, an object is only ever itself), relational comparison of index
//! values of any type, and the `TypeError`s upstream throws out of the
//! whole call.
//!
//! Upstream mutates the restored objects in place, and every pass sees the
//! others' changes; here the elements live in one vector in restored order,
//! addressed by position, and bound text order is a permutation of it
//! applied at the end, so a lookup by id (upstream's `restoredElementsMap`,
//! built once) always finds the element's current state.
//!
//! Two JS behaviours that JSON values cannot carry are not reproduced.
//! Object identity: an element `id` that is an object or an array is a
//! `Map` and `Set` key only as itself, which the port models as never
//! found; but upstream's `repairBoundElement` copies a text's own id
//! reference into its container's `boundElements`, after which upstream
//! finds the text by it and the port does not. And a string item of
//! `elements`, which `arrayToMap` keys by the string itself, so an arrow
//! binding to that id would find it; the port drops non-object items
//! before building the map. Neither can come from a file an editor wrote.

use serde_json::{json, Map, Value};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;

use super::element::{bump_version, plus, restore_element_encoded, sticky_note_font_size, MapKey};
use super::{ElementsMap, EscapingEnv, RestoreEnv, RestoreError, RestoreOptions};
use crate::color;
use crate::constants::COLOR_BLACK;
use crate::js::{self, TypeError};
use crate::json;
use crate::order_key::{generate_n_keys_between, validate_order_key, OrderKeyError};

/// `restoreElements`' `opts` (`restore.ts:949-955`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestoreElementsOptions {
    /// Refit text to its content, and sticky notes with their labels,
    /// through [`RestoreEnv::refresh_text_dimensions`] and
    /// [`RestoreEnv::sticky_note_layout`] (`restore.ts:1032-1045`,
    /// `931-941`). Only with `repair_bindings`.
    pub refresh_dimensions: bool,
    /// Run the binding repairs (`restore.ts:1012-1137`): frames, bound text
    /// in both directions, linear bindings, sticky notes, bound text order
    /// and elbow arrow fix-ups. What upstream's file loading and initial
    /// scene pass (`data/blob.ts:161-164`, `components/App.tsx:3653-3656`).
    pub repair_bindings: bool,
    /// Mark invisibly small elements and empty text deleted, bumping their
    /// versions (`restore.ts:983-994`, `583-589`).
    pub delete_invisible_elements: bool,
}

/// An error upstream throws out of `restoreElements` or
/// `bumpElementVersions` as a whole (an element whose own restore throws is
/// dropped instead), with upstream's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreElementsError {
    message: String,
}

impl RestoreElementsError {
    fn type_error(message: impl Into<String>) -> RestoreElementsError {
        RestoreElementsError {
            message: message.into(),
        }
    }

    /// Upstream's `Error.message` (V8's for a `TypeError`).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RestoreElementsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RestoreElementsError {}

impl From<TypeError> for RestoreElementsError {
    fn from(err: TypeError) -> RestoreElementsError {
        RestoreElementsError::type_error(err.0)
    }
}

impl From<OrderKeyError> for RestoreElementsError {
    fn from(err: OrderKeyError) -> RestoreElementsError {
        RestoreElementsError::type_error(err.message())
    }
}

impl From<RestoreError> for RestoreElementsError {
    fn from(err: RestoreError) -> RestoreElementsError {
        RestoreElementsError::type_error(err.to_string())
    }
}

/// What [`RestoreEnv::refresh_text_dimensions`] is asked:
/// `refreshTextDimensions(text, getContainerElement(text), elementsMap)`
/// (`packages/element/src/newElement.ts:533-...`), for every text but a
/// sticky note label when `refresh_dimensions` is set.
#[derive(Debug, Clone, Copy)]
pub struct TextDimensionsRequest<'a> {
    /// The text as it stands after its binding repair.
    pub text: &'a Map<String, Value>,
    /// Its container, when `containerId` names an element.
    pub container: Option<&'a Map<String, Value>>,
    /// Every restored element, in restored order (upstream's
    /// `restoredElementsMap`).
    pub elements: &'a [Map<String, Value>],
}

/// What [`RestoreEnv::sticky_note_layout`] is asked:
/// `getStickyNoteLayout(note, getBoundTextElement(note))`
/// (`packages/element/src/stickyNote.ts:669-...`), for every non-deleted
/// sticky note when `refresh_dimensions` is set.
#[derive(Debug, Clone, Copy)]
pub struct StickyNoteLayoutRequest<'a> {
    pub note: &'a Map<String, Value>,
    /// The note's label: its first bound element of type text, when that
    /// id names an element.
    pub text: Option<&'a Map<String, Value>>,
    /// Every restored element, in restored order.
    pub elements: &'a [Map<String, Value>],
}

/// `getStickyNoteLayout`'s result: the keys to assign to the note and to
/// its label (`restore.ts:937-941`).
#[derive(Debug, Clone, PartialEq)]
pub struct StickyNoteLayout {
    pub container: Map<String, Value>,
    pub text: Option<Map<String, Value>>,
}

/// What [`RestoreEnv::update_elbow_arrow_points`] is asked:
/// `updateElbowArrowPoints(arrow, restoredElementsMap, {points})`
/// (`packages/element/src/elbowArrow.ts:907-...`), for an elbow arrow with
/// no binding whose points are not all axis-aligned (`restore.ts:1076-1093`).
#[derive(Debug, Clone, Copy)]
pub struct ElbowArrowRequest<'a> {
    /// The arrow as restored and repaired.
    pub arrow: &'a Map<String, Value>,
    /// The new points: `[0, 0]` and the arrow's last point.
    pub points: &'a [Value],
    /// Every restored element, in restored order.
    pub elements: &'a [Map<String, Value>],
}

/// `restoreElements(elements, existing, opts)` (`restore.ts:946-1138`).
///
/// `elements` is what a file's `elements` holds (`&[]` for a missing or
/// `null` one); `existing` the elements already in the scene, used to
/// migrate legacy arrow bindings and to bump versions past local ones.
///
/// 1. Each element is restored by [`super::restore_element`]; a
///    `selection`, an unknown type, a non-object item and an element whose
///    restore throws are dropped. A `null` item throws.
/// 2. With `delete_invisible_elements`, an element that is invisibly small
///    as given (`isInvisiblySmallElement`, `sizeHelpers.ts:61-78`: a line,
///    arrow or freedraw with fewer than 2 points, a 2-point arrow whose ends
///    are within 0.1, anything else 0 x 0) is deleted, its version bumped
///    past the existing element's.
/// 3. An id seen before gets `randomId()`.
/// 4. `syncInvalidIndices` fills every invalid `index`, bumping versions.
///
/// With `repair_bindings`, then, element by element: a `frameId` naming no
/// element is cleared; a bound text takes its container's angle (0 for an
/// arrow) and is added to the container's `boundElements`, or loses a
/// `containerId` naming no element; a container's `boundElements` are
/// deduplicated and cleared of missing and deleted elements, and a listed
/// text without `containerId` gets it; with `refresh_dimensions` text is
/// refitted ([`RestoreEnv::refresh_text_dimensions`]); a line's bindings,
/// and an arrow's to a missing element, are cleared. Then sticky note
/// labels get a font ceiling (`baseFontSize`) and share one visible stroke
/// colour with their note, other text loses its ceiling, and with
/// `refresh_dimensions` each note is refitted with its label
/// ([`RestoreEnv::sticky_note_layout`]). Bound text is moved right after
/// its container (`normalizeBoundElementsOrder`, `sortElements.ts:57-110`)
/// and gets an index there (`syncMovedIndices`). Last, an unbound elbow
/// arrow with a segment that is not axis-aligned is re-routed
/// ([`RestoreEnv::update_elbow_arrow_points`]), and a self-bound elbow arrow
/// with a point beyond 1e6 is reset around its target.
///
/// Returns the elements in order, or the error upstream throws.
pub fn restore_elements(
    elements: &[Value],
    existing: Option<&[Map<String, Value>]>,
    opts: RestoreElementsOptions,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<Map<String, Value>>, RestoreElementsError> {
    let elements: Vec<Value> = elements.iter().map(json::escape).collect();
    let existing: Option<Vec<Map<String, Value>>> =
        existing.map(|e| e.iter().map(json::escape_map).collect());
    let mut env = EscapingEnv(env);
    restore_scene(&elements, existing.as_deref(), opts, &mut env)
        .map(|restored| restored.iter().map(json::decode_map).collect())
}

/// `bumpElementVersions(elements, local)` (`restore.ts:1150-1173`): an
/// element whose local copy (same id) has a higher version, or the same
/// version and another `versionNonce`, gets `local.version + 1`, a fresh
/// `versionNonce` and `updated` now. Meant for restored elements that
/// replace local ones, so reconciliation keeps them.
pub fn bump_element_versions(
    elements: Vec<Map<String, Value>>,
    local: Option<&[Map<String, Value>]>,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<Map<String, Value>>, RestoreElementsError> {
    let elements: Vec<Map<String, Value>> = elements.iter().map(json::escape_map).collect();
    let local: Option<Vec<Map<String, Value>>> =
        local.map(|l| l.iter().map(json::escape_map).collect());
    let mut env = EscapingEnv(env);
    bump_versions(elements, local.as_deref(), &mut env)
        .map(|bumped| bumped.iter().map(json::decode_map).collect())
}

/// [`restore_elements`] without local elements on sentinel-form values
/// (from [`json::parse`]), giving sentinel-form elements: what the scene
/// loader (`crate::document::load_scene_json`) restores, so lone
/// surrogates in a file survive restore. The loader owns the elements,
/// and each is moved into the lookup map restore reads (`arrayToMap`)
/// rather than copied.
pub(crate) fn restore_elements_sentinel_owned(
    elements: Vec<Value>,
    opts: RestoreElementsOptions,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<Map<String, Value>>, RestoreElementsError> {
    let targets = target_maps(elements.into_iter().map(Cow::Owned))?;
    restore_targets(
        ElementsMap::from_encoded(targets),
        None,
        opts,
        &mut EscapingEnv(env),
    )
}

/// Which upstream call [`restore_elements_encoded`] makes.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum SceneCall {
    /// `restoreElements(elements, existing, opts)`.
    Restore(RestoreElementsOptions),
    /// `bumpElementVersions(restoreElements(elements, null), existing)`.
    BumpVersions,
}

/// [`restore_elements`] and [`bump_element_versions`] on sentinel-form
/// values, as the fixture calls them.
#[cfg(test)]
pub(crate) fn restore_elements_encoded(
    elements: &[Value],
    existing: Option<&[Map<String, Value>]>,
    call: SceneCall,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<Map<String, Value>>, RestoreElementsError> {
    match call {
        SceneCall::Restore(opts) => restore_scene(elements, existing, opts, env),
        SceneCall::BumpVersions => {
            let restored = restore_scene(elements, None, RestoreElementsOptions::default(), env)?;
            bump_versions(restored, existing, env)
        }
    }
}

// -- restoreElements ----------------------------------------------------------------

type Elements = Vec<Map<String, Value>>;

/// Elements by id, the last of an id winning (`arrayToMap`), as positions.
struct IdMap(HashMap<MapKey, usize>);

impl IdMap {
    fn new(elements: &[Map<String, Value>]) -> IdMap {
        let mut map = HashMap::new();
        for (i, element) in elements.iter().enumerate() {
            if let Some(key) = MapKey::of(element.get("id")) {
                map.insert(key, i);
            }
        }
        IdMap(map)
    }

    /// `map.get(key)`; an object or array key is never found.
    fn get(&self, key: Option<&Value>) -> Option<usize> {
        MapKey::of(key).and_then(|k| self.0.get(&k).copied())
    }
}

fn restore_scene(
    elements: &[Value],
    existing: Option<&[Map<String, Value>]>,
    opts: RestoreElementsOptions,
    env: &mut dyn RestoreEnv,
) -> Result<Elements, RestoreElementsError> {
    let targets = target_maps(elements.iter().map(Cow::Borrowed))?;
    restore_targets(ElementsMap::from_encoded(targets), existing, opts, env)
}

/// The objects restore works on: arrayToMap(targetElements) reads every
/// item's id; other primitives and arrays are dropped by restoreElement
/// (their `type` is undefined).
fn target_maps<'a>(
    items: impl Iterator<Item = Cow<'a, Value>>,
) -> Result<Vec<Map<String, Value>>, RestoreElementsError> {
    let mut targets = Vec::new();
    for item in items {
        match item {
            Cow::Borrowed(Value::Null) | Cow::Owned(Value::Null) => {
                return Err(RestoreElementsError::type_error(
                    "Cannot read properties of null (reading 'id')",
                ))
            }
            Cow::Borrowed(Value::Object(element)) => targets.push(element.clone()),
            Cow::Owned(Value::Object(element)) => targets.push(element),
            _ => {}
        }
    }
    Ok(targets)
}

/// [`restore_scene`] of the target objects, in order, in `targets_map`.
fn restore_targets(
    targets_map: ElementsMap,
    existing: Option<&[Map<String, Value>]>,
    opts: RestoreElementsOptions,
    env: &mut dyn RestoreEnv,
) -> Result<Elements, RestoreElementsError> {
    let targets = targets_map.elements();
    let existing_map = existing.map(|e| ElementsMap::from_encoded(e.to_vec()));
    let element_opts = RestoreOptions {
        delete_invisible_elements: opts.delete_invisible_elements,
    };

    let mut seen: HashSet<MapKey> = HashSet::new();
    let mut restored: Elements = Vec::new();
    for element in targets {
        // legacy, no longer kept in elements
        if element.get("type").and_then(Value::as_str) == Some("selection") {
            continue;
        }
        // restoreElement throwing drops the element (console.error)
        let Ok(Some(mut migrated)) = restore_element_encoded(
            element,
            &targets_map,
            existing_map.as_ref(),
            element_opts,
            env,
        ) else {
            continue;
        };
        if opts.delete_invisible_elements && is_invisibly_small(element)? {
            let local_version = existing_map
                .as_ref()
                .and_then(|m| m.get_encoded(element.get("id")))
                .and_then(|local| local.get("version"));
            bump_version_from(&mut migrated, local_version, env)?;
            migrated.insert("isDeleted".to_owned(), json!(true));
        }
        let key = MapKey::of(migrated.get("id"));
        if key.as_ref().is_some_and(|k| seen.contains(k)) {
            migrated.insert("id".to_owned(), Value::String(env.random_id()));
        }
        if let Some(key) = MapKey::of(migrated.get("id")) {
            seen.insert(key);
        }
        restored.push(migrated);
    }

    let order: Vec<usize> = (0..restored.len()).collect();
    sync_invalid_indices(&mut restored, &order, env)?;

    if !opts.repair_bindings {
        return Ok(restored);
    }

    let map = IdMap::new(&restored);
    for i in 0..restored.len() {
        if js::truthy(restored[i].get("frameId")) {
            repair_frame_membership(&mut restored, &map, i);
        }
        if is_type(&restored[i], "text") && js::truthy(restored[i].get("containerId")) {
            repair_bound_element(&mut restored, &map, i)?;
        } else if js::truthy(restored[i].get("boundElements")) {
            repair_container_element(&mut restored, &map, i)?;
        }
        if opts.refresh_dimensions
            && is_type(&restored[i], "text")
            && !is_sticky_note_bound_text(&restored, &map, i)
        {
            let container = container_of(&restored, &map, i);
            let update = env.refresh_text_dimensions(TextDimensionsRequest {
                text: &restored[i],
                container: container.map(|c| &restored[c]),
                elements: &restored,
            });
            if let Some(update) = update {
                assign(&mut restored[i], update);
            }
        }
        if is_type(&restored[i], "arrow") || is_type(&restored[i], "line") {
            let is_arrow = is_type(&restored[i], "arrow");
            for key in ["startBinding", "endBinding"] {
                let binding = restored[i].get(key);
                if js::truthy(binding)
                    && (map.get(binding_element_id(binding)).is_none() || !is_arrow)
                {
                    restored[i].insert(key.to_owned(), Value::Null);
                }
            }
        }
    }

    restore_sticky_notes(&mut restored, &map, opts.refresh_dimensions, env)?;

    let order = repair_bound_text_element_order(&mut restored, &map, env)?;

    // the elbow arrows are fixed first, reading the elements as restored;
    // the others are then moved out rather than copied (the last time an
    // index is ordered; copied before that)
    let mut fixes = Vec::with_capacity(order.len());
    for &p in &order {
        fixes.push(if is_elbow_arrow(&restored[p]) {
            Some(fix_elbow_arrow(&restored, &map, p, env)?)
        } else {
            None
        });
    }
    let mut remaining = vec![0usize; restored.len()];
    for &p in &order {
        remaining[p] += 1;
    }
    let mut out = Vec::with_capacity(order.len());
    for (&p, fix) in order.iter().zip(fixes) {
        remaining[p] -= 1;
        out.push(match fix {
            Some(fixed) => fixed,
            None if remaining[p] == 0 => std::mem::take(&mut restored[p]),
            None => restored[p].clone(),
        });
    }
    Ok(out)
}

/// `isElbowArrow(element)`: what [`fix_elbow_arrow`] changes.
fn is_elbow_arrow(element: &Map<String, Value>) -> bool {
    is_type(element, "arrow") && js::truthy(element.get("elbowed"))
}

/// `element.type === ty`.
fn is_type(element: &Map<String, Value>, ty: &str) -> bool {
    element.get("type").and_then(Value::as_str) == Some(ty)
}

/// `Object.assign(element, update)`: existing keys keep their place.
fn assign(element: &mut Map<String, Value>, update: Map<String, Value>) {
    for (key, value) in update {
        element.insert(key, value);
    }
}

/// `binding.elementId` for a truthy binding: only an object has one.
fn binding_element_id(binding: Option<&Value>) -> Option<&Value> {
    match binding {
        Some(Value::Object(b)) => b.get("elementId"),
        _ => None,
    }
}

/// `value.id` where `value` is an item of a `boundElements` array: `null`
/// throws, a primitive or array has none.
fn item_id(item: &Value) -> Result<Option<&Value>, RestoreElementsError> {
    match item {
        Value::Null => Err(RestoreElementsError::type_error(
            "Cannot read properties of null (reading 'id')",
        )),
        Value::Object(m) => Ok(m.get("id")),
        _ => Ok(None),
    }
}

/// `bumpVersion(element, version)` (`mutateElement.ts:188-196`):
/// `(version ?? element.version) + 1`, a fresh `versionNonce`, `updated`
/// now.
fn bump_version_from(
    element: &mut Map<String, Value>,
    version: Option<&Value>,
    env: &mut dyn RestoreEnv,
) -> Result<(), RestoreElementsError> {
    let base = js::nullish_or(version, || element.get("version").cloned());
    element.insert("version".to_owned(), plus(base.as_ref(), 1.0)?);
    element.insert("versionNonce".to_owned(), js::number(env.random_integer()));
    element.insert("updated".to_owned(), js::number(env.now()));
    Ok(())
}

// -- isInvisiblySmallElement --------------------------------------------------------

/// `INVISIBLY_SMALL_ELEMENT_SIZE` (`sizeHelpers.ts:55`).
const INVISIBLY_SMALL_ELEMENT_SIZE: f64 = 0.1;

/// `isInvisiblySmallElement(element)` (`sizeHelpers.ts:61-78`) on the
/// element as given, before restore.
fn is_invisibly_small(element: &Map<String, Value>) -> Result<bool, RestoreElementsError> {
    let linear = is_type(element, "arrow") || is_type(element, "line");
    if !(linear || is_type(element, "freedraw")) {
        return Ok(js::strictly_equal(element.get("width"), Some(&json!(0)))
            && js::strictly_equal(element.get("height"), Some(&json!(0))));
    }
    let points = element.get("points");
    let length = js_length(points, "length")?;
    if js::less_than(length.as_ref(), Some(&json!(2)))? {
        return Ok(true);
    }
    if !js::strictly_equal(length.as_ref(), Some(&json!(2))) || !is_type(element, "arrow") {
        return Ok(false);
    }
    let points = points.expect("a length was read");
    let first = member(points, 0, "0")?;
    let last = member(points, 1, "1")?;
    // pointsEqual (packages/math/src/point.ts:108-115)
    let close =
        |a: &Option<Value>, b: &Option<Value>, i: usize| -> Result<bool, RestoreElementsError> {
            let (a, b) = (member_of(a.as_ref(), i)?, member_of(b.as_ref(), i)?);
            let d = js::to_number(a.as_ref())? - js::to_number(b.as_ref())?;
            Ok(d.abs() < INVISIBLY_SMALL_ELEMENT_SIZE)
        };
    Ok(close(&first, &last, 0)? && close(&first, &last, 1)?)
}

/// `value.length`: an array's or string's (UTF-16) length, an object's
/// `length` key; `null` and `undefined` throw.
fn js_length(value: Option<&Value>, what: &str) -> Result<Option<Value>, RestoreElementsError> {
    Ok(match value {
        None => {
            return Err(RestoreElementsError::type_error(format!(
                "Cannot read properties of undefined (reading '{what}')"
            )))
        }
        Some(Value::Null) => {
            return Err(RestoreElementsError::type_error(format!(
                "Cannot read properties of null (reading '{what}')"
            )))
        }
        Some(Value::Array(items)) => Some(json!(items.len())),
        Some(Value::String(s)) => Some(json!(json::to_utf16(s).len())),
        Some(Value::Object(m)) => m.get("length").cloned(),
        Some(_) => None,
    })
}

/// `value[i]` for a value that is not `null` or `undefined`.
fn member(value: &Value, i: usize, key: &str) -> Result<Option<Value>, RestoreElementsError> {
    Ok(match value {
        Value::Array(items) => items.get(i).cloned(),
        Value::String(s) => json::to_utf16(s)
            .get(i)
            .map(|&unit| Value::String(json::from_utf16(&[unit]))),
        Value::Object(m) => m.get(key).cloned(),
        _ => None,
    })
}

/// `value[i]`, throwing for `null` and `undefined`.
fn member_of(value: Option<&Value>, i: usize) -> Result<Option<Value>, RestoreElementsError> {
    let key = i.to_string();
    match value {
        None => Err(RestoreElementsError::type_error(format!(
            "Cannot read properties of undefined (reading '{key}')"
        ))),
        Some(Value::Null) => Err(RestoreElementsError::type_error(format!(
            "Cannot read properties of null (reading '{key}')"
        ))),
        Some(v) => member(v, i, &key),
    }
}

// -- fractional indices --------------------------------------------------------------

/// The index at array position `k` of `order`: `elements[k]?.index`, `None`
/// out of range or absent.
fn index_at<'a>(
    elements: &'a [Map<String, Value>],
    order: &[usize],
    k: isize,
) -> Option<&'a Value> {
    let p = usize::try_from(k).ok().and_then(|k| order.get(k))?;
    elements[*p].get("index")
}

/// `isValidFractionalIndex` (`fractionalIndex.ts:396-428`) for index values
/// of any type: set, a well-formed string, and strictly between its set
/// neighbours by JS `<`.
fn is_valid_index(
    index: Option<&Value>,
    predecessor: Option<&Value>,
    successor: Option<&Value>,
) -> Result<bool, RestoreElementsError> {
    if !js::truthy(index) {
        return Ok(false);
    }
    // validateOrderKey throws for anything but a well-formed string
    match index {
        Some(Value::String(key)) if validate_order_key(key).is_ok() => {}
        _ => return Ok(false),
    }
    Ok(match (js::truthy(predecessor), js::truthy(successor)) {
        (true, true) => js::less_than(predecessor, index)? && js::less_than(index, successor)?,
        // first element
        (false, true) => js::less_than(index, successor)?,
        // last element
        (true, false) => js::less_than(predecessor, index)?,
        // only element in the array
        (false, false) => true,
    })
}

/// `getInvalidIndicesGroups` (`fractionalIndex.ts:296-394`) over the
/// elements in `order`: runs of invalid indices, each preceded by its lower
/// bound position and followed by its upper bound position.
fn invalid_indices_groups(
    elements: &[Map<String, Value>],
    order: &[usize],
) -> Result<Vec<Vec<isize>>, RestoreElementsError> {
    let len = order.len() as isize;
    let value = |k: isize| index_at(elements, order, k);
    let mut lower_bound_index: isize = -1;
    let mut upper_bound_index: isize = 0;

    // maybe valid lower bound
    let lower_bound =
        |index: isize, lower_bound_index: isize| -> Result<isize, RestoreElementsError> {
            let lower = value(lower_bound_index);
            // iterating left to right, so no additional looping is needed
            let candidate = value(index - 1);
            let next = (!js::truthy(lower) && js::truthy(candidate))
                || (js::truthy(lower) && js::truthy(candidate) && js::less_than(lower, candidate)?);
            Ok(if next { index - 1 } else { lower_bound_index })
        };
    // always valid upper bound
    let upper_bound = |index: isize,
                       upper_bound_index: isize|
     -> Result<isize, RestoreElementsError> {
        let upper = value(upper_bound_index);
        // cache hit! don't let it find the upper bound again
        if js::truthy(upper) && index < upper_bound_index {
            return Ok(upper_bound_index);
        }
        let mut i = upper_bound_index;
        loop {
            i += 1;
            if i >= len {
                // reached the end, sky is the limit
                return Ok(i);
            }
            let candidate = value(i);
            if (!js::truthy(upper) && js::truthy(candidate))
                || (js::truthy(upper) && js::truthy(candidate) && js::less_than(upper, candidate)?)
            {
                return Ok(i);
            }
        }
    };

    let mut groups = Vec::new();
    let mut i: isize = 0;
    while i < len {
        lower_bound_index = lower_bound(i, lower_bound_index)?;
        upper_bound_index = upper_bound(i, upper_bound_index)?;
        if !is_valid_index(value(i), value(lower_bound_index), value(upper_bound_index))? {
            // the lower bound position first
            let mut group = vec![lower_bound_index, i];
            loop {
                i += 1;
                if i >= len {
                    break;
                }
                let next_lower = lower_bound(i, lower_bound_index)?;
                let next_upper = upper_bound(i, upper_bound_index)?;
                if is_valid_index(value(i), value(next_lower), value(next_upper))? {
                    break;
                }
                // assign bounds only for the moved elements
                lower_bound_index = next_lower;
                upper_bound_index = next_upper;
                group.push(i);
            }
            // the upper bound position last
            group.push(upper_bound_index);
            groups.push(group);
        } else {
            i += 1;
        }
    }
    Ok(groups)
}

/// `getMovedIndicesGroups` (`fractionalIndex.ts:261-289`): runs of moved
/// positions with their adjacent bounds.
fn moved_indices_groups(order: &[usize], moved: &HashSet<usize>) -> Vec<Vec<isize>> {
    let mut groups = Vec::new();
    let len = order.len();
    let mut i = 0;
    while i < len {
        if moved.contains(&order[i]) {
            let mut group = vec![i as isize - 1, i as isize];
            i += 1;
            while i < len && moved.contains(&order[i]) {
                group.push(i as isize);
                i += 1;
            }
            group.push(i as isize);
            groups.push(group);
        } else {
            i += 1;
        }
    }
    groups
}

/// A bound's index as `generateNKeysBetween` takes it: `null` and
/// `undefined` are open; anything else must be a valid key string, or
/// `validateOrderKey` throws.
fn bound_key(value: Option<&Value>) -> Result<Option<&str>, RestoreElementsError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(RestoreElementsError::type_error(
            "invalid order key head: undefined",
        )),
    }
}

/// `generateIndices` (`fractionalIndex.ts:430-459`): `(array position,
/// key)` for each group's inner positions, in group order.
fn generate_indices(
    elements: &[Map<String, Value>],
    order: &[usize],
    groups: Vec<Vec<isize>>,
) -> Result<Vec<(usize, String)>, RestoreElementsError> {
    let mut updates = Vec::new();
    for group in groups {
        let (Some(&lower), Some(&upper)) = (group.first(), group.last()) else {
            continue;
        };
        let inner = &group[1..group.len() - 1];
        let keys = generate_n_keys_between(
            bound_key(index_at(elements, order, lower))?,
            bound_key(index_at(elements, order, upper))?,
            inner.len(),
        )?;
        for (&k, key) in inner.iter().zip(keys) {
            updates.push((k as usize, key));
        }
    }
    Ok(updates)
}

/// `mutateElement(element, elementsMap, {index})`: nothing when the index
/// is already that string; otherwise the index set and `version + 1`, a
/// fresh `versionNonce`, `updated` now.
fn set_index(
    element: &mut Map<String, Value>,
    key: String,
    env: &mut dyn RestoreEnv,
) -> Result<(), RestoreElementsError> {
    if element.get("index").and_then(Value::as_str) == Some(key.as_str()) {
        return Ok(());
    }
    element.insert("index".to_owned(), Value::String(key));
    bump_version(element, env)?;
    Ok(())
}

/// `syncInvalidIndices` (`fractionalIndex.ts:223-235`) over the elements
/// in `order`.
fn sync_invalid_indices(
    elements: &mut [Map<String, Value>],
    order: &[usize],
    env: &mut dyn RestoreEnv,
) -> Result<(), RestoreElementsError> {
    let groups = invalid_indices_groups(elements, order)?;
    let updates = generate_indices(elements, order, groups)?;
    for (k, key) in updates {
        set_index(&mut elements[order[k]], key, env)?;
    }
    Ok(())
}

/// `syncMovedIndices` (`fractionalIndex.ts:175-216`) over the elements in
/// `order`, `moved` holding element positions: keys between the moved
/// runs' neighbours, applied only if every index is then valid; anything
/// failing on the way falls back to [`sync_invalid_indices`].
fn sync_moved_indices(
    elements: &mut [Map<String, Value>],
    order: &[usize],
    moved: &HashSet<usize>,
    env: &mut dyn RestoreEnv,
) -> Result<(), RestoreElementsError> {
    let attempt = || -> Result<Option<Vec<(usize, String)>>, RestoreElementsError> {
        let groups = moved_indices_groups(order, moved);
        // throws on invalid moved elements
        let updates = generate_indices(elements, order, groups)?;
        let mut candidates: Vec<Option<Value>> = order
            .iter()
            .map(|&p| elements[p].get("index").cloned())
            .collect();
        for (k, key) in &updates {
            candidates[*k] = Some(Value::String(key.clone()));
        }
        // ensure next indices are valid before mutation; bound text is not
        // auto-fixed, hence not validated
        let at = |k: isize| {
            usize::try_from(k)
                .ok()
                .and_then(|k| candidates.get(k))
                .and_then(Option::as_ref)
        };
        for k in 0..candidates.len() as isize {
            if !is_valid_index(at(k), at(k - 1), at(k + 1))? {
                return Ok(None);
            }
        }
        Ok(Some(updates))
    };
    match attempt() {
        Ok(Some(updates)) => {
            // split mutation so we don't end up in an inconsistent state
            for (k, key) in updates {
                set_index(&mut elements[order[k]], key, env)?;
            }
            Ok(())
        }
        // fallback to default sync
        Ok(None) | Err(_) => sync_invalid_indices(elements, order, env),
    }
}

// -- binding repair ------------------------------------------------------------------

/// `repairFrameMembership` (`restore.ts:876-887`).
fn repair_frame_membership(elements: &mut [Map<String, Value>], map: &IdMap, i: usize) {
    if map.get(elements[i].get("frameId")).is_none() {
        elements[i].insert("frameId".to_owned(), Value::Null);
    }
}

/// `getContainerElement(text, elementsMap)` (`textElement.ts:357-372`).
fn container_of(elements: &[Map<String, Value>], map: &IdMap, i: usize) -> Option<usize> {
    let id = elements[i].get("containerId");
    if js::truthy(id) {
        map.get(id)
    } else {
        None
    }
}

/// `isStickyNoteBoundText` (`stickyNote.ts:387-395`).
fn is_sticky_note_bound_text(elements: &[Map<String, Value>], map: &IdMap, i: usize) -> bool {
    container_of(elements, map, i).is_some_and(|c| is_type(&elements[c], "stickynote"))
}

/// `repairBoundElement` (`restore.ts:809-841`) for the text at `i`, whose
/// `containerId` is truthy.
fn repair_bound_element(
    elements: &mut [Map<String, Value>],
    map: &IdMap,
    i: usize,
) -> Result<(), RestoreElementsError> {
    let container = map.get(elements[i].get("containerId"));
    let angle = match container {
        Some(c) if is_type(&elements[c], "arrow") => json!(0),
        Some(c) => {
            js::nullish_or(elements[c].get("angle"), || Some(json!(0))).unwrap_or(Value::Null)
        }
        None => json!(0),
    };
    elements[i].insert("angle".to_owned(), angle);
    let Some(c) = container else {
        elements[i].insert("containerId".to_owned(), Value::Null);
        return Ok(());
    };
    if js::truthy(elements[i].get("isDeleted")) {
        return Ok(());
    }
    let text_id = elements[i].get("id").cloned();
    let bound = elements[c].get("boundElements");
    if !js::truthy(bound) {
        return Ok(());
    }
    let Some(Value::Array(bindings)) = bound else {
        return Err(RestoreElementsError::type_error(
            "container.boundElements.find is not a function",
        ));
    };
    for binding in bindings {
        if js::strictly_equal(item_id(binding)?, text_id.as_ref()) {
            return Ok(());
        }
    }
    let mut bindings = bindings.clone();
    let mut entry = Map::new();
    entry.insert("type".to_owned(), json!("text"));
    if let Some(id) = text_id {
        entry.insert("id".to_owned(), id);
    }
    bindings.push(Value::Object(entry));
    elements[c].insert("boundElements".to_owned(), Value::Array(bindings));
    Ok(())
}

/// `repairContainerElement` (`restore.ts:761-801`) for the element at `i`,
/// whose `boundElements` is truthy.
fn repair_container_element(
    elements: &mut [Map<String, Value>],
    map: &IdMap,
    i: usize,
) -> Result<(), RestoreElementsError> {
    let bindings = match elements[i].get("boundElements") {
        Some(Value::Array(bindings)) => bindings.clone(),
        // a string has slice, not reduce
        Some(Value::String(_)) => {
            return Err(RestoreElementsError::type_error(
                "boundElements.reduce is not a function",
            ))
        }
        _ => {
            return Err(RestoreElementsError::type_error(
                "container.boundElements.slice is not a function",
            ))
        }
    };
    let container_id = elements[i].get("id").cloned();
    let mut bound_ids: HashSet<MapKey> = HashSet::new();
    let mut kept = Vec::new();
    for binding in bindings {
        let id = item_id(&binding)?;
        let Some(j) = map.get(id) else {
            continue;
        };
        // found, so the id is a primitive key
        let Some(key) = MapKey::of(id) else {
            continue;
        };
        if !bound_ids.insert(key) {
            continue;
        }
        if js::truthy(elements[j].get("isDeleted")) {
            continue;
        }
        kept.push(binding.clone());
        // conservative: an existing containerId is kept, lest boundElements
        // be stale
        if is_type(&elements[j], "text") && !js::truthy(elements[j].get("containerId")) {
            let id = container_id.clone().unwrap_or(Value::Null);
            elements[j].insert("containerId".to_owned(), id);
        }
    }
    elements[i].insert("boundElements".to_owned(), Value::Array(kept));
    Ok(())
}

// -- sticky notes ----------------------------------------------------------------------

/// `isTransparent(color)` (`colors.ts:389-391`) for any value.
fn is_transparent(color: Option<&Value>) -> Result<bool, RestoreElementsError> {
    Ok(color::alpha_of_value(color)? == 0.0)
}

/// `normalizeStickyNoteStrokeColor` (`stickyNote.ts:67-73`).
fn sticky_note_stroke_color(color: Option<&Value>) -> Result<Value, RestoreElementsError> {
    if !js::truthy(color) || is_transparent(color)? {
        return Ok(json!(COLOR_BLACK));
    }
    Ok(color.cloned().unwrap_or(Value::Null))
}

/// `getBoundTextElement(element, elementsMap)` (`textElement.ts:326-355`):
/// the element named by the first `boundElements` item of type text.
fn bound_text_element(
    elements: &[Map<String, Value>],
    map: &IdMap,
    i: usize,
) -> Result<Option<usize>, RestoreElementsError> {
    let bound = elements[i].get("boundElements");
    let Some(Value::Array(bindings)) = bound else {
        // repairContainerElement has thrown for every other truthy value
        return Ok(None);
    };
    for binding in bindings {
        let ty = match binding {
            Value::Null => {
                return Err(RestoreElementsError::type_error(
                    "Cannot read properties of null (reading 'type')",
                ))
            }
            Value::Object(b) => b.get("type"),
            _ => None,
        };
        if ty.and_then(Value::as_str) == Some("text") {
            let id = item_id(binding)?;
            return Ok(if js::truthy(id) { map.get(id) } else { None });
        }
    }
    Ok(None)
}

/// `restoreStickyNotes` (`restore.ts:898-944`).
fn restore_sticky_notes(
    elements: &mut [Map<String, Value>],
    map: &IdMap,
    refresh_dimensions: bool,
    env: &mut dyn RestoreEnv,
) -> Result<(), RestoreElementsError> {
    for i in 0..elements.len() {
        if !is_type(&elements[i], "text") || js::truthy(elements[i].get("isDeleted")) {
            continue;
        }
        if is_sticky_note_bound_text(elements, map, i) {
            let container = container_of(elements, map, i);
            // one ink per note: a transparent label takes the note's colour,
            // otherwise the label wins
            let own = elements[i].get("strokeColor");
            let source = if is_transparent(own)? {
                container.and_then(|c| elements[c].get("strokeColor"))
            } else {
                own
            };
            let stroke = sticky_note_stroke_color(source)?;
            let base = js::nullish_or(elements[i].get("baseFontSize"), || {
                elements[i].get("fontSize").cloned()
            });
            let base = sticky_note_font_size(js::finite_number(base.as_ref()).unwrap_or(f64::NAN));
            elements[i].insert("baseFontSize".to_owned(), js::number(base));
            elements[i].insert("strokeColor".to_owned(), stroke.clone());
            if let Some(c) = container {
                if !js::strictly_equal(elements[c].get("strokeColor"), Some(&stroke)) {
                    elements[c].insert("strokeColor".to_owned(), stroke);
                }
            }
        } else if !js::nullish(elements[i].get("baseFontSize")) {
            elements[i].insert("baseFontSize".to_owned(), Value::Null);
        }
    }

    if refresh_dimensions {
        for i in 0..elements.len() {
            if !is_type(&elements[i], "stickynote") || js::truthy(elements[i].get("isDeleted")) {
                continue;
            }
            let text = bound_text_element(elements, map, i)?;
            let layout = env.sticky_note_layout(StickyNoteLayoutRequest {
                note: &elements[i],
                text: text.map(|t| &elements[t]),
                elements,
            });
            if let Some(layout) = layout {
                assign(&mut elements[i], layout.container);
                if let (Some(t), Some(update)) = (text, layout.text) {
                    assign(&mut elements[t], update);
                }
            }
        }
    }
    Ok(())
}

// -- bound text order ------------------------------------------------------------------

/// `normalizeBoundElementsOrder` (`sortElements.ts:57-110`) as a
/// permutation of positions: each container followed by its listed text,
/// a text listed by its container skipped where it stands. Returns the
/// original order if an element would be lost.
fn normalize_bound_elements_order(
    elements: &[Map<String, Value>],
    map: &IdMap,
) -> Result<Vec<usize>, RestoreElementsError> {
    // a Set of elements: insertion order, each once
    let mut sorted: Vec<usize> = Vec::with_capacity(elements.len());
    let mut added: HashSet<usize> = HashSet::new();
    let add = |p: usize, sorted: &mut Vec<usize>, added: &mut HashSet<usize>| {
        if added.insert(p) {
            sorted.push(p);
        }
    };
    for (p, element) in elements.iter().enumerate() {
        if added.contains(&p) {
            continue;
        }
        match element.get("boundElements") {
            Some(Value::Array(bindings)) if !bindings.is_empty() => {
                add(p, &mut sorted, &mut added);
                for binding in bindings {
                    let child = map.get(item_id(binding)?);
                    let is_text = binding.get("type").and_then(Value::as_str) == Some("text");
                    if let (Some(child), true) = (child, is_text) {
                        add(child, &mut sorted, &mut added);
                    }
                }
                continue;
            }
            // iterating a string gives characters, which have no id
            Some(Value::String(s)) if !s.is_empty() => {
                add(p, &mut sorted, &mut added);
                continue;
            }
            Some(Value::Object(m)) if js::truthy(m.get("length")) => {
                return Err(RestoreElementsError::type_error(
                    "element.boundElements is not iterable",
                ));
            }
            _ => {}
        }
        // a text its container lists is taken care of by the container
        if is_type(element, "text") && js::truthy(element.get("containerId")) {
            if let Some(c) = map.get(element.get("containerId")) {
                let listed = match elements[c].get("boundElements") {
                    None | Some(Value::Null) => false,
                    Some(Value::Array(bindings)) => {
                        let mut listed = false;
                        for binding in bindings {
                            if js::strictly_equal(item_id(binding)?, element.get("id")) {
                                listed = true;
                                break;
                            }
                        }
                        listed
                    }
                    Some(_) => {
                        return Err(RestoreElementsError::type_error(
                            "elementsMap.get(...)?.boundElements?.some is not a function",
                        ))
                    }
                };
                if listed {
                    continue;
                }
            }
        }
        add(p, &mut sorted, &mut added);
    }
    // an element would be lost: better keep the original order
    if sorted.len() != elements.len() {
        return Ok((0..elements.len()).collect());
    }
    Ok(sorted)
}

/// `repairBoundTextElementOrder` (`restore.ts:849-869`): the order with
/// bound text after its container, the moved text given indices there.
fn repair_bound_text_element_order(
    elements: &mut [Map<String, Value>],
    map: &IdMap,
    env: &mut dyn RestoreEnv,
) -> Result<Vec<usize>, RestoreElementsError> {
    let order = normalize_bound_elements_order(elements, map)?;
    // originalPositions: by id, the last of an id winning; an object id is
    // never found, so its text always counts as moved
    let moved: HashSet<usize> = order
        .iter()
        .enumerate()
        .filter(|&(k, &p)| {
            is_type(&elements[p], "text")
                && js::truthy(elements[p].get("containerId"))
                && map.get(elements[p].get("id")) != Some(k)
        })
        .map(|(_, &p)| p)
        .collect();
    if !moved.is_empty() {
        sync_moved_indices(elements, &order, &moved, env)?;
    }
    Ok(order)
}

// -- elbow arrows ------------------------------------------------------------------------

/// `DEDUP_TRESHOLD` (`elbowArrow.ts:110`), `validateElbowPoints`' default
/// tolerance.
const DEDUP_TRESHOLD: f64 = 1.0;

/// A point's coordinates as numbers.
fn point_xy(point: &Value) -> Result<(f64, f64), RestoreElementsError> {
    let coordinate = |i: usize| -> Result<f64, RestoreElementsError> {
        Ok(js::to_number(member_of(Some(point), i)?.as_ref())?)
    };
    Ok((coordinate(0)?, coordinate(1)?))
}

/// `validateElbowPoints(points)` (`elbowArrow.ts:2293-2304`): every segment
/// within 1 of horizontal or vertical.
fn valid_elbow_points(points: &[Value]) -> Result<bool, RestoreElementsError> {
    for pair in points.windows(2) {
        let (a, b) = (point_xy(&pair[0])?, point_xy(&pair[1])?);
        if !((b.0 - a.0).abs() < DEDUP_TRESHOLD || (b.1 - a.1).abs() < DEDUP_TRESHOLD) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The elbow arrow fix-ups (`restore.ts:1072-1137`) for the element at `p`.
fn fix_elbow_arrow(
    elements: &[Map<String, Value>],
    map: &IdMap,
    p: usize,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreElementsError> {
    let element = &elements[p];
    if !is_elbow_arrow(element) {
        return Ok(element.clone());
    }
    // restoreElement always gives an arrow an array of points
    let points: &[Value] = match element.get("points") {
        Some(Value::Array(points)) => points,
        _ => &[],
    };
    let start = element.get("startBinding");
    let end = element.get("endBinding");

    // invalid, unbound: re-routed from [0, 0] to the last point
    if !js::truthy(start) && !js::truthy(end) && !valid_elbow_points(points)? {
        let last = points.last().cloned().unwrap_or(Value::Null);
        let update = env.update_elbow_arrow_points(ElbowArrowRequest {
            arrow: element,
            points: &[json!([0, 0]), last],
            elements,
        });
        let mut next = element.clone();
        if let Some(update) = update {
            assign(&mut next, update);
            match element.get("index") {
                Some(index) => next.insert("index".to_owned(), index.clone()),
                None => next.shift_remove("index"),
            };
        }
        return Ok(next);
    }

    // self-bound with runaway points: reset around the target
    let start_id = binding_element_id(start);
    if js::truthy(start)
        && js::truthy(end)
        && js::strictly_equal(start_id, binding_element_id(end))
        && points.len() > 1
    {
        let mut runaway = false;
        for point in points {
            let (x, y) = point_xy(point)?;
            if x.abs() > 1e6 || y.abs() > 1e6 {
                runaway = true;
                break;
            }
        }
        if runaway {
            return match map.get(start_id) {
                Some(b) => self_bound_reset(element, &elements[b]),
                None => Ok(element.clone()),
            };
        }
    }
    Ok(element.clone())
}

/// `restore.ts:1119-1134`: the arrow at the top centre of its target,
/// looping out and back into its right side.
fn self_bound_reset(
    element: &Map<String, Value>,
    target: &Map<String, Value>,
) -> Result<Map<String, Value>, RestoreElementsError> {
    let width = js::to_number(target.get("width"))?;
    let height = js::to_number(target.get("height"))?;
    let x = plus(target.get("x"), width / 2.0)?;
    let y = js::to_number(target.get("y"))? - 5.0;
    let mut next = element.clone();
    next.insert("x".to_owned(), x);
    next.insert("y".to_owned(), js::number(y));
    for key in ["width", "height"] {
        match target.get(key) {
            Some(v) => next.insert(key.to_owned(), v.clone()),
            None => next.shift_remove(key),
        };
    }
    let (w, h) = (width / 2.0 + 5.0, height / 2.0 + 5.0);
    next.insert(
        "points".to_owned(),
        Value::Array(vec![
            json!([0, 0]),
            json!([0, -10]),
            Value::Array(vec![js::number(w), json!(-10)]),
            Value::Array(vec![js::number(w), js::number(h)]),
        ]),
    );
    Ok(next)
}

// -- bumpElementVersions -------------------------------------------------------------------

fn bump_versions(
    mut elements: Elements,
    local: Option<&[Map<String, Value>]>,
    env: &mut dyn RestoreEnv,
) -> Result<Elements, RestoreElementsError> {
    let Some(local) = local else {
        return Ok(elements);
    };
    let local = ElementsMap::from_encoded(local.to_vec());
    for element in &mut elements {
        let Some(local) = local.get_encoded(element.get("id")) else {
            continue;
        };
        let (local_version, version) = (local.get("version"), element.get("version"));
        // same versions but a different versionNonce: different edits
        let newer = js::less_than(version, local_version)?
            || (js::strictly_equal(local_version, version)
                && !js::strictly_equal(local.get("versionNonce"), element.get("versionNonce")));
        if newer {
            let local_version = local_version.cloned();
            bump_version_from(element, local_version.as_ref(), env)?;
        }
    }
    Ok(elements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore::TestEnv;

    fn obj(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => panic!("not an object"),
        }
    }

    /// A self-bound elbow arrow with a point beyond 1e6 is reset around
    /// its target (`restore.ts:1095-1135`). `restoreElements` cannot reach
    /// this with a restored arrow (the 75,000 px cap of `restoreElement`
    /// replaces it first), so the step is checked on its own.
    #[test]
    fn self_bound_elbow_arrow_is_reset() {
        let target = obj(
            json!({ "id": "r", "type": "rectangle", "x": 100, "y": 200, "width": 80, "height": 60 }),
        );
        let arrow = obj(json!({
            "id": "e",
            "type": "arrow",
            "elbowed": true,
            "x": 0,
            "y": 0,
            "width": 50,
            "height": 2000000,
            "index": "a1",
            "points": [[0, 0], [0, -2000000], [50, -2000000], [50, 0]],
            "startBinding": { "elementId": "r", "fixedPoint": [0.5001, 0], "mode": "orbit" },
            "endBinding": { "elementId": "r", "fixedPoint": [1, 0.5001], "mode": "orbit" }
        }));
        let elements = vec![target, arrow];
        let map = IdMap::new(&elements);
        let fixed = fix_elbow_arrow(&elements, &map, 1, &mut TestEnv::default()).expect("fixes");
        assert_eq!(fixed["x"], 140);
        assert_eq!(fixed["y"], 195);
        assert_eq!(fixed["width"], 80);
        assert_eq!(fixed["height"], 60);
        assert_eq!(
            fixed["points"],
            json!([[0, 0], [0, -10], [45, -10], [45, 35]])
        );
        assert_eq!(fixed["index"], "a1");

        // the target missing: kept
        let lone = vec![elements[1].clone()];
        let map = IdMap::new(&lone);
        let kept = fix_elbow_arrow(&lone, &map, 0, &mut TestEnv::default()).expect("keeps");
        assert_eq!(kept, lone[0]);

        // bound to two elements: kept
        let mut two = elements[1].clone();
        two["endBinding"]["elementId"] = json!("s");
        let both = vec![elements[0].clone(), two];
        let map = IdMap::new(&both);
        let kept = fix_elbow_arrow(&both, &map, 1, &mut TestEnv::default()).expect("keeps");
        assert_eq!(kept, both[1]);
    }

    #[test]
    fn elbow_points_validation() {
        let valid = |points: Value| valid_elbow_points(points.as_array().unwrap()).unwrap();
        assert!(valid(json!([[0, 0], [50, 0], [50, 50]])));
        assert!(valid(json!([[0, 0], [0.9, 30]])));
        assert!(!valid(json!([[0, 0], [1, 30]])));
        assert!(valid(json!([[0, 0]])));
    }

    #[test]
    fn invisibly_small() {
        let small = |v: Value| is_invisibly_small(&obj(v)).unwrap();
        assert!(small(
            json!({ "type": "rectangle", "width": 0, "height": 0 })
        ));
        assert!(!small(
            json!({ "type": "rectangle", "width": 0, "height": "0" })
        ));
        assert!(small(json!({ "type": "line", "points": [[0, 0]] })));
        assert!(!small(
            json!({ "type": "line", "points": [[0, 0], [0, 0]] })
        ));
        assert!(small(
            json!({ "type": "arrow", "points": [[0, 0], [0.09, -0.09]] })
        ));
        assert!(!small(
            json!({ "type": "arrow", "points": [[0, 0], [0.1, 0]] })
        ));
        assert!(!small(
            json!({ "type": "arrow", "points": [[0, 0], [0, 0], [0, 0]] })
        ));
        assert!(small(json!({ "type": "freedraw", "points": [] })));
        assert!(!small(json!({ "type": "freedraw", "points": 5 })));
        assert_eq!(
            is_invisibly_small(&obj(json!({ "type": "arrow", "points": [null, [0, 0]] })))
                .unwrap_err()
                .to_string(),
            "Cannot read properties of null (reading '0')"
        );
    }
}
