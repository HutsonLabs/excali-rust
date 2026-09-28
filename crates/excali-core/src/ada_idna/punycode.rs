//! Punycode as ada 4.0.0 does it (`src/punycode.cpp`): `punycode_to_utf32`
//! and `utf32_to_punycode`, with its overflow checks. Decoding can give
//! values past U+10FFFF or surrogates; ada's mapping then turns them down.

const BASE: i32 = 36;
const TMIN: i32 = 1;
const TMAX: i32 = 26;
const SKEW: i32 = 38;
const DAMP: i32 = 700;
const INITIAL_BIAS: i32 = 72;
const INITIAL_N: u32 = 128;

fn char_to_digit_value(value: u8) -> i32 {
    match value {
        b'a'..=b'z' => i32::from(value - b'a'),
        b'0'..=b'9' => i32::from(value - b'0') + 26,
        _ => -1,
    }
}

fn digit_to_char(digit: i32) -> char {
    if digit < 26 {
        char::from((digit + 97) as u8)
    } else {
        char::from((digit + 22) as u8)
    }
}

fn adapt(mut d: i32, n: i32, first_time: bool) -> i32 {
    if first_time {
        d /= DAMP;
    } else {
        d /= 2;
    }
    d += d / n;
    let mut k = 0;
    while d > ((BASE - TMIN) * TMAX) / 2 {
        d /= BASE - TMIN;
        k += BASE;
    }
    k + (((BASE - TMIN + 1) * d) / (d + SKEW))
}

fn threshold(k: i32, bias: i32) -> i32 {
    if k <= bias {
        TMIN
    } else if k >= bias + TMAX {
        TMAX
    } else {
        k - bias
    }
}

/// `punycode_to_utf32`: the code points of the label whose Punycode (after
/// `xn--`) is `input`, `None` where ada returns false, including a decoded
/// label starting with `xn--`.
pub(super) fn punycode_to_utf32(input: &[u8]) -> Option<Vec<u32>> {
    let mut out: Vec<u32> = Vec::with_capacity(input.len());
    let mut written_out: i32 = 0;
    let mut n = INITIAL_N;
    let mut i: i32 = 0;
    let mut bias = INITIAL_BIAS;
    let mut rest = input;
    if let Some(end_of_ascii) = input.iter().rposition(|&b| b == b'-') {
        for &c in &input[..end_of_ascii] {
            if c >= 0x80 {
                return None;
            }
            out.push(u32::from(c));
            written_out += 1;
        }
        rest = &input[end_of_ascii + 1..];
    }
    while !rest.is_empty() {
        let oldi = i;
        let mut w: i32 = 1;
        let mut k = BASE;
        loop {
            let (&code_point, tail) = rest.split_first()?;
            rest = tail;
            let digit = char_to_digit_value(code_point);
            if digit < 0 {
                return None;
            }
            if digit > (0x7fff_ffff - i) / w {
                return None;
            }
            i += digit * w;
            let t = threshold(k, bias);
            if digit < t {
                break;
            }
            if w > 0x7fff_ffff / (BASE - t) {
                return None;
            }
            w *= BASE - t;
            k += BASE;
        }
        bias = adapt(i - oldi, written_out + 1, oldi == 0);
        if i / (written_out + 1) > (0x7fff_ffff - n) as i32 {
            return None;
        }
        n += (i / (written_out + 1)) as u32;
        i %= written_out + 1;
        if n < 0x80 {
            return None;
        }
        out.insert(i as usize, n);
        written_out += 1;
        i += 1;
    }
    if out.len() >= 4 && out[..4] == [0x78, 0x6E, 0x2D, 0x2D] {
        return None;
    }
    Some(out)
}

/// `utf32_to_punycode`: appends the Punycode of `input` to `out`; false
/// where ada returns false.
pub(super) fn utf32_to_punycode(input: &[u32], out: &mut String) -> bool {
    let mut n = INITIAL_N;
    let mut d: i32 = 0;
    let mut bias = INITIAL_BIAS;
    let mut h: usize = 0;
    for &c in input {
        if c < 0x80 {
            h += 1;
            out.push(char::from(c as u8));
        }
        if c > 0x10_ffff || (0xd800..0xe000).contains(&c) {
            return false;
        }
    }
    let b = h;
    if b > 0 {
        out.push('-');
    }
    while h < input.len() {
        let mut m: u32 = 0x10_ffff;
        for &code_point in input {
            if code_point >= n && code_point < m {
                m = code_point;
            }
        }
        if u64::from(m - n) > (0x7fff_ffff - d as i64) as u64 / (h as u64 + 1) {
            return false;
        }
        d += ((m - n) as usize * (h + 1)) as i32;
        n = m;
        for &c in input {
            if c < n {
                if d == 0x7fff_ffff {
                    return false;
                }
                d += 1;
            }
            if c == n {
                let mut q = d;
                let mut k = BASE;
                loop {
                    let t = threshold(k, bias);
                    if q < t {
                        break;
                    }
                    out.push(digit_to_char(t + ((q - t) % (BASE - t))));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                out.push(digit_to_char(q));
                bias = adapt(d, (h + 1) as i32, h == b);
                d = 0;
                h += 1;
            }
        }
        d += 1;
        n += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(s: &str) -> String {
        let mut out = String::new();
        assert!(utf32_to_punycode(
            &s.chars().map(u32::from).collect::<Vec<_>>(),
            &mut out
        ));
        out
    }

    #[test]
    fn round_trips() {
        assert_eq!(encode("\u{e9}"), "9ca");
        assert_eq!(encode("\u{1F4A9}"), "ls8h");
        assert_eq!(encode("a\u{10D70}"), "a-ho6i");
        assert_eq!(punycode_to_utf32(b"ls8h"), Some(vec![0x1F4A9]));
        assert_eq!(punycode_to_utf32(b"a-ho6i"), Some(vec![0x61, 0x10D70]));
    }

    #[test]
    fn rejects_what_ada_rejects() {
        assert_eq!(punycode_to_utf32(b"zz"), None);
        assert_eq!(punycode_to_utf32(b"99999999999999a"), None);
        assert_eq!(punycode_to_utf32(b"A"), None);
        // decodes to a label starting with xn--
        assert_eq!(punycode_to_utf32(b"xn--a-ecp"), None);
    }
}
