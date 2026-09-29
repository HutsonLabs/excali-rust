//! Parity with upstream's own `packages/math/src`.
//!
//! `goldens/math.json` is written by `tools/goldens/generate.mjs`, which runs
//! upstream's TypeScript from the pinned checkout under Node: every case is a
//! call `math[fn](...args)` and the value upstream returned. The file covers
//! every function the package exports except `pca.ts`
//! (`tools/goldens/test/math.test.mjs` checks that against the checkout); this
//! test checks that the Rust port returns the same doubles, compared with `==`
//! (so `-0` equals `0`, as JSON cannot tell them apart).
//!
//! That includes the functions whose upstream result goes through
//! `Math.sin`, `Math.cos`, `Math.atan2` or `Math.pow` (`pointRotateRads`,
//! `cartesian2Polar`, `simplifyConvexPolygon`, `bezierEquation` and the
//! curve functions built on it, the ellipse functions): `excali_math::js`
//! computes V8's own fdlibm, bit for bit and the same on every platform
//! (ex-009), where the platform's libm was one ulp away from V8 on a few
//! percent of arguments. `Math.pow` is the platform's `pow` in V8; the
//! generator leaves out the cases where macOS and glibc disagree
//! (`PLATFORM_DEPENDENT_CASES` in tools/goldens/math.mjs), and on the rest
//! the correctly rounded `js::pow` agrees with both.

use std::collections::BTreeSet;
use std::path::Path;

use excali_math::*;
use serde_json::{json, Value};

type P = GlobalPoint;

fn load() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../goldens/math.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run node tools/goldens/generate.mjs)",
            path.display()
        )
    });
    let doc: Value = serde_json::from_str(&text).expect("math.json parses");
    doc["cases"].as_array().expect("cases").clone()
}

// -- arguments -----------------------------------------------------------------

fn f(v: &Value) -> f64 {
    v.as_f64()
        .unwrap_or_else(|| panic!("number expected, got {v}"))
}

fn p(v: &Value) -> P {
    point_from(f(&v[0]), f(&v[1]))
}

fn vec2(v: &Value) -> Vector {
    vector(f(&v[0]), f(&v[1]))
}

fn seg(v: &Value) -> LineSegment<Global> {
    line_segment(p(&v[0]), p(&v[1]))
}

fn ln(v: &Value) -> Line<Global> {
    line(p(&v[0]), p(&v[1]))
}

fn points(v: &Value) -> Vec<P> {
    v.as_array().expect("point list").iter().map(p).collect()
}

fn ell(v: &Value) -> Ellipse<Global> {
    ellipse(p(&v["center"]), f(&v["halfWidth"]), f(&v["halfHeight"]))
}

fn range(v: &Value) -> InclusiveRange {
    range_inclusive(f(&v[0]), f(&v[1]))
}

fn rect(v: &Value) -> Rectangle<Global> {
    rectangle(p(&v[0]), p(&v[1]))
}

fn crv(v: &Value) -> Curve<Global> {
    curve(p(&v[0]), p(&v[1]), p(&v[2]), p(&v[3]))
}

fn steps(v: &Value) -> u32 {
    u32::try_from(v.as_u64().expect("integer step count")).expect("u32 step count")
}

/// `curveIntersectLineSegment`'s `{ tolerance, iterLimit }` object.
fn intersect_options(v: &Value) -> CurveIntersectOptions {
    CurveIntersectOptions {
        tolerance: v.get("tolerance").map(f),
        iter_limit: v.get("iterLimit").map(steps),
    }
}

fn rounding(v: &Value) -> RoundingFn {
    match v.as_str() {
        Some("round") => RoundingFn::Round,
        Some("floor") => RoundingFn::Floor,
        Some("ceil") => RoundingFn::Ceil,
        _ => panic!("rounding function expected, got {v}"),
    }
}

/// A JSON value as the untyped JS value the shape predicates receive.
fn unknown(v: &Value) -> Unknown {
    match v {
        Value::Null => Unknown::Null,
        Value::Bool(b) => Unknown::Bool(*b),
        Value::Number(n) => Unknown::Number(n.as_f64().expect("f64")),
        Value::String(s) => Unknown::String(s.clone()),
        Value::Array(items) => Unknown::Array(items.iter().map(unknown).collect()),
        Value::Object(_) => Unknown::Object,
    }
}

// -- results -------------------------------------------------------------------

fn out_p<S: Space>(q: Point<S>) -> Value {
    json!([q.x, q.y])
}

fn out_v(v: Vector) -> Value {
    json!([v.x, v.y])
}

fn out_points<S: Space>(ps: &[Point<S>]) -> Value {
    Value::Array(ps.iter().copied().map(out_p).collect())
}

fn out_opt_p(q: Option<P>) -> Value {
    q.map_or(Value::Null, out_p)
}

fn out_seg<S: Space>(s: LineSegment<S>) -> Value {
    json!([out_p(s.0), out_p(s.1)])
}

fn out_range(r: InclusiveRange) -> Value {
    json!([r.0, r.1])
}

fn out_curve<S: Space>(c: Curve<S>) -> Value {
    json!([out_p(c.0), out_p(c.1), out_p(c.2), out_p(c.3)])
}

fn call(fun: &str, a: &[Value]) -> Value {
    let arg = |i: usize| &a[i];
    let has = |i: usize| a.len() > i;
    match fun {
        // utils.ts
        "clamp" => json!(clamp(f(arg(0)), f(arg(1)), f(arg(2)))),
        "round" => json!(round(f(arg(0)), f(arg(1)), rounding(arg(2)))),
        "roundToStep" => json!(round_to_step(f(arg(0)), f(arg(1)), rounding(arg(2)))),
        "average" => json!(average(f(arg(0)), f(arg(1)))),
        "isFiniteNumber" => json!(is_finite_number(&unknown(arg(0)))),
        "isCloseTo" if has(2) => json!(is_close_to_with(f(arg(0)), f(arg(1)), f(arg(2)))),
        "isCloseTo" => json!(is_close_to(f(arg(0)), f(arg(1)))),
        // angle.ts
        "normalizeRadians" => json!(normalize_radians(Radians(f(arg(0)))).0),
        "cartesian2Polar" => {
            let polar = cartesian2_polar(p(arg(0)));
            json!([polar.radius, polar.angle.0])
        }
        "degreesToRadians" => json!(degrees_to_radians(Degrees(f(arg(0)))).0),
        "radiansToDegrees" => json!(radians_to_degrees(Radians(f(arg(0)))).0),
        "isRightAngleRads" => json!(is_right_angle_rads(Radians(f(arg(0))))),
        "radiansBetweenAngles" => json!(radians_between_angles(
            Radians(f(arg(0))),
            Radians(f(arg(1))),
            Radians(f(arg(2)))
        )),
        "radiansDifference" => {
            json!(radians_difference(Radians(f(arg(0))), Radians(f(arg(1)))).0)
        }
        // point.ts
        "pointFrom" if arg(0).is_object() => out_p::<Global>(point_from_coords(Coord::new(
            f(&arg(0)["x"]),
            f(&arg(0)["y"]),
        ))),
        "pointFrom" => out_p(point_from::<Global>(f(arg(0)), f(arg(1)))),
        "pointFromArray" => {
            let numbers: Vec<f64> = arg(0).as_array().expect("array").iter().map(f).collect();
            point_from_array::<Global>(&numbers).map_or(Value::Null, out_p)
        }
        "pointFromPair" => out_p(point_from_pair::<Global>([f(&arg(0)[0]), f(&arg(0)[1])])),
        "pointFromVector" if has(1) => out_p(point_from_vector(vec2(arg(0)), p(arg(1)))),
        "pointFromVector" => out_p(point_from_vector(vec2(arg(0)), P::ORIGIN)),
        "isPoint" => json!(is_point(&unknown(arg(0)))),
        "pointsEqual" if has(2) => json!(points_equal_with(p(arg(0)), p(arg(1)), f(arg(2)))),
        "pointsEqual" => json!(points_equal(p(arg(0)), p(arg(1)))),
        "pointRotateRads" => out_p(point_rotate_rads(p(arg(0)), p(arg(1)), Radians(f(arg(2))))),
        "pointRotateDegs" => out_p(point_rotate_degs(p(arg(0)), p(arg(1)), Degrees(f(arg(2))))),
        "pointTranslate" if has(1) => {
            out_p(point_translate::<Global, Local>(p(arg(0)), vec2(arg(1))))
        }
        "pointTranslate" => out_p(point_translate::<Global, Global>(p(arg(0)), Vector::ZERO)),
        "pointCenter" => out_p(point_center(p(arg(0)), p(arg(1)))),
        "pointDistance" => json!(point_distance(p(arg(0)), p(arg(1)))),
        "pointDistanceSq" => json!(point_distance_sq(p(arg(0)), p(arg(1)))),
        "pointScaleFromOrigin" => out_p(point_scale_from_origin(p(arg(0)), p(arg(1)), f(arg(2)))),
        "isPointWithinBounds" => json!(is_point_within_bounds(p(arg(0)), p(arg(1)), p(arg(2)))),
        "isValidPoint" => json!(is_valid_point(&unknown(arg(0)))),
        // vector.ts
        "vector" => {
            let origin_x = if has(2) { f(arg(2)) } else { 0.0 };
            let origin_y = if has(3) { f(arg(3)) } else { 0.0 };
            out_v(vector_from_origin(f(arg(0)), f(arg(1)), origin_x, origin_y))
        }
        "vectorFromPoint" => {
            let origin = if has(1) { p(arg(1)) } else { P::ORIGIN };
            if has(2) {
                let default = if has(3) {
                    vec2(arg(3))
                } else {
                    vector(0.0, 1.0)
                };
                out_v(vector_from_point_with(
                    p(arg(0)),
                    origin,
                    Some(f(arg(2))),
                    default,
                ))
            } else {
                out_v(vector_from_point(p(arg(0)), origin))
            }
        }
        "vectorCross" => json!(vector_cross(vec2(arg(0)), vec2(arg(1)))),
        "vectorDot" => json!(vector_dot(vec2(arg(0)), vec2(arg(1)))),
        "isVector" => json!(is_vector(&unknown(arg(0)))),
        "vectorAdd" => out_v(vector_add(vec2(arg(0)), vec2(arg(1)))),
        "vectorSubtract" => out_v(vector_subtract(vec2(arg(0)), vec2(arg(1)))),
        "vectorScale" => out_v(vector_scale(vec2(arg(0)), f(arg(1)))),
        "vectorMagnitudeSq" => json!(vector_magnitude_sq(vec2(arg(0)))),
        "vectorMagnitude" => json!(vector_magnitude(vec2(arg(0)))),
        "vectorNormalize" => out_v(vector_normalize(vec2(arg(0)))),
        "vectorNormal" => out_v(vector_normal(vec2(arg(0)))),
        // line.ts
        "line" => {
            let l = line(p(arg(0)), p(arg(1)));
            json!([out_p(l.0), out_p(l.1)])
        }
        "linesIntersectAt" => out_opt_p(lines_intersect_at(ln(arg(0)), ln(arg(1)))),
        // segment.ts
        "lineSegment" => out_seg(line_segment(p(arg(0)), p(arg(1)))),
        "isLineSegment" => json!(is_line_segment(&unknown(arg(0)))),
        "lineSegmentRotate" => {
            let origin = if has(2) { Some(p(arg(2))) } else { None };
            out_seg(line_segment_rotate(seg(arg(0)), Radians(f(arg(1))), origin))
        }
        "segmentsIntersectAt" => out_opt_p(segments_intersect_at(seg(arg(0)), seg(arg(1)))),
        "pointOnLineSegment" if has(2) => {
            json!(point_on_line_segment_with(
                p(arg(0)),
                seg(arg(1)),
                f(arg(2))
            ))
        }
        "pointOnLineSegment" => json!(point_on_line_segment(p(arg(0)), seg(arg(1)))),
        "distanceToLineSegment" => json!(distance_to_line_segment(p(arg(0)), seg(arg(1)))),
        "lineSegmentPointAt" => out_p(line_segment_point_at(seg(arg(0)), f(arg(1)))),
        "lineSegmentIntersectionPoints" if has(2) => out_opt_p(
            line_segment_intersection_points_with(seg(arg(0)), seg(arg(1)), f(arg(2))),
        ),
        "lineSegmentIntersectionPoints" => {
            out_opt_p(line_segment_intersection_points(seg(arg(0)), seg(arg(1))))
        }
        "lineSegmentsDistance" => json!(line_segments_distance(seg(arg(0)), seg(arg(1)))),
        "lineSegmentClosestParameter" => {
            json!(line_segment_closest_parameter(p(arg(0)), seg(arg(1))))
        }
        // ellipse.ts
        "ellipse" => {
            let e = ellipse(p(arg(0)), f(arg(1)), f(arg(2)));
            json!({ "center": out_p(e.center), "halfWidth": e.half_width, "halfHeight": e.half_height })
        }
        "ellipseIncludesPoint" => json!(ellipse_includes_point(p(arg(0)), ell(arg(1)))),
        "ellipseTouchesPoint" if has(2) => {
            json!(ellipse_touches_point_with(
                p(arg(0)),
                ell(arg(1)),
                f(arg(2))
            ))
        }
        "ellipseTouchesPoint" => json!(ellipse_touches_point(p(arg(0)), ell(arg(1)))),
        "ellipseDistanceFromPoint" => json!(ellipse_distance_from_point(p(arg(0)), ell(arg(1)))),
        "ellipseSegmentInterceptPoints" => {
            out_points(&ellipse_segment_intercept_points(ell(arg(0)), seg(arg(1))))
        }
        "ellipseLineIntersectionPoints" => {
            out_points(&ellipse_line_intersection_points(ell(arg(0)), ln(arg(1))))
        }
        // polygon.ts
        "polygon" => {
            let ps: Vec<P> = a.iter().map(p).collect();
            out_points(&polygon(&ps))
        }
        "polygonFromPoints" => out_points(&polygon_from_points(points(arg(0)))),
        "polygonIncludesPoint" => {
            json!(polygon_includes_point(p(arg(0)), &points(arg(1))))
        }
        "polygonIncludesPointNonZero" => {
            json!(polygon_includes_point_non_zero(p(arg(0)), &points(arg(1))))
        }
        "polygonIsClosed" if has(1) => json!(polygon_is_closed_with(&points(arg(0)), f(arg(1)))),
        "polygonIsClosed" => json!(polygon_is_closed(&points(arg(0)))),
        "polygonSignedArea" if has(1) => {
            json!(polygon_signed_area_with(&points(arg(0)), f(arg(1))))
        }
        "polygonSignedArea" => json!(polygon_signed_area(&points(arg(0)))),
        "polygonArea" if has(1) => json!(polygon_area_with(&points(arg(0)), f(arg(1)))),
        "polygonArea" => json!(polygon_area(&points(arg(0)))),
        "convexHull" => out_points(&convex_hull(&points(arg(0)))),
        "simplifyConvexPolygon" => out_points(&simplify_convex_polygon(&points(arg(0)), f(arg(1)))),
        // range.ts
        "rangeInclusive" => out_range(range_inclusive(f(arg(0)), f(arg(1)))),
        "rangeInclusiveFromPair" => {
            out_range(range_inclusive_from_pair([f(&arg(0)[0]), f(&arg(0)[1])]))
        }
        "rangesOverlap" => json!(ranges_overlap(range(arg(0)), range(arg(1)))),
        "rangeIntersection" => {
            range_intersection(range(arg(0)), range(arg(1))).map_or(Value::Null, out_range)
        }
        "rangeIncludesValue" => json!(range_includes_value(f(arg(0)), range(arg(1)))),
        // rectangle.ts
        "rectangle" => {
            let r = rectangle(p(arg(0)), p(arg(1)));
            json!([out_p(r.0), out_p(r.1)])
        }
        "rectangleFromNumberSequence" => {
            let r = rectangle_from_number_sequence::<Global>(
                f(arg(0)),
                f(arg(1)),
                f(arg(2)),
                f(arg(3)),
            );
            json!([out_p(r.0), out_p(r.1)])
        }
        "rectangleIntersectLineSegment" => {
            out_points(&rectangle_intersect_line_segment(rect(arg(0)), seg(arg(1))))
        }
        "rectangleIntersectRectangle" => {
            json!(rectangle_intersect_rectangle(rect(arg(0)), rect(arg(1))))
        }
        // triangle.ts
        "triangleIncludesPoint" => {
            let t = arg(0);
            json!(triangle_includes_point(
                Triangle(p(&t[0]), p(&t[1]), p(&t[2])),
                p(arg(1))
            ))
        }
        // curve.ts
        "curve" => out_curve(curve(p(arg(0)), p(arg(1)), p(arg(2)), p(arg(3)))),
        "bezierEquation" => out_p(bezier_equation(crv(arg(0)), f(arg(1)))),
        "curveIntersectLineSegment" if has(2) => out_points(&curve_intersect_line_segment_with(
            crv(arg(0)),
            seg(arg(1)),
            intersect_options(arg(2)),
        )),
        "curveIntersectLineSegment" => {
            out_points(&curve_intersect_line_segment(crv(arg(0)), seg(arg(1))))
        }
        "curveClosestParameter" if has(2) => json!(curve_closest_parameter_with(
            crv(arg(0)),
            p(arg(1)),
            f(arg(2))
        )),
        "curveClosestParameter" => json!(curve_closest_parameter(crv(arg(0)), p(arg(1)))),
        "curveClosestPoint" if has(2) => {
            out_p(curve_closest_point_with(crv(arg(0)), p(arg(1)), f(arg(2))))
        }
        "curveClosestPoint" => out_p(curve_closest_point(crv(arg(0)), p(arg(1)))),
        "curvePointDistance" if has(2) => {
            json!(curve_point_distance_with(crv(arg(0)), p(arg(1)), f(arg(2))))
        }
        "curvePointDistance" => json!(curve_point_distance(crv(arg(0)), p(arg(1)))),
        "isCurve" => json!(is_curve(&unknown(arg(0)))),
        "curveTangent" => out_v(curve_tangent(crv(arg(0)), f(arg(1)))),
        "curveCatmullRomQuadraticApproxPoints" => {
            let ps = points(arg(0));
            let sets = if has(1) {
                curve_catmull_rom_quadratic_approx_points_with(&ps, f(arg(1)))
            } else {
                curve_catmull_rom_quadratic_approx_points(&ps)
            };
            sets.map_or(Value::Null, |sets| {
                Value::Array(
                    sets.iter()
                        .map(|[a, b]| json!([out_p(*a), out_p(*b)]))
                        .collect(),
                )
            })
        }
        "curveCatmullRomCubicApproxPoints" => {
            let ps = points(arg(0));
            let curves = if has(1) {
                curve_catmull_rom_cubic_approx_points_with(&ps, f(arg(1)))
            } else {
                curve_catmull_rom_cubic_approx_points(&ps)
            };
            curves.map_or(Value::Null, |curves| {
                Value::Array(curves.into_iter().map(out_curve).collect())
            })
        }
        "curveOffsetPoints" if has(2) => out_points(&curve_offset_points_with(
            crv(arg(0)),
            f(arg(1)),
            steps(arg(2)),
        )),
        "curveOffsetPoints" => out_points(&curve_offset_points(crv(arg(0)), f(arg(1)))),
        "offsetPointsForQuadraticBezier" if has(4) => {
            out_points(&offset_points_for_quadratic_bezier_with(
                p(arg(0)),
                p(arg(1)),
                p(arg(2)),
                f(arg(3)),
                steps(arg(4)),
            ))
        }
        "offsetPointsForQuadraticBezier" => out_points(&offset_points_for_quadratic_bezier(
            p(arg(0)),
            p(arg(1)),
            p(arg(2)),
            f(arg(3)),
        )),
        "curveLength" => json!(curve_length(crv(arg(0)))),
        "curveLengthAtParameter" => json!(curve_length_at_parameter(crv(arg(0)), f(arg(1)))),
        "curvePointAtLength" if has(2) => out_p(curve_point_at_length_with(
            crv(arg(0)),
            f(arg(1)),
            f(arg(2)),
        )),
        "curvePointAtLength" => out_p(curve_point_at_length(crv(arg(0)), f(arg(1)))),
        other => panic!("math.json calls {other}, which the port does not cover"),
    }
}

/// Structural equality with numbers compared exactly as doubles (`==`, so
/// `-0` equals `0`, which JSON cannot tell apart).
fn same(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().expect("f64") == b.as_f64().expect("f64")
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| same(v, w)))
        }
        _ => actual == expected,
    }
}

#[test]
fn every_case_matches_upstream() {
    let cases = load();
    assert!(cases.len() > 1000, "math.json has {} cases", cases.len());
    let mut failures = Vec::new();
    for c in &cases {
        let fun = c["fn"].as_str().expect("fn");
        let args = c["args"].as_array().expect("args");
        let actual = call(fun, args);
        if !same(&actual, &c["result"]) {
            failures.push(format!(
                "{}: {fun}{} = {actual}, upstream {}",
                c["id"].as_str().unwrap_or("?"),
                Value::Array(args.clone()),
                c["result"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from upstream:\n{}",
        failures.len(),
        cases.len(),
        failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn every_covered_function_has_cases() {
    let seen: BTreeSet<String> = load()
        .iter()
        .map(|c| c["fn"].as_str().expect("fn").to_owned())
        .collect();
    // The upstream exports (tools/goldens/math.mjs MATH_FUNCTIONS, checked
    // against the checkout by tools/goldens/test/math.test.mjs).
    let expected = [
        "average",
        "bezierEquation",
        "curve",
        "curveCatmullRomCubicApproxPoints",
        "curveCatmullRomQuadraticApproxPoints",
        "curveClosestParameter",
        "curveClosestPoint",
        "curveIntersectLineSegment",
        "curveLength",
        "curveLengthAtParameter",
        "curveOffsetPoints",
        "curvePointAtLength",
        "curvePointDistance",
        "curveTangent",
        "isCurve",
        "offsetPointsForQuadraticBezier",
        "cartesian2Polar",
        "clamp",
        "convexHull",
        "degreesToRadians",
        "distanceToLineSegment",
        "ellipse",
        "ellipseDistanceFromPoint",
        "ellipseIncludesPoint",
        "ellipseLineIntersectionPoints",
        "ellipseSegmentInterceptPoints",
        "ellipseTouchesPoint",
        "isCloseTo",
        "isFiniteNumber",
        "isLineSegment",
        "isPoint",
        "isPointWithinBounds",
        "isRightAngleRads",
        "isValidPoint",
        "isVector",
        "line",
        "lineSegment",
        "lineSegmentClosestParameter",
        "lineSegmentIntersectionPoints",
        "lineSegmentPointAt",
        "lineSegmentRotate",
        "lineSegmentsDistance",
        "linesIntersectAt",
        "normalizeRadians",
        "pointCenter",
        "pointDistance",
        "pointDistanceSq",
        "pointFrom",
        "pointFromArray",
        "pointFromPair",
        "pointFromVector",
        "pointOnLineSegment",
        "pointRotateDegs",
        "pointRotateRads",
        "pointScaleFromOrigin",
        "pointTranslate",
        "pointsEqual",
        "polygon",
        "polygonArea",
        "polygonFromPoints",
        "polygonIncludesPoint",
        "polygonIncludesPointNonZero",
        "polygonIsClosed",
        "polygonSignedArea",
        "radiansBetweenAngles",
        "radiansDifference",
        "radiansToDegrees",
        "rangeIncludesValue",
        "rangeInclusive",
        "rangeInclusiveFromPair",
        "rangeIntersection",
        "rangesOverlap",
        "rectangle",
        "rectangleFromNumberSequence",
        "rectangleIntersectLineSegment",
        "rectangleIntersectRectangle",
        "round",
        "roundToStep",
        "segmentsIntersectAt",
        "simplifyConvexPolygon",
        "triangleIncludesPoint",
        "vector",
        "vectorAdd",
        "vectorCross",
        "vectorDot",
        "vectorFromPoint",
        "vectorMagnitude",
        "vectorMagnitudeSq",
        "vectorNormal",
        "vectorNormalize",
        "vectorScale",
        "vectorSubtract",
    ];
    let expected: BTreeSet<String> = expected.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(seen, expected);
}

/// Every case, the trigonometric and `pow` ones included, is compared to
/// the last bit (ex-009).
#[test]
fn comparison_is_exact() {
    assert!(same(&json!(1.0), &json!(1.0)));
    assert!(same(&json!(-0.0), &json!(0.0)));
    assert!(!same(&json!(1.0), &json!(1.0 + f64::EPSILON)));
    assert!(!same(
        &json!([0.9310058770591821]),
        &json!([0.9310058770591822])
    ));
    assert!(!same(&json!([1.0]), &json!([1.0, 2.0])));
    assert!(!same(&json!(true), &json!(false)));
}

/// ex-202 acceptance: `curveLength` matches upstream on the fixtures to
/// 1e-6. `curveLength` and `curveLengthAtParameter` only multiply, add and
/// take square roots, so the port is also held to the bit, which is stricter.
#[test]
fn curve_length_fixtures_match_upstream() {
    let cases: Vec<Value> = load()
        .into_iter()
        .filter(|c| c["fn"] == "curveLength" || c["fn"] == "curveLengthAtParameter")
        .collect();
    assert!(cases.len() > 100, "{} curve length cases", cases.len());
    for c in &cases {
        let args = c["args"].as_array().expect("args");
        let expected = f(&c["result"]);
        let actual = f(&call(c["fn"].as_str().expect("fn"), args));
        assert!(
            (actual - expected).abs() <= 1e-6,
            "{}: {actual} vs upstream {expected}",
            c["id"]
        );
        assert_eq!(actual, expected, "{}", c["id"]);
    }
}
