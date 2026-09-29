//! `LinearElementEditor` (`packages/element/src/linearElementEditor.ts`):
//! editing a line's or an arrow's points.
//!
//! - Point geometry: where a point is in scene coordinates, the local point
//!   a scene point is, and `movePoints`, which moves some points and keeps
//!   the first at `[0, 0]` by moving the element instead.
//! - Point handles ([`POINT_HANDLE_SIZE`] 10 screen px,
//!   [`get_point_index_under_cursor`], [`is_point_handle`]).
//! - Midpoint handles: one per segment ([`get_editor_mid_points`]) unless
//!   the segment is shorter on screen than four handles (half a handle on
//!   an elbow arrow, [`is_segment_too_short`]); hit testing them
//!   ([`get_segment_midpoint_hit_coords`]) and inserting a point there
//!   ([`should_add_midpoint`], [`add_midpoint`]).
//! - Point edits ([`delete_points`], [`add_points`]) and moving an arrow's
//!   label along its path ([`handle_bound_text_dragging`]); where the label
//!   sits is `excali_scene::linear_element::get_bound_text_element_position`,
//!   which [`crate::text_layout::SceneArrowGeometry`] gives excali-text.

use excali_core::constants::DRAGGING_THRESHOLD;
use excali_core::element::{Element, ElementKind, FixedPointBinding, LocalPoint};
use excali_math::{
    bezier_equation, curve_closest_parameter, curve_length, curve_length_at_parameter, js,
    line_segment_closest_parameter, line_segment_point_at, point_distance, point_from,
    point_rotate_rads, points_equal, GlobalPoint, Radians,
};
use excali_rough::RoughGenerator;
use excali_scene::bounds::{
    get_bound_text_element, get_curve_path_ops, get_element_absolute_coords,
    get_min_max_xy_from_curve_path_ops, Bounds, ElementsMap,
};
use excali_scene::linear_element::{
    deconstruct_linear_or_freedraw_element, get_linear_element_path_segments,
    get_point_at_path_parameter, get_segment_mid_point, LinearPathSegment,
};
use excali_scene::rough_options::generate_rough_options;

use crate::scene::{ElementUpdate, MutationEnv, Scene};
use crate::transform::get_grid_point;

pub use excali_scene::linear_element::get_point_global_coordinates;

fn rotate(p: [f64; 2], center: [f64; 2], angle: f64) -> [f64; 2] {
    let p: GlobalPoint = point_rotate_rads(
        point_from(p[0], p[1]),
        point_from(center[0], center[1]),
        Radians(angle),
    );
    [p.x, p.y]
}

fn points(element: &Element) -> &[LocalPoint] {
    element.kind.points().unwrap_or(&[])
}

fn is_elbow_arrow(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Arrow(a) if a.elbowed)
}

/// `LinearElementEditor.getPointAtIndexGlobalCoordinates(element,
/// indexMaybeFromEnd, elementsMap)` (`linearElementEditor.ts:1373-1394`):
/// point `index` (counting from the end when negative) in scene
/// coordinates, rotated about the centre of the element's box; the
/// element's position when there is no such point.
pub fn get_point_at_index_global_coordinates(
    element: &Element,
    index_maybe_from_end: isize,
    elements_map: &ElementsMap<'_>,
) -> [f64; 2] {
    let pts = points(element);
    let index = if index_maybe_from_end < 0 {
        pts.len() as isize + index_maybe_from_end
    } else {
        index_maybe_from_end
    };
    let [_, _, _, _, cx, cy] = get_element_absolute_coords(element, elements_map, false);
    let b = &element.base;
    let p = usize::try_from(index).ok().and_then(|i| pts.get(i));
    match p {
        Some(p) => rotate([b.x + p[0], b.y + p[1]], [cx, cy], b.angle.0),
        None => rotate([b.x, b.y], [cx, cy], b.angle.0),
    }
}

/// `LinearElementEditor.pointFromAbsoluteCoords(element, absoluteCoords,
/// elementsMap)` (`linearElementEditor.ts:1396-1418`): a scene point as a
/// point of the element, unrotated about its box's centre (an elbow arrow
/// is never rotated).
pub fn point_from_absolute_coords(
    element: &Element,
    absolute_coords: [f64; 2],
    elements_map: &ElementsMap<'_>,
) -> LocalPoint {
    let b = &element.base;
    if is_elbow_arrow(element) {
        return [absolute_coords[0] - b.x, absolute_coords[1] - b.y];
    }
    let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, elements_map, false);
    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;
    let [x, y] = rotate(absolute_coords, [cx, cy], -b.angle.0);
    [x - b.x, y - b.y]
}

/// `LinearElementEditor.createPointAt(element, elementsMap, scenePointerX,
/// scenePointerY, gridSize)` (`linearElementEditor.ts:1461-1479`): the
/// pointer (on the grid when there is one) as a point of the element.
pub fn create_point_at(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    scene_pointer_x: f64,
    scene_pointer_y: f64,
    grid_size: Option<f64>,
) -> LocalPoint {
    let pointer_on_grid = get_grid_point(scene_pointer_x, scene_pointer_y, grid_size);
    let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, elements_map, false);
    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;
    let b = &element.base;
    let [rotated_x, rotated_y] = rotate(pointer_on_grid, [cx, cy], -b.angle.0);
    [rotated_x - b.x, rotated_y - b.y]
}

/// `getElementPointsCoords(element, points)` (`bounds.ts:1094-1115`): the
/// box of the rough.js curve through `points` (a linear path for sharp
/// corners), at the element's position.
pub fn get_element_points_coords(element: &Element, points: &[LocalPoint]) -> Bounds {
    let generator = RoughGenerator::new();
    let ops = generate_rough_options(element, false, false)
        .ok()
        .and_then(|options| {
            let options = options.to_rough(generator.default_options());
            if element.base.roundness.is_none() {
                Some(generator.linear_path(points, &options))
            } else {
                generator.curve(points, &options).ok()
            }
        })
        .map(|shape| get_curve_path_ops(&shape).to_vec())
        .unwrap_or_default();
    let [min_x, min_y, max_x, max_y] = get_min_max_xy_from_curve_path_ops(&ops, None);
    let b = &element.base;
    [min_x + b.x, min_y + b.y, max_x + b.x, max_y + b.y]
}

/// One entry of `PointsPositionUpdates` (`types.ts`): where a point goes
/// and whether it is being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointUpdate {
    pub point: LocalPoint,
    pub is_dragging: bool,
}

impl PointUpdate {
    /// A point moved, not dragged (`{ point }`).
    pub fn to(point: LocalPoint) -> PointUpdate {
        PointUpdate {
            point,
            is_dragging: false,
        }
    }
}

/// `movePoints`' `otherUpdates`: bindings to set with the points (`None`
/// leaves the key out, `Some(None)` sets `null`) and
/// `moveMidPointsWithElement`, which the non-elbow path writes onto the
/// element with the points (`...otherUpdates`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MovePointsOtherUpdates {
    pub start_binding: Option<Option<FixedPointBinding>>,
    pub end_binding: Option<Option<FixedPointBinding>>,
    pub move_mid_points_with_element: Option<bool>,
}

/// The updates in `Map` order: the last update of an index replaces an
/// earlier one in place, as `Map.set` does.
fn lookup(updates: &[(usize, PointUpdate)], index: usize) -> Option<&PointUpdate> {
    updates
        .iter()
        .rev()
        .find(|(i, _)| *i == index)
        .map(|(_, u)| u)
}

/// `LinearElementEditor.movePoints(element, scene, pointUpdates,
/// otherUpdates)` (`linearElementEditor.ts:1649-1719`): moves the points
/// in `point_updates` (in insertion order; a polygon's first and last
/// points move together). The first point stays at `[0, 0]`: when it
/// moves, every other point moves the opposite way and the element takes
/// up the difference (`_updatePoints`). An elbow arrow keeps only its
/// first and last points, which it re-routes between.
pub fn move_points(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    element_id: &str,
    point_updates: &[(usize, PointUpdate)],
    other_updates: MovePointsOtherUpdates,
) {
    let Some(element) = scene.get(element_id).cloned() else {
        return;
    };
    let pts = points(&element).to_vec();
    if pts.is_empty() {
        return;
    }
    let last = pts.len() - 1;
    let mut updates: Vec<(usize, PointUpdate)> = point_updates.to_vec();

    // if polygon, move start and end points together
    if matches!(&element.kind, ElementKind::Line(l) if l.polygon) {
        if let Some(first) = lookup(&updates, 0).copied() {
            set(&mut updates, last, first);
        } else if let Some(last_update) = lookup(&updates, last).copied() {
            set(&mut updates, 0, last_update);
        }
    }

    let [offset_x, offset_y] = lookup(&updates, 0).map_or([0.0, 0.0], |u| u.point);

    let next_points: Vec<LocalPoint> = if is_elbow_arrow(&element) {
        vec![
            lookup(&updates, 0).map_or(pts[0], |u| u.point),
            lookup(&updates, last).map_or(pts[last], |u| u.point),
        ]
    } else {
        pts.iter()
            .enumerate()
            .map(|(idx, p)| {
                let current = lookup(&updates, idx).map_or(*p, |u| u.point);
                if other_updates.move_mid_points_with_element == Some(true)
                    && idx != 0
                    && idx != last
                    && lookup(&updates, idx).is_none()
                {
                    return current;
                }
                [current[0] - offset_x, current[1] - offset_y]
            })
            .collect()
    };

    update_points(
        scene,
        env,
        &element,
        next_points,
        offset_x,
        offset_y,
        other_updates,
    );
}

/// `Map.set(index, update)`: replaces the entry in place, or appends.
fn set(updates: &mut Vec<(usize, PointUpdate)>, index: usize, update: PointUpdate) {
    match updates.iter_mut().find(|(i, _)| *i == index) {
        Some(entry) => entry.1 = update,
        None => updates.push((index, update)),
    }
}

/// `LinearElementEditor._updatePoints` (`linearElementEditor.ts:1832-1890`).
fn update_points(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    element: &Element,
    next_points: Vec<LocalPoint>,
    offset_x: f64,
    offset_y: f64,
    other_updates: MovePointsOtherUpdates,
) {
    let id = element.base.id.clone();
    if is_elbow_arrow(element) {
        scene.mutate_element(
            &id,
            ElementUpdate {
                start_binding: other_updates.start_binding,
                end_binding: other_updates.end_binding,
                points: Some(next_points),
                ..ElementUpdate::default()
            },
            env,
        );
        return;
    }
    let next_coords = get_element_points_coords(element, &next_points);
    let prev_coords = get_element_points_coords(element, points(element));
    let next_center_x = (next_coords[0] + next_coords[2]) / 2.0;
    let next_center_y = (next_coords[1] + next_coords[3]) / 2.0;
    let prev_center_x = (prev_coords[0] + prev_coords[2]) / 2.0;
    let prev_center_y = (prev_coords[1] + prev_coords[3]) / 2.0;
    let d_x = prev_center_x - next_center_x;
    let d_y = prev_center_y - next_center_y;
    let rotated_offset = rotate([offset_x, offset_y], [d_x, d_y], element.base.angle.0);
    scene.mutate_element(
        &id,
        ElementUpdate {
            start_binding: other_updates.start_binding,
            end_binding: other_updates.end_binding,
            move_mid_points_with_element: other_updates.move_mid_points_with_element,
            points: Some(next_points),
            x: Some(element.base.x + rotated_offset[0]),
            y: Some(element.base.y + rotated_offset[1]),
            ..ElementUpdate::default()
        },
        env,
    );
}

// -- point handles and midpoints ----------------------------------------------------------

/// `LinearElementEditor.POINT_HANDLE_SIZE` (`linearElementEditor.ts:231`):
/// the diameter of a point handle, in screen pixels.
pub const POINT_HANDLE_SIZE: f64 = 10.0;

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    let a: GlobalPoint = point_from(a[0], a[1]);
    point_distance(a, point_from(b[0], b[1]))
}

/// `LinearElementEditor.getPointsGlobalCoordinates(element, elementsMap)`
/// (`linearElementEditor.ts:1356-1371`): every point in scene coordinates,
/// rotated about the centre of the element's box.
pub fn get_points_global_coordinates(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> Vec<[f64; 2]> {
    let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, elements_map, false);
    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;
    let b = &element.base;
    points(element)
        .iter()
        .map(|p| rotate([b.x + p[0], b.y + p[1]], [cx, cy], b.angle.0))
        .collect()
}

/// `LinearElementEditor.isPointHandle(element, index)`
/// (`linearElementEditor.ts:1424-1431`): whether point `index` has a
/// handle; an elbow arrow has one at each end only.
pub fn is_point_handle(element: &Element, index: isize) -> bool {
    index >= 0
        && (!is_elbow_arrow(element) || index == 0 || index == points(element).len() as isize - 1)
}

/// `LinearElementEditor.getPointIndexUnderCursor(element, elementsMap,
/// zoom, x, y)` (`linearElementEditor.ts:1433-1459`): the last point whose
/// handle (plus a pixel of outline) is under the pointer, or -1. Later
/// points are drawn over earlier ones, so they win.
pub fn get_point_index_under_cursor(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    zoom: f64,
    x: f64,
    y: f64,
) -> isize {
    let handles = get_points_global_coordinates(element, elements_map);
    handles
        .iter()
        .rposition(|&p| distance([x, y], p) * zoom < POINT_HANDLE_SIZE + 1.0)
        .map_or(-1, |i| i as isize)
}

/// `LinearElementEditor.isSegmentTooShort(element, startPoint, endPoint,
/// index, zoom, elementsMap)` (`linearElementEditor.ts:935-974`): whether a
/// segment is too short on screen for a midpoint handle. An elbow arrow's
/// segment is when it is under half a handle; any other when it is under
/// four handles, measured along the curve for a round line of more than two
/// points.
pub fn is_segment_too_short(
    element: &Element,
    start_point: [f64; 2],
    end_point: [f64; 2],
    index: usize,
    zoom: f64,
    elements_map: &ElementsMap<'_>,
) -> bool {
    let pts = points(element);
    if is_elbow_arrow(element) {
        return index < pts.len()
            && distance(start_point, end_point) * zoom < POINT_HANDLE_SIZE / 2.0;
    }
    let mut length = distance(start_point, end_point);
    if pts.len() > 2 && element.base.roundness.is_some() {
        let (_, curves) = deconstruct_linear_or_freedraw_element(element, elements_map);
        // upstream asserts the curve exists
        length = curves.get(index).map_or(f64::NAN, |&c| curve_length(c));
    }
    length * zoom < POINT_HANDLE_SIZE * 4.0
}

/// `LinearElementEditor.getEditorMidPoints(element, elementsMap, appState)`
/// (`linearElementEditor.ts:811-860`): each segment's midpoint, `None` for
/// a segment too short for one ([`is_segment_too_short`]). Outside the
/// editor (`is_editing` false) a line of more than two points without a
/// label has none; an elbow arrow always has them.
pub fn get_editor_mid_points(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    is_editing: bool,
    zoom: f64,
) -> Vec<Option<[f64; 2]>> {
    let pts = points(element);
    let bound_text = get_bound_text_element(element, elements_map);
    if !is_elbow_arrow(element) && !is_editing && pts.len() > 2 && bound_text.is_none() {
        return Vec::new();
    }
    (0..pts.len().saturating_sub(1))
        .map(|index| {
            if is_segment_too_short(
                element,
                pts[index],
                pts[index + 1],
                index,
                zoom,
                elements_map,
            ) {
                None
            } else {
                Some(get_segment_mid_point(element, index + 1, elements_map))
            }
        })
        .collect()
}

/// `LinearElementEditor.getSegmentMidpointHitCoords(linearElementEditor,
/// scenePointer, appState, elementsMap)` (`linearElementEditor.ts:862-933`):
/// the midpoint handle under the pointer, within a handle and a pixel.
/// `hovered` is the editor's `segmentMidPointHoveredCoords`, which wins
/// while the pointer stays near it. A point handle under the pointer takes
/// precedence (except on an elbow arrow), and outside the editor a line of
/// three or more points offers none.
pub fn get_segment_midpoint_hit_coords(
    element: &Element,
    hovered: Option<[f64; 2]>,
    scene_pointer: [f64; 2],
    zoom: f64,
    is_editing: bool,
    elements_map: &ElementsMap<'_>,
) -> Option<[f64; 2]> {
    let [x, y] = scene_pointer;
    let elbow = is_elbow_arrow(element);
    if !elbow && get_point_index_under_cursor(element, elements_map, zoom, x, y) >= 0 {
        return None;
    }
    if points(element).len() >= 3 && !is_editing && !elbow {
        return None;
    }
    let threshold = (POINT_HANDLE_SIZE + 1.0) / zoom;
    if let Some(h) = hovered {
        if distance(h, scene_pointer) <= threshold {
            return Some(h);
        }
    }
    get_editor_mid_points(element, elements_map, is_editing, zoom)
        .into_iter()
        .flatten()
        .find(|&m| distance(m, scene_pointer) <= threshold)
}

/// `LinearElementEditor.getSegmentMidPointIndex(linearElementEditor,
/// appState, midPoint, elementsMap)` (`linearElementEditor.ts:1020-1046`):
/// the segment (counting from 1) whose midpoint `mid_point` is, or -1.
pub fn get_segment_mid_point_index(
    element: &Element,
    is_editing: bool,
    zoom: f64,
    mid_point: [f64; 2],
    elements_map: &ElementsMap<'_>,
) -> isize {
    let p: GlobalPoint = point_from(mid_point[0], mid_point[1]);
    get_editor_mid_points(element, elements_map, is_editing, zoom)
        .iter()
        .position(|m| m.is_some_and(|m| points_equal(p, point_from(m[0], m[1]))))
        .map_or(-1, |i| i as isize + 1)
}

/// `initialState.segmentMidpoint`: the midpoint handle the pointer went
/// down on, its segment (counting from 1), and whether a point was already
/// added there.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SegmentMidpoint {
    pub value: Option<[f64; 2]>,
    pub index: Option<usize>,
    pub added: bool,
}

/// `LinearElementEditor.shouldAddMidpoint(linearElementEditor,
/// pointerCoords, appState, elementsMap)` (`linearElementEditor.ts:1736-1779`):
/// whether dragging from a midpoint handle (pressed at `origin`) should
/// insert a point there. Never on an elbow arrow or twice in one drag;
/// outside the editor only once the pointer has moved
/// [`DRAGGING_THRESHOLD`] screen pixels.
pub fn should_add_midpoint(
    element: &Element,
    segment_midpoint: &SegmentMidpoint,
    origin: Option<[f64; 2]>,
    pointer: [f64; 2],
    is_editing: bool,
    zoom: f64,
) -> bool {
    if is_elbow_arrow(element) {
        return false;
    }
    let Some(origin) = origin else {
        return false;
    };
    if segment_midpoint.added
        || segment_midpoint.value.is_none()
        || segment_midpoint.index.is_none()
    {
        return false;
    }
    is_editing || distance(origin, pointer) >= DRAGGING_THRESHOLD / zoom
}

/// What [`add_midpoint`] hands back to the editor: `lastClickedPoint` and
/// `selectedPointsIndices` (the new point).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedMidpoint {
    pub last_clicked_point: usize,
    pub selected_points_indices: Vec<usize>,
}

/// `LinearElementEditor.addMidpoint(linearElementEditor, pointerCoords,
/// app, snapToGrid, scene)` (`linearElementEditor.ts:1781-1830`): inserts
/// the pointer ([`create_point_at`], on the grid of `grid_size` unless the
/// arrow is elbowed) as point `index`, the segment the midpoint handle
/// belonged to. `None` when the element is not in the scene.
pub fn add_midpoint(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    element_id: &str,
    index: usize,
    pointer: [f64; 2],
    grid_size: Option<f64>,
) -> Option<AddedMidpoint> {
    let element = scene.get_non_deleted(element_id)?.clone();
    let midpoint = {
        let map = scene.elements_map();
        let grid = if is_elbow_arrow(&element) {
            None
        } else {
            grid_size
        };
        create_point_at(&element, &map, pointer[0], pointer[1], grid)
    };
    let pts = points(&element);
    let at = index.min(pts.len());
    let mut next = pts[..at].to_vec();
    next.push(midpoint);
    next.extend_from_slice(&pts[at..]);
    scene.mutate_element(
        element_id,
        ElementUpdate {
            points: Some(next),
            ..ElementUpdate::default()
        },
        env,
    );
    Some(AddedMidpoint {
        last_clicked_point: index,
        selected_points_indices: vec![index],
    })
}

/// `getNormalizedPoints({ points })` (`linearElementEditor.ts:106-125`):
/// the points moved so the first is `[0, 0]`, and the offset.
fn get_normalized_points(pts: &[LocalPoint]) -> (Vec<LocalPoint>, f64, f64) {
    let [offset_x, offset_y] = pts.first().copied().unwrap_or([f64::NAN, f64::NAN]);
    let normalized = pts
        .iter()
        .map(|p| [p[0] - offset_x, p[1] - offset_y])
        .collect();
    (normalized, offset_x, offset_y)
}

fn is_polygon(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Line(l) if l.polygon)
}

/// Normalizes `next` and writes it with `_updatePoints`.
fn set_points(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    element: &Element,
    mut next: Vec<LocalPoint>,
    close_polygon: bool,
) {
    if next.is_empty() {
        return;
    }
    if close_polygon {
        next[0] = next[next.len() - 1];
    }
    let (normalized, offset_x, offset_y) = get_normalized_points(&next);
    update_points(
        scene,
        env,
        element,
        normalized,
        offset_x,
        offset_y,
        MovePointsOtherUpdates::default(),
    );
}

/// `LinearElementEditor.deletePoints(element, app, pointIndices)`
/// (`linearElementEditor.ts:1576-1618`): removes the points at `indices`
/// and keeps the first at `[0, 0]`. A polygon stays closed when its first
/// or last point goes, or the uncommitted point does
/// (`is_uncommitted_point`: the editor is editing and its
/// `lastUncommittedPoint` is the element's last point).
pub fn delete_points(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    element_id: &str,
    indices: &[usize],
    is_uncommitted_point: bool,
) {
    let Some(element) = scene.get(element_id).cloned() else {
        return;
    };
    let pts = points(&element);
    let next: Vec<LocalPoint> = pts
        .iter()
        .enumerate()
        .filter(|(i, _)| !indices.contains(i))
        .map(|(_, p)| *p)
        .collect();
    let close = is_polygon(&element)
        && (is_uncommitted_point
            || indices.contains(&0)
            || indices.contains(&pts.len().wrapping_sub(1)));
    set_points(scene, env, &element, next, close);
}

/// `LinearElementEditor.addPoints(element, scene, addedPoints)`
/// (`linearElementEditor.ts:1620-1647`): appends points (a polygon's first
/// point follows its new last one) and keeps the first at `[0, 0]`.
pub fn add_points(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    element_id: &str,
    added_points: &[LocalPoint],
) {
    let Some(element) = scene.get(element_id).cloned() else {
        return;
    };
    let mut next = points(&element).to_vec();
    next.extend_from_slice(added_points);
    let close = is_polygon(&element);
    set_points(scene, env, &element, next, close);
}

// -- arrow labels ------------------------------------------------------------------------

/// `pathSegmentLength` (`linearElementEditor.ts:2818-2821`).
fn path_segment_length(segment: &LinearPathSegment) -> f64 {
    match *segment {
        LinearPathSegment::Line(l) => point_distance(l.0, l.1),
        LinearPathSegment::Curve(c) => curve_length(c),
    }
}

/// `pathSegmentClosestParameter(segment, point)`
/// (`linearElementEditor.ts:2827-2839`): the parameter of the segment's
/// point closest to `point`, and its distance.
fn path_segment_closest_parameter(segment: &LinearPathSegment, point: GlobalPoint) -> (f64, f64) {
    match *segment {
        LinearPathSegment::Curve(c) => {
            let t = curve_closest_parameter(c, point);
            (t, point_distance(point, bezier_equation(c, t)))
        }
        LinearPathSegment::Line(l) => {
            let t = line_segment_closest_parameter(point, l);
            (t, point_distance(point, line_segment_point_at(l, t)))
        }
    }
}

/// `pathSegmentLengthAtParameter(segment, t, segmentLength)`
/// (`linearElementEditor.ts:2841-2846`).
fn path_segment_length_at_parameter(segment: &LinearPathSegment, t: f64, length: f64) -> f64 {
    match *segment {
        LinearPathSegment::Curve(c) => curve_length_at_parameter(c, t),
        LinearPathSegment::Line(_) => t * length,
    }
}

/// `LinearElementEditor.handleBoundTextDragging(linearElementEditor, scene,
/// pointerX, pointerY)` (`linearElementEditor.ts:1963-2035`): moves an
/// arrow's label to the point of the arrow's path closest to the pointer
/// (less the editor's `pointerOffset`), storing that point's fraction of
/// the path length as the label's `labelPosition`. Returns whether the
/// label moved (upstream answers the editor with `isDragging` set, or
/// `null`).
pub fn handle_bound_text_dragging(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    element_id: &str,
    pointer: [f64; 2],
    pointer_offset: [f64; 2],
) -> bool {
    let (text_id, update) = {
        let map = scene.elements_map();
        let Some(element) = map.get(element_id) else {
            return false;
        };
        if !matches!(element.kind, ElementKind::Arrow(_)) {
            return false;
        }
        let Some(text) = get_bound_text_element(element, &map) else {
            return false;
        };
        let pointer_global: GlobalPoint = point_from(
            pointer[0] - pointer_offset[0],
            pointer[1] - pointer_offset[1],
        );
        let segments = get_linear_element_path_segments(element, &map);
        let lengths: Vec<f64> = segments.iter().map(path_segment_length).collect();
        let mut prefix_sums = vec![0.0; lengths.len() + 1];
        for (i, length) in lengths.iter().enumerate() {
            prefix_sums[i + 1] = prefix_sums[i] + length;
        }
        let total_length = prefix_sums[lengths.len()];
        if segments.is_empty() || total_length == 0.0 {
            return false;
        }

        let mut best_distance = f64::INFINITY;
        let mut best_segment_index = 0;
        let mut best_parameter = 0.0;
        for (i, segment) in segments.iter().enumerate() {
            let (t, d) = path_segment_closest_parameter(segment, pointer_global);
            if d < best_distance {
                best_distance = d;
                best_segment_index = i;
                best_parameter = t;
            }
        }
        let length_within_segment = path_segment_length_at_parameter(
            &segments[best_segment_index],
            best_parameter,
            lengths[best_segment_index],
        );
        let label_position = js::min(
            js::max(
                (prefix_sums[best_segment_index] + length_within_segment) / total_length,
                0.0,
            ),
            1.0,
        );
        let Some(path_point) = get_point_at_path_parameter(element, label_position, &map) else {
            return false;
        };
        (
            text.base.id.clone(),
            ElementUpdate {
                label_position: Some(label_position),
                x: Some(path_point[0] - text.base.width / 2.0),
                y: Some(path_point[1] - text.base.height / 2.0),
                ..ElementUpdate::default()
            },
        )
    };
    scene.mutate_element(&text_id, update, env);
    true
}
