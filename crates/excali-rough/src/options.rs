//! rough.js `Options` / `ResolvedOptions` (`bin/core.d.ts`) with the
//! generator's `defaultOptions` block (`bin/generator.js`).

/// A rough.js point, `[x, y]`.
pub type Point = [f64; 2];

/// Resolved rough.js options: every field of `ResolvedOptions`, and the
/// optional fields of `Options` as `Option`s.
///
/// rough.js merges a call's options over the generator's defaults with
/// `Object.assign({}, defaultOptions, options)`; in Rust that is struct
/// update syntax, `Options { seed, roughness, ..Options::default() }` (or
/// `..generator.default_options().clone()`).
///
/// `fill` is the fill colour: `None` (rough.js `undefined`) draws no fill.
/// Which values count as "no fill" depends on the method, as in rough.js:
/// rectangle, polygon, ellipse and arc fill for any non-empty string,
/// `curve` also skips `"none"`, and `path` also skips `"transparent"`.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub max_randomness_offset: f64,
    pub roughness: f64,
    pub bowing: f64,
    pub stroke: String,
    pub stroke_width: f64,
    pub curve_fitting: f64,
    pub curve_tightness: f64,
    pub curve_step_count: f64,
    /// `fill`: the fill colour, `None` for no fill.
    pub fill: Option<String>,
    /// `fillStyle`: `hachure` (the default, and any unknown name),
    /// `solid`, `zigzag`, `cross-hatch`, `dots`, `dashed` or `zigzag-line`.
    pub fill_style: String,
    pub fill_weight: f64,
    pub hachure_angle: f64,
    pub hachure_gap: f64,
    pub simplification: Option<f64>,
    pub dash_offset: f64,
    pub dash_gap: f64,
    pub zigzag_offset: f64,
    /// The RNG seed; 0 means `Math.random` (see [`crate::Random`]). rough.js
    /// feeds it to `Math.imul`, which reads it as a 32-bit integer.
    pub seed: i32,
    pub stroke_line_dash: Option<Vec<f64>>,
    pub stroke_line_dash_offset: Option<f64>,
    pub fill_line_dash: Option<Vec<f64>>,
    pub fill_line_dash_offset: Option<f64>,
    pub disable_multi_stroke: bool,
    pub disable_multi_stroke_fill: bool,
    pub preserve_vertices: bool,
    pub fixed_decimal_place_digits: Option<f64>,
    pub fill_shape_roughness_gain: f64,
}

impl Default for Options {
    /// `RoughGenerator.defaultOptions` (rough.js 4.6.4 `bin/generator.js`).
    fn default() -> Self {
        Self {
            max_randomness_offset: 2.0,
            roughness: 1.0,
            bowing: 1.0,
            stroke: "#000".to_owned(),
            stroke_width: 1.0,
            curve_tightness: 0.0,
            curve_fitting: 0.95,
            curve_step_count: 9.0,
            fill: None,
            fill_style: "hachure".to_owned(),
            fill_weight: -1.0,
            hachure_angle: -41.0,
            hachure_gap: -1.0,
            dash_offset: -1.0,
            dash_gap: -1.0,
            zigzag_offset: -1.0,
            seed: 0,
            disable_multi_stroke: false,
            disable_multi_stroke_fill: false,
            preserve_vertices: false,
            fill_shape_roughness_gain: 0.8,
            simplification: None,
            stroke_line_dash: None,
            stroke_line_dash_offset: None,
            fill_line_dash: None,
            fill_line_dash_offset: None,
            fixed_decimal_place_digits: None,
        }
    }
}
