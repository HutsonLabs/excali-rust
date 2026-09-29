//! Dragging the selection: `dragSelectedElements`
//! (`packages/element/src/dragElements.ts:39-163`) as `App.onPointerMove`
//! calls it (`App.tsx`, `handlePointerMoveOverScenePointerDownState`): the
//! offset is the pointer's scene position less where the press started, and
//! every element is placed from its original, the copy taken at the press.
//!
//! The snap offset is what `snapDraggedElements` found (the editor runs
//! it first); an axis it did not snap is left to the grid
//! (`calculateOffset`).

use std::collections::{HashMap, HashSet};

use excali_core::element::{Element, ElementKind};
use excali_editor::binding::{
    unbind_binding_element, update_bound_elements, BindingEnd, BindingEnv,
};
use excali_editor::scene::{ElementUpdate, Scene};
use excali_editor::transform::get_grid_point;
use excali_scene::bounds::{get_bound_text_element_id, get_common_bounds};

/// `DRAGGING_THRESHOLD` (`common/src/constants.ts:28`), px.
pub const DRAGGING_THRESHOLD: f64 = 10.0;

fn is_arrow(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Arrow(_))
}

fn is_elbow_arrow(e: &Element) -> bool {
    matches!(&e.kind, ElementKind::Arrow(a) if a.elbowed)
}

fn bindings(e: &Element) -> (Option<&str>, Option<&str>) {
    match (&e.kind, e.kind.linear()) {
        (ElementKind::Arrow(_), Some(l)) => (
            l.start_binding.as_ref().map(|b| b.element_id.as_str()),
            l.end_binding.as_ref().map(|b| b.element_id.as_str()),
        ),
        _ => (None, None),
    }
}

/// `updateElementCoords` (`dragElements.ts:200-216`).
fn update_element_coords(
    originals: &HashMap<String, Element>,
    scene: &mut Scene,
    id: &str,
    offset: [f64; 2],
    env: &mut dyn BindingEnv,
) {
    let Some(original) = originals.get(id).or_else(|| scene.get(id)).cloned() else {
        return;
    };
    scene.mutate_element(
        id,
        ElementUpdate::position(original.base.x + offset[0], original.base.y + offset[1]),
        env,
    );
}

/// `calculateOffset(commonBounds, dragOffset, snapOffset, gridSize)`
/// (`dragElements.ts:169-198`).
fn calculate_offset(
    bounds: [f64; 4],
    drag: [f64; 2],
    snap: [f64; 2],
    grid_size: Option<f64>,
) -> [f64; 2] {
    let [x, y, _, _] = bounds;
    let mut next = [x + drag[0] + snap[0], y + drag[1] + snap[1]];
    if snap[0] == 0.0 || snap[1] == 0.0 {
        let [grid_x, grid_y] = get_grid_point(x + drag[0], y + drag[1], grid_size);
        if snap[0] == 0.0 {
            next[0] = grid_x;
        }
        if snap[1] == 0.0 {
            next[1] = grid_y;
        }
    }
    [next[0] - x, next[1] - y]
}

/// `dragSelectedElements(pointerDownState, selectedElements, offset, scene,
/// snapOffset, gridSize)`: `selected` are the selected elements' ids
/// (bound text left out), `originals` the scene's elements at the press.
pub fn drag_selected_elements(
    originals: &HashMap<String, Element>,
    selected: &[String],
    offset: [f64; 2],
    scene: &mut Scene,
    snap_offset: [f64; 2],
    grid_size: Option<f64>,
    env: &mut dyn BindingEnv,
) {
    let selected_elements: Vec<Element> = selected
        .iter()
        .filter_map(|id| scene.get_non_deleted(id).cloned())
        .collect();
    if let [only] = selected_elements.as_slice() {
        let (start, end) = bindings(only);
        if is_elbow_arrow(only) && (start.is_some() || end.is_some()) {
            return;
        }
    }
    let ids: HashSet<&str> = selected_elements
        .iter()
        .map(|e| e.base.id.as_str())
        .collect();
    let kept: Vec<&Element> = selected_elements
        .iter()
        .filter(|e| {
            let (start, end) = bindings(e);
            match (is_elbow_arrow(e), start, end) {
                (true, Some(s), Some(t)) => ids.contains(s) && ids.contains(t),
                _ => true,
            }
        })
        .collect();

    // a frame and its children are moved once each
    let mut to_update: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for e in &kept {
        if seen.insert(e.base.id.clone()) {
            to_update.push(e.base.id.clone());
        }
    }
    let frames: HashSet<&str> = kept
        .iter()
        .filter(|e| e.element_type().is_frame_like())
        .map(|e| e.base.id.as_str())
        .collect();
    if !frames.is_empty() {
        for e in scene.non_deleted() {
            if e.base
                .frame_id
                .as_deref()
                .is_some_and(|f| frames.contains(f))
                && seen.insert(e.base.id.clone())
            {
                to_update.push(e.base.id.clone());
            }
        }
    }

    let mut orig_elements: Vec<&Element> = Vec::new();
    for id in &to_update {
        match originals.get(id) {
            Some(e) => orig_elements.push(e),
            // no original (duplicated during the drag): nothing moves
            None => return,
        }
    }
    let adjusted = calculate_offset(
        get_common_bounds(&orig_elements),
        offset,
        snap_offset,
        grid_size,
    );
    let update_ids: HashSet<String> = to_update.iter().cloned().collect();

    for id in &to_update {
        let Some(element) = scene.get(id).cloned() else {
            continue;
        };
        if !is_arrow(&element) {
            update_element_coords(originals, scene, id, adjusted, env);
            // skip arrow labels since their position is calculated at render
            let text = get_bound_text_element_id(&element)
                .and_then(|t| scene.get_non_deleted(t))
                .map(|t| t.base.id.clone());
            if let Some(text) = text {
                update_element_coords(originals, scene, &text, adjusted, env);
            }
            update_bound_elements(scene, env, id, Some(&to_update), None);
        } else {
            let (start, end) = bindings(&element);
            if to_update.len() > 1
                || adjusted[0].abs().max(adjusted[1].abs()) > DRAGGING_THRESHOLD
                || (start.is_none() && end.is_none())
            {
                update_element_coords(originals, scene, id, adjusted, env);
                let unbind_start = start.is_none_or(|s| !update_ids.contains(s));
                let unbind_end = end.is_none_or(|s| !update_ids.contains(s));
                if unbind_start {
                    unbind_binding_element(scene, env, id, BindingEnd::Start);
                }
                if unbind_end {
                    unbind_binding_element(scene, env, id, BindingEnd::End);
                }
            }
        }
    }
}
