//! Ports of upstream's `packages/math/tests/*.test.ts` at the pinned commit.
//! Test names and cases follow the upstream `describe`/`it` blocks one for one.

use excali_math::*;

type P = GlobalPoint;

fn pt(x: f64, y: f64) -> P {
    point_from(x, y)
}

/// Vitest's `toBeCloseTo(expected, digits = 2)`.
fn close_to(actual: f64, expected: f64, digits: i32) -> bool {
    (actual - expected).abs() < 10f64.powi(-digits) / 2.0
}

/// `toCloselyEqualPoints(expected)` (`packages/utils/src/test-utils.ts`):
/// every expected point has a received point at the same index with both
/// coordinates within `Math.pow(10, precision ?? 2)`. Upstream's window is
/// that power itself, so the default is 100 units; the curve tests below
/// check this matcher and then the 0.01 their expected values are written to.
fn closely_equal_points<S: Space>(received: &[Point<S>], expected: &[[f64; 2]]) -> bool {
    let compare = js::pow(10f64, 2.0);
    expected.iter().enumerate().all(|(idx, point)| {
        let got = received
            .get(idx)
            .unwrap_or_else(|| panic!("no received point at {idx}"));
        (got[0] - point[0]).abs() < compare && (got[1] - point[1]).abs() < compare
    })
}

/// The expected points to the 0.01 the upstream test writes them with.
fn points_to_hundredths<S: Space>(received: &[Point<S>], expected: &[[f64; 2]]) -> bool {
    received.len() == expected.len()
        && received
            .iter()
            .zip(expected)
            .all(|(got, want)| (got.x - want[0]).abs() < 0.01 && (got.y - want[1]).abs() < 0.01)
}

mod curve_test {
    use super::*;

    fn check(received: &[P], expected: &[[f64; 2]]) {
        assert!(closely_equal_points(received, expected), "{received:?}");
        assert!(points_to_hundredths(received, expected), "{received:?}");
    }

    // describe("Math curve") / describe("line segment intersection")
    #[test]
    fn point_is_found_when_control_points_are_the_same() {
        let c = curve(
            pt(100.0, 0.0),
            pt(100.0, 100.0),
            pt(100.0, 100.0),
            pt(0.0, 100.0),
        );
        let l = line_segment(pt(0.0, 0.0), pt(200.0, 200.0));

        check(&curve_intersect_line_segment(c, l), &[[87.5, 87.5]]);
    }

    #[test]
    fn point_is_found_when_control_points_arent_the_same() {
        let c = curve(
            pt(100.0, 0.0),
            pt(100.0, 60.0),
            pt(60.0, 100.0),
            pt(0.0, 100.0),
        );
        let l = line_segment(pt(0.0, 0.0), pt(200.0, 200.0));

        check(&curve_intersect_line_segment(c, l), &[[72.5, 72.5]]);
    }

    #[test]
    fn points_are_found_when_curve_is_sliced_at_3_points() {
        let c = curve(
            pt(-50.0, -50.0),
            pt(10.0, -50.0),
            pt(10.0, 50.0),
            pt(50.0, 50.0),
        );
        let l = line_segment(pt(10.0, -60.0), pt(10.0, 60.0));

        check(&curve_intersect_line_segment(c, l), &[[9.99, 5.05]]);
    }

    #[test]
    fn can_be_detected_where_the_determinant_is_overly_precise() {
        let c = curve(
            pt(41.028864759926016, 12.226249068355052),
            pt(41.028864759926016, 33.55958240168839),
            pt(30.362198093259348, 44.22624906835505),
            pt(9.028864759926016, 44.22624906835505),
        );
        let l = line_segment(
            pt(-82.30963544324186, -41.19949363038283),
            pt(188.2149592542487, 134.75505940984908),
        );

        check(&curve_intersect_line_segment(c, l), &[[34.4, 34.71]]);
    }

    // describe("point closest to other")
    #[test]
    fn point_can_be_found() {
        let c = curve(
            pt(-50.0, -50.0),
            pt(10.0, -50.0),
            pt(10.0, 50.0),
            pt(50.0, 50.0),
        );
        let p = pt(0.0, 0.0);

        check(
            &[bezier_equation(c, curve_closest_parameter_with(c, p, 1e-3))],
            &[[5.965462100367372, -3.04104878946646]],
        );
    }

    // describe("point shortest distance")
    #[test]
    fn can_be_determined() {
        let c = curve(
            pt(-50.0, -50.0),
            pt(10.0, -50.0),
            pt(10.0, 50.0),
            pt(50.0, 50.0),
        );
        let p = pt(0.0, 0.0);

        assert!(close_to(curve_point_distance(c, p), 6.695873043213627, 2));
    }
}

mod ellipse_test {
    use super::*;

    // describe("point and ellipse")
    #[test]
    fn point_on_ellipse() {
        let target: Ellipse<Global> = ellipse(pt(1.0, 2.0), 2.0, 1.0);
        for p in [pt(1.0, 3.0), pt(1.0, 1.0), pt(3.0, 2.0), pt(-1.0, 2.0)] {
            assert!(ellipse_touches_point(p, target), "{p:?}");
        }
        assert!(ellipse_touches_point_with(pt(-0.4, 2.7), target, 0.1));
        assert!(ellipse_touches_point_with(pt(-0.4, 2.71), target, 0.01));

        assert!(ellipse_touches_point_with(pt(2.4, 2.7), target, 0.1));
        assert!(ellipse_touches_point_with(pt(2.4, 2.71), target, 0.01));

        assert!(ellipse_touches_point_with(pt(2.0, 1.14), target, 0.1));
        assert!(ellipse_touches_point_with(pt(2.0, 1.14), target, 0.01));

        assert!(ellipse_touches_point_with(pt(0.0, 1.14), target, 0.1));
        assert!(ellipse_touches_point_with(pt(0.0, 1.14), target, 0.01));

        assert!(!ellipse_touches_point(pt(0.0, 2.8), target));
        assert!(!ellipse_touches_point(pt(2.0, 1.2), target));
    }

    #[test]
    fn point_in_ellipse() {
        let target: Ellipse<Global> = ellipse(pt(0.0, 0.0), 2.0, 1.0);
        for p in [pt(0.0, 1.0), pt(0.0, -1.0), pt(2.0, 0.0), pt(-2.0, 0.0)] {
            assert!(ellipse_includes_point(p, target), "{p:?}");
        }

        assert!(ellipse_includes_point(pt(-1.0, 0.8), target));
        assert!(ellipse_includes_point(pt(1.0, -0.8), target));

        // Point on outline
        assert!(ellipse_includes_point(pt(2.0, 0.0), target));

        assert!(!ellipse_includes_point(pt(-1.0, 1.0), target));
        assert!(!ellipse_includes_point(pt(-1.4, 0.8), target));
    }

    // describe("point distance to ellipse outline")
    #[test]
    fn is_the_radius_at_a_circles_center() {
        let center = pt(0.0, 0.0);
        let circle = ellipse(center, 100.0, 100.0);

        assert_eq!(ellipse_distance_from_point(center, circle), 100.0);
        assert_eq!(ellipse_distance_from_point(pt(1.0, 0.0), circle), 99.0);
        assert_eq!(ellipse_distance_from_point(pt(30.0, 40.0), circle), 50.0);
    }

    #[test]
    fn measures_points_on_the_axes_of_a_non_circular_ellipse() {
        let center = pt(0.0, 0.0);
        let wide = ellipse(center, 200.0, 50.0);

        // closest to the top/bottom of the outline, not to the long axis
        assert!(close_to(ellipse_distance_from_point(center, wide), 50.0, 2));
        assert!(close_to(
            ellipse_distance_from_point(pt(10.0, 0.0), wide),
            49.93,
            1
        ));
        assert!(close_to(
            ellipse_distance_from_point(pt(-150.0, 0.0), wide),
            31.62,
            1
        ));
        assert!(close_to(
            ellipse_distance_from_point(pt(0.0, 10.0), wide),
            40.0,
            2
        ));
        assert!(close_to(
            ellipse_distance_from_point(pt(250.0, 0.0), wide),
            50.0,
            2
        ));
    }

    // describe("segment and ellipse")
    #[test]
    fn detects_outside_segment() {
        let e = ellipse(pt(0.0, 0.0), 2.0, 2.0);

        assert_eq!(
            ellipse_segment_intercept_points(e, line_segment(pt(-100.0, 0.0), pt(-10.0, 0.0))),
            vec![]
        );
        assert_eq!(
            ellipse_segment_intercept_points(e, line_segment(pt(-10.0, 0.0), pt(10.0, 0.0))),
            vec![pt(-2.0, 0.0), pt(2.0, 0.0)]
        );
        assert_eq!(
            ellipse_segment_intercept_points(e, line_segment(pt(-10.0, -2.0), pt(10.0, -2.0))),
            vec![pt(0.0, -2.0)]
        );
        assert_eq!(
            ellipse_segment_intercept_points(e, line_segment(pt(0.0, -1.0), pt(0.0, 1.0))),
            vec![]
        );
    }

    // describe("line and ellipse")
    #[test]
    fn detects_outside_line() {
        let e = ellipse(pt(0.0, 0.0), 2.0, 2.0);
        assert_eq!(
            ellipse_line_intersection_points(e, line(pt(-10.0, -10.0), pt(10.0, -10.0))),
            vec![]
        );
    }

    #[test]
    fn detects_line_intersecting_ellipse() {
        let e = ellipse(pt(0.0, 0.0), 2.0, 2.0);
        assert_eq!(
            ellipse_line_intersection_points(e, line(pt(0.0, -1.0), pt(0.0, 1.0))),
            vec![pt(0.0, 2.0), pt(0.0, -2.0)]
        );
        let rounded: Vec<P> =
            ellipse_line_intersection_points(e, line(pt(-100.0, 0.0), pt(-10.0, 0.0)))
                .into_iter()
                .map(|p| pt(js::round(p.x), js::round(p.y)))
                .collect();
        assert_eq!(rounded, vec![pt(2.0, 0.0), pt(-2.0, 0.0)]);
    }

    #[test]
    fn detects_line_touching_ellipse() {
        let e = ellipse(pt(0.0, 0.0), 2.0, 2.0);
        assert_eq!(
            ellipse_line_intersection_points(e, line(pt(-2.0, -2.0), pt(2.0, -2.0))),
            vec![pt(0.0, -2.0)]
        );
    }
}

mod line_test {
    use super::*;

    // describe("line-line intersections")
    #[test]
    fn should_correctly_detect_intersection_at_origin() {
        assert_eq!(
            lines_intersect_at(
                line(pt(-5.0, -5.0), pt(5.0, 5.0)),
                line(pt(5.0, -5.0), pt(-5.0, 5.0)),
            ),
            Some(pt(0.0, 0.0))
        );
    }

    #[test]
    fn should_correctly_detect_intersection_at_non_origin() {
        assert_eq!(
            lines_intersect_at(
                line(pt(0.0, 0.0), pt(10.0, 10.0)),
                line(pt(10.0, 0.0), pt(0.0, 10.0)),
            ),
            Some(pt(5.0, 5.0))
        );
    }

    #[test]
    fn should_correctly_detect_parallel_lines() {
        assert_eq!(
            lines_intersect_at(
                line(pt(0.0, 0.0), pt(0.0, 10.0)),
                line(pt(10.0, 0.0), pt(10.0, 10.0)),
            ),
            None
        );
    }
}

mod point_test {
    use super::*;

    // describe("rotate")
    #[test]
    fn should_rotate_over_x2_y2_and_return_the_rotated_coordinates_for_x1_y1() {
        let (x1, y1) = (10.0, 20.0);
        let (x2, y2) = (20.0, 30.0);
        let angle = Radians(std::f64::consts::PI / 2.0);
        let rotated = point_rotate_rads(pt(x1, y1), pt(x2, y2), angle);
        assert_eq!([rotated.x, rotated.y], [30.0, 20.0]);
        let res2 = point_rotate_rads(pt(rotated.x, rotated.y), pt(x2, y2), -angle);
        // upstream compares against [x1, x2] (sic), which is [10, 20] = [x1, y1]
        assert_eq!(res2, pt(x1, x2));
    }
}

mod polygon_test {
    use super::*;

    fn square() -> Vec<P> {
        vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0), pt(0.0, 10.0)]
    }

    fn reversed(points: &[P]) -> Vec<P> {
        points.iter().rev().copied().collect()
    }

    // describe("polygonArea")
    #[test]
    fn measures_a_polygon_whichever_way_it_winds() {
        assert_eq!(polygon_area(&square()), 100.0);
        assert_eq!(polygon_area(&reversed(&square())), 100.0);
    }

    #[test]
    fn ignores_a_repeated_closing_vertex() {
        let mut closed = square();
        closed.push(square()[0]);
        assert_eq!(polygon_area(&closed), 100.0);
    }

    #[test]
    fn reports_the_winding_direction_in_the_sign() {
        assert_eq!(polygon_signed_area(&square()), 100.0);
        assert_eq!(polygon_signed_area(&reversed(&square())), -100.0);
    }

    // describe("convexHull")
    #[test]
    fn drops_the_points_inside_the_hull() {
        let mut points = square();
        points.push(pt(5.0, 5.0));
        let hull = convex_hull(&points);

        assert_eq!(hull.len(), 4);
        for p in square() {
            assert!(hull.contains(&p), "{p:?}");
        }
    }

    #[test]
    fn drops_collinear_points() {
        let mut points = square();
        points.push(pt(5.0, 0.0));
        assert_eq!(convex_hull(&points).len(), 4);
    }

    #[test]
    fn wraps_a_point_cloud_whatever_order_it_arrives_in() {
        let cloud: Vec<P> = (0..30)
            .map(|i| {
                let a = f64::from(i) * 2.4;
                pt(js::cos(a) * 10.0, js::sin(a) * 10.0)
            })
            .collect();
        let hull = convex_hull(&cloud);

        assert!(close_to(
            polygon_area(&hull),
            polygon_area(&convex_hull(&reversed(&cloud))),
            2
        ));
        // Every point is inside or on the hull it produced.
        assert!(polygon_area(&hull) > 250.0);
        assert!(polygon_area(&hull) <= std::f64::consts::PI * 100.0);
    }

    #[test]
    fn convex_hull_returns_degenerate_input_as_is() {
        let two = square()[..2].to_vec();
        assert_eq!(convex_hull(&two), two);
    }

    // describe("simplifyConvexPolygon")
    fn circle(n: u32) -> Vec<P> {
        (0..n)
            .map(|i| {
                let a = (f64::from(i) * 2.0 * std::f64::consts::PI) / f64::from(n);
                pt(js::cos(a) * 100.0, js::sin(a) * 100.0)
            })
            .collect()
    }

    const DEG25: f64 = (25.0 * std::f64::consts::PI) / 180.0;

    #[test]
    fn keeps_the_corners_of_a_polygon_and_drops_the_wobble_along_its_sides() {
        // A square whose sides bulge slightly outward, as a hand-drawn one would.
        let mut points = square();
        points.extend([pt(5.0, -0.2), pt(10.2, 5.0), pt(5.0, 10.2), pt(-0.2, 5.0)]);
        let wobbly = convex_hull(&points);

        assert_eq!(simplify_convex_polygon(&wobbly, DEG25).len(), 4);
    }

    #[test]
    fn spreads_an_ellipses_turn_over_many_corners() {
        let corners = simplify_convex_polygon(&convex_hull(&circle(64)), DEG25);

        // A full turn is 360 degrees, emitted in ~25 degree steps.
        assert!(corners.len() > 10);
        assert!(corners.len() <= 15);
    }

    #[test]
    fn does_not_depend_on_where_the_hull_happens_to_start() {
        let mut points = square();
        points.extend([pt(5.0, -0.2), pt(10.2, 5.0)]);
        let hull = convex_hull(&points);
        let rotated: Vec<P> = hull[2..].iter().chain(&hull[..2]).copied().collect();

        assert_eq!(
            simplify_convex_polygon(&rotated, DEG25).len(),
            simplify_convex_polygon(&hull, DEG25).len()
        );
    }

    #[test]
    fn simplify_returns_degenerate_input_as_is() {
        let two = square()[..2].to_vec();
        assert_eq!(simplify_convex_polygon(&two, 0.4), two);
    }
}

mod range_test {
    use super::*;

    fn range1_4() -> InclusiveRange {
        range_inclusive(1.0, 4.0)
    }

    // describe("range overlap")
    #[test]
    fn should_overlap_when_range_a_contains_range_b() {
        assert!(ranges_overlap(range1_4(), range_inclusive(2.0, 3.0)));
        assert!(ranges_overlap(range1_4(), range1_4()));
        assert!(ranges_overlap(range1_4(), range_inclusive(1.0, 3.0)));
        assert!(ranges_overlap(range1_4(), range_inclusive(2.0, 4.0)));
    }

    #[test]
    fn should_overlap_when_range_b_contains_range_a() {
        assert!(ranges_overlap(range_inclusive(2.0, 3.0), range1_4()));
        assert!(ranges_overlap(range_inclusive(1.0, 3.0), range1_4()));
        assert!(ranges_overlap(range_inclusive(2.0, 4.0), range1_4()));
    }

    #[test]
    fn should_overlap_when_range_a_and_b_intersect() {
        assert!(ranges_overlap(range1_4(), range_inclusive(2.0, 5.0)));
    }

    // describe("range intersection")
    #[test]
    fn should_intersect_completely_with_itself() {
        assert_eq!(range_intersection(range1_4(), range1_4()), Some(range1_4()));
    }

    #[test]
    fn should_intersect_irrespective_of_order() {
        assert_eq!(
            range_intersection(range1_4(), range_inclusive(2.0, 3.0)),
            Some(range_inclusive(2.0, 3.0))
        );
        assert_eq!(
            range_intersection(range_inclusive(2.0, 3.0), range1_4()),
            Some(range_inclusive(2.0, 3.0))
        );
        assert_eq!(
            range_intersection(range1_4(), range_inclusive(3.0, 5.0)),
            Some(range_inclusive(3.0, 4.0))
        );
        assert_eq!(
            range_intersection(range_inclusive(3.0, 5.0), range1_4()),
            Some(range_inclusive(3.0, 4.0))
        );
    }

    #[test]
    fn should_intersect_at_the_edge() {
        assert_eq!(
            range_intersection(range1_4(), range_inclusive(4.0, 5.0)),
            Some(range_inclusive(4.0, 4.0))
        );
    }

    #[test]
    fn should_not_intersect() {
        assert_eq!(
            range_intersection(range1_4(), range_inclusive(5.0, 7.0)),
            None
        );
    }
}

mod segment_test {
    use super::*;

    // describe("line-segment intersections")
    #[test]
    fn should_correctly_detect_intersection() {
        assert_eq!(
            line_segment_intersection_points(
                line_segment(pt(0.0, 0.0), pt(5.0, 0.0)),
                line_segment(pt(2.0, -2.0), pt(3.0, 2.0)),
            ),
            Some(pt(2.5, 0.0))
        );
    }

    #[test]
    fn should_correctly_detect_non_intersection() {
        assert_eq!(
            line_segment_intersection_points(
                line_segment(pt(0.0, 0.0), pt(5.0, 0.0)),
                line_segment(pt(3.0, 1.0), pt(4.0, 4.0)),
            ),
            None
        );
    }

    // describe("isLineSegment validation")
    fn point_value(x: f64, y: f64) -> Unknown {
        Unknown::Array(vec![Unknown::Number(x), Unknown::Number(y)])
    }

    #[test]
    fn should_return_true_for_a_valid_segment() {
        let segment = Unknown::Array(vec![point_value(0.0, 0.0), point_value(1.0, 1.0)]);
        assert!(is_line_segment(&segment));
    }

    #[test]
    fn should_return_false_if_second_element_is_not_a_point() {
        let invalid = Unknown::Array(vec![
            point_value(0.0, 0.0),
            Unknown::String("not-a-point".into()),
        ]);
        assert!(!is_line_segment(&invalid));
    }

    #[test]
    fn should_return_false_for_wrong_length() {
        assert!(!is_line_segment(&Unknown::Array(vec![point_value(
            0.0, 0.0
        )])));
    }
}

mod vector_test {
    use super::*;

    fn pair(x: f64, y: f64) -> Unknown {
        Unknown::Array(vec![Unknown::Number(x), Unknown::Number(y)])
    }

    // describe("Vector")
    #[test]
    fn is_vector_checks_the_shape() {
        assert!(is_vector(&pair(5.0, 5.0)));
        assert!(is_vector(&pair(-5.0, -5.0)));
        assert!(is_vector(&pair(5.0, 0.5)));
        assert!(!is_vector(&Unknown::Null));
        assert!(!is_vector(&Unknown::Undefined));
        assert!(!is_vector(&pair(5.0, f64::NAN)));
    }
}

mod pca_test {
    use super::*;
    use std::f64::consts::PI;

    fn rotate(points: &[P], angle: f64, scale: f64) -> Vec<P> {
        points
            .iter()
            .map(|q| {
                pt(
                    (q.x * js::cos(angle) - q.y * js::sin(angle)) * scale + 17.0,
                    (q.x * js::sin(angle) + q.y * js::cos(angle)) * scale - 4.0,
                )
            })
            .collect()
    }

    fn horizontal_spread() -> Vec<P> {
        (0..21).map(|i| pt(f64::from(i) - 10.0, 0.0)).collect()
    }

    fn us(coords: &[PrincipalCoords]) -> Vec<f64> {
        coords.iter().map(|[u, _]| *u).collect()
    }

    // describe("centroid")
    #[test]
    fn centroid_averages_the_points() {
        assert_eq!(
            centroid(&[pt(0.0, 0.0), pt(4.0, 0.0), pt(2.0, 6.0)]),
            pt(2.0, 2.0)
        );
    }

    // describe("principalAxes")
    #[test]
    fn principal_axes_finds_the_direction_the_points_spread_along() {
        let axes = principal_axes(&horizontal_spread());
        assert!(close_to(axes.major.x.abs(), 1.0, 2));
        assert!(close_to(axes.major.y, 0.0, 2));
        assert!(axes.major_variance > axes.minor_variance);
    }

    #[test]
    fn principal_axes_tracks_the_points_when_they_are_rotated() {
        let axes = principal_axes(&rotate(&horizontal_spread(), PI / 6.0, 1.0));
        assert!(close_to(axes.major.x.abs(), js::cos(PI / 6.0), 2));
        assert!(close_to(axes.major.y.abs(), js::sin(PI / 6.0), 2));
    }

    #[test]
    fn principal_axes_keeps_the_axes_orthonormal() {
        let mut points = rotate(&horizontal_spread(), 1.1, 1.0);
        points.push(pt(3.0, 9.0));
        let PrincipalAxes { major, minor, .. } = principal_axes(&points);
        assert!(close_to(js::hypot(major.x, major.y), 1.0, 2));
        assert!(close_to(js::hypot(minor.x, minor.y), 1.0, 2));
        assert!(close_to(major.x * minor.x + major.y * minor.y, 0.0, 2));
    }

    #[test]
    fn principal_axes_falls_back_to_the_coordinate_axes_for_an_already_diagonal_covariance() {
        let axes = principal_axes(&[pt(-1.0, 0.0), pt(1.0, 0.0), pt(0.0, -5.0), pt(0.0, 5.0)]);
        assert_eq!(axes.major, vector(0.0, 1.0));
    }

    // describe("principalCoords")
    #[test]
    fn principal_coords_undoes_translation_rotation_and_scale() {
        let points = rotate(&horizontal_spread(), 0.7, 3.0);
        let axes = principal_axes(&points);
        let coords = principal_coords_with(&points, &axes, 1.0 / axes.major_variance.sqrt());
        let mut us = us(&coords);
        us.sort_by(f64::total_cmp);
        assert!(coords.iter().all(|[_, v]| v.abs() < 1e-9));
        assert!(close_to(us[0].abs(), us[us.len() - 1].abs(), 2));
        assert!(close_to(us[0].abs(), 1.65, 1));
    }

    // describe("orientPrincipalAxes")
    #[test]
    fn orient_principal_axes_points_the_major_axis_at_the_dense_end() {
        let lopsided: Vec<P> = (0..5)
            .map(|i| pt(-100.0 + f64::from(i) * 10.0, 0.0))
            .chain((0..30).map(|i| pt(f64::from(i), 0.0)))
            .collect();
        for angle in [0.0, 1.0, 2.5, 4.0] {
            let points = rotate(&lopsided, angle, 1.0);
            let axes = orient_principal_axes(&points, &principal_axes(&points));
            assert!(
                skewness(&us(&principal_coords(&points, &axes))) < 0.0,
                "{angle}"
            );
        }
    }

    #[test]
    fn orient_principal_axes_leaves_a_symmetric_point_set_alone() {
        let spread = horizontal_spread();
        let axes = principal_axes(&spread);
        assert_eq!(orient_principal_axes(&spread, &axes), axes);
    }

    // describe("elongation")
    #[test]
    fn elongation_is_0_for_a_straight_spread_and_1_for_an_isotropic_one() {
        assert!(close_to(
            elongation(&principal_axes(&horizontal_spread())),
            0.0,
            2
        ));
        let circle: Vec<P> = (0..36)
            .map(|i| {
                let a = f64::from(i) * PI / 18.0;
                pt(js::cos(a), js::sin(a))
            })
            .collect();
        assert!(close_to(elongation(&principal_axes(&circle)), 1.0, 2));
    }

    #[test]
    fn elongation_is_invariant_to_rotation_and_scale() {
        let points: Vec<P> = (0..40)
            .map(|i| {
                let t = f64::from(i) / 6.0;
                pt(js::cos(t) * 4.0, js::sin(t))
            })
            .collect();
        assert!(close_to(
            elongation(&principal_axes(&rotate(&points, 0.9, 7.0))),
            elongation(&principal_axes(&points)),
            2
        ));
    }

    // describe("standardizedMoment")
    #[test]
    fn standardized_moment_is_0_for_a_sample_with_no_spread() {
        assert_eq!(standardized_moment(&[3.0, 3.0, 3.0], 3.0), 0.0);
    }

    #[test]
    fn standardized_moment_is_unchanged_by_shifting_and_scaling_the_sample() {
        let sample = [1.0, 2.0, 2.0, 3.0, 9.0, 4.0];
        let moved: Vec<f64> = sample.iter().map(|v| v * 5.0 + 100.0).collect();
        assert!(close_to(
            standardized_moment(&moved, 3.0),
            standardized_moment(&sample, 3.0),
            2
        ));
    }

    // describe("skewness")
    #[test]
    fn skewness_is_0_for_a_symmetric_sample() {
        assert!(close_to(skewness(&[-2.0, -1.0, 0.0, 1.0, 2.0]), 0.0, 2));
    }

    #[test]
    fn skewness_is_positive_when_the_tail_runs_to_the_right_of_the_mass() {
        assert!(skewness(&[1.0, 1.0, 1.0, 1.0, 1.0, 9.0]) > 0.0);
        assert!(skewness(&[-9.0, -1.0, -1.0, -1.0, -1.0, -1.0]) < 0.0);
    }

    // describe("kurtosis")
    #[test]
    fn kurtosis_separates_a_uniform_sample_from_a_normal_ish_one() {
        let uniform: Vec<f64> = (0..101).map(|i| f64::from(i) / 100.0).collect();
        assert!(close_to(kurtosis(&uniform), 1.8, 1));
    }

    #[test]
    fn kurtosis_is_invariant_to_rotation_via_the_principal_frame() {
        let points: Vec<P> = (0..50)
            .map(|i| pt(f64::from(i) - 25.0, f64::from(i % 5 - 2) * 0.3))
            .collect();
        let project = |pts: &[P]| {
            let axes = principal_axes(pts);
            kurtosis(&us(&principal_coords(pts, &axes)))
        };
        assert!(close_to(
            project(&rotate(&points, 1.3, 2.0)),
            project(&points),
            2
        ));
    }
}
