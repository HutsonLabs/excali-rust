//! The store: snapshots of the scene and the observed app state, and the
//! increments that tell history and collaborators what changed
//! (`packages/element/src/store.ts` at the pinned commit).
//!
//! The editor keeps one [`StoreSnapshot`], the last captured state. On
//! every commit (upstream's `componentDidUpdate`) the store compares the
//! current elements and app state with it:
//!
//! - an update captured [`CaptureUpdateAction::Immediately`] replaces the
//!   snapshot and emits a durable increment carrying a [`StoreDelta`]
//!   (the elements and app state deltas), which history records;
//! - [`CaptureUpdateAction::Never`] (remote updates, scene
//!   initialisation) replaces the snapshot and emits an ephemeral increment
//!   only, so it is never undoable;
//! - [`CaptureUpdateAction::Eventually`] (the default) leaves the snapshot
//!   alone, so the change is captured with the next immediate update.
//!
//! Increments are returned from [`Store::commit`] instead of going through
//! upstream's emitters; [`crate::session::Session`] hands the durable ones
//! to [`crate::history::History::record`], as `App` subscribes history to
//! `onDurableIncrementEmitter` (`App.tsx:3872-3874`).
//!
//! Snapshot elements are shared between snapshots through [`Rc`], so the
//! reference checks upstream relies on (`getChangedElements`, "due to the
//! structural clone inside `maybeClone`") are pointer comparisons here.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use excali_core::app_state::AppState;
use excali_core::constants::COLOR_WHITE;
use excali_core::element::{Element, ElementKind};
use excali_core::fractional_index::{
    sync_invalid_indices_immutable, ChangeStamp, SceneElementsMap,
};
use excali_core::library::{hash_elements_version_of, hash_string};
use excali_core::order_key::OrderKeyError;
use excali_text::text_measurements::{CharWidthCache, TextMetricsProvider};
use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use crate::binding::BindingEnv;
use crate::delta::{AppStateDelta, ApplyToOptions, Delta, DeltaError, ElementsDelta};
use crate::js_value::truthy;
use crate::scene::MutationEnv;

/// `CaptureUpdateAction` (`store.ts:38-69`): whether and when an update
/// becomes undoable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureUpdateAction {
    /// Immediately undoable: most local updates.
    Immediately,
    /// Never undoable: remote updates and scene initialisation.
    Never,
    /// Captured with a later immediate update: parts of a multi-step
    /// process, and every update that names no action.
    Eventually,
}

/// What the store and history draw from the outside world, and the leaf
/// layout that runs after a delta is applied.
///
/// - The [`ChangeStamp`] supertrait: `randomInteger()` for version nonces
///   and `getUpdatedTimestamp()` for `updated`.
/// - [`HistoryEnv::random_id`], `randomId()` for store delta ids.
/// - [`HistoryEnv::redraw_text_bounding_box`] and
///   [`HistoryEnv::update_bound_elements`], the two layout calls of
///   `ElementsDelta.redrawElements` (`delta.ts:2034-2124`). Which elements
///   they run on is decided by [`crate::delta::redraw_elements`]; the
///   layout itself is text layout's
///   ([`crate::text_layout::TextLayouter::redraw_text_bounding_box`]) and
///   arrow binding's (ex-510), and the host wires it in (the environment
///   holds its `TextLayouter`, whose original container heights text
///   editing shares). Every element a leaf changes must go through
///   [`crate::mutate::mutate_element`], which bumps its version; history
///   checks that only elements the delta reaches were changed.
pub trait HistoryEnv: ChangeStamp {
    /// `randomId()`.
    fn random_id(&mut self) -> String;

    /// `redrawTextBoundingBox(textElement, container, scene)`
    /// (`textElement.ts:51`): re-wrap and re-measure the text `text_id`
    /// bound to `container_id` and place it in the container, growing the
    /// container when the text no longer fits. `elements` is the whole
    /// scene being built (deleted elements included); both elements are
    /// in it and not deleted. An error is what upstream's layout throws.
    fn redraw_text_bounding_box(
        &mut self,
        elements: &mut SceneElementsMap,
        text_id: &str,
        container_id: &str,
    ) -> Result<(), String>;

    /// The text metrics and the per-font character width cache an arrow's
    /// label is re-wrapped with when the arrow is laid out again
    /// (`textMeasurements.ts`'s provider and `charWidth`).
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache);

    /// `updateBoundElements(changedElement, scene, { changedElements })`
    /// (`binding.ts:1321`): re-route the arrows bound to the bindable
    /// element `element_id` (in `elements`, not deleted), reading the
    /// elements of `changed` in place of the scene's. An error is what
    /// upstream's layout throws. The default is the port's
    /// [`crate::binding::update_bound_elements_in_map`], which does not
    /// fail.
    fn update_bound_elements(
        &mut self,
        elements: &mut SceneElementsMap,
        element_id: &str,
        changed: &SceneElementsMap,
    ) -> Result<(), String> {
        crate::binding::update_bound_elements_in_map(
            elements,
            element_id,
            changed,
            &mut HistoryBindingEnv(self),
        );
        Ok(())
    }

    /// `isTestEnv() || isDevEnv()`: whether applying a delta fails on an
    /// error (a delta that cannot be applied, a layout error, layout
    /// touching an element the delta does not reach) instead of carrying
    /// on as upstream's production build does (`delta.ts:1431-1443`,
    /// `1500-1552`, `2034-2057`). Debug builds check, release builds do not.
    fn dev_checks(&self) -> bool {
        cfg!(debug_assertions)
    }
}

/// A [`HistoryEnv`] as the environment of binding's layout: its version
/// nonces and timestamps, and its text metrics.
struct HistoryBindingEnv<'a, E: HistoryEnv + ?Sized>(&'a mut E);

impl<E: HistoryEnv + ?Sized> MutationEnv for HistoryBindingEnv<'_, E> {
    fn random_integer(&mut self) -> f64 {
        self.0.version_nonce()
    }

    fn now(&mut self) -> f64 {
        self.0.updated()
    }
}

impl<E: HistoryEnv + ?Sized> BindingEnv for HistoryBindingEnv<'_, E> {
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache) {
        self.0.text()
    }
}

/// A [`ChangeStamp`] over a borrowed trait object, for the generic
/// fractional index functions.
pub(crate) struct DynStamp<'a>(pub &'a mut dyn ChangeStamp);

impl ChangeStamp for DynStamp<'_> {
    fn version_nonce(&mut self) -> f64 {
        self.0.version_nonce()
    }

    fn updated(&mut self) -> f64 {
        self.0.updated()
    }
}

// ---------------------------------------------------------------------------
// Observed app state

/// `ObservedAppState` (`packages/excalidraw/types.ts`): the part of the app
/// state history tracks, as `getObservedAppState` (`store.ts:998-1025`)
/// picks it.
///
/// `selectedLinearElement` is reduced to `{elementId, isEditing}`; when
/// history restores it, the app state gets that object (upstream builds a
/// `LinearElementEditor` from it, whose other fields are the linear
/// editor's interaction state).
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedAppState {
    map: Map<String, Value>,
}

/// The observed keys that are about elements (`ObservedElementsAppState`).
pub(crate) const ELEMENTS_KEYS: [&str; 7] = [
    "editingGroupId",
    "selectedGroupIds",
    "selectedElementIds",
    "selectedLinearElement",
    "croppingElementId",
    "lockedMultiSelections",
    "activeLockedId",
];

/// The observed keys that stand alone (`ObservedStandaloneAppState`).
pub(crate) const STANDALONE_KEYS: [&str; 2] = ["name", "viewBackgroundColor"];

impl ObservedAppState {
    /// `getObservedAppState(appState)` of an app state (or of an observed
    /// one, which gives it back): the observed keys in upstream's order, a
    /// missing key as `null`.
    pub fn from_map(app_state: &Map<String, Value>) -> ObservedAppState {
        let get = |key: &str| app_state.get(key).cloned().unwrap_or(Value::Null);
        let mut map = Map::new();
        for key in [
            "name",
            "editingGroupId",
            "viewBackgroundColor",
            "selectedElementIds",
            "selectedGroupIds",
            "croppingElementId",
            "activeLockedId",
            "lockedMultiSelections",
        ] {
            map.insert(key.into(), get(key));
        }
        let linear = app_state.get("selectedLinearElement");
        let linear = if truthy(linear) {
            let linear = linear.unwrap_or(&Value::Null);
            json!({
                "elementId": linear.get("elementId").cloned().unwrap_or(Value::Null),
                "isEditing": truthy(linear.get("isEditing")),
            })
        } else {
            Value::Null
        };
        map.insert("selectedLinearElement".into(), linear);
        ObservedAppState { map }
    }

    /// `getObservedAppState(appState)`.
    pub fn from_app_state(app_state: &AppState) -> ObservedAppState {
        ObservedAppState::from_map(app_state.as_map())
    }

    /// `getDefaultObservedAppState()` (`store.ts:984-996`).
    pub fn default_observed() -> ObservedAppState {
        ObservedAppState {
            map: match json!({
                "name": null,
                "editingGroupId": null,
                "viewBackgroundColor": COLOR_WHITE,
                "selectedElementIds": {},
                "selectedGroupIds": {},
                "selectedLinearElement": null,
                "croppingElementId": null,
                "activeLockedId": null,
                "lockedMultiSelections": {},
            }) {
                Value::Object(map) => map,
                _ => Map::new(),
            },
        }
    }

    /// The observed state as a JSON object.
    pub fn as_map(&self) -> &Map<String, Value> {
        &self.map
    }

    /// The value of `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key)
    }
}

// ---------------------------------------------------------------------------
// Snapshot

/// A snapshot's elements, by id in scene order; each element shared with
/// the snapshots it did not change in.
pub type SnapshotElements = IndexMap<String, Rc<Element>>;

/// A snapshot's `metadata`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SnapshotMetadata {
    pub did_elements_change: bool,
    pub did_app_state_change: bool,
    pub is_empty: bool,
}

/// `StoreSnapshot` (`store.ts:643-979`): the captured elements (deleted
/// ones included) and observed app state.
#[derive(Debug)]
pub struct StoreSnapshot {
    pub elements: Rc<SnapshotElements>,
    pub app_state: Rc<ObservedAppState>,
    pub metadata: SnapshotMetadata,
    last_changed_elements_hash: Cell<u32>,
    last_changed_app_state_hash: Cell<u32>,
}

/// The next elements a commit compares with the snapshot: lent by the
/// caller, or the caller's own copy, which the snapshot moves the changed
/// ones out of.
enum NextElements<'a> {
    Lent(&'a SceneElementsMap),
    Owned(SceneElementsMap),
}

impl NextElements<'_> {
    fn map(&self) -> &SceneElementsMap {
        match self {
            NextElements::Lent(map) => map,
            NextElements::Owned(map) => map,
        }
    }
}

/// `isImageElement(element) && !isInitializedImageElement(element)`.
fn is_uninitialized_image(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Image(image) if image.file_id.as_ref().is_none_or(|f| f.0.is_empty()))
}

/// `newElementWith(element, { isDeleted: true })`: the element itself when
/// already deleted, otherwise a deleted copy with the version bumped.
pub(crate) fn deleted_copy(element: &Element, stamp: &mut dyn ChangeStamp) -> Element {
    let mut copy = element.clone();
    if !element.base.is_deleted {
        copy.base.is_deleted = true;
        copy.base.version += 1.0;
        copy.base.version_nonce = stamp.version_nonce();
        copy.base.updated = stamp.updated();
    }
    copy
}

impl StoreSnapshot {
    fn new(
        elements: Rc<SnapshotElements>,
        app_state: Rc<ObservedAppState>,
        metadata: SnapshotMetadata,
    ) -> StoreSnapshot {
        StoreSnapshot {
            elements,
            app_state,
            metadata,
            last_changed_elements_hash: Cell::new(0),
            last_changed_app_state_hash: Cell::new(0),
        }
    }

    /// `StoreSnapshot.create(elements, appState)`: the elements as they
    /// are, no change flagged.
    pub fn create(elements: &SceneElementsMap, app_state: &ObservedAppState) -> StoreSnapshot {
        StoreSnapshot::new(
            Rc::new(
                elements
                    .iter()
                    .map(|(id, e)| (id.clone(), Rc::new(e.clone())))
                    .collect(),
            ),
            Rc::new(app_state.clone()),
            SnapshotMetadata::default(),
        )
    }

    /// `StoreSnapshot.empty()`: no elements, the default observed app
    /// state.
    pub fn empty() -> StoreSnapshot {
        StoreSnapshot::new(
            Rc::new(SnapshotElements::new()),
            Rc::new(ObservedAppState::default_observed()),
            SnapshotMetadata {
                is_empty: true,
                ..SnapshotMetadata::default()
            },
        )
    }

    /// `isEmpty()`: whether this is the empty snapshot.
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty
    }

    /// `getChangedElements(prevSnapshot)`: the elements of `prev` missing
    /// here, marked deleted, and every element that is not the same
    /// instance as in `prev`.
    pub fn get_changed_elements(
        &self,
        prev: &StoreSnapshot,
        stamp: &mut dyn ChangeStamp,
    ) -> IndexMap<String, Rc<Element>> {
        let mut changed = IndexMap::new();
        for (id, element) in prev.elements.iter() {
            if !self.elements.contains_key(id) {
                changed.insert(id.clone(), Rc::new(deleted_copy(element, stamp)));
            }
        }
        for (id, element) in self.elements.iter() {
            let same = prev
                .elements
                .get(id)
                .is_some_and(|p| Rc::ptr_eq(p, element));
            if !same {
                changed.insert(id.clone(), Rc::clone(element));
            }
        }
        changed
    }

    /// `getChangedAppState(prevSnapshot)`: the observed keys whose value
    /// differs from `prev`, with this snapshot's values.
    pub fn get_changed_app_state(&self, prev: &StoreSnapshot) -> Map<String, Value> {
        Delta::get_right_differences(prev.app_state.as_map(), self.app_state.as_map())
            .into_iter()
            .filter_map(|key| self.app_state.get(&key).cloned().map(|value| (key, value)))
            .collect()
    }

    /// `applyChange(change)`: a new snapshot with the change's elements set
    /// and its app state keys merged; each part flagged as changed when
    /// the change has any.
    pub fn apply_change(&self, change: &StoreChange) -> StoreSnapshot {
        let mut elements = (*self.elements).clone();
        for (id, element) in &change.elements {
            elements.insert(id.clone(), Rc::clone(element));
        }
        let mut app_state = self.app_state.as_map().clone();
        for (key, value) in &change.app_state {
            app_state.insert(key.clone(), value.clone());
        }
        StoreSnapshot::new(
            Rc::new(elements),
            Rc::new(ObservedAppState::from_map(&app_state)),
            SnapshotMetadata {
                did_elements_change: !change.elements.is_empty(),
                did_app_state_change: !change.app_state.is_empty(),
                is_empty: false,
            },
        )
    }

    /// `maybeClone(action, elements, appState)`: this snapshot when
    /// nothing changed, otherwise a new one sharing the unchanged elements
    /// and holding copies of the changed ones.
    ///
    /// An element changed when it is new, has a higher version, or is
    /// missing from `elements` (then it is kept, marked deleted); an image
    /// without a file is ignored until it has one. For an `Eventually`
    /// update, a change equal to the last one detected (same versions, same
    /// app state) is not reported again.
    pub fn maybe_clone(
        self: &Rc<Self>,
        action: CaptureUpdateAction,
        elements: Option<&SceneElementsMap>,
        app_state: Option<&ObservedAppState>,
        stamp: &mut dyn ChangeStamp,
    ) -> Rc<StoreSnapshot> {
        self.maybe_clone_next(action, elements.map(NextElements::Lent), app_state, stamp)
    }

    fn maybe_clone_next(
        self: &Rc<Self>,
        action: CaptureUpdateAction,
        elements: Option<NextElements<'_>>,
        app_state: Option<&ObservedAppState>,
        stamp: &mut dyn ChangeStamp,
    ) -> Rc<StoreSnapshot> {
        let compare_hashes = action == CaptureUpdateAction::Eventually;
        let next_elements = self.maybe_create_elements_snapshot(elements, compare_hashes, stamp);
        let next_app_state = self.maybe_create_app_state_snapshot(app_state, compare_hashes);
        let did_elements_change = !Rc::ptr_eq(&self.elements, &next_elements);
        let did_app_state_change = !Rc::ptr_eq(&self.app_state, &next_app_state);
        if !did_elements_change && !did_app_state_change {
            return Rc::clone(self);
        }
        Rc::new(StoreSnapshot::new(
            next_elements,
            next_app_state,
            SnapshotMetadata {
                did_elements_change,
                did_app_state_change,
                is_empty: false,
            },
        ))
    }

    /// Whether `next` (the scene's elements in order; for a repeated id the
    /// last counts, as in the map the scene would pass) holds a change
    /// [`StoreSnapshot::maybe_clone`] would detect: an element of the
    /// snapshot missing, or one new or at a higher version that is not an
    /// image without a file. Decided on the elements as they are, without
    /// the copy into a map a commit would make.
    pub fn has_element_changes(&self, next: &[Element]) -> bool {
        let mut last: HashMap<&str, &Element> = HashMap::with_capacity(next.len());
        for element in next {
            last.insert(element.base.id.as_str(), element);
        }
        if self
            .elements
            .keys()
            .any(|id| !last.contains_key(id.as_str()))
        {
            return true;
        }
        last.values().any(|element| {
            let updated = match self.elements.get(element.base.id.as_str()) {
                None => true,
                Some(prev) => prev.base.version < element.base.version,
            };
            updated && !is_uninitialized_image(element)
        })
    }

    fn maybe_create_app_state_snapshot(
        &self,
        app_state: Option<&ObservedAppState>,
        compare_hashes: bool,
    ) -> Rc<ObservedAppState> {
        let Some(app_state) = app_state else {
            return Rc::clone(&self.app_state);
        };
        if !Delta::is_right_different(self.app_state.as_map(), app_state.as_map()) {
            return Rc::clone(&self.app_state);
        }
        let hash = hash_string(&Value::Object(app_state.as_map().clone()).to_string());
        if compare_hashes && self.last_changed_app_state_hash.get() == hash {
            return Rc::clone(&self.app_state);
        }
        self.last_changed_app_state_hash.set(hash);
        Rc::new(app_state.clone())
    }

    fn maybe_create_elements_snapshot(
        &self,
        elements: Option<NextElements<'_>>,
        compare_hashes: bool,
        stamp: &mut dyn ChangeStamp,
    ) -> Rc<SnapshotElements> {
        let Some(elements) = elements else {
            return Rc::clone(&self.elements);
        };
        let Some(changed) = self.detect_changed_elements(elements, compare_hashes, stamp) else {
            return Rc::clone(&self.elements);
        };
        // clone previous elements, never delete, in case the next elements
        // are just a subset of the previous ones
        let mut cloned = (*self.elements).clone();
        for (id, element) in changed {
            cloned.insert(id, Rc::new(element));
        }
        Rc::new(cloned)
    }

    fn detect_changed_elements(
        &self,
        next: NextElements<'_>,
        compare_hashes: bool,
        stamp: &mut dyn ChangeStamp,
    ) -> Option<SceneElementsMap> {
        let mut changed = SceneElementsMap::new();
        for (id, prev) in self.elements.iter() {
            if !next.map().contains_key(id) {
                // element was deleted
                changed.insert(id.clone(), deleted_copy(prev, stamp));
            }
        }
        let updated = |id: &str, element: &Element| {
            let updated = match self.elements.get(id) {
                None => true,
                Some(prev) => prev.base.version < element.base.version,
            };
            // ignore any updates on uninitialized image elements
            updated && !is_uninitialized_image(element)
        };
        match next {
            NextElements::Lent(next) => {
                for (id, element) in next {
                    if updated(id, element) {
                        changed.insert(id.clone(), element.clone());
                    }
                }
            }
            // the scene's own copy: the changed elements are moved
            NextElements::Owned(next) => {
                for (id, element) in next {
                    if updated(&id, &element) {
                        changed.insert(id, element);
                    }
                }
            }
        }
        if changed.is_empty() {
            return None;
        }
        let hash = hash_elements_version_of(changed.values());
        if compare_hashes && self.last_changed_elements_hash.get() == hash {
            return None;
        }
        self.last_changed_elements_hash.set(hash);
        Some(changed)
    }
}

// ---------------------------------------------------------------------------
// Change, delta, increments

/// `StoreChange` (`store.ts:432-452`): the elements that changed between
/// two snapshots and the observed app state keys that did.
#[derive(Debug, Clone, Default)]
pub struct StoreChange {
    pub elements: IndexMap<String, Rc<Element>>,
    pub app_state: Map<String, Value>,
}

impl StoreChange {
    /// `StoreChange.create(prevSnapshot, nextSnapshot)`.
    pub fn create(
        prev: &StoreSnapshot,
        next: &StoreSnapshot,
        stamp: &mut dyn ChangeStamp,
    ) -> StoreChange {
        StoreChange {
            elements: next.get_changed_elements(prev, stamp),
            app_state: next.get_changed_app_state(prev),
        }
    }
}

/// `StoreDelta` (`store.ts:497-634`): an elements delta and an app state
/// delta under an id.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreDelta {
    pub id: String,
    pub elements: ElementsDelta,
    pub app_state: AppStateDelta,
    /// Whether the delta is a history entry (upstream's `HistoryDelta`
    /// subclass), which history does not record again.
    pub(crate) history_entry: bool,
}

impl StoreDelta {
    /// `StoreDelta.create(elements, appState, { id })`.
    pub fn create(
        elements: ElementsDelta,
        app_state: AppStateDelta,
        id: impl Into<String>,
    ) -> StoreDelta {
        StoreDelta {
            id: id.into(),
            elements,
            app_state,
            history_entry: false,
        }
    }

    fn with_random_id(
        elements: ElementsDelta,
        app_state: AppStateDelta,
        history_entry: bool,
        env: &mut dyn HistoryEnv,
    ) -> StoreDelta {
        StoreDelta {
            id: env.random_id(),
            elements,
            app_state,
            history_entry,
        }
    }

    /// Whether this delta is a history entry.
    pub fn is_history_entry(&self) -> bool {
        self.history_entry
    }

    /// `StoreDelta.calculate(prevSnapshot, nextSnapshot)`: the deltas of
    /// the parts the next snapshot flags as changed.
    pub fn calculate(
        prev: &StoreSnapshot,
        next: &StoreSnapshot,
        env: &mut dyn HistoryEnv,
    ) -> StoreDelta {
        let elements = if next.metadata.did_elements_change {
            ElementsDelta::calculate(prev.elements.as_ref(), next.elements.as_ref(), env)
        } else {
            ElementsDelta::empty()
        };
        let app_state = if next.metadata.did_app_state_change {
            AppStateDelta::calculate(&prev.app_state, &next.app_state)
        } else {
            AppStateDelta::empty()
        };
        StoreDelta::with_random_id(elements, app_state, false, env)
    }

    /// `StoreDelta.squash(...deltas)`: the deltas squashed in order into a
    /// new empty delta.
    pub fn squash(deltas: &[StoreDelta], env: &mut dyn HistoryEnv) -> StoreDelta {
        let mut aggregated =
            StoreDelta::with_random_id(ElementsDelta::empty(), AppStateDelta::empty(), false, env);
        for delta in deltas {
            aggregated.elements.squash(&delta.elements);
            aggregated.app_state.squash(&delta.app_state);
        }
        aggregated
    }

    /// `StoreDelta.inverse(delta)`: a new delta, with a new id, undoing
    /// this one.
    pub fn inverse(&self, env: &mut dyn HistoryEnv) -> StoreDelta {
        StoreDelta::with_random_id(
            self.elements.inverse(),
            self.app_state.inverse(),
            self.history_entry,
            env,
        )
    }

    /// `StoreDelta.applyTo(delta, elements, appState, options)`: the
    /// elements and app state with the delta applied (an element missing
    /// from `elements` is created from the partial), and whether that is a
    /// visible change.
    pub fn apply_to(
        &mut self,
        elements: &SceneElementsMap,
        app_state: &AppState,
        options: &ApplyToOptions,
        env: &mut dyn HistoryEnv,
    ) -> Result<(SceneElementsMap, AppState, bool), DeltaError> {
        let (next_elements, elements_visible) =
            self.elements
                .apply_to(elements, &SnapshotElements::new(), options, env)?;
        let (next_app_state, app_state_visible) =
            self.app_state.apply_to(app_state, &next_elements);
        Ok((
            next_elements,
            next_app_state,
            elements_visible || app_state_visible,
        ))
    }

    /// `StoreDelta.applyLatestChanges(delta, prevElements, nextElements,
    /// modifierOptions)`: a delta with the same id whose element partials
    /// hold the current values of their keys.
    pub fn apply_latest_changes<P, N>(
        &self,
        prev: &IndexMap<String, P>,
        next: &IndexMap<String, N>,
        modifier_options: Option<crate::delta::Side>,
    ) -> StoreDelta
    where
        P: std::borrow::Borrow<Element>,
        N: std::borrow::Borrow<Element>,
    {
        StoreDelta {
            id: self.id.clone(),
            elements: self
                .elements
                .apply_latest_changes(prev, next, modifier_options),
            app_state: self.app_state.clone(),
            history_entry: self.history_entry,
        }
    }

    /// `StoreDelta.empty()`.
    pub fn empty(env: &mut dyn HistoryEnv) -> StoreDelta {
        StoreDelta::with_random_id(ElementsDelta::empty(), AppStateDelta::empty(), false, env)
    }

    /// `isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.app_state.is_empty()
    }

    /// The delta as upstream's `DTO<StoreDelta>` serialises: `{id,
    /// elements: {added, removed, updated}, appState: {delta: {deleted,
    /// inserted}}}` (an `undefined` value is left out, as `JSON.stringify`
    /// leaves it).
    pub fn to_dto(&self) -> Value {
        json!({
            "id": self.id,
            "elements": self.elements.to_dto(),
            "appState": self.app_state.to_dto(),
        })
    }

    /// `StoreDelta.load(dto)` / `StoreDelta.restore(dto)`: a delta from its
    /// DTO; `None` when the object does not have that shape.
    pub fn from_dto(dto: &Value) -> Option<StoreDelta> {
        let id = dto.get("id")?.as_str()?.to_owned();
        let elements = ElementsDelta::from_dto(dto.get("elements")?)?;
        let app_state = AppStateDelta::from_dto(dto.get("appState")?)?;
        Some(StoreDelta {
            id,
            elements,
            app_state,
            history_entry: false,
        })
    }
}

/// `StoreIncrement` (`store.ts:454-495`): what a commit emits.
#[derive(Debug, Clone)]
pub enum StoreIncrement {
    /// `DurableIncrement`: a captured change, with its delta.
    Durable {
        change: StoreChange,
        delta: Box<StoreDelta>,
    },
    /// `EphemeralIncrement`: a change that is not captured.
    Ephemeral { change: StoreChange },
}

/// Why a store operation failed where upstream throws.
#[derive(Debug, Clone, PartialEq)]
pub enum StoreError {
    /// Syncing the scheduled elements' fractional indices failed.
    Indices(OrderKeyError),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Indices(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

struct MicroAction {
    action: CaptureUpdateAction,
    change: StoreChange,
    delta: Option<StoreDelta>,
}

/// `Store` (`store.ts:78-418`).
pub struct Store {
    snapshot: Rc<StoreSnapshot>,
    scheduled_macro_actions: HashSet<CaptureUpdateAction>,
    scheduled_micro_actions: Vec<MicroAction>,
    /// Whether anything listens for every increment (upstream's
    /// `onIncrement` prop). Without a listener, an `Eventually` update is
    /// not even compared (`store.ts:326-333`); durable increments are
    /// always produced for history.
    pub observe_increments: bool,
}

impl Default for Store {
    fn default() -> Store {
        Store::new()
    }
}

impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Store")
            .field("snapshot", &self.snapshot)
            .field("scheduled_macro_actions", &self.scheduled_macro_actions)
            .field(
                "scheduled_micro_actions",
                &self.scheduled_micro_actions.len(),
            )
            .field("observe_increments", &self.observe_increments)
            .finish()
    }
}

impl Store {
    /// A store with the empty snapshot.
    pub fn new() -> Store {
        Store {
            snapshot: Rc::new(StoreSnapshot::empty()),
            scheduled_macro_actions: HashSet::new(),
            scheduled_micro_actions: Vec::new(),
            observe_increments: false,
        }
    }

    /// The last captured snapshot.
    pub fn snapshot(&self) -> &Rc<StoreSnapshot> {
        &self.snapshot
    }

    /// Replace the snapshot.
    pub fn set_snapshot(&mut self, snapshot: Rc<StoreSnapshot>) {
        self.snapshot = snapshot;
    }

    /// `scheduleAction(action)`: the macro action of the next commit; the
    /// strongest scheduled one wins (`Immediately`, then `Never`).
    pub fn schedule_action(&mut self, action: CaptureUpdateAction) {
        self.scheduled_macro_actions.insert(action);
    }

    /// `scheduleCapture()`: [`Store::schedule_action`] with `Immediately`.
    pub fn schedule_capture(&mut self) {
        self.schedule_action(CaptureUpdateAction::Immediately);
    }

    /// `scheduleMicroAction({ action, elements, appState })`: compare the
    /// given elements (their invalid indices synced on copies) and observed
    /// app state with the current scene now, and process that change first
    /// on the next commit.
    pub fn schedule_micro_action(
        &mut self,
        action: CaptureUpdateAction,
        elements: Option<&[Element]>,
        app_state: Option<&ObservedAppState>,
        current_elements: &SceneElementsMap,
        current_app_state: &ObservedAppState,
        stamp: &mut dyn ChangeStamp,
    ) -> Result<(), StoreError> {
        let current = Rc::new(StoreSnapshot::create(current_elements, current_app_state));
        let synced = match elements {
            Some(elements) => Some(
                sync_invalid_indices_immutable(elements, &mut DynStamp(stamp))
                    .map_err(StoreError::Indices)?,
            ),
            None => None,
        };
        let scheduled = current.maybe_clone(action, synced.as_ref(), app_state, stamp);
        let change = StoreChange::create(&current, &scheduled, stamp);
        self.schedule_micro_change(action, change, None);
        Ok(())
    }

    /// `scheduleMicroAction({ action, change, delta })`: process a
    /// computed change (and, for `Immediately`, its delta) first on the
    /// next commit.
    pub fn schedule_micro_change(
        &mut self,
        action: CaptureUpdateAction,
        change: StoreChange,
        delta: Option<StoreDelta>,
    ) {
        self.scheduled_micro_actions.push(MicroAction {
            action,
            change,
            delta,
        });
    }

    /// `commit(elements, appState)`: process the scheduled micro actions,
    /// then the scheduled macro action against the given state, and return
    /// the increments emitted.
    pub fn commit(
        &mut self,
        elements: Option<&SceneElementsMap>,
        app_state: Option<&ObservedAppState>,
        env: &mut dyn HistoryEnv,
    ) -> Vec<StoreIncrement> {
        self.commit_next(elements.map(NextElements::Lent), app_state, env)
    }

    /// [`Store::commit`] of the scene's own copy of its elements, which the
    /// snapshot takes the changed ones from instead of copying them.
    pub fn commit_owned(
        &mut self,
        elements: Option<SceneElementsMap>,
        app_state: Option<&ObservedAppState>,
        env: &mut dyn HistoryEnv,
    ) -> Vec<StoreIncrement> {
        self.commit_next(elements.map(NextElements::Owned), app_state, env)
    }

    fn commit_next(
        &mut self,
        elements: Option<NextElements<'_>>,
        app_state: Option<&ObservedAppState>,
        env: &mut dyn HistoryEnv,
    ) -> Vec<StoreIncrement> {
        let mut increments = Vec::new();
        for micro in std::mem::take(&mut self.scheduled_micro_actions) {
            self.process_change(
                micro.action,
                &micro.change,
                micro.delta,
                env,
                &mut increments,
            );
        }
        let action = self.scheduled_macro_action();
        self.scheduled_macro_actions.clear();
        if action == CaptureUpdateAction::Eventually && !self.observe_increments {
            return increments;
        }
        if elements.is_none() && app_state.is_none() {
            return increments;
        }
        let next = self
            .snapshot
            .maybe_clone_next(action, elements, app_state, env);
        if Rc::ptr_eq(&next, &self.snapshot) {
            return increments;
        }
        self.emit(action, next, None, None, env, &mut increments);
        increments
    }

    /// `clear()`: the empty snapshot and no scheduled macro action.
    pub fn clear(&mut self) {
        self.snapshot = Rc::new(StoreSnapshot::empty());
        self.scheduled_macro_actions.clear();
    }

    fn scheduled_macro_action(&self) -> CaptureUpdateAction {
        if self
            .scheduled_macro_actions
            .contains(&CaptureUpdateAction::Immediately)
        {
            CaptureUpdateAction::Immediately
        } else if self
            .scheduled_macro_actions
            .contains(&CaptureUpdateAction::Never)
        {
            CaptureUpdateAction::Never
        } else {
            CaptureUpdateAction::Eventually
        }
    }

    fn process_change(
        &mut self,
        action: CaptureUpdateAction,
        change: &StoreChange,
        delta: Option<StoreDelta>,
        env: &mut dyn HistoryEnv,
        increments: &mut Vec<StoreIncrement>,
    ) {
        if action == CaptureUpdateAction::Eventually && !self.observe_increments {
            return;
        }
        let next = Rc::new(self.snapshot.apply_change(change));
        self.emit(action, next, Some(change), delta, env, increments);
    }

    fn emit(
        &mut self,
        action: CaptureUpdateAction,
        next: Rc<StoreSnapshot>,
        change: Option<&StoreChange>,
        delta: Option<StoreDelta>,
        env: &mut dyn HistoryEnv,
        increments: &mut Vec<StoreIncrement>,
    ) {
        let change = match change {
            Some(change) => change.clone(),
            None => StoreChange::create(&self.snapshot, &next, env),
        };
        match action {
            CaptureUpdateAction::Immediately => {
                // the delta may be given (a history entry being applied),
                // then it is emitted as it is
                let delta = match delta {
                    Some(delta) => delta,
                    None => StoreDelta::calculate(&self.snapshot, &next, env),
                };
                if !delta.is_empty() {
                    increments.push(StoreIncrement::Durable {
                        change,
                        delta: Box::new(delta),
                    });
                }
            }
            CaptureUpdateAction::Never | CaptureUpdateAction::Eventually => {
                increments.push(StoreIncrement::Ephemeral { change });
            }
        }
        // `Immediately` and `Never` update the snapshot, `Eventually` does not
        if action != CaptureUpdateAction::Eventually {
            self.snapshot = next;
        }
    }
}
