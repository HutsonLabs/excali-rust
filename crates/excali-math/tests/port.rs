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
        assert_eq!(vector_from_point_with(p, p, Some(0.0), default), vector(0.0, 0.0));
        assert_eq!(vector_from_point_with(p, p, Some(f64::NAN), default), vector(0.0, 0.0));
        assert_eq!(vector_from_point_with(p, p, None, default), vector(0.0, 0.0));
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
        assert!(!is_point(&Unknown::Array(vec![num(1.0), num(2.0), num(3.0)])));
        assert!(!is_point(&Unknown::Array(vec![num(1.0), Unknown::String("2".into())])));
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
        assert_eq!(point_from_coords::<Global>(Coord::new(1.0, 2.0)), pt(1.0, 2.0));
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
        assert_eq!(degrees_to_radians(Degrees(180.0)), Radians(std::f64::consts::PI));
        assert_eq!(radians_to_degrees(Radians(std::f64::consts::PI)), Degrees(180.0));
    }

    #[test]
    fn constants_are_upstreams() {
        // utils.ts:1 `PRECISION = 10e-5`
        assert_eq!(PRECISION, 10e-5);
        // constants.ts: Legendre-Gauss n=24, abscissae in +/- pairs, weights sum to 2
        assert_eq!(LEGENDRE_GAUSS_N24_T_VALUES.len(), 24);
        assert_eq!(LEGENDRE_GAUSS_N24_C_VALUES.len(), 24);
        for i in (0..24).step_by(2) {
            assert_eq!(LEGENDRE_GAUSS_N24_T_VALUES[i], -LEGENDRE_GAUSS_N24_T_VALUES[i + 1]);
            assert_eq!(LEGENDRE_GAUSS_N24_C_VALUES[i], LEGENDRE_GAUSS_N24_C_VALUES[i + 1]);
        }
        assert_eq!(LEGENDRE_GAUSS_N24_T_VALUES[1], 0.0640568928626056260850430826247450385909);
        assert_eq!(LEGENDRE_GAUSS_N24_C_VALUES[23], 0.0123412297999871995468056670700372915759);
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
    fn ellipse_and_ranges_expose_their_parts() {
        let e = ellipse(pt(1.0, 2.0), 3.0, 4.0);
        assert_eq!((e.center, e.half_width, e.half_height), (pt(1.0, 2.0), 3.0, 4.0));
        let r = range_inclusive_from_pair([2.0, 3.0]);
        assert_eq!((r.0, r.1), (2.0, 3.0));
        assert!(range_includes_value(2.0, r));
        assert!(!range_includes_value(f64::NAN, r));
    }
}
