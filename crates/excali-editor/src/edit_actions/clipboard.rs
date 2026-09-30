//! Copy and paste: the clipboard JSON `actionCopy` writes
//! (`packages/excalidraw/actions/actionClipboard.tsx`, `copyToClipboard`
//! of `packages/excalidraw/clipboard.ts`) and the insertion of pasted
//! elements (`App.addElementsFromPasteOrLibrary`,
//! `packages/excalidraw/components/App.tsx:4885-4990`, with
//! `AppDuplicate.duplicateAtSceneCoords` of `App.duplicate.ts`).

use std::collections::HashSet;

use excali_core::app_state::AppState;
use excali_core::clipboard::{parse_clipboard, serialize_as_clipboard_json, ClipboardData};
use excali_core::element::Element;
use excali_core::fractional_index::sync_moved_indices;
use excali_core::restore::{restore_elements, RestoreElementsOptions};
use excali_math::js;
use excali_scene::bounds::get_common_bounds;
use serde_json::{Map, Value};

use super::duplicate::{duplicate_elements, DuplicateType};
use super::{
    bump, exclude_elements_in_frames_from_selection, get_selected_elements, non_deleted,
    object_key, selection_state_for_elements, ActionResult, EditEnv,
};

/// The clipboard JSON of `actionCopy.perform` (`actionClipboard.tsx:
/// 80-110`): the selection with its bound text and frames' children
/// (`scene.getSelectedElements`), serialized with the scene's `files`
/// (`serializeAsClipboardJSON`, which detaches a copied child from a frame
/// that is not copied, drawing its version stamp from `env`). The caller
/// puts it on the system clipboard (`copyToClipboard`,
/// [`excali_core::clipboard::clipboard_items`]). Nothing selected copies
/// no elements.
pub fn copy_selected<E: EditEnv>(
    elements: &[Element],
    app_state: &AppState,
    files: Option<&Map<String, Value>>,
    env: &mut E,
) -> String {
    let live = non_deleted(elements);
    let selected = object_key(app_state, "selectedElementIds");
    let to_copy: Vec<Element> = get_selected_elements(&live, &selected, true, true)
        .into_iter()
        .cloned()
        .collect();
    serialize_as_clipboard_json(&to_copy, files, env)
}

/// `getGridPoint(x, y, gridSize)` (`packages/common/src/points.ts:69-81`).
fn grid_point(x: f64, y: f64, grid_size: Option<f64>) -> (f64, f64) {
    match grid_size.filter(|g| *g != 0.0 && !g.is_nan()) {
        Some(g) => (js::round(x / g) * g, js::round(y / g) * g),
        None => (x, y),
    }
}

/// Pasting clipboard text at the pointer: what the paste handler does with
/// Excalidraw clipboard JSON (`parseClipboard`, then
/// `addElementsFromPasteOrLibrary` with `position: "cursor"`, new seeds and
/// `preserveFrameChildrenOrder`). The elements are restored
/// (`restoreElements` with `deleteInvisibleElements`), moved so that their
/// common bounds' top left corner lands at `pointer` (scene coordinates)
/// less half their size, snapped to `grid_size` (the effective grid size,
/// `None` when grid mode is off), duplicated with new ids and groups
/// (`duplicateElements` with `type: "everything"`), put on top of the scene
/// with their fractional indices synced, and selected
/// (`getSelectionStateForElements` without frames' children). Sets
/// `selectedLinearElement`, `selectedElementIds`, `selectedGroupIds` and
/// `editingGroupId`; captured (`store.scheduleCapture`).
///
/// `None` for anything that is not Excalidraw clipboard JSON with
/// elements, or when restoring, routing or indexing fails where upstream
/// throws. Not ported, left to the caller: text pasted as a new text
/// element (`addTextFromPaste`), images, embeddable links, Mermaid, the
/// host API's element skeletons (`programmaticAPI`), a frame under the
/// pointer taking the pasted elements (`getTopLayerFrameAtSceneCoords`,
/// `addElementsToFrame`), the host's `onDuplicate` hook, the layout of
/// pasted bound text (`redrawTextBoundingBox` of each pasted label), the
/// pasted `files`, and the selection tool coming back (`setActiveTool`).
pub fn paste_elements<E: EditEnv>(
    clipboard_text: &str,
    elements: &[Element],
    app_state: &AppState,
    pointer: [f64; 2],
    grid_size: Option<f64>,
    env: &mut E,
) -> Option<ActionResult> {
    let ClipboardData::Elements(data) = parse_clipboard(Some(clipboard_text), false) else {
        return None;
    };
    if data.programmatic_api || data.elements.is_empty() {
        return None;
    }
    let options = RestoreElementsOptions {
        delete_invisible_elements: true,
        ..RestoreElementsOptions::default()
    };
    let restored: Vec<Element> = restore_elements(&data.elements, None, options, env)
        .ok()?
        .into_iter()
        .map(Element::from_restored)
        .collect::<Result<_, _>>()
        .ok()?;
    add_elements_at(restored, elements, app_state, pointer, grid_size, true, env)
}

/// The shared half of `addElementsFromPasteOrLibrary` (`App.tsx:
/// 4885-4990`) after `restoreElements`: `duplicateAtSceneCoords` of
/// `App.duplicate.ts` at `pointer` (scene coordinates) with new seeds and
/// `preserve_frame_children_order`, the duplicates on top of the scene with
/// their fractional indices synced, and selected. `None` when nothing is
/// left to add, or routing or indexing fails where upstream throws.
pub(super) fn add_elements_at<E: EditEnv>(
    restored: Vec<Element>,
    elements: &[Element],
    app_state: &AppState,
    pointer: [f64; 2],
    grid_size: Option<f64>,
    preserve_frame_children_order: bool,
    env: &mut E,
) -> Option<ActionResult> {
    if restored.is_empty() {
        return None;
    }
    // duplicateAtSceneCoords
    let [min_x, min_y, max_x, max_y] = get_common_bounds(&restored.iter().collect::<Vec<_>>());
    let center_x = (min_x - max_x).abs() / 2.0;
    let center_y = (min_y - max_y).abs() / 2.0;
    let (grid_x, grid_y) = grid_point(pointer[0] - center_x, pointer[1] - center_y, grid_size);
    let moved: Vec<Element> = restored
        .into_iter()
        .map(|element| {
            // newElementWith(element, { x, y })
            let x = element.base.x + grid_x - min_x;
            let y = element.base.y + grid_y - min_y;
            if x == element.base.x && y == element.base.y {
                return element;
            }
            let mut copy = element;
            copy.base.x = x;
            copy.base.y = y;
            bump(&mut copy, env);
            copy
        })
        .collect();
    let kind = DuplicateType::Everything {
        preserve_frame_children_order,
    };
    let duplication = duplicate_elements(&moved, &kind, true, env).ok()?;

    let mut next: Vec<Element> = elements.to_vec();
    next.extend(duplication.duplicated_elements.iter().cloned());
    let ids: HashSet<String> = duplication
        .duplicated_elements
        .iter()
        .map(|e| e.base.id.clone())
        .collect();
    sync_moved_indices(&mut next, &ids, env).ok()?;

    let pasted: Vec<&Element> = next[elements.len()..].iter().collect();
    let targets = exclude_elements_in_frames_from_selection(&pasted);
    let live = non_deleted(&next);
    let patch = selection_state_for_elements(&targets, &live, app_state);
    Some(ActionResult {
        elements: Some(next),
        app_state: patch,
        capture: true,
        never: false,
    })
}
