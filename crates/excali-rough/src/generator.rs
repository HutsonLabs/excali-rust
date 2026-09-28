//! rough.js 4.6.4 `RoughGenerator` (`bin/generator.js`).
//!
//! Each method resolves its options, draws the outline with a fresh RNG
//! seeded from `options.seed` (rough.js creates the RNG on the merged options
//! object, so one call draws from one sequence), then the fill, and returns
//! a [`Drawable`] whose sets are the fill (if any) and then the outline. As in
//! rough.js, the outline is drawn even when `stroke` is `"none"` and only
//! left out of the result, so the draws the fill makes follow it.
//!
//! The polygons handed to the pattern fillers are rotated in place (see
//! [`crate::hachure_fill`]); where rough.js hands over the caller's own
//! points (`polygon`), the port fills a copy, so the caller's slice is left
//! alone.

use crate::core::{Drawable, Op, OpSet, OpSetType, Shape};
use crate::hachure_fill::PolygonList;
use crate::path_data::PathError;
use crate::points_on_curve::{curve_to_bezier, points_on_bezier_curves};
use crate::points_on_path::points_on_path_list;
use crate::renderer;
use crate::{random_seed, Options, Point, Random};

/// `NOS`: the stroke value that hides the outline.
const NOS: &str = "none";

/// `RoughGenerator`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoughGenerator {
    default_options: Options,
}

impl RoughGenerator {
    /// `new RoughGenerator()`: rough.js's default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// `new RoughGenerator({ options })`: `options` become the defaults.
    pub fn with_options(options: Options) -> Self {
        Self {
            default_options: options,
        }
    }

    /// `defaultOptions`, the base for struct-update syntax:
    /// `Options { seed, ..g.default_options().clone() }`.
    pub fn default_options(&self) -> &Options {
        &self.default_options
    }

    /// `RoughGenerator.newSeed()`: a random seed in `[0, 2^31)`.
    pub fn new_seed() -> i32 {
        random_seed()
    }

    fn drawable(shape: Shape, sets: Vec<OpSet>, o: &Options) -> Drawable {
        Drawable {
            shape,
            options: o.clone(),
            sets,
        }
    }

    /// `paths` (the fill, if any) and then the outline unless `stroke` is
    /// `"none"`.
    fn stroked(shape: Shape, mut paths: Vec<OpSet>, outline: OpSet, o: &Options) -> Drawable {
        if o.stroke != NOS {
            paths.push(outline);
        }
        Self::drawable(shape, paths, o)
    }

    /// `solidFillPolygon` or `patternFillPolygons` for one polygon, by
    /// `fillStyle`.
    fn fill_polygon(points: &[Point], o: &Options, rng: &mut Random) -> OpSet {
        if o.fill_style == "solid" {
            renderer::solid_fill_polygon(&[points.to_vec()], o, rng)
        } else {
            renderer::pattern_fill_polygons(&mut [points.to_vec()], o, rng)
        }
    }

    /// `line(x1, y1, x2, y2, options)`. Unlike the other shapes, a line is
    /// returned whatever the stroke.
    pub fn line(&self, x1: f64, y1: f64, x2: f64, y2: f64, o: &Options) -> Drawable {
        let mut rng = Random::new(o.seed);
        let set = renderer::line(x1, y1, x2, y2, o, &mut rng);
        Self::drawable(Shape::Line, vec![set], o)
    }

    /// `rectangle(x, y, width, height, options)`.
    pub fn rectangle(&self, x: f64, y: f64, width: f64, height: f64, o: &Options) -> Drawable {
        let mut rng = Random::new(o.seed);
        let outline = renderer::rectangle(x, y, width, height, o, &mut rng);
        let mut paths = Vec::new();
        if has_fill(o) {
            let points = [
                [x, y],
                [x + width, y],
                [x + width, y + height],
                [x, y + height],
            ];
            paths.push(Self::fill_polygon(&points, o, &mut rng));
        }
        Self::stroked(Shape::Rectangle, paths, outline, o)
    }

    /// `ellipse(x, y, width, height, options)`: centred on `(x, y)`.
    pub fn ellipse(&self, x: f64, y: f64, width: f64, height: f64, o: &Options) -> Drawable {
        let mut rng = Random::new(o.seed);
        let params = renderer::generate_ellipse_params(width, height, o, &mut rng);
        let mut response = renderer::ellipse_with_params(x, y, o, &params, &mut rng);
        let mut paths = Vec::new();
        if has_fill(o) {
            if o.fill_style == "solid" {
                let mut shape = renderer::ellipse_with_params(x, y, o, &params, &mut rng).opset;
                shape.kind = OpSetType::FillPath;
                paths.push(shape);
            } else {
                paths.push(renderer::pattern_fill_polygons(
                    &mut [std::mem::take(&mut response.estimated_points)],
                    o,
                    &mut rng,
                ));
            }
        }
        Self::stroked(Shape::Ellipse, paths, response.opset, o)
    }

    /// `circle(x, y, diameter, options)`: an ellipse named `circle`.
    pub fn circle(&self, x: f64, y: f64, diameter: f64, o: &Options) -> Drawable {
        let mut d = self.ellipse(x, y, diameter, diameter, o);
        d.shape = Shape::Circle;
        d
    }

    /// `linearPath(points, options)`: an open polyline, returned whatever the
    /// stroke.
    pub fn linear_path(&self, points: &[Point], o: &Options) -> Drawable {
        let mut rng = Random::new(o.seed);
        let set = renderer::linear_path(points, false, o, &mut rng);
        Self::drawable(Shape::LinearPath, vec![set], o)
    }

    /// `arc(x, y, width, height, start, stop, closed, options)`: angles in
    /// radians; a closed arc is joined to the centre with rough lines, and
    /// only a closed arc is filled (solid: the arc drawn once more, single
    /// stroke, closed with straight lines).
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        start: f64,
        stop: f64,
        closed: bool,
        o: &Options,
    ) -> Drawable {
        let mut rng = Random::new(o.seed);
        let outline = renderer::arc(x, y, width, height, start, stop, closed, true, o, &mut rng);
        let mut paths = Vec::new();
        if closed && has_fill(o) {
            if o.fill_style == "solid" {
                // the arc always draws, so the copy shares the randomizer
                let fill_options = Options {
                    disable_multi_stroke: true,
                    ..o.clone()
                };
                let mut shape = renderer::arc(
                    x,
                    y,
                    width,
                    height,
                    start,
                    stop,
                    true,
                    false,
                    &fill_options,
                    &mut rng,
                );
                shape.kind = OpSetType::FillPath;
                paths.push(shape);
            } else {
                paths.push(renderer::pattern_fill_arc(
                    x, y, width, height, start, stop, o, &mut rng,
                ));
            }
        }
        Self::stroked(Shape::Arc, paths, outline, o)
    }

    /// `curve(points, options)`: a Catmull-Rom curve through the points,
    /// filled when it has three points or more (solid: the curve drawn once
    /// more at `roughness + fillShapeRoughnessGain`, single stroke, as one
    /// subpath; pattern: the curve flattened with `pointsOnBezierCurves`).
    ///
    /// [`PathError::CallStackExceeded`] where rough.js overflows the stack:
    /// a pattern-filled curve that never flattens (a non-finite point).
    pub fn curve(&self, points: &[Point], o: &Options) -> Result<Drawable, PathError> {
        let mut rng = Random::new(o.seed);
        let outline = renderer::curve(points, o, &mut rng);
        let mut paths = Vec::new();
        if has_fill(o) && o.fill.as_deref() != Some(NOS) && points.len() >= 3 {
            if o.fill_style == "solid" {
                let fill_options = Options {
                    disable_multi_stroke: true,
                    roughness: fill_roughness(o),
                    ..o.clone()
                };
                // three points or more always draw, so the copy shares the
                // randomizer
                let fill_shape = renderer::curve(points, &fill_options, &mut rng);
                paths.push(OpSet::fill_path(merged_shape(fill_shape.ops)));
            } else {
                let bcurve =
                    curve_to_bezier(points, 0.0).expect("three points or more make a Bezier chain");
                let poly_points =
                    points_on_bezier_curves(&bcurve, 10.0, Some((1.0 + o.roughness) / 2.0))?;
                paths.push(renderer::pattern_fill_polygons(
                    &mut [poly_points],
                    o,
                    &mut rng,
                ));
            }
        }
        Ok(Self::stroked(Shape::Curve, paths, outline, o))
    }

    /// `polygon(points, options)`: a closed polyline.
    pub fn polygon(&self, points: &[Point], o: &Options) -> Drawable {
        let mut rng = Random::new(o.seed);
        let outline = renderer::linear_path(points, true, o, &mut rng);
        let mut paths = Vec::new();
        if has_fill(o) {
            paths.push(Self::fill_polygon(points, o, &mut rng));
        }
        Self::stroked(Shape::Polygon, paths, outline, o)
    }

    /// `path(d, options)`: SVG path data drawn rough. With `simplification`
    /// below 1 the stroke is the path flattened and simplified to
    /// `4 - 4 * simplification` and drawn as polylines instead.
    ///
    /// The fill (unless `fill` is `"transparent"` or `"none"`) is drawn from
    /// the flattened subpaths: solid with one subpath is the path drawn once
    /// more at `roughness + fillShapeRoughnessGain`, single stroke, as one
    /// subpath; solid with several is `solidFillPolygon` over them; a pattern
    /// fills them all, rotating them in place first, so a simplified stroke
    /// drawn after it follows the rotated points as rough.js's does.
    ///
    /// Errors where rough.js throws: path data the parser rejects, and
    /// [`PathError::CallStackExceeded`] where rough.js overflows the stack.
    /// As in rough.js the path is flattened (`pointsOnPath(d, 1, distance)`)
    /// whether or not it is simplified, to `(1 + roughness) / 2` when it is
    /// not, so a curve that never flattens (a coordinate that is, or
    /// overflows to, infinity or NaN) or a roughness below -1 (a negative
    /// distance) is an error with or without `simplification`.
    ///
    /// Documented divergences: data with a number after `Z`, on which
    /// rough.js never returns, is [`PathError::ParamAfterClose`]; a path
    /// whose simplification recurses deeper than the JavaScript stack allows
    /// is simplified rather than an error (see
    /// [`crate::points_on_curve::simplify_points`]).
    pub fn path(&self, d: &str, o: &Options) -> Result<Drawable, PathError> {
        if d.is_empty() {
            return Ok(Self::drawable(Shape::Path, Vec::new(), o));
        }
        let d = preprocess(d);
        let fill = has_fill(o)
            && o.fill.as_deref() != Some("transparent")
            && o.fill.as_deref() != Some(NOS);
        let has_stroke = o.stroke != NOS;
        let simplification = o.simplification.unwrap_or(0.0);
        let simplified = simplification != 0.0 && !simplification.is_nan() && simplification < 1.0;
        let distance = if simplified {
            4.0 - 4.0 * simplification
        } else {
            (1.0 + o.roughness) / 2.0
        };
        let mut rng = Random::new(o.seed);
        // pointsOnPath comes first in rough.js and draws no random numbers
        let mut sets = points_on_path_list(&d, 1.0, distance)?;
        let shape = renderer::svg_path(&d, o, &mut rng)?;
        let mut paths = Vec::new();
        if fill {
            paths.push(Self::fill_path_sets(&d, &mut sets, o, &mut rng)?);
        }
        if has_stroke {
            if simplified {
                for set in sets.to_vecs() {
                    paths.push(renderer::linear_path(&set, false, o, &mut rng));
                }
            } else {
                paths.push(shape);
            }
        }
        Ok(Self::drawable(Shape::Path, paths, o))
    }

    /// The fill branch of `path`.
    fn fill_path_sets(
        d: &str,
        sets: &mut PolygonList,
        o: &Options,
        rng: &mut Random,
    ) -> Result<OpSet, PathError> {
        if o.fill_style != "solid" {
            return Ok(renderer::pattern_fill_list(sets, o, rng));
        }
        if sets.len() != 1 {
            return Ok(renderer::solid_fill_polygon(&sets.to_vecs(), o, rng));
        }
        let fill_options = Options {
            disable_multi_stroke: true,
            roughness: fill_roughness(o),
            ..o.clone()
        };
        // The copy shares the randomizer if the outline drew; a path that
        // drew nothing (moves only) leaves it to make its own.
        let fill_shape = if rng.has_drawn() {
            renderer::svg_path(d, &fill_options, rng)?
        } else {
            renderer::svg_path(d, &fill_options, &mut Random::new(o.seed))?
        };
        Ok(OpSet::fill_path(merged_shape(fill_shape.ops)))
    }
}

/// `o.fill` is truthy: set and not empty.
fn has_fill(o: &Options) -> bool {
    o.fill.as_deref().is_some_and(|f| !f.is_empty())
}

/// `o.roughness ? (o.roughness + o.fillShapeRoughnessGain) : 0`.
fn fill_roughness(o: &Options) -> f64 {
    if o.roughness != 0.0 && !o.roughness.is_nan() {
        o.roughness + o.fill_shape_roughness_gain
    } else {
        0.0
    }
}

/// `_mergedShape(input)`: the first op, then every op that is not a move.
fn merged_shape(input: Vec<Op>) -> Vec<Op> {
    input
        .into_iter()
        .enumerate()
        .filter(|(i, op)| *i == 0 || !matches!(op, Op::Move(_)))
        .map(|(_, op)| op)
        .collect()
}

/// `d.replace(/\n/g, ' ').replace(/(-\s)/g, '-').replace('/(\s\s)/g', ' ')`.
///
/// The last call passes a string, not a regular expression, so it replaces
/// the first literal occurrence of `/(\s\s)/g`, which path data never holds;
/// it is kept for fidelity.
fn preprocess(d: &str) -> String {
    let d = d.replace('\n', " ");
    let mut out = String::with_capacity(d.len());
    let mut chars = d.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '-' && chars.peek().copied().is_some_and(is_js_whitespace) {
            chars.next();
        }
    }
    out.replacen("/(\\s\\s)/g", " ", 1)
}

/// ECMAScript `\s`: WhiteSpace and LineTerminator.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_joins_minus_and_whitespace() {
        assert_eq!(preprocess("L - 20 30\n-\t5"), "L -20 30 -5");
        // one whitespace character per match, matches do not overlap
        assert_eq!(preprocess("-  5"), "- 5");
        assert_eq!(preprocess("- - 5"), "--5");
        assert_eq!(preprocess("-\u{00A0}5"), "-5");
        assert_eq!(preprocess("a/(\\s\\s)/gb/(\\s\\s)/g"), "a b/(\\s\\s)/g");
    }
}
