//! rough.js 4.6.4 `RoughGenerator` (`bin/generator.js`), strokes.
//!
//! Each method resolves its options, draws the outline with a fresh RNG
//! seeded from `options.seed` (rough.js creates the RNG on the merged options
//! object, so one call draws from one sequence), and returns a [`Drawable`].
//! As in rough.js, the outline is drawn even when `stroke` is `"none"` and
//! only left out of the result, so the draws a fill makes (ex-204) follow it.

use crate::core::{Drawable, OpSet, Shape};
use crate::path_data::PathError;
use crate::points_on_path::points_on_path;
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

    fn stroked(shape: Shape, outline: OpSet, o: &Options) -> Drawable {
        let sets = if o.stroke != NOS {
            vec![outline]
        } else {
            Vec::new()
        };
        Self::drawable(shape, sets, o)
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
        Self::stroked(Shape::Rectangle, outline, o)
    }

    /// `ellipse(x, y, width, height, options)`: centred on `(x, y)`.
    pub fn ellipse(&self, x: f64, y: f64, width: f64, height: f64, o: &Options) -> Drawable {
        let mut rng = Random::new(o.seed);
        let params = renderer::generate_ellipse_params(width, height, o, &mut rng);
        let response = renderer::ellipse_with_params(x, y, o, &params, &mut rng);
        Self::stroked(Shape::Ellipse, response.opset, o)
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
    /// radians; a closed arc is joined to the centre with rough lines.
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
        Self::stroked(Shape::Arc, outline, o)
    }

    /// `curve(points, options)`: a Catmull-Rom curve through the points.
    pub fn curve(&self, points: &[Point], o: &Options) -> Drawable {
        let mut rng = Random::new(o.seed);
        let outline = renderer::curve(points, o, &mut rng);
        Self::stroked(Shape::Curve, outline, o)
    }

    /// `polygon(points, options)`: a closed polyline.
    pub fn polygon(&self, points: &[Point], o: &Options) -> Drawable {
        let mut rng = Random::new(o.seed);
        let outline = renderer::linear_path(points, true, o, &mut rng);
        Self::stroked(Shape::Polygon, outline, o)
    }

    /// `path(d, options)`: SVG path data drawn rough. With `simplification`
    /// below 1 the stroke is the path flattened and simplified to
    /// `4 - 4 * simplification` and drawn as polylines instead.
    pub fn path(&self, d: &str, o: &Options) -> Result<Drawable, PathError> {
        if d.is_empty() {
            return Ok(Self::drawable(Shape::Path, Vec::new(), o));
        }
        let d = preprocess(d);
        let has_stroke = o.stroke != NOS;
        let simplification = o.simplification.unwrap_or(0.0);
        let simplified = simplification != 0.0 && !simplification.is_nan() && simplification < 1.0;
        let mut rng = Random::new(o.seed);
        // pointsOnPath(d, 1, distance) comes first in rough.js; it draws no
        // random numbers, and only the simplified stroke uses it here.
        let sets = if simplified {
            points_on_path(&d, 1.0, 4.0 - 4.0 * simplification)?
        } else {
            Vec::new()
        };
        let shape = renderer::svg_path(&d, o, &mut rng)?;
        let mut paths = Vec::new();
        if has_stroke {
            if simplified {
                for set in &sets {
                    paths.push(renderer::linear_path(set, false, o, &mut rng));
                }
            } else {
                paths.push(shape);
            }
        }
        Ok(Self::drawable(Shape::Path, paths, o))
    }
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
