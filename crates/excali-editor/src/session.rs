//! The scene, app state, store and history wired as upstream's `App`
//! wires them (`packages/excalidraw/components/App.tsx`).
//!
//! - [`Session::sync_action_result`] is `syncActionResult` (`App.tsx:3160`):
//!   schedule the action's capture, replace the elements, merge the app
//!   state, then commit.
//! - [`Session::update_scene`] is `updateScene` (`App.tsx:5425-5475`): with
//!   a capture action, the update is compared with the current state right
//!   away (a micro action), then applied and committed.
//! - [`Session::undo`] and [`Session::redo`] are the undo and redo actions
//!   (`actions/actionHistory.tsx`): ignored while an interaction is in
//!   progress; otherwise history's result, ordered by fractional index, is
//!   applied without capture.
//! - [`Session::initialize_scene`] resets the store and history and loads
//!   a scene without capture (`initializeScene`, `App.tsx:3725-3732`).
//!
//! Each of these ends with the commit upstream runs in
//! `componentDidUpdate` (`App.tsx:4508`); durable increments go to
//! [`History::record`] and every increment is returned to the caller.

use std::fmt;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_core::fractional_index::{
    order_by_fractional_index, sync_invalid_indices, SceneElementsMap,
};
use excali_core::order_key::OrderKeyError;
use serde_json::{Map, Value};

use crate::history::{History, HistoryError};
use crate::js_value::truthy;
use crate::store::{
    CaptureUpdateAction, DynStamp, HistoryEnv, ObservedAppState, Store, StoreError, StoreIncrement,
};

/// `ActionResult`: what an action gives back to the app.
#[derive(Debug, Clone)]
pub struct ActionResult {
    /// The next elements, all of them, in order (deleted ones included).
    pub elements: Option<Vec<Element>>,
    /// App state keys to set.
    pub app_state: Option<Map<String, Value>>,
    pub capture_update: CaptureUpdateAction,
}

/// Why a session operation failed where upstream throws.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionError {
    /// Syncing the fractional indices of the new elements failed.
    Indices(OrderKeyError),
    /// The store could not schedule the update.
    Store(StoreError),
    /// A history entry could not be applied.
    History(HistoryError),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::Indices(e) => write!(f, "{e}"),
            SessionError::Store(e) => write!(f, "{e}"),
            SessionError::History(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// The editor's scene with its store and history.
pub struct Session<E: HistoryEnv> {
    pub env: E,
    pub store: Store,
    pub history: History,
    elements: Vec<Element>,
    app_state: AppState,
}

impl<E: HistoryEnv> fmt::Debug for Session<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("elements", &self.elements.len())
            .field("store", &self.store)
            .field("history", &self.history)
            .finish()
    }
}

/// The app state keys that mean an interaction is in progress, during
/// which undo and redo do nothing (`actionHistory.tsx:30-41`). Upstream
/// also waits for a flowchart being created and a pending draw-shape
/// gesture, which are editor state outside the app state; the tools that
/// have them add them to this check.
const INTERACTION_KEYS: [&str; 6] = [
    "multiElement",
    "resizingElement",
    "editingTextElement",
    "newElement",
    "selectedElementsAreBeingDragged",
    "selectionElement",
];

impl<E: HistoryEnv> Session<E> {
    /// An empty scene with the given app state, the store's snapshot empty.
    pub fn new(env: E, app_state: AppState) -> Session<E> {
        Session {
            env,
            store: Store::new(),
            history: History::new(),
            elements: Vec::new(),
            app_state,
        }
    }

    /// The scene's elements (`scene.getElementsIncludingDeleted()`), in
    /// order.
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    /// The app state.
    pub fn app_state(&self) -> &AppState {
        &self.app_state
    }

    /// `scene.getElementsMapIncludingDeleted()`.
    pub fn elements_map(&self) -> SceneElementsMap {
        self.elements
            .iter()
            .map(|e| (e.base.id.clone(), e.clone()))
            .collect()
    }

    /// The initial scene load: store and history reset, then the elements
    /// and app state applied without capture.
    pub fn initialize_scene(
        &mut self,
        elements: Vec<Element>,
        app_state: Map<String, Value>,
    ) -> Result<Vec<StoreIncrement>, SessionError> {
        self.store.clear();
        self.history.clear();
        self.sync_action_result(ActionResult {
            elements: Some(elements),
            app_state: Some(app_state),
            capture_update: CaptureUpdateAction::Never,
        })
    }

    /// `scene.replaceAllElements(elements)`: the elements with invalid
    /// fractional indices synced (bumping their versions).
    fn replace_all_elements(&mut self, mut elements: Vec<Element>) -> Result<(), SessionError> {
        sync_invalid_indices(&mut elements, &mut DynStamp(&mut self.env))
            .map_err(SessionError::Indices)?;
        self.elements = elements;
        Ok(())
    }

    /// `setState(appState)`: the keys merged into the app state.
    fn set_state(&mut self, app_state: Map<String, Value>) {
        for (key, value) in app_state {
            self.app_state.insert(key, value);
        }
    }

    /// The commit of `componentDidUpdate`: durable increments recorded in
    /// history, all increments returned.
    pub fn commit(&mut self) -> Vec<StoreIncrement> {
        let elements = self.elements_map();
        let observed = ObservedAppState::from_app_state(&self.app_state);
        let increments = self
            .store
            .commit(Some(&elements), Some(&observed), &mut self.env);
        for increment in &increments {
            if let StoreIncrement::Durable { delta, .. } = increment {
                self.history.record(delta, &mut self.env);
            }
        }
        increments
    }

    /// `syncActionResult(actionResult)`.
    pub fn sync_action_result(
        &mut self,
        result: ActionResult,
    ) -> Result<Vec<StoreIncrement>, SessionError> {
        self.store.schedule_action(result.capture_update);
        if let Some(elements) = result.elements {
            self.replace_all_elements(elements)?;
        }
        if let Some(app_state) = result.app_state {
            self.set_state(app_state);
        }
        Ok(self.commit())
    }

    /// `updateScene({ elements, appState, captureUpdate })`.
    pub fn update_scene(
        &mut self,
        elements: Option<Vec<Element>>,
        app_state: Option<Map<String, Value>>,
        capture_update: Option<CaptureUpdateAction>,
    ) -> Result<Vec<StoreIncrement>, SessionError> {
        if let Some(action) = capture_update {
            let observed = app_state.as_ref().map(|patch| {
                // getObservedAppState({...store.snapshot.appState, ...appState})
                let mut merged = self.store.snapshot().app_state.as_map().clone();
                for (key, value) in patch {
                    merged.insert(key.clone(), value.clone());
                }
                ObservedAppState::from_map(&merged)
            });
            let current_elements = self.elements_map();
            let current_app_state = ObservedAppState::from_app_state(&self.app_state);
            self.store
                .schedule_micro_action(
                    action,
                    elements.as_deref(),
                    observed.as_ref(),
                    &current_elements,
                    &current_app_state,
                    &mut self.env,
                )
                .map_err(SessionError::Store)?;
        }
        if let Some(app_state) = app_state {
            self.set_state(app_state);
        }
        if let Some(elements) = elements {
            self.replace_all_elements(elements)?;
        }
        Ok(self.commit())
    }

    fn interaction_in_progress(&self) -> bool {
        INTERACTION_KEYS
            .iter()
            .any(|key| truthy(self.app_state.get(key)))
    }

    /// The undo action (`createUndoAction`).
    pub fn undo(&mut self) -> Result<Vec<StoreIncrement>, SessionError> {
        self.history_action(true)
    }

    /// The redo action (`createRedoAction`).
    pub fn redo(&mut self) -> Result<Vec<StoreIncrement>, SessionError> {
        self.history_action(false)
    }

    /// `executeHistoryAction(app, appState, updater)`.
    fn history_action(&mut self, undo: bool) -> Result<Vec<StoreIncrement>, SessionError> {
        let nothing = ActionResult {
            elements: None,
            app_state: None,
            capture_update: CaptureUpdateAction::Eventually,
        };
        if self.interaction_in_progress() {
            return self.sync_action_result(nothing);
        }
        let elements = self.elements_map();
        let app_state = self.app_state.clone();
        let result = if undo {
            self.history
                .undo(&mut self.store, elements, app_state, &mut self.env)
        } else {
            self.history
                .redo(&mut self.store, elements, app_state, &mut self.env)
        }
        .map_err(SessionError::History)?;
        let Some((elements, app_state)) = result else {
            return self.sync_action_result(nothing);
        };
        // order by fractional indices in case the map was modified meanwhile
        let mut elements: Vec<Element> = elements.into_values().collect();
        order_by_fractional_index(&mut elements);
        self.sync_action_result(ActionResult {
            elements: Some(elements),
            app_state: Some(app_state.into_map()),
            capture_update: CaptureUpdateAction::Never,
        })
    }
}
