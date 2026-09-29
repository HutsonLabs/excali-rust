//! The `perform`s of the editing actions (`packages/excalidraw/actions/*`)
//! the element runs from its keys and menus: each takes the scene's
//! elements and app state and returns upstream's `ActionResult`, the
//! elements to write (when they changed) and the app state keys to set.

use excali_core::app_state::AppState;
use excali_core::element::{Element, ElementKind};
use serde_json::{Map, Value};

use crate::groups::select_groups_for_selected_elements;

/// What an action's `perform` returns (`ActionResult`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActionResult {
    /// The scene's elements after the action, deleted ones included, or
    /// `None` when the action left them.
    pub elements: Option<Vec<Element>>,
    /// The app state keys the action sets.
    pub app_state: Map<String, Value>,
    /// `captureUpdate: IMMEDIATELY` (else `EVENTUALLY`).
    pub capture: bool,
}

fn is_editing_linear_element(app_state: &AppState) -> bool {
    app_state
        .get("selectedLinearElement")
        .and_then(|l| l.get("isEditing"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// `actionSelectAll.perform` (`actions/actionSelectAll.ts:26-68`): every
/// element but deleted ones, bound text and locked ones, their groups
/// selected whole; the linear element editor for a lone linear element.
/// Nothing while a linear element is being edited.
pub fn select_all(elements: &[Element], app_state: &AppState) -> Option<ActionResult> {
    if is_editing_linear_element(app_state) {
        return None;
    }
    let selected: Map<String, Value> = elements
        .iter()
        .filter(|e| {
            !e.base.is_deleted
                && !matches!(&e.kind, ElementKind::Text(t) if t.container_id.is_some())
                && !e.base.locked
        })
        .map(|e| (e.base.id.clone(), Value::Bool(true)))
        .collect();
    let live: Vec<&Element> = elements.iter().filter(|e| !e.base.is_deleted).collect();
    let groups = select_groups_for_selected_elements(&selected, None, &live);
    let mut patch = Map::new();
    patch.insert(
        "selectedElementIds".into(),
        Value::Object(groups.selected_element_ids),
    );
    patch.insert(
        "selectedGroupIds".into(),
        Value::Object(groups.selected_group_ids),
    );
    patch.insert(
        "editingGroupId".into(),
        groups.editing_group_id.map_or(Value::Null, Value::String),
    );
    // a lone linear element (upstream tests elements[0]) opens its editor
    let lone_linear = selected.len() == 1
        && elements
            .first()
            .is_some_and(|e| matches!(e.kind, ElementKind::Line(_) | ElementKind::Arrow(_)));
    patch.insert(
        "selectedLinearElement".into(),
        if lone_linear {
            serde_json::json!({
                "elementId": elements[0].base.id,
                "isEditing": false,
            })
        } else {
            Value::Null
        },
    );
    Some(ActionResult {
        elements: None,
        app_state: patch,
        capture: true,
    })
}
