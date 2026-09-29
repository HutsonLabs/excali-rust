//! Groups (`packages/element/src/groups.ts`): selecting a group through
//! one of its elements, and the group ids of new groups.

use excali_core::element::Element;
use serde_json::{Map, Value};

/// What `selectGroupsForSelectedElements` and `selectGroup` return: the
/// app state's `selectedElementIds`, `selectedGroupIds` and
/// `editingGroupId`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupSelection {
    pub selected_element_ids: Map<String, Value>,
    pub selected_group_ids: Map<String, Value>,
    pub editing_group_id: Option<String>,
}

fn is_true(ids: &Map<String, Value>, id: &str) -> bool {
    ids.get(id)
        .is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)))
}

/// `selectGroupsForSelectedElements(appState, elements, prevAppState,
/// app)` (`groups.ts:68-196`): the selected elements' outermost groups
/// (below the group being edited) selected, with every element in them; a
/// group of fewer than two elements is not a group (`false`). `elements`
/// are the scene's non-deleted elements, in order.
pub fn select_groups_for_selected_elements(
    selected_element_ids: &Map<String, Value>,
    editing_group_id: Option<&str>,
    elements: &[&Element],
) -> GroupSelection {
    let selected: Vec<&Element> = elements
        .iter()
        .copied()
        .filter(|e| !e.base.is_deleted && is_true(selected_element_ids, &e.base.id))
        .collect();
    if selected.is_empty() {
        return GroupSelection {
            selected_element_ids: selected_element_ids.clone(),
            selected_group_ids: Map::new(),
            editing_group_id: None,
        };
    }

    let mut selected_group_ids = Map::new();
    // the groups of the selected elements
    for element in &selected {
        let mut group_ids: &[String] = &element.base.group_ids;
        if let Some(editing) = editing_group_id {
            // a group nested in the one being edited
            if let Some(i) = group_ids.iter().position(|g| g == editing) {
                group_ids = &group_ids[..i];
            }
        }
        if let Some(last) = group_ids.last() {
            selected_group_ids.insert(last.clone(), Value::Bool(true));
        }
    }

    // the elements of the selected groups
    let mut group_elements: Vec<(String, usize)> = Vec::new();
    let mut in_groups: Map<String, Value> = Map::new();
    for element in elements {
        if element.base.is_deleted {
            continue;
        }
        let group_id = element
            .base
            .group_ids
            .iter()
            .find(|id| is_true(&selected_group_ids, id));
        if let Some(group_id) = group_id {
            in_groups.insert(element.base.id.clone(), Value::Bool(true));
            match group_elements.iter_mut().find(|(g, _)| g == group_id) {
                Some((_, n)) => *n += 1,
                None => group_elements.push((group_id.clone(), 1)),
            }
        }
    }
    for (group_id, count) in &group_elements {
        // one element in a group is no group
        if *count < 2 && is_true(&selected_group_ids, group_id) {
            selected_group_ids.insert(group_id.clone(), Value::Bool(false));
        }
    }

    let mut ids = selected_element_ids.clone();
    for (id, v) in in_groups {
        ids.insert(id, v);
    }
    GroupSelection {
        selected_element_ids: ids,
        selected_group_ids,
        editing_group_id: editing_group_id.map(str::to_owned),
    }
}

/// `getElementsInGroup(elements, groupId)` (`groups.ts:245-250`).
pub fn get_elements_in_group<'a>(elements: &[&'a Element], group_id: &str) -> Vec<&'a Element> {
    elements
        .iter()
        .copied()
        .filter(|e| e.base.group_ids.iter().any(|g| g == group_id))
        .collect()
}
