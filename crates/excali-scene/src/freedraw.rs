//! Freedraw strokes as SVG paths (`packages/element/src/shape.ts:1186-1344`
//! at the pinned commit). See `site/content/research/rendering.md`
//! section 2.
//!
//! A freedraw element's stroke is not a rough.js shape: its outline polygon
//! (perfect-freehand, or the laser pointer for constant width) becomes an
//! SVG path string that the renderer fills with the stroke colour.
//! [`get_svg_path_from_stroke`] is upstream's `getSvgPathFromStroke`:
//!
//! ```text
//! M p0 Q p0 mid(p0, p1) p1 mid(p1, p2) … pn mid(pn, p0) L p0 Z
//! ```
//!
//! built as a JS array joined with spaces (so each point is written as
//! `String([x, y])`, `x,y`, with `Number::toString`), then every number with
//! a decimal point truncated (not rounded) to two decimals by the
//! `TO_FIXED_PRECISION` regex ([`trim_to_fixed_precision`]).

use std::fmt;

use excali_core::element::{Element, ElementKind, StrokeVariability};
use excali_core::json::number_to_string;
use excali_freehand::{
    constant_width_outline, get_stroke_points, variable_width_outline, InputPoint, StrokeOptions,
    CONSTANT_WIDTH_SIZE_FACTOR, VARIABLE_WIDTH_SIZE_FACTOR,
};
use excali_math::{polygon_from_points, LocalPoint as MathLocalPoint, Point, Polygon};
use excali_rough::points_on_curve::{points_on_bezier_curves, simplify};
use excali_rough::{Options, RoughGenerator};

/// Why [`get_freedraw_outline_points`] or [`get_free_draw_svg_path`] gave
/// no outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreedrawOutlineError {
    /// The element is not a freedraw (upstream's functions take only
    /// `ExcalidrawFreeDrawElement`).
    NotFreedraw,
}

impl fmt::Display for FreedrawOutlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FreedrawOutlineError::NotFreedraw => f.write_str("not a freedraw element"),
        }
    }
}

impl std::error::Error for FreedrawOutlineError {}

/// `getFreedrawOutlinePoints(element)` (`shape.ts:1270-1277`): the outline
/// polygon of a freedraw element, switching on
/// `strokeOptions.variability`. Variable width (upstream's default for an
/// unknown or absent value, which restore writes as `"variable"`) is
/// [`get_variable_width_freedraw_outline`]; constant width is
/// [`get_constant_width_freedraw_outline`], the laser-pointer outline.
pub fn get_freedraw_outline_points(
    element: &Element,
) -> Result<Vec<[f64; 2]>, FreedrawOutlineError> {
    let ElementKind::Freedraw(fields) = &element.kind else {
        return Err(FreedrawOutlineError::NotFreedraw);
    };
    match fields.stroke_options.variability {
        StrokeVariability::Constant => {
            get_constant_width_freedraw_outline(element).ok_or(FreedrawOutlineError::NotFreedraw)
        }
        StrokeVariability::Variable => {
            get_variable_width_freedraw_outline(element).ok_or(FreedrawOutlineError::NotFreedraw)
        }
    }
}

/// `getFreeDrawSvgPath(element)` (`shape.ts:1187-1191`):
/// [`get_svg_path_from_stroke`] of [`get_freedraw_outline_points`].
pub fn get_free_draw_svg_path(element: &Element) -> Result<String, FreedrawOutlineError> {
    get_freedraw_outline_points(element).map(|points| get_svg_path_from_stroke(&points))
}

/// `getVariableWidthFreedrawOutline(element)` (`shape.ts:1224-1245`): the
/// perfect-freehand outline of a freedraw element, with the element's
/// `strokeWidth`, `simulatePressure`, `pressures` and
/// `strokeOptions.streamline`. `None` for any other element type.
///
/// This ignores `strokeOptions.variability`; [`get_freedraw_outline_points`]
/// is the switch.
pub fn get_variable_width_freedraw_outline(element: &Element) -> Option<Vec<[f64; 2]>> {
    let ElementKind::Freedraw(fields) = &element.kind else {
        return None;
    };
    Some(variable_width_outline(
        &fields.points,
        &fields.pressures,
        element.base.stroke_width,
        Some(fields.stroke_options.streamline),
        fields.simulate_pressure,
    ))
}

/// `getConstantWidthFreedrawOutline(element)` (`shape.ts:1247-1268`): the
/// laser-pointer outline of a freedraw element (size `strokeWidth * 1.4`,
/// the element's `strokeOptions.streamline`, simplify 0, every point at
/// pressure 1). `None` for any other element type.
///
/// This ignores `strokeOptions.variability`; [`get_freedraw_outline_points`]
/// is the switch.
pub fn get_constant_width_freedraw_outline(element: &Element) -> Option<Vec<[f64; 2]>> {
    let ElementKind::Freedraw(fields) = &element.kind else {
        return None;
    };
    Some(constant_width_outline(
        &fields.points,
        element.base.stroke_width,
        Some(fields.stroke_options.streamline),
    ))
}

/// The distance `getFreedrawFillCurvePoints` simplifies a freedraw's
/// points to (`shape.ts:580-581`).
pub const FREEDRAW_FILL_SIMPLIFY_DISTANCE: f64 = 0.75;

/// `getFreedrawFillCurvePoints(element)` (`shape.ts:580-581`): the
/// simplified centerline the freedraw fill is drawn along,
/// points-on-curve's `simplify(element.points, 0.75)` (Ramer-Douglas-Peucker).
/// `None` for any other element type.
pub fn get_freedraw_fill_curve_points(element: &Element) -> Option<Vec<[f64; 2]>> {
    let ElementKind::Freedraw(fields) = &element.kind else {
        return None;
    };
    // simplify only fails for a negative distance
    Some(
        simplify(&fields.points, FREEDRAW_FILL_SIMPLIFY_DISTANCE)
            .expect("a non-negative distance always simplifies"),
    )
}

/// `getFreedrawFillPolygon(element)` (`shape.ts:583-619`): the flattened
/// contour of a freedraw loop's fill, in local unrotated coordinates,
/// following the rendered fill rather than the stroke outline: rough.js's
/// `curve` through [`get_freedraw_fill_curve_points`] at roughness 0 with a
/// single stroke, its control points flattened by points-on-curve's
/// `pointsOnBezierCurves(points, 0.5)` and closed (`polygonFromPoints`).
/// `None` for any other element type.
///
/// Upstream draws with an unseeded generator (seed 0, `Math.random`); every
/// draw is multiplied by the roughness 0, so none reaches the contour.
/// Upstream caches the contour per element and version; the port computes
/// it afresh, which gives the same points.
pub fn get_freedraw_fill_polygon(element: &Element) -> Option<Polygon<excali_math::Local>> {
    let points = get_freedraw_fill_curve_points(element)?;
    let generator = RoughGenerator::new();
    let options = Options {
        roughness: 0.0,
        disable_multi_stroke: true,
        ..generator.default_options().clone()
    };
    let ops = generator
        .curve(&points, &options)
        .ok()
        .and_then(|drawable| drawable.sets.into_iter().next())
        .map(|set| set.ops)
        .unwrap_or_default();
    // a single curve pass is a move followed by cubic control points
    let bezier_points: Vec<[f64; 2]> = ops
        .iter()
        .flat_map(|op| {
            op.data()
                .chunks(2)
                .map(|c| [c[0], c[1]])
                .collect::<Vec<_>>()
        })
        .collect();
    // no distance: the boundary is not simplified again
    let flattened = points_on_bezier_curves(&bezier_points, 0.5, None).unwrap_or_default();
    Some(polygon_from_points(
        flattened
            .into_iter()
            .map(|[x, y]| -> MathLocalPoint { Point::new(x, y) })
            .collect(),
    ))
}

/// `getFreedrawMaxStrokeRadius(element)` (`shape.ts:1279-1291`): how far a
/// freedraw stroke's ink can reach past its centerline points,
/// `strokeWidth * 1.4` at constant width, `strokeWidth * 4.25 + 3` at
/// variable width (perfect-freehand's start cap on strokes under 3px).
/// 0 for any other element type.
pub fn get_freedraw_max_stroke_radius(element: &Element) -> f64 {
    let ElementKind::Freedraw(fields) = &element.kind else {
        return 0.0;
    };
    let stroke_width = element.base.stroke_width;
    match fields.stroke_options.variability {
        StrokeVariability::Constant => stroke_width * CONSTANT_WIDTH_SIZE_FACTOR,
        StrokeVariability::Variable => stroke_width * VARIABLE_WIDTH_SIZE_FACTOR + 3.0,
    }
}

/// `getFreedrawStrokeCenterPoints(element)` (`shape.ts:1305-1312`): the
/// rendered centerline of a freedraw stroke, perfect-freehand's
/// streamline-smoothed stroke points (`getStrokePoints` with size
/// `strokeWidth * 4.25`, the element's streamline and `last: true`; the
/// points go in as `[x, y]`, without pressures). `None` for any other
/// element type.
pub fn get_freedraw_stroke_center_points(element: &Element) -> Option<Vec<[f64; 2]>> {
    let ElementKind::Freedraw(fields) = &element.kind else {
        return None;
    };
    let input: Vec<InputPoint> = fields
        .points
        .iter()
        .map(|&[x, y]| InputPoint::new(x, y))
        .collect();
    let options = StrokeOptions {
        size: element.base.stroke_width * VARIABLE_WIDTH_SIZE_FACTOR,
        streamline: fields.stroke_options.streamline,
        last: true,
        ..StrokeOptions::default()
    };
    Some(
        get_stroke_points(&input, &options)
            .into_iter()
            .map(|p| p.point)
            .collect(),
    )
}

/// `med(A, B)` (`shape.ts:1314-1316`).
fn med(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// `getSvgPathFromStroke(points)` (`shape.ts:1323-1344`): the closed
/// quadratic path through the midpoints of an outline polygon, numbers
/// trimmed to two decimals. An empty outline gives `""`.
pub fn get_svg_path_from_stroke(points: &[[f64; 2]]) -> String {
    let Some(&first) = points.first() else {
        return String::new();
    };
    // ["M", points[0], "Q", p_i, med(p_i, p_i+1), …, "L", points[0], "Z"]
    // joined with " "; an array item is written as `x,y`.
    let point = |out: &mut String, [x, y]: [f64; 2]| {
        out.push(' ');
        out.push_str(&number_to_string(x));
        out.push(',');
        out.push_str(&number_to_string(y));
    };
    let mut d = String::from("M");
    point(&mut d, first);
    d.push_str(" Q");
    for (i, &p) in points.iter().enumerate() {
        let next = points.get(i + 1).copied().unwrap_or(first);
        point(&mut d, p);
        point(&mut d, med(p, next));
    }
    d.push_str(" L");
    point(&mut d, first);
    d.push_str(" Z");
    trim_to_fixed_precision(&d)
}

/// `s.replace(TO_FIXED_PRECISION, "$1")` with upstream's
/// `/(\s?[A-Z]?,?-?[0-9]*\.[0-9]{0,2})(([0-9]|e|-)*)/g` (`shape.ts:1321`):
/// every run that has a decimal point keeps an optional whitespace, capital
/// letter, comma and minus, its integer digits, the point and at most two
/// decimals; the digits, `e` and `-` that follow are dropped. So
/// `1.5e-7` becomes `1.5` and `1.5e+21` becomes `1.5+21`, as in upstream.
///
/// The regex needs no backtracking: each optional item, when skipped,
/// leaves a character the next item cannot take, so trying each start
/// position with greedy matching finds exactly the matches of the JS
/// engine.
pub fn trim_to_fixed_precision(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    // `copied` is the byte offset up to which `s` is in `out`.
    let mut copied = 0;
    let mut start = 0;
    while start < b.len() {
        match match_at(s, start) {
            Some((keep_end, match_end)) => {
                out.push_str(&s[copied..keep_end]);
                copied = match_end;
                start = match_end;
            }
            None => {
                // advance one character (whitespace may be multi-byte)
                start += s[start..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    out.push_str(&s[copied..]);
    out
}

/// A match of `TO_FIXED_PRECISION` starting at byte `start`: the end of
/// group 1 and the end of the whole match.
fn match_at(s: &str, start: usize) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let mut i = start;
    // \s? (ECMAScript WhiteSpace and LineTerminator)
    if let Some(c) = s[i..].chars().next() {
        if is_js_whitespace(c) {
            i += c.len_utf8();
        }
    }
    let take = |i: &mut usize, pred: fn(u8) -> bool| {
        if *i < b.len() && pred(b[*i]) {
            *i += 1;
            true
        } else {
            false
        }
    };
    take(&mut i, |c| c.is_ascii_uppercase()); // [A-Z]?
    take(&mut i, |c| c == b','); // ,?
    take(&mut i, |c| c == b'-'); // -?
    while take(&mut i, |c| c.is_ascii_digit()) {} // [0-9]*
    if !take(&mut i, |c| c == b'.') {
        return None; // \.
    }
    for _ in 0..2 {
        take(&mut i, |c| c.is_ascii_digit()); // [0-9]{0,2}
    }
    let keep_end = i;
    while take(&mut i, |c| c.is_ascii_digit() || c == b'e' || c == b'-') {} // ([0-9]|e|-)*
    Some((keep_end, i))
}

/// ECMAScript `\s`: WhiteSpace (tab, VT, FF, space, NBSP, ZWNBSP and the
/// Unicode `Zs` spaces) and LineTerminator (LF, CR, LS, PS).
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\u{b}'
            | '\u{c}'
            | ' '
            | '\u{a0}'
            | '\u{feff}'
            | '\n'
            | '\r'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}
