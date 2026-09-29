//! Duplicating elements (`packages/element/src/duplicate.ts`) and
//! `actionDuplicateSelection`
//! (`packages/excalidraw/actions/actionDuplicateSelection.tsx`).

use std::collections::{HashMap, HashSet};

use excali_core::app_state::AppState;
use excali_core::element::{BoundElementType, Element, ElementKind};
use excali_core::fractional_index::sync_moved_indices;
use excali_core::restore::RestoreEnv;
use excali_scene::bounds::{get_bound_text_element, get_container_element, ElementsMap};
use excali_scene::frame::is_frame_like;
use serde_json::{Map, Value};

use super::{
    bump, editing_group_id, frame_id, get_selected_elements, is_bound_to_container,
    is_editing_linear_element, object_key, selection_state_for_elements, set_frame_id, truthy,
    ActionResult, EditEnv,
};
use crate::binding::fix_duplicated_bindings_after_duplication;
use crate::elbow_arrow::ElbowArrowError;

/// `DEFAULT_GRID_SIZE` (`packages/common/src/constants.ts`).
const DEFAULT_GRID_SIZE: f64 = 20.0;

/// Which elements `duplicateElements` duplicates.
#[derive(Debug, Clone, PartialEq)]
pub enum DuplicateType {
    /// `type: "everything"`: every element, as programmatic duplication
    /// (paste, library insertion) does. With
    /// `preserve_frame_children_order`, a frame and its children are
    /// duplicated in their own places rather than the children together
    /// under the frame.
    Everything { preserve_frame_children_order: bool },
    /// `type: "in-place"`: the elements `ids` (the selection), with the
    /// elements of every group in `selected_group_ids`, each duplicate
    /// inserted after its original (a group's after the group's last
    /// element), as the duplicate action and alt-drag do.
    InPlace {
        ids: Vec<String>,
        editing_group_id: Option<String>,
        selected_group_ids: Map<String, Value>,
    },
}

/// What `duplicateElements` returns.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Duplication {
    /// The duplicates, in the order they were made.
    pub duplicated_elements: Vec<Element>,
    /// The elements (normalised, `normalizeElementOrder`) with the
    /// duplicates inserted.
    pub elements_with_duplicates: Vec<Element>,
    /// The id of each original's duplicate.
    pub orig_id_to_duplicate_id: HashMap<String, String>,
}

/// `getNewGroupIdsForDuplication(groupIds, editingGroupId, mapper)`
/// (`groups.ts:395-411`): the groups below the one being edited mapped
/// to new ids, that group and its parents kept.
pub fn get_new_group_ids_for_duplication(
    group_ids: &[String],
    editing_group_id: Option<&str>,
    mut mapper: impl FnMut(&str) -> String,
) -> Vec<String> {
    let end = editing_group_id
        .and_then(|editing| group_ids.iter().position(|g| g == editing))
        .unwrap_or(group_ids.len());
    group_ids
        .iter()
        .enumerate()
        .map(|(i, g)| if i < end { mapper(g) } else { g.clone() })
        .collect()
}

/// `duplicateElement(editingGroupId, groupIdMapForOperation, element,
/// randomizeSeed)` (`duplicate.ts:108-140`): a deep copy with a new id,
/// `updated` and `created` now, with `randomize_seed` a new seed and a
/// version bump, and its groups below the edited one mapped to new ids
/// (shared through `group_id_map`).
pub fn duplicate_element<E: EditEnv>(
    editing_group_id: Option<&str>,
    group_id_map: &mut HashMap<String, String>,
    element: &Element,
    randomize_seed: bool,
    env: &mut E,
) -> Element {
    let mut copy = element.clone();
    // _deepCopyElement leaves out the render caches
    copy.extra.shift_remove("shape");
    copy.extra.shift_remove("canvas");
    copy.base.id = RestoreEnv::random_id(env);
    copy.base.updated = excali_core::fractional_index::ChangeStamp::updated(env);
    copy.base.created = Some(copy.base.updated);
    if randomize_seed {
        copy.base.seed = RestoreEnv::random_integer(env);
        bump(&mut copy, env);
    }
    copy.base.group_ids =
        get_new_group_ids_for_duplication(&copy.base.group_ids, editing_group_id, |group| {
            if let Some(id) = group_id_map.get(group) {
                return id.clone();
            }
            let id = RestoreEnv::random_id(env);
            group_id_map.insert(group.to_owned(), id.clone());
            id
        });
    copy
}

/// `defragmentGroups(elements)` (`sortElements.ts:5-54`): the elements of
/// each group made contiguous, outermost groups first, at the place of the
/// group's first element.
fn defragment_groups(elements: &[usize], all: &[Element]) -> Vec<usize> {
    fn order_level(level_elements: &[usize], level: usize, all: &[Element]) -> Vec<usize> {
        enum Slot<'a> {
            Element(usize),
            Group(&'a str),
        }
        let mut buckets: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut slots: Vec<Slot<'_>> = Vec::new();
        for &i in level_elements {
            let groups = &all[i].base.group_ids;
            let group = groups
                .len()
                .checked_sub(level + 1)
                .map(|at| groups[at].as_str());
            let Some(group) = group else {
                slots.push(Slot::Element(i));
                continue;
            };
            match buckets.get_mut(group) {
                Some(bucket) => bucket.push(i),
                None => {
                    buckets.insert(group, vec![i]);
                    slots.push(Slot::Group(group));
                }
            }
        }
        slots
            .iter()
            .flat_map(|slot| match slot {
                Slot::Element(i) => vec![*i],
                Slot::Group(g) => order_level(&buckets[g], level + 1, all),
            })
            .collect()
    }
    let sorted = order_level(elements, 0, all);
    if sorted.len() != elements.len() {
        return elements.to_vec();
    }
    sorted
}

/// `normalizeBoundElementsOrder(elements)` (`sortElements.ts:65-113`): each
/// container's bound text right after it.
fn normalize_bound_elements_order(elements: &[usize], all: &[Element]) -> Vec<usize> {
    // arrayToMap: the last element of an id
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for &i in elements {
        by_id.insert(all[i].base.id.as_str(), i);
    }
    let mut sorted: Vec<usize> = Vec::new();
    let mut added: HashSet<usize> = HashSet::new();
    let mut add = |i: usize, sorted: &mut Vec<usize>| {
        if added.insert(i) {
            sorted.push(i);
        }
    };
    for &i in elements {
        if sorted.contains(&i) {
            continue;
        }
        let element = &all[i];
        if let Some(bound) = element
            .base
            .bound_elements
            .as_ref()
            .filter(|b| !b.is_empty())
        {
            add(i, &mut sorted);
            for b in bound {
                if let Some(&child) = by_id.get(b.id.as_str()) {
                    if b.kind == BoundElementType::Text {
                        add(child, &mut sorted);
                    }
                }
            }
            continue;
        }
        if let ElementKind::Text(t) = &element.kind {
            let listed = t
                .container_id
                .as_deref()
                .filter(|c| !c.is_empty())
                .and_then(|c| by_id.get(c))
                .is_some_and(|&c| {
                    all[c]
                        .base
                        .bound_elements
                        .iter()
                        .flatten()
                        .any(|b| b.id == element.base.id)
                });
            if listed {
                continue;
            }
        }
        add(i, &mut sorted);
    }
    if sorted.len() != elements.len() {
        return elements.to_vec();
    }
    sorted
}

/// `normalizeElementOrder(elements)` (`sortElements.ts:115-119`), as
/// positions in `all`.
fn normalize_element_order(all: &[Element]) -> Vec<usize> {
    let indices: Vec<usize> = (0..all.len()).collect();
    normalize_bound_elements_order(&defragment_groups(&indices, all), all)
}

/// `hasBoundTextElement(element)` (`typeChecks.ts:297-305`).
fn has_bound_text_element(element: &Element) -> bool {
    matches!(
        element.kind,
        ElementKind::Rectangle
            | ElementKind::StickyNote(_)
            | ElementKind::Diamond
            | ElementKind::Ellipse
            | ElementKind::Arrow(_)
    ) && element
        .base
        .bound_elements
        .iter()
        .flatten()
        .any(|b| b.kind == BoundElementType::Text)
}

/// An element of `elementsWithDuplicates`: an original (by its position in
/// the input) or a duplicate (by its position in the duplicates).
#[derive(Clone, Copy)]
enum Slot {
    Orig(usize),
    Dup(usize),
}

struct Duplicator<'a, 'e, E> {
    all: &'a [Element],
    env: &'e mut E,
    editing_group_id: Option<String>,
    randomize_seed: bool,
    processed: HashSet<String>,
    group_id_map: HashMap<String, String>,
    duplicates: Vec<Element>,
    originals: Vec<usize>,
    orig_id_to_duplicate_id: HashMap<String, String>,
    with_duplicates: Vec<Slot>,
}

impl<E: EditEnv> Duplicator<'_, '_, E> {
    fn slot(&self, slot: Slot) -> &Element {
        match slot {
            Slot::Orig(i) => &self.all[i],
            Slot::Dup(i) => &self.duplicates[i],
        }
    }

    /// `copyElements(elements)`: the duplicates of the elements not
    /// processed yet.
    fn copy(&mut self, elements: &[usize]) -> Vec<Slot> {
        let mut made = Vec::new();
        for &i in elements {
            let element = &self.all[i];
            if self.processed.contains(&element.base.id) {
                continue;
            }
            self.processed.insert(element.base.id.clone());
            let duplicate = duplicate_element(
                self.editing_group_id.as_deref(),
                &mut self.group_id_map,
                element,
                self.randomize_seed,
                self.env,
            );
            self.processed.insert(duplicate.base.id.clone());
            self.orig_id_to_duplicate_id
                .insert(element.base.id.clone(), duplicate.base.id.clone());
            self.originals.push(i);
            self.duplicates.push(duplicate);
            made.push(Slot::Dup(self.duplicates.len() - 1));
        }
        made
    }

    /// `findLastIndex(elementsWithDuplicates, predicate)`.
    fn find_last(&self, predicate: impl Fn(&Element) -> bool) -> isize {
        self.with_duplicates
            .iter()
            .rposition(|&s| predicate(self.slot(s)))
            .map_or(-1, |i| i as isize)
    }

    /// `insertBeforeOrAfterIndex(index, elements)`.
    fn insert_after(&mut self, index: isize, slots: Vec<Slot>) {
        if slots.is_empty() {
            return;
        }
        let len = self.with_duplicates.len() as isize;
        if index > len - 1 {
            self.with_duplicates.extend(slots);
            return;
        }
        let at = (index + 1).max(0) as usize;
        self.with_duplicates.splice(at..at, slots);
    }
}

/// `duplicateElements(opts)` (`duplicate.ts:142-458`) without the
/// `overrides` callback (the caller assigns over
/// [`Duplication::duplicated_elements`] and
/// [`Duplication::elements_with_duplicates`], both in the same order of
/// duplicates): the elements normalised (`normalizeElementOrder`), each
/// one to duplicate copied with what belongs with it (a selected group's
/// elements, a frame's children, a container's bound text, a label's
/// container), the copies inserted after their originals, then the
/// duplicates' bindings, bound elements and containers pointed at each
/// other (`fixDuplicatedBindingsAfterDuplication`) and their frames
/// (`bindElementsToFramesAfterDuplication`, a version bump where the frame
/// changes).
///
/// An error is what upstream's elbow arrow routing throws.
pub fn duplicate_elements<E: EditEnv>(
    elements: &[Element],
    kind: &DuplicateType,
    randomize_seed: bool,
    env: &mut E,
) -> Result<Duplication, ElbowArrowError> {
    let (editing_group_id, selected_group_ids, preserve_frame_children_order) = match kind {
        DuplicateType::Everything {
            preserve_frame_children_order,
        } => (None, Map::new(), *preserve_frame_children_order),
        DuplicateType::InPlace {
            editing_group_id,
            selected_group_ids,
            ..
        } => (editing_group_id.clone(), selected_group_ids.clone(), false),
    };
    let mut to_duplicate: HashSet<String> = match kind {
        DuplicateType::Everything { .. } => elements.iter().map(|e| e.base.id.clone()).collect(),
        DuplicateType::InPlace { ids, .. } => ids.iter().cloned().collect(),
    };
    if matches!(kind, DuplicateType::InPlace { .. }) {
        // for sanity
        for group in selected_group_ids.keys() {
            for element in elements {
                if element.base.group_ids.iter().any(|g| g == group) {
                    to_duplicate.insert(element.base.id.clone());
                }
            }
        }
    }

    let order = normalize_element_order(elements);
    let elements_map = ElementsMap::new(elements.iter());
    let position_of = |element: &Element| {
        elements
            .iter()
            .position(|e| std::ptr::eq(e, element))
            .expect("an element of the scene")
    };

    let mut d = Duplicator {
        all: elements,
        env,
        editing_group_id: editing_group_id.clone(),
        randomize_seed,
        processed: HashSet::new(),
        group_id_map: HashMap::new(),
        duplicates: Vec::new(),
        originals: Vec::new(),
        orig_id_to_duplicate_id: HashMap::new(),
        with_duplicates: order.iter().map(|&i| Slot::Orig(i)).collect(),
    };

    let frames_to_duplicate: HashSet<&str> = order
        .iter()
        .map(|&i| &elements[i])
        .filter(|e| to_duplicate.contains(&e.base.id) && is_frame_like(e))
        .map(|e| e.base.id.as_str())
        .collect();
    let frame_children = |frame: &str| -> Vec<usize> {
        order
            .iter()
            .copied()
            .filter(|&i| elements[i].base.frame_id.as_deref() == Some(frame))
            .collect()
    };

    for &i in &order {
        let element = &elements[i];
        if d.processed.contains(&element.base.id) || !to_duplicate.contains(&element.base.id) {
            continue;
        }

        // groups
        let group = element
            .base
            .group_ids
            .iter()
            .filter(|g| Some(g.as_str()) != editing_group_id.as_deref())
            .find(|g| truthy(selected_group_ids.get(g.as_str())))
            .cloned();
        if let Some(group) = group {
            let group_elements: Vec<usize> = order
                .iter()
                .copied()
                .filter(|&j| elements[j].base.group_ids.contains(&group))
                .flat_map(|j| {
                    if is_frame_like(&elements[j]) && !preserve_frame_children_order {
                        let mut v = frame_children(&elements[j].base.id);
                        v.push(j);
                        v
                    } else {
                        vec![j]
                    }
                })
                .collect();
            let target = d.find_last(|e| e.base.group_ids.contains(&group));
            let copies = d.copy(&group_elements);
            d.insert_after(target, copies);
            continue;
        }

        // frame duplication
        if !preserve_frame_children_order
            && frame_id(element).is_some_and(|f| frames_to_duplicate.contains(f))
        {
            continue;
        }
        if is_frame_like(element) {
            let frame = element.base.id.clone();
            if preserve_frame_children_order {
                let target = d.find_last(|e| e.base.id == frame);
                let copies = d.copy(&[i]);
                d.insert_after(target, copies);
                continue;
            }
            let mut list = frame_children(&frame);
            list.push(i);
            let target = d.find_last(|e| {
                e.base.frame_id.as_deref() == Some(frame.as_str()) || e.base.id == frame
            });
            let copies = d.copy(&list);
            d.insert_after(target, copies);
            continue;
        }

        // text container
        if has_bound_text_element(element) {
            let bound_text = get_bound_text_element(element, &elements_map);
            let id = element.base.id.clone();
            let target = d.find_last(|e| {
                e.base.id == id
                    || matches!(&e.kind, ElementKind::Text(t) if t.container_id.as_deref() == Some(id.as_str()))
            });
            let list = match bound_text {
                Some(text) => vec![i, position_of(text)],
                None => vec![i],
            };
            let copies = d.copy(&list);
            d.insert_after(target, copies);
            continue;
        }
        if is_bound_to_container(element) {
            let container = get_container_element(element, &elements_map);
            let id = element.base.id.clone();
            let container_id = container.map(|c| c.base.id.clone());
            let target =
                d.find_last(|e| e.base.id == id || Some(&e.base.id) == container_id.as_ref());
            let list = match container {
                Some(container) => vec![position_of(container), i],
                None => vec![i],
            };
            let copies = d.copy(&list);
            d.insert_after(target, copies);
            continue;
        }

        // default duplication (regular elements)
        let id = element.base.id.clone();
        let target = d.find_last(|e| e.base.id == id);
        let copies = d.copy(&[i]);
        d.insert_after(target, copies);
    }

    fix_duplicated_bindings_after_duplication(&mut d.duplicates, &d.orig_id_to_duplicate_id)?;

    // bindElementsToFramesAfterDuplication
    for (k, &orig) in d.originals.iter().enumerate() {
        let Some(frame) = frame_id(&elements[orig]) else {
            continue;
        };
        let next_frame = d.orig_id_to_duplicate_id.get(frame).cloned();
        set_frame_id(&mut d.duplicates[k], next_frame.as_deref(), d.env);
    }

    let elements_with_duplicates = d
        .with_duplicates
        .iter()
        .map(|&s| d.slot(s).clone())
        .collect();
    Ok(Duplication {
        duplicated_elements: d.duplicates,
        elements_with_duplicates,
        orig_id_to_duplicate_id: d.orig_id_to_duplicate_id,
    })
}

/// Assigns over both copies of each duplicate (`Object.assign(duplicate,
/// ...)`, no version bump).
fn assign_duplicates(duplication: &mut Duplication, mut assign: impl FnMut(&mut Element, usize)) {
    let positions: HashMap<String, usize> = duplication
        .duplicated_elements
        .iter()
        .enumerate()
        .map(|(k, e)| (e.base.id.clone(), k))
        .collect();
    for (k, duplicate) in duplication.duplicated_elements.iter_mut().enumerate() {
        assign(duplicate, k);
    }
    for element in duplication.elements_with_duplicates.iter_mut() {
        if let Some(&k) = positions.get(&element.base.id) {
            assign(element, k);
        }
    }
}

/// `actionDuplicateSelection.perform` (`actionDuplicateSelection.tsx:
/// 35-121`): the selection (with its bound text and frames' children)
/// duplicated in place (`duplicateElements` with `type: "in-place"` and
/// new seeds), each duplicate `DEFAULT_GRID_SIZE / 2` right and down of
/// its original and in its original's frame (or that frame's duplicate),
/// the duplicates' fractional indices synced (`syncMovedIndices`) and the
/// duplicates selected (`getSelectionStateForElements`). Sets
/// `selectedLinearElement`, `selectedElementIds`, `selectedGroupIds` and
/// `editingGroupId`; captured.
///
/// `None` while the selection is dragged. Not ported: the host's
/// `onDuplicate` hook, and duplicating the selected points of a linear
/// element being edited (`LinearElementEditor.duplicateSelectedPoints`),
/// where the port returns `None`. An error routing a duplicated elbow
/// arrow, or syncing the indices, where upstream throws, is `None` too.
pub fn duplicate_selection<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    env: &mut E,
) -> Option<ActionResult> {
    if truthy(app_state.get("selectedElementsAreBeingDragged")) {
        return None;
    }
    if is_editing_linear_element(app_state) {
        return None;
    }
    let selected = object_key(app_state, "selectedElementIds");
    let all: Vec<&Element> = elements.iter().collect();
    // arrayToMap: each id once
    let mut ids: Vec<String> = Vec::new();
    for element in get_selected_elements(&all, &selected, true, true) {
        if !ids.contains(&element.base.id) {
            ids.push(element.base.id.clone());
        }
    }
    let kind = DuplicateType::InPlace {
        ids,
        editing_group_id: editing_group_id(app_state),
        selected_group_ids: object_key(app_state, "selectedGroupIds"),
    };
    let mut duplication = duplicate_elements(elements, &kind, true, env).ok()?;

    // the overrides: next to the original, in its frame's duplicate
    let by_id: HashMap<&str, &Element> = elements.iter().map(|e| (e.base.id.as_str(), e)).collect();
    let dup_to_orig: HashMap<String, String> = duplication
        .orig_id_to_duplicate_id
        .iter()
        .map(|(o, d)| (d.clone(), o.clone()))
        .collect();
    let overrides: Vec<Option<(f64, f64, Option<String>)>> = duplication
        .duplicated_elements
        .iter()
        .map(|duplicate| {
            let orig = by_id.get(dup_to_orig.get(&duplicate.base.id)?.as_str())?;
            let frame = match frame_id(orig) {
                Some(f) => duplication
                    .orig_id_to_duplicate_id
                    .get(f)
                    .cloned()
                    .or_else(|| orig.base.frame_id.clone()),
                None => orig.base.frame_id.clone(),
            };
            Some((
                orig.base.x + DEFAULT_GRID_SIZE / 2.0,
                orig.base.y + DEFAULT_GRID_SIZE / 2.0,
                frame,
            ))
        })
        .collect();
    assign_duplicates(&mut duplication, |element, k| {
        if let Some((x, y, frame)) = &overrides[k] {
            element.base.x = *x;
            element.base.y = *y;
            element.base.frame_id = frame.clone();
        }
    });

    let mut next = duplication.elements_with_duplicates;
    let moved: HashSet<String> = duplication
        .duplicated_elements
        .iter()
        .map(|e| e.base.id.clone())
        .collect();
    sync_moved_indices(&mut next, &moved, env).ok()?;

    let live: Vec<&Element> = next.iter().filter(|e| !e.base.is_deleted).collect();
    let targets: Vec<&Element> = duplication
        .duplicated_elements
        .iter()
        .filter_map(|d| next.iter().find(|e| e.base.id == d.base.id))
        .collect();
    let patch = selection_state_for_elements(&targets, &live, app_state);
    Some(ActionResult {
        elements: Some(next),
        app_state: patch,
        capture: true,
    })
}
