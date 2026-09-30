//! The stats panel's element property edits in the editor
//! (`components/Stats/DragInput.tsx`): a typed value runs the component's
//! callback once and is captured as one history entry
//! (`app.syncActionResult({ captureUpdate: IMMEDIATELY })`); a label drag
//! runs it on every pointer move, from the copies taken at the press, and
//! is captured once at the release, after the `dragFinishedCallback`.

use excali_editor::scene::Scene;
use excali_editor::stats::{
    grid_step_after_drag, HighlightPatch, StatsChange, StatsDrag, StatsGesture, StatsProperty,
    GRID_STEP_SENSITIVITY,
};
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{json, Value};

use crate::editor::Editor;

/// A label pressed in the stats panel.
pub(crate) enum StatsDragState {
    /// An element property: the gesture's copies and the pointer.
    Element(StatsGesture, StatsDrag),
    /// The grid step (`CanvasGrid`), from the app state as it is.
    GridStep(StatsDrag),
}

/// `DragInput`'s `sensitivity` for the element properties (its default).
const SENSITIVITY: f64 = 1.0;

impl<P: TextMetricsProvider + Clone> Editor<P> {
    /// A typed value committed in the stats panel's input for `property`
    /// (`handleInputValue`, `DragInput.tsx:122-173`, once the panel's
    /// `typed_value` accepted it). Returns whether an input took it.
    pub fn stats_input(&mut self, property: StatsProperty, value: f64) -> bool {
        let mut scene = Scene::new(self.session.elements().to_vec());
        let app_state = self.session.app_state().clone();
        let Some(gesture) = StatsGesture::begin(&scene, &app_state, property) else {
            return false;
        };
        let patch = gesture.apply(
            &mut scene,
            &app_state,
            StatsChange::typed(value),
            &mut self.session.env,
        );
        self.session.store.schedule_capture();
        self.apply(scene, app_state);
        self.set_highlight(patch);
        self.session.commit();
        self.report();
        true
    }

    /// A press on the label of the input for `property`: the component's
    /// elements, the scene and the app state copied for the drag.
    pub fn stats_drag_start(&mut self, property: StatsProperty) -> bool {
        if property == StatsProperty::GridStep {
            self.stats_drag = Some(StatsDragState::GridStep(StatsDrag::new()));
            return true;
        }
        let scene = Scene::new(self.session.elements().to_vec());
        self.stats_drag = StatsGesture::begin(&scene, self.session.app_state(), property)
            .map(|gesture| StatsDragState::Element(gesture, StatsDrag::new()));
        self.stats_drag.is_some()
    }

    /// A pointer move while the label is pressed.
    pub fn stats_drag_move(&mut self, client_x: f64, shift: bool) {
        let (gesture, mut drag) = match self.stats_drag.take() {
            Some(StatsDragState::Element(gesture, drag)) => (gesture, drag),
            Some(StatsDragState::GridStep(mut drag)) => {
                if let Some((_, instant)) = drag.pointer_move(client_x, GRID_STEP_SENSITIVITY) {
                    let step = self.session.app_state().grid_step().unwrap_or(f64::NAN);
                    if let Some(next) = grid_step_after_drag(step, instant, shift) {
                        self.set_keys(vec![("gridStep", json!(next))]);
                        self.session.commit();
                    }
                }
                self.stats_drag = Some(StatsDragState::GridStep(drag));
                return;
            }
            None => return,
        };
        if let Some((accumulated_change, instant_change)) = drag.pointer_move(client_x, SENSITIVITY)
        {
            let mut scene = Scene::new(self.session.elements().to_vec());
            let app_state = self.session.app_state().clone();
            let change = StatsChange {
                accumulated_change,
                instant_change,
                should_change_by_step_size: shift,
                next_value: None,
            };
            let patch = gesture.apply(&mut scene, &app_state, change, &mut self.session.env);
            self.apply(scene, app_state);
            self.set_highlight(patch);
            self.session.commit();
            self.report();
        }
        self.stats_drag = Some(StatsDragState::Element(gesture, drag));
    }

    /// The release: the `dragFinishedCallback`, then one history entry for
    /// the drag.
    pub fn stats_drag_end(&mut self) {
        let gesture = match self.stats_drag.take() {
            Some(StatsDragState::Element(gesture, _)) => gesture,
            Some(StatsDragState::GridStep(_)) => {
                self.session.commit();
                return;
            }
            None => return,
        };
        let mut scene = Scene::new(self.session.elements().to_vec());
        let app_state = self.session.app_state().clone();
        let patch = gesture.finish(&mut scene, &mut self.session.env);
        self.session.store.schedule_capture();
        self.apply(scene, app_state);
        self.set_highlight(patch);
        self.session.commit();
        self.report();
    }

    /// `setAppState({ elementsToHighlight })`: the elements, or `null`.
    fn set_highlight(&mut self, patch: Option<HighlightPatch>) {
        let Some(patch) = patch else {
            return;
        };
        let value = match patch {
            Some(ids) => Value::Array(
                ids.iter()
                    .map(|id| self.element_value(Some(id)))
                    .filter(|v| !v.is_null())
                    .collect(),
            ),
            None => Value::Null,
        };
        self.set_keys(vec![("elementsToHighlight", value)]);
    }
}
