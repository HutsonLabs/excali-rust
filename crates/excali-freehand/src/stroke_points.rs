//! perfect-freehand 1.2.0 `getStrokePoints`.

use crate::vec::{add, dist, is_equal, lrp, sub, uni};
use crate::{InputPoint, StrokeOptions, StrokePoint};

/// `pressure >= 0 ? pressure : fallback`: `undefined >= 0` and `NaN >= 0`
/// are false in JS.
fn pressure_or(pressure: Option<f64>, fallback: f64) -> f64 {
    match pressure {
        Some(p) if p >= 0.0 => p,
        _ => fallback,
    }
}

/// `getStrokePoints(points, options)`: streamlines the input (each point
/// moves `0.15 + (1 - streamline) * 0.85` of the way from the previous
/// stroke point; with `last`, the final point is kept as is), skips points
/// until the stroke is `size` long, and records pressure, vector, distance
/// and running length. Uses `streamline`, `size` and `last`.
pub fn get_stroke_points(points: &[InputPoint], options: &StrokeOptions) -> Vec<StrokePoint> {
    let StrokeOptions {
        streamline,
        size,
        last: is_complete,
        ..
    } = *options;

    if points.is_empty() {
        return Vec::new();
    }

    let t = 0.15 + (1.0 - streamline) * 0.85;
    let mut pts = points.to_vec();

    // Two points: the first, then four lerps towards the second (the lerps
    // are [x, y] only, so they carry no pressure).
    if pts.len() == 2 {
        let end = pts[1].xy();
        pts.truncate(1);
        let start = pts[0].xy();
        for i in 1..5 {
            let [x, y] = lrp(start, end, f64::from(i) / 4.0);
            pts.push(InputPoint::new(x, y));
        }
    }

    // One point: add a second one at (+1, +1) with the same pressure.
    if pts.len() == 1 {
        let [x, y] = add(pts[0].xy(), [1.0, 1.0]);
        pts.push(InputPoint {
            x,
            y,
            pressure: pts[0].pressure,
        });
    }

    let mut stroke_points = vec![StrokePoint {
        point: pts[0].xy(),
        pressure: pressure_or(pts[0].pressure, 0.25),
        vector: [1.0, 1.0],
        distance: 0.0,
        running_length: 0.0,
    }];

    let mut has_reached_minimum_length = false;
    let mut running_length = 0.0;
    let mut prev = stroke_points[0];
    let max = pts.len() - 1;

    for (i, input) in pts.iter().enumerate().skip(1) {
        let point = if is_complete && i == max {
            input.xy()
        } else {
            lrp(prev.point, input.xy(), t)
        };

        if is_equal(prev.point, point) {
            continue;
        }

        let distance = dist(point, prev.point);
        running_length += distance;

        // Skip points until the line is `size` long (the last point always
        // counts). The skipped distances still add to the running length.
        if i < max && !has_reached_minimum_length {
            if running_length < size {
                continue;
            }
            has_reached_minimum_length = true;
        }

        prev = StrokePoint {
            point,
            pressure: pressure_or(input.pressure, 0.5),
            vector: uni(sub(prev.point, point)),
            distance,
            running_length,
        };
        stroke_points.push(prev);
    }

    // The first point takes the second's vector (or [0, 0] when alone).
    stroke_points[0].vector = stroke_points.get(1).map_or([0.0, 0.0], |p| p.vector);

    stroke_points
}
