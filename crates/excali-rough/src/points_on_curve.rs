//! `points-on-curve` 0.2.0 (the copy rough.js 4.6.4 and points-on-path 0.2.1
//! depend on): Bezier flattening, Ramer–Douglas–Peucker simplification and
//! the Catmull-Rom to Bezier conversion.

use excali_math::js;

use crate::path_data::PathError;
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

/// Subdivision depth past which [`points_on_bezier_with_splitting`] reports
/// [`PathError::CallStackExceeded`].
///
/// A curve that flattens does so on every branch of the (balanced) split at
/// about the same depth, so one that needs more than a few dozen levels
/// already takes `2^depth` steps; any curve still splitting at this depth is
/// one whose flatness never falls below the tolerance (a non-finite point,
/// a tolerance that is not positive), on which the package recurses until
/// the JavaScript stack overflows (about 3,300 frames on Node 26).
pub const MAX_SPLIT_DEPTH: usize = 1024;

/// `getPointsOnBezierCurveWithSplitting(points, offset, tolerance, newPoints)`.
///
/// The package recurses, first half before second; this walks the same
/// order with an explicit stack, so the native stack cannot overflow, and
/// stops with [`PathError::CallStackExceeded`] where the package would throw
/// `RangeError` (see [`MAX_SPLIT_DEPTH`]).
fn points_on_bezier_with_splitting(
    points: &[Point],
    offset: usize,
    tolerance: f64,
    out: &mut Vec<Point>,
) -> Result<(), PathError> {
    let first = [
        points[offset],
        points[offset + 1],
        points[offset + 2],
        points[offset + 3],
    ];
    let mut stack: Vec<([Point; 4], usize)> = vec![(first, 0)];
    while let Some((curve, depth)) = stack.pop() {
        let [p1, p2, p3, p4] = curve;
        if flatness(&curve, 0) < tolerance {
            match out.last() {
                Some(&last) => {
                    if distance(last, p1) > 1.0 {
                        out.push(p1);
                    }
                }
                None => out.push(p1),
            }
            out.push(p4);
        } else {
            if depth >= MAX_SPLIT_DEPTH {
                return Err(PathError::CallStackExceeded);
            }
            let t = 0.5;
            let q1 = lerp(p1, p2, t);
            let q2 = lerp(p2, p3, t);
            let q3 = lerp(p3, p4, t);
            let r1 = lerp(q1, q2, t);
            let r2 = lerp(q2, q3, t);
            let red = lerp(r1, r2, t);
            // pushed second half first so the first half is drawn first
            stack.push(([red, r2, q3, p4], depth + 1));
            stack.push(([p1, q1, r1, red], depth + 1));
        }
    }
    Ok(())
}

/// `simplify(points, distance)`.
pub fn simplify(points: &[Point], distance: f64) -> Result<Vec<Point>, PathError> {
    let mut out = Vec::new();
    simplify_points(points, 0, points.len(), distance, &mut out)?;
    Ok(out)
}

/// `simplifyPoints(points, start, end, epsilon, newPoints)`: Ramer–Douglas–
/// Peucker over `points[start..end]`, appending to `out`. An empty range
/// appends nothing (the package reads `points[-1]` there).
///
/// A negative `epsilon` is [`PathError::CallStackExceeded`]: the package
/// then splits every range, even an empty one, until the stack overflows.
///
/// Divergence: the package recurses once per split, so on a polyline whose
/// splits peel off one point at a time it throws `RangeError` once the
/// recursion outgrows the JavaScript stack (on Node 26, a 5,000-point
/// zigzag). That limit belongs to the engine, not the package; this walks
/// the same ranges in the same order with an explicit stack and returns the
/// simplified polyline at any depth.
pub fn simplify_points(
    points: &[Point],
    start: usize,
    end: usize,
    epsilon: f64,
    out: &mut Vec<Point>,
) -> Result<(), PathError> {
    if epsilon < 0.0 {
        return Err(PathError::CallStackExceeded);
    }
    let mut stack = vec![(start, end)];
    while let Some((start, end)) = stack.pop() {
        if end <= start {
            continue;
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
            // with epsilon >= 0 a split implies start < max_ndx < end - 1,
            // so both ranges are shorter; the first is walked first
            stack.push((max_ndx, end));
            stack.push((start, max_ndx + 1));
        } else {
            if out.is_empty() {
                out.push(s);
            }
            out.push(e);
        }
    }
    Ok(())
}

/// `pointsOnBezierCurves(points, tolerance = 0.15, distance)`: a chain of
/// cubic Beziers (`1 + 3n` points) flattened to a polyline, then simplified
/// when `distance` is positive. A trailing partial segment is ignored (the
/// package throws on it). [`PathError::CallStackExceeded`] where the package
/// throws `RangeError` (see [`MAX_SPLIT_DEPTH`]).
pub fn points_on_bezier_curves(
    points: &[Point],
    tolerance: f64,
    distance: Option<f64>,
) -> Result<Vec<Point>, PathError> {
    let mut new_points = Vec::new();
    let num_segments = points.len().saturating_sub(1) / 3;
    for i in 0..num_segments {
        points_on_bezier_with_splitting(points, i * 3, tolerance, &mut new_points)?;
    }
    if let Some(d) = distance.filter(|d| *d > 0.0) {
        let mut out = Vec::new();
        simplify_points(&new_points, 0, new_points.len(), d, &mut out)?;
        return Ok(out);
    }
    Ok(new_points)
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
