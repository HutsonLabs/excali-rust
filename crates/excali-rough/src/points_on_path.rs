//! `points-on-path` 0.2.1: an SVG path as polylines, one per subpath.
//! rough.js uses it for path fills and for `simplification`.

use crate::path_data::{absolutize, normalize, parse_path, PathError};
use crate::points_on_curve::{points_on_bezier_curves, simplify};
use crate::Point;

/// `pointsOnPath(path, tolerance, distance)`: curves flattened to within
/// `tolerance`, then each subpath simplified to `distance` unless it is 0
/// (`if (!distance) return sets`). [`PathError::CallStackExceeded`] where
/// the package throws `RangeError`: a curve that never flattens (a
/// non-finite coordinate, a tolerance that is not positive) or a negative
/// `distance`.
pub fn points_on_path(
    path: &str,
    tolerance: f64,
    distance: f64,
) -> Result<Vec<Vec<Point>>, PathError> {
    let normalized = normalize(&absolutize(&parse_path(path)?));
    let mut sets: Vec<Vec<Point>> = Vec::new();
    let mut current_points: Vec<Point> = Vec::new();
    let mut start: Point = [0.0, 0.0];
    let mut pending_curve: Vec<Point> = Vec::new();

    fn append_pending_curve(
        pending: &mut Vec<Point>,
        current: &mut Vec<Point>,
        tolerance: f64,
    ) -> Result<(), PathError> {
        if pending.len() >= 4 {
            current.extend(points_on_bezier_curves(pending, tolerance, None)?);
        }
        pending.clear();
        Ok(())
    }
    fn append_pending_points(
        pending: &mut Vec<Point>,
        current: &mut Vec<Point>,
        sets: &mut Vec<Vec<Point>>,
        tolerance: f64,
    ) -> Result<(), PathError> {
        append_pending_curve(pending, current, tolerance)?;
        if !current.is_empty() {
            sets.push(std::mem::take(current));
        }
        Ok(())
    }

    for s in &normalized {
        let data = &s.data;
        match s.key {
            'M' => {
                append_pending_points(
                    &mut pending_curve,
                    &mut current_points,
                    &mut sets,
                    tolerance,
                )?;
                start = [data[0], data[1]];
                current_points.push(start);
            }
            'L' => {
                append_pending_curve(&mut pending_curve, &mut current_points, tolerance)?;
                current_points.push([data[0], data[1]]);
            }
            'C' => {
                if pending_curve.is_empty() {
                    let last = current_points.last().copied().unwrap_or(start);
                    pending_curve.push(last);
                }
                pending_curve.push([data[0], data[1]]);
                pending_curve.push([data[2], data[3]]);
                pending_curve.push([data[4], data[5]]);
            }
            'Z' => {
                append_pending_curve(&mut pending_curve, &mut current_points, tolerance)?;
                current_points.push(start);
            }
            _ => {}
        }
    }
    append_pending_points(
        &mut pending_curve,
        &mut current_points,
        &mut sets,
        tolerance,
    )?;

    // `!distance`: 0 and NaN leave the sets as they are
    if distance == 0.0 || distance.is_nan() {
        return Ok(sets);
    }
    let mut out = Vec::with_capacity(sets.len());
    for set in &sets {
        let simplified = simplify(set, distance)?;
        if !simplified.is_empty() {
            out.push(simplified);
        }
    }
    Ok(out)
}
