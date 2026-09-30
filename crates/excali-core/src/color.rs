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
//!
//! It also holds the dark-mode colour filter (`applyDarkModeFilter` and
//! `removeDarkModeFilter`, `colors.ts:86-160`), `rgbToHex` and
//! `COLOR_PALETTE`; `excali-scene/tests/dark_mode.rs` checks them against
//! upstream (`tools/goldens/dark-mode.mjs`).

use excali_math::js;
use serde_json::{Map, Value};

use crate::constants::{COLOR_BLACK, COLOR_TRANSPARENT, COLOR_WHITE};

use crate::js::{
    is_whitespace_char, number_to_string, parse_float, parse_float_value, parse_int_of_number,
    string_to_number, to_int32, to_string, truthy, TypeError,
};

/// A parsed colour: tinycolor's `_r`, `_g`, `_b`, `_a` and `_ok`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TinyColor {
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    ok: bool,
    /// `getFormat()` is `"hex"` or `"hex8"`: the input was hex digits, not
    /// a colour name.
    hex: bool,
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
        let Some((parsed, hex)) = string_input_to_object(color) else {
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
        TinyColor {
            hex,
            ..TinyColor::from_rgb(rgb, a, true)
        }
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
            hex: false,
        }
    }

    /// `isValid()`: the input was a colour tinycolor recognises.
    pub fn is_valid(&self) -> bool {
        self.ok
    }

    /// `getFormat()` is `"hex"` or `"hex8"` (`stringInputToObject`,
    /// `tinycolor.js:1125-1162`): hex digits, with or without `#`; a colour
    /// name is `"name"` even though it resolves through hex.
    pub fn is_hex_format(&self) -> bool {
        self.hex
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
    Some(rgb_to_hex(r, g, b, Some(a)))
}

/// `isTransparent(color)` (`colors.ts:389-391`): the alpha is 0.
pub fn is_transparent(color: &str) -> bool {
    TinyColor::parse(color).alpha() == 0.0
}

/// `isOpaqueColor(color)` (`colors.ts:400-402`): the alpha is 1.
pub fn is_opaque_color(color: &str) -> bool {
    TinyColor::parse(color).alpha() == 1.0
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
    let (r, g, b, _) = tc.to_rgb();
    let (r, g, b) = dark_mode_filter_rgb(r, g, b);
    rgb_to_hex(r, g, b, Some(alpha))
}

/// `removeDarkModeFilter(color)` (`colors.ts:141-160`): the colour that
/// [`apply_dark_mode_filter`] turns into `color`, as near as whole
/// components allow, as `#rrggbb` (or `#rrggbbaa` below alpha 1; the alpha
/// is kept). The 180 degree hue rotation is undone first (it is its own
/// inverse), then the 93% inversion, clamped to 0..=255. A string tinycolor
/// does not recognise reads as opaque black.
pub fn remove_dark_mode_filter(color: &str) -> String {
    let tc = TinyColor::parse(color);
    let alpha = tc.alpha();
    let (r, g, b, _) = tc.to_rgb();
    let (r, g, b) = css_hue_rotate((r, g, b), DARK_MODE_FILTER_HUE_ROTATE_DEGREES);
    let (r, g, b) = reverse_dark_mode_invert(r, g, b);
    rgb_to_hex(r, g, b, Some(alpha))
}

/// `_reverseDarkModeInvert(r, g, b)` (`colors.ts:124-139`): solves
/// `c * (1 - p) + (255 - c) * p` for `c`, rounded and clamped to 0..=255.
fn reverse_dark_mode_invert(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let p = DARK_MODE_FILTER_INVERT_PERCENT / 100.0;
    let denominator = 1.0 - 2.0 * p;
    let restore = |c: f64| {
        js::round(excali_math::clamp(
            (c - 255.0 * p) / denominator,
            0.0,
            255.0,
        ))
    };
    (restore(r), restore(g), restore(b))
}

/// The numeric `DARK_THEME_FILTER` of [`apply_dark_mode_filter`] on 0..=255
/// components (`colors.ts:98-112`): `cssInvert` by 93%, then `cssHueRotate`
/// by 180 degrees, each rounding to whole components. The display list
/// applies it to image pixels (`excali_scene::display::ImageFilter`).
pub fn dark_mode_filter_rgb(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    // Order matters: invert, then hue-rotate, as the CSS filter list.
    let inverted = css_invert(r, g, b, DARK_MODE_FILTER_INVERT_PERCENT);
    css_hue_rotate(inverted, DARK_MODE_FILTER_HUE_ROTATE_DEGREES)
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
    let (c, s) = (js::cos(a), js::sin(a));
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

/// `ColorTuple` (`colors.ts:179`): five shades of one hue, lightest first.
pub type ColorTuple = [&'static str; 5];

/// One entry of [`COLOR_PALETTE`]: a single colour or a hue's shades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteColor {
    Single(&'static str),
    Shades(ColorTuple),
}

/// The shape of `COLOR_PALETTE` (`colors.ts:193-212`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorPalette {
    pub transparent: &'static str,
    pub black: &'static str,
    pub white: &'static str,
    pub gray: ColorTuple,
    pub red: ColorTuple,
    pub pink: ColorTuple,
    pub grape: ColorTuple,
    pub violet: ColorTuple,
    pub blue: ColorTuple,
    pub cyan: ColorTuple,
    pub teal: ColorTuple,
    pub green: ColorTuple,
    pub yellow: ColorTuple,
    pub orange: ColorTuple,
    pub bronze: ColorTuple,
}

impl ColorPalette {
    /// The entries by name, in upstream's key order.
    pub fn entries(&self) -> [(&'static str, PaletteColor); 15] {
        use PaletteColor::{Shades, Single};
        [
            ("transparent", Single(self.transparent)),
            ("black", Single(self.black)),
            ("white", Single(self.white)),
            ("gray", Shades(self.gray)),
            ("red", Shades(self.red)),
            ("pink", Shades(self.pink)),
            ("grape", Shades(self.grape)),
            ("violet", Shades(self.violet)),
            ("blue", Shades(self.blue)),
            ("cyan", Shades(self.cyan)),
            ("teal", Shades(self.teal)),
            ("green", Shades(self.green)),
            ("yellow", Shades(self.yellow)),
            ("orange", Shades(self.orange)),
            ("bronze", Shades(self.bronze)),
        ]
    }
}

/// `COLOR_PALETTE` (`colors.ts:193-212`): open-color shades at indexes
/// 0, 2, 4, 6, 8 (weights 50, 200, 400, 600, 800) and radix bronze shades
/// 3, 5, 7, 9, 11.
pub const COLOR_PALETTE: ColorPalette = ColorPalette {
    transparent: COLOR_TRANSPARENT,
    black: COLOR_BLACK,
    white: COLOR_WHITE,
    gray: ["#f8f9fa", "#e9ecef", "#ced4da", "#868e96", "#343a40"],
    red: ["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"],
    pink: ["#fff0f6", "#fcc2d7", "#f783ac", "#e64980", "#c2255c"],
    grape: ["#f8f0fc", "#eebefa", "#da77f2", "#be4bdb", "#9c36b5"],
    violet: ["#f3f0ff", "#d0bfff", "#9775fa", "#7950f2", "#6741d9"],
    blue: ["#e7f5ff", "#a5d8ff", "#4dabf7", "#228be6", "#1971c2"],
    cyan: ["#e3fafc", "#99e9f2", "#3bc9db", "#15aabf", "#0c8599"],
    teal: ["#e6fcf5", "#96f2d7", "#38d9a9", "#12b886", "#099268"],
    green: ["#ebfbee", "#b2f2bb", "#69db7c", "#40c057", "#2f9e44"],
    yellow: ["#fff9db", "#ffec99", "#ffd43b", "#fab005", "#f08c00"],
    orange: ["#fff4e6", "#ffd8a8", "#ffa94d", "#fd7e14", "#e8590c"],
    bronze: ["#f8f1ee", "#eaddd7", "#d2bab0", "#a18072", "#846358"],
};

/// `MAX_CUSTOM_COLORS_USED_IN_CANVAS` (`colors.ts:185`): how many
/// most-used custom colours the picker offers.
pub const MAX_CUSTOM_COLORS_USED_IN_CANVAS: usize = 5;
/// `COLORS_PER_ROW` (`colors.ts:186`): the picker's grid width.
pub const COLORS_PER_ROW: usize = 5;
/// `DEFAULT_ELEMENT_STROKE_COLOR_INDEX` (`colors.ts:190`): the shade stroke
/// picks and hotkeys use.
pub const DEFAULT_ELEMENT_STROKE_COLOR_INDEX: usize = 4;
/// `DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX` (`colors.ts:191`): the shade
/// background picks and hotkeys use.
pub const DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX: usize = 1;

/// One palette entry: its name (a `colors.*` locale key) and colour.
pub type PaletteEntry = (&'static str, PaletteColor);

const fn stroke_shade(shades: ColorTuple) -> &'static str {
    shades[DEFAULT_ELEMENT_STROKE_COLOR_INDEX]
}

const fn background_shade(shades: ColorTuple) -> &'static str {
    shades[DEFAULT_ELEMENT_BACKGROUND_COLOR_INDEX]
}

/// `DEFAULT_ELEMENT_STROKE_PICKS` (`colors.ts:239-245`): the stroke
/// picker's top picks, in strip order.
pub const DEFAULT_ELEMENT_STROKE_PICKS: ColorTuple = [
    COLOR_PALETTE.black,
    stroke_shade(COLOR_PALETTE.red),
    stroke_shade(COLOR_PALETTE.green),
    stroke_shade(COLOR_PALETTE.blue),
    stroke_shade(COLOR_PALETTE.yellow),
];

/// `DEFAULT_ELEMENT_BACKGROUND_PICKS` (`colors.ts:248-254`).
pub const DEFAULT_ELEMENT_BACKGROUND_PICKS: ColorTuple = [
    COLOR_PALETTE.transparent,
    background_shade(COLOR_PALETTE.red),
    background_shade(COLOR_PALETTE.green),
    background_shade(COLOR_PALETTE.blue),
    background_shade(COLOR_PALETTE.yellow),
];

/// `BUCKET_FILL_BACKGROUND_PICKS` (`colors.ts:259-265`): the background
/// picks with white for transparent, a no-op fill.
pub const BUCKET_FILL_BACKGROUND_PICKS: ColorTuple = [
    COLOR_PALETTE.white,
    background_shade(COLOR_PALETTE.red),
    background_shade(COLOR_PALETTE.green),
    background_shade(COLOR_PALETTE.blue),
    background_shade(COLOR_PALETTE.yellow),
];

/// `STICKY_NOTE_STROKE_PICKS` (`colors.ts:270-271`).
pub const STICKY_NOTE_STROKE_PICKS: ColorTuple = DEFAULT_ELEMENT_STROKE_PICKS;

/// `STICKY_NOTE_BACKGROUND_PICKS` (`colors.ts:275-281`): classic note
/// colours, never transparent.
pub const STICKY_NOTE_BACKGROUND_PICKS: ColorTuple = [
    crate::constants::DEFAULT_STICKY_NOTE_BG,
    COLOR_PALETTE.pink[1],
    COLOR_PALETTE.green[1],
    COLOR_PALETTE.blue[1],
    COLOR_PALETTE.orange[1],
];

/// `DEFAULT_CANVAS_BACKGROUND_PICKS` (`colors.ts:284-294`): white and the
/// radix slate2, blue2, yellow2 and bronze2.
pub const DEFAULT_CANVAS_BACKGROUND_PICKS: ColorTuple = [
    COLOR_PALETTE.white,
    "#f8f9fa",
    "#f5faff",
    "#fffce8",
    "#fdf8f6",
];

/// The stroke and background palettes' shared shape (`colors.ts:299-319`):
/// the 5×3 grid, row 1 transparent, white, gray, black, bronze, then
/// `COMMON_ELEMENT_SHADES` (`colors.ts:214-225`) in its key order.
const ELEMENT_COLOR_PALETTE: [PaletteEntry; 15] = {
    use PaletteColor::{Shades, Single};
    let p = COLOR_PALETTE;
    [
        ("transparent", Single(p.transparent)),
        ("white", Single(p.white)),
        ("gray", Shades(p.gray)),
        ("black", Single(p.black)),
        ("bronze", Shades(p.bronze)),
        ("cyan", Shades(p.cyan)),
        ("blue", Shades(p.blue)),
        ("violet", Shades(p.violet)),
        ("grape", Shades(p.grape)),
        ("pink", Shades(p.pink)),
        ("green", Shades(p.green)),
        ("teal", Shades(p.teal)),
        ("yellow", Shades(p.yellow)),
        ("orange", Shades(p.orange)),
        ("red", Shades(p.red)),
    ]
};

/// `DEFAULT_ELEMENT_STROKE_COLOR_PALETTE` (`colors.ts:299-308`).
pub const DEFAULT_ELEMENT_STROKE_COLOR_PALETTE: [PaletteEntry; 15] = ELEMENT_COLOR_PALETTE;

/// `DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE` (`colors.ts:311-319`).
pub const DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE: [PaletteEntry; 15] = ELEMENT_COLOR_PALETTE;

/// `getAllColorsSpecificShade(index)` (`colors.ts:326-340`): shade `index`
/// of palette rows 2 and 3.
pub fn get_all_colors_specific_shade(index: usize) -> [&'static str; 10] {
    let p = COLOR_PALETTE;
    [
        p.cyan, p.blue, p.violet, p.grape, p.pink, p.green, p.teal, p.yellow, p.orange, p.red,
    ]
    .map(|shades| shades[index])
}

/// `COLOR_OUTLINE_CONTRAST_THRESHOLD` (`colors.ts:408`): a swatch lighter
/// than this gets an outline.
pub const COLOR_OUTLINE_CONTRAST_THRESHOLD: f64 = 240.0;

/// `isColorDark(color, threshold = 160)` (`colors.ts:416-434`): the YIQ
/// brightness of the colour is below `threshold` (`None` is 160). No
/// colour (`""`) and an invalid one count as dark (they default to
/// black), a fully transparent one as light.
pub fn is_color_dark(color: &str, threshold: Option<f64>) -> bool {
    if color.is_empty() {
        return true;
    }
    if is_transparent(color) {
        return false;
    }
    let tc = TinyColor::parse(color);
    if !tc.is_valid() {
        return true;
    }
    let (r, g, b, _) = tc.to_rgb();
    (r * 299.0 + g * 587.0 + b * 114.0) / 1000.0 < threshold.unwrap_or(160.0)
}

/// `String.prototype.trim`: `s` without the ECMAScript white space and
/// line terminators around it (as the colour picker's hex input trims what
/// is typed).
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_whitespace_char)
}

/// `normalizeInputColor(color)` (`colors.ts:444-461`): the trimmed input
/// when it is a colour, with `#` added to bare hex digits; `None` when it
/// is not one.
pub fn normalize_input_color(color: &str) -> Option<String> {
    // String.prototype.trim
    let color = color.trim_matches(is_whitespace_char);
    if is_transparent(color) {
        return Some(color.to_owned());
    }
    let tc = TinyColor::parse(color);
    if !tc.is_valid() {
        return None;
    }
    if tc.is_hex_format() && !color.starts_with('#') {
        return Some(format!("#{color}"));
    }
    Some(color.to_owned())
}

/// `rgbToHex(r, g, b, a)` (`colors.ts:345-362`): `#rrggbb`, with a
/// two-digit alpha `round(a * 255)` appended when `a` is given and below 1
/// (`None` is `undefined`).
///
/// The components are whole numbers, as every caller passes them
/// (tinycolor's rounded `toRgb()` and the filter's rounded channels).
/// Outside 0..=255 they spill into the neighbouring digits as upstream's
/// `(1 << 24) + (r << 16) + (g << 8) + b` does.
pub fn rgb_to_hex(r: f64, g: f64, b: f64, a: Option<f64>) -> String {
    debug_assert!(
        [r, g, b].iter().all(|x| x.fract() == 0.0),
        "rgbToHex takes whole components"
    );
    // `<<` converts its left operand with ToInt32 and wraps in 32 bits; the
    // additions are on numbers.
    let shl = |x: f64, n: u32| f64::from(to_int32(x).wrapping_shl(n));
    let sum = f64::from(1 << 24) + shl(r, 16) + shl(g, 8) + b;
    // `.toString(16).slice(1)` drops the leading "1" (or the minus sign).
    let hex = js_radix16_integer(sum);
    let hex6 = format!("#{}", hex.get(1..).unwrap_or(""));
    match a {
        Some(a) if a < 1.0 => {
            let alpha = js_radix16_integer(js::round(a * 255.0));
            format!("{hex6}{alpha:0>2}")
        }
        _ => hex6,
    }
}

/// `Number.prototype.toString(16)` of a whole number or an infinity.
fn js_radix16_integer(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_owned();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    if x.is_infinite() {
        return format!("{sign}Infinity");
    }
    // Every whole double below 2^128 is exactly a u128; an alpha beyond
    // that is not one a colour carries.
    let magnitude = x.abs().min(u128::MAX as f64) as u128;
    format!("{sign}{magnitude:x}")
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
/// The object `stringInputToObject` returns, and whether its format is
/// `hex` or `hex8`.
fn string_input_to_object(color: &str) -> Option<(Parsed, bool)> {
    let lower = color.trim_matches(is_whitespace_char).to_lowercase();
    let named = named_color(&lower);
    let color = match named {
        Some(hex) => hex,
        None if lower == "transparent" => {
            return Some((
                Parsed::Rgb {
                    r: Unit::Num(0.0),
                    g: Unit::Num(0.0),
                    b: Unit::Num(0.0),
                    a: Some(Unit::Num(0.0)),
                },
                false,
            ));
        }
        None => lower.as_str(),
    };
    if let Some(parsed) = functional_input(color) {
        return Some((parsed, false));
    }
    hex_input(color).map(|parsed| (parsed, named.is_none()))
}

/// The `rgb`, `rgba`, `hsl`, `hsla`, `hsv` and `hsva` notations.
fn functional_input(color: &str) -> Option<Parsed> {
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
    None
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
    fn rgb_to_hex_spills_like_upstreams_shifts() {
        // (1 << 24) + (256 << 16) = 0x2000000; slice(1) drops the "2".
        assert_eq!(rgb_to_hex(256.0, 0.0, 0.0, None), "#000000");
        // 0x1000000 - 0x10000 = 0xff0000; slice(1) drops an "f".
        assert_eq!(rgb_to_hex(-1.0, 0.0, 0.0, None), "#f0000");
        // (2^31 + 1) << 16 wraps in 32 bits to 0x10000.
        assert_eq!(rgb_to_hex(2_147_483_649.0, 0.0, 0.0, None), "#010000");
        // round(-0.5 * 255) = -127, "-7f"; NaN is not below 1.
        assert_eq!(rgb_to_hex(0.0, 0.0, 0.0, Some(-0.5)), "#000000-7f");
        assert_eq!(rgb_to_hex(0.0, 0.0, 0.0, Some(f64::NAN)), "#000000");
        assert_eq!(
            rgb_to_hex(0.0, 0.0, 0.0, Some(f64::NEG_INFINITY)),
            "#000000-Infinity"
        );
    }

    #[test]
    fn palette_singles_are_the_element_defaults() {
        assert_eq!(COLOR_PALETTE.black, COLOR_BLACK);
        assert_eq!(COLOR_PALETTE.white, COLOR_WHITE);
        assert_eq!(COLOR_PALETTE.transparent, COLOR_TRANSPARENT);
        assert_eq!(COLOR_PALETTE.entries()[0].0, "transparent");
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
