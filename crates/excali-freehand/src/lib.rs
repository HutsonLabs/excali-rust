//! Freedraw stroke outlines and the laser pointer trail, a port of perfect-freehand.
//!
//! Upstream counterpart: `perfect-freehand` 1.2.0 and `packages/laser-pointer`.
//!
//! Targets: native, wasm32. Internal dependencies allowed by the architecture
//! overview (`site/content/architecture/overview.md`, ADR-008): `excali-math`.
//!
//! # perfect-freehand
//!
//! [`get_stroke_points`], [`get_stroke_outline_points`], [`get_stroke`] and
//! [`get_stroke_radius`] are ports of the library's four exports, statement
//! for statement from the published `dist/esm/index.js` (the tarball
//! upstream's `yarn.lock` resolves, pinned in `tools/goldens`), so they
//! return the same doubles: `Math.hypot`, `Math.min` and `Math.max` go
//! through [`excali_math::js`], and the loops that step by `1/13` and `1/29`
//! accumulate the step in a double as the JS does. `goldens/freehand.json`
//! checks every branch.
//!
//! The crates.io `perfect_freehand` 0.1.1 was evaluated first (ex-213
//! evidence) and not used: it is not bit-compatible with 1.2.0 (the first
//! point's missing pressure defaults to 0.5 instead of 0.25, the cap loops
//! compute `i / 13` instead of accumulating `1/13`, lengths use
//! `powi(2).sqrt()` instead of V8's `Math.hypot`, and it adds a `closed`
//! option 1.2.0 does not have), and the architecture overview allows this
//! crate no external dependencies.
//!
//! # Excalidraw
//!
//! [`variable_width_outline`] is upstream's `getVariableWidthFreedrawOutline`
//! (`packages/element/src/shape.ts:1221-1245`): the outline of a freedraw
//! element whose `strokeOptions.variability` is not `"constant"`.

mod freedraw;
mod outline;
mod stroke_points;
mod vec;

pub use freedraw::{
    ease_out_sine, variable_width_options, variable_width_outline, DEFAULT_STROKE_STREAMLINE,
    VARIABLE_WIDTH_SIZE_FACTOR, VARIABLE_WIDTH_SMOOTHING, VARIABLE_WIDTH_THINNING,
};
pub use outline::{get_stroke_outline_points, get_stroke_radius};
pub use stroke_points::get_stroke_points;

/// An input point. perfect-freehand accepts `[x, y]`, `[x, y, pressure]` or
/// `{x, y, pressure}`; `pressure` is `None` where the JS value is
/// `undefined`. A pressure that is not `>= 0` (negative, NaN, missing) is
/// replaced by the library's default: 0.25 for the first point, 0.5 for the
/// others.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputPoint {
    pub x: f64,
    pub y: f64,
    pub pressure: Option<f64>,
}

impl InputPoint {
    /// `[x, y]`.
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            pressure: None,
        }
    }

    /// `[x, y, pressure]`.
    pub fn with_pressure(x: f64, y: f64, pressure: f64) -> Self {
        Self {
            x,
            y,
            pressure: Some(pressure),
        }
    }

    /// `{x, y, pressure}`: `getStrokePoints` maps objects to arrays with
    /// `pressure = 0.5` as the destructuring default, so a missing pressure
    /// is 0.5 here even for the first point.
    pub fn from_object(x: f64, y: f64, pressure: Option<f64>) -> Self {
        Self {
            x,
            y,
            pressure: Some(pressure.unwrap_or(0.5)),
        }
    }

    pub(crate) fn xy(&self) -> [f64; 2] {
        [self.x, self.y]
    }
}

/// `StrokePoint`: an input point after streamlining, with its pressure, the
/// unit vector back to the previous stroke point, the distance to it and the
/// running length of the stroke so far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokePoint {
    pub point: [f64; 2],
    pub pressure: f64,
    pub vector: [f64; 2],
    pub distance: f64,
    pub running_length: f64,
}

/// A start or end `taper`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Taper {
    /// `false`: no taper (same as a distance of 0).
    Off,
    /// `true`: taper over `max(size, totalLength)`.
    Full,
    /// A taper distance. 0 and NaN mean no taper, as they are falsy in JS.
    Distance(f64),
}

/// `options.start` / `options.end`.
#[derive(Clone, Copy, Debug)]
pub struct CapOptions {
    /// `cap`, default `true`: a round cap; `false` is a flat one.
    pub cap: bool,
    /// `taper`, `None` where the JS value is `undefined` (the default).
    pub taper: Option<Taper>,
    /// `easing` of the taper, a function of the fraction of the taper
    /// distance covered.
    pub easing: fn(f64) -> f64,
}

impl CapOptions {
    /// The defaults for `start`: round cap, no taper, `t * (2 - t)`.
    pub fn start() -> Self {
        Self {
            cap: true,
            taper: None,
            easing: |t| t * (2.0 - t),
        }
    }

    /// The defaults for `end`: round cap, no taper, `--t * t * t + 1`.
    pub fn end() -> Self {
        Self {
            cap: true,
            taper: None,
            easing: |t| {
                let t = t - 1.0;
                t * t * t + 1.0
            },
        }
    }
}

/// `StrokeOptions`. [`Default`] gives the library's destructuring defaults.
#[derive(Clone, Copy, Debug)]
pub struct StrokeOptions {
    /// The base diameter. Default 16.
    pub size: f64,
    /// The effect of pressure on the size. Default 0.5.
    pub thinning: f64,
    /// How much to soften the stroke's edges. Default 0.5.
    pub smoothing: f64,
    /// How much to streamline the input points. Default 0.5.
    pub streamline: f64,
    /// Applied to each point's pressure. Default the identity.
    pub easing: fn(f64) -> f64,
    /// Simulate pressure from the distance between points. Default `true`.
    pub simulate_pressure: bool,
    pub start: CapOptions,
    pub end: CapOptions,
    /// Whether the points form a completed stroke. Default `false`.
    pub last: bool,
}

impl Default for StrokeOptions {
    fn default() -> Self {
        Self {
            size: 16.0,
            thinning: 0.5,
            smoothing: 0.5,
            streamline: 0.5,
            easing: |t| t,
            simulate_pressure: true,
            start: CapOptions::start(),
            end: CapOptions::end(),
            last: false,
        }
    }
}

/// `getStroke(points, options)`: the outline polygon of a stroke through
/// `points`.
pub fn get_stroke(points: &[InputPoint], options: &StrokeOptions) -> Vec<[f64; 2]> {
    get_stroke_outline_points(&get_stroke_points(points, options), options)
}
