//! Which transform handle a pointer is on, and the cursor for it
//! (`packages/element/src/resizeTest.ts`).
//!
//! The handles are tested in the order upstream's handle object holds them
//! (rotation first for one element); then, where the editor resizes from
//! the sides ([`can_resize_from_sides`]), the four borders of the selection
//! box, pushed out by `SIDE_RESIZING_THRESHOLD / zoom` (none for images)
//! and turned with the element, within that same distance.

use std::collections::HashSet;

use excali_core::constants::SIDE_RESIZING_THRESHOLD;
use excali_core::element::{Element, ElementKind};
use excali_math::{
    js, point_from, point_on_line_segment_with, point_rotate_rads, GlobalPoint, LineSegment,
    Radians,
};
use excali_scene::bounds::{get_element_absolute_coords, Bounds, ElementsMap};

use crate::tools::PointerType;
use crate::transform_handles::{
    can_resize_from_sides, get_omit_sides_for_editor_interface, get_transform_handles,
    get_transform_handles_from_coords, EditorInterface, TransformHandle, TransformHandleDirection,
    TransformHandleType,
};

/// `isInsideTransformHandle` (`resizeTest.ts:39-47`): inclusive on every
/// edge.
fn is_inside_transform_handle(handle: TransformHandle, x: f64, y: f64) -> bool {
    x >= handle[0] && x <= handle[0] + handle[2] && y >= handle[1] && y <= handle[1] + handle[3]
}

/// `getSelectionBorders([x1, y1], [x2, y2], center, angle)`
/// (`resizeTest.ts:277-294`): the four sides, `n`, `e`, `s`, `w`, turned
/// about `center`.
fn get_selection_borders(
    [x1, y1]: [f64; 2],
    [x2, y2]: [f64; 2],
    center: [f64; 2],
    angle: f64,
) -> [(TransformHandleDirection, LineSegment<excali_math::Global>); 4] {
    let c: GlobalPoint = point_from(center[0], center[1]);
    let rotate = |x: f64, y: f64| point_rotate_rads(point_from(x, y), c, Radians(angle));
    let top_left = rotate(x1, y1);
    let top_right = rotate(x2, y1);
    let bottom_left = rotate(x1, y2);
    let bottom_right = rotate(x2, y2);
    [
        (
            TransformHandleDirection::N,
            LineSegment(top_left, top_right),
        ),
        (
            TransformHandleDirection::E,
            LineSegment(top_right, bottom_right),
        ),
        (
            TransformHandleDirection::S,
            LineSegment(bottom_right, bottom_left),
        ),
        (
            TransformHandleDirection::W,
            LineSegment(bottom_left, top_left),
        ),
    ]
}

/// The first border within `threshold` of `x, y`.
fn side_at(
    borders: &[(TransformHandleDirection, LineSegment<excali_math::Global>); 4],
    x: f64,
    y: f64,
    threshold: f64,
) -> Option<TransformHandleType> {
    borders
        .iter()
        .find(|(_, side)| point_on_line_segment_with(point_from(x, y), *side, threshold))
        .map(|(dir, _)| TransformHandleType::Resize(*dir))
}

/// `resizeTest(element, elementsMap, appState, x, y, zoom, pointerType,
/// editorInterface)` (`resizeTest.ts:49-128`): the handle of `element` at
/// `x, y`, when the element is selected: the rotation handle, then the
/// other handles, then (where sides resize) the selection border, except
/// for a line or arrow of two points.
#[allow(clippy::too_many_arguments)]
pub fn resize_test(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    selected_element_ids: &HashSet<String>,
    x: f64,
    y: f64,
    zoom: f64,
    pointer_type: PointerType,
    editor: &EditorInterface,
) -> Option<TransformHandleType> {
    if !selected_element_ids.contains(&element.base.id) {
        return None;
    }

    let handles = get_transform_handles(
        element,
        zoom,
        elements_map,
        pointer_type,
        &get_omit_sides_for_editor_interface(editor),
    );

    if let Some(rotation) = handles.rotation {
        if is_inside_transform_handle(rotation, x, y) {
            return Some(TransformHandleType::Rotation);
        }
    }

    if let Some((kind, _)) = handles
        .iter()
        .filter(|(kind, _)| *kind != TransformHandleType::Rotation)
        .find(|(_, handle)| is_inside_transform_handle(*handle, x, y))
    {
        return Some(kind);
    }

    if can_resize_from_sides(editor) {
        let [x1, y1, x2, y2, cx, cy] = get_element_absolute_coords(element, elements_map, false);
        let is_linear = matches!(element.kind, ElementKind::Line(_) | ElementKind::Arrow(_));
        // do not resize from the sides for linear elements with only two points
        if !(is_linear && element.kind.points().map_or(0, <[_]>::len) <= 2) {
            let spacing = if matches!(element.kind, ElementKind::Image(_)) {
                0.0
            } else {
                SIDE_RESIZING_THRESHOLD / zoom
            };
            let zoomed_side_resizing_threshold = SIDE_RESIZING_THRESHOLD / zoom;
            let borders = get_selection_borders(
                [x1 - spacing, y1 - spacing],
                [x2 + spacing, y2 + spacing],
                [cx, cy],
                element.base.angle.0,
            );
            return side_at(&borders, x, y, zoomed_side_resizing_threshold);
        }
    }
    None
}

/// `getElementWithTransformHandleType(elements, appState, x, y, zoom,
/// pointerType, elementsMap, editorInterface)` (`resizeTest.ts:130-156`):
/// the first element, in order, whose [`resize_test`] finds a handle.
#[allow(clippy::too_many_arguments)]
pub fn get_element_with_transform_handle_type<'a>(
    elements: &[&'a Element],
    selected_element_ids: &HashSet<String>,
    scene_pointer_x: f64,
    scene_pointer_y: f64,
    zoom: f64,
    pointer_type: PointerType,
    elements_map: &ElementsMap<'_>,
    editor: &EditorInterface,
) -> Option<(&'a Element, TransformHandleType)> {
    elements.iter().find_map(|element| {
        resize_test(
            element,
            elements_map,
            selected_element_ids,
            scene_pointer_x,
            scene_pointer_y,
            zoom,
            pointer_type,
            editor,
        )
        .map(|handle| (*element, handle))
    })
}

/// `getTransformHandleTypeFromCoords([x1, y1, x2, y2], x, y, zoom,
/// pointerType, editorInterface)` (`resizeTest.ts:158-217`): the handle of
/// an unrotated selection box (several elements) at `x, y`: the handles
/// in order, rotation included, then the border.
pub fn get_transform_handle_type_from_coords(
    [x1, y1, x2, y2]: Bounds,
    scene_pointer_x: f64,
    scene_pointer_y: f64,
    zoom: f64,
    pointer_type: PointerType,
    editor: &EditorInterface,
) -> Option<TransformHandleType> {
    let handles = get_transform_handles_from_coords(
        [x1, y1, x2, y2, (x1 + x2) / 2.0, (y1 + y2) / 2.0],
        0.0,
        zoom,
        pointer_type,
        &get_omit_sides_for_editor_interface(editor),
        None,
        None,
    );

    if let Some((kind, _)) = handles
        .iter()
        .find(|(_, handle)| is_inside_transform_handle(*handle, scene_pointer_x, scene_pointer_y))
    {
        return Some(kind);
    }

    if can_resize_from_sides(editor) {
        let cx = (x1 + x2) / 2.0;
        let cy = (y1 + y2) / 2.0;
        let spacing = SIDE_RESIZING_THRESHOLD / zoom;
        let borders = get_selection_borders(
            [x1 - spacing, y1 - spacing],
            [x2 + spacing, y2 + spacing],
            [cx, cy],
            0.0,
        );
        return side_at(&borders, scene_pointer_x, scene_pointer_y, spacing);
    }
    None
}

/// `RESIZE_CURSORS` (`resizeTest.ts:219`).
const RESIZE_CURSORS: [&str; 4] = ["ns", "nesw", "ew", "nwse"];

/// `rotateResizeCursor(cursor, angle)` (`resizeTest.ts:220-227`): the
/// cursor turned by the nearest eighth of a turn. A turn that indexes
/// outside the list (a negative or non-finite angle) is no cursor.
fn rotate_resize_cursor(cursor: &'static str, angle: f64) -> Option<&'static str> {
    let Some(index) = RESIZE_CURSORS.iter().position(|c| *c == cursor) else {
        return Some(cursor);
    };
    let a = js::round(angle / (std::f64::consts::PI / 4.0));
    let i = (index as f64 + a) % RESIZE_CURSORS.len() as f64;
    if i >= 0.0 && i.fract() == 0.0 {
        RESIZE_CURSORS.get(i as usize).copied()
    } else {
        None
    }
}

/// `Math.sign(x)`.
fn sign(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// `getCursorForResizingElement({ element, transformHandleType })`
/// (`resizeTest.ts:232-275`): the CSS cursor for a handle, `"grab"` for
/// rotation; the diagonal cursors swap for an element whose width and
/// height differ in sign, and all turn with the element. `""` for no
/// handle.
pub fn get_cursor_for_resizing_element(
    element: Option<&Element>,
    transform_handle_type: Option<TransformHandleType>,
) -> String {
    use TransformHandleDirection as D;
    let should_swap_cursors =
        element.is_some_and(|e| sign(e.base.height) * sign(e.base.width) == -1.0);
    let cursor = match transform_handle_type {
        Some(TransformHandleType::Resize(D::N | D::S)) => Some("ns"),
        Some(TransformHandleType::Resize(D::W | D::E)) => Some("ew"),
        Some(TransformHandleType::Resize(D::Nw | D::Se)) => {
            Some(if should_swap_cursors { "nesw" } else { "nwse" })
        }
        Some(TransformHandleType::Resize(D::Ne | D::Sw)) => {
            Some(if should_swap_cursors { "nwse" } else { "nesw" })
        }
        Some(TransformHandleType::Rotation) => return "grab".to_string(),
        None => None,
    };
    let cursor = match (cursor, element) {
        (Some(cursor), Some(element)) => rotate_resize_cursor(cursor, element.base.angle.0),
        (cursor, _) => cursor,
    };
    cursor.map_or_else(String::new, |c| format!("{c}-resize"))
}
