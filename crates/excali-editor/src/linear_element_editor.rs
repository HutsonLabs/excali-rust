//! The point geometry and point moves of `LinearElementEditor`
//! (`packages/element/src/linearElementEditor.ts`) that binding uses:
//! where a line's or an arrow's point is in scene coordinates, the local
//! point a scene point is, and `movePoints`, which moves some points and
//! keeps the first at `[0, 0]` by moving the element instead.

use excali_core::element::{Element, ElementKind, FixedPointBinding, LocalPoint};
use excali_math::{point_from, point_rotate_rads, GlobalPoint, Radians};
use excali_rough::RoughGenerator;
use excali_scene::bounds::{
    get_curve_path_ops, get_element_absolute_coords, get_min_max_xy_from_curve_path_ops, Bounds,
    ElementsMap,
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
