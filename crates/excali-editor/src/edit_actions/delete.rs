//! `actionDeleteSelected` (`packages/excalidraw/actions/actionDeleteSelected.tsx`).

use std::collections::HashSet;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_scene::frame::is_frame_like;
use serde_json::{Map, Value};

use super::{
    bump, container_id, editing_group_id, frame_id, get_selected_elements, is_bound_to_container,
    is_editing_linear_element, is_elbow_arrow, is_some_element_selected, is_true, object_key,
    ActionResult, EditEnv,
};
use crate::binding::fix_bindings_after_deletion;
use crate::groups::select_groups_for_selected_elements;
use crate::scene::{ElementUpdate, Scene};

/// An element of the next scene: the scene's own (which later in-place
/// mutations still reach) or a copy (`newElementWith`).
enum Next {
    Same,
    New(Box<Element>),
}

/// The non-deleted element `id` of the scene (`getNonDeletedElementsMap`).
fn live<'a>(scene: &'a Scene, id: &str) -> Option<&'a Element> {
    scene.get_non_deleted(id)
}

/// `newElementWith(element, { isDeleted: true })`: unchanged when it is
/// deleted already.
fn deleted_copy<E: EditEnv>(element: &Element, env: &mut E) -> Next {
    if element.base.is_deleted {
        return Next::Same;
    }
    let mut copy = element.clone();
    copy.base.is_deleted = true;
    bump(&mut copy, env);
    Next::New(Box::new(copy))
}

/// `getElementsInGroup(elements, groupId)` of the non-deleted elements.
fn live_in_group<'a>(elements: &'a [Element], group: &str) -> Vec<&'a Element> {
    elements
        .iter()
        .filter(|e| !e.base.is_deleted && e.base.group_ids.iter().any(|g| g == group))
        .collect()
}

/// `deleteSelectedElements(elements, appState, app)`
/// (`actionDeleteSelected.tsx:39-178`): the next elements, and the
/// selection and editing group to select through
/// `selectGroupsForSelectedElements`.
fn delete_selected_elements<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    selected: &Map<String, Value>,
    env: &mut E,
) -> (Vec<Element>, Map<String, Value>, Option<String>) {
    let all: Vec<&Element> = elements.iter().collect();
    let frames: Vec<&Element> = all.iter().copied().filter(|e| is_frame_like(e)).collect();
    let frames_to_delete: Vec<String> = get_selected_elements(&frames, selected, false, false)
        .iter()
        .map(|e| e.base.id.clone())
        .collect();
    let deleting_frame =
        |id: Option<&str>| id.is_some_and(|f| frames_to_delete.iter().any(|d| d == f));

    let mut scene = Scene::new(elements.to_vec());
    let mut selected_ids = Map::new();
    let mut processed: HashSet<String> = HashSet::new();
    for frame in &frames_to_delete {
        for child in all
            .iter()
            .filter(|e| e.base.frame_id.as_deref() == Some(frame))
        {
            if processed.contains(&child.base.id) {
                continue;
            }
            if is_bound_to_container(child) {
                if let Some(container) = container_id(child).and_then(|c| live(&scene, c)) {
                    selected_ids.insert(container.base.id.clone(), Value::Bool(true));
                }
            } else {
                selected_ids.insert(child.base.id.clone(), Value::Bool(true));
            }
            processed.insert(child.base.id.clone());
        }
    }

    let mut should_select_editing_group = true;
    let mut next: Vec<Next> = Vec::with_capacity(elements.len());
    for i in 0..elements.len() {
        let element = scene.elements()[i].clone();
        if is_true(selected, &element.base.id) {
            let container_frame = if is_bound_to_container(&element) {
                container_id(&element)
                    .and_then(|c| live(&scene, c))
                    .and_then(|c| frame_id(c).map(str::to_owned))
            } else {
                None
            };
            if deleting_frame(frame_id(&element)) {
                should_select_editing_group = false;
                selected_ids.insert(element.base.id.clone(), Value::Bool(true));
                next.push(Next::Same);
                continue;
            }
            if deleting_frame(container_frame.as_deref()) {
                next.push(Next::Same);
                continue;
            }
            // elbow arrows bound to the element let go of it (in place)
            for candidate in element.base.bound_elements.iter().flatten() {
                let Some(bound) = live(&scene, &candidate.id) else {
                    continue;
                };
                if !is_elbow_arrow(bound) {
                    continue;
                }
                let Some(linear) = bound.kind.linear() else {
                    continue;
                };
                let keep = |b: &Option<excali_core::element::FixedPointBinding>| match b {
                    Some(b) if b.element_id == element.base.id => None,
                    other => other.clone(),
                };
                let update = ElementUpdate {
                    start_binding: Some(keep(&linear.start_binding)),
                    end_binding: Some(keep(&linear.end_binding)),
                    ..ElementUpdate::default()
                };
                let id = bound.base.id.clone();
                scene.mutate_element(&id, update, env);
            }
            next.push(deleted_copy(&element, env));
            continue;
        }

        // if deleting a frame, remove the children from it and select them
        if deleting_frame(frame_id(&element)) {
            should_select_editing_group = false;
            if !is_bound_to_container(&element) {
                selected_ids.insert(element.base.id.clone(), Value::Bool(true));
            }
            let mut copy = element.clone();
            copy.base.frame_id = None;
            bump(&mut copy, env);
            next.push(Next::New(Box::new(copy)));
            continue;
        }

        let container_selected = match &element.kind {
            excali_core::element::ElementKind::Text(t) => t
                .container_id
                .as_deref()
                .is_some_and(|c| is_true(selected, c)),
            _ => false,
        };
        if is_bound_to_container(&element) && container_selected {
            next.push(deleted_copy(&element, env));
            continue;
        }
        next.push(Next::Same);
    }
    let next_elements: Vec<Element> = next
        .into_iter()
        .enumerate()
        .map(|(i, n)| match n {
            Next::Same => scene.elements()[i].clone(),
            Next::New(e) => *e,
        })
        .collect();

    let mut next_editing_group_id = editing_group_id(app_state);
    // select next eligible element in currently editing group or supergroup
    if should_select_editing_group {
        if let Some(editing) = next_editing_group_id.clone() {
            let elems = live_in_group(&next_elements, &editing);
            if elems.len() > 1 {
                selected_ids.insert(elems[0].base.id.clone(), Value::Bool(true));
            } else {
                next_editing_group_id = None;
                if let Some(last) = elems.first() {
                    selected_ids.insert(last.base.id.clone(), Value::Bool(true));
                    let index = last.base.group_ids.iter().position(|g| *g == editing);
                    let super_index = index.map_or(0, |i| i + 1);
                    if let Some(super_group) = last.base.group_ids.get(super_index) {
                        let supers = live_in_group(&next_elements, super_group);
                        if supers.len() > 1 {
                            next_editing_group_id = Some(super_group.clone());
                            for e in supers {
                                selected_ids.insert(e.base.id.clone(), Value::Bool(true));
                            }
                        }
                    }
                }
            }
        }
    }
    (next_elements, selected_ids, next_editing_group_id)
}

/// `actionDeleteSelected.perform` (`actionDeleteSelected.tsx:203-306`):
/// the selected elements deleted (`deleteSelectedElements`): a selected
/// frame's children kept, taken out of it and selected; the text bound to
/// a deleted container deleted with it; elbow arrows bound to a deleted
/// element unbound from it; while a group is edited, the next element of
/// it (or of its super group) selected. Then the bindings of and to the
/// deleted elements are dropped (`fixBindingsAfterDeletion`), and while a
/// group is still being edited its first element is selected
/// (`handleGroupEditingState`).
///
/// Sets `selectedElementIds`, `selectedGroupIds`, `editingGroupId`,
/// `multiElement`, `newElement`, `activeEmbeddable` and
/// `selectedLinearElement` (to `null`); upstream also sets `activeTool`
/// back to the preferred selection tool (`updateActiveTool`), which is the
/// caller's (the tool state lives in [`crate::tools`]). Captured
/// (`IMMEDIATELY`) only when something was selected.
///
/// Not ported: while a linear element is being edited upstream deletes its
/// selected points (`LinearElementEditor.deletePoints`) instead; the port
/// returns `None` there.
pub fn delete_selected<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    env: &mut E,
) -> Option<ActionResult> {
    if is_editing_linear_element(app_state) {
        return None;
    }
    let selected = object_key(app_state, "selectedElementIds");
    let (next, selected_ids, editing) =
        delete_selected_elements(elements, app_state, &selected, env);
    let groups = {
        let live: Vec<&Element> = next.iter().filter(|e| !e.base.is_deleted).collect();
        select_groups_for_selected_elements(&selected_ids, editing.as_deref(), &live)
    };

    let deleted: Vec<String> = next
        .iter()
        .filter(|e| e.base.is_deleted)
        .map(|e| e.base.id.clone())
        .collect();
    let mut scene = Scene::new(next);
    fix_bindings_after_deletion(&mut scene, env, &deleted);
    let next = scene.elements().to_vec();

    // handleGroupEditingState
    let mut selected_element_ids = groups.selected_element_ids;
    if let Some(editing) = groups.editing_group_id.as_deref() {
        if let Some(first) = live_in_group(&next, editing).first() {
            selected_element_ids = Map::new();
            selected_element_ids.insert(first.base.id.clone(), Value::Bool(true));
        }
    }

    let mut patch = Map::new();
    patch.insert(
        "selectedElementIds".into(),
        Value::Object(selected_element_ids),
    );
    patch.insert(
        "selectedGroupIds".into(),
        Value::Object(groups.selected_group_ids),
    );
    patch.insert(
        "editingGroupId".into(),
        groups.editing_group_id.map_or(Value::Null, Value::String),
    );
    for key in [
        "multiElement",
        "newElement",
        "activeEmbeddable",
        "selectedLinearElement",
    ] {
        patch.insert(key.into(), Value::Null);
    }
    Some(ActionResult {
        elements: Some(next),
        app_state: patch,
        capture: is_some_element_selected(elements, &selected),
        never: false,
    })
}
