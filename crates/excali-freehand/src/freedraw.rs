//! Upstream's variable-width freedraw outline
//! (`packages/element/src/shape.ts:1193-1245` at the pinned commit).

use crate::{get_stroke, InputPoint, StrokeOptions};

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
