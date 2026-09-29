//! Untyped JSON round trip with upstream's output.
//!
//! Upstream writes `.excalidraw` files with `JSON.stringify(data, null, 2)`
//! (`packages/excalidraw/data/json.ts`, `serializeAsJSON`) and reads them with
//! `JSON.parse`. [`round_trip`] produces exactly what
//! `JSON.stringify(JSON.parse(text), null, 2)` produces:
//!
//! - two-space indent, empty containers written as `[]` / `{}`, no trailing
//!   newline;
//! - object keys in JS property order: array-index keys (`"0"` to
//!   `"4294967294"`) first in ascending order, then the rest in insertion
//!   order; a duplicated key keeps its first position and its last value;
//! - numbers as ECMAScript `Number::toString`: parsed to the nearest f64,
//!   then plain decimal for `1e-6 <= |x| < 1e21`, exponent form (`1e+21`,
//!   `1.5e-7`) otherwise, integral values without `.0`, `-0` as `0`;
//! - strings as well-formed `JSON.stringify`: `\b \t \n \f \r \" \\` short
//!   escapes, other control characters as lowercase `\u00xx`, everything
//!   else raw, and a lone UTF-16 surrogate (from a `\udXXX` escape that is
//!   not half of a valid pair, e.g. a split emoji in a text element) written
//!   back as its lowercase `\udxxx` escape.
//!
//! Rust strings cannot hold lone surrogates, so [`round_trip`] carries them
//! through the parsed value in a private *sentinel form*: a lone surrogate
//! is U+FDD0 followed by a character in U+E000..=U+E7FF, and a literal
//! U+FDD0 is doubled. [`write_parsed`] turns the form back into what
//! `JSON.stringify` writes. [`to_string_pretty`] takes an ordinary
//! [`serde_json::Value`] and writes every string as it is.
//!
//! The sentinel form is internal to the crate: [`parse`] produces it and
//! [`write_parsed`] consumes it. The typed codec converts at its boundary:
//! [`decode`] gives the public value (a lone surrogate becomes U+FFFD, a
//! doubled U+FDD0 one U+FDD0), [`escape`] turns a public value back into the
//! sentinel form (U+FDD0 doubled), and [`crate::layout::Layout`] keeps the
//! raw sentinel-form value of anything [`decode`] changed until the typed
//! value changes. So no public field, `extra` map or serde output ever holds
//! a sentinel, and a string built in Rust is written as it is.
//!
//! A number literal beyond the f64 range (`1e400`, `-1E+400`, 400 digits)
//! is `Infinity` or `-Infinity` to `JSON.parse`, and `JSON.stringify`
//! writes both as `null`. A [`Value`] cannot hold a non-finite number, so
//! [`parse`] reads such a literal as `null`: every writer here then gives
//! upstream's output, and a document or paste that holds one parses as it
//! does upstream instead of failing. What differs is a reader that tells
//! `Infinity` from `null`: restore's `x: element.x ?? 0` keeps `Infinity`
//! upstream and gives `0` here, and `isFiniteNumber` checks see `null`
//! either way.
//!
//! The typed codec ([`crate::document::Document`]) builds on this module.

use serde::Serialize;
use serde_json::ser::{CompactFormatter, Formatter, PrettyFormatter};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::io::{self, Write};

/// Error returned when the input is not valid JSON.
pub type Error = serde_json::Error;

/// Serialise a JSON value as `JSON.stringify(value, null, 2)` would, given a
/// JS object with the same properties assigned in the map's order: array
/// index keys first in ascending order (a JS object always enumerates them
/// so), then the other keys in map order.
pub fn to_string_pretty(value: &Value) -> String {
    write(value, false)
}

/// Parse `text` and write it back as `JSON.stringify(JSON.parse(text), null, 2)`
/// would.
///
/// A file that upstream wrote therefore comes back byte-identical (minus any
/// trailing newline an editor added), key order included.
pub fn round_trip(text: &str) -> Result<String, Error> {
    Ok(write(&parse(text)?, true))
}

/// `JSON.parse(text)` as a [`Value`]: keys in JS property order, lone
/// surrogates carried as sentinel pairs, a number beyond the f64 range as
/// `null` (see the module docs). Only [`write_parsed`] turns the sentinels
/// back into escapes.
pub(crate) fn parse(text: &str) -> Result<Value, Error> {
    let encoded = encode_lone_surrogates(text);
    let mut value: Value = match serde_json::from_str(&encoded) {
        Ok(value) => value,
        // serde_json rejects a literal that rounds to infinity; JSON.parse
        // does not. Only then is the text rescanned.
        Err(error) => match null_overflowing_numbers(&encoded) {
            Some(nulled) => serde_json::from_str(&nulled)?,
            None => return Err(error),
        },
    };
    order_keys_like_js(&mut value);
    Ok(value)
}

/// `text` with every number literal outside strings that is valid JSON
/// number syntax and rounds to an infinite f64 replaced by `null`; `None`
/// when there is none. Anything malformed is left for serde_json to reject.
fn null_overflowing_numbers(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut copied = 0;
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            match b {
                b'\\' => i += 2,
                b'"' => {
                    in_string = false;
                    i += 1;
                }
                _ => i += 1,
            }
            continue;
        }
        if b == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if b != b'-' && !b.is_ascii_digit() {
            i += 1;
            continue;
        }
        let end = i + bytes[i..]
            .iter()
            .take_while(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E'))
            .count();
        let token = &text[i..end];
        if is_json_number(token) && token.parse::<f64>().is_ok_and(f64::is_infinite) {
            out.push_str(&text[copied..i]);
            out.push_str("null");
            copied = end;
        }
        i = end;
    }
    if copied == 0 {
        return None;
    }
    out.push_str(&text[copied..]);
    Some(out)
}

/// `token` matches the JSON number grammar (RFC 8259 section 6):
/// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`.
fn is_json_number(token: &str) -> bool {
    let digits = |s: &[u8]| s.iter().take_while(|c| c.is_ascii_digit()).count();
    let mut s = token.as_bytes();
    if let [b'-', rest @ ..] = s {
        s = rest;
    }
    let int = digits(s);
    if int == 0 || (int > 1 && s[0] == b'0') {
        return false;
    }
    s = &s[int..];
    if let [b'.', rest @ ..] = s {
        let frac = digits(rest);
        if frac == 0 {
            return false;
        }
        s = &rest[frac..];
    }
    if let [b'e' | b'E', rest @ ..] = s {
        let rest = match rest {
            [b'+' | b'-', r @ ..] => r,
            r => r,
        };
        let exp = digits(rest);
        if exp == 0 {
            return false;
        }
        s = &rest[exp..];
    }
    s.is_empty()
}

/// [`to_string_pretty`] for a value in the sentinel form (from [`parse`] or
/// [`escape`]): sentinel pairs are written back as the lone surrogate
/// escapes they stand for, a doubled U+FDD0 as one.
pub(crate) fn write_parsed(value: &Value) -> String {
    write(value, true)
}

/// True when [`to_string_pretty`] writes `a` and `b` identically: numbers
/// compared as the f64 `JSON.parse` gives, object keys compared in order.
pub(crate) fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|((ka, va), (kb, vb))| ka == kb && same(va, vb))
        }
        _ => a == b,
    }
}

/// [`to_string_pretty`] without indentation: `JSON.stringify(value)`.
pub fn to_string_compact(value: &Value) -> String {
    write_with(value, false, false)
}

/// [`write_parsed`] without indentation: `JSON.stringify(value)` for a value
/// in the sentinel form.
pub(crate) fn write_parsed_compact(value: &Value) -> String {
    write_with(value, true, false)
}

fn write(value: &Value, decode_sentinels: bool) -> String {
    write_with(value, decode_sentinels, true)
}

fn write_with(value: &Value, decode_sentinels: bool, pretty: bool) -> String {
    if has_array_index_key(value) {
        let mut ordered = value.clone();
        order_keys_like_js(&mut ordered);
        return write_ordered(&ordered, decode_sentinels, pretty);
    }
    write_ordered(value, decode_sentinels, pretty)
}

/// [`write_with`] for a value whose objects are already in JS property
/// order.
fn write_ordered(value: &Value, decode_sentinels: bool, pretty: bool) -> String {
    let mut out = Vec::new();
    let formatter = JsFormatter {
        pretty: pretty.then(|| PrettyFormatter::with_indent(b"  ")),
        decode_sentinels,
    };
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    // Writing a `Value` into a Vec cannot fail: every key is a string, the
    // formatter only fails on I/O errors and a Vec never returns one.
    if value.serialize(&mut ser).is_err() {
        return String::new();
    }
    // The serializer only writes the UTF-8 of `str` fragments and ASCII.
    String::from_utf8(out).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Numbers

/// Largest integer every f64 below it represents exactly (2^53).
const MAX_SAFE: u64 = 1 << 53;

/// ECMAScript `Number::toString(x)` (ECMA-262 §6.1.6.1.20) for a finite `x`.
/// `JSON.stringify` writes non-finite numbers as `null`.
pub(crate) fn js_number(x: f64) -> String {
    if !x.is_finite() {
        return "null".to_owned();
    }
    if x == 0.0 {
        return "0".to_owned();
    }
    let (buf, len, e) = shortest_digits(x.abs());
    // ASCII digits
    let digits = std::str::from_utf8(&buf[..len]).unwrap_or_default();
    let k = len as i64;
    let n = e + 1;
    let mut out = String::with_capacity(len + 8);
    if x < 0.0 {
        out.push('-');
    }
    if k <= n && n <= 21 {
        out.push_str(digits);
        out.extend(std::iter::repeat_n('0', (n - k) as usize));
    } else if 0 < n && n <= 21 {
        let (int, frac) = digits.split_at(n as usize);
        out.push_str(int);
        out.push('.');
        out.push_str(frac);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-n) as usize));
        out.push_str(digits);
    } else {
        let (first, rest) = digits.split_at(1);
        out.push_str(first);
        if !rest.is_empty() {
            out.push('.');
            out.push_str(rest);
        }
        let e = n - 1;
        out.push('e');
        out.push(if e < 0 { '-' } else { '+' });
        out.push_str(&e.abs().to_string());
    }
    out
}

/// `parseFloat(string)` (ECMA-262 §19.2.4): the longest decimal prefix
/// after leading whitespace, `NaN` when there is none (`"20px Virgil"` is
/// 20).
pub fn parse_float(s: &str) -> f64 {
    crate::js::parse_float(s)
}

/// `Number::toString(x)` (ECMA-262 §6.1.6.1.20), non-finite values
/// included: what `String(x)` and a template literal's `${x}` write.
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        "NaN".to_owned()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else {
        js_number(x)
    }
}

/// Shortest decimal digits `d1..dk` and exponent `e` with
/// `x == d1.d2..dk × 10^e` after rounding to f64, for finite `x > 0`.
///
/// ECMA-262 step 5: among the shortest digit strings that round to `x`, take
/// the one closest to `x`; if two are equally close, the one whose last digit
/// is even. Rust's `{:e}` gives a shortest, closest string but rounds such a
/// tie up (571516643625357.25 is written ...357.3 where JS writes ...357.2),
/// so ties are detected and resolved here.
fn shortest_digits(x: f64) -> ([u8; 32], usize, i64) {
    // `{:e}` of a positive f64 is at most 23 bytes ("1.2345678901234567e-308")
    let mut sci = StackText::default();
    let _ = std::fmt::Write::write_fmt(&mut sci, format_args!("{x:e}"));
    let sci = sci.as_bytes();
    let mut buf = [0u8; 32];
    let mut len = 0;
    let mut e: i64 = 0;
    for (i, &c) in sci.iter().enumerate() {
        match c {
            b'0'..=b'9' => {
                buf[len] = c;
                len += 1;
            }
            b'e' => {
                e = std::str::from_utf8(&sci[i + 1..])
                    .ok()
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(0);
                break;
            }
            _ => {}
        }
    }
    let last = buf[len - 1] - b'0';
    if last.is_multiple_of(2) {
        return (buf, len, e);
    }
    // A tie puts x exactly on the midpoint of the digits and a neighbour
    // (one more digit, a 5), which [`equals_decimal`] decides on integers.
    // The neighbours are tried in order, the first that also rounds to x
    // deciding, as before.
    let head = &buf[..len - 1];
    let digits_of = |last: u8| -> u128 {
        let v = head
            .iter()
            .fold(0u128, |v, &d| v * 10 + u128::from(d - b'0'));
        v * 10 + u128::from(last)
    };
    let text = |last: u8| {
        let mut t = String::from_utf8_lossy(head).into_owned();
        t.push(char::from(b'0' + last));
        t
    };
    let parses_to_x = |d: &str| {
        let (first, rest) = d.split_at(1);
        format!("{first}.{rest}0e{e}").parse::<f64>() == Ok(x)
    };
    // the midpoint's last digit is worth 10^(e - k), k = len
    let p = e - len as i64;
    for neighbour in [last - 1, last + 1] {
        if neighbour > 9 || (neighbour == 0 && head.is_empty()) {
            continue;
        }
        let low = last.min(neighbour);
        let fast = equals_decimal_int(x, digits_of(low) * 10 + 5, p);
        let candidate = text(neighbour);
        if fast == Some(false) {
            // not a tie: the digits stay, whether or not this neighbour
            // rounds to x (the loop ends at the first that does)
            if parses_to_x(&candidate) {
                break;
            }
            continue;
        }
        if !parses_to_x(&candidate) {
            continue;
        }
        let midpoint = text(low) + "5";
        let tie = fast.unwrap_or_else(|| exact_expansion_is(x, &midpoint, e));
        if tie {
            let digits = candidate.as_bytes();
            buf[..digits.len()].copy_from_slice(digits);
        }
        break;
    }
    (buf, len, e)
}

/// A fixed buffer to format one number into without allocating.
#[derive(Default)]
struct StackText {
    buf: [u8; 32],
    len: usize,
}

impl StackText {
    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl std::fmt::Write for StackText {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let end = self.len + s.len();
        if end > self.buf.len() {
            return Err(std::fmt::Error);
        }
        self.buf[self.len..end].copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

/// Whether the exact decimal expansion of `x` is `digits` (`d1.d2… × 10^e`)
/// followed only by zeros. `{:.1100e}` is that expansion (an f64 has at
/// most 767 significant digits); slow, so [`equals_decimal`] answers first.
fn exact_expansion_is(x: f64, digits: &str, e: i64) -> bool {
    let exact = format!("{x:.1100e}");
    let (exact_mantissa, exact_exp) = exact.split_once('e').unwrap_or((&exact, "0"));
    let exact_digits: String = exact_mantissa.chars().filter(|c| *c != '.').collect();
    exact_exp.parse::<i64>() == Ok(e) && exact_digits.trim_end_matches('0') == digits
}

/// Whether finite `x > 0` is exactly the integer `digits` times `10^p`,
/// decided on 128-bit integers: `x = m × 2^q` against `M × 10^p`. `None`
/// when the numbers do not fit (`digits` over 18 digits, `p` below -27 or
/// above 20).
#[cfg(test)]
fn equals_decimal(x: f64, digits: &str, p: i64) -> Option<bool> {
    if digits.len() > 18 {
        return None;
    }
    equals_decimal_int(x, digits.parse().ok()?, p)
}

/// [`equals_decimal`] of the integer `big_m` (under 10^18).
fn equals_decimal_int(x: f64, big_m: u128, p: i64) -> Option<bool> {
    if big_m >= 1_000_000_000_000_000_000 || !(-27..=20).contains(&p) {
        return None;
    }
    let bits = x.to_bits();
    let biased = ((bits >> 52) & 0x7FF) as i64;
    let fraction = u128::from(bits & ((1 << 52) - 1));
    let (m, q) = if biased == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), biased - 1075)
    };
    if p < 0 {
        // x × 10^n = m × 5^n × 2^(q + n) against M, n = -p
        let n = (-p) as u32;
        let lhs = m * 5u128.pow(n);
        let s = q + i64::from(n);
        Some(if s >= 0 {
            i64::from(lhs.leading_zeros()) >= s && lhs << s == big_m
        } else {
            let t = -s;
            t < 128 && i64::from(lhs.trailing_zeros()) >= t && lhs >> t == big_m
        })
    } else {
        // M × 10^p < 10^38 < 2^127 against m × 2^q
        let rhs = big_m * 10u128.pow(p as u32);
        Some(if q >= 0 {
            q <= 74 && m << q == rhs
        } else {
            let t = -q;
            t < 128 && i64::from(m.trailing_zeros()) >= t && m >> t == rhs
        })
    }
}

// ---------------------------------------------------------------------------
// Formatter

/// `PrettyFormatter` with a two-space indent, ECMAScript number output and,
/// for [`round_trip`], lone-surrogate sentinel decoding.
struct JsFormatter {
    /// `None` writes compactly, as `JSON.stringify(value)` does.
    pretty: Option<PrettyFormatter<'static>>,
    decode_sentinels: bool,
}

impl JsFormatter {
    fn integer<W: ?Sized + Write>(
        &mut self,
        w: &mut W,
        abs: u64,
        negative: bool,
    ) -> io::Result<()> {
        if abs <= MAX_SAFE {
            if negative {
                w.write_all(b"-")?;
            }
            write!(w, "{abs}")
        } else {
            // JSON.parse rounds every number to the nearest f64; `as` rounds
            // an integer to nearest, ties to even, which is the same.
            let x = abs as f64;
            w.write_all(js_number(if negative { -x } else { x }).as_bytes())
        }
    }
}

impl Formatter for JsFormatter {
    fn write_i64<W: ?Sized + Write>(&mut self, w: &mut W, value: i64) -> io::Result<()> {
        self.integer(w, value.unsigned_abs(), value < 0)
    }

    fn write_u64<W: ?Sized + Write>(&mut self, w: &mut W, value: u64) -> io::Result<()> {
        self.integer(w, value, false)
    }

    fn write_f64<W: ?Sized + Write>(&mut self, w: &mut W, value: f64) -> io::Result<()> {
        w.write_all(js_number(value).as_bytes())
    }

    fn write_f32<W: ?Sized + Write>(&mut self, w: &mut W, value: f32) -> io::Result<()> {
        self.write_f64(w, f64::from(value))
    }

    fn write_string_fragment<W: ?Sized + Write>(
        &mut self,
        w: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        if !self.decode_sentinels || !fragment.contains(SENTINEL) {
            return w.write_all(fragment.as_bytes());
        }
        // A sentinel pair is two characters that need no escaping, so
        // serde_json never splits one across fragments.
        let mut chars = fragment.chars();
        while let Some(c) = chars.next() {
            if c != SENTINEL {
                let mut buf = [0; 4];
                w.write_all(c.encode_utf8(&mut buf).as_bytes())?;
                continue;
            }
            let mut buf = [0; 4];
            match chars.next() {
                Some(SENTINEL) | None => {
                    w.write_all(SENTINEL.encode_utf8(&mut buf).as_bytes())?;
                }
                Some(tag) if SURROGATE_TAGS.contains(&u32::from(tag)) => {
                    let unit = u32::from(tag) - SURROGATE_TAG_BASE + 0xD800;
                    write!(w, "\\u{unit:04x}")?;
                }
                // Not a sentinel pair; the sentinel form never has one (every
                // U+FDD0 from `parse` or `escape` is paired). Written as is.
                Some(other) => {
                    w.write_all(SENTINEL.encode_utf8(&mut buf).as_bytes())?;
                    w.write_all(other.encode_utf8(&mut buf).as_bytes())?;
                }
            }
        }
        Ok(())
    }

    fn begin_array<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        match &mut self.pretty {
            Some(pretty) => pretty.begin_array(w),
            None => CompactFormatter.begin_array(w),
        }
    }

    fn end_array<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        match &mut self.pretty {
            Some(pretty) => pretty.end_array(w),
            None => CompactFormatter.end_array(w),
        }
    }

    fn begin_array_value<W: ?Sized + Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        match &mut self.pretty {
            Some(pretty) => pretty.begin_array_value(w, first),
            None => CompactFormatter.begin_array_value(w, first),
        }
    }

    fn end_array_value<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        match &mut self.pretty {
            Some(pretty) => pretty.end_array_value(w),
            None => CompactFormatter.end_array_value(w),
        }
    }

    fn begin_object<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        match &mut self.pretty {
            Some(pretty) => pretty.begin_object(w),
            None => CompactFormatter.begin_object(w),
        }
    }

    fn end_object<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        match &mut self.pretty {
            Some(pretty) => pretty.end_object(w),
            None => CompactFormatter.end_object(w),
        }
    }

    fn begin_object_key<W: ?Sized + Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        match &mut self.pretty {
            Some(pretty) => pretty.begin_object_key(w, first),
            None => CompactFormatter.begin_object_key(w, first),
        }
    }

    fn begin_object_value<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        match &mut self.pretty {
            Some(pretty) => pretty.begin_object_value(w),
            None => CompactFormatter.begin_object_value(w),
        }
    }

    fn end_object_value<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        match &mut self.pretty {
            Some(pretty) => pretty.end_object_value(w),
            None => CompactFormatter.end_object_value(w),
        }
    }
}

// ---------------------------------------------------------------------------
// Lone surrogates

/// Noncharacter that introduces a sentinel pair.
const SENTINEL: char = '\u{FDD0}';
/// UTF-8 of [`SENTINEL`].
const SENTINEL_UTF8: &[u8] = "\u{FDD0}".as_bytes();
/// Second character of a sentinel pair is this plus (unit - 0xD800), so the
/// 2048 surrogate code units map onto U+E000..=U+E7FF.
const SURROGATE_TAG_BASE: u32 = 0xE000;
/// The second characters of sentinel pairs that stand for a surrogate.
const SURROGATE_TAGS: std::ops::RangeInclusive<u32> = 0xE000..=0xE7FF;

fn hex4(bytes: &[u8]) -> Option<u32> {
    let s = std::str::from_utf8(bytes.get(..4)?).ok()?;
    if !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

fn push_sentinel(out: &mut String, second: char) {
    out.push(SENTINEL);
    out.push(second);
}

/// Rewrite, inside JSON strings only, every lone surrogate escape to a
/// sentinel pair and every literal U+FDD0 (raw or escaped) to a doubled
/// U+FDD0. Everything else is copied unchanged; malformed input is left for
/// serde_json to reject.
fn encode_lone_surrogates(text: &str) -> String {
    if !text.contains("\\u") && !text.contains(SENTINEL) {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut i = 0;
    // Copy runs of text between the positions we act on.
    let mut copied = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if !in_string {
            if b == b'"' {
                in_string = true;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                in_string = false;
                i += 1;
            }
            b'\\' if bytes.get(i + 1) == Some(&b'u') => {
                let Some(unit) = hex4(&bytes[i + 2..]) else {
                    i += 2;
                    continue;
                };
                let replacement = if (0xD800..0xDC00).contains(&unit) {
                    let low = (bytes.get(i + 6) == Some(&b'\\') && bytes.get(i + 7) == Some(&b'u'))
                        .then(|| hex4(&bytes[i + 8..]))
                        .flatten();
                    if low.is_some_and(|l| (0xDC00..0xE000).contains(&l)) {
                        // A valid pair: leave both escapes to serde_json.
                        i += 12;
                        continue;
                    }
                    Some(unit)
                } else if (0xDC00..0xE000).contains(&unit) {
                    Some(unit)
                } else if unit == u32::from(SENTINEL) {
                    None
                } else {
                    i += 6;
                    continue;
                };
                out.push_str(&text[copied..i]);
                match replacement {
                    Some(unit) => push_sentinel(
                        &mut out,
                        char::from_u32(unit - 0xD800 + SURROGATE_TAG_BASE).unwrap_or(SENTINEL),
                    ),
                    None => push_sentinel(&mut out, SENTINEL),
                }
                i += 6;
                copied = i;
            }
            b'\\' => i += 2,
            _ if bytes[i..].starts_with(SENTINEL_UTF8) => {
                out.push_str(&text[copied..i]);
                push_sentinel(&mut out, SENTINEL);
                i += SENTINEL_UTF8.len();
                copied = i;
            }
            _ => i += 1,
        }
    }
    out.push_str(&text[copied.min(text.len())..]);
    out
}

/// A public string in the sentinel form: every U+FDD0 doubled. Borrowed
/// when there is none.
pub(crate) fn escape_str(s: &str) -> Cow<'_, str> {
    if !s.contains(SENTINEL) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + SENTINEL_UTF8.len());
    for c in s.chars() {
        out.push(c);
        if c == SENTINEL {
            out.push(SENTINEL);
        }
    }
    Cow::Owned(out)
}

/// The UTF-16 code units of the JavaScript string a sentinel-form string
/// stands for: a sentinel pair is its lone surrogate, a doubled U+FDD0 one
/// U+FDD0. What `charCodeAt` sees in the string `JSON.parse` produced.
pub(crate) fn utf16_units(s: &str) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len());
    let mut buf = [0u16; 2];
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == SENTINEL {
            match chars.next() {
                Some(tag) if SURROGATE_TAGS.contains(&u32::from(tag)) => {
                    // A surrogate code unit, always below 0x10000.
                    out.push((u32::from(tag) - SURROGATE_TAG_BASE + 0xD800) as u16);
                }
                Some(SENTINEL) | None => out.push(SENTINEL as u16),
                Some(other) => {
                    out.push(SENTINEL as u16);
                    out.extend_from_slice(other.encode_utf16(&mut buf));
                }
            }
            continue;
        }
        out.extend_from_slice(c.encode_utf16(&mut buf));
    }
    out
}

/// The public string for a sentinel-form one: a sentinel pair for a lone
/// surrogate becomes U+FFFD (what a lossy UTF-16 decode gives), a doubled
/// U+FDD0 one U+FDD0. Borrowed when there is no sentinel.
pub(crate) fn decode_str(s: &str) -> Cow<'_, str> {
    if !s.contains(SENTINEL) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != SENTINEL {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(tag) if SURROGATE_TAGS.contains(&u32::from(tag)) => {
                out.push(char::REPLACEMENT_CHARACTER);
            }
            Some(SENTINEL) | None => out.push(SENTINEL),
            Some(other) => {
                out.push(SENTINEL);
                out.push(other);
            }
        }
    }
    Cow::Owned(out)
}

/// The UTF-16 code units of the JS string a sentinel-form string stands
/// for: a sentinel pair gives its lone surrogate, a doubled U+FDD0 one
/// U+FDD0. String functions ported from JS (`trim`, regex replaces,
/// `String.fromCharCode`) work on these units.
pub(crate) fn to_utf16(s: &str) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        let c = if c == SENTINEL {
            match chars.next() {
                Some(tag) if SURROGATE_TAGS.contains(&u32::from(tag)) => {
                    out.push((u32::from(tag) - SURROGATE_TAG_BASE + 0xD800) as u16);
                    continue;
                }
                Some(SENTINEL) | None => SENTINEL,
                Some(other) => {
                    out.push(SENTINEL as u16);
                    other
                }
            }
        } else {
            c
        };
        let mut buf = [0u16; 2];
        out.extend_from_slice(c.encode_utf16(&mut buf));
    }
    out
}

/// The sentinel-form string for JS string code units: a lone surrogate
/// becomes a sentinel pair, U+FDD0 is doubled. Inverse of [`to_utf16`].
pub(crate) fn from_utf16(units: &[u16]) -> String {
    let mut out = String::with_capacity(units.len());
    for unit in char::decode_utf16(units.iter().copied()) {
        match unit {
            Ok(SENTINEL) => push_sentinel(&mut out, SENTINEL),
            Ok(c) => out.push(c),
            Err(e) => {
                let tag = u32::from(e.unpaired_surrogate()) - 0xD800 + SURROGATE_TAG_BASE;
                push_sentinel(&mut out, char::from_u32(tag).unwrap_or(SENTINEL));
            }
        }
    }
    out
}

/// `value` with every string and key mapped by `f`.
fn map_strings(value: &Value, f: fn(&str) -> Cow<'_, str>) -> Value {
    match value {
        Value::String(s) => Value::String(f(s).into_owned()),
        Value::Array(items) => Value::Array(items.iter().map(|v| map_strings(v, f)).collect()),
        Value::Object(map) => Value::Object(map_object(map, f)),
        other => other.clone(),
    }
}

fn map_object(map: &Map<String, Value>, f: fn(&str) -> Cow<'_, str>) -> Map<String, Value> {
    map.iter()
        .map(|(k, v)| (f(k).into_owned(), map_strings(v, f)))
        .collect()
}

/// A public value in the sentinel form ([`escape_str`] on every string and
/// key).
pub(crate) fn escape(value: &Value) -> Value {
    map_strings(value, escape_str)
}

/// [`escape`] for an object.
pub(crate) fn escape_map(map: &Map<String, Value>) -> Map<String, Value> {
    map_object(map, escape_str)
}

/// The public value for a sentinel-form one ([`decode_str`] on every string
/// and key). Two keys that decode alike (two different lone surrogates)
/// keep the first position and the last value, as a duplicated key does.
pub(crate) fn decode(value: &Value) -> Value {
    map_strings(value, decode_str)
}

/// [`decode`] for an object.
pub(crate) fn decode_map(map: &Map<String, Value>) -> Map<String, Value> {
    map_object(map, decode_str)
}

// ---------------------------------------------------------------------------
// Key order

/// True when `key` is a canonical array index: the decimal form of an
/// integer in 0..=2^32-2, as ECMAScript orders such keys first.
pub(crate) fn is_array_index(key: &str) -> bool {
    if key.is_empty() || key.len() > 10 || !key.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if key.len() > 1 && key.starts_with('0') {
        return false;
    }
    key.parse::<u64>().is_ok_and(|n| n < u64::from(u32::MAX))
}

/// `map` with its keys, and those of every nested object, in JS property
/// order (see [`order_keys_like_js`]).
pub(crate) fn ordered_like_js(map: Map<String, Value>) -> Map<String, Value> {
    let mut value = Value::Object(map);
    order_keys_like_js(&mut value);
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// True when an object in `value` has an array-index key, i.e. when
/// [`order_keys_like_js`] may change it.
fn has_array_index_key(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(has_array_index_key),
        Value::Object(map) => {
            map.keys().any(|k| is_array_index(k)) || map.values().any(has_array_index_key)
        }
        _ => false,
    }
}

/// Reorder every object's keys the way a JS object enumerates them
/// (ECMA-262 `OrdinaryOwnPropertyKeys`): array indices ascending, then
/// strings in insertion order. `JSON.parse` and property assignment both
/// give that order, so it applies to every object written.
pub(crate) fn order_keys_like_js(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(order_keys_like_js),
        Value::Object(map) => {
            if map.keys().any(|k| is_array_index(k)) {
                let entries = std::mem::take(map);
                let (mut indices, strings): (Vec<_>, Vec<_>) =
                    entries.into_iter().partition(|(k, _)| is_array_index(k));
                indices.sort_by_key(|(k, _)| k.parse::<u64>().unwrap_or(0));
                map.extend(indices);
                map.extend(strings);
            }
            map.values_mut().for_each(order_keys_like_js);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_containers_are_compact_like_json_stringify() {
        // JSON.stringify({a: [], b: {}}, null, 2) === '{\n  "a": [],\n  "b": {}\n}'
        assert_eq!(
            round_trip(r#"{"a":[],"b":{}}"#).unwrap(),
            "{\n  \"a\": [],\n  \"b\": {}\n}"
        );
    }

    #[test]
    fn compact_output_is_json_stringify_without_indent() {
        // JSON.stringify(JSON.parse('{"b":[1.50,{}],"2":-0,"a":1e21,"s":"\ud83d"}'))
        // === '{"2":0,"b":[1.5,{}],"a":1e+21,"s":"\\ud83d"}'
        let parsed = parse(r#"{"b":[1.50,{}],"2":-0,"a":1e21,"s":"\ud83d"}"#).unwrap();
        assert_eq!(
            write_parsed_compact(&parsed),
            r#"{"2":0,"b":[1.5,{}],"a":1e+21,"s":"\ud83d"}"#
        );
        let value: Value = serde_json::json!({"x": [1, 2.5], "7": "y"});
        assert_eq!(to_string_compact(&value), r#"{"7":"y","x":[1,2.5]}"#);
    }

    #[test]
    fn nested_arrays_indent_two_spaces_per_level() {
        // JSON.stringify({p: [[0, 0], [10, 5]]}, null, 2)
        assert_eq!(
            round_trip(r#"{"p":[[0,0],[10,5]]}"#).unwrap(),
            "{\n  \"p\": [\n    [\n      0,\n      0\n    ],\n    [\n      10,\n      5\n    ]\n  ]\n}"
        );
    }

    #[test]
    fn non_ascii_is_written_raw_not_escaped() {
        // JSON.stringify("日本") === '"日本"'
        assert_eq!(round_trip(r#""日本""#).unwrap(), "\"日本\"");
    }
    // Every expected string below was produced by node v26.10.0 with
    // `JSON.stringify(JSON.parse(input), null, 2)` (or without the indent
    // arguments where the input is a scalar); JSON.stringify implements
    // ECMAScript Number::toString for numbers and well-formed JSON.stringify
    // (lone surrogates written as `\uXXXX` escapes) for strings.

    /// `(input, node output)` pairs for scalar numbers.
    const NUMBERS: &[(&str, &str)] = &[
        ("0.000001", "0.000001"),
        ("1e-6", "0.000001"),
        ("1.5e-7", "1.5e-7"),
        ("1e-7", "1e-7"),
        ("0.0000001234", "1.234e-7"),
        ("-2e-7", "-2e-7"),
        ("1e21", "1e+21"),
        ("1.25e+21", "1.25e+21"),
        ("999999999999999999999", "1e+21"),
        ("1e20", "100000000000000000000"),
        ("1.0", "1"),
        ("1E2", "100"),
        ("100", "100"),
        ("-0", "0"),
        ("-0.0", "0"),
        ("0.1", "0.1"),
        ("-1.5", "-1.5"),
        ("2.5E-3", "0.0025"),
        ("3.14159", "3.14159"),
        ("123456.789", "123456.789"),
        ("0.30000000000000004", "0.30000000000000004"),
        ("123e-20", "1.23e-18"),
        ("5e-324", "5e-324"),
        ("1.7976931348623157e308", "1.7976931348623157e+308"),
        ("123456789012345678901234", "1.2345678901234569e+23"),
        ("12345678901234567890", "12345678901234567000"),
        ("9007199254740993", "9007199254740992"),
        ("-9223372036854775808", "-9223372036854776000"),
        // Two shortest strings equally close to the value: ECMA-262 takes
        // the one with the even last digit.
        ("571516643625357.25", "571516643625357.2"),
        ("82075870703310.125", "82075870703310.12"),
        ("2806231691801.40625", "2806231691801.4062"),
    ];

    /// The ECMAScript formatting as first ported: `{:e}`, a tie decided on
    /// the exact expansion. [`js_number`] must write the same.
    fn reference_js_number(x: f64) -> String {
        if !x.is_finite() {
            return "null".to_owned();
        }
        if x == 0.0 {
            return "0".to_owned();
        }
        let sign = if x < 0.0 { "-" } else { "" };
        let a = x.abs();
        let sci = format!("{a:e}");
        let (mantissa, exp) = sci.split_once('e').unwrap();
        let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
        let e = exp.parse::<i64>().unwrap();
        let last = digits.bytes().last().unwrap() - b'0';
        if last % 2 == 1 {
            let parses_to_x = |d: &str| {
                let (first, rest) = d.split_at(1);
                format!("{first}.{rest}0e{e}").parse::<f64>() == Ok(a)
            };
            let head = digits[..digits.len() - 1].to_owned();
            for neighbour in [last - 1, last + 1] {
                if neighbour > 9 || (neighbour == 0 && head.is_empty()) {
                    continue;
                }
                let candidate = format!("{head}{neighbour}");
                if !parses_to_x(&candidate) {
                    continue;
                }
                let midpoint = format!("{head}{}5", last.min(neighbour));
                if exact_expansion_is(a, &midpoint, e) {
                    digits = candidate;
                }
                break;
            }
        }
        let k = digits.len() as i64;
        let n = e + 1;
        let body = if k <= n && n <= 21 {
            format!("{digits}{}", "0".repeat((n - k) as usize))
        } else if 0 < n && n <= 21 {
            let (int, frac) = digits.split_at(n as usize);
            format!("{int}.{frac}")
        } else if -6 < n && n <= 0 {
            format!("0.{}{digits}", "0".repeat((-n) as usize))
        } else {
            let (first, rest) = digits.split_at(1);
            let dot = if rest.is_empty() { "" } else { "." };
            let e = n - 1;
            let e_sign = if e < 0 { '-' } else { '+' };
            format!("{first}{dot}{rest}e{e_sign}{}", e.abs())
        };
        format!("{sign}{body}")
    }

    #[test]
    fn js_number_writes_what_the_reference_writes() {
        let mut values: Vec<f64> = NUMBERS
            .iter()
            .filter_map(|(input, _)| input.parse::<f64>().ok())
            .collect();
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..4_000 {
            values.push(f64::from_bits(next() & 0xFFEF_FFFF_FFFF_FFFF));
            values.push((next() % 4_000_000) as f64 / 8.0 - 250_000.0);
            values.push((next() >> 11) as f64 / (1u64 << 53) as f64 * 600.0 - 300.0);
        }
        // exact dyadic midpoints of short decimals, and their neighbours
        for j in 1..30 {
            for n in [1u64, 3, 7, 101, 12_345, 987_654_321] {
                let x = (2 * n + 1) as f64 / (1u64 << j) as f64;
                values.extend([
                    x,
                    -x,
                    f64::from_bits(x.to_bits() + 1),
                    f64::from_bits(x.to_bits() - 1),
                ]);
            }
        }
        for x in values {
            assert_eq!(js_number(x), reference_js_number(x), "{x:e}");
        }
    }

    /// The tie check on integers agrees with the exact decimal expansion
    /// (`{:.1100e}`) wherever it answers: the ties above, their
    /// neighbours, and a spread of doubles from subnormal to huge,
    /// full-precision coordinates like a freedraw outline's included.
    #[test]
    fn the_integer_tie_check_agrees_with_the_exact_expansion() {
        let mut values: Vec<f64> = NUMBERS
            .iter()
            .filter_map(|(input, _)| input.parse::<f64>().ok())
            .filter(|x| x.is_finite() && *x > 0.0)
            .flat_map(|x| {
                [
                    x,
                    f64::from_bits(x.to_bits() - 1),
                    f64::from_bits(x.to_bits() + 1),
                ]
            })
            .collect();
        // xorshift: every exponent, and coordinates of a drawing
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..3_000 {
            let bits = next() & 0x7FEF_FFFF_FFFF_FFFF;
            values.push(f64::from_bits(bits.max(1)));
            values.push((next() % 2_000_000) as f64 / 1024.0 + (next() % 1000) as f64 * 1e-13);
            values.push((next() >> 11) as f64 / (1u64 << 53) as f64 * 300.0);
        }
        // exact halves at many scales: 0.5, 1.25, 2.375, ... x 10^k
        for k in -30..30 {
            for m in [
                5u64,
                25,
                125,
                375,
                1_234_567_890_125,
                57_151_664_362_535_725,
            ] {
                values.push(m as f64 * 10f64.powi(k));
            }
        }
        let (mut answered, mut ties) = (0, 0);
        for x in values.into_iter().filter(|x| x.is_finite() && *x > 0.0) {
            let sci = format!("{x:e}");
            let (mantissa, exp) = sci.split_once('e').unwrap();
            let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
            let e = exp.parse::<i64>().unwrap();
            for last in 0..10u8 {
                let head = &digits[..digits.len() - 1];
                if head.is_empty() && last == 0 {
                    continue;
                }
                let midpoint = format!("{head}{last}5");
                let slow = exact_expansion_is(x, &midpoint, e);
                if let Some(fast) = equals_decimal(x, &midpoint, e - digits.len() as i64) {
                    assert_eq!(fast, slow, "{x:e} against {midpoint}e{e}");
                    answered += 1;
                    ties += usize::from(fast);
                }
            }
        }
        // dyadic values against their own exact expansion and its neighbours
        for j in 0..40 {
            for n in [1u64, 3, 5, 77, 1_025, 999_999, 123_456_789] {
                let x = n as f64 / (1u64 << j) as f64 * if j % 3 == 0 { 1e6 } else { 1.0 };
                let exact = format!("{x:.1100e}");
                let (mantissa, exp) = exact.split_once('e').unwrap();
                let own: String = mantissa.chars().filter(|c| *c != '.').collect();
                let own = own.trim_end_matches('0');
                let e = exp.parse::<i64>().unwrap();
                let p = e - (own.len() as i64 - 1);
                let as_int: u128 = match own.parse() {
                    Ok(v) if own.len() <= 18 => v,
                    _ => continue,
                };
                for delta in [0i128, -1, 1] {
                    let digits = (as_int as i128 + delta).to_string();
                    if digits.len() != own.len() {
                        continue;
                    }
                    if let Some(fast) = equals_decimal(x, &digits, p) {
                        assert_eq!(
                            fast,
                            exact_expansion_is(x, &digits, e),
                            "{x:e} against {digits}e{p}"
                        );
                        assert_eq!(fast, delta == 0, "{x:e} against {digits}e{p}");
                        answered += 1;
                        ties += usize::from(fast);
                    }
                }
            }
        }
        assert!(
            answered > 50_000,
            "the integer check answered only {answered} times"
        );
        assert!(ties >= 100, "only {ties} ties");
    }

    #[test]
    fn numbers_are_written_as_ecmascript_number_to_string() {
        for (input, node) in NUMBERS {
            assert_eq!(round_trip(input).unwrap(), *node, "input {input}");
        }
    }

    #[test]
    fn numbers_inside_a_document_match_node() {
        assert_eq!(
            round_trip(r#"{"p":[1.0,-0,0.000001,1.5e-7,1e21,123456789012345678901234]}"#)
                .unwrap(),
            "{\n  \"p\": [\n    1,\n    0,\n    0.000001,\n    1.5e-7,\n    1e+21,\n    1.2345678901234569e+23\n  ]\n}"
        );
    }

    // JSON.stringify(JSON.parse(text)) in Node 22 for each input.
    #[test]
    fn numbers_beyond_f64_range_are_written_as_null_like_node() {
        let digits = format!("1{}", "0".repeat(400));
        let text = format!(
            r#"{{"a":1e400,"b":-1E+400,"c":[{digits},-0.5e309],"d":1e-400,"e":1.7976931348623157e308,"s":"1e400 \" 1e400"}}"#
        );
        assert_eq!(
            write_parsed_compact(&parse(&text).unwrap()),
            r#"{"a":null,"b":null,"c":[null,null],"d":0,"e":1.7976931348623157e+308,"s":"1e400 \" 1e400"}"#
        );
        assert_eq!(round_trip("1e999").unwrap(), "null");
    }

    #[test]
    fn overflowing_literal_with_invalid_syntax_is_still_an_error() {
        for text in [
            "[01e400]",
            "[1e400.5]",
            "[1.e400]",
            "[1e400",
            "[+1e400]",
            "[1e400 1]",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn to_string_pretty_formats_f64_values_like_node() {
        let value = serde_json::json!([1.0, -0.0, 0.000001, 1e21]);
        assert_eq!(
            to_string_pretty(&value),
            "[\n  1,\n  0,\n  0.000001,\n  1e+21\n]"
        );
    }

    #[test]
    fn lone_surrogate_escape_is_kept() {
        assert_eq!(
            round_trip(r#"{"x":"\ud800"}"#).unwrap(),
            "{\n  \"x\": \"\\ud800\"\n}"
        );
    }

    #[test]
    fn split_emoji_keeps_lone_halves_and_decodes_the_pair() {
        assert_eq!(
            round_trip(r#"{"x":"a\udc00b\ud83d\ude00c\ud83d"}"#).unwrap(),
            "{\n  \"x\": \"a\\udc00b\u{1F600}c\\ud83d\"\n}"
        );
    }

    #[test]
    fn lone_surrogates_in_keys_and_repeated_are_kept() {
        assert_eq!(
            round_trip(r#"{"\ud800k":"\ud800\ud800"}"#).unwrap(),
            "{\n  \"\\ud800k\": \"\\ud800\\ud800\"\n}"
        );
    }

    #[test]
    fn noncharacter_next_to_lone_surrogate_is_written_raw() {
        assert_eq!(
            round_trip(r#"{"x":"\ufdd0\ud800"}"#).unwrap(),
            "{\n  \"x\": \"\u{FDD0}\\ud800\"\n}"
        );
        assert_eq!(
            round_trip("[\"\u{FDD0}\u{E000}\"]").unwrap(),
            "[\n  \"\u{FDD0}\u{E000}\"\n]"
        );
    }

    #[test]
    fn escaped_backslash_before_u_is_not_a_surrogate_escape() {
        assert_eq!(
            round_trip(r#"["\\ud800","\"\ud800"]"#).unwrap(),
            "[\n  \"\\\\ud800\",\n  \"\\\"\\ud800\"\n]"
        );
    }

    #[test]
    fn string_escapes_match_node() {
        assert_eq!(
            round_trip(r#""\u0000\b\t\n\u000b\f\r\u001f\"\\/\u007f\u2028\u00e9""#).unwrap(),
            "\"\\u0000\\b\\t\\n\\u000b\\f\\r\\u001f\\\"\\\\/\u{7f}\u{2028}\u{e9}\""
        );
    }

    #[test]
    fn array_index_keys_come_first_like_a_js_object() {
        assert_eq!(
            round_trip(
                r#"{"b":1,"10":2,"2":3,"01":4,"4294967295":5,"4294967294":6,"-1":7,"a":{"z":0,"0":1}}"#
            )
            .unwrap(),
            "{\n  \"2\": 3,\n  \"10\": 2,\n  \"4294967294\": 6,\n  \"b\": 1,\n  \"01\": 4,\n  \"4294967295\": 5,\n  \"-1\": 7,\n  \"a\": {\n    \"0\": 1,\n    \"z\": 0\n  }\n}"
        );
    }

    #[test]
    fn written_objects_put_array_index_keys_first_like_js() {
        // JSON.stringify({b: 1, "1": 2, c: {z: 0, "10": 1, "2": 2}}, null, 2)
        let value = serde_json::json!({"b": 1, "1": 2, "c": {"z": 0, "10": 1, "2": 2}});
        let expected =
            "{\n  \"1\": 2,\n  \"b\": 1,\n  \"c\": {\n    \"2\": 2,\n    \"10\": 1,\n    \"z\": 0\n  }\n}";
        assert_eq!(to_string_pretty(&value), expected);
        assert_eq!(write_parsed(&value), expected);
    }

    #[test]
    fn duplicate_key_keeps_first_position_and_last_value() {
        assert_eq!(
            round_trip(r#"{"a":1,"a":2,"b":3}"#).unwrap(),
            "{\n  \"a\": 2,\n  \"b\": 3\n}"
        );
    }

    #[test]
    fn escape_and_decode_are_inverse_on_public_strings() {
        for s in [
            "",
            "plain",
            "\u{FDD0}",
            "\u{FDD0}\u{E000}",
            "\u{FDD0}\u{FDD0}",
            "a\u{FDD0}\u{E7FF}\u{FDD0}",
            "\u{FFFD}",
        ] {
            assert_eq!(decode_str(&escape_str(s)), s, "{s:?}");
            let escaped = Value::from(escape_str(s).as_ref());
            assert_eq!(escape(&decode(&escaped)), escaped, "{s:?}");
        }
    }

    #[test]
    fn parsed_sentinels_decode_to_public_strings() {
        let parsed = parse(r#"{"\ud800k":["a\ud83d","﷐\ud800","﷐"]}"#).unwrap();
        assert_ne!(escape(&decode(&parsed)), parsed);
        assert_eq!(
            decode(&parsed),
            serde_json::json!({"\u{FFFD}k": ["a\u{FFFD}", "\u{FDD0}\u{FFFD}", "\u{FDD0}\u{E000}"]})
        );
        let literal = parse("[\"\u{FDD0}\u{E000}\"]").unwrap();
        assert_eq!(escape(&decode(&literal)), literal);
    }

    #[test]
    fn utf16_units_of_sentinel_strings() {
        // JSON.parse('"a\\ud83d\\ud83d\\ude00\\udc00\\ufdd0"') has code units
        // 61 d83d d83d de00 dc00 fdd0.
        let parsed = parse(r#""a\ud83d\ud83d\ude00\udc00\ufdd0""#).unwrap();
        let s = parsed.as_str().unwrap();
        let units = to_utf16(s);
        assert_eq!(units, [0x61, 0xD83D, 0xD83D, 0xDE00, 0xDC00, 0xFDD0]);
        assert_eq!(from_utf16(&units), s);
        assert_eq!(to_utf16(&escape_str("\u{FDD0}x")), [0xFDD0, 0x78]);
        assert_eq!(from_utf16(&[0xD83D, 0xDE00]), "\u{1F600}");
        // A stray sentinel (not from parse) is kept as itself.
        assert_eq!(to_utf16("\u{FDD0}x"), [0xFDD0, 0x78]);
    }

    #[test]
    fn escaped_public_strings_are_written_as_they_are() {
        let value = serde_json::json!(["\u{FDD0}\u{E000}", "\u{FDD0}\u{FDD0}", "\u{FDD0}"]);
        assert_eq!(write_parsed(&escape(&value)), to_string_pretty(&value));
    }
}
