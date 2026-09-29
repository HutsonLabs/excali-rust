//! Undo and redo stacks (`packages/excalidraw/history.ts` at the pinned
//! commit).
//!
//! History records every durable store increment as its inverse on the
//! undo stack. Undo pops an entry, applies it to the current elements and
//! app state, and pushes its inverse on the redo stack; redo does the
//! reverse. Entries are applied without their `version` and `versionNonce`,
//! so an undo is a new version of each element it touches, as any other
//! user action is for collaborators.
//!
//! Before an entry is pushed back it is brought up to date with the
//! elements as they are now (`applyLatestChanges`), so a remote change made
//! since the entry was recorded is what the opposite action restores. An
//! entry whose application changes nothing visible (a selection of
//! elements deleted remotely, say) is not a step of its own: history keeps
//! popping until something visible changes or the stack is empty.
//!
//! Upstream's `onHistoryChangedEmitter` is the event queue of
//! [`History::take_events`]: one [`HistoryChangedEvent`] per record, undo
//! or redo.

use std::fmt;
use std::ops::{Deref, DerefMut};

use excali_core::app_state::AppState;
use excali_core::fractional_index::SceneElementsMap;

use crate::delta::{ApplyToOptions, DeltaError};
use crate::store::{
    CaptureUpdateAction, HistoryEnv, ObservedAppState, Store, StoreChange, StoreDelta,
    StoreSnapshot,
};

/// `HistoryDelta` (`history.ts:15-86`): a store delta on the undo or redo
/// stack. Scheduled back into the store when applied, it is emitted but
/// not recorded again.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryDelta(StoreDelta);

impl From<StoreDelta> for HistoryDelta {
    fn from(mut delta: StoreDelta) -> HistoryDelta {
        delta.history_entry = true;
        HistoryDelta(delta)
    }
}

impl Deref for HistoryDelta {
    type Target = StoreDelta;

    fn deref(&self) -> &StoreDelta {
        &self.0
    }
}

impl DerefMut for HistoryDelta {
    fn deref_mut(&mut self) -> &mut StoreDelta {
        &mut self.0
    }
}

impl HistoryDelta {
    /// The store delta.
    pub fn into_inner(self) -> StoreDelta {
        self.0
    }

    /// `HistoryDelta.inverse(delta)`.
    pub fn inverse(&self, env: &mut dyn HistoryEnv) -> HistoryDelta {
        HistoryDelta::from(self.0.inverse(env))
    }

    /// `applyTo(elements, appState, snapshot)`: the entry applied without
    /// `version` and `versionNonce`, falling back to the snapshot for
    /// elements missing from the scene; and whether that changed anything
    /// visible.
    pub fn apply_to(
        &mut self,
        elements: &SceneElementsMap,
        app_state: &AppState,
        snapshot: &StoreSnapshot,
        env: &mut dyn HistoryEnv,
    ) -> Result<(SceneElementsMap, AppState, bool), DeltaError> {
        let options = ApplyToOptions {
            excluded_properties: ["version", "versionNonce"]
                .into_iter()
                .map(String::from)
                .collect(),
        };
        let (next_elements, elements_visible) =
            self.0
                .elements
                .apply_to(elements, &snapshot.elements, &options, env)?;
        let (next_app_state, app_state_visible) =
            self.0.app_state.apply_to(app_state, &next_elements);
        Ok((
            next_elements,
            next_app_state,
            elements_visible || app_state_visible,
        ))
    }
}

/// `HistoryChangedEvent` (`history.ts:88-93`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryChangedEvent {
    pub is_undo_stack_empty: bool,
    pub is_redo_stack_empty: bool,
}

/// An entry could not be applied. It has been moved to the other stack
/// anyway, so the user is not stuck on it.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryError(pub DeltaError);

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for HistoryError {}

/// `History` (`history.ts:95-249`).
#[derive(Debug, Default)]
pub struct History {
    pub undo_stack: Vec<HistoryDelta>,
    pub redo_stack: Vec<HistoryDelta>,
    events: Vec<HistoryChangedEvent>,
}

impl History {
    /// Empty stacks.
    pub fn new() -> History {
        History::default()
    }

    /// `isUndoStackEmpty`.
    pub fn is_undo_stack_empty(&self) -> bool {
        self.undo_stack.is_empty()
    }

    /// `isRedoStackEmpty`.
    pub fn is_redo_stack_empty(&self) -> bool {
        self.redo_stack.is_empty()
    }

    /// `clear()`: both stacks emptied.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// The history change events since the last call.
    pub fn take_events(&mut self) -> Vec<HistoryChangedEvent> {
        std::mem::take(&mut self.events)
    }

    fn changed(&mut self) {
        self.events.push(HistoryChangedEvent {
            is_undo_stack_empty: self.is_undo_stack_empty(),
            is_redo_stack_empty: self.is_redo_stack_empty(),
        });
    }

    /// `record(delta)`: push the inverse of a non-empty local durable
    /// delta on the undo stack. A change to the elements clears the redo
    /// stack; an app state change alone does not (a click that deselects
    /// would otherwise lose every redo). History entries are not recorded
    /// again.
    pub fn record(&mut self, delta: &StoreDelta, env: &mut dyn HistoryEnv) {
        if delta.is_empty() || delta.is_history_entry() {
            return;
        }
        let entry = HistoryDelta::from(delta.inverse(env));
        let clears_redo = !entry.elements.is_empty();
        self.undo_stack.push(entry);
        if clears_redo {
            self.redo_stack.clear();
        }
        self.changed();
    }

    /// `undo(elements, appState)`: the elements and app state with the
    /// undo entries applied until one makes a visible change, or `None`
    /// when the undo stack is empty. The applied entries are scheduled in
    /// `store` as immediate changes.
    pub fn undo(
        &mut self,
        store: &mut Store,
        elements: SceneElementsMap,
        app_state: AppState,
        env: &mut dyn HistoryEnv,
    ) -> Result<Option<(SceneElementsMap, AppState)>, HistoryError> {
        let result = self.perform(store, elements, app_state, true, env);
        self.changed();
        result
    }

    /// `redo(elements, appState)`: as [`History::undo`], from the redo
    /// stack.
    pub fn redo(
        &mut self,
        store: &mut Store,
        elements: SceneElementsMap,
        app_state: AppState,
        env: &mut dyn HistoryEnv,
    ) -> Result<Option<(SceneElementsMap, AppState)>, HistoryError> {
        let result = self.perform(store, elements, app_state, false, env);
        self.changed();
        result
    }

    fn pop(&mut self, undo: bool) -> Option<HistoryDelta> {
        if undo {
            self.undo_stack.pop()
        } else {
            self.redo_stack.pop()
        }
    }

    fn push(&mut self, undo: bool, entry: &HistoryDelta, env: &mut dyn HistoryEnv) {
        let inverse = entry.inverse(env);
        if undo {
            self.redo_stack.push(inverse);
        } else {
            self.undo_stack.push(inverse);
        }
    }

    fn perform(
        &mut self,
        store: &mut Store,
        elements: SceneElementsMap,
        app_state: AppState,
        undo: bool,
        env: &mut dyn HistoryEnv,
    ) -> Result<Option<(SceneElementsMap, AppState)>, HistoryError> {
        let Some(mut entry) = self.pop(undo) else {
            return Ok(None);
        };
        let action = CaptureUpdateAction::Immediately;
        let mut prev_snapshot = std::rc::Rc::clone(store.snapshot());
        let mut next_elements = elements;
        let mut next_app_state = app_state;

        // iterate through the entries in case they make no visible change
        loop {
            let applied = entry.apply_to(&next_elements, &next_app_state, &prev_snapshot, env);
            let (elements, app_state, visible) = match applied {
                Ok(applied) => applied,
                Err(e) => {
                    self.push(undo, &entry, env);
                    return Err(HistoryError(e));
                }
            };
            next_elements = elements;
            next_app_state = app_state;

            let observed = ObservedAppState::from_app_state(&next_app_state);
            let next_snapshot =
                prev_snapshot.maybe_clone(action, Some(&next_elements), Some(&observed), env);
            let change = StoreChange::create(&prev_snapshot, &next_snapshot, env);
            let delta = HistoryDelta::from(entry.apply_latest_changes(
                prev_snapshot.elements.as_ref(),
                &next_elements,
                None,
            ));
            if !delta.is_empty() {
                // scheduled, so that it is emitted for sync purposes
                store.schedule_micro_change(action, change, Some(delta.0.clone()));
                entry = delta;
            }
            prev_snapshot = next_snapshot;
            self.push(undo, &entry, env);

            if visible {
                break;
            }
            match self.pop(undo) {
                Some(next) => entry = next,
                None => break,
            }
        }
        Ok(Some((next_elements, next_app_state)))
    }
}
