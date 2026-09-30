//! The `perform`s of the actions behind the deselect, flip, element lock,
//! copy and paste styles, view mode and theme shortcuts, behind
//! [`perform_shortcut_action`]:
//!
//! - `actionDeselect` (`actionDeselect.ts:65-128`, Esc): [`deselect`];
//! - `actionFlipHorizontal` and `actionFlipVertical` (`actionFlip.ts`,
//!   Shift+H and Shift+V): [`flip`];
//! - `actionToggleElementLock` (`actionElementLock.ts:26-157`,
//!   Ctrl/Cmd+Shift+L): [`toggle_element_lock`];
//! - `actionCopyStyles` and `actionPasteStyles` (`actionStyles.ts`,
//!   Ctrl/Cmd+Alt+C and V): [`copy_styles`] and the paste in
//!   [`super::properties`];
//! - `actionToggleViewMode` (`actionToggleViewMode.tsx`, Alt+R) and
//!   `actionToggleTheme` (`actionCanvas.tsx:429-457`, Alt+Shift+D).
//!
//! Upstream's module-level `copiedStyles` and `app.props.onThemeChange`
//! are the host's: [`ShortcutHost`].

use std::collections::HashSet;

use excali_core::app_state::AppState;
use excali_core::element::{Element, ElementKind};
use excali_core::restore::RestoreEnv;
use excali_scene::bounds::{get_bound_text_element, ElementsMap};
use serde_json::{json, Map, Value};

use super::properties::{paste_styles, Triggering};
use super::{
    editing_group_id, get_selected_elements, is_elbow_arrow, is_true, non_deleted, object_key,
    truthy, ActionResult, StyleEnv,
};
use crate::actions::{elements_are_in_same_group, has_bound_text_element, ActionName};
use crate::binding::bind_or_unbind_binding_elements;
use crate::frame::{update_frame_membership_of_selected_elements, MembershipState};
use crate::groups::{get_elements_in_group, select_groups_for_selected_elements};
use crate::keyboard::binding_app_state;
use crate::mutate::new_element_with;
use crate::resize_elements::{
    get_common_bounding_box, original_selection_with_labels, resize_multiple_elements,
    MultipleResize, TransformEnv,
};
use crate::scene::{ElementUpdate, Scene};
use crate::tools::TOGGLE_TOOLS;
use crate::transform_handles::TransformHandleDirection;

/// What the shortcut actions draw, measure and lay out: a [`StyleEnv`]
/// that also transforms ([`TransformEnv`], the flips and the sticky notes'
/// layout).
pub trait ShortcutEnv: StyleEnv + TransformEnv {}

impl<T: StyleEnv + TransformEnv> ShortcutEnv for T {}

/// What the shortcut actions read from and write to the host.
#[derive(Debug, Clone, PartialEq)]
pub struct ShortcutHost {
    /// `copiedStyles` (`actionStyles.ts:48`): the JSON copyStyles writes
    /// and pasteStyles reads, `"{}"` until a copy.
    pub copied_styles: String,
    /// `app.props.onThemeChange` is set: toggleTheme hands the theme over
    /// (and returns `false`).
    pub on_theme_change: bool,
    /// `t("toast.copyStyles")`.
    pub copy_styles_toast: String,
}

impl Default for ShortcutHost {
    fn default() -> ShortcutHost {
        ShortcutHost {
            copied_styles: "{}".to_owned(),
            on_theme_change: false,
            copy_styles_toast: "Copied styles.".to_owned(),
        }
    }
}

/// `action.perform(elements, appState, value, app)` for the shortcut
/// actions (deselect, flipHorizontal, flipVertical, toggleElementLock,
/// copyStyles, pasteStyles, viewMode, toggleTheme) on a scene of
/// `elements` (deleted ones included, in order); `value` is toggleTheme's
/// theme. `None` for another action or where upstream returns `false`.
pub fn perform_shortcut_action<E: ShortcutEnv>(
    name: ActionName,
    elements: &[Element],
    app_state: &AppState,
    value: &Value,
    host: &mut ShortcutHost,
    env: &mut E,
) -> Option<ActionResult> {
    use ActionName as N;
    match name {
        N::Deselect => Some(deselect(elements, app_state)),
        N::FlipHorizontal => Some(flip(elements, app_state, Flip::Horizontal, env)),
        N::FlipVertical => Some(flip(elements, app_state, Flip::Vertical, env)),
        N::ToggleElementLock => toggle_element_lock(elements, app_state, env),
        N::CopyStyles => Some(copy_styles(elements, app_state, host)),
        N::PasteStyles => Some(paste_styles(elements, app_state, &host.copied_styles, env)),
        N::ViewMode => {
            // viewModeEnabled: !this.checked(appState)
            let on = truthy(app_state.get("viewModeEnabled"));
            Some(eventually(one("viewModeEnabled", json!(!on))))
        }
        N::ToggleTheme => {
            let theme = match value.as_str().filter(|t| !t.is_empty()) {
                Some(theme) => theme.to_owned(),
                None if app_state.get("theme").and_then(Value::as_str) == Some("light") => {
                    "dark".to_owned()
                }
                None => "light".to_owned(),
            };
            if host.on_theme_change {
                // app.props.onThemeChange(nextTheme); return false
                return None;
            }
            Some(eventually(one("theme", json!(theme))))
        }
        _ => None,
    }
}

fn one(key: &str, value: Value) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(key.to_owned(), value);
    m
}

/// `{ appState, captureUpdate: EVENTUALLY }`.
fn eventually(app_state: Map<String, Value>) -> ActionResult {
    ActionResult {
        elements: None,
        app_state,
        capture: false,
        never: false,
    }
}

// -- deselect -------------------------------------------------------------------

/// `updateActiveTool(appState, data)` (`common/src/utils.ts:276-307`) on
/// the app state's `activeTool` JSON; `data` holds `type`, and optionally
/// `customType`, `locked`, `fromSelection` and `lastActiveTool`.
fn update_active_tool(current: &Map<String, Value>, data: &Map<String, Value>) -> Value {
    let mut next = current.clone();
    let given = |key: &str| data.get(key).filter(|v| !v.is_null()).cloned();
    if data.get("type").and_then(Value::as_str) == Some("custom") {
        next.insert("type".into(), json!("custom"));
        next.insert(
            "customType".into(),
            data.get("customType").cloned().unwrap_or(Value::Null),
        );
        let locked = given("locked").or_else(|| current.get("locked").cloned());
        next.insert("locked".into(), locked.unwrap_or(Value::Null));
        return Value::Object(next);
    }
    if let Some(last) = data.get("lastActiveTool") {
        next.insert("lastActiveTool".into(), last.clone());
    }
    next.insert(
        "type".into(),
        data.get("type").cloned().unwrap_or(Value::Null),
    );
    next.insert("customType".into(), Value::Null);
    let locked = given("locked").or_else(|| current.get("locked").cloned());
    next.insert("locked".into(), locked.unwrap_or(Value::Null));
    next.insert(
        "fromSelection".into(),
        given("fromSelection").unwrap_or(json!(false)),
    );
    Value::Object(next)
}

/// `getNextActiveTool(appState, app)` (`actionDeselect.ts:18-35`): a toggle
/// tool goes back to the tool it was toggled from (else the preferred
/// selection tool), any other to the preferred selection tool.
fn next_active_tool(app_state: &AppState) -> Value {
    let current = object_key(app_state, "activeTool");
    let preferred = app_state
        .get("preferredSelectionTool")
        .and_then(|p| p.get("type"))
        .cloned()
        .unwrap_or(json!("selection"));
    let is_toggle = current
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| TOGGLE_TOOLS.iter().any(|tool| tool.as_str() == t));
    if is_toggle {
        let mut data = match current.get("lastActiveTool") {
            Some(Value::Object(last)) => last.clone(),
            _ => one("type", preferred),
        };
        data.insert("lastActiveTool".into(), Value::Null);
        update_active_tool(&current, &data)
    } else {
        update_active_tool(&current, &one("type", preferred))
    }
}

/// `getParentEditingGroupId(appState, app, selectedElementIds)`
/// (`actionDeselect.ts:37-63`): the group one level out from the one
/// being edited, read from the selected elements (else the edited group's).
fn parent_editing_group_id(
    editing: &str,
    live: &[&Element],
    selected_ids: &Map<String, Value>,
) -> Option<String> {
    let selected = get_selected_elements(live, selected_ids, false, false);
    let candidates = if selected.is_empty() {
        get_elements_in_group(live, editing)
    } else {
        selected
    };
    candidates.iter().find_map(|element| {
        let groups = &element.base.group_ids;
        let i = groups.iter().position(|g| g == editing)?;
        groups.get(i + 1).cloned()
    })
}

/// `actionDeselect.perform` (`actionDeselect.ts:68-128`): the selection
/// cleared (inside a group being edited, the editing steps out one level
/// with the selection kept), the tool back to the selection tool, and the
/// transient selection state reset.
pub fn deselect(elements: &[Element], app_state: &AppState) -> ActionResult {
    let active_tool = next_active_tool(app_state);
    let mut patch = Map::new();
    if let Some(editing) = editing_group_id(app_state) {
        let live = non_deleted(elements);
        let current = object_key(app_state, "selectedElementIds");
        let selected_ids = if current.is_empty() {
            get_elements_in_group(&live, &editing)
                .iter()
                .map(|e| (e.base.id.clone(), json!(true)))
                .collect()
        } else {
            current
        };
        let parent = parent_editing_group_id(&editing, &live, &selected_ids);
        let groups = select_groups_for_selected_elements(&selected_ids, parent.as_deref(), &live);
        patch.insert("editingGroupId".into(), json!(groups.editing_group_id));
        patch.insert(
            "selectedElementIds".into(),
            Value::Object(groups.selected_element_ids),
        );
        patch.insert(
            "selectedGroupIds".into(),
            Value::Object(groups.selected_group_ids),
        );
        patch.insert("activeEmbeddable".into(), Value::Null);
        patch.insert("activeTool".into(), active_tool);
    } else {
        patch.insert("activeEmbeddable".into(), Value::Null);
        patch.insert("activeTool".into(), active_tool);
        patch.insert("editingGroupId".into(), Value::Null);
        patch.insert("selectedElementIds".into(), json!({}));
        patch.insert("selectedGroupIds".into(), json!({}));
    }
    patch.insert("selectedLinearElement".into(), Value::Null);
    patch.insert("selectionElement".into(), Value::Null);
    patch.insert("showHyperlinkPopup".into(), json!(false));
    patch.insert("suggestedBinding".into(), Value::Null);
    patch.insert("frameToHighlight".into(), Value::Null);
    ActionResult {
        elements: None,
        app_state: patch,
        capture: true,
        never: false,
    }
}

// -- flip -----------------------------------------------------------------------

/// The flip's axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flip {
    Horizontal,
    Vertical,
}

/// `actionFlipHorizontal.perform` and `actionFlipVertical.perform`
/// (`actionFlip.ts:28-80`): `flipSelectedElements` (the selection with
/// its labels and its frames' children, mirrored about its centre by
/// `resizeMultipleElements`, the arrows bound or unbound again and the
/// selection moved back to its centre; a selection of bound arrows only
/// swaps their arrowheads), then `updateFrameMembershipOfSelectedElements`.
pub fn flip<E: ShortcutEnv>(
    elements: &[Element],
    app_state: &AppState,
    direction: Flip,
    env: &mut E,
) -> ActionResult {
    let mut scene = Scene::new(elements.to_vec());
    let selected_ids: Vec<String> = {
        let live = non_deleted(elements);
        get_selected_elements(
            &live,
            &object_key(app_state, "selectedElementIds"),
            true,
            true,
        )
        .into_iter()
        .map(|e| e.base.id.clone())
        .collect()
    };
    flip_elements(&mut scene, &selected_ids, app_state, direction, env);
    let mut next = scene.elements().to_vec();
    let state = MembershipState::from_app_state(app_state);
    update_frame_membership_of_selected_elements(&mut next, &state, env);
    ActionResult {
        elements: Some(next),
        app_state: Map::new(),
        capture: true,
        never: false,
    }
}

/// A bound arrow (`isArrowElement(element) && (startBinding || endBinding)`).
fn is_bound_arrow(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Arrow(a)
        if a.linear.start_binding.is_some() || a.linear.end_binding.is_some())
}

/// `flipElements(selectedElements, elementsMap, flipDirection, app)`
/// (`actionFlip.ts:108-197`) on the scene's elements `ids`.
fn flip_elements<E: ShortcutEnv>(
    scene: &mut Scene,
    ids: &[String],
    app_state: &AppState,
    direction: Flip,
    env: &mut E,
) {
    let selected = |scene: &Scene| -> Vec<Element> {
        ids.iter().filter_map(|id| scene.get(id).cloned()).collect()
    };
    let current = selected(scene);
    if current.iter().all(is_bound_arrow) {
        // newElementWith(arrow, { startArrowhead: end, endArrowhead: start })
        for arrow in current {
            let ElementKind::Arrow(a) = &arrow.kind else {
                continue;
            };
            let mut updates = Map::new();
            updates.insert("startArrowhead".into(), json!(a.linear.end_arrowhead));
            updates.insert("endArrowhead".into(), json!(a.linear.start_arrowhead));
            let map = arrow.to_map();
            let changed = updates.iter().any(|(k, v)| map.get(k) != Some(v));
            if changed {
                if let Ok(next) = new_element_with(&arrow, updates, true, env) {
                    scene.replace_element(next);
                }
            }
        }
        return;
    }

    let refs: Vec<&Element> = current.iter().collect();
    let before = get_common_bounding_box(&refs);

    // originalElementsMap: deep copies of every non-deleted element
    let originals: Vec<Element> = scene.non_deleted().into_iter().cloned().collect();
    let original_bounding_box = {
        let with_labels = original_selection_with_labels(scene, ids, &originals);
        let refs: Vec<&Element> = with_labels.iter().collect();
        get_common_bounding_box(&refs)
    };
    let width = original_bounding_box.max_x - original_bounding_box.min_x;
    let height = original_bounding_box.max_y - original_bounding_box.min_y;
    let mut triggering = Triggering(env);
    let resized = resize_multiple_elements(
        ids,
        TransformHandleDirection::Nw,
        scene,
        &mut triggering,
        &originals,
        MultipleResize {
            should_maintain_aspect_ratio: true,
            should_resize_from_center: true,
            flip_by_x: direction == Flip::Horizontal,
            flip_by_y: direction == Flip::Vertical,
            next_width: width,
            next_height: height,
            original_bounding_box,
        },
    );
    if resized {
        // scene.triggerUpdate()
        let _ = RestoreEnv::random_integer(triggering.0);
    }

    let arrows: Vec<String> = selected(scene)
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Arrow(_)))
        .map(|e| e.base.id.clone())
        .collect();
    bind_or_unbind_binding_elements(
        scene,
        &mut triggering,
        &arrows,
        &binding_app_state(app_state),
    );

    // flipping arrows can move the selection off its centre: move it back
    let after_flip = selected(scene);
    let refs: Vec<&Element> = after_flip.iter().collect();
    let after = get_common_bounding_box(&refs);
    let (dx, dy) = (before.mid_x - after.mid_x, before.mid_y - after.mid_y);
    let (elbows, others): (Vec<&Element>, Vec<&Element>) =
        after_flip.iter().partition(|e| is_elbow_arrow(e));
    for element in others.into_iter().chain(elbows) {
        let Some(latest) = scene.get(&element.base.id) else {
            continue;
        };
        let update = ElementUpdate {
            x: Some(latest.base.x + dx),
            y: Some(latest.base.y + dy),
            ..ElementUpdate::default()
        };
        scene.mutate_element(&element.base.id, update, &mut triggering);
    }
}

// -- element lock -----------------------------------------------------------------

/// `selectGroupsFromGivenElements(elements, appState)` (`groups.ts:243-271`):
/// the outermost group (below the one being edited) of each element, as
/// `selectGroup` selects it among `elements`.
fn select_groups_from_given_elements(
    elements: &[&Element],
    app_state: &AppState,
) -> Map<String, Value> {
    let editing = editing_group_id(app_state);
    let mut selected_group_ids = Map::new();
    let mut next_editing = editing.clone();
    for element in elements {
        let mut groups: &[String] = &element.base.group_ids;
        if let Some(editing) = &editing {
            if let Some(i) = groups.iter().position(|g| g == editing) {
                groups = &groups[..i];
            }
        }
        let Some(group_id) = groups.last() else {
            continue;
        };
        // selectGroup(groupId, nextAppState, elements)
        let in_group = elements
            .iter()
            .filter(|e| e.base.group_ids.contains(group_id))
            .count();
        if in_group < 2 {
            if is_true(&selected_group_ids, group_id) || next_editing.as_ref() == Some(group_id) {
                selected_group_ids.insert(group_id.clone(), json!(false));
                next_editing = None;
            }
        } else {
            selected_group_ids.insert(group_id.clone(), json!(true));
        }
    }
    selected_group_ids
}

/// `actionToggleElementLock.perform` (`actionElementLock.ts:50-146`): the
/// selection (with its labels and its frames' children) locked, several
/// elements that are not one group held together by a new group recorded
/// in `lockedMultiSelections`, and deselected; or, when any is locked,
/// unlocked, the lock groups dropped, and selected. `None` (upstream's
/// `false`) with nothing selected.
pub fn toggle_element_lock<E: ShortcutEnv>(
    elements: &[Element],
    app_state: &AppState,
    env: &mut E,
) -> Option<ActionResult> {
    let live = non_deleted(elements);
    let selected: Vec<&Element> = get_selected_elements(
        &live,
        &object_key(app_state, "selectedElementIds"),
        true,
        true,
    );
    if selected.is_empty() {
        return None;
    }
    let next_lock = selected.iter().all(|e| !e.base.locked);
    let selected_ids: HashSet<&str> = selected.iter().map(|e| e.base.id.as_str()).collect();
    let is_a_group = selected.len() > 1 && elements_are_in_same_group(&selected);
    let is_a_single_unit = selected.len() == 1 || is_a_group;
    let new_group_id = (!is_a_single_unit).then(|| env.random_id());

    let locked_multi = object_key(app_state, "lockedMultiSelections");
    let mut next_locked_multi = locked_multi.clone();
    if next_lock {
        if let Some(group) = &new_group_id {
            next_locked_multi.insert(group.clone(), json!(true));
        }
    } else if is_a_group {
        if let Some(group) = selected[0].base.group_ids.last() {
            next_locked_multi.remove(group);
        }
    }

    let next_elements: Vec<Element> = elements
        .iter()
        .map(|element| {
            if !selected_ids.contains(element.base.id.as_str()) {
                return element.clone();
            }
            let group_ids: Vec<String> = if next_lock {
                let mut groups = element.base.group_ids.clone();
                groups.extend(new_group_id.clone());
                groups
            } else {
                element
                    .base
                    .group_ids
                    .iter()
                    .filter(|g| !is_true(&locked_multi, g))
                    .cloned()
                    .collect()
            };
            let mut updates = Map::new();
            updates.insert("locked".into(), json!(next_lock));
            updates.insert("groupIds".into(), json!(group_ids));
            // groupIds is an array: newElementWith always copies
            new_element_with(element, updates, true, env).unwrap_or_else(|_| element.clone())
        })
        .collect();

    let mut patch = Map::new();
    if next_lock {
        patch.insert("selectedElementIds".into(), json!({}));
        patch.insert("selectedGroupIds".into(), json!({}));
        patch.insert("selectedLinearElement".into(), Value::Null);
    } else {
        let ids: Map<String, Value> = selected
            .iter()
            .map(|e| (e.base.id.clone(), json!(true)))
            .collect();
        let unlocked: Vec<&Element> = selected
            .iter()
            .filter_map(|e| next_elements.iter().find(|n| n.base.id == e.base.id))
            .filter(|e| !e.base.is_deleted)
            .collect();
        patch.insert("selectedElementIds".into(), Value::Object(ids));
        patch.insert(
            "selectedGroupIds".into(),
            Value::Object(select_groups_from_given_elements(&unlocked, app_state)),
        );
    }
    patch.insert(
        "lockedMultiSelections".into(),
        Value::Object(next_locked_multi),
    );
    let active_locked_id = if !next_lock {
        Value::Null
    } else if let Some(group) = new_group_id {
        json!(group)
    } else if is_a_group {
        json!(selected[0].base.group_ids.last())
    } else {
        json!(selected[0].base.id)
    };
    patch.insert("activeLockedId".into(), active_locked_id);
    Some(ActionResult {
        elements: Some(next_elements),
        app_state: patch,
        capture: true,
        never: false,
    })
}

// -- copy styles ------------------------------------------------------------------

/// `actionCopyStyles.perform` (`actionStyles.ts:55-78`): the first selected
/// element (deleted or not) and its label, as JSON, become `copiedStyles`;
/// the toast says so either way.
pub fn copy_styles(
    elements: &[Element],
    app_state: &AppState,
    host: &mut ShortcutHost,
) -> ActionResult {
    let selected_ids = object_key(app_state, "selectedElementIds");
    let element = elements.iter().find(|e| is_true(&selected_ids, &e.base.id));
    if let Some(element) = element {
        let mut copied = vec![Value::Object(element.to_map())];
        if has_bound_text_element(element) {
            let live = non_deleted(elements);
            let map = ElementsMap::new(live.iter().copied());
            copied.push(
                get_bound_text_element(element, &map)
                    .map_or(Value::Null, |text| Value::Object(text.to_map())),
            );
        }
        host.copied_styles = Value::Array(copied).to_string();
    }
    eventually(one("toast", json!({ "message": host.copy_styles_toast })))
}
