//! Order keys: a port of the vendored `@excalidraw/fractional-indexing`
//! (`packages/fractional-indexing/src/index.ts`, itself vendored from
//! `rocicorp/fractional-indexing`, CC0; see
//! `site/content/research/data-model.md`, section 6).
//!
//! A key is an integer part (head character plus a head-dependent number of
//! digits) followed by an optional fraction with no trailing zero. Keys
//! compare as plain JS strings, that is by UTF-16 code unit
//! ([`compare_js_strings`]).
//!
//! The port reproduces upstream's exact key strings and error messages.
//! Like upstream, it works on UTF-16 code units, so a custom digit alphabet
//! behaves as it does in JS.
//!
//! The `fractional_index` crate (2.0.2) was evaluated and not used. Its keys
//! are byte strings ending in a 0x80 terminator and written as lowercase hex
//! (`src/fract_index.rs:12, 29-31, 131-132`: the first key is `"80"`, the
//! README's example `"817f80"`), so it can neither read upstream's base-62
//! keys (`"a0"`, `"a0V"`) nor produce them.
//!
//! Upstream throws `Error`; here every function returns [`OrderKeyError`]
//! carrying the same message.

use std::cmp::Ordering;
use std::fmt;

/// The base-62 alphabet, in ascending code-unit order (`index.ts:5-6`).
pub const BASE_62_DIGITS: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// An error upstream throws, with upstream's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderKeyError {
    message: String,
}

impl OrderKeyError {
    fn new(message: String) -> OrderKeyError {
        OrderKeyError { message }
    }

    /// Upstream's `Error.message`.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for OrderKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OrderKeyError {}

type Result<T> = std::result::Result<T, OrderKeyError>;

/// Compare two strings as JS relational operators do: by UTF-16 code unit.
/// This differs from Rust's `str` order (UTF-8 bytes, i.e. code points) only
/// for characters above U+FFFF against U+E000..=U+FFFF.
pub fn compare_js_strings(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        a.as_bytes().cmp(b.as_bytes())
    } else {
        a.encode_utf16().cmp(b.encode_utf16())
    }
}

// ---------------------------------------------------------------------------
// UTF-16 helpers: upstream indexes strings by code unit.

type Units = Vec<u16>;

fn units(s: &str) -> Units {
    s.encode_utf16().collect()
}

fn text(u: &[u16]) -> String {
    String::from_utf16_lossy(u)
}

const fn unit(c: char) -> u16 {
    c as u16
}

/// `String(undefined)`: what JS yields when indexing past the end.
const UNDEFINED: &str = "undefined";

/// `digits.indexOf(ch)`; -1 when absent.
fn index_of(digits: &[u16], ch: Option<u16>) -> i64 {
    ch.and_then(|c| digits.iter().position(|&d| d == c))
        .map_or(-1, |p| p as i64)
}

/// `digits[i]` as a string; `"undefined"` when out of range.
fn digit_at(digits: &[u16], i: i64) -> Units {
    usize::try_from(i)
        .ok()
        .and_then(|i| digits.get(i))
        .map_or_else(|| units(UNDEFINED), |&d| vec![d])
}

/// `s.slice(n)` for `n >= 0`.
fn slice_from(s: &[u16], n: usize) -> &[u16] {
    &s[n.min(s.len())..]
}

// ---------------------------------------------------------------------------
// index.ts

/// `midpoint(a, b, digits)` (`index.ts:19-62`). `a` may be empty; `b` is
/// `None` or, in every reachable call, non-empty; `a < b` when `b` is set.
fn midpoint(a: &[u16], b: Option<&[u16]>, digits: &[u16]) -> Result<Units> {
    let zero = digits[0];
    if let Some(b) = b {
        if a >= b {
            return Err(OrderKeyError::new(format!("{} >= {}", text(a), text(b))));
        }
    }
    if a.last() == Some(&zero) || b.is_some_and(|b| b.last() == Some(&zero)) {
        return Err(OrderKeyError::new("trailing zero".to_owned()));
    }
    if let Some(b) = b.filter(|b| !b.is_empty()) {
        // remove longest common prefix. pad `a` with 0s as we go. note that
        // we don't need to pad `b`, because it can't end before `a` while
        // traversing the common prefix.
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(zero) == b[n] {
            n += 1;
        }
        if n > 0 {
            let mut out = b[..n].to_vec();
            out.extend(midpoint(slice_from(a, n), Some(&b[n..]), digits)?);
            return Ok(out);
        }
    }
    // first digits (or lack of digit) are different
    let digit_a = if a.is_empty() {
        0
    } else {
        index_of(digits, a.first().copied())
    };
    let digit_b = match b {
        Some(b) => index_of(digits, b.first().copied()),
        None => digits.len() as i64,
    };
    if digit_b - digit_a > 1 {
        // Math.round(0.5 * (digitA + digitB)): halves round up
        let mid = (digit_a + digit_b + 1).div_euclid(2);
        return Ok(digit_at(digits, mid));
    }
    // first digits are consecutive
    if let Some(b) = b {
        if b.len() > 1 {
            return Ok(b[..1].to_vec());
        }
    }
    // `b` is null or has length 1 (a single digit). the first digit of `a`
    // is the previous digit to `b`, or 9 if `b` is null. given, for
    // example, midpoint('49', '5'), return '4' + midpoint('9', null), which
    // will become '4' + '9' + midpoint('', null), which is '495'
    let mut out = digit_at(digits, digit_a);
    out.extend(midpoint(slice_from(a, 1), None, digits)?);
    Ok(out)
}

/// `validateInteger` (`index.ts:69-73`).
fn validate_integer(int: &[u16]) -> Result<()> {
    if int.len() != integer_length(int.first().copied())? {
        return Err(OrderKeyError::new(format!(
            "invalid integer part of order key: {}",
            text(int)
        )));
    }
    Ok(())
}

/// `getIntegerLength` (`index.ts:80-88`): `a`..`z` give 2..27, `A`..`Z`
/// give 27..2.
fn integer_length(head: Option<u16>) -> Result<usize> {
    match head {
        Some(h) if (unit('a')..=unit('z')).contains(&h) => Ok(usize::from(h - unit('a')) + 2),
        Some(h) if (unit('A')..=unit('Z')).contains(&h) => Ok(usize::from(unit('Z') - h) + 2),
        Some(h) => Err(OrderKeyError::new(format!(
            "invalid order key head: {}",
            text(&[h])
        ))),
        None => Err(OrderKeyError::new(format!(
            "invalid order key head: {UNDEFINED}"
        ))),
    }
}

/// `getIntegerPart` (`index.ts:95-102`).
fn integer_part(key: &[u16]) -> Result<&[u16]> {
    let len = integer_length(key.first().copied())?;
    if len > key.len() {
        return Err(OrderKeyError::new(format!(
            "invalid order key: {}",
            text(key)
        )));
    }
    Ok(&key[..len])
}

/// `"A" + digits[0].repeat(26)`: the smallest integer, not a valid key.
fn smallest_integer(digits: &[u16]) -> Units {
    let mut s = vec![unit('A')];
    s.extend(std::iter::repeat_n(digits[0], 26));
    s
}

fn validate_units(key: &[u16], digits: &[u16]) -> Result<()> {
    let valid_chars = key.iter().all(|c| digits.contains(c));
    if key == smallest_integer(digits).as_slice() || !valid_chars {
        return Err(OrderKeyError::new(format!(
            "invalid order key: {}",
            text(key)
        )));
    }
    // integer_part fails if the first character is bad, or the key is too
    // short
    let i = integer_part(key)?;
    let f = &key[i.len()..];
    if f.last() == Some(&digits[0]) {
        return Err(OrderKeyError::new(format!(
            "invalid order key: {}",
            text(key)
        )));
    }
    Ok(())
}

/// `validateOrderKey(key)` with the base-62 alphabet (`index.ts:109-125`):
/// every character is a digit, the key is not the smallest integer
/// `"A" + "0" * 26`, its integer part is complete, and its fraction has no
/// trailing zero.
pub fn validate_order_key(key: &str) -> Result<()> {
    validate_order_key_with(key, BASE_62_DIGITS)
}

/// `validateOrderKey(key, digits)`. `digits` must be non-empty and in
/// ascending code-unit order.
pub fn validate_order_key_with(key: &str, digits: &str) -> Result<()> {
    validate_units(&units(key), &units(digits))
}

/// `incrementInteger` (`index.ts:133-162`); `None` past the largest integer.
fn increment_integer(x: &[u16], digits: &[u16]) -> Result<Option<Units>> {
    validate_integer(x)?;
    let head = x[0];
    let mut digs = x[1..].to_vec();
    let mut carry = true;
    for d in digs.iter_mut().rev() {
        let next = index_of(digits, Some(*d)) + 1;
        if next == digits.len() as i64 {
            *d = digits[0];
        } else {
            *d = digit_at(digits, next)[0];
            carry = false;
            break;
        }
    }
    if carry {
        if head == unit('Z') {
            return Ok(Some(vec![unit('a'), digits[0]]));
        }
        if head == unit('z') {
            return Ok(None);
        }
        let h = head + 1;
        if h > unit('a') {
            digs.push(digits[0]);
        } else {
            digs.pop();
        }
        let mut out = vec![h];
        out.extend(digs);
        return Ok(Some(out));
    }
    let mut out = vec![head];
    out.extend(digs);
    Ok(Some(out))
}

/// `decrementInteger` (`index.ts:170-199`); `None` below the smallest
/// integer.
fn decrement_integer(x: &[u16], digits: &[u16]) -> Result<Option<Units>> {
    validate_integer(x)?;
    let head = x[0];
    let last = digits[digits.len() - 1];
    let mut digs = x[1..].to_vec();
    let mut borrow = true;
    for d in digs.iter_mut().rev() {
        let prev = index_of(digits, Some(*d)) - 1;
        if prev == -1 {
            *d = last;
        } else {
            *d = digit_at(digits, prev)[0];
            borrow = false;
            break;
        }
    }
    if borrow {
        if head == unit('a') {
            return Ok(Some(vec![unit('Z'), last]));
        }
        if head == unit('A') {
            return Ok(None);
        }
        let h = head - 1;
        if h < unit('Z') {
            digs.push(last);
        } else {
            digs.pop();
        }
        let mut out = vec![h];
        out.extend(digs);
        return Ok(Some(out));
    }
    let mut out = vec![head];
    out.extend(digs);
    Ok(Some(out))
}

fn concat(a: &[u16], b: Units) -> Units {
    let mut out = a.to_vec();
    out.extend(b);
    out
}

fn key_between(a: Option<&[u16]>, b: Option<&[u16]>, digits: &[u16]) -> Result<Units> {
    if let Some(a) = a {
        validate_units(a, digits)?;
    }
    if let Some(b) = b {
        validate_units(b, digits)?;
    }
    match (a, b) {
        (Some(a), Some(b)) if a >= b => {
            Err(OrderKeyError::new(format!("{} >= {}", text(a), text(b))))
        }
        (None, None) => Ok(vec![unit('a'), digits[0]]),
        (None, Some(b)) => {
            let ib = integer_part(b)?;
            let fb = &b[ib.len()..];
            if ib == smallest_integer(digits).as_slice() {
                return Ok(concat(ib, midpoint(&[], Some(fb), digits)?));
            }
            if ib < b {
                return Ok(ib.to_vec());
            }
            decrement_integer(ib, digits)?
                .ok_or_else(|| OrderKeyError::new("cannot decrement any more".to_owned()))
        }
        (Some(a), None) => {
            let ia = integer_part(a)?;
            let fa = &a[ia.len()..];
            match increment_integer(ia, digits)? {
                Some(i) => Ok(i),
                None => Ok(concat(ia, midpoint(fa, None, digits)?)),
            }
        }
        (Some(a), Some(b)) => {
            let ia = integer_part(a)?;
            let fa = &a[ia.len()..];
            let ib = integer_part(b)?;
            let fb = &b[ib.len()..];
            if ia == ib {
                return Ok(concat(ia, midpoint(fa, Some(fb), digits)?));
            }
            let i = increment_integer(ia, digits)?
                .ok_or_else(|| OrderKeyError::new("cannot increment any more".to_owned()))?;
            if i.as_slice() < b {
                return Ok(i);
            }
            Ok(concat(ia, midpoint(fa, None, digits)?))
        }
    }
}

/// `generateKeyBetween(a, b)` in base 62 (`index.ts:212-268`): a key
/// strictly between `a` and `b`, where `None` is the start (for `a`) or the
/// end (for `b`). `generate_key_between(None, None)` is `"a0"`.
pub fn generate_key_between(a: Option<&str>, b: Option<&str>) -> Result<String> {
    generate_key_between_with(a, b, BASE_62_DIGITS)
}

/// `generateKeyBetween(a, b, digits)`. `digits` must be non-empty and in
/// ascending code-unit order.
pub fn generate_key_between_with(a: Option<&str>, b: Option<&str>, digits: &str) -> Result<String> {
    let (a, b, digits) = (a.map(units), b.map(units), units(digits));
    key_between(a.as_deref(), b.as_deref(), &digits).map(|k| text(&k))
}

fn n_keys_between(
    a: Option<&[u16]>,
    b: Option<&[u16]>,
    n: usize,
    digits: &[u16],
) -> Result<Vec<Units>> {
    if n == 0 {
        return Ok(Vec::new());
    }
    if n == 1 {
        return Ok(vec![key_between(a, b, digits)?]);
    }
    if b.is_none() {
        let mut c = key_between(a, b, digits)?;
        let mut result = Vec::with_capacity(n);
        for _ in 0..n - 1 {
            let next = key_between(Some(&c), b, digits)?;
            result.push(std::mem::replace(&mut c, next));
        }
        result.push(c);
        return Ok(result);
    }
    if a.is_none() {
        let mut c = key_between(a, b, digits)?;
        let mut result = Vec::with_capacity(n);
        for _ in 0..n - 1 {
            let next = key_between(a, Some(&c), digits)?;
            result.push(std::mem::replace(&mut c, next));
        }
        result.push(c);
        result.reverse();
        return Ok(result);
    }
    let mid = n / 2;
    let c = key_between(a, b, digits)?;
    let mut result = n_keys_between(a, Some(&c), mid, digits)?;
    let upper = n_keys_between(Some(&c), b, n - mid - 1, digits)?;
    result.push(c);
    result.extend(upper);
    Ok(result)
}

/// `generateNKeysBetween(a, b, n)` in base 62 (`index.ts:284-322`): `n`
/// distinct keys in sorted order. With both bounds open it returns `a0`,
/// `a1`, ...; with one bound open, consecutive integers; otherwise short keys
/// between the bounds, bisecting.
pub fn generate_n_keys_between(a: Option<&str>, b: Option<&str>, n: usize) -> Result<Vec<String>> {
    generate_n_keys_between_with(a, b, n, BASE_62_DIGITS)
}

/// `generateNKeysBetween(a, b, n, digits)`.
pub fn generate_n_keys_between_with(
    a: Option<&str>,
    b: Option<&str>,
    n: usize,
    digits: &str,
) -> Result<Vec<String>> {
    let (a, b, digits) = (a.map(units), b.map(units), units(digits));
    n_keys_between(a.as_deref(), b.as_deref(), n, &digits)
        .map(|keys| keys.iter().map(|k| text(k)).collect())
}
