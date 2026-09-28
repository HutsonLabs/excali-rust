//! Ports of upstream's `packages/math/tests/*.test.ts` at the pinned commit
//! (curve.test.ts and pca.test.ts belong to ex-202 and shape recognition).
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
                pt(a.cos() * 10.0, a.sin() * 10.0)
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
                pt(a.cos() * 100.0, a.sin() * 100.0)
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
