//! Binding-target geometry the elbow arrow router reads: bounds, centre,
//! outline distance and side headings of a bindable element, and where a
//! fixed point lands on it.
//!
//! Ports of `packages/element/src/bounds.ts` (`ElementBounds`,
//! `elementCenterPoint`, `aabbForElement`, `pointInsideBounds`,
//! `getCenterForBounds`), `distance.ts` (`distanceToElement`), `utils.ts`
//! (`deconstructRectanguloidElement`, `getDiamondBaseCorners`,
//! `deconstructDiamondElement`), `stickyNote.ts`
//! (`getStickyNoteCornerRadius`), `heading.ts`
//! (`headingForPointFromElement`) and `binding.ts` (`getBindingGap`,
//! `maxBindingDistance_simple`, `getHeadingForElbowArrowSnap`,
//! `normalizeFixedPoint`, `getGlobalFixedPointForBindableElement`), for
//! bindable elements (`isBindableElement`, `typeChecks.ts:184-202`): the
//! box-like types, diamonds, ellipses and unbound text. Lines, arrows and
//! freedraw are never binding targets, and their bounds need the curve
//! code of `LinearElementEditor`; they are not handled here.

use excali_core::element::{Element, ElementKind};
use excali_math::{
    curve, curve_point_distance, distance_to_line_segment, ellipse, ellipse_distance_from_point,
    js, line_segment, point_rotate_rads, point_scale_from_origin, triangle_includes_point,
    vector_cross, Curve, GlobalPoint, LineSegment, Point, Radians, Triangle, Vector,
};
use excali_scene::bounds::get_diamond_points;
use excali_scene::heading::{heading_for_point, vector_to_heading, Heading};
use excali_scene::utils::get_corner_radius;

/// `Bounds`: `[minX, minY, maxX, maxY]` (`common/src/utility-types` /
/// `bounds.ts`).
pub type Bounds = [f64; 4];

/// `BASE_BINDING_GAP` (`binding.ts:117`): the gap between an arrow end and
/// its target's outline, before half the target's stroke width is added.
pub const BASE_BINDING_GAP: f64 = 5.0;

/// `FIXED_POINT_BOUND` (`binding.ts:2750`).
const FIXED_POINT_BOUND: f64 = 10.0;

/// `STICKY_NOTE_CORNER_RADIUS_RATIO` and `STICKY_NOTE_MAX_CORNER_RADIUS`
/// (`stickyNote.ts:64-65`).
const STICKY_NOTE_CORNER_RADIUS_RATIO: f64 = 0.04;
const STICKY_NOTE_MAX_CORNER_RADIUS: f64 = 16.0;

fn gp(p: [f64; 2]) -> GlobalPoint {
    Point::new(p[0], p[1])
}

fn xy(p: GlobalPoint) -> [f64; 2] {
    [p.x, p.y]
}

fn angle(element: &Element) -> Radians {
    Radians(element.base.angle.0)
}

fn min_of(values: &[f64]) -> f64 {
    values.iter().fold(f64::INFINITY, |m, &v| js::min(m, v))
}

fn max_of(values: &[f64]) -> f64 {
    values.iter().fold(f64::NEG_INFINITY, |m, &v| js::max(m, v))
}

/// `getBindingGap(bindTarget)` (`binding.ts:125-131`): `5 + strokeWidth / 2`.
pub fn get_binding_gap(element: &Element) -> f64 {
    BASE_BINDING_GAP + element.base.stroke_width / 2.0
}

/// `maxBindingDistance_simple(zoom)` (`binding.ts:133-143`): 15 at zoom 1
/// and above, growing as the zoom drops below 1, up to 30.
pub fn max_binding_distance_simple(zoom: f64) -> f64 {
    let base_binding_distance = js::max(BASE_BINDING_GAP, 15.0);
    let zoom_value = if zoom != 0.0 && zoom < 1.0 { zoom } else { 1.0 };
    excali_math::clamp(
        base_binding_distance / (zoom_value * 1.5),
        base_binding_distance,
        base_binding_distance * 2.0,
    )
}

/// `getElementBounds(element)` for a bindable element
/// (`ElementBounds.calculateBounds`, `bounds.ts:147-240`): the rotated
/// corners' box, the rotated vertices' box for a diamond, and the rotated
/// ellipse's own box for an ellipse.
pub fn element_bounds(element: &Element) -> Bounds {
    let b = &element.base;
    let (x1, y1, x2, y2) = (b.x, b.y, b.x + b.width, b.y + b.height);
    let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    let center = gp([cx, cy]);
    let rotate = |x: f64, y: f64| xy(point_rotate_rads(gp([x, y]), center, angle(element)));
    match element.kind {
        ElementKind::Diamond => {
            let [x11, y11] = rotate(cx, y1);
            let [x12, y12] = rotate(cx, y2);
            let [x22, y22] = rotate(x1, cy);
            let [x21, y21] = rotate(x2, cy);
            [
                min_of(&[x11, x12, x22, x21]),
                min_of(&[y11, y12, y22, y21]),
                max_of(&[x11, x12, x22, x21]),
                max_of(&[y11, y12, y22, y21]),
            ]
        }
        ElementKind::Ellipse => {
            let w = (x2 - x1) / 2.0;
            let h = (y2 - y1) / 2.0;
            let cos = b.angle.0.cos();
            let sin = b.angle.0.sin();
            let ww = js::hypot(w * cos, h * sin);
            let hh = js::hypot(h * cos, w * sin);
            [cx - ww, cy - hh, cx + ww, cy + hh]
        }
        _ => {
            let [x11, y11] = rotate(x1, y1);
            let [x12, y12] = rotate(x1, y2);
            let [x22, y22] = rotate(x2, y2);
            let [x21, y21] = rotate(x2, y1);
            [
                min_of(&[x11, x12, x22, x21]),
                min_of(&[y11, y12, y22, y21]),
                max_of(&[x11, x12, x22, x21]),
                max_of(&[y11, y12, y22, y21]),
            ]
        }
    }
}

/// `getCenterForBounds(bounds)` (`bounds.ts:1164-1168`).
pub fn get_center_for_bounds(bounds: Bounds) -> [f64; 2] {
    [
        bounds[0] + (bounds[2] - bounds[0]) / 2.0,
        bounds[1] + (bounds[3] - bounds[1]) / 2.0,
    ]
}

/// `elementCenterPoint(element)` (`bounds.ts:1555-1571`) of a bindable
/// element: the centre of its bounds.
pub fn element_center_point(element: &Element) -> [f64; 2] {
    get_center_for_bounds(element_bounds(element))
}

/// `aabbForElement(element, elementsMap, offset)` (`bounds.ts:1173-1227`):
/// the box around the element's rotated corners, grown by `offset` =
/// `[top, right, down, left]`.
pub fn aabb_for_element(element: &Element, offset: Option<[f64; 4]>) -> Bounds {
    let b = &element.base;
    let (min_x, min_y, max_x, max_y) = (b.x, b.y, b.x + b.width, b.y + b.height);
    let center = gp(element_center_point(element));
    let rotate = |x: f64, y: f64| xy(point_rotate_rads(gp([x, y]), center, angle(element)));
    let [top_left_x, top_left_y] = rotate(min_x, min_y);
    let [top_right_x, top_right_y] = rotate(max_x, min_y);
    let [bottom_right_x, bottom_right_y] = rotate(max_x, max_y);
    let [bottom_left_x, bottom_left_y] = rotate(min_x, max_y);
    let bounds = [
        min_of(&[top_left_x, top_right_x, bottom_right_x, bottom_left_x]),
        min_of(&[top_left_y, top_right_y, bottom_right_y, bottom_left_y]),
        max_of(&[top_left_x, top_right_x, bottom_right_x, bottom_left_x]),
        max_of(&[top_left_y, top_right_y, bottom_right_y, bottom_left_y]),
    ];
    match offset {
        Some([top, right, down, left]) => [
            bounds[0] - left,
            bounds[1] - top,
            bounds[2] + right,
            bounds[3] + down,
        ],
        None => bounds,
    }
}

/// `pointInsideBounds(p, bounds)` (`bounds.ts:1229-1233`): strictly inside.
pub fn point_inside_bounds(p: [f64; 2], bounds: Bounds) -> bool {
    p[0] > bounds[0] && p[0] < bounds[2] && p[1] > bounds[1] && p[1] < bounds[3]
}

/// `getStickyNoteCornerRadius(element)` (`stickyNote.ts:213-224`).
fn sticky_note_corner_radius(element: &Element) -> f64 {
    if element.base.roundness.is_none() {
        return 0.0;
    }
    let b = &element.base;
    js::min(
        js::min(b.width, b.height) * STICKY_NOTE_CORNER_RADIUS_RATIO,
        STICKY_NOTE_MAX_CORNER_RADIUS,
    )
}

/// The unrotated sides and corner curves of an outline, as
/// `deconstructRectanguloidElement` and `deconstructDiamondElement` return
/// them with no offset.
struct ElementShape {
    sides: [LineSegment; 4],
    corners: [Curve; 4],
}

impl ElementShape {
    /// The sides between consecutive corners (`utils.ts:357-374`,
    /// `utils.ts:472-489`).
    fn from_corners(corners: [Curve; 4]) -> ElementShape {
        let side = |a: usize, b: usize| line_segment(corners[a].3, corners[b].0);
        ElementShape {
            sides: [side(0, 1), side(1, 2), side(2, 3), side(3, 0)],
            corners,
        }
    }
}

/// `deconstructRectanguloidElement(element)` (`utils.ts:245-376`).
fn deconstruct_rectanguloid_element(element: &Element) -> ElementShape {
    let b = &element.base;
    let mut radius = if matches!(element.kind, ElementKind::StickyNote(_)) {
        sticky_note_corner_radius(element)
    } else {
        get_corner_radius(js::min(b.width, b.height), element)
    };
    if radius == 0.0 {
        radius = 0.01;
    }
    let r0 = [b.x, b.y];
    let r1 = [b.x + b.width, b.y + b.height];
    let top = [[r0[0] + radius, r0[1]], [r1[0] - radius, r0[1]]];
    let right = [[r1[0], r0[1] + radius], [r1[0], r1[1] - radius]];
    let bottom = [[r0[0] + radius, r1[1]], [r1[0] - radius, r1[1]]];
    let left = [[r0[0], r1[1] - radius], [r0[0], r0[1] + radius]];
    let toward = |p: [f64; 2], cx: f64, cy: f64| {
        gp([
            p[0] + (2.0 / 3.0) * (cx - p[0]),
            p[1] + (2.0 / 3.0) * (cy - p[1]),
        ])
    };
    let corners = [
        // TOP LEFT
        curve(
            gp(left[1]),
            toward(left[1], r0[0], r0[1]),
            toward(top[0], r0[0], r0[1]),
            gp(top[0]),
        ),
        // TOP RIGHT
        curve(
            gp(top[1]),
            toward(top[1], r1[0], r0[1]),
            toward(right[0], r1[0], r0[1]),
            gp(right[0]),
        ),
        // BOTTOM RIGHT
        curve(
            gp(right[1]),
            toward(right[1], r1[0], r1[1]),
            toward(bottom[1], r1[0], r1[1]),
            gp(bottom[1]),
        ),
        // BOTTOM LEFT
        curve(
            gp(bottom[0]),
            toward(bottom[0], r0[0], r1[1]),
            toward(left[0], r0[0], r1[1]),
            gp(left[0]),
        ),
    ];
    ElementShape::from_corners(corners)
}

/// `getDiamondBaseCorners(element)` and `deconstructDiamondElement(element)`
/// (`utils.ts:378-493`).
fn deconstruct_diamond_element(element: &Element) -> ElementShape {
    let b = &element.base;
    let [top_x, top_y, right_x, right_y, bottom_x, bottom_y, left_x, left_y] =
        get_diamond_points(element);
    let (vertical_radius, horizontal_radius) = if b.roundness.is_some() {
        (
            get_corner_radius((top_x - left_x).abs(), element),
            get_corner_radius((right_y - top_y).abs(), element),
        )
    } else {
        ((top_x - left_x) * 0.01, (right_y - top_y) * 0.01)
    };
    let top = [b.x + top_x, b.y + top_y];
    let right = [b.x + right_x, b.y + right_y];
    let bottom = [b.x + bottom_x, b.y + bottom_y];
    let left = [b.x + left_x, b.y + left_y];
    let (v, h) = (vertical_radius, horizontal_radius);
    let corners = [
        // RIGHT
        curve(
            gp([right[0] - v, right[1] - h]),
            gp(right),
            gp(right),
            gp([right[0] - v, right[1] + h]),
        ),
        // BOTTOM
        curve(
            gp([bottom[0] + v, bottom[1] - h]),
            gp(bottom),
            gp(bottom),
            gp([bottom[0] - v, bottom[1] - h]),
        ),
        // LEFT
        curve(
            gp([left[0] + v, left[1] + h]),
            gp(left),
            gp(left),
            gp([left[0] + v, left[1] - h]),
        ),
        // TOP
        curve(
            gp([top[0] - v, top[1] + h]),
            gp(top),
            gp(top),
            gp([top[0] + v, top[1] + h]),
        ),
    ];
    ElementShape::from_corners(corners)
}

fn distance_to_shape(shape: &ElementShape, p: GlobalPoint) -> f64 {
    let sides = shape.sides.iter().map(|&s| distance_to_line_segment(p, s));
    let corners = shape.corners.iter().map(|&c| curve_point_distance(c, p));
    sides.chain(corners).fold(f64::INFINITY, js::min)
}

/// `distanceToElement(element, elementsMap, p)` (`distance.ts:30-...`) of a
/// bindable element: the distance from `p` to the outline, rounded corners
/// and rotation included. Lines, arrows and freedraw, which are never
/// binding targets, answer infinity.
pub fn distance_to_element(element: &Element, p: [f64; 2]) -> f64 {
    let center = gp(element_center_point(element));
    let rotated = point_rotate_rads(gp(p), center, -angle(element));
    match element.kind {
        ElementKind::Diamond => distance_to_shape(&deconstruct_diamond_element(element), rotated),
        ElementKind::Ellipse => ellipse_distance_from_point(
            rotated,
            ellipse(center, element.base.width / 2.0, element.base.height / 2.0),
        ),
        ElementKind::Line(_) | ElementKind::Arrow(_) | ElementKind::Freedraw(_) => f64::INFINITY,
        _ => distance_to_shape(&deconstruct_rectanguloid_element(element), rotated),
    }
}

fn vector_from(p: [f64; 2], origin: [f64; 2]) -> Vector {
    Vector {
        x: p[0] - origin[0],
        y: p[1] - origin[1],
    }
}

/// `headingForPointFromDiamondElement(element, aabb, point)`
/// (`heading.ts:68-226`): the diamond's vertices, rotated about the box
/// centre and pulled 5% towards it, split the plane into corner and side
/// cones.
fn heading_for_point_from_diamond_element(
    element: &Element,
    aabb: Bounds,
    point: [f64; 2],
) -> Heading {
    let mid_point = get_center_for_bounds(aabb);
    let b = &element.base;
    const SHRINK: f64 = 0.95;
    let vertex = |x: f64, y: f64| {
        let rotated = xy(point_rotate_rads(gp([x, y]), gp(mid_point), angle(element)));
        let v = vector_from(rotated, mid_point);
        [mid_point[0] + v.x * SHRINK, mid_point[1] + v.y * SHRINK]
    };
    let top = vertex(b.x + b.width / 2.0, b.y);
    let right = vertex(b.x + b.width, b.y + b.height / 2.0);
    let bottom = vertex(b.x + b.width / 2.0, b.y + b.height);
    let left = vertex(b.x, b.y + b.height / 2.0);
    let cross = |a: [f64; 2], a_origin: [f64; 2], c: [f64; 2], c_origin: [f64; 2]| {
        vector_cross(vector_from(a, a_origin), vector_from(c, c_origin))
    };

    // Corners
    if cross(point, top, top, right) <= 0.0 && cross(point, top, top, left) > 0.0 {
        return heading_for_point(top, mid_point);
    } else if cross(point, right, right, bottom) <= 0.0 && cross(point, right, right, top) > 0.0 {
        return heading_for_point(right, mid_point);
    } else if cross(point, bottom, bottom, left) <= 0.0 && cross(point, bottom, bottom, right) > 0.0
    {
        return heading_for_point(bottom, mid_point);
    } else if cross(point, left, left, top) <= 0.0 && cross(point, left, left, bottom) > 0.0 {
        return heading_for_point(left, mid_point);
    }

    // Sides
    let wide = b.width > b.height;
    if cross(point, mid_point, top, mid_point) <= 0.0
        && cross(point, mid_point, right, mid_point) > 0.0
    {
        let p = if wide { top } else { right };
        return heading_for_point(p, mid_point);
    } else if cross(point, mid_point, right, mid_point) <= 0.0
        && cross(point, mid_point, bottom, mid_point) > 0.0
    {
        let p = if wide { bottom } else { right };
        return heading_for_point(p, mid_point);
    } else if cross(point, mid_point, bottom, mid_point) <= 0.0
        && cross(point, mid_point, left, mid_point) > 0.0
    {
        let p = if wide { bottom } else { left };
        return heading_for_point(p, mid_point);
    }
    let p = if wide { top } else { left };
    heading_for_point(p, mid_point)
}

/// `headingForPointFromElement(element, aabb, p)` (`heading.ts:231-280`):
/// which side of the element `p` is on, by four search cones from the
/// centre of `aabb` through its corners pushed out twice as far; a diamond
/// goes by its own vertices.
///
/// Upstream's development and test builds assert that a diamond has a
/// size and that `p` is not its centre; the production build does not,
/// and neither does this port.
pub fn heading_for_point_from_element(element: &Element, aabb: Bounds, p: [f64; 2]) -> Heading {
    const SEARCH_CONE_MULTIPLIER: f64 = 2.0;
    let mid_point = gp(get_center_for_bounds(aabb));
    if matches!(element.kind, ElementKind::Diamond) {
        return heading_for_point_from_diamond_element(element, aabb, p);
    }
    let scaled =
        |x: f64, y: f64| point_scale_from_origin(gp([x, y]), mid_point, SEARCH_CONE_MULTIPLIER);
    let top_left = scaled(aabb[0], aabb[1]);
    let top_right = scaled(aabb[2], aabb[1]);
    let bottom_left = scaled(aabb[0], aabb[3]);
    let bottom_right = scaled(aabb[2], aabb[3]);
    let p = gp(p);
    if triangle_includes_point(Triangle(top_left, top_right, mid_point), p) {
        Heading::Up
    } else if triangle_includes_point(Triangle(top_right, bottom_right, mid_point), p) {
        Heading::Right
    } else if triangle_includes_point(Triangle(bottom_right, bottom_left, mid_point), p) {
        Heading::Down
    } else {
        Heading::Left
    }
}

/// `getHeadingForElbowArrowSnap(p, otherPoint, bindableElement, aabb,
/// origPoint, elementsMap, zoom)` (`binding.ts:1574-1615`) at zoom 1: with
/// no element, the heading from `p` towards `otherPoint`; with the
/// original point out of binding distance (`getDistanceForBinding`) or
/// exactly on the outline, the heading away from the element's centre;
/// otherwise the side `p` is on.
pub fn get_heading_for_elbow_arrow_snap(
    p: [f64; 2],
    other_point: [f64; 2],
    bindable_element: Option<&Element>,
    aabb: Option<Bounds>,
    orig_point: [f64; 2],
) -> Heading {
    let other_point_heading = vector_to_heading([other_point[0] - p[0], other_point[1] - p[1]]);
    let (Some(element), Some(aabb)) = (bindable_element, aabb) else {
        return other_point_heading;
    };
    let distance = distance_to_element(element, orig_point);
    // getDistanceForBinding: null past the binding distance; `!distance`
    // is also true for 0 (and NaN)
    let far = distance > max_binding_distance_simple(1.0);
    if far || distance == 0.0 || distance.is_nan() {
        let center = element_center_point(element);
        return vector_to_heading([p[0] - center[0], p[1] - center[1]]);
    }
    heading_for_point_from_element(element, aabb, p)
}

/// `normalizeFixedPoint(fixedPoint)` (`binding.ts:2752-2777`): a fixed
/// point with a non-finite ratio becomes `[0.5001, 0.5001]`; otherwise each
/// ratio is clamped to `[-10, 10]`, and when either is within 0.0001 of 0.5,
/// each such ratio becomes 0.5001, so the heading does not flip on
/// rounding.
pub fn normalize_fixed_point(fixed_point: [f64; 2]) -> [f64; 2] {
    if !fixed_point.iter().all(|r| r.is_finite()) {
        return [0.5001, 0.5001];
    }
    const EPSILON: f64 = 0.0001;
    let clamped = fixed_point.map(|r| excali_math::clamp(r, -FIXED_POINT_BOUND, FIXED_POINT_BOUND));
    if (clamped[0] - 0.5).abs() < EPSILON || (clamped[1] - 0.5).abs() < EPSILON {
        return clamped.map(|r| if (r - 0.5).abs() < EPSILON { 0.5001 } else { r });
    }
    clamped
}

/// `getGlobalFixedPointForBindableElement(fixedPointRatio, element)`
/// (`binding.ts:2670-2685`): the normalised fixed point on the unrotated
/// element, rotated with it about its centre.
pub fn get_global_fixed_point_for_bindable_element(
    fixed_point_ratio: [f64; 2],
    element: &Element,
) -> [f64; 2] {
    let [fixed_x, fixed_y] = normalize_fixed_point(fixed_point_ratio);
    let b = &element.base;
    xy(point_rotate_rads(
        gp([b.x + b.width * fixed_x, b.y + b.height * fixed_y]),
        gp(element_center_point(element)),
        angle(element),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn element(kind: &str, extra: serde_json::Value) -> Element {
        let mut map = json!({
            "id": "e", "type": kind, "x": 10, "y": 20, "width": 100, "height": 50,
            "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
            "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
            "opacity": 100, "groupIds": [], "frameId": null, "index": null, "roundness": null,
            "seed": 1, "version": 1, "versionNonce": 0, "isDeleted": false,
            "boundElements": null, "updated": 1, "link": null, "locked": false
        });
        for (k, v) in extra.as_object().expect("object") {
            map[k] = v.clone();
        }
        Element::from_map(map.as_object().expect("object").clone()).expect("element")
    }

    #[test]
    fn binding_gap_and_distance() {
        let rect = element("rectangle", json!({"strokeWidth": 4}));
        assert_eq!(get_binding_gap(&rect), 7.0);
        assert_eq!(max_binding_distance_simple(1.0), 15.0);
        assert_eq!(max_binding_distance_simple(0.25), 30.0);
        assert_eq!(max_binding_distance_simple(0.8), 15.0);
    }

    #[test]
    fn fixed_points_avoid_exact_half() {
        assert_eq!(normalize_fixed_point([0.5, 0.2]), [0.5001, 0.2]);
        assert_eq!(normalize_fixed_point([0.50005, 0.5]), [0.5001, 0.5001]);
        assert_eq!(normalize_fixed_point([20.0, -12.0]), [10.0, -10.0]);
        assert_eq!(normalize_fixed_point([f64::NAN, 0.0]), [0.5001, 0.5001]);
    }

    #[test]
    fn outline_distance() {
        let rect = element("rectangle", json!({}));
        // 5 right of the right side, level with its middle
        assert!((distance_to_element(&rect, [115.0, 45.0]) - 5.0).abs() < 1e-9);
        let ellipse = element("ellipse", json!({}));
        assert!((distance_to_element(&ellipse, [60.0, 10.0]) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn sides_by_cone() {
        let rect = element("rectangle", json!({}));
        let aabb = aabb_for_element(&rect, None);
        assert_eq!(aabb, [10.0, 20.0, 110.0, 70.0]);
        assert_eq!(
            heading_for_point_from_element(&rect, aabb, [60.0, 10.0]),
            Heading::Up
        );
        assert_eq!(
            heading_for_point_from_element(&rect, aabb, [120.0, 45.0]),
            Heading::Right
        );
        assert_eq!(
            heading_for_point_from_element(&rect, aabb, [60.0, 80.0]),
            Heading::Down
        );
        assert_eq!(
            heading_for_point_from_element(&rect, aabb, [0.0, 45.0]),
            Heading::Left
        );
        let diamond = element("diamond", json!({}));
        let aabb = aabb_for_element(&diamond, None);
        assert_eq!(
            heading_for_point_from_element(&diamond, aabb, [60.0, 15.0]),
            Heading::Up
        );
        assert_eq!(
            heading_for_point_from_element(&diamond, aabb, [115.0, 45.0]),
            Heading::Right
        );
    }

    #[test]
    fn rotated_fixed_point() {
        let rect = element("rectangle", json!({"angle": std::f64::consts::FRAC_PI_2}));
        let p = get_global_fixed_point_for_bindable_element([1.0, 0.5001], &rect);
        // the right-middle of a 100 x 50 box turned a quarter about (60, 45)
        assert!(
            (p[0] - 59.995).abs() < 1e-9 && (p[1] - 95.0).abs() < 1e-9,
            "{p:?}"
        );
    }
}
