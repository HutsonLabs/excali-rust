//! Pure geometric shapes of elements: `packages/utils/src/shape.ts` and
//! `getElementShape` (`packages/element/src/shape.ts:1087-1136`), which
//! the frame tests read through [`crate::bounds::get_element_line_segments`].
//!
//! - Boxes, diamonds, frames, images, iframes, text and selections: the
//!   rotated corners as a closed polygon ([`get_polygon_shape`]; a
//!   diamond's are its side midpoints).
//! - Ellipses: centre, angle and half axes ([`get_ellipse_shape`]).
//! - Lines and arrows: the first rough.js shape upstream draws, read as
//!   curves ([`get_curve_shape`]), or, when the line is tested inside
//!   ([`should_test_inside`]), as a closed polygon: its points when sharp,
//!   the flattened curves of its first stroke when round
//!   ([`get_closed_curve_shape`]).
//! - Freedraw: its points as a polyline, or a closed polygon when tested
//!   inside ([`get_freedraw_shape`]).

use excali_core::color::is_transparent;
use excali_core::element::{Element, ElementKind};
use excali_math::{
    curve, line_segment, point_from, point_rotate_rads, polygon, polygon_from_points, Curve,
    GlobalPoint, LineSegment, Polygon, Radians,
};
use excali_rough::points_on_curve::points_on_bezier_curves;
use excali_rough::{Op, RoughGenerator};

use crate::bounds::{get_curve_path_ops, get_element_absolute_coords, ElementsMap};
use crate::shape::{generate_linear_element_shapes, RenderConfig, ShapeError};
use crate::utils::is_path_a_loop;

/// `Ellipse` of `packages/utils/src/shape.ts`: centre, angle and half
/// width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipseShape {
    pub center: GlobalPoint,
    pub angle: Radians,
    pub half_width: f64,
    pub half_height: f64,
}

/// `GeometricShape` (`packages/utils/src/shape.ts:74-99`), the kinds
/// `getElementShape` returns.
#[derive(Clone, Debug, PartialEq)]
pub enum GeometricShape {
    Polygon(Polygon),
    Ellipse(EllipseShape),
    /// A line made of segments.
    Polyline(Vec<LineSegment>),
    /// A curve made of cubic curves.
    Polycurve(Vec<Curve>),
}

fn angle(element: &Element) -> Radians {
    Radians(element.base.angle.0)
}

/// `getPolygonShape(element)` (`shape.ts:118-155`): the box's corners
/// rotated about its centre (a diamond's side midpoints), closed.
pub fn get_polygon_shape(element: &Element) -> GeometricShape {
    let b = &element.base;
    let (x, y, width, height) = (b.x, b.y, b.width, b.height);
    let cx = x + width / 2.0;
    let cy = y + height / 2.0;
    let center: GlobalPoint = point_from(cx, cy);
    let rotate = |px: f64, py: f64| point_rotate_rads(point_from(px, py), center, angle(element));
    let data = if matches!(element.kind, ElementKind::Diamond) {
        polygon(&[
            rotate(cx, y),
            rotate(x + width, cy),
            rotate(cx, y + height),
            rotate(x, cy),
        ])
    } else {
        polygon(&[
            rotate(x, y),
            rotate(x + width, y),
            rotate(x + width, y + height),
            rotate(x, y + height),
        ])
    };
    GeometricShape::Polygon(data)
}

/// `getEllipseShape(element)` (`shape.ts:183-197`).
pub fn get_ellipse_shape(element: &Element) -> GeometricShape {
    let b = &element.base;
    GeometricShape::Ellipse(EllipseShape {
        center: point_from(b.x + b.width / 2.0, b.y + b.height / 2.0),
        angle: angle(element),
        half_width: b.width / 2.0,
        half_height: b.height / 2.0,
    })
}

/// `getCurveShape(roughShape, startingPoint, angle, center)`
/// (`shape.ts:214-250`): each `bcurveTo` of the ops as a curve from the
/// point before it, every point moved by `start` and rotated about
/// `center`; a `move` sets the next curve's start, `lineTo`s are skipped.
pub fn get_curve_shape(
    ops: &[Op],
    start: GlobalPoint,
    angle: Radians,
    center: GlobalPoint,
) -> GeometricShape {
    let transform = |x: f64, y: f64| -> GlobalPoint {
        point_rotate_rads(point_from(x + start.x, y + start.y), center, angle)
    };
    let mut polycurve = Vec::new();
    let mut p0: GlobalPoint = point_from(0.0, 0.0);
    for op in ops {
        match *op {
            Op::Move([x, y]) => p0 = transform(x, y),
            Op::BCurveTo(d) => {
                let p1 = transform(d[0], d[1]);
                let p2 = transform(d[2], d[3]);
                let p3 = transform(d[4], d[5]);
                polycurve.push(curve(p0, p1, p2, p3));
                p0 = p3;
            }
            Op::LineTo(_) => {}
        }
    }
    GeometricShape::Polycurve(polycurve)
}

/// `getFreedrawShape(element, center, isClosed)` (`shape.ts:267-296`): the
/// points moved to the element's position and rotated about `center`,
/// joined as a polyline; when closed, the polyline's segment ends in
/// order (each inner point twice) as a closed polygon.
pub fn get_freedraw_shape(
    element: &Element,
    center: GlobalPoint,
    is_closed: bool,
) -> GeometricShape {
    let b = &element.base;
    let points = element.kind.points().unwrap_or(&[]);
    let transformed: Vec<GlobalPoint> = points
        .iter()
        .map(|&[x, y]| point_rotate_rads(point_from(x + b.x, y + b.y), center, angle(element)))
        .collect();
    let polyline: Vec<LineSegment> = transformed
        .windows(2)
        .map(|w| line_segment(w[0], w[1]))
        .collect();
    if is_closed {
        let flat = polyline.iter().flat_map(|s| [s.0, s.1]).collect();
        GeometricShape::Polygon(polygon_from_points(flat))
    } else {
        GeometricShape::Polyline(polyline)
    }
}

/// `getClosedCurveShape(element, roughShape, startingPoint, angle,
/// center)` (`shape.ts:298-349`): a sharp line's points, or a round line's
/// first stroke (the points of every other `move` and the ops after it)
/// flattened by `pointsOnBezierCurves(points, 10, 5)`; moved by `start`,
/// rotated about `center`, closed.
pub fn get_closed_curve_shape(
    element: &Element,
    ops: &[Op],
    start: GlobalPoint,
    angle: Radians,
    center: GlobalPoint,
) -> Result<GeometricShape, ShapeError> {
    let transform = |x: f64, y: f64| -> GlobalPoint {
        point_rotate_rads(point_from(x + start.x, y + start.y), center, angle)
    };
    if element.base.roundness.is_none() {
        let points = element.kind.points().unwrap_or(&[]);
        return Ok(GeometricShape::Polygon(polygon_from_points(
            points.iter().map(|&[x, y]| transform(x, y)).collect(),
        )));
    }
    let mut points: Vec<[f64; 2]> = Vec::new();
    let mut odd = false;
    for op in ops {
        match *op {
            Op::Move(p) => {
                odd = !odd;
                if odd {
                    points.push(p);
                }
            }
            Op::BCurveTo(d) => {
                if odd {
                    points.push([d[0], d[1]]);
                    points.push([d[2], d[3]]);
                    points.push([d[4], d[5]]);
                }
            }
            Op::LineTo(p) => {
                if odd {
                    points.push(p);
                }
            }
        }
    }
    let flattened = points_on_bezier_curves(&points, 10.0, Some(5.0)).map_err(ShapeError::Path)?;
    Ok(GeometricShape::Polygon(polygon_from_points(
        flattened.iter().map(|&[x, y]| transform(x, y)).collect(),
    )))
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

/// `hasBoundTextElement(element)` (`typeChecks.ts:297-305`): a text
/// container (`isTextBindableContainer`: rectangle, sticky note, diamond,
/// ellipse, arrow) listing a bound text.
fn has_bound_text_element(element: &Element) -> bool {
    matches!(
        element.kind,
        ElementKind::Rectangle
            | ElementKind::StickyNote(_)
            | ElementKind::Diamond
            | ElementKind::Ellipse
            | ElementKind::Arrow(_)
    ) && element.base.bound_elements.as_deref().is_some_and(|b| {
        b.iter()
            .any(|e| e.kind == excali_core::element::BoundElementType::Text)
    })
}

/// `shouldTestInside(element)` (`collision.ts:87-107`): arrows never; a
/// line or freedraw when it can be dragged from inside (a visible
/// background or bound text) and its path is a loop; anything else when it
/// can be dragged from inside (a visible background, bound text, an
/// iframe-like or text) or is an image.
pub fn should_test_inside(element: &Element) -> bool {
    if matches!(element.kind, ElementKind::Arrow(_)) {
        return false;
    }
    let is_draggable_from_inside = (has_background(element)
        && !is_transparent(&element.base.background_color))
        || has_bound_text_element(element)
        || matches!(element.kind, ElementKind::Iframe | ElementKind::Embeddable)
        || matches!(element.kind, ElementKind::Text(_));
    match &element.kind {
        ElementKind::Line(_) | ElementKind::Freedraw(_) => {
            is_draggable_from_inside && is_path_a_loop(element.kind.points().unwrap_or(&[]), 1.0)
        }
        ElementKind::Image(_) => true,
        _ => is_draggable_from_inside,
    }
}

/// `getElementShape(element, elementsMap)` (`shape.ts:1087-1136`).
///
/// A line's or arrow's shape starts from the first shape upstream draws
/// for it (`ShapeCache.generateElementShape(element, null)[0]`), so it
/// fails where drawing it fails.
pub fn get_element_shape(
    element: &Element,
    elements_map: &ElementsMap<'_>,
) -> Result<GeometricShape, ShapeError> {
    let b = &element.base;
    match &element.kind {
        ElementKind::Rectangle
        | ElementKind::StickyNote(_)
        | ElementKind::Diamond
        | ElementKind::Frame(_)
        | ElementKind::MagicFrame(_)
        | ElementKind::Embeddable
        | ElementKind::Image(_)
        | ElementKind::Iframe
        | ElementKind::Text(_)
        | ElementKind::Selection => Ok(get_polygon_shape(element)),
        ElementKind::Arrow(_) | ElementKind::Line(_) => {
            let shapes = generate_linear_element_shapes(
                element,
                &RoughGenerator::new(),
                &RenderConfig::default(),
            )?;
            // getCurvePathOps(undefined) answers no ops
            let ops = shapes.first().map_or(&[][..], get_curve_path_ops);
            let [_, _, _, _, cx, cy] = get_element_absolute_coords(element, elements_map, false);
            let start = point_from(b.x, b.y);
            let center = point_from(cx, cy);
            if should_test_inside(element) {
                get_closed_curve_shape(element, ops, start, angle(element), center)
            } else {
                Ok(get_curve_shape(ops, start, angle(element), center))
            }
        }
        ElementKind::Ellipse => Ok(get_ellipse_shape(element)),
        ElementKind::Freedraw(_) => {
            let [_, _, _, _, cx, cy] = get_element_absolute_coords(element, elements_map, false);
            Ok(get_freedraw_shape(
                element,
                point_from(cx, cy),
                should_test_inside(element),
            ))
        }
    }
}
