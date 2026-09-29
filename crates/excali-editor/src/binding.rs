//! Arrow binding: a port of `packages/element/src/binding.ts` and the
//! binding helpers of `utils.ts` (`getAllMidpoints`,
//! `getElbowArrowSnapMidPoint`, `getSnapOutlineMidPoint`,
//! `projectFixedPointOntoDiagonal`). See `site/content/research/
//! rendering.md` section 5 and `data-model.md` section 7.
//!
//! - An arrow end is bound to a bindable element by a
//!   [`FixedPointBinding`]: the target's id, a fixed point as ratios of its
//!   width and height ([`normalize_fixed_point`]: clamped to `[-10, 10]`,
//!   and 0.5001 in place of 0.5 so the heading never flips on rounding),
//!   and a mode: `inside` (the end sits on the fixed point, inside the
//!   shape), `orbit` (it stays on the outline, a gap of
//!   `5 + strokeWidth / 2` out, [`get_binding_gap`]) or `skip`. The target
//!   lists the arrow in `boundElements`.
//! - An end binds when it comes within the binding distance of a bindable
//!   element: 15 at zoom 1 and above, up to 30 zoomed out
//!   ([`max_binding_distance_simple`]); the element found is the one the
//!   editor highlights (`suggestedBinding`, [`get_hovered_element_for_binding`]).
//!   Which ends bind, in which mode and on which point is the binding
//!   strategy for the dragged ends
//!   ([`get_binding_strategy_for_dragging_binding_element_endpoints`]), in
//!   upstream's default (simple) flavour or, with the `COMPLEX_BINDINGS`
//!   feature flag, the complex one ([`BindingAppState::complex_bindings`]).
//! - When a bound element moves, resizes or rotates, the arrows bound to it
//!   follow ([`update_bound_elements`], [`update_bound_point`]): an inside
//!   end goes to its fixed point, an orbiting end to where the line from its
//!   fixed point to the other end crosses the outline grown by the gap,
//!   except where that would invert the arrow. Elbow arrows are re-routed.
//!
//! Where upstream throws an invariant (`invariant(...)`), the port answers
//! a [`BindingError`] with upstream's message. Where upstream would throw a
//! `TypeError` reading an element that is not in the map (a binding to a
//! missing element), the port treats the binding as absent.
//!
//! [`get_hovered_element_for_binding`]: crate::collision::get_hovered_element_for_binding

use std::collections::{HashMap, HashSet};
use std::fmt;

use excali_core::color::is_transparent;
use excali_core::element::{
    BindMode, BoundElement, BoundElementType, Element, ElementKind, FixedPointBinding, LocalPoint,
};
use excali_core::fractional_index::SceneElementsMap;
use excali_math::{
    bezier_equation, js, line_segment, line_segment_intersection_points, point_from,
    point_rotate_rads, points_equal, GlobalPoint, Radians, PRECISION,
};
use excali_scene::bounds::{element_center_point, get_bound_text_element, ElementsMap};
use excali_scene::heading::{heading_is_horizontal, vector_to_heading};
use excali_scene::utils::{
    deconstruct_diamond_element, deconstruct_rectanguloid_element, get_diamond_base_corners,
};
use excali_text::text_measurements::{CharWidthCache, TextMetricsProvider};

use crate::collision::{
    get_all_hovered_element_at_point, get_hovered_element_for_binding, hit_element_itself,
    intersect_element_with_line_segment, is_bindable_element_inside_other_bindable,
    is_point_in_element, HitTestArgs, HitTestCache,
};
use crate::elbow_arrow::{self, ElbowArrowError, ElbowArrowUpdates};
use crate::geometry::{aabb_for_element, get_center_for_bounds, heading_for_point_from_element};
use crate::linear_element_editor::{
    create_point_at, get_point_at_index_global_coordinates, get_point_global_coordinates,
    move_points, point_from_absolute_coords, MovePointsOtherUpdates, PointUpdate,
};
use crate::resize_elements::resize_bound_text;
use crate::scene::{ElementUpdate, MutationEnv, Scene};
use crate::transform::get_grid_point;

pub use crate::geometry::{
    get_binding_gap, get_global_fixed_point_for_bindable_element, max_binding_distance_simple,
    normalize_fixed_point, BASE_BINDING_GAP,
};

/// `BASE_ARROW_MIN_LENGTH` (`binding.ts:118`): an orbiting end closer than
/// this to the other end goes to its fixed point instead of the outline.
pub const BASE_ARROW_MIN_LENGTH: f64 = 10.0;

/// `FOCUS_POINT_SIZE` (`binding.ts:119`).
pub const FOCUS_POINT_SIZE: f64 = 10.0 / 1.5;

/// `MIN_BINDABLE_SIZE` (`binding.ts:123`): below this size a target binds
/// at its centre.
const MIN_BINDABLE_SIZE: f64 = 1.0;

// -- the environment -------------------------------------------------------------------

/// What binding needs from the editor: `mutateElement`'s nonce and clock,
/// and the text metrics an arrow's label is re-wrapped with when the arrow
/// moves (`handleBindTextResize`).
pub trait BindingEnv: MutationEnv {
    /// The text metrics and the per-font character width cache
    /// (`textMeasurements.ts`'s provider and `charWidth`).
    fn text(&mut self) -> (&dyn TextMetricsProvider, &mut CharWidthCache);
}

/// The start or the end of an arrow (`"start" | "end"`, and
/// `"startBinding" | "endBinding"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingEnd {
    Start,
    End,
}

impl BindingEnd {
    /// The other end.
    pub fn opposite(self) -> BindingEnd {
        match self {
            BindingEnd::Start => BindingEnd::End,
            BindingEnd::End => BindingEnd::Start,
        }
    }

    /// The index of the adjacent point, counting from the end when
    /// negative (`startOrEnd === "start" ? 1 : -2`).
    fn adjacent_index(self) -> isize {
        match self {
            BindingEnd::Start => 1,
            BindingEnd::End => -2,
        }
    }

    /// The index of the end point (`0` or `-1`).
    fn index(self) -> isize {
        match self {
            BindingEnd::Start => 0,
            BindingEnd::End => -1,
        }
    }
}

/// An invariant upstream's binding code asserts, with upstream's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingError(pub String);

impl fmt::Display for BindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BindingError {}

fn invariant(condition: bool, message: &str) -> Result<(), BindingError> {
    if condition {
        Ok(())
    } else {
        Err(BindingError(message.to_owned()))
    }
}

// -- small helpers ---------------------------------------------------------------------

type P = [f64; 2];

fn gp(p: P) -> GlobalPoint {
    point_from(p[0], p[1])
}

fn rotate(p: P, center: P, angle: f64) -> P {
    let r = point_rotate_rads(gp(p), gp(center), Radians(angle));
    [r.x, r.y]
}

fn distance(a: P, b: P) -> f64 {
    js::hypot(b[0] - a[0], b[1] - a[1])
}

fn distance_sq(a: P, b: P) -> f64 {
    let x = b[0] - a[0];
    let y = b[1] - a[1];
    x * x + y * y
}

/// `vectorNormalize(vectorFromPoint(p, origin))`.
fn direction(p: P, origin: P) -> P {
    let v = [p[0] - origin[0], p[1] - origin[1]];
    let m = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if m == 0.0 {
        [0.0, 0.0]
    } else {
        [v[0] / m, v[1] / m]
    }
}

/// `pointFromVector(vectorScale(v, s), origin)`.
fn along(origin: P, v: P, s: f64) -> P {
    [origin[0] + v[0] * s, origin[1] + v[1] * s]
}

/// JavaScript truthiness of a number (`a || b`).
fn or(a: f64, b: f64) -> f64 {
    if a != 0.0 && !a.is_nan() {
        a
    } else {
        b
    }
}

/// `points.sort((a, b) => distSq(a, to) - distSq(b, to))[0]`.
fn nearest(mut points: Vec<P>, to: P) -> Option<P> {
    js::sort(&mut points, |a, b| {
        distance_sq(*a, to) - distance_sq(*b, to)
    });
    points.first().copied()
}

fn intersect(element: &Element, map: &ElementsMap<'_>, segment: [P; 2], offset: f64) -> Vec<P> {
    intersect_element_with_line_segment(element, map, segment, offset, false)
}

/// `hitElementItself({ point, element, elementsMap, threshold,
/// overrideShouldTestInside })`, with a cache of its own.
fn hit(element: &Element, point: P, map: &ElementsMap<'_>, threshold: f64, inside: bool) -> bool {
    hit_element_itself(
        &mut HitTestCache::new(),
        &HitTestArgs {
            point,
            element,
            threshold,
            elements_map: map,
            frame_name_bound: None,
            override_should_test_inside: inside,
        },
    )
}

fn is_arrow(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Arrow(_))
}

fn is_elbow_arrow(e: &Element) -> bool {
    matches!(&e.kind, ElementKind::Arrow(a) if a.elbowed)
}

fn points(e: &Element) -> &[LocalPoint] {
    e.kind.points().unwrap_or(&[])
}

/// `arrow.startBinding` / `arrow.endBinding`.
fn binding_at(arrow: &Element, end: BindingEnd) -> Option<&FixedPointBinding> {
    let linear = arrow.kind.linear()?;
    match end {
        BindingEnd::Start => linear.start_binding.as_ref(),
        BindingEnd::End => linear.end_binding.as_ref(),
    }
}

/// `isRectanguloidElement(element)` (`typeChecks.ts:204-219`).
fn is_rectanguloid(e: &Element) -> bool {
    match &e.kind {
        ElementKind::Text(t) => t.container_id.as_deref().is_none_or(str::is_empty),
        ElementKind::Rectangle
        | ElementKind::StickyNote(_)
        | ElementKind::Diamond
        | ElementKind::Image(_)
        | ElementKind::Iframe
        | ElementKind::Embeddable
        | ElementKind::Frame(_)
        | ElementKind::MagicFrame(_) => true,
        _ => false,
    }
}

/// `isRectangularElement(element)` (`typeChecks.ts:223-238`).
fn is_rectangular(e: &Element) -> bool {
    matches!(
        e.kind,
        ElementKind::Rectangle
            | ElementKind::StickyNote(_)
            | ElementKind::Image(_)
            | ElementKind::Text(_)
            | ElementKind::Iframe
            | ElementKind::Embeddable
            | ElementKind::Frame(_)
            | ElementKind::MagicFrame(_)
            | ElementKind::Freedraw(_)
    )
}

/// The scene's non-deleted elements, plus the `extra` ids even when
/// deleted (`updateBoundElements` with `changedElements`).
fn lookup<'a>(scene: &'a Scene, extra: &HashSet<&str>) -> ElementsMap<'a> {
    ElementsMap::new(
        scene
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted || extra.contains(e.base.id.as_str())),
    )
}

// -- fixed points ----------------------------------------------------------------------

/// `isFixedPoint(fixedPoint)` (`binding.ts:2739-2748`): two finite ratios.
pub fn is_fixed_point(fixed_point: &[f64]) -> bool {
    fixed_point.len() == 2 && fixed_point.iter().all(|r| r.is_finite())
}

/// `getGlobalFixedPoints(arrow, elementsMap)` (`binding.ts:2687-2725`):
/// each end's fixed point in scene coordinates, or the end point where the
/// end is not bound (its target not in the map).
pub fn get_global_fixed_points(arrow: &Element, elements_map: &ElementsMap<'_>) -> [P; 2] {
    let pts = points(arrow);
    let b = &arrow.base;
    let at = |p: Option<&LocalPoint>| {
        let p = p.copied().unwrap_or([f64::NAN, f64::NAN]);
        [b.x + p[0], b.y + p[1]]
    };
    let fixed = |end: BindingEnd| {
        binding_at(arrow, end).and_then(|binding| {
            elements_map.get(&binding.element_id).map(|target| {
                get_global_fixed_point_for_bindable_element(binding.fixed_point, target)
            })
        })
    };
    [
        fixed(BindingEnd::Start).unwrap_or_else(|| at(pts.first())),
        fixed(BindingEnd::End).unwrap_or_else(|| at(pts.last())),
    ]
}

/// `getArrowLocalFixedPoints(arrow, elementsMap)` (`binding.ts:2727-2737`):
/// [`get_global_fixed_points`] as points of the arrow.
pub fn get_arrow_local_fixed_points(arrow: &Element, elements_map: &ElementsMap<'_>) -> [P; 2] {
    let [start, end] = get_global_fixed_points(arrow, elements_map);
    [
        point_from_absolute_coords(arrow, start, elements_map),
        point_from_absolute_coords(arrow, end, elements_map),
    ]
}

/// `calculateFixedPointForElbowArrowBinding(linearElement, hoveredElement,
/// startOrEnd, elementsMap, zoom, shouldSnapToOutline,
/// isMidpointSnappingEnabled)` (`binding.ts:2130-2194`): the elbow arrow's
/// end, snapped to the outline, as ratios of the unrotated target (at the
/// centre of a target under 1 px).
pub fn calculate_fixed_point_for_elbow_arrow_binding(
    arrow: &Element,
    hovered: &Element,
    end: BindingEnd,
    elements_map: &ElementsMap<'_>,
    zoom: f64,
    should_snap_to_outline: bool,
    is_midpoint_snapping_enabled: bool,
) -> [f64; 2] {
    let h = &hovered.base;
    let bounds = [h.x, h.y, h.x + h.width, h.y + h.height];
    let snapped = if should_snap_to_outline {
        bind_point_to_snap_to_element_outline(
            arrow,
            hovered,
            end,
            elements_map,
            zoom,
            None,
            is_midpoint_snapping_enabled,
        )
    } else {
        get_point_at_index_global_coordinates(arrow, end.index(), elements_map)
    };
    let global_mid_point = [
        bounds[0] + (bounds[2] - bounds[0]) / 2.0,
        bounds[1] + (bounds[3] - bounds[1]) / 2.0,
    ];
    let non_rotated = rotate(snapped, global_mid_point, -h.angle.0);
    if h.width < MIN_BINDABLE_SIZE || h.height < MIN_BINDABLE_SIZE {
        return normalize_fixed_point([0.5, 0.5]);
    }
    let size_floor = get_binding_gap(hovered);
    normalize_fixed_point([
        (non_rotated[0] - h.x) / js::max(h.width, size_floor),
        (non_rotated[1] - h.y) / js::max(h.height, size_floor),
    ])
}

/// `calculateFixedPointForNonElbowArrowBinding(linearElement,
/// hoveredElement, startOrEnd, elementsMap, focusPoint)`
/// (`binding.ts:2196-2247`): the focus point (or the arrow's end) as ratios
/// of the unrotated target (its centre for a target under 1 px).
pub fn calculate_fixed_point_for_non_elbow_arrow_binding(
    arrow: &Element,
    hovered: &Element,
    end: BindingEnd,
    elements_map: &ElementsMap<'_>,
    focus_point: Option<P>,
) -> [f64; 2] {
    let edge_point = focus_point
        .unwrap_or_else(|| get_point_at_index_global_coordinates(arrow, end.index(), elements_map));
    let center = element_center_point(hovered, elements_map);
    let h = &hovered.base;
    let non_rotated = rotate(edge_point, center, -h.angle.0);
    if h.width < MIN_BINDABLE_SIZE || h.height < MIN_BINDABLE_SIZE {
        return normalize_fixed_point([0.5, 0.5]);
    }
    let size_floor = get_binding_gap(hovered);
    normalize_fixed_point([
        (non_rotated[0] - h.x) / js::max(h.width, size_floor),
        (non_rotated[1] - h.y) / js::max(h.height, size_floor),
    ])
}

// -- snapping to the outline ---------------------------------------------------------

/// `avoidRectangularCorner(arrowElement, bindTarget, elementsMap, p)`
/// (`binding.ts:1765-1854`): a point past a corner of the target's box
/// moved onto the gap line of the nearer side, next to the corner.
pub fn avoid_rectangular_corner(
    _arrow: &Element,
    target: &Element,
    elements_map: &ElementsMap<'_>,
    p: P,
) -> P {
    let center = element_center_point(target, elements_map);
    let t = &target.base;
    let angle = t.angle.0;
    let non_rotated = rotate(p, center, -angle);
    let gap = get_binding_gap(target);
    let back = |x: f64, y: f64| rotate([x, y], center, angle);

    if non_rotated[0] < t.x && non_rotated[1] < t.y {
        // Top left
        if non_rotated[1] - t.y > -gap {
            return back(t.x - gap, t.y);
        }
        back(t.x, t.y - gap)
    } else if non_rotated[0] < t.x && non_rotated[1] > t.y + t.height {
        // Bottom left
        if non_rotated[0] - t.x > -gap {
            return back(t.x, t.y + t.height + gap);
        }
        back(t.x - gap, t.y + t.height)
    } else if non_rotated[0] > t.x + t.width && non_rotated[1] > t.y + t.height {
        // Bottom right
        if non_rotated[0] - t.x < t.width + gap {
            return back(t.x + t.width, t.y + t.height + gap);
        }
        back(t.x + t.width + gap, t.y + t.height)
    } else if non_rotated[0] > t.x + t.width && non_rotated[1] < t.y {
        // Top right
        if non_rotated[0] - t.x < t.width + gap {
            return back(t.x + t.width, t.y - gap);
        }
        back(t.x + t.width + gap, t.y)
    } else {
        p
    }
}

/// `bindPointToSnapToElementOutline(arrowElement, bindableElement,
/// startOrEnd, elementsMap, zoom, customIntersector,
/// isMidpointSnappingEnabled)` (`binding.ts:1617-1763`): where the arrow's
/// end meets the target's outline grown by the binding gap. A simple arrow
/// looks along the line from its adjacent point; an elbow arrow along the
/// axis of the side it binds to, snapped to the side's midpoint when near
/// it.
pub fn bind_point_to_snap_to_element_outline(
    arrow: &Element,
    bindable: &Element,
    end: BindingEnd,
    elements_map: &ElementsMap<'_>,
    zoom: f64,
    custom_intersector: Option<[P; 2]>,
    is_midpoint_snapping_enabled: bool,
) -> P {
    let elbowed = is_elbow_arrow(arrow);
    let point = get_point_at_index_global_coordinates(arrow, end.index(), elements_map);
    if points(arrow).len() < 2 {
        // New arrow creation, so no snapping
        return point;
    }
    let edge_point = if is_rectanguloid(bindable) && elbowed {
        avoid_rectangular_corner(arrow, bindable, elements_map, point)
    } else {
        point
    };
    let adjacent_point = match custom_intersector {
        Some(i) if !elbowed => i[1],
        _ => get_point_at_index_global_coordinates(arrow, end.adjacent_index(), elements_map),
    };
    let gap = get_binding_gap(bindable);
    let aabb = aabb_for_element(bindable, None);
    let bindable_center = get_center_for_bounds(aabb);
    let reach = js::max(bindable.base.width, bindable.base.height) * 2.0;

    let intersection = if elbowed {
        let snap = if is_midpoint_snapping_enabled {
            get_elbow_arrow_snap_mid_point(edge_point, bindable, elements_map, zoom)
        } else {
            None
        };
        let resolved = snap.map_or(point, |s| s.point);
        let is_horizontal = heading_is_horizontal(match snap {
            Some(s) if s.on_axis => vector_to_heading([
                s.point[0] - bindable_center[0],
                s.point[1] - bindable_center[1],
            ]),
            _ => heading_for_point_from_element(bindable, aabb, point),
        });
        let other_point = [
            if is_horizontal {
                bindable_center[0]
            } else {
                resolved[0]
            },
            if !is_horizontal {
                bindable_center[1]
            } else {
                resolved[1]
            },
        ];
        let intersector = custom_intersector.unwrap_or_else(|| {
            [
                other_point,
                along(other_point, direction(resolved, other_point), reach),
            ]
        });
        nearest(
            intersect(bindable, elements_map, intersector, gap),
            resolved,
        )
        .or_else(|| {
            let another_point = [
                if !is_horizontal {
                    bindable_center[0]
                } else {
                    resolved[0]
                },
                if is_horizontal {
                    bindable_center[1]
                } else {
                    resolved[1]
                },
            ];
            let another_intersector = [
                another_point,
                along(another_point, direction(resolved, another_point), reach),
            ];
            nearest(
                intersect(
                    bindable,
                    elements_map,
                    another_intersector,
                    BASE_BINDING_GAP,
                ),
                resolved,
            )
        })
    } else {
        let intersector = custom_intersector.unwrap_or_else(|| {
            let length = distance(edge_point, adjacent_point)
                + js::max(bindable.base.width, bindable.base.height)
                + gap * 2.0;
            let half = direction(edge_point, adjacent_point);
            [
                along(adjacent_point, half, length),
                along(adjacent_point, half, -length),
            ]
        });
        if distance(edge_point, adjacent_point) < 1.0 {
            Some(edge_point)
        } else {
            nearest(
                intersect(bindable, elements_map, intersector, gap),
                adjacent_point,
            )
        }
    };

    match intersection {
        // Too close to determine vector from intersection to edgePoint
        Some(i) if distance_sq(edge_point, i) >= PRECISION => i,
        _ => edge_point,
    }
}

/// `snapBoundPointToGrid(outlinePoint, bindableElement, elementsMap,
/// gridSize, arrowElement, adjacentPoint)` (`binding.ts:1896-1967`): the
/// outline point moved to the grid along the side it is on, keeping the
/// gap from the side (where the grid line meets the grown outline).
pub fn snap_bound_point_to_grid(
    outline_point: P,
    bindable: &Element,
    elements_map: &ElementsMap<'_>,
    grid_size: Option<f64>,
    _arrow: &Element,
    adjacent_point: Option<P>,
) -> P {
    let Some(grid) = grid_size.filter(|&g| g != 0.0 && !g.is_nan()) else {
        return outline_point;
    };
    let aabb = aabb_for_element(bindable, None);
    let heading = match adjacent_point {
        Some(adjacent) if matches!(bindable.kind, ElementKind::Ellipse | ElementKind::Diamond) => {
            vector_to_heading([
                adjacent[0] - outline_point[0],
                adjacent[1] - outline_point[1],
            ])
        }
        _ => heading_for_point_from_element(bindable, aabb, outline_point),
    };
    let gap = get_binding_gap(bindable);
    let extent = js::max(bindable.base.width, bindable.base.height) + gap * 2.0;
    let center = get_center_for_bounds(aabb);
    let [snapped_x, snapped_y] = get_grid_point(outline_point[0], outline_point[1], Some(grid));
    let intersector = if heading_is_horizontal(heading) {
        // Global X is closest to the perpendicular so snap Y, intersect horizontal line
        [
            [center[0] - extent, snapped_y],
            [center[0] + extent, snapped_y],
        ]
    } else {
        // Global Y is closest to the perpendicular so snap X, intersect vertical line
        [
            [snapped_x, center[1] - extent],
            [snapped_x, center[1] + extent],
        ]
    };
    let fallback = if heading_is_horizontal(heading) {
        [outline_point[0], snapped_y]
    } else {
        [snapped_x, outline_point[1]]
    };
    nearest(
        intersect(bindable, elements_map, intersector, gap),
        outline_point,
    )
    .unwrap_or(fallback)
}

// -- midpoints ------------------------------------------------------------------------

/// `getAllMidpoints(element, elementsMap)` (`utils.ts:731-757`): the
/// midpoints of the right, bottom, left and top sides (a diamond's
/// vertices), rotated with the element.
pub fn get_all_midpoints(element: &Element, elements_map: &ElementsMap<'_>) -> Vec<P> {
    let center = element_center_point(element, elements_map);
    let b = &element.base;
    if matches!(element.kind, ElementKind::Diamond) {
        return get_diamond_base_corners(element)
            .iter()
            .map(|&curve| {
                let p = bezier_equation(curve, 0.5);
                rotate([p.x, p.y], center, b.angle.0)
            })
            .collect();
    }
    [
        [b.width, b.height / 2.0],
        [b.width / 2.0, b.height],
        [0.0, b.height / 2.0],
        [b.width / 2.0, 0.0],
    ]
    .iter()
    .map(|&[x, y]| rotate([b.x + x, b.y + y], center, b.angle.0))
    .collect()
}

/// A midpoint an elbow arrow's end snaps to: `onAxis` for the midpoints on
/// the element's axes (whose direction from the centre is unambiguous),
/// not for a diamond's edge midpoints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnappedMidpoint {
    pub point: P,
    pub on_axis: bool,
}

/// `getSnappedMidpointForElbowArrow` (`utils.ts:633-707`).
fn get_snapped_midpoint_for_elbow_arrow(
    element: &Element,
    point: P,
    elements_map: &ElementsMap<'_>,
    center: P,
    horizontal_threshold: f64,
    vertical_threshold: f64,
) -> Option<SnappedMidpoint> {
    let b = &element.base;
    let (x, y, width, height, angle) = (b.x, b.y, b.width, b.height, b.angle.0);
    let non_rotated = rotate(point, center, -angle);
    let gap = get_binding_gap(element);
    if distance(center, non_rotated) < gap {
        return None;
    }
    let mids = get_all_midpoints(element, elements_map);
    let (right, bottom, left, top) = (mids[0], mids[1], mids[2], mids[3]);
    let on_axis = |point: P| {
        Some(SnappedMidpoint {
            point,
            on_axis: true,
        })
    };
    if non_rotated[0] <= x + width / 2.0
        && non_rotated[1] > center[1] - vertical_threshold
        && non_rotated[1] < center[1] + vertical_threshold
    {
        return on_axis(left);
    } else if non_rotated[1] <= y + height / 2.0
        && non_rotated[0] > center[0] - horizontal_threshold
        && non_rotated[0] < center[0] + horizontal_threshold
    {
        return on_axis(top);
    } else if non_rotated[0] >= x + width / 2.0
        && non_rotated[1] > center[1] - vertical_threshold
        && non_rotated[1] < center[1] + vertical_threshold
    {
        return on_axis(right);
    } else if non_rotated[1] >= y + height / 2.0
        && non_rotated[0] > center[0] - horizontal_threshold
        && non_rotated[0] < center[0] + horizontal_threshold
    {
        return on_axis(bottom);
    } else if matches!(element.kind, ElementKind::Diamond) {
        // Elbow arrows can also snap to the midpoints of a diamond's edges
        let threshold = js::max(horizontal_threshold, vertical_threshold);
        let edge_midpoints = [
            // top-left, top-right, bottom-left, bottom-right
            [x + width / 4.0, y + height / 4.0, -1.0, -1.0],
            [x + (3.0 * width) / 4.0, y + height / 4.0, 1.0, -1.0],
            [x + width / 4.0, y + (3.0 * height) / 4.0, -1.0, 1.0],
            [x + (3.0 * width) / 4.0, y + (3.0 * height) / 4.0, 1.0, 1.0],
        ];
        for [edge_x, edge_y, dir_x, dir_y] in edge_midpoints {
            // the snap zone sits a binding gap outside the edge
            let zone_center = [edge_x + dir_x * gap, edge_y + dir_y * gap];
            if distance(zone_center, non_rotated) < threshold {
                return Some(SnappedMidpoint {
                    point: rotate([edge_x, edge_y], center, angle),
                    on_axis: false,
                });
            }
        }
    }
    None
}

/// `getElbowArrowSnapMidPoint(point, element, elementsMap, zoom)`
/// (`utils.ts:769-786`): the midpoint an elbow arrow's end at `point`
/// snaps to, within 5% of the element's size (at least 5, at most the
/// binding distance and half the stroke).
pub fn get_elbow_arrow_snap_mid_point(
    point: P,
    element: &Element,
    elements_map: &ElementsMap<'_>,
    zoom: f64,
) -> Option<SnappedMidpoint> {
    const TOLERANCE: f64 = 0.05;
    let max_distance = max_binding_distance_simple(zoom) + element.base.stroke_width / 2.0;
    get_snapped_midpoint_for_elbow_arrow(
        element,
        point,
        elements_map,
        element_center_point(element, elements_map),
        excali_math::clamp(TOLERANCE * element.base.width, 5.0, max_distance),
        excali_math::clamp(TOLERANCE * element.base.height, 5.0, max_distance),
    )
}

/// `getSnappedMidpointIndexForSimpleArrow` (`utils.ts:709-729`): the first
/// midpoint within `threshold` of a point outside the element.
fn get_snapped_midpoint_index_for_simple_arrow(
    element: &Element,
    point: P,
    elements_map: &ElementsMap<'_>,
    threshold: f64,
) -> Option<usize> {
    get_all_midpoints(element, elements_map)
        .iter()
        .position(|&mid| {
            distance(mid, point) <= threshold && !hit(element, point, elements_map, 0.0, true)
        })
}

/// `getSnapOutlineMidPoint(point, element, elementsMap, zoom, arrow)`
/// (`utils.ts:788-808`): the side midpoint an arrow's end at `point` snaps
/// to, for an elbow arrow ([`get_elbow_arrow_snap_mid_point`]) or a simple
/// one (a midpoint within the binding distance, from outside).
pub fn get_snap_outline_mid_point(
    point: P,
    element: &Element,
    elements_map: &ElementsMap<'_>,
    zoom: f64,
    elbowed: bool,
) -> Option<P> {
    if elbowed {
        return get_elbow_arrow_snap_mid_point(point, element, elements_map, zoom).map(|s| s.point);
    }
    let max_distance = max_binding_distance_simple(zoom) + element.base.stroke_width / 2.0;
    get_snapped_midpoint_index_for_simple_arrow(element, point, elements_map, max_distance)
        .map(|i| get_all_midpoints(element, elements_map)[i])
}

/// `getDiagonalsForBindableElement(element, elementsMap)`
/// (`utils.ts:550-631`): a rectangular element's diagonals (a rectangle's
/// shortened by 15 px at each end), anything else's centre lines.
fn get_diagonals_for_bindable_element(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> [[P; 2]; 2] {
    let offset_px = if matches!(element.kind, ElementKind::Rectangle) {
        15.0
    } else {
        0.0
    };
    let shrink = |seg: [P; 2]| {
        let v = direction(seg[1], seg[0]);
        let offset = [v[0] * offset_px, v[1] * offset_px];
        [
            [seg[0][0] + offset[0], seg[0][1] + offset[1]],
            [seg[1][0] - offset[0], seg[1][1] - offset[1]],
        ]
    };
    let center = element_center_point(element, elements_map);
    let b = &element.base;
    let r = |x: f64, y: f64| rotate([x, y], center, b.angle.0);
    if is_rectangular(element) {
        [
            shrink([r(b.x, b.y), r(b.x + b.width, b.y + b.height)]),
            shrink([r(b.x + b.width, b.y), r(b.x, b.y + b.height)]),
        ]
    } else {
        [
            shrink([
                r(b.x + b.width / 2.0, b.y),
                r(b.x + b.width / 2.0, b.y + b.height),
            ]),
            shrink([
                r(b.x, b.y + b.height / 2.0),
                r(b.x + b.width, b.y + b.height / 2.0),
            ]),
        ]
    }
}

/// `projectFixedPointOntoDiagonal(arrow, point, element, startOrEnd,
/// elementsMap, zoom, isMidpointSnappingEnabled)` (`utils.ts:810-903`): a
/// side midpoint near `point`, or where the line from the arrow's other
/// end (its other fixed point for a two-point arrow) through `point`
/// crosses the nearer diagonal, if that is inside the element. `None` for
/// an arrow under 3 px each way (no direction to project along) and for an
/// arrow of fewer than two points (upstream asserts two).
pub fn project_fixed_point_onto_diagonal(
    arrow: &Element,
    point: P,
    element: &Element,
    end: BindingEnd,
    elements_map: &ElementsMap<'_>,
    zoom: f64,
    is_midpoint_snapping_enabled: bool,
) -> Option<P> {
    let pts = points(arrow);
    if pts.len() < 2 {
        return None;
    }
    if is_midpoint_snapping_enabled {
        if let Some(mid) =
            get_snap_outline_mid_point(point, element, elements_map, zoom, is_elbow_arrow(arrow))
        {
            return Some(mid);
        }
    }
    // Projection needs a meaningful arrow direction
    if arrow.base.width < 3.0 && arrow.base.height < 3.0 {
        return None;
    }
    let [diagonal_one, diagonal_two] = get_diagonals_for_bindable_element(element, elements_map);
    let index = match end {
        BindingEnd::Start => 1,
        BindingEnd::End => pts.len() as isize - 2,
    };
    let mut a = get_point_at_index_global_coordinates(arrow, index, elements_map);
    if pts.len() == 2 {
        let other = binding_at(arrow, end.opposite()).and_then(|binding| {
            elements_map.get(&binding.element_id).map(|bindable| {
                get_global_fixed_point_for_bindable_element(
                    normalize_fixed_point(binding.fixed_point),
                    bindable,
                )
            })
        });
        if let Some(other) = other {
            a = other;
        }
    }
    let longest = js::max(
        distance(diagonal_one[0], diagonal_one[1]),
        distance(diagonal_two[0], diagonal_two[1]),
    );
    let v = [point[0] - a[0], point[1] - a[1]];
    let scale = 2.0 * distance(a, point) + longest;
    let b = [a[0] + v[0] * scale, a[1] + v[1] * scale];
    let intersector = line_segment(gp(b), gp(a));
    let seg = |s: [P; 2]| line_segment(gp(s[0]), gp(s[1]));
    let p1 = line_segment_intersection_points(seg(diagonal_one), intersector).map(|p| [p.x, p.y]);
    let p2 = line_segment_intersection_points(seg(diagonal_two), intersector).map(|p| [p.x, p.y]);
    let projection = match (p1, p2) {
        (Some(p1), Some(p2)) => {
            if distance(a, p1) < distance(a, p2) {
                Some(p1)
            } else {
                Some(p2)
            }
        }
        (p1, p2) => p1.or(p2),
    };
    projection.filter(|&p| is_point_in_element(p, element, elements_map))
}

// -- side midpoints --------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
}

/// `SHAPE_CONFIGS` (`binding.ts:2805-2841`): each sector's centre angle
/// and width in degrees. Rectangles have 15 degree corners and 75 degree
/// sides; diamonds and ellipses the other way round.
fn shape_config(element: &Element) -> [(f64, f64, Side); 8] {
    let (corner, side) = match element.kind {
        ElementKind::Ellipse | ElementKind::Diamond => (75.0, 15.0),
        _ => (15.0, 75.0),
    };
    [
        (0.0, side, Side::Right),
        (45.0, corner, Side::BottomRight),
        (90.0, side, Side::Bottom),
        (135.0, corner, Side::BottomLeft),
        (180.0, side, Side::Left),
        (225.0, corner, Side::TopLeft),
        (270.0, side, Side::Top),
        (315.0, corner, Side::TopRight),
    ]
}

/// `getShapeSideAdaptive(fixedPoint, shapeType)` (`binding.ts:2860-2911`).
fn get_shape_side_adaptive(fixed_point: [f64; 2], element: &Element) -> Side {
    let [x, y] = fixed_point;
    let mut angle = js::atan2(y - 0.5, x - 0.5);
    if angle < 0.0 {
        angle += 2.0 * std::f64::consts::PI;
    }
    let degrees = (angle * 180.0) / std::f64::consts::PI;
    let config = shape_config(element);
    for &(center, width, side) in &config {
        let half = width / 2.0;
        let start = (((center - half) % 360.0) + 360.0) % 360.0;
        let end = (((center + half) % 360.0) + 360.0) % 360.0;
        if start <= end {
            if degrees >= start && degrees <= end {
                return side;
            }
        } else if degrees >= start || degrees <= end {
            return side;
        }
    }
    // fallback - find nearest sector center
    let mut min_diff = f64::INFINITY;
    let mut nearest_side = config[0].2;
    for &(center, _, side) in &config {
        let mut diff = (degrees - center).abs();
        if diff > 180.0 {
            diff = 360.0 - diff;
        }
        if diff < min_diff {
            min_diff = diff;
            nearest_side = side;
        }
    }
    nearest_side
}

fn mid(a: GlobalPoint, b: GlobalPoint) -> P {
    [(a.x + b.x) / 2.0, (a.y + b.y) / 2.0]
}

/// `getBindingSideMidPoint(binding, elementsMap)` (`binding.ts:2913-3204`):
/// the midpoint of the side (or corner) of the bound element the fixed
/// point is on, a hundredth of a pixel outside it, rotated with the
/// element. `None` when the element is missing, deleted or not bindable.
pub fn get_binding_side_mid_point(
    binding: &FixedPointBinding,
    elements_map: &ElementsMap<'_>,
) -> Option<P> {
    let element = elements_map
        .get(&binding.element_id)
        .filter(|e| !e.base.is_deleted && e.is_bindable())?;
    let center = element_center_point(element, elements_map);
    let side = get_shape_side_adaptive(normalize_fixed_point(binding.fixed_point), element);
    // small offset to avoid precision issues in elbow
    const OFFSET: f64 = 0.01;
    const DIAGONAL: f64 = OFFSET * 0.707;
    let b = &element.base;

    let [x, y] = match element.kind {
        ElementKind::Diamond => {
            let outline = deconstruct_diamond_element(element);
            let [bottom_right, bottom_left, top_left, top_right] = outline.sides;
            let corners = outline.corners;
            match side {
                // the vertex of each corner curve
                Side::Left => [corners[2].1.x - OFFSET, corners[2].1.y],
                Side::Right => [corners[0].1.x + OFFSET, corners[0].1.y],
                Side::Top => [corners[3].1.x, corners[3].1.y - OFFSET],
                Side::Bottom => [corners[1].1.x, corners[1].1.y + OFFSET],
                Side::TopRight => {
                    let m = mid(top_right.0, top_right.1);
                    [m[0] + DIAGONAL, m[1] - DIAGONAL]
                }
                Side::BottomRight => {
                    let m = mid(bottom_right.0, bottom_right.1);
                    [m[0] + DIAGONAL, m[1] + DIAGONAL]
                }
                Side::BottomLeft => {
                    let m = mid(bottom_left.0, bottom_left.1);
                    [m[0] - DIAGONAL, m[1] + DIAGONAL]
                }
                Side::TopLeft => {
                    let m = mid(top_left.0, top_left.1);
                    [m[0] - DIAGONAL, m[1] - DIAGONAL]
                }
            }
        }
        ElementKind::Ellipse => {
            let cx = b.x + b.width / 2.0;
            let cy = b.y + b.height / 2.0;
            let rx = b.width / 2.0;
            let ry = b.height / 2.0;
            let on = |angle: f64| [rx * js::cos(angle), ry * js::sin(angle)];
            let pi = std::f64::consts::PI;
            match side {
                Side::Top => [cx, cy - ry - OFFSET],
                Side::Right => [cx + rx + OFFSET, cy],
                Side::Bottom => [cx, cy + ry + OFFSET],
                Side::Left => [cx - rx - OFFSET, cy],
                Side::TopRight => {
                    let [ex, ey] = on(-pi / 4.0);
                    [cx + ex + DIAGONAL, cy + ey - DIAGONAL]
                }
                Side::BottomRight => {
                    let [ex, ey] = on(pi / 4.0);
                    [cx + ex + DIAGONAL, cy + ey + DIAGONAL]
                }
                Side::BottomLeft => {
                    let [ex, ey] = on((3.0 * pi) / 4.0);
                    [cx + ex - DIAGONAL, cy + ey + DIAGONAL]
                }
                Side::TopLeft => {
                    let [ex, ey] = on((-3.0 * pi) / 4.0);
                    [cx + ex - DIAGONAL, cy + ey - DIAGONAL]
                }
            }
        }
        _ if is_rectangular(element) => {
            let outline = deconstruct_rectanguloid_element(element);
            let [top, right, bottom, left] = outline.sides;
            let corner_mid = |i: usize| mid(outline.corners[i].0, outline.corners[i].3);
            match side {
                Side::Top => {
                    let m = mid(top.0, top.1);
                    [m[0], m[1] - OFFSET]
                }
                Side::Right => {
                    let m = mid(right.0, right.1);
                    [m[0] + OFFSET, m[1]]
                }
                Side::Bottom => {
                    let m = mid(bottom.0, bottom.1);
                    [m[0], m[1] + OFFSET]
                }
                Side::Left => {
                    let m = mid(left.0, left.1);
                    [m[0] - OFFSET, m[1]]
                }
                Side::TopLeft => {
                    let m = corner_mid(0);
                    [m[0] - DIAGONAL, m[1] - DIAGONAL]
                }
                Side::TopRight => {
                    let m = corner_mid(1);
                    [m[0] + DIAGONAL, m[1] - DIAGONAL]
                }
                Side::BottomRight => {
                    let m = corner_mid(2);
                    [m[0] + DIAGONAL, m[1] + DIAGONAL]
                }
                Side::BottomLeft => {
                    let m = corner_mid(3);
                    [m[0] - DIAGONAL, m[1] + DIAGONAL]
                }
            }
        }
        _ => return None,
    };
    Some(rotate([x, y], center, b.angle.0))
}

// -- following a moved element ---------------------------------------------------------

/// `extractBinding(arrow, startOrEnd, elementsMap)` (`binding.ts:1856-1894`):
/// the bound element and its fixed point in scene coordinates.
fn extract_binding<'a>(
    arrow: &Element,
    end: BindingEnd,
    elements_map: &ElementsMap<'a>,
) -> Option<(&'a Element, P)> {
    let binding = binding_at(arrow, end)?;
    let element = elements_map.get(&binding.element_id)?;
    Some((
        element,
        get_global_fixed_point_for_bindable_element(
            normalize_fixed_point(binding.fixed_point),
            element,
        ),
    ))
}

fn element_area(e: &Element) -> f64 {
    e.base.width * e.base.height
}

/// `updateBoundPoint(arrow, startOrEnd, binding, bindableElement,
/// elementsMap, dragging)` (`binding.ts:1972-2128`): where the arrow's end
/// goes for `binding` on `bindable`, as a point of the arrow; `None` when
/// there is no binding, when a multi-point arrow's binding is to another
/// element, and for an arrow just created (last point at `[0, 0]`).
///
/// An inside binding goes to the fixed point. An orbiting end goes where
/// the line from its fixed point to the other end's (or the adjacent
/// point) crosses the outline grown by the gap, except that it goes to the
/// fixed point (for an end with an arrowhead, or with none on either end)
/// when that crossing lies inside the other bound element, and to the
/// fixed point when the arrow would be under 10 long.
pub fn update_bound_point(
    arrow: &Element,
    end: BindingEnd,
    binding: Option<&FixedPointBinding>,
    bindable: &Element,
    elements_map: &ElementsMap<'_>,
    dragging: bool,
) -> Option<LocalPoint> {
    let pts = points(arrow);
    let binding = binding?;
    let last = pts.last()?;
    if (binding.element_id != bindable.base.id && pts.len() > 2)
        // Initial arrow created on pointer down needs to not update the points
        || points_equal(gp(*last), gp([0.0, 0.0]))
    {
        return None;
    }
    let focus = get_global_fixed_point_for_bindable_element(
        normalize_fixed_point(binding.fixed_point),
        bindable,
    );
    let at = |x: f64, y: f64| Some(create_point_at(arrow, elements_map, x, y, None));

    // 0. Short-circuit for inside binding as it doesn't require any
    // calculations and is not affected by other bindings
    if binding.mode == BindMode::Inside {
        return at(focus[0], focus[1]);
    }

    let other = extract_binding(arrow, end.opposite(), elements_map);
    let other_bindable = other.map(|(e, _)| e);
    let other_focus = other.map(|(_, p)| p);
    let other_arrow_point =
        get_point_at_index_global_coordinates(arrow, end.adjacent_index(), elements_map);
    let other_focus_or_arrow_point = if pts.len() == 2 {
        other_focus.unwrap_or(other_arrow_point)
    } else {
        other_arrow_point
    };
    let intersector = [focus, other_focus_or_arrow_point];
    let other_outline_point = other_bindable.and_then(|other| {
        nearest(
            intersect(other, elements_map, intersector, get_binding_gap(other)),
            focus,
        )
    });
    let outline_point = nearest(
        intersect(
            bindable,
            elements_map,
            intersector,
            get_binding_gap(bindable),
        ),
        other_focus_or_arrow_point,
    );
    let (start_has_arrowhead, end_has_arrowhead) =
        arrow.kind.linear().map_or((false, false), |l| {
            (l.start_arrowhead.is_some(), l.end_arrowhead.is_some())
        });
    let resolved_target = if (!start_has_arrowhead && !end_has_arrowhead)
        || (end == BindingEnd::Start && start_has_arrowhead)
        || (end == BindingEnd::End && end_has_arrowhead)
    {
        focus
    } else {
        outline_point.unwrap_or(focus)
    };

    // 1. Handle case when the outline point (or focus point) is inside
    // the other shape by short-circuiting to the focus point, otherwise
    // the arrow would invert
    if let (Some(other), Some(outline)) = (other_bindable, outline_point) {
        if !dragging
            // Arbitrary threshold to handle wireframing use cases
            && element_area(other) < element_area(bindable) * 2.0
            && hit(other, outline, elements_map, get_binding_gap(other), true)
        {
            return at(resolved_target[0], resolved_target[1]);
        }
    }

    let other_target_point = if other_bindable.is_some() {
        other_outline_point
            .or(other_focus)
            .unwrap_or(other_arrow_point)
    } else {
        other_arrow_point
    };
    let arrow_too_short =
        distance(other_target_point, outline_point.unwrap_or(focus)) <= BASE_ARROW_MIN_LENGTH;

    // 2. If the arrow is unconnected at the other end, just check arrow size
    // and short-circuit to the focus point if the arrow is too short to
    // avoid inversion
    if other_bindable.is_none() {
        let p = if arrow_too_short {
            focus
        } else {
            outline_point.unwrap_or(focus)
        };
        return at(p[0], p[1]);
    }

    // 3. If the arrow is too short while connected on both ends and
    // the other arrow endpoint will not be inside the bindable, just
    // check the arrow size and make a decision based on that
    if arrow_too_short {
        return at(
            or(resolved_target[0], focus[0]),
            or(resolved_target[1], focus[1]),
        );
    }

    // 4. In the general case, snap to the outline if possible
    let outline = outline_point.unwrap_or([f64::NAN, f64::NAN]);
    at(or(outline[0], focus[0]), or(outline[1], focus[1]))
}

/// `updateBoundElements(changedElement, scene, { simultaneouslyUpdated,
/// changedElements })` (`binding.ts:1321-1429`): after the bindable
/// element `changed_id` moved, resized or rotated, every arrow bound to it
/// (read from its `boundElements`) has its bound ends laid out again
/// ([`update_bound_point`], moved with `LinearElementEditor.movePoints`,
/// the middle points left in place when both ends are bound to the same
/// element) and its label re-wrapped. Arrows in `simultaneously_updated`
/// (moved with the element) are left alone.
///
/// `changed_elements` are the ids of the elements `ElementsDelta` passes
/// as `changedElements` after an undo or redo: upstream's scene elements
/// themselves, so the port reads them from the scene, but found by id even
/// when they are deleted.
pub fn update_bound_elements(
    scene: &mut Scene,
    env: &mut dyn BindingEnv,
    changed_id: &str,
    simultaneously_updated: Option<&[String]>,
    changed_elements: Option<&[String]>,
) {
    let Some(changed) = scene.get(changed_id) else {
        return;
    };
    if !changed.is_bindable() {
        return;
    }
    let simultaneous: HashSet<&str> = simultaneously_updated
        .unwrap_or(&[])
        .iter()
        .map(String::as_str)
        .collect();
    let extra: HashSet<&str> = changed_elements
        .unwrap_or(&[])
        .iter()
        .map(String::as_str)
        .collect();
    // create new instance so that possible mutations won't play a role in
    // visiting order
    let bound_ids: Vec<String> = changed
        .base
        .bound_elements
        .iter()
        .flatten()
        .map(|b| b.id.clone())
        .collect();

    for id in bound_ids {
        let (updates, same_element_both_ends) = {
            let map = lookup(scene, &extra);
            let Some(element) = map.get(&id) else {
                continue;
            };
            if !is_arrow(element) || element.base.is_deleted {
                continue;
            }
            // In case the boundElements are stale
            let start = binding_at(element, BindingEnd::Start);
            let end = binding_at(element, BindingEnd::End);
            if start.is_none_or(|b| b.element_id != changed_id)
                && end.is_none_or(|b| b.element_id != changed_id)
            {
                continue;
            }
            // Check for intersections before updating bound elements in case
            // connected elements overlap
            let start_element = start.and_then(|b| map.get(&b.element_id));
            let end_element = end.and_then(|b| map.get(&b.element_id));
            // `linearElement` is being moved/scaled already, just update the binding
            if simultaneous.contains(id.as_str()) {
                continue;
            }
            let same =
                start_element.is_some_and(|s| end_element.is_some_and(|e| s.base.id == e.base.id));
            let last = points(element).len().saturating_sub(1);
            let mut updates: Vec<(usize, PointUpdate)> = Vec::new();
            // bindableElementsVisitor: frameId, then the bindings
            for which in [BindingEnd::Start, BindingEnd::End] {
                let Some(binding) = binding_at(element, which) else {
                    continue;
                };
                let Some(bindable) = map.get(&binding.element_id) else {
                    continue;
                };
                let other = binding_at(element, which.opposite());
                if bindable.is_bindable()
                    && (binding.element_id == changed_id
                        || other.is_some_and(|o| o.element_id == changed_id))
                {
                    if let Some(point) =
                        update_bound_point(element, which, Some(binding), bindable, &map, false)
                    {
                        let index = match which {
                            BindingEnd::Start => 0,
                            BindingEnd::End => last,
                        };
                        updates.push((index, PointUpdate::to(point)));
                    }
                }
            }
            (updates, same)
        };

        move_points(
            scene,
            env,
            &id,
            &updates,
            MovePointsOtherUpdates {
                move_mid_points_with_element: Some(same_element_both_ends),
                ..MovePointsOtherUpdates::default()
            },
        );

        let has_label = {
            let map = lookup(scene, &extra);
            map.get(&id)
                .and_then(|arrow| get_bound_text_element(arrow, &map))
                .is_some_and(|text| !text.base.is_deleted)
        };
        if has_label {
            resize_bound_text(&id, scene, env, None, false, false, false);
        }
    }
}

/// [`update_bound_elements`] over the elements an undo or redo is building
/// (`ElementsDelta.redrawBoundArrows` on `new Scene(nextElements)`,
/// `delta.ts:2034-2123`), with `changed` passed as `changedElements`. The
/// default of [`crate::store::HistoryEnv::update_bound_elements`].
pub fn update_bound_elements_in_map(
    elements: &mut SceneElementsMap,
    element_id: &str,
    changed: &SceneElementsMap,
    env: &mut dyn BindingEnv,
) {
    let mut scene = Scene::new(elements.values().cloned().collect());
    let changed_ids: Vec<String> = changed.keys().cloned().collect();
    update_bound_elements(&mut scene, env, element_id, None, Some(&changed_ids));
    for element in scene.elements() {
        match elements.get_mut(&element.base.id) {
            Some(slot) if slot != element => *slot = element.clone(),
            _ => {}
        }
    }
}

// -- binding and unbinding -------------------------------------------------------------

/// `applyBinding(arrow, bindableElement, binding, startOrEnd, scene)`
/// (`binding.ts:1119-1139`): the binding on the arrow, and the arrow in the
/// target's `boundElements` (once).
fn apply_binding(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    arrow_id: &str,
    bindable_id: &str,
    binding: FixedPointBinding,
    end: BindingEnd,
) {
    let update = match end {
        BindingEnd::Start => ElementUpdate {
            start_binding: Some(Some(binding)),
            ..ElementUpdate::default()
        },
        BindingEnd::End => ElementUpdate {
            end_binding: Some(Some(binding)),
            ..ElementUpdate::default()
        },
    };
    scene.mutate_element(arrow_id, update, env);
    let Some(bindable) = scene.get(bindable_id) else {
        return;
    };
    let current = bindable.base.bound_elements.clone().unwrap_or_default();
    if !current.iter().any(|b| b.id == arrow_id) {
        let mut next = current;
        next.push(BoundElement {
            id: arrow_id.to_owned(),
            kind: BoundElementType::Arrow,
        });
        scene.mutate_element(
            bindable_id,
            ElementUpdate {
                bound_elements: Some(Some(next)),
                ..ElementUpdate::default()
            },
            env,
        );
    }
}

/// `bindBindingElement(arrow, hoveredElement, mode, startOrEnd, scene,
/// zoom, focusPoint, shouldSnapToOutline, isMidpointSnappingEnabled)`
/// (`binding.ts:1141-1185`): binds the arrow's end to `hovered_id`. An
/// elbow arrow always orbits, at its end snapped to the outline; any other
/// arrow binds in `mode` at the focus point (or its end).
#[allow(clippy::too_many_arguments)]
pub fn bind_binding_element(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    arrow_id: &str,
    hovered_id: &str,
    mode: BindMode,
    end: BindingEnd,
    zoom: f64,
    focus_point: Option<P>,
    should_snap_to_outline: bool,
    is_midpoint_snapping_enabled: bool,
) {
    let binding = {
        let map = scene.elements_map();
        let (Some(arrow), Some(hovered)) = (map.get(arrow_id), map.get(hovered_id)) else {
            return;
        };
        if is_elbow_arrow(arrow) {
            FixedPointBinding {
                element_id: hovered_id.to_owned(),
                mode: BindMode::Orbit,
                fixed_point: calculate_fixed_point_for_elbow_arrow_binding(
                    arrow,
                    hovered,
                    end,
                    &map,
                    zoom,
                    should_snap_to_outline,
                    is_midpoint_snapping_enabled,
                ),
            }
        } else {
            FixedPointBinding {
                element_id: hovered_id.to_owned(),
                mode,
                fixed_point: calculate_fixed_point_for_non_elbow_arrow_binding(
                    arrow,
                    hovered,
                    end,
                    &map,
                    focus_point,
                ),
            }
        }
    };
    apply_binding(scene, env, arrow_id, hovered_id, binding, end);
}

/// `bindBindingElementToFixedPoint(arrow, bindableElement, startOrEnd,
/// fixedPoint, scene)` (`binding.ts:3216-3234`): binds the end, orbiting,
/// at the given (normalised) fixed point.
pub fn bind_binding_element_to_fixed_point(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    arrow_id: &str,
    bindable_id: &str,
    end: BindingEnd,
    fixed_point: [f64; 2],
) {
    let binding = FixedPointBinding {
        element_id: bindable_id.to_owned(),
        fixed_point: normalize_fixed_point(fixed_point),
        mode: BindMode::Orbit,
    };
    apply_binding(scene, env, arrow_id, bindable_id, binding, end);
}

/// `unbindBindingElement(arrow, startOrEnd, scene)` (`binding.ts:1187-1217`):
/// drops the arrow's binding at that end and, unless the other end is
/// bound to the same element, the arrow from that element's
/// `boundElements`. Returns the element it was bound to.
pub fn unbind_binding_element(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    arrow_id: &str,
    end: BindingEnd,
) -> Option<String> {
    let arrow = scene.get(arrow_id)?;
    let binding = binding_at(arrow, end)?.element_id.clone();
    let opposite = binding_at(arrow, end.opposite());
    if opposite.is_none_or(|o| o.element_id != binding) {
        // Only remove the record on the bound element if the other end is
        // not bound to the same element
        if let Some(bound) = scene.get_non_deleted(&binding) {
            let remaining = bound.base.bound_elements.as_ref().map(|list| {
                list.iter()
                    .filter(|b| b.id != arrow_id)
                    .cloned()
                    .collect::<Vec<_>>()
            });
            scene.mutate_element(
                &binding,
                ElementUpdate {
                    // `boundElements?.filter(...)`: undefined for no list
                    bound_elements: remaining.map(Some),
                    ..ElementUpdate::default()
                },
                env,
            );
        }
    }
    let update = match end {
        BindingEnd::Start => ElementUpdate {
            start_binding: Some(None),
            ..ElementUpdate::default()
        },
        BindingEnd::End => ElementUpdate {
            end_binding: Some(None),
            ..ElementUpdate::default()
        },
    };
    scene.mutate_element(arrow_id, update, env);
    Some(binding)
}

// -- binding strategies ----------------------------------------------------------------

/// `BindingStrategy` (`binding.ts:92-111`): what to do with an end.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingStrategy {
    /// Keep the existing binding (`mode: undefined`).
    Keep,
    /// Break the binding (`mode: null`).
    Unbind,
    /// Bind to `element` in `mode`, at `focus_point`.
    Bind {
        mode: BindMode,
        element: String,
        focus_point: P,
    },
}

impl BindingStrategy {
    fn bind(mode: BindMode, element: &Element, focus_point: P) -> BindingStrategy {
        BindingStrategy::Bind {
            mode,
            element: element.base.id.clone(),
            focus_point,
        }
    }

    /// The focus point, when binding.
    pub fn focus_point(&self) -> Option<P> {
        match self {
            BindingStrategy::Bind { focus_point, .. } => Some(*focus_point),
            _ => None,
        }
    }

    /// The element to bind to, when binding.
    pub fn element(&self) -> Option<&str> {
        match self {
            BindingStrategy::Bind { element, .. } => Some(element),
            _ => None,
        }
    }
}

/// The `LinearElementEditor.initialState` fields the binding strategies
/// read (`appState.selectedLinearElement.initialState`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LinearElementInitialState {
    /// Where the pointer went down.
    pub origin: Option<P>,
    /// The new arrow started inside a shape.
    pub arrow_start_is_inside: bool,
    /// The other end's focus point, fixed once per drag.
    pub alt_focus_point: Option<P>,
    /// The mode of the other end's binding when the drag began.
    pub arrow_other_endpoint_initial_binding: Option<BindMode>,
}

/// The app state the binding strategies read.
#[derive(Debug, Clone, PartialEq)]
pub struct BindingAppState {
    /// `zoom.value`.
    pub zoom: f64,
    /// `isBindingEnabled` (Ctrl/Cmd held while dragging turns it off).
    pub is_binding_enabled: bool,
    pub is_midpoint_snapping_enabled: bool,
    pub grid_mode_enabled: bool,
    /// `gridSize`.
    pub grid_size: Option<f64>,
    /// `bindMode`: "orbit", or "inside" / "skip" while the free binding
    /// mode is on.
    pub bind_mode: BindMode,
    /// `selectedLinearElement?.initialState`.
    pub selected_linear_element: Option<LinearElementInitialState>,
    /// `getFeatureFlag("COMPLEX_BINDINGS")` (off by default).
    pub complex_bindings: bool,
}

impl Default for BindingAppState {
    /// Upstream's defaults (`getDefaultAppState`): zoom 1, binding and
    /// midpoint snapping on, no grid mode, grid size 20, orbit.
    fn default() -> BindingAppState {
        BindingAppState {
            zoom: 1.0,
            is_binding_enabled: true,
            is_midpoint_snapping_enabled: true,
            grid_mode_enabled: false,
            grid_size: Some(20.0),
            bind_mode: BindMode::Orbit,
            selected_linear_element: None,
            complex_bindings: false,
        }
    }
}

/// The options of the binding strategies (`opts`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BindingOpts {
    pub new_arrow: bool,
    pub angle_locked: bool,
    pub alt_key: bool,
    pub finalize: bool,
    pub initial_binding: bool,
    pub grid_size: Option<f64>,
    pub shift_key: bool,
}

/// `PointsPositionUpdates` as a list in insertion order: the last entry of
/// an index is the one `Map.get` answers.
fn dragged(dragging_points: &[(usize, P)], index: usize) -> Option<P> {
    dragging_points
        .iter()
        .rev()
        .find(|(i, _)| *i == index)
        .map(|(_, p)| *p)
}

/// The number of distinct indices (`Map.size`).
fn dragged_count(dragging_points: &[(usize, P)]) -> usize {
    dragging_points
        .iter()
        .map(|(i, _)| *i)
        .collect::<HashSet<_>>()
        .len()
}

/// `getBindingStrategyForDraggingBindingElementEndpoints(arrow,
/// draggingPoints, screenPointerX, screenPointerY, elementsMap, elements,
/// appState, opts)` (`binding.ts:614-652`): what to do with each end of an
/// arrow whose `dragging_points` (point index and new local point) are
/// being dragged, the pointer at `scene_pointer`. `elements` are the
/// scene's non-deleted elements in z-order.
pub fn get_binding_strategy_for_dragging_binding_element_endpoints(
    arrow: &Element,
    dragging_points: &[(usize, P)],
    scene_pointer: P,
    elements_map: &ElementsMap<'_>,
    elements: &[&Element],
    app_state: &BindingAppState,
    opts: &BindingOpts,
) -> Result<(BindingStrategy, BindingStrategy), BindingError> {
    if app_state.complex_bindings {
        return strategy_complex(
            arrow,
            dragging_points,
            elements_map,
            elements,
            app_state,
            opts,
        );
    }
    strategy_simple(
        arrow,
        dragging_points,
        scene_pointer,
        elements_map,
        elements,
        app_state,
        opts,
    )
}

/// `bindingStrategyForElbowArrowEndpointDragging` (`binding.ts:267-317`):
/// the dragged end orbits the element under it, or unbinds.
fn strategy_for_elbow_arrow(
    arrow: &Element,
    dragging_points: &[(usize, P)],
    elements_map: &ElementsMap<'_>,
    elements: &[&Element],
    zoom: f64,
) -> Result<(BindingStrategy, BindingStrategy), BindingError> {
    invariant(
        dragged_count(dragging_points) == 1,
        "Bound elbow arrows cannot be moved",
    )?;
    let &(point_idx, point) = dragging_points.first().ok_or_else(|| {
        BindingError(
            "There should be a position update for dragging an elbow arrow endpoint".to_owned(),
        )
    })?;
    let global_point = get_point_global_coordinates(arrow, point, elements_map);
    let current = match get_hovered_element_for_binding(global_point, elements, elements_map, zoom)
    {
        Some(hit) => BindingStrategy::bind(
            BindMode::Orbit,
            hit,
            get_point_at_index_global_coordinates(arrow, point_idx as isize, elements_map),
        ),
        None => BindingStrategy::Unbind,
    };
    Ok(if point_idx == 0 {
        (current, BindingStrategy::Keep)
    } else {
        (BindingStrategy::Keep, current)
    })
}

/// `getBindingStrategyForDraggingBindingElementEndpoints_simple`
/// (`binding.ts:654-962`).
fn strategy_simple(
    arrow: &Element,
    dragging_points: &[(usize, P)],
    scene_pointer: P,
    elements_map: &ElementsMap<'_>,
    elements: &[&Element],
    app_state: &BindingAppState,
    opts: &BindingOpts,
) -> Result<(BindingStrategy, BindingStrategy), BindingError> {
    use BindingStrategy::{Keep, Unbind};
    let pts = points(arrow);
    if pts.len() < 2 {
        // a single-point can't be bound -> cancel
        return Ok((Keep, Keep));
    }
    let end_idx = pts.len() - 1;
    let start_dragged = dragged(dragging_points, 0).is_some();
    let end_dragged = dragged(dragging_points, end_idx).is_some();

    // If none of the ends are dragged, we don't change anything
    if !start_dragged && !end_dragged {
        return Ok((Keep, Keep));
    }
    // If both ends are dragged, we don't bind to anything and break
    // existing bindings
    if start_dragged && end_dragged {
        return Ok((Unbind, Unbind));
    }
    // If binding is disabled and an endpoint is dragged, we actively break
    // the end binding
    if !app_state.is_binding_enabled {
        return Ok((
            if start_dragged { Unbind } else { Keep },
            if end_dragged { Unbind } else { Keep },
        ));
    }
    // Handle simpler elbow arrow binding
    if is_elbow_arrow(arrow) {
        return strategy_for_elbow_arrow(
            arrow,
            dragging_points,
            elements_map,
            elements,
            app_state.zoom,
        );
    }

    let dragged_end = if start_dragged {
        BindingEnd::Start
    } else {
        BindingEnd::End
    };
    let other_binding = binding_at(arrow, dragged_end.opposite());
    let local_point = dragged(dragging_points, if start_dragged { 0 } else { end_idx });
    let local_point = local_point.ok_or_else(|| {
        BindingError(format!(
            "Local point must be defined for {} dragging",
            if start_dragged { "start" } else { "end" }
        ))
    })?;
    let global_point = get_point_global_coordinates(arrow, local_point, elements_map);
    let hit_point = if opts.angle_locked || app_state.grid_mode_enabled {
        scene_pointer
    } else {
        global_point
    };
    let hovered =
        get_hovered_element_for_binding(hit_point, elements, elements_map, app_state.zoom);
    let point_in_element = hovered.is_some_and(|h| {
        is_point_in_element(
            if opts.angle_locked {
                scene_pointer
            } else {
                global_point
            },
            h,
            elements_map,
        )
    });
    let other_bindable = other_binding.and_then(|b| elements_map.get(&b.element_id));
    let other_focus_point = other_binding.and_then(|b| {
        other_bindable.map(|e| get_global_fixed_point_for_bindable_element(b.fixed_point, e))
    });
    let other_focus_point_is_in_element = match (other_bindable, other_focus_point) {
        (Some(e), Some(p)) => hit(e, p, elements_map, 0.0, true),
        _ => false,
    };
    let initial = app_state.selected_linear_element.as_ref();
    let other_endpoint_initial_binding =
        initial.and_then(|s| s.arrow_other_endpoint_initial_binding);

    // Handle outside-outside binding to the same element
    if let (Some(other), Some(hit)) = (other_binding, hovered) {
        if other.element_id == hit.base.id
            && (!opts.new_arrow || initial.and_then(|s| s.origin).is_some())
        {
            let other_fixed = other_bindable
                .map(|e| get_global_fixed_point_for_bindable_element(other.fixed_point, e));
            let start_focus = if start_dragged {
                global_point
            } else if opts.new_arrow {
                // NOTE: Can only affect the start point because new arrows
                // always drag the end point
                let origin = initial
                    .and_then(|s| s.origin)
                    .unwrap_or([f64::NAN, f64::NAN]);
                get_grid_point(origin[0], origin[1], opts.grid_size)
            } else {
                other_fixed.unwrap_or_else(|| {
                    get_point_at_index_global_coordinates(arrow, 0, elements_map)
                })
            };
            let end_focus = if end_dragged {
                global_point
            } else {
                other_fixed.unwrap_or_else(|| {
                    get_point_at_index_global_coordinates(arrow, -1, elements_map)
                })
            };
            return Ok((
                BindingStrategy::bind(BindMode::Inside, hit, start_focus),
                BindingStrategy::bind(BindMode::Inside, hit, end_focus),
            ));
        }
    }

    // Handle special alt key case to inside bind no matter what
    if opts.alt_key {
        let inside = || match hovered {
            Some(hit) => BindingStrategy::bind(BindMode::Inside, hit, global_point),
            None => Unbind,
        };
        return Ok((
            if start_dragged { inside() } else { Keep },
            if end_dragged { inside() } else { Keep },
        ));
    }

    // Handle normal cases
    let current = match hovered {
        Some(hit) if point_in_element => BindingStrategy::bind(BindMode::Inside, hit, global_point),
        Some(hit) => {
            let projected_from = if opts.angle_locked {
                global_point
            } else if app_state.grid_mode_enabled {
                snap_bound_point_to_grid(
                    scene_pointer,
                    hit,
                    elements_map,
                    app_state.grid_size,
                    arrow,
                    Some(get_point_at_index_global_coordinates(
                        arrow,
                        if start_dragged { 1 } else { -2 },
                        elements_map,
                    )),
                )
            } else {
                global_point
            };
            let focus = project_fixed_point_onto_diagonal(
                arrow,
                projected_from,
                hit,
                dragged_end,
                elements_map,
                app_state.zoom,
                app_state.is_midpoint_snapping_enabled
                    && !opts.angle_locked
                    && !app_state.grid_mode_enabled,
            )
            .unwrap_or(global_point);
            BindingStrategy::bind(BindMode::Orbit, hit, focus)
        }
        None => Unbind,
    };

    let other_endpoint = get_point_at_index_global_coordinates(
        arrow,
        if start_dragged { -1 } else { 0 },
        elements_map,
    );
    let point_is_close_to_other_element = match (other_focus_point, other_bindable) {
        (Some(_), Some(e)) => hit(
            e,
            global_point,
            elements_map,
            max_binding_distance_simple(app_state.zoom),
            true,
        ),
        _ => false,
    };
    let other_point_was_inside_at_start = other_endpoint_initial_binding == Some(BindMode::Inside);
    let other_never_override = if opts.new_arrow {
        initial.is_some_and(|s| s.arrow_start_is_inside)
    } else {
        other_binding.is_some_and(|b| b.mode == BindMode::Inside) && other_point_was_inside_at_start
    };

    let alt_focus_point = initial.and_then(|s| s.alt_focus_point);
    let other = match other_bindable {
        _ if other_never_override => Keep,
        Some(e)
            if other_binding.is_some_and(|b| b.mode == BindMode::Inside)
                && !other_point_was_inside_at_start
                && !opts.new_arrow =>
        {
            BindingStrategy::bind(
                BindMode::Orbit,
                e,
                other_focus_point.unwrap_or(other_endpoint),
            )
        }
        Some(e)
            if !other_focus_point_is_in_element
                && !point_is_close_to_other_element
                && alt_focus_point.is_some() =>
        {
            BindingStrategy::bind(BindMode::Orbit, e, alt_focus_point.expect("checked"))
        }
        Some(e) if opts.angle_locked => {
            let focus = project_fixed_point_onto_diagonal(
                arrow,
                other_endpoint,
                e,
                dragged_end.opposite(),
                elements_map,
                app_state.zoom,
                app_state.is_midpoint_snapping_enabled,
            )
            .unwrap_or(other_endpoint);
            BindingStrategy::bind(BindMode::Orbit, e, focus)
        }
        _ => Keep,
    };

    Ok(if start_dragged {
        (current, other)
    } else {
        (other, current)
    })
}

/// `getBindingStrategyForDraggingBindingElementEndpoints_complex`
/// (`binding.ts:964-1096`).
fn strategy_complex(
    arrow: &Element,
    dragging_points: &[(usize, P)],
    elements_map: &ElementsMap<'_>,
    elements: &[&Element],
    app_state: &BindingAppState,
    opts: &BindingOpts,
) -> Result<(BindingStrategy, BindingStrategy), BindingError> {
    use BindingStrategy::{Keep, Unbind};
    let global_bind_mode = app_state.bind_mode;
    let pts = points(arrow);
    if pts.len() < 2 {
        return Ok((Keep, Keep));
    }
    let end_idx = pts.len() - 1;
    let start_dragged = dragged(dragging_points, 0).is_some();
    let end_dragged = dragged(dragging_points, end_idx).is_some();

    if !start_dragged && !end_dragged {
        return Ok((Keep, Keep));
    }
    if start_dragged && end_dragged {
        return Ok((Unbind, Unbind));
    }
    if !app_state.is_binding_enabled {
        return Ok((
            if start_dragged { Unbind } else { Keep },
            if end_dragged { Unbind } else { Keep },
        ));
    }
    if is_elbow_arrow(arrow) {
        return strategy_for_elbow_arrow(
            arrow,
            dragging_points,
            elements_map,
            elements,
            app_state.zoom,
        );
    }
    // Handle new arrow creation separately, as it is special
    if opts.new_arrow {
        return strategy_for_new_simple_arrow(
            arrow,
            dragging_points,
            elements_map,
            elements,
            start_dragged,
            end_dragged,
            end_idx,
            app_state,
            global_bind_mode,
            opts.shift_key,
        );
    }
    let (dragged_end, idx) = if start_dragged {
        (BindingEnd::Start, 0)
    } else {
        (BindingEnd::End, end_idx)
    };
    let local_point = dragged(dragging_points, idx).ok_or_else(|| {
        BindingError(format!(
            "Local point must be defined for {} dragging",
            if start_dragged { "start" } else { "end" }
        ))
    })?;
    let global_point = get_point_global_coordinates(arrow, local_point, elements_map);
    let (current, other) = strategy_for_simple_arrow_endpoint_complex(
        global_point,
        binding_at(arrow, dragged_end),
        binding_at(arrow, dragged_end.opposite()),
        elements_map,
        elements,
        global_bind_mode,
        arrow,
        app_state.zoom,
        opts.finalize,
    );
    Ok(if start_dragged {
        (current, other)
    } else {
        (other, current)
    })
}

/// `bindingStrategyForNewSimpleArrowEndpointDragging` (`binding.ts:319-485`).
#[allow(clippy::too_many_arguments)]
fn strategy_for_new_simple_arrow(
    arrow: &Element,
    dragging_points: &[(usize, P)],
    elements_map: &ElementsMap<'_>,
    elements: &[&Element],
    start_dragged: bool,
    end_dragged: bool,
    end_idx: usize,
    app_state: &BindingAppState,
    global_bind_mode: BindMode,
    shift_key: bool,
) -> Result<(BindingStrategy, BindingStrategy), BindingError> {
    use BindingStrategy::{Keep, Unbind};
    let is_multi_point = points(arrow).len() > 2;
    let local = dragged(dragging_points, if start_dragged { 0 } else { end_idx })
        .unwrap_or([f64::NAN, f64::NAN]);
    let point = get_point_global_coordinates(arrow, local, elements_map);
    let hovered = get_hovered_element_for_binding(point, elements, elements_map, app_state.zoom);
    let start_binding = binding_at(arrow, BindingEnd::Start);
    let is_inside_mode = matches!(global_bind_mode, BindMode::Inside | BindMode::Skip);

    // With new arrows this handles the binding at arrow creation
    if start_dragged {
        let start = match hovered {
            Some(hit) => BindingStrategy::bind(BindMode::Inside, hit, point),
            None => Unbind,
        };
        return Ok((start, Keep));
    }

    // With new arrows it represents the continuous dragging of the end point
    if end_dragged {
        let initial = app_state.selected_linear_element.as_ref();
        let origin = initial.and_then(|s| s.origin);
        let arrow_origin = [arrow.base.x, arrow.base.y];

        // Inside -> inside binding
        if let (Some(hit), Some(sb)) = (hovered, start_binding) {
            if sb.element_id == hit.base.id {
                let center = [
                    hit.base.x + hit.base.width / 2.0,
                    hit.base.y + hit.base.height / 2.0,
                ];
                return Ok((
                    if is_multi_point {
                        Keep
                    } else {
                        BindingStrategy::bind(BindMode::Inside, hit, origin.unwrap_or(center))
                    },
                    if is_multi_point {
                        BindingStrategy::bind(BindMode::Orbit, hit, point)
                    } else {
                        BindingStrategy::bind(BindMode::Inside, hit, point)
                    },
                ));
            }
        }

        // Check and handle nested shapes
        if let (Some(hit), Some(sb)) = (hovered, start_binding) {
            let all_hits =
                get_all_hovered_element_at_point(point, elements, elements_map, app_state.zoom);
            if all_hits.iter().any(|e| e.base.id == sb.element_id) {
                let other = elements_map.get(&sb.element_id);
                invariant(other.is_some(), "Other element must be in the elements map")?;
                let other = other.expect("checked");
                return Ok((
                    if is_multi_point {
                        Keep
                    } else {
                        BindingStrategy::bind(
                            if other.base.id != hit.base.id {
                                BindMode::Orbit
                            } else {
                                BindMode::Inside
                            },
                            other,
                            origin.unwrap_or(arrow_origin),
                        )
                    },
                    BindingStrategy::bind(BindMode::Orbit, hit, point),
                ));
            }
        }

        // Inside -> outside binding
        if let Some(sb) = start_binding {
            if hovered.is_none_or(|h| h.base.id != sb.element_id) {
                let other = elements_map.get(&sb.element_id);
                invariant(other.is_some(), "Other element must be in the elements map")?;
                let other = other.expect("checked");
                let other_is_inside = initial.is_some_and(|s| s.arrow_start_is_inside);
                let other_strategy = BindingStrategy::bind(
                    if other_is_inside {
                        BindMode::Inside
                    } else {
                        BindMode::Orbit
                    },
                    other,
                    if shift_key {
                        element_center_point(other, elements_map)
                    } else {
                        origin.unwrap_or(arrow_origin)
                    },
                );
                // We are hovering another element with the end point
                let current = match hovered {
                    Some(hit) => {
                        let is_nested =
                            is_bindable_element_inside_other_bindable(other, hit, elements_map);
                        BindingStrategy::bind(
                            if is_inside_mode && !is_nested {
                                BindMode::Inside
                            } else {
                                BindMode::Orbit
                            },
                            hit,
                            point,
                        )
                    }
                    None => Unbind,
                };
                return Ok((if is_multi_point { Keep } else { other_strategy }, current));
            }
        }

        // No start binding
        if start_binding.is_none() {
            let end = match hovered {
                Some(hit) => BindingStrategy::bind(
                    if is_inside_mode {
                        BindMode::Inside
                    } else {
                        BindMode::Orbit
                    },
                    hit,
                    point,
                ),
                None => Unbind,
            };
            return Ok((Keep, end));
        }
    }

    Err(BindingError(
        "New arrow creation should not reach here".to_owned(),
    ))
}

/// `bindingStrategyForSimpleArrowEndpointDragging_complex`
/// (`binding.ts:487-612`): the dragged end's strategy (`current`) and the
/// other end's.
#[allow(clippy::too_many_arguments)]
fn strategy_for_simple_arrow_endpoint_complex(
    point: P,
    current_binding: Option<&FixedPointBinding>,
    opposite_binding: Option<&FixedPointBinding>,
    elements_map: &ElementsMap<'_>,
    elements: &[&Element],
    global_bind_mode: BindMode,
    arrow: &Element,
    zoom: f64,
    finalize: bool,
) -> (BindingStrategy, BindingStrategy) {
    use BindingStrategy::{Keep, Unbind};
    let is_multi_point = points(arrow).len() > 2;
    let hovered = get_hovered_element_for_binding(point, elements, elements_map, zoom);
    let is_overlapping = opposite_binding.is_some_and(|o| {
        get_all_hovered_element_at_point(point, elements, elements_map, zoom)
            .iter()
            .any(|e| e.base.id == o.element_id)
    });
    let opposite_element = opposite_binding.and_then(|o| elements_map.get(&o.element_id));
    let other_is_transparent = match opposite_element {
        Some(e) if is_overlapping => is_transparent(&e.base.background_color),
        _ => false,
    };

    // If the global bind mode is in free binding mode, just bind where the
    // pointer is and keep the other end intact
    if matches!(global_bind_mode, BindMode::Inside | BindMode::Skip) {
        let current = match hovered {
            Some(hit) => {
                let element = match opposite_element {
                    Some(o) if is_overlapping && !other_is_transparent => o,
                    _ => hit,
                };
                BindingStrategy::bind(BindMode::Inside, element, point)
            }
            None => Unbind,
        };
        let other = match (hovered, opposite_binding) {
            (Some(hit), Some(o)) if finalize && hit.base.id == o.element_id => Unbind,
            _ => Keep,
        };
        return (current, other);
    }

    // Dragged point is outside of any bindable element so we break any
    // existing binding
    let Some(hit) = hovered else {
        return (Unbind, Keep);
    };

    // Already inside binding over the same hit element should remain inside
    // bound
    if current_binding.is_some_and(|c| c.element_id == hit.base.id && c.mode == BindMode::Inside) {
        return (BindingStrategy::bind(BindMode::Inside, hit, point), Keep);
    }

    let keep_if_multi = |other: BindingStrategy| if is_multi_point { Keep } else { other };

    // The dragged point is inside the hovered bindable element
    if let Some(opposite) = opposite_binding {
        if opposite.element_id == hit.base.id {
            // The opposite binding is on the binding gap of the same element
            if opposite.mode == BindMode::Orbit {
                let other = if finalize { Unbind } else { Keep };
                return (
                    BindingStrategy::bind(BindMode::Orbit, hit, point),
                    keep_if_multi(other),
                );
            }
            // The opposite binding is inside the same element
            return (
                BindingStrategy::bind(BindMode::Inside, hit, point),
                keep_if_multi(Keep),
            );
        }
        // The opposite binding is on a different element (or nested)
        let current = match opposite_element {
            Some(o) if is_overlapping && !other_is_transparent => {
                BindingStrategy::bind(BindMode::Inside, o, point)
            }
            _ => BindingStrategy::bind(BindMode::Orbit, hit, point),
        };
        return (current, keep_if_multi(Keep));
    }

    // The opposite binding is on a different element or no binding
    (
        BindingStrategy::bind(BindMode::Orbit, hit, point),
        keep_if_multi(Keep),
    )
}

/// `bindOrUnbindBindingElementEdge` (`binding.ts:240-265`).
#[allow(clippy::too_many_arguments)]
fn bind_or_unbind_edge(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    arrow_id: &str,
    strategy: &BindingStrategy,
    end: BindingEnd,
    zoom: f64,
    should_snap_to_outline: bool,
    is_midpoint_snapping_enabled: bool,
) {
    match strategy {
        BindingStrategy::Unbind => {
            unbind_binding_element(scene, env, arrow_id, end);
        }
        BindingStrategy::Bind {
            mode,
            element,
            focus_point,
        } => bind_binding_element(
            scene,
            env,
            arrow_id,
            element,
            *mode,
            end,
            zoom,
            Some(*focus_point),
            should_snap_to_outline,
            is_midpoint_snapping_enabled,
        ),
        BindingStrategy::Keep => {}
    }
}

/// `bindOrUnbindBindingElement(arrow, draggingPoints, scenePointerX,
/// scenePointerY, scene, appState, opts)` (`binding.ts:151-238`): applies
/// the binding strategy for the dragged ends (finalised), then moves each
/// end bound with a focus point to where its binding puts it.
pub fn bind_or_unbind_binding_element(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    arrow_id: &str,
    dragging_points: &[(usize, P)],
    scene_pointer: P,
    app_state: &BindingAppState,
    opts: &BindingOpts,
) -> Result<(BindingStrategy, BindingStrategy), BindingError> {
    let (start, end) = {
        let Some(arrow) = scene.get(arrow_id) else {
            return Ok((BindingStrategy::Keep, BindingStrategy::Keep));
        };
        let map = scene.elements_map();
        let elements = scene.non_deleted();
        get_binding_strategy_for_dragging_binding_element_endpoints(
            arrow,
            dragging_points,
            scene_pointer,
            &map,
            &elements,
            app_state,
            &BindingOpts {
                finalize: true,
                ..opts.clone()
            },
        )?
    };
    let is_midpoint_snapping_enabled = app_state.is_midpoint_snapping_enabled
        && !opts.angle_locked
        && !app_state.grid_mode_enabled;
    for (strategy, which) in [(&start, BindingEnd::Start), (&end, BindingEnd::End)] {
        bind_or_unbind_edge(
            scene,
            env,
            arrow_id,
            strategy,
            which,
            app_state.zoom,
            app_state.is_binding_enabled,
            is_midpoint_snapping_enabled,
        );
    }
    if start.focus_point().is_some() || end.focus_point().is_some() {
        // If the strategy dictates a focus point override, then update the
        // arrow points to point to the focus point.
        let updates = {
            let map = scene.elements_map();
            let Some(arrow) = map.get(arrow_id) else {
                return Ok((start, end));
            };
            let pts = points(arrow);
            let mut updates = Vec::new();
            for (strategy, which, index) in [
                (&start, BindingEnd::Start, 0),
                (&end, BindingEnd::End, pts.len().saturating_sub(1)),
            ] {
                let (Some(_), Some(element)) = (strategy.focus_point(), strategy.element()) else {
                    continue;
                };
                let fallback = pts.get(index).copied().unwrap_or([f64::NAN, f64::NAN]);
                let point = map
                    .get(element)
                    .and_then(|bindable| {
                        update_bound_point(
                            arrow,
                            which,
                            binding_at(arrow, which),
                            bindable,
                            &map,
                            false,
                        )
                    })
                    .unwrap_or(fallback);
                updates.push((index, PointUpdate::to(point)));
            }
            updates
        };
        move_points(
            scene,
            env,
            arrow_id,
            &updates,
            MovePointsOtherUpdates::default(),
        );
    }
    Ok((start, end))
}

/// `bindOrUnbindBindingElements(selectedArrows, scene, appState)`
/// (`binding.ts:1098-1117`): [`bind_or_unbind_binding_element`] for each
/// arrow with no dragged points (the pointer at infinity), which keeps
/// every binding.
pub fn bind_or_unbind_binding_elements(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    arrow_ids: &[String],
    app_state: &BindingAppState,
) {
    for id in arrow_ids {
        // with no dragged points neither strategy can fail
        let _ = bind_or_unbind_binding_element(
            scene,
            env,
            id,
            &[],
            [f64::INFINITY, f64::INFINITY],
            app_state,
            &BindingOpts::default(),
        );
    }
}

/// `updateArrowBindings(latestElement, startOrEnd, elementsMap, scene,
/// appState)` (`binding.ts:1431-1494`): unbinds the end, and binds it
/// again to the same element when the end is still within the binding
/// distance and the simple strategy picks that element.
fn update_arrow_bindings(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    arrow_id: &str,
    end: BindingEnd,
    app_state: &BindingAppState,
) -> Result<(), BindingError> {
    let (bindable_id, point, hit_now) = {
        let map = scene.elements_map();
        let Some(arrow) = map.get(arrow_id) else {
            return Ok(());
        };
        invariant(
            !is_elbow_arrow(arrow),
            "Elbow arrows not supported for indirect updates",
        )?;
        let bindable = binding_at(arrow, end).and_then(|b| map.get(&b.element_id));
        let point = get_point_at_index_global_coordinates(arrow, end.index(), &map);
        let hit_now = bindable.is_some_and(|b| {
            hit(
                b,
                point,
                &map,
                max_binding_distance_simple(app_state.zoom),
                false,
            )
        });
        (bindable.map(|b| b.base.id.clone()), point, hit_now)
    };
    unbind_binding_element(scene, env, arrow_id, end);
    let (Some(bindable_id), true) = (bindable_id, hit_now) else {
        return Ok(());
    };
    let strategy = {
        let map = scene.elements_map();
        let Some(arrow) = map.get(arrow_id) else {
            return Ok(());
        };
        let pts = points(arrow);
        let point_idx = match end {
            BindingEnd::Start => 0,
            BindingEnd::End => pts.len().saturating_sub(1),
        };
        let local = pts.get(point_idx).copied().unwrap_or([f64::NAN, f64::NAN]);
        let elements = scene.non_deleted();
        let (start, finish) = strategy_simple(
            arrow,
            &[(point_idx, local)],
            point,
            &map,
            &elements,
            app_state,
            &BindingOpts::default(),
        )?;
        match end {
            BindingEnd::Start => start,
            BindingEnd::End => finish,
        }
    };
    if let BindingStrategy::Bind {
        mode,
        element,
        focus_point,
    } = strategy
    {
        if element == bindable_id {
            bind_binding_element(
                scene,
                env,
                arrow_id,
                &bindable_id,
                mode,
                end,
                app_state.zoom,
                Some(focus_point),
                true,
                true,
            );
        }
    }
    Ok(())
}

/// `updateBindings(latestElement, scene, appState, options)`
/// (`binding.ts:1496-1533`): an arrow's ends re-bound where they are
/// ([`update_arrow_bindings`]; elbow arrows are refused), or the arrows
/// bound to a bindable element laid out again ([`update_bound_elements`]).
pub fn update_bindings(
    scene: &mut Scene,
    env: &mut dyn BindingEnv,
    element_id: &str,
    app_state: &BindingAppState,
    simultaneously_updated: Option<&[String]>,
) -> Result<(), BindingError> {
    let Some(element) = scene.get(element_id) else {
        return Ok(());
    };
    if is_arrow(element) {
        for end in [BindingEnd::Start, BindingEnd::End] {
            let bound = scene
                .get(element_id)
                .is_some_and(|e| binding_at(e, end).is_some());
            if bound {
                update_arrow_bindings(scene, env, element_id, end, app_state)?;
            }
        }
        return Ok(());
    }
    update_bound_elements(
        scene,
        env,
        element_id,
        simultaneously_updated,
        Some(&[element_id.to_owned()]),
    );
    Ok(())
}

/// `reanchorBindingsToOutline(changedElement, scene, zoom)`
/// (`binding.ts:1219-1319`): the arrows bound to the element whose fixed
/// point is no longer within the gap of its outline get a fixed point on
/// the outline, where the line from the centre through the old one crosses
/// it (an elbow arrow's end is snapped again).
pub fn reanchor_bindings_to_outline(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    element_id: &str,
    zoom: f64,
) {
    let Some(changed) = scene.get(element_id).cloned() else {
        return;
    };
    let center = element_center_point(&changed, &scene.elements_map());
    let reach = js::max(changed.base.width, changed.base.height) * 2.0;
    for id in bound_ids(&changed) {
        let update = {
            let map = scene.elements_map();
            let Some(element) = map.get(&id) else {
                continue;
            };
            if !is_arrow(element) {
                continue;
            }
            let changed = &changed;
            let mut update = ElementUpdate::default();
            for which in [BindingEnd::Start, BindingEnd::End] {
                let Some(binding) = binding_at(element, which) else {
                    continue;
                };
                if binding.element_id != element_id {
                    continue;
                }
                let fixed_point = if is_elbow_arrow(element) {
                    calculate_fixed_point_for_elbow_arrow_binding(
                        element, changed, which, &map, zoom, true, true,
                    )
                } else {
                    let focus =
                        get_global_fixed_point_for_bindable_element(binding.fixed_point, changed);
                    if hit(changed, focus, &map, get_binding_gap(changed), true) {
                        continue;
                    }
                    let dir = [focus[0] - center[0], focus[1] - center[1]];
                    if dir[0] == 0.0 && dir[1] == 0.0 {
                        continue;
                    }
                    let segment = [center, along(center, direction(focus, center), reach)];
                    let Some(outline) = nearest(intersect(changed, &map, segment, 0.0), focus)
                    else {
                        continue;
                    };
                    calculate_fixed_point_for_non_elbow_arrow_binding(
                        element,
                        changed,
                        which,
                        &map,
                        Some(outline),
                    )
                };
                let next = Some(Some(FixedPointBinding {
                    fixed_point,
                    ..binding.clone()
                }));
                match which {
                    BindingEnd::Start => update.start_binding = next,
                    BindingEnd::End => update.end_binding = next,
                }
            }
            update
        };
        if update.start_binding.is_some() || update.end_binding.is_some() {
            scene.mutate_element(&id, update, env);
        }
    }
}

// -- deletion and duplication -------------------------------------------------------------

/// The ids an element is bound to, with the property that holds each
/// (`bindableElementsVisitor`, `binding.ts:2412-2442`): `frameId`, a text's
/// `containerId`, an arrow's bindings.
fn bindable_ids(element: &Element) -> Vec<(BindingProp, String)> {
    let mut out = Vec::new();
    if let Some(frame) = &element.base.frame_id {
        if !frame.is_empty() {
            out.push((BindingProp::FrameId, frame.clone()));
        }
    }
    if let ElementKind::Text(t) = &element.kind {
        if let Some(container) = &t.container_id {
            out.push((BindingProp::ContainerId, container.clone()));
        }
    }
    if is_arrow(element) {
        if let Some(b) = binding_at(element, BindingEnd::Start) {
            out.push((BindingProp::StartBinding, b.element_id.clone()));
        }
        if let Some(b) = binding_at(element, BindingEnd::End) {
            out.push((BindingProp::EndBinding, b.element_id.clone()));
        }
    }
    out
}

/// `BindingProp` (`binding.ts:2372-2376`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingProp {
    FrameId,
    ContainerId,
    StartBinding,
    EndBinding,
}

impl BindingProp {
    /// `{ [bindingProp]: null }`.
    fn cleared(self) -> ElementUpdate {
        match self {
            BindingProp::FrameId => ElementUpdate {
                frame_id: Some(None),
                ..ElementUpdate::default()
            },
            BindingProp::ContainerId => ElementUpdate {
                container_id: Some(None),
                ..ElementUpdate::default()
            },
            BindingProp::StartBinding => ElementUpdate {
                start_binding: Some(None),
                ..ElementUpdate::default()
            },
            BindingProp::EndBinding => ElementUpdate {
                end_binding: Some(None),
                ..ElementUpdate::default()
            },
        }
    }
}

/// The ids in a bindable element's `boundElements`
/// (`boundElementsVisitor`, `binding.ts:2393-2410`).
fn bound_ids(element: &Element) -> Vec<String> {
    if !element.is_bindable() {
        return Vec::new();
    }
    element
        .base
        .bound_elements
        .iter()
        .flatten()
        .map(|b| b.id.clone())
        .collect()
}

/// `fixBindingsAfterDeletion(sceneElements, deletedElements)`
/// (`binding.ts:2321-2335`): for each deleted element (in scene order),
/// the elements it was bound to drop it from `boundElements`
/// (`BoundElement.unbindAffected`), and the elements bound to it drop
/// their binding (`BindableElement.unbindAffected`). Every element is
/// looked up, deleted or not.
pub fn fix_bindings_after_deletion(
    scene: &mut Scene,
    env: &mut dyn MutationEnv,
    deleted_ids: &[String],
) {
    let deleted: Vec<String> = scene
        .elements()
        .iter()
        .filter(|e| deleted_ids.contains(&e.base.id))
        .map(|e| e.base.id.clone())
        .collect();
    for id in deleted {
        let Some(element) = scene.get(&id).cloned() else {
            continue;
        };
        // BoundElement.unbindAffected
        for (_, bindable_id) in bindable_ids(&element) {
            let Some(bindable) = scene.get(&bindable_id).cloned() else {
                continue;
            };
            // bindable element is deleted, this is fine
            if bindable.base.is_deleted {
                continue;
            }
            for bound_id in bound_ids(&bindable) {
                if bound_id != id {
                    continue;
                }
                let current = scene
                    .get(&bindable_id)
                    .and_then(|b| b.base.bound_elements.clone());
                let next = current.map(|list| {
                    list.into_iter()
                        .filter(|b| b.id != bound_id)
                        .collect::<Vec<_>>()
                });
                scene.mutate_element(
                    &bindable_id,
                    ElementUpdate {
                        bound_elements: Some(next),
                        ..ElementUpdate::default()
                    },
                    env,
                );
            }
        }
        // BindableElement.unbindAffected
        for bound_id in bound_ids(&element) {
            let Some(bound) = scene.get(&bound_id).cloned() else {
                continue;
            };
            // bound element is deleted, this is fine
            if bound.base.is_deleted {
                continue;
            }
            for (prop, bindable_id) in bindable_ids(&bound) {
                // making sure there is an element to be unbound
                if bindable_id == id {
                    scene.mutate_element(&bound_id, prop.cleared(), env);
                }
            }
        }
    }
}

/// `fixDuplicatedBindingsAfterDuplication(duplicatedElements,
/// origIdToDuplicateId, duplicateElementsMap)` (`binding.ts:2249-2319`):
/// the duplicates' `boundElements`, `containerId` and bindings pointed at
/// the duplicates (dropped where the original was not duplicated), and
/// each duplicated elbow arrow re-routed between its ends among the
/// duplicates. No version bump (the duplicates are new).
pub fn fix_duplicated_bindings_after_duplication(
    duplicates: &mut [Element],
    orig_id_to_duplicate_id: &HashMap<String, String>,
) -> Result<(), ElbowArrowError> {
    for i in 0..duplicates.len() {
        let dup = &mut duplicates[i];
        if let Some(list) = &dup.base.bound_elements {
            let next: Vec<BoundElement> = list
                .iter()
                .filter_map(|b| {
                    orig_id_to_duplicate_id.get(&b.id).map(|id| BoundElement {
                        id: id.clone(),
                        kind: b.kind,
                    })
                })
                .collect();
            dup.base.bound_elements = Some(next);
        }
        if let ElementKind::Text(t) = &mut dup.kind {
            if let Some(container) = t.container_id.as_ref().filter(|c| !c.is_empty()) {
                t.container_id = orig_id_to_duplicate_id.get(container).cloned();
            }
        }
        if let Some(linear) = dup.kind.linear_mut() {
            for binding in [&mut linear.end_binding, &mut linear.start_binding] {
                if let Some(b) = binding.as_ref() {
                    *binding =
                        orig_id_to_duplicate_id
                            .get(&b.element_id)
                            .map(|id| FixedPointBinding {
                                element_id: id.clone(),
                                ..b.clone()
                            });
                }
            }
        }
        if is_elbow_arrow(&duplicates[i]) {
            let routed = {
                let dup = &duplicates[i];
                let pts = points(dup);
                let ends = match (pts.first(), pts.last()) {
                    (Some(&first), Some(&last)) => vec![first, last],
                    _ => pts.to_vec(),
                };
                let map = elbow_arrow::ElementsMap::new(duplicates.iter());
                elbow_arrow::update_elbow_arrow_points(
                    dup,
                    &map,
                    &ElbowArrowUpdates {
                        points: Some(ends),
                        ..ElbowArrowUpdates::default()
                    },
                )?
            };
            let dup = &mut duplicates[i];
            let b = &mut dup.base;
            if let Some(x) = routed.x {
                b.x = x;
            }
            if let Some(y) = routed.y {
                b.y = y;
            }
            if let Some(width) = routed.width {
                b.width = width;
            }
            if let Some(height) = routed.height {
                b.height = height;
            }
            if let ElementKind::Arrow(arrow) = &mut dup.kind {
                if let Some(points) = routed.points {
                    arrow.linear.points = points;
                }
                if let Some(v) = routed.fixed_segments {
                    arrow.fixed_segments = Some(v);
                }
                if let Some(v) = routed.start_is_special {
                    arrow.start_is_special = Some(v);
                }
                if let Some(v) = routed.end_is_special {
                    arrow.end_is_special = Some(v);
                }
                if let Some(v) = routed.start_binding {
                    arrow.linear.start_binding = v;
                }
                if let Some(v) = routed.end_binding {
                    arrow.linear.end_binding = v;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excali_scene::heading::Heading;

    #[test]
    fn fixed_points_are_two_finite_ratios() {
        assert!(is_fixed_point(&[0.5, 1.0]));
        assert!(!is_fixed_point(&[0.5]));
        assert!(!is_fixed_point(&[f64::NAN, 0.0]));
        assert!(!is_fixed_point(&[0.0, f64::INFINITY]));
    }

    #[test]
    fn javascript_or_falls_back_on_zero_and_nan() {
        assert_eq!(or(0.0, 3.0), 3.0);
        assert_eq!(or(f64::NAN, 3.0), 3.0);
        assert_eq!(or(-2.0, 3.0), -2.0);
    }

    #[test]
    fn a_heading_decides_the_grid_axis() {
        // Heading::Right is [1, 0]: |x| >= |y|, snap y
        assert!(heading_is_horizontal(Heading::Right));
        assert!(!heading_is_horizontal(Heading::Up));
    }
}
