//! Frame membership as the canvas gestures change it
//! (`packages/element/src/frame.ts`): which frame is under the pointer
//! ([`is_cursor_in_frame`]), the elements a new frame takes in
//! ([`get_elements_in_new_frame`]), adding elements to a frame and taking
//! them out ([`add_elements_to_frame`], [`remove_elements_from_frame`]),
//! and the membership of the selection after a drag or a resize
//! ([`update_frame_membership_of_selected_elements`],
//! [`get_elements_in_resizing_frame`], [`replace_all_elements_in_frame`]).
//!
//! The functions take the scene's elements, deleted ones included, and
//! return them (or change them in place) with `frameId` updated and
//! versions bumped, as upstream's `mutateElement` does; elements are
//! named by their position in the list.

use std::collections::HashSet;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_scene::bounds::{get_element_absolute_coords, ElementsMap};
use excali_scene::frame::{
    elements_are_in_frame_bounds, get_elements_in_group, is_element_in_frame, is_frame_like,
};
use excali_scene::static_scene::StaticCanvasAppState;
use serde_json::Value;

use crate::edit_actions::{self, EditEnv};
use crate::selection::{get_elements_within_selection, BoxSelectionMode};

/// `getCommonFrameId(elements)` (`frame.ts:503-519`): the frame all the
/// elements are in, when they are all in one (none is a frame).
pub fn get_common_frame_id(elements: &[&Element]) -> Option<String> {
    edit_actions::get_common_frame_id(elements)
}

/// `isCursorInFrame(cursorCoords, frame, elementsMap)`
/// (`frame.ts:160-175`): the point is within the frame's box, edges
/// included.
pub fn is_cursor_in_frame(
    point: [f64; 2],
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) -> bool {
    let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(frame, elements_map, false);
    // isPointWithinBounds
    point[0] >= x1 && point[0] <= x2 && point[1] >= y1 && point[1] <= y2
}

fn position_of(elements: &[Element], found: &Element) -> usize {
    elements
        .iter()
        .position(|e| std::ptr::eq(e, found))
        .expect("an element of the scene")
}

/// `getElementsCompletelyInFrame(elements, frame, elementsMap)`
/// (`frame.ts:93-104`): the elements wholly inside the frame's box (groups
/// holding a frame left out) that are in no frame, or in this one.
fn get_elements_completely_in_frame(
    elements: &[Element],
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) -> Vec<usize> {
    let refs: Vec<&Element> = elements.iter().collect();
    let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(frame, elements_map, false);
    let within: Vec<usize> = get_elements_within_selection(
        &refs,
        [x1, y1, x2, y2],
        elements_map,
        false,
        BoxSelectionMode::Contain,
    )
    .into_iter()
    .map(|w| position_of(elements, w))
    .collect();
    edit_actions::omit_groups_containing_frame_likes(elements, &within, None)
        .into_iter()
        .filter(|&i| {
            let e = &elements[i];
            let frame_id = e.base.frame_id.as_deref().filter(|f| !f.is_empty());
            (!is_frame_like(e) && frame_id.is_none()) || frame_id == Some(frame.base.id.as_str())
        })
        .collect()
}

/// `omitPartialGroups(elements, frame, allElementsMap)`
/// (`frame.ts:395-434`): the elements whose groups lie wholly in the
/// frame's box (an element in no group stays).
fn omit_partial_groups(
    elements: &[Element],
    candidates: &[usize],
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) -> Vec<usize> {
    let mut checked: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for &i in candidates {
        let element = &elements[i];
        let mut omit = false;
        if !element.base.group_ids.is_empty() {
            if element
                .base
                .group_ids
                .iter()
                .any(|g| checked.get(g).copied().unwrap_or(false))
            {
                omit = true;
            } else {
                let mut seen = HashSet::new();
                let in_group: Vec<&Element> = element
                    .base
                    .group_ids
                    .iter()
                    .flat_map(|g| get_elements_in_group(elements_map, g))
                    .filter(|e| seen.insert(e.base.id.clone()))
                    .collect();
                omit = !elements_are_in_frame_bounds(&in_group, frame, elements_map);
            }
            for g in &element.base.group_ids {
                checked.insert(g.clone(), omit);
            }
        }
        if !omit {
            out.push(i);
        }
    }
    out
}

/// `getElementsInNewFrame(elements, frame, elementsMap)`
/// (`frame.ts:380-393`): what a frame just drawn takes in: the elements
/// wholly inside it, whole groups only. `elements_map` holds the
/// non-deleted elements.
pub fn get_elements_in_new_frame(
    elements: &[Element],
    frame: &Element,
    elements_map: &ElementsMap<'_>,
) -> Vec<usize> {
    let all: Vec<usize> = (0..elements.len()).collect();
    let completely = get_elements_completely_in_frame(elements, frame, elements_map);
    let without_frames =
        edit_actions::omit_groups_containing_frame_likes(elements, &all, Some(&completely));
    omit_partial_groups(elements, &without_frames, frame, elements_map)
}

/// `getFrameChildrenInsertionIndex(elements, frameId)`
/// (`frame.ts:521-538`): where a new child of the frame goes, above its
/// highest child (or below the frame).
pub fn get_frame_children_insertion_index(elements: &[&Element], frame: &str) -> Option<usize> {
    edit_actions::get_frame_children_insertion_index(elements, frame)
}

/// `addElementsToFrame(allElements, elementsToAdd, frame)`
/// (`frame.ts:544-635`): the elements (with their bound text; never
/// frames, other frames' children or groups holding a frame) put in the
/// frame and moved above its highest child, fractional indices synced.
pub fn add_elements_to_frame<E: EditEnv>(
    elements: Vec<Element>,
    to_add: &[usize],
    frame: &Element,
    env: &mut E,
) -> Vec<Element> {
    edit_actions::add_elements_to_frame(elements, to_add, frame, env)
}

/// `removeElementsFromFrame(elementsToRemove, elementsMap)`
/// (`frame.ts:637-674`): the elements, and their bound text, out of their
/// frames.
pub fn remove_elements_from_frame<E: EditEnv>(
    elements: &mut [Element],
    to_remove: &[usize],
    env: &mut E,
) {
    edit_actions::remove_elements_from_frame(elements, to_remove, false, env);
}

/// `replaceAllElementsInFrame(allElements, nextElementsInFrame, frame)`
/// (`frame.ts:684-694`).
pub fn replace_all_elements_in_frame<E: EditEnv>(
    elements: Vec<Element>,
    next_in_frame: &[usize],
    frame: &Element,
    env: &mut E,
) -> Vec<Element> {
    edit_actions::replace_all_elements_in_frame(elements, next_in_frame, frame, env)
}

/// `getElementsInResizingFrame(allElements, frame, appState, elementsMap)`
/// (`frame.ts:283-378`).
pub fn get_elements_in_resizing_frame(
    elements: &[Element],
    frame: &Element,
    app_state: &AppState,
    elements_map: &ElementsMap<'_>,
) -> Vec<usize> {
    edit_actions::get_elements_in_resizing_frame(elements, frame, app_state, elements_map)
}

/// The app state frame membership reads: the selection, whether it is
/// being dragged, the frame it is dragged over, the group being edited.
#[derive(Clone, Debug, Default)]
pub struct MembershipState {
    pub selected_element_ids: HashSet<String>,
    pub selected_elements_are_being_dragged: bool,
    /// The id of `frameToHighlight`, the frame the drag is over.
    pub frame_to_highlight: Option<String>,
    pub editing_group_id: Option<String>,
}

impl MembershipState {
    /// From the app state's `selectedElementIds`, `editingGroupId`,
    /// `selectedElementsAreBeingDragged` and `frameToHighlight` (the
    /// element, or its id).
    pub fn from_app_state(app_state: &AppState) -> MembershipState {
        MembershipState {
            selected_element_ids: app_state
                .get("selectedElementIds")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter(|(_, v)| v.as_bool() == Some(true))
                        .map(|(k, _)| k.clone())
                        .collect()
                })
                .unwrap_or_default(),
            selected_elements_are_being_dragged: app_state
                .get("selectedElementsAreBeingDragged")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            frame_to_highlight: app_state
                .get("frameToHighlight")
                .and_then(|f| f.get("id").or(Some(f)))
                .and_then(Value::as_str)
                .map(str::to_owned),
            editing_group_id: app_state
                .get("editingGroupId")
                .and_then(Value::as_str)
                .filter(|g| !g.is_empty())
                .map(str::to_owned),
        }
    }

    fn canvas_state(&self, elements: &[Element]) -> StaticCanvasAppState {
        StaticCanvasAppState {
            selected_element_ids: self.selected_element_ids.clone(),
            selected_elements_are_being_dragged: self.selected_elements_are_being_dragged,
            frame_to_highlight: self.frame_to_highlight.as_deref().and_then(|id| {
                elements
                    .iter()
                    .find(|e| e.base.id == id && !e.base.is_deleted)
                    .cloned()
            }),
            editing_group_id: self.editing_group_id.clone(),
            ..StaticCanvasAppState::default()
        }
    }
}

/// `isElementInFrame(element, allElementsMap, appState)`
/// (`frame.ts:817-909`) over the scene's elements; an element whose shape
/// cannot be built counts as outside.
pub fn is_in_frame(elements: &[Element], index: usize, state: &MembershipState) -> bool {
    let canvas = state.canvas_state(elements);
    let map = ElementsMap::new(elements.iter());
    is_element_in_frame(&elements[index], &map, &canvas, None, None).unwrap_or(false)
}

/// `updateFrameMembershipOfSelectedElements(allElements, appState, app)`
/// (`frame.ts:697-745`): the selected elements (while a group is edited,
/// with the rest of their groups) that are in a frame but no longer
/// [`is_in_frame`] taken out of it.
pub fn update_frame_membership_of_selected_elements<E: EditEnv>(
    elements: &mut [Element],
    state: &MembershipState,
    env: &mut E,
) {
    let selected: Vec<usize> = (0..elements.len())
        .filter(|&i| {
            !elements[i].base.is_deleted
                && state.selected_element_ids.contains(&elements[i].base.id)
        })
        .collect();
    let mut to_filter: Vec<usize> = selected.clone();
    if state.editing_group_id.is_some() {
        for &i in &selected {
            for g in elements[i].base.group_ids.clone() {
                for (j, e) in elements.iter().enumerate() {
                    if e.base.group_ids.contains(&g) && !to_filter.contains(&j) {
                        to_filter.push(j);
                    }
                }
            }
        }
    }
    let to_remove: Vec<usize> = to_filter
        .into_iter()
        .filter(|&i| {
            let e = &elements[i];
            e.base.frame_id.as_deref().is_some_and(|f| !f.is_empty())
                && !is_frame_like(e)
                && !is_in_frame(elements, i, state)
        })
        .collect();
    if !to_remove.is_empty() {
        remove_elements_from_frame(elements, &to_remove, env);
    }
}
