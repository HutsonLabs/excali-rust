//! `path-data-parser` 0.1.0 (rough.js 4.6.4's SVG path parser): `parsePath`,
//! `absolutize` and `normalize`.
//!
//! `parsePath` tokenises with the package's own rules (numbers such as
//! `.5.5`, `-3e1` and `1.` split without separators; separators are space,
//! tab, CR, LF and comma), repeats the last command for extra parameters
//! (a repeated `M`/`m` becomes `L`/`l`), and retries with `M0,0` prepended
//! when the data does not start with a move. `normalize` reduces an
//! absolute path to `M`, `L`, `C` and `Z`, turning quadratics, smooth curves
//! and arcs into cubics.

use excali_math::js;
use std::fmt;

use crate::Point;

/// One path command with its parameters (`{ key, data }`).
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub key: char,
    pub data: Vec<f64>,
}

/// Why path data could not be parsed or flattened. rough.js throws in the
/// first three cases (the first is a `TypeError` from reading an empty token
/// list), never returns in the fourth and throws `RangeError` in the fifth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    /// A character that is not a command, a number or a separator.
    InvalidData,
    /// `Param not a number: <mode>,<text>`: a command's parameter slot holds
    /// a command (or the end of the data).
    ParamNotANumber { mode: char, text: String },
    /// `Path data ended short`: fewer parameters left than the command takes.
    EndedShort,
    /// A number after `Z`/`z`. `Z` takes no parameters, so the parser never
    /// consumes the number and path-data-parser 0.1.0 loops forever.
    ParamAfterClose,
    /// `RangeError: Maximum call stack size exceeded`: points-on-curve's
    /// recursion never bottoms out. Bezier flattening never reaches its
    /// tolerance when a coordinate is, or overflows to, a non-finite value
    /// or when the tolerance is not positive; Ramer–Douglas–Peucker
    /// simplification never stops on a negative epsilon.
    CallStackExceeded,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathError::InvalidData => write!(f, "Invalid path data"),
            PathError::ParamNotANumber { mode, text } => {
                write!(f, "Param not a number: {mode},{text}")
            }
            PathError::EndedShort => write!(f, "Path data ended short"),
            PathError::ParamAfterClose => write!(f, "Number after a closepath"),
            PathError::CallStackExceeded => write!(f, "Maximum call stack size exceeded"),
        }
    }
}

impl std::error::Error for PathError {}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Command(char),
    Number(f64),
    Eod,
}

/// `PARAMS[key]`.
fn params(key: char) -> usize {
    match key.to_ascii_uppercase() {
        'A' => 7,
        'C' => 6,
        'Q' | 'S' => 4,
        'L' | 'M' | 'T' => 2,
        'H' | 'V' => 1,
        _ => 0, // Z
    }
}

/// Length of the number at the start of `s`, if any:
/// `^(([-+]?[0-9]+(\.[0-9]*)?|[-+]?\.[0-9]+)([eE][-+]?[0-9]+)?)`.
fn number_len(s: &[u8]) -> Option<usize> {
    let digits = |from: usize| s[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    let mut i = usize::from(matches!(s.first(), Some(b'+' | b'-')));
    let int = digits(i);
    if int > 0 {
        i += int;
        if s.get(i) == Some(&b'.') {
            i += 1 + digits(i + 1);
        }
    } else if s.get(i) == Some(&b'.') && digits(i + 1) > 0 {
        i += 1 + digits(i + 1);
    } else {
        return None;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(s.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        let exp = digits(j);
        if exp > 0 {
            i = j + exp;
        }
    }
    Some(i)
}

/// `tokenize(d)`; `None` where the package returns an empty list.
fn tokenize(d: &str) -> Option<Vec<Token>> {
    let s = d.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b',') {
            i += 1;
        } else if b"aAcChHlLmMqQsStTvVzZ".contains(&c) {
            tokens.push(Token::Command(char::from(c)));
            i += 1;
        } else {
            // Anything else must be a number, or the data is invalid.
            let n = number_len(&s[i..])?;
            // The token text is `${parseFloat(match)}` and the parameter is
            // `+text`: the nearest double, with -0 printed (and so read
            // back) as 0.
            let text = std::str::from_utf8(&s[i..i + n]).ok()?;
            let v: f64 = text.parse().ok()?;
            tokens.push(Token::Number(if v == 0.0 { 0.0 } else { v }));
            i += n;
        }
    }
    tokens.push(Token::Eod);
    Some(tokens)
}

fn token_text(t: &Token) -> String {
    match t {
        Token::Command(c) => c.to_string(),
        Token::Number(v) => v.to_string(),
        Token::Eod => String::new(),
    }
}

/// `parsePath(d)`.
pub fn parse_path(d: &str) -> Result<Vec<Segment>, PathError> {
    let tokens = tokenize(d).ok_or(PathError::InvalidData)?;
    let mut segments = Vec::new();
    // None is 'BOD', the beginning of data.
    let mut mode: Option<char> = None;
    let mut index = 0;
    let mut token = &tokens[index];
    while *token != Token::Eod {
        let params_count;
        match (mode, token) {
            (None, Token::Command(c @ ('M' | 'm'))) => {
                index += 1;
                params_count = params(*c);
                mode = Some(*c);
            }
            (None, _) => return parse_path(&format!("M0,0{d}")),
            (Some(m), Token::Number(_)) => {
                params_count = params(m);
                if params_count == 0 {
                    return Err(PathError::ParamAfterClose);
                }
            }
            (Some(_), Token::Command(c)) => {
                index += 1;
                params_count = params(*c);
                mode = Some(*c);
            }
            (Some(_), Token::Eod) => unreachable!("loop condition"),
        }
        let m = mode.expect("set above");
        if index + params_count < tokens.len() {
            let mut data = Vec::with_capacity(params_count);
            for t in &tokens[index..index + params_count] {
                match t {
                    Token::Number(v) => data.push(*v),
                    other => {
                        return Err(PathError::ParamNotANumber {
                            mode: m,
                            text: token_text(other),
                        })
                    }
                }
            }
            segments.push(Segment { key: m, data });
            index += params_count;
            token = &tokens[index];
            if m == 'M' {
                mode = Some('L');
            }
            if m == 'm' {
                mode = Some('l');
            }
        } else {
            return Err(PathError::EndedShort);
        }
    }
    Ok(segments)
}

fn seg(key: char, data: Vec<f64>) -> Segment {
    Segment { key, data }
}

/// `absolutize(segments)`: relative commands to absolute ones.
pub fn absolutize(segments: &[Segment]) -> Vec<Segment> {
    let (mut cx, mut cy) = (0.0, 0.0);
    let (mut subx, mut suby) = (0.0, 0.0);
    let mut out = Vec::with_capacity(segments.len());
    // `(i % 2) ? (d + cy) : (d + cx)`
    let shift = |data: &[f64], cx: f64, cy: f64| -> Vec<f64> {
        data.iter()
            .enumerate()
            .map(|(i, d)| if i % 2 == 1 { d + cy } else { d + cx })
            .collect()
    };
    for Segment { key, data } in segments {
        match key {
            'M' => {
                out.push(seg('M', data.clone()));
                (cx, cy) = (data[0], data[1]);
                (subx, suby) = (data[0], data[1]);
            }
            'm' => {
                cx += data[0];
                cy += data[1];
                out.push(seg('M', vec![cx, cy]));
                subx = cx;
                suby = cy;
            }
            'L' => {
                out.push(seg('L', data.clone()));
                (cx, cy) = (data[0], data[1]);
            }
            'l' => {
                cx += data[0];
                cy += data[1];
                out.push(seg('L', vec![cx, cy]));
            }
            'C' => {
                out.push(seg('C', data.clone()));
                cx = data[4];
                cy = data[5];
            }
            'c' => {
                let newdata = shift(data, cx, cy);
                cx = newdata[4];
                cy = newdata[5];
                out.push(seg('C', newdata));
            }
            'Q' => {
                out.push(seg('Q', data.clone()));
                cx = data[2];
                cy = data[3];
            }
            'q' => {
                let newdata = shift(data, cx, cy);
                cx = newdata[2];
                cy = newdata[3];
                out.push(seg('Q', newdata));
            }
            'A' => {
                out.push(seg('A', data.clone()));
                cx = data[5];
                cy = data[6];
            }
            'a' => {
                cx += data[5];
                cy += data[6];
                out.push(seg(
                    'A',
                    vec![data[0], data[1], data[2], data[3], data[4], cx, cy],
                ));
            }
            'H' => {
                out.push(seg('H', data.clone()));
                cx = data[0];
            }
            'h' => {
                cx += data[0];
                out.push(seg('H', vec![cx]));
            }
            'V' => {
                out.push(seg('V', data.clone()));
                cy = data[0];
            }
            'v' => {
                cy += data[0];
                out.push(seg('V', vec![cy]));
            }
            'S' => {
                out.push(seg('S', data.clone()));
                cx = data[2];
                cy = data[3];
            }
            's' => {
                let newdata = shift(data, cx, cy);
                cx = newdata[2];
                cy = newdata[3];
                out.push(seg('S', newdata));
            }
            'T' => {
                out.push(seg('T', data.clone()));
                cx = data[0];
                cy = data[1];
            }
            't' => {
                cx += data[0];
                cy += data[1];
                out.push(seg('T', vec![cx, cy]));
            }
            'Z' | 'z' => {
                out.push(seg('Z', Vec::new()));
                cx = subx;
                cy = suby;
            }
            _ => {}
        }
    }
    out
}

/// `normalize(segments)`: an absolute path reduced to `M`, `L`, `C` and `Z`.
pub fn normalize(segments: &[Segment]) -> Vec<Segment> {
    let mut out = Vec::with_capacity(segments.len());
    let mut last_type = ' ';
    let (mut cx, mut cy) = (0.0, 0.0);
    let (mut subx, mut suby) = (0.0, 0.0);
    let (mut lcx, mut lcy) = (0.0, 0.0);
    for Segment { key, data } in segments {
        match key {
            'M' => {
                out.push(seg('M', data.clone()));
                (cx, cy) = (data[0], data[1]);
                (subx, suby) = (data[0], data[1]);
            }
            'C' => {
                out.push(seg('C', data.clone()));
                cx = data[4];
                cy = data[5];
                lcx = data[2];
                lcy = data[3];
            }
            'L' => {
                out.push(seg('L', data.clone()));
                (cx, cy) = (data[0], data[1]);
            }
            'H' => {
                cx = data[0];
                out.push(seg('L', vec![cx, cy]));
            }
            'V' => {
                cy = data[0];
                out.push(seg('L', vec![cx, cy]));
            }
            'S' => {
                let (cx1, cy1) = if last_type == 'C' || last_type == 'S' {
                    (cx + (cx - lcx), cy + (cy - lcy))
                } else {
                    (cx, cy)
                };
                out.push(seg('C', vec![cx1, cy1, data[0], data[1], data[2], data[3]]));
                lcx = data[0];
                lcy = data[1];
                cx = data[2];
                cy = data[3];
            }
            'T' => {
                let (x, y) = (data[0], data[1]);
                let (x1, y1) = if last_type == 'Q' || last_type == 'T' {
                    (cx + (cx - lcx), cy + (cy - lcy))
                } else {
                    (cx, cy)
                };
                let cx1 = cx + 2.0 * (x1 - cx) / 3.0;
                let cy1 = cy + 2.0 * (y1 - cy) / 3.0;
                let cx2 = x + 2.0 * (x1 - x) / 3.0;
                let cy2 = y + 2.0 * (y1 - y) / 3.0;
                out.push(seg('C', vec![cx1, cy1, cx2, cy2, x, y]));
                lcx = x1;
                lcy = y1;
                cx = x;
                cy = y;
            }
            'Q' => {
                let (x1, y1, x, y) = (data[0], data[1], data[2], data[3]);
                let cx1 = cx + 2.0 * (x1 - cx) / 3.0;
                let cy1 = cy + 2.0 * (y1 - cy) / 3.0;
                let cx2 = x + 2.0 * (x1 - x) / 3.0;
                let cy2 = y + 2.0 * (y1 - y) / 3.0;
                out.push(seg('C', vec![cx1, cy1, cx2, cy2, x, y]));
                lcx = x1;
                lcy = y1;
                cx = x;
                cy = y;
            }
            'A' => {
                let r1 = data[0].abs();
                let r2 = data[1].abs();
                let angle = data[2];
                let large_arc_flag = data[3];
                let sweep_flag = data[4];
                let x = data[5];
                let y = data[6];
                if r1 == 0.0 || r2 == 0.0 {
                    out.push(seg('C', vec![cx, cy, x, y, x, y]));
                    cx = x;
                    cy = y;
                } else if cx != x || cy != y {
                    for curve in
                        arc_to_cubic_curves(cx, cy, x, y, r1, r2, angle, large_arc_flag, sweep_flag)
                    {
                        out.push(seg('C', curve.to_vec()));
                    }
                    cx = x;
                    cy = y;
                }
            }
            'Z' => {
                out.push(seg('Z', Vec::new()));
                cx = subx;
                cy = suby;
            }
            _ => {}
        }
        last_type = *key;
    }
    out
}

const PI: f64 = std::f64::consts::PI;

/// JavaScript truthiness of a number.
fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

fn deg_to_rad(degrees: f64) -> f64 {
    (PI * degrees) / 180.0
}

fn rotate(x: f64, y: f64, angle_rad: f64) -> Point {
    let big_x = x * js::cos(angle_rad) - y * js::sin(angle_rad);
    let big_y = x * js::sin(angle_rad) + y * js::cos(angle_rad);
    [big_x, big_y]
}

/// `arcToCubicCurves(...)` without `recursive`: the arc as cubics.
#[allow(clippy::too_many_arguments)]
fn arc_to_cubic_curves(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    r1: f64,
    r2: f64,
    angle: f64,
    large_arc_flag: f64,
    sweep_flag: f64,
) -> Vec<[f64; 6]> {
    let angle_rad = deg_to_rad(angle);
    let params = arc_points(
        x1,
        y1,
        x2,
        y2,
        r1,
        r2,
        angle,
        large_arc_flag,
        sweep_flag,
        None,
    );
    params
        .chunks(3)
        .filter(|c| c.len() == 3)
        .map(|c| {
            let a = rotate(c[0][0], c[0][1], angle_rad);
            let b = rotate(c[1][0], c[1][1], angle_rad);
            let d = rotate(c[2][0], c[2][1], angle_rad);
            [a[0], a[1], b[0], b[1], d[0], d[1]]
        })
        .collect()
}

/// The body of `arcToCubicCurves`: the control and end points of the cubics
/// in the unrotated frame, `[m2, m3, m4].concat(params)`. `recursive` is
/// `[f1, f2, cx, cy]` for the continuation of an arc split at 120 degrees.
#[allow(clippy::too_many_arguments)]
fn arc_points(
    mut x1: f64,
    mut y1: f64,
    mut x2: f64,
    mut y2: f64,
    mut r1: f64,
    mut r2: f64,
    angle: f64,
    large_arc_flag: f64,
    sweep_flag: f64,
    recursive: Option<[f64; 4]>,
) -> Vec<Point> {
    let angle_rad = deg_to_rad(angle);
    let mut params: Vec<Point> = Vec::new();
    let (mut f1, mut f2, cx, cy);
    if let Some([rf1, rf2, rcx, rcy]) = recursive {
        (f1, f2, cx, cy) = (rf1, rf2, rcx, rcy);
    } else {
        [x1, y1] = rotate(x1, y1, -angle_rad);
        [x2, y2] = rotate(x2, y2, -angle_rad);
        let x = (x1 - x2) / 2.0;
        let y = (y1 - y2) / 2.0;
        let mut h = (x * x) / (r1 * r1) + (y * y) / (r2 * r2);
        if h > 1.0 {
            h = h.sqrt();
            r1 *= h;
            r2 *= h;
        }
        // `largeArcFlag === sweepFlag`
        let sign = if large_arc_flag == sweep_flag {
            -1.0
        } else {
            1.0
        };
        let r1_pow = r1 * r1;
        let r2_pow = r2 * r2;
        let left = r1_pow * r2_pow - r1_pow * y * y - r2_pow * x * x;
        let right = r1_pow * y * y + r2_pow * x * x;
        let k = sign * (left / right).abs().sqrt();
        cx = k * r1 * y / r2 + (x1 + x2) / 2.0;
        cy = k * -r2 * x / r1 + (y1 + y2) / 2.0;
        f1 = js::asin(to_fixed_9((y1 - cy) / r2));
        f2 = js::asin(to_fixed_9((y2 - cy) / r2));
        if x1 < cx {
            f1 = PI - f1;
        }
        if x2 < cx {
            f2 = PI - f2;
        }
        // `f = Math.PI * 2 + f` (addition commutes exactly)
        if f1 < 0.0 {
            f1 += PI * 2.0;
        }
        if f2 < 0.0 {
            f2 += PI * 2.0;
        }
        if truthy(sweep_flag) && f1 > f2 {
            f1 -= PI * 2.0;
        }
        if !truthy(sweep_flag) && f2 > f1 {
            f2 -= PI * 2.0;
        }
    }
    let mut df = f2 - f1;
    if df.abs() > (PI * 120.0 / 180.0) {
        let f2old = f2;
        let x2old = x2;
        let y2old = y2;
        if truthy(sweep_flag) && f2 > f1 {
            f2 = f1 + (PI * 120.0 / 180.0);
        } else {
            f2 = f1 - (PI * 120.0 / 180.0);
        }
        x2 = cx + r1 * js::cos(f2);
        y2 = cy + r2 * js::sin(f2);
        params = arc_points(
            x2,
            y2,
            x2old,
            y2old,
            r1,
            r2,
            angle,
            0.0,
            sweep_flag,
            Some([f2, f2old, cx, cy]),
        );
    }
    df = f2 - f1;
    let c1 = js::cos(f1);
    let s1 = js::sin(f1);
    let c2 = js::cos(f2);
    let s2 = js::sin(f2);
    let t = js::tan(df / 4.0);
    let hx = 4.0 / 3.0 * r1 * t;
    let hy = 4.0 / 3.0 * r2 * t;
    let m1 = [x1, y1];
    let mut m2 = [x1 + hx * s1, y1 - hy * c1];
    let m3 = [x2 + hx * s2, y2 - hy * c2];
    let m4 = [x2, y2];
    m2[0] = 2.0 * m1[0] - m2[0];
    m2[1] = 2.0 * m1[1] - m2[1];
    let mut out = vec![m2, m3, m4];
    out.extend(params);
    out
}

/// `parseFloat(x.toFixed(9))`.
///
/// `Number.prototype.toFixed` rounds the exact binary value to the nearest
/// multiple of 10^-9, taking the larger magnitude on a tie (Rust's `{:.9}`
/// takes the even one), and prints the sign of a negative number even when
/// the digits round to zero. At 10^21 and above it prints the number itself.
fn to_fixed_9(x: f64) -> f64 {
    const DIGITS: usize = 9;
    if !x.is_finite() || x.abs() >= 1e21 {
        return x;
    }
    // Every finite double has a finite decimal expansion of at most 1074
    // fractional digits, and `{:.N}` prints it exactly.
    let exact = format!("{:.1100}", x.abs());
    let (int_part, frac) = exact.split_once('.').expect("fixed notation");
    let kept = &frac[..DIGITS];
    let mut n: u128 = format!("{int_part}{kept}").parse().expect("digits");
    if frac.as_bytes()[DIGITS] >= b'5' {
        n += 1;
    }
    let digits = format!("{n:0>width$}", width = DIGITS + 1);
    let (i, f) = digits.split_at(digits.len() - DIGITS);
    let sign = if x < 0.0 { "-" } else { "" };
    format!("{sign}{i}.{f}").parse().expect("decimal")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_fixed_rounds_half_up_on_the_exact_value() {
        // 1/1024 = 0.0009765625 exactly: a tie at nine digits
        assert_eq!(to_fixed_9(0.0009765625), 0.000976563);
        assert_eq!(to_fixed_9(-0.0009765625), -0.000976563);
        // 0.1 is 0.1000000000000000055..., well below a tie
        assert_eq!(to_fixed_9(0.1), 0.1);
        assert_eq!(to_fixed_9(0.9999999996), 1.0);
        assert_eq!(to_fixed_9(0.9999999994), 0.999999999);
        assert_eq!(to_fixed_9(123.4567890123), 123.456789012);
    }

    #[test]
    fn to_fixed_keeps_the_sign_of_small_negatives() {
        let v = to_fixed_9(-1e-12);
        assert_eq!(v, 0.0);
        assert!(v.is_sign_negative());
        // (-0).toFixed(9) is "0.000000000": -0 < 0 is false
        assert!(to_fixed_9(-0.0).is_sign_positive());
    }

    #[test]
    fn to_fixed_passes_large_and_non_finite_values_through() {
        assert_eq!(to_fixed_9(1e21), 1e21);
        assert_eq!(to_fixed_9(f64::INFINITY), f64::INFINITY);
        assert!(to_fixed_9(f64::NAN).is_nan());
        assert_eq!(to_fixed_9(1e20 + 0.5), 1e20);
    }

    #[test]
    fn number_token_grammar() {
        assert_eq!(number_len(b"12.5e3x"), Some(6));
        assert_eq!(number_len(b"-.5.5"), Some(3));
        assert_eq!(number_len(b"1.e"), Some(2));
        assert_eq!(number_len(b"1e+"), Some(1));
        assert_eq!(number_len(b"+."), None);
        assert_eq!(number_len(b"e5"), None);
    }
}
