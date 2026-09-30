//! The `perform`s of the editing actions (`packages/excalidraw/actions/*`)
//! the element runs from its keys and menus: each takes the scene's
//! elements (deleted ones included, in order) and app state and returns
//! upstream's `ActionResult`, the elements to write (when they changed) and
//! the app state keys to set, or `None` where upstream's `perform` returns
//! `false`.
//!
//! - `actionSelectAll`: [`select_all`];
//! - `actionDeleteSelected`: [`delete_selected`];
//! - `actionDuplicateSelection`: [`duplicate_selection`], over
//!   `duplicateElements` ([`duplicate::duplicate_elements`]);
//! - `actionGroup` and `actionUngroup`: [`group`] and [`ungroup`];
//! - the z-order actions of `actionZindex.tsx`: [`bring_to_front`],
//!   [`send_to_back`], [`bring_forward`] and [`send_backward`];
//! - the clipboard JSON `actionCopy` writes and the paste insertion of
//!   `App.addElementsFromPasteOrLibrary`: [`copy_selected`] and
//!   [`paste_elements`];
//! - inserting library items (`LibraryMenuItems`'s click, App's drop,
//!   `distributeLibraryItemsOnSquareGrid`): [`insert_library_items`] and
//!   [`distribute_library_items_on_square_grid`];
//! - the styles panel's actions (`actionProperties.tsx`, `togglePolygon`,
//!   `actionAlign.tsx`, `actionDistribute.tsx`, `actionLink.tsx`,
//!   `actionCropEditor.tsx`): [`perform_style_action`], over a
//!   [`StyleEnv`] that lays text out.
//!
//! Upstream mutates elements in place (`mutateElement`) or copies them
//! (`newElementWith`); either way a changed element gets `version + 1`, a
//! fresh `versionNonce` and `updated` now, drawn from the [`EditEnv`] in
//! upstream's order.

mod align;
mod clipboard;
mod delete;
pub mod duplicate;
mod grouping;
mod library;
mod properties;
mod zindex;

use std::collections::HashSet;

use excali_core::app_state::AppState;
use excali_core::element::{Element, ElementKind};
use excali_core::fractional_index::ChangeStamp;
use excali_core::restore::RestoreEnv;
use excali_scene::frame::is_frame_like;
use serde_json::{Map, Value};

use crate::groups::select_groups_for_selected_elements;
use crate::mutate::bump_version;
use crate::scene::MutationEnv;
use crate::store::CaptureUpdateAction;

pub use clipboard::{copy_selected, paste_elements};
pub use delete::delete_selected;
pub use duplicate::duplicate_selection;
pub(crate) use grouping::{
    add_elements_to_frame, get_common_frame_id, get_elements_in_resizing_frame,
    get_frame_children_insertion_index, omit_groups_containing_frame_likes,
    remove_elements_from_frame, replace_all_elements_in_frame,
};
pub use grouping::{group, ungroup};
pub use library::{
    distribute_library_items_on_square_grid, duplicate_library_items, insert_library_items,
};
pub use properties::{perform_style_action, StyleEnv};
pub use zindex::{bring_forward, bring_to_front, send_backward, send_to_back};

/// Where the edit actions draw what upstream draws: new ids (`randomId()`,
/// [`RestoreEnv::random_id`]), seeds (`randomInteger()`,
/// [`RestoreEnv::random_integer`]), the version nonces and timestamps of
/// changed elements ([`ChangeStamp`], and [`MutationEnv`] where an update
/// goes through [`crate::scene::Scene::mutate_element`]), and what
/// restoring pasted elements draws ([`RestoreEnv`]). Upstream draws all
/// its random integers from one generator, so an environment whose three
/// integer sources share one reproduces upstream's sequence. Any
/// environment that restores, mutates and stamps elements is one (the
/// element's `EditorEnv` included).
pub trait EditEnv: RestoreEnv + MutationEnv + ChangeStamp {}

impl<T: RestoreEnv + MutationEnv + ChangeStamp> EditEnv for T {}

/// What an action's `perform` returns (`ActionResult`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActionResult {
    /// The scene's elements after the action, deleted ones included, or
    /// `None` when the action left them.
    pub elements: Option<Vec<Element>>,
    /// The app state keys the action sets.
    pub app_state: Map<String, Value>,
    /// `captureUpdate: IMMEDIATELY` (else `EVENTUALLY`, or `NEVER` with
    /// [`ActionResult::never`]).
    pub capture: bool,
    /// `captureUpdate: NEVER` (the update is never undoable, as the font
    /// picker's reset of what hovering previewed); only read when
    /// `capture` is false.
    pub never: bool,
}

impl ActionResult {
    /// Upstream's `captureUpdate`.
    pub fn capture_update(&self) -> CaptureUpdateAction {
        if self.capture {
            CaptureUpdateAction::Immediately
        } else if self.never {
            CaptureUpdateAction::Never
        } else {
            CaptureUpdateAction::Eventually
        }
    }

    /// `{ appState, elements, captureUpdate: EVENTUALLY }` with both as
    /// they were: nothing to do.
    fn unchanged() -> ActionResult {
        ActionResult::default()
    }
}

fn is_editing_linear_element(app_state: &AppState) -> bool {
    app_state
        .get("selectedLinearElement")
        .and_then(|l| l.get("isEditing"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// JavaScript truthiness of an app state value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// An app state object key (`selectedElementIds`, `selectedGroupIds`), or
/// an empty one.
fn object_key(app_state: &AppState, key: &str) -> Map<String, Value> {
    app_state
        .get(key)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// `appState.editingGroupId` when it is a (non-empty) group id.
fn editing_group_id(app_state: &AppState) -> Option<String> {
    app_state
        .get("editingGroupId")
        .and_then(Value::as_str)
        .filter(|g| !g.is_empty())
        .map(str::to_owned)
}

/// `ids[id]` is truthy.
fn is_true(ids: &Map<String, Value>, id: &str) -> bool {
    truthy(ids.get(id))
}

/// `isBoundToContainer(element)` (`typeChecks.ts:307-316`): a text whose
/// `containerId` is not `null`.
fn is_bound_to_container(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Text(t) if t.container_id.is_some())
}

/// A text's `containerId`, when truthy.
fn container_id(element: &Element) -> Option<&str> {
    match &element.kind {
        ElementKind::Text(t) => t.container_id.as_deref().filter(|c| !c.is_empty()),
        _ => None,
    }
}

/// `element.frameId`, when truthy.
fn frame_id(element: &Element) -> Option<&str> {
    element.base.frame_id.as_deref().filter(|f| !f.is_empty())
}

/// `isLinearElement(element)`: a line or an arrow.
fn is_linear(element: &Element) -> bool {
    matches!(element.kind, ElementKind::Line(_) | ElementKind::Arrow(_))
}

/// `isElbowArrow(element)`.
fn is_elbow_arrow(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Arrow(a) if a.elbowed)
}

/// The version bump of `mutateElement` and `newElementWith`: `version + 1`,
/// a fresh `versionNonce`, `updated` now.
fn bump<E: ChangeStamp>(element: &mut Element, env: &mut E) {
    bump_version(element, None, env);
}

/// `mutateElement(element, elementsMap, { frameId })`: set unless equal.
fn set_frame_id<E: ChangeStamp>(element: &mut Element, frame: Option<&str>, env: &mut E) {
    if element.base.frame_id.as_deref() != frame {
        element.base.frame_id = frame.map(str::to_owned);
        bump(element, env);
    }
}

/// `getFrameChildren(allElements, frameId)` (`frame.ts:242-253`), deleted
/// elements included.
fn get_frame_children<'a>(elements: &[&'a Element], frame: &str) -> Vec<&'a Element> {
    elements
        .iter()
        .copied()
        .filter(|e| e.base.frame_id.as_deref() == Some(frame))
        .collect()
}

/// `getSelectedElements(elements, { selectedElementIds }, opts)` over the
/// non-deleted elements, as copies: the selected elements, with the text
/// bound to a selected container (`include_bound_text`) and each selected
/// frame preceded by its children (`include_elements_in_frames`).
pub fn selected_elements(
    elements: &[Element],
    selected_ids: &Map<String, Value>,
    include_bound_text: bool,
    include_elements_in_frames: bool,
) -> Vec<Element> {
    let live = non_deleted(elements);
    get_selected_elements(
        &live,
        selected_ids,
        include_bound_text,
        include_elements_in_frames,
    )
    .into_iter()
    .cloned()
    .collect()
}

/// `getSelectedElements(elements, appState, opts)` (`selection.ts:161-210`,
/// also `scene.getSelectedElements`, over the non-deleted elements): the
/// selected elements (deleted ones never), with the text bound to a
/// selected container, and each selected frame preceded by its children.
fn get_selected_elements<'a>(
    elements: &[&'a Element],
    selected_ids: &Map<String, Value>,
    include_bound_text: bool,
    include_elements_in_frames: bool,
) -> Vec<&'a Element> {
    let mut added: HashSet<&str> = HashSet::new();
    let mut selected: Vec<&Element> = Vec::new();
    for &element in elements {
        if is_true(selected_ids, &element.base.id) {
            // selection can only contain non-deleted elements
            if !element.base.is_deleted {
                selected.push(element);
                added.insert(&element.base.id);
            }
            continue;
        }
        if include_bound_text
            && !element.base.is_deleted
            && is_bound_to_container(element)
            && container_id(element).is_some_and(|c| is_true(selected_ids, c))
        {
            selected.push(element);
            added.insert(&element.base.id);
        }
    }
    if !include_elements_in_frames {
        return selected;
    }
    let mut with_children = Vec::new();
    for element in selected {
        if is_frame_like(element) {
            for child in get_frame_children(elements, &element.base.id) {
                if !added.contains(child.base.id.as_str()) {
                    with_children.push(child);
                }
            }
        }
        with_children.push(element);
    }
    with_children
}

/// `isSomeElementSelected(elements, appState)` over the non-deleted
/// elements.
fn is_some_element_selected(elements: &[Element], selected_ids: &Map<String, Value>) -> bool {
    elements
        .iter()
        .any(|e| !e.base.is_deleted && is_true(selected_ids, &e.base.id))
}

/// `excludeElementsInFramesFromSelection(selectedElements)`
/// (`selection.ts:54-68`): the children of selected frames left out.
fn exclude_elements_in_frames_from_selection<'a>(selected: &[&'a Element]) -> Vec<&'a Element> {
    let frames: HashSet<&str> = selected
        .iter()
        .filter(|e| is_frame_like(e))
        .map(|e| e.base.id.as_str())
        .collect();
    selected
        .iter()
        .copied()
        .filter(|e| !frame_id(e).is_some_and(|f| frames.contains(f)))
        .collect()
}

/// `getSelectionStateForElements(targetElements, allElements, appState)`
/// (`selection.ts:268-293`): the targets selected (their frames' children
/// and bound text left out) with their groups, and the linear element
/// editor (`{ elementId, isEditing: false }`) when the targets are one
/// linear element and what is bound to it.
fn selection_state_for_elements(
    targets: &[&Element],
    all: &[&Element],
    app_state: &AppState,
) -> Map<String, Value> {
    let linears: Vec<&Element> = targets.iter().copied().filter(|e| is_linear(e)).collect();
    let linear_editor = match linears.as_slice() {
        [linear] => {
            let bound: Vec<&str> = linear
                .base
                .bound_elements
                .iter()
                .flatten()
                .map(|b| b.id.as_str())
                .collect();
            targets
                .iter()
                .all(|e| e.base.id == linear.base.id || bound.contains(&e.base.id.as_str()))
                .then(|| serde_json::json!({ "elementId": linear.base.id, "isEditing": false }))
        }
        _ => None,
    };
    let mut ids = Map::new();
    for element in exclude_elements_in_frames_from_selection(targets) {
        if !is_bound_to_container(element) {
            ids.insert(element.base.id.clone(), Value::Bool(true));
        }
    }
    let editing = editing_group_id(app_state);
    let groups = select_groups_for_selected_elements(&ids, editing.as_deref(), all);
    let mut patch = Map::new();
    patch.insert(
        "selectedLinearElement".into(),
        linear_editor.unwrap_or(Value::Null),
    );
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
    patch
}

/// The non-deleted elements (`getNonDeletedElements`).
fn non_deleted(elements: &[Element]) -> Vec<&Element> {
    elements.iter().filter(|e| !e.base.is_deleted).collect()
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
        never: false,
    })
}
