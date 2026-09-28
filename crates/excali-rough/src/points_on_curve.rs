//! `points-on-curve` 0.2.0 (the copy rough.js 4.6.4 and points-on-path 0.2.1
//! depend on): Bezier flattening, Ramer–Douglas–Peucker simplification and
//! the Catmull-Rom to Bezier conversion.

use excali_math::js;

use crate::Point;

fn distance_sq(p1: Point, p2: Point) -> f64 {
    (p1[0] - p2[0]).powi(2) + (p1[1] - p2[1]).powi(2)
}

fn distance(p1: Point, p2: Point) -> f64 {
    distance_sq(p1, p2).sqrt()
}

/// Squared distance from `p` to the segment `vw`.
fn distance_to_segment_sq(p: Point, v: Point, w: Point) -> f64 {
    let l2 = distance_sq(v, w);
    if l2 == 0.0 {
        return distance_sq(p, v);
    }
    let mut t = ((p[0] - v[0]) * (w[0] - v[0]) + (p[1] - v[1]) * (w[1] - v[1])) / l2;
    t = js::max(0.0, js::min(1.0, t));
    distance_sq(p, lerp(v, w, t))
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// `flatness(points, offset)`.
fn flatness(points: &[Point], offset: usize) -> f64 {
    let p1 = points[offset];
    let p2 = points[offset + 1];
    let p3 = points[offset + 2];
    let p4 = points[offset + 3];
    let mut ux = 3.0 * p2[0] - 2.0 * p1[0] - p4[0];
    ux *= ux;
    let mut uy = 3.0 * p2[1] - 2.0 * p1[1] - p4[1];
    uy *= uy;
    let mut vx = 3.0 * p3[0] - 2.0 * p4[0] - p1[0];
    vx *= vx;
    let mut vy = 3.0 * p3[1] - 2.0 * p4[1] - p1[1];
    vy *= vy;
    if ux < vx {
        ux = vx;
    }
    if uy < vy {
        uy = vy;
    }
    ux + uy
}

/// `getPointsOnBezierCurveWithSplitting(points, offset, tolerance, newPoints)`.
fn points_on_bezier_with_splitting(
    points: &[Point],
    offset: usize,
    tolerance: f64,
    out: &mut Vec<Point>,
) {
    if flatness(points, offset) < tolerance {
        let p0 = points[offset];
        match out.last() {
            Some(&last) => {
                if distance(last, p0) > 1.0 {
                    out.push(p0);
                }
            }
            None => out.push(p0),
        }
        out.push(points[offset + 3]);
    } else {
        let t = 0.5;
        let p1 = points[offset];
        let p2 = points[offset + 1];
        let p3 = points[offset + 2];
        let p4 = points[offset + 3];
        let q1 = lerp(p1, p2, t);
        let q2 = lerp(p2, p3, t);
        let q3 = lerp(p3, p4, t);
        let r1 = lerp(q1, q2, t);
        let r2 = lerp(q2, q3, t);
        let red = lerp(r1, r2, t);
        points_on_bezier_with_splitting(&[p1, q1, r1, red], 0, tolerance, out);
        points_on_bezier_with_splitting(&[red, r2, q3, p4], 0, tolerance, out);
    }
}

/// `simplify(points, distance)`.
pub fn simplify(points: &[Point], distance: f64) -> Vec<Point> {
    let mut out = Vec::new();
    simplify_points(points, 0, points.len(), distance, &mut out);
    out
}

/// `simplifyPoints(points, start, end, epsilon, newPoints)`: Ramer–Douglas–
/// Peucker over `points[start..end]`, appending to `out`. An empty range
/// appends nothing (the package reads `points[-1]` there). `epsilon` must
/// not be negative: the package then recurses until the stack overflows.
pub fn simplify_points(
    points: &[Point],
    start: usize,
    end: usize,
    epsilon: f64,
    out: &mut Vec<Point>,
) {
    if end <= start {
        return;
    }
    let s = points[start];
    let e = points[end - 1];
    let mut max_dist_sq = 0.0;
    // the package starts from 1, not start + 1
    let mut max_ndx = 1;
    for (i, &p) in points.iter().enumerate().take(end - 1).skip(start + 1) {
        let dist_sq = distance_to_segment_sq(p, s, e);
        if dist_sq > max_dist_sq {
            max_dist_sq = dist_sq;
            max_ndx = i;
        }
    }
    if f64::sqrt(max_dist_sq) > epsilon {
        simplify_points(points, start, max_ndx + 1, epsilon, out);
        simplify_points(points, max_ndx, end, epsilon, out);
    } else {
        if out.is_empty() {
            out.push(s);
        }
        out.push(e);
    }
}

/// `pointsOnBezierCurves(points, tolerance = 0.15, distance)`: a chain of
/// cubic Beziers (`1 + 3n` points) flattened to a polyline, then simplified
/// when `distance` is positive. A trailing partial segment is ignored (the
/// package throws on it).
pub fn points_on_bezier_curves(
    points: &[Point],
    tolerance: f64,
    distance: Option<f64>,
) -> Vec<Point> {
    let mut new_points = Vec::new();
    let num_segments = points.len().saturating_sub(1) / 3;
    for i in 0..num_segments {
        points_on_bezier_with_splitting(points, i * 3, tolerance, &mut new_points);
    }
    if let Some(d) = distance.filter(|d| *d > 0.0) {
        let mut out = Vec::new();
        simplify_points(&new_points, 0, new_points.len(), d, &mut out);
        return out;
    }
    new_points
}

/// `curveToBezier(pointsIn, curveTightness = 0)`: the Catmull-Rom curve
/// through the points as a Bezier chain. Fewer than three points is the
/// package's `A curve must have at least three points.` error.
pub fn curve_to_bezier(points_in: &[Point], curve_tightness: f64) -> Result<Vec<Point>, String> {
    let len = points_in.len();
    if len < 3 {
        return Err("A curve must have at least three points.".to_owned());
    }
    let mut out = Vec::new();
    if len == 3 {
        out.extend([points_in[0], points_in[1], points_in[2], points_in[2]]);
    } else {
        let mut points = vec![points_in[0], points_in[0]];
        points.extend_from_slice(&points_in[1..]);
        points.push(points_in[len - 1]);
        let s = 1.0 - curve_tightness;
        out.push(points[0]);
        let mut i = 1;
        while i + 2 < points.len() {
            let v = points[i];
            let b1 = [
                v[0] + (s * points[i + 1][0] - s * points[i - 1][0]) / 6.0,
                v[1] + (s * points[i + 1][1] - s * points[i - 1][1]) / 6.0,
            ];
            let b2 = [
                points[i + 1][0] + (s * points[i][0] - s * points[i + 2][0]) / 6.0,
                points[i + 1][1] + (s * points[i][1] - s * points[i + 2][1]) / 6.0,
            ];
            let b3 = [points[i + 1][0], points[i + 1][1]];
            out.extend([b1, b2, b3]);
            i += 1;
        }
    }
    Ok(out)
}
