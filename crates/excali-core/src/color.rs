//! Colour parsing: the part of tinycolor2 1.6.0 (MIT, Brian Grinstead)
//! that upstream's `colorToHex` and `isTransparent`
//! (`packages/common/src/colors.ts:380-391`) use.
//!
//! Upstream pins `tinycolor2@1.6.0` (`packages/common/package.json`,
//! `yarn.lock`). This is a port of its `tinycolor(color)` constructor,
//! `inputToRGB`, `stringInputToObject` and the conversions they call
//! (`tinycolor.js:26-48`, `313-512`, `785-1166`), with tinycolor's own
//! permissive rules: surrounding whitespace and case are ignored, the
//! functional notations need neither parentheses nor commas and are found
//! anywhere in the string (`"xrgb(1,2,3)"` is `rgb(1,2,3)`), hex may omit
//! `#`, and an alpha outside 0..=1 is 1.
//!
//! `tests/color.rs` checks it against upstream's output for every notation
//! (`tools/goldens/app-state.mjs`).

use excali_math::js;
use serde_json::{Map, Value};

use crate::js::{
    is_whitespace_char, number_to_string, parse_float, parse_float_value, parse_int_of_number,
    string_to_number, to_string, truthy, TypeError,
};

/// A parsed colour: tinycolor's `_r`, `_g`, `_b`, `_a` and `_ok`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TinyColor {
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    ok: bool,
}

/// A component as `stringInputToObject` gives it: a regex capture (a
/// string, possibly with `%`) or a number decoded from hex.
#[derive(Debug, Clone, PartialEq)]
enum Unit {
    Str(String),
    Num(f64),
}

impl Unit {
    /// `parseFloat(n)`.
    fn parse_float(&self) -> f64 {
        match self {
            Unit::Str(s) => parse_float(s),
            Unit::Num(x) => *x,
        }
    }

    /// `n` as a relational comparison or `*` converts it: `Number(n)`.
    fn to_number(&self) -> f64 {
        match self {
            Unit::Str(s) => string_to_number(s),
            Unit::Num(x) => *x,
        }
    }
}

/// The object `stringInputToObject` returns.
#[derive(Debug, Clone, PartialEq)]
enum Parsed {
    Rgb {
        r: Unit,
        g: Unit,
        b: Unit,
        a: Option<Unit>,
    },
    Hsl {
        h: Unit,
        s: Unit,
        l: Unit,
        a: Option<Unit>,
    },
    Hsv {
        h: Unit,
        s: Unit,
        v: Unit,
        a: Option<Unit>,
    },
}

impl TinyColor {
    /// `tinycolor(color)` for a string.
    pub fn parse(color: &str) -> TinyColor {
        let Some(parsed) = string_input_to_object(color) else {
            return TinyColor::from_rgb((0.0, 0.0, 0.0), 1.0, false);
        };
        let (rgb, a) = match parsed {
            Parsed::Rgb { r, g, b, a } => (rgb_to_rgb(&r, &g, &b), a),
            Parsed::Hsv { h, s, v, a } => (
                hsv_to_rgb(&h, &convert_to_percentage(s), &convert_to_percentage(v)),
                a,
            ),
            Parsed::Hsl { h, s, l, a } => (
                hsl_to_rgb(&h, &convert_to_percentage(s), &convert_to_percentage(l)),
                a,
            ),
        };
        let a = a.map_or(1.0, |a| bound_alpha(a.parse_float()));
        TinyColor::from_rgb(rgb, a, true)
    }

    /// The clamping at the end of `inputToRGB` (`tinycolor.js:363-369`) and
    /// the constructor's rounding of components below 1
    /// (`tinycolor.js:41-47`).
    fn from_rgb((r, g, b): (f64, f64, f64), a: f64, ok: bool) -> TinyColor {
        let clamp = |x: f64| {
            let x = js::min(255.0, js::max(x, 0.0));
            if x < 1.0 {
                js::round(x)
            } else {
                x
            }
        };
        TinyColor {
            r: clamp(r),
            g: clamp(g),
            b: clamp(b),
            a,
            ok,
        }
    }

    /// `isValid()`: the input was a colour tinycolor recognises.
    pub fn is_valid(&self) -> bool {
        self.ok
    }

    /// `getAlpha()`: 1 for an input that is not a colour.
    pub fn alpha(&self) -> f64 {
        self.a
    }

    /// `toRgb()`: the components rounded with `Math.round`, and the alpha.
    pub fn to_rgb(&self) -> (f64, f64, f64, f64) {
        (
            js::round(self.r),
            js::round(self.g),
            js::round(self.b),
            self.a,
        )
    }
}

/// `colorToHex(color)` (`colors.ts:380-387`): `#rrggbb`, or `#rrggbbaa`
/// when the alpha is below 1; `None` when `color` is not a colour.
pub fn color_to_hex(color: &str) -> Option<String> {
    let tc = TinyColor::parse(color);
    if !tc.is_valid() {
        return None;
    }
    let (r, g, b, a) = tc.to_rgb();
    Some(rgb_to_hex(r, g, b, a))
}

/// `isTransparent(color)` (`colors.ts:389-391`): the alpha is 0.
pub fn is_transparent(color: &str) -> bool {
    TinyColor::parse(color).alpha() == 0.0
}

/// `DARK_MODE_FILTER_INVERT_PERCENT`, `colors.ts:16`.
const DARK_MODE_FILTER_INVERT_PERCENT: f64 = 93.0;
/// `DARK_MODE_FILTER_HUE_ROTATE_DEGREES`, `colors.ts:17`.
const DARK_MODE_FILTER_HUE_ROTATE_DEGREES: f64 = 180.0;

/// `applyDarkModeFilter(color, enable)` (`colors.ts:86-125`): the colour
/// the canvas shows through `DARK_THEME_FILTER`, CSS
/// `invert(93%) hue-rotate(180deg)`, computed numerically as `#rrggbb` (or
/// `#rrggbbaa` below alpha 1; the alpha is kept). A string tinycolor does
/// not recognise reads as opaque black. With `enable` false the colour is
/// returned as given. Upstream's browser-only memo cache
/// (`DARK_MODE_COLORS_CACHE`) does not change the result and is not ported.
pub fn apply_dark_mode_filter(color: &str, enable: bool) -> String {
    if !enable {
        return color.to_owned();
    }
    let tc = TinyColor::parse(color);
    let alpha = tc.alpha();
    // Order matters: invert, then hue-rotate, as the CSS filter list.
    let (r, g, b, _) = tc.to_rgb();
    let inverted = css_invert(r, g, b, DARK_MODE_FILTER_INVERT_PERCENT);
    let (r, g, b) = css_hue_rotate(inverted, DARK_MODE_FILTER_HUE_ROTATE_DEGREES);
    rgb_to_hex(r, g, b, alpha)
}

/// `cssInvert(r, g, b, percent)` (`colors.ts:62-84`).
fn css_invert(r: f64, g: f64, b: f64, percent: f64) -> (f64, f64, f64) {
    let p = excali_math::clamp(percent, 0.0, 100.0) / 100.0;
    let invert = |c: f64| {
        let inverted = c * (1.0 - p) + (255.0 - c) * p;
        js::round(excali_math::clamp(inverted, 0.0, 255.0))
    };
    (invert(r), invert(g), invert(b))
}

/// `cssHueRotate(red, green, blue, degrees)` (`colors.ts:19-60`): the
/// `hue-rotate()` matrix of the Filter Effects spec.
fn css_hue_rotate((red, green, blue): (f64, f64, f64), degrees: f64) -> (f64, f64, f64) {
    let (r, g, b) = (red / 255.0, green / 255.0, blue / 255.0);
    let a = excali_math::degrees_to_radians(excali_math::Degrees(degrees)).0;
    let (c, s) = (a.cos(), a.sin());
    let m = [
        0.213 + c * 0.787 - s * 0.213,
        0.715 - c * 0.715 - s * 0.715,
        0.072 - c * 0.072 + s * 0.928,
        0.213 - c * 0.213 + s * 0.143,
        0.715 + c * 0.285 + s * 0.14,
        0.072 - c * 0.072 - s * 0.283,
        0.213 - c * 0.213 - s * 0.787,
        0.715 - c * 0.715 + s * 0.715,
        0.072 + c * 0.928 + s * 0.072,
    ];
    let new_r = r * m[0] + g * m[1] + b * m[2];
    let new_g = r * m[3] + g * m[4] + b * m[5];
    let new_b = r * m[6] + g * m[7] + b * m[8];
    let to_byte = |x: f64| js::round(js::max(0.0, js::min(1.0, x)) * 255.0);
    (to_byte(new_r), to_byte(new_g), to_byte(new_b))
}

/// `rgbToHex(r, g, b, a)` (`colors.ts:335-352`).
fn rgb_to_hex(r: f64, g: f64, b: f64, a: f64) -> String {
    // `<<` converts with ToInt32; the components are integers in 0..=255.
    let int = |x: f64| if x.is_finite() { x as i64 } else { 0 };
    let hex6 = format!("#{:06x}", (int(r) << 16) + (int(g) << 8) + int(b));
    if a < 1.0 {
        format!("{hex6}{:02x}", int(js::round(a * 255.0)))
    } else {
        hex6
    }
}

/// `tinycolor(value).getAlpha()` for any JSON value (`None` is
/// `undefined`), as `isTransparent` computes it for an untyped app-state
/// value. A falsy value reads as `""`, a string is parsed, another
/// primitive or an array has alpha 1, and an object is read as tinycolor
/// reads one (`inputToRGB`, `tinycolor.js:325-358`): its `a` key, after the
/// component keys it converts to strings on the way, which throws for an
/// object with its own `toString` key.
pub(crate) fn alpha_of_value(value: Option<&Value>) -> Result<f64, TypeError> {
    if !truthy(value) {
        return Ok(TinyColor::parse("").alpha());
    }
    match value {
        Some(Value::String(s)) => Ok(TinyColor::parse(s).alpha()),
        Some(Value::Object(map)) => object_alpha(map),
        _ => Ok(1.0),
    }
}

fn object_alpha(color: &Map<String, Value>) -> Result<f64, TypeError> {
    // isValidCSSUnit(x): CSS_UNIT, unanchored, matches String(x), i.e. it
    // holds a digit.
    let valid = |key: &str| -> Result<bool, TypeError> {
        Ok(to_string(color.get(key))?
            .bytes()
            .any(|b| b.is_ascii_digit()))
    };
    // The && chains short-circuit. Only their conversions matter here: the
    // components they validate do not change the alpha, and converting the
    // same values again in the conversions cannot throw where this did not.
    let all = |keys: [&str; 3]| -> Result<bool, TypeError> {
        for key in keys {
            if !valid(key)? {
                return Ok(false);
            }
        }
        Ok(true)
    };
    if !all(["r", "g", "b"])? && !all(["h", "s", "v"])? {
        all(["h", "s", "l"])?;
    }
    // color.hasOwnProperty("a"): an own key of that name is not callable.
    if color.contains_key("hasOwnProperty") {
        return Err(TypeError(
            "color.hasOwnProperty is not a function".to_owned(),
        ));
    }
    match color.get("a") {
        Some(a) => Ok(bound_alpha(parse_float_value(Some(a))?)),
        None => Ok(1.0),
    }
}

/// `boundAlpha(a)` (`tinycolor.js:955-961`), given `parseFloat(a)`.
fn bound_alpha(a: f64) -> f64 {
    if a.is_nan() || !(0.0..=1.0).contains(&a) {
        1.0
    } else {
        a
    }
}

/// `bound01(n, max)` (`tinycolor.js:964-982`): `n` from 0..=max (or a
/// percentage) to 0..=1.
fn bound01(n: &Unit, max: f64) -> f64 {
    let one_point_zero = matches!(n, Unit::Str(s) if s.contains('.') && parse_float(s) == 1.0);
    let n = if one_point_zero {
        Unit::Str("100%".to_owned())
    } else {
        n.clone()
    };
    let percent = matches!(&n, Unit::Str(s) if s.contains('%'));
    let mut x = js::min(max, js::max(0.0, n.parse_float()));
    if percent {
        x = parse_int_of_number(x * max) / 100.0;
    }
    if (x - max).abs() < 0.000001 {
        return 1.0;
    }
    x % max / max
}

/// `convertToPercentage(n)` (`tinycolor.js:1010-1015`).
fn convert_to_percentage(n: Unit) -> Unit {
    let x = n.to_number();
    if x <= 1.0 {
        Unit::Str(format!("{}%", number_to_string(x * 100.0)))
    } else {
        n
    }
}

/// `rgbToRgb(r, g, b)` (`tinycolor.js:372-378`).
fn rgb_to_rgb(r: &Unit, g: &Unit, b: &Unit) -> (f64, f64, f64) {
    (
        bound01(r, 255.0) * 255.0,
        bound01(g, 255.0) * 255.0,
        bound01(b, 255.0) * 255.0,
    )
}

/// `hslToRgb(h, s, l)` (`tinycolor.js:422-449`).
fn hsl_to_rgb(h: &Unit, s: &Unit, l: &Unit) -> (f64, f64, f64) {
    let h = bound01(h, 360.0);
    let s = bound01(s, 100.0);
    let l = bound01(l, 100.0);
    let hue_to_rgb = |p: f64, q: f64, mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    };
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        (
            hue_to_rgb(p, q, h + 1.0 / 3.0),
            hue_to_rgb(p, q, h),
            hue_to_rgb(p, q, h - 1.0 / 3.0),
        )
    };
    (r * 255.0, g * 255.0, b * 255.0)
}

/// `hsvToRgb(h, s, v)` (`tinycolor.js:493-512`).
fn hsv_to_rgb(h: &Unit, s: &Unit, v: &Unit) -> (f64, f64, f64) {
    let h = bound01(h, 360.0) * 6.0;
    let s = bound01(s, 100.0);
    let v = bound01(v, 100.0);
    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let m = i % 6.0;
    // `[...][mod]` is undefined (NaN once multiplied) off the array.
    let pick = |values: [f64; 6]| {
        if (0.0..6.0).contains(&m) {
            values[m as usize]
        } else {
            f64::NAN
        }
    };
    let r = pick([v, q, p, p, t, v]);
    let g = pick([t, v, v, q, p, p]);
    let b = pick([p, p, t, v, v, q]);
    (r * 255.0, g * 255.0, b * 255.0)
}

// ---------------------------------------------------------------------------
// String input

/// `stringInputToObject(color)` (`tinycolor.js:1065-1165`).
fn string_input_to_object(color: &str) -> Option<Parsed> {
    let lower = color.trim_matches(is_whitespace_char).to_lowercase();
    let color = match named_color(&lower) {
        Some(hex) => hex,
        None if lower == "transparent" => {
            return Some(Parsed::Rgb {
                r: Unit::Num(0.0),
                g: Unit::Num(0.0),
                b: Unit::Num(0.0),
                a: Some(Unit::Num(0.0)),
            });
        }
        None => lower.as_str(),
    };

    let unit = |s: &str| Unit::Str(s.to_owned());
    if let Some(m) = permissive_match(color, "rgb", 3) {
        return Some(Parsed::Rgb {
            r: unit(m[0]),
            g: unit(m[1]),
            b: unit(m[2]),
            a: None,
        });
    }
    if let Some(m) = permissive_match(color, "rgba", 4) {
        return Some(Parsed::Rgb {
            r: unit(m[0]),
            g: unit(m[1]),
            b: unit(m[2]),
            a: Some(unit(m[3])),
        });
    }
    if let Some(m) = permissive_match(color, "hsl", 3) {
        return Some(Parsed::Hsl {
            h: unit(m[0]),
            s: unit(m[1]),
            l: unit(m[2]),
            a: None,
        });
    }
    if let Some(m) = permissive_match(color, "hsla", 4) {
        return Some(Parsed::Hsl {
            h: unit(m[0]),
            s: unit(m[1]),
            l: unit(m[2]),
            a: Some(unit(m[3])),
        });
    }
    if let Some(m) = permissive_match(color, "hsv", 3) {
        return Some(Parsed::Hsv {
            h: unit(m[0]),
            s: unit(m[1]),
            v: unit(m[2]),
            a: None,
        });
    }
    if let Some(m) = permissive_match(color, "hsva", 4) {
        return Some(Parsed::Hsv {
            h: unit(m[0]),
            s: unit(m[1]),
            v: unit(m[2]),
            a: Some(unit(m[3])),
        });
    }
    hex_input(color)
}

/// The anchored `hex8`, `hex6`, `hex4` and `hex3` matchers
/// (`tinycolor.js:1052-1055`, tried in that order at `tinycolor.js:1125-1162`).
fn hex_input(color: &str) -> Option<Parsed> {
    let hex = color.strip_prefix('#').unwrap_or(color);
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // parseIntFromHex, and convertHexToDecimal (parseInt(h, 16) / 255).
    let int = |s: &str| f64::from(u8::from_str_radix(s, 16).unwrap_or(0));
    let byte = |s: &str| Unit::Num(int(s));
    let alpha = |s: &str| Unit::Num(int(s) / 255.0);
    let pair = |i: usize| hex[i..i + 1].repeat(2);
    match hex.len() {
        8 => Some(Parsed::Rgb {
            r: byte(&hex[0..2]),
            g: byte(&hex[2..4]),
            b: byte(&hex[4..6]),
            a: Some(alpha(&hex[6..8])),
        }),
        6 => Some(Parsed::Rgb {
            r: byte(&hex[0..2]),
            g: byte(&hex[2..4]),
            b: byte(&hex[4..6]),
            a: None,
        }),
        4 => Some(Parsed::Rgb {
            r: byte(&pair(0)),
            g: byte(&pair(1)),
            b: byte(&pair(2)),
            a: Some(alpha(&pair(3))),
        }),
        3 => Some(Parsed::Rgb {
            r: byte(&pair(0)),
            g: byte(&pair(1)),
            b: byte(&pair(2)),
            a: None,
        }),
        _ => None,
    }
}

/// The `rgb`, `rgba`, `hsl`, `hsla`, `hsv` and `hsva` matchers
/// (`tinycolor.js:1025-1056`): `prefix` then
/// `[\s|\(]+(UNIT)[,|\s]+(UNIT)...\s*\)?`, unanchored, where
/// `UNIT = [-\+]?\d*\.\d+%?|[-\+]?\d+%?`. The separators and a unit share
/// no character, so from a given start the greedy match below is the only
/// one the regex can find; the leftmost start that matches wins, as with
/// `exec`.
fn permissive_match<'a>(s: &'a str, prefix: &str, units: usize) -> Option<Vec<&'a str>> {
    let mut from = 0;
    while let Some(offset) = s[from..].find(prefix) {
        let start = from + offset;
        if let Some(captures) = match_units(&s[start + prefix.len()..], units) {
            return Some(captures);
        }
        // The prefixes are ASCII, so the next start is a char boundary.
        from = start + 1;
    }
    None
}

fn match_units(s: &str, units: usize) -> Option<Vec<&str>> {
    let mut rest = s;
    let mut captures = Vec::with_capacity(units);
    for i in 0..units {
        let separator =
            |c: char| is_whitespace_char(c) || c == '|' || if i == 0 { c == '(' } else { c == ',' };
        let after = rest.trim_start_matches(separator);
        if after.len() == rest.len() {
            return None;
        }
        let len = css_unit_len(after)?;
        captures.push(&after[..len]);
        rest = &after[len..];
    }
    // `\s*\)?` also matches the empty string.
    Some(captures)
}

/// The length of the `CSS_UNIT` match at the start of `s`, if any.
fn css_unit_len(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let sign = usize::from(matches!(bytes.first(), Some(b'-' | b'+')));
    let digits = |from: usize| {
        bytes
            .get(from..)
            .unwrap_or_default()
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let percent = |at: usize| usize::from(bytes.get(at) == Some(&b'%'));
    // [-\+]?\d*\.\d+%?
    let int = digits(sign);
    if bytes.get(sign + int) == Some(&b'.') {
        let frac = digits(sign + int + 1);
        if frac > 0 {
            let end = sign + int + 1 + frac;
            return Some(end + percent(end));
        }
    }
    // [-\+]?\d+%?
    if int > 0 {
        let end = sign + int;
        return Some(end + percent(end));
    }
    None
}

/// tinycolor's `names` (`tinycolor.js:785-936`): a CSS colour name to its
/// hex digits. `names[color]` is a plain object lookup, so it also finds
/// inherited properties (`constructor`, `toString`, ...), but those are
/// functions, which no matcher accepts: they behave as unknown names.
fn named_color(name: &str) -> Option<&'static str> {
    NAMES
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| NAMES[i].1)
}

/// tinycolor's `names`, sorted by name (as tinycolor lists them).
#[rustfmt::skip]
const NAMES: [(&str, &str); 149] = [
    ("aliceblue", "f0f8ff"), ("antiquewhite", "faebd7"), ("aqua", "0ff"),
    ("aquamarine", "7fffd4"), ("azure", "f0ffff"), ("beige", "f5f5dc"), ("bisque", "ffe4c4"),
    ("black", "000"), ("blanchedalmond", "ffebcd"), ("blue", "00f"), ("blueviolet", "8a2be2"),
    ("brown", "a52a2a"), ("burlywood", "deb887"), ("burntsienna", "ea7e5d"),
    ("cadetblue", "5f9ea0"), ("chartreuse", "7fff00"), ("chocolate", "d2691e"),
    ("coral", "ff7f50"), ("cornflowerblue", "6495ed"), ("cornsilk", "fff8dc"),
    ("crimson", "dc143c"), ("cyan", "0ff"), ("darkblue", "00008b"), ("darkcyan", "008b8b"),
    ("darkgoldenrod", "b8860b"), ("darkgray", "a9a9a9"), ("darkgreen", "006400"),
    ("darkgrey", "a9a9a9"), ("darkkhaki", "bdb76b"), ("darkmagenta", "8b008b"),
    ("darkolivegreen", "556b2f"), ("darkorange", "ff8c00"), ("darkorchid", "9932cc"),
    ("darkred", "8b0000"), ("darksalmon", "e9967a"), ("darkseagreen", "8fbc8f"),
    ("darkslateblue", "483d8b"), ("darkslategray", "2f4f4f"), ("darkslategrey", "2f4f4f"),
    ("darkturquoise", "00ced1"), ("darkviolet", "9400d3"), ("deeppink", "ff1493"),
    ("deepskyblue", "00bfff"), ("dimgray", "696969"), ("dimgrey", "696969"),
    ("dodgerblue", "1e90ff"), ("firebrick", "b22222"), ("floralwhite", "fffaf0"),
    ("forestgreen", "228b22"), ("fuchsia", "f0f"), ("gainsboro", "dcdcdc"),
    ("ghostwhite", "f8f8ff"), ("gold", "ffd700"), ("goldenrod", "daa520"), ("gray", "808080"),
    ("green", "008000"), ("greenyellow", "adff2f"), ("grey", "808080"), ("honeydew", "f0fff0"),
    ("hotpink", "ff69b4"), ("indianred", "cd5c5c"), ("indigo", "4b0082"), ("ivory", "fffff0"),
    ("khaki", "f0e68c"), ("lavender", "e6e6fa"), ("lavenderblush", "fff0f5"),
    ("lawngreen", "7cfc00"), ("lemonchiffon", "fffacd"), ("lightblue", "add8e6"),
    ("lightcoral", "f08080"), ("lightcyan", "e0ffff"), ("lightgoldenrodyellow", "fafad2"),
    ("lightgray", "d3d3d3"), ("lightgreen", "90ee90"), ("lightgrey", "d3d3d3"),
    ("lightpink", "ffb6c1"), ("lightsalmon", "ffa07a"), ("lightseagreen", "20b2aa"),
    ("lightskyblue", "87cefa"), ("lightslategray", "789"), ("lightslategrey", "789"),
    ("lightsteelblue", "b0c4de"), ("lightyellow", "ffffe0"), ("lime", "0f0"),
    ("limegreen", "32cd32"), ("linen", "faf0e6"), ("magenta", "f0f"), ("maroon", "800000"),
    ("mediumaquamarine", "66cdaa"), ("mediumblue", "0000cd"), ("mediumorchid", "ba55d3"),
    ("mediumpurple", "9370db"), ("mediumseagreen", "3cb371"), ("mediumslateblue", "7b68ee"),
    ("mediumspringgreen", "00fa9a"), ("mediumturquoise", "48d1cc"), ("mediumvioletred", "c71585"),
    ("midnightblue", "191970"), ("mintcream", "f5fffa"), ("mistyrose", "ffe4e1"),
    ("moccasin", "ffe4b5"), ("navajowhite", "ffdead"), ("navy", "000080"), ("oldlace", "fdf5e6"),
    ("olive", "808000"), ("olivedrab", "6b8e23"), ("orange", "ffa500"), ("orangered", "ff4500"),
    ("orchid", "da70d6"), ("palegoldenrod", "eee8aa"), ("palegreen", "98fb98"),
    ("paleturquoise", "afeeee"), ("palevioletred", "db7093"), ("papayawhip", "ffefd5"),
    ("peachpuff", "ffdab9"), ("peru", "cd853f"), ("pink", "ffc0cb"), ("plum", "dda0dd"),
    ("powderblue", "b0e0e6"), ("purple", "800080"), ("rebeccapurple", "663399"), ("red", "f00"),
    ("rosybrown", "bc8f8f"), ("royalblue", "4169e1"), ("saddlebrown", "8b4513"),
    ("salmon", "fa8072"), ("sandybrown", "f4a460"), ("seagreen", "2e8b57"),
    ("seashell", "fff5ee"), ("sienna", "a0522d"), ("silver", "c0c0c0"), ("skyblue", "87ceeb"),
    ("slateblue", "6a5acd"), ("slategray", "708090"), ("slategrey", "708090"), ("snow", "fffafa"),
    ("springgreen", "00ff7f"), ("steelblue", "4682b4"), ("tan", "d2b48c"), ("teal", "008080"),
    ("thistle", "d8bfd8"), ("tomato", "ff6347"), ("turquoise", "40e0d0"), ("violet", "ee82ee"),
    ("wheat", "f5deb3"), ("white", "fff"), ("whitesmoke", "f5f5f5"), ("yellow", "ff0"),
    ("yellowgreen", "9acd32"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_are_sorted_for_binary_search() {
        assert!(NAMES.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(named_color("rebeccapurple"), Some("663399"));
        assert_eq!(named_color("transparent"), None);
    }

    #[test]
    fn functional_notations_match_anywhere() {
        assert_eq!(
            permissive_match("xrgb(1,2,3)", "rgb", 3),
            Some(vec!["1", "2", "3"])
        );
        assert_eq!(permissive_match("rgba(1,2,3)", "rgb", 3), None);
        assert_eq!(permissive_match("rgb(1., 2, 3)", "rgb", 3), None);
        assert_eq!(
            permissive_match("rgb(-.5%,+2,3.25)", "rgb", 3),
            Some(vec!["-.5%", "+2", "3.25"])
        );
    }

    #[test]
    fn object_alpha_reads_a_and_throws_like_tinycolor() {
        assert_eq!(alpha_of_value(Some(&json!({"a": 0}))), Ok(0.0));
        assert_eq!(alpha_of_value(Some(&json!({"a": [0.5, 1]}))), Ok(0.5));
        assert_eq!(alpha_of_value(Some(&json!({"a": null}))), Ok(1.0));
        assert_eq!(alpha_of_value(Some(&json!([0]))), Ok(1.0));
        assert_eq!(alpha_of_value(Some(&json!(5))), Ok(1.0));
        assert!(alpha_of_value(Some(&json!({"a": {"toString": 1}}))).is_err());
        assert!(alpha_of_value(Some(&json!({"r": {"toString": 1}}))).is_err());
    }
}
