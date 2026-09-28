//! The CSS `<color>` parser the canvas applies to `fillStyle` and
//! `strokeStyle`.
//!
//! Upstream assigns element colours to the context as stored
//! (`renderElement.ts`: `context.fillStyle = element.strokeColor`; roughjs
//! `ctx.strokeStyle = o.stroke`), so the browser decides what a colour
//! paints. This module reads a colour string as Chrome's canvas does, after
//! CSS Color 4 (<https://www.w3.org/TR/css-color-4/>):
//!
//! - keywords, ASCII case-insensitive: the 148 named colours,
//!   `transparent`, `currentcolor` (black: the canvases upstream draws
//!   elements on are not rendered) and the system colours with
//!   Chrome's light-scheme values;
//! - `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (escapes allowed after `#`);
//! - `rgb()`/`rgba()` and `hsl()`/`hsla()` in the legacy comma syntax and
//!   the modern space syntax with `/ alpha` and `none`; `hwb()`, `lab()`,
//!   `lch()`, `oklab()`, `oklch()` and `color()` with the predefined
//!   spaces `srgb`, `srgb-linear`, `display-p3`, `a98-rgb`,
//!   `prophoto-rgb`, `rec2020`, `xyz`, `xyz-d50` and `xyz-d65`, converted to
//!   sRGB and clipped to its gamut, as Chrome paints them on an sRGB canvas;
//! - `calc()` (and bare parentheses inside it) over numbers, percentages
//!   and angles with `+ - * /` and the constants `e`, `pi`, `infinity`,
//!   `-infinity` and `NaN`, and `min()`, `max()` and `clamp()`, in any
//!   component.
//!
//! What Chrome's canvas also accepts and this parser does not: the CSS
//! Color 5 forms `color-mix()` and relative colours (`rgb(from red r g b)`),
//! and the math functions other than `calc()`, `min()`, `max()` and
//! `clamp()` (`round()`, `mod()`, `rem()`, `abs()`, `sign()`,
//! trigonometry, exponentials). They read as not a colour, which leaves the
//! context's current style (see `display/mod.rs`). Chrome's own quirks are
//! kept where they differ from the specification: an identifier with an
//! escape (`r\65 d`) and a comment outside a function are rejected.
//!
//! `crates/excali-scene/tests/fixtures/css-colors.json`, written by
//! `scripts/fixtures/css-color-goldens.sh` from headless Chrome, pins every
//! case.

use super::paint::Rgba;

/// Parse `input` as the canvas parses a `fillStyle` assignment, or `None`
/// when the canvas ignores it.
pub(crate) fn parse(input: &str) -> Option<Rgba> {
    let s = input.trim_matches(is_css_space);
    if let Some(hash) = s.strip_prefix('#') {
        return parse_hash(hash);
    }
    let name_len = s
        .bytes()
        .position(|b| !(b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
        .unwrap_or(s.len());
    let (name, rest) = s.split_at(name_len);
    if name.is_empty() {
        return None;
    }
    let name = name.to_ascii_lowercase();
    if rest.is_empty() {
        return keyword(&name);
    }
    let body = rest.strip_prefix('(')?;
    let mut lexer = Lexer::new(body);
    let args = lexer.function_body()?;
    // Only whitespace may follow the closing parenthesis.
    if !lexer.at_end() {
        return None;
    }
    color_function(&name, &args)
}

fn is_css_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0c')
}

// ---------------------------------------------------------------------------
// Tokens

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Percentage(f64),
    /// A number with a unit, the unit lower-cased.
    Dimension(f64, String),
    /// An identifier, lower-cased.
    Ident(String),
    Comma,
    Slash,
    Delim(char),
    /// A function and its arguments, the name lower-cased.
    Function(String, Vec<Token>),
    /// A parenthesised block.
    Block(Vec<Token>),
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            s: s.as_bytes(),
            i: 0,
        }
    }

    fn peek(&self, k: usize) -> Option<u8> {
        self.s.get(self.i + k).copied()
    }

    /// Skip whitespace and comments (an unterminated comment runs to the end).
    fn skip_space(&mut self) {
        loop {
            match self.peek(0) {
                Some(b' ' | b'\t' | b'\n' | b'\r' | b'\x0c') => self.i += 1,
                Some(b'/') if self.peek(1) == Some(b'*') => {
                    self.i += 2;
                    while self.i < self.s.len()
                        && !(self.peek(0) == Some(b'*') && self.peek(1) == Some(b'/'))
                    {
                        self.i += 1;
                    }
                    self.i = (self.i + 2).min(self.s.len());
                }
                _ => return,
            }
        }
    }

    fn at_end(&mut self) -> bool {
        self.skip_space();
        self.i >= self.s.len()
    }

    /// The tokens up to the matching `)`, or to the end of the input, which
    /// closes every open block as CSS does.
    fn function_body(&mut self) -> Option<Vec<Token>> {
        let mut out = Vec::new();
        loop {
            self.skip_space();
            let Some(c) = self.peek(0) else {
                return Some(out);
            };
            match c {
                b')' => {
                    self.i += 1;
                    return Some(out);
                }
                b'(' => {
                    self.i += 1;
                    out.push(Token::Block(self.function_body()?));
                }
                b',' => {
                    self.i += 1;
                    out.push(Token::Comma);
                }
                b'/' => {
                    self.i += 1;
                    out.push(Token::Slash);
                }
                _ if self.starts_number() => out.push(self.numeric()?),
                _ if self.starts_ident() => {
                    let name = self.ident();
                    if self.peek(0) == Some(b'(') {
                        self.i += 1;
                        out.push(Token::Function(name, self.function_body()?));
                    } else {
                        out.push(Token::Ident(name));
                    }
                }
                b'+' | b'-' | b'*' => {
                    self.i += 1;
                    out.push(Token::Delim(c as char));
                }
                // Escapes, strings, other delimiters: nothing a colour
                // function accepts.
                _ => return None,
            }
        }
    }

    fn starts_number(&self) -> bool {
        let digit = |k| self.peek(k).is_some_and(|b: u8| b.is_ascii_digit());
        match self.peek(0) {
            Some(b'+' | b'-') => digit(1) || (self.peek(1) == Some(b'.') && digit(2)),
            Some(b'.') => digit(1),
            Some(b) => b.is_ascii_digit(),
            None => false,
        }
    }

    fn starts_ident(&self) -> bool {
        let start = |b: Option<u8>| b.is_some_and(|b| b.is_ascii_alphabetic() || b == b'_');
        match self.peek(0) {
            Some(b'-') => start(self.peek(1)) || self.peek(1) == Some(b'-'),
            b => start(b),
        }
    }

    fn ident(&mut self) -> String {
        let begin = self.i;
        while self
            .peek(0)
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.s[begin..self.i]).to_ascii_lowercase()
    }

    /// A number, percentage or dimension token (CSS Syntax 3 §4.3.12).
    fn numeric(&mut self) -> Option<Token> {
        let begin = self.i;
        if matches!(self.peek(0), Some(b'+' | b'-')) {
            self.i += 1;
        }
        let digits = |lx: &mut Self| {
            while lx.peek(0).is_some_and(|b| b.is_ascii_digit()) {
                lx.i += 1;
            }
        };
        digits(self);
        if self.peek(0) == Some(b'.') && self.peek(1).is_some_and(|b| b.is_ascii_digit()) {
            self.i += 1;
            digits(self);
        }
        if matches!(self.peek(0), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(self.peek(1), Some(b'+' | b'-')));
            if self.peek(1 + sign).is_some_and(|b| b.is_ascii_digit()) {
                self.i += 1 + sign;
                digits(self);
            }
        }
        let text = std::str::from_utf8(&self.s[begin..self.i]).ok()?;
        let value: f64 = text.parse().ok()?;
        if self.peek(0) == Some(b'%') {
            self.i += 1;
            Some(Token::Percentage(value))
        } else if self.starts_ident() {
            Some(Token::Dimension(value, self.ident()))
        } else {
            Some(Token::Number(value))
        }
    }
}

// ---------------------------------------------------------------------------
// Values

/// A component value after `calc()`: angles in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Value {
    Number(f64),
    Percentage(f64),
    Angle(f64),
    None,
}

fn angle_degrees(value: f64, unit: &str) -> Option<f64> {
    Some(match unit {
        "deg" => value,
        "grad" => value * 0.9,
        "rad" => value.to_degrees(),
        "turn" => value * 360.0,
        _ => return None,
    })
}

/// One component token as a value.
fn value(token: &Token) -> Option<Value> {
    match token {
        Token::Number(n) => Some(Value::Number(*n)),
        Token::Percentage(p) => Some(Value::Percentage(*p)),
        Token::Dimension(v, unit) => angle_degrees(*v, unit).map(Value::Angle),
        Token::Ident(name) if name == "none" => Some(Value::None),
        Token::Function(name, args) => math_function(name, args),
        _ => None,
    }
}

/// `calc()`, `min()`, `max()` or `clamp()`.
fn math_function(name: &str, args: &[Token]) -> Option<Value> {
    if name == "calc" {
        return calc(args);
    }
    let values: Vec<Value> = args
        .split(|t| *t == Token::Comma)
        .map(calc)
        .collect::<Option<_>>()?;
    let first = *values.first()?;
    let same = |v: &Value| std::mem::discriminant(v) == std::mem::discriminant(&first);
    if !values.iter().all(same) {
        return None;
    }
    let raw = |v: Value| match v {
        Value::Number(x) | Value::Percentage(x) | Value::Angle(x) => x,
        Value::None => 0.0,
    };
    let xs: Vec<f64> = values.into_iter().map(raw).collect();
    let result = match (name, xs.as_slice()) {
        ("min", _) => xs.iter().copied().fold(f64::INFINITY, f64::min),
        ("max", _) => xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        // clamp(MIN, VAL, MAX) = max(MIN, min(VAL, MAX)).
        ("clamp", [lo, v, hi]) => lo.max(v.min(*hi)),
        _ => return None,
    };
    Some(match first {
        Value::Percentage(_) => Value::Percentage(result),
        Value::Angle(_) => Value::Angle(result),
        _ => Value::Number(result),
    })
}

/// `calc(args)`: a sum of products (CSS Values 4 §10).
fn calc(args: &[Token]) -> Option<Value> {
    let mut i = 0;
    let v = calc_sum(args, &mut i)?;
    (i == args.len()).then_some(v)
}

fn calc_sum(t: &[Token], i: &mut usize) -> Option<Value> {
    let mut acc = calc_product(t, i)?;
    while let Some(Token::Delim(op @ ('+' | '-'))) = t.get(*i) {
        *i += 1;
        let rhs = calc_product(t, i)?;
        let sign = if *op == '+' { 1.0 } else { -1.0 };
        acc = match (acc, rhs) {
            (Value::Number(a), Value::Number(b)) => Value::Number(a + sign * b),
            (Value::Percentage(a), Value::Percentage(b)) => Value::Percentage(a + sign * b),
            (Value::Angle(a), Value::Angle(b)) => Value::Angle(a + sign * b),
            _ => return None,
        };
    }
    Some(acc)
}

fn calc_product(t: &[Token], i: &mut usize) -> Option<Value> {
    let mut acc = calc_term(t, i)?;
    loop {
        let op = match t.get(*i) {
            Some(Token::Delim('*')) => '*',
            Some(Token::Slash) => '/',
            _ => return Some(acc),
        };
        *i += 1;
        let rhs = calc_term(t, i)?;
        acc = match (op, acc, rhs) {
            ('*', Value::Number(a), b) => scale(b, a),
            ('*', a, Value::Number(b)) => scale(a, b),
            ('/', a, Value::Number(b)) => scale(a, 1.0 / b),
            _ => return None,
        };
    }
}

fn scale(v: Value, k: f64) -> Value {
    match v {
        Value::Number(x) => Value::Number(x * k),
        Value::Percentage(x) => Value::Percentage(x * k),
        Value::Angle(x) => Value::Angle(x * k),
        Value::None => Value::None,
    }
}

fn calc_term(t: &[Token], i: &mut usize) -> Option<Value> {
    let token = t.get(*i)?;
    *i += 1;
    match token {
        Token::Block(inner) => calc(inner),
        Token::Ident(name) => match name.as_str() {
            "e" => Some(Value::Number(std::f64::consts::E)),
            "pi" => Some(Value::Number(std::f64::consts::PI)),
            "infinity" => Some(Value::Number(f64::INFINITY)),
            "-infinity" => Some(Value::Number(f64::NEG_INFINITY)),
            "nan" => Some(Value::Number(f64::NAN)),
            _ => None,
        },
        Token::Number(_) | Token::Percentage(_) | Token::Dimension(..) => value(token),
        Token::Function(name, args) => math_function(name, args),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Argument lists

/// The arguments of a colour function: three components and an optional
/// alpha, from the legacy comma list or the modern space list.
struct Args {
    c: [Value; 3],
    alpha: Option<Value>,
    legacy: bool,
}

fn split_args(tokens: &[Token]) -> Option<Args> {
    if tokens.contains(&Token::Comma) {
        // a, b, c[, alpha]: exactly one value between commas.
        let parts: Vec<&[Token]> = tokens.split(|t| *t == Token::Comma).collect();
        if !(parts.len() == 3 || parts.len() == 4) || parts.iter().any(|p| p.len() != 1) {
            return None;
        }
        let v: Vec<Value> = parts.iter().map(|p| value(&p[0])).collect::<Option<_>>()?;
        if v.contains(&Value::None) {
            return None;
        }
        return Some(Args {
            c: [v[0], v[1], v[2]],
            alpha: v.get(3).copied(),
            legacy: true,
        });
    }
    let (components, alpha) = match tokens.iter().position(|t| *t == Token::Slash) {
        Some(at) => {
            let alpha = &tokens[at + 1..];
            if alpha.len() != 1 {
                return None;
            }
            (&tokens[..at], Some(value(&alpha[0])?))
        }
        None => (tokens, None),
    };
    if components.len() != 3 {
        return None;
    }
    Some(Args {
        c: [
            value(&components[0])?,
            value(&components[1])?,
            value(&components[2])?,
        ],
        alpha,
        legacy: false,
    })
}

/// `<alpha-value> | none`, clamped to `0..=1`.
fn alpha(v: Option<Value>) -> Option<f64> {
    let a = match v {
        None => 1.0,
        Some(Value::Number(n)) => n,
        Some(Value::Percentage(p)) => p / 100.0,
        Some(Value::None) => 0.0,
        Some(Value::Angle(_)) => return None,
    };
    Some(clamp01(a))
}

fn clamp01(x: f64) -> f64 {
    if x.is_nan() {
        0.0
    } else {
        x.clamp(0.0, 1.0)
    }
}

/// `<number> | <percentage> | none` with `100%` equal to `full`.
fn scalar(v: Value, full: f64) -> Option<f64> {
    match v {
        Value::Number(n) => Some(n),
        Value::Percentage(p) => Some(p / 100.0 * full),
        Value::None => Some(0.0),
        Value::Angle(_) => None,
    }
}

/// `<hue> | none` in degrees.
fn hue(v: Value) -> Option<f64> {
    match v {
        Value::Number(n) | Value::Angle(n) => Some(n),
        Value::None => Some(0.0),
        Value::Percentage(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Colour functions

fn color_function(name: &str, tokens: &[Token]) -> Option<Rgba> {
    if name == "color" {
        return color_space_function(tokens);
    }
    let args = split_args(tokens)?;
    let a = alpha(args.alpha)?;
    let [x, y, z] = args.c;
    match name {
        "rgb" | "rgba" => {
            if args.legacy {
                let all_numbers = args.c.iter().all(|v| matches!(v, Value::Number(_)));
                let all_percentages = args.c.iter().all(|v| matches!(v, Value::Percentage(_)));
                if !(all_numbers || all_percentages) {
                    return None;
                }
            }
            let channel = |v| scalar(v, 255.0).map(|n| byte(n / 255.0));
            Some(Rgba {
                r: channel(x)?,
                g: channel(y)?,
                b: channel(z)?,
                a,
            })
        }
        "hsl" | "hsla" => {
            if args.legacy
                && !(matches!(y, Value::Percentage(_)) && matches!(z, Value::Percentage(_)))
            {
                return None;
            }
            let h = hue(x)?;
            let s = clamp01(scalar(y, 100.0)? / 100.0);
            let l = clamp01(scalar(z, 100.0)? / 100.0);
            let [r, g, b] = hsl_to_rgb(h, s, l);
            Some(srgb_bytes([r, g, b], a))
        }
        "hwb" if !args.legacy => {
            let h = hue(x)?;
            let w = clamp01(scalar(y, 100.0)? / 100.0);
            let bl = clamp01(scalar(z, 100.0)? / 100.0);
            Some(srgb_bytes(hwb_to_rgb(h, w, bl), a))
        }
        "lab" if !args.legacy => {
            let l = scalar(x, 100.0)?.clamp(0.0, 100.0);
            let aa = scalar(y, 125.0)?;
            let bb = scalar(z, 125.0)?;
            Some(from_xyz_d50(lab_to_xyz_d50(l, aa, bb), a))
        }
        "lch" if !args.legacy => {
            let l = scalar(x, 100.0)?.clamp(0.0, 100.0);
            let c = scalar(y, 150.0)?.max(0.0);
            let h = hue(z)?.to_radians();
            Some(from_xyz_d50(lab_to_xyz_d50(l, c * h.cos(), c * h.sin()), a))
        }
        "oklab" if !args.legacy => {
            let l = scalar(x, 1.0)?.clamp(0.0, 1.0);
            Some(from_linear_srgb(
                oklab_to_linear_srgb(l, scalar(y, 0.4)?, scalar(z, 0.4)?),
                a,
            ))
        }
        "oklch" if !args.legacy => {
            let l = scalar(x, 1.0)?.clamp(0.0, 1.0);
            let c = scalar(y, 0.4)?.max(0.0);
            let h = hue(z)?.to_radians();
            Some(from_linear_srgb(
                oklab_to_linear_srgb(l, c * h.cos(), c * h.sin()),
                a,
            ))
        }
        _ => None,
    }
}

/// `color(<space> c1 c2 c3 [/ alpha])`.
fn color_space_function(tokens: &[Token]) -> Option<Rgba> {
    let (Token::Ident(space), rest) = tokens.split_first()? else {
        return None;
    };
    let args = split_args(rest)?;
    if args.legacy {
        return None;
    }
    let a = alpha(args.alpha)?;
    let mut c = [0.0; 3];
    for (out, v) in c.iter_mut().zip(args.c) {
        *out = scalar(v, 1.0)?;
    }
    let rgb_to_xyz = |m: &[[f64; 3]; 3], decode: fn(f64) -> f64| mul(m, c.map(decode));
    Some(match space.as_str() {
        "srgb" => srgb_bytes(c, a),
        "srgb-linear" => from_linear_srgb(c, a),
        "display-p3" => from_xyz_d65(rgb_to_xyz(&P3_TO_XYZ, srgb_decode), a),
        "a98-rgb" => from_xyz_d65(rgb_to_xyz(&A98_TO_XYZ, a98_decode), a),
        "prophoto-rgb" => from_xyz_d50(rgb_to_xyz(&PROPHOTO_TO_XYZ_D50, prophoto_decode), a),
        "rec2020" => from_xyz_d65(rgb_to_xyz(&REC2020_TO_XYZ, rec2020_decode), a),
        "xyz" | "xyz-d65" => from_xyz_d65(c, a),
        "xyz-d50" => from_xyz_d50(c, a),
        _ => return None,
    })
}

/// `hslToRgb` (CSS Color 4 §7.1), each channel in `0..=1`.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    let h = h.rem_euclid(360.0);
    let h = if h.is_finite() { h } else { 0.0 };
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let a = s * l.min(1.0 - l);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [f(0.0), f(8.0), f(4.0)]
}

/// `hwbToRgb` (CSS Color 4 §8.1).
fn hwb_to_rgb(h: f64, w: f64, b: f64) -> [f64; 3] {
    if w + b >= 1.0 {
        let gray = w / (w + b);
        return [gray; 3];
    }
    hsl_to_rgb(h, 1.0, 0.5).map(|c| c * (1.0 - w - b) + w)
}

/// CIE Lab (D50) to XYZ (D50) (CSS Color 4 §18, `Lab_to_XYZ`).
fn lab_to_xyz_d50(l: f64, a: f64, b: f64) -> [f64; 3] {
    const KAPPA: f64 = 24389.0 / 27.0;
    const EPSILON: f64 = 216.0 / 24389.0;
    let f1 = (l + 16.0) / 116.0;
    let f0 = a / 500.0 + f1;
    let f2 = f1 - b / 200.0;
    let x = if f0.powi(3) > EPSILON {
        f0.powi(3)
    } else {
        (116.0 * f0 - 16.0) / KAPPA
    };
    let y = if l > KAPPA * EPSILON {
        ((l + 16.0) / 116.0).powi(3)
    } else {
        l / KAPPA
    };
    let z = if f2.powi(3) > EPSILON {
        f2.powi(3)
    } else {
        (116.0 * f2 - 16.0) / KAPPA
    };
    [x * D50[0], y * D50[1], z * D50[2]]
}

/// OKLab to linear-light sRGB (CSS Color 4 §18, `OKLab_to_XYZ` composed
/// with `XYZ_to_lin_sRGB`, Ottosson's direct matrices).
fn oklab_to_linear_srgb(l: f64, a: f64, b: f64) -> [f64; 3] {
    let l_ = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m_ = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s_ = (l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    [
        4.076_741_662_1 * l_ - 3.307_711_591_3 * m_ + 0.230_969_929_2 * s_,
        -1.268_438_004_6 * l_ + 2.609_757_401_1 * m_ - 0.341_319_396_5 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_614_7 * m_ + 1.707_614_701 * s_,
    ]
}

/// The D50 white point, `[0.3457 / 0.3585, 1, (1 - 0.3457 - 0.3585) / 0.3585]`.
const D50: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];

/// Bradford chromatic adaptation from D50 to D65 (CSS Color 4 §18, `D50_to_D65`).
const D50_TO_D65: [[f64; 3]; 3] = [
    [0.955473421488075, -0.02309845494876471, 0.06325924320057072],
    [
        -0.0283697093338637,
        1.0099953980813041,
        0.021041441191917323,
    ],
    [
        0.012314014864481998,
        -0.020507649298898964,
        1.330365926242124,
    ],
];

/// XYZ (D65) to linear-light sRGB (`XYZ_to_lin_sRGB`).
const XYZ_TO_SRGB: [[f64; 3]; 3] = [
    [3.2409699419045226, -1.537383177570094, -0.4986107602930034],
    [-0.9692436362808796, 1.8759675015077202, 0.04155505740717559],
    [
        0.05563007969699366,
        -0.20397695888897652,
        1.0569715142428786,
    ],
];

/// Linear-light Display P3 to XYZ (D65) (`lin_P3_to_XYZ`).
const P3_TO_XYZ: [[f64; 3]; 3] = [
    [0.4865709486482162, 0.26566769316909306, 0.1982172852343625],
    [0.2289745640697488, 0.6917385218365064, 0.079286914093745],
    [0.0, 0.04511338185890264, 1.043944368900976],
];

/// Linear-light A98 RGB to XYZ (D65) (`lin_a98rgb_to_XYZ`).
const A98_TO_XYZ: [[f64; 3]; 3] = [
    [0.5766690429101305, 0.1855582379065463, 0.1882286462349947],
    [0.29734497525053605, 0.6273635662554661, 0.07529145849399788],
    [0.02703136138641234, 0.07068885253582723, 0.9913375368376388],
];

/// Linear-light ProPhoto RGB to XYZ (D50) (`lin_ProPhoto_to_XYZ`).
const PROPHOTO_TO_XYZ_D50: [[f64; 3]; 3] = [
    [0.7977604896723027, 0.13518583717574031, 0.0313493495815248],
    [
        0.2880711282292934,
        0.7118432178101014,
        0.00008565396060525902,
    ],
    [0.0, 0.0, 0.8251046025104601],
];

/// Linear-light Rec. 2020 to XYZ (D65) (`lin_2020_to_XYZ`).
const REC2020_TO_XYZ: [[f64; 3]; 3] = [
    [0.6369580483012914, 0.14461690358620832, 0.1688809751641721],
    [0.2627002120112671, 0.6779980715188708, 0.05930171646986196],
    [0.0, 0.028072693049087428, 1.060985057710791],
];

fn mul(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

/// The sRGB transfer function's inverse (`lin_sRGB`), sign-preserving.
fn srgb_decode(v: f64) -> f64 {
    let x = v.abs();
    let lin = if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    };
    lin.copysign(v)
}

/// The sRGB transfer function (`gam_sRGB`), sign-preserving.
fn srgb_encode(v: f64) -> f64 {
    let x = v.abs();
    let gam = if x > 0.0031308 {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * x
    };
    gam.copysign(v)
}

fn a98_decode(v: f64) -> f64 {
    v.abs().powf(563.0 / 256.0).copysign(v)
}

/// ProPhoto's transfer as Chrome applies it: a pure 1.8 power. CSS
/// Color 4's `lin_ProPhoto` has a linear segment below 16/512, which Chrome
/// does not (`color(prophoto-rgb 0.01 0.02 0.03)` paints `[0, 3, 6]`, the
/// segment would give `[0, 5, 7]`; `tests/fixtures/css-colors.json`).
fn prophoto_decode(v: f64) -> f64 {
    v.abs().powf(1.8).copysign(v)
}

fn rec2020_decode(v: f64) -> f64 {
    const ALPHA: f64 = 1.09929682680944;
    const BETA: f64 = 0.018053968510807;
    let x = v.abs();
    let lin = if x < BETA * 4.5 {
        x / 4.5
    } else {
        ((x + ALPHA - 1.0) / ALPHA).powf(1.0 / 0.45)
    };
    lin.copysign(v)
}

fn from_xyz_d50(xyz: [f64; 3], a: f64) -> Rgba {
    from_xyz_d65(mul(&D50_TO_D65, xyz), a)
}

fn from_xyz_d65(xyz: [f64; 3], a: f64) -> Rgba {
    from_linear_srgb(mul(&XYZ_TO_SRGB, xyz), a)
}

fn from_linear_srgb(rgb: [f64; 3], a: f64) -> Rgba {
    srgb_bytes(rgb.map(srgb_encode), a)
}

/// Gamma-encoded sRGB in `0..=1` (clipped) to bytes.
fn srgb_bytes([r, g, b]: [f64; 3], a: f64) -> Rgba {
    Rgba {
        r: byte(r),
        g: byte(g),
        b: byte(b),
        a,
    }
}

/// `x` in `0..=1` (clipped, NaN as 0) to `0..=255`, halves rounding up.
fn byte(x: f64) -> u8 {
    (clamp01(x) * 255.0 + 0.5).floor() as u8
}

// ---------------------------------------------------------------------------
// Hex and keywords

/// The text after `#`: 3, 4, 6 or 8 hex digits. Escapes are a hash token's
/// code points, so `#\66 00` is `#f00`.
fn parse_hash(s: &str) -> Option<Rgba> {
    let mut digits = Vec::with_capacity(8);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let c = if c == '\\' {
            let mut code = String::new();
            while code.len() < 6 && chars.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                code.push(chars.next()?);
            }
            if code.is_empty() {
                chars.next()?
            } else {
                if chars.peek().copied().is_some_and(is_css_space) {
                    chars.next();
                }
                char::from_u32(u32::from_str_radix(&code, 16).ok()?)?
            }
        } else {
            c
        };
        digits.push(c.to_digit(16)? as u8);
    }
    let pair = |i: usize| digits[i] * 16 + digits[i + 1];
    let (r, g, b, a) = match digits.len() {
        3 | 4 => (
            digits[0] * 17,
            digits[1] * 17,
            digits[2] * 17,
            digits.get(3).map_or(255, |d| d * 17),
        ),
        6 | 8 => (
            pair(0),
            pair(2),
            pair(4),
            if digits.len() == 8 { pair(6) } else { 255 },
        ),
        _ => return None,
    };
    Some(Rgba {
        r,
        g,
        b,
        a: f64::from(a) / 255.0,
    })
}

fn keyword(name: &str) -> Option<Rgba> {
    let opaque = |hex: u32| Rgba {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
        a: 1.0,
    };
    match name {
        "transparent" => {
            return Some(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0.0,
            })
        }
        // The canvas's `currentcolor` is the canvas element's `color`, or
        // black when the element is not rendered. Upstream draws elements
        // on canvases from `document.createElement("canvas")`
        // (renderElement.ts:278) and on export canvases, none of them in the
        // document. Not modelled: RTL text, whose element canvas upstream
        // appends to `document.body` (renderElement.ts:632), where it would
        // take the page's `color`.
        "currentcolor" => return Some(opaque(0x000000)),
        // Chrome's Highlight: macOS's default selection colour at 60%.
        "highlight" => {
            return Some(Rgba {
                r: 128,
                g: 188,
                b: 254,
                a: 0.6,
            })
        }
        _ => {}
    }
    let table = NAMED.iter().chain(SYSTEM.iter());
    table
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, hex)| opaque(*hex))
}

/// CSS Color 4 §6.1 named colours.
const NAMED: [(&str, u32); 148] = [
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

/// CSS Color 4 §6.2 system colours and the deprecated ones of Appendix A,
/// with Chrome's light-scheme values (`highlight` is in [`keyword`]).
/// Chrome does not accept `AccentColor` or `AccentColorText` on a canvas.
const SYSTEM: [(&str, u32); 39] = [
    ("activetext", 0xff0000),
    ("buttonborder", 0x000000),
    ("buttonface", 0xefefef),
    ("buttontext", 0x000000),
    ("canvas", 0xffffff),
    ("canvastext", 0x000000),
    ("field", 0xffffff),
    ("fieldtext", 0x000000),
    ("graytext", 0x808080),
    ("highlighttext", 0x000000),
    ("linktext", 0x0000ee),
    ("mark", 0xffff00),
    ("marktext", 0x000000),
    ("selecteditem", 0xb3d7ff),
    ("selecteditemtext", 0x000000),
    ("visitedtext", 0x551a8b),
    ("activeborder", 0x000000),
    ("activecaption", 0xffffff),
    ("appworkspace", 0xffffff),
    ("background", 0xffffff),
    ("buttonhighlight", 0xefefef),
    ("buttonshadow", 0xefefef),
    ("captiontext", 0x000000),
    ("inactiveborder", 0x000000),
    ("inactivecaption", 0xffffff),
    ("inactivecaptiontext", 0x808080),
    ("infobackground", 0xffffff),
    ("infotext", 0x000000),
    ("menu", 0xffffff),
    ("menutext", 0x000000),
    ("scrollbar", 0xffffff),
    ("threeddarkshadow", 0x000000),
    ("threedface", 0xefefef),
    ("threedhighlight", 0x000000),
    ("threedlightshadow", 0x000000),
    ("threedshadow", 0x000000),
    ("window", 0xffffff),
    ("windowframe", 0x000000),
    ("windowtext", 0x000000),
];
