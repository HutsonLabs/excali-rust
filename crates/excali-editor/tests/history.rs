//! Undo and redo, ported from `packages/excalidraw/tests/history.test.tsx`
//! at the pinned commit.
//!
//! Upstream drives the React app (`render(<Excalidraw />)`, `UI.*`,
//! `mouse.*`, `Keyboard.undo()`); the editor here has no DOM, so each step
//! is replayed through [`Session`] as the app performs it: an `API.*` call
//! is the same call (`updateScene`, `setElements`), and a pointer or key
//! gesture is the action result the app commits for it (a new rectangle
//! added and selected, a selection change, a background colour change on
//! the selection, a deletion), captured `IMMEDIATELY` as upstream's
//! handlers schedule it. `Keyboard.undo()` / `redo()` are the undo / redo
//! actions (`actions/actionHistory.tsx`). Every assertion on the undo and
//! redo stacks, the scene and the selection is upstream's.

mod support;

use excali_core::app_state::AppState;
use excali_core::element::{
    BindMode, BoundElement, BoundElementType, Element, ElementKind, FixedPointBinding,
    FractionalIndex,
};
use excali_core::fractional_index::sync_moved_indices;
use excali_editor::delta::{AppStateDelta, Delta, ElementsDelta};
use excali_editor::history::HistoryDelta;
use excali_editor::session::{ActionResult, Session};
use excali_editor::store::{CaptureUpdateAction, StoreDelta};
use indexmap::IndexMap;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use support::{arrow, frame, obj, prop, rect, rect_sized, text, with, TestEnv};

const TRANSPARENT: &str = "transparent";
// COLOR_PALETTE.<name>[DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX]
const RED: &str = "#ffc9c9";
const BLUE: &str = "#a5d8ff";
const YELLOW: &str = "#ffec99";
const VIOLET: &str = "#eebefa";

use CaptureUpdateAction::{Immediately, Never};

/// The rendered app: `render(<Excalidraw />)` with optional `initialData`.
struct App {
    s: Session<TestEnv>,
}

impl App {
    fn new() -> App {
        App::with_initial_data(vec![], json!({}))
    }

    fn with_initial_data(elements: Vec<Element>, app_state: Value) -> App {
        let mut s = Session::new(TestEnv::default(), AppState::default());
        s.initialize_scene(elements, obj(app_state))
            .expect("initializeScene");
        App { s }
    }

    fn env(&mut self) -> &mut TestEnv {
        &mut self.s.env
    }

    /// `h.elements`
    fn elements(&self) -> Vec<Element> {
        self.s.elements().to_vec()
    }

    fn ids(&self) -> Vec<String> {
        self.s
            .elements()
            .iter()
            .map(|e| e.base.id.clone())
            .collect()
    }

    fn get(&self, id: &str) -> Element {
        self.s
            .elements()
            .iter()
            .find(|e| e.base.id == id)
            .cloned()
            .unwrap_or_else(|| panic!("no element {id}"))
    }

    fn at(&self, i: usize) -> Element {
        self.s.elements()[i].clone()
    }

    fn state(&self, key: &str) -> Value {
        self.s.app_state().get(key).cloned().unwrap_or(Value::Null)
    }

    fn undo_len(&self) -> usize {
        self.s.history.undo_stack.len()
    }

    fn redo_len(&self) -> usize {
        self.s.history.redo_stack.len()
    }

    fn stacks(&self) -> (usize, usize) {
        (self.undo_len(), self.redo_len())
    }

    /// `API.getSelectedElements()`: non-deleted scene elements whose id is
    /// selected, in scene order.
    fn selected(&self) -> Vec<String> {
        let ids = self.state("selectedElementIds");
        self.s
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted && ids.get(&e.base.id) == Some(&json!(true)))
            .map(|e| e.base.id.clone())
            .collect()
    }

    /// `assertSelectedElements(...)`: the same set of ids.
    fn assert_selected(&self, ids: &[&str]) {
        let mut actual = self.selected();
        actual.sort();
        let mut expected: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(actual, expected, "selected elements");
    }

    /// `Keyboard.undo()`
    fn undo(&mut self) {
        self.s.undo().expect("undo");
    }

    /// `Keyboard.redo()`
    fn redo(&mut self) {
        self.s.redo().expect("redo");
    }

    /// `API.updateScene({ elements, appState, captureUpdate })`
    fn update_scene(
        &mut self,
        elements: Option<Vec<Element>>,
        app_state: Option<Value>,
        capture: Option<CaptureUpdateAction>,
    ) {
        self.s
            .update_scene(elements, app_state.map(obj), capture)
            .expect("updateScene");
    }

    /// `API.setElements(elements)` (`h.elements = elements`): the scene
    /// replaced, nothing captured.
    fn set_elements(&mut self, elements: Vec<Element>) {
        self.s
            .sync_action_result(ActionResult {
                elements: Some(elements),
                app_state: None,
                capture_update: CaptureUpdateAction::Eventually,
            })
            .expect("setElements");
    }

    /// An action result captured immediately.
    fn act(&mut self, elements: Option<Vec<Element>>, app_state: Option<Value>) {
        self.s
            .sync_action_result(ActionResult {
                elements,
                app_state: app_state.map(obj),
                capture_update: Immediately,
            })
            .expect("action");
    }

    /// `UI.createElement("rectangle", { x, y })`: the rectangle is drawn,
    /// added and selected; on pointer up the app captures it.
    fn create_rect(&mut self, id: &str, x: f64, y: f64) {
        let mut elements = self.elements();
        elements.push(rect_sized(id, x, y, 10.0, 10.0));
        self.act(
            Some(elements),
            Some(json!({"selectedElementIds": {id: true}, "selectedGroupIds": {}})),
        );
    }

    /// A click on the canvas away from every element (`mouse.clickAt(-10, -10)`).
    fn click_empty(&mut self) {
        self.act(
            None,
            Some(json!({
                "selectedElementIds": {},
                "selectedGroupIds": {},
                "editingGroupId": null,
                "selectedLinearElement": null,
            })),
        );
    }

    /// The selection set to `ids` (a click, a box selection or a
    /// shift-click adding to it).
    fn set_selection(&mut self, ids: &[&str]) {
        let selected: Map<String, Value> =
            ids.iter().map(|id| (id.to_string(), json!(true))).collect();
        self.act(
            None,
            Some(json!({"selectedElementIds": selected, "selectedGroupIds": {}})),
        );
    }

    /// `mouse.select([a, b, ...])`: a click on the first, then a
    /// shift-click on each other one.
    fn select(&mut self, ids: &[&str]) {
        for i in 0..ids.len() {
            self.set_selection(&ids[..=i]);
        }
    }

    /// Changing an element property of the selection from the properties
    /// panel (`changeProperty`, `actions/actionProperties.tsx`).
    fn change_selected(&mut self, updates: Value) {
        let selected: HashSet<String> = self.selected().into_iter().collect();
        let elements: Vec<Element> = self
            .elements()
            .iter()
            .map(|e| {
                if selected.contains(&e.base.id) {
                    with(e, updates.clone(), self.env())
                } else {
                    e.clone()
                }
            })
            .collect();
        self.act(Some(elements), None);
    }

    /// `UI.clickOnTestId("color-...")` in the background picker.
    fn background(&mut self, color: &str) {
        self.change_selected(json!({"backgroundColor": color}));
    }

    /// `Keyboard.keyDown(KEYS.DELETE)` (`actionDeleteSelected`): the
    /// selection deleted and deselected.
    fn delete_selected(&mut self) {
        let selected: HashSet<String> = self.selected().into_iter().collect();
        let elements: Vec<Element> = self
            .elements()
            .iter()
            .map(|e| {
                if selected.contains(&e.base.id) {
                    with(e, json!({"isDeleted": true}), self.env())
                } else {
                    e.clone()
                }
            })
            .collect();
        self.act(Some(elements), Some(json!({"selectedElementIds": {}})));
    }

    /// `newElementWith(h.elements[i], updates)`
    fn with(&mut self, element: &Element, updates: Value) -> Element {
        with(element, updates, self.env())
    }

    /// The scene reordered so that `order` is the new array order, with
    /// the moved elements' indices synced (`zindex.ts`, then
    /// `syncMovedIndices`), captured as the z-index actions do.
    fn reorder(&mut self, order: &[&str], moved: &[&str]) {
        let mut elements: Vec<Element> = order.iter().map(|id| self.get(id)).collect();
        let moved: HashSet<String> = moved.iter().map(|s| s.to_string()).collect();
        sync_moved_indices(&mut elements, &moved, self.env()).unwrap();
        self.act(Some(elements), None);
    }
}

fn index(element: &Element) -> String {
    element.base.index.clone().map(|i| i.0).unwrap_or_default()
}

fn bound_elements(element: &Element) -> Value {
    prop(element, "boundElements")
}

fn container_id(element: &Element) -> Value {
    prop(element, "containerId")
}

fn binding(element_id: &str, fixed_point: [f64; 2]) -> FixedPointBinding {
    FixedPointBinding {
        element_id: element_id.into(),
        fixed_point,
        mode: BindMode::Orbit,
    }
}

fn bound(id: &str, kind: BoundElementType) -> BoundElement {
    BoundElement {
        id: id.into(),
        kind,
    }
}

// ===========================================================================
// singleplayer undo/redo

#[test]
fn should_not_collapse_when_applying_corrupted_history_entry() {
    let mut app = App::new();
    let r = rect("rect", 0.0, 0.0);
    app.set_elements(vec![r]);

    // an entry which cannot be applied, in either direction
    let mut updated = IndexMap::new();
    let corrupt = |version: i64| -> IndexMap<String, Option<Value>> {
        [
            ("type".to_string(), Some(json!("not-an-element-type"))),
            ("version".to_string(), Some(json!(version))),
        ]
        .into_iter()
        .collect()
    };
    updated.insert("rect".to_string(), Delta::new(corrupt(1), corrupt(2)));
    let corrupted = HistoryDelta::from(StoreDelta::create(
        ElementsDelta::create(IndexMap::new(), IndexMap::new(), updated, false),
        AppStateDelta::empty(),
        "corrupted",
    ));
    app.s.history.undo_stack.push(corrupted);

    assert!(app.s.undo().is_err());
    // popped even though it is corrupted, so the user is not stuck on it
    assert_eq!(app.undo_len(), 0);
    // pushed, as it might be valid on a subsequent redo
    assert_eq!(app.redo_len(), 1);
    assert!(!app.get("rect").base.is_deleted);

    assert!(app.s.redo().is_err());
    assert_eq!(app.stacks(), (1, 0));
    assert!(!app.get("rect").base.is_deleted);
}

#[test]
fn should_not_end_up_with_history_entry_when_there_are_no_elements_changes() {
    let mut app = App::new();
    let r1 = rect("rect1", 0.0, 0.0);
    let r2 = rect("rect2", 0.0, 0.0);
    app.update_scene(Some(vec![r1.clone(), r2.clone()]), None, Some(Immediately));
    assert_eq!(app.stacks(), (1, 0));
    assert_eq!(app.ids(), ["rect1", "rect2"]);

    // even though the flag is on, same elements are passed, nothing to commit
    let same = app.elements();
    app.update_scene(Some(same), None, Some(Immediately));
    assert_eq!(app.stacks(), (1, 0));
    assert!(app.elements().iter().all(|e| !e.base.is_deleted));
}

#[test]
fn should_not_modify_anything_on_unrelated_appstate_change() {
    let r = rect("rect", 0.0, 0.0);
    let mut app = App::with_initial_data(vec![r], json!({}));
    app.update_scene(None, Some(json!({"viewModeEnabled": true})), Some(Never));
    assert_eq!(app.state("viewModeEnabled"), json!(true));
    assert_eq!(app.stacks(), (0, 0));
    assert!(!app.get("rect").base.is_deleted);
    let snapshot = app.s.store.snapshot();
    assert!(!snapshot.elements["rect"].base.is_deleted);
}

#[test]
fn should_not_clear_the_redo_stack_on_standalone_appstate_change() {
    let mut app = App::new();
    app.create_rect("rect1", 10.0, 10.0);
    app.create_rect("rect2", 20.0, 20.0);
    assert_eq!(app.stacks(), (2, 0));
    app.assert_selected(&["rect2"]);

    app.undo();
    assert_eq!(app.stacks(), (1, 1));
    app.assert_selected(&["rect1"]);
    assert!(!app.get("rect1").base.is_deleted);
    assert!(app.get("rect2").base.is_deleted);

    app.click_empty();
    assert_eq!(app.stacks(), (2, 1)); // we still have a possibility to redo!
    assert!(app.selected().is_empty());

    // box selection over rect1
    app.set_selection(&["rect1"]);
    assert_eq!(app.stacks(), (3, 1)); // even after re-select!
    app.assert_selected(&["rect1"]);

    app.redo();
    assert_eq!(app.stacks(), (4, 0));
    app.assert_selected(&["rect2"]);
    assert!(!app.get("rect1").base.is_deleted);
    assert!(!app.get("rect2").base.is_deleted);
}

#[test]
fn should_not_override_appstate_changes_when_redo_stack_is_not_cleared() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);
    app.background(RED);
    app.background(BLUE);
    assert_eq!(app.stacks(), (3, 0));
    app.assert_selected(&["rect"]);
    assert_eq!(app.get("rect").base.background_color, BLUE);

    app.undo();
    assert_eq!(app.stacks(), (2, 1));
    app.assert_selected(&["rect"]);
    assert_eq!(app.get("rect").base.background_color, RED);

    app.undo();
    assert_eq!(app.stacks(), (1, 2));
    app.assert_selected(&["rect"]);
    assert_eq!(app.get("rect").base.background_color, TRANSPARENT);

    app.click_empty();
    assert_eq!(app.stacks(), (2, 2)); // pushed appstate change, redo stack is not cleared
    assert!(app.selected().is_empty());

    app.redo();
    assert_eq!(app.stacks(), (3, 1));
    assert!(app.selected().is_empty());
    assert_eq!(app.get("rect").base.background_color, RED);

    app.redo();
    assert_eq!(app.stacks(), (4, 0));
    assert!(app.selected().is_empty());
    assert_eq!(app.get("rect").base.background_color, BLUE);

    app.undo();
    assert_eq!(app.stacks(), (3, 1));
    assert!(app.selected().is_empty());
    assert_eq!(app.get("rect").base.background_color, RED);

    app.undo();
    assert_eq!(app.stacks(), (2, 2));
    assert!(app.selected().is_empty());
    assert_eq!(app.get("rect").base.background_color, TRANSPARENT);

    app.undo();
    assert_eq!(app.stacks(), (1, 3));
    app.assert_selected(&["rect"]); // gets reselected without a pushed entry!
    assert_eq!(app.get("rect").base.background_color, TRANSPARENT);
}

#[test]
fn should_clear_the_redo_stack_on_elements_change() {
    let mut app = App::new();
    app.create_rect("rect1", 10.0, 10.0);
    assert_eq!(app.stacks(), (1, 0));
    app.assert_selected(&["rect1"]);

    app.undo();
    assert_eq!(app.stacks(), (0, 1));
    assert!(app.selected().is_empty());
    assert!(app.get("rect1").base.is_deleted);

    app.create_rect("rect2", 20.0, 20.0);
    app.assert_selected(&["rect2"]);
    assert_eq!(app.stacks(), (1, 0)); // redo stack got cleared
    let snapshot = app.s.store.snapshot();
    let snapshot: Vec<(&str, bool)> = snapshot
        .elements
        .values()
        .map(|e| (e.base.id.as_str(), e.base.is_deleted))
        .collect();
    assert_eq!(snapshot, [("rect1", true), ("rect2", false)]);
    assert!(app.get("rect1").base.is_deleted);
    assert!(!app.get("rect2").base.is_deleted);
}

#[test]
fn should_iterate_through_the_history_when_selection_changes_do_not_produce_visible_change() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);

    app.click_empty();
    assert_eq!(app.stacks(), (2, 0));
    assert!(app.selected().is_empty());

    app.undo();
    assert_eq!(app.stacks(), (1, 1));
    app.assert_selected(&["rect"]);

    app.click_empty();
    assert_eq!(app.stacks(), (2, 1));
    assert!(app.selected().is_empty());

    app.undo();
    assert_eq!(app.stacks(), (1, 2)); // now we have two same redos
    app.assert_selected(&["rect"]);

    app.redo();
    // didn't iterate through completely, as the first redo already results
    // in a visible change
    assert_eq!(app.stacks(), (2, 1));
    assert!(app.selected().is_empty());

    app.redo(); // acceptable empty redo
    assert_eq!(app.stacks(), (3, 0));
    assert!(app.selected().is_empty());

    app.undo();
    assert_eq!(app.stacks(), (2, 1));
    app.assert_selected(&["rect"]);

    app.undo();
    assert_eq!(app.stacks(), (0, 3)); // now we iterated through the same undos!
    assert!(app.selected().is_empty());
    assert!(app.get("rect").base.is_deleted);
}

#[test]
fn should_end_up_with_no_history_entry_after_initializing_scene() {
    let mut app =
        App::with_initial_data(vec![rect("A", 0.0, 0.0)], json!({"zenModeEnabled": true}));
    assert_eq!(app.state("zenModeEnabled"), json!(true));
    assert_eq!(app.ids(), ["A"]);
    assert!(app.s.history.is_undo_stack_empty());

    // noop
    app.undo();
    assert!(!app.get("A").base.is_deleted);

    app.create_rect("rectangle", 0.0, 0.0);
    assert_eq!(app.ids(), ["A", "rectangle"]);
    app.undo();
    assert!(!app.get("A").base.is_deleted);
    assert!(app.get("rectangle").base.is_deleted);

    // noop
    app.undo();
    assert!(!app.get("A").base.is_deleted);
    assert!(app.get("rectangle").base.is_deleted);
    assert_eq!(app.undo_len(), 0);

    app.redo();
    assert!(!app.get("A").base.is_deleted);
    assert!(!app.get("rectangle").base.is_deleted);
    assert_eq!(app.undo_len(), 1);
}

#[test]
fn should_create_new_history_entry_on_scene_import() {
    let mut app = App::with_initial_data(
        vec![rect("A", 0.0, 0.0)],
        json!({"viewBackgroundColor": "#FFF"}),
    );
    assert_eq!(app.state("viewBackgroundColor"), json!("#FFF"));
    assert_eq!(app.ids(), ["A"]);

    // the dropped file's scene replaces the current one (`loadFromBlob`)
    app.act(
        Some(vec![rect("B", 0.0, 0.0)]),
        Some(json!({"viewBackgroundColor": "#000"})),
    );
    assert_eq!(app.undo_len(), 1);
    assert_eq!(app.state("viewBackgroundColor"), json!("#000"));
    let snapshot = |app: &App| -> Vec<(String, bool)> {
        app.s
            .store
            .snapshot()
            .elements
            .values()
            .map(|e| (e.base.id.clone(), e.base.is_deleted))
            .collect()
    };
    assert_eq!(
        snapshot(&app),
        [("A".to_string(), true), ("B".to_string(), false)]
    );
    assert_eq!(app.ids(), ["B"]);

    app.undo();
    assert_eq!(
        snapshot(&app),
        [("A".to_string(), false), ("B".to_string(), true)]
    );
    assert_eq!(app.ids(), ["A", "B"]);
    assert!(!app.get("A").base.is_deleted);
    assert!(app.get("B").base.is_deleted);
    assert_eq!(app.state("viewBackgroundColor"), json!("#FFF"));

    app.redo();
    assert_eq!(app.state("viewBackgroundColor"), json!("#000"));
    assert_eq!(
        snapshot(&app),
        [("A".to_string(), true), ("B".to_string(), false)]
    );
    assert!(app.get("A").base.is_deleted);
    assert!(!app.get("B").base.is_deleted);
}

#[test]
fn should_support_appstate_name_or_view_background_color_change() {
    let mut app = App::with_initial_data(
        vec![],
        json!({"name": "Old name", "viewBackgroundColor": "#FFF"}),
    );
    assert_eq!(app.state("name"), json!("Old name"));

    app.update_scene(None, Some(json!({"name": "New name"})), Some(Immediately));
    assert_eq!(app.stacks(), (1, 0));
    assert_eq!(app.state("name"), json!("New name"));

    app.update_scene(
        None,
        Some(json!({"viewBackgroundColor": "#000"})),
        Some(Immediately),
    );
    assert_eq!(app.stacks(), (2, 0));
    assert_eq!(app.state("name"), json!("New name"));
    assert_eq!(app.state("viewBackgroundColor"), json!("#000"));

    // just to double check that same change is not recorded
    app.update_scene(
        None,
        Some(json!({"name": "New name", "viewBackgroundColor": "#000"})),
        Some(Immediately),
    );
    assert_eq!(app.stacks(), (2, 0));

    app.undo();
    assert_eq!(app.stacks(), (1, 1));
    assert_eq!(app.state("name"), json!("New name"));
    assert_eq!(app.state("viewBackgroundColor"), json!("#FFF"));

    app.undo();
    assert_eq!(app.stacks(), (0, 2));
    assert_eq!(app.state("name"), json!("Old name"));
    assert_eq!(app.state("viewBackgroundColor"), json!("#FFF"));

    app.redo();
    assert_eq!(app.stacks(), (1, 1));
    assert_eq!(app.state("name"), json!("New name"));
    assert_eq!(app.state("viewBackgroundColor"), json!("#FFF"));

    app.redo();
    assert_eq!(app.stacks(), (2, 0));
    assert_eq!(app.state("name"), json!("New name"));
    assert_eq!(app.state("viewBackgroundColor"), json!("#000"));
}

#[test]
fn should_support_element_creation_deletion_and_appstate_element_selection_change() {
    let mut app = App::new();
    app.create_rect("rect1", 10.0, 10.0);
    app.create_rect("rect2", 20.0, 20.0);
    app.create_rect("rect3", 40.0, 40.0);
    app.select(&["rect2", "rect3"]);
    app.delete_selected();
    assert_eq!(app.undo_len(), 6);

    let deleted =
        |app: &App| -> Vec<bool> { app.elements().iter().map(|e| e.base.is_deleted).collect() };

    app.undo();
    app.assert_selected(&["rect2", "rect3"]);
    assert_eq!(app.ids(), ["rect1", "rect2", "rect3"]);
    assert_eq!(deleted(&app), [false, false, false]);

    app.undo();
    app.assert_selected(&["rect2"]);

    app.undo();
    app.assert_selected(&["rect3"]);

    app.undo();
    app.assert_selected(&["rect2"]);
    assert_eq!(deleted(&app), [false, false, true]);

    app.undo();
    app.assert_selected(&["rect1"]);
    assert_eq!(deleted(&app), [false, true, true]);

    app.undo();
    app.assert_selected(&[]);
    assert_eq!(deleted(&app), [true, true, true]);

    // no-op
    app.undo();
    app.assert_selected(&[]);
    assert_eq!(deleted(&app), [true, true, true]);

    app.redo();
    app.assert_selected(&["rect1"]);
    assert_eq!(deleted(&app), [false, true, true]);

    app.redo();
    app.assert_selected(&["rect2"]);
    assert_eq!(deleted(&app), [false, false, true]);

    app.redo();
    app.assert_selected(&["rect3"]);

    app.redo();
    app.assert_selected(&["rect2"]);

    app.redo();
    app.assert_selected(&["rect2", "rect3"]);
    assert_eq!(deleted(&app), [false, false, false]);

    app.redo();
    assert_eq!(app.stacks(), (6, 0));
    app.assert_selected(&[]);
    assert_eq!(deleted(&app), [false, true, true]);

    // no-op
    app.redo();
    assert_eq!(app.stacks(), (6, 0));
    app.assert_selected(&[]);
    assert_eq!(deleted(&app), [false, true, true]);
}

#[test]
fn should_support_changes_in_elements_order() {
    let mut app = App::new();
    app.create_rect("rect1", 10.0, 10.0);
    app.create_rect("rect2", 20.0, 20.0);
    app.create_rect("rect3", 40.0, 40.0);

    // actionSendBackward on rect3
    app.reorder(&["rect1", "rect3", "rect2"], &["rect3"]);
    assert_eq!(app.stacks(), (4, 0));
    app.assert_selected(&["rect3"]);

    app.undo();
    assert_eq!(app.stacks(), (3, 1));
    app.assert_selected(&["rect3"]);
    assert_eq!(app.ids(), ["rect1", "rect2", "rect3"]);

    app.redo();
    assert_eq!(app.stacks(), (4, 0));
    app.assert_selected(&["rect3"]);
    assert_eq!(app.ids(), ["rect1", "rect3", "rect2"]);

    app.select(&["rect1", "rect3"]);
    assert_eq!(app.stacks(), (6, 0));
    app.assert_selected(&["rect1", "rect3"]);

    // actionBringForward on rect1 and rect3
    app.reorder(&["rect2", "rect1", "rect3"], &["rect2"]);
    assert_eq!(app.stacks(), (7, 0));
    app.assert_selected(&["rect1", "rect3"]);

    app.undo();
    assert_eq!(app.stacks(), (6, 1));
    app.assert_selected(&["rect1", "rect3"]);
    assert_eq!(app.ids(), ["rect1", "rect3", "rect2"]);

    app.redo();
    assert_eq!(app.stacks(), (7, 0));
    app.assert_selected(&["rect1", "rect3"]);
    assert_eq!(app.ids(), ["rect2", "rect1", "rect3"]);
}

// ---------------------------------------------------------------------------
// should support bidirectional bindings

/// `beforeEach` of "should support bidirectional bindings": rect1 with
/// `text` bound to it and an arrow from rect1 to rect2, four entries plus
/// the arrow's.
fn bindings_scene() -> App {
    let mut app = App::new();
    let r1 = rect_sized("rect1", -100.0, -50.0, 100.0, 100.0);
    let t = text("text", "ola", -200.0, -200.0);
    let r2 = rect_sized("rect2", 100.0, -50.0, 100.0, 100.0);
    app.update_scene(Some(vec![r1, t, r2]), None, Some(Immediately));

    // bind text to rect1: select both, then "Bind text to the container"
    app.select(&["rect1", "text"]);
    let r1 = app.get("rect1");
    let t = app.get("text");
    let r1 = app.with(
        &r1,
        json!({"boundElements": [{"id": "text", "type": "text"}]}),
    );
    let t = app.with(&t, json!({"containerId": "rect1"}));
    let r2 = app.get("rect2");
    app.act(
        Some(vec![r1, t, r2]),
        Some(json!({"selectedElementIds": {"rect1": true}})),
    );
    assert_eq!(app.undo_len(), 4);
    assert_eq!(container_id(&app.get("text")), json!("rect1"));

    // draw an arrow from rect1 to rect2
    let mut a = arrow("arrow", vec![[0.0, 0.0], [44.0, 0.0]]);
    a.base.x = 3.0;
    if let Some(linear) = a.kind.linear_mut() {
        linear.start_binding = Some(binding("rect1", [1.0, 0.5001]));
        linear.end_binding = Some(binding("rect2", [0.5001, 0.5001]));
    }
    let r1 = app.get("rect1");
    let r2 = app.get("rect2");
    let r1 = app.with(
        &r1,
        json!({"boundElements": [{"id": "text", "type": "text"}, {"id": "arrow", "type": "arrow"}]}),
    );
    let r2 = app.with(
        &r2,
        json!({"boundElements": [{"id": "arrow", "type": "arrow"}]}),
    );
    let t = app.get("text");
    app.act(
        Some(vec![r1, t, r2, a]),
        Some(json!({"selectedElementIds": {"arrow": true}})),
    );
    assert_eq!(app.undo_len(), 5);
    assert_eq!(
        bound_elements(&app.get("rect1")),
        json!([{"id": "text", "type": "text"}, {"id": "arrow", "type": "arrow"}])
    );
    assert_eq!(
        bound_elements(&app.get("rect2")),
        json!([{"id": "arrow", "type": "arrow"}])
    );
    app
}

fn arrow_binding(app: &App, end: &str) -> Value {
    prop(&app.get("arrow"), end)
}

#[test]
fn should_unbind_arrow_from_non_deleted_bindable_elements_on_undo_and_rebind_on_redo() {
    let mut app = bindings_scene();
    app.undo();
    assert_eq!(app.stacks(), (4, 1));
    assert_eq!(arrow_binding(&app, "startBinding")["elementId"], "rect1");
    assert_eq!(arrow_binding(&app, "endBinding")["elementId"], "rect2");
    assert_eq!(app.ids(), ["rect1", "text", "rect2", "arrow"]);
    assert_eq!(
        bound_elements(&app.get("rect1")),
        json!([{"id": "text", "type": "text"}])
    );
    assert_eq!(bound_elements(&app.get("rect2")), json!([]));
    assert!(app.get("arrow").base.is_deleted);

    app.redo();
    assert_eq!(app.stacks(), (5, 0));
    assert_eq!(arrow_binding(&app, "startBinding")["elementId"], "rect1");
    assert_eq!(arrow_binding(&app, "endBinding")["elementId"], "rect2");
    assert_eq!(
        bound_elements(&app.get("rect1")),
        json!([{"id": "text", "type": "text"}, {"id": "arrow", "type": "arrow"}])
    );
    assert_eq!(
        bound_elements(&app.get("rect2")),
        json!([{"id": "arrow", "type": "arrow"}])
    );
    assert!(!app.get("arrow").base.is_deleted);
}

#[test]
fn should_unbind_everything_when_iterating_through_the_whole_undo_stack_and_rebind_on_redo() {
    let mut app = bindings_scene();
    for _ in 0..5 {
        app.undo();
    }
    assert_eq!(app.stacks(), (0, 5));
    let r1 = app.get("rect1");
    assert_eq!(bound_elements(&r1), json!([]));
    assert!(r1.base.is_deleted);
    let t = app.get("text");
    assert_eq!(container_id(&t), Value::Null);
    assert!(t.base.is_deleted);
    let r2 = app.get("rect2");
    assert_eq!(bound_elements(&r2), json!([]));
    assert!(r2.base.is_deleted);
    let a = app.get("arrow");
    assert_eq!(prop(&a, "startBinding")["elementId"], "rect1");
    assert_eq!(prop(&a, "endBinding")["elementId"], "rect2");
    assert!(a.base.is_deleted);

    for _ in 0..5 {
        app.redo();
    }
    assert_eq!(app.stacks(), (5, 0));
    let r1 = app.get("rect1");
    let r1_bound: HashSet<String> = bound_elements(&r1)
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.to_string())
        .collect();
    assert_eq!(
        r1_bound,
        [
            json!({"id": "text", "type": "text"}).to_string(),
            json!({"id": "arrow", "type": "arrow"}).to_string()
        ]
        .into_iter()
        .collect()
    );
    assert!(!r1.base.is_deleted);
    assert_eq!(container_id(&app.get("text")), json!("rect1"));
    assert!(!app.get("text").base.is_deleted);
    assert_eq!(
        bound_elements(&app.get("rect2")),
        json!([{"id": "arrow", "type": "arrow"}])
    );
    assert_eq!(arrow_binding(&app, "startBinding")["elementId"], "rect1");
    assert_eq!(arrow_binding(&app, "endBinding")["elementId"], "rect2");
    assert!(!app.get("arrow").base.is_deleted);
}

#[test]
fn should_unbind_rectangles_from_arrow_on_deletion_and_rebind_on_undo() {
    let mut app = bindings_scene();
    app.select(&["rect1", "rect2"]);
    assert_eq!(app.undo_len(), 7);
    // actionDeleteSelected: the rectangles, rect1's label, and the arrow's
    // bindings to them
    let elements: Vec<Element> = app
        .elements()
        .iter()
        .map(|e| match e.base.id.as_str() {
            "rect1" | "rect2" | "text" => with(e, json!({"isDeleted": true}), app.env()),
            "arrow" => with(
                e,
                json!({"startBinding": null, "endBinding": null}),
                app.env(),
            ),
            _ => e.clone(),
        })
        .collect();
    app.act(Some(elements), Some(json!({"selectedElementIds": {}})));
    assert_eq!(app.stacks(), (8, 0));
    assert!(app.get("rect1").base.is_deleted);
    assert_eq!(container_id(&app.get("text")), json!("rect1"));
    assert_eq!(arrow_binding(&app, "startBinding"), Value::Null);
    assert_eq!(arrow_binding(&app, "endBinding"), Value::Null);

    app.undo();
    assert_eq!(app.stacks(), (7, 1));
    let r1 = app.get("rect1");
    assert!(!r1.base.is_deleted);
    // order has now changed!
    assert_eq!(
        bound_elements(&r1),
        json!([{"id": "arrow", "type": "arrow"}, {"id": "text", "type": "text"}])
    );
    assert_eq!(container_id(&app.get("text")), json!("rect1"));
    assert!(!app.get("text").base.is_deleted);
    assert_eq!(
        bound_elements(&app.get("rect2")),
        json!([{"id": "arrow", "type": "arrow"}])
    );
    assert!(!app.get("rect2").base.is_deleted);
    assert_eq!(arrow_binding(&app, "startBinding")["elementId"], "rect1");
    assert_eq!(arrow_binding(&app, "endBinding")["elementId"], "rect2");
    assert!(!app.get("arrow").base.is_deleted);
}

// ===========================================================================
// multiplayer undo/redo

#[test]
fn should_not_override_remote_changes_on_different_elements() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);
    app.background(RED);
    assert_eq!(app.undo_len(), 2);
    assert_eq!(app.get("rect").base.background_color, RED);

    // Simulate remote update
    let mut remote = rect("remote", 0.0, 0.0);
    remote.base.stroke_color = BLUE.into();
    let mut elements = app.elements();
    elements.push(remote);
    app.update_scene(Some(elements), None, Some(Never));

    app.undo();
    assert_eq!(app.at(0).base.background_color, TRANSPARENT);
    assert_eq!(app.at(1).base.stroke_color, BLUE);

    app.redo();
    assert_eq!(app.at(0).base.background_color, RED);
    assert_eq!(app.at(1).base.stroke_color, BLUE);

    app.undo();
    assert_eq!(app.undo_len(), 1);
    assert_eq!(app.at(0).base.background_color, TRANSPARENT);
    assert_eq!(app.at(1).base.stroke_color, BLUE);
}

#[test]
fn should_not_override_remote_changes_on_different_properties() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);
    app.background(RED);
    assert_eq!(app.undo_len(), 2);

    // Simulate remote update
    let r = app.at(0);
    let remote = app.with(&r, json!({"strokeColor": YELLOW}));
    app.update_scene(Some(vec![remote]), None, Some(Never));

    app.undo();
    assert_eq!(app.at(0).base.background_color, TRANSPARENT);
    assert_eq!(app.at(0).base.stroke_color, YELLOW);

    app.redo();
    assert_eq!(app.at(0).base.background_color, RED);
    assert_eq!(app.at(0).base.stroke_color, YELLOW);
}

#[test]
fn should_update_history_entries_after_remote_changes_on_the_same_properties() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);
    app.background(RED);
    app.background(BLUE);
    assert_eq!(app.undo_len(), 3);

    app.undo();
    assert_eq!(app.at(0).base.background_color, RED);
    app.redo();
    assert_eq!(app.at(0).base.background_color, BLUE);

    // Simulate remote update
    let r = app.at(0);
    let remote = app.with(&r, json!({"backgroundColor": YELLOW}));
    app.update_scene(Some(vec![remote]), None, Some(Never));

    // our entry gets updated from red -> blue into red -> yellow
    app.undo();
    assert_eq!(app.at(0).base.background_color, RED);

    // Simulate remote update
    let r = app.at(0);
    let remote = app.with(&r, json!({"backgroundColor": VIOLET}));
    app.update_scene(Some(vec![remote]), None, Some(Never));

    // our (inversed) entry gets updated from red -> yellow into violet -> yellow
    app.redo();
    assert_eq!(app.at(0).base.background_color, YELLOW);

    app.undo();
    assert_eq!(app.at(0).base.background_color, VIOLET);

    app.undo();
    assert_eq!(app.at(0).base.background_color, TRANSPARENT);
}

#[test]
fn should_override_remotely_added_groups_on_undo_but_restore_them_on_redo() {
    let mut app = App::new();
    let r1 = rect("rect1", 0.0, 0.0);
    let r2 = rect("rect2", 0.0, 0.0);
    app.update_scene(Some(vec![r1, r2]), None, Some(Never));

    // Simulate local update
    let (a, b) = (app.at(0), app.at(1));
    let local = vec![
        app.with(&a, json!({"groupIds": ["A"]})),
        app.with(&b, json!({"groupIds": ["A"]})),
    ];
    app.update_scene(Some(local), None, Some(Immediately));

    let mut r3 = rect("rect3", 0.0, 0.0);
    r3.base.group_ids = vec!["B".into()];
    let mut r4 = rect("rect4", 0.0, 0.0);
    r4.base.group_ids = vec!["B".into()];

    // Simulate remote update
    let (a, b) = (app.at(0), app.at(1));
    let remote = vec![
        app.with(&a, json!({"groupIds": ["A", "B"]})),
        app.with(&b, json!({"groupIds": ["A", "B"]})),
        r3,
        r4,
    ];
    app.update_scene(Some(remote), None, Some(Never));

    let groups =
        |app: &App| -> Vec<Value> { app.elements().iter().map(|e| prop(e, "groupIds")).collect() };

    app.undo();
    assert_eq!(app.stacks(), (0, 1));
    assert_eq!(app.ids(), ["rect1", "rect2", "rect3", "rect4"]);
    assert_eq!(
        groups(&app),
        [json!([]), json!([]), json!(["B"]), json!(["B"])]
    );

    app.redo();
    assert_eq!(app.stacks(), (1, 0));
    assert_eq!(
        groups(&app),
        [
            json!(["A", "B"]),
            json!(["A", "B"]),
            json!(["B"]),
            json!(["B"])
        ]
    );
}

#[test]
fn should_override_remotely_added_points_on_undo_but_restore_them_on_redo() {
    let mut app = App::new();
    // UI.clickTool("arrow"); click (0, 0), (10, 10), (20, 20); Enter
    let a = arrow("arrow", vec![[0.0, 0.0], [10.0, 10.0]]);
    app.act(
        Some(vec![a]),
        Some(json!({"selectedElementIds": {"arrow": true}})),
    );
    let a = app.at(0);
    let a = app.with(&a, json!({"points": [[0, 0], [10, 10], [20, 20]]}));
    app.act(Some(vec![a]), None);
    assert_eq!(app.undo_len(), 2);

    // Simulate remote update
    let a = app.at(0);
    let remote = app.with(
        &a,
        json!({"points": [[0, 0], [5, 5], [10, 10], [15, 15], [20, 20]]}),
    );
    app.update_scene(Some(vec![remote]), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (1, 1));
    // overriding all the remote points as they are not being postprocessed
    assert_eq!(prop(&app.at(0), "points"), json!([[0, 0], [10, 10]]));

    app.undo();
    assert_eq!(app.stacks(), (0, 2));
    assert!(app.at(0).base.is_deleted);
    assert_eq!(prop(&app.at(0), "points"), json!([[0, 0], [10, 10]]));

    app.redo();
    assert_eq!(app.stacks(), (1, 1));
    assert!(!app.at(0).base.is_deleted);
    assert_eq!(prop(&app.at(0), "points"), json!([[0, 0], [10, 10]]));

    app.redo();
    assert_eq!(app.stacks(), (2, 0));
    assert_eq!(
        prop(&app.at(0), "points"),
        json!([[0, 0], [5, 5], [10, 10], [15, 15], [20, 20]])
    );
}

#[test]
fn should_redistribute_deltas_when_element_gets_removed_locally_but_is_restored_remotely() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);
    app.delete_selected();
    assert_eq!(app.undo_len(), 2);
    assert!(app.at(0).base.is_deleted);
    assert_eq!(app.at(0).base.background_color, TRANSPARENT);

    // Simulate remote update & restore
    let r = app.at(0);
    let remote = app.with(&r, json!({"backgroundColor": YELLOW, "isDeleted": false}));
    app.update_scene(Some(vec![remote]), None, Some(Never));
    assert!(app.selected().is_empty());
    assert!(!app.at(0).base.is_deleted);
    assert_eq!(app.at(0).base.background_color, YELLOW);

    // inserted.isDeleted: true is updated with the latest changes to false,
    // so the removed delta becomes an updated delta
    app.undo();
    app.assert_selected(&["rect"]);
    assert_eq!(app.stacks(), (1, 1));
    assert!(!app.at(0).base.is_deleted);
    assert_eq!(app.at(0).base.background_color, YELLOW);

    app.undo();
    assert!(app.selected().is_empty());
    assert_eq!(app.stacks(), (0, 2));
    assert!(app.at(0).base.is_deleted);
    assert_eq!(app.at(0).base.background_color, YELLOW);

    app.redo();
    app.assert_selected(&["rect"]);
    assert_eq!(app.stacks(), (1, 1));
    assert!(!app.at(0).base.is_deleted);

    app.redo();
    app.assert_selected(&[]);
    assert_eq!(app.stacks(), (2, 0));
    assert!(!app.at(0).base.is_deleted); // isDeleted got updated
    assert_eq!(app.at(0).base.background_color, YELLOW);
}

#[test]
fn should_iterate_through_the_history_when_element_change_relates_to_remotely_deleted_element() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);
    app.background(RED);
    assert_eq!(app.undo_len(), 2);

    // Simulate remote update & deletion
    let r = app.at(0);
    let remote = app.with(&r, json!({"backgroundColor": YELLOW, "isDeleted": true}));
    app.update_scene(Some(vec![remote]), None, Some(Never));
    assert!(app.at(0).base.is_deleted);

    // iterates through the undo stack since applying the change results in
    // no visible change on a deleted element
    app.undo();
    assert_eq!(app.stacks(), (0, 2));
    assert_eq!(app.at(0).base.background_color, TRANSPARENT);
    assert!(app.at(0).base.is_deleted);

    // we reached the bottom, again we iterate through invisible changes
    app.redo();
    app.assert_selected(&[]);
    assert_eq!(app.stacks(), (2, 0));
    assert_eq!(app.at(0).base.background_color, YELLOW); // the colour still gets updated
    assert!(app.at(0).base.is_deleted); // but the element remains deleted
}

#[test]
fn should_iterate_through_the_history_when_element_changes_relate_only_to_remotely_deleted_elements(
) {
    let mut app = App::new();
    app.create_rect("rect1", 10.0, 10.0);
    app.create_rect("rect2", 20.0, 20.0);
    app.background(RED);
    app.create_rect("rect3", 30.0, 30.0);
    // move rect3 by (20, 20)
    let r3 = app.get("rect3");
    let moved = app.with(&r3, json!({"x": 50, "y": 50}));
    let elements = vec![app.at(0), app.at(1), moved];
    app.act(Some(elements), None);
    assert_eq!(app.undo_len(), 5);

    // Simulate remote update
    let (a, b, c) = (app.at(0), app.at(1), app.at(2));
    let remote = vec![
        a,
        app.with(&b, json!({"isDeleted": true})),
        app.with(&c, json!({"isDeleted": true})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (1, 4));
    assert_eq!(app.selected(), ["rect1"]);
    assert!(!app.get("rect1").base.is_deleted);
    let r2 = app.get("rect2");
    assert!(r2.base.is_deleted);
    assert_eq!(r2.base.background_color, TRANSPARENT);
    let r3 = app.get("rect3");
    assert!(r3.base.is_deleted);
    assert_eq!((r3.base.x, r3.base.y), (30.0, 30.0));

    app.redo();
    assert_eq!(app.stacks(), (5, 0));
    assert!(app.selected().is_empty());
    assert!(!app.get("rect1").base.is_deleted);
    let r2 = app.get("rect2");
    assert!(r2.base.is_deleted);
    assert_eq!(r2.base.background_color, RED);
    let r3 = app.get("rect3");
    assert!(r3.base.is_deleted);
    assert_eq!((r3.base.x, r3.base.y), (50.0, 50.0));
}

#[test]
fn should_iterate_through_the_history_when_selected_elements_relate_only_to_remotely_deleted_elements(
) {
    let mut app = App::new();
    app.set_elements(vec![
        rect("rect1", 10.0, 10.0),
        rect("rect2", 20.0, 20.0),
        rect("rect3", 30.0, 30.0),
    ]);
    app.select(&["rect1"]);
    app.select(&["rect2", "rect3"]);
    assert_eq!(app.undo_len(), 3);
    assert_eq!(app.selected(), ["rect2", "rect3"]);

    // Simulate remote update
    let (a, b, c) = (app.at(0), app.at(1), app.at(2));
    let remote = vec![
        a,
        app.with(&b, json!({"isDeleted": true})),
        app.with(&c, json!({"isDeleted": true})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (1, 2));
    assert_eq!(app.selected(), ["rect1"]);

    app.redo();
    assert_eq!(app.stacks(), (3, 0));
    // no selectedElementIds, as all relate to deleted elements
    assert!(app.selected().is_empty());
    let deleted: Vec<bool> = app.elements().iter().map(|e| e.base.is_deleted).collect();
    assert_eq!(deleted, [false, true, true]);

    app.undo();
    assert_eq!(app.stacks(), (1, 2));
    assert_eq!(app.selected(), ["rect1"]);

    // Simulate remote update
    let (a, b, c) = (app.at(0), app.at(1), app.at(2));
    let remote = vec![
        a,
        app.with(&b, json!({"isDeleted": false})),
        app.with(&c, json!({"isDeleted": false})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    app.redo();
    assert_eq!(app.stacks(), (2, 1));
    assert_eq!(app.selected(), ["rect2"]);

    app.redo();
    assert_eq!(app.stacks(), (3, 0));
    // selected again, as they got restored remotely
    assert_eq!(app.selected(), ["rect2", "rect3"]);
    let deleted: Vec<bool> = app.elements().iter().map(|e| e.base.is_deleted).collect();
    assert_eq!(deleted, [false, false, false]);
}

#[test]
fn should_iterate_through_the_history_when_selected_groups_contain_only_remotely_deleted_elements()
{
    let mut app = App::new();
    let grouped = |id: &str, group: &str| {
        let mut r = rect(id, 0.0, 0.0);
        r.base.group_ids = vec![group.into()];
        r
    };
    let (r1, r2, r3, r4) = (
        grouped("rect1", "A"),
        grouped("rect2", "A"),
        grouped("rect3", "B"),
        grouped("rect4", "B"),
    );

    // Simulate remote update
    app.update_scene(Some(vec![r1, r2]), None, Some(Never));
    // Ctrl+A (actionSelectAll): every element and its groups
    app.act(
        None,
        Some(json!({
            "selectedElementIds": {"rect1": true, "rect2": true},
            "selectedGroupIds": {"A": true},
        })),
    );

    // Simulate remote update
    let elements = vec![app.at(0), app.at(1), r3.clone(), r4.clone()];
    app.update_scene(Some(elements), None, Some(Never));
    app.act(
        None,
        Some(json!({
            "selectedElementIds": {"rect1": true, "rect2": true, "rect3": true, "rect4": true},
            "selectedGroupIds": {"A": true, "B": true},
        })),
    );
    assert_eq!(app.stacks(), (2, 0));
    assert_eq!(app.state("selectedGroupIds"), json!({"A": true, "B": true}));

    // Simulate remote update
    let (a, b) = (app.at(0), app.at(1));
    let remote = vec![
        app.with(&a, json!({"isDeleted": true})),
        app.with(&b, json!({"isDeleted": true})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (0, 2)); // iterated two steps back!
    assert_eq!(app.state("selectedGroupIds"), json!({}));

    app.redo();
    assert_eq!(app.stacks(), (2, 0)); // iterated two steps forward!
    assert_eq!(app.state("selectedGroupIds"), json!({}));

    app.undo();

    // Simulate remote update
    let (a, b) = (app.at(0), app.at(1));
    let remote = vec![
        app.with(&a, json!({"isDeleted": false})),
        app.with(&b, json!({"isDeleted": false})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    app.redo();
    assert_eq!(app.stacks(), (1, 1));
    assert_eq!(app.state("selectedGroupIds"), json!({"A": true}));

    // Simulate remote update
    let elements = vec![app.at(0), app.at(1), r3, r4];
    app.update_scene(Some(elements), None, Some(Never));

    app.redo();
    assert_eq!(app.stacks(), (2, 0));
    assert_eq!(app.state("selectedGroupIds"), json!({"A": true, "B": true}));
}

#[test]
fn should_iterate_through_the_history_when_editing_group_contains_only_remotely_deleted_elements() {
    let mut app = App::new();
    let mut r1 = rect("rect1", 0.0, 0.0);
    r1.base.group_ids = vec!["A".into()];
    let mut r2 = rect("rect2", 100.0, 100.0);
    r2.base.group_ids = vec!["A".into()];
    app.set_elements(vec![r1, r2]);
    // mouse.select(rect1): the whole group
    app.act(
        None,
        Some(json!({
            "selectedElementIds": {"rect1": true, "rect2": true},
            "selectedGroupIds": {"A": true},
        })),
    );
    // inside the editing group: double click on rect2
    app.act(
        None,
        Some(json!({
            "selectedElementIds": {"rect2": true},
            "selectedGroupIds": {},
            "editingGroupId": "A",
        })),
    );
    assert_eq!(app.stacks(), (2, 0));
    assert_eq!(app.state("editingGroupId"), json!("A"));

    app.click_empty();
    assert!(app.selected().is_empty());
    assert_eq!(app.stacks(), (3, 0));
    assert_eq!(app.state("editingGroupId"), Value::Null);

    // Simulate remote update
    let (a, b) = (app.at(0), app.at(1));
    let remote = vec![
        app.with(&a, json!({"isDeleted": true})),
        app.with(&b, json!({"isDeleted": true})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (0, 3));
    assert_eq!(app.state("editingGroupId"), Value::Null);

    app.redo();
    assert_eq!(app.stacks(), (3, 0));
    assert_eq!(app.state("editingGroupId"), Value::Null);

    // Simulate remote update
    let (a, b) = (app.at(0), app.at(1));
    let remote = vec![app.with(&a, json!({"isDeleted": false})), b];
    app.update_scene(Some(remote), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (2, 1));
    assert_eq!(app.state("editingGroupId"), json!("A"));

    app.redo();
    assert_eq!(app.stacks(), (3, 0));
    assert_eq!(app.state("editingGroupId"), Value::Null);
}

#[test]
fn should_iterate_through_the_history_when_selected_or_editing_linear_element_was_remotely_deleted()
{
    let mut app = App::new();
    // create an arrow, then finalize it (selected as a linear element)
    let a = arrow("arrow", vec![[0.0, 0.0], [10.0, 10.0]]);
    app.act(
        Some(vec![a]),
        Some(json!({
            "selectedElementIds": {"arrow": true},
            "selectedLinearElement": {"elementId": "arrow", "isEditing": false},
        })),
    );
    // open the editor
    app.act(
        None,
        Some(json!({"selectedLinearElement": {"elementId": "arrow", "isEditing": true}})),
    );
    // leave the editor
    app.act(
        None,
        Some(json!({"selectedLinearElement": {"elementId": "arrow", "isEditing": false}})),
    );
    assert_eq!(app.stacks(), (3, 0));
    assert_eq!(
        app.state("selectedLinearElement")["isEditing"],
        json!(false)
    );

    // Simulate remote update
    let a = app.at(0);
    let remote = app.with(&a, json!({"isDeleted": true}));
    app.update_scene(Some(vec![remote]), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (0, 3));
    assert_eq!(app.state("selectedLinearElement"), Value::Null);

    app.redo();
    assert_eq!(app.stacks(), (3, 0));
    assert_eq!(app.state("selectedLinearElement"), Value::Null);
}

#[test]
fn should_iterate_through_the_history_when_z_index_changes_do_not_produce_visible_change_and_we_synced_changed_indices(
) {
    let mut app = App::new();
    app.set_elements(vec![
        rect("rect1", 10.0, 10.0), // a "a0"
        rect("rect2", 20.0, 20.0), // b "a1"
        rect("rect3", 30.0, 30.0), // c "a2"
    ]);
    app.select(&["rect2"]);
    // actionSendToBack
    app.reorder(&["rect2", "rect1", "rect3"], &["rect2"]);
    assert_eq!(app.stacks(), (2, 0));
    app.assert_selected(&["rect2"]);
    assert_eq!(app.ids(), ["rect2", "rect1", "rect3"]);
    assert_eq!(index(&app.get("rect2")), "Zz");

    // Simulate remote update
    let (a, b, c) = (app.at(0), app.at(1), app.at(2));
    let remote = vec![app.with(&c, json!({"index": "Zy"})), a, b];
    app.update_scene(Some(remote), None, Some(Never));
    assert_eq!(app.stacks(), (2, 0));
    app.assert_selected(&["rect2"]);
    assert_eq!(app.ids(), ["rect3", "rect2", "rect1"]);

    app.undo();
    assert_eq!(app.stacks(), (1, 1));
    app.assert_selected(&["rect2"]);
    assert_eq!(app.ids(), ["rect3", "rect1", "rect2"]);

    app.redo();
    assert_eq!(app.stacks(), (2, 0));
    app.assert_selected(&["rect2"]);
    assert_eq!(app.ids(), ["rect3", "rect2", "rect1"]);

    // Simulate remote update
    let (a, b, c) = (app.at(0), app.at(1), app.at(2));
    let remote = vec![app.with(&c, json!({"index": "Zx"})), a, b];
    app.update_scene(Some(remote), None, Some(Never));
    assert_eq!(app.stacks(), (2, 0));
    app.assert_selected(&["rect2"]);
    assert_eq!(app.ids(), ["rect1", "rect3", "rect2"]);

    app.undo();
    // we iterated two steps as there was no change in order!
    assert_eq!(app.stacks(), (0, 2));
    assert!(app.selected().is_empty());
    assert_eq!(app.ids(), ["rect1", "rect3", "rect2"]);
}

#[test]
fn should_iterate_through_the_history_when_z_index_changes_do_not_produce_visible_change_and_we_synced_all_indices(
) {
    let mut app = App::new();
    app.set_elements(vec![
        rect("rect1", 10.0, 10.0),
        rect("rect2", 20.0, 20.0),
        rect("rect3", 30.0, 30.0),
    ]);
    app.select(&["rect2"]);
    app.reorder(&["rect2", "rect1", "rect3"], &["rect2"]);
    assert_eq!(app.stacks(), (2, 0));
    app.assert_selected(&["rect2"]);
    assert_eq!(app.ids(), ["rect2", "rect1", "rect3"]);

    // Simulate remote update (fixes all invalid z-indices)
    let (a, b, c) = (app.at(0), app.at(1), app.at(2));
    app.update_scene(Some(vec![c, a, b]), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (1, 1));
    app.assert_selected(&["rect2"]);
    assert_eq!(app.ids(), ["rect2", "rect3", "rect1"]);

    app.redo();
    assert_eq!(app.stacks(), (2, 0));
    app.assert_selected(&["rect2"]);
    assert_eq!(app.ids(), ["rect3", "rect2", "rect1"]);

    // Simulate remote update
    let (a, b, c) = (app.at(0), app.at(1), app.at(2));
    app.update_scene(Some(vec![b, a, c]), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (0, 2)); // now we iterated two steps back!
    app.assert_selected(&[]);
    assert_eq!(app.ids(), ["rect2", "rect3", "rect1"]);
}

// ---------------------------------------------------------------------------
// conflicts in bound text elements and their containers

fn container() -> Element {
    rect_sized("container", 10.0, 10.0, 100.0, 100.0)
}

fn label() -> Element {
    text("text", "que pasa", 15.0, 15.0)
}

fn bound_text_state(app: &App) -> Vec<(String, Value, bool)> {
    app.elements()
        .iter()
        .map(|e| {
            let binding = match &e.kind {
                ElementKind::Text(_) => container_id(e),
                _ => bound_elements(e),
            };
            (e.base.id.clone(), binding, e.base.is_deleted)
        })
        .collect()
}

fn row(id: &str, binding: Value, deleted: bool) -> (String, Value, bool) {
    (id.to_string(), binding, deleted)
}

/// The container and label bound locally, captured.
fn bind_locally(app: &mut App) {
    let (c, t) = (app.at(0), app.at(1));
    let local = vec![
        app.with(
            &c,
            json!({"boundElements": [{"id": "text", "type": "text"}]}),
        ),
        app.with(&t, json!({"containerId": "container"})),
    ];
    app.update_scene(Some(local), None, Some(Immediately));
}

#[test]
fn should_rebind_bindings_when_both_are_updated_through_the_history_and_there_no_conflicting_updates_in_the_meantime(
) {
    let mut app = App::new();
    app.update_scene(Some(vec![container(), label()]), None, Some(Never));
    bind_locally(&mut app);

    app.undo();
    assert_eq!(app.stacks(), (0, 1));
    assert_eq!(
        bound_text_state(&app),
        [
            row("container", json!([]), false),
            row("text", Value::Null, false)
        ]
    );

    // Simulate remote update, no conflicting updates
    let (c, t) = (app.at(0), app.at(1));
    let remote = vec![
        app.with(&c, json!({"x": t.base.x + 20.0})),
        app.with(&t, json!({"x": t.base.x + 10.0})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        assert_eq!(
            bound_text_state(&app),
            [
                row("container", json!([{"id": "text", "type": "text"}]), false),
                row("text", json!("container"), false)
            ]
        );

        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(
            bound_text_state(&app),
            [
                row("container", json!([]), false),
                row("text", Value::Null, false)
            ]
        );
    }
}

#[test]
fn should_rebind_bindings_when_both_are_updated_through_the_history_and_the_container_got_bound_to_a_different_text_in_the_meantime(
) {
    let mut app = App::new();
    app.update_scene(Some(vec![container(), label()]), None, Some(Never));
    bind_locally(&mut app);
    app.undo();
    assert_eq!(app.stacks(), (0, 1));

    let mut remote_text = text("remoteText", "ola", 0.0, 0.0);
    if let ElementKind::Text(t) = &mut remote_text.kind {
        t.container_id = Some("container".into());
    }
    // Simulate remote update
    let (c, t) = (app.at(0), app.at(1));
    let remote = vec![
        app.with(
            &c,
            json!({"boundElements": [{"id": "remoteText", "type": "text"}]}),
        ),
        remote_text,
        t,
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        assert_eq!(
            bound_text_state(&app),
            [
                // last added was `text`, removing `remoteText`
                row("container", json!([{"id": "text", "type": "text"}]), false),
                // unbound as `remoteText` was removed
                row("remoteText", Value::Null, false),
                // rebound!
                row("text", json!("container"), false),
            ]
        );

        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(
            bound_text_state(&app),
            [
                row(
                    "container",
                    json!([{"id": "remoteText", "type": "text"}]),
                    false
                ),
                row("remoteText", json!("container"), false),
                row("text", Value::Null, false),
            ]
        );
    }
}

#[test]
fn should_rebind_bindings_when_both_are_updated_through_the_history_and_the_text_got_bound_to_a_different_container_in_the_meantime(
) {
    let mut app = App::new();
    app.update_scene(Some(vec![container(), label()]), None, Some(Never));
    bind_locally(&mut app);
    app.undo();
    assert_eq!(app.stacks(), (0, 1));

    let mut remote_container = rect_sized("remoteContainer", 100.0, 100.0, 50.0, 50.0);
    remote_container.base.bound_elements = Some(vec![bound("text", BoundElementType::Text)]);
    // Simulate remote update
    let (c, t) = (app.at(0), app.at(1));
    let remote = vec![
        c,
        app.with(
            &remote_container,
            json!({"boundElements": [{"id": "text", "type": "text"}]}),
        ),
        app.with(&t, json!({"containerId": "remoteContainer"})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        assert_eq!(
            bound_text_state(&app),
            [
                // rebound the text as we captured the full bidirectional binding
                row("container", json!([{"id": "text", "type": "text"}]), false),
                // previous binding got unbound
                row("remoteContainer", json!([]), false),
                // rebound!
                row("text", json!("container"), false),
            ]
        );

        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(
            bound_text_state(&app),
            [
                // deleted binding (already during applyDelta)
                row("container", json!([]), false),
                // due to the restored binding, we could rebind the remote container!
                row(
                    "remoteContainer",
                    json!([{"id": "text", "type": "text"}]),
                    false
                ),
                // due to applying latest changes to the history entries
                row("text", json!("remoteContainer"), false),
            ]
        );
    }
}

#[test]
fn should_rebind_remotely_added_bound_text_when_its_container_is_added_through_the_history() {
    let mut app = App::new();
    // Simulate local update
    app.update_scene(Some(vec![container()]), None, Some(Immediately));

    // Simulate remote update
    let c = app.at(0);
    let t = label();
    let remote = vec![
        app.with(
            &c,
            json!({"boundElements": [{"id": "text", "type": "text"}]}),
        ),
        app.with(&t, json!({"containerId": "container"})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(
            bound_text_state(&app),
            [
                // binding from deleted to non deleted is correct!
                row("container", json!([{"id": "text", "type": "text"}]), true),
                // we trigger unbind - binding from non deleted to deleted cannot exist!
                row("text", Value::Null, false),
            ]
        );
        let c = app.get("container");
        assert_eq!((c.base.x, c.base.y, c.base.width), (10.0, 10.0, 100.0));
        let t = app.get("text");
        assert_eq!((t.base.x, t.base.y), (15.0, 15.0));

        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        assert_eq!(
            bound_text_state(&app),
            [
                row("container", json!([{"id": "text", "type": "text"}]), false),
                // we triggered rebind!
                row("text", json!("container"), false),
            ]
        );
    }
}

#[test]
fn should_rebind_remotely_added_container_when_its_bound_text_is_added_through_the_history() {
    let mut app = App::new();
    // Simulate local update
    app.update_scene(Some(vec![label()]), None, Some(Immediately));

    // Simulate remote update
    let t = app.at(0);
    let c = container();
    let remote = vec![
        app.with(
            &c,
            json!({"boundElements": [{"id": "text", "type": "text"}]}),
        ),
        app.with(&t, json!({"containerId": "container"})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(
            bound_text_state(&app),
            [
                // we triggered unbind - bindings from non deleted to deleted cannot exist!
                row("container", json!([]), false),
                // binding from deleted to non deleted is correct
                row("text", json!("container"), true),
            ]
        );

        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        assert_eq!(
            bound_text_state(&app),
            [
                // we triggered rebind!
                row("container", json!([{"id": "text", "type": "text"}]), false),
                row("text", json!("container"), false),
            ]
        );
    }
}

#[test]
fn should_preserve_latest_remotely_added_binding_and_unbind_previous_one_when_the_container_is_added_through_the_history(
) {
    let mut app = App::new();
    // Simulate local update
    app.update_scene(Some(vec![container()]), None, Some(Immediately));

    // Simulate remote update
    let c = app.at(0);
    let t = label();
    let remote = vec![
        app.with(
            &c,
            json!({"boundElements": [{"id": "text", "type": "text"}]}),
        ),
        app.with(&t, json!({"containerId": "container"})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (0, 1));
    assert_eq!(
        bound_text_state(&app),
        [
            row("container", json!([{"id": "text", "type": "text"}]), true),
            // unbound!
            row("text", Value::Null, false),
        ]
    );

    let mut remote_text = text("remoteText", "ola", 0.0, 0.0);
    if let ElementKind::Text(t) = &mut remote_text.kind {
        t.container_id = Some("container".into());
    }
    // Simulate remote update
    let (c, t) = (app.at(0), app.at(1));
    let remote = vec![
        app.with(
            &c,
            // purposefully undeleting, mimicking a concurrent update
            json!({"boundElements": [{"id": "remoteText", "type": "text"}], "isDeleted": false}),
        ),
        t,
        // rebinding the container with a new text element!
        remote_text,
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        assert_eq!(
            bound_text_state(&app),
            [
                // previously bound text is preserved, text bindings are not duplicated
                row(
                    "container",
                    json!([{"id": "remoteText", "type": "text"}]),
                    false
                ),
                // unbound
                row("text", Value::Null, false),
                // preserved existing binding!
                row("remoteText", json!("container"), false),
            ]
        );

        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(
            bound_text_state(&app),
            [
                row(
                    "container",
                    json!([{"id": "remoteText", "type": "text"}]),
                    false
                ),
                row("text", Value::Null, false),
                row("remoteText", json!("container"), false),
            ]
        );
    }
}

#[test]
fn should_preserve_latest_remotely_added_binding_and_unbind_previous_one_when_the_text_is_added_through_history(
) {
    let mut app = App::new();
    // Simulate local update
    app.update_scene(Some(vec![label()]), None, Some(Immediately));

    // Simulate remote update
    let t = app.at(0);
    let c = container();
    let remote = vec![
        app.with(
            &c,
            json!({"boundElements": [{"id": "text", "type": "text"}]}),
        ),
        app.with(&t, json!({"containerId": "container"})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    app.undo();
    assert_eq!(app.stacks(), (0, 1));
    assert_eq!(
        bound_text_state(&app),
        [
            // unbind affected bindable element
            row("container", json!([]), false),
            row("text", json!("container"), true),
        ]
    );

    let mut remote_text = text("remoteText", "ola", 0.0, 0.0);
    if let ElementKind::Text(t) = &mut remote_text.kind {
        t.container_id = Some("container".into());
    }
    // Simulate remote update
    let (c, t) = (app.at(0), app.at(1));
    let remote = vec![
        app.with(
            &c,
            json!({"boundElements": [{"id": "remoteText", "type": "text"}]}),
        ),
        t,
        app.with(&remote_text, json!({"containerId": "container"})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        assert_eq!(
            bound_text_state(&app),
            [
                // previously bound text is preserved, text bindings are not duplicated
                row(
                    "container",
                    json!([{"id": "remoteText", "type": "text"}]),
                    false
                ),
                // unbound from container!
                row("text", Value::Null, false),
                // preserved existing binding!
                row("remoteText", json!("container"), false),
            ]
        );

        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(
            bound_text_state(&app),
            [
                row(
                    "container",
                    json!([{"id": "remoteText", "type": "text"}]),
                    false
                ),
                row("text", json!("container"), true),
                row("remoteText", json!("container"), false),
            ]
        );
    }
}

#[test]
fn should_unbind_remotely_deleted_bound_text_from_container_when_the_container_is_added_through_the_history(
) {
    let mut app = App::new();
    // Simulate local update
    app.update_scene(Some(vec![container()]), None, Some(Immediately));

    // Simulate remote update
    let c = app.at(0);
    let t = label();
    let remote = vec![
        app.with(
            &c,
            json!({"boundElements": [{"id": "text", "type": "text"}]}),
        ),
        app.with(&t, json!({"containerId": "container", "isDeleted": true})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(
            bound_text_state(&app),
            [
                row("container", json!([{"id": "text", "type": "text"}]), true),
                row("text", json!("container"), true),
            ]
        );

        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        assert_eq!(
            bound_text_state(&app),
            [
                // unbound!
                row("container", json!([]), false),
                row("text", json!("container"), true),
            ]
        );
    }
}

#[test]
fn should_unbind_remotely_deleted_container_from_bound_text_when_the_text_is_added_through_the_history(
) {
    let mut app = App::new();
    // Simulate local update
    app.update_scene(Some(vec![label()]), None, Some(Immediately));

    // Simulate remote update
    let t = app.at(0);
    let c = container();
    let remote = vec![
        app.with(
            &c,
            json!({"boundElements": [{"id": "text", "type": "text"}], "isDeleted": true}),
        ),
        app.with(&t, json!({"containerId": "container"})),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(
            bound_text_state(&app),
            [
                row("container", json!([{"id": "text", "type": "text"}]), true),
                row("text", json!("container"), true),
            ]
        );

        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        assert_eq!(
            bound_text_state(&app),
            [
                row("container", json!([{"id": "text", "type": "text"}]), true),
                // unbound!
                row("text", Value::Null, false),
            ]
        );
    }
}

// ---------------------------------------------------------------------------
// conflicts in arrows and their bindable elements

fn arrow_scene() -> App {
    let mut app = App::new();
    let r1 = rect_sized("rect1", -100.0, -50.0, 100.0, 100.0);
    let r2 = rect_sized("rect2", 100.0, -50.0, 100.0, 100.0);
    // Simulate local update
    app.update_scene(Some(vec![r1, r2]), None, Some(Immediately));
    app
}

#[test]
fn should_rebind_remotely_added_arrow_when_its_bindable_elements_are_added_through_the_history() {
    let mut app = arrow_scene();
    let mut a = arrow("arrow", vec![[0.0, 0.0], [100.0, 100.0]]);
    if let Some(linear) = a.kind.linear_mut() {
        linear.start_binding = Some(binding("rect1", [1.0, 0.5]));
        linear.end_binding = Some(binding("rect2", [0.5, 1.0]));
    }
    // Simulate remote update
    let (r1, r2) = (app.at(0), app.at(1));
    let remote = vec![
        a,
        app.with(
            &r1,
            json!({"boundElements": [{"id": "arrow", "type": "arrow"}]}),
        ),
        app.with(
            &r2,
            json!({"boundElements": [{"id": "arrow", "type": "arrow"}]}),
        ),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        assert_eq!(app.ids(), ["arrow", "rect1", "rect2"]);
        assert_eq!(arrow_binding(&app, "startBinding"), Value::Null);
        assert_eq!(arrow_binding(&app, "endBinding"), Value::Null);
        for id in ["rect1", "rect2"] {
            let r = app.get(id);
            assert_eq!(
                bound_elements(&r),
                json!([{"id": "arrow", "type": "arrow"}])
            );
            assert!(r.base.is_deleted);
        }

        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        // now we are back in the previous state!
        assert_eq!(arrow_binding(&app, "startBinding")["elementId"], "rect1");
        assert_eq!(arrow_binding(&app, "endBinding")["elementId"], "rect2");
        for id in ["rect1", "rect2"] {
            let r = app.get(id);
            assert_eq!(
                bound_elements(&r),
                json!([{"id": "arrow", "type": "arrow"}])
            );
            assert!(!r.base.is_deleted);
        }
    }
}

#[test]
fn should_rebind_remotely_added_bindable_elements_when_its_arrow_is_added_through_the_history() {
    let mut app = arrow_scene();
    app.undo();
    let r1 = rect_sized("rect1", -100.0, -50.0, 100.0, 100.0);
    let r2 = rect_sized("rect2", 100.0, -50.0, 100.0, 100.0);
    let a = arrow("arrow", vec![[0.0, 0.0], [100.0, 100.0]]);
    // Simulate local update
    app.update_scene(Some(vec![a]), None, Some(Immediately));

    // Simulate remote update
    let a = app.at(0);
    let remote = vec![
        app.with(
            &a,
            json!({
                "startBinding": {"elementId": "rect1", "fixedPoint": [0.5, 1], "mode": "orbit"},
                "endBinding": {"elementId": "rect2", "fixedPoint": [1, 0.5], "mode": "orbit"},
            }),
        ),
        app.with(
            &r1,
            json!({"boundElements": [{"id": "arrow", "type": "arrow"}]}),
        ),
        app.with(
            &r2,
            json!({"boundElements": [{"id": "arrow", "type": "arrow"}]}),
        ),
    ];
    app.update_scene(Some(remote), None, Some(Never));

    for _ in 0..2 {
        app.undo();
        assert_eq!(app.stacks(), (0, 1));
        let a = app.get("arrow");
        assert_eq!(prop(&a, "startBinding")["elementId"], "rect1");
        assert_eq!(prop(&a, "endBinding")["elementId"], "rect2");
        assert!(a.base.is_deleted);
        for id in ["rect1", "rect2"] {
            let r = app.get(id);
            assert_eq!(bound_elements(&r), json!([]));
            assert!(!r.base.is_deleted);
        }

        app.redo();
        assert_eq!(app.stacks(), (1, 0));
        let a = app.get("arrow");
        assert_eq!(prop(&a, "startBinding")["elementId"], "rect1");
        assert_eq!(prop(&a, "endBinding")["elementId"], "rect2");
        assert!(!a.base.is_deleted);
        for id in ["rect1", "rect2"] {
            let r = app.get(id);
            assert_eq!(
                bound_elements(&r),
                json!([{"id": "arrow", "type": "arrow"}])
            );
            assert!(!r.base.is_deleted);
        }
    }
}

// ---------------------------------------------------------------------------
// conflicts in frames and their children

#[test]
fn should_not_rebind_frame_child_with_frame_when_frame_was_remotely_deleted_and_frame_child_is_added_back_through_the_history(
) {
    let mut app = App::new();
    let f = frame("frame", 0.0, 500.0);
    let r = rect_sized("rect", 10.0, 10.0, 100.0, 100.0);
    // Initialize the scene
    app.update_scene(Some(vec![f]), None, Some(Never));
    // Simulate local update
    let f = app.at(0);
    app.update_scene(Some(vec![r, f]), None, Some(Immediately));
    // Simulate local update
    let (r, f) = (app.at(0), app.at(1));
    let local = vec![app.with(&r, json!({"frameId": "frame"})), f];
    app.update_scene(Some(local), None, Some(Immediately));

    let state = |app: &App| -> Vec<(String, Value, bool)> {
        app.elements()
            .iter()
            .map(|e| (e.base.id.clone(), prop(e, "frameId"), e.base.is_deleted))
            .collect()
    };

    app.undo();
    assert_eq!(app.stacks(), (1, 1));
    assert_eq!(
        state(&app),
        [
            row("rect", Value::Null, false),
            row("frame", Value::Null, false)
        ]
    );

    app.redo();
    assert_eq!(app.stacks(), (2, 0));
    assert_eq!(
        state(&app),
        [
            // double check that the element is rebound
            row("rect", json!("frame"), false),
            row("frame", Value::Null, false)
        ]
    );

    app.undo();
    app.undo();

    // Simulate remote update
    let (r, f) = (app.at(0), app.at(1));
    let remote = vec![r, app.with(&f, json!({"isDeleted": true}))];
    app.update_scene(Some(remote), None, Some(Never));

    app.redo();
    app.redo();
    assert_eq!(app.stacks(), (2, 0));
    assert_eq!(
        state(&app),
        [
            // the element is not bound to the deleted frame
            row("rect", Value::Null, false),
            row("frame", Value::Null, true)
        ]
    );
}

// ---------------------------------------------------------------------------
// version bumps through the history

#[test]
fn every_undo_and_redo_bumps_version_nonce_and_updated() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);
    app.background(RED);
    let before = app.get("rect");

    app.undo();
    let undone = app.get("rect");
    // history never goes back to a previous version: each undo is a new
    // user action for collaborators (history.ts:26-33)
    assert!(undone.base.version > before.base.version);
    assert_ne!(undone.base.version_nonce, before.base.version_nonce);
    assert_eq!(undone.base.updated, 1.0);

    app.redo();
    let redone = app.get("rect");
    assert!(redone.base.version > undone.base.version);
    assert_ne!(redone.base.version_nonce, undone.base.version_nonce);
}

#[test]
fn undo_is_ignored_while_an_interaction_is_in_progress() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);
    app.update_scene(
        None,
        Some(json!({"selectedElementsAreBeingDragged": true})),
        None,
    );
    app.undo();
    assert_eq!(app.stacks(), (1, 0));
    assert!(!app.get("rect").base.is_deleted);
    app.update_scene(
        None,
        Some(json!({"selectedElementsAreBeingDragged": false})),
        None,
    );
    app.undo();
    assert_eq!(app.stacks(), (0, 1));
    assert!(app.get("rect").base.is_deleted);
}

#[test]
fn history_changed_events_report_the_stacks() {
    let mut app = App::new();
    app.s.history.take_events();
    app.create_rect("rect", 10.0, 10.0);
    let events = app.s.history.take_events();
    assert_eq!(events.len(), 1);
    assert!(!events[0].is_undo_stack_empty);
    assert!(events[0].is_redo_stack_empty);
    app.undo();
    let events = app.s.history.take_events();
    assert_eq!(events.len(), 1, "one event per undo, not per entry");
    assert!(events[0].is_undo_stack_empty);
    assert!(!events[0].is_redo_stack_empty);
}

#[test]
fn store_delta_round_trips_through_its_dto() {
    let mut app = App::new();
    app.create_rect("rect", 10.0, 10.0);
    let entry = app.s.history.undo_stack[0].clone();
    let dto = entry.to_dto();
    assert_eq!(
        dto["elements"]["removed"]["rect"]["inserted"]["isDeleted"],
        json!(true)
    );
    let restored = StoreDelta::from_dto(&dto).unwrap();
    assert_eq!(restored.to_dto(), dto);
    let _: FractionalIndex = FractionalIndex("a0".into());
}
