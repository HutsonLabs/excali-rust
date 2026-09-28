//! perfect-freehand 1.2.0 `getStrokeOutlinePoints` and `getStrokeRadius`.

use excali_math::js;

use crate::vec::{add, dist2, dpr, lrp, mul, neg, per, prj, rot_around, sub, uni, Vec2};
use crate::{StrokeOptions, StrokePoint, Taper};

/// `RATE_OF_PRESSURE_CHANGE`.
const RATE_OF_PRESSURE_CHANGE: f64 = 0.275;

/// `FIXED_PI`: a hair over pi, so the half-circle caps overlap.
const FIXED_PI: f64 = std::f64::consts::PI + 1e-4;

/// `getStrokeRadius(size, thinning, pressure, easing)`:
/// `size * easing(0.5 - thinning * (0.5 - pressure))`.
pub fn get_stroke_radius(size: f64, thinning: f64, pressure: f64, easing: fn(f64) -> f64) -> f64 {
    size * easing(0.5 - thinning * (0.5 - pressure))
}

/// JS truthiness of a taper distance that may be `undefined`.
fn truthy(v: Option<f64>) -> bool {
    matches!(v, Some(v) if v != 0.0 && !v.is_nan())
}

/// `a < b` where `b` may be `undefined` (always false then).
fn lt(a: f64, b: Option<f64>) -> bool {
    b.is_some_and(|b| a < b)
}

fn taper_distance(taper: Option<Taper>, size: f64, total_length: f64) -> Option<f64> {
    match taper? {
        Taper::Off => Some(0.0),
        Taper::Full => Some(js::max(size, total_length)),
        Taper::Distance(d) => Some(d),
    }
}

/// `for (let t = step; t <= 1 (or < 1); t += step)`, the step accumulated in
/// a double as the JS loop does.
fn steps(step: f64, first: f64, inclusive: bool) -> impl Iterator<Item = f64> {
    std::iter::successors(Some(first), move |t| Some(t + step)).take_while(move |&t| {
        if inclusive {
            t <= 1.0
        } else {
            t < 1.0
        }
    })
}

/// `getStrokeOutlinePoints(points, options)`: the outline polygon around
/// stroke points: the left side, the end cap, the right side reversed, then
/// the start cap. Uses `size`, `smoothing`, `thinning`, `simulate_pressure`,
/// `easing`, `start`, `end` and `last`; empty for no points or `size <= 0`.
pub fn get_stroke_outline_points(points: &[StrokePoint], options: &StrokeOptions) -> Vec<Vec2> {
    let StrokeOptions {
        size,
        smoothing,
        thinning,
        simulate_pressure,
        easing,
        start,
        end,
        last: is_complete,
        ..
    } = *options;
    let cap_start = start.cap;
    let taper_start_ease = start.easing;
    let cap_end = end.cap;
    let taper_end_ease = end.easing;

    if points.is_empty() || size <= 0.0 {
        return Vec::new();
    }

    let last = points.len() - 1;
    let total_length = points[last].running_length;
    let taper_start = taper_distance(start.taper, size, total_length);
    let taper_end = taper_distance(end.taper, size, total_length);

    // Points closer than this to the previous one on a side are dropped.
    let min_distance = (size * smoothing) * (size * smoothing);

    let mut left_pts: Vec<Vec2> = Vec::new();
    let mut right_pts: Vec<Vec2> = Vec::new();

    // The pressure the first point starts from: an average over the first
    // ten points, which gives better starts.
    let mut prev_pressure = points
        .iter()
        .take(10)
        .fold(points[0].pressure, |acc, curr| {
            let mut pressure = curr.pressure;
            if simulate_pressure {
                // Speed of change: how fast the pressure should change.
                let sp = js::min(1.0, curr.distance / size);
                // Rate of change: how much the pressure should change.
                let rp = js::min(1.0, 1.0 - sp);
                pressure = js::min(1.0, acc + (rp - acc) * (sp * RATE_OF_PRESSURE_CHANGE));
            }
            (acc + pressure) / 2.0
        });

    let mut radius = get_stroke_radius(size, thinning, points[last].pressure, easing);
    let mut first_radius: Option<f64> = None;
    let mut prev_vector = points[0].vector;
    let mut pl = points[0].point;
    let mut pr = pl;
    let mut tl = pl;
    let mut tr = pr;
    let mut is_prev_point_sharp_corner = false;

    for (i, curr) in points.iter().enumerate() {
        let mut pressure = curr.pressure;
        let StrokePoint {
            point,
            vector,
            distance,
            running_length,
            ..
        } = *curr;

        // Removes noise from the end of the line.
        if i < last && total_length - running_length < 3.0 {
            continue;
        }

        if thinning != 0.0 && !thinning.is_nan() {
            if simulate_pressure {
                let sp = js::min(1.0, distance / size);
                let rp = js::min(1.0, 1.0 - sp);
                pressure = js::min(
                    1.0,
                    prev_pressure + (rp - prev_pressure) * (sp * RATE_OF_PRESSURE_CHANGE),
                );
            }
            radius = get_stroke_radius(size, thinning, pressure, easing);
        } else {
            radius = size / 2.0;
        }

        if first_radius.is_none() {
            first_radius = Some(radius);
        }

        let ts = if lt(running_length, taper_start) {
            taper_start_ease(running_length / taper_start.unwrap_or(f64::NAN))
        } else {
            1.0
        };
        let te = if lt(total_length - running_length, taper_end) {
            taper_end_ease((total_length - running_length) / taper_end.unwrap_or(f64::NAN))
        } else {
            1.0
        };
        radius = js::max(0.01, radius * js::min(ts, te));

        // Sharp corners: add a half circle and move on.
        let next_vector = if i < last {
            points[i + 1].vector
        } else {
            curr.vector
        };
        let next_dpr = if i < last {
            dpr(vector, next_vector)
        } else {
            1.0
        };
        let prev_dpr = dpr(vector, prev_vector);
        let is_point_sharp_corner = prev_dpr < 0.0 && !is_prev_point_sharp_corner;
        let is_next_point_sharp_corner = next_dpr < 0.0;

        if is_point_sharp_corner || is_next_point_sharp_corner {
            let offset = mul(per(prev_vector), radius);
            for t in steps(1.0 / 13.0, 0.0, true) {
                tl = rot_around(sub(point, offset), point, FIXED_PI * t);
                left_pts.push(tl);
                tr = rot_around(add(point, offset), point, FIXED_PI * -t);
                right_pts.push(tr);
            }
            pl = tl;
            pr = tr;
            if is_next_point_sharp_corner {
                is_prev_point_sharp_corner = true;
            }
            continue;
        }

        is_prev_point_sharp_corner = false;

        // The last point: straight out to the sides.
        if i == last {
            let offset = mul(per(vector), radius);
            left_pts.push(sub(point, offset));
            right_pts.push(add(point, offset));
            continue;
        }

        // Every other point: offset along the perpendicular of the vector
        // blended towards the next one, dropped when too close to the last
        // kept point on that side.
        let offset = mul(per(lrp(next_vector, vector, next_dpr)), radius);

        tl = sub(point, offset);
        if i <= 1 || dist2(pl, tl) > min_distance {
            left_pts.push(tl);
            pl = tl;
        }

        tr = add(point, offset);
        if i <= 1 || dist2(pr, tr) > min_distance {
            right_pts.push(tr);
            pr = tr;
        }

        prev_pressure = pressure;
        prev_vector = vector;
    }

    let first_point = points[0].point;
    let last_point = if points.len() > 1 {
        points[last].point
    } else {
        add(points[0].point, [1.0, 1.0])
    };

    let mut start_cap: Vec<Vec2> = Vec::new();
    let mut end_cap: Vec<Vec2> = Vec::new();

    if points.len() == 1 {
        // A single point: a dot, unless tapered and not complete.
        if !(truthy(taper_start) || truthy(taper_end)) || is_complete {
            let r = match first_radius {
                Some(r) if r != 0.0 && !r.is_nan() => r,
                _ => radius,
            };
            let start = prj(first_point, uni(per(sub(first_point, last_point))), -r);
            return steps(1.0 / 13.0, 1.0 / 13.0, true)
                .map(|t| rot_around(start, first_point, FIXED_PI * 2.0 * t))
                .collect();
        }
    } else {
        // Start cap, unless there is a start taper.
        if !(truthy(taper_start) || (truthy(taper_end) && points.len() == 1)) {
            if cap_start {
                // The loop always pushes the last point to both sides, so
                // right_pts is not empty here.
                let from = right_pts[0];
                start_cap.extend(
                    steps(1.0 / 13.0, 1.0 / 13.0, true)
                        .map(|t| rot_around(from, first_point, FIXED_PI * t)),
                );
            } else {
                let corners_vector = sub(left_pts[0], right_pts[0]);
                let offset_a = mul(corners_vector, 0.5);
                let offset_b = mul(corners_vector, 0.51);
                start_cap.extend([
                    sub(first_point, offset_a),
                    sub(first_point, offset_b),
                    add(first_point, offset_b),
                    add(first_point, offset_a),
                ]);
            }
        }

        // End cap.
        let direction = per(neg(points[last].vector));
        if truthy(taper_end) || (truthy(taper_start) && points.len() == 1) {
            end_cap.push(last_point);
        } else if cap_end {
            let start = prj(last_point, direction, radius);
            end_cap.extend(
                steps(1.0 / 29.0, 1.0 / 29.0, false)
                    .map(|t| rot_around(start, last_point, FIXED_PI * 3.0 * t)),
            );
        } else {
            end_cap.extend([
                add(last_point, mul(direction, radius)),
                add(last_point, mul(direction, radius * 0.99)),
                sub(last_point, mul(direction, radius * 0.99)),
                sub(last_point, mul(direction, radius)),
            ]);
        }
    }

    left_pts
        .into_iter()
        .chain(end_cap)
        .chain(right_pts.into_iter().rev())
        .chain(start_cap)
        .collect()
}
