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
use excali_freehand::variable_width_outline;

/// Why [`get_freedraw_outline_points`] or [`get_free_draw_svg_path`] gave
/// no outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreedrawOutlineError {
    /// The element is not a freedraw (upstream's functions take only
    /// `ExcalidrawFreeDrawElement`).
    NotFreedraw,
    /// `strokeOptions.variability` is `"constant"`: upstream draws the
    /// laser-pointer outline (`getConstantWidthFreedrawOutline`,
    /// `shape.ts:1247-1268`), which is not ported yet.
    // TODO(ex-214): port the laser pointer in excali-freehand and return
    // its outline here (size strokeWidth * 1.4, streamline, simplify 0,
    // pressure 1, `sizeMapping` max(0.1, pressure)).
    ConstantWidthNotPorted,
}

impl fmt::Display for FreedrawOutlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FreedrawOutlineError::NotFreedraw => f.write_str("not a freedraw element"),
            FreedrawOutlineError::ConstantWidthNotPorted => f.write_str(
                "constant-width freedraw (laser-pointer outline, shape.ts:1247-1277) \
                 is not ported yet (ex-214)",
            ),
        }
    }
}

impl std::error::Error for FreedrawOutlineError {}

/// `getFreedrawOutlinePoints(element)` (`shape.ts:1270-1277`): the outline
/// polygon of a freedraw element, switching on
/// `strokeOptions.variability`. Variable width (upstream's default for an
/// unknown or absent value, which restore writes as `"variable"`) is
/// [`get_variable_width_freedraw_outline`]; constant width is an explicit
/// [`FreedrawOutlineError::ConstantWidthNotPorted`] until ex-214 lands,
/// never a fallback to the variable outline.
pub fn get_freedraw_outline_points(
    element: &Element,
) -> Result<Vec<[f64; 2]>, FreedrawOutlineError> {
    let ElementKind::Freedraw(fields) = &element.kind else {
        return Err(FreedrawOutlineError::NotFreedraw);
    };
    match fields.stroke_options.variability {
        StrokeVariability::Constant => Err(FreedrawOutlineError::ConstantWidthNotPorted),
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
