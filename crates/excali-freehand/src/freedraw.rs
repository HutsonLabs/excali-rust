//! Upstream's freedraw outlines (`packages/element/src/shape.ts:1193-1268`
//! at the pinned commit): perfect-freehand for variable width, the laser
//! pointer for constant width.

use std::sync::Arc;

use excali_math::js;

use crate::{get_stroke, InputPoint, LaserPointer, LaserPointerOptions, StrokeOptions};

/// `DEFAULT_STROKE_STREAMLINE` (`packages/common/src/constants.ts:622`): the
/// streamline of a freedraw element without `strokeOptions.streamline`.
pub const DEFAULT_STROKE_STREAMLINE: f64 = 0.5;

/// `VARIABLE_WIDTH_FREEDRAW.SIZE_FACTOR`: the perfect-freehand `size` is
/// `strokeWidth * 4.25`.
pub const VARIABLE_WIDTH_SIZE_FACTOR: f64 = 4.25;

/// `VARIABLE_WIDTH_FREEDRAW.THINNING`.
pub const VARIABLE_WIDTH_THINNING: f64 = 0.6;

/// `VARIABLE_WIDTH_FREEDRAW.SMOOTHING`.
pub const VARIABLE_WIDTH_SMOOTHING: f64 = 0.5;

/// easeOutSine, `Math.sin((t * Math.PI) / 2)` (`shape.ts:1241`).
pub fn ease_out_sine(t: f64) -> f64 {
    ((t * std::f64::consts::PI) / 2.0).sin()
}

/// The `getStroke` options of `getVariableWidthFreedrawOutline`: size
/// `strokeWidth * 4.25`, thinning 0.6, smoothing 0.5, streamline from
/// `strokeOptions.streamline` (default [`DEFAULT_STROKE_STREAMLINE`]),
/// easeOutSine, `simulatePressure` from the element, `last: true`, and the
/// library's cap defaults.
pub fn variable_width_options(
    stroke_width: f64,
    streamline: Option<f64>,
    simulate_pressure: bool,
) -> StrokeOptions {
    StrokeOptions {
        simulate_pressure,
        size: stroke_width * VARIABLE_WIDTH_SIZE_FACTOR,
        thinning: VARIABLE_WIDTH_THINNING,
        smoothing: VARIABLE_WIDTH_SMOOTHING,
        streamline: streamline.unwrap_or(DEFAULT_STROKE_STREAMLINE),
        easing: ease_out_sine,
        last: true,
        ..StrokeOptions::default()
    }
}

/// `getVariableWidthFreedrawOutline(element)`: the perfect-freehand outline
/// of a freedraw element's `points`. With `simulate_pressure` the points go
/// in as `[x, y]`; otherwise as `[x, y, pressures[i]]` (a missing pressure
/// is `undefined`, so the library's default applies), and an element with
/// no points is drawn as a dot `[0, 0, 0.5]`.
pub fn variable_width_outline(
    points: &[[f64; 2]],
    pressures: &[f64],
    stroke_width: f64,
    streamline: Option<f64>,
    simulate_pressure: bool,
) -> Vec<[f64; 2]> {
    let input: Vec<InputPoint> = if simulate_pressure {
        points.iter().map(|&[x, y]| InputPoint::new(x, y)).collect()
    } else if points.is_empty() {
        vec![InputPoint::with_pressure(0.0, 0.0, 0.5)]
    } else {
        points
            .iter()
            .enumerate()
            .map(|(i, &[x, y])| InputPoint {
                x,
                y,
                pressure: pressures.get(i).copied(),
            })
            .collect()
    };
    get_stroke(
        &input,
        &variable_width_options(stroke_width, streamline, simulate_pressure),
    )
}

/// `CONSTANT_WIDTH_FREEDRAW.SIZE_FACTOR`: the laser pointer's `size` is
/// `strokeWidth * 1.4`.
pub const CONSTANT_WIDTH_SIZE_FACTOR: f64 = 1.4;

/// `CONSTANT_WIDTH_FREEDRAW.COLLISION_SIMPLIFY_TOLERANCE`: the largest
/// deviation (px) when dropping vertices of the dense laser outline for
/// hit testing.
pub const CONSTANT_WIDTH_COLLISION_SIMPLIFY_TOLERANCE: f64 = 0.2;

/// `createLaserPointer(element)` (`shape.ts:1247-1253`) as options: size
/// `strokeWidth * 1.4`, streamline from `strokeOptions.streamline` (default
/// [`DEFAULT_STROKE_STREAMLINE`]), simplify 0, `sizeMapping`
/// `max(0.1, pressure)`, and the library's defaults otherwise.
pub fn constant_width_options(stroke_width: f64, streamline: Option<f64>) -> LaserPointerOptions {
    LaserPointerOptions {
        size: stroke_width * CONSTANT_WIDTH_SIZE_FACTOR,
        streamline: streamline.unwrap_or(DEFAULT_STROKE_STREAMLINE),
        simplify: 0.0,
        size_mapping: Arc::new(|details| js::max(0.1, details.pressure)),
        ..LaserPointerOptions::default()
    }
}

/// `getConstantWidthFreedrawOutline(element)` (`shape.ts:1255-1268`): the
/// laser-pointer outline of a freedraw element's `points`, each added as
/// `[x, y, 1]` so the stroke keeps a constant width, without the third
/// coordinate. An element with no points has no outline.
pub fn constant_width_outline(
    points: &[[f64; 2]],
    stroke_width: f64,
    streamline: Option<f64>,
) -> Vec<[f64; 2]> {
    let mut laser_pointer = LaserPointer::new(constant_width_options(stroke_width, streamline));
    for &[x, y] in points {
        laser_pointer
            .add_point([x, y, 1.0])
            .expect("simplify 0 never reaches the tail simplification");
    }
    laser_pointer
        .get_stroke_outline(None)
        .into_iter()
        .map(|[x, y, _]| [x, y])
        .collect()
}
