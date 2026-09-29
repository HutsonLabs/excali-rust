//! Inserting library items: a click on an item in the library sidebar
//! (`LibraryMenuItems.getInsertedElements`, `LibraryMenu.onInsertLibraryItems`
//! and `App.onInsertElements`) and a drop of items on the canvas
//! (`App.handleAppOnDrop`, `App.tsx:13194-13240`). Each item's elements are
//! duplicated to confine their ids and bindings to the item (upstream's
//! #6465), the items laid out on a square grid
//! (`distributeLibraryItemsOnSquareGrid`, `data/library.ts:405-493`), and
//! the result added as `addElementsFromPasteOrLibrary` adds it
//! (`App.tsx:4885-4990`).

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_core::restore::{restore_elements, RestoreElementsOptions};
use excali_scene::bounds::get_common_bounds;
use serde_json::{json, Value};

use super::clipboard::add_elements_at;
use super::duplicate::{duplicate_elements, DuplicateType};
use super::{ActionResult, EditEnv};
use crate::elbow_arrow::ElbowArrowError;

/// `getCommonBoundingBox(elements)` (`bounds.ts:1128-1144`): min x, min y,
/// width and height.
fn bounding_box(elements: &[Element]) -> (f64, f64, f64, f64) {
    let [min_x, min_y, max_x, max_y] = get_common_bounds(&elements.iter().collect::<Vec<_>>());
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Each item's elements duplicated as `getInsertedElements` and the drop
/// handler do: `duplicateElements({ type: "everything", randomizeSeed:
/// true, preserveFrameChildrenOrder: true })`, item by item.
pub fn duplicate_library_items<E: EditEnv>(
    items: &[Vec<Element>],
    env: &mut E,
) -> Result<Vec<Vec<Element>>, ElbowArrowError> {
    let kind = DuplicateType::Everything {
        preserve_frame_children_order: true,
    };
    items
        .iter()
        .map(|elements| Ok(duplicate_elements(elements, &kind, true, env)?.duplicated_elements))
        .collect()
}

/// `distributeLibraryItemsOnSquareGrid(libraryItems)`: the items' elements
/// laid out `ceil(sqrt(n))` to a row, 50 apart, each item centred in its
/// row's height and its column's width and moved so that its box starts at
/// the cell's corner. The elements are copies with new `x` and `y` only
/// (upstream spreads them, no version bump).
pub fn distribute_library_items_on_square_grid(items: &[Vec<Element>]) -> Vec<Element> {
    const PADDING: f64 = 50.0;
    let per_row = (items.len() as f64).sqrt().ceil() as usize;
    let boxes: Vec<(f64, f64, f64, f64)> = items.iter().map(|e| bounding_box(e)).collect();
    let max_height_per_row = |row: usize| {
        boxes
            .iter()
            .skip(row * per_row)
            .take(per_row)
            .fold(0.0_f64, |acc, b| acc.max(b.3))
    };
    let max_width_per_col = |col: usize| {
        boxes
            .iter()
            .enumerate()
            .filter(|(i, _)| i % per_row == col)
            .fold(0.0_f64, |acc, (_, b)| acc.max(b.2))
    };

    let mut out = Vec::new();
    let mut col_offset_x = 0.0;
    let mut row_offset_y = 0.0;
    let mut max_height_curr_row = 0.0;
    let mut col = 0;
    let mut row = 0;
    for (index, (item, &(min_x, min_y, width, height))) in items.iter().zip(&boxes).enumerate() {
        if index != 0 && index % per_row == 0 {
            row_offset_y += max_height_curr_row + PADDING;
            col_offset_x = 0.0;
            col = 0;
            row += 1;
        }
        if col == 0 {
            max_height_curr_row = max_height_per_row(row);
        }
        let max_width_curr_col = max_width_per_col(col);
        let offset_center_x = (max_width_curr_col - width) / 2.0;
        let offset_center_y = (max_height_curr_row - height) / 2.0;
        for element in item {
            let mut copy = element.clone();
            copy.base.x = element.base.x + col_offset_x + offset_center_x - min_x;
            copy.base.y = element.base.y + row_offset_y + offset_center_y - min_y;
            out.push(copy);
        }
        col_offset_x += max_width_curr_col + PADDING;
        col += 1;
    }
    out
}

/// Inserts library items (each one's elements) at `pointer`, in scene
/// coordinates: the viewport's centre for a click in the library
/// (`onInsertElements`, position `"center"`), the drop point for a drop.
/// The items are duplicated ([`duplicate_library_items`]), distributed
/// ([`distribute_library_items_on_square_grid`]), restored
/// (`restoreElements` with `deleteInvisibleElements`) and added: moved so
/// their common box is centred on `pointer` snapped to `grid_size`,
/// duplicated again with new seeds, put on top of the scene and selected.
/// The sidebar stays open only when it is docked and fits
/// (`sidebar_docked_and_fits`); else `openSidebar` becomes `null`. Captured.
///
/// `None` for no items (or no elements left to add), or when routing or
/// indexing fails where upstream throws (its drop handler shows the error).
/// Not ported, left to the caller as for paste: the layout of bound text,
/// fonts, files, a frame under the pointer, and the selection tool coming
/// back (`setActiveTool`).
pub fn insert_library_items<E: EditEnv>(
    items: &[Vec<Element>],
    elements: &[Element],
    app_state: &AppState,
    pointer: [f64; 2],
    grid_size: Option<f64>,
    sidebar_docked_and_fits: bool,
    env: &mut E,
) -> Option<ActionResult> {
    if items.is_empty() {
        return None;
    }
    let duplicated = duplicate_library_items(items, env).ok()?;
    let distributed: Vec<Value> = distribute_library_items_on_square_grid(&duplicated)
        .iter()
        .map(|e| Value::Object(e.to_map()))
        .collect();
    let options = RestoreElementsOptions {
        delete_invisible_elements: true,
        ..RestoreElementsOptions::default()
    };
    let restored: Vec<Element> = restore_elements(&distributed, None, options, env)
        .ok()?
        .into_iter()
        .map(Element::from_restored)
        .collect::<Result<_, _>>()
        .ok()?;
    let mut result = add_elements_at(
        restored, elements, app_state, pointer, grid_size, false, env,
    )?;
    let open = app_state.get("openSidebar").cloned().unwrap_or(Value::Null);
    result.app_state.insert(
        "openSidebar".into(),
        if !open.is_null() && sidebar_docked_and_fits {
            open
        } else {
            json!(null)
        },
    );
    Some(result)
}
