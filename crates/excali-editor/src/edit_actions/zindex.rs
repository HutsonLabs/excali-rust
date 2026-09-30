//! The z-order actions (`packages/excalidraw/actions/actionZindex.tsx`)
//! over `moveOneLeft`, `moveOneRight`, `moveAllLeft` and `moveAllRight`
//! (`packages/element/src/zindex.ts`).
//!
//! Upstream reorders arrays of the scene's element objects and finds them
//! by identity (`indexOf`); the port reorders positions in the input and
//! finds an element by its id, which is unique in a scene.

use std::collections::{HashMap, HashSet};

use excali_core::app_state::AppState;
use excali_core::element::{BoundElementType, Element, ElementKind};
use excali_core::fractional_index::sync_moved_indices;
use excali_scene::frame::is_frame_like;
use serde_json::Map;

use super::{editing_group_id, frame_id, get_selected_elements, object_key, ActionResult, EditEnv};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Left,
    Right,
}

/// The z-order being built: positions in the input elements.
struct Order<'a> {
    all: &'a [Element],
    /// The scene's elements by id (`scene.getElement`).
    by_id: HashMap<&'a str, usize>,
}

impl<'a> Order<'a> {
    fn new(all: &'a [Element]) -> Order<'a> {
        let mut by_id = HashMap::new();
        for (i, e) in all.iter().enumerate() {
            by_id.insert(e.base.id.as_str(), i);
        }
        Order { all, by_id }
    }

    fn el(&self, i: usize) -> &'a Element {
        &self.all[i]
    }

    /// `elements.indexOf(element)` of the element at input position `p`.
    fn index_of(order: &[usize], p: usize) -> isize {
        order
            .iter()
            .position(|&q| q == p)
            .map_or(-1, |i| i as isize)
    }
}

/// `getIndicesToMove(elements, appState, elementsToBeMoved)`
/// (`zindex.ts:28-66`): the positions of the elements to move, with the
/// deleted elements between two of them.
fn get_indices_to_move(o: &Order<'_>, order: &[usize], ids: &HashSet<&str>) -> Vec<usize> {
    let mut selected: Vec<usize> = Vec::new();
    let mut deleted: Vec<usize> = Vec::new();
    let mut include_deleted_index: Option<usize> = None;
    for (index, &p) in order.iter().enumerate() {
        let element = o.el(p);
        if ids.contains(element.base.id.as_str()) {
            if !deleted.is_empty() {
                selected.append(&mut deleted);
            }
            selected.push(index);
            include_deleted_index = Some(index + 1);
        } else if element.base.is_deleted && include_deleted_index == Some(index) {
            include_deleted_index = Some(index + 1);
            deleted.push(index);
        } else {
            deleted.clear();
        }
    }
    selected
}

/// The ids `getSelectedElements(elements, appState, { includeBoundTextElement,
/// includeElementsInFrames })` gives.
fn selected_ids<'a>(o: &Order<'a>, order: &[usize], app_state: &AppState) -> Vec<&'a Element> {
    let refs: Vec<&Element> = order.iter().map(|&p| o.el(p)).collect();
    get_selected_elements(
        &refs,
        &object_key(app_state, "selectedElementIds"),
        true,
        true,
    )
}

/// `toContiguousGroups(array)` (`zindex.ts:68-77`).
fn to_contiguous_groups(array: &[usize]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, &value) in array.iter().enumerate() {
        if i == 0 || array[i - 1] + 1 != value {
            groups.push(Vec::new());
        }
        groups.last_mut().expect("a group").push(value);
    }
    groups
}

/// `getTargetIndexAccountingForBinding(nextElement, elements, direction,
/// scene)` (`zindex.ts:84-125`): the far side of a container and its
/// bound text, taken as one.
fn target_index_accounting_for_binding(
    o: &Order<'_>,
    order: &[usize],
    next: usize,
    direction: Direction,
) -> Option<isize> {
    let element = o.el(next);
    let pick = |other: usize| {
        let a = Order::index_of(order, other);
        let b = Order::index_of(order, next);
        match direction {
            Direction::Left => a.min(b),
            Direction::Right => a.max(b),
        }
    };
    let container = match &element.kind {
        ElementKind::Text(t) => t.container_id.as_deref().filter(|c| !c.is_empty()),
        _ => None,
    };
    if let Some(container) = container {
        return o.by_id.get(container).map(|&c| pick(c));
    }
    let bound = element
        .base
        .bound_elements
        .iter()
        .flatten()
        .find(|b| b.kind != BoundElementType::Arrow)
        .map(|b| b.id.as_str())
        .filter(|id| !id.is_empty())?;
    o.by_id.get(bound).map(|&t| pick(t))
}

/// `getContiguousFrameRangeElements(allElements, frameId)`
/// (`zindex.ts:127-147`): the first and last positions of the frame and
/// its children.
fn contiguous_frame_range(o: &Order<'_>, order: &[usize], frame: &str) -> Option<(usize, usize)> {
    let of_frame = |p: usize| {
        let e = o.el(p);
        e.base.frame_id.as_deref() == Some(frame) || e.base.id == frame
    };
    let start = order.iter().position(|&p| of_frame(p))?;
    let end = order.iter().rposition(|&p| of_frame(p))?;
    Some((start, end))
}

/// `getTargetIndex(appState, elements, boundaryIndex, direction,
/// containingFrame, scene)` (`zindex.ts:190-288`): where the elements at
/// the boundary move to, one step over the next visible element (its
/// whole group, frame or container with it), or -1.
fn get_target_index(
    o: &Order<'_>,
    editing: Option<&str>,
    order: &[usize],
    boundary: usize,
    direction: Direction,
    containing_frame: Option<&str>,
) -> isize {
    let source = order.get(boundary).map(|&p| o.el(p));
    let index_filter = |p: usize| {
        let e = o.el(p);
        if e.base.is_deleted {
            return false;
        }
        if let Some(frame) = containing_frame {
            return e.base.frame_id.as_deref() == Some(frame);
        }
        // if we're editing group, find closest sibling irrespective of
        // whether there's a different-group element between them
        if let Some(editing) = editing {
            return e.base.group_ids.iter().any(|g| g == editing);
        }
        true
    };
    let candidate: Option<usize> = match direction {
        // findLastIndex(elements, filter, Math.max(0, boundaryIndex - 1))
        Direction::Left => {
            let from = boundary
                .saturating_sub(1)
                .min(order.len().saturating_sub(1));
            if order.is_empty() {
                None
            } else {
                (0..=from).rev().find(|&i| index_filter(order[i]))
            }
        }
        Direction::Right => (boundary + 1..order.len()).find(|&i| index_filter(order[i])),
    };
    let Some(candidate) = candidate else {
        return -1;
    };
    let next_p = order[candidate];
    let next = o.el(next_p);
    let with_binding = || {
        target_index_accounting_for_binding(o, order, next_p, direction)
            .unwrap_or(candidate as isize)
    };

    if let Some(editing) = editing {
        if source.map(|s| s.base.group_ids.concat()) == Some(next.base.group_ids.concat()) {
            // candidate element is a sibling in current editing group
            return with_binding();
        } else if !next.base.group_ids.iter().any(|g| g == editing) {
            // candidate element is outside current editing group → prevent
            return -1;
        }
    }

    if containing_frame.is_none() && (frame_id(next).is_some() || is_frame_like(next)) {
        let frame = frame_id(next).unwrap_or(next.base.id.as_str());
        return match contiguous_frame_range(o, order, frame) {
            Some((start, end)) => match direction {
                Direction::Left => start as isize,
                Direction::Right => end as isize,
            },
            None => -1,
        };
    }

    if next.base.group_ids.is_empty() {
        return with_binding();
    }

    let groups = &next.base.group_ids;
    let sibling_group = match editing {
        Some(editing) => groups
            .iter()
            .position(|g| g == editing)
            .and_then(|i| i.checked_sub(1))
            .map(|i| &groups[i]),
        None => groups.last(),
    };
    if let Some(sibling_group) = sibling_group {
        let in_group: Vec<usize> = (0..order.len())
            .filter(|&i| o.el(order[i]).base.group_ids.contains(sibling_group))
            .collect();
        if let (Some(&first), Some(&last)) = (in_group.first(), in_group.last()) {
            return match direction {
                Direction::Left => first as isize,
                Direction::Right => last as isize,
            };
        }
    }
    candidate as isize
}

/// `elements.slice(start, end)` over positions, clamped as JavaScript
/// clamps (negative counts from the end).
fn slice(order: &[usize], start: isize, end: Option<isize>) -> Vec<usize> {
    let len = order.len() as isize;
    let clamp = |i: isize| {
        if i < 0 {
            (len + i).max(0)
        } else {
            i.min(len)
        }
    };
    let s = clamp(start);
    let e = end.map_or(len, clamp);
    if s >= e {
        return Vec::new();
    }
    order[s as usize..e as usize].to_vec()
}

/// `hasSameElementIds(prevElements, nextElements)` (`zindex.ts:303-335`).
fn has_same_element_ids(o: &Order<'_>, prev: &[usize], next: &[usize]) -> bool {
    if prev.len() != next.len() {
        return false;
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for &p in prev {
        *counts.entry(o.el(p).base.id.as_str()).or_default() += 1;
    }
    for &p in next {
        match counts.get_mut(o.el(p).base.id.as_str()) {
            Some(n) if *n > 0 => *n -= 1,
            _ => return false,
        }
    }
    true
}

/// `shiftElementsByOne(elements, appState, direction, scene)`
/// (`zindex.ts:337-424`). Returns the new order and the moved positions.
fn shift_elements_by_one(
    o: &Order<'_>,
    app_state: &AppState,
    direction: Direction,
) -> (Vec<usize>, Vec<usize>) {
    let original: Vec<usize> = (0..o.all.len()).collect();
    let mut elements = original.clone();
    let editing = editing_group_id(app_state);
    let ids: HashSet<&str> = selected_ids(o, &elements, app_state)
        .iter()
        .map(|e| e.base.id.as_str())
        .collect();
    let indices = get_indices_to_move(o, &elements, &ids);
    let targets: Vec<usize> = indices.iter().map(|&i| elements[i]).collect();
    let mut grouped = to_contiguous_groups(&indices);
    if direction == Direction::Right {
        grouped.reverse();
    }
    let selected_frames: HashSet<&str> = indices
        .iter()
        .map(|&i| o.el(elements[i]))
        .filter(|e| is_frame_like(e))
        .map(|e| e.base.id.as_str())
        .collect();

    for group in grouped {
        let leading = group[0];
        let trailing = group[group.len() - 1];
        let boundary = if direction == Direction::Left {
            leading
        } else {
            trailing
        };
        // upstream reads the positions against the array as reordered so far
        let frame_selected = group.iter().any(|&i| {
            elements
                .get(i)
                .and_then(|&p| frame_id(o.el(p)))
                .is_some_and(|f| selected_frames.contains(f))
        });
        let containing_frame: Option<String> = if frame_selected {
            None
        } else {
            elements
                .get(boundary)
                .and_then(|&p| frame_id(o.el(p)))
                .map(str::to_owned)
        };
        let target = get_target_index(
            o,
            editing.as_deref(),
            &elements,
            boundary,
            direction,
            containing_frame.as_deref(),
        );
        if target == -1 || boundary as isize == target {
            continue;
        }
        let (leading, trailing) = (leading as isize, trailing as isize);
        elements = match direction {
            Direction::Left => [
                slice(&elements, 0, Some(target)),
                slice(&elements, leading, Some(trailing + 1)),
                slice(&elements, target, Some(leading)),
                slice(&elements, trailing + 1, None),
            ]
            .concat(),
            Direction::Right => [
                slice(&elements, 0, Some(leading)),
                slice(&elements, trailing + 1, Some(target + 1)),
                slice(&elements, leading, Some(trailing + 1)),
                slice(&elements, target + 1, None),
            ]
            .concat(),
        };
    }
    if !has_same_element_ids(o, &original, &elements) {
        return (original, Vec::new());
    }
    (elements, targets)
}

/// `shiftElementsToEnd(elements, appState, direction, containingFrame,
/// elementsToBeMoved)` (`zindex.ts:426-532`). Returns the new order and
/// the moved positions.
fn shift_elements_to_end(
    o: &Order<'_>,
    elements: &[usize],
    app_state: &AppState,
    direction: Direction,
    containing_frame: Option<&str>,
    to_move: &[usize],
) -> (Vec<usize>, Vec<usize>) {
    let unchanged = (elements.to_vec(), Vec::new());
    let ids: HashSet<&str> = to_move.iter().map(|&p| o.el(p).base.id.as_str()).collect();
    let indices = get_indices_to_move(o, elements, &ids);
    // nothing to move
    if indices.is_empty() {
        return unchanged;
    }
    // getTargetElementsMap: each id once, in order
    let mut targets: Vec<usize> = Vec::new();
    let mut target_ids: HashSet<&str> = HashSet::new();
    for &i in &indices {
        let p = elements[i];
        let id = o.el(p).base.id.as_str();
        if target_ids.insert(id) {
            targets.push(p);
        } else if let Some(slot) = targets.iter_mut().find(|q| o.el(**q).base.id == id) {
            *slot = p;
        }
    }
    let editing = editing_group_id(app_state);
    let of_frame = |p: usize, frame: &str| {
        let e = o.el(p);
        e.base.frame_id.as_deref() == Some(frame) || e.base.id == frame
    };
    let group_bounds = |group: &str| -> Option<(usize, usize)> {
        let first = elements
            .iter()
            .position(|&p| o.el(p).base.group_ids.iter().any(|g| g == group))?;
        let last = elements
            .iter()
            .rposition(|&p| o.el(p).base.group_ids.iter().any(|g| g == group))?;
        Some((first, last))
    };
    let (mut leading, trailing): (isize, isize) = match direction {
        Direction::Left => {
            let leading = if let Some(frame) = containing_frame {
                elements
                    .iter()
                    .position(|&p| of_frame(p, frame))
                    .map_or(-1, |i| i as isize)
            } else if let Some(editing) = editing.as_deref() {
                let Some((first, _)) = group_bounds(editing) else {
                    return unchanged;
                };
                first as isize
            } else {
                0
            };
            (leading, *indices.last().expect("indices") as isize)
        }
        Direction::Right => {
            let trailing = if let Some(frame) = containing_frame {
                elements
                    .iter()
                    .rposition(|&p| of_frame(p, frame))
                    .map_or(-1, |i| i as isize)
            } else if let Some(editing) = editing.as_deref() {
                let Some((_, last)) = group_bounds(editing) else {
                    return unchanged;
                };
                last as isize
            } else {
                elements.len() as isize - 1
            };
            (indices[0] as isize, trailing)
        }
    };
    if leading == -1 {
        leading = 0;
    }
    if leading < 0
        || trailing < 0
        || leading > trailing
        || indices
            .iter()
            .any(|&i| (i as isize) < leading || i as isize > trailing)
    {
        return unchanged;
    }
    let (leading, trailing) = (leading as usize, trailing as usize);
    let displaced: Vec<usize> = (leading..=trailing)
        .filter(|i| !indices.contains(i))
        .map(|i| elements[i])
        .collect();
    let before = &elements[..leading];
    let after = &elements[trailing + 1..];
    let next: Vec<usize> = match direction {
        Direction::Left => [before, &targets, &displaced, after].concat(),
        Direction::Right => [before, &displaced, &targets, after].concat(),
    };
    if !has_same_element_ids(o, elements, &next) {
        return unchanged;
    }
    (next, targets)
}

/// `shiftElementsAccountingForFrames(allElements, appState, direction,
/// shiftFunction)` (`zindex.ts:534-608`): frames' children moved within
/// their frames first (frame by frame), then everything else. Returns
/// each pass's order and moved positions.
fn shift_elements_accounting_for_frames(
    o: &Order<'_>,
    app_state: &AppState,
    direction: Direction,
) -> Vec<(Vec<usize>, Vec<usize>)> {
    let all: Vec<usize> = (0..o.all.len()).collect();
    let to_move: HashSet<&str> = selected_ids(o, &all, app_state)
        .iter()
        .map(|e| e.base.id.as_str())
        .collect();
    let fully_selected_frames: HashSet<&str> = all
        .iter()
        .map(|&p| o.el(p))
        .filter(|e| to_move.contains(e.base.id.as_str()) && is_frame_like(e))
        .map(|e| e.base.id.as_str())
        .collect();
    let mut regular: Vec<usize> = Vec::new();
    let mut frame_children: Vec<(&str, Vec<usize>)> = Vec::new();
    for &p in &all {
        let e = o.el(p);
        if !to_move.contains(e.base.id.as_str()) {
            continue;
        }
        let frame = frame_id(e);
        if is_frame_like(e) || frame.is_some_and(|f| fully_selected_frames.contains(f)) {
            regular.push(p);
        } else if let Some(frame) = frame {
            match frame_children.iter_mut().find(|(f, _)| *f == frame) {
                Some((_, children)) => children.push(p),
                None => frame_children.push((frame, vec![p])),
            }
        } else {
            regular.push(p);
        }
    }
    let mut next = all;
    let mut passes = Vec::new();
    for (frame, children) in &frame_children {
        let pass = shift_elements_to_end(o, &next, app_state, direction, Some(frame), children);
        next = pass.0.clone();
        passes.push(pass);
    }
    passes.push(shift_elements_to_end(
        o, &next, app_state, direction, None, &regular,
    ));
    passes
}

/// The elements after the passes: each pass's moved elements get their
/// fractional indices synced over the order that pass produced
/// (`syncMovedIndices` mutates the shifted array's elements, which a later
/// pass then reorders), then the last order.
fn materialize<E: EditEnv>(
    o: &Order<'_>,
    passes: Vec<(Vec<usize>, Vec<usize>)>,
    env: &mut E,
) -> Option<Vec<Element>> {
    let mut current: Vec<Element> = o.all.to_vec();
    let mut last: Vec<usize> = (0..o.all.len()).collect();
    for (order, moved) in passes {
        if !moved.is_empty() {
            let mut next: Vec<Element> = order.iter().map(|&p| current[p].clone()).collect();
            let ids: HashSet<String> = moved.iter().map(|&p| o.el(p).base.id.clone()).collect();
            sync_moved_indices(&mut next, &ids, env).ok()?;
            for (element, &p) in next.into_iter().zip(&order) {
                current[p] = element;
            }
        }
        last = order;
    }
    Some(last.iter().map(|&p| current[p].clone()).collect())
}

fn result(elements: Vec<Element>) -> Option<ActionResult> {
    Some(ActionResult {
        elements: Some(elements),
        app_state: Map::new(),
        capture: true,
        never: false,
    })
}

/// `moveOneLeft` / `moveOneRight`.
fn move_one<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    direction: Direction,
    env: &mut E,
) -> Option<ActionResult> {
    let o = Order::new(elements);
    let pass = shift_elements_by_one(&o, app_state, direction);
    result(materialize(&o, vec![pass], env)?)
}

/// `moveAllLeft` / `moveAllRight`.
fn move_all<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    direction: Direction,
    env: &mut E,
) -> Option<ActionResult> {
    let o = Order::new(elements);
    let passes = shift_elements_accounting_for_frames(&o, app_state, direction);
    result(materialize(&o, passes, env)?)
}

/// `actionSendBackward.perform` (`actionZindex.tsx:23-35`, `moveOneLeft`):
/// the selection (with bound text and frames' children) one step back, over
/// the next element below with its group, frame or container, within the
/// group being edited or the selection's frame. The moved elements'
/// fractional indices are synced. Sets no app state; captured.
pub fn send_backward<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    env: &mut E,
) -> Option<ActionResult> {
    move_one(elements, app_state, Direction::Left, env)
}

/// `actionBringForward.perform` (`actionZindex.tsx:53-65`,
/// `moveOneRight`): as [`send_backward`], one step forward.
pub fn bring_forward<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    env: &mut E,
) -> Option<ActionResult> {
    move_one(elements, app_state, Direction::Right, env)
}

/// `actionSendToBack.perform` (`actionZindex.tsx:83-95`, `moveAllLeft`):
/// the selection to the back, frames' children to the back of their frame
/// and within the group being edited to its back. Sets no app state;
/// captured.
pub fn send_to_back<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    env: &mut E,
) -> Option<ActionResult> {
    move_all(elements, app_state, Direction::Left, env)
}

/// `actionBringToFront.perform` (`actionZindex.tsx:121-133`,
/// `moveAllRight`): as [`send_to_back`], to the front.
pub fn bring_to_front<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    env: &mut E,
) -> Option<ActionResult> {
    move_all(elements, app_state, Direction::Right, env)
}
