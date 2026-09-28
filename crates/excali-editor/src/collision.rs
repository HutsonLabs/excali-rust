//! Hit testing: a port of `packages/element/src/collision.ts` and
//! `App.getElementHitThreshold` / `App.hitElement`
//! (`packages/excalidraw/components/App.tsx:6927-6982`). See
//! `site/content/research/rendering.md` section 5.
//!
//! - The threshold ([`get_element_hit_threshold`]):
//!   `max(strokeWidth / 2 + 0.1, 0.85 * DEFAULT_COLLISION_THRESHOLD / zoom)`,
//!   `DEFAULT_COLLISION_THRESHOLD = 2 * 4 - 1e-5`.
//! - [`hit_element_itself`] (`collision.ts:137-230`): a single-entry result
//!   cache ([`HitTestCache`]), then an early reject on the element's rotated
//!   bounds grown by the threshold (and, for freedraw, by the reach of its
//!   ink), then either inside or on the outline ([`should_test_inside`]),
//!   or on the outline only. A frame's name label counts as a hit.
//! - Inside ([`is_point_in_element`]): a ray from the point away from the
//!   bounds' centre crosses the outline an odd number of times; a freedraw
//!   loop tests its rendered fill contour, even-odd. Lines and freedraw
//!   that are not loops have no inside.
//! - On the outline: [`distance_to_element`] within the threshold.
//! - Per-shape intersections ([`intersect_element_with_line_segment`],
//!   `collision.ts:494-829`): box-like elements and diamonds by their sides
//!   and corner curves (grown by an offset), ellipses analytically, lines,
//!   arrows and freedraw by their collision shape.
//! - Binding targets ([`get_hovered_element_for_binding`],
//!   [`get_all_hovered_element_at_point`], `collision.ts:290-492`): bindable
//!   elements within the zoom's binding distance, front to back, stopping
//!   at an opaque element that contains the point.
//!
//! Upstream keeps the result cache in module globals; the port hands it to
//! the caller (one per editor), so it can be reset and inspected. Upstream
//! also caches shapes, bounds and fill contours per element version; the
//! port computes them afresh, which gives the same numbers.

use std::borrow::Cow;

use excali_core::color::is_transparent;
use excali_core::constants::DEFAULT_COLLISION_THRESHOLD;
use excali_core::element::{Element, ElementKind};
use excali_math::{
    curve_intersect_line_segment, curve_intersect_line_segment_with, ellipse,
    ellipse_segment_intercept_points, is_point_within_bounds, js, line_segment,
    line_segment_intersection_points, point_from_vector, point_rotate_rads, points_equal,
    polygon_includes_point, vector, vector_from_point_with, vector_normalize, vector_scale, Curve,
    CurveIntersectOptions, GlobalPoint, LineSegment, LocalPoint, Point, Radians,
};
use excali_scene::bounds::{
    do_bounds_intersect, element_center_point, get_bound_text_element,
    get_cubic_bezier_curve_bound, get_diamond_points, get_element_bounds,
    get_element_bounds_non_rotated, Bounds, ElementsMap,
};
use excali_scene::frame::is_frame_like;
use excali_scene::freedraw::{get_freedraw_fill_polygon, get_freedraw_max_stroke_radius};
use excali_scene::linear_element::{
    deconstruct_linear_or_freedraw_element, get_bound_text_element_position,
};
use excali_scene::utils::{
    deconstruct_diamond_element_with_offset, deconstruct_rectanguloid_element_with_offset,
    is_path_a_loop,
};

pub use excali_scene::geometric_shape::should_test_inside;

use crate::distance::distance_to_element;
use crate::geometry::max_binding_distance_simple;

fn gp(p: [f64; 2]) -> GlobalPoint {
    Point::new(p[0], p[1])
}

fn xy(p: GlobalPoint) -> [f64; 2] {
    [p.x, p.y]
}

fn angle(element: &Element) -> Radians {
    Radians(element.base.angle.0)
}

/// `App.getElementHitThreshold(element)` (`App.tsx:6927-6934`): how far
/// from an element's outline a pointer still hits it, in scene units:
/// `max(strokeWidth / 2 + 0.1, 0.85 * (DEFAULT_COLLISION_THRESHOLD / zoom))`.
///
/// Upstream warns not to go under the 0.63 multiplier: hit testing becomes
/// unreliable in floating point at high zoom.
pub fn get_element_hit_threshold(stroke_width: f64, zoom: f64) -> f64 {
    js::max(
        stroke_width / 2.0 + 0.1,
        0.85 * (DEFAULT_COLLISION_THRESHOLD / zoom),
    )
}

/// `FrameNameBounds` (`excalidraw/types.ts:1435-1440`): where a frame's name
/// label is drawn, in scene coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameNameBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// `HitTestArgs` (`collision.ts:109-116`).
#[derive(Clone, Copy, Debug)]
pub struct HitTestArgs<'a> {
    pub point: [f64; 2],
    pub element: &'a Element,
    pub threshold: f64,
    pub elements_map: &'a ElementsMap<'a>,
    /// A frame's name label, hit like the frame itself.
    pub frame_name_bound: Option<FrameNameBounds>,
    /// Test the inside even when [`should_test_inside`] says not to.
    pub override_should_test_inside: bool,
}

/// The element a cached result was computed for: upstream holds a weak
/// reference and compares `id`, `version` and `versionNonce`.
#[derive(Clone, Debug, PartialEq)]
struct CachedElement {
    id: String,
    version: f64,
    version_nonce: f64,
}

/// `hitElementItself`'s single-entry result cache (`collision.ts:118-123`):
/// the last point, element, threshold, override and frame name bound tested
/// and the answer.
///
/// A cached hit answers again for any larger threshold, a cached miss for
/// any threshold no larger; a text bound to a container is always tested
/// afresh, since its position can change without its version.
#[derive(Clone, Debug)]
pub struct HitTestCache {
    point: Option<[f64; 2]>,
    element: Option<CachedElement>,
    threshold: f64,
    hit: bool,
    override_should_test_inside: bool,
    frame_name_bound: Option<FrameNameBounds>,
    outline_tests: u64,
}

impl Default for HitTestCache {
    fn default() -> Self {
        HitTestCache::new()
    }
}

impl HitTestCache {
    /// An empty cache (upstream's initial module state).
    pub fn new() -> HitTestCache {
        HitTestCache {
            point: None,
            element: None,
            threshold: f64::INFINITY,
            hit: false,
            override_should_test_inside: false,
            frame_name_bound: None,
            outline_tests: 0,
        }
    }

    /// How many outline tests (`distanceToElement` calls) the hit tests
    /// through this cache have run; a cached answer runs none.
    pub fn outline_tests(&self) -> u64 {
        self.outline_tests
    }

    /// The cached answer for these arguments, if it still holds
    /// (`collision.ts:145-167`).
    fn lookup(&self, args: &HitTestArgs<'_>) -> Option<bool> {
        let cached_point = self.point?;
        let threshold_ok = if self.hit {
            self.threshold <= args.threshold
        } else {
            self.threshold >= args.threshold
        };
        if !is_bound_to_container(args.element)
            && points_equal(gp(args.point), gp(cached_point))
            && threshold_ok
            && args.override_should_test_inside == self.override_should_test_inside
            && frame_name_bounds_equal(args.frame_name_bound, self.frame_name_bound)
        {
            let cached = self.element.as_ref()?;
            let b = &args.element.base;
            if cached.id == b.id
                && cached.version == b.version
                && cached.version_nonce == b.version_nonce
            {
                return Some(self.hit);
            }
        }
        None
    }

    fn store(&mut self, args: &HitTestArgs<'_>, element: &Element, hit: bool) {
        self.point = Some(args.point);
        self.element = Some(CachedElement {
            id: element.base.id.clone(),
            version: element.base.version,
            version_nonce: element.base.version_nonce,
        });
        self.threshold = args.threshold;
        self.override_should_test_inside = args.override_should_test_inside;
        self.frame_name_bound = args.frame_name_bound;
        self.hit = hit;
    }
}

/// `frameNameBoundsEqual(a, b)` (`collision.ts:125-135`).
fn frame_name_bounds_equal(a: Option<FrameNameBounds>, b: Option<FrameNameBounds>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// `isBoundToContainer(element)` (`typeChecks.ts:307-316`): a text with a
/// `containerId` (even an empty one).
fn is_bound_to_container(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Text(t) if t.container_id.is_some())
}

/// `getTextElementWithAccuratePosition(textElement, elementsMap)`
/// (`textElement.ts:452-473`): an arrow label at the position its arrow
/// puts it, since its stored coordinates can be stale; anything else as is.
pub fn get_text_element_with_accurate_position<'e>(
    text: &'e Element,
    elements_map: &ElementsMap<'_>,
) -> Cow<'e, Element> {
    let ElementKind::Text(fields) = &text.kind else {
        return Cow::Borrowed(text);
    };
    let Some(container_id) = fields.container_id.as_deref().filter(|id| !id.is_empty()) else {
        return Cow::Borrowed(text);
    };
    match elements_map.get(container_id) {
        Some(container) if matches!(container.kind, ElementKind::Arrow(_)) => {
            let [x, y] = get_bound_text_element_position(container, text, elements_map);
            let mut positioned = text.clone();
            positioned.base.x = x;
            positioned.base.y = y;
            Cow::Owned(positioned)
        }
        _ => Cow::Borrowed(text),
    }
}

/// `hitElementItself(args)` (`collision.ts:137-230`): whether the point
/// hits the element itself (not its bound text or its selection box).
pub fn hit_element_itself(cache: &mut HitTestCache, args: &HitTestArgs<'_>) -> bool {
    if let Some(hit) = cache.lookup(args) {
        return hit;
    }
    let HitTestArgs {
        point,
        element,
        threshold,
        elements_map,
        frame_name_bound,
        override_should_test_inside,
    } = *args;

    // Hit test against a frame's name
    let hit_frame_name = frame_name_bound.is_some_and(|b| {
        is_point_within_bounds(
            gp([b.x - threshold, b.y - threshold]),
            gp(point),
            gp([b.x + b.width + threshold, b.y + b.height + threshold]),
        )
    });

    // Hit test against the extended, rotated bounding box of the element
    // first; freedraw bounds follow the centerline points, but the ink
    // extends past them
    let bounds = get_element_bounds_non_rotated(element, elements_map);
    let tolerance = if matches!(element.kind, ElementKind::Freedraw(_)) {
        threshold + get_freedraw_max_stroke_radius(element)
    } else {
        threshold
    };
    let hit_bounds = is_point_in_rotated_bounds(point, bounds, angle(element), tolerance);

    // Bail out early if the point is not even in the rotated bounding box
    // or on the frame name
    if !hit_bounds && !hit_frame_name {
        return false;
    }

    // an arrow label's stored coordinates can be stale
    let element = if matches!(element.kind, ElementKind::Text(_)) {
        get_text_element_with_accurate_position(element, elements_map)
    } else {
        Cow::Borrowed(element)
    };

    // `inShape` tests strictly inside, so the outline is tested as well
    let hit_element = if override_should_test_inside || should_test_inside(&element) {
        is_point_in_element(point, &element, elements_map)
            || is_point_on_element_outline(cache, point, &element, elements_map, threshold)
    } else {
        is_point_on_element_outline(cache, point, &element, elements_map, threshold)
    };

    let result = hit_element || hit_frame_name;
    cache.store(args, &element, result);
    result
}

/// `isPointInRotatedBounds(point, bounds, angle, tolerance)`
/// (`collision.ts:232-249`): the point, rotated back about the bounds'
/// centre, within the bounds grown by `tolerance`.
fn is_point_in_rotated_bounds(
    point: [f64; 2],
    bounds: Bounds,
    angle: Radians,
    tolerance: f64,
) -> bool {
    let adjusted = if angle.0 == 0.0 {
        gp(point)
    } else {
        let center = [
            bounds[0] + (bounds[2] - bounds[0]) / 2.0,
            bounds[1] + (bounds[3] - bounds[1]) / 2.0,
        ];
        point_rotate_rads(gp(point), gp(center), Radians(-angle.0))
    };
    is_point_within_bounds(
        gp([bounds[0] - tolerance, bounds[1] - tolerance]),
        adjusted,
        gp([bounds[2] + tolerance, bounds[3] + tolerance]),
    )
}

/// `hitElementBoundingBox(point, element, elementsMap, tolerance)`
/// (`collision.ts:251-259`): the point within the element's rotated box
/// grown by `tolerance`.
pub fn hit_element_bounding_box(
    point: [f64; 2],
    element: &Element,
    elements_map: &ElementsMap<'_>,
    tolerance: f64,
) -> bool {
    let bounds = get_element_bounds_non_rotated(element, elements_map);
    is_point_in_rotated_bounds(point, bounds, angle(element), tolerance)
}

/// `hitElementBoundingBoxOnly(hitArgs, elementsMap)`
/// (`collision.ts:261-268`): the point in the element's box but on neither
/// the element nor its bound text.
pub fn hit_element_bounding_box_only(
    cache: &mut HitTestCache,
    args: &HitTestArgs<'_>,
    elements_map: &ElementsMap<'_>,
) -> bool {
    !hit_element_itself(cache, args)
        // bound text is part of the element, even outside its box
        && !hit_element_bound_text(args.point, args.element, elements_map)
        && hit_element_bounding_box(args.point, args.element, elements_map, 0.0)
}

/// `hitElementBoundText(point, element, elementsMap)`
/// (`collision.ts:270-287`): the point inside the element's bound text.
pub fn hit_element_bound_text(
    point: [f64; 2],
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> bool {
    let Some(candidate) = get_bound_text_element(element, elements_map) else {
        return false;
    };
    let bound_text = get_text_element_with_accurate_position(candidate, elements_map);
    is_point_in_element(point, &bound_text, elements_map)
}

/// `App.hitElement(x, y, element, considerBoundingBox)`
/// (`App.tsx:6936-6982`): a selected element with a bounding box is hit
/// anywhere in its box grown by the threshold; then its bound text; then
/// [`hit_element_itself`] with the threshold [`get_element_hit_threshold`]
/// gives at `zoom`, and the frame's name label.
///
/// `selected_with_bounding_box` is upstream's
/// `selectedElementIds[element.id] && hasBoundingBox([element], ...)`.
#[allow(clippy::too_many_arguments)]
pub fn hit_element(
    cache: &mut HitTestCache,
    point: [f64; 2],
    element: &Element,
    elements_map: &ElementsMap<'_>,
    zoom: f64,
    consider_bounding_box: bool,
    selected_with_bounding_box: bool,
    frame_name_bound: Option<FrameNameBounds>,
) -> bool {
    let threshold = get_element_hit_threshold(element.base.stroke_width, zoom);
    if consider_bounding_box
        && selected_with_bounding_box
        && hit_element_bounding_box(point, element, elements_map, threshold)
    {
        return true;
    }
    if hit_element_bound_text(point, element, elements_map) {
        return true;
    }
    hit_element_itself(
        cache,
        &HitTestArgs {
            point,
            element,
            threshold,
            elements_map,
            frame_name_bound: if is_frame_like(element) {
                frame_name_bound
            } else {
                None
            },
            override_should_test_inside: false,
        },
    )
}

/// `isPointClippedByEnclosingFrame(element, point, elementsMap)`
/// (`collision.ts:289-307`): frame children are clipped to their frame, so
/// a point outside it cannot reach them.
fn is_point_clipped_by_enclosing_frame(
    element: &Element,
    point: [f64; 2],
    elements_map: &ElementsMap<'_>,
) -> bool {
    let Some(frame_id) = element.base.frame_id.as_deref().filter(|id| !id.is_empty()) else {
        return false;
    };
    match elements_map.get(frame_id) {
        Some(frame) if is_frame_like(frame) => {
            let b = get_element_bounds(frame, elements_map);
            // pointInsideBounds: strictly inside
            !(point[0] > b[0] && point[0] < b[2] && point[1] > b[1] && point[1] < b[3])
        }
        _ => false,
    }
}

/// `bindableElementBorderDistanceIfClose(element, point, elementsMap,
/// tolerance)` (`collision.ts:309-343`): the distance to the outline,
/// positive inside and negative outside, or `-Infinity` when farther than
/// `tolerance`, clipped by the enclosing frame, or inside a frame.
fn bindable_element_border_distance_if_close(
    element: &Element,
    point: [f64; 2],
    elements_map: &ElementsMap<'_>,
    tolerance: f64,
) -> f64 {
    // a cheap test first
    let t = js::max(1.0, tolerance);
    let bounds = [point[0] - t, point[1] - t, point[0] + t, point[1] + t];
    if !do_bounds_intersect(bounds, get_element_bounds(element, elements_map)) {
        return f64::NEG_INFINITY;
    }
    if is_point_clipped_by_enclosing_frame(element, point, elements_map) {
        return f64::NEG_INFINITY;
    }
    let is_inside = is_point_in_element(point, element, elements_map);
    // frames are only bindable from the outside
    if is_inside && is_frame_like(element) {
        return f64::NEG_INFINITY;
    }
    let distance = distance_to_element(element, elements_map, point);
    if is_inside {
        return distance;
    }
    if distance > tolerance {
        f64::NEG_INFINITY
    } else {
        -distance
    }
}

/// `BindingCandidate` (`collision.ts:345-349`).
#[derive(Clone, Copy)]
struct BindingCandidate<'a> {
    element: &'a Element,
    /// to the outline: positive inside, negative outside
    distance: f64,
}

/// `isOpaqueForBinding(element)` (`collision.ts:351-357`): images, and
/// anything with a background that is not transparent.
fn is_opaque_for_binding(element: &Element) -> bool {
    matches!(element.kind, ElementKind::Image(_))
        || (has_background(element) && !is_transparent(&element.base.background_color))
}

/// `hasBackground(type)` (`comparisons.ts:3-14`) for element types.
fn has_background(element: &Element) -> bool {
    matches!(
        element.kind,
        ElementKind::Rectangle
            | ElementKind::StickyNote(_)
            | ElementKind::Iframe
            | ElementKind::Embeddable
            | ElementKind::Ellipse
            | ElementKind::Diamond
            | ElementKind::Line(_)
            | ElementKind::Freedraw(_)
    )
}

/// `getBindingCandidates(point, elements, elementsMap, zoom)`
/// (`collision.ts:359-423`): bindable elements within binding distance,
/// front to back, stopping at the first opaque element containing the
/// point. Locked elements are not candidates but still hide what is behind
/// them; an opaque frame containing the point hides only non-children.
///
/// # Panics
///
/// On a deleted element, as upstream's invariant throws ("Elements passed
/// to binding hit tests should not contain deleted elements").
fn get_binding_candidates<'a>(
    point: [f64; 2],
    elements: &[&'a Element],
    elements_map: &ElementsMap<'_>,
    zoom: f64,
) -> Vec<BindingCandidate<'a>> {
    let max_distance = max_binding_distance_simple(zoom);
    let mut candidates = Vec::new();
    let mut occluding_frame_id: Option<&str> = None;
    for &element in elements.iter().rev() {
        assert!(
            !element.base.is_deleted,
            "Elements passed to binding hit tests should not contain deleted elements"
        );
        if let Some(frame_id) = occluding_frame_id {
            if element.base.frame_id.as_deref() != Some(frame_id) {
                continue;
            }
        }
        if !element.is_bindable() {
            continue;
        }
        if is_frame_like(element)
            && is_opaque_for_binding(element)
            && is_point_in_element(point, element, elements_map)
        {
            occluding_frame_id = Some(element.base.id.as_str());
        }
        let distance =
            bindable_element_border_distance_if_close(element, point, elements_map, max_distance);
        if distance > -max_distance {
            if !element.base.locked {
                candidates.push(BindingCandidate { element, distance });
            }
            if distance >= 0.0 && is_opaque_for_binding(element) {
                break;
            }
        }
    }
    candidates
}

/// `getAllHoveredElementAtPoint(point, elements, elementsMap, zoom)`
/// (`collision.ts:425-438`): every element an arrow end at the point could
/// bind to, front to back. `elements` are in z-order, back to front.
pub fn get_all_hovered_element_at_point<'a>(
    point: [f64; 2],
    elements: &[&'a Element],
    elements_map: &ElementsMap<'_>,
    zoom: f64,
) -> Vec<&'a Element> {
    get_binding_candidates(point, elements, elements_map, zoom)
        .into_iter()
        .map(|c| c.element)
        .collect()
}

/// `getHoveredElementForBinding(point, elements, elementsMap, zoom)`
/// (`collision.ts:440-492`): the candidate whose outline is closest, unless
/// the point is inside it and inside a smaller element overlapping it by
/// more than a quarter (and under three quarters its area), which wins.
pub fn get_hovered_element_for_binding<'a>(
    point: [f64; 2],
    elements: &[&'a Element],
    elements_map: &ElementsMap<'_>,
    zoom: f64,
) -> Option<&'a Element> {
    let mut candidates = get_binding_candidates(point, elements, elements_map, zoom);
    match candidates.len() {
        0 => return None,
        1 => return Some(candidates[0].element),
        _ => {}
    }
    js::sort(&mut candidates, |a, b| a.distance.abs() - b.distance.abs());
    let candidate = candidates[0];
    let [cx1, cy1, cx2, cy2] = get_element_bounds(candidate.element, elements_map);
    let candidate_area = js::max(0.00001, (cx2 - cx1).abs() * (cy2 - cy1).abs());
    let overlap = candidates.iter().find(|c| {
        if std::ptr::eq(c.element, candidate.element) || c.distance < 0.0 || c.distance.is_nan() {
            return false;
        }
        let [x1, y1, x2, y2] = get_element_bounds(c.element, elements_map);
        let overlap_width = js::max(0.0, js::min(x2, cx2) - js::max(x1, cx1));
        let overlap_height = js::max(0.0, js::min(y2, cy2) - js::max(y1, cy1));
        let area = js::max(0.00001, (x2 - x1).abs() * (y2 - y1).abs());
        let overlap_percent = (overlap_height * overlap_width) / area;
        let relative_area = area / candidate_area;
        overlap_percent > 0.25 && relative_area < 0.75
    });
    match overlap {
        Some(o) if candidate.distance >= 0.0 => Some(o.element),
        _ => Some(candidate.element),
    }
}

/// `intersectElementWithLineSegment(element, elementsMap, line, offset,
/// onlyFirst)` (`collision.ts:494-570`): where the segment crosses the
/// element's outline grown by `offset`; with `only_first`, stop at the
/// first part of the outline that it crosses.
pub fn intersect_element_with_line_segment(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    line: [[f64; 2]; 2],
    offset: f64,
    only_first: bool,
) -> Vec<[f64; 2]> {
    // the segment's box against the element's first: much cheaper
    let intersector_bounds = [
        js::min(line[0][0] - offset, line[1][0] - offset),
        js::min(line[0][1] - offset, line[1][1] - offset),
        js::max(line[0][0] + offset, line[1][0] + offset),
        js::max(line[0][1] + offset, line[1][1] + offset),
    ];
    if !do_bounds_intersect(
        intersector_bounds,
        get_element_bounds(element, elements_map),
    ) {
        return Vec::new();
    }
    let segment = line_segment(gp(line[0]), gp(line[1]));
    let points = match element.kind {
        ElementKind::Rectangle
        | ElementKind::StickyNote(_)
        | ElementKind::Image(_)
        | ElementKind::Text(_)
        | ElementKind::Iframe
        | ElementKind::Embeddable
        | ElementKind::Frame(_)
        | ElementKind::Selection
        | ElementKind::MagicFrame(_) => {
            let shape = deconstruct_rectanguloid_element_with_offset(element, offset);
            intersect_outline_with_line_segment(
                element,
                elements_map,
                segment,
                &shape.sides,
                &shape.corners,
                only_first,
            )
        }
        ElementKind::Diamond => {
            let shape = deconstruct_diamond_element_with_offset(element, offset);
            intersect_outline_with_line_segment(
                element,
                elements_map,
                segment,
                &shape.sides,
                &shape.corners,
                only_first,
            )
        }
        ElementKind::Ellipse => {
            intersect_ellipse_with_line_segment(element, elements_map, segment, offset)
        }
        ElementKind::Line(_) | ElementKind::Freedraw(_) | ElementKind::Arrow(_) => {
            intersect_linear_or_freedraw_with_line_segment(
                element,
                segment,
                elements_map,
                only_first,
            )
        }
    };
    points.into_iter().map(xy).collect()
}

/// `CURVE_BOUNDS_EPSILON` (`collision.ts:572`).
const CURVE_BOUNDS_EPSILON: f64 = 1e-6;

fn curve_bound(c: Curve) -> Bounds {
    get_cubic_bezier_curve_bound(xy(c.0), xy(c.1), xy(c.2), xy(c.3))
}

/// `curveIntersections(curves, segment, intersections, center, angle,
/// onlyFirst)` (`collision.ts:574-613`).
fn curve_intersections(
    curves: &[Curve],
    segment: LineSegment,
    intersections: &mut Vec<GlobalPoint>,
    center: GlobalPoint,
    angle: Radians,
    only_first: bool,
) {
    // Padded so an axis-aligned segment through the joint of two curves
    // still overlaps their bounds (touching bounds do not intersect)
    let b2 = [
        js::min(segment.0.x, segment.1.x) - CURVE_BOUNDS_EPSILON,
        js::min(segment.0.y, segment.1.y) - CURVE_BOUNDS_EPSILON,
        js::max(segment.0.x, segment.1.x) + CURVE_BOUNDS_EPSILON,
        js::max(segment.0.y, segment.1.y) + CURVE_BOUNDS_EPSILON,
    ];
    for &c in curves {
        if !do_bounds_intersect(curve_bound(c), b2) {
            continue;
        }
        let hits = curve_intersect_line_segment(c, segment);
        if !hits.is_empty() {
            intersections.extend(
                hits.into_iter()
                    .map(|j| point_rotate_rads(j, center, angle)),
            );
            if only_first {
                return;
            }
        }
    }
}

/// `lineIntersections(lines, segment, intersections, center, angle,
/// onlyFirst)` (`collision.ts:615-635`).
fn line_intersections(
    lines: &[LineSegment],
    segment: LineSegment,
    intersections: &mut Vec<GlobalPoint>,
    center: GlobalPoint,
    angle: Radians,
    only_first: bool,
) {
    for &l in lines {
        if let Some(intersection) = line_segment_intersection_points(l, segment) {
            intersections.push(point_rotate_rads(intersection, center, angle));
            if only_first {
                return;
            }
        }
    }
}

/// `intersectLinearOrFreeDrawWithLineSegment(element, segment, elementsMap,
/// onlyFirst)` (`collision.ts:637-691`): the collision shape is already
/// rotated.
fn intersect_linear_or_freedraw_with_line_segment(
    element: &Element,
    segment: LineSegment,
    elements_map: &ElementsMap<'_>,
    only_first: bool,
) -> Vec<GlobalPoint> {
    let (lines, curves) = deconstruct_linear_or_freedraw_element(element, elements_map);
    let mut intersections = Vec::new();
    for &l in &lines {
        if let Some(intersection) = line_segment_intersection_points(l, segment) {
            intersections.push(intersection);
            if only_first {
                return intersections;
            }
        }
    }
    let b2 = [
        js::min(segment.0.x, segment.1.x),
        js::min(segment.0.y, segment.1.y),
        js::max(segment.0.x, segment.1.x),
        js::max(segment.0.y, segment.1.y),
    ];
    for &c in &curves {
        if !do_bounds_intersect(curve_bound(c), b2) {
            continue;
        }
        let hits = curve_intersect_line_segment_with(
            c,
            segment,
            CurveIntersectOptions {
                tolerance: None,
                iter_limit: Some(10),
            },
        );
        if !hits.is_empty() {
            intersections.extend(hits);
            if only_first {
                return intersections;
            }
        }
    }
    intersections
}

/// `intersectRectanguloidWithLineSegment` and
/// `intersectDiamondWithLineSegment` (`collision.ts:693-790`): the segment
/// rotated the other way about the element's centre against the unrotated
/// sides, then corners, the hits rotated back.
fn intersect_outline_with_line_segment(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    segment: LineSegment,
    sides: &[LineSegment],
    corners: &[Curve],
    only_first: bool,
) -> Vec<GlobalPoint> {
    let center = gp(element_center_point(element, elements_map));
    let back = Radians(-element.base.angle.0);
    let rotated = line_segment(
        point_rotate_rads(segment.0, center, back),
        point_rotate_rads(segment.1, center, back),
    );
    let mut intersections = Vec::new();
    line_intersections(
        sides,
        rotated,
        &mut intersections,
        center,
        angle(element),
        only_first,
    );
    if only_first && !intersections.is_empty() {
        return intersections;
    }
    curve_intersections(
        corners,
        rotated,
        &mut intersections,
        center,
        angle(element),
        only_first,
    );
    intersections
}

/// `intersectEllipseWithLineSegment(element, elementsMap, l, offset)`
/// (`collision.ts:799-814`).
fn intersect_ellipse_with_line_segment(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    l: LineSegment,
    offset: f64,
) -> Vec<GlobalPoint> {
    let center = gp(element_center_point(element, elements_map));
    let back = Radians(-element.base.angle.0);
    let rotated_a = point_rotate_rads(l.0, center, back);
    let rotated_b = point_rotate_rads(l.1, center, back);
    let b = &element.base;
    ellipse_segment_intercept_points(
        ellipse(center, b.width / 2.0 + offset, b.height / 2.0 + offset),
        line_segment(rotated_a, rotated_b),
    )
    .into_iter()
    .map(|p| point_rotate_rads(p, center, angle(element)))
    .collect()
}

/// `isPointOnElementOutline(point, element, elementsMap, tolerance)`
/// (`collision.ts:824-829`): within `tolerance` of the outline. Counted in
/// the cache's [`HitTestCache::outline_tests`].
fn is_point_on_element_outline(
    cache: &mut HitTestCache,
    point: [f64; 2],
    element: &Element,
    elements_map: &ElementsMap<'_>,
    tolerance: f64,
) -> bool {
    cache.outline_tests += 1;
    distance_to_element(element, elements_map, point) <= tolerance
}

/// `isPointInElement(point, element, elementsMap)` (`collision.ts:838-895`):
/// strictly inside the element's outline.
///
/// - A line or freedraw that is not a loop has no inside.
/// - A freedraw loop: the point, rotated back about the unrotated bounds'
///   centre, in its fill contour ([`get_freedraw_fill_polygon`], even-odd).
/// - Anything else: within the rotated bounds, and a segment from the point
///   away from the bounds' centre (twice the element's larger side long)
///   crosses the outline an odd number of times, repeated points counted
///   once.
pub fn is_point_in_element(
    point: [f64; 2],
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> bool {
    if let (ElementKind::Line(_) | ElementKind::Arrow(_) | ElementKind::Freedraw(_), Some(points)) =
        (&element.kind, element.kind.points())
    {
        if !is_path_a_loop(points, 1.0) {
            // There isn't any "inside" for a non-looping path
            return false;
        }
    }

    if matches!(element.kind, ElementKind::Freedraw(_)) {
        // Before the rotated bounds check: the smoothed fill can bulge past
        // the recorded points, and an asymmetric loop's rotated bounds
        // centre is not its rotation pivot
        let [x1, y1, x2, y2] = get_element_bounds_non_rotated(element, elements_map);
        let p = point_rotate_rads(
            gp(point),
            gp([(x1 + x2) / 2.0, (y1 + y2) / 2.0]),
            Radians(-element.base.angle.0),
        );
        let local: LocalPoint = Point::new(p.x - element.base.x, p.y - element.base.y);
        return get_freedraw_fill_polygon(element)
            .is_some_and(|polygon| polygon_includes_point(local, &polygon));
    }

    let [x1, y1, x2, y2] = get_element_bounds(element, elements_map);
    if !is_point_within_bounds(gp([x1, y1]), gp(point), gp([x2, y2])) {
        return false;
    }

    let center = gp([(x1 + x2) / 2.0, (y1 + y2) / 2.0]);
    let other_point = point_from_vector(
        vector_scale(
            vector_normalize(vector_from_point_with(
                gp(point),
                center,
                Some(0.1),
                vector(0.0, 1.0),
            )),
            js::max(element.base.width, element.base.height) * 2.0,
        ),
        center,
    );
    let intersections = intersect_element_with_line_segment(
        element,
        elements_map,
        [point, xy(other_point)],
        0.0,
        false,
    );
    let unique = intersections
        .iter()
        .enumerate()
        .filter(|&(pos, &p)| {
            intersections
                .iter()
                .position(|&q| points_equal(gp(q), gp(p)))
                == Some(pos)
        })
        .count();
    unique % 2 == 1
}

/// `isBindableElementInsideOtherBindable(innerElement, outerElement,
/// elementsMap)` (`collision.ts:897-942`): every extreme point of the inner
/// element, pulled in by 5% of its larger side, is inside the outer
/// element: a diamond's vertices, an ellipse's axis ends, anything else's
/// corners, rotated with the element.
pub fn is_bindable_element_inside_other_bindable(
    inner: &Element,
    outer: &Element,
    elements_map: &ElementsMap<'_>,
) -> bool {
    let b = &inner.base;
    // 5% of the larger side, inwards
    let offset = -js::max(b.width, b.height) / 20.0;
    let center = gp(element_center_point(inner, elements_map));
    let corners: [[f64; 2]; 4] = match inner.kind {
        ElementKind::Diamond => {
            let [top_x, top_y, right_x, right_y, bottom_x, bottom_y, left_x, left_y] =
                get_diamond_points(inner);
            [
                [b.x + top_x, b.y + top_y - offset],
                [b.x + right_x + offset, b.y + right_y],
                [b.x + bottom_x, b.y + bottom_y + offset],
                [b.x + left_x - offset, b.y + left_y],
            ]
        }
        ElementKind::Ellipse => {
            let cx = b.x + b.width / 2.0;
            let cy = b.y + b.height / 2.0;
            let rx = b.width / 2.0;
            let ry = b.height / 2.0;
            [
                [cx, cy - ry - offset],
                [cx + rx + offset, cy],
                [cx, cy + ry + offset],
                [cx - rx - offset, cy],
            ]
        }
        _ => [
            [b.x - offset, b.y - offset],
            [b.x + b.width + offset, b.y - offset],
            [b.x + b.width + offset, b.y + b.height + offset],
            [b.x - offset, b.y + b.height + offset],
        ],
    };
    corners.iter().all(|&corner| {
        let rotated = point_rotate_rads(gp(corner), center, angle(inner));
        is_point_in_element(xy(rotated), outer, elements_map)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_name_bounds_compare_by_value() {
        let a = FrameNameBounds {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        };
        assert!(frame_name_bounds_equal(None, None));
        assert!(frame_name_bounds_equal(Some(a), Some(a)));
        assert!(!frame_name_bounds_equal(Some(a), None));
        assert!(!frame_name_bounds_equal(
            Some(a),
            Some(FrameNameBounds { width: 5.0, ..a })
        ));
    }

    #[test]
    fn rotated_bounds_test_rotates_the_point_back_about_the_bounds_centre() {
        let bounds = [0.0, 0.0, 100.0, 20.0];
        // a quarter turn maps (50, 50) onto (10, 10) about (50, 10)
        let quarter = Radians(std::f64::consts::FRAC_PI_2);
        assert!(!is_point_in_rotated_bounds(
            [50.0, 50.0],
            bounds,
            Radians(0.0),
            0.0
        ));
        assert!(is_point_in_rotated_bounds(
            [50.0, 50.0],
            bounds,
            quarter,
            0.0
        ));
        assert!(is_point_in_rotated_bounds(
            [103.0, 10.0],
            bounds,
            Radians(0.0),
            3.0
        ));
        assert!(!is_point_in_rotated_bounds(
            [103.5, 10.0],
            bounds,
            Radians(0.0),
            3.0
        ));
    }
}
