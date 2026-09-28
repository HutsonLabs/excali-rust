//! `packages/math/src/curve.ts`: cubic Bézier curves.
//!
//! Evaluation ([`bezier_equation`], [`curve_tangent`]), intersection with a
//! line segment (Newton's method with the analytical Jacobian), the closest
//! point (sampling then bisection), Catmull-Rom approximation of a polyline,
//! offset curves, and arc length by Legendre-Gauss quadrature with n = 24
//! ([`LEGENDRE_GAUSS_N24_T_VALUES`], [`LEGENDRE_GAUSS_N24_C_VALUES`]).
//!
//! Arithmetic follows upstream's expressions term for term; `**` goes
//! through [`js::pow`].

use crate::constants::{LEGENDRE_GAUSS_N24_C_VALUES, LEGENDRE_GAUSS_N24_T_VALUES};
use crate::js;
use crate::point::{is_point, point_distance, point_from, point_from_vector};
use crate::types::{Curve, Global, GlobalPoint, LineSegment, Point, Space, Unknown, Vector};
use crate::vector::{vector, vector_normal, vector_normalize, vector_scale};

/// `curve(a, b, c, d)` (`curve.ts:15`): start, first handle, second handle,
/// end.
pub const fn curve<S: Space>(a: Point<S>, b: Point<S>, c: Point<S>, d: Point<S>) -> Curve<S> {
    Curve(a, b, c, d)
}

/// `solveWithAnalyticalJacobian(curve, lineSegment, t0, s0, tolerance,
/// iterLimit)` (`curve.ts:24`): Newton's method on `B(t) - L(s) = 0`.
/// `None` when the iteration limit is reached or the Jacobian is singular.
fn solve_with_analytical_jacobian<S: Space>(
    curve: Curve<S>,
    line_segment: LineSegment<S>,
    mut t0: f64,
    mut s0: f64,
    tolerance: f64,
    iter_limit: u32,
) -> Option<(f64, f64)> {
    let [c0, c1, c2, c3] = curve.points();
    let (l0, l1) = (line_segment.0, line_segment.1);
    let mut error = f64::INFINITY;
    let mut iter = 0;

    while error >= tolerance {
        if iter >= iter_limit {
            return None;
        }

        // Compute bezier point at parameter t0
        let bt = 1.0 - t0;
        let bt2 = bt * bt;
        let bt3 = bt2 * bt;
        let t0_2 = t0 * t0;
        let t0_3 = t0_2 * t0;

        let bezier_x = bt3 * c0.x + 3.0 * bt2 * t0 * c1.x + 3.0 * bt * t0_2 * c2.x + t0_3 * c3.x;
        let bezier_y = bt3 * c0.y + 3.0 * bt2 * t0 * c1.y + 3.0 * bt * t0_2 * c2.y + t0_3 * c3.y;

        // Compute line point at parameter s0
        let line_x = l0.x + s0 * (l1.x - l0.x);
        let line_y = l0.y + s0 * (l1.y - l0.y);

        // Function values
        let fx = bezier_x - line_x;
        let fy = bezier_y - line_y;

        error = fx.abs() + fy.abs();

        if error < tolerance {
            break;
        }

        // Analytical derivatives
        let dfx_dt =
            -3.0 * bt2 * c0.x + 3.0 * bt2 * c1.x - 6.0 * bt * t0 * c1.x - 3.0 * t0_2 * c2.x
                + 6.0 * bt * t0 * c2.x
                + 3.0 * t0_2 * c3.x;

        let dfy_dt =
            -3.0 * bt2 * c0.y + 3.0 * bt2 * c1.y - 6.0 * bt * t0 * c1.y - 3.0 * t0_2 * c2.y
                + 6.0 * bt * t0 * c2.y
                + 3.0 * t0_2 * c3.y;

        // Line derivatives
        let dfx_ds = -(l1.x - l0.x);
        let dfy_ds = -(l1.y - l0.y);

        // Jacobian determinant
        let det = dfx_dt * dfy_ds - dfx_ds * dfy_dt;

        if det.abs() < 1e-12 {
            return None;
        }

        // Newton step
        let inv_det = 1.0 / det;
        let dt = inv_det * (dfy_ds * -fx - dfx_ds * -fy);
        let ds = inv_det * (-dfy_dt * -fx + dfx_dt * -fy);

        t0 += dt;
        s0 += ds;
        iter += 1;
    }

    Some((t0, s0))
}

/// `bezierEquation(c, t)` (`curve.ts:115`): the point at parameter `t`.
pub fn bezier_equation<S: Space>(c: Curve<S>, t: f64) -> Point<S> {
    let [c0, c1, c2, c3] = c.points();
    point_from(
        js::pow(1.0 - t, 3.0) * c0.x
            + 3.0 * js::pow(1.0 - t, 2.0) * t * c1.x
            + 3.0 * (1.0 - t) * js::pow(t, 2.0) * c2.x
            + js::pow(t, 3.0) * c3.x,
        js::pow(1.0 - t, 3.0) * c0.y
            + 3.0 * js::pow(1.0 - t, 2.0) * t * c1.y
            + 3.0 * (1.0 - t) * js::pow(t, 2.0) * c2.y
            + js::pow(t, 3.0) * c3.y,
    )
}

/// The `[t0, s0]` starting guesses `curveIntersectLineSegment` tries in
/// order (`curve.ts:130`).
const INITIAL_GUESSES: [(f64, f64); 3] = [(0.5, 0.0), (0.2, 0.0), (0.8, 0.0)];

/// The options object of `curveIntersectLineSegment(c, l, opts)`. `None`
/// (upstream's `undefined`) takes the default: tolerance `1e-2`, at most 4
/// Newton steps per starting guess.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurveIntersectOptions {
    pub tolerance: Option<f64>,
    pub iter_limit: Option<u32>,
}

/// `calculate([t0, s0], l, c, tolerance, iterLimit)` (`curve.ts:136`): the
/// intersection found from one starting guess, if its parameters land on
/// both the curve and the segment.
fn calculate<S: Space>(
    (t0, s0): (f64, f64),
    l: LineSegment<S>,
    c: Curve<S>,
    opts: CurveIntersectOptions,
) -> Option<Point<S>> {
    let tolerance = opts.tolerance.unwrap_or(1e-2);
    let iter_limit = opts.iter_limit.unwrap_or(4);
    let (t, s) = solve_with_analytical_jacobian(c, l, t0, s0, tolerance, iter_limit)?;

    // Not `!(0.0..=1.0).contains(..)`: a NaN parameter passes these
    // comparisons upstream and must pass them here.
    #[allow(clippy::manual_range_contains)]
    if t < 0.0 || t > 1.0 || s < 0.0 || s > 1.0 {
        return None;
    }

    Some(bezier_equation(c, t))
}

/// `curveIntersectLineSegment(c, l)` with the default options.
pub fn curve_intersect_line_segment<S: Space>(c: Curve<S>, l: LineSegment<S>) -> Vec<Point<S>> {
    curve_intersect_line_segment_with(c, l, CurveIntersectOptions::default())
}

/// `curveIntersectLineSegment(c, l, opts)` (`curve.ts:168`): at most one
/// intersection of the curve and the segment, from the first of the three
/// starting guesses that converges onto both.
pub fn curve_intersect_line_segment_with<S: Space>(
    c: Curve<S>,
    l: LineSegment<S>,
    opts: CurveIntersectOptions,
) -> Vec<Point<S>> {
    INITIAL_GUESSES
        .iter()
        .find_map(|&guess| calculate(guess, l, c, opts))
        .into_iter()
        .collect()
}

/// `curveClosestParameter(c, p)` with the default tolerance `1e-3`.
pub fn curve_closest_parameter<S: Space>(c: Curve<S>, p: Point<S>) -> f64 {
    curve_closest_parameter_with(c, p, 1e-3)
}

/// `curveClosestParameter(c, p, tolerance)` (`curve.ts:223`): the parameter
/// of the curve point closest to `p`. The curve is sampled at 31 evenly
/// spaced parameters, then the window around the closest sample is bisected
/// until it is no wider than `tolerance`.
pub fn curve_closest_parameter_with<S: Space>(c: Curve<S>, p: Point<S>, tolerance: f64) -> f64 {
    let f = |t: f64| point_distance(p, bezier_equation(c, t));
    let local_minimum = |min: f64, max: f64, e: f64| {
        let mut m = min;
        let mut n = max;
        let mut k = None;

        while n - m > e {
            let mid = (n + m) / 2.0;
            k = Some(mid);
            if f(mid - e) < f(mid + e) {
                n = mid;
            } else {
                m = mid;
            }
        }

        k
    };

    let max_steps = 30.0;
    let mut closest_step = 0.0;
    let mut min = f64::INFINITY;
    let mut step = 0.0;
    while step <= max_steps {
        let d = f(step / max_steps);
        if d < min {
            min = d;
            closest_step = step;
        }
        step += 1.0;
    }

    let t0 = js::max((closest_step - 1.0) / max_steps, 0.0);
    let t1 = js::min((closest_step + 1.0) / max_steps, 1.0);
    let param = local_minimum(t0, t1, tolerance);

    param.unwrap_or(closest_step / max_steps)
}

/// `curveClosestPoint(c, p)` with the default tolerance `1e-3`.
pub fn curve_closest_point<S: Space>(c: Curve<S>, p: Point<S>) -> Point<S> {
    curve_closest_point_with(c, p, 1e-3)
}

/// `curveClosestPoint(c, p, tolerance)` (`curve.ts:277`): the curve point
/// at [`curve_closest_parameter_with`].
pub fn curve_closest_point_with<S: Space>(c: Curve<S>, p: Point<S>, tolerance: f64) -> Point<S> {
    bezier_equation(c, curve_closest_parameter_with(c, p, tolerance))
}

/// `curvePointDistance(c, p)` with the default tolerance `1e-3`.
pub fn curve_point_distance<S: Space>(c: Curve<S>, p: Point<S>) -> f64 {
    curve_point_distance_with(c, p, 1e-3)
}

/// `curvePointDistance(c, p, tolerance)` (`curve.ts:292`): the distance from
/// `p` to [`curve_closest_point_with`].
pub fn curve_point_distance_with<S: Space>(c: Curve<S>, p: Point<S>, tolerance: f64) -> f64 {
    point_distance(p, curve_closest_point_with(c, p, tolerance))
}

/// `isCurve(value)` (`curve.ts:303`): an array of four values that each
/// pass [`is_point`].
pub fn is_curve(v: &Unknown) -> bool {
    match v.as_array() {
        Some(items) => items.len() == 4 && items.iter().all(is_point),
        None => false,
    }
}

/// `curveTangent(c, t)` (`curve.ts:316`): the derivative `B'(t)`, not
/// normalised.
pub fn curve_tangent<S: Space>(c: Curve<S>, t: f64) -> Vector {
    let [p0, p1, p2, p3] = c.points();
    vector(
        -3.0 * (1.0 - t) * (1.0 - t) * p0.x + 3.0 * (1.0 - t) * (1.0 - t) * p1.x
            - 6.0 * t * (1.0 - t) * p1.x
            - 3.0 * t * t * p2.x
            + 6.0 * t * (1.0 - t) * p2.x
            + 3.0 * t * t * p3.x,
        -3.0 * (1.0 - t) * (1.0 - t) * p0.y + 3.0 * (1.0 - t) * (1.0 - t) * p1.y
            - 6.0 * t * (1.0 - t) * p1.y
            - 3.0 * t * t * p2.y
            + 6.0 * t * (1.0 - t) * p2.y
            + 3.0 * t * t * p3.y,
    )
}

/// The previous, current, next and after-next point of a Catmull-Rom
/// segment starting at `i`, clamped to the ends of `points`.
fn catmull_rom_window<S: Space>(points: &[Point<S>], i: usize) -> [Point<S>; 4] {
    let last = points.len() - 1;
    [
        points[i.saturating_sub(1)],
        points[i],
        points[(i + 1).min(last)],
        points[(i + 2).min(last)],
    ]
}

/// `curveCatmullRomQuadraticApproxPoints(points)` with the default tension
/// `0.5`.
pub fn curve_catmull_rom_quadratic_approx_points(
    points: &[GlobalPoint],
) -> Option<Vec<[GlobalPoint; 2]>> {
    curve_catmull_rom_quadratic_approx_points_with(points, 0.5)
}

/// `curveCatmullRomQuadraticApproxPoints(points, tension)`
/// (`curve.ts:336`): per polyline segment, the quadratic control point and
/// the segment's end. `None` (upstream's `undefined`) for fewer than two
/// points.
pub fn curve_catmull_rom_quadratic_approx_points_with(
    points: &[GlobalPoint],
    tension: f64,
) -> Option<Vec<[GlobalPoint; 2]>> {
    if points.len() < 2 {
        return None;
    }

    let point_sets = (0..points.len() - 1)
        .map(|i| {
            let [p0, p1, p2, _] = catmull_rom_window(points, i);
            let cp_x = p1.x + ((p2.x - p0.x) * tension) / 2.0;
            let cp_y = p1.y + ((p2.y - p0.y) * tension) / 2.0;

            [point_from(cp_x, cp_y), point_from(p2.x, p2.y)]
        })
        .collect();

    Some(point_sets)
}

/// `curveCatmullRomCubicApproxPoints(points)` with the default tension
/// `0.5`.
pub fn curve_catmull_rom_cubic_approx_points<S: Space>(
    points: &[Point<S>],
) -> Option<Vec<Curve<S>>> {
    curve_catmull_rom_cubic_approx_points_with(points, 0.5)
}

/// `curveCatmullRomCubicApproxPoints(points, tension)` (`curve.ts:361`):
/// one cubic curve per polyline segment, the handles a third of the
/// Catmull-Rom tangents from the ends. `None` (upstream's `undefined`) for
/// fewer than two points.
pub fn curve_catmull_rom_cubic_approx_points_with<S: Space>(
    points: &[Point<S>],
    tension: f64,
) -> Option<Vec<Curve<S>>> {
    if points.len() < 2 {
        return None;
    }

    let point_sets = (0..points.len() - 1)
        .map(|i| {
            let [p0, p1, p2, p3] = catmull_rom_window(points, i);
            let tangent1 = [(p2.x - p0.x) * tension, (p2.y - p0.y) * tension];
            let tangent2 = [(p3.x - p1.x) * tension, (p3.y - p1.y) * tension];
            let cp1x = p1.x + tangent1[0] / 3.0;
            let cp1y = p1.y + tangent1[1] / 3.0;
            let cp2x = p2.x - tangent2[0] / 3.0;
            let cp2y = p2.y - tangent2[1] / 3.0;

            curve(
                point_from(p1.x, p1.y),
                point_from(cp1x, cp1y),
                point_from(cp2x, cp2y),
                point_from(p2.x, p2.y),
            )
        })
        .collect();

    Some(point_sets)
}

/// `curveOffsetPoints(curve, offset)` with the default 50 steps.
pub fn curve_offset_points(c: Curve<Global>, offset: f64) -> Vec<GlobalPoint> {
    curve_offset_points_with(c, offset, 50)
}

/// `curveOffsetPoints(curve, offset, steps)` (`curve.ts:394`): `steps + 1`
/// points at `t = i / steps`, each moved `offset` along the curve's
/// right-hand normal there. Zero steps give one NaN point, as `0 / 0` does
/// upstream.
pub fn curve_offset_points_with(c: Curve<Global>, offset: f64, steps: u32) -> Vec<GlobalPoint> {
    let steps = f64::from(steps);
    let mut offset_points = Vec::new();

    let mut i = 0.0;
    while i <= steps {
        let t = i / steps;
        let point = bezier_equation(c, t);
        let tangent = vector_normalize(curve_tangent(c, t));
        let normal = vector_normal(tangent);

        offset_points.push(point_from_vector(vector_scale(normal, offset), point));
        i += 1.0;
    }

    offset_points
}

/// `offsetPointsForQuadraticBezier(p0, p1, p2, offsetDist)` with the
/// default 50 steps.
pub fn offset_points_for_quadratic_bezier(
    p0: GlobalPoint,
    p1: GlobalPoint,
    p2: GlobalPoint,
    offset_dist: f64,
) -> Vec<GlobalPoint> {
    offset_points_for_quadratic_bezier_with(p0, p1, p2, offset_dist, 50)
}

/// `offsetPointsForQuadraticBezier(p0, p1, p2, offsetDist, steps)`
/// (`curve.ts:414`): as [`curve_offset_points_with`] for the quadratic curve
/// with control points `p0`, `p1`, `p2`.
pub fn offset_points_for_quadratic_bezier_with(
    p0: GlobalPoint,
    p1: GlobalPoint,
    p2: GlobalPoint,
    offset_dist: f64,
    steps: u32,
) -> Vec<GlobalPoint> {
    let steps = f64::from(steps);
    let mut offset_points = Vec::new();

    let mut i = 0.0;
    while i <= steps {
        let t = i / steps;
        let t1 = 1.0 - t;
        let point: GlobalPoint = point_from(
            t1 * t1 * p0.x + 2.0 * t1 * t * p1.x + t * t * p2.x,
            t1 * t1 * p0.y + 2.0 * t1 * t * p1.y + t * t * p2.y,
        );
        let tangent_x = 2.0 * (1.0 - t) * (p1.x - p0.x) + 2.0 * t * (p2.x - p1.x);
        let tangent_y = 2.0 * (1.0 - t) * (p1.y - p0.y) + 2.0 * t * (p2.y - p1.y);
        let tangent = vector_normalize(vector(tangent_x, tangent_y));
        let normal = vector_normal(tangent);

        offset_points.push(point_from_vector(vector_scale(normal, offset_dist), point));
        i += 1.0;
    }

    offset_points
}

/// The Legendre-Gauss sum of `|B'|` over the 24 nodes mapped by
/// `t = z1 * x + z2`.
fn legendre_gauss_sum<S: Space>(c: Curve<S>, z1: f64, z2: f64) -> f64 {
    let mut sum = 0.0;

    for i in 0..24 {
        let t = z1 * LEGENDRE_GAUSS_N24_T_VALUES[i] + z2;
        let derivative_vector = curve_tangent(c, t);
        let magnitude = (derivative_vector.x * derivative_vector.x
            + derivative_vector.y * derivative_vector.y)
            .sqrt();
        sum += LEGENDRE_GAUSS_N24_C_VALUES[i] * magnitude;
    }

    sum
}

/// `curveLength(c)` (`curve.ts:450`): the arc length by Legendre-Gauss
/// quadrature with n = 24 (<https://pomax.github.io/bezierinfo/#arclength>).
pub fn curve_length<S: Space>(c: Curve<S>) -> f64 {
    let z2 = 0.5;
    z2 * legendre_gauss_sum(c, z2, z2)
}

/// `curveLengthAtParameter(c, t)` (`curve.ts:477`): the arc length from
/// `t = 0` to `t`, by the same quadrature over `[0, t]`; 0 for `t <= 0` and
/// [`curve_length`] for `t >= 1`.
pub fn curve_length_at_parameter<S: Space>(c: Curve<S>, t: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return curve_length(c);
    }

    // Scale and shift the integration interval from [0,t] to [-1,1]
    // which is what the Legendre-Gauss quadrature expects
    let z1 = t / 2.0;
    let z2 = t / 2.0;

    z1 * legendre_gauss_sum(c, z1, z2)
}

/// `curvePointAtLength(c, percent)`: the total length is [`curve_length`].
pub fn curve_point_at_length<S: Space>(c: Curve<S>, percent: f64) -> Point<S> {
    curve_point_at_length_with(c, percent, curve_length(c))
}

/// `curvePointAtLength(c, percent, totalLength)` (`curve.ts:516`): the point
/// `percent` of the way along the curve by arc length, found by at most 20
/// bisection steps on the parameter, stopping within `totalLength * 1e-4`.
/// `total_length` is the curve's length when the caller already knows it.
pub fn curve_point_at_length_with<S: Space>(
    c: Curve<S>,
    percent: f64,
    total_length: f64,
) -> Point<S> {
    if percent <= 0.0 {
        return bezier_equation(c, 0.0);
    }

    if percent >= 1.0 {
        return bezier_equation(c, 1.0);
    }

    let target_length = total_length * percent;

    // Binary search to find parameter t where length at t equals target length
    let mut t_min = 0.0;
    let mut t_max = 1.0;
    let mut t = percent;

    // Tolerance for length comparison and iteration limit to avoid infinite loops
    let tolerance = total_length * 0.0001;
    let max_iterations = 20;

    for _ in 0..max_iterations {
        let current_length = curve_length_at_parameter(c, t);
        let error = (current_length - target_length).abs();

        if error < tolerance {
            break;
        }

        if current_length < target_length {
            t_min = t;
        } else {
            t_max = t;
        }

        t = (t_min + t_max) / 2.0;
    }

    bezier_equation(c, t)
}
