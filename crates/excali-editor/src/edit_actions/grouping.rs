//! `actionGroup` and `actionUngroup`
//! (`packages/excalidraw/actions/actionGroup.tsx`), with the group helpers
//! of `packages/element/src/groups.ts` and the frame membership updates of
//! `packages/element/src/frame.ts` they run.

use std::collections::HashSet;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_core::fractional_index::sync_moved_indices;
use excali_core::restore::RestoreEnv;
use excali_scene::bounds::{get_bound_text_element_id, get_element_absolute_coords, ElementsMap};
use excali_scene::frame::{
    elements_are_in_frame_bounds, is_element_containing_frame, is_element_intersecting_frame,
    is_frame_like,
};
use serde_json::{Map, Value};

use super::{
    bump, editing_group_id, frame_id, get_selected_elements, is_bound_to_container, is_true,
    non_deleted, object_key, set_frame_id, truthy, ActionResult, EditEnv,
};
use crate::groups::select_groups_for_selected_elements;
use crate::selection::{get_elements_within_selection, BoxSelectionMode};

/// `selectedElementIds`, `selectedGroupIds` and `editingGroupId`.
type Selection = (Map<String, Value>, Map<String, Value>, Option<String>);

/// `selectGroup(groupId, appState, elements)` (`groups.ts:24-66`): the
/// group's elements selected with it, unless it has fewer than two, when a
/// selected or edited group is deselected and anything else is left.
fn select_group(group_id: &str, state: Selection, elements: &[&Element]) -> Selection {
    let (selected_element_ids, selected_group_ids, editing) = state;
    let mut in_group = Map::new();
    for element in elements {
        if element.base.group_ids.iter().any(|g| g == group_id) {
            in_group.insert(element.base.id.clone(), Value::Bool(true));
        }
    }
    if in_group.len() < 2 {
        if truthy(selected_group_ids.get(group_id)) || editing.as_deref() == Some(group_id) {
            let mut groups = selected_group_ids;
            groups.insert(group_id.to_owned(), Value::Bool(false));
            return (selected_element_ids, groups, None);
        }
        return (selected_element_ids, selected_group_ids, editing);
    }
    let mut groups = selected_group_ids;
    groups.insert(group_id.to_owned(), Value::Bool(true));
    let mut ids = selected_element_ids;
    for (id, v) in in_group {
        ids.insert(id, v);
    }
    (ids, groups, editing)
}

/// `getSelectedGroupIds(appState)` (`groups.ts:234-240`).
fn get_selected_group_ids(app_state: &AppState) -> Vec<String> {
    object_key(app_state, "selectedGroupIds")
        .iter()
        .filter(|(_, v)| truthy(Some(v)))
        .map(|(g, _)| g.clone())
        .collect()
}

/// `addToGroup(prevGroupIds, newGroupId, editingGroupId)`
/// (`groups.ts:311-324`): the new group inserted before the edited one,
/// or last.
fn add_to_group(group_ids: &[String], new_group: &str, editing: Option<&str>) -> Vec<String> {
    let mut ids = group_ids.to_vec();
    let at = editing
        .and_then(|e| ids.iter().position(|g| g == e))
        .unwrap_or(ids.len());
    ids.insert(at, new_group.to_owned());
    ids
}

/// `allElementsInSameGroup(elements, editingGroupId)`
/// (`actionGroup.tsx:48-67`).
fn all_elements_in_same_group(elements: &[&Element], editing: Option<&str>) -> bool {
    if elements.len() < 2 {
        return false;
    }
    let first = &elements[0].base.group_ids;
    let end = editing
        .and_then(|e| first.iter().position(|g| g == e))
        .unwrap_or(first.len());
    first[..end]
        .iter()
        .any(|g| elements.iter().all(|e| e.base.group_ids.contains(g)))
}

/// `frameAndChildrenSelectedTogether(selectedElements)` (`frame.ts:1000-1011`).
fn frame_and_children_selected_together(selected: &[&Element]) -> bool {
    let ids: HashSet<&str> = selected.iter().map(|e| e.base.id.as_str()).collect();
    selected.len() > 1
        && selected
            .iter()
            .any(|e| frame_id(e).is_some_and(|f| ids.contains(f)))
}

/// `getRootElements(allElements)` (`frame.ts:271-281`): frames and the
/// elements whose frame is not among them.
fn get_root_elements<'a>(elements: &[&'a Element]) -> Vec<&'a Element> {
    let frames: HashSet<&str> = elements
        .iter()
        .filter(|e| is_frame_like(e))
        .map(|e| e.base.id.as_str())
        .collect();
    elements
        .iter()
        .copied()
        .filter(|e| {
            frames.contains(e.base.id.as_str()) || frame_id(e).is_none_or(|f| !frames.contains(f))
        })
        .collect()
}

/// The position of the element `id` in `elements` as `arrayToMap` finds
/// it (the last one), among the non-deleted ones with `live_only`.
fn position_by_id(elements: &[Element], id: &str, live_only: bool) -> Option<usize> {
    elements
        .iter()
        .rposition(|e| e.base.id == id && !(live_only && e.base.is_deleted))
}

/// Pushes `i` unless it is there (a `Set`, or a `Map` keyed by id).
fn push_unique(list: &mut Vec<usize>, i: usize) {
    if !list.contains(&i) {
        list.push(i);
    }
}

/// `removeElementsFromFrame(elementsToRemove, elementsMap)`
/// (`frame.ts:637-674`): the elements in a frame, and their bound text,
/// taken out of it (`frameId: null`, in place).
pub(crate) fn remove_elements_from_frame<E: EditEnv>(
    elements: &mut [Element],
    to_remove: &[usize],
    live_only: bool,
    env: &mut E,
) {
    let mut removing: Vec<usize> = Vec::new();
    for &i in to_remove {
        if frame_id(&elements[i]).is_none() {
            continue;
        }
        push_unique(&mut removing, i);
        let text = get_bound_text_element_id(&elements[i]).map(str::to_owned);
        if let Some(t) = text.and_then(|t| position_by_id(elements, &t, live_only)) {
            push_unique(&mut removing, t);
        }
    }
    for i in removing {
        set_frame_id(&mut elements[i], None, env);
    }
}

/// `omitGroupsContainingFrameLikes(allElements, selectedElements)`
/// (`frame.ts:747-782`): `selected` (or `all`) without the elements whose
/// outermost group holds a frame among `all`.
pub(crate) fn omit_groups_containing_frame_likes(
    elements: &[Element],
    all: &[usize],
    selected: Option<&[usize]>,
) -> Vec<usize> {
    let list = selected.unwrap_or(all);
    let mut groups: Vec<&str> = Vec::new();
    for &i in list {
        if let Some(g) = elements[i].base.group_ids.last() {
            if !groups.contains(&g.as_str()) {
                groups.push(g);
            }
        }
    }
    let rejected: HashSet<&str> = groups
        .into_iter()
        .filter(|g| {
            all.iter().any(|&i| {
                elements[i].base.group_ids.iter().any(|x| x == g) && is_frame_like(&elements[i])
            })
        })
        .collect();
    list.iter()
        .copied()
        .filter(|&i| {
            elements[i]
                .base
                .group_ids
                .last()
                .is_none_or(|g| !rejected.contains(g.as_str()))
        })
        .collect()
}

/// `getCommonFrameId(elements)` (`frame.ts:503-519`).
pub(crate) fn get_common_frame_id(elements: &[&Element]) -> Option<String> {
    let mut common: Option<&str> = None;
    for element in elements {
        let frame = frame_id(element);
        if is_frame_like(element) || frame.is_none() {
            return None;
        }
        match common {
            None => common = frame,
            Some(c) if Some(c) != frame => return None,
            _ => {}
        }
    }
    common.map(str::to_owned)
}

/// `getFrameChildrenInsertionIndex(elements, frameId)` (`frame.ts:521-538`).
pub(crate) fn get_frame_children_insertion_index(
    elements: &[&Element],
    frame: &str,
) -> Option<usize> {
    for (index, element) in elements.iter().enumerate().rev() {
        if element.base.id == frame {
            return Some(index);
        }
        if element.base.frame_id.as_deref() == Some(frame) {
            return Some(index + 1);
        }
    }
    None
}

/// `addElementsToFrame(allElements, elementsToAdd, frame)`
/// (`frame.ts:544-635`): the elements (their bound text with them, never
/// frames, other frames' children or groups holding a frame) put in the
/// frame, and moved below the frame or above its highest child with their
/// fractional indices synced, unless they all were in it already.
pub(crate) fn add_elements_to_frame<E: EditEnv>(
    mut elements: Vec<Element>,
    to_add: &[usize],
    frame: &Element,
    env: &mut E,
) -> Vec<Element> {
    let frame_id_str = frame.base.id.clone();
    let common = get_common_frame_id(&to_add.iter().map(|&i| &elements[i]).collect::<Vec<_>>());
    let other_frames: HashSet<String> = to_add
        .iter()
        .map(|&i| &elements[i])
        .filter(|e| is_frame_like(e) && e.base.id != frame_id_str)
        .map(|e| e.base.id.clone())
        .collect();
    let all: Vec<usize> = (0..elements.len()).collect();
    let mut adding: Vec<usize> = Vec::new();
    for i in omit_groups_containing_frame_likes(&elements, &all, Some(to_add)) {
        let element = &elements[i];
        if is_frame_like(element) || frame_id(element).is_some_and(|f| other_frames.contains(f)) {
            continue;
        }
        push_unique(&mut adding, i);
        let text = get_bound_text_element_id(element).map(str::to_owned);
        if let Some(t) = text.and_then(|t| position_by_id(&elements, &t, false)) {
            push_unique(&mut adding, t);
        }
    }
    for &i in &adding {
        if elements[i].base.frame_id.as_deref() != Some(frame_id_str.as_str()) {
            set_frame_id(&mut elements[i], Some(&frame_id_str), env);
        }
    }
    if adding.is_empty() || common.as_deref() == Some(frame_id_str.as_str()) {
        return elements;
    }
    let others: Vec<usize> = all
        .iter()
        .copied()
        .filter(|i| !adding.contains(i))
        .collect();
    let insertion = get_frame_children_insertion_index(
        &others.iter().map(|&i| &elements[i]).collect::<Vec<_>>(),
        &frame_id_str,
    );
    let Some(insertion) = insertion else {
        return elements;
    };
    let order: Vec<usize> = others[..insertion]
        .iter()
        .chain(&adding)
        .chain(&others[insertion..])
        .copied()
        .collect();
    let moved: HashSet<String> = adding
        .iter()
        .map(|&i| elements[i].base.id.clone())
        .collect();
    let mut reordered: Vec<Element> = order.iter().map(|&i| elements[i].clone()).collect();
    if sync_moved_indices(&mut reordered, &moved, env).is_err() {
        return elements;
    }
    reordered
}

/// `replaceAllElementsInFrame(allElements, nextElementsInFrame, frame)`
/// (`frame.ts:684-694`): every child taken out of the frame
/// (`removeAllElementsFromFrame`), then the given ones added.
pub(crate) fn replace_all_elements_in_frame<E: EditEnv>(
    mut elements: Vec<Element>,
    next_in_frame: &[usize],
    frame: &Element,
    env: &mut E,
) -> Vec<Element> {
    let children: Vec<usize> = (0..elements.len())
        .filter(|&i| elements[i].base.frame_id.as_deref() == Some(frame.base.id.as_str()))
        .collect();
    remove_elements_from_frame(&mut elements, &children, false, env);
    add_elements_to_frame(elements, next_in_frame, frame, env)
}

/// `selectGroupsFromGivenElements(elements, appState)` (`groups.ts:244-273`):
/// the outermost groups (below the edited one) of the elements, selected
/// one by one.
fn select_groups_from_given_elements(
    elements: &[&Element],
    app_state: &AppState,
) -> Map<String, Value> {
    let editing = editing_group_id(app_state);
    let mut state: Selection = (
        object_key(app_state, "selectedElementIds"),
        Map::new(),
        editing.clone(),
    );
    for element in elements {
        let mut groups: &[String] = &element.base.group_ids;
        if let Some(editing) = editing.as_deref() {
            if let Some(i) = groups.iter().position(|g| g == editing) {
                groups = &groups[..i];
            }
        }
        if let Some(group) = groups.last() {
            state = select_group(group, state, elements);
        }
    }
    state.1
}

/// `getElementsInResizingFrame(allElements, frame, appState, elementsMap)`
/// (`frame.ts:283-378`): the elements a frame holds after a change (its
/// children still in or crossing it, whole groups kept together, and new
/// elements and groups wholly inside it), bound text left out.
pub(crate) fn get_elements_in_resizing_frame(
    elements: &[Element],
    frame: &Element,
    app_state: &AppState,
    elements_map: &ElementsMap<'_>,
) -> Vec<usize> {
    let frame_str = frame.base.id.as_str();
    let prev: Vec<usize> = (0..elements.len())
        .filter(|&i| elements[i].base.frame_id.as_deref() == Some(frame_str))
        .collect();
    let mut next_in_frame: Vec<usize> = prev.clone();

    // getElementsCompletelyInFrame
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
    .map(|w| {
        elements
            .iter()
            .position(|e| std::ptr::eq(e, w))
            .expect("an element of the scene")
    })
    .collect();
    let mut completely: Vec<usize> = Vec::new();
    for i in omit_groups_containing_frame_likes(elements, &within, None) {
        let e = &elements[i];
        if (!is_frame_like(e) && frame_id(e).is_none())
            || e.base.frame_id.as_deref() == Some(frame_str)
        {
            push_unique(&mut completely, i);
        }
    }
    for &i in &prev {
        if is_element_containing_frame(&elements[i], frame, elements_map) {
            push_unique(&mut completely, i);
        }
    }

    let not_completely: Vec<usize> = prev
        .iter()
        .copied()
        .filter(|i| !completely.contains(i))
        .collect();
    let mut groups_to_keep: HashSet<String> = completely
        .iter()
        .flat_map(|&i| elements[i].base.group_ids.iter().cloned())
        .collect();
    for &i in &not_completely {
        let element = &elements[i];
        // a shape upstream fails to build throws; the port counts it as
        // not crossing
        let intersecting =
            is_element_intersecting_frame(element, frame, elements_map).unwrap_or(false);
        if !intersecting {
            if element.base.group_ids.is_empty() {
                next_in_frame.retain(|&j| j != i);
            }
        } else {
            groups_to_keep.extend(element.base.group_ids.iter().cloned());
        }
    }
    for &i in &not_completely {
        let groups = &elements[i].base.group_ids;
        if !groups.is_empty() && !groups.iter().any(|g| groups_to_keep.contains(g)) {
            next_in_frame.retain(|&j| j != i);
        }
    }
    for &i in &completely {
        if elements[i].base.group_ids.is_empty() {
            push_unique(&mut next_in_frame, i);
        }
    }
    let new_group_elements: Vec<&Element> = completely
        .iter()
        .map(|&i| &elements[i])
        .filter(|e| !e.base.group_ids.is_empty() && !e.base.is_deleted)
        .collect();
    let group_ids = select_groups_from_given_elements(&new_group_elements, app_state);
    for (group, selected) in &group_ids {
        if !truthy(Some(selected)) {
            continue;
        }
        let in_group: Vec<usize> = (0..elements.len())
            .filter(|&i| elements[i].base.group_ids.iter().any(|g| g == group))
            .collect();
        let refs: Vec<&Element> = in_group.iter().map(|&i| &elements[i]).collect();
        if elements_are_in_frame_bounds(&refs, frame, elements_map) {
            for i in in_group {
                push_unique(&mut next_in_frame, i);
            }
        }
    }
    next_in_frame
        .into_iter()
        .filter(|&i| !super::container_id(&elements[i]).is_some())
        .collect()
}

/// `actionGroup.perform` (`actionGroup.tsx:87-203`): the selection (with
/// bound text, frames' children left to their frames) put in a new group
/// (inside the group being edited), taken out of their frames when they
/// come from different ones, and moved to the z-order of the topmost one
/// with their fractional indices synced; the new group selected
/// (`selectGroup`). Sets `selectedElementIds`, `selectedGroupIds` and
/// `editingGroupId`; captured.
///
/// `None` (the action is disabled) unless two or more elements are
/// selected, not all in one group already, and not a frame with its
/// children; nothing to do (not captured) when the selection is one group.
pub fn group<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    env: &mut E,
) -> Option<ActionResult> {
    let selected = object_key(app_state, "selectedElementIds");
    let editing = editing_group_id(app_state);
    let live = non_deleted(elements);

    // enableActionGroup
    let plain = get_selected_elements(&live, &selected, false, false);
    if !(plain.len() >= 2
        && !all_elements_in_same_group(&plain, editing.as_deref())
        && !frame_and_children_selected_together(&plain))
    {
        return None;
    }

    let selected_elements =
        get_root_elements(&get_selected_elements(&live, &selected, true, false));
    if selected_elements.len() < 2 {
        return Some(ActionResult::unchanged());
    }
    // if everything is already grouped into 1 group, there is nothing to do
    if let [group] = get_selected_group_ids(app_state).as_slice() {
        let in_group: HashSet<&str> = elements
            .iter()
            .filter(|e| e.base.group_ids.contains(group))
            .map(|e| e.base.id.as_str())
            .collect();
        let mut combined = in_group.clone();
        combined.extend(selected_elements.iter().map(|e| e.base.id.as_str()));
        if combined.len() == in_group.len() {
            return Some(ActionResult::unchanged());
        }
    }

    let mut next: Vec<Element> = elements.to_vec();
    let selected_ids: Vec<String> = selected_elements
        .iter()
        .map(|e| e.base.id.clone())
        .collect();

    // grouping elements from different frames takes them out of theirs
    let frames: HashSet<Option<&str>> = selected_elements.iter().map(|e| frame_id(e)).collect();
    if frames.len() > 1 {
        // groupByFrameLikes
        let mut by_frame: Vec<(String, Vec<String>)> = Vec::new();
        for element in &selected_elements {
            let frame = if is_frame_like(element) {
                Some(element.base.id.as_str())
            } else {
                frame_id(element)
            };
            let Some(frame) = frame else {
                continue;
            };
            if by_frame.iter().any(|(f, _)| f == frame) {
                continue;
            }
            let children = selected_elements
                .iter()
                .filter(|e| e.base.frame_id.as_deref() == Some(frame))
                .map(|e| e.base.id.clone())
                .collect();
            by_frame.push((frame.to_owned(), children));
        }
        for (_, children) in by_frame {
            let positions: Vec<usize> = children
                .iter()
                .filter_map(|id| position_by_id(&next, id, true))
                .collect();
            remove_elements_from_frame(&mut next, &positions, true, env);
        }
    }

    let new_group_id = RestoreEnv::random_id(env);
    for element in next.iter_mut() {
        if !selected_ids.contains(&element.base.id) {
            continue;
        }
        element.base.group_ids =
            add_to_group(&element.base.group_ids, &new_group_id, editing.as_deref());
        bump(element, env);
    }

    // keep the z order within the group the same, but move them to the z
    // order of the highest element in the layer stack
    let in_group = |e: &Element| e.base.group_ids.contains(&new_group_id);
    let group_positions: Vec<usize> = (0..next.len()).filter(|&i| in_group(&next[i])).collect();
    let last = *group_positions.last()?;
    let order: Vec<usize> = (0..last)
        .filter(|&i| !in_group(&next[i]))
        .chain(group_positions.iter().copied())
        .chain(last + 1..next.len())
        .collect();
    let moved: HashSet<String> = group_positions
        .iter()
        .map(|&i| next[i].base.id.clone())
        .collect();
    let mut reordered: Vec<Element> = order.iter().map(|&i| next[i].clone()).collect();
    sync_moved_indices(&mut reordered, &moved, env).ok()?;

    let live_next = non_deleted(&next);
    let (ids, groups, editing) =
        select_group(&new_group_id, (selected, Map::new(), editing), &live_next);
    let mut patch = Map::new();
    patch.insert("selectedElementIds".into(), Value::Object(ids));
    patch.insert("selectedGroupIds".into(), Value::Object(groups));
    patch.insert(
        "editingGroupId".into(),
        editing.map_or(Value::Null, Value::String),
    );
    Some(ActionResult {
        elements: Some(reordered),
        app_state: patch,
        capture: true,
        never: false,
    })
}

/// `actionUngroup.perform` (`actionGroup.tsx:212-310`): the selected groups
/// removed from every element's `groupIds` (`removeFromSelectedGroups`),
/// the selection's groups selected again
/// (`selectGroupsForSelectedElements`) without bound text, and the frames
/// of the selected elements given their children again
/// (`replaceAllElementsInFrame` with `getElementsInResizingFrame`). Sets
/// `selectedElementIds`, `selectedGroupIds` and `editingGroupId`;
/// captured. Nothing to do (not captured) when no group is selected.
pub fn ungroup<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    env: &mut E,
) -> Option<ActionResult> {
    let group_ids = get_selected_group_ids(app_state);
    if group_ids.is_empty() {
        return Some(ActionResult::unchanged());
    }
    let selected_group_ids = object_key(app_state, "selectedGroupIds");
    let selected = object_key(app_state, "selectedElementIds");
    let editing = editing_group_id(app_state);
    let elements_map = ElementsMap::new(elements.iter());

    let mut bound_text_ids: Vec<String> = Vec::new();
    let mut next: Vec<Element> = Vec::with_capacity(elements.len());
    for element in elements {
        if is_bound_to_container(element) {
            bound_text_ids.push(element.base.id.clone());
        }
        let kept: Vec<String> = element
            .base
            .group_ids
            .iter()
            .filter(|g| !is_true(&selected_group_ids, g))
            .cloned()
            .collect();
        if kept.len() == element.base.group_ids.len() {
            next.push(element.clone());
            continue;
        }
        let mut copy = element.clone();
        copy.base.group_ids = kept;
        bump(&mut copy, env);
        next.push(copy);
    }

    let groups = {
        let live = non_deleted(&next);
        select_groups_for_selected_elements(&selected, editing.as_deref(), &live)
    };

    let live = non_deleted(elements);
    let selected_frames: HashSet<&str> = get_selected_elements(&live, &selected, false, false)
        .iter()
        .filter_map(|e| frame_id(e))
        .collect();
    let target_frames: Vec<&Element> = elements
        .iter()
        .filter(|e| is_frame_like(e) && selected_frames.contains(e.base.id.as_str()))
        .collect();
    for frame in target_frames {
        let in_frame = get_elements_in_resizing_frame(&next, frame, app_state, &elements_map);
        next = replace_all_elements_in_frame(next, &in_frame, frame, env);
    }

    // remove bound text elements from selection
    let ids: Map<String, Value> = groups
        .selected_element_ids
        .iter()
        .filter(|(id, v)| truthy(Some(v)) && !bound_text_ids.contains(id))
        .map(|(id, _)| (id.clone(), Value::Bool(true)))
        .collect();
    let mut patch = Map::new();
    patch.insert("selectedElementIds".into(), Value::Object(ids));
    patch.insert(
        "selectedGroupIds".into(),
        Value::Object(groups.selected_group_ids),
    );
    patch.insert(
        "editingGroupId".into(),
        groups.editing_group_id.map_or(Value::Null, Value::String),
    );
    Some(ActionResult {
        elements: Some(next),
        app_state: patch,
        capture: true,
        never: false,
    })
}
