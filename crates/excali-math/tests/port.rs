//! Behaviour the goldens cannot carry: JavaScript number semantics (NaN,
//! infinities, signed zero, which JSON cannot hold), the typed coordinate
//! spaces that stand in for upstream's branded tuples, and the few inputs on
//! which upstream throws a TypeError.

use excali_math::*;

type P = GlobalPoint;

fn pt(x: f64, y: f64) -> P {
    point_from(x, y)
}

fn num(x: f64) -> Unknown {
    Unknown::Number(x)
}

fn pair(x: f64, y: f64) -> Unknown {
    Unknown::Array(vec![num(x), num(y)])
}

fn is_neg_zero(x: f64) -> bool {
    x == 0.0 && x.is_sign_negative()
}

mod js_semantics {
    use super::*;

    #[test]
    fn hypot_follows_v8() {
        // V8 builtins/math.tq MathHypot: infinity wins over NaN, NaN over
        // finite, zero when every argument is zero, else scaled Kahan sum.
        assert_eq!(js::hypot(3.0, 4.0), 5.0);
        assert_eq!(js::hypot(f64::INFINITY, f64::NAN), f64::INFINITY);
        assert_eq!(js::hypot(f64::NAN, f64::NEG_INFINITY), f64::INFINITY);
        assert!(js::hypot(f64::NAN, 1.0).is_nan());
        assert_eq!(js::hypot(0.0, -0.0), 0.0);
        assert!(!js::hypot(-0.0, -0.0).is_sign_negative());
        assert_eq!(js::hypot(-3.0, -4.0), 5.0);
        assert_eq!(js::hypot(1e200, 1e200), 1e200 * std::f64::consts::SQRT_2);
        assert_eq!(js::hypot(1e-200, 0.0), 1e-200);
    }

    #[test]
    fn round_is_math_round() {
        assert_eq!(js::round(2.5), 3.0);
        assert_eq!(js::round(-2.5), -2.0);
        assert_eq!(js::round(0.49999999999999994), 0.0);
        assert_eq!(js::round(-0.5), 0.0);
        assert!(is_neg_zero(js::round(-0.5)));
        assert!(is_neg_zero(js::round(-0.4)));
        assert!(is_neg_zero(js::round(-0.0)));
        assert_eq!(js::round(4503599627370497.0), 4503599627370497.0);
        assert_eq!(js::round(-4503599627370497.0), -4503599627370497.0);
        assert!(js::round(f64::NAN).is_nan());
        assert_eq!(js::round(f64::INFINITY), f64::INFINITY);
        assert_eq!(js::round(f64::NEG_INFINITY), f64::NEG_INFINITY);
    }

    #[test]
    fn min_and_max_propagate_nan_and_order_signed_zeros() {
        assert!(js::max(f64::NAN, 1.0).is_nan());
        assert!(js::max(1.0, f64::NAN).is_nan());
        assert!(js::min(f64::NAN, 1.0).is_nan());
        assert!(js::min(1.0, f64::NAN).is_nan());
        assert!(!js::max(-0.0, 0.0).is_sign_negative());
        assert!(!js::max(0.0, -0.0).is_sign_negative());
        assert!(is_neg_zero(js::min(0.0, -0.0)));
        assert!(is_neg_zero(js::min(-0.0, 0.0)));
        assert_eq!(js::max(1.0, 2.0), 2.0);
        assert_eq!(js::min(1.0, 2.0), 1.0);
    }

    #[test]
    fn clamp_and_rounding_keep_nan() {
        assert!(clamp(f64::NAN, 0.0, 1.0).is_nan());
        assert!(round(f64::NAN, 2.0, RoundingFn::Round).is_nan());
        assert!(round_to_step(f64::NAN, 5.0, RoundingFn::Floor).is_nan());
        assert!(!is_close_to(f64::NAN, f64::NAN));
    }

    #[test]
    fn round_adds_epsilon_before_scaling() {
        // utils.ts:10 `(value + Number.EPSILON) * multiplier`
        assert_eq!(round(1.005, 2.0, RoundingFn::Round), 1.01);
        assert_eq!(round(1.005, 2.0, RoundingFn::Floor), 1.0);
        assert_eq!(round(1.001, 2.0, RoundingFn::Ceil), 1.01);
        assert_eq!(round_to_step(13.0, 5.0, RoundingFn::Round), 15.0);
        assert_eq!(round_to_step(-13.0, 5.0, RoundingFn::Ceil), -10.0);
    }

    #[test]
    fn zero_or_nan_angle_rotates_nothing() {
        // point.ts:143 `if (!angle) return point;` is also true for NaN
        let p = pt(3.0, -4.0);
        assert_eq!(point_rotate_rads(p, pt(100.0, 100.0), Radians(0.0)), p);
        assert_eq!(point_rotate_rads(p, pt(100.0, 100.0), Radians(-0.0)), p);
        assert_eq!(point_rotate_rads(p, pt(100.0, 100.0), Radians(f64::NAN)), p);
    }

    #[test]
    fn a_zero_or_nan_threshold_is_no_threshold() {
        // vector.ts:34 `if (threshold && ...)`
        let p = pt(1.0, 1.0);
        let default = vector(7.0, 8.0);
        assert_eq!(
            vector_from_point_with(p, p, Some(0.0), default),
            vector(0.0, 0.0)
        );
        assert_eq!(
            vector_from_point_with(p, p, Some(f64::NAN), default),
            vector(0.0, 0.0)
        );
        assert_eq!(
            vector_from_point_with(p, p, None, default),
            vector(0.0, 0.0)
        );
        assert_eq!(vector_from_point_with(p, p, Some(0.5), default), default);
    }
}

mod shape_predicates {
    use super::*;

    #[test]
    fn is_point_accepts_infinities_but_not_nan() {
        assert!(is_point(&pair(1.0, 2.0)));
        assert!(is_point(&pair(f64::INFINITY, 2.0)));
        assert!(!is_point(&pair(f64::NAN, 2.0)));
        assert!(!is_point(&pair(1.0, f64::NAN)));
        assert!(!is_point(&Unknown::Array(vec![num(1.0)])));
        assert!(!is_point(&Unknown::Array(vec![
            num(1.0),
            num(2.0),
            num(3.0)
        ])));
        assert!(!is_point(&Unknown::Array(vec![
            num(1.0),
            Unknown::String("2".into())
        ])));
        assert!(!is_point(&Unknown::Undefined));
        assert!(!is_point(&Unknown::Object));
    }

    #[test]
    fn is_valid_point_needs_finite_numbers() {
        assert!(is_valid_point(&pair(1.0, 2.0)));
        assert!(!is_valid_point(&pair(f64::INFINITY, 2.0)));
        assert!(!is_valid_point(&pair(1.0, f64::NEG_INFINITY)));
        assert!(!is_valid_point(&pair(f64::NAN, 2.0)));
        assert!(!is_valid_point(&Unknown::Null));
    }

    #[test]
    fn is_finite_number_is_typeof_number_and_finite() {
        assert!(is_finite_number(&num(0.0)));
        assert!(is_finite_number(&num(-1e308)));
        assert!(!is_finite_number(&num(f64::NAN)));
        assert!(!is_finite_number(&num(f64::INFINITY)));
        assert!(!is_finite_number(&Unknown::String("3".into())));
        assert!(!is_finite_number(&Unknown::Bool(true)));
        assert!(!is_finite_number(&Unknown::Undefined));
    }

    #[test]
    fn is_vector_matches_is_point() {
        assert!(is_vector(&pair(f64::INFINITY, 0.0)));
        assert!(!is_vector(&pair(0.0, f64::NAN)));
        assert!(!is_vector(&Unknown::Array(vec![])));
    }

    #[test]
    fn is_line_segment_needs_two_points() {
        let seg = Unknown::Array(vec![pair(0.0, 0.0), pair(f64::NAN, 1.0)]);
        assert!(!is_line_segment(&seg));
        let seg = Unknown::Array(vec![pair(0.0, 0.0), pair(f64::INFINITY, 1.0)]);
        assert!(is_line_segment(&seg));
        assert!(!is_line_segment(&pair(0.0, 0.0)));
    }

    #[test]
    fn unknown_converts_from_plain_values() {
        assert_eq!(Unknown::from(2.0), num(2.0));
        assert_eq!(Unknown::from([1.0, 2.0]), pair(1.0, 2.0));
        assert_eq!(Unknown::from("x"), Unknown::String("x".into()));
        assert_eq!(Unknown::from(true), Unknown::Bool(true));
    }
}

mod types {
    use super::*;

    #[test]
    fn points_index_like_upstream_tuples() {
        let p = pt(3.0, 4.0);
        assert_eq!((p[0], p[1]), (3.0, 4.0));
        let v = vector(5.0, 6.0);
        assert_eq!((v[0], v[1]), (5.0, 6.0));
        assert_eq!(P::ORIGIN, pt(0.0, 0.0));
        assert_eq!(Vector::ZERO, vector(0.0, 0.0));
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn a_point_has_two_coordinates() {
        let _ = pt(3.0, 4.0)[2];
    }

    #[test]
    fn translation_moves_between_coordinate_spaces() {
        let global = pt(10.0, 20.0);
        let local: LocalPoint = point_translate(global, vector(-10.0, -20.0));
        assert_eq!((local.x, local.y), (0.0, 0.0));
        let back: GlobalPoint = point_translate(local, vector(10.0, 20.0));
        assert_eq!(back, global);
        let same: GlobalPoint = global.cast();
        assert_eq!(same, global);
    }

    #[test]
    fn point_constructors() {
        assert_eq!(
            point_from_coords::<Global>(Coord::new(1.0, 2.0)),
            pt(1.0, 2.0)
        );
        assert_eq!(point_from_pair::<Global>([1.0, 2.0]), pt(1.0, 2.0));
        assert_eq!(point_from_array::<Global>(&[1.0, 2.0]), Some(pt(1.0, 2.0)));
        assert_eq!(point_from_array::<Global>(&[1.0]), None);
        assert_eq!(point_from_array::<Global>(&[1.0, 2.0, 3.0]), None);
        assert_eq!(Point::<Local>::new(1.0, 2.0), point_from(1.0, 2.0));
    }

    #[test]
    fn angles_convert_and_negate() {
        assert_eq!(-Radians(1.5), Radians(-1.5));
        assert_eq!(-Degrees(90.0), Degrees(-90.0));
        assert_eq!(
            degrees_to_radians(Degrees(180.0)),
            Radians(std::f64::consts::PI)
        );
        assert_eq!(
            radians_to_degrees(Radians(std::f64::consts::PI)),
            Degrees(180.0)
        );
    }

    #[test]
    // upstream's literals, digit for digit
    #[allow(clippy::excessive_precision)]
    fn constants_are_upstreams() {
        // utils.ts:1 `PRECISION = 10e-5`
        assert_eq!(PRECISION, 10e-5);
        // constants.ts: Legendre-Gauss n=24, abscissae in +/- pairs, weights sum to 2
        assert_eq!(LEGENDRE_GAUSS_N24_T_VALUES.len(), 24);
        assert_eq!(LEGENDRE_GAUSS_N24_C_VALUES.len(), 24);
        for i in (0..24).step_by(2) {
            assert_eq!(
                LEGENDRE_GAUSS_N24_T_VALUES[i],
                -LEGENDRE_GAUSS_N24_T_VALUES[i + 1]
            );
            assert_eq!(
                LEGENDRE_GAUSS_N24_C_VALUES[i],
                LEGENDRE_GAUSS_N24_C_VALUES[i + 1]
            );
        }
        assert_eq!(
            LEGENDRE_GAUSS_N24_T_VALUES[1],
            0.0640568928626056260850430826247450385909
        );
        assert_eq!(
            LEGENDRE_GAUSS_N24_C_VALUES[23],
            0.0123412297999871995468056670700372915759
        );
        let sum: f64 = LEGENDRE_GAUSS_N24_C_VALUES.iter().sum();
        assert!((sum - 2.0).abs() < 1e-12);
    }

    #[test]
    fn polygon_closes_itself_once() {
        let square = [pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0), pt(0.0, 10.0)];
        let closed = polygon(&square);
        assert_eq!(closed.len(), 5);
        assert_eq!(closed[4], square[0]);
        assert_eq!(polygon(&closed).len(), 5);
        assert_eq!(polygon_from_points(closed.to_vec()), closed);
        assert_eq!(closed.into_points().len(), 5);
    }

    #[test]
    fn empty_polygons_do_not_panic() {
        // Upstream reads polygon[0] of an empty array and throws a TypeError
        // (polygon.ts:79); the port treats an empty polygon as open with no
        // area and nothing inside it.
        let empty: [P; 0] = [];
        assert!(!polygon_is_closed(&empty));
        assert_eq!(polygon(&empty).len(), 0);
        assert_eq!(polygon_signed_area(&empty), 0.0);
        assert_eq!(polygon_area(&empty), 0.0);
        assert!(!polygon_includes_point(pt(0.0, 0.0), &empty));
        assert!(!polygon_includes_point_non_zero(pt(0.0, 0.0), &empty));
        assert!(convex_hull(&empty).is_empty());
        assert!(simplify_convex_polygon(&empty, 0.4).is_empty());
    }

    #[test]
    fn js_sort_is_stable_and_total_safe() {
        // Consistent comparator: the same order as a stable std sort, over
        // lengths on both sides of V8's small-array (< 8) and min-run (64)
        // cutoffs. The NaN-mixed permutations are pinned against V8 itself
        // in tests/js_sort.rs.
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        for len in 0..200 {
            let items: Vec<(i32, usize)> = (0..len)
                .map(|i| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (((seed >> 33) % 7) as i32, i)
                })
                .collect();
            let mut ours = items.clone();
            js::sort(&mut ours, |a, b| f64::from(a.0 - b.0));
            let mut std_sorted = items.clone();
            std_sorted.sort_by_key(|a| a.0);
            assert_eq!(ours, std_sorted);
        }
        // Always-NaN comparator: nothing moves, as in V8.
        let mut v: Vec<usize> = (0..100).rev().collect();
        js::sort(&mut v, |_, _| f64::NAN);
        assert_eq!(v, (0..100).rev().collect::<Vec<_>>());
        // Mixed NaN: still a permutation of the input (which one is
        // checked against V8 in tests/js_sort.rs).
        let mut w: Vec<f64> = (0..100)
            .map(|i| {
                if i % 4 == 0 {
                    f64::NAN
                } else {
                    f64::from(i * 37 % 101)
                }
            })
            .collect();
        js::sort(&mut w, |a, b| a - b);
        assert_eq!(w.iter().filter(|x| x.is_nan()).count(), 25);
        let mut finite: Vec<f64> = w.into_iter().filter(|x| !x.is_nan()).collect();
        finite.sort_by(f64::total_cmp);
        let mut expected: Vec<f64> = (0..100)
            .filter(|i| i % 4 != 0)
            .map(|i| f64::from(i * 37 % 101))
            .collect();
        expected.sort_by(f64::total_cmp);
        assert_eq!(finite, expected);
    }

    #[test]
    fn hulls_of_points_with_nan_or_infinite_coordinates_do_not_panic() {
        // Upstream sorts with Array.prototype.sort, which never throws when
        // the comparator answers NaN (polygon.ts convexHull); it returns
        // TimSort's order. Rust's slice sort may abort on an inconsistent
        // comparator, so the port must not hand it one. This only checks
        // that nothing panics; tests/js_sort.rs checks the hulls themselves
        // against upstream's.
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let specials = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
        for round in 0..2000 {
            let len = 20 + round % 60;
            let cloud: Vec<P> = (0..len)
                .map(|i| {
                    let (x, y) = (next() * 200.0 - 100.0, next() * 200.0 - 100.0);
                    match i % 3 {
                        0 => pt(specials[(i / 3 + round) % 3], y),
                        1 if round % 2 == 0 => pt(x, f64::NAN),
                        _ => pt(x, y),
                    }
                })
                .collect();
            let hull = convex_hull(&cloud);
            // A chain pops only on cross <= 0, so each keeps at most n - 1.
            assert!(hull.len() <= 2 * cloud.len());
            let _ = simplify_convex_polygon(&hull, 0.4);
            let _ = simplify_convex_polygon(&cloud, 0.4);
        }
        let all_nan = vec![pt(f64::NAN, f64::NAN); 50];
        // As upstream: no cross is <= 0, so each 50-point chain keeps 49.
        assert_eq!(convex_hull(&all_nan).len(), 98);
        let _ = simplify_convex_polygon(&all_nan, 0.4);
    }

    #[test]
    fn ellipse_and_ranges_expose_their_parts() {
        let e = ellipse(pt(1.0, 2.0), 3.0, 4.0);
        assert_eq!(
            (e.center, e.half_width, e.half_height),
            (pt(1.0, 2.0), 3.0, 4.0)
        );
        let r = range_inclusive_from_pair([2.0, 3.0]);
        assert_eq!((r.0, r.1), (2.0, 3.0));
        assert!(range_includes_value(2.0, r));
        assert!(!range_includes_value(f64::NAN, r));
    }
}

mod curves {
    use super::*;

    fn s_curve() -> Curve<Global> {
        curve(
            pt(-50.0, -50.0),
            pt(10.0, -50.0),
            pt(10.0, 50.0),
            pt(50.0, 50.0),
        )
    }

    #[test]
    fn pow_is_the_exponent_operator() {
        // `**` and Math.pow (ES Number::exponentiate): a NaN exponent gives
        // NaN, a zero exponent gives 1 even for NaN, and 1 or -1 to an
        // infinite power is NaN where C's pow answers 1.
        assert_eq!(js::pow(2.0, 3.0), 8.0);
        assert_eq!(js::pow(f64::NAN, 0.0), 1.0);
        assert!(js::pow(1.0, f64::NAN).is_nan());
        assert!(js::pow(1.0, f64::INFINITY).is_nan());
        assert!(js::pow(-1.0, f64::NEG_INFINITY).is_nan());
        assert_eq!(js::pow(0.5, f64::INFINITY), 0.0);
        let x = 0.1 + 0.2;
        assert_eq!(js::pow(x, 2.0), x * x);
    }

    #[test]
    fn curves_index_like_upstream_tuples() {
        let c = s_curve();
        assert_eq!(c[0], pt(-50.0, -50.0));
        assert_eq!(c[3][1], 50.0);
        assert_eq!(c.points(), [c.0, c.1, c.2, c.3]);
        let local: Curve<Local> = curve(
            point_from(0.0, 0.0),
            point_from(0.0, 0.0),
            point_from(3.0, 4.0),
            point_from(3.0, 4.0),
        );
        assert_eq!(bezier_equation(local, 1.0), point_from(3.0, 4.0));
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn a_curve_has_four_points() {
        let _ = s_curve()[4];
    }

    #[test]
    fn is_curve_needs_four_points() {
        let p = |x: f64| pair(x, x);
        assert!(is_curve(&Unknown::Array(vec![
            p(0.0),
            p(1.0),
            p(2.0),
            p(3.0)
        ])));
        assert!(is_curve(&Unknown::Array(vec![
            p(0.0),
            p(1.0),
            p(2.0),
            p(f64::INFINITY)
        ])));
        assert!(!is_curve(&Unknown::Array(vec![
            p(0.0),
            p(1.0),
            p(2.0),
            p(f64::NAN)
        ])));
        assert!(!is_curve(&Unknown::Array(vec![p(0.0), p(1.0), p(2.0)])));
        assert!(!is_curve(&Unknown::Undefined));
    }

    #[test]
    fn zero_steps_divide_zero_by_zero() {
        // curve.ts `const t = i / steps` is 0 / 0 = NaN for i = 0.
        let offsets = curve_offset_points_with(s_curve(), 2.0, 0);
        assert_eq!(offsets.len(), 1);
        assert!(offsets[0].x.is_nan() && offsets[0].y.is_nan());
        let offsets = offset_points_for_quadratic_bezier_with(
            pt(0.0, 0.0),
            pt(50.0, 100.0),
            pt(100.0, 0.0),
            5.0,
            0,
        );
        assert_eq!(offsets.len(), 1);
        assert!(offsets[0].x.is_nan() && offsets[0].y.is_nan());
        assert_eq!(curve_offset_points(s_curve(), 2.0).len(), 51);
    }

    #[test]
    fn catmull_rom_needs_two_points() {
        assert_eq!(curve_catmull_rom_cubic_approx_points::<Global>(&[]), None);
        assert_eq!(
            curve_catmull_rom_quadratic_approx_points(&[pt(1.0, 2.0)]),
            None
        );
        let two = [pt(0.0, 0.0), pt(10.0, 0.0)];
        assert_eq!(
            curve_catmull_rom_cubic_approx_points(&two).map(|c| c.len()),
            Some(1)
        );
        // cp = p1 + (p2 - p0) * tension / 2 with p0 = p1 at the start
        assert_eq!(
            curve_catmull_rom_quadratic_approx_points(&two),
            Some(vec![[pt(2.5, 0.0), pt(10.0, 0.0)]])
        );
    }

    #[test]
    fn a_nan_tolerance_skips_the_bisection() {
        // `while (n - m > e)` never runs, so `param ?? closestStep /
        // maxSteps` falls back to the closest of the 31 samples.
        let c = s_curve();
        let t = curve_closest_parameter_with(c, pt(0.0, 0.0), f64::NAN);
        assert_eq!(t * 30.0, (t * 30.0).round());
        assert_eq!(t, 0.4666666666666667, "upstream under Node 26");
        assert_eq!(
            curve_closest_parameter_with(c, pt(0.0, 0.0), 0.5),
            t,
            "a window wider than two samples also skips it"
        );
    }

    #[test]
    fn a_nan_distance_never_becomes_the_minimum() {
        // `if (d < min)` is false for NaN: step 0 is kept, so the window is
        // [0, 1/30]. `f(k - e) < f(k + e)` is false too, so every bisection
        // step moves the lower end up.
        let c = s_curve();
        let (mut m, n, e) = (0.0, 1.0 / 30.0, 1e-3);
        let mut k = f64::NAN;
        while n - m > e {
            k = (n + m) / 2.0;
            m = k;
        }
        assert!(k > 0.03 && k < n);
        assert_eq!(k, 0.032812499999999994, "upstream under Node 26");
        assert_eq!(curve_closest_parameter(c, pt(f64::NAN, 0.0)), k);
    }

    #[test]
    fn a_nan_intersection_tolerance_accepts_the_first_guess() {
        // `while (error >= tolerance)` is false for NaN, so the first
        // initial guess [0.5, 0] comes back without a Newton step.
        let c = s_curve();
        let far = line_segment(pt(1000.0, 1000.0), pt(2000.0, 1000.0));
        let hits = curve_intersect_line_segment_with(
            c,
            far,
            CurveIntersectOptions {
                tolerance: Some(f64::NAN),
                iter_limit: None,
            },
        );
        assert_eq!(hits, vec![bezier_equation(c, 0.5)]);
        let none = curve_intersect_line_segment_with(
            c,
            line_segment(pt(10.0, -60.0), pt(10.0, 60.0)),
            CurveIntersectOptions {
                tolerance: None,
                iter_limit: Some(0),
            },
        );
        assert!(none.is_empty(), "iterLimit 0 fails every guess");
        assert_eq!(CurveIntersectOptions::default().tolerance, None);
    }

    #[test]
    fn a_nan_solution_passes_the_range_check() {
        // A NaN control point makes the error NaN, which ends the Newton
        // loop with t = s = NaN; `t < 0 || t > 1 || ...` is false for NaN,
        // so upstream returns the NaN point rather than no intersection.
        let c = curve(
            pt(f64::NAN, 0.0),
            pt(10.0, -50.0),
            pt(10.0, 50.0),
            pt(50.0, 50.0),
        );
        let hits = curve_intersect_line_segment(c, line_segment(pt(10.0, -60.0), pt(10.0, 60.0)));
        assert_eq!(hits.len(), 1);
        assert!(hits[0].x.is_nan() && hits[0].y.is_nan());
    }

    #[test]
    fn nan_parameters_propagate() {
        let c = s_curve();
        assert!(curve_length_at_parameter(c, f64::NAN).is_nan());
        let p = curve_point_at_length(c, f64::NAN);
        assert!(p.x.is_nan() && p.y.is_nan());
        assert_eq!(curve_length_at_parameter(c, -1.0), 0.0);
        assert_eq!(curve_length_at_parameter(c, 2.0), curve_length(c));
        assert_eq!(curve_point_at_length(c, -1.0), c.0);
        assert_eq!(curve_point_at_length(c, 2.0), c.3);
    }

    #[test]
    fn length_of_a_straight_curve_is_its_chord() {
        let c = curve(pt(0.0, 0.0), pt(10.0, 0.0), pt(20.0, 0.0), pt(30.0, 0.0));
        assert!((curve_length(c) - 30.0).abs() < 1e-12);
        assert!((curve_length_at_parameter(c, 0.5) - 15.0).abs() < 1e-12);
        let mid = curve_point_at_length(c, 0.5);
        assert!((mid.x - 15.0).abs() < 30.0 * 1e-4 && mid.y == 0.0);
    }
}
