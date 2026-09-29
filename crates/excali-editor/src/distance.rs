//! How far a point is from an element's outline: a port of
//! `packages/element/src/distance.ts` for every element type.
//!
//! - Box-like elements (rectangles, sticky notes, images, text, iframes,
//!   embeddables, frames, selections), diamonds and ellipses: the point is
//!   rotated the other way about the element's centre and measured against
//!   the unrotated outline, rounded corners included
//!   ([`deconstruct_rectanguloid_element`], [`deconstruct_diamond_element`]).
//! - Lines and arrows: the segments and curves of the collision shape
//!   ([`deconstruct_linear_or_freedraw_element`], already rotated).
//! - Freedraw: 0 inside the inked outline (non-zero winding), otherwise the
//!   distance to the outline; infinity without an outline.

use excali_core::element::{Element, ElementKind};
use excali_math::{
    curve_point_distance, distance_to_line_segment, ellipse, ellipse_distance_from_point, js,
    point_rotate_rads, polygon_includes_point_non_zero, Curve, GlobalPoint, LineSegment, Point,
    Radians,
};
use excali_scene::bounds::{element_center_point, ElementsMap};
use excali_scene::linear_element::deconstruct_linear_or_freedraw_element;
use excali_scene::utils::{deconstruct_diamond_element, deconstruct_rectanguloid_element};

fn gp(p: [f64; 2]) -> GlobalPoint {
    Point::new(p[0], p[1])
}

/// `Math.min(...sides.map(distanceToLineSegment), ...curves.map(curvePointDistance))`:
/// infinity for no parts, NaN when any distance is NaN.
fn min_distance(p: GlobalPoint, sides: &[LineSegment], curves: &[Curve]) -> f64 {
    let sides = sides.iter().map(|&s| distance_to_line_segment(p, s));
    let curves = curves.iter().map(|&c| curve_point_distance(c, p));
    sides.chain(curves).fold(f64::INFINITY, js::min)
}

/// The point rotated by `-angle` about the element's centre
/// (`elementCenterPoint`), to measure against the unrotated outline.
fn unrotated(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    p: [f64; 2],
) -> (GlobalPoint, GlobalPoint) {
    let center = gp(element_center_point(element, elements_map));
    let rotated = point_rotate_rads(gp(p), center, Radians(-element.base.angle.0));
    (center, rotated)
}

/// `distanceToElement(element, elementsMap, p)` (`distance.ts:30-55`): the
/// distance from `p` to the element's outline.
pub fn distance_to_element(element: &Element, elements_map: &ElementsMap<'_>, p: [f64; 2]) -> f64 {
    match element.kind {
        ElementKind::Selection
        | ElementKind::Rectangle
        | ElementKind::StickyNote(_)
        | ElementKind::Image(_)
        | ElementKind::Text(_)
        | ElementKind::Iframe
        | ElementKind::Embeddable
        | ElementKind::Frame(_)
        | ElementKind::MagicFrame(_) => {
            // distanceToRectanguloidElement (distance.ts:65-83)
            let (_, rotated) = unrotated(element, elements_map, p);
            let outline = deconstruct_rectanguloid_element(element);
            min_distance(rotated, &outline.sides, &outline.corners)
        }
        ElementKind::Diamond => {
            // distanceToDiamondElement (distance.ts:93-110)
            let (_, rotated) = unrotated(element, elements_map, p);
            let outline = deconstruct_diamond_element(element);
            min_distance(rotated, &outline.sides, &outline.corners)
        }
        ElementKind::Ellipse => {
            // distanceToEllipseElement (distance.ts:120-131)
            let (center, rotated) = unrotated(element, elements_map, p);
            ellipse_distance_from_point(
                rotated,
                ellipse(center, element.base.width / 2.0, element.base.height / 2.0),
            )
        }
        ElementKind::Line(_) | ElementKind::Arrow(_) => {
            // distanceToLinearOrFreeDraElement (distance.ts:133-146)
            let (lines, curves) = deconstruct_linear_or_freedraw_element(element, elements_map);
            min_distance(gp(p), &lines, &curves)
        }
        ElementKind::Freedraw(_) => distance_to_free_draw_element(element, elements_map, p),
    }
}

/// `distanceToFreeDrawElement(element, elementsMap, p)`
/// (`distance.ts:156-173`): 0 when the point is within the inked area (the
/// outline's first points as a polygon, non-zero winding), otherwise the
/// distance to the outline; infinity when there is no outline.
fn distance_to_free_draw_element(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    p: [f64; 2],
) -> f64 {
    let (lines, _) = deconstruct_linear_or_freedraw_element(element, elements_map);
    if lines.is_empty() {
        return f64::INFINITY;
    }
    let polygon: Vec<GlobalPoint> = lines.iter().map(|l| l.0).collect();
    if polygon_includes_point_non_zero(gp(p), &polygon) {
        return 0.0;
    }
    min_distance(gp(p), &lines, &[])
}
