//! The `perform`s of the styles panel's actions.

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_core::fractional_index::SceneElementsMap;
use serde_json::Value;

use super::{ActionResult, EditEnv};
use crate::actions::ActionName;

/// What the property actions draw and measure besides [`EditEnv`]: text
/// layout and arrow routing.
pub trait StyleEnv: EditEnv + crate::binding::BindingEnv {
    /// `redrawTextBoundingBox(text, container, scene)` over `elements`.
    fn redraw_text_bounding_box(
        &mut self,
        elements: &mut SceneElementsMap,
        text_id: &str,
        container_id: Option<&str>,
    ) -> Result<(), String>;
}

/// `action.perform(elements, appState, value, app)` for the styles panel's
/// actions; `None` for another action or where upstream returns `false`.
pub fn perform_style_action<E: StyleEnv>(
    _name: ActionName,
    _elements: &[Element],
    _app_state: &AppState,
    _value: &Value,
    _env: &mut E,
) -> Option<ActionResult> {
    None
}
